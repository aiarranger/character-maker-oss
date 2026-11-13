// アメコミの ＞＜ の絵（今の版の lib/character-maker/preset-hurt-face.ts の comicCoverTexture・comicMarkTexture と同じ描き方）。
// canvas で描いて RGBA を Stage へ渡す。three は読まない。

/** GLB の材質の色の画像を取り出す（無ければ null） */
async function glbMaterialImage(bytes: Uint8Array, material: string): Promise<ImageBitmap | null> {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const jsonLength = view.getUint32(12, true);
  const json = JSON.parse(new TextDecoder().decode(bytes.subarray(20, 20 + jsonLength)));
  const binStart = 20 + jsonLength + 8;
  const mat = (json.materials ?? []).find((item: { name?: string }) => item.name === material);
  const texture = mat?.pbrMetallicRoughness?.baseColorTexture?.index;
  if (texture === undefined) return null;
  const image = json.images[json.textures[texture].source];
  const bufferView = json.bufferViews[image.bufferView];
  const data = bytes.subarray(binStart + (bufferView.byteOffset ?? 0), binStart + (bufferView.byteOffset ?? 0) + bufferView.byteLength);
  return createImageBitmap(new Blob([data.slice()], { type: image.mimeType }), { premultiplyAlpha: 'none', colorSpaceConversion: 'none' });
}

function pixels(canvas: HTMLCanvasElement) {
  const context = canvas.getContext('2d')!;
  return { width: canvas.width, height: canvas.height, rgba: new Uint8Array(context.getImageData(0, 0, canvas.width, canvas.height).data.buffer) };
}

/** 閉じた目：目の絵の白目と瞳を、ぼかした肌の色で覆う */
export async function comicCoverImage(bodyGlb: Uint8Array) {
  const size = 256;
  const canvas = document.createElement('canvas');
  canvas.width = size; canvas.height = size;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (!context) throw new Error('＞＜ の絵を作れませんでした。');
  const image = await glbMaterialImage(bodyGlb, 'maker-eyes');
  let skin = 'rgb(214, 140, 132)';
  if (image) {
    context.drawImage(image, 0, 0, size, size);
    const samples = [[128, 22], [128, 232], [24, 128], [232, 128]].map(([x, y]) => context.getImageData(x, y, 1, 1).data);
    skin = `rgb(${[0, 1, 2].map(k => Math.round(samples.reduce((sum, pixel) => sum + pixel[k], 0) / samples.length)).join(',')})`;
  } else {
    context.fillStyle = skin; context.fillRect(0, 0, size, size);
  }
  context.save();
  context.filter = 'blur(6px)';
  context.fillStyle = skin;
  context.beginPath(); context.ellipse(130, 120, 92, 70, 0, 0, Math.PI * 2); context.fill();
  context.restore();
  context.fillStyle = skin;
  context.beginPath(); context.ellipse(130, 120, 80, 60, 0, 0, Math.PI * 2); context.fill();
  return pixels(canvas);
}

/** ＞ の線（透明の板に描く）。左右の目で U を反転して ＞＜ にする */
export function comicMarkImage() {
  const canvas = document.createElement('canvas');
  canvas.width = 256; canvas.height = 192;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('＞＜ の絵を作れませんでした。');
  context.strokeStyle = '#2b1a17';
  context.lineWidth = 26; context.lineCap = 'round'; context.lineJoin = 'round';
  context.beginPath();
  context.moveTo(44, 26); context.lineTo(212, 96); context.lineTo(44, 166);
  context.stroke();
  return pixels(canvas);
}
