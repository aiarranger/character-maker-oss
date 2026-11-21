// コース: 内柵の線（閉じた曲線）を 1m 刻みに直した表。走者は s（内柵に沿った距離）と d（内柵からの距離）で動く。src/sim/track.ts の写し。
// d が増えるほど外側。カーブでは外側ほど同じ s を進むのに長い距離を走る（固定の内側ボーナスは付けない。企画 05）。
use crate::pow_table::{OVAL_HILL, OVAL_TURF, OVAL_WATER};
use sim_math::{atan2, hypot, round};

/// 内外の距離差の効き（1 で幾何のまま）。外へ一人分出る手が釣り合うように調整
pub const LANE_COST: f64 = 0.42;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObstacleKind { Hurdle, Water }
impl ObstacleKind {
    pub fn name(self) -> &'static str { match self { ObstacleKind::Hurdle => "hurdle", ObstacleKind::Water => "water" } }
}

#[derive(Clone, Copy, Debug)]
pub struct Obstacle {
    pub kind: ObstacleKind,
    /// 周回の中の位置（内柵に沿った距離）
    pub s: f64,
    /// 進む向きの長さ
    pub len: f64,
    /// 塞ぐ幅（内柵からの距離）
    pub d0: f64,
    pub d1: f64,
}

#[derive(Clone, Debug)]
pub struct CourseDef {
    pub id: &'static str,
    pub name: &'static str,
    /// コースの一言（選ぶ画面）
    pub blurb: &'static str,
    /// 内柵の線の制御点（反時計回り。上から見て）
    pub points: Vec<[f64; 2]>,
    /// 走路の幅
    pub width: f64,
    pub laps: i32,
    /// スタート地点はゴールのどれだけ手前か
    pub start_back: f64,
    /// ホームストレッチの中のゴールの位置（1周の中の割合）
    pub finish_at: f64,
    pub obstacles: Vec<Obstacle>,
    /// 高低差 [周回の割合, 高さ m]（なければ平ら）
    pub hills: Option<Vec<[f64; 2]>>,
    /// 練習用: CPU の密度を下げる
    pub gentle: bool,
}

/// 角を丸めた楕円（競馬場の形）の制御点。半径 rx・rz。式は track.ts の superOval で、Math の結果は表（pow_table.rs）
fn super_oval(rx: f64, rz: f64, table: &[[u64; 3]]) -> Vec<[f64; 2]> {
    table.iter().map(|r| { let k = f64::from_bits(r[2]); [f64::from_bits(r[0]) * rx * k, f64::from_bits(r[1]) * rz * k] }).collect()
}

pub fn courses() -> Vec<CourseDef> {
    let ob = |kind, s, len, d0, d1| Obstacle { kind, s, len, d0, d1 };
    use ObstacleKind::*;
    vec![
        CourseDef {
            id: "turf", name: "はじまり芝コース", blurb: "広い楕円。障害なし。位置取りとダッシュの使いどころ",
            points: super_oval(122.0, 56.0, &OVAL_TURF), width: 12.0, laps: 2, start_back: 46.0, finish_at: 0.86, obstacles: vec![], hills: None, gentle: false,
        },
        CourseDef {
            id: "water", name: "みずべ障害コース", blurb: "ハードルと水たまり。跳ぶか、外へ回るか",
            points: super_oval(116.0, 60.0, &OVAL_WATER), width: 13.0, laps: 2, start_back: 46.0, finish_at: 0.86,
            // 配置は実際の長さに合わせて 1 周の割合で書く（作るときに m へ直す）。連続ジャンプの作業にならない間隔を取る。障害の直後に別の見えない障害を置かない（企画 06）
            obstacles: vec![ob(Hurdle, 0.13, 0.6, 0.0, 8.6), ob(Water, 0.355, 6.5, 3.2, 13.0), ob(Hurdle, 0.53, 0.6, 3.8, 13.0), ob(Water, 0.72, 6.5, 0.0, 9.2)],
            hills: None, gentle: false,
        },
        CourseDef {
            id: "hill", name: "丘のグランプリ", blurb: "上りでの消耗と、長い最終直線",
            points: super_oval(142.0, 54.0, &OVAL_HILL), width: 12.0, laps: 2, start_back: 46.0, finish_at: 0.93, obstacles: vec![],
            hills: Some(vec![[0.0, 0.0], [0.1, 0.0], [0.22, 9.0], [0.32, 12.0], [0.42, 9.0], [0.52, 2.5], [0.62, 0.0], [1.0, 0.0]]), gentle: false,
        },
    ]
}

/// 閉じた Catmull-Rom を細かく取る
fn sample_closed(points: &[[f64; 2]], per_seg: usize) -> Vec<(f64, f64)> {
    let n = points.len();
    let mut out = Vec::with_capacity(n * per_seg);
    for i in 0..n {
        let (p0, p1, p2, p3) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n], points[(i + 2) % n]);
        for k in 0..per_seg {
            let t = k as f64 / per_seg as f64;
            let t2 = t * t;
            let t3 = t2 * t;
            let f = |a: f64, b: f64, c: f64, d: f64| 0.5 * (2.0 * b + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (3.0 * b - a - 3.0 * c + d) * t3);
            out.push((f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])));
        }
    }
    out
}

#[derive(Clone, Copy, Debug)]
pub struct Curve { pub s0: f64, pub s1: f64 }

#[derive(Clone, Copy, Debug, Default)]
pub struct P3 { pub x: f64, pub y: f64, pub z: f64 }

#[derive(Clone, Debug)]
pub struct Track {
    pub def: CourseDef,
    pub n: usize,
    pub step: f64,
    pub lap: f64,
    pub width: f64,
    pub finish_s: f64,
    pub start_s: f64,
    pub race_len: f64,
    pub obstacles: Vec<Obstacle>,
    pub curves: Vec<Curve>,
    pub x: Vec<f64>, pub z: Vec<f64>, pub tx: Vec<f64>, pub tz: Vec<f64>, pub nx: Vec<f64>, pub nz: Vec<f64>, pub k: Vec<f64>, pub h: Vec<f64>, pub sl: Vec<f64>,
}

pub fn build_track(def: &CourseDef) -> Track {
    // 1. 細かく取って長さを測り、1m 刻みに取り直す
    let raw = sample_closed(&def.points, 24);
    let mut cum = vec![0.0];
    for i in 0..raw.len() {
        let (a, b) = (raw[i], raw[(i + 1) % raw.len()]);
        let c = cum[i] + hypot(b.0 - a.0, b.1 - a.1);
        cum.push(c);
    }
    let total = cum[raw.len()];
    let n = round(total) as usize;
    let lap = total;
    let step = lap / n as f64;
    let (mut xs, mut zs) = (vec![0.0; n], vec![0.0; n]);
    let mut j = 0;
    for i in 0..n {
        let t = i as f64 * step;
        while cum[j + 1] < t { j += 1; }
        let k = (t - cum[j]) / (cum[j + 1] - cum[j]);
        let (a, b) = (raw[j], raw[(j + 1) % raw.len()]);
        xs[i] = a.0 + (b.0 - a.0) * k;
        zs[i] = a.1 + (b.1 - a.1) * k;
    }
    // 2. ホームストレッチの手前（+z 側の直線の入り口）を s=0 にそろえる: 周回の始まりを、+z の直線で x が最も負の所へ
    let (mut origin, mut best) = (0, f64::INFINITY);
    let mut max_x = f64::NEG_INFINITY;
    let mut max_z = f64::NEG_INFINITY;
    for p in &def.points { max_x = sim_math::max(max_x, p[0]); max_z = sim_math::max(max_z, p[1]); }
    for i in 0..n {
        // +z 側で、直線の始まり（x が負で、曲がりが小さくなる所）を探す
        let score = if zs[i] > 0.0 { hypot(xs[i] + max_x * 0.72, zs[i] - max_z) } else { f64::INFINITY };
        if score < best { best = score; origin = i; }
    }
    let (mut x, mut z) = (vec![0.0; n], vec![0.0; n]);
    for i in 0..n { let q = (i + origin) % n; x[i] = xs[q]; z[i] = zs[q]; }
    // 3. 接線と外向きの法線、曲がり（符号付き。左に曲がるほど正）
    let (mut tx, mut tz, mut nx, mut nz, mut k) = (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    let w = 5usize;
    for i in 0..n {
        let (a, b) = ((i + n - w) % n, (i + w) % n);
        let (dx, dz) = (x[b] - x[a], z[b] - z[a]);
        let l = sim_math::or1(hypot(dx, dz));
        tx[i] = dx / l;
        tz[i] = dz / l;
        // 走る向きの右が外（d が増える向き）。上から見て反時計回りのとき、右は外側
        nx[i] = -tz[i];
        nz[i] = tx[i];
    }
    for i in 0..n {
        let (a, b) = ((i + n - w) % n, (i + w) % n);
        // 接線の向きの変わり方 / 距離。外向きの法線の側へ曲率の中心があると負になるように、内へ曲がるのを正にする
        let cross = tx[a] * tz[b] - tz[a] * tx[b];
        k[i] = -cross / (2.0 * w as f64 * step);
    }
    // 少しなめらかに
    for _ in 0..2 {
        let c = k.clone();
        for i in 0..n {
            let mut sum = 0.0;
            for o in -6i64..=6 { sum += c[(i as i64 + o + n as i64) as usize % n]; }
            k[i] = sum / 13.0;
        }
    }
    // 4. 高さ
    let mut h = vec![0.0; n];
    if let Some(hs) = &def.hills {
        for i in 0..n {
            let u = i as f64 / n as f64;
            let mut v = 0.0;
            for q in 0..hs.len() - 1 {
                if u >= hs[q][0] && u <= hs[q + 1][0] {
                    let kk = (u - hs[q][0]) / (hs[q + 1][0] - hs[q][0]);
                    v = hs[q][1] + (hs[q + 1][1] - hs[q][1]) * (kk * kk * (3.0 - 2.0 * kk));
                }
            }
            h[i] = v;
        }
    }
    let mut sl = vec![0.0; n];
    for i in 0..n { sl[i] = (h[(i + 4) % n] - h[(i + n - 4) % n]) / (8.0 * step); }

    let finish_s = lap * def.finish_at;
    let wrap0 = |s: f64| { let m = s % lap; if m < 0.0 { m + lap } else { m } };
    let start_s = wrap0(finish_s - def.start_back);
    // スタートからゴールまで
    let race_len = def.laps as f64 * lap + def.start_back;
    // 障害: 1周の中の割合 → m
    let obstacles: Vec<Obstacle> = def.obstacles.iter().map(|o| Obstacle { s: o.s * lap, ..*o }).collect();
    // カーブの区間（第1〜第4コーナーの案内）
    let mut curves = Vec::new();
    let (mut inside, mut begin) = (false, 0.0);
    for i in 0..=n {
        let c = k[i % n].abs() > 0.0035;
        if c && !inside { inside = true; begin = i as f64 * step; }
        if !c && inside {
            inside = false;
            if i as f64 * step - begin > 12.0 { curves.push(Curve { s0: begin, s1: i as f64 * step }); }
        }
    }
    Track { def: def.clone(), n, step, lap, width: def.width, finish_s, start_s, race_len, obstacles, curves, x, z, tx, tz, nx, nz, k, h, sl }
}

impl Track {
    #[inline]
    pub fn wrap(&self, s: f64) -> f64 { let m = s % self.lap; if m < 0.0 { m + self.lap } else { m } }
    #[inline]
    fn idx(&self, s: f64) -> (usize, usize, f64) {
        let u = self.wrap(s) / self.step;
        let fl = u.floor();
        let i = fl as usize % self.n;
        (i, (i + 1) % self.n, u - fl)
    }
    /// s の曲がり（内へ曲がるほど正）
    pub fn curv(&self, s: f64) -> f64 { let (i, b, f) = self.idx(s); self.k[i] + (self.k[b] - self.k[i]) * f }
    pub fn height(&self, s: f64) -> f64 { let (i, b, f) = self.idx(s); self.h[i] + (self.h[b] - self.h[i]) * f }
    pub fn slope(&self, s: f64) -> f64 { let (i, b, f) = self.idx(s); self.sl[i] + (self.sl[b] - self.sl[i]) * f }
    /// s・d → 世界の位置
    pub fn pos(&self, s: f64, d: f64) -> P3 {
        let (i, b, f) = self.idx(s);
        let x = self.x[i] + (self.x[b] - self.x[i]) * f;
        let z = self.z[i] + (self.z[b] - self.z[i]) * f;
        let nx = self.nx[i] + (self.nx[b] - self.nx[i]) * f;
        let nz = self.nz[i] + (self.nz[b] - self.nz[i]) * f;
        P3 { x: x + nx * d, z: z + nz * d, y: self.h[i] + (self.h[b] - self.h[i]) * f }
    }
    /// s の進む向きの角度（世界の y 回り。+z を 0 とする）
    pub fn yaw(&self, s: f64) -> f64 {
        let (i, b, f) = self.idx(s);
        atan2(self.tx[i] + (self.tx[b] - self.tx[i]) * f, self.tz[i] + (self.tz[b] - self.tz[i]) * f)
    }
    /// d の位置で 1m 走ると、s はどれだけ進むか（カーブの内側ほど大きい）
    pub fn progress_per_meter(&self, s: f64, d: f64) -> f64 {
        let k = self.curv(s);
        // k > 0（内へ曲がる）: 半径 1/k の内柵に対し、外側の半径は 1/k + d。
        // 幾何のままだと 1 周で 2π·d 損をして、外を回る手が死ぬ（試遊 1 回目: 内べったりが 3 着内 100%）。差を LANE_COST 倍に弱めて使う
        1.0 / sim_math::max(0.5, 1.0 + k * d * LANE_COST)
    }
    /// スタートからの距離 → 周回の s
    pub fn s_at(&self, dist: f64) -> f64 { self.wrap(self.start_s + dist) }
    /// 周回の s が、スタートから見てどの距離か（次に通る所）
    pub fn next_dist(&self, dist: f64, s: f64) -> f64 {
        let here = self.wrap(self.start_s + dist);
        let ahead = s - here;
        dist + if ahead >= 0.0 { ahead } else { ahead + self.lap }
    }
}
