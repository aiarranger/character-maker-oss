'use client';

import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ComponentRef, type RefObject } from 'react';
import { Canvas, useFrame, useThree } from '@react-three/fiber';
import { OrbitControls } from '@react-three/drei';
import { AnimationMixer, Box3, LoopOnce, LoopRepeat, PerspectiveCamera, PropertyBinding, Quaternion, Vector3, type AnimationAction, type Interpolant, type Object3D } from 'three';
import { HIT_CLIP, HURT_FACE_CLIP } from '../../lib/character-maker/hurt';
import { measureOriginalArmedCameraBounds, measureOriginalCameraBounds } from './original-camera';
import { HEIGHT_RANGE, type CharacterConfig } from '../../lib/character-maker/config';
import { assembleCharacter, type MakerInstance } from '../../lib/character-maker/assembly';
import { selectedMakerWeaponId } from '../../lib/character-maker/weapon-catalog';
import { retryMakerWeapon } from '../../lib/character-maker/registered-weapon';
import { makerArmedCameraDistance } from '../../lib/character-maker/armed-camera';
import { StageCameraTween, markCameraSettled, projectBox, stageFaceWindow, prefersInstantCamera, setRegisteredWeaponsVisible, stageDirection, useStageDoubleTap } from './stage-camera';
import { StageFloor } from './StageFloor';

/** Studio lens for presets (spec §5.3): fov 28, sized to match the originals' stage occupancy. */
const STUDIO_FOV = 28;
const STUDIO_HALF_TAN = Math.tan((STUDIO_FOV / 2) * Math.PI / 180);
/** Height fraction of the canvas a height-1.0 preset fills in the full view (originals measure ≈.67-.69). */
const STUDIO_FULL_OCCUPANCY = .645;
/** Feet line above the canvas bottom (full / armed). */
const STUDIO_FEET = { full: .085, armed: .085 };
/** Headroom above the armed envelope. */
const STUDIO_ARMED_TOP = .015;
/** Pulls the armed preset back so a height-1.0 preset matches the originals' armed occupancy (≈.64, ±5%). */
const STUDIO_ARMED_MATCH = 1.1;

// Armed envelopes sample a whole clip on a cloned skeleton: once per instance.
// Stance and swing share the stance's composition (as the originals do), so
// switching between them never re-zooms or moves the feet. Stored at height 1.
const presetArmedCache = new WeakMap<MakerInstance, Box3>();
function presetArmedBounds(instance: MakerInstance, height: number) {
  let unit = presetArmedCache.get(instance);
  if (!unit) {
    const box = new Box3();
    if (!measureOriginalArmedCameraBounds(instance.root, instance.animations, box, ['sword_idle'])) return null;
    box.min.divideScalar(height); box.max.divideScalar(height);
    unit = box;
    presetArmedCache.set(instance, unit);
  }
  return unit.clone();
}
const STUDIO_AZIMUTH = { front: 0, side: Math.PI / 2, back: Math.PI } as const;

type Props = {
  config: CharacterConfig;
  modelRevision?: number;
  framing: 'full' | 'face';
  view: 'front' | 'side' | 'back';
  motion: 'idle' | 'walk' | 'run' | 'sword_idle' | 'sword_attack';
  motionRevision?: number;
  /** のけぞりを1回再生する（値が変わるたび）。docs/main-character-spec.md */
  hitRevision?: number;
  onHitComplete?: () => void;
  /** ＞＜ の顔を出し続ける（のけぞり中は自動で出る） */
  hurtFace?: boolean;
  playing?: boolean; speed?: number;
  onMotionComplete?: () => void;
  cameraRevision?: number;
  onReady?: (ready: boolean) => void;
  onError?: (message: string) => void;
  /** Studio presentation (spec §5). Defaults to false: classic stays unchanged. */
  studio?: boolean;
  accent?: string;
  onFramingToggle?: () => void;
};

/** のけぞりの重み：入りは素早く、最後は下の動作へなめらかに戻す（秒） */
const HIT_FADE_IN = 0.06, HIT_FADE_OUT = 0.2;
const smoothstep = (edge0: number, edge1: number, x: number) => { const t = Math.max(0, Math.min(1, (x - edge0) / (edge1 - edge0))); return t * t * (3 - 2 * t); };
type HitBinding = { target: Object3D; property: 'quaternion' | 'position'; interpolant: Interpolant; rest: number[] | null };

/** のけぞりは今の動作を止めずに上から重ねる（下の動作の重みは 1−h）。体の骨だけを動かし、武器の表示は下の動作のまま。 */
function hitLayer(instance: MakerInstance) {
  const clip = instance.animations.find(item => item.name === HIT_CLIP);
  if (!clip) return null;
  // 下の動作が動かさない骨は、毎フレーム元の姿へ戻してから重ねる（重ねた値が残らないように）。
  const animated = new Set(instance.animations.filter(item => item.name !== HIT_CLIP && item.name !== HURT_FACE_CLIP).flatMap(item => item.tracks.map(track => track.name)));
  const bindings: HitBinding[] = [];
  for (const track of clip.tracks) {
    const { nodeName, propertyName } = PropertyBinding.parseTrackName(track.name);
    if (propertyName !== 'quaternion' && propertyName !== 'position') continue;
    const target = PropertyBinding.findNode(instance.root, nodeName) as Object3D | undefined;
    if (!target) continue;
    bindings.push({ target, property: propertyName, interpolant: (track as unknown as { createInterpolant: () => Interpolant }).createInterpolant(),
      rest: animated.has(track.name) ? null : target[propertyName].toArray() });
  }
  return { duration: clip.duration, bindings };
}

function Model({ instance, config, motion, motionRevision, onMotionComplete, playing, speed, hitRevision, onHitComplete, hurtFace }: {
  instance: MakerInstance; config: CharacterConfig; motion: Props['motion'];
  motionRevision: number; onMotionComplete?: () => void; playing: boolean; speed: number;
  hitRevision: number; onHitComplete?: () => void; hurtFace: boolean;
}) {
  const mixer = useRef<AnimationMixer | null>(null);
  const activeAction = useRef<AnimationAction | null>(null);
  const complete = useRef(onMotionComplete);
  complete.current = onMotionComplete;
  const hitDone = useRef(onHitComplete);
  hitDone.current = onHitComplete;
  const layer = useMemo(() => hitLayer(instance), [instance]);
  const hit = useRef<{ time: number } | null>(null);
  const seenHit = useRef(hitRevision);
  const faceShown = useRef(false);
  const hitQuaternion = useMemo(() => new Quaternion(), []), hitPosition = useMemo(() => new Vector3(), []);
  // 値が変わるたびに最初から（連打は最初から再生し直す）。最初の値では再生しない。
  useEffect(() => {
    if (seenHit.current === hitRevision) return;
    seenHit.current = hitRevision;
    if (layer) hit.current = { time: 0 };
  }, [hitRevision, layer]);
  useEffect(() => () => { faceShown.current = false; instance.setHurtFace(false); }, [instance]);
  useLayoutEffect(() => { instance.apply(config); }, [instance, config]);
  useEffect(() => {
    mixer.current = new AnimationMixer(instance.root);
    return () => {
      mixer.current?.stopAllAction();
      mixer.current?.uncacheRoot(instance.root);
      mixer.current = null;
      activeAction.current = null;
    };
  }, [instance]);
  useEffect(() => {
    const currentMixer = mixer.current;
    const clip = instance.animations.find(item => item.name === motion);
    if (!currentMixer || !clip) return;
    const previous = activeAction.current;
    const action = currentMixer.clipAction(clip);
    // Repeated presses reset the same action; attack is a request, not a loop.
    if (previous === action) action.stop();
    action.reset().setEffectiveTimeScale(1).setEffectiveWeight(1);
    action.setLoop(motion === 'sword_attack' ? LoopOnce : LoopRepeat, motion === 'sword_attack' ? 1 : Infinity);
    action.clampWhenFinished = motion === 'sword_attack';
    action.play();
    if (previous && previous !== action) previous.crossFadeTo(action, 0.16, false);
    activeAction.current = action;
    currentMixer.update(0);
    const finished = (event: { action: AnimationAction }) => {
      if (event.action === action && activeAction.current === action && motion === 'sword_attack') complete.current?.();
    };
    currentMixer.addEventListener('finished', finished);
    return () => { currentMixer.removeEventListener('finished', finished); };
  }, [instance, motion, motionRevision]);
  useFrame((_, delta) => {
    const step = playing ? Math.min(delta, 0.05) * speed : 0;
    const state = hit.current;
    // のけぞり中は止めていても下の動作を書き直す（重ねた値が積み重ならないように）。
    if (playing || state) mixer.current?.update(step);
    if (state && layer) {
      state.time += step;
      const t = Math.min(state.time, layer.duration);
      const weight = smoothstep(0, HIT_FADE_IN, t) * (1 - smoothstep(layer.duration - HIT_FADE_OUT, layer.duration, t));
      for (const binding of layer.bindings) {
        if (binding.rest) binding.target[binding.property].fromArray(binding.rest);
        const value = binding.interpolant.evaluate(t);
        if (binding.property === 'quaternion') binding.target.quaternion.slerp(hitQuaternion.fromArray(value), weight);
        else binding.target.position.lerp(hitPosition.fromArray(value), weight);
      }
      if (state.time >= layer.duration) {
        hit.current = null;
        for (const binding of layer.bindings) if (binding.rest) binding.target[binding.property].fromArray(binding.rest);
        hitDone.current?.();
      }
    }
    // ＞＜：切り替えを押している間と、のけぞっている間。
    const face = hurtFace || !!hit.current;
    if (face !== faceShown.current) { faceShown.current = face; instance.setHurtFace(face); }
    instance.applyEyeGaze();
  });
  return <primitive object={instance.root} dispose={null} />;
}

function Camera({ instance, framing, view, revision, height, armed }: {
  instance: MakerInstance; framing: Props['framing']; view: Props['view']; revision: number; height: number; armed: boolean;
}) {
  const controls = useRef<ComponentRef<typeof OrbitControls>>(null);
  const { camera, size } = useThree();
  const aspect = size.width / Math.max(size.height, 1);
  // Capture the actual rest head in character-local units before the mixer
  // starts. The two rigs have different head heights and root conventions.
  const restHeadY = useMemo(() => {
    const head = instance.root.getObjectByName('J_Bip_C_Head') ?? instance.root.getObjectByName('Head');
    instance.root.updateWorldMatrix(true, true);
    return head
      ? instance.root.worldToLocal(head.getWorldPosition(new Vector3())).y
      : instance.restBounds.top;
  }, [instance]);
  const previous = useRef<{ instance: MakerInstance; framing: Props['framing']; view: Props['view']; revision: number; height: number; armed: boolean; aspect: number } | null>(null);
  useLayoutEffect(() => {
    const last = previous.current;
    const reset = !last || last.instance !== instance || last.framing !== framing || last.view !== view || last.revision !== revision || last.armed !== armed || last.aspect !== aspect;
    previous.current = { instance, framing, view, revision, height, armed, aspect };
    const face = framing === 'face';
    if (!reset) {
      // Height edits preserve the user's orbit and zoom. Portraits follow the
      // head by translation only; full views keep their ground reference fixed.
      if (face && last.height !== height) {
        const deltaY = restHeadY * (height - last.height);
        camera.position.y += deltaY;
        if (controls.current) controls.current.target.y += deltaY;
        controls.current?.update();
      }
      return;
    }
    const referenceHeight = face ? 1 : HEIGHT_RANGE.max;
    const top = instance.restBounds.top * referenceHeight;
    const modelHeight = Math.max(1.3, instance.restBounds.height * referenceHeight);
    // Portrait includes the hair silhouette and shoulders. Full framing leaves
    // room for the tallest character and sword arc at a fixed camera distance.
    const target = new Vector3(0, face ? top - modelHeight * 0.16 + restHeadY * (height - 1) : top * (armed ? 0.61 : 0.51), 0);
    const distance = face ? modelHeight * 0.78 : (armed ? makerArmedCameraDistance(modelHeight, aspect) : modelHeight * 2.05);
    const angle = view === 'side' ? Math.PI / 2 : view === 'back' ? Math.PI : 0;
    camera.position.set(Math.sin(angle) * distance, target.y + (face ? 0 : modelHeight * 0.025), Math.cos(angle) * distance);
    camera.lookAt(target);
    camera.updateProjectionMatrix();
    controls.current?.target.copy(target);
    controls.current?.update();
  }, [instance, framing, view, revision, height, armed, camera, restHeadY, aspect]);
  return <OrbitControls ref={controls} makeDefault enableDamping dampingFactor={0.12} minDistance={0.35} maxDistance={Math.max(8, makerArmedCameraDistance(Math.max(1.3, instance.restBounds.height * HEIGHT_RANGE.max), aspect) * 1.2)} minPolarAngle={0.2} maxPolarAngle={Math.PI * 0.85} />;
}

function StudioCamera({ instance, framing, view, revision, height, armed, root }: {
  instance: MakerInstance; framing: Props['framing']; view: Props['view']; revision: number; height: number; armed: boolean;
  root: RefObject<HTMLDivElement | null>;
}) {
  const controls = useRef<ComponentRef<typeof OrbitControls>>(null);
  const { camera, size } = useThree();
  const aspect = size.width / Math.max(size.height, 1);
  const tween = useMemo(() => new StageCameraTween(), []);
  const baseDistance = useRef(1);
  // Portrait bounds are measured once per instance, right after loading (before any
  // motion), relative to the head bone at height 1. Re-entering the face view after
  // running or swinging reuses them, so a mid-motion pose never crops the crown.
  const portrait = useMemo(() => {
    const head = instance.root.getObjectByName('J_Bip_C_Head') ?? instance.root.getObjectByName('Head') ?? null;
    const body = new Box3(), box = new Box3();
    measureOriginalCameraBounds(instance.root, head, body, box);
    const anchor = head ? head.getWorldPosition(new Vector3()) : box.getCenter(new Vector3());
    const scale = Math.max(instance.root.scale.y, 1e-3);
    box.min.sub(anchor).divideScalar(scale); box.max.sub(anchor).divideScalar(scale);
    return { head, relative: box };
  }, [instance]);
  const followed = useRef(new Vector3()), headNow = useRef(new Vector3()), shift = useRef(new Vector3());
  const headAnchor = (out: Vector3) => {
    if (portrait.head) return portrait.head.getWorldPosition(out);
    return out.set(0, instance.restBounds.top * instance.root.scale.y * .86, 0);
  };
  // Face view is a portrait: registered weapons are hidden on screen only.
  useLayoutEffect(() => {
    setRegisteredWeaponsVisible(instance.root, framing !== 'face');
    return () => setRegisteredWeaponsVisible(instance.root, true);
  }, [instance, framing]);
  // Stance and swing share one armed composition: only arming / disarming re-fits.
  const stance = armed ? 'armed' : '';
  const previous = useRef<{ instance: MakerInstance; framing: Props['framing']; view: Props['view']; revision: number; height: number; stance: string; aspect: number } | null>(null);
  useLayoutEffect(() => {
    if (!(camera instanceof PerspectiveCamera)) return;
    const last = previous.current;
    const reset = !last || last.instance !== instance || last.framing !== framing || last.view !== view || last.revision !== revision || last.stance !== stance || last.aspect !== aspect;
    previous.current = { instance, framing, view, revision, height, stance, aspect };
    const face = framing === 'face';
    // Height edits preserve the user's orbit and zoom: portraits follow the head
    // bone every frame, full views keep their ground reference fixed.
    if (!reset) return;
    const animate = !!last && last.instance === instance && last.aspect === aspect && (last.framing !== framing || last.view !== view || last.revision !== revision || last.stance !== stance) && !prefersInstantCamera();
    const top = instance.restBounds.top, restHeight = instance.restBounds.height, feet = top - restHeight;
    const direction = stageDirection(STUDIO_AZIMUTH[view]);
    let target: Vector3, distance: number;
    if (face) {
      // Same portrait rule as the originals: head bounds measured at load (weapon
      // excluded), placed on the head bone's current position, padded 1.12, head
      // centre 45% from the top.
      const anchor = headAnchor(new Vector3());
      const headBox = portrait.relative.clone();
      headBox.min.multiplyScalar(height).add(anchor); headBox.max.multiplyScalar(height).add(anchor);
      followed.current.copy(anchor);
      const frame = stageFaceWindow(headBox, headBox.getCenter(new Vector3()), direction, aspect, 1.06);
      distance = Math.max(.2, frame.visible) / (2 * STUDIO_HALF_TAN);
      target = frame.target;
    } else {
      const feetLine = armed ? STUDIO_FEET.armed : STUDIO_FEET.full;
      // Armed: the sampled clip envelope at the tallest height (sizes compare in one frame).
      const envelope = armed ? presetArmedBounds(instance, height) : null;
      let centreR = 0, right: Vector3 | null = null;
      if (envelope) {
        envelope.min.multiplyScalar(HEIGHT_RANGE.max); envelope.max.multiplyScalar(HEIGHT_RANGE.max);
        const projected = projectBox(envelope, direction);
        const spanU = Math.max(projected.u1 - projected.u0, .01), spanR = Math.max(projected.r1 - projected.r0, .01);
        const depth = Math.abs(envelope.getSize(new Vector3()).dot(new Vector3(Math.abs(direction.x), Math.abs(direction.y), Math.abs(direction.z))));
        distance = STUDIO_ARMED_MATCH * Math.max(spanU / (1 - feetLine - STUDIO_ARMED_TOP), spanR * 1.04 / Math.max(aspect, .1)) / (2 * STUDIO_HALF_TAN) + depth / 2;
        // Optical centre (as the originals): halfway between the body (root at the
        // origin) and the whole silhouette, clamped so the full swing stays in view.
        const visW = 2 * STUDIO_HALF_TAN * (distance - depth / 2) * aspect, m = visW * .01;
        const lo = projected.r1 - visW / 2 + m, hi = projected.r0 + visW / 2 - m;
        centreR = lo <= hi ? Math.min(Math.max((projected.r0 + projected.r1) / 4, lo), hi) : (projected.r0 + projected.r1) / 2;
        right = projected.right;
      } else {
        distance = armed
          ? makerArmedCameraDistance(Math.max(1.3, restHeight * HEIGHT_RANGE.max), aspect, 1)
          : restHeight / (STUDIO_FULL_OCCUPANCY * 2 * STUDIO_HALF_TAN);
      }
      // Solve the target height that puts the feet on the stage floor line.
      const visible = 2 * STUDIO_HALF_TAN * distance;
      const goalNdc = -1 + 2 * feetLine;
      target = new Vector3(0, feet + visible * (.5 - feetLine), 0);
      if (right) target.addScaledVector(right, centreR);
      const probe = new PerspectiveCamera(STUDIO_FOV, aspect, .01, 100), point = new Vector3();
      for (let i = 0; i < 4; i++) {
        probe.position.copy(target).addScaledVector(direction, distance); probe.lookAt(target); probe.updateMatrixWorld(); probe.updateProjectionMatrix();
        // Toes reach toward the lens; a tilted camera draws them lower than the heel line.
        point.set(direction.x * restHeight * .08, feet, direction.z * restHeight * .08).project(probe);
        target.y += (point.y - goalNdc) * visible / 2;
      }
    }
    const goal = { target, position: target.clone().addScaledVector(direction, distance), zoom: 1 };
    const orbit = controls.current;
    baseDistance.current = distance;
    if (orbit) { orbit.minDistance = distance * .25; orbit.maxDistance = distance / .6; }
    camera.near = .01; camera.far = Math.max(50, distance * 4);
    if (animate && orbit) {
      // Controls clamp the radius on every update; let the arc pass between both framings.
      const current = camera.position.distanceTo(orbit.target);
      orbit.minDistance = Math.min(current, distance) * .25; orbit.maxDistance = Math.max(current, distance) / .6;
      tween.start(goal);
      markCameraSettled(root.current, false);
      return;
    }
    tween.stop();
    camera.position.copy(goal.position);
    camera.zoom = 1;
    camera.lookAt(target);
    camera.updateProjectionMatrix();
    orbit?.target.copy(target);
    orbit?.update();
    markCameraSettled(root.current, true);
  // headAnchor reads the memoised portrait and refs only.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instance, framing, view, revision, height, armed, stance, camera, portrait, aspect, tween, root]);
  useFrame((_, delta) => {
    const orbit = controls.current;
    if (!orbit) return;
    if (framing === 'face') {
      // Follow the head bone by translation (as the originals do), including
      // height edits, idle sway and an arc that is still running.
      headAnchor(headNow.current);
      shift.current.copy(headNow.current).sub(followed.current);
      if (shift.current.lengthSq() > 0) {
        camera.position.add(shift.current); orbit.target.add(shift.current);
        tween.translate(shift.current);
        followed.current.copy(headNow.current);
        if (!tween.active) orbit.update();
      }
    }
    if (!tween.active) return;
    const settled = tween.step(camera, orbit.target, delta);
    camera.updateProjectionMatrix(); orbit.update();
    if (settled) { orbit.minDistance = baseDistance.current * .25; orbit.maxDistance = baseDistance.current / .6; markCameraSettled(root.current, true); }
  });
  return <OrbitControls ref={controls} makeDefault enableDamping dampingFactor={.08} enablePan={false} zoomToCursor minPolarAngle={.2} maxPolarAngle={Math.PI * .85}
    onStart={() => { if (tween.active) { tween.stop(); markCameraSettled(root.current, true); } }} />;
}

export default function CharacterMakerScene(props: Props) {
  const { config, modelRevision = 0, framing, view, motion, motionRevision = 0, onMotionComplete, cameraRevision = 0 } = props;
  const latest = useRef(props);
  latest.current = props;
  const [instance, setInstance] = useState<MakerInstance | null>(null);
  const [error, setError] = useState('');
  const [attempt, setAttempt] = useState(0);
  const studio = props.studio ?? false;
  const root = useRef<HTMLDivElement>(null);
  const doubleTap = useStageDoubleTap(() => latest.current.onFramingToggle?.());
  // Mature is a different topology-preserving head geometry, not a head-size filter.
  const matureHead = config.style === 'fantasy' && config.face === 'mature';
  // Studio retries a failed model load once on its own (a dropped fetch is common on phones).
  const retriedRef = useRef(false), retryTimer = useRef(0);
  useEffect(() => { retriedRef.current = false; }, [config.style, config.base, config.hair, config.outfitPalette, config.weaponId, matureHead, modelRevision]);
  useEffect(() => () => window.clearTimeout(retryTimer.current), []);

  useEffect(() => {
    let cancelled = false;
    let owned: MakerInstance | null = null;
    setInstance(null);
    setError('');
    latest.current.onReady?.(false);
    assembleCharacter(latest.current.config).then(result => {
      if (cancelled) { result.dispose(); return; }
      owned = result;
      result.apply(latest.current.config);
      setInstance(result);
      latest.current.onReady?.(true);
    }).catch(reason => {
      if (cancelled) return;
      const message = reason instanceof Error ? reason.message : '3Dモデルを読み込めませんでした。';
      if (studio && !retriedRef.current) {
        retriedRef.current = true;
        console.warn('[studio] model load failed, retrying once:', message);
        retryMakerWeapon(selectedMakerWeaponId(latest.current.config));
        retryTimer.current = window.setTimeout(() => setAttempt(value => value + 1), 500);
        return;
      }
      if (studio) console.warn('[studio] model load failed:', message);
      setError(message);
      latest.current.onError?.(message);
    });
    return () => { cancelled = true; owned?.dispose(); };
  // Rebuild the owned model on deliberate reloads while retaining the Canvas.
  // The cleanup flag rejects late results from an earlier model request.
  }, [config.style, config.base, config.hair, config.outfitPalette, config.weaponId, matureHead, modelRevision, attempt]);

  if (studio) return <div ref={root} data-stage-canvas="" data-camera-settled="false" style={{ position: 'absolute', inset: 0, minHeight: 0 }} {...doubleTap}>
    <Canvas camera={{ position: [0, 1, 4], fov: STUDIO_FOV, near: 0.01, far: 50 }} dpr={[1, 2]} gl={{ antialias: true, alpha: true, preserveDrawingBuffer: true }}>
      {instance && <>
        <Model playing={props.playing ?? true} speed={props.speed ?? 1} key={`model-${instance.root.uuid}`} instance={instance} config={config} motion={motion} motionRevision={motionRevision} onMotionComplete={onMotionComplete} hitRevision={props.hitRevision ?? 0} onHitComplete={props.onHitComplete} hurtFace={props.hurtFace ?? false} />
        <StudioCamera key={`camera-${instance.root.uuid}`} root={root} instance={instance} framing={framing} view={view} revision={cameraRevision} height={config.height} armed={motion.startsWith('sword_')} />
      </>}
      <StageFloor subject={instance?.root ?? null} scale={config.height} restHeight={instance?.restBounds.height ?? 0} accent={props.accent} pulseKey={`${view}:${framing}`} visible={!!instance} />
    </Canvas>
    {!instance && (error
      ? <StageError onRetry={() => { retryMakerWeapon(selectedMakerWeaponId(config)); setAttempt(value => value + 1); }} />
      : <div role="status" style={SR_ONLY}>キャラクターを準備しています…</div>)}
  </div>;

  return <div style={{ position: 'relative', width: '100%', height: '100%', minHeight: 0 }} aria-label="キャラクターの3Dプレビュー">
    <Canvas camera={{ position: [0, 1, 4], fov: 32, near: 0.01, far: 50 }} dpr={[1, 2]} gl={{ antialias: true, alpha: true, preserveDrawingBuffer: true }}>
      {instance && <>
        <Model playing={props.playing ?? true} speed={props.speed ?? 1} key={`model-${instance.root.uuid}`} instance={instance} config={config} motion={motion} motionRevision={motionRevision} onMotionComplete={onMotionComplete} hitRevision={props.hitRevision ?? 0} onHitComplete={props.onHitComplete} hurtFace={props.hurtFace ?? false} />
        <Camera key={`camera-${instance.root.uuid}`} instance={instance} framing={framing} view={view} revision={cameraRevision} height={config.height} armed={motion.startsWith('sword_')} />
      </>}
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.035, 0]}>
        <circleGeometry args={[0.72, 64]} />
        <meshBasicMaterial color="#b8cfcd" transparent opacity={0.24} toneMapped={false} />
      </mesh>
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, -0.034, 0]}>
        <ringGeometry args={[0.77, 0.775, 64]} />
        <meshBasicMaterial color="#a5b4a9" transparent opacity={0.4} toneMapped={false} />
      </mesh>
    </Canvas>
    {!instance && <div role={error ? 'alert' : 'status'} style={{ position: 'absolute', inset: 0, display: 'grid', alignContent: 'center', justifyItems: 'center', gap: 12, padding: 28, textAlign: 'center', background: '#eef3eee8', color: '#31483e' }}>
      <span>{error || 'キャラクターを準備しています…'}</span>
      {error && <button type="button" onClick={() => { retryMakerWeapon(selectedMakerWeaponId(config)); setAttempt(value => value + 1); }}>もう一度読み込む</button>}
    </div>}
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
