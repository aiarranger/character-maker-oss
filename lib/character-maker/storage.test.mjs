import test from 'node:test';
import assert from 'node:assert/strict';
import { register } from 'node:module';
import { COMIC_DEFAULT_CONFIG, DEFAULT_CONFIG, makeDocument, serializeDocument } from './config.ts';

// Next resolves extensionless TypeScript imports; use the same files directly in Node's test runner.
register('data:text/javascript,' + encodeURIComponent(`export async function resolve(specifier, context, next) {
  const bare = /^\\.\\.?\\//.test(specifier) && !/\\.[a-z]+$/.test(specifier) && context.parentURL?.endsWith('.ts');
  return next(bare ? specifier + '.ts' : specifier, context);
}`));
// 武器（schema5）より前の形式を確かめるテストは、武器なしの初期設定を使う。
const { weaponId: _defaultWeapon, ...LEGACY_DEFAULT } = DEFAULT_CONFIG;
const { weaponId: _comicWeapon, ...LEGACY_COMIC } = COMIC_DEFAULT_CONFIG;
const { DRAFT_KEY, STORAGE_PREFIX, loadBrowserDraft, loadDraft, loadSavedCharacters, resolveDraftOrigin, saveBrowserDraft, saveDraft, saveCharacter } = await import('./storage.ts');

class MemoryStorage {
  data = new Map();
  get length() { return this.data.size; }
  key(index) { return [...this.data.keys()][index] ?? null; }
  getItem(key) { return this.data.get(key) ?? null; }
  setItem(key, value) { this.data.set(key, value); }
}
const document = name => makeDocument(name, DEFAULT_CONFIG);

test('同名の2体を別IDで保存し、片方の再編集がもう片方へ波及しない', () => {
  const storage = new MemoryStorage();
  const a = saveCharacter(storage, document('同じ名前'));
  const b = saveCharacter(storage, document('同じ名前'));
  const beforeB = storage.getItem(STORAGE_PREFIX + b.id);
  const updated = saveCharacter(storage, makeDocument('編集後', { ...DEFAULT_CONFIG, hair: 'none', build: 0.5 }), a);
  assert.notEqual(a.id, b.id); assert.equal(updated.id, a.id); assert.equal(updated.createdAt, a.createdAt);
  assert.equal(storage.getItem(STORAGE_PREFIX + b.id), beforeB);
  const restored = loadSavedCharacters(storage);
  assert.equal(restored.characters.length, 2); assert.equal(restored.unreadable, 0);
  assert.equal(restored.characters.find(item => item.id === a.id).config.hair, 'none');
});

test('編集中の控えは保存済みキャラとは分けて復元できる', () => {
  const storage = new MemoryStorage();
  const saved = saveCharacter(storage, document('保存した名前'));
  const savedBytes = storage.getItem(STORAGE_PREFIX + saved.id);
  const draft = { document: makeDocument('編集中', { ...DEFAULT_CONFIG, height: 1.12 }), savedId: saved.id, savedUpdatedAt: saved.updatedAt };
  saveDraft(storage, draft);
  assert.deepEqual(loadDraft(storage), draft);
  assert.equal(storage.getItem(STORAGE_PREFIX + saved.id), savedBytes);
  assert.equal(loadSavedCharacters(storage).characters.length, 1);
});

test('一部の保存データが壊れても他の作品を開けて、壊れた値を勝手に消さない', () => {
  const storage = new MemoryStorage();
  const saved = saveCharacter(storage, document('読める作品'));
  const brokenKey = STORAGE_PREFIX + 'broken';
  storage.setItem(brokenKey, '{ broken');
  const result = loadSavedCharacters(storage);
  assert.deepEqual(result.characters.map(item => item.id), [saved.id]);
  assert.equal(result.unreadable, 1); assert.equal(storage.getItem(brokenKey), '{ broken');
  storage.setItem(DRAFT_KEY, 'null');
  assert.throws(() => loadDraft(storage));
});

test('容量不足とreadback失敗を保存成功にしない', () => {
  const quota = new MemoryStorage();
  quota.setItem = () => { throw new Error('QuotaExceededError'); };
  assert.throws(() => saveCharacter(quota, document('容量不足')), /保存できません/);
  const mismatch = new MemoryStorage();
  mismatch.getItem = () => null;
  assert.throws(() => saveCharacter(mismatch, document('readback失敗')), /保存できません/);
});

test('別画面で更新された同じ作品を古い状態で上書きしない', () => {
  const storage = new MemoryStorage();
  const saved = saveCharacter(storage, document('共通作品'));
  storage.setItem(STORAGE_PREFIX + saved.id, JSON.stringify({ ...saved, name: '別画面の更新', updatedAt: '2099-01-01T00:00:00.000Z' }));
  assert.throws(() => saveCharacter(storage, document('古い画面の更新'), saved), /別の画面/);
  assert.equal(loadSavedCharacters(storage).characters[0].name, '別画面の更新');
});

test('古いdraftの再読込とrevisionのない旧draftは、内容を残して別作品へ保存する', () => {
  for (const legacy of [false, true]) {
    const storage = new MemoryStorage();
    const original = saveCharacter(storage, document('元の作品'));
    const draft = { document: document('古い画面の編集中'), savedId: original.id, savedUpdatedAt: original.updatedAt };
    saveDraft(storage, draft);
    assert.equal(resolveDraftOrigin(loadDraft(storage), [original]).editing.id, original.id);
    const newest = saveCharacter(storage, document('別画面の最新版'), original);
    assert.notEqual(newest.updatedAt, original.updatedAt);
    if (legacy) {
      const { savedUpdatedAt, ...oldFormat } = draft;
      storage.setItem(DRAFT_KEY, JSON.stringify(oldFormat));
    }
    const restoredDraft = loadDraft(storage);
    const restored = resolveDraftOrigin(restoredDraft, loadSavedCharacters(storage).characters);
    assert.equal(restored.editing, null);
    assert.match(restored.warning, /別のキャラとして保存/);
    assert.equal(restoredDraft.document.name, '古い画面の編集中');
    const copy = saveCharacter(storage, restoredDraft.document, restored.editing);
    assert.notEqual(copy.id, newest.id);
    const all = loadSavedCharacters(storage).characters;
    assert.equal(all.length, 2);
    assert.equal(all.find(item => item.id === newest.id).name, '別画面の最新版');
    assert.equal(all.find(item => item.id === copy.id).name, '古い画面の編集中');
  }
});

test('2タブは自分のdraftを復元し、新しいタブは最後の控えを復元する', () => {
  const localStorage = new MemoryStorage();
  const a = { localStorage, sessionStorage: new MemoryStorage() };
  const b = { localStorage, sessionStorage: new MemoryStorage() };
  const aDraft = { document: document('タブAの編集中'), savedId: null, savedUpdatedAt: null };
  const bDraft = { document: document('タブBの編集中'), savedId: null, savedUpdatedAt: null };
  saveBrowserDraft(a, aDraft);
  assert.deepEqual(loadBrowserDraft(b).draft, aDraft);
  saveBrowserDraft(b, bDraft);
  assert.deepEqual(loadBrowserDraft(a), { draft: aDraft, warning: null });
  assert.deepEqual(loadBrowserDraft(b), { draft: bDraft, warning: null });
  const freshTab = { localStorage, sessionStorage: new MemoryStorage() };
  assert.deepEqual(loadBrowserDraft(freshTab), { draft: bDraft, warning: null });
});

test('片方の控えが保存できなくても他方へ保存し、失敗を通知する', () => {
  const failing = new MemoryStorage();
  failing.setItem = () => { throw new Error('QuotaExceededError'); };
  const working = new MemoryStorage();
  const draft = { document: document('編集中'), savedId: null, savedUpdatedAt: null };
  assert.throws(() => saveBrowserDraft({ sessionStorage: failing, localStorage: working }, draft), /このタブ専用/);
  assert.deepEqual(loadDraft(working), draft);
  assert.throws(() => saveBrowserDraft({ sessionStorage: working, localStorage: failing }, draft), /タブを閉じた後/);
  assert.deepEqual(loadDraft(working), draft);
  assert.throws(() => saveBrowserDraft({ sessionStorage: failing, localStorage: failing }, draft), /保存できません/);
});

test('旧作品を開いてdraftへ控えても保存recordを変えず、comicの永続形式をschema1に保つ', () => {
  const storage = new MemoryStorage();
  const legacy = {
    ...serializeDocument(makeDocument('従来のアメコミ', { ...LEGACY_COMIC, hair: 'buns' })),
    id: '11111111-1111-4111-8111-111111111111', createdAt: '2026-10-01T00:00:00.000Z', updatedAt: '2026-10-02T00:00:00.000Z',
  };
  const key = STORAGE_PREFIX + legacy.id;
  const originalBytes = JSON.stringify(legacy);
  storage.setItem(key, originalBytes);
  const restored = loadSavedCharacters(storage).characters[0];
  assert.equal(restored.config.style, 'comic');
  saveBrowserDraft({ localStorage: storage, sessionStorage: new MemoryStorage() }, { document: restored, savedId: restored.id, savedUpdatedAt: restored.updatedAt });
  assert.equal(storage.getItem(key), originalBytes);
  const persistedDraft = JSON.parse(storage.getItem(DRAFT_KEY));
  assert.equal(persistedDraft.document.schemaVersion, 1);
  assert.equal('style' in persistedDraft.document.config, false);
  const updated = saveCharacter(storage, makeDocument('明示して保存', { ...restored.config, hair: 'long' }), restored);
  const persistedRecord = JSON.parse(storage.getItem(key));
  assert.equal(updated.id, legacy.id);
  assert.equal(persistedRecord.schemaVersion, 1);
  assert.equal('style' in persistedRecord.config, false);
  assert.equal(persistedRecord.config.hair, 'long');
  assert.equal(loadSavedCharacters(storage).characters[0].config.style, 'comic');
});

test('表情と強さを保存・控え・再読込で維持し、リセット後は従来形式へ戻る', () => {
  const localStorage = new MemoryStorage();
  const sessionStorage = new MemoryStorage();
  const browser = { localStorage, sessionStorage };
  const current = makeDocument('笑顔の作品', { ...LEGACY_DEFAULT, expression: 'happy', expressionStrength: 0.5, eyes: 'slender', mouth: 'full' });
  const saved = saveCharacter(localStorage, current);
  assert.equal(JSON.parse(localStorage.getItem(STORAGE_PREFIX + saved.id)).schemaVersion, 3);
  assert.deepEqual(loadSavedCharacters(localStorage).characters[0].config, current.config);
  const draft = { document: makeDocument('編集中', { ...current.config, expression: 'lookLeft', expressionStrength: 0 }), savedId: saved.id, savedUpdatedAt: saved.updatedAt };
  saveBrowserDraft(browser, draft);
  assert.deepEqual(loadBrowserDraft(browser).draft, draft);
  assert.deepEqual(loadBrowserDraft({ localStorage, sessionStorage: new MemoryStorage() }).draft, draft);
  const updated = saveCharacter(localStorage, makeDocument(saved.name, { ...saved.config, expression: 'neutral' }), saved);
  const stored = JSON.parse(localStorage.getItem(STORAGE_PREFIX + updated.id));
  assert.equal(stored.schemaVersion, 2);
  assert.equal('expression' in stored.config, false);
  assert.equal('expressionStrength' in stored.config, false);
  assert.equal(loadSavedCharacters(localStorage).characters[0].config.expression, 'neutral');
  assert.equal(loadSavedCharacters(localStorage).characters[0].config.expressionStrength, 1);
});
