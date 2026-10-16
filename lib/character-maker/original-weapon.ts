import { AnimationClip, BufferGeometry, Float32BufferAttribute, Group, Matrix4, Mesh, MeshBasicMaterial, SkinnedMesh, VectorKeyframeTrack, Vector3, type Object3D } from 'three';
import { addWeaponTracks, createMakerSword, fitMakerWeaponScale, disposeMakerWeapon } from './weapon';
import type { MakerWeaponRig } from './weapon-fit';
import type { MakerWeaponId } from './weapon-catalog';
import { registeredMakerWeaponMetadata } from './registered-weapon';
import haruClips from '../../distribution/characters/generic-human-a/experiments/character-maker-sword-2026-10-03-v1/motions.json';
import haruManifest from '../../distribution/characters/generic-human-a/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import nagiClips from '../../distribution/characters/generic-human-b/experiments/character-maker-sword-2026-10-03-v1/motions.json';
import nagiManifest from '../../distribution/characters/generic-human-b/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import luxClips from '../../distribution/characters/generic-robot/experiments/character-maker-sword-2026-10-03-v1/motions.json';
import luxManifest from '../../distribution/characters/generic-robot/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import mokaClips from '../../distribution/characters/generic-cat/experiments/character-maker-sword-2026-10-03-v1/motions.json';
import mokaManifest from '../../distribution/characters/generic-cat/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import haruBodyMap from '../../distribution/characters/generic-human-a/experiments/character-maker-modular-2026-10-03-v1/body-vertex-map.json';
import nagiBodyMapV1 from '../../distribution/characters/generic-human-b/experiments/character-maker-modular-2026-10-03-v1/body-vertex-map.json';
import nagiBodyMapV18 from '../../distribution/characters/generic-human-b/experiments/character-maker-modular-2026-10-03-v18-rc/body-vertex-map.json';

import haruHit from '../../parts/assets/original-hit-v1/generic-human-a-hit.json';
import nagiHit from '../../parts/assets/original-hit-v1/generic-human-b-hit.json';
import luxHit from '../../parts/assets/original-hit-v1/generic-robot-hit.json';
import mokaHit from '../../parts/assets/original-hit-v1/generic-cat-hit.json';
import { HIT_CLIP } from './hurt';

import haruHand from '../../distribution/characters/generic-human-a/experiments/character-maker-sword-hand-2026-10-03-v1/hand.json';
import nagiHand from '../../distribution/characters/generic-human-b/experiments/character-maker-sword-hand-2026-10-03-v1/hand.json';

import haruHandManifest from '../../distribution/characters/generic-human-a/experiments/character-maker-sword-hand-2026-10-03-v1/manifest.json';
import nagiHandManifest from '../../distribution/characters/generic-human-b/experiments/character-maker-sword-hand-2026-10-03-v1/manifest.json';
const closedHands = { 'generic-human-a': haruHand, 'generic-human-b': nagiHand };
const closedHandManifests = { 'generic-human-a': haruHandManifest, 'generic-human-b': nagiHandManifest };
const modularBodyMaps = { 'generic-human-a': [haruBodyMap], 'generic-human-b': [nagiBodyMapV1, nagiBodyMapV18] };

const packages = {
  'generic-human-a': { clips: haruClips, manifest: haruManifest },
  'generic-human-b': { clips: nagiClips, manifest: nagiManifest },
  'generic-robot': { clips: luxClips, manifest: luxManifest },
  'generic-cat': { clips: mokaClips, manifest: mokaManifest },
};
/** 武器と手の表示を切り替える scale のトラック（プレビュー用。GLB の書き出しでは外す） */
export const ORIGINAL_VISIBILITY_TRACKS: ReadonlySet<string> = new Set(['OriginalNativeRightHand.scale', 'OriginalSwordRightHand.scale', 'CharacterMakerSword.scale']);

const hitClips = { 'generic-human-a': haruHit, 'generic-human-b': nagiHit, 'generic-robot': luxHit, 'generic-cat': mokaHit };

/** のけぞり（UAL1 Hit_Chest を各体の骨へ移した追加のクリップ。tools/character-maker/build-original-hit-motions.mjs）。 */
export function originalHitClip(characterId: string): AnimationClip {
  const json = hitClips[characterId as keyof typeof hitClips]?.[0];
  if (!json) throw new Error('このキャラクターののけぞりの動作を読み込めません。');
  return AnimationClip.parse(json as Parameters<typeof AnimationClip.parse>[0]);
}

function originalWeaponPackage(characterId: string) {
  const entry = packages[characterId as keyof typeof packages];
  if (!entry) throw new Error('このキャラクターの武器の動作を読み込めません。');
  return entry;
}

/** The retained three clips keep every source track. Only sword visibility is
 * appended; the adapted clips (two sword motions and the hit) are new and never replace the source motions.
 */
export function originalClipsWithSword(characterId: string, source: readonly AnimationClip[], equipped = true): AnimationClip[] {
  const { clips } = originalWeaponPackage(characterId);
  const additions = [...clips.map(json => AnimationClip.parse(json as Parameters<typeof AnimationClip.parse>[0])), originalHitClip(characterId)];
  const result = addWeaponTracks([...source.filter(clip => !['sword_idle', 'sword_attack', HIT_CLIP].includes(clip.name)), ...additions], equipped);
  if (equipped && characterId in closedHands) for (const clip of result) {
    const sword = clip.name.startsWith('sword_');
    for (const [name, value] of [['OriginalNativeRightHand', sword ? 0 : 1], ['OriginalSwordRightHand', sword ? 1 : 0]] as const) {
      clip.tracks.push(new VectorKeyframeTrack(`${name}.scale`, [0, clip.duration], Array(6).fill(value)));
    }
  }
  // The swap uses scale tracks only. Existing per-node expression arrays stay
  // untouched, including every channel on the separate modular face meshes.

  return result;
}

function attachClosedHand(model: Object3D, characterId: string) {
  const profile = closedHands[characterId as keyof typeof closedHands];
  if (!profile) return null;
  const bodyMaps = modularBodyMaps[characterId as keyof typeof modularBodyMaps] ?? [];
  const outputCount = (bodyMap: (typeof bodyMaps)[number]) => {
    const outputIndices = new Set([...bodyMap.sourceToOutput.flat(), ...bodyMap.unmatchedOutput]);
    let minOutput = Infinity, maxOutput = -1;
    for (const index of outputIndices) {
      if (!Number.isInteger(index) || index < 0) throw new Error('通常の手の頂点対応表が正しくありません。');
      minOutput = Math.min(minOutput, index); maxOutput = Math.max(maxOutput, index);
    }
    if (minOutput !== 0 || maxOutput + 1 !== outputIndices.size) throw new Error('通常の手の頂点対応表が正しくありません。');
    return outputIndices.size;
  };
  const candidates: SkinnedMesh[] = [];
  model.traverse(object => { if (object instanceof SkinnedMesh && object.name.startsWith(profile.source.node)) candidates.push(object); });
  let native: SkinnedMesh | undefined;
  for (const object of candidates) {
    const geometry = object.geometry, position = geometry.getAttribute('position'), index = geometry.index;
    if (!index) continue;
    const compact = position.count !== profile.source.vertexCount;
    const bodyMap = compact ? bodyMaps.find(map => map.sourceSha256 === profile.source.sha256 && outputCount(map) === position.count) : undefined;
    // Legacy mouth primitives share the body name prefix but have their own
    // index space. Select only a verified body map matching this mesh's layout.
    if (compact && !bodyMap) continue;
    const keys = new Set<string>();
    for (const tri of profile.nativeTriangles) {
      const ids = compact ? tri.map(i => bodyMap!.sourceToOutput[i]) : tri.map(i => [i]);
      for (const a of ids[0]) for (const b of ids[1]) for (const c of ids[2]) keys.add([a, b, c].sort((a, b) => a - b).join(','));
    }
    const handIndices: number[] = [], remaining: number[] = [];
    for (let i = 0; i < index.count; i += 3) {
      const tri = [index.getX(i), index.getX(i + 1), index.getX(i + 2)];
      (keys.has([...tri].sort((a, b) => a - b).join(',')) ? handIndices : remaining).push(...tri);
    }
    if (!handIndices.length) continue;
    if (object.scale.toArray().some(value => Math.abs(value - 1) > 1e-7)) throw new Error('通常の手のスケールが固定原本と一致しません。');
    if (native || handIndices.length !== profile.nativeTriangles.length * 3 || remaining.length + handIndices.length !== index.count) throw new Error('通常の手の三角形と保存された原本が一致しません。');
    // Share every original attribute: splitting only indices preserves native
    // positions, normals, UV, skinning, and each complete morph channel exactly.
    const handGeometry = new BufferGeometry();
    for (const [key, attribute] of Object.entries(geometry.attributes)) handGeometry.setAttribute(key, attribute);
    handGeometry.morphAttributes = { ...geometry.morphAttributes };
    handGeometry.morphTargetsRelative = geometry.morphTargetsRelative;
    handGeometry.setIndex(handIndices); handGeometry.computeBoundingBox(); handGeometry.computeBoundingSphere();
    native = object.clone(false); native.name = 'OriginalNativeRightHand'; native.geometry = handGeometry;
    native.morphTargetDictionary = { ...object.morphTargetDictionary };
    native.morphTargetInfluences = object.morphTargetInfluences ? [...object.morphTargetInfluences] : undefined;
    native.userData.originalWeaponHand = 'native'; native.frustumCulled = false;
    object.parent!.add(native); geometry.setIndex(remaining);
  }
  if (!native) throw new Error('通常の手と武器用の手を組み合わせられません。');
  const geometry = new BufferGeometry();
  geometry.setAttribute('position', new Float32BufferAttribute(profile.positions, 3));
  geometry.setAttribute('color', new Float32BufferAttribute(profile.colors, 3));
  geometry.setIndex(profile.indices); geometry.computeVertexNormals(); geometry.computeBoundingSphere();
  const material = new MeshBasicMaterial({ vertexColors: true, toneMapped: false });
  const closed = new Mesh(geometry, material); closed.name = 'OriginalSwordRightHand';
  closed.userData.originalWeaponHand = 'closed'; closed.scale.setScalar(0); closed.frustumCulled = false;
  model.getObjectByName('R_Hand')!.add(closed);
  return { native, closed, profile, dispose() { native!.geometry.dispose(); geometry.dispose(); material.dispose(); } };
}

/** Each character retains its measured socket while registered designs are
 * shared. The returned attachment owns its geometry and materials.
 */
export function attachOriginalSword(model: Object3D, characterId: string, weaponId: MakerWeaponId, source: Object3D | null, baseCalibrationScale = 2) {
  const { manifest } = originalWeaponPackage(characterId);
  const handParts = weaponId === 'none' ? null : attachClosedHand(model, characterId);
  const socket = handParts?.profile.socket ?? manifest.socket;
  const hand = model.getObjectByName(socket.bone);
  if (!hand) throw new Error('武器を持つ手の骨を読み込めません。');
  const sword = createMakerSword('fantasy', 'feminine', weaponId, source, characterId as MakerWeaponRig);
  const grip = sword.getObjectByName('MakerSwordGrip') as Group;
  const y = new Vector3().fromArray(socket.blade).normalize();
  const z = new Vector3().fromArray(socket.palm).addScaledVector(y, -new Vector3().fromArray(socket.palm).dot(y)).normalize();
  const x = new Vector3().crossVectors(y, z).normalize(); z.crossVectors(x, y).normalize();
  grip.position.fromArray(socket.point);
  grip.quaternion.setFromRotationMatrix(new Matrix4().makeBasis(x, y, z));
  grip.scale.setScalar(socket.size);
  sword.userData = { ...sword.userData, attachmentBone: socket.bone, characterId, motionVersion: manifest.version,
    sourceModelSha256: manifest.sourceModel.sha256, motionSha256: manifest.output.sha256 };
  hand.add(sword);
  fitMakerWeaponScale(sword, hand, model, baseCalibrationScale);
  return { sword, applyGrip(value: number) { if (handParts) { handParts.native.scale.setScalar(value ? 0 : 1); handParts.closed.scale.setScalar(value ? 1 : 0); } }, dispose() {
    // Keep the hierarchy intact for React StrictMode effect replay.
    disposeMakerWeapon(sword);
    handParts?.dispose();
  } };
}

export function originalSwordMetadata(characterId: string, weaponId: MakerWeaponId) {
  const { manifest } = originalWeaponPackage(characterId);
  return { ...registeredMakerWeaponMetadata(weaponId, characterId as MakerWeaponRig), version: manifest.version, sourceModelSha256: manifest.sourceModel.sha256, motionSha256: manifest.output.sha256,
    closedHand: characterId in closedHands ? { version: closedHands[characterId as keyof typeof closedHands].version, donorSha256: closedHands[characterId as keyof typeof closedHands].donor.sha256, packageSha256: closedHandManifests[characterId as keyof typeof closedHandManifests].output.sha256, transition: 'preview-post-mixer-binary-swap; GLB-separate-armed-static-variant-native-hand-removed' } : undefined,
    attachmentBone: manifest.socket.bone, playback: { sword_attack: { loop: false, next: 'sword_idle' } } };
}
