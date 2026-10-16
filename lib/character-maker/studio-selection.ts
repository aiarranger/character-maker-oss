const SELECTION_KEY = 'characters.character-maker.selection.v1';
type BrowserStorage = { readonly sessionStorage: Pick<Storage, 'getItem' | 'setItem'>; readonly localStorage: Pick<Storage, 'getItem' | 'setItem'> };

/** Resolve only current chooser entries; invalid/stale keys never select hidden data. */
export function loadStudioSelection(browser: BrowserStorage, allowed: readonly string[]): string | null {
  for (const source of ['sessionStorage', 'localStorage'] as const) try {
    const value = browser[source].getItem(SELECTION_KEY);
    if (value && allowed.includes(value)) return value;
  } catch { /* Try the other browser store. */ }
  return null;
}

export function saveStudioSelection(browser: BrowserStorage, value: string): boolean {
  let saved = false;
  for (const source of ['sessionStorage', 'localStorage'] as const) try {
    browser[source].setItem(SELECTION_KEY, value);
    saved = browser[source].getItem(SELECTION_KEY) === value || saved;
  } catch { /* The editor's scoped draft is independently preserved. */ }
  return saved;
}
