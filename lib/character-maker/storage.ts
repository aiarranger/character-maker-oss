import { serializeDocument, validateDocument, type CharacterMakerDocument } from './config';

export const STORAGE_PREFIX = 'characters.character-maker.v1.record.';
export const DRAFT_KEY = 'characters.character-maker.v1.draft';
const ID_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export type StorageLike = Pick<Storage, 'getItem' | 'setItem' | 'key' | 'length'>;
export type SavedCharacter = CharacterMakerDocument & { id: string; createdAt: string; updatedAt: string };
export type CharacterDraft = { selectionKey?: string; document: CharacterMakerDocument; savedId: string | null; savedUpdatedAt: string | null };
type BrowserDraftStorage = { readonly sessionStorage: StorageLike; readonly localStorage: StorageLike };

function record(value: unknown): SavedCharacter {
  const document = validateDocument(value);
  const input = value as Record<string, unknown>;
  if (typeof input.id !== 'string' || !ID_PATTERN.test(input.id)) throw new Error('保存したキャラクターのIDが読み込めません。');
  for (const field of ['createdAt', 'updatedAt']) {
    if (typeof input[field] !== 'string' || !Number.isFinite(Date.parse(input[field] as string))) throw new Error('保存日時が読み込めません。');
  }
  return { ...document, id: input.id, createdAt: input.createdAt as string, updatedAt: input.updatedAt as string };
}

function writeVerified(storage: StorageLike, key: string, value: unknown, subject = 'キャラクター') {
  const text = JSON.stringify(value);
  try {
    storage.setItem(key, text);
    if (storage.getItem(key) !== text) throw new Error('readback');
  } catch {
    throw new Error(`${subject}をこのブラウザに保存できませんでした。編集中の内容は画面に残っています。「JSONを書き出す」でファイルに保存できます。`);
  }
}

export function loadSavedCharacters(storage: StorageLike): { characters: SavedCharacter[]; unreadable: number } {
  const characters: SavedCharacter[] = [];
  let unreadable = 0;
  // Each character has its own key. Saving one character never rewrites another character's record.
  for (let i = 0; i < storage.length; i++) {
    const key = storage.key(i);
    if (!key?.startsWith(STORAGE_PREFIX)) continue;
    try {
      const entry = record(JSON.parse(storage.getItem(key) ?? 'null'));
      if (key !== STORAGE_PREFIX + entry.id) throw new Error('id mismatch');
      characters.push(entry);
    } catch { unreadable++; }
  }
  return { characters: characters.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id)), unreadable };
}

export function saveCharacter(storage: StorageLike, document: CharacterMakerDocument, previous?: SavedCharacter | null): SavedCharacter {
  const clean = validateDocument(document);
  // updatedAt also identifies the saved revision, so two edits in one millisecond must differ.
  const now = new Date(Math.max(Date.now(), previous ? Date.parse(previous.updatedAt) + 1 : 0)).toISOString();
  const saved: SavedCharacter = {
    ...clean, id: previous?.id ?? crypto.randomUUID(), createdAt: previous?.createdAt ?? now, updatedAt: now,
  };
  if (previous) {
    if (!ID_PATTERN.test(previous.id)) throw new Error('保存先のキャラクターが読み込めません。別のキャラとして保存してください。');
    const current = storage.getItem(STORAGE_PREFIX + previous.id);
    if (current) {
      const existing = record(JSON.parse(current));
      if (existing.updatedAt !== previous.updatedAt) throw new Error('このキャラクターは別の画面で更新されています。「別のキャラとして保存」で今の姿を残せます。');
    }
  }
  writeVerified(storage, STORAGE_PREFIX + saved.id, { ...serializeDocument(saved), id: saved.id, createdAt: saved.createdAt, updatedAt: saved.updatedAt });
  return saved;
}

function presetDraftKey(selectionKey: string) {
  if (!/^(fantasy|comic):[a-z][a-z0-9-]{0,48}$/.test(selectionKey)) throw new Error('控えのキャラクターを特定できません。');
  return `${DRAFT_KEY}.${selectionKey}`;
}
export function saveDraft(storage: StorageLike, draft: CharacterDraft) {
  const clean = {
    ...(draft.selectionKey ? { selectionKey: draft.selectionKey } : {}),
    document: serializeDocument(draft.document), savedId: draft.savedId,
    savedUpdatedAt: draft.savedId ? draft.savedUpdatedAt ?? null : null,
  };
  if (draft.selectionKey) writeVerified(storage, presetDraftKey(draft.selectionKey), clean, '編集中の控え');
  writeVerified(storage, DRAFT_KEY, clean, '編集中の控え');
}

export function loadDraft(storage: StorageLike, selectionKey?: string): CharacterDraft | null {
  const text = (selectionKey ? storage.getItem(presetDraftKey(selectionKey)) : null) ?? storage.getItem(DRAFT_KEY);
  if (!text) return null;
  let input: { selectionKey?: unknown; document?: unknown; savedId?: unknown; savedUpdatedAt?: unknown };
  try { input = JSON.parse(text); } catch { throw new Error('前回の編集中データを読み込めませんでした。保存したキャラクターは一覧から開けます。'); }
  if (!input || typeof input !== 'object') throw new Error('前回の編集中データを読み込めませんでした。');
  if (input.savedId !== null && (typeof input.savedId !== 'string' || !ID_PATTERN.test(input.savedId))) throw new Error('前回の保存先を読み込めませんでした。');
  if (selectionKey && input.selectionKey !== selectionKey) return null;
  if (input.selectionKey !== undefined && typeof input.selectionKey !== 'string') throw new Error('控えのキャラクターを特定できません。');
  if (typeof input.selectionKey === 'string') presetDraftKey(input.selectionKey);
  const savedUpdatedAt = typeof input.savedUpdatedAt === 'string' && Number.isFinite(Date.parse(input.savedUpdatedAt)) ? input.savedUpdatedAt : null;
  return { ...(typeof input.selectionKey === 'string' ? { selectionKey: input.selectionKey } : {}), document: validateDocument(input.document), savedId: input.savedId as string | null, savedUpdatedAt };
}

/** Keep each tab's work through reloads, and retain the last draft after a tab closes. */
export function saveBrowserDraft(browser: BrowserDraftStorage, draft: CharacterDraft) {
  const clean = { ...draft, document: validateDocument(draft.document) };
  let sessionSaved = false, localSaved = false;
  try { saveDraft(browser.sessionStorage, clean); sessionSaved = true; } catch { /* Try the persistent backup as well. */ }
  try { saveDraft(browser.localStorage, clean); localSaved = true; } catch { /* Report partial persistence below. */ }
  if (sessionSaved && localSaved) return;
  if (sessionSaved) throw new Error('このタブの編集中データは残せましたが、タブを閉じた後の控えを保存できませんでした。「JSONを書き出す」でファイルに保存できます。');
  if (localSaved) throw new Error('編集中データの控えは残せましたが、このタブ専用には保存できませんでした。別のタブで編集する前に「JSONを書き出す」でファイルに保存してください。');
  throw new Error('編集中の控えをこのブラウザに保存できませんでした。編集中の内容は画面に残っています。「JSONを書き出す」でファイルに保存できます。');
}

export function loadBrowserDraft(browser: BrowserDraftStorage, selectionKey?: string): { draft: CharacterDraft | null; warning: string | null } {
  let sessionFailed = false;
  try {
    const draft = loadDraft(browser.sessionStorage, selectionKey);
    if (draft) return { draft, warning: null };
  } catch { sessionFailed = true; }
  // A new tab has no session draft; the shared backup preserves the previous recovery behavior.
  const draft = loadDraft(browser.localStorage, selectionKey);
  return {
    draft,
    warning: sessionFailed
      ? draft ? 'このタブの編集中データを読み込めなかったため、ブラウザに残っている最後の控えを復元しました。' : 'このタブの編集中データを読み込めませんでした。保存したキャラクターは一覧から開けます。'
      : null,
  };
}

/** A restored draft can update its original only when it was based on that exact saved revision. */
export function resolveDraftOrigin(draft: CharacterDraft, characters: SavedCharacter[]): { editing: SavedCharacter | null; warning: string | null } {
  if (!draft.savedId) return { editing: null, warning: null };
  const current = characters.find(item => item.id === draft.savedId);
  if (current && draft.savedUpdatedAt === current.updatedAt) return { editing: current, warning: null };
  return {
    editing: null,
    warning: draft.savedUpdatedAt && current
      ? '保存したキャラクターは別の画面で更新されています。今の編集中データは残しています。「別のキャラとして保存」で残せます。'
      : '以前の編集中データを復元しました。元の保存内容を確認できないため、「別のキャラとして保存」で残してください。',
  };
}
