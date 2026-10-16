// The generic-character viewer's own copy of lib/character-hair-springs.ts (2026-10-01). Frozen (components/rig-actor/kind-copies.test.mjs).
import { Object3D, Quaternion, Vector3 } from "three";

/** Fixed-step secondary motion, based on the VRMC_springBone tail/length model.
 * Uses a damped positional spring and small angular limits for this short bob.
 * https://github.com/vrm-c/vrm-specification/tree/master/specification/VRMC_springBone-1.0
 * This is not a complete VRM loader. Parameters and rest poses live in GLB extras.
 */
export const HAIR_STEP = 1 / 120;
type Settings = { child: string; frequency: number; dampingRatio: number; maxAngle: number; restRotation: number[] };
type Joint = { bone: Object3D; child: Object3D; settings: Settings; rest: Quaternion; axis: Vector3; current: Vector3; previous: Vector3 };

export class CharacterHairSprings {
  readonly joints: Joint[] = [];
  private model: Object3D;
  private root = new Vector3();
  private target = new Vector3();
  private direction = new Vector3();
  private restDirection = new Vector3();
  private next = new Vector3();
  private velocity = new Vector3();
  private force = new Vector3();
  private parentRotation = new Quaternion();
  private worldRestRotation = new Quaternion();
  private rotation = new Quaternion();
  private identity = new Quaternion();

  constructor(model: Object3D) {
    this.model = model;
    // Depth-first traversal orders each chain from the scalp toward its tip.
    model.traverse(bone => {
      const settings = bone.userData.aisnsHairJoint as Settings | undefined;
      if (!settings) return;
      const child = model.getObjectByName(settings.child);
      if (!child) throw new Error(`Missing hair endpoint: ${settings.child}`);
      this.joints.push({ bone, child, settings, rest: new Quaternion().fromArray(settings.restRotation),
        axis: child.position.clone().normalize(), current: new Vector3(), previous: new Vector3() });
    });
    this.reset();
  }

  restorePose() {
    for (const joint of this.joints) joint.bone.quaternion.copy(joint.rest);
    this.model.updateWorldMatrix(true, true);
  }

  reset() {
    this.restorePose();
    for (const joint of this.joints) {
      joint.child.getWorldPosition(joint.current);
      joint.previous.copy(joint.current);
    }
  }

  /** Caller advances the body pose with the same fixed timestep first. */
  update(dt = HAIR_STEP, strength = 1) {
    if (strength <= 0) { this.reset(); return; }
    this.restorePose();
    for (const joint of this.joints) {
      const { bone, child, settings, rest, axis, current, previous } = joint;
      bone.getWorldPosition(this.root);
      child.getWorldPosition(this.target);
      const length = this.root.distanceTo(this.target);
      if (length < 1e-8) continue;
      this.restDirection.subVectors(this.target, this.root).divideScalar(length);
      const omega = Math.PI * 2 * settings.frequency;
      const drag = Math.exp(-2 * settings.dampingRatio * omega * dt);
      this.velocity.subVectors(current, previous).multiplyScalar(drag);
      this.force.subVectors(this.target, current);
      this.next.copy(current).add(this.velocity).addScaledVector(this.force, omega * omega * dt * dt);
      this.direction.subVectors(this.next, this.root).normalize();
      const angle = this.restDirection.angleTo(this.direction);
      const limit = settings.maxAngle * strength;
      if (angle > limit) {
        this.rotation.setFromUnitVectors(this.restDirection, this.direction);
        this.rotation.slerp(this.identity, 1 - limit / angle);
        this.direction.copy(this.restDirection).applyQuaternion(this.rotation);
      }
      this.next.copy(this.root).addScaledVector(this.direction, length);
      previous.copy(current); current.copy(this.next);
      if (bone.parent) bone.parent.getWorldQuaternion(this.parentRotation);
      else this.parentRotation.identity();
      this.worldRestRotation.copy(this.parentRotation).multiply(rest).invert();
      this.direction.applyQuaternion(this.worldRestRotation).normalize();
      this.rotation.setFromUnitVectors(axis, this.direction);
      bone.quaternion.copy(rest).multiply(this.rotation).normalize();
      bone.updateWorldMatrix(false, true);
    }
  }
}
