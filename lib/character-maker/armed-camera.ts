/** Fixed full-view distance for the complete enlarged weapon arc. Aspect is
 * considered only when entering/resetting/resizing the view, never per swing.
 * `factor` scales the distance for the studio lens (classic keeps the default 1). */
export function makerArmedCameraDistance(modelHeight: number, aspect: number, factor = 1) {
  return modelHeight * 3.3 * factor * Math.max(1, 1 / Math.max(aspect, 0.1));
}
