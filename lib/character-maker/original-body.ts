import { BufferAttribute, BufferGeometry, InterleavedBufferAttribute, Matrix3, Matrix4, Mesh, SkinnedMesh, Vector3, type Object3D } from 'three';

// The same restrained rest-space field is used by preview and export. Its
// proportions are character-specific; the skeleton and its animation stay put.
const BODY_PROFILES: Record<string, { width: number; depth: number; arm: number; leg: number; depthEnvelope?: [number, number] }> = {
  'generic-human-a': { width: .24, depth: .25, arm: .18, leg: .19 },
  'generic-human-b': { width: .22, depth: .23, arm: .17, leg: .18 },
  'generic-robot': { width: .16, depth: .16, arm: .12, leg: .13 },
  'generic-cat': { width: .24, depth: .23, arm: .14, leg: .16, depthEnvelope: [.65, 1.1] },
};

type Attribute = BufferAttribute | InterleavedBufferAttribute;
type BodyPart = { start: Vector3; axis: Vector3; length: number; strength: number; taperEnd: boolean };
type Surface = { position: Attribute; base: Float32Array; delta: Float32Array; geometries: Set<BufferGeometry> };
const smooth = (start: number, end: number, value: number) => {
  const t = Math.max(0, Math.min(1, (value - start) / (end - start)));
  return t * t * (3 - 2 * t);
};

/** Own geometry separately from useGLTF's immutable cache, without multiplying
 * shared vertex/morph attributes across material primitives. No source target,
 * dictionary, weight, skin attribute, or index is rewritten. */
export function createOriginalBody(model: Object3D, characterId: string) {
  const profile = BODY_PROFILES[characterId];
  if (!profile) throw new Error('このキャラクターの体型設定が見つかりません。');
  model.updateWorldMatrix(true, true);
  const inverseModel = model.matrixWorld.clone().invert();
  const joint = (name: string) => {
    const bone = model.getObjectByName(name);
    if (!bone) throw new Error('体型を調整する骨を読み込めませんでした。');
    return bone.getWorldPosition(new Vector3()).applyMatrix4(inverseModel);
  };
  const hip = joint('Hip'), neck = joint('NeckTwist01');
  const up = neck.clone().sub(hip), torsoLength = up.length();
  if (torsoLength < 1e-5) throw new Error('体型を調整する基準位置を読み込めませんでした。');
  up.normalize();
  const right = joint('R_Upperarm').sub(joint('L_Upperarm'));
  right.addScaledVector(up, -right.dot(up)).normalize();
  const forward = new Vector3().crossVectors(right, up).normalize();
  const limbs = new Map<string, BodyPart>();
  for (const side of ['L', 'R']) for (const [part, end, strength, taperEnd] of [
    ['Upperarm', 'Forearm', profile.arm, false], ['Forearm', 'Hand', profile.arm, true],
    ['Thigh', 'Calf', profile.leg, false], ['Calf', 'Foot', profile.leg, true],
  ] as const) {
    const start = joint(`${side}_${part}`), axis = joint(`${side}_${end}`).sub(start);
    const length = axis.length();
    if (length < 1e-5) continue;
    const entry = { start, axis: axis.normalize(), length, strength, taperEnd };
    for (const suffix of ['', 'Twist01', 'Twist02']) limbs.set(`${side}_${part}${suffix}`, entry);
  }

  const attributes = new Map<Attribute, Attribute>();
  const geometries = new Set<BufferGeometry>();
  const surfaces = new Map<Attribute, Surface>();
  const privateAttribute = (source: Attribute) => {
    let result = attributes.get(source);
    if (!result) {
      if (source instanceof InterleavedBufferAttribute) {
        // GLB buffer views can share one ArrayBuffer at different byte offsets.
        // Copy raw components explicitly: InterleavedBuffer.clone's buffer cache
        // does not retain those view offsets. Keep normalized integer data raw.
        const ArrayType = source.array.constructor as new (length: number) => BufferAttribute['array'];
        const packed = new ArrayType(source.count * source.itemSize);
        for (let vertex = 0; vertex < source.count; vertex++) for (let component = 0; component < source.itemSize; component++) {
          packed[vertex * source.itemSize + component] = source.array[vertex * source.data.stride + source.offset + component];
        }
        result = new BufferAttribute(packed, source.itemSize, source.normalized).setUsage(source.data.usage);
        result.name = source.name;
      } else result = source.clone();
      attributes.set(source, result);
    }
    return result;
  };
  const point = new Vector3(), relative = new Vector3(), offset = new Vector3(), delta = new Vector3();
  const meshToModel = new Matrix4(), modelToMesh = new Matrix3();
  model.traverse(object => {
    if (!(object instanceof Mesh)) return;
    const source: BufferGeometry = object.geometry;
    const geometry = new BufferGeometry();
    geometry.name = source.name;
    geometry.userData = structuredClone(source.userData);
    for (const [name, attribute] of Object.entries(source.attributes)) geometry.setAttribute(name, privateAttribute(attribute));
    if (source.index) geometry.setIndex(privateAttribute(source.index) as BufferAttribute);
    for (const [name, targets] of Object.entries(source.morphAttributes)) {
      (geometry.morphAttributes as Record<string, Attribute[]>)[name] = targets!.map(privateAttribute);
    }
    geometry.morphTargetsRelative = source.morphTargetsRelative;
    geometry.groups = source.groups.map(group => ({ ...group }));
    geometry.setDrawRange(source.drawRange.start, source.drawRange.count);
    geometry.boundingBox = source.boundingBox?.clone() ?? null;
    geometry.boundingSphere = source.boundingSphere?.clone() ?? null;
    object.geometry = geometry;
    geometries.add(geometry);
    const position = geometry.getAttribute('position');
    if (!position || !(object instanceof SkinnedMesh)) return;
    // Primitives sharing POSITION also share their rest transform and skin in
    // the validated original models. Reuse that field and private buffer.
    const existing = surfaces.get(position);
    if (existing) { existing.geometries.add(geometry); return; }
    const skinIndex = geometry.getAttribute('skinIndex'), skinWeight = geometry.getAttribute('skinWeight');
    if (!skinIndex || !skinWeight) return;
    meshToModel.multiplyMatrices(inverseModel, object.matrixWorld);
    modelToMesh.setFromMatrix4(meshToModel.clone().invert());
    const base = new Float32Array(position.count * 3), changes = new Float32Array(position.count * 3);
    for (let vertex = 0; vertex < position.count; vertex++) {
      point.fromBufferAttribute(position, vertex);
      point.toArray(base, vertex * 3);
      point.applyMatrix4(meshToModel);
      delta.set(0, 0, 0);
      for (let influence = 0; influence < 4; influence++) {
        const weight = skinWeight.getComponent(vertex, influence);
        if (weight <= 0) continue;
        const name = object.skeleton.bones[skinIndex.getComponent(vertex, influence)]?.name;
        if (!name) continue;
        if (/^(Hip|Pelvis|Waist|Spine01|Spine02|[LR]_Clavicle)$/.test(name)) {
          relative.copy(point).sub(hip);
          const height = relative.dot(up) / torsoLength;
          const fade = 1 - smooth(.82, 1, height);
          offset.copy(right).multiplyScalar(relative.dot(right) * profile.width)
            .addScaledVector(forward, relative.dot(forward) * profile.depth);
          delta.addScaledVector(offset, weight * fade);
        } else {
          const part = limbs.get(name);
          if (!part) continue; // Head, neck, hair, hands, feet and display faces.
          relative.copy(point).sub(part.start);
          const projection = relative.dot(part.axis);
          const fade = part.taperEnd ? 1 - smooth(.72, 1, projection / part.length) : 1;
          offset.copy(relative).addScaledVector(part.axis, -projection);
          delta.addScaledVector(offset, weight * fade * part.strength);
        }
      }
      // Moka's tail shares spine/thigh weights with the torso. Fade outside its
      // measured body depth so changing build does not expand the tail tip.
      if (profile.depthEnvelope) {
        const depth = Math.abs(relative.copy(point).sub(hip).dot(forward)) / torsoLength;
        delta.multiplyScalar(1 - smooth(...profile.depthEnvelope, depth));
      }
      delta.applyMatrix3(modelToMesh).toArray(changes, vertex * 3);
    }
    surfaces.set(position, { position, base, delta: changes, geometries: new Set([geometry]) });
  });

  let previous = Number.NaN;
  return {
    apply(build: number) {
      if (!Number.isFinite(build) || build < -1 || build > 1) throw new Error('体型は−1〜1の範囲で選んでください。');
      if (build === previous) return;
      previous = build;
      for (const { position, base, delta, geometries: affected } of surfaces.values()) {
        for (let vertex = 0; vertex < position.count; vertex++) {
          const at = vertex * 3;
          position.setXYZ(vertex, base[at] + build * delta[at], base[at + 1] + build * delta[at + 1], base[at + 2] + build * delta[at + 2]);
        }
        position.needsUpdate = true;
        for (const geometry of affected) { geometry.computeBoundingBox(); geometry.computeBoundingSphere(); }
      }
    },
    dispose() { geometries.forEach(geometry => geometry.dispose()); },
  };
}
