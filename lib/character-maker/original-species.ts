import { BufferAttribute, BufferGeometry, CanvasTexture, Color, InterleavedBufferAttribute, Mesh, Source, SRGBColorSpace, Texture, type Object3D } from 'three';

export type OriginalSpeciesParts = {
  head: 'original' | 'rounded' | 'wide' | 'slim' | 'cheeks';
  ears: 'original' | 'rounded' | 'folded';
  eyes: 'original' | 'large' | 'narrow';
  mouth: 'original' | 'small' | 'wide';
  coatColor: string; accentColor: string; eyeColor: string;
};
export function defaultOriginalSpeciesParts(): OriginalSpeciesParts {
  return { head: 'original', ears: 'original', eyes: 'original', mouth: 'original', coatColor: 'source', accentColor: 'source', eyeColor: 'source' };
}
const choices = <T extends string>(items: readonly (readonly [T, string])[]) => items.map(([id, label]) => ({ id, label }));
export function originalSpeciesChoices(characterId: string) {
  const robot = characterId === 'generic-robot', cat = characterId === 'generic-cat', human = ['generic-human-a', 'generic-human-b'].includes(characterId);
  return {
    head: choices(robot ? [['original', 'いつもの形'], ['rounded', 'まるい頭'], ['wide', 'ワイドな頭']] as const : cat ? [['original', 'いつもの顔'], ['slim', 'すっきり'], ['cheeks', 'ふっくらほっぺ']] as const : human ? [['original', 'いつもの顔'], ['rounded', 'まるみ'], ['slim', 'すっきり']] as const : [['original', '元の形']] as const),
    ears: choices(cat ? [['original', 'いつもの耳'], ['rounded', '丸みのある耳'], ['folded', 'たれ耳']] as const : [['original', '元の形']] as const),
    eyes: choices(robot || cat || human ? [['original', 'いつもの目'], ['large', '大きめ'], ['narrow', '横長']] as const : [['original', '元の形']] as const),
    mouth: choices(robot || cat || human ? [['original', 'いつもの口'], ['small', '小さめ'], ['wide', '横に広め']] as const : [['original', '元の形']] as const),
  };
}
export function validateOriginalSpeciesParts(value: unknown, characterId: string): OriginalSpeciesParts {
  if (value === undefined) return defaultOriginalSpeciesParts();
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('顔と色の設定が正しくありません。');
  const input = value as Record<string, unknown>, allowed = originalSpeciesChoices(characterId), output = defaultOriginalSpeciesParts();
  for (const key of ['head', 'ears', 'eyes', 'mouth'] as const) {
    if (!allowed[key].some(item => item.id === input[key])) throw new Error('このキャラクターのパーツを選んでください。');
    Object.assign(output, { [key]: input[key] });
  }
  for (const key of ['coatColor', 'accentColor', 'eyeColor'] as const) {
    const color = input[key];
    if (typeof color !== 'string' || (color !== 'source' && !/^#[0-9a-f]{6}$/i.test(color))) throw new Error('パーツの色が正しくありません。');
    if (color !== 'source' && (!['generic-cat', 'generic-robot'].includes(characterId) || characterId === 'generic-robot' && key === 'eyeColor')) throw new Error('このキャラクターには、その色の設定がありません。');
    output[key] = color.toLowerCase();
  }
  return output;
}

type Mask = { version: string; width: number; height: number; runs: number[][]; softFurRuns?: number[][]; sourceSha256: string; textureSha256: string };
type LidFur = { version: string; width: number; height: number; rgb: string; weights: string; uv: number[] };
const imageCache = new WeakMap<Texture, Map<string, HTMLCanvasElement>>();
const clamp = (v: number, min = 0, max = 1) => Math.max(min, Math.min(max, v));
function privateGeometry(source: BufferGeometry) {
  const output = new BufferGeometry();
  const copy = (a: BufferAttribute | InterleavedBufferAttribute) => {
    if (!(a instanceof InterleavedBufferAttribute)) return a.clone();
    const Type = a.array.constructor as new (length: number) => BufferAttribute['array'];
    const packed = new Type(a.count * a.itemSize);
    for (let i = 0; i < a.count; i++) for (let c = 0; c < a.itemSize; c++) packed[i * a.itemSize + c] = a.array[i * a.data.stride + a.offset + c];
    return new BufferAttribute(packed, a.itemSize, a.normalized);
  };
  for (const [name, attribute] of Object.entries(source.attributes)) output.setAttribute(name, copy(attribute));
  if (source.index) output.setIndex(copy(source.index));
  for (const [name, targets] of Object.entries(source.morphAttributes)) (output.morphAttributes as Record<string, BufferAttribute[]>)[name] = targets!.map(copy);
  output.name = source.name; output.userData = structuredClone(source.userData); output.morphTargetsRelative = source.morphTargetsRelative;
  output.groups = source.groups.map(group => ({ ...group })); output.setDrawRange(source.drawRange.start, source.drawRange.count);
  return output;
}
const srgb = (color: string) => { const c = new Color(color); return [c.r, c.g, c.b].map(v => Math.round(255 * (v <= .0031308 ? 12.92 * v : 1.055 * Math.pow(v, 1 / 2.4) - .055))); };
function tint(rgb: number[], region: number, colors: (number[] | null)[], characterId: string) {
  const target = colors[region]; if (!target) return rgb;
  const light = .2126 * rgb[0] + .7152 * rgb[1] + .0722 * rgb[2];
  const normal = characterId === 'generic-robot' ? 210 : region === 2 ? 66 : region === 3 ? 224 : region === 4 ? 130 : 139;
  const shade = clamp(light / normal, .32, 1.4);
  return target.map(c => clamp((region === 3 ? 192 + c * .25 : c) * shade, 0, 255));
}
const palette = (parts: OriginalSpeciesParts) => [null, parts.coatColor, parts.accentColor, parts.coatColor, parts.eyeColor].map(c => c && c !== 'source' ? srgb(c) : null);
function lidMap(lid: LidFur, parts: OriginalSpeciesParts) {
  const rgb = Uint8Array.from(atob(lid.rgb), c => c.charCodeAt(0)), weights = Uint8Array.from(atob(lid.weights), c => c.charCodeAt(0)), colors = palette(parts);
  const canvas = document.createElement('canvas'); canvas.width = lid.width; canvas.height = lid.height;
  const context = canvas.getContext('2d'); if (!context) throw new Error('まぶたの色を準備できませんでした。');
  const image = context.createImageData(lid.width, lid.height);
  for (let i = 0; i < lid.width * lid.height; i++) {
    const source = Array.from(rgb.subarray(i * 3, i * 3 + 3)), result = [0, 0, 0];
    for (let region = 1; region <= 3; region++) { const color = tint(source, region, colors, 'generic-cat'); for (let c = 0; c < 3; c++) result[c] += color[c] * weights[i * 3 + region - 1] / 255; }
    for (let c = 0; c < 3; c++) image.data[i * 4 + c] = Math.round(result[c]); image.data[i * 4 + 3] = 255;
  }
  context.putImageData(image, 0, 0); const texture = new CanvasTexture(canvas); texture.colorSpace = SRGBColorSpace; texture.flipY = false; return texture;
}
function coloredMap(source: Texture, mask: Mask, characterId: string, parts: OriginalSpeciesParts) {
  const key = JSON.stringify([mask, characterId, parts.coatColor, parts.accentColor, parts.eyeColor]);
  let versions = imageCache.get(source); if (!versions) { versions = new Map(); imageCache.set(source, versions); }
  let canvas = versions.get(key);
  if (!canvas) {
    const input = source.image as HTMLImageElement;
    if (mask.version !== 'original-species-mask-v1' || (input.naturalWidth || input.width) !== mask.width || (input.naturalHeight || input.height) !== mask.height) throw new Error('パーツの色画像を読み込めませんでした。');
    canvas = document.createElement('canvas'); canvas.width = mask.width; canvas.height = mask.height;
    const context = canvas.getContext('2d', { willReadFrequently: true }); if (!context) throw new Error('パーツの色を準備できませんでした。');
    context.drawImage(input, 0, 0); const pixels = context.getImageData(0, 0, mask.width, mask.height);
    const colors = palette(parts), original = Uint8ClampedArray.from(pixels.data);
    for (const [start, count, region] of mask.runs) {
      const target = colors[region]; if (!target) continue;
      for (let pixel = start; pixel < start + count; pixel++) {
        const at = pixel * 4, color = tint(Array.from(original.subarray(at, at + 3)), region, colors, characterId);
        for (let c = 0; c < 3; c++) pixels.data[at + c] = Math.round(color[c]);
      }
    }
    for (const [start, count, w1, w2, w3] of mask.softFurRuns ?? []) for (let pixel = start; pixel < start + count; pixel++) {
      const at = pixel * 4, rgb = Array.from(original.subarray(at, at + 3)), result = [0, 0, 0];
      for (const [region, weight] of [[1, w1], [2, w2], [3, w3]]) {
        const color = tint(rgb, region, colors, characterId);
        for (let c = 0; c < 3; c++) result[c] += color[c] * weight / 255;
      }
      for (let c = 0; c < 3; c++) pixels.data[at + c] = Math.round(result[c]);
    }
    context.putImageData(pixels, 0, 0); versions.set(key, canvas);
    if (versions.size > 8) versions.delete(versions.keys().next().value!);
  }
  const result = source.clone(); result.source = new Source(canvas); result.needsUpdate = true; return result;
}

/** Apply once to an instance, after material conversion and BEFORE body capture.
 * Fixed builder attributes select the original regions. No segmentation runs here.
 * All position targets are rebuilt before first render with F(p+d)-F(p). */
export function configureOriginalSpecies(model: Object3D, characterId: string, value?: OriginalSpeciesParts) {
  // Legacy documents keep their authored material, including the original lids.
  if (value === undefined) return { dispose() {} };
  const parts = validateOriginalSpeciesParts(value, characterId);
  const geometries: BufferGeometry[] = [], textures: Texture[] = [];
  if (!['generic-cat', 'generic-robot'].includes(characterId)) return { dispose() {} };
  const changedShape = ['head', 'ears', 'eyes', 'mouth'].some(key => parts[key as 'head'] !== 'original');
  const changedColor = [parts.coatColor, parts.accentColor, parts.eyeColor].some(color => color !== 'source');
  if (!changedShape && !changedColor && characterId !== 'generic-cat') return { dispose() {} };
  // The display is curved: moved glyphs must stay on its surface. Use the
  // authored display triangles, including the same selected head affine map.
  const panel = model.getObjectByName('Lux_DisplayOverlay') as Mesh | undefined;
  const panelPosition = panel?.geometry.getAttribute('position'), panelIndex = panel?.geometry.index;
  const panelDepth = (y: number, z: number) => {
    if (!panelPosition || !panelIndex) return -Infinity;
    let depth = -Infinity;
    const point = (i: number) => {
      let py = panelPosition.getY(i), pz = panelPosition.getZ(i);
      if (parts.head === 'rounded') { py += (py - .705) * .15; pz *= .83; }
      if (parts.head === 'wide') { py -= (py - .705) * .12; pz *= 1.14; }
      return [panelPosition.getX(i), py, pz];
    };
    for (let i = 0; i < panelIndex.count; i += 3) {
      const a = point(panelIndex.getX(i)), b = point(panelIndex.getX(i + 1)), c = point(panelIndex.getX(i + 2));
      const den = (b[2] - c[2]) * (a[1] - c[1]) + (c[1] - b[1]) * (a[2] - c[2]);
      if (Math.abs(den) < 1e-10) continue;
      const u = ((b[2] - c[2]) * (y - c[1]) + (c[1] - b[1]) * (z - c[2])) / den;
      const v = ((c[2] - a[2]) * (y - c[1]) + (a[1] - c[1]) * (z - c[2])) / den, w = 1 - u - v;
      if (u >= -.001 && v >= -.001 && w >= -.001) depth = Math.max(depth, u * a[0] + v * b[0] + w * c[0]);
    }
    return depth;
  };
  let found = false;
  model.traverse(object => {
    if (!(object instanceof Mesh) || !object.userData.originalSpeciesPart) return;
    found = true;
    const role = object.userData.originalSpeciesPart as string;
    const coverage = object.userData.originalSpeciesLidCoverage as { version: string; basis: number[]; indices: number[]; uv: number[] | null; targets: number[][]; names: string[]; skinIndex: number[]; skinWeight: number[] } | undefined;
    if (characterId === 'generic-cat' && coverage?.version === 'moka-lid-grid-v1') {
      const geometry = new BufferGeometry(), count = coverage.basis.length / 3;
      geometry.setAttribute('position', new BufferAttribute(Float32Array.from(coverage.basis), 3));
      geometry.setIndex(coverage.indices);
      geometry.setAttribute('normal', new BufferAttribute(Float32Array.from({ length: count * 3 }, (_, i) => i % 3 === 0 ? 1 : 0), 3));
      geometry.setAttribute('skinIndex', new BufferAttribute(Uint16Array.from({ length: count * 4 }, (_, i) => coverage.skinIndex[i % 4]), 4));
      geometry.setAttribute('skinWeight', new BufferAttribute(Float32Array.from({ length: count * 4 }, (_, i) => coverage.skinWeight[i % 4]), 4));
      if (coverage.uv) geometry.setAttribute('uv', new BufferAttribute(Float32Array.from(coverage.uv), 2));
      for (const field of ['head', 'eye_l', 'eye_r', 'mouth', 'ear_l', 'ear_r']) {
        const value = field === 'head' || field === (role.endsWith('left') ? 'eye_l' : 'eye_r') ? 1 : 0;
        geometry.setAttribute(`_species_${field}`, new BufferAttribute(new Float32Array(count).fill(value), 1));
      }
      geometry.morphTargetsRelative = true;
      geometry.morphAttributes.position = coverage.targets.map(values => new BufferAttribute(Float32Array.from(values), 3));
      object.geometry = geometry; geometries.push(geometry);
      object.morphTargetDictionary = Object.fromEntries(coverage.names.map((name, i) => [name, i]));
      object.morphTargetInfluences = coverage.names.map(() => 0);
    }
    if (changedShape) {
      const source = object.geometry, geometry = privateGeometry(source); geometries.push(geometry); object.geometry = geometry;
      const position = source.getAttribute('position'), result = geometry.getAttribute('position');
      const weights = ['head', 'eye_l', 'eye_r', 'mouth', 'ear_l', 'ear_r'].map(name => source.getAttribute(`_species_${name}`));
      const transform = (x: number, y: number, z: number, vertex: number) => {
        let nx = x, ny = y, nz = z;
        const head = weights[0]?.getX(vertex) ?? 0;
        if (head && parts.head !== 'original') {
          if (characterId === 'generic-cat') {
            if (parts.head === 'slim') { nz += z * -.15 * head; ny += (y - .78) * .035 * head; }
            if (parts.head === 'cheeks') { const cheeks = Math.exp(-Math.pow((y - .747) / .085, 2)); nz += z * .19 * cheeks * head; nx += .008 * cheeks * head; }
          } else {
            const cy = .705, hy = Math.min(y, .868);
            if (parts.head === 'rounded') { nz += z * -.17 * head; ny += ((hy - cy) * .15) * head; }
            if (parts.head === 'wide') { nz += z * .14 * head; ny -= (hy - cy) * .12 * head; }
          }
        }
        for (let side = 0; side < 2; side++) {
          const eye = weights[1 + side]?.getX(vertex) ?? 0;
          if (eye && parts.eyes !== 'original') {
            const cy = characterId === 'generic-cat' ? .802 : .704, cz = (side === 0 ? 1 : -1) * (characterId === 'generic-cat' ? (side === 0 ? .0875 : .0825) : .100);
            const sx = parts.eyes === 'large' ? 1.15 : characterId === 'generic-robot' ? 1.45 : 1.25, sy = parts.eyes === 'large' ? 1.20 : characterId === 'generic-robot' ? .40 : .66;
            nz += (z - cz) * (sx - 1) * eye; ny += (y - cy) * (sy - 1) * eye;
          }
          const ear = weights[4 + side]?.getX(vertex) ?? 0;
          if (ear && parts.ears !== 'original') {
            const t = clamp((y - .874) / .126), cz = (side === 0 ? 1 : -1) * .142;
            if (parts.ears === 'rounded') { ny -= .052 * t * t * ear; nz += (z - cz) * .17 * ear; }
            if (parts.ears === 'folded') { nx += .085 * t * t * ear; ny -= .070 * t * t * ear; }
          }
        }
        const mouth = weights[3]?.getX(vertex) ?? 0;
        if (mouth && parts.mouth !== 'original') {
          const cy = characterId === 'generic-cat' ? .704 : .615;
          nz += z * (parts.mouth === 'small' ? -.28 : .27) * mouth;
          ny += (y - cy) * (parts.mouth === 'small' ? -.22 : .10) * mouth;
        }
        if (characterId === 'generic-robot' && (role.startsWith('display-eye-') && parts.eyes !== 'original' || role === 'display-mouth' && parts.mouth !== 'original')) nx = Math.max(nx, panelDepth(ny, nz) + .002);
        return [nx, ny, nz];
      };
      for (let i = 0; i < position.count; i++) {
        const point = transform(position.getX(i), position.getY(i), position.getZ(i), i); result.setXYZ(i, point[0], point[1], point[2]);
        for (let k = 0; k < (source.morphAttributes.position?.length ?? 0); k++) {
          const from = source.morphAttributes.position![k], to = geometry.morphAttributes.position![k];
          if (coverage?.version === 'moka-lid-grid-v1' && k > 0) {
            const b = k * .25, full = source.morphAttributes.position![0];
            const target = transform(position.getX(i) + b * full.getX(i) + from.getX(i), position.getY(i) + b * full.getY(i) + from.getY(i), position.getZ(i) + b * full.getZ(i) + from.getZ(i), i);
            const end = transform(position.getX(i) + full.getX(i), position.getY(i) + full.getY(i), position.getZ(i) + full.getZ(i), i);
            to.setXYZ(i, target[0] - point[0] - b * (end[0] - point[0]), target[1] - point[1] - b * (end[1] - point[1]), target[2] - point[2] - b * (end[2] - point[2]));
          } else {
            const target = transform(position.getX(i) + from.getX(i), position.getY(i) + from.getY(i), position.getZ(i) + from.getZ(i), i);
            to.setXYZ(i, target[0] - point[0], target[1] - point[1], target[2] - point[2]);
          }
        }
      }
      result.needsUpdate = true; geometry.computeVertexNormals(); geometry.computeBoundingBox(); geometry.computeBoundingSphere();
    }
    for (const material of Array.isArray(object.material) ? object.material : [object.material]) {
      const mask = material.userData.originalSpeciesColorMask as Mask | undefined;
      if (mask && 'map' in material && material.map instanceof Texture && changedColor) { material.map = coloredMap(material.map, mask, characterId, parts); textures.push(material.map); }
      if (characterId === 'generic-robot' && role.startsWith('display-') && role !== 'display-panel' && parts.accentColor !== 'source' && 'color' in material && material.color instanceof Color) material.color.set(parts.accentColor);
      const lid = object.userData.originalSpeciesLid as LidFur | undefined;
      if (characterId === 'generic-cat' && role.startsWith('lid-') && lid?.version === 'original-species-lid-v2' && 'map' in material) {
        if (!geometries.includes(object.geometry)) { object.geometry = privateGeometry(object.geometry); geometries.push(object.geometry); }
        object.geometry.setAttribute('uv', new BufferAttribute(Float32Array.from(lid.uv), 2));
        // glTF always multiplies COLOR_0, even if the preview material disables
        // vertex colors. Keep a white private attribute so export matches it.
        object.geometry.setAttribute('color', new BufferAttribute(new Float32Array(lid.uv.length / 2 * 3).fill(1), 3));
        const texture = lidMap(lid, parts); material.map = texture; material.vertexColors = false; material.needsUpdate = true; textures.push(texture);
      }
    }
    const hasFurLid = Boolean(object.userData.originalSpeciesLid);
    if (!hasFurLid && characterId === 'generic-cat' && role.startsWith('lid-') && (parts.coatColor !== 'source' || parts.accentColor !== 'source')) {
      if (!geometries.includes(object.geometry)) { object.geometry = privateGeometry(object.geometry); geometries.push(object.geometry); }
      const attribute = object.geometry.getAttribute('color'), source = attribute?.clone();
      const chosen = role === 'lid-left' ? parts.coatColor : parts.accentColor;
      if (attribute && source && chosen !== 'source') {
        const c = new Color(chosen), base = role === 'lid-left' ? [.670588, .462745, .290196] : [.294118, .258824, .239216];
        const normal = .2126 * base[0] + .7152 * base[1] + .0722 * base[2];
        for (let i = 0; i < attribute.count; i++) { const light = .2126 * source.getX(i) + .7152 * source.getY(i) + .0722 * source.getZ(i), scale = light / normal; attribute.setXYZ(i, clamp(c.r * scale), clamp(c.g * scale), clamp(c.b * scale)); }
        attribute.needsUpdate = true;
      }
    }
  });
  if (!found) throw new Error('このキャラクターの編集用パーツを読み込めませんでした。');
  return { dispose() { geometries.forEach(geometry => geometry.dispose()); textures.forEach(texture => texture.dispose()); } };
}
