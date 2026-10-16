import { MeshBasicMaterial, Texture, TextureLoader } from 'three';
import type { CharacterConfig } from './config';
import { makerAssetUrl } from './asset-url';
import manifest from './outfit-manifest.json';

type Palette = Exclude<NonNullable<CharacterConfig['outfitPalette']>, 'original'>;
const files: Record<`${CharacterConfig['style']}:${CharacterConfig['base']}`, Record<Palette, Record<string, string>>> = manifest.files;
const templates = new Map<string, Promise<Texture>>();

/** Only exact, explicitly listed clothing materials can receive a palette map. */
export function outfitPaletteFiles(config: Pick<CharacterConfig, 'style' | 'base' | 'outfitPalette'>): Readonly<Record<string, string>> {
  const palette = config.outfitPalette;
  return !palette || palette === 'original' ? {} : files[`${config.style}:${config.base}`][palette];
}

export async function loadOutfitPalette(config: CharacterConfig): Promise<ReadonlyMap<string, Texture>> {
  const entries = await Promise.all(Object.entries(outfitPaletteFiles(config)).map(async ([name, file]) => {
    let pending = templates.get(file);
    if (!pending) {
      pending = new TextureLoader().loadAsync(makerAssetUrl(file)).catch(error => {
        templates.delete(file);
        throw error;
      });
      templates.set(file, pending);
    }
    return [name, await pending] as const;
  }));
  return new Map(entries);
}

/** The caller owns this material. Source textures and cached replacement images stay immutable. */
export function applyOutfitPaletteMap(material: MeshBasicMaterial, palette: ReadonlyMap<string, Texture>): Texture | null {
  if (material.name !== 'maker-outfit' && !material.name.endsWith('_CLOTH')) return null;
  const replacement = palette.get(material.name);
  const original = material.map;
  if (!replacement || !original) return null;
  // Allocate a Source as well as a Texture: Texture.clone()/copy() shares Source.
  const map = new Texture(replacement.image);
  map.name = replacement.name || original.name;
  map.mapping = original.mapping; map.channel = original.channel;
  map.wrapS = original.wrapS; map.wrapT = original.wrapT;
  map.magFilter = original.magFilter; map.minFilter = original.minFilter; map.anisotropy = original.anisotropy;
  map.format = original.format; map.type = original.type; map.colorSpace = original.colorSpace;
  map.offset.copy(original.offset); map.repeat.copy(original.repeat); map.center.copy(original.center);
  map.rotation = original.rotation; map.matrixAutoUpdate = original.matrixAutoUpdate; map.matrix.copy(original.matrix);
  map.generateMipmaps = original.generateMipmaps; map.premultiplyAlpha = original.premultiplyAlpha;
  map.flipY = original.flipY; map.unpackAlignment = original.unpackAlignment; map.internalFormat = original.internalFormat;
  map.needsUpdate = true;
  material.map = map;
  return map;
}
