// 確認用スクリプトが使う場所と Playwright の探し方（characters の tools/bench/lib/paths.mjs から必要分だけ移した）。
//   NODE_MODULES  three など（このリポジトリの node_modules。描画ページへ配る）
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { homedir } from 'node:os';

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const NODE_MODULES = join(ROOT, 'node_modules');
/** 確認対象の開発サーバー。既定は `pnpm dev` の固定ポート。別ポートのサーバーは CHARACTER_MAKER_URL で渡す */
export const MAKER_URL = (process.env.CHARACTER_MAKER_URL ?? 'http://127.0.0.1:4284').replace(/\/+$/, '');

/** Playwright を探す。repo の依存には入れない（npm の依存を増やさない）。
 *  順に: 環境変数 BENCH_PLAYWRIGHT（playwright のフォルダ）→ ~/.npm/_npx の中の playwright（版の番号が大きいものから。試作版は後回し） */
export async function importPlaywright() {
  const candidates = [];
  if (process.env.BENCH_PLAYWRIGHT) candidates.push(resolve(process.env.BENCH_PLAYWRIGHT));
  const npx = join(homedir(), '.npm', '_npx');
  if (existsSync(npx)) {
    const found = [];
    for (const h of readdirSync(npx)) {
      const dir = join(npx, h, 'node_modules', 'playwright');
      const pkg = join(dir, 'package.json');
      if (!existsSync(pkg)) continue;
      const version = String(JSON.parse(readFileSync(pkg, 'utf8')).version ?? '0');
      const [core, pre] = version.split('-');
      found.push({ dir, nums: core.split('.').map(Number), pre: pre ? 1 : 0 });
    }
    found.sort((a, b) => a.pre - b.pre || b.nums[0] - a.nums[0] || b.nums[1] - a.nums[1] || b.nums[2] - a.nums[2]);
    candidates.push(...found.map(f => f.dir));
  }
  for (const dir of candidates) {
    const file = join(dir, 'index.mjs');
    if (existsSync(file)) return import(pathToFileURL(file).href);
  }
  throw new Error('Playwright が見つかりません。`npx -y playwright@1 --version` を1回実行して npx のキャッシュに入れるか、BENCH_PLAYWRIGHT に playwright のフォルダを渡してください（描画には Google Chrome を使います）');
}
