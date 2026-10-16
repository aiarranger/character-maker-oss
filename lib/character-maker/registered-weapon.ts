import { Group, Mesh, MeshBasicMaterial, TextureLoader, type Object3D, type MeshStandardMaterial } from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js';
import savedWeapons from '../../public/review-swing-2026-10-02/weapons.json';
import { makerWeaponFit, type MakerWeaponRig } from './weapon-fit';
import { validateMakerWeaponId, type MakerWeaponId } from './weapon-catalog';

const assets = new Map(savedWeapons.weapons.map(weapon => [weapon.id, weapon]));
type Resource = { promise: Promise<Object3D>; value?: Object3D; error?: unknown };
const templates = new Map<MakerWeaponId, Resource>();

export function registeredMakerWeaponMetadata(id: MakerWeaponId, rig?: MakerWeaponRig) {
  if (id === 'none') return { included: false, id };
  const weapon = assets.get(id);
  if (!weapon) throw new Error('登録された武器が見つかりません。');
  return { included: true, id, name: weapon.name, source: savedWeapons.source, sha256: weapon.sha256.model,
    length: weapon.length, grip: weapon.grip, modelConvention: savedWeapons.modelConvention,
    ...(rig ? { fit: { ...makerWeaponFit(id, rig), nominalLength: weapon.length, calibratedLengthAtDefaultSize: weapon.length * makerWeaponFit(id, rig).effectiveScale } } : {}) };
}

async function loadRegisteredWeapon(id: MakerWeaponId): Promise<Object3D> {
  const weapon = assets.get(id);
  if (!weapon) throw new Error('登録された武器が見つかりません。');
  const url = `/review-swing-2026-10-02/${weapon.model}`;
  const response = await fetch(url);
  if (!response.ok) throw new Error(`「${weapon.name}」を読み込めませんでした。もう一度読み込んでください。`);
  const bytes = await response.arrayBuffer();
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  const sha256 = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
  if (sha256 !== weapon.sha256.model) throw new Error(`「${weapon.name}」のモデルが登録された内容と一致しません。`);
  const loader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder);
  // HTML texture images are compatible with GLTFExporter, unlike ImageBitmap in
  // some browsers. This changes neither the source texture nor its sampler.
  loader.register(parser => ({ name: 'MAKER_WEAPON_HTML_TEXTURES', beforeRoot: () => {
    parser.textureLoader = new TextureLoader(parser.options.manager).setCrossOrigin(parser.options.crossOrigin).setRequestHeader(parser.options.requestHeader);
    return null;
  } }));
  const gltf = await loader.parseAsync(bytes, new URL('.', new URL(url, window.location.href)).href);
  const oldMaterials = new Set<MeshStandardMaterial>();
  gltf.scene.traverse(object => {
    if (!(object instanceof Mesh)) return;
    const convert = (source: MeshStandardMaterial) => {
      oldMaterials.add(source);
      const material = new MeshBasicMaterial({ map: source.map, color: source.color, side: source.side, vertexColors: source.vertexColors,
        transparent: source.transparent, opacity: source.opacity, alphaTest: source.alphaTest, depthWrite: source.depthWrite, toneMapped: false });
      material.name = source.name; return material;
    };
    object.material = Array.isArray(object.material) ? object.material.map(m => convert(m as MeshStandardMaterial)) : convert(object.material as MeshStandardMaterial);
    object.frustumCulled = false;
  });
  oldMaterials.forEach(material => material.dispose());
  // Exact main-list mounting convention: grip adjustment, then registered
  // physical length. Do not reshape, recenter, or infer a replacement pivot.
  const adjustment = new Group(); adjustment.name = `RegisteredWeapon_${id}`;
  adjustment.position.fromArray(weapon.grip.pos);
  adjustment.rotation.set(weapon.grip.rot[0], weapon.grip.rot[1], weapon.grip.rot[2]);
  gltf.scene.scale.multiplyScalar(weapon.length * weapon.grip.scale);
  adjustment.add(gltf.scene);
  adjustment.userData.registeredWeapon = registeredMakerWeaponMetadata(id);
  return adjustment;
}

export function loadMakerWeapon(id: MakerWeaponId): Promise<Object3D | null> {
  id = validateMakerWeaponId(id);
  if (id === 'none') return Promise.resolve(null);
  let resource = templates.get(id);
  if (!resource) {
    resource = { promise: null as unknown as Promise<Object3D> };
    const current = resource;
    resource.promise = loadRegisteredWeapon(id).then(value => { current.value = value; return value; }, error => { current.error = error; throw error; });
    templates.set(id, resource);
  }
  return resource.promise;
}

/** Suspense boundary and async exports share the same verified template. */
export function readMakerWeapon(id: MakerWeaponId): Object3D | null {
  if (id === 'none') return null;
  const pending = loadMakerWeapon(id), resource = templates.get(id)!;
  if (resource.error) throw resource.error;
  if (resource.value) return resource.value;
  throw pending;
}

/** Retry only a failed request; live cached templates remain immutable. */
export function retryMakerWeapon(id: MakerWeaponId) {
  if (templates.get(id)?.error) templates.delete(id);
}
