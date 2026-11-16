// ゲーム用の動作の名前（docs/game-motions-spec.md）。書き出す GLB のクリップ名で、GLB を使う側との契約なので変えない。
// どのキャラクターにも同じ動作を入れる（キャラによって違わない）。前半は characters のキャラクター一覧の共通動作
// （lib/main-character-defaults.ts の COMMON_MOTIONS と必殺技。名前も画面の呼び名も同じ）、後半はこのメーカーだけの追加。
// 原本は UAL1（CC0）と、姿勢から作った動作（tools/character-maker/pose-motions.mjs）。体ごとの JSON は
// tools/character-maker/build-action-motions.mjs（プリセット）と build-original-action-motions.mjs（オリジナル 4 体）が作る。

type Action = { id: string; label: string; loop: boolean; hold?: boolean; next?: string };
export const ACTION_MOTIONS = [
  // ---- キャラクター一覧と同じ（待機・歩行・走行・被弾・武器の 2 つは、今までの「動き」の並びにある） ----
  // 基準姿勢は動作ではなく、動きを止めて素の姿（骨の休止の姿）を見せる。GLB のクリップにはならない（下見だけ）
  { id: 'rest', label: '基準姿勢', loop: true },
  { id: 'jump', label: 'ジャンプ', loop: false },
  { id: 'dodge', label: '回避', loop: false },
  { id: 'restDown', label: '座る', loop: false, next: 'restLoop' },
  { id: 'restLoop', label: '休む', loop: true },
  { id: 'restUp', label: '立つ', loop: false },
  { id: 'banzai', label: 'バンザイ', loop: false },
  { id: 'greet', label: '手を振る', loop: false },
  { id: 'spinKick', label: '回転キック', loop: false },
  { id: 'eat', label: '食べる', loop: true },
  { id: 'skill', label: '必殺技', loop: false },
  // ---- このメーカーだけの追加 ----
  { id: 'punch', label: 'パンチ', loop: false },
  { id: 'interact', label: '調べる', loop: false },
  { id: 'down', label: 'やられ', loop: false, hold: true },
  { id: 'dance', label: 'ダンス', loop: true },
  { id: 'talk', label: '話す', loop: true },
  { id: 'crouch', label: 'しゃがむ', loop: true },
] as const satisfies readonly Action[];
export type ActionMotion = typeof ACTION_MOTIONS[number]['id'];
/** キャラクター一覧と同じ動作の数（ACTION_MOTIONS の前半） */
export const COMMON_ACTION_COUNT = 11;
const BY_ID = new Map<string, Action>(ACTION_MOTIONS.map(item => [item.id, item]));

export function isActionMotion(value: unknown): value is ActionMotion { return typeof value === 'string' && BY_ID.has(value); }
/** 1 回で終わる動作か（終わったら待機へ戻る。やられは倒れたまま） */
export function actionPlaysOnce(id: string) { const item = BY_ID.get(id); return !!item && !item.loop; }
/** 終わっても最後の姿のまま止める動作か（やられ） */
export function actionHolds(id: string) { return BY_ID.get(id)?.hold === true; }
/** 終わったら続けて始める動作（座る → 休む）。無ければ null（元の動きへ戻る） */
export function actionNext(id: string): ActionMotion | null { return (BY_ID.get(id)?.next as ActionMotion | undefined) ?? null; }
