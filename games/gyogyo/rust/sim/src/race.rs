// レースの規則（描画から独立。1/120 秒の固定の刻み）。CPU にもプレイヤーと同じ速度・スタミナ・ジャンプ・接触の規則を当てる（企画 09）。src/sim/race.ts の写し。
use crate::ai::{Brain, Difficulty, Tactic, View};
use crate::items::{BASE_LEVELS, ITEM, Levels, MAX_LEVEL, Pickup, PickupKind, Shot, ShotKind, Trap, Weapon, drain_mul, guts_mul, hop_time, mystery_content, place_pickups, speed_mul, turbo_mul};
use crate::track::{CourseDef, Obstacle, ObstacleKind, Track, build_track};
use sim_math::{max, min, sign};

pub struct Tune {
    pub base: f64, pub dash_mul: f64, pub start_accel: f64, pub dash_rise: f64, pub lateral: f64, pub lateral_air: f64, pub r: f64, pub stamina_max: f64,
    pub drain: f64, pub regen: f64, pub regen_delay: f64, pub exhaust: f64, pub low_at: f64, pub jump_v: f64, pub gravity: f64, pub hurdle_clear: f64,
    pub stumble_s: f64, pub stumble_mul: f64, pub wet_mul: f64, pub block_gap: f64, pub draft_gap: f64, pub draft_width: f64, pub draft_regen: f64, pub draft_drain: f64,
    pub slope_drain: f64, pub slope_speed: f64, pub headwind_regen: f64, pub headwind_drain: f64, pub slip_charge: f64, pub slip_mul: f64, pub slip_time: f64,
}
pub const TUNE: Tune = Tune {
    base: 13.5,
    dash_mul: 1.2,
    // 通常の速さまでの加速（m/s²）。止まった所から約 1 秒
    start_accel: 14.0,
    // ダッシュの最高速まで 0.6 秒
    dash_rise: 0.6,
    lateral: 4.8,
    lateral_air: 0.45,
    r: 0.56,
    stamina_max: 100.0,
    drain: 22.0,
    regen: 10.0,
    regen_delay: 0.8,
    exhaust: 2.0,
    low_at: 22.0,
    jump_v: 7.3,
    gravity: 24.3,
    hurdle_clear: 0.5,
    stumble_s: 0.55,
    stumble_mul: 0.62,
    wet_mul: 0.8,
    block_gap: 1.3,
    draft_gap: 8.0,
    draft_width: 0.95,
    draft_regen: 1.7,
    draft_drain: 0.78,
    slope_drain: 7.0,
    slope_speed: 0.55,
    // 先頭は風を一人で受ける: 回復が遅く、ダッシュの消費が多い（順位で速さは変えない。逃げっぱなしを苦しくして、仕掛けどころを作る）
    headwind_regen: 0.7,
    headwind_drain: 1.14,
    slip_charge: 0.75,
    // 風よけから抜け出したときの伸び（企画の「外へ出て一気に抜く」を手触りにする。スタミナは使わない）
    slip_mul: 1.08,
    slip_time: 1.7,
};

const STEP: f64 = 1.0 / 120.0;
/// 合図のあと走り出すまで（企画 03。3 秒の合図）
pub const COUNTDOWN: f64 = 3.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub x: f64, pub dash: bool, pub jump: bool,
    /// 攻撃（押している間 true。離したとき振る。押し続けると息を流してためる）
    pub attack: bool,
}
impl Input { pub const NONE: Input = Input { x: 0.0, dash: false, jump: false, attack: false }; }

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub dash_early: f64, pub dash_late: f64, pub blocked: f64, pub outer: f64, pub hurdles: i32, pub cleared: i32, pub wet: f64, pub empties: i32, pub final_stamina: f64,
    pub passes: i32, pub draft: f64, pub lane_sum: f64, pub lane_n: i32, pub slings: i32, pub late_lane_sum: f64, pub late_lane_n: i32, pub led_for: f64,
    /// 取ったマーク（能力）の数・当てた回数・当てられた回数・攻撃とわなを跳んでよけた回数
    pub marks: i32, pub hits_dealt: i32, pub charged_hits: i32, pub hits_taken: i32, pub dodges: i32,
}

#[derive(Clone, Debug)]
pub struct Runner {
    pub i: usize,
    /// 枠番（1〜8）
    pub gate: usize,
    pub is_player: bool,
    /// スタートからの距離（内柵に沿って測る）
    pub dist: f64,
    pub s: f64,
    pub d: f64,
    pub vd: f64,
    pub y: f64,
    pub vy: f64,
    pub air: bool,
    pub speed: f64,
    pub stamina: f64,
    pub dashing: bool,
    /// ダッシュの効き 0..1（最高速まで 0.6 秒で上がる）
    pub dash_k: f64,
    pub since_dash: f64,
    pub exhausted: f64,
    pub stumble: f64,
    pub wet: f64,
    pub blocked: bool,
    pub blocked_for: f64,
    pub draft: bool,
    pub draft_for: f64,
    /// 風よけの溜まり 0..1
    pub slip: f64,
    /// 抜け出しの伸びの残り秒
    pub slip_burst: f64,
    /// 秒ごとの順位の記録
    pub rank_log: Vec<i32>,
    /// 各コーナー出口の順位（通過順）
    pub corners: Vec<i32>,
    /// 最後の直線に入ったときの順位
    pub rank_at_final: i32,
    pub finished: bool,
    pub finish_t: f64,
    /// ゴール順（1 始まり。ゴールするまで 0）
    pub place: i32,
    pub rank: i32,
    pub jump_buf: f64,
    pub next_ob: usize,
    pub tactic: Tactic,
    pub stats: Stats,
    /// 能力の段（始めの値 + このレースで取ったマーク）
    pub lv: Levels,
    /// 次に見る落ちている物（pickups の番号）
    pub next_pick: usize,
    /// 持っている武器・残りの秒・残りの回数
    pub weapon: Option<Weapon>,
    pub weapon_t: f64,
    pub weapon_uses: i32,
    /// 攻撃を押し続けている秒と、息のたまり 0..1
    pub atk_hold: f64,
    pub charge: f64,
    /// このコマに振る（Some(いぶきの一撃か)）。combat が読んで消す
    pub swing: Option<bool>,
    pub swing_cd: f64,
    /// 人が操作する走者か（みんなで走るときは複数。CPU の判断を使わない）と、ジャンプの受付を広げるか
    pub human: bool,
    pub wide: bool,
    /// 当たったあと、次の攻撃を受けない残りの秒
    pub immune: f64,
    /// 跳び越えたあとの伸びの残りの秒
    pub hop: f64,
    /// ハンマーで真上へ飛ばされている残りの秒（その間は進まず、操作も効かない）と、その長さ・高さ
    pub launch: f64,
    pub launch_len: f64,
    pub launch_h: f64,
    /// 噴射の残りの秒
    pub jet: f64,
    /// 攻撃が当たってぼうぜんとしている残りの秒（よろけに重ねて大きく遅くなる。ITEM.hit_slow）
    pub dazed: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RaceEvent {
    Go,
    Count { n: i32 },
    Jump { i: usize },
    Land { i: usize, clean: bool },
    Clear { i: usize, perfect: bool, kind: ObstacleKind },
    Stumble { i: usize, kind: ObstacleKind },
    Splash { i: usize },
    Bump { i: usize, j: usize, side: bool },
    Rail { i: usize },
    DashStart { i: usize },
    DashEnd { i: usize },
    Low { i: usize },
    Empty { i: usize },
    Recovered { i: usize },
    SlipReady { i: usize },
    Sling { i: usize },
    Pass { i: usize, j: usize },
    Lap { lap: i32 },
    FinalStraight,
    Lead { i: usize },
    Finish { i: usize, place: i32, time: f64 },
    /// 落ちている物を取った
    Pick { i: usize, kind: PickupKind },
    ChargeStart { i: usize },
    ChargeFull { i: usize },
    Swing { i: usize, weapon: Weapon, charged: bool },
    /// by の攻撃が i に当たった
    Hit { by: usize, i: usize, weapon: Weapon, charged: bool },
    /// 飛んでくる攻撃・床のわなを跳んでよけた
    Dodge { i: usize },
    WeaponGone { i: usize },
    /// by のハンマーで i が真上へ飛ばされた
    Launch { i: usize, by: usize },
    /// ボーガンの矢が 1 周して、撃った本人の前で消えた（dist は消えた所）
    BoltDone { owner: usize, dist: f64 },
}

/// mulberry（`seed >>> 0`）
#[derive(Clone, Copy, Debug)]
pub struct Rng(pub u32);
impl Rng {
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        (t ^ (t >> 14)) as f64 / 4294967296.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ObEntry { pub ob: Obstacle, pub dist: f64 }

/// 人が操作する走者（みんなで走るとき。枠と、その人の操作補助）
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Human { pub gate: usize, pub auto_jump: bool, pub wide_jump: bool }

pub struct RaceOptions<'a> {
    pub course: &'a CourseDef,
    pub seed: u32,
    pub difficulty: Difficulty,
    /// プレイヤーの枠（0 始まり）。None なら全員 CPU（タイトルの見本）
    pub player_gate: Option<usize>,
    /// 操作補助: 障害の手前で自動ジャンプ
    pub auto_jump: bool,
    /// 操作補助: ジャンプの受付を少し広げる
    pub wide_jump: bool,
    /// 走者ごとの能力の段（枠の順）。None なら全員 BASE_LEVELS（単発・カップ）
    pub levels: Option<[Levels; 8]>,
    /// みんなで走るときの、人が操作する走者（枠の順）。空なら 1 人で遊ぶ（player_gate と上の操作補助）。
    /// 空でないとき、player_gate は「この端末の人」の印だけ（出来事の出し分け。計算には効かない）で、上の操作補助は使わない
    pub humans: Vec<Human>,
}

pub struct Race {
    pub track: Track,
    pub runners: Vec<Runner>,
    pub obstacle_list: Vec<ObEntry>,
    pub difficulty: Difficulty,
    pub t: f64,
    pub started: bool,
    pub done: bool,
    pub finished_count: i32,
    pub leader: usize,
    pub events: Vec<RaceEvent>,
    pub acc: f64,
    pub final_announced: bool,
    pub lap_shown: i32,
    /// 順位の記録（結果の見返しなどに使う）
    pub order: Vec<usize>,
    pub rand: Rng,
    pub player_gate: Option<usize>,
    /// 落ちている物（距離の順）・飛んでいる攻撃・床のわな
    pub pickups: Vec<Pickup>,
    pub shots: Vec<Shot>,
    pub traps: Vec<Trap>,
    next_id: u32,
    brains: Vec<Option<Brain>>,
    /// 自動ジャンプの人の、跳ぶ判断（走者ごと。使わない走者は None）
    autos: Vec<Option<Brain>>,
    corner_dists: Vec<f64>,
    last_count: i32,
    latched_jump: bool,
    latched_attack: bool,
}

const FORMS: [f64; 8] = [0.62, 0.8, 0.95, 1.1, 1.3, 1.6, 2.0, 2.6];

impl Race {
    pub fn new(o: RaceOptions) -> Race {
        let track = build_track(o.course);
        let mut rand = Rng(o.seed);
        let w = track.width;
        let mut obstacle_list: Vec<ObEntry> = Vec::new();
        for lap in -1..=(o.course.laps + 1) {
            for ob in &track.obstacles {
                let dist = track.next_dist(0.0, ob.s) + lap as f64 * track.lap;
                if dist > 0.0 && dist < track.race_len { obstacle_list.push(ObEntry { ob: *ob, dist }); }
            }
        }
        obstacle_list.sort_by(|a, b| a.dist.partial_cmp(&b.dist).unwrap());

        // コーナーの出口（スタートからの距離）
        let mut corner_dists: Vec<f64> = Vec::new();
        for lap in 0..=o.course.laps {
            for c in &track.curves {
                let dd = track.next_dist(0.0, c.s1) + lap as f64 * track.lap;
                if dd < track.race_len { corner_dists.push(dd); }
            }
        }
        corner_dists.sort_by(|a, b| a.partial_cmp(b).unwrap());

        use Tactic::*;
        let mut tactics = [Front, Closer, Inner, Gap, Front, Closer, Inner, Gap];
        for i in (1..tactics.len()).rev() {
            let j = (rand.next() * (i + 1) as f64).floor() as usize;
            tactics.swap(i, j);
        }

        let gate_w = (w - 1.3) / 7.0;
        // 人が操作する走者（1 人で遊ぶときは自分だけ）
        let humans: Vec<Human> = if o.humans.is_empty() { o.player_gate.map(|gate| Human { gate, auto_jump: o.auto_jump, wide_jump: o.wide_jump }).into_iter().collect() } else { let mut h = o.humans.clone(); h.sort_by_key(|x| x.gate); h };
        let human_at = |i: usize| humans.iter().find(|h| h.gate == i);
        let runners: Vec<Runner> = (0..8).map(|i| Runner {
            i, gate: i + 1, is_player: Some(i) == o.player_gate,
            dist: 0.0, s: track.start_s, d: 0.65 + gate_w * i as f64, vd: 0.0, y: 0.0, vy: 0.0, air: false,
            speed: 0.0, stamina: TUNE.stamina_max, dashing: false, dash_k: 0.0, since_dash: 99.0, exhausted: 0.0, stumble: 0.0, wet: 0.0,
            blocked: false, blocked_for: 0.0, draft: false, draft_for: 0.0, slip: 0.0, slip_burst: 0.0, rank_log: Vec::new(), corners: Vec::new(), rank_at_final: 0, finished: false, finish_t: 0.0, place: 0, rank: i as i32 + 1, jump_buf: 0.0, next_ob: 0,
            tactic: tactics[i], stats: Stats::default(),
            lv: o.levels.map_or(BASE_LEVELS, |l| l[i]), next_pick: 0, weapon: None, weapon_t: 0.0, weapon_uses: 0, atk_hold: 0.0, charge: 0.0, swing: None, swing_cd: 0.0, human: human_at(i).is_some(), wide: human_at(i).is_some_and(|h| h.wide_jump), immune: 0.0, hop: 0.0, launch: 0.0, launch_len: 0.0, launch_h: 0.0, jet: 0.0, dazed: 0.0,
        }).collect();

        // 走者ごとの上手さの段（毎レース入れ替わる）
        let mut forms = FORMS;
        for i in (1..forms.len()).rev() {
            let j = (rand.next() * (i + 1) as f64).floor() as usize;
            forms.swap(i, j);
        }
        let gentle = o.course.gentle;
        let mut brains: Vec<Option<Brain>> = Vec::with_capacity(8);
        for x in &runners {
            brains.push(if x.human { None } else { Some(Brain::new(x.i, &runners, &mut rand, o.difficulty, gentle, false, Some(forms[x.i]))) });
        }
        // 自動ジャンプの人には、跳ぶ判断だけの脳を付ける。1 人で遊ぶときは今までどおり 1 つ作る（使わなくても乱数を 1 回読む。前と同じ並びを保つ）
        let mut autos: Vec<Option<Brain>> = (0..8).map(|_| None).collect();
        if o.humans.is_empty() {
            let b = Brain::new(o.player_gate.unwrap_or(0), &runners, &mut rand, Difficulty::Hard, false, true, None);
            if let Some(h) = humans.first() && h.auto_jump { autos[h.gate] = Some(b); }
        } else {
            for h in humans.iter().filter(|h| h.auto_jump) { autos[h.gate] = Some(Brain::new(h.gate, &runners, &mut rand, Difficulty::Hard, false, true, None)); }
        }
        let order = runners.iter().map(|x| x.i).collect();
        let pickups = place_pickups(&track, &obstacle_list, &mut rand);
        Race {
            pickups, shots: Vec::new(), traps: Vec::new(), next_id: 0,
            track, runners, obstacle_list, difficulty: o.difficulty, t: -COUNTDOWN, started: false, done: false, finished_count: 0, leader: 0, events: Vec::new(), acc: 0.0,
            final_announced: false, lap_shown: 1, order, rand, player_gate: o.player_gate, brains, autos, corner_dists, last_count: 4, latched_jump: false, latched_attack: false,
        }
    }

    /// 確かめ用（TS の x.brain = () => NO_INPUT）: この走者の判断を止める
    pub fn clear_brain(&mut self, i: usize) { self.brains[i] = None; }

    /// state.player
    pub fn player(&self) -> Option<&Runner> { self.player_gate.map(|g| &self.runners[g]) }

    /// 1 人で遊ぶときの 1 刻み（自分の枠にだけ入力を入れる）
    fn tick(&mut self, input: Input) {
        let mut inputs = [Input::NONE; 8];
        if let Some(g) = self.player_gate { inputs[g] = input; }
        self.tick_inputs(&inputs);
    }

    /// 人が全員ゴールしたか（みんなで走るとき、ここで残りの CPU の着順を確定する）
    pub fn humans_finished(&self) -> bool {
        if !self.runners.iter().any(|x| x.human) { return self.done; }
        self.runners.iter().all(|x| !x.human || x.finished)
    }

    /// 計算の照合値（端末どうしで食い違いを見つける。同じ入力の列なら、どの端末でも同じ値）
    pub fn checksum(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut add = |v: u64| { h ^= v; h = h.wrapping_mul(0x0000_0100_0000_01b3); };
        add(self.t.to_bits());
        add(self.rand.0 as u64);
        add(self.finished_count as u64);
        for x in &self.runners {
            for v in [x.dist, x.d, x.vd, x.y, x.speed, x.stamina, x.stumble, x.weapon_t, x.charge, x.finish_t, x.launch, x.jet] { add(v.to_bits()); }
            add(x.lv.iter().fold(0u64, |a, l| a * 16 + *l as u64));
            add(x.weapon.map_or(0, |w| w.i() as u64 + 1) * 8 + x.weapon_uses as u64);
        }
        add(self.pickups.iter().filter(|p| p.taken).count() as u64);
        add((self.shots.len() * 16 + self.traps.len()) as u64);
        h
    }

    /// 1 刻み進める。人の走者は inputs のその枠の入力で、ほかは CPU の判断で動く（みんなで走るときは、全員の端末が同じ inputs で呼ぶ）。
    /// jump は「押した」の合図（この刻みで受ける）、dash と attack は押している間 true
    pub fn tick_inputs(&mut self, inputs: &[Input; 8]) {
        if self.t < 0.0 {
            let n = (-self.t).ceil() as i32;
            if n != self.last_count { self.last_count = n; self.events.push(RaceEvent::Count { n }); }
            self.t += STEP;
            if self.t >= 0.0 { self.t = 0.0; self.started = true; self.events.push(RaceEvent::Go); }
            return;
        }
        self.t += STEP;
        // 判断
        for i in 0..self.runners.len() {
            if self.runners[i].finished { continue; }
            let inp = if self.runners[i].human {
                let input = inputs[i];
                if let Some(b) = self.autos[i].as_mut() {
                    let mut v = View { track: &self.track, runners: &self.runners, obstacle_list: &self.obstacle_list, pickups: &self.pickups, shots: &self.shots, traps: &self.traps, rand: &mut self.rand };
                    let a = b.decide(STEP, &mut v);
                    Input { jump: input.jump || a.jump, ..input }
                } else { input }
            } else if let Some(b) = self.brains[i].as_mut() {
                let mut v = View { track: &self.track, runners: &self.runners, obstacle_list: &self.obstacle_list, pickups: &self.pickups, shots: &self.shots, traps: &self.traps, rand: &mut self.rand };
                b.decide(STEP, &mut v)
            } else { Input::NONE };
            // 脚質の走りの配分は、判断する CPU だけ（人と、判断を止めた走者には効かない）
            let paced = !self.runners[i].human && self.brains[i].is_some();
            self.advance(i, inp, paced);
        }
        for i in 0..self.runners.len() { if self.runners[i].finished { self.coast(i); } }
        self.combat();
        self.collide();
        self.order();
        if !self.done && self.finished_count == self.runners.len() as i32 { self.done = true; }
    }

    fn advance(&mut self, i: usize, inp: Input, paced: bool) {
        let dt = STEP;
        let t = &TUNE;
        let (r, w) = (t.r, self.track.width);
        let state_t = self.t;
        let track = &self.track;
        let ev = &mut self.events;
        let cpu = crate::ai::cpu_pace(self.difficulty);
        let human_drain = crate::ai::human_drain(self.difficulty);
        let x = &mut self.runners[i];
        // ハンマーで飛ばされている: 真上へ上がって落ちる（進まない・操作も効かない）。落ちたら少しよろけて走り出す
        if x.launch > 0.0 {
            x.launch -= dt;
            x.speed = 0.0;
            x.dashing = false;
            x.dash_k = 0.0;
            x.vd = 0.0;
            x.jet = 0.0;
            if x.launch <= 0.0 {
                x.launch = 0.0;
                x.y = 0.0;
                x.vy = 0.0;
                x.air = false;
                x.stumble = 0.4;
                ev.push(RaceEvent::Land { i: x.i, clean: false });
            } else {
                let k = 1.0 - x.launch / x.launch_len;
                x.y = 4.0 * x.launch_h * k * (1.0 - k);
                x.air = true;
            }
            if x.immune > 0.0 { x.immune -= dt; }
            if x.weapon.is_some() { x.weapon_t -= dt; if x.weapon_t <= 0.0 { x.weapon = None; ev.push(RaceEvent::WeaponGone { i: x.i }); } }
            return;
        }
        // ジャンプ（着地後に次が可能。長押しで連発しない）。短い先行入力だけ受ける
        if inp.jump { x.jump_buf = if x.wide { 0.22 } else { 0.1 }; }
        if x.jump_buf > 0.0 { x.jump_buf -= dt; }
        if !x.air && x.jump_buf > 0.0 && x.stumble <= 0.0 { x.air = true; x.vy = t.jump_v; x.jump_buf = 0.0; ev.push(RaceEvent::Jump { i: x.i }); }
        // ダッシュとスタミナ（企画 04）
        let want = inp.dash && x.exhausted <= 0.0 && x.stamina > 0.0;
        if want && !x.dashing { ev.push(RaceEvent::DashStart { i: x.i }); }
        if !want && x.dashing { ev.push(RaceEvent::DashEnd { i: x.i }); }
        x.dashing = want;
        let sl = track.slope(x.s);
        let lv = x.lv;
        // 前に誰もいない（先頭）と風を受ける
        let headwind = x.rank == 1 && state_t > 4.0;
        if x.dashing {
            let drain = t.drain * drain_mul(&lv) * (if x.human { human_drain } else { 1.0 }) * (if x.draft { t.draft_drain } else { 1.0 }) * (if headwind { t.headwind_drain } else { 1.0 }) * max(0.75, 1.0 + sl * t.slope_drain);
            let before = x.stamina;
            x.stamina = max(0.0, x.stamina - drain * dt);
            x.since_dash = 0.0;
            if before > t.low_at && x.stamina <= t.low_at { ev.push(RaceEvent::Low { i: x.i }); }
            if x.stamina <= 0.0 { x.exhausted = t.exhaust; x.dashing = false; x.stats.empties += 1; ev.push(RaceEvent::Empty { i: x.i }); ev.push(RaceEvent::DashEnd { i: x.i }); }
            if x.dist < track.race_len * 0.5 { x.stats.dash_early += dt; } else { x.stats.dash_late += dt; }
        } else {
            x.since_dash += dt;
            // 押し続けている間は回復しない
            if !inp.dash && x.since_dash >= t.regen_delay { x.stamina = min(t.stamina_max, x.stamina + t.regen * (if x.draft { t.draft_regen } else if headwind { t.headwind_regen } else { 1.0 }) * dt); }
        }
        if x.exhausted > 0.0 { x.exhausted -= dt; if x.exhausted <= 0.0 { x.exhausted = 0.0; ev.push(RaceEvent::Recovered { i: x.i }); } }
        // 武器: 持てるのは少しの間。押して離すと振る。押し続けると息を流してためる（ためる間は回復しない）
        if x.weapon.is_some() {
            x.weapon_t -= dt;
            if x.weapon_t <= 0.0 { x.weapon = None; x.atk_hold = 0.0; x.charge = 0.0; ev.push(RaceEvent::WeaponGone { i: x.i }); }
        }
        if x.swing_cd > 0.0 { x.swing_cd -= dt; }
        if x.immune > 0.0 { x.immune -= dt; }
        if x.weapon.is_none() || x.stumble > 0.0 { x.atk_hold = 0.0; x.charge = 0.0; }
        else if inp.attack && x.atk_hold < ITEM.charge_auto {
            x.atk_hold += dt;
            if x.atk_hold > ITEM.charge_start && x.charge < 1.0 && x.exhausted <= 0.0 && x.stamina > 1.0 {
                if x.charge == 0.0 { ev.push(RaceEvent::ChargeStart { i: x.i }); }
                x.charge = min(1.0, x.charge + dt / (ITEM.charge_full - ITEM.charge_start));
                x.stamina = max(0.0, x.stamina - ITEM.charge_drain * dt);
                x.since_dash = 0.0;
                if x.charge >= 1.0 { ev.push(RaceEvent::ChargeFull { i: x.i }); }
            }
        } else if x.atk_hold > 0.0 && x.swing_cd <= 0.0 {
            // 離した（か、押し続けて勝手に出た）。振った直後に押した分は、振れるようになるまで持ち越す（連打の 2 回目を捨てない）
            x.swing = Some(x.charge >= 1.0);
            x.atk_hold = 0.0;
            x.charge = 0.0;
        }
        x.dash_k += ((if x.dashing { 1.0 } else { 0.0 }) - x.dash_k) * min(1.0, dt / (if x.dashing { t.dash_rise * 0.42 } else { 0.3 }));
        // 速さ
        let mut target = t.base * speed_mul(&lv) * (1.0 + (t.dash_mul - 1.0) * turbo_mul(&lv) * x.dash_k);
        if x.hop > 0.0 { x.hop -= dt; target *= ITEM.hop_mul; }
        // 噴射: 少しの間、大きく速い
        if x.jet > 0.0 { x.jet -= dt; target *= ITEM.jet_mul; }
        // 脚質（CPU の走りの配分）: 逃げは前半に飛ばして後半に落ち、追込は前半を抑えて後半に伸びる
        if paced { target *= x.tactic.pace(x.dist / track.race_len) * cpu; }
        target *= 1.0 - max(-0.05, min(0.09, sl * t.slope_speed));
        // 風よけ: 後ろに付くと溜まる。満ちてから抜け出すと、少しのあいだ伸びる
        if x.draft && !x.blocked {
            let before = x.slip;
            x.slip = min(1.0, x.slip + dt / t.slip_charge);
            if before < 1.0 && x.slip >= 1.0 { ev.push(RaceEvent::SlipReady { i: x.i }); }
        } else if !x.draft && x.slip > 0.0 {
            if x.slip >= 1.0 { x.slip_burst = t.slip_time; x.stats.slings += 1; ev.push(RaceEvent::Sling { i: x.i }); }
            x.slip = 0.0;
        }
        if x.slip_burst > 0.0 { x.slip_burst -= dt; target *= t.slip_mul; }
        if x.stumble > 0.0 { target *= t.stumble_mul; x.stumble -= dt; }
        if x.dazed > 0.0 { target *= ITEM.hit_slow; x.dazed -= dt; }
        if x.wet > 0.0 && !x.air { target *= t.wet_mul; }
        if x.speed < target { x.speed = min(target, x.speed + (if x.speed < t.base * 0.95 { t.start_accel } else if x.jet > 0.0 { 22.0 } else { 9.0 }) * dt); }
        else { x.speed = max(target, x.speed - 11.0 * dt); }
        // 進路: 瞬時に全幅を横切れない（企画 05）。空中は緩やか（企画 03）
        let ax = max(-1.0, min(1.0, inp.x));
        let lat = t.lateral * (if x.air { t.lateral_air } else { 1.0 });
        x.vd += (ax * lat - x.vd) * min(1.0, dt * 12.0);
        x.d += x.vd * dt;
        if x.d < r { if x.vd < -2.5 && x.is_player { ev.push(RaceEvent::Rail { i: x.i }); } x.d = r; x.vd = 0.0; }
        if x.d > w - r { if x.vd > 2.5 && x.is_player { ev.push(RaceEvent::Rail { i: x.i }); } x.d = w - r; x.vd = 0.0; }
        // 空中
        if x.air {
            x.vy -= t.gravity * dt;
            x.y += x.vy * dt;
            if x.y <= 0.0 { x.y = 0.0; x.vy = 0.0; x.air = false; ev.push(RaceEvent::Land { i: x.i, clean: x.stumble <= 0.0 }); }
        }
        // 進む。外側ほど同じ走りで s が進まない
        let per_m = track.progress_per_meter(x.s, x.d);
        let ds = x.speed * per_m * dt;
        let before = x.dist;
        x.dist += ds;
        x.s = track.wrap(x.s + ds);
        x.stats.outer += x.speed * dt * (1.0 - per_m);
        x.stats.lane_sum += x.d;
        x.stats.lane_n += 1;
        if x.dist > track.race_len - 300.0 { x.stats.late_lane_sum += x.d; x.stats.late_lane_n += 1; }
        if x.rank == 1 && x.dist > track.race_len * 0.1 { x.stats.led_for += dt; }
        if x.blocked { x.stats.blocked += dt; x.blocked_for += dt; } else { x.blocked_for = 0.0; }
        if x.draft { x.stats.draft += dt; x.draft_for += dt; } else { x.draft_for = 0.0; }
        // 障害
        let list = &self.obstacle_list;
        while x.next_ob < list.len() && list[x.next_ob].dist + list[x.next_ob].ob.len < before { x.next_ob += 1; }
        let mut q = x.next_ob;
        while q < list.len() && list[q].dist < x.dist + 1.0 {
            let (ob, dist) = (list[q].ob, list[q].dist);
            q += 1;
            if x.d < ob.d0 + 0.2 || x.d > ob.d1 - 0.2 {
                // 迂回路を通った
                if ob.kind == ObstacleKind::Water && x.wet > 0.0 && x.dist > dist + ob.len { x.wet = 0.0; }
                continue;
            }
            if ob.kind == ObstacleKind::Hurdle {
                if before < dist && x.dist >= dist {
                    if x.y < t.hurdle_clear {
                        x.stumble = t.stumble_s * guts_mul(&lv);
                        x.stats.hurdles += 1;
                        x.speed *= 0.82;
                        ev.push(RaceEvent::Stumble { i: x.i, kind: ObstacleKind::Hurdle });
                    } else {
                        x.stats.cleared += 1;
                        x.hop = hop_time(&lv);
                        ev.push(RaceEvent::Clear { i: x.i, kind: ObstacleKind::Hurdle, perfect: x.y > t.jump_v * t.jump_v / (2.0 * t.gravity) * 0.82 });
                    }
                }
            } else {
                let in_water = x.dist >= dist && x.dist <= dist + ob.len;
                if in_water && !x.air {
                    if x.wet <= 0.0 { ev.push(RaceEvent::Splash { i: x.i }); }
                    x.wet = 0.08;
                    x.stats.wet += dt;
                } else if before < dist + ob.len && x.dist >= dist + ob.len && x.wet <= 0.0 {
                    x.stats.cleared += 1;
                    x.hop = hop_time(&lv);
                    ev.push(RaceEvent::Clear { i: x.i, kind: ObstacleKind::Water, perfect: true });
                }
            }
        }
        if x.wet > 0.0 { x.wet -= dt; }
        // 落ちている物: 体が上を通ると取れる（先に通った 1 人だけ。跳んでいる間は取れない）
        let rand = &mut self.rand;
        let picks = &mut self.pickups;
        while x.next_pick < picks.len() && picks[x.next_pick].dist < before { x.next_pick += 1; }
        let mut q = x.next_pick;
        while q < picks.len() && picks[q].dist <= x.dist {
            let p = &mut picks[q];
            q += 1;
            if p.taken || p.dist < before || (x.d - p.d).abs() > ITEM.pick_r || x.y > ITEM.pick_y { continue; }
            p.taken = true;
            // 「？」の箱は、取った瞬間の順位で中身が決まる（後ろほど強い物）
            let kind = if p.kind == PickupKind::Mystery { mystery_content((x.rank - 1) as f64 / 7.0, rand.next()) } else { p.kind };
            match kind {
                PickupKind::Mark(st) => { x.lv[st.i()] = (x.lv[st.i()] + 1).min(MAX_LEVEL); x.stats.marks += 1; }
                PickupKind::Breath => x.stamina = min(t.stamina_max, x.stamina + ITEM.breath),
                PickupKind::Poison => x.stamina = max(0.0, x.stamina - ITEM.poison),
                PickupKind::Arm(w) => { x.weapon = Some(w); x.weapon_t = ITEM.weapon_time; x.weapon_uses = w.uses(); x.atk_hold = 0.0; x.charge = 0.0; }
                PickupKind::Jet => x.jet = ITEM.jet_t,
                PickupKind::Mystery => {}
            }
            ev.push(RaceEvent::Pick { i: x.i, kind });
        }
        // ゴール（体の中心が線を越えた順。企画 03）
        if x.dist >= track.race_len {
            let over = x.dist - track.race_len;
            x.finished = true;
            x.finish_t = state_t - (over / max(1.0, x.speed * per_m));
            x.stats.final_stamina = x.stamina;
            x.dashing = false;
            x.weapon = None;
            x.atk_hold = 0.0;
            x.charge = 0.0;
            x.swing = None;
            self.finished_count += 1;
            x.place = self.finished_count;
            ev.push(RaceEvent::Finish { i: x.i, place: x.place, time: x.finish_t });
        }
    }

    /// ゴールのあと: 息を整えながら流す
    fn coast(&mut self, i: usize) {
        let dt = STEP;
        let (r, w) = (TUNE.r, self.track.width);
        let track = &self.track;
        let x = &mut self.runners[i];
        x.speed = max(3.2, x.speed - 3.6 * dt);
        x.dash_k = max(0.0, x.dash_k - dt * 3.0);
        let ds = x.speed * track.progress_per_meter(x.s, x.d) * dt;
        x.dist += ds;
        x.s = track.wrap(x.s + ds);
        x.vd *= 1.0 - dt * 3.0;
        x.d = max(r, min(w - r, x.d + x.vd * dt));
        if x.air {
            x.vy -= TUNE.gravity * dt;
            x.y += x.vy * dt;
            if x.y <= 0.0 { x.y = 0.0; x.vy = 0.0; x.air = false; }
        }
    }

    /// 攻撃が当たった: よろけて息が減る（ガッツで減る）。倒れて止まらない。当たったあと少しの間は次を受けない
    fn hit(&mut self, by: usize, j: usize, weapon: Weapon, charged: bool) -> bool {
        let x = &mut self.runners[j];
        if x.finished || x.immune > 0.0 { return false; }
        let g = guts_mul(&x.lv);
        x.stumble = ITEM.hit_stumble * (if charged { ITEM.charge_mul } else { 1.0 }) * g;
        x.dazed = x.stumble;
        x.stamina = max(0.0, x.stamina - ITEM.hit_stamina * g);
        x.speed *= ITEM.hit_speed;
        x.immune = ITEM.immune;
        x.atk_hold = 0.0;
        x.charge = 0.0;
        // ハンマー: 真上へ高く飛ばされ、その場に落ちてくる（ためると長く高い）
        if weapon == Weapon::Hammer {
            x.launch_len = ITEM.launch_t * (if charged { 1.25 } else { 1.0 });
            x.launch = x.launch_len;
            x.launch_h = ITEM.launch_h * (if charged { 1.3 } else { 1.0 });
            x.immune = x.launch_len + 1.0;
            x.dashing = false;
            x.stumble = 0.0;
            self.events.push(RaceEvent::Launch { i: j, by });
        }
        x.stats.hits_taken += 1;
        self.runners[by].stats.hits_dealt += 1;
        if charged { self.runners[by].stats.charged_hits += 1; }
        self.events.push(RaceEvent::Hit { by, i: j, weapon, charged });
        true
    }

    /// 跳んでよけた: 少し伸びる（ジャンプの能力）
    fn dodge(&mut self, j: usize) {
        let x = &mut self.runners[j];
        x.hop = hop_time(&x.lv);
        x.stats.dodges += 1;
        self.events.push(RaceEvent::Dodge { i: j });
    }

    fn swing(&mut self, i: usize, charged: bool) {
        let Some(w) = self.runners[i].weapon else { return };
        let (dist, d, speed) = { let x = &self.runners[i]; (x.dist, x.d, x.speed) };
        let width = self.track.width;
        self.events.push(RaceEvent::Swing { i, weapon: w, charged });
        let k = if charged { 1.6 } else { 1.0 };
        let n = self.runners.len();
        match w {
            Weapon::Sword => {
                for j in 0..n {
                    if j == i { continue; }
                    let (gap, dd) = (self.runners[j].dist - dist, self.runners[j].d - d);
                    if gap > -0.3 && gap <= ITEM.sword_reach && dd.abs() <= ITEM.sword_side { self.hit(i, j, w, charged); }
                }
                if charged {
                    self.next_id += 1;
                    self.shots.push(Shot { id: self.next_id, kind: ShotKind::Wave, owner: i, dist: dist + 1.0, d, v: speed + ITEM.wave_v, left: ITEM.wave_range / ITEM.wave_v, hw: ITEM.wave_hw, charged, seen: 1 << i });
                }
            }
            Weapon::Chain => {
                for j in 0..n {
                    if j == i { continue; }
                    let (gap, dd) = (self.runners[j].dist - dist, self.runners[j].d - d);
                    if gap.abs() <= ITEM.chain_reach * k && dd.abs() <= ITEM.chain_side * k && self.hit(i, j, w, charged) {
                        // 外へはじく
                        self.runners[j].vd = (if dd == 0.0 { 1.0 } else { sign(dd) }) * ITEM.chain_push * k;
                    }
                }
            }
            Weapon::Rose => {
                let fan: &[f64] = if charged { &[-1.0, 0.0, 1.0] } else { &[0.0] };
                for f in fan {
                    self.next_id += 1;
                    let sd = max(0.4, min(width - 0.4, d + f * ITEM.rose_fan));
                    self.shots.push(Shot { id: self.next_id, kind: ShotKind::Rose, owner: i, dist: dist + 1.0, d: sd, v: speed + ITEM.rose_v, left: ITEM.rose_range / ITEM.rose_v, hw: ITEM.rose_hw, charged, seen: 1 << i });
                }
            }
            Weapon::Bottle => {
                self.next_id += 1;
                self.traps.push(Trap { id: self.next_id, owner: i, dist: dist - ITEM.trap_back, d, r: ITEM.trap_r * (if charged { 2.4 } else { 1.0 }), life: ITEM.trap_life, seen: 1 << i });
            }
            Weapon::Crossbow => {
                // 矢は走路の真ん中を 1 周する。進路は問わず触れた全員に当たる（hw に走路の幅）。消えるのは combat が決める
                self.next_id += 1;
                self.shots.push(Shot { id: self.next_id, kind: ShotKind::Bolt, owner: i, dist: dist + 1.0, d: width / 2.0, v: ITEM.bolt_v, left: 120.0, hw: width, charged: true, seen: 1 << i });
            }
            Weapon::Hammer => {
                for j in 0..n {
                    if j == i { continue; }
                    let (gap, dd) = (self.runners[j].dist - dist, self.runners[j].d - d);
                    if gap > -0.5 && gap <= ITEM.hammer_reach * k && dd.abs() <= ITEM.hammer_side * k { self.hit(i, j, w, charged); }
                }
            }
        }
        let x = &mut self.runners[i];
        x.swing_cd = ITEM.swing_cd;
        x.weapon_uses -= 1;
        if x.weapon_uses <= 0 { x.weapon = None; self.events.push(RaceEvent::WeaponGone { i }); }
    }

    /// 振る・飛ぶ・踏むを進める（走者が動いたあと、同じ刻みで）
    fn combat(&mut self) {
        let n = self.runners.len();
        for i in 0..n {
            if let Some(charged) = self.runners[i].swing.take() { self.swing(i, charged); }
        }
        // 飛んでいる攻撃: 同じ進路の走者に当たる。跳んでいればよける。衝撃波は通り抜け、バラは当たったら消える
        let mut shots = std::mem::take(&mut self.shots);
        let lap = self.track.lap;
        for s in shots.iter_mut() {
            s.dist += s.v * STEP;
            s.left -= STEP;
            let bolt = s.kind == ShotKind::Bolt;
            // ボーガンの矢: 撃った本人を後ろから 1 周追い抜いたら、すぐ前で消える
            if bolt && s.dist >= self.runners[s.owner].dist + lap + 3.0 {
                s.left = 0.0;
                self.events.push(RaceEvent::BoltDone { owner: s.owner, dist: s.dist });
                continue;
            }
            for j in 0..n {
                if s.left <= 0.0 { break; }
                if s.seen & (1 << j) != 0 || self.runners[j].finished { continue; }
                let x = &self.runners[j];
                // 矢は周回をまたいで当たる（走路の同じ所にいれば、何周差でも）
                let gap = if bolt { let g = (x.dist - s.dist).rem_euclid(lap); if g > lap / 2.0 { g - lap } else { g } } else { x.dist - s.dist };
                if gap.abs() > 0.7 || (x.d - s.d).abs() > s.hw { continue; }
                s.seen |= 1 << j;
                if x.y > ITEM.dodge_y { self.dodge(j); continue; }
                let w = match s.kind { ShotKind::Wave => Weapon::Sword, ShotKind::Rose => Weapon::Rose, ShotKind::Bolt => Weapon::Crossbow };
                if self.hit(s.owner, j, w, s.charged) && s.kind == ShotKind::Rose { s.left = 0.0; }
            }
        }
        shots.retain(|s| s.left > 0.0);
        self.shots = shots;
        // 床のわな: 踏むとすべる（1 人で消える）。跳んでいればよける
        let mut traps = std::mem::take(&mut self.traps);
        for t in traps.iter_mut() {
            t.life -= STEP;
            for j in 0..n {
                if t.life <= 0.0 { break; }
                if t.seen & (1 << j) != 0 || self.runners[j].finished { continue; }
                let x = &self.runners[j];
                if (x.dist - t.dist).abs() > ITEM.trap_len || (x.d - t.d).abs() > t.r { continue; }
                if x.y > 0.2 { t.seen |= 1 << j; self.dodge(j); continue; }
                if self.hit(t.owner, j, Weapon::Bottle, false) { t.life = 0.0; }
            }
        }
        traps.retain(|t| t.life > 0.0);
        self.traps = traps;
    }

    fn collide(&mut self) {
        let t = &TUNE;
        let dt = STEP;
        let (r, w) = (t.r, self.track.width);
        for x in &mut self.runners { x.blocked = false; x.draft = false; }
        let n = self.runners.len();
        for a in 0..n {
            for b in a + 1..n {
                if self.runners[a].finished || self.runners[b].finished { continue; }
                // 空中でも当たる（ジャンプで他の走者を飛び越えられない。企画 03）
                let gap = self.runners[b].dist - self.runners[a].dist;
                let ad = gap.abs();
                let dd = self.runners[a].d - self.runners[b].d;
                let add = dd.abs();
                if ad > t.draft_gap + 1.0 { continue; }
                let (rear, front) = if gap > 0.0 { (a, b) } else { (b, a) };
                if add < t.draft_width && ad < t.draft_gap && ad > t.block_gap * 0.8 { self.runners[rear].draft = true; }
                if add < r * 2.0 * 0.92 && ad < t.block_gap {
                    // 追突: 後ろの走者が軽く減速する。前を押して加速させない
                    if ad > 0.85 {
                        let fs = self.runners[front].speed;
                        let front_player = self.runners[front].is_player;
                        let track_lap = &self.track;
                        let x = &mut self.runners[rear];
                        if x.speed > fs * 0.985 { x.speed = fs * 0.985; }
                        x.blocked = true;
                        if x.blocked_for == 0.0 && (x.is_player || front_player) { self.events.push(RaceEvent::Bump { i: rear, j: front, side: false }); }
                        // 重なりを戻す
                        let push = (t.block_gap - ad) * 0.5 * dt * 8.0;
                        x.dist -= push;
                        x.s = track_lap.wrap(x.s - push);
                    } else {
                        // 横から触れた: 少し横へ滑る
                        let k = (r * 2.0 - add) / (r * 2.0);
                        let side = if dd == 0.0 { if a % 2 == 1 { 1.0 } else { -1.0 } } else { sign(dd) };
                        self.runners[a].d += side * 3.2 * k * dt;
                        self.runners[b].d -= side * 3.2 * k * dt;
                        self.runners[a].d = max(r, min(w - r, self.runners[a].d));
                        self.runners[b].d = max(r, min(w - r, self.runners[b].d));
                        if ad < 0.8 && (self.runners[a].is_player || self.runners[b].is_player) && add < r * 1.5 && self.t > 0.3 { self.events.push(RaceEvent::Bump { i: a, j: b, side: true }); }
                    }
                }
            }
        }
    }

    fn order(&mut self) {
        let prev = self.order.clone();
        let mut now: Vec<usize> = (0..self.runners.len()).collect();
        let rs = &self.runners;
        now.sort_by(|&p, &q| {
            let (p, q) = (&rs[p], &rs[q]);
            if p.finished && q.finished { return p.place.cmp(&q.place); }
            if p.finished { return std::cmp::Ordering::Less; }
            if q.finished { return std::cmp::Ordering::Greater; }
            q.dist.partial_cmp(&p.dist).unwrap()
        });
        for (k, &i) in now.iter().enumerate() { self.runners[i].rank = k as i32 + 1; }
        if self.t.floor() != (self.t - STEP).floor() {
            for x in &mut self.runners { if !x.finished { x.rank_log.push(x.rank); } }
        }
        for x in &mut self.runners {
            if x.finished { continue; }
            // コーナーの出口の順位
            let idx = x.corners.len();
            if let Some(&c) = self.corner_dists.get(idx) && x.dist >= c { x.corners.push(x.rank); }
        }
        if self.t > 3.0 {
            // 追い抜き（順位が上がった走者）
            let mut prev_rank = [0usize; 8];
            for (k, &i) in prev.iter().enumerate() { prev_rank[i] = k; }
            for k in 0..now.len() {
                let i = now[k];
                let was = prev_rank[i];
                if k < was && !self.runners[i].finished {
                    let j = prev[k];
                    self.events.push(RaceEvent::Pass { i, j });
                    self.runners[i].stats.passes += 1;
                }
            }
            if now[0] != self.leader && !self.runners[now[0]].finished { self.leader = now[0]; self.events.push(RaceEvent::Lead { i: now[0] }); }
        } else { self.leader = now[0]; }
        self.order = now.clone();
        // 周回と最後の直線（先頭基準で一度だけ）
        let lead_dist = self.runners[now[0]].dist;
        let laps = self.track.def.laps;
        let lap = laps.min(1 + (max(0.0, lead_dist - self.track.def.start_back) / self.track.lap).floor() as i32);
        if lap != self.lap_shown { self.lap_shown = lap; self.events.push(RaceEvent::Lap { lap }); }
        let last = self.track.curves.iter().rfind(|c| c.s1 < self.track.finish_s).copied();
        let straight_len = match last { Some(c) => self.track.finish_s - c.s1, None => 200.0 };
        if !self.final_announced && lead_dist > self.track.race_len - straight_len {
            self.final_announced = true;
            self.events.push(RaceEvent::FinalStraight);
        }
        let line = self.track.race_len - straight_len;
        for x in &mut self.runners { if x.rank_at_final == 0 && x.dist > line { x.rank_at_final = x.rank; } }
    }

    pub fn step(&mut self, dt: f64, input: Input) {
        // 押した瞬間は刻みまで持ち越す（取りこぼさない）
        if input.jump { self.latched_jump = true; }
        if input.attack { self.latched_attack = true; }
        self.acc += min(0.25, dt);
        while self.acc >= STEP {
            self.acc -= STEP;
            self.tick(Input { x: input.x, dash: input.dash, jump: self.latched_jump, attack: input.attack || self.latched_attack });
            self.latched_jump = false;
            self.latched_attack = false;
        }
    }

    /// プレイヤーがゴールしたあと、残りの走者の着順とタイムを見積もって確定する（結果をすぐ出すため）
    pub fn fast_forward_finish(&mut self) {
        let mut rest: Vec<(usize, f64)> = self.runners.iter().filter(|x| !x.finished).map(|x| (x.i, self.t + (self.track.race_len - x.dist) / max(4.0, x.speed))).collect();
        rest.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        for (i, t) in rest {
            self.finished_count += 1;
            let fc = self.finished_count;
            let x = &mut self.runners[i];
            x.finished = true;
            x.finish_t = t;
            x.stats.final_stamina = x.stamina;
            x.dashing = false;
            x.place = fc;
        }
        self.order();
        self.done = true;
    }
}

