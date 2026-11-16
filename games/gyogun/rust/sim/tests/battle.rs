// 戦いの計算の受入条件（src/sim/battle.test.ts と同じ内容）: 通常攻撃の数で強攻撃が変わる・隊長を倒すと砦が落ちる・砦が全部落ちると大将が主役の前へ出る・乱舞
use sim::battle::*;
use sim::math::{self, cos, hypot, sin, PI};
use std::collections::HashSet;

const DT: f64 = 1.0 / 60.0;
fn press(f: impl FnOnce(&mut Input)) -> Input { let mut i = NO_INPUT; f(&mut i); i }
fn attack() -> Input { press(|i| i.attack = true) }
fn heavy() -> Input { press(|i| i.heavy = true) }
fn dodge() -> Input { press(|i| i.dodge = true) }
fn ranbu() -> Input { press(|i| i.ranbu = true) }
fn run_i(b: &mut Battle, sec: f64, input: Input) { for _ in 0..math::round(sec * 60.0) as i64 { b.step(DT, input); } }
fn run(b: &mut Battle, sec: f64) { run_i(b, sec, NO_INPUT); }
fn move_id(b: &Battle) -> Option<&'static str> { b.player.r#move.map(|m| m.id) }
fn has_ev(b: &Battle, f: impl Fn(&BattleEvent) -> bool) -> bool { b.events.iter().any(f) }
fn count_ev(b: &Battle, f: impl Fn(&BattleEvent) -> bool) -> usize { b.events.iter().filter(|e| f(e)).count() }

#[test]
fn 通常攻撃を3回つないでから強攻撃を出すと烈波になる() {
    let mut b = create_battle(3, true);
    b.debug_teleport(0.0, 80.0);
    b.step(DT, attack());
    assert_eq!(move_id(&b), Some("n1"));
    let mut seen: Vec<&str> = vec![];
    for _ in 0..200 {
        if seen.len() >= 4 { break; }
        let n = seen.len();
        b.step(DT, if n < 2 { attack() } else { heavy() });
        if let Some(id) = move_id(&b) { if seen.last() != Some(&id) && id != "n1" { seen.push(id); } }
    }
    assert_eq!(&seen[..3], &["n2", "n3", "c3"]);
}

#[test]
fn 隊長を倒すとその砦が味方になりcaptureが出る() {
    let mut b = create_battle(4, true);
    b.debug_capture(0);
    assert_eq!(b.holds[0].owner, Owner::Ally);
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::Capture { hold: 0, .. })));
}

#[test]
fn 砦を4つ落として本陣へ近づくと大将が主役の手前へ跳んでくる() {
    let mut b = create_battle(5, true);
    for i in 0..4 { b.debug_capture(i); }
    assert!(b.gate_open);
    b.debug_teleport(GATE.x, GATE.z + 26.0);
    run(&mut b, 0.2);
    assert_eq!(b.phase, Phase::Boss);
    run(&mut b, 2.0);
    let d = hypot(b.boss.x - b.player.x, b.boss.z - b.player.z);
    assert!(d < 16.0, "大将との距離 {d:.1}m");
}

#[test]
fn ゲージが満ちたら乱舞が出てまわりの敵をまとめて倒す() {
    let mut b = create_battle(6, true);
    b.debug_teleport(-46.0, 30.0); // 苺の砦（守りが多い）
    run(&mut b, 0.5);
    b.debug_ranbu();
    let before = b.player.kos;
    b.step(DT, ranbu());
    assert_eq!(move_id(&b), Some("ranbu"));
    run(&mut b, 4.0);
    assert!(b.player.kos - before >= 15, "乱舞で倒した数 {}", b.player.kos - before);
}

// 群衆ドミノ: 飛んだ敵が弾になり、ぶつかった敵も飛ぶ
fn chain_victims(b: &Battle) -> HashSet<i64> { b.events.iter().filter_map(|e| if let BattleEvent::Chain { victim, .. } = e { Some(*victim) } else { None }).collect() }

#[test]
fn 飛んだ雑兵が並んだ雑兵にぶつかると次々に飛んで連鎖する() {
    let mut b = create_battle(8, true);
    b.debug_teleport(-20.0, 70.0);
    let ammo = b.debug_spawn(EnemyKind::Strawberry, -20.0, 67.0);
    let row: Vec<usize> = (0..6).map(|i| b.debug_spawn(EnemyKind::Mikan, -20.0, 63.5 - i as f64 * 1.3)).collect();
    b.debug_fling(ammo, 0.0, -15.0, 2.0);
    run(&mut b, 1.0);
    let v = chain_victims(&b);
    let hit = row.iter().filter(|&&e| v.contains(&b.enemies[e].id)).count();
    assert!(hit >= 4, "連鎖で飛んだ数 {hit} / 6");
}

#[test]
fn 重騎を弾にすると雑兵の弾より大きく連鎖する() {
    let count = |kind| {
        let mut b = create_battle(9, true);
        b.debug_teleport(-20.0, 72.0);
        let ammo = b.debug_spawn(kind, -20.0, 68.0);
        for i in 0..7 { for j in 0..5 { b.debug_spawn(EnemyKind::Mikan, -22.4 + j as f64 * 1.2, 65.0 - i as f64 * 1.2); } }
        b.debug_fling(ammo, 0.0, -15.0, 2.0);
        run(&mut b, 1.2);
        chain_victims(&b).len()
    };
    let (g, p) = (count(EnemyKind::Strawberry), count(EnemyKind::Pudding));
    assert!(p as f64 > g as f64 * 1.5, "重騎 {p} / 雑兵 {g}");
}

/// 重騎を密集へ投げ込む（大きな連鎖を作る）
fn big_chain(seed: u32) -> Battle {
    let mut b = create_battle(seed, true);
    b.debug_teleport(-20.0, 72.0);
    let ammo = b.debug_spawn(EnemyKind::Pudding, -20.0, 68.0);
    for i in 0..9 { for j in 0..7 { b.debug_spawn(EnemyKind::Mikan, -23.6 + j as f64 * 1.2, 65.0 - i as f64 * 1.2); } }
    b.debug_fling(ammo, 0.0, -16.0, 2.0);
    b
}

#[test]
fn 連鎖が10体に届いた瞬間ほんの一瞬スローになる() {
    let mut b = big_chain(15);
    let mut slow_at_10 = false;
    for _ in 0..90 {
        b.step(DT, NO_INPUT);
        if has_ev(&b, |e| matches!(e, BattleEvent::Chain { n: 10, .. })) { slow_at_10 = b.time_scale() < 1.0; break; }
    }
    assert!(slow_at_10, "10体目でスロー");
}

#[test]
fn 連鎖が終わると撃破数と網の中心が出て10体を超えた連鎖は得点に足される() {
    let mut b = big_chain(16);
    run(&mut b, 2.5);
    let ends: Vec<(i64, i64, f64, f64, f64)> = b.events.iter().filter_map(|e| if let BattleEvent::ChainEnd { n, kills, bonus, x, z, .. } = e { Some((*n, *kills, *bonus, *x, *z)) } else { None }).collect();
    let big = ends.iter().find(|e| e.0 >= 10).expect("大きな連鎖");
    assert!(big.1 >= 1 && big.2 > 0.0 && big.3.is_finite() && big.4.is_finite());
    assert_eq!(b.result().chain_bonus, ends.iter().map(|e| e.2).sum::<f64>());
}

#[test]
fn 連鎖のあとに乱舞を出すと締めで金の印をまとめて爆発させる() {
    let mut b = big_chain(17);
    run(&mut b, 0.6);
    b.debug_ranbu();
    b.step(DT, ranbu());
    assert_eq!(move_id(&b), Some("ranbu"));
    run(&mut b, 5.0); // 乱舞（約3.8秒）＋締めの止め絵のあと、第二波で起爆
    let n = count_ev(&b, |e| matches!(e, BattleEvent::Detonate { .. }));
    assert!((3..=20).contains(&n), "起爆した数 {n}");
}

#[test]
fn 連鎖のない乱舞では起爆は起きない() {
    let mut b = create_battle(18, true);
    b.debug_teleport(-20.0, 72.0);
    b.debug_ranbu();
    b.step(DT, ranbu());
    run(&mut b, 4.0);
    assert_eq!(count_ev(&b, |e| matches!(e, BattleEvent::Detonate { .. })), 0);
}

/// 隊長のまわりに雑兵を置き、弾を投げ込む
fn throw_at_captain(seed: u32, ammo_kind: EnemyKind) -> (Battle, usize, Vec<usize>) {
    let mut b = create_battle(seed, true);
    b.debug_teleport(-20.0, 76.0);
    let cap = b.debug_spawn(EnemyKind::Shortcake, -20.0, 64.0);
    let grunts: Vec<usize> = (0..8).map(|i| b.debug_spawn(EnemyKind::Mikan, -20.0 + sin(i as f64 * 0.785) * 3.2, 64.0 + cos(i as f64 * 0.785) * 3.2)).collect();
    let ammo = b.debug_spawn(ammo_kind, -20.0, 70.0);
    b.debug_fling(ammo, 0.0, -16.0, 7.0); // 強い吹き飛ばしと同じくらいの高さ
    run(&mut b, 0.8);
    (b, cap, grunts)
}

#[test]
fn 重騎の弾が隊長に当たると隊長が倒れまわりの雑兵が吹き飛ぶ() {
    let (b, cap, grunts) = throw_at_captain(19, EnemyKind::Pudding);
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::CaptainBreak { .. })), "隊長崩し");
    assert_eq!(b.enemies[cap].state, EnemyState::Down);
    let v = chain_victims(&b);
    let flown = grunts.iter().filter(|&&g| v.contains(&b.enemies[g].id)).count();
    assert!(flown >= 4, "吹き飛んだ雑兵 {flown} / 8");
}

#[test]
fn 雑兵の弾では隊長は崩れない() {
    let (b, _, _) = throw_at_captain(20, EnemyKind::Strawberry);
    assert!(!has_ev(&b, |e| matches!(e, BattleEvent::CaptainBreak { .. })));
}

/// 密集の前で技を出す（keys: 押す順）
fn strike_crowd(seed: u32, keys: &[char]) -> usize {
    let mut b = create_battle(seed, true);
    b.debug_teleport(-20.0, 72.0);
    for i in 0..48 { b.debug_spawn(EnemyKind::Strawberry, -20.0 + ((i % 8) as f64 - 3.5) * 1.05, 69.5 - (i as f64 / 8.0).floor() * 1.05); }
    let mut k = 0;
    for _ in 0..60 * 3 {
        let before = b.player.move_key;
        b.step(DT, if k < keys.len() { if keys[k] == 'n' { attack() } else { heavy() } } else { NO_INPUT });
        if b.player.move_key != before { k += 1; }
    }
    count_ev(&b, |e| matches!(e, BattleEvent::Chain { heavy: true, .. }))
}

#[test]
fn 通常攻撃で集めてから強攻撃を当てると飛んだ敵が重騎なみの弾になる() { assert!(strike_crowd(21, &['n', 'n', 'c']) > 0, "集めて旋風斬"); }

#[test]
fn いきなりの迅雷突きでは重騎なみの弾にならない() { assert_eq!(strike_crowd(22, &['c']), 0); }

/// 正面から突進してくる重騎を、回避ですり抜ける
fn dodge_through_brute(seed: u32) -> (Battle, usize) {
    let mut b = create_battle(seed, true);
    b.debug_teleport(-20.0, 72.0); // 主役は -Z を向いている
    let br = b.debug_spawn(EnemyKind::Pudding, -20.0, 66.0);
    { let e = &mut b.enemies[br]; e.awake = true; e.state = EnemyState::Strike; e.st = 0.0; e.face = 0.0; e.vx = 0.0; e.vz = 22.0; }
    b.step(DT, dodge());
    run(&mut b, 0.45);
    (b, br)
}

#[test]
fn 重騎の突進を回避ですり抜けるとその重騎を担ぐ() {
    let (b, br) = dodge_through_brute(23);
    assert_eq!(b.player.carry, b.enemies[br].id);
    assert_eq!(b.enemies[br].state, EnemyState::Carried);
}

#[test]
fn 担いだ重騎を投げると群れが重騎なみに連鎖する() {
    let (mut b, br) = dodge_through_brute(24);
    let (px, pz) = (b.player.x, b.player.z);
    for i in 0..40 { b.debug_spawn(EnemyKind::Strawberry, px + ((i % 8) as f64 - 3.5) * 1.05, pz - 4.0 - (i as f64 / 8.0).floor() * 1.05); }
    b.player.face = PI;
    b.step(DT, attack());
    run(&mut b, 0.5);
    assert_eq!(b.player.carry, 0);
    assert_ne!(b.enemies[br].state, EnemyState::Carried);
    run(&mut b, 1.2);
    let heavy = count_ev(&b, |e| matches!(e, BattleEvent::Chain { heavy: true, .. }));
    assert!(heavy >= 5, "重騎の弾の連鎖 {heavy}");
}

#[test]
fn 担いだまま3秒たつと自動で投げる() {
    let (mut b, br) = dodge_through_brute(25);
    assert_eq!(b.player.carry, b.enemies[br].id);
    run(&mut b, 3.6);
    assert_eq!(b.player.carry, 0);
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::Hurl { .. })));
}

#[test]
fn 投げは大車輪で振り回してから放ち振り回しでまわりの敵も押し払う() {
    let (mut b, br) = dodge_through_brute(26);
    let (px, pz) = (b.player.x, b.player.z);
    let ring: Vec<usize> = (0..6).map(|i| b.debug_spawn(EnemyKind::Strawberry, px + sin(i as f64 * 1.05) * 2.4, pz + cos(i as f64 * 1.05) * 2.4)).collect();
    b.step(DT, attack());
    assert_eq!(move_id(&b), Some("hurl"));
    assert_eq!(b.player.carry, b.enemies[br].id, "振り回している間はまだ放さない");
    run(&mut b, 0.6);
    assert_eq!(b.player.carry, 0);
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::Hurl { .. })));
    let hit = ring.iter().filter(|&&e| b.enemies[e].hp < b.enemies[e].max_hp).count();
    assert!(hit >= 4, "振り回しで当たった数 {hit}");
}

/// 何もない所へ投げて、着地で爆ぜた場所を返す。around を渡すと、投げた直後にその場所のまわりへ敵を置く
fn throw_to_land(seed: u32, around: Option<(f64, f64)>) -> (Battle, usize, Option<(f64, f64)>, Vec<usize>) {
    let (mut b, br) = dodge_through_brute(seed);
    b.player.face = PI;
    b.step(DT, attack());
    for _ in 0..60 { if has_ev(&b, |e| matches!(e, BattleEvent::Hurl { .. })) { break; } b.step(DT, NO_INPUT); }
    let placed: Vec<usize> = match around { Some((x, z)) => (0..8).map(|i| b.debug_spawn(EnemyKind::Strawberry, x + sin(i as f64 * 0.785) * 2.5, z + cos(i as f64 * 0.785) * 2.5)).collect(), None => vec![] };
    for _ in 0..180 { if has_ev(&b, |e| matches!(e, BattleEvent::HurlBoom { .. })) { break; } b.step(DT, NO_INPUT); }
    let boom = b.events.iter().find_map(|e| if let BattleEvent::HurlBoom { x, z, .. } = e { Some((*x, *z)) } else { None });
    (b, br, boom, placed)
}

#[test]
fn 投げた重騎は着地で爆ぜまわりの敵をまとめて吹き飛ばす() {
    let (b, br, boom, _) = throw_to_land(27, None);
    let boom = boom.expect("着地で爆ぜる");
    assert_eq!(b.enemies[br].state, EnemyState::Dead, "投げた重騎は爆ぜて倒れる");
    let (b, _, _, placed) = throw_to_land(27, Some(boom));
    let blown = placed.iter().filter(|&&e| b.enemies[e].state == EnemyState::Dead || b.enemies[e].state == EnemyState::Air).count();
    assert!(blown >= 6, "爆発で吹き飛んだ数 {blown}");
}

#[test]
fn 密集に六の太刀を当てると刀の届かない敵まで連鎖で飛ぶ() {
    let mut b = create_battle(10, true);
    b.debug_teleport(-20.0, 72.0);
    // 寄せても刀の外に敵が残る大きさの群れ
    for i in 0..80 { b.debug_spawn(EnemyKind::Strawberry, -20.0 + ((i % 10) as f64 - 4.5) * 1.1, 68.0 - (i as f64 / 10.0).floor() * 1.1); }
    for _ in 0..240 {
        b.step(DT, attack());
        if move_id(&b) == Some("n6") && b.player.move_t > 0.6 { break; }
    }
    run(&mut b, 0.8);
    // 六の太刀の届く範囲（半径 4.8m）より外で、連鎖に巻き込まれた敵
    let (px, pz) = (b.player.x, b.player.z);
    let far = count_ev(&b, |e| matches!(e, BattleEvent::Chain { x, z, .. } if hypot(x - px, z - pz) > 6.0));
    assert!(far >= 3, "刀の届かない所で連鎖した数 {far}");
}

#[test]
fn 跳びかかりの途中の隊長に天嵐を当て続けても浮いたままにならず地面へ落ちる() {
    let mut b = create_battle(12, true);
    b.debug_teleport(-20.0, 72.0);
    // 通常5回 → 天嵐（0.1秒ごとに当たる）
    for _ in 0..400 {
        if move_id(&b) == Some("c5") { break; }
        let inp = if b.player.chain >= 5 { heavy() } else { attack() };
        b.step(DT, inp);
    }
    assert_eq!(move_id(&b), Some("c5"));
    let (px, pz, pf) = (b.player.x, b.player.z, b.player.face);
    let c = b.debug_spawn(EnemyKind::Shortcake, px + sin(pf) * 2.0, pz + cos(pf) * 2.0);
    // 跳びかかりの構えで高く上がった所
    { let e = &mut b.enemies[c]; e.y = 4.0; e.vy = 0.0; e.state = EnemyState::Air; }
    let mut landed_at = -1.0;
    for i in 0..60 * 2 {
        b.step(DT, NO_INPUT);
        if b.enemies[c].y <= 0.01 && landed_at < 0.0 { landed_at = i as f64 / 60.0; }
    }
    assert!((0.0..1.2).contains(&landed_at), "着地までの秒 {landed_at}");
}

#[test]
fn 昇り龍で跳ね上がった途中で回避しても主役は地面へ戻る() {
    let mut b = create_battle(13, true);
    b.debug_teleport(-20.0, 72.0);
    for _ in 0..200 {
        if move_id(&b) == Some("c1") { break; }
        let inp = if b.player.chain >= 1 { heavy() } else { attack() };
        b.step(DT, inp);
    }
    assert_eq!(move_id(&b), Some("c1"));
    while b.player.move_t < 0.3 { b.step(DT, NO_INPUT); }
    assert!(b.player.y > 0.5, "跳ね上がっている");
    b.step(DT, dodge());
    run(&mut b, 1.2);
    assert_eq!(b.player.y, 0.0, "主役の高さ {:.2}", b.player.y);
}

#[test]
fn 昇り龍から通常攻撃につないでも主役は浮いたままにならない() {
    let mut b = create_battle(14, true);
    b.debug_teleport(-20.0, 72.0);
    for _ in 0..200 {
        if move_id(&b) == Some("c1") { break; }
        let inp = if b.player.chain >= 1 { heavy() } else { attack() };
        b.step(DT, inp);
    }
    // 昇り龍の途中から、通常攻撃を押し続ける
    let mut max_late: f64 = 0.0;
    for i in 0..60 * 2 {
        b.step(DT, attack());
        if i > 50 { max_late = max_late.max(b.player.y); }
    }
    assert!(max_late < 0.05, "つないだあとの高さ {max_late:.2}");
}

#[test]
fn 大きく吹き飛ばされたあと着地してから地上の技が出る() {
    let mut b = create_battle(7, false);
    b.debug_teleport(0.0, 80.0);
    // 重騎の突進に当たった状態を作る
    { let p = &mut b.player; p.state = PlayerState::Hurt; p.st = 0.0; p.hurt_t = 0.75; p.air = true; p.vy = 6.0; }
    run(&mut b, 1.2);
    assert!(!b.player.air);
    b.step(DT, attack());
    assert_eq!(move_id(&b), Some("n1"));
}

// ---- 士気崩壊 ----
/// まだ落ちていない、h からいちばん近い砦
fn nearest_hold(b: &Battle, i: usize) -> Stronghold {
    let h = b.holds[i];
    let mut v: Vec<Stronghold> = b.holds.iter().copied().filter(|o| o.owner == Owner::Enemy && o.id != i as i32).collect();
    v.sort_by(|a, c| hypot(a.x - h.x, a.z - h.z).partial_cmp(&hypot(c.x - h.x, c.z - h.z)).unwrap());
    v[0]
}
fn defenders(b: &Battle, i: i32) -> Vec<usize> { (0..b.enemies.len()).filter(|&k| { let e = &b.enemies[k]; e.alive && e.state != EnemyState::Dead && e.home == i && e.role != Role::Captain }).collect() }

#[test]
fn 砦を落とすと守りは消えずに一番近い砦へ逃げ逃げている間は攻撃してこない() {
    let mut b = create_battle(40, true);
    let to = nearest_hold(&b, 0);
    let guard = defenders(&b, 0);
    assert!(guard.len() >= 10);
    b.debug_capture(0);
    assert!(guard.iter().all(|&e| b.enemies[e].alive && b.enemies[e].state != EnemyState::Dead), "守りは崩れない");
    assert!(guard.iter().all(|&e| b.enemies[e].flee == to.id), "逃げ先 {}", to.name);
    let cap = b.events.iter().find_map(|e| if let BattleEvent::Capture { to, fled, .. } = e { Some((*to, *fled)) } else { None }).unwrap();
    assert!(cap.0 == to.id && cap.1 as usize == guard.len());
    // 主役を群れの中へ置いても、逃げる敵は構えない
    let (gx, gz) = (b.enemies[guard[0]].x, b.enemies[guard[0]].z);
    b.debug_teleport(gx, gz);
    let mut attacked = false;
    for _ in 0..120 {
        b.step(DT, NO_INPUT);
        if guard.iter().any(|&e| b.enemies[e].state == EnemyState::Windup || b.enemies[e].token) { attacked = true; }
    }
    assert!(!attacked);
}

#[test]
fn 逃げた敵は逃げ先の砦に着くとその砦の守りになる() {
    let mut b = create_battle(41, true);
    let to = nearest_hold(&b, 0);
    let guard = defenders(&b, 0);
    b.debug_capture(0);
    b.debug_teleport(0.0, 80.0); // 主役は遠くに置く
    run(&mut b, 25.0);
    let arrived = guard.iter().filter(|&&k| { let e = &b.enemies[k]; e.alive && e.flee == -1 && e.home == to.id && hypot(e.x - to.x, e.z - to.z) < 12.0 }).count();
    assert!(arrived as f64 >= guard.len() as f64 * 0.8, "着いた数 {arrived} / {}", guard.len());
}

#[test]
fn 最後の砦を落とすと逃げ先がないので守りはその場で崩れる() {
    let mut b = create_battle(42, true);
    for i in 0..3 { b.debug_capture(i); }
    let guard = defenders(&b, 3);
    b.debug_capture(3);
    assert!(guard.iter().all(|&e| b.enemies[e].state == EnemyState::Dead));
}

// ---- 巨人と雑兵の大将戦 ----
/// 大将戦へ入り、体力を半分より下にして巨人を組ませる
fn giant_battle(seed: u32) -> Battle {
    let mut b = create_battle(seed, true);
    for i in 0..4 { b.debug_capture(i); }
    b.debug_teleport(GATE.x, GATE.z + 26.0);
    run(&mut b, 4.5);
    assert_eq!(b.phase, Phase::Boss);
    let (bx, bz) = (b.boss.x, b.boss.z);
    b.debug_teleport(bx, bz + 16.0); // 少し離れて見る
    b.boss.hp = b.boss.max_hp * 0.49;
    run(&mut b, 3.5);
    b
}
fn stacked_of(b: &Battle, part: GiantPart) -> Vec<usize> {
    b.giant.slots.iter().enumerate().filter_map(|(i, s)| match s { Some(e) if b.enemies[*e].state == EnemyState::Stacked && GIANT_SLOTS[i].part == part => Some(*e), _ => None }).collect()
}

#[test]
fn 大将の体力が半分を切ると雑兵が積み重なって巨人になり大将はその上に立つ() {
    let b = giant_battle(50);
    assert!(b.giant.on);
    let n = b.enemies.iter().filter(|e| e.alive && e.state == EnemyState::Stacked).count();
    assert!(n >= 45, "積み重なった数 {n}");
    assert!(b.boss_y > 3.0, "大将の高さ {:.1}", b.boss_y);
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::GiantForm { .. })));
}

#[test]
fn 巨人の脚に重騎の弾をぶつけると雑兵が外れ半分外れると巨人が崩れて大将が倒れる() {
    let mut b = giant_battle(51);
    let leg = stacked_of(&b, GiantPart::LegL);
    let n0 = leg.len();
    let base = *leg.iter().reduce(|a, e| if b.enemies[*e].y < b.enemies[*a].y { e } else { a }).unwrap();
    // 脚の根元へ、横から重騎を飛ばす（根元の高さで届く山なり。大将の技で浮くかどうかに頼らない）
    let (fx, fz) = (cos(b.boss.face), -sin(b.boss.face));
    let (bx0, bz0) = (b.enemies[base].x, b.enemies[base].z);
    let br = b.debug_spawn(EnemyKind::Pudding, bx0 - fx * 7.0, bz0 - fz * 7.0);
    b.debug_fling(br, fx * 22.0, fz * 22.0, 5.5);
    run(&mut b, 1.2);
    let left = stacked_of(&b, GiantPart::LegL).len();
    assert!(left as f64 <= n0 as f64 / 2.0 || !b.giant.on, "残った脚の雑兵 {left} / {n0}");
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::GiantFall { .. })));
    assert!(!b.giant.on);
    assert_eq!(b.boss.state, BossState::Down);
    assert!(b.boss_y < 0.5);
    let rained = b.enemies.iter().filter(|e| e.alive && e.state != EnemyState::Dead && e.state != EnemyState::Stacked).count();
    assert!(rained >= 30, "降ってきて生きている雑兵 {rained}");
}

#[test]
fn 脚の雑兵は雑兵の弾では外れない() {
    let mut b = giant_battle(54);
    let leg = stacked_of(&b, GiantPart::LegL);
    let base = *leg.iter().reduce(|a, e| if b.enemies[*e].y < b.enemies[*a].y { e } else { a }).unwrap();
    let (fx, fz) = (cos(b.boss.face), -sin(b.boss.face));
    let (bx0, bz0) = (b.enemies[base].x, b.enemies[base].z);
    let g = b.debug_spawn(EnemyKind::Mikan, bx0 - fx * 3.0, bz0 - fz * 3.0);
    b.debug_fling(g, fx * 18.0, fz * 18.0, 1.0);
    run(&mut b, 0.6);
    assert_eq!(stacked_of(&b, GiantPart::LegL).len(), leg.len());
}

#[test]
fn 脚の雑兵には斬りが効かない() {
    let mut b = giant_battle(52);
    let e = stacked_of(&b, GiantPart::LegR)[0];
    let hp = b.enemies[e].hp;
    b.debug_hit_enemy(e, 999.0);
    assert_eq!(b.enemies[e].state, EnemyState::Stacked);
    assert_eq!(b.enemies[e].hp, hp);
}

#[test]
fn 腕の雑兵が6体倒れると腕が壊れ残りの雑兵が降ってくる() {
    let mut b = giant_battle(53);
    let arm = stacked_of(&b, GiantPart::ArmL);
    assert_eq!(arm.len(), 9);
    for &e in &arm[..6] { b.debug_hit_enemy(e, 999.0); }
    run(&mut b, 0.1);
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::GiantArm { .. })));
    assert!(arm[6..].iter().all(|&e| b.enemies[e].alive && b.enemies[e].state != EnemyState::Stacked && b.enemies[e].state != EnemyState::Dead));
    assert!(b.giant.on, "腕が壊れても巨人は立っている");
}

// ---- 格 ----
#[test]
fn 敵を倒すと武勲が貯まり格が上がる() {
    let mut b = create_battle(60, true);
    b.debug_teleport(-20.0, 90.0);
    assert_eq!(b.player.rank, 0);
    for i in 0..RANK_AT[0] {
        let e = b.debug_spawn(EnemyKind::Mikan, -20.0 + (i % 20) as f64 * 1.5, 60.0 - (i as f64 / 20.0).floor() * 1.5);
        b.debug_hit_enemy(e, 999.0);
    }
    b.step(DT, NO_INPUT);
    assert_eq!(b.player.rank, 1);
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::RankUp { rank: 1 })));
}

#[test]
fn 砦を落とすと武勲が大きく入る() {
    let mut b = create_battle(61, true);
    b.debug_capture(0);
    b.step(DT, NO_INPUT);
    assert!(b.merit >= 80, "武勲 {}", b.merit);
}

/// 主役の正面 d m に雑兵を置いて一の太刀を振る。当たったか
fn reach_at(rank: usize, d: f64) -> bool {
    let mut b = create_battle(62, true);
    b.debug_teleport(-20.0, 72.0);
    if rank > 0 { b.debug_merit(RANK_AT[rank - 1]); }
    b.step(DT, NO_INPUT);
    let (px, pz, pf) = (b.player.x, b.player.z, b.player.face);
    let e = b.debug_spawn(EnemyKind::Creampuff, px + sin(pf) * d, pz + cos(pf) * d);
    b.step(DT, attack());
    run(&mut b, 0.4);
    b.enemies[e].hp < b.enemies[e].max_hp
}

#[test]
fn 格_壱で通常攻撃の当たる範囲が広がる() {
    assert!(!reach_at(0, 5.6), "初陣では届かない");
    assert!(reach_at(1, 5.6), "壱なら届く");
}

/// 迅雷突きで雑兵を一体飛ばし、当たった直後の速さ
fn fling_speed(rank: usize) -> f64 {
    let mut b = create_battle(63, true);
    b.debug_teleport(-20.0, 72.0);
    if rank > 0 { b.debug_merit(RANK_AT[rank - 1]); }
    b.step(DT, NO_INPUT);
    let (px, pz, pf) = (b.player.x, b.player.z, b.player.face);
    let e = b.debug_spawn(EnemyKind::Creampuff, px + sin(pf) * 3.0, pz + cos(pf) * 3.0);
    b.step(DT, heavy());
    for _ in 0..60 { if b.enemies[e].hp != b.enemies[e].max_hp { break; } b.step(DT, NO_INPUT); }
    hypot(b.enemies[e].vx, b.enemies[e].vz)
}

#[test]
fn 格_弐で技で飛ばした敵の勢いが1_25倍() {
    let (v0, v2) = (fling_speed(0), fling_speed(2));
    assert!((v2 / v0 - 1.25).abs() < 0.03, "初陣 {v0:.2} / 弐 {v2:.2}");
}

#[test]
fn 格_参で上がった瞬間に乱舞ゲージが50増える() {
    let mut b = create_battle(64, true);
    b.player.ranbu = 10.0;
    b.debug_merit(RANK_AT[2]);
    assert_eq!(b.player.rank, 3);
    assert!(b.player.ranbu >= 60.0);
}

// ---- 飴鏡 ----
/// 飴鏡の中心・向き（沿う向き u と、法線 n）
fn mirror(i: usize) -> (Mirror, f64, f64, f64, f64) {
    let m = MIRRORS[i];
    (m, sin(m.a), cos(m.a), cos(m.a), -sin(m.a))
}

#[test]
fn 飴鏡に飛ばした弾は跳ね返り勢いは85パーセントになる() {
    let mut b = create_battle(70, true);
    b.debug_teleport(0.0, 80.0);
    let (m, _, _, nx, nz) = mirror(0);
    let e = b.debug_spawn(EnemyKind::Mikan, m.x + nx * 4.0, m.z + nz * 4.0);
    b.debug_fling(e, -nx * 18.0, -nz * 18.0, 4.0);
    let mut before = 0.0;
    for _ in 0..60 {
        if has_ev(&b, |ev| matches!(ev, BattleEvent::Reflect { .. })) { break; }
        before = hypot(b.enemies[e].vx, b.enemies[e].vz);
        b.step(DT, NO_INPUT);
    }
    assert!(has_ev(&b, |ev| matches!(ev, BattleEvent::Reflect { .. })), "跳ね返る");
    assert!(b.enemies[e].vx * nx + b.enemies[e].vz * nz > 0.0, "鏡から離れる向き");
    let ratio = hypot(b.enemies[e].vx, b.enemies[e].vz) / before;
    assert!((ratio - 0.85).abs() < 0.06, "勢いの比 {ratio:.2}");
}

#[test]
fn 飴鏡で跳ね返った弾がまっすぐでは届かない群れを巻き込む() {
    let mut b = create_battle(71, true);
    b.debug_teleport(0.0, 80.0);
    let (m, ux, uz, nx, nz) = mirror(0);
    // 45度で鏡の中心へ。跳ね返った先（鏡の反対の斜め）に群れを置く
    let s2 = std::f64::consts::SQRT_2;
    let (in_x, in_z) = ((-nx + ux) / s2, (-nz + uz) / s2);
    let (out_x, out_z) = ((nx + ux) / s2, (nz + uz) / s2);
    let e = b.debug_spawn(EnemyKind::Pudding, m.x - in_x * 4.0, m.z - in_z * 4.0);
    let crowd: Vec<usize> = (0..12).map(|i| {
        let row = (i as f64 / 3.0).floor() * 1.1;
        let side = ((i % 3) as f64 - 1.0) * 0.9;
        b.debug_spawn(EnemyKind::Mikan, m.x + out_x * (4.0 + row) + side * -out_z, m.z + out_z * (4.0 + row) + side * out_x)
    }).collect();
    b.debug_fling(e, in_x * 20.0, in_z * 20.0, 8.0);
    run(&mut b, 1.5);
    let ids: HashSet<i64> = crowd.iter().map(|&c| b.enemies[c].id).collect();
    let hit = count_ev(&b, |ev| matches!(ev, BattleEvent::Chain { victim, .. } if ids.contains(victim)));
    assert!(hit >= 4, "跳ね返った先の群れで巻き込んだ数 {hit}");
}

#[test]
fn 主役は飴鏡を通り抜けられない() {
    let mut b = create_battle(72, true);
    let (m, _, _, nx, nz) = mirror(1);
    b.debug_teleport(m.x + nx * 2.5, m.z + nz * 2.5);
    run_i(&mut b, 1.2, press(|i| { i.mx = -nx; i.mz = -nz; }));
    let side = (b.player.x - m.x) * nx + (b.player.z - m.z) * nz;
    assert!(side > 0.3, "鏡からの距離 {side:.2}");
}

// ---- 武器の弾道 ----
fn ang(a: f64) -> f64 { math::atan2(sin(a), cos(a)) }
/// 正面の扇に雑兵を並べて迅雷突きを当て、直接飛んだ敵の向きを、当たった直後と 0.25 秒後で返す
fn strike_path(path: Path) -> (Vec<f64>, Vec<f64>, usize) {
    let mut b = create_battle(80, true);
    b.set_path(path);
    b.debug_teleport(-20.0, 72.0);
    let (px, pz, pf) = (b.player.x, b.player.z, b.player.face);
    let crowd: Vec<usize> = (0..9).map(|i| {
        let a = pf + (i as f64 - 4.0) * 0.16;
        let d = 2.6 + (i % 2) as f64 * 0.6;
        b.debug_spawn(EnemyKind::Creampuff, px + sin(a) * d, pz + cos(a) * d)
    }).collect();
    b.step(DT, heavy());
    let flown = |b: &Battle| -> Vec<usize> { crowd.iter().copied().filter(|&k| { let e = &b.enemies[k]; e.fling != 0 && e.gen == 0 && hypot(e.vx, e.vz) > 4.0 }).collect() };
    for _ in 0..40 { if flown(&b).len() >= 4 { break; } b.step(DT, NO_INPUT); }
    let sel = flown(&b);
    let at0: Vec<f64> = sel.iter().map(|&k| ang(math::atan2(b.enemies[k].vx, b.enemies[k].vz) - b.player.face)).collect();
    run(&mut b, 0.25);
    let at1: Vec<f64> = sel.iter().map(|&k| ang(math::atan2(b.enemies[k].vx, b.enemies[k].vz) - b.player.face)).collect();
    (at0, at1, sel.len())
}
fn spread_of(a: &[f64]) -> f64 { a.iter().fold(f64::NEG_INFINITY, |m, v| m.max(v.abs())) }

#[test]
fn 槍の直進で強攻撃で飛んだ敵はそろって正面へ飛ぶ() {
    let (none, st) = (strike_path(Path::None), strike_path(Path::Straight));
    assert!(st.2 >= 4);
    assert!(spread_of(&st.0) < 0.05, "直進の広がり {:.2}", spread_of(&st.0));
    assert!(spread_of(&none.0) > 0.08, "ふつうの広がり {:.2}", spread_of(&none.0));
}

#[test]
fn フライパンの拡散で強攻撃で飛んだ敵は扇に大きく広がる() {
    let (none, sp) = (strike_path(Path::None), strike_path(Path::Spread));
    assert!(spread_of(&sp.0) > spread_of(&none.0) * 1.6, "拡散 {:.2} / ふつう {:.2}", spread_of(&sp.0), spread_of(&none.0));
}

#[test]
fn 杖の曲進で強攻撃で飛んだ敵は飛んでいる間に曲がる() {
    let (none, cv) = (strike_path(Path::None), strike_path(Path::Curve));
    let turn = |r: &(Vec<f64>, Vec<f64>, usize)| r.0.iter().zip(&r.1).map(|(a, b)| ang(b - a)).fold(f64::INFINITY, f64::min);
    assert!(turn(&cv) > 0.3, "曲進の曲がり {:.2}", turn(&cv));
    assert!(turn(&none).abs() < 0.1, "ふつうの曲がり {:.2}", turn(&none));
}

// ---- 育つ武器（試作） ----
#[test]
fn 武勲が貯まると武器が育ち通常攻撃の届く範囲も広がる() {
    let mut b = create_battle(64, true);
    assert_eq!(b.weapon_grow(), 1.0);
    b.debug_merit(RANK_AT[0]);
    assert!((b.weapon_grow() - 1.5).abs() < 1e-9, "壱の大きさ {}", b.weapon_grow());
    b.debug_merit((RANK_AT[1] - RANK_AT[0]) / 2);
    assert!(b.weapon_grow() > 1.5 && b.weapon_grow() < 2.2, "壱と弐の間もなめらかに育つ {}", b.weapon_grow());
    b.debug_merit(RANK_AT[2]);
    assert_eq!(b.weapon_grow(), 3.0, "参で3倍");
    b.debug_merit(5000);
    assert_eq!(b.weapon_grow(), 3.0, "参で止まる");
    assert!(!reach_at(1, 7.5), "壱では届かない");
    assert!(reach_at(3, 7.5), "参なら届く");
}

// ---- 飴糸（試作） ----
fn wire() -> Input { press(|i| i.wire = true) }
fn strikes(b: &Battle) -> usize { count_ev(b, |e| matches!(e, BattleEvent::WireStrike { .. })) }

#[test]
fn 飴糸を隊長へ向けて出すと飛びついて斬る() {
    let mut b = create_battle(70, true);
    b.debug_teleport(0.0, 78.0);
    b.step(DT, NO_INPUT);
    let (ex, ez) = (b.player.x + 14.0, b.player.z);
    let cap = b.debug_spawn(EnemyKind::Shortcake, ex, ez);
    b.player.ame = 2;
    b.step(DT, press(|i| { i.mx = 1.0; i.wire = true; }));
    assert_eq!(b.player.state, PlayerState::Wire);
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::Wire { target: true, .. })));
    for _ in 0..90 { if strikes(&b) > 0 { break; } b.step(DT, NO_INPUT); }
    assert_eq!(strikes(&b), 1, "着いて斬る");
    assert!(hypot(b.player.x - b.enemies[cap].x, b.player.z - b.enemies[cap].z) < 4.0, "隊長のそばまで飛ぶ");
    assert!(b.enemies[cap].hp < b.enemies[cap].max_hp);
    assert!(b.player.state == PlayerState::Free && b.player.air, "斬ったあとは空中に残る");
    // 自分では離れない
    run(&mut b, 0.5);
    assert!(hypot(b.player.x - b.enemies[cap].x, b.player.z - b.enemies[cap].z) < 4.0, "斬ったあとも隊長のそばにいる");
}

#[test]
fn 飴糸は足軽にも重騎にも向かわず空へ飛ぶ() {
    for kind in [EnemyKind::Macaron, EnemyKind::Pudding] {
        let mut b = create_battle(78, true);
        b.debug_teleport(0.0, 78.0);
        b.step(DT, NO_INPUT);
        b.debug_spawn(kind, b.player.x + 10.0, b.player.z);
        b.player.ame = 1;
        // 続けている間でも向かわない
        b.player.wire_chain = 1; b.player.wire_chain_t = 1.0;
        b.step(DT, press(|i| { i.mx = 1.0; i.wire = true; }));
        assert_eq!(b.player.state, PlayerState::Wire);
        assert!(has_ev(&b, |e| matches!(e, BattleEvent::Wire { target: false, .. })), "{kind:?} へは向かわない");
    }
}

#[test]
fn 大きな敵がいない向きへ飴糸を出すと遠くまで飛んで空中に出る() {
    let mut b = create_battle(71, true);
    b.debug_teleport(0.0, 78.0);
    b.step(DT, NO_INPUT);
    let x0 = b.player.x;
    b.player.ame = 1;
    b.step(DT, press(|i| { i.mx = 1.0; i.wire = true; }));
    assert_eq!(b.player.state, PlayerState::Wire);
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::Wire { target: false, .. })));
    let (mut top, mut out_air) = (0.0f64, false);
    for _ in 0..60 {
        b.step(DT, NO_INPUT);
        top = top.max(b.player.y);
        if b.player.state == PlayerState::Free { out_air = b.player.air; break; }
    }
    assert!(b.player.x - x0 >= 12.0, "飛んだ距離 {:.1}m", b.player.x - x0);
    assert!(top > 2.5, "いちばん高い所 {top:.1}m");
    assert!(out_air, "空中に出る（空中の技につながる）");
    assert_eq!(strikes(&b), 0);
    // 空中の技につながる
    b.step(DT, heavy());
    assert_eq!(move_id(&b), Some("jc"));
}

#[test]
fn 巨人の上の大将を飴糸で3回斬ると巨人が崩れる() {
    let mut b = giant_battle(55);
    assert!(b.giant.on && b.boss_y > 3.0);
    let hp0 = b.boss.hp;
    b.player.ame = 5;
    b.player.face = math::atan2(b.boss.x - b.player.x, b.boss.z - b.player.z);
    for n in 1..=3 {
        for _ in 0..60 { if b.player.wire_cd <= 0.0 && b.player.state == PlayerState::Free { break; } b.step(DT, NO_INPUT); }
        if n > 1 {
            // 空へ逃げてから、同じ大将へ戻る
            b.step(DT, wire());
            for _ in 0..120 { if b.player.state != PlayerState::Wire { break; } b.step(DT, NO_INPUT); }
            for _ in 0..60 { if b.player.wire_cd <= 0.0 { break; } b.step(DT, NO_INPUT); }
        }
        b.step(DT, wire());
        assert_eq!(b.player.state, PlayerState::Wire, "{n} 回目");
        let mut top = 0.0f64;
        for _ in 0..120 { if strikes(&b) >= n { break; } b.step(DT, NO_INPUT); top = top.max(b.player.y); }
        assert_eq!(strikes(&b), n, "{n} 回目の糸斬り");
        if n == 1 {
            assert!(top > 4.0, "大将の高さまで飛ぶ {top:.1}m");
            assert!(hp0 - b.boss.hp >= 140.0, "巨人の上でも傷が減らない {:.0}", hp0 - b.boss.hp);
        }
        if n < 3 { assert!(b.giant.on, "{n} 回ではまだ立っている"); }
    }
    run(&mut b, 0.5); // 糸斬りのヒットストップが明けてから崩れる
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::GiantFall { .. })));
    assert!(!b.giant.on);
    assert_eq!(b.boss.state, BossState::Down);
}

// ---- 飴（飴糸の元手、試作） ----
#[test]
fn 砂糖でできた敵を8体倒すと飴が1個たまり果物では増えない() {
    let mut b = create_battle(73, true);
    b.debug_teleport(-20.0, 72.0);
    b.step(DT, NO_INPUT);
    assert_eq!(b.player.ame, 0, "出陣したときは 0 個");
    let (px, pz) = (b.player.x, b.player.z);
    for k in 0..12 { let i = b.debug_spawn(if k % 2 == 0 { EnemyKind::Strawberry } else { EnemyKind::Mikan }, px + 30.0 + k as f64, pz); b.debug_hit_enemy(i, 9999.0); }
    assert_eq!(b.player.ame, 0, "いちご・みかんでは増えない");
    for k in 0..7 { let i = b.debug_spawn(EnemyKind::Macaron, px + 30.0 + k as f64, pz + 4.0); b.debug_hit_enemy(i, 9999.0); }
    assert_eq!(b.player.ame, 0, "7 体ではまだ");
    let i = b.debug_spawn(EnemyKind::Creampuff, px + 30.0, pz + 8.0); b.debug_hit_enemy(i, 9999.0);
    assert_eq!(b.player.ame, 1, "8 体で 1 個");
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::Ame { count: 1 })));
    // 6 個で止まる
    for k in 0..80 { let i = b.debug_spawn(EnemyKind::Macaron, px + 30.0 + (k % 10) as f64, pz + 12.0); b.debug_hit_enemy(i, 9999.0); }
    assert_eq!(b.player.ame, AME_MAX);
}

#[test]
fn 飴が無いと飴糸は出ず1個あれば出て飴が減る() {
    let mut b = create_battle(74, true);
    b.debug_teleport(0.0, 78.0);
    b.step(DT, NO_INPUT);
    b.step(DT, wire());
    assert_ne!(b.player.state, PlayerState::Wire, "飴 0 個では出ない");
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::NoAme)));
    b.player.ame = 1;
    b.step(DT, wire());
    assert_eq!(b.player.state, PlayerState::Wire);
    assert_eq!(b.player.ame, 0, "1 個使う");
}

// ---- 猪鹿蝶（糸斬りの三連、試作） ----
fn chain_of(b: &Battle) -> Vec<u8> { b.events.iter().filter_map(|e| if let BattleEvent::WireStrike { chain, .. } = e { Some(*chain) } else { None }).collect() }
fn wait_wire(b: &mut Battle) { for _ in 0..60 { if b.player.wire_cd <= 0.0 && b.player.state == PlayerState::Free { break; } b.step(DT, NO_INPUT); } }
/// 飴糸を出し、斬るまで進める。返すのはその一撃の傷
fn strike_captain(b: &mut Battle, cap: usize, input: Input) -> f64 {
    wait_wire(b);
    let (n, hp) = (chain_of(b).len(), b.enemies[cap].hp);
    b.step(DT, input);
    assert_eq!(b.player.state, PlayerState::Wire);
    for _ in 0..120 { if chain_of(b).len() > n { break; } b.step(DT, NO_INPUT); }
    assert_eq!(chain_of(b).len(), n + 1, "糸斬りが当たる");
    hp - b.enemies[cap].hp
}
/// 斬った直後に飴糸を出して空へ逃げ、着くまで進める。返すのは（隊長との距離, 高さ）
fn escape(b: &mut Battle, cap: usize, input: Input) -> (f64, f64) {
    wait_wire(b);
    let n = chain_of(b).len();
    b.step(DT, input);
    assert_eq!(b.player.state, PlayerState::Wire, "逃げる飴糸が出る");
    for _ in 0..120 { if b.player.state != PlayerState::Wire { break; } b.step(DT, NO_INPUT); }
    assert_eq!(chain_of(b).len(), n, "逃げるときは斬らない");
    (hypot(b.player.x - b.enemies[cap].x, b.player.z - b.enemies[cap].z), b.player.y)
}
fn captain_ahead(seed: u32) -> (Battle, usize) {
    let mut b = create_battle(seed, true);
    b.debug_teleport(-20.0, 72.0);
    b.step(DT, NO_INPUT);
    let (px, pz, pf) = (b.player.x, b.player.z, b.player.face);
    let cap = b.debug_spawn(EnemyKind::Cakeking, px + sin(pf) * 14.0, pz + cos(pf) * 14.0);
    b.enemies[cap].poise = 1e9; // 倒れると傷が 1.3 倍になるので、倍率だけを見るために崩れないようにする
    (b, cap)
}

#[test]
fn 斬る逃げる斬る逃げる斬ると飴糸を5回出すと傷が上がり3回目で猪鹿蝶になる() {
    let (mut b, cap) = captain_ahead(75);
    b.player.ame = 5;
    let d1 = strike_captain(&mut b, cap, wire());
    let (far1, high1) = escape(&mut b, cap, wire());
    let d2 = strike_captain(&mut b, cap, wire());
    let (far2, high2) = escape(&mut b, cap, wire());
    let d3 = strike_captain(&mut b, cap, wire());
    assert_eq!(chain_of(&b), vec![1, 2, 3], "猪・鹿・蝶の順");
    assert!((d2 / d1 - 1.5).abs() < 0.01 && (d3 / d1 - 2.5).abs() < 0.01, "傷 {d1:.0} → {d2:.0} → {d3:.0}");
    assert!(far1 >= 8.0 && far2 >= 8.0, "逃げると離れる {far1:.1}m {far2:.1}m");
    assert!(high1 >= 4.0 && high2 >= 4.0, "空へ逃げる 高さ {high1:.1}m {high2:.1}m");
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::Inoshikacho { .. })));
    assert_eq!(b.player.ame, 0, "飴を 5 個使う");
}

#[test]
fn 空へ逃げるとき向きを押すとその向きへ飛び逃げたあとも向きで飛び回れる() {
    let (mut b, cap) = captain_ahead(77);
    b.player.ame = 4;
    strike_captain(&mut b, cap, wire());
    // 隊長から見て真横（+x）へ逃げる
    let x0 = b.player.x;
    escape(&mut b, cap, press(|i| { i.mx = 1.0; i.wire = true; }));
    assert!(b.player.x - x0 >= 8.0, "押した向きへ逃げる {:.1}m", b.player.x - x0);
    // 逃げたあと、隊長のいない向き（さらに +x）を押すと、斬り込まずにその向きへ飛ぶ
    let x1 = b.player.x;
    escape(&mut b, cap, press(|i| { i.mx = 1.0; i.wire = true; }));
    assert!(b.player.x - x1 >= 8.0, "飛び回れる {:.1}m", b.player.x - x1);
    // 向きを押さずに出すと、いま斬った隊長へ戻る（2 回目）
    strike_captain(&mut b, cap, wire());
    assert_eq!(chain_of(&b), vec![1, 2]);
}

#[test]
fn 糸斬りの間が2秒を超えると1回目に戻る() {
    let (mut b, cap) = captain_ahead(76);
    b.player.ame = 2;
    strike_captain(&mut b, cap, wire());
    run(&mut b, 2.3);
    // 2 秒を過ぎたあとの飴糸は逃げずに、ふだんどおり斬り込む
    b.player.face = math::atan2(b.enemies[cap].x - b.player.x, b.enemies[cap].z - b.player.z);
    strike_captain(&mut b, cap, wire());
    assert_eq!(chain_of(&b), vec![1, 1]);
    assert!(!has_ev(&b, |e| matches!(e, BattleEvent::Inoshikacho { .. })));
}

// ---- わたあめ雲（試作） ----
fn jump() -> Input { press(|i| i.jump = true) }
/// 空いた所で跳び、空中でもう一度跳んで雲に乗る
fn on_cloud(seed: u32, god: bool, x: f64, z: f64) -> Battle {
    let mut b = create_battle(seed, god);
    b.debug_teleport(x, z);
    b.step(DT, NO_INPUT);
    b.step(DT, jump());
    run(&mut b, 0.2);
    b.step(DT, jump());
    b
}

#[test]
fn 空中でもう一度跳ぶと雲に乗り走るより速く飛べる() {
    let mut b = on_cloud(80, true, 0.0, 78.0);
    assert_eq!(b.player.state, PlayerState::Ride);
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::Ride)));
    let x0 = b.player.x;
    run_i(&mut b, 2.0, press(|i| i.mx = 1.0));
    assert_eq!(b.player.state, PlayerState::Ride);
    assert!((b.player.y - RIDE_Y).abs() < 0.2, "雲の高さ {:.2}", b.player.y);
    assert!(b.player.x - x0 > 8.2 * 2.0 * 1.3, "2 秒で飛んだ距離 {:.1}m", b.player.x - x0);
}

#[test]
fn 雲は6秒で溶けて落ち3秒は呼べない() {
    let mut b = on_cloud(81, true, 0.0, 78.0);
    run(&mut b, RIDE_T - 0.2);
    assert_eq!(b.player.state, PlayerState::Ride);
    run(&mut b, 0.3);
    assert_eq!(b.player.state, PlayerState::Free);
    assert!(b.player.air && b.player.y > 2.0, "溶けたら落ち始める");
    assert!(has_ev(&b, |e| matches!(e, BattleEvent::RideEnd { melted: true })));
    // 落ちる途中でもう一度跳んでも、まだ呼べない
    b.step(DT, jump());
    assert_ne!(b.player.state, PlayerState::Ride);
    run(&mut b, 3.0);
    assert!(!b.player.air);
    b.step(DT, jump());
    run(&mut b, 0.2);
    b.step(DT, jump());
    assert_eq!(b.player.state, PlayerState::Ride, "3 秒たてばまた呼べる");
}

#[test]
fn 雲の上から強攻撃を出すと地面まで落ちて下の敵に当たる() {
    let mut b = on_cloud(82, true, -20.0, 72.0);
    run(&mut b, 1.0);
    assert!(b.player.y > 3.5);
    let e = b.debug_spawn(EnemyKind::Creampuff, b.player.x + 2.0, b.player.z);
    b.step(DT, heavy());
    assert_eq!(move_id(&b), Some("jc"));
    run(&mut b, 0.4);
    assert_eq!(b.player.y, 0.0, "地面に着く");
    assert!(b.enemies[e].hp < b.enemies[e].max_hp, "下の敵に当たる");
}

#[test]
fn 雲の上にいる間は足軽の斬りを受けない() {
    // 出陣の地点: 先鋒（足軽だけ 34 体）が押し寄せる。地上で立っていれば斬られる
    let mut ground = create_battle(83, false);
    run(&mut ground, 5.5);
    assert!(ground.player.dmg_taken > 0.0, "地上では斬られる");
    let mut b = on_cloud(83, false, START.x, START.z);
    run(&mut b, 5.5);
    assert_eq!(b.player.state, PlayerState::Ride);
    assert_eq!(b.player.dmg_taken, 0.0);
}
