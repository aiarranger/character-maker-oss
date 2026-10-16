// Run: node --test tests/maker-cake-default.test.cjs
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true }, fileName: filename,
}).outputText, filename);
const { test } = require('node:test');
const assert = require('node:assert/strict');
const generic = require('../lib/character-maker/config.ts');
const original = require('../lib/character-maker/original-config.ts');
const genericStorage = require('../lib/character-maker/storage.ts');
const originalStorage = require('../lib/character-maker/original-storage.ts');
const { selectedMakerWeaponId } = require('../lib/character-maker/weapon-catalog.ts');
const { createEditHistory, commitEdit, undoEdit, redoEdit } = require('../lib/character-maker/edit-history.ts');
const catalog = require('../lib/character-maker/studio-catalog.json');
const activePresets = ['fantasy', 'comic'].flatMap(style => generic.getPresets(style)
  .filter(preset => style !== 'fantasy' || generic.ACTIVE_FANTASY_PRESET_IDS.includes(preset.id))
  .map(preset => ({ ...preset, selectionKey: `${style}:${preset.id}` })));
class Store {
  data = new Map(); get length() { return this.data.size; }
  key(index) { return [...this.data.keys()][index] ?? null; }
  getItem(key) { return this.data.get(key) ?? null; }
  setItem(key, value) { this.data.set(key, value); }
}
const browser = () => ({ sessionStorage: new Store(), localStorage: new Store() });
const draft = document => ({ document, savedId: null, savedUpdatedAt: null });

// Execute the actual UI initializer/reset functions in isolation, without WebGL or a browser.
function sourceNode(filename, predicate) {
  const source = ts.createSourceFile(filename, fs.readFileSync(path.join(__dirname, '../components/character-maker', filename), 'utf8'), ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
  let found;
  function visit(node) { if (!found && predicate(node)) found = node; else ts.forEachChild(node, visit); }
  visit(source);
  assert.ok(found, `Missing UI node in ${filename}`);
  return found.getText(source);
}
function run(source, context) {
  return vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022 } }).outputText, context);
}
const motionInitializer = sourceNode('MakerPresetEditor.tsx', node => ts.isVariableDeclaration(node)
  && ts.isArrayBindingPattern(node.name) && node.name.elements[0]?.name?.getText() === 'motion');
function initialMotion(document, session) {
  return run(`const ${motionInitializer}; motion;`, { document, session, selectedMakerWeaponId, useState: value => [value] });
}
const presetReset = sourceNode('MakerPresetEditor.tsx', node => ts.isFunctionDeclaration(node) && node.name?.text === 'resetAppearance');
const originalReset = sourceNode('OriginalCharacterMaker.tsx', node => ts.isFunctionDeclaration(node) && node.name?.text === 'resetAppearance');
const noop = () => {};

test('all 12 visible fresh characters and both general defaults explicitly start with cake and ready pose', () => {
  assert.deepEqual(catalog.map(item => item.id), ['generic-human-b', 'generic-human-a', 'generic-robot', 'generic-cat']);
  assert.deepEqual(activePresets.map(item => item.selectionKey), ['fantasy:soft', 'fantasy:fresh', 'fantasy:bright', 'fantasy:bold', 'comic:soft', 'comic:fresh', 'comic:calm', 'comic:bright']);
  assert.equal(catalog.length + activePresets.length, 12);
  for (const character of catalog) {
    const document = original.makeOriginalDocument(character);
    assert.equal(document.config.weaponId, 'cake_sword', character.name);
    assert.equal(document.config.motion, 'sword_idle', character.name);
    assert.equal(document.schemaVersion, 6);
    assert.equal(original.resolveOriginalCharacter(document, catalog), character);
    assert.deepEqual(original.validateOriginalDocument(JSON.parse(JSON.stringify(document))), document);
  }
  for (const preset of activePresets) {
    const document = generic.makeDocument(preset.name, preset.config);
    assert.equal(document.config.weaponId, 'cake_sword', preset.selectionKey);
    assert.equal(initialMotion(document), 'sword_idle', preset.selectionKey);
    assert.equal(document.schemaVersion, 5);
    assert.deepEqual(generic.parseDocument(JSON.stringify(generic.serializeDocument(document))), document);
  }
  for (const config of [generic.DEFAULT_CONFIG, generic.COMIC_DEFAULT_CONFIG]) assert.equal(config.weaponId, 'cake_sword');
});

test('each preset reset returns to its own cake default and ready pose, even when cake was already selected', () => {
  for (const preset of activePresets) for (const weaponId of ['none', 'cake_sword', 'saba_sword']) {
    const initial = generic.makeDocument(preset.name, preset.config);
    const before = generic.makeDocument('編集中の名前', { ...initial.config, weaponId, height: 1.02 });
    let document = before, motion = 'walk';
    run(`${presetReset}; resetAppearance();`, {
      initial, selectedMakerWeaponId, chosenWeaponMotion: { current: null }, setPendingWeaponMotion: noop,
      updateDocument: update => { document = update(document); },
      withConfig: (current, config) => generic.makeDocument(current.name, structuredClone(config)),
      setView: noop, setFraming: noop, setMotion: value => { motion = value; }, setPlaying: noop, setSpeed: noop,
      setCameraRevision: noop, setSceneRevision: noop,
    });
    assert.deepEqual(document.config, initial.config);
    assert.equal(document.name, before.name);
    assert.equal(motion, 'sword_idle');
    const history = commitEdit(createEditHistory(before), document, generic.sameDocument);
    assert.equal(undoEdit(history).present.config.weaponId, weaponId);
    assert.equal(redoEdit(undoEdit(history)).present.config.weaponId, 'cake_sword');
  }
});

test('each Original reset returns to cake and ready pose while retaining its identity and undo selection', () => {
  for (const character of catalog) for (const weaponId of ['none', 'cake_sword', 'tamago_pan']) {
    const defaults = original.makeOriginalDocument(character);
    const before = original.currentOriginalDocument({ ...defaults, name: '編集中の名前', config: { ...defaults.config, weaponId, motion: 'walk', size: 1.1 } });
    let document = before;
    run(`${originalReset}; resetAppearance();`, {
      character, makeOriginalDocument: original.makeOriginalDocument, editableOriginalDocument: original.editableOriginalDocument,
      setPendingWeaponMotion: noop, updateDocument: update => { document = original.currentOriginalDocument(update(document)); },
      setSeek: noop, setView: noop, setFraming: noop, setCameraCustom: noop, setCameraRevision: noop,
    });
    assert.deepEqual(document.config, defaults.config);
    assert.equal(document.name, before.name);
    assert.equal(document.characterId, before.characterId);
    assert.equal(document.modelSha256, before.modelSha256);
    const history = commitEdit(createEditHistory(before), document, original.sameOriginalDocument);
    assert.equal(undoEdit(history).present.config.weaponId, weaponId);
    assert.equal(redoEdit(undoEdit(history)).present.config.motion, 'sword_idle');
  }
});

test('existing scoped drafts, imports and session choices remain untouched for none, another weapon and missing fields', () => {
  for (const weaponId of ['none', 'baguette_sword', undefined]) {
    for (const preset of activePresets) {
      const config = { ...preset.config, weaponId };
      if (weaponId === undefined) delete config.weaponId;
      const document = generic.makeDocument('保存した名前', config);
      const b = browser(), entry = { ...draft(document), selectionKey: preset.selectionKey };
      genericStorage.saveBrowserDraft(b, entry);
      const snapshot = JSON.stringify([...b.sessionStorage.data, ...b.localStorage.data]);
      assert.deepEqual(genericStorage.loadBrowserDraft(b, preset.selectionKey).draft, entry);
      assert.deepEqual(generic.parseDocument(JSON.stringify(generic.serializeDocument(document))), document);
      assert.equal(JSON.stringify([...b.sessionStorage.data, ...b.localStorage.data]), snapshot);
      assert.equal(selectedMakerWeaponId(document.config), weaponId ?? 'none');
      assert.equal(initialMotion(document), weaponId === 'baguette_sword' ? 'sword_idle' : 'idle');
      assert.equal(initialMotion(document, { preview: { motion: 'walk' } }), 'walk');
    }
    for (const character of catalog) {
      const document = original.makeOriginalDocument(character);
      document.config.weaponId = weaponId;
      document.config.motion = 'walk';
      if (weaponId === undefined) { delete document.config.weaponId; document.schemaVersion = 5; }
      const b = browser(), entry = draft(document);
      originalStorage.saveOriginalDraft(b, entry);
      const snapshot = JSON.stringify([...b.sessionStorage.data, ...b.localStorage.data]);
      assert.deepEqual(originalStorage.loadOriginalDraft(b, character.id).draft, entry);
      assert.deepEqual(original.validateOriginalDocument(JSON.parse(JSON.stringify(document))), document);
      assert.equal(JSON.stringify([...b.sessionStorage.data, ...b.localStorage.data]), snapshot);
      assert.equal(selectedMakerWeaponId(document.config), weaponId ?? 'none');
      assert.equal(document.config.motion, 'walk');
    }
  }
});
