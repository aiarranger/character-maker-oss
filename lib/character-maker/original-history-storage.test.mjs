import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { createEditHistory, commitEdit, replaceEditPresent, undoEdit, redoEdit } from './edit-history.ts';

// Mirrors the app's extensionless TypeScript resolution, scoped to these files.
const configUrl = new URL('./original-config.ts', import.meta.url).href;
const storageUrl = new URL('./original-storage.ts', import.meta.url).href;
const hook = registerHooks({ resolve(specifier, context, nextResolve) {
  if (context.parentURL === configUrl && specifier === './original-species') return nextResolve(new URL('./original-species.ts', import.meta.url).href, context);
  if (context.parentURL === storageUrl && specifier === './original-config') return nextResolve(configUrl, context);
  return nextResolve(specifier, context);
} });
const { makeOriginalDocument, sameOriginalDocument, validateOriginalDocument } = await import(configUrl);
const { saveOriginalCharacter, saveOriginalDraft, loadOriginalDraft, resolveOriginalDraftOrigin, ORIGINAL_STORAGE_PREFIX } = await import(storageUrl);
hook.deregister();
const character = { id: 'generic-human-b', name: 'ナミ', nameEn: 'Nami', thumbnail: null, thumbnailIsSheet: false, modelSha256: 'a'.repeat(64), modelUrl: '/model.glb', viewer: { neutralMouthClose: 0, headTurnDegrees: 15 }, expressions: ['blinkLeft', 'blinkRight', 'mouthClose', 'aa', 'ee', 'ou', 'happy'], motions: ['idle', 'walk', 'run', 'sword_idle', 'sword_attack'], hairMotion: false };
class MemoryStorage {
  data = new Map();
  get length() { return this.data.size; }
  key(index) { return [...this.data.keys()][index] ?? null; }
  getItem(key) { return this.data.get(key) ?? null; }
  setItem(key, value) { this.data.set(key, value); }
}
const changeSize = (h, size) => commitEdit(h, { ...h.present, config: { ...h.present.config, size } }, sameOriginalDocument);

test('same-record save → undo → save retains identity with current revision', () => {
  const storage = new MemoryStorage();
  let editing = saveOriginalCharacter(storage, makeOriginalDocument(character));
  let h = createEditHistory(validateOriginalDocument(editing));
  h = changeSize(h, 1.2);
  editing = saveOriginalCharacter(storage, h.present, editing);
  h = replaceEditPresent(h, validateOriginalDocument(editing));
  const id = editing.id, revision = editing.updatedAt;
  h = undoEdit(h);
  const saved = saveOriginalCharacter(storage, h.present, editing);
  assert.equal(saved.id, id); assert.notEqual(saved.updatedAt, revision); assert.equal(saved.config.size, 1);
  assert.equal(redoEdit(h).present.config.size, 1.2);
});

test('external newer save still rejects saving an undo snapshot; copy remains safe', () => {
  const storage = new MemoryStorage();
  const editing = saveOriginalCharacter(storage, makeOriginalDocument(character));
  let h = changeSize(createEditHistory(validateOriginalDocument(editing)), 1.2);
  const newer = saveOriginalCharacter(storage, { ...h.present, name: 'Other tab' }, editing);
  const before = storage.getItem(ORIGINAL_STORAGE_PREFIX + newer.id);
  h = undoEdit(h);
  assert.throws(() => saveOriginalCharacter(storage, h.present, editing), /別の画面で更新/);
  assert.equal(storage.getItem(ORIGINAL_STORAGE_PREFIX + newer.id), before);
  const copy = saveOriginalCharacter(storage, h.present, null);
  assert.notEqual(copy.id, newer.id);
  assert.equal(createEditHistory(validateOriginalDocument(copy)).past.length, 0);
});

test('undo snapshot draft reload preserves saved identity/revision and remains dirty', () => {
  const browser = { sessionStorage: new MemoryStorage(), localStorage: new MemoryStorage() };
  let editing = saveOriginalCharacter(browser.localStorage, makeOriginalDocument(character));
  let h = changeSize(createEditHistory(validateOriginalDocument(editing)), 1.2);
  editing = saveOriginalCharacter(browser.localStorage, h.present, editing);
  const baseline = validateOriginalDocument(editing);
  h = undoEdit(replaceEditPresent(h, baseline));
  saveOriginalDraft(browser, { document: h.present, savedId: editing.id, savedUpdatedAt: editing.updatedAt });
  const { draft } = loadOriginalDraft(browser);
  assert.ok(draft);
  const origin = resolveOriginalDraftOrigin(draft, [editing]);
  assert.equal(origin.editing.id, editing.id); assert.equal(origin.editing.updatedAt, editing.updatedAt);
  assert.equal(sameOriginalDocument(draft.document, baseline), false);
  assert.deepEqual(validateOriginalDocument(JSON.parse(JSON.stringify(draft.document))), h.present);
});
