#!/usr/bin/env node
/** オリジナルの人の髪型 1 つを、作り直した髪（Tripo で作って頭へ合わせた GLB）へ差し替える。docs/original-remake-spec.md
 * node tools/character-maker/remake-original-hair.mjs --base=<今のモデル.glb> --hairstyle=haru-short-v2 --hair=<合わせた髪.glb> --out=<出力.glb>
 *
 * 髪のノード（OriginalModularHair_<髪型>）の名前・目印（extras）・骨はそのまま、形と材質だけを替える。頭の骨（Head）に 1.0 で付ける。
 * 合わせた髪の GLB は、メッシュ 2 つ（髪・髪の土台）で、今のモデルと同じ座標（+X が正面、Y が上）。
 */
import fs from 'node:fs/promises';
import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import os from 'node:os';
import path from 'node:path';
import assert from 'node:assert/strict';
import { NodeIO } from '@gltf-transform/core';
import { ALL_EXTENSIONS } from '@gltf-transform/extensions';
import { MeshoptDecoder } from 'meshoptimizer';

const arg = name => process.argv.slice(2).find(v => v.startsWith(`--${name}=`))?.slice(name.length + 3);
const base = arg('base'), hairstyle = arg('hairstyle'), hairFile = arg('hair'), out = arg('out');
assert(base && hairstyle && hairFile && out, 'base, hairstyle, hair, out are required');
await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({ 'meshopt.decoder': MeshoptDecoder });
const doc = await io.read(base), hairDoc = await io.read(hairFile), root = doc.getRoot();
const node = root.listNodes().find(n => n.getExtras()?.originalModularPart === 'hair' && n.getExtras()?.hairstyleId === hairstyle);
assert(node, `${hairstyle}: hair node is missing`);
const skin = node.getSkin(), head = skin.listJoints().findIndex(n => n.getName() === 'Head'), buffer = root.listBuffers()[0];
assert(head >= 0);
const old = node.getMesh(), oldMaterial = old.listPrimitives()[0].getMaterial();
const color = oldMaterial.getExtras()?.defaultHairColor ?? old.listPrimitives().map(p => p.getMaterial().getExtras()?.defaultHairColor).find(Boolean);
assert(color, 'defaultHairColor is missing');
/** 絵を JPEG にする（sharp が使えない環境のため PIL） */
function jpeg(bytes, size) {
  // 絵はファイルで渡す（標準入力で渡すと、まれに python が読み終わらず止まった）
  const dir = os.tmpdir(), a = path.join(dir, `remake-hair-${process.pid}.png`), b = path.join(dir, `remake-hair-${process.pid}.jpg`);
  writeFileSync(a, bytes);
  execFileSync('python3', ['-c', `import sys\nfrom PIL import Image\nim=Image.open(sys.argv[1]).convert('RGB')\nim.thumbnail((${size},${size}))\nim.save(sys.argv[2],quality=90)`, a, b], { timeout: 120000 });
  return readFileSync(b);
}
// 部分が 2 つ以上のメッシュは、Stage がメッシュ側の目印（extras）を読む。前の髪のメッシュの目印をそのまま引き継ぐ
const mesh = doc.createMesh(old.getName()).setExtras({ ...node.getExtras(), ...old.getExtras() });
const parts = hairDoc.getRoot().listNodes().filter(n => n.getMesh()).map(n => [n.getName(), n.getMesh().listPrimitives()[0]]);
assert.equal(parts.length, 2, 'hair GLB must hold the hair and its cap');
for (const [name, source] of parts) {
  const count = source.getAttribute('POSITION').getCount(), cap = /Cap/.test(name), primitive = doc.createPrimitive();
  for (const semantic of ['POSITION', 'NORMAL', 'TEXCOORD_0']) { const from = source.getAttribute(semantic); primitive.setAttribute(semantic, doc.createAccessor().setType(from.getType()).setArray(Float32Array.from(from.getArray())).setBuffer(buffer)); }
  primitive.setIndices(doc.createAccessor().setType('SCALAR').setArray(Uint32Array.from(source.getIndices().getArray())).setBuffer(buffer));
  const joints = new Uint16Array(count * 4), weights = new Float32Array(count * 4);
  for (let i = 0; i < count; i++) { joints[i * 4] = head; weights[i * 4] = 1; }
  primitive.setAttribute('JOINTS_0', doc.createAccessor().setType('VEC4').setArray(joints).setBuffer(buffer));
  primitive.setAttribute('WEIGHTS_0', doc.createAccessor().setType('VEC4').setArray(weights).setBuffer(buffer));
  const image = source.getMaterial().getBaseColorTexture();
  const texture = doc.createTexture(`${hairstyle} remade ${cap ? 'cap' : 'hair'}`);
  if (cap) texture.setImage(image.getImage()).setMimeType(image.getMimeType()); else texture.setImage(jpeg(image.getImage(), 1024)).setMimeType('image/jpeg');
  primitive.setMaterial(doc.createMaterial(cap ? `maker-original-hair-remade-cap-${hairstyle}` : `Modular_Hair_${hairstyle}`).setBaseColorTexture(texture)
    .setExtras({ originalMakerHair: true, originalMakerHairStyle: hairstyle, defaultHairColor: color, design: cap ? 'Scalp-fitted cap under the remade hair (prevents skin showing between strands)' : 'Remade with Tripo H3.1 from a generated multi-view drawing; fitted to the current head' })
    .setDoubleSided(true).setMetallicFactor(0).setRoughnessFactor(1));
  mesh.addPrimitive(primitive);
}
node.setMesh(mesh);
// 前の髪の形・材質・絵は、ほかで使われていなければ消す
const lonely = item => item.listParents().every(parent => parent === root);
const materials = new Set(), accessors = new Set();
for (const primitive of old.listPrimitives()) { materials.add(primitive.getMaterial()); for (const a of [primitive.getIndices(), ...primitive.listAttributes()]) accessors.add(a); primitive.dispose(); }
old.dispose();
for (const a of accessors) if (a && lonely(a)) a.dispose();
for (const m of materials) { if (!m || !lonely(m)) continue; const textures = [m.getBaseColorTexture(), m.getNormalTexture(), m.getMetallicRoughnessTexture(), m.getOcclusionTexture(), m.getEmissiveTexture()]; m.dispose(); for (const t of textures) if (t && lonely(t)) t.dispose(); }
await fs.writeFile(out, Buffer.from(await io.writeBinary(doc)));
console.log(JSON.stringify({ hairstyle, primitives: mesh.listPrimitives().map(p => ({ material: p.getMaterial().getName(), vertices: p.getAttribute('POSITION').getCount(), triangles: p.getIndices().getCount() / 3 })), bytes: (await fs.stat(out)).size }));
