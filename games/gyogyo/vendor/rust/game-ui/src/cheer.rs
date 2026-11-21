// うれしい瞬間の紙吹雪（画面の座標。3D の世界には置かない）。0.4（2026-11-11）。
// 自己ベスト・クリアなどの節目で、ゲームが burst を呼ぶ。毎コマ step して draw する。乱数は自前（遊びの乱数を進めない）。
// 動きを減らす設定（page::reduced_motion）のときは set_calm(true): 枚数を減らし、回さず、揺らさない（祝いそのものは出す）。
use crate::{C, Pen};
use egui::{Pos2, pos2};

// ---- 結果の画面の流れ（0.5。2026-11-11: 各ゲームの rust/sim/src/cheer.rs が同じ形で持っていた物） ----
// 主な数の数え上げ → 数え終わりの祝い（段は 3 つ）→ いちばん上の評価のときのもう 1 回。見た目（紙吹雪・弾み）と音は、
// ゲームがここの出来事を受けて出す。遊びの計算・保存・乱数には触れない。描画にもブラウザにも依らない。

/// 祝いの段（毎回同じ最大の演出にしない）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cheer {
    /// 自己ベストでない: 主な数が 1 回だけ小さく弾む
    None,
    /// 自己ベスト: 印（NEW）が弾んで出る・札の左右の下の角から紙吹雪・上向きの和音
    Best,
    /// 自己ベストで、そのゲームのいちばん上の評価: 上に加えて、少し後にもう 1 回
    BestTop,
}

/// new_best は、そのゲームが「NEW」「自己ベスト！」を出す条件。top は、いちばん上の評価（星が全部・いちばん上の格・1 着など。ゲームが決める）
pub fn cheer_of(new_best: bool, top: bool) -> Cheer {
    if !new_best { Cheer::None } else if top { Cheer::BestTop } else { Cheer::Best }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CheerEvent {
    /// 数え上げの刻み（進み 0..1。小さな音を鳴らす）
    Tick(f32),
    /// 数え終わった
    Done(Cheer),
    /// いちばん上の評価のときのもう 1 回
    Encore,
}

/// 数え上げの長さ（秒）。動きを減らす設定のときは短い
pub const COUNT_S: f64 = 0.8;
pub const COUNT_CALM_S: f64 = 0.3;
/// 刻みの間隔と、もう 1 回までの間
pub const TICK_S: f64 = 0.07;
pub const ENCORE_S: f64 = 0.35;
/// 同じ刻みを足していって境目にちょうど届くとき、丸めで 1 コマ遅れないための余裕
const EPS: f64 = 1e-9;

#[derive(Clone, Debug)]
pub struct ResultShow { total: f64, cheer: Cheer, start: f64, dur: f64, t: f64, ticks: i64, done: bool, encore: bool, cancelled: bool }

impl ResultShow {
    /// total は結果の主な数（本当の値）。calm は端末の「動きを減らす」設定（page::reduced_motion）
    pub fn new(total: i64, cheer: Cheer, calm: bool) -> ResultShow { ResultShow::new_f64(total as f64, cheer, calm) }
    /// 小数の数（タイムなど）。表示は value_f64 で読む
    pub fn new_f64(total: f64, cheer: Cheer, calm: bool) -> ResultShow {
        ResultShow { total, cheer, start: 0.0, dur: if calm { COUNT_CALM_S } else { COUNT_S }, t: 0.0, ticks: 0, done: false, encore: false, cancelled: false }
    }
    /// 数え始めを s 秒待つ（格の判など、先に出る演出を待つ）。待つ間は 0 のままで刻まない
    pub fn wait(mut self, s: f64) -> ResultShow { self.start = s.max(0.0); self }
    /// この時刻（結果が出てからの秒）より前には数え終わらない（星が灯りきる時刻に合わせる）
    pub fn end_at(mut self, s: f64) -> ResultShow { self.start = (s - self.dur).max(0.0); self }
    /// 何も起きない物（結果の画面が出ていない間）
    pub fn idle() -> ResultShow { ResultShow::idle_at(0) }
    /// 何も起きず、数ははじめから本当の値（祝いを入れない結果）
    pub fn idle_at(total: i64) -> ResultShow { ResultShow { cancelled: true, ..ResultShow::new(total, Cheer::None, false) } }
    /// 表示する数（小数）: 0 から増えるだけ（終わりへ向けてゆるむ）。数え終わり・打ち切りの後は本当の値
    pub fn value_f64(&self) -> f64 {
        if self.done || self.cancelled { return self.total; }
        let k = ((self.t - self.start) / self.dur).clamp(0.0, 1.0);
        (self.total * (1.0 - (1.0 - k).powi(3))).clamp(0.0, self.total.max(0.0))
    }
    /// 表示する数（整数。切り捨て）
    pub fn value(&self) -> i64 { if self.done || self.cancelled { self.total as i64 } else { self.value_f64().floor() as i64 } }
    /// 数えている途中か（待っている間を含む。表示が本当の値でない間）
    pub fn counting(&self) -> bool { !self.done && !self.cancelled }
    /// 数え終わったか（打ち切った後は false）
    pub fn done(&self) -> bool { self.done && !self.cancelled }
    /// 数え終わってからの秒（まだ・打ち切った後は None）
    pub fn done_for(&self) -> Option<f64> { self.done().then(|| self.t - self.start - self.dur) }
    /// まだ何か起きるか
    pub fn active(&self) -> bool { !self.cancelled && !(self.done && (self.cheer != Cheer::BestTop || self.encore)) }
    pub fn cheer(&self) -> Cheer { self.cheer }
    /// ボタンを押した: 打ち切る（数は本当の値になり、以後は何も起きない）
    pub fn cancel(&mut self) { self.cancelled = true; }
    /// dt 秒進める
    pub fn step(&mut self, dt: f64) -> Vec<CheerEvent> {
        let mut ev = Vec::new();
        if self.cancelled { return ev; }
        self.t += dt;
        let at = self.t - self.start;
        if !self.done && at > 0.0 {
            let n = ((at.min(self.dur) + EPS) / TICK_S).floor() as i64;
            while self.ticks < n {
                self.ticks += 1;
                ev.push(CheerEvent::Tick((self.ticks as f64 * TICK_S / self.dur) as f32));
            }
            if at >= self.dur - EPS {
                self.done = true;
                ev.push(CheerEvent::Done(self.cheer));
            }
        }
        if self.done && self.cheer == Cheer::BestTop && !self.encore && at >= self.dur + ENCORE_S - EPS {
            self.encore = true;
            ev.push(CheerEvent::Encore);
        }
        ev
    }
}

// ---- 紙吹雪 ----

/// 同時に出す枚数の上限（多角形 1 枚ずつ。これを超える分は出さない）
pub const MAX_PIECES: usize = 320;
const GRAVITY: f32 = 900.0;
const DRAG: f32 = 2.4;
const FADE: f32 = 0.4;

/// いちばん速い 1 枚が、出した所から px だけ上がる速さ（burst の speed に渡す）。画面の高さがゲームと向きで違うので、
/// 「札の上の端まで」のように高さで決める。重さと空気の抵抗は step と同じ（上がる高さ = v/k − g/k²·ln(1 + v·k/g) を解く）
pub fn speed_to_rise(px: f32) -> f32 {
    let rise = |v: f32| v / DRAG - GRAVITY / (DRAG * DRAG) * (1.0 + v * DRAG / GRAVITY).ln();
    let (mut lo, mut hi) = (0.0f32, 20_000.0f32);
    for _ in 0..40 { let mid = (lo + hi) * 0.5; if rise(mid) < px.max(0.0) { lo = mid; } else { hi = mid; } }
    hi
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Piece { x: f32, y: f32, vx: f32, vy: f32, rot: f32, spin: f32, flip: f32, flip_v: f32, sway: f32, sway_v: f32, w: f32, h: f32, color: u8, age: f32, life: f32 }

#[derive(Clone, Debug)]
pub struct Confetti { pieces: Vec<Piece>, seed: u32, calm: bool }

impl Default for Confetti { fn default() -> Self { Confetti { pieces: Vec::new(), seed: 0x9e37_79b9, calm: false } } }

impl Confetti {
    pub fn new() -> Self { Self::default() }
    /// 動きを減らす（枚数は 3 分の 1・回さない・揺らさない）
    pub fn set_calm(&mut self, on: bool) { self.calm = on; }
    pub fn len(&self) -> usize { self.pieces.len() }
    pub fn is_empty(&self) -> bool { self.pieces.is_empty() }
    pub fn clear(&mut self) { self.pieces.clear(); }

    fn rnd(&mut self) -> f32 {
        let mut s = self.seed;
        s ^= s << 13; s ^= s >> 17; s ^= s << 5;
        self.seed = s;
        (s >> 8) as f32 / 16_777_216.0
    }

    /// at から、dir_deg（0 が右・-90 が上）を中心に spread_deg の幅で n 枚を飛ばす。speed は px/秒（600〜1100 が目安）
    pub fn burst(&mut self, at: Pos2, dir_deg: f32, spread_deg: f32, n: usize, speed: f32) {
        let n = if self.calm { n.div_ceil(3) } else { n };
        let calm = self.calm;
        for _ in 0..n.min(MAX_PIECES.saturating_sub(self.pieces.len())) {
            let a = (dir_deg + (self.rnd() - 0.5) * spread_deg).to_radians();
            let v = speed * (0.45 + 0.55 * self.rnd()) * if calm { 0.6 } else { 1.0 };
            let (spin, flip_v, sway_v) = if calm { (0.0, 0.0, 0.0) } else { ((self.rnd() - 0.5) * 14.0, 5.0 + self.rnd() * 9.0, 2.0 + self.rnd() * 3.0) };
            let p = Piece {
                x: at.x, y: at.y, vx: a.cos() * v, vy: a.sin() * v,
                rot: self.rnd() * std::f32::consts::TAU, spin,
                // 回さないときは面を正面に向ける（横を向いたまま細い線で止まらない）
                flip: if calm { 0.0 } else { self.rnd() * std::f32::consts::TAU }, flip_v,
                sway: self.rnd() * std::f32::consts::TAU, sway_v,
                w: 7.0 + self.rnd() * 6.0, h: 10.0 + self.rnd() * 8.0,
                color: (self.rnd() * 255.0) as u8, age: 0.0, life: 1.6 + self.rnd() * 1.4,
            };
            self.pieces.push(p);
        }
    }

    /// from から to をねらって n 枚を飛ばす（いちばん速い 1 枚が to の高さのあたりまで上がる）。札の左右の下の角から
    /// 札の上の辺の真ん中へ、のように場所で決める（縦長でも横長でも真ん中へ届く）。to は from より上
    pub fn burst_to(&mut self, from: Pos2, to: Pos2, spread_deg: f32, n: usize) {
        let a = (to.y - from.y).atan2(to.x - from.x);
        self.burst(from, a.to_degrees(), spread_deg, n, speed_to_rise((from.y - to.y).max(0.0)) / a.sin().abs().max(0.5));
    }

    /// 札の左右の下の角（inset だけ内へ寄せた所）から、札の上の辺の真ん中をねらって、左右 n 枚ずつ飛ばす（burst_to を 2 回）
    pub fn burst_corners(&mut self, card: egui::Rect, inset: f32, spread_deg: f32, n: usize) {
        let to = pos2(card.center().x, card.top());
        self.burst_to(pos2(card.left() + inset, card.bottom() - inset), to, spread_deg, n);
        self.burst_to(pos2(card.right() - inset, card.bottom() - inset), to, spread_deg, n);
    }

    /// 1 コマ進める。floor は画面の下の端（論理 px）。そこを過ぎた物と、時間の切れた物は消える
    pub fn step(&mut self, dt: f32, floor: f32) {
        let dt = dt.clamp(0.0, 0.05);
        let k = (-DRAG * dt).exp();
        for p in &mut self.pieces {
            p.vy += GRAVITY * dt;
            p.vx *= k; p.vy *= k;
            p.sway += p.sway_v * dt;
            p.x += (p.vx + p.sway.sin() * 40.0 * p.sway_v.min(1.0)) * dt;
            p.y += p.vy * dt;
            p.rot += p.spin * dt;
            p.flip += p.flip_v * dt;
            p.age += dt;
        }
        self.pieces.retain(|p| p.age < p.life && p.y < floor + 24.0);
    }

    /// 描く。色は palette を順に使う（空なら描かない）
    pub fn draw(&self, pen: &Pen, palette: &[C]) {
        if palette.is_empty() { return; }
        for p in &self.pieces {
            let turn = p.flip.cos();
            let c = palette[p.color as usize % palette.len()];
            let c = if turn < 0.0 { c.bright(0.78) } else { c }.alpha(((p.life - p.age) / FADE).clamp(0.0, 1.0));
            let (s, co) = p.rot.sin_cos();
            let (hw, hh) = (p.w * 0.5, p.h * 0.5 * turn.abs().max(0.12));
            let at = |dx: f32, dy: f32| pos2(p.x + dx * co - dy * s, p.y + dx * s + dy * co);
            pen.poly(&[at(-hw, -hh), at(hw, -hh), at(hw, hh), at(-hw, hh)], c);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_is_capped_and_repeatable() {
        let mut a = Confetti::new();
        let mut b = Confetti::new();
        a.burst(pos2(100.0, 400.0), -90.0, 60.0, 80, 900.0);
        b.burst(pos2(100.0, 400.0), -90.0, 60.0, 80, 900.0);
        assert_eq!(a.len(), 80);
        assert_eq!(a.pieces, b.pieces, "同じ呼び方なら同じ並び");
        a.burst(pos2(0.0, 0.0), -90.0, 60.0, 1000, 900.0);
        assert_eq!(a.len(), MAX_PIECES);
    }

    #[test]
    fn pieces_rise_then_fall_and_all_leave() {
        let mut c = Confetti::new();
        c.burst(pos2(200.0, 600.0), -90.0, 40.0, 60, 1000.0);
        let top = |c: &Confetti| c.pieces.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        for _ in 0..12 { c.step(1.0 / 60.0, 800.0); }
        assert!(top(&c) < 520.0, "上へ飛ぶ: {}", top(&c));
        for _ in 0..600 { c.step(1.0 / 60.0, 800.0); }
        assert!(c.is_empty(), "5 秒より長くは残らない: {}", c.len());
    }

    #[test]
    fn speed_to_rise_reaches_the_height() {
        for px in [200.0f32, 500.0, 800.0] {
            let mut c = Confetti::new();
            c.burst(pos2(0.0, 0.0), -90.0, 0.0, 300, speed_to_rise(px));
            let mut top = 0.0f32;
            for _ in 0..360 { c.step(1.0 / 120.0, 10_000.0); top = top.min(c.pieces.iter().map(|p| p.y).fold(0.0, f32::min)); }
            assert!((-top - px).abs() < px * 0.05, "{px}px のつもりが {}px", -top);
        }
    }

    #[test]
    fn burst_to_reaches_the_target_height() {
        // 札の左下の角から、札の上の辺の真ん中へ: いちばん速い 1 枚が、ねらった高さのあたりまで上がり、内側へ進む
        for (from, to) in [(pos2(20.0, 600.0), pos2(195.0, 200.0)), (pos2(120.0, 330.0), pos2(420.0, 60.0))] {
            let mut c = Confetti::new();
            c.burst_to(from, to, 0.0, 300);
            let (mut top, mut at_top_x) = (from.y, from.x);
            for _ in 0..480 { c.step(1.0 / 120.0, 10_000.0); for p in &c.pieces { if p.y < top { top = p.y; at_top_x = p.x; } } }
            let rise = from.y - to.y;
            assert!((from.y - top - rise).abs() < rise * 0.08, "{rise}px のつもりが {}px", from.y - top);
            assert!(at_top_x > from.x, "内側（ねらった側）へ進む");
        }
    }

    #[test]
    fn burst_corners_sends_both_sides_inward() {
        let card = egui::Rect::from_min_max(pos2(100.0, 100.0), pos2(500.0, 400.0));
        let mut c = Confetti::new();
        c.burst_corners(card, 10.0, 0.0, 40);
        assert_eq!(c.len(), 80);
        let (left, right): (Vec<&Piece>, Vec<&Piece>) = c.pieces.iter().partition(|p| p.x < 300.0);
        assert_eq!((left.len(), right.len()), (40, 40));
        assert!(left.iter().all(|p| p.x == 110.0 && p.y == 390.0 && p.vx > 0.0 && p.vy < 0.0));
        assert!(right.iter().all(|p| p.x == 490.0 && p.y == 390.0 && p.vx < 0.0 && p.vy < 0.0));
    }

    #[test]
    fn calm_has_fewer_pieces_and_no_spin() {
        let mut c = Confetti::new();
        c.set_calm(true);
        c.burst(pos2(0.0, 0.0), -90.0, 60.0, 90, 900.0);
        assert_eq!(c.len(), 30);
        assert!(c.pieces.iter().all(|p| p.spin == 0.0 && p.flip_v == 0.0 && p.sway_v == 0.0));
        let x0: Vec<f32> = c.pieces.iter().map(|p| p.rot).collect();
        c.step(0.016, 800.0);
        assert!(c.pieces.iter().zip(&x0).all(|(p, r)| p.rot == *r));
    }
}

#[cfg(test)]
mod show_tests {
    use super::*;

    fn run(show: &mut ResultShow, seconds: f64) -> Vec<(f64, CheerEvent)> {
        let (dt, mut t, mut out) = (1.0 / 60.0, 0.0, vec![]);
        while t < seconds { t += dt; for e in show.step(dt) { out.push((t, e)); } }
        out
    }
    fn done_at(ev: &[(f64, CheerEvent)]) -> f64 { ev.iter().find(|(_, e)| matches!(e, CheerEvent::Done(_))).map(|(t, _)| *t).expect("数え終わる") }

    #[test]
    fn tiers() {
        assert_eq!(cheer_of(false, false), Cheer::None);
        assert_eq!(cheer_of(false, true), Cheer::None);
        assert_eq!(cheer_of(true, false), Cheer::Best);
        assert_eq!(cheer_of(true, true), Cheer::BestTop);
    }

    #[test]
    fn counts_up_and_ends_on_the_real_value() {
        for total in [0i64, 7, 42, 7_750, 123_456] {
            let mut s = ResultShow::new(total, Cheer::None, false);
            assert_eq!(s.value(), 0);
            let (mut last, mut t) = (0, 0.0);
            while t < 0.76 {
                s.step(1.0 / 60.0);
                t += 1.0 / 60.0;
                assert!(s.value() >= last && s.value() <= total);
                last = s.value();
            }
            if total > 100 { assert!(last < total && s.counting() && !s.done()); }
            s.step(0.05);
            assert_eq!(s.value(), total);
            assert!(s.done_for().is_some() && s.done() && !s.counting());
        }
    }

    #[test]
    fn top_tier_gets_one_encore() {
        let ev = run(&mut ResultShow::new(7_750, Cheer::BestTop, false), 2.0);
        let ticks = ev.iter().filter(|(_, e)| matches!(e, CheerEvent::Tick(_))).count();
        assert_eq!(ticks, 11);
        let done = done_at(&ev);
        let encore: Vec<f64> = ev.iter().filter(|(_, e)| *e == CheerEvent::Encore).map(|(t, _)| *t).collect();
        assert_eq!(encore.len(), 1);
        assert!((done - 0.8).abs() < 0.03 && (encore[0] - done - 0.35).abs() < 0.03);
        assert!(ev.iter().all(|(t, e)| !matches!(e, CheerEvent::Tick(_)) || *t <= done));
        for level in [Cheer::Best, Cheer::None] {
            let ev = run(&mut ResultShow::new(7_750, level, false), 2.0);
            assert_eq!(ev.iter().filter(|(_, e)| *e == CheerEvent::Done(level)).count(), 1);
            assert!(!ev.iter().any(|(_, e)| *e == CheerEvent::Encore));
        }
    }

    #[test]
    fn calm_counts_faster() {
        let ev = run(&mut ResultShow::new(7_750, Cheer::Best, true), 1.0);
        assert!((done_at(&ev) - 0.3).abs() < 0.03);
        assert_eq!(ev.iter().filter(|(_, e)| matches!(e, CheerEvent::Tick(_))).count(), 4);
    }

    #[test]
    fn cancel_stops_everything() {
        let mut s = ResultShow::new(7_750, Cheer::BestTop, false);
        run(&mut s, 0.4);
        assert!(s.value() < 7_750 && s.active());
        s.cancel();
        assert_eq!(s.value(), 7_750);
        assert!(!s.active() && !s.counting() && s.done_for().is_none());
        assert!(run(&mut s, 2.0).is_empty());
        let mut s = ResultShow::new(7_750, Cheer::BestTop, false);
        run(&mut s, 0.9);
        s.cancel();
        assert!(run(&mut s, 2.0).is_empty());
        assert!(run(&mut ResultShow::idle(), 2.0).is_empty());
        // 何も起きない物でも、数は本当の値を見せられる（時間切れの結果など）
        let mut s = ResultShow::idle_at(4_200);
        assert_eq!(s.value(), 4_200);
        assert!(run(&mut s, 2.0).is_empty());
    }

    /// 数え始めを待つ（格の判・星が出きるのを待つ）: 待つ間は 0 のままで刻まない。数え終わりは 待ち + 数え上げ
    #[test]
    fn wait_delays_the_count() {
        let mut s = ResultShow::new(7_750, Cheer::Best, false).wait(0.9);
        let ev = run(&mut s, 0.85);
        assert!(ev.is_empty() && s.value() == 0 && s.counting());
        let ev = run(&mut s, 2.0);
        assert!((done_at(&ev) + 0.85 - 1.7).abs() < 0.04);
        assert_eq!(ev.iter().filter(|(_, e)| matches!(e, CheerEvent::Tick(_))).count(), 11);
        // 動きを減らす設定でも待ちは同じ
        assert!((done_at(&run(&mut ResultShow::new(7_750, Cheer::Best, true).wait(0.9), 3.0)) - 1.2).abs() < 0.03);
    }

    /// この時刻より前には数え終わらない（星が灯りきる時刻に合わせる）。数え上げより短い時刻なら、すぐ数え始める
    #[test]
    fn end_at_ends_no_earlier() {
        assert!((done_at(&run(&mut ResultShow::new(7_750, Cheer::Best, false).end_at(1.71), 3.0)) - 1.71).abs() < 0.03);
        assert!((done_at(&run(&mut ResultShow::new(7_750, Cheer::Best, true).end_at(1.71), 3.0)) - 1.71).abs() < 0.03);
        assert!((done_at(&run(&mut ResultShow::new(7_750, Cheer::Best, false).end_at(0.2), 3.0)) - 0.8).abs() < 0.03);
        assert!((done_at(&run(&mut ResultShow::new(7_750, Cheer::Best, false).end_at(0.0), 3.0)) - 0.8).abs() < 0.03);
    }

    /// 小数の数（タイム）: 切り捨てずに数え上がり、終わりはちょうど本当の値。同じ刻みを足して数え終わりに届く
    #[test]
    fn fractional_totals() {
        let mut s = ResultShow::new_f64(86.45, Cheer::Best, false);
        let (mut last, mut seen_fraction) = (0.0, false);
        for _ in 0..47 {
            s.step(1.0 / 60.0);
            let v = s.value_f64();
            assert!(v >= last && v <= 86.45);
            if v.fract() != 0.0 { seen_fraction = true; }
            last = v;
        }
        assert!(seen_fraction && s.counting());
        let mut ev = vec![];
        for _ in 0..2 { ev.extend(s.step(1.0 / 60.0)); }
        assert!(ev.contains(&CheerEvent::Done(Cheer::Best)), "0.8 秒ちょうどの刻みで数え終わる");
        assert_eq!(s.value_f64(), 86.45);
    }
}
