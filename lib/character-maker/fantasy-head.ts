import { BufferAttribute, BufferGeometry, Mesh, type Object3D } from 'three';
import type { CharacterConfig } from './config';

const smooth = (a: number, b: number, x: number) => {
  const t = Math.max(0, Math.min(1, (x - a) / (b - a)));
  return t * t * (3 - 2 * t);
};
type Point = [number, number, number];
type Attribute = BufferGeometry['attributes'][string];
type NormalData = {
  normal: BufferAttribute;
  endpoints: Map<Attribute, Map<Attribute | undefined, BufferAttribute>>;
};
type WarpData = {
  position: BufferAttribute;
  deltas: Map<Attribute, BufferAttribute>;
  normals: Map<Attribute, NormalData>;
};
// Source GLTF accessors are immutable. These CPU-only results are never given to
// a mesh; each assembly receives private clones. Weak keys release cached warps
// when the corresponding source template is no longer retained by its loader.
const templateWarps = new WeakMap<Attribute, Map<string, WarpData>>();

/** The same anatomical warp is evaluated at neutral and every expression endpoint.
 * This creates a genuinely different head mesh without duplicate textures/rigs,
 * and keeps a closed lip/lid closed instead of adding a neutral-only face delta.
 * The eye pivots and upper scalp stay fixed so gaze and every fitted hair align.
 */
export function fantasyHeadPoint(base: CharacterConfig['base'], vx: number, y: number, z: number, body = false): Point {
  let px = vx, py = y, pz = z;
  if (base === 'masculine') {
    const x = vx - 0.00076345, ax = Math.abs(x), side = Math.sign(x);
    for (const s of [-1, 1]) {
      const cx = s * 0.042, cy = 1.7278;
      const w = Math.exp(-Math.pow((x - cx) / 0.039, 6) - Math.pow((y - cy) / 0.040, 6)) * smooth(-0.041, -0.068, z);
      px += (x - cx) * -0.10 * w;
      py += (y - cy) * -0.12 * w;
    }
    const jaw = Math.exp(-Math.pow((y - 1.651) / 0.041, 4)) * smooth(0.016, 0.057, ax) * (1 - smooth(0.085, 0.112, ax));
    px += side * 0.009 * jaw;
    const chin = (1 - smooth(1.631, 1.653, y)) * smooth(1.605, 1.622, y) * smooth(-0.052, -0.077, z);
    px += x * 0.20 * chin;
    py += (0.003 * (1 - smooth(0.006, 0.026, ax)) - 0.003 * smooth(0.02, 0.053, ax)) * chin;
    const bridge = Math.exp(-Math.pow(x / 0.018, 4) - Math.pow((y - 1.694) / 0.037, 4)) * smooth(-0.070, -0.097, z);
    pz -= 0.007 * bridge;
    py -= 0.003 * bridge;
    const cheek = Math.exp(-Math.pow((y - 1.698) / 0.026, 4)) * smooth(0.05, 0.07, ax) * (1 - smooth(0.09, 0.11, ax));
    px += side * 0.0025 * cheek;
    if (body) {
      const neck = Math.exp(-Math.pow((y - 1.627) / 0.055, 4)) * (1 - smooth(0.05, 0.084, ax));
      px += x * 0.19 * neck;
      pz += (z + 0.015) * 0.10 * neck;
    }
  } else {
    const x = vx - 0.00048, ax = Math.abs(x);
    const lower = (1 - smooth(1.54, 1.61, y)) * smooth(1.452, 1.48, y);
    px -= x * 0.095 * lower;
    py -= 0.005 * Math.exp(-Math.pow((y - 1.487) / 0.045, 4)) * (1 - smooth(0.025, 0.085, ax));
    for (const s of [-1, 1]) {
      const w = Math.exp(-Math.pow((x - s * 0.041) / 0.039, 6) - Math.pow((y - 1.566) / 0.040, 6)) * smooth(-0.035, -0.064, z);
      px += (x - s * 0.041) * -0.035 * w;
      py += (y - 1.566) * -0.075 * w;
    }
  }
  return [px, py, pz];
}

/** Transport authored shading normals rather than recalculating triangle normals
 * separately for each material primitive (which would introduce visible seams).
 * A 0.1-micron central difference is small relative to the warp's millimeter-scale
 * features, while retaining double precision until the final attribute write.
 */
function fantasyHeadNormal(base: CharacterConfig['base'], x: number, y: number, z: number,
  nx: number, ny: number, nz: number, body: boolean): Point {
  const h = 1e-7;
  const xp = fantasyHeadPoint(base, x + h, y, z, body), xm = fantasyHeadPoint(base, x - h, y, z, body);
  const yp = fantasyHeadPoint(base, x, y + h, z, body), ym = fantasyHeadPoint(base, x, y - h, z, body);
  const zp = fantasyHeadPoint(base, x, y, z + h, body), zm = fantasyHeadPoint(base, x, y, z - h, body);
  const ax = (xp[0] - xm[0]) / (2 * h), ay = (xp[1] - xm[1]) / (2 * h), az = (xp[2] - xm[2]) / (2 * h);
  const bx = (yp[0] - ym[0]) / (2 * h), by = (yp[1] - ym[1]) / (2 * h), bz = (yp[2] - ym[2]) / (2 * h);
  const cx = (zp[0] - zm[0]) / (2 * h), cy = (zp[1] - zm[1]) / (2 * h), cz = (zp[2] - zm[2]) / (2 * h);
  // Cofactor columns / determinant are J^-T. Normalization removes the
  // determinant's magnitude, but its sign must survive a reflected Jacobian.
  const cxx = by * cz - bz * cy, cxy = bz * cx - bx * cz, cxz = bx * cy - by * cx;
  const determinant = ax * cxx + ay * cxy + az * cxz;
  const tx = nx * cxx + ny * (cy * az - cz * ay) + nz * (ay * bz - az * by);
  const ty = nx * cxy + ny * (cz * ax - cx * az) + nz * (az * bx - ax * bz);
  const tz = nx * cxz + ny * (cx * ay - cy * ax) + nz * (ax * by - ay * bx);
  const length = Math.hypot(tx, ty, tz);
  if (!Number.isFinite(length) || length < 1e-12 || !Number.isFinite(determinant) || Math.abs(determinant) < 1e-8) {
    throw new Error(`Invalid fantasy head normal or singular warp at ${x}, ${y}, ${z}`);
  }
  const scale = Math.sign(determinant) / length;
  return [tx * scale, ty * scale, tz * scale];
}

/** Private, assembly-owned geometry. Cached GLTF templates are never mutated. */
export function createFantasyHead(bodyRoot: Object3D, config: CharacterConfig) {
  const geometries: BufferGeometry[] = [];
  if (config.style !== 'fantasy' || config.face !== 'mature') return { dispose: () => {} };
  // Shared accessors remain shared between material primitives, but never across
  // assemblies. Warm rebuilds copy arrays instead of reevaluating the warp.
  const ownedAttributes = new Map<BufferAttribute, BufferAttribute>();
  const own = (attribute: BufferAttribute) => {
    let owned = ownedAttributes.get(attribute);
    if (!owned) { owned = attribute.clone(); ownedAttributes.set(attribute, owned); }
    return owned;
  };
  bodyRoot.traverse(object => {
    if (!(object instanceof Mesh)) return;
    const source = object.geometry as BufferGeometry;
    if (!object.morphTargetDictionary || !source.attributes.position) return;
    const isBody = object.morphTargetDictionary.bodySlim !== undefined;
    const isFace = object.morphTargetDictionary.eyesRound !== undefined;
    if (!isBody && !isFace) return;
    const original = source.attributes.position;
    const relative = source.morphTargetsRelative;
    let cache = templateWarps.get(original);
    if (!cache) { cache = new Map(); templateWarps.set(original, cache); }
    const profile = `mature-v2-${config.base}-${isBody}-${relative}`;
    let data = cache.get(profile);
    if (!data) {
      const values = new Float32Array(original.count * 3);
      for (let i = 0; i < original.count; i++) {
        values.set(fantasyHeadPoint(config.base, original.getX(i), original.getY(i), original.getZ(i), isBody), i * 3);
      }
      const position = new BufferAttribute(values, 3); position.name = original.name;
      data = { position, deltas: new Map(), normals: new Map() };
      cache.set(profile, data);
    }
    const { position, deltas, normals } = data;
    const geometry = new BufferGeometry();
    geometry.name = source.name;
    geometry.index = source.index;
    for (const [name, attribute] of Object.entries(source.attributes)) geometry.setAttribute(name, name === 'position' ? own(position) : attribute);
    geometry.morphTargetsRelative = source.morphTargetsRelative;
    geometry.morphAttributes = { ...source.morphAttributes };
    geometry.morphAttributes.position = (source.morphAttributes.position ?? []).map(attribute => {
      let modified = deltas.get(attribute);
      if (!modified) {
        const values = new Float32Array(original.count * 3);
        for (let i = 0; i < original.count; i++) {
          const endpoint = fantasyHeadPoint(config.base,
            attribute.getX(i) + (relative ? original.getX(i) : 0),
            attribute.getY(i) + (relative ? original.getY(i) : 0),
            attribute.getZ(i) + (relative ? original.getZ(i) : 0), isBody);
          values[i * 3] = endpoint[0] - (relative ? position.getX(i) : 0);
          values[i * 3 + 1] = endpoint[1] - (relative ? position.getY(i) : 0);
          values[i * 3 + 2] = endpoint[2] - (relative ? position.getZ(i) : 0);
        }
        modified = new BufferAttribute(values, 3); modified.name = attribute.name;
        deltas.set(attribute, modified);
      }
      return own(modified);
    });
    const originalNormal = source.attributes.normal;
    if (originalNormal) {
      let normalData = normals.get(originalNormal);
      if (!normalData) {
        const values = new Float32Array(original.count * 3);
        for (let i = 0; i < original.count; i++) {
          values.set(fantasyHeadNormal(config.base, original.getX(i), original.getY(i), original.getZ(i),
            originalNormal.getX(i), originalNormal.getY(i), originalNormal.getZ(i), isBody), i * 3);
        }
        const normal = new BufferAttribute(values, 3); normal.name = originalNormal.name;
        normalData = { normal, endpoints: new Map() };
        normals.set(originalNormal, normalData);
      }
      const { normal, endpoints } = normalData;
      geometry.setAttribute('normal', own(normal));
      if (source.morphAttributes.normal) {
        geometry.morphAttributes.normal = source.morphAttributes.normal.map((attribute, target) => {
          const targetPosition = source.morphAttributes.position?.[target];
          let byPosition = endpoints.get(attribute);
          if (!byPosition) { byPosition = new Map(); endpoints.set(attribute, byPosition); }
          let modified = byPosition.get(targetPosition);
          if (!modified) {
            const values = new Float32Array(original.count * 3);
            for (let i = 0; i < original.count; i++) {
              const point: Point = targetPosition
                ? [targetPosition.getX(i) + (relative ? original.getX(i) : 0),
                  targetPosition.getY(i) + (relative ? original.getY(i) : 0),
                  targetPosition.getZ(i) + (relative ? original.getZ(i) : 0)]
                : [original.getX(i), original.getY(i), original.getZ(i)];
              const endpoint = fantasyHeadNormal(config.base, ...point,
                attribute.getX(i) + (relative ? originalNormal.getX(i) : 0),
                attribute.getY(i) + (relative ? originalNormal.getY(i) : 0),
                attribute.getZ(i) + (relative ? originalNormal.getZ(i) : 0), isBody);
              values[i * 3] = endpoint[0] - (relative ? normal.getX(i) : 0);
              values[i * 3 + 1] = endpoint[1] - (relative ? normal.getY(i) : 0);
              values[i * 3 + 2] = endpoint[2] - (relative ? normal.getZ(i) : 0);
            }
            modified = new BufferAttribute(values, 3); modified.name = attribute.name;
            byPosition.set(targetPosition, modified);
          }
          return own(modified);
        });
      }
    }
    geometry.groups = source.groups.map(group => ({ ...group }));
    geometry.setDrawRange(source.drawRange.start, source.drawRange.count);
    geometry.userData = { ...source.userData, fantasyHeadProfile: `mature-v2-${config.base}` };
    geometry.computeBoundingBox(); geometry.computeBoundingSphere();
    geometries.push(geometry); object.geometry = geometry;
  });
  return { dispose: () => geometries.forEach(geometry => geometry.dispose()) };
}
