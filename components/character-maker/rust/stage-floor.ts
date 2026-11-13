// Studio の床の動き（今の版は components/character-maker/StageFloor.tsx の useFrame）。絵は Rust の Stage（rust/game/src/floor.rs）。
import { STAGE_RING_PER_HEIGHT, STAGE_SHADOW_PER_HEIGHT } from '../StageFloor.constants';
import { prefersInstantCamera } from '../stage-camera';
import type { MakerStage } from './stage';

const easeOutExpo = (t: number) => t >= 1 ? 1 : 1 - Math.pow(2, -10 * t);

/** 接地の影の絵（StageFloor の contactShadowTexture と同じ描き方）を Stage へ渡す */
export function sendFloorShadow(stage: MakerStage) {
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 128;
  const context = canvas.getContext('2d')!;
  const gradient = context.createRadialGradient(64, 64, 0, 64, 64, 64);
  gradient.addColorStop(0, 'rgba(23,21,28,.38)');
  gradient.addColorStop(.45, 'rgba(23,21,28,.16)');
  gradient.addColorStop(1, 'rgba(23,21,28,0)');
  context.fillStyle = gradient;
  context.fillRect(0, 0, 128, 128);
  // CanvasTexture は flipY（上下を返して上げる）。丸いので見た目は同じだが、値をそろえる
  const data = context.getImageData(0, 0, 128, 128).data, flipped = new Uint8Array(data.length);
  for (let y = 0; y < 128; y++) flipped.set(data.subarray((127 - y) * 512, (128 - y) * 512), y * 512);
  stage.image('floor-shadow', 128, 128, flipped);
}

export class StageFloorMotion {
  private follow = { x: 0, z: 0 };
  private entrance = { started: false, elapsed: 0 };
  private pulse = { elapsed: 1, key: '' };
  private instant = prefersInstantCamera();
  private vals = new Float64Array(7);
  private lastColor = '';

  /** subjectKey が替わったら腰の追いを 0 から（useEffect の subject） */
  reset() { this.follow = { x: 0, z: 0 }; }

  frame(stage: MakerStage, o: { hips: [number, number, number] | null; scale: number; restHeight: number; accent?: string; pulseKey: string; visible: boolean; delta: number }) {
    const ringRadius = Math.max(o.restHeight, 0) * STAGE_RING_PER_HEIGHT, shadowDiameter = Math.max(o.restHeight, 0) * STAGE_SHADOW_PER_HEIGHT;
    const visible = o.visible && o.restHeight > 0;
    if (this.pulse.key !== o.pulseKey) { if (this.pulse.key) this.pulse.elapsed = this.instant ? 1 : 0; this.pulse.key = o.pulseKey; }
    if (o.hips) {
      this.follow.x += (o.hips[0] - this.follow.x) * .2;
      this.follow.z += (o.hips[2] - this.follow.z) * .2;
    }
    const state = this.entrance;
    if (visible && !state.started) { state.started = true; state.elapsed = this.instant ? 1 : 0; }
    if (state.started && state.elapsed < .4) state.elapsed += o.delta;
    const grow = !state.started ? 0 : this.instant ? 1 : .6 + .4 * easeOutExpo(Math.min(1, state.elapsed / .4));
    const opacity = !state.started ? 0 : this.instant ? 1 : Math.min(1, state.elapsed / .4);
    const p = this.pulse;
    if (p.elapsed < .3) p.elapsed += o.delta;
    const bump = p.elapsed < .3 ? Math.sin(Math.PI * Math.min(1, p.elapsed / .3)) * .06 : 0;
    this.vals.set([this.follow.x, this.follow.z, shadowDiameter * o.scale * grow, opacity, Math.max(ringRadius, 1e-3), o.scale * (1 + bump), state.started ? 1 : 0]);
    const color = o.accent || '#B0512A';
    stage.floor(this.vals, color === this.lastColor ? '' : color);
    this.lastColor = color;
  }
}
