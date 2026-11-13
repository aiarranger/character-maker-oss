// yukikau.jp の持ち込み（https://yukikau.jp/play?import=1）がそのまま読める .character.zip を作る。docs/yukikau-route.md
// 中身と検査は @aiarranger/character-runtime の読み込み（contract.mjs validateManifest / read.mjs）に合わせる:
// - 入れてよいのは character.json・model.glb・thumbnail.png の3つだけ
// - character.json は schemaVersion 1。決まったキーだけを書く（余分なキーははじかれる）
// - model.glb の extensionsRequired は EXT_meshopt_compression / KHR_materials_unlit / KHR_texture_transform だけ
// - clips は GLB の中で「名前が ':idle' で終わるもの → 名前が 'idle' のもの」の順に探した結果と一致させる
// - 選んだ武器は最上位の任意の項目 weapon（aisns の lib/wilds/weapons.ts と同じ id）。素手は "none"。GLB には武器を入れない
//   （weapon を受け付けるのは character-runtime 0.2.6 より後の版。0.2.6 以前の読み込みは weapon つきの ZIP をはじく）
import { zipFiles } from './zip';
import { downloadFileName, type CharacterConfig } from './config';
import { validateMakerWeaponId } from './weapon-catalog';

export const YUKIKAU_IMPORT_URL = 'https://yukikau.jp/play?import=1';
export const YUKIKAU_REFERENCE = 'https://github.com/aiarranger/character-maker-oss';
export const YUKIKAU_AUTHOR = 'AI Arranger Character Studio';
/** オリジナル4体は CC BY 4.0、プリセット（Quaternius・VRoid β の原本と派生物）は CC0 1.0。LICENSE-ASSETS.md */
export const YUKIKAU_LICENSES = {
  original: 'CC BY 4.0（Character Studio assets by AI Arranger）',
  preset: 'CC0 1.0（Quaternius・VRoid β の原本と派生物）',
} as const;
export type YukikauLicense = keyof typeof YUKIKAU_LICENSES;
/** yukikau.jp の標準の人の背丈（catalog の heightMeters）。キャラクターメイクの標準の人をこの高さに合わせる */
export const YUKIKAU_HUMAN_HEIGHT = 1.15;
const ALLOWED_EXTENSIONS = ['EXT_meshopt_compression', 'KHR_materials_unlit', 'KHR_texture_transform'];

type GltfJson = { asset?: { version?: string }; animations?: { name?: string }[]; extensionsRequired?: string[] };
export type YukikauClips = { idle: string; walk: string; run?: string };

export function readGlbJson(bytes: Uint8Array): GltfJson {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.byteLength < 28 || view.getUint32(0, true) !== 0x46546c67 || view.getUint32(4, true) !== 2) throw new Error('GLB 2.0 のモデルではありません。');
  const length = view.getUint32(12, true);
  return JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + length))) as GltfJson;
}

/** 読み込み側（contract.mjs inspectModel）と同じ探し方で、idle / walk / run の実際のクリップ名を決める */
export function yukikauClips(gltf: GltfJson): YukikauClips {
  const names = (gltf.animations ?? []).map(animation => animation.name ?? '');
  const pick = (family: string) => names.find(name => name.endsWith(`:${family}`)) ?? names.find(name => name === family);
  const idle = pick('idle'), walk = pick('walk'), run = pick('run');
  if (!idle || !walk) throw new Error('yukikau.jp 用には待機（idle）と歩く（walk）の動作が必要です。');
  return { idle, walk, ...(run ? { run } : {}) };
}

/** 0.2〜3m に収め、1mm 単位に丸める */
export function yukikauHeightMeters(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return YUKIKAU_HUMAN_HEIGHT;
  return Math.round(Math.min(3, Math.max(0.2, value)) * 1000) / 1000;
}

async function sha256(bytes: Uint8Array) {
  const digest = await crypto.subtle.digest('SHA-256', bytes as Uint8Array<ArrayBuffer>);
  return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
}

/** Studio の武器の id を character.json の weapon に移す。id は aisns の武器と同じ12種なのでそのまま。素手も "none" と書く
 *  （aisns は項目なしのキャラでは端末に残る前の武器のまま入るので、「持たない」を伝えるには none が要る）。呼び出し側が渡さないときだけ項目なし */
export function yukikauWeaponId(id: unknown): string | undefined {
  return id === undefined ? undefined : validateMakerWeaponId(id);
}

export type YukikauPackageInput = {
  name: string;
  model: ArrayBuffer | Uint8Array;
  thumbnail: Uint8Array;
  heightMeters: number;
  license: YukikauLicense;
  /** 選んだ武器の id（Studio の weapon-catalog）。素手は "none" と書く。省略なら character.json に weapon を書かない */
  weapon?: string;
};

/** <名前>.character.zip（character.json・model.glb・thumbnail.png）を作る */
export async function buildYukikauPackage(input: YukikauPackageInput): Promise<File> {
  const model = input.model instanceof Uint8Array ? input.model : new Uint8Array(input.model);
  const thumbnail = input.thumbnail;
  if (!(thumbnail[0] === 137 && thumbnail[1] === 80 && thumbnail[2] === 78 && thumbnail[3] === 71)) throw new Error('見本画像は PNG にしてください。');
  const gltf = readGlbJson(model);
  const unsupported = (gltf.extensionsRequired ?? []).filter(name => !ALLOWED_EXTENSIONS.includes(name));
  if (unsupported.length) throw new Error(`yukikau.jp が読めない形式（${unsupported.join(', ')}）がモデルに入っています。`);
  const clips = yukikauClips(gltf);
  const name = input.name.trim().slice(0, 160) || 'キャラクター';
  const modelSha = await sha256(model);
  const weapon = yukikauWeaponId(input.weapon);
  const manifest = {
    schemaVersion: 1,
    id: `maker-${modelSha.slice(0, 12)}`,
    name,
    author: YUKIKAU_AUTHOR,
    modelVersion: '1',
    model: { path: 'model.glb', sha256: modelSha, bytes: model.byteLength },
    thumbnail: { path: 'thumbnail.png', sha256: await sha256(thumbnail) },
    // 書き出す GLB は glTF 標準の +Z が正面
    coordinates: { up: '+Y', forward: '+Z', ground: 'bounds-min-y', heightMeters: yukikauHeightMeters(input.heightMeters) },
    motion: { controller: 'morph-v1', neutralMouthClose: 0, clips, expressions: [] as string[] },
    terms: {
      version: '1',
      assetLicense: YUKIKAU_LICENSES[input.license],
      aiPerformance: 'キャラクターメイク（AI Arranger Character Studio）で作った姿です。AI による会話・演技への出演は、読み込む先の利用条件に従います。',
      reference: YUKIKAU_REFERENCE,
    },
    ...(weapon ? { weapon } : {}),
  };
  const zip = await zipFiles({
    'character.json': new TextEncoder().encode(JSON.stringify(manifest, null, 2) + '\n'),
    'model.glb': model,
    'thumbnail.png': [thumbnail, { level: 0 }],
  });
  return new File([zip as Uint8Array<ArrayBuffer>], `${downloadFileName(name)}.character.zip`, { type: 'application/zip' });
}

// ---- ここから下はブラウザだけで使う（ステージの撮影・GLB の高さの測定・書き出し） ----

/** ステージの canvas（preserveDrawingBuffer）を正方形に切り出して小さな PNG にする。見つからなければ無地 */
export async function captureStageThumbnail(source: HTMLCanvasElement | null | undefined, size = 256): Promise<Uint8Array> {
  const canvas = window.document.createElement('canvas');
  canvas.width = canvas.height = size;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('見本画像を作れませんでした。');
  context.fillStyle = '#eef0f4';
  context.fillRect(0, 0, size, size);
  // Rust の下見（WebGPU）の canvas は、描いた直後の同じ処理の中でしか読めない。読む前に描き直す（components/character-maker/rust/RustMakerScene.tsx）
  (source as (HTMLCanvasElement & { __makerRedraw?: () => void }) | null | undefined)?.__makerRedraw?.();
  if (source && source.width > 0 && source.height > 0) {
    const side = Math.min(source.width, source.height);
    context.drawImage(source, (source.width - side) / 2, (source.height - side) / 2, side, side, 0, 0, size, size);
  }
  const blob = await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, 'image/png'));
  if (!blob) throw new Error('見本画像を作れませんでした。');
  return new Uint8Array(await blob.arrayBuffer());
}

/** 標準の人（大きさ・身長 1）の測った高さ。これを yukikau.jp の 1.15m に合わせる（モデル単位） */
const REFERENCE_HEIGHT = { original: 2, preset: 1.8 } as const;

/** 高さ（GLB のモデル単位）を測ってある書き出しから作る（Rust の版は Stage の glb_height で測る） */
export async function packageMeasured(name: string, model: ArrayBuffer | Uint8Array, height: number, canvas: HTMLCanvasElement | null | undefined, license: YukikauLicense, weapon: string) {
  const thumbnail = await captureStageThumbnail(canvas);
  return buildYukikauPackage({ name, model, thumbnail, license, weapon, heightMeters: YUKIKAU_HUMAN_HEIGHT * height / REFERENCE_HEIGHT[license] });
}

/** ファイルを保存させる（a[download]） */
export function downloadYukikauPackage(file: File) {
  const url = URL.createObjectURL(file), link = window.document.createElement('a');
  link.href = url; link.download = file.name; window.document.body.appendChild(link); link.click(); link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
