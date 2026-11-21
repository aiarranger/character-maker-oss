// 画面の項目の計算のうち、どのゲームでも同じ所（0.5。2026-11-11: ひな形と各ゲームが別々に持っていた物）。
// 最後に触った機器と輪・項目を移る・結果の決定の待ち・確かめ用の端の安全域（?safe=）。ブラウザにも描画にも依らない。

/// このコマに使った物
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Used { Key, Pad, Mouse, Touch }

/// 操作の説明の表記の種類（ゲームの割り当て表の機器へ読み替える）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind { Keys, Pad, Touch }

/// 最後に触った機器（表記）と、選んでいる項目の輪を出すか（輪はキーかパッドで触った間だけ）。
/// 同じコマに複数触ったときは、used を呼んだ順の最後が勝つ（どれを勝たせるかはゲームが呼ぶ順で決める）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen { pub kind: Kind, pub ring: bool }
impl Seen {
    pub fn new(kind: Kind) -> Seen { Seen { kind, ring: false } }
    pub fn used(&mut self, u: Used) {
        *self = match u {
            Used::Key => Seen { kind: Kind::Keys, ring: true },
            Used::Pad => Seen { kind: Kind::Pad, ring: true },
            // マウスはキーの表記のまま、輪は消す
            Used::Mouse => Seen { kind: Kind::Keys, ring: false },
            Used::Touch => Seen { kind: Kind::Touch, ring: false },
        };
    }
}

/// 項目を移る（prev で前、next で次。端はつながる。両方なら動かない）。項目が減ったあとは端に収める
pub fn step(focus: usize, n: usize, prev: bool, next: bool) -> usize {
    if n == 0 { return 0; }
    let f = focus.min(n - 1);
    match (prev, next) { (true, false) => (f + n - 1) % n, (false, true) => (f + 1) % n, _ => f }
}

/// 結果が出てから決定を受けるまでの秒（遊びのボタンの連打・押しっぱなしで飛ばさない）。もっと長く待つゲームは自分の秒を使う
pub const RESULT_WAIT: f64 = 0.6;
pub fn result_accepts(shown_for: f64) -> bool { shown_for >= RESULT_WAIT }

/// 確かめ用の端の安全域「上,右,下,左」（論理 px）。4 つの 0 以上の数でなければ None
pub fn parse_safe(s: &str) -> Option<[f32; 4]> {
    let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse::<f32>().ok()).filter(|x| x.is_finite() && *x >= 0.0).collect();
    if v.len() == 4 && s.split(',').count() == 4 { Some([v[0], v[1], v[2], v[3]]) } else { None }
}
/// URL の ? 以降（location.search）から ?safe=上,右,下,左 を読む（%2C も区切りとして読む）
pub fn safe_from_query(search: &str) -> Option<[f32; 4]> {
    search.trim_start_matches('?').split('&').find_map(|kv| kv.strip_prefix("safe=")).and_then(|v| parse_safe(&v.replace("%2C", ",").replace("%2c", ",")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_used_decides_labels_and_ring() {
        let mut s = Seen::new(Kind::Keys);
        assert!(!s.ring);
        s.used(Used::Touch);
        assert_eq!(s, Seen { kind: Kind::Touch, ring: false });
        s.used(Used::Key);
        assert_eq!(s, Seen { kind: Kind::Keys, ring: true });
        s.used(Used::Mouse);
        assert_eq!(s, Seen { kind: Kind::Keys, ring: false });
        s.used(Used::Pad);
        assert_eq!(s, Seen { kind: Kind::Pad, ring: true });
        s.used(Used::Touch);
        assert_eq!(s, Seen { kind: Kind::Touch, ring: false });
    }

    #[test]
    fn step_wraps_and_clamps() {
        assert_eq!(step(0, 4, false, true), 1);
        assert_eq!(step(3, 4, false, true), 0);
        assert_eq!(step(0, 4, true, false), 3);
        assert_eq!(step(2, 4, false, false), 2);
        assert_eq!(step(2, 4, true, true), 2);
        assert_eq!(step(9, 2, false, false), 1);
        assert_eq!(step(0, 0, false, true), 0);
    }

    #[test]
    fn result_waits() {
        assert!(!result_accepts(0.0) && !result_accepts(0.59));
        assert!(result_accepts(0.6) && result_accepts(3.0));
    }

    #[test]
    fn safe_override() {
        assert_eq!(parse_safe("0,47,21,47"), Some([0.0, 47.0, 21.0, 47.0]));
        assert_eq!(parse_safe(" 47, 0, 34, 0 "), Some([47.0, 0.0, 34.0, 0.0]));
        for bad in ["47,0,34", "a,b,c,d", "-1,0,0,0", "", "1,2,3,4,5", "1,,2,3"] { assert_eq!(parse_safe(bad), None, "{bad}"); }
        // URL の ? 以降をそのまま渡す形
        assert_eq!(safe_from_query("?safe=0,47,21,47"), Some([0.0, 47.0, 21.0, 47.0]));
        assert_eq!(safe_from_query("?hero=aichan&safe=47%2C0%2c34,0&x=1"), Some([47.0, 0.0, 34.0, 0.0]));
        assert_eq!(safe_from_query("safe=1,2,3,4"), Some([1.0, 2.0, 3.0, 4.0]));
        for none in ["", "?", "?unsafe=1,2,3,4", "?safe=", "?safe=1,2,3", "?xsafe=1,2,3,4"] { assert_eq!(safe_from_query(none), None, "{none}"); }
    }
}
