// Run: node --test tests/original-current-data.test.cjs
// Use the project's TypeScript compiler; no test-runner dependency is needed.
const fs = require('node:fs');
const ts = require('typescript');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true },
  fileName: filename,
}).outputText, filename);
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { Group } = require('three');
const config = require('../lib/character-maker/original-config.ts');
const storage = require('../lib/character-maker/original-storage.ts');
const { undoEdit, redoEdit } = require('../lib/character-maker/edit-history.ts');
const { configureOriginalHair } = require('../lib/character-maker/original-hair.ts');
const catalog = require('../lib/character-maker/studio-catalog.json');
const nagi = catalog.find(item => item.id === 'generic-human-b');
const haru = catalog.find(item => item.id === 'generic-human-a');
const clone = value => structuredClone(value);
const savedId = '01234567-89ab-cdef-0123-456789abcdef';
const timestamp = '2026-10-05T12:00:00.000Z';
class MemoryStorage {
  data = new Map();
  get length() { return this.data.size; }
  key(index) { return [...this.data.keys()][index] ?? null; }
  getItem(key) { return this.data.get(key) ?? null; }
  setItem(key, value) { this.data.set(key, value); }
}
function customized(character, hairstyle) {
  const document = config.makeOriginalDocument(character, '保存した名前');
  // Exercise existing pre-weapon v5 records, independently of new defaults.
  document.schemaVersion = 5;
  delete document.config.weaponId;
  document.config = { ...document.config, size: 1.13, speed: 0.75, motion: 'walk', playing: false,
    autoBlink: false, speaking: true, headTurn: character.id === nagi.id ? 9 : 0,
    face: { ...document.config.face, aa: 0.35, blinkLeft: 0.4 },
    appearance: { ...document.config.appearance, build: -0.3, hairstyle, hairColor: '#b97991' },
  };
  return document;
}
function legacy(document) {
  const { weaponId, ...appearanceConfig } = document.config;
  return { ...document, schemaVersion: 4, assetVersion: 'original-maker-v1', config: { ...appearanceConfig, motion: 'idle',
    appearance: { ...document.config.appearance, version: 'original-appearance-v1', parts: undefined } } };
}
function expectOnlyHairMigration(before, after, hairstyle) {
  const expected = clone(before);
  expected.schemaVersion = 5;
  expected.assetVersion = 'original-maker-v2';
  expected.config.appearance = { ...expected.config.appearance, version: 'original-appearance-v2',
    parts: expected.config.appearance.parts ?? config.makeOriginalDocument(nagi).config.appearance.parts, hairstyle };
  assert.deepEqual(after, expected);
  assert.equal(after.modelSha256, before.modelSha256);
}

test('restored Wavy Midi draft uses current long hair from session and local backup without deleting records', () => {
  const browser = { sessionStorage: new MemoryStorage(), localStorage: new MemoryStorage() };
  const before = customized(nagi, 'nagi-wavy-lob-v3');
  before.config.appearance.parts.head = 'rounded';
  const draft = { document: before, savedId, savedUpdatedAt: timestamp };
  const raw = JSON.stringify(draft);
  for (const source of ['localStorage', 'sessionStorage']) {
    browser[source].setItem(storage.ORIGINAL_DRAFT_KEY, raw);
    const loaded = storage.loadOriginalDraft(browser).draft;
    expectOnlyHairMigration(before, loaded.document, 'nagi-long-v2');
    assert.equal(loaded.savedId, savedId);
    assert.equal(loaded.savedUpdatedAt, timestamp);
    assert.equal(browser[source].getItem(storage.ORIGINAL_DRAFT_KEY), raw);
  }
  storage.saveOriginalDraft(browser, draft);
  for (const source of Object.values(browser)) {
    assert.equal(JSON.parse(source.getItem(storage.ORIGINAL_DRAFT_KEY)).document.config.appearance.hairstyle, 'nagi-long-v2');
  }
});

test('saved selection, JSON import/export, and ordinary save normalize Midi while preserving saved identity', () => {
  const memory = new MemoryStorage();
  const before = customized(nagi, 'nagi-wavy-lob-v3');
  const saved = { ...before, id: savedId, createdAt: timestamp, updatedAt: timestamp };
  memory.setItem(storage.ORIGINAL_STORAGE_PREFIX + savedId, JSON.stringify(saved));
  const loaded = storage.loadOriginalCharacters(memory);
  assert.equal(loaded.unreadable, 0);
  assert.equal(loaded.characters.length, 1);
  const migrated = loaded.characters[0];
  assert.equal(migrated.config.appearance.hairstyle, 'nagi-long-v2');
  assert.equal(migrated.createdAt, timestamp);
  assert.equal(migrated.updatedAt, timestamp);
  const exported = config.validateOriginalDocument(JSON.parse(JSON.stringify(before)));
  expectOnlyHairMigration(before, exported, 'nagi-long-v2');
  assert.ok(!JSON.stringify(exported).includes('nagi-wavy-lob-v3'));
  assert.deepEqual(config.validateOriginalDocument(exported), exported);
  const updated = storage.saveOriginalCharacter(memory, before, migrated);
  assert.equal(updated.id, savedId);
  assert.equal(updated.createdAt, timestamp);
  assert.equal(updated.config.appearance.hairstyle, 'nagi-long-v2');
  assert.equal(memory.length, 1);
});

test('all known legacy human hairstyles upgrade to current modular counterparts in v4 and v5', () => {
  for (const [character, aliases] of [[nagi, { original: 'nagi-bob-v2', short: 'nagi-pixie-v2', ponytail: 'nagi-pony-v2' }],
    [haru, { original: 'haru-short-v2', short: 'haru-crop-v2', ponytail: 'haru-short-v2' }]]) {
    for (const [old, current] of Object.entries(aliases)) for (const isLegacy of [true, false]) {
      const before = isLegacy ? legacy(customized(character, old)) : customized(character, old);
      const migrated = config.validateOriginalDocument(before);
      expectOnlyHairMigration(before, migrated, current);
      assert.equal(config.resolveOriginalCharacter(migrated, catalog), character);
      assert.equal(config.originalModelAsset(character, migrated.config.appearance), character.modularModel);
      assert.equal(config.originalModelAsset(character, before.config.appearance), character.modularModel);
      assert.equal(config.originalModelAsset(character), character.modularModel);
    }
  }
  const missingAppearance = legacy(customized(nagi, 'original'));
  delete missingAppearance.config.appearance;
  assert.equal(config.validateOriginalDocument(missingAppearance).config.appearance.hairstyle, 'nagi-bob-v2');
});

test('session restoration migrates present, undo, redo, and unfinished name edits without changing history scope', () => {
  const old = customized(nagi, 'nagi-wavy-lob-v3');
  const past = { ...legacy(customized(nagi, 'short')), name: '' };
  const future = { ...clone(old), name: 'やり直す名前' };
  const raw = { present: old, past: [past], future: [future], group: 'name:1' };
  const history = config.currentOriginalHistory(raw);
  assert.equal(history.present.config.appearance.hairstyle, 'nagi-long-v2');
  assert.equal(history.past[0].config.appearance.hairstyle, 'nagi-pixie-v2');
  assert.equal(history.past[0].name, '');
  assert.equal(history.future[0].config.appearance.hairstyle, 'nagi-long-v2');
  assert.equal(history.group, raw.group);
  assert.equal(undoEdit(history).present.config.appearance.hairstyle, 'nagi-pixie-v2');
  assert.equal(redoEdit(history).present.config.appearance.hairstyle, 'nagi-long-v2');
  assert.equal(config.currentOriginalHistory(history), history);
  assert.equal(config.currentOriginalDocument(history.present), history.present);
  assert.equal(raw.present.config.appearance.hairstyle, 'nagi-wavy-lob-v3');
});

test('preview/export boundaries cannot select a retired mesh or fall back to old human models', () => {
  const appearance = customized(nagi, 'nagi-wavy-lob-v3').config.appearance;
  assert.equal(config.originalModelAsset(nagi, appearance), nagi.modularModel);
  assert.throws(() => config.originalModelAsset({ ...nagi, modularModel: undefined }, appearance));
  assert.throws(() => config.originalModelAsset({ ...haru, modularModel: undefined }, { ...appearance, hairstyle: 'original' }));
  assert.throws(() => config.originalModelAsset(nagi, { ...appearance, hairstyle: 'unrecognized-hair' }));
  assert.throws(() => config.originalModelAsset(haru, { ...appearance, hairstyle: 'nagi-long-v2' }));
  const model = new Group();
  for (const hairstyle of ['nagi-wavy-lob-v3', 'nagi-long-v2', 'nagi-bob-v2']) {
    const hair = new Group(); hair.name = hairstyle; hair.userData = { originalModularPart: 'hair', hairstyleId: hairstyle }; model.add(hair);
  }
  configureOriginalHair(model, nagi.id, appearance);
  assert.deepEqual(model.children.map(child => child.name), ['nagi-long-v2']);
  assert.ok(!config.originalHumanHairstyles(nagi.id).some(item => item.id === 'nagi-wavy-lob-v3'));
});

test('unknown identities, foreign hashes/hair, and unrelated characters keep their safety boundaries', () => {
  const before = customized(nagi, 'nagi-wavy-lob-v3');
  const wrongHash = config.validateOriginalDocument({ ...before, modelSha256: 'a'.repeat(64) });
  assert.equal(wrongHash.modelSha256, 'a'.repeat(64));
  assert.throws(() => config.resolveOriginalCharacter(wrongHash, catalog), /モデルが更新/);
  const unknown = customized(nagi, 'original'); unknown.characterId = 'unknown-character';
  const checked = config.validateOriginalDocument(unknown);
  assert.equal(checked.characterId, 'unknown-character');
  assert.equal(checked.config.appearance.hairstyle, 'original');
  assert.throws(() => config.resolveOriginalCharacter(checked, catalog), /キャラクターを読み込めません/);
  assert.throws(() => config.validateOriginalDocument({ ...before, characterId: haru.id }), /髪型/);
  assert.throws(() => config.validateOriginalDocument({ ...before, characterId: 'unknown-character' }), /髪型/);
  for (const character of catalog.filter(item => ![haru.id, nagi.id].includes(item.id))) {
    const current = config.makeOriginalDocument(character);
    assert.deepEqual(config.validateOriginalDocument(current), current);
    assert.equal(config.currentOriginalDocument(current), current);
    const old = legacy(current);
    delete old.config.appearance.parts;
    assert.deepEqual(config.validateOriginalDocument(old), old);
    assert.equal(config.originalModelAsset(character, old.config.appearance).url, character.appearanceModel?.url ?? character.modelUrl);
  }
});

test('retired masculine fantasy pony normalizes on config, JSON, saved and draft load without relabeling names', () => {
  const fantasy = require('../lib/character-maker/config.ts');
  const records = require('../lib/character-maker/storage.ts');
  const before = { format: fantasy.DOCUMENT_FORMAT, schemaVersion: 4, assetVersion: fantasy.FANTASY_ASSET_VERSION,
    name: 'シオンという自分の名前', config: { ...fantasy.DEFAULT_CONFIG, base: 'masculine', hair: 'sidePony',
      height: 1.07, build: -0.75, expression: 'happy', expressionStrength: 0.45, outfitPalette: 'wine' } };
  delete before.config.weaponId;
  const expected = { ...before, config: { ...before.config, hair: 'sideSweep' } };
  assert.deepEqual(fantasy.validateConfig(before.config), expected.config);
  assert.deepEqual(fantasy.validateDocument(before), expected);
  assert.deepEqual(fantasy.parseDocument(JSON.stringify(before)), expected);
  assert.deepEqual(fantasy.serializeDocument(before), expected);
  const memory = new MemoryStorage();
  const saved = { ...before, id: savedId, createdAt: timestamp, updatedAt: timestamp };
  memory.setItem(records.STORAGE_PREFIX + savedId, JSON.stringify(saved));
  memory.setItem(records.DRAFT_KEY, JSON.stringify({ document: before, savedId, savedUpdatedAt: timestamp }));
  const loaded = records.loadSavedCharacters(memory);
  assert.equal(loaded.unreadable, 0);
  assert.deepEqual(loaded.characters[0], { ...saved, config: expected.config });
  assert.deepEqual(records.loadDraft(memory).document, expected);
  assert.equal(JSON.parse(memory.getItem(records.STORAGE_PREFIX + savedId)).config.hair, 'sidePony');
  assert.equal(memory.length, 2);
});

test('fantasy session/history migration keeps unfinished names, female ponytails and comic settings', () => {
  const fantasy = require('../lib/character-maker/config.ts');
  const current = fantasy.makeDocument('元の名前', { ...fantasy.DEFAULT_CONFIG, base: 'masculine', hair: 'sideSweep' });
  const old = { ...current, name: '', config: { ...current.config, hair: 'sidePony' } };
  const normalized = fantasy.currentCharacterDocument(old);
  assert.equal(normalized.name, '');
  assert.equal(normalized.config.hair, 'sideSweep');
  assert.deepEqual([old, current].map(fantasy.currentCharacterDocument), [normalized, current]);
  assert.equal(fantasy.currentCharacterDocument(normalized), normalized);
  assert.equal(old.config.hair, 'sidePony');
  const feminine = fantasy.makeDocument('女性のサイドポニー', fantasy.DEFAULT_CONFIG);
  assert.equal(feminine.config.hair, 'sidePony');
  assert.equal(fantasy.currentCharacterDocument(feminine), feminine);
  const comic = fantasy.makeDocument('コミック', fantasy.COMIC_DEFAULT_CONFIG);
  assert.equal(fantasy.currentCharacterDocument(comic), comic);
  assert.deepEqual(fantasy.parseDocument(JSON.stringify(fantasy.serializeDocument(comic))), comic);
});
