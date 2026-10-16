// メインキャラ対応：のけぞり と ＞＜ の名前（docs/main-character-spec.md）。GLB を使う側との契約なので変えない。

/** のけぞり（被弾）の一回再生クリップ。体の動き＋ ＞＜ の表示 */
export const HIT_CLIP = 'hit';
/** ＞＜ を出し続けるだけのクリップ（体の骨は動かさない） */
export const HURT_FACE_CLIP = 'face_hurt';
/** ＞＜ の顔の部品のノード名。通常は scale 0、クリップの scale トラックで 1 にする */
export const HURT_FACE_NODE = 'maker-hurt-face';
/** UAL1 の Hit_Chest（約0.33秒）を2倍に伸ばした長さの目安（秒） */
export const HIT_DURATION = 0.66;
