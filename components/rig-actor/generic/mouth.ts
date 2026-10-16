// The generic-character viewer's own copy of speakingMouth in packages/character-runtime/src/runtime/mouth.ts (2026-10-01). Frozen (components/rig-actor/kind-copies.test.mjs).
export type MouthShape = 'mouthClose' | 'aa' | 'ee' | 'ou';
export type MouthWeights = Record<MouthShape, number>;

// A silent visual preview, not audio recognition. Coarticulation crossfades
// complete poses, with genuine closed-mouth intervals between short phrases.
const SEQUENCE: [MouthShape, number][] = [
  ['mouthClose', .16], ['aa', .25], ['ee', .21], ['ou', .23],
  ['aa', .22], ['mouthClose', .27], ['ee', .22], ['aa', .30],
  ['ou', .27], ['ee', .18], ['mouthClose', .48],
];
const duration = SEQUENCE.reduce((sum, item) => sum + item[1], 0);
export function speakingMouth(time: number): MouthWeights {
  let phase = ((time % duration) + duration) % duration;
  for (let i = 0; i < SEQUENCE.length; i++) {
    const [shape, hold] = SEQUENCE[i];
    if (phase >= hold) { phase -= hold; continue; }
    const previous = SEQUENCE[(i + SEQUENCE.length - 1) % SEQUENCE.length][0];
    const t = Math.min(1, phase / .095), mix = t * t * (3 - 2 * t);
    const result: MouthWeights = { mouthClose: 0, aa: 0, ee: 0, ou: 0 };
    result[previous] += 1 - mix; result[shape] += mix;
    return result;
  }
  return { mouthClose: 1, aa: 0, ee: 0, ou: 0 };
}
