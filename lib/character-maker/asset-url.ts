/** Motion revisions prevent a cached pre-correction ready pose from resurfacing. */
const MOTION_REVISIONS: Readonly<Record<string, string>> = {
  "comic-feminine-hit-motions.json": "396a973615d4f307",
  "comic-feminine-sword-motions.json": "cb9efe3624e23590",
  "comic-masculine-hit-motions.json": "7374c577dac44d72",
  "comic-masculine-sword-motions.json": "b9fd60cb687148d0",
  "fantasy-feminine-hit-motions.json": "6a2b9bdba3b46e1d",
  "fantasy-feminine-motions.json": "e2daf85f9878b324",
  "fantasy-masculine-hit-motions.json": "92d70c5f54c4b102",
  "fantasy-masculine-motions.json": "b4267149bc981d99"
};

/** Same canonical maker asset names, with content revisions for corrected motions. */
export function makerAssetUrl(file: string): string {
  if (!/^[a-zA-Z0-9_.-]+$/.test(file)) throw new Error('キャラクター素材の名前が正しくありません。');
  const revision = MOTION_REVISIONS[file];
  return `/maker-assets/${file}${revision ? `?v=${revision}` : ''}`;
}
