import { AnimationMixer, Box3, LoopOnce, Matrix4, Mesh, SkinnedMesh, Vector3, type AnimationClip, type Object3D } from 'three';
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js';

export type OriginalCameraView = 'front' | 'side' | 'back';

/** Fit both viewport axes: a portrait canvas must not crop wide poses/weapons. */
export function originalCameraFit(bounds: Box3, view: OriginalCameraView, width: number, height: number, padding = 1.16) {
  const span = bounds.getSize(new Vector3());
  const horizontal = view === 'side' ? span.z : span.x;
  return {
    target: bounds.getCenter(new Vector3()),
    zoom: Math.min(Math.max(width, 1) / Math.max(horizontal * padding, .01), Math.max(height, 1) / Math.max(span.y * padding, .01)),
    distance: Math.max(5, span.length() * 1.5),
  };
}

const vertices = new WeakMap<Mesh, { key: string; indices: number[]; geometry: Mesh['geometry'] }>();
function drawnVertices(mesh: Mesh) {
  const geometry = mesh.geometry, index = geometry.index;
  const materials = Array.isArray(mesh.material) ? mesh.material : [mesh.material];
  const visible = materials.map(material => material.visible && (!material.transparent || material.opacity > 0));
  const key = `${index?.version}:${index?.count}:${geometry.attributes.position.count}:${geometry.drawRange.start}:${geometry.drawRange.count}:${visible.join(',')}:${geometry.groups.map(group => `${group.start},${group.count},${group.materialIndex}`).join(';')}`;
  const cached = vertices.get(mesh);
  if (cached?.geometry === geometry && cached.key === key) return cached.indices;
  // GLTF material primitives can share POSITION while drawing different indices.
  // Measuring every POSITION would include hidden/replaced hair and body parts.
  const count = index?.count ?? geometry.attributes.position.count;
  const seen = new Set<number>();
  const ranges = Array.isArray(mesh.material) ? geometry.groups : [{ start: 0, count, materialIndex: 0 }];
  for (const range of ranges) {
    if (!visible[range.materialIndex ?? 0]) continue;
    const start = Math.max(range.start, geometry.drawRange.start);
    const end = Math.min(count, range.start + range.count, geometry.drawRange.start + geometry.drawRange.count);
    for (let i = start; i < end; i++) seen.add(index ? index.getX(i) : i);
  }
  const indices = [...seen];
  vertices.set(mesh, { geometry, key, indices });
  return indices;
}

const point = new Vector3(), skinnedPoint = new Vector3();
const boneTransforms = new WeakMap<SkinnedMesh, Matrix4[]>();
function skinTransforms(mesh: SkinnedMesh) {
  let matrices = boneTransforms.get(mesh);
  if (!matrices || matrices.length !== mesh.skeleton.bones.length) {
    matrices = mesh.skeleton.bones.map(() => new Matrix4());
    boneTransforms.set(mesh, matrices);
  }
  mesh.skeleton.bones.forEach((bone, i) => matrices[i].multiplyMatrices(bone.matrixWorld, mesh.skeleton.boneInverses[i]).multiply(mesh.bindMatrix));
  return matrices;
}
function isWithin(object: Object3D, ancestor: Object3D) {
  for (let current: Object3D | null = object; current; current = current.parent) if (current === ancestor) return true;
  return false;
}
function isWeapon(object: Object3D) {
  for (let current: Object3D | null = object; current; current = current.parent) if (current.userData?.registeredWeapon !== undefined) return true;
  return false;
}

/** Current rendered vertices, including nested calibration, body edits, morphs,
 * skinning, and bone-attached accessories. Never reuse a rest-pose bounding box.
 * `excludeWeapons`, when given, receives the same vertices minus registered weapons. */
export function measureOriginalCameraBounds(root: Object3D, head: Object3D | null, bodyBounds: Box3, headBounds: Box3, excludeWeapons?: Box3) {
  bodyBounds.makeEmpty(); headBounds.makeEmpty(); excludeWeapons?.makeEmpty();
  root.updateWorldMatrix(true, true);
  // SkinnedMesh.updateMatrixWorld refreshes its bindMatrixInverse too. Calling
  // only updateWorldMatrix leaves that value stale after an outer size change.
  root.updateMatrixWorld(true);
  root.traverseVisible(object => {
    if (!(object instanceof Mesh) || !object.geometry.attributes.position || Math.abs(object.matrixWorld.determinant()) < 1e-12) return;
    const attachedToHead = head ? isWithin(object, head) : false;
    const bodyOnly = excludeWeapons && !isWeapon(object) ? excludeWeapons : null;
    const headBones = head && object instanceof SkinnedMesh ? object.skeleton.bones.map(bone => isWithin(bone, head)) : null;
    const skinIndex = object.geometry.attributes.skinIndex, skinWeight = object.geometry.attributes.skinWeight;
    const matrices = object instanceof SkinnedMesh ? skinTransforms(object) : null;
    for (const index of drawnVertices(object)) {
      // Apply morphs first, then skin with once-per-mesh bone matrices. Three's
      // SkinnedMesh.getVertexPosition repeats matrix products for every vertex.
      Mesh.prototype.getVertexPosition.call(object, index, point);
      if (matrices && skinIndex && skinWeight && object instanceof SkinnedMesh) {
        const x = point.x, y = point.y, z = point.z;
        skinnedPoint.set(0, 0, 0);
        for (let component = 0; component < 4; component++) {
          const weight = skinWeight.getComponent(index, component);
          if (weight === 0) continue;
          const e = matrices[skinIndex.getComponent(index, component)].elements;
          skinnedPoint.x += (e[0] * x + e[4] * y + e[8] * z + e[12]) * weight;
          skinnedPoint.y += (e[1] * x + e[5] * y + e[9] * z + e[13]) * weight;
          skinnedPoint.z += (e[2] * x + e[6] * y + e[10] * z + e[14]) * weight;
        }
        point.copy(skinnedPoint).applyMatrix4(object.bindMatrixInverse);
      }
      point.applyMatrix4(object.matrixWorld);
      if (!Number.isFinite(point.x + point.y + point.z)) continue;
      bodyBounds.expandByPoint(point);
      bodyOnly?.expandByPoint(point);
      let headWeight = 0;
      if (headBones && skinIndex && skinWeight) for (let component = 0; component < 4; component++) {
        if (headBones[skinIndex.getComponent(index, component)]) headWeight += skinWeight.getComponent(index, component);
      }
      if (attachedToHead || headWeight >= .25) headBounds.expandByPoint(point);
    }
  });
  if (!bodyBounds.isEmpty() && headBounds.isEmpty()) {
    // Non-skinned/unnamed rigs still get a useful upper-body close-up.
    headBounds.copy(bodyBounds);
    headBounds.min.y = bodyBounds.max.y - (bodyBounds.max.y - bodyBounds.min.y) * .32;
  }
  return !bodyBounds.isEmpty();
}

/** A disposable skeleton samples whole armed clips (`names`, default the idle +
 * attack pair) without seeking or resetting the live actor. Shared geometry and
 * materials are read-only. Padding covers extrema between these 12 Hz samples.
 * `bodyOnly` receives the same envelope without registered weapons. */
export function measureOriginalArmedCameraBounds(root: Object3D, clips: readonly AnimationClip[], bounds: Box3,
  names: readonly string[] = ['sword_idle', 'sword_attack'], bodyOnly?: Box3) {
  const armed = clips.filter(clip => names.includes(clip.name));
  if (armed.length !== names.length) return false;
  root.updateWorldMatrix(true, true);
  const sample = clone(root);
  root.matrixWorld.decompose(sample.position, sample.quaternion, sample.scale);
  const mixer = new AnimationMixer(sample), body = new Box3(), head = new Box3(), unarmed = bodyOnly ? new Box3() : undefined;
  bounds.makeEmpty(); bodyOnly?.makeEmpty();
  try {
    for (const clip of armed) {
      mixer.stopAllAction();
      const action = mixer.clipAction(clip).setLoop(LoopOnce, 1).play();
      action.clampWhenFinished = true;
      const steps = Math.ceil(clip.duration * 12);
      for (let frame = 0; frame <= steps; frame++) {
        action.time = clip.duration * frame / steps;
        mixer.update(0);
        if (measureOriginalCameraBounds(sample, null, body, head, unarmed)) {
          bounds.union(body);
          if (bodyOnly && unarmed && !unarmed.isEmpty()) bodyOnly.union(unarmed);
        }
      }
    }
  } finally {
    mixer.stopAllAction(); mixer.uncacheRoot(sample);
    const skeletons = new Set<SkinnedMesh['skeleton']>();
    sample.traverse(object => { if (object instanceof SkinnedMesh) skeletons.add(object.skeleton); });
    skeletons.forEach(skeleton => skeleton.dispose());
  }
  return !bounds.isEmpty();
}
