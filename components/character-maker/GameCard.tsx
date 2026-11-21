'use client';

// ゲームで遊ぶ入口のカード（docs/gyogun-spec.md の「入口のカード」・docs/gyogyo-spec.md の「Studio の入口」）。3D のステージの左下（武器の並びの上）に重ねる。
// ゲームのタイトル画面を録った短い動画（音なし・繰り返し）と、押せる札。単体の Studio のページだけ（games の指定があるとき）。
// 魚群繚乱のカードがいちばん下、その上に魚々娘のカードを積む（stack）。
// 形と大きさはステージの大きさから決める（lib/character-maker/game-card-fit.ts）。窓の幅で隠さず、いつもステージに出す。
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from 'react';
import { gameCardFit, GAME_PILL_WIDTH, type GameCardFit } from '../../lib/character-maker/game-card-fit';
import { STUDIO_GAMES, type StudioGame } from '../../lib/character-maker/game-handoff';
import { StudioIcon } from './StudioIcon';
import styles from './character-studio.module.css';

/** カードの動画と止め絵（public/character-maker/）。札の字は「<名前>で遊ぶ」 */
export const GAME_CARD_MEDIA: Record<StudioGame, { video: string; poster: string; label: string }> = {
  gyogun: { video: '/character-maker/gyogun-title.mp4', poster: '/character-maker/gyogun-title.jpg', label: `${STUDIO_GAMES.gyogun.name}で遊ぶ` },
  gyogyo: { video: '/character-maker/gyogyo-title.mp4', poster: '/character-maker/gyogyo-title.jpg', label: `${STUDIO_GAMES.gyogyo.name}で遊ぶ` },
};
/** ステージに出すゲームの入口（下から順） */
export const STUDIO_GAME_ORDER: StudioGame[] = ['gyogun', 'gyogyo'];

/** ステージ（root の中の #studio-stage）の大きさを見て、入口の形を決める。enabled はエディタが描かれてから true にする */
/** 'pending' = ステージがまだ画面に無く、測れていない（読み込み中）。測れたら出す */
export type GameEntryFit = GameCardFit | { mode: 'pending'; width: 0 };

export function useGameCardFit(root: RefObject<HTMLElement | null>, enabled: boolean, framing: 'full' | 'face'): GameEntryFit {
  const [size, setSize] = useState<[number, number]>([0, 0]);
  useLayoutEffect(() => {
    const stage = enabled ? root.current?.querySelector<HTMLElement>('#studio-stage') : null;
    if (!stage) { setSize([0, 0]); return; }
    const measure = () => { const box = stage.getBoundingClientRect(); setSize(previous => previous[0] === box.width && previous[1] === box.height ? previous : [box.width, box.height]); };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(stage);
    return () => observer.disconnect();
  }, [root, enabled]);
  const fit: GameEntryFit = size[0] > 0 && size[1] > 0 ? gameCardFit(size[0], size[1], framing) : { mode: 'pending', width: 0 };
  // 最初の案内（ドラッグで回す）は同じ左下に出るので、カードの右へずらす（studio-stage.module.css の .hint）
  useLayoutEffect(() => {
    const stage = enabled ? root.current?.querySelector<HTMLElement>('#studio-stage') : null;
    if (!stage || fit.mode === 'pending') return;
    stage.style.setProperty('--game-hint-shift', `${fit.width + 12}px`);
    return () => { stage.style.removeProperty('--game-hint-shift'); };
  }, [root, enabled, fit.mode, fit.width]);
  return fit;
}

/** カード 1 枚の高さ（下に積んだカードの上へ置くため）。動画 16:9 ＋ 上の余白 6px ＋ 札の行 38px。札だけなら 44px */
export function gameCardHeight(fit: GameCardFit): number {
  return fit.mode === 'card' ? (fit.width - 12) * 9 / 16 + 6 + 38 : 44;
}
/** 積んだカードの間 */
const STACK_GAP = 10;

export function GameCard({ game, stack, fit, busy, disabled, onPlay }: { game: StudioGame; stack: number; fit: GameEntryFit; busy: boolean; disabled: boolean; onPlay: () => void }) {
  const video = useRef<HTMLVideoElement>(null);
  // 動きを減らす設定の端末では流さない（止め絵）
  const [moving, setMoving] = useState(false);
  useEffect(() => {
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
    const decide = () => setMoving(!reduced.matches);
    decide();
    reduced.addEventListener('change', decide);
    return () => reduced.removeEventListener('change', decide);
  }, []);
  const card = fit.mode === 'card';
  // React は muted を属性に書かないので、流す前に値で入れる（音なしでないと自動再生が止められる）
  useEffect(() => {
    const element = video.current;
    if (!card || !moving || !element) return;
    element.muted = true;
    element.play().catch(() => { /* 自動再生できない端末は止め絵のまま */ });
  }, [card, moving]);
  if (fit.mode === 'pending') return null;
  const media = GAME_CARD_MEDIA[game];
  const lift = stack * (gameCardHeight(fit) + STACK_GAP);
  return <button type="button" className={styles.gameCard} data-mode={fit.mode} data-game={game} style={{ '--game-card-w': `${card ? fit.width : GAME_PILL_WIDTH}px`, '--game-card-lift': `${lift}px` } as CSSProperties} disabled={disabled} aria-busy={busy || undefined} onClick={onPlay}>
    {card && (moving
      ? <video ref={video} className={styles.gameCardMedia} src={media.video} poster={media.poster} muted autoPlay loop playsInline preload="metadata" disablePictureInPicture aria-hidden="true" tabIndex={-1} />
      // eslint-disable-next-line @next/next/no-img-element
      : <img className={styles.gameCardMedia} src={media.poster} alt="" width={480} height={270} decoding="async" draggable={false} />)}
    <span className={styles.gameCardLabel}><span>{busy ? '姿を作っています…' : media.label}</span><i className={styles.gameCardGo} aria-hidden="true"><StudioIcon name="play" size={14} /></i></span>
  </button>;
}
