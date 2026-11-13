// Studio のオリジナル 4 体のカメラ（今の版は components/character-maker/OriginalCharacterScene.tsx の OriginalCamera と drei の OrbitControls）を three なしで。
// 正射影（R3F の orthographic の Canvas: left・right・top・bottom は canvas の CSS の大きさの半分、zoom で拡大）。数学・カメラ・回す操作は lib/engine。
// 測り（今の姿の箱・頭の骨・構えの包み）は Rust の Stage から。Stage のカメラは透視だけなので、write で「同じ見え方になる、とても遠く・とても狭い fov の透視」に直して渡す。
import { Box3, Matrix4, Vector3 } from '../../../lib/engine/math';
import { OrthographicCamera } from '../../../lib/engine/camera';
import { OrbitControls } from '../../../lib/engine/orbit-controls';
import { ORIGINAL_SIZE_RANGE } from '../../../lib/character-maker/original-config';
import { STAGE_ARMED_LAYOUT, StageCameraTween, fitStageFace, fitStageFull, markCameraSettled, prefersInstantCamera, stageDirection } from '../stage-camera';

export type Framing = 'full' | 'face';
export type View = 'front' | 'side' | 'back';
const VIEW_AZIMUTH = { front: 0, side: -Math.PI / 2, back: Math.PI } as const;
/** 透視へ直すときに後ろへ下げる距離（拡大の差は被写体の前後 ±0.3 で 0.15% ほど） */
const FAR_LENS = 200;

/** 1 体の測り（Stage の query） */
export type OriginalSubject = {
  key: string;
  token: string;
  /** 今の姿の描く頂点の箱（体・頭・武器を除いた体）。無ければ null */
  bounds(): { body: Box3; head: Box3; unarmed: Box3 } | null;
  /** 頭の骨の世界の行列。頭の骨が無ければ null */
  head(): Matrix4 | null;
  /** 構えの包み（今の大きさ。武器ありと武器なし）。動作が無ければ null */
  armed(): { envelope: Box3; body: Box3 } | null;
};

// 構えの全身の包みは 1 体・武器・体型ごとに 1 回だけ測る（大きさ 1 で控える。今の版の armedBoundsCache）
const armedBoundsCache = new Map<string, { envelope: Box3; body: Box3 }>();
function cachedArmedBounds(key: string, subject: OriginalSubject, scale: number, envelope: Box3, body: Box3) {
  let cached = armedBoundsCache.get(key);
  if (!cached) {
    const measured = subject.armed();
    if (!measured) return false;
    const unit = (box: Box3) => { const stored = box.clone(); stored.min.divideScalar(scale); stored.max.divideScalar(scale); return stored; };
    cached = { envelope: unit(measured.envelope), body: measured.body.isEmpty() ? unit(measured.envelope) : unit(measured.body) };
    armedBoundsCache.set(key, cached);
    if (armedBoundsCache.size > 48) armedBoundsCache.delete(armedBoundsCache.keys().next().value!);
  }
  envelope.copy(cached.envelope); envelope.min.multiplyScalar(scale); envelope.max.multiplyScalar(scale);
  body.copy(cached.body); body.min.multiplyScalar(scale); body.max.multiplyScalar(scale);
  return true;
}

type Props = {
  framing: Framing; view: View; revision: number; scale: number; sourceToken: string; boundsRevision: string; armedKey: string;
  playing: boolean; armed: boolean; width: number; height: number;
};

export class OriginalCameraRig {
  // R3F の <Canvas orthographic camera={{ position: [0, 1.04, 5], near: .01, far: 30, zoom: 250 }}>（回転を渡さないので原点を見る）
  readonly camera = new OrthographicCamera(-1, 1, 1, -1, .01, 30);
  readonly controls: OrbitControls;
  onCameraInteraction?: () => void;
  private pendingReset = true; private custom = false; private interacting = false; private programmatic = false;
  private bounds = { body: new Box3(), head: new Box3(), envelope: new Box3(), frame: new Box3(), unarmed: new Box3(), centre: new Box3() };
  private anchorLocal = new Vector3(); private followed = new Vector3(); private nextAnchor = new Vector3(); private translation = new Vector3();
  private previousScale = 1; private sampleElapsed = 0; private pendingMeasure = true;
  private tween = new StageCameraTween();
  private animateNext = false; private baseZoom = 1;
  private previous: { subject: string | null; sourceToken: string; framing: string; view: string; revision: number; stance: string; width: number; height: number } | null = null;
  private layoutKey = ''; private measureKey = '';
  private size = { width: 0, height: 0 };
  private headMatrix = new Matrix4(); private inverse = new Matrix4();

  constructor(dom: HTMLElement, private root: HTMLElement) {
    this.camera.position.set(0, 1.04, 5);
    this.camera.zoom = 250;
    this.camera.lookAt(new Vector3(0, 0, 0));
    this.camera.updateProjectionMatrix();
    this.controls = new OrbitControls(this.camera as never, dom);
    Object.assign(this.controls, { enableDamping: true, dampingFactor: .08, enablePan: false, zoomToCursor: true, minPolarAngle: .2, maxPolarAngle: Math.PI * .85 });
    this.controls.addEventListener('start', () => { this.interacting = true; this.tween.stop(); markCameraSettled(this.root, true); });
    this.controls.addEventListener('end', () => { this.interacting = false; });
    this.controls.addEventListener('change', () => {
      if (!this.interacting || this.programmatic || this.pendingReset || this.custom) return;
      this.custom = true;
      this.onCameraInteraction?.();
    });
  }
  dispose() { this.controls.dispose(); }

  /** R3F の updateCamera（正射影: 大きさの半分を左右上下に） */
  private resize(width: number, height: number) {
    if (this.size.width === width && this.size.height === height) return;
    this.size = { width, height };
    const c = this.camera;
    c.left = width / -2; c.right = width / 2; c.top = height / 2; c.bottom = height / -2;
    c.updateProjectionMatrix();
  }

  /** OriginalCamera の 2 つの useLayoutEffect（測り直し・構図の作り直し）。毎コマ呼んでよい（値が変わったときだけ効く） */
  layout(subject: OriginalSubject | null, p: Props) {
    this.resize(p.width, p.height);
    const measureKey = `${p.boundsRevision}|${p.scale}`;
    if (measureKey !== this.measureKey) { this.measureKey = measureKey; this.pendingMeasure = true; }
    const stance = p.armed ? 'armed' : '';
    const key = [subject?.key ?? '', p.sourceToken, p.framing, p.view, p.revision, p.armed, stance, p.width, p.height].join('|');
    if (key === this.layoutKey) return;
    this.layoutKey = key;
    const last = this.previous, id = subject?.key ?? null;
    // 回す弧は、見る方向・表示範囲・方向のボタン・構えの切り替えのときだけ。読み込み・大きさの変化・最初の構図はすぐ切り替える
    this.animateNext = !!last && last.subject === id && last.sourceToken === p.sourceToken && last.width === p.width && last.height === p.height
      && (last.framing !== p.framing || last.view !== p.view || last.revision !== p.revision || last.stance !== stance) && !prefersInstantCamera();
    this.previous = { subject: id, sourceToken: p.sourceToken, framing: p.framing, view: p.view, revision: p.revision, stance, width: p.width, height: p.height };
    if (!this.animateNext) this.tween.stop();
    this.pendingReset = true;
    this.custom = false;
    this.bounds.envelope.makeEmpty();
    markCameraSettled(this.root, false);
  }

  /** drei の OrbitControls の useFrame（priority -1。模型の -2 のあと、カメラの 0 の前） */
  beforeFrame() { if (this.controls.enabled) this.controls.update(); }

  private updateControls() {
    this.programmatic = true;
    try { this.controls.update(); } finally { this.programmatic = false; }
  }
  private headWorld(subject: OriginalSubject) {
    const m = subject.head();
    if (!m) return null;
    this.headMatrix.copy(m);
    return this.headMatrix;
  }

  /** OriginalCamera の useFrame */
  frame(subject: OriginalSubject | null, p: Props, delta: number) {
    if (!subject || subject.token !== p.sourceToken) return;
    const camera = this.camera, orbit = this.controls, bounds = this.bounds, scale = p.scale;
    const followFace = () => {
      const head = p.framing === 'face' ? this.headWorld(subject) : null;
      if (head) {
        // 頭の骨を追うのは軽いので毎コマ（回したあとも）
        this.nextAnchor.copy(this.anchorLocal).applyMatrix4(head);
        this.translation.copy(this.nextAnchor).sub(this.followed);
        camera.position.add(this.translation); orbit.target.add(this.translation);
        this.tween.translate(this.translation);
        this.followed.copy(this.nextAnchor);
        if (this.previousScale !== scale) { camera.zoom *= this.previousScale / scale; camera.updateProjectionMatrix(); }
        this.updateControls();
      }
      this.previousScale = scale;
    };
    const stepTween = () => {
      if (!this.tween.active) return;
      this.programmatic = true;
      try {
        const settled = this.tween.step(camera as never, orbit.target, delta);
        camera.updateProjectionMatrix(); orbit.update();
        if (settled) { orbit.minZoom = this.baseZoom * .6; orbit.maxZoom = this.baseZoom * 4; markCameraSettled(this.root, true); }
      } finally { this.programmatic = false; }
    };
    if (this.custom && !this.pendingReset) { followFace(); stepTween(); return; }
    this.sampleElapsed += delta;
    if (!this.pendingReset && !this.pendingMeasure && (!p.playing || this.sampleElapsed < .5)) { followFace(); stepTween(); return; }
    // 今の姿の箱は 2 Hz まで（編集の直後はすぐ）。構えの全身は構えの動作の包みに 1 回だけ合わせる
    let measured = false;
    if (p.armed && p.framing === 'full') measured = cachedArmedBounds(p.armedKey, subject, scale, bounds.body, bounds.unarmed);
    else {
      const b = subject.bounds();
      if (b) { bounds.body.copy(b.body); bounds.head.copy(b.head); measured = !b.body.isEmpty(); }
    }
    if (!measured) return;
    this.sampleElapsed = 0; this.pendingMeasure = false;
    const resetting = this.pendingReset;
    const head = this.headWorld(subject);
    if (resetting) {
      bounds.envelope.makeEmpty();
      bounds.head.getCenter(this.followed);
      this.anchorLocal.copy(this.followed);
      if (head) this.anchorLocal.applyMatrix4(this.inverse.copy(head).invert());
    }
    const direction = stageDirection(VIEW_AZIMUTH[p.view]);
    let fit;
    if (p.framing === 'face') {
      this.nextAnchor.copy(this.anchorLocal);
      if (head) this.nextAnchor.applyMatrix4(head);
      else bounds.head.getCenter(this.nextAnchor);
      // 頭からの差の箱を重ねる（動きで顔を追っても、動きの道のりで引いていかない）
      bounds.frame.copy(bounds.head);
      bounds.frame.min.sub(this.nextAnchor).divideScalar(scale);
      bounds.frame.max.sub(this.nextAnchor).divideScalar(scale);
      bounds.envelope.union(bounds.frame);
      bounds.frame.copy(bounds.envelope);
      bounds.frame.min.multiplyScalar(scale).add(this.nextAnchor);
      bounds.frame.max.multiplyScalar(scale).add(this.nextAnchor);
      this.followed.copy(this.nextAnchor);
      fit = fitStageFace(bounds.frame, bounds.frame.getCenter(new Vector3()), direction, p.width, p.height);
    } else {
      // いちばん大きい大きさの枠で合わせる（大きさの比べが見える）。姿の包みは広がるだけ
      const swing = p.armed && p.framing === 'full';
      const reference = ORIGINAL_SIZE_RANGE.max;
      bounds.frame.copy(bounds.body);
      bounds.frame.min.multiplyScalar(reference / scale);
      bounds.frame.max.multiplyScalar(reference / scale);
      bounds.envelope.union(bounds.frame);
      bounds.frame.copy(bounds.envelope);
      if (swing) {
        bounds.centre.copy(bounds.unarmed);
        bounds.centre.min.multiplyScalar(reference / scale);
        bounds.centre.max.multiplyScalar(reference / scale);
      }
      fit = swing
        ? fitStageFull(bounds.frame, direction, p.width, p.height, STAGE_ARMED_LAYOUT.horizontal, bounds.centre)
        : fitStageFull(bounds.frame, direction, p.width, p.height);
    }
    const goal = { target: fit.target, position: fit.target.clone().addScaledVector(direction, fit.distance), zoom: fit.zoom };
    camera.near = .01; camera.far = fit.distance * 3 + 10; camera.up.set(0, 1, 0);
    this.baseZoom = fit.zoom;
    orbit.minZoom = fit.zoom * .6; orbit.maxZoom = fit.zoom * 4;
    if (resetting && this.animateNext) { this.tween.start(goal); this.animateNext = false; }
    if (this.tween.active) {
      orbit.minZoom = Math.min(camera.zoom, fit.zoom) * .6; orbit.maxZoom = Math.max(camera.zoom, fit.zoom) * 4;
      this.tween.goal!.target.copy(goal.target); this.tween.goal!.position.copy(goal.position); this.tween.goal!.zoom = goal.zoom;
      stepTween();
    } else {
      camera.position.copy(goal.position);
      camera.lookAt(goal.target);
      camera.zoom = goal.zoom;
      camera.updateProjectionMatrix();
      orbit.target.copy(goal.target); this.updateControls();
      markCameraSettled(this.root, true);
    }
    this.previousScale = scale;
    this.pendingReset = false;
  }

  /** Stage へ渡すカメラ（位置・回転・fov・near・far・aspect）。正射影を、視線の後ろへ FAR_LENS 下げた狭い透視に直す（注視点の面で同じ大きさ） */
  write(out: Float64Array) {
    const c = this.camera;
    c.updateMatrixWorld();
    const back = new Vector3(0, 0, 1).applyQuaternion(c.quaternion);
    const visible = (c.top - c.bottom) / c.zoom;
    const along = Math.max(this.controls.target.clone().sub(c.position).dot(back.clone().negate()), 0);
    const fov = 2 * Math.atan(visible / 2 / (FAR_LENS + along)) * 180 / Math.PI;
    const eye = c.position.clone().addScaledVector(back, FAR_LENS);
    out.set([eye.x, eye.y, eye.z, c.quaternion.x, c.quaternion.y, c.quaternion.z, c.quaternion.w, fov, FAR_LENS + c.near, FAR_LENS + c.far, (c.right - c.left) / Math.max(c.top - c.bottom, 1e-6)]);
  }
}
