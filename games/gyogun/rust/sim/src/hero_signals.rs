// 戦いの状態 → 主役の部品への合図。
// 共通ルール: のけぞり・＞＜・回避・ジャンプ・振り・必殺技の体の動きは部品、
// どの技を出すか・割り込めるか・当たりの時刻・無敵・技の数値・世界の位置と向きはこのゲーム。ここは描画を読まない。
use crate::battle::{Player, PlayerState, DODGE_T, GRAVITY, JUMP_V, WIRE_AIM};
use crate::moves::{same, AnimClip, RANBU};

/// 前のコマの状態（呼ぶ側が持つ）
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeroPrev { pub move_key: i64, pub state: PlayerState, pub air: bool }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeroStartKind { Attack, Skill }

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct HeroStartOpts { pub seconds: f64, pub from: Option<f64>, pub to: Option<f64>, pub ease_swing: bool, pub cycle: Option<f64> }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HeroCommand {
    Start { kind: HeroStartKind, opts: HeroStartOpts },
    Dodge { seconds: f64 },
    Jump { air_seconds: f64 },
    /// 再生中の動作を止めて、のけぞりと ＞＜。seconds は立ち直るまで（遊びの hurtT。倒れたときは無し＝部品の既定）
    Hurt { seconds: Option<f64> },
    /// 技が途中で終わった（着地など）: 再生中の動作を止める
    Stop,
}

/// 乱舞は必殺技の動きを、この秒数で1回ずつ繰り返す（部品へ入れる前と同じ）
const RANBU_CYCLE: f64 = 1.0 / 0.9;

/// このコマの合図。prev は書き換える
pub fn hero_commands(p: &Player, prev: &mut HeroPrev) -> Vec<HeroCommand> {
    let mut out = vec![];
    let entered = |s: PlayerState| p.state == s && prev.state != s;
    if entered(PlayerState::Hurt) { out.push(HeroCommand::Hurt { seconds: Some(p.hurt_t) }); }
    else if entered(PlayerState::Dead) { out.push(HeroCommand::Hurt { seconds: None }); }
    else if p.state == PlayerState::Attack && p.r#move.is_some() && p.move_key != prev.move_key {
        let m = p.r#move.unwrap();
        out.push(if same(m, &RANBU) {
            HeroCommand::Start { kind: HeroStartKind::Skill, opts: HeroStartOpts { seconds: m.dur, cycle: Some(RANBU_CYCLE), ..Default::default() } }
        } else {
            HeroCommand::Start {
                kind: if m.anim.clip == AnimClip::Skill { HeroStartKind::Skill } else { HeroStartKind::Attack },
                opts: HeroStartOpts { seconds: m.dur, from: Some(m.anim.from), to: Some(m.anim.to), ease_swing: true, cycle: None },
            }
        });
    } else if entered(PlayerState::Dodge) { out.push(HeroCommand::Dodge { seconds: DODGE_T }); }
    // 飴糸: 飛んでいる間は跳ぶ動き（着いたあと落ちるまでの分も足す）。着いて空中に残っても跳び直さない
    else if entered(PlayerState::Wire) { out.push(HeroCommand::Jump { air_seconds: WIRE_AIM + p.wire.dur + 0.9 }); }
    else if p.state == PlayerState::Free && p.air && !prev.air && p.vy > 0.0 { out.push(HeroCommand::Jump { air_seconds: 2.0 * JUMP_V / GRAVITY }); }
    else if prev.state == PlayerState::Attack && p.state != PlayerState::Attack { out.push(HeroCommand::Stop); }
    prev.move_key = p.move_key; prev.state = p.state; prev.air = p.air;
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeroGait { pub walk: f64, pub run: f64 }

/// 歩き・走りに切り替わる速さ（部品の空間。場では 0.6・5）
pub fn hero_gait(scale: f64) -> HeroGait { HeroGait { walk: 0.6 / scale, run: 5.0 / scale } }

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeroMove { pub speed: f64, pub grounded: bool, pub armed: f64, pub gait: HeroGait }

/// 移動の合図（速さ・切り替えの速さは主役の大きさ scale に直す）。部品へ入れる前と同じく、地面で 0.6 を超えると歩き、5 を超えると走り。止まっていれば 0.85 で構える
pub fn hero_move(p: &Player, scale: f64) -> HeroMove {
    let free = p.state == PlayerState::Free && p.y < 0.05;
    HeroMove {
        speed: if free { p.speed / scale } else { 0.0 },
        // わたあめ雲の上では、立って構える（雲が足場）
        grounded: !p.air || p.state == PlayerState::Ride,
        armed: if p.state == PlayerState::Free || p.state == PlayerState::Ride { 0.85 } else { 0.0 },
        gait: hero_gait(scale),
    }
}
