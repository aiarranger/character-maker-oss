// オリジナル 4 体を Rust の Stage で組み立てる値（three を読まない）。今の版の素材の読み方は lib/character-maker/original-weapon.ts・
// original-assembly.ts の resolveOriginalProfile・components/character-maker/OriginalEditableActor.tsx。素材の JSON は同じ物を import する。
import savedWeapons from '../../../public/review-swing-2026-10-02/weapons.json';
import haruClips from '../../../distribution/characters/generic-human-a/experiments/character-maker-sword-2026-10-03-v1/motions.json';
import haruManifest from '../../../distribution/characters/generic-human-a/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import nagiClips from '../../../distribution/characters/generic-human-b/experiments/character-maker-sword-2026-10-03-v1/motions.json';
import nagiManifest from '../../../distribution/characters/generic-human-b/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import luxClips from '../../../distribution/characters/generic-robot/experiments/character-maker-sword-2026-10-03-v1/motions.json';
import luxManifest from '../../../distribution/characters/generic-robot/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import mokaClips from '../../../distribution/characters/generic-cat/experiments/character-maker-sword-2026-10-03-v1/motions.json';
import mokaManifest from '../../../distribution/characters/generic-cat/experiments/character-maker-sword-2026-10-03-v1/manifest.json';
import haruBodyMap from '../../../distribution/characters/generic-human-a/experiments/character-maker-modular-2026-10-03-v1/body-vertex-map.json';
import nagiBodyMapV1 from '../../../distribution/characters/generic-human-b/experiments/character-maker-modular-2026-10-03-v1/body-vertex-map.json';
import nagiBodyMapV18 from '../../../distribution/characters/generic-human-b/experiments/character-maker-modular-2026-10-03-v18-rc/body-vertex-map.json';
import haruHit from '../../../parts/assets/original-hit-v1/generic-human-a-hit.json';
import nagiHit from '../../../parts/assets/original-hit-v1/generic-human-b-hit.json';
import luxHit from '../../../parts/assets/original-hit-v1/generic-robot-hit.json';
import mokaHit from '../../../parts/assets/original-hit-v1/generic-cat-hit.json';
import haruHand from '../../../distribution/characters/generic-human-a/experiments/character-maker-sword-hand-2026-10-03-v1/hand.json';
import nagiHand from '../../../distribution/characters/generic-human-b/experiments/character-maker-sword-hand-2026-10-03-v1/hand.json';
import { currentOriginalAppearance, originalHurtFaceStyle, originalModelAsset, type OriginalCharacter, type OriginalConfig } from '../../../lib/character-maker/original-config';
import { selectedMakerWeaponId, type MakerWeaponId } from '../../../lib/character-maker/weapon-catalog';
import { makerWeaponFit, type MakerWeaponRig } from '../../../lib/character-maker/weapon-fit';
import { registeredMakerWeaponMetadata } from '../../../lib/character-maker/registered-weapon-metadata';
import { fetchRegisteredWeapon, weaponAssetName } from '../../../lib/character-maker/weapon-assets';
import type { MakerStage } from './stage';

const packages = {
  'generic-human-a': { clips: haruClips, manifest: haruManifest, hit: haruHit },
  'generic-human-b': { clips: nagiClips, manifest: nagiManifest, hit: nagiHit },
  'generic-robot': { clips: luxClips, manifest: luxManifest, hit: luxHit },
  'generic-cat': { clips: mokaClips, manifest: mokaManifest, hit: mokaHit },
} as const;
const closedHands = { 'generic-human-a': haruHand, 'generic-human-b': nagiHand } as const;
const bodyMaps = { 'generic-human-a': [haruBodyMap], 'generic-human-b': [nagiBodyMapV1, nagiBodyMapV18] } as const;
const weapons = new Map(savedWeapons.weapons.map(weapon => [weapon.id, weapon]));

/** 模型の GLB のバイト列（同じ URL は 1 回だけ取る） */
const modelBytes = new Map<string, Promise<Uint8Array>>();
export function fetchModel(url: string) {
  let pending = modelBytes.get(url);
  if (!pending) {
    pending = fetch(url).then(async response => {
      if (!response.ok) throw new Error('3Dモデルを読み込めませんでした。');
      return new Uint8Array(await response.arrayBuffer());
    });
    pending.catch(() => modelBytes.delete(url));
    modelBytes.set(url, pending);
  }
  return pending;
}
/** 読み直しのとき（今の版の useGLTF.clear と retryMakerWeapon） */
export function forgetModel(url: string) { modelBytes.delete(url); }

const json = (value: unknown) => new TextEncoder().encode(JSON.stringify(value));

/** 組み立ての値と、そのための素材を Stage へ渡す */
export async function originalSpec(stage: MakerStage, character: OriginalCharacter, config: OriginalConfig) {
  const id = character.id as keyof typeof packages;
  const pack = packages[id];
  if (!pack) throw new Error('このキャラクターの武器の動作を読み込めません。');
  const asset = originalModelAsset(character, config.appearance);
  const model = `${asset.sha256}.glb`;
  const weaponId: MakerWeaponId = selectedMakerWeaponId(config);
  const hand = weaponId === 'none' ? null : closedHands[id as keyof typeof closedHands] ?? null;
  const parts = config.appearance.parts;
  // ネコとロボットは、独自の頂点の値（_SPECIES_*。形の歪みと GLB の書き出しが使う）を読むため GLB の生のバイト列も渡す
  const raw = (['generic-cat', 'generic-robot'] as string[]).includes(id) ? `bytes-${asset.sha256}` : null;
  const sword = `original-sword-${id}.json`, hit = `original-hit-${id}.json`;
  let weapon = null;
  if (weaponId !== 'none') {
    const entry = weapons.get(weaponId);
    if (!entry) throw new Error('登録された武器が見つかりません。');
    const fit = makerWeaponFit(weaponId, id as MakerWeaponRig);
    weapon = { id: weaponId, asset: weaponAssetName(weaponId), length: entry.length, grip: entry.grip,
      fit: { sizeScale: fit.sizeScale, gripOffset: fit.gripOffset, visualScale: fit.visualScale, radius: fit.gripProtection.radius, transition: fit.gripProtection.transition },
      socket: hand?.socket ?? pack.manifest.socket };
  }
  await Promise.all([
    (async () => {
      if (stage.has(model) && (!raw || stage.has(raw))) return;
      const bytes = await fetchModel(asset.url);
      if (!stage.has(model)) stage.asset(model, bytes);
      // 生のバイト列は「画像」の入れ物に入れて渡す（Stage の口を増やさないため。original_edit.rs の raw_accessor が読む）
      if (raw && !stage.has(raw)) stage.image(raw, bytes.length, 1, bytes);
    })(),
    stage.has(sword) ? null : Promise.resolve().then(() => stage.asset(sword, json(pack.clips))),
    stage.has(hit) ? null : Promise.resolve().then(() => stage.asset(hit, json(pack.hit))),
    weapon && !stage.has(weapon.asset) ? fetchRegisteredWeapon(weaponId).then(bytes => { if (!stage.has(weapon.asset)) stage.asset(weapon.asset, bytes); }) : null,
  ]);
  const current = currentOriginalAppearance(id, config.appearance);
  return {
    kind: 'original', character: id, model, raw, clips: [sword, hit], weapon,
    closedHand: hand, bodyMaps: hand ? bodyMaps[id as keyof typeof bodyMaps] : [],
    hurtStyle: originalHurtFaceStyle(id, parts) ?? null,
    viewer: { bodyHz: character.viewer.bodyHz ?? null, modelOffsetY: character.viewer.modelOffsetY ?? 0, modelScale: character.viewer.modelScale ?? 2 },
    smoothBody: true,
    // 種族の部品は保存のまま（古い保存は無し）、髪型は今の名前へ直した物、髪の色は保存のまま（今の版の useMemo と同じ）
    appearance: { parts: parts ?? null, hairstyle: current.hairstyle, hairColor: config.appearance.hairColor },
    config: originalStageConfig(config),
  };
}

/** 作り直さずに当てる設定（Rust の Original::apply_config） */
export function originalStageConfig(config: OriginalConfig) {
  return { size: config.size, face: config.face, autoBlink: config.autoBlink, speaking: config.speaking, hairMotion: config.hairMotion, headTurn: config.headTurn, appearance: { build: config.appearance.build } };
}

/** 書き出しの CharacterMakerSword の控え（createMakerSword と attachOriginalSword が userData に入れる値。測った長さは Rust が足す） */
export function originalSwordExtras(characterId: string, weaponId: MakerWeaponId) {
  const pack = packages[characterId as keyof typeof packages];
  if (!pack) throw new Error('このキャラクターの武器の動作を読み込めません。');
  const manifest = pack.manifest as { version: string; sourceModel: { sha256: string }; output: { sha256: string } };
  const extras: Record<string, unknown> = { characterId, motionVersion: manifest.version, sourceModelSha256: manifest.sourceModel.sha256, motionSha256: manifest.output.sha256 };
  if (weaponId !== 'none') {
    const entry = weapons.get(weaponId), fit = makerWeaponFit(weaponId, characterId as MakerWeaponRig);
    extras.registeredWeapon = registeredMakerWeaponMetadata(weaponId);
    if (entry) extras.makerWeaponFit = { ...fit, nominalLength: entry.length, calibratedLengthAtDefaultSize: entry.length * fit.effectiveScale };
  }
  return extras;
}
