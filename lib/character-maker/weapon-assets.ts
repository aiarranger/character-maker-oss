// 登録された武器の素材（three を読まない。Rust の下見が使う。今の版の読み込みは lib/character-maker/registered-weapon.ts）。
import savedWeapons from '../../public/review-swing-2026-10-02/weapons.json';
import type { CharacterConfig } from './config';
import { selectedMakerWeaponId, type MakerWeaponId } from './weapon-catalog';
import { makerWeaponFit, type MakerWeaponRig } from './weapon-fit';
import { MAKER_WEAPON_SOCKETS } from './weapon-socket';

const assets = new Map(savedWeapons.weapons.map(weapon => [weapon.id, weapon]));

/** Stage へ渡す素材の名前（GLB として読む） */
export const weaponAssetName = (id: MakerWeaponId) => `weapon-${id}.glb`;

/** 登録された武器のバイト列（登録の SHA-256 と同じか確かめる） */
export async function fetchRegisteredWeapon(id: MakerWeaponId): Promise<Uint8Array> {
  const weapon = assets.get(id);
  if (!weapon) throw new Error('登録された武器が見つかりません。');
  const response = await fetch(`/review-swing-2026-10-02/${weapon.model}`);
  if (!response.ok) throw new Error(`「${weapon.name}」を読み込めませんでした。もう一度読み込んでください。`);
  const bytes = await response.arrayBuffer();
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  const sha256 = Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
  if (sha256 !== weapon.sha256.model) throw new Error(`「${weapon.name}」のモデルが登録された内容と一致しません。`);
  return new Uint8Array(bytes);
}

/** プリセットの武器の組み立ての値（createMakerSword と同じ値。武器なしは null） */
export function presetWeaponSpec(config: CharacterConfig) {
  const id = selectedMakerWeaponId(config);
  if (id === 'none') return null;
  const weapon = assets.get(id);
  if (!weapon) throw new Error('登録された武器が見つかりません。');
  const fit = makerWeaponFit(id, `${config.style}-${config.base}` as MakerWeaponRig);
  return { id, asset: weaponAssetName(id), length: weapon.length, grip: weapon.grip,
    fit: { sizeScale: fit.sizeScale, gripOffset: fit.gripOffset, visualScale: fit.visualScale, radius: fit.gripProtection.radius, transition: fit.gripProtection.transition },
    socket: MAKER_WEAPON_SOCKETS[config.style][config.base] };
}
