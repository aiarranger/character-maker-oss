/** Matches the registered main-list choices. No imported document can name a URL. */
export const MAKER_WEAPON_CHOICES = [
  { id: 'none', name: '持たない' },
  { id: 'baguette_sword', name: 'バゲットの剣' },
  { id: 'cake_sword', name: 'ケーキの剣' },
  { id: 'candy_axe', name: 'キャンディの斧' },
  { id: 'daikon_club', name: '大根のこん棒' },
  { id: 'ebi_lance', name: 'エビフライの槍' },
  { id: 'gyoza_mace', name: '餃子のメイス' },
  { id: 'ika_spear', name: 'イカの槍' },
  { id: 'momo_hammer', name: '鶏モモのハンマー' },
  { id: 'saba_sword', name: 'サバの剣' },
  { id: 'softserve_staff', name: 'ソフトクリームの杖' },
  { id: 'taiyaki_sword', name: 'たい焼きの剣' },
  { id: 'tamago_pan', name: '目玉焼きのフライパン' },
] as const;
export type MakerWeaponId = typeof MAKER_WEAPON_CHOICES[number]['id'];
export type RegisteredMakerWeaponId = Exclude<MakerWeaponId, 'none'>;
export function validateMakerWeaponId(value: unknown): MakerWeaponId {
  if (value === undefined) return 'none';
  if (typeof value !== 'string' || !MAKER_WEAPON_CHOICES.some(item => item.id === value)) throw new Error('武器の選択を読み込めません。');
  return value as MakerWeaponId;
}
/** Older settings deliberately resolve to none, matching the main list. */
export function selectedMakerWeaponId(config: { weaponId?: MakerWeaponId }): MakerWeaponId {
  return validateMakerWeaponId(config.weaponId);
}
