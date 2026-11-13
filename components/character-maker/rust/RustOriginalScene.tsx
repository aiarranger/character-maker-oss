'use client';

// オリジナル 4 体の Studio の下見を Rust の Stage で描く（今の版は ../OriginalCharacterScene.tsx。props は同じ）。
// 模型（組み立て・体型・部品・髪・武器・＞＜・動き・測り）は Rust（rust/game/src/original.rs）、カメラ（正射影）と回す操作と床の動きは three なしの TS。docs/rust-stage-plan.md
import { useEffect, useRef, useState } from 'react';
import { Box3, Matrix4, Vector3 } from '../../../lib/engine/math';
import { originalModelAsset, type OriginalCharacter, type OriginalConfig } from '../../../lib/character-maker/original-config';
import { selectedMakerWeaponId } from '../../../lib/character-maker/weapon-catalog';
import { useStageDoubleTap } from '../stage-camera';
import { StageError } from '../StageError';
import { createMakerStage, markUnsupported, type MakerStage } from './stage';
import { OriginalCameraRig, type OriginalSubject } from './original-camera';
import { forgetModel, originalSpec, originalStageConfig } from './original-spec';
import { StageFloorMotion, sendFloorShadow } from './stage-floor';

type Props = {
  character: OriginalCharacter;
  config: OriginalConfig;
  attackRevision?: number;
  hitRevision?: number;
  onHitComplete?: () => void;
  hurtFace?: boolean;
  onAttackComplete?: () => void;
  framing: 'full' | 'face';
  view: 'front' | 'side' | 'back';
  cameraRevision?: number;
  modelRevision?: number;
  onReady?: (ready: boolean) => void;
  onError?: (message: string) => void;
  onCameraInteraction?: () => void;
  seek?: { value: number; revision: number };
  report?: (time: number, duration: number) => void;
  accent?: string;
  onFramingToggle?: () => void;
  studio?: boolean;
};
type Info = { hasHead: boolean; restHeight: number };
const INITIAL_SEEK = { value: 0, revision: 0 };
const v3 = (a: number[]) => new Vector3(a[0], a[1], a[2]);
const box = (a: number[][] | null) => a ? new Box3(v3(a[0]), v3(a[1])) : new Box3();

export default function RustOriginalScene(props: Props) {
  const { character, config, framing, view, cameraRevision = 0, modelRevision = 0, seek = INITIAL_SEEK } = props;
  const latest = useRef(props);
  latest.current = props;
  const root = useRef<HTMLDivElement>(null), canvas = useRef<HTMLCanvasElement>(null);
  const [stage, setStage] = useState<MakerStage | null>(null);
  const [info, setInfo] = useState<Info | null>(null);
  const [ready, setReady] = useState(false);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const subject = useRef<OriginalSubject | null>(null);
  const rig = useRef<OriginalCameraRig | null>(null);
  const doubleTap = useStageDoubleTap(() => latest.current.onFramingToggle?.());
  const asset = originalModelAsset(character, config.appearance);
  const weaponId = selectedMakerWeaponId(config);
  const studio = props.studio ?? false;
  const baseToken = `${weaponId}:${character.id}:${character.modelSha256}:${asset.sha256}:${modelRevision}`;
  const token = `${baseToken}:${attempt}`;
  // 作り直しても続ける再生の時刻（今の版の playback。同じキャラ・動作・振るの回数のときだけ）
  const playback = useRef<{ character: string; motion: string; attackRevision: number; time: number } | null>(null);
  const appliedSeek = useRef<number | null>(null);

  // Stage は 1 回だけ作る（今の版の Canvas と同じく、模型を作り直しても canvas は替えない）
  useEffect(() => {
    const el = canvas.current, holder = root.current;
    if (!el || !holder) return;
    let live = true, made: MakerStage | null = null;
    createMakerStage(el, { dpr: Math.min(Math.max(devicePixelRatio, 1), 2) }).then(s => {
      if (!live) { s.free(); return; }
      made = s;
      sendFloorShadow(s);
      rig.current = new OriginalCameraRig(holder, holder);
      setStage(s);
      if (process.env.NODE_ENV !== 'production') (window as Window & { __makerStage?: MakerStage }).__makerStage = s;
    }, markUnsupported);
    return () => { live = false; rig.current?.dispose(); rig.current = null; made?.free(); setStage(null); };
  }, []);
  useEffect(() => { if (rig.current) rig.current.onCameraInteraction = () => latest.current.onCameraInteraction?.(); });

  // Studio は読み込みに失敗したら 1 回だけ自分で読み直す
  const retried = useRef(''), retryTimer = useRef(0);
  useEffect(() => () => window.clearTimeout(retryTimer.current), []);

  // 模型を作り直す物（今の版の useMemo の鍵: 模型・キャラ・髪型・髪の色・部品・武器）
  const hairstyle = config.appearance.hairstyle, hairColor = config.appearance.hairColor, partsKey = JSON.stringify(config.appearance.parts ?? null);
  useEffect(() => {
    if (!stage) return;
    let cancelled = false;
    setInfo(null); setError(''); setReady(false);
    subject.current = null;
    latest.current.onReady?.(false);
    const p = latest.current;
    originalSpec(stage, p.character, p.config).then(spec => {
      if (cancelled) return;
      const made = JSON.parse(stage.assemble(JSON.stringify(spec))) as Info;
      const q = latest.current, c = q.config, attackRevision = q.attackRevision ?? 0;
      const last = playback.current;
      const resume = last && last.character === q.character.id && last.motion === c.motion && last.attackRevision === attackRevision ? last.time : undefined;
      const s = q.seek ?? INITIAL_SEEK;
      const first = appliedSeek.current !== s.revision;
      appliedSeek.current = s.revision;
      stage.play(JSON.stringify({ speed: c.speed, playing: c.playing, motion: c.motion, attackRevision, resume, ...(first ? { seek: s.value } : {}), hurtFace: q.hurtFace ?? false, weaponVisible: q.framing !== 'face' }));
      seenMotion.current = `${c.motion}:${attackRevision}`;
      seenPlaying.current = `${c.playing}`;
      const key = `${Date.now()}:${Math.random()}`;
      subject.current = {
        key, token,
        bounds: () => { const b = JSON.parse(stage.query('{"q":"bounds"}')) as { body: number[][] | null; head: number[][] | null; unarmed: number[][] | null }; return b.body ? { body: box(b.body), head: box(b.head), unarmed: box(b.unarmed) } : null; },
        head: () => { const m = JSON.parse(stage.query('{"q":"head"}')) as number[] | null; return m ? new Matrix4().fromArray(m) : null; },
        armed: () => { const a = JSON.parse(stage.query('{"q":"armed","names":["sword_idle"]}')) as { min: number[]; max: number[]; bodyMin: number[]; bodyMax: number[] } | null; return a ? { envelope: new Box3(v3(a.min), v3(a.max)), body: new Box3(v3(a.bodyMin), v3(a.bodyMax)) } : null; },
      };
      // 床の印の大きさ: 読み込んだ姿（動作の最初のコマ）の、武器を除いた立った高さ（大きさ 1。今の版の restHeight）
      const measured = subject.current.bounds();
      if (measured) { const stand = measured.unarmed.isEmpty() ? measured.body : measured.unarmed; made.restHeight = (stand.max.y - stand.min.y) / Math.max(c.size, 1e-3); }
      setInfo(made);
      paintFrames.current = 0;
    }).catch(reason => {
      if (cancelled) return;
      const message = reason instanceof Error ? reason.message : String(reason || '3Dモデルを読み込めませんでした。');
      if (studio && retried.current !== baseToken) {
        retried.current = baseToken;
        console.warn('[studio] model load failed, retrying once:', message);
        forgetModel(asset.url);
        retryTimer.current = window.setTimeout(() => setAttempt(value => value + 1), 500);
        return;
      }
      if (studio) console.warn('[studio] model load failed:', message);
      setError(message);
      latest.current.onReady?.(false);
      latest.current.onError?.(message);
    });
    return () => { cancelled = true; };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [stage, token, hairstyle, hairColor, partsKey]);

  // 作り直さずに当てる設定
  const configKey = JSON.stringify(originalStageConfig(config));
  useEffect(() => { if (stage && info) stage.apply(configKey); }, [stage, info, configKey]);
  // 動作（作り直した直後は assemble の後で送った）
  const seenMotion = useRef(''), seenPlaying = useRef('');
  const attackRevision = props.attackRevision ?? 0;
  useEffect(() => {
    if (!stage || !info) return;
    const key = `${config.motion}:${attackRevision}`;
    if (seenMotion.current === key) return;
    seenMotion.current = key;
    stage.play(JSON.stringify({ motion: config.motion, attackRevision }));
  }, [stage, info, config.motion, attackRevision]);
  useEffect(() => {
    if (!stage || !info) return;
    const key = `${config.playing}`;
    stage.play(JSON.stringify(seenPlaying.current === key ? { speed: config.speed } : { playing: config.playing, speed: config.speed }));
    seenPlaying.current = key;
  }, [stage, info, config.playing, config.speed]);
  useEffect(() => { if (stage && info) stage.play(JSON.stringify({ hurtFace: props.hurtFace ?? false })); }, [stage, info, props.hurtFace]);
  useEffect(() => {
    if (!stage || !info || appliedSeek.current === seek.revision) return;
    appliedSeek.current = seek.revision;
    stage.play(JSON.stringify({ seek: seek.value }));
  }, [stage, info, seek]);
  // のけぞりを 1 回（値が変わるたびに最初から。最初の値では再生しない）
  const seenHit = useRef(props.hitRevision ?? 0);
  useEffect(() => {
    const revision = props.hitRevision ?? 0;
    if (seenHit.current === revision) return;
    seenHit.current = revision;
    if (stage && info) stage.play(JSON.stringify({ hit: true }));
    else latest.current.onHitComplete?.();
  }, [stage, info, props.hitRevision]);
  // 顔の表示では登録された武器を隠す（setRegisteredWeaponsVisible）
  useEffect(() => { if (stage && info) stage.play(JSON.stringify({ weaponVisible: framing !== 'face' })); }, [stage, info, framing]);

  // 描いてから 2 コマ目で「準備できた」（今の版の onPaint）
  const paintFrames = useRef(-1);
  const [appeared, setAppeared] = useState(false);
  const restHeight = info?.restHeight ?? 0;

  // 毎コマ: 動き → 回す操作 → カメラ → 床 → 描く
  useEffect(() => {
    if (!stage) return;
    let raf = 0, last = performance.now();
    const cam = new Float64Array(11);
    const floor = new StageFloorMotion();
    let floorKey = '';
    const tick = (now: number) => {
      raf = requestAnimationFrame(tick);
      const delta = Math.max(0, (now - last) / 1000); last = now;
      const el = canvas.current, r = rig.current, s = subject.current, p = latest.current;
      if (!el || !r) return;
      try {
        stage.step(delta);
        const events = stage.events();
        if (events !== '[]') for (const e of JSON.parse(events) as { t: string; time?: number; duration?: number }[]) {
          if (e.t === 'motionComplete') p.onAttackComplete?.();
          else if (e.t === 'hitComplete') p.onHitComplete?.();
          else if (e.t === 'report') {
            playback.current = { character: p.character.id, motion: p.config.motion, attackRevision: p.attackRevision ?? 0, time: e.time ?? 0 };
            p.report?.(e.time ?? 0, e.duration ?? 0);
          }
        }
        r.beforeFrame();
        const c = p.config, armed = c.motion.startsWith('sword_');
        const wid = selectedMakerWeaponId(c);
        const camProps = {
          framing: p.framing, view: p.view, revision: p.cameraRevision ?? 0, scale: c.size, sourceToken: s?.token ?? '',
          boundsRevision: `${wid}:${c.appearance.build}:${armed && p.framing === 'full' ? 'armed' : `${c.motion}:${c.headTurn}:${(p.seek ?? INITIAL_SEEK).revision}:${p.attackRevision ?? 0}`}`,
          armedKey: `${s?.token ?? ''}:${c.appearance.build}`, playing: c.playing && !armed && c.motion !== 'idle', armed,
          width: el.clientWidth, height: el.clientHeight,
        };
        r.layout(s, camProps);
        r.frame(s, camProps, delta);
        if (s && s.key !== floorKey) { floorKey = s.key; floor.reset(); }
        floor.frame(stage, { hips: null, scale: c.size, restHeight: s ? restHeightRef.current : 0, accent: p.accent, pulseKey: `${p.view}:${p.framing}`, visible: appearedRef.current, delta });
        r.write(cam);
        if (process.env.NODE_ENV !== 'production') (window as Window & { __originalCam?: number[] }).__originalCam = Array.from(cam);
        stage.draw(cam);
        if (s && paintFrames.current >= 0 && ++paintFrames.current === 2) {
          paintFrames.current = -1;
          setAppeared(true); setReady(true);
          latest.current.onReady?.(true);
        }
      } catch (e) { markUnsupported(e); }
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [stage]);
  const restHeightRef = useRef(0), appearedRef = useRef(false);
  restHeightRef.current = restHeight;
  appearedRef.current = appeared;

  const retry = () => { forgetModel(asset.url); setAttempt(value => value + 1); };
  return <div ref={root} data-stage-canvas="" data-stage-engine="rust" data-camera-settled="false" style={{ position: 'absolute', inset: 0, minHeight: 0 }} {...doubleTap}>
    <canvas ref={canvas} style={{ display: 'block', width: '100%', height: '100%', touchAction: 'none' }} />
    {!ready && (error && studio
      ? <StageError onRetry={retry} />
      : error
      ? <div role="alert" style={{ position: 'absolute', inset: 0, display: 'grid', alignContent: 'center', justifyItems: 'center', gap: 12, padding: 28, textAlign: 'center', background: 'color-mix(in oklab, var(--paper, #F7F3EC) 88%, transparent)', color: 'var(--ink, #17151C)', font: '600 14px/1.5 var(--font-ui, system-ui, sans-serif)' }}>
        <span>{error}</span>
        <button type="button" onClick={retry} style={{ minHeight: 40, padding: '0 20px', borderRadius: 999, border: 0, background: 'var(--ink, #17151C)', color: '#fff', font: 'inherit', cursor: 'pointer', boxShadow: '0 2px 0 #000' }}>もう一度読み込む</button>
      </div>
      : <div role="status" style={SR_ONLY}>キャラクターを準備しています…</div>)}
  </div>;
}

const SR_ONLY = { position: 'absolute', width: 1, height: 1, padding: 0, margin: -1, overflow: 'hidden', clip: 'rect(0,0,0,0)', whiteSpace: 'nowrap', border: 0 } as const;
