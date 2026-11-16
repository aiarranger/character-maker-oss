#!/usr/bin/env node
/** オリジナルの人の体（服つき）を、作り直した体（Tripo で作って今の体へ重ねた GLB）へ差し替える。docs/original-remake-spec.md
 * node tools/character-maker/remake-original-body.mjs --base=<今のモデル.glb> --body=<重ねた体.glb> --out=<出力.glb> [--hand=<握った手の JSON の出力先>]
 *
 * 骨・動作・頭・顔・髪はそのまま。体のノード（extras.originalModularPart が body）の形と絵だけを替える。
 * 骨の重みは、今の体の近い頂点（4 つ）から写す。新しい体は今の体と同じ姿勢・同じ位置に重ねてあること（.snaps/blender/fit-body.py）。
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
const base = arg('base'), bodyFile = arg('body'), out = arg('out');
assert(base && bodyFile && out, 'base, body, out are required');
await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({ 'meshopt.decoder': MeshoptDecoder });
const doc = await io.read(base), bodyDoc = await io.read(bodyFile), root = doc.getRoot(), buffer = root.listBuffers()[0];
const node = root.listNodes().find(n => n.getExtras()?.originalModularPart === 'body' && n.getMesh());
assert(node, 'body node is missing');
const old = node.getMesh(), oldPrimitive = old.listPrimitives()[0], skin = node.getSkin();
assert.equal(old.listPrimitives().length, 1);
const oldP = oldPrimitive.getAttribute('POSITION').getArray(), oldJ = oldPrimitive.getAttribute('JOINTS_0').getArray(), oldW = oldPrimitive.getAttribute('WEIGHTS_0').getArray(), oldCount = oldP.length / 3;
const source = bodyDoc.getRoot().listMeshes()[0].listPrimitives()[0];
const P = Float32Array.from(source.getAttribute('POSITION').getArray()), count = P.length / 3;

// ---- 骨の重み: 今の体の近い 4 頂点から、距離の 2 乗の逆数で混ぜる（格子で近くだけ探す） ----
const CELL = 0.02, key = (x, y, z) => `${Math.floor(x / CELL)},${Math.floor(y / CELL)},${Math.floor(z / CELL)}`;
const grid = new Map();
for (let i = 0; i < oldCount; i++) { const k = key(oldP[i * 3], oldP[i * 3 + 1], oldP[i * 3 + 2]); (grid.get(k) ?? grid.set(k, []).get(k)).push(i); }
const joints = new Uint16Array(count * 4), weights = new Float32Array(count * 4);
let far = 0, maxDistance = 0;
for (let v = 0; v < count; v++) {
  const x = P[v * 3], y = P[v * 3 + 1], z = P[v * 3 + 2], cx = Math.floor(x / CELL), cy = Math.floor(y / CELL), cz = Math.floor(z / CELL);
  let found = [];
  for (let ring = 1; ring <= 12 && found.length < 4; ring++) {
    found = [];
    for (let a = -ring; a <= ring; a++) for (let b = -ring; b <= ring; b++) for (let c = -ring; c <= ring; c++) for (const i of grid.get(`${cx + a},${cy + b},${cz + c}`) ?? []) found.push([(oldP[i * 3] - x) ** 2 + (oldP[i * 3 + 1] - y) ** 2 + (oldP[i * 3 + 2] - z) ** 2, i]);
  }
  assert(found.length, `vertex ${v} has no neighbour in the current body`);
  found.sort((a, b) => a[0] - b[0]); found = found.slice(0, 4);
  const d0 = Math.sqrt(found[0][0]); maxDistance = Math.max(maxDistance, d0); if (d0 > 0.02) far++;
  const sum = new Map();
  for (const [d2, i] of found) { const w = 1 / (d2 + 1e-8); for (let k = 0; k < 4; k++) if (oldW[i * 4 + k] > 0) sum.set(oldJ[i * 4 + k], (sum.get(oldJ[i * 4 + k]) ?? 0) + w * oldW[i * 4 + k]); }
  const top = [...sum].sort((a, b) => b[1] - a[1]).slice(0, 4), total = top.reduce((t, [, w]) => t + w, 0);
  top.forEach(([j, w], k) => { joints[v * 4 + k] = j; weights[v * 4 + k] = w / total; });
}

/** 今の体の近い頂点から重みを写す（1 点） */
function weightAt(x, y, z) {
  const cx = Math.floor(x / CELL), cy = Math.floor(y / CELL), cz = Math.floor(z / CELL);
  let found = [];
  for (let ring = 1; ring <= 12 && found.length < 4; ring++) {
    found = [];
    for (let a = -ring; a <= ring; a++) for (let b = -ring; b <= ring; b++) for (let c = -ring; c <= ring; c++) for (const i of grid.get(`${cx + a},${cy + b},${cz + c}`) ?? []) found.push([(oldP[i * 3] - x) ** 2 + (oldP[i * 3 + 1] - y) ** 2 + (oldP[i * 3 + 2] - z) ** 2, i]);
  }
  found.sort((a, b) => a[0] - b[0]); found = found.slice(0, 4);
  const sum = new Map();
  for (const [d2, i] of found) { const w = 1 / (d2 + 1e-8); for (let k = 0; k < 4; k++) if (oldW[i * 4 + k] > 0) sum.set(oldJ[i * 4 + k], (sum.get(oldJ[i * 4 + k]) ?? 0) + w * oldW[i * 4 + k]); }
  const top = [...sum].sort((a, b) => b[1] - a[1]).slice(0, 4), total = top.reduce((t, [, w]) => t + w, 0);
  return top.map(([j, w]) => [j, w / total]);
}
function jpeg(bytes, size) {
  // 絵はファイルで渡す（標準入力で渡すと、まれに python が読み終わらず止まった）
  const dir = os.tmpdir(), a = path.join(dir, `remake-body-${process.pid}.png`), b = path.join(dir, `remake-body-${process.pid}.jpg`);
  writeFileSync(a, bytes);
  execFileSync('python3', ['-c', `import sys\nfrom PIL import Image\nim=Image.open(sys.argv[1]).convert('RGB')\nim.thumbnail((${size},${size}))\nim.save(sys.argv[2],quality=90)`, a, b], { timeout: 120000 });
  return readFileSync(b);
}
const primitive = doc.createPrimitive();
primitive.setAttribute('POSITION', doc.createAccessor().setType('VEC3').setArray(P).setBuffer(buffer));
for (const [semantic, type] of [['NORMAL', 'VEC3'], ['TEXCOORD_0', 'VEC2']]) primitive.setAttribute(semantic, doc.createAccessor().setType(type).setArray(Float32Array.from(source.getAttribute(semantic).getArray())).setBuffer(buffer));
primitive.setAttribute('JOINTS_0', doc.createAccessor().setType('VEC4').setArray(joints).setBuffer(buffer));
primitive.setAttribute('WEIGHTS_0', doc.createAccessor().setType('VEC4').setArray(weights).setBuffer(buffer));
primitive.setIndices(doc.createAccessor().setType('SCALAR').setArray(Uint32Array.from(source.getIndices().getArray())).setBuffer(buffer));
const oldMaterial = oldPrimitive.getMaterial(), image = source.getMaterial().getBaseColorTexture();
const texture = doc.createTexture('remade body colour').setImage(jpeg(image.getImage(), 2048)).setMimeType('image/jpeg');
primitive.setMaterial(doc.createMaterial(oldMaterial.getName()).setBaseColorTexture(texture).setMetallicFactor(0).setRoughnessFactor(1).setDoubleSided(true)
  .setExtras({ ...oldMaterial.getExtras(), design: 'Remade with Tripo H3.1 from a generated multi-view drawing; overlaid on the current body, weights copied from it' }));
const extras = { ...old.getExtras() }; delete extras.targetNames;
const mesh = doc.createMesh(old.getName()).setExtras(extras).addPrimitive(primitive);
node.setMesh(mesh);
// ---- 首のつなぎ: 頭の部品の下の縁（首の切り口）を、襟の内側へ伸ばす。新しい服は襟が開いているので、頭の首の下にすき間が見えないようにする ----
const headNode = root.listNodes().find(n => n.getName() === 'OriginalModularHead' && n.getMesh());
root.listNodes().filter(n => n.getName() === 'OriginalNeckBridge').forEach(n => { n.getMesh()?.dispose(); n.dispose(); });
let bridge = null;
if (headNode) {
  // 頭の部品（頭と、あれば前からある首のつなぎ）のいちばん下の輪を、角度ごとの外側の頂点で作る。縁の辺をたどるより、切り口が欠けていても 1 周の筒になる
  const parts = root.listNodes().filter(n => n.getMesh() && n.getSkin() && (n.getName() === 'OriginalModularHead' || /NeckBridge/.test(n.getName())));
  const points = [];
  for (const part of parts) for (const prim of part.getMesh().listPrimitives()) {
    const p = prim.getAttribute('POSITION').getArray(), j = prim.getAttribute('JOINTS_0').getArray(), w = prim.getAttribute('WEIGHTS_0').getArray(), uv = prim.getAttribute('TEXCOORD_0')?.getArray();
    for (let i = 0; i < p.length / 3; i++) points.push({ x: p[i * 3], y: p[i * 3 + 1], z: p[i * 3 + 2], joints: [...j.slice(i * 4, i * 4 + 4)], weights: [...w.slice(i * 4, i * 4 + 4)], uv: uv && part === headNode ? [uv[i * 2], uv[i * 2 + 1]] : null });
  }
  const minY = Math.min(...points.map(p => p.y)), low = points.filter(p => p.y < minY + 0.02);
  const cx = low.reduce((t, p) => t + p.x, 0) / low.length, cz = low.reduce((t, p) => t + p.z, 0) / low.length;
  const BINS = 40, ring = new Array(BINS).fill(null);
  for (const p of low) { const angle = Math.atan2(p.z - cz, p.x - cx), bin = Math.floor(((angle + Math.PI) / (2 * Math.PI)) * BINS) % BINS, radius = Math.hypot(p.x - cx, p.z - cz); if (!ring[bin] || radius > ring[bin].radius) ring[bin] = { ...p, radius }; }
  const filled = ring.filter(Boolean).length;
  if (filled >= BINS * 0.6) {
    for (let i = 0; i < BINS; i++) if (!ring[i]) { let a = i, b = i; while (!ring[(a + BINS) % BINS]) a--; while (!ring[b % BINS]) b++; const A = ring[(a + BINS) % BINS], B = ring[b % BINS], t = (i - a) / (b - a); ring[i] = { ...A, radius: A.radius + (B.radius - A.radius) * t, y: A.y + (B.y - A.y) * t }; }
    // 首の肌の色: 頭の絵の、輪の頂点の UV の所の中央値
    const image = headNode.getMesh().listPrimitives()[0].getMaterial().getBaseColorTexture(), uvs = low.filter(p => p.uv).map(p => p.uv);
    const rgb = execFileSync('python3', ['-c', `import sys,io,json\nfrom PIL import Image\nuv=json.loads(sys.argv[1])\nim=Image.open(io.BytesIO(sys.stdin.buffer.read())).convert('RGB')\nw,h=im.size\nc=[im.getpixel((min(w-1,max(0,int((u%1)*w))),min(h-1,max(0,int((v%1)*h))))) for u,v in uv]\nprint(json.dumps([sorted(x[k] for x in c)[len(c)//2] for k in range(3)]))`, JSON.stringify(uvs.slice(0, 400))], { input: image.getImage(), maxBuffer: 1 << 28 });
    const linear = JSON.parse(rgb).map(v => { const c = v / 255; return (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4) * 0.92; });
    // 前からある首のつなぎ（ナミ）も、同じ色・同じ下向きの法線にそろえる（そこだけ明るい四角に見えていた）
    for (const part of parts) if (part !== headNode) for (const prim of part.getMesh().listPrimitives()) {
      const p = prim.getAttribute('POSITION').getArray(), normal = prim.getAttribute('NORMAL'), colour = prim.getAttribute('COLOR_0');
      if (normal) { const n = normal.getArray(); for (let i = 0; i < p.length / 3; i++) { const dx = p[i * 3] - cx, dz = p[i * 3 + 2] - cz, l = Math.hypot(dx, dz) || 1; n[i * 3] = dx / l * 0.6; n[i * 3 + 1] = -0.8; n[i * 3 + 2] = dz / l * 0.6; } normal.setArray(n); }
      if (colour) { const c = colour.getArray(), size = colour.getElementSize(), max = c instanceof Float32Array ? 1 : c instanceof Uint16Array ? 65535 : 255; for (let i = 0; i < c.length; i += size) for (let k = 0; k < 3; k++) c[i + k] = linear[k] * max; colour.setArray(c); }
    }
    const DROP = 0.1, WIDEN = 1.45, topY = minY + 0.012;
    const bp = new Float32Array(BINS * 6), bn = new Float32Array(BINS * 6), bc = new Float32Array(BINS * 8), bj = new Uint16Array(BINS * 8), bw = new Float32Array(BINS * 8), bi = [];
    for (let i = 0; i < BINS; i++) {
      const angle = ((i + 0.5) / BINS) * 2 * Math.PI - Math.PI, dx = Math.cos(angle), dz = Math.sin(angle), r = ring[i].radius * 0.98;
      const top = [cx + dx * r, topY, cz + dz * r], bottom = [cx + dx * r * WIDEN, topY - DROP, cz + dz * r * WIDEN];
      bp.set([...top, ...bottom], i * 6);
      // あごの下の首と同じに見えるよう、法線は少し下向き（陰影のシェーダーで首だけ明るく浮かないように）
      bn.set([dx * 0.6, -0.8, dz * 0.6, dx * 0.6, -0.8, dz * 0.6], i * 6);
      bc.set([...linear, 1, ...linear, 1], i * 8);
      for (let k = 0; k < 4; k++) { bj[i * 8 + k] = ring[i].joints[k]; bw[i * 8 + k] = ring[i].weights[k]; }
      weightAt(...bottom).forEach(([j, w], k) => { bj[i * 8 + 4 + k] = j; bw[i * 8 + 4 + k] = w; });
      const n = (i + 1) % BINS; bi.push(i * 2, n * 2, i * 2 + 1, n * 2, n * 2 + 1, i * 2 + 1);
    }
    const bridgePrimitive = doc.createPrimitive()
      .setAttribute('POSITION', doc.createAccessor().setType('VEC3').setArray(bp).setBuffer(buffer)).setAttribute('NORMAL', doc.createAccessor().setType('VEC3').setArray(bn).setBuffer(buffer))
      .setAttribute('COLOR_0', doc.createAccessor().setType('VEC4').setArray(bc).setBuffer(buffer)).setAttribute('JOINTS_0', doc.createAccessor().setType('VEC4').setArray(bj).setBuffer(buffer))
      .setAttribute('WEIGHTS_0', doc.createAccessor().setType('VEC4').setArray(bw).setBuffer(buffer)).setIndices(doc.createAccessor().setType('SCALAR').setArray(Uint32Array.from(bi)).setBuffer(buffer))
      .setMaterial(doc.createMaterial('Original neck bridge sampled skin').setMetallicFactor(0).setRoughnessFactor(1).setDoubleSided(true).setExtras({ originalMakerHair: false, design: 'Tube from the lowest ring of the head part down inside the collar, coloured from the head texture' }));
    const bridgeExtras = { ...headNode.getExtras(), originalNeckBridge: 'v2' };
    const bridgeNode = doc.createNode('OriginalNeckBridge').setMesh(doc.createMesh('OriginalNeckBridge').addPrimitive(bridgePrimitive).setExtras(bridgeExtras)).setSkin(skin).setExtras(bridgeExtras);
    headNode.getParentNode().addChild(bridgeNode);
    bridge = { ringBins: filled, vertices: BINS * 2, drop: DROP, widen: WIDEN, colour: JSON.parse(rgb) };
  }
}
const lonely = item => item.listParents().every(parent => parent === root);
const accessors = new Set([oldPrimitive.getIndices(), ...oldPrimitive.listAttributes(), ...oldPrimitive.listTargets().flatMap(t => t.listAttributes())]);
const targets = oldPrimitive.listTargets();
oldPrimitive.dispose(); old.dispose(); for (const t of targets) t.dispose();
for (const a of accessors) if (a && lonely(a)) a.dispose();
if (lonely(oldMaterial)) { const t = oldMaterial.getBaseColorTexture(); oldMaterial.dispose(); if (t && lonely(t)) t.dispose(); }
await fs.writeFile(out, Buffer.from(await io.writeBinary(doc)));

// ---- 握った手（武器を持つとき、右手の三角形を握った形へ替える）: 右手の骨に付いた三角形の番号 ----
const hand = skin.listJoints().findIndex(n => n.getName() === 'R_Hand');
const index = primitive.getIndices().getArray(), handWeight = v => { let w = 0; for (let k = 0; k < 4; k++) if (joints[v * 4 + k] === hand) w += weights[v * 4 + k]; return w; };
const nativeTriangles = [];
for (let i = 0; i < index.length; i += 3) if (handWeight(index[i]) > 0.5 && handWeight(index[i + 1]) > 0.5 && handWeight(index[i + 2]) > 0.5) nativeTriangles.push([index[i], index[i + 1], index[i + 2]]);
// 握った手の形・色・取り付け位置は前の物（--hand-base）をそのまま使い、体の側の「元の手の三角形」だけを新しい体の番号にする
if (arg('hand')) {
  const previous = JSON.parse(await fs.readFile(arg('hand-base'), 'utf8'));
  await fs.writeFile(arg('hand'), `${JSON.stringify({ ...previous, version: 'original-closed-hand-remade-v1', source: { path: 'parts/assets/original-remake-v1 (remade body)', node: node.getName(), vertexCount: count }, nativeTriangles, cut: 'triangles of the remade body whose three vertices follow R_Hand by more than half' })}\n`);
}
console.log(JSON.stringify({ node: node.getName(), vertices: count, triangles: index.length / 3, weightSource: { vertices: oldCount, fartherThan2cm: far, maxDistance: Number(maxDistance.toFixed(4)) }, rightHandTriangles: nativeTriangles.length, neckBridge: bridge, bytes: (await fs.stat(out)).size }));
