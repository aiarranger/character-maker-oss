// Character Studio のキャラごとの色の世界（docs/studio-quality-spec.md §4.1）。
// 画面の見た目だけに使う。studio-catalog.json・保存データ・JSON には入れない。

export type StudioGroup = 'original' | 'fantasy' | 'comic';
export type StudioLook = { accent: string; look: string; group: StudioGroup };

export const STUDIO_GROUP_LABELS: Record<StudioGroup, string> = { original: 'オリジナル', fantasy: 'ファンタジー', comic: 'コミック' };

/** キーは選択ID（`original:<id>` と preset key）。どの2色も OKLab ΔE≥0.08（新4色）、C≥0.09、paper に対して 4.67〜7.87 のコントラスト。 */
export const STUDIO_PALETTE: Readonly<Record<string, StudioLook>> = {
  'original:generic-human-b': { accent: '#B0512A', look: '01', group: 'original' },
  'original:generic-human-a': { accent: '#3D6394', look: '02', group: 'original' },
  'original:generic-robot': { accent: '#0A7686', look: '03', group: 'original' },
  'original:generic-cat': { accent: '#7A4F9A', look: '04', group: 'original' },
  'fantasy:soft': { accent: '#0A684E', look: '05', group: 'fantasy' },
  'fantasy:fresh': { accent: '#4A43B5', look: '06', group: 'fantasy' },
  'fantasy:bright': { accent: '#8A6200', look: '07', group: 'fantasy' },
  'fantasy:bold': { accent: '#8A3A7A', look: '08', group: 'fantasy' },
  'comic:soft': { accent: '#367C14', look: '09', group: 'comic' },
  'comic:fresh': { accent: '#813C0B', look: '10', group: 'comic' },
  'comic:calm': { accent: '#54570F', look: '11', group: 'comic' },
  'comic:bright': { accent: '#8E2A45', look: '12', group: 'comic' },
};

export const STUDIO_DEFAULT_ACCENT = '#B0512A';

/** 表にない選択（将来の追加など）は、群だけ推定して既定の色にする。 */
export function studioLook(selectionId: string, fallbackIndex = 0): StudioLook {
  const known = STUDIO_PALETTE[selectionId];
  if (known) return known;
  const group: StudioGroup = selectionId.startsWith('original:') ? 'original' : selectionId.startsWith('comic:') ? 'comic' : 'fantasy';
  return { accent: STUDIO_DEFAULT_ACCENT, look: String(fallbackIndex + 1).padStart(2, '0'), group };
}

/** スライダーの塗り（§3.1 ⑤）。各 range の値の割合を、その input の --p に書く。描画のたびに呼ぶ。 */
export function paintRangeFills(root: ParentNode | null) {
  if (!root) return;
  for (const input of root.querySelectorAll<HTMLInputElement>('input[type="range"]')) {
    const min = Number(input.min || 0), max = Number(input.max || 100), value = Number(input.value);
    const percent = max > min ? ((value - min) / (max - min)) * 100 : 0;
    input.style.setProperty('--p', `${Math.min(100, Math.max(0, percent))}%`);
  }
}
