// Run: node --test tests/yukikau-package.test.cjs
// yukikau.jp 用（.character.zip）の中身が、yukikau.jp の読み込み（@aiarranger/character-runtime の contract.mjs）の規則に合うこと。docs/yukikau-route.md
const fs = require('node:fs');
const { createHash } = require('node:crypto');
const ts = require('typescript');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true, resolveJsonModule: true },
  fileName: filename,
}).outputText, filename);
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { unzipSync } = require('three/examples/jsm/libs/fflate.module.js');
const { buildYukikauPackage, yukikauClips, yukikauHeightMeters, YUKIKAU_IMPORT_URL } = require('../lib/character-maker/yukikau-package.ts');

const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==', 'base64');
function glb(json) {
  let text = Buffer.from(JSON.stringify({ asset: { version: '2.0' }, buffers: [{ byteLength: 4 }], ...json }));
  text = Buffer.concat([text, Buffer.alloc((4 - text.length % 4) % 4, 0x20)]);
  const bin = Buffer.concat([Buffer.from([8, 0, 0, 0]), Buffer.from('BIN\0'), Buffer.alloc(4)]);
  const header = Buffer.alloc(20);
  header.writeUInt32LE(0x46546c67, 0); header.writeUInt32LE(2, 4); header.writeUInt32LE(20 + text.length + bin.length, 8);
  header.writeUInt32LE(text.length, 12); header.writeUInt32LE(0x4e4f534a, 16);
  return new Uint8Array(Buffer.concat([header, text, bin]));
}
const ORIGINAL_CLIPS = ['preset:biped:idle', 'preset:biped:walk', 'preset:biped:run', 'hit', 'face_hurt'].map(name => ({ name }));

test('the package holds exactly character.json, model.glb and thumbnail.png with the reader manifest rules', async () => {
  const model = glb({ animations: ORIGINAL_CLIPS, extensionsRequired: ['KHR_materials_unlit'] });
  const file = await buildYukikauPackage({ name: ' ハル ', model, thumbnail: new Uint8Array(PNG), heightMeters: 1.15, license: 'original' });
  assert.equal(file.name, 'ハル.character.zip');
  const files = unzipSync(new Uint8Array(await file.arrayBuffer()));
  assert.deepEqual(Object.keys(files).sort(), ['character.json', 'model.glb', 'thumbnail.png']);
  const manifest = JSON.parse(Buffer.from(files['character.json']).toString('utf8'));
  // contract.mjs validateManifest: only these keys, schemaVersion 1, id pattern
  assert.deepEqual(Object.keys(manifest), ['schemaVersion', 'id', 'name', 'author', 'modelVersion', 'model', 'thumbnail', 'coordinates', 'motion', 'terms']);
  assert.equal(manifest.schemaVersion, 1);
  assert.equal(manifest.id, `maker-${sha(model).slice(0, 12)}`);
  assert.match(manifest.id, /^[a-z0-9][a-z0-9._-]{0,79}$/);
  assert.equal(manifest.name, 'ハル');
  assert.equal(manifest.author, 'AI Arranger Character Studio');
  assert.deepEqual(manifest.model, { path: 'model.glb', sha256: sha(model), bytes: model.byteLength });
  assert.deepEqual(manifest.thumbnail, { path: 'thumbnail.png', sha256: sha(PNG) });
  assert.deepEqual(manifest.coordinates, { up: '+Y', forward: '+Z', ground: 'bounds-min-y', heightMeters: 1.15 });
  assert.deepEqual(manifest.motion, { controller: 'morph-v1', neutralMouthClose: 0, clips: { idle: 'preset:biped:idle', walk: 'preset:biped:walk', run: 'preset:biped:run' }, expressions: [] });
  assert.deepEqual(Object.keys(manifest.terms), ['version', 'assetLicense', 'aiPerformance', 'reference']);
  assert.match(manifest.terms.assetLicense, /^CC BY 4\.0/);
  assert.equal(manifest.terms.reference, 'https://github.com/aiarranger/character-maker-oss');
  assert.equal(sha(files['model.glb']), sha(model));
  assert.equal(YUKIKAU_IMPORT_URL, 'https://yukikau.jp/play?import=1');
});

test('preset clips use the exact names, and the preset licence is CC0', async () => {
  const model = glb({ animations: ['idle', 'walk', 'run', 'sword_idle', 'sword_attack', 'hit', 'face_hurt'].map(name => ({ name })) });
  const file = await buildYukikauPackage({ name: '', model, thumbnail: new Uint8Array(PNG), heightMeters: 9, license: 'preset' });
  const manifest = JSON.parse(Buffer.from(unzipSync(new Uint8Array(await file.arrayBuffer()))['character.json']).toString('utf8'));
  assert.deepEqual(manifest.motion.clips, { idle: 'idle', walk: 'walk', run: 'run' });
  assert.equal(manifest.name, 'キャラクター');
  assert.equal(manifest.coordinates.heightMeters, 3);
  assert.match(manifest.terms.assetLicense, /^CC0 1\.0/);
});

test('models yukikau.jp cannot read are refused before download', async () => {
  const quantized = glb({ animations: ORIGINAL_CLIPS, extensionsRequired: ['KHR_mesh_quantization'] });
  await assert.rejects(buildYukikauPackage({ name: 'x', model: quantized, thumbnail: new Uint8Array(PNG), heightMeters: 1, license: 'original' }), /KHR_mesh_quantization/);
  // 武器用（構える・振るだけ）は歩く動作がない
  assert.throws(() => yukikauClips({ animations: ['preset:biped:sword_idle', 'preset:biped:sword_attack', 'hit'].map(name => ({ name })) }), /walk/);
  await assert.rejects(buildYukikauPackage({ name: 'x', model: glb({ animations: ORIGINAL_CLIPS }), thumbnail: new Uint8Array([1, 2, 3, 4]), heightMeters: 1, license: 'original' }), /PNG/);
});

test('height is clamped to the reader range 0.2–3 m', () => {
  assert.equal(yukikauHeightMeters(0.01), 0.2);
  assert.equal(yukikauHeightMeters(1.23456), 1.235);
  assert.equal(yukikauHeightMeters(12), 3);
  assert.equal(yukikauHeightMeters(Number.NaN), 1.15);
});
