// イカの体の形と動き（ブラウザに頼らない部分。ネイティブで試験できる）。Squid Kit（CC0、https://cda-social.github.io/fish/gallery#squid の kit/squid-kit.zip）の
// squid-kit.js の `// src/game/squid/squid-species.ts`（種の値）と `// src/game/squid/squid.ts`（formOf・gridGeometry・Tube・bakePhoto・Squid.update の
// buildMantle / buildFins / buildArms / buildTentacles / bendFunnel）の写し。毎コマ全頂点を作り直す（骨・動きの記録は無い）。
// 写真は、背・腹・横の 3 枚を頂点ごとに混ぜる作り（three ではシェーダーで混ぜる）を、読み込みのときに部品ごとの 1 枚の模様（atlas）へ焼く。
// 動きの状態機械（SquidBrain）は写さず、走者の SquidDrive から pose の目標を決める（hover・ひれで進む・ジェット・跳ぶ・よろけ・喜び）。
// 座標: 写しの中は three と同じ「腕の方が +Z・背が +Y・胴の前の縁が原点」。外へ出すときに「外套の先が +Z・全長 length_m・真ん中が原点」へ直す。
use glam::{DMat3, DMat4, DQuat, DVec3};
use island_kit::geometry::Geometry;
use serde_json::Value;
use std::f64::consts::PI;

// ---------------------------------------------------------------- 種の値（squid-species.ts。写真の形があるときに使う値だけ）

#[derive(Debug)]
pub struct Species {
    pub id: &'static str,
    pub ml: f64,
    dorsal_squeeze: f64,
    fin_waves: f64,
    fin_amp: f64,
    fin_freq: f64,
    arm_r: f64,
    tent_rest: f64,
    tent_r: f64,
    club_r: f64,
    arm_ratio: [f64; 4],
    arm_fix: f64,
    arm_r_frac: f64,
    height_fix: f64,
    jet: f64,
    /// 写真のひれの透け（SKIN の uFinLum・uFinMin）
    fin_lum: f64,
    fin_min: f64,
    /// 腕を白っぽく（コウイカ。uArmWhite）
    arm_white: f64,
    belly: [f64; 3],
}

pub static AORI: Species = Species {
    id: "aori", ml: 0.3, dorsal_squeeze: 1.0, fin_waves: 1.1, fin_amp: 0.55, fin_freq: 1.8, arm_r: 0.03, tent_rest: 0.45, tent_r: 0.011, club_r: 0.024,
    arm_ratio: [0.72, 0.86, 1.0, 0.94], arm_fix: 1.05, arm_r_frac: 0.17, height_fix: 1.0, jet: 1.0, fin_lum: 0.19, fin_min: 0.6, arm_white: 0.0, belly: [0.9, 0.88, 0.86],
};
pub static KOUIKA: Species = Species {
    id: "kouika", ml: 0.18, dorsal_squeeze: 0.3, fin_waves: 2.6, fin_amp: 0.45, fin_freq: 3.2, arm_r: 0.04, tent_rest: 0.0, tent_r: 0.012, club_r: 0.03,
    arm_ratio: [0.9, 0.88, 0.94, 1.0], arm_fix: 1.12, arm_r_frac: 0.2, height_fix: 0.8, jet: 0.55, fin_lum: 0.22, fin_min: 0.68, arm_white: 0.85, belly: [0.88, 0.84, 0.78],
};

pub fn species(id: &str) -> Option<&'static Species> {
    match id { "aori" => Some(&AORI), "kouika" => Some(&KOUIKA), _ => None }
}

// ---------------------------------------------------------------- 形の JSON（squid/<id>.json：写真から測った輪郭）

#[derive(Clone, Copy, Debug)]
pub struct PhotoMap { pub x0: f64, pub yc: f64, pub k: f64, pub w: f64, pub h: f64 }
#[derive(Clone, Debug)]
pub struct ArmTrack { pub y: Vec<f64>, pub hw: f64 }
#[derive(Clone, Debug)]
pub struct Shape {
    pub bg: [f64; 3],
    pub dorsal: PhotoMap,
    pub ventral: PhotoMap,
    pub side: PhotoMap,
    pub mantle_len: f64,
    pub mantle_w: Vec<f64>,
    pub mantle_top: Vec<f64>,
    pub mantle_bot: Vec<f64>,
    pub fin_s0: f64,
    pub fin_s1: f64,
    pub fin_w: Vec<f64>,
    pub head_len: f64,
    pub head_w: Vec<f64>,
    pub head_top: Vec<f64>,
    pub head_bot: Vec<f64>,
    pub arms_len: f64,
    pub arms_bundle: f64,
    /// 目（z・y・半径・横の写真の画素 [x, y, 半径]・x）
    pub eye_z: f64,
    pub eye_y: f64,
    pub eye_r: f64,
    pub eye_px: [f64; 3],
    pub eye_x: f64,
    pub track_x: Vec<f64>,
    pub tracks: Vec<ArmTrack>,
}

fn num(v: &Value, k: &str) -> Result<f64, String> { v.get(k).and_then(Value::as_f64).ok_or_else(|| format!("squid json: {k} が無い")) }
fn arr(v: &Value, k: &str) -> Result<Vec<f64>, String> {
    v.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).collect::<Vec<_>>()).filter(|a| a.len() >= 2).ok_or_else(|| format!("squid json: {k} が無い"))
}
fn obj<'a>(v: &'a Value, k: &str) -> Result<&'a Value, String> { v.get(k).filter(|o| o.is_object()).ok_or_else(|| format!("squid json: {k} が無い")) }
fn photo(v: &Value, k: &str) -> Result<PhotoMap, String> {
    let o = obj(v, k)?;
    Ok(PhotoMap { x0: num(o, "x0")?, yc: num(o, "yc")?, k: num(o, "k")?, w: num(o, "w")?, h: num(o, "h")? })
}

pub fn parse_shape(text: &str) -> Result<Shape, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("squid json: {e}"))?;
    let bg = arr(&v, "bg")?;
    let (m, fin, hd, arms, eye, at) = (obj(&v, "mantle")?, obj(&v, "fin")?, obj(&v, "head")?, obj(&v, "arms")?, obj(&v, "eye")?, obj(&v, "armTracks")?);
    let px = arr(eye, "px")?;
    let tracks = at.get("tracks").and_then(Value::as_array).ok_or("squid json: armTracks.tracks が無い")?
        .iter().map(|t| Ok(ArmTrack { y: arr(t, "y")?, hw: num(t, "hw")? })).collect::<Result<Vec<_>, String>>()?;
    Ok(Shape {
        bg: [bg[0], bg[1], *bg.get(2).unwrap_or(&bg[1])],
        dorsal: photo(&v, "dorsal")?, ventral: photo(&v, "ventral")?, side: photo(&v, "side")?,
        mantle_len: num(m, "len")?, mantle_w: arr(m, "w")?, mantle_top: arr(m, "top")?, mantle_bot: arr(m, "bot")?,
        fin_s0: num(fin, "s0")?, fin_s1: num(fin, "s1")?, fin_w: arr(fin, "w")?,
        head_len: num(hd, "len")?, head_w: arr(hd, "w")?, head_top: arr(hd, "top")?, head_bot: arr(hd, "bot")?,
        arms_len: num(arms, "len")?, arms_bundle: num(arms, "bundle")?,
        eye_z: num(eye, "z")?, eye_y: num(eye, "y")?, eye_r: num(eye, "r")?, eye_px: [px[0], px[1], *px.get(2).ok_or("squid json: eye.px")?], eye_x: num(eye, "x")?,
        track_x: arr(at, "x")?, tracks,
    })
}

// ---------------------------------------------------------------- 小さな式

fn smooth(a: f64, b: f64, x: f64) -> f64 { let t = ((x - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }
/// 0〜1 を配列の中で直線に補う（sample$1）
fn sample(a: &[f64], t: f64) -> f64 {
    let x = t.clamp(0.0, 1.0) * (a.len() - 1) as f64;
    let i = (x.floor() as usize).min(a.len() - 2);
    a[i] + (a[i + 1] - a[i]) * (x - i as f64)
}
/// Vector3.normalize（長さ 0 はそのまま）
fn norm(v: DVec3) -> DVec3 { let l = v.length(); if l > 0.0 { v / l } else { v } }
/// T を R の方へ a だけ回す（R も一緒に回す）
fn rotate(t: &mut DVec3, r: &mut DVec3, a: f64) {
    if a == 0.0 { return; }
    let (s, c) = a.sin_cos();
    let nt = *t * c + *r * s;
    *r = *r * c - *t * s;
    *t = nt;
}
fn axis_angle(v: DVec3, axis: DVec3, a: f64) -> DVec3 { DQuat::from_axis_angle(axis, a) * v }

// ---------------------------------------------------------------- 形の値（formOf の写真の形がある方）

#[derive(Clone, Debug)]
struct Form {
    mlen: f64,
    mw: Vec<f64>,
    mtop: Vec<f64>,
    mbot: Vec<f64>,
    hf: f64,
    head_len: f64,
    hdw: Vec<f64>,
    hdt: Vec<f64>,
    hdb: Vec<f64>,
    fin0: f64,
    fin1: f64,
    finw: Vec<f64>,
    arms: [f64; 4],
    arm_r: f64,
    eye: DVec3,
    eye_r: f64,
}
impl Form {
    fn new(sp: &Species, s: &Shape) -> Form {
        let l = s.arms_len * sp.arm_fix;
        let r = s.eye_r * 1.12;
        Form {
            mlen: s.mantle_len, mw: s.mantle_w.clone(), mtop: s.mantle_top.clone(), mbot: s.mantle_bot.clone(), hf: sp.height_fix,
            head_len: s.head_len, hdw: s.head_w.clone(), hdt: s.head_top.clone(), hdb: s.head_bot.clone(),
            fin0: s.fin_s0, fin1: s.fin_s1, finw: s.fin_w.clone(),
            arms: sp.arm_ratio.map(|k| k * l), arm_r: s.arms_bundle * sp.arm_r_frac,
            eye: DVec3::new(s.eye_x - r * 0.62, s.eye_y * sp.height_fix, s.eye_z), eye_r: r,
        }
    }
    fn a(&self, s: f64) -> f64 { sample(&self.mw, s) }
    fn top(&self, s: f64) -> f64 { sample(&self.mtop, s) * self.hf }
    fn bot(&self, s: f64) -> f64 { sample(&self.mbot, s) * self.hf }
    fn h_at(a: &[f64], h: f64) -> f64 { sample(a, (h + 0.15) / 1.15) }
    fn hw(&self, h: f64) -> f64 { Self::h_at(&self.hdw, h) }
    fn ht(&self, h: f64) -> f64 { Self::h_at(&self.hdt, h) * self.hf }
    fn hb(&self, h: f64) -> f64 { Self::h_at(&self.hdb, h) * self.hf }
    fn fin_w(&self, u: f64) -> f64 { sample(&self.finw, u) }
    fn arm_max(&self) -> f64 { self.arms.iter().cloned().fold(0.0, f64::max) }
}

// ---------------------------------------------------------------- 姿勢の値（NEUTRAL と、近づく速さ RATE。色の値は省く）

macro_rules! pose {
    ($($name:ident = $v:expr, $rate:expr;)*) => {
        /// 姿勢の値（squid.ts の NEUTRAL のうち形に効くもの）。target へ毎コマ近づける
        #[derive(Clone, Copy, Debug, PartialEq)]
        pub struct Pose { $(pub $name: f64,)* }
        pub const NEUTRAL: Pose = Pose { $($name: $v,)* };
        impl Pose {
            fn approach(&mut self, t: &Pose, dt: f64) {
                if dt <= 0.0 { return; }
                $( self.$name += (t.$name - self.$name) * (1.0 - (-($rate) * dt).exp()); )*
            }
            pub fn lerp(&self, o: &Pose, k: f64) -> Pose { Pose { $($name: self.$name + (o.$name - self.$name) * k,)* } }
        }
    };
}
pose! {
    mantle = 0.0, f64::INFINITY; // behavior が直接決める（direct）
    breathe = 1.0, 4.0;
    head_pitch = 0.0, 4.0;
    head_yaw = 0.0, 4.0;
    eye_yaw = 0.15, 7.0;
    eye_pitch = 0.0, 7.0;
    fin_amp = 1.5, 4.0;
    fin_freq = 0.8, 4.0;
    fin_dir = 1.0, 4.0;
    fin_fold = 0.0, 6.0;
    fin_lift = 0.0, 4.0;
    fin_turn = 0.0, 4.0;
    arm_spread = 0.12, 4.0;
    arm_curl = -0.15, 4.0;
    arm_tip = 0.25, 4.0;
    arm_wave = 0.15, 4.0;
    arm_pitch = 0.12, 4.0;
    arm_gather = 0.2, 5.0;
    arm_raise = 0.0, 4.0;
    arm_idle = 1.0, 4.0;
    arm_pinch = 0.6, 1.5;
    arm_droop = 0.0, 1.2;
    arm_bundle = 0.0, 6.0;
    arm_flare = 0.0, 8.0;
    wrap = 0.0, 3.0;
    tent_show = 1.0, 4.0;
    funnel_aim = 0.25, 4.0;
    funnel_open = 0.3, 4.0;
    sag = 0.0, 4.0;
    flat = 0.0, 2.0;
}

/// 走者からイカへ渡す値（view/squid.rs の SquidDrive）
#[derive(Clone, Copy, Debug, Default)]
pub struct SquidDrive {
    /// 前へ進む速さ m/s（走者は 0〜約 20 m/s）
    pub speed: f64,
    /// 0..1 ダッシュの強さ（ジェットで速く泳ぐ）
    pub dash: f64,
    /// 跳んでいる（ハードルを跳ぶ）
    pub air: bool,
    /// 0..1 攻撃を受けてよろけている
    pub hurt: f64,
    /// ゴール後・表彰台
    pub joy: bool,
}

// ---------------------------------------------------------------- 形の細かさ（RES。full は three 版と同じ、MID がゲームの既定）

#[derive(Clone, Copy, Debug)]
pub struct Res { pub ms: usize, pub ma: usize, pub fu: usize, pub fv: usize, pub hr: usize, pub arm_n: usize, pub tent_n: usize, pub tube_m: usize, pub eye: (usize, usize), pub fun: usize }
/// three 版の detail 'full'（試験で three 版と頂点を比べる）
pub const FULL: Res = Res { ms: 46, ma: 36, fu: 34, fv: 7, hr: 22, arm_n: 20, tent_n: 28, tube_m: 8, eye: (28, 18), fun: 14 };
/// ゲームの既定（1 体で約 2700 頂点。three の 'low' より少し細かい）
pub const MID: Res = Res { ms: 28, ma: 20, fu: 22, fv: 5, hr: 12, arm_n: 14, tent_n: 16, tube_m: 6, eye: (14, 9), fun: 10 };

// ---------------------------------------------------------------- 部品

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind { Mantle, Head, Eye(usize), Arm(usize), Tent(usize), Funnel, Fin(usize) }

/// 頂点ごとの写真の貼り方（bakePhoto の aUvT・aUvB・aUvS・aW）と、皮の上の位置（腕の吸盤・白っぽさに使う：前後 m/ml、まわり、口の側 cos）
#[derive(Clone, Copy, Debug, Default)]
struct VAttr { uv: [[f64; 2]; 3], w: [f64; 3], skin: [f64; 3] }

/// 部品の格子（rows × cols、行ごとに並ぶ）。pos・nrm は部品の座標（胴・頭・目・漏斗）
#[derive(Clone, Debug)]
struct Part { kind: Kind, rows: usize, cols: usize, idx: Vec<u32>, pos: Vec<DVec3>, nrm: Vec<DVec3> }

/// 描く 1 枚（部品の表、ひれは表と裏の 2 枚）。atlas の区画と、出す形の中の頂点の始まり
#[derive(Clone, Debug)]
struct Piece { part: usize, flip: bool, fin_mesh: bool, attr: Vec<VAttr>, region: [u32; 4], off: usize }

#[derive(Clone, Debug)]
struct Arm { k: usize, side: f64, psi: f64, len: f64, phase: f64, pts: Vec<DVec3>, part: usize }
#[derive(Clone, Debug)]
struct Tent { side: f64, psi: f64, pts: Vec<DVec3>, part: usize }

fn grid_index(rows: usize, cols: usize, flip: bool) -> Vec<u32> {
    let mut idx = Vec::with_capacity((rows - 1) * (cols - 1) * 6);
    for i in 0..rows - 1 {
        for j in 0..cols - 1 {
            let a = (i * cols + j) as u32;
            let b = a + cols as u32;
            if flip { idx.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]); } else { idx.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]); }
        }
    }
    idx
}
fn part(kind: Kind, rows: usize, cols: usize, idx: Vec<u32>) -> Part {
    Part { kind, rows, cols, idx, pos: vec![DVec3::ZERO; rows * cols], nrm: vec![DVec3::ZERO; rows * cols] }
}
/// three の computeVertexNormals（fastNormals と同じ式）
fn face_normals(p: &mut Part) {
    for n in p.nrm.iter_mut() { *n = DVec3::ZERO; }
    for t in p.idx.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        let n = (p.pos[c] - p.pos[b]).cross(p.pos[a] - p.pos[b]);
        p.nrm[a] += n;
        p.nrm[b] += n;
        p.nrm[c] += n;
    }
    for n in p.nrm.iter_mut() { *n = norm(*n); }
}

/// 1 体のイカの形と動き。SquidData が休んだ形を 1 つ持ち、体ごとに写して使う
#[derive(Clone, Debug)]
pub struct Rig {
    pub sp: &'static Species,
    form: Form,
    res: Res,
    pub pose: Pose,
    pub target: Pose,
    pub time: f64,
    pub fin_phase: f64,
    pub breath_phase: f64,
    fun_prev: f64,
    fun_lag: f64,
    pub fun_seed: f64,
    resting: bool,
    parts: Vec<Part>,
    pieces: Vec<Piece>,
    arms: Vec<Arm>,
    tents: Vec<Tent>,
    funnel_base: Vec<(DVec3, DVec3)>,
    funnel_len: f64,
    /// 毎コマの部品の行列（胴の座標へ）
    head_m: DMat4,
    eye_m: [DMat4; 2],
    funnel_m: DMat4,
    g_body: DVec3,
    g_head: DVec3,
    /// 外へ出す向きと大きさ（写しの座標 → 体の座標）
    xf: DMat4,
    length: f64,
    /// atlas の大きさ
    pub atlas_w: u32,
    pub atlas_h: u32,
    /// ジェット（updateJet）：経過（−1 で休み）・強さ・次まで
    jet_t: f64,
    jet_str: f64,
    jet_clock: f64,
    pub body_vertices: usize,
    pub fin_vertices: usize,
}

const ATLAS_W: u32 = 1024;
const GUTTER: u32 = 2;

impl Rig {
    /// 休んだ形で作り、写真の貼り方と atlas の区画を決める（全長 1 m。set_length で変える）
    pub fn new(sp: &'static Species, shape: &Shape, res: Res) -> Rig {
        let form = Form::new(sp, shape);
        let ml = sp.ml;
        let mut parts = Vec::new();
        parts.push(part(Kind::Mantle, res.ms, res.ma + 1, grid_index(res.ms, res.ma + 1, false)));
        for f in 0..2 { parts.push(part(Kind::Fin(f), res.fu, res.fv, grid_index(res.fu, res.fv, false))); }
        parts.push(part(Kind::Head, res.hr + 1, res.ma + 1, grid_index(res.hr + 1, res.ma + 1, true)));
        // 目（three の SphereGeometry(R, ws, hs) と同じ並び）
        let (ws, hs) = res.eye;
        for e in 0..2 {
            let cols = ws + 1;
            let mut idx = Vec::new();
            for iy in 0..hs {
                for ix in 0..ws {
                    let a = (iy * cols + ix + 1) as u32;
                    let b = (iy * cols + ix) as u32;
                    let c = ((iy + 1) * cols + ix) as u32;
                    let d = ((iy + 1) * cols + ix + 1) as u32;
                    if iy != 0 { idx.extend_from_slice(&[a, b, d]); }
                    if iy != hs - 1 { idx.extend_from_slice(&[b, c, d]); }
                }
            }
            let mut p = part(Kind::Eye(e), hs + 1, cols, idx);
            for iy in 0..=hs {
                let v = iy as f64 / hs as f64;
                for ix in 0..=ws {
                    let u = ix as f64 / ws as f64;
                    let q = DVec3::new(-form.eye_r * (u * 2.0 * PI).cos() * (v * PI).sin(), form.eye_r * (v * PI).cos(), form.eye_r * (u * 2.0 * PI).sin() * (v * PI).sin());
                    p.pos[iy * cols + ix] = q;
                    p.nrm[iy * cols + ix] = norm(q);
                }
            }
            parts.push(p);
        }
        let rnd = |i: f64| { let x = (i * 12.9898).sin() * 43758.5453; x - x.floor() };
        let mut arms = Vec::new();
        for k in 0..4 {
            for side in [1.0, -1.0] {
                let pi = parts.len();
                parts.push(part(Kind::Arm(arms.len()), res.arm_n, res.tube_m + 1, grid_index(res.arm_n, res.tube_m + 1, true)));
                arms.push(Arm {
                    k, side, psi: PI / 2.0 - side * (PI / 8.0 + k as f64 * PI / 4.0), len: form.arms[k],
                    phase: rnd((k * 2 + if side > 0.0 { 0 } else { 1 }) as f64) * 6.28, pts: vec![DVec3::ZERO; res.arm_n], part: pi,
                });
            }
        }
        let mut tents = Vec::new();
        for side in [1.0, -1.0] {
            let pi = parts.len();
            parts.push(part(Kind::Tent(tents.len()), res.tent_n, res.tube_m + 1, grid_index(res.tent_n, res.tube_m + 1, true)));
            tents.push(Tent { side, psi: PI / 2.0 - side * (PI * 0.75), pts: vec![DVec3::ZERO; res.tent_n], part: pi });
        }
        // 漏斗（three の CylinderGeometry(rb·0.45, rb, fl, FUN, FUN > 8 ? 7 : 2, true) を rotateX(π/2)・translate(0, 0, fl/2)）
        let fl = 0.12 * ml;
        let rb = form.hw(0.05) * 0.34;
        let (rs, hsg) = (res.fun, if res.fun > 8 { 7 } else { 2 });
        let (rt, rbo) = (rb * 0.45, rb);
        let slope = (rbo - rt) / fl;
        let cols = rs + 1;
        let mut fidx = Vec::new();
        for x in 0..rs {
            for y in 0..hsg {
                let a = (y * cols + x) as u32;
                let b = ((y + 1) * cols + x) as u32;
                let c = ((y + 1) * cols + x + 1) as u32;
                let d = (y * cols + x + 1) as u32;
                fidx.extend_from_slice(&[a, b, d, b, c, d]);
            }
        }
        let mut fp = part(Kind::Funnel, hsg + 1, cols, fidx);
        let mut funnel_base = Vec::new();
        for y in 0..=hsg {
            let v = y as f64 / hsg as f64;
            let r = v * (rbo - rt) + rt;
            for x in 0..=rs {
                let th = x as f64 / rs as f64 * 2.0 * PI;
                let (s, c) = th.sin_cos();
                // (r sin, −v·fl + fl/2, r cos) → rotateX(π/2)：(x, −z, y) → z + fl/2
                let p0 = DVec3::new(r * s, -v * fl + fl / 2.0, r * c);
                let n0 = norm(DVec3::new(s, slope, c));
                let p = DVec3::new(p0.x, -p0.z, p0.y + fl / 2.0);
                let n = DVec3::new(n0.x, -n0.z, n0.y);
                fp.pos[y * cols + x] = p;
                fp.nrm[y * cols + x] = n;
                funnel_base.push((p, n));
            }
        }
        parts.push(fp);

        let mut rig = Rig {
            sp, form, res, pose: NEUTRAL, target: NEUTRAL, time: 0.0, fin_phase: 0.0, breath_phase: 0.0, fun_prev: f64::NAN, fun_lag: 0.0, fun_seed: 0.0, resting: true,
            parts, pieces: Vec::new(), arms, tents, funnel_base, funnel_len: fl,
            head_m: DMat4::IDENTITY, eye_m: [DMat4::IDENTITY; 2], funnel_m: DMat4::IDENTITY, g_body: DVec3::NEG_Y, g_head: DVec3::NEG_Y,
            xf: DMat4::IDENTITY, length: 1.0, atlas_w: ATLAS_W, atlas_h: 0, jet_t: -1.0, jet_str: 0.0, jet_clock: 0.0, body_vertices: 0, fin_vertices: 0,
        };
        // 頭は休んだ形のまま（毎コマ頭の行列をかけるだけ）
        rig.build_head();
        rig.build(0.0);
        rig.bake_photo(shape);
        rig.resting = false;
        rig.set_length(1.0);
        rig.build(0.0);
        rig
    }

    /// 全長（外套の先から、まっすぐ伸ばした腕の先まで）を length_m にそろえる
    pub fn set_length(&mut self, length_m: f64) {
        let f = &self.form;
        let extent = f.mlen + f.head_len + f.arm_max();
        let s = length_m / extent;
        let zc = (-f.mlen + f.head_len + f.arm_max()) / 2.0;
        let top = (0..=20).map(|i| f.top(i as f64 / 20.0)).fold(0.0, f64::max);
        let bot = (0..=20).map(|i| f.bot(i as f64 / 20.0)).fold(0.0, f64::max);
        let yc = (top - bot) / 2.0;
        self.length = length_m;
        // 写しの (x, y, z) → 外套の先を +Z へ（Y のまわりに半回転）・大きさ s・真ん中を原点
        self.xf = DMat4::from_scale(DVec3::splat(s)) * DMat4::from_rotation_y(PI) * DMat4::from_translation(DVec3::new(0.0, -yc, -zc));
    }
    pub fn length(&self) -> f64 { self.length }
    /// 体の厚み（休んだ形の、背から腹まで。体の座標）
    pub fn height(&self) -> f64 {
        let f = &self.form;
        let top = (0..=20).map(|i| f.top(i as f64 / 20.0)).fold(0.0, f64::max);
        let bot = (0..=20).map(|i| f.bot(i as f64 / 20.0)).fold(0.0, f64::max);
        (top + bot) * self.length / (f.mlen + f.head_len + f.arm_max())
    }

    // ---------------------------------------------------------------- 毎コマ（Squid.update）

    /// 走者の値から姿勢の目標を決めて、形を作り直す
    pub fn update(&mut self, dt: f64, now: f64, d: &SquidDrive) {
        let dt = dt.clamp(0.0, 0.1);
        self.set_targets(now, dt, d);
        self.update_jet(dt);
        self.pose.approach(&self.target, dt);
        self.build(dt);
    }

    /// 形だけ作り直す（姿勢は今の pose。dt は時計・ひれの波・漏斗の遅れを進める）
    pub fn build(&mut self, dt: f64) {
        self.time += dt;
        self.fin_phase += PI * 2.0 * self.sp.fin_freq * self.pose.fin_freq * dt;
        self.breath_phase += PI * 2.0 * 0.8 * dt;
        let pose = if self.resting { NEUTRAL } else { self.pose };
        self.head_m = if self.resting { DMat4::IDENTITY } else { DMat4::from_rotation_y(pose.head_yaw) * DMat4::from_rotation_x(-pose.head_pitch) };
        self.g_body = DVec3::NEG_Y;
        self.g_head = DMat3::from_mat4(self.head_m).transpose() * DVec3::NEG_Y;
        self.build_mantle();
        self.build_fins();
        self.build_arms();
        self.build_tentacles();
        let f = &self.form;
        for (e, side) in [1.0f64, -1.0].into_iter().enumerate() {
            let (ep, ey) = if self.resting { (0.0, NEUTRAL.eye_yaw) } else { (pose.eye_pitch, pose.eye_yaw) };
            self.eye_m[e] = self.head_m * DMat4::from_translation(DVec3::new(side * f.eye.x, f.eye.y, f.eye.z)) * DMat4::from_rotation_y(side * (PI / 2.0 - ey)) * DMat4::from_rotation_x(-ep);
        }
        let aim = if self.resting { NEUTRAL.funnel_aim } else { pose.funnel_aim };
        self.funnel_m = DMat4::from_translation(DVec3::new(0.0, -f.hb(0.05) * 0.82, -0.02 * self.sp.ml)) * DMat4::from_rotation_x(aim) * DMat4::from_scale(DVec3::new(1.0, 0.8, 1.0));
        let open = if self.resting { 1.0 } else { 0.75 + 0.5 * pose.funnel_open };
        self.bend_funnel(aim, open, dt);
    }

    /// 胴の点（s 0〜1 前→後ろ、断面の角 phi）。縮み・息・つぶれを入れる（mantlePoint）
    fn mantle_point(&self, s: f64, phi: f64) -> DVec3 {
        let (sp, f, pose) = (self.sp, &self.form, &self.pose);
        let sc = s.clamp(0.0, 1.0);
        let mut a = f.a(sc);
        let sn = phi.sin();
        let mut b = if sn >= 0.0 { f.top(sc) } else { f.bot(sc) };
        let squeeze_at = 1.0 - 0.45 * smooth(0.6, 1.0, s);
        let breath = if self.resting { 0.0 } else { 0.025 * pose.breathe * self.breath_phase.sin() };
        let m = if self.resting { 0.0 } else { pose.mantle };
        let dorsal = if sn > 0.0 { 1.0 + (sp.dorsal_squeeze - 1.0) * sn } else { 1.0 };
        a *= 1.0 + (m + breath) * squeeze_at * dorsal;
        b *= 1.0 + (m + breath) * squeeze_at * if sn > 0.0 { dorsal } else { 1.0 };
        if !self.resting && pose.flat > 0.0 {
            b *= 1.0 - 0.32 * pose.flat;
            a *= 1.0 + 0.12 * pose.flat;
        }
        if s < 0.0 {
            let t = -s / 0.035;
            a += (f.hw(0.0) * 0.97 - a) * t;
            b += ((if sn >= 0.0 { f.ht(0.0) } else { f.hb(0.0) }) * 0.97 - b) * t;
        }
        DVec3::new(a * phi.cos(), b * sn, -s * f.mlen)
    }

    fn phi_at(j: usize, na: usize) -> f64 { -PI / 2.0 + j as f64 / na as f64 * PI * 2.0 }

    fn build_mantle(&mut self) {
        let Res { ms, ma, .. } = self.res;
        let (sp, f) = (self.sp, &self.form);
        let pose = &self.pose;
        let breath = if self.resting { 0.0 } else { 0.025 * pose.breathe * self.breath_phase.sin() };
        let mm = if self.resting { 0.0 } else { pose.mantle };
        let flat = if !self.resting && pose.flat > 0.0 { pose.flat } else { 0.0 };
        let (ac, tc, bc) = (f.hw(0.0) * 0.97, f.ht(0.0) * 0.97, f.hb(0.0) * 0.97);
        let p = &mut self.parts[0];
        for i in 0..ms {
            let t = i as f64 / (ms - 1) as f64;
            let s = -0.035 + 1.035 * (1.0 - (1.0 - t).powf(1.35));
            let sc = s.clamp(0.0, 1.0);
            let (a0, top0, bot0) = (f.a(sc), f.top(sc), f.bot(sc));
            let k = (mm + breath) * (1.0 - 0.45 * smooth(0.6, 1.0, s));
            let ct = if s < 0.0 { -s / 0.035 } else { 0.0 };
            let z = -s * f.mlen;
            for j in 0..=ma {
                let phi = Self::phi_at(j, ma);
                let (sn, cs) = phi.sin_cos();
                let dorsal = if sn > 0.0 { 1.0 + (sp.dorsal_squeeze - 1.0) * sn } else { 1.0 };
                let mut a = a0 * (1.0 + k * dorsal);
                let mut b = (if sn >= 0.0 { top0 } else { bot0 }) * (1.0 + k * if sn > 0.0 { dorsal } else { 1.0 });
                if flat > 0.0 {
                    b *= 1.0 - 0.32 * flat;
                    a *= 1.0 + 0.12 * flat;
                }
                if s < 0.0 {
                    a += (ac - a) * ct;
                    b += ((if sn >= 0.0 { tc } else { bc }) - b) * ct;
                }
                p.pos[i * (ma + 1) + j] = DVec3::new(a * cs, b * sn, z);
            }
        }
        face_normals(p);
        // 継ぎ目（お腹の真ん中）の法線をそろえる
        for i in 0..ms {
            let (a, b) = (i * (ma + 1), i * (ma + 1) + ma);
            let n = (p.nrm[a] + p.nrm[b]) / 2.0;
            p.nrm[a] = n;
            p.nrm[b] = n;
        }
    }

    fn build_head(&mut self) {
        let Res { hr, ma, .. } = self.res;
        let f = self.form.clone();
        let ml = self.sp.ml;
        let p = &mut self.parts[3];
        let l = f.head_len;
        for i in 0..=hr {
            let h = -0.15 + 1.15 * i as f64 / hr as f64;
            let close = if i == hr { 0.05 } else { 1.0 };
            for j in 0..=ma {
                let phi = Self::phi_at(j, ma);
                let sn = phi.sin();
                p.pos[i * (ma + 1) + j] = DVec3::new(f.hw(h) * close * phi.cos(), (if sn >= 0.0 { f.ht(h) } else { f.hb(h) }) * close * sn, h * l + if i == hr { 0.01 * ml } else { 0.0 });
            }
        }
        face_normals(p);
    }

    /// ひれ：胴の横の線に付いたリボン（buildFins）
    fn build_fins(&mut self) {
        let Res { fu, fv, .. } = self.res;
        let rest = self.resting;
        let pose = self.pose;
        let amp = if rest { 0.0 } else { self.sp.fin_amp * pose.fin_amp };
        let ml = self.sp.ml;
        for fi in 0..2 {
            let side = if fi == 0 { 1.0 } else { -1.0 };
            let fwd = (1.0 + (pose.fin_dir + side * pose.fin_turn).clamp(-1.0, 1.0)) / 2.0;
            let phi_a = if side > 0.0 { 0.0 } else { PI };
            let mut pos = std::mem::take(&mut self.parts[1 + fi].pos);
            for i in 0..fu {
                let u = i as f64 / (fu - 1) as f64;
                let s = self.form.fin0 + (self.form.fin1 - self.form.fin0) * u;
                let root = self.mantle_point(s, phi_a);
                let ahead = self.mantle_point((s + 0.01).min(1.02), phi_a);
                let mut n = DVec3::new(-(ahead.z - root.z), 0.0, ahead.x - root.x) * side;
                if n.x * side < 0.0 { n = -n; }
                if n.length_squared() < 1e-12 { n = DVec3::new(side, 0.0, 0.0); }
                let n = norm(n);
                let w = self.form.fin_w(u);
                let env = (PI * u.clamp(0.0, 1.0)).sin().powf(0.5);
                let k = 2.0 * PI * self.sp.fin_waves * u;
                let wave = amp * env * (fwd * (self.fin_phase - k).sin() + (1.0 - fwd) * (self.fin_phase + k).sin());
                let r_local = (0.02 * ml).max(self.form.a(s));
                let lift = if rest { 0.0 } else { pose.fin_lift };
                let fold = if rest { 0.0 } else { pose.fin_fold };
                let mut p = root;
                let mut along = 0.0;
                for j in 0..fv {
                    let v = j as f64 / (fv - 1) as f64;
                    if j > 0 {
                        let step = w / (fv - 1) as f64;
                        along += step;
                        let ang = lift + wave * (0.7 + 0.6 * v) - fold * (0.35 + along / (r_local * 1.08));
                        let mut dd = n * ang.cos();
                        dd.y += ang.sin();
                        p += dd * step;
                        if !rest && pose.sag > 0.0 { p += self.g_body * (pose.sag * step * 0.55 * v); }
                    }
                    pos[i * fv + j] = p;
                }
            }
            let part = &mut self.parts[1 + fi];
            part.pos = pos;
            face_normals(part);
        }
    }

    /// 腕：節の鎖（buildArms。抱く筒・地面は無いので pushOut・keepAbove は省く）
    fn build_arms(&mut self) {
        let n_pts = self.res.arm_n;
        let f = &self.form;
        let pose = if self.resting { NEUTRAL } else { self.pose };
        let hj = 0.8;
        let za = f.head_len * hj;
        let rx = f.hw(hj);
        let ry = (f.ht(hj) + f.hb(hj)) * 0.5;
        let buried = f.head_len * 0.199_999_999_999_999_96;
        let t = if self.resting { 0.0 } else { self.time };
        let x_axis = DVec3::X;
        let pinch_dir = axis_angle(DVec3::Z, x_axis, pose.arm_pitch * (1.0 - 0.6 * pose.wrap) + pose.arm_droop * 0.7);
        let pinch = pose.arm_pinch * (1.0 - pose.wrap) * (1.0 - 0.7 * pose.arm_spread / 1.1);
        let g_head = self.g_head;
        let arm_r = f.arm_r;
        for ai in 0..self.arms.len() {
            let (k, psi, len, phase) = { let a = &self.arms[ai]; (a.k, a.psi, a.len, a.phase) };
            let mut pts = std::mem::take(&mut self.arms[ai].pts);
            let radial = DVec3::new(psi.cos(), psi.sin(), 0.0);
            let rp = 1.0 / ((radial.x / rx).powi(2) + (radial.y / ry).powi(2)).sqrt();
            let r_base = rp * 0.33;
            pts[0] = (radial * (rp - r_base * 0.95)).with_z(za);
            let idle = pose.arm_idle * (0.12 * (t * 0.7 + phase).sin() + 0.08 * (t * 1.9 + phase * 2.0).sin());
            let mut spread = pose.arm_spread * (1.0 - pose.arm_gather) - 0.06 * pose.arm_gather + idle * 0.3;
            if k == 0 { spread += pose.arm_raise; }
            let flare = if k == 2 { pose.arm_flare * pose.arm_bundle * (1.0 - pose.wrap) } else { 0.0 };
            spread += flare * 1.1;
            spread = (spread * (1.0 - pose.wrap) + pose.wrap * 0.18).clamp(-0.5, 1.3);
            let mut tt = axis_angle(DVec3::Z, x_axis, pose.arm_pitch * (1.0 - 0.6 * pose.wrap));
            let mut r = norm(radial - tt * radial.dot(tt));
            let mut b = tt.cross(r);
            let curl = (pose.arm_curl * (1.0 - pose.arm_gather * 0.8) * (1.0 - pose.wrap) + if k == 0 { pose.arm_raise * 0.8 } else { 0.0 } - pose.wrap * 2.4 + idle) * (1.0 - pose.arm_bundle) - flare * 1.5;
            let tip = pose.arm_tip * (1.0 - pose.wrap) - pose.wrap * 1.8;
            let total = len + buried;
            let ds = total / (n_pts - 1) as f64;
            let open0 = buried / total;
            let open1 = (buried + len * 0.25) / total;
            let mut opened = 0.0;
            for i in 1..n_pts {
                let along = i as f64 / (n_pts - 1) as f64;
                let sig = ((along * total - buried) / len).max(0.0);
                let o = smooth(open0, open1, along);
                let k_spread = spread * (o - opened);
                opened = o;
                let kr = k_spread + (curl * if pose.wrap > 0.0 { smooth(0.15, 0.7, sig) * 1.6 } else { 1.0 } + tip * 2.2 * smooth(0.55, 1.0, sig)) * (ds / len);
                let kt = pose.arm_wave * 2.2 * (PI * 2.0 * (sig * 0.9 - t * 0.55) + phase).sin() * sig * (ds / len);
                rotate(&mut tt, &mut r, kr);
                rotate(&mut tt, &mut b, kt);
                if pose.arm_droop > 0.001 && sig > 0.0 {
                    let axis = tt.cross(g_head);
                    let sn = axis.length();
                    if sn > 1e-4 {
                        let a = sn.min(1.0).asin().min(pose.arm_droop * 2.4 * (ds / len) * (0.4 + 0.6 * smooth(0.0, 0.5, sig)));
                        let axis = axis / sn;
                        tt = axis_angle(tt, axis, a);
                        r = axis_angle(r, axis, a);
                    }
                }
                r = norm(r - tt * r.dot(tt));
                b = tt.cross(r);
                pts[i] = pts[i - 1] + tt * ds;
                if pose.sag > 0.0 { pts[i] += g_head * (pose.sag * ds * sig * 0.9); }
            }
            let _ = b;
            if pinch > 0.001 {
                let tip_p = DVec3::new(0.0, 0.0, za) + pinch_dir * (total * 0.95) + radial * (arm_r * 0.55);
                for i in 1..n_pts {
                    let along = i as f64 / (n_pts - 1) as f64;
                    let line = pts[0].lerp(tip_p, along) + radial * ((PI * (along * 1.1).min(1.0)).sin() * rp * 0.28);
                    pts[i] = pts[i].lerp(line, pinch * smooth(0.0, 0.3, along));
                }
            }
            let bundle = pose.arm_bundle * (1.0 - pose.wrap) * (1.0 - flare);
            if bundle > 0.001 {
                let tip_b = DVec3::new(0.0, 0.0, za) + pinch_dir * (total * 0.97);
                let ring0 = rp - r_base * 0.95;
                for i in 1..n_pts {
                    let along = i as f64 / (n_pts - 1) as f64;
                    let sig = ((along * total - buried) / len).max(0.0);
                    let ring = ring0 + (arm_r * (1.0 - 0.82 * sig.powf(0.9)) * 1.15 - ring0) * smooth(0.0, 0.3, along);
                    let line = DVec3::new(0.0, 0.0, za).lerp(tip_b, along) + radial * ring;
                    pts[i] = pts[i].lerp(line, bundle * smooth(0.0, 0.12, along));
                }
            }
            let r0 = arm_r * if k == 3 { 1.1 } else if k == 0 { 0.85 } else { 1.0 };
            let radius = |i: usize| {
                if i == n_pts - 1 { return 0.0; }
                let sig = ((i as f64 / (n_pts - 1) as f64 * total - buried) / len).max(0.0);
                (r_base + (r0 - r_base) * smooth(0.0, 0.3, sig)) * (1.0 - 0.82 * sig.powf(0.9))
            };
            let pi = self.arms[ai].part;
            tube_set(&mut self.parts[pi], self.res.tube_m, &pts, radius, |_| 0.85, -radial);
            self.arms[ai].pts = pts;
        }
    }

    /// 触腕：ふだんは腕の間で縮める（コウイカは隠す）。伸ばす動き（tentOut）は使わない
    fn build_tentacles(&mut self) {
        let n_pts = self.res.tent_n;
        let (sp, f) = (self.sp, &self.form);
        let ml = sp.ml;
        let pose = if self.resting { NEUTRAL } else { self.pose };
        let za = f.head_len;
        let t = if self.resting { 0.0 } else { self.time };
        let tent_r = f.arm_r * (sp.tent_r / sp.arm_r);
        let club_r = f.arm_r * (sp.club_r / sp.arm_r);
        let arm_max = f.arm_max();
        let g_head = self.g_head;
        for ti in 0..self.tents.len() {
            let (tside, psi, pi) = { let tn = &self.tents[ti]; (tn.side, tn.psi, tn.part) };
            let mut pts = std::mem::take(&mut self.tents[ti].pts);
            let radial = DVec3::new(psi.cos(), psi.sin(), 0.0);
            let base = DVec3::new(f.hw(1.0) * 0.4 * radial.x, f.hb(1.0) * 0.4 * radial.y, za - 0.02 * ml);
            let down = pose.arm_pitch + pose.arm_droop * 1.1;
            let bun = pose.arm_bundle * (1.0 - pose.wrap);
            let open = (1.0 - pose.arm_pinch * 0.7) * (1.0 - bun);
            let dir = norm(DVec3::new(radial.x * 0.08 * open, radial.y * 0.08 * open - down.sin(), down.cos()));
            let mut len = sp.tent_rest * ml * pose.tent_show;
            len += (len.min(arm_max * 0.6) - len) * bun;
            if len <= 0.004 {
                for q in pts.iter_mut() { *q = base; }
                tube_set(&mut self.parts[pi], self.res.tube_m, &pts, |_| 0.0, |_| 1.0, -radial);
                self.tents[ti].pts = pts;
                continue;
            }
            let side = norm(dir.cross(DVec3::Y));
            for (i, p) in pts.iter_mut().enumerate() {
                let sig = i as f64 / (n_pts - 1) as f64;
                *p = base + dir * (len * sig);
                let wob = (1.0 - bun) * 0.04 * ml * (sig * 5.0 + t * 1.3 + tside).sin() * sig;
                *p += side * wob;
                if pose.sag > 0.0 { *p += g_head * (pose.sag * len * sig * sig * 0.5); }
            }
            let club0 = 0.74;
            let radius = |i: usize| {
                let sig = i as f64 / (n_pts - 1) as f64;
                if i == n_pts - 1 { return 0.0; }
                let c = smooth(club0, 0.82, sig) * (1.0 - smooth(0.93, 1.0, sig) * 0.6);
                tent_r + (club_r - tent_r) * c
            };
            tube_set(&mut self.parts[pi], self.res.tube_m, &pts, radius, |i| 1.0 - 0.45 * smooth(club0, 0.82, i as f64 / (n_pts - 1) as f64), -radial);
            self.tents[ti].pts = pts;
        }
    }

    /// 漏斗をやわらかい管にする（bendFunnel）
    fn bend_funnel(&mut self, aim: f64, open: f64, dt: f64) {
        let fl = self.funnel_len;
        let (mut bb, mut sway, mut len_k) = (0.0, 0.0, 1.0);
        if !self.resting {
            let fold = 1.0 - smooth(0.25, 1.1, aim);
            len_k = 1.0 - 0.3 * fold;
            if dt > 0.0 {
                let v = if self.fun_prev.is_nan() { 0.0 } else { (aim - self.fun_prev) / dt };
                let goal = (v * 0.12).clamp(-0.6, 0.6);
                self.fun_lag += (goal - self.fun_lag) * (1.0 - (-6.0 * dt).exp());
            }
            self.fun_prev = aim;
            let t = self.time + self.fun_seed;
            bb = 0.65 * fold + self.fun_lag + 0.07 * (t * 1.9).sin() + 0.035 * (t * 4.3).sin();
            sway = 0.1 * (t * 1.3).sin() + 0.05 * (t * 3.1 + 1.0).sin();
        }
        let l = len_k * fl;
        let arc = |u: f64| -> (DVec3, f64) {
            let phi = bb * u;
            let mut c = if bb.abs() < 1e-4 { DVec3::new(0.0, 0.0, l * u) } else { DVec3::new(0.0, l / bb * (1.0 - phi.cos()), l / bb * phi.sin()) };
            c.x = sway * fl * u * u;
            (c, phi)
        };
        let fp = self.parts.iter_mut().find(|p| p.kind == Kind::Funnel).unwrap();
        for (i, (bp, bn)) in self.funnel_base.iter().enumerate() {
            let u = (bp.z / fl).clamp(0.0, 1.0);
            let (c, phi) = arc(u);
            let (sn, cs) = phi.sin_cos();
            let rk = 1.0 + (open - 1.0) * (0.35 + 0.65 * u);
            let (x, y) = (bp.x * rk, bp.y * rk);
            fp.pos[i] = DVec3::new(c.x + x, c.y + y * cs, c.z - y * sn);
            fp.nrm[i] = DVec3::new(bn.x, bn.y * cs + bn.z * sn, -bn.y * sn + bn.z * cs);
        }
    }

    // ---------------------------------------------------------------- 動き（SquidBrain の hover・backward・escape と updateJet の写し）

    /// ジェット：ふくらむ → 縮んで噴く → 吸い直す（updateJet。進む速さは親が決めるので、胴の形だけ）
    fn update_jet(&mut self, dt: f64) {
        let s = self.jet_str * self.sp.jet;
        if self.jet_t < 0.0 {
            self.target.mantle = 0.0;
            return;
        }
        self.jet_t += dt;
        let t = self.jet_t;
        self.target.mantle = if t < 0.14 {
            0.09 * smooth(0.0, 0.14, t) * (s + 0.3).min(1.0)
        } else if t < 0.27 {
            0.09 * (s + 0.3).min(1.0) - (0.09 + 0.24 * s.min(1.1)) * smooth(0.14, 0.24, t)
        } else if t < 0.75 {
            -(0.24 * s.min(1.1)) * (1.0 - smooth(0.27, 0.75, t))
        } else {
            self.jet_t = -1.0;
            0.0
        };
    }

    fn set_targets(&mut self, now: f64, dt: f64, d: &SquidDrive) {
        let kou = self.sp.id == "kouika";
        let mut base = NEUTRAL;
        if kou {
            base.tent_show = 0.0;
            base.arm_spread = 0.06;
            base.arm_gather = 0.35;
            base.arm_pitch = 0.18;
        }
        let ph = now + self.fun_seed * 3.0;
        // その場：ひれを波打たせてとどまる（hover。腕の構えは時間で切り替える armPosture）
        let mut hover = base;
        let c = ph.rem_euclid(22.0);
        if c < 10.0 {
            hover.arm_pinch = 1.0; hover.arm_droop = 0.0; hover.arm_gather = 0.3; hover.arm_wave = 0.08;
        } else if c < 17.0 {
            hover.arm_pinch = 0.12; hover.arm_droop = 0.85; hover.arm_pitch = 0.25; hover.arm_gather = 0.0; hover.arm_spread = 0.1; hover.arm_curl = 0.0; hover.arm_tip = 0.15; hover.arm_wave = 0.12;
        } else {
            hover.arm_pinch = 0.45; hover.arm_droop = 0.2;
        }
        hover.eye_yaw = 0.2 + 0.2 * (ph * 0.45).sin();
        hover.eye_pitch = 0.15 * (ph * 0.31).sin();
        // ひれで進む：外套の先を前にして進むので、three 版の backward（ひれの波を後ろから前へ、腕をそろえ、弱いジェットを足す）
        let sp = d.speed.max(0.0);
        let mut swim = base;
        swim.arm_pinch = 0.9; swim.fin_dir = -1.0; swim.fin_amp = 1.5 + 0.4 * smooth(1.0, 14.0, sp); swim.fin_freq = 1.3 + 1.2 * smooth(1.0, 14.0, sp);
        swim.arm_gather = 0.85; swim.arm_spread = 0.0; swim.arm_curl = 0.0; swim.arm_tip = 0.1; swim.arm_pitch = 0.04; swim.arm_wave = 0.08; swim.funnel_aim = 0.05; swim.funnel_open = 0.6;
        // ジェット：escape の始め（ひれをたたみ、腕をそろえ、漏斗を腕の方へ向けて噴く）
        let mut jet = base;
        jet.fin_fold = 1.0; jet.fin_amp = 0.15; jet.arm_gather = 1.0; jet.arm_spread = 0.0; jet.arm_curl = 0.0; jet.arm_tip = 0.0; jet.arm_pitch = 0.0; jet.arm_idle = 0.0; jet.arm_wave = 0.0;
        jet.arm_pinch = 0.0; jet.funnel_aim = 0.0; jet.funnel_open = 1.0; jet.eye_yaw = 0.5;
        let run = smooth(0.3, 2.5, sp);
        // 走者のふつうの速さは約 13.5 m/s（sim の base）。それまではひれで進み、ダッシュ（約 16 m/s〜）でジェットにする
        let fast = d.dash.clamp(0.0, 1.0).max(smooth(14.5, 19.0, sp));
        let mut t = hover.lerp(&swim, run).lerp(&jet, fast);
        if d.air {
            // 跳ぶ：腕を根元から束ねて細くなる
            let mut air = jet;
            air.arm_bundle = 0.6; air.fin_fold = 0.6; air.fin_amp = 0.5; air.funnel_open = 0.8;
            t = t.lerp(&air, 0.85);
        }
        let hurt = d.hurt.clamp(0.0, 1.0);
        if hurt > 0.0 {
            // よろけ：腕を広げて反らし、ひれをばたつかせ、頭をふる
            let mut h = base;
            h.arm_spread = 1.0; h.arm_curl = 0.9; h.arm_tip = 1.0; h.arm_wave = 0.6; h.arm_idle = 3.0; h.arm_pinch = 0.0; h.arm_gather = 0.0;
            h.fin_amp = 2.6; h.fin_freq = 2.4; h.fin_dir = 0.0; h.funnel_open = 1.0; h.funnel_aim = 0.9; h.head_yaw = 0.18 * (now * 23.0).sin(); h.flat = 0.3; h.eye_yaw = 0.5;
            t = t.lerp(&h, hurt);
        }
        if d.joy {
            // 喜び：腕を開いて上の 2 本をふり、ひれを大きく波打たせる
            let mut j = base;
            j.arm_spread = 0.55; j.arm_curl = -0.35; j.arm_tip = 0.5; j.arm_wave = 0.5; j.arm_raise = 0.35 + 0.3 * (now * 5.0).sin(); j.arm_idle = 2.0; j.arm_pinch = 0.0; j.arm_gather = 0.0;
            j.fin_amp = 2.0; j.fin_freq = 1.5; j.fin_lift = 0.15 * (now * 3.0).sin(); j.head_pitch = 0.12; j.eye_yaw = 0.35;
            t = j;
        }
        t.mantle = self.target.mantle;
        self.target = t;
        // ジェットの間合い（速いほど強く・短い間で噴く。ひれで進むときは弱いジェットをときどき）
        let (period, strength) = if d.joy { (f64::INFINITY, 0.0) } else if hurt > 0.3 { (0.5, 0.8) } else if fast > 0.25 { (1.0 - 0.45 * fast, 0.55 + 0.45 * fast) } else if run > 0.5 { (1.4 - 0.6 * smooth(4.0, 14.0, sp), 0.25 + 0.15 * smooth(4.0, 14.0, sp)) } else { (f64::INFINITY, 0.0) };
        self.jet_clock += dt;
        if self.jet_t < 0.0 && self.jet_clock >= period {
            self.jet_t = 0.0;
            self.jet_str = strength;
            self.jet_clock = 0.0;
        }
    }

    // ---------------------------------------------------------------- 写真の貼り方（bakePhoto・Tube.setPhoto・目の材質）と atlas の区画

    fn bake_photo(&mut self, sh: &Shape) {
        let hf = self.sp.height_fix;
        let ml = self.sp.ml;
        let (dm, vm, smp) = (sh.dorsal, sh.ventral, sh.side);
        let photo_uv = |p: DVec3| -> [[f64; 2]; 3] {
            [
                [(dm.x0 - p.z * dm.k) / dm.w, 1.0 - (dm.yc + p.x * dm.k) / dm.h],
                [(vm.x0 - p.z * vm.k) / vm.w, 1.0 - (vm.yc + p.x * vm.k) / vm.h],
                [(smp.x0 - p.z * smp.k) / smp.w, 1.0 - (smp.yc - p.y / hf * smp.k) / smp.h],
            ]
        };
        let blend = |n: DVec3| -> [f64; 3] {
            let t = n.y.max(0.0).powi(3);
            let b = (-n.y).max(0.0).powi(3);
            let s = n.x.abs().powi(3) * 1.1;
            let sum = t + b + s;
            let sum = if sum == 0.0 { 1.0 } else { sum };
            [t / sum, b / sum, s / sum]
        };
        // 腕の写真：背中の写真で、腕の通り道（armTracks）を根元から先へ
        let track_of = |side: f64, k: usize| -> &ArmTrack {
            let mut mine: Vec<&ArmTrack> = sh.tracks.iter().filter(|t| (t.y[0] - dm.yc) * side > 0.0).collect();
            mine.sort_by(|a, b| (a.y[0] - dm.yc).abs().partial_cmp(&(b.y[0] - dm.yc).abs()).unwrap());
            mine[k.min(mine.len() - 1)]
        };
        let strip_uv = |tr: &ArmTrack, sig: f64, ang: f64| -> [f64; 2] {
            let px = sample(&sh.track_x, sig);
            let py = sample(&tr.y, sig) + ang.sin() * tr.hw * 0.75 * (1.0 - 0.5 * sig);
            [px / dm.w, 1.0 - py / dm.h]
        };
        let e = sh.eye_px;
        let ueye = [e[0] / smp.w, e[1] / smp.h, e[2] / 0.85 / smp.w, e[2] / 0.85 / smp.h];
        let mut pieces = Vec::new();
        for (pi, p) in self.parts.iter().enumerate() {
            let n = p.pos.len();
            let mut attr = vec![VAttr::default(); n];
            match p.kind {
                Kind::Mantle | Kind::Head | Kind::Funnel => {
                    let m = if p.kind == Kind::Funnel { self.funnel_m } else { DMat4::IDENTITY };
                    let nm = DMat3::from_mat4(m).inverse().transpose();
                    for i in 0..n {
                        let q = m.transform_point3(p.pos[i]);
                        attr[i] = VAttr { uv: photo_uv(q), w: blend(norm(nm * p.nrm[i])), skin: [0.0, 0.0, 0.0] };
                    }
                    pieces.push(Piece { part: pi, flip: false, fin_mesh: false, attr, region: [0; 4], off: 0 });
                }
                Kind::Fin(_) => {
                    // ひれは 1 枚の板の表と裏：上を向いた面は背中の写真、下を向いた面はお腹の写真
                    let up = p.nrm.iter().map(|n| n.y).sum::<f64>() >= 0.0;
                    for sheet in 0..2 {
                        let flip = sheet == 1;
                        let dorsal = up != flip;
                        let mut a = vec![VAttr::default(); n];
                        for i in 0..n {
                            a[i] = VAttr { uv: photo_uv(p.pos[i]), w: if dorsal { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }, skin: [0.0, 0.0, 0.0] };
                        }
                        pieces.push(Piece { part: pi, flip, fin_mesh: true, attr: a, region: [0; 4], off: 0 });
                    }
                }
                Kind::Arm(_) | Kind::Tent(_) => {
                    let (side, k, tent, pts) = match p.kind {
                        Kind::Arm(ai) => { let a = &self.arms[ai]; (a.side, a.k, false, &a.pts) }
                        Kind::Tent(ti) => { let t = &self.tents[ti]; (t.side, 1, true, &t.pts) }
                        _ => unreachable!(),
                    };
                    let tr = track_of(side, k);
                    let m = p.cols - 1;
                    let mut along = 0.0;
                    for i in 0..p.rows {
                        if i > 0 { along += pts[i].distance(pts[i - 1]); }
                        let sig = i as f64 / (p.rows - 1) as f64;
                        // 休んだ形の太さ（まわりの長さの目安）
                        let ri = p.pos[i * p.cols].distance(pts[i]).max(0.002);
                        for j in 0..=m {
                            let ang = j as f64 / m as f64 * PI * 2.0;
                            let uv = strip_uv(tr, if tent { (sig * 0.9).min(1.0) } else { sig }, ang);
                            attr[i * p.cols + j] = VAttr { uv: [uv, [0.0; 2], [0.0; 2]], w: [1.0, 0.0, 0.0], skin: [along / ml, ang * ri / ml, ang.cos()] };
                        }
                    }
                    pieces.push(Piece { part: pi, flip: false, fin_mesh: false, attr, region: [0; 4], off: 0 });
                }
                Kind::Eye(ei) => {
                    let mirror = if ei == 0 { 1.0 } else { -1.0 };
                    for i in 0..n {
                        let dd = norm(p.pos[i]);
                        attr[i] = VAttr { uv: [[0.0; 2], [0.0; 2], [ueye[0] + mirror * dd.x * ueye[2], 1.0 - (ueye[1] - dd.y * ueye[3])]], w: [0.0, 0.0, 1.0], skin: [0.0; 3] };
                    }
                    pieces.push(Piece { part: pi, flip: false, fin_mesh: false, attr, region: [0; 4], off: 0 });
                }
            }
        }
        // 区画の大きさ：休んだ形の長さ × 密度（胴長を 600 画素ほど）。幅 1024 の棚に詰める
        let mut density = 600.0 / self.form.mlen;
        let sizes: Vec<(f64, f64)> = pieces.iter().map(|pc| {
            let p = &self.parts[pc.part];
            let m = match p.kind { Kind::Funnel => self.funnel_m, _ => DMat4::IDENTITY };
            let at = |i: usize, j: usize| m.transform_point3(p.pos[i * p.cols + j]);
            let lc = (0..p.rows).map(|i| (1..p.cols).map(|j| at(i, j).distance(at(i, j - 1))).sum::<f64>()).fold(0.0, f64::max);
            let lr = (0..p.cols).map(|j| (1..p.rows).map(|i| at(i, j).distance(at(i - 1, j))).sum::<f64>()).fold(0.0, f64::max);
            (lc, lr)
        }).collect();
        loop {
            let dims: Vec<(u32, u32)> = sizes.iter().map(|(lc, lr)| (((lc * density).ceil() as u32).clamp(4, ATLAS_W - 2 * GUTTER), ((lr * density).ceil() as u32).clamp(4, 1024))).collect();
            let mut order: Vec<usize> = (0..dims.len()).collect();
            order.sort_by_key(|&i| std::cmp::Reverse(dims[i].1));
            let (mut x, mut y, mut shelf) = (0u32, 0u32, 0u32);
            let mut regions = vec![[0u32; 4]; dims.len()];
            for &i in &order {
                let (w, h) = (dims[i].0 + 2 * GUTTER, dims[i].1 + 2 * GUTTER);
                if x + w > ATLAS_W { x = 0; y += shelf; shelf = 0; }
                regions[i] = [x + GUTTER, y + GUTTER, dims[i].0, dims[i].1];
                x += w;
                shelf = shelf.max(h);
            }
            let total = y + shelf;
            if total <= 2048 {
                for (pc, r) in pieces.iter_mut().zip(regions) { pc.region = r; }
                self.atlas_h = total.div_ceil(4) * 4;
                break;
            }
            density *= 0.85;
        }
        // 出す形の中の頂点の始まり（体：胴・頭・目・腕・触腕・漏斗、ひれ：2 枚ずつ）
        let (mut ob, mut of) = (0, 0);
        for pc in pieces.iter_mut() {
            let n = self.parts[pc.part].pos.len();
            if pc.fin_mesh { pc.off = of; of += n; } else { pc.off = ob; ob += n; }
        }
        self.body_vertices = ob;
        self.fin_vertices = of;
        self.pieces = pieces;
    }

    // ---------------------------------------------------------------- 外へ出す形

    fn part_matrix(&self, k: Kind) -> DMat4 {
        match k {
            Kind::Mantle | Kind::Fin(_) => DMat4::IDENTITY,
            Kind::Head | Kind::Arm(_) | Kind::Tent(_) => self.head_m,
            Kind::Eye(e) => self.eye_m[e],
            Kind::Funnel => self.funnel_m,
        }
    }

    /// 体（ひれ以外）とひれの形を作る（頂点の並び・UV・添字。位置と法線は fill が毎コマ書く）
    pub fn new_geometries(&self) -> (Geometry, Geometry) {
        let (aw, ah) = (self.atlas_w as f64, self.atlas_h as f64);
        let mut g = [Geometry::default(), Geometry::default()];
        g[0].positions = vec![[0.0; 3]; self.body_vertices];
        g[0].normals = vec![[0.0; 3]; self.body_vertices];
        g[0].uvs = vec![[0.0; 2]; self.body_vertices];
        g[1].positions = vec![[0.0; 3]; self.fin_vertices];
        g[1].normals = vec![[0.0; 3]; self.fin_vertices];
        g[1].uvs = vec![[0.0; 2]; self.fin_vertices];
        let mut idx = [Vec::new(), Vec::new()];
        for pc in &self.pieces {
            let p = &self.parts[pc.part];
            let gi = pc.fin_mesh as usize;
            let [x0, y0, w, h] = pc.region;
            for i in 0..p.rows {
                for j in 0..p.cols {
                    let u = (x0 as f64 + 0.5 + j as f64 / (p.cols - 1) as f64 * (w - 1) as f64) / aw;
                    let v = (y0 as f64 + 0.5 + i as f64 / (p.rows - 1) as f64 * (h - 1) as f64) / ah;
                    g[gi].uvs[pc.off + i * p.cols + j] = [u as f32, v as f32];
                }
            }
            for t in p.idx.chunks_exact(3) {
                let o = pc.off as u32;
                if pc.flip { idx[gi].extend_from_slice(&[t[0] + o, t[2] + o, t[1] + o]); } else { idx[gi].extend_from_slice(&[t[0] + o, t[1] + o, t[2] + o]); }
            }
        }
        let [i0, i1] = idx;
        g[0].indices = Some(i0);
        g[1].indices = Some(i1);
        let [a, b] = g;
        let mut r = (a, b);
        self.fill(&mut r.0, &mut r.1);
        r
    }

    /// 今の形の位置と法線を書く（体の座標：外套の先 +Z・背 +Y・真ん中が原点）
    pub fn fill(&self, body: &mut Geometry, fins: &mut Geometry) {
        let xr = DMat3::from_mat4(self.xf);
        for pc in &self.pieces {
            let p = &self.parts[pc.part];
            let m = self.xf * self.part_matrix(p.kind);
            let nm = xr * DMat3::from_mat4(self.part_matrix(p.kind)).inverse().transpose();
            let g = if pc.fin_mesh { &mut *fins } else { &mut *body };
            let sgn = if pc.flip { -1.0 } else { 1.0 };
            for i in 0..p.pos.len() {
                let q = m.transform_point3(p.pos[i]);
                let n = norm(nm * p.nrm[i]) * sgn;
                g.positions[pc.off + i] = [q.x as f32, q.y as f32, q.z as f32];
                g.normals[pc.off + i] = [n.x as f32, n.y as f32, n.z as f32];
            }
        }
    }

    /// 部品ごとの頂点（胴の座標＝写しの座標のまま。three 版と比べる試験用）
    pub fn parts_in_root(&self) -> Vec<(Kind, Vec<DVec3>)> {
        self.parts.iter().map(|p| { let m = self.part_matrix(p.kind); (p.kind, p.pos.iter().map(|&q| m.transform_point3(q)).collect()) }).collect()
    }

    /// 姿勢と時計をそのまま置いて形を作る（three 版と比べる試験用。dt = 0 で作り直す）
    pub fn set_state(&mut self, pose: Pose, time: f64, fin_phase: f64, breath_phase: f64) {
        self.pose = pose;
        self.target = pose;
        self.time = time;
        self.fin_phase = fin_phase;
        self.breath_phase = breath_phase;
        self.fun_prev = f64::NAN;
        self.fun_lag = 0.0;
        self.build(0.0);
    }

    /// atlas を焼く。photos は背・腹・横の写真（幅・高さ・RGBA。上下を反転した並び＝下の行が先。island-kit の load_image と同じ）。
    /// 返す RGBA は sRGB（上の行が先ではなく、UV の v の小さい方が先。data_texture にそのまま渡す）
    pub fn bake_atlas(&self, shape: &Shape, photos: [(u32, u32, &[u8]); 3]) -> Vec<u8> {
        let (aw, ah) = (self.atlas_w as usize, self.atlas_h as usize);
        let mut out = vec![0u8; aw * ah * 4];
        let lut: Vec<f64> = (0..256).map(|i| srgb_to_linear(i as f64 / 255.0)).collect();
        let samp = |k: usize, uv: [f64; 2]| -> [f64; 3] {
            let (w, h, px) = photos[k];
            let x = (uv[0] * w as f64 - 0.5).clamp(0.0, (w - 1) as f64);
            let y = (uv[1] * h as f64 - 0.5).clamp(0.0, (h - 1) as f64);
            let (x0, y0) = (x.floor() as usize, y.floor() as usize);
            let (x1, y1) = ((x0 + 1).min(w as usize - 1), (y0 + 1).min(h as usize - 1));
            let (fx, fy) = (x - x0 as f64, y - y0 as f64);
            let mut c = [0.0; 3];
            for (ch, cv) in c.iter_mut().enumerate() {
                let g = |xx: usize, yy: usize| lut[px[(yy * w as usize + xx) * 4 + ch] as usize];
                *cv = (g(x0, y0) * (1.0 - fx) + g(x1, y0) * fx) * (1.0 - fy) + (g(x0, y1) * (1.0 - fx) + g(x1, y1) * fx) * fy;
            }
            c
        };
        let lum = |c: [f64; 3]| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
        let bg = shape.bg.map(srgb_to_linear);
        let belly = self.sp.belly.map(srgb_to_linear);
        let arm_light = [0.9, 0.86, 0.79].map(srgb_to_linear);
        let arm_brown = [0.56, 0.44, 0.32].map(srgb_to_linear);
        let mix3 = |a: [f64; 3], b: [f64; 3], k: f64| [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k];
        for pc in &self.pieces {
            let p = &self.parts[pc.part];
            let [x0, y0, w, h] = pc.region;
            let (x0, y0, w, h) = (x0 as i64, y0 as i64, w as i64, h as i64);
            let g = GUTTER as i64;
            for py in (y0 - g)..(y0 + h + g) {
                if py < 0 || py >= ah as i64 { continue; }
                let fi = ((py - y0) as f64 / (h - 1) as f64).clamp(0.0, 1.0) * (p.rows - 1) as f64;
                let i0 = (fi.floor() as usize).min(p.rows - 2);
                let ti = fi - i0 as f64;
                for px in (x0 - g)..(x0 + w + g) {
                    if px < 0 || px >= aw as i64 { continue; }
                    let fj = ((px - x0) as f64 / (w - 1) as f64).clamp(0.0, 1.0) * (p.cols - 1) as f64;
                    let j0 = (fj.floor() as usize).min(p.cols - 2);
                    let tj = fj - j0 as f64;
                    let a = |i: usize, j: usize| &pc.attr[i * p.cols + j];
                    let (a00, a01, a10, a11) = (a(i0, j0), a(i0, j0 + 1), a(i0 + 1, j0), a(i0 + 1, j0 + 1));
                    let bl = |f: &dyn Fn(&VAttr) -> f64| (f(a00) * (1.0 - tj) + f(a01) * tj) * (1.0 - ti) + (f(a10) * (1.0 - tj) + f(a11) * tj) * ti;
                    let wt = [bl(&|v| v.w[0]), bl(&|v| v.w[1]), bl(&|v| v.w[2])];
                    let mut col = [0.0; 3];
                    for k in 0..3 {
                        if wt[k] <= 1e-4 { continue; }
                        let c = samp(k, [bl(&|v| v.uv[k][0]), bl(&|v| v.uv[k][1])]);
                        for ch in 0..3 { col[ch] += c[ch] * wt[k]; }
                    }
                    let mut alpha = 1.0;
                    match p.kind {
                        Kind::Fin(_) => {
                            // 透けたひれ：写真は暗い背景に透けた膜なので、膜の色と透け具合に分ける（C = a·F + (1 − a)·bg）
                            let a = ((lum(col) - lum(bg)) / (self.sp.fin_lum - lum(bg)).max(0.05)).clamp(self.sp.fin_min, 1.0);
                            for ch in 0..3 { col[ch] = ((col[ch] - (1.0 - a) * bg[ch]) / a).clamp(0.0, 1.0); }
                            alpha = a;
                        }
                        Kind::Arm(_) | Kind::Tent(_) => {
                            let skin = [bl(&|v| v.skin[0]), bl(&|v| v.skin[1]), bl(&|v| v.skin[2])];
                            if matches!(p.kind, Kind::Tent(_)) { col = mix3(col, col.map(|c| c * 1.25 + 0.04), 0.5); }
                            if self.sp.arm_white > 0.0 {
                                let l = lum(col);
                                let wc = mix3(arm_brown, arm_light, smooth(0.04, 0.32, l));
                                let k = 0.8 + 0.5 * smooth(0.1, 0.5, l);
                                col = mix3(col, wc.map(|c| c * k), self.sp.arm_white * (0.55 + 0.45 * smooth(0.03, 0.12, skin[0])));
                            }
                            // 腕の口の側は、白っぽい皮と吸盤
                            let q = [skin[0] * 70.0, skin[1] * 70.0];
                            let dd = ((q[0] - q[0].floor() - 0.5).powi(2) + (q[1] - q[1].floor() - 0.5).powi(2)).sqrt();
                            let oral = smooth(0.35, 0.8, skin[2]);
                            col = mix3(col, mix3(belly, col, 0.35), oral * 0.8);
                            col = mix3(col, col.map(|c| c * 0.62), oral * smooth(0.38, 0.28, dd) * 0.7);
                        }
                        _ => {}
                    }
                    let o = (py as usize * aw + px as usize) * 4;
                    for ch in 0..3 { out[o + ch] = (linear_to_srgb(col[ch]) * 255.0 + 0.5).clamp(0.0, 255.0) as u8; }
                    out[o + 3] = (alpha * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
                }
            }
        }
        out
    }
}

/// Tube.set：点 pts、太さ r(i)、平たさ flat(i)、最初の口の側の向き oral
fn tube_set(p: &mut Part, m: usize, pts: &[DVec3], r: impl Fn(usize) -> f64, flat: impl Fn(usize) -> f64, oral: DVec3) {
    let n = pts.len();
    let mut nn = oral;
    for i in 0..n {
        let a = pts[i.saturating_sub(1)];
        let b = pts[(i + 1).min(n - 1)];
        let mut t = b - a;
        if t.length_squared() < 1e-12 { t = DVec3::Z; }
        let t = norm(t);
        nn -= t * nn.dot(t);
        if nn.length_squared() < 1e-10 { nn = DVec3::X - t * t.x; }
        nn = norm(nn);
        let bb = t.cross(nn);
        let (ri, fl) = (r(i), flat(i));
        for j in 0..=m {
            let ang = j as f64 / m as f64 * PI * 2.0;
            let (s, c) = ang.sin_cos();
            let d = nn * (c * fl) + bb * s;
            p.pos[i * (m + 1) + j] = pts[i] + d * ri;
            p.nrm[i * (m + 1) + j] = norm(nn * c + bb * (s * fl));
        }
    }
}

fn srgb_to_linear(c: f64) -> f64 { if c < 0.04045 { c * 0.0773993808 } else { (c * 0.9478672986 + 0.0521327014).powf(2.4) } }
fn linear_to_srgb(c: f64) -> f64 { if c < 0.0031308 { c * 12.92 } else { 1.055 * c.powf(0.41666) - 0.055 } }

#[cfg(test)]
mod tests {
    use super::*;

    fn shape_of(id: &str) -> Shape {
        let p = format!("{}/../../public/squid/{id}.json", env!("CARGO_MANIFEST_DIR"));
        parse_shape(&std::fs::read_to_string(p).unwrap()).unwrap()
    }
    fn zs(g: &Geometry) -> (f32, f32) { g.positions.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p[2]), b.max(p[2]))) }

    #[test]
    fn shape_faces_plus_z_and_length() {
        for id in ["aori", "kouika"] {
            let sh = shape_of(id);
            let mut rig = Rig::new(species(id).unwrap(), &sh, MID);
            rig.set_length(2.0);
            rig.build(0.0);
            let (body, fins) = rig.new_geometries();
            // 頂点数は 1 体で数千まで
            let n = body.positions.len() + fins.positions.len();
            assert!(n > 1500 && n < 4000, "{id}: 頂点 {n}");
            // 外套の先（胴の最後の輪）が +Z の端（全長の半分）
            let mantle_tip = rig.xf.transform_point3(rig.parts[0].pos[rig.parts[0].pos.len() - 1]);
            assert!((mantle_tip.z - 1.0).abs() < 1e-6, "{id}: 外套の先 z = {}", mantle_tip.z);
            let (lo, hi) = zs(&body);
            assert!(hi <= 1.0 + 1e-4 && hi > 0.99, "{id}: 前の端 {hi}");
            // 後ろの端は腕の先（−1）のあたり。アオリの触腕は腕より 2% ほど先へ出る
            assert!(lo >= -1.05 && lo < -0.8, "{id}: 後ろの端 {lo}");
            // 背が +Y：いちばん高い頂点は背中の写真を貼る所（胴・頭）
            let (top_i, _) = body.positions.iter().enumerate().max_by(|a, b| a.1[1].partial_cmp(&b.1[1]).unwrap()).unwrap();
            let pc = rig.pieces.iter().find(|pc| !pc.fin_mesh && top_i >= pc.off && top_i < pc.off + rig.parts[pc.part].pos.len()).unwrap();
            let w = pc.attr[top_i - pc.off].w;
            assert!(w[0] > 0.8, "{id}: いちばん上の頂点の写真の重み {w:?}（{:?}）", rig.parts[pc.part].kind);
            // 目は頭の左右（体の座標で z < 外套の前の縁）
            assert!(body.indices.as_ref().unwrap().iter().all(|&i| (i as usize) < body.positions.len()));
        }
    }

    #[test]
    fn moves_and_stays_finite() {
        let sh = shape_of("aori");
        let mut rig = Rig::new(&AORI, &sh, MID);
        rig.set_length(1.0);
        let (mut body, mut fins) = rig.new_geometries();
        let drives = [
            SquidDrive::default(),
            SquidDrive { speed: 5.0, ..Default::default() },
            SquidDrive { speed: 18.0, dash: 1.0, ..Default::default() },
            SquidDrive { speed: 10.0, air: true, ..Default::default() },
            SquidDrive { speed: 3.0, hurt: 1.0, ..Default::default() },
            SquidDrive { joy: true, ..Default::default() },
        ];
        let mut t = 0.0;
        for d in drives {
            for _ in 0..90 {
                t += 1.0 / 60.0;
                rig.update(1.0 / 60.0, t, &d);
                rig.fill(&mut body, &mut fins);
                assert!(body.positions.iter().chain(&fins.positions).all(|p| p.iter().all(|v| v.is_finite() && v.abs() < 2.0)), "{d:?}");
            }
            let (_, hi) = zs(&body);
            assert!(hi > 0.45, "{d:?}: 外套の先が前にある {hi}");
        }
        // ダッシュではジェットで胴が縮む
        rig = Rig::new(&AORI, &sh, MID);
        let mut min_mantle: f64 = 0.0;
        for i in 0..120 { rig.update(1.0 / 60.0, i as f64 / 60.0, &SquidDrive { speed: 18.0, dash: 1.0, ..Default::default() }); min_mantle = min_mantle.min(rig.pose.mantle); }
        assert!(min_mantle < -0.15, "ジェットで縮む {min_mantle}");
    }

    #[test]
    fn atlas_maps_photos() {
        // 背・腹・横を赤・緑・青の写真にして焼くと、背中は赤・お腹は緑・目は青になる
        let sh = shape_of("aori");
        let rig = Rig::new(&AORI, &sh, MID);
        let solid = |c: [u8; 3]| { let mut v = vec![0u8; 8 * 8 * 4]; for p in v.chunks_exact_mut(4) { p[..3].copy_from_slice(&c); p[3] = 255; } v };
        let (r, g, b) = (solid([255, 0, 0]), solid([0, 255, 0]), solid([0, 0, 255]));
        let atlas = rig.bake_atlas(&sh, [(8, 8, &r), (8, 8, &g), (8, 8, &b)]);
        let (body, _) = rig.new_geometries();
        let px = |i: usize| {
            let uv = body.uvs[i];
            let (x, y) = ((uv[0] * rig.atlas_w as f32) as usize, (uv[1] * rig.atlas_h as f32) as usize);
            let o = (y * rig.atlas_w as usize + x) * 4;
            [atlas[o], atlas[o + 1], atlas[o + 2]]
        };
        let ma = MID.ma;
        let ring = 10 * (ma + 1);
        let top = px(ring + ma / 2); // phi = π/2（背）
        let bot = px(ring); // phi = −π/2（お腹）
        assert!(top[0] > 200 && top[1] < 40, "背 {top:?}");
        assert!(bot[1] > 200 && bot[0] < 40, "お腹 {bot:?}");
        let eye = rig.pieces.iter().find(|pc| matches!(rig.parts[pc.part].kind, Kind::Eye(_))).unwrap();
        let e = px(eye.off + 20);
        assert!(e[2] > 200, "目 {e:?}");
        println!("atlas {}x{}", rig.atlas_w, rig.atlas_h);
    }

    /// three 版（squid-kit.js）と同じ細かさで、部品ごとの頂点を比べる。SQUID_REF に node で書き出した JSON を渡したときだけ
    #[test]
    fn matches_three_reference() {
        let Ok(path) = std::env::var("SQUID_REF") else { return };
        let v: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for case in v.as_array().unwrap() {
            let id = case["id"].as_str().unwrap();
            let sh = shape_of(id);
            let mut rig = Rig::new(species(id).unwrap(), &sh, FULL);
            let mut pose = NEUTRAL;
            let pj = &case["pose"];
            macro_rules! set { ($($f:ident = $k:literal),*) => { $( if let Some(x) = pj.get($k).and_then(Value::as_f64) { pose.$f = x; } )* } }
            set!(mantle = "mantle", breathe = "breathe", head_pitch = "headPitch", head_yaw = "headYaw", eye_yaw = "eyeYaw", eye_pitch = "eyePitch", fin_amp = "finAmp", fin_freq = "finFreq",
                fin_dir = "finDir", fin_fold = "finFold", fin_lift = "finLift", fin_turn = "finTurn", arm_spread = "armSpread", arm_curl = "armCurl", arm_tip = "armTip", arm_wave = "armWave",
                arm_pitch = "armPitch", arm_gather = "armGather", arm_raise = "armRaise", arm_idle = "armIdle", arm_pinch = "armPinch", arm_droop = "armDroop", arm_bundle = "armBundle",
                arm_flare = "armFlare", tent_show = "tentShow", funnel_aim = "funnelAim", funnel_open = "funnelOpen", sag = "sag", flat = "flat");
            rig.set_state(pose, case["time"].as_f64().unwrap(), case["finPhase"].as_f64().unwrap(), case["breathPhase"].as_f64().unwrap());
            let mine = rig.parts_in_root();
            let refp = case["parts"].as_array().unwrap();
            let want = |k: Kind| -> &str { match k { Kind::Mantle => "mantle", Kind::Fin(_) => "fin", Kind::Head => "head", Kind::Eye(_) => "eye", Kind::Arm(_) => "arm", Kind::Tent(_) => "tentacle", Kind::Funnel => "funnel" } };
            let mut used = vec![false; refp.len()];
            let mut worst = 0.0f64;
            for (k, pts) in &mine {
                // 同じ種類の、まだ使っていない部品（作った順）
                let (ri, r) = refp.iter().enumerate().find(|(i, r)| !used[*i] && r["kind"].as_str() == Some(want(*k))).unwrap_or_else(|| panic!("{id}: {k:?} が three 版に無い"));
                used[ri] = true;
                let rp = r["pos"].as_array().unwrap();
                assert_eq!(rp.len() / 3, pts.len(), "{id}: {k:?} の頂点数");
                for (i, q) in pts.iter().enumerate() {
                    let d = (DVec3::new(rp[i * 3].as_f64().unwrap(), rp[i * 3 + 1].as_f64().unwrap(), rp[i * 3 + 2].as_f64().unwrap()) - *q).length();
                    worst = worst.max(d);
                    assert!(d < 1e-5, "{id} {}: {k:?} の {i} 番が {d} m ずれる", case["name"]);
                }
            }
            println!("{id} {}: いちばんのずれ {worst:e} m", case["name"]);
        }
    }

    /// 形の絵の確かめ用：SQUID_DUMP のフォルダに、写真（raw RGBA、下の行が先）から atlas と OBJ を書く
    #[test]
    fn dump_for_pictures() {
        let Ok(dir) = std::env::var("SQUID_DUMP") else { return };
        for id in ["aori", "kouika"] {
            let sh = shape_of(id);
            let mut rig = Rig::new(species(id).unwrap(), &sh, MID);
            rig.set_length(1.0);
            let read = |v: &str| {
                let b = std::fs::read(format!("{dir}/{id}_{v}.rgba")).unwrap();
                let w = u32::from_le_bytes(b[0..4].try_into().unwrap());
                let h = u32::from_le_bytes(b[4..8].try_into().unwrap());
                (w, h, b[8..].to_vec())
            };
            let (d, v, s) = (read("dorsal"), read("ventral"), read("side"));
            let atlas = rig.bake_atlas(&sh, [(d.0, d.1, &d.2), (v.0, v.1, &v.2), (s.0, s.1, &s.2)]);
            let mut f = Vec::new();
            f.extend_from_slice(&rig.atlas_w.to_le_bytes());
            f.extend_from_slice(&rig.atlas_h.to_le_bytes());
            f.extend_from_slice(&atlas);
            std::fs::write(format!("{dir}/{id}_atlas.rgba"), f).unwrap();
            let (mut body, mut fins) = rig.new_geometries();
            let drives = [("hover", SquidDrive::default()), ("swim", SquidDrive { speed: 6.0, ..Default::default() }), ("dash", SquidDrive { speed: 18.0, dash: 1.0, ..Default::default() }),
                ("air", SquidDrive { speed: 10.0, air: true, ..Default::default() }), ("hurt", SquidDrive { speed: 2.0, hurt: 1.0, ..Default::default() }), ("joy", SquidDrive { joy: true, ..Default::default() })];
            for (name, d) in drives {
                let mut r = rig.clone();
                for i in 0..150 { r.update(1.0 / 60.0, i as f64 / 60.0, &d); }
                r.fill(&mut body, &mut fins);
                let mut o = String::new();
                for (gi, g) in [&body, &fins].into_iter().enumerate() {
                    let base = if gi == 0 { 0 } else { body.positions.len() };
                    for p in &g.positions { o += &format!("v {} {} {}\n", p[0], p[1], p[2]); }
                    for t in &g.uvs { o += &format!("vt {} {}\n", t[0], t[1]); }
                    o += &format!("g {}\n", if gi == 0 { "body" } else { "fins" });
                    for t in g.indices.as_ref().unwrap().chunks_exact(3) {
                        let (a, b, c) = (t[0] as usize + base + 1, t[1] as usize + base + 1, t[2] as usize + base + 1);
                        o += &format!("f {a}/{a} {b}/{b} {c}/{c}\n");
                    }
                }
                std::fs::write(format!("{dir}/{id}_{name}.obj"), o).unwrap();
            }
        }
    }
}
