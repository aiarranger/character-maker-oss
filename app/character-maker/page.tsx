'use client';

import { useEffect, useRef, useState, type CSSProperties, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import OriginalCharacterMaker, { type OriginalEditorHandle, type OriginalEditorSession } from '../../components/character-maker/OriginalCharacterMaker';
import MakerPresetEditor, { type MakerEditorHandle, type MakerEditorSession } from '../../components/character-maker/MakerPresetEditor';
import { SwapGhost, captureStageSnapshot, type StageSnapshot } from '../../components/character-maker/StudioStage';
import { StudioMark } from '../../components/character-maker/StudioIcon';
import type { OriginalCharacter } from '../../lib/character-maker/original-config';
import { ACTIVE_FANTASY_PRESET_IDS, getPresets, makeDocument, type CharacterConfig } from '../../lib/character-maker/config';
import { loadStudioSelection, saveStudioSelection } from '../../lib/character-maker/studio-selection';
import { makerAssetUrl } from '../../lib/character-maker/asset-url';
import { STUDIO_GROUP_LABELS, studioLook, type StudioGroup, type StudioLook } from '../../lib/character-maker/studio-palette';
import styles from '../../components/character-maker/character-studio.module.css';
import catalog from '../../lib/character-maker/studio-catalog.json';

const characters = catalog as OriginalCharacter[];
// Keep legacy preset definitions/imports intact; the chooser contains the curated cast.
const activeFantasyPresets = new Set<string>(ACTIVE_FANTASY_PRESET_IDS);
const presets = (['fantasy', 'comic'] as const).flatMap(style => getPresets(style)
  .filter(preset => style !== 'fantasy' || activeFantasyPresets.has(preset.id))
  .map(preset => ({ ...preset, style, key: `${style}:${preset.id}` })));
type Selection = { kind: 'original'; characterId: string } | { kind: 'preset'; presetKey: string };
function selectionId(selection: Selection) { return selection.kind === 'original' ? `original:${selection.characterId}` : selection.presetKey; }

function presetPortrait(config: CharacterConfig, id: string) {
  return config.style === 'fantasy'
    ? makerAssetUrl(`thumb-fantasy-${config.base}-preset-${id}.jpg`)
    : `/portraits/selector-comic-${id}-v1.jpg`;
}

type CastEntry = { id: string; name: string; portrait?: string; sheet: boolean; selection: Selection; look: StudioLook };
const cast: CastEntry[] = [
  ...characters.map(character => ({ id: `original:${character.id}`, name: character.name, portrait: character.thumbnail || undefined, sheet: Boolean(character.thumbnailIsSheet), selection: { kind: 'original', characterId: character.id } as Selection })),
  ...presets.map(item => ({ id: item.key, name: item.name, portrait: presetPortrait(item.config, item.id), sheet: false, selection: { kind: 'preset', presetKey: item.key } as Selection })),
].map((entry, index) => ({ ...entry, look: studioLook(entry.id, index) }));

const groups = (['original', 'fantasy', 'comic'] as StudioGroup[]).map(group => {
  const items = cast.filter(entry => entry.look.group === group);
  return { group, label: STUDIO_GROUP_LABELS[group], items, range: items.length ? `${items[0].look.look}–${items[items.length - 1].look.look}` : '' };
}).filter(group => group.items.length);

/** ボタンのまま、矢印キー・Home・End で隣へ移る（ロービング tabindex。role=radio にはしない）。 */
function moveFocusWithArrows(event: ReactKeyboardEvent<HTMLElement>) {
  const forward = ['ArrowDown', 'ArrowRight'], backward = ['ArrowUp', 'ArrowLeft'];
  if (![...forward, ...backward, 'Home', 'End'].includes(event.key)) return;
  const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)')];
  if (!buttons.length) return;
  const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
  event.preventDefault();
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1
    : index < 0 ? 0 : (index + (forward.includes(event.key) ? 1 : -1) + buttons.length) % buttons.length;
  buttons[next].focus();
  buttons[next].scrollIntoView({ block: 'nearest', inline: 'nearest' });
}

export default function Page() {
  const [selection, setSelection] = useState<Selection>({ kind: 'original', characterId: characters[0].id });
  const [hydrated, setHydrated] = useState(false);
  const [selectionWarning, setSelectionWarning] = useState('');
  const [ghost, setGhost] = useState<StageSnapshot | null>(null);
  const originalSessions = useRef(new Map<string, OriginalEditorSession>());
  const presetSessions = useRef(new Map<string, MakerEditorSession>());
  const originalEditor = useRef<OriginalEditorHandle>(null);
  const presetEditor = useRef<MakerEditorHandle>(null);
  const castList = useRef<HTMLElement>(null);
  const activeId = selectionId(selection);
  const activeIdRef = useRef(activeId);
  activeIdRef.current = activeId;
  const preset = selection.kind === 'preset' ? presets.find(item => item.key === selection.presetKey) : null;
  const active = cast.find(entry => entry.id === activeId) ?? cast[0];
  const groupLabel = `${active.look.look} · ${STUDIO_GROUP_LABELS[active.look.group]}`;
  const poster = active.sheet ? undefined : active.portrait;

  useEffect(() => {
    const restored = loadStudioSelection(window, [...characters.map(item => `original:${item.id}`), ...presets.map(item => item.key)]);
    if (restored) setSelection(restored.startsWith('original:')
      ? { kind: 'original', characterId: restored.slice('original:'.length) }
      : { kind: 'preset', presetKey: restored });
    setHydrated(true);
  }, []);

  // 書き出し・•••・保存データのメニューは、Esc・外側のクリック・項目の選択で閉じる（§3・U10・U12）。
  useEffect(() => {
    const openMenus = () => [...document.querySelectorAll<HTMLDetailsElement>('details[data-menu][open]')];
    const onPointer = (event: PointerEvent) => {
      for (const menu of openMenus()) if (!(event.target instanceof Node && menu.contains(event.target))) menu.open = false;
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      const open = openMenus();
      if (!open.length) return;
      const focused = open.find(menu => menu.contains(document.activeElement));
      for (const menu of open) menu.open = false;
      focused?.querySelector('summary')?.focus();
    };
    const onClick = (event: MouseEvent) => {
      if (!(event.target instanceof Element)) return;
      const item = event.target.closest('details[data-menu] [data-menu-panel] button');
      const menu = item?.closest<HTMLDetailsElement>('details[data-menu]');
      if (!menu) return;
      menu.open = false;
      // 項目を選んだ後は、確認ダイアログなどが取っていなければ、フォーカスをメニューの見出しへ戻す（U10）。
      // 閉じたパネルの中に残ったフォーカス（Chrome は描画まで body へ移さない）も戻す対象にする。
      const summary = menu.querySelector('summary');
      requestAnimationFrame(() => {
        const focused = document.activeElement;
        if (menu.isConnected && (!focused || focused === document.body || (menu.contains(focused) && focused !== summary))) summary?.focus({ preventScroll: true });
      });
    };
    // 文書がスクロールする幅（スマホ）では、開いたパネルを画面内へ出す（U12）。toggle は泡立たないので capture で受ける。
    const onToggle = (event: Event) => {
      const menu = event.target;
      if (!(menu instanceof HTMLDetailsElement) || !menu.open || !menu.matches('details[data-menu]')) return;
      requestAnimationFrame(() => {
        const panel = menu.querySelector<HTMLElement>('[data-menu-panel]');
        if (!panel || document.documentElement.scrollHeight <= window.innerHeight + 1) return;
        let reduced = false;
        try { reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches; } catch { reduced = false; }
        panel.scrollIntoView({ block: 'nearest', behavior: reduced ? 'auto' : 'smooth' });
      });
    };
    document.addEventListener('pointerdown', onPointer);
    document.addEventListener('keydown', onKey);
    document.addEventListener('click', onClick);
    document.addEventListener('toggle', onToggle, true);
    return () => {
      document.removeEventListener('pointerdown', onPointer);
      document.removeEventListener('keydown', onKey);
      document.removeEventListener('click', onClick);
      document.removeEventListener('toggle', onToggle, true);
    };
  }, []);

  // [ と ] で前後のキャラクターへ送る（入力中は除く）。
  const selectRef = useRef<(next: Selection) => void>(() => {});
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if ((event.key !== '[' && event.key !== ']') || event.ctrlKey || event.metaKey || event.altKey || event.isComposing) return;
      if (event.target instanceof HTMLElement && event.target.closest('input, textarea, select, [contenteditable]')) return;
      const index = cast.findIndex(entry => entry.id === activeIdRef.current);
      const next = cast[(index + (event.key === ']' ? 1 : -1) + cast.length) % cast.length];
      event.preventDefault();
      selectRef.current(next.selection);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, []);

  // スマホの横帯では、選んだカードを帯の中央へ寄せる（文書は縦に動かさない）。
  useEffect(() => {
    const list = castList.current;
    if (!list || !window.matchMedia('(max-width: 899px)').matches) return;
    const card = list.querySelector<HTMLElement>('[aria-current="true"]');
    if (!card) return;
    const listBox = list.getBoundingClientRect(), cardBox = card.getBoundingClientRect();
    list.scrollTo({ left: list.scrollLeft + cardBox.left - listBox.left - (listBox.width - cardBox.width) / 2, behavior: hydrated ? 'smooth' : 'auto' });
  }, [activeId, hydrated]);

  selectRef.current = selectCharacter;
  function selectCharacter(next: Selection) {
    if (selectionId(next) === activeId) return;
    // Capture synchronously while the current editor is still mounted. Failed
    // persistence cancels selection, preserving the current draft and history.
    if (selection.kind === 'original') {
      const session = originalEditor.current?.captureSession();
      if (!session) return;
      originalSessions.current.set(session.history.present.characterId, session);
    } else {
      const session = presetEditor.current?.captureSession();
      if (!session) return;
      presetSessions.current.set(selection.presetKey, session);
    }
    setGhost(captureStageSnapshot());
    setSelectionWarning(saveStudioSelection(window, selectionId(next)) ? '' : '選んだキャラクターを次回に引き継げませんでした。編集内容は各キャラクターの控えに残っています。');
    setSelection(next);
  }

  return <main className={`${styles.shell} ${styles.selectionStudio}`} data-studio-page style={{ '--accent': active.look.accent } as CSSProperties}>
    <a className={styles.skipLink} href="#studio-edit">編集へ移動</a>
    <header className={styles.brandCell}>
      <StudioMark />
      <h1 className={styles.wordmark}>Character Studio</h1>
    </header>
    <aside className={styles.castColumn} aria-label="キャラクターを選ぶ">
      <nav ref={castList} className={styles.characterList} aria-label="キャラクター一覧" onKeyDown={moveFocusWithArrows}>
        {groups.map(group => <div key={group.group} className={styles.castGroup}>
          <h2 className={styles.castHeading}><span>{group.label}</span><i aria-hidden="true" /><small>{group.range}</small></h2>
          <div className={styles.castGrid}>
            {group.items.map(entry => <button type="button" key={entry.id} className={styles.castCard}
              aria-label={`${entry.name}を選ぶ`} aria-current={activeId === entry.id ? 'true' : undefined} tabIndex={activeId === entry.id ? 0 : -1}
              style={{ '--card-accent': entry.look.accent } as CSSProperties}
              onClick={() => selectCharacter(entry.selection)}>
              <span className={`${styles.castPortrait} ${entry.sheet ? styles.castSheet : ''}`}>
                {/* eslint-disable-next-line @next/next/no-img-element */}
                {entry.portrait && <img src={entry.portrait} alt="" loading="lazy" decoding="async" />}
              </span>
              {/* 番号は CSS で描き、ボタンの文字は名前だけにする（E2E が名前の完全一致で探す）。 */}
              <span className={styles.castNumber} data-look={entry.look.look} aria-hidden="true" />
              <span className={styles.castName}>{entry.name}</span>
            </button>)}
          </div>
        </div>)}
      </nav>
    </aside>
    <div className={styles.selectedWorkspace}>
      {selectionWarning && <p className={styles.pageWarning} role="alert">{selectionWarning}</p>}
      {!hydrated ? <div className={`${styles.liveEditor} ${styles.editorLoading}`} aria-busy="true"><span className={styles.srOnly} role="status">編集中のキャラクターを確認しています…</span></div> : selection.kind === 'original' ? <OriginalCharacterMaker
        key={activeId} embedded editorRef={originalEditor} characters={characters}
        initialCharacterId={selection.characterId} initialSession={originalSessions.current.get(selection.characterId)}
        accent={active.look.accent} look={active.look.look} groupLabel={groupLabel} poster={poster}
      /> : preset ? <MakerPresetEditor key={activeId} embedded editorRef={presetEditor}
        selectionKey={preset.key} initialDocument={makeDocument(preset.name, preset.config)} initialSession={presetSessions.current.get(preset.key)}
        accent={active.look.accent} look={active.look.look} groupLabel={groupLabel} poster={poster}
      /> : null}
    </div>
    {ghost && <SwapGhost snapshot={ghost} onDone={() => setGhost(null)} />}
  </main>;
}
