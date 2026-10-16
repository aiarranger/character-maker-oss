#!/usr/bin/env node
/**
 * 取り込み先（aisns）へ渡す版付き tgz を作る。package.json の version が版。
 *
 *   node tools/pack.mjs            # .pack/aiarranger-character-maker-<version>.tgz と SHA-256 を出す
 *
 * 中身: キャラメイク画面のソース（テスト・E2E を除く。Studio 本体の CharacterStudio を含む）、配布モデルの JSON、
 * 実行時素材（public/ の4フォルダ）、/api/character-maker/assets 用の部品（api-assets/）、届けるための server/。
 * Studio のオリジナル4体の表示用モデル（public/models/、約97MB）は入れず、studio-models.json に
 * 公開 OSS リポジトリの固定コミットの URL・SHA-256・バイト数を書く（取得は server/studio-models.mjs の ensureStudioModels）。
 */
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { studioModelManifest } from '../server/studio-models.mjs';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const pkg = JSON.parse(readFileSync(path.join(ROOT, 'package.json'), 'utf8'));
const OUT = path.join(ROOT, '.pack');
const STAGE = path.join(OUT, 'package');
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');

execFileSync(process.execPath, ['tools/character-maker/sync-studio-runtime-assets.mjs', '--check'], { cwd: ROOT, stdio: 'inherit' });
execFileSync(process.execPath, ['tools/import-characters.mjs', '--check'], { cwd: ROOT, stdio: 'inherit' });

rmSync(STAGE, { recursive: true, force: true });
const copy = relative => {
  const dest = path.join(STAGE, relative);
  mkdirSync(path.dirname(dest), { recursive: true });
  copyFileSync(path.join(ROOT, relative), dest);
};
const walk = dir => readdirSync(path.join(ROOT, dir), { withFileTypes: true }).flatMap(entry => {
  if (entry.name.startsWith('.')) return [];
  const relative = path.join(dir, entry.name);
  return entry.isDirectory() ? walk(relative) : [relative];
});

const source = [
  ...walk('components/character-maker'), ...walk('components/rig-actor/generic'), ...walk('lib/character-maker'),
  'lib/viewer-material/generic.mjs', 'lib/viewer-material/generic.d.mts', 'server/assets.mjs', 'server/studio-models.mjs',
  ...walk('distribution').filter(file => file.endsWith('.json')),
  // original-weapon.ts が読む、オリジナル4体ののけぞり
  ...walk('parts/assets/original-hit-v1').filter(file => file.endsWith('.json')),
  ...walk('public/maker-assets'), ...walk('public/review-swing-2026-10-02'), ...walk('public/character-maker'), ...walk('public/portraits'),
].filter(file => !/\.(test|e2e)\.(mjs|cjs|ts)$/.test(file))
  // look-*.jpg は Studio 用に同期されるが、どのコードからも読まれない
  .filter(file => !/^public\/maker-assets\/look-.+\.jpg$/.test(file));
for (const file of source) copy(file);

// /api/character-maker/assets の名前 → parts の実ファイル（aisns の旧 server/character-maker-assets.mjs と同じ対応）
const api = [
  ...['masculine.glb', 'feminine.glb', 'masculine-motions.json', 'feminine-motions.json',
    ...['parted', 'long', 'buns', 'crop', 'pixie'].map(hair => `hair-${hair}.glb`)].map(name => [name, `v1/${name}`]),
  ...['feminine', 'masculine'].flatMap(base => [
    [`fantasy-${base}.glb`, `fantasy-v1/${base}.glb`],
    [`fantasy-${base}-motions.json`, `fantasy-v1/${base}-motions.json`],
    [`fantasy-${base}-mature-skin.png`, `mature-v1/${base}-mature-skin.png`],
    [`comic-${base}-sword-motions.json`, `extended-v1/${base}-sword-motions.json`],
    ...['animeParted', 'sidePony', 'layered'].map(hair => [`fantasy-hair-${base}-${hair}.glb`, `fantasy-v1/hair-${base}-${hair}.glb`]),
  ]),
  ...readdirSync(path.join(ROOT, 'parts/assets/thumbnails-v2')).filter(name => /^thumb-.+\.jpg$/.test(name)).map(name => [name, `thumbnails-v2/${name}`]),
];
const index = { files: [], sameAsMakerAssets: [] };
mkdirSync(path.join(STAGE, 'api-assets'), { recursive: true });
for (const [name, relative] of api) {
  const bytes = readFileSync(path.join(ROOT, 'parts/assets', relative));
  const maker = path.join(ROOT, 'public/maker-assets', name);
  if (existsSync(maker) && sha256(readFileSync(maker)) === sha256(bytes)) { index.sameAsMakerAssets.push(name); continue; }
  writeFileSync(path.join(STAGE, 'api-assets', name), bytes);
  index.files.push(name);
}
writeFileSync(path.join(STAGE, 'api-assets/index.json'), `${JSON.stringify(index, null, 2)}\n`);

// Studio の表示用モデルは tgz に入れず、固定コミットから SHA-256 を確かめて取る一覧だけを入れる。
const studioModels = studioModelManifest(JSON.parse(readFileSync(path.join(ROOT, 'parts/assets/studio-runtime-v1/manifest.json'), 'utf8')));
for (const file of studioModels.files) {
  const local = path.join(ROOT, 'public', file.public);
  if (!existsSync(local) || sha256(readFileSync(local)) !== file.sha256) throw new Error(`public${file.public} が manifest と一致しません。pnpm assets:sync を実行してください。`);
}
writeFileSync(path.join(STAGE, 'studio-models.json'), `${JSON.stringify(studioModels, null, 2)}\n`);

writeFileSync(path.join(STAGE, 'package.json'), `${JSON.stringify({
  name: pkg.name,
  version: pkg.version,
  description: pkg.description,
  type: 'module',
  exports: {
    './CharacterMaker': './components/character-maker/CharacterMaker.tsx',
    './CharacterStudio': './components/character-maker/CharacterStudio.tsx',
    './server': './server/assets.mjs',
    './package.json': './package.json',
  },
  peerDependencies: {
    '@react-three/drei': pkg.dependencies['@react-three/drei'],
    '@react-three/fiber': pkg.dependencies['@react-three/fiber'],
    next: pkg.dependencies.next,
    react: pkg.dependencies.react,
    three: pkg.dependencies.three,
  },
}, null, 2)}\n`);

const tgzName = execFileSync('npm', ['pack', '--pack-destination', OUT], { cwd: STAGE, encoding: 'utf8' }).trim().split('\n').at(-1);
const tgz = path.join(OUT, tgzName);
// GitHub は 50 MiB を超えるファイルに警告を出す（上限は 100 MiB）。取り込み先の git に入れるので超えないようにする。
const LIMIT = 50 * 1024 * 1024;
if (statSync(tgz).size >= LIMIT) throw new Error(`tgz が ${(statSync(tgz).size / 1048576).toFixed(1)} MiB あります。50 MiB 未満にしてください。`);
console.log(JSON.stringify({ tgz: path.relative(ROOT, tgz), version: pkg.version, files: source.length + index.files.length + 3,
  bytes: statSync(tgz).size, mebibytes: Number((statSync(tgz).size / 1048576).toFixed(2)), sha256: sha256(readFileSync(tgz)), apiAssets: index.files.length, apiSameAsMakerAssets: index.sameAsMakerAssets.length,
  studioModels: { commit: studioModels.commit, files: studioModels.files.length, bytes: studioModels.files.reduce((sum, file) => sum + file.bytes, 0) } }, null, 2));
