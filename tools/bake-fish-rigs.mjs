#!/usr/bin/env node
// Bake the upstream Fish Kit geometry and photo projections for the two Rust games.
import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { resolve, join, basename, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { runInNewContext } from 'node:vm';
import * as THREE from 'three';
import sharp from 'sharp';

const root = dirname(dirname(fileURLToPath(import.meta.url)));
if (!process.argv[2]) throw Error('Usage: node tools/bake-fish-rigs.mjs /path/to/fish-kit [--check]');
const kit = resolve(process.argv[2]);
const checkOnly = process.argv[3] === '--check';
const source = readFileSync(join(kit, 'fish-kit.js'), 'utf8');
const expected = 'baf56089ec0749760decd5cf34b9ce99e7a0052df57c7b198fbd3e23c26d2e31';
if (createHash('sha256').update(source).digest('hex') !== expected)
  throw Error('Fish Kit version differs. See tools/fish-rigs.md.');

// fillFinBases reads/writes a single Canvas ImageData. sharp supplies the same RGBA bytes without a browser.
const document = {
  createElement(tag) {
    if (tag !== 'canvas') throw Error('Unexpected DOM operation: ' + tag);
    const canvas = {};
    canvas.getContext = () => ({
      drawImage(image) {
        canvas.data = Uint8ClampedArray.from(image.data);
      },
      getImageData() {
        return { data: canvas.data };
      },
      putImageData(image) {
        canvas.data = image.data;
      }
    });
    return canvas;
  }
};
const exports =
  'GILLS,MOUTHS,HEAD_FILES,HEAD_PHOTO,FOLD_MAX,STILL,fillFinBases,fixFinEdges,addFlutterWeights,addGillCover,splitAtLip,addJawPlate,addUpperJawPlate,addJawWeights,addEyePivots,swimMaterial,paramsOf';
const js = source.replace(/^import .*;$/gm, '').replace(/^export \{[^\n]+;$/m, '');
const api = runInNewContext(js + '\n({' + exports + '})', { THREE, document });
const attributeNames = {
  POSITION: 'position',
  NORMAL: 'normal',
  TEXCOORD_0: 'uv',
  _PIV: '_piv',
  _FIN: '_fin',
  _JAW: 'jaw',
  _FLUTTER: 'flutter',
  _FINH: 'finH'
};
const components = { SCALAR: 1, VEC2: 2, VEC3: 3, VEC4: 4 };
const types = { 5126: Float32Array, 5125: Uint32Array, 5123: Uint16Array, 5121: Uint8Array };
const align = (n) => (n + 3) & ~3;
const clamp = (n, low, high) => Math.min(high, Math.max(low, n));

function model(file) {
  const b = readFileSync(join(kit, 'fish', file + '.glb'));
  const len = b.readUInt32LE(12);
  const json = JSON.parse(b.subarray(20, 20 + len));
  const bytes = b.subarray(28 + len);
  function accessor(id) {
    const a = json.accessors[id],
      v = json.bufferViews[a.bufferView],
      T = types[a.componentType];
    const size = components[a.type],
      start = v.byteOffset + (a.byteOffset || 0);
    if (v.byteStride && v.byteStride !== T.BYTES_PER_ELEMENT * size) throw Error('Unexpected interleaving');
    const raw = bytes.subarray(start, start + a.count * size * T.BYTES_PER_ELEMENT);
    return new THREE.BufferAttribute(new T(Uint8Array.from(raw).buffer), size);
  }
  const primitive = json.meshes[0].primitives[0];
  const geo = new THREE.BufferGeometry();
  for (const [key, id] of Object.entries(primitive.attributes)) geo.setAttribute(attributeNames[key], accessor(id));
  geo.setIndex(accessor(primitive.indices));
  const im = json.bufferViews[json.images[0].bufferView];
  return {
    json,
    geo,
    image: bytes.subarray(im.byteOffset, im.byteOffset + im.byteLength),
    swim: json.nodes[0].extras.swim
  };
}

async function pixels(input) {
  const {
    data,
    info: { width, height }
  } = await sharp(input).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  return { data, width, height };
}
function makeGeometry(geo, swim, texHeight, gill, mouth) {
  api.fixFinEdges(geo, texHeight);
  api.addFlutterWeights(geo);
  if (gill) api.addGillCover(geo, gill, swim.length);
  if (mouth) {
    if (mouth.close === 0) api.splitAtLip(geo, mouth);
    api.addJawPlate(geo, mouth, swim.length);
    api.addUpperJawPlate(geo, mouth, swim.length);
  }
  api.addJawWeights(geo, mouth);
}

function writeGlb(src, rig, image) {
  const { json: j, geo } = src;
  const regions = [],
    chunks = [],
    accessors = [];
  let offset = 0;
  function region(data, target) {
    const start = offset;
    chunks.push(data, Buffer.alloc(align(data.length) - data.length));
    offset += align(data.length);
    return regions.push({ buffer: 0, byteOffset: start, byteLength: data.length, ...(target ? { target } : {}) }) - 1;
  }
  function add(a, name) {
    const arr = a.array;
    const v = region(Buffer.from(arr.buffer, arr.byteOffset, arr.byteLength), name === 'indices' ? 34963 : 34962);
    const type = Object.keys(components).find((k) => components[k] === a.itemSize);
    const componentType = { Float32Array: 5126, Uint32Array: 5125, Uint16Array: 5123, Uint8Array: 5121 }[
      arr.constructor.name
    ];
    if (!componentType) throw Error('Unexpected component type: ' + arr.constructor.name);
    const record = { bufferView: v, componentType, count: a.count, type };
    if (name === 'POSITION') {
      geo.computeBoundingBox();
      record.min = geo.boundingBox.min.toArray();
      record.max = geo.boundingBox.max.toArray();
    }
    return accessors.push(record) - 1;
  }
  const prim = j.meshes[0].primitives[0];
  prim.attributes = Object.fromEntries(
    Object.entries(attributeNames).map(([key, name]) => [key, add(geo.getAttribute(name), key)])
  );
  prim.indices = add(geo.index, 'indices');
  j.images[0].bufferView = region(image);
  j.images[0].mimeType = 'image/webp';
  j.accessors = accessors;
  j.bufferViews = regions;
  j.buffers = [{ byteLength: offset }];
  j.nodes[0].extras.fishRig = rig;
  const json = Buffer.from(JSON.stringify(j));
  const len = align(json.length);
  const header = Buffer.alloc(20);
  header.write('glTF');
  header.writeUInt32LE(2, 4);
  header.writeUInt32LE(28 + len + offset, 8);
  header.writeUInt32LE(len, 12);
  header.write('JSON', 16);
  const bin = Buffer.alloc(8);
  bin.writeUInt32LE(offset);
  bin.write('BIN\0', 4);
  return Buffer.concat([header, json, Buffer.alloc(len - json.length, 32), bin, ...chunks]);
}

// Retain the original atlas for body and fins. Separate face tiles let the gill cover,
// lower jaw and eye keep their photographs when they move away from the underlying skin.
const smooth = (a, b, x) => {
  const k = clamp((x - a) / (b - a), 0, 1);
  return k * k * (3 - 2 * k);
};
function sample(im, u, v) {
  const x = clamp(u * im.width - 0.5, 0, im.width - 1),
    y = clamp(v * im.height - 0.5, 0, im.height - 1);
  const ix = Math.floor(x),
    iy = Math.floor(y),
    fx = x - ix,
    fy = y - iy,
    out = [0, 0, 0, 0];
  for (const [xx, wx] of [
    [ix, 1 - fx],
    [Math.min(ix + 1, im.width - 1), fx]
  ])
    for (const [yy, wy] of [
      [iy, 1 - fy],
      [Math.min(iy + 1, im.height - 1), fy]
    ]) {
      const off = (yy * im.width + xx) * 4;
      for (let c = 0; c < 4; c++) out[c] += im.data[off + c] * wx * wy;
    }
  return out;
}
function mixRGB(c, tex, amount) {
  for (let i = 0; i < 3; i++) c[i] += ((tex[i] - c[i]) * amount * tex[3]) / 255;
}
function lineHeight(line, x) {
  if (x <= line[0][0]) return line[0][1];
  for (let i = 1; i < line.length; i++)
    if (x <= line[i][0]) {
      const [x0, y0] = line[i - 1],
        [x1, y1] = line[i];
      return y0 + ((y1 - y0) * (x - x0)) / (x1 - x0 || 1);
    }
  return line.at(-1)[1];
}
function inside(poly, x, y) {
  let yes = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [a, b] = poly[i],
      [c, d] = poly[j];
    if (b > y !== d > y && x < ((c - a) * (y - b)) / (d - b) + a) yes = !yes;
  }
  return yes;
}
function faceAtlas(geo, body, head, cover, u, geoEye, swim, mouth) {
  const uv = geo.attributes.uv,
    pos = geo.attributes.position,
    fin = geo.attributes._fin,
    idx = geo.index;
  const shape = u.uMouthShape.value.toArray(),
    angle = u.uMouthUpper.value.w;
  const box = u.uHeadBox.value.toArray(),
    proj = u.uHeadMap.value.toArray();
  const toPhoto = (x, y) => [(proj[0] - x * proj[2] - box[0]) / box[2], (proj[1] - y * proj[2] - box[1]) / box[3]];
  function projection(part, x, y) {
    if (part === 8) {
      const dx = x - shape[0],
        dy = y - shape[1],
        c = Math.cos(angle),
        s = Math.sin(angle);
      return toPhoto(shape[0] + dx * c - dy * s, shape[1] + dx * s + dy * c);
    }
    return toPhoto(x, y);
  }
  const region = Array.from({ length: pos.count }, (_, i) => {
    const p = Math.round(fin.getX(i)),
      rim = fin.getZ(i);
    if (p >= 7 && rim > 0.5) return p === 7 ? 10 : p === 8 ? 11 : 12;
    if (p === 6 && head && geoEye && swim.photo) return p;
    if (p === 7 || p === 8) return p;
    return p === 9 ? 9 : 0;
  });
  // A chart's coordinates are in either the original UV system or the head photograph.
  // Its border supplies a guard band for bilinear filtering.
  const tiles = new Map();
  const texcoords = Array.from({ length: pos.count }, (_, i) => {
    const k = region[i];
    const photo = k === 6 || k === 7 || k === 8;
    const tex = photo ? head : body;
    const [a, b] = photo ? projection(k, pos.getX(i), pos.getY(i)) : [uv.getX(i), uv.getY(i)];
    const xy = k >= 10 ? [1, 1] : [a * tex.width, b * tex.height];
    if (!tiles.has(k)) tiles.set(k, { k, minX: Infinity, minY: Infinity, maxX: -Infinity, maxY: -Infinity });
    const t = tiles.get(k);
    t.minX = Math.min(t.minX, xy[0]);
    t.minY = Math.min(t.minY, xy[1]);
    t.maxX = Math.max(t.maxX, xy[0]);
    t.maxY = Math.max(t.maxY, xy[1]);
    return xy;
  });
  const base = tiles.get(0);
  Object.assign(base, { left: 0, top: 0, w: body.width, h: body.height, dx: 0, dy: 0 });
  const smaller = [...tiles.values()].filter((t) => t.k !== 0).sort((a, b) => a.k - b.k);
  const pad = 3;
  let left = 0,
    top = body.height + pad,
    shelf = 0;
  let width = body.width;
  for (const t of smaller) {
    t.dx = Math.floor(t.minX) - pad;
    t.dy = Math.floor(t.minY) - pad;
    t.w = Math.ceil(t.maxX) - t.dx + pad + 1;
    t.h = Math.ceil(t.maxY) - t.dy + pad + 1;
    width = Math.max(width, t.w);
  }
  for (const t of smaller) {
    if (left && left + t.w > width) {
      top += shelf;
      left = 0;
      shelf = 0;
    }
    t.left = left;
    t.top = top;
    left += t.w;
    shelf = Math.max(shelf, t.h);
  }
  const height = top + shelf;
  const data = Buffer.alloc(width * height * 4);
  for (let y = 0; y < body.height; y++)
    Buffer.from(body.data.buffer, body.data.byteOffset + y * body.width * 4, body.width * 4).copy(data, y * width * 4);
  for (const t of smaller) {
    const im = t.k === 6 || t.k === 8 ? head : t.k === 7 ? cover : body;
    for (let y = 0; y < t.h; y++)
      for (let x = 0; x < t.w; x++) {
        const start = ((t.top + y) * width + t.left + x) * 4;
        if (t.k >= 10)
          data.set(t.k === 10 ? [75, 72, 75, 255] : t.k === 11 ? [205, 196, 196, 255] : [182, 170, 166, 255], start);
        else {
          const sx = clamp(t.dx + x, 0, im.width - 1),
            sy = clamp(t.dy + y, 0, im.height - 1),
            i = (sy * im.width + sx) * 4;
          data.set(im.data.subarray(i, i + 4), start);
        }
      }
  }
  const atlasXY = texcoords.map(([a, b], i) => {
    const t = tiles.get(region[i]);
    return [t.left + a - t.dx, t.top + b - t.dy];
  });
  // Rasterize the upstream head-photo overlay and static fragment cutout into diffuse alpha.
  // Part 9 has its own copy of the surface so the body cutout never erases the upper jaw.
  for (let n = 0; n < idx.count; n += 3) {
    const is = [idx.getX(n), idx.getX(n + 1), idx.getX(n + 2)],
      part = Math.round(fin.getX(is[0]));
    if (!head || ![0, 6, 7, 8].includes(part) || region[is[0]] >= 10) continue;
    const points = is.map((i) => atlasXY[i]),
      xyz = is.map((i) => [pos.getX(i), pos.getY(i)]),
      uv0 = is.map((i) => [uv.getX(i), uv.getY(i)]);
    const [a, b, c] = points;
    const den = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
    if (Math.abs(den) < 1e-9) continue;
    const t = tiles.get(region[is[0]]);
    const min = (axis) => Math.max(axis ? t.top : t.left, Math.floor(Math.min(...points.map((p) => p[axis]))));
    const max = (axis) =>
      Math.min(axis ? t.top + t.h - 1 : t.left + t.w - 1, Math.ceil(Math.max(...points.map((p) => p[axis]))));
    const x0 = min(0),
      x1 = max(0),
      y0 = min(1),
      y1 = max(1);
    if (part === 0 && xyz.every((p) => p[0] < Math.min(u.uGillBox.value.x, shape[2]) - 0.008)) continue;
    for (let py = y0; py <= y1; py++)
      for (let px = x0; px <= x1; px++) {
        const w0 = ((b[1] - c[1]) * (px + 0.5 - c[0]) + (c[0] - b[0]) * (py + 0.5 - c[1])) / den;
        const w1 = ((c[1] - a[1]) * (px + 0.5 - c[0]) + (a[0] - c[0]) * (py + 0.5 - c[1])) / den;
        const w2 = 1 - w0 - w1;
        if (Math.min(w0, w1, w2) < -1e-5) continue;
        const interp = (attrs, k) => w0 * attrs[0][k] + w1 * attrs[1][k] + w2 * attrs[2][k];
        const x = interp(xyz, 0),
          y = interp(xyz, 1),
          [phx, phy] = projection(part, x, y);
        const out = sample(body, interp(uv0, 0), interp(uv0, 1));
        if (phx >= 0 && phx <= 1 && phy >= 0 && phy <= 1) {
          if (part === 0) {
            if (x <= u.uGillBox.value.y && sample(cover, phx, phy)[3] >= 127.5) mixRGB(out, sample(head, phx, phy), 1);
          } else if (part === 6 && geoEye && swim.photo) mixRGB(out, sample(head, phx, phy), 1);
          else if (part === 7) mixRGB(out, sample(cover, phx, phy), 1);
          else if (part === 8) mixRGB(out, sample(head, phx, phy), smooth(shape[0], shape[0] + 0.008, x));
        }
        if (part === 0 && mouth && x > shape[2] && x < shape[3]) {
          const lip = lineHeight(mouth.lowerLip, x),
            edge = mouth.close === 0 ? mouth.lowerLip : mouth.jawEdge;
          if (
            y > lip - 0.003 &&
            ((mouth.close > 0 && inside(mouth.upper, x, y)) ||
              (x > edge[0][0] && x < mouth.lowerLip.at(-1)[0] + 0.002 && y < lineHeight(edge, x)))
          )
            out[3] = 0;
        }
        data.set(out.map(Math.round), (py * width + px) * 4);
      }
  }
  atlasXY.forEach(([x, y], i) => uv.setXY(i, x / width, y / height));
  return { data, width, height };
}
async function bake(file) {
  const base = file.replace(/_lo$/, ''),
    species = base === 'suzuki' ? 'seabass' : base;
  const src = model(file),
    hi = file === base ? src : model(base),
    swim = hi.swim;
  const texture = await pixels(hi.image),
    h = texture.height,
    geo = src.geo;
  const fin = hi.geo.attributes._fin,
    uv = hi.geo.attributes.uv;
  let bodyV = 0;
  for (let i = 0; i < uv.count; i++) if (Math.round(fin.getX(i)) === 0) bodyV = Math.max(bodyV, uv.getY(i));
  const map = new THREE.Texture(texture);
  const result = api.fillFinBases(hi.geo, map, swim.length, bodyV + 4 / h).image;
  const body = { data: result.data, width: result.width, height: result.height };
  const gill = api.GILLS[species],
    mouth = api.MOUTHS[species],
    still = api.STILL.has(species);
  makeGeometry(geo, swim, h, gill, mouth);
  let eye = null;
  if (!still) {
    if (hi === src) eye = api.addEyePivots(geo);
    else {
      eye = api.addEyePivots(hi.geo);
      if (eye) api.addEyePivots(geo);
    }
  }
  const hf = api.HEAD_FILES[species],
    head = hf && gill ? await pixels(join(kit, 'fish', hf.file + '_head.webp')) : null;
  const cover = head ? await pixels(join(kit, 'fish', hf.file + '_cover.webp')) : null;
  const [x0, z0, k] = swim.photo ?? [api.HEAD_PHOTO.x0, api.HEAD_PHOTO.z0, api.HEAD_PHOTO.k];
  const foldMax = api.FOLD_MAX[species] ?? 1.1;
  const u = api.swimMaterial({
    map,
    swim,
    gill,
    mouth,
    eye,
    photos: null,
    headMap: { x0, z0, k, box: hf?.box ?? [0, 0, 1, 1] },
    foldMax
  }).uniforms;
  const { finFlutter, finFlag, finMove } = api.paramsOf(species);
  const rig = { version: 1, still, foldMax, finFlutter, finFlag, finMove, tailFlutter: false };
  for (const key of ['uMouthShape', 'uMouthUpper', 'uMouthUpper2', 'uMouthClose', 'uEyeBulge', 'uPectRest']) {
    const val = u[key].value;
    rig[key] = val.toArray ? val.toArray() : val;
  }
  const atlas = faceAtlas(geo, body, head, cover, u, eye, swim, mouth);
  const encoded = await sharp(atlas.data, { raw: { width: atlas.width, height: atlas.height, channels: 4 } })
    .webp({ quality: 95, alphaQuality: 100, effort: 4 })
    .toBuffer();
  const parts = new Set(
    Array.from({ length: geo.attributes._fin.count }, (_, i) => Math.round(geo.attributes._fin.getX(i)))
  );
  if (!still && (!parts.has(7) || !parts.has(8) || !parts.has(9))) throw Error('Missing gill or jaw: ' + file);
  console.log(
    file,
    geo.attributes.position.count + ' vertices',
    [...parts].sort().join(','),
    atlas.width + 'x' + atlas.height
  );
  return writeGlb(src, rig, encoded);
}
const dirs = ['gyogyo', 'gyogun'].map((g) => join(root, 'games', g, 'public/models/fish'));
const sets = dirs.map((dir) => readdirSync(dir).filter((f) => f.endsWith('.glb')));
for (const filename of new Set(sets.flat())) {
  const bytes = await bake(basename(filename, '.glb'));
  for (let i = 0; i < dirs.length; i++) {
    if (!sets[i].includes(filename)) continue;
    const path = join(dirs[i], filename);
    if (checkOnly) {
      if (!readFileSync(path).equals(bytes)) throw Error('Regenerated GLB differs: ' + path);
    } else {
      writeFileSync(path, bytes);
    }
  }
}
