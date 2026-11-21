// レースの状態 → 主役の部品（characters の character-actor）への合図。src/hero/hero-signals.ts の写し。
// 共通ルール: 走り・ジャンプ・のけぞり・＞＜・身振りの体と顔の動きは部品、いつ・どの速さで走るかはこのゲーム。
use crate::race::{Runner, TUNE};
use sim_math::max;

#[derive(Clone, Copy, Debug, Default)]
pub struct HeroPrev { pub air: bool, pub stumble: bool, pub finished: bool, pub cheered: bool }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeroCommand { Jump { air_seconds: f64 }, Hurt { seconds: f64 }, Banzai }

/// このコマの合図（順番も TS と同じ）。prev は書き換える
pub fn hero_commands(r: &Runner, prev: &mut HeroPrev) -> Vec<HeroCommand> {
    let mut out = Vec::new();
    if r.air && !prev.air && r.vy > 0.0 { out.push(HeroCommand::Jump { air_seconds: 2.0 * TUNE.jump_v / TUNE.gravity }); }
    if r.stumble > 0.0 && !prev.stumble { out.push(HeroCommand::Hurt { seconds: max(0.4, TUNE.stumble_s) }); }
    // ゴール後、足がゆるんだら喜ぶ（3 着まで）
    if r.finished && !prev.cheered && r.place <= 3 && r.speed < 6.0 { prev.cheered = true; out.push(HeroCommand::Banzai); }
    prev.air = r.air;
    prev.stumble = r.stumble > 0.0;
    prev.finished = r.finished;
    out
}

/// 歩き・走りに切り替わる速さ（部品の空間。場では 0.6・3.4）
pub fn hero_gait(scale: f64) -> (f64, f64) { (0.6 / scale, 3.4 / scale) }

/// 走りの足の運びの再生の速さ。速いほど回転が上がる（地面を滑って見えないように進む速さへ合わせる）。ダッシュは足の回転で速さを見せる（企画 04）
pub fn hero_rate(speed: f64, dash_k: f64) -> f64 { (0.55 + speed * 0.1) * (1.0 + dash_k * 0.12) }
