#!/usr/bin/env node
/**
 * 公開用リポジトリ（aiarranger/character-maker-oss）の中身を、履歴なしのスナップショットで作る。
 *
 *   node tools/export-oss.mjs <空のフォルダ> [--remote=https://github.com/aiarranger/character-maker-oss.git]
 *
 * - HEAD のコミット済みファイルだけを使う（作業中の変更は入らない）
 * - 公開しないもの（EXCLUDE）を除く: 私的な会話URLやローカルパスを含む検証記録、社内連携のメモ
 * - git init → 先に remote と作者（aiarranger の noreply）を設定 → 1コミット。push はしない
 */
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const dest = args.find(arg => !arg.startsWith('--'));
const remote = args.find(arg => arg.startsWith('--remote='))?.slice(9) ?? 'https://github.com/aiarranger/character-maker-oss.git';
const AUTHOR = { name: 'aiarranger', email: '269849995+aiarranger@users.noreply.github.com' };
const EXCLUDE = ['parts/reviews', 'parts/evidence', 'docs/maintainers.md', '.claude'];

if (!dest) throw new Error('出力先のフォルダを指定してください。');
const out = path.resolve(dest);
if (existsSync(out) && readdirSync(out).length) throw new Error(`${out} が空ではありません。`);
mkdirSync(out, { recursive: true });

const run = (command, commandArgs, options = {}) => execFileSync(command, commandArgs, { encoding: 'utf8', ...options });
const head = run('git', ['-C', ROOT, 'rev-parse', '--short', 'HEAD']).trim();
const archive = execFileSync('git', ['-C', ROOT, 'archive', '--format=tar', 'HEAD'], { maxBuffer: 1024 * 1024 * 1024 });
execFileSync('tar', ['-x', '-C', out], { input: archive, maxBuffer: 1024 * 1024 * 1024 });
for (const relative of EXCLUDE) rmSync(path.join(out, relative), { recursive: true, force: true });

const git = gitArgs => run('git', ['-C', out, ...gitArgs]);
git(['init', '-q', '-b', 'main']);
git(['config', 'user.name', AUTHOR.name]);
git(['config', 'user.email', AUTHOR.email]);
git(['remote', 'add', 'origin', remote]);
git(['add', '-A']);
git(['commit', '-q', '-m', `Initial public release of Character Studio\n\nSnapshot of the private development repository at ${head}.\n\nCo-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`]);
const identities = git(['log', '--format=%an <%ae> / %cn <%ce>']).trim();
if (identities !== `${AUTHOR.name} <${AUTHOR.email}> / ${AUTHOR.name} <${AUTHOR.email}>`) throw new Error(`作者が想定と違います: ${identities}`);
const files = git(['ls-files']).trim().split('\n').length;
console.log(JSON.stringify({ out, source: head, files, remote, author: identities, excluded: EXCLUDE }, null, 2));
