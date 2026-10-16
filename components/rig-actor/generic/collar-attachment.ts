// The generic-character viewer's own copy of lib/character-collar-attachment.ts (2026-10-01). Frozen (components/rig-actor/kind-copies.test.mjs).
import { Matrix4, Object3D } from "three";

type MountSettings = { point: number[]; sources: { name: string; weight: number; inverseBindMatrix: number[] }[] };

/** Follow the collar's measured skin blend, rather than a different torso bone.
 * Both the clip baker and the live head-turn control use this attachment.
 */
export class CharacterCollarAttachment {
  private mounts: { node: Object3D; point: Matrix4; sources: { bone: Object3D; weight: number; inverse: Matrix4 }[] }[] = [];
  private skin = new Matrix4();
  private blended = new Matrix4();
  private parentInverse = new Matrix4();

  constructor(model: Object3D) {
    model.traverse(node => {
      const settings = node.userData.aisnsCollarAttachment as MountSettings | undefined;
      if (!settings) return;
      this.mounts.push({ node, point: new Matrix4().makeTranslation(...settings.point as [number, number, number]),
        sources: settings.sources.map(source => {
          const bone = model.getObjectByName(source.name);
          if (!bone) throw new Error(`Missing collar source: ${source.name}`);
          return { bone, weight: source.weight, inverse: new Matrix4().fromArray(source.inverseBindMatrix) };
        }) });
    });
  }

  update() {
    for (const { node, point, sources } of this.mounts) {
      this.blended.elements.fill(0);
      for (const { bone, weight, inverse } of sources) {
        bone.updateWorldMatrix(true, false);
        this.skin.multiplyMatrices(bone.matrixWorld, inverse);
        for (let i = 0; i < 16; i++) this.blended.elements[i] += this.skin.elements[i] * weight;
      }
      this.blended.multiply(point);
      if (node.parent) {
        node.parent.updateWorldMatrix(true, false);
        this.parentInverse.copy(node.parent.matrixWorld).invert();
        this.blended.premultiply(this.parentInverse);
      }
      this.blended.decompose(node.position, node.quaternion, node.scale);
      node.updateWorldMatrix(false, true);
    }
  }
}
