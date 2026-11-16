'use client';

// 「魚群繚乱で遊ぶ」の入口のカード（docs/gyogun-spec.md の「入口のカード」）。3D のステージの左下（武器の並びの上）に重ねる。
// 魚群繚乱のタイトル画面を録った短い動画（音なし・繰り返し）と、押せる札。単体の Studio のページだけ（gyogun の指定があるとき）。
// 形と大きさはステージの大きさから決める（lib/character-maker/gyogun-card-fit.ts）。窓の幅で隠さず、いつもステージに出す。
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties, type RefObject } from 'react';
import { gyogunCardFit, GYOGUN_PILL_WIDTH, type GyogunCardFit } from '../../lib/character-maker/gyogun-card-fit';
import { StudioIcon } from './StudioIcon';
import styles from './character-studio.module.css';

export const GYOGUN_CARD_VIDEO = '/character-maker/gyogun-title.mp4';
export const GYOGUN_CARD_POSTER = '/character-maker/gyogun-title.jpg';

/** ステージ（root の中の #studio-stage）の大きさを見て、入口の形を決める。enabled はエディタが描かれてから true にする */
/** 'pending' = ステージがまだ画面に無く、測れていない（読み込み中）。測れたら出す */
export type GyogunEntryFit = GyogunCardFit | { mode: 'pending'; width: 0 };

export function useGyogunCardFit(root: RefObject<HTMLElement | null>, enabled: boolean, framing: 'full' | 'face'): GyogunEntryFit {
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
  const fit: GyogunEntryFit = size[0] > 0 && size[1] > 0 ? gyogunCardFit(size[0], size[1], framing) : { mode: 'pending', width: 0 };
  // 最初の案内（ドラッグで回す）は同じ左下に出るので、カードの右へずらす（studio-stage.module.css の .hint）
  useLayoutEffect(() => {
    const stage = enabled ? root.current?.querySelector<HTMLElement>('#studio-stage') : null;
    if (!stage || fit.mode === 'pending') return;
    stage.style.setProperty('--gyogun-hint-shift', `${fit.width + 12}px`);
    return () => { stage.style.removeProperty('--gyogun-hint-shift'); };
  }, [root, enabled, fit.mode, fit.width]);
  return fit;
}

export function GyogunCard({ fit, busy, disabled, onPlay }: { fit: GyogunEntryFit; busy: boolean; disabled: boolean; onPlay: () => void }) {
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
  return <button type="button" className={styles.gyogunCard} data-mode={fit.mode} style={{ '--gyogun-card-w': `${card ? fit.width : GYOGUN_PILL_WIDTH}px` } as CSSProperties} disabled={disabled} aria-busy={busy || undefined} onClick={onPlay}>
    {card && (moving
      ? <video ref={video} className={styles.gyogunCardMedia} src={GYOGUN_CARD_VIDEO} poster={GYOGUN_CARD_POSTER} muted autoPlay loop playsInline preload="metadata" disablePictureInPicture aria-hidden="true" tabIndex={-1} />
      // eslint-disable-next-line @next/next/no-img-element
      : <img className={styles.gyogunCardMedia} src={GYOGUN_CARD_POSTER} alt="" width={480} height={270} decoding="async" draggable={false} />)}
    <span className={styles.gyogunCardLabel}><span>{busy ? '姿を作っています…' : '魚群繚乱で遊ぶ'}</span><i className={styles.gyogunCardGo} aria-hidden="true"><StudioIcon name="play" size={14} /></i></span>
  </button>;
}
