#!/usr/bin/env node
/** オリジナルの人の頭の肌の絵をならす（耳の前〜あご〜首に見える継ぎ目の線を消す）。docs/original-remake-spec.md
 * node tools/character-maker/remake-original-skin.mjs --base=<モデル.glb> --out=<出力.glb> [--keep=0.1]
 *
 * 頭の絵（材質 Modular head skin）は Tripo の細かい島に分かれていて、島の縁の白いにじみと、焼き込まれた陰の差が線に見える。
 * 肌の画素は中央の肌色へ寄せ（元の濃淡を keep だけ残す）、肌でない画素（島の間の白・灰色）は肌色で埋める。大きさ・形式・UV は変えない。
 */
import fs from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { NodeIO } from '@gltf-transform/core';
import { ALL_EXTENSIONS } from '@gltf-transform/extensions';
import { MeshoptDecoder } from 'meshoptimizer';

const arg = name => process.argv.slice(2).find(v => v.startsWith(`--${name}=`))?.slice(name.length + 3);
const base = arg('base'), out = arg('out'), keep = Number(arg('keep') ?? 0.1);
assert(base && out);
await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({ 'meshopt.decoder': MeshoptDecoder });
const doc = await io.read(base);
const find = name => { const m = doc.getRoot().listMaterials().find(item => item.getName() === name); assert(m, `${name} is missing`); return m; };
const head = find('Modular head skin'), face = find('Modular continuous face skin');
const script = `
import sys, io, json, warnings
warnings.simplefilter('ignore')
import numpy as np
from PIL import Image
keep = float(sys.argv[1]); target = json.loads(sys.argv[2])
im = Image.open(io.BytesIO(sys.stdin.buffer.read())); mode = im.mode
a = np.asarray(im.convert('RGBA')).astype(np.float32) / 255
r, g, b = a[..., 0], a[..., 1], a[..., 2]
skin = (r > 0.45) & (r > g) & (g > b) & (r - b > 0.07) & (r - b < 0.6) & (a[..., 3] > 0.5)
med = np.median(a[skin][:, :3], axis=0)
if target is None:
    sys.stdout.write(json.dumps(med.tolist())); sys.exit(0)
t = np.array(target, dtype=np.float32)
rgb = a[..., :3].copy()
rgb[skin] = np.minimum(t + keep * (rgb[skin] - med), t + 0.03)
opaque_other = (~skin) & (a[..., 3] > 0.5)
if float(sys.argv[3]) > 0: rgb[~skin] = t
o = np.concatenate([rgb, a[..., 3:]], axis=-1)
buf = io.BytesIO(); Image.fromarray((np.clip(o, 0, 1) * 255 + 0.5).astype(np.uint8)).convert(mode).save(buf, 'PNG', optimize=True)
sys.stdout.buffer.write(buf.getvalue())
`;
const python = (image, ...args) => execFileSync('python3', ['-c', script, ...args], { input: image, maxBuffer: 1 << 28 });
// 合わせ先の肌色は、顔の中央の絵（目・口のまわり。いちばん目に入る所）の中央値
const target = python(face.getBaseColorTexture().getImage(), '1', 'null', '0').toString();
// 頭（耳・首・後ろ）は濃淡を keep だけ残し、島の間は肌色で埋める。顔の中央は鼻・口の形が読めるように濃淡を多めに残し、肌でない画素（口の中など）は触らない
const done = new Set(), report = [];
for (const [material, amount, fill] of [[head, keep, '1'], [face, Math.max(keep, 0.35), '0']]) {
  const texture = material.getBaseColorTexture();
  if (done.has(texture)) continue; done.add(texture);
  const result = python(texture.getImage(), String(amount), target, fill);
  texture.setImage(result).setMimeType('image/png');
  material.setExtras({ ...material.getExtras(), remadeSkin: `shifted to the face skin colour, shading kept ${amount}` });
  report.push({ material: material.getName(), bytes: result.length });
}
// 頂点の色で塗った首のつなぎ（ナミの NagiNeckBridgeTrial）は、同じ肌色へ塗り直す
const linear = JSON.parse(target).map(c => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
for (const mesh of doc.getRoot().listMeshes()) for (const primitive of mesh.listPrimitives()) {
  const colors = primitive.getAttribute('COLOR_0');
  if (!colors || !/neck bridge/i.test(primitive.getMaterial()?.getName() ?? '')) continue;
  const array = colors.getArray(), size = colors.getElementSize(), max = array instanceof Float32Array ? 1 : array instanceof Uint16Array ? 65535 : 255;
  for (let i = 0; i < array.length; i += size) for (let k = 0; k < 3; k++) array[i + k] = linear[k] * max;
  colors.setArray(array); report.push({ material: primitive.getMaterial().getName(), vertexColours: array.length / size });
}
await fs.writeFile(out, Buffer.from(await io.writeBinary(doc)));
console.log(JSON.stringify({ target: JSON.parse(target).map(v => Math.round(v * 255)), textures: report, out }));
