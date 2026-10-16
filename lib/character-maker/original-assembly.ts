import { AnimationClip, Group, InterpolateDiscrete, Material, Mesh, NumberKeyframeTrack, PropertyBinding, Quaternion, Texture, TextureLoader, Vector3, VectorKeyframeTrack, type KeyframeTrack, type Object3D } from 'three';
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js';
import { GLTFExporter } from 'three/examples/jsm/exporters/GLTFExporter.js';
import { MeshoptDecoder } from 'three/examples/jsm/libs/meshopt_decoder.module.js';
import { clone } from 'three/examples/jsm/utils/SkeletonUtils.js';
import { blinkCorrectiveWeight } from '../../components/rig-actor/generic/blink';
import genericViewerMaterial from '../viewer-material/generic.mjs';
import { originalHurtFaceStyle, currentOriginalAppearance, defaultOriginalAppearance, originalModelAsset, usesModularOriginal, resolveOriginalCharacter, validateOriginalDocument, type OriginalAppearance, type OriginalCharacter, type OriginalConfig, type OriginalDocument, type OriginalFaceKey, type OriginalMotion } from './original-config';
import { createOriginalBody } from './original-body';
import { configureOriginalSpecies, defaultOriginalSpeciesParts } from './original-species';
import { configureOriginalHumanParts } from './original-human-parts';
import { configureOriginalHair } from './original-hair';
import { selectedMakerWeaponId } from './weapon-catalog';
import { loadMakerWeapon } from './registered-weapon';
import { attachOriginalSword, originalClipsWithSword, originalHitClip, originalSwordMetadata, ORIGINAL_VISIBILITY_TRACKS } from './original-weapon';
import { createHurtFace, worldForward } from './hurt-face';
import { HIT_CLIP, HURT_FACE_CLIP, HURT_FACE_NODE } from './hurt';

export type OriginalGlbVariant = 'standard' | 'armed';
// The hand/sword switches are preview-only: an armed GLB shows its sword and closed hand all the time
// (also through 'hit'), and a standard GLB has neither. The ＞＜ node's scale track is kept.
const ORIGINAL_GLB_VISIBILITY_TRACKS = ORIGINAL_VISIBILITY_TRACKS;
/** ＞＜ を出し続けるだけのクリップの長さ（秒）。ループ再生か、他のクリップと重ねて使う */
const HURT_FACE_CLIP_DURATION = 1;

function clipHasFamily(name: string, family: string) {
  return name === family || name.endsWith(`:${family}`);
}

/** ＞＜ のノードの scale を、clip の間ずっと 1（hit は最後のキーで 0 に戻す。STEP で切り替える） */
function hurtFaceTrack(duration: number, hideAtEnd: boolean) {
  return new VectorKeyframeTrack(`${HURT_FACE_NODE}.scale`, [0, duration], [1, 1, 1, ...(hideAtEnd ? [0, 0, 0] : [1, 1, 1])], InterpolateDiscrete);
}

type HurtTracks = (duration: number, reopen: boolean) => KeyframeTrack[];
/** ＞＜ の板の下で目を閉じるモーフのトラック（目のメッシュごとに全チャンネル。目以外のチャンネルは書き出す静止の値のまま）。
 * 開いた目の白目・まつ毛が板より手前に出ないよう、プレビューと同じく、まばたきで閉じる。applyStaticFace の後に作る */
function hurtEyeTracks(model: Object3D): HurtTracks {
  const meshes: { target: string; open: number[]; closed: number[] }[] = [];
  model.traverse(object => {
    if (!(object instanceof Mesh) || !object.visible || !object.morphTargetDictionary || !object.morphTargetInfluences) return;
    const entries = Object.entries(object.morphTargetDictionary);
    if (!entries.some(([name]) => name.startsWith('blink'))) return;
    const open = [...object.morphTargetInfluences], closed = [...open];
    for (const [name, index] of entries) {
      if (!name.startsWith('blink')) continue;
      const corrective = blinkCorrectiveWeight(name, 1);
      closed[index] = corrective !== null ? corrective : name.endsWith('Arc') ? 0 : 1;
    }
    // The uuid names exactly this mesh even when node names repeat; GLTFExporter resolves it to the node.
    meshes.push({ target: `${object.uuid}.morphTargetInfluences`, open, closed });
  });
  return (duration, reopen) => meshes.map(mesh => new NumberKeyframeTrack(mesh.target, [0, duration], [...mesh.closed, ...(reopen ? mesh.open : mesh.closed)], InterpolateDiscrete));
}

function selectOriginalGlbClips(character: OriginalCharacter, source: readonly AnimationClip[], variant: OriginalGlbVariant, eyes: HurtTracks) {
  const possibleFamilies = variant === 'armed' ? ['sword_idle', 'sword_attack'] : ['idle', 'walk', 'run'];
  const families = possibleFamilies.filter(family => character.motions.includes(family as OriginalMotion));
  // Both variants append the hit (body) after their motions; existing clips are selected exactly as before.
  const candidates = variant === 'armed' ? originalClipsWithSword(character.id, source) : [...source.filter(clip => clip.name !== HIT_CLIP), originalHitClip(character.id)];
  const selected = families.flatMap(family => candidates.filter(clip => clipHasFamily(clip.name, family)));
  const missingOrDuplicate = [...families, HIT_CLIP].filter(family => candidates.filter(clip => clipHasFamily(clip.name, family)).length !== 1);
  if (!families.length || missingOrDuplicate.length) {
    throw new Error(`${variant === 'armed' ? '武器用' : '通常用'}GLBに必要な動作が揃っていません。`);
  }
  const hit = candidates.find(clip => clip.name === HIT_CLIP)!;
  const clips = [...selected, hit].map(sourceClip => {
    const clip = sourceClip.clone();
    clip.tracks = clip.tracks.filter(track => !ORIGINAL_GLB_VISIBILITY_TRACKS.has(track.name));
    if (clip.name === HIT_CLIP) clip.tracks.push(hurtFaceTrack(clip.duration, true), ...eyes(clip.duration, true));
    return clip;
  });
  clips.push(new AnimationClip(HURT_FACE_CLIP, HURT_FACE_CLIP_DURATION, [hurtFaceTrack(HURT_FACE_CLIP_DURATION, false), ...eyes(HURT_FACE_CLIP_DURATION, false)]));
  if (character.id === 'generic-human-b') {
    // 武器用は構える・振る・hit・face_hurt の4本、通常用は待機・歩く・走る・hit・face_hurt の5本
    const expectedCount = variant === 'armed' ? 4 : 5;
    if (families.length + 2 !== expectedCount || clips.length !== expectedCount) {
      throw new Error(`ナミの${variant === 'armed' ? '武器用' : '通常用'}GLBに必要な動作が揃っていません。`);
    }
  }
  return clips;
}

function assertAnimationTargetsResolve(root: Object3D, animations: readonly AnimationClip[]) {
  for (const clip of animations) for (const track of clip.tracks) {
    const parsed = PropertyBinding.parseTrackName(track.name);
    if (!PropertyBinding.findNode(root, parsed.nodeName)) {
      throw new Error(`GLBの動作「${clip.name}」が対象ノード「${parsed.nodeName}」を見つけられません。`);
    }
  }
}

/** The frozen generic viewer and downloaded GLBs use the same calibration. */
export function resolveOriginalProfile(character: OriginalCharacter, appearance?: OriginalAppearance) {
  const modularHuman = usesModularOriginal(currentOriginalAppearance(character.id, appearance ?? defaultOriginalAppearance()));
  return {
    id: character.id,
    latest: null,
    viewerModel: { publicGlb: originalModelAsset(character, appearance).url },
    rotationY: -Math.PI / 2,
    viewer: {
      // An omitted bodyHz also selects the generic viewer's per-frame face
      // updates, so retain omission rather than resolving it to the same 120 Hz.
      bodyHz: character.viewer.bodyHz,
      modelScale: character.viewer.modelScale ?? 2,
      modelOffsetY: character.viewer.modelOffsetY ?? 0,
      fullSpan: character.viewer.fullSpan ?? 2.35,
      faceHeight: character.id === 'generic-robot' ? 1.41
        : modularHuman && character.id === 'generic-human-b' ? 1.74 : character.viewer.faceHeight ?? 1.48,
      faceSpan: Math.max(character.viewer.faceSpan ?? 1.35,
        character.id === 'generic-robot' ? 1.35 : character.id === 'generic-cat' ? 1.15 : modularHuman ? character.id === 'generic-human-b' ? .76 : .90 : 0),
      headTurnDegrees: character.viewer.headTurnDegrees ?? 25,
      neutralMouthClose: character.viewer.neutralMouthClose ?? 1,
    },
  };
}

function applyStaticFace(model: Object3D, face: OriginalConfig['face']) {
  model.traverse(object => {
    if (!(object instanceof Mesh) || !object.morphTargetDictionary || !object.morphTargetInfluences) return;
    for (const [name, index] of Object.entries(object.morphTargetDictionary)) {
      const mouthCorrective = /^mouthCloseInbetween(25|50|75)$/.exec(name);
      if (mouthCorrective) {
        object.morphTargetInfluences[index] = Math.max(0, 1 - Math.abs(face.mouthClose - Number(mouthCorrective[1]) / 100) / .25);
        continue;
      }
      const blink = name.startsWith('blinkLeft') ? face.blinkLeft : face.blinkRight;
      const corrective = blinkCorrectiveWeight(name, blink);
      if (corrective !== null) { object.morphTargetInfluences[index] = corrective; continue; }
      const key = (name.endsWith('Arc') ? name.slice(0, -3) : name) as OriginalFaceKey;
      if (!(key in face)) continue;
      // Every owned channel is written, including the saved closed-eye defaults
      // in Lux/Moka. Live blinking and speaking never enter this static export.
      object.morphTargetInfluences[index] = name.endsWith('Arc') ? Math.sin(Math.PI * face[key]) : face[key];
    }
  });
}

function applyStaticHeadTurn(model: Object3D, degrees: number) {
  const head = model.getObjectByName('Head') ?? model.getObjectByName('J_Bip_C_Head');
  if (!head || degrees === 0) return;
  model.updateWorldMatrix(true, true);
  const modelRotation = model.getWorldQuaternion(new Quaternion());
  const parentRotation = head.parent?.getWorldQuaternion(new Quaternion()) ?? new Quaternion();
  const axis = new Vector3(0, 1, 0).applyQuaternion(modelRotation).applyQuaternion(parentRotation.invert());
  head.quaternion.premultiply(new Quaternion().setFromAxisAngle(axis, degrees * Math.PI / 180));
}

function disposeOwnedScenes(materials: Set<Material>, ...scenes: Object3D[]) {
  const geometries = new Set<Mesh['geometry']>();
  const textures = new Set<Texture>();
  const skeletons = new Set<{ dispose: () => void }>();
  for (const scene of scenes) scene.traverse(object => {
    if (!(object instanceof Mesh)) return;
    geometries.add(object.geometry);
    for (const material of Array.isArray(object.material) ? object.material : [object.material]) {
      materials.add(material);
      for (const value of Object.values(material)) if (value instanceof Texture) textures.add(value);
    }
    if ('isSkinnedMesh' in object && object.isSkinnedMesh && 'skeleton' in object) skeletons.add(object.skeleton as { dispose: () => void });
  });
  skeletons.forEach(skeleton => skeleton.dispose());
  geometries.forEach(geometry => geometry.dispose());
  materials.forEach(material => material.dispose());
  textures.forEach(texture => texture.dispose());
}

/** Export a fresh source instance; never sample or mutate the live preview. */
export async function exportOriginalCharacterGlb(character: OriginalCharacter, document: OriginalDocument): Promise<ArrayBuffer> {
  document = validateOriginalDocument(document);
  resolveOriginalCharacter(document, [character]);
  const weaponId = selectedMakerWeaponId(document.config);
  const variant: OriginalGlbVariant = ['sword_idle', 'sword_attack'].includes(document.config.motion) ? 'armed' : 'standard';
  const asset = originalModelAsset(character, document.config.appearance);
  const sourceUrl = asset.url;
  const sourceSha = asset.sha256;
  const response = await fetch(sourceUrl);
  if (!response.ok) throw new Error('キャラクターのモデルを読み込めませんでした。');
  const bytes = await response.arrayBuffer();
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  const sha256 = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
  if (sha256 !== sourceSha) throw new Error('キャラクターのモデルが保存時の内容と一致しません。');

  const loader = new GLTFLoader().setMeshoptDecoder(MeshoptDecoder);
  // Match the frozen viewer's HTML images, which GLTFExporter can draw to canvas.
  loader.register(parser => ({ name: 'ORIGINAL_MAKER_HTML_TEXTURES', beforeRoot: () => {
    parser.textureLoader = new TextureLoader(parser.options.manager).setCrossOrigin(parser.options.crossOrigin).setRequestHeader(parser.options.requestHeader);
    return null;
  } }));
  const baseUrl = new URL('.', new URL(sourceUrl, window.location.href)).href;
  const gltf = await loader.parseAsync(bytes, baseUrl);
  const model = clone(gltf.scene);
  const geometryCopies = new Map<Mesh['geometry'], Mesh['geometry']>();
  model.traverse(object => {
    if (!(object instanceof Mesh)) return;
    const sourceGeometry = object.geometry;
    if (!geometryCopies.has(sourceGeometry)) geometryCopies.set(sourceGeometry, sourceGeometry.clone());
    object.geometry = geometryCopies.get(sourceGeometry)!;
  });
  const root = new Group();
  root.name = 'CharacterMakerOriginal';
  const profile = resolveOriginalProfile(character, document.config.appearance);
  const calibration = new Group();
  calibration.name = 'CharacterMakerOriginalCalibration';
  calibration.position.y = profile.viewer.modelOffsetY;
  calibration.rotation.y = profile.rotationY;
  calibration.scale.setScalar(profile.viewer.modelScale);
  calibration.add(model);
  root.add(calibration);
  root.scale.setScalar(document.config.size);
  let body: ReturnType<typeof createOriginalBody> | undefined;
  let species: ReturnType<typeof configureOriginalSpecies> | undefined;
  let humanParts: ReturnType<typeof configureOriginalHumanParts> | undefined;
  let hair: ReturnType<typeof configureOriginalHair> | undefined;
  let detachedNativeHand: Object3D | undefined;
  const ownedMaterials = new Set<Material>();
  try {
    model.traverse(object => {
      if (!(object instanceof Mesh)) return;
      const originals = Array.isArray(object.material) ? object.material : [object.material];
      const materials = originals.map(material => {
        const result = genericViewerMaterial(material);
        result.name = material.name;
        result.userData = structuredClone(material.userData);
        ownedMaterials.add(result);
        return result;
      });
      object.material = Array.isArray(object.material) ? materials : materials[0];
    });
    species = configureOriginalSpecies(model, character.id, document.config.appearance.parts);
    humanParts = configureOriginalHumanParts(model, character.id, document.config.appearance.parts ?? defaultOriginalSpeciesParts());
    body = createOriginalBody(model, character.id);
    body.apply(document.config.appearance.build);
    hair = configureOriginalHair(model, character.id, document.config.appearance);
    if (variant === 'armed') {
      attachOriginalSword(model, character.id, weaponId, await loadMakerWeapon(weaponId), profile.viewer.modelScale);
      if (character.id === 'generic-human-a' || character.id === 'generic-human-b') {
        const nativeHand = model.getObjectByName('OriginalNativeRightHand');
        const closedHand = model.getObjectByName('OriginalSwordRightHand');
        if (!nativeHand || !closedHand || !nativeHand.parent) throw new Error('武器用GLBの手のパーツを確認できません。');
        nativeHand.parent.remove(nativeHand);
        detachedNativeHand = nativeHand;
        closedHand.scale.setScalar(1);
      }
      const sword = model.getObjectByName('CharacterMakerSword');
      if (!sword) throw new Error('武器用GLBの武器を確認できません。');
      sword.scale.setScalar(1);
    }
    // ＞＜ の板（scale 0 のノード。hit・face_hurt の scale トラックで 1 になる）。顔の正面は模型の +X
    const hurtStyle = originalHurtFaceStyle(character.id, document.config.appearance.parts);
    const hurt = hurtStyle ? createHurtFace(model, hurtStyle, worldForward(model, new Vector3(1, 0, 0))) : null;
    if (hurt) hurt.prepareForExport();
    else {
      // 目が見つからない形でも、GLB の契約（ノードと2本のクリップ）は保つ。空のノードを頭の骨に付ける
      const head = model.getObjectByName('Head');
      if (!head) throw new Error('＞＜の顔を付ける頭の骨がありません。');
      const empty = new Group(); empty.name = HURT_FACE_NODE; empty.scale.setScalar(0); head.add(empty);
    }
    applyStaticFace(model, document.config.face);
    const animations = selectOriginalGlbClips(character, gltf.animations, variant, hurtEyeTracks(model));
    assertAnimationTargetsResolve(root, animations);
    applyStaticHeadTurn(model, document.config.headTurn);
    root.updateMatrixWorld(true);
    root.userData.characterMaker = {
      ...JSON.parse(JSON.stringify(document)),
      export: {
        pose: 'source-rest',
        face: 'configured-static-values',
        autoBlinkAndSpeaking: 'settings-only-not-baked',
        headTurn: 'applied-to-static-pose-original-animation-tracks-take-precedence',
        variant,
        motionScope: variant === 'armed' ? 'sword-idle-and-attack-plus-hit' : 'idle-walk-and-run-plus-hit',
        clips: animations.map(clip => clip.name),
        // docs/main-character-spec.md: hit は一回再生、face_hurt は ＞＜ だけ（ループか重ねがけ）。＞＜ は node の scale で切り替える
        reactions: { hit: { clip: HIT_CLIP, loop: 'once', hurtFace: true }, hurtFace: { clip: HURT_FACE_CLIP, node: HURT_FACE_NODE, switch: 'node-scale-0-hidden-1-shown' } },
        weapon: variant === 'armed' ? originalSwordMetadata(character.id, weaponId) : { included: false, selectedId: weaponId, reason: 'standard-variant-excludes-weapon-and-closed-hand' },
        sourceProvenance: {
          modelSha256: sourceSha,
          originalModular: (gltf.parser.json as { extras?: { originalModular?: unknown } }).extras?.originalModular ?? null,
          originalModularPlacement: 'copied-from-source-model-top-level-extras-to-export-root-node-extras; not current editor state',
        },
      },
    };
    const result = await new GLTFExporter().parseAsync(root, {
      binary: true, trs: true, animations, onlyVisible: false,
    });
    if (!(result instanceof ArrayBuffer)) throw new Error('GLBの書き出しに失敗しました。');
    if (result.byteLength > 25 * 1024 * 1024) throw new Error('書き出したGLBが25MiBを超えています。');
    return result;
  } finally {
    body?.dispose();
    species?.dispose();
    humanParts?.dispose();
    hair?.dispose();
    disposeOwnedScenes(ownedMaterials, root, gltf.scene, ...(detachedNativeHand ? [detachedNativeHand] : []));
  }
}
