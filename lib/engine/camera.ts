// カメラ（段階4 Z「three を外す」。docs/plan/2026-11-12-z-three-removal-plan.md の 2.1）。
// three r178 の src/cameras/Camera.js・PerspectiveCamera.js の写し（式と順番はそのまま。tools/engine-loop.test.mjs で three と Object.is で突き合わせる）。
// 節点は lib/engine/node.ts の Node（r178 の Object3D の写し）。three 全体・React は読まない。
// 既定のカメラの作り方（R3F 9.8.0 の createRoot の configure と同じ）は defaultCamera、大きさに合わせるのは updateCamera（R3F の updateCamera と同じ）。
// 写していない物: copy・clone・toJSON・setFocalLength などの焦点の計算。OrthographicCamera はキャラメイクのオリジナルの下見（components/character-maker/rust/original-camera.ts）が使う。
import { MathUtils, Matrix4, Vector2, Vector3 } from './math';
import { Node } from './node';

/** three の WebGLCoordinateSystem（constants.js）。Rust の kit へは three と同じ -1..1 の行列を送る（components/rust/bridge.ts の writeCamera） */
export const WebGLCoordinateSystem = 2000;

const _v3 = new Vector3();
const _minTarget = new Vector2();
const _maxTarget = new Vector2();

/** three r178 の Camera の写し */
export class Camera extends Node {
  readonly isCamera = true;
  matrixWorldInverse = new Matrix4();
  projectionMatrix = new Matrix4();
  projectionMatrixInverse = new Matrix4();
  coordinateSystem = WebGLCoordinateSystem;

  constructor() {
    super();
    this.type = 'Camera';
  }

  getWorldDirection(target: Vector3) {
    return super.getWorldDirection(target).negate();
  }

  updateMatrixWorld(force?: boolean) {
    super.updateMatrixWorld(force);
    this.matrixWorldInverse.copy(this.matrixWorld).invert();
  }

  updateWorldMatrix(updateParents: boolean, updateChildren: boolean) {
    super.updateWorldMatrix(updateParents, updateChildren);
    this.matrixWorldInverse.copy(this.matrixWorld).invert();
  }
}

type View = { enabled: boolean; fullWidth: number; fullHeight: number; offsetX: number; offsetY: number; width: number; height: number };

/** three r178 の PerspectiveCamera の写し */
export class PerspectiveCamera extends Camera {
  readonly isPerspectiveCamera = true;
  fov: number;
  zoom = 1;
  near: number;
  far: number;
  focus = 10;
  aspect: number;
  view: View | null = null;
  filmGauge = 35;
  filmOffset = 0;
  /** R3F の印（true なら大きさに合わせて aspect を替えない） */
  manual?: boolean;

  constructor(fov = 50, aspect = 1, near = 0.1, far = 2000) {
    super();
    this.type = 'PerspectiveCamera';
    this.fov = fov;
    this.near = near;
    this.far = far;
    this.aspect = aspect;
    this.updateProjectionMatrix();
  }

  getEffectiveFOV() {
    return MathUtils.RAD2DEG * 2 * Math.atan(Math.tan(MathUtils.DEG2RAD * 0.5 * this.fov) / this.zoom);
  }

  getFilmWidth() {
    return this.filmGauge * Math.min(this.aspect, 1);
  }

  getFilmHeight() {
    return this.filmGauge / Math.max(this.aspect, 1);
  }

  getViewBounds(distance: number, minTarget: Vector2, maxTarget: Vector2) {
    _v3.set(-1, -1, 0.5).applyMatrix4(this.projectionMatrixInverse);
    minTarget.set(_v3.x, _v3.y).multiplyScalar(-distance / _v3.z);
    _v3.set(1, 1, 0.5).applyMatrix4(this.projectionMatrixInverse);
    maxTarget.set(_v3.x, _v3.y).multiplyScalar(-distance / _v3.z);
  }

  getViewSize(distance: number, target: Vector2) {
    this.getViewBounds(distance, _minTarget, _maxTarget);
    return target.subVectors(_maxTarget, _minTarget);
  }

  setViewOffset(fullWidth: number, fullHeight: number, x: number, y: number, width: number, height: number) {
    this.aspect = fullWidth / fullHeight;
    if (this.view === null) this.view = { enabled: true, fullWidth: 1, fullHeight: 1, offsetX: 0, offsetY: 0, width: 1, height: 1 };
    this.view.enabled = true;
    this.view.fullWidth = fullWidth;
    this.view.fullHeight = fullHeight;
    this.view.offsetX = x;
    this.view.offsetY = y;
    this.view.width = width;
    this.view.height = height;
    this.updateProjectionMatrix();
  }

  clearViewOffset() {
    if (this.view !== null) this.view.enabled = false;
    this.updateProjectionMatrix();
  }

  updateProjectionMatrix() {
    const near = this.near;
    let top = near * Math.tan(MathUtils.DEG2RAD * 0.5 * this.fov) / this.zoom;
    let height = 2 * top;
    let width = this.aspect * height;
    let left = -0.5 * width;
    const view = this.view;
    if (this.view !== null && this.view.enabled) {
      const fullWidth = view!.fullWidth, fullHeight = view!.fullHeight;
      left += view!.offsetX * width / fullWidth;
      top -= view!.offsetY * height / fullHeight;
      width *= view!.width / fullWidth;
      height *= view!.height / fullHeight;
    }
    const skew = this.filmOffset;
    if (skew !== 0) left += near * skew / this.getFilmWidth();
    this.projectionMatrix.makePerspective(left, left + width, top, top - height, near, this.far, this.coordinateSystem as never);
    this.projectionMatrixInverse.copy(this.projectionMatrix).invert();
  }
}

/** three r178 の OrthographicCamera の写し（setViewOffset は写していない） */
export class OrthographicCamera extends Camera {
  readonly isOrthographicCamera = true;
  zoom = 1;
  view: View | null = null;
  left: number;
  right: number;
  top: number;
  bottom: number;
  near: number;
  far: number;
  /** R3F の印（true なら大きさに合わせて left などを替えない） */
  manual?: boolean;

  constructor(left = -1, right = 1, top = 1, bottom = -1, near = 0.1, far = 2000) {
    super();
    this.type = 'OrthographicCamera';
    this.left = left;
    this.right = right;
    this.top = top;
    this.bottom = bottom;
    this.near = near;
    this.far = far;
    this.updateProjectionMatrix();
  }

  updateProjectionMatrix() {
    const dx = (this.right - this.left) / (2 * this.zoom);
    const dy = (this.top - this.bottom) / (2 * this.zoom);
    const cx = (this.right + this.left) / 2;
    const cy = (this.top + this.bottom) / 2;
    let left = cx - dx;
    let right = cx + dx;
    let top = cy + dy;
    let bottom = cy - dy;
    if (this.view !== null && this.view.enabled) {
      const scaleW = (this.right - this.left) / this.view.fullWidth / this.zoom;
      const scaleH = (this.top - this.bottom) / this.view.fullHeight / this.zoom;
      left += scaleW * this.view.offsetX;
      right = left + scaleW * this.view.width;
      top -= scaleH * this.view.offsetY;
      bottom = top - scaleH * this.view.height;
    }
    this.projectionMatrix.makeOrthographic(left, right, top, bottom, this.near, this.far, this.coordinateSystem as never);
    this.projectionMatrixInverse.copy(this.projectionMatrix).invert();
  }
}

/** R3F の Canvas の camera の引数のうち /play が使う物（applyProps の代わり。配列は set、数はそのまま入れる） */
export type CameraOptions = { position?: [number, number, number]; rotation?: [number, number, number]; fov?: number; near?: number; far?: number; zoom?: number; aspect?: number };

/**
 * R3F 9.8.0 の既定のカメラ（createRoot の configure）: PerspectiveCamera(75, 0, 0.1, 1000)・position.z = 5・引数を当てる・
 * aspect などを渡したら manual・rotation を渡さなければ原点を見る。大きさに合わせるのは updateCamera
 */
export function defaultCamera(options?: CameraOptions) {
  const camera = new PerspectiveCamera(75, 0, 0.1, 1000);
  camera.position.z = 5;
  if (options) {
    // applyProps は引数の順に当てる（配列は fromArray、数はそのまま入れる。射影は作り直さない。大きさが決まったときの updateCamera が作る）
    for (const [key, value] of Object.entries(options)) {
      if (value === undefined) continue;
      if (key === 'position') camera.position.fromArray(value as number[]);
      else if (key === 'rotation') camera.rotation.fromArray(value as [number, number, number]);
      else (camera as unknown as Record<string, number>)[key] = value as number;
    }
    if (!camera.manual && 'aspect' in options) { camera.manual = true; camera.updateProjectionMatrix(); }
  }
  if (!options?.rotation) camera.lookAt(0, 0, 0);
  return camera;
}

/** R3F の updateCamera（大きさが変わったとき）: manual でなければ aspect = 幅 / 高さ にして射影を作り直す */
export function updateCamera(camera: PerspectiveCamera, size: { width: number; height: number }) {
  if (camera.manual) return;
  camera.aspect = size.width / size.height;
  camera.updateProjectionMatrix();
}
