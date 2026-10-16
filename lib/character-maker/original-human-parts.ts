import { BufferAttribute, BufferGeometry, InterleavedBufferAttribute, Mesh, type Object3D } from 'three';
import type { OriginalSpeciesParts } from './original-species';

type Attribute = BufferAttribute | InterleavedBufferAttribute;
type Fit = { scale: [number, number, number]; offset: [number, number, number] };
type FaceProfile = {
  eyes: { side: string; center: [number, number]; radius: [number, number] }[];
  mouth: { center: [number, number]; radius: [number, number] };
  headEnvelope: [number, number, number, number];
};
const smooth = (a: number, b: number, v: number) => {
  const t = Math.max(0, Math.min(1, (v - a) / (b - a)));
  return t * t * (3 - 2 * t);
};

/** The independent head and all its facial targets use one rest-space field.
 * Apply F(Basis + delta) - F(Basis), not a differently fitted old morph array.
 * The original body, collar, neck attachment and skeleton are not changed.
 */
export function configureOriginalHumanParts(model: Object3D, characterId: string, parts: OriginalSpeciesParts) {
  const geometries: BufferGeometry[] = [];
  if (!['generic-human-a', 'generic-human-b'].includes(characterId)) return { dispose() {} };
  if (parts.head === 'original' && parts.eyes === 'original' && parts.mouth === 'original') return { dispose() {} };
  const copy = (source: Attribute) => {
    if (!(source instanceof InterleavedBufferAttribute)) return source.clone();
    const Type = source.array.constructor as new (length: number) => BufferAttribute['array'];
    const data = new Type(source.count * source.itemSize);
    for (let i = 0; i < source.count; i++) for (let c = 0; c < source.itemSize; c++) data[i * source.itemSize + c] = source.array[i * source.data.stride + source.offset + c];
    const result = new BufferAttribute(data, source.itemSize, source.normalized);
    result.name = source.name;
    return result;
  };
  model.traverse(object => {
    if (!(object instanceof Mesh) || !['head', 'hair'].includes(object.userData.originalModularPart)) return;
    const fit = object.userData.originalModularFit as Fit | undefined;
    const profile = object.userData.originalModularFaceProfile as FaceProfile | undefined;
    if (!fit || !profile) return;
    const source: BufferGeometry = object.geometry, geometry = new BufferGeometry();
    const eyeRole = source.userData.originalMakerEyeRole ?? object.userData.originalMakerEyeRole;
    // Narrow eyes are a lid-aperture change. Keep the new static iris/sclera
    // round behind the moving lids instead of vertically crushing its texture.
    const preserveIrisAperture = eyeRole === 'iris-sclera' && parts.eyes === 'narrow';
    geometry.name = source.name; geometry.userData = structuredClone(source.userData);
    for (const [name, attribute] of Object.entries(source.attributes)) geometry.setAttribute(name, copy(attribute));
    if (source.index) geometry.setIndex(copy(source.index));
    for (const [name, targets] of Object.entries(source.morphAttributes)) (geometry.morphAttributes as Record<string, Attribute[]>)[name] = targets!.map(copy);
    geometry.groups = source.groups.map(group => ({ ...group }));
    geometry.setDrawRange(source.drawRange.start, source.drawRange.count);
    geometry.morphTargetsRelative = source.morphTargetsRelative;
    object.geometry = geometry; geometries.push(geometry);
    const position = geometry.getAttribute('position'), targets = geometry.morphAttributes.position ?? [];
    const transform = (x: number, y: number, z: number) => {
      // Native original models face +X and use half the viewer's dimensions.
      let px = (-2 * z - fit.offset[0]) / fit.scale[0];
      let py = (2 * y - fit.offset[1]) / fit.scale[1];
      const pz = (2 * x - fit.offset[2]) / fit.scale[2];
      const originalX = px, originalY = py;
      const headEnvelope = smooth(profile.headEnvelope[0], profile.headEnvelope[1], py) * (1 - smooth(profile.headEnvelope[2], profile.headEnvelope[3], py));
      if (parts.head === 'rounded') px *= 1 + .105 * headEnvelope;
      if (parts.head === 'slim') px *= 1 - .10 * headEnvelope;
      if (parts.eyes !== 'original' && !preserveIrisAperture) {
        const eye = profile.eyes.reduce((a, b) => Math.abs(originalX - a.center[0]) < Math.abs(originalX - b.center[0]) ? a : b);
        const [cx, cy] = eye.center;
        const radius = Math.hypot((originalX - cx) / eye.radius[0], (originalY - cy) / eye.radius[1]);
        const amount = (1 - smooth(.68, 1.2, radius)) * smooth(.02, .19, pz);
        const sx = parts.eyes === 'large' ? 1.13 : 1.045, sy = parts.eyes === 'large' ? 1.14 : .77;
        px += (originalX - cx) * (sx - 1) * amount;
        py += (originalY - cy) * (sy - 1) * amount;
      }
      if (parts.mouth !== 'original') {
        const radius = Math.hypot((originalX - profile.mouth.center[0]) / profile.mouth.radius[0], (originalY - profile.mouth.center[1]) / profile.mouth.radius[1]);
        const amount = (1 - smooth(.72, 1.35, radius)) * smooth(.02, .17, pz);
        const scale = parts.mouth === 'wide' ? 1.17 : .82;
        px += (originalX - profile.mouth.center[0]) * (scale - 1) * amount;
      }
      return [(pz * fit.scale[2] + fit.offset[2]) / 2, (py * fit.scale[1] + fit.offset[1]) / 2, -(px * fit.scale[0] + fit.offset[0]) / 2];
    };
    const names = Object.fromEntries(Object.entries(object.morphTargetDictionary ?? {}).map(([name, index]) => [index, name]));
    for (let i = 0; i < position.count; i++) {
      const bx = position.getX(i), by = position.getY(i), bz = position.getZ(i), p = transform(bx, by, bz);
      const before = targets.map(target => [target.getX(i), target.getY(i), target.getZ(i)]);
      for (let index = 0; index < targets.length; index++) {
        const target = targets[index], [tx, ty, tz] = before[index];
        const corrective = names[index]?.match(/^(blinkLeft|blinkRight)Inbetween(25|50|75)$/);
        if (corrective && geometry.morphTargetsRelative) {
          const mainIndex = object.morphTargetDictionary?.[corrective[1]];
          if (mainIndex !== undefined) {
            const f = Number(corrective[2]) / 100, main = before[mainIndex];
            const full = transform(bx + main[0], by + main[1], bz + main[2]);
            const mid = transform(bx + f * main[0] + tx, by + f * main[1] + ty, bz + f * main[2] + tz);
            target.setXYZ(i, mid[0] - p[0] - f * (full[0] - p[0]), mid[1] - p[1] - f * (full[1] - p[1]), mid[2] - p[2] - f * (full[2] - p[2]));
            continue;
          }
        }
        const t = transform(tx + (geometry.morphTargetsRelative ? bx : 0), ty + (geometry.morphTargetsRelative ? by : 0), tz + (geometry.morphTargetsRelative ? bz : 0));
        target.setXYZ(i, t[0] - (geometry.morphTargetsRelative ? p[0] : 0), t[1] - (geometry.morphTargetsRelative ? p[1] : 0), t[2] - (geometry.morphTargetsRelative ? p[2] : 0));
      }
      position.setXYZ(i, p[0], p[1], p[2]);
    }
    position.needsUpdate = true; targets.forEach(target => { target.needsUpdate = true; });
    if (geometry.hasAttribute('normal')) geometry.computeVertexNormals();
    geometry.computeBoundingBox(); geometry.computeBoundingSphere();
  });
  return { dispose() { geometries.forEach(geometry => geometry.dispose()); } };
}
