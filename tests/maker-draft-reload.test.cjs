const fs = require('node:fs');
const ts = require('typescript');
require.extensions['.ts'] = (module, filename) => module._compile(ts.transpileModule(fs.readFileSync(filename, 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true }, fileName: filename,
}).outputText, filename);
const { test } = require('node:test');
const assert = require('node:assert/strict');
const original = require('../lib/character-maker/original-config.ts');
const os = require('../lib/character-maker/original-storage.ts');
const config = require('../lib/character-maker/config.ts');
const ps = require('../lib/character-maker/storage.ts');
const selection = require('../lib/character-maker/studio-selection.ts');
const { createDraftWriter } = require('../lib/character-maker/draft-writer.ts');
const catalog = require('../lib/character-maker/studio-catalog.json');
class Store {
  data = new Map(); get length() { return this.data.size; }
  getItem(key) { return this.data.get(key) ?? null; }
  setItem(key, value) { this.data.set(key, value); }
  key(index) { return [...this.data.keys()][index] ?? null; }
}
const browser = () => ({ sessionStorage: new Store(), localStorage: new Store() });
const draft = document => ({ document, savedId: null, savedUpdatedAt: null });
function armed(id, weaponId) {
  const doc = original.makeOriginalDocument(catalog.find(c => c.id === id));
  return original.currentOriginalDocument({ ...doc, config: { ...doc.config, weaponId, motion: weaponId === 'none' ? 'idle' : 'sword_idle' } });
}

test('Nami weapon -> Moka -> return Nami -> reload keeps unsaved weapon and independent drafts', () => {
  const b = browser(), nagi = draft(armed('generic-human-b', 'baguette_sword')), moka = draft(armed('generic-cat', 'none'));
  os.saveOriginalDraft(b, nagi);
  os.saveOriginalDraft(b, moka);
  assert.equal(JSON.parse(b.sessionStorage.getItem(os.ORIGINAL_DRAFT_KEY)).document.characterId, 'generic-cat');
  // Reload after returning via the in-memory Nami session also has a durable own slot.
  assert.equal(os.loadOriginalDraft(b, 'generic-human-b').draft.document.config.weaponId, 'baguette_sword');
  assert.equal(os.loadOriginalDraft(b, 'generic-human-b').draft.document.config.motion, 'sword_idle');
  assert.equal(os.loadOriginalDraft(b, 'generic-cat').draft.document.config.weaponId, 'none');
  // Deliberate capture persists unchanged restored sessions too, including legacy migration.
  const writer = createDraftWriter({ getSnapshot: () => nagi, write: d => os.saveOriginalDraft(b, d), setTimer: () => 1, clearTimer() {}, onError: e => { throw e; } });
  writer.changed(); writer.flush();
  assert.equal(writer.hasPending(), false);
  assert.deepEqual(os.loadOriginalDraft(b, 'generic-human-b').draft, nagi);
  assert.equal(os.loadOriginalCharacters(b.localStorage).characters.length, 0);
});

test('legacy matching Original fallback normalizes hair and is not hidden by another session character', () => {
  const b = browser(), nagi = draft(armed('generic-human-b', 'cake_sword'));
  nagi.document.config.appearance.hairstyle = 'nagi-wavy-lob-v3';
  b.localStorage.setItem(os.ORIGINAL_DRAFT_KEY, JSON.stringify(nagi));
  b.sessionStorage.setItem(os.ORIGINAL_DRAFT_KEY, JSON.stringify(draft(armed('generic-cat', 'none'))));
  const loaded = os.loadOriginalDraft(b, 'generic-human-b').draft;
  assert.equal(loaded.document.config.appearance.hairstyle, 'nagi-long-v2');
  assert.equal(loaded.document.config.weaponId, 'cake_sword');
  assert.equal(loaded.document.schemaVersion, 6);
  assert.equal(os.loadOriginalDraft(b, 'generic-human-a').draft, null);
});

test('preset switching/reload restores by immutable selectionKey, never editable name or base', () => {
  const b = browser();
  const a = { ...draft(config.makeDocument('同じ名前', { ...config.DEFAULT_CONFIG, weaponId: 'cake_sword' })), selectionKey: 'fantasy:soft' };
  const other = { ...draft(config.makeDocument('同じ名前', { ...config.DEFAULT_CONFIG, weaponId: 'saba_sword' })), selectionKey: 'fantasy:bright' };
  ps.saveBrowserDraft(b, a); ps.saveBrowserDraft(b, other);
  assert.deepEqual(ps.loadBrowserDraft(b, a.selectionKey).draft, a);
  assert.deepEqual(ps.loadBrowserDraft(b, other.selectionKey).draft, other);
  assert.equal(ps.loadBrowserDraft(b, 'comic:soft').draft, null);
  const legacy = browser(); ps.saveBrowserDraft(legacy, draft(a.document));
  assert.equal(ps.loadBrowserDraft(legacy, a.selectionKey).draft, null);
  assert.equal(ps.loadBrowserDraft(legacy).draft.document.config.weaponId, 'cake_sword');
  assert.equal(ps.loadSavedCharacters(b.localStorage).characters.length, 0);
});

test('chooser restores only allowed entries, preferring its tab and retaining a local fallback', () => {
  const b = browser(), allowed = ['original:generic-human-b', 'original:generic-cat', 'fantasy:soft'];
  assert.equal(selection.saveStudioSelection(b, 'fantasy:soft'), true);
  assert.equal(selection.loadStudioSelection(b, allowed), 'fantasy:soft');
  const nextTab = { sessionStorage: new Store(), localStorage: b.localStorage };
  assert.equal(selection.loadStudioSelection(nextTab, allowed), 'fantasy:soft');
  selection.saveStudioSelection(b, 'retired:unknown');
  assert.equal(selection.loadStudioSelection(b, allowed), null);
});
