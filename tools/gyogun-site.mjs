#!/usr/bin/env node
// 魚群繚乱と Studio を gyogun.yukikau.jp で静的に配るための書き出し（docs/gyogun-spec.md の「公開先」）。pnpm gyogun:site
//   1. Studio の写しを .site-build/ に組む（静的書き出しで使えない旧画面 /character-maker/classic と、それだけが使う
//      app/api/character-maker/assets は写さない）。node_modules・parts・distribution は元を指す
//      → 作業ツリーは変えない。静的書き出しは中間の .next を必ず使うので、起動中の pnpm dev（根の .next）とぶつからないよう写しの中で組む
//   2. 写しの中で GYOGUN_SITE=1 next build（next.config.mjs が output: 'export' になる）→ 根の out/ へ
//   3. 実行時素材（public/ の決まったフォルダ。public/gyogun は pnpm gyogun:build の出来上がり）を out/ へ写し、_redirects（/ → /gyogun/）を書く
//   4. Workers の静的アセットの上限（1 ファイル 25 MiB・Free で 20,000 ファイル）と、要るファイルがそろっているかを確かめる
// node tools/gyogun-site.mjs [--check]  （--check は 4 だけ。out/ を作り直さない）
import { cpSync, existsSync, mkdirSync, readdirSync, renameSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, join, relative } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const BUILD = join(ROOT, '.site-build');
const OUT = join(ROOT, 'out');
/** out/ に入れる public/ のフォルダ（public/muso など古い出来上がりは入れない） */
export const SITE_PUBLIC = ['gyogun', 'character-maker', 'maker-assets', 'maker-stage', 'models', 'portraits', 'review-swing-2026-10-02'];
/** 写しに入れない物（静的書き出しで使えない） */
const SKIP = ['app/character-maker/classic', 'app/api'];
export const MAX_FILE = 25 * 1024 * 1024;
export const MAX_FILES = 20000;
export const REDIRECTS = '/ /gyogun/ 302\n';
/** out/ に必ずある物 */
export const REQUIRED = ['gyogun/index.html', 'gyogun/gyogun_game_bg.wasm', 'gyogun/delivery/channels/stable/v1/index.json', 'character-maker.html', 'maker-stage/character_maker_stage_bg.wasm', 'character-maker/gyogun-title.mp4', '404.html', '_redirects'];

function run(cmd, args, opts) {
  const r = spawnSync(cmd, args, { stdio: 'inherit', ...opts });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')} が失敗しました（${r.status}）`);
}

function walk(dir, base = dir, out = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walk(p, base, out); else out.push({ path: relative(base, p), bytes: statSync(p).size });
  }
  return out;
}

/** out/ を調べる。問題があれば errors に入れる */
export function inspectSite(out = OUT) {
  const errors = [];
  if (!existsSync(out)) return { errors: [`${out} がありません`], files: [] };
  const files = walk(out);
  for (const r of REQUIRED) if (!existsSync(join(out, r))) errors.push(`無い: ${r}`);
  for (const f of files) if (f.bytes > MAX_FILE) errors.push(`25 MiB を超える: ${f.path}（${f.bytes} bytes）`);
  if (files.length > MAX_FILES) errors.push(`ファイルが ${files.length} 個（上限 ${MAX_FILES}）`);
  for (const bad of ['muso', 'character-maker/classic.html', 'character-maker/classic', 'api']) if (existsSync(join(out, bad))) errors.push(`入れない物がある: ${bad}`);
  const total = files.reduce((n, f) => n + f.bytes, 0);
  const largest = files.reduce((a, f) => (f.bytes > a.bytes ? f : a), { path: '', bytes: 0 });
  return { errors, files, total, largest };
}

function build() {
  for (const dir of ['gyogun/index.html', 'maker-stage/character_maker_stage_bg.wasm']) {
    if (!existsSync(join(ROOT, 'public', dir))) throw new Error(`public/${dir} がありません。先に pnpm gyogun:build と pnpm stage:build を実行してください。`);
  }
  // 1. 写し
  rmSync(BUILD, { recursive: true, force: true });
  mkdirSync(BUILD, { recursive: true });
  for (const name of ['app', 'components', 'lib']) {
    cpSync(join(ROOT, name), join(BUILD, name), { recursive: true, filter: src => !SKIP.some(s => src === join(ROOT, s) || src.startsWith(join(ROOT, s) + '/')) && !src.endsWith('.DS_Store') });
  }
  for (const file of ['next.config.mjs', 'tsconfig.json', 'package.json']) cpSync(join(ROOT, file), join(BUILD, file));
  for (const name of ['node_modules', 'parts', 'distribution']) symlinkSync(join(ROOT, name), join(BUILD, name));
  // Studio のコードが取り込む public の物（武器の一覧）。ほかの public は 3 で写す
  cpSync(join(ROOT, 'public/review-swing-2026-10-02/weapons.json'), join(BUILD, 'public/review-swing-2026-10-02/weapons.json'));
  // 2. 書き出し
  run(process.execPath, [join(ROOT, 'node_modules/next/dist/bin/next'), 'build'], { cwd: BUILD, env: { ...process.env, GYOGUN_SITE: '1', NEXT_TELEMETRY_DISABLED: '1' } });
  rmSync(OUT, { recursive: true, force: true });
  renameSync(join(BUILD, 'out'), OUT);
  // 3. 素材と _redirects
  for (const name of SITE_PUBLIC) cpSync(join(ROOT, 'public', name), join(OUT, name), { recursive: true, filter: src => !src.endsWith('.DS_Store') });
  writeFileSync(join(OUT, '_redirects'), REDIRECTS);
  rmSync(BUILD, { recursive: true, force: true });
}

const checkOnly = process.argv.includes('--check');
if (process.argv[1] === fileURLToPath(import.meta.url)) {
  if (!checkOnly) build();
  const r = inspectSite();
  const skipped = readdirSync(join(ROOT, 'public')).filter(n => !SITE_PUBLIC.includes(n) && !n.startsWith('.'));
  console.log(JSON.stringify({ out: relative(ROOT, OUT), files: r.files.length, bytes: r.total, mebibytes: Number((r.total / 1048576).toFixed(1)), largest: r.largest, skippedPublic: skipped, errors: r.errors }, null, 2));
  if (r.errors.length) process.exit(1);
}
