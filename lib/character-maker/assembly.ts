import { AnimationClip, Box3, Group, Material, Mesh, MeshBasicMaterial, Texture, TextureLoader } from 'three';
import { GLTFLoader, type GLTF } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { GLTFExporter } from 'three/examples/jsm/exporters/GLTFExporter.js';
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js';
import { currentCharacterConfig, type CharacterConfig } from './config';
import { createMakerSword, addWeaponTracks, fitMakerWeaponScale, disposeMakerWeapon } from './weapon';
import { selectedMakerWeaponId } from './weapon-catalog';
import { loadMakerWeapon } from './registered-weapon';
import { createMakerExpressions } from './expressions';
import { createFantasyHead } from './fantasy-head';
import { createPresetHurtFace, presetHurtClips, withHurtMorphTracks } from './preset-hurt-face';
import { applyOutfitPaletteMap, loadOutfitPalette } from './outfit-palettes';

import { makerAssetUrl as assetUrl } from './asset-url';
import { HURT_FACE_NODE } from './hurt';
// Templates are immutable. Instances own their skeletons and materials; geometry/maps
// are shared only because appearance editing changes morph weights and material colour.
const templates = new Map<string, Promise<GLTF>>();
const motions = new Map<string, Promise<AnimationClip[]>>();
const matureSkinTemplates = new Map<string, Promise<Texture>>();

function loadMatureSkin(base: CharacterConfig['base']): Promise<Texture> {
  let pending = matureSkinTemplates.get(base);
  if (!pending) {
    pending = new TextureLoader().loadAsync(assetUrl(`fantasy-${base}-mature-skin.png`)).catch(error => {
      matureSkinTemplates.delete(base);
      throw error;
    });
    matureSkinTemplates.set(base, pending);
  }
  return pending;
}

const matureEyeTemplates = new Map<string, Promise<Texture>>();
function loadMatureEyes(): Promise<{ eyelash: Texture; eyeline: Texture }> {
  const load = (part: 'eyelash' | 'eyeline') => {
    let pending = matureEyeTemplates.get(part);
    if (!pending) {
      pending = new TextureLoader().loadAsync(assetUrl(`fantasy-masculine-mature-${part}.png`)).catch(error => {
        matureEyeTemplates.delete(part); throw error;
      });
      matureEyeTemplates.set(part, pending);
    }
    return pending;
  };
  return Promise.all([load('eyelash'), load('eyeline')]).then(([eyelash, eyeline]) => ({ eyelash, eyeline }));
}

function loadTemplate(file: string): Promise<GLTF> {
  let pending = templates.get(file);
  if (!pending) {
    pending = new GLTFLoader().loadAsync(assetUrl(file)).catch(error => {
      templates.delete(file);
      throw error;
    });
    templates.set(file, pending);
  }
  return pending;
}

async function loadMotions(config: CharacterConfig): Promise<AnimationClip[]> {
  const key = `${config.style}:${config.base}:v2`;
  let pending = motions.get(key);
  if (!pending) {
    // のけぞり（hit）は既存の動作 JSON を変えずに、別のファイルから足す（docs/main-character-spec.md）。
    const files = config.style === 'fantasy'
      ? [`fantasy-${config.base}-motions.json`, `fantasy-${config.base}-hit-motions.json`]
      : [`${config.base}-motions.json`, `comic-${config.base}-sword-motions.json`, `comic-${config.base}-hit-motions.json`];
    pending = Promise.all(files.map(file => fetch(assetUrl(file)).then(async response => {
      if (!response.ok) throw new Error('動作データを読み込めませんでした。再読み込みしてください。');
      const data = await response.json();
      return (Array.isArray(data) ? data : data.clips).map((clip: Parameters<typeof AnimationClip.parse>[0]) => AnimationClip.parse(clip));
    }))).then(clips => clips.flat()).catch(error => { motions.delete(key); throw error; });
    motions.set(key, pending);
  }
  return pending;
}

export type MakerInstance = {
  root: Group;
  animations: AnimationClip[];
  restBounds: { height: number; top: number };
  apply: (config: CharacterConfig) => void;
  applyEyeGaze: () => void;
  /** ＞＜ の顔を出す・消す（保存しない一時の表示）。のけぞり中も使う */
  setHurtFace: (on: boolean) => void;
  /** 書き出し用の動作。ファンタジーは hit・face_hurt に顔のモーフのトラックを足す */
  exportAnimations: () => AnimationClip[];
  dispose: () => void;
};

function ownedUnlitMaterial(source: Material): MeshBasicMaterial {
  const original = source as MeshBasicMaterial;
  const material = new MeshBasicMaterial({
    color: original.color ?? 0xffffff,
    map: original.map ?? null,
    side: source.side,
    transparent: source.transparent,
    opacity: source.opacity,
    alphaTest: source.alphaTest,
    depthWrite: source.depthWrite,
    vertexColors: source.vertexColors,
    toneMapped: false,
  });
  material.name = source.name;
  return material;
}

/** Build the model only, with no camera, stage, light or viewer state. */
export async function assembleCharacter(config: CharacterConfig): Promise<MakerInstance> {
  config = currentCharacterConfig(config);
  const fantasy = config.style === 'fantasy';
  const weaponId = selectedMakerWeaponId(config);
  const [body, hair, animationClips, matureSkin, outfitPalette, matureEyes, weaponSource] = await Promise.all([
    loadTemplate(`${fantasy ? 'fantasy-' : ''}${config.base}.glb`),
    config.hair === 'none' ? Promise.resolve(null) : loadTemplate(fantasy
      ? `fantasy-hair-${config.base}-${config.hair}.glb` : `hair-${config.hair}.glb`),
    loadMotions(config),
    fantasy ? loadMatureSkin(config.base) : Promise.resolve(null),
    loadOutfitPalette(config),
    fantasy && config.base === 'masculine' ? loadMatureEyes() : Promise.resolve(null),
    loadMakerWeapon(weaponId),
  ]);
  const root = new Group();
  root.name = 'CharacterMaker';
  const bodyInstance = clone(body.scene);
  const headGeometry = createFantasyHead(bodyInstance, config);
  const head = bodyInstance.getObjectByName(fantasy ? 'J_Bip_C_Head' : 'Head');
  const originalHeadScale = head?.scale.clone();
  root.add(bodyInstance);
  if (hair) {
    if (!head) throw new Error('モデルの頭の接続点が見つかりません。');
    const hairInstance = clone(hair.scene);
    hairInstance.name = 'HairStyle';
    head.add(hairInstance);
  }

  const ownedMaterials: MeshBasicMaterial[] = [];
  const matureMaps = new Map<Texture, Texture>();
  const outfitMaps: Texture[] = [];
  const faceMaps: { material: MeshBasicMaterial; original: Texture; mature: Texture }[] = [];
  const meshes: Mesh[] = [];
  root.traverse(object => {
    if (!(object instanceof Mesh)) return;
    meshes.push(object);
    // Target extremes remain visible when the editor modifies the silhouette.
    object.frustumCulled = false;
    const sources = Array.isArray(object.material) ? object.material : [object.material];
    const materials = sources.map(source => {
      const material = ownedUnlitMaterial(source);
      const outfitMap = applyOutfitPaletteMap(material, outfitPalette);
      if (outfitMap) outfitMaps.push(outfitMap);
      const matureSource = material.name.endsWith('Face_00_SKIN') ? matureSkin
        : material.name.endsWith('FaceEyelash_00_FACE') ? matureEyes?.eyelash
        : material.name.endsWith('FaceEyeline_00_FACE') ? matureEyes?.eyeline : null;
      if (matureSource && material.map) {
        const original = material.map;
        let map = matureMaps.get(original);
        if (!map) {
          // A fresh Texture owns its Source; Texture.clone() would temporarily
          // share and mark the cached Source for upload during Texture.copy().
          map = new Texture(matureSource.image);
          map.mapping = original.mapping; map.channel = original.channel;
          map.wrapS = original.wrapS; map.wrapT = original.wrapT;
          map.magFilter = original.magFilter; map.minFilter = original.minFilter; map.anisotropy = original.anisotropy;
          map.format = original.format; map.type = original.type; map.colorSpace = original.colorSpace;
          map.offset.copy(original.offset); map.repeat.copy(original.repeat); map.center.copy(original.center);
          map.rotation = original.rotation; map.matrixAutoUpdate = original.matrixAutoUpdate; map.matrix.copy(original.matrix);
          map.generateMipmaps = original.generateMipmaps; map.premultiplyAlpha = original.premultiplyAlpha;
          map.flipY = original.flipY; map.unpackAlignment = original.unpackAlignment; map.internalFormat = original.internalFormat;
          map.needsUpdate = true;
          matureMaps.set(original, map);
        }
        faceMaps.push({ material, original, mature: map });
      }
      ownedMaterials.push(material);
      return material;
    });
    object.material = Array.isArray(object.material) ? materials : materials[0];
  });
  // The sword belongs to this assembled instance, so preview and export have
  // the same grip, geometry and show/hide animation. It is never a stage prop.
  const sword = createMakerSword(config.style, config.base, weaponId, weaponSource);
  const hand = bodyInstance.getObjectByName(sword.userData.attachmentBone);
  if (!hand) throw new Error('武器を持つ手の接続点が見つかりません。');
  hand.add(sword);
  fitMakerWeaponScale(sword, hand, bodyInstance);

  const expression = fantasy ? createMakerExpressions(body, bodyInstance) : null;
  if (!head) throw new Error('モデルの頭の接続点が見つかりません。');
  const hurtFace = createPresetHurtFace(config.style, bodyInstance, head);
  let hurtOn = false, applied = config;

  const apply = (appearance: CharacterConfig) => {
    appearance = currentCharacterConfig(appearance);
    root.scale.setScalar(appearance.height);
    const mature = fantasy && appearance.face === 'mature';
    // Scale this instance's head bone so its skinned face, scalp and child hair
    // stay together. Every other face restores the complete original scale.
    if (head && originalHeadScale) {
      head.scale.copy(originalHeadScale);
      if (mature) {
        if (appearance.base === 'masculine') head.scale.multiply({ x: 1.035, y: 1, z: 1.025 });
        else head.scale.multiply({ x: 0.94, y: 1.01, z: 0.98 });
      }
    }
    faceMaps.forEach(({ material, original, mature: map }) => { material.map = mature ? map : original; });
    const values: Record<string, number> = {
      hairFitMasculine: appearance.base === 'masculine' ? 1 : 0,
      hairFitFeminine: appearance.base === 'feminine' ? 1 : 0,
      bodySlim: Math.max(0, -appearance.build),
      bodyBroad: Math.max(0, appearance.build),
      faceRound: appearance.face === 'round' ? 1 : 0,
      faceAngular: appearance.face === 'angular' ? 1 : 0,
      eyesRound: appearance.eyes === 'round' ? 1 : 0,
      eyesSlender: appearance.eyes === 'slender' ? 1 : 0,
      mouthSmile: appearance.mouth === 'smile' ? 1 : 0,
      mouthFull: appearance.mouth === 'full' ? 1 : 0,
    };
    meshes.forEach(mesh => {
      for (const [name, value] of Object.entries(values)) {
        const index = mesh.morphTargetDictionary?.[name];
        if (index !== undefined && mesh.morphTargetInfluences) mesh.morphTargetInfluences[index] = value;
      }
    });
    expression?.apply(appearance, hurtOn);
    hurtFace.update();
    applied = appearance;
    ownedMaterials.forEach(material => {
      if (material.name === 'maker-hair' || material.name === 'maker-brow') {
        material.color.set(appearance.hairColor);
        if (mature && appearance.base === 'masculine' && material.name === 'maker-brow') material.color.multiplyScalar(0.62);
      }
    });
    root.userData.characterMaker = {
      assetVersion: fantasy ? 'fantasy-maker-v1' : 'human-maker-v1', config: { ...appearance },
      playback: { sword_attack: { loop: false, next: 'sword_idle' }, hit: { loop: false }, face_hurt: { loop: true } },
      hurtFace: { node: HURT_FACE_NODE, method: fantasy ? 'morph' : 'overlay' },
      ...(expression ? { expressionMorphs: expression.profile } : {}),
    };
    root.updateMatrixWorld(true);
  };
  apply(config);
  const bounds = new Box3().setFromObject(root);
  const animations = addWeaponTracks(presetHurtClips(animationClips), weaponId !== 'none');
  const setHurtFace = (on: boolean) => {
    hurtFace.node.scale.setScalar(on ? 1 : 0);
    if (on === hurtOn) return;
    hurtOn = on;
    expression?.apply(applied, on);
  };
  return {
    root,
    restBounds: { height: (bounds.max.y - bounds.min.y) / config.height, top: bounds.max.y / config.height },
    animations,
    apply,
    applyEyeGaze: () => expression?.applyEyeGaze(),
    setHurtFace,
    exportAnimations: () => {
      if (!expression) return animations;
      const was = hurtOn, read = () => expression.meshes.map(mesh => [...(mesh.morphTargetInfluences ?? [])]);
      setHurtFace(false); const normal = read();
      setHurtFace(true); const hurt = read();
      setHurtFace(was);
      return withHurtMorphTracks(animations, expression.meshes, normal, hurt);
    },
    dispose: () => {
      hurtFace.dispose();
      expression?.dispose();
      headGeometry.dispose();
      ownedMaterials.forEach(material => material.dispose());
      matureMaps.forEach(map => map.dispose());
      outfitMaps.forEach(map => map.dispose());
      disposeMakerWeapon(sword);
      // SkeletonUtils allocates an instance skeleton; cached vertex/maps stay alive.
      const skeletons = new Set<{ dispose: () => void }>();
      root.traverse(object => {
        if ('isSkinnedMesh' in object && object.isSkinnedMesh && 'skeleton' in object) skeletons.add(object.skeleton as { dispose: () => void });
      });
      skeletons.forEach(skeleton => skeleton.dispose());
      root.removeFromParent();
    },
  };
}

export async function exportCharacterGlb(config: CharacterConfig): Promise<ArrayBuffer> {
  const instance = await assembleCharacter({ ...config });
  try {
    const result = await new GLTFExporter().parseAsync(instance.root, {
      binary: true,
      trs: true,
      animations: instance.exportAnimations(),
      maxTextureSize: 1024,
      onlyVisible: true,
    });
    if (!(result instanceof ArrayBuffer)) throw new Error('GLBの書き出しに失敗しました。');
    return result;
  } finally {
    instance.dispose();
  }
}
