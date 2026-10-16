'use client';

import dynamic from 'next/dynamic';
import { Component, useCallback, useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type ChangeEvent, type CSSProperties, type InputHTMLAttributes, type ReactNode, type Ref } from 'react';
import {
  ORIGINAL_FACE_CONTROLS, ORIGINAL_SIZE_RANGE, ORIGINAL_MOTIONS, editableOriginalDocument, ORIGINAL_HAIRSTYLES, ORIGINAL_HAIR_COLORS, originalHumanHairstyles, usesModularOriginal, makeOriginalDocument, neutralOriginalFace,
  currentOriginalDocument, currentOriginalHistory, resolveOriginalCharacter, sameOriginalDocument, validateOriginalDocument,
  type OriginalCharacter, type OriginalConfig, type OriginalDocument, type OriginalFace,
} from '../../lib/character-maker/original-config';
import {
  loadOriginalCharacters, loadOriginalDraft, resolveOriginalDraftOrigin, saveOriginalCharacter, saveOriginalDraft,
  type SavedOriginal,
} from '../../lib/character-maker/original-storage';
import { createEditHistory, commitEdit, endEditGroup, replaceEditPresent, undoEdit, redoEdit, type EditHistory } from '../../lib/character-maker/edit-history';
import { downloadFileName } from '../../lib/character-maker/config';
import { createDraftWriter } from '../../lib/character-maker/draft-writer';
import styles from './character-studio.module.css';
import { WeaponPicker, type WeaponPickerHandle } from './WeaponPicker';
import { requestWeaponForMotion, motionAfterWeaponChoice, type ArmedWeaponMotion } from '../../lib/character-maker/weapon-motion-intent';
import { selectedMakerWeaponId, type MakerWeaponId } from '../../lib/character-maker/weapon-catalog';
import { useCharacterReadTool } from './useCharacterReadTool';
import { defaultOriginalSpeciesParts, originalSpeciesChoices, type OriginalSpeciesParts } from '../../lib/character-maker/original-species';
import { StageBackdrop, StageOverlay } from './StudioStage';
import { StudioIcon, type StudioIconName } from './StudioIcon';
import { paintRangeFills } from '../../lib/character-maker/studio-palette';

const OriginalScene = dynamic(() => import('./OriginalCharacterScene'), { ssr: false });
type Notice = { error?: boolean; text: string; link?: { href: string; label: string } } | null;
type Pending = { kind: 'source'; character: OriginalCharacter } | { kind: 'saved'; entry: SavedOriginal } | { kind: 'import'; document: OriginalDocument };
type Category = 'body' | 'hair' | 'head' | 'ears' | 'eyes' | 'mouth' | 'color' | 'expression' | 'motion';

function message(error: unknown) { return error instanceof Error ? error.message : '操作を完了できませんでした。もう一度お試しください。'; }
function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob), link = window.document.createElement('a');
  link.href = url; link.download = name; window.document.body.appendChild(link); link.click(); link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}

class PreviewBoundary extends Component<{ children: ReactNode; resetKey: string; onError: (value: string) => void }, { error: boolean }> {
  state = { error: false };
  static getDerivedStateFromError() { return { error: true }; }
  componentDidCatch(error: Error) { this.props.onError(error.message); }
  componentDidUpdate(previous: Readonly<{ resetKey: string }>) { if (this.state.error && previous.resetKey !== this.props.resetKey) this.setState({ error: false }); }
  render() { return this.state.error ? <div className={styles.loading}>3Dを表示できませんでした。もう一度読み込んでください。</div> : this.props.children; }
}

function CategoryIcon({ category }: { category: Category }) {
  return <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {category === 'body' && <><circle cx="12" cy="4" r="2" /><path d="M7 8h10l2 6m-12-6-2 6m4-3v5l-2 6m8-11v5l2 6M9 16h6" /></>}
    {category === 'hair' && <><path d="M5 20V9a7 7 0 0 1 14 0v11M5 12c4 0 7-3 8-6 1 4 3 5 6 5" /><path d="M8 15c0 4 8 4 8 0" /></>}
    {category === 'head' && <><ellipse cx="12" cy="12" rx="7" ry="9" /><path d="M8 10h1m6 0h1m-6 6h4" /></>}
    {category === 'ears' && <><path d="m4 15-1-12 8 7m2 0 8-7-1 12M4 15c0 7 16 7 16 0" /></>}
    {category === 'eyes' && <><path d="M2 12c5-9 15-9 20 0-5 9-15 9-20 0Z" /><circle cx="12" cy="12" r="4" /></>}
    {category === 'mouth' && <><path d="M3 12c4-1 6-5 9-2 3-3 5 1 9 2-6 8-12 8-18 0Zm0 0h18" /></>}
    {category === 'color' && <><path d="M12 2C9 6 4 12 4 16a8 8 0 0 0 16 0c0-4-5-10-8-14Z" /><path d="M8 16c0 2 1 3 3 3" /></>}
    {category === 'expression' && <><circle cx="12" cy="12" r="9" /><path d="M7 9h1m8 0h1M7 14c2 4 8 4 10 0" /></>}
    {category === 'motion' && <><circle cx="14" cy="4" r="2" /><path d="m7 11 4-3 5 4 4 1M11 8l-1 7-6 6m6-6 5 2 2 5M3 8h3M2 12h3" /></>}
  </svg>;
}

export type OriginalEditorHandle = { captureSession: () => OriginalEditorSession | null };

export type OriginalEditorSession = {
  history: EditHistory<OriginalDocument>;
  baseline: OriginalDocument;
  editing: SavedOriginal | null;
  preview?: { category: Category; framing: 'full' | 'face'; view: 'front' | 'side' | 'back'; detailsOpen: boolean };
};

type Props = {
  characters: OriginalCharacter[];
  initialCharacterId: string;
  initialEntry?: SavedOriginal | null;
  initialSession?: OriginalEditorSession;
  onBackToList?: (session: OriginalEditorSession) => void;
  embedded?: boolean;
  editorRef?: Ref<OriginalEditorHandle>;
  /** ゲームへ渡す（yukikau.jp への組み込み）。あるときは「この姿で遊ぶ」を出し、yukikau.jp 用の .character.zip を渡す。docs/yukikau-route.md */
  onPlay?: (file: File) => void | Promise<void>;
  /** Studio の表紙の情報（page から渡す）。docs/studio-quality-spec.md §8 */
  accent?: string;
  look?: string;
  groupLabel?: string;
  poster?: string;
};

const MOTION_ICONS: Record<OriginalConfig['motion'], StudioIconName> = { idle: 'idle', walk: 'walk', run: 'run', sword_idle: 'guard', sword_attack: 'swing' };

export default function OriginalCharacterMaker({ characters, initialCharacterId, initialEntry, initialSession, onBackToList, embedded = false, editorRef, onPlay, accent, look = '', groupLabel = '', poster }: Props) {
  const initialCharacter = characters.find(item => item.id === initialCharacterId) ?? characters[0];
  const matchingSession = initialSession?.history.present.characterId === initialCharacter.id ? initialSession : undefined;
  const [history, setHistory] = useState(() => matchingSession ? currentOriginalHistory(matchingSession.history) : createEditHistory(initialEntry ? validateOriginalDocument(initialEntry) : makeOriginalDocument(initialCharacter)));
  const historyRef = useRef(history);
  const document = history.present;
  const weaponId = selectedMakerWeaponId(document.config);
  useCharacterReadTool(document);
  const [historyAnnouncement, setHistoryAnnouncement] = useState('');
  const interaction = useRef<{ key: string; token: string } | null>(null);
  const interactionSequence = useRef(0);
  const canUndo = history.past.length > 0, canRedo = history.future.length > 0;
  const [baseline, setBaseline] = useState(() => currentOriginalDocument(matchingSession?.baseline ?? document));
  const [saved, setSaved] = useState<SavedOriginal[]>([]), [editing, setEditing] = useState<SavedOriginal | null>(null);
  const [hydrated, setHydrated] = useState(false), [category, setCategory] = useState<Category>(matchingSession?.preview?.category ?? 'hair');
  const [framing, setFraming] = useState<'full' | 'face'>(matchingSession?.preview?.framing ?? 'full'), [view, setView] = useState<'front' | 'side' | 'back'>(matchingSession?.preview?.view ?? 'front');
  const [detailsOpen, setDetailsOpen] = useState(matchingSession?.preview?.detailsOpen ?? false);
  const [cameraCustom, setCameraCustom] = useState(false);
  const [cameraRevision, setCameraRevision] = useState(0), [modelRevision, setModelRevision] = useState(0);
  const [ready, setReady] = useState(false), [sceneError, setSceneError] = useState(''), [storageError, setStorageError] = useState('');
  const [notice, setNotice] = useState<Notice>(null);
  const [exporting, setExporting] = useState(false), [importing, setImporting] = useState(false);
  // yukikau.jp 用の .character.zip を作っている間（'download' は書き出し、'play' は「この姿で遊ぶ」）
  const [packaging, setPackaging] = useState<'download' | 'play' | null>(null);
  const [pendingWeaponMotion, setPendingWeaponMotion] = useState<ArmedWeaponMotion | null>(null);
  const weaponPicker = useRef<WeaponPickerHandle>(null);
  const [entered, setEntered] = useState(false), [saveDone, setSaveDone] = useState(false);
  const detailsButton = useRef<HTMLButtonElement>(null), refocusDetails = useRef(false), rootRef = useRef<HTMLElement>(null);
  // スライダーの塗りは値から毎回描く（ドラッグ中も React の再描画で追従する）。
  useLayoutEffect(() => paintRangeFills(rootRef.current));
  const [attackRevision, setAttackRevision] = useState(0);
  // のけぞり・＞＜ はその場の再生操作で、保存データには入れない（docs/main-character-spec.md）
  const [hitRevision, setHitRevision] = useState(0), [hitting, setHitting] = useState(false), [hurtFace, setHurtFace] = useState(false);
  const onHitComplete = useCallback(() => setHitting(false), []);
  const [seek, setSeek] = useState({ value: 0, revision: 0 }), [position, setPosition] = useState({ time: 0, duration: 1 });
  const file = useRef<HTMLInputElement>(null), name = useRef<HTMLInputElement>(null);
  const editingRef = useRef<SavedOriginal | null>(null);
  const hydratedRef = useRef(false);
  function setEditingIdentity(next: SavedOriginal | null) { editingRef.current = next; setEditing(next); }
  const [draftWriter] = useState(() => createDraftWriter({
    getSnapshot: () => {
      const latest = historyRef.current.present;
      if (!hydratedRef.current || !latest.name.trim()) return null;
      return { document: latest, savedId: editingRef.current?.id ?? null, savedUpdatedAt: editingRef.current?.updatedAt ?? null };
    },
    write: snapshot => saveOriginalDraft(window, snapshot),
    setTimer: (callback, delay) => window.setTimeout(callback, delay),
    clearTimer: timer => window.clearTimeout(timer),
    onError: error => { setStorageError(message(error)); },
  }));
  let character: OriginalCharacter | null = null, sourceError = '';
  try { character = resolveOriginalCharacter(document, characters); } catch (error) { sourceError = message(error); }
  const displayCharacter = character ?? characters.find(item => item.id === document.characterId) ?? characters[0];
  const config = document.config, dirty = !sameOriginalDocument(document, baseline);
  const isCat = displayCharacter.id === 'generic-cat', isRobot = displayCharacter.id === 'generic-robot';
  const species = isCat || isRobot;
  const modular = usesModularOriginal(config.appearance);
  const humanHair = originalHumanHairstyles(displayCharacter.id);
  const availableHair = humanHair.filter(item => displayCharacter.modularModel?.hairstyles.includes(item.id));
  const hairChoices = humanHair.length ? availableHair : ORIGINAL_HAIRSTYLES;
  const partThumbnailRevision = humanHair.length && displayCharacter.modularModel ? `?v=${displayCharacter.modularModel.sha256}` : '';
  const hairThumbnail = (hairstyle: string) => (displayCharacter.id === 'generic-human-b' && hairstyle === 'nagi-long-v2'
    ? '/character-maker/original-parts/generic-human-b/hair-nagi-long-v2-front-3ebd4576e87ca87e.jpg'
    : `/character-maker/original-parts/${displayCharacter.id}/hair-${hairstyle}.jpg`) + partThumbnailRevision;
  const parts = config.appearance.parts ?? defaultOriginalSpeciesParts();
  const partChoices = originalSpeciesChoices(displayCharacter.id);
  const titles: Record<Category, [string, string]> = {
    body: ['体型', 'すらりと、ふっくらと。あなたのバランス。'],
    hair: ['髪', '髪型と色で、新しい雰囲気に。'],
    head: [isRobot ? '頭の形' : '顔の形', isRobot ? 'まるく、きりっと。あなたらしい輪郭に。' : '輪郭に、少しの個性を。'],
    ears: ['耳', 'ぴんと、ふわりと。耳のかたちを選ぼう。'],
    eyes: ['目', 'まなざしの印象を、あなた好みに。'],
    mouth: ['口', '口もとのバランスを整えよう。'],
    color: [isCat ? '毛色' : 'カラー', isCat ? '毛並みと瞳を、好きな色に。' : '外装と光に、あなたの色を。'],
    expression: ['表情', 'まなざしと口もとに、気持ちを。'],
    motion: ['動き', '歩いて、走って、武器を振って。'],
  };
  const categories: Category[] = species
    ? ['body', 'head', ...(isCat ? ['ears' as const] : []), 'eyes', 'mouth', 'color', 'expression', 'motion']
    : ['body', 'hair', ...(modular ? ['head' as const, 'eyes' as const, 'mouth' as const] : []), 'expression', 'motion'];
  // A saved accessory remains editable; new species designs start with their own parts.
  if (species && config.appearance.hairstyle !== 'original') categories.splice(1, 0, 'hair');
  const hints: Record<Category, string> = { body: '大きさ・シルエット', hair: '髪型・髪色', head: '輪郭を選ぶ', ears: '耳のシルエット', eyes: '目の形を選ぶ', mouth: '口もとの印象', color: isCat ? '毛並み・瞳の色' : '外装・表示面の色', expression: '気持ちをまなざしに', motion: '動作・まばたき' };
  const activeCategory = categories.includes(category) ? category : 'body';
  const editorSection = activeCategory === 'expression' || activeCategory === 'motion' ? activeCategory : 'appearance';
  const appearanceCategories: Category[] = categories.filter(value => value !== 'expression' && value !== 'motion');
  const [title] = titles[activeCategory];

  function installHistory(next: EditHistory<OriginalDocument>) {
    next = currentOriginalHistory(next);
    historyRef.current = next;
    setHistory(next);
  }
  function replaceDocument(next: OriginalDocument) {
    setPendingWeaponMotion(null);
    interaction.current = null;
    installHistory(createEditHistory(next));
    setHistoryAnnouncement('');
  }
  function updateDocument(update: (previous: OriginalDocument) => OriginalDocument, key?: string) {
    const before = historyRef.current;
    const group = key && interaction.current?.key === key ? interaction.current.token : null;
    const next = commitEdit(before, update(before.present), sameOriginalDocument, group);
    installHistory(next);
    if (next !== before) draftWriter.changed();
    setHistoryAnnouncement('');
    setNotice(null);
  }
  function endInteraction() {
    interaction.current = null;
    installHistory(endEditGroup(historyRef.current));
  }
  function beginInteraction(key: string) {
    endInteraction();
    interaction.current = { key, token: `${key}:${++interactionSequence.current}` };
  }
  // One pointer gesture, held arrow key, color dialog, or name-focus session is
  // one undo step. Pausing during a drag does not split it into dozens of steps.
  function continuousInput(key: string, kind: 'range' | 'color' | 'text' = 'range'): Pick<InputHTMLAttributes<HTMLInputElement>, 'onFocus' | 'onBlur' | 'onPointerDown' | 'onPointerUp' | 'onPointerCancel' | 'onClick' | 'onKeyDown' | 'onKeyUp'> {
    const rangeKey = (value: string) => ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown', 'PageUp', 'PageDown', 'Home', 'End'].includes(value);
    return {
      onFocus: () => { if (interaction.current?.key !== key) beginInteraction(key); },
      onBlur: endInteraction,
      onPointerDown: () => beginInteraction(key),
      onPointerUp: kind === 'range' ? endInteraction : undefined,
      onPointerCancel: endInteraction,
      onClick: kind === 'color' ? () => beginInteraction(key) : undefined,
      onKeyDown: event => { if (kind === 'range' && rangeKey(event.key) && !event.repeat) beginInteraction(key); },
      onKeyUp: event => { if (kind === 'range' && rangeKey(event.key)) endInteraction(); },
    };
  }
  function moveHistory(direction: 'undo' | 'redo') {
    setPendingWeaponMotion(null);
    const before = historyRef.current;
    const next = direction === 'undo' ? undoEdit(before) : redoEdit(before);
    if (next === before) return;
    interaction.current = null;
    installHistory(next);
    draftWriter.changed();
    setNotice(null);
    if (next.present.config.motion === 'sword_attack') setAttackRevision(value => value + 1);
    setHistoryAnnouncement(direction === 'undo' ? '直前の変更を元に戻しました。' : '変更をやり直しました。');
  }
  function resetAppearance() {
    setPendingWeaponMotion(null);
    if (!character) return;
    const defaults = makeOriginalDocument(character).config;
    updateDocument(previous => ({ ...editableOriginalDocument(previous), config: defaults }));
    setSeek(previous => ({ value: 0, revision: previous.revision + 1 }));
    setView('front'); setFraming('full'); setCameraCustom(false); setCameraRevision(value => value + 1);
  }

  useEffect(() => {
    const listener = (event: KeyboardEvent) => {
      if (!hydrated || event.defaultPrevented || event.isComposing || event.altKey || !(event.ctrlKey || event.metaKey)) return;
      const key = event.key.toLowerCase();
      if (key !== 'z' && key !== 'y') return;
      const target = event.target;
      if (target instanceof HTMLElement) {
        if (target.isContentEditable) return;
        const editor = target.closest('input, textarea, select, [contenteditable]');
        if (editor && !(editor instanceof HTMLInputElement && editor.type === 'range')) return;
      }
      const direction = key === 'y' || event.shiftKey ? 'redo' : 'undo';
      const available = direction === 'undo' ? historyRef.current.past.length : historyRef.current.future.length;
      if (!available) return;
      event.preventDefault();
      moveHistory(direction);
    };
    window.addEventListener('keydown', listener);
    return () => window.removeEventListener('keydown', listener);
  }, [hydrated]);

  // 描画前に控えを読み、キャラ切替のたびに空の画面が1コマ出ないようにする。
  useLayoutEffect(() => {
    let entries: SavedOriginal[] = [];
    try { const loaded = loadOriginalCharacters(window.localStorage); entries = loaded.characters; setSaved(entries); if (loaded.unreadable) setStorageError(`${loaded.unreadable}件の保存データを読み込めませんでした。読み込めるキャラクターは一覧に表示しています。`); }
    catch (error) { setStorageError(message(error)); }
    try {
      // An explicit gallery selection must never be replaced by another
      // character's global draft. In-memory sessions preserve per-character
      // edits (and undo history) while moving between the list and editor.
      if (matchingSession) {
        const previous = matchingSession.editing;
        const origin = resolveOriginalDraftOrigin({ document: matchingSession.history.present, savedId: previous?.id ?? null, savedUpdatedAt: previous?.updatedAt ?? null }, entries);
        setEditingIdentity(origin.editing);
        if (origin.warning) setStorageError(origin.warning);
      } else if (initialEntry) {
        const origin = resolveOriginalDraftOrigin({ document: validateOriginalDocument(initialEntry), savedId: initialEntry.id, savedUpdatedAt: initialEntry.updatedAt }, entries);
        setEditingIdentity(origin.editing);
        if (origin.warning) setStorageError(origin.warning);
      } else {
        const loaded = loadOriginalDraft(window, initialCharacter.id);
        if (loaded.warning) setStorageError(loaded.warning);
        if (loaded.draft?.document.characterId === initialCharacter.id) {
          replaceDocument(loaded.draft.document);
          const origin = resolveOriginalDraftOrigin(loaded.draft, entries);
          setEditingIdentity(origin.editing);
          if (origin.editing) setBaseline(validateOriginalDocument(origin.editing));
          if (origin.warning) setStorageError(origin.warning);
        }
      }
    } catch (error) { setStorageError(message(error)); }
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
    const listener = () => { try { setSaved(loadOriginalCharacters(window.localStorage).characters); } catch (error) { setStorageError(message(error)); } };
    window.addEventListener('storage', listener); return () => window.removeEventListener('storage', listener);
  }, []);

  // 幅1100px以上は細部を開いて始める。セッションの開閉が残っていれば、そちらを使う（§3.1）。
  useLayoutEffect(() => {
    if (embedded && !matchingSession?.preview && window.matchMedia('(min-width: 1100px)').matches) setDetailsOpen(true);
  }, []);
  useEffect(() => { if (ready) setEntered(true); }, [ready]);
  useEffect(() => {
    if (detailsOpen || !refocusDetails.current) return;
    refocusDetails.current = false;
    detailsButton.current?.focus();
  }, [detailsOpen]);
  // M5: 保存できたら保存ボタンに✓を1.6秒出す。成功の通知は4秒で消え、エラーは残る。
  useEffect(() => {
    if (!notice || notice.error || notice.link) return;
    if (notice.text.endsWith('を保存しました。')) setSaveDone(true);
    const hide = window.setTimeout(() => setNotice(null), 4000);
    return () => window.clearTimeout(hide);
  }, [notice]);
  useEffect(() => {
    if (!saveDone) return;
    const done = window.setTimeout(() => setSaveDone(false), 1600);
    return () => window.clearTimeout(done);
  }, [saveDone]);

  const onReady = useCallback((value: boolean) => { setReady(value); if (value) setSceneError(''); else setCameraCustom(false); }, []);
  const onError = useCallback((value: string) => { setReady(false); setSceneError(value); }, []);
  // 埋め込み（Studio）では、読み込みの失敗はシーン内の板だけで知らせる（I-1・I-2）。下のバーは描画の例外（PreviewBoundary）だけに使う。
  const onSceneLoadError = useCallback(() => setReady(false), []);
  const report = useCallback((time: number, duration: number) => setPosition({ time, duration }), []);

  function change(values: Partial<OriginalConfig>, key?: string) {
    updateDocument(previous => {
      const next = editableOriginalDocument(previous);
      return { ...next, config: { ...next.config, ...values, appearance: values.appearance ? { ...values.appearance, version: next.config.appearance.version, parts: values.appearance.parts ?? next.config.appearance.parts } : next.config.appearance } };
    }, key);
  }
  function chooseCategory(value: Category) { if (embedded) setDetailsOpen(true); endInteraction(); setCategory(value); setFraming(['expression', 'hair', 'head', 'ears', 'eyes', 'mouth', 'color'].includes(value) ? 'face' : 'full'); setCameraCustom(false); setCameraRevision(revision => revision + 1); }
  function appearance(values: Partial<OriginalConfig['appearance']>, key?: string) { change({ appearance: { ...historyRef.current.present.config.appearance, ...values } }, key); }
  function changeParts(values: Partial<OriginalSpeciesParts>, key?: string) { appearance({ parts: { ...(historyRef.current.present.config.appearance.parts ?? defaultOriginalSpeciesParts()), ...values } }, key); }
  function chooseWeapon(value: MakerWeaponId) {
    const nextMotion = motionAfterWeaponChoice(value, pendingWeaponMotion);
    setPendingWeaponMotion(null);
    change({ weaponId: value, motion: nextMotion, playing: true });
    if (nextMotion === 'sword_attack') setAttackRevision(revision => revision + 1);
    setFraming('full'); setView('front'); setCameraCustom(false); setCameraRevision(revision => revision + 1);
  }
  function chooseMotion(value: OriginalConfig['motion']) {
    const request = requestWeaponForMotion(value, weaponId);
    setPendingWeaponMotion(request);
    if (request) { weaponPicker.current?.focusChoices(); return; }
    // Ready and swing share one view; replaying a motion must not reset an orbit.
    const keepCamera = framing === 'full' && (value === config.motion || (value.startsWith('sword_') && config.motion.startsWith('sword_')));
    change({ motion: value, playing: true });
    if (value === 'sword_attack') setAttackRevision(revision => revision + 1);
    setFraming('full');
    if (!keepCamera) { setCameraCustom(false); setCameraRevision(revision => revision + 1); }
  }
  const onAttackComplete = useCallback(() => {
    const before = historyRef.current;
    if (before.present.config.motion !== 'sword_attack') return;
    installHistory(replaceEditPresent(before, { ...before.present, config: { ...before.present.config, motion: 'sword_idle' } }));
    draftWriter.changed();
  }, []);
  function facePreset(values: Partial<OriginalFace>) { change({ face: { ...neutralOriginalFace(displayCharacter), ...values }, speaking: false, autoBlink: false }); }
  function captureSession(): OriginalEditorSession | null {
    if (!hydratedRef.current) return null;
    if (importing || exporting) {
      setNotice({ error: true, text: '読み込み・書き出しが終わってから、キャラクターを切り替えてください。' });
      return null;
    }
    endInteraction();
    // Also migrate a restored legacy global draft before leaving this character.
    draftWriter.changed();
    draftWriter.flush();
    // A failed or incomplete draft must stay mounted, including its undo history.
    if (draftWriter.hasPending()) {
      setNotice({ error: true, text: !historyRef.current.present.name.trim() ? '切り替える前に名前を入力してください。' : '編集内容の控えを保存できませんでした。保存またはJSONの書き出しを確認してから、もう一度お試しください。' });
      if (!historyRef.current.present.name.trim()) name.current?.focus();
      return null;
    }
    return { history: historyRef.current, baseline, editing: editingRef.current, preview: { category, framing, view, detailsOpen } };
  }
  useImperativeHandle(editorRef, () => ({ captureSession }));
  function returnToList() {
    const session = captureSession();
    if (session) onBackToList?.(session);
  }
  function save(asNew = false) {
    try {
      const clean = validateOriginalDocument(historyRef.current.present), entry = saveOriginalCharacter(window.localStorage, clean, asNew ? null : editingRef.current);
      // A new saved identity gets its own undo scope; ordinary saves retain
      // edits but never put the saved record's revision into document history.
      if (entry.id !== editingRef.current?.id) replaceDocument(clean);
      else { endInteraction(); installHistory(replaceEditPresent(historyRef.current, clean)); }
      setBaseline(clean); setEditingIdentity(entry);
      setSaved(previous => [entry, ...previous.filter(item => item.id !== entry.id)].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt)));
      setStorageError(''); setNotice({ text: `「${entry.name}」を保存しました。` });
      try { saveOriginalDraft(window, { document: clean, savedId: entry.id, savedUpdatedAt: entry.updatedAt }); draftWriter.acceptPersisted(); }
      catch (error) { draftWriter.changed(); setStorageError(message(error)); }
    } catch (error) { setNotice({ error: true, text: message(error) }); if (!document.name.trim()) name.current?.focus(); }
  }
  function apply(next: Pending) {
    const clean = next.kind === 'source' ? makeOriginalDocument(next.character) : next.kind === 'saved' ? validateOriginalDocument(next.entry) : next.document;
    const entry = next.kind === 'saved' ? next.entry : null;
    let persisted = false;
    try { saveOriginalDraft(window, { document: clean, savedId: entry?.id ?? null, savedUpdatedAt: entry?.updatedAt ?? null }); persisted = true; }
    catch (error) { setStorageError(message(error)); }
    replaceDocument(clean); setBaseline(next.kind === 'import' ? makeOriginalDocument(displayCharacter) : clean); setEditingIdentity(entry);
    if (persisted) draftWriter.acceptPersisted(); else draftWriter.changed();
    setNotice(next.kind === 'import' ? { text: `「${clean.name}」を読み込みました。` } : null);
    setSceneError(''); setReady(false); setModelRevision(value => value + 1); setView('front'); setFraming('full'); setCameraCustom(false); setCameraRevision(value => value + 1);
    setSeek(previous => ({ value: 0, revision: previous.revision + 1 }));
  }
  function request(next: Pending) { apply(next); }
  function exportJson() {
    try { const clean = validateOriginalDocument(document); download(new Blob([JSON.stringify(clean, null, 2) + '\n'], { type: 'application/json' }), `${downloadFileName(clean.name)}.character.json`); setNotice({ text: '設定JSONを書き出しました。' }); }
    catch (error) { setNotice({ error: true, text: message(error) }); }
  }
  async function importJson(event: ChangeEvent<HTMLInputElement>) {
    const selected = event.target.files?.[0]; event.target.value = ''; if (!selected) return;
    setImporting(true);
    try { request({ kind: 'import', document: validateOriginalDocument(JSON.parse(await selected.text())) }); }
    catch (error) { setNotice({ error: true, text: error instanceof SyntaxError ? 'JSONを読み込めませんでした。オリジナルの設定JSONを選んでください。' : message(error) }); }
    finally { setImporting(false); }
  }
  async function exportGlb() {
    if (!character || !ready || exporting) return;
    setExporting(true);
    try {
      const clean = validateOriginalDocument(document), { exportOriginalCharacterGlb } = await import('../../lib/character-maker/original-assembly');
      const armed = ['sword_idle', 'sword_attack'].includes(clean.config.motion);
      const bytes = await exportOriginalCharacterGlb(character, clean);
      download(new Blob([bytes], { type: 'model/gltf-binary' }), `${downloadFileName(clean.name)}-${armed ? 'armed' : 'standard'}.glb`); setNotice({ text: `${armed ? '武器用（2動作）' : '通常用（3動作）'}のGLBを書き出しました。` });
    } catch (error) { setNotice({ error: true, text: message(error) }); }
    finally { setExporting(false); }
  }
  /** yukikau.jp 用（.character.zip）。'download' は保存して持ち込みへのリンクを出し、'play' は onPlay へ渡す。docs/yukikau-route.md */
  async function exportYukikau(kind: 'download' | 'play') {
    if (!character || !ready || exporting) return;
    setExporting(true); setPackaging(kind);
    try {
      const clean = validateOriginalDocument(document);
      const { exportOriginalYukikauPackage, downloadYukikauPackage, YUKIKAU_IMPORT_URL } = await import('../../lib/character-maker/yukikau-package');
      const file = await exportOriginalYukikauPackage(character, clean, rootRef.current?.querySelector<HTMLCanvasElement>('#studio-stage canvas'));
      if (kind === 'play' && onPlay) { await onPlay(file); setNotice({ text: 'この姿で遊べます。' }); }
      else { downloadYukikauPackage(file); setNotice({ text: 'yukikau.jp 用のファイルを書き出しました。', link: { href: YUKIKAU_IMPORT_URL, label: 'yukikau.jp で読み込む' } }); }
    } catch (error) { setNotice({ error: true, text: message(error) }); }
    finally { setExporting(false); setPackaging(null); }
  }

  if (!hydrated) return <section className={`${styles.shell} ${embedded ? styles.liveEditor : ''} ${styles.editorLoading}`} aria-busy="true"><span className={styles.srOnly} role="status">編集内容を確認しています…</span></section>;

  const savedHere = saved.filter(item => item.characterId === displayCharacter.id);
  const motionChoices = ORIGINAL_MOTIONS.filter(([value]) => displayCharacter.motions.includes(value));
  const toggleFraming = () => { setFraming(value => value === 'full' ? 'face' : 'full'); setCameraCustom(false); setCameraRevision(revision => revision + 1); };

  return <section ref={rootRef} className={`${styles.shell} ${styles.studio} ${embedded ? styles.liveEditor : ''}`} data-style="original" data-details-open={detailsOpen || !embedded}>
    <header className={styles.bar}>
      {!embedded && <button type="button" className={styles.backLink} onClick={returnToList}>一覧に戻る</button>}
      <div className={styles.historyControls} role="group" aria-label="編集履歴">
        <button type="button" className={styles.iconButton} disabled={!canUndo} aria-keyshortcuts="Control+Z Meta+Z" title="元に戻す (Ctrl/⌘+Z)" onClick={() => moveHistory('undo')}><StudioIcon name="undo" /><span className={styles.srOnly}>元に戻す</span></button>
        <button type="button" className={styles.iconButton} disabled={!canRedo} aria-keyshortcuts="Control+Shift+Z Meta+Shift+Z Control+Y" title="やり直す (Ctrl/⌘+Shift+Z)" onClick={() => moveHistory('redo')}><StudioIcon name="redo" /><span className={styles.srOnly}>やり直す</span></button>
        <button type="button" className={styles.iconButton} disabled={!character} title="見た目と再生設定を初期状態に戻す。元に戻す操作で取り消せます。" onClick={resetAppearance}><StudioIcon name="reset" /><span className={styles.srOnly}>リセット</span></button>
      </div>
      <span className={styles.historyStatus} aria-live="polite">{dirty && <span className={styles.dirtyChip}>未保存</span>}</span>
      <span className={styles.srOnly} aria-live="polite">{historyAnnouncement}</span>
      <div className={styles.saveArea}>
        {onPlay && <button type="button" className={styles.primary} disabled={!character || !ready || exporting} onClick={() => exportYukikau('play')}>{packaging === 'play' ? '姿を作っています…' : 'この姿で遊ぶ'}</button>}
        <button type="button" className={styles.primary} data-done={saveDone} onClick={() => save()}><span className={styles.primaryCheck} aria-hidden="true"><StudioIcon name="check" size={18} /></span>保存する</button>
        <details className={styles.workspaceMenu} data-menu><summary><span className={styles.menuLabel}>書き出し</span><StudioIcon name="download" /></summary><div className={styles.menuPanel} data-menu-panel role="group" aria-label="設定とモデルの書き出し">
          <strong>ファイルに書き出す</strong>
          <button type="button" aria-describedby="original-glb-help" disabled={!character || !ready || exporting} onClick={exportGlb}>{exporting && !packaging ? '書き出し中…' : '3Dモデル（GLB）'}</button>
          <button type="button" onClick={exportJson}>編集設定（JSON）</button>
          <button type="button" disabled={!character || !ready || exporting} onClick={() => exportYukikau('download')}>{packaging === 'download' ? '書き出し中…' : 'yukikau.jp 用（.character.zip）'}</button>
          <small id="original-glb-help">{['sword_idle', 'sword_attack'].includes(config.motion) ? 'GLB：選んだ武器と、構える・振るの2動作。' : 'GLB：待機・歩く・走るの3動作。'}{embedded && 'のけぞりと＞＜の顔も入ります。'} JSONには編集・再生設定が入り、読み込み後も5種類の動作を選べます。</small>
        </div></details>
        <details className={`${styles.workspaceMenu} ${styles.moreMenu}`} data-menu><summary aria-label="その他の操作"><StudioIcon name="more" /><span className={styles.srOnly}>•••</span></summary><div className={styles.menuPanel} data-menu-panel>
          <button type="button" onClick={() => save(true)}>別のキャラクターとして保存</button>
          <button type="button" disabled={importing} onClick={() => file.current?.click()}>{importing ? '読み込み中…' : 'JSONを読み込む'}</button>
        </div></details>
      </div>
      <input ref={file} type="file" accept=".json,application/json" aria-label="キャラクター設定JSON" className={styles.fileInput} onChange={importJson} />
    </header>
    <section className={styles.stage} id="studio-stage" aria-label="キャラクターの3Dプレビュー" data-entered={entered} data-view={view} data-framing={framing}>
      <StageBackdrop look={look} />
      <div className={styles.canvasSlot} data-entered={entered}>
        {character ? <PreviewBoundary resetKey={`${character.id}-${modelRevision}`} onError={onError}><OriginalScene studio={embedded} character={character} config={config} framing={framing} view={view} cameraRevision={cameraRevision} onCameraInteraction={() => setCameraCustom(true)} modelRevision={modelRevision} onReady={onReady} onError={embedded ? onSceneLoadError : onError} seek={seek} report={report} attackRevision={attackRevision} onAttackComplete={onAttackComplete} hitRevision={hitRevision} onHitComplete={onHitComplete} hurtFace={embedded && hurtFace} accent={accent} onFramingToggle={toggleFraming} /></PreviewBoundary> : <div className={styles.originalMissing} role="status"><strong>保存した姿を表示できません</strong><p>{sourceError}</p>{!embedded && <button type="button" className={styles.secondary} onClick={returnToList}>一覧に戻る</button>}</div>}
      </div>
      <StageOverlay poster={poster} entered={entered} weaponId={weaponId} view={view} framing={framing} />
      <div className={styles.nameBlock}>
        {(groupLabel || dirty) && <small className={styles.nameKicker}>{groupLabel}{dirty && <span className={styles.stageDirty} aria-hidden="true">未保存</span>}</small>}
        <label className={styles.nameField}><span className={styles.srOnly}>名前</span><input ref={name} aria-label="キャラクターの名前" value={document.name} placeholder={displayCharacter.name} {...continuousInput('name', 'text')} onChange={event => updateDocument(previous => ({ ...previous, name: event.target.value }), 'name')} /><StudioIcon name="pencil" size={18} className={styles.namePencil} /></label>
      </div>
      <div className={styles.cameraPill}>
        <div className={styles.pillGroup} role="group" aria-label="表示範囲">{([['full', '全身'], ['face', '顔']] as const).map(([value, label]) => <button type="button" key={value} aria-pressed={!cameraCustom && framing === value} onClick={() => { setFraming(value); setCameraCustom(false); setCameraRevision(revision => revision + 1); }}>{label}</button>)}</div>
        <i className={styles.pillDivider} aria-hidden="true" />
        <div className={styles.pillGroup} role="group" aria-label="見る方向">{([['front', '正面'], ['side', '横'], ['back', '後ろ']] as const).map(([value, label]) => <button type="button" key={value} aria-pressed={!cameraCustom && view === value} onClick={() => { setView(value); setCameraCustom(false); setCameraRevision(revision => revision + 1); }}>{label}</button>)}</div>
      </div>
      <WeaponPicker value={weaponId} onChange={chooseWeapon} disabled={!character} pickerRef={weaponPicker} requestedMotion={pendingWeaponMotion} onCancelRequest={() => setPendingWeaponMotion(null)} />
      <div className={styles.stageAlert}>
        {sceneError && <div className={styles.error} role="alert"><span>{sceneError}</span><button type="button" className={styles.retryButton} onClick={() => { setSceneError(''); setReady(false); setModelRevision(value => value + 1); }}>もう一度読み込む</button></div>}
      </div>
    </section>
    {/* 通知はステージに重ねるが、3Dプレビューの section の外に置く（section 内の alert は描画エラーだけ）。 */}
    <div className={styles.toasts}>
        {notice && <div className={notice.error ? styles.error : styles.success} role={notice.error ? 'alert' : 'status'}>{!notice.error && <StudioIcon name="check" size={18} />}<span>{notice.text}{notice.link && <> <a href={notice.link.href} target="_blank" rel="noopener">{notice.link.label}</a></>}</span><button type="button" aria-label="通知を閉じる" onClick={() => setNotice(null)}><StudioIcon name="close" size={16} /></button></div>}
        {storageError && <div className={styles.error} role="alert"><span>{storageError}</span></div>}
    </div>
    <div className={styles.rail}>
      {embedded && <section id="studio-edit" tabIndex={-1} className={styles.commonControls} aria-label="共通のカスタマイズ">
        <div className={styles.commonSliders}>
          <label className={styles.slider} style={{ '--p': `${((config.size - ORIGINAL_SIZE_RANGE.min) / (ORIGINAL_SIZE_RANGE.max - ORIGINAL_SIZE_RANGE.min)) * 100}%`, '--d': `${((1 - ORIGINAL_SIZE_RANGE.min) / (ORIGINAL_SIZE_RANGE.max - ORIGINAL_SIZE_RANGE.min)) * 100}%` } as CSSProperties}><span>大きさ<output>{Math.round(config.size * 100)}%</output></span><input aria-label="大きさ" type="range" {...ORIGINAL_SIZE_RANGE} value={config.size} {...continuousInput('size')} onChange={event => change({ size: Number(event.target.value) }, 'size')} /></label>
          <label className={styles.slider} style={{ '--p': `${((config.appearance.build + 1) / 2) * 100}%`, '--d': '50%' } as CSSProperties}><span>体型<output data-text={config.appearance.build === 0 || undefined}>{config.appearance.build === 0 ? '標準' : `${config.appearance.build > 0 ? '+' : '−'}${Math.abs(Math.round(config.appearance.build * 100))}`}</output></span><input aria-label="体型" type="range" min={-1} max={1} step={0.05} value={config.appearance.build} {...continuousInput('build')} onChange={event => appearance({ build: Number(event.target.value) }, 'build')} /></label>
        </div>
        <div className={styles.commonMotion}>
          <div className={styles.motionHeading}><span id="original-common-motion">動き</span><button type="button" className={styles.playToggle} aria-label={config.playing ? '動作を一時停止' : '動作を再生'} aria-pressed={config.playing} onClick={() => change({ playing: !config.playing })}><StudioIcon name={config.playing ? 'pause' : 'play'} /></button></div>
          <div className={styles.motionGrid} role="group" aria-labelledby="original-common-motion" style={{ '--n': motionChoices.length + 1 } as CSSProperties}>{motionChoices.map(([value, label]) => <button type="button" key={value} aria-label={label} title={label} aria-pressed={(pendingWeaponMotion ?? config.motion) === value} onClick={() => chooseMotion(value)}><StudioIcon name={MOTION_ICONS[value]} /><span>{label.replace('武器を', '')}</span></button>)}<button type="button" aria-label="のけぞる" title="のけぞる" data-hit-playing={hitting || undefined} onClick={() => { if (!ready) return; setHitting(true); setHitRevision(value => value + 1); }}><StudioIcon name="hit" /><span>のけぞる</span></button></div>
        </div>
        {!detailsOpen && <button ref={detailsButton} type="button" className={styles.detailsToggle} aria-expanded={false} aria-controls="original-details" onClick={() => chooseCategory(activeCategory)}><StudioIcon name="plus" />細かくカスタマイズ</button>}
      </section>}
      {embedded && !detailsOpen && <div className={styles.shortcutGrid}>{categories.map(value => <button type="button" key={value} aria-label={`${titles[value][0]}を編集`} title={hints[value]} onClick={() => chooseCategory(value)}><CategoryIcon category={value} /><span>{titles[value][0]}</span></button>)}</div>}
      {(!embedded || detailsOpen) && <aside id="original-details" className={styles.inspector} aria-label="カスタマイズ項目">
        {embedded && <div className={styles.detailHeading}><strong>細かくカスタマイズ</strong><button type="button" className={styles.iconButton} aria-label="詳細を閉じる" onClick={() => { refocusDetails.current = true; setDetailsOpen(false); }}><StudioIcon name="close" /></button></div>}
        <nav className={styles.sectionTabs} aria-label="編集する内容">
          {([['appearance', '見た目', 'hair'], ['expression', '表情', 'expression'], ['motion', '動き', 'motion']] as const).map(([section, label, next]) => <button type="button" key={section} aria-pressed={editorSection === section} onClick={() => chooseCategory(section === 'appearance' ? (appearanceCategories.includes(category) ? category : appearanceCategories.includes('hair') ? 'hair' : 'body') : next)}><CategoryIcon category={next} />{label}</button>)}
        </nav>
        {editorSection === 'appearance' && <nav className={styles.categories} aria-label="見た目の項目">{appearanceCategories.map(value => <button type="button" key={value} title={hints[value]} aria-pressed={activeCategory === value} className={activeCategory === value ? styles.activeCategory : ''} onClick={() => chooseCategory(value)}><CategoryIcon category={value} />{titles[value][0]}</button>)}</nav>}
        <section className={styles.editor} aria-labelledby="original-category">
          <div className={styles.editorHeading}><h2 id="original-category">{title}</h2></div>
        {activeCategory === 'body' && <div className={styles.originalOptions}>
          <label className={styles.slider}><span>大きさ<output>{Math.round(config.size * 100)}%</output></span><input aria-label="大きさ" type="range" {...ORIGINAL_SIZE_RANGE} value={config.size} {...continuousInput('size')} onChange={event => change({ size: Number(event.target.value) }, 'size')} /><small><span>小さめ</span><span>大きめ</span></small></label>
          <label className={styles.slider}><span>体型<output data-text={config.appearance.build === 0 || undefined}>{config.appearance.build === 0 ? '標準' : `${config.appearance.build > 0 ? '+' : '−'}${Math.abs(Math.round(config.appearance.build * 100))}`}</output></span><input aria-label="体型" type="range" min={-1} max={1} step={0.05} value={config.appearance.build} {...continuousInput('build')} onChange={event => appearance({ build: Number(event.target.value) }, 'build')} /><small><span>すらり</span><span>ふっくら</span></small></label>
          <div className={styles.expressionButtons} role="group" aria-label="体型の見本">{([[-1, 'すらり'], [0, '標準'], [1, 'ふっくら']] as const).map(([value, label]) => <button type="button" key={value} aria-pressed={config.appearance.build === value} onClick={() => appearance({ build: value })}>{label}</button>)}</div>
          <button type="button" className={styles.resetButton} onClick={() => { change({ size: 1, appearance: { ...config.appearance, build: 0 } }); }}>体型と大きさを元に戻す</button>
        </div>}
        {activeCategory === 'hair' && <div className={styles.originalOptions}>
          <div className={styles.originalHairChoices} role="group" aria-label="髪型">{hairChoices.map(item => <button type="button" key={item.id} aria-pressed={config.appearance.hairstyle === item.id} className={config.appearance.hairstyle === item.id ? styles.selected : ''} onClick={() => appearance({ hairstyle: item.id })}><>{availableHair.some(hair => hair.id === item.id) ? <img className={styles.originalPartThumb} src={hairThumbnail(item.id)} alt="" width={76} height={76} /> : <span className={styles.originalHairIcon} data-hair={item.id} aria-hidden="true"><CategoryIcon category="hair" />{item.id === 'ponytail' && <i />}</span>}</><span><strong>{item.label}</strong><small>{item.description}</small></span><b aria-hidden="true">{config.appearance.hairstyle === item.id ? '✓' : '+'}</b></button>)}</div>
          <fieldset className={styles.originalFieldset}><legend>髪色</legend><div className={styles.colorOptions}>
            <button type="button" className={styles.resetButton} aria-pressed={config.appearance.hairColor === 'source'} onClick={() => appearance({ hairColor: 'source' })}>元の髪色</button>
            <div className={styles.swatches} role="group" aria-label="髪色の見本">{ORIGINAL_HAIR_COLORS.map(([color, label]) => <button type="button" key={color} aria-label={label} aria-pressed={config.appearance.hairColor === color} title={label} className={config.appearance.hairColor === color ? styles.selectedSwatch : ''} style={{ backgroundColor: color }} onClick={() => appearance({ hairColor: color })}>{config.appearance.hairColor === color && <span aria-hidden="true">✓</span>}</button>)}</div>
            <label className={styles.customColor}><span>好きな色を選ぶ<small>あなたらしい色を、自由に。</small></span><input type="color" aria-label="髪色を自由に選ぶ" value={config.appearance.hairColor === 'source' ? '#614439' : config.appearance.hairColor} {...continuousInput('hairColor', 'color')} onChange={event => appearance({ hairColor: event.target.value }, 'hairColor')} /></label>
          </div></fieldset>
          {['generic-robot', 'generic-cat'].includes(displayCharacter.id) && config.appearance.hairstyle === 'original' && <p className={styles.help}>髪型を選ぶと、髪色が反映されます。</p>}
          <button type="button" className={styles.resetButton} onClick={() => appearance({ hairstyle: availableHair[0]?.id ?? 'original', hairColor: 'source' })}>髪型と色を標準に戻す</button>
        </div>}
        {(species || modular) && (['head', 'ears', 'eyes', 'mouth'] as Category[]).includes(activeCategory) && <div className={styles.originalOptions}>
          <div className={styles.originalHairChoices} role="group" aria-label={title}>{partChoices[activeCategory as 'head' | 'ears' | 'eyes' | 'mouth'].map(item => <button type="button" key={item.id} aria-pressed={parts[activeCategory as 'head' | 'ears' | 'eyes' | 'mouth'] === item.id} className={parts[activeCategory as 'head' | 'ears' | 'eyes' | 'mouth'] === item.id ? styles.selected : ''} onClick={() => changeParts({ [activeCategory]: item.id })}>{species || modular ? <img className={styles.originalPartThumb} src={`/character-maker/original-parts/${displayCharacter.id}/${activeCategory}-${item.id}.jpg${partThumbnailRevision}`} alt="" width={76} height={76} /> : <span className={styles.originalHairIcon}><CategoryIcon category={activeCategory} /></span>}<strong>{item.label}</strong><b aria-hidden="true">{parts[activeCategory as 'head' | 'ears' | 'eyes' | 'mouth'] === item.id ? '✓' : '+'}</b></button>)}</div>
          <button type="button" className={styles.resetButton} onClick={() => changeParts({ [activeCategory]: 'original' })}>いつもの形に戻す</button>
        </div>}
        {species && activeCategory === 'color' && <div className={styles.originalOptions}>
          {(['coatColor', 'accentColor', ...(isCat ? ['eyeColor' as const] : [])] as const).map(key => {
            const label = key === 'coatColor' ? (isCat ? '主な毛色' : '外装の色') : key === 'accentColor' ? (isCat ? '模様の色' : '表示面の色') : '瞳の色';
            const colors = key === 'eyeColor' ? [['#9e682d', '琥珀'], ['#5b9b73', '緑'], ['#589bc2', '青'], ['#a284b4', 'すみれ']] : [['#bb814d', 'キャラメル'], ['#e1c599', 'クリーム'], ['#63524b', 'チョコレート'], ['#9099a6', 'シルバー'], ['#b87992', 'ローズ'], ['#6c9d91', 'ミント']];
            return <fieldset key={key} className={styles.originalFieldset}><legend>{label}</legend><div className={styles.colorOptions}><button type="button" className={styles.resetButton} aria-pressed={parts[key] === 'source'} onClick={() => changeParts({ [key]: 'source' })}>元の{label}</button><div className={styles.swatches} role="group" aria-label={`${label}の見本`}>{colors.map(([color, name]) => <button key={color} type="button" aria-label={`${label}・${name}`} title={name} aria-pressed={parts[key] === color} className={parts[key] === color ? styles.selectedSwatch : ''} style={{ backgroundColor: color }} onClick={() => changeParts({ [key]: color })}>{parts[key] === color && <span aria-hidden="true">✓</span>}</button>)}</div><label className={styles.customColor}><span>好きな色を選ぶ</span><input type="color" aria-label={`${label}を自由に選ぶ`} value={parts[key] === 'source' ? colors[0][0] : parts[key]} {...continuousInput(`color:${key}`, 'color')} onChange={event => changeParts({ [key]: event.target.value }, `color:${key}`)} /></label></div></fieldset>;
          })}
          <button type="button" className={styles.resetButton} onClick={() => changeParts({ coatColor: 'source', accentColor: 'source', eyeColor: 'source' })}>いつもの色に戻す</button>
        </div>}
        {activeCategory === 'expression' && <div className={styles.originalOptions}>
          <div className={styles.expressionButtons} role="group" aria-label="表情"><button type="button" onClick={() => facePreset({})}>ふだんの表情</button><button type="button" onClick={() => facePreset(['kiyogon', 'pyonmi'].includes(displayCharacter.id) ? { happy: 1 } : { ee: 0.65, mouthClose: 0.35, happy: 0.3 })}>にっこり</button><button type="button" onClick={() => facePreset({ blinkLeft: 1, blinkRight: 1, mouthClose: 1 })}>目と口を閉じる</button>{embedded && <button type="button" aria-label="＞＜の顔" title="＞＜の顔" aria-pressed={hurtFace} onClick={() => setHurtFace(value => !value)}><span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}><StudioIcon name="hurtFace" size={16} />＞＜</span></button>}</div>
          <fieldset className={styles.originalFieldset}><legend>口の動き</legend><div className={styles.expressionButtons}>{([['aa', 'あ'], ['ee', 'い'], ['ou', 'う・お']] as const).filter(([key]) => displayCharacter.expressions.includes(key)).map(([key, label]) => <button type="button" key={key} onClick={() => facePreset({ [key]: 1, mouthClose: 0 })}>{label}</button>)}</div></fieldset>
          <details className={styles.expressionDetails}><summary>目と口を細かく整える</summary><div className={styles.originalFine}>{ORIGINAL_FACE_CONTROLS.filter(([key]) => displayCharacter.expressions.includes(key)).map(([key, label]) => <label key={key} className={styles.slider}><span>{label}<output>{Math.round(config.face[key] * 100)}%</output></span><input aria-label={label} type="range" min={0} max={1} step={0.05} value={config.face[key]} {...continuousInput(`face:${key}`)} onChange={event => change({ face: { ...historyRef.current.present.config.face, [key]: Number(event.target.value) }, speaking: false, autoBlink: false }, `face:${key}`)} /></label>)}</div></details>
          <button type="button" className={styles.resetButton} onClick={() => facePreset({})}>ふだんの表情に戻す</button>
        </div>}
        {activeCategory === 'motion' && <div className={styles.originalOptions}>
          <fieldset className={styles.originalFieldset}><legend>動作を選ぶ</legend><div className={styles.motionChoices} role="group" aria-label="動作">{ORIGINAL_MOTIONS.filter(([value]) => displayCharacter.motions.includes(value)).map(([value, label]) => <button type="button" key={value} aria-pressed={(pendingWeaponMotion ?? config.motion) === value} onClick={() => chooseMotion(value)}>{label}</button>)}</div></fieldset>
          <label className={styles.slider}><span>再生速度<output>{config.speed.toFixed(2)}倍</output></span><input aria-label="再生速度" type="range" min={0.25} max={2} step={0.05} value={config.speed} {...continuousInput('speed')} onChange={event => change({ speed: Number(event.target.value) }, 'speed')} /><small><span>ゆっくり</span><span>速く</span></small></label>
          <label className={styles.slider}><span>再生位置<output>{position.time.toFixed(1)}秒</output></span><input aria-label="再生位置" type="range" min={0} max={1} step={0.001} value={Math.min(1, position.time / Math.max(position.duration, 0.01))} onChange={event => { const value = Number(event.target.value); change({ playing: false }); setSeek(previous => ({ value, revision: previous.revision + 1 })); }} /></label>
          <div className={styles.originalChecks}><label><input type="checkbox" checked={config.autoBlink} onChange={event => change({ autoBlink: event.target.checked, playing: true })} />自動でまばたき</label><label><input type="checkbox" checked={config.speaking} onChange={event => change({ speaking: event.target.checked, playing: true })} />会話の口の動き</label>{displayCharacter.hairMotion && <label><input type="checkbox" checked={config.hairMotion} onChange={event => change({ hairMotion: event.target.checked })} />髪を揺らす</label>}</div>
          {!!displayCharacter.viewer.headTurnDegrees && <div><label className={styles.slider}><span>顔の向き<output>{config.headTurn}°</output></span><input aria-label="顔の向き" type="range" min={-displayCharacter.viewer.headTurnDegrees} max={displayCharacter.viewer.headTurnDegrees} step={1} value={config.headTurn} {...continuousInput('headTurn')} onChange={event => change({ headTurn: Number(event.target.value) }, 'headTurn')} /><small><span>右</span><span>左</span></small></label><p className={styles.help}>顔の向きは、この画面とJSONに残ります。GLBの再生では元の動作に戻ります。</p></div>}
        </div>}
        </section>
      </aside>}
      <details className={styles.savedDrawer} data-menu>
        <summary><StudioIcon name="cards" /><span>保存データ</span><span className={styles.countBadge}>{savedHere.length}</span></summary>
        <div className={styles.savedPanel} data-menu-panel>
          <section className={styles.library} aria-labelledby="original-saved"><h2 id="original-saved">{displayCharacter.name}の保存データ</h2>{savedHere.length ? <div className={styles.savedList}>{savedHere.map(item => <button type="button" key={item.id} className={`${styles.savedCharacter} ${editing?.id === item.id ? styles.selected : ''}`} aria-pressed={editing?.id === item.id} onClick={() => request({ kind: 'saved', entry: item })}><span>{item.name}</span><small>開く</small></button>)}</div> : <p>名前を付けて保存すると、ここから開けます。</p>}</section>
          <p className={styles.savedNote} title="キャラクターや保存データの読み込み・別名保存で、元に戻す履歴は新しくなります。">このブラウザに保存・別の端末へはJSON</p>
        </div>
      </details>
    </div>
  </section>;
}
