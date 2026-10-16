'use client';

// Stage presentation around the 3D canvas (spec §5.1, §5.5, §5.6, §6 M1/M2/M3/M8).
// Exports and prop types are the A/B contract (spec §8); keep them stable.
import { useEffect, useRef, useState, type CSSProperties } from 'react';
import { createPortal } from 'react-dom';
import styles from './studio-stage.module.css';

export type StageView = 'front' | 'side' | 'back';
export type StageFraming = 'full' | 'face';
export type StageSnapshot = { src: string; rect: DOMRect };

export type StageBackdropProps = { look: string };
export type StageOverlayProps = { poster?: string; entered: boolean; weaponId: string; view: StageView; framing: StageFraming };
export type SwapGhostProps = { snapshot: StageSnapshot | null; onDone: () => void };

const HINT_KEY = 'cs.hint.v1';

/** z0: cyclorama wall-to-floor sweep, paper grain, vignette and the watermark number. */
export function StageBackdrop({ look }: StageBackdropProps) {
  return <div className={styles.backdrop} aria-hidden="true">
    {look && <p key={look} className={styles.look}>{look}</p>}
  </div>;
}

function readHintSeen() {
  try { return window.localStorage.getItem(HINT_KEY) === '1'; } catch { return true; }
}
function writeHintSeen() {
  try { window.localStorage.setItem(HINT_KEY, '1'); } catch { /* storage blocked: the hint simply returns next visit */ }
}

function OrbitIcon() {
  return <svg width="18" height="18" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <ellipse cx="10" cy="10" rx="7.5" ry="3.2" />
    <path d="M14.6 5.6 17.5 7.4l-2.2 2.4" />
    <circle cx="10" cy="10" r="1.6" />
  </svg>;
}

/** z2 (pointer-events none): loading poster, equip ring (M3), crop marks (M2), first-run hint (M8). */
export function StageOverlay({ poster, entered, weaponId, view, framing }: StageOverlayProps) {
  const root = useRef<HTMLDivElement>(null);
  // M3: ring on every weapon change after the first render.
  const [equip, setEquip] = useState(0);
  const lastWeapon = useRef(weaponId);
  useEffect(() => {
    if (lastWeapon.current === weaponId) return;
    lastWeapon.current = weaponId;
    if (entered) setEquip(value => value + 1);
  }, [weaponId, entered]);
  // M2: marks nudge inward on a view or framing change (alternate keyframes restart the animation).
  const [pulse, setPulse] = useState<'' | 'a' | 'b'>('');
  const lastCamera = useRef(`${view}:${framing}`);
  useEffect(() => {
    const key = `${view}:${framing}`;
    if (lastCamera.current === key) return;
    lastCamera.current = key;
    setPulse(value => value === 'a' ? 'b' : 'a');
  }, [view, framing]);
  // M8: first-run hint, removed 200 ms after the first pointerdown on the stage.
  // The pinch line shows on touch and narrow stages (CSS decides).
  const [hint, setHint] = useState<'hidden' | 'shown' | 'leaving'>('hidden');
  useEffect(() => {
    if (readHintSeen()) return;
    setHint('shown');
  }, []);
  // IF-3: the poster stays 460 ms after the first ready (covering the entrance), then fades out over 240 ms.
  const [posterPhase, setPosterPhase] = useState<'shown' | 'fading' | 'gone'>(entered ? 'gone' : 'shown');
  useEffect(() => {
    if (!entered) return;
    const fade = window.setTimeout(() => setPosterPhase(phase => phase === 'gone' ? phase : 'fading'), 460);
    const gone = window.setTimeout(() => setPosterPhase('gone'), 460 + 240);
    return () => { window.clearTimeout(fade); window.clearTimeout(gone); };
  }, [entered]);
  useEffect(() => {
    if (hint !== 'shown') return;
    const stage = root.current?.parentElement;
    if (!stage) return;
    const dismiss = () => { writeHintSeen(); setHint('leaving'); };
    // Capture phase: the canvas controls may stop propagation of their pointerdown.
    stage.addEventListener('pointerdown', dismiss, { once: true, capture: true });
    return () => stage.removeEventListener('pointerdown', dismiss, { capture: true });
  }, [hint]);
  useEffect(() => {
    if (hint !== 'leaving') return;
    const timer = window.setTimeout(() => setHint('hidden'), 200);
    return () => window.clearTimeout(timer);
  }, [hint]);
  return <div ref={root} className={styles.overlay} aria-hidden="true">
    {poster && posterPhase !== 'gone' && <img className={styles.poster} src={poster} alt="" draggable={false} data-hidden={posterPhase === 'fading' || undefined} />}
    {!entered && <span className={styles.loader} />}
    {equip > 0 && <span key={equip} className={styles.equipRing} onAnimationEnd={() => setEquip(0)} />}
    <div className={styles.marks} data-pulse={pulse || undefined}>
      <span className={styles.mark} /><span className={styles.mark} /><span className={styles.mark} /><span className={styles.mark} />
    </div>
    {entered && hint !== 'hidden' && <span className={styles.hint} data-leaving={hint === 'leaving'} title="ドラッグで回す・ピンチで拡大">
      <OrbitIcon /><span className={styles.hintText}><span>ドラッグで回す</span><span className={styles.hintPinch}>ピンチで拡大</span></span>
    </span>}
  </div>;
}

/** Half-resolution WebP of the live stage canvas, taken before a character switch (M1). */
export function captureStageSnapshot(): StageSnapshot | null {
  try {
    const canvas = document.querySelector<HTMLCanvasElement>('[data-stage-canvas] canvas');
    if (!canvas || canvas.width === 0 || canvas.height === 0) return null;
    const rect = canvas.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) return null;
    const copy = document.createElement('canvas');
    copy.width = Math.max(1, Math.round(canvas.width / 2));
    copy.height = Math.max(1, Math.round(canvas.height / 2));
    const context = copy.getContext('2d');
    if (!context) return null;
    context.drawImage(canvas, 0, 0, copy.width, copy.height);
    return { src: copy.toDataURL('image/webp', .8), rect };
  } catch {
    return null;
  }
}

/** Fixed-position after-image of the previous character: fades, blurs and drops 12px over 420 ms. */
export function SwapGhost({ snapshot, onDone }: SwapGhostProps) {
  const done = useRef(onDone);
  done.current = onDone;
  useEffect(() => {
    if (!snapshot) return;
    // Safety net: some browsers skip animationend for hidden tabs.
    const timer = window.setTimeout(() => done.current(), 700);
    return () => window.clearTimeout(timer);
  }, [snapshot]);
  if (!snapshot || typeof document === 'undefined') return null;
  const { rect } = snapshot;
  const style: CSSProperties = { left: rect.left, top: rect.top, width: rect.width, height: rect.height };
  return createPortal(<img className={styles.ghost} src={snapshot.src} alt="" aria-hidden="true" draggable={false} style={style} onAnimationEnd={() => done.current()} />, document.body);
}
