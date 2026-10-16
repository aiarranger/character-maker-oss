'use client';

import dynamic from 'next/dynamic';
import { Component, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type ChangeEvent, type CSSProperties, type ReactNode, type Ref } from 'react';
import {
  BUILD_RANGE, EXPRESSION_CHOICES, EXPRESSION_STRENGTH_RANGE, HAIR_COLORS, HEIGHT_RANGE, downloadFileName, getFaceShapes, getHairStyles, HAIR_STYLE_LABELS,
  currentCharacterDocument, makeDocument, parseDocument, sameDocument, serializeDocument, type CharacterConfig, type CharacterMakerDocument,
} from '../../lib/character-maker/config';
import {
  STORAGE_PREFIX, loadSavedCharacters, loadBrowserDraft, resolveDraftOrigin, saveBrowserDraft, saveCharacter, type SavedCharacter,
} from '../../lib/character-maker/storage';
import { createDraftWriter } from '../../lib/character-maker/draft-writer';
import styles from './character-studio.module.css';
import { WeaponPicker, type WeaponPickerHandle } from './WeaponPicker';
import { requestWeaponForMotion, motionAfterWeaponChoice, type ArmedWeaponMotion } from '../../lib/character-maker/weapon-motion-intent';
import { selectedMakerWeaponId, type MakerWeaponId } from '../../lib/character-maker/weapon-catalog';
import { makerAssetUrl } from '../../lib/character-maker/asset-url';
import { StageBackdrop, StageOverlay } from './StudioStage';
import { StudioIcon, type StudioIconName } from './StudioIcon';
import { paintRangeFills } from '../../lib/character-maker/studio-palette';

const CharacterMakerScene = dynamic(() => import('./CharacterMakerScene'), {
  ssr: false,
  loading: () => null,
});

type Category = 'body' | 'hair' | 'hairColor' | 'face' | 'eyes' | 'mouth' | 'expression' | 'motion';
type Notice = { type: 'success' | 'error'; text: string; link?: { href: string; label: string } } | null;
type PendingCharacter = { kind: 'saved'; entry: SavedCharacter } | { kind: 'import'; document: CharacterMakerDocument };
type HistoryState = { past: CharacterMakerDocument[]; future: CharacterMakerDocument[]; lastGroup: string; lastAt: number };
export type MakerEditorHandle = { captureSession: () => MakerEditorSession | null };
export type MakerEditorSession = {
  selectionKey: string; document: CharacterMakerDocument; baseline: CharacterMakerDocument;
  history: HistoryState; editing: SavedCharacter | null;
  preview: { category: Category; framing: 'full' | 'face'; view: 'front' | 'side' | 'back'; motion: MakerMotion; playing: boolean; speed: number; detailsOpen?: boolean };
};
type MakerMotion = 'idle' | 'walk' | 'run' | 'sword_idle' | 'sword_attack';
export type MakerPresetEditorProps = {
  selectionKey: string; initialDocument: CharacterMakerDocument;
  initialSession?: MakerEditorSession; initialEntry?: SavedCharacter | null;
  onBackToList?: (session: MakerEditorSession) => void;
  embedded?: boolean;
  editorRef?: Ref<MakerEditorHandle>;
  /** ゲームへ渡す（yukikau.jp への組み込み）。あるときは「この姿で遊ぶ」を出し、yukikau.jp 用の .character.zip を渡す。docs/yukikau-route.md */
  onPlay?: (file: File) => void | Promise<void>;
  /** Studio の表紙の情報（page から渡す）。docs/studio-quality-spec.md §8 */
  accent?: string;
  look?: string;
  groupLabel?: string;
  poster?: string;
};
const MOTIONS = [['idle', '待機'], ['walk', '歩く'], ['run', '走る'], ['sword_idle', '武器を構える'], ['sword_attack', '武器を振る']] as const;
const MOTION_ICONS: Record<MakerMotion, StudioIconName> = { idle: 'idle', walk: 'walk', run: 'run', sword_idle: 'guard', sword_attack: 'swing' };
const CATEGORIES: Array<{ id: Category; label: string; subtitle: string }> = [
  { id: 'body', label: '体型', subtitle: 'ベース・身長・体つき' },
  { id: 'hair', label: '髪型', subtitle: 'シルエットを選ぶ' },
  { id: 'hairColor', label: '髪色', subtitle: '好きな色に' },
  { id: 'face', label: '顔の形', subtitle: '輪郭を選ぶ' },
  { id: 'eyes', label: '目', subtitle: '目の形を選ぶ' },
  { id: 'mouth', label: '口', subtitle: '口もとの印象' },
  { id: 'expression', label: '表情', subtitle: '気持ちをまなざしに' },
  { id: 'motion', label: '動き', subtitle: '動作と再生設定' },
];
const FACE_LABELS: Record<CharacterConfig['face'], string> = { oval: 'たまご型', round: '丸顔', angular: 'シャープ', mature: '大人びた' };
const EYES: Array<[CharacterConfig['eyes'], string]> = [['almond', 'アーモンド'], ['round', 'まるい目'], ['slender', '細めの目']];
const MOUTHS: Array<[CharacterConfig['mouth'], string]> = [['natural', 'ナチュラル'], ['smile', '口角を上げる'], ['full', 'ふっくら']];
const OUTFIT_COLORS: ReadonlyArray<{ id: NonNullable<CharacterConfig['outfitPalette']>; name: string; color: string }> = [
  { id: 'original', name: 'オリジナル', color: '#b6afa6' }, { id: 'pearl', name: 'パール', color: '#E9E0C9' },
  { id: 'ink', name: 'インク', color: '#272B38' }, { id: 'sage', name: 'セージ', color: '#31594E' },
  { id: 'wine', name: 'ワイン', color: '#71364F' }, { id: 'cobalt', name: 'コバルト', color: '#284D85' },
  { id: 'ochre', name: 'オーカー', color: '#B56B35' },
];

function Symbol({ kind }: { kind: Category }) {
  return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {kind === 'body' && <><circle cx="12" cy="4.5" r="2.2" /><path d="M8 10h8l2 6M8 10l-2 6m3-6v5l-1 7m7-12v5l1 7M9 15h6" /></>}
    {kind === 'hair' && <><path d="M5 20V9a7 7 0 0 1 14 0v11M5 12c4 0 7-3 8-6 1 4 3 5 6 5" /><path d="M8 15c0 4 8 4 8 0" /></>}
    {kind === 'hairColor' && <><path d="M12 2S5 9 5 14a7 7 0 0 0 14 0C19 9 12 2 12 2Z" /><path d="M9 15a3 3 0 0 0 3 3" /></>}
    {kind === 'face' && <><path d="M5 8a7 7 0 0 1 14 0v5c0 5-5 9-7 9s-7-4-7-9Z" /><path d="M8 11h1m6 0h1m-6 5h4" /></>}
    {kind === 'eyes' && <><path d="M2 12s4-6 10-6 10 6 10 6-4 6-10 6S2 12 2 12Z" /><circle cx="12" cy="12" r="3" /></>}
    {kind === 'mouth' && <><path d="M3 12c5 0 6-4 9-2 3-2 4 2 9 2-3 6-15 6-18 0Z" /><path d="M3 12c6 2 12 2 18 0" /></>}
    {kind === 'expression' && <><circle cx="12" cy="12" r="9" /><path d="M7 9h1m8 0h1M7 14c2 4 8 4 10 0" /></>}
  </svg>;
}

function thumbnailUrl(config: Pick<CharacterConfig, 'style' | 'base'>, kind: string, value: string) {
  return makerAssetUrl(`thumb-${config.style}-${config.base}-${kind}-${value}.jpg`);
}

function CharacterThumbnail({ config, kind, value, className }: { config: Pick<CharacterConfig, 'style' | 'base'>; kind: string; value: string; className?: string }) {
  // These portraits are rendered from the same assets as the live character.
  // eslint-disable-next-line @next/next/no-img-element
  return <img src={thumbnailUrl(config, kind, value)} className={className} alt="" loading="lazy" decoding="async" />;
}

class SceneBoundary extends Component<{ children: ReactNode; onError: (message: string) => void; resetKey: string }, { error: boolean }> {
  state = { error: false };
  static getDerivedStateFromError() { return { error: true }; }
  componentDidCatch(error: Error) { this.props.onError(error.message || '3Dの表示に失敗しました。'); }
  componentDidUpdate(previous: Readonly<{ resetKey: string }>) {
    if (this.state.error && previous.resetKey !== this.props.resetKey) this.setState({ error: false });
  }
  render() { return this.state.error ? <div className={styles.loading} style={{ minHeight: 0, height: '100%', padding: 24, textAlign: 'center' }}>3Dを表示できませんでした。「もう一度読み込む」で再試行できます。</div> : this.props.children; }
}

function download(data: Blob, filename: string) {
  const url = URL.createObjectURL(data);
  const link = window.document.createElement('a');
  link.href = url; link.download = filename;
  window.document.body.appendChild(link); link.click(); link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function errorMessage(error: unknown) { return error instanceof Error ? error.message : '操作を完了できませんでした。もう一度お試しください。'; }

/** Updating appearance also updates the saved format; an unfinished name remains editable. */
function withConfig(document: CharacterMakerDocument, config: CharacterConfig): CharacterMakerDocument {
  return { ...makeDocument(document.name.trim() || '新しいキャラクター', config), name: document.name };
}

/** Mount with key={selectionKey}; only a matching explicitly supplied session is restored. */
export default function MakerPresetEditor({ selectionKey, initialDocument, initialSession, initialEntry, onBackToList, embedded = false, editorRef, onPlay, accent, look = '', groupLabel = '', poster }: MakerPresetEditorProps) {
  const [initial] = useState(() => makeDocument(initialDocument.name, initialDocument.config));
  const session = initialSession?.selectionKey === selectionKey ? initialSession : undefined;
  const [document, setDocument] = useState<CharacterMakerDocument>(() => currentCharacterDocument(session?.document ?? (initialEntry ? makeDocument(initialEntry.name, initialEntry.config) : initial)));
  const [baseline, setBaseline] = useState<CharacterMakerDocument>(() => currentCharacterDocument(session?.baseline ?? document));
  const [history, setHistory] = useState<HistoryState>(() => session ? { ...session.history,
    past: session.history.past.map(document => currentCharacterDocument(document)),
    future: session.history.future.map(document => currentCharacterDocument(document)),
  } : { past: [], future: [], lastGroup: '', lastAt: 0 });
  const alive = useRef(true);
  useEffect(() => { alive.current = true; return () => { alive.current = false; }; }, []);
  const documentRef = useRef(document);
  const historyRef = useRef(history);
  const [historyAnnouncement, setHistoryAnnouncement] = useState('');
  const [saved, setSaved] = useState<SavedCharacter[]>([]);
  const [editing, setEditing] = useState<SavedCharacter | null>(() => session?.editing ?? initialEntry ?? null);
  const [hydrated, setHydrated] = useState(false);
  const [category, setCategory] = useState<Category>(session?.preview.category ?? 'hair');
  const [detailsOpen, setDetailsOpen] = useState(session?.preview.detailsOpen ?? false);
  const [framing, setFraming] = useState<'full' | 'face'>(session?.preview.framing ?? 'full');
  const [view, setView] = useState<'front' | 'side' | 'back'>(session?.preview.view ?? 'front');
  const [motion, setMotion] = useState<MakerMotion>(selectedMakerWeaponId(document.config) === 'none' ? (session?.preview.motion.startsWith('sword_') ? 'idle' : session?.preview.motion ?? 'idle') : session?.preview.motion ?? 'sword_idle');
  const [pendingWeaponMotion, setPendingWeaponMotion] = useState<ArmedWeaponMotion | null>(null);
  const weaponPicker = useRef<WeaponPickerHandle>(null);
  const [entered, setEntered] = useState(false), [saveDone, setSaveDone] = useState(false);
  const detailsButton = useRef<HTMLButtonElement>(null), refocusDetails = useRef(false), rootRef = useRef<HTMLElement>(null);
  // スライダーの塗りは値から毎回描く（ドラッグ中も React の再描画で追従する）。
  useLayoutEffect(() => paintRangeFills(rootRef.current));
  const chosenWeaponMotion = useRef<MakerMotion | null>(null);
  const [playing, setPlaying] = useState(session?.preview.playing ?? true);
  const [speed, setSpeed] = useState(session?.preview.speed ?? 1);
  const [motionRevision, setMotionRevision] = useState(0);
  // のけぞり・＞＜ はその場の再生操作で、保存データには入れない（docs/main-character-spec.md）。
  const [hitRevision, setHitRevision] = useState(0);
  const [hurtFace, setHurtFace] = useState(false);
  const [cameraRevision, setCameraRevision] = useState(0);
  const [sceneRevision, setSceneRevision] = useState(0);
  const [ready, setReady] = useState(false);
  const [sceneError, setSceneError] = useState('');
  const [exporting, setExporting] = useState(false);
  // yukikau.jp 用の .character.zip を作っている間（'download' は書き出し、'play' は「この姿で遊ぶ」）
  const [packaging, setPackaging] = useState<'download' | 'play' | null>(null);
  const [notice, setNotice] = useState<Notice>(null);
  const [storageError, setStorageError] = useState('');
  const [importing, setImporting] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const draftFailureShown = useRef(false);
  const editingRef = useRef<SavedCharacter | null>(editing);
  const hydratedRef = useRef(false);
  function setEditingIdentity(next: SavedCharacter | null) { editingRef.current = next; setEditing(next); }
  const [draftWriter] = useState(() => createDraftWriter({
    getSnapshot: () => {
      const latest = documentRef.current;
      if (!hydratedRef.current || !latest.name.trim()) return null;
      return { selectionKey, document: latest, savedId: editingRef.current?.id ?? null, savedUpdatedAt: editingRef.current?.updatedAt ?? null };
    },
    write: snapshot => saveBrowserDraft(window, snapshot),
    setTimer: (callback, delay) => window.setTimeout(callback, delay),
    clearTimer: timer => window.clearTimeout(timer),
    onError: error => { if (!draftFailureShown.current) { setStorageError(errorMessage(error)); draftFailureShown.current = true; } },
    onSuccess: () => { if (draftFailureShown.current) { setStorageError(''); draftFailureShown.current = false; } },
  }));
  const config = document.config;
  const weaponId = selectedMakerWeaponId(config);
  const previousWeapon = useRef(weaponId);
  useEffect(() => {
    if (previousWeapon.current === weaponId) return;
    previousWeapon.current = weaponId;
    setMotion(chosenWeaponMotion.current ?? (weaponId === 'none' ? 'idle' : 'sword_idle')); chosenWeaponMotion.current = null; setPlaying(true);
    setFraming('full'); setView('front'); setMotionRevision(value => value + 1); setCameraRevision(value => value + 1);
  }, [weaponId]);
  const dirty = !sameDocument(document, baseline);
  const canUndo = history.past.length > 0;
  const canRedo = history.future.length > 0;
  const activeCategory = CATEGORIES.find(item => item.id === category)!;
  // アメコミの表情は ＞＜ の切り替えだけ（気持ちの表情はファンタジーだけ）。
  const categories = CATEGORIES;
  const hairs: Array<[CharacterConfig['hair'], string]> = getHairStyles(config.style, config.base).map(hair => [hair, HAIR_STYLE_LABELS[hair]]);
  const faces: Array<[CharacterConfig['face'], string]> = getFaceShapes(config.style).map(value => [value, FACE_LABELS[value]]);

  useLayoutEffect(() => { documentRef.current = document; }, [document]);

  function updateDocument(nextOrUpdate: CharacterMakerDocument | ((current: CharacterMakerDocument) => CharacterMakerDocument), group = '') {
    const current = documentRef.current;
    const next = currentCharacterDocument(typeof nextOrUpdate === 'function' ? nextOrUpdate(current) : nextOrUpdate);
    if (sameDocument(current, next)) return;
    const now = Date.now();
    const before = historyRef.current;
    const merge = Boolean(group && before.lastGroup === group && now - before.lastAt < 850);
    const nextHistory: HistoryState = {
      past: merge ? before.past : [...before.past, current].slice(-40),
      future: [], lastGroup: group, lastAt: now,
    };
    documentRef.current = next;
    historyRef.current = nextHistory;
    setHistory(nextHistory);
    setDocument(next);
    draftWriter.changed();
    setHistoryAnnouncement('');
  }

  function replaceDocument(next: CharacterMakerDocument) {
    setPendingWeaponMotion(null); chosenWeaponMotion.current = null;
    next = currentCharacterDocument(next);
    const empty: HistoryState = { past: [], future: [], lastGroup: '', lastAt: 0 };
    documentRef.current = next;
    historyRef.current = empty;
    setHistory(empty);
    setDocument(next);
    setHistoryAnnouncement('');
  }

  function undo() {
    setPendingWeaponMotion(null); chosenWeaponMotion.current = null;
    const before = historyRef.current;
    const previous = before.past[before.past.length - 1];
    if (!previous) return;
    const current = documentRef.current;
    const nextHistory: HistoryState = { past: before.past.slice(0, -1), future: [current, ...before.future].slice(0, 40), lastGroup: '', lastAt: 0 };
    documentRef.current = previous;
    historyRef.current = nextHistory;
    setHistory(nextHistory);
    setDocument(previous);
    draftWriter.changed();
    setNotice(null);
    setHistoryAnnouncement('直前の変更を元に戻しました。');
  }

  function redo() {
    setPendingWeaponMotion(null); chosenWeaponMotion.current = null;
    const before = historyRef.current;
    const nextDocument = before.future[0];
    if (!nextDocument) return;
    const current = documentRef.current;
    const nextHistory: HistoryState = { past: [...before.past, current].slice(-40), future: before.future.slice(1), lastGroup: '', lastAt: 0 };
    documentRef.current = nextDocument;
    historyRef.current = nextHistory;
    setHistory(nextHistory);
    setDocument(nextDocument);
    draftWriter.changed();
    setNotice(null);
    setHistoryAnnouncement('変更をやり直しました。');
  }

  function resetAppearance() {
    setPendingWeaponMotion(null); chosenWeaponMotion.current = null;
    updateDocument(current => withConfig(current, { ...initial.config }));
    setView('front'); setFraming('full'); setMotion(selectedMakerWeaponId(initial.config) === 'none' ? 'idle' : 'sword_idle'); setPlaying(true); setSpeed(1);
    setCameraRevision(value => value + 1); setSceneRevision(value => value + 1);
  }

  useEffect(() => {
    let entries: SavedCharacter[] = [];
    try {
      const loaded = loadSavedCharacters(window.localStorage);
      entries = loaded.characters; setSaved(entries);
      if (loaded.unreadable) setStorageError(`${loaded.unreadable}件の保存データを読み込めませんでした。読み込めるキャラクターは一覧に表示しています。`);
    } catch { setStorageError('このブラウザの保存データを開けませんでした。作成したキャラクターはJSONファイルに保存できます。'); }
    // A scoped draft may restore this exact chooser item, never another preset.
    try {
      if (session || initialEntry) {
        const previous = session?.editing ?? initialEntry;
        const origin = resolveDraftOrigin({ selectionKey, document: documentRef.current, savedId: previous?.id ?? null, savedUpdatedAt: previous?.updatedAt ?? null }, entries);
        setEditingIdentity(origin.editing);
        if (origin.warning) setStorageError(origin.warning);
      } else {
        const loaded = loadBrowserDraft(window, selectionKey);
        if (loaded.warning) setStorageError(loaded.warning);
        if (loaded.draft) {
          replaceDocument(loaded.draft.document);
          const origin = resolveDraftOrigin(loaded.draft, entries);
          setEditingIdentity(origin.editing);
          if (origin.editing) setBaseline(makeDocument(origin.editing.name, origin.editing.config));
          if (origin.warning) setStorageError(origin.warning);
          setMotion(selectedMakerWeaponId(loaded.draft.document.config) === 'none' ? 'idle' : 'sword_idle');
        }
      }
    } catch (error) { setStorageError(errorMessage(error)); }
    hydratedRef.current = true;
    setHydrated(true);
  }, []);

  useEffect(() => {
    const flush = () => draftWriter.flush();
    const onVisibility = () => { if (window.document.visibilityState === 'hidden') flush(); };
    window.addEventListener('pagehide', flush);
    window.document.addEventListener('visibilitychange', onVisibility);
    return () => {
      window.removeEventListener('pagehide', flush);
      window.document.removeEventListener('visibilitychange', onVisibility);
      // StrictMode/unmount cleanup must never write an obsolete source/identity.
      draftWriter.cancel();
    };
  }, [draftWriter]);

  useEffect(() => {
    const update = (event: StorageEvent) => {
      if (!event.key || event.key.startsWith(STORAGE_PREFIX)) {
        try { setSaved(loadSavedCharacters(window.localStorage).characters); } catch { /* A direct save reports storage failures. */ }
      }
    };
    window.addEventListener('storage', update);
    return () => window.removeEventListener('storage', update);
  }, []);

  const onSceneError = useCallback((message: string) => { setSceneError(message); setReady(false); }, []);
  // 埋め込み（Studio）では、読み込みの失敗はシーン内の板だけで知らせる（I-1・I-2）。SceneBoundary の例外は下のバーで知らせる。
  const onSceneLoadError = useCallback(() => setReady(false), []);
  const onReady = useCallback((value: boolean) => { setReady(value); if (value) setSceneError(''); }, []);

  // 幅1100px以上は細部を開いて始める。セッションの開閉が残っていれば、そちらを使う（§3.1）。
  useLayoutEffect(() => {
    if (embedded && session?.preview.detailsOpen === undefined && window.matchMedia('(min-width: 1100px)').matches) setDetailsOpen(true);
  }, []);
  useEffect(() => { if (ready) setEntered(true); }, [ready]);
  useEffect(() => {
    if (detailsOpen || !refocusDetails.current) return;
    refocusDetails.current = false;
    detailsButton.current?.focus();
  }, [detailsOpen]);
  // 髪型を開いたとき・キャラを替えたとき、選択中の髪型カードを編集欄の中で見える位置へ寄せる（編集欄がスクロールする幅だけ。文書は動かさない）。
  useEffect(() => {
    if (category !== 'hair' || !detailsOpen) return;
    const editor = rootRef.current?.querySelector<HTMLElement>(`.${styles.editor}`);
    const card = editor?.querySelector<HTMLElement>(`.${styles.originalHairChoices} [aria-pressed="true"]`);
    if (!editor || !card || !/(auto|scroll)/.test(getComputedStyle(editor).overflowY)) return;
    const box = editor.getBoundingClientRect(), item = card.getBoundingClientRect();
    if (item.top < box.top) editor.scrollTop += item.top - box.top - 8;
    else if (item.bottom > box.bottom) editor.scrollTop += item.bottom - box.bottom + 8;
  }, [category, detailsOpen]);
  // M5: 保存できたら保存ボタンに✓を1.6秒出す。成功の通知は4秒で消え、エラーは残る。
  useEffect(() => {
    if (!notice || notice.type === 'error' || notice.link) return;
    if (notice.text.endsWith('保存しました。')) setSaveDone(true);
    const hide = window.setTimeout(() => setNotice(null), 4000);
    return () => window.clearTimeout(hide);
  }, [notice]);
  useEffect(() => {
    if (!saveDone) return;
    const done = window.setTimeout(() => setSaveDone(false), 1600);
    return () => window.clearTimeout(done);
  }, [saveDone]);

  const onMotionComplete = useCallback(() => { setMotion(selectedMakerWeaponId(documentRef.current.config) === 'none' ? 'idle' : 'sword_idle'); }, []);

  useEffect(() => {
    const handleHistoryShortcut = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
      const key = event.key.toLowerCase();
      if (key !== 'z' && key !== 'y') return;
      const target = event.target;
      if (target instanceof HTMLElement) {
        const editor = target.closest('input, textarea, select, [contenteditable="true"]');
        if (editor && !(editor instanceof HTMLInputElement && editor.type === 'range')) return;
      }
      const redoAction = key === 'y' || (key === 'z' && event.shiftKey);
      if (redoAction ? !canRedo : !canUndo) return;
      event.preventDefault();
      if (redoAction) redo(); else undo();
    };
    window.addEventListener('keydown', handleHistoryShortcut);
    return () => window.removeEventListener('keydown', handleHistoryShortcut);
  }, [canUndo, canRedo]);

  function chooseWeapon(value: MakerWeaponId) {
    const nextMotion = motionAfterWeaponChoice(value, pendingWeaponMotion);
    chosenWeaponMotion.current = value === weaponId ? null : nextMotion; setPendingWeaponMotion(null);
    changeConfig('weaponId', value);
    setMotion(nextMotion); setPlaying(true); setFraming('full'); setView('front');
    setMotionRevision(revision => revision + 1); setCameraRevision(revision => revision + 1);
  }

  function chooseMotion(next: typeof motion) {
    const request = requestWeaponForMotion(next, weaponId);
    setPendingWeaponMotion(request); chosenWeaponMotion.current = null;
    if (request) { weaponPicker.current?.focusChoices(); return; }
    // Ready and swing share one view; replaying a motion must not reset an orbit.
    const keepCamera = framing === 'full' && (next === motion || (next.startsWith('sword_') && motion.startsWith('sword_')));
    setMotion(next); setPlaying(true); setMotionRevision(value => value + 1);
    if (next !== 'idle') { setFraming('full'); if (!keepCamera) setCameraRevision(value => value + 1); }
  }

  /** のけぞる：今の動作の上で1回のけぞり、終わると元の動作へ戻る。連打は最初から。 */
  function playHit() {
    setPlaying(true); setHitRevision(value => value + 1);
  }

  function changeConfig<K extends keyof CharacterConfig>(key: K, value: CharacterConfig[K], group = '') {
    const replacesPony = key === 'base' && value === 'masculine' && config.base !== 'masculine' && config.style === 'fantasy' && config.hair === 'sidePony';
    updateDocument(current => withConfig(current, { ...current.config, [key]: value, ...(replacesPony ? { hair: 'sideSweep' as const } : {}) }), group);
    setNotice(replacesPony ? { type: 'success', text: 'タイプBに合わせて髪型をサイドスイープに変更しました。元に戻すで前の設定に戻せます。' } : null);
  }

  function chooseCategory(next: Category) {
    if (embedded) setDetailsOpen(true);
    setCategory(next); setFraming(next === 'body' || next === 'motion' ? 'full' : 'face'); setCameraRevision(value => value + 1);
  }

  function chooseExpression(expression: CharacterConfig['expression']) {
    updateDocument(current => withConfig(current, { ...current.config, expression, expressionStrength: expression === 'neutral' || current.config.expressionStrength === 0 ? 1 : current.config.expressionStrength }));
    setNotice(null);
  }

  function save(asNew = false) {
    try {
      const latest = documentRef.current;
      const clean = makeDocument(latest.name, latest.config);
      const entry = saveCharacter(window.localStorage, clean, asNew ? null : editingRef.current);
      updateDocument(clean); setBaseline(clean); setEditingIdentity(entry);
      setSaved(current => [entry, ...current.filter(item => item.id !== entry.id)].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)));
      setStorageError(''); setNotice({ type: 'success', text: `「${entry.name}」を${asNew && editing ? '別のキャラとして' : ''}保存しました。` });
      try { saveBrowserDraft(window, { selectionKey, document: clean, savedId: entry.id, savedUpdatedAt: entry.updatedAt }); draftWriter.acceptPersisted(); } catch (error) { draftWriter.changed(); setStorageError(errorMessage(error)); }
    } catch (error) {
      setNotice({ type: 'error', text: errorMessage(error) });
      if (!document.name.trim()) nameInput.current?.focus();
    }
  }

  function applyCharacter(next: PendingCharacter) {
    const clean = next.kind === 'saved' ? makeDocument(next.entry.name, next.entry.config) : next.document;
    // A deliberate switch must survive an immediate reload; do not wait for the editing debounce.
    let persisted = false;
    try { saveBrowserDraft(window, {
      selectionKey, document: clean, savedId: next.kind === 'saved' ? next.entry.id : null,
      savedUpdatedAt: next.kind === 'saved' ? next.entry.updatedAt : null,
    }); persisted = true; }
    catch (error) { setStorageError(errorMessage(error)); }
    if (next.kind === 'saved') {
      replaceDocument(clean); setBaseline(clean); setEditingIdentity(next.entry); setNotice(null);
    } else {
      replaceDocument(clean); setBaseline(initial); setEditingIdentity(null);
      setNotice({ type: 'success', text: `「${next.document.name}」を読み込みました。保存すると一覧に追加できます。` });
    }
    if (persisted) draftWriter.acceptPersisted(); else draftWriter.changed();
    setMotion(selectedMakerWeaponId(clean.config) === 'none' ? 'idle' : 'sword_idle'); setPlaying(true); setFraming('full'); setView('front');
    setSceneRevision(value => value + 1); setCameraRevision(value => value + 1);
  }

  function requestCharacter(next: PendingCharacter) {
    applyCharacter(next);
  }

  function openSaved(id: string) {
    const entry = saved.find(item => item.id === id);
    if (entry) requestCharacter({ kind: 'saved', entry });
  }

  function exportJson() {
    try {
      const snapshot = makeDocument(document.name, config);
      download(new Blob([JSON.stringify(serializeDocument(snapshot), null, 2) + '\n'], { type: 'application/json' }), `${downloadFileName(snapshot.name)}.character.json`);
      setNotice({ type: 'success', text: '設定JSONを書き出しました。読み込むと、続きから編集できます。' });
    } catch (error) { setNotice({ type: 'error', text: errorMessage(error) }); }
  }

  async function importJson(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    setImporting(true);
    try {
      const imported = parseDocument(await file.text());
      if (!alive.current) return;
      requestCharacter({ kind: 'import', document: imported });
    } catch (error) { if (alive.current) setNotice({ type: 'error', text: errorMessage(error) }); }
    finally { if (alive.current) setImporting(false); }
  }

  async function exportGlb() {
    if (exporting || !ready) return;
    setExporting(true);
    try {
      const snapshot = makeDocument(document.name, config);
      const { exportCharacterGlb } = await import('../../lib/character-maker/assembly');
      const bytes = await exportCharacterGlb(snapshot.config);
      if (!alive.current) return;
      download(new Blob([bytes], { type: 'model/gltf-binary' }), `${downloadFileName(snapshot.name)}.glb`);
      setNotice({ type: 'success', text: '3DモデルのGLBを書き出しました。' });
    } catch (error) { if (alive.current) setNotice({ type: 'error', text: `GLBを書き出せませんでした。${errorMessage(error)}` }); }
    finally { if (alive.current) setExporting(false); }
  }
  /** yukikau.jp 用（.character.zip）。'download' は保存して持ち込みへのリンクを出し、'play' は onPlay へ渡す。docs/yukikau-route.md */
  async function exportYukikau(kind: 'download' | 'play') {
    if (exporting || !ready) return;
    setExporting(true); setPackaging(kind);
    try {
      const snapshot = makeDocument(document.name, config);
      const { exportPresetYukikauPackage, downloadYukikauPackage, YUKIKAU_IMPORT_URL } = await import('../../lib/character-maker/yukikau-package');
      const file = await exportPresetYukikauPackage(snapshot.name, snapshot.config, rootRef.current?.querySelector<HTMLCanvasElement>('#studio-stage canvas'));
      if (!alive.current) return;
      if (kind === 'play' && onPlay) { await onPlay(file); if (alive.current) setNotice({ type: 'success', text: 'この姿で遊べます。' }); }
      else { downloadYukikauPackage(file); setNotice({ type: 'success', text: 'yukikau.jp 用のファイルを書き出しました。', link: { href: YUKIKAU_IMPORT_URL, label: 'yukikau.jp で読み込む' } }); }
    } catch (error) { if (alive.current) setNotice({ type: 'error', text: `yukikau.jp 用のファイルを作れませんでした。${errorMessage(error)}` }); }
    finally { if (alive.current) { setExporting(false); setPackaging(null); } }
  }

  function shapeChoices<T extends string>(items: Array<[T, string]>, selected: T, onChange: (value: T) => void, kind: Category) {
    return <div className={styles.originalHairChoices} role="group" aria-label={activeCategory.label}>
      {items.map(([value, label]) => <button key={value} type="button" className={`${selected === value ? styles.selected : ''}`} aria-pressed={selected === value} onClick={() => onChange(value)}>
        <CharacterThumbnail config={config} kind={kind} value={value} className={styles.originalPartThumb} />
        <span>{label}</span>{selected === value && <b  aria-hidden="true">✓</b>}
      </button>)}
    </div>;
  }

  function captureSession(): MakerEditorSession | null {
    if (!hydratedRef.current) return null;
    if (importing || exporting) {
      setNotice({ type: 'error', text: '読み込み・書き出しが終わってから、キャラクターを切り替えてください。' });
      return null;
    }
    // Preserve the current slot even when this was restored without a new edit.
    draftWriter.changed();
    draftWriter.flush();
    // Never unmount after a failed flush or lose an unfinished name / history.
    if (draftWriter.hasPending()) {
      setNotice({ type: 'error', text: !documentRef.current.name.trim() ? '切り替える前に名前を入力してください。' : '編集内容の控えを保存できませんでした。保存またはJSONの書き出しを確認してから、もう一度お試しください。' });
      if (!documentRef.current.name.trim()) nameInput.current?.focus();
      return null;
    }
    // End the current edit group so an edit after returning is its own undo step.
    const capturedHistory = { ...historyRef.current, lastGroup: '', lastAt: 0 };
    historyRef.current = capturedHistory;
    return { selectionKey, document: documentRef.current, baseline, history: capturedHistory, editing: editingRef.current,
      preview: { category, framing, view, motion, playing, speed, detailsOpen } };
  }
  useImperativeHandle(editorRef, () => ({ captureSession }));
  function returnToList() {
    const session = captureSession();
    if (session) onBackToList?.(session);
  }
  const editorSection = category === 'expression' || category === 'motion' ? category : 'appearance';
  const appearanceCategories = categories.filter(item => item.id !== 'expression' && item.id !== 'motion');

  const toggleFraming = () => { setFraming(value => value === 'full' ? 'face' : 'full'); setCameraRevision(value => value + 1); };
  const heightPercent = ((config.height - HEIGHT_RANGE.min) / (HEIGHT_RANGE.max - HEIGHT_RANGE.min)) * 100;
  const buildPercent = ((config.build - BUILD_RANGE.min) / (BUILD_RANGE.max - BUILD_RANGE.min)) * 100;

  return <section ref={rootRef} className={`${styles.shell} ${styles.studio} ${embedded ? styles.liveEditor : ''}`} data-style={config.style} data-details-open={detailsOpen || !embedded}>
    <header className={styles.bar}>
      {!embedded && <button type="button" className={styles.backLink} onClick={returnToList}><span aria-hidden="true">‹</span>キャラクター一覧</button>}
      <div className={styles.historyControls} role="group" aria-label="編集履歴">
        <button type="button" className={styles.iconButton} disabled={!canUndo} aria-keyshortcuts="Control+Z Meta+Z" title="元に戻す (Ctrl/⌘+Z)" onClick={undo}><StudioIcon name="undo" /><span className={styles.srOnly}>元に戻す</span></button>
        <button type="button" className={styles.iconButton} disabled={!canRedo} aria-keyshortcuts="Control+Shift+Z Meta+Shift+Z Control+Y" title="やり直す (Ctrl/⌘+Shift+Z)" onClick={redo}><StudioIcon name="redo" /><span className={styles.srOnly}>やり直す</span></button>
        <button type="button" className={styles.iconButton} title="選んだキャラクターの見た目に戻す。元に戻す操作で取り消せます。" onClick={resetAppearance}><StudioIcon name="reset" /><span className={styles.srOnly}>リセット</span></button>
      </div>
      <span className={styles.historyStatus} aria-live="polite">{dirty && <span className={styles.dirtyChip}>未保存</span>}</span>
      <span className={styles.srOnly} aria-live="polite">{historyAnnouncement}</span>
      <div className={styles.saveArea}>
        {onPlay && <button type="button" className={styles.primary} disabled={!ready || exporting} onClick={() => exportYukikau('play')}>{packaging === 'play' ? '姿を作っています…' : 'この姿で遊ぶ'}</button>}
        <button type="button" className={styles.primary} data-done={saveDone} disabled={!hydrated} onClick={() => save()}><span className={styles.primaryCheck} aria-hidden="true"><StudioIcon name="check" size={18} /></span>保存する</button>
        <details className={styles.workspaceMenu} data-menu><summary><span className={styles.menuLabel}>書き出し</span><StudioIcon name="download" /></summary><div className={styles.menuPanel} data-menu-panel>
          <strong>ファイルに書き出す</strong>
          <button type="button" disabled={!ready || exporting} onClick={exportGlb}>{exporting && !packaging ? 'GLBを書き出し中…' : '3DモデルのGLB'}</button>
          <button type="button" onClick={exportJson}>編集用のJSON</button>
          <button type="button" disabled={!ready || exporting} onClick={() => exportYukikau('download')}>{packaging === 'download' ? '書き出し中…' : 'yukikau.jp 用（.character.zip）'}</button>
          <small>JSONは編集を続けるための設定です。GLBにはモデルと5種類の動作、のけぞり・＞＜が入ります。</small>
        </div></details>
        <details className={`${styles.workspaceMenu} ${styles.moreMenu}`} data-menu><summary aria-label="その他の操作"><StudioIcon name="more" /><span className={styles.srOnly}>•••</span></summary><div className={styles.menuPanel} data-menu-panel>
          <button type="button" disabled={!hydrated} onClick={() => save(true)}>別のキャラクターとして保存</button>
          <button type="button" disabled={importing} onClick={() => fileInput.current?.click()}>{importing ? '読み込み中…' : 'JSONを読み込む'}</button>
        </div></details>
      </div>
      <input ref={fileInput} type="file" accept=".json,application/json" aria-label="キャラクター設定JSON" className={styles.fileInput} onChange={importJson} />
    </header>
    <section className={styles.stage} id="studio-stage" aria-label="キャラクターの3Dプレビュー" data-entered={entered} data-view={view} data-framing={framing}>
      <StageBackdrop look={look} />
      <div className={styles.canvasSlot} data-entered={entered}>
        <SceneBoundary resetKey={`${sceneRevision}-${config.style}-${config.base}-${config.hair}`} onError={onSceneError}>
          <CharacterMakerScene config={config} modelRevision={sceneRevision} framing={framing} view={view} motion={motion} motionRevision={motionRevision} onMotionComplete={onMotionComplete} hitRevision={hitRevision} hurtFace={hurtFace} cameraRevision={cameraRevision} onReady={onReady} onError={embedded ? onSceneLoadError : onSceneError} playing={playing} speed={speed} studio={embedded} accent={accent} onFramingToggle={toggleFraming} />
        </SceneBoundary>
      </div>
      <StageOverlay poster={poster} entered={entered} weaponId={weaponId} view={view} framing={framing} />
      <div className={styles.nameBlock}>
        {(groupLabel || dirty) && <small className={styles.nameKicker}>{groupLabel}{dirty && <span className={styles.stageDirty} aria-hidden="true">未保存</span>}</small>}
        <label className={styles.nameField}><span className={styles.srOnly}>名前</span><input ref={nameInput} aria-label="キャラクターの名前" value={document.name} placeholder={initial.name} onChange={event => { updateDocument(current => ({ ...current, name: event.target.value }), 'name'); setNotice(null); }} /><StudioIcon name="pencil" size={18} className={styles.namePencil} /></label>
      </div>
      <div className={styles.cameraPill}>
        <div className={styles.pillGroup} role="group" aria-label="表示範囲">{([['full', '全身'], ['face', '顔']] as const).map(([value, label]) => <button type="button" key={value} aria-pressed={framing === value} onClick={() => { setFraming(value); setCameraRevision(revision => revision + 1); }}>{label}</button>)}</div>
        <i className={styles.pillDivider} aria-hidden="true" />
        <div className={styles.pillGroup} role="group" aria-label="見る方向">{([['front', '正面'], ['side', '横'], ['back', '後ろ']] as const).map(([value, label]) => <button type="button" key={value} aria-pressed={view === value} onClick={() => { setView(value); setCameraRevision(revision => revision + 1); }}>{label}</button>)}</div>
      </div>
      <WeaponPicker value={weaponId} onChange={chooseWeapon} pickerRef={weaponPicker} requestedMotion={pendingWeaponMotion} onCancelRequest={() => setPendingWeaponMotion(null)} />
      <div className={styles.stageAlert}>
        {sceneError && <div className={styles.error} role="alert"><span>{sceneError}</span><button type="button" className={styles.retryButton} onClick={() => { setSceneError(''); setReady(false); setSceneRevision(value => value + 1); }}>もう一度読み込む</button></div>}
      </div>
    </section>
    {/* 通知はステージに重ねるが、3Dプレビューの section の外に置く（section 内の alert は描画エラーだけ）。 */}
    <div className={styles.toasts}>
        {notice && <div className={notice.type === 'error' ? styles.error : styles.success} role={notice.type === 'error' ? 'alert' : 'status'}>{notice.type !== 'error' && <StudioIcon name="check" size={18} />}<span>{notice.text}{notice.link && <> <a href={notice.link.href} target="_blank" rel="noopener">{notice.link.label}</a></>}</span><button type="button" aria-label="通知を閉じる" onClick={() => setNotice(null)}><StudioIcon name="close" size={16} /></button></div>}
        {storageError && <div className={styles.error} role="alert"><span>{storageError}</span></div>}
    </div>
    <div className={styles.rail}>
      {embedded && <section id="studio-edit" tabIndex={-1} className={styles.commonControls} aria-label="共通のカスタマイズ">
        <div className={styles.commonSliders}>
          <label className={styles.slider} style={{ '--p': `${heightPercent}%`, '--d': `${((1 - HEIGHT_RANGE.min) / (HEIGHT_RANGE.max - HEIGHT_RANGE.min)) * 100}%` } as CSSProperties}><span>大きさ<output>{Math.round(config.height * 100)}%</output></span><input aria-label="大きさ" type="range" {...HEIGHT_RANGE} value={config.height} onChange={event => changeConfig('height', Number(event.target.value), 'slider:height')} /></label>
          <label className={styles.slider} style={{ '--p': `${buildPercent}%`, '--d': `${((0 - BUILD_RANGE.min) / (BUILD_RANGE.max - BUILD_RANGE.min)) * 100}%` } as CSSProperties}><span>体型<output data-text={config.build === 0 || undefined}>{config.build === 0 ? '標準' : `${config.build > 0 ? '+' : '−'}${Math.abs(Math.round(config.build * 100))}`}</output></span><input aria-label="体型" type="range" {...BUILD_RANGE} value={config.build} onChange={event => changeConfig('build', Number(event.target.value), 'slider:build')} /></label>
        </div>
        <div className={styles.commonMotion}>
          <div className={styles.motionHeading}><span id="maker-common-motion">動き</span><button type="button" className={styles.playToggle} aria-label={playing ? '動作を一時停止' : '動作を再生'} aria-pressed={playing} onClick={() => setPlaying(value => !value)}><StudioIcon name={playing ? 'pause' : 'play'} /></button></div>
          <div className={styles.motionGrid} role="group" aria-labelledby="maker-common-motion" style={{ '--n': MOTIONS.length + 1 } as CSSProperties}>{MOTIONS.map(([value, label]) => <button type="button" key={value} aria-label={label} title={label} aria-pressed={(pendingWeaponMotion ?? motion) === value} onClick={() => chooseMotion(value)}><StudioIcon name={MOTION_ICONS[value]} /><span>{label.replace('武器を', '')}</span></button>)}<button type="button" aria-label="のけぞる" title="のけぞる（1回のけぞって元の動きへ戻る）" onClick={playHit}><StudioIcon name="hit" /><span>のけぞる</span></button></div>
        </div>
        {!detailsOpen && <button ref={detailsButton} type="button" className={styles.detailsToggle} aria-expanded={false} aria-controls="maker-details" onClick={() => chooseCategory(category)}><StudioIcon name="plus" />細かくカスタマイズ</button>}
      </section>}
      {embedded && !detailsOpen && <div className={styles.shortcutGrid}>{categories.map(item => <button type="button" key={item.id} aria-label={`${item.label}を編集`} title={item.subtitle} onClick={() => chooseCategory(item.id)}>{item.id === 'motion' ? <StudioIcon name="walk" size={24} /> : <Symbol kind={item.id} />}<span>{item.label}</span></button>)}</div>}
      {(!embedded || detailsOpen) && <aside id="maker-details" className={styles.inspector} aria-label="カスタマイズ項目">
        {embedded && <div className={styles.detailHeading}><strong>細かくカスタマイズ</strong><button type="button" className={styles.iconButton} aria-label="詳細を閉じる" onClick={() => { refocusDetails.current = true; setDetailsOpen(false); }}><StudioIcon name="close" /></button></div>}
        <nav className={styles.sectionTabs} aria-label="編集する内容" style={{ gridTemplateColumns: 'repeat(3,minmax(0,1fr))' }}>
          <button type="button" aria-pressed={editorSection === 'appearance'} onClick={() => chooseCategory(appearanceCategories.some(item => item.id === category) ? category : 'hair')}><Symbol kind="hair" />見た目</button>
          <button type="button" aria-pressed={editorSection === 'expression'} onClick={() => chooseCategory('expression')}><Symbol kind="expression" />表情</button>
          <button type="button" aria-pressed={editorSection === 'motion'} onClick={() => chooseCategory('motion')}><StudioIcon name="walk" />動き</button>
        </nav>
        {editorSection === 'appearance' && <nav className={styles.categories} aria-label="見た目の項目">{appearanceCategories.map(item => <button type="button" key={item.id} title={item.subtitle} aria-pressed={category === item.id} className={category === item.id ? styles.activeCategory : ''} onClick={() => chooseCategory(item.id)}><Symbol kind={item.id} />{item.label}</button>)}</nav>}
        <section className={styles.editor} aria-labelledby="maker-current-category">
          <div className={styles.editorHeading}><h2 id="maker-current-category">{activeCategory.label}</h2></div>
        {category === 'body' && <div className={styles.originalOptions}>
          <fieldset className={styles.originalFieldset}><legend>体のベース</legend><div className={styles.originalHairChoices}>{([['feminine', 'タイプA', 'soft'], ['masculine', 'タイプB', 'fresh']] as const).map(([value, label, preset]) => <button type="button" key={value} aria-pressed={config.base === value} className={config.base === value ? styles.selected : ''} onClick={() => changeConfig('base', value)}><CharacterThumbnail config={{ style: config.style, base: value }} kind="preset" value={preset} className={styles.originalPartThumb} /><span>{label}</span></button>)}</div></fieldset>
          <fieldset className={styles.originalFieldset}><legend>服の色</legend><div className={styles.swatches} role="group" aria-label="服の色">{OUTFIT_COLORS.map(item => <button type="button" key={item.id} aria-label={item.name} aria-pressed={(config.outfitPalette ?? 'original') === item.id} className={(config.outfitPalette ?? 'original') === item.id ? styles.selectedSwatch : ''} style={{ backgroundColor: item.color }} title={item.name} onClick={() => changeConfig('outfitPalette', item.id)}>{(config.outfitPalette ?? 'original') === item.id && <span aria-hidden="true">✓</span>}</button>)}</div></fieldset>
          <label className={styles.slider}><span>身長<output>{Math.round(config.height * 100)}%</output></span><input aria-label="身長" type="range" {...HEIGHT_RANGE} value={config.height} onChange={event => changeConfig('height', Number(event.target.value), 'slider:height')} /><small><span>低め</span><span>高め</span></small></label>
          <label className={styles.slider}><span>体つき<output>{Math.round(config.build * 100)}</output></span><input aria-label="体つき" type="range" {...BUILD_RANGE} value={config.build} onChange={event => changeConfig('build', Number(event.target.value), 'slider:build')} /><small><span>細め</span><span>がっしり</span></small></label>
          <button type="button" className={styles.resetButton} onClick={() => { updateDocument(current => withConfig(current, { ...current.config, height: 1, build: 0 })); setNotice(null); }}>身長・体つきを標準に戻す</button>
        </div>}
        {category === 'hair' && <>{shapeChoices(hairs, config.hair, value => changeConfig('hair', value), 'hair')}</>}
        {category === 'hairColor' && <div className={styles.colorOptions}><div className={styles.swatches} role="group" aria-label="髪色の見本">{HAIR_COLORS.map(item => <button type="button" key={item.color} aria-label={item.name} aria-pressed={config.hairColor.toUpperCase() === item.color.toUpperCase()} className={config.hairColor.toUpperCase() === item.color.toUpperCase() ? styles.selectedSwatch : ''} style={{ backgroundColor: item.color }} title={item.name} onClick={() => changeConfig('hairColor', item.color)}>{config.hairColor.toUpperCase() === item.color.toUpperCase() && <span aria-hidden="true">✓</span>}</button>)}</div><label className={styles.customColor}><span>好きな色を選ぶ<small>あなたらしい色を、自由に。</small></span><input type="color" aria-label="髪色を自由に選ぶ" value={config.hairColor} onChange={event => changeConfig('hairColor', event.target.value.toUpperCase(), 'color:hairColor')} /></label>{config.hair === 'none' && <p className={styles.help}>髪型を選ぶと、この色が反映されます。</p>}</div>}
        {category === 'face' && shapeChoices(faces, config.face, value => changeConfig('face', value), 'face')}
        {category === 'eyes' && shapeChoices(EYES, config.eyes, value => changeConfig('eyes', value), 'eyes')}
        {category === 'mouth' && shapeChoices(MOUTHS, config.mouth, value => changeConfig('mouth', value), 'mouth')}
        {category === 'expression' && <div className={styles.originalOptions}>
          <fieldset className={styles.originalFieldset}><legend>のけぞったときの顔</legend><div className={styles.expressionButtons}><button type="button" aria-label="＞＜の顔" aria-pressed={hurtFace} onClick={() => setHurtFace(value => !value)}><StudioIcon name="hurtFace" size={18} />＞＜</button></div></fieldset>
          <p className={styles.help}>＞＜ はプレビューの表示で、保存しません。のけぞると自動で出ます。</p>
          {config.style === 'fantasy' && <>
          <div className={styles.originalHairChoices} role="group" aria-label="気持ちを選ぶ">{EXPRESSION_CHOICES.filter(item => item.group === 'emotion').map(item => <button type="button" key={item.id} className={`${''} ${config.expression === item.id ? styles.selected : ''}`} aria-pressed={config.expression === item.id} onClick={() => chooseExpression(item.id)}>
            <CharacterThumbnail config={config} kind="expression" value={item.id} className={styles.originalPartThumb} /><span>{item.label}</span>{config.expression === item.id && <b  aria-hidden="true">✓</b>}
          </button>)}</div>
          <label className={styles.slider}><span>表情の強さ<output>{Math.round(config.expressionStrength * 100)}%</output></span><input aria-label="表情の強さ" type="range" {...EXPRESSION_STRENGTH_RANGE} value={config.expressionStrength} disabled={config.expression === 'neutral'} onChange={event => changeConfig('expressionStrength', Number(event.target.value), 'slider:expressionStrength')} /><small><span>ひかえめ</span><span>はっきり</span></small></label>
          <details className={styles.expressionDetails}><summary>目・口・視線も試す</summary><p>選んだ表情に切り替わります。</p>
            {([['blink', '目を閉じる'], ['mouth', '口の動き'], ['gaze', '視線']] as const).map(([group, label]) => <fieldset key={group} className={styles.originalFieldset}><legend>{label}</legend><div className={styles.expressionButtons}>{EXPRESSION_CHOICES.filter(item => item.group === group).map(item => <button type="button" key={item.id} aria-pressed={config.expression === item.id} onClick={() => chooseExpression(item.id)}>{item.label}</button>)}</div></fieldset>)}
          </details>
          <button type="button" className={styles.resetButton} onClick={() => chooseExpression('neutral')}>ふつうの表情に戻す</button>
          </>}
        </div>}
        {category === 'motion' && <div className={styles.originalOptions}>
          <fieldset className={styles.originalFieldset}><legend>動作を選ぶ</legend><div className={styles.motionChoices} role="group" aria-label="動作">{MOTIONS.map(([value, label]) => <button type="button" key={value} aria-pressed={(pendingWeaponMotion ?? motion) === value} onClick={() => chooseMotion(value)}>{label}</button>)}</div></fieldset>
          <label className={styles.slider}><span>再生速度<output>{speed.toFixed(2)}倍</output></span><input aria-label="再生速度" type="range" min={0.25} max={2} step={0.05} value={speed} onChange={event => setSpeed(Number(event.target.value))} /><small><span>ゆっくり</span><span>速く</span></small></label>
          <p className={styles.help}>再生速度と一時停止はプレビューの設定です。</p>
        </div>}
        </section>
      </aside>}
      <details className={styles.savedDrawer} data-menu>
        <summary><StudioIcon name="cards" /><span>保存データ</span><span className={styles.countBadge}>{saved.length}</span></summary>
        <div className={styles.savedPanel} data-menu-panel>
          <section className={styles.library} aria-labelledby="maker-saved"><h2 id="maker-saved">保存データ</h2>{saved.length ? <div className={styles.savedList}>{saved.map(item => <button type="button" key={item.id} aria-label={item.name} aria-pressed={editing?.id === item.id} className={`${styles.savedCharacter} ${editing?.id === item.id ? styles.selected : ''}`} onClick={() => openSaved(item.id)}><span>{item.name}</span><small>{item.config.style === 'fantasy' ? 'ファンタジー' : 'コミック'}</small></button>)}</div> : <p>名前を付けて保存すると、ここから開けます。</p>}</section>
          <p className={styles.savedNote} title="保存データやJSONの読み込みで、元に戻す履歴は新しくなります。">このブラウザに保存・別の端末へはJSON</p>
        </div>
      </details>
    </div>
  </section>;
}
