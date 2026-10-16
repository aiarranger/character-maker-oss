import { validateMakerWeaponId, type MakerWeaponId } from './weapon-catalog';

export const BASES = ['feminine', 'masculine'] as const;
export const STYLES = ['fantasy', 'comic'] as const;
export const OUTFIT_PALETTES = ['original', 'pearl', 'ink', 'sage', 'wine', 'cobalt', 'ochre'] as const;
export const COMIC_HAIR_STYLES = ['parted', 'long', 'buns', 'crop', 'pixie', 'none'] as const;
export const FANTASY_HAIR_STYLES = ['animeParted', 'sidePony', 'layered', 'sideSweep', 'sweptBack', 'bluntBob', 'longLoose', 'none'] as const;
export const HAIR_STYLES = [...COMIC_HAIR_STYLES, 'animeParted', 'sidePony', 'layered', 'sideSweep', 'sweptBack', 'bluntBob', 'longLoose'] as const;
export const COMIC_FACE_SHAPES = ['oval', 'round', 'angular'] as const;
export const FANTASY_FACE_SHAPES = [...COMIC_FACE_SHAPES, 'mature'] as const;
export const FACE_SHAPES = FANTASY_FACE_SHAPES;
export const EYE_STYLES = ['almond', 'round', 'slender'] as const;
export const MOUTH_STYLES = ['natural', 'smile', 'full'] as const;
export const EXPRESSIONS = ['neutral', 'happy', 'angry', 'sad', 'relaxed', 'surprised', 'aa', 'ih', 'ou', 'ee', 'oh', 'blink', 'blinkLeft', 'blinkRight', 'lookLeft', 'lookRight', 'lookUp', 'lookDown'] as const;
export const EXPRESSION_CHOICES: ReadonlyArray<{ id: typeof EXPRESSIONS[number]; label: string; group: 'emotion' | 'mouth' | 'blink' | 'gaze' }> = [
  { id: 'neutral', label: 'ふつう', group: 'emotion' }, { id: 'happy', label: '笑顔', group: 'emotion' },
  { id: 'angry', label: '怒り', group: 'emotion' }, { id: 'sad', label: '悲しみ', group: 'emotion' },
  { id: 'relaxed', label: '穏やか', group: 'emotion' }, { id: 'surprised', label: '驚き', group: 'emotion' },
  { id: 'aa', label: 'あ', group: 'mouth' }, { id: 'ih', label: 'い', group: 'mouth' },
  { id: 'ou', label: 'う', group: 'mouth' }, { id: 'ee', label: 'え', group: 'mouth' }, { id: 'oh', label: 'お', group: 'mouth' },
  { id: 'blink', label: '両目', group: 'blink' }, { id: 'blinkLeft', label: '左目', group: 'blink' }, { id: 'blinkRight', label: '右目', group: 'blink' },
  { id: 'lookLeft', label: '本人の左', group: 'gaze' }, { id: 'lookRight', label: '本人の右', group: 'gaze' },
  { id: 'lookUp', label: '上', group: 'gaze' }, { id: 'lookDown', label: '下', group: 'gaze' },
];

export type CharacterConfig = {
  style: typeof STYLES[number];
  base: typeof BASES[number];
  height: number;
  build: number;
  hair: typeof HAIR_STYLES[number];
  hairColor: string;
  face: typeof FACE_SHAPES[number];
  eyes: typeof EYE_STYLES[number];
  mouth: typeof MOUTH_STYLES[number];
  expression: typeof EXPRESSIONS[number];
  expressionStrength: number;
  /** Omitted (or original) keeps the source clothing maps and legacy document format. */
  outfitPalette?: typeof OUTFIT_PALETTES[number];
  /** Omitted legacy selections resolve to none without rewriting their config. */
  weaponId?: MakerWeaponId;
};

export const HEIGHT_RANGE = { min: 0.85, max: 1.12, step: 0.01 } as const;
export const BUILD_RANGE = { min: -1, max: 1, step: 0.05 } as const;
export const EXPRESSION_STRENGTH_RANGE = { min: 0, max: 1, step: 0.05 } as const;
export const DOCUMENT_FORMAT = 'characters.character-maker' as const;
export const ASSET_VERSION = 'human-maker-v1' as const;
export const FANTASY_ASSET_VERSION = 'fantasy-maker-v1' as const;

export type CharacterMakerDocument = {
  format: typeof DOCUMENT_FORMAT;
  schemaVersion: 1 | 2 | 3 | 4 | 5;
  assetVersion: typeof ASSET_VERSION | typeof FANTASY_ASSET_VERSION;
  name: string;
  config: CharacterConfig;
};

export const DEFAULT_CONFIG: CharacterConfig = {
  style: 'fantasy', base: 'feminine', height: 1, build: 0, hair: 'sidePony', hairColor: '#EEE3CF',
  face: 'oval', eyes: 'almond', mouth: 'natural', expression: 'neutral', expressionStrength: 1, weaponId: 'cake_sword',
};
export const COMIC_DEFAULT_CONFIG: CharacterConfig = { ...DEFAULT_CONFIG, style: 'comic', hair: 'parted', hairColor: '#514036' };

export const PRESETS: ReadonlyArray<{ id: string; name: string; description: string; config: CharacterConfig }> = [
  {
    "id": "soft",
    "name": "セレネ",
    "description": "黒髪のサイドポニー / すっきりした面長 / 真珠と青磁",
    "config": {
      "weaponId": "cake_sword",
      "style": "fantasy",
      "base": "feminine",
      "height": 1.06,
      "build": -0.4,
      "hair": "sidePony",
      "hairColor": "#292D36",
      "face": "mature",
      "eyes": "slender",
      "mouth": "natural",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "pearl"
    }
  },
  {
    "id": "fresh",
    "name": "アズール",
    "description": "流した青髪 / 柔らかな細面 / 群青と白",
    "config": {
      "weaponId": "cake_sword",
      "style": "fantasy",
      "base": "masculine",
      "height": 0.98,
      "build": -0.55,
      "hair": "sideSweep",
      "hairColor": "#507FA0",
      "face": "oval",
      "eyes": "round",
      "mouth": "natural",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "cobalt"
    }
  },
  {
    "id": "calm",
    "name": "ノワール",
    "description": "スモーク色のレイヤー / 黒曜と銀",
    "config": {
      "style": "fantasy",
      "base": "feminine",
      "height": 1.1,
      "build": 0.2,
      "hair": "layered",
      "hairColor": "#D9DCE8",
      "face": "mature",
      "eyes": "slender",
      "mouth": "natural",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "ink"
    }
  },
  {
    "id": "bright",
    "name": "コハク",
    "description": "銅色のフリンジボブ / 丸い頬 / 琥珀と墨",
    "config": {
      "weaponId": "cake_sword",
      "style": "fantasy",
      "base": "feminine",
      "height": 0.87,
      "build": 0.65,
      "hair": "bluntBob",
      "hairColor": "#AD4C2F",
      "face": "round",
      "eyes": "round",
      "mouth": "smile",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "ochre"
    }
  },
  {
    "id": "elegant",
    "name": "ロゼ",
    "description": "淡い金のポニー / ワインと黒",
    "config": {
      "style": "fantasy",
      "base": "feminine",
      "height": 0.97,
      "build": 0.1,
      "hair": "sidePony",
      "hairColor": "#E5D2B0",
      "face": "oval",
      "eyes": "almond",
      "mouth": "full",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "wine"
    }
  },
  {
    "id": "composed",
    "name": "レオン",
    "description": "深い茶のレイヤー / 深緑と真鍮",
    "config": {
      "style": "fantasy",
      "base": "masculine",
      "height": 1.12,
      "build": 0.75,
      "hair": "layered",
      "hairColor": "#473528",
      "face": "mature",
      "eyes": "slender",
      "mouth": "natural",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "sage"
    }
  },
  {
    "id": "scholar",
    "name": "シオン",
    "description": "アッシュのサイドポニー / ワインと黒",
    "config": {
      "style": "fantasy",
      "base": "masculine",
      "height": 1.04,
      "build": -0.75,
      "hair": "sidePony",
      "hairColor": "#D5D5DC",
      "face": "oval",
      "eyes": "slender",
      "mouth": "natural",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "wine"
    }
  },
  {
    "id": "bold",
    "name": "ジン",
    "description": "金のアップバング / 広い顎と落ち着いた目 / 黒曜と銀",
    "config": {
      "weaponId": "cake_sword",
      "style": "fantasy",
      "base": "masculine",
      "height": 1.08,
      "build": 0.65,
      "hair": "sweptBack",
      "hairColor": "#D7B46A",
      "face": "mature",
      "eyes": "almond",
      "mouth": "natural",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "ink"
    }
  }
];

const COMIC_PRESETS: typeof PRESETS = [
  {
    "id": "soft",
    "name": "アイリス",
    "description": "銀のピクシー / 群青と白",
    "config": {
      "weaponId": "cake_sword",
      "style": "comic",
      "base": "feminine",
      "height": 1.1,
      "build": -0.65,
      "hair": "pixie",
      "hairColor": "#C8D3E0",
      "face": "angular",
      "eyes": "slender",
      "mouth": "natural",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "cobalt"
    }
  },
  {
    "id": "fresh",
    "name": "ローワン",
    "description": "黒髪のクロップ / 琥珀と墨",
    "config": {
      "weaponId": "cake_sword",
      "style": "comic",
      "base": "masculine",
      "height": 1.12,
      "build": 0.95,
      "hair": "crop",
      "hairColor": "#262A2D",
      "face": "angular",
      "eyes": "almond",
      "mouth": "natural",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "ochre"
    }
  },
  {
    "id": "calm",
    "name": "セージ",
    "description": "長い黒髪 / 深緑と真鍮",
    "config": {
      "weaponId": "cake_sword",
      "style": "comic",
      "base": "feminine",
      "height": 1.04,
      "build": 0.45,
      "hair": "long",
      "hairColor": "#28393A",
      "face": "oval",
      "eyes": "slender",
      "mouth": "full",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "sage"
    }
  },
  {
    "id": "bright",
    "name": "パイパー",
    "description": "赤毛のツインバン / ワインと黒",
    "config": {
      "weaponId": "cake_sword",
      "style": "comic",
      "base": "feminine",
      "height": 0.88,
      "build": 0.65,
      "hair": "buns",
      "hairColor": "#BD653C",
      "face": "round",
      "eyes": "round",
      "mouth": "smile",
      "expression": "neutral",
      "expressionStrength": 1,
      "outfitPalette": "wine"
    }
  }
];

export function getPresets(style: CharacterConfig['style']) {
  return style === 'comic' ? COMIC_PRESETS : PRESETS;
}

/** Hidden presets remain readable so old JSON/browser saves never lose identity. */
export const ACTIVE_FANTASY_PRESET_IDS = ['soft', 'bright', 'fresh', 'bold'] as const;

/** Retired masculine ponytails are migrated on load and never offered for edits. */
export function getHairStyles(style: CharacterConfig['style'], base: CharacterConfig['base']) {
  return style === 'comic' ? COMIC_HAIR_STYLES : FANTASY_HAIR_STYLES.filter(hair => base !== 'masculine' || hair !== 'sidePony');
}

export const HAIR_STYLE_LABELS: Record<CharacterConfig['hair'], string> = {
  parted: 'サイドパート', long: 'ボブ', buns: 'おだんご', crop: 'ショート', pixie: 'ベリーショート', none: '髪なし',
  animeParted: 'エアリーショート', sidePony: 'サイドポニー', layered: 'レイヤーショート',
  sideSweep: 'サイドスイープ', sweptBack: 'アップバング', bluntBob: 'フリンジボブ', longLoose: 'ストレートロング',
};

export function getFaceShapes(style: CharacterConfig['style']) {
  return style === 'comic' ? COMIC_FACE_SHAPES : FANTASY_FACE_SHAPES;
}

export const HAIR_COLORS = [
  { color: '#292B32', name: '黒' }, { color: '#514036', name: 'こげ茶' },
  { color: '#8D5540', name: '赤茶' }, { color: '#B38558', name: '明るい茶' },
  { color: '#D2B77B', name: '金' }, { color: '#C4C7CD', name: '銀' },
  { color: '#6B527D', name: '紫' }, { color: '#B87991', name: 'ピンク' },
  { color: '#3D6873', name: '青緑' }, { color: '#4D6888', name: '青' },
] as const;

function object(value: unknown, label: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${label}の形式が正しくありません。`);
  const prototype = Object.getPrototypeOf(value);
  if (prototype !== Object.prototype && prototype !== null) throw new Error(`${label}の形式が正しくありません。`);
  return value as Record<string, unknown>;
}

function choice<T extends string>(value: unknown, values: readonly T[], label: string): T {
  if (typeof value !== 'string' || !values.includes(value as T)) throw new Error(`${label}の選択が読み込めません。`);
  return value as T;
}

function numberInRange(value: unknown, min: number, max: number, label: string): number {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < min || value > max) {
    throw new Error(`${label}は${min}〜${max}の数値にしてください。`);
  }
  return value;
}

/** Only this retired combination changes; names and feminine ponytails stay intact. */
export function currentCharacterConfig(config: CharacterConfig): CharacterConfig {
  return config.style === 'fantasy' && config.base === 'masculine' && config.hair === 'sidePony'
    ? { ...config, hair: 'sideSweep' } : config;
}

/** Typed history can contain unfinished names, so normalization is separate from validation. */
export function currentCharacterDocument<T extends CharacterMakerDocument>(document: T): T {
  const config = currentCharacterConfig(document.config);
  const schemaVersion = config.weaponId === undefined ? document.schemaVersion : 5;
  return config === document.config && schemaVersion === document.schemaVersion ? document : { ...document, schemaVersion, config };
}

/** Build a new object from supported fields. Imported objects are never used as scene or application state directly. */
export function validateConfig(value: unknown): CharacterConfig {
  const input = object(value, 'キャラクター設定');
  if (typeof input.hairColor !== 'string' || !/^#[0-9a-f]{6}$/i.test(input.hairColor)) throw new Error('髪色の形式が正しくありません。');
  const style = choice(input.style, STYLES, 'スタイル');
  const outfitPalette = choice(input.outfitPalette === undefined ? 'original' : input.outfitPalette, OUTFIT_PALETTES, '服の色');
  const expression = choice(input.expression === undefined ? 'neutral' : input.expression, EXPRESSIONS, '表情');
  const expressionStrength = numberInRange(input.expressionStrength === undefined ? 1 : input.expressionStrength, 0, 1, '表情の強さ');
  const weaponId = validateMakerWeaponId(input.weaponId);
  if (style === 'comic' && expression !== 'neutral') throw new Error('この表情はファンタジースタイルで使えます。');
  return currentCharacterConfig({
    style,
    base: choice(input.base, BASES, '体のベース'),
    height: numberInRange(input.height, HEIGHT_RANGE.min, HEIGHT_RANGE.max, '身長'),
    build: numberInRange(input.build, BUILD_RANGE.min, BUILD_RANGE.max, '体つき'),
    hair: choice(input.hair, style === 'comic' ? COMIC_HAIR_STYLES : FANTASY_HAIR_STYLES, '髪型'), hairColor: input.hairColor.toUpperCase(),
    face: choice(input.face, getFaceShapes(style), '顔の形'), eyes: choice(input.eyes, EYE_STYLES, '目'),
    mouth: choice(input.mouth, MOUTH_STYLES, '口'),
    expression, expressionStrength: expression === 'neutral' ? 1 : expressionStrength,
    ...(outfitPalette === 'original' ? {} : { outfitPalette }),
    ...(input.weaponId === undefined ? {} : { weaponId }),
  });
}

export function validateDocument(value: unknown): CharacterMakerDocument {
  const input = object(value, '設定ファイル');
  if (input.format !== DOCUMENT_FORMAT) throw new Error('キャラクターメイクの設定JSONを選んでください。');
  if (input.schemaVersion !== 1 && input.schemaVersion !== 2 && input.schemaVersion !== 3 && input.schemaVersion !== 4 && input.schemaVersion !== 5) throw new Error('この設定ファイルのバージョンには対応していません。');
  const legacy = input.schemaVersion === 1;
  const rawConfig = object(input.config, 'キャラクター設定');
  const weaponDocument = input.schemaVersion === 5;
  if (!weaponDocument && Object.prototype.hasOwnProperty.call(rawConfig, 'weaponId')) throw new Error('武器を保存するにはバージョン5の設定ファイルが必要です。');
  const paletteDocument = input.schemaVersion === 4 || weaponDocument;
  const comic = legacy || (paletteDocument && rawConfig.style === 'comic');
  const assetVersion = comic ? ASSET_VERSION : FANTASY_ASSET_VERSION;
  if (input.assetVersion !== assetVersion) throw new Error('この設定ファイルで使われているキャラクター素材には対応していません。');
  if (typeof input.name !== 'string' || !input.name.trim()) throw new Error('キャラクターの名前を入力してください。');
  if ((input.schemaVersion === 3 || paletteDocument) && (rawConfig.expression === undefined || rawConfig.expressionStrength === undefined)) throw new Error('表情の設定が不足しています。');
  if (legacy && rawConfig.style !== undefined && rawConfig.style !== 'comic') throw new Error('設定ファイルのスタイルと素材が一致していません。');
  const config = validateConfig(legacy ? { ...rawConfig, style: 'comic' } : rawConfig);
  if (!comic && config.style !== 'fantasy') throw new Error('設定ファイルのスタイルと素材が一致していません。');
  if (!paletteDocument && config.outfitPalette) throw new Error('服の色を保存するにはバージョン4の設定ファイルが必要です。');
  if (input.schemaVersion !== 3 && !paletteDocument && config.expression !== 'neutral') throw new Error('この設定ファイルのバージョンでは表情を保存できません。');
  return {
    format: DOCUMENT_FORMAT, schemaVersion: input.schemaVersion, assetVersion,
    name: input.name.trim(), config,
  };
}

export function makeDocument(name: string, config: CharacterConfig): CharacterMakerDocument {
  const clean = validateConfig(config);
  return validateDocument({ format: DOCUMENT_FORMAT, schemaVersion: clean.weaponId !== undefined ? 5 : clean.outfitPalette ? 4 : clean.style === 'comic' ? 1 : clean.expression === 'neutral' ? 2 : 3, assetVersion: clean.style === 'comic' ? ASSET_VERSION : FANTASY_ASSET_VERSION, name, config: clean });
}

/** Legacy appearance stays in v1–v4; an explicit weapon selection requires a v5-aware reader. */
export function serializeDocument(document: CharacterMakerDocument) {
  const clean = makeDocument(document.name, document.config);
  if (clean.schemaVersion === 3 || clean.schemaVersion === 4 || clean.schemaVersion === 5) return clean;
  const { expression: _expression, expressionStrength: _strength, ...appearance } = clean.config;
  if (clean.config.style === 'fantasy') return { ...clean, config: appearance };
  const { style: _style, ...config } = appearance;
  return { ...clean, schemaVersion: 1 as const, assetVersion: ASSET_VERSION, config };
}

export function parseDocument(text: string): CharacterMakerDocument {
  let value: unknown;
  try { value = JSON.parse(text); } catch { throw new Error('JSONを読み込めませんでした。キャラクターメイクから書き出した設定ファイルを選んでください。'); }
  return validateDocument(value);
}

/** 読み込み時と同じ正規化（引退した髪型の置き換えなど）をしてから、項目の並びに関係なく比べる。プリセットの選択中表示に使う。 */
export function sameConfig(a: CharacterConfig, b: CharacterConfig): boolean {
  const x = makeDocument('比較', a).config as Record<string, unknown>, y = makeDocument('比較', b).config as Record<string, unknown>;
  return [...new Set([...Object.keys(x), ...Object.keys(y)])].every(key => x[key] === y[key]);
}

export function sameDocument(a: CharacterMakerDocument, b: CharacterMakerDocument): boolean {
  return a.name === b.name && JSON.stringify(a.config) === JSON.stringify(b.config);
}

export function downloadFileName(name: string): string {
  return name.trim().replace(/[<>:"/\\|?*\u0000-\u001f]/g, '-').replace(/^[. ]+|[. ]+$/g, '') || 'character';
}
