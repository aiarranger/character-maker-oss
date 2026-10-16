"use client";

// Maker-owned actor: keep the frozen generic viewer intact while giving each
// editable instance its own geometry and rest-space appearance lifecycle.
import { useEffect, useLayoutEffect, useMemo, useRef, type MutableRefObject } from "react";
import { useFrame } from "@react-three/fiber";
import { useGLTF } from "@react-three/drei";
import { clone } from "three/examples/jsm/utils/SkeletonUtils.js";
import * as THREE from "three";
import { CharacterHairSprings, HAIR_STEP } from "../rig-actor/generic/hair-springs";
import { CharacterCollarAttachment } from "../rig-actor/generic/collar-attachment";
import { CharacterTailMotion } from "../rig-actor/generic/tail-motion";
import { speakingMouth } from "../rig-actor/generic/mouth";
import { automaticBlink, blinkCorrectiveWeight } from "../rig-actor/generic/blink";
import genericViewerMaterial from "../../lib/viewer-material/generic.mjs";

import { createOriginalBody } from "../../lib/character-maker/original-body";
import { configureOriginalSpecies, defaultOriginalSpeciesParts } from "../../lib/character-maker/original-species";
import { configureOriginalHumanParts } from "../../lib/character-maker/original-human-parts";
import { configureOriginalHair } from "../../lib/character-maker/original-hair";
import { attachOriginalSword, originalClipsWithSword, ORIGINAL_VISIBILITY_TRACKS } from "../../lib/character-maker/original-weapon";
import { createHurtFace, worldForward } from "../../lib/character-maker/hurt-face";
import { HIT_CLIP } from "../../lib/character-maker/hurt";
import { originalHurtFaceStyle } from "../../lib/character-maker/original-config";
import { readMakerWeapon } from '../../lib/character-maker/registered-weapon';
import type { MakerWeaponId } from '../../lib/character-maker/weapon-catalog';
import type { OriginalAppearance } from "../../lib/character-maker/original-config";

export type Motion = "idle" | "walk" | "run" | "sword_idle" | "sword_attack";
export type Face = { blinkLeft: number; blinkRight: number; mouthClose: number; aa: number; ee: number; ou: number; happy: number };
const NEUTRAL: Face = { blinkLeft: 0, blinkRight: 0, mouthClose: 1, aa: 0, ee: 0, ou: 0, happy: 0 };
/** のけぞりの重みの立ち上がり・戻り（秒） */
const HIT_IN = .06, HIT_OUT = .15;
type Character = string;
// The fields of the character ledger this viewer reads (lib/character-profile.ts CharacterProfile has them).
type RigProfile = {
  id: string;
  latest: null | { publicGlb: string };
  viewerModel?: null | { publicGlb: string };
  viewer?: { bodyHz?: number; modelOffsetY?: number; modelScale?: number; fullSpan?: number; faceHeight?: number; faceSpan?: number };
};
// As getViewerModel in lib/character-profile.ts: the loader's validated model when available, else latest.
const modelUrl = (profile: RigProfile) => {
  const model = profile.viewerModel !== undefined ? profile.viewerModel : profile.latest;
  if (!model) throw Error(`No validated viewer model for ${profile.id}`);
  return "/" + model.publicGlb.replace(/^\/+/, "").replace(/^public\//, "");
};
type OriginalEditableActorProps = {
  character: Character; profile: RigProfile; motion: Motion; playing: boolean; speed: number; face: Face; autoBlink: boolean; speaking: boolean;
  hairMotion: boolean; headTurn: number;
  seek: { value: number; revision: number }; report: (time: number, duration: number) => void;
  clock?: MutableRefObject<number>; onReady?: () => void;
  weaponId: MakerWeaponId;
  bodyShape: number; appearance: OriginalAppearance; onModel?: (model: THREE.Object3D, clips: readonly THREE.AnimationClip[]) => void | (() => void);
  attackRevision?: number; onAttackComplete?: () => void;
  /** のけぞりを1回再生する（値が変わるたび。docs/main-character-spec.md）。終わると onHitComplete */
  hitRevision?: number; onHitComplete?: () => void;
  /** ＞＜ の顔を出し続ける（のけぞりの間は自動で出る） */
  hurtFace?: boolean;
  /** Studio stage: step the body at 120 Hz even on comic rigs authored for a stepped bodyHz (the face keeps its tick). */
  smoothBody?: boolean;
};

export function OriginalEditableActor({ character, profile, motion, playing, speed, face, autoBlink, speaking, hairMotion, headTurn, seek, report, clock, onReady, bodyShape, appearance, weaponId, onModel, attackRevision = 0, onAttackComplete, hitRevision = 0, onHitComplete, hurtFace = false, smoothBody = false }: OriginalEditableActorProps) {
  const { scene, animations } = useGLTF(modelUrl(profile), true, true, loader => {
    loader.register(parser => ({ name: "RIG_HTML_TEXTURES", beforeRoot: () => {
      parser.textureLoader = new THREE.TextureLoader(parser.options.manager).setCrossOrigin(parser.options.crossOrigin).setRequestHeader(parser.options.requestHeader);
      return null;
    } }));
  });
  const weaponSource = readMakerWeapon(weaponId);
  const { model, body, hair, species, humanParts, weapon, hurt, mixer, materials, faces, springs, collar, tail, head, headPose, headAxis, headRotation, parentRotation } = useMemo(() => {
    const model = clone(scene);
    // Initialize before animation or caller appearance hooks alter the model.
    const materials: THREE.Material[] = [], faces: THREE.Mesh[] = [];
    model.traverse(object => {
      if (!(object instanceof THREE.Mesh)) return;
      // Animated bounds change with poses. This viewer keeps the full character visible.
      object.frustumCulled = false;
      const originals = Array.isArray(object.material) ? object.material : [object.material];
      const mapped = originals.map((m: THREE.MeshStandardMaterial) => {
        const result = genericViewerMaterial(m);
        result.name = m.name;
        result.userData = structuredClone(m.userData);
        materials.push(result); return result;
      });
      object.material = Array.isArray(object.material) ? mapped : mapped[0];
    });
    const species = configureOriginalSpecies(model, character, appearance.parts);
    const humanParts = configureOriginalHumanParts(model, character, appearance.parts ?? defaultOriginalSpeciesParts());
    const body = createOriginalBody(model, character);
    const hair = configureOriginalHair(model, character, appearance);
    const weapon = attachOriginalSword(model, character, weaponId, weaponSource, profile.viewer?.modelScale ?? 2);
    // ＞＜ の板は bind の姿のまま作る（顔の正面は模型の +X。表示ではこれを -90° 回して正面へ向ける）
    const hurtStyle = originalHurtFaceStyle(character, appearance.parts);
    const hurt = hurtStyle ? createHurtFace(model, hurtStyle, worldForward(model, new THREE.Vector3(1, 0, 0))) : null;
    model.traverse(object => { if (object instanceof THREE.Mesh && object.morphTargetDictionary) faces.push(object); });
    // Tripo's first rigs name the head bone "Head"; the VRM-style rigs (NPCs, heroes) name it J_Bip_C_Head.
    const head = model.getObjectByName("Head") ?? model.getObjectByName("J_Bip_C_Head");
    return { model, body, hair, species, humanParts, weapon, hurt, materials, faces, mixer: new THREE.AnimationMixer(model), springs: new CharacterHairSprings(model), collar: new CharacterCollarAttachment(model), tail: new CharacterTailMotion(model),
      head, headPose: head?.quaternion.clone() ?? new THREE.Quaternion(), headAxis: new THREE.Vector3(),
      headRotation: new THREE.Quaternion(), parentRotation: new THREE.Quaternion() };
  }, [scene, character, appearance.hairstyle, appearance.hairColor, appearance.parts, weaponId, weaponSource, profile.viewer?.modelScale]);
  useLayoutEffect(() => { body.apply(bodyShape); }, [body, bodyShape]);
  // The downloadable clips contain baked hair. In this viewer the live solver
  // owns these joints, so additional head turns do not get a second animation.
  const bodyClips = useMemo(() => originalClipsWithSword(character, animations, weaponId !== 'none').map(clip => new THREE.AnimationClip(clip.name, clip.duration,
    clip.tracks.filter(track => !track.name.startsWith("Hair_") && !track.name.startsWith("Collar_Mount.") && !track.name.startsWith("TailCurl_") && !track.name.startsWith("KiyogonTail_")))), [animations, character, weaponId]);
  // Supply the same preview clips for a stable, weapon-aware motion envelope.
  useLayoutEffect(() => onModel?.(model, bodyClips), [model, bodyClips, onModel]);
  // のけぞりは今の動作の上に重ねる一回再生。武器と手の表示のトラックは今の動作と同じ値にする（重みで混ざっても変わらない）
  const hitClip = useMemo(() => {
    const source = bodyClips.find(clip => clip.name === HIT_CLIP);
    if (!source) return null;
    const base = bodyClips.find(a => a.name.endsWith(`:${motion}`)) ?? bodyClips.find(a => a.name === motion);
    const tracks = source.tracks.filter(track => !ORIGINAL_VISIBILITY_TRACKS.has(track.name));
    for (const track of base?.tracks ?? []) if (ORIGINAL_VISIBILITY_TRACKS.has(track.name)) {
      const value = Array.from(track.values.slice(0, 3));
      tracks.push(new THREE.VectorKeyframeTrack(track.name, [0, source.duration], [...value, ...value]));
    }
    return new THREE.AnimationClip(`${HIT_CLIP}:${motion}`, source.duration, tracks);
  }, [bodyClips, motion]);
  const hit = useRef<THREE.AnimationAction | null>(null);
  const appliedHit = useRef(hitRevision);
  const hitCompletion = useRef(onHitComplete);
  hitCompletion.current = onHitComplete;
  const active = useRef<THREE.AnimationAction | null>(null);
  const playback = useRef<{ character: string; motion: Motion; attackRevision: number; time: number } | null>(null);
  const completion = useRef(onAttackComplete);
  completion.current = onAttackComplete;
  const scrubbing = useRef(false);
  const appliedSeek = useRef<number | null>(null);
  const elapsed = useRef(0), lastReport = useRef(0), accumulator = useRef(0), headYaw = useRef(0), headSettling = useRef(0);
  const lastClock = useRef(0), bodyAccumulator = useRef(0), faceTick = useRef(-1);
  const faceInput = useRef<{ face: Face; autoBlink: boolean; speaking: boolean; hurt: number } | null>(null);
  const bodyStep = 1 / (smoothBody ? 120 : profile.viewer?.bodyHz ?? 120);
  useLayoutEffect(() => {
    headYaw.current = 0; headSettling.current = .8; accumulator.current = 0; bodyAccumulator.current = 0;
    faceTick.current = -1; faceInput.current = null;
    // A hit on a replaced model's mixer can never finish; end it so the caller is not left waiting.
    if (hit.current) { hit.current = null; hitCompletion.current?.(); }
  }, [model]);
  // Weights for the next mixer step: the hit fades in over HIT_IN and out over HIT_OUT; the motion underneath keeps running with the rest.
  // Returns whether a hit is still playing.
  const stepHit = () => {
    const action = hit.current;
    if (!action) return false;
    const duration = action.getClip().duration, time = action.time;
    const finished = !action.isScheduled() || action.paused || time >= duration - 1e-6;
    const weight = finished ? 0 : Math.max(0, Math.min(1, time / HIT_IN, (duration - time) / HIT_OUT));
    action.weight = weight;
    for (const clip of bodyClips) { const other = mixer.existingAction(clip); if (other) other.weight = 1 - weight; }
    if (!finished) return true;
    action.stop(); hit.current = null;
    hitCompletion.current?.();
    return false;
  };
  useEffect(() => {
    if (hitRevision === appliedHit.current) return;
    appliedHit.current = hitRevision;
    if (!hitClip) { hitCompletion.current?.(); return; }
    // Pressing again restarts the hit from its first frame.
    if (hit.current && hit.current.getClip() !== hitClip) hit.current.stop();
    const action = mixer.clipAction(hitClip);
    action.stop();
    action.reset().setLoop(THREE.LoopOnce, 1);
    action.clampWhenFinished = true; action.weight = 0; action.play();
    hit.current = action;
    // While paused the motion underneath holds its pose; only the hit advances.
    if (!playing && active.current) active.current.paused = true;
    // playing is read only when a hit starts.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hitRevision, hitClip, mixer]);
  useEffect(() => { onReady?.(); }, [model, onReady]);
  useEffect(() => {
    // Tripo presets end in ":idle"; models rigged later use the bare name "idle".
    const clip = bodyClips.find(a => a.name.endsWith(`:${motion}`)) ?? bodyClips.find(a => a.name === motion);
    if (!clip) return;
    const previous = active.current;
    const sameMixer = previous?.getMixer() === mixer;
    const resumeTime = !sameMixer && playback.current?.character === character && playback.current.motion === motion
      && playback.current.attackRevision === attackRevision ? playback.current.time : 0;
    const action = mixer.clipAction(clip);
    // A paused replacement cannot advance a crossfade. Clear old actions and
    // show this clip immediately, including its sword/hand scale tracks.
    if (!playing) mixer.stopAllAction();
    // Pressing the attack again must restart this same action, even mid-swing.
    if (action === previous) action.stop();
    action.reset().setLoop(motion === 'sword_attack' ? THREE.LoopOnce : THREE.LoopRepeat, motion === 'sword_attack' ? 1 : Infinity).play();
    action.clampWhenFinished = motion === 'sword_attack';
    action.time = resumeTime;
    if (playing && sameMixer && previous && previous !== action) { previous.fadeOut(.18); action.fadeIn(.18); }
    else action.stopFading().setEffectiveWeight(1);
    active.current = action;
    if (head) head.quaternion.copy(headPose);
    mixer.update(0); if (head) headPose.copy(head.quaternion);
    collar.update(); springs.reset(); accumulator.current = 0; bodyAccumulator.current = 0; report(action.time, clip.duration);
    const finished = (event: { action: THREE.AnimationAction }) => {
      if (!scrubbing.current && motion === 'sword_attack' && event.action === action && active.current === action) completion.current?.();
    };
    mixer.addEventListener('finished', finished);
    return () => { mixer.removeEventListener('finished', finished); };
    // Sample playing on a motion/model change; play/pause itself must retain time.
  }, [bodyClips, mixer, motion, attackRevision, character, report, springs, collar, head, headPose]);
  useEffect(() => { if (active.current) {
    if (appliedSeek.current === seek.revision) return;
    appliedSeek.current = seek.revision;
    if (head) head.quaternion.copy(headPose);
    // A paused scrub must show this clip, even if its crossfade has just begun.
    mixer.stopAllAction();
    active.current.stopFading().setEffectiveWeight(1).play();
    active.current.time = seek.value * active.current.getClip().duration;
    scrubbing.current = true;
    try { mixer.update(0); } finally { scrubbing.current = false; }
    playback.current = { character, motion, attackRevision, time: active.current.time };
    report(active.current.time, active.current.getClip().duration);
    if (head) headPose.copy(head.quaternion);
    headYaw.current = 0; collar.update(); springs.reset(); tail.seek(active.current.time, motion === 'walk' || motion === 'run' ? motion : 'idle'); accumulator.current = 0; bodyAccumulator.current = 0; faceTick.current = -1;
  } }, [seek, mixer, springs, collar, tail, head, headPose, report, motion, character, attackRevision]);
  useEffect(() => {
    if (!playing && active.current) {
      // A fade cannot finish while the mixer is stopped. Keep the selected
      // phase and evaluate only that action, including sword visibility tracks.
      const action = active.current, time = action.time;
      if (head) head.quaternion.copy(headPose);
      mixer.stopAllAction();
      action.stopFading().setEffectiveWeight(1).play();
      action.time = time;
      scrubbing.current = true;
      try { mixer.update(0); } finally { scrubbing.current = false; }
      if (head) headPose.copy(head.quaternion);
      headSettling.current = .8;
      collar.update(); springs.reset();
      tail.seek(time, motion === 'walk' || motion === 'run' ? motion : 'idle');
      accumulator.current = 0; bodyAccumulator.current = 0; faceTick.current = -1;
      playback.current = { character, motion, attackRevision, time: active.current.time };
      report(active.current.time, active.current.getClip().duration);
    } else if (playing && active.current) {
      // A scrub to the final frame clamps a one-shot action. Explicit play
      // starts it again; pausing halfway keeps the selected time unchanged.
      if (motion === 'sword_attack' && active.current.time >= active.current.getClip().duration) active.current.time = 0;
      active.current.paused = false;
    }
  }, [playing, report, motion, character, attackRevision, mixer, head, headPose, collar, springs, tail]);
  useEffect(() => { headSettling.current = .8; }, [headTurn, hairMotion]);
  useEffect(() => () => {
    mixer.stopAllAction(); mixer.uncacheRoot(model); body.dispose(); hair.dispose(); species.dispose(); humanParts.dispose(); weapon.dispose(); hurt?.dispose(); materials.forEach(m => m.dispose());
    const skeletons = new Set<THREE.Skeleton>();
    model.traverse(object => { if (object instanceof THREE.SkinnedMesh) skeletons.add(object.skeleton); });
    skeletons.forEach(skeleton => skeleton.dispose());
  }, [mixer, model, body, hair, species, humanParts, weapon, hurt, materials]);
  useFrame((_, delta) => {
    if (clock) { delta = Math.max(0, clock.current - lastClock.current); lastClock.current = clock.current; }
    const advancing = playing || hit.current !== null;
    if (advancing || headSettling.current > 0 || Math.abs(headYaw.current - headTurn) > .001) {
      accumulator.current += clock ? delta : Math.min(delta, .08);
      while (accumulator.current >= HAIR_STEP) {
        if (head) head.quaternion.copy(headPose);
        if (advancing) {
          bodyAccumulator.current += HAIR_STEP;
          if (bodyAccumulator.current + 1e-8 >= bodyStep) {
            if (stepHit() || playing) mixer.update(bodyAccumulator.current * speed);
            bodyAccumulator.current = 0;
          }
          elapsed.current += HAIR_STEP;
        }
        if (head) {
          headPose.copy(head.quaternion);
          headYaw.current = THREE.MathUtils.damp(headYaw.current, headTurn, 9, HAIR_STEP);
          // Turn around the character's vertical axis in the head parent's space.
          model.getWorldQuaternion(headRotation); headAxis.set(0, 1, 0).applyQuaternion(headRotation);
          head.parent?.getWorldQuaternion(parentRotation);
          headAxis.applyQuaternion(parentRotation.invert());
          head.quaternion.premultiply(headRotation.setFromAxisAngle(headAxis, THREE.MathUtils.degToRad(headYaw.current)));
        }
        collar.update(); springs.update(HAIR_STEP, hairMotion ? 1 : 0);
        if (playing) tail.update(HAIR_STEP, motion === 'walk' || motion === 'run' ? motion : 'idle', speed);
        headSettling.current = Math.max(0, headSettling.current - HAIR_STEP);
        accumulator.current -= HAIR_STEP;
      }
    }
    // ＞＜: the toggle keeps it on; during a hit it shows at once and fades with the hit's last HIT_OUT seconds.
    const hitAction = hit.current;
    const hurtWeight = Math.max(hurtFace ? 1 : 0, hitAction ? Math.min(1, Math.max(0, (hitAction.getClip().duration - hitAction.time) / HIT_OUT)) : 0);
    hurt?.set(hurtWeight);
    const hurtStyle = hurtWeight > 0 ? originalHurtFaceStyle(character) : undefined;
    const shown: Face = hurtStyle ? { ...face } : face;
    if (hurtStyle) for (const [key, value] of Object.entries(hurtStyle.mouth)) if (key in shown) shown[key as keyof Face] += (value - shown[key as keyof Face]) * hurtWeight;
    const nextFaceTick = Math.floor(elapsed.current * 60 + 1e-8);
    const changedFace = faceInput.current?.face !== face || faceInput.current.autoBlink !== autoBlink || faceInput.current.speaking !== speaking || faceInput.current.hurt !== hurtWeight;
    if (!profile.viewer?.bodyHz || nextFaceTick !== faceTick.current || changedFace) {
    faceTick.current = nextFaceTick; faceInput.current = { face, autoBlink, speaking, hurt: hurtWeight };
    const phase = elapsed.current % 4.1;
    // The closed eyes under the ＞＜ plates use the blink channels.
    const blink = Math.max(hurtWeight, autoBlink ? character === "aichan" ? automaticBlink(elapsed.current) : phase > 3.72 ? Math.sin((phase - 3.72) / .38 * Math.PI) : 0 : 0);
    const talk = speaking && hurtWeight < .5 ? speakingMouth(elapsed.current) : null;
    for (const object of faces) {
      const dictionary = object.morphTargetDictionary!, weights = object.morphTargetInfluences!;
      for (const [name, index] of Object.entries(dictionary)) {
        if (name === 'makerGrip') continue;
        const mouthCorrective = /^mouthCloseInbetween(25|50|75)$/.exec(name);
        if (mouthCorrective) {
          const closure = talk?.mouthClose ?? shown.mouthClose;
          weights[index] = Math.max(0, 1 - Math.abs(closure - Number(mouthCorrective[1]) / 100) / .25);
          continue;
        }
        const blinkKey = name.startsWith("blinkLeft") ? "blinkLeft" : "blinkRight";
        const corrective = blinkCorrectiveWeight(name, Math.max(shown[blinkKey], blink));
        if (corrective !== null) { weights[index] = corrective; continue; }
        if (name.endsWith("Arc")) {
          const key = name.replace(/Arc$/, "") as keyof Face;
          const base = talk && key in talk ? talk[key as keyof typeof talk] : shown[key] ?? NEUTRAL[key];
          const closure = key.startsWith("blink") ? Math.max(base, blink) : base;
          weights[index] = Math.sin(Math.PI * closure);
          continue;
        }
        // Pyonmi's happy key is a complete mouth pose, so speech replaces it.
        const value = character === "pyonmi" && speaking && name === "happy" ? 0 : shown[name as keyof Face] || 0;
        weights[index] = name.startsWith("blink") ? Math.max(value, blink) : talk && name in talk ? talk[name as keyof typeof talk] : value;
      }
    }
    }
    // Face animation owns only face channels; the hand follows the selected
    // motion after the mixer update, including paused poses and scrubbing.
    weapon.applyGrip(motion.startsWith('sword_') ? 1 : 0);
    if (active.current) {
      playback.current = { character, motion, attackRevision, time: active.current.time };
      if (elapsed.current - lastReport.current > .1) { lastReport.current = elapsed.current; report(active.current.time, active.current.getClip().duration); }
    }
  }, -2);
  return <group position={[0, profile.viewer?.modelOffsetY ?? 0, 0]} rotation={[0, -Math.PI / 2, 0]} scale={profile.viewer?.modelScale ?? 2}><primitive object={model} dispose={null} /></group>;
}
