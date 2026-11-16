// HUD: 体力・乱舞ゲージ・撃破数・連撃数・目標・敵将の体力・小地図・札（隊長・大将の名前）・大きな知らせ（hud.ts）。
// 値の更新（TS の update）と描画を毎コマ Hud::ui の 1 回で行う。
use crate::controls::{self, Act, Device};
use crate::gfx::Camera;
use crate::stage::STAGE;
use crate::ui::*;
use egui::epaint::Mesh;
use egui::{Context, Id, LayerId, Order, Pos2, Rect, pos2, vec2};
use glam::{DMat4, DVec4};
use sim::battle::{AME_MAX, AME_PER, Battle, BossState, EnemyState, FIELD_R, GATE, Owner, PlayerState, Role};
use sim::moves::{CHARGES, is_normal};

/// 見出しの色（say の tone）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone { Plain, Gold, Red }

struct Banner { k: String, s: String, tone: Tone, at: f64 }
#[derive(Clone, Copy)]
struct Pop { x: f64, y: f64, z: f64, t: f64, s: f32, n: i64 }
#[derive(Clone, Copy)]
struct Reward { x: f64, z: f64, t: f64, kills: i64, bonus: i64 }

pub struct Hud {
    now: f64,
    visible: bool,
    /// 最後に触った機器（表記を合わせる。指のときはキーの札と下の説明を出さない）
    device: Device,
    /// 端の安全域（端の HUD をこの分だけ内へ寄せる）
    safe: Safe,
    /// 上に画面（一時停止・結果・敗北など）が出ている。下の操作の説明の列を描かない（説明は一時停止の表にある）
    covered: bool,
    name: String,
    face: Option<Tex>,
    // 飴（持っている数・次の 1 個のたまり具合・増えた合図・足りない合図）と、猪鹿蝶の札（めくった時刻。0 枚目 = 猪）
    // 札は斬った敵の頭の上に出す: 3 枚が並んだ時刻・的（-1 = 大将、ほかは敵の id）・札を出す場所（的について動く）
    fuda_born: f64,
    /// 猪鹿蝶の一閃（画面を斜めに横切る光の線）を出した時刻
    issen_at: Shot,
    /// 糸斬りの衝撃のコマ（画面を一瞬暗く締める）を出した時刻と強さ
    impact_at: Shot,
    impact_k: f32,
    /// 札を出しておく最後の時刻（続けられる間は進む。止まると消えていく）
    fuda_live: f64,
    /// 主役の場所（札と重なるかを見る）
    hero_pos: [f64; 3],
    fuda_target: i64,
    fuda_pos: [f64; 3],
    ame: i32,
    ame_fill: f32,
    ame_bump: Shot,
    ame_no: Shot,
    fuda: Vec<f64>,
    fuda_tex: [Option<Tex>; 3],
    rank: i32,
    rank_up: Shot,
    /// 面（大将・隊長の名前を面に合わせる）
    // 体力・ゲージ
    hp: Tween,
    hp_lag: Tween,
    mu: Tween,
    foe_fill: Tween,
    foe_op: Tween,
    foe_name: String,
    // 撃破・連撃
    last_ko: i64,
    ko: i64,
    ko_bump: Shot,
    last_combo: i64,
    combo_n: i64,
    combo_hot: bool,
    combo_hide: f64,
    combo_op: Tween,
    combo_pop: Shot,
    // 連鎖
    chain_id: i64,
    chain_hide: f64,
    chain_n: i64,
    chain_on: bool,
    chain_hot: bool,
    chain_k: f32,
    chain_op: Tween,
    chain_tick: Shot,
    chain_end_at: Shot,
    // 巻き込んだ場所に浮かぶ連鎖の数（決まった数の札を使い回す）
    pops: [Pop; 28],
    pop_i: usize,
    // 締めのご褒美: 網の真ん中に「+N 撃破」と連鎖の得点
    rewards: [Reward; 4],
    reward_i: usize,
    // 案内
    hint: String,
    hint_on: bool,
    hint_op: Tween,
    hint_bump: Shot,
    // 大きな知らせ
    banner: Option<Banner>,
    /// 大将の登場で、顔の寄りへ切り替わってからの秒（負 = まだ跳んでいる）
    boss_card: Option<f32>,
    foe_lag: Tween,
    // つなぎの道しるべ
    route_key: (i32, usize),
    route_hide: f64,
    route_op: Tween,
    route_chain: i32,
    route_next: &'static str,
    route_load: usize,
    pips: [Tween; 6],
    load: usize,
    frame: u64,
    help_t: f64,
    help_op: Tween,
    /// 窓の大きさ（論理。確かめ用の写しにも使う）
    #[allow(dead_code)]
    size: (f32, f32),
}

impl Default for Hud { fn default() -> Self { Self::new() } }

impl Hud {
    pub fn new() -> Self {
        Hud {
            ame: 0, ame_fill: 0.0, ame_bump: Shot::default(), ame_no: Shot::default(), fuda: Vec::new(), fuda_tex: [None, None, None], fuda_born: 0.0, issen_at: Shot::default(), impact_at: Shot::default(), impact_k: 0.0, fuda_live: 0.0, hero_pos: [0.0; 3], fuda_target: 0, fuda_pos: [0.0; 3],
            now: 0.0, visible: false, device: Device::Key, safe: Safe::default(), covered: false, name: String::new(), face: None, rank: 0, rank_up: Shot::default(),
            hp: Tween::new(1.0, 0.12, 0.0, css_ease), hp_lag: Tween::new(1.0, 0.6, 0.3, css_ease), mu: Tween::new(0.0, 0.15, 0.0, css_ease), foe_fill: Tween::new(1.0, 0.15, 0.0, css_ease), foe_op: Tween::new(0.0, 0.3, 0.0, css_ease), foe_name: String::new(),
            last_ko: -1, ko: 0, ko_bump: Shot::default(), last_combo: -1, combo_n: 0, combo_hot: false, combo_hide: 0.0, combo_op: Tween::new(0.0, 0.3, 0.0, css_ease), combo_pop: Shot::default(),
            chain_id: -1, chain_hide: 0.0, chain_n: 0, chain_on: false, chain_hot: false, chain_k: 1.0, chain_op: Tween::new(0.0, 0.35, 0.0, css_ease), chain_tick: Shot::default(), chain_end_at: Shot::default(),
            pops: [Pop { x: 0.0, y: 0.0, z: 0.0, t: 9.0, s: 1.0, n: 0 }; 28], pop_i: 0, rewards: [Reward { x: 0.0, z: 0.0, t: 9.0, kills: 0, bonus: 0 }; 4], reward_i: 0,
            hint: String::new(), hint_on: false, hint_op: Tween::new(0.0, 0.2, 0.0, css_ease), hint_bump: Shot::default(),
            banner: None, boss_card: None, foe_lag: Tween::new(1.0, 0.7, 0.35, css_ease), route_key: (-1, 0), route_hide: 0.0, route_op: Tween::new(0.0, 0.25, 0.0, css_ease), route_chain: 0, route_next: "", route_load: 0, pips: [Tween::new(0.0, 0.1, 0.0, css_ease); 6],
            load: 0, frame: 0, help_t: 0.0, help_op: Tween::new(1.0, 1.0, 0.0, css_ease), size: (1.0, 1.0),
        }
    }
    /// 最後に触った機器（今の版の body.touch の役）
    pub fn set_device(&mut self, d: Device) { self.device = d; }
    pub fn set_safe(&mut self, s: Safe) { self.safe = s; }
    /// 上に画面が出ているか（毎コマ渡す）
    pub fn set_covered(&mut self, on: bool) { self.covered = on; }
    /// 下の操作の説明の列を描くか: 指では出さない（画面のボタンが説明）。上に画面が出ている間も出さない
    fn help_shown(&self) -> bool { !self.covered && !controls::help_groups(self.device).is_empty() }
    pub fn show(&mut self, on: bool) {
        self.visible = on;
        if on { self.help_t = 0.0; self.help_op.jump(1.0); }
    }
    pub fn set_hero(&mut self, name: &str, thumb: Option<Tex>) { self.name = name.to_string(); self.face = thumb; }
    /// 大将の登場の進み（顔の寄りへ切り替わってからの秒）。名前の札はこれに合わせて出す
    pub fn set_boss_intro(&mut self, t: Option<f32>) { self.boss_card = t; }
    pub fn say(&mut self, kanji: &str, sub: &str, tone: Tone) { self.banner = Some(Banner { k: kanji.to_string(), s: sub.to_string(), tone, at: self.now }); }

    /// 群衆ドミノの連鎖数（3 体からだけ出す。いちばん新しい連鎖を見せる）
    pub fn chain(&mut self, id: i64, n: i64) {
        if n < 3 && id != self.chain_id { return; }
        self.chain_id = id;
        self.chain_n = n;
        self.chain_k = (0.85 + n as f32 / 60.0).min(1.6);
        self.chain_on = true;
        self.chain_hot = n >= 30;
        self.chain_end_at = Shot::default();
        self.chain_tick.fire(self.now);
        self.chain_hide = 1.2;
    }
    pub fn chain_pop(&mut self, x: f64, y: f64, z: f64, n: i64) {
        // 数が重ならないよう、5 の倍数だけ（道筋は光の線と金の光で読める）
        if n % 5 != 0 { return; }
        let i = self.pop_i % self.pops.len();
        self.pop_i += 1;
        self.pops[i] = Pop { x, y: y + 0.6, z, t: 0.0, s: ((1.1 + n as f64 / 30.0).min(2.2)) as f32, n };
    }
    pub fn chain_reward(&mut self, x: f64, z: f64, kills: i64, bonus: i64) {
        let i = self.reward_i % self.rewards.len();
        self.reward_i += 1;
        self.rewards[i] = Reward { x, z, t: 0.0, kills, bonus };
    }
    pub fn chain_end(&mut self, id: i64, n: i64) {
        if id != self.chain_id || n < 3 { return; }
        self.chain_tick = Shot::default();
        self.chain_end_at.fire(self.now);
        self.chain_hide = 1.1;
    }

    /// コマの始めに時計を進める（遊びの中で出す文字や札も、このコマの時刻から数える。今の版の CSS と同じ）
    pub fn advance(&mut self, d: f64) { self.now += d; }
    /// 面を替える（名前を面に合わせる）
    /// 格（名前の横に小さく。初陣は出さない）
    pub fn set_rank(&mut self, rank: i32) {
        self.rank = rank;
        if rank > 0 { self.rank_up.fire(self.now); }
    }
    /// 画面下の短い案内（空で消す）
    pub fn set_hint(&mut self, text: &str) {
        if text == self.hint && self.hint_on == !text.is_empty() { return; }
        let on = !text.is_empty();
        if on { self.hint = text.to_string(); if !self.hint_on { self.hint_bump.fire(self.now); } }
        self.hint_on = on;
    }
    /// 強攻撃の範囲にいる敵の数（装填）
    pub fn set_load(&mut self, n: usize) { self.load = n; }

    pub fn reset(&mut self) {
        self.last_ko = -1; self.last_combo = -1; self.combo_op.set(0.0, self.now); self.combo_hide = 0.0;
        self.chain_on = false; self.chain_id = -1;
        for q in &mut self.pops { q.t = 9.0; }
        for r in &mut self.rewards { r.t = 9.0; }
        self.banner = None;
        self.boss_card = None;
        self.ame = 0; self.ame_fill = 0.0; self.fuda.clear();
    }
    /// 飴が無くて飴糸が出なかった（飴の表示を赤く揺らす）
    pub fn ame_empty(&mut self) { self.ame_no.fire(self.now); }
    /// 猪鹿蝶の一閃（画面を斜めに横切る光の線）
    pub fn issen(&mut self) { self.issen_at.fire(self.now); }
    /// 糸斬りの衝撃のコマ（k = 強さ 0..1）
    pub fn impact(&mut self, k: f32) { self.impact_at.fire(self.now); self.impact_k = k; }
    /// 猪鹿蝶の札をめくる（n = 1 猪・2 鹿・3 蝶）。target = 斬った的（-1 = 大将、ほかは敵の id）、at = 斬った場所
    pub fn fuda(&mut self, n: u8, target: i64, at: [f64; 3]) {
        if n <= 1 { self.fuda.clear(); self.fuda_born = self.now; self.fuda_pos = [at[0], at[1] + 2.8, at[2]]; }
        self.fuda_target = target;
        if self.fuda.len() + 1 == n as usize { self.fuda.push(self.now); }
        self.fuda_live = self.now;
    }

    /// 毎コマ呼ぶ。advance = false は TS で update を呼ばない場面（敗北の画面など。値は止めて描くだけ）
    pub fn ui(&mut self, ctx: &Context, b: &Battle, dt: f64, camera: &Camera, cam_yaw: f64, advance: bool) -> Vec<UiAction> {
        let acts = vec![];
        self.now = ctx.input(|i| i.time);
        // three の project: 今の位置と前のコマの回転
        let pv = camera.project_pv();
        let cam_yaw = cam_yaw as f32;
        if advance { self.step(b, dt, &pv); }
        if !self.visible { return acts; }
        if self.fuda_tex[0].is_none() {
            self.fuda_tex = [
                load_image(ctx, "card-ino", include_bytes!("../assets/cards/ino.png")),
                load_image(ctx, "card-shika", include_bytes!("../assets/cards/shika.png")),
                load_image(ctx, "card-cho", include_bytes!("../assets/cards/cho.png")),
            ];
        }
        let pen = Pen::new(ctx, ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("hud"))));
        let scr = ctx.content_rect();
        self.size = (scr.width(), scr.height());
        let m = M { w: scr.width(), h: scr.height(), narrow: scr.width() <= 760.0, short: scr.height() <= 520.0, safe: self.safe, touch: self.device == Device::Touch };
        self.draw_top_left(&pen, &m);
        self.draw_top_center(&pen, b, &m);
        self.draw_map(&pen, b, cam_yaw, &m);
        self.draw_right(&pen, &m);
        self.draw_chain(&pen, &m);
        self.draw_impact(&pen, &m);
        self.draw_issen(&pen, &m);
        self.draw_fuda(&pen, &pv, &m);
        self.draw_pops(&pen, &pv, &m);
        self.draw_hint(&pen, &m);
        self.draw_banner(&pen, &m);
        self.draw_boss_card(&pen, &m);
        self.draw_route(&pen, &m);
        self.draw_help(&pen, &m);
        self.draw_tags(&pen, b, &pv, &m);
        acts
    }

    /// TS の update の、値を動かす部分
    fn step(&mut self, b: &Battle, dt: f64, pv: &DMat4) {
        let now = self.now;
        self.frame += 1;
        self.help_t += dt;
        self.chain_hide -= dt;
        if self.chain_hide <= 0.0 && self.chain_on { self.chain_on = false; self.chain_id = -1; }
        if self.help_t > 14.0 { self.help_op.set(0.0, now); }
        let p = &b.player;
        let r = (p.hp / p.max_hp) as f32;
        self.hp.set(r, now);
        self.hp_lag.set(r, now);
        self.mu.set((p.ranbu / 100.0) as f32, now);
        self.hero_pos = [p.x, p.y, p.z];
        // 札の場所: 的の頭の上（的が倒れたら、最後にいた場所のまま）
        if let Some(&last) = self.fuda.last() {
            // 続けられる間（3 枚そろったら 2 秒）は札を残す
            let going = p.wire_chain > 0 && (p.wire_chain_t > 0.0 || p.state == PlayerState::Wire);
            if going || now - last < if self.fuda.len() >= 3 { 2.05 } else { 0.5 } { self.fuda_live = now; }
            if self.fuda_target == -1 {
                if !matches!(b.boss.state, BossState::Dormant | BossState::Dead) { self.fuda_pos = [b.boss.x, b.boss_y + 6.0, b.boss.z]; }
            } else if let Some(e) = b.enemies.iter().find(|e| e.id == self.fuda_target && e.alive && e.state != EnemyState::Dead) {
                self.fuda_pos = [e.x, e.y + e.kind.def().height + 1.7, e.z];
            }
        }
        if p.ame > self.ame { self.ame_bump.fire(now); }
        self.ame = p.ame;
        self.ame_fill = p.ame_fill as f32 / AME_PER as f32;
        if p.kos != self.last_ko {
            self.ko = p.kos;
            if self.last_ko >= 0 { self.ko_bump.fire(now); }
            self.last_ko = p.kos;
        }
        if p.combo != self.last_combo {
            if p.combo > 1 {
                self.combo_n = p.combo;
                self.combo_op.set(1.0, now);
                self.combo_hot = p.combo >= 100;
                self.combo_pop.fire(now);
                self.combo_hide = 2.6;
            }
            self.last_combo = p.combo;
        }
        self.combo_hide -= dt;
        if self.combo_hide <= 0.0 { self.combo_op.set(0.0, now); }
        // つなぎの道しるべ（通常攻撃の最中だけ）
        let in_normal = p.state == PlayerState::Attack && p.r#move.is_some_and(is_normal);
        if in_normal { self.route_hide = 0.5; } else { self.route_hide -= dt; }
        let chain = if in_normal { p.chain } else { 0 };
        let key = (chain, self.load);
        if key != self.route_key && in_normal {
            self.route_key = key;
            for (i, t) in self.pips.iter_mut().enumerate() { t.set(if (i as i32) < chain { 1.0 } else { 0.0 }, now); }
            self.route_chain = chain;
            self.route_next = CHARGES[(chain as usize).min(CHARGES.len() - 1)].name;
            self.route_load = self.load;
        }
        self.route_op.set(if self.route_hide > 0.0 { 1.0 } else { 0.0 }, now);
        // 敵将の体力（いちばん近い隊長か大将）
        let mut foe_on = false;
        if boss_on(b) {
            foe_on = true;
            self.foe_name = format!("大将 {}", STAGE.boss());
            self.foe_fill.set((b.boss.hp / b.boss.max_hp) as f32, now);
            self.foe_lag.set((b.boss.hp / b.boss.max_hp) as f32, now);
        } else {
            let (mut best, mut bd) = (None, 40.0);
            for e in &b.enemies {
                if e.alive && e.role == Role::Captain && e.state != EnemyState::Dead {
                    let d = ((e.x - p.x).powi(2) + (e.z - p.z).powi(2)).sqrt();
                    if d < bd { bd = d; best = Some(e); }
                }
            }
            if let Some(e) = best {
                foe_on = true;
                self.foe_name = STAGE.kind_name(e.kind).to_string();
                self.foe_fill.set(((e.hp / e.max_hp) as f32).max(0.0), now);
            }
        }
        self.foe_op.set(if foe_on { 1.0 } else { 0.0 }, now);
        // 連鎖の数: ぽんと出て、少し浮いて消える
        for q in &mut self.pops {
            if q.t > 0.8 { continue; }
            q.t += dt;
            if q.t / 0.8 >= 1.0 || project(pv, q.x, q.y + q.t / 0.8 * 0.9, q.z).is_none() { q.t = 9.0; }
        }
        for r in &mut self.rewards {
            if r.t > 1.6 { continue; }
            r.t += dt;
            if r.t / 1.6 >= 1.0 || project(pv, r.x, 1.6 + r.t / 1.6 * 1.2, r.z).is_none() { r.t = 9.0; }
        }
    }

    fn draw_top_left(&self, pen: &Pen, m: &M) {
        let now = self.now;
        let face = if m.short { 46.0 } else { 64.0 };
        let name_fs = if m.short { 16.0 } else { 22.0 };
        let (hp_w, hp_h, mu_w, mu_h) = if m.short {
            // 低い画面: 棒の右端（格の字）が、上の真ん中の目標の札へ入らない長さまで（安全域で右へ寄るぶん短くする）
            let room = m.w * 0.5 - 116.0 - (22.0 + m.safe.left + face + 20.0);
            let hp = (m.w * 0.3).min(room).max(120.0);
            (hp, 9.0, (m.w * 0.24).min(hp * 0.8), 12.0)
        } else { (360.0_f32.min(m.w * 0.34), 12.0, 300.0_f32.min(m.w * 0.28), 16.0) };
        // 飴玉の列（半径）
        let ar = if m.short { 5.0 } else { 6.5 };
        let ame_h = ar * 2.0 + 2.0;
        // 名前・体力・ゲージ・飴を縦に（間 6px）
        let col_h: f32 = name_fs + 6.0 + hp_h + 6.0 + mu_h + 6.0 + ame_h;
        let row_h = col_h.max(face);
        let (top, left) = (18.0 + m.safe.top, 22.0 + m.safe.left);
        let cy = top + row_h * 0.5;
        // 顔（45 度回した札）。絵は回さず菱形に切る
        let fc = pos2(left + face * 0.5, cy);
        let bx = pen.sub(Xf::about(fc.x, fc.y, Xf::rot(45.0)), 1.0);
        let fr = Rect::from_center_size(fc, vec2(face, face));
        bx.soft(fr, 0.0, 18.0, rgba(255, 201, 74, 0.35), false);
        bx.rect(fr, INK2);
        if let Some(t) = &self.face {
            let d = (face * 0.5 - 2.0) * std::f32::consts::SQRT_2;
            let img = Rect::from_center_size(fc, vec2((face - 4.0) * 1.42, (face - 4.0) * 1.42));
            pen.image_poly(t.handle.id(), &[pos2(fc.x, fc.y - d), pos2(fc.x + d, fc.y), pos2(fc.x, fc.y + d), pos2(fc.x - d, fc.y)], img, cover_uv(t, img.width(), img.height(), 0.5, 0.5), WHITE);
        }
        bx.frame(fr, 2.0, GOLD);
        // 棒の欄
        let x = left + face + 12.0 + 8.0;
        let mut y = cy - col_h * 0.5;
        let tn = pen.t(&self.name, Fam::Brush, name_fs, 0.0);
        pen.text(&tn, x, y + name_fs * 0.5 + name_fs * BASE_K, CREAM, &[SHADOW8]);
        // 格 壱・弐・参（棒の右端の上）
        if self.rank > 0 {
            let label = ["", "格 壱", "格 弐", "格 参"][self.rank.clamp(0, 3) as usize];
            let tr = pen.t(label, Fam::Brush, 18.0, 0.0);
            let (rx, rcy) = (x + hp_w - tr.w, y - 2.0 + 18.0 * LH * 0.5);
            let e = snap(self.rank_up.k(now, 0.5));
            let sc = lerp(1.25, 1.0, e);
            let q = pen.sub(Xf::about(rx + tr.w * 0.5, rcy, Xf::scale(sc, sc)), 1.0);
            q.text(&tr, rx, rcy + 18.0 * BASE_K, GOLD, &[TS(0.0, 0.0, 12.0, rgba(255, 180, 60, 0.8)), TS(0.0, 2.0, 6.0, rgba(0, 0, 0, 0.9))]);
        }
        y += name_fs + 6.0;
        // 体力
        let bar = Rect::from_min_size(pos2(x, y), vec2(hp_w, hp_h));
        let sk = bar_pen(pen, bar);
        bar_base(&sk, bar, rgba(255, 243, 224, 0.25));
        let inner = bar.shrink(1.0);
        fill_rect(&sk, inner, clamp01(self.hp_lag.get(now)), |p, r| p.rect(r, hx(0xff4a4a)));
        let low = self.hp.target() < 0.3;
        let kb = if low {
            // lowhp .7s ease-in-out infinite: 50% brightness(1.6)
            let t = (now as f32 / 0.7).rem_euclid(1.0);
            lerp(1.0, 1.6, if t < 0.5 { ease_io(t * 2.0) } else { 1.0 - ease_io((t - 0.5) * 2.0) })
        } else { 1.0 };
        let (c0, c1) = if low { (hx(0xff9a7a).bright(kb), hx(0xff3a3a).bright(kb)) } else { (hx(0x8affb0), hx(0x22c46a)) };
        fill_rect(&sk, inner, clamp01(self.hp.get(now)), |p, r| p.vgrad(r, &[(0.0, c0), (1.0, c1)]));
        y += hp_h + 6.0;
        // 乱舞ゲージ
        let bar = Rect::from_min_size(pos2(x, y), vec2(mu_w, mu_h));
        let sk = bar_pen(pen, bar);
        let full = self.mu.target() >= 1.0;
        if full {
            let g = full_glow(now, 0.6);
            sk.soft(bar, 0.0, lerp(18.0, 30.0, g), rgba(255, 201, 74, 0.8).lerp(rgba(255, 160, 40, 1.0), g), false);
        }
        bar_base(&sk, bar, if full { GOLD } else { rgba(255, 243, 224, 0.25) });
        let inner = bar.shrink(1.0);
        let k = clamp01(self.mu.get(now));
        fill_rect(&sk, inner, k, |p, r| p.hgrad(r, &[(0.0, hx(0xff7a2a)), (0.7, GOLD), (1.0, hx(0xfff3b0))]));
        if k > 0.0 {
            // 流れる光（i の中の 40% の幅が -40%〜140% を動く）。i ごと縮むので、ゲージの先で切れる
            let t = (now as f32 / 1.4).rem_euclid(1.0);
            let pos = lerp(-0.4, 1.4, t) * 0.6;
            let (a, b_) = (pos.max(0.0), (pos + 0.4).min(1.0));
            if b_ > a {
                let shine = |u: f32| rgba(255, 255, 255, 0.7 * (1.0 - ((u - (pos + 0.2)) / 0.2).abs()).max(0.0));
                let map = |u: f32| inner.left() + inner.width() * k * u;
                let mid = (pos + 0.2).clamp(a, b_);
                let r = Rect::from_min_max(pos2(map(a), inner.top()), pos2(map(b_), inner.bottom()));
                let w = (b_ - a).max(1e-4);
                sk.hgrad(r, &[(0.0, shine(a)), ((mid - a) / w, shine(mid)), (1.0, shine(b_))]);
            }
        }
        if full {
            // 満タンの札（傾きを打ち消す）
            let mut items = vec![P::T(pen.t("乱舞", Fam::Brush, 13.0, 0.0), hx(0x2a0a10), vec![])];
            if !m.touch { items.push(P::K(Kbd::new(pen, controls::label(Act::Ranbu, self.device), 13.0, 0.0, hx(0x2a0a10), rgba(255, 243, 224, 0.45)))); }
            let w = row_w(&items);
            let lx = inner.right() - 6.0 - w;
            let cy = inner.center().y - 1.0;
            let q = sk.sub(Xf::about(lx + w * 0.5, cy, Xf::skew_x(24.0)), 1.0);
            row_draw(&q, &items, lx, cy + 13.0 * BASE_K);
        }
        y += mu_h + 6.0;
        // 飴（飴糸の元手）: 飴玉を AME_MAX 個。持っている分が光り、次の 1 個はたまり具合でふくらむ。足りないときは赤く揺れる
        let no_k = self.ame_no.k(now, 0.35);
        let shake = if no_k < 1.0 { (no_k * 40.0).sin() * 4.0 * (1.0 - no_k) } else { 0.0 };
        for i in 0..AME_MAX {
            let c = pos2(x + ar + 5.0 + i as f32 * (ar * 2.0 + 11.0) + shake, y + ame_h * 0.5);
            let have = i < self.ame;
            let pop = if have && i == self.ame - 1 { lerp(1.7, 1.0, snap(self.ame_bump.k(now, 0.3))) } else { 1.0 };
            let q = pen.sub(Xf::about(c.x, c.y, Xf::scale(pop, pop)), 1.0);
            let (body, edge) = if have { (hx(0xffae38), hx(0xfff0b0)) } else if no_k < 1.0 { (rgba(120, 30, 30, 0.75), rgba(255, 96, 96, 0.95)) } else { (rgba(10, 6, 20, 0.6), rgba(255, 243, 224, 0.32)) };
            if have { q.soft(Rect::from_center_size(c, vec2(ar * 2.0, ar * 2.0)), ar, 8.0, rgba(255, 170, 50, 0.55), false); }
            // 包みの両端（飴玉に見せる）
            for sx in [-1.0_f32, 1.0] { q.poly(&[pos2(c.x + sx * (ar - 1.0), c.y), pos2(c.x + sx * (ar + 4.5), c.y - 3.6), pos2(c.x + sx * (ar + 4.5), c.y + 3.6)], edge); }
            q.circle(c, ar, edge);
            q.circle(c, ar - 1.3, body);
            if i == self.ame && self.ame_fill > 0.0 { q.circle(c, (ar - 1.3) * self.ame_fill.sqrt(), rgba(255, 174, 56, 0.6)); }
            if have { q.circle(pos2(c.x - ar * 0.3, c.y - ar * 0.32), ar * 0.26, rgba(255, 255, 255, 0.9)); }
        }
    }

    /// 猪鹿蝶の札: 斬った敵の頭の上に裏向きの札が 3 枚並び、糸斬りのたびに 1 枚ずつめくれる（猪 → 鹿 → 蝶）。続けられる間は残り、3 枚そろうと光る
    fn draw_fuda(&self, pen: &Pen, pv: &DMat4, m: &M) {
        if self.fuda.is_empty() { return; }
        let now = self.now;
        let n = self.fuda.len();
        let age = (now - self.fuda_live) as f32;
        if age > 0.35 { return; }
        let Some((nx, ny)) = project(pv, self.fuda_pos[0], self.fuda_pos[1], self.fuda_pos[2]) else { return };
        let out = 1.0 - (age / 0.35).clamp(0.0, 1.0);
        let h = if m.short { 60.0 } else if m.narrow { 72.0 } else { 104.0_f32.min(m.h * 0.15) };
        let (w, gap) = (h * 8.0 / 13.0, h * 0.09);
        let total = w * 3.0 + gap * 2.0;
        // 敵の頭の上（画面の外へは出さない）
        let cx = ((nx * 0.5 + 0.5) * m.w).clamp(total * 0.5 + 12.0 + m.safe.left, m.w - total * 0.5 - 12.0 - m.safe.right);
        let cy = ((-ny * 0.5 + 0.5) * m.h - h * 0.5).clamp(h * 0.5 + 70.0 + m.safe.top, m.h * 0.72);
        // 並ぶとき: 3 枚が上から落ちてくる
        let born = (now - self.fuda_born) as f32;
        // 主役と重なるときは薄くする（空へ逃げた主役が札に隠れない）
        let mut veil = 1.0;
        if let Some((hx_, hy_)) = project(pv, self.hero_pos[0], self.hero_pos[1] + 0.8, self.hero_pos[2]) {
            let (sx, sy) = ((hx_ * 0.5 + 0.5) * m.w, (-hy_ * 0.5 + 0.5) * m.h);
            if (sx - cx).abs() < total * 0.5 + 30.0 && (sy - cy).abs() < h * 0.5 + 50.0 { veil = 0.3; }
        }
        let out = out * veil;
        // 決め絵: 3 枚そろって扇に開く間だけ、まわりを暗くして札を立たせる
        if n >= 3 {
            let t3 = (now - self.fuda[2]) as f32;
            let dim = ((t3 - 0.22) / 0.12).clamp(0.0, 1.0) * (1.0 - ((t3 - 1.05) / 0.35).clamp(0.0, 1.0));
            if dim > 0.0 { pen.rect(Rect::from_min_size(pos2(0.0, 0.0), vec2(m.w, m.h)), rgba(14, 6, 22, 0.55 * dim)); }
        }
        for i in 0..3 {
            let drop = snap(((born - i as f32 * 0.04) / 0.22).clamp(0.0, 1.0));
            let rest = [-6.0, 0.0, 6.0][i] * if n >= 3 { 1.0 + 2.2 * snap((((now - self.fuda[2]) as f32 - 0.3) / 0.28).clamp(0.0, 1.0)) } else { 1.0 };
            // めくれる: 横幅が縮んで裏から表へ返り、そのあいだ札が跳ねる
            let flip = self.fuda.get(i).map(|&at| ((now - at) as f32 / 0.34).clamp(0.0, 1.0));
            let (turn, hop) = match flip { Some(t) => (t, (t * std::f32::consts::PI).sin()), None => (0.0, 0.0) };
            let sx = (turn * std::f32::consts::PI).cos().abs().max(0.04);
            // 決め絵: 3 枚そろうと、少しあとで扇に開いて大きくなる
            let fan = if n >= 3 { snap((((now - self.fuda[2]) as f32 - 0.3) / 0.28).clamp(0.0, 1.0)) } else { 0.0 };
            // 扇に開くとき、札は画面の真ん中へ寄って大きくなる（決め絵の主役。ChatGPT の講評 2026-11-15）
            let sc = lerp(0.4, 1.0, drop) * (1.0 + 0.3 * hop) * (1.0 + 1.15 * fan);
            let fx = (i as f32 - 1.0) * (w + gap) * (1.0 + 1.5 * fan);
            let (cx, cy) = (lerp(cx, m.w * 0.5, fan), lerp(cy, m.h * 0.4, fan));
            let c = pos2(cx + fx, cy + [4.0, -4.0, 4.0][i] - (1.0 - drop) * 26.0 - hop * h * 0.3 - fan * h * 0.35 + fan * [1.0, 0.0, 1.0][i] * h * 0.18);
            let r = Rect::from_center_size(c, vec2(w, h));
            let q = pen.sub(Xf::about(c.x, c.y, Xf::scale(sx * sc, sc).then(Xf::rot(rest + hop * 10.0))), drop.clamp(0.0, 1.0) * out);
            if turn > 0.5 {
                if n >= 3 { q.soft(r, 6.0, lerp(18.0, 30.0, full_glow(now, 0.5)), rgba(255, 201, 74, 0.95), false); } else { q.soft(r, 6.0, 10.0, rgba(0, 0, 0, 0.55), false); }
                if let Some(t) = &self.fuda_tex[i] { q.image(t, r, WHITE); }
                // めくれた瞬間の白い光
                if turn < 1.0 { q.rect(r, rgba(255, 255, 255, 0.9 * (1.0 - (turn - 0.5) * 2.0))); }
            } else {
                // 裏（黒い縁に朱の地）
                q.soft(r, 6.0, 10.0, rgba(0, 0, 0, 0.55), false);
                q.rect(r, hx(0x17110f));
                q.rect(r.shrink(w * 0.07), hx(0x8f2a1c));
                q.frame(r.shrink(w * 0.16), 1.5, rgba(255, 224, 170, 0.55));
            }
        }
    }

    /// 糸斬りの衝撃のコマ: 当たった瞬間だけ画面を暗く締め、すぐ戻す（光の斬線が浮き立つ）
    fn draw_impact(&self, pen: &Pen, m: &M) {
        let k = self.impact_at.k(self.now, 0.16);
        if k >= 1.0 { return; }
        let a = (if k < 0.35 { 1.0 } else { 1.0 - (k - 0.35) / 0.65 }) * self.impact_k * 0.6;
        pen.rect(Rect::from_min_size(pos2(0.0, 0.0), vec2(m.w, m.h)), rgba(12, 6, 20, a));
    }

    /// 猪鹿蝶の一閃: 画面を斜めに横切る光の線。0.1 秒で走り抜けて、0.5 秒で細く消える
    fn draw_issen(&self, pen: &Pen, m: &M) {
        let k = self.issen_at.k(self.now, 0.55);
        if k >= 1.0 { return; }
        let t = k * 0.55;
        let run = (t / 0.1).clamp(0.0, 1.0);
        let fade = 1.0 - ((t - 0.1) / 0.45).clamp(0.0, 1.0);
        let (cx, cy) = (m.w * 0.5, m.h * 0.45);
        let len = m.w * 1.5;
        let q = pen.sub(Xf::about(cx, cy, Xf::rot(-17.0)), fade);
        let x0 = cx - len * 0.5;
        let x1 = x0 + len * snap(run);
        let thick = lerp(3.0, 16.0, fade) * if m.short { 0.7 } else { 1.0 };
        // 外の光（飴色）・芯（白）。先端は細くとがる
        for (th, col) in [(thick * 3.2, rgba(255, 176, 60, 0.35)), (thick * 1.6, rgba(255, 226, 150, 0.7)), (thick * 0.55, rgba(255, 255, 255, 1.0))] {
            q.poly(&[pos2(x0, cy), pos2(x0 + (x1 - x0) * 0.15, cy - th * 0.5), pos2(x1 - th * 2.0, cy - th * 0.5), pos2(x1, cy), pos2(x1 - th * 2.0, cy + th * 0.5), pos2(x0 + (x1 - x0) * 0.15, cy + th * 0.5)], col);
        }
    }

    fn draw_top_center(&self, pen: &Pen, b: &Battle, m: &M) {
        let now = self.now;
        // 縦持ち（幅 760 以下）は、名前・体力（左）と小地図（右）の下の段に置く（同じ段では重なる）
        let (fs, pv, ph, top) = if m.short { (12.0, 4.0, 14.0, 10.0) } else if m.portrait() { (12.0, 4.0, 14.0, 14.0 + map_size(m.w, m.h) + 8.0) } else { (15.0, 6.0, 22.0, 18.0) };
        let top = top + m.safe.top;
        // その段の右端に Ⅱ があるときは、残りの幅の真ん中へ
        let cx = if m.portrait() && m.touch { (m.safe.left + pause_hit_rect(m.w, m.h, m.safe).left() - 6.0) * 0.5 } else { m.w * 0.5 };
        let sp = fs * 0.15;
        let cream = |s: &str| P::T(pen.t(s, Fam::Mincho800, fs, sp), CREAM, vec![]);
        let gold = |s: &str| P::T(pen.t(s, Fam::Mincho800, fs, sp), GOLD, vec![]);
        // 目標
        let left = b.holds.iter().filter(|x| x.owner == Owner::Enemy).count();
        let obj: Vec<P> = if boss_on(b) { vec![cream("大将 "), gold(STAGE.boss()), cream(" を討て")] }
            else if b.gate_open { vec![cream("本陣の門が開いた — "), gold("北の本陣"), cream("へ攻め込め")] }
            else { vec![cream("敵の砦を落とせ "), gold(&format!("残り {left}"))] };
        let w = row_w(&obj);
        let h = fs * LH + pv * 2.0;
        let r = Rect::from_min_size(pos2(cx - w * 0.5 - ph, top), vec2(w + ph * 2.0, h));
        let ink = rgba(10, 6, 20, 0.75);
        pen.hgrad(r, &[(0.0, CLEAR), (0.2, ink), (0.8, ink), (1.0, CLEAR)]);
        row_draw(pen, &obj, r.left() + ph, r.center().y + fs * BASE_K);
        // 敵将
        let op = self.foe_op.get(now);
        if op > 0.0 {
            let q = pen.with_alpha(op);
            let mut y = r.bottom() + 10.0;
            let tn = q.t(&self.foe_name, Fam::Brush, 20.0, 0.0);
            q.text(&tn, m.w * 0.5 - tn.w * 0.5, y + 20.0 * LH * 0.5 + 20.0 * BASE_K, CREAM, &[SHADOW8]);
            y += 20.0 * LH + 4.0;
            let (bw, bh) = if m.short { (m.w * 0.4, 9.0) } else { (520.0_f32.min(m.w * 0.5), 10.0) };
            let bar = Rect::from_min_size(pos2(m.w * 0.5 - bw * 0.5, y), vec2(bw, bh));
            let sk = bar_pen(&q, bar);
            bar_base(&sk, bar, rgba(255, 243, 224, 0.25));
            // 大将: 減った分が遅れて減る帯と、半分の印
            if boss_on(b) { fill_rect(&sk, bar.shrink(1.0), clamp01(self.foe_lag.get(now)), |p, r| p.rect(r, hx(0xffe0a0))); }
            fill_rect(&sk, bar.shrink(1.0), clamp01(self.foe_fill.get(now)), |p, r| p.vgrad(r, &[(0.0, hx(0xff7a8a)), (1.0, hx(0xd8203a))]));
            if boss_on(b) { q.rect(Rect::from_min_size(pos2(bar.center().x - 1.0, bar.top() - 3.0), vec2(2.0, bh + 6.0)), CREAM); }
        }
    }

    /// 小地図（カメラの向きを上に。見ている方向がいつも上）
    fn draw_map(&self, pen: &Pen, b: &Battle, cam_yaw: f32, m: &M) {
        let size = map_size(m.w, m.h);
        let (ox, oy) = { let o = map_origin(m.w, m.h, m.safe); (o.x, o.y) };
        let center = pos2(ox + size * 0.5, oy + size * 0.5);
        // キャンバスは 200×200（CSS で縮めて表示する）
        let s = (size - 2.0) / 200.0;
        pen.circle(center, size * 0.5, rgba(10, 6, 20, 0.55));
        inset_ring(pen, center, size * 0.5, 30.0, rgba(0, 0, 0, 0.6));
        pen.circle_line(center, size * 0.5 - 0.5, 1.0, LINE);
        let k = 94.0 / FIELD_R as f32;
        let clip_r = 98.0_f32;
        let (sn, cs) = cam_yaw.sin_cos();
        let to = |x: f32, y: f32| pos2(center.x + (x * cs - y * sn) * s, center.y + (x * sn + y * cs) * s);
        let p = &b.player;
        let (px0, pz0) = (p.x as f32, p.z as f32);
        let ss = |x: f32, z: f32| ((x - px0 * 0.35) * k, (z - pz0 * 0.35) * k);
        // 円の外は切る（点を円の内へ寄せる）
        let keep = |x: f32, y: f32| { let d = (x * x + y * y).sqrt(); if d > clip_r { (x / d * clip_r, y / d * clip_r) } else { (x, y) } };
        let ring = |x: f32, y: f32, r: f32, n: usize| -> Vec<(f32, f32)> { (0..n).map(|i| { let a = i as f32 / n as f32 * std::f32::consts::TAU; (x + a.cos() * r, y + a.sin() * r) }).collect() };
        let stroke_ring = |pts: &[(f32, f32)], w: f32, c: C| {
            let n = pts.len();
            let mut cur: Vec<Pos2> = vec![];
            for i in 0..=n {
                let (x, y) = pts[i % n];
                if x * x + y * y <= clip_r * clip_r { cur.push(to(x, y)); }
                else { if cur.len() > 1 { pen.line(&cur, w * s, c); } cur.clear(); }
            }
            if cur.len() > 1 { pen.line(&cur, w * s, c); }
        };
        {
            let (x, y) = ss(0.0, 0.0);
            stroke_ring(&ring(x, y, FIELD_R as f32 * k, 96), 1.0, rgba(255, 243, 224, 0.15));
        }
        for h in &b.holds {
            let (x, y) = ss(h.x as f32, h.z as f32);
            let ally = h.owner == Owner::Ally;
            let pts = ring(x, y, h.r as f32 * k, 28);
            let fill: Vec<Pos2> = pts.iter().map(|(a, b)| { let q = keep(*a, *b); to(q.0, q.1) }).collect();
            pen.poly(&fill, if ally { rgba(255, 201, 74, 0.35) } else { rgba(255, 58, 90, 0.3) });
            stroke_ring(&pts, 1.5, if ally { hx(0xffc94a) } else { hx(0xff3a5a) });
        }
        {
            let (x, y) = ss(GATE.x as f32, GATE.z as f32);
            let cs4 = [(x - 9.0, y - 3.0), (x + 9.0, y - 3.0), (x + 9.0, y + 3.0), (x - 9.0, y + 3.0)];
            let q: Vec<Pos2> = cs4.iter().map(|(a, b)| { let c = keep(*a, *b); to(c.0, c.1) }).collect();
            pen.poly(&q, if b.gate_open { hx(0xff3a5a) } else { rgba(255, 58, 90, 0.4) });
        }
        // 敵（2×2 の四角をまとめて 1 つのメッシュへ。隊長は金の丸）
        let dot = rgba(255, 90, 110, 0.85);
        let mut mesh = Mesh::default();
        let col = dot.alpha(pen.alpha);
        let (ex, ey) = (vec2(cs, sn) * s, vec2(-sn, cs) * s);
        for e in &b.enemies {
            if !e.alive || e.state == EnemyState::Dead || e.role == Role::Captain { continue; }
            let (x, y) = ss(e.x as f32, e.z as f32);
            if x * x + y * y > clip_r * clip_r { continue; }
            let c = pen.xf.ap(to(x, y));
            let i = mesh.vertices.len() as u32;
            let cc = c32(col);
            mesh.colored_vertex(c - ex - ey, cc);
            mesh.colored_vertex(c + ex - ey, cc);
            mesh.colored_vertex(c - ex + ey, cc);
            mesh.colored_vertex(c + ex + ey, cc);
            mesh.add_triangle(i, i + 1, i + 2);
            mesh.add_triangle(i + 2, i + 1, i + 3);
        }
        if !mesh.is_empty() { pen.painter.add(egui::Shape::mesh(mesh)); }
        for e in &b.enemies {
            if !e.alive || e.state == EnemyState::Dead || e.role != Role::Captain { continue; }
            let (x, y) = ss(e.x as f32, e.z as f32);
            if x * x + y * y > clip_r * clip_r { continue; }
            pen.circle(to(x, y), 4.0 * s, GOLD);
        }
        // 士気崩壊: 逃げる群れの真ん中から逃げ先の砦へ矢印
        for h in &b.holds {
            let (mut n, mut sx, mut sz) = (0, 0.0_f64, 0.0_f64);
            for e in &b.enemies { if e.alive && e.state != EnemyState::Dead && e.flee == h.id { n += 1; sx += e.x; sz += e.z; } }
            if n < 3 { continue; }
            let (ax, ay) = ss((sx / n as f64) as f32, (sz / n as f64) as f32);
            let (bx, by) = ss(h.x as f32, h.z as f32);
            let (dx, dy) = (bx - ax, by - ay);
            let l = (dx * dx + dy * dy).sqrt();
            let hr = h.r as f32 * k;
            if l < hr + 6.0 { continue; }
            let (ux, uy) = (dx / l, dy / l);
            let (ex2, ey2) = (bx - ux * (hr + 2.0), by - uy * (hr + 2.0));
            // 破線（5, 4）が流れる
            let total = ((ex2 - ax).powi(2) + (ey2 - ay).powi(2)).sqrt();
            let mut d = (self.frame as f32 * 0.6).rem_euclid(9.0) - 9.0;
            while d < total {
                let (d0, d1) = (d.max(0.0), (d + 5.0).min(total));
                if d1 > d0 {
                    let (a0, a1) = (keep(ax + ux * d0, ay + uy * d0), keep(ax + ux * d1, ay + uy * d1));
                    pen.line(&[to(a0.0, a0.1), to(a1.0, a1.1)], 2.5 * s, GOLD);
                }
                d += 9.0;
            }
            let tip = [keep(ex2 + ux * 6.0, ey2 + uy * 6.0), keep(ex2 - uy * 5.0, ey2 + ux * 5.0), keep(ex2 + uy * 5.0, ey2 - ux * 5.0)];
            pen.poly(&[to(tip[0].0, tip[0].1), to(tip[1].0, tip[1].1), to(tip[2].0, tip[2].1)], GOLD);
        }
        if boss_on(b) {
            let (x, y) = ss(b.boss.x as f32, b.boss.z as f32);
            let (x, y) = keep(x, y);
            pen.circle(to(x, y), 6.0 * s, hx(0xff2a3a));
            pen.circle_line(to(x, y), 6.0 * s, 1.5 * s, WHITE);
        }
        // 主役
        let (x, y) = ss(px0, pz0);
        let a = -(p.face as f32) + std::f32::consts::PI;
        let (s2, c2) = a.sin_cos();
        let arrow = |vx: f32, vy: f32| { let (qx, qy) = (x + vx * c2 - vy * s2, y + vx * s2 + vy * c2); to(qx, qy) };
        pen.poly(&[arrow(0.0, -7.0), arrow(5.0, 5.0), arrow(0.0, 2.0)], CREAM);
        pen.poly(&[arrow(0.0, -7.0), arrow(0.0, 2.0), arrow(-5.0, 5.0)], CREAM);
    }

    fn draw_right(&self, pen: &Pen, m: &M) {
        let now = self.now;
        let (ko_fs, ko_small, combo_fs, combo_small) = if m.short { (28.0, 14.0, 38.0, 16.0) } else if m.narrow { (34.0, 20.0, 46.0, 22.0) } else { (46.0, 20.0, 64.0, 22.0) };
        let top = m.right_top();
        let right_margin = if m.narrow && !m.short { 16.0 } else { 26.0 } + m.safe.right;
        // 撃破
        let (tb, tb_w) = pen.tabular(&self.ko.to_string(), ko_fs);
        let ts = pen.t("撃破", Fam::Brush, ko_small, 0.0);
        let row_w = ts.w + 8.0 + tb_w;
        // .hud-r は右寄せ。低い画面だけ左寄せ（left 22px）
        let x_end = |w: f32| if m.short { 22.0 + m.safe.left + w } else { m.w - right_margin };
        let (right, base) = (x_end(row_w), top + ko_fs * 0.5 + ko_fs * BASE_K);
        pen.text(&ts, right - row_w, base, CREAM_DIM, &[]);
        let e = snap(self.ko_bump.k(now, 0.25));
        let sc = lerp(1.25, 1.0, e);
        let cx = right - tb_w * 0.5;
        let q = pen.sub(Xf::about(cx, top + ko_fs * 0.5, Xf::scale(sc, sc)), 1.0);
        let shadows = [TS(0.0, 3.0, 0.0, rgba(90, 20, 40, 0.8)), TS(0.0, 0.0, 20.0, rgba(255, 120, 60, 0.4))];
        for (d, off) in &tb { q.text(d, right - tb_w + off, base, GOLD.lerp(CREAM, e.clamp(0.0, 1.0)), &shadows); }
        // 連撃
        let op = self.combo_op.get(now);
        if op > 0.0 {
            let y = top + ko_fs + 4.0;
            let tc = pen.t(&self.combo_n.to_string(), Fam::Num, combo_fs, 0.0);
            let th = pen.t("HIT", Fam::Num, combo_small, 0.0);
            let w = tc.w + 6.0 + th.w;
            let right = x_end(w);
            let e = snap(self.combo_pop.k(now, 0.18));
            let (sc, rot) = (lerp(1.35, 1.0, e), lerp(-4.0, 0.0, e));
            let cy = y + combo_fs * 0.5;
            let q = pen.sub(Xf::about(right, cy, Xf::scale(sc, sc).then(Xf::rot(rot))), op);
            let base = cy + combo_fs * BASE_K;
            let normal: &[(f32, C)] = &[(0.0, hx(0xfff6c8)), (0.5, GOLD), (1.0, hx(0xff6a2a))];
            let hot: &[(f32, C)] = &[(0.0, WHITE), (0.4, hx(0xff9ad0)), (1.0, hx(0xff2a5a))];
            q.text_f(&tc, right - w, base, Fill::V(if self.combo_hot { hot } else { normal }), &[TS(0.0, 3.0, 0.0, rgba(90, 20, 40, 0.9))], 0.0);
            q.text(&th, right - th.w, base, GOLD, &[]);
        }
    }

    /// 群衆ドミノの連鎖数: 増えるたびに跳ね、大きいほど大きく熱く
    fn draw_chain(&mut self, pen: &Pen, m: &M) {
        let now = self.now;
        // on の間は .05s で出て、消えるときは .35s（TS の transition と同じ）
        let target = if self.chain_on { 1.0 } else { 0.0 };
        if target != self.chain_op.target() {
            let cur = self.chain_op.get(now);
            self.chain_op = Tween::new(cur, if self.chain_on { 0.05 } else { 0.35 }, 0.0, css_ease);
            self.chain_op.set(target, now);
        }
        let op = self.chain_op.get(now);
        if op <= 0.0 { return; }
        // 縦持ちは、撃破数（右）と同じ段の左寄り（真ん中では撃破数と重なる）
        let (fs_b, fs_s, top) = if m.short { (40.0, 18.0, m.h * 0.12 + m.safe.top) } else if m.narrow { (50.0, 22.0, m.right_top()) } else { (72.0, 30.0, m.h * 0.15 + m.safe.top) };
        let mid = if m.portrait() { m.w * 0.36 } else { m.w * 0.5 };
        let (tb, tb_w) = pen.tabular(&self.chain_n.to_string(), fs_b);
        let ts = pen.t("連鎖", Fam::Brush, fs_s, 0.0);
        let w = ts.w + 10.0 + tb_w;
        let cy = top + fs_b * 0.5;
        let q = pen.sub(Xf::about(mid, cy, Xf::scale(self.chain_k, self.chain_k)), op);
        let x = mid - w * 0.5;
        let base = cy + fs_b * BASE_K;
        q.text(&ts, x, base, CREAM, &[TS(0.0, 2.0, 10.0, rgba(0, 0, 0, 0.9))]);
        // 跳ねる数字
        let (mut sc, mut dy, mut bright, mut glow) = (1.0, 0.0, 1.0, 18.0);
        if self.chain_tick.0.is_some() {
            let e = snap(self.chain_tick.k(now, 0.14));
            sc = lerp(1.3, 1.0, e);
            dy = lerp(-4.0, 0.0, e);
        }
        if self.chain_end_at.0.is_some() {
            let e = snap(self.chain_end_at.k(now, 0.5));
            sc = lerp(1.7, 1.0, e);
            bright = lerp(2.0, 1.0, e);
            glow = lerp(30.0, 18.0, e);
        }
        let bx = x + ts.w + 10.0;
        let num = q.sub(Xf::about(bx + tb_w * 0.5, cy, Xf::tr(0.0, dy).then(Xf::scale(sc, sc))), 1.0);
        let stops: Vec<(f32, C)> = if self.chain_hot { vec![(0.0, WHITE), (0.4, hx(0xffb0e0)), (1.0, hx(0xff2a5a))] } else { vec![(0.0, WHITE), (0.45, hx(0xffe08a)), (1.0, hx(0xff7a2a))] };
        let stops: Vec<(f32, C)> = stops.into_iter().map(|(u, c)| (u, c.bright(bright))).collect();
        for (d, off) in &tb { num.text_f(d, bx + off, base, Fill::V(&stops), &[TS(0.0, 4.0, 0.0, rgba(90, 20, 40, 0.9)), TS(0.0, 0.0, glow, rgba(255, 140, 40, 0.55))], 0.0); }
    }

    fn draw_pops(&self, pen: &Pen, pv: &DMat4, m: &M) {
        let (w, h) = (m.w, m.h);
        for q in &self.pops {
            if q.t > 0.8 { continue; }
            let k = q.t / 0.8;
            let Some((nx, ny)) = project(pv, q.x, q.y + k * 0.9, q.z) else { continue };
            let sc = q.s * if q.t < 0.08 { (1.6 - q.t / 0.08 * 0.6) as f32 } else { 1.0 };
            let op = if k < 0.6 { 1.0 } else { (1.0 - (k - 0.6) / 0.4) as f32 };
            let (x, y) = ((nx * 0.5 + 0.5) * w, (-ny * 0.5 + 0.5) * h);
            let p = pen.sub(Xf::tr(x, y).then(Xf::scale(sc, sc)), op);
            let t = p.t(&q.n.to_string(), Fam::Num, 22.0, 0.0);
            let hot = q.n >= 30;
            let (col, gl) = if hot { (hx(0xffd0ec), TS(0.0, 0.0, 12.0, rgba(255, 60, 140, 0.95))) } else { (hx(0xfff3c0), TS(0.0, 0.0, 10.0, rgba(255, 150, 40, 0.9))) };
            p.text(&t, -t.w * 0.5, 22.0 * BASE_K, col, &[TS(0.0, 2.0, 0.0, rgba(90, 20, 40, 0.95)), gl]);
        }
        let sh = [TS(0.0, 2.0, 0.0, rgba(90, 20, 40, 0.95)), TS(0.0, 0.0, 14.0, rgba(255, 150, 40, 0.8))];
        for r in &self.rewards {
            if r.t > 1.6 { continue; }
            let k = r.t / 1.6;
            let Some((nx, ny)) = project(pv, r.x, 1.6 + k * 1.2, r.z) else { continue };
            let sc = if r.t < 0.12 { (1.5 - r.t / 0.12 * 0.5) as f32 } else { 1.0 };
            let op = if k < 0.7 { 1.0 } else { (1.0 - (k - 0.7) / 0.3) as f32 };
            let (x, y) = ((nx * 0.5 + 0.5) * w, (-ny * 0.5 + 0.5) * h);
            let p = pen.sub(Xf::tr(x, y).then(Xf::scale(sc, sc)), op);
            let span = p.t(&format!("{} 体 撃破", r.kills), Fam::Mincho800, 16.0, 2.4);
            let span_h = 16.0 * LH;
            if r.bonus > 0 {
                let tb = p.t(&format!("+{}", fmt_num(r.bonus)), Fam::Num, 44.0, 0.0);
                let tp = p.t("点", Fam::Brush, 22.0, 0.0);
                let bw = tb.w + 4.0 + tp.w;
                let total = 44.0 + 2.0 + span_h;
                let top = -total * 0.5;
                let base = top + 22.0 + 44.0 * BASE_K;
                p.text(&tb, -bw * 0.5, base, hx(0xffd060), &sh);
                p.text(&tp, -bw * 0.5 + tb.w + 4.0, base, CREAM, &sh);
                p.text(&span, -span.w * 0.5, top + 44.0 + 2.0 + span_h * 0.5 + 16.0 * BASE_K, CREAM, &sh);
            } else {
                p.text(&span, -span.w * 0.5, 16.0 * BASE_K, CREAM, &sh);
            }
        }
    }

    /// 担いでいる間の案内（画面下）
    fn draw_hint(&mut self, pen: &Pen, m: &M) {
        let now = self.now;
        self.hint_op.set(if self.hint_on { 1.0 } else { 0.0 }, now);
        let op = self.hint_op.get(now);
        if op <= 0.0 || self.hint.is_empty() { return; }
        let t = pen.t(&self.hint, Fam::Mincho800, 16.0, 3.2);
        let (bw, bh) = (t.w + 52.0, 16.0 * LH + 12.0);
        let r = Rect::from_min_size(pos2(m.w * 0.5 - bw * 0.5, m.h - 96.0 - m.safe.bottom - m.lift() - bh), vec2(bw, bh));
        // bump は transform を丸ごと置き換えるので、translateX(-50%) へ向かって動く（TS と同じ）
        let e = snap(self.hint_bump.k(now, 0.3));
        let sc = lerp(1.25, 1.0, e);
        let shift = lerp(bw * 0.5, 0.0, e);
        let q = pen.sub(Xf::about(r.center().x + shift, r.center().y, Xf::scale(sc, sc)).then(Xf::tr(shift, 0.0)), op);
        let ink = rgba(10, 6, 20, 0.8);
        q.hgrad(r, &[(0.0, CLEAR), (0.2, ink), (0.8, ink), (1.0, CLEAR)]);
        q.text(&t, r.left() + 26.0, r.center().y + 16.0 * BASE_K, GOLD, &[]);
    }

    fn draw_banner(&self, pen: &Pen, m: &M) {
        let Some(bn) = &self.banner else { return };
        let t = ((self.now - bn.at) as f32 / 2.6).clamp(0.0, 1.0);
        if t >= 1.0 { return; }
        // bnIn 2.6s var(--ease) both
        let op = kf(t, &[(0.0, 0.0), (0.1, 1.0), (0.8, 1.0), (1.0, 0.0)], ease);
        let sc = kf(t, &[(0.0, 1.6), (0.1, 0.97), (0.14, 1.0), (1.0, 1.02)], ease);
        let dy = kf(t, &[(0.14, 0.0), (1.0, -10.0)], ease);
        let blur = kf(t, &[(0.0, 12.0), (0.1, 0.0)], ease);
        let fs = if m.short { 56.0 } else { cl(64.0, m.w * 0.1, 130.0) };
        let col = match bn.tone { Tone::Plain => CREAM, Tone::Gold => GOLD, Tone::Red => hx(0xff6a7a) };
        let top = m.h * 0.3;
        let has_sub = !bn.s.is_empty();
        // 窓より広いと折り返す（箱は窓いっぱいになり、文字は左へ寄る）
        let k_rows = pen.lines(&bn.k, Fam::Brush, fs, fs * 0.08, m.w);
        let s_rows = if has_sub { pen.lines(&bn.s, Fam::Mincho800, 16.0, 6.4, m.w - 60.0) } else { vec![] };
        let k_h = k_rows.len() as f32 * fs;
        let s_h = s_rows.len() as f32 * 16.0 * LH;
        let sub_h = if has_sub { 6.0 + s_h + 8.0 } else { 0.0 };
        let cy = top + (k_h + sub_h) * 0.5;
        let q = pen.sub(Xf::about(m.w * 0.5, cy, Xf::tr(0.0, dy).then(Xf::scale(sc, sc))), op);
        // 後ろの暗がり
        let bw = 900.0_f32.min(m.w * 0.9);
        let back = Rect::from_min_size(pos2(m.w * 0.5 - bw * 0.5, top), vec2(bw, 140.0));
        q.radial(back, back.center(), bw * 0.5, 70.0, &[(0.0, rgba(10, 6, 20, 0.7)), (0.7, CLEAR)], 24, 8);
        for (i, tk) in k_rows.iter().enumerate() {
            let x = if k_rows.len() > 1 { 0.0 } else { m.w * 0.5 - tk.w * 0.5 };
            q.text_f(tk, x, top + fs * (i as f32 + 0.5) + fs * BASE_K, Fill::S(col), &[TS(0.0, 0.0, 40.0, rgba(255, 90, 60, 0.7)), TS(0.0, 6.0, 0.0, rgba(90, 20, 40, 0.8))], blur);
        }
        if has_sub {
            let bw = if s_rows.len() > 1 { m.w } else { s_rows[0].w + 60.0 };
            let r = Rect::from_min_size(pos2(m.w * 0.5 - bw * 0.5, top + k_h + 6.0), vec2(bw, s_h + 8.0));
            let ink = rgba(10, 6, 20, 0.8);
            q.hgrad(r, &[(0.0, CLEAR), (0.2, ink), (0.8, ink), (1.0, CLEAR)]);
            for (i, ts) in s_rows.iter().enumerate() {
                q.text_f(ts, r.left() + 30.0, r.top() + 4.0 + 16.0 * LH * (i as f32 + 0.5) + 16.0 * BASE_K, Fill::S(GOLD), &[], blur);
            }
        }
    }

    /// 大将の名前の札: 画面の左下に、小さな肩書き・筆の名前・伸びる金の線
    fn draw_boss_card(&self, pen: &Pen, m: &M) {
        let Some(at) = self.boss_card else { return };
        // 顔の寄りの 2.05 秒（着地 1.45 秒 → 登場の終わり 3.5 秒）で出して消す
        let t = (at / 2.05).clamp(0.0, 1.0);
        if at < 0.0 || t >= 1.0 { return; }
        let op = kf(t, &[(0.0, 0.0), (0.12, 1.0), (0.82, 1.0), (1.0, 0.0)], ease);
        let dx = kf(t, &[(0.0, -28.0), (0.16, 0.0), (1.0, 6.0)], ease);
        let blur = kf(t, &[(0.0, 10.0), (0.12, 0.0)], ease);
        let line = kf(t, &[(0.05, 0.0), (0.35, 1.0)], ease);
        let fs = if m.short { 60.0 } else { cl(72.0, m.w * 0.085, 124.0) };
        let x = m.w * 0.075;
        let y = m.h * 0.62;
        let q = pen.sub(Xf::tr(dx, 0.0), op);
        let back = Rect::from_min_size(pos2(0.0, y - 40.0), vec2((m.w * 0.62).min(900.0), fs + 96.0));
        q.hgrad(back, &[(0.0, rgba(10, 6, 20, 0.72)), (0.6, rgba(10, 6, 20, 0.5)), (1.0, CLEAR)]);
        let rank = q.t("大将", Fam::Mincho800, 20.0, 9.0);
        q.text_f(&rank, x + 4.0, y + 20.0 * BASE_K, Fill::S(GOLD), &[], blur);
        let name = q.t(STAGE.boss(), Fam::Brush, fs, fs * 0.08);
        q.text_f(&name, x, y + 26.0 + fs * 0.5 + fs * BASE_K, Fill::S(CREAM), &[TS(0.0, 0.0, 36.0, rgba(255, 150, 60, 0.75)), TS(0.0, 5.0, 0.0, rgba(60, 20, 10, 0.85))], blur);
        let lw = (name.w + 40.0) * line;
        q.hgrad(Rect::from_min_size(pos2(x, y + 34.0 + fs), vec2(lw, 2.0)), &[(0.0, GOLD), (0.75, GOLD), (1.0, CLEAR)]);
    }

    /// つなぎの道しるべ: 通常攻撃の段と、いま強攻撃を押すと出る技
    fn draw_route(&self, pen: &Pen, m: &M) {
        let now = self.now;
        let op = self.route_op.get(now);
        if op <= 0.0 { return; }
        let q = pen.with_alpha(op);
        let bottom = m.h - if m.short { 64.0 } else { 52.0 } - m.safe.bottom - m.lift();
        let next_h = 14.0 * LH;
        let top = bottom - next_h - 6.0 - 12.0;
        // 段の菱形
        let total = 12.0 * 6.0 + 8.0 * 5.0;
        for i in 0..6 {
            let c = pos2(m.w * 0.5 - total * 0.5 + 6.0 + i as f32 * 20.0, top + 6.0);
            let k = self.pips[i].get(now);
            let d = q.sub(Xf::about(c.x, c.y, Xf::rot(45.0)), 1.0);
            let r = Rect::from_center_size(c, vec2(12.0, 12.0));
            if k > 0.0 { d.soft(r, 0.0, 10.0, rgba(255, 201, 74, 0.9 * k), false); }
            d.rect(r, rgba(10, 6, 20, 0.5).lerp(GOLD, k));
            d.frame(r, 1.0, rgba(255, 243, 224, 0.4).lerp(GOLD, k));
        }
        // いま押すと出る技
        let sp = 14.0 * 0.15;
        let sh = vec![SHADOW8];
        // 強攻撃の札は、最後に触った機器の表記（指は画面のボタンの字）
        let mut items = vec![P::K(Kbd::gold(&q, controls::label(Act::Heavy, self.device), 14.0, sp))];
        items.push(P::T(q.t(self.route_next, Fam::Mincho800, 14.0, sp), CREAM, sh.clone()));
        if self.route_load > 0 {
            items.push(P::Gap(10.0));
            items.push(P::T(q.t(&format!("×{}", self.route_load), Fam::Num, 14.0 * 1.35, sp), hx(0xffc040), vec![TS(0.0, 2.0, 0.0, rgba(90, 20, 40, 0.9)), TS(0.0, 0.0, 10.0, rgba(255, 160, 40, 0.7))]));
        }
        let w = row_w(&items);
        row_draw(&q, &items, m.w * 0.5 - w * 0.5, bottom - next_h * 0.5 + 14.0 * BASE_K);
    }

    fn draw_help(&self, pen: &Pen, m: &M) {
        // 指のときは画面のボタンが説明なので出さない。キーボードとパッドは、割り当ての表から機器の表記で
        if !self.help_shown() { return; }
        let help = controls::help_groups(self.device);
        let op = self.help_op.get(self.now);
        if op <= 0.0 { return; }
        let q = pen.with_alpha(op);
        let (fs, col) = (11.5, rgba(255, 243, 224, 0.62));
        let kb = |s: &str| P::K(Kbd::new(&q, s, fs, 0.0, col, rgba(255, 243, 224, 0.45)));
        let tx = |s: &str| P::T(q.t(s, Fam::Jp500, fs, 0.0), col, vec![]);
        let groups: Vec<Vec<P>> = help.iter().map(|(keys, text)| keys.iter().map(|k| kb(k)).chain(std::iter::once(tx(text))).collect()).collect();
        let w: f32 = groups.iter().map(|g| row_w(g)).sum::<f32>() + 18.0 * (groups.len() - 1) as f32;
        let mut x = m.w * 0.5 - w * 0.5;
        let base = m.h - 14.0 - m.safe.bottom - fs * LH * 0.5 + fs * BASE_K;
        for g in &groups {
            row_draw(&q, g, x, base);
            x += row_w(g) + 18.0;
        }
    }

    /// 札（隊長・大将の名前）
    fn draw_tags(&self, pen: &Pen, b: &Battle, pv: &DMat4, m: &M) {
        let place = |text: &str, x: f64, y: f64, z: f64, boss: bool| {
            let Some((nx, ny)) = project(pv, x, y, z) else { return };
            if nx.abs() > 1.1 || ny.abs() > 1.1 { return; }
            let (sx, sy) = ((nx * 0.5 + 0.5) * m.w, (-ny * 0.5 + 0.5) * m.h);
            let fs = if boss { 16.0 } else { 13.0 };
            let t = pen.t(text, Fam::Mincho800, fs, 0.0);
            let (w, h) = (t.w + 23.0, fs * LH + 4.0);
            let r = Rect::from_min_size(pos2(sx - w * 0.5, sy - h), vec2(w, h));
            pen.rect(r, rgba(10, 6, 20, 0.7));
            pen.rect(Rect::from_min_size(r.min, vec2(3.0, h)), if boss { GOLD } else { RED });
            pen.text(&t, r.left() + 13.0, r.center().y + fs * BASE_K, CREAM, &[]);
        };
        for e in &b.enemies {
            if e.alive && e.role == Role::Captain && e.state != EnemyState::Dead {
                let d = e.kind.def();
                place(STAGE.kind_name(e.kind), e.x, e.y + d.height + 0.7, e.z, false);
            }
        }
        // 登場の間は名前の札（draw_boss_card）が出るので、頭の上の札は出さない
        if boss_on(b) && self.boss_card.is_none() { place(&if b.giant.on { format!("{} {}", STAGE.giant(), STAGE.boss()) } else { format!("大将 {}", STAGE.boss()) }, b.boss.x, 8.4 + b.boss_y, b.boss.z, true); }
    }
}

/// 画面の大きさと、@media の分かれ目（max-width: 760px / max-height: 520px）
struct M { w: f32, h: f32, narrow: bool, short: bool, safe: Safe, touch: bool }
impl M {
    /// 縦持ち（幅 760 以下で、低くない）
    fn portrait(&self) -> bool { self.narrow && !self.short }
    /// 撃破数・連撃数の段の上端。縦持ちは 目標 → 敵将の体力 の下。Ⅱ が小地図の下にあるときはその下
    fn right_top(&self) -> f32 {
        let base: f32 = if self.short { 96.0 } else if self.narrow { 230.0 } else { 220.0 };
        let under = if self.touch && pause_under_map(self.w, self.h) { 14.0 + map_size(self.w, self.h) + 8.0 + HIT_MIN + 10.0 } else { 0.0 };
        base.max(under) + self.safe.top
    }
    /// 縦持ちで指のとき、下の案内（担いだ・道しるべ）を指のボタンの上へ上げる量
    fn lift(&self) -> f32 { if self.portrait() && self.touch { 200.0 } else { 0.0 } }
}

fn boss_on(b: &Battle) -> bool { b.boss.state != BossState::Dormant && b.boss.state != BossState::Dead }

/// three の project（-1..1、y は上向き）。カメラの後ろは None（TS の v.z > 1）
fn project(pv: &DMat4, x: f64, y: f64, z: f64) -> Option<(f32, f32)> {
    let c = *pv * DVec4::new(x, y, z, 1.0);
    if c.w <= 0.0 { return None; }
    Some(((c.x / c.w) as f32, (c.y / c.w) as f32))
}

fn c32(c: C) -> egui::Color32 { egui::Color32::from_rgba_unmultiplied(c.r as u8, c.g as u8, c.b as u8, (c.a.clamp(0.0, 1.0) * 255.0) as u8) }

/// @keyframes full の明るさ（infinite alternate）
fn full_glow(now: f64, dur: f32) -> f32 {
    let t = (now as f32 / dur).rem_euclid(2.0);
    css_ease(if t < 1.0 { t } else { 2.0 - t })
}

/// .bar の傾き（skewX(-24deg)）を掛けたペン
fn bar_pen(pen: &Pen, r: Rect) -> Pen { pen.sub(Xf::about(r.center().x, r.center().y, Xf::skew_x(-24.0)), 1.0) }
fn bar_base(p: &Pen, r: Rect, border: C) {
    p.rect(r, rgba(10, 6, 20, 0.7));
    p.frame(r, 1.0, border);
}
/// 左から k の幅（transform: scaleX(k)、transform-origin: left）
fn fill_rect(p: &Pen, inner: Rect, k: f32, f: impl FnOnce(&Pen, Rect)) {
    if k <= 0.0 { return; }
    f(p, Rect::from_min_size(inner.min, vec2(inner.width() * k, inner.height())));
}

/// box-shadow: inset の丸（外から内へ薄れる）
fn inset_ring(pen: &Pen, c: Pos2, r: f32, blur: f32, col: C) {
    const RINGS: [(f32, f32); 4] = [(0.0, 0.5), (0.5, 0.159), (1.0, 0.023), (1.3, 0.0)];
    let n = 48u32;
    let mut m = Mesh::default();
    for (d, k) in RINGS {
        let rr = (r - d * blur).max(0.0);
        for i in 0..n {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            m.colored_vertex(pen.xf.ap(pos2(c.x + a.cos() * rr, c.y + a.sin() * rr)), c32(col.alpha(k * pen.alpha)));
        }
    }
    for j in 0..RINGS.len() as u32 - 1 {
        for i in 0..n {
            let (p, q) = (j * n + i, j * n + (i + 1) % n);
            m.add_triangle(p, q, p + n);
            m.add_triangle(q, q + n, p + n);
        }
    }
    pen.painter.add(egui::Shape::mesh(m));
}

#[cfg(test)]
mod tests {
    use super::*;
    // 連鎖の表示の条件（TS の chain と同じ）
    #[test]
    fn chain_rules() {
        let mut h = Hud::new();
        h.chain(5, 2);
        assert!(!h.chain_on, "3 体未満の新しい連鎖は出さない");
        h.chain(5, 3);
        assert!(h.chain_on);
        h.chain(5, 2);
        assert_eq!(h.chain_n, 2, "出ている連鎖なら更新する");
        h.chain_end(6, 9);
        assert!(h.chain_end_at.0.is_none(), "別の連鎖の終わりは無視する");
    }
}

// ---- 確かめ用: 画面に出している物の写し（今の版は DOM から同じ形を作る。rust/compare/scenes.py の UI_TS） ----
#[cfg(target_arch = "wasm32")]
impl Hud {
    pub fn dump(&self, b: Option<&Battle>, cam: Option<&Camera>) -> serde_json::Value {
        use serde_json::json;
        let tone = |t: Tone| match t { Tone::Plain => "", Tone::Gold => "gold", Tone::Red => "red" };
        let (w, h) = self.size;
        let (mut tags, mut pops, mut rewards) = (vec![], vec![], vec![]);
        if let Some(c) = cam {
            let pv = c.project_pv();
            let at = |nx: f32, ny: f32| ((nx as f64 * 0.5 + 0.5) * w as f64, (-ny as f64 * 0.5 + 0.5) * h as f64);
            if let Some(b) = b {
                let mut place = |text: &str, x: f64, y: f64, z: f64| { if let Some((nx, ny)) = project(&pv, x, y, z) && nx.abs() <= 1.1 && ny.abs() <= 1.1 { let (sx, sy) = at(nx, ny); tags.push(json!([text, sx, sy])); } };
                for e in &b.enemies { if e.alive && e.role == Role::Captain && e.state != EnemyState::Dead { let d = e.kind.def(); place(STAGE.kind_name(e.kind), e.x, e.y + d.height + 0.7, e.z); } }
                if boss_on(b) && self.boss_card.is_none() { place(&if b.giant.on { format!("{} {}", STAGE.giant(), STAGE.boss()) } else { format!("大将 {}", STAGE.boss()) }, b.boss.x, 8.4 + b.boss_y, b.boss.z); }
            }
            for q in &self.pops { if q.t <= 0.8 { let k = q.t / 0.8; if let Some((nx, ny)) = project(&pv, q.x, q.y + k * 0.9, q.z) { let (x, y) = at(nx, ny); pops.push(json!({ "n": q.n, "x": x, "y": y, "op": if k < 0.6 { 1.0 } else { 1.0 - (k - 0.6) / 0.4 }, "sc": q.s as f64 * if q.t < 0.08 { 1.6 - q.t / 0.08 * 0.6 } else { 1.0 } })); } } }
            for r in &self.rewards { if r.t <= 1.6 { let k = r.t / 1.6; if let Some((nx, ny)) = project(&pv, r.x, 1.6 + k * 1.2, r.z) { let (x, y) = at(nx, ny); rewards.push(json!({ "text": format!("{}{} 体 撃破", if r.bonus > 0 { format!("+{}点", fmt_num(r.bonus)) } else { String::new() }, r.kills), "x": x, "y": y, "op": if k < 0.7 { 1.0 } else { 1.0 - (k - 0.7) / 0.3 }, "sc": if r.t < 0.12 { 1.5 - r.t / 0.12 * 0.5 } else { 1.0 } })); } } }
        }
        json!({
            "visible": self.visible, "name": self.name, "rank": if self.rank > 0 { format!("格 {}", ["", "壱", "弐", "参"][self.rank.clamp(0, 3) as usize]) } else { String::new() },
            "hp": self.hp.target(), "mu": self.mu.target(), "muFull": self.mu.target() >= 1.0, "low": self.hp.target() < 0.3,
            "ko": self.ko, "comboOn": self.combo_op.target() > 0.5, "comboN": self.combo_n, "comboHot": self.combo_hot,
            "chainOn": self.chain_on, "chainN": self.chain_n, "chainHot": self.chain_hot, "chainK": self.chain_k,
            "hint": if self.hint_on { self.hint.clone() } else { String::new() },
            "banner": self.banner.as_ref().map(|b| json!({ "k": b.k, "s": b.s, "tone": tone(b.tone) })),
            "routeOn": self.route_op.target() > 0.5, "routePips": self.pips.iter().filter(|p| p.target() > 0.5).count(), "routeNext": self.route_next, "routeLoad": self.route_load,
            "helpFade": self.help_op.target() < 0.5, "helpShown": self.visible && self.help_shown() && self.help_op.target() > 0.5, "foeOn": self.foe_op.target() > 0.5, "foeName": self.foe_name, "foeFill": self.foe_fill.target(),
            "tags": tags, "pops": pops, "rewards": rewards,
        })
    }
}
