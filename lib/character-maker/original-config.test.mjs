import assert from 'node:assert/strict';
import test from 'node:test';
import { registerHooks } from 'node:module';

// The app resolves extensionless TS imports. Node 24 strips TS types itself,
// but needs this one test-scoped resolver for the config's runtime dependency.
const configUrl = new URL('./original-config.ts', import.meta.url).href;
const hook = registerHooks({
  resolve(specifier, context, nextResolve) {
    return nextResolve(context.parentURL === configUrl && specifier === './original-species'
      ? new URL('./original-species.ts', import.meta.url).href : specifier, context);
  },
});
const {
  editableOriginalDocument,
  makeOriginalDocument, neutralOriginalFace, resolveOriginalCharacter,
  sameOriginalDocument, validateOriginalDocument, originalHumanHairstyles,
} = await import(configUrl);
hook.deregister();

const SOURCE_PARTS = { head: 'original', ears: 'original', eyes: 'original', mouth: 'original', coatColor: 'source', accentColor: 'source', eyeColor: 'source' };
const roundTrip = document => validateOriginalDocument(JSON.parse(JSON.stringify(document)));

const BASE = {
  name: 'ナミ', nameEn: 'Nami', thumbnail: null, thumbnailIsSheet: false,
  modelSha256: 'a'.repeat(64), modelUrl: '/model.glb', viewer: { neutralMouthClose: 0, headTurnDegrees: 15 },
  expressions: ['blinkLeft', 'blinkRight', 'mouthClose', 'aa', 'ee', 'ou', 'happy'],
  motions: ['idle', 'walk', 'run'], hairMotion: false,
};
const HUMANS = ['generic-human-a', 'generic-human-b'];
/** 人間（ハル・ナミ）は頭と髪のパーツ（modularModel）が必須。猫・ロボットは元のモデルのまま。 */
const fixture = (id, extra = {}) => ({ ...BASE, id, ...(HUMANS.includes(id) ? { modularModel: {
  version: 'original-human-modular-v1', url: '/modular.glb', sha256: 'c'.repeat(64), hairstyles: originalHumanHairstyles(id).map(item => item.id) } } : {}), ...extra });
const character = fixture('generic-human-b');
const cat = fixture('generic-cat');
// 現行の保存は武器付きの schema6。武器より前の schema4/5 を確かめるときは武器を外す。
const withoutWeapon = document => { delete document.config.weaponId; return document; };

function legacyDocument(base = cat) {
  const document = withoutWeapon(makeOriginalDocument(base));
  document.schemaVersion = 4;
  document.assetVersion = 'original-maker-v1';
  document.config.appearance = { version: 'original-appearance-v1', build: 0, hairstyle: 'original', hairColor: 'source' };
  return document;
}

test('a new schema-6 document keeps source identity, neutral face, the default weapon and current human parts', () => {
  const document = makeOriginalDocument(character);
  assert.equal(document.schemaVersion, 6);
  assert.equal(document.config.weaponId, 'cake_sword');
  assert.equal(document.sourceKind, 'original');
  assert.equal(document.assetVersion, 'original-maker-v2');
  assert.equal(document.characterId, character.id);
  assert.equal(document.modelSha256, character.modelSha256);
  assert.deepEqual(document.config.face, { blinkLeft: 0, blinkRight: 0, mouthClose: 0, aa: 0, ee: 0, ou: 0, happy: 0 });
  assert.deepEqual(document.config.appearance, { version: 'original-appearance-v2', build: 0, hairstyle: character.modularModel.hairstyles[0], hairColor: 'source', parts: SOURCE_PARTS });
  assert.deepEqual(makeOriginalDocument(cat).config.appearance, { version: 'original-appearance-v2', build: 0, hairstyle: 'original', hairColor: 'source', parts: SOURCE_PARTS });
  assert.equal(neutralOriginalFace({ ...character, viewer: {} }).mouthClose, 1);
  assert.equal(resolveOriginalCharacter(document, [character]), character);
});

test('JSON round-trip retains independent size, face and playback settings without aliasing input', () => {
  const input = makeOriginalDocument(character, '  ナミの笑顔  ');
  input.config = { ...input.config, size: 1.35, motion: 'run', playing: false, speed: 0.5, autoBlink: false, speaking: true, headTurn: 15,
    face: { ...input.config.face, blinkLeft: 0.4, aa: 0.7, happy: 0.25 } };
  const document = validateOriginalDocument(JSON.parse(JSON.stringify(input)));
  assert.equal(document.name, 'ナミの笑顔');
  assert.ok(sameOriginalDocument(document, input));
  assert.equal(resolveOriginalCharacter(document, [character]), character);
  input.config.face.aa = 0;
  assert.equal(document.config.face.aa, 0.7);
  assert.equal(document.config.size, 1.35);
});

test('a stale source remains exportable but cannot silently load the replacement model', () => {
  const document = makeOriginalDocument(character);
  const updated = { ...character, modelSha256: 'b'.repeat(64) };
  const preserved = validateOriginalDocument(JSON.parse(JSON.stringify(document)));
  assert.deepEqual(preserved, document);
  assert.throws(() => resolveOriginalCharacter(preserved, [updated]), /モデルが更新/);
  assert.throws(() => resolveOriginalCharacter(preserved, []), /元のキャラクター/);
});

test('unsupported controls keep the source neutral state and reject unavailable saved effects', () => {
  const reduced = { ...fixture('generic-robot'), expressions: ['blinkLeft', 'blinkRight', 'aa'], motions: ['idle'], viewer: { neutralMouthClose: 0, headTurnDegrees: 0 } };
  const document = makeOriginalDocument(reduced);
  assert.equal(resolveOriginalCharacter(document, [reduced]), reduced);
  for (const change of [
    { motion: 'run' }, { headTurn: 1 }, { hairMotion: true }, { speaking: true },
    { face: { ...document.config.face, happy: 0.2 } },
  ]) assert.throws(() => resolveOriginalCharacter({ ...document, config: { ...document.config, ...change } }, [reduced]));
});

test('original files reject invalid editing ranges and cannot be confused with human maker documents', () => {
  const document = makeOriginalDocument(character);
  assert.throws(() => validateOriginalDocument({ ...document, sourceKind: 'human', schemaVersion: 3 }), /オリジナル/);
  assert.throws(() => validateOriginalDocument({ ...document, modelSha256: '../model.glb' }), /モデルの情報/);
  for (const change of [{ size: 0.64 }, { size: 1.36 }, { speed: 0 }, { speed: 2.1 }, { playing: 'false' }, { face: { ...document.config.face, aa: NaN } }]) {
    assert.throws(() => validateOriginalDocument({ ...document, config: { ...document.config, ...change } }));
  }
});

test('older schema-4 files restore the original appearance without changing the source or expression', () => {
  const legacy = legacyDocument(cat);
  legacy.name = '前から保存しているモカ';
  delete legacy.config.appearance;
  legacy.config.face.aa = 0.45;
  const document = validateOriginalDocument(JSON.parse(JSON.stringify(legacy)));
  assert.equal(document.schemaVersion, 4);
  assert.equal(document.modelSha256, character.modelSha256);
  assert.equal(document.config.face.aa, 0.45);
  assert.deepEqual(document.config.appearance, { version: 'original-appearance-v1', build: 0, hairstyle: 'original', hairColor: 'source' });
  assert.equal(document.assetVersion, 'original-maker-v1');
  assert.deepEqual(roundTrip(document), document);
  assert.equal('appearance' in legacy.config, false);
});

test('appearance JSON preserves body, hairstyle and color independently of source identity and playback', () => {
  const input = makeOriginalDocument(cat);
  input.config.appearance = { ...input.config.appearance, build: 1, hairstyle: 'short', hairColor: '#68472d' };
  input.config.motion = 'walk';
  const document = validateOriginalDocument(JSON.parse(JSON.stringify(input)));
  assert.deepEqual(document.config.appearance, input.config.appearance);
  assert.equal(document.characterId, cat.id);
  assert.equal(document.modelSha256, cat.modelSha256);
  assert.equal(document.config.motion, 'walk');
  input.config.appearance.build = -1;
  assert.equal(document.config.appearance.build, 1);
  assert.equal(sameOriginalDocument(document, input), false);
  const ponytail = { ...document, config: { ...document.config, appearance: { ...document.config.appearance, build: -1, hairstyle: 'ponytail', hairColor: 'source' } } };
  assert.deepEqual(validateOriginalDocument(ponytail).config.appearance, ponytail.config.appearance);
});

test('appearance rejects unknown versions and malformed body, hairstyle or color values', () => {
  const document = makeOriginalDocument(character);
  for (const change of [{ version: 'future' }, { build: 1.01 }, { build: -1.01 }, { build: NaN }, { hairstyle: 'unknown' }, { hairColor: 'red' }, { hairColor: '#123' }]) {
    assert.throws(() => validateOriginalDocument({ ...document, config: { ...document.config,
      appearance: { ...document.config.appearance, ...change } } }));
  }
});

test('all schema-4 hairstyle IDs survive read and repeated JSON round-trips for all four originals', () => {
  // 人間は引退した髪型IDだけを、決めた現行の髪パーツへ読み替える（2026-10-05 以降の仕様）。猫・ロボットは読み替えない。
  const current = {
    'generic-human-a': { original: 'haru-short-v2', short: 'haru-crop-v2', ponytail: 'haru-short-v2' },
    'generic-human-b': { original: 'nagi-bob-v2', short: 'nagi-pixie-v2', ponytail: 'nagi-pony-v2' },
  };
  for (const id of ['generic-human-a', 'generic-human-b', 'generic-cat', 'generic-robot']) {
    for (const hairstyle of ['original', 'short', 'ponytail']) {
      const input = legacyDocument(fixture(id));
      input.config.appearance = { version: 'original-appearance-v1', build: -.6, hairstyle, hairColor: '#b97991' };
      input.config.motion = 'walk'; input.config.face.blinkRight = .35;
      const document = roundTrip(input);
      if (current[id]) {
        assert.equal(document.schemaVersion, 5, `${id}/${hairstyle}`);
        assert.deepEqual(document.config.appearance, { version: 'original-appearance-v2', parts: SOURCE_PARTS, build: -.6, hairstyle: current[id][hairstyle], hairColor: '#b97991' });
        assert.deepEqual({ ...document.config, appearance: undefined }, { ...input.config, appearance: undefined });
      } else {
        assert.deepEqual(document, input, `${id}/${hairstyle}: load must not upgrade or restyle`);
        assert.equal('parts' in document.config.appearance, false);
      }
      assert.deepEqual(roundTrip(document), document);
    }
  }
});

test('explicit editing upgrades schema-4 while preserving saved hair, source, face and playback', () => {
  for (const hairstyle of ['original', 'short', 'ponytail']) {
    const input = legacyDocument(cat);
    input.config.appearance = { ...input.config.appearance, build: .8, hairstyle, hairColor: '#7a739d' };
    input.config.face.happy = .7; input.config.motion = 'run'; input.config.playing = false;
    const snapshot = structuredClone(input), upgraded = editableOriginalDocument(input);
    assert.deepEqual(input, snapshot, 'editing conversion must not mutate the loaded draft');
    assert.equal(upgraded.schemaVersion, 5); assert.equal(upgraded.assetVersion, 'original-maker-v2');
    assert.deepEqual(upgraded.config.appearance, { ...snapshot.config.appearance, version: 'original-appearance-v2', parts: SOURCE_PARTS });
    assert.deepEqual(upgraded.config.face, snapshot.config.face);
    assert.equal(upgraded.modelSha256, snapshot.modelSha256);
    assert.equal(upgraded.config.motion, 'run'); assert.equal(upgraded.config.playing, false);
    assert.deepEqual(roundTrip(upgraded), upgraded);
    assert.equal(editableOriginalDocument(upgraded), upgraded);
  }
});

test('schema-5 cat and robot parts round-trip without aliasing or changing body, face and hair', () => {
  const variants = [
    ['generic-cat', { head: 'slim', ears: 'rounded', eyes: 'large', mouth: 'small', coatColor: '#CCBB99', accentColor: '#302b30', eyeColor: '#548D97' }],
    ['generic-cat', { head: 'cheeks', ears: 'folded', eyes: 'narrow', mouth: 'wide', coatColor: 'source', accentColor: 'source', eyeColor: 'source' }],
    ['generic-robot', { head: 'rounded', ears: 'original', eyes: 'large', mouth: 'small', coatColor: '#AABBCC', accentColor: '#547a91', eyeColor: 'source' }],
    ['generic-robot', { head: 'wide', ears: 'original', eyes: 'narrow', mouth: 'wide', coatColor: 'source', accentColor: 'source', eyeColor: 'source' }],
  ];
  for (const [id, parts] of variants) {
    const input = makeOriginalDocument(fixture(id));
    input.config.appearance = { ...input.config.appearance, build: -.4, hairstyle: 'short', hairColor: '#B97991', parts: { ...parts } };
    input.config.face.aa = .6;
    const document = roundTrip(input);
    assert.deepEqual(document.config.appearance.parts, Object.fromEntries(Object.entries(parts).map(([key, value]) => [key, value.toLowerCase()])));
    assert.equal(document.config.appearance.build, -.4);
    assert.equal(document.config.appearance.hairstyle, 'short');
    assert.equal(document.config.appearance.hairColor, '#b97991');
    assert.equal(document.config.face.aa, .6);
    assert.deepEqual(roundTrip(document), document);
    input.config.appearance.parts.head = 'original';
    assert.equal(document.config.appearance.parts.head, parts.head);
  }
});

test('schema-5 omitted species parts mean original parts, but the appearance version stays explicit', () => {
  const input = makeOriginalDocument(cat);
  delete input.config.appearance.parts;
  assert.deepEqual(roundTrip(input).config.appearance.parts, SOURCE_PARTS);
  delete input.config.appearance;
  assert.throws(() => roundTrip(input), /バージョン/);
});

test('species validation rejects cross-species shapes, unsupported colors and malformed parts', () => {
  const cases = [
    ['generic-human-a', { head: 'wide' }], ['generic-human-b', { head: 'cheeks' }],
    ['generic-human-a', { ears: 'folded' }], ['generic-human-b', { coatColor: '#aabbcc' }],
    ['generic-cat', { head: 'wide' }], ['generic-robot', { head: 'cheeks' }],
    ['generic-robot', { ears: 'folded' }], ['generic-robot', { eyeColor: '#aabbcc' }],
    ['generic-cat', { eyes: 'unknown' }], ['generic-cat', { mouth: '' }],
    ['generic-cat', { coatColor: '#abc' }], ['generic-cat', { accentColor: 'red' }],
    ['generic-cat', { eyeColor: 123 }],
  ];
  for (const [id, patch] of cases) {
    const input = makeOriginalDocument(fixture(id));
    input.config.appearance.parts = { ...SOURCE_PARTS, ...patch };
    assert.throws(() => roundTrip(input), `${id}/${JSON.stringify(patch)}`);
  }
  for (const parts of [null, [], 'original', {}]) {
    const input = makeOriginalDocument(character); input.config.appearance.parts = parts;
    assert.throws(() => roundTrip(input));
  }
});

test('schema-5 sword motions save and resolve only when the chosen character supplies them', () => {
  const withSword = { ...character, motions: ['idle', 'walk', 'run', 'sword_idle', 'sword_attack'] };
  for (const motion of withSword.motions) {
    const input = makeOriginalDocument(withSword); input.config.motion = motion;
    const document = roundTrip(input);
    assert.equal(document.config.motion, motion);
    assert.equal(resolveOriginalCharacter(document, [withSword]), withSword);
    if (motion.startsWith('sword_')) {
      assert.throws(() => resolveOriginalCharacter(document, [character]), /保存された動作/);
      const old = legacyDocument({ ...cat, motions: withSword.motions }); old.config.motion = motion;
      assert.throws(() => roundTrip(old), /動作/);
      // 剣の動作は武器があるときだけ。武器なしの旧保存を編集に開くと待機へ戻し、武器を選べば剣の動作を保つ。
      assert.equal(roundTrip(editableOriginalDocument(old)).config.motion, 'idle');
      assert.equal(roundTrip(editableOriginalDocument({ ...old, config: { ...old.config, weaponId: 'cake_sword' } })).config.motion, motion);
    }
  }
  const invalid = makeOriginalDocument(withSword); invalid.config.motion = 'attack';
  assert.throws(() => roundTrip(invalid), /動作/);
});

test('schema, asset and appearance versions must form a supported combination', () => {
  const current = makeOriginalDocument(character), old = legacyDocument();
  assert.equal(current.schemaVersion, 6);
  for (const input of [
    { ...current, assetVersion: 'original-maker-v1' },
    { ...old, assetVersion: 'original-maker-v2' },
    { ...current, schemaVersion: 7 },
    { ...current, config: { ...current.config, appearance: old.config.appearance } },
    { ...old, config: { ...old.config, appearance: current.config.appearance } },
  ]) assert.throws(() => roundTrip(input), /バージョン/);
});

test('new human parts use a separate asset while saved legacy hairstyles keep the old asset', async () => {
  const { originalModelAsset } = await import(configUrl);
  const modular = { ...BASE, id: 'generic-human-b', appearanceModel: { version: 'original-appearance-v1', url: '/legacy.glb', sha256: 'b'.repeat(64) },
    modularModel: { version: 'original-human-modular-v1', url: '/modular.glb', sha256: 'c'.repeat(64), hairstyles: ['nagi-bob-v2', 'nagi-pixie-v2'] } };
  const fresh = makeOriginalDocument(modular);
  assert.equal(fresh.config.appearance.hairstyle, 'nagi-bob-v2');
  fresh.config.appearance.parts = { ...SOURCE_PARTS, head: 'rounded', eyes: 'large', mouth: 'small' };
  const saved = roundTrip(fresh);
  assert.equal(resolveOriginalCharacter(saved, [modular]), modular);
  assert.equal(originalModelAsset(modular, saved.config.appearance).url, '/modular.glb');
  // 保存済みの旧髪型も現行の頭と髪のパーツで描く。人間は引退した頭・モデルへ戻らない。
  for (const id of ['original', 'short']) {
    const old = legacyDocument(modular); old.config.appearance.hairstyle = id;
    assert.equal(originalModelAsset(modular, roundTrip(old).config.appearance).url, '/modular.glb');
  }
  const pony = legacyDocument(modular); pony.config.appearance.hairstyle = 'ponytail';
  assert.throws(() => originalModelAsset(modular, roundTrip(pony).config.appearance), /頭と髪/);
  assert.throws(() => resolveOriginalCharacter(saved, [{ ...modular, modularModel: { ...modular.modularModel, hairstyles: ['nagi-pixie-v2'] } }]), /髪パーツ/);
  saved.config.appearance.hairstyle = 'haru-short-v2';
  assert.throws(() => roundTrip(saved), /髪型/);
});
