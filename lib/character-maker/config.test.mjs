import test from 'node:test';
import assert from 'node:assert/strict';
import { COMIC_DEFAULT_CONFIG, COMIC_HAIR_STYLES, DEFAULT_CONFIG, EXPRESSIONS, EXPRESSION_CHOICES, getFaceShapes, getPresets, makeDocument, parseDocument, sameConfig, serializeDocument, validateConfig, validateDocument } from './config.ts';

// 武器（schema5）・服の色（schema4）より前の形式を確かめるテストは、武器なしの初期設定を使う。
const { weaponId: _defaultWeapon, ...LEGACY_DEFAULT } = DEFAULT_CONFIG;
const { weaponId: _comicWeapon, ...LEGACY_COMIC } = COMIC_DEFAULT_CONFIG;

test('名前と全選択をJSONで往復し、読み込んだオブジェクトを共有しない', () => {
  for (const preset of [...getPresets('fantasy'), ...getPresets('comic')]) {
    const document = makeDocument('自分のキャラクター🌱', preset.config);
    const restored = parseDocument(JSON.stringify(serializeDocument(document)));
    assert.deepEqual(restored, document);
    assert.notEqual(restored.config, preset.config);
  }
});

test('fantasy8体（女性4・男性4）とcomic4体が重複なく並び、保存済みの旧おだやかを読み直せる', () => {
  const fantasy = getPresets('fantasy');
  assert.equal(fantasy.length, 8);
  for (const base of ['feminine', 'masculine']) assert.equal(fantasy.filter(preset => preset.config.base === base).length, 4);
  assert.equal(new Set(fantasy.map(preset => preset.id)).size, 8);
  assert.equal(new Set(fantasy.map(preset => JSON.stringify(preset.config))).size, 8);
  for (const preset of fantasy) validateConfig(preset.config);
  assert.deepEqual(getPresets('comic').map(preset => preset.id), ['soft', 'fresh', 'calm', 'bright']);
  for (const preset of getPresets('comic')) assert.equal(preset.config.style, 'comic');
  const savedCalm = makeDocument('保存済みのおだやか', { ...LEGACY_DEFAULT, hairColor: '#3B2D32', eyes: 'slender', mouth: 'smile' });
  assert.deepEqual(parseDocument(JSON.stringify(serializeDocument(savedCalm))), savedCalm);
});

test('別形式・未知バージョン・別素材版を黙って読み替えない', () => {
  const document = makeDocument('キャラクター', DEFAULT_CONFIG);
  for (const update of [{ format: 'something-else' }, { schemaVersion: 4 }, { assetVersion: 'human-maker-v2' }]) {
    assert.throws(() => validateDocument({ ...document, ...update }));
  }
  assert.throws(() => parseDocument('{ broken JSON'));
});

test('不正な数値・選択肢・色・名前を拒否する', () => {
  for (const update of [
    { height: NaN }, { height: Infinity }, { height: 0.84 }, { height: '1' },
    { build: 1.01 }, { build: null }, { base: 'unknown' }, { hair: 'external.glb' },
    { face: 'unknown' }, { eyes: 'unknown' }, { mouth: 'unknown' }, { hairColor: 'red' }, { style: 'unknown' },
  ]) assert.throws(() => validateConfig({ ...DEFAULT_CONFIG, ...update }));
  assert.throws(() => makeDocument('  ', DEFAULT_CONFIG));
});

test('従来の全髪型をschema1のまま読込・書出でき、内部styleはcomicに復元する', () => {
  for (const base of ['feminine', 'masculine']) for (const hair of COMIC_HAIR_STYLES) {
    const { style, expression, expressionStrength, ...config } = { ...LEGACY_COMIC, base, hair };
    const legacy = { format: 'characters.character-maker', schemaVersion: 1, assetVersion: 'human-maker-v1', name: '従来の作品', config };
    const restored = validateDocument(legacy);
    assert.equal(restored.config.style, 'comic');
    assert.deepEqual(serializeDocument(restored), legacy);
  }
});

test('fantasyはschema2で保存し、異なるstyleの髪・素材を黙って混ぜない', () => {
  const fantasy = serializeDocument(makeDocument('新しい作品', LEGACY_DEFAULT));
  assert.equal(fantasy.schemaVersion, 2);
  assert.equal(fantasy.assetVersion, 'fantasy-maker-v1');
  assert.equal(fantasy.config.style, 'fantasy');
  assert.throws(() => validateConfig({ ...DEFAULT_CONFIG, hair: 'parted' }));
  assert.throws(() => validateConfig({ ...COMIC_DEFAULT_CONFIG, hair: 'sidePony' }));
  assert.throws(() => validateDocument({ ...fantasy, config: { ...COMIC_DEFAULT_CONFIG } }));
  assert.throws(() => validateDocument({ ...fantasy, assetVersion: 'human-maker-v1' }));
  assert.throws(() => validateDocument({ ...fantasy, schemaVersion: 1, assetVersion: 'human-maker-v1', config: { ...COMIC_DEFAULT_CONFIG, style: 'fantasy' } }));
});

test('大人びた顔はfantasyだけで使え、目・口の選択と独立してJSONに保持する', () => {
  assert.deepEqual(getFaceShapes('comic'), ['oval', 'round', 'angular']);
  assert.deepEqual(getFaceShapes('fantasy'), ['oval', 'round', 'angular', 'mature']);
  for (const base of ['feminine', 'masculine']) for (const eyes of ['almond', 'round', 'slender']) for (const mouth of ['natural', 'smile', 'full']) {
    const document = makeDocument('大人びた顔', { ...LEGACY_DEFAULT, base, face: 'mature', eyes, mouth });
    const saved = serializeDocument(document);
    assert.equal(saved.schemaVersion, 2);
    assert.deepEqual(parseDocument(JSON.stringify(saved)), document);
  }
  assert.throws(() => validateConfig({ ...COMIC_DEFAULT_CONFIG, face: 'mature' }));
  const legacy = serializeDocument(makeDocument('従来の作品', LEGACY_COMIC));
  assert.throws(() => validateDocument({ ...legacy, config: { ...legacy.config, face: 'mature' } }));
  for (const face of ['oval', 'round', 'angular']) {
    const previous = makeDocument('以前の顔', { ...LEGACY_DEFAULT, face });
    assert.deepEqual(parseDocument(JSON.stringify(serializeDocument(previous))), previous);
  }
  // 開始候補のどれに大人びた顔を使うかは Owner が選ぶ。comic の候補には混ざらない。
  assert.ok(getPresets('fantasy').some(preset => preset.config.face === 'mature'));
  assert.equal(getPresets('comic').some(preset => preset.config.face === 'mature'), false);
});

test('範囲端の設定を保ち、未知の追加フィールドは状態へ持ち込まない', () => {
  const input = JSON.parse(JSON.stringify({ ...DEFAULT_CONFIG, height: 0.85, build: -1, hairColor: '#abcdef', modelUrl: 'https://example.invalid/a.glb', __ignored: 1 }));
  const clean = validateConfig(input);
  assert.equal(clean.height, 0.85); assert.equal(clean.build, -1); assert.equal(clean.hairColor, '#ABCDEF');
  assert.equal('modelUrl' in clean, false); assert.equal('__ignored' in clean, false);
  assert.equal(validateConfig({ ...DEFAULT_CONFIG, height: 1.12, build: 1 }).height, 1.12);
});

test('従来のschema1/2をふつうの表情として読み、再出力で新しい項目を足さない', () => {
  for (const original of [LEGACY_DEFAULT, LEGACY_COMIC]) {
    const stored = serializeDocument(makeDocument('以前の作品', original));
    assert.equal('expression' in stored.config, false);
    assert.equal('expressionStrength' in stored.config, false);
    const restored = parseDocument(JSON.stringify(stored));
    assert.equal(restored.config.expression, 'neutral');
    assert.equal(restored.config.expressionStrength, 1);
    assert.deepEqual(serializeDocument(restored), stored);
    assert.throws(() => validateDocument({ ...stored, config: { ...stored.config, expression: 'happy', expressionStrength: 0.5 } }));
  }
});

test('18種類の表情と強さが独立した顔・目・口とともにJSONを往復する', () => {
  assert.equal(EXPRESSIONS.length, 18);
  assert.deepEqual(EXPRESSION_CHOICES.map(item => item.id), [...EXPRESSIONS]);
  assert.equal(EXPRESSION_CHOICES.filter(item => item.group === 'emotion').length, 6);
  for (const expression of EXPRESSIONS) for (const expressionStrength of [0, 0.5, 1]) {
    const current = makeDocument('表情つきの作品', { ...LEGACY_DEFAULT, face: 'mature', eyes: 'slender', mouth: 'full', expression, expressionStrength });
    const persisted = serializeDocument(current);
    assert.equal(persisted.schemaVersion, expression === 'neutral' ? 2 : 3);
    assert.equal(current.config.expressionStrength, expression === 'neutral' ? 1 : expressionStrength);
    assert.deepEqual(parseDocument(JSON.stringify(persisted)), current);
  }
});

test('表情の誤入力とcomicへの混入を拒否し、schema3では必要項目を省略させない', () => {
  for (const update of [{ expression: 'external' }, { expression: null }, { expressionStrength: NaN }, { expressionStrength: Infinity }, { expressionStrength: -0.1 }, { expressionStrength: 1.1 }, { expressionStrength: '1' }, { expressionStrength: null }]) {
    assert.throws(() => validateConfig({ ...DEFAULT_CONFIG, ...update }));
  }
  assert.throws(() => validateConfig({ ...COMIC_DEFAULT_CONFIG, expression: 'happy' }));
  const current = makeDocument('表情つき', { ...LEGACY_DEFAULT, expression: 'happy', expressionStrength: 0.5 });
  for (const key of ['expression', 'expressionStrength']) {
    const config = { ...current.config };
    delete config[key];
    assert.throws(() => validateDocument({ ...current, config }), /表情の設定が不足/);
  }
  assert.throws(() => validateDocument({ ...current, config: { ...COMIC_DEFAULT_CONFIG } }));
});

test('編集中の表情変更とリセットで出力versionを正規化する', () => {
  const previous = makeDocument('作品', LEGACY_DEFAULT);
  const stale = { ...previous, config: { ...previous.config, expression: 'angry', expressionStrength: 0.5 } };
  const expressed = serializeDocument(stale);
  assert.equal(expressed.schemaVersion, 3);
  assert.equal(expressed.config.expression, 'angry');
  const reset = serializeDocument({ ...expressed, config: { ...expressed.config, expression: 'neutral' } });
  assert.equal(reset.schemaVersion, 2);
  assert.equal('expression' in reset.config, false);
  assert.equal(parseDocument(JSON.stringify(reset)).config.expressionStrength, 1);
});

test('選んだプリセットは、項目の並びや引退した髪型の置き換えに関係なく選択中と判定する', () => {
  for (const style of ['fantasy', 'comic']) for (const preset of getPresets(style)) {
    const chosen = makeDocument(preset.name, preset.config).config;
    assert.ok(sameConfig(chosen, preset.config), `${style}:${preset.id}`);
    for (const other of getPresets(style).filter(item => item.id !== preset.id)) assert.equal(sameConfig(chosen, other.config), false, `${style}:${preset.id}≠${other.id}`);
  }
  assert.equal(sameConfig({ ...DEFAULT_CONFIG, build: 0.5 }, DEFAULT_CONFIG), false);
});
