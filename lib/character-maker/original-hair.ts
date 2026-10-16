import { Mesh, Source, Texture, type MeshBasicMaterial, type Object3D } from 'three';
import { currentOriginalAppearance, type OriginalAppearance } from './original-config';
import { applyOriginalSkinPatch } from './original-skin-patch.mjs';

const DEFAULT_COLORS: Record<string, string> = {
  'generic-human-a': '#383338', 'generic-human-b': '#654b40',
  'generic-robot': '#4a737b', 'generic-cat': '#87614b',
};
const grayscaleImages = new WeakMap<Texture, HTMLCanvasElement>();
const correctedSkinImages = new WeakMap<Texture, Map<string, HTMLCanvasElement>>();

/** The builder pins this small atlas correction to its original source. Only
 * replacement hairstyles use it; the original face map and geometry stay intact. */
function correctedSkinMap(source: Texture, profile: Parameters<typeof applyOriginalSkinPatch>[3]): Texture {
  const identity = JSON.stringify(profile);
  let versions = correctedSkinImages.get(source);
  if (!versions) { versions = new Map(); correctedSkinImages.set(source, versions); }
  let canvas = versions.get(identity);
  if (!canvas) {
    const input = source.image as HTMLImageElement;
    canvas = document.createElement('canvas');
    canvas.width = input.naturalWidth || input.width;
    canvas.height = input.naturalHeight || input.height;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (!context || !canvas.width || !canvas.height) throw new Error('髪型に合わせた顔の画像を読み込めませんでした。');
    context.drawImage(input, 0, 0);
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
    pixels.data.set(applyOriginalSkinPatch(pixels.data, canvas.width, canvas.height, profile));
    context.putImageData(pixels, 0, 0);
    versions.set(identity, canvas);
  }
  const texture = source.clone();
  texture.source = new Source(canvas);
  texture.needsUpdate = true;
  return texture;
}

/** A colorable copy of the source hair map. It is never assigned to skin or
 * clothes, and the source texture/image remains immutable in the loader cache. */
function colorableMap(source: Texture): Texture {
  let canvas = grayscaleImages.get(source);
  if (!canvas) {
    const input = source.image as HTMLImageElement;
    canvas = document.createElement('canvas');
    canvas.width = input.naturalWidth || input.width;
    canvas.height = input.naturalHeight || input.height;
    const context = canvas.getContext('2d', { willReadFrequently: true });
    if (!context || !canvas.width || !canvas.height) throw new Error('髪色の画像を読み込めませんでした。');
    context.drawImage(input, 0, 0);
    const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
    for (let index = 0; index < pixels.data.length; index += 4) {
      const value = .2126 * pixels.data[index] + .7152 * pixels.data[index + 1] + .0722 * pixels.data[index + 2];
      // Preserve strand shading while allowing black/brown source hair to take
      // light colors. Alpha and UV layout are kept byte-for-byte.
      const neutral = Math.round(255 * Math.min(1, .47 + .72 * value / 255));
      pixels.data[index] = pixels.data[index + 1] = pixels.data[index + 2] = neutral;
    }
    context.putImageData(pixels, 0, 0);
    grayscaleImages.set(source, canvas);
  }
  const texture = source.clone();
  // Texture.clone shares its Source. Detach it before assigning image data or
  // recoloring hair would also overwrite the original skin/clothing atlas.
  texture.source = new Source(canvas);
  texture.needsUpdate = true;
  return texture;
}

/** Apply the same fixed builder package to a private preview/export instance.
 * Unselected parts are detached, so a standard GLB consumer sees only the saved
 * hairstyle even though glTF has no standard mesh visibility property. */
export function configureOriginalHair(model: Object3D, characterId: string, appearance: OriginalAppearance) {
  appearance = currentOriginalAppearance(characterId, appearance);
  const remove: Object3D[] = [], ownedTextures: Texture[] = [];
  model.traverse(object => {
    if (object.userData.originalModularPart === 'hair' && object.userData.hairstyleId !== appearance.hairstyle) remove.push(object);
    const part = object.userData.originalMakerPart;
    if (part === 'source-hair' && appearance.hairstyle !== 'original'
      || part === 'scalp' && appearance.hairstyle === 'original'
      || part === 'short' && appearance.hairstyle !== 'short'
      || part === 'ponytail' && appearance.hairstyle !== 'ponytail') remove.push(object);
  });
  remove.forEach(object => object.removeFromParent());
  model.traverse(object => {
    if (!(object instanceof Mesh)) return;
    const materials = Array.isArray(object.material) ? object.material : [object.material];
    for (const material of materials as MeshBasicMaterial[]) {
      if (appearance.hairstyle !== 'original' && material.map && material.userData.originalMakerSkinPatch) {
        material.map = correctedSkinMap(material.map, material.userData.originalMakerSkinPatch);
        ownedTextures.push(material.map);
        material.needsUpdate = true;
      }
      if (material.userData.originalMakerHair === false || (!material.userData.originalMakerHair && !material.name.startsWith('maker-original-hair-'))) continue;
      const original = material.name.startsWith('maker-original-hair-source') || object.userData.originalModularPart === 'hair';
      if (appearance.hairColor === 'source' && original) continue;
      const color = appearance.hairColor === 'source'
        ? material.userData.defaultHairColor ?? DEFAULT_COLORS[characterId] ?? '#614439'
        : appearance.hairColor;
      if (original && material.map) {
        material.map = colorableMap(material.map);
        ownedTextures.push(material.map);
      }
      material.color.set(color);
      material.needsUpdate = true;
    }
  });
  return { dispose() { ownedTextures.forEach(texture => texture.dispose()); } };
}
