// The generic-character viewer's own copy of characters/aichan/runtime/character-tail-motion.ts (2026-10-01). Frozen (components/rig-actor/kind-copies.test.mjs).
import * as THREE from 'three';

export type TailMotion = 'idle' | 'walk' | 'run';
const SETTINGS = {
  idle: { period: 12.25 / 3, amplitude: .021, curl: .85, sway: .010 },
  walk: { period: 1.149999976158142, amplitude: .052, curl: 1.05, sway: .018 },
  run: { period: 1, amplitude: .110, curl: 1.40, sway: .035 },
};

/** Art-directed traveling bend on a fixed-length chain, in radians/seconds.
 * The root stays on Hip; distal phases lag behind it. This is animation, not
 * a collision simulation. The same controller is baked into the portable GLB.
 * Root-to-tip chain convention: VRMC_springBone-1.0, used as construction input.
 */
export class CharacterTailMotion {
  joints: { bone: THREE.Object3D; rest: THREE.Quaternion; index: number; count: number }[] = [];
  phase = 0;
  private amplitude = SETTINGS.idle.amplitude;
  private curl = SETTINGS.idle.curl;
  private sway = SETTINGS.idle.sway;
  private frequency = 1 / SETTINGS.idle.period;
  private x = new THREE.Vector3(1, 0, 0);
  private y = new THREE.Vector3(0, 1, 0);
  private bend = new THREE.Quaternion();
  private turn = new THREE.Quaternion();
  constructor(root: THREE.Object3D) {
    root.traverse(bone => {
      const data = bone.userData.aisnsTailJoint;
      if (data) this.joints.push({ bone, rest: new THREE.Quaternion().fromArray(data.restRotation), index: data.index, count: data.count });
    });
    this.joints.sort((a, b) => a.index - b.index);
  }
  seek(time: number, motion: TailMotion) {
    const settings = SETTINGS[motion];
    this.phase = time / settings.period * Math.PI * 2;
    this.amplitude = settings.amplitude; this.curl = settings.curl;
    this.sway = settings.sway; this.frequency = 1 / settings.period;
    this.apply();
  }
  update(dt: number, motion: TailMotion, speed = 1) {
    const settings = SETTINGS[motion];
    this.amplitude = THREE.MathUtils.damp(this.amplitude, settings.amplitude, 7, dt);
    this.curl = THREE.MathUtils.damp(this.curl, settings.curl, 7, dt);
    this.sway = THREE.MathUtils.damp(this.sway, settings.sway, 7, dt);
    this.frequency = THREE.MathUtils.damp(this.frequency, 1 / settings.period, 7, dt);
    this.phase = (this.phase + dt * speed * this.frequency * Math.PI * 2) % (Math.PI * 2);
    this.apply();
  }
  private apply() {
    for (const joint of this.joints) {
      const t = joint.index / (joint.count - 1);
      const wave = Math.sin(this.phase - t * 2.1) + .17 * Math.sin(this.phase * 2 - t * 2.7);
      const curl = (.012 + .330 * t * t * t) * this.curl;
      const angle = curl + this.amplitude * (.35 + .85 * t) * wave;
      this.bend.setFromAxisAngle(this.x, angle);
      this.turn.setFromAxisAngle(this.y, this.sway * Math.sin(this.phase - t * 1.7) * (.2 + .8 * t));
      joint.bone.quaternion.copy(joint.rest).multiply(this.bend).multiply(this.turn);
      joint.bone.updateMatrix();
    }
  }
}
