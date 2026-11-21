// 画面の項目の計算のうち、このゲームだけの物（ブラウザが無くても試験できる所。2026-11-11）。画面（rust/game の app.rs・ui.rs・pause.rs）が使う。
// 一時停止に出す項目（キー設定を出すか）・札を指で送る・安全域の中の配置（Ⅱ・指のボタン・スティック）。
// どのゲームでも同じ物（最後に触った機器と輪・項目を移る・結果の決定の待ち・?safe=）は共通部品 game_ui::menu（game-ui 0.5）へ移した。

/// 一時停止の「キー設定」の番号（app.rs の pause_run）
pub const PAUSE_KEYS: usize = 5;
/// 一時停止に「キー設定」を出すか。指の間は出さない。キーかパッドに触ったら出し、その一時停止の間は消さない
/// （キーを押して出した入口を、指で押そうとした瞬間に消さない）。was は今まで出していたか（一時停止を開いたときは false）、touch は最後に触った機器が指か
pub fn keys_entry(was: bool, touch: bool) -> bool { was || !touch }
/// 一時停止に出す項目（pause_run の番号）
pub fn pause_ids(keys_entry: bool) -> Vec<usize> {
    (0..8).filter(|i| *i != PAUSE_KEYS || keys_entry).collect()
}

/// 指がこれだけ動いたら送り（押したことにしない）
pub const DRAG_START: f32 = 8.0;

/// 札の中身の送り（縦か横の 1 方向。at は送った量 px）
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Scroll {
    pub at: f32,
    /// 指で押した所・そのときの at・送りになったか
    drag: Option<(f32, f32, bool)>,
}
impl Scroll {
    /// 指が中身の上で触れた（pos は送る向きの座標）
    pub fn press(&mut self, pos: f32) {
        self.drag = Some((pos, self.at, false));
    }
    /// 指が動いた。送りになった瞬間だけ true（押したことを取り消す）
    pub fn moved(&mut self, pos: f32) -> bool {
        let Some((p0, s0, moved)) = self.drag.as_mut() else { return false };
        let d = pos - *p0;
        let started = !*moved && d.abs() > DRAG_START;
        if started { *moved = true; }
        if *moved { self.at = *s0 - d; }
        started
    }
    pub fn release(&mut self) { self.drag = None; }
    /// ホイール（d は中身を進める向きが正）
    pub fn wheel(&mut self, d: f32) {
        self.at += d;
    }
    /// 0..max に収める
    pub fn clamp(&mut self, max: f32) {
        self.at = self.at.clamp(0.0, max.max(0.0));
    }
    /// 続きがあるか（前, 後ろ）
    pub fn more(&self, max: f32) -> (bool, bool) {
        (self.at > 0.5, self.at < max - 0.5)
    }
}

/// 箱 [x, y, 幅, 高さ]
pub type Box4 = [f32; 4];

/// Ⅱ の見た目の丸の直径と、押せる箱の一辺
pub const PAUSE_DOT: f32 = 44.0;
pub const PAUSE_HIT: f32 = 56.0;
/// Ⅱ（押せる箱, 丸の中心）。右上、安全域の内側
pub fn pause_box(w: f32, safe: [f32; 4]) -> (Box4, [f32; 2]) {
    let c = [w - 14.0 - safe[1] - PAUSE_DOT * 0.5, 14.0 + safe[0] + PAUSE_DOT * 0.5];
    ([c[0] - PAUSE_HIT * 0.5, c[1] - PAUSE_HIT * 0.5, PAUSE_HIT, PAUSE_HIT], c)
}
/// 指のボタン（ダッシュ, ジャンプ）。右下、安全域の内側。低い画面（高さ 520 以下）は小さく
pub fn touch_buttons(w: f32, h: f32, safe: [f32; 4]) -> (Box4, Box4) {
    let (jw, dw) = if h <= 520.0 { (70.0, 86.0) } else { (86.0, 104.0) };
    let bottom = h - 22.0 - safe[2];
    let right = w - 18.0 - safe[1];
    let jump = [right - jw, bottom - jw, jw, jw];
    ([jump[0] - 12.0 - dw, bottom - dw, dw, dw], jump)
}
/// 指の「こうげき」（武器を持っている間だけ出す）。「ダッシュ」と「ジャンプ」の間の上
pub fn touch_attack(w: f32, h: f32, safe: [f32; 4]) -> Box4 {
    let (d, j) = touch_buttons(w, h, safe);
    let a = if h <= 520.0 { 62.0 } else { 76.0 };
    [(d[0] + d[2] + j[0]) * 0.5 - a * 0.5 + 6.0, j[1].min(d[1]) - a - 6.0, a, a]
}
/// 指のスティックを始められる場所（画面の左半分の、安全域の内側）
pub fn stick_area(w: f32, h: f32, safe: [f32; 4]) -> Box4 {
    [safe[3], safe[0], (w * 0.5 - safe[3]).max(0.0), (h - safe[0] - safe[2]).max(0.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_settings_hide_on_touch_only() {
        // 指で開いた一時停止には出さない
        let e = keys_entry(false, true);
        assert_eq!(pause_ids(e), vec![0, 1, 2, 3, 4, 6, 7]);
        // キーを押すと出る。そのあと指で触っても、その一時停止の間は消えない
        let e = keys_entry(e, false);
        assert_eq!(pause_ids(e), vec![0, 1, 2, 3, 4, 5, 6, 7]);
        let e = keys_entry(e, true);
        assert_eq!(pause_ids(e), vec![0, 1, 2, 3, 4, 5, 6, 7]);
        assert!(keys_entry(false, false));
    }

    #[test]
    fn finger_drag_scrolls_after_8px() {
        let mut s = Scroll::default();
        s.press(300.0);
        // 少し動いただけの指は押したまま
        assert!(!s.moved(295.0));
        assert_eq!(s.at, 0.0);
        // 上へ 20px なぞると、中身が 20px 進む。送りになった瞬間だけ true
        assert!(s.moved(280.0));
        assert_eq!(s.at, 20.0);
        assert!(!s.moved(250.0));
        assert_eq!(s.at, 50.0);
        // 端を越えない
        s.clamp(40.0);
        assert_eq!(s.at, 40.0);
        s.release();
        assert!(!s.moved(100.0));
        assert_eq!(s.at, 40.0);
        // 下へなぞると戻る
        s.press(100.0);
        s.moved(130.0);
        assert_eq!(s.at, 10.0);
        s.moved(400.0);
        s.clamp(40.0);
        assert_eq!(s.at, 0.0);
    }

    #[test]
    fn wheel_and_more() {
        let mut s = Scroll::default();
        assert_eq!(s.more(0.0), (false, false));
        assert_eq!(s.more(100.0), (false, true));
        s.wheel(60.0);
        s.clamp(100.0);
        assert_eq!(s.more(100.0), (true, true));
        s.wheel(60.0);
        s.clamp(100.0);
        assert_eq!(s.at, 100.0);
        assert_eq!(s.more(100.0), (true, false));
    }

    fn inside(b: Box4, w: f32, h: f32, safe: [f32; 4]) -> bool {
        b[0] >= safe[3] && b[1] >= safe[0] && b[0] + b[2] <= w - safe[1] && b[1] + b[3] <= h - safe[2]
    }
    fn overlap(a: Box4, b: Box4) -> bool { a[0] < b[0] + b[2] && b[0] < a[0] + a[2] && a[1] < b[1] + b[3] && b[1] < a[1] + a[3] }

    #[test]
    fn touch_parts_stay_inside_the_safe_area() {
        // 横持ちの iPhone・縦持ちの iPhone・安全域なし
        for (w, h, safe) in [(844.0, 390.0, [0.0, 47.0, 21.0, 47.0]), (390.0, 844.0, [47.0, 0.0, 34.0, 0.0]), (1280.0, 720.0, [0.0; 4])] {
            let (hit, c) = pause_box(w, safe);
            assert!(hit[2] >= 48.0 && hit[3] >= 48.0, "Ⅱ の箱は 48px 以上");
            assert!(inside(hit, w, h, safe), "Ⅱ {hit:?} が安全域の外（{w}x{h}）");
            // 見た目の丸は箱の真ん中
            assert_eq!(c, [hit[0] + hit[2] * 0.5, hit[1] + hit[3] * 0.5]);
            let (dash, jump) = touch_buttons(w, h, safe);
            assert!(inside(dash, w, h, safe) && inside(jump, w, h, safe), "指のボタンが安全域の外（{w}x{h}）");
            assert!(!overlap(dash, jump) && !overlap(dash, hit) && !overlap(jump, hit));
            assert!(dash[2] >= 48.0 && jump[2] >= 48.0);
            // 「こうげき」（武器を持っている間だけ）: 安全域の中で、ほかのボタン・Ⅱ・スティックの場所に重ならない
            let atk = touch_attack(w, h, safe);
            assert!(inside(atk, w, h, safe), "こうげき {atk:?} が安全域の外（{w}x{h}）");
            assert!(!overlap(atk, dash) && !overlap(atk, jump) && !overlap(atk, hit) && atk[2] >= 48.0);
            assert!(atk[0] >= w * 0.5, "こうげきは右半分（スティックの場所の外）");
            let st = stick_area(w, h, safe);
            assert!(inside(st, w, h, safe), "スティックの場所が安全域の外（{w}x{h}）");
            assert!(st[0] + st[2] <= w * 0.5 + 0.01, "スティックは左半分");
        }
        // 安全域なしの場所は今までと同じ（rust/compare/touch.py の式）
        let (dash, jump) = touch_buttons(1180.0, 820.0, [0.0; 4]);
        assert_eq!(jump, [1180.0 - 18.0 - 86.0, 820.0 - 22.0 - 86.0, 86.0, 86.0]);
        assert_eq!(dash, [jump[0] - 12.0 - 104.0, 820.0 - 22.0 - 104.0, 104.0, 104.0]);
        let (_, c) = pause_box(1280.0, [0.0; 4]);
        assert_eq!(c, [1280.0 - 14.0 - 22.0, 14.0 + 22.0]);
    }
}
