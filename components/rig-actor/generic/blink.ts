// The generic-character viewer's own copy of packages/character-runtime/src/runtime/blink.ts (2026-10-01). Frozen (components/rig-actor/kind-copies.test.mjs).
/** In-between corrections are offsets from the linearly blended base blink. */
export function blinkCorrectiveWeight(name: string, closure: number): number | null {
  const match = /^blink(?:Left|Right)Inbetween(25|50|75)$/.exec(name);
  if (!match) return null;
  const center = Number(match[1]) / 100;
  return Math.max(0, 1 - Math.abs(closure - center) / .25);
}

/** Ease into closure, hold briefly, then reopen more slowly. */
export function automaticBlink(elapsed: number): number {
  const time = elapsed % 4.1 - 3.81;
  const smooth = (v: number) => v * v * (3 - 2 * v);
  if (time < 0) return 0;
  if (time < .085) return smooth(time / .085);
  if (time < .115) return 1;
  if (time < .275) return 1 - smooth((time - .115) / .16);
  return 0;
}
