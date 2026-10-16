import { validateOriginalDocument, type OriginalDocument } from './original-config';

export const ORIGINAL_STORAGE_PREFIX = 'characters.character-maker.original.v1.record.';
export const ORIGINAL_DRAFT_KEY = 'characters.character-maker.original.v1.draft';
type StorageLike = Pick<Storage, 'getItem' | 'setItem' | 'key' | 'length'>;
type BrowserStorage = { readonly sessionStorage: StorageLike; readonly localStorage: StorageLike };
const idPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
export type SavedOriginal = OriginalDocument & { id: string; createdAt: string; updatedAt: string };
export type OriginalDraft = { document: OriginalDocument; savedId: string | null; savedUpdatedAt: string | null };

function record(value: unknown): SavedOriginal {
  const clean = validateOriginalDocument(value);
  const input = value as Record<string, unknown>;
  if (typeof input.id !== 'string' || !idPattern.test(input.id)) throw new Error('保存したキャラクターのIDが読み込めません。');
  for (const key of ['createdAt', 'updatedAt']) {
    if (typeof input[key] !== 'string' || !Number.isFinite(Date.parse(input[key] as string))) throw new Error('保存日時が読み込めません。');
  }
  return { ...clean, id: input.id, createdAt: input.createdAt as string, updatedAt: input.updatedAt as string };
}

function write(storage: StorageLike, key: string, data: unknown) {
  const text = JSON.stringify(data);
  try {
    storage.setItem(key, text);
    if (storage.getItem(key) !== text) throw new Error('readback');
  } catch {
    throw new Error('このブラウザに保存できませんでした。編集中の姿は残っています。「JSONを書き出す」でファイルに保存できます。');
  }
}

export function loadOriginalCharacters(storage: StorageLike): { characters: SavedOriginal[]; unreadable: number } {
  const characters: SavedOriginal[] = [];
  let unreadable = 0;
  for (let index = 0; index < storage.length; index++) {
    const key = storage.key(index);
    if (!key?.startsWith(ORIGINAL_STORAGE_PREFIX)) continue;
    try {
      const saved = record(JSON.parse(storage.getItem(key) ?? 'null'));
      if (key !== ORIGINAL_STORAGE_PREFIX + saved.id) throw new Error('id mismatch');
      characters.push(saved);
    } catch { unreadable++; }
  }
  return { characters: characters.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt) || a.id.localeCompare(b.id)), unreadable };
}

export function saveOriginalCharacter(storage: StorageLike, document: OriginalDocument, previous?: SavedOriginal | null): SavedOriginal {
  const clean = validateOriginalDocument(document);
  const now = new Date(Math.max(Date.now(), previous ? Date.parse(previous.updatedAt) + 1 : 0)).toISOString();
  if (previous) {
    if (!idPattern.test(previous.id)) throw new Error('保存先を読み込めません。別のキャラとして保存してください。');
    const current = storage.getItem(ORIGINAL_STORAGE_PREFIX + previous.id);
    if (current && record(JSON.parse(current)).updatedAt !== previous.updatedAt) throw new Error('別の画面で更新されています。「別のキャラとして保存」で今の姿を残せます。');
  }
  const saved: SavedOriginal = { ...clean, id: previous?.id ?? crypto.randomUUID(), createdAt: previous?.createdAt ?? now, updatedAt: now };
  write(storage, ORIGINAL_STORAGE_PREFIX + saved.id, saved);
  return saved;
}

function originalDraftKey(characterId: string) {
  if (!/^[a-z][a-z0-9-]{1,48}$/.test(characterId)) throw new Error('控えのキャラクターを特定できません。');
  return `${ORIGINAL_DRAFT_KEY}.${characterId}`;
}
function readDraft(storage: StorageLike, characterId?: string): OriginalDraft | null {
  const raw = (characterId ? storage.getItem(originalDraftKey(characterId)) : null) ?? storage.getItem(ORIGINAL_DRAFT_KEY);
  if (!raw) return null;
  const value = JSON.parse(raw);
  if (!value || typeof value !== 'object') throw new Error('編集中の控えを読み込めません。');
  if (value.savedId !== null && (typeof value.savedId !== 'string' || !idPattern.test(value.savedId))) throw new Error('編集中の保存先を読み込めません。');
  const document = validateOriginalDocument(value.document);
  if (characterId && document.characterId !== characterId) return null;
  return {
    document, savedId: value.savedId,
    savedUpdatedAt: typeof value.savedUpdatedAt === 'string' && Number.isFinite(Date.parse(value.savedUpdatedAt)) ? value.savedUpdatedAt : null,
  };
}

export function loadOriginalDraft(browser: BrowserStorage, characterId?: string): { draft: OriginalDraft | null; warning: string | null } {
  let failed = false;
  try {
    const draft = readDraft(browser.sessionStorage, characterId);
    if (draft) return { draft, warning: null };
  } catch { failed = true; }
  const draft = readDraft(browser.localStorage, characterId);
  return { draft, warning: failed ? 'このタブの控えを読み込めなかったため、ブラウザに残っている控えを確認しました。' : null };
}

export function saveOriginalDraft(browser: BrowserStorage, draft: OriginalDraft) {
  const clean = { ...draft, document: validateOriginalDocument(draft.document) };
  let session = false, local = false;
  try { write(browser.sessionStorage, originalDraftKey(clean.document.characterId), clean); write(browser.sessionStorage, ORIGINAL_DRAFT_KEY, clean); session = true; } catch { /* Persistent backup below. */ }
  try { write(browser.localStorage, originalDraftKey(clean.document.characterId), clean); write(browser.localStorage, ORIGINAL_DRAFT_KEY, clean); local = true; } catch { /* Preserve the session draft. */ }
  if (session && local) return;
  if (session) throw new Error('このタブの控えは残せましたが、タブを閉じた後の控えを保存できません。「JSONを書き出す」で保存できます。');
  if (local) throw new Error('ブラウザに控えは残せましたが、このタブ専用には保存できません。「JSONを書き出す」で保存できます。');
  throw new Error('編集中の控えを保存できません。今の姿は画面に残っています。「JSONを書き出す」で保存できます。');
}

export function resolveOriginalDraftOrigin(draft: OriginalDraft, characters: SavedOriginal[]): { editing: SavedOriginal | null; warning: string | null } {
  if (!draft.savedId) return { editing: null, warning: null };
  const saved = characters.find(item => item.id === draft.savedId);
  if (saved && saved.updatedAt === draft.savedUpdatedAt) return { editing: saved, warning: null };
  return { editing: null, warning: '元の保存内容が変更されています。編集中の控えは残しています。「別のキャラとして保存」で残せます。' };
}
