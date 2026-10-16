"use client";
// The generic-character copy of the 4255 viewer's RigActor and Camera (2026-10-01, Owner: "npcとメインキャラ、汎用キャラでは、
// 共通部品は作らないほうがいいよ"). components/samples/TripoRigStudy.tsx picks this copy by the kind in
// lib/character-kind.ts. Every file in this folder is this kind's own copy (springs, collar, tail, mouth, blink) and
// never imports another kind's folder or the shared originals; the material rule is lib/viewer-material/generic.mjs.
// FROZEN: this folder draws the generic characters exactly as the viewer did on 2026-10-01 (after the
// material fix 54d17a7a..8a03de87). components/rig-actor/kind-copies.test.mjs pins every file here by SHA-256.
// Hero work changes components/rig-actor/hero/ only.
import { useEffect, useMemo, useRef, type MutableRefObject } from "react";
import { useFrame, useThree } from "@react-three/fiber";
import { useGLTF } from "@react-three/drei";
import { clone } from "three/examples/jsm/utils/SkeletonUtils.js";
import * as THREE from "three";
import { CharacterHairSprings, HAIR_STEP } from "./hair-springs";
import { CharacterCollarAttachment } from "./collar-attachment";
import { CharacterTailMotion } from "./tail-motion";
import { speakingMouth } from "./mouth";
import { automaticBlink, blinkCorrectiveWeight } from "./blink";
import genericViewerMaterial from "../../../lib/viewer-material/generic.mjs";

export type Motion = "idle" | "walk" | "run";
export type Face = { blinkLeft: number; blinkRight: number; mouthClose: number; aa: number; ee: number; ou: number; happy: number };
const NEUTRAL: Face = { blinkLeft: 0, blinkRight: 0, mouthClose: 1, aa: 0, ee: 0, ou: 0, happy: 0 };
export type View = "front" | "frontRight30" | "frontRight60" | "right" | "backRight60" | "backRight30" | "back" | "backLeft30" | "backLeft60" | "left" | "frontLeft60" | "frontLeft30";
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
type RigActorProps = {
  character: Character; profile: RigProfile; motion: Motion; playing: boolean; speed: number; face: Face; autoBlink: boolean; speaking: boolean;
  hairMotion: boolean; headTurn: number;
  seek: { value: number; revision: number }; report: (time: number, duration: number) => void;
  clock?: MutableRefObject<number>; onReady?: () => void;
};

export function RigActor({ character, profile, motion, playing, speed, face, autoBlink, speaking, hairMotion, headTurn, seek, report, clock, onReady }: RigActorProps) {
  const { scene, animations } = useGLTF(modelUrl(profile), true, true, loader => {
    loader.register(parser => ({ name: "RIG_HTML_TEXTURES", beforeRoot: () => {
      parser.textureLoader = new THREE.TextureLoader(parser.options.manager).setCrossOrigin(parser.options.crossOrigin).setRequestHeader(parser.options.requestHeader);
      return null;
    } }));
  });
  const { model, mixer, materials, faces, springs, collar, tail, head, headPose, headAxis, headRotation, parentRotation } = useMemo(() => {
    const model = clone(scene);
    const materials: THREE.Material[] = [], faces: THREE.Mesh[] = [];
    model.traverse(object => {
      if (!(object instanceof THREE.Mesh)) return;
      // Animated bounds change with poses. This viewer keeps the full character visible.
      object.frustumCulled = false;
      if (object.morphTargetDictionary) faces.push(object);
      const originals = Array.isArray(object.material) ? object.material : [object.material];
      const mapped = originals.map((m: THREE.MeshStandardMaterial) => {
        const result = genericViewerMaterial(m);
        materials.push(result); return result;
      });
      object.material = Array.isArray(object.material) ? mapped : mapped[0];
    });
    // Tripo's first rigs name the head bone "Head"; the VRM-style rigs (NPCs, heroes) name it J_Bip_C_Head.
    const head = model.getObjectByName("Head") ?? model.getObjectByName("J_Bip_C_Head");
    return { model, materials, faces, mixer: new THREE.AnimationMixer(model), springs: new CharacterHairSprings(model), collar: new CharacterCollarAttachment(model), tail: new CharacterTailMotion(model),
      head, headPose: head?.quaternion.clone() ?? new THREE.Quaternion(), headAxis: new THREE.Vector3(),
      headRotation: new THREE.Quaternion(), parentRotation: new THREE.Quaternion() };
  }, [scene, character]);
  // The downloadable clips contain baked hair. In this viewer the live solver
  // owns these joints, so additional head turns do not get a second animation.
  const bodyClips = useMemo(() => animations.map(clip => new THREE.AnimationClip(clip.name, clip.duration,
    clip.tracks.filter(track => !track.name.startsWith("Hair_") && !track.name.startsWith("Collar_Mount.") && !track.name.startsWith("TailCurl_") && !track.name.startsWith("KiyogonTail_")))), [animations]);
  const active = useRef<THREE.AnimationAction | null>(null);
  const elapsed = useRef(0), lastReport = useRef(0), accumulator = useRef(0), headYaw = useRef(0), headSettling = useRef(0);
  const lastClock = useRef(0), bodyAccumulator = useRef(0), faceTick = useRef(-1);
  const faceInput = useRef<{ face: Face; autoBlink: boolean; speaking: boolean } | null>(null);
  const bodyStep = 1 / (profile.viewer?.bodyHz ?? 120);
  useEffect(() => { onReady?.(); }, [onReady]);
  useEffect(() => {
    // Tripo presets end in ":idle"; models rigged later use the bare name "idle".
    const clip = bodyClips.find(a => a.name.endsWith(`:${motion}`)) ?? bodyClips.find(a => a.name === motion);
    if (!clip) return;
    const previous = active.current;
    const action = mixer.clipAction(clip).reset().setLoop(THREE.LoopRepeat, Infinity).play();
    if (previous && previous !== action) { previous.fadeOut(.18); action.fadeIn(.18); }
    active.current = action;
    if (head) head.quaternion.copy(headPose);
    mixer.update(0); if (head) headPose.copy(head.quaternion);
    collar.update(); springs.reset(); accumulator.current = 0; bodyAccumulator.current = 0; report(0, clip.duration);
  }, [bodyClips, mixer, motion, report, springs, collar, head, headPose]);
  useEffect(() => { if (active.current) {
    if (head) head.quaternion.copy(headPose);
    // A paused scrub must show this clip, even if its crossfade has just begun.
    mixer.stopAllAction();
    active.current.stopFading().setEffectiveWeight(1).play();
    active.current.time = seek.value * active.current.getClip().duration; mixer.update(0);
    if (head) headPose.copy(head.quaternion);
    headYaw.current = 0; collar.update(); springs.reset(); tail.seek(active.current.time, active.current.getClip().name.split(":").at(-1) as Motion); accumulator.current = 0; bodyAccumulator.current = 0; faceTick.current = -1;
  } }, [seek, mixer, springs, collar, tail, head, headPose]);
  useEffect(() => { headSettling.current = .8; }, [headTurn, hairMotion]);
  useEffect(() => () => { mixer.stopAllAction(); mixer.uncacheRoot(model); materials.forEach(m => m.dispose()); }, [mixer, model, materials]);
  useFrame((_, delta) => {
    if (clock) { delta = Math.max(0, clock.current - lastClock.current); lastClock.current = clock.current; }
    if (playing || headSettling.current > 0 || Math.abs(headYaw.current - headTurn) > .001) {
      accumulator.current += clock ? delta : Math.min(delta, .08);
      while (accumulator.current >= HAIR_STEP) {
        if (head) head.quaternion.copy(headPose);
        if (playing) {
          bodyAccumulator.current += HAIR_STEP;
          if (bodyAccumulator.current + 1e-8 >= bodyStep) {
            mixer.update(bodyAccumulator.current * speed);
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
        if (playing) tail.update(HAIR_STEP, motion, speed);
        headSettling.current = Math.max(0, headSettling.current - HAIR_STEP);
        accumulator.current -= HAIR_STEP;
      }
    }
    const nextFaceTick = Math.floor(elapsed.current * 60 + 1e-8);
    const changedFace = faceInput.current?.face !== face || faceInput.current.autoBlink !== autoBlink || faceInput.current.speaking !== speaking;
    if (!profile.viewer?.bodyHz || nextFaceTick !== faceTick.current || changedFace) {
    faceTick.current = nextFaceTick; faceInput.current = { face, autoBlink, speaking };
    const phase = elapsed.current % 4.1;
    const blink = autoBlink ? character === "aichan" ? automaticBlink(elapsed.current) : phase > 3.72 ? Math.sin((phase - 3.72) / .38 * Math.PI) : 0 : 0;
    const talk = speaking ? speakingMouth(elapsed.current) : null;
    for (const object of faces) {
      const dictionary = object.morphTargetDictionary!, weights = object.morphTargetInfluences!;
      for (const [name, index] of Object.entries(dictionary)) {
        const mouthCorrective = /^mouthCloseInbetween(25|50|75)$/.exec(name);
        if (mouthCorrective) {
          const closure = talk?.mouthClose ?? face.mouthClose;
          weights[index] = Math.max(0, 1 - Math.abs(closure - Number(mouthCorrective[1]) / 100) / .25);
          continue;
        }
        const blinkKey = name.startsWith("blinkLeft") ? "blinkLeft" : "blinkRight";
        const corrective = blinkCorrectiveWeight(name, Math.max(face[blinkKey], blink));
        if (corrective !== null) { weights[index] = corrective; continue; }
        if (name.endsWith("Arc")) {
          const key = name.replace(/Arc$/, "") as keyof Face;
          const base = talk && key in talk ? talk[key as keyof typeof talk] : face[key] ?? NEUTRAL[key];
          const closure = key.startsWith("blink") ? Math.max(base, blink) : base;
          weights[index] = Math.sin(Math.PI * closure);
          continue;
        }
        // Pyonmi's happy key is a complete mouth pose, so speech replaces it.
        const value = character === "pyonmi" && speaking && name === "happy" ? 0 : face[name as keyof Face] || 0;
        weights[index] = name.startsWith("blink") ? Math.max(value, blink) : talk && name in talk ? talk[name as keyof typeof talk] : value;
      }
    }
    }
    if (active.current && elapsed.current - lastReport.current > .1) { lastReport.current = elapsed.current; report(active.current.time, active.current.getClip().duration); }
  });
  return <group position={[0, profile.viewer?.modelOffsetY ?? 0, 0]} rotation={[0, -Math.PI / 2, 0]} scale={profile.viewer?.modelScale ?? 2}><primitive object={model} /></group>;
}
export function Camera({ view, close, character, profile }: { view: View; close: boolean; character: Character; profile: RigProfile }) {
  const { camera, controls, size } = useThree();
  useEffect(() => {
    const center = new THREE.Vector3(0, close ? (profile.viewer?.faceHeight ?? (character === "aichan" ? 1.76 : 1.48)) : 1.04, 0);
    const directions: Record<View, [number, number, number]> = {
      front: [0, 0, 5], frontRight30: [-2.5, 0, 4.33], frontRight60: [-4.33, 0, 2.5], right: [-5, 0, 0],
      backRight60: [-4.33, 0, -2.5], backRight30: [-2.5, 0, -4.33], back: [0, 0, -5], backLeft30: [2.5, 0, -4.33],
      backLeft60: [4.33, 0, -2.5], left: [5, 0, 0], frontLeft60: [4.33, 0, 2.5], frontLeft30: [2.5, 0, 4.33],
    };
    camera.position.copy(center).add(new THREE.Vector3(...directions[view])); camera.lookAt(center);
    if (camera instanceof THREE.OrthographicCamera) camera.zoom = Math.min(size.width, size.height) / (close ? (profile.viewer?.faceSpan ?? (character === "aichan" ? .72 : 1.35)) : (profile.viewer?.fullSpan ?? 2.35));
    camera.updateProjectionMatrix();
    const orbit = controls as unknown as { target: THREE.Vector3; update: () => void } | null;
    orbit?.target.copy(center); orbit?.update();
  }, [camera, controls, size, view, close, character, profile]);
  return null;
}
