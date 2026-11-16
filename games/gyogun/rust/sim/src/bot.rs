// 自動プレイ: 砦 → 本陣の順に進み、群れに通常攻撃と強攻撃を混ぜ、予告の円からは避け、ゲージが満ちたら乱舞する。
// 釣り合い（時間・被ダメージ・撃破数）を数で確かめるのと、タイトルの裏の見本に使う。
use crate::battle::{rng, Battle, BossState, Input, PlayerState, Rng, TelegraphKind, Owner, Role, EnemyState, GATE, NO_INPUT};
use crate::math::{hypot, or1};

pub struct Bot {
    skill: f64,
    t: f64,
    /// 何回の通常攻撃のあとで強攻撃を出すか
    plan: f64,
    pressed: f64,
    wait: f64,
    /// TS の Math.random の代わり（同じ mulberry32）。呼ぶ順番も同じ
    random: Rng,
    /// 確かめ用: Math.random の値を外から入れる（今の版と同じ乱数で見本の戦いを比べる）
    pub source: Option<fn() -> f64>,
}

/// TS の createBot(b, skill)。戦いの状態は think に毎回渡す（Rust では持ち続けられないので）。random_seed は Math.random の代わりの種
pub fn create_bot(_b: &Battle, skill: f64, random_seed: u32) -> Bot {
    Bot { skill, t: 0.0, plan: 0.0, pressed: 0.0, wait: 0.0, random: rng(random_seed), source: None }
}

impl Bot {
    /// いまの Math.random 相当の状態（検査用）
    pub fn random_state(&self) -> u32 { self.random.state() }
    fn rnd(&mut self) -> f64 { match self.source { Some(f) => f(), None => self.random.next() } }

    pub fn think(&mut self, b: &Battle, dt: f64) -> Input {
        self.t += dt;
        let skill = self.skill;
        let p = &b.player;
        let mut input = NO_INPUT;
        if p.state == PlayerState::Dead { return input; }
        // 目標: 敵の砦 → 本陣の門 → 大将
        let (mut tx, mut tz) = (GATE.x, GATE.z);
        if b.holds.iter().any(|h| h.owner == Owner::Enemy) {
            let mut bd = 1e9;
            for h in b.holds.iter().filter(|h| h.owner == Owner::Enemy) { let d = hypot(h.x - p.x, h.z - p.z); if d < bd { bd = d; tx = h.x; tz = h.z; } }
            // 隊長が出ていれば隊長へ
            for e in &b.enemies { if e.alive && e.role == Role::Captain && e.state != EnemyState::Dead { tx = e.x; tz = e.z; break; } }
        }
        if b.boss.state != BossState::Dormant && b.boss.state != BossState::Dead { tx = b.boss.x; tz = b.boss.z; }
        // 近くの敵
        let (mut near, mut nx, mut nz, mut nd) = (0, 0.0, 0.0, 1e9);
        for e in &b.enemies {
            if !e.alive || e.state == EnemyState::Dead { continue; }
            let d = hypot(e.x - p.x, e.z - p.z);
            if d < 6.0 { near += 1; }
            if d < nd { nd = d; nx = e.x; nz = e.z; }
        }
        let fight_target = near >= 3 || nd < 3.2;
        let (mut gx, mut gz) = (tx - p.x, tz - p.z);
        if fight_target && nd < 6.0 { gx = nx - p.x; gz = nz - p.z; }
        let gl = or1(hypot(gx, gz));
        input.mx = gx / gl; input.mz = gz / gl;
        // 予告から逃げる
        for tel in &b.telegraphs {
            if tel.done { continue; }
            let (dx, dz) = (p.x - tel.x, p.z - tel.z);
            let d = hypot(dx, dz);
            let danger = if tel.kind == TelegraphKind::Ring { tel.t > tel.fire - 0.25 && tel.t < tel.fire + 0.5 && d < 14.0 } else { d < tel.r + 1.2 };
            if danger && tel.fire - tel.t < 0.9 * skill + 0.1 {
                if tel.kind == TelegraphKind::Ring { if tel.t >= tel.fire - 0.05 { input.jump = self.rnd() < 0.25 * skill; } }
                else { input.mx = dx / or1(d); input.mz = dz / or1(d); if self.rnd() < 0.08 * skill { input.dodge = true; } }
                return input;
            }
        }
        if p.ranbu >= 100.0 && (near >= 6 || (b.boss.state != BossState::Dormant && hypot(b.boss.x - p.x, b.boss.z - p.z) < 8.0)) { input.ranbu = true; return input; }
        let boss_near = b.boss.state != BossState::Dormant && b.boss.state != BossState::Dead && hypot(b.boss.x - p.x, b.boss.z - p.z) < 7.0;
        if fight_target || boss_near {
            self.wait -= dt;
            if self.wait <= 0.0 {
                self.wait = 0.09 / skill;
                if p.state == PlayerState::Free { self.plan = (self.rnd() * 6.0).floor(); self.pressed = 0.0; }
                if self.pressed < self.plan { input.attack = true; self.pressed += 1.0; }
                else { input.heavy = true; self.pressed = 99.0; }
                if p.state == PlayerState::Free && self.pressed > self.plan { input.attack = true; self.pressed = 1.0; }
            }
        }
        input
    }
}
