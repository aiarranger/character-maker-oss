// Run: node --test tests/maker-weapon-persistence.test.cjs tests/original-current-data.test.cjs
const fs = require('node:fs');
const ts = require('typescript');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true },
  fileName: filename,
}).outputText, filename);
const { test } = require('node:test');
const assert = require('node:assert/strict');
const generic = require('../lib/character-maker/config.ts');
const originals = require('../lib/character-maker/original-config.ts');
const records = require('../lib/character-maker/storage.ts');
const originalRecords = require('../lib/character-maker/original-storage.ts');
const { selectedMakerWeaponId, validateMakerWeaponId } = require('../lib/character-maker/weapon-catalog.ts');
const { createEditHistory, commitEdit, undoEdit, redoEdit } = require('../lib/character-maker/edit-history.ts');
const catalog = require('../lib/character-maker/studio-catalog.json');
const nagi = catalog.find(item => item.id === 'generic-human-b');
const robot = catalog.find(item => item.id === 'generic-robot');
const WEAPONS = ['none', 'baguette_sword', 'cake_sword', 'candy_axe', 'daikon_club', 'ebi_lance', 'gyoza_mace', 'ika_spear', 'momo_hammer', 'saba_sword', 'softserve_staff', 'taiyaki_sword', 'tamago_pan'];
const clone = value => structuredClone(value);
const json = value => JSON.parse(JSON.stringify(value));
class MemoryStorage {
  data = new Map();
  get length() { return this.data.size; }
  key(index) { return [...this.data.keys()][index] ?? null; }
  getItem(key) { return this.data.get(key) ?? null; }
  setItem(key, value) { this.data.set(key, value); }
}
const browserStorage = () => ({ sessionStorage: new MemoryStorage(), localStorage: new MemoryStorage() });
function selectOriginal(document, weaponId, motion = document.config.motion) {
  return originals.currentOriginalDocument({ ...document, config: { ...document.config, weaponId, motion } });
}
// Legacy fixtures must omit the new-character weapon default explicitly.
function legacyAppearance(config) {
  const { weaponId, ...appearance } = config;
  return appearance;
}
function unarmedOriginal(document) {
  const result = clone(document);
  result.schemaVersion = 5;
  delete result.config.weaponId;
  result.config.motion = 'idle';
  return result;
}
function legacyOriginal(document) {
  const result = unarmedOriginal(document);
  result.schemaVersion = 4;
  result.assetVersion = 'original-maker-v1';
  result.config.appearance.version = 'original-appearance-v1';
  delete result.config.appearance.parts;
  return result;
}

test('the twelve registered weapons and explicit none round-trip generic make, validation, JSON, saved records and both browser drafts', () => {
  for (const defaults of [generic.DEFAULT_CONFIG, generic.COMIC_DEFAULT_CONFIG]) for (const weaponId of WEAPONS) {
    const document = generic.makeDocument(`選んだ ${weaponId}`, { ...defaults, outfitPalette: 'wine', weaponId });
    assert.equal(document.schemaVersion, 5);
    assert.equal(document.assetVersion, defaults.style === 'comic' ? generic.ASSET_VERSION : generic.FANTASY_ASSET_VERSION);
    assert.equal(document.config.weaponId, weaponId);
    assert.equal(selectedMakerWeaponId(document.config), weaponId);
    assert.deepEqual(generic.validateConfig(document.config), document.config);
    assert.deepEqual(generic.validateDocument(document), document);
    assert.deepEqual(generic.serializeDocument(document), document);
    assert.deepEqual(generic.parseDocument(JSON.stringify(generic.serializeDocument(document))), document);
    const browser = browserStorage();
    const saved = records.saveCharacter(browser.localStorage, document);
    assert.equal(json(JSON.parse(browser.localStorage.getItem(records.STORAGE_PREFIX + saved.id))).config.weaponId, weaponId);
    const loaded = records.loadSavedCharacters(browser.localStorage);
    assert.equal(loaded.unreadable, 0);
    assert.deepEqual(loaded.characters, [saved]);
    const draft = { document, savedId: saved.id, savedUpdatedAt: saved.updatedAt };
    records.saveBrowserDraft(browser, draft);
    assert.deepEqual(records.loadBrowserDraft(browser).draft, draft);
    assert.deepEqual(records.loadDraft(browser.sessionStorage), draft);
    assert.deepEqual(records.loadDraft(browser.localStorage), draft);
    const reloadedTab = { sessionStorage: new MemoryStorage(), localStorage: browser.localStorage };
    assert.deepEqual(records.loadBrowserDraft(reloadedTab).draft, draft);
    assert.equal(records.resolveDraftOrigin(draft, loaded.characters).editing.id, saved.id);
  }
});

test('the twelve registered weapons and explicit none round-trip Original typed make/edit, validation, JSON, saved records and both browser drafts', () => {
  for (const character of [nagi, robot]) for (const weaponId of WEAPONS) {
    const document = selectOriginal(originals.makeOriginalDocument(character), weaponId, 'sword_attack');
    assert.equal(document.schemaVersion, 6);
    assert.equal(document.assetVersion, 'original-maker-v2');
    assert.equal(document.config.weaponId, weaponId);
    assert.equal(document.config.motion, weaponId === 'none' ? 'idle' : 'sword_attack');
    assert.equal(document.modelSha256, character.modelSha256);
    assert.deepEqual(originals.validateOriginalDocument(document), document);
    assert.deepEqual(originals.validateOriginalDocument(json(document)), document);
    assert.equal(originals.editableOriginalDocument(document), document);
    const browser = browserStorage();
    const saved = originalRecords.saveOriginalCharacter(browser.localStorage, document);
    const loaded = originalRecords.loadOriginalCharacters(browser.localStorage);
    assert.equal(loaded.unreadable, 0);
    assert.deepEqual(loaded.characters, [saved]);
    const draft = { document, savedId: saved.id, savedUpdatedAt: saved.updatedAt };
    originalRecords.saveOriginalDraft(browser, draft);
    assert.deepEqual(originalRecords.loadOriginalDraft(browser).draft, draft);
    for (const source of Object.values(browser)) {
      assert.deepEqual(JSON.parse(source.getItem(originalRecords.ORIGINAL_DRAFT_KEY)), draft);
      assert.deepEqual(originalRecords.loadOriginalDraft({ sessionStorage: new MemoryStorage(), localStorage: source }).draft, draft);
    }
    assert.equal(originalRecords.resolveOriginalDraftOrigin(draft, loaded.characters).editing.id, saved.id);
  }
});

test('unknown weapon IDs and non-string selections cannot enter config, JSON, saved records or drafts', () => {
  for (const weaponId of ['sword', 'not-registered', '', null, 1, {}, [], true]) {
    assert.throws(() => validateMakerWeaponId(weaponId));
    assert.throws(() => generic.validateConfig({ ...generic.DEFAULT_CONFIG, weaponId }));
    assert.throws(() => generic.makeDocument('不明', { ...generic.DEFAULT_CONFIG, weaponId }));
    const document = { ...generic.makeDocument('不明', generic.DEFAULT_CONFIG), schemaVersion: 5,
      config: { ...generic.DEFAULT_CONFIG, weaponId } };
    assert.throws(() => generic.validateDocument(document));
    assert.throws(() => generic.parseDocument(JSON.stringify(document)));
    assert.throws(() => records.saveCharacter(new MemoryStorage(), document));
    assert.throws(() => records.saveBrowserDraft(browserStorage(), { document, savedId: null, savedUpdatedAt: null }));
    const original = originals.makeOriginalDocument(nagi);
    original.schemaVersion = 6;
    original.config.weaponId = weaponId;
    assert.throws(() => originals.validateOriginalDocument(original));
    assert.throws(() => originalRecords.saveOriginalCharacter(new MemoryStorage(), original));
    assert.throws(() => originalRecords.saveOriginalDraft(browserStorage(), { document: original, savedId: null, savedUpdatedAt: null }));
  }
});

test('legacy missing weapon defaults to none without materializing weaponId or changing unrelated data', () => {
  assert.equal(validateMakerWeaponId(undefined), 'none');
  assert.equal(selectedMakerWeaponId({}), 'none');
  for (const appearance of [generic.COMIC_DEFAULT_CONFIG, generic.DEFAULT_CONFIG,
    { ...generic.DEFAULT_CONFIG, expression: 'happy', expressionStrength: 0.4 },
    { ...generic.DEFAULT_CONFIG, outfitPalette: 'wine' }]) {
    const document = generic.makeDocument('前の設定', legacyAppearance(appearance));
    assert.ok(document.schemaVersion < 5);
    assert.equal(Object.hasOwn(document.config, 'weaponId'), false);
    assert.deepEqual(generic.parseDocument(JSON.stringify(generic.serializeDocument(document))), document);
    assert.equal(Object.hasOwn(generic.validateConfig({ ...appearance, weaponId: undefined }), 'weaponId'), false);
    assert.equal(generic.currentCharacterDocument(document), document);
  }
  for (const character of [nagi, robot]) {
    const document = unarmedOriginal(originals.makeOriginalDocument(character));
    document.config.motion = 'walk';
    assert.equal(document.schemaVersion, 5);
    assert.equal(Object.hasOwn(document.config, 'weaponId'), false);
    assert.deepEqual(originals.validateOriginalDocument(document), document);
    assert.equal(originals.currentOriginalDocument(document), document);
  }
  const legacy = legacyOriginal(originals.makeOriginalDocument(robot));
  assert.deepEqual(originals.validateOriginalDocument(legacy), legacy);
  assert.equal(Object.hasOwn(legacy.config, 'weaponId'), false);
});

test('serialized older schemas reject weapon fields rather than silently accepting or dropping them', () => {
  const genericLegacy = [generic.makeDocument('旧', legacyAppearance(generic.COMIC_DEFAULT_CONFIG)),
    generic.makeDocument('旧', legacyAppearance(generic.DEFAULT_CONFIG)),
    generic.makeDocument('旧', { ...legacyAppearance(generic.DEFAULT_CONFIG), expression: 'happy' }),
    generic.makeDocument('旧', { ...legacyAppearance(generic.DEFAULT_CONFIG), outfitPalette: 'wine' })];
  assert.deepEqual(genericLegacy.map(document => document.schemaVersion), [1, 2, 3, 4]);
  for (const document of genericLegacy) for (const weaponId of ['none', 'cake_sword', undefined]) {
    const unsupported = { ...document, config: { ...document.config, weaponId } };
    assert.throws(() => generic.validateDocument(unsupported), /武器/);
  }
  const current = unarmedOriginal(originals.makeOriginalDocument(robot));
  for (const document of [legacyOriginal(current), current]) for (const weaponId of ['none', 'cake_sword', undefined]) {
    const unsupported = { ...document, config: { ...document.config, weaponId } };
    assert.throws(() => originals.validateOriginalDocument(unsupported), /武器/);
  }
});

test('current generic snapshots upgrade explicit choices and preserve unfinished names, hairstyle migration, undo and redo', () => {
  const document = generic.makeDocument('前の名前', legacyAppearance(generic.DEFAULT_CONFIG));
  const history = createEditHistory(document);
  const edited = generic.currentCharacterDocument({ ...document, name: '',
    config: { ...document.config, base: 'masculine', hair: 'sidePony', weaponId: 'tamago_pan' } });
  assert.equal(edited.schemaVersion, 5);
  assert.equal(edited.name, '');
  assert.equal(edited.config.hair, 'sideSweep');
  assert.equal(edited.config.weaponId, 'tamago_pan');
  assert.equal(generic.currentCharacterDocument(edited), edited);
  const committed = commitEdit(history, edited, generic.sameDocument);
  assert.equal(selectedMakerWeaponId(undoEdit(committed).present.config), 'none');
  assert.equal(redoEdit(undoEdit(committed)).present.config.weaponId, 'tamago_pan');
  const explicitNone = generic.currentCharacterDocument({ ...document, config: { ...document.config, weaponId: 'none' } });
  assert.equal(explicitNone.schemaVersion, 5);
  assert.equal(explicitNone.config.weaponId, 'none');
  assert.equal(document.schemaVersion, 2);
  assert.equal(Object.hasOwn(document.config, 'weaponId'), false);
});

test('Original schema 6 hair migrations retain schema, weapon, unrelated appearance and source identity', () => {
  for (const [hair, expected] of [['original', 'nagi-bob-v2'], ['short', 'nagi-pixie-v2'],
    ['ponytail', 'nagi-pony-v2'], ['nagi-wavy-lob-v3', 'nagi-long-v2']]) {
    const document = selectOriginal(originals.makeOriginalDocument(nagi), 'gyoza_mace', 'sword_idle');
    document.config.appearance.hairstyle = hair;
    document.config.appearance.hairColor = '#b97991';
    document.config.appearance.parts.head = 'rounded';
    const normalized = originals.validateOriginalDocument(document);
    const expectedDocument = clone(document);
    expectedDocument.config.appearance.hairstyle = expected;
    assert.deepEqual(normalized, expectedDocument);
    assert.equal(normalized.modelSha256, document.modelSha256);
    assert.equal(normalized.schemaVersion, 6);
    assert.equal(originals.editableOriginalDocument(normalized), normalized);
  }
});

test('Original edited legacy documents upgrade to schema 6 with current appearance while strict import rejects unsupported old payloads', () => {
  for (const character of [nagi, robot]) for (const weaponId of ['none', 'ika_spear']) {
    const before = legacyOriginal(originals.makeOriginalDocument(character));
    before.config.appearance.hairstyle = 'original';
    const edited = { ...before, name: '', config: { ...before.config, weaponId } };
    const current = originals.currentOriginalDocument(edited);
    assert.equal(current.schemaVersion, 6);
    assert.equal(current.assetVersion, 'original-maker-v2');
    assert.equal(current.config.appearance.version, 'original-appearance-v2');
    assert.ok(current.config.appearance.parts);
    assert.equal(current.config.weaponId, weaponId);
    assert.equal(current.name, '');
    assert.equal(current.modelSha256, before.modelSha256);
    assert.equal(originals.editableOriginalDocument(current), current);
    assert.equal(originals.editableOriginalDocument(edited).schemaVersion, 6);
    assert.throws(() => originals.validateOriginalDocument({ ...edited, name: '旧' }), /武器/);
    assert.deepEqual(originals.validateOriginalDocument({ ...current, name: '現在' }), { ...current, name: '現在' });
  }
});

test('none and missing weapons remove armed Original poses at validation and all typed history boundaries', () => {
  for (const weaponId of [undefined, 'none']) for (const motion of ['sword_idle', 'sword_attack']) {
    const before = unarmedOriginal(originals.makeOriginalDocument(nagi));
    before.config.motion = motion;
    if (weaponId !== undefined) { before.schemaVersion = 6; before.config.weaponId = weaponId; }
    const loaded = originals.validateOriginalDocument(before);
    assert.equal(loaded.config.motion, 'idle');
    assert.equal(loaded.schemaVersion, weaponId === undefined ? 5 : 6);
    assert.equal(Object.hasOwn(loaded.config, 'weaponId'), weaponId !== undefined);
    const history = originals.currentOriginalHistory({ present: before,
      past: [{ ...clone(before), name: '' }], future: [clone(before)], group: 'motion' });
    for (const document of [history.present, ...history.past, ...history.future]) assert.equal(document.config.motion, 'idle');
    assert.equal(history.past[0].name, '');
    assert.equal(history.group, 'motion');
    assert.equal(undoEdit(history).present.config.motion, 'idle');
    assert.equal(redoEdit(history).present.config.motion, 'idle');
    assert.equal(originals.currentOriginalHistory(history), history);
    assert.equal(before.config.motion, motion);
  }
  for (const motion of ['idle', 'walk', 'run']) {
    const before = unarmedOriginal(originals.makeOriginalDocument(nagi));
    before.config.motion = motion;
    assert.equal(originals.currentOriginalDocument(before), before);
  }
});

test('removing and undoing Original weapon selections never downgrades schema 6 or loses selected IDs', () => {
  const first = selectOriginal(originals.makeOriginalDocument(nagi), 'ebi_lance', 'sword_attack');
  const none = selectOriginal(first, 'none');
  const history = commitEdit(createEditHistory(first), none, originals.sameOriginalDocument);
  assert.equal(history.present.config.motion, 'idle');
  assert.equal(history.present.config.weaponId, 'none');
  assert.equal(history.present.schemaVersion, 6);
  assert.equal(undoEdit(history).present.config.weaponId, 'ebi_lance');
  assert.equal(undoEdit(history).present.config.motion, 'sword_attack');
  assert.equal(redoEdit(undoEdit(history)).present.config.weaponId, 'none');
  const missing = clone(none);
  delete missing.config.weaponId;
  assert.equal(originals.currentOriginalDocument(missing).schemaVersion, 6);
  assert.equal(originals.editableOriginalDocument(missing).schemaVersion, 6);
  assert.equal(Object.hasOwn(originals.validateOriginalDocument(missing).config, 'weaponId'), false);
});
