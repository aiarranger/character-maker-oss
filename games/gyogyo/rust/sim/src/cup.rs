// 3 戦カップの点数と順位（企画 10）。同点なら 1 着回数、その次に最終戦の着順。src/sim/cup.ts の写し。
use std::collections::HashMap;

pub const POINTS: [i32; 8] = [10, 8, 6, 5, 4, 3, 2, 1];

#[derive(Clone, Debug, PartialEq)]
pub struct CupRow { pub id: String, pub points: i32, pub wins: i32, pub last: i32, pub places: Vec<i32> }

pub fn empty_cup(ids: &[String]) -> Vec<CupRow> {
    ids.iter().map(|id| CupRow { id: id.clone(), points: 0, wins: 0, last: 9, places: Vec::new() }).collect()
}

/// places: id → そのレースの着順
pub fn add_race(rows: &mut [CupRow], places: &HashMap<String, i32>) {
    for r in rows.iter_mut() {
        // TS の POINTS[p - 1] ?? 0（名簿に無い走者は 0 点）
        let p = places.get(&r.id).copied().unwrap_or(0);
        r.points += if (1..=8).contains(&p) { POINTS[p as usize - 1] } else { 0 };
        if p == 1 { r.wins += 1; }
        r.last = p;
        r.places.push(p);
    }
}

pub fn standings(rows: &[CupRow]) -> Vec<CupRow> {
    let mut out = rows.to_vec();
    out.sort_by(|a, b| b.points.cmp(&a.points).then(b.wins.cmp(&a.wins)).then(a.last.cmp(&b.last)));
    out
}
