// 難しさ（docs/gyogyo-spec.md の「難しさ」）。
// 測る用（`cargo test -p sim --test difficulty report -- --ignored --nocapture`）: 人の代わりに CPU と同じ脳で 1 人を走らせ、難しさごとの着順を測る。
// 防ぐ故障: ふつうでも CPU に勝てない。
use sim::ai::{Brain, Difficulty, View};
use sim::career::{Career, RACES};
use sim::race::{Input, Race, RaceOptions, Rng};
use sim::track::courses;

const DT: f64 = 1.0 / 120.0;

/// 人の代わり: skill の上手さ・form の使い残し（1 = ふつう、大きいほど息を余らせる）で 1 人を走らせ、その着順。skill が None は何もしない人（進路もダッシュも触らない）
fn place_in(course: usize, seed: u32, diff: Difficulty, who: Option<(Difficulty, f64)>, career: bool) -> i32 {
    let defs = courses();
    let gate = (seed % 8) as usize;
    let levels = career.then(|| Career::new("me").levels_for(gate));
    let mut r = Race::new(RaceOptions { course: &defs[course], seed, difficulty: diff, player_gate: Some(gate), auto_jump: false, wide_jump: false, levels, humans: vec![] });
    let mut rand = Rng(seed ^ 0x5eed);
    let mut me = who.map(|(skill, form)| Brain::new(gate, &r.runners, &mut rand, skill, defs[course].gentle, false, Some(form)));
    while !r.done {
        let input = match me.as_mut() {
            Some(me) => { let mut v = View { track: &r.track, runners: &r.runners, obstacle_list: &r.obstacle_list, pickups: &r.pickups, shots: &r.shots, traps: &r.traps, rand: &mut rand }; me.decide(DT, &mut v) }
            None => Input::NONE,
        };
        r.step(DT, input);
        r.events.clear();
    }
    r.runners[gate].place
}

fn place_of(course: usize, seed: u32, diff: Difficulty, skill: Difficulty, form: f64) -> i32 { place_in(course, seed, diff, Some((skill, form)), false) }

/// 着順の平均・1 着の割合・3 着以内の割合（course が None なら 3 コースを順に）
fn tally(n: u32, f: impl Fn(u32) -> i32) -> (f64, f64, f64) {
    let (mut sum, mut win, mut top3) = (0, 0, 0);
    for k in 0..n {
        let p = f(k);
        sum += p;
        if p == 1 { win += 1; }
        if p <= 3 { top3 += 1; }
    }
    (sum as f64 / n as f64, win as f64 * 100.0 / n as f64, top3 as f64 * 100.0 / n as f64)
}

/// 育成の第 1 戦（わかば新馬戦: やさしい・相手 1 段・自分 1 段）
fn first_career(who: Option<(Difficulty, f64)>, n: u32) -> (f64, f64, f64) {
    let r = &RACES[0];
    let course = courses().iter().position(|c| c.id == r.course).unwrap();
    tally(n, |k| place_in(course, 7000 + k, r.difficulty, who, true))
}

fn measure(diff: Difficulty, skill: Difficulty, form: f64, n: u32) -> (f64, f64, f64) {
    let (mut sum, mut win, mut top3) = (0, 0, 0);
    for k in 0..n {
        let p = place_of((k % 3) as usize, 9000 + k, diff, skill, form);
        sum += p;
        if p == 1 { win += 1; }
        if p <= 3 { top3 += 1; }
    }
    (sum as f64 / n as f64, win as f64 * 100.0 / n as f64, top3 as f64 * 100.0 / n as f64)
}

#[test]
#[ignore]
fn report() {
    for (who, skill, form) in [("上手い人", Difficulty::Normal, 1.0), ("ふつうの人", Difficulty::Easy, 1.3), ("はじめての人", Difficulty::Easy, 2.0)] {
        for diff in [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard] {
            let (avg, win, top3) = measure(diff, skill, form, 90);
            println!("{who} × {:?}: 平均 {avg:.1} 着・1 着 {win:.0}%・3 着以内 {top3:.0}%", diff);
        }
    }
    for diff in [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard] {
        let (avg, win, top3) = tally(90, |k| place_in((k % 3) as usize, 9000 + k, diff, None, false));
        println!("何もしない人 × {:?}: 平均 {avg:.1} 着・1 着 {win:.0}%・3 着以内 {top3:.0}%", diff);
    }
    for (who, w) in [("何もしない人", None), ("はじめての人", Some((Difficulty::Easy, 2.0))), ("ふつうの人", Some((Difficulty::Easy, 1.3))), ("上手い人", Some((Difficulty::Normal, 1.0)))] {
        let (avg, win, top3) = first_career(w, 60);
        println!("{who} × 育成の第 1 戦: 平均 {avg:.1} 着・1 着 {win:.0}%・3 着以内 {top3:.0}%");
    }
}

/// 何もしない（ダッシュも進路も触らない）と勝てない。育成の第 1 戦（やさしい）でも
/// 防ぐ故障: わかば新馬戦で何もしなくても 1 着になる
#[test]
fn doing_nothing_does_not_win() {
    let (avg, win, _) = first_career(None, 30);
    assert!(win <= 5.0 && avg >= 5.0, "何もしない人 × 育成の第 1 戦: 平均 {avg:.1} 着・1 着 {win:.0}%");
    let (avg, win, _) = tally(30, |k| place_in((k % 3) as usize, 9000 + k, Difficulty::Easy, None, false));
    assert!(win <= 5.0 && avg >= 5.0, "何もしない人 × やさしい: 平均 {avg:.1} 着・1 着 {win:.0}%");
}

/// ふつうの難しさで、ふつうの人（上手さはやさしいの CPU と同じ）が 3 着以内に入れる
#[test]
fn an_average_player_can_place_on_normal() {
    let (avg, _, top3) = measure(Difficulty::Normal, Difficulty::Easy, 1.3, 30);
    assert!(top3 >= 40.0 && avg <= 3.8, "ふつうの人 × ふつう: 平均 {avg:.1} 着・3 着以内 {top3:.0}%");
}
