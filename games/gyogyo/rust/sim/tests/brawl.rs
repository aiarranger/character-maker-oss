// ボーガン・ハンマー・「？」の箱・脚質（docs/spec.md「マークと武器」「脚質」）。
// 防ぐ故障: ボーガンの矢が 1 周しない・後ろの人に当たらない・撃った本人に当たる／ハンマーで飛ばない・戻ってこない／
// 「？」の箱が順位に関係なく同じ物を出す／8 人が固まったまま走る
use sim::ai::Difficulty;
use sim::items::{Pickup, PickupKind, Weapon};
use sim::race::{Input, Race, RaceEvent, RaceOptions, TUNE};
use sim::track::courses;

const DT: f64 = 1.0 / 120.0;

fn race(course: usize, seed: u32, player: Option<usize>) -> Race {
    let defs = courses();
    Race::new(RaceOptions { course: &defs[course], seed, difficulty: Difficulty::Normal, player_gate: player, auto_jump: false, wide_jump: false, levels: None, humans: vec![] })
}

fn quiet() -> Race {
    let mut r = race(0, 7, Some(0));
    for i in 1..8 { r.clear_brain(i); }
    r.t = 0.0;
    r.started = true;
    r.pickups.clear();
    r.obstacle_list.clear();
    for i in 1..8 { put(&mut r, i, 20.0 + i as f64 * 30.0, 11.0); }
    r
}

fn put(r: &mut Race, i: usize, dist: f64, d: f64) {
    let s = r.track.s_at(dist);
    let x = &mut r.runners[i];
    x.dist = dist;
    x.s = s;
    x.d = d;
    x.speed = TUNE.base;
}

fn run(r: &mut Race, sec: f64, input: Input) -> Vec<RaceEvent> {
    let mut ev = Vec::new();
    for _ in 0..(sec / DT).round() as usize {
        r.step(DT, input);
        ev.append(&mut r.events);
    }
    ev
}

fn arm(r: &mut Race, i: usize, w: Weapon) {
    let x = &mut r.runners[i];
    x.weapon = Some(w);
    x.weapon_t = 12.0;
    x.weapon_uses = w.uses();
}

const TAP: Input = Input { x: 0.0, dash: false, jump: false, attack: true };

fn hits_on(ev: &[RaceEvent], j: usize) -> usize { ev.iter().filter(|e| matches!(e, RaceEvent::Hit { i, .. } if *i == j)).count() }

#[test]
fn crossbow_bolt_goes_a_full_lap_hitting_everyone_it_meets_and_ends_just_ahead_of_the_shooter() {
    let mut r = quiet();
    put(&mut r, 0, 300.0, 3.0);
    // 前に 1 人、後ろに 1 人（矢が 1 周して後ろから追いつく）。どちらも別の進路
    put(&mut r, 1, 340.0, 8.0);
    put(&mut r, 2, 260.0, 1.5);
    for i in 3..8 { r.runners[i].finished = true; }
    arm(&mut r, 0, Weapon::Crossbow);
    let mut ev = run(&mut r, 0.05, TAP);
    ev.append(&mut run(&mut r, 0.05, Input::NONE));
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Swing { i: 0, weapon: Weapon::Crossbow, .. })));
    assert_eq!(r.shots.len(), 1, "矢が 1 本飛ぶ");
    ev.append(&mut run(&mut r, 20.0, Input::NONE));
    assert_eq!(hits_on(&ev, 1), 1, "前の走者に当たる");
    assert_eq!(hits_on(&ev, 2), 1, "1 周して後ろの走者にも当たる");
    assert_eq!(hits_on(&ev, 0), 0, "撃った本人には当たらない");
    assert!(r.shots.is_empty(), "1 周したら消える");
    let end = ev.iter().find_map(|e| if let RaceEvent::BoltDone { dist, .. } = e { Some(*dist) } else { None }).expect("矢が戻った知らせ");
    let me = r.runners[0].dist;
    assert!(end > me - r.track.lap && end - r.track.lap < me + 12.0, "撃った本人のすぐ前で消える: 矢 {end:.1} / 本人 {me:.1}");
}

#[test]
fn a_jump_dodges_the_bolt() {
    let mut r = quiet();
    put(&mut r, 0, 300.0, 3.0);
    put(&mut r, 1, 312.0, 3.0);
    for i in 2..8 { r.runners[i].finished = true; }
    arm(&mut r, 0, Weapon::Crossbow);
    run(&mut r, 0.05, TAP);
    run(&mut r, 0.05, Input::NONE);
    // 矢が届く少し前に跳んでいる
    r.runners[1].air = true;
    r.runners[1].y = 1.0;
    r.runners[1].vy = 5.0;
    let ev = run(&mut r, 0.6, Input::NONE);
    assert_eq!(hits_on(&ev, 1), 0, "跳んでいればよける");
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Dodge { i: 1 })));
}

#[test]
fn hammer_launches_the_target_straight_up_and_it_lands_about_two_and_a_half_seconds_later() {
    let mut r = quiet();
    put(&mut r, 0, 300.0, 4.0);
    put(&mut r, 1, 302.0, 4.0);
    for i in 2..8 { r.runners[i].finished = true; }
    arm(&mut r, 0, Weapon::Hammer);
    let mut ev = run(&mut r, 0.05, TAP);
    ev.append(&mut run(&mut r, 0.05, Input::NONE));
    assert_eq!(hits_on(&ev, 1), 1, "ハンマーが当たる");
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Launch { i: 1, .. })), "飛ばされた知らせ");
    let d0 = r.runners[1].dist;
    run(&mut r, 1.2, Input::NONE);
    let x = &r.runners[1];
    assert!(x.y > 6.0, "高く飛ぶ: {}", x.y);
    assert!(x.launch > 0.0);
    let ev = run(&mut r, 1.4, Input::NONE);
    let x = &r.runners[1];
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Land { i: 1, .. })), "落ちてくる");
    assert_eq!(x.launch, 0.0);
    assert!(x.dist - d0 < 8.0, "飛んでいる間はその場からほとんど進まない: {:.1}m", x.dist - d0);
    // そのあとは走り出す
    let before = r.runners[1].dist;
    run(&mut r, 2.0, Input::NONE);
    assert!(r.runners[1].dist - before > 15.0, "落ちたあとまた走る");
}

#[test]
fn a_mystery_box_gives_strong_items_to_the_back_and_light_ones_to_the_front() {
    let mut back = std::collections::HashMap::new();
    let mut front = std::collections::HashMap::new();
    for seed in 0..300u32 {
        for (rank, out) in [(8, &mut back), (1, &mut front)] {
            let mut r = race(0, seed, Some(0));
            for i in 1..8 { r.clear_brain(i); }
            r.t = 0.0;
            r.started = true;
            r.pickups.clear();
            r.obstacle_list.clear();
            for i in 1..8 { put(&mut r, i, if rank == 8 { 400.0 + i as f64 } else { 100.0 - i as f64 * 3.0 }, 11.0); }
            put(&mut r, 0, 200.0, 5.0);
            r.pickups.push(Pickup { dist: 205.0, d: 5.0, kind: PickupKind::Mystery, taken: false });
            let ev = run(&mut r, 1.0, Input::NONE);
            let got = ev.iter().find_map(|e| if let RaceEvent::Pick { i: 0, kind } = e { Some(*kind) } else { None }).expect("箱を取る");
            let key = match got { PickupKind::Arm(w) => w.id().to_string(), PickupKind::Jet => "jet".into(), k => k.id().to_string() };
            *out.entry(key).or_insert(0) += 1;
        }
    }
    let strong = |m: &std::collections::HashMap<String, i32>| ["crossbow", "hammer", "jet"].iter().map(|k| m.get(*k).copied().unwrap_or(0)).sum::<i32>();
    assert!(strong(&back) >= 210, "最下位はボーガン・ハンマー・噴射が 7 割以上: {back:?}");
    assert_eq!(m_get(&front, "crossbow"), 0, "先頭にボーガンは出ない: {front:?}");
    assert!(strong(&front) <= 30, "先頭は強い物が 1 割以下: {front:?}");
}
fn m_get(m: &std::collections::HashMap<String, i32>, k: &str) -> i32 { m.get(k).copied().unwrap_or(0) }

#[test]
fn a_jet_gives_a_big_short_burst() {
    let mut r = quiet();
    put(&mut r, 0, 200.0, 5.0);
    r.pickups.push(Pickup { dist: 202.0, d: 5.0, kind: PickupKind::Jet, taken: false });
    run(&mut r, 0.4, Input::NONE);
    assert!(r.runners[0].jet > 0.0);
    run(&mut r, 0.6, Input::NONE);
    assert!(r.runners[0].speed > TUNE.base * 1.3, "噴射で大きく速くなる: {}", r.runners[0].speed);
}

/// 8 人の先頭と最後尾の差（レースの半分のころ）
fn spread(seed: u32) -> f64 {
    let mut r = race(0, seed, None);
    let half = r.track.race_len * 0.5;
    while r.runners.iter().map(|x| x.dist).fold(0.0, f64::max) < half { r.step(DT, Input::NONE); }
    let ds: Vec<f64> = r.runners.iter().map(|x| x.dist).collect();
    ds.iter().cloned().fold(0.0, f64::max) - ds.iter().cloned().fold(f64::MAX, f64::min)
}

#[test]
fn running_styles_stretch_the_field() {
    let gaps: Vec<f64> = (1..=6).map(spread).collect();
    let avg = gaps.iter().sum::<f64>() / gaps.len() as f64;
    assert!(avg > 40.0, "レースの半分で、先頭と最後尾が平均 40m 以上離れる: {gaps:?}");
}

#[test]
#[ignore]
fn report_spread() {
    let gaps: Vec<f64> = (1..=10).map(spread).collect();
    println!("spread at half: {gaps:?}");
}
