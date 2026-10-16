import type { MakerWeaponId } from './weapon-catalog';
export type ArmedWeaponMotion = 'sword_idle' | 'sword_attack';
export type WeaponEntryMotion = 'idle' | 'walk' | 'run' | ArmedWeaponMotion;

/** An armed entry with no equipment requests a choice, never a default weapon. */
export function requestWeaponForMotion(motion: WeaponEntryMotion, weaponId: MakerWeaponId): ArmedWeaponMotion | null {
  return weaponId === 'none' && (motion === 'sword_idle' || motion === 'sword_attack') ? motion : null;
}
export function motionAfterWeaponChoice(weaponId: MakerWeaponId, request: ArmedWeaponMotion | null): 'idle' | ArmedWeaponMotion {
  return weaponId === 'none' ? 'idle' : request ?? 'sword_idle';
}
