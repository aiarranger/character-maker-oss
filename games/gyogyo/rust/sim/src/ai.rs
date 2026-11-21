// CPU: 見えている状況（前方の空き・スタミナ・残り距離）だけで判断する。プレイヤーの入力は読まない。順位だけを理由に加速しない（企画 09）。src/sim/ai.ts の写し。
// 速さは変えず、判断の遅さとミスの多さで難しさを分ける。作戦で性格を分ける。
use crate::items::{ITEM, Pickup, PickupKind, Shot, Trap, Weapon};
use crate::race::{Input, ObEntry, Rng, Runner, TUNE};
use crate::track::{ObstacleKind, Track};
use sim_math::{max, min};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tactic { Front, Closer, Inner, Gap }
impl Tactic {
    pub fn id(self) -> &'static str { match self { Tactic::Front => "front", Tactic::Closer => "closer", Tactic::Inner => "inner", Tactic::Gap => "gap" } }
    /// 脚質の名前（競馬の逃げ・先行・差し・追込）
    pub fn name(self) -> &'static str { match self { Tactic::Front => "逃げ", Tactic::Inner => "先行", Tactic::Gap => "差し", Tactic::Closer => "追込" } }
    /// 脚質の走りの配分（CPU の速さの倍率。p はレースの進み 0..1）。逃げは前半に飛ばして後半に落ち、追込は前半を抑えて後半に伸びる。
    /// 集団を縦に伸ばして、前後の距離を取る（8 人がほぼ同じ速さで固まっていた）
    pub fn pace(self, p: f64) -> f64 {
        match self {
            Tactic::Front => if p < 0.6 { 1.02 } else { 0.98 },
            Tactic::Inner => if p < 0.6 { 1.0 } else { 1.005 },
            Tactic::Gap => if p < 0.55 { 0.975 } else { 1.025 },
            Tactic::Closer => if p < 0.6 { 0.955 } else { 1.045 },
        }
    }
}

/// 難しさごとの CPU の速さの倍率（判断する CPU だけ。順位は見ない）。
/// プレイヤー役を自動操作して測り、ふつうの人がふつうで 3 着以内に入れる値
pub fn cpu_pace(d: Difficulty) -> f64 { match d { Difficulty::Easy => CPU_PACE[0], Difficulty::Normal => CPU_PACE[1], Difficulty::Hard => CPU_PACE[2] } }
pub const CPU_PACE: [f64; 3] = [0.985, 0.97, 0.99];
/// 難しさごとの、人の走者のダッシュの息の減りの倍率。ダッシュを使うほど得をし、何もしない人は得をしない
pub fn human_drain(d: Difficulty) -> f64 { match d { Difficulty::Easy => HUMAN_DRAIN[0], Difficulty::Normal => HUMAN_DRAIN[1], Difficulty::Hard => HUMAN_DRAIN[2] } }
pub const HUMAN_DRAIN: [f64; 3] = [0.45, 0.6, 0.85];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Difficulty { Easy, Normal, Hard }
impl Difficulty {
    pub fn id(self) -> &'static str { match self { Difficulty::Easy => "easy", Difficulty::Normal => "normal", Difficulty::Hard => "hard" } }
    pub fn parse(s: &str) -> Option<Difficulty> { match s { "easy" => Some(Difficulty::Easy), "normal" => Some(Difficulty::Normal), "hard" => Some(Difficulty::Hard), _ => None } }
}

// bar・rest: スタミナの使いどころの判断の遅さ（大きいほど満タンのまま回復を捨て、使い切れない）。速さやスタミナの規則はどの難しさでも同じ
#[derive(Clone, Copy, Debug)]
// want: 落ちている物を取りに寄る強さ。use_p: 当たる相手がいるときに武器を使う割合。charge_p: 息をためる割合。dodge: 飛んでくる攻撃・わなを跳んでよける割合
struct Skill { react: f64, jump_miss: f64, aim: f64, spurt_slop: f64, bar: f64, rest: f64, want: f64, use_p: f64, charge_p: f64, dodge: f64 }
const fn skill(d: Difficulty) -> Skill {
    match d {
        Difficulty::Easy => Skill { react: 0.55, jump_miss: 0.28, aim: 0.8, spurt_slop: 0.6, bar: 30.0, rest: 11.0, want: 0.5, use_p: 0.45, charge_p: 0.1, dodge: 0.15 },
        Difficulty::Normal => Skill { react: 0.3, jump_miss: 0.1, aim: 0.4, spurt_slop: 0.8, bar: 20.0, rest: 6.0, want: 0.8, use_p: 0.75, charge_p: 0.3, dodge: 0.5 },
        Difficulty::Hard => Skill { react: 0.14, jump_miss: 0.02, aim: 0.1, spurt_slop: 0.98, bar: 10.0, rest: 3.0, want: 1.0, use_p: 0.95, charge_p: 0.5, dodge: 0.85 },
    }
}

const CANDS: i32 = 15;

/// createBrain の閉じた値（me は runners の番号）
#[derive(Clone, Debug)]
pub struct Brain {
    me: usize,
    skill: Skill,
    gentle: bool,
    jump_only: bool,
    // form: この走者の上手さ（1 が基準、大きいほどスタミナを使い残す）。上から下まで段があり、後方でも次の一人を追える（企画 09）
    form: f64,
    think: f64,
    target: f64,
    burst: f64,
    cooling: f64,
    planned_jump: Option<f64>,
    /// 攻撃を押し続ける残りの秒（0 になったら離す＝振る）と、次に武器を考えるまでの秒
    atk_hold: f64,
    atk_think: f64,
    /// 最後に「よけるか」を決めた攻撃・わなの番号と、その答え
    threat: Option<(u32, bool)>,
    out: Input,
}

/// 脳が読む盤面（TS の race）
pub struct View<'a> { pub track: &'a Track, pub runners: &'a [Runner], pub obstacle_list: &'a [ObEntry], pub pickups: &'a [Pickup], pub shots: &'a [Shot], pub traps: &'a [Trap], pub rand: &'a mut Rng }

impl Brain {
    /// createBrain。race.rand を 1 回読む
    pub fn new(me: usize, runners: &[Runner], rand: &mut Rng, difficulty: Difficulty, gentle: bool, jump_only: bool, form: Option<f64>) -> Brain {
        let sk = skill(difficulty);
        let think = rand.next() * sk.react;
        Brain { me, skill: sk, gentle, jump_only, form: form.unwrap_or(1.0), think, target: runners[me].d, burst: 0.0, cooling: 0.0, planned_jump: None, atk_hold: 0.0, atk_think: 0.0, threat: None, out: Input::NONE }
    }

    fn lane_clear(&self, v: &View, c: f64, look: f64) -> f64 {
        let me = &v.runners[self.me];
        let mut clear = look;
        for (i, o2) in v.runners.iter().enumerate() {
            if i == self.me || o2.finished { continue; }
            let gap = o2.dist - me.dist;
            if gap < -1.1 || gap > look { continue; }
            if (o2.d - c).abs() < 1.08 { clear = min(clear, max(0.0, gap)); }
        }
        clear
    }

    fn choose_lane(&mut self, v: &mut View) {
        let (w, r) = (v.track.width, TUNE.r);
        let look = 26.0;
        let (me_d, me_s, me_dist, tactic) = { let m = &v.runners[self.me]; (m.d, m.s, m.dist, m.tactic) };
        // 少し先の曲がりを見る（内を取る価値）
        let mut curve: f64 = 0.0;
        let mut k = 10;
        while k <= 70 { curve = max(curve, v.track.curv(me_s + k as f64).abs()); k += 15; }
        let curve_w = curve * 60.0;
        let (mut best, mut best_score) = (self.target, f64::NEG_INFINITY);
        for q in 0..CANDS {
            let c = max(r, min(w - r, me_d + (q as f64 - (CANDS - 1) as f64 / 2.0) * 0.62));
            let clear = self.lane_clear(v, c, look);
            let mut score = (clear / look) * (if tactic == Tactic::Gap { 1.25 } else if tactic == Tactic::Inner { 0.85 } else { 1.1 });
            // 内の短さ
            score -= (c / w) * (0.3 + curve_w) * (if tactic == Tactic::Inner { 1.25 } else if tactic == Tactic::Gap { 1.05 } else { 1.2 });
            // 手数を増やさない
            score -= (c - me_d).abs() * 0.035;
            // 障害: 空きを探す性格ほど迂回する
            for e in v.obstacle_list {
                let g = e.dist - me_dist;
                if !(0.0..=50.0).contains(&g) { continue; }
                if c > e.ob.d0 && c < e.ob.d1 { score -= (if tactic == Tactic::Gap { 0.3 } else if tactic == Tactic::Closer { 0.2 } else { 0.12 }) * (1.0 - g / 60.0); }
            }
            // 落ちている物: マーク・息・武器へ寄り、毒はよける。先に取りそうな走者が前にいる物は追わない
            let (stamina, armed, from) = { let m = &v.runners[self.me]; (m.stamina, m.weapon.is_some(), m.next_pick) };
            for p in v.pickups.iter().skip(from) {
                let g = p.dist - me_dist;
                if g > 45.0 { break; }
                if p.taken || g < 2.0 || (c - p.d).abs() > 0.7 { continue; }
                let mut val = match p.kind {
                    PickupKind::Mark(_) => 0.22,
                    PickupKind::Breath => if stamina < 55.0 { 0.26 } else { 0.08 },
                    PickupKind::Poison => -0.3,
                    PickupKind::Arm(_) | PickupKind::Mystery => if armed { 0.05 } else { 0.26 },
                    PickupKind::Jet => 0.3,
                };
                if val > 0.0 && v.runners.iter().any(|o| o.i != self.me && !o.finished && o.dist > me_dist && o.dist < p.dist && (o.d - p.d).abs() < 1.0) { val *= 0.2; }
                score += val * (1.0 - g / 50.0) * if val > 0.0 { self.skill.want } else { 1.0 };
            }
            // 床のわな
            for t in v.traps {
                let g = t.dist - me_dist;
                if t.owner != self.me && g > 0.0 && g < 30.0 && (c - t.d).abs() < t.r + 0.5 { score -= 0.3 * (1.0 - g / 40.0); }
            }
            if (c - self.target).abs() < 0.4 { score += 0.05; }
            if score > best_score { best_score = score; best = c; }
        }
        self.target = best + (v.rand.next() - 0.5) * self.skill.aim * 0.3;
    }

    fn go(&mut self, v: &mut View, clear: f64, bar: f64, sec: f64) -> bool {
        let stamina = v.runners[self.me].stamina;
        if stamina > min(96.0, bar + self.skill.bar + (self.form - 1.0) * 16.0) && clear > 6.0 {
            self.burst = sec * (0.85 + v.rand.next() * 0.3);
            return true;
        }
        false
    }

    fn dash_decision(&mut self, v: &mut View) -> bool {
        let (me_d, me_s, me_dist, stamina, exhausted, blocked, tactic) = { let m = &v.runners[self.me]; (m.d, m.s, m.dist, m.stamina, m.exhausted, m.blocked, m.tactic) };
        let remaining = v.track.race_len - me_dist;
        let dash_speed = TUNE.base * TUNE.dash_mul;
        let all_in = (stamina / TUNE.drain) * dash_speed;
        let clear = self.lane_clear(v, me_d, 16.0);
        if exhausted > 0.0 { return false; }
        if blocked || clear < 2.5 { self.burst = 0.0; return false; }
        if self.burst > 0.0 { return true; }
        let can_spurt = remaining < all_in * self.skill.spurt_slop;
        if can_spurt { return true; }
        if self.cooling > 0.0 { return false; }
        if self.gentle {
            if me_dist < 30.0 { return false; }
            if v.rand.next() < 0.0015 && stamina > 70.0 { self.burst = 1.1; return true; }
            return false;
        }
        let p = me_dist / v.track.race_len;
        let curv = v.track.curv(me_s).abs();
        // 作戦ごとの使いどころ（序盤の位置取り・中盤に一人抜く・終盤の仕掛け）。使う量はどの作戦もほぼ同じで、使う時期が違う
        match tactic {
            Tactic::Front => {
                if p < 0.2 { return self.go(v, clear, 30.0, 1.8); }
                if p < 0.72 { return if curv < 0.004 { self.go(v, clear, 48.0, 1.5) } else { self.go(v, clear, 70.0, 1.2) }; }
                self.go(v, clear, 50.0, 1.5)
            }
            Tactic::Closer => {
                if p < 0.2 { return self.go(v, clear, 70.0, 1.2); }
                if p < 0.5 { return if curv < 0.004 { self.go(v, clear, 88.0, 1.3) } else { false }; }
                self.go(v, clear, 50.0, 1.8)
            }
            Tactic::Inner => {
                if p < 0.12 { return self.go(v, clear, 40.0, 1.4); }
                if me_d < 4.0 && clear > 10.0 { self.go(v, clear, 52.0, 1.6) } else { self.go(v, clear, 84.0, 1.3) }
            }
            Tactic::Gap => {
                let ahead = self.lane_clear(v, me_d, 24.0);
                if ahead > 5.0 && ahead < 24.0 { return self.go(v, clear, 38.0, 1.6); }
                self.go(v, clear, 58.0, 1.5)
            }
        }
    }

    /// いま持っている武器が当たる相手がいるか。Some(ためる値打ちがあるか)
    fn target(&self, v: &View, w: Weapon) -> Option<bool> {
        let me = &v.runners[self.me];
        let mut best: Option<bool> = None;
        for o in v.runners {
            if o.i == self.me || o.finished || o.immune > 0.0 { continue; }
            let (gap, dd) = (o.dist - me.dist, (o.d - me.d).abs());
            let hit = match w {
                Weapon::Sword => if gap > 0.3 && gap <= ITEM.sword_reach - 0.3 && dd <= ITEM.sword_side - 0.2 { Some(false) } else if gap > 6.0 && gap < 24.0 && dd <= ITEM.wave_hw - 0.3 { Some(true) } else { None },
                Weapon::Chain => (gap.abs() <= ITEM.chain_reach - 0.3 && dd <= ITEM.chain_side - 0.3).then_some(false),
                Weapon::Rose => (gap > 4.0 && gap < 21.0 && dd <= ITEM.rose_hw - 0.15).then_some(gap > 9.0),
                Weapon::Bottle => (gap < -1.8 && gap > -9.0 && dd <= ITEM.trap_r).then_some(false),
                // ボーガンは誰がどこにいても 1 周で当たる。すぐ撃つ
                Weapon::Crossbow => Some(false),
                Weapon::Hammer => (gap > 0.3 && gap <= ITEM.hammer_reach - 0.3 && dd <= ITEM.hammer_side - 0.2).then_some(false),
            };
            // 近くを切れるなら、ためずに切る
            match hit { Some(false) => return Some(false), Some(true) => best = Some(true), None => {} }
        }
        best
    }

    /// 攻撃のボタン。押して離すと振る。ためるときは満ちるまで押し続ける
    fn attack_decision(&mut self, dt: f64, v: &mut View) -> bool {
        let (weapon, cd, stamina, stumble) = { let m = &v.runners[self.me]; (m.weapon, m.swing_cd, m.stamina, m.stumble) };
        let Some(w) = weapon else { self.atk_hold = 0.0; return false };
        if self.atk_hold > 0.0 { self.atk_hold -= dt; return self.atk_hold > 0.0; }
        self.atk_think -= dt;
        if self.atk_think > 0.0 || cd > 0.0 || stumble > 0.0 { return false; }
        self.atk_think = 0.08 + self.skill.react * (0.4 + v.rand.next() * 0.6);
        let Some(far) = self.target(v, w) else { return false };
        if v.rand.next() > self.skill.use_p { return false; }
        // 遠い相手（衝撃波・扇のバラ）は、息に余裕があればためる。剣の衝撃波はためないと出ない
        let charge = far && stamina > 50.0 && (w == Weapon::Sword || v.rand.next() < self.skill.charge_p);
        if far && w == Weapon::Sword && !charge { return false; }
        self.atk_hold = if charge { ITEM.charge_full + 0.06 } else { 0.03 };
        true
    }

    /// 飛んでくる攻撃・床のわなを跳んでよけるか（1 つにつき 1 回だけ決める。上手いほどよける）
    fn dodge_decision(&mut self, v: &mut View) -> bool {
        let (me_d, me_dist, me_speed, me_air) = { let m = &v.runners[self.me]; (m.d, m.dist, m.speed, m.air) };
        if me_air { return false; }
        let air_t = 2.0 * TUNE.jump_v / TUNE.gravity;
        let mut id = None;
        for s in v.shots {
            if s.owner == self.me || s.seen & (1 << self.me) != 0 || (s.d - me_d).abs() > s.hw + 0.1 { continue; }
            // ボーガンの矢は周回をまたいで来る
            let behind = if s.kind == crate::items::ShotKind::Bolt { let lap = v.track.lap; let g = (me_dist - s.dist).rem_euclid(lap); if g > lap / 2.0 { g - lap } else { g } } else { me_dist - s.dist };
            let close = s.v - me_speed;
            if behind > 0.0 && close > 1.0 && behind / close < air_t * 0.4 { id = Some(s.id); break; }
        }
        if id.is_none() {
            for t in v.traps {
                if t.owner == self.me || t.seen & (1 << self.me) != 0 || (t.d - me_d).abs() > t.r + 0.1 { continue; }
                let g = t.dist - me_dist;
                if g > 0.0 && g < me_speed * air_t * 0.45 { id = Some(t.id); break; }
            }
        }
        let Some(id) = id else { return false };
        if self.threat.is_none_or(|(k, _)| k != id) { self.threat = Some((id, v.rand.next() < self.skill.dodge)); }
        self.threat.is_some_and(|(_, go)| go)
    }

    fn jump_decision(&mut self, v: &mut View) -> bool {
        let (me_d, me_dist, me_speed, me_air) = { let m = &v.runners[self.me]; (m.d, m.dist, m.speed, m.air) };
        let look = me_speed * 0.72;
        for e in v.obstacle_list {
            let g = e.dist - me_dist;
            if g < -e.ob.len { continue; }
            if g > look + 4.0 { break; }
            if me_d < e.ob.d0 + 0.2 || me_d > e.ob.d1 - 0.2 { continue; }
            if self.planned_jump.is_none() {
                // 人のように踏み切りがぶれる。たまに遅れる
                let miss = v.rand.next() < self.skill.jump_miss;
                let air_t = 2.0 * TUNE.jump_v / TUNE.gravity;
                let ideal = if e.ob.kind == ObstacleKind::Hurdle { me_speed * air_t * 0.5 } else { me_speed * air_t * 0.14 };
                self.planned_jump = Some(if miss { -9.0 } else { ideal + (v.rand.next() - 0.5) * self.skill.aim * 1.6 });
            }
            let pj = self.planned_jump.unwrap();
            if pj != -9.0 && g <= pj && !me_air { self.planned_jump = None; return true; }
            return false;
        }
        self.planned_jump = None;
        false
    }

    /// TS の brain(dt)
    pub fn decide(&mut self, dt: f64, v: &mut View) -> Input {
        self.out.jump = self.jump_decision(v);
        if self.jump_only { return self.out; }
        if !self.out.jump && (!v.shots.is_empty() || !v.traps.is_empty()) { self.out.jump = self.dodge_decision(v); }
        self.out.attack = self.attack_decision(dt, v);
        self.think -= dt;
        if self.burst > 0.0 {
            self.burst -= dt;
            if self.burst <= 0.0 { self.cooling = self.skill.rest * self.form * (0.6 + v.rand.next() * 0.8); }
        }
        if self.cooling > 0.0 { self.cooling -= dt; }
        if self.think <= 0.0 {
            self.think = self.skill.react * (0.6 + v.rand.next() * 0.8);
            self.choose_lane(v);
            self.out.dash = self.dash_decision(v);
        } else if self.out.dash {
            let m = &v.runners[self.me];
            if m.exhausted > 0.0 || m.blocked { self.out.dash = false; }
        }
        let e = self.target - v.runners[self.me].d;
        self.out.x = if e.abs() < 0.12 { 0.0 } else { max(-1.0, min(1.0, e / 0.7)) };
        self.out
    }
}
