// 主役の技の表。時刻は技の始まりからの秒。当たり（hits）・踏み込み（lunge）・体の回転（spin）・跳躍（hop）と、
// 描画が使う斬撃の弧（arcs）・体の動きのクリップ（anim）を一緒に持つ（当たりと見た目がずれないように）。
use crate::math::{self, PI};
use std::sync::LazyLock;

/// Gather: 主役の正面へ寄せる（群衆ドミノの弾込め）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Knock { Push, Gather, Launch, Spin, Down, Blast }

/// Arc: 正面の扇 / Circle: 自分のまわり / Line: 正面の帯
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitShape { Arc, Circle, Line }
impl HitShape { pub fn name(self) -> &'static str { match self { HitShape::Arc => "arc", HitShape::Circle => "circle", HitShape::Line => "line" } } }

/// 当たりの見せ方
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitFx { Slash, Heavy, Ring, Wave, Blast, Tornado }
impl HitFx { pub fn name(self) -> &'static str { match self { HitFx::Slash => "slash", HitFx::Heavy => "heavy", HitFx::Ring => "ring", HitFx::Wave => "wave", HitFx::Blast => "blast", HitFx::Tornado => "tornado" } } }

#[derive(Clone, Copy, Debug)]
pub struct Hit {
    pub t: f64,
    /// arc: 正面の扇 / circle: 自分のまわり / line: 正面の帯
    pub shape: HitShape,
    pub r: f64,
    /// 扇の半分の角度（rad）
    pub arc: Option<f64>,
    /// 帯の幅
    pub w: Option<f64>,
    /// 前へずらす距離
    pub off: Option<f64>,
    pub dmg: f64,
    pub kb: Knock,
    /// 吹き飛ばしの強さ（m/s）。launch は上向き
    pub power: f64,
    /// ヒットストップ（秒）
    pub stop: f64,
    /// 画面の揺れ（0〜1）
    pub shake: f64,
    /// 当たりの見せ方
    pub fx: HitFx,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ArcKind { Slash, Heavy, Spin, Ranbu }

/// 斬撃の弧: tilt は剣の軌跡の面の傾き（0 = 水平、正で右上がり）、from→to は正面からの角度（rad、左が正）、r は半径、y は高さ
#[derive(Clone, Copy, Debug)]
pub struct Arc { pub t: f64, pub tilt: f64, pub from: f64, pub to: f64, pub r: f64, pub y: f64, pub dur: f64, pub width: f64, pub kind: ArcKind }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AnimClip { Swing, Skill, Ready }

#[derive(Clone, Copy, Debug)]
pub struct Anim {
    pub clip: AnimClip, pub from: f64, pub to: f64,
    /// 左右を反転して見せる（体の向きで返す）
    pub yaw: Option<f64>,
}

#[derive(Debug)]
pub struct Move {
    pub id: &'static str,
    pub name: &'static str,
    pub dur: f64,
    /// この時刻を過ぎたら次の技へつなげる（先行入力はいつでも受ける）
    pub cancel: f64,
    /// 踏み込み: [始め, 終わり, 距離]
    pub lunge: Vec<[f64; 3]>,
    /// 体の回転: [始め, 終わり, 角度 rad]
    pub spin: Vec<[f64; 3]>,
    /// 跳躍: [始め, 終わり, 高さ]。sin の山
    pub hop: Option<[f64; 3]>,
    pub hits: Vec<Hit>,
    pub arcs: Vec<Arc>,
    pub anim: Anim,
    /// 入力の方向へ向き直れる時間
    pub steer: f64,
    /// この技の間は無敵
    pub armor: bool,
    pub invuln: bool,
}

const D: f64 = PI / 180.0;

/// slash の既定（o: Partial<Hit> の代わりに、呼ぶ側が書き換える）
fn slash(t: f64) -> Hit { Hit { t, shape: HitShape::Arc, r: 3.6, arc: Some(70.0 * D), w: None, off: None, dmg: 42.0, kb: Knock::Push, power: 5.0, stop: 0.045, shake: 0.12, fx: HitFx::Slash } }
/// 弧の既定（tilt・from・to は度で渡す）
fn arc(t: f64, tilt: f64, from: f64, to: f64) -> Arc { Arc { t, tilt: tilt * D, from: from * D, to: to * D, r: 3.0, y: 1.0, dur: 0.16, width: 1.0, kind: ArcKind::Slash } }
fn arc_o(t: f64, tilt: f64, from: f64, to: f64, o: impl FnOnce(&mut Arc)) -> Arc { let mut a = arc(t, tilt, from, to); o(&mut a); a }
fn anim(clip: AnimClip, from: f64, to: f64) -> Anim { Anim { clip, from, to, yaw: None } }
fn swing(from: f64, to: f64) -> Anim { anim(AnimClip::Swing, from, to) }
fn hit(t: f64, shape: HitShape, r: f64, dmg: f64, kb: Knock, power: f64, stop: f64, shake: f64, fx: HitFx) -> Hit { Hit { t, shape, r, arc: None, w: None, off: None, dmg, kb, power, stop, shake, fx } }

/// 通常攻撃（最大 6 段）。1〜5 段は敵を正面へ寄せて固め、6 段で吹き飛ばす
fn gather(t: f64, o: impl FnOnce(&mut Hit)) -> Hit { let mut h = slash(t); h.kb = Knock::Gather; o(&mut h); h.dmg = math::round(h.dmg * 0.5); h }

fn base(id: &'static str, name: &'static str, dur: f64, cancel: f64, steer: f64, anim: Anim) -> Move {
    Move { id, name, dur, cancel, lunge: vec![], spin: vec![], hop: None, hits: vec![], arcs: vec![], anim, steer, armor: false, invuln: false }
}

pub static NORMALS: LazyLock<Vec<Move>> = LazyLock::new(|| vec![
    Move { lunge: vec![[0.02, 0.14, 1.0]], hits: vec![gather(0.12, |_| {})], arcs: vec![arc(0.08, 18.0, 80.0, -70.0)], ..base("n1", "一の太刀", 0.4, 0.2, 0.1, swing(0.42, 0.72)) },
    Move { lunge: vec![[0.02, 0.14, 1.0]], hits: vec![gather(0.12, |_| {})], arcs: vec![arc(0.08, 165.0, 80.0, -75.0)], spin: vec![[0.0, 0.16, 0.5]], ..base("n2", "二の太刀", 0.4, 0.2, 0.1, swing(0.42, 0.72)) },
    Move { lunge: vec![[0.02, 0.16, 1.4]], hits: vec![gather(0.14, |h| { h.r = 3.9; h.arc = Some(80.0 * D); h.dmg = 46.0; })],
        arcs: vec![arc_o(0.09, -25.0, -85.0, 85.0, |a| a.r = 3.3)], spin: vec![[0.0, 0.18, -0.6]], ..base("n3", "三の太刀", 0.44, 0.22, 0.1, swing(0.4, 0.74)) },
    Move { lunge: vec![[0.02, 0.18, 1.2]], hits: vec![gather(0.15, |h| { h.r = 4.0; h.arc = Some(100.0 * D); h.dmg = 50.0; })],
        arcs: vec![arc_o(0.1, 8.0, 110.0, -110.0, |a| { a.r = 3.5; a.width = 1.2; })], ..base("n4", "四の太刀", 0.46, 0.24, 0.1, swing(0.4, 0.76)) },
    Move { lunge: vec![[0.02, 0.24, 1.6]],
        hits: vec![gather(0.13, |h| h.dmg = 34.0), gather(0.25, |h| { h.dmg = 34.0; h.arc = Some(90.0 * D); })],
        arcs: vec![arc(0.08, 35.0, 70.0, -70.0), arc(0.2, 150.0, 70.0, -80.0)], spin: vec![[0.12, 0.26, 0.7]], ..base("n5", "五の太刀", 0.5, 0.3, 0.1, swing(0.35, 0.8)) },
    Move { lunge: vec![[0.05, 0.3, 2.2]],
        hits: vec![{ let mut h = slash(0.28); h.r = 4.8; h.arc = Some(160.0 * D); h.dmg = 85.0; h.kb = Knock::Blast; h.power = 11.0; h.stop = 0.1; h.shake = 0.5; h.fx = HitFx::Heavy; h }],
        arcs: vec![arc_o(0.22, -8.0, 160.0, -160.0, |a| { a.r = 4.2; a.width = 1.7; a.dur = 0.22; a.kind = ArcKind::Heavy; })], spin: vec![[0.12, 0.32, -1.2]], ..base("n6", "六の太刀・断", 0.68, 0.5, 0.14, swing(0.3, 0.95)) },
]);

/// 強攻撃（通常攻撃を n 回つないだあと。index = n）
pub static CHARGES: LazyLock<Vec<Move>> = LazyLock::new(|| vec![
    Move { lunge: vec![[0.08, 0.3, 6.5]], armor: true,
        hits: vec![Hit { w: Some(2.4), off: Some(-1.5), ..hit(0.16, HitShape::Line, 6.5, 90.0, Knock::Blast, 12.0, 0.07, 0.35, HitFx::Heavy) }],
        arcs: vec![arc_o(0.1, 90.0, 6.0, -6.0, |a| { a.r = 3.8; a.width = 0.8; a.dur = 0.24; a.kind = ArcKind::Heavy; })], ..base("c0", "迅雷突き", 0.66, 0.5, 0.12, swing(0.5, 0.85)) },
    Move { lunge: vec![[0.02, 0.15, 0.8]], hop: Some([0.12, 0.55, 1.4]), armor: true,
        hits: vec![Hit { arc: Some(85.0 * D), ..hit(0.17, HitShape::Arc, 4.0, 70.0, Knock::Launch, 12.0, 0.08, 0.3, HitFx::Heavy) }],
        arcs: vec![arc_o(0.12, 92.0, -40.0, 140.0, |a| { a.r = 3.4; a.width = 1.3; a.dur = 0.22; a.kind = ArcKind::Heavy; a.y = 1.4; })], ..base("c1", "昇り龍", 0.66, 0.42, 0.1, swing(0.48, 0.9)) },
    Move { spin: vec![[0.08, 0.56, PI * 2.0]], lunge: vec![[0.08, 0.5, 1.5]], armor: true,
        hits: [0.2, 0.34, 0.48].iter().enumerate().map(|(i, &t)| hit(t, HitShape::Circle, 4.4, 42.0, if i == 2 { Knock::Blast } else { Knock::Spin }, if i == 2 { 9.0 } else { 4.0 }, if i == 2 { 0.08 } else { 0.035 }, if i == 2 { 0.4 } else { 0.15 }, HitFx::Slash)).collect(),
        arcs: vec![arc_o(0.1, 4.0, 180.0, -180.0, |a| { a.r = 3.8; a.width = 1.4; a.dur = 0.46; a.kind = ArcKind::Spin; })], ..base("c2", "旋風斬", 0.78, 0.62, 0.08, swing(0.45, 0.8)) },
    Move { lunge: vec![[0.05, 0.2, 1.2]], armor: true,
        hits: vec![Hit { w: Some(3.4), off: Some(0.5), ..hit(0.24, HitShape::Line, 14.0, 110.0, Knock::Blast, 13.0, 0.09, 0.55, HitFx::Wave) }],
        arcs: vec![arc_o(0.18, -82.0, 30.0, -30.0, |a| { a.r = 4.0; a.width = 1.4; a.dur = 0.2; a.kind = ArcKind::Heavy; })], ..base("c3", "烈波", 0.82, 0.62, 0.14, swing(0.42, 0.88)) },
    Move { hop: Some([0.0, 0.46, 2.6]), lunge: vec![[0.02, 0.4, 2.4]], armor: true,
        hits: vec![hit(0.46, HitShape::Circle, 7.2, 120.0, Knock::Launch, 10.0, 0.12, 0.85, HitFx::Ring)],
        arcs: vec![arc_o(0.34, -88.0, 60.0, -60.0, |a| { a.r = 4.0; a.width = 1.6; a.dur = 0.16; a.kind = ArcKind::Heavy; a.y = 1.8; })], ..base("c4", "地裂", 0.95, 0.72, 0.16, swing(0.3, 0.92)) },
    Move { spin: vec![[0.06, 1.0, PI * 6.0]], lunge: vec![[0.06, 1.0, 5.0]], armor: true,
        hits: [0.14, 0.24, 0.34, 0.44, 0.54, 0.64, 0.74, 0.84, 0.94].iter().map(|&t| hit(t, HitShape::Circle, 4.6, 24.0, Knock::Spin, 3.0, 0.02, 0.1, HitFx::Tornado))
            .chain(std::iter::once(hit(1.04, HitShape::Circle, 6.2, 90.0, Knock::Blast, 12.0, 0.1, 0.6, HitFx::Blast))).collect(),
        arcs: vec![arc_o(0.08, 10.0, 180.0, -180.0, |a| { a.r = 3.8; a.width = 1.5; a.dur = 0.9; a.kind = ArcKind::Spin; }), arc_o(0.98, -10.0, 180.0, -180.0, |a| { a.r = 5.0; a.width = 2.0; a.dur = 0.2; a.kind = ArcKind::Heavy; })],
        ..base("c5", "天嵐", 1.3, 1.1, 0.9, swing(0.4, 0.9)) },
    Move { hop: Some([0.1, 0.75, 3.2]), lunge: vec![[0.1, 0.7, 2.0]], spin: vec![[0.1, 0.62, -PI * 2.0]], armor: true,
        hits: vec![hit(0.3, HitShape::Circle, 4.5, 50.0, Knock::Launch, 9.0, 0.04, 0.2, HitFx::Slash),
            hit(0.5, HitShape::Circle, 4.5, 50.0, Knock::Launch, 7.0, 0.04, 0.2, HitFx::Slash),
            hit(0.76, HitShape::Circle, 9.0, 210.0, Knock::Blast, 15.0, 0.16, 1.0, HitFx::Blast)],
        arcs: vec![arc_o(0.2, 30.0, 180.0, -180.0, |a| { a.r = 3.8; a.width = 1.4; a.dur = 0.4; a.kind = ArcKind::Spin; a.y = 2.4; }), arc_o(0.66, -86.0, 70.0, -70.0, |a| { a.r = 4.6; a.width = 2.0; a.dur = 0.16; a.kind = ArcKind::Heavy; a.y = 2.0; })],
        ..base("c6", "百花・終の型", 1.35, 1.15, 0.2, anim(AnimClip::Skill, 0.45, 0.95)) },
]);

pub static JUMP_ATTACK: LazyLock<Move> = LazyLock::new(|| Move {
    hits: vec![{ let mut h = slash(0.1); h.r = 3.6; h.arc = Some(120.0 * D); h.kb = Knock::Down; h.power = 4.0; h }],
    arcs: vec![arc_o(0.06, -30.0, 100.0, -100.0, |a| a.r = 3.2)], ..base("ja", "飛燕", 0.36, 0.24, 0.1, swing(0.45, 0.75)) });
pub static JUMP_CHARGE: LazyLock<Move> = LazyLock::new(|| Move { armor: true,
    hits: vec![hit(0.22, HitShape::Circle, 6.4, 110.0, Knock::Launch, 9.0, 0.11, 0.8, HitFx::Ring)],
    arcs: vec![arc_o(0.12, -88.0, 50.0, -50.0, |a| { a.r = 3.6; a.width = 1.5; a.dur = 0.14; a.kind = ArcKind::Heavy; a.y = 1.6; })], ..base("jc", "墜星", 0.62, 0.5, 0.06, swing(0.55, 0.92)) });
pub static DASH_ATTACK: LazyLock<Move> = LazyLock::new(|| Move { lunge: vec![[0.0, 0.26, 4.0]], spin: vec![[0.02, 0.3, PI * 2.0]],
    hits: vec![hit(0.12, HitShape::Circle, 3.8, 50.0, Knock::Spin, 6.0, 0.04, 0.2, HitFx::Slash), hit(0.24, HitShape::Circle, 3.8, 50.0, Knock::Blast, 8.0, 0.05, 0.25, HitFx::Slash)],
    arcs: vec![arc_o(0.02, 6.0, 180.0, -180.0, |a| { a.r = 3.3; a.width = 1.2; a.dur = 0.28; a.kind = ArcKind::Spin; })], ..base("da", "疾風", 0.5, 0.34, 0.08, swing(0.5, 0.85)) });

/// 担いだ重騎を投げる大車輪: 跳んで2回転振り回し、HURL_RELEASE 秒で放つ。振り回しはまわりの敵を押し払う（投げる先の群れは残す）
pub const HURL_RELEASE: f64 = 0.36;
pub static HURL: LazyLock<Move> = LazyLock::new(|| Move { spin: vec![[0.0, HURL_RELEASE, PI * 4.0]], hop: Some([0.02, 0.44, 1.1]),
    hits: vec![hit(0.18, HitShape::Circle, 3.0, 60.0, Knock::Push, 10.0, 0.03, 0.3, HitFx::Slash)],
    arcs: vec![arc_o(0.0, 4.0, 180.0, -180.0, |a| { a.r = 3.2; a.width = 1.8; a.dur = 0.18; a.kind = ArcKind::Heavy; a.y = 2.0; }), arc_o(0.18, -6.0, 180.0, -180.0, |a| { a.r = 3.4; a.width = 1.8; a.dur = 0.18; a.kind = ArcKind::Heavy; a.y = 2.0; })],
    armor: true, invuln: true, ..base("hurl", "大車輪", 0.62, 0.56, 0.3, swing(0.5, 0.85)) });

/// 技の始まりから t 秒での体の回転（rad）
pub fn spin_at(m: &Move, t: f64) -> f64 {
    let mut v = 0.0;
    for &[a, b, d] in &m.spin { v += d * math::max(0.0, math::min(1.0, (t - a) / (b - a))); }
    v
}

/// 乱舞（ゲージ技）。始めの CUTIN 秒は切り込みの演出（敵も止まる）
pub const RANBU_CUTIN: f64 = 0.9;
pub static RANBU: LazyLock<Move> = LazyLock::new(|| {
    let mut hits = vec![];
    let mut arcs = vec![];
    let t0 = RANBU_CUTIN + 0.1;
    for i in 0..16 {
        let t = t0 + i as f64 * 0.12;
        hits.push(hit(t, HitShape::Circle, 6.5, 38.0, if i % 4 == 3 { Knock::Launch } else { Knock::Spin }, if i % 4 == 3 { 7.0 } else { 3.0 }, 0.015, 0.18, HitFx::Slash));
        let tilt = [20.0, 160.0, -30.0, 100.0, 60.0, 200.0, -70.0, 135.0][i % 8];
        arcs.push(arc_o(t - 0.04, tilt, 180.0, -180.0, |a| { a.r = 4.0 + (i % 3) as f64 * 0.6; a.width = 1.4; a.dur = 0.18; a.kind = ArcKind::Ranbu; a.y = 1.0 + (i % 3) as f64 * 0.5; }));
    }
    let tf = t0 + 16.0 * 0.12 + 0.28;
    hits.push(hit(tf, HitShape::Circle, 12.0, 420.0, Knock::Blast, 18.0, 0.2, 1.0, HitFx::Blast));
    arcs.push(arc_o(tf - 0.1, -88.0, 80.0, -80.0, |a| { a.r = 5.5; a.width = 2.6; a.dur = 0.2; a.kind = ArcKind::Ranbu; a.y = 2.2; }));
    Move { lunge: vec![[t0, t0 + 1.9, 4.0]], spin: vec![[t0, t0 + 1.9, PI * 8.0]], hop: Some([tf - 0.42, tf, 2.4]), hits, arcs, invuln: true, armor: true,
        ..base("ranbu", "乱舞", tf + 0.6, tf + 0.5, tf, anim(AnimClip::Skill, 0.0, 1.0)) }
});

pub fn all_moves() -> Vec<&'static Move> {
    let mut v: Vec<&'static Move> = NORMALS.iter().chain(CHARGES.iter()).collect();
    v.extend([&*JUMP_ATTACK, &*JUMP_CHARGE, &*DASH_ATTACK, &*RANBU]);
    v
}

/// 同じ技か（JS の === と同じ、表の中の同じ行か）
#[inline] pub fn same(a: &Move, b: &Move) -> bool { std::ptr::eq(a, b) }
#[inline] pub fn is_normal(m: &Move) -> bool { NORMALS.iter().any(|n| same(n, m)) }
#[inline] pub fn is_charge(m: &Move) -> bool { CHARGES.iter().any(|n| same(n, m)) }
