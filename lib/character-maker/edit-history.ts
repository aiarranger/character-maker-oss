/** In-memory document history. Persisted record identity/revision is deliberately
 * not stored here: undo changes the draft, never which saved record it targets. */
export type EditHistory<T> = {
  readonly present: T;
  readonly past: readonly T[];
  readonly future: readonly T[];
  readonly group: string | null;
};
export const EDIT_HISTORY_LIMIT = 40;

export function createEditHistory<T>(present: T): EditHistory<T> {
  return { present: structuredClone(present), past: [], future: [], group: null };
}

export function commitEdit<T>(history: EditHistory<T>, next: T, same: (a: T, b: T) => boolean, group: string | null = null): EditHistory<T> {
  if (same(history.present, next)) return history;
  const merge = group !== null && group === history.group;
  return {
    present: structuredClone(next),
    past: merge ? history.past : [...history.past, history.present].slice(-EDIT_HISTORY_LIMIT),
    future: [],
    group,
  };
}

export function endEditGroup<T>(history: EditHistory<T>): EditHistory<T> {
  return history.group === null ? history : { ...history, group: null };
}

/** Normalization/animation completion is not a new user edit. */
export function replaceEditPresent<T>(history: EditHistory<T>, present: T): EditHistory<T> {
  return { ...history, present: structuredClone(present), group: null };
}

export function undoEdit<T>(history: EditHistory<T>): EditHistory<T> {
  if (history.past.length === 0) return history;
  // Indexing a non-empty history yields T even when T itself permits undefined.
  return { present: history.past[history.past.length - 1], past: history.past.slice(0, -1), future: [history.present, ...history.future].slice(0, EDIT_HISTORY_LIMIT), group: null };
}

export function redoEdit<T>(history: EditHistory<T>): EditHistory<T> {
  if (history.future.length === 0) return history;
  return { present: history.future[0], past: [...history.past, history.present].slice(-EDIT_HISTORY_LIMIT), future: history.future.slice(1), group: null };
}
