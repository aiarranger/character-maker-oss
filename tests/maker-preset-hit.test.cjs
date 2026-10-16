// のけぞり（hit）と ＞＜（face_hurt）のクリップの決まり。docs/main-character-spec.md
const fs = require('node:fs');
const path = require('node:path');
const ts = require('typescript');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true }, fileName: filename,
}).outputText, filename);
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { AnimationClip, InterpolateDiscrete, Mesh, BufferGeometry, MeshBasicMaterial, QuaternionKeyframeTrack } = require('three');
const { addWeaponTracks } = require('../lib/character-maker/weapon.ts');
const { presetHurtClips, withHurtMorphTracks } = require('../lib/character-maker/preset-hurt-face.ts');
const { HIT_CLIP, HURT_FACE_CLIP, HURT_FACE_NODE } = require('../lib/character-maker/hurt.ts');
const { makerAssetUrl } = require('../lib/character-maker/asset-url.ts');

const body = () => new AnimationClip(HIT_CLIP, 0.6667, [new QuaternionKeyframeTrack('Head.quaternion', [0, 0.6667], [0, 0, 0, 1, 0, 0, 0, 1])]);
const base = ['idle', 'walk', 'run', 'sword_idle', 'sword_attack'].map(name => new AnimationClip(name, 1, []));

test('hit shows the hurt face during the clip and face_hurt only holds the face', () => {
  const clips = presetHurtClips([...base, body()]);
  assert.deepEqual(clips.map(clip => clip.name), ['idle', 'walk', 'run', 'sword_idle', 'sword_attack', HIT_CLIP, HURT_FACE_CLIP]);
  const hit = clips.find(clip => clip.name === HIT_CLIP), face = clips.find(clip => clip.name === HURT_FACE_CLIP);
  const scale = hit.tracks.find(track => track.name === `${HURT_FACE_NODE}.scale`);
  assert.deepEqual(Array.from(scale.values), [1, 1, 1, 0, 0, 0]);
  assert.equal(scale.getInterpolation(), InterpolateDiscrete);
  assert.ok(hit.tracks.some(track => track.name === 'Head.quaternion'), 'the body motion is kept');
  assert.deepEqual(face.tracks.map(track => track.name), [`${HURT_FACE_NODE}.scale`]);
  assert.ok(Array.from(face.tracks[0].values).every(value => value === 1));
  // 既存のクリップは同じものを返す（作り直さない）。
  base.forEach(clip => assert.equal(clips.find(item => item.name === clip.name), clip));
  assert.throws(() => presetHurtClips(base), /のけぞり/);
});

test('hit shows the weapon only for armed exports; face_hurt never carries a weapon track', () => {
  const clips = presetHurtClips([...base, body()]);
  for (const armed of [true, false]) {
    const result = addWeaponTracks(clips, armed);
    const sword = name => result.find(clip => clip.name === name).tracks.find(track => track.name === 'CharacterMakerSword.scale');
    assert.deepEqual(Array.from(sword(HIT_CLIP).values), Array(6).fill(armed ? 1 : 0));
    assert.equal(sword(HURT_FACE_CLIP), undefined);
    for (const name of ['idle', 'walk', 'run']) assert.deepEqual(Array.from(sword(name).values), Array(6).fill(0));
    for (const name of ['sword_idle', 'sword_attack']) assert.deepEqual(Array.from(sword(name).values), Array(6).fill(armed ? 1 : 0));
  }
});

test('fantasy exports add whole-array morph tracks only for the meshes the hurt face changes', () => {
  const clips = presetHurtClips([...base, body()]);
  const face = new Mesh(new BufferGeometry(), new MeshBasicMaterial()); face.name = 'Facebaked_7';
  const other = new Mesh(new BufferGeometry(), new MeshBasicMaterial()); other.name = 'Bodybaked';
  const result = withHurtMorphTracks(clips, [face, other], [[0.5, 0, 0], [1, 0]], [[0.5, 1, 1], [1, 0]]);
  const hit = result.find(clip => clip.name === HIT_CLIP).tracks.filter(track => track.name.endsWith('.morphTargetInfluences'));
  assert.deepEqual(hit.map(track => track.name), ['Facebaked_7.morphTargetInfluences']);
  assert.deepEqual(Array.from(hit[0].values), [0.5, 1, 1, 0.5, 0, 0], 'appearance weights stay; the face returns at the end');
  const held = result.find(clip => clip.name === HURT_FACE_CLIP).tracks.find(track => track.name === 'Facebaked_7.morphTargetInfluences');
  assert.deepEqual(Array.from(held.values), [0.5, 1, 1, 0.5, 1, 1]);
  for (const name of ['idle', 'sword_attack']) assert.equal(result.find(clip => clip.name === name), clips.find(clip => clip.name === name));
});

test('the hit motion data are new files: one clip named hit for every preset body', () => {
  for (const style of ['fantasy', 'comic']) for (const base of ['feminine', 'masculine']) {
    const file = path.join(__dirname, `../public/maker-assets/${style}-${base}-hit-motions.json`);
    const clips = JSON.parse(fs.readFileSync(file, 'utf8'));
    assert.deepEqual(clips.map(clip => clip.name), [HIT_CLIP]);
    assert.ok(Math.abs(clips[0].duration - 0.6667) < 0.001);
    assert.ok(clips[0].tracks.length >= 50);
    // 作り直したら、ブラウザのキャッシュを避ける版（asset-url.ts）も変える。
    const sha = require('node:crypto').createHash('sha256').update(fs.readFileSync(file)).digest('hex').slice(0, 16);
    assert.ok(makerAssetUrl(`${style}-${base}-hit-motions.json`).endsWith(`?v=${sha}`), `${style}-${base}: cache revision matches the file`);
  }
});
