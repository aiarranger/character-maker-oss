import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { EXPRESSION_CHOICES, getFaceShapes, getPresets, STYLES } from '../../../../../lib/character-maker/config';

const assetsRoot = 'parts/assets';
const assetFiles = new Map([
  'masculine.glb', 'feminine.glb',
  'hair-parted.glb', 'hair-long.glb', 'hair-buns.glb', 'hair-crop.glb', 'hair-pixie.glb',
  'masculine-motions.json', 'feminine-motions.json',
].map(file => [file, `v1/${file}`]));
for (const base of ['feminine', 'masculine']) {
  assetFiles.set(`fantasy-${base}.glb`, `fantasy-v1/${base}.glb`);
  assetFiles.set(`fantasy-${base}-motions.json`, `fantasy-v1/${base}-motions.json`);
  assetFiles.set(`fantasy-${base}-mature-skin.png`, `mature-v1/${base}-mature-skin.png`);
  assetFiles.set(`comic-${base}-sword-motions.json`, `extended-v1/${base}-sword-motions.json`);
  for (const hair of ['animeParted', 'sidePony', 'layered']) {
    assetFiles.set(`fantasy-hair-${base}-${hair}.glb`, `fantasy-v1/hair-${base}-${hair}.glb`);
  }
  for (const expression of EXPRESSION_CHOICES.filter(item => item.group === 'emotion')) {
    const name = `thumb-fantasy-${base}-expression-${expression.id}.jpg`;
    assetFiles.set(name, `thumbnails-v2/${name}`);
  }
  for (const style of STYLES) {
    const options = {
      hair: style === 'comic' ? ['parted', 'long', 'buns', 'crop', 'pixie', 'none'] : ['animeParted', 'sidePony', 'layered', 'none'],
      face: getFaceShapes(style), eyes: ['almond', 'round', 'slender'],
      mouth: ['natural', 'smile', 'full'], preset: getPresets(style).map(preset => preset.id),
    };
    for (const [kind, values] of Object.entries(options)) for (const value of values) {
      const name = `thumb-${style}-${base}-${kind}-${value}.jpg`;
      assetFiles.set(name, `thumbnails-v2/${name}`);
    }
  }
}

export async function GET(_request: Request, { params }: { params: Promise<{ file: string }> }) {
  const { file } = await params;
  const relative = assetFiles.get(file);
  if (!relative) return new Response('Not found', { status: 404 });
  try {
    const bytes = await readFile(path.join(process.cwd(), assetsRoot, relative));
    return new Response(bytes, {
      headers: {
        'Content-Type': file.endsWith('.json') ? 'application/json' : file.endsWith('.jpg') ? 'image/jpeg' : file.endsWith('.png') ? 'image/png' : 'model/gltf-binary',
        'Cache-Control': 'no-cache',
        'X-Content-Type-Options': 'nosniff',
      },
    });
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
    return new Response('Asset not found', { status: 404 });
  }
}
