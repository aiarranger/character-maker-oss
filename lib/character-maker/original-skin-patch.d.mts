/** original-skin-patch.mjs の型（取り込み先が node_modules 内の JS から型を推論しないため）。 */
export type OriginalSkinPatchProfile = {
  version: 'original-skin-patch-v1';
  width: number;
  height: number;
  samples: { rowStart: number; rowEnd: number; [key: string]: unknown };
  [key: string]: unknown;
};
export function applyOriginalSkinPatch(pixels: ArrayLike<number>, width: number, height: number, profile: OriginalSkinPatchProfile): Uint8ClampedArray;
