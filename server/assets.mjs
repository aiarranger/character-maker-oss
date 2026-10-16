import { copyFileSync, createReadStream, existsSync, mkdirSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * 取り込み先（aisns など）がキャラメイクの部品を届けるための入口。配布パッケージの中で使う。
 * - /maker-assets/・/review-swing-2026-10-02/・/character-maker/ はパッケージの public/ から届ける
 * - /api/character-maker/assets/<名前> は api-assets/index.json の一覧から届ける（public/maker-assets/ と同じ中身のものは pack 時に省き、そちらを使う）
 */
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const PUBLIC = path.join(ROOT, 'public');
const API = path.join(ROOT, 'api-assets');
export const PUBLIC_DIRS = ['maker-assets', 'review-swing-2026-10-02', 'character-maker'];
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

/** 開発サーバー用。キャラメイクのパスなら届けて true を返す。 */
export function serveMakerAsset(req, res) {
  const url = req.url?.split('?')[0] ?? '';
  let file = null;
  if (url.startsWith(API_PREFIX)) file = apiAssetFile(decodeURIComponent(url.slice(API_PREFIX.length)));
  else {
    const dir = PUBLIC_DIRS.find(d => url.startsWith(`/${d}/`));
    if (!dir) return false;
    const name = decodeURIComponent(url.slice(dir.length + 2));
    file = safeName(name) ? path.join(PUBLIC, dir, name) : null;
  }
  if (!file || !existsSync(file)) {
    res.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
    res.end('キャラメイクの部品がありません。');
    return true;
  }
  res.writeHead(200, { 'Content-Type': typeOf(file), 'Cache-Control': 'no-cache', 'X-Content-Type-Options': 'nosniff' });
  createReadStream(file).pipe(res);
  return true;
}

/** 静的書き出し用。outRoot（例: out/）へ部品をすべて写し、写した数を返す。 */
export function copyMakerAssets(outRoot) {
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
  return count;
}
