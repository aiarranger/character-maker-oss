'use client';

import { Component, Suspense, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type ComponentRef, type ReactNode, type RefObject } from 'react';
import { Canvas, useFrame, useThree } from '@react-three/fiber';
import { OrbitControls, useGLTF } from '@react-three/drei';
import { Box3, OrthographicCamera, Vector3, type AnimationClip, type Object3D } from 'three';
import { measureOriginalArmedCameraBounds, measureOriginalCameraBounds } from './original-camera';
import { STAGE_ARMED_LAYOUT, StageCameraTween, fitStageFace, fitStageFull, markCameraSettled, prefersInstantCamera, setRegisteredWeaponsVisible, stageDirection, useStageDoubleTap } from './stage-camera';
import { StageFloor } from './StageFloor';
import { OriginalEditableActor } from './OriginalEditableActor';
import { resolveOriginalProfile } from '../../lib/character-maker/original-assembly';
import { selectedMakerWeaponId } from '../../lib/character-maker/weapon-catalog';
import { retryMakerWeapon } from '../../lib/character-maker/registered-weapon';
import { ORIGINAL_SIZE_RANGE, originalModelAsset, type OriginalCharacter, type OriginalConfig } from '../../lib/character-maker/original-config';

type Props = {
  character: OriginalCharacter;
  config: OriginalConfig;
  attackRevision?: number;
  /** のけぞりを1回再生する（値が変わるたび）。docs/main-character-spec.md */
  hitRevision?: number;
  onHitComplete?: () => void;
  /** ＞＜ の顔を出し続ける（のけぞり中は自動で出る） */
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
  /** Studio accent colour (floor ring). Falls back to the CSS custom property default. */
  accent?: string;
  /** Double-click / double-tap on the stage toggles full ⇄ face (spec §5.3). */
  onFramingToggle?: () => void;
  /** Studio stage: retry a failed load once and show the plain-Japanese error board (spec §5.5). */
  studio?: boolean;
};

type Profile = ReturnType<typeof resolveOriginalProfile>;
type CameraSubject = { root: Object3D; head: Object3D | null; token: string; clips: readonly AnimationClip[] };
const INITIAL_SEEK = { value: 0, revision: 0 };

class ModelBoundary extends Component<{ children: ReactNode; onError: (message: string) => void }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(error: Error) { this.props.onError(error.message || '3Dモデルを読み込めませんでした。'); }
  render() { return this.state.failed ? null : this.props.children; }
}

function OriginalModel({ character, config, profile, seek, report, onPaint, onModel, attackRevision, onAttackComplete, hitRevision, onHitComplete, hurtFace }: {
  character: OriginalCharacter; config: OriginalConfig; profile: Profile;
  seek: NonNullable<Props['seek']>; report: NonNullable<Props['report']>; onPaint: () => void;
  onModel: (model: Object3D, clips: readonly AnimationClip[]) => void | (() => void);
  attackRevision?: number; onAttackComplete?: () => void;
  hitRevision?: number; onHitComplete?: () => void; hurtFace?: boolean;
}) {
  const ready = useRef(false), frames = useRef(0), painted = useRef(false), request = useRef(0);
  const afterPaint = useRef(onPaint);
  afterPaint.current = onPaint;
  const onLoaded = useCallback(() => { ready.current = true; }, []);
  useEffect(() => () => { if (request.current) cancelAnimationFrame(request.current); }, []);
  useFrame(() => {
    if (!ready.current || painted.current || ++frames.current < 2) return;
    painted.current = true;
    request.current = requestAnimationFrame(() => afterPaint.current());
  });
  return <group scale={config.size} dispose={null}>
    <OriginalEditableActor weaponId={selectedMakerWeaponId(config)} character={character.id} profile={profile} bodyShape={config.appearance.build} appearance={config.appearance} motion={config.motion} playing={config.playing} speed={config.speed}
      face={config.face} autoBlink={config.autoBlink} speaking={config.speaking} hairMotion={config.hairMotion}
      headTurn={config.headTurn} seek={seek} report={report} onReady={onLoaded} onModel={onModel} attackRevision={attackRevision} onAttackComplete={onAttackComplete} hitRevision={hitRevision} onHitComplete={onHitComplete} hurtFace={hurtFace} smoothBody />
  </group>;
}

// Armed full-view bounds sample whole clips on a cloned skeleton (up to ~1 s of
// CPU). They depend only on (character, weapon, build, model token), so view,
// framing, canvas-size and stance ⇄ swing changes reuse them: one composition for
// both armed motions, so switching never re-zooms or moves the feet. It is the
// stance's range; the stance+swing union (blade raised overhead) would shrink a
// 1440 stance to ≈48% of the canvas (spec §5.3 asks ≥56%), so the blade may pass
// the canvas edges for a few frames around the swing apex. Stored at size 1, with and without the weapon.
const ARMED_FRAME_CLIPS = ['sword_idle'];
const armedBoundsCache = new Map<string, { envelope: Box3; body: Box3 }>();
function cachedArmedBounds(key: string, subject: CameraSubject, scale: number, envelope: Box3, body: Box3) {
  let cached = armedBoundsCache.get(key);
  if (!cached) {
    const names = ARMED_FRAME_CLIPS;
    if (!measureOriginalArmedCameraBounds(subject.root, subject.clips, envelope, names, body)) return false;
    const unit = (box: Box3) => { const stored = box.clone(); stored.min.divideScalar(scale); stored.max.divideScalar(scale); return stored; };
    cached = { envelope: unit(envelope), body: body.isEmpty() ? unit(envelope) : unit(body) };
    armedBoundsCache.set(key, cached);
    if (armedBoundsCache.size > 48) armedBoundsCache.delete(armedBoundsCache.keys().next().value!);
  }
  envelope.copy(cached.envelope); envelope.min.multiplyScalar(scale); envelope.max.multiplyScalar(scale);
  body.copy(cached.body); body.min.multiplyScalar(scale); body.max.multiplyScalar(scale);
  return true;
}

const VIEW_AZIMUTH = { front: 0, side: -Math.PI / 2, back: Math.PI } as const;

function OriginalCamera({ framing, view, revision, scale, subject, sourceToken, boundsRevision, armedKey, playing, armed, onCameraInteraction, root }: {
  framing: Props['framing']; view: Props['view']; revision: number; scale: number;
  subject: CameraSubject | null; sourceToken: string; boundsRevision: string; armedKey: string; playing: boolean; armed: boolean; onCameraInteraction?: () => void;
  root: RefObject<HTMLDivElement | null>;
}) {
  const controls = useRef<ComponentRef<typeof OrbitControls>>(null);
  const { camera, size } = useThree();
  const pendingReset = useRef(true), custom = useRef(false), interacting = useRef(false), programmatic = useRef(false);
  const bounds = useMemo(() => ({ body: new Box3(), head: new Box3(), envelope: new Box3(), frame: new Box3(), unarmed: new Box3(), centre: new Box3() }), []);
  const anchorLocal = useRef(new Vector3()), followed = useRef(new Vector3()), nextAnchor = useRef(new Vector3());
  const translation = useRef(new Vector3());
  const previousScale = useRef(scale);
  const sampleElapsed = useRef(0), pendingMeasure = useRef(true);
  const tween = useMemo(() => new StageCameraTween(), []);
  const animateNext = useRef(false), baseZoom = useRef(1);
  const previous = useRef<{ subject: CameraSubject | null; sourceToken: string; framing: string; view: string; revision: number; stance: string; width: number; height: number } | null>(null);
  // Stance and swing share one armed composition: only arming / disarming re-fits.
  const stance = armed ? 'armed' : '';
  useLayoutEffect(() => { pendingMeasure.current = true; }, [boundsRevision, scale]);
  // Face view is a portrait: registered weapons are hidden on screen only.
  useLayoutEffect(() => {
    setRegisteredWeaponsVisible(subject?.root, framing !== 'face');
    return () => setRegisteredWeaponsVisible(subject?.root, true);
  }, [subject, framing]);
  useLayoutEffect(() => {
    // Loaded model identity is a dependency: Suspense may resolve long after the
    // first camera effect, including after a source/retry change.
    const last = previous.current;
    // Only user camera requests (view, framing, or a view button press that
    // bumps the revision) arc; loads, resizes and the first framing cut.
    animateNext.current = !!last && last.subject === subject && last.sourceToken === sourceToken && last.width === size.width && last.height === size.height
      && (last.framing !== framing || last.view !== view || last.revision !== revision || last.stance !== stance) && !prefersInstantCamera();
    previous.current = { subject, sourceToken, framing, view, revision, stance, width: size.width, height: size.height };
    if (!animateNext.current) tween.stop();
    pendingReset.current = true;
    custom.current = false;
    bounds.envelope.makeEmpty();
    markCameraSettled(root.current, false);
  }, [subject, sourceToken, framing, view, revision, armed, stance, size.width, size.height, bounds, tween, root]);
  useFrame((_, delta) => {
    if (!subject || subject.token !== sourceToken || !(camera instanceof OrthographicCamera)) return;
    const orbit = controls.current;
    if (!orbit) return;
    const updateControls = () => {
      programmatic.current = true;
      try { orbit.update(); } finally { programmatic.current = false; }
    };
    const followFace = () => {
      if (framing === 'face' && subject.head) {
        // Head following is cheap and remains smooth between bounds samples,
        // including when a user has manually orbited or panned the face.
        subject.root.updateWorldMatrix(true, true);
        nextAnchor.current.copy(anchorLocal.current);
        subject.head.localToWorld(nextAnchor.current);
        translation.current.copy(nextAnchor.current).sub(followed.current);
        camera.position.add(translation.current); orbit.target.add(translation.current);
        tween.translate(translation.current);
        followed.current.copy(nextAnchor.current);
        if (previousScale.current !== scale) { camera.zoom *= previousScale.current / scale; camera.updateProjectionMatrix(); }
        updateControls();
      }
      previousScale.current = scale;
    };
    const stepTween = () => {
      if (!tween.active) return;
      programmatic.current = true;
      try {
        const settled = tween.step(camera, orbit.target, delta);
        camera.updateProjectionMatrix(); orbit.update();
        if (settled) { orbit.minZoom = baseZoom.current * .6; orbit.maxZoom = baseZoom.current * 4; markCameraSettled(root.current, true); }
      } finally { programmatic.current = false; }
    };
    if (custom.current && !pendingReset.current) { followFace(); stepTween(); return; }
    sampleElapsed.current += delta;
    if (!pendingReset.current && !pendingMeasure.current && (!playing || sampleElapsed.current < .5)) { followFace(); stepTween(); return; }
    // Sample live skin/morph bounds at most 2 Hz, immediately after explicit
    // pose/size edits. Padding reserves room for motion between samples; the growing pose
    // envelope stabilizes after a cycle without per-render CPU skinning.
    // Armed full views are fitted once to the stance clip. Chasing the moving sword
    // with the camera makes a planted character appear to slide before a swing.
    const measured = armed && framing === 'full'
      ? cachedArmedBounds(armedKey, subject, scale, bounds.body, bounds.unarmed)
      : measureOriginalCameraBounds(subject.root, subject.head, bounds.body, bounds.head);
    if (!measured) return;
    sampleElapsed.current = 0; pendingMeasure.current = false;
    const resetting = pendingReset.current;
    if (resetting) {
      bounds.envelope.makeEmpty();
      bounds.head.getCenter(followed.current);
      anchorLocal.current.copy(followed.current);
      subject.head?.worldToLocal(anchorLocal.current);
    }
    const direction = stageDirection(VIEW_AZIMUTH[view]);
    let fit;
    if (framing === 'face') {
      nextAnchor.current.copy(anchorLocal.current);
      if (subject.head) subject.head.localToWorld(nextAnchor.current);
      else bounds.head.getCenter(nextAnchor.current);
      // Accumulate head-relative bounds, not world positions, so animation
      // follows the face without repeatedly zooming out along its motion path.
      bounds.frame.copy(bounds.head);
      bounds.frame.min.sub(nextAnchor.current).divideScalar(scale);
      bounds.frame.max.sub(nextAnchor.current).divideScalar(scale);
      bounds.envelope.union(bounds.frame);
      bounds.frame.copy(bounds.envelope);
      bounds.frame.min.multiplyScalar(scale).add(nextAnchor.current);
      bounds.frame.max.multiplyScalar(scale).add(nextAnchor.current);
      followed.current.copy(nextAnchor.current);
      fit = fitStageFace(bounds.frame, bounds.frame.getCenter(new Vector3()), direction, size.width, size.height);
    } else {
      // Frame at maximum permitted size to preserve visible size comparisons.
      // The pose envelope only grows, avoiding breathing/zoom pumping per frame.
      const swing = armed && framing === 'full';
      const reference = ORIGINAL_SIZE_RANGE.max;
      bounds.frame.copy(bounds.body);
      bounds.frame.min.multiplyScalar(reference / scale);
      bounds.frame.max.multiplyScalar(reference / scale);
      bounds.envelope.union(bounds.frame);
      bounds.frame.copy(bounds.envelope);
      if (swing) {
        // Centre the body, not the sword arc, as long as the arc still fits.
        bounds.centre.copy(bounds.unarmed);
        bounds.centre.min.multiplyScalar(reference / scale);
        bounds.centre.max.multiplyScalar(reference / scale);
      }
      fit = swing
        ? fitStageFull(bounds.frame, direction, size.width, size.height, STAGE_ARMED_LAYOUT.horizontal, bounds.centre)
        : fitStageFull(bounds.frame, direction, size.width, size.height);
    }
    const goal = { target: fit.target, position: fit.target.clone().addScaledVector(direction, fit.distance), zoom: fit.zoom };
    camera.near = .01; camera.far = fit.distance * 3 + 10; camera.up.set(0, 1, 0);
    baseZoom.current = fit.zoom;
    orbit.minZoom = fit.zoom * .6; orbit.maxZoom = fit.zoom * 4;
    if (resetting && animateNext.current) { tween.start(goal); animateNext.current = false; }
    if (tween.active) {
      // Controls clamp zoom on every update; let the arc pass between both framings.
      orbit.minZoom = Math.min(camera.zoom, fit.zoom) * .6; orbit.maxZoom = Math.max(camera.zoom, fit.zoom) * 4;
      tween.goal!.target.copy(goal.target); tween.goal!.position.copy(goal.position); tween.goal!.zoom = goal.zoom;
      stepTween();
    } else {
      camera.position.copy(goal.position);
      camera.lookAt(goal.target);
      camera.zoom = goal.zoom;
      camera.updateProjectionMatrix();
      orbit.target.copy(goal.target); updateControls();
      markCameraSettled(root.current, true);
    }
    previousScale.current = scale;
    pendingReset.current = false;
  });
  return <OrbitControls ref={controls} makeDefault enableDamping dampingFactor={.08} enablePan={false} zoomToCursor minPolarAngle={.2} maxPolarAngle={Math.PI * .85}
    onStart={() => { interacting.current = true; tween.stop(); markCameraSettled(root.current, true); }} onEnd={() => { interacting.current = false; }}
    onChange={() => {
      if (!interacting.current || programmatic.current || pendingReset.current || custom.current) return;
      custom.current = true;
      onCameraInteraction?.();
    }} />;
}

export default function OriginalCharacterScene(props: Props) {
  const { character, config, framing, view, cameraRevision = 0, modelRevision = 0, seek = INITIAL_SEEK } = props;
  const latest = useRef(props);
  latest.current = props;
  const [attempt, setAttempt] = useState(0);
  const asset = originalModelAsset(character, config.appearance);
  const weaponId = selectedMakerWeaponId(config);
  const studio = props.studio ?? false;
  const baseToken = `${weaponId}:${character.id}:${character.modelSha256}:${asset.sha256}:${modelRevision}`;
  const token = `${baseToken}:${attempt}`;
  const retried = useRef(''), retryTimer = useRef(0);
  useEffect(() => () => window.clearTimeout(retryTimer.current), []);
  const currentToken = useRef(token);
  currentToken.current = token;
  const [status, setStatus] = useState({ token: '', ready: false, error: '' });
  const profile = useMemo(() => resolveOriginalProfile(character, config.appearance), [character, asset.url, asset.sha256, config.appearance.version]);
  const [subject, setSubject] = useState<CameraSubject | null>(null);
  const onModel = useCallback((model: Object3D, clips: readonly AnimationClip[]) => {
    const next = { root: model, head: model.getObjectByName('Head') ?? model.getObjectByName('J_Bip_C_Head') ?? null, token: currentToken.current, clips };
    setSubject(next);
    return () => { setSubject(current => current === next ? null : current); };
  }, []);
  useLayoutEffect(() => { latest.current.onReady?.(false); }, [token]);
  const report = useCallback((time: number, duration: number) => latest.current.report?.(time, duration), []);
  const onHitComplete = useCallback(() => latest.current.onHitComplete?.(), []);
  const onPaint = useCallback(() => {
    if (currentToken.current !== token) return;
    setStatus({ token, ready: true, error: '' });
    latest.current.onReady?.(true);
  }, [token]);
  const onError = useCallback((message: string) => {
    if (currentToken.current !== token) return;
    if (studio && retried.current !== baseToken) {
      retried.current = baseToken;
      console.warn('[studio] model load failed, retrying once:', message);
      useGLTF.clear(asset.url);
      retryMakerWeapon(weaponId);
      retryTimer.current = window.setTimeout(() => setAttempt(value => value + 1), 500);
      return;
    }
    if (studio) console.warn('[studio] model load failed:', message);
    setStatus({ token, ready: false, error: message });
    latest.current.onReady?.(false);
    latest.current.onError?.(message);
  }, [token, studio, baseToken, asset.url, weaponId]);
  const ready = status.token === token && status.ready;
  const error = status.token === token ? status.error : '';
  const root = useRef<HTMLDivElement>(null);
  const [appeared, setAppeared] = useState(false);
  useEffect(() => { if (ready) setAppeared(true); }, [ready]);
  // Floor mark scale: the standing height without the weapon, measured once per loaded model, at size 1.
  const [restHeight, setRestHeight] = useState(0);
  useEffect(() => {
    if (!subject) return;
    const body = new Box3(), head = new Box3(), unarmed = new Box3();
    if (!measureOriginalCameraBounds(subject.root, null, body, head, unarmed)) return;
    const measured = unarmed.isEmpty() ? body : unarmed;
    setRestHeight((measured.max.y - measured.min.y) / Math.max(latest.current.config.size, 1e-3));
  }, [subject]);
  const doubleTap = useStageDoubleTap(() => latest.current.onFramingToggle?.());
  return <div ref={root} data-stage-canvas="" data-camera-settled="false" style={{ position: 'absolute', inset: 0, minHeight: 0 }} {...doubleTap}>
    <Canvas orthographic camera={{ position: [0, 1.04, 5], near: .01, far: 30, zoom: 250 }} dpr={[1, 2]} gl={{ antialias: true, alpha: true, preserveDrawingBuffer: true }}>
      {/* Existing lights only reach Accessory_ materials kept as authored; characters stay unlit. */}
      <ambientLight intensity={1} /><directionalLight position={[3, 5, 4]} intensity={2.5} /><directionalLight position={[-3, 2, -2]} intensity={.6} />
      <ModelBoundary key={token} onError={onError}>
        <Suspense fallback={null}>
          <OriginalModel character={character} config={config} profile={profile} seek={seek} report={report} onPaint={onPaint} onModel={onModel} attackRevision={props.attackRevision} onAttackComplete={props.onAttackComplete} hitRevision={props.hitRevision} onHitComplete={onHitComplete} hurtFace={props.hurtFace} />
        </Suspense>
      </ModelBoundary>
      <OriginalCamera root={root} framing={framing} view={view} revision={cameraRevision} scale={config.size} subject={subject} sourceToken={token} armed={config.motion.startsWith('sword_')} armedKey={`${token}:${config.appearance.build}`} playing={config.playing && !config.motion.startsWith('sword_') && config.motion !== 'idle'} boundsRevision={`${weaponId}:${config.appearance.build}:${config.motion.startsWith('sword_') && framing === 'full' ? 'armed' : `${config.motion}:${config.headTurn}:${seek.revision}:${props.attackRevision ?? 0}`}`} onCameraInteraction={props.onCameraInteraction} />
      <StageFloor subject={subject?.root ?? null} scale={config.size} restHeight={restHeight} accent={props.accent} pulseKey={`${view}:${framing}`} visible={appeared} />
    </Canvas>
    {!ready && (error && studio
      ? <StageError onRetry={() => { useGLTF.clear(asset.url); retryMakerWeapon(weaponId); setAttempt(value => value + 1); }} />
      : error
      ? <div role="alert" style={{ position: 'absolute', inset: 0, display: 'grid', alignContent: 'center', justifyItems: 'center', gap: 12, padding: 28, textAlign: 'center', background: 'color-mix(in oklab, var(--paper, #F7F3EC) 88%, transparent)', color: 'var(--ink, #17151C)', font: '600 14px/1.5 var(--font-ui, system-ui, sans-serif)' }}>
        <span>{error}</span>
        <button type="button" onClick={() => { useGLTF.clear(asset.url); retryMakerWeapon(weaponId); setAttempt(value => value + 1); }} style={{ minHeight: 40, padding: '0 20px', borderRadius: 999, border: 0, background: 'var(--ink, #17151C)', color: '#fff', font: 'inherit', cursor: 'pointer', boxShadow: '0 2px 0 #000' }}>もう一度読み込む</button>
      </div>
      : <div role="status" style={SR_ONLY}>キャラクターを準備しています…</div>)}
  </div>;
}

/** Studio load failure (spec §5.5): one plain-Japanese board; raw URLs and English stay in console.warn. */
function StageError({ onRetry }: { onRetry: () => void }) {
  return <div role="alert" data-stage-error="" style={{ position: 'absolute', inset: 0, display: 'grid', alignContent: 'center', justifyItems: 'center', gap: 12, padding: '96px 28px 28px', maxWidth: '100%', boxSizing: 'border-box', overflowWrap: 'anywhere', textAlign: 'center', background: 'color-mix(in oklab, var(--paper, #F7F3EC) 88%, transparent)', color: 'var(--ink, #17151C)', font: '600 var(--t-14, 14px)/1.5 var(--font-ui, system-ui, sans-serif)' }}>
    <strong style={{ font: '700 var(--t-16, 16px)/1.4 var(--font-ui, system-ui, sans-serif)' }}>キャラクターを読み込めませんでした</strong>
    <span>通信を確かめて、もう一度読み込んでください。</span>
    <button type="button" onClick={onRetry} style={{ minHeight: 44, maxWidth: '100%', boxSizing: 'border-box', padding: '10px 22px', borderRadius: 999, border: 0, background: 'var(--ink, #17151C)', color: '#fff', font: 'inherit', overflowWrap: 'anywhere', cursor: 'pointer', boxShadow: '0 2px 0 #000' }}>もう一度読み込む</button>
  </div>;
}

const SR_ONLY = { position: 'absolute', width: 1, height: 1, padding: 0, margin: -1, overflow: 'hidden', clip: 'rect(0,0,0,0)', whiteSpace: 'nowrap', border: 0 } as const;
