// マークと武器（docs/spec.md「マークと武器」）。防ぐ故障: 取れない・二重に取れる・当たらない・当たり続けて動けない・よけられない・CPU が使わない。
use sim::ai::Difficulty;
use sim::items::{BASE_LEVELS, Levels, Pickup, PickupKind, Stat, Weapon};
use sim::race::{Input, Race, RaceEvent, RaceOptions, TUNE};
use sim::track::courses;

const DT: f64 = 1.0 / 120.0;

fn race(course: usize, seed: u32, player: Option<usize>, levels: Option<[Levels; 8]>) -> Race {
    let defs = courses();
    Race::new(RaceOptions { course: &defs[course], seed, difficulty: Difficulty::Normal, player_gate: player, auto_jump: false, wide_jump: false, levels, humans: vec![] })
}

/// 走り出したあとの、誰も判断しないレース（0 番がプレイヤー）。落ちている物は消してある
fn quiet() -> Race {
    let mut r = race(0, 7, Some(0), None);
    for i in 1..8 { r.clear_brain(i); }
    r.t = 0.0;
    r.started = true;
    r.pickups.clear();
    // ほかの走者は遠くへ（試す 2 人だけを近くに置く）
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

const ATTACK: Input = Input { x: 0.0, dash: false, jump: false, attack: true };

#[test]
fn pickups_are_placed_by_seed_and_keep_clear_of_start_goal_and_obstacles() {
    let (a, b) = (race(1, 42, None, None), race(1, 42, None, None));
    assert!(a.pickups.len() >= 20, "1 レースに 20 個以上: {}", a.pickups.len());
    assert_eq!(a.pickups.len(), b.pickups.len());
    for (p, q) in a.pickups.iter().zip(&b.pickups) { assert_eq!((p.dist, p.d, p.kind), (q.dist, q.d, q.kind)); }
    let c = race(1, 43, None, None);
    assert!(a.pickups.iter().zip(&c.pickups).any(|(p, q)| p.kind != q.kind || p.d != q.d), "種が違えば並びが違う");
    for p in &a.pickups {
        assert!(p.dist >= 60.0 && p.dist <= a.track.race_len - 120.0, "スタートとゴール前は空ける: {}", p.dist);
        assert!(p.d > 0.5 && p.d < a.track.width - 0.5);
        for e in &a.obstacle_list { assert!(p.dist < e.dist - 10.0 || p.dist > e.dist + e.ob.len + 8.0, "障害の前後に置かない: {} / {}", p.dist, e.dist); }
    }
    for w in a.pickups.windows(2) { assert!(w[0].dist <= w[1].dist, "距離の順に並ぶ"); }
}

#[test]
fn passing_over_a_mark_raises_that_level_once_and_only_for_the_first_runner() {
    let mut r = quiet();
    r.pickups.push(Pickup { dist: 110.0, d: 5.0, kind: PickupKind::Mark(Stat::Speed), taken: false });
    put(&mut r, 0, 100.0, 5.0);
    put(&mut r, 1, 90.0, 5.0);
    let ev = run(&mut r, 2.5, Input::NONE);
    assert_eq!(r.runners[0].lv[Stat::Speed.i()], BASE_LEVELS[0] + 1);
    assert_eq!(r.runners[1].lv[Stat::Speed.i()], BASE_LEVELS[0], "後から通った走者は取れない");
    assert!(r.pickups[0].taken);
    assert_eq!(ev.iter().filter(|e| matches!(e, RaceEvent::Pick { .. })).count(), 1);
    assert_eq!(r.runners[0].stats.marks, 1);
}

#[test]
fn a_runner_in_the_air_or_in_another_lane_does_not_pick() {
    let mut r = quiet();
    r.pickups.push(Pickup { dist: 104.0, d: 5.0, kind: PickupKind::Mark(Stat::Turbo), taken: false });
    r.pickups.push(Pickup { dist: 130.0, d: 9.0, kind: PickupKind::Mark(Stat::Guts), taken: false });
    put(&mut r, 0, 100.0, 5.0);
    // 踏み切ってすぐ上を通る
    run(&mut r, DT, Input { jump: true, ..Input::NONE });
    run(&mut r, 3.0, Input::NONE);
    assert!(!r.pickups[0].taken, "跳んでいる間は取れない");
    assert!(!r.pickups[1].taken, "進路が違えば取れない");
    assert_eq!(r.runners[0].lv, BASE_LEVELS);
}

#[test]
fn breath_restores_and_poison_drains_stamina() {
    let mut r = quiet();
    r.pickups.push(Pickup { dist: 104.0, d: 5.0, kind: PickupKind::Breath, taken: false });
    r.pickups.push(Pickup { dist: 108.5, d: 5.0, kind: PickupKind::Poison, taken: false });
    put(&mut r, 0, 100.0, 5.0);
    r.runners[0].stamina = 40.0;
    r.runners[0].since_dash = 0.0;
    run(&mut r, 0.5, Input::NONE);
    assert!((r.runners[0].stamina - 85.0).abs() < 0.01, "息で 45 回復: {}", r.runners[0].stamina);
    run(&mut r, 0.25, Input::NONE);
    assert!(r.pickups[1].taken);
    assert!((r.runners[0].stamina - 55.0).abs() < 0.01, "毒で 30 減る: {}", r.runners[0].stamina);
}

#[test]
fn picking_a_weapon_arms_the_runner_and_it_runs_out_by_time() {
    let mut r = quiet();
    r.pickups.push(Pickup { dist: 104.0, d: 5.0, kind: PickupKind::Arm(Weapon::Rose), taken: false });
    put(&mut r, 0, 100.0, 5.0);
    run(&mut r, 1.0, Input::NONE);
    assert_eq!(r.runners[0].weapon, Some(Weapon::Rose));
    assert_eq!(r.runners[0].weapon_uses, 3);
    let ev = run(&mut r, 12.0, Input::NONE);
    assert_eq!(r.runners[0].weapon, None, "12 秒で消える");
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::WeaponGone { i: 0 })));
}

#[test]
fn sword_hits_the_runner_in_front_once_and_they_keep_running() {
    let mut r = quiet();
    put(&mut r, 0, 100.0, 5.0);
    put(&mut r, 1, 102.4, 5.4);
    arm(&mut r, 0, Weapon::Sword);
    let mut ev = run(&mut r, DT, ATTACK);
    ev.append(&mut run(&mut r, DT, Input::NONE));
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Swing { i: 0, weapon: Weapon::Sword, charged: false })), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Hit { by: 0, i: 1, weapon: Weapon::Sword, charged: false })), "{ev:?}");
    let v = &r.runners[1];
    assert!(v.stumble > 1.5 && v.stumble <= 1.6, "よろけ 1.6 秒: {}", v.stumble);
    assert!((v.stamina - 75.0).abs() < 0.5, "スタミナ −25: {}", v.stamina);
    assert_eq!(r.runners[0].weapon_uses, 2);
    assert_eq!((r.runners[0].stats.hits_dealt, r.runners[1].stats.hits_taken), (1, 1));
    // すぐもう一度振っても、当たったあと 1.6 秒は受けない
    run(&mut r, 0.6, Input::NONE);
    let d = r.runners[0].dist + 2.0;
    put(&mut r, 1, d, 5.0);
    let mut ev = run(&mut r, DT, ATTACK);
    ev.append(&mut run(&mut r, DT, Input::NONE));
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Swing { .. })));
    assert!(!ev.iter().any(|e| matches!(e, RaceEvent::Hit { .. })), "続けては当たらない");
    // 倒れて止まらない: 当たったあとも前へ進む
    let before = r.runners[1].dist;
    run(&mut r, 1.0, Input::NONE);
    assert!(r.runners[1].dist - before > 7.0);
}

#[test]
fn sword_misses_a_runner_out_of_reach() {
    let mut r = quiet();
    put(&mut r, 0, 100.0, 5.0);
    put(&mut r, 1, 105.0, 5.0);
    put(&mut r, 2, 102.0, 8.0);
    arm(&mut r, 0, Weapon::Sword);
    let mut ev = run(&mut r, DT, ATTACK);
    ev.append(&mut run(&mut r, DT, Input::NONE));
    assert!(!ev.iter().any(|e| matches!(e, RaceEvent::Hit { .. })), "{ev:?}");
}

#[test]
fn holding_attack_spends_breath_and_releases_a_wave_that_a_jump_dodges() {
    let mut r = quiet();
    put(&mut r, 0, 100.0, 5.0);
    put(&mut r, 1, 118.0, 5.0);
    put(&mut r, 2, 128.0, 5.0);
    arm(&mut r, 0, Weapon::Sword);
    r.runners[0].since_dash = 0.0;
    let ev = run(&mut r, 1.0, ATTACK);
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::ChargeFull { i: 0 })), "{ev:?}");
    assert!((r.runners[0].stamina - 82.0).abs() < 0.6, "ためるのに 18 使う: {}", r.runners[0].stamina);
    // 1 番は波が来るころに跳ぶ。2 番はそのまま走る
    let mut ev = run(&mut r, DT, Input::NONE);
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Swing { i: 0, weapon: Weapon::Sword, charged: true })), "{ev:?}");
    assert_eq!(r.shots.len(), 1);
    for _ in 0..360 {
        let gap = r.runners[1].dist - r.shots.first().map_or(f64::MAX, |s| s.dist);
        if gap > 0.0 && gap < 3.0 && !r.runners[1].air { let x = &mut r.runners[1]; x.air = true; x.vy = TUNE.jump_v; }
        r.step(DT, Input::NONE);
        ev.append(&mut r.events);
    }
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Dodge { i: 1 })), "跳んでよける: {ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, RaceEvent::Hit { i: 1, .. })));
    let hit = ev.iter().find_map(|e| if let RaceEvent::Hit { by: 0, i: 2, charged, .. } = e { Some(*charged) } else { None });
    assert_eq!(hit, Some(true), "跳ばなかった走者には当たる: {ev:?}");
    assert!(r.shots.is_empty(), "30m 走ったら消える");
}

#[test]
fn chain_hits_around_and_pushes_outward() {
    let mut r = quiet();
    put(&mut r, 0, 100.0, 5.0);
    put(&mut r, 1, 99.0, 7.4);
    put(&mut r, 2, 101.5, 2.8);
    put(&mut r, 3, 104.0, 5.0);
    arm(&mut r, 0, Weapon::Chain);
    let mut ev = run(&mut r, DT, ATTACK);
    ev.append(&mut run(&mut r, DT, Input::NONE));
    let hits: Vec<usize> = ev.iter().filter_map(|e| if let RaceEvent::Hit { i, .. } = e { Some(*i) } else { None }).collect();
    assert_eq!(hits, vec![1, 2], "まわりの 2 人だけ: {ev:?}");
    run(&mut r, 0.3, Input::NONE);
    assert!(r.runners[1].d > 7.6 && r.runners[2].d < 2.6, "外へはじく: {} {}", r.runners[1].d, r.runners[2].d);
}

#[test]
fn rose_flies_down_the_lane_and_is_used_up() {
    let mut r = quiet();
    put(&mut r, 0, 100.0, 5.0);
    put(&mut r, 1, 115.0, 5.3);
    arm(&mut r, 0, Weapon::Rose);
    let mut ev = run(&mut r, DT, ATTACK);
    ev.append(&mut run(&mut r, 2.0, Input::NONE));
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Hit { by: 0, i: 1, weapon: Weapon::Rose, .. })), "{ev:?}");
    assert!(r.shots.is_empty(), "当たったバラは消える");
    // 3 回投げたら無くなる
    for _ in 0..2 { run(&mut r, DT, ATTACK); run(&mut r, 0.8, Input::NONE); }
    assert_eq!(r.runners[0].weapon, None);
}

#[test]
fn bottle_leaves_a_slick_that_trips_a_follower_but_not_the_owner_or_a_jumper() {
    let mut r = quiet();
    put(&mut r, 0, 100.0, 5.0);
    put(&mut r, 1, 92.0, 5.0);
    put(&mut r, 2, 84.0, 5.0);
    arm(&mut r, 0, Weapon::Bottle);
    let mut ev = run(&mut r, DT, ATTACK);
    ev.append(&mut run(&mut r, DT, Input::NONE));
    assert_eq!(r.traps.len(), 1);
    let at = r.traps[0].dist;
    assert!(at < 100.0);
    for _ in 0..360 {
        // 1 番は手前で跳ぶ
        let gap = at - r.runners[1].dist;
        if gap > 0.0 && gap < 3.5 && !r.runners[1].air { let x = &mut r.runners[1]; x.air = true; x.vy = TUNE.jump_v; }
        r.step(DT, Input::NONE);
        ev.append(&mut r.events);
    }
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Dodge { i: 1 })), "{ev:?}");
    assert!(ev.iter().any(|e| matches!(e, RaceEvent::Hit { by: 0, i: 2, weapon: Weapon::Bottle, .. })), "{ev:?}");
    assert!(!ev.iter().any(|e| matches!(e, RaceEvent::Hit { i: 0, .. })));
    assert!(r.traps.is_empty(), "踏まれたら消える");
}

#[test]
fn levels_change_how_fast_a_runner_goes() {
    let dist_after = |lv: Levels| {
        let mut all = [BASE_LEVELS; 8];
        all[0] = lv;
        let mut r = race(0, 7, Some(0), Some(all));
        for i in 1..8 { r.clear_brain(i); }
        r.pickups.clear();
        r.t = 0.0;
        r.started = true;
        run(&mut r, 20.0, Input { dash: true, ..Input::NONE });
        (r.runners[0].dist, r.runners[0].stamina)
    };
    let (base, _) = dist_after(BASE_LEVELS);
    let (fast, _) = dist_after([9, 3, 3, 3, 3]);
    let (slow, _) = dist_after([0, 3, 3, 3, 3]);
    assert!(fast > base * 1.06 && fast < base * 1.11, "スピード 9 段は約 9% 速い: {fast} / {base}");
    assert!(slow < base * 0.99);
    let (turbo, _) = dist_after([3, 3, 3, 3, 9]);
    assert!(turbo > base + 1.0, "ターボはダッシュの伸びが大きい: {turbo} / {base}");
    // スタミナ: 同じだけ押して、残りが多い
    let left = |lv: Levels| {
        let mut all = [BASE_LEVELS; 8];
        all[0] = lv;
        let mut r = race(0, 7, Some(0), Some(all));
        for i in 1..8 { r.clear_brain(i); }
        r.pickups.clear();
        r.t = 0.0;
        r.started = true;
        run(&mut r, 2.0, Input { dash: true, ..Input::NONE });
        r.runners[0].stamina
    };
    assert!(left([3, 9, 3, 3, 3]) > left(BASE_LEVELS) + 5.0);
}

#[test]
fn guts_softens_a_hit() {
    let mut r = quiet();
    r.runners[1].lv = [3, 3, 9, 3, 3];
    put(&mut r, 0, 100.0, 5.0);
    put(&mut r, 1, 102.4, 5.0);
    arm(&mut r, 0, Weapon::Sword);
    run(&mut r, DT, ATTACK);
    run(&mut r, DT, Input::NONE);
    let v = &r.runners[1];
    assert!(v.stumble > 0.6 && v.stumble < 0.65, "ガッツ 9 段でよろけが 60% 減る（1.6 秒 → 0.64 秒）: {}", v.stumble);
    assert!(v.stamina > 88.0);
}

#[test]
fn cpu_runners_pick_marks_and_fight_and_the_race_still_finishes() {
    let (mut picks, mut hits, mut swings) = (0, 0, 0);
    for seed in [3u32, 11, 29] {
        let mut r = race(seed as usize % 3, seed, None, None);
        let mut guard = 0;
        while !r.done {
            r.step(DT, Input::NONE);
            for e in r.events.drain(..) {
                match e { RaceEvent::Pick { .. } => picks += 1, RaceEvent::Hit { .. } => hits += 1, RaceEvent::Swing { .. } => swings += 1, _ => {} }
            }
            guard += 1;
            assert!(guard < 120 * 200, "200 秒で終わらない");
        }
        let last = r.runners.iter().map(|x| x.finish_t).fold(0.0, f64::max);
        let first = r.runners.iter().map(|x| x.finish_t).fold(f64::MAX, f64::min);
        assert!(last - first < 20.0, "攻撃で大差が付きすぎない: {first} 〜 {last}");
    }
    assert!(picks >= 45, "3 レースで 45 個以上は取る: {picks}");
    assert!(swings >= 9, "武器を振る: {swings}");
    assert!(hits >= 3, "当たることがある: {hits}");
}

/// 測る用（`cargo test -p sim --test items report -- --ignored --nocapture`）: 全員 CPU のレースで、落ちている物の使われ方と、1 着のタイムの分布（落ちている物あり・なし）
#[test]
#[ignore]
fn report() {
    for (ci, name) in ["turf", "water", "hill"].iter().enumerate() {
        for diff in [Difficulty::Normal, Difficulty::Hard] {
            for items in [false, true] {
                let (mut picks, mut swings, mut hits, mut dodges, mut charged) = (0, 0, 0, 0, 0);
                let mut wins: Vec<f64> = Vec::new();
                let mut spread = 0.0;
                let mut placed = 0;
                let n = 40;
                for seed in 0..n {
                    let defs = courses();
                    let mut r = Race::new(RaceOptions { course: &defs[ci], seed: 1000 + seed, difficulty: diff, player_gate: None, auto_jump: false, wide_jump: false, levels: None, humans: vec![] });
                    if !items { r.pickups.clear(); }
                    placed += r.pickups.len();
                    while !r.done {
                        r.step(DT, Input::NONE);
                        for e in r.events.drain(..) {
                            match e { RaceEvent::Pick { .. } => picks += 1, RaceEvent::Hit { .. } => hits += 1, RaceEvent::Dodge { .. } => dodges += 1, RaceEvent::Swing { charged: c, .. } => { swings += 1; if c { charged += 1; } } _ => {} }
                        }
                    }
                    let mut t: Vec<f64> = r.runners.iter().map(|x| x.finish_t).collect();
                    t.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    wins.push(t[0]);
                    spread += t[7] - t[0];
                }
                wins.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let q = |k: f64| wins[((wins.len() - 1) as f64 * k) as usize];
                let f = n as f64;
                println!("{name} {:?} items={items}: 置いた {:.0} 取った {:.1} 振った {:.1}（ため {:.1}） 当たり {:.1} よけ {:.1} | 1 着 最速 {:.2} 10% {:.2} 40% {:.2} 90% {:.2} | 1 着と 8 着の差 {:.1}", diff, placed as f64 / f, picks as f64 / f, swings as f64 / f, charged as f64 / f, hits as f64 / f, dodges as f64 / f, q(0.0), q(0.1), q(0.4), q(0.9), spread / f);
            }
        }
    }
}

/// 測る用: 相手が 7 段のむずかしいレースで、同じ上手さの 1 人だけ能力を変えると何着になるか（育成で、どれだけ育てば付いていけるか）
#[test]
#[ignore]
fn report_levels() {
    for (cpu, diff) in [(7u8, Difficulty::Hard), (4, Difficulty::Normal), (1, Difficulty::Easy)] {
        for me in [1u8, 3, 5, 7, 9] {
            let (mut sum, mut top4, mut gap) = (0, 0, 0.0);
            let n = 60;
            for seed in 0..n {
                let defs = courses();
                let mut lv = [[cpu; 5]; 8];
                let g = (seed % 8) as usize;
                lv[g] = [me; 5];
                let mut r = Race::new(RaceOptions { course: &defs[(seed % 3) as usize], seed: 500 + seed, difficulty: diff, player_gate: None, auto_jump: false, wide_jump: false, levels: Some(lv), humans: vec![] });
                while !r.done { r.step(DT, Input::NONE); r.events.clear(); }
                sum += r.runners[g].place;
                if r.runners[g].place <= 4 { top4 += 1; }
                gap += r.runners[g].finish_t - r.runners.iter().map(|x| x.finish_t).fold(f64::MAX, f64::min);
            }
            println!("相手 {cpu} 段 {:?}・自分 {me} 段: 平均 {:.1} 着・4 着以内 {:.0}%・1 着との差 {:.2} 秒", diff, sum as f64 / n as f64, top4 as f64 * 100.0 / n as f64, gap / n as f64);
        }
    }
}

#[test]
fn a_tap_right_after_a_swing_is_kept_and_fires_when_the_weapon_is_ready() {
    // 防ぐ故障: 連打の 2 回目が、振った直後の 0.5 秒に押すと消える
    let mut r = quiet();
    put(&mut r, 0, 100.0, 5.0);
    arm(&mut r, 0, Weapon::Sword);
    run(&mut r, DT, ATTACK);
    let ev = run(&mut r, 0.2, Input::NONE);
    assert_eq!(ev.iter().filter(|e| matches!(e, RaceEvent::Swing { .. })).count(), 1);
    // 0.2 秒後（まだ振れない間）に短く押す
    let mut ev = run(&mut r, 0.06, ATTACK);
    ev.append(&mut run(&mut r, 0.2, Input::NONE));
    assert!(ev.is_empty(), "振れるようになるまでは出ない: {ev:?}");
    let ev = run(&mut r, 0.2, Input::NONE);
    assert_eq!(ev.iter().filter(|e| matches!(e, RaceEvent::Swing { charged: false, .. })).count(), 1, "振れるようになったら出る: {ev:?}");
    assert_eq!(r.runners[0].weapon_uses, 1);
}

/// 当たった相手は、当たらなかった走者より何 m 遅れるか（バラ・ためた一撃）
fn hit_loss(charged: bool) -> f64 {
    let mut r = quiet();
    put(&mut r, 0, 100.0, 5.0);
    put(&mut r, 1, 112.0, 5.3);
    // 比べる走者: 同じ所から、離れた進路を走る
    put(&mut r, 2, 112.0, 10.5);
    arm(&mut r, 0, Weapon::Rose);
    if charged { r.runners[0].stamina = 100.0; run(&mut r, 0.9, ATTACK); }
    let ev = run(&mut r, DT, if charged { Input::NONE } else { ATTACK });
    let mut ev2 = run(&mut r, 6.0, Input::NONE);
    ev2.extend(ev);
    assert!(ev2.iter().any(|e| matches!(e, RaceEvent::Hit { i: 1, .. })), "当たる: {ev2:?}");
    r.runners[2].dist - r.runners[1].dist
}

/// 防ぐ故障: バラを当てても止まる時間が短く、すぐ後ろからでも抜けない
#[test]
fn a_hit_costs_enough_to_be_overtaken() {
    let (plain, charged) = (hit_loss(false), hit_loss(true));
    println!("当たりで遅れる: ふつう {plain:.1} m・ため {charged:.1} m");
    assert!(plain >= 12.0, "ふつうの当たりで 12 m 以上遅れる: {plain:.1}");
    assert!(charged >= plain + 4.0, "ためた一撃はもっと遅れる: {charged:.1} / {plain:.1}");
}
