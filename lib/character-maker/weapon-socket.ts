// 武器の手の接続（three を読まない。今の版の lib/character-maker/weapon.ts と Rust の下見が同じ値を使う）
export type MakerWeaponStyle = 'comic' | 'fantasy';
export type MakerBase = 'feminine' | 'masculine';
type Vec3 = readonly [number, number, number];
type Socket = { bone: string; point: Vec3; blade: Vec3; palm: Vec3; size: number };
export const MAKER_SWORD_NAME = 'CharacterMakerSword';

// Raw hand-local frames, measured from the retained skeletons at Sword_Idle.
// The weapon points out of the thumb/index side of the closed fist. Body-build
// morphs keep the hand fixed; registered equipment inherits character height.
export const MAKER_WEAPON_SOCKETS: Record<MakerWeaponStyle, Record<MakerBase, Socket>> = {
  comic: {
    feminine: { bone: 'hand_r', point: [-0.0298, 0.0825, 0.0111], blade: [-0.0055, 0.2521, 0.9677], palm: [-1, 0, 0], size: 0.92 },
    masculine: { bone: 'hand_r', point: [-0.0357, 0.1087, 0.0076], blade: [-0.0052, 0.2565, 0.9665], palm: [-1, 0, 0], size: 1 },
  },
  fantasy: {
    feminine: { bone: 'J_Bip_R_Hand', point: [0.0633, -0.0182, -0.0084], blade: [0.01334, 0.05044, -0.99863], palm: [0, -1, 0], size: 0.9 },
    masculine: { bone: 'J_Bip_R_Hand', point: [0.0696, -0.0200, -0.0095], blade: [0.01262, 0.01226, -0.99985], palm: [0, -1, 0], size: 1 },
  },
};

