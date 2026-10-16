import { createHash, randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * オリジナル4体の表示用モデル（public/models/、合計約97MB）は tgz に入れない（50 MiB の上限）。
 * 公開 OSS リポジトリの固定コミットから取り、SHA-256 を確かめてキャッシュに置く。docs/yukikau-route.md
 *
 * - 配布パッケージでは pack が書く studio-models.json を読む
 * - このリポジトリの中では parts/assets/studio-runtime-v1/manifest.json から同じ一覧を作る（public/models/ が同期済みならそれを使う）
 */
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const STUDIO_MODELS_REPOSITORY = 'aiarranger/character-maker-oss';
/** 公開リポジトリの固定コミット（parts/assets/studio-runtime-v1/models/* と distribution/** を含む）。 */
export const STUDIO_MODELS_COMMIT = 'f5ee2e34f1cd1251159266db5e9a87cbc9d5f26c';
const RAW_BASE = `https://raw.githubusercontent.com/${STUDIO_MODELS_REPOSITORY}/${STUDIO_MODELS_COMMIT}/`;
const MODELS_PREFIX = '/models/';

/** studio-runtime-v1 の manifest から、tgz に入れないモデルの一覧を作る（pack とこのリポジトリ内で共通）。 */
export function studioModelManifest(runtimeManifest) {
  const files = runtimeManifest.entries.filter(entry => entry.public.startsWith('public/models/')).map(entry => ({
    public: entry.public.slice('public'.length),
    sha256: entry.sha256,
    bytes: entry.bytes,
    url: RAW_BASE + entry.source,
  }));
  return { version: 1, repository: STUDIO_MODELS_REPOSITORY, commit: STUDIO_MODELS_COMMIT, files };
}

let manifest = null;
/** 届けるモデルの一覧（public は /models/<sha>.glb）。 */
export function studioModels() {
  if (manifest) return manifest;
  const packed = path.join(ROOT, 'studio-models.json');
  manifest = existsSync(packed) ? JSON.parse(readFileSync(packed, 'utf8'))
    : studioModelManifest(JSON.parse(readFileSync(path.join(ROOT, 'parts/assets/studio-runtime-v1/manifest.json'), 'utf8')));
  return manifest;
}

let cacheDir = process.env.CHARACTER_MAKER_MODEL_CACHE || path.join(process.cwd(), 'node_modules/.cache/character-maker-models');
/** serveMakerAsset・copyMakerAssets が使うキャッシュの場所（最後に ensureStudioModels へ渡した場所）。 */
export function studioModelCacheDir() { return cacheDir; }

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const fileName = entry => entry.public.slice(MODELS_PREFIX.length);
const matches = (file, entry) => existsSync(file) && statSync(file).size === entry.bytes;

/** /models/<名前> の実ファイル（パッケージの public/models/ → キャッシュの順）。一覧にない名前は null、まだ取っていなければ undefined。 */
export function studioModelFile(name, dir = cacheDir) {
  const entry = studioModels().files.find(item => fileName(item) === name);
  if (!entry) return null;
  for (const file of [path.join(ROOT, 'public/models', name), path.join(dir, name)]) if (matches(file, entry)) return file;
  return undefined;
}

const pending = new Map();
async function download(entry, dir) {
  const dest = path.join(dir, fileName(entry));
  const response = await fetch(entry.url);
  if (!response.ok) throw new Error(`モデルを取得できませんでした（${response.status}）: ${entry.url}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  if (bytes.length !== entry.bytes || sha256(bytes) !== entry.sha256) throw new Error(`モデルの SHA-256 が一致しません: ${entry.url}`);
  mkdirSync(dir, { recursive: true });
  const temp = `${dest}.${randomUUID()}.part`;
  try { writeFileSync(temp, bytes); renameSync(temp, dest); } finally { rmSync(temp, { force: true }); }
}

/** 1つのモデルを用意して、実ファイルのパスを返す（同じ名前の取得は1回にまとめる）。 */
export function ensureStudioModel(name, dir = cacheDir) {
  const found = studioModelFile(name, dir);
  if (found) return Promise.resolve(found);
  if (found === null) return Promise.reject(new Error(`一覧にないモデルです: ${name}`));
  const entry = studioModels().files.find(item => fileName(item) === name);
  const key = path.join(dir, name);
  if (!pending.has(key)) pending.set(key, download(entry, dir).finally(() => pending.delete(key)));
  return pending.get(key).then(() => key);
}

/**
 * 足りない・中身の違うモデルを固定コミットから取り、SHA-256 を確かめてから置く（途中のファイルは残さない）。
 * 置き場所は以後の serveMakerAsset・copyMakerAssets でも使う。結果は { cacheDir, downloaded, present }。
 */
export async function ensureStudioModels(dir = cacheDir) {
  cacheDir = path.resolve(dir);
  let downloaded = 0, present = 0;
  for (const entry of studioModels().files) {
    const name = fileName(entry);
    const cached = path.join(cacheDir, name);
    if (studioModelFile(name, cacheDir) && (!existsSync(cached) || sha256(readFileSync(cached)) === entry.sha256)) { present += 1; continue; }
    rmSync(cached, { force: true });
    await ensureStudioModel(name, cacheDir);
    downloaded += 1;
  }
  return { cacheDir, downloaded, present };
}
