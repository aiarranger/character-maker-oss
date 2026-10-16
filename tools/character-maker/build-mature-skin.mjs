#!/usr/bin/env node
/** Deterministic cheek-only derivatives; canonical GLBs and VRM originals stay immutable. */
import fs from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';
import { NodeIO } from '@gltf-transform/core';
import { KHRMaterialsUnlit } from '@gltf-transform/extensions';
import sharp from 'sharp';

const repo = fileURLToPath(new URL('../../', import.meta.url));
const sourceDir = path.join(repo, 'parts/assets/fantasy-v1');
const outputDir = path.join(repo, 'parts/assets/mature-v1');
const expected = {
  feminine: '0cd1bb11c894c7c3295917f66faa2caa3f5360502863955584ccee6388f856a0',
  masculine: 'a342e4976f00813059f73f8338853d4f2c67488cc1d28adb3958c7449039cbbe',
};
const settings = {
  strength: .75,
  cheekCenters: [[.318, .651], [.682, .651]],
  ellipseRadius: [.125, .083],
  eyeExclusion: [.600, .619],
  lowerFade: [.700, .733],
  skinReferenceY: .738,
  referenceSampleRadiusPixels: 4,
  method: 'Reduce excess R-G and B-G cheek chroma toward adjacent unblushed skin; retain source R shading and alpha. Compact elliptical mask excludes eyes, lips, nose and ears.',
};
const sha = data => createHash('sha256').update(data).digest('hex');
const smooth = (a, b, x) => { const t = Math.max(0, Math.min(1, (x - a) / (b - a))); return t * t * (3 - 2 * t); };
const io = new NodeIO().registerExtensions([KHRMaterialsUnlit]);
await fs.mkdir(outputDir, { recursive: true });
const assets = [];
for (const base of ['feminine', 'masculine']) {
  const inputFile = path.join(sourceDir, `${base}.glb`), bytes = await fs.readFile(inputFile);
  assert.equal(sha(bytes), expected[base], `Canonical input changed: ${base}`);
  const document = await io.readBinary(bytes);
  const material = document.getRoot().listMaterials().find(m => m.getName().endsWith('Face_00_SKIN'));
  assert.ok(material, 'Face skin material missing');
  const image = material.getBaseColorTexture().getImage();
  const { data, info } = await sharp(image).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  const result = Buffer.from(data), { width, height } = info;
  const references = settings.cheekCenters.map(([u]) => {
    const sum = [0, 0, 0]; let count = 0;
    const cx = Math.round(u * (width - 1)), cy = Math.round(settings.skinReferenceY * (height - 1));
    for (let dy = -settings.referenceSampleRadiusPixels; dy <= settings.referenceSampleRadiusPixels; dy++) for (let dx = -settings.referenceSampleRadiusPixels; dx <= settings.referenceSampleRadiusPixels; dx++) {
      const offset = ((cy + dy) * width + cx + dx) * 4;
      for (let k = 0; k < 3; k++) sum[k] += data[offset + k]; count++;
    }
    return sum.map(value => value / count);
  });
  let changedPixels = 0, maximumChannelDelta = 0;
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const u = x / (width - 1), v = y / (height - 1), offset = (y * width + x) * 4;
    const side = u < .5 ? 0 : 1, [cu, cv] = settings.cheekCenters[side];
    const radius = Math.hypot((u - cu) / settings.ellipseRadius[0], (v - cv) / settings.ellipseRadius[1]);
    const mask = (1 - smooth(.35, 1, radius)) * smooth(...settings.eyeExclusion, v) * (1 - smooth(...settings.lowerFade, v));
    if (!mask) continue;
    const [r, g, b] = data.subarray(offset, offset + 3), [rr, rg, rb] = references[side];
    const redExcess = Math.max(0, (r - g) - (rr - rg));
    const blueExcess = Math.max(0, (b - g) - (rb - rg));
    const amount = mask * settings.strength;
    result[offset + 1] = Math.round(Math.min(r, g + redExcess * amount));
    result[offset + 2] = Math.round(Math.min(r, b + (redExcess - blueExcess) * amount));
    let changed = false;
    for (let k = 0; k < 3; k++) { const delta = Math.abs(result[offset + k] - data[offset + k]); changed ||= delta > 0; maximumChannelDelta = Math.max(maximumChannelDelta, delta); }
    if (changed) changedPixels++;
  }
  const png = await sharp(result, { raw: { width, height, channels: 4 } }).png({ compressionLevel: 9 }).toBuffer();
  const filename = `${base}-mature-skin.png`;
  await fs.writeFile(path.join(outputDir, filename), png);
  assert.equal(sha(await fs.readFile(inputFile)), expected[base], 'Canonical was modified');
  assets.push({ base, file: filename, publicName: `fantasy-${base}-mature-skin.png`, input: path.relative(repo, inputFile), inputSha256: expected[base], material: material.getName(), inputImageSha256: sha(image), outputSha256: sha(png), bytes: png.length, width, height, references, changedPixels, maximumChannelDelta });
}
await fs.writeFile(path.join(outputDir, 'manifest.json'), JSON.stringify({ schemaVersion: 1, version: 'mature-skin-v1', sourceLicense: 'CC0-1.0', provenance: '../../sources/fantasy-v1/provenance.json', builder: path.relative(repo, fileURLToPath(import.meta.url)), builderSha256: sha(await fs.readFile(fileURLToPath(import.meta.url))), settings, assets }, null, 2) + '\n');
console.log(JSON.stringify({ outputDir, assets }, null, 2));
