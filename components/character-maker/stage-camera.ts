import { useMemo, useRef, type PointerEvent as ReactPointerEvent } from 'react';
import { Box3, MathUtils, Spherical, Vector3, type Camera, type Object3D } from 'three';

/** Studio camera rules shared by both editors (spec §5.3). */
export const STAGE_TILT = MathUtils.degToRad(7);
/** Damping rate for the turntable arc (≈330 ms to settle). */
export const STAGE_CAMERA_LAMBDA = 9;
/** Full view: feet sit 6% above the canvas bottom; the max-size frame keeps a little headroom. */
export const STAGE_FULL_LAYOUT = { bottom: .06, top: .015, horizontal: 1.06 };
/** Face view: padding 1.12 with the head centre 45% from the top. */
export const STAGE_FACE_LAYOUT = { padding: 1.12, headFromTop: .45 };
/** Armed full view: the swing envelope is already the sampled extreme (12 Hz over both
 * clips), so it gets a tighter side margin. Sizes still compare in the max-size frame. */
export const STAGE_ARMED_LAYOUT = { horizontal: 1.02 };

/** Unit vector from target to camera for an azimuth (radians around +Y, 0 = +Z) with the stage tilt. */
export function stageDirection(azimuth: number, tilt = STAGE_TILT) {
  return new Vector3(Math.sin(azimuth) * Math.cos(tilt), Math.sin(tilt), Math.cos(azimuth) * Math.cos(tilt));
}

const corner = new Vector3();
/** Project a box onto the camera's right/up axes for a given view direction. */
export function projectBox(box: Box3, direction: Vector3) {
  const up = new Vector3(0, 1, 0), right = new Vector3().crossVectors(up, direction).normalize();
  const trueUp = new Vector3().crossVectors(direction, right).normalize();
  let r0 = Infinity, r1 = -Infinity, u0 = Infinity, u1 = -Infinity;
  for (let i = 0; i < 8; i++) {
    corner.set(i & 1 ? box.max.x : box.min.x, i & 2 ? box.max.y : box.min.y, i & 4 ? box.max.z : box.min.z);
    const r = corner.dot(right), u = corner.dot(trueUp);
    r0 = Math.min(r0, r); r1 = Math.max(r1, r); u0 = Math.min(u0, u); u1 = Math.max(u1, u);
  }
  return { right, up: trueUp, r0, r1, u0, u1 };
}

const soles = new Vector3();
/** Orthographic full-body fit: horizontal padding, feet at a fixed fraction above the bottom edge.
 * `centre` (the body without its weapon) pulls the horizontal centre toward the body, clamped so the
 * whole `box` envelope stays inside the visible width. */
export function fitStageFull(box: Box3, direction: Vector3, width: number, height: number, horizontal = STAGE_FULL_LAYOUT.horizontal, centre?: Box3) {
  const { right, up, r0, r1, u0, u1 } = projectBox(box, direction);
  const { bottom, top } = STAGE_FULL_LAYOUT;
  const spanR = Math.max(r1 - r0, .01), spanU = Math.max(u1 - u0, .01);
  const zoom = Math.min(Math.max(width, 1) / (spanR * horizontal), Math.max(height, 1) / (spanU / (1 - bottom - top)));
  const visible = Math.max(height, 1) / zoom;
  // The feet sit on the same floor line for every character; any spare
  // vertical room (wide armed poses, portrait canvases) goes above the head.
  // The line is the soles of a character standing at the world origin (toes
  // reaching 8% of the height toward the lens): on the tilted lens a box corner
  // would drop with every tail, cape or blade that makes the box deep. The
  // envelope may still dip into the floor margin, but never below 30% of it.
  const body = centre && !centre.isEmpty() ? projectBox(centre, direction) : null;
  const stand = centre && !centre.isEmpty() ? centre : box;
  soles.set(direction.x, 0, direction.z).normalize().multiplyScalar((stand.max.y - stand.min.y) * .08).setY(stand.min.y);
  const floor = Math.min(Math.max(u0, soles.dot(up)), u0 + bottom * visible * .7);
  const centreU = floor - bottom * visible + visible / 2;
  let centreR = (r0 + r1) / 2;
  if (body) {
    const visW = Math.max(width, 1) / zoom, m = visW * .01;
    const lo = r1 - visW / 2 + m, hi = r0 + visW / 2 - m;
    // Optical centre: halfway between the body and the whole silhouette (body + prop),
    // so a held weapon neither pushes the body aside nor leaves one side empty.
    const optical = ((body.r0 + body.r1) / 2 + centreR) / 2;
    centreR = lo <= hi ? Math.min(Math.max(optical, lo), hi) : centreR;
  }
  const target = box.getCenter(new Vector3());
  target.addScaledVector(up, centreU - target.dot(up)).addScaledVector(right, centreR - target.dot(right));
  return { target, zoom, distance: Math.max(5, box.getSize(new Vector3()).length() * 1.5) };
}

/** Orthographic face fit: padding around the head bounds, head anchor at 45% from the top. */
export function fitStageFace(box: Box3, anchor: Vector3, direction: Vector3, width: number, height: number) {
  const { visible, target } = stageFaceWindow(box, anchor, direction, Math.max(width, 1) / Math.max(height, 1));
  return { target, zoom: Math.max(height, 1) / visible, distance: Math.max(5, box.getSize(new Vector3()).length() * 1.5) };
}

/** Face window shared by both lenses: visible world height and the look-at target.
 * `lens` > 1 leaves room for perspective growth of the near/top edges. The crown
 * keeps ≥2% headroom even when that nudges the head centre slightly below 45%. */
export function stageFaceWindow(box: Box3, anchor: Vector3, direction: Vector3, aspect: number, lens = 1) {
  const { right, up, r0, r1, u0, u1 } = projectBox(box, direction);
  const { padding, headFromTop } = STAGE_FACE_LAYOUT;
  const spanR = Math.max(r1 - r0, .01), spanU = Math.max(u1 - u0, .01);
  const visible = Math.max(spanU * padding * lens, spanR * padding * lens / Math.max(aspect, .1));
  // Screen centre sits below the head anchor by (0.5 - headFromTop) of the view.
  const centreU = Math.max(anchor.dot(up) - (.5 - headFromTop) * visible, u1 - .48 * visible), centreR = (r0 + r1) / 2;
  const target = anchor.clone();
  target.addScaledVector(up, centreU - target.dot(up)).addScaledVector(right, centreR - target.dot(right));
  return { visible, target };
}

/** Automated tests and reduced-motion users get instant camera cuts. */
export function prefersInstantCamera() {
  if (typeof window === 'undefined') return true;
  try {
    if (navigator.webdriver) return true;
    return window.matchMedia?.('(prefers-reduced-motion: reduce)').matches ?? false;
  } catch { return true; }
}

export type CameraGoal = { target: Vector3; position: Vector3; zoom: number };

/** Turntable interpolation: target and zoom damp linearly, the camera offset damps on a sphere so view changes arc around the subject. */
export class StageCameraTween {
  goal: CameraGoal | null = null;
  private from = new Spherical(); private to = new Spherical();
  private offset = new Vector3(); private elapsed = 0;
  start(goal: CameraGoal) { this.goal = { target: goal.target.clone(), position: goal.position.clone(), zoom: goal.zoom }; this.elapsed = 0; }
  stop() { this.goal = null; }
  get active() { return this.goal !== null; }
  /** Move the goal with a followed subject without restarting the arc. */
  translate(delta: Vector3) { if (this.goal) { this.goal.target.add(delta); this.goal.position.add(delta); } }
  /** Returns true once settled. Writes camera position/zoom and target. */
  step(camera: Camera & { zoom?: number }, target: Vector3, delta: number) {
    const goal = this.goal;
    if (!goal) return true;
    this.elapsed += delta;
    const t = 1 - Math.exp(-STAGE_CAMERA_LAMBDA * Math.min(delta, .05));
    this.from.setFromVector3(this.offset.copy(camera.position).sub(target));
    this.to.setFromVector3(this.offset.copy(goal.position).sub(goal.target));
    let dTheta = this.to.theta - this.from.theta;
    if (dTheta > Math.PI) dTheta -= Math.PI * 2; else if (dTheta < -Math.PI) dTheta += Math.PI * 2;
    target.lerp(goal.target, t);
    this.from.theta += dTheta * t;
    this.from.phi += (this.to.phi - this.from.phi) * t;
    this.from.radius += (this.to.radius - this.from.radius) * t;
    camera.position.setFromSpherical(this.from).add(target);
    if (typeof camera.zoom === 'number') camera.zoom += (goal.zoom - camera.zoom) * t;
    const settled = this.elapsed > .5 || (Math.abs(dTheta) < 2e-3 && Math.abs(this.to.phi - this.from.phi) < 2e-3
      && target.distanceToSquared(goal.target) < 1e-8 && (typeof camera.zoom !== 'number' || Math.abs(goal.zoom - camera.zoom) < goal.zoom * 1e-3));
    if (settled) {
      target.copy(goal.target); camera.position.copy(goal.position);
      if (typeof camera.zoom === 'number') camera.zoom = goal.zoom;
      this.goal = null;
    }
    camera.lookAt(target);
    return settled;
  }
}

/** Display-only: hide registered weapon groups (face view). Never touches saved data or exports. */
export function setRegisteredWeaponsVisible(root: Object3D | null | undefined, visible: boolean) {
  if (!root) return;
  root.traverse(object => { if (object.userData?.registeredWeapon !== undefined) object.visible = visible; });
}

/** Double-click, or two taps within 320 ms on touch, toggles full ⇄ face (spec §5.3). */
export function useStageDoubleTap(onToggle: () => void) {
  const last = useRef({ time: 0, x: 0, y: 0 });
  const toggle = useRef(onToggle);
  toggle.current = onToggle;
  return useMemo(() => ({
    onDoubleClick: () => toggle.current(),
    onPointerUp: (event: ReactPointerEvent<HTMLElement>) => {
      if (event.pointerType !== 'touch') return;
      const now = event.timeStamp, previous = last.current;
      if (now - previous.time < 320 && Math.hypot(event.clientX - previous.x, event.clientY - previous.y) < 24) {
        last.current = { time: 0, x: 0, y: 0 };
        toggle.current();
      } else last.current = { time: now, x: event.clientX, y: event.clientY };
    },
  }), []);
}

/** Write data-camera-settled only when it changes. */
export function markCameraSettled(element: HTMLElement | null | undefined, settled: boolean) {
  if (!element) return;
  const value = settled ? 'true' : 'false';
  if (element.dataset.cameraSettled !== value) element.dataset.cameraSettled = value;
}
