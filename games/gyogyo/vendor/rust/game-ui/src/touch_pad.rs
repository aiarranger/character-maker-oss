// 指のスティックとボタン（ゲームにも DOM にも依らない共通部品。2026-11-10 iceclimber の rust/game/src/touch_pad.rs（commit c9a7cd3）から取り込み）。
// 今の版の各ゲームの input.ts の #touch と同じ決まり: 指が始まった所がボタンならそのボタン、スティックの場所でスティックが空いていればスティック、ほかは「叩いた」。
// 場所と大きさ・ボタンの一覧はゲームが毎コマ渡す（set_layout）。座標は論理 px（CSS px）。描くのはゲーム（view() の値を Pen で）。
use egui::{Pos2, Rect, Vec2, vec2};

/// ボタンの形
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape { Circle { c: Pos2, r: f32 }, Rect(Rect) }
impl Shape {
    pub fn contains(&self, p: Pos2) -> bool { match *self { Shape::Circle { c, r } => c.distance(p) <= r, Shape::Rect(r) => r.contains(p) } }
    /// 縁までの距離（中は 0）
    pub fn distance(&self, p: Pos2) -> f32 { match *self { Shape::Circle { c, r } => (c.distance(p) - r).max(0.0), Shape::Rect(r) => r.distance_to_pos(p) } }
}

#[derive(Clone, Copy, Debug)]
pub struct Button { pub name: &'static str, pub shape: Shape }

/// スティックを始められる場所と、倒しきる距離（px）。four_way は縦か横の一方だけ
#[derive(Clone, Copy, Debug)]
pub struct Stick { pub area: Rect, pub radius: f32, pub four_way: bool }

/// 1 コマ分の値（read で取り出す。押した・離した瞬間はそこで消える）
#[derive(Clone, Debug, Default)]
pub struct PadFrame {
    /// スティック（-1..1。y は上が正）
    pub x: f32, pub y: f32,
    pub down: Vec<&'static str>,
    pub pressed: Vec<&'static str>,
    pub released: Vec<&'static str>,
    /// ボタンにもスティックにも当たらなかった指（今の版の tapP）。set_drag(true) のときは、引く指が空いていて引く指になった指だけ
    pub tap: bool,
    /// 引く指（ボタンにもスティックにも当たらなかった最初の 1 本）の動き（px、前の read から。set_drag(true) のときだけ。そらつぎの右の指でカメラを回す camX・camY）
    pub drag: Vec2,
}
impl PadFrame {
    pub fn is_down(&self, n: &str) -> bool { self.down.contains(&n) }
    pub fn was_pressed(&self, n: &str) -> bool { self.pressed.contains(&n) }
    pub fn was_released(&self, n: &str) -> bool { self.released.contains(&n) }
}

/// 描くための今の様子
#[derive(Clone, Debug, Default)]
pub struct PadView {
    /// スティックの中心（始めた場所。倒しきった先へ指が進むと付いてくる。離すと始めた場所へ戻って残る）
    pub stick_at: Option<Pos2>,
    pub stick_on: bool,
    /// ノブのずれ（px）
    pub knob: Vec2,
    /// 押されているボタン
    pub on: Vec<&'static str>,
}

pub struct TouchPad {
    buttons: Vec<Button>,
    stick: Option<Stick>,
    /// ボタンごとに押している指
    held: Vec<(&'static str, u64)>,
    pressed: Vec<&'static str>,
    released: Vec<&'static str>,
    stick_id: Option<u64>,
    origin: Pos2,
    /// この指が触り始めた場所（離したら土台をここへ戻す。付いていった先に残すと、ボタンや主役の上に残る）
    home: Pos2,
    value: Vec2,
    knob: Vec2,
    stick_at: Option<Pos2>,
    tap: bool,
    drag_on: bool,
    drag_id: Option<u64>,
    drag_last: Pos2,
    drag: Vec2,
    follow: bool,
    slop: f32,
}
impl Default for TouchPad {
    fn default() -> Self {
        TouchPad {
            buttons: vec![], stick: None, held: vec![], pressed: vec![], released: vec![], stick_id: None, origin: Pos2::ZERO, home: Pos2::ZERO, value: Vec2::ZERO, knob: Vec2::ZERO,
            stick_at: None, tap: false, drag_on: false, drag_id: None, drag_last: Pos2::ZERO, drag: Vec2::ZERO, follow: true, slop: 10.0,
        }
    }
}

impl TouchPad {
    pub fn new() -> Self { Self::default() }
    /// ボタンとスティックの場所（窓の大きさが変わるので毎コマ渡してよい）
    pub fn set_layout(&mut self, buttons: Vec<Button>, stick: Option<Stick>) { self.buttons = buttons; self.stick = stick; }
    /// ボタンにもスティックにも当たらなかった最初の指を「引く指」にする（今の版の camId。叩いた印はその指だけに立つ）
    pub fn set_drag(&mut self, on: bool) { self.drag_on = on; }
    /// 倒しきった先へ指が進んだとき、スティックの中心が指に付いてくるか（既定は付いてくる。戻すとすぐ値が減り、画面の端で始めても倒しきれる）
    pub fn set_follow(&mut self, on: bool) { self.follow = on; }
    /// ボタンの外側の、押したことにする余白（px。既定 10）。見た目ぴったりだと、指では縁を押し損ねる
    pub fn set_slop(&mut self, px: f32) { self.slop = px.max(0.0); }

    /// 指が触れた。ボタン → スティック → 叩いた の順に受ける
    pub fn start(&mut self, id: u64, p: Pos2) {
        // 中に入っているボタン。無ければ、余白の中でいちばん近いボタン
        let near = || self.buttons.iter().map(|b| (b, b.shape.distance(p))).filter(|(_, d)| *d <= self.slop).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(b, _)| b);
        if let Some(b) = self.buttons.iter().find(|b| b.shape.contains(p)).or_else(near) {
            self.held.push((b.name, id));
            if !self.pressed.contains(&b.name) { self.pressed.push(b.name); }
            return;
        }
        if let Some(s) = self.stick && self.stick_id.is_none() && s.area.contains(p) {
            self.stick_id = Some(id);
            self.origin = p;
            self.home = p;
            self.stick_at = Some(p);
            return;
        }
        if self.drag_on {
            if self.drag_id.is_none() { self.drag_id = Some(id); self.drag_last = p; self.tap = true; }
            return;
        }
        self.tap = true;
    }
    pub fn moved(&mut self, id: u64, p: Pos2) {
        if self.drag_id == Some(id) { self.drag += p - self.drag_last; self.drag_last = p; return; }
        if self.stick_id != Some(id) { return; }
        let Some(s) = self.stick else { return };
        let raw = p - self.origin;
        let mut d = raw;
        if s.four_way { if d.x.abs() > d.y.abs() { d.y = 0.0; } else { d.x = 0.0; } }
        let len = d.length();
        if len > s.radius { d *= s.radius / len; }
        self.knob = d;
        self.value = vec2(d.x / s.radius, -d.y / s.radius);
        // 中心を、指から倒しきる距離の所まで寄せる
        let far = raw.length();
        if self.follow && far > s.radius {
            self.origin = p - raw * (s.radius / far);
            self.stick_at = Some(self.origin);
        }
    }
    /// 離した・取り消された
    pub fn end(&mut self, id: u64) {
        if self.stick_id == Some(id) { self.stick_id = None; self.value = Vec2::ZERO; self.knob = Vec2::ZERO; self.stick_at = Some(self.home); }
        if self.drag_id == Some(id) { self.drag_id = None; }
        let mut i = 0;
        while i < self.held.len() {
            if self.held[i].1 == id {
                let (n, _) = self.held.remove(i);
                if !self.held.iter().any(|h| h.0 == n) && !self.released.contains(&n) { self.released.push(n); }
            } else { i += 1; }
        }
    }
    /// このコマの値を返し、押した・離した瞬間と叩いた印を消す（毎コマ 1 回）
    pub fn read(&mut self) -> PadFrame {
        let mut down: Vec<&'static str> = vec![];
        for (n, _) in &self.held { if !down.contains(n) { down.push(*n); } }
        PadFrame { x: self.value.x, y: self.value.y, down, pressed: std::mem::take(&mut self.pressed), released: std::mem::take(&mut self.released), tap: std::mem::take(&mut self.tap), drag: std::mem::take(&mut self.drag) }
    }
    pub fn view(&self) -> PadView {
        let mut on: Vec<&'static str> = vec![];
        for (n, _) in &self.held { if !on.contains(n) { on.push(*n); } }
        PadView { stick_at: self.stick_at, stick_on: self.stick_id.is_some(), knob: self.knob, on }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;
    fn pad() -> TouchPad {
        let mut p = TouchPad::new();
        p.set_layout(vec![Button { name: "jump", shape: Shape::Circle { c: pos2(900.0, 600.0), r: 48.0 } }], Some(Stick { area: Rect::from_min_max(pos2(0.0, 0.0), pos2(500.0, 700.0)), radius: 58.0, four_way: false }));
        p
    }
    #[test]
    fn button_stick_tap() {
        let mut p = pad();
        p.start(1, pos2(900.0, 610.0));
        p.start(2, pos2(100.0, 500.0));
        p.moved(2, pos2(200.0, 500.0));
        p.start(3, pos2(700.0, 100.0));
        let f = p.read();
        assert!(f.is_down("jump") && f.was_pressed("jump") && f.tap);
        assert_eq!((f.x, f.y), (1.0, 0.0));
        let f = p.read();
        assert!(f.is_down("jump") && !f.was_pressed("jump") && !f.tap);
        p.end(1); p.end(2);
        let f = p.read();
        assert!(!f.is_down("jump") && f.was_released("jump") && f.x == 0.0);
        // スティックは 1 本だけ。2 本目の左の指は叩いた扱い
        p.start(4, pos2(100.0, 500.0)); p.start(5, pos2(120.0, 500.0));
        assert!(p.read().tap && p.view().stick_on);
    }
    /// 倒しきった先へ指が進むと、スティックの中心が付いてくる（戻すとすぐ値が減る。画面の端で始めても倒しきれる）
    #[test]
    fn stick_follows_finger() {
        let mut p = pad();
        p.start(1, pos2(100.0, 500.0));
        p.moved(1, pos2(300.0, 500.0));
        assert_eq!((p.read().x, p.view().stick_at), (1.0, Some(pos2(242.0, 500.0))));
        p.moved(1, pos2(271.0, 500.0));
        assert_eq!(p.read().x, 0.5);
        // 離すと、土台は触り始めた所へ戻る（付いていった先に残さない）
        p.end(1);
        assert_eq!(p.view().stick_at, Some(pos2(100.0, 500.0)));
        // 画面の左端で始めて右へ: 中心は動かず、左へ引くと中心が左へ付いてくる
        p.start(2, pos2(4.0, 500.0));
        p.moved(2, pos2(62.0, 500.0));
        assert_eq!((p.read().x, p.view().stick_at), (1.0, Some(pos2(4.0, 500.0))));
        // 付いてこない設定（今までの動き）
        let mut p = pad();
        p.set_follow(false);
        p.start(1, pos2(100.0, 500.0));
        p.moved(1, pos2(300.0, 500.0));
        p.moved(1, pos2(271.0, 500.0));
        assert!(p.read().x > 0.999 && p.view().stick_at == Some(pos2(100.0, 500.0)));
    }
    /// ボタンの少し外（既定 10px）も押せる。2 つに近いときは近い方。外側の余白は 0 にもできる
    #[test]
    fn button_slop() {
        let two = |slop: Option<f32>| {
            let mut p = TouchPad::new();
            p.set_layout(vec![
                Button { name: "a", shape: Shape::Circle { c: pos2(800.0, 600.0), r: 40.0 } },
                Button { name: "b", shape: Shape::Rect(Rect::from_min_max(pos2(852.0, 560.0), pos2(932.0, 640.0))) },
            ], None);
            if let Some(s) = slop { p.set_slop(s); }
            p
        };
        let mut p = two(None);
        p.start(1, pos2(800.0, 551.0));
        let f = p.read();
        assert!(f.was_pressed("a") && !f.tap);
        p.start(2, pos2(800.0, 549.0));
        let f = p.read();
        assert!(f.tap && f.pressed.is_empty());
        // a の縁から 7px・b の縁から 5px → b
        p.start(3, pos2(847.0, 600.0));
        assert!(p.read().was_pressed("b"));
        // 重なって見える所は今までどおり先に書いた方（中に入っている方が勝つ）
        p.start(4, pos2(839.0, 600.0));
        assert!(p.read().was_pressed("a"));
        let mut p = two(Some(0.0));
        p.start(1, pos2(800.0, 551.0));
        assert!(p.read().tap);
    }
    #[test]
    fn drag_finger() {
        let mut p = pad();
        p.set_drag(true);
        p.start(1, pos2(700.0, 100.0));
        p.moved(1, pos2(710.0, 95.0));
        // 2 本目は引く指にも叩いた印にもならない（今の版の camId が埋まっているとき）
        p.start(2, pos2(800.0, 100.0));
        p.moved(2, pos2(900.0, 100.0));
        let f = p.read();
        assert!(f.tap && f.drag == vec2(10.0, -5.0));
        p.start(3, pos2(750.0, 100.0));
        assert!(!p.read().tap);
        p.end(1);
        p.start(4, pos2(760.0, 100.0));
        assert!(p.read().tap);
    }
}
