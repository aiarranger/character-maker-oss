'use client';

// プリセット（ファンタジー・アメコミ）の Studio の下見を Rust の Stage で描く（前の three の版は CharacterMakerScene.tsx の studio の枝。R7 で消した。props は同じ）。
// 模型（組み立て・設定・動き・のけぞり・＞＜・武器・測り）は Rust（rust/game/src/maker.rs）、カメラと回す操作と床の動きは three なしの TS。docs/rust-stage-plan.md
import { useEffect, useRef, useState } from 'react';
import type { CharacterConfig } from '../../../lib/character-maker/config';
import { currentCharacterConfig } from '../../../lib/character-maker/config';
import { actionPlaysOnce, type ActionMotion } from '../../../lib/character-maker/actions';
import { makerAssetUrl } from '../../../lib/character-maker/asset-url';
import { outfitPaletteFiles, presetBodyFile, presetHairFile, presetMatureFiles, presetMotionFiles, presetSwapFiles } from '../../../lib/character-maker/preset-assets';
import { fetchRegisteredWeapon, presetWeaponSpec } from '../../../lib/character-maker/weapon-assets';
import { Box3, Vector3 } from '../../../lib/engine/math';
import { useStageDoubleTap } from '../stage-camera';
import { StageError } from '../StageError';
import { comicCoverImage, comicMarkImage } from './hurt-face-images';
import { createMakerStage, markUnsupported, type MakerStage } from './stage';
import { ClassicCameraRig, StudioCameraRig, type Subject } from './studio-camera';
import { StageFloorMotion, sendFloorShadow } from './stage-floor';

type Motion = 'idle' | 'walk' | 'run' | 'sword_idle' | 'sword_attack' | ActionMotion;
/** 1 回で止まる動作か（振る・ゲーム用の 1 回の動作） */
const playsOnce = (motion: Motion) => motion === 'sword_attack' || actionPlaysOnce(motion);
type Props = {
  config: CharacterConfig; modelRevision?: number; framing: 'full' | 'face'; view: 'front' | 'side' | 'back';
  motion: Motion; motionRevision?: number; playing?: boolean; speed?: number;
  onReady?: (ready: boolean) => void; onError?: (message: string) => void;
  hitRevision?: number; onHitComplete?: () => void; hurtFace?: boolean; onMotionComplete?: () => void; cameraRevision?: number;
  studio?: boolean; accent?: string; onFramingToggle?: () => void;
};
type Info = { restHeight: number; restTop: number; height: number; headY: number; portrait: { min: number[]; max: number[]; hasHead: boolean } };

/** 素材のバイト列（同じ名前は 1 回だけ取る。アメコミの ＞＜ の絵は体の GLB から作るので、バイト列も控える） */
const bytesCache = new Map<string, Promise<Uint8Array>>();
export function assetBytes(name: string) {
  let pending = bytesCache.get(name);
  if (!pending) {
    pending = fetch(makerAssetUrl(name)).then(async response => {
      if (!response.ok) throw new Error('3Dモデルを読み込めませんでした。');
      return new Uint8Array(await response.arrayBuffer());
    });
    pending.catch(() => bytesCache.delete(name));
    bytesCache.set(name, pending);
  }
  return pending;
}
export async function fetchAsset(stage: MakerStage, name: string, load = () => assetBytes(name)) {
  if (stage.has(name)) return;
  const bytes = await load();
  if (!stage.has(name)) stage.asset(name, bytes);
}

/** 組み立ての値（素材の名前と設定）。書き出しも同じ物を使う */
/** studio: Studio の見え方（アメコミのならした肌）。旧画面 classic とその書き出しは今までどおり */
export async function presetSpec(stage: MakerStage, config: CharacterConfig, studio = false) {
  const c = currentCharacterConfig(config);
  const body = presetBodyFile(c, studio), hair = presetHairFile(c), motions = presetMotionFiles(c), outfit = outfitPaletteFiles(c), mature = presetMatureFiles(c), swap = studio ? presetSwapFiles(c) : {};
  const weapon = presetWeaponSpec(c);
  const hurtFace = c.style === 'comic' ? { cover: `hurt-cover-${body}.png`, marks: 'hurt-marks.png' } : null;
  await Promise.all([
    ...[body, ...(hair ? [hair] : []), ...motions, ...Object.values(outfit), ...Object.values(mature), ...Object.values(swap)].map(name => fetchAsset(stage, name)),
    ...(weapon ? [fetchAsset(stage, weapon.asset, () => fetchRegisteredWeapon(weapon.id))] : []),
    ...(hurtFace ? [Promise.all([assetBytes(body), swap['maker-skin'] ? assetBytes(swap['maker-skin']) : undefined]).then(([glb, skin]) => comicCoverImage(glb, skin)).then(image => stage.image(hurtFace.cover, image.width, image.height, image.rgba)),
      Promise.resolve(comicMarkImage()).then(image => stage.image(hurtFace.marks, image.width, image.height, image.rgba))] : []),
  ]);
  return { body, hair, motions, outfit, mature, swap, weapon, hurtFace, config: c };
}

const v3 = (a: number[]) => new Vector3(a[0], a[1], a[2]);

export default function RustMakerScene(props: Props) {
  const { modelRevision = 0, motion, motionRevision = 0 } = props;
  const config = currentCharacterConfig(props.config);
  const latest = useRef(props);
  latest.current = props;
  const root = useRef<HTMLDivElement>(null), canvas = useRef<HTMLCanvasElement>(null);
  const [stage, setStage] = useState<MakerStage | null>(null);
  const [info, setInfo] = useState<Info | null>(null);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const subject = useRef<Subject | null>(null);
  const rig = useRef<StudioCameraRig | null>(null);
  const classic = useRef<ClassicCameraRig | null>(null);
  const studio = props.studio ?? false;
  const doubleTap = useStageDoubleTap(() => latest.current.onFramingToggle?.());

  // Stage は 1 回だけ作る（今の版の Canvas と同じく、模型を作り直しても canvas は替えない）
  useEffect(() => {
    const el = canvas.current, holder = root.current;
    if (!el || !holder) return;
    let live = true, made: MakerStage | null = null;
    createMakerStage(el, { dpr: Math.min(Math.max(devicePixelRatio, 1), 2), look: latest.current.studio ? 'studio' : 'flat' }).then(s => {
      if (!live) { s.free(); return; }
      made = s;
      if (latest.current.studio) { sendFloorShadow(s); rig.current = new StudioCameraRig(holder, holder); }
      else { s.classic_floor(); classic.current = new ClassicCameraRig(holder); }
      setStage(s);
      // 開発用（値の読み出し）。本番の build では付けない
      if (process.env.NODE_ENV !== 'production') (window as Window & { __makerStage?: MakerStage }).__makerStage = s;
    }, markUnsupported);
    return () => { live = false; rig.current?.dispose(); rig.current = null; classic.current?.dispose(); classic.current = null; made?.free(); setStage(null); };
  }, []);

  // Studio は読み込みに失敗したら 1 回だけ自分で読み直す（スマホで fetch が落ちるのはよくある）
  const retried = useRef(false), retryTimer = useRef(0);
  useEffect(() => { retried.current = false; }, [config.style, config.base, config.hair, config.outfitPalette, config.weaponId, config.face, modelRevision]);
  useEffect(() => () => window.clearTimeout(retryTimer.current), []);

  // 模型を作り直す物（今の版の assembleCharacter の useEffect と同じ鍵）
  const matureHead = config.style === 'fantasy' && config.face === 'mature';
  useEffect(() => {
    if (!stage) return;
    let cancelled = false;
    setInfo(null); setError('');
    subject.current = null;
    latest.current.onReady?.(false);
    presetSpec(stage, latest.current.config, latest.current.studio ?? false).then(spec => {
      if (cancelled) return;
      const made = JSON.parse(stage.assemble(JSON.stringify({ ...spec, config: currentCharacterConfig(latest.current.config) }))) as Info;
      const p = latest.current;
      stage.play(JSON.stringify({ motion: p.motion, once: playsOnce(p.motion), playing: p.playing ?? true, speed: p.speed ?? 1, hurtFace: p.hurtFace ?? false, weaponVisible: p.framing !== 'face' }));
      const key = `${Date.now()}:${Math.random()}`;
      subject.current = {
        key, rest: made, headY: made.headY, hasHead: made.portrait.hasHead, portrait: new Box3(v3(made.portrait.min), v3(made.portrait.max)),
        head: () => { const h = JSON.parse(stage.query('{"q":"bone","name":"head"}')) as number[] | null; return h ? v3(h) : null; },
        armed: () => { const a = JSON.parse(stage.query('{"q":"armed","names":["sword_idle"]}')) as { min: number[]; max: number[] } | null; return a ? new Box3(v3(a.min), v3(a.max)) : null; },
      };
      setInfo(made);
      latest.current.onReady?.(true);
    }).catch(reason => {
      if (cancelled) return;
      const message = reason instanceof Error ? reason.message : '3Dモデルを読み込めませんでした。';
      if (latest.current.studio && !retried.current) {
        retried.current = true;
        console.warn('[studio] model load failed, retrying once:', message);
        retryTimer.current = window.setTimeout(() => setAttempt(value => value + 1), 500);
        return;
      }
      console.warn('[studio] model load failed:', message);
      setError(message);
      latest.current.onError?.(message);
    });
    return () => { cancelled = true; };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stage, config.style, config.base, config.hair, config.outfitPalette, config.weaponId, matureHead, modelRevision, attempt]);

  // 作り直さずに当てる設定（instance.apply(config)）
  const configKey = JSON.stringify(config);
  useEffect(() => { if (stage && info) stage.apply(configKey); }, [stage, info, configKey]);
  useEffect(() => { if (stage && info) stage.play(JSON.stringify({ motion, once: playsOnce(motion) })); }, [stage, info, motion, motionRevision]);
  useEffect(() => { if (stage && info) stage.play(JSON.stringify({ playing: props.playing ?? true, speed: props.speed ?? 1 })); }, [stage, info, props.playing, props.speed]);
  useEffect(() => { if (stage && info) stage.play(JSON.stringify({ hurtFace: props.hurtFace ?? false })); }, [stage, info, props.hurtFace]);
  // のけぞりを 1 回（値が変わるたびに最初から。最初の値では再生しない）
  const seenHit = useRef(props.hitRevision ?? 0);
  useEffect(() => {
    const revision = props.hitRevision ?? 0;
    if (seenHit.current === revision) return;
    seenHit.current = revision;
    if (stage && info) stage.play(JSON.stringify({ hit: true }));
  }, [stage, info, props.hitRevision]);
  // 顔の表示では登録された武器を隠す（setRegisteredWeaponsVisible）
  useEffect(() => { if (stage && info) stage.play(JSON.stringify({ weaponVisible: props.framing !== 'face' })); }, [stage, info, props.framing]);

  // 毎コマ: 回す操作 → 動き → カメラ → 床 → 描く
  useEffect(() => {
    if (!stage) return;
    let raf = 0, last = performance.now();
    const cam = new Float64Array(11);
    const floor = new StageFloorMotion();
    let floorKey = '';
    // 画素を読む前に描き直す口（WebGPU の canvas は描いた直後の同じ処理の中でしか読めない。見本画像・切り替えの残像）
    const el0 = canvas.current as (HTMLCanvasElement & { __makerRedraw?: () => void }) | null;
    if (el0) el0.__makerRedraw = () => { const r = rig.current ?? classic.current; if (!r) return; r.write(cam); stage.draw(cam); };
    const tick = (now: number) => {
      raf = requestAnimationFrame(tick);
      const delta = Math.max(0, (now - last) / 1000); last = now;
      const el = canvas.current, r = rig.current, s = subject.current, p = latest.current, k = classic.current;
      if (el && k) {
        try {
          k.beforeFrame();
          stage.step(delta);
          const events = stage.events();
          if (events !== '[]') for (const e of JSON.parse(events) as { t: string }[]) {
            if (e.t === 'motionComplete') p.onMotionComplete?.();
            else if (e.t === 'hitComplete') p.onHitComplete?.();
          }
          if (s) k.layout(s, p.framing, p.view, p.cameraRevision ?? 0, currentCharacterConfig(p.config).height, p.motion.startsWith('sword_'), el.clientWidth / Math.max(el.clientHeight, 1));
          k.write(cam);
          stage.draw(cam);
        } catch (e) { markUnsupported(e); }
        return;
      }
      if (!el || !r) return;
      try {
        r.beforeFrame();
        stage.step(delta);
        const events = stage.events();
        if (events !== '[]') for (const e of JSON.parse(events) as { t: string }[]) {
          if (e.t === 'motionComplete') p.onMotionComplete?.();
          else if (e.t === 'hitComplete') p.onHitComplete?.();
        }
        const height = currentCharacterConfig(p.config).height;
        if (s) {
          const aspect = el.clientWidth / Math.max(el.clientHeight, 1);
          r.layout(s, p.framing, p.view, p.cameraRevision ?? 0, height, p.motion.startsWith('sword_'), aspect);
          r.frame(s, p.framing, height, delta);
        }
        if (s && s.key !== floorKey) { floorKey = s.key; floor.reset(); }
        const hips = s ? JSON.parse(stage.query('{"q":"bone","name":"hips"}')) as [number, number, number] | null : null;
        floor.frame(stage, { hips, scale: height, restHeight: s?.rest.restHeight ?? 0, accent: p.accent, pulseKey: `${p.view}:${p.framing}`, visible: !!s, delta });
        r.write(cam);
        stage.draw(cam);
      } catch (e) { markUnsupported(e); }
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [stage]);

  if (!studio) return <div ref={root} style={{ position: 'relative', width: '100%', height: '100%', minHeight: 0 }} aria-label="キャラクターの3Dプレビュー" data-stage-engine="rust">
    <canvas ref={canvas} style={{ display: 'block', width: '100%', height: '100%', touchAction: 'none' }} />
    {!info && <div role={error ? 'alert' : 'status'} style={{ position: 'absolute', inset: 0, display: 'grid', alignContent: 'center', justifyItems: 'center', gap: 12, padding: 28, textAlign: 'center', background: '#eef3eee8', color: '#31483e' }}>
      <span>{error || 'キャラクターを準備しています…'}</span>
      {error && <button type="button" onClick={() => setAttempt(value => value + 1)}>もう一度読み込む</button>}
    </div>}
  </div>;

  return <div ref={root} data-stage-canvas="" data-stage-engine="rust" data-camera-settled="false" style={{ position: 'absolute', inset: 0, minHeight: 0 }} {...doubleTap}>
    <canvas ref={canvas} style={{ display: 'block', width: '100%', height: '100%', touchAction: 'none' }} />
    {!info && (error
      ? <StageError onRetry={() => setAttempt(value => value + 1)} />
      : <div role="status" style={SR_ONLY}>キャラクターを準備しています…</div>)}
  </div>;
}

const SR_ONLY = { position: 'absolute', width: 1, height: 1, padding: 0, margin: -1, overflow: 'hidden', clip: 'rect(0,0,0,0)', whiteSpace: 'nowrap', border: 0 } as const;
