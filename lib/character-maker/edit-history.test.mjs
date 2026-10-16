import test from 'node:test';
import assert from 'node:assert/strict';
import { createEditHistory, commitEdit, endEditGroup, replaceEditPresent, undoEdit, redoEdit, EDIT_HISTORY_LIMIT } from './edit-history.ts';

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const draft = (value = 0) => ({ name: 'ナミ', config: { value, nested: { color: '#614439' } } });
const edit = (history, value, group = null) => commitEdit(history, { ...history.present, config: { ...history.present.config, value } }, same, group);

test('undo/redo preserve full nested snapshots and do not mutate input history', () => {
  const initial = createEditHistory(draft());
  const first = edit(initial, 1);
  const second = commitEdit(first, { ...first.present, name: 'New name', config: { ...first.present.config, nested: { color: '#123456' } } }, same);
  assert.deepEqual(undoEdit(second).present, first.present);
  assert.deepEqual(redoEdit(undoEdit(second)).present, second.present);
  assert.deepEqual(initial.present, draft());
  assert.equal(initial.past.length, 0);
});

test('a long drag stays one edit; ending the gesture creates a new step', () => {
  let h = createEditHistory(draft());
  for (let value = 1; value <= 120; value++) h = edit(h, value, 'size:1');
  assert.equal(h.past.length, 1);
  assert.equal(undoEdit(h).present.config.value, 0);
  h = edit(endEditGroup(h), 121, 'size:2');
  assert.equal(h.past.length, 2);
  assert.equal(undoEdit(h).present.config.value, 120);
});

test('separate keys, pointer gestures, color dialogs and presets never merge', () => {
  let h = createEditHistory(draft());
  h = edit(h, 1, 'size:1'); h = edit(h, 2, 'size:2'); h = edit(h, 3, 'color:1'); h = edit(h, 4);
  assert.equal(h.past.length, 4);
});

test('a no-op preserves redo and does not grow history', () => {
  const h = undoEdit(edit(createEditHistory(draft()), 1));
  assert.equal(commitEdit(h, structuredClone(h.present), same, 'unused'), h);
  assert.equal(h.future.length, 1);
});

test('new edit after undo clears the redo branch', () => {
  const h = edit(undoEdit(edit(edit(createEditHistory(draft()), 1), 2)), 3);
  assert.equal(h.future.length, 0);
  assert.equal(undoEdit(h).present.config.value, 1);
});

test('history is bounded and undo/redo empty boundaries are stable', () => {
  let h = createEditHistory(draft());
  assert.equal(undoEdit(h), h); assert.equal(redoEdit(h), h);
  for (let i = 1; i <= 100; i++) h = edit(h, i);
  assert.equal(h.past.length, EDIT_HISTORY_LIMIT);
  for (let i = 0; i < EDIT_HISTORY_LIMIT; i++) h = undoEdit(h);
  assert.equal(h.present.config.value, 60);
  assert.equal(h.future.length, EDIT_HISTORY_LIMIT);
  assert.equal(undoEdit(h), h);
});

test('source/saved/import and new saved identity reset cannot reach the old document', () => {
  const old = edit(createEditHistory({ ...draft(), characterId: 'nagi' }), 3);
  for (const characterId of ['haru', 'saved-nagi', 'imported-nagi', 'new-copy']) {
    const fresh = createEditHistory({ ...draft(), characterId });
    assert.equal(fresh.past.length, 0); assert.equal(fresh.future.length, 0);
    assert.equal(undoEdit(fresh), fresh); assert.equal(old.present.characterId, 'nagi');
  }
});

test('animation completion and successful-save normalization do not add steps', () => {
  let h = edit(createEditHistory(draft()), 1);
  const count = h.past.length;
  h = replaceEditPresent(h, { ...h.present, name: 'Trimmed' });
  assert.equal(h.past.length, count); assert.equal(h.group, null);
  assert.equal(undoEdit(h).present.name, 'ナミ');
  assert.equal(redoEdit(undoEdit(h)).present.name, 'Trimmed');
});

test('history owns clones of incoming values', () => {
  const source = draft(); const h = createEditHistory(source);
  source.config.nested.color = 'changed';
  assert.equal(h.present.config.nested.color, '#614439');
  const next = draft(1); const changed = commitEdit(h, next, same);
  next.config.nested.color = 'changed again';
  assert.equal(changed.present.config.nested.color, '#614439');
});
