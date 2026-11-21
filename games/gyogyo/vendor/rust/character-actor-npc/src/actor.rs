// NPC・汎用キャラ 1 人（凍結: @aiarranger/character-runtime-npc・-generic 0.2.5 の createCharacterInstance と update の写し）。
// 待機・歩き・走り・根の移動・まばたき・3 つの口と話す口・台帳の表情（版 1）・写真の姿・揺れ物・NPC の間引き（体 30 回/秒・顔 60 回/秒）。
// 主役の character-actor 0.1.14（2026-11-12）の actor.rs の同じ所から写し、0.2.5 の決まり（材質は map と color のまま・根の移動の骨は名前そのまま・
// 1 つのモーフに 1 つの表情）にした。主役の crate の直しをここへ持ち込まない（Owner 2026-11-12「種類ごとに分ける」）。
// 動作（攻撃・のけぞり・休む）・＞＜・武器の持ち方は持たない（TS の NPC・汎用の部品にも無い）。武器を描くなら、持ち手の世界の行列をゲームが set_grip_world で渡す。
// 取得と解析は GPU を触らない（NpcActor は Send）。描くのは character-view-npc の NpcView。
use crate::anim::{self, ActionId, Clip, Mixer, Palette, Prop, Skeleton, Track};
use crate::glb::{self, Glb, Primitive};
use crate::mouth::{self, CharacterSpeech};
use crate::spring::SpringMotion;
use crate::Vertex;
use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Quat, Vec3};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// 写真の固定の姿（character-runtime の PhotoPose。待機は 0 秒・歩きは 15%）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PhotoPose { Idle, Walk }

/// 光なしの材質（character-runtime 0.2.5 の convertCharacterMaterial の結果）
#[derive(Clone, Debug)]
pub struct BasicMaterial {
    pub map: Option<usize>,
    pub color: [f32; 3],
    pub opacity: f32,
    pub double_sided: bool,
    pub transparent: bool,
    pub depth_write: bool,
    pub alpha_test: f32,
}

struct PrimRef<'a> {
    name: String,
    node: usize,
    p: &'a Primitive,
    /// 足して 1 に直した重み（three の normalizeSkinWeights）
    weights: Vec<[f32; 4]>,
    skin: Option<usize>,
    material: usize,
}

/// 閉じて、少し保って、ゆっくり開く（blink.js の automaticBlink）
pub fn automatic_blink(elapsed: f32) -> f32 {
    let time = elapsed % 4.1 - 3.81;
    let smooth = |v: f32| v * v * (3.0 - 2.0 * v);
    if time < 0.0 { return 0.0; }
    if time < 0.085 { return smooth(time / 0.085); }
    if time < 0.115 { return 1.0; }
    if time < 0.275 { return 1.0 - smooth((time - 0.115) / 0.16); }
    0.0
}
/// blinkCorrectiveWeight（中間のまばたきの補い）
pub fn blink_corrective(center: f32, closure: f32) -> f32 { (1.0 - (closure - center).abs() / 0.25).max(0.0) }

// ---- 顔のモーフ ----

#[derive(Clone, Default)]
struct FaceKey {
    index: usize,
    /// blink(Left|Right)Inbetween の中心
    corrective: Option<f32>,
    /// 名前が blinkLeft・blinkRight そのもの
    lr: bool,
    mouth_close: bool,
    /// 話す口が書く口（mouthClose・aa・ee・ou のどれか。applySpeechMouth）
    speech: Option<&'static str>,
    /// 台帳の表情（FaceProfile.keys の番号）のうち、このモーフを指す物（0.2.5 は 1 つ）
    sem: Option<usize>,
}

#[derive(Clone)]
struct Face {
    node: usize,
    keys: Vec<FaceKey>,
    /// 読んだときのモーフの重み（collectMorphFaces の neutral）
    neutral: Vec<f32>,
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

/// 0.2.5: 骨は名前そのままで探す（0.2.6 からの読み替えは無い）
fn create_root_motion(skel: &Skeleton, setting: Option<&Value>, clips: &[(ActionId, Arc<Clip>)], linear: Mat3) -> Result<Option<RootMotion>, String> {
    let Some(setting) = setting else { return Ok(None) };
    if setting.get("mode").and_then(Value::as_str) != Some("local-linear-horizontal") { return Err("対応していない移動設定です。".into()); }
    let name = setting.get("node").and_then(Value::as_str).unwrap_or("");
    let node = [name.to_string()].iter().find_map(|n| {
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

/// three の GLTFLoader がメッシュに付ける名前（1 プリミティブなら節点の名前）
fn prim_name(g: &Glb, skel: &Skeleton, node: usize, mesh: usize, i: usize) -> String {
    let m = &g.meshes[mesh];
    if m.primitives.len() == 1 && !skel.names[node].is_empty() { return skel.names[node].clone(); }
    let base = if m.name.is_empty() { format!("mesh_{mesh}") } else { anim::sanitize_name(&m.name) };
    if i == 0 { base } else { format!("{base}_{i}") }
}

fn basic_material(g: &Glb, index: Option<usize>) -> BasicMaterial {
    // 材質が無いプリミティブは glTF の既定（白）。0.2.5 の materials.js: map と color のまま（0.2.7 の「光の色だけの GLB」の直しは無い）
    let Some(i) = index else { return BasicMaterial { map: None, color: [1.0; 3], opacity: 1.0, double_sided: false, transparent: false, depth_write: true, alpha_test: 0.0 } };
    let m = &g.materials[i];
    BasicMaterial { map: m.texture, color: [m.base_color[0], m.base_color[1], m.base_color[2]], opacity: m.base_color[3], double_sided: m.double_sided, transparent: m.alpha_blend, depth_write: !m.alpha_blend, alpha_test: m.alpha_cutoff.unwrap_or(0.0) }
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


/// NPC・汎用 1 人（three の Object3D の木の代わりに、骨の表と根の行列を持つ）。Send
pub struct NpcActor {
    pub id: String,
    pub name: String,
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
    /// 揺れ物（VRMC_springBone のある GLB だけ）
    pub spring: Option<SpringMotion>,
    /// 動作を混ぜる（待機・歩き・走り）。ゲームが自分のクリップを足すときに使う
    pub mixer: Mixer,
    idle: ActionId,
    walk: ActionId,
    run: Option<ActionId>,
    root_motion: Option<RootMotion>,
    parent_world: Mat4,
    wrapper_pos: Vec3,
    /// normalized × scene（高さの拡大・前向き・足もとの位置）
    norm_m: Mat4,
    grip: Option<Mat4>,
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
    moving: bool,
    running: bool,
    npc: bool,
    photo: Option<PhotoPose>,
    speech: Option<CharacterSpeech>,
    spring_reset: bool,
    spring_photo: Option<PhotoPose>,
    was_speaking: bool,
    body_tick: i64,
    face_tick: i64,
}

/// 読んだ manifest（CharacterAsset の manifest。呼ぶ側が身長・口の既定値を書き換えた物でよい）と GLB から組み立てる（createCharacterInstance）
pub fn assemble(pm: &Value, model: &[u8]) -> Result<NpcActor, String> {
    let g = Arc::new(glb::parse(model)?);
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
            // 骨の番号が skin の外（壊れた GLB）。three は描き崩れるだけだが、ここでは読めないとする
            if let Some(sk) = skin && p.joints.iter().any(|j| j.iter().any(|&k| k as usize >= g.skins[sk].joints.len())) { return Err("モデルの頂点データが不完全です。".into()); }
            if skin.is_some() && (p.joints.len() != p.positions.len() || p.weights.len() != p.positions.len()) { return Err("モデルの頂点データが不完全です。".into()); }
            // three の normalizeSkinWeights
            let weights: Vec<[f32; 4]> = if skin.is_some() {
                p.weights.iter().map(|w| {
                    let s = w[0].abs() + w[1].abs() + w[2].abs() + w[3].abs();
                    if s == 0.0 { [1.0, 0.0, 0.0, 0.0] } else { [w[0] / s, w[1] / s, w[2] / s, w[3] / s] }
                }).collect()
            } else { Vec::new() };
            prim_refs.push(PrimRef { name: prim_name(&g, &skel, node, mesh, pi), node, p, weights, skin, material });
        }
    }
    if prim_refs.is_empty() { return Err("モデルに表示できるメッシュがありません。".into()); }

    // ---- 大きさと向きをそろえる（normalizeCharacter） ----
    let skin_mats: Vec<Vec<Mat4>> = g.skins.iter().map(|sk| sk.joints.iter().zip(&sk.inverse_bind).map(|(j, ib)| skel.world[*j] * *ib).collect()).collect();
    let (lo, hi) = model_bounds(&g, &skel, &prim_refs, &skin_mats);
    let size = hi - lo;
    if !size.y.is_finite() || size.y <= 0.0 { return Err("モデルの大きさを読み取れません。".into()); }
    let center = (lo + hi) * 0.5;
    let height = pm["coordinates"]["heightMeters"].as_f64().unwrap_or(1.0) as f32;
    let norm_scale = height / size.y;
    let rot_y = match pm["coordinates"]["forward"].as_str() { Some("+X") => -std::f32::consts::FRAC_PI_2, Some("-X") => std::f32::consts::FRAC_PI_2, Some("-Z") => std::f32::consts::PI, _ => 0.0 };
    let norm_q = Quat::from_rotation_y(rot_y);
    let normalized = Mat4::from_scale_rotation_translation(Vec3::splat(norm_scale), norm_q, Vec3::ZERO);
    let norm_m = normalized * Mat4::from_translation(Vec3::new(-center.x, -lo.y, -center.z));
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

    // ---- 顔 ----
    let profile = pm["motion"].get("face").filter(|v| !v.is_null());
    let face_profile = profile.map(FaceProfile::new).unwrap_or_default();
    let mut faces: Vec<Face> = Vec::new();
    for (node, _) in &morph_nodes {
        let names = &g.meshes[g.nodes[*node].mesh.unwrap()].target_names;
        let mut keys = Vec::new();
        for (index, name) in names.iter().enumerate() {
            let mut k = FaceKey { index, ..Default::default() };
            k.corrective = corrective_center(name);
            k.lr = name == "blinkLeft" || name == "blinkRight";
            k.mouth_close = name == "mouthClose";
            k.speech = ["mouthClose", "aa", "ee", "ou"].into_iter().find(|n| n == name);
            // 0.2.5: モーフ → 表情は 1 つ（同じモーフを指す表情があれば後の物。版 1 の決まりでは無い）
            if let Some(ms) = profile.and_then(|p| p.get("morphs")).and_then(Value::as_object) {
                k.sem = ms.iter().filter(|(_, v)| v.as_str() == Some(name.as_str())).map(|(s, _)| s).last().and_then(|s| face_profile.keys.iter().position(|f| f == s));
            }
            keys.push(k);
        }
        faces.push(Face { node: *node, keys, neutral: skel.morphs[*node].clone() });
    }

    let mut actor = NpcActor {
        id: pm["id"].as_str().unwrap_or("").to_string(), name: pm["name"].as_str().unwrap_or("").to_string(),
        model_sha256: pm["model"]["sha256"].as_str().unwrap_or("").to_string(), height_meters: height,
        glb: g.clone(), skel, vertices, skin, indices, deltas, prims, materials, palette, morph_weights, morph_nodes, spring,
        mixer, idle, walk, run, root_motion, parent_world: Mat4::IDENTITY, wrapper_pos: Vec3::ZERO, norm_m, grip: None,
        a_blend: 0.0, a_run_blend: 0.0, a_time: 0.0, a_body_time: 0.0,
        has_profile: profile.is_some(), faces, face_profile, face_control: None, face_values: Vec::new(),
        neutral_mouth_close: pm["motion"].get("neutralMouthClose").and_then(Value::as_f64).map(|v| v as f32),
        moving: false, running: false, npc: false, photo: None, speech: None, spring_reset: false, spring_photo: None, was_speaking: false, body_tick: 0, face_tick: 0,
    };
    actor.finish_frame();
    Ok(actor)
}

/// sampleAnimationTime（hz は 30・60）。新しいコマがあればその番号（時刻は 番号 / hz）
fn sample_tick(previous: i64, elapsed: f64, hz: f64) -> Option<i64> {
    let tick = ((elapsed + 1e-8) * hz).floor() as i64;
    if tick <= previous { None } else { Some(tick) }
}

impl NpcActor {
    /// 歩く・走る（NPC の CharacterActor・PackageActor の moving・running。走っているかは動いていなくても効く）
    pub fn move_runtime(&mut self, moving: bool, running: bool) { self.moving = moving; self.running = running; }
    /// isNpc: 体は 30 回/秒・顔は 60 回/秒に間引く（写真の間は間引かない）。aisns の NPC はいつも true
    pub fn set_npc(&mut self, npc: bool) { self.npc = npc; }
    /// 話す口（None で黙る）。時計は update の now_seconds。台帳の表情があれば 5 つの母音、無ければ 3 つの口
    pub fn set_speech(&mut self, speech: Option<CharacterSpeech>) { self.speech = speech; }
    /// 表情（faceControl。台帳の表情のあるキャラだけに効く）
    pub fn express(&mut self, face: Option<FaceControl>) { self.face_control = face; }
    /// 写真の固定の姿（photoPose。None で戻す）。揺れ物は替わったコマに付け直す
    pub fn set_photo(&mut self, photo: Option<PhotoPose>) { self.photo = photo; }
    /// 揺れ物を次のコマで付け直す（springReset の番号を増やすのと同じ）
    pub fn reset_spring(&mut self) { self.spring_reset = true; }
    /// ゲームが場に置く入れ物の世界の行列（update の前に渡す。揺れ物が入れ物の動きにも反応する）
    pub fn set_parent_world(&mut self, m: Mat4) { self.parent_world = m; }
    pub fn parent_world(&self) -> Mat4 { self.parent_world }
    /// 武器を持たせて描くときの持ち手の世界の行列（持ち方はゲーム。aisns の weapon-rig）。None で外す
    pub fn set_grip_world(&mut self, m: Option<Mat4>) { self.grip = m; }
    pub fn grip_world(&self) -> Option<Mat4> { self.grip }

    /// 1 コマ進める（instance.update）。now_seconds は話す口の時計（TS の nowSeconds）
    pub fn update(&mut self, dt: f64, now_seconds: f64) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let photo = self.photo;
        if photo.is_none() { self.a_time += dt.min(0.05); }
        if self.npc && photo.is_none() {
            if let Some(t) = sample_tick(self.body_tick, self.a_time, 30.0) { self.body_tick = t; self.update_body(t as f64 / 30.0); }
            if let Some(t) = sample_tick(self.face_tick, self.a_time, 60.0) { self.face_tick = t; self.update_face(t as f64 / 60.0, now_seconds); }
        } else {
            let time = self.a_time;
            self.update_body(time);
            self.update_face(time, now_seconds);
        }
        // 揺れ物（入れ物の動き → 体 → 顔のあと）。写真の姿が替わった・付け直しの番号が変わったら付け直す
        if let Some(sp) = &mut self.spring {
            let reset = std::mem::take(&mut self.spring_reset) || photo != self.spring_photo;
            self.spring_photo = photo;
            let chain = Mat4::from_translation(self.wrapper_pos) * self.norm_m;
            sp.advance_frame(dt.min(0.05), &mut self.skel, self.parent_world, chain, photo.is_some(), reset);
        }
        self.finish_frame();
    }

    /// updateBody（待機・歩き・走りを混ぜる。写真の間は決まった時刻の姿）
    fn update_body(&mut self, time: f64) {
        let step = time - self.a_body_time;
        self.a_body_time = time;
        if let Some(p) = self.photo {
            self.a_blend = if p == PhotoPose::Walk { 1.0 } else { 0.0 };
            self.a_run_blend = 0.0;
        } else {
            // クリップの時刻は進め続ける（短い入力で歩きの姿が始めに戻らない）
            let k = (1.0 - (-12.0 * step).exp()) as f32;
            let damp = |x: f32, y: f32| x + (y - x) * k;
            self.a_blend = damp(self.a_blend, if self.moving { 1.0 } else { 0.0 });
            self.a_run_blend = damp(self.a_run_blend, if self.running { 1.0 } else { 0.0 });
        }
        self.mixer.set_effective_weight(self.idle, 1.0 - self.a_blend);
        self.mixer.set_effective_weight(self.walk, self.a_blend * if self.run.is_some() { 1.0 - self.a_run_blend } else { 1.0 });
        if let Some(r) = self.run { self.mixer.set_effective_weight(r, self.a_blend * self.a_run_blend); }
        // 走りの無いモデルは歩きを速く
        self.mixer.actions[self.walk].time_scale = if self.run.is_some() { 1.0 } else { 1.0 + self.a_run_blend * 0.8 };
        if self.photo.is_some() {
            self.mixer.actions[self.idle].time = 0.0;
            self.mixer.actions[self.walk].time = self.mixer.actions[self.walk].clip.seconds * 0.15;
            self.mixer.advance(0.0, &mut self.skel);
        } else {
            self.mixer.advance(step, &mut self.skel);
        }
        if let Some(rm) = &self.root_motion { self.wrapper_pos = rm.apply(&self.skel, &self.mixer); }
    }

    /// updateFace（台帳の表情か、前からの 3 つの口とまばたき）
    fn update_face(&mut self, time: f64, now: f64) {
        let photo = self.photo.is_some();
        let blink = if photo { 0.0 } else { automatic_blink(time as f32) };
        if self.has_profile {
            let vowels = if photo { None } else { mouth::sample_five_mouth_speech(self.speech.as_ref(), now).map(|m| m.vowels()) };
            self.face_profile.compose(self.face_control.as_ref(), blink, vowels, &mut self.face_values);
            for f in &self.faces {
                let w = &mut self.skel.morphs[f.node];
                for k in &f.keys { if let Some(s) = k.sem { w[k.index] = self.face_values[s]; } }
            }
            return;
        }
        let neutral = self.neutral_mouth_close;
        let speech = if photo { None } else { mouth::sample_character_speech(self.speech.as_ref(), now) };
        for f in &self.faces {
            let w = &mut self.skel.morphs[f.node];
            for k in &f.keys {
                if let Some(c) = k.corrective { w[k.index] = blink_corrective(c, blink); }
                else if k.lr { w[k.index] = blink; }
                else if k.mouth_close && let Some(n) = neutral { w[k.index] = n; }
            }
        }
        // applySpeechMouth: 話している間は 3 つの口を書き、話し終えたコマに読んだときの値へ戻す
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

    /// 世界の行列・骨の行列の表・モーフの重みの列を作り直す
    fn finish_frame(&mut self) {
        let root = self.parent_world * Mat4::from_translation(self.wrapper_pos) * self.norm_m;
        self.skel.update_world(root);
        self.palette.update(&self.skel);
        for (node, base) in &self.morph_nodes {
            let w = &self.skel.morphs[*node];
            self.morph_weights[*base..*base + w.len()].copy_from_slice(w);
        }
    }

    /// 骨やモーフをゲームが update のあとに書き換えたら呼ぶ（aisns の FieldNpcActor の仕草など）。動きは進めない
    pub fn refresh(&mut self) { self.finish_frame(); }
}
