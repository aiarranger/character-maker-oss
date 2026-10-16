import { AnimationClip, Group, Matrix4, Mesh, Texture, Vector3, VectorKeyframeTrack, type Material, type Object3D } from 'three';
import type { MakerWeaponId } from './weapon-catalog';
import { makerWeaponFit, makerWeaponGripCompensation, type MakerWeaponRig } from './weapon-fit';
import { HIT_CLIP, HURT_FACE_CLIP } from './hurt';

export type MakerWeaponStyle = 'comic' | 'fantasy';
type MakerBase = 'feminine' | 'masculine';
type Vec3 = readonly [number, number, number];
type Socket = { bone: string; point: Vec3; blade: Vec3; palm: Vec3; size: number };
export const MAKER_SWORD_NAME = 'CharacterMakerSword';

// Raw hand-local frames, measured from the retained skeletons at Sword_Idle.
// The weapon points out of the thumb/index side of the closed fist. Body-build
// morphs keep the hand fixed; registered equipment inherits character height.
export const MAKER_WEAPON_SOCKETS: Record<MakerWeaponStyle, Record<MakerBase, Socket>> = {
  comic: {
    feminine: { bone: 'hand_r', point: [-0.0298, 0.0825, 0.0111], blade: [-0.0055, 0.2521, 0.9677], palm: [-1, 0, 0], size: 0.92 },
    masculine: { bone: 'hand_r', point: [-0.0357, 0.1087, 0.0076], blade: [-0.0052, 0.2565, 0.9665], palm: [-1, 0, 0], size: 1 },
  },
  fantasy: {
    feminine: { bone: 'J_Bip_R_Hand', point: [0.0633, -0.0182, -0.0084], blade: [0.01334, 0.05044, -0.99863], palm: [0, -1, 0], size: 0.9 },
    masculine: { bone: 'J_Bip_R_Hand', point: [0.0696, -0.0200, -0.0095], blade: [0.01262, 0.01226, -0.99985], palm: [0, -1, 0], size: 1 },
  },
};

/** Clone the immutable registered content; each attachment owns its resources. */
function ownedWeaponContent(source: Object3D): Object3D {
  const result = source.clone(true);
  const materials = new Map<Material, Material>();
  const textures = new Map<Texture, Texture>();
  result.traverse(object => {
    if (!(object instanceof Mesh)) return;
    object.geometry = object.geometry.clone(); object.frustumCulled = false;
    const own = (sourceMaterial: Material) => {
      let material = materials.get(sourceMaterial);
      if (!material) {
        material = sourceMaterial.clone();
        for (const [key, value] of Object.entries(material)) if (value instanceof Texture) {
          let texture = textures.get(value);
          if (!texture) { texture = value.clone(); textures.set(value, texture); }
          (material as unknown as Record<string, unknown>)[key] = texture;
        }
        materials.set(sourceMaterial, material);
      }
      return material;
    };
    object.material = Array.isArray(object.material) ? object.material.map(own) : own(object.material);
  });
  return result;
}

/** Derived instance geometry only: preserve the fitted handle around the fist,
 * enlarge the blade/head with its original proportions, and blend smoothly.
 * UVs, topology, registered source assets and shared templates are untouched.
 */
function enlargeWeaponOutsideGrip(content: Object3D, fit: ReturnType<typeof makerWeaponFit>) {
  content.updateMatrixWorld(true);
  const placement = new Matrix4().makeTranslation(...fit.gripOffset);
  const point = new Vector3();
  let minY = Infinity, maxY = -Infinity;
  content.traverse(object => {
    if (!(object instanceof Mesh)) return;
    const toGrip = placement.clone().multiply(object.matrixWorld);
    const fromGrip = toGrip.clone().invert();
    const geometry = object.geometry, positions = geometry.getAttribute('position');
    for (let i = 0; i < positions.count; i++) {
      point.fromBufferAttribute(positions, i).applyMatrix4(toGrip);
      point.multiplyScalar(makerWeaponGripCompensation(point.y * fit.sizeScale, fit));
      minY = Math.min(minY, point.y); maxY = Math.max(maxY, point.y);
      point.applyMatrix4(fromGrip);
      positions.setXYZ(i, point.x, point.y, point.z);
    }
    positions.needsUpdate = true;
    geometry.computeVertexNormals(); geometry.computeBoundingBox(); geometry.computeBoundingSphere();
    object.userData.makerWeaponGeometryTransform = fit.geometryTransform;
  });
  content.userData.makerWeaponGeometryTransform = { version: fit.geometryTransform, gripProtection: fit.gripProtection,
    sourceGeometryUnchanged: false, registeredSourceAssetsUnchanged: true,
    measuredLengthAtDefaultSize: Number.isFinite(maxY - minY) ? (maxY - minY) * fit.sizeScale * fit.visualScale : undefined };
  return content;
}

/** Existing scale-track/socket names are retained for all registered designs.
 * Registered content is already in the source catalog's grip-local frame.
 */
export function createMakerSword(style: MakerWeaponStyle, base: MakerBase, weaponId: MakerWeaponId, source: Object3D | null, rig: MakerWeaponRig = `${style}-${base}`): Group {
  if ((weaponId === 'none') !== (source === null)) throw new Error('選んだ武器のモデルを読み込めません。');
  const socket = MAKER_WEAPON_SOCKETS[style][base];
  const root = new Group(); root.name = MAKER_SWORD_NAME; root.visible = true; root.scale.setScalar(0);
  root.userData = { attachmentBone: socket.bone, makerWeapon: weaponId, visibilityTrack: `${MAKER_SWORD_NAME}.scale`,
    ...(source ? { registeredWeapon: structuredClone(source.userData.registeredWeapon) } : {}) };
  const grip = new Group(); grip.name = 'MakerSwordGrip';
  const y = new Vector3().fromArray(socket.blade).normalize();
  const z = new Vector3().fromArray(socket.palm); z.addScaledVector(y, -z.dot(y)).normalize();
  const x = new Vector3().crossVectors(y, z).normalize(); z.crossVectors(x, y).normalize();
  grip.position.fromArray(socket.point); grip.quaternion.setFromRotationMatrix(new Matrix4().makeBasis(x, y, z));
  grip.scale.setScalar(socket.size); root.add(grip);
  if (source) {
    const fit = makerWeaponFit(weaponId, rig);
    const visual = new Group(); visual.name = 'MakerWeaponVisualScale'; visual.scale.setScalar(fit.visualScale);
    const size = new Group(); size.name = 'MakerWeaponFit'; size.scale.setScalar(fit.sizeScale);
    const placement = new Group(); placement.name = 'MakerWeaponHandlePlacement'; placement.position.fromArray(fit.gripOffset);
    const content = enlargeWeaponOutsideGrip(ownedWeaponContent(source), fit);
    placement.add(content); size.add(placement); visual.add(size); grip.add(visual);
    const nominalLength = source.userData.registeredWeapon?.length;
    root.userData.makerWeaponFit = { ...fit, nominalLength, measuredLengthAtDefaultSize: content.userData.makerWeaponGeometryTransform.measuredLengthAtDefaultSize, calibratedLengthAtDefaultSize: typeof nominalLength === 'number' ? nominalLength * fit.effectiveScale : undefined };
  }
  return root;
}

/** Match the main-list meter convention without canceling the Maker size slider.
 * Socket points/axes remain in the retained hand frame; no geometry is changed. */
export function fitMakerWeaponScale(sword: Object3D, hand: Object3D, model: Object3D, baseCalibrationScale = 1) {
  hand.updateWorldMatrix(true, false);
  const boneScale = hand.getWorldScale(new Vector3());
  const parentScale = model.parent?.getWorldScale(new Vector3()) ?? new Vector3(1, 1, 1);
  boneScale.divide(parentScale).multiplyScalar(baseCalibrationScale);
  if (boneScale.toArray().some(value => !Number.isFinite(value) || Math.abs(value) < 1e-8)) throw new Error('武器の大きさを手の骨に合わせられません。');
  sword.getObjectByName('MakerSwordGrip')!.scale.set(1 / boneScale.x, 1 / boneScale.y, 1 / boneScale.z);
}

/** Dispose only instance-owned resources; keep the hierarchy for StrictMode. */
export function disposeMakerWeapon(root: Object3D) {
  const materials = new Set<Material>(), textures = new Set<Texture>();
  root.traverse(object => {
    if (!(object instanceof Mesh)) return;
    object.geometry.dispose();
    for (const material of Array.isArray(object.material) ? object.material : [object.material]) {
      materials.add(material);
      for (const value of Object.values(material)) if (value instanceof Texture) textures.add(value);
    }
  });
  materials.forEach(material => material.dispose()); textures.forEach(texture => texture.dispose());
}

/** Add the exportable show/hide track without modifying cached source clips.
 * のけぞり（hit）は武器ありの書き出しで武器を出し、標準では出さない。
 * ＞＜ だけのクリップ（face_hurt）は他の動作と重ねて使うので、武器のトラックを持たない。 */
export function addWeaponTracks(clips: readonly AnimationClip[], equipped = true): AnimationClip[] {
  return clips.map(source => {
    const clip = source.clone();
    clip.tracks = clip.tracks.filter(track => track.name !== `${MAKER_SWORD_NAME}.scale`);
    if (clip.name === HURT_FACE_CLIP) return clip;
    const value = equipped && ['sword_idle', 'sword_attack', HIT_CLIP].includes(clip.name) ? 1 : 0;
    clip.tracks.push(new VectorKeyframeTrack(`${MAKER_SWORD_NAME}.scale`, [0, clip.duration], Array(6).fill(value)));
    return clip;
  });
}
