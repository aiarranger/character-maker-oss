// 結果の言葉: 着差の呼び方・走行データから言える振り返りを一つ（企画 09）。根拠のない助言や、毎回同じ説教は出さない。src/sim/result.ts の写し。
use crate::race::{Race, Runner, TUNE};
use sim_math::{max, round};

/// JS の toFixed（2 つが同じ近さのときは大きい方。Rust の {:.n} は偶数へ丸めるので、ちょうど半分だけ直す）
pub fn to_fixed(x: f64, digits: usize) -> String {
    let s = format!("{:.*}", digits, x);
    // 10^digits 倍で端数がちょうど 0.5 のときだけ違う（有限の 2 進小数なので 80 桁で見れば足りる）
    let long = format!("{:.*}", digits + 80, x.abs());
    let cut = long.len() - 80;
    if long.as_bytes()[cut] != b'5' || long[cut + 1..].bytes().any(|b| b != b'0') { return s; }
    let head = &long[..cut];
    let mut bytes: Vec<u8> = head.bytes().collect();
    let mut i = bytes.len();
    loop {
        if i == 0 { bytes.insert(0, b'1'); break; }
        i -= 1;
        if bytes[i] == b'.' { continue; }
        if bytes[i] == b'9' { bytes[i] = b'0'; } else { bytes[i] += 1; break; }
    }
    let t = String::from_utf8(bytes).unwrap();
    let t = t.strip_suffix('.').map(str::to_string).unwrap_or(t);
    if x < 0.0 { format!("-{t}") } else { t }
}

/// 体の長さ（着差の 1 身）
const BODY: f64 = 2.4;
/// 時間差 → 競走の着差の呼び方（ハナ・アタマ・クビ・1/2・3/4・1…・大差）
pub fn margin_text(dt: f64) -> &'static str {
    let body = dt * TUNE.base * TUNE.dash_mul / BODY;
    if dt < 0.002 { return "同着"; }
    if body < 0.08 { return "ハナ"; }
    if body < 0.18 { return "アタマ"; }
    if body < 0.35 { return "クビ"; }
    if body < 0.62 { return "1/2"; }
    if body < 0.87 { return "3/4"; }
    if body < 1.13 { return "1"; }
    if body < 1.37 { return "1 1/4"; }
    if body < 1.62 { return "1 1/2"; }
    if body < 1.87 { return "1 3/4"; }
    if body < 2.25 { return "2"; }
    if body < 2.75 { return "2 1/2"; }
    if body < 3.5 { return "3"; }
    if body < 4.5 { return "4"; }
    if body < 5.5 { return "5"; }
    if body < 8.0 { return "6"; }
    if body < 12.0 { return "10"; }
    "大差"
}

/// 文の中で読める着差（数字は「身」を付ける）
pub fn margin_words(dt: f64) -> String {
    let m = margin_text(dt);
    if m.as_bytes()[0].is_ascii_digit() { format!("{m}身") } else { m.to_string() }
}

pub fn time_text(t: f64) -> String {
    let m = (t / 60.0).floor();
    let s = t - m * 60.0;
    format!("{}:{:0>5}", m as i64, to_fixed(s, 2))
}

/// このレースの走りを、データで言える一言に。一番効いたものだけ
pub fn reflect(r: &Runner, race: &Race) -> (String, String) {
    let s = &r.stats;
    let dash_total = s.dash_early + s.dash_late;
    let place = r.place;
    let leader = race.runners.iter().find(|x| x.place == 1).unwrap();
    let gap = r.finish_t - leader.finish_t;
    let avg_lane = s.lane_sum / max(1.0, s.lane_n as f64);
    let mut options: Vec<(f64, String, String)> = Vec::new();
    let mut o = |w: f64, line: String, next: &str| options.push((w, line, next.to_string()));
    if s.empties > 0 { o(3.0 + s.empties as f64, format!("スタミナを {} 回切らして、足が止まった", s.empties), "切れる前に離すと、0.8 秒後から回復が始まる"); }
    if s.final_stamina > 45.0 { o(2.0 + s.final_stamina / 30.0, format!("スタミナを {} 残してゴールした", round(s.final_stamina) as i64), "最後の直線に入ったら、残りを使い切ってよい"); }
    if s.slings == 0 && s.draft > 6.0 { o(2.2, format!("風よけに {} 秒入っていたが、抜け出さなかった", to_fixed(s.draft, 0)), "後ろで溜めて横へ出ると、少し伸びる"); }
    if s.blocked > 7.0 { o(1.0 + s.blocked / 5.0, format!("前が詰まっていた時間が {} 秒あった", to_fixed(s.blocked, 1)), "一人分だけ外へ出ると、前が空く"); }
    if s.hurdles >= 1 { o(2.5 + s.hurdles as f64, format!("ハードルに {} 回ぶつかった", s.hurdles), "手前の矢じりが見えたら、一拍おいて跳ぶ"); }
    if s.wet > 1.2 { o(2.0 + s.wet, format!("水たまりで {} 秒ぬかるんだ", to_fixed(s.wet, 1)), "跳ぶか、水のない所へ寄る"); }
    if avg_lane > race.track.width * 0.6 { o(2.0 + avg_lane / 4.0, format!("外を長く走って {}m 余計に走った", round(s.outer) as i64), "抜いたら内へ戻ると、距離を損しない"); }
    if s.hits_taken >= 2 { o(1.6 + s.hits_taken as f64, format!("攻撃を {} 回受けて、よろけた", s.hits_taken), "飛んでくる物と床のわなは、跳ぶとよけられる"); }
    if s.marks == 0 && !race.pickups.is_empty() { o(2.4, "マークを 1 つも取らなかった".to_string(), "丸い札の上を通ると、能力が上がる"); }
    if dash_total < 12.0 { o(3.0, format!("ダッシュは合計 {} 秒だけ", to_fixed(dash_total, 0)), "回復を待ちながら、短く何度も使える"); }
    if s.dash_early > s.dash_late * 2.2 && dash_total > 12.0 { o(2.2, format!("前半に {} 秒使って、後半は {} 秒だった", to_fixed(s.dash_early, 0), to_fixed(s.dash_late, 0)), "次は仕掛けを少し遅らせる"); }
    if s.dash_late > s.dash_early * 3.0 && place > 1 { o(2.0, format!("後半に {} 秒使ったが、前半の差が {} 秒残った", to_fixed(s.dash_late, 0), to_fixed(gap, 1)), "序盤に短く使って、前で位置を取る"); }
    options.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    if place == 1 {
        let second = race.runners.iter().find(|x| x.place == 2).unwrap();
        let line = format!("{} 回抜いて、{}差で勝った", s.passes, margin_words(second.finish_t - r.finish_t));
        let next = options.first().map_or("次は一つ上の難しさで".to_string(), |x| x.2.clone());
        return (line, next);
    }
    match options.into_iter().next() {
        None => (format!("先頭まで {} 秒", to_fixed(gap, 2)), "次は仕掛けを少し早くする".to_string()),
        Some(x) => (x.1, x.2),
    }
}

/// 勝ち方の称号: 走行データから言えるものだけ。コレクションになる（id, 名前, 取り方）
pub const BADGES: [(&str, &str, &str); 13] = [
    ("nige", "逃げ切り", "先頭を長く守って 1 着"),
    ("sashi", "差し切り", "最後の直線に 4 番手以下で入って 1 着"),
    ("ogai", "大外一気", "最後の 300m を外で走って 1 着"),
    ("uchi", "内ラチ一気", "最後の 300m を内で走って 1 着"),
    ("gyakuten", "大逆転", "最後の直線に 6 番手以下で入って 3 着以内"),
    ("hana", "ハナ差", "ハナ差で勝つ"),
    ("pittari", "使い切り", "スタミナを残さず 1 着"),
    ("slip", "風よけ名人", "風よけからの抜け出しを 6 回"),
    ("nomiss", "ノーミス", "障害コースをミスなく走る"),
    ("tobi", "ひとつも譲らず", "一度も前を詰まらせず 1 着"),
    ("ibuki", "いぶきの一撃", "息をためた一撃を当てる"),
    ("hirari", "ひらり", "攻撃やわなを 3 回跳んでよける"),
    ("sodachi", "育ちざかり", "1 レースでマークを 6 個取る"),
];
pub fn badge(id: &str) -> Option<&'static (&'static str, &'static str, &'static str)> { BADGES.iter().find(|b| b.0 == id) }

pub fn earned_badges(r: &Runner, race: &Race) -> Vec<&'static str> {
    let mut out = Vec::new();
    let (w, s) = (race.track.width, &r.stats);
    let late_lane = if s.late_lane_n != 0 { s.late_lane_sum / s.late_lane_n as f64 } else { w / 2.0 };
    let win = r.place == 1;
    let second = race.runners.iter().find(|x| x.place == 2).unwrap();
    if win && s.led_for > r.finish_t * 0.6 { out.push("nige"); }
    if win && r.rank_at_final >= 4 { out.push("sashi"); }
    if win && late_lane > w * 0.55 { out.push("ogai"); }
    if win && late_lane < 2.2 { out.push("uchi"); }
    if r.place <= 3 && r.rank_at_final >= 6 { out.push("gyakuten"); }
    if win && margin_text(second.finish_t - r.finish_t) == "ハナ" { out.push("hana"); }
    if win && s.final_stamina < 6.0 { out.push("pittari"); }
    if s.slings >= 6 { out.push("slip"); }
    if !race.track.obstacles.is_empty() && s.hurdles == 0 && s.wet < 0.2 { out.push("nomiss"); }
    if win && s.blocked < 0.5 { out.push("tobi"); }
    if s.charged_hits >= 1 { out.push("ibuki"); }
    if s.dodges >= 3 { out.push("hirari"); }
    if s.marks >= 6 { out.push("sodachi"); }
    out
}

/// コースごとのメダルのタイム（上手いボットの分布で決めた: 金は最速のあたり、銀は上位 4 割、銅は 9 割）。
/// 2026-11-12: マークと武器を足したので測り直した。全員 CPU（ふつう）の 1 着のタイムが、マークと武器の無いときからどれだけ動いたか
///（最速・上位 4 割・9 割。`cargo test -p sim --test items report -- --ignored --nocapture`）を、前の値に足した。運の幅が広がり、金は厳しく、銅は緩くなった
pub fn medal_times(course: &str) -> Option<[f64; 3]> {
    match course {
        "turf" => Some([90.85, 91.75, 92.7]),
        "water" => Some([90.8, 91.55, 92.85]),
        "hill" => Some([102.0, 102.75, 103.55]),
        _ => None,
    }
}
pub fn medal_of(course: &str, time: f64) -> i32 {
    let Some(m) = medal_times(course) else { return 0 };
    if time <= m[0] { 3 } else if time <= m[1] { 2 } else if time <= m[2] { 1 } else { 0 }
}
