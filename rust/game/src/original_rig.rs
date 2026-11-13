// オリジナル 4 体の小さな動き（今の版は components/rig-actor/generic/hair-springs.ts・collar-attachment.ts・tail-motion.ts・blink.ts・mouth.ts）。
// 骨の値は character_actor::anim::Skeleton に書く。世界の行列は呼ぶ側の根（base）から作る。式と順は今の版と同じ。
use character_actor::anim::{Skeleton, three_decompose};
use character_actor::glb::Glb;
use glam::{Mat4, Quat, Vec3};
use serde_json::Value;

pub const HAIR_STEP: f64 = 1.0 / 120.0;

fn q4(v: &Value) -> Quat { Quat::from_xyzw(v[0].as_f64().unwrap_or(0.0) as f32, v[1].as_f64().unwrap_or(0.0) as f32, v[2].as_f64().unwrap_or(0.0) as f32, v[3].as_f64().unwrap_or(1.0) as f32) }
fn pos(s: &Skeleton, i: usize) -> Vec3 { s.world[i].w_axis.truncate() }

/// three の Quaternion.setFromUnitVectors
fn from_unit_vectors(from: Vec3, to: Vec3) -> Quat {
    let (vf, vt) = (from.as_dvec3(), to.as_dvec3());
    let mut r = vf.dot(vt) + 1.0;
    let q = if r < 1e-8 {
        r = 0.0;
        if vf.x.abs() > vf.z.abs() { glam::DVec4::new(-vf.y, vf.x, 0.0, r) } else { glam::DVec4::new(0.0, -vf.z, vf.y, r) }
    } else {
        glam::DVec4::new(vf.y * vt.z - vf.z * vt.y, vf.z * vt.x - vf.x * vt.z, vf.x * vt.y - vf.y * vt.x, r)
    };
    let q = q.normalize();
    Quat::from_xyzw(q.x as f32, q.y as f32, q.z as f32, q.w as f32)
}

/// 髪の揺れ（CharacterHairSprings。VRMC_springBone の尾の長さの形の、減衰する位置のばねと小さな角度の制限）
pub struct HairSprings { joints: Vec<Joint> }
struct Joint { bone: usize, child: usize, frequency: f64, damping: f64, max_angle: f64, rest: Quat, axis: Vec3, current: Vec3, previous: Vec3 }

impl HairSprings {
    pub fn new(g: &Glb, skel: &Skeleton, order: &[usize]) -> Result<HairSprings, String> {
        let mut joints = Vec::new();
        for &i in order {
            let Some(node) = g.nodes.get(i) else { continue };
            let s = &node.extras["aisnsHairJoint"];
            if s.is_null() { continue; }
            let child_name = s["child"].as_str().unwrap_or("");
            let child = skel.find(child_name).ok_or(format!("Missing hair endpoint: {child_name}"))?;
            joints.push(Joint { bone: i, child, frequency: s["frequency"].as_f64().unwrap_or(0.0), damping: s["dampingRatio"].as_f64().unwrap_or(0.0), max_angle: s["maxAngle"].as_f64().unwrap_or(0.0),
                rest: q4(&s["restRotation"]), axis: skel.t[child].normalize_or_zero(), current: Vec3::ZERO, previous: Vec3::ZERO });
        }
        Ok(HairSprings { joints })
    }
    fn restore(&self, skel: &mut Skeleton, base: Mat4) {
        for j in &self.joints { skel.r[j.bone] = j.rest; }
        skel.update_world(base);
    }
    pub fn reset(&mut self, skel: &mut Skeleton, base: Mat4) {
        if self.joints.is_empty() { return; }
        self.restore(skel, base);
        for j in &mut self.joints { j.current = pos(skel, j.child); j.previous = j.current; }
    }
    /// 体の姿は呼ぶ側が同じ刻みで先に進める
    pub fn update(&mut self, skel: &mut Skeleton, base: Mat4, dt: f64, strength: f64) {
        if self.joints.is_empty() { return; }
        if strength <= 0.0 { self.reset(skel, base); return; }
        self.restore(skel, base);
        for j in &mut self.joints {
            let (root, target) = (pos(skel, j.bone), pos(skel, j.child));
            let length = root.distance(target);
            if length < 1e-8 { continue; }
            let rest_dir = (target - root) / length;
            let omega = std::f64::consts::PI * 2.0 * j.frequency;
            let drag = (-2.0 * j.damping * omega * dt).exp() as f32;
            let velocity = (j.current - j.previous) * drag;
            let force = target - j.current;
            let mut next = j.current + velocity + force * (omega * omega * dt * dt) as f32;
            let mut direction = (next - root).normalize();
            let angle = rest_dir.angle_between(direction) as f64;
            let limit = j.max_angle * strength;
            if angle > limit {
                let rot = crate::maker::three_slerp(from_unit_vectors(rest_dir, direction), Quat::IDENTITY, (1.0 - limit / angle) as f32);
                direction = rot * rest_dir;
            }
            next = root + direction * length;
            j.previous = j.current; j.current = next;
            let parent_rot = skel.parent[j.bone].map_or(Quat::IDENTITY, |p| three_decompose(skel.world[p]).1);
            let world_rest_inv = (parent_rot * j.rest).inverse();
            let local_dir = (world_rest_inv * direction).normalize();
            skel.r[j.bone] = (j.rest * from_unit_vectors(j.axis, local_dir)).normalize();
            skel.update_subtree(j.bone, base);
        }
    }
}

/// 襟の付け所（CharacterCollarAttachment: 測った肌の重みのまま、体の骨に付いて動く）
pub struct CollarAttachment { mounts: Vec<(usize, Mat4, Vec<(usize, f32, Mat4)>)> }
impl CollarAttachment {
    pub fn new(g: &Glb, skel: &Skeleton, order: &[usize]) -> Result<CollarAttachment, String> {
        let mut mounts = Vec::new();
        for &i in order {
            let Some(node) = g.nodes.get(i) else { continue };
            let s = &node.extras["aisnsCollarAttachment"];
            if s.is_null() { continue; }
            let p = &s["point"];
            let point = Mat4::from_translation(Vec3::new(p[0].as_f64().unwrap_or(0.0) as f32, p[1].as_f64().unwrap_or(0.0) as f32, p[2].as_f64().unwrap_or(0.0) as f32));
            let mut sources = Vec::new();
            for src in s["sources"].as_array().into_iter().flatten() {
                let name = src["name"].as_str().unwrap_or("");
                let bone = skel.find(name).ok_or(format!("Missing collar source: {name}"))?;
                let m: Vec<f32> = src["inverseBindMatrix"].as_array().into_iter().flatten().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
                if m.len() != 16 { return Err(format!("Missing collar source: {name}")); }
                sources.push((bone, src["weight"].as_f64().unwrap_or(0.0) as f32, Mat4::from_cols_slice(&m)));
            }
            mounts.push((i, point, sources));
        }
        Ok(CollarAttachment { mounts })
    }
    pub fn update(&self, skel: &mut Skeleton, base: Mat4) {
        if self.mounts.is_empty() { return; }
        skel.update_world(base);
        for (node, point, sources) in &self.mounts {
            let mut blended = [0f32; 16];
            for (bone, weight, inverse) in sources {
                let m = (skel.world[*bone] * *inverse).to_cols_array();
                for k in 0..16 { blended[k] += m[k] * weight; }
            }
            let mut b = Mat4::from_cols_array(&blended) * *point;
            if let Some(p) = skel.parent[*node] { b = skel.world[p].inverse() * b; }
            let (s, r, t) = three_decompose(b);
            skel.t[*node] = t; skel.r[*node] = r; skel.s[*node] = s;
            skel.update_subtree(*node, base);
        }
    }
}

/// しっぽ（CharacterTailMotion: 長さの決まった鎖に、根から先へ遅れて伝わる曲げ）
pub struct TailMotion { joints: Vec<(usize, Quat, f32, f32)>, phase: f64, amplitude: f64, curl: f64, sway: f64, frequency: f64 }
fn tail_settings(motion: &str) -> (f64, f64, f64, f64) {
    match motion { "walk" => (1.149999976158142, 0.052, 1.05, 0.018), "run" => (1.0, 0.110, 1.40, 0.035), _ => (12.25 / 3.0, 0.021, 0.85, 0.010) }
}
fn damp(x: f64, y: f64, lambda: f64, dt: f64) -> f64 { x + (y - x) * (1.0 - (-lambda * dt).exp()) }
impl TailMotion {
    pub fn new(g: &Glb, order: &[usize]) -> TailMotion {
        let mut joints: Vec<(usize, Quat, f32, f32)> = order.iter().filter_map(|&i| {
            let d = &g.nodes.get(i)?.extras["aisnsTailJoint"];
            if d.is_null() { return None; }
            Some((i, q4(&d["restRotation"]), d["index"].as_f64().unwrap_or(0.0) as f32, d["count"].as_f64().unwrap_or(1.0) as f32))
        }).collect();
        joints.sort_by(|a, b| a.2.total_cmp(&b.2));
        let (period, amplitude, curl, sway) = tail_settings("idle");
        TailMotion { joints, phase: 0.0, amplitude, curl, sway, frequency: 1.0 / period }
    }
    pub fn seek(&mut self, skel: &mut Skeleton, time: f64, motion: &str) {
        let (period, amplitude, curl, sway) = tail_settings(motion);
        self.phase = time / period * std::f64::consts::PI * 2.0;
        self.amplitude = amplitude; self.curl = curl; self.sway = sway; self.frequency = 1.0 / period;
        self.apply(skel);
    }
    pub fn update(&mut self, skel: &mut Skeleton, dt: f64, motion: &str, speed: f64) {
        if self.joints.is_empty() { return; }
        let (period, amplitude, curl, sway) = tail_settings(motion);
        self.amplitude = damp(self.amplitude, amplitude, 7.0, dt);
        self.curl = damp(self.curl, curl, 7.0, dt);
        self.sway = damp(self.sway, sway, 7.0, dt);
        self.frequency = damp(self.frequency, 1.0 / period, 7.0, dt);
        self.phase = (self.phase + dt * speed * self.frequency * std::f64::consts::PI * 2.0) % (std::f64::consts::PI * 2.0);
        self.apply(skel);
    }
    fn apply(&self, skel: &mut Skeleton) {
        for (bone, rest, index, count) in &self.joints {
            let t = (*index / (*count - 1.0)) as f64;
            let wave = (self.phase - t * 2.1).sin() + 0.17 * (self.phase * 2.0 - t * 2.7).sin();
            let curl = (0.012 + 0.330 * t * t * t) * self.curl;
            let angle = curl + self.amplitude * (0.35 + 0.85 * t) * wave;
            let bend = Quat::from_axis_angle(Vec3::X, angle as f32);
            let turn = Quat::from_axis_angle(Vec3::Y, (self.sway * (self.phase - t * 1.7).sin() * (0.2 + 0.8 * t)) as f32);
            skel.r[*bone] = *rest * bend * turn;
        }
    }
}

/// まばたきの途中の補正（blinkCorrectiveWeight。線形に混ぜた本体のまばたきからの差）
pub fn blink_corrective(name: &str, closure: f64) -> Option<f64> {
    let rest = name.strip_prefix("blinkLeft").or_else(|| name.strip_prefix("blinkRight"))?.strip_prefix("Inbetween")?;
    let center = match rest { "25" => 0.25, "50" => 0.5, "75" => 0.75, _ => return None };
    Some((1.0 - (closure - center).abs() / 0.25).max(0.0))
}

/// 自動のまばたき（automaticBlink。ゆっくり閉じ、少し止め、さらにゆっくり開く）
pub fn automatic_blink(elapsed: f64) -> f64 {
    let time = elapsed % 4.1 - 3.81;
    let smooth = |v: f64| v * v * (3.0 - 2.0 * v);
    if time < 0.0 { return 0.0; }
    if time < 0.085 { return smooth(time / 0.085); }
    if time < 0.115 { return 1.0; }
    if time < 0.275 { return 1.0 - smooth((time - 0.115) / 0.16); }
    0.0
}

/// 話す口（speakingMouth。音を聞かない見た目だけの口。形のまま切り替えて、短い区切りの間は口を閉じる）
pub fn speaking_mouth(time: f64) -> [(&'static str, f64); 4] {
    const SEQ: [(usize, f64); 11] = [(0, 0.16), (1, 0.25), (2, 0.21), (3, 0.23), (1, 0.22), (0, 0.27), (2, 0.22), (1, 0.30), (3, 0.27), (2, 0.18), (0, 0.48)];
    let duration: f64 = SEQ.iter().map(|s| s.1).sum();
    let mut phase = ((time % duration) + duration) % duration;
    let mut out = [0.0f64; 4];
    for i in 0..SEQ.len() {
        let (shape, hold) = SEQ[i];
        if phase >= hold { phase -= hold; continue; }
        let previous = SEQ[(i + SEQ.len() - 1) % SEQ.len()].0;
        let t = (phase / 0.095).min(1.0);
        let mix = t * t * (3.0 - 2.0 * t);
        out[previous] += 1.0 - mix; out[shape] += mix;
        return [("mouthClose", out[0]), ("aa", out[1]), ("ee", out[2]), ("ou", out[3])];
    }
    [("mouthClose", 1.0), ("aa", 0.0), ("ee", 0.0), ("ou", 0.0)]
}
