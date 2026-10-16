#!/usr/bin/env node
/**
 * characters の配布モデル（キャラメイク用モデル・剣の動作など）を distribution/ へ取り込む。
 * 取り込む一覧と SHA-256 は distribution/manifest.json が正本。
 *
 *   node tools/import-characters.mjs --check                 # 手元の distribution/ が manifest と一致するか確認
 *   node tools/import-characters.mjs --from ../characters    # characters の作業ツリーから一覧のファイルを取り込み、SHA と commit を記録
 */
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const DIST = path.join(ROOT, 'distribution');
const MANIFEST = path.join(DIST, 'manifest.json');
const fromArg = process.argv.find(arg => arg.startsWith('--from='))?.slice(7)
  ?? (process.argv.includes('--from') ? process.argv[process.argv.indexOf('--from') + 1] : undefined);
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

const manifest = JSON.parse(readFileSync(MANIFEST, 'utf8'));
if (manifest.version !== 1 || !Array.isArray(manifest.files)) throw new Error('distribution/manifest.json の形式が正しくありません。');

if (fromArg) {
  const from = path.resolve(fromArg);
  const commit = execFileSync('git', ['-C', from, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim();
  for (const entry of manifest.files) {
    const source = path.join(from, entry.path);
    if (!existsSync(source)) throw new Error(`characters に ${entry.path} がありません。`);
    const dest = path.join(DIST, entry.path);
    mkdirSync(path.dirname(dest), { recursive: true });
    copyFileSync(source, dest);
    const bytes = readFileSync(dest);
    entry.sha256 = sha256(bytes);
    entry.bytes = bytes.byteLength;
  }
  manifest.source = { ...manifest.source, commit };
  writeFileSync(MANIFEST, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`取り込み ${manifest.files.length} 件（characters ${commit.slice(0, 12)}）`);
} else {
  const problems = manifest.files.flatMap(entry => {
    const file = path.join(DIST, entry.path);
    if (!existsSync(file)) return [`ありません: ${entry.path}`];
    return sha256(readFileSync(file)) === entry.sha256 ? [] : [`SHA が違います: ${entry.path}`];
  });
  if (problems.length) { console.error(problems.join('\n')); process.exit(1); }
  console.log(`一致 ${manifest.files.length} 件（characters ${manifest.source.commit.slice(0, 12)}）`);
}
