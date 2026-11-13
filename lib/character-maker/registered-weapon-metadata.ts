// 登録された武器の書き出し用の控え（three を読まない。Rust の下見の書き出しも使う）。読み込みは registered-weapon.ts。
import savedWeapons from '../../public/review-swing-2026-10-02/weapons.json';
import { makerWeaponFit, type MakerWeaponRig } from './weapon-fit';
import type { MakerWeaponId } from './weapon-catalog';

const assets = new Map(savedWeapons.weapons.map(weapon => [weapon.id, weapon]));

export function registeredMakerWeaponMetadata(id: MakerWeaponId, rig?: MakerWeaponRig) {
  if (id === 'none') return { included: false, id };
  const weapon = assets.get(id);
  if (!weapon) throw new Error('登録された武器が見つかりません。');
  return { included: true, id, name: weapon.name, source: savedWeapons.source, sha256: weapon.sha256.model,
    length: weapon.length, grip: weapon.grip, modelConvention: savedWeapons.modelConvention,
    ...(rig ? { fit: { ...makerWeaponFit(id, rig), nominalLength: weapon.length, calibratedLengthAtDefaultSize: weapon.length * makerWeaponFit(id, rig).effectiveScale } } : {}) };
}
