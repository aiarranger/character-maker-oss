/** Apply a builder-measured atlas correction without changing the source data.
 * Coordinates are decoded image pixels, with the origin at the top left.
 * The profile contains fixed polygons and clean skin sample rows; no color
 * threshold, face detection, or geometry inference runs in the viewer. */
export function applyOriginalSkinPatch(pixels, width, height, profile) {
  if (profile.version !== 'original-skin-patch-v1' || profile.width !== width || profile.height !== height || pixels.length !== width * height * 4) {
    throw new Error('Character skin correction does not match its source atlas.');
  }
  const output = new Uint8ClampedArray(pixels);
  const { rowStart, rowEnd } = profile.samples;
  const count = rowEnd - rowStart + 1;
  const inside = (x, y, polygon) => {
    let hit = false;
    for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
      const [ax, ay] = polygon[i], [bx, by] = polygon[j];
      if ((ay > y) !== (by > y) && x < (bx - ax) * (y - ay) / (by - ay) + ax) hit = !hit;
    }
    return hit;
  };
  const edgeDistance = (x, y, polygon) => {
    let distance = Infinity;
    for (let i = 0; i < polygon.length; i++) {
      const [ax, ay] = polygon[i], [bx, by] = polygon[(i + 1) % polygon.length];
      const dx = bx - ax, dy = by - ay, length2 = dx * dx + dy * dy;
      const t = length2 ? Math.max(0, Math.min(1, ((x - ax) * dx + (y - ay) * dy) / length2)) : 0;
      distance = Math.min(distance, Math.hypot(x - ax - t * dx, y - ay - t * dy));
    }
    return distance;
  };
  for (const polygon of profile.polygons) {
    const minX = Math.max(0, Math.floor(Math.min(...polygon.map(p => p[0]))));
    const maxX = Math.min(width - 1, Math.ceil(Math.max(...polygon.map(p => p[0]))));
    const minY = Math.max(0, Math.floor(Math.min(...polygon.map(p => p[1]))));
    const maxY = Math.min(height - 1, Math.ceil(Math.max(...polygon.map(p => p[1]))));
    for (let y = minY; y <= maxY; y++) for (let x = minX; x <= maxX; x++) {
      if (!inside(x + .5, y + .5, polygon)) continue;
      const t = Math.min(1, edgeDistance(x + .5, y + .5, polygon) / profile.feather);
      const blend = t * t * (3 - 2 * t), at = (y * width + x) * 4;
      for (let channel = 0; channel < 3; channel++) {
        let skin = 0;
        for (let sy = rowStart; sy <= rowEnd; sy++) skin += pixels[(sy * width + x) * 4 + channel];
        output[at + channel] = Math.round(pixels[at + channel] * (1 - blend) + skin / count * blend);
      }
      // Keep alpha exactly, including at a partially transparent UV border.
    }
  }
  return output;
}
