// hero_signals と aim_shape の受入条件（src/hero/hero-signals.test.ts・src/view/aim-shape.test.ts と同じ内容）。
use sim::aim_shape::{aim_shape, in_aim};
use sim::battle::{create_battle, Player, PlayerState, DODGE_T, GRAVITY, JUMP_V};
use sim::hero_signals::*;
use sim::moves::{Knock, Move, CHARGES, DASH_ATTACK, JUMP_CHARGE, RANBU, NORMALS};

fn player() -> Player { let mut p = create_battle(1, false).player; p.state = PlayerState::Free; p.r#move = None; p.move_key = 0; p.air = false; p.vy = 0.0; p.y = 0.0; p.speed = 0.0; p }
fn attacking(m: &'static Move, key: i64) -> Player { let mut p = player(); p.state = PlayerState::Attack; p.r#move = Some(m); p.move_key = key; p }
fn prev() -> HeroPrev { HeroPrev { move_key: 0, state: PlayerState::Free, air: false } }
fn is_start(v: &[HeroCommand]) -> bool { matches!(v.first(), Some(HeroCommand::Start { .. })) }

#[test]
fn 技が始まったコマに始めて続くコマでは始め直さない() {
    let mut pr = prev();
    let n0 = &NORMALS[0];
    assert_eq!(hero_commands(&attacking(n0, 1), &mut pr), vec![HeroCommand::Start { kind: HeroStartKind::Attack, opts: HeroStartOpts { seconds: n0.dur, from: Some(n0.anim.from), to: Some(n0.anim.to), ease_swing: true, cycle: None } }]);
    assert!(hero_commands(&attacking(n0, 1), &mut pr).is_empty());
    // 連続技: 次の段（moveKey が進む）でまた始める
    assert!(is_start(&hero_commands(&attacking(&NORMALS[1], 2), &mut pr)));
}

#[test]
fn 必殺技のクリップはskillで乱舞は繰り返す() {
    let c6 = &CHARGES[6];
    assert_eq!(hero_commands(&attacking(c6, 1), &mut prev()), vec![HeroCommand::Start { kind: HeroStartKind::Skill, opts: HeroStartOpts { seconds: c6.dur, from: Some(c6.anim.from), to: Some(c6.anim.to), ease_swing: true, cycle: None } }]);
    assert_eq!(hero_commands(&attacking(&RANBU, 1), &mut prev()), vec![HeroCommand::Start { kind: HeroStartKind::Skill, opts: HeroStartOpts { seconds: RANBU.dur, from: None, to: None, ease_swing: false, cycle: Some(1.0 / 0.9) } }]);
}

#[test]
fn 回避は遊びと同じ長さで疾風へつなぐと攻撃を始める() {
    let mut pr = prev();
    let mut d = player(); d.state = PlayerState::Dodge;
    assert_eq!(hero_commands(&d, &mut pr), vec![HeroCommand::Dodge { seconds: DODGE_T }]);
    assert!(hero_commands(&d, &mut pr).is_empty());
    assert!(is_start(&hero_commands(&attacking(&DASH_ATTACK, 1), &mut pr)));
}

#[test]
fn ジャンプの長さと吹き飛ばしはジャンプではない() {
    let mut j = player(); j.air = true; j.vy = JUMP_V;
    assert_eq!(hero_commands(&j, &mut prev()), vec![HeroCommand::Jump { air_seconds: 2.0 * JUMP_V / GRAVITY }]);
    let mut h = player(); h.state = PlayerState::Hurt; h.air = true; h.vy = 6.0; h.hurt_t = 0.75;
    assert_eq!(hero_commands(&h, &mut prev()), vec![HeroCommand::Hurt { seconds: Some(0.75) }]);
}

#[test]
fn 被弾と倒れと技の途中終わり() {
    let mut pr = prev();
    hero_commands(&attacking(&NORMALS[0], 1), &mut pr);
    let mut h = player(); h.state = PlayerState::Hurt; h.move_key = 1; h.hurt_t = 0.42;
    assert_eq!(hero_commands(&h, &mut pr), vec![HeroCommand::Hurt { seconds: Some(0.42) }]);
    assert!(hero_commands(&h, &mut pr).is_empty());
    let mut pr2 = prev();
    hero_commands(&attacking(&NORMALS[0], 1), &mut pr2);
    let mut dd = player(); dd.state = PlayerState::Dead; dd.move_key = 1;
    assert_eq!(hero_commands(&dd, &mut pr2), vec![HeroCommand::Hurt { seconds: None }]);
    let mut pr3 = prev();
    hero_commands(&attacking(&NORMALS[0], 1), &mut pr3);
    let mut f = player(); f.move_key = 1;
    assert_eq!(hero_commands(&f, &mut pr3), vec![HeroCommand::Stop]);
}

#[test]
fn 歩き走り構えの切り替え() {
    let scale = 2.05 / 1.2;
    let mut p = player(); p.speed = 3.0;
    assert!((hero_move(&p, scale).speed - 3.0 / scale).abs() < 1e-9);
    let g = hero_move(&player(), scale).gait;
    assert!((g.walk - 0.6 / scale).abs() < 1e-9 && (g.run - 5.0 / scale).abs() < 1e-9);
    let mut p = player(); p.speed = 3.0; p.y = 0.2;
    assert_eq!(hero_move(&p, scale).speed, 0.0, "跳ねている間は歩きにしない");
    let mut p = player(); p.state = PlayerState::Attack; p.speed = 3.0;
    assert_eq!(hero_move(&p, scale).speed, 0.0);
    assert_eq!(hero_move(&p, scale).armed, 0.0);
    let mut p = player(); p.air = true;
    assert!(!hero_move(&p, scale).grounded);
    assert_eq!(hero_move(&player(), scale).armed, 0.85);
}

#[test]
fn 敵を弾にする技だけが装填の範囲を持つ() {
    assert!(aim_shape(&NORMALS[0]).is_none(), "一の太刀は寄せるだけ");
    assert_eq!(aim_shape(&NORMALS[5]).unwrap().hit.kb, Knock::Blast, "六の太刀は吹き飛ばし");
    assert_eq!(aim_shape(&CHARGES[2]).unwrap().hit.kb, Knock::Blast, "旋風斬は締めの吹き飛ばし");
    assert_eq!(aim_shape(&JUMP_CHARGE).unwrap().hit.kb, Knock::Launch, "墜星は打ち上げ");
}

#[test]
fn 烈波の当たる帯にいる敵だけを装填として数える() {
    let s = aim_shape(&CHARGES[3]).unwrap();
    // 主役は原点で +Z を向く
    assert!(in_aim(&s, 0.0, 0.0, 0.0, 0.0, 8.0, 0.5), "正面 8m");
    assert!(!in_aim(&s, 0.0, 0.0, 0.0, 0.0, -3.0, 0.5), "背後");
    assert!(!in_aim(&s, 0.0, 0.0, 0.0, 5.0, 8.0, 0.5), "帯の外");
}

#[test]
fn 飴糸で飛び始めたコマに跳ぶ動きを始め着いて空中に残っても跳び直さない() {
    let mut pr = prev();
    let mut p = player();
    p.state = PlayerState::Wire; p.air = true; p.wire.dur = 0.4;
    assert!(matches!(hero_commands(&p, &mut pr)[..], [HeroCommand::Jump { air_seconds }] if air_seconds > 0.4));
    assert!(hero_commands(&p, &mut pr).is_empty());
    p.state = PlayerState::Free; p.vy = 7.0;
    assert!(hero_commands(&p, &mut pr).is_empty());
}

#[test]
fn 雲の上では立って構える() {
    let mut p = player();
    p.state = PlayerState::Ride; p.air = true; p.y = 4.0; p.speed = 12.0;
    let m = hero_move(&p, 1.0);
    assert!(m.grounded && m.speed == 0.0 && m.armed > 0.5);
}
