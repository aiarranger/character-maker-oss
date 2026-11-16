// Studio のプリセットのカメラ（前の three の版の CharacterMakerScene.tsx の StudioCamera と drei の OrbitControls。R7 で消した）を three なしで。
// 数学・カメラ・回す操作は lib/engine（three r178 と aisns の写し）。測り（立った姿・顔の箱・構えの包み・頭の骨）は Rust の Stage から。
import { Box3, Vector3 } from '../../../lib/engine/math';
import { PerspectiveCamera } from '../../../lib/engine/camera';
import { OrbitControls } from '../../../lib/engine/orbit-controls';
import { HEIGHT_RANGE } from '../../../lib/character-maker/config';
import { makerArmedCameraDistance } from '../../../lib/character-maker/armed-camera';
import { StageCameraTween, markCameraSettled, prefersInstantCamera, projectBox, stageDirection, stageFaceWindow } from '../stage-camera';

/** Studio lens for presets (spec §5.3): fov 28, sized to match the originals' stage occupancy. */
export const STUDIO_FOV = 28;
const HALF_TAN = Math.tan((STUDIO_FOV / 2) * Math.PI / 180);
const STUDIO_FULL_OCCUPANCY = .645;
const STUDIO_FEET = { full: .085, armed: .085 };
const STUDIO_ARMED_TOP = .015;
const STUDIO_ARMED_MATCH = 1.1;
const AZIMUTH = { front: 0, side: Math.PI / 2, back: Math.PI } as const;

export type Framing = 'full' | 'face';
export type View = keyof typeof AZIMUTH;
/** 1 体の測り（Stage の assemble の返り値と query） */
export type Subject = {
  key: string;
  rest: { restHeight: number; restTop: number };
  /** 頭の骨の高さ（身長 1・動かす前。classic のカメラ） */
  headY?: number;
  portrait: Box3;
  hasHead: boolean;
  head: () => Vector3 | null;
  /** 構えの包み（身長 1。無ければ null）。1 体に 1 回だけ測る */
  armed: () => Box3 | null;
};
type Layout = { key: string; framing: Framing; view: View; revision: number; height: number; stance: string; aspect: number };

export class StudioCameraRig {
  readonly camera = new PerspectiveCamera(STUDIO_FOV, 1, .01, 50);
  readonly controls: OrbitControls;
  private tween = new StageCameraTween();
  private baseDistance = 1;
  private previous: Layout | null = null;
  private followed = new Vector3();
  private headNow = new Vector3();
  private shift = new Vector3();
  private armedCache = new Map<string, Box3 | null>();

  constructor(dom: HTMLElement, private root: HTMLElement) {
    this.camera.position.set(0, 1, 4);
    this.controls = new OrbitControls(this.camera, dom);
    Object.assign(this.controls, { enableDamping: true, dampingFactor: .08, enablePan: false, zoomToCursor: true, minPolarAngle: .2, maxPolarAngle: Math.PI * .85 });
    this.controls.addEventListener('start', () => { if (this.tween.active) { this.tween.stop(); markCameraSettled(this.root, true); } });
  }
  dispose() { this.controls.dispose(); }

  private headAnchor(subject: Subject, height: number, out: Vector3) {
    const head = subject.hasHead ? subject.head() : null;
    if (head) return out.copy(head);
    return out.set(0, subject.rest.restTop * height * .86, 0);
  }

  /** StudioCamera の useLayoutEffect（向き・表示範囲・構え・大きさが変わったとき） */
  layout(subject: Subject, framing: Framing, view: View, revision: number, height: number, armed: boolean, aspect: number) {
    const camera = this.camera, orbit = this.controls;
    if (camera.aspect !== aspect) { camera.aspect = aspect; camera.updateProjectionMatrix(); }
    const stance = armed ? 'armed' : '';
    const last = this.previous;
    const reset = !last || last.key !== subject.key || last.framing !== framing || last.view !== view || last.revision !== revision || last.stance !== stance || last.aspect !== aspect;
    this.previous = { key: subject.key, framing, view, revision, height, stance, aspect };
    if (!reset) return;
    const face = framing === 'face';
    const animate = !!last && last.key === subject.key && last.aspect === aspect && (last.framing !== framing || last.view !== view || last.revision !== revision || last.stance !== stance) && !prefersInstantCamera();
    const top = subject.rest.restTop, restHeight = subject.rest.restHeight, feet = top - restHeight;
    const direction = stageDirection(AZIMUTH[view]);
    let target: Vector3, distance: number;
    if (face) {
      const anchor = this.headAnchor(subject, height, new Vector3());
      const headBox = subject.portrait.clone();
      headBox.min.multiplyScalar(height).add(anchor); headBox.max.multiplyScalar(height).add(anchor);
      this.followed.copy(anchor);
      const frame = stageFaceWindow(headBox, headBox.getCenter(new Vector3()), direction, aspect, 1.06);
      distance = Math.max(.2, frame.visible) / (2 * HALF_TAN);
      target = frame.target;
    } else {
      const feetLine = armed ? STUDIO_FEET.armed : STUDIO_FEET.full;
      let envelope: Box3 | null = null;
      if (armed) {
        if (!this.armedCache.has(subject.key)) this.armedCache.set(subject.key, subject.armed());
        envelope = this.armedCache.get(subject.key)?.clone() ?? null;
      }
      let centreR = 0, right: Vector3 | null = null;
      if (envelope) {
        envelope.min.multiplyScalar(HEIGHT_RANGE.max); envelope.max.multiplyScalar(HEIGHT_RANGE.max);
        const projected = projectBox(envelope, direction);
        const spanU = Math.max(projected.u1 - projected.u0, .01), spanR = Math.max(projected.r1 - projected.r0, .01);
        const depth = Math.abs(envelope.getSize(new Vector3()).dot(new Vector3(Math.abs(direction.x), Math.abs(direction.y), Math.abs(direction.z))));
        distance = STUDIO_ARMED_MATCH * Math.max(spanU / (1 - feetLine - STUDIO_ARMED_TOP), spanR * 1.04 / Math.max(aspect, .1)) / (2 * HALF_TAN) + depth / 2;
        const visW = 2 * HALF_TAN * (distance - depth / 2) * aspect, m = visW * .01;
        const lo = projected.r1 - visW / 2 + m, hi = projected.r0 + visW / 2 - m;
        centreR = lo <= hi ? Math.min(Math.max((projected.r0 + projected.r1) / 4, lo), hi) : (projected.r0 + projected.r1) / 2;
        right = projected.right;
      } else {
        distance = armed
          ? makerArmedCameraDistance(Math.max(1.3, restHeight * HEIGHT_RANGE.max), aspect, 1)
          : restHeight / (STUDIO_FULL_OCCUPANCY * 2 * HALF_TAN);
      }
      const visible = 2 * HALF_TAN * distance;
      const goalNdc = -1 + 2 * feetLine;
      target = new Vector3(0, feet + visible * (.5 - feetLine), 0);
      if (right) target.addScaledVector(right, centreR);
      const probe = new PerspectiveCamera(STUDIO_FOV, aspect, .01, 100), point = new Vector3();
      for (let i = 0; i < 4; i++) {
        probe.position.copy(target).addScaledVector(direction, distance); probe.lookAt(target); probe.updateMatrixWorld(); probe.updateProjectionMatrix();
        point.set(direction.x * restHeight * .08, feet, direction.z * restHeight * .08).project(probe as never);
        target.y += (point.y - goalNdc) * visible / 2;
      }
    }
    const goal = { target, position: target.clone().addScaledVector(direction, distance), zoom: 1 };
    this.baseDistance = distance;
    orbit.minDistance = distance * .25; orbit.maxDistance = distance / .6;
    camera.near = .01; camera.far = Math.max(50, distance * 4);
    if (animate) {
      const current = camera.position.distanceTo(orbit.target);
      orbit.minDistance = Math.min(current, distance) * .25; orbit.maxDistance = Math.max(current, distance) / .6;
      this.tween.start(goal);
      markCameraSettled(this.root, false);
      return;
    }
    this.tween.stop();
    camera.position.copy(goal.position);
    camera.zoom = 1;
    camera.lookAt(target);
    camera.updateProjectionMatrix();
    orbit.target.copy(target);
    orbit.update();
    markCameraSettled(this.root, true);
  }

  /** drei の OrbitControls の useFrame（priority -1。模型より先） */
  beforeFrame() { if (this.controls.enabled) this.controls.update(); }

  /** StudioCamera の useFrame（顔は頭の骨を平行移動で追う・切り替えの弧） */
  frame(subject: Subject | null, framing: Framing, height: number, delta: number) {
    const camera = this.camera, orbit = this.controls;
    if (subject && framing === 'face') {
      this.headAnchor(subject, height, this.headNow);
      this.shift.copy(this.headNow).sub(this.followed);
      if (this.shift.lengthSq() > 0) {
        camera.position.add(this.shift); orbit.target.add(this.shift);
        this.tween.translate(this.shift);
        this.followed.copy(this.headNow);
        if (!this.tween.active) orbit.update();
      }
    }
    if (!this.tween.active) return;
    const settled = this.tween.step(camera, orbit.target, delta);
    camera.updateProjectionMatrix(); orbit.update();
    if (settled) { orbit.minDistance = this.baseDistance * .25; orbit.maxDistance = this.baseDistance / .6; markCameraSettled(this.root, true); }
  }

  /** Stage へ渡すカメラの値（位置・回転・fov・near・far・aspect） */
  write(out: Float64Array) {
    const c = this.camera;
    c.updateMatrixWorld();
    out.set([c.position.x, c.position.y, c.position.z, c.quaternion.x, c.quaternion.y, c.quaternion.z, c.quaternion.w, c.fov, c.near, c.far, c.aspect]);
  }
}

/** classic（Studio でない下見）のカメラ（前の three の版の CharacterMakerScene の Camera と drei の OrbitControls。fov 32・減衰 0.12） */
export class ClassicCameraRig {
  readonly camera = new PerspectiveCamera(32, 1, .01, 50);
  readonly controls: OrbitControls;
  private previous: { key: string; framing: Framing; view: View; revision: number; height: number; armed: boolean; aspect: number } | null = null;

  constructor(dom: HTMLElement) {
    this.camera.position.set(0, 1, 4);
    this.controls = new OrbitControls(this.camera, dom);
    Object.assign(this.controls, { enableDamping: true, dampingFactor: .12, minDistance: .35, minPolarAngle: .2, maxPolarAngle: Math.PI * .85 });
  }
  dispose() { this.controls.dispose(); }
  beforeFrame() { if (this.controls.enabled) this.controls.update(); }

  layout(subject: Subject, framing: Framing, view: View, revision: number, height: number, armed: boolean, aspect: number) {
    const camera = this.camera, controls = this.controls;
    if (camera.aspect !== aspect) { camera.aspect = aspect; camera.updateProjectionMatrix(); }
    controls.maxDistance = Math.max(8, makerArmedCameraDistance(Math.max(1.3, subject.rest.restHeight * HEIGHT_RANGE.max), aspect) * 1.2);
    const last = this.previous;
    const reset = !last || last.key !== subject.key || last.framing !== framing || last.view !== view || last.revision !== revision || last.armed !== armed || last.aspect !== aspect;
    this.previous = { key: subject.key, framing, view, revision, height, armed, aspect };
    const face = framing === 'face', restHeadY = subject.headY ?? subject.rest.restTop;
    if (!reset) {
      // 身長を変えても回した向きと寄りは保つ。顔は頭の分だけ平行に動かす
      if (face && last.height !== height) {
        const deltaY = restHeadY * (height - last.height);
        camera.position.y += deltaY; controls.target.y += deltaY;
        controls.update();
      }
      return;
    }
    const referenceHeight = face ? 1 : HEIGHT_RANGE.max;
    const top = subject.rest.restTop * referenceHeight;
    const modelHeight = Math.max(1.3, subject.rest.restHeight * referenceHeight);
    const target = new Vector3(0, face ? top - modelHeight * 0.16 + restHeadY * (height - 1) : top * (armed ? 0.61 : 0.51), 0);
    const distance = face ? modelHeight * 0.78 : (armed ? makerArmedCameraDistance(modelHeight, aspect) : modelHeight * 2.05);
    const angle = view === 'side' ? Math.PI / 2 : view === 'back' ? Math.PI : 0;
    camera.position.set(Math.sin(angle) * distance, target.y + (face ? 0 : modelHeight * 0.025), Math.cos(angle) * distance);
    camera.lookAt(target);
    camera.updateProjectionMatrix();
    controls.target.copy(target);
    controls.update();
  }

  write(out: Float64Array) {
    const c = this.camera;
    c.updateMatrixWorld();
    out.set([c.position.x, c.position.y, c.position.z, c.quaternion.x, c.quaternion.y, c.quaternion.z, c.quaternion.w, c.fov, c.near, c.far, c.aspect]);
  }
}
