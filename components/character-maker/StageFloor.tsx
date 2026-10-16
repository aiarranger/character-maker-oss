'use client';

// Unlit studio floor (spec §5.2): a contact shadow and an accent ring.
// Everything here is MeshBasicMaterial with alpha < .6 so canvas pixel probes
// (alpha > 250 = character) never count the floor.
import { useEffect, useMemo, useRef } from 'react';
import { useFrame } from '@react-three/fiber';
import { CanvasTexture, Color, Vector3, type Group, type Mesh, type MeshBasicMaterial, type Object3D } from 'three';
import { prefersInstantCamera } from './stage-camera';

let shadowTexture: CanvasTexture | null = null;
function contactShadowTexture() {
  if (shadowTexture) return shadowTexture;
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 128;
  const context = canvas.getContext('2d')!;
  const gradient = context.createRadialGradient(64, 64, 0, 64, 64, 64);
  gradient.addColorStop(0, 'rgba(23,21,28,.38)');
  gradient.addColorStop(.45, 'rgba(23,21,28,.16)');
  gradient.addColorStop(1, 'rgba(23,21,28,0)');
  context.fillStyle = gradient;
  context.fillRect(0, 0, 128, 128);
  shadowTexture = new CanvasTexture(canvas);
  return shadowTexture;
}

const hipsWorld = new Vector3(), hipsLocal = new Vector3();
const easeOutExpo = (t: number) => t >= 1 ? 1 : 1 - Math.pow(2, -10 * t);

type Props = {
  /** Character root; its hips drive the shadow position. */
  subject: Object3D | null;
  /** Character size multiplier (shadow and ring scale with it). */
  scale: number;
  /** Standing height at scale 1, in world units (weapon excluded). The ring and the
   * contact shadow are proportional to it, so every cast member stands on the same-looking
   * floor mark. Nothing is drawn until it is known (> 0). */
  restHeight: number;
  accent?: string;
  /** Changes on user view/framing changes: the ring pulses 1 → 1.06 → 1 (M2). */
  pulseKey?: string;
  /** False until the character first appears: the shadow grows .6 → 1 (M1). */
  visible: boolean;
};

/** Ring radius and shadow diameter per unit of standing height (matched to ナミ's look). */
export const STAGE_RING_PER_HEIGHT = .245;
export const STAGE_SHADOW_PER_HEIGHT = .4;

export function StageFloor({ subject, scale, restHeight, accent, pulseKey, visible: shown }: Props) {
  const ringRadius = Math.max(restHeight, 0) * STAGE_RING_PER_HEIGHT, shadowDiameter = Math.max(restHeight, 0) * STAGE_SHADOW_PER_HEIGHT;
  const visible = shown && restHeight > 0;
  const shadow = useRef<Mesh>(null), ring = useRef<Group>(null);
  const texture = useMemo(() => typeof document === 'undefined' ? null : contactShadowTexture(), []);
  const color = useMemo(() => new Color(accent || '#B0512A'), [accent]);
  const follow = useRef(new Vector3()), hips = useRef<Object3D | null>(null);
  const entrance = useRef({ started: false, elapsed: 0 }), pulse = useRef({ elapsed: 1, key: pulseKey });
  const instant = useMemo(() => prefersInstantCamera(), []);
  useEffect(() => {
    hips.current = subject?.getObjectByName('Hips') ?? subject?.getObjectByName('J_Bip_C_Hips') ?? null;
    follow.current.set(0, 0, 0);
  }, [subject]);
  useEffect(() => {
    if (pulse.current.key !== pulseKey) { pulse.current.key = pulseKey; pulse.current.elapsed = instant ? 1 : 0; }
  }, [pulseKey, instant]);
  useFrame((_, delta) => {
    const plane = shadow.current, circle = ring.current;
    if (!plane || !circle) return;
    const bone = hips.current;
    if (bone && plane.parent) {
      bone.getWorldPosition(hipsWorld);
      hipsLocal.copy(hipsWorld); plane.parent.worldToLocal(hipsLocal);
      follow.current.x += (hipsLocal.x - follow.current.x) * .2;
      follow.current.z += (hipsLocal.z - follow.current.z) * .2;
    }
    plane.position.set(follow.current.x, .002, follow.current.z);
    // Entrance: shadow scale .6 → 1 over 400 ms once the character is visible.
    const state = entrance.current;
    if (visible && !state.started) { state.started = true; state.elapsed = instant ? 1 : 0; }
    if (state.started && state.elapsed < .4) state.elapsed += delta;
    const grow = !state.started ? 0 : instant ? 1 : .6 + .4 * easeOutExpo(Math.min(1, state.elapsed / .4));
    const diameter = shadowDiameter * scale * grow;
    plane.scale.set(Math.max(diameter, 1e-4), Math.max(diameter, 1e-4), 1);
    (plane.material as MeshBasicMaterial).opacity = !state.started ? 0 : instant ? 1 : Math.min(1, state.elapsed / .4);
    // Ring pulse 1 → 1.06 → 1 over 300 ms on a turntable move.
    const p = pulse.current;
    if (p.elapsed < .3) p.elapsed += delta;
    const bump = p.elapsed < .3 ? Math.sin(Math.PI * Math.min(1, p.elapsed / .3)) * .06 : 0;
    circle.scale.setScalar(scale * (1 + bump));
    circle.visible = state.started;
  });
  return <group>
    <mesh ref={shadow} rotation={[-Math.PI / 2, 0, 0]} renderOrder={-1}>
      <planeGeometry args={[1, 1]} />
      <meshBasicMaterial map={texture} transparent depthWrite={false} toneMapped={false} opacity={0} />
    </mesh>
    <group ref={ring} visible={false}>
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, .001, 0]} renderOrder={-1}>
        <ringGeometry args={[Math.max(ringRadius, 1e-3), Math.max(ringRadius, 1e-3) * 1.0194, 96]} />
        <meshBasicMaterial color={color} transparent opacity={.22} depthWrite={false} toneMapped={false} />
      </mesh>
    </group>
  </group>;
}
