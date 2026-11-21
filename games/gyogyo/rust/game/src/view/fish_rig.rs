// Fish Kit (CC0) の SWIM_BODY。回転軸と距離はキットの模型座標 (+X が口、+Y が上)。
// 写真の口を閉じた姿勢へ戻してから、呼吸・視線・ひれを動かす。走者の向きと全長をそろえるのはこの後。
use character_actor::glb::Primitive;
use glam::{Quat, Vec2, Vec3};
use serde_json::Value;
use std::f32::consts::{PI, TAU};

struct Vertex {
    part: u8,
    weight: f32,
    angle: f32,
    pivot: Vec3,
    jaw: f32,
    fin_height: f32,
    flutter: Vec2,
    plate: Option<usize>,
}

pub struct FishRig {
    vertices: Vec<Vertex>,
    plate_ids: Vec<f32>,
    jaw_pivot: Vec3,
    upper_pivot: Vec3,
    upper_close: f32,
    upper_open: f32,
    upper_range: f32,
    upper_slide: f32,
    mouth_close: f32,
    eye: Vec2,
    pect_rest: f32,
    fold_max: f32,
    flutter_amp: f32,
    flag_amp: f32,
    still: bool,
    tail_flutter: bool,
    length: f32,
    snout: f32,
}

fn number(v: &Value, key: &str, fallback: f32) -> f32 {
    v[key].as_f64().map(|n| n as f32).unwrap_or(fallback)
}

impl FishRig {
    pub fn load(p: &Primitive, extras: &Value) -> Result<Option<Self>, String> {
        let r = &extras["fishRig"];
        if r.is_null() { return Ok(None); }
        let attr = |name: &str, components: usize| {
            p.custom.get(name).filter(|(c, v)| *c == components && v.len() == p.positions.len() * components)
                .map(|(_, v)| v.as_slice()).ok_or_else(|| format!("魚の可動部: {name} の形式が違います"))
        };
        let fin = attr("_FIN", 3)?;
        let piv = attr("_PIV", 3)?;
        let jaw = attr("_JAW", 2)?;
        let flutter = attr("_FLUTTER", 3)?;
        let height = attr("_FINH", 1)?;
        let mut vertices = Vec::with_capacity(p.positions.len());
        let mut plate_ids = Vec::new();
        for i in 0..p.positions.len() {
            let o = i * 3;
            let part = fin[o].round() as u8;
            let pivot = Vec3::new(piv[o], piv[o + 2], -piv[o + 1]);
            let id = if part == 4 { if pivot.z > 0.0 { 0.31 } else { 0.77 } } else { flutter[o + 2] };
            let plate = if id >= 0.0 && (flutter[o] > 0.0 || height[i] != 0.0) {
                Some(plate_ids.iter().position(|&x| x == id).unwrap_or_else(|| {
                    plate_ids.push(id);
                    plate_ids.len() - 1
                }))
            } else { None };
            vertices.push(Vertex { part, weight: fin[o + 1], angle: fin[o + 2], pivot, jaw: jaw[i * 2],
                fin_height: height[i], flutter: Vec2::new(flutter[o], flutter[o + 1]), plate });
        }
        let vec = |name: &str, i: usize, fallback: f32| r[name][i].as_f64().map(|v| v as f32).unwrap_or(fallback);
        let point = |name| Vec3::new(vec(name, 0, 0.0), vec(name, 1, 0.0), 0.0);
        Ok(Some(Self {
            vertices, plate_ids, jaw_pivot: point("uMouthShape"), upper_pivot: point("uMouthUpper"),
            upper_close: vec("uMouthUpper", 2, 0.0), upper_open: vec("uMouthUpper2", 0, 0.0),
            upper_range: vec("uMouthUpper2", 1, 1.0).max(0.0001), upper_slide: vec("uMouthUpper2", 3, 0.0),
            mouth_close: number(r, "uMouthClose", 0.0), eye: Vec2::new(vec("uEyeBulge", 0, 1.0), vec("uEyeBulge", 1, 0.0)),
            pect_rest: number(r, "uPectRest", 0.0), fold_max: number(r, "foldMax", 1.0),
            flutter_amp: number(r, "finFlutter", 0.008), flag_amp: number(r, "finFlag", 0.035),
            still: r["still"].as_bool().unwrap_or(false), tail_flutter: r["tailFlutter"].as_bool().unwrap_or(false),
            length: number(&extras["swim"], "length", 1.0).max(0.0001), snout: number(&extras["swim"], "snout", 0.0),
        }))
    }
}

fn hash(x: f32) -> f32 {
    let n = ((x as f64 * 12.9898 + 4.1414).sin()) * 43758.5453;
    (n - n.floor()) as f32
}

fn smooth(x: f32) -> f32 {
    let s = x.clamp(0.0, 1.0);
    s * s * (3.0 - 2.0 * s)
}

fn eye_jump(id: f32, period: f32, time: f32) -> Vec2 {
    let k = time / period + hash(id + 1.0) * 7.0;
    let gaze = |n| Vec2::new(hash(n * 1.7 + id), hash(n * 2.3 + id + 11.0));
    gaze(k.floor() - 1.0).lerp(gaze(k.floor()), smooth(k.fract() * period / 0.08)) - 0.5
}

fn eye_gaze(id: f32, side: f32, time: f32, bend: f32) -> Quat {
    let g = eye_jump(id, 0.35 + 0.85 * hash(id), time)
        + 0.3 * eye_jump(id + 5.3, 0.2 + 0.3 * hash(id + 2.0), time)
        + 0.06 * Vec2::new((time * 1.7 + id * 5.0).sin(), (time * 2.3 + id * 3.0).sin());
    Quat::from_rotation_y(side * g.x * 0.9 + bend.clamp(-1.0, 1.0) * 0.3)
        * Quat::from_rotation_x(-side * g.y * 0.5)
}

fn fin_whim(id: f32, t: f32) -> f32 {
    let period = 1.4 + 2.6 * hash(id * 17.0 + 0.3);
    let k = t / period + hash(id * 5.0 + 1.7) * 9.0;
    let value = |seg| if hash(seg * 3.7 + id * 11.0) < 0.45 { 0.0 } else { -0.6 + 1.6 * hash(seg * 1.3 + id * 23.0 + 5.0) };
    let next = k.floor();
    let u = smooth(k.fract() / (0.12 + 0.18 * hash(id + next * 0.37)));
    value(next - 1.0) * (1.0 - u) + value(next) * u
}

#[derive(Default, Clone, Copy)]
struct PlatePose {
    fold: f32,
    tilt: f32,
    flag_phase: f32,
}

#[derive(Default)]
pub struct RigMotion {
    time: f32,
    fin_time: f32,
    mouth: f32,
    fin_fold: f32,
    plates: Vec<PlatePose>,
}

pub struct Pose<'a> {
    rig: &'a FishRig,
    plates: &'a [PlatePose],
    eyes: [Quat; 2],
    lower_angle: f32,
    upper_rotation: Quat,
    upper_slide: Vec3,
    gill: f32,
    gill_wave: (f32, f32),
    dorsal: f32,
    pelvic: f32,
    tail: f32,
    pect: [f32; 2],
    flap_phase: f32,
    flap_amp: f32,
    flutter_amp: f32,
    flutter_time: f32,
    flag_amp: f32,
}

// 選択画面は元キットの「動くところ」(range) の4秒周期で、レース中は元の毎秒0.8回の呼吸。
impl RigMotion {
    #[allow(clippy::too_many_arguments)]
    pub fn advance<'a>(&'a mut self, r: &'a FishRig, dt: f32, cycle: f32, amplitude: f32,
        speed_bl: f32, bend: f32, air: bool, showcase: bool) -> Pose<'a> {
        self.time += dt;
        let flow = (speed_bl / 3.0).clamp(0.0, 1.0);
        self.fin_time += dt * (1.0 + 0.6 * flow);
        let t = self.time;
        let clock = self.fin_time;
        let seed = r.snout * 17.0;
        let individuality = hash(seed);
        let breath = TAU * (t * 0.8 + individuality);
        let max_mouth = r.mouth_close + 0.35;
        let fast = smooth((speed_bl - 1.5) / 2.5);
        let (fold, sway, wobble) = if air { (0.3, 0.4, 0.6) } else { (0.4 + 0.35 * fast, 0.2 - 0.14 * fast, 0.6 - 0.2 * fast) };
        self.fin_fold += (fold - self.fin_fold) * (dt * 3.0).min(1.0);
        let (gill, mouth, fold, sway, wobble, whim, pect) = if showcase {
            let s = 0.5 - 0.5 * (t * PI * 0.5).cos();
            (0.35 * s, max_mouth * s, 1.0 - s, 0.4 * s, 0.0, 0.0, -0.6 + 1.02 * s)
        } else {
            self.mouth += (0.6 * max_mouth * (0.5 - 0.5 * (breath + 1.4).cos()) - self.mouth) * (dt * 12.0).min(1.0);
            let effort = if air { 1.5 } else { 1.0 };
            (0.35 * (0.5 - 0.5 * breath.cos()) * effort, self.mouth * effort, self.fin_fold, sway, wobble, 0.55, 0.6 * (0.25 - 0.15 * self.fin_fold))
        };
        let slow = 1.0 - flow;
        let pect_side = |side: f32| {
            if r.still { return r.pect_rest; }
            let hz = 0.2 + 0.3 * hash(side * 3.1 + individuality * 7.0);
            let phase = if side > 0.0 { 0.2 } else { 0.7 };
            let id = if side > 0.0 { 0.13 } else { 0.57 };
            r.pect_rest + 0.03 + slow * 0.08 + pect + wobble * 0.36 * (TAU * (hz * clock + phase)).sin()
                - whim * 0.45 * fin_whim(id + individuality * 3.1, clock)
        };
        self.plates.resize(r.plate_ids.len(), PlatePose::default());
        for (&id, plate) in r.plate_ids.iter().zip(&mut self.plates) {
            let phase = |hz, salt| TAU * (hz * clock + hash(salt));
            let tilt_phase = phase(0.35 + 0.45 * hash(id * 5.0 + individuality * 11.0), id * 3.0 + individuality * 5.0);
            let fold_phase = phase(0.2 + 0.4 * hash(id * 9.0 + individuality * 13.0), id * 4.0 + individuality * 3.0);
            plate.tilt = if r.still { 0.0 } else { sway * (tilt_phase.sin() + 0.5 * (2.3 * tilt_phase + 1.1).sin()) + bend.clamp(-0.35, 0.35) };
            plate.fold = if r.still { 0.0 } else { (fold + wobble * (0.7 * fold_phase.sin() + 0.3 * (2.6 * fold_phase + 0.8).sin())
                + whim * fin_whim(id + individuality * 3.1, clock)).clamp(0.0, 1.0) * r.fold_max };
            plate.flag_phase = phase(1.1 + 1.2 * hash(id * 13.0 + individuality * 31.0), id * 7.0 + individuality * 17.0 + 3.0);
        }
        let opening = (mouth / r.upper_range).clamp(0.0, 1.0).powi(2);
        let flutter = !r.still || r.tail_flutter;
        Pose {
            rig: r, plates: &self.plates, eyes: [eye_gaze(seed * 7.13 + 3.7, -1.0, t, bend), eye_gaze(seed * 7.13, 1.0, t, bend)],
            lower_angle: r.mouth_close - mouth.min(max_mouth), upper_rotation: Quat::from_rotation_z(r.upper_open * opening - r.upper_close),
            upper_slide: Vec3::new(0.94, -0.34, 0.0) * r.upper_slide * opening, gill, gill_wave: gill.sin_cos(),
            pect: [pect_side(-1.0), pect_side(1.0)], flap_phase: TAU * cycle * 0.8,
            flap_amp: if r.still { 0.0 } else { slow * 0.3 }, dorsal: if r.still { 0.0 } else { 1.0 - (0.15 + slow * 0.6) },
            pelvic: -0.12 + (if r.still { 0.0 } else { 0.67 * slow * 0.5 }) + 0.04 * (4.4 * t + seed).sin(),
            tail: 0.22 * (1.0 - (amplitude * 1.5).clamp(0.0, 1.0)), flutter_time: clock,
            flutter_amp: if flutter { r.flutter_amp * (1.0 + 0.4 * flow) } else { 0.0 },
            flag_amp: if flutter { r.flag_amp * (0.6 + 0.7 * flow) } else { 0.0 },
        }
    }
}

impl Pose<'_> {
    pub fn deform(&self, index: usize, mut p: Vec3, mut n: Vec3) -> (Vec3, Vec3) {
        let r = self.rig;
        let v = &r.vertices[index];
        let side = v.pivot.z.signum();
        let eye_side = usize::from(side > 0.0);
        let rotation = match v.part {
            1 | 2 => Quat::from_rotation_z(v.angle * self.dorsal),
            3 => Quat::from_rotation_z(v.angle * self.tail),
            4 => {
                let a = self.pect[eye_side] + self.flap_amp * (0.5 - 0.5 * (self.flap_phase - v.weight * 0.9).cos());
                Quat::from_axis_angle(Vec3::new(0.19611613, 0.9805807, 0.0), side * v.weight * a.max(0.0))
            }
            5 => Quat::from_rotation_x(-side * v.weight * self.pelvic),
            6 => {
                p = v.pivot + (p - v.pivot) * r.eye.x + Vec3::Z * side * r.eye.y;
                self.eyes[eye_side]
            }
            7 => {
                let side = p.z.signum();
                p.x += v.weight * (1.0 - self.gill_wave.1);
                p.z += side * v.weight * self.gill_wave.0;
                n = Quat::from_rotation_y(side * self.gill * (v.weight / (0.05 * r.length)).clamp(0.0, 1.0)) * n;
                return (p, n);
            }
            9 => {
                let rot = self.upper_rotation;
                return (r.upper_pivot + rot * (p + self.upper_slide - r.upper_pivot), rot * n);
            }
            _ => {
                let w = if v.part == 8 { 1.0 } else { v.jaw };
                if w > 0.0 {
                    let rot = Quat::from_rotation_z(self.lower_angle * w);
                    return (r.jaw_pivot + rot * (p - r.jaw_pivot), rot * n);
                }
                return (p, n);
            }
        };
        if v.pivot.length_squared() > 1.0e-10 || matches!(v.part, 4 | 5) {
            p = v.pivot + rotation * (p - v.pivot);
            n = rotation * n;
        }
        let plate = v.plate.map(|k| self.plates[k]);
        if v.fin_height != 0.0 && v.part != 4 && let Some(plate) = plate {
            let (fs, fc) = plate.fold.sin_cos();
            let rot = Quat::from_rotation_x(v.fin_height.signum() * plate.tilt);
            p -= Vec3::Y * v.fin_height;
            p += rot * Vec3::new(-v.fin_height.abs() * fs, v.fin_height * fc, 0.0);
            n = rot * n;
        }
        let w = v.flutter.x;
        if w > 0.0 && self.flutter_amp > 0.0 {
            let along = (r.snout - p.x) / r.length;
            let arg = TAU * (2.5 * self.flutter_time - along * 2.5 - p.y / r.length * 1.5);
            let (s, c) = arg.sin_cos();
            let (s2, c2) = (1.7 * arg + 1.3).sin_cos();
            let amp = r.length * self.flutter_amp * w * w.sqrt() * if v.part == 4 { 0.1 } else { 1.0 };
            let mut dz = amp * (s + 0.35 * s2);
            if let Some(plate) = plate {
                let ph = plate.flag_phase - TAU * (v.flutter.y * 0.8 + w * 0.3);
                let edge = w * (0.35 + 0.65 * v.flutter.y);
                let flag = r.length * self.flag_amp * edge * edge * if v.part == 4 { 0.6 } else { 1.0 };
                dz += flag * (ph.sin() + 0.45 * (1.73 * ph + 2.1 + TAU * 0.37 * self.flutter_time).sin());
            }
            if v.part == 4 { p += n * dz; }
            else {
                p.z += dz;
                n.x -= amp * (c + 0.6 * c2) * TAU * 2.5 / r.length * n.z;
                n = n.normalize_or_zero();
            }
        }
        (p, n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use character_actor::glb;

    #[test]
    fn shipped_fish_move_their_jaws_eyes_gills_and_fins_independently_of_the_body_wave() {
        for species in ["madai", "buri", "kihada", "saba", "aji", "kasago", "tachiuo", "magochi", "kurodai", "suzuki"] {
            let file = format!("{}/../../public/models/fish/{species}.glb", env!("CARGO_MANIFEST_DIR"));
            let g = glb::parse(&std::fs::read(file).unwrap()).unwrap();
            let node = g.nodes.iter().find(|n| n.mesh.is_some()).unwrap();
            let p = &g.meshes[node.mesh.unwrap()].primitives[0];
            let rig = FishRig::load(p, &node.extras).unwrap().unwrap();
            for showcase in [true, false] {
                let mut motion = RigMotion::default();
                let mut first = vec![Vec3::ZERO; p.positions.len()];
                let mut travel = [0.0_f32; 10];
                for frame in 0..=120 {
                    let pose = motion.advance(&rig, 1.0 / 60.0, frame as f32 / 30.0, 0.75,
                        if showcase { 0.0 } else { 4.0 }, 0.0, false, showcase);
                    if ![0, 15, 41, 80, 120].contains(&frame) { continue; }
                    for (i, &raw) in p.positions.iter().enumerate() {
                        let (x, normal) = pose.deform(i, raw.into(), p.normals[i].into());
                        assert!(x.is_finite() && normal.is_finite(), "{species} vertex {i}");
                        if frame == 0 { first[i] = x; }
                        let part = rig.vertices[i].part as usize;
                        travel[part] = travel[part].max(x.distance(first[i]) / rig.length);
                    }
                }
                // 体の向きや尾の全体的な揺れを使わず、元の部位番号ごとに変形を確かめる。
                for (part, name) in [(1, "dorsal fin"), (4, "pectoral fin"), (6, "eye"), (7, "gill"), (8, "lower jaw"), (9, "upper jaw")] {
                    assert!(travel[part] > 0.00001, "{species} {name}, showcase={showcase}, travel={travel:?}");
                }
                eprintln!("{species:7} select={showcase} (body lengths) fin={:.5} eye={:.5} gill={:.5} jaw={:.5} upper={:.5}",
                    travel[4], travel[6], travel[7], travel[8], travel[9]);
            }
        }
    }
}
