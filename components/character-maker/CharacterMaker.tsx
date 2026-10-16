'use client';

import dynamic from 'next/dynamic';
import Link from 'next/link';
import { Component, useCallback, useEffect, useLayoutEffect, useRef, useState, type ChangeEvent, type ReactNode } from 'react';
import {
  BUILD_RANGE, COMIC_DEFAULT_CONFIG, DEFAULT_CONFIG, EXPRESSION_CHOICES, EXPRESSION_STRENGTH_RANGE, HAIR_COLORS, HEIGHT_RANGE, PRESETS, downloadFileName, getFaceShapes, getPresets,
  makeDocument, parseDocument, sameConfig, sameDocument, serializeDocument, type CharacterConfig, type CharacterMakerDocument,
} from '../../lib/character-maker/config';
import {
  STORAGE_PREFIX, loadBrowserDraft, loadSavedCharacters, resolveDraftOrigin, saveBrowserDraft, saveCharacter, type SavedCharacter,
} from '../../lib/character-maker/storage';
import { createDraftWriter } from '../../lib/character-maker/draft-writer';
import styles from './character-maker.module.css';
import OriginalCharacterMaker from './OriginalCharacterMaker';
import type { OriginalCharacter } from '../../lib/character-maker/original-config';

const CharacterMakerScene = dynamic(() => import('./CharacterMakerScene'), {
  ssr: false,
  loading: () => <div className={styles.loading}>キャラクターを読み込んでいます…</div>,
});

type Category = 'body' | 'hair' | 'hairColor' | 'face' | 'eyes' | 'mouth' | 'expression';
type Notice = { type: 'success' | 'error'; text: string } | null;
type PendingCharacter = { kind: 'saved'; entry: SavedCharacter } | { kind: 'import'; document: CharacterMakerDocument };
type HistoryState = { past: CharacterMakerDocument[]; future: CharacterMakerDocument[]; lastGroup: string; lastAt: number };
const MAKER_MODE_KEY = 'characters.character-maker.mode';
const CATEGORIES: Array<{ id: Category; label: string; subtitle: string }> = [
  { id: 'body', label: '体型', subtitle: 'ベース・身長・体つき' },
  { id: 'hair', label: '髪型', subtitle: 'シルエットを選ぶ' },
  { id: 'hairColor', label: '髪色', subtitle: '好きな色に' },
  { id: 'face', label: '顔の形', subtitle: '輪郭を選ぶ' },
  { id: 'eyes', label: '目', subtitle: '目の形を選ぶ' },
  { id: 'mouth', label: '口', subtitle: '口もとの印象' },
  { id: 'expression', label: '表情', subtitle: '気持ちをまなざしに' },
];
const COMIC_HAIRS: Array<[CharacterConfig['hair'], string]> = [
  ['parted', 'サイドパート'], ['long', 'ボブ'], ['buns', 'おだんご'], ['crop', 'ショート'], ['pixie', 'ベリーショート'], ['none', '髪なし'],
];
const FANTASY_HAIRS: Array<[CharacterConfig['hair'], string]> = [['animeParted', 'エアリーショート'], ['sidePony', 'サイドポニー'], ['layered', 'レイヤーショート'], ['none', '髪なし']];
const FACE_LABELS: Record<CharacterConfig['face'], string> = { oval: 'たまご型', round: '丸顔', angular: 'シャープ', mature: '大人びた' };
const EYES: Array<[CharacterConfig['eyes'], string]> = [['almond', 'アーモンド'], ['round', 'まるい目'], ['slender', '細めの目']];
const MOUTHS: Array<[CharacterConfig['mouth'], string]> = [['natural', 'ナチュラル'], ['smile', '口角を上げる'], ['full', 'ふっくら']];

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
  return `/api/character-maker/assets/thumb-${config.style}-${config.base}-${kind}-${value}.jpg`;
}

function CharacterThumbnail({ config, kind, value, className }: { config: Pick<CharacterConfig, 'style' | 'base'>; kind: string; value: string; className?: string }) {
  // These portraits are rendered from the same assets as the live character.
  // eslint-disable-next-line @next/next/no-img-element
  return <img src={thumbnailUrl(config, kind, value)} className={className} alt="" loading="lazy" decoding="async" />;
}

function AtelierMark() {
  return <svg viewBox="0 0 40 40" fill="none" aria-hidden="true"><path d="M20 2 37 20 20 38 3 20Z" stroke="currentColor" strokeWidth=".7" /><path d="M20 8c0 8 4 12 12 12-8 0-12 4-12 12 0-8-4-12-12-12 8 0 12-4 12-12Z" fill="currentColor" /><circle cx="20" cy="20" r="16" stroke="currentColor" strokeWidth=".4" /></svg>;
}

class SceneBoundary extends Component<{ children: ReactNode; onError: (message: string) => void; resetKey: string }, { error: boolean }> {
  state = { error: false };
  static getDerivedStateFromError() { return { error: true }; }
  componentDidCatch(error: Error) { this.props.onError(error.message || '3Dの表示に失敗しました。'); }
  componentDidUpdate(previous: Readonly<{ resetKey: string }>) {
    if (this.state.error && previous.resetKey !== this.props.resetKey) this.setState({ error: false });
  }
  render() { return this.state.error ? <div className={styles.loading}>3Dを表示できませんでした。プリセットを選び直すと再試行できます。</div> : this.props.children; }
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

export default function CharacterMaker({ originals, onPlay, initialName }: { originals: OriginalCharacter[]; onPlay?: (file: File) => void; initialName?: string }) {
  const [originalMode, setOriginalMode] = useState(false);
  const initial = makeDocument(initialName?.trim() || '新しいキャラクター', DEFAULT_CONFIG);
  const [document, setDocument] = useState<CharacterMakerDocument>(initial);
  const [baseline, setBaseline] = useState<CharacterMakerDocument>(initial);
  const [history, setHistory] = useState<HistoryState>({ past: [], future: [], lastGroup: '', lastAt: 0 });
  const documentRef = useRef(initial);
  const historyRef = useRef(history);
  const [historyAnnouncement, setHistoryAnnouncement] = useState('');
  const [saved, setSaved] = useState<SavedCharacter[]>([]);
  const [editing, setEditing] = useState<SavedCharacter | null>(null);
  const [hydrated, setHydrated] = useState(false);
  const [category, setCategory] = useState<Category>('face');
  const [framing, setFraming] = useState<'full' | 'face'>('face');
  const [view, setView] = useState<'front' | 'side' | 'back'>('front');
  const [motion, setMotion] = useState<'idle' | 'walk' | 'run' | 'sword_idle' | 'sword_attack'>('idle');
  const [motionRevision, setMotionRevision] = useState(0);
  const [cameraRevision, setCameraRevision] = useState(0);
  const [sceneRevision, setSceneRevision] = useState(0);
  const [ready, setReady] = useState(false);
  const [sceneError, setSceneError] = useState('');
  const [exporting, setExporting] = useState(false);
  const [notice, setNotice] = useState<Notice>(null);
  const [storageError, setStorageError] = useState('');
  const [importing, setImporting] = useState(false);
  const [previewPinned, setPreviewPinned] = useState(false);
  const previewColumn = useRef<HTMLElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const draftFailureShown = useRef(false);
  const styleHair = useRef<Record<CharacterConfig['style'], CharacterConfig['hair']>>({ fantasy: DEFAULT_CONFIG.hair, comic: COMIC_DEFAULT_CONFIG.hair });
  const styleFace = useRef<Record<CharacterConfig['style'], CharacterConfig['face']>>({ fantasy: DEFAULT_CONFIG.face, comic: COMIC_DEFAULT_CONFIG.face });
  const fantasyExpression = useRef<Pick<CharacterConfig, 'expression' | 'expressionStrength'>>({ expression: 'neutral', expressionStrength: 1 });
  const editingRef = useRef<SavedCharacter | null>(null);
  const hydratedRef = useRef(false);
  function setEditingIdentity(next: SavedCharacter | null) { editingRef.current = next; setEditing(next); }
  const [draftWriter] = useState(() => createDraftWriter({
    getSnapshot: () => {
      const latest = documentRef.current;
      if (!hydratedRef.current || !latest.name.trim()) return null;
      return { document: latest, savedId: editingRef.current?.id ?? null, savedUpdatedAt: editingRef.current?.updatedAt ?? null };
    },
    write: snapshot => saveBrowserDraft(window, snapshot),
    setTimer: (callback, delay) => window.setTimeout(callback, delay),
    clearTimer: timer => window.clearTimeout(timer),
    onError: error => { if (!draftFailureShown.current) { setStorageError(errorMessage(error)); draftFailureShown.current = true; } },
    onSuccess: () => { if (draftFailureShown.current) { setStorageError(''); draftFailureShown.current = false; } },
  }));
  const config = document.config;
  const dirty = !sameDocument(document, baseline);
  const canUndo = history.past.length > 0;
  const canRedo = history.future.length > 0;
  const activeCategory = CATEGORIES.find(item => item.id === category)!;
  const categories = CATEGORIES.filter(item => config.style === 'fantasy' || item.id !== 'expression');
  const presets = getPresets(config.style);
  const presetGroups = config.style === 'fantasy'
    ? (['feminine', 'masculine'] as const).map(base => ({ id: base, label: base === 'feminine' ? '女性' : '男性', presets: presets.filter(preset => preset.config.base === base) }))
    : [{ id: 'comic', label: null, presets }];
  const hairs = config.style === 'fantasy' ? FANTASY_HAIRS : COMIC_HAIRS;
  const faces: Array<[CharacterConfig['face'], string]> = getFaceShapes(config.style).map(value => [value, FACE_LABELS[value]]);

  useLayoutEffect(() => { documentRef.current = document; }, [document]);

  function updateDocument(nextOrUpdate: CharacterMakerDocument | ((current: CharacterMakerDocument) => CharacterMakerDocument), group = '') {
    const current = documentRef.current;
    const next = typeof nextOrUpdate === 'function' ? nextOrUpdate(current) : nextOrUpdate;
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
    const empty: HistoryState = { past: [], future: [], lastGroup: '', lastAt: 0 };
    documentRef.current = next;
    historyRef.current = empty;
    setHistory(empty);
    setDocument(next);
    setHistoryAnnouncement('');
  }

  function undo() {
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
    const defaults = documentRef.current.config.style === 'fantasy' ? DEFAULT_CONFIG : COMIC_DEFAULT_CONFIG;
    updateDocument(current => withConfig(current, { ...defaults, style: current.config.style }));
    setView('front'); setFraming('face'); setMotion('idle');
    setCameraRevision(value => value + 1); setSceneRevision(value => value + 1);
  }

  useEffect(() => {
    try { setOriginalMode(window.sessionStorage.getItem(MAKER_MODE_KEY) === 'original'); } catch { /* Editing remains available when browser storage is restricted. */ }
    let characters: SavedCharacter[] = [];
    try {
      const loaded = loadSavedCharacters(window.localStorage);
      characters = loaded.characters;
      setSaved(loaded.characters);
      if (loaded.unreadable) setStorageError(`${loaded.unreadable}件の保存データを読み込めませんでした。読み込めるキャラクターは一覧に表示しています。`);
    } catch { setStorageError('このブラウザの保存データを開けませんでした。作成したキャラクターはJSONファイルに保存できます。'); }
    try {
      const { draft, warning } = loadBrowserDraft(window);
      if (warning) setStorageError(warning);
      if (draft) {
        replaceDocument(draft.document);
        const restored = resolveDraftOrigin(draft, characters);
        setEditingIdentity(restored.editing);
        if (restored.editing) setBaseline(restored.editing);
        if (restored.warning) setStorageError(restored.warning);
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

  useEffect(() => {
    let frame = 0;
    const update = () => {
      frame = 0;
      setPreviewPinned(window.matchMedia('(max-width: 650px)').matches && (previewColumn.current?.getBoundingClientRect().top ?? 1) <= 0);
    };
    const schedule = () => { if (!frame) frame = window.requestAnimationFrame(update); };
    update();
    window.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', schedule);
    return () => { window.cancelAnimationFrame(frame); window.removeEventListener('scroll', schedule); window.removeEventListener('resize', schedule); };
  }, []);

  const onSceneError = useCallback((message: string) => { setSceneError(message); setReady(false); }, []);
  const onReady = useCallback((value: boolean) => { setReady(value); if (value) setSceneError(''); }, []);

  const onMotionComplete = useCallback(() => { setMotion('sword_idle'); }, []);

  useEffect(() => {
    if (config.style === 'comic' && category === 'expression') setCategory('face');
  }, [config.style, category]);

  useEffect(() => {
    if (originalMode) return;
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
  }, [canUndo, canRedo, originalMode]);

  function chooseStyle(style: CharacterConfig['style']) {
    if (style === config.style) return;
    styleHair.current[config.style] = config.hair;
    styleFace.current[config.style] = config.face;
    if (config.style === 'fantasy') fantasyExpression.current = { expression: config.expression, expressionStrength: config.expressionStrength };
    updateDocument(current => withConfig(current, { ...current.config, style, hair: styleHair.current[style], face: styleFace.current[style], ...(style === 'fantasy' ? fantasyExpression.current : { expression: 'neutral' as const, expressionStrength: 1 }) }));
    if (style === 'comic' && category === 'expression') setCategory('face');
    setSceneRevision(value => value + 1); setSceneError(''); setReady(false);
    setFraming('face'); setMotion('idle'); setCameraRevision(value => value + 1); setNotice(null);
  }

  function chooseMotion(next: typeof motion) {
    setMotion(next); setMotionRevision(value => value + 1);
    if (next !== 'idle') { setFraming('full'); setCameraRevision(value => value + 1); }
  }

  function changeConfig<K extends keyof CharacterConfig>(key: K, value: CharacterConfig[K], group = '') {
    updateDocument(current => withConfig(current, { ...current.config, [key]: value }), group);
    setNotice(null);
  }

  function chooseCategory(next: Category) {
    setCategory(next); setFraming(next === 'body' ? 'full' : 'face'); setCameraRevision(value => value + 1);
  }

  function chooseExpression(expression: CharacterConfig['expression']) {
    updateDocument(current => withConfig(current, { ...current.config, expression, expressionStrength: expression === 'neutral' || current.config.expressionStrength === 0 ? 1 : current.config.expressionStrength }));
    setNotice(null);
  }

  function choosePreset(preset: typeof PRESETS[number]) {
    updateDocument(current => withConfig(current, { ...preset.config }));
    setSceneRevision(value => value + 1); setSceneError(''); setReady(false);
    setCameraRevision(value => value + 1); setNotice(null);
  }

  function save(asNew = false) {
    try {
      const latest = documentRef.current;
      const clean = makeDocument(latest.name, latest.config);
      const entry = saveCharacter(window.localStorage, clean, asNew ? null : editingRef.current);
      updateDocument(clean); setBaseline(clean); setEditingIdentity(entry);
      setSaved(current => [entry, ...current.filter(item => item.id !== entry.id)].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)));
      setStorageError(''); setNotice({ type: 'success', text: `「${entry.name}」を${asNew && editing ? '別のキャラとして' : ''}保存しました。` });
      try { saveBrowserDraft(window, { document: clean, savedId: entry.id, savedUpdatedAt: entry.updatedAt }); draftWriter.acceptPersisted(); } catch (error) { draftWriter.changed(); setStorageError(errorMessage(error)); }
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
      document: clean, savedId: next.kind === 'saved' ? next.entry.id : null,
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
    if (clean.config.style === 'comic' && category === 'expression') setCategory('face');
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
      requestCharacter({ kind: 'import', document: imported });
    } catch (error) { setNotice({ type: 'error', text: errorMessage(error) }); }
    finally { setImporting(false); }
  }

  async function exportGlb() {
    if (exporting || !ready) return;
    setExporting(true);
    try {
      const snapshot = makeDocument(document.name, config);
      const { exportCharacterGlb } = await import('../../lib/character-maker/assembly');
      const bytes = await exportCharacterGlb(snapshot.config);
      const filename = `${downloadFileName(snapshot.name)}.glb`;
      if (onPlay) {
        await onPlay(new File([bytes], filename, { type: 'model/gltf-binary' }));
        setNotice({ type: 'success', text: 'この姿で遊べます。' });
      } else {
        download(new Blob([bytes], { type: 'model/gltf-binary' }), filename);
        setNotice({ type: 'success', text: '3DモデルのGLBを書き出しました。' });
      }
    } catch (error) { setNotice({ type: 'error', text: `GLBを書き出せませんでした。${errorMessage(error)}` }); }
    finally { setExporting(false); }
  }

  function shapeChoices<T extends string>(items: Array<[T, string]>, selected: T, onChange: (value: T) => void, kind: Category) {
    return <div className={`${styles.shapeGrid} ${items.length === 3 ? styles.shapeRows : ''} ${kind === 'face' && items.length === 4 ? styles.faceGrid : ''}`} role="group" aria-label={activeCategory.label}>
      {items.map(([value, label]) => <button key={value} type="button" className={`${styles.shapeCard} ${selected === value ? styles.selected : ''}`} aria-pressed={selected === value} onClick={() => onChange(value)}>
        <CharacterThumbnail config={config} kind={kind} value={value} className={styles.featurePreview} />
        <span>{label}</span>{selected === value && <b className={styles.check} aria-hidden="true">✓</b>}
      </button>)}
    </div>;
  }

  function openOriginals() {
    if (hydrated) {
      try { const latest = documentRef.current; saveBrowserDraft(window, { document: { ...latest, name: latest.name.trim() || '新しいキャラクター' }, savedId: editingRef.current?.id ?? null, savedUpdatedAt: editingRef.current?.updatedAt ?? null }); draftWriter.acceptPersisted(); }
      catch (error) { setStorageError(errorMessage(error)); }
    }
    try { window.sessionStorage.setItem(MAKER_MODE_KEY, 'original'); } catch { /* Mode is still kept in this page. */ }
    setOriginalMode(true);
  }

  if (!hydrated) return <main className={styles.shell} aria-busy="true">
    <section className={styles.libraryBar} role="status" aria-live="polite" style={{ minHeight: 240, alignContent: 'center', textAlign: 'center' }}>
      <p>保存した編集状態を確認しています…</p>
    </section>
  </main>;

  if (originalMode && originals.length) return <OriginalCharacterMaker characters={originals} initialCharacterId={originals[0].id} onBackToList={() => {
    setOriginalMode(false);
    try { window.sessionStorage.setItem(MAKER_MODE_KEY, 'human'); } catch { /* Mode is still kept in this page. */ }
  }} />;

  return <main className={styles.shell} data-style={config.style}>
    <header className={styles.header}>
      <div className={styles.brand}>
        {onPlay ? <span className={styles.backLink}>サービス内で作る</span> : <Link href="/" className={styles.backLink} onClick={() => draftWriter.flush()}>← キャラクター一覧</Link>}
        <div className={styles.brandTitle}><span className={styles.brandMark}><AtelierMark /></span><div><span className={styles.eyebrow}>CHARACTER ATELIER</span><h1>キャラクターメイク</h1></div></div>
      </div>
      <p className={styles.intro}>まだ知らない、<br />あなたの物語を。</p>
      <div className={styles.saveArea}>
        <label className={styles.nameField}><span>キャラクターの名前</span><input ref={nameInput} aria-label="キャラクターの名前" value={document.name} onChange={event => { updateDocument(current => ({ ...current, name: event.target.value }), 'name'); setNotice(null); }} /></label>
        <div className={styles.saveActions}><button type="button" className={styles.primary} disabled={!hydrated} onClick={() => save()}>保存</button><button type="button" className={styles.secondary} disabled={!hydrated} onClick={() => save(true)}>別のキャラとして保存</button></div>
      </div>
    </header>

    <div className={styles.actionBar} aria-label="編集とファイル操作">
      <div className={styles.historyControls} role="group" aria-label="編集を元に戻す">
        <button type="button" disabled={!canUndo} aria-keyshortcuts="Control+Z Meta+Z" title="直前の変更を元に戻す (Ctrl/⌘+Z)" onClick={undo}><span aria-hidden="true">↶</span>元に戻す</button>
        <button type="button" disabled={!canRedo} aria-keyshortcuts="Control+Shift+Z Meta+Shift+Z Control+Y" title="元に戻した変更をやり直す (Ctrl/⌘+Shift+Z)" onClick={redo}><span aria-hidden="true">↷</span>やり直す</button>
        <button type="button" disabled={!hydrated} onClick={resetAppearance}>初期状態</button>
      </div>
      <span className={styles.historyStatus} data-dirty={dirty} aria-live="polite">{!hydrated ? '準備しています' : dirty ? '保存前の変更あり' : editing ? '保存済み' : '新しいキャラクター'}</span>
      <div className={styles.fileActions} role="group" aria-label="設定とモデルの読み書き">
        <button type="button" onClick={exportJson}>JSONを書き出す</button>
        <button type="button" disabled={importing} onClick={() => fileInput.current?.click()}>{importing ? '読み込み中…' : 'JSONを読み込む'}</button>
        <button type="button" className={styles.glbButton} disabled={!ready || exporting} onClick={exportGlb}>{exporting ? (onPlay ? '姿を作っています…' : 'GLBを書き出し中…') : (onPlay ? 'この姿で遊ぶ' : 'GLBを書き出す')}</button>
      </div>
      <input ref={fileInput} type="file" accept=".json,application/json" aria-label="キャラクター設定JSON" className={styles.fileInput} onChange={importJson} />
      <span className={styles.srOnly} aria-live="polite">{historyAnnouncement}</span>
    </div>

    <div className={styles.workspace}>
      <aside className={styles.sidebar}>
        <section className={styles.styleSection} aria-labelledby="maker-style"><div className={styles.sectionHeading}><span>01</span><h2 id="maker-style">スタイル</h2></div>
          <div className={styles.styleOptions}>{([['fantasy', 'ファンタジー', '繊細で、やわらかな姿'], ['comic', 'アメコミ', '輪郭の際立つ、力強い姿']] as const).map(([style, label, description]) => <button key={style} type="button" aria-pressed={config.style === style} className={`${styles.styleOption} ${config.style === style ? styles.selected : ''}`} onClick={() => chooseStyle(style)}>
            <CharacterThumbnail config={{ style, base: 'feminine' }} kind="preset" value="soft" />
            <span><strong>{label}</strong><small>{description}</small></span><span className={styles.choiceDot} aria-hidden="true" />
          </button>)}{originals.length > 0 && <button type="button" aria-pressed={false} className={styles.styleOption} onClick={openOriginals}><span className={styles.originalMark} aria-hidden="true">✦</span><span><strong>オリジナル</strong><small>おなじみの仲間から</small></span><span className={styles.choiceDot} aria-hidden="true" /></button>}</div>
        </section>
        <div className={styles.sectionHeading}><span>02</span><h2>パーツを整える</h2></div>
        <nav className={styles.categories} aria-label="変更するパーツ">{categories.map((item, index) => <button type="button" key={item.id} aria-pressed={category === item.id} className={`${styles.category} ${category === item.id ? styles.activeCategory : ''}`} onClick={() => chooseCategory(item.id)}>
          <span className={styles.categorySymbol}><Symbol kind={item.id} /></span><span>{item.label}<small>{item.subtitle}</small></span><b aria-hidden="true">{String(index + 1).padStart(2, '0')}</b>
        </button>)}</nav>
        <p className={styles.sidebarNote}>ひとつずつ、あなたらしく。</p>
      </aside>

      <section ref={previewColumn} className={`${styles.stageColumn} ${previewPinned ? styles.pinnedPreview : ''}`} aria-label="キャラクターの3Dプレビュー">
        <div className={styles.stage}>
          <div className={styles.orbit} aria-hidden="true" />
          <div className={styles.stageHeading}><span>{config.style === 'fantasy' ? 'FANTASY' : 'COMIC'}</span><small>{dirty ? '保存していない変更あり' : editing ? '保存済み' : '自分だけの姿をつくろう'}</small></div>
          <div className={styles.framing} role="group" aria-label="表示範囲">{([['full', '全身'], ['face', '顔']] as const).map(([value, label]) => <button type="button" key={value} aria-pressed={framing === value} onClick={() => { setFraming(value); setCameraRevision(revision => revision + 1); }}>{label}</button>)}</div>
          <SceneBoundary resetKey={`${sceneRevision}-${config.style}-${config.base}-${config.hair}`} onError={onSceneError}>
            <CharacterMakerScene config={config} modelRevision={sceneRevision} framing={framing} view={view} motion={motion} motionRevision={motionRevision} onMotionComplete={onMotionComplete} cameraRevision={cameraRevision} onReady={onReady} onError={onSceneError} />
          </SceneBoundary>
          {!ready && !sceneError && <div className={styles.stageLoading} role="status"><AtelierMark /><span>キャラクターを読み込んでいます…</span></div>}
        </div>
        <div className={styles.cameraBar}>
          <div className={styles.previewCaption}><strong>{document.name || '新しいキャラクター'}</strong><span className={styles.stageHint}>ドラッグで回転・スクロールで拡大</span></div>
          <div className={styles.viewControls} role="group" aria-label="見る方向">{([['front', '正面'], ['side', '横'], ['back', '後ろ']] as const).map(([value, label]) => <button type="button" key={value} aria-pressed={view === value} onClick={() => { setView(value); setCameraRevision(revision => revision + 1); }}>{label}</button>)}</div>
        </div>
        <div className={styles.stageFooter}>
          <div className={styles.motionChoices} role="group" aria-label="動作">{([['idle', '待機'], ['walk', '歩く'], ['run', '走る'], ['sword_idle', '剣を構える']] as const).map(([value, label]) => <button type="button" key={value} aria-pressed={motion === value} onClick={() => chooseMotion(value)}>{label}</button>)}</div>
          <button type="button" className={styles.swordButton} disabled={!ready} onClick={() => chooseMotion('sword_attack')}><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true"><path d="m7 17 11-13 3-1-1 4L9 19M5 13l6 6M3 21l4-4M2 19l3 3" /></svg>剣を振る</button>
        </div>
        {sceneError && <div className={styles.error} role="alert">{sceneError}<button type="button" onClick={() => { setSceneError(''); setSceneRevision(value => value + 1); }}>もう一度読み込む</button></div>}
      </section>

      <section className={styles.editor} aria-labelledby="maker-current-category">
        <div className={styles.editorHeading}><div><span className={styles.eyebrow}>YOUR DETAILS</span><h2 id="maker-current-category">{activeCategory.label}</h2><p>{activeCategory.subtitle}</p></div><span className={styles.categorySymbol}><Symbol kind={category} /></span></div>
        {category === 'body' && <div className={styles.bodyOptions}>
          <fieldset><legend>体のベース</legend><div className={styles.baseOptions}>{([['feminine', 'タイプA', 'soft'], ['masculine', 'タイプB', 'fresh']] as const).map(([value, label, preset]) => <button type="button" key={value} aria-pressed={config.base === value} className={config.base === value ? styles.selected : ''} onClick={() => changeConfig('base', value)}><CharacterThumbnail config={{ style: config.style, base: value }} kind="preset" value={preset} /><span>{label}</span></button>)}</div></fieldset>
          <label className={styles.slider}><span>身長<output>{Math.round(config.height * 100)}%</output></span><input aria-label="身長" type="range" {...HEIGHT_RANGE} value={config.height} onChange={event => changeConfig('height', Number(event.target.value), 'slider:height')} /><small><span>低め</span><span>高め</span></small></label>
          <label className={styles.slider}><span>体つき<output>{Math.round(config.build * 100)}</output></span><input aria-label="体つき" type="range" {...BUILD_RANGE} value={config.build} onChange={event => changeConfig('build', Number(event.target.value), 'slider:build')} /><small><span>細め</span><span>がっしり</span></small></label>
          <button type="button" className={styles.resetButton} onClick={() => { updateDocument(current => withConfig(current, { ...current.config, height: 1, build: 0 })); setNotice(null); }}>身長・体つきを標準に戻す</button>
        </div>}
        {category === 'hair' && shapeChoices(hairs, config.hair, value => changeConfig('hair', value), 'hair')}
        {category === 'hairColor' && <div className={styles.colorOptions}><div className={styles.swatches} role="group" aria-label="髪色の見本">{HAIR_COLORS.map(item => <button type="button" key={item.color} aria-label={item.name} aria-pressed={config.hairColor.toUpperCase() === item.color.toUpperCase()} className={config.hairColor.toUpperCase() === item.color.toUpperCase() ? styles.selectedSwatch : ''} style={{ backgroundColor: item.color }} title={item.name} onClick={() => changeConfig('hairColor', item.color)}>{config.hairColor.toUpperCase() === item.color.toUpperCase() && <span aria-hidden="true">✓</span>}</button>)}</div><label className={styles.customColor}><span>好きな色を選ぶ<small>あなたらしい色を、自由に。</small></span><input type="color" aria-label="髪色を自由に選ぶ" value={config.hairColor} onChange={event => changeConfig('hairColor', event.target.value.toUpperCase(), 'color:hairColor')} /></label>{config.hair === 'none' && <p className={styles.help}>髪型を選ぶと、この色が反映されます。</p>}</div>}
        {category === 'face' && shapeChoices(faces, config.face, value => changeConfig('face', value), 'face')}
        {category === 'eyes' && shapeChoices(EYES, config.eyes, value => changeConfig('eyes', value), 'eyes')}
        {category === 'mouth' && shapeChoices(MOUTHS, config.mouth, value => changeConfig('mouth', value), 'mouth')}
        {category === 'expression' && config.style === 'fantasy' && <div className={styles.expressionOptions}>
          <div className={styles.expressionGrid} role="group" aria-label="気持ちを選ぶ">{EXPRESSION_CHOICES.filter(item => item.group === 'emotion').map(item => <button type="button" key={item.id} className={`${styles.shapeCard} ${config.expression === item.id ? styles.selected : ''}`} aria-pressed={config.expression === item.id} onClick={() => chooseExpression(item.id)}>
            <CharacterThumbnail config={config} kind="expression" value={item.id} className={styles.featurePreview} /><span>{item.label}</span>{config.expression === item.id && <b className={styles.check} aria-hidden="true">✓</b>}
          </button>)}</div>
          <label className={styles.slider}><span>表情の強さ<output>{Math.round(config.expressionStrength * 100)}%</output></span><input aria-label="表情の強さ" type="range" {...EXPRESSION_STRENGTH_RANGE} value={config.expressionStrength} disabled={config.expression === 'neutral'} onChange={event => changeConfig('expressionStrength', Number(event.target.value), 'slider:expressionStrength')} /><small><span>ひかえめ</span><span>はっきり</span></small></label>
          <details className={styles.expressionDetails}><summary>目・口・視線も試す</summary><p>選んだ表情に切り替わります。</p>
            {([['blink', '目を閉じる'], ['mouth', '口の動き'], ['gaze', '視線']] as const).map(([group, label]) => <fieldset key={group}><legend>{label}</legend><div className={styles.expressionButtons}>{EXPRESSION_CHOICES.filter(item => item.group === group).map(item => <button type="button" key={item.id} aria-pressed={config.expression === item.id} onClick={() => chooseExpression(item.id)}>{item.label}</button>)}</div></fieldset>)}
          </details>
          <button type="button" className={styles.resetButton} onClick={() => chooseExpression('neutral')}>ふつうの表情に戻す</button>
        </div>}
        <section className={styles.presets} aria-labelledby="maker-presets"><div className={styles.presetHeading}><h3 id="maker-presets">スタイルから選ぶ</h3><span>迷ったら、ここから。</span></div>
          {presetGroups.map(group => <div key={group.id} className={styles.presetGroup} role={group.label ? 'group' : undefined} aria-labelledby={group.label ? `maker-presets-${group.id}` : undefined}>
            {group.label && <h4 id={`maker-presets-${group.id}`} className={styles.presetGroupHeading}>{group.label}<span>{group.presets.length}体</span></h4>}
            <div className={styles.presetGrid}>{group.presets.map(preset => <button type="button" key={preset.id} title={preset.description} aria-pressed={sameConfig(config, preset.config)} className={`${styles.preset} ${sameConfig(config, preset.config) ? styles.selected : ''}`} onClick={() => choosePreset(preset)}>
              <CharacterThumbnail config={preset.config} kind="preset" value={preset.id} className={styles.presetPortrait} /><span>{preset.name}</span>
            </button>)}</div>
          </div>)}
        </section>
      </section>
    </div>

    {notice && <div className={notice.type === 'error' ? styles.error : styles.success} role={notice.type === 'error' ? 'alert' : 'status'}>{notice.text}<button type="button" aria-label="通知を閉じる" onClick={() => setNotice(null)}>×</button></div>}
    {storageError && <div className={styles.error} role="alert">{storageError}</div>}

    <footer className={styles.libraryBar}>
      <section className={styles.library} aria-labelledby="maker-saved"><h2 id="maker-saved">保存したキャラクター{saved.length > 0 && <span>{saved.length}</span>}</h2>{saved.length ? <div className={styles.savedList}>{saved.map(item => <button type="button" key={item.id} aria-label={item.name} aria-pressed={editing?.id === item.id} className={`${styles.savedCharacter} ${editing?.id === item.id ? styles.selected : ''}`} onClick={() => openSaved(item.id)}><span className={styles.savedColor} style={{ backgroundColor: item.config.hairColor }} /><span>{item.name}</span><small>{item.config.style === 'fantasy' ? 'ファンタジー' : 'アメコミ'}</small></button>)}</div> : <p>気に入った姿に名前を付けて、残しておこう。</p>}<small>このブラウザに保存されます。</small></section>
    </footer>
  </main>;
}
