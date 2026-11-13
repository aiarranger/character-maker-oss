// オリジナル 4 体の武器の書き出し用の控え（three を読まない。Rust の下見の書き出しも使う）。武器を付ける処理は original-weapon.ts。
import haruManifest from '../../distribution/characters/generic-human-a/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import nagiManifest from '../../distribution/characters/generic-human-b/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import luxManifest from '../../distribution/characters/generic-robot/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import mokaManifest from '../../distribution/characters/generic-cat/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import haruHand from '../../distribution/characters/generic-human-a/experiments/character-maker-sword-hand-2026-10-03-v1/hand.json';
import nagiHand from '../../distribution/characters/generic-human-b/experiments/character-maker-sword-hand-2026-10-03-v1/hand.json';
import haruHandManifest from '../../distribution/characters/generic-human-a/experiments/character-maker-sword-hand-2026-10-03-v1/manifest.json';
import nagiHandManifest from '../../distribution/characters/generic-human-b/experiments/character-maker-sword-hand-2026-10-03-v1/manifest.json';
import { registeredMakerWeaponMetadata } from './registered-weapon-metadata';
import type { MakerWeaponRig } from './weapon-fit';
import type { MakerWeaponId } from './weapon-catalog';

const closedHands = { 'generic-human-a': haruHand, 'generic-human-b': nagiHand };
const closedHandManifests = { 'generic-human-a': haruHandManifest, 'generic-human-b': nagiHandManifest };
const manifests = { 'generic-human-a': haruManifest, 'generic-human-b': nagiManifest, 'generic-robot': luxManifest, 'generic-cat': mokaManifest };
function originalWeaponPackage(characterId: string) {
  const manifest = manifests[characterId as keyof typeof manifests];
  if (!manifest) throw new Error('このキャラクターの武器の動作を読み込めません。');
  return { manifest };
}

export function originalSwordMetadata(characterId: string, weaponId: MakerWeaponId) {
  const { manifest } = originalWeaponPackage(characterId);
  return { ...registeredMakerWeaponMetadata(weaponId, characterId as MakerWeaponRig), version: manifest.version, sourceModelSha256: manifest.sourceModel.sha256, motionSha256: manifest.output.sha256,
    closedHand: characterId in closedHands ? { version: closedHands[characterId as keyof typeof closedHands].version, donorSha256: closedHands[characterId as keyof typeof closedHands].donor.sha256, packageSha256: closedHandManifests[characterId as keyof typeof closedHandManifests].output.sha256, transition: 'preview-post-mixer-binary-swap; GLB-separate-armed-static-variant-native-hand-removed' } : undefined,
    attachmentBone: manifest.socket.bone, playback: { sword_attack: { loop: false, next: 'sword_idle' } } };
}
