// 端末に残すもの（企画 10）: 自己ベスト（キャラ・コース・難しさ・補助ごと）、はじめての記録、設定、カップの途中経過。src/save.ts の写し。
// 形は TS と同じ JSON（キー gyogyo.save）。知らないキーもそのまま残す。
use island_kit::app::store;
use serde_json::{Value, json};
use sim::ai::Difficulty;
use sim::career::{Career, HallEntry};
use sim::cup::CupRow;

const KEY: &str = "gyogyo.save";

#[derive(Clone, Debug)]
pub struct Settings { pub hero: String, pub course: String, pub difficulty: Difficulty, pub auto_jump: bool, pub wide_jump: bool, pub low: bool, pub first_done: bool }

pub struct Save { pub data: Value, pub s: Settings }

fn fresh() -> Value {
    json!({
        "settings": { "hero": "", "course": "turf", "difficulty": "normal", "autoJump": false, "wideJump": false, "low": false, "firstDone": false },
        "bests": {}, "firsts": { "finish": false, "podium": false, "win": false, "cupWin": false }, "cup": null, "career": null, "hall": [], "races": 0, "wins": 0, "badges": {}, "medals": {},
    })
}

pub fn row_to_json(r: &CupRow) -> Value { json!({ "id": r.id, "points": r.points, "wins": r.wins, "last": r.last, "places": r.places }) }
pub fn row_of_json(v: &Value) -> Option<CupRow> {
    Some(CupRow {
        id: v.get("id")?.as_str()?.to_string(), points: v.get("points")?.as_i64()? as i32, wins: v.get("wins")?.as_i64()? as i32, last: v.get("last")?.as_i64()? as i32,
        places: v.get("places")?.as_array()?.iter().filter_map(|x| x.as_i64().map(|n| n as i32)).collect(),
    })
}

fn levels_of(v: &Value) -> Option<[u8; 5]> {
    let a = v.as_array()?;
    if a.len() != 5 { return None; }
    let mut out = [0u8; 5];
    for (k, x) in a.iter().enumerate() { out[k] = x.as_u64()?.min(9) as u8; }
    Some(out)
}
fn places_of(v: &Value) -> Option<Vec<i32>> { Some(v.as_array()?.iter().filter_map(|x| x.as_i64().map(|n| n as i32)).collect()) }
/// 育成の途中経過
pub fn career_to_json(c: &Career) -> Value { json!({ "hero": c.hero, "index": c.index, "levels": c.levels, "prize": c.prize, "wins": c.wins, "places": c.places }) }
pub fn career_of_json(v: &Value) -> Option<Career> {
    let index = v.get("index")?.as_u64()? as usize;
    if index >= sim::career::RACES.len() { return None; }
    Some(Career { hero: v.get("hero")?.as_str()?.to_string(), index, levels: levels_of(v.get("levels")?)?, prize: v.get("prize")?.as_i64()? as i32, wins: v.get("wins")?.as_i64()? as i32, places: places_of(v.get("places")?)?, retired: false })
}
/// 殿堂の 1 人
pub fn hall_to_json(e: &HallEntry) -> Value { json!({ "hero": e.hero, "prize": e.prize, "wins": e.wins, "places": e.places, "levels": e.levels, "title": e.title }) }
pub fn hall_of_json(v: &Value) -> Option<HallEntry> {
    Some(HallEntry { hero: v.get("hero")?.as_str()?.to_string(), prize: v.get("prize")?.as_i64()? as i32, wins: v.get("wins")?.as_i64()? as i32, places: places_of(v.get("places")?)?, levels: levels_of(v.get("levels")?)?, title: v.get("title")?.as_str()?.to_string() })
}

/// 整数の値の f64 を整数の字にする（JSON の 4.0 → 4）
fn js_numbers(v: &mut Value) {
    match v {
        Value::Number(n) => { if let Some(f) = n.as_f64() && !n.is_i64() && !n.is_u64() && f.fract() == 0.0 && f.abs() < 9e15 { *v = json!(f as i64); } }
        Value::Array(a) => a.iter_mut().for_each(js_numbers),
        Value::Object(o) => o.values_mut().for_each(js_numbers),
        _ => {}
    }
}

impl Save {
    pub fn load() -> Save {
        let mut d = fresh();
        if let Some(Value::Object(j)) = store::get_json(KEY) {
            for (k, v) in j {
                if k == "settings" || k == "firsts" {
                    if let (Some(base), Value::Object(over)) = (d.get_mut(&k).and_then(|x| x.as_object_mut()), v) { for (a, b) in over { base.insert(a, b); } }
                } else { d[k] = v; }
            }
        }
        let st = &d["settings"];
        let s = Settings {
            hero: st["hero"].as_str().unwrap_or("").to_string(), course: st["course"].as_str().unwrap_or("turf").to_string(),
            difficulty: st["difficulty"].as_str().and_then(Difficulty::parse).unwrap_or(Difficulty::Normal),
            auto_jump: st["autoJump"].as_bool().unwrap_or(false), wide_jump: st["wideJump"].as_bool().unwrap_or(false), low: st["low"].as_bool().unwrap_or(false), first_done: st["firstDone"].as_bool().unwrap_or(false),
        };
        Save { data: d, s }
    }
    pub fn write(&mut self) {
        let s = &self.s;
        let st = &mut self.data["settings"];
        st["hero"] = json!(s.hero); st["course"] = json!(s.course); st["difficulty"] = json!(s.difficulty.id());
        st["autoJump"] = json!(s.auto_jump); st["wideJump"] = json!(s.wide_jump); st["low"] = json!(s.low); st["firstDone"] = json!(s.first_done);
        // 今の版の JSON.stringify と同じ数の書き方（4.0 でなく 4）
        let mut out = self.data.clone();
        js_numbers(&mut out);
        store::set_json(KEY, &out);
    }
    /// bestKey
    pub fn best_key(&self, hero: &str, course: &str) -> String {
        format!("{hero}|{course}|{}|{}{}", self.s.difficulty.id(), if self.s.auto_jump { "a" } else { "" }, if self.s.wide_jump { "w" } else { "" })
    }
    pub fn best(&self, key: &str) -> Option<f64> { self.data["bests"].get(key).and_then(|v| v.as_f64()) }
    pub fn split(&self, key: &str) -> Option<f64> { self.data.get("splits")?.get(key)?.as_f64() }
    pub fn ghost(&self, key: &str) -> Option<Vec<f64>> { Some(self.data.get("ghosts")?.get(key)?.as_array()?.iter().filter_map(|x| x.as_f64()).collect()) }
    /// `data.xx ??= {}` のあと入れる
    pub fn put(&mut self, group: &str, key: &str, v: Value) {
        if !self.data.get(group).is_some_and(|x| x.is_object()) { self.data[group] = json!({}); }
        self.data[group][key] = v;
    }
    pub fn num(&self, name: &str) -> i64 { self.data[name].as_i64().unwrap_or(0) }
    /// 育成の途中経過（無ければ None）と、殿堂
    pub fn career(&self) -> Option<Career> { career_of_json(self.data.get("career")?) }
    pub fn hall(&self) -> Vec<HallEntry> { self.data.get("hall").and_then(|h| h.as_array()).map_or(Vec::new(), |a| a.iter().filter_map(hall_of_json).collect()) }
}
