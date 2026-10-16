#!/usr/bin/env node
/** Canonical, offline character-maker assets. Source ZIPs and bone rest/bind are immutable. */
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';
import { Document, NodeIO, Accessor } from '@gltf-transform/core';
import { KHRMaterialsUnlit } from '@gltf-transform/extensions';
import { copyToDocument, prune, dedup, sparse } from '@gltf-transform/functions';
import { Matrix4, Vector3, Ray } from 'three';
import sharp from 'sharp';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const HOME = path.join(ROOT, 'parts');
const SOURCES = path.join(HOME, 'sources');
const OUT = path.join(HOME, 'assets/v1');
const io = new NodeIO().registerExtensions([KHRMaterialsUnlit]);
const sha = data => createHash('sha256').update(data).digest('hex');
const args = process.argv.slice(2);
const value = key => args.find(x => x.startsWith(`${key}=`))?.slice(key.length + 1);
const HAIRS = {
  parted: { source: 'Hair_SimpleParted', donor: 'Male' },
  long: { source: 'Hair_Long', donor: 'Female' },
  buns: { source: 'Hair_Buns', donor: 'Female' },
  crop: { source: 'Hair_Buzzed', donor: 'Male' },
  pixie: { source: 'Hair_BuzzedFemale', donor: 'Female' },
};
const PACKS = {
  base: {
    zip: 'Universal Base Characters[Standard].zip',
    prefix: 'Universal Base Characters[Standard]/',
    sha256: 'fdbf1804c90dfc1ea03e992bff7da2dfd1a79318e13270a660180f9308455f40',
    url: 'https://quaternius.itch.io/universal-base-characters',
  },
  outfits: {
    zip: 'Modular Character Outfits - Fantasy[Standard].zip',
    prefix: 'Modular Character Outfits - Fantasy[Standard]/',
    sha256: 'c3468b18871cc8c8f05ab14df7712baf22cb9f389cbd870babf130e595187f70',
    url: 'https://quaternius.itch.io/modular-character-outfits-fantasy',
  },
};
const B = 'Base Characters/Godot - UE';
const H = 'Hairstyles/Rigged to Head Bone/glTF (Godot -Unreal)';
const O = 'Exports/glTF (Godot-Unreal)/Outfits';
const LIGHT = { Male: 'T_Superhero_Male_Ligh.png', Female: 'T_Superhero_Female_Light_BaseColor.png' };
const profiles = {
  masculine: { sex: 'Male', eyeY: 1.6985, eyeX: 0.032, mouthY: 1.625, chinY: 1.587, faceZ: 0.066 },
  feminine: { sex: 'Female', eyeY: 1.6555, eyeX: 0.033, mouthY: 1.581, chinY: 1.545, faceZ: 0.064 },
};
const MORPHS = ['bodySlim', 'bodyBroad', 'faceRound', 'faceAngular', 'eyesRound', 'eyesSlender', 'mouthSmile', 'mouthFull'];
const HAIR_FITS = ['hairFitMasculine', 'hairFitFeminine'];

async function importSources(downloads) {
  // A strict allowlist imports only original files actually needed to rebuild the unlit assets.
  const selected = {
    base: ['License_Standard.txt', ...['Male', 'Female'].flatMap(sex => [
      `${B}/Superhero_${sex}_FullBody.gltf`, `${B}/Superhero_${sex}_FullBody.bin`,
      `Base Characters/Textures/${LIGHT[sex]}`,
    ]), `${B}/T_Eye_Brown.png`, `${B}/T_Hair_1_BaseColor.png`, `${B}/T_Hair_2_BaseColor.png`,
    ...Object.values(HAIRS).flatMap(h => [`${H}/${h.source}.gltf`, `${H}/${h.source}.bin`]),
    `${H}/T_Hair_1_BaseColor.png`, `${H}/T_Hair_2_BaseColor.png`],
    outfits: ['License_Standard.txt', ...['Male', 'Female'].flatMap(sex => [
      `${O}/${sex}_Peasant.gltf`, `${O}/${sex}_Peasant.bin`,
    ]), `${O}/T_Peasant_BaseColor.png`],
  };
  for (const [id, pack] of Object.entries(PACKS)) {
    const zip = path.join(downloads, pack.zip);
    assert.equal(sha(await fs.readFile(zip)), pack.sha256, `${id}: source ZIP SHA mismatch`);
    const code = `import json,zipfile,pathlib,sys\nc=json.loads(sys.stdin.read())\nz=zipfile.ZipFile(c['zip'])\nfor rel in c['files']:\n p=pathlib.Path(c['dest'])/rel\n p.parent.mkdir(parents=True,exist_ok=True)\n data=z.read(c['prefix']+rel)\n if p.exists() and p.read_bytes()!=data: raise RuntimeError('Refuse changing original source '+str(p))\n if not p.exists(): p.write_bytes(data)\n`;
    const result = spawnSync('python3', ['-c', code], { input: JSON.stringify({ zip, files: selected[id], prefix: pack.prefix, dest: path.join(SOURCES, id) }), encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  }
  const files = [];
  for (const [pack, names] of Object.entries(selected)) for (const rel of names) {
    const data = await fs.readFile(path.join(SOURCES, pack, rel));
    files.push({ path: `${pack}/${rel}`, bytes: data.length, sha256: sha(data) });
  }
  await fs.writeFile(path.join(SOURCES, 'manifest.json'), JSON.stringify({ schemaVersion: 1, license: 'CC0-1.0', author: 'Quaternius', packs: PACKS, files }, null, 2) + '\n');
}

async function verifySources() {
  const manifest = JSON.parse(await fs.readFile(path.join(SOURCES, 'manifest.json'), 'utf8'));
  for (const file of manifest.files) assert.equal(sha(await fs.readFile(path.join(SOURCES, file.path))), file.sha256, `Source changed: ${file.path}`);
}

async function readUnlit(relative, lightSex = null) {
  const file = path.join(SOURCES, relative);
  const json = JSON.parse(await fs.readFile(file, 'utf8'));
  const textures = [], images = [], resources = {};
  for (const material of json.materials ?? []) {
    const sourceIndex = material.pbrMetallicRoughness?.baseColorTexture?.index;
    const originalImage = sourceIndex !== undefined ? json.images[json.textures[sourceIndex].source] : null;
    const skin = /Superhero|Regular/.test(material.name ?? '');
    let imagePath = originalImage ? path.join(path.dirname(file), originalImage.uri) : null;
    if (lightSex && skin) imagePath = path.join(SOURCES, 'base/Base Characters/Textures', LIGHT[lightSex]);
    const pbr = { baseColorFactor: [1, 1, 1, 1], metallicFactor: 0, roughnessFactor: 1 };
    if (imagePath) {
      const uri = `base-color-${images.length}.png`;
      resources[uri] = new Uint8Array(await fs.readFile(imagePath));
      textures.push({ source: images.length }); images.push({ uri });
      pbr.baseColorTexture = { index: textures.length - 1 };
    }
    material.pbrMetallicRoughness = pbr;
    delete material.normalTexture; delete material.occlusionTexture; delete material.emissiveTexture;
    delete material.emissiveFactor;
    material.extensions = { KHR_materials_unlit: {} };
  }
  json.images = images; json.textures = textures;
  json.extensionsUsed = ['KHR_materials_unlit'];
  for (const mesh of json.meshes) for (const primitive of mesh.primitives) {
    for (const key of Object.keys(primitive.attributes)) if (!['POSITION', 'TEXCOORD_0', 'JOINTS_0', 'WEIGHTS_0'].includes(key)) delete primitive.attributes[key];
  }
  for (const buffer of json.buffers ?? []) resources[buffer.uri] = new Uint8Array(await fs.readFile(path.join(path.dirname(file), buffer.uri)));
  return io.readJSON({ json, resources });
}

function roundList(a) { return a.map(x => Math.abs(x) < 1e-10 ? 0 : Number(x.toFixed(9))); }
function skinSignature(skin) {
  return { joints: skin.listJoints().map(n => ({ name: n.getName(), matrix: n.getMatrix(), children: n.listChildren().map(c => c.getName()) })), inverseBind: Array.from(skin.getInverseBindMatrices().getArray()) };
}
function bounds(doc) {
  const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity], v = [];
  for (const node of doc.getRoot().listNodes()) if (node.getMesh()) {
    const matrix = new Matrix4().fromArray(node.getWorldMatrix());
    for (const p of node.getMesh().listPrimitives()) {
      const pos = p.getAttribute('POSITION');
      for (let i = 0; i < pos.getCount(); i++) {
        pos.getElement(i, v); const q = new Vector3(...v).applyMatrix4(matrix);
        q.toArray().forEach((x, j) => { lo[j] = Math.min(lo[j], x); hi[j] = Math.max(hi[j], x); });
      }
    }
  }
  return { min: roundList(lo), max: roundList(hi), height: hi[1] - lo[1] };
}

function collarBoundary(doc, sex) {
  const prim = doc.getRoot().listNodes().find(n => n.getName() === `${sex}_Peasant_Body`).getMesh().listPrimitives()[0];
  const pos = prim.getAttribute('POSITION'), indices = prim.getIndices().getArray(), points = new Map(), ids = [];
  for (let i = 0; i < pos.getCount(); i++) {
    const p = pos.getElement(i, []), key = p.map(v => Math.round(v * 1e5)).join(',');
    points.set(key, p); ids.push(key);
  }
  const edges = new Map();
  for (let i = 0; i < indices.length; i += 3) for (let j = 0; j < 3; j++) {
    const ends = [ids[indices[i + j]], ids[indices[i + (j + 1) % 3]]].sort(), key = ends.join('|');
    if (edges.has(key)) edges.get(key).count++;
    else edges.set(key, { ends, count: 1 });
  }
  const width = sex === 'Male' ? 0.13 : 0.10, bottom = sex === 'Male' ? 1.46 : 1.455;
  const boundary = [...edges.values()].filter(e => e.count === 1 && e.ends.every(k => {
    const [x, y] = points.get(k); return Math.abs(x) < width && y > bottom;
  })).map(e => e.ends.map(k => points.get(k)));
  assert(boundary.length >= 20, 'Original garment neck opening not found');
  return boundary;
}

function collarHeight(boundary, [x, , z]) {
  // Neck opening is star-shaped in XZ. Intersect its actual boundary segments
  // with the angular position of the skin point, retaining the front V shape.
  const angle = Math.atan2(x, z + 0.04), dx = Math.sin(angle), dz = Math.cos(angle);
  let height = null, farthest = -Infinity;
  for (const [a, b] of boundary) {
    const ax = a[0], az = a[2] + 0.04, ex = b[0] - ax, ez = b[2] - a[2];
    const cross = dx * ez - dz * ex;
    if (Math.abs(cross) < 1e-10) continue;
    const distance = (ax * ez - az * ex) / cross, t = (ax * dz - az * dx) / cross;
    if (distance >= 0 && t >= -1e-6 && t <= 1 + 1e-6 && distance > farthest) {
      farthest = distance; height = a[1] + (b[1] - a[1]) * t;
    }
  }
  assert(height !== null, 'Neck opening has an angular gap');
  return height;
}

function trimNeck(doc, prim, boundary) {
  const semantics = prim.listSemantics(), originals = new Map(), packed = new Map(), attributes = Object.fromEntries(semantics.map(s => [s, []]));
  const positions = prim.getAttribute('POSITION'), indices = prim.getIndices().getArray(), outputIndices = [];
  // Ten millimeters remain inside the shirt. Clip triangles across the boundary
  // instead of discarding crossing triangles, which could open a visible seam.
  const overlap = 0.010, field = p => p[1] - collarHeight(boundary, p) + overlap;
  function original(index) {
    if (!originals.has(index)) originals.set(index, { key: `v${index}`, attrs: Object.fromEntries(semantics.map(s => [s, prim.getAttribute(s).getElement(index, [])])) });
    return originals.get(index);
  }
  function crossing(a, b) {
    const key = [a.key, b.key].sort().join('-'), pa = a.attrs.POSITION, pb = b.attrs.POSITION;
    let lo = 0, hi = 1;
    for (let n = 0; n < 28; n++) {
      const t = (lo + hi) / 2, f = field(pa.map((v, j) => v + (pb[j] - v) * t));
      if ((f >= 0) === (field(pa) >= 0)) lo = t; else hi = t;
    }
    const t = (lo + hi) / 2, attrs = {};
    for (const s of semantics) if (!['JOINTS_0', 'WEIGHTS_0'].includes(s)) attrs[s] = a.attrs[s].map((v, j) => v + (b.attrs[s][j] - v) * t);
    const weights = new Map();
    for (const [vertex, factor] of [[a, 1 - t], [b, t]]) vertex.attrs.JOINTS_0.forEach((joint, j) => weights.set(joint, (weights.get(joint) ?? 0) + vertex.attrs.WEIGHTS_0[j] * factor));
    const strongest = [...weights].sort((a, b) => b[1] - a[1]).slice(0, 4), total = strongest.reduce((n, pair) => n + pair[1], 0);
    while (strongest.length < 4) strongest.push([0, 0]);
    attrs.JOINTS_0 = strongest.map(pair => pair[0]); attrs.WEIGHTS_0 = strongest.map(pair => pair[1] / total);
    return { key, attrs };
  }
  function pack(vertex) {
    if (!packed.has(vertex.key)) {
      packed.set(vertex.key, packed.size);
      for (const s of semantics) attributes[s].push(...vertex.attrs[s]);
    }
    return packed.get(vertex.key);
  }
  for (let i = 0; i < indices.length; i += 3) {
    const polygon = Array.from(indices.slice(i, i + 3), original), clipped = [];
    for (let j = 0; j < polygon.length; j++) {
      const a = polygon[j], b = polygon[(j + 1) % polygon.length], insideA = field(a.attrs.POSITION) >= 0, insideB = field(b.attrs.POSITION) >= 0;
      if (insideA) clipped.push(a);
      if (insideA !== insideB) clipped.push(crossing(a, b));
    }
    for (let j = 1; j + 1 < clipped.length; j++) outputIndices.push(pack(clipped[0]), pack(clipped[j]), pack(clipped[j + 1]));
  }
  for (const s of semantics) {
    const original = prim.getAttribute(s), ArrayType = original.getArray().constructor;
    prim.setAttribute(s, doc.createAccessor(`neck-cut-${s}`).setType(original.getType()).setNormalized(original.getNormalized()).setArray(new ArrayType(attributes[s])).setBuffer(doc.getRoot().listBuffers()[0]));
  }
  prim.setIndices(doc.createAccessor('neck-cut-indices').setType(Accessor.Type.SCALAR).setArray(Uint16Array.from(outputIndices)).setBuffer(doc.getRoot().listBuffers()[0]));
  return { collarEdges: boundary.length, overlapMeters: overlap, beforeVertices: positions.getCount(), afterVertices: packed.size, rule: 'Clip head/neck to the original garment neck opening minus an inside overlap; interpolate cut-edge UV and joint weights, without welding to the garment or changing skeleton.' };
}

function extractHead(doc, baseNode, boundary = null) {
  const skin = baseNode.getSkin();
  const names = skin.listJoints().map(x => x.getName());
  const prim = baseNode.getMesh().listPrimitives()[0];
  const joints = prim.getAttribute('JOINTS_0'), weights = prim.getAttribute('WEIGHTS_0');
  const choose = new Set(), j = [], w = [];
  for (let i = 0; i < joints.getCount(); i++) {
    joints.getElement(i, j); weights.getElement(i, w);
    if (j.reduce((sum, bone, k) => sum + (['Head', 'neck_01'].includes(names[bone]) ? w[k] : 0), 0) > 0.0001) choose.add(i);
  }
  const oldIndices = prim.getIndices().getArray(), retained = [];
  for (let i = 0; i < oldIndices.length; i += 3) if ([0, 1, 2].every(k => choose.has(oldIndices[i + k]))) retained.push(...oldIndices.slice(i, i + 3));
  const oldIds = [...new Set(retained)].sort((a, b) => a - b), newIds = new Map(oldIds.map((v, i) => [v, i]));
  const buffer = doc.getRoot().listBuffers()[0];
  for (const semantic of prim.listSemantics()) {
    const old = prim.getAttribute(semantic), array = old.getArray(), size = old.getElementSize();
    const replacement = new array.constructor(oldIds.length * size);
    oldIds.forEach((id, i) => replacement.set(array.subarray(id * size, (id + 1) * size), i * size));
    prim.setAttribute(semantic, doc.createAccessor(`head-${semantic}`).setType(old.getType()).setNormalized(old.getNormalized()).setArray(replacement).setBuffer(buffer));
  }
  prim.setIndices(doc.createAccessor('head-indices').setType(Accessor.Type.SCALAR).setArray(Uint16Array.from(retained.map(i => newIds.get(i)))).setBuffer(buffer));
  const neckCut = boundary ? trimNeck(doc, prim, boundary) : null;
  return { vertices: prim.getAttribute('POSITION').getCount(), triangles: prim.getIndices().getCount() / 3, rule: 'Start with triangles whose vertices have nonzero Head + neck_01 weights, then separate the neck along the actual garment opening.', ...(neckCut ? { neckCut } : {}) };
}

const smooth = (a, b, x) => { const t = Math.max(0, Math.min(1, (x - a) / (b - a))); return t * t * (3 - 2 * t); };
const gaussian = (x, radius) => Math.exp(-(x * x) / (2 * radius * radius));
function morphDelta(name, [x, y, z], profile, role) {
  if (name.startsWith('body')) {
    // A continuous modest contour field shared by every garment and the retained neck.
    // No joint origins or rest matrices move; hands, face and hair anchor remain fixed.
    const sign = name === 'bodyBroad' ? 1 : -0.75;
    const top = 1 - smooth(1.38, 1.53, y), bottom = smooth(0.20, 0.58, y);
    const core = (1 - smooth(0.18, 0.40, Math.abs(x))) * top * bottom;
    const arm = smooth(0.20, 0.33, Math.abs(x)) * (1 - smooth(0.57, 0.80, Math.abs(x))) * gaussian(y - 1.43, 0.10);
    return [x * 0.17 * core * sign, (y - 1.43) * 0.13 * arm * sign, (z + 0.03) * (0.18 * core + 0.16 * arm) * sign];
  }
  const front = smooth(0.015, 0.060, z);
  if (name.startsWith('face')) {
    if (role !== 'head') return [0, 0, 0];
    const lower = gaussian(y - (profile.chinY + 0.039), 0.038) * front * (1 - smooth(profile.eyeY - 0.019, profile.eyeY + 0.004, y));
    if (name === 'faceRound') return [x * 0.20 * lower, 0.006 * gaussian(y - profile.chinY, 0.018) * front, 0.003 * lower];
    return [-x * 0.16 * lower, -0.010 * gaussian(y - profile.chinY, 0.022) * front, -0.002 * lower];
  }
  if (name.startsWith('eyes')) {
    if (!['head', 'eyes', 'brow'].includes(role)) return [0, 0, 0];
    const cx = x < 0 ? -profile.eyeX : profile.eyeX;
    const socket = gaussian(x - cx, 0.023) * gaussian(y - profile.eyeY, 0.026) * smooth(0.025, 0.055, z);
    const sy = name === 'eyesRound' ? 0.30 : -0.25;
    const sx = name === 'eyesRound' ? 0.045 : 0.095;
    return [(x - cx) * sx * socket, (y - profile.eyeY) * sy * socket, 0];
  }
  if (name.startsWith('mouth')) {
    if (role !== 'head') return [0, 0, 0];
    const lip = gaussian(x, 0.026) * gaussian(y - profile.mouthY, 0.013) * front;
    if (name === 'mouthSmile') return [x * 0.10 * lip, 0.005 * Math.min(1, Math.abs(x) / 0.025) * lip, 0];
    return [x * 0.065 * lip, (y - profile.mouthY) * 0.36 * lip, 0.004 * lip];
  }
  return [0, 0, 0];
}

function addMorphs(doc, node, profile, role) {
  const mesh = node.getMesh(), names = role === 'head' ? MORPHS : role === 'outfit' ? MORPHS.slice(0, 2) : ['eyesRound', 'eyesSlender'];
  for (const prim of mesh.listPrimitives()) {
    const pos = prim.getAttribute('POSITION'), p = [];
    for (const name of names) {
      const data = new Float32Array(pos.getCount() * 3);
      for (let i = 0; i < pos.getCount(); i++) { pos.getElement(i, p); data.set(morphDelta(name, p, profile, role), i * 3); }
      prim.addTarget(doc.createPrimitiveTarget(name).setAttribute('POSITION', doc.createAccessor(`${node.getName()}-${name}`).setType(Accessor.Type.VEC3).setArray(data).setBuffer(doc.getRoot().listBuffers()[0])));
    }
  }
  mesh.setWeights(names.map(() => 0)).setExtras({ ...mesh.getExtras(), targetNames: names });
}

async function prepareMaterials(doc) {
  const neutral = new Set();
  const unlit = doc.createExtension(KHRMaterialsUnlit);
  for (const mat of doc.getRoot().listMaterials()) {
    mat.setExtension('KHR_materials_unlit', unlit.createUnlit());
    const original = mat.getName();
    if (/Hair|brow/.test(original)) { mat.setName('maker-brow'); if (mat.getBaseColorTexture()) neutral.add(mat.getBaseColorTexture()); }
    else if (/Superhero|Regular|skin/.test(original)) mat.setName('maker-skin');
    else if (/Eye/.test(original)) mat.setName('maker-eyes');
    else if (original !== 'maker-hair') mat.setName('maker-outfit');
    if (mat.getName() === 'maker-hair' && mat.getBaseColorTexture()) neutral.add(mat.getBaseColorTexture());
    mat.setBaseColorFactor([1, 1, 1, 1]);
  }
  for (const texture of doc.getRoot().listTextures()) {
    let pipeline = sharp(texture.getImage()).resize({ width: 1024, height: 1024, fit: 'inside', withoutEnlargement: true });
    if (neutral.has(texture)) pipeline = pipeline.greyscale().normalise();
    const image = await pipeline.png({ compressionLevel: 9, palette: false }).toBuffer();
    texture.setImage(image).setMimeType('image/png').setURI('');
  }
}

async function finish(doc, filename, contract, expectedSkin = null) {
  await prepareMaterials(doc);
  const buffer = doc.getRoot().listBuffers()[0] ?? doc.createBuffer('canonical');
  for (const accessor of doc.getRoot().listAccessors()) accessor.setBuffer(buffer);
  await doc.transform(prune({ keepAttributes: true, keepExtras: true, keepLeaves: true }), dedup(), sparse());
  if (expectedSkin) assert.deepEqual(skinSignature(doc.getRoot().listSkins()[0]), expectedSkin, 'Offline processing changed rest or inverse bind');
  // Preserve the source TRS precision, including nearly-identity rotations/scales that
  // the standard writer otherwise omits. Hair/root identities remain explicit.
  const encoded = Buffer.from(await io.writeBinary(doc));
  const jsonSize = encoded.readUInt32LE(12);
  const json = JSON.parse(encoded.subarray(20, 20 + jsonSize).toString());
  const transforms = new Map(doc.getRoot().listNodes().map(n => [n.getName(), { translation: n.getTranslation(), rotation: n.getRotation(), scale: n.getScale() }]));
  for (const node of json.nodes ?? []) if (transforms.has(node.name)) { delete node.matrix; Object.assign(node, transforms.get(node.name)); }
  const raw = Buffer.from(JSON.stringify(json));
  const padded = Buffer.alloc(Math.ceil(raw.length / 4) * 4, 0x20); raw.copy(padded);
  const tail = encoded.subarray(20 + jsonSize), header = Buffer.from(encoded.subarray(0, 20));
  header.writeUInt32LE(20 + padded.length + tail.length, 8); header.writeUInt32LE(padded.length, 12);
  const bytes = Buffer.concat([header, padded, tail]);
  const decoded = await io.readBinary(bytes);
  if (expectedSkin) {
    const actual = skinSignature(decoded.getRoot().listSkins()[0]);
    assert.equal(actual.joints.length, 65);
    // glTF writing omits numerically identity TRS components; compare with a tight numerical tolerance.
    for (let i = 0; i < actual.joints.length; i++) {
      assert.equal(actual.joints[i].name, expectedSkin.joints[i].name);
      actual.joints[i].matrix.forEach((x, k) => assert(Math.abs(x - expectedSkin.joints[i].matrix[k]) < 1e-7));
    }
    assert.deepEqual(actual.inverseBind, expectedSkin.inverseBind);
  }
  for (const texture of decoded.getRoot().listTextures()) { const m = await sharp(texture.getImage()).metadata(); assert(m.width <= 1024 && m.height <= 1024); }
  assert(bytes.length <= 25 * 1024 * 1024);
  await fs.writeFile(path.join(OUT, filename), bytes);
  const entry = { file: filename, sha256: sha(bytes), bytes: bytes.length, ...contract, bounds: bounds(decoded), materials: decoded.getRoot().listMaterials().map(m => m.getName()), meshes: decoded.getRoot().listMeshes().map(m => ({ name: m.getName(), targets: m.getExtras().targetNames ?? [], vertices: m.listPrimitives().reduce((n, p) => n + p.getAttribute('POSITION').getCount(), 0) })) };
  console.log(JSON.stringify({ file: filename, bytes: bytes.length, height: entry.bounds.height }));
  return entry;
}

async function buildBody(id, profile) {
  const doc = await readUnlit(`outfits/${O}/${profile.sex}_Peasant.gltf`, profile.sex);
  const base = await readUnlit(`base/${B}/Superhero_${profile.sex}_FullBody.gltf`, profile.sex);
  const skin = doc.getRoot().listSkins()[0], originalSkin = skinSignature(skin);
  const baseSkin = base.getRoot().listSkins()[0];
  assert.deepEqual(baseSkin.listJoints().map(n => n.getName()), skin.listJoints().map(n => n.getName()));
  const checks = ['root', 'pelvis', 'spine_01', 'spine_02', 'spine_03', 'neck_01', 'Head'];
  for (let i = 0; i < skin.listJoints().length; i++) if (checks.includes(skin.listJoints()[i].getName())) assert.deepEqual(baseSkin.getInverseBindMatrices().getElement(i, []), skin.getInverseBindMatrices().getElement(i, []));
  const sourceHead = base.getRoot().listNodes().find(n => /SuperHero_Male|Superhero_Female/.test(n.getName()));
  assert(sourceHead?.getMesh(), 'Source body not found');
  const extraction = extractHead(base, sourceHead, collarBoundary(doc, profile.sex));
  const armature = doc.getRoot().listScenes()[0].listChildren()[0];
  armature.setName('maker-root');
  for (const node of [...doc.getRoot().listNodes()]) if (node.getMesh()) {
    const part = node.getName().split('_').at(-1).toLowerCase(); node.setName(`maker-${part}`); node.getMesh().setName(`maker-${part}`);
    addMorphs(doc, node, profile, 'outfit');
  }
  for (const [source, name, role] of [[sourceHead, 'maker-head', 'head'], [base.getRoot().listNodes().find(n => n.getName() === 'Eyes'), 'maker-eyes', 'eyes'], [base.getRoot().listNodes().find(n => n.getName() === 'Eyebrows'), 'maker-brow', 'brow']]) {
    const map = copyToDocument(doc, base, [source.getMesh()]);
    const mesh = map.get(source.getMesh()).setName(name);
    const node = doc.createNode(name).setMesh(mesh).setSkin(skin).setMatrix(source.getMatrix());
    armature.addChild(node); addMorphs(doc, node, profile, role);
  }
  doc.getRoot().setExtras({ characterMaker: { schemaVersion: 1, base: id, front: '+Z', up: '+Y', unit: 'meter', morphology: 'appearance', hairAnchor: 'Head' } });
  const head = skin.listJoints().find(n => n.getName() === 'Head');
  return finish(doc, `${id}.glb`, { kind: 'body', base: id, skeleton: { bones: 65, root: 'maker-root', jointRoot: 'root', head: 'Head', headLocalMatrix: roundList(head.getMatrix()), headWorldMatrix: roundList(head.getWorldMatrix()), rootMatrix: roundList(armature.getMatrix()), signatureSha256: sha(JSON.stringify(originalSkin)) }, sourceHeadExtraction: extraction, landmarks: profile, front: '+Z', up: '+Y', unit: 'meter' }, originalSkin);
}

// Transfer the donor hairstyle's scalp clearance to the other original head.
// The radial cage is derived from actual head triangles, not a guessed sex scale.
// This keeps the donor neutral shape exact and avoids moving bones or hiding skin.
const scalpOrigin = new Vector3(0, 0.14, -0.012);
const scalpSurfaces = new Map();
async function scalpSurface(sex) {
  if (scalpSurfaces.has(sex)) return scalpSurfaces.get(sex);
  const doc = await readUnlit(`base/${B}/Superhero_${sex}_FullBody.gltf`, sex);
  const node = doc.getRoot().listNodes().find(n => /SuperHero_Male|Superhero_Female/.test(n.getName()));
  extractHead(doc, node);
  const head = node.getSkin().listJoints().find(n => n.getName() === 'Head');
  const toHead = new Matrix4().fromArray(head.getWorldMatrix()).invert().multiply(new Matrix4().fromArray(node.getWorldMatrix()));
  const triangles = [];
  for (const prim of node.getMesh().listPrimitives()) {
    const pos = prim.getAttribute('POSITION'), indices = prim.getIndices().getArray();
    const points = Array.from({ length: pos.getCount() }, (_, i) => new Vector3(...pos.getElement(i, [])).applyMatrix4(toHead));
    for (let i = 0; i < indices.length; i += 3) triangles.push([points[indices[i]], points[indices[i + 1]], points[indices[i + 2]]]);
  }
  const surface = { triangles };
  scalpSurfaces.set(sex, surface);
  return surface;
}

function surfaceRadius(surface, direction) {
  const ray = new Ray(scalpOrigin, direction), hit = new Vector3();
  let radius = 0;
  // Rays toward a hairstyle's internal cap can leave the head through the neck
  // and hit a shoulder. Such intersections are not scalp correspondences.
  for (const triangle of surface.triangles) if (ray.intersectTriangle(...triangle, false, hit) && hit.y > 0.045) radius = Math.max(radius, hit.distanceTo(scalpOrigin));
  return radius;
}

async function fitHair(doc, mesh, donor) {
  const from = await scalpSurface(donor);
  const targetSex = donor === 'Male' ? 'Female' : 'Male';
  const to = await scalpSurface(targetSex);
  let changed = 0, maximumShift = 0, maximumShiftVertex = null;
  for (const prim of mesh.listPrimitives()) {
    const pos = prim.getAttribute('POSITION'), data = new Float32Array(pos.getCount() * 3);
    for (let i = 0; i < pos.getCount(); i++) {
      const p = new Vector3(...pos.getElement(i, [])), direction = p.clone().sub(scalpOrigin);
      const radius = direction.length(); direction.normalize();
      const sourceRadius = surfaceRadius(from, direction), targetRadius = surfaceRadius(to, direction);
      if (!sourceRadius || !targetRadius) continue;
      // Carry scalp-adjacent hair with the measured surface; let detached lengths
      // return smoothly to the original silhouette and keep the long hair tips.
      const scalpDistance = Math.abs(radius - sourceRadius);
      const lowerScalpY = scalpOrigin.y + direction.y * (direction.y < 0 ? Math.max(sourceRadius, targetRadius) : Math.min(sourceRadius, targetRadius));
      const falloff = smooth(0.045, 0.085, lowerScalpY) * (1 - smooth(0.045, 0.16, scalpDistance));
      const shift = (targetRadius - sourceRadius) * falloff;
      if (Math.abs(shift) > 1e-7) changed++;
      if (Math.abs(shift) > maximumShift) {
        maximumShift = Math.abs(shift);
        maximumShiftVertex = { vertex: i, headLocalPosition: p.toArray(), sourceScalpRadius: sourceRadius, targetScalpRadius: targetRadius, lowerScalpY, scalpDistance, falloff, delta: direction.clone().multiplyScalar(shift).toArray() };
      }
      data.set(direction.multiplyScalar(shift).toArray(), i * 3);
    }
    for (const [j, name] of HAIR_FITS.entries()) {
      const sex = j === 0 ? 'Male' : 'Female';
      prim.addTarget(doc.createPrimitiveTarget(name).setAttribute('POSITION', doc.createAccessor(`${mesh.getName()}-${name}`).setType(Accessor.Type.VEC3).setArray(sex === donor ? new Float32Array(data.length) : data).setBuffer(doc.getRoot().listBuffers()[0])));
    }
  }
  mesh.setWeights([0, 0]).setExtras({ ...mesh.getExtras(), targetNames: HAIR_FITS });
  return { targets: HAIR_FITS, selectedBaseWeight: 1, otherBaseWeight: 0, donorShapeUnchanged: true, method: 'Head-local radial transfer of original donor-to-target scalp surface difference, smoothly reduced by absolute scalp distance for both internal caps and detached lengths/tips', origin: scalpOrigin.toArray(), changedVertices: changed, maximumShiftMeters: maximumShift, maximumShiftVertex };
}

async function buildHair(id, config) {
  const source = await readUnlit(`base/${H}/${config.source}.gltf`);
  const sourceNode = source.getRoot().listNodes().find(n => n.getMesh());
  const skin = sourceNode.getSkin(), names = skin.listJoints().map(n => n.getName()), headIndex = names.indexOf('Head');
  const head = skin.listJoints()[headIndex];
  const headInverse = new Matrix4().fromArray(head.getWorldMatrix()).invert();
  const inverseBinds = skin.getInverseBindMatrices();
  const jointWorld = skin.listJoints().map(n => new Matrix4().fromArray(n.getWorldMatrix()));
  const doc = new Document(), scene = doc.createScene('Hair');
  const map = copyToDocument(doc, source, [sourceNode.getMesh()]);
  const mesh = map.get(sourceNode.getMesh()).setName(`hair-${id}`), node = doc.createNode(`hair-${id}`).setMesh(mesh);
  scene.addChild(node);
  for (const prim of mesh.listPrimitives()) {
    const pos = prim.getAttribute('POSITION'), joints = prim.getAttribute('JOINTS_0'), weights = prim.getAttribute('WEIGHTS_0');
    const data = new Float32Array(pos.getCount() * 3), p = [], js = [], ws = [];
    for (let i = 0; i < pos.getCount(); i++) {
      pos.getElement(i, p); joints.getElement(i, js); weights.getElement(i, ws);
      const baked = new Vector3();
      for (let k = 0; k < 4; k++) if (ws[k] > 0) {
        assert.equal(names[js[k]], 'Head', 'Hair has non-head weights; needs explicit design');
        const bind = new Matrix4().fromArray(inverseBinds.getElement(js[k], []));
        baked.addScaledVector(new Vector3(...p).applyMatrix4(bind).applyMatrix4(jointWorld[js[k]]), ws[k]);
      }
      baked.applyMatrix4(headInverse); data.set(baked.toArray(), i * 3);
    }
    prim.setAttribute('POSITION', doc.createAccessor(`${id}-head-local`).setType(Accessor.Type.VEC3).setArray(data).setBuffer(doc.getRoot().listBuffers()[0]));
    prim.setAttribute('JOINTS_0', null).setAttribute('WEIGHTS_0', null);
    prim.getMaterial().setName('maker-hair');
  }
  const fit = await fitHair(doc, mesh, config.donor);
  doc.getRoot().setExtras({ characterMaker: { schemaVersion: 1, hair: id, attachTo: 'Head', coordinateSpace: 'Head-local', donor: config.donor } });
  return finish(doc, `hair-${id}.glb`, { kind: 'hair', hair: id, attachTo: 'Head', coordinateSpace: 'Head-local', rootMatrix: new Matrix4().toArray(), donor: config.donor, donorHeadWorldMatrix: roundList(head.getWorldMatrix()), fit, texture: 'neutral grayscale, white material factor; tint maker-hair at runtime' });
}

if (args.includes('--import')) await importSources(value('--downloads') ?? path.join(process.env.HOME, 'Downloads'));
await verifySources();
await fs.mkdir(OUT, { recursive: true });
let previous = { assets: [] };
try { previous = JSON.parse(await fs.readFile(path.join(OUT, 'manifest.json'), 'utf8')); } catch {}
const entries = new Map(previous.assets.map(x => [x.file, x]));
if (!args.includes('--skip-bodies')) for (const [id, profile] of Object.entries(profiles)) entries.set(`${id}.glb`, await buildBody(id, profile));
for (const id of (value('--hair') ?? Object.keys(HAIRS).join(',')).split(',')) { assert(HAIRS[id], `Unknown hairstyle ${id}`); entries.set(`hair-${id}.glb`, await buildHair(id, HAIRS[id])); }
const manifest = { schemaVersion: 1, version: 'v1', sourceLicense: 'CC0-1.0', builder: path.relative(ROOT, fileURLToPath(import.meta.url)), builderSha256: sha(await fs.readFile(fileURLToPath(import.meta.url))), sourceManifestSha256: sha(await fs.readFile(path.join(SOURCES, 'manifest.json'))), morphNames: MORPHS, hairFitMorphNames: HAIR_FITS, assets: [...entries.values()].sort((a, b) => a.file.localeCompare(b.file)) };
await fs.writeFile(path.join(OUT, 'manifest.json'), JSON.stringify(manifest, null, 2) + '\n');
