#!/usr/bin/env node
/** オリジナルの人（ハル・ナミ）の作り直しを、素材から組み立てて登録する。docs/original-remake-spec.md
 * node tools/character-maker/remake-original.mjs --id=generic-human-a [--no-fit]
 *
 * 入力（parts/assets/original-remake-v1）: base/<id>.glb（作り直す前のモデル。骨・動作・顔の部品の元）、
 *   sources/<id>/hair-<髪型>.glb と body.glb（Tripo の出力。頭は緑のマネキン）。
 * 手順: 髪を頭へ合わせて差し替え（remake/fit-hair.py・remake-original-hair.mjs）→ 肌の絵をならす（remake-original-skin.mjs）
 *   → 体を今の体へ重ねて差し替え（remake/fit-body.py・remake-original-body.mjs）→ モデルを登録（studio-runtime-v1 と studio-catalog.json）。
 * Blender（/Applications/Blender.app）と python3 の PIL・numpy を使う。--no-fit は、前に合わせた結果（.snaps/remake）をそのまま使う。
 * 登録のあと、動作の出力を作り直す: build-original-action-motions.mjs・build-original-hit-motions.mjs、最後に pnpm assets:sync。
 */
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import assert from 'node:assert/strict';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const arg = name => process.argv.slice(2).find(v => v.startsWith(`--${name}=`))?.slice(name.length + 3);
const id = arg('id'), fit = !process.argv.includes('--no-fit');
const HAND_BASE = { 'generic-human-a': 'distribution/characters/generic-human-a/experiments/character-maker-sword-hand-2026-10-03-v1/hand.json', 'generic-human-b': 'distribution/characters/generic-human-b/experiments/character-maker-sword-hand-2026-10-03-v1/hand.json' };
assert(HAND_BASE[id], '--id=generic-human-a or generic-human-b');
/** 髪の色（元の髪の材質の defaultHairColor と同じ） */
const HAIR_COLOUR = { 'generic-human-a': '#302b30', 'generic-human-b': '#795142' };
const BLENDER = process.env.BLENDER ?? '/Applications/Blender.app/Contents/MacOS/Blender';
const remake = 'parts/assets/original-remake-v1', sources = path.join(ROOT, remake, 'sources', id), base = path.join(ROOT, remake, 'base', `${id}.glb`), work = path.join(ROOT, '.snaps/remake', id);
fs.mkdirSync(work, { recursive: true });
const run = (command, args) => { const out = execFileSync(command, args, { cwd: ROOT, maxBuffer: 1 << 28, encoding: 'utf8' }); return out; };
const node = (script, args) => process.stdout.write(run('node', [`tools/character-maker/${script}`, ...args]).split('\n').map(line => line.slice(0, 260)).join('\n'));
const blender = (script, args) => { const out = run(BLENDER, ['-b', '-P', `tools/character-maker/remake/${script}`, '--', ...args]); const lines = out.split('\n').filter(line => /^(FIT|HAIR|CAP|TEXTURE|NEWBODY|SKIN|COLLAR|YELLOWGREEN)/.test(line)); console.log(lines.join('\n')); assert(lines.some(line => line.startsWith('FIT')), `${script} failed:\n${out.slice(-2000)}`); };

let current = path.join(work, 'step.glb');
fs.copyFileSync(base, current);
const hairs = fs.readdirSync(sources).filter(name => /^hair-.+\.glb$/.test(name)).map(name => name.slice(5, -4)).sort();
for (const hairstyle of hairs) {
  const fitted = path.join(work, `fit-hair-${hairstyle}`);
  if (fit) blender('fit-hair.py', [base, path.join(sources, `hair-${hairstyle}.glb`), fitted, '0', HAIR_COLOUR[id]]);
  const next = path.join(work, 'step-next.glb');
  node('remake-original-hair.mjs', [`--base=${current}`, `--hairstyle=${hairstyle}`, `--hair=${fitted}.glb`, `--out=${next}`]);
  fs.renameSync(next, current);
}
const skinned = path.join(work, 'skin.glb');
node('remake-original-skin.mjs', [`--base=${current}`, `--out=${skinned}`]);
let output = skinned;
if (fs.existsSync(path.join(sources, 'body.glb'))) {
  const fitted = path.join(work, 'fit-body');
  // 襟の内側の色: ハルは上着の暗い色（自動）、ナミは襟の裏地の茶色
  if (fit) blender('fit-body.py', [skinned, path.join(sources, 'body.glb'), fitted, '30000', ...(id === 'generic-human-b' ? ['#7f553a'] : [])]);
  output = path.join(work, 'out.glb');
  node('remake-original-body.mjs', [`--base=${skinned}`, `--body=${fitted}.glb`, `--out=${output}`, `--hand=${remake}/${id}-hand.json`, `--hand-base=${HAND_BASE[id]}`]);
}

// ---- 登録: モデルを studio-runtime-v1 に置き、台帳（manifest）とカタログの sha256 を書き換える ----
const bytes = fs.readFileSync(output), sha = createHash('sha256').update(bytes).digest('hex');
const catalogPath = path.join(ROOT, 'lib/character-maker/studio-catalog.json'), manifestPath = path.join(ROOT, 'parts/assets/studio-runtime-v1/manifest.json');
const catalogText = fs.readFileSync(catalogPath, 'utf8'), previous = JSON.parse(catalogText).find(item => item.id === id).modularModel.sha256;
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
const models = 'parts/assets/studio-runtime-v1/models';
let entry = manifest.entries.find(item => item.kind === 'model' && item.public === `public/models/${previous}.glb`) ?? manifest.entries.find(item => item.kind === 'model' && item.sha256 === JSON.parse(fs.readFileSync(path.join(ROOT, remake, 'manifest.json'), 'utf8')).characters[id]?.replaced);
assert(entry, `runtime manifest entry for the current ${id} model is missing`);
const replaced = entry.sha256;
if (entry.source.startsWith(models) && entry.sha256 !== sha) fs.rmSync(path.join(ROOT, entry.source), { force: true });
if (replaced !== sha) fs.rmSync(path.join(ROOT, 'public/models', `${replaced}.glb`), { force: true });
fs.writeFileSync(path.join(ROOT, models, `${sha}.glb`), bytes);
Object.assign(entry, { public: `public/models/${sha}.glb`, source: `${models}/${sha}.glb`, sha256: sha, bytes: bytes.length });
fs.writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
fs.writeFileSync(catalogPath, catalogText.split(previous).join(sha));
const recordPath = path.join(ROOT, remake, 'manifest.json'), record = fs.existsSync(recordPath) ? JSON.parse(fs.readFileSync(recordPath, 'utf8')) : { version: 'original-remake-v1', characters: {} };
const fileSha = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
record.characters[id] = { base: { path: `${remake}/base/${id}.glb`, sha256: fileSha(base) }, replaced: sha, model: { path: `${models}/${sha}.glb`, sha256: sha, bytes: bytes.length },
  sources: Object.fromEntries(fs.readdirSync(sources).sort().map(name => [name, fileSha(path.join(sources, name))])), hairstyles: hairs, body: fs.existsSync(path.join(sources, 'body.glb')),
  generator: 'Tripo Studio H3.1 (multi-view: front / left / back, ultra mesh, 4K texture, lighting removed); drawings by ChatGPT image generation' };
fs.writeFileSync(recordPath, `${JSON.stringify(record, null, 2)}\n`);
console.log(JSON.stringify({ id, previous, sha256: sha, bytes: bytes.length, hairstyles: hairs }));
