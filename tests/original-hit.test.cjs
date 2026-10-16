// Run: node --test tests/original-hit.test.cjs
// のけぞり（hit）と ＞＜ のオリジナル4体の資料（docs/main-character-spec.md）。
const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');
const { spawnSync } = require('node:child_process');
const ts = require('typescript');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true, resolveJsonModule: true },
  fileName: filename,
}).outputText, filename);
const { test } = require('node:test');
const assert = require('node:assert/strict');

const ROOT = path.resolve(__dirname, '..');
const IDS = ['generic-human-a', 'generic-human-b', 'generic-robot', 'generic-cat'];
const hurt = require('../lib/character-maker/hurt.ts');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const read = file => fs.readFileSync(path.join(ROOT, file));

test('each original character has one additive hit clip on its 41 Tripo bones', () => {
  const manifest = JSON.parse(read('parts/assets/original-hit-v1/manifest.json'));
  assert.equal(manifest.sourceAnimation.clip, 'Hit_Chest');
  assert.equal(sha(read(manifest.sourceAnimation.path)), manifest.sourceAnimation.sha256);
  assert.deepEqual(manifest.characters.map(item => item.id), IDS);
  for (const id of IDS) {
    const file = `parts/assets/original-hit-v1/${id}-hit.json`, bytes = read(file);
    const entry = manifest.characters.find(item => item.id === id);
    assert.equal(entry.output.path, file); assert.equal(sha(bytes), entry.output.sha256, `${id}: manifest fingerprint`);
    const clips = JSON.parse(bytes);
    assert.equal(clips.length, 1);
    const [clip] = clips;
    assert.equal(clip.name, hurt.HIT_CLIP);
    assert.ok(Math.abs(clip.duration - hurt.HIT_DURATION) < .01, `${id}: about ${hurt.HIT_DURATION}s`);
    // 剣の動作と同じ骨（41本の回転と Hip の位置）を動かす
    const sword = require(`../distribution/characters/${id}/experiments/character-maker-sword-2026-10-03-v1/motions.json`);
    assert.deepEqual(clip.tracks.map(track => track.name).sort(), sword[0].tracks.map(track => track.name).sort());
    for (const track of clip.tracks) {
      assert.ok(Math.abs(track.times.at(-1) - clip.duration) < 1e-5);
      assert.equal(track.values.length, track.times.length * (track.type === 'quaternion' ? 4 : 3));
      assert.ok(track.values.every(Number.isFinite));
    }
    // 最初は上体が後ろへ反り、最後は前へ戻る（頭の前後の差）
    assert.ok(entry.headOffsetAlongForward.start < entry.headOffsetAlongForward.end - .08, `${id}: starts leaning back and returns`);
  }
});

test('the builder reproduces the saved hit clips without touching the sword clips', () => {
  const swords = IDS.map(id => sha(read(`distribution/characters/${id}/experiments/character-maker-sword-2026-10-03-v1/motions.json`)));
  const run = spawnSync(process.execPath, ['tools/character-maker/build-original-hit-motions.mjs', '--check'], { cwd: ROOT, encoding: 'utf8' });
  assert.equal(run.status, 0, run.stderr);
  assert.deepEqual(IDS.map(id => sha(read(`distribution/characters/${id}/experiments/character-maker-sword-2026-10-03-v1/motions.json`))), swords);
});

test('the preview clips add hit after the unchanged motions', () => {
  const { originalClipsWithSword, originalHitClip } = require('../lib/character-maker/original-weapon.ts');
  for (const id of IDS) {
    const clips = originalClipsWithSword(id, []);
    assert.deepEqual(clips.map(clip => clip.name), ['sword_idle', 'sword_attack', hurt.HIT_CLIP]);
    const hit = originalHitClip(id);
    assert.equal(hit.name, hurt.HIT_CLIP);
    assert.equal(hit.tracks.length, 42);
  }
});

test('each original character has a ＞＜ style; Lux draws it in the display colour', () => {
  const config = require('../lib/character-maker/original-config.ts');
  for (const id of IDS) assert.ok(config.originalHurtFaceStyle(id), id);
  const parts = { head: 'original', ears: 'original', eyes: 'original', mouth: 'original', coatColor: 'source', accentColor: '#b87992', eyeColor: 'source' };
  assert.equal(config.originalHurtFaceStyle('generic-robot', parts).line, '#b87992');
  assert.equal(config.originalHurtFaceStyle('generic-robot', { ...parts, accentColor: 'source' }).line, config.ORIGINAL_HURT_FACE_STYLES['generic-robot'].line);
  assert.equal(config.originalHurtFaceStyle('generic-cat').sampled, true);
  // 保存データ（JSON）の形は変えない: 設定に ＞＜・のけぞりの項目は無い
  const catalog = require('../lib/character-maker/studio-catalog.json');
  const document = config.makeOriginalDocument(catalog.find(item => item.id === 'generic-human-a'));
  assert.ok(!('hurtFace' in document.config) && !('hitRevision' in document.config));
});
