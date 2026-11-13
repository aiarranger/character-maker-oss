// 主役 1 人の入口（共通ルール「主役キャラとゲームの境目」の部品。@aiarranger/character-actor・character-runtime の写し）。
// キャラがどう見え・どう動くかはここ。世界の位置・速さ・ジャンプの高さ・当たり判定・どの動きが割り込めるかはゲーム。
// - 待機・歩き・走り・まばたき・口は character-runtime と同じ（Mixer）
// - 振り・構え・のけぞり・休む・必殺技・ジャンプ・回避は、その上に重ねる（Overlay）。共通ののけぞり・休止は motion.rs で骨に当て直す
// - ＞＜ は hurt_face.rs。重みはのけぞりと同じ
// 取得と解析は GPU を触らない（CharacterActor は Send。別のスレッドで作り、character-view の ActorView::set_actor で GPU へ上げる）。
use crate::hurt_face::{self, BasicMaterial, Ctx, HurtFace, HurtFaceStyle, PrimRef};
use crate::motion::{self as mo, BindPose, BodyKey, CombatValues, Levelled, StillBones, StillLevel};
use crate::anim::{self, ActionId, Clip, Mixer, NamedClip, Palette, Prop, Skeleton, Track};
use crate::delivery::{DeliveryClient, Prepared};
use crate::mouth::{self, CharacterSpeech};
use crate::Vertex;
use crate::glb::{self, Glb};
use crate::spring::SpringMotion;
use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Quat, Vec3};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActionKind { Attack, Skill, Dodge, Jump, Rest, Banzai, Greet }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MotionKind { Attack, Skill, Dodge, Jump, Rest, Banzai, Greet, Hit, Idle, Walk, Run, Ready }
impl From<ActionKind> for MotionKind {
    fn from(k: ActionKind) -> MotionKind {
        match k { ActionKind::Attack => MotionKind::Attack, ActionKind::Skill => MotionKind::Skill, ActionKind::Dodge => MotionKind::Dodge, ActionKind::Jump => MotionKind::Jump, ActionKind::Rest => MotionKind::Rest, ActionKind::Banzai => MotionKind::Banzai, ActionKind::Greet => MotionKind::Greet }
    }
}
/// own は本人用、shared は共通の動きをこのキャラの骨に当て直したもの、fallback は代わりの動き
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MotionSource { Own, Shared, Fallback }
/// 意味のある合図。ゲームは秒数でなく、これで判定する
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cue { Strike, Takeoff, Land, Seated }

#[derive(Clone, Debug, Default)]
pub struct MotionInfo {
    pub available: bool,
    pub source: Option<MotionSource>,
    /// 部品の既定の長さ（秒）。休むは立つまで続くので None
    pub seconds: Option<f32>,
    /// seconds の倍精度（three の clip.duration そのまま。動作の終わりのコマを TS とそろえる）
    pub seconds_exact: Option<f64>,
    pub cues: Vec<Cue>,
    /// 回避: クリップの中の体の移動（部品の空間・メートル）
    pub travel: Option<[f32; 3]>,
    /// ジャンプ: クリップの中の体の上がる量
    pub height: Option<f32>,
    /// 被弾: ＞＜ の板を出せるか
    pub face: Option<bool>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActorEvent { Start(ActionKind), Interrupt(ActionKind), End(ActionKind), Cue(ActionKind, Cue) }

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum StartResult { Ok { seconds: Option<f32>, fallback: bool }, Unavailable }

/// 攻撃・必殺技の再生のしかた（省くと、クリップ全体をそのままの形で 1 回）
#[derive(Clone, Copy, Default, Debug)]
pub struct StartOptions {
    pub seconds: Option<f64>,
    pub air_seconds: Option<f64>,
    pub from: Option<f64>,
    pub to: Option<f64>,
    /// 進み方が swing か
    pub ease_swing: bool,
    pub cycle: Option<f64>,
    pub since: Option<f64>,
}

/// 歩き・走りに切り替わる速さ。今の版と同じく倍精度で比べる（f32 だと減速の途中で速さがちょうど切り替えの値に丸まり、判定が 1 コマ逆になる）
#[derive(Clone, Copy, Debug)]
pub struct Gait { pub walk: f64, pub run: f64 }
impl Default for Gait { fn default() -> Self { Gait { walk: 0.15, run: 2.5 } } }
impl Gait {
    /// (歩いているか, 走っているか)
    pub fn flags(&self, grounded: bool, speed: f64) -> (bool, bool) {
        let moving = grounded && speed > self.walk;
        (moving, moving && speed > self.run)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Quality { Lite, Normal, Full }
impl Quality { fn key(self) -> &'static str { match self { Quality::Lite => "lite", Quality::Normal => "normal", Quality::Full => "full" } } }

/// 写真の固定の姿（character-runtime の PhotoPose。待機は 0 秒・歩きは 15%）。渡している間は動作・のけぞり・構え・まばたき・話す口を止める（休むはそのまま）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PhotoPose { Idle, Walk }

pub struct LoadOptions {
    pub id: String,
    pub revision: Option<String>,
    pub quality: Quality,
    pub gait: Gait,
}

// ---- 配布の版から、読むファイルと使う動きを決める（character-actor の release.js） ----

#[derive(Clone, Debug, Default)]
pub struct NativePack { pub file: String, pub names: HashMap<String, String> }

#[derive(Clone, Default)]
pub struct ReleasePlan {
    pub model_sha256: String,
    pub model: String,
    pub files: Vec<String>,
    pub hurt_face: Option<HurtFaceStyle>,
    pub weapon: bool,
    pub weapon_clip_sha256: Option<String>,
    /// (モデルの中のクリップか, ファイル, 名前)
    pub skill_model: Option<String>,
    pub skill_file: Option<(String, String)>,
    pub native_body: Option<NativePack>,
    pub native_actions: Option<NativePack>,
    pub shared: Vec<(BodyKeyName, String)>,
    pub still: Option<StillBones>,
}

pub type BodyKeyName = &'static str;
pub const SHARED_BODY_FILES: [(BodyKeyName, &str); 4] = [("hit", "clip:biped-hit.glb"), ("restDown", "clip:biped-rest-down.glb"), ("restLoop", "clip:biped-rest-loop.glb"), ("restUp", "clip:biped-rest-up.glb")];
const BODY_CLIP_NAMES: [(BodyKeyName, &str, BodyKey); 4] = [("hit", "preset:biped:hit_to_head", BodyKey::Hit), ("restDown", "ai:rest_down", BodyKey::RestDown), ("restLoop", "ai:rest_loop", BodyKey::RestLoop), ("restUp", "ai:rest_up", BodyKey::RestUp)];

pub fn release_plan(manifest: &Value, quality: Quality) -> ReleasePlan {
    let files_v = &manifest["files"];
    let registry = manifest.get("registry").cloned().unwrap_or(Value::Null);
    let sha = files_v.get("full").and_then(|f| f.get("sha256")).and_then(Value::as_str)
        .or_else(|| registry.get("delivered").and_then(|d| d.get("modelSha256")).and_then(Value::as_str)).unwrap_or("").to_string();
    let delivered_sha = registry.get("delivered").and_then(|d| d.get("modelSha256")).and_then(Value::as_str).unwrap_or("").to_string();
    let tuned = |key: &str| -> Option<&Value> {
        let v = registry.get(key)?;
        if v.is_object() && v.get("none").is_none() && v.get("forModel").and_then(Value::as_str) == Some(sha.as_str()) && delivered_sha == sha { Some(v) } else { None }
    };
    let delivered = |file: Option<&Value>| file.and_then(Value::as_str).is_some_and(|f| files_v.get(format!("clip:{f}")).is_some());
    let mut files = vec![quality.key().to_string()];
    let add = |files: &mut Vec<String>, f: String| { if !files.contains(&f) { files.push(f); } };
    let face = tuned("hurtFace");
    let weapon_row = tuned("weapon");
    let weapon = weapon_row.is_some() && files_v.get("weapon").is_some();
    if weapon { add(&mut files, "weapon".into()); }
    let (mut skill_model, mut skill_file) = (None, None);
    if let Some(s) = tuned("skillClip") && let Some(name) = s.get("name").and_then(Value::as_str) && delivered(s.get("file")) {
        let file = s["file"].as_str().unwrap();
        let f = &files_v[format!("clip:{file}")];
        let mode = s.get("mode").and_then(Value::as_str);
        // 本人用の必殺技はモデルと同じ GLB（10MB を超える）。取りに行かずに読み込んだモデルの中のクリップを使う
        if mode == Some("native-model") && f.get("sha256").and_then(Value::as_str) == Some(sha.as_str()) {
            skill_model = Some(name.to_string());
        } else if mode != Some("native-model") {
            let key = format!("clip:{file}");
            skill_file = Some((key.clone(), name.to_string()));
            add(&mut files, key);
        }
    }
    let native_row = |key: &str, keys: &[&str]| -> Option<NativePack> {
        let v = tuned(key)?;
        if v.get("mode").and_then(Value::as_str) != Some("native-model") || !delivered(v.get("file")) { return None; }
        let names = v.get("names")?.as_object()?;
        let mut out = HashMap::new();
        for k in keys { if let Some(n) = names.get(*k).and_then(Value::as_str) { out.insert(k.to_string(), n.to_string()); } }
        Some(NativePack { file: format!("clip:{}", v["file"].as_str().unwrap()), names: out })
    };
    let body = native_row("bodyClips", &["hit", "restDown", "restLoop", "restUp"]);
    let actions = native_row("actionClips", &["jump", "dodge", "banzai", "greet"]);
    if let Some(b) = &body { add(&mut files, b.file.clone()); }
    if let Some(a) = &actions { add(&mut files, a.file.clone()); }
    // 共通ののけぞり・休止は、本人用の動きが無い主役だけ
    let mut shared = Vec::new();
    if body.is_none() {
        for (k, f) in SHARED_BODY_FILES { if files_v.get(f).is_some() { shared.push((k, f.to_string())); add(&mut files, f.to_string()); } }
    }
    let still = tuned("stillBones").and_then(|st| {
        let bones: Vec<String> = st.get("bones")?.as_array()?.iter().filter_map(|b| b.as_str().map(String::from)).collect();
        let pose_clip = st.get("poseClip")?.as_str()?.to_string();
        let level = st.get("level").and_then(|l| {
            let a = l.get("localUp")?.as_array()?;
            Some(StillLevel { bone: l.get("bone")?.as_str()?.to_string(), local_up: [a[0].as_f64()? as f32, a[1].as_f64()? as f32, a[2].as_f64()? as f32] })
        });
        Some(StillBones { bones, pose_clip, level, motions: st.get("motions").cloned() })
    });
    let clip_sha = weapon_row.and_then(|w| w.get("clipModelSha256")).and_then(Value::as_str).unwrap_or(&sha).to_string();
    ReleasePlan {
        model_sha256: sha, model: quality.key().to_string(), files,
        hurt_face: face.and_then(|f| f.get("style")).and_then(HurtFaceStyle::from_json),
        weapon, weapon_clip_sha256: weapon.then_some(clip_sha),
        skill_model, skill_file, native_body: body, native_actions: actions, shared, still,
    }
}

// ---- 動きの重ね方（character-actor の overlay.js） ----

/// 動きのクリップを、動かした姿の上へ重みで重ねる。骨に当て直したクリップは回転だけ。本人用の動き（native）は位置・大きさも重ねる
#[derive(Clone)]
pub struct Overlay {
    pub duration: f32,
    pub seconds: f64,
    pub nodes: Vec<usize>,
    pub tracks: Vec<Track>,
    holds: Vec<(usize, Prop, [f32; 4])>,
}

fn prop_value(s: &Skeleton, node: usize, p: Prop) -> [f32; 4] {
    match p { Prop::T => s.t[node].extend(0.0).to_array(), Prop::S => s.s[node].extend(0.0).to_array(), _ => s.r[node].to_array() }
}

impl Overlay {
    /// under: この動きの下にある動き。それだけが動かす節点は、重みの分だけアニメ前の姿へ戻す。作るのはアニメを当てる前
    pub fn new(skel: &Skeleton, clip: &Clip, all: bool, under: &[Option<&Clip>]) -> Overlay {
        let mut ov = Overlay { duration: clip.duration, seconds: clip.seconds, nodes: Vec::new(), tracks: Vec::new(), holds: Vec::new() };
        for t in &clip.tracks {
            if t.prop == Prop::W || (t.prop != Prop::R && !all) { continue; }
            if !ov.nodes.contains(&t.node) { ov.nodes.push(t.node); }
            ov.tracks.push(t.clone());
        }
        let own: Vec<(usize, Prop)> = ov.tracks.iter().map(|t| (t.node, t.prop)).collect();
        let mut seen: Vec<(usize, Prop)> = Vec::new();
        for c in under.iter().flatten() {
            for t in &c.tracks {
                if t.prop == Prop::W { continue; }
                let key = (t.node, t.prop);
                if own.contains(&key) || seen.contains(&key) { continue; }
                seen.push(key);
                if !ov.nodes.contains(&t.node) { ov.nodes.push(t.node); }
                ov.holds.push((t.node, t.prop, prop_value(skel, t.node, t.prop)));
            }
        }
        ov
    }

    pub fn ok(&self) -> bool { self.tracks.len() >= 3 }

    pub fn apply(&self, time: f32, w: f32, skel: &mut Skeleton) {
        if !(w > 0.001) { return; }
        let tt = time.min(self.duration - 1e-4).max(0.0);
        let k = w.min(1.0);
        let mut a = [0f32; 4];
        for tr in &self.tracks {
            tr.eval(tt, &mut a);
            match tr.prop {
                Prop::R => skel.r[tr.node] = mo::q_slerp(skel.r[tr.node], Quat::from_xyzw(a[0], a[1], a[2], a[3]).normalize(), k),
                Prop::T => skel.t[tr.node] = skel.t[tr.node].lerp(Vec3::new(a[0], a[1], a[2]), k),
                Prop::S => skel.s[tr.node] = skel.s[tr.node].lerp(Vec3::new(a[0], a[1], a[2]), k),
                Prop::W => {}
            }
        }
        for (node, p, v) in &self.holds {
            match p {
                Prop::R => skel.r[*node] = mo::q_slerp(skel.r[*node], Quat::from_xyzw(v[0], v[1], v[2], v[3]), k),
                Prop::T => skel.t[*node] = skel.t[*node].lerp(Vec3::new(v[0], v[1], v[2]), k),
                Prop::S => skel.s[*node] = skel.s[*node].lerp(Vec3::new(v[0], v[1], v[2]), k),
                Prop::W => {}
            }
        }
    }
}

/// 重ねる前の姿を、重ねる節点ごとに控えて戻す
#[derive(Clone, Default)]
struct Base { saved: Vec<(usize, Vec3, Quat, Vec3)> }
impl Base {
    fn new(overlays: &[Option<&Overlay>], skel: &Skeleton) -> Base {
        let mut nodes: Vec<usize> = Vec::new();
        for o in overlays.iter().flatten() { for n in &o.nodes { if !nodes.contains(n) { nodes.push(*n); } } }
        Base { saved: nodes.iter().map(|&n| (n, skel.t[n], skel.r[n], skel.s[n])).collect() }
    }
    fn restore(&self, skel: &mut Skeleton) { for (n, t, r, s) in &self.saved { skel.t[*n] = *t; skel.r[*n] = *r; skel.s[*n] = *s; } }
    fn capture(&mut self, skel: &Skeleton) { for (n, t, r, s) in &mut self.saved { *t = skel.t[*n]; *r = skel.r[*n]; *s = skel.s[*n]; } }
}

/// 本人用の動きで、体全体（動く節点の平均）がどれだけ動くかを、アニメを当てる前の姿で測る
#[derive(Clone, Default)]
pub struct BodyMeasure { pub rise: Vec<f32>, pub top: f32, pub apex: f32, pub takeoff: f64, pub landing: f64, pub travel: Vec3, duration: f32, n: usize }
impl BodyMeasure {
    pub fn rise_at(&self, t: f32) -> f32 {
        let x = (t / self.duration * self.n as f32).clamp(0.0, self.n as f32);
        let i = (self.n - 1).min(x.floor() as usize);
        self.rise[i] + (self.rise[i + 1] - self.rise[i]) * (x - i as f32)
    }
}

pub fn measure_body(skel: &Skeleton, chain: Mat4, ov: &Overlay) -> BodyMeasure {
    let n = 48usize;
    let mut s = skel.clone();
    let mut pts: Vec<Vec3> = Vec::with_capacity(n + 1);
    for i in 0..=n {
        for &o in &ov.nodes { s.t[o] = skel.t[o]; s.r[o] = skel.r[o]; s.s[o] = skel.s[o]; }
        ov.apply(i as f32 / n as f32 * ov.duration, 1.0, &mut s);
        s.update_world(chain);
        let mut c = Vec3::ZERO;
        for &o in &ov.nodes { c += s.world[o].w_axis.truncate(); }
        pts.push(c / (ov.nodes.len().max(1) as f32));
    }
    let rise: Vec<f32> = pts.iter().map(|p| p.y - pts[0].y).collect();
    let top = rise.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let air: Vec<usize> = rise.iter().enumerate().filter(|(_, r)| **r > top * 0.2).map(|(i, _)| i).collect();
    let apex = rise.iter().position(|r| *r == top).unwrap_or(0);
    let tn = n as f32;
    BodyMeasure {
        top, apex: apex as f32 / tn * ov.duration,
        // 宙の区間は倍精度で（ジャンプの終わりのコマが TS と同じになる）
        takeoff: if top > 1e-3 && !air.is_empty() { air[0] as f64 / n as f64 * ov.seconds } else { 0.0 },
        landing: if top > 1e-3 && !air.is_empty() { *air.last().unwrap() as f64 / n as f64 * ov.seconds } else { ov.seconds },
        travel: Vec3::new(pts[n].x - pts[0].x, 0.0, pts[n].z - pts[0].z),
        rise, duration: ov.duration, n,
    }
}

// ---- 武器の持ち方（character-actor の weapon-hold.js） ----

const HAND_BONES: [&str; 4] = ["R_Hand", "J_Bip_R_Hand", "mixamorigRightHand", "hand_r"];
/// 待機・歩きで武器を持つ向きと、手首から握りまでの位置（aisns の野の HAND_MOUNT・PAW_MOUNT と同じ値）
const HAND_MOUNT: ([f32; 3], [f32; 3]) = ([0.0, -0.06, 0.04], [-3.142, -0.785, 2.351]);
const PAW_MOUNT: ([f32; 3], [f32; 3]) = ([0.0, 0.0, 0.0], [-1.571, -0.611, -2.021]);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GripPose { pub p: Vec3, pub q: Quat, pub s: f32 }
impl GripPose { pub fn matrix(&self) -> Mat4 { Mat4::from_scale_rotation_translation(Vec3::splat(self.s), self.q, self.p) } }

#[derive(Clone, Copy, Debug)]
pub struct WeaponHold { pub bone: usize, pub rest: GripPose, pub swing: Option<GripPose> }

#[derive(Clone, Debug)]
pub struct WeaponSocket { pub bone: String, pub point: Vec3, pub blade: Vec3, pub palm: Vec3 }

fn socket_pose(sk: &WeaponSocket) -> (Vec3, Quat) {
    let y = sk.blade.normalize();
    let mut z = sk.palm;
    z -= y * z.dot(y);
    z = if z.length_squared() > 1e-8 { z.normalize() } else { Vec3::Z };
    let x = y.cross(z).normalize();
    let z = x.cross(y);
    (sk.point, Quat::from_mat3(&Mat3::from_cols(x, y, z)))
}

/// この主役の持ち方。人型は待機・歩きで手の外へ立て、振り・構えは配布の持つ位置。手も前足も無い主役は配布の持つ位置の骨にいつも持つ
fn weapon_hold(skel: &Skeleton, chain: Mat4, idle: Option<&Clip>, socket: Option<&WeaponSocket>, weapon_scale: f32, length: f32) -> Option<WeaponHold> {
    let hand = HAND_BONES.iter().find_map(|n| skel.find(n));
    let limb = hand.or_else(|| skel.find("Paw_R"));
    let socket_bone = socket.and_then(|s| skel.find(&s.bone));
    let bone = limb.or(socket_bone)?;
    // 骨の、待機の 0 秒の姿での向きと大きさ（いまの姿は変えない）
    let mut s = skel.clone();
    if let Some(c) = idle {
        let mut a = [0f32; 4];
        for t in &c.tracks {
            if t.prop == Prop::W { continue; }
            t.eval(0.0, &mut a);
            match t.prop { Prop::T => s.t[t.node] = Vec3::new(a[0], a[1], a[2]), Prop::S => s.s[t.node] = Vec3::new(a[0], a[1], a[2]), _ => s.r[t.node] = Quat::from_xyzw(a[0], a[1], a[2], a[3]) }
        }
    }
    s.update_world(chain);
    let (size, fq, _) = s.world[bone].to_scale_rotation_translation();
    let scale = { let v = (size.x + size.y + size.z) / 3.0; if v == 0.0 { 1.0 } else { v } };
    let at_socket = match (socket, socket_bone) { (Some(sk), Some(b)) if b == bone => Some(socket_pose(sk)), _ => None };
    if limb.is_some() {
        // 手の外へ立てて持つ（mount は主役の空間の向きと位置。骨の局所へ直す）
        let (pos, rot) = if limb == hand { HAND_MOUNT } else { PAW_MOUNT };
        let inv = fq.inverse();
        let euler = Quat::from_rotation_x(rot[0]) * Quat::from_rotation_y(rot[1]) * Quat::from_rotation_z(rot[2]);
        let rest = GripPose { p: inv * Vec3::from(pos) / scale, q: inv * euler, s: length / scale };
        return Some(WeaponHold { bone, rest, swing: at_socket.map(|(p, q)| GripPose { p, q, s: rest.s }) });
    }
    let (p, q) = at_socket?;
    Some(WeaponHold { bone, rest: GripPose { p, q, s: length * weapon_scale / scale }, swing: None })
}

// ---- 体の根の移動（character-runtime の root-motion.js） ----

#[derive(Clone)]
struct RootClip { action: ActionId, rest: Vec3, start: Vec3, delta: Vec3, duration: f32, turns: Vec<Option<Track>> }
#[derive(Clone)]
struct RootMotion { ancestors: Vec<usize>, rest_turns: Vec<Quat>, clips: Vec<RootClip>, linear: Mat3 }

/// 節点の位置（モデルの座標。シーンの根より下の局所の変形から）
fn position_in_model(s: &Skeleton, node: usize) -> Vec3 {
    let mut v = Vec3::ZERO;
    let mut o = Some(node);
    while let Some(i) = o {
        v = s.local(i).transform_point3(v);
        o = s.parent[i];
    }
    v
}

fn write_clip_at(c: &Clip, t: f32, s: &mut Skeleton) {
    let mut a = [0f32; 4];
    for tr in &c.tracks {
        if tr.prop == Prop::W { continue; }
        tr.eval(t, &mut a);
        match tr.prop { Prop::T => s.t[tr.node] = Vec3::new(a[0], a[1], a[2]), Prop::S => s.s[tr.node] = Vec3::new(a[0], a[1], a[2]), _ => s.r[tr.node] = Quat::from_xyzw(a[0], a[1], a[2], a[3]) }
    }
}

fn create_root_motion(skel: &Skeleton, setting: Option<&Value>, clips: &[(ActionId, Arc<Clip>)], linear: Mat3) -> Result<Option<RootMotion>, String> {
    let Some(setting) = setting else { return Ok(None) };
    if setting.get("mode").and_then(Value::as_str) != Some("local-linear-horizontal") { return Err("対応していない移動設定です。".into()); }
    let name = setting.get("node").and_then(Value::as_str).unwrap_or("");
    let node = [name.to_string(), anim::sanitize_name(name)].iter().find_map(|n| {
        let hits: Vec<usize> = skel.order.iter().copied().filter(|&i| skel.names[i] == *n).collect();
        if hits.is_empty() { None } else { Some(if hits.len() == 1 { Ok(hits[0]) } else { Err(()) }) }
    }).ok_or(format!("移動設定の骨を一意に特定できません: {name}"))?.map_err(|_| format!("移動設定の骨を一意に特定できません: {name}"))?;
    let mut ancestors = Vec::new();
    let mut p = skel.parent[node];
    while let Some(a) = p { ancestors.push(a); p = skel.parent[a]; }
    let rest_turns: Vec<Quat> = ancestors.iter().map(|&a| skel.r[a]).collect();
    let mut out = Vec::new();
    for (action, clip) in clips {
        if !(clip.duration > 0.0) { return Err(format!("移動設定には通常blendの動作が必要です: {}", clip.name)); }
        let mut translations = 0;
        for t in &clip.tracks {
            if t.node == node && t.prop == Prop::T { translations += 1; }
            if !ancestors.contains(&t.node) || !(t.prop == Prop::T || t.prop == Prop::S) { continue; }
            for i in t.size..t.values.len() {
                if (t.values[i] - t.values[i % t.size]).abs() > 1e-7 { return Err(format!("移動設定の祖先に位置/scaleの変動があります: {}", clip.name)); }
            }
        }
        if translations != 1 { return Err(format!("移動設定の位置trackを一意に特定できません: {}", clip.name)); }
        let turns = ancestors.iter().map(|&a| clip.tracks.iter().find(|t| t.node == a && t.prop == Prop::R).cloned()).collect();
        let rest = position_in_model(skel, node);
        let mut m = skel.clone();
        write_clip_at(clip, 0.0, &mut m);
        let start = position_in_model(&m, node);
        write_clip_at(clip, clip.duration, &mut m);
        let delta = position_in_model(&m, node) - start;
        out.push(RootClip { action: *action, rest, start, delta, duration: clip.duration, turns });
    }
    Ok(Some(RootMotion { ancestors, rest_turns, clips: out, linear }))
}

impl RootMotion {
    /// 動作を混ぜたあと。体の根の移動を打ち消す量（wrapper の位置）を返す
    fn apply(&self, skel: &Skeleton, mixer: &Mixer) -> Vec3 {
        let mut delta = Vec3::ZERO;
        let mut now = Quat::IDENTITY;
        for &a in &self.ancestors { now = skel.r[a] * now; }
        let mut q = [0f32; 4];
        for c in &self.clips {
            let act = &mixer.actions[c.action];
            let mut part = c.rest - c.start + c.delta * (-act.time / c.duration as f64) as f32;
            let mut alone = Quat::IDENTITY;
            for (i, t) in c.turns.iter().enumerate() {
                let turn = match t { Some(tr) => { tr.eval(act.time as f32, &mut q); Quat::from_xyzw(q[0], q[1], q[2], q[3]) } None => self.rest_turns[i] };
                alone = turn * alone;
            }
            part = (now * alone.inverse()) * part;
            delta += part * act.effective_weight;
        }
        let d = self.linear * delta;
        Vec3::new(d.x, 0.0, d.z)
    }
}

// ---- 顔のモーフ ----

#[derive(Clone, Default)]
struct FaceKey {
    index: usize,
    /// blink(Left|Right)Inbetween の中心
    corrective: Option<f32>,
    /// 0 = blinkLeft・1 = blinkRight（名前がそのもの）
    lr: Option<usize>,
    /// 名前が blinkLeft / blinkRight で始まる（戦う顔）
    side: Option<usize>,
    mouth_close: bool,
    /// 話す口が書く 3 つの口と閉じた口（mouthClose・aa・ee・ou のどれか。applySpeechMouth）
    speech: Option<&'static str>,
    /// 台帳の表情の持ち主: Some(true) は まばたき、Some(false) は 0 を書くモーフ
    profile: Option<bool>,
    /// 台帳の表情（FaceProfile.keys の番号）のうち、このモーフを指すもの
    sems_idx: Vec<usize>,
    /// まばたきのモーフか（＞＜ で閉じる）
    blink_name: bool,
    /// ＞＜ の間に重ねる口
    mouth: Option<f32>,
    mouth_inb: Option<f32>,
    /// 名前が …Arc: (COMBAT_KEYS の番号, blink か)
    arc: Option<(Option<usize>, bool)>,
    /// COMBAT_KEYS の番号
    sem: Option<usize>,
}

#[derive(Clone)]
struct Face {
    node: usize,
    keys: Vec<FaceKey>,
    /// 読んだときのモーフの重み（character-runtime の collectMorphFaces の neutral。話し終えた口を戻す値）
    neutral: Vec<f32>,
}

/// 表情（character-runtime の CharacterFaceControl。どの場面で出すかはゲーム）。名前は "neutral" か台帳の semantic
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceControl {
    pub emotion: &'static str,
    pub emotion_weight: f32,
    pub mouth: &'static str,
    pub mouth_weight: f32,
    pub look: &'static str,
    pub look_weight: f32,
}

/// 台帳の表情の表（character-runtime の composeCharacterFaceWeights）
#[derive(Clone, Default)]
struct FaceProfile {
    keys: Vec<String>,
    /// keys ごと: blink・mouth・lookAt の上書き（0 none・1 blend・2 block）
    rules: Vec<[u8; 3]>,
    /// keys ごと: 分類（0 blink・1 mouth・2 lookAt）
    cats: Vec<Option<usize>>,
}
impl FaceProfile {
    fn new(profile: &Value) -> FaceProfile {
        let mut p = FaceProfile::default();
        let Some(ms) = profile.get("morphs").and_then(Value::as_object) else { return p };
        for key in ms.keys() {
            let rule = |cat: &str| match profile.get("overrides").and_then(|o| o.get(key.as_str())).and_then(|o| o.get(cat)).and_then(Value::as_str) { Some("blend") => 1u8, Some("block") => 2u8, _ => 0u8 };
            p.rules.push([rule("blink"), rule("mouth"), rule("lookAt")]);
            p.cats.push(if ["aa", "ih", "ou", "ee", "oh"].contains(&key.as_str()) { Some(1) } else if key.starts_with("blink") { Some(0) } else if key.starts_with("look") { Some(2) } else { None });
            p.keys.push(key.clone());
        }
        p
    }
    /// composeCharacterFace。vowels は話す口（sampleFiveMouthSpeech の母音。あれば表情の口の代わり）。out は keys の順
    fn compose(&self, control: Option<&FaceControl>, blink: f32, vowels: Option<[(&str, f64); 5]>, out: &mut Vec<f32>) {
        out.clear();
        out.resize(self.keys.len(), 0.0);
        let clamp = |v: f32| if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
        let mut set = |name: &str, v: f32| { if let Some(i) = self.keys.iter().position(|k| k == name) { out[i] = clamp(v); } };
        // あとの指定が勝つ（TS と同じ順）
        if let Some(c) = control {
            if c.emotion != "neutral" { set(c.emotion, c.emotion_weight); }
            if c.look != "neutral" { set(c.look, c.look_weight); }
        }
        match vowels {
            Some(v) => for (k, w) in v { set(k, w as f32); },
            None => if let Some(c) = control && c.mouth != "neutral" { set(c.mouth, c.mouth_weight); },
        }
        set("blinkLeft", blink);
        set("blinkRight", blink);
        let mut mult = [1.0f32; 3];
        for (i, rules) in self.rules.iter().enumerate() {
            for (c, r) in rules.iter().enumerate() {
                mult[c] -= match *r { 1 => out[i], 2 if out[i] > 0.0 => 1.0, _ => 0.0 };
            }
        }
        for (i, cat) in self.cats.iter().enumerate() {
            if let Some(c) = cat { out[i] *= mult[*c].max(0.0); }
        }
    }
}

fn corrective_center(name: &str) -> Option<f32> {
    let tail = name.strip_prefix("blinkLeftInbetween").or_else(|| name.strip_prefix("blinkRightInbetween"))?;
    match tail { "25" => Some(0.25), "50" => Some(0.5), "75" => Some(0.75), _ => None }
}

// ---- モデルの形（GPU へ上げる前） ----

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct SkinVertex { pub joints: [u16; 4], pub weights: [f32; 4] }

/// モーフの差分（位置と法線。頂点ごと・モーフごと）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct MorphDelta { pub pos: [f32; 3], pub nor: [f32; 3] }

/// 描く 1 つ（three の Mesh 1 つ）
#[derive(Clone, Debug)]
pub struct ActorPrim {
    pub name: String,
    pub node: usize,
    pub rigid: bool,
    pub material: usize,
    pub vstart: u32,
    pub vcount: u32,
    pub istart: u32,
    pub icount: u32,
    pub morph_base: u32,
    pub morph_count: u32,
    pub weight_base: u32,
    pub vertex_colors: bool,
    pub visible: bool,
}

pub struct Current {
    kind: ActionKind,
    at: f64,
    seconds: f64,
    cued: u8,
    land_at: Option<f64>,
    off_at: Option<f64>,
    left_ground: bool,
    air: f64,
    play: Option<(Option<f64>, Option<f64>, bool, Option<f64>)>,
}
fn cue_bit(c: Cue) -> u8 { match c { Cue::Strike => 1, Cue::Takeoff => 2, Cue::Land => 4, Cue::Seated => 8 } }

#[derive(Clone, Copy, PartialEq)]
enum SkillMode { Model, File, Fallback }

/// 主役 1 人（three の Object3D の木の代わりに、骨の表と根の行列を持つ）。Send
pub struct CharacterActor {
    pub id: String,
    pub revision: String,
    pub name: String,
    pub quality: String,
    pub model_sha256: String,
    pub height_meters: f32,
    pub glb: Arc<Glb>,
    pub skel: Skeleton,
    pub vertices: Vec<Vertex>,
    pub skin: Vec<SkinVertex>,
    pub indices: Vec<u32>,
    pub deltas: Vec<MorphDelta>,
    pub prims: Vec<ActorPrim>,
    pub materials: Vec<BasicMaterial>,
    pub palette: Palette,
    /// モーフの重み（全部を 1 列に）
    pub morph_weights: Vec<f32>,
    morph_nodes: Vec<(usize, usize)>,
    pub hurt: Option<HurtFace>,
    pub hurt_joint: u16,
    /// ゲームが場に置く入れ物の世界の行列（update の前に set_parent_world で渡す）
    parent_world: Mat4,
    body_pos: Vec3,
    wrapper_pos: Vec3,
    /// normalized × scene（高さの拡大・前向き・足もとの位置）
    norm_m: Mat4,
    gait: Gait,

    // character-runtime の体
    mixer: Mixer,
    idle: ActionId,
    walk: ActionId,
    run: Option<ActionId>,
    root_motion: Option<RootMotion>,
    /// 揺れ物（配布の full の一部の主役だけ。lite・normal のモデルには入っていない）
    pub spring: Option<SpringMotion>,
    a_blend: f32,
    a_run_blend: f32,
    a_time: f64,
    a_body_time: f64,
    faces: Vec<Face>,
    face_profile: FaceProfile,
    face_control: Option<FaceControl>,
    face_values: Vec<f32>,
    has_profile: bool,
    neutral_mouth_close: Option<f32>,

    // 重ねる動き
    overlays: Vec<Overlay>,
    swing: Option<usize>,
    ready: Option<usize>,
    jump: Option<(usize, BodyMeasure)>,
    dodge: Option<(usize, BodyMeasure)>,
    banzai: Option<usize>,
    greet: Option<usize>,
    hit_ov: Option<(usize, MotionSource)>,
    rest: Option<(usize, usize, usize, MotionSource)>,
    skill: Option<(usize, SkillMode)>,
    base: Base,
    still: Option<StillBones>,
    level_obj: Option<usize>,
    levelled: Levelled,
    feet: Vec<usize>,
    foot_y: f32,
    combat_has: [bool; 13],
    pub motions: Vec<(MotionKind, MotionInfo)>,

    idle_clip: Option<Arc<Clip>>,
    socket: Option<WeaponSocket>,
    weapon_scale: f32,
    grip_poses: Option<WeaponHold>,
    grip: Option<GripPose>,

    rate: f64,
    clock: f64,
    face_time: f64,
    speed: f64,
    grounded: bool,
    was_grounded: bool,
    armed: f32,
    ready_w: f32,
    air_w: f32,
    hit_at: Option<f64>,
    hit_seconds: Option<f64>,
    current: Option<Current>,
    skill_swing_at: Option<f64>,
    events: Vec<ActorEvent>,
    runtime_started: bool,

    // character-runtime の入力のうち、速さと動作の外の物（PackageActor・NPC の CharacterActor の props）
    /// isNpc: 体は 30 回/秒・顔は 60 回/秒に間引く
    npc: bool,
    photo: Option<PhotoPose>,
    speech: Option<CharacterSpeech>,
    /// moving・running をそのまま渡す（速さと歩き方から決めない）
    runtime_flags: Option<(bool, bool)>,
    /// 揺れ物を次のコマで付け直す（springReset の番号が変わった）
    spring_reset: bool,
    /// 揺れ物が前のコマに見た写真の姿（替わったら付け直す）
    spring_photo: Option<PhotoPose>,
    was_speaking: bool,
    body_tick: i64,
    face_tick: i64,
}

/// Pristine runtime state for instances that use an already-uploaded GPU model.
/// Contains no CPU geometry, decoded images, or reference to the source full GLB.
/// Create immediately after assemble, before weapon attachment or phase seeding.
pub struct CharacterRuntimeTemplate {
    initial: CharacterActor,
}

impl CharacterRuntimeTemplate {
    /// Each instance owns its skeleton, mixer/cursors, clocks, palette, morph
    /// weights and display/action state. Immutable Clip/keyframe Arcs are shared.
    /// Use the renderer's shared-GPU instance path, not a fresh model upload.
    pub fn spawn(&self) -> CharacterActor {
        self.initial.copy_runtime(self.initial.glb.clone())
    }
}

/// three の GLTFLoader がメッシュに付ける名前（1 プリミティブなら節点の名前）
fn prim_name(g: &Glb, skel: &Skeleton, node: usize, mesh: usize, i: usize) -> String {
    let m = &g.meshes[mesh];
    if m.primitives.len() == 1 && !skel.names[node].is_empty() { return skel.names[node].clone(); }
    let base = if m.name.is_empty() { format!("mesh_{mesh}") } else { anim::sanitize_name(&m.name) };
    if i == 0 { base } else { format!("{base}_{i}") }
}

fn basic_material(g: &Glb, index: Option<usize>) -> BasicMaterial {
    // 材質が無いプリミティブは glTF の既定（白）
    let Some(i) = index else { return BasicMaterial { map: None, color: [1.0; 3], opacity: 1.0, double_sided: false, transparent: false, depth_write: true, alpha_test: 0.0 } };
    let m = &g.materials[i];
    // 見える色を全部 emission に持つ GLB: PBR の色が黒で emissive が実際の色
    let black = m.base_color[0] == 0.0 && m.base_color[1] == 0.0 && m.base_color[2] == 0.0;
    let emissive_only = m.texture.is_none() && black && (m.emissive_texture.is_some() || m.emissive != [0.0; 3]);
    let strength = g.json["materials"][i]["extensions"]["KHR_materials_emissive_strength"]["emissiveStrength"].as_f64().unwrap_or(1.0) as f32;
    let (map, color) = if emissive_only {
        (m.emissive_texture, [m.emissive[0] * strength, m.emissive[1] * strength, m.emissive[2] * strength])
    } else {
        (m.texture, [m.base_color[0], m.base_color[1], m.base_color[2]])
    };
    BasicMaterial { map, color, opacity: m.base_color[3], double_sided: m.double_sided, transparent: m.alpha_blend, depth_write: !m.alpha_blend, alpha_test: m.alpha_cutoff.unwrap_or(0.0) }
}

/// three の Box3.setFromObject(gltf.scene)。骨のあるメッシュは今の姿の頂点、骨の無いメッシュはアクセサの min/max とモーフの最大の差
fn model_bounds(g: &Glb, skel: &Skeleton, prims: &[PrimRef], skin_mats: &[Vec<Mat4>]) -> (Vec3, Vec3) {
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    let mut union = |bmin: Vec3, bmax: Vec3, m: Mat4| {
        if bmin.x > bmax.x { return; }
        for i in 0..8 {
            let p = Vec3::new(if i & 1 == 0 { bmin.x } else { bmax.x }, if i & 2 == 0 { bmin.y } else { bmax.y }, if i & 4 == 0 { bmin.z } else { bmax.z });
            let w = m.transform_point3(p);
            lo = lo.min(w);
            hi = hi.max(w);
        }
    };
    for pr in prims {
        let mw = skel.world[pr.node];
        if let Some(s) = pr.skin {
            let inv = mw.inverse();
            let (mut bmin, mut bmax) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
            let infl = &skel.morphs[pr.node];
            for i in 0..pr.p.positions.len() {
                let mut p = Vec3::from(pr.p.positions[i]);
                for (k, w) in infl.iter().enumerate() { if *w != 0.0 && k < pr.p.morph_pos.len() { p += Vec3::from(pr.p.morph_pos[k][i]) * *w; } }
                let (j, w) = (pr.p.joints[i], pr.weights[i]);
                let mut o = Vec3::ZERO;
                for k in 0..4 { if w[k] != 0.0 { o += skin_mats[s][j[k] as usize].transform_point3(p) * w[k]; } }
                let l = inv.transform_point3(o);
                bmin = bmin.min(l);
                bmax = bmax.max(l);
            }
            union(bmin, bmax, mw);
        } else {
            let mesh = g.nodes[pr.node].mesh.unwrap();
            let pi = g.meshes[mesh].primitives.iter().position(|x| std::ptr::eq(x, pr.p)).unwrap_or(0);
            let (bmin, bmax) = accessor_bounds(g, mesh, pi).unwrap_or_else(|| {
                let (mut a, mut b) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
                for p in &pr.p.positions { a = a.min(Vec3::from(*p)); b = b.max(Vec3::from(*p)); }
                (a, b)
            });
            union(bmin, bmax, mw);
        }
    }
    (lo, hi)
}

fn accessor_bounds(g: &Glb, mesh: usize, prim: usize) -> Option<(Vec3, Vec3)> {
    let pr = g.json.get("meshes")?.get(mesh)?.get("primitives")?.get(prim)?;
    let acc = g.json.get("accessors")?.as_array()?;
    let scale = |a: &Value| -> f32 {
        if !a.get("normalized").and_then(Value::as_bool).unwrap_or(false) { return 1.0; }
        match a.get("componentType").and_then(Value::as_i64) { Some(5120) => 1.0 / 127.0, Some(5121) => 1.0 / 255.0, Some(5122) => 1.0 / 32767.0, Some(5123) => 1.0 / 65535.0, _ => 1.0 }
    };
    let v3 = |a: &Value, k: &str| -> Option<Vec3> { let v = a.get(k)?.as_array()?; Some(Vec3::new(v[0].as_f64()? as f32, v[1].as_f64()? as f32, v[2].as_f64()? as f32)) };
    let a = acc.get(pr.get("attributes")?.get("POSITION")?.as_u64()? as usize)?;
    let s = scale(a);
    let (mut lo, mut hi) = (v3(a, "min")? * s, v3(a, "max")? * s);
    if let Some(ts) = pr.get("targets").and_then(Value::as_array) {
        let mut md = Vec3::ZERO;
        for t in ts {
            let Some(i) = t.get("POSITION").and_then(Value::as_u64) else { continue };
            let Some(ta) = acc.get(i as usize) else { continue };
            let (Some(a0), Some(a1)) = (v3(ta, "min"), v3(ta, "max")) else { continue };
            md = md.max(a0.abs().max(a1.abs()) * scale(ta));
        }
        lo -= md;
        hi += md;
    }
    Some((lo, hi))
}

/// 配布から主役を読む（版を決めてから、要るファイルを一度に取る。全部の SHA を確かめてから使う。部分だけ取れた版と前の版を混ぜない）
pub fn load_character_actor(client: &DeliveryClient, o: LoadOptions) -> Result<CharacterActor, String> {
    let rev = o.revision.as_deref();
    let not = || format!("{} は配布されていません", o.id);
    let listed = client.manifest(&o.id, rev)?.ok_or_else(not)?;
    let mut plan = release_plan(&listed, o.quality);
    let mut prepared: Arc<Prepared> = client.prepare(&o.id, rev, &plan.files, true)?.ok_or_else(not)?;
    plan = release_plan(&prepared.manifest, o.quality);
    if plan.files.iter().any(|f| !prepared.data.contains_key(f)) {
        let r2 = prepared.manifest["revision"].as_str().map(String::from);
        prepared = client.prepare(&o.id, r2.as_deref(), &plan.files, true)?.ok_or_else(not)?;
    }
    assemble(&o, &plan, &prepared.manifest, &prepared.data)
}

/// 取ったファイルから組み立てる（character-actor の assemble + character-runtime の createCharacterInstance）
pub fn assemble(o: &LoadOptions, plan: &ReleasePlan, manifest: &Value, data: &HashMap<String, Arc<Vec<u8>>>) -> Result<CharacterActor, String> {
    let pm = &manifest["packageManifest"];
    let g = Arc::new(glb::parse(data.get(&plan.model).ok_or("モデルがありません")?)?);
    let mut skel = Skeleton::from_glb(&g);
    let spring = SpringMotion::from_glb(&g, &mut skel);

    // ---- プリミティブ（検査・光なしの材質） ----
    let mut materials: Vec<BasicMaterial> = Vec::new();
    let mut mat_index: HashMap<Option<usize>, usize> = HashMap::new();
    let mut prim_refs: Vec<PrimRef> = Vec::new();
    for &node in &skel.order {
        let Some(mesh) = g.nodes[node].mesh else { continue };
        let skin = g.nodes[node].skin;
        for (pi, p) in g.meshes[mesh].primitives.iter().enumerate() {
            if p.positions.len() < 3 { return Err("モデルの頂点データが不完全です。".into()); }
            let material = *mat_index.entry(p.material).or_insert_with(|| { materials.push(basic_material(&g, p.material)); materials.len() - 1 });
            // three の normalizeSkinWeights
            // 骨の番号が skin の外（壊れた持ち込みの GLB）。three は描き崩れるだけだが、ここでは読めないとする
            if let Some(sk) = skin && p.joints.iter().any(|j| j.iter().any(|&k| k as usize >= g.skins[sk].joints.len())) { return Err("モデルの頂点データが不完全です。".into()); }
            if skin.is_some() && (p.joints.len() != p.positions.len() || p.weights.len() != p.positions.len()) { return Err("モデルの頂点データが不完全です。".into()); }
            let weights: Vec<[f32; 4]> = if skin.is_some() {
                p.weights.iter().map(|w| {
                    let s = w[0].abs() + w[1].abs() + w[2].abs() + w[3].abs();
                    if s == 0.0 { [1.0, 0.0, 0.0, 0.0] } else { [w[0] / s, w[1] / s, w[2] / s, w[3] / s] }
                }).collect()
            } else { Vec::new() };
            prim_refs.push(PrimRef { name: prim_name(&g, &skel, node, mesh, pi), node, p, weights, skin, material, targets: &g.meshes[mesh].target_names });
        }
    }
    if prim_refs.is_empty() { return Err("モデルに表示できるメッシュがありません。".into()); }

    // ---- 大きさと向きをそろえる（character-runtime の normalizeCharacter） ----
    let skin_mats_of = |s: &Skeleton| -> Vec<Vec<Mat4>> { g.skins.iter().map(|sk| sk.joints.iter().zip(&sk.inverse_bind).map(|(j, ib)| s.world[*j] * *ib).collect()).collect() };
    let skin_mats0 = skin_mats_of(&skel);
    let (lo, hi) = model_bounds(&g, &skel, &prim_refs, &skin_mats0);
    let size = hi - lo;
    if !size.y.is_finite() || size.y <= 0.0 { return Err("モデルの大きさを読み取れません。".into()); }
    let center = (lo + hi) * 0.5;
    let height = pm["coordinates"]["heightMeters"].as_f64().unwrap_or(1.0) as f32;
    let norm_scale = height / size.y;
    let rot_y = match pm["coordinates"]["forward"].as_str() { Some("+X") => -std::f32::consts::FRAC_PI_2, Some("-X") => std::f32::consts::FRAC_PI_2, Some("-Z") => std::f32::consts::PI, _ => 0.0 };
    let norm_q = Quat::from_rotation_y(rot_y);
    let normalized = Mat4::from_scale_rotation_translation(Vec3::splat(norm_scale), norm_q, Vec3::ZERO);
    let scene_pos = Vec3::new(-center.x, -lo.y, -center.z);
    let norm_m = normalized * Mat4::from_translation(scene_pos);
    skel.update_world(norm_m);

    // ---- 待機・歩き・走り ----
    let own_clips = Clip::all_from_glb(&g, &skel);
    let clips_of = &pm["motion"]["clips"];
    let own_named = |name: Option<&str>| -> Option<Arc<Clip>> { name.and_then(|n| Clip::find(&own_clips, n).cloned()) };
    let idle_c = own_named(clips_of["idle"].as_str()).ok_or("待機・歩行の動作を読み込めません。")?;
    let walk_c = own_named(clips_of["walk"].as_str()).ok_or("待機・歩行の動作を読み込めません。")?;
    let run_c = own_named(clips_of["run"].as_str());
    let mut mixer = Mixer::new();
    let idle = mixer.add(idle_c.clone());
    let walk = mixer.add(walk_c.clone());
    let run = run_c.clone().map(|c| mixer.add(c));
    let mut rm_clips = vec![(idle, idle_c.clone()), (walk, walk_c.clone())];
    if let (Some(a), Some(c)) = (run, &run_c) { rm_clips.push((a, c.clone())); }
    let root_motion = create_root_motion(&skel, pm["motion"].get("rootMotion").filter(|v| !v.is_null()), &rm_clips, Mat3::from_quat(norm_q) * norm_scale)?;
    mixer.play(idle, &skel);
    mixer.set_effective_weight(walk, 0.0);
    mixer.play(walk, &skel);
    if let Some(r) = run { mixer.set_effective_weight(r, 0.0); mixer.play(r, &skel); }

    // ---- bind の姿・＞＜ ----
    let root_rot = norm_m.to_scale_rotation_translation().1;
    let bind: BindPose = mo::capture_bind_pose(&skel, root_rot);
    let skin_mats = skin_mats_of(&skel);
    let hurt = plan.hurt_face.as_ref().and_then(|style| {
        let c = Ctx { g: &g, skel: &skel, skin_mats: skin_mats.clone(), prims: &prim_refs, materials: &materials };
        hurt_face::create_hurt_face(&c, style, Vec3::Z)
    });

    // ---- 頂点の表 ----
    let mut palette = Palette::new();
    let (mut vertices, mut skin, mut indices, mut deltas): (Vec<Vertex>, Vec<SkinVertex>, Vec<u32>, Vec<MorphDelta>) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut prims: Vec<ActorPrim> = Vec::new();
    let mut morph_weights: Vec<f32> = Vec::new();
    let mut morph_nodes: Vec<(usize, usize)> = Vec::new();
    for pr in &prim_refs {
        let p = pr.p;
        let vstart = vertices.len() as u32;
        let vcount = p.positions.len();
        let istart = indices.len() as u32;
        let has_color = p.colors.len() == vcount;
        for i in 0..vcount {
            vertices.push(Vertex { pos: p.positions[i], normal: p.normals[i], uv: p.uvs[i], color: if has_color { p.colors[i] } else { [1.0; 4] }, sway: 0.0 });
        }
        match pr.skin {
            Some(s) => {
                let j = palette.remap_skin(&g.skins[s], &p.joints, &pr.weights);
                for i in 0..vcount { skin.push(SkinVertex { joints: j[i], weights: pr.weights[i] }); }
            }
            None => {
                let j = palette.add(pr.node, Mat4::IDENTITY);
                for _ in 0..vcount { skin.push(SkinVertex { joints: [j, 0, 0, 0], weights: [1.0, 0.0, 0.0, 0.0] }); }
            }
        }
        indices.extend_from_slice(&p.indices);
        let morph_base = deltas.len() as u32;
        let morph_count = p.morph_pos.len() as u32;
        for t in 0..p.morph_pos.len() {
            for i in 0..vcount { deltas.push(MorphDelta { pos: p.morph_pos[t][i], nor: p.morph_nor.get(t).map(|n| n[i]).unwrap_or([0.0; 3]) }); }
        }
        let weight_base = if morph_count > 0 {
            match morph_nodes.iter().find(|(n, _)| *n == pr.node) {
                Some((_, b)) => *b as u32,
                None => {
                    let b = morph_weights.len();
                    morph_weights.resize(b + skel.morphs[pr.node].len(), 0.0);
                    morph_nodes.push((pr.node, b));
                    b as u32
                }
            }
        } else { 0 };
        prims.push(ActorPrim {
            name: pr.name.clone(), node: pr.node, rigid: pr.skin.is_none(), material: pr.material, vstart, vcount: vcount as u32, istart, icount: p.indices.len() as u32,
            morph_base, morph_count, weight_base, vertex_colors: has_color, visible: true,
        });
    }
    let hurt_joint = hurt.as_ref().map(|h| palette.add(h.head, h.head_inverse)).unwrap_or(0);

    // ---- 重ねる動き ----
    let still = plan.still.clone();
    let hold_for = |m: &str| -> Option<HashMap<String, Quat>> {
        let st = still.as_ref()?;
        let clip = Clip::find(&own_clips, &st.pose_clip)?;
        let first = |bone: &str| -> Option<Quat> {
            let n = skel.find(bone)?;
            let t = clip.tracks.iter().find(|t| t.node == n && t.prop == Prop::R)?;
            if t.values.len() < 4 { return None; }
            Some(mo::q_from_array(&t.values, 0).normalize())
        };
        mo::still_pose_for(&bind, st, &first, m)
    };
    let level_obj = still.as_ref().and_then(|s| s.level.as_ref()).and_then(|l| skel.find(&l.bone));
    let feet: Vec<usize> = ["L_Foot", "R_Foot", "J_Bip_L_Foot", "J_Bip_R_Foot", "mixamorigLeftFoot", "mixamorigRightFoot"].iter().filter_map(|n| skel.find(n)).collect();
    let foot_y = if feet.is_empty() { 0.0 } else { feet.iter().map(|&f| skel.world[f].w_axis.y).fold(f32::INFINITY, f32::min) };

    let json_of = |key: &str| -> Option<Value> { serde_json::from_slice(data.get(key)?).ok() };
    let parse_clips = |list: Option<&Value>| -> Vec<NamedClip> {
        list.and_then(Value::as_array).map(|a| a.iter().filter_map(NamedClip::from_json).filter(|c| c.duration > 0.0).collect()).unwrap_or_default()
    };
    let glb_clip = |key: &str, name: &str| -> Option<NamedClip> {
        let b = data.get(key)?;
        NamedClip::from_glb(&glb::parse(b).ok()?, name)
    };
    let named_find = |list: &[NamedClip], name: Option<&str>| -> Option<Clip> { let n = name?; list.iter().find(|c| c.name == n).map(|c| c.bind(&skel)) };

    let mut overlays: Vec<Overlay> = Vec::new();
    let mut push_ov = |ov: Overlay| -> usize { overlays.push(ov); overlays.len() - 1 };

    // 武器の動作（構え・振り・持つ位置）。登録表が指すモデルで作った配布だけ。無ければモデル自身の振り
    let pack = if plan.weapon { json_of("weapon") } else { None };
    let pack_ok = pack.as_ref().is_some_and(|p| p.get("modelSha256").and_then(Value::as_str).map(String::from) == plan.weapon_clip_sha256);
    let pack_clips = if pack_ok { parse_clips(pack.as_ref().and_then(|p| p.get("clips"))) } else { Vec::new() };
    let own_or = |pack_name: &str, key: &str| -> Option<Clip> {
        named_find(&pack_clips, Some(pack_name)).or_else(|| Clip::find(&own_clips, clips_of[key].as_str().unwrap_or(pack_name)).map(|c| (**c).clone()))
    };
    let swing_clip = own_or("sword_attack", "swordAttack");
    let ready_clip = own_or("sword_idle", "swordIdle");
    // 体の高さを Root が持つ待機と Hip が持つ武器の動作がある。下の動きの節点をアニメ前の姿へ戻さないと、体が宙に浮く
    let loco: [Option<&Clip>; 3] = [Some(&*idle_c), Some(&*walk_c), run_c.as_deref()];
    let weapon_ov = |clip: &Option<Clip>, under: &[Option<&Clip>]| -> Option<Overlay> {
        let o = Overlay::new(&skel, clip.as_ref()?, true, under);
        if o.ok() { Some(o) } else { None }
    };
    let under_swing: [Option<&Clip>; 4] = [loco[0], loco[1], loco[2], ready_clip.as_ref()];
    let swing = weapon_ov(&swing_clip, &under_swing).map(&mut push_ov);
    let ready = weapon_ov(&ready_clip, &loco).map(&mut push_ov);
    let socket = if pack_ok {
        pack.as_ref().and_then(|p| p.get("socket")).and_then(|s| {
            let v3 = |k: &str| -> Option<Vec3> { let a = s.get(k)?.as_array()?; Some(Vec3::new(a[0].as_f64()? as f32, a[1].as_f64()? as f32, a[2].as_f64()? as f32)) };
            Some(WeaponSocket { bone: s.get("bone")?.as_str()?.to_string(), point: v3("point")?, blade: v3("blade")?, palm: v3("palm")? })
        })
    } else { None };
    let weapon_scale = pack.as_ref().and_then(|p| p.get("weaponScale")).and_then(Value::as_f64).unwrap_or(1.0) as f32;

    // 本人用の動き（配布の clip:*.json。{ modelSha256, clips }。当て直さずに再生する）
    let native_pack = |pk: &Option<NativePack>| -> Vec<NamedClip> {
        let Some(pk) = pk else { return Vec::new() };
        let Some(p) = json_of(&pk.file) else { return Vec::new() };
        if p.get("modelSha256").and_then(Value::as_str) == Some(plan.model_sha256.as_str()) { parse_clips(p.get("clips")) } else { Vec::new() }
    };
    let (action_clips, body_clips) = (native_pack(&plan.native_actions), native_pack(&plan.native_body));
    let over = |c: Option<Clip>, all: bool| -> Option<Overlay> {
        let o = Overlay::new(&skel, c.as_ref()?, all, &[]);
        if o.ok() { Some(o) } else { None }
    };
    let aname = |k: &str| plan.native_actions.as_ref().and_then(|a| a.names.get(k)).map(|s| s.as_str());
    let chain0 = norm_m;
    let jump = over(named_find(&action_clips, aname("jump")), true).map(|ov| { let m = measure_body(&skel, chain0, &ov); (push_ov(ov), m) });
    let dodge = over(named_find(&action_clips, aname("dodge")), true).map(|ov| { let m = measure_body(&skel, chain0, &ov); (push_ov(ov), m) });
    let banzai = over(named_find(&action_clips, aname("banzai")), true).map(&mut push_ov);
    let greet = over(named_find(&action_clips, aname("greet")), true).map(&mut push_ov);

    // のけぞり・休止: 本人用 → 共通（骨に当て直す）→ のけぞりだけ手作り（ぱっちむ型の骨）
    let mut body_motion: HashMap<&str, (usize, MotionSource)> = HashMap::new();
    for (key, clip_name, bk) in BODY_CLIP_NAMES {
        let own = over(named_find(&body_clips, plan.native_body.as_ref().and_then(|b| b.names.get(key)).map(|s| s.as_str())), true);
        if let Some(ov) = own { body_motion.insert(key, (push_ov(ov), MotionSource::Own)); continue; }
        let file = plan.shared.iter().find(|(k, _)| *k == key).map(|(_, f)| f.as_str());
        let src = file.and_then(|f| glb_clip(f, clip_name));
        let fitted = src.and_then(|s| mo::fit_body_clip(&s, &bind, clip_name, &mo::body_fit_options(&bind, bk, hold_for(if key == "hit" { "hit" } else { "rest" }))));
        if let Some(ov) = over(fitted.map(|f| f.bind(&skel)), false) { body_motion.insert(key, (push_ov(ov), MotionSource::Shared)); }
    }
    if !body_motion.contains_key("hit") && let Some(ov) = over(mo::paw_hit_clip(&bind).map(|f| f.bind(&skel)), false) {
        body_motion.insert("hit", (push_ov(ov), MotionSource::Fallback));
    }
    let hit_ov = body_motion.get("hit").copied();
    let rest = match (body_motion.get("restDown"), body_motion.get("restLoop"), body_motion.get("restUp")) {
        (Some(d), Some(l), Some(u)) => Some((d.0, l.0, u.0, d.1)),
        _ => None,
    };
    // 必殺技: 本人用（モデルの中、または配布のクリップを骨に当て直す）→ 代わりは武器の振り
    let mut skill: Option<(usize, SkillMode)> = None;
    if let Some(name) = &plan.skill_model {
        if let Some(ov) = over(Clip::find(&own_clips, name).map(|c| (**c).clone()), true) { skill = Some((push_ov(ov), SkillMode::Model)); }
    } else if let Some((file, name)) = &plan.skill_file {
        let fitted = glb_clip(file, name).and_then(|s| mo::fit_body_clip(&s, &bind, name, &mo::body_fit_options(&bind, BodyKey::RestLoop, hold_for("skill"))));
        if let Some(ov) = over(fitted.map(|f| f.bind(&skel)), false) { skill = Some((push_ov(ov), SkillMode::File)); }
    }
    if skill.is_none() && let Some(s) = swing { skill = Some((s, SkillMode::Fallback)); }

    let all_ov: Vec<Option<&Overlay>> = [swing, ready, jump.as_ref().map(|j| j.0), dodge.as_ref().map(|j| j.0), banzai, greet, hit_ov.map(|h| h.0), rest.map(|r| r.0), rest.map(|r| r.1), rest.map(|r| r.2), skill.map(|s| s.0)]
        .iter().map(|i| i.map(|i| &overlays[i])).collect();
    let base = Base::new(&all_ov, &skel);

    // ---- 顔 ----
    let profile = pm["motion"].get("face").filter(|v| !v.is_null());
    let morph = |semantic: &str| -> String { profile.and_then(|p| p.get("morphs")).and_then(|m| m.get(semantic)).and_then(Value::as_str).unwrap_or(semantic).to_string() };
    let blink_names: Vec<String> = if profile.is_some() { vec![morph("blinkLeft"), morph("blinkRight")] } else { vec!["blinkLeft".into(), "blinkRight".into(), "blink".into()] };
    let mouth: Vec<(String, f32)> = plan.hurt_face.as_ref().map(|s| s.mouth.iter().map(|(k, v)| (morph(k), *v)).collect()).unwrap_or_default();
    let mut combat_has = [false; 13];
    let face_profile = profile.map(FaceProfile::new).unwrap_or_default();
    let mut faces: Vec<Face> = Vec::new();
    for (node, _) in &morph_nodes {
        let mesh = g.nodes[*node].mesh.unwrap();
        let names = &g.meshes[mesh].target_names;
        let mut keys = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let mut k = FaceKey { index, ..Default::default() };
            k.corrective = corrective_center(name);
            k.lr = if name == "blinkLeft" { Some(0) } else if name == "blinkRight" { Some(1) } else { None };
            k.side = if name.starts_with("blinkLeft") { Some(0) } else if name.starts_with("blinkRight") { Some(1) } else { None };
            k.mouth_close = name == "mouthClose";
            k.speech = ["mouthClose", "aa", "ee", "ou"].into_iter().find(|n| n == name);
            if let Some(ms) = profile.and_then(|p| p.get("morphs")).and_then(Value::as_object) {
                let sems: Vec<&String> = ms.iter().filter(|(_, v)| v.as_str() == Some(name.as_str())).map(|(s, _)| s).collect();
                if !sems.is_empty() { k.profile = Some(sems.iter().any(|s| *s == "blinkLeft" || *s == "blinkRight")); }
                // 0.2.7: 左右のまばたきが同じモーフを指せる（大きいほう）
                k.sems_idx = sems.iter().filter_map(|s| face_profile.keys.iter().position(|f| f == *s)).collect();
            }
            k.blink_name = blink_names.contains(name);
            k.mouth = mouth.iter().rev().find(|(n, _)| n == name).map(|(_, v)| *v);
            k.mouth_inb = name.strip_prefix("mouthCloseInbetween").and_then(|t| match t { "25" => Some(0.25), "50" => Some(0.5), "75" => Some(0.75), _ => None });
            if let Some(key) = name.strip_suffix("Arc") { k.arc = Some((mo::COMBAT_KEYS.iter().position(|c| *c == key), key.starts_with("blink"))); }
            k.sem = mo::COMBAT_KEYS.iter().position(|c| *c == name);
            if let Some(s) = k.sem { combat_has[s] = true; }
            keys.push(k);
        }
        faces.push(Face { node: *node, keys, neutral: skel.morphs[*node].clone() });
    }
    let neutral_mouth_close = pm["motion"].get("neutralMouthClose").and_then(Value::as_f64).map(|v| v as f32);

    // ---- 動きの一覧 ----
    let info = |available: bool, source: Option<MotionSource>, secs: Option<f64>, cues: &[Cue]| { let secs = secs.filter(|_| available); MotionInfo { available, source: if available { source } else { None }, seconds: secs.map(|s| s as f32), seconds_exact: secs, cues: if available { cues.to_vec() } else { Vec::new() }, ..Default::default() } };
    let dur = |i: Option<usize>| i.map(|i| overlays[i].seconds);
    let skill_secs = skill.map(|(i, m)| match m { SkillMode::File => mo::SKILL_SECONDS as f64, SkillMode::Fallback => mo::ATTACK_PLAN_WS_SECONDS as f64, SkillMode::Model => overlays[i].seconds });
    let mut dodge_info = info(dodge.is_some(), Some(MotionSource::Own), dodge.as_ref().map(|d| overlays[d.0].seconds), &[]);
    if let Some(d) = &dodge { dodge_info.travel = Some([d.1.travel.x, 0.0, d.1.travel.z]); }
    let mut jump_info = info(jump.is_some(), Some(MotionSource::Own), jump.as_ref().map(|d| overlays[d.0].seconds), &[Cue::Takeoff, Cue::Land]);
    if let Some(j) = &jump { jump_info.height = Some(j.1.top); }
    let mut hit_info = info(hit_ov.is_some(), hit_ov.map(|h| h.1), hit_ov.map(|h| if h.1 == MotionSource::Own { overlays[h.0].seconds } else { mo::HIT_SECONDS_64 }), &[]);
    hit_info.face = Some(hurt.is_some());
    let motions = vec![
        (MotionKind::Idle, info(true, Some(MotionSource::Own), None, &[])),
        (MotionKind::Walk, info(true, Some(MotionSource::Own), None, &[])),
        (MotionKind::Run, info(true, Some(if run.is_some() { MotionSource::Own } else { MotionSource::Fallback }), None, &[])),
        (MotionKind::Ready, info(ready.is_some(), Some(MotionSource::Own), dur(ready), &[])),
        (MotionKind::Attack, info(swing.is_some(), Some(MotionSource::Own), dur(swing), &[Cue::Strike])),
        (MotionKind::Skill, info(skill.is_some(), Some(if skill.is_some_and(|s| s.1 == SkillMode::Fallback) { MotionSource::Fallback } else { MotionSource::Own }), skill_secs, if skill.is_some_and(|s| s.1 == SkillMode::Fallback) { &[Cue::Strike] } else { &[] })),
        (MotionKind::Dodge, dodge_info),
        (MotionKind::Jump, jump_info),
        (MotionKind::Banzai, info(banzai.is_some(), Some(MotionSource::Own), dur(banzai), &[])),
        (MotionKind::Greet, info(greet.is_some(), Some(MotionSource::Own), dur(greet), &[])),
        (MotionKind::Rest, info(rest.is_some(), rest.map(|r| r.3), None, &[Cue::Seated])),
        (MotionKind::Hit, hit_info),
    ];

    drop(all_ov);
    let actor_id = manifest["id"].as_str().unwrap_or(&o.id).to_string();
    let heights = manifest["actor"]["heightMeters"].as_f64().unwrap_or(1.0) as f32;
    let idle_clip = Some(idle_c);
    drop(prim_refs);
    let mut actor = CharacterActor {
        id: actor_id, revision: manifest["revision"].as_str().unwrap_or("").to_string(), name: manifest["name"].as_str().unwrap_or("").to_string(),
        quality: plan.model.clone(), model_sha256: plan.model_sha256.clone(), height_meters: heights,
        glb: g.clone(), skel, vertices, skin, indices, deltas, prims, materials, palette, morph_weights, morph_nodes, hurt, hurt_joint,
        parent_world: Mat4::IDENTITY, body_pos: Vec3::ZERO, wrapper_pos: Vec3::ZERO, norm_m, gait: o.gait,
        mixer, idle, walk, run, root_motion, spring, a_blend: 0.0, a_run_blend: 0.0, a_time: 0.0, a_body_time: 0.0,
        has_profile: profile.is_some(), faces, face_profile, face_control: None, face_values: Vec::new(), neutral_mouth_close,
        overlays, swing, ready, jump, dodge, banzai, greet, hit_ov, rest, skill, base, still, level_obj, levelled: Levelled::default(), feet, foot_y, combat_has, motions,
        idle_clip, socket, weapon_scale, grip_poses: None, grip: None,
        rate: 1.0, clock: 0.0, face_time: 0.0, speed: 0.0, grounded: true, was_grounded: true, armed: 0.0, ready_w: 0.0, air_w: 0.0, hit_at: None, hit_seconds: None, current: None, skill_swing_at: None, events: Vec::new(), runtime_started: false,
        npc: false, photo: None, speech: None, runtime_flags: None, spring_reset: false, spring_photo: None, was_speaking: false, body_tick: 0, face_tick: 0,
    };
    actor.finish_frame();
    Ok(actor)
}

impl CharacterActor {
    /// Capture an immediately assembled, unarmed actor before any update or
    /// caller mutation of its public pose fields. A runtime template cannot be
    /// uploaded as a model; its instances must share the source actor's GPU data.
    pub fn runtime_template(&self) -> Result<CharacterRuntimeTemplate, String> {
        if self.runtime_started || self.current.is_some() || !self.events.is_empty()
            || self.grip.is_some() || self.grip_poses.is_some() || self.hit_at.is_some()
            || self.face_control.is_some() || self.parent_world != Mat4::IDENTITY
            || self.speed != 0.0 || self.armed != 0.0 || !self.grounded || self.rate != 1.0 {
            return Err("runtime template は assemble 直後、武器装着・位相設定前に作成してください".into());
        }
        // Do not clone the source Arc: release_shapes must still uniquely own
        // the full GLB after GPU upload. Runtime update never reads this field.
        let runtime_glb = Arc::new(Glb { json: Value::Null, nodes: Vec::new(), meshes: Vec::new(),
            skins: Vec::new(), animations: Vec::new(), materials: Vec::new(), images: Vec::new(), scene: Vec::new() });
        Ok(CharacterRuntimeTemplate { initial: self.copy_runtime(runtime_glb) })
    }

    // Explicit runtime-only copy. Never clone CharacterActor and then clear its
    // large arrays: geometry and image payloads must not be copied even briefly.
    fn copy_runtime(&self, runtime_glb: Arc<Glb>) -> CharacterActor {
        CharacterActor {
            id: self.id.clone(), revision: self.revision.clone(), name: self.name.clone(), quality: self.quality.clone(),
            model_sha256: self.model_sha256.clone(), height_meters: self.height_meters, glb: runtime_glb,
            skel: self.skel.clone(), vertices: Vec::new(), skin: Vec::new(), indices: Vec::new(), deltas: Vec::new(),
            prims: self.prims.clone(), materials: Vec::new(), palette: self.palette.clone(),
            morph_weights: self.morph_weights.clone(), morph_nodes: self.morph_nodes.clone(),
            hurt: self.hurt.as_ref().map(|h| HurtFace { head: h.head, head_inverse: h.head_inverse,
                vertices: Vec::new(), indices: Vec::new(), tex: (h.tex.0, h.tex.1, Vec::new()),
                hidden: h.hidden.clone(), opacity: h.opacity, visible: h.visible }), hurt_joint: self.hurt_joint,
            parent_world: self.parent_world, body_pos: self.body_pos, wrapper_pos: self.wrapper_pos, norm_m: self.norm_m, gait: self.gait,
            mixer: self.mixer.clone(), idle: self.idle, walk: self.walk, run: self.run, root_motion: self.root_motion.clone(),
            spring: self.spring.clone(), a_blend: self.a_blend, a_run_blend: self.a_run_blend, a_time: self.a_time, a_body_time: self.a_body_time,
            faces: self.faces.clone(), face_profile: self.face_profile.clone(), face_control: self.face_control,
            face_values: self.face_values.clone(), has_profile: self.has_profile, neutral_mouth_close: self.neutral_mouth_close,
            overlays: self.overlays.clone(), swing: self.swing, ready: self.ready, jump: self.jump.clone(), dodge: self.dodge.clone(),
            banzai: self.banzai, greet: self.greet, hit_ov: self.hit_ov, rest: self.rest, skill: self.skill, base: self.base.clone(),
            still: self.still.clone(), level_obj: self.level_obj, levelled: self.levelled.clone(), feet: self.feet.clone(),
            foot_y: self.foot_y, combat_has: self.combat_has, motions: self.motions.clone(), idle_clip: self.idle_clip.clone(),
            socket: self.socket.clone(), weapon_scale: self.weapon_scale, grip_poses: None, grip: None,
            rate: self.rate, clock: self.clock, face_time: self.face_time, speed: self.speed, grounded: self.grounded,
            was_grounded: self.was_grounded, armed: self.armed, ready_w: self.ready_w, air_w: self.air_w,
            hit_at: None, hit_seconds: None, current: None, skill_swing_at: None, events: Vec::new(), runtime_started: false,
            npc: self.npc, photo: self.photo, speech: self.speech, runtime_flags: self.runtime_flags, spring_reset: self.spring_reset,
            spring_photo: self.spring_photo, was_speaking: self.was_speaking, body_tick: self.body_tick, face_tick: self.face_tick,
        }
    }

    /// Compare cached/legacy idle, walk and run key-interval searches in one build.
    /// Disabled by default. This does not reset clocks or touch attack/other overlays.
    pub fn set_locomotion_interval_cache(&mut self, enabled: bool) {
        self.mixer.set_interval_cache(self.idle, enabled);
        self.mixer.set_interval_cache(self.walk, enabled);
        if let Some(run) = self.run { self.mixer.set_interval_cache(run, enabled); }
    }

    /// いま再生中の動作（無ければ None）
    pub fn current(&self) -> Option<ActionKind> { self.current.as_ref().map(|c| c.kind) }

    pub fn query(&self, kind: MotionKind) -> &MotionInfo { &self.motions.iter().find(|(k, _)| *k == kind).unwrap().1 }

    /// 表情（TS の express の face。None で既定の顔。台帳に表情の無い主役には効かない。被弾の間は ＞＜ が上）
    pub fn express(&mut self, face: Option<FaceControl>) { self.face_control = face; }

    /// 話す口（TS の express の speech。None で黙る）。口の時計は update の now_seconds。台帳の表情のある主役は 5 つの母音、無い主役・NPC・汎用は 3 つの口
    pub fn set_speech(&mut self, speech: Option<CharacterSpeech>) { self.speech = speech; }
    pub fn speech(&self) -> Option<CharacterSpeech> { self.speech }

    /// 写真の固定の姿（TS の move の photo・PackageActor の photoPose。None で戻す）。揺れ物は替わったコマに付け直す
    pub fn set_photo(&mut self, photo: Option<PhotoPose>) { self.photo = photo; }

    /// 仲間・ほかの人・NPC として描く（TS の LoadOptions の npc・CharacterActor の isNpc）。体は 30 回/秒・顔は 60 回/秒に間引く（写真の間は間引かない）
    pub fn set_npc(&mut self, npc: bool) { self.npc = npc; }

    /// 揺れ物を次のコマで付け直す（PackageActor の springReset の番号を増やすのと同じ。瞬間移動のあと）
    pub fn reset_spring(&mut self) { self.spring_reset = true; }

    /// 歩く・走るを、速さでなくそのまま渡す（PackageActor・NPC の CharacterActor の moving・running と同じ。走っているかは動いていなくても効く）。
    /// 地面にいて、構えなし・足の運びの速さ 1。move_ を呼ぶと速さの決め方に戻る
    pub fn move_runtime(&mut self, moving: bool, running: bool) {
        self.runtime_flags = Some((moving, running));
        self.speed = 0.0;
        self.grounded = true;
        self.rate = 1.0;
        self.armed = 0.0;
    }

    /// ゲームが場に置く入れ物の世界の行列（update の前に渡す。持つ物を世界で水平に保つのに使う）
    pub fn set_parent_world(&mut self, m: Mat4) { self.parent_world = m; }
    pub fn parent_world(&self) -> Mat4 { self.parent_world }

    /// 毎コマ。速さはメートル毎秒（部品の空間＝共通の倍率を掛ける前）。armed は構えの強さ 0〜1。rate は足の運びの再生の速さ（既定 1。速さと同じく倍精度）
    pub fn move_(&mut self, speed: f64, grounded: bool, armed: f32, rate: Option<f64>) {
        self.runtime_flags = None;
        self.speed = if speed.is_finite() { speed.max(0.0) } else { 0.0 };
        self.grounded = grounded;
        self.rate = match rate { Some(r) if r.is_finite() && r > 0.0 => r, _ => 1.0 };
        self.armed = armed.clamp(0.0, 1.0);
    }

    fn finish(&mut self, r: &Current, end: bool) {
        self.events.push(if end { ActorEvent::End(r.kind) } else { ActorEvent::Interrupt(r.kind) });
    }

    /// 動作を始める。再生中の動作があれば中断の合図を出してから始める（割り込めるかはゲームが決めて呼ぶ）。
    /// seconds は攻撃・回避・必殺技の長さ（省くと部品の既定）。air_seconds はジャンプで宙にいる長さ（省くとクリップの宙の区間）
    pub fn start(&mut self, kind: ActionKind, o: StartOptions) -> StartResult {
        let m = self.query(kind.into()).clone();
        if !m.available { return StartResult::Unavailable; }
        if let Some(c) = self.current.take() { self.finish(&c, false); }
        let secs = match o.seconds { Some(s) if s > 0.0 => Some(s), _ => m.seconds_exact.or(m.seconds.map(|s| s as f64)) };
        let since = match o.since { Some(s) if s > 0.0 && s.is_finite() => s, _ => 0.0 };
        let fallback = m.source == Some(MotionSource::Fallback);
        let mut cur = Current { kind, at: self.clock - since, seconds: secs.unwrap_or(0.0), cued: 0, land_at: None, off_at: None, left_ground: false, air: 0.0, play: None };
        if (kind == ActionKind::Attack || kind == ActionKind::Skill) && (o.from.is_some() || o.to.is_some() || o.ease_swing || o.cycle.is_some()) {
            cur.play = Some((o.from, o.to, o.ease_swing, o.cycle));
        }
        self.skill_swing_at = if kind == ActionKind::Skill && cur.play.is_none() && self.skill.is_some_and(|s| s.1 != SkillMode::Fallback) { Some(cur.at) } else { None };
        if kind == ActionKind::Jump {
            let j = &self.jump.as_ref().unwrap().1;
            cur.air = match o.air_seconds { Some(a) if a > 0.0 => a, _ => (j.landing - j.takeoff).max(0.1) };
        }
        self.events.push(ActorEvent::Start(kind));
        // ジャンプはしゃがみを飛ばして、始めたらすぐ跳ぶ
        if kind == ActionKind::Jump {
            cur.cued |= cue_bit(Cue::Takeoff);
            self.events.push(ActorEvent::Cue(kind, Cue::Takeoff));
        }
        self.current = Some(cur);
        StartResult::Ok { seconds: if kind == ActionKind::Rest { None } else { secs.map(|s| s as f32) }, fallback }
    }

    /// 動作を止める。休むは立ち上がってから終わる。ほかはその場で中断
    pub fn stop(&mut self, kind: ActionKind) {
        if self.current.as_ref().map(|c| c.kind) != Some(kind) { return; }
        if kind == ActionKind::Rest {
            let clock = self.clock;
            let c = self.current.as_mut().unwrap();
            if c.off_at.is_none() { c.off_at = Some(clock); }
            return;
        }
        if kind == ActionKind::Skill { self.skill_swing_at = None; }
        let c = self.current.take().unwrap();
        self.finish(&c, false);
    }

    /// ダメージが成立した。のけぞりと ＞＜ を出す（再生中の動作は止めない。止めるならゲームが stop を呼ぶ）。seconds は立ち直るまでの長さ
    pub fn hit(&mut self, seconds: Option<f64>, since: Option<f64>) {
        self.hit_at = Some(self.clock - since.filter(|s| *s > 0.0).unwrap_or(0.0));
        self.hit_seconds = seconds.filter(|s| *s > 0.0);
    }

    /// 武器を手に付ける（None で外す）。length は部品の空間での長さ（メートル）。持てれば true。持ち手の世界の行列は grip_world
    pub fn hold_weapon(&mut self, length: Option<f32>) -> bool {
        self.grip = None;
        self.grip_poses = None;
        let Some(length) = length else { return false };
        let chain = self.object_chain();
        self.grip_poses = weapon_hold(&self.skel, chain, self.idle_clip.as_deref(), self.socket.as_ref(), self.weapon_scale, length);
        match self.grip_poses { Some(p) => { self.grip = Some(p.rest); true } None => false }
    }

    /// 持ち手の世界の行列（three の weaponGrip.matrixWorld）
    pub fn grip_world(&self) -> Option<Mat4> {
        let (g, h) = (self.grip?, self.grip_poses?);
        Some(self.skel.world[h.bone] * g.matrix())
    }
    pub fn grip_pose(&self) -> Option<GripPose> { self.grip }

    /// object の空間での根の行列（body × wrapper × normalized × scene）
    fn object_chain(&self) -> Mat4 { Mat4::from_translation(self.body_pos + self.wrapper_pos) * self.norm_m }
    fn world_root(&self) -> Mat4 { self.parent_world * self.object_chain() }

    /// character-runtime の update（待機・歩き・走りと顔と揺れ物。instance.js の update と同じ順）。now は話す口の時計
    fn runtime_update(&mut self, dt: f64, moving: bool, running: bool, now: f64) {
        let photo = self.photo;
        if photo.is_none() { self.a_time += dt.min(0.05); }
        if self.npc && photo.is_none() {
            // NPC: 体 30 回/秒・顔 60 回/秒（画面の書き換えの速さによらない。sampleAnimationTime）
            if let Some(t) = sample_tick(self.body_tick, self.a_time, 30.0) { self.body_tick = t; self.runtime_body(t as f64 / 30.0, moving, running); }
            if let Some(t) = sample_tick(self.face_tick, self.a_time, 60.0) { self.face_tick = t; self.runtime_face(t as f64 / 60.0, now); }
        } else {
            let time = self.a_time;
            self.runtime_body(time, moving, running);
            self.runtime_face(time, now);
        }
        // 揺れ物（入れ物の動き → 体 → 顔のあと。重ねる動きの前。body の位置はこの時点で 0）。写真の姿が替わった・付け直しの番号が変わったら付け直す
        if let Some(sp) = &mut self.spring {
            let reset = std::mem::take(&mut self.spring_reset) || photo != self.spring_photo;
            self.spring_photo = photo;
            let chain = Mat4::from_translation(self.wrapper_pos) * self.norm_m;
            sp.advance_frame(dt.min(0.05), &mut self.skel, self.parent_world, chain, photo.is_some(), reset);
        }
    }

    /// updateBody（待機・歩き・走りを混ぜる。写真の間は決まった時刻の姿）
    fn runtime_body(&mut self, time: f64, moving: bool, running: bool) {
        let step = time - self.a_body_time;
        self.a_body_time = time;
        let photo = self.photo;
        if let Some(p) = photo {
            self.a_blend = if p == PhotoPose::Walk { 1.0 } else { 0.0 };
            self.a_run_blend = 0.0;
        } else {
            // クリップの時刻は進め続ける（短い入力で歩きの姿が始めに戻らない）
            let k = (1.0 - (-12.0 * step).exp()) as f32;
            let damp = |x: f32, y: f32| x + (y - x) * k;
            self.a_blend = damp(self.a_blend, if moving { 1.0 } else { 0.0 });
            self.a_run_blend = damp(self.a_run_blend, if running { 1.0 } else { 0.0 });
        }
        self.mixer.set_effective_weight(self.idle, 1.0 - self.a_blend);
        self.mixer.set_effective_weight(self.walk, self.a_blend * if self.run.is_some() { 1.0 - self.a_run_blend } else { 1.0 });
        if let Some(r) = self.run { self.mixer.set_effective_weight(r, self.a_blend * self.a_run_blend); }
        // 走りの無いモデルは歩きを速く
        self.mixer.actions[self.walk].time_scale = if self.run.is_some() { 1.0 } else { 1.0 + self.a_run_blend * 0.8 };
        if photo.is_some() {
            self.mixer.actions[self.idle].time = 0.0;
            self.mixer.actions[self.walk].time = self.mixer.actions[self.walk].clip.seconds * 0.15;
            self.mixer.advance(0.0, &mut self.skel);
        } else {
            self.mixer.advance(step, &mut self.skel);
        }
        if let Some(rm) = &self.root_motion { self.wrapper_pos = rm.apply(&self.skel, &self.mixer); }
    }

    /// updateFace（台帳の表情か、前からの 3 つの口とまばたき）
    fn runtime_face(&mut self, time: f64, now: f64) {
        let photo = self.photo.is_some();
        let blink = if photo { 0.0 } else { mo::automatic_blink(time as f32) };
        if self.has_profile {
            let vowels = if photo { None } else { mouth::sample_five_mouth_speech(self.speech.as_ref(), now).map(|m| m.vowels()) };
            self.face_profile.compose(self.face_control.as_ref(), blink, vowels, &mut self.face_values);
            for f in &self.faces {
                let w = &mut self.skel.morphs[f.node];
                for k in &f.keys {
                    if !k.sems_idx.is_empty() { w[k.index] = k.sems_idx.iter().fold(f32::MIN, |a, i| a.max(self.face_values[*i])); }
                    // Match the released character-runtime 0.2.7 writer. Its profile
                    // path owns semantic morphs only; unpublished src/dist also writes
                    // in-between blink corrections and is not the game baseline.
                }
            }
            return;
        }
        let neutral = self.neutral_mouth_close;
        let speech = if photo { None } else { mouth::sample_character_speech(self.speech.as_ref(), now) };
        for f in &self.faces {
            let w = &mut self.skel.morphs[f.node];
            for k in &f.keys {
                if let Some(c) = k.corrective {
                    w[k.index] = mo::blink_corrective(c, blink);
                } else if k.lr.is_some() {
                    w[k.index] = blink;
                } else if k.mouth_close && let Some(n) = neutral {
                    w[k.index] = n;
                }
            }
        }
        // applySpeechMouth: 話している間は 3 つの口を書き、話し終えたコマに読んだときの値へ戻す（目・表情・知らないモーフは触らない）
        if speech.is_some() || self.was_speaking {
            for f in &self.faces {
                let w = &mut self.skel.morphs[f.node];
                for k in &f.keys {
                    let Some(name) = k.speech else { continue };
                    w[k.index] = match speech {
                        Some(m) => m.get(name).unwrap_or(0.0) as f32,
                        None if name == "mouthClose" && neutral.is_some() => neutral.unwrap(),
                        None => f.neutral.get(k.index).copied().unwrap_or(0.0),
                    };
                }
            }
        }
        self.was_speaking = speech.is_some();
    }

    /// 1 コマ進める。この間に起きた合図を返す。now_seconds は話す口（set_speech）を測る時計（TS の nowSeconds。話させないなら何でもよい）
    pub fn update(&mut self, dt: f64, now_seconds: f64) -> Vec<ActorEvent> {
        self.runtime_started = true;
        let step = if dt.is_finite() { dt.min(0.1).max(0.0) } else { 0.0 };
        self.clock += step;
        self.face_time += step.min(0.05);
        let clock = self.clock;
        self.base.restore(&mut self.skel);
        self.body_pos = Vec3::ZERO;
        let (moving, running) = self.runtime_flags.unwrap_or_else(|| self.gait.flags(self.grounded, self.speed));
        // 足の運びの速さ: mixer は待機・歩き・走りだけを持つ（重ねる動きは部品の時計）
        self.mixer.time_scale = if moving { self.rate } else { 1.0 };
        self.runtime_update(step, moving, running, now_seconds);
        self.base.capture(&self.skel);

        let photo = self.photo.is_some();
        let mut r = self.current.take();
        // 写真の間は、休む（座ったまま撮れる）のほかの動作を描かない（動作の時計は進む）
        let shown = !(photo && r.as_ref().is_some_and(|c| c.kind != ActionKind::Rest));
        // 構え: 戦闘中に止まっている間（振りと必殺技の間も。その重みの分だけ隠れ、振りのあとは立ち姿を挟まず構えに戻る）
        let rk = if shown { r.as_ref().map(|c| c.kind) } else { None };
        let want = if !moving && self.grounded && (rk.is_none() || rk == Some(ActionKind::Attack) || rk == Some(ActionKind::Skill)) { self.armed } else { 0.0 };
        if photo { self.ready_w = 0.0; } else { self.ready_w += (want - self.ready_w) * (step as f32 * 8.0).min(1.0); }
        if let Some(i) = self.ready { let d = self.overlays[i].seconds; self.overlays[i].apply((clock % d) as f32, self.ready_w, &mut self.skel); }
        // 武器の振りの重みと位置（戦う顔と武器の持ち方に使う）
        let (mut swing_w, mut swing_phase) = (0f32, 0f32);
        // 宙（ジャンプを始めずに地面から離れた）: ジャンプの頂点の姿。高さはゲームのもの
        let air_want = if !self.grounded && rk != Some(ActionKind::Jump) && self.jump.is_some() { 1.0 } else { 0.0 };
        self.air_w += (air_want - self.air_w) * (step as f32 * 12.0).min(1.0);
        if let Some((i, j)) = &self.jump && self.air_w > 0.001 {
            self.overlays[*i].apply(j.apex, self.air_w, &mut self.skel);
            self.body_pos.y -= j.rise_at(j.apex).max(0.0) * self.air_w;
        }
        let (mut rest_w, mut skill_w) = (0f32, 0f32);
        // 必殺技の始めの間は武器の振りもクリップの長さだけ混ぜる（振りと必殺技を同時に始め、必殺技の重みの分だけ振りが隠れる。必殺技が先に終われば振りだけ続く）
        if let (Some(at), Some(sw), false) = (self.skill_swing_at, self.swing, photo) {
            let (e, len) = ((clock - at) as f32, self.overlays[sw].duration);
            let (e64, len64) = (clock - at, self.overlays[sw].seconds);
            if e64 < len64 && r.as_ref().is_none_or(|c| c.kind == ActionKind::Skill) {
                let w = mo::attack_weight64(e64, len64, mo::ATTACK_FADE_64) as f32;
                self.overlays[sw].apply((e / len).min(0.999) * len, w, &mut self.skel);
                swing_w = w;
                swing_phase = (e / len).min(1.0);
            } else {
                self.skill_swing_at = None;
            }
        }
        let mut ended = false;
        let fallback_skill = self.skill.is_some_and(|s| s.1 == SkillMode::Fallback);
        if let Some(c) = r.as_mut().filter(|_| shown) {
            let e64 = clock - c.at;
            let e = e64 as f32;
            let secs = c.seconds as f32;
            let cue = |c: &mut Current, q: Cue, events: &mut Vec<ActorEvent>| { if c.cued & cue_bit(q) == 0 { c.cued |= cue_bit(q); events.push(ActorEvent::Cue(c.kind, q)); } };
            if let Some((from, to, ease, cycle)) = c.play {
                // 技の表どおり: クリップの from〜to を、ease の形で、seconds（cycle があれば cycle ごとに繰り返す）で流す
                let ov = if c.kind == ActionKind::Attack { self.swing.unwrap() } else { self.skill.unwrap().0 };
                let (f0, t0) = (from.unwrap_or(0.0) as f32, to.unwrap_or(1.0) as f32);
                let k = match cycle { Some(cy) if cy > 0.0 => ((e64 % cy) / cy) as f32, _ => (e64 / c.seconds).min(1.0) as f32 };
                let f = f0 + (t0 - f0) * if ease { mo::ease_swing(k) } else { k };
                let w = mo::attack_weight64(e64, c.seconds, mo::ATTACK_FADE_64) as f32;
                let d = self.overlays[ov].duration;
                self.overlays[ov].apply(f * d, w, &mut self.skel);
                if c.kind == ActionKind::Skill { skill_w = w; }
                if Some(ov) == self.swing { swing_w = w; swing_phase = k; }
                if (c.kind == ActionKind::Attack || fallback_skill) && f >= 0.4 { cue(c, Cue::Strike, &mut self.events); }
                if e64 >= c.seconds { ended = true; }
            } else if c.kind == ActionKind::Attack || (c.kind == ActionKind::Skill && fallback_skill) {
                let ov = if c.kind == ActionKind::Attack { self.swing.unwrap() } else { self.skill.unwrap().0 };
                let w = mo::attack_weight64(e64, c.seconds, mo::ATTACK_FADE_64) as f32;
                let d = self.overlays[ov].duration;
                self.overlays[ov].apply((e / secs).min(1.0) * d, w, &mut self.skel);
                if c.kind == ActionKind::Skill { skill_w = w; }
                swing_w = w;
                swing_phase = (e / secs).min(1.0);
                if e64 >= 0.4 * c.seconds { cue(c, Cue::Strike, &mut self.events); }
                if e64 >= c.seconds { ended = true; }
            } else if c.kind == ActionKind::Skill {
                let (ov, mode) = self.skill.unwrap();
                let d = self.overlays[ov].duration;
                // 本人用はそのままの速さで 1 回。当て直したクリップは SKILL_SECONDS の長さに縮めて 1 回
                let fade = 0.08f32.min(secs / 4.0);
                let st = if mode == SkillMode::File { mo::skill_clip_state((e64 * mo::SKILL_SECONDS as f64 / c.seconds) as f32, d) }
                    else if e64 >= 0.0 && e64 < c.seconds { Some((e * d / secs, 1f32.min(e / fade).min((secs - e) / fade).max(0.0))) } else { None };
                if let Some((t, w)) = st { self.overlays[ov].apply(t, w, &mut self.skel); skill_w = w; }
                if e64 >= c.seconds { ended = true; }
            } else if c.kind == ActionKind::Dodge {
                // 回避: 世界はゲームが動かすので、クリップの中の横の移動を打ち消す
                let (ov, m) = self.dodge.as_ref().unwrap();
                let k = (e / secs).min(1.0);
                let d = self.overlays[*ov].duration;
                self.overlays[*ov].apply(k * d, 1.0, &mut self.skel);
                self.body_pos.x -= m.travel.x * k;
                self.body_pos.z -= m.travel.z * k;
                if e64 >= c.seconds { ended = true; }
            } else if c.kind == ActionKind::Banzai || c.kind == ActionKind::Greet {
                // 身振り: 始めと終わりだけ薄めて重ねる。体の移動は無い
                let ov = if c.kind == ActionKind::Banzai { self.banzai.unwrap() } else { self.greet.unwrap() };
                let d = self.overlays[ov].duration;
                self.overlays[ov].apply((e / secs).min(1.0) * d, mo::attack_weight64(e64, c.seconds, 0.12) as f32, &mut self.skel);
                if e64 >= c.seconds { ended = true; }
            } else if c.kind == ActionKind::Jump {
                let (ov, j) = self.jump.as_ref().unwrap();
                if !self.grounded { c.left_ground = true; }
                // 着地: 地面へ戻ったとき（ゲームが宙を動かさないときは、宙の長さが過ぎたら）
                if c.land_at.is_none() && self.grounded && ((c.left_ground && !self.was_grounded) || e64 >= c.air) {
                    c.land_at = Some(clock);
                    cue(c, Cue::Land, &mut self.events);
                }
                let d = self.overlays[*ov].seconds;
                let (t, w) = match c.land_at {
                    None => (j.takeoff + (e64 / c.air).min(1.0) * (j.landing - j.takeoff), (e64 / 0.06).min(1.0)),
                    Some(l) => { let t = j.landing + (clock - l); (t, ((d - t) / 0.1).clamp(0.0, 1.0)) }
                };
                self.overlays[*ov].apply(t as f32, w as f32, &mut self.skel);
                // 体全体の上がりは打ち消す（高さはゲーム）。しゃがみ・着地の沈みは残す
                self.body_pos.y -= j.rise_at(t as f32).max(0.0) * w as f32;
                if c.land_at.is_some() && t >= d { ended = true; }
            } else if c.kind == ActionKind::Rest && let Some((rd, rl, ru, src)) = self.rest {
                let (dd, dl, du) = (self.overlays[rd].seconds, self.overlays[rl].seconds, self.overlays[ru].seconds);
                let st = mo::rest_clip_state(clock * 1000.0, c.at * 1000.0, c.off_at.map(|o| o * 1000.0), (dd, dl, du));
                rest_w = st.weight;
                self.overlays[rd].apply(st.down_t, st.weight * st.down, &mut self.skel);
                self.overlays[rl].apply(st.loop_t, st.weight * st.r#loop, &mut self.skel);
                self.overlays[ru].apply(st.up_t, st.weight * st.up, &mut self.skel);
                if e64 >= dd { cue(c, Cue::Seated, &mut self.events); }
                // 共通の休止はクリップが腰の高さを持たないので、足が地面から離れないように体を下げる
                if src == MotionSource::Shared && !self.feet.is_empty() && st.weight > 0.001 {
                    let chain = Mat4::from_translation(self.body_pos + self.wrapper_pos) * self.norm_m;
                    self.skel.update_world(chain);
                    let foot = self.feet.iter().map(|&f| self.skel.world[f].w_axis.y).fold(f32::INFINITY, f32::min);
                    self.body_pos.y += (self.foot_y - foot) * st.weight;
                }
                if c.off_at.is_some() && st.weight == 0.0 && clock > c.off_at.unwrap() { ended = true; }
            }
        }
        if ended {
            let c = r.take().unwrap();
            self.finish(&c, true);
        }
        self.current = r;
        self.was_grounded = self.grounded;

        // のけぞり（いちばん上に重ねる）と ＞＜
        // 重みは倍精度で（0.5 の境目を TS と同じ側へ。戦う顔と ＞＜ の切り替わりのコマがそろう）
        let (mut hw, mut hw64) = (0f32, 0f64);
        if let (Some(at), false) = (self.hit_at, photo) {
            let own = self.hit_ov.is_some_and(|h| h.1 == MotionSource::Own);
            let base_s = if own { self.overlays[self.hit_ov.unwrap().0].seconds } else { mo::HIT_SECONDS_64 };
            let e = (clock - at) * self.hit_seconds.map(|s| base_s / s).unwrap_or(1.0);
            if e >= 0.0 && e < base_s {
                let t = if own { e as f32 } else { mo::HIT_FROM + (mo::HIT_TO - mo::HIT_FROM) * e as f32 / mo::HIT_SECONDS };
                hw64 = mo::attack_weight64(e, base_s, mo::HIT_FADE_64);
                hw = hw64 as f32;
                if let Some((i, _)) = self.hit_ov { self.overlays[i].apply(t, hw, &mut self.skel); }
            } else if e > 0.0 { self.hit_at = None; }
        }
        if let (Some(obj), Some(level)) = (self.level_obj, self.still.as_ref().and_then(|s| s.level.as_ref())) {
            let amount = mo::level_weight(self.still.as_ref(), hw, rest_w, skill_w);
            let up = level.local_up;
            let root = self.world_root();
            self.skel.update_world(root);
            mo::level_bone(&mut self.skel, obj, up, amount, &mut self.levelled, root);
        }
        if let Some(h) = &mut self.hurt {
            let hide = h.set(hw);
            for &i in &h.hidden { self.prims[i].visible = !hide; }
        }
        // 武器の持ち方: 振り・構えの間は配布の持つ位置、ほかは手の外へ立てる
        if let Some(p) = self.grip_poses {
            self.grip = Some(match p.swing { Some(s) if swing_w > 0.15 || self.ready_w > 0.15 => s, _ => p.rest });
        }
        // 戦う顔: 構え・振りの間は口を開ける顔（＞＜ が強い間は ＞＜）
        let combat = if !photo && hw64 < 0.5 && self.swing.is_some() && (swing_w > 0.0 || self.ready_w > 0.001) {
            mo::mix_combat_faces(
                if self.ready.is_some() && self.ready_w > 0.001 && swing_w < 1.0 { Some(mo::combat_face(false, 0.0, &self.combat_has, self.ready_w * (1.0 - swing_w))) } else { None },
                if swing_w > 0.0 { Some(mo::combat_face(true, swing_phase, &self.combat_has, swing_w)) } else { None },
            )
        } else { None };
        if let Some(values) = combat {
            let blink = mo::automatic_blink(self.face_time as f32).max(hw);
            for f in &self.faces { apply_combat(&f.keys, &mut self.skel.morphs[f.node], &values, blink); }
        } else {
            if hw > 0.0 {
                let blink = mo::automatic_blink(self.face_time as f32).max(hw);
                for f in &self.faces {
                    let w = &mut self.skel.morphs[f.node];
                    for k in &f.keys {
                        if k.blink_name { w[k.index] = w[k.index].max(hw); }
                        else if !self.has_profile && let Some(c) = k.corrective { w[k.index] = mo::blink_corrective(c, blink); }
                    }
                }
            }
            // ＞＜ の口（のけぞりの間は毎コマ書く。のけぞりが終われば 0）。ただし、のけぞりの無いコマは、顔（runtime_face）がこのコマに書いた口を消さない
            // （話す口・台帳の表情の口・写真の顔の口。消すと主役の話す口の aa がいつも 0 になる。2026-11-13）
            let owned = |k: &FaceKey| if self.has_profile { !k.sems_idx.is_empty() } else { (k.speech.is_some() && self.was_speaking) || (k.mouth_close && self.neutral_mouth_close.is_some()) };
            for f in &self.faces {
                let w = &mut self.skel.morphs[f.node];
                for k in &f.keys { if let Some(v) = k.mouth && (hw > 0.0 || !owned(k)) { w[k.index] = v * hw; } }
            }
        }
        self.finish_frame();
        std::mem::take(&mut self.events)
    }

    /// 世界の行列・骨の行列の表・モーフの重みの列を作り直す
    fn finish_frame(&mut self) {
        let root = self.world_root();
        self.skel.update_world(root);
        self.palette.update(&self.skel);
        for (node, base) in &self.morph_nodes {
            let w = &self.skel.morphs[*node];
            self.morph_weights[*base..*base + w.len()].copy_from_slice(w);
        }
    }

    /// 親の行列だけ替えて世界を作り直す（動作は進めない。hero_check など）
    pub fn refresh(&mut self) { self.finish_frame(); }
}

/// sampleAnimationTime（hz は 30・60）。新しいコマがあればその番号（時刻は 番号 / hz）
fn sample_tick(previous: i64, elapsed: f64, hz: f64) -> Option<i64> {
    let tick = ((elapsed + 1e-8) * hz).floor() as i64;
    if tick <= previous { None } else { Some(tick) }
}

/// 戦う顔をモーフへ書く。まばたきの中間と Arc も追う
fn apply_combat(keys: &[FaceKey], w: &mut [f32], values: &CombatValues, auto_blink: f32) {
    let blink_of = |s: usize| values[mo::K_BLINK_L + s].unwrap_or(0.0).max(auto_blink);
    for k in keys {
        if let (Some(c), Some(closure)) = (k.mouth_inb, values[mo::K_MOUTH_CLOSE]) {
            w[k.index] = (1.0 - (closure - c).abs() / 0.25).max(0.0);
            continue;
        }
        if let Some((sem, blink)) = k.arc {
            let base = sem.and_then(|s| values[s]).unwrap_or(0.0);
            let closure = if blink { base.max(auto_blink) } else { base };
            w[k.index] = (std::f32::consts::PI * closure).sin();
            continue;
        }
        if let Some(side) = k.side {
            if let Some(c) = k.corrective { w[k.index] = mo::blink_corrective(c, blink_of(side)); continue; }
            if k.lr == Some(side) { w[k.index] = blink_of(side); continue; }
        }
        if let Some(s) = k.sem && let Some(v) = values[s] { w[k.index] = v; }
    }
}

#[cfg(test)]
mod face_tests {
    use super::*;
    use serde_json::json;

    fn profile() -> FaceProfile {
        FaceProfile::new(&json!({
            "morphs": { "aa": "a", "ou": "u", "blinkLeft": "bl", "blinkRight": "br", "happy": "h", "sad": "s", "lookUp": "lu" },
            "overrides": { "happy": { "blink": "block", "mouth": "blend", "lookAt": "blend" }, "sad": { "blink": "blend", "mouth": "blend", "lookAt": "blend" } },
        }))
    }
    fn value(p: &FaceProfile, out: &[f32], k: &str) -> f32 { out[p.keys.iter().position(|n| n == k).unwrap()] }
    fn face(emotion: &'static str, ew: f32, mouth: &'static str, mw: f32) -> FaceControl {
        FaceControl { emotion, emotion_weight: ew, mouth, mouth_weight: mw, look: "neutral", look_weight: 0.0 }
    }

    /// character-runtime の composeCharacterFaceWeights と同じ（override の block・blend と分類）
    #[test]
    fn compose_follows_runtime() {
        let p = profile();
        let mut out = Vec::new();
        // 指定が無ければまばたきだけ
        p.compose(None, 0.4, None, &mut out);
        assert_eq!((value(&p, &out, "blinkLeft"), value(&p, &out, "aa"), value(&p, &out, "happy")), (0.4, 0.0, 0.0));
        // 押す口: 表情なしなら口はそのまま
        p.compose(Some(&face("neutral", 0.0, "ou", 1.0)), 0.0, None, &mut out);
        assert_eq!(value(&p, &out, "ou"), 1.0);
        // happy は blink を block（0 より大きければ閉じない）、mouth を blend（重みの分だけ引く）
        p.compose(Some(&face("happy", 0.5, "aa", 0.3)), 0.8, None, &mut out);
        assert_eq!(value(&p, &out, "happy"), 0.5);
        assert_eq!(value(&p, &out, "blinkLeft"), 0.0);
        assert!((value(&p, &out, "aa") - 0.15).abs() < 1e-6);
        // sad は blink も blend
        p.compose(Some(&face("sad", 1.0, "neutral", 0.0)), 0.8, None, &mut out);
        assert_eq!(value(&p, &out, "blinkRight"), 0.0);
        // 範囲の外は丸める
        p.compose(Some(&face("sad", 3.0, "neutral", 0.0)), 0.0, None, &mut out);
        assert_eq!(value(&p, &out, "sad"), 1.0);
    }
}
