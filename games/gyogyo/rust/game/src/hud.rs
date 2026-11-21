// 走る画面の表示: 順位・周回・残り距離・スタミナ（企画 08）と、通過順・ミニマップ・実況の一行・最初のレースの案内。src/ui/hud.ts の写し。
// 値は刻みの中で update が決め、描くのは ui.rs の Screens から（style.css の #hud と同じ配置・動き）。
use crate::ui::*;
use egui::{Pos2, Rect, pos2, vec2};
use serde_json::{Value, json};
use sim::items::{BASE_LEVELS, ITEM, Levels, Stat, Weapon};
use sim::race::{Race, Runner, TUNE};
use sim::result::to_fixed;

/// スタミナの見た目の値（割合・少ない・切れた・少なくなった時刻・上に出す字）
pub struct Stamina { pub k: f32, pub low: bool, pub out: bool, pub low_at: f64, pub label: String }

/// 持っている武器（残りの回数・残りの時間 0..1・息のたまり 0..1）
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Armed { pub weapon: Weapon, pub uses: i32, pub time: f32, pub charge: f32 }

/// 画面に弾んで出る字（当たった所の擬音・取った能力）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PopKind { Hit, Big, Good, Bad, Mark(Stat) }
#[derive(Clone, Debug)]
pub struct Pop { pub text: String, pub x: f32, pub y: f32, pub at: f64, pub kind: PopKind }

/// 能力の色（走路の札と同じ。view/items.rs の stat_color）
pub fn stat_c(s: Stat) -> C { match s { Stat::Speed => hx(0xff6b3d), Stat::Stamina => hx(0x35b86b), Stat::Guts => hx(0xe9a400), Stat::Jump => hx(0x3d94f0), Stat::Turbo => hx(0xa763f2) } }

pub struct Hud {
    pub now: f64,
    /// 撮影用（URL の ?video）: 順位・残り・武器だけを出す（能力の段・差・ミニマップ・通過順・実況・案内・スタミナの棒は出さない）
    pub video: bool,
    map_pts: Vec<(f32, f32)>,
    map_box: (f64, f64, f64),
    /// 走者の点（360×200 の絵の中の x, y, 枠, 自分か）とゴールの位置
    dots: Vec<(f32, f32, usize, bool)>,
    goal: (f32, f32),
    last_rank: i32,
    rank: String,
    rank_anim: Option<(bool, f64)>,
    lap: String,
    remain: String,
    lead: String,
    /// 通過順（走者の番号, 枠, 自分か）
    order: Vec<(usize, usize, bool)>,
    ticker: String,
    ticker_on: bool,
    ticker_k: Tween,
    tip_html: String,
    tip_on: bool,
    tip_k: Tween,
    tip_y: Tween,
    tick_t: f64,
    tip_t: f64,
    /// スタミナの割合（今の版は小数 3 桁の文字で渡す）
    st_k: f64,
    st_low: bool,
    st_low_at: f64,
    st_out: bool,
    st_label: String,
    count: String,
    count_go: bool,
    count_at: Option<f64>,
    place: String,
    place_gold: bool,
    place_at: Option<f64>,
    /// 背後の気配（走者の番号, 枠, 大きさ）。消えても中身は残す（今の版は innerHTML を書かない）
    rear: [Option<(usize, usize, f32)>; 2],
    rear_on: [bool; 2],
    rear_k: [Tween; 2],
    rear_key: String,
    fin_k: Tween,
    finishing: bool,
    /// 走者の番号 → 顔の絵
    pub faces: Vec<Option<Tex>>,
    /// 持っている武器・能力の段（上がった時刻）・攻撃のキーの表記（指のときは空）
    pub armed: Option<Armed>,
    armed_at: f64,
    levels: Levels,
    level_at: [f64; 5],
    pub attack_key: String,
    pops: Vec<Pop>,
}

impl Hud {
    pub fn new() -> Hud {
        Hud { video: false, now: 0.0, map_pts: vec![], map_box: (0.0, 0.0, 1.0), dots: vec![], goal: (0.0, 0.0), last_rank: 0, rank: "8".into(), rank_anim: None, lap: String::new(), remain: "0".into(), lead: String::new(),
            order: vec![], ticker: String::new(), ticker_on: false, ticker_k: Tween::new(0.0, 0.22, 0.0, css_ease), tip_html: String::new(), tip_on: false, tip_k: Tween::new(0.0, 0.25, 0.0, css_ease), tip_y: Tween::new(8.0, 0.3, 0.0, bezier_spring),
            tick_t: 0.0, tip_t: 0.0, st_k: 1.0, st_low: false, st_low_at: 0.0, st_out: false, st_label: String::new(), count: String::new(), count_go: false, count_at: None, place: String::new(), place_gold: false, place_at: None,
            rear: [None, None], rear_on: [false, false], rear_k: [Tween::new(0.0, 0.15, 0.0, css_ease); 2], rear_key: String::new(), fin_k: Tween::new(1.0, 0.25, 0.0, css_ease), finishing: false, faces: vec![],
            armed: None, armed_at: 0.0, levels: BASE_LEVELS, level_at: [-9.0; 5], attack_key: String::new(), pops: vec![],
        }
    }

    pub fn set_track(&mut self, race: &Race) {
        let tr = &race.track;
        let (mut x0, mut x1, mut z0, mut z1) = (1e9f64, -1e9f64, 1e9f64, -1e9f64);
        for i in 0..tr.n { x0 = x0.min(tr.x[i]); x1 = x1.max(tr.x[i]); z0 = z0.min(tr.z[i]); z1 = z1.max(tr.z[i]); }
        let (mw, mh) = (360.0, 200.0);
        let s = ((mw - 40.0) / (x1 - x0)).min((mh - 40.0) / (z1 - z0));
        self.map_box = ((x0 + x1) / 2.0, (z0 + z1) / 2.0, s);
        self.map_pts.clear();
        let mut i = 0;
        while i < tr.n {
            self.map_pts.push(((mw / 2.0 + (tr.x[i] - self.map_box.0) * s) as f32, (mh / 2.0 + (tr.z[i] - self.map_box.1) * s) as f32));
            i += 4;
        }
    }
    fn to_map(&self, x: f64, z: f64) -> (f32, f32) { ((180.0 + (x - self.map_box.0) * self.map_box.2) as f32, (100.0 + (z - self.map_box.1) * self.map_box.2) as f32) }

    /// 走者の番号 → 顔の絵（通過順の列に使う）
    pub fn set_faces(&mut self, list: Vec<Option<Tex>>) { self.faces = list; }

    pub fn reset(&mut self) {
        self.last_rank = 0;
        // 通過順の顔は次の update で作り直す（今の版は innerHTML を消さないので、隠れている間も前の並びのまま）
        self.rear_key.clear();
        self.rear_on = [false, false];
        for k in self.rear_k.iter_mut() { k.set(0.0, self.now); }
        self.ticker_on = false; self.ticker_k.set(0.0, self.now);
        self.tip_on = false; self.tip_k.set(0.0, self.now); self.tip_y.set(8.0, self.now);
        self.st_low = false;
        self.st_out = false;
        self.armed = None;
        self.level_at = [-9.0; 5];
        self.pops.clear();
    }
    /// 弾んで出る字（x・y は画面の px）
    pub fn pop(&mut self, text: &str, x: f32, y: f32, kind: PopKind) {
        self.pops.retain(|p| self.now - p.at < 1.2);
        if self.pops.len() >= 8 { self.pops.remove(0); }
        // 出たばかりの文字と重なるときは、上へずらす（近くで 2 つの当たりが続いたとき、字が重なって読めなかった）
        let mut y = y;
        for _ in 0..4 {
            if !self.pops.iter().any(|p| self.now - p.at < 0.7 && (p.x - x).abs() < 160.0 && (p.y - y).abs() < 52.0) { break; }
            y -= 58.0;
        }
        self.pops.push(Pop { text: text.to_string(), x, y, at: self.now, kind });
    }
    /// body.finishing（ゴールの瞬間からの表示）と h-place
    /// h-place の .show は次のゴールまで残る（見えるのは body.finishing の間だけ）
    pub fn set_finishing(&mut self, on: bool) { self.finishing = on; self.fin_k.set(if on { 0.0 } else { 1.0 }, self.now); }
    pub fn show_place(&mut self, text: &str, gold: bool) { self.place = text.to_string(); self.place_gold = gold; self.place_at = Some(self.now); }

    pub fn update(&mut self, race: &Race, dt: f64) {
        let now = self.now;
        if self.tick_t > 0.0 { self.tick_t -= dt; if self.tick_t <= 0.0 { self.ticker_on = false; self.ticker_k.set(0.0, now); } }
        if self.tip_t > 0.0 { self.tip_t -= dt; if self.tip_t <= 0.0 { self.tip_on = false; self.tip_k.set(0.0, now); self.tip_y.set(8.0, now); } }
        // ミニマップの点
        let tr = &race.track;
        let gp = tr.pos(tr.finish_s, tr.width / 2.0);
        self.goal = self.to_map(gp.x, gp.z);
        let me = race.player_gate;
        self.dots.clear();
        for r in &race.runners { if Some(r.i) != me { let p = tr.pos(r.s, r.d * 0.5); let (x, y) = self.to_map(p.x, p.z); self.dots.push((x, y, r.gate, false)); } }
        if let Some(p) = race.player() { let q = tr.pos(p.s, p.d * 0.5); let (x, y) = self.to_map(q.x, q.z); self.dots.push((x, y, p.gate, true)); }
        self.order = race.order.iter().map(|&i| (i, race.runners[i].gate, Some(i) == race.player_gate)).collect();
        let Some(p) = race.player() else { return };
        if p.rank != self.last_rank {
            if self.last_rank != 0 { self.rank_anim = Some((p.rank < self.last_rank, now)); }
            self.last_rank = p.rank;
            self.rank = p.rank.to_string();
        }
        let rem = (tr.race_len - p.dist).max(0.0);
        self.remain = (rem.ceil() as i64).to_string();
        let laps = tr.def.laps;
        let lap_now = laps.min(1 + ((p.dist - tr.def.start_back).max(0.0) / tr.lap).floor() as i32);
        self.lap = format!("{lap_now}周目 / {laps}周");
        self.lead = if p.rank > 1 {
            let top = &race.runners[race.order[0]];
            format!("先頭まで {} 秒", to_fixed((top.dist - p.dist).max(0.0) / p.speed.max(6.0), 1))
        } else {
            let sec = &race.runners[race.order[1]];
            format!("2番手まで {} 秒", to_fixed((p.dist - sec.dist).max(0.0) / p.speed.max(6.0), 1))
        };
        // 背後の気配: 真後ろの近い走者を、左右どちらから来ているかで出す（前を見たまま分かる）
        let (mut l, mut r, mut lg, mut rg): (Option<&Runner>, Option<&Runner>, f64, f64) = (None, None, 5.2, 5.2);
        for o in &race.runners {
            if o.i == p.i || o.finished { continue; }
            let g = p.dist - o.dist;
            if g <= 0.2 || g > 5.2 { continue; }
            if o.d < p.d { if g < lg { lg = g; l = Some(o); } } else if g < rg { rg = g; r = Some(o); }
        }
        let rk = format!("{}|{}", l.map_or(-1, |o| o.i as i64), r.map_or(-1, |o| o.i as i64));
        if rk != self.rear_key {
            self.rear_key = rk;
            for (k, o) in [l, r].iter().enumerate() {
                let on = o.is_some();
                if let Some(o) = o { self.rear[k] = Some((o.i, o.gate, self.rear[k].map_or(1.0, |x| x.2))); }
                self.rear_on[k] = on;
                self.rear_k[k].set(if on { 1.0 } else { 0.0 }, now);
            }
        }
        // 近いほど大きい（今の版は小数 2 桁の文字で渡す）
        if let (Some(_), Some(x)) = (l, self.rear[0].as_mut()) { x.2 = to_fixed(1.7 - lg / 5.2, 2).parse().unwrap_or(1.0); }
        if let (Some(_), Some(x)) = (r, self.rear[1].as_mut()) { x.2 = to_fixed(1.7 - rg / 5.2, 2).parse().unwrap_or(1.0); }
        self.st_k = to_fixed(p.stamina / TUNE.stamina_max, 3).parse::<f64>().unwrap_or(1.0);
        let low = p.exhausted <= 0.0 && p.stamina < TUNE.low_at;
        if low && !self.st_low { self.st_low_at = now; }
        self.st_low = low;
        self.st_out = p.exhausted > 0.0;
        self.st_label = (if p.exhausted > 0.0 { "息切れ" } else if p.draft && !p.dashing { "風よけ" } else { "" }).to_string();
        let armed = p.weapon.map(|w| Armed { weapon: w, uses: p.weapon_uses, time: (p.weapon_t / ITEM.weapon_time).clamp(0.0, 1.0) as f32, charge: p.charge as f32 });
        if armed.map(|a| a.weapon) != self.armed.map(|a| a.weapon) && armed.is_some() { self.armed_at = now; }
        self.armed = armed;
        for k in 0..5 { if p.lv[k] > self.levels[k] && self.last_rank != 0 { self.level_at[k] = now; } }
        self.levels = p.lv;
    }

    pub fn say(&mut self, text: &str, sec: f64) {
        self.ticker = text.to_string();
        self.ticker_on = true;
        self.ticker_k.set(1.0, self.now);
        self.tick_t = sec;
    }
    pub fn tip(&mut self, html: &str, sec: f64) {
        self.tip_html = html.to_string();
        self.tip_on = true;
        self.tip_k.set(1.0, self.now);
        self.tip_y.set(0.0, self.now);
        self.tip_t = sec;
    }
    pub fn count(&mut self, text: &str, go: bool) {
        self.count = text.to_string();
        self.count_go = go;
        self.count_at = Some(self.now);
    }

    /// 確かめ用: 今の版の #hud の中身と同じ文字
    pub fn dump(&self) -> Value {
        json!({
            "rank": self.rank, "lap": self.lap, "remain": self.remain, "lead": self.lead,
            "order": self.order.iter().map(|o| o.1).collect::<Vec<_>>(), "me": self.order.iter().position(|o| o.2),
            "ticker": self.ticker, "tickerOn": self.ticker_on, "tip": self.tip_html, "tipOn": self.tip_on,
            "stamina": self.st_k, "low": self.st_low, "out": self.st_out, "stLabel": self.st_label,
            "count": self.count, "go": self.count_go, "place": self.place, "gold": self.place_gold,
            "rear": [self.rear_on[0].then(|| self.rear[0].map(|x| x.1)).flatten(), self.rear_on[1].then(|| self.rear[1].map(|x| x.1)).flatten()],
            // 今の版の class と style の写し（アニメーションを始めた印。終わっても残る）
            "rankC": match self.rank_anim { Some((true, _)) => "up", Some((false, _)) => "down", None => "" },
            "countC": match (self.count_at.is_some(), self.count_go) { (true, true) => "go pop", (true, false) => "pop", _ => "" },
            "placeC": format!("{}{}", if self.place_gold { "gold" } else { "" }, if self.place_at.is_some() { if self.place_gold { " show" } else { "show" } } else { "" }),
            "rearS": self.rear.map(|x| x.map_or(String::new(), |r| format!("scale({})", r.2))),
            "levels": self.levels, "weapon": self.armed.map(|a| json!({ "id": a.weapon.id(), "uses": a.uses, "time": a.time, "charge": a.charge })),
            "pops": self.pops.iter().filter(|p| self.now - p.at < 1.0).map(|p| p.text.clone()).collect::<Vec<_>>(),
        })
    }

    fn face(&self, pen: &Pen, c: Pos2, d: f32, b: f32, i: usize, gate: usize, me: bool, notrans: bool) {
        let p = if me { pen.sub(Xf::about(c.x, c.y, Xf::scale(1.38, 1.38)), 1.0) } else { pen.clone() };
        let _ = notrans;
        let r = Rect::from_center_size(c, vec2(d, d));
        p.box_shadow(r, d * 0.5, 0.0, 1.0, 4.0, ink(0.6));
        p.circle(c, d * 0.5, GATE_COLOR[gate - 1]);
        let inner = r.shrink(b);
        p.circle(c, inner.width() * 0.5, CREAM);
        if let Some(Some(t)) = self.faces.get(i) {
            let pts: Vec<Pos2> = (0..32).map(|k| { let a = k as f32 / 32.0 * std::f32::consts::TAU; c + vec2(a.cos(), a.sin()) * inner.width() * 0.5 }).collect();
            p.image_poly(t.handle.id(), &pts, inner, cover_uv(t, inner.width(), inner.height(), 0.5, 0.5), WHITE);
        }
        if me { p.circle_line(c, d * 0.5 + 2.0, 2.0, CREAM); }
    }

    /// スタミナ（指のときは ui.rs が「ダッシュ」を囲む輪で描く）
    pub fn stamina(&self) -> Stamina { Stamina { k: self.st_k.clamp(0.0, 1.0) as f32, low: self.st_low, out: self.st_out, low_at: self.st_low_at, label: self.st_label.clone() } }

    /// #hud を描く（screen は画面、safe は端の安全域）。dash は指の「ダッシュ」のボタン（指の機器のときだけ）
    pub fn draw(&mut self, pen: &Pen, r: Rect, safe: [f32; 4], dash: Option<Rect>, finishing: bool, notrans: bool) {
        let touch = dash.is_some();
        let now = self.now;
        let (w, h) = (r.width(), r.height());
        let (narrow, short) = (w <= 820.0, h <= 520.0);
        let fk = if notrans { if finishing { 0.0 } else { 1.0 } } else { self.fin_k.get(now) };
        let others = pen.with_alpha(fk);
        let tk = |t: &Tween, on: bool| if notrans { if on { 1.0 } else { 0.0 } } else { t.get(now) };

        // 順位
        if fk > 0.0 {
            let p = &others;
            let (bfs, sfs, efs, emt): (f32, f32, f32, f32) = if short { (54.0, 18.0, 12.0, 2.0) } else if narrow { (68.0, 20.0, 15.0, 6.0) } else { (96.0, 28.0, 15.0, 6.0) };
            let above = (0.819 * bfs).max(13.1_f32).max(0.819 * sfs);
            let below = (0.081 * bfs).max(1.3_f32);
            let line_h = above + below;
            let bt = pen.t(&self.rank, NUM7, bfs, 0.0);
            let st = pen.t("/8", NUM6, sfs, 0.0);
            let et = pen.t(&self.lap, JP7, efs, 0.0);
            let cw = (bt.w + 4.0 + st.w).max(et.w);
            let bx = Rect::from_min_size(pos2(14.0 + safe[3], 10.0 + safe[0]), vec2(10.0 + cw + 16.0, 6.0 + line_h + emt + 0.9 * efs + 8.0));
            p.rrect(bx, 18.0, ink(0.42));
            let (cx, cy) = (bx.left() + 10.0, bx.top() + 6.0);
            let base = cy + above;
            // 順位が上がる・下がるの動き
            let (mut xf, mut col) = (Xf::ID, CREAM);
            if let Some((up, at)) = self.rank_anim && !notrans {
                let t = (now - at) as f32;
                let bc = pos2(cx + bt.w * 0.5, base - 0.819 * bfs + 0.45 * bfs);
                if up && t < 0.42 {
                    let k = t / 0.42;
                    let s = kf(k, &[(0.0, 1.0), (0.35, 1.32), (1.0, 1.0)], bezier_spring);
                    xf = Xf::about(bc.x, bc.y, Xf::scale(s, s));
                    col = if k < 0.35 { GOLD } else { GOLD.lerp(CREAM, bezier_spring((k - 0.35) / 0.65)) };
                } else if !up && t < 0.34 {
                    let k = t / 0.34;
                    xf = Xf::tr(0.0, kf(k, &[(0.0, 0.0), (0.4, 8.0), (1.0, 0.0)], ease));
                    let pink = hx(0xffb8a8);
                    col = if k < 0.4 { pink } else { pink.lerp(CREAM, ease((k - 0.4) / 0.6)) };
                }
            }
            p.sub(xf, 1.0).text(&bt, cx, base, col, &[]);
            p.with_alpha(0.86).text(&st, cx + bt.w + 4.0, base, CREAM, &[]);
            let etop = cy + line_h + emt;
            p.text(&et, cx, base_l(JP7, efs, etop, 0.9 * efs), CREAM, &[]);
            // 能力の段（順位の箱の下。札と同じ色と 1 字。上がった瞬間に弾む）
            if !self.video {
                // 縦持ち（狭い画面）は右のミニマップに重ならないよう、詰めて並べる
                let (d, gap, fs, tail): (f32, f32, f32, f32) = if narrow && !short { (17.0, 2.0, 9.0, 4.0) } else if short { (20.0, 3.0, 10.0, 7.0) } else { (26.0, 4.0, 12.0, 7.0) };
                let mut x = bx.left();
                let y = bx.bottom() + 6.0;
                for s in Stat::ALL {
                    let lv = self.levels[s.i()].to_string();
                    let lt = pen.t(&lv, NUM7, fs + 1.0, 0.0);
                    let cw = d + 3.0 + lt.w + tail;
                    let cr = Rect::from_min_size(pos2(x, y), vec2(cw, d));
                    let t = (now - self.level_at[s.i()]) as f32;
                    let sc = if !notrans && t < 0.45 { 1.0 + 0.45 * (1.0 - t / 0.45) * bezier_spring((t / 0.45).min(1.0)) } else { 1.0 };
                    let q = p.sub(Xf::about(cr.center().x, cr.center().y, Xf::scale(sc, sc)), 1.0);
                    q.rrect(cr, d * 0.5, if t < 0.45 && !notrans { GOLD } else { ink(0.5) });
                    q.circle(pos2(x + d * 0.5, y + d * 0.5), d * 0.5 - 2.0, stat_c(s));
                    let gt = pen.t(s.glyph(), JP9, fs, 0.0);
                    q.text(&gt, x + d * 0.5 - gt.w * 0.5, base_n(JP9, s.glyph(), fs, y + d * 0.5 - fs * 1.448 * 0.5), WHITE, &[]);
                    q.text(&lt, x + d + 3.0, base_n(NUM7, &lv, fs + 1.0, y + d * 0.5 - nlh(NUM7, &lv, fs + 1.0) * 0.5), if t < 0.45 && !notrans { GREEN } else { CREAM }, &[]);
                    x += cw + gap;
                }
            }

            // 右の列: 残り・差・ミニマップ
            let sh = [TS(0.0, 1.0, 8.0, ink(0.9))];
            let xr = w - 68.0 - safe[1];
            let mut y = 10.0 + safe[0];
            let (rfs, bfs2): (f32, f32) = if short { (13.0, 26.0) } else if narrow { (16.0, 28.0) } else { (16.0, 40.0) };
            let la = (1.16 * rfs).max(0.974 * bfs2);
            let ld = (0.288 * rfs).max(0.236 * bfs2);
            let (t1, t2, t3) = (pen.t("残り", JP7, rfs, 0.0), pen.t(&self.remain, NUM6, bfs2, 0.0), pen.t("m", JP7, rfs, 0.0));
            let rw = t1.w + 4.0 + t2.w + 4.0 + t3.w;
            let base = y + la;
            p.text(&t1, xr - rw, base, CREAM, &sh);
            p.text(&t2, xr - rw + t1.w + 4.0, base, CREAM, &sh);
            p.text(&t3, xr - t3.w, base, CREAM, &sh);
            y += la + ld + 2.0;
            let lfs: f32 = if short { 12.0 } else { 14.0 };
            let lh = if self.lead.is_empty() { 18.0 } else { (lfs * 1.448).max(18.0) };
            if !self.lead.is_empty() && !self.video {
                let t = pen.t(&self.lead, JP7, lfs, 0.0);
                p.text(&t, xr - t.w, base_l(JP7, lfs, y, lh), CREAM, &sh);
            }
            y += lh + 2.0 + 4.0;
            let (mw, mh) = if short { (112.0, 62.0) } else if narrow { (120.0, 66.0) } else { (180.0, 100.0) };
            if !self.video { self.draw_map(p, Rect::from_min_size(pos2(xr - mw, y), vec2(mw, mh))); }

            // 通過順（狭い画面では出さない）
            if !narrow && !self.video && !self.order.is_empty() {
                let (d, b) = if short { (22.0, 2.0) } else { (30.0, 3.0) };
                let n = self.order.len();
                let me_first = self.order.first().is_some_and(|o| o.2);
                let extra = if self.order.iter().any(|o| o.2) { if me_first { 10.0 } else { 5.0 } } else { 0.0 };
                let iw = n as f32 * d + (n - 1) as f32 * 3.0 + extra;
                let bx = Rect::from_min_size(pos2(w * 0.5 - (iw + 16.0) * 0.5, 10.0 + safe[0]), vec2(iw + 16.0, d + 10.0));
                p.rrect(bx, bx.height() * 0.5, ink(0.5));
                let mut x = bx.left() + 8.0;
                // 自分は最後に描く（z-index: 2）
                let mut mine = None;
                for (k, &(i, gate, me)) in self.order.iter().enumerate() {
                    if me && k == 0 { x += 5.0; }
                    let c = pos2(x + d * 0.5, bx.top() + 5.0 + d * 0.5);
                    if me { mine = Some((c, i, gate)); } else { self.face(p, c, d, b, i, gate, false, notrans); }
                    x += d + 3.0 + if me { 5.0 } else { 0.0 };
                }
                if let Some((c, i, gate)) = mine { self.face(p, c, d, b, i, gate, true, notrans); }
            }

            // 実況の一行
            let k = tk(&self.ticker_k, self.ticker_on);
            if k > 0.0 && !self.ticker.is_empty() && !self.video {
                let fs = if short || narrow { 15.0 } else { 20.0 };
                // 縦持ち（狭い画面）は、右の列（残り・差・ミニマップ）に重ならないよう、その下
                let top = if short { 40.0 } else if narrow { 148.0 } else { 56.0 } + safe[0];
                let t = pen.t(&self.ticker, JP9, fs, fs * 0.02);
                p.with_alpha(k).text(&t, w * 0.5 - t.w * 0.5, base_n(JP9, &self.ticker, fs, top), CREAM, &[TS(0.0, 2.0, 12.0, ink(0.9)), TS(0.0, 0.0, 3.0, ink(0.8))]);
            }
            // 案内
            let k = tk(&self.tip_k, self.tip_on);
            if k > 0.0 && !self.tip_html.is_empty() && !self.video {
                let fs = if short { 14.0 } else { 16.0 };
                let items = rich_items(pen, &rich(&self.tip_html), fs, JP7, CREAM);
                let cw = it_w(pen, &items);
                let ph = fs * 1.448 + 18.0;
                let dy = if notrans { if self.tip_on { 0.0 } else { 8.0 } } else { self.tip_y.get(now) };
                // 指のときは上の方（下はスティック・ボタン・背後の気配、真ん中は自分。縦持ちでは実況の一行の下）
                let top = if short { h * 0.26 } else if touch { (h * 0.26).max(186.0 + safe[0]) } else { h - 112.0 - safe[2] - ph } + dy;
                let pr = Rect::from_min_size(pos2(w * 0.5 - (cw + 36.0) * 0.5, top), vec2(cw + 36.0, ph));
                let q = p.with_alpha(k);
                q.rrect(pr, ph * 0.5, GLASS);
                it_draw(&q, &items, pr.left() + 18.0, pr.center().y, CREAM, &[]);
            }
            // スタミナ（指のときは「ダッシュ」を囲む輪。ui.rs の touch_ui）
            if !touch && !self.video {
                let sw = if short { 300.0_f32.min(w * 0.38) } else { 300.0_f32.min(w * 0.5) };
                let shh = if short { 14.0 } else { 13.0 };
                let bottom = if short { 10.0 } else { 22.0 } + safe[2];
                let sr = Rect::from_min_size(pos2(w * 0.5 - sw * 0.5, h - bottom - shh), vec2(sw, shh));
                let rad = shh * 0.5;
                let (spread, rc) = if self.st_low && !notrans {
                    let t = ((now - self.st_low_at) as f32 / 0.42).rem_euclid(2.0);
                    let k = ease_io(if t < 1.0 { t } else { 2.0 - t });
                    (lerp(2.0, 3.0, k), rgba(255, 246, 228, 0.55).lerp(hx(0xff8a5c), k))
                } else { (2.0, rgba(255, 246, 228, 0.55)) };
                p.rframe(sr.expand(spread), rad + spread, spread, rc);
                p.rrect(sr, rad, ink(0.68));
                let sk = self.st_k.clamp(0.0, 1.0) as f32;
                let fw = sw * sk;
                if fw > 0.0 {
                    let fr = Rect::from_min_size(sr.min, vec2(fw, shh));
                    let pts = rr_xy(fr, rad * sk, rad);
                    if self.st_out { p.poly(&pts, hx(0x7a8a82)); }
                    else if self.st_low { p.poly_grad(&pts, fr, 90.0, hx(0xff6a3a), hx(0xffb04d)); }
                    else { p.poly_grad(&pts, fr, 90.0, hx(0x2fc98a), hx(0x8fe27a)); }
                }
                p.rect(Rect::from_min_size(pos2(sr.left() + sw * 0.22, sr.top()), vec2(2.0, shh)), rgba(255, 246, 228, 0.5));
                if !self.st_label.is_empty() {
                    let t = pen.t(&self.st_label, JP9, 10.0, 0.0);
                    p.text(&t, sr.center().x - t.w * 0.5, base_n(JP9, &self.st_label, 10.0, sr.center().y - 10.0 * 1.448 * 0.5), WHITE, &[TS(0.0, 1.0, 3.0, ink(0.9))]);
                }
            }
            // 持っている武器（指のときは「こうげき」のボタンが見せる。ui.rs の touch_ui）: スタミナの棒の上に、キー・名前・残りの回数・残りの時間。ためている間は金色に満ちる
            if !touch && let Some(a) = self.armed {
                let fs = if short { 13.0 } else { 15.0 };
                let name = a.weapon.name();
                let mut items = vec![];
                if !self.attack_key.is_empty() { items.push(It::K(Kbd::new(pen, &self.attack_key, fs))); }
                items.push(It::T(name.to_string(), JP9, fs, 0.0, CREAM));
                let iw = it_w(pen, &items);
                let dots = a.weapon.uses() as f32 * 11.0;
                let (pw, ph) = (16.0 + iw + 10.0 + dots + 12.0, fs * 1.448 + 14.0);
                let bottom = if short { 10.0 + 14.0 + 8.0 } else { 22.0 + 13.0 + 12.0 } + safe[2];
                let pr = Rect::from_min_size(pos2(w * 0.5 - pw * 0.5, h - bottom - ph), vec2(pw, ph));
                let t = (now - self.armed_at) as f32;
                let sc = if !notrans && t < 0.4 { lerp(1.5, 1.0, bezier_spring(t / 0.4)) } else { 1.0 };
                let q = p.sub(Xf::about(pr.center().x, pr.center().y, Xf::scale(sc, sc)), 1.0);
                q.box_shadow(pr, ph * 0.5, 0.0, 2.0, 10.0, ink(0.5));
                q.rrect(pr, ph * 0.5, ink(0.72));
                // 息のたまり（左から満ちる）
                if a.charge > 0.0 {
                    let fr = Rect::from_min_size(pr.min, vec2(pw * a.charge, ph));
                    let full = a.charge >= 1.0;
                    let pulse = if full && !notrans { 0.75 + 0.25 * ((now * 14.0).sin() as f32).abs() } else { 0.8 };
                    q.with_alpha(pulse).poly(&rr_xy(fr, (ph * 0.5).min(fr.width() * 0.5), ph * 0.5), if full { GOLD } else { hx(0xc98a1a) });
                }
                q.rframe(pr, ph * 0.5, 2.0, if a.charge >= 1.0 { CREAM } else { GOLD });
                it_draw(&q, &items, pr.left() + 16.0, pr.center().y, CREAM, &[TS(0.0, 1.0, 3.0, ink(0.8))]);
                for k in 0..a.weapon.uses() {
                    let c = pos2(pr.left() + 16.0 + iw + 10.0 + 5.0 + k as f32 * 11.0, pr.center().y);
                    if k < a.uses { q.circle(c, 4.0, CREAM); } else { q.circle_line(c, 3.5, 1.5, rgba(255, 246, 228, 0.45)); }
                }
                // 残りの時間（下の細い線。少なくなると赤い）
                let tr_ = Rect::from_min_size(pos2(pr.left() + ph * 0.5, pr.bottom() + 3.0), vec2((pw - ph) * a.time, 3.0));
                q.rrect(tr_, 1.5, if a.time < 0.22 { hx(0xff8a5c) } else { CREAM });
                if a.charge > 0.0 {
                    let s = if a.charge >= 1.0 { "いぶき満タン！ 離すと一撃" } else { "息をためている…" };
                    let lt = pen.t(s, JP9, 12.0, 0.0);
                    q.text(&lt, pr.center().x - lt.w * 0.5, base_n(JP9, s, 12.0, pr.top() - 12.0 * 1.448 - 3.0), if a.charge >= 1.0 { GOLD } else { CREAM }, &[TS(0.0, 1.0, 4.0, ink(0.9))]);
                }
            }
            // 弾んで出る字（当たった所の擬音・取った能力）。大きく傾けて、太い影で縁を取る
            if !notrans {
                for (k, pp) in self.pops.iter().enumerate() {
                    let t = (now - pp.at) as f32;
                    let life = match pp.kind { PopKind::Big => 1.0, PopKind::Hit => 0.8, _ => 0.9 };
                    if t >= life { continue; }
                    let u = t / life;
                    let (fs, col, rot): (f32, C, f32) = match pp.kind {
                        PopKind::Big => (cl(54.0, h * 0.13, 120.0), GOLD, -9.0),
                        PopKind::Hit => (cl(40.0, h * 0.095, 86.0), CREAM, if k % 2 == 0 { -8.0 } else { 7.0 }),
                        PopKind::Good => (if short { 20.0 } else { 26.0 }, hx(0x8fe27a), -4.0),
                        PopKind::Bad => (if short { 20.0 } else { 26.0 }, hx(0xc88cff), 4.0),
                        PopKind::Mark(s) => (if short { 18.0 } else { 22.0 }, stat_c(s).lerp(WHITE, 0.35), 0.0),
                    };
                    let big = matches!(pp.kind, PopKind::Big | PopKind::Hit);
                    let sc = if big { kf(u, &[(0.0, 2.2), (0.16, 0.92), (0.3, 1.0), (1.0, 1.06)], ease) } else { kf(u, &[(0.0, 0.5), (0.2, 1.12), (0.35, 1.0), (1.0, 1.0)], ease) };
                    let op = kf(u, &[(0.0, 0.0), (0.08, 1.0), (0.7, 1.0), (1.0, 0.0)], ease);
                    let rise = if big { 0.0 } else { -34.0 * u };
                    let fam = if big { DISP } else { JP9 };
                    let tx = pen.t(&pp.text, fam, fs, 0.0);
                    // 狭い画面（縦持ち）では、幅に入るように縮める。端からはみ出さない所へ寄せる（入らなければ真ん中）
                    let room = (w - safe[1] - safe[3] - 20.0).max(40.0);
                    let fit = (room / tx.w).min(1.0);
                    let half = tx.w * fit * 0.5 + 10.0;
                    let (lo, hi) = (half + safe[3], w - half - safe[1]);
                    let cx = if lo <= hi { pp.x.clamp(lo, hi) } else { w * 0.5 };
                    let (ylo, yhi) = (fs + safe[0], h - fs - safe[2]);
                    let cy = if ylo <= yhi { pp.y.clamp(ylo, yhi) } else { h * 0.5 } + rise;
                    let sc = sc * fit;
                    let q = p.sub(Xf::about(cx, cy, Xf::rot(rot).then(Xf::scale(sc, sc))), op);
                    let o = (fs * 0.07).max(2.0);
                    let sh: Vec<TS> = if big { vec![TS(o, o, 0.0, hx(0x27103a)), TS(-o, o, 0.0, hx(0x27103a)), TS(o, -o, 0.0, hx(0x27103a)), TS(-o, -o, 0.0, hx(0x27103a)), TS(0.0, o * 2.4, 0.0, hx(0xe6402f))] } else { vec![TS(0.0, 2.0, 0.0, ink(0.9)), TS(0.0, 0.0, 6.0, ink(0.8))] };
                    q.text(&tx, cx - tx.w * 0.5, base_n(fam, &pp.text, fs, cy - nlh(fam, &pp.text, fs) * 0.5), col, &sh);
                }
            }
            // カウントダウン
            if let Some(at) = self.count_at && !notrans {
                let k = (now - at) as f32 / 0.72;
                if k < 1.0 {
                    let op = kf(k, &[(0.0, 0.0), (0.18, 1.0), (0.75, 1.0), (1.0, 0.0)], ease);
                    let s = kf(k, &[(0.0, 1.8), (0.18, 1.0), (1.0, 0.92)], ease);
                    let fs = cl(90.0, h * 0.18, 200.0);
                    let t = pen.t(&self.count, NUM7, fs, 0.0);
                    let c = pos2(w * 0.5, h * 0.34);
                    let lh = nlh(NUM7, &self.count, fs);
                    let q = p.sub(Xf::about(c.x, c.y, Xf::scale(s, s)), op);
                    q.text(&t, c.x - t.w * 0.5, base_n(NUM7, &self.count, fs, c.y - lh * 0.5), if self.count_go { GOLD } else { CREAM }, &[TS(0.0, 6.0, 30.0, ink(0.5))]);
                }
            }
            // 背後の気配（撮影用は出さない）
            for k in 0..2 {
                if self.video { break; }
                let op = tk(&self.rear_k[k], self.rear_on[k]);
                let Some((i, gate, scale)) = self.rear[k] else { continue };
                if op <= 0.0 { continue; }
                let d = if short { 22.0 } else { 30.0 };
                let b = if short { 2.0 } else { 3.0 };
                let arrow = if k == 0 { "◀" } else { "▶" };
                let at = pen.t(arrow, JP9, 13.0, 0.0);
                let ew = d + 4.0 + at.w;
                let eh = d.max(13.0 * 1.448);
                // 指の縦持ちでは左右に広げるとボタンに重なるので、ボタンの上で真ん中に寄せる
                let tight = dash.filter(|d| w * 0.5 + 150.0 > d.left() - 8.0);
                let bottom = if let Some(d) = tight { h - d.top() + 18.0 } else if touch { 40.0 + safe[2] } else if short { 34.0 + safe[2] } else { 66.0 + safe[2] };
                let left = if tight.is_some() { if k == 0 { w * 0.5 - 16.0 - ew } else { w * 0.5 + 16.0 } } else if touch { if k == 0 { w * 0.5 - 150.0 } else { w * 0.5 + 150.0 - ew } } else if narrow { if k == 0 { 12.0 + safe[3] } else { w - 12.0 - safe[1] - ew } } else if k == 0 { w * 0.5 - 250.0 } else { w * 0.5 + 250.0 - ew };
                let er = Rect::from_min_size(pos2(left, h - bottom - eh), vec2(ew, eh));
                let q = p.sub(Xf::about(er.center().x, er.bottom(), Xf::scale(scale, scale)), op);
                let sh = [TS(0.0, 1.0, 6.0, ink(0.9))];
                let (fx, ax) = if k == 0 { (er.left() + d * 0.5, er.left() + d + 4.0) } else { (er.right() - d * 0.5, er.left()) };
                self.face(&q, pos2(fx, er.center().y), d, b, i, gate, false, notrans);
                q.text(&at, ax, base_n(JP9, arrow, 13.0, er.center().y - 13.0 * 1.448 * 0.5), CREAM, &sh);
            }
        }
        // 着順（ゴールの瞬間）
        // body.notrans では placePop が外れ、元の opacity: 0 のまま
        if finishing && !notrans && let Some(at) = self.place_at {
            let fs = cl(64.0, h * 0.15, 150.0);
            let (x, y) = (w * 0.05 + safe[3], h * 0.08 + safe[0]);
            let k = clamp01((now - at) as f32 / 0.6);
            let e = bezier_spring(k);
            let s = lerp(1.6, 1.0, e);
            let q = pen.sub(Xf::about(x, y, Xf::scale(s, s)), clamp01(e));
            let t = pen.t(&self.place, DISP, fs, 0.0);
            q.text(&t, x, base_l(DISP, fs, y, fs), if self.place_gold { GOLD } else { CREAM }, &[TS(0.0, 0.05 * fs, 0.0, ink(0.55)), TS(0.0, 0.0, 40.0, ink(0.35))]);
        }
    }

    /// #h-map（360×200 の絵を枠に合わせて縮める）
    fn draw_map(&self, pen: &Pen, r: Rect) {
        let (kx, ky) = (r.width() / 360.0, r.height() / 200.0);
        let at = |x: f32, y: f32| pos2(r.left() + x * kx, r.top() + y * ky);
        let k = kx.min(ky);
        let p = pen.sub(Xf::tr(r.left(), r.top()).then(Xf::scale(kx, ky)), 1.0);
        p.rrect(Rect::from_min_size(pos2(0.0, 0.0), vec2(360.0, 200.0)), 26.0, ink(0.55));
        if self.map_pts.len() > 2 {
            let mut pts: Vec<Pos2> = self.map_pts.iter().map(|(x, y)| at(*x, *y)).collect();
            pts.push(pts[0]);
            pen.closed(&pts[..pts.len() - 1], 7.0 * k, rgba(255, 246, 228, 0.92));
        }
        p.rect(Rect::from_min_size(pos2(self.goal.0 - 2.0, self.goal.1 - 9.0), vec2(4.0, 18.0)), hx(0x111111));
        for &(x, y, gate, me) in &self.dots {
            let c = at(x, y);
            let rad = if me { 11.0 } else { 7.0 } * k;
            pen.circle(c, rad, GATE_COLOR[gate - 1]);
            pen.circle_line(c, rad, if me { 4.0 } else { 2.0 } * k, if me { WHITE } else { rgba(10, 30, 24, 0.8) });
        }
    }
}
