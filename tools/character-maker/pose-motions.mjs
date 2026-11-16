/** 原本（UAL1）に無い動作を、姿勢から作る。docs/game-motions-spec.md
 * バンザイ・手を振る・地面に座る（座る → 座って待つ → 立つ）・回転キック・食べる。
 *
 * UAL1 の骨（root・pelvis・spine_01〜03・neck_01・Head・clavicle・upperarm・lowerarm・hand・thigh・calf・foot・ball。左 = +X、上 = +Y、前 = +Z）の上で、
 * 待機の最初の姿から「骨を世界のこの向きへ向ける」で姿勢を作り、姿勢の間をなめらかにつなぐ。出来た各コマは UAL1 のコマと同じ形
 * （{ locals, worlds }）なので、UAL1 の動作と同じ道具（build-action-motions.mjs・build-original-action-motions.mjs）が各体の骨へ移す。
 * 名前は characters の主役の動作の名前（banzai・greet・restDown・restLoop・restUp・spinKick・eat）と同じ。
 */
import { Matrix4, Quaternion, Vector3 } from 'three';

const FPS = 15;
const rotation = m => { const q = new Quaternion(); m.decompose(new Vector3(), q, new Vector3()); return q.normalize(); };
const position = m => new Vector3().setFromMatrixPosition(m);
const ease = x => { const t = Math.min(1, Math.max(0, x)); return t * t * (3 - 2 * t); };
const copy = locals => new Map([...locals].map(([name, l]) => [name, { t: [...l.t], q: [...l.q], s: [...l.s] }]));
const child = { clavicle: 'upperarm', upperarm: 'lowerarm', lowerarm: 'hand', hand: 'middle_01', thigh: 'calf', calf: 'foot', foot: 'ball' };
const Y = new Vector3(0, 1, 0), X = new Vector3(1, 0, 0), Z = new Vector3(0, 0, 1);

/** 姿勢を組み立てる道具。skel: 原本の骨（rest・ordered）、evaluate: (skel, locals) → 骨の世界の行列 */
function poser(skel, evaluate, base) {
  const worldQ = (worlds, name) => rotation(worlds.get(name));
  const parentQ = (worlds, name) => { const parent = skel.rest.get(name).parent; return worlds.has(parent) ? worldQ(worlds, parent) : rotation(skel.rest.get(name).parentWorld); };
  const api = {
    /** 待機の最初の姿から始める */
    start: () => copy(base),
    /** 骨（'upperarm_l' など）を、子の骨が世界の dir の向きに来るように回す */
    aim(locals, bone, dir) {
      const [part, side] = bone.split('_');
      const worlds = evaluate(skel, locals), tip = `${child[part]}_${side}`;
      const now = position(worlds.get(tip)).sub(position(worlds.get(bone))).normalize();
      const q = new Quaternion().setFromUnitVectors(now, new Vector3(...dir).normalize()).multiply(worldQ(worlds, bone));
      locals.get(bone).q = parentQ(worlds, bone).invert().multiply(q).normalize().toArray();
      return api;
    },
    /** 左右の同じ骨を、左右対称の向きへ（dir は左。右は x を反転） */
    both(locals, part, dir) { api.aim(locals, `${part}_l`, dir); api.aim(locals, `${part}_r`, [-dir[0], dir[1], dir[2]]); return api; },
    /** 骨を世界の軸のまわりに回す（度）。子の骨もいっしょに回る */
    turn(locals, bone, axis, degrees) {
      const worlds = evaluate(skel, locals);
      const q = new Quaternion().setFromAxisAngle(axis, degrees * Math.PI / 180).multiply(worldQ(worlds, bone));
      locals.get(bone).q = parentQ(worlds, bone).invert().multiply(q).normalize().toArray();
      return api;
    },
    /** 腰を動かす（m。世界の向き） */
    hips(locals, dx, dy, dz) {
      // 腰の位置は親（root）から見た値なので、世界の向きの差を親の向きへ直して足す
      const worlds = evaluate(skel, locals), d = new Vector3(dx, dy, dz).applyQuaternion(parentQ(worlds, 'pelvis').invert()), t = locals.get('pelvis').t;
      t[0] += d.x; t[1] += d.y; t[2] += d.z; return api;
    },
  };
  return api;
}

/** 2 つの姿勢の間（w = 0 で a、1 で b） */
function blend(a, b, w) {
  const out = new Map();
  for (const [name, la] of a) {
    const lb = b.get(name);
    out.set(name, { t: la.t.map((v, k) => v + (lb.t[k] - v) * w), q: new Quaternion().fromArray(la.q).slerp(new Quaternion().fromArray(lb.q), w).normalize().toArray(), s: [...la.s] });
  }
  return out;
}
/** 姿勢の列（[秒, 姿勢]）を時刻 t で読む。間はなめらかにつなぐ */
function at(keys, t) {
  if (t <= keys[0][0]) return copy(keys[0][1]);
  for (let i = 0; i + 1 < keys.length; i += 1) if (t <= keys[i + 1][0]) return blend(keys[i][1], keys[i + 1][1], ease((t - keys[i][0]) / (keys[i + 1][0] - keys[i][0])));
  return copy(keys.at(-1)[1]);
}

/** 動作の一覧: 名前 → { duration, floor, pose(t) }。floor: 'snap' は毎コマ足もとを床へ合わせる（座る動作） */
function motions(p, options = {}) {
  const idle = p.start();
  // ---- バンザイ ----
  const crouch = p.start(); p.hips(crouch, 0, -0.05, 0).turn(crouch, 'spine_01', X, 7).both(crouch, 'upperarm', [0.22, -0.94, -0.26]).both(crouch, 'lowerarm', [0.16, -0.9, 0.2]);
  // 頭の大きい体（ルクス・モカ）は、腕が頭に隠れないように外へ広い V 字にする（options.wideArms）
  const [arm, fore, armLow] = options.wideArms ? [[0.74, 0.66, 0.14], [0.6, 0.78, 0.18], [0.8, 0.58, 0.14]] : [[0.4, 0.91, 0.08], [0.3, 0.95, 0.06], [0.48, 0.87, 0.1]];
  const up = p.start(); p.turn(up, 'spine_02', X, -5).turn(up, 'Head', X, -9).both(up, 'clavicle', [0.86, 0.5, -0.1]).both(up, 'upperarm', arm).both(up, 'lowerarm', fore).both(up, 'hand', fore);
  const upLow = copy(up); p.hips(upLow, 0, -0.022, 0).both(upLow, 'upperarm', armLow);
  // ---- 手を振る（本人の右手 = -X） ----
  const raise = sway => { const l = p.start(); p.turn(l, 'spine_03', Z, -2).turn(l, 'Head', Z, 5).aim(l, 'clavicle_r', [-0.9, 0.42, 0.05]).aim(l, 'upperarm_r', [-0.8, 0.42, 0.42]).aim(l, 'lowerarm_r', [-0.26 + sway, 0.93, 0.26]).aim(l, 'hand_r', [-0.2 + sway * 1.5, 0.96, 0.2]); return l; };
  const wave = [raise(0), raise(-0.34), raise(0.3), raise(-0.34), raise(0.3), raise(0)];
  // ---- 地面に座る（あぐら） ----
  const sit = lean => {
    const l = p.start();
    p.hips(l, 0, -0.72, -0.06).turn(l, 'spine_01', X, 6 + lean).turn(l, 'Head', X, -4 - lean * 0.6)
      .both(l, 'thigh', [0.74, 0.04, 0.67]).both(l, 'calf', [-0.86, -0.16, 0.48]).both(l, 'foot', [-0.5, -0.1, 0.86])
      .both(l, 'upperarm', [0.3, -0.86, 0.4]).both(l, 'lowerarm', [0.34, -0.36, 0.87]).both(l, 'hand', [0.3, -0.5, 0.8]);
    return l;
  };
  const seated = sit(0), seatedBreath = sit(1.6);
  const squat = p.start(); p.hips(squat, 0, -0.42, -0.1).turn(squat, 'spine_01', X, 26).turn(squat, 'Head', X, -14).both(squat, 'thigh', [0.3, -0.34, 0.89]).both(squat, 'calf', [-0.06, -0.93, -0.36]).both(squat, 'upperarm', [0.2, -0.7, 0.68]).both(squat, 'lowerarm', [0.1, -0.5, 0.86]);
  // ---- 食べる（座ったまま、右手 → 左手の順に口へ） ----
  const bite = side => {
    const l = sit(2.5), x = side === 'r' ? -1 : 1;
    p.aim(l, `upperarm_${side}`, [0.34 * x, -0.42, 0.84]).aim(l, `lowerarm_${side}`, [-0.5 * x, 0.76, 0.42]).aim(l, `hand_${side}`, [-0.6 * x, 0.7, 0.4]).turn(l, 'Head', X, 7);
    return l;
  };
  const biteR = bite('r'), biteL = bite('l');
  // ---- 回転キック（体を 1 回まわし、本人の右脚を横へ伸ばす） ----
  const spin = t => {
    const wind = ease(t / 0.22) * (1 - ease((t - 0.22) / 0.12)), go = ease((t - 0.2) / 0.62), kick = ease((t - 0.28) / 0.2) * (1 - ease((t - 0.7) / 0.2)), end = ease((t - 0.86) / 0.24);
    const yaw = -32 * wind + 360 * go;
    const l = p.start();
    p.hips(l, 0, -0.07 * wind - 0.03 * kick, 0).turn(l, 'spine_02', Z, 14 * kick).both(l, 'upperarm', [0.72 - 0.3 * (1 - kick), -0.62 + 0.5 * kick, 0.3 * kick]);
    if (kick > 0.01) {
      const reach = [-0.2 - 0.76 * kick, -0.97 + 0.99 * kick, 0.1 + 0.16 * kick];
      p.aim(l, 'thigh_r', reach).aim(l, 'calf_r', [reach[0], reach[1] - 0.06 * kick, reach[2]]).aim(l, 'thigh_l', [0.06, -0.98, 0.14 * kick]).aim(l, 'calf_l', [0.02, -0.99, -0.12 * kick]);
    }
    p.turn(l, 'pelvis', Y, yaw);
    return end > 0 ? blend(l, idle, end) : l;
  };
  return {
    banzai: { duration: 2.4, pose: t => at([[0, idle], [0.3, crouch], [0.62, up], [0.92, upLow], [1.2, up], [1.48, upLow], [1.76, up], [2.4, idle]], t) },
    greet: { duration: 2.2, pose: t => at([[0, idle], [0.42, wave[0]], [0.66, wave[1]], [0.9, wave[2]], [1.14, wave[3]], [1.38, wave[4]], [1.62, wave[5]], [2.2, idle]], t) },
    restDown: { duration: 1.2, floor: 'snap', pose: t => at([[0, idle], [0.55, squat], [1.2, seated]], t) },
    restLoop: { duration: 3.2, floor: 'snap', pose: t => at([[0, seated], [1.6, seatedBreath], [3.2, seated]], t) },
    restUp: { duration: 1.2, floor: 'snap', pose: t => at([[0, seated], [0.65, squat], [1.2, idle]], t) },
    eat: { duration: 4, floor: 'snap', pose: t => at([[0, seated], [0.7, biteR], [1.25, biteR], [2, seated], [2.7, biteL], [3.25, biteL], [4, seated]], t) },
    spinKick: { duration: 1.1, pose: spin },
  };
}

export const POSED_NAMES = ['banzai', 'greet', 'restDown', 'restLoop', 'restUp', 'eat', 'spinKick'];

/** 姿勢から作る動作のコマ: 名前 → { floor, frames: [{ time, frame: { locals, worlds } }] }。base は待機の最初の姿（sample(source, Idle_Loop, 0).locals） */
export function posedMotions(skel, evaluate, base, options = {}) {
  const defs = motions(poser(skel, evaluate, base), options);
  return Object.fromEntries(Object.entries(defs).map(([name, def]) => {
    const count = Math.round(def.duration * FPS);
    const frames = Array.from({ length: count + 1 }, (_, i) => {
      const time = Number((i / FPS).toFixed(5)), locals = def.pose(Math.min(time, def.duration));
      return { time, frame: { locals, worlds: evaluate(skel, locals) } };
    });
    return [name, { floor: def.floor ?? null, frames }];
  }));
}
