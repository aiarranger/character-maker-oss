// @aiarranger/character-motion の写し（クリップを骨へ当てる・のけぞり・休む・必殺技の時刻と重み・戦う顔・持つ物を水平に保つ）。
// 手順は characters の knowledge/guides/game-motions.md。three の値の計算と同じ順に書く。
use crate::anim::{NamedClip, NamedTrack, Prop, Skeleton, three_rotation};
use crate::glb::Interp;
use glam::{DVec4, Quat, Vec3};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// 攻撃のクリップの歩き・待機とのクロスフェード（秒）。始まりと終わりの両方
pub const ATTACK_FADE_S: f32 = 0.08;
/// 倍精度の境目を比べるときの値（f32 を広げると 0.08 からずれて、切り替わりのコマが 1 つずれる）
pub const ATTACK_FADE_64: f64 = 0.08;
/// 骨の名前が合うトラックがこの数に満たないキャラには、クリップを当てない
pub const ATTACK_MIN_TRACKS: usize = 10;
/// 武器の振りで代える必殺技の長さ（aisns の ws）
pub const ATTACK_PLAN_WS_SECONDS: f32 = 1.0;

/// 攻撃のクリップの重み（0〜1）。始まりから fade 秒で 1 へ、終わりの fade 秒前から 0 へ。始まる前・終わった後は 0
/// attack_weight を倍精度で（TS と同じ。終わりの 1 コマが f32 の丸めで消えない。消えると ＞＜ や戦う顔の最後の値が変わる）
pub fn attack_weight64(elapsed: f64, duration: f64, fade: f64) -> f64 {
    if !(duration > 0.0) || !elapsed.is_finite() || elapsed < 0.0 || elapsed >= duration { return 0.0; }
    let f = fade.min(duration / 2.0);
    1f64.min(elapsed / f).min((duration - elapsed) / f)
}

pub fn attack_weight(elapsed: f32, duration: f32, fade: f32) -> f32 {
    if !(duration > 0.0) || !elapsed.is_finite() || elapsed < 0.0 || elapsed >= duration { return 0.0; }
    let f = fade.min(duration / 2.0);
    1f32.min(elapsed / f).min((duration - elapsed) / f)
}

/// three の Quaternion.slerp（倍精度で計算する）。three は内積が 1 以上なら a を返すが、ここの四元数は f32 に丸めてあるので、
/// 向きが少し違っても内積が 1 になる（w が 1.0 に丸まる）。そのときも線形に混ぜて b へ寄せる
#[inline]
pub fn q_slerp(a: Quat, b: Quat, t: f32) -> Quat {
    if t == 0.0 { return a; }
    if t == 1.0 { return b; }
    if a == b { return a; }
    let (ad, bd) = (DVec4::from(a.to_array().map(f64::from)), DVec4::from(b.to_array().map(f64::from)));
    let t = t as f64;
    let mut cos = ad.dot(bd);
    let c = if cos < 0.0 { cos = -cos; -bd } else { bd };
    let sqr = 1.0 - cos * cos;
    let o = if sqr <= f64::EPSILON {
        (ad * (1.0 - t) + c * t).normalize()
    } else {
        let sin = sqr.sqrt();
        let half = sin.atan2(cos);
        ad * (((1.0 - t) * half).sin() / sin) + c * ((t * half).sin() / sin)
    };
    Quat::from_xyzw(o.x as f32, o.y as f32, o.z as f32, o.w as f32)
}

/// three の Vector3.applyQuaternion（長さが 1 でない四元数では glam の q * v と値が違う）
pub fn q_apply(q: Quat, v: Vec3) -> Vec3 {
    let b = Vec3::new(q.x, q.y, q.z);
    let t = 2.0 * b.cross(v);
    v + q.w * t + b.cross(t)
}

/// three の Quaternion.setFromUnitVectors
pub fn q_from_unit(from: Vec3, to: Vec3) -> Quat {
    let mut r = from.dot(to) + 1.0;
    let (x, y, z);
    if r < 1e-8 {
        r = 0.0;
        if from.x.abs() > from.z.abs() { x = -from.y; y = from.x; z = 0.0; } else { x = 0.0; y = -from.z; z = from.y; }
    } else {
        x = from.y * to.z - from.z * to.y;
        y = from.z * to.x - from.x * to.z;
        z = from.x * to.y - from.y * to.x;
    }
    Quat::from_xyzw(x, y, z, r).normalize()
}

pub fn q_from_array(v: &[f32], off: usize) -> Quat { Quat::from_xyzw(v[off], v[off + 1], v[off + 2], v[off + 3]) }
fn q_arr(q: Quat) -> [f32; 4] { [q.x, q.y, q.z, q.w] }

#[derive(Clone)]
pub struct BindBone { pub world: Quat, pub parent_world: Quat, pub parent: String, pub position: Vec3 }
pub type BindPose = HashMap<String, BindBone>;

/// キャラの bind の姿を控える（世界の行列は呼ぶ前に作っておく。アニメを当てる前）。root_rot はシーンの根の親の回転
pub fn capture_bind_pose(skel: &Skeleton, root_rot: Quat) -> BindPose {
    let mut bind = HashMap::new();
    for &i in &skel.order {
        if skel.names[i].is_empty() { continue; }
        let (_, q, p) = skel.world[i].to_scale_rotation_translation();
        let (parent_world, parent) = match skel.parent[i] {
            Some(pi) => (skel.world[pi].to_scale_rotation_translation().1, skel.names[pi].clone()),
            None => (root_rot, String::new()),
        };
        bind.insert(skel.names[i].clone(), BindBone { world: q, parent_world, parent, position: p });
    }
    bind
}

fn usable_track(t: &NamedTrack) -> bool {
    if t.times.is_empty() || t.values.is_empty() || t.values.len() != t.times.len() * 4 { return false; }
    for i in 0..t.times.len() {
        if !t.times[i].is_finite() || (i > 0 && t.times[i] < t.times[i - 1]) { return false; }
    }
    t.values.iter().all(|v| v.is_finite())
}

#[derive(Default)]
pub struct FitOptions {
    pub bone_map: Option<HashMap<String, String>>,
    pub strength: Option<&'static [(&'static str, f32)]>,
    pub max_root_deg: Option<f32>,
    pub rest_fix: Option<HashMap<String, Quat>>,
    pub hold: Option<HashMap<String, Quat>>,
    pub min_tracks: Option<usize>,
    pub fold_root: bool,
}

/// クリップをこのキャラの骨に合わせる。回転の値は軸が世界に揃った骨の局所の回転。q = B(親)^-1 · q_file · B(骨)
pub fn fit_clip_to_bind(clip: &NamedClip, bind: &BindPose, name: &str, o: &FitOptions) -> Option<NamedClip> {
    let mut tracks = Vec::new();
    let mut duration = 0f32;
    let max_root = o.max_root_deg.unwrap_or(60.0);
    for t in &clip.tracks {
        let target: Option<&str> = match &o.bone_map { Some(m) => m.get(&t.node).map(|s| s.as_str()), None => Some(t.node.as_str()) };
        let Some(target) = target else { continue };
        let Some(b) = bind.get(target) else { continue };
        if t.prop != Prop::R || !usable_track(t) || o.hold.as_ref().is_some_and(|h| h.contains_key(target)) { continue; }
        let strength = o.strength.and_then(|s| s.iter().find(|(k, _)| *k == t.node)).map(|(_, v)| v.clamp(0.0, 1.0)).unwrap_or(1.0);
        let mut values = vec![0f32; t.values.len()];
        // 立ち姿の補正 F（この骨）と Fp（親）: q = B(親)^-1 · Fp^-1 · q_file · F · B（補正の無い骨は単位）
        let bone_fix = o.rest_fix.as_ref().and_then(|f| f.get(target));
        let parent_fix = o.rest_fix.as_ref().and_then(|f| f.get(&b.parent));
        let mut parent_inv = b.parent_world.inverse();
        if let Some(f) = parent_fix { parent_inv *= f.inverse(); }
        for i in 0..t.times.len() {
            let mut q = q_from_array(&t.values, i * 4);
            if t.node == "Root" && 2.0 * q.w.abs().min(1.0).acos() * 180.0 / std::f32::consts::PI > max_root { return None; }
            if strength < 1.0 { q = q_slerp(q, Quat::IDENTITY, 1.0 - strength); }
            let mut out = parent_inv * q;
            if let Some(f) = bone_fix { out *= *f; }
            out *= b.world;
            values[i * 4..i * 4 + 4].copy_from_slice(&q_arr(out));
        }
        tracks.push(NamedTrack { node: target.to_string(), prop: Prop::R, interp: Interp::Linear, times: t.times.clone(), values: Arc::new(values), size: 4 });
        duration = duration.max(*t.times.last().unwrap());
    }
    if tracks.len() < o.min_tracks.unwrap_or(ATTACK_MIN_TRACKS) || !(duration > 0.0) { return None; }
    if let Some(hold) = &o.hold {
        for (bone, rot) in hold {
            if bind.contains_key(bone) {
                tracks.push(NamedTrack { node: bone.clone(), prop: Prop::R, interp: Interp::Linear, times: Arc::new(vec![0.0]), values: Arc::new(q_arr(*rot).to_vec()), size: 4 });
            }
        }
    }
    Some(NamedClip { name: name.to_string(), duration, seconds: duration as f64, tracks })
}

/// Tripo の biped の骨 → VRM の骨
pub fn vrm_bone_map() -> HashMap<String, String> {
    let mut m: HashMap<String, String> = [("Hip", "J_Bip_C_Hips"), ("Waist", "J_Bip_C_Spine"), ("Spine01", "J_Bip_C_Chest"), ("Spine02", "J_Bip_C_UpperChest"), ("NeckTwist01", "J_Bip_C_Neck"), ("Head", "J_Bip_C_Head")]
        .iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    for s in ["L", "R"] {
        for (a, b) in [("Clavicle", "Shoulder"), ("Upperarm", "UpperArm"), ("Forearm", "LowerArm"), ("Hand", "Hand"), ("Thigh", "UpperLeg"), ("Calf", "LowerLeg"), ("Foot", "Foot"), ("ToeBase", "ToeBase")] {
            m.insert(format!("{s}_{a}"), format!("J_Bip_{s}_{b}"));
        }
        for (finger, vrm) in [("Thumb", "Thumb"), ("Index", "Index"), ("Mid", "Middle"), ("Ring", "Ring"), ("Pinky", "Little")] {
            for i in 1..=3 { m.insert(format!("{s}_{finger}{i}"), format!("J_Bip_{s}_{vrm}{i}")); }
        }
    }
    m
}

/// Tripo の biped の骨 → Mixamo の骨（three は ':' を落として mixamorigHips と読む）
pub fn mixamo_bone_map() -> HashMap<String, String> {
    let mut m: HashMap<String, String> = [("Hip", "mixamorigHips"), ("Waist", "mixamorigSpine"), ("Spine01", "mixamorigSpine1"), ("Spine02", "mixamorigSpine2"), ("NeckTwist01", "mixamorigNeck"), ("Head", "mixamorigHead")]
        .iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    for (s, side) in [("L", "Left"), ("R", "Right")] {
        for (a, b) in [("Clavicle", "Shoulder"), ("Upperarm", "Arm"), ("Forearm", "ForeArm"), ("Hand", "Hand"), ("Thigh", "UpLeg"), ("Calf", "Leg"), ("Foot", "Foot"), ("ToeBase", "ToeBase")] {
            m.insert(format!("{s}_{a}"), format!("mixamorig{side}{b}"));
        }
    }
    m
}

/// 骨ごとの動きの強さ（クリップの骨の名前で。無い骨は 1）
pub const BODY_STRENGTH: [(&str, f32); 8] = [("Hip", 0.35), ("Pelvis", 0.35), ("Waist", 0.6), ("Spine01", 0.6), ("Spine02", 0.6), ("NeckTwist01", 0.35), ("NeckTwist02", 0.35), ("Head", 0.35)];

/// Root の回転を Hip へ掛けて、Root のトラックを外す
fn fold_root_into_hip(clip: &NamedClip) -> NamedClip {
    let root = clip.tracks.iter().find(|t| t.node == "Root" && t.prop == Prop::R);
    let hip = clip.tracks.iter().find(|t| t.node == "Hip" && t.prop == Prop::R);
    let (Some(root), Some(hip)) = (root, hip) else { return clip.clone() };
    if root.times.len() != hip.times.len() { return clip.clone(); }
    let mut values = vec![0f32; hip.values.len()];
    for i in 0..hip.times.len() {
        let q = (q_from_array(&root.values, i * 4) * q_from_array(&hip.values, i * 4)).normalize();
        values[i * 4..i * 4 + 4].copy_from_slice(&q_arr(q));
    }
    let mut tracks: Vec<NamedTrack> = clip.tracks.iter().filter(|t| !(t.prop == Prop::R && (t.node == "Root" || t.node == "Hip"))).cloned().collect();
    tracks.push(NamedTrack { node: "Hip".into(), prop: Prop::R, interp: Interp::Linear, times: hip.times.clone(), values: Arc::new(values), size: 4 });
    NamedClip { name: clip.name.clone(), duration: clip.duration, seconds: clip.seconds, tracks }
}

#[derive(Clone, Copy, PartialEq)]
pub enum BodyKey { Hit, RestDown, RestLoop, RestUp }

/// クリップの元の立ち姿の、上腕・前腕の向き（2026-10-13 に骨の位置から測った）
const SOURCE_REST_DIRS: [(&str, &str, [f32; 3]); 4] = [
    ("L_Upperarm", "L_Forearm", [0.403, -0.911, -0.091]), ("L_Forearm", "L_Hand", [0.417, -0.908, -0.033]),
    ("R_Upperarm", "R_Forearm", [-0.404, -0.913, -0.057]), ("R_Forearm", "R_Hand", [-0.423, -0.905, 0.047]),
];

/// 上腕・前腕を、このキャラの立ち姿の向きからクリップの元の立ち姿の向きへ回す補正。補正した骨の子孫にも同じ補正を入れる
pub fn arm_rest_fix(bind: &BindPose, map: Option<&HashMap<String, String>>) -> HashMap<String, Quat> {
    let name = |s: &str| -> Option<String> { match map { Some(m) => m.get(s).cloned(), None => Some(s.to_string()) } };
    let mut own = HashMap::new();
    for (source, child, want) in SOURCE_REST_DIRS {
        let (Some(bone), Some(child)) = (name(source), name(child)) else { continue };
        let (Some(a), Some(b)) = (bind.get(&bone), bind.get(&child)) else { continue };
        let have = b.position - a.position;
        if have.length_squared() < 1e-12 { continue; }
        own.insert(bone, q_from_unit(have.normalize(), Vec3::from(want).normalize()));
    }
    let mut fix = own.clone();
    for bone in bind.keys() {
        if fix.contains_key(bone) { continue; }
        let mut p = bind.get(bone).map(|b| b.parent.clone());
        let mut step = 0;
        while let Some(pn) = p {
            if pn.is_empty() || step >= 64 { break; }
            if let Some(f) = own.get(&pn) { fix.insert(bone.clone(), *f); break; }
            p = bind.get(&pn).map(|b| b.parent.clone());
            step += 1;
        }
    }
    fix
}

/// キャラの骨（bind）とクリップの種類に合う当て方。強さを弱めるのはのけぞりだけ
pub fn body_fit_options(bind: &BindPose, key: BodyKey, hold: Option<HashMap<String, Quat>>) -> FitOptions {
    let (map, fold) = if bind.contains_key("J_Bip_C_Hips") { (Some(vrm_bone_map()), true) } else if bind.contains_key("mixamorigHips") { (Some(mixamo_bone_map()), true) } else { (None, false) };
    let rest_fix = Some(arm_rest_fix(bind, map.as_ref()));
    FitOptions { bone_map: map, strength: if key == BodyKey::Hit { Some(&BODY_STRENGTH) } else { None }, max_root_deg: Some(f32::INFINITY), rest_fix, hold, min_tracks: None, fold_root: fold }
}

/// クリップをキャラの骨に当てる。骨の合わないキャラは None
pub fn fit_body_clip(clip: &NamedClip, bind: &BindPose, name: &str, o: &FitOptions) -> Option<NamedClip> {
    if o.fold_root { fit_clip_to_bind(&fold_root_into_hip(clip), bind, name, o) } else { fit_clip_to_bind(clip, bind, name, o) }
}

// ---- 動かさない骨・水平に保つ ----

#[derive(Clone)]
pub struct StillLevel { pub bone: String, pub local_up: [f32; 3] }

#[derive(Clone)]
pub struct StillBones {
    pub bones: Vec<String>,
    pub pose_clip: String,
    pub level: Option<StillLevel>,
    /// {hit,rest,skill: {hold, override?, level}, idle: {level}}
    pub motions: Option<Value>,
}

/// 止める骨とその子孫の、止める姿（モデル自身のクリップの最初のキーの局所の回転）
pub fn still_pose(bind: &BindPose, still: &StillBones, first_key: &dyn Fn(&str) -> Option<Quat>) -> Option<HashMap<String, Quat>> {
    if still.bones.is_empty() || still.bones.iter().any(|b| !bind.contains_key(b)) { return None; }
    let mut pose = HashMap::new();
    for bone in bind.keys() {
        let mut held = false;
        let mut b = Some(bone.clone());
        let mut step = 0;
        while let Some(n) = b {
            if n.is_empty() || step >= 64 { break; }
            if still.bones.contains(&n) { held = true; break; }
            b = bind.get(&n).map(|x| x.parent.clone());
            step += 1;
        }
        if !held { continue; }
        pose.insert(bone.clone(), first_key(bone)?);
    }
    Some(pose)
}

/// その動きのクリップで止める姿（motion は hit・rest・skill）
pub fn still_pose_for(bind: &BindPose, still: &StillBones, first_key: &dyn Fn(&str) -> Option<Quat>, motion: &str) -> Option<HashMap<String, Quat>> {
    let m = still.motions.as_ref().and_then(|x| x.get(motion));
    if let Some(m) = m && !m.get("hold").and_then(Value::as_bool).unwrap_or(false) { return None; }
    let mut pose = still_pose(bind, still, first_key)?;
    if let Some(over) = m.and_then(|m| m.get("override")).and_then(Value::as_object) {
        for (bone, q) in over {
            if !pose.contains_key(bone) { return None; }
            let a: Vec<f32> = q.as_array()?.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect();
            pose.insert(bone.clone(), Quat::from_xyzw(a[0], a[1], a[2], a[3]).normalize());
        }
    }
    Some(pose)
}

/// 水平に保つ強さ（0〜1）。weights はクリップの重み。idle の重みは 1 − その合計
pub fn level_weight(still: Option<&StillBones>, hit: f32, rest: f32, skill: f32) -> f32 {
    let Some(still) = still else { return 0.0 };
    if still.level.is_none() { return 0.0; }
    let Some(m) = &still.motions else { return 1.0 };
    let lv = |k: &str| m.get(k).and_then(|x| x.get("level")).and_then(Value::as_bool).unwrap_or(false);
    let idle = (1.0 - hit - rest - skill).max(0.0);
    let sum = (if lv("hit") { hit } else { 0.0 }) + (if lv("rest") { rest } else { 0.0 }) + (if lv("skill") { skill } else { 0.0 }) + (if lv("idle") { idle } else { 0.0 });
    sum.clamp(0.0, 1.0)
}

#[derive(Clone, Default)]
pub struct Levelled { state: Option<(Quat, Quat)> }

/// 持つ物を世界で水平に保つ（マスター・ナギのトレー。2026-10-15 Owner「トレーはいつも水平」）。世界の行列は呼ぶ前に作っておく。直したら骨と子孫の世界の行列も直す
pub fn level_bone(skel: &mut Skeleton, bone: usize, local_up: [f32; 3], amount: f32, last: &mut Levelled, root: glam::Mat4) {
    let (animated, fixed) = last.state.unwrap_or((Quat::IDENTITY, Quat::IDENTITY));
    let restored = last.state.is_some() && skel.r[bone] == fixed;
    if restored { skel.r[bone] = animated; }
    let animated = skel.r[bone];
    if !(amount > 0.0) {
        last.state = Some((animated, skel.r[bone]));
        if restored { skel.update_subtree(bone, root); }
        return;
    }
    if restored { skel.update_subtree(bone, root); }
    let world = skel.world_rotation(bone);
    let axis = q_apply(world, Vec3::from(local_up).normalize());
    let angle = axis.angle_between(Vec3::Y);
    if angle < 1e-7 {
        last.state = Some((animated, skel.r[bone]));
        return;
    }
    let mut fix = q_from_unit(axis, Vec3::Y);
    if amount < 1.0 { fix = q_slerp(Quat::IDENTITY, fix, amount); }
    let parent = match skel.parent[bone] { Some(p) => skel.world_rotation(p), None => three_rotation(root) };
    skel.r[bone] = parent.inverse() * (fix * world);
    last.state = Some((animated, skel.r[bone]));
    skel.update_subtree(bone, root);
}

/// ぱっちむ型の骨（Root・Body・Head・Wing・Paw だけ）の、のけぞりの手作りのクリップ。Body・Head が無い・Hip があるキャラは None
pub fn paw_hit_clip(bind: &BindPose) -> Option<NamedClip> {
    if !bind.contains_key("Body") || !bind.contains_key("Head") || bind.contains_key("Hip") { return None; }
    let times = Arc::new(vec![0.0f32, 0.3, 0.6, 0.9, 1.25, 1.8]);
    let env = [0.0f32, 0.7, 1.0, 0.6, 0.15, 0.0];
    let body = bind["Body"].position;
    let track = |bone: &str, axis: Vec3, deg: f32| {
        let mut values = Vec::with_capacity(24);
        for e in env { values.extend_from_slice(&q_arr(Quat::from_axis_angle(axis, deg * e * std::f32::consts::PI / 180.0))); }
        NamedTrack { node: bone.to_string(), prop: Prop::R, interp: Interp::Linear, times: times.clone(), values: Arc::new(values), size: 4 }
    };
    let mut tracks = vec![track("Body", Vec3::X, -10.0), track("Head", Vec3::X, -8.0)];
    for wing in ["Wing_L", "Wing_R"] {
        if let Some(b) = bind.get(wing) {
            let d = b.position.x - body.x;
            let d = if d == 0.0 { 1.0 } else { d };
            tracks.push(track(wing, Vec3::Z, d.signum() * 50.0));
        }
    }
    for paw in ["Paw_L", "Paw_R"] { if bind.contains_key(paw) { tracks.push(track(paw, Vec3::X, -55.0)); } }
    let clip = NamedClip { name: "paw:hit".into(), duration: 1.8, seconds: 1.8, tracks };
    fit_clip_to_bind(&clip, bind, "paw:hit", &FitOptions { min_tracks: Some(4), ..Default::default() })
}

// ---- のけぞり・必殺技・休むの時刻と重み ----

/// のけぞりの再生の計画（秒）。信号から seconds の間に、クリップの from〜to 秒を一定の速さで再生する
pub const HIT_FROM: f32 = 0.05;
pub const HIT_TO: f32 = 1.25;
pub const HIT_SECONDS: f32 = 0.8;
pub const HIT_FADE: f32 = 0.1;
/// 倍精度の境目を比べるときの値（f32 を広げると 0.8・0.1 からずれて、重み 0.5 の切り替わりのコマが 1 つずれる）
pub const HIT_SECONDS_64: f64 = 0.8;
pub const HIT_FADE_64: f64 = 0.1;

pub fn hit_clip_state(elapsed: f32) -> Option<(f32, f32)> {
    if !elapsed.is_finite() || elapsed < 0.0 || elapsed >= HIT_SECONDS { return None; }
    Some((HIT_FROM + (HIT_TO - HIT_FROM) * elapsed / HIT_SECONDS, attack_weight(elapsed, HIT_SECONDS, HIT_FADE)))
}

/// 必殺技の動きの再生（秒）。クリップ（約 3 秒）をこの長さに縮めて 1 回だけ流す
pub const SKILL_SECONDS: f32 = 2.0;
pub const SKILL_FADE: f32 = 0.15;

pub fn skill_clip_state(elapsed: f32, clip_seconds: f32) -> Option<(f32, f32)> {
    if !elapsed.is_finite() || elapsed < 0.0 || elapsed >= SKILL_SECONDS { return None; }
    Some((clip_seconds * elapsed / SKILL_SECONDS, attack_weight(elapsed, SKILL_SECONDS, SKILL_FADE)))
}

pub const REST_FADE_S: f64 = 0.25;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct RestState { pub weight: f32, pub down: f32, pub r#loop: f32, pub up: f32, pub down_t: f32, pub loop_t: f32, pub up_t: f32 }

fn ramp(x: f64) -> f64 { x.clamp(0.0, 1.0) }
/// 往復の時刻（0→d→0）
fn ping_pong(t: f64, d: f64) -> f64 { let m = t.rem_euclid(2.0 * d); if m <= d { m } else { 2.0 * d - m } }

/// 休む（座り込む → 座ったまま息をする → 立ち上がる）の重みと時刻。now・on_at・off_at はミリ秒、d は秒（TS の restClipState と同じく倍精度で測る。立ち終わりのコマが同じになる）
pub fn rest_clip_state(now: f64, on_at: f64, off_at: Option<f64>, d: (f64, f64, f64)) -> RestState {
    let (d_down, d_loop, d_up) = d;
    let sitting = match off_at { None => true, Some(o) => o < on_at };
    let e = (now - on_at) / 1000.0;
    if e < 0.0 { return RestState::default(); }
    let loop_w = ramp((e - (d_down - REST_FADE_S)) / REST_FADE_S);
    let down_t = e.min(d_down);
    let loop_t = ping_pong((e - d_down).max(0.0), d_loop);
    if sitting { return RestState { weight: ramp(e / REST_FADE_S) as f32, down: (1.0 - loop_w) as f32, r#loop: loop_w as f32, up: 0.0, down_t: down_t as f32, loop_t: loop_t as f32, up_t: 0.0 }; }
    let off = off_at.unwrap();
    let u = (now - off) / 1000.0;
    if u < 0.0 || u >= d_up { return RestState::default(); }
    let at = (off - on_at) / 1000.0;
    let up_w = ramp(u / REST_FADE_S);
    let seated_w = 1.0 - up_w;
    let seat_loop = ramp((at - (d_down - REST_FADE_S)) / REST_FADE_S);
    RestState {
        weight: ramp(at / REST_FADE_S + u / REST_FADE_S).min(ramp((d_up - u) / REST_FADE_S)) as f32,
        down: (seated_w * (1.0 - seat_loop)) as f32, r#loop: (seated_w * seat_loop) as f32, up: up_w as f32,
        down_t: at.min(d_down) as f32, loop_t: ping_pong((at - d_down).max(0.0), d_loop) as f32, up_t: u as f32,
    }
}

// ---- 戦う顔 ----

/// 戦う顔が書くモーフ（この順で番号）
pub const COMBAT_KEYS: [&str; 13] = ["happy", "angry", "sad", "relaxed", "surprised", "aa", "ih", "ee", "ou", "oh", "mouthClose", "blinkLeft", "blinkRight"];
pub const K_ANGRY: usize = 1;
pub const K_AA: usize = 5;
pub const K_EE: usize = 7;
pub const K_MOUTH_CLOSE: usize = 10;
pub const K_BLINK_L: usize = 11;
pub const K_BLINK_R: usize = 12;

pub type CombatValues = [Option<f32>; 13];

fn smooth01(v: f32) -> f32 { let x = v.clamp(0.0, 1.0); x * x * (3.0 - 2.0 * x) }

/// 振りの強さ。両端は 0。当たりは位相 0.4
pub fn combat_pulse(attack: bool, phase: f32) -> f32 {
    if !attack || !phase.is_finite() { return 0.0; }
    if phase <= 0.4 { smooth01(phase / 0.4) } else { 1.0 - smooth01((phase - 0.4) / 0.6) }
}

/// そのキャラが持つモーフだけ。怒りは、当たる瞬間の口に譲る。has は COMBAT_KEYS の順にそのキャラが持つか
pub fn combat_face(attack: bool, phase: f32, has: &[bool; 13], weight: f32) -> CombatValues {
    let pulse = combat_pulse(attack, phase);
    let w = weight.clamp(0.0, 1.0);
    let mut v: CombatValues = [None; 13];
    for k in 0..11 { if has[k] { v[k] = Some(0.0); } }
    if has[K_ANGRY] {
        v[K_ANGRY] = Some((0.65 - 0.3 * pulse) * w);
    } else {
        for k in [K_BLINK_L, K_BLINK_R] { if has[k] { v[k] = Some(0.12 * w); } }
    }
    if has[K_EE] { v[K_EE] = Some(0.15 * (1.0 - pulse) * w); }
    if has[K_AA] { v[K_AA] = Some(0.8 * pulse * w); }
    if has[K_MOUTH_CLOSE] { v[K_MOUTH_CLOSE] = Some((1.0 - 0.8 * pulse) * w); }
    v
}

/// 構えと振りが同時に出ているとき、それぞれの重み済みの値を足す
pub fn mix_combat_faces(a: Option<CombatValues>, b: Option<CombatValues>) -> Option<CombatValues> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(mut p), Some(q)) => {
            for k in 0..13 { if let Some(v) = q[k] { p[k] = Some(p[k].unwrap_or(0.0) + v); } }
            Some(p)
        }
    }
}

// ---- まばたき（character-runtime の blink） ----

/// 閉じ具合の間の補正（blink(Left|Right)Inbetween(25|50|75)）。center は 0.25 など
pub fn blink_corrective(center: f32, closure: f32) -> f32 { (1.0 - (closure - center).abs() / 0.25).max(0.0) }

/// 閉じて、少し保って、ゆっくり開く
pub fn automatic_blink(elapsed: f32) -> f32 {
    let time = elapsed % 4.1 - 3.81;
    let smooth = |v: f32| v * v * (3.0 - 2.0 * v);
    if time < 0.0 { return 0.0; }
    if time < 0.085 { return smooth(time / 0.085); }
    if time < 0.115 { return 1.0; }
    if time < 0.275 { return 1.0 - smooth((time - 0.115) / 0.16); }
    0.0
}

/// 振りの速さの形: 振りかぶりは速く抜け、打撃のあたりをゆっくり見せ、最後はさっと戻る
pub fn ease_swing(k: f32) -> f32 {
    if k < 0.35 { (k / 0.35) * 0.55 } else { 0.55 + (1.0 - (1.0 - (k - 0.35) / 0.65).powi(2)) * 0.45 }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: f32, b: f32) -> bool { (a - b).abs() < 1e-5 }

    #[test]
    fn weights_match_ts() {
        // node: attackWeight(0.04, 1, 0.08) = 0.5, attackWeight(0.96,1,0.08) ≈ 0.5, attackWeight(1,1)=0
        assert!(close(attack_weight(0.04, 1.0, 0.08), 0.5));
        assert!(close(attack_weight(0.96, 1.0, 0.08), 0.5));
        assert_eq!(attack_weight(1.0, 1.0, 0.08), 0.0);
        assert_eq!(attack_weight(-0.1, 1.0, 0.08), 0.0);
        // easeSwing(0.35) = 0.55, easeSwing(1) = 1
        assert!(close(ease_swing(0.35), 0.55));
        assert!(close(ease_swing(1.0), 1.0));
        // automaticBlink(3.9): time = .09 → 1
        assert_eq!(automatic_blink(3.9), 1.0);
        assert_eq!(automatic_blink(1.0), 0.0);
        let (t, w) = hit_clip_state(0.4).unwrap();
        assert!(close(t, 0.65) && close(w, 1.0));
        assert!(hit_clip_state(0.8).is_none());
        let r = rest_clip_state(100.0, 0.0, None, (1.0, 2.0, 1.0));
        assert!(close(r.weight, 0.4) && close(r.down, 1.0) && close(r.r#loop, 0.0));
        let r = rest_clip_state(3500.0, 0.0, Some(3000.0), (1.0, 2.0, 1.0));
        assert!(close(r.weight, 1.0) && close(r.up, 1.0) && close(r.up_t, 0.5));
    }

    /// 防ぐ故障: w が 1.0 に丸まった四元数で内積が 1 になり、そのコマだけ重ねた動きが消える（きゃしーの触手が 1 コマ跳ねた）
    #[test]
    fn slerp_reaches_b_when_the_dot_rounds_to_one() {
        let a = Quat::from_xyzw(-1.949906e-6, 0.0, 0.00027331052, 1.0);
        let o = q_slerp(a, Quat::IDENTITY, 0.9999998);
        assert!(o.z.abs() < 1e-7, "{o:?}");
    }

    /// 防ぐ故障: 入れ物が回って伸び縮みしている間、持つ物を水平に保つ手の骨が three とずれる（pinball のマスター・ナギの左手）。答えは three の decompose・applyQuaternion（0.178・0.186 で同じ）
    #[test]
    fn stretched_parent_is_read_like_three() {
        let m = glam::Mat4::from_cols_array(&[-0.66463874, 0.72831295, 0.5708505, 0.0, -0.46882452, -1.1108295, 0.5793732, 0.0, 1.0528864, 0.34066877, 0.43348429, 0.0, 0.2021005, 0.91817284, -0.6896128, 1.0]);
        let q = three_rotation(m);
        for (v, e) in q.to_array().iter().zip([0.41602718, 0.2158975, 0.8334424, 0.29687855]) { assert!((v - e).abs() < 2e-6, "{q:?}"); }
        let v = q_apply(q, Vec3::new(0.6, 0.64, 0.48).normalize());
        for (v, e) in v.to_array().iter().zip([-0.09683198, -0.01179026, 0.9966775]) { assert!((v - e).abs() < 4e-6, "{v}"); }
    }

    #[test]
    fn combat_face_matches_ts() {
        // combatFace('sword_attack', 0.4, keys, 1): pulse=1, angry=.35, aa=.8, mouthClose=.2, ee=0
        let mut has = [false; 13];
        has[K_ANGRY] = true; has[K_AA] = true; has[K_EE] = true; has[K_MOUTH_CLOSE] = true;
        let v = combat_face(true, 0.4, &has, 1.0);
        assert!(close(v[K_ANGRY].unwrap(), 0.35) && close(v[K_AA].unwrap(), 0.8) && close(v[K_MOUTH_CLOSE].unwrap(), 0.2) && close(v[K_EE].unwrap(), 0.0));
        // 怒りの無いキャラは目を少し閉じる
        let mut has2 = [false; 13];
        has2[K_BLINK_L] = true; has2[K_BLINK_R] = true;
        let v = combat_face(false, 0.0, &has2, 0.5);
        assert!(close(v[K_BLINK_L].unwrap(), 0.06));
    }
}
