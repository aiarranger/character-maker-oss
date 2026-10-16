/** 配布モデルの表示設定（characters の character.json の viewer と同じ形）。 */
export type OriginalViewer = {
  bodyHz?: number; modelOffsetY?: number; modelScale?: number; fullSpan?: number; faceHeight?: number; faceSpan?: number;
  headTurnDegrees?: number; neutralMouthClose?: number; expressionsAvailable?: boolean; expressionKeys?: string[]; motionsAvailable?: boolean;
  model?: { version: string; glb: string; sha256: string; bytes: number }; note?: string;
};
import type { EditHistory } from './edit-history';
import type { HurtFaceStyle } from './hurt-face';
import { defaultOriginalSpeciesParts, validateOriginalSpeciesParts, type OriginalSpeciesParts } from './original-species';
import { selectedMakerWeaponId, validateMakerWeaponId, type MakerWeaponId } from './weapon-catalog';

export const ORIGINAL_SIZE_RANGE = { min: 0.65, max: 1.35, step: 0.01 } as const;
export const ORIGINAL_FACE_CONTROLS = [
  ['blinkLeft', '左目を閉じる'], ['blinkRight', '右目を閉じる'], ['mouthClose', '口を閉じる'],
  ['aa', 'あ'], ['ee', 'い'], ['ou', 'う・お'], ['happy', '口角を上げる'],
] as const;
export type OriginalFaceKey = typeof ORIGINAL_FACE_CONTROLS[number][0];
export type OriginalFace = Record<OriginalFaceKey, number>;
export type OriginalMotion = 'idle' | 'walk' | 'run' | 'sword_idle' | 'sword_attack';
export const ORIGINAL_MOTIONS = [
  ['idle', '待機'], ['walk', '歩く'], ['run', '走る'], ['sword_idle', '武器を構える'], ['sword_attack', '武器を振る'],
] as const;
export const ORIGINAL_APPEARANCE_VERSION = 'original-appearance-v1' as const;
export const ORIGINAL_SPECIES_VERSION = 'original-appearance-v2' as const;
export const ORIGINAL_HAIRSTYLES = [
  { id: 'original', label: '元の髪型', description: 'その子らしい、いつもの姿' },
  { id: 'short', label: 'ショート', description: '軽やかな、すっきりシルエット' },
  { id: 'ponytail', label: 'サイドポニー', description: '横に結んだ、華やかな髪' },
] as const;
export const ORIGINAL_HUMAN_HAIRSTYLES = {
  'generic-human-a': [
    { id: 'haru-short-v2', label: 'ショート', description: '軽い毛束で、自然な表情に' },
    { id: 'haru-crop-v2', label: '刈り上げショート', description: 'すっきりと、活動的に' },
    { id: 'haru-center-v2', label: 'センターパート', description: '中央の分け目と、軽やかな前髪' },
    { id: 'haru-swept-v2', label: 'オールバック', description: '大人らしい、端正な横顔' },
    { id: 'haru-sidepart-v3', label: 'サイドパート', description: '前髪を横へ流した、すっきりした分け目' },
    { id: 'haru-buzz-v3', label: 'ベリーショート', description: '短い毛の質感を残した、軽快なスタイル' },
  ],
  'generic-human-b': [
    { id: 'nagi-bob-v2', label: 'ボブ', description: '頬を包む、いつものシルエット' },
    { id: 'nagi-pixie-v2', label: 'ピクシー', description: '短い毛束で、凛とした印象に' },
    { id: 'nagi-long-v2', label: 'ロングウェーブ', description: 'ゆるやかな毛先の流れ' },
    { id: 'nagi-pony-v2', label: 'ハイポニー', description: '後ろで結ぶ、軽やかな髪' },
  ],
} as const;
export type OriginalHumanHairstyle = typeof ORIGINAL_HUMAN_HAIRSTYLES[keyof typeof ORIGINAL_HUMAN_HAIRSTYLES][number]['id'];
export function originalHumanHairstyles(characterId: string) {
  return ORIGINAL_HUMAN_HAIRSTYLES[characterId as keyof typeof ORIGINAL_HUMAN_HAIRSTYLES] ?? [];
}
export function usesModularOriginal(appearance: OriginalAppearance) {
  return Object.values(ORIGINAL_HUMAN_HAIRSTYLES).some(items => items.some(item => item.id === appearance.hairstyle));
}
export const ORIGINAL_HAIR_COLORS = [
  ['#302b30', '黒髪'], ['#614439', '栗色'], ['#b88853', 'キャラメル'],
  ['#dfc79c', 'ブロンド'], ['#cbd0dc', 'シルバー'], ['#b97991', 'ローズ'],
  ['#7a739d', 'ラベンダー'], ['#587583', 'ブルーグレー'],
] as const;
export type OriginalAppearance = {
  version: typeof ORIGINAL_APPEARANCE_VERSION | typeof ORIGINAL_SPECIES_VERSION;
  parts?: OriginalSpeciesParts;
  build: number;
  hairstyle: typeof ORIGINAL_HAIRSTYLES[number]['id'] | OriginalHumanHairstyle | 'nagi-wavy-lob-v3';
  hairColor: string;
};
export function defaultOriginalAppearance(): OriginalAppearance {
  return { version: ORIGINAL_APPEARANCE_VERSION, build: 0, hairstyle: 'original', hairColor: 'source' };
}
export type OriginalCharacter = {
  id: string; name: string; nameEn: string;
  previewLabel?: string;
  thumbnail: string | null; thumbnailIsSheet: boolean;
  modelSha256: string; modelUrl: string;
  viewer: OriginalViewer;
  expressions: OriginalFaceKey[]; motions: OriginalMotion[]; hairMotion: boolean;
  appearanceModel?: { version: typeof ORIGINAL_APPEARANCE_VERSION; sha256: string; url: string };
  modularModel?: { version: 'original-human-modular-v1'; sha256: string; url: string; hairstyles: OriginalHumanHairstyle[] };
};
export type OriginalConfig = {
  size: number; face: OriginalFace; motion: OriginalMotion; playing: boolean; speed: number;
  autoBlink: boolean; speaking: boolean; hairMotion: boolean; headTurn: number;
  appearance: OriginalAppearance;
  /** Omitted legacy selections resolve to none without rewriting their config. */
  weaponId?: MakerWeaponId;
};
export type OriginalDocument = {
  format: 'characters.character-maker'; sourceKind: 'original'; schemaVersion: 4 | 5 | 6;
  assetVersion: 'original-maker-v1' | 'original-maker-v2'; name: string; characterId: string; modelSha256: string;
  config: OriginalConfig;
};

// Explicit aliases only: character identity and source hashes are never changed.
// Retired hair IDs remain importable, but are never a live model/mesh selection.
const CURRENT_HUMAN_HAIR: Record<string, Partial<Record<OriginalAppearance['hairstyle'], OriginalHumanHairstyle>>> = {
  'generic-human-a': { original: 'haru-short-v2', short: 'haru-crop-v2', ponytail: 'haru-short-v2' },
  'generic-human-b': { original: 'nagi-bob-v2', short: 'nagi-pixie-v2', ponytail: 'nagi-pony-v2', 'nagi-wavy-lob-v3': 'nagi-long-v2' },
};

export function currentOriginalAppearance(characterId: string, appearance: OriginalAppearance): OriginalAppearance {
  const hairstyle = CURRENT_HUMAN_HAIR[characterId]?.[appearance.hairstyle];
  if (!hairstyle) return appearance;
  return { ...appearance, version: ORIGINAL_SPECIES_VERSION, parts: appearance.parts ?? defaultOriginalSpeciesParts(), hairstyle };
}

/** Normalize typed editor snapshots without validating unfinished name edits. */
export function currentOriginalDocument<T extends OriginalDocument>(document: T): T {
  let appearance = currentOriginalAppearance(document.characterId, document.config.appearance);
  const hasWeaponSelection = document.config.weaponId !== undefined;
  // Editor snapshots can gain a weapon before their old schema has been upgraded.
  // Serialized imports are checked strictly before reaching this normalization.
  if (hasWeaponSelection && appearance.version === ORIGINAL_APPEARANCE_VERSION) {
    appearance = { ...appearance, version: ORIGINAL_SPECIES_VERSION, parts: appearance.parts ?? defaultOriginalSpeciesParts() };
  }
  const schemaVersion = hasWeaponSelection || document.schemaVersion === 6 ? 6
    : appearance !== document.config.appearance ? 5 : document.schemaVersion;
  const assetVersion = schemaVersion >= 5 ? 'original-maker-v2' : document.assetVersion;
  const motion = selectedMakerWeaponId(document.config) === 'none'
    && (document.config.motion === 'sword_idle' || document.config.motion === 'sword_attack') ? 'idle' : document.config.motion;
  if (appearance === document.config.appearance && motion === document.config.motion
    && schemaVersion === document.schemaVersion && assetVersion === document.assetVersion) return document;
  return { ...document, schemaVersion, assetVersion, config: { ...document.config, appearance, motion } };
}

export function currentOriginalHistory(history: EditHistory<OriginalDocument>): EditHistory<OriginalDocument> {
  const present = currentOriginalDocument(history.present);
  const past = history.past.map(document => currentOriginalDocument(document));
  const future = history.future.map(document => currentOriginalDocument(document));
  return present === history.present && past.every((document, index) => document === history.past[index])
    && future.every((document, index) => document === history.future[index]) ? history : { ...history, present, past, future };
}

export function neutralOriginalFace(character: OriginalCharacter): OriginalFace {
  return { blinkLeft: 0, blinkRight: 0, mouthClose: character.viewer.neutralMouthClose ?? 1, aa: 0, ee: 0, ou: 0, happy: 0 };
}

export function makeOriginalDocument(character: OriginalCharacter, name = character.name): OriginalDocument {
  return validateOriginalDocument({
    format: 'characters.character-maker', sourceKind: 'original', schemaVersion: 6,
    assetVersion: 'original-maker-v2', name, characterId: character.id, modelSha256: character.modelSha256,
    config: {
      size: 1, face: neutralOriginalFace(character), weaponId: 'cake_sword',
      motion: character.motions.includes('sword_idle') ? 'sword_idle' : character.motions[0] ?? 'idle', playing: true, speed: 1,
      autoBlink: ['blinkLeft', 'blinkRight'].every(key => character.expressions.includes(key as OriginalFaceKey)),
      speaking: false, hairMotion: character.hairMotion, headTurn: 0,
      appearance: { ...defaultOriginalAppearance(), version: ORIGINAL_SPECIES_VERSION, parts: defaultOriginalSpeciesParts(),
        ...(character.modularModel ? { hairstyle: character.modularModel.hairstyles[0] } : {}),
      },
    },
  });
}

function object(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${label}の形式が正しくありません。`);
  return value as Record<string, unknown>;
}
function number(value: unknown, min: number, max: number, label: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max) throw new Error(`${label}は${min}〜${max}の数値にしてください。`);
  return value;
}
function bool(value: unknown, label: string): boolean {
  if (typeof value !== 'boolean') throw new Error(`${label}の設定が正しくありません。`);
  return value;
}

/** Validate persisted/JSON input, then retire only explicitly known human hair IDs. */
export function validateOriginalDocument(value: unknown): OriginalDocument {
  const input = object(value, '設定ファイル');
  if (input.format !== 'characters.character-maker' || input.sourceKind !== 'original') throw new Error('オリジナルキャラクターの設定JSONを選んでください。');
  const legacy = input.schemaVersion === 4 && input.assetVersion === 'original-maker-v1';
  const weaponDocument = input.schemaVersion === 6 && input.assetVersion === 'original-maker-v2';
  if (!legacy && !weaponDocument && !(input.schemaVersion === 5 && input.assetVersion === 'original-maker-v2')) throw new Error('このオリジナル設定のバージョンには対応していません。');
  if (typeof input.name !== 'string' || !input.name.trim()) throw new Error('キャラクターの名前を入力してください。');
  if (typeof input.characterId !== 'string' || !/^[a-z][a-z0-9-]{1,48}$/.test(input.characterId)) throw new Error('元のキャラクターを特定できません。');
  if (typeof input.modelSha256 !== 'string' || !/^[a-f0-9]{64}$/i.test(input.modelSha256)) throw new Error('元のモデルの情報が正しくありません。');
  const config = object(input.config, 'キャラクター設定');
  if (!weaponDocument && Object.prototype.hasOwnProperty.call(config, 'weaponId')) throw new Error('武器を保存するにはバージョン6の設定ファイルが必要です。');
  const weaponId = validateMakerWeaponId(config.weaponId);
  const rawFace = object(config.face, '表情');
  // Validate the input schema before migrating known legacy human appearances.
  const appearance = config.appearance === undefined ? defaultOriginalAppearance() : object(config.appearance, '体型と髪');
  if (appearance.version !== (legacy ? ORIGINAL_APPEARANCE_VERSION : ORIGINAL_SPECIES_VERSION)) throw new Error('この体型・髪型のバージョンには対応していません。');
  const currentAppearance = currentOriginalAppearance(input.characterId, appearance as OriginalAppearance);
  if (!ORIGINAL_HAIRSTYLES.some(item => item.id === appearance.hairstyle)
    && (legacy || !originalHumanHairstyles(input.characterId).some(item => item.id === currentAppearance.hairstyle))) throw new Error('髪型の選択が読み込めません。');
  if (typeof appearance.hairColor !== 'string' || (appearance.hairColor !== 'source' && !/^#[a-f0-9]{6}$/i.test(appearance.hairColor))) throw new Error('髪色の設定が正しくありません。');
  const parts = legacy ? undefined : validateOriginalSpeciesParts(appearance.parts, input.characterId);
  if (parts && originalHumanHairstyles(input.characterId).length && !usesModularOriginal(currentAppearance)
    && ['head', 'eyes', 'mouth'].some(key => parts[key as 'head'] !== 'original')) throw new Error('顔のパーツを変えるには、新しい髪型を選んでください。');
  const face = Object.fromEntries(ORIGINAL_FACE_CONTROLS.map(([key, label]) => [key, number(rawFace[key], 0, 1, label)])) as OriginalFace;
  if (!(legacy ? ['idle', 'walk', 'run'] : ORIGINAL_MOTIONS.map(([id]) => id)).includes(String(config.motion) as OriginalMotion)) throw new Error('動作の選択が読み込めません。');
  return currentOriginalDocument({
    format: 'characters.character-maker', sourceKind: 'original', schemaVersion: weaponDocument ? 6 : legacy ? 4 : 5, assetVersion: legacy ? 'original-maker-v1' : 'original-maker-v2',
    name: input.name.trim(), characterId: input.characterId, modelSha256: input.modelSha256.toLowerCase(),
    config: {
      size: number(config.size, ORIGINAL_SIZE_RANGE.min, ORIGINAL_SIZE_RANGE.max, '大きさ'), face,
      motion: config.motion as OriginalMotion, playing: bool(config.playing, '再生'), speed: number(config.speed, 0.25, 2, '再生速度'),
      autoBlink: bool(config.autoBlink, '自動まばたき'), speaking: bool(config.speaking, '話す'), hairMotion: bool(config.hairMotion, '髪の揺れ'),
      headTurn: number(config.headTurn, -45, 45, '顔の向き'),
      ...(config.weaponId === undefined ? {} : { weaponId }),
      appearance: {
        version: legacy ? ORIGINAL_APPEARANCE_VERSION : ORIGINAL_SPECIES_VERSION,
        ...(parts ? { parts } : {}),
        build: number(appearance.build, -1, 1, '体型'),
        hairstyle: appearance.hairstyle as OriginalAppearance['hairstyle'], hairColor: appearance.hairColor.toLowerCase(),
      },
    },
  });
}

export function resolveOriginalCharacter(document: OriginalDocument, characters: readonly OriginalCharacter[]): OriginalCharacter {
  const character = characters.find(item => item.id === document.characterId);
  if (!character) throw new Error('元のキャラクターを読み込めません。設定は残っています。JSONに保存するか、キャラクターを選び直してください。');
  if (character.modelSha256 !== document.modelSha256) throw new Error('元のモデルが更新されています。以前の姿に自動で置き換えることはできません。設定はJSONに残せます。新しく作る場合はキャラクターを選び直してください。');
  assertOriginalCapabilities(character, document.config);
  return character;
}

export function assertOriginalCapabilities(character: OriginalCharacter, config: OriginalConfig) {
  const neutral = neutralOriginalFace(character);
  const appearance = currentOriginalAppearance(character.id, config.appearance);
  if (usesModularOriginal(appearance) && !character.modularModel?.hairstyles.some(id => id === appearance.hairstyle)) throw new Error('保存した髪パーツを読み込めません。設定はそのまま残っています。');
  if (!character.motions.includes(config.motion)) throw new Error('このキャラクターには、保存された動作がありません。');
  for (const [key] of ORIGINAL_FACE_CONTROLS) {
    if (!character.expressions.includes(key) && config.face[key] !== neutral[key]) throw new Error('このキャラクターには、保存された表情がありません。');
  }
  if (config.autoBlink && !['blinkLeft', 'blinkRight'].every(key => character.expressions.includes(key as OriginalFaceKey))) throw new Error('このキャラクターは自動まばたきに対応していません。');
  if (config.speaking && !['aa', 'ee', 'ou'].every(key => character.expressions.includes(key as OriginalFaceKey))) throw new Error('このキャラクターは会話の口の動きに対応していません。');
  if (config.hairMotion && !character.hairMotion) throw new Error('このキャラクターには髪の揺れの設定がありません。');
  if (Math.abs(config.headTurn) > (character.viewer.headTurnDegrees ?? 25)) throw new Error('このキャラクターで変えられる顔の向きを超えています。');
}

export function originalModelAsset(character: OriginalCharacter, appearance?: OriginalAppearance) {
  const current = currentOriginalAppearance(character.id, appearance ?? defaultOriginalAppearance());
  if (usesModularOriginal(current)) {
    if (!character.modularModel?.hairstyles.includes(current.hairstyle as OriginalHumanHairstyle)
      || !originalHumanHairstyles(character.id).some(item => item.id === current.hairstyle)) throw new Error('頭と髪のパーツを読み込めません。');
    return character.modularModel;
  }
  // Human bases must never fall back to a retired head/model, even if a caller
  // bypasses document loading or the current modular package is unavailable.
  if (originalHumanHairstyles(character.id).length) throw new Error('頭と髪のパーツを読み込めません。');
  return character.appearanceModel ?? { url: character.modelUrl, sha256: character.modelSha256 };
}

// Non-human legacy documents upgrade on edit; human hair is migrated on load.
export function editableOriginalDocument(document: OriginalDocument): OriginalDocument {
  document = currentOriginalDocument(document);
  if (document.schemaVersion >= 5) return document;
  return { ...document, schemaVersion: 5, assetVersion: 'original-maker-v2', config: { ...document.config, appearance: { ...document.config.appearance, version: ORIGINAL_SPECIES_VERSION, parts: defaultOriginalSpeciesParts() } } };
}

export function sameOriginalDocument(a: OriginalDocument, b: OriginalDocument) {
  return a.name === b.name && a.characterId === b.characterId && a.modelSha256 === b.modelSha256
    && JSON.stringify(a.config) === JSON.stringify(b.config);
}

/**
 * ＞＜ の顔の見た目（lib/character-maker/hurt-face.ts）。オリジナル4体それぞれの目に合わせた値。保存データには入れない。
 * mouth はのけぞり・＞＜ の間に重ねる口のモーフ。ルクスは画面の顔なので、画面の色で目を隠して光る線で描く。
 */
export const ORIGINAL_HURT_FACE_STYLES: Readonly<Record<string, HurtFaceStyle>> = {
  'generic-human-a': { line: '#3b2a2c', skin: '#f2d6c4', width: .085, size: 1.35, open: 62, lift: 0, inset: 0, mouth: { ee: .7, mouthClose: 0 } },
  'generic-human-b': { line: '#3b2a2c', skin: '#f4dccb', width: .095, size: 1.35, open: 62, lift: 0, inset: 0, mouth: { ee: .7, mouthClose: 0 } },
  'generic-robot': { line: '#8ff3ff', skin: '#1d2227', width: .11, size: 1.9, open: 66, lift: 0, inset: 0, fixedSkin: true, glow: 1.2, mouth: { ee: .7, mouthClose: 0 } },
  'generic-cat': { line: '#3a2a26', skin: '#e9c9a0', width: .09, size: 1.35, open: 62, lift: 0, inset: 0, sampled: true, mouth: { ee: .6, mouthClose: 0 } },
};

/** そのキャラの ＞＜ の見た目。ルクスの線は、画面の目と口の色（表示面の色）に合わせる */
export function originalHurtFaceStyle(characterId: string, parts?: OriginalSpeciesParts): HurtFaceStyle | undefined {
  const style = ORIGINAL_HURT_FACE_STYLES[characterId];
  if (style && characterId === 'generic-robot' && parts && parts.accentColor !== 'source') return { ...style, line: parts.accentColor };
  return style;
}
