import { copyFileSync, createReadStream, existsSync, mkdirSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ensureStudioModel, studioModelCacheDir, studioModelFile, studioModels } from './studio-models.mjs';

export { ensureStudioModels, studioModels, STUDIO_MODELS_COMMIT } from './studio-models.mjs';

/**
 * 取り込み先（aisns など）がキャラメイクの部品を届けるための入口。配布パッケージの中で使う。
 * - /maker-assets/・/review-swing-2026-10-02/・/character-maker/・/portraits/ はパッケージの public/ から届ける
 * - /models/（Studio のオリジナル4体の表示用モデル）は tgz に入れず、ensureStudioModels(cacheDir) が固定コミットから取ったキャッシュから届ける。
 *   まだ無いものは、頼まれたときに取ってから届ける（server/studio-models.mjs）
 * - /api/character-maker/assets/<名前> は api-assets/index.json の一覧から届ける（public/maker-assets/ と同じ中身のものは pack 時に省き、そちらを使う）
 */
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const PUBLIC = path.join(ROOT, 'public');
const API = path.join(ROOT, 'api-assets');
export const PUBLIC_DIRS = ['maker-assets', 'review-swing-2026-10-02', 'character-maker', 'portraits'];
const MODELS_PREFIX = '/models/';
const API_PREFIX = '/api/character-maker/assets/';
/** pack が書く api-assets/index.json。files は api-assets/ に入れた名前、sameAsMakerAssets は public/maker-assets/ と同じ中身なので省いた名前。 */
const INDEX = JSON.parse(readFileSync(path.join(API, 'index.json'), 'utf8'));

const safeName = name => Boolean(name) && !name.includes('..') && !name.includes('\\');

function typeOf(name) {
  if (name.endsWith('.json')) return 'application/json';
  if (name.endsWith('.jpg')) return 'image/jpeg';
  if (name.endsWith('.png')) return 'image/png';
  return 'model/gltf-binary';
}

/** /api/character-maker/assets/<名前> の実ファイル。無ければ null。 */
export function apiAssetFile(name) {
  if (INDEX.files.includes(name)) return path.join(API, name);
  if (INDEX.sameAsMakerAssets.includes(name)) return path.join(PUBLIC, 'maker-assets', name);
  return null;
}

function send(res, file) {
  res.writeHead(200, { 'Content-Type': typeOf(file), 'Cache-Control': 'no-cache', 'X-Content-Type-Options': 'nosniff' });
  createReadStream(file).pipe(res);
}

function notFound(res) {
  res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
  res.end('キャラメイクの部品がありません。');
}

/** 開発サーバー用。キャラメイクの部品なら届けて true、取り込み先自身のファイルなら false を返す。options.modelCacheDir で /models/ のキャッシュを指定できる（既定は最後に ensureStudioModels へ渡した場所）。 */
export function serveMakerAsset(req, res, options = {}) {
  const url = req.url?.split('?')[0] ?? '';
  let file = null;
  if (url.startsWith(MODELS_PREFIX)) {
    const name = decodeURIComponent(url.slice(MODELS_PREFIX.length));
    const dir = options.modelCacheDir ?? studioModelCacheDir();
    const found = safeName(name) ? studioModelFile(name, dir) : null;
    // 一覧にない /models/ は取り込み先自身のモデル（例: /models/env/*.glb）なので、ここでは扱わない
    if (found === null) return false;
    if (found) send(res, found);
    else ensureStudioModel(name, dir).then(ready => send(res, ready), error => {
      if (!res.headersSent) res.writeHead(502, { 'Content-Type': 'text/plain; charset=utf-8' });
      res.end(`キャラメイクのモデルを用意できませんでした。${error.message}`);
    });
    return true;
  }
  if (url.startsWith(API_PREFIX)) file = apiAssetFile(decodeURIComponent(url.slice(API_PREFIX.length)));
  else {
    const dir = PUBLIC_DIRS.find(d => url.startsWith(`/${d}/`));
    if (!dir) return false;
    const name = decodeURIComponent(url.slice(dir.length + 2));
    file = safeName(name) ? path.join(PUBLIC, dir, name) : null;
    // /portraits/ は一般的な名前なので、パッケージに無いものは取り込み先に任せる
    if (dir === 'portraits' && (!file || !existsSync(file))) return false;
  }
  if (!file || !existsSync(file)) {
    notFound(res);
    return true;
  }
  send(res, file);
  return true;
}

/**
 * 静的書き出し用。outRoot（例: out/）へ部品をすべて写し、写した数を返す。
 * /models/ は先に `await ensureStudioModels(cacheDir)` で用意したものを写す。足りないものは書き出さず警告する（旧 CharacterMaker だけなら不要）。
 */
export function copyMakerAssets(outRoot, options = {}) {
  let count = 0;
  const walk = (from, to) => {
    mkdirSync(to, { recursive: true });
    for (const entry of readdirSync(from, { withFileTypes: true })) {
      if (entry.name.startsWith('.')) continue;
      if (entry.isDirectory()) walk(path.join(from, entry.name), path.join(to, entry.name));
      else { copyFileSync(path.join(from, entry.name), path.join(to, entry.name)); count += 1; }
    }
  };
  for (const dir of PUBLIC_DIRS) if (existsSync(path.join(PUBLIC, dir))) walk(path.join(PUBLIC, dir), path.join(outRoot, dir));
  const apiOut = path.join(outRoot, API_PREFIX.slice(1));
  mkdirSync(apiOut, { recursive: true });
  for (const name of [...INDEX.files, ...INDEX.sameAsMakerAssets]) { copyFileSync(apiAssetFile(name), path.join(apiOut, name)); count += 1; }
  const missing = [];
  const modelsOut = path.join(outRoot, MODELS_PREFIX.slice(1));
  for (const entry of studioModels().files) {
    const name = entry.public.slice(MODELS_PREFIX.length);
    const found = studioModelFile(name, options.modelCacheDir ?? studioModelCacheDir());
    if (!found) { missing.push(name); continue; }
    mkdirSync(modelsOut, { recursive: true });
    copyFileSync(found, path.join(modelsOut, name)); count += 1;
  }
  if (missing.length) console.warn(`[character-maker] Studio のモデル ${missing.length} 件がありません。先に await ensureStudioModels(cacheDir) を実行してください。`);
  return count;
}
