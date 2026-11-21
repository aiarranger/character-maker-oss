// 育成: デビューから引退までの全 8 戦（docs/spec.md「育成」）。4 着以内で次へ進み、5 着以下はその場で引退。
// レース中に取ったマークの分だけ能力が育ち、次のレースへ持ち越す。保存の形（JSON）は game/src/save.rs。
use crate::ai::Difficulty;
use crate::items::Levels;

pub struct CareerRace {
    pub grade: &'static str,
    pub name: &'static str,
    /// コースの id（track::courses）
    pub course: &'static str,
    pub difficulty: Difficulty,
    /// 相手の能力の段（全部の能力が同じ段）
    pub cpu_level: u8,
    /// 1 着の賞金（万円）
    pub purse: i32,
}

pub const RACES: [CareerRace; 8] = [
    CareerRace { grade: "新馬", name: "わかば新馬戦", course: "turf", difficulty: Difficulty::Easy, cpu_level: 1, purse: 300 },
    CareerRace { grade: "一般", name: "こもれび特別", course: "water", difficulty: Difficulty::Easy, cpu_level: 1, purse: 500 },
    CareerRace { grade: "一般", name: "夕やけ賞", course: "hill", difficulty: Difficulty::Normal, cpu_level: 2, purse: 500 },
    CareerRace { grade: "GIII", name: "しずく杯", course: "turf", difficulty: Difficulty::Normal, cpu_level: 3, purse: 1000 },
    CareerRace { grade: "GIII", name: "そよかぜステークス", course: "water", difficulty: Difficulty::Normal, cpu_level: 4, purse: 1000 },
    CareerRace { grade: "GII", name: "ほしぞら記念", course: "hill", difficulty: Difficulty::Normal, cpu_level: 5, purse: 2000 },
    CareerRace { grade: "GI", name: "にじいろ大賞", course: "water", difficulty: Difficulty::Hard, cpu_level: 5, purse: 4000 },
    CareerRace { grade: "GI", name: "魚々娘グランプリ", course: "hill", difficulty: Difficulty::Hard, cpu_level: 6, purse: 6000 },
];

/// 自分の始めの能力
pub const START_LEVELS: Levels = [1; 5];
/// 次へ進める着順
pub const PASS_PLACE: i32 = 4;
pub const HALL_MAX: usize = 20;

/// 賞金は 3 着まで（2 着は 40%、3 着は 25%）
pub fn prize_of(purse: i32, place: i32) -> i32 {
    match place { 1 => purse, 2 => purse * 40 / 100, 3 => purse * 25 / 100, _ => 0 }
}

/// 引退のときの称号（賞金で決まる。8 戦 8 勝は別）
pub fn title_of(prize: i32, wins: i32, races: usize) -> &'static str {
    if races == RACES.len() && wins == RACES.len() as i32 { return "無敗の伝説"; }
    if prize >= 11000 { "殿堂の名ランナー" } else if prize >= 7000 { "GI ランナー" } else if prize >= 3000 { "重賞ランナー" } else if prize >= 1000 { "オープンランナー" } else { "かけだし" }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Career {
    pub hero: String,
    /// 次に走るレース（RACES の番号）。引退したら、最後に走ったレース
    pub index: usize,
    pub levels: Levels,
    pub prize: i32,
    pub wins: i32,
    /// 走ったレースの着順
    pub places: Vec<i32>,
    pub retired: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Outcome { pub prize: i32, pub retired: bool }

impl Career {
    pub fn new(hero: &str) -> Career { Career { hero: hero.to_string(), index: 0, levels: START_LEVELS, prize: 0, wins: 0, places: Vec::new(), retired: false } }
    pub fn race(&self) -> &'static CareerRace { &RACES[self.index.min(RACES.len() - 1)] }
    /// 相手の能力（枠の順に 8 人分。自分の枠だけ自分の値）
    pub fn levels_for(&self, player_gate: usize) -> [Levels; 8] {
        let mut all = [[self.race().cpu_level; 5]; 8];
        all[player_gate] = self.levels;
        all
    }
    /// レースを走り終えた: 着順と、ゴールしたときの能力（取ったマークの分が入っている）
    pub fn record(&mut self, place: i32, levels: Levels) -> Outcome {
        let prize = prize_of(self.race().purse, place);
        self.prize += prize;
        if place == 1 { self.wins += 1; }
        self.places.push(place);
        self.levels = levels;
        if place > PASS_PLACE || self.index + 1 >= RACES.len() { self.retired = true; } else { self.index += 1; }
        Outcome { prize, retired: self.retired }
    }
    pub fn title(&self) -> &'static str { title_of(self.prize, self.wins, self.places.len()) }
}

/// 殿堂の 1 人（引退した走者）
#[derive(Clone, Debug, PartialEq)]
pub struct HallEntry { pub hero: String, pub prize: i32, pub wins: i32, pub places: Vec<i32>, pub levels: Levels, pub title: String }
impl HallEntry {
    pub fn of(c: &Career) -> HallEntry { HallEntry { hero: c.hero.clone(), prize: c.prize, wins: c.wins, places: c.places.clone(), levels: c.levels, title: c.title().to_string() } }
}

/// 殿堂へ入れる（賞金の多い順。同じ額は先に入った方が上。HALL_MAX 人まで）
pub fn hall_insert(list: &mut Vec<HallEntry>, e: HallEntry) {
    let at = list.iter().position(|x| x.prize < e.prize).unwrap_or(list.len());
    list.insert(at, e);
    list.truncate(HALL_MAX);
}
