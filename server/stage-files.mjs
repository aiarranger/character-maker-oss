import { createHash, randomUUID } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * 下見の Rust の Stage（wasm と JS。/maker-stage/）は tgz に入れない（50 MiB の上限）。オリジナル4体のモデルと同じく、
 * 公開 OSS リポジトリの固定コミットの distribution/maker-stage/ から取り、SHA-256 を確かめてキャッシュに置く。docs/rust-stage-plan.md
 * - 配布パッケージでは pack が書く stage-files.json を読む
 * - このリポジトリの中では public/maker-stage/（rust/web/build.sh が作る）をそのまま届ける
 */
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const STAGE_FILES_REPOSITORY = 'aiarranger/character-maker-oss';
/** 公開リポジトリの固定コミット（distribution/maker-stage/ を含む）。公開リポジトリへ push したあとに書く。未定なら null（pack が止まる） */
export const STAGE_FILES_COMMIT = null;
export const STAGE_PREFIX = '/maker-stage/';
export const STAGE_SOURCE = 'distribution/maker-stage';

const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

/** distribution/maker-stage/ の中身から一覧を作る（pack が使う） */
export function stageFileManifest(commit = STAGE_FILES_COMMIT) {
  const dir = path.join(ROOT, STAGE_SOURCE);
  const names = ['character_maker_stage.js', 'character_maker_stage_bg.wasm'];
  const files = names.map(name => {
    const bytes = readFileSync(path.join(dir, name));
    return { public: STAGE_PREFIX + name, sha256: sha256(bytes), bytes: bytes.length,
      url: `https://raw.githubusercontent.com/${STAGE_FILES_REPOSITORY}/${commit}/${STAGE_SOURCE}/${name}` };
  });
  return { version: 1, repository: STAGE_FILES_REPOSITORY, commit, files };
}

let manifest;
/** 届ける一覧（配布パッケージの中だけ。このリポジトリでは null で、public/maker-stage/ を使う） */
export function stageFiles() {
  if (manifest !== undefined) return manifest;
  const packed = path.join(ROOT, 'stage-files.json');
  manifest = existsSync(packed) ? JSON.parse(readFileSync(packed, 'utf8')) : null;
  return manifest;
}

let cacheDir = process.env.CHARACTER_MAKER_STAGE_CACHE || path.join(process.cwd(), 'node_modules/.cache/character-maker-stage');
export function stageCacheDir() { return cacheDir; }

const nameOf = entry => entry.public.slice(STAGE_PREFIX.length);
/** /maker-stage/<名前> の実ファイル。パッケージの public/maker-stage/（このリポジトリ）→ キャッシュの順。一覧に無ければ null、まだ取っていなければ undefined */
export function stageFile(name, dir = cacheDir) {
  const local = path.join(ROOT, 'public/maker-stage', name);
  const list = stageFiles();
  if (!list) return existsSync(local) ? local : null;
  const entry = list.files.find(item => nameOf(item) === name);
  if (!entry) return null;
  const cached = path.join(dir, name);
  return existsSync(cached) && statSync(cached).size === entry.bytes ? cached : undefined;
}

const pending = new Map();
async function download(entry, dir) {
  const response = await fetch(entry.url);
  if (!response.ok) throw new Error(`下見の Stage を取得できませんでした（${response.status}）: ${entry.url}`);
  const bytes = Buffer.from(await response.arrayBuffer());
  if (bytes.length !== entry.bytes || sha256(bytes) !== entry.sha256) throw new Error(`下見の Stage の SHA-256 が一致しません: ${entry.url}`);
  mkdirSync(dir, { recursive: true });
  const dest = path.join(dir, nameOf(entry)), temp = `${dest}.${randomUUID()}.part`;
  try { writeFileSync(temp, bytes); renameSync(temp, dest); } finally { rmSync(temp, { force: true }); }
}

/** 1 つを用意して実ファイルのパスを返す */
export function ensureStageFile(name, dir = cacheDir) {
  const found = stageFile(name, dir);
  if (found) return Promise.resolve(found);
  if (found === null) return Promise.reject(new Error(`一覧にない Stage のファイルです: ${name}`));
  const entry = stageFiles().files.find(item => nameOf(item) === name);
  const key = path.join(dir, name);
  if (!pending.has(key)) pending.set(key, download(entry, dir).finally(() => pending.delete(key)));
  return pending.get(key).then(() => key);
}

/** 全部を用意する（静的書き出しの前・開発サーバーの起動時）。結果は { cacheDir, downloaded, present } */
export async function ensureStageFiles(dir = cacheDir) {
  cacheDir = path.resolve(dir);
  const list = stageFiles();
  if (!list) return { cacheDir, downloaded: 0, present: 0 };
  let downloaded = 0, present = 0;
  for (const entry of list.files) {
    const name = nameOf(entry), cached = path.join(cacheDir, name);
    if (existsSync(cached) && sha256(readFileSync(cached)) === entry.sha256) { present += 1; continue; }
    rmSync(cached, { force: true });
    await ensureStageFile(name, cacheDir);
    downloaded += 1;
  }
  return { cacheDir, downloaded, present };
}
