import type { CharacterConfig } from './config';
import manifest from './outfit-manifest.json';

// プリセット（ファンタジー・アメコミ）の素材の名前。three を読まない（Rust の下見と今の版の組み立て lib/character-maker/assembly.ts が同じ名前を使う）。

type Palette = Exclude<NonNullable<CharacterConfig['outfitPalette']>, 'original'>;
const files: Record<`${CharacterConfig['style']}:${CharacterConfig['base']}`, Record<Palette, Record<string, string>>> = manifest.files;

/** Only exact, explicitly listed clothing materials can receive a palette map. */
export function outfitPaletteFiles(config: Pick<CharacterConfig, 'style' | 'base' | 'outfitPalette'>): Readonly<Record<string, string>> {
  const palette = config.outfitPalette;
  return !palette || palette === 'original' ? {} : files[`${config.style}:${config.base}`][palette];
}

/** 体の GLB */
export function presetBodyFile(config: CharacterConfig) {
  return `${config.style === 'fantasy' ? 'fantasy-' : ''}${config.base}.glb`;
}
/** 髪の GLB（無しは null） */
export function presetHairFile(config: CharacterConfig) {
  if (config.hair === 'none') return null;
  return config.style === 'fantasy' ? `fantasy-hair-${config.base}-${config.hair}.glb` : `hair-${config.hair}.glb`;
}
/** 動作の JSON。のけぞり（hit）は既存の動作 JSON を変えずに、別のファイルから足す（docs/main-character-spec.md） */
export function presetMotionFiles(config: CharacterConfig) {
  return config.style === 'fantasy'
    ? [`fantasy-${config.base}-motions.json`, `fantasy-${config.base}-hit-motions.json`]
    : [`${config.base}-motions.json`, `comic-${config.base}-sword-motions.json`, `comic-${config.base}-hit-motions.json`];
}
/** 大人の顔の肌（と男性の目のまつげ・目の線）の画像。大人の顔でなければ空 */
export function presetMatureFiles(config: CharacterConfig): { skin?: string; eyelash?: string; eyeline?: string } {
  if (config.style !== 'fantasy' || config.face !== 'mature') return {};
  return { skin: `fantasy-${config.base}-mature-skin.png`,
    ...(config.base === 'masculine' ? { eyelash: 'fantasy-masculine-mature-eyelash.png', eyeline: 'fantasy-masculine-mature-eyeline.png' } : {}) };
}
