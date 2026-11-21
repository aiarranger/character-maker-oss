// 「魚群繚乱で遊ぶ」の入口を、ステージの左下（武器の並びの上）にどの形で出すか（docs/gyogun-spec.md の「入口のカード」）。
// 入口はいつもステージに出す（Owner 2026-11-16「幅で出さないのはやめて常に出す」）。窓の幅で隠したり、バーのボタンへ移したりしない:
// - 'card': 動画つきのカード（幅 132〜220px）。ステージが広いほど大きく、キャラと構えた武器に重なりにくい幅を選ぶ
// - 'pill': 動画を畳んだ札だけ（顔の表示のとき。キャラがステージいっぱいになるので小さくする。Owner 2026-11-16「札でよい」）
// 幅の数字は、Studio の 3 系統のキャラを「構える」で測った値（幅 1440・1280・1024・768・390。全身の正面・横・後ろ）:
//   足もとの左端は ステージの中央 − 高さ × 0.17、構えた武器の左端は 中央 − 高さ × 0.40、武器の下端は 高さ × 0.52。
export type GameCardMode = 'card' | 'pill';
export type GameCardFit = { mode: GameCardMode; width: number };

export const GAME_CARD_MAX = 220, GAME_CARD_MIN = 132;
/** 札だけのときの幅（文字と丸い矢印） */
export const GAME_PILL_WIDTH = 152;
/** ステージの端からの余白（四隅のトンボの内側）と、キャラとの間 */
const INSET = 34, GAP = 12;

export function gameCardFit(stageWidth: number, stageHeight: number, framing: 'full' | 'face'): GameCardFit {
  if (framing === 'face') return { mode: 'pill', width: GAME_PILL_WIDTH };
  // 足もとの左までに入る幅と、武器の左端より左か武器の下（武器の並びの見出しの上まで）に収まる幅。小さすぎるときも最小のカードは出す
  const beside = stageWidth / 2 - stageHeight * 0.17 - INSET - GAP;
  const leftOfWeapon = stageWidth / 2 - stageHeight * 0.40 - INSET - GAP;
  const underWeapon = (stageHeight * 0.48 - 126 - 16 - 44) * 16 / 9;
  const roomy = Math.min(GAME_CARD_MAX, beside, Math.max(leftOfWeapon, underWeapon));
  // ステージの左右の余白より広くはしない（スマホの細いステージでもはみ出さない）
  const width = Math.floor(Math.min(Math.max(GAME_CARD_MIN, roomy), GAME_CARD_MAX, stageWidth - INSET * 2));
  return { mode: 'card', width: Math.max(0, width) };
}
