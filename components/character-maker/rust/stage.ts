// キャラメイクの下見は Rust の Stage（rust/game。wasm は /maker-stage/。このリポジトリでは rust/web/build.sh が public/maker-stage/ に作る）で描く。docs/rust-stage-plan.md
// WebGPU が無い（navigator.gpu が無い・requestAdapter() が null・Stage を作れない・描いている途中で GPU を失った）ときは、
// 遊べないことを知らせる表示（../StageUnsupported.tsx）にする。three の版は無い（2026-11-13 Owner: WebGPU の無い端末には対応しない）。
import { useSyncExternalStore } from 'react';

export type StageSupport = 'checking' | 'ok' | 'unsupported';
const hasGpu = () => typeof navigator !== 'undefined' && !!(navigator as Navigator & { gpu?: unknown }).gpu;
let support: StageSupport = typeof navigator === 'undefined' ? 'checking' : hasGpu() ? 'checking' : 'unsupported';
const listeners = new Set<() => void>();
function set(next: StageSupport) {
  if (support === next || support === 'unsupported') return;
  support = next;
  for (const f of listeners) f();
}
if (typeof window !== 'undefined' && support === 'checking') {
  (navigator as Navigator & { gpu: { requestAdapter(): Promise<unknown | null> } }).gpu.requestAdapter()
    .then(adapter => set(adapter ? 'ok' : 'unsupported'), () => set('unsupported'));
  window.addEventListener('character-maker-gpu-lost', () => markUnsupported('GPU の Device を失いました'));
}
/** 描けない（Stage を作れない・描いている途中で落ちた）。このページの間は戻らない */
export function markUnsupported(reason: unknown) {
  if (support === 'unsupported') return;
  console.warn('[studio] 3D を描けません:', reason);
  set('unsupported');
}
const subscribe = (f: () => void) => { listeners.add(f); return () => { listeners.delete(f); }; };
/** 描けるか。サーバーの描画では 'checking' */
export function useStageSupport() { return useSyncExternalStore(subscribe, () => support, () => 'checking' as StageSupport); }

export type MakerStage = {
  has(name: string): boolean;
  asset(name: string, bytes: Uint8Array): void;
  assemble(json: string): string;
  apply(json: string): void;
  play(json: string): void;
  step(dt: number): void;
  draw(cam: Float64Array): void;
  query(json: string): string;
  floor(vals: Float64Array, color: string): void;
  set_dpr(dpr: number): void;
  debug(): string;
  events(): string;
  export(json: string): Uint8Array;
  glb_height(bytes: Uint8Array): number;
  classic_floor(): void;
  image(name: string, w: number, h: number, rgba: Uint8Array): void;
  free(): void;
};
type Module = { default: (o?: unknown) => Promise<unknown>; create_stage: (canvas: HTMLCanvasElement, init: string) => Promise<MakerStage> };
let wasm: Promise<Module> | null = null;
/** wasm は /maker-stage/（バンドラを通さない） */
function loadWasm() {
  return wasm ??= (async () => {
    const url = '/maker-stage/character_maker_stage.js';
    const m = await import(/* webpackIgnore: true */ /* turbopackIgnore: true */ url) as Module;
    await m.default();
    return m;
  })();
}
export async function createMakerStage(canvas: HTMLCanvasElement, init: Record<string, unknown>): Promise<MakerStage> {
  if (!hasGpu()) throw new Error('このブラウザは WebGPU に対応していません。');
  const m = await loadWasm();
  return m.create_stage(canvas, JSON.stringify(init));
}

