// 画面の文字・枠・ボタン（今の版の index.html・style.css・app.ts の DOM の部分）を、Rust が egui で描く（muso 式）。
// 描く道具（Pen・色・動き・文字）は characters の共通部品 game-ui（vendor/rust/game-ui）。ここは魚々娘（かけっこグランプリの写し）の画面と字体の表。
// 配色・大きさ・配置・アニメーションの数値は src/style.css と同じ。当たり判定も自前（DOM の click と同じく、押した物と離した物が同じなら押した）。
// 走る画面の表示は hud.rs、一時停止の札は pause.rs。
pub use game_ui::*;
use crate::hud::Hud;
use crate::pause::Pause;
use egui::{Context, Pos2, Rect, pos2, vec2};
use sim::menu::{self, Scroll};

// ---- 色（:root） ----
pub const CREAM: C = hx(0xfff6e4);
pub const CREAM_DIM: C = rgba(255, 246, 228, 0.72);
pub const GOLD: C = hx(0xffcf4d);
pub const GREEN: C = hx(0x0f3d2e);
pub const RED: C = hx(0xe6402f);
pub const GLASS: C = rgba(10, 30, 24, 0.66);
pub const LINE: C = rgba(255, 246, 228, 0.22);
pub const PAPER: C = hx(0xf8f1df);
pub const SHADOW_GREEN: C = hx(0xb8861d);
/// 走路の色（rgba(10, 30, 24, a)）
pub const fn ink(a: f32) -> C { rgba(10, 30, 24, a) }

// ---- 字体 ----
/// --disp: Dela Gothic One
pub const DISP: Fam = Fam::Custom("dela");
/// --jp: Zen Maru Gothic（400 は 500 の字で出る。今の版が 500・700・900 だけ読むため）
pub const JP5: Fam = Fam::Jp500;
pub const JP7: Fam = Fam::Jp700;
pub const JP9: Fam = Fam::Jp900;
/// --num: Fredoka（400 は 500）
pub const NUM5: Fam = Fam::Custom("fredoka500");
pub const NUM6: Fam = Fam::Custom("fredoka600");
pub const NUM7: Fam = Fam::Custom("fredoka700");

/// 役と同梱のファイル（OS の字体は使わない）
const FONTS: [FontRow; 7] = [
    (DISP, "DelaGothicOne-Regular.ttf", "", 0),
    (JP5, "ZenMaruGothic-Medium.ttf", "", 0),
    (JP7, "ZenMaruGothic-Bold.ttf", "", 0),
    (JP9, "ZenMaruGothic-Black.ttf", "", 0),
    (NUM5, "Fredoka-500.ttf", "", 0),
    (NUM6, "Fredoka-600.ttf", "", 0),
    (NUM7, "Fredoka-700.ttf", "", 0),
];
/// 無い字を先に探す役。和文は CSS の次の候補 Zen Maru Gothic の近い太さ（600 → 700）
fn prefer(f: Fam) -> Vec<Fam> {
    match f {
        DISP => vec![JP9, NUM7],
        JP5 => vec![NUM5, DISP],
        JP7 | JP9 => vec![NUM7, DISP],
        NUM5 => vec![JP5, DISP],
        _ => vec![JP7, DISP],
    }
}

/// 先に取っておく字体のファイル（ページからの相対）
pub fn font_files() -> Vec<String> { font_files_of(&FONTS) }

/// 字体を入れる。read はファイルの名前（fonts/…）→ 中身
pub fn install_fonts(ctx: &Context, read: &dyn Fn(&str) -> Option<Vec<u8>>) {
    install_fonts_with(ctx, &FONTS, &prefer, std::path::Path::new("fonts"), &|p| read(p.to_str()?));
}

pub fn is_num(f: Fam) -> bool { matches!(f, NUM5 | NUM6 | NUM7) }
/// Fredoka に無い字（和文・矢印・記号）。CSS は次の候補の字体で出し、行の高さもその字体の分だけ伸びる
pub fn cjk(s: &str) -> bool { s.chars().any(|c| (c as u32) >= 0x2190 && c != '\u{2212}') }
/// (ascent, descent)。line-height: normal の行の箱（無い字の字体も数える）
pub fn metrics(f: Fam, s: &str, size: f32) -> (f32, f32) {
    if is_num(f) && !cjk(s) { (0.974 * size, 0.236 * size) } else { (1.16 * size, 0.288 * size) }
}
/// line-height: normal の高さ
pub fn nlh(f: Fam, s: &str, size: f32) -> f32 { let (a, d) = metrics(f, s, size); a + d }
/// line-height: normal の行の箱の上 top から基線
pub fn base_n(f: Fam, s: &str, size: f32, top: f32) -> f32 { top + metrics(f, s, size).0 }
/// line-height を決めた行（L px）の基線。字体の分は最初の字体だけで決まる
pub fn base_l(f: Fam, size: f32, top: f32, l: f32) -> f32 {
    let (a, d) = if is_num(f) { (0.974 * size, 0.236 * size) } else { (1.16 * size, 0.288 * size) };
    top + (l - (a + d)) * 0.5 + a
}

// ---- 短い HTML（<b>・.kbd・<br>）。HUD の案内と、比べる道具に今の版と同じ文字を返すため ----
#[derive(Clone, Debug, PartialEq)]
pub enum Seg { T(String), B(String), K(String), Br }
pub fn rich(html: &str) -> Vec<Seg> {
    let mut out = vec![];
    let mut s = html;
    while !s.is_empty() {
        if let Some(r) = s.strip_prefix("<b>") {
            let e = r.find("</b>").unwrap_or(r.len());
            out.push(Seg::B(r[..e].to_string()));
            s = r.get(e + 4..).unwrap_or("");
        } else if let Some(r) = s.strip_prefix(r#"<span class="kbd">"#) {
            let e = r.find("</span>").unwrap_or(r.len());
            out.push(Seg::K(r[..e].to_string()));
            s = r.get(e + 7..).unwrap_or("");
        } else if let Some(r) = s.strip_prefix("<br>") {
            out.push(Seg::Br);
            s = r;
        } else {
            // 次の '<' まで（字の切れ目で切る。和文は 3 バイト）
            let e = s.char_indices().skip(1).find(|(_, c)| *c == '<').map_or(s.len(), |(i, _)| i);
            out.push(Seg::T(s[..e].to_string()));
            s = &s[e..];
        }
    }
    out
}
/// textContent
pub fn plain(html: &str) -> String { rich(html).iter().map(|s| match s { Seg::T(t) | Seg::B(t) | Seg::K(t) => t.as_str(), Seg::Br => "" }).collect() }

// ---- 当たり判定 ----

/// 押せる物（app が受け取る）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Start, Records, RecClose, Go, Again, Menu, Pause,
    /// 読み込みの失敗の「読み込み直す」・タイトルの音の切り替え
    Reload, Mute,
    /// 出走登録: 単発（false）・カップ（true）
    Mode(bool), Course(usize), Diff(usize), Hero(usize),
    /// 出走登録: 育成を選ぶ・育成を最初からやり直す
    Career, CareerNew,
    /// みんなで走る: このゲームには入れない。かけっこグランプリ（ONLINE_URL）を開く
    Online,
    /// 一時停止の札（pause_labels の順）・キー設定の札
    PauseBtn(usize), Key(usize),
    /// 押せないが下へ通さない面（.screen.show など）
    Block,
}

/// 押せる面の形
#[derive(Clone, Copy, Debug)]
pub enum Shape { Rect, Round(f32) }
#[derive(Clone, Copy, Debug)]
struct HitBox { act: Act, r: Rect, shape: Shape, clip: Rect }
impl HitBox {
    fn contains(&self, p: Pos2) -> bool {
        if !self.clip.contains(p) || !self.r.contains(p) { return false; }
        match self.shape {
            Shape::Rect => true,
            Shape::Round(rad) => {
                let rad = rad.min(self.r.width() * 0.5).min(self.r.height() * 0.5);
                let cx = p.x.clamp(self.r.left() + rad, self.r.right() - rad);
                let cy = p.y.clamp(self.r.top() + rad, self.r.bottom() - rad);
                p.distance(pos2(cx, cy)) <= rad
            }
        }
    }
}

fn top_in(list: &[HitBox], p: Pos2) -> Option<Act> { list.iter().rev().find(|h| h.contains(p)).map(|h| h.act).filter(|a| *a != Act::Block) }

/// 1 コマで描いた押せる面（描いた順 = 重なりの下から）。押す・離すの判定は前のコマの面で行う（今の版の画面に出ていた物を押す）
#[derive(Default)]
pub struct Hits {
    list: Vec<HitBox>,
    last: Vec<HitBox>,
    clip: Option<Rect>,
    /// 押した物（離したとき同じ物なら押した）
    down: Option<Act>,
    pub hover: Option<Act>,
    pub pressed: Option<Act>,
    pointer: Option<Pos2>,
}
impl Hits {
    pub fn add(&mut self, act: Act, r: Rect, shape: Shape) { let clip = self.clip.unwrap_or(Rect::EVERYTHING); self.list.push(HitBox { act, r, shape, clip }); }
    pub fn mark(&self) -> usize { self.list.len() }
    pub fn truncate(&mut self, n: usize) { self.list.truncate(n); }
    pub fn block(&mut self, r: Rect) { self.add(Act::Block, r, Shape::Rect); }
    fn top(&self, p: Pos2) -> Option<Act> { top_in(&self.last, p) }
    /// コマの始め（遊びの計算の前）: いま画面に出ている面（前のコマに描いた面）で入力を読み、押した物の列を返す。
    /// 今の版の click はコマの間に効くので、Rust 版もこのコマの計算の前に効かせる
    fn feed_one(&mut self, e: &egui::Event, acts: &mut Vec<Act>) {
        match e {
            egui::Event::PointerMoved(p) => self.pointer = Some(*p),
            egui::Event::PointerGone => self.pointer = None,
            egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, .. } => {
                if *pressed { self.down = top_in(&self.list, *pos); self.pressed = self.down; }
                else {
                    if let (Some(d), Some(u)) = (self.down, top_in(&self.list, *pos)) && d == u { acts.push(d); }
                    self.down = None; self.pressed = None;
                }
            }
            _ => {}
        }
    }
    /// 押したことを取り消す（指が札の中身を送り始めた。ブラウザの pointercancel）
    fn cancel_down(&mut self) { self.down = None; self.pressed = None; }
    /// 描く前: 前のコマの面をしまい、指・マウスが乗っている物を決める
    fn begin(&mut self) {
        self.last = std::mem::take(&mut self.list);
        self.hover = self.pointer.and_then(|p| self.top(p));
    }
    /// このコマに描いた面すべて（押せない面も。切る箱の中だけ）。dom_input の over_ui へ知らせる
    fn areas(&self) -> impl Iterator<Item = Rect> + '_ { self.list.iter().map(|h| h.r.intersect(h.clip)).filter(|r| r.is_positive()) }
    /// 確かめ用: 前のコマに描いた押せる面（押せない面は除く。重なりの上から）
    pub fn rects(&self) -> Vec<(Act, Rect, Rect)> { self.last.iter().rev().filter(|h| h.act != Act::Block).map(|h| (h.act, h.r, h.clip)).collect() }
    /// 今の指・マウスの場所が UI の面の上か（ホイールの行き先）
    pub fn pointer(&self) -> Option<Pos2> { self.pointer }
    pub fn is_hover(&self, a: Act) -> bool { self.hover == Some(a) }
    pub fn is_active(&self, a: Act) -> bool { self.pressed == Some(a) && self.hover == Some(a) }
}

// ---- 形 ----

/// 押せる面を切る箱（overflow の中）を替える。前の箱を返す
pub fn hits_clip(h: &mut Hits, c: Option<Rect>) -> Option<Rect> { std::mem::replace(&mut h.clip, c) }

/// 角の丸みが縦横で違う四角（scaleX をかけた角丸）
pub fn rr_xy(r: Rect, rx: f32, ry: f32) -> Vec<Pos2> {
    let rx = rx.min(r.width() * 0.5).max(0.0);
    let ry = ry.min(r.height() * 0.5).max(0.0);
    let seg = 4;
    let mut out = Vec::with_capacity((seg + 1) * 4);
    let cs = [(r.right() - rx, r.top() + ry, -90.0_f32), (r.right() - rx, r.bottom() - ry, 0.0), (r.left() + rx, r.bottom() - ry, 90.0), (r.left() + rx, r.top() + ry, 180.0)];
    for (cx, cy, start) in cs {
        for i in 0..=seg {
            let a = (start + 90.0 * i as f32 / seg as f32).to_radians();
            out.push(pos2(cx + a.cos() * rx, cy + a.sin() * ry));
        }
    }
    out
}

// ---- 文字の部品 ----

/// 文字を左上（行の箱の上）から（line-height: normal）
pub fn label(pen: &Pen, s: &str, f: Fam, size: f32, sp: f32, x: f32, top: f32, c: C, sh: &[TS]) -> f32 {
    let t = pen.t(s, f, size, sp);
    pen.text(&t, x, base_n(f, s, size, top), c, sh);
    t.w
}
pub fn tw(pen: &Pen, s: &str, f: Fam, size: f32, sp: f32) -> f32 { pen.t(s, f, size, sp).w }

/// .kbd（大きさは親の文字の em から）。color は currentColor
pub struct Kbd { t: T, label: String, fs: f32, pub w: f32, pub h: f32, pub mr: f32, help: bool }
impl Kbd {
    /// 普通の .kbd（min-width 1.9em・高さ 1.7em・左右 .45em・右の間 .3em。字は .72em）
    pub fn new(pen: &Pen, s: &str, parent: f32) -> Kbd {
        let fs = parent * 0.72;
        let t = pen.t(s, NUM6, fs, fs * 0.02);
        Kbd { w: (t.w + fs * 0.9 + 2.0).max(fs * 1.9), h: fs * 1.7, mr: fs * 0.3, t, label: s.to_string(), fs, help: false }
    }
    /// .help .kbd（.85em・高さは中身・上下 .2em 左右 .6em・間なし）
    pub fn help(pen: &Pen, s: &str, parent: f32) -> Kbd {
        let fs = parent * 0.85;
        let t = pen.t(s, NUM6, fs, fs * 0.02);
        let h = nlh(NUM6, s, fs) + fs * 0.4 + 3.5;
        Kbd { w: (t.w + fs * 1.2 + 2.0).max(fs * 1.9), h, mr: 0.0, t, label: s.to_string(), fs, help: true }
    }
    pub fn full_w(&self) -> f32 { self.w + self.mr }
    /// 箱の左 x、縦の中心 cy
    pub fn draw(&self, pen: &Pen, x: f32, cy: f32, color: C) {
        let p = pen.with_alpha(0.85);
        let r = Rect::from_min_size(pos2(x, cy - self.h * 0.5), vec2(self.w, self.h));
        let rad = self.fs * 0.4;
        p.rframe(r, rad, 1.0, color);
        p.line(&[pos2(r.left() + rad * 0.6, r.bottom() - 1.75), pos2(r.right() - rad * 0.6, r.bottom() - 1.75)], 1.5, color);
        // 中身の箱（上の線 1px・下の線 2.5px の内側）の中央
        let inner_top = r.top() + 1.0;
        let inner_h = self.h - 3.5;
        let lh = nlh(NUM6, &self.label, self.fs);
        let top = if self.help { inner_top + self.fs * 0.2 } else { inner_top + (inner_h - lh) * 0.5 };
        p.text(&self.t, r.center().x - self.t.w * 0.5 + self.fs * 0.01, base_n(NUM6, &self.label, self.fs, top), color, &[]);
    }
}

/// 1 行に並ぶ中身（文字と .kbd）
pub enum It { T(String, Fam, f32, f32, C), K(Kbd) }
pub fn it_w(pen: &Pen, items: &[It]) -> f32 {
    items.iter().map(|i| match i { It::T(s, f, z, sp, _) => tw(pen, s, *f, *z, *sp), It::K(k) => k.full_w() }).sum()
}
/// x は左端、cy は行の中心（文字は行の箱 line-height: normal の中央）、base は基線を決める字体と大きさ
pub fn it_draw(pen: &Pen, items: &[It], x: f32, cy: f32, kcolor: C, sh: &[TS]) {
    let mut x = x;
    for i in items {
        match i {
            It::T(s, f, z, sp, c) => {
                let top = cy - nlh(*f, s, *z) * 0.5;
                x += label(pen, s, *f, *z, *sp, x, top, *c, sh);
            }
            It::K(k) => { k.draw(pen, x, cy, kcolor); x += k.full_w(); }
        }
    }
}
/// 短い HTML の行（b は太く。.kbd は札）。weight は親の太さ
pub fn rich_items(pen: &Pen, segs: &[Seg], size: f32, f: Fam, c: C) -> Vec<It> {
    let bold = match f { JP5 => JP7, _ => JP9 };
    segs.iter().filter_map(|s| match s {
        Seg::T(t) => Some(It::T(t.clone(), f, size, 0.0, c)),
        Seg::B(t) => Some(It::T(t.clone(), bold, size, 0.0, c)),
        Seg::K(t) => Some(It::K(Kbd::new(pen, t, size))),
        Seg::Br => None,
    }).collect()
}

/// .eyebrow（Fredoka 600 13px・字間 .34em）。高さを返す
pub fn eyebrow(pen: &Pen, s: &str, x: f32, top: f32, c: C) -> f32 {
    label(pen, s, NUM6, 13.0, 13.0 * 0.34, x, top, c, &[]);
    nlh(NUM6, s, 13.0)
}
pub fn eyebrow_w(pen: &Pen, s: &str) -> f32 { tw(pen, s, NUM6, 13.0, 13.0 * 0.34) }

// ---- ボタン ----

/// .primary の大きさの決め（標準・低い画面）
#[derive(Clone, Copy)]
pub struct PrimarySize { pub pv: f32, pub ph: f32, pub fs: f32, pub sh: f32 }
pub const PRIMARY: PrimarySize = PrimarySize { pv: 15.0, ph: 34.0, fs: 18.0, sh: 5.0 };
/// 低い画面（横持ちのスマホ）でも指で押せる高さ（44px）
pub const PRIMARY_SHORT: PrimarySize = PrimarySize { pv: 12.0, ph: 20.0, fs: 14.0, sh: 3.0 };
fn primary_items(pen: &Pen, key: Option<&str>, s: &str, z: PrimarySize) -> Vec<It> {
    let mut v = vec![];
    if let Some(k) = key { v.push(It::K(Kbd::new(pen, k, z.fs))); }
    v.push(It::T(s.to_string(), JP9, z.fs, z.fs * 0.06, GREEN));
    v
}
pub fn primary_size(pen: &Pen, key: Option<&str>, s: &str, z: PrimarySize) -> (f32, f32) {
    (it_w(pen, &primary_items(pen, key, s, z)) + z.ph * 2.0, z.fs * 1.448 + z.pv * 2.0)
}
/// .primary を r に描く（hover で上がり、押している間は沈む）
pub fn primary(pen: &Pen, hits: &mut Hits, act: Act, r: Rect, key: Option<&str>, s: &str, z: PrimarySize, tw_: &mut Tw) {
    hits.add(act, r, Shape::Round(r.height() * 0.5));
    let target = if hits.is_active(act) { 2.0 } else if hits.is_hover(act) { 1.0 } else { 0.0 };
    let k = tw_.go(act, target, 0.15, bezier_spring);
    // translateY: 0 → -2（hover）→ 4（押す）。影は 5 → 7 → 1
    let (dy, sh) = if k <= 1.0 { (-2.0 * k, z.sh + 2.0 * k) } else { (lerp(-2.0, 4.0, k - 1.0), lerp(z.sh + 2.0, 1.0, k - 1.0)) };
    let p = pen.sub(Xf::tr(0.0, dy), 1.0);
    let rad = r.height() * 0.5;
    p.rrect(r.translate(vec2(0.0, sh)), rad, SHADOW_GREEN);
    p.rrect(r, rad, GOLD);
    let items = primary_items(pen, key, s, z);
    let w = it_w(pen, &items);
    it_draw(&p, &items, r.center().x - w * 0.5, r.center().y, GREEN, &[]);
}
/// .ghost（枠 2px、hover で枠がクリーム）
pub fn ghost(pen: &Pen, hits: &mut Hits, act: Act, r: Rect, s: &str, fs: f32, bg: Option<C>, border: C) {
    hits.add(act, r, Shape::Round(r.height() * 0.5));
    let rad = r.height() * 0.5;
    if let Some(b) = bg { pen.rrect(r, rad, b); }
    pen.rframe(r, rad, 2.0, if hits.is_hover(act) { CREAM } else { border });
    let t = pen.t(s, JP7, fs, 0.0);
    pen.text(&t, r.center().x - t.w * 0.5, base_n(JP7, s, fs, r.center().y - nlh(JP7, s, fs) * 0.5), CREAM, &[]);
}
pub fn ghost_size(pen: &Pen, s: &str, short: bool) -> (f32, f32) {
    let (pv, ph, fs) = if short { (11.0, 14.0, 12.0) } else { (13.0, 22.0, 14.0) };
    (tw(pen, s, JP7, fs, 0.0) + ph * 2.0 + 4.0, fs * 1.448 + pv * 2.0 + 4.0)
}
/// --spring
pub fn bezier_spring(t: f32) -> f32 { bezier(0.34, 1.56, 0.64, 1.0, t) }

/// ボタンの見た目の動き（transition）を物ごとに持つ
#[derive(Default)]
pub struct Tw { now: f64, map: Vec<(Act, Tween)> }
impl Tw {
    pub fn go(&mut self, a: Act, to: f32, dur: f32, f: fn(f32) -> f32) -> f32 {
        let now = self.now;
        let i = match self.map.iter().position(|x| x.0 == a) { Some(i) => i, None => { self.map.push((a, Tween::new(to, dur, 0.0, f))); self.map.len() - 1 } };
        self.map[i].1.set(to, now);
        self.map[i].1.get(now)
    }
}

// ---- 送れる箱 ----

/// 送れる箱（指のなぞりとホイールで送る。計算は sim::menu::Scroll）。view と max は描いたときの値
#[derive(Clone, Copy, Debug)]
pub struct Scroller { pub s: Scroll, pub view: Rect, pub max: f32, horiz: bool }
impl Scroller {
    pub fn new(horiz: bool) -> Scroller { Scroller { s: Scroll::default(), view: Rect::NOTHING, max: 0.0, horiz } }
    fn coord(&self, p: Pos2) -> f32 { if self.horiz { p.x } else { p.y } }
    fn can(&self) -> bool { self.max > 0.5 }
    /// 描くとき: 箱と送れる量を決め、収めた今の量を返す
    pub fn place(&mut self, view: Rect, max: f32) -> f32 { self.view = view; self.max = max.max(0.0); self.s.clamp(self.max); self.s.at }
    /// 描かなかったコマ（送れない）
    pub fn off(&mut self) { self.view = Rect::NOTHING; self.max = 0.0; }
    pub fn reset(&mut self) { self.s = Scroll::default(); }
    pub fn dump(&self) -> serde_json::Value { serde_json::json!({ "at": self.s.at, "max": self.max, "x": self.view.center().x, "y": self.view.center().y, "w": self.view.width().max(0.0), "h": self.view.height().max(0.0) }) }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sc { Rec, Pause, Res, Board, Strip }

/// 縦に送れる箱の合図: 続きがある側の端を bg へ薄くし、小さな山形を出す。右の端に今の場所の細い帯
pub fn more_cue(pen: &Pen, sc: &Scroller, bg: C, fg: C) {
    if !sc.can() { return; }
    let (v, at, max) = (sc.view, sc.s.at, sc.max);
    let (up, down) = sc.s.more(max);
    let clear = C { a: 0.0, ..bg };
    let chev = |cy: f32, dir: f32| {
        let cx = v.center().x;
        pen.line(&[pos2(cx - 8.0, cy - 3.0 * dir), pos2(cx, cy + 3.0 * dir), pos2(cx + 8.0, cy - 3.0 * dir)], 2.5, fg);
    };
    if up {
        pen.vgrad(Rect::from_min_size(v.min, vec2(v.width(), 26.0_f32.min(v.height() * 0.3))), &[(0.0, bg), (1.0, clear)]);
        chev(v.top() + 8.0, -1.0);
    }
    if down {
        let fh = 34.0_f32.min(v.height() * 0.3);
        pen.vgrad(Rect::from_min_size(pos2(v.left(), v.bottom() - fh), vec2(v.width(), fh)), &[(0.0, clear), (1.0, bg)]);
        chev(v.bottom() - 8.0, 1.0);
    }
    let track = v.height() - 12.0;
    let th = (track * v.height() / (v.height() + max)).max(24.0).min(track);
    let ty = v.top() + 6.0 + (track - th) * (at / max).clamp(0.0, 1.0);
    pen.rrect(Rect::from_min_size(pos2(v.right() - 5.0, ty), vec2(3.0, th)), 1.5, fg.alpha(0.4));
}

// ---- 画面 ----

/// みんなで走る（最大 4 人のオンライン）は、このゲームに入れない。同じ遊びを人の主役で遊べる、かけっこグランプリを開く（docs/gyogyo-spec.md）
pub const ONLINE_URL: &str = "https://kakekko.yukikau.jp/";
/// タイトルの札の字（外へ出る印 ↗）
pub const ONLINE_LABEL: &str = "かけっこグランプリでみんなで走る ↗";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scr { Loading, Title, Select, Intro, Result, Photo }
const SCRS: [Scr; 6] = [Scr::Loading, Scr::Title, Scr::Select, Scr::Intro, Scr::Result, Scr::Photo];
fn si(s: Scr) -> usize { SCRS.iter().position(|x| *x == s).unwrap() }

/// 出走登録のコースの札
#[derive(Clone)]
pub struct CourseBtn { pub name: String, pub blurb: String, pub best: Option<String>, pub on: bool }
/// キャラの札（.full は全身の絵）
#[derive(Clone)]
pub struct HeroBtn { pub tex: Option<Tex>, pub full: bool }
/// 育成の札（出走登録。次のレースと、育てた能力）
#[derive(Clone, Default)]
pub struct CareerCard {
    pub grade: String, pub race: String,
    /// 「第 3 戦 / 全 8 戦・はじまり芝コース」「相手: ふつう・能力 3 段」「2 戦 1 勝・賞金 800 万」
    pub sub: String, pub rivals: String, pub record: String,
    pub levels: [u8; 5],
    /// 途中経過がある（「最初から育て直す」を出す）と、そのボタンの字（1 回押すと確かめの字に変わる）
    pub running: bool, pub reset_label: String,
}
#[derive(Clone, Default)]
pub struct SelectData {
    pub cup_mode: bool, pub cup_label: String,
    /// 育成を選んでいるか・札の字・育成の札
    pub career_mode: bool, pub career_label: String, pub career: CareerCard,
    pub courses: Vec<CourseBtn>, pub diff: usize,
    pub hero_on: usize, pub name: String, pub best: String,
}

/// 結果の 1 行
#[derive(Clone)]
pub struct ResRow { pub me: bool, pub place: String, pub gate: Option<usize>, pub name: String, pub time: Option<String>, pub margin: String, pub pts: Option<String> }
#[derive(Clone, Default)]
pub struct Pill { pub text: String, pub medal: Option<i64>, pub new: bool }
#[derive(Clone, Default)]
pub struct ResultData {
    pub eyebrow: String, pub place: String, pub unit: String, pub win: bool,
    pub time: String, pub time_em: Option<(String, bool)>,
    /// 本当のタイム（秒。数え上げの終わり）
    pub time_s: f64,
    pub rival: String, pub line: String, pub next: String,
    pub pills: Vec<Pill>,
    /// 順位の流れ（0..1 の横、0..56 の縦）・通過順・次のメダル
    pub chart: Vec<(f32, f32)>, pub pass: String, pub next_medal: Option<String>,
    pub rows: Vec<ResRow>, pub cup: String, pub again: String,
    /// 2 つ目のボタンの字（空なら「コース・キャラを変える」。みんなで走るときは「部屋を出る」）
    pub menu: String,
}
#[derive(Clone, Default)]
pub struct RecCourse { pub name: String, pub best: String, pub medal: i64, pub medal_name: String }
#[derive(Clone, Default)]
pub struct RecBadge { pub name: String, pub how: String, pub got: bool }
#[derive(Clone, Default)]
pub struct RecHall { pub name: String, pub title: String, pub line: String, pub levels: [u8; 5] }
#[derive(Clone, Default)]
pub struct RecordsData { pub courses: Vec<RecCourse>, pub badges: Vec<RecBadge>, pub hall: Vec<RecHall>, pub note: String }

/// 指のボタンの見た目（入力の部品が持つ値を渡す）
#[derive(Clone, Copy, Debug, Default)]
pub struct TouchView { pub stick: Option<(f32, f32)>, pub stick_on: bool, pub knob: (f32, f32), pub dash: bool, pub jump: bool, pub attack: bool }

/// 枠の色と字の色（view/runner.rs の GATE_COLOR・GATE_TEXT と同じ）
pub const GATE_COLOR: [C; 8] = [hx(0xf7f3e8), hx(0x25262b), hx(0xe23b35), hx(0x2f6fd8), hx(0xf5cc2a), hx(0x2f9e5b), hx(0xf28a24), hx(0xf39ac0)];
pub const GATE_TEXT: [C; 8] = [hx(0x25262b), hx(0xf7f3e8), WHITE, WHITE, hx(0x25262b), WHITE, WHITE, hx(0x25262b)];

pub struct Screens {
    pub now: f64,
    t0: f64,
    /// body.notrans（比べる道具が付ける。動きを止める）
    pub notrans: bool,
    pub reduce_motion: bool,
    /// body.touch
    pub touch: bool,
    pub racing: bool,
    pub finishing: bool,
    pub paused: bool,
    /// 端の安全域（上・右・下・左。app が起動のときと窓の大きさが変わったときに読む）
    pub safe: [f32; 4],
    /// 画面の大きさ（描いたときの値。確かめ用の dump が指のボタンの場所を返す）
    size: (f32, f32),
    shown: Option<Scr>,
    fade: [Tween; 6],
    title_at: f64,
    // 読み込み
    load_fill: Tween,
    pub load_text: String,
    /// 読み込めなかった（知らせの文と「読み込み直す」を出す）
    pub load_failed: bool,
    // タイトル
    pub title_stats: String,
    /// 音を切っているか（タイトルの切り替えの字）
    pub muted: bool,
    /// 選んでいる項目の輪を出すか（最後に触ったのがキーかパッド）と、結果の画面で選んでいる項目（0 もう一度・1 コース・キャラを変える）
    pub ring: bool,
    pub res_focus: usize,
    // 出走登録
    pub sel: SelectData,
    pub heroes: Vec<HeroBtn>,
    strip: Scroller,
    strip_center: bool,
    // 発走前
    pub intro: [String; 3],
    // 写真判定
    pub photo: Option<Tex>,
    pub photo_res: String,
    photo_k: Tween,
    // 結果
    pub res: ResultData,
    res_at: f64,
    res_scroll: Scroller,
    board_scroll: Scroller,
    /// 自己ベストの祝い: 紙吹雪・数え上げ中のタイム（None は本当のタイムをそのまま出す）・数え終わった時刻・自己ベストか・動きを減らすか
    confetti: game_ui::cheer::Confetti, count: Option<f64>, cheer_at: Option<f64>, cheer_best: bool, calm: bool,
    /// 紙吹雪を出す欄（結果の左の欄。描いたときの場所）・紙吹雪が消える下の端と右の端（掲示板とボタンの上には出さない）・前のコマの時刻
    cheer_box: Rect, cheer_floor: f32, cheer_right: f32, cheer_t: f64,
    // きろく
    pub records: Option<RecordsData>,
    rec_data: RecordsData,
    rec_fade: Tween,
    rec_k: Tween,
    rec_scroll: Scroller,
    // 切り替えの幕
    wipe_on: Option<f64>,
    wipe_out: Option<f64>,
    pub touch_view: TouchView,
    pub hud: Hud,
    pub pause: Pause,
    pub hits: Hits,
    tw: Tw,
    hud_fade: Tween,
    pause_fade: Tween,
}

impl Screens {
    pub fn new(now: f64) -> Screens {
        let mut fade = [Tween::new(0.0, 0.45, 0.0, ease); 6];
        fade[0].jump(1.0);
        Screens {
            now, t0: now, notrans: false, reduce_motion: false, touch: false, racing: false, finishing: false, paused: false, safe: [0.0; 4], size: (0.0, 0.0),
            shown: Some(Scr::Loading), fade, title_at: now,
            load_fill: Tween::new(0.0, 0.3, 0.0, ease), load_text: "読み込んでいます…".into(), load_failed: false,
            title_stats: String::new(), muted: false, ring: false, res_focus: 0,
            sel: SelectData::default(), heroes: vec![], strip: Scroller::new(true), strip_center: false,
            intro: Default::default(),
            photo: None, photo_res: String::new(), photo_k: Tween::new(0.0, 0.5, 0.0, bezier_spring),
            // index.html の最初の中身
            res: ResultData { eyebrow: "RESULT".into(), unit: "着".into(), again: "もう一度".into(), ..Default::default() }, res_at: now, res_scroll: Scroller::new(false), board_scroll: Scroller::new(false),
            confetti: game_ui::cheer::Confetti::new(), count: None, cheer_at: None, cheer_best: false, calm: false, cheer_box: Rect::NOTHING, cheer_floor: 0.0, cheer_right: 0.0, cheer_t: now,
            records: None, rec_data: RecordsData::default(), rec_fade: Tween::new(0.0, 0.45, 0.0, ease), rec_k: Tween::new(0.0, 0.45, 0.0, bezier_spring), rec_scroll: Scroller::new(false),
            wipe_on: None, wipe_out: None,
            touch_view: TouchView::default(),
            hud: Hud::new(), pause: Pause::new(), hits: Hits::default(), tw: Tw::default(),
            hud_fade: Tween::new(0.0, 0.3, 0.0, css_ease), pause_fade: Tween::new(0.0, 0.45, 0.0, ease),
        }
    }
    /// コマの始めに時計を合わせる（秒）
    pub fn set_now(&mut self, now: f64) { self.now = now; self.hud.now = now; self.pause.now = now; self.tw.now = now; }
    /// TS の show(id)。None で全部隠す
    pub fn show(&mut self, s: Option<Scr>) {
        let now = self.now;
        if s == Some(Scr::Title) && self.shown != Some(Scr::Title) { self.title_at = now; }
        if s == Some(Scr::Photo) && self.shown != Some(Scr::Photo) { self.photo_k.set(1.0, now); }
        if s != Some(Scr::Photo) { self.photo_k.set(0.0, now); }
        self.shown = s;
        for (i, x) in SCRS.iter().enumerate() { self.fade[i].set(if Some(*x) == s { 1.0 } else { 0.0 }, now); }
    }
    pub fn progress(&mut self, k: f32, text: Option<&str>) {
        self.load_fill.set(k, self.now);
        if let Some(t) = text { self.load_text = t.to_string(); }
    }
    /// 出走登録を組み直す（今の版の buildSelect: 札を作り直し、選んだ札を帯の中央へ）
    pub fn set_select(&mut self, d: SelectData) { self.sel = d; self.strip_center = true; }
    pub fn set_result(&mut self, r: ResultData) {
        self.res = r; self.res_at = self.now; self.res_scroll.reset(); self.board_scroll.reset(); self.res_focus = 0;
        // タイムは 0 から数え上がる（app が cheer_count で渡す）
        self.confetti.clear();
        self.count = Some(0.0);
        self.cheer_at = None;
    }
    /// 動きを減らす設定（紙吹雪は控えめ・弾みなし）
    pub fn set_calm(&mut self, on: bool) { self.calm = on; self.confetti.set_calm(on); }
    /// 数え上げ中のタイム
    pub fn cheer_count(&mut self, v: f64) { if self.count.is_some() { self.count = Some(v); } }
    /// 数え終わった。自己ベストなら「自己ベスト！」が弾んで出て、左の欄の左右の下の角から紙吹雪が内向き・上向きに 1 回ずつ（欄の上の端まで）。そうでなければタイムが小さく弾む
    pub fn cheer_done(&mut self, best: bool) {
        self.cheer_at = Some(self.now);
        self.cheer_best = best;
        self.count = None;
        let b = self.cheer_box;
        if !best || !b.is_positive() { return; }
        let speed = game_ui::cheer::speed_to_rise(b.height() - 6.0);
        self.confetti.burst(pos2(b.left() + 6.0, b.bottom() - 6.0), -62.0, 44.0, 60, speed);
        self.confetti.burst(pos2(b.right() - 6.0, b.bottom() - 6.0), -118.0, 44.0, 60, speed);
    }
    /// いちばん上の評価の自己ベスト: 欄の上の中央からもう 1 回
    pub fn cheer_encore(&mut self) {
        let b = self.cheer_box;
        if b.is_positive() { self.confetti.burst(pos2(b.center().x, b.top() + 4.0), 90.0, 150.0, 60, 520.0); }
    }
    /// 先へ進んだ: 紙吹雪を消し、タイムは本当の値をそのまま出す
    pub fn cheer_clear(&mut self) {
        self.confetti.clear();
        self.count = None;
        self.cheer_at = None;
    }
    pub fn show_records(&mut self, r: RecordsData) { self.rec_data = r.clone(); self.records = Some(r); self.rec_fade.set(1.0, self.now); self.rec_k.set(1.0, self.now); self.rec_scroll.reset(); }
    /// 読み込めなかった: 知らせの文と「読み込み直す」
    pub fn fail(&mut self, text: &str) { self.load_failed = true; self.load_text = text.to_string(); self.show(Some(Scr::Loading)); }
    pub fn close_records(&mut self) { self.records = None; self.rec_fade.set(0.0, self.now); self.rec_k.set(0.0, self.now); }
    pub fn records_shown(&self) -> bool { self.records.is_some() }
    pub fn set_racing(&mut self, on: bool) { self.racing = on; self.hud_fade.set(if on { 1.0 } else { 0.0 }, self.now); }
    pub fn set_paused(&mut self, on: bool) { self.paused = on; self.pause_fade.set(if on { 1.0 } else { 0.0 }, self.now); }
    /// #wipe.on（幕が閉じ始める）・.out（開き始める）・外す
    pub fn wipe_start(&mut self) { self.wipe_on = Some(self.now); self.wipe_out = None; }
    pub fn wipe_open(&mut self) { self.wipe_out = Some(self.now); }
    pub fn wipe_end(&mut self) { self.wipe_on = None; self.wipe_out = None; }
    pub fn wipe_shown(&self) -> bool { self.wipe_on.is_some() }

    fn k(&self, s: Scr) -> f32 { if self.notrans { if self.shown == Some(s) { 1.0 } else { 0.0 } } else { self.fade[si(s)].get(self.now) } }

    /// 毎コマ描く（押した物は、その前に hits.feed で受けてある）
    pub fn ui(&mut self, ctx: &Context) {
        self.hits.begin();
        let screen = ctx.content_rect();
        self.size = (screen.width(), screen.height());
        let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("gyogyo-ui")));
        let pen = Pen::new(ctx, painter);
        // 紙吹雪を進める（欄の下の端で消える）
        self.confetti.step((self.now - self.cheer_t) as f32, self.cheer_floor - 24.0);
        self.cheer_t = self.now;
        self.wheel(ctx, screen);
        if self.notrans { for t in self.fade.iter_mut() { let v = t.target(); t.jump(v); } }
        // 今の版の重なりの順: 読み込み・タイトル・出走登録・発走前 < HUD < 写真判定・結果 < Ⅱ < 指 < 一時停止 < きろく（z 20）< 幕（z 40）
        for s in [Scr::Loading, Scr::Title, Scr::Select, Scr::Intro] { self.one(&pen, s, screen); }
        let hk = if self.notrans { if self.racing { 1.0 } else { 0.0 } } else { self.hud_fade.get(self.now) };
        if hk > 0.0 {
            let (t, f) = (self.touch, self.finishing);
            let dash = t.then(|| self.touch_buttons(screen.width(), screen.height()).0);
            self.hud.draw(&pen.with_alpha(hk), screen, self.safe, dash, f, self.notrans);
        }
        for s in [Scr::Photo, Scr::Result] { self.one(&pen, s, screen); }
        // 遊ぶ画面の部品（Ⅱ・指のスティックとボタン）は、一時停止の上に重ねない。ゴールの後（順位が出ている間）も効かないので出さない
        let play = self.racing && !self.paused && !self.finishing;
        if play && !self.hud.video { self.pause_button(&pen, screen); }
        if self.touch && play { self.touch_ui(&pen, screen); }
        let pk = if self.notrans { if self.paused { 1.0 } else { 0.0 } } else { self.pause_fade.get(self.now) };
        self.pause.ring = self.ring;
        if pk > 0.0 || self.paused { self.pause.draw(&pen.with_alpha(pk), &mut self.hits, &mut self.tw, screen, self.safe, self.paused, self.notrans); } else { self.pause.scroll.off(); }
        self.records_ui(&pen, screen);
        self.wipe_ui(&pen, screen);
        if self.hits.hover.is_some() { ctx.set_cursor_icon(egui::CursorIcon::PointingHand); }
        // 描いた面（ボタン・出ている画面）を game-ui の dom_input へ知らせる。その上で始まった指は遊びの指でなく画面のボタンへ
        // （走る間の Ⅱ も。今の版は #touch が被さって指では押せなかった）、マウスの押下は window の Input へ届かない（今の版は DOM の要素が受ける）
        for (i, r) in self.hits.areas().enumerate() { block(ctx, egui::Id::new(("gyogyo-hit", i)), r, egui::Order::Middle); }
    }

    fn one(&mut self, pen: &Pen, s: Scr, r: Rect) {
        let k = self.k(s);
        if k <= 0.0 && self.shown != Some(s) {
            match s { Scr::Select => self.strip.off(), Scr::Result => { self.res_scroll.off(); self.board_scroll.off(); } _ => {} }
            return;
        }
        let p = pen.with_alpha(k);
        // .screen.show だけ押せる（下へ通さない）
        let on = self.shown == Some(s);
        let hit_start = self.hits.list.len();
        if on { self.hits.block(r); }
        match s {
            Scr::Loading => self.loading(&p, r),
            Scr::Title => self.title(&p, r),
            Scr::Select => self.select(&p, r),
            Scr::Intro => self.intro_ui(&p, r),
            Scr::Photo => self.photo_ui(&p, r),
            Scr::Result => self.result(&p, r),
        }
        if !on { self.hits.list.truncate(hit_start); }
    }

    /// いま送れる箱（上に出ている画面の物だけ。先の物が先に受ける）
    fn scroll_ids(&self) -> &'static [Sc] {
        if self.records.is_some() { &[Sc::Rec] } else if self.paused { &[Sc::Pause] } else {
            match self.shown { Some(Scr::Select) => &[Sc::Strip], Some(Scr::Result) => &[Sc::Board, Sc::Res], _ => &[] }
        }
    }
    fn scroller(&mut self, id: Sc) -> &mut Scroller {
        match id { Sc::Rec => &mut self.rec_scroll, Sc::Pause => &mut self.pause.scroll, Sc::Res => &mut self.res_scroll, Sc::Board => &mut self.board_scroll, Sc::Strip => &mut self.strip }
    }
    /// コマの始め（遊びの計算の前）: 届いている指・マウスの入力を、いま画面に出ている面で読み、押した物の列を返す。
    /// touch は指か。指が送れる箱の上でなぞったら中身を送り、押したことにはしない
    pub fn feed(&mut self, events: &[egui::Event], touch: bool) -> Vec<Act> {
        let mut acts = vec![];
        for e in events {
            self.hits.feed_one(e, &mut acts);
            match e {
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, .. } if touch => {
                    let ids = self.scroll_ids();
                    if let Some(id) = ids.iter().copied().find(|id| { let s = self.scroller(*id); s.can() && s.view.contains(*pos) }) {
                        let s = self.scroller(id);
                        let c = s.coord(*pos);
                        s.s.press(c);
                    }
                }
                egui::Event::PointerMoved(p) => {
                    let mut started = false;
                    for id in [Sc::Rec, Sc::Pause, Sc::Res, Sc::Board, Sc::Strip] {
                        let s = self.scroller(id);
                        let c = s.coord(*p);
                        started |= s.s.moved(c);
                    }
                    if started { self.hits.cancel_down(); }
                }
                egui::Event::PointerButton { button: egui::PointerButton::Primary, pressed: false, .. } => {
                    for id in [Sc::Rec, Sc::Pause, Sc::Res, Sc::Board, Sc::Strip] { self.scroller(id).s.release(); }
                }
                _ => {}
            }
        }
        acts
    }

    /// ホイールで中身を動かす（overflow: auto の箱）。指・マウスが乗っている箱、無ければ送れる最初の箱
    fn wheel(&mut self, ctx: &Context, _r: Rect) {
        // DomInput は向きを egui に合わせて逆にしている。行は 16px（Chrome）
        let d = ctx.input(|i| i.events.iter().fold(egui::Vec2::ZERO, |a, e| match e {
            egui::Event::MouseWheel { delta, unit, .. } => a - *delta * match unit { egui::MouseWheelUnit::Line => 16.0, egui::MouseWheelUnit::Page => 400.0, egui::MouseWheelUnit::Point => 1.0 },
            _ => a,
        }));
        if d == egui::Vec2::ZERO { return; }
        let Some(p) = self.hits.pointer() else { return };
        let ids = self.scroll_ids();
        let on = ids.iter().copied().find(|id| { let s = self.scroller(*id); s.can() && s.view.contains(p) });
        let Some(id) = on.or_else(|| ids.iter().copied().find(|id| self.scroller(*id).can())) else { return };
        let s = self.scroller(id);
        // 横にしか送れない箱は、縦のホイールでも横に送る（Chrome）
        s.s.wheel(if !s.horiz { d.y } else if d.x.abs() > d.y.abs() { d.x } else { d.y });
    }

    // ---- 読み込み ----
    fn loading(&mut self, pen: &Pen, r: Rect) {
        let (w, h) = (r.width(), r.height());
        pen.rect(r, GREEN);
        let max_w = (w - self.safe[1] - self.safe[3]) * 0.86;
        // 改行（\n）で行を分ける（知らせの文は文ごとに 1 行）
        let rows: Vec<T> = self.load_text.split('\n').flat_map(|l| pen.lines(l, JP7, 15.0, 15.0 * 0.08, max_w)).collect();
        let lh = 15.0 * 1.448;
        let text_h = rows.len() as f32 * lh;
        let (bw_, bh_) = primary_size(pen, None, "読み込み直す", PRIMARY);
        let total = 26.0 + 20.0 + text_h + 20.0 + if self.load_failed { bh_ } else { 4.0 };
        let mut y = (h - total) * 0.5;
        const COLS: [u32; 8] = [0xf7f3e8, 0x25262b, 0xe23b35, 0x2f6fd8, 0xf5cc2a, 0x2f9e5b, 0xf28a24, 0xf39ac0];
        let x0 = w * 0.5 - (8.0 * 26.0 + 7.0 * 6.0) * 0.5;
        for (i, c) in COLS.iter().enumerate() {
            let t = (self.now - self.t0) as f32 - i as f32 * 0.07;
            let dy = if t < 0.0 || self.notrans || self.load_failed { 0.0 } else { kf(t.rem_euclid(1.0), &[(0.0, 0.0), (0.3, -12.0), (0.6, 0.0), (1.0, 0.0)], ease) };
            pen.rrect(Rect::from_min_size(pos2(x0 + i as f32 * 32.0, y + dy), vec2(26.0, 26.0)), 6.0, hx(*c));
        }
        y += 46.0;
        for (i, t) in rows.iter().enumerate() {
            pen.text(t, w * 0.5 - t.w * 0.5, base_n(JP7, &self.load_text, 15.0, y + i as f32 * lh), CREAM, &[]);
        }
        y += text_h + 20.0;
        if self.load_failed {
            // 読めなかった: 帯の代わりに「読み込み直す」（押すとページを読み直す）
            let br = Rect::from_min_size(pos2(w * 0.5 - bw_ * 0.5, y), vec2(bw_, bh_));
            let (mut hits, mut tw) = (std::mem::take(&mut self.hits), std::mem::take(&mut self.tw));
            primary(pen, &mut hits, Act::Reload, br, None, "読み込み直す", PRIMARY, &mut tw);
            (self.hits, self.tw) = (hits, tw);
            return;
        }
        let bw = 280.0_f32.min(w * 0.7);
        let bar = Rect::from_min_size(pos2(w * 0.5 - bw * 0.5, y), vec2(bw, 4.0));
        pen.rrect(bar, 2.0, rgba(255, 255, 255, 0.15));
        let k = clamp01(if self.notrans { self.load_fill.target() } else { self.load_fill.get(self.now) });
        if k > 0.0 { pen.clip(bar).rect(Rect::from_min_size(bar.min, vec2(bw * k, 4.0)), GOLD); }
    }

    // ---- タイトル ----
    fn title(&mut self, pen: &Pen, r: Rect) {
        let (w, h) = (r.width(), r.height());
        let (narrow, short) = (w <= 820.0, h <= 520.0);
        pen.vgrad(r, &[(0.0, ink(0.42)), (0.42, ink(0.0)), (0.7, ink(0.0)), (1.0, ink(0.5))]);
        let (x0, x1) = (w * 0.05 + self.safe[3], w * 0.95 - self.safe[1]);
        let (y0, y1) = (h * 0.045 + self.safe[0], h - h * 0.05 - self.safe[2]);
        eyebrow(pen, "GYOGYO MUSUME", x0, y0, GOLD);
        // ロゴ（一文字ずつ跳ねて出る）
        let fs = if short { cl(40.0, h * 0.17, 90.0) } else if narrow { w * 0.17 } else { cl(54.0, w * 0.12, 190.0) };
        let top = y0 + nlh(NUM6, "T", 13.0) + 10.0;
        let shown = self.shown == Some(Scr::Title);
        let t = (self.now - self.title_at) as f32;
        let mut i = 0;
        for (li, (line, col)) in [("魚々娘", GOLD)].iter().enumerate() {
            let ly = top + li as f32 * fs * 0.96;
            let base = base_l(DISP, fs, ly, fs * 0.96);
            let mut x = x0 + if li == 1 { fs * 0.02 } else { 0.0 };
            for ch in line.chars() {
                let s = ch.to_string();
                let tx = pen.t(&s, DISP, fs, -0.02 * fs);
                let cx = x + tx.w * 0.5;
                let cy = ly + fs * 0.48;
                // body.notrans はアニメーションを外す（字は元の opacity: 0 のまま）。prefers-reduced-motion は止めて見せる
                let still = self.notrans || self.reduce_motion;
                let (op, ty, rot) = if self.notrans { (0.0, 0.55 * fs, 4.0) } else if self.reduce_motion { (1.0, 0.0, 0.0) } else if !shown { (0.0, 0.55 * fs, 4.0) } else {
                    let k = clamp01((t - (0.15 + i as f32 * 0.055)) / 0.7);
                    let e = bezier_spring(k);
                    (clamp01(e), 0.55 * fs * (1.0 - e), 4.0 * (1.0 - e))
                };
                let tb = t - (1.5 + i as f32 * 0.14);
                let bob = if shown && !still && tb >= 0.0 { kf((tb / 3.6).rem_euclid(1.0), &[(0.0, 0.0), (0.06, -0.07), (0.12, 0.0), (1.0, 0.0)], ease_io) * fs } else { 0.0 };
                if op > 0.0 {
                    let q = pen.sub(Xf::tr(0.0, bob).then(Xf::about(cx, cy, Xf::tr(0.0, ty).then(Xf::rot(rot)))), op);
                    q.text(&tx, x, base, *col, &[TS(0.0, 0.035 * fs, 0.0, ink(0.55))]);
                }
                x += tx.w;
                i += 1;
            }
        }
        // 下の段: 出走登録へ・成績・きろく
        let key = if self.touch { None } else { Some("Space") };
        let mut items = vec![];
        if let Some(k) = key { items.push(It::K(Kbd::new(pen, k, 19.0))); }
        items.push(It::T("出走登録へ".into(), JP9, 19.0, 19.0 * 0.06, GREEN));
        let sw = it_w(pen, &items) + 60.0;
        let sh = 19.0 * 1.448 + 32.0;
        let stats: Vec<String> = if self.title_stats.is_empty() { vec![] } else { self.title_stats.split("<br>").map(String::from).collect() };
        let st_w = stats.iter().map(|s| tw(pen, s, NUM5, 14.0, 0.0)).fold(0.0, f32::max);
        let st_h = stats.len() as f32 * 22.4;
        let (gw, gh) = ghost_size(pen, "きろく", false);
        // 音の切り替え（一時停止の「音」と同じ字。幅は長い方で決めて、切り替えても動かさない）
        let mute_s = if self.muted { "音: オフ" } else { "音: オン" };
        let mw = ghost_size(pen, "音: オン", false).0.max(ghost_size(pen, "音: オフ", false).0);
        let rw = st_w + 14.0 + mw + 8.0 + gw;
        let rh = st_h.max(gh);
        // みんなで走る（かけっこグランプリへ移る）の入口は「出走登録へ」の右
        let ow = tw(pen, ONLINE_LABEL, JP9, 16.0, 0.0) + 52.0;
        let one_line = sw + 12.0 + ow + 20.0 + rw <= x1 - x0;
        let (start_r, right_x, right_bottom) = if one_line {
            (Rect::from_min_size(pos2(x0, y1 - sh), vec2(sw, sh)), x1 - rw, y1)
        } else {
            (Rect::from_min_size(pos2(x0, y1 - rh - 20.0 - sh), vec2(sw, sh)), x0, y1)
        };
        // .start（hover で浮く）
        self.hits.add(Act::Start, start_r, Shape::Round(sh * 0.5));
        let hk = self.tw.go(Act::Start, if self.hits.is_hover(Act::Start) { 1.0 } else { 0.0 }, 0.25, bezier_spring);
        let p = pen.sub(Xf::about(start_r.center().x, start_r.center().y, Xf::tr(0.0, -3.0 * hk).then(Xf::scale(1.0 + 0.03 * hk, 1.0 + 0.03 * hk))), 1.0);
        p.box_shadow(start_r, sh * 0.5, 0.0, 10.0, 40.0, ink(0.4));
        p.rrect(start_r, sh * 0.5, CREAM);
        it_draw(&p, &items, start_r.left() + 30.0, start_r.center().y, GREEN, &[]);
        {
            let or = Rect::from_min_size(pos2(start_r.right() + 12.0, start_r.top()), vec2(ow, sh));
            self.hits.add(Act::Online, or, Shape::Round(sh * 0.5));
            pen.rrect(or, sh * 0.5, ink(0.55));
            pen.rframe(or, sh * 0.5, 2.0, if self.hits.is_hover(Act::Online) { CREAM } else { GOLD });
            let t = pen.t(ONLINE_LABEL, JP9, 16.0, 0.0);
            pen.text(&t, or.center().x - t.w * 0.5, base_n(JP9, ONLINE_LABEL, 16.0, or.center().y - 16.0 * 1.448 * 0.5), CREAM, &[]);
        }
        // 成績（右寄せ・行の高さ 1.6）
        for (i, s) in stats.iter().enumerate() {
            let w_ = tw(pen, s, NUM5, 14.0, 0.0);
            let top = right_bottom - st_h + i as f32 * 22.4;
            let t = pen.t(s, NUM5, 14.0, 0.0);
            let (a, d) = metrics(NUM5, s, 14.0);
            pen.text(&t, right_x + st_w - w_, top + (22.4 - (a + d)) * 0.5 + a, CREAM, &[]);
        }
        let mr = Rect::from_min_size(pos2(right_x + st_w + 14.0, right_bottom - gh), vec2(mw, gh));
        ghost(pen, &mut self.hits, Act::Mute, mr, mute_s, 14.0, Some(ink(0.5)), CREAM);
        let gr = Rect::from_min_size(pos2(mr.right() + 8.0, right_bottom - gh), vec2(gw, gh));
        ghost(pen, &mut self.hits, Act::Records, gr, "きろく", 14.0, Some(ink(0.5)), CREAM);
    }

    // ---- 出走登録 ----
    fn select(&mut self, pen: &Pen, r: Rect) {
        let (w, h) = (r.width(), r.height());
        let (narrow, short) = (w <= 820.0, h <= 520.0);
        pen.vgrad(r, &[(0.0, ink(0.0)), (0.62, ink(0.0)), (1.0, ink(0.72))]);
        pen.hgrad(r, &[(0.0, ink(0.62)), (0.38, ink(0.0))]);
        let (pt, px, pb) = if short { (8.0 + self.safe[0], 14.0, 8.0 + self.safe[2]) } else { (h * 0.035 + self.safe[0], w * 0.035, h * 0.03 + self.safe[2]) };
        let (x0, x1, y0, y1) = (px + self.safe[3], w - px - self.safe[1], pt, h - pb);
        let left_w = if short { 330.0_f32.min((x1 - x0) * 0.46) } else if narrow { x1 - x0 } else { 370.0_f32.min(w * 0.92) };
        let gap = if short { 4.0 } else { 10.0 };
        let mut y = y0;
        if !short { y += eyebrow(pen, "ENTRY", x0, y, GOLD) + gap; }
        // 単発・カップ
        let (bfs, bpv) = if short { (12.0, 4.0) } else { (14.0, 8.0) };
        let seg_h = bfs * 1.448 + bpv * 2.0;
        let cup = self.sel.cup_mode;
        let career = self.sel.career_mode;
        let labels = vec![("単発レース".to_string(), Act::Mode(false), !cup && !career), (self.sel.cup_label.clone(), Act::Mode(true), cup), (self.sel.career_label.clone(), Act::Career, career)];
        self.segment(pen, &labels, x0, y, left_w, seg_h, bfs);
        y += seg_h + 8.0 + gap;
        if career { self.career_card(pen, x0, y, left_w, short); }
        if !career {
        // コース
        let (cpv, cph, cr, cfs) = if short { (5.0, 10.0, 10.0, 13.0) } else { (11.0, 14.0, 16.0, 17.0) };
        let blurb = !narrow && !short;
        let cgap = if short { 3.0 } else { 6.0 };
        let courses = self.sel.courses.clone();
        for (i, c) in courses.iter().enumerate() {
            let bh = 4.0 + cpv * 2.0 + cfs * 1.448 + if blurb { 1.0 + 12.0 * 1.448 } else { 0.0 };
            let br = Rect::from_min_size(pos2(x0, y), vec2(left_w, bh));
            self.hits.add(Act::Course(i), br, Shape::Round(cr));
            let p = pen.sub(Xf::tr(if c.on { 6.0 } else { 0.0 }, 0.0), if cup { 0.85 } else { 1.0 });
            p.rrect(br, cr, ink(0.6));
            if c.on { p.rframe(br, cr, 2.0, GOLD); }
            let (cx, cy) = (br.left() + 2.0 + cph, br.top() + 2.0 + cpv);
            label(&p, &c.name, JP9, cfs, cfs * 0.02, cx, cy, CREAM, &[]);
            if let Some(b) = &c.best { let bw = tw(pen, b, NUM5, 12.0, 0.0); label(&p, b, NUM5, 12.0, 0.0, br.right() - 2.0 - cph - bw, cy + 3.0, GOLD, &[]); }
            if blurb { label(&p, &c.blurb, JP5, 12.0, 0.0, cx, cy + cfs * 1.448 + 1.0, CREAM_DIM, &[]); }
            y += bh + cgap;
        }
        y += gap - cgap;
        // 難しさ
        let labels: Vec<(String, Act, bool)> = ["やさしい", "ふつう", "むずかしい"].iter().enumerate().map(|(i, s)| (s.to_string(), Act::Diff(i), self.sel.diff == i)).collect();
        self.segment(pen, &labels, x0, y, left_w, seg_h, bfs);
        }

        // 下の段: 名前・キャラの帯・ゲートへ
        let nfs = if short { 24.0 } else { cl(30.0, w * 0.05, 58.0) };
        let best = self.sel.best.clone();
        let (sa, sd) = if best.is_empty() { (0.0, 0.0) } else { metrics(NUM5, &best, 15.0) };
        let name_h = (0.936 * nfs).max(sa) + (0.064 * nfs).max(sd);
        let cell = if short { 42.0 } else { 64.0 };
        let (spv, sph, sgap) = if short { (4.0, 2.0, 5.0) } else { (8.0, 4.0, 8.0) };
        let strip_h = cell + spv * 2.0;
        let z = if short { PRIMARY_SHORT } else { PRIMARY };
        let key = if self.touch { None } else { Some("Space") };
        let (gw0, gh) = primary_size(pen, key, "ゲートへ", z);
        let (name_r, strip_r, go_r);
        if short {
            let row1 = name_h.max(gh);
            let top = y1 - strip_h - 2.0 - row1;
            go_r = Rect::from_min_size(pos2(x1 - gw0, top + row1 - gh), vec2(gw0, gh));
            name_r = Rect::from_min_size(pos2(x0, top + row1 - name_h), vec2(x1 - x0 - gw0 - 10.0, name_h));
            strip_r = Rect::from_min_size(pos2(x0, y1 - strip_h), vec2(x1 - x0, strip_h));
        } else if narrow {
            go_r = Rect::from_min_size(pos2(x0, y1 - gh), vec2(x1 - x0, gh));
            strip_r = Rect::from_min_size(pos2(x0, go_r.top() - 6.0 - strip_h), vec2(x1 - x0, strip_h));
            name_r = Rect::from_min_size(pos2(x0, strip_r.top() - 6.0 - name_h), vec2(x1 - x0, name_h));
        } else {
            let row2 = strip_h.max(gh);
            go_r = Rect::from_min_size(pos2(x1 - gw0, y1 - gh), vec2(gw0, gh));
            strip_r = Rect::from_min_size(pos2(x0, y1 - strip_h), vec2(x1 - x0 - gw0 - 18.0, strip_h));
            name_r = Rect::from_min_size(pos2(x0, y1 - row2 - 6.0 - name_h), vec2(x1 - x0, name_h));
        }
        // 名前と自己ベスト（基線をそろえる）
        let base = name_r.top() + (0.936 * nfs).max(sa);
        let nt = pen.t(&self.sel.name, DISP, nfs, 0.0);
        pen.text(&nt, name_r.left(), base, CREAM, &[]);
        if !best.is_empty() { let t = pen.t(&best, NUM5, 15.0, 0.0); pen.text(&t, name_r.left() + nt.w + 14.0, base, GOLD, &[]); }
        // 帯（横に流れる。選んだ札を中央へ）
        let n = self.heroes.len();
        let content = sph * 2.0 + n as f32 * cell + n.saturating_sub(1) as f32 * sgap;
        let max_s = (content - strip_r.width()).max(0.0);
        if self.strip_center {
            self.strip_center = false;
            let c = sph + self.sel.hero_on as f32 * (cell + sgap) + cell * 0.5;
            self.strip.s.at = c - strip_r.width() * 0.5;
        }
        // 指で横になぞると送れる（ホイールでも）
        let strip_at = self.strip.place(strip_r, max_s);
        let clip = pen.clip(strip_r);
        let old_clip = self.hits.clip.replace(strip_r);
        let heroes = self.heroes.clone();
        for (i, hb) in heroes.iter().enumerate() {
            let x = strip_r.left() + sph - strip_at + i as f32 * (cell + sgap);
            let br = Rect::from_min_size(pos2(x, strip_r.top() + spv), vec2(cell, cell));
            if br.right() < strip_r.left() - 20.0 || br.left() > strip_r.right() + 20.0 { continue; }
            self.hits.add(Act::Hero(i), br, Shape::Round(if short { 10.0 } else { 18.0 }));
            let on = i == self.sel.hero_on;
            let xf = if on { Xf::about(br.center().x, br.center().y, Xf::tr(0.0, -6.0).then(Xf::scale(1.12, 1.12))) } else { Xf::ID };
            let p = clip.sub(xf, if on { 1.0 } else { 0.8 });
            let rad = if short { 10.0 } else { 18.0 };
            p.rrect(br, rad, rgba(16, 40, 32, 0.7));
            if let Some(t) = &hb.tex {
                let pb = br.shrink(2.0);
                if hb.full {
                    let iw = pb.width() * 2.1;
                    let ih = iw * t.h / t.w;
                    let img = Rect::from_min_size(pos2(pb.left() + (pb.width() - iw) * 0.5, pb.top() + (pb.height() - ih) * 0.08), vec2(iw, ih));
                    p.image_poly(t.handle.id(), &rr_points(br, rad, 6), img, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), WHITE);
                } else {
                    p.image_poly(t.handle.id(), &rr_points(pb, (rad - 2.0).max(0.0), 6), pb, cover_uv(t, pb.width(), pb.height(), 0.5, 0.5), WHITE);
                }
            }
            p.rframe(br, rad, 2.0, if on { GOLD } else { rgba(255, 246, 228, 0.25) });
        }
        self.hits.clip = old_clip;
        // ゲートへ
        let tw_ = &mut self.tw;
        primary(pen, &mut self.hits, Act::Go, go_r, key, "ゲートへ", z, tw_);
        // キーの案内（右下）
        if !self.touch && !short {
            let mut items = vec![It::K(Kbd::new(pen, "←", 12.0)), It::K(Kbd::new(pen, "→", 12.0)), It::T("キャラ　".into(), JP7, 12.0, 0.0, CREAM_DIM), It::K(Kbd::new(pen, "↑", 12.0)), It::K(Kbd::new(pen, "↓", 12.0)), It::T("コース".into(), JP7, 12.0, 0.0, CREAM_DIM)];
            let kw = it_w(pen, &items);
            let bottom = h - (h * 0.03 + 66.0) - self.safe[2];
            let cy = bottom - 12.0 * 1.448 * 0.5;
            let sh = [TS(0.0, 1.0, 6.0, ink(0.8))];
            let _ = &mut items;
            it_draw(pen, &items, x1 - kw, cy, CREAM_DIM, &sh);
        }
    }

    /// 育成の札: 次のレース（階級・名前・コース・相手）と、育てた能力（5 つの段）・戦績・決まり
    fn career_card(&mut self, pen: &Pen, x: f32, y0: f32, w: f32, short: bool) {
        let c = self.sel.career.clone();
        let (pad, tfs, sfs): (f32, f32, f32) = if short { (8.0, 14.0, 11.0) } else { (14.0, 18.0, 12.0) };
        let lh = sfs * 1.448;
        let stat_h = if short { 20.0 } else { 5.0 * 17.0 + 4.0 * 3.0 };
        let h = pad + tfs * 1.448 + 2.0 + lh + lh + 6.0 + stat_h + 6.0 + lh + lh + pad;
        let r = Rect::from_min_size(pos2(x, y0), vec2(w, h));
        pen.rrect(r, 16.0, ink(0.6));
        pen.rframe(r, 16.0, 2.0, GOLD);
        let (ix, iw) = (x + pad + 2.0, w - pad * 2.0 - 4.0);
        let mut y = y0 + pad;
        // 階級の札と、レースの名前
        let gw = tw(pen, &c.grade, NUM7, tfs - 4.0, 0.0) + 14.0;
        let gr = Rect::from_min_size(pos2(ix, y + (tfs * 1.448 - (tfs + 4.0)) * 0.5), vec2(gw, tfs + 4.0));
        pen.rrect(gr, 6.0, GOLD);
        label(pen, &c.grade, NUM7, tfs - 4.0, 0.0, gr.left() + 7.0, gr.center().y - nlh(NUM7, &c.grade, tfs - 4.0) * 0.5, GREEN, &[]);
        label(pen, &c.race, JP9, tfs, tfs * 0.02, gr.right() + 8.0, y, CREAM, &[]);
        y += tfs * 1.448 + 2.0;
        label(pen, &c.sub, JP5, sfs, 0.0, ix, y, CREAM_DIM, &[]);
        y += lh;
        label(pen, &c.rivals, JP7, sfs, 0.0, ix, y, CREAM, &[]);
        y += lh + 6.0;
        // 能力: 低い画面は 1 行の札、ほかは 1 つずつ 9 段の目盛り
        use sim::items::Stat;
        if short {
            let mut cx = ix;
            for s in Stat::ALL {
                let lv = c.levels[s.i()].to_string();
                let lt = pen.t(&lv, NUM7, 12.0, 0.0);
                pen.circle(pos2(cx + 10.0, y + 10.0), 9.0, crate::hud::stat_c(s));
                let gt = pen.t(s.glyph(), JP9, 10.0, 0.0);
                pen.text(&gt, cx + 10.0 - gt.w * 0.5, base_n(JP9, s.glyph(), 10.0, y + 10.0 - 10.0 * 1.448 * 0.5), WHITE, &[]);
                pen.text(&lt, cx + 23.0, base_n(NUM7, &lv, 12.0, y + 10.0 - nlh(NUM7, &lv, 12.0) * 0.5), CREAM, &[]);
                cx += 23.0 + lt.w + 12.0;
            }
        } else {
            let name_w = Stat::ALL.iter().map(|s| tw(pen, s.name(), JP7, 12.0, 0.0)).fold(0.0, f32::max);
            for (k, s) in Stat::ALL.iter().enumerate() {
                let ry = y + k as f32 * 20.0;
                let col = crate::hud::stat_c(*s);
                pen.circle(pos2(ix + 8.5, ry + 8.5), 8.5, col);
                let gt = pen.t(s.glyph(), JP9, 10.0, 0.0);
                pen.text(&gt, ix + 8.5 - gt.w * 0.5, base_n(JP9, s.glyph(), 10.0, ry + 8.5 - 10.0 * 1.448 * 0.5), WHITE, &[]);
                label(pen, s.name(), JP7, 12.0, 0.0, ix + 23.0, ry + 8.5 - 12.0 * 1.448 * 0.5, CREAM, &[]);
                let lv = c.levels[s.i()];
                let bx = ix + 23.0 + name_w + 10.0;
                let nw = tw(pen, "9", NUM7, 13.0, 0.0) + 8.0;
                let pw = ((ix + iw - nw - bx) - 8.0 * 3.0) / 9.0;
                for q in 0..9u8 {
                    let pr = Rect::from_min_size(pos2(bx + q as f32 * (pw + 3.0), ry + 4.0), vec2(pw, 9.0));
                    pen.rrect(pr, 3.0, if q < lv { col } else { rgba(255, 246, 228, 0.14) });
                }
                let ls = lv.to_string();
                let lw_ = tw(pen, &ls, NUM7, 13.0, 0.0);
                label(pen, &ls, NUM7, 13.0, 0.0, ix + iw - lw_, ry + 8.5 - nlh(NUM7, &ls, 13.0) * 0.5, CREAM, &[]);
            }
        }
        y += stat_h + 6.0;
        label(pen, &c.record, NUM5, sfs, 0.0, ix, y, GOLD, &[]);
        y += lh;
        label(pen, "4 着以内で次へ。5 着以下は、その場で引退", JP7, sfs, 0.0, ix, y, CREAM_DIM, &[]);
        // 途中経過があるときだけ: 最初から育て直す（押すと今の途中経過は消える）
        if c.running {
            let (gw2, gh2) = ghost_size(pen, &c.reset_label, true);
            let gr2 = Rect::from_min_size(pos2(x, r.bottom() + 6.0), vec2(gw2, gh2));
            let mut hits = std::mem::take(&mut self.hits);
            ghost(pen, &mut hits, Act::CareerNew, gr2, &c.reset_label, 12.0, Some(ink(0.5)), LINE);
            self.hits = hits;
        }
    }

    /// .sel-mode / .sel-diff（丸い溝に並ぶ札）
    fn segment(&mut self, pen: &Pen, items: &[(String, Act, bool)], x: f32, y: f32, w: f32, bh: f32, fs: f32) {
        let r = Rect::from_min_size(pos2(x, y), vec2(w, bh + 8.0));
        pen.rrect(r, r.height() * 0.5, ink(0.6));
        let n = items.len() as f32;
        let bw = (w - 8.0 - 4.0 * (n - 1.0)) / n;
        for (i, (s, act, on)) in items.iter().enumerate() {
            let br = Rect::from_min_size(pos2(x + 4.0 + i as f32 * (bw + 4.0), y + 4.0), vec2(bw, bh));
            self.hits.add(*act, br, Shape::Round(bh * 0.5));
            if *on { pen.rrect(br, bh * 0.5, CREAM); }
            let rows = pen.lines(s, JP7, fs, 0.0, bw - 12.0);
            let lh = fs * 1.448;
            let top = br.center().y - rows.len() as f32 * lh * 0.5;
            for (j, t) in rows.iter().enumerate() {
                pen.text(t, br.center().x - t.w * 0.5, base_n(JP7, s, fs, top + j as f32 * lh), if *on { GREEN } else { CREAM_DIM }, &[]);
            }
        }
    }

    // ---- 発走前 ----
    fn intro_ui(&self, pen: &Pen, r: Rect) {
        let (w, h) = (r.width(), r.height());
        let short = h <= 520.0;
        pen.vgrad(r, &[(0.0, ink(0.0)), (0.66, ink(0.0)), (1.0, ink(0.55))]);
        let [sub, name, len] = &self.intro;
        let bfs = if short { 34.0 } else { cl(36.0, w * 0.064, 92.0) };
        let (h1, h2, h3) = (nlh(NUM5, sub, 15.0), bfs * 1.05, nlh(NUM5, len, 18.0));
        let x = w * 0.05 + self.safe[3];
        let mut y = h - h * 0.06 - self.safe[2] - (h1 + 2.0 + h2 + 2.0 + h3);
        label(pen, sub, NUM5, 15.0, 3.0, x, y, GOLD, &[]);
        y += h1 + 2.0;
        let t = pen.t(name, DISP, bfs, 0.0);
        pen.text(&t, x, base_l(DISP, bfs, y, h2), CREAM, &[]);
        y += h2 + 2.0;
        label(pen, len, NUM5, 18.0, 0.0, x, y, CREAM, &[]);
    }

    // ---- 写真判定 ----
    fn photo_ui(&self, pen: &Pen, r: Rect) {
        let w = r.width();
        pen.rect(r, rgba(8, 20, 16, 0.5));
        let cw = 640.0_f32.min(w * 0.8);
        let chh = cw * 360.0 / 640.0;
        let eb = nlh(NUM6, "P", 13.0);
        let h2 = 28.0 * 1.448;
        let res_h = 26.0_f32.max(19.0 * 1.448);
        let card = Rect::from_center_size(r.center(), vec2(cw + 44.0, 18.0 + eb + 2.0 + h2 + 10.0 + chh + 10.0 + res_h + 20.0));
        let k = if self.notrans { 1.0 } else { self.photo_k.get(self.now) };
        let sc = lerp(0.92, 1.0, k);
        let p = pen.sub(Xf::about(card.center().x, card.center().y, Xf::rot(-1.2).then(Xf::scale(sc, sc))), 1.0);
        p.box_shadow(card, 10.0, 0.0, 30.0, 80.0, rgba(0, 0, 0, 0.5));
        p.rrect(card, 10.0, PAPER);
        let cx = card.center().x;
        let mut y = card.top() + 18.0;
        let ew = eyebrow_w(pen, "PHOTO FINISH");
        eyebrow(&p, "PHOTO FINISH", cx - ew * 0.5, y, hx(0xb0732e));
        y += eb + 2.0;
        let t = pen.t("写真判定", DISP, 28.0, 0.0);
        p.text(&t, cx - t.w * 0.5, base_n(DISP, "写", 28.0, y), hx(0x27231a), &[]);
        y += h2 + 10.0;
        let img = Rect::from_min_size(pos2(card.left() + 22.0, y), vec2(cw, chh));
        p.rect(img, hx(0x333333));
        if let Some(t) = &self.photo { p.image(t, img, WHITE); }
        y += chh + 10.0;
        if !self.photo_res.is_empty() {
            let t = pen.t(&self.photo_res, JP9, 19.0, 0.0);
            p.text(&t, cx - t.w * 0.5, base_n(JP9, &self.photo_res, 19.0, y), hx(0x27231a), &[]);
        }
    }

    // ---- 結果 ----
    fn result(&mut self, pen: &Pen, r: Rect) {
        let (w, h) = (r.width(), r.height());
        let (narrow, short) = (w <= 820.0, h <= 520.0);
        let column = narrow && !short;
        if column { pen.rect(r, ink(0.55)); }
        else if short { pen.hgrad(r, &[(0.0, ink(0.82)), (0.4, ink(0.4)), (0.52, ink(0.0))]); }
        else { pen.hgrad(r, &[(0.0, ink(0.82)), (0.26, ink(0.5)), (0.38, ink(0.0)), (0.66, ink(0.0)), (1.0, ink(0.35))]); }
        let sf = self.safe;
        let (pt, px, pb) = if short { (8.0 + sf[0], 14.0, 8.0 + sf[2]) } else { (h * 0.03 + sf[0], w * 0.04, h * 0.03 + sf[2]) };
        let (x0, x1) = (px + sf[3], w - px - sf[1]);
        let aw = x1 - x0;
        let board_w = if short { aw * 0.434 } else if narrow { aw } else { 370.0_f32.min(w * 0.42) };
        // 縦持ちでは、左の欄も掲示板と同じ幅（幅の 40% だと字が細かく折れる）
        let left_w = if short { aw * 0.455 } else if column { board_w } else { 350.0_f32.min(w * 0.4) };
        let res = self.res.clone();
        let board_full = self.board(pen, None, &res, Rect::from_min_size(pos2(0.0, 0.0), vec2(board_w, 1e5)), narrow, short);
        if column {
            // ボタンは送りの外（下の端）に置き、送らなくても見える。上の欄と掲示板は、足りなければ指・ホイールで送る
            self.board_scroll.off();
            let bar_h = self.res_buttons(pen, None, &res, 0.0, 0.0, left_w, short);
            let bar_top = h - pb - bar_h;
            let area = Rect::from_min_max(pos2(0.0, pt), pos2(w, bar_top - 14.0));
            let info_h = self.left(pen, None, &res, 0.0, 0.0, left_w, narrow, short);
            let total = info_h + 10.0 + board_full;
            let at = self.res_scroll.place(area, total - area.height());
            let top = area.top() + ((area.height() - total) * 0.5).max(0.0) - at;
            let (lx, bx) = (w * 0.5 + (sf[3] - sf[1]) * 0.5 - left_w * 0.5, w * 0.5 + (sf[3] - sf[1]) * 0.5 - board_w * 0.5);
            // 送るときだけ切る（入りきるときは掲示板の影を切らない）
            let clip = if self.res_scroll.can() { pen.clip(area) } else { pen.clone() };
            let old = self.hits.clip.replace(area);
            self.draw_left(&clip, &res, lx, top, left_w, narrow, short);
            self.draw_board(&clip, &res, Rect::from_min_size(pos2(bx, top + info_h + 10.0), vec2(board_w, board_full)), board_full, narrow, short, false);
            self.hits.clip = old;
            more_cue(pen, &self.res_scroll, rgba(10, 30, 24, 0.92), CREAM);
            // 紙吹雪は上の欄（着順・タイム）の字の前。掲示板とボタンの上には出さない（欄の下の端で消える）
            self.cheer_box = Rect::from_min_size(pos2(lx, top), vec2(left_w, info_h));
            self.cheer_confetti(pen, (top + info_h + 6.0).min(area.bottom()), w);
            let (mut hits, mut tw) = (std::mem::take(&mut self.hits), std::mem::take(&mut self.tw));
            self.res_buttons(pen, Some((&mut hits, &mut tw)), &res, lx, bar_top, left_w, short);
            (self.hits, self.tw) = (hits, tw);
        } else {
            self.res_scroll.off();
            // 左の欄（着順・タイム…）→ 紙吹雪 → ボタンの順に描く（紙吹雪は欄の字の前・ボタンの後ろ。右の掲示板の上には出さない）
            let info_h = self.left(pen, None, &res, 0.0, 0.0, left_w, narrow, short);
            let btn_gap = self.left_items(short).0 + 6.0;
            let lh_left = info_h + btn_gap + self.res_buttons(pen, None, &res, 0.0, 0.0, left_w, short);
            let board_h = board_full.min(h - pt - pb);
            let mid = pt + (h - pt - pb) * 0.5;
            let ly = mid - lh_left * 0.5;
            self.draw_left(pen, &res, x0, ly, left_w, narrow, short);
            self.draw_board(pen, &res, Rect::from_min_size(pos2(x1 - board_w, mid - board_h * 0.5), vec2(board_w, board_h)), board_full, narrow, short, true);
            self.cheer_box = Rect::from_min_size(pos2(x0, ly), vec2(left_w, info_h));
            self.cheer_confetti(pen, ly + info_h + btn_gap - 4.0, x1 - board_w - 10.0);
            let (mut hits, mut tw) = (std::mem::take(&mut self.hits), std::mem::take(&mut self.tw));
            self.res_buttons(pen, Some((&mut hits, &mut tw)), &res, x0, ly + info_h + btn_gap, left_w, short);
            (self.hits, self.tw) = (hits, tw);
        }
    }

    /// 紙吹雪を描く。floor は消える下の端、right は右の端（掲示板とボタンの上には出さない）
    fn cheer_confetti(&mut self, pen: &Pen, floor: f32, right: f32) {
        self.cheer_floor = floor;
        self.cheer_right = right;
        if self.notrans || self.confetti.is_empty() { return; }
        // 8 枠の色から（黄・赤・青・緑・桃）とクリーム
        const COLORS: [C; 6] = [GOLD, hx(0xe23b35), hx(0x2f6fd8), hx(0x2f9e5b), hx(0xf39ac0), CREAM];
        self.confetti.draw(&pen.clip(Rect::from_min_max(pos2(0.0, 0.0), pos2(right, floor))), &COLORS);
    }

    fn left_items(&self, short: bool) -> (f32, f32) { if short { (3.0, 60.0) } else { (7.0, 0.0) } }

    fn place_fs(&self, h: f32, narrow: bool, short: bool) -> f32 { if short { 60.0 } else if narrow { 90.0 } else { cl(100.0, h * 0.22, 210.0) } }

    fn draw_left(&mut self, pen: &Pen, d: &ResultData, x: f32, y: f32, lw: f32, narrow: bool, short: bool) {
        let (mut hits, mut tw) = (std::mem::take(&mut self.hits), std::mem::take(&mut self.tw));
        self.left(pen, Some((&mut hits, &mut tw)), d, x, y, lw, narrow, short);
        (self.hits, self.tw) = (hits, tw);
    }
    /// .res-left（着順・タイム・振り返り・メダル・順位の流れ）を描く（hits が None なら測るだけ）。高さを返す。ボタンは別（res_buttons。紙吹雪をボタンの後ろに出すため）
    #[allow(clippy::too_many_arguments)]
    fn left(&self, pen: &Pen, hits: Option<(&mut Hits, &mut Tw)>, d: &ResultData, x: f32, y0: f32, lw: f32, narrow: bool, short: bool) -> f32 {
        let draw = hits.is_some();
        let (gap, _) = self.left_items(short);
        let h = pen.ctx.content_rect().height();
        let mut y = y0;
        // 見出し
        if draw { eyebrow(pen, &d.eyebrow, x, y, GOLD); }
        y += nlh(NUM6, &d.eyebrow, 13.0) + gap;
        // 着順
        let pfs = self.place_fs(h, narrow, short);
        let (ufs, umb) = if short { (22.0, 4.0) } else { (40.0, 14.0) };
        let row = (0.85 * pfs).max(0.85 * ufs + umb);
        if draw {
            let bt = pen.t(&d.place, NUM7, pfs, 0.0);
            let btop = y + row - 0.85 * pfs;
            pen.text(&bt, x, base_l(NUM7, pfs, btop, 0.85 * pfs), if d.win { GOLD } else { CREAM }, &[]);
            let ut = pen.t(&d.unit, DISP, ufs, 0.0);
            let utop = y + row - umb - 0.85 * ufs;
            pen.text(&ut, x + bt.w + 8.0, base_l(DISP, ufs, utop, 0.85 * ufs), CREAM, &[]);
        }
        y += row + gap;
        // タイム
        let tfs = if short { 15.0 } else { 21.0 };
        let (mut a, mut dd) = metrics(NUM6, &d.time, tfs);
        if let Some((em, _)) = &d.time_em { let (a2, d2) = metrics(NUM6, em, 16.0); a = a.max(a2); dd = dd.max(d2); }
        // 狭い欄では em の字が次の行へ折れる（和文の字の間・空白の後ろで）
        let t = pen.t(&d.time, NUM6, tfs, 0.0);
        let mut lines: Vec<(f32, String)> = vec![(x + t.w + 8.0, String::new())];
        if let Some((em, _)) = &d.time_em {
            for c in break_chunks(em) {
                let (lx, cur) = lines.last().unwrap().clone();
                let cand = format!("{cur}{c}");
                if !cur.is_empty() && lx + tw(pen, cand.trim_end(), NUM6, 16.0, 0.0) > x + lw { lines.push((x, c)); } else { lines.last_mut().unwrap().1 = cand; }
            }
        }
        if draw {
            // タイム: 0 から数え上がる（並びの幅は本当のタイムで取ってある）。数え終わりに、自己ベストでなければ 1 回だけ小さく弾む
            let still = self.notrans;
            let counting = match self.count { Some(v) if !still => Some(pen.t(&sim::result::time_text(v), NUM6, tfs, 0.0)), _ => None };
            let ts = counting.as_ref().unwrap_or(&t);
            let sc = match self.cheer_at {
                Some(at) if !self.cheer_best && !self.calm && !still => { let u = ((self.now - at) as f32) / 0.32; if u < 1.0 { 1.0 + 0.16 * (u * std::f32::consts::PI).sin() } else { 1.0 } }
                _ => 1.0,
            };
            pen.sub(Xf::about(x + t.w * 0.5, y + a - tfs * 0.35, Xf::scale(sc, sc)), 1.0).text(ts, x, y + a, CREAM, &[]);
            if let Some((_, dim)) = &d.time_em {
                // 「自己ベスト！」: 数え上げの間は出さず、数え終わりに弾んで出る（動きを減らす設定では弾まない）。「ベスト 1:23.45」（自己ベストでない）は始めから出す
                let k = if *dim || still { 1.0 } else {
                    match (self.count, self.cheer_at) { (Some(_), None) => 0.0, (_, Some(at)) if !self.calm => snap(clamp01(((self.now - at) as f32) / 0.35)), _ => 1.0 }
                };
                if k > 0.0 {
                    for (i, (lx, s)) in lines.iter().enumerate() {
                        let e = pen.t(s.trim_end(), NUM6, 16.0, 0.0);
                        let base = y + a + i as f32 * (a + dd);
                        pen.sub(Xf::about(*lx + e.w * 0.5, base - 16.0 * 0.4, Xf::scale(k, k)), if *dim { 0.75 } else { 1.0 }).text(&e, *lx, base, GOLD, &[]);
                    }
                }
            }
        }
        y += (a + dd) * lines.len() as f32 + gap;
        // 次の相手
        if !d.rival.is_empty() {
            let rows = pen.lines(&d.rival, JP9, 14.0, 0.0, lw);
            for (i, t) in rows.iter().enumerate() { if draw { pen.text(t, x, base_n(JP9, &d.rival, 14.0, y + i as f32 * 14.0 * 1.448), GOLD, &[]); } }
            y += rows.len() as f32 * 14.0 * 1.448;
        }
        y += gap;
        // 振り返り
        let (bfs, sfs, m) = if short { (13.0, 11.0, (0.0, 0.0)) } else { (19.0, 15.0, (6.0, 4.0)) };
        y += m.0;
        let rows = pen.lines(&d.line, JP9, bfs, 0.0, lw);
        for (i, t) in rows.iter().enumerate() { if draw { pen.text(t, x, base_l(JP9, bfs, y + i as f32 * bfs * 1.35, bfs * 1.35), CREAM, &[]); } }
        y += rows.len() as f32 * bfs * 1.35 + 2.0;
        let rows = pen.lines(&d.next, JP7, sfs, 0.0, lw);
        for (i, t) in rows.iter().enumerate() { if draw { pen.text(t, x, base_n(JP7, &d.next, sfs, y + i as f32 * sfs * 1.448), CREAM_DIM, &[]); } }
        y += rows.len() as f32 * sfs * 1.448 + m.1 + gap;
        // メダル・称号
        if !d.pills.is_empty() {
            let (pv, ph, pfs2) = if short { (2.0, 7.0, 11.0) } else { (4.0, 11.0, 13.0) };
            let ph_ = pfs2 * 1.448 + pv * 2.0;
            let (mut px_, mut py) = (x, y);
            for p in &d.pills {
                let pw = tw(pen, &p.text, JP9, pfs2, 0.0) + ph * 2.0;
                if px_ > x && px_ + pw > x + lw { px_ = x; py += ph_ + 6.0; }
                if draw {
                    let pr = Rect::from_min_size(pos2(px_, py), vec2(pw, ph_));
                    let (bg, fg) = match p.medal { Some(1) => (hx(0xd79a68), hx(0x271b08)), Some(2) => (hx(0xd9dee4), hx(0x271b08)), Some(3) => (GOLD, hx(0x271b08)), Some(_) => (hx(0xe8e2d2), hx(0x8a8471)), None => if p.new { (GOLD, GREEN) } else { (rgba(255, 246, 228, 0.16), CREAM) } };
                    let q = if p.new && !self.notrans {
                        let k = clamp01(((self.now - self.res_at) as f32 - 0.4) / 0.5);
                        let e = bezier_spring(k);
                        pen.sub(Xf::about(pr.center().x, pr.center().y, Xf::scale(lerp(0.4, 1.0, e), lerp(0.4, 1.0, e))), clamp01(e))
                    } else { pen.clone() };
                    q.rrect(pr, ph_ * 0.5, bg);
                    label(&q, &p.text, JP9, pfs2, 0.0, pr.left() + ph, pr.top() + pv, fg, &[]);
                }
                px_ += pw + 6.0;
            }
            y = py + ph_;
        }
        y += gap;
        // 順位の流れ（低い画面では出さない）
        if !short {
            if draw {
                let cr = Rect::from_min_size(pos2(x, y), vec2(lw, 44.0));
                pen.rrect(cr, 8.0, ink(0.4));
                let pts: Vec<Pos2> = d.chart.iter().map(|(u, v)| pos2(x + u * lw, y + v / 56.0 * 44.0)).collect();
                if pts.len() >= 2 { pen.clip(cr).line(&pts, 3.0, GOLD); }
                let fy = y + 44.0 + 3.0;
                label(pen, &d.pass, NUM7, 12.0, 0.0, x, fy, CREAM_DIM, &[]);
                if let Some(m) = &d.next_medal { let mw = tw(pen, m, NUM7, 12.0, 0.0); label(pen, m, NUM7, 12.0, 0.0, x + lw - mw, fy, CREAM_DIM, &[]); }
            }
            y += 44.0 + 3.0 + nlh(NUM7, &d.pass, 12.0) + gap;
        }
        y - gap - y0
    }
    /// 結果のボタン（もう一度・コース・キャラを変える）。並ばなければ縦に積む。高さを返す（hits が None なら測るだけ）
    #[allow(clippy::too_many_arguments)]
    fn res_buttons(&self, pen: &Pen, hits: Option<(&mut Hits, &mut Tw)>, d: &ResultData, x: f32, y: f32, lw: f32, short: bool) -> f32 {
        let z = if short { PRIMARY_SHORT } else { PRIMARY };
        let key = if self.touch { None } else { Some("Space") };
        let (aw, ah) = primary_size(pen, key, &d.again, z);
        let menu = if d.menu.is_empty() { "コース・キャラを変える" } else { d.menu.as_str() };
        let (gw, gh) = ghost_size(pen, menu, short);
        let (ar, gr, bottom);
        if aw + 10.0 + gw <= lw {
            let rh = ah.max(gh);
            ar = Rect::from_min_size(pos2(x, y + (rh - ah) * 0.5), vec2(aw, ah));
            gr = Rect::from_min_size(pos2(x + aw + 10.0, y + (rh - gh) * 0.5), vec2(gw, gh));
            bottom = y + rh;
        } else {
            ar = Rect::from_min_size(pos2(x, y), vec2(aw, ah));
            gr = Rect::from_min_size(pos2(x, y + ah + 10.0), vec2(gw, gh));
            bottom = y + ah + 10.0 + gh;
        }
        if let Some((hits, tw)) = hits {
            primary(pen, hits, Act::Again, ar, key, &d.again, z, tw);
            ghost(pen, hits, Act::Menu, gr, menu, if short { 12.0 } else { 14.0 }, None, LINE);
            // 選んでいる項目の輪（キー・パッドで触っている間）
            if self.ring && self.shown == Some(Scr::Result) {
                let fr = if self.res_focus == 0 { ar } else { gr };
                pen.rframe(fr.expand(5.0), fr.height() * 0.5 + 5.0, 3.0, CREAM);
            }
        }
        bottom - y
    }

    /// 掲示板。scroll: 入りきらないときに中を送れる箱にする（横に並ぶ配置。縦持ちは画面ごと送る）
    #[allow(clippy::too_many_arguments)]
    fn draw_board(&mut self, pen: &Pen, d: &ResultData, r: Rect, full: f32, narrow: bool, short: bool, scroll: bool) {
        pen.box_shadow(r, 12.0, 0.0, 20.0, 60.0, rgba(0, 0, 0, 0.45));
        pen.rrect(r, 12.0, GREEN);
        pen.rframe(r, 12.0, 3.0, hx(0xe8dcc0));
        let inner = r.shrink(3.0);
        let at = if scroll { self.board_scroll.place(inner, full - r.height()) } else { 0.0 };
        let p = pen.clip(inner);
        let rr = Rect::from_min_size(pos2(r.left(), r.top() - at), vec2(r.width(), full));
        let mut hits = std::mem::take(&mut self.hits);
        self.board(&p, Some(&mut hits), d, rr, narrow, short);
        self.hits = hits;
        if scroll { more_cue(pen, &self.board_scroll, GREEN, CREAM); }
    }
    /// 掲示板（表と、カップのひとこと）。中身の高さを返す
    fn board(&self, pen: &Pen, hits: Option<&mut Hits>, d: &ResultData, r: Rect, narrow: bool, short: bool) -> f32 {
        let draw = hits.is_some();
        let (pv, ph) = if short { (4.0, 6.0) } else { (8.0, 10.0) };
        let (fs, cp_v, cp_h) = if short { (11.0, 1.0, 3.0) } else if narrow { (13.0, 4.0, 5.0) } else { (14.0, 4.0, 5.0) };
        let (pl_fs, chip, chip_fs) = if short { (13.0, 16.0, 10.0) } else { (19.0, 22.0, 13.0) };
        let x0 = r.left() + 3.0 + ph;
        let tw_all = r.width() - 6.0 - ph * 2.0;
        // 列の幅（自動の表: 名前の列が残りを取る）
        let tm_w = d.rows.iter().filter_map(|x| x.time.as_ref()).map(|s| tw(pen, s, NUM5, fs, 0.0)).fold(0.0, f32::max) + cp_h * 2.0;
        let has_tm = d.rows.iter().any(|x| x.time.is_some());
        let mg_w = d.rows.iter().map(|x| tw(pen, &x.margin, JP7, if short { fs } else { 13.0 }, 0.0)).fold(0.0, f32::max) + cp_h * 2.0;
        let mg_w = mg_w.max(46.0) + if has_tm { 8.0 } else { 0.0 };
        let pt_w = d.rows.iter().filter_map(|x| x.pts.as_ref()).map(|s| tw(pen, s, NUM7, fs, 0.0)).fold(0.0, f32::max) + cp_h * 2.0;
        let has_pt = d.rows.iter().any(|x| x.pts.is_some());
        let pl_w = 28.0_f32.max(tw(pen, "8", NUM7, pl_fs, 0.0) + cp_h * 2.0);
        let ch_w = chip + cp_h * 2.0;
        let nm_w = (tw_all - pl_w - ch_w - if has_tm { tm_w } else { 0.0 } - mg_w - if has_pt { pt_w } else { 0.0 }).max(d.rows.iter().map(|x| tw(pen, &x.name, JP7, fs, 0.0)).fold(0.0, f32::max) + cp_h * 2.0);
        let row_h = (pl_fs * 1.21).max(chip).max(fs * 1.448) + cp_v * 2.0;
        let mut y = r.top() + 3.0 + pv;
        for (k, row) in d.rows.iter().enumerate() {
            if draw {
                let (op, dx) = if self.notrans { (1.0, 0.0) } else {
                    let t = clamp01(((self.now - self.res_at) as f32 - 0.05 * k as f32) / 0.42);
                    let e = ease(t);
                    (e, 14.0 * (1.0 - e))
                };
                let p = pen.sub(Xf::tr(dx, 0.0), op);
                let rr = Rect::from_min_size(pos2(x0, y), vec2(tw_all, row_h));
                if row.me { p.rect(rr, rgba(255, 207, 77, 0.2)); }
                if k + 1 < d.rows.len() { p.rect(Rect::from_min_size(pos2(x0, y + row_h), vec2(tw_all, 1.0)), rgba(255, 246, 228, 0.13)); }
                let cy = y + row_h * 0.5;
                let mut cx = x0;
                let t = pen.t(&row.place, NUM7, pl_fs, 0.0);
                p.text(&t, cx + pl_w * 0.5 - t.w * 0.5, base_n(NUM7, &row.place, pl_fs, cy - nlh(NUM7, &row.place, pl_fs) * 0.5), GOLD, &[]);
                cx += pl_w;
                if let Some(g) = row.gate { chip_draw(&p, Rect::from_min_size(pos2(cx + cp_h, cy - chip * 0.5), vec2(chip, chip)), g, chip_fs, 0.0); }
                cx += ch_w;
                label(&p, &row.name, JP7, fs, 0.0, cx + cp_h, cy - nlh(JP7, &row.name, fs) * 0.5, CREAM, &[]);
                cx += nm_w;
                if has_tm {
                    if let Some(s) = &row.time { let w_ = tw(pen, s, NUM5, fs, 0.0); label(&p, s, NUM5, fs, 0.0, cx + tm_w - cp_h - w_, cy - nlh(NUM5, s, fs) * 0.5, CREAM, &[]); }
                    cx += tm_w;
                }
                let mfs = if short { fs } else { 13.0 };
                let w_ = tw(pen, &row.margin, JP7, mfs, 0.0);
                if has_tm && !row.margin.is_empty() {
                    // 着差（ハナ・クビ・大差…）は、上の走者との差。2 人の行の境目にまたがる札にして、どの 2 人の間の差かを並びで見せる
                    let (pw, phh) = (w_ + 12.0, mfs * 1.448);
                    let pr = Rect::from_min_size(pos2(cx + mg_w - cp_h + 2.0 - pw, y - 0.5 - phh * 0.5), vec2(pw, phh));
                    p.rrect(pr, phh * 0.5, hx(0x0a2f23));
                    if !short { p.rframe(pr, phh * 0.5, 1.0, rgba(255, 246, 228, 0.25)); }
                    label(&p, &row.margin, JP7, mfs, 0.0, pr.center().x - w_ * 0.5, pr.top(), rgba(255, 246, 228, 0.85), &[]);
                } else {
                    // カップの最終結果では、この列は各戦の着順（行の中）
                    label(&p, &row.margin, JP7, mfs, 0.0, cx + mg_w - cp_h - w_, cy - nlh(JP7, &row.margin, mfs) * 0.5, CREAM_DIM, &[]);
                }
                cx += mg_w;
                if let Some(s) = &row.pts { let w_ = tw(pen, s, NUM7, fs, 0.0); label(&p, s, NUM7, fs, 0.0, cx + pt_w - cp_h - w_, cy - nlh(NUM7, s, fs) * 0.5, GOLD, &[]); }
            }
            y += row_h + if k + 1 < d.rows.len() { 1.0 } else { 0.0 };
        }
        // カップのひとこと
        if !d.cup.is_empty() {
            let rows = pen.lines(&d.cup, JP7, 13.0, 0.0, tw_all - 8.0);
            y += 6.0;
            for (i, t) in rows.iter().enumerate() { if draw { pen.text(t, x0 + 4.0, base_n(JP7, &d.cup, 13.0, y + i as f32 * 13.0 * 1.448), CREAM_DIM, &[]); } }
            y += rows.len() as f32 * 13.0 * 1.448 + 2.0;
        } else { y += 8.0; } // 空の #res-cup も上下の padding（6 + 2）の高さがある
        // 下の余白と枠
        y += pv + 3.0;
        y - r.top()
    }

    // ---- きろく ----
    fn records_ui(&mut self, pen: &Pen, r: Rect) {
        let k = if self.notrans { if self.records.is_some() { 1.0 } else { 0.0 } } else { self.rec_fade.get(self.now) };
        if k <= 0.0 && self.records.is_none() { self.rec_scroll.off(); return; }
        let on = self.records.is_some();
        let d = self.rec_data.clone();
        // 札は安全域の内側に置く
        let sf = self.safe;
        let a = Rect::from_min_max(pos2(r.left() + sf[3], r.top() + sf[0]), pos2(r.right() - sf[1], r.bottom() - sf[2]));
        let (w, h) = (a.width(), a.height());
        let p = pen.with_alpha(k);
        p.rect(r, rgba(8, 20, 16, 0.55));
        if on { self.hits.block(r); }
        let short = r.height() <= 520.0;
        let cw = (640.0_f32.min(w * 0.94)).clamp(430.0_f32.min(w * 0.92), w * 0.92);
        let (pt, ph, pb) = if short { (10.0, 16.0, 10.0) } else if r.width() <= 480.0 { (16.0, 14.0, 18.0) } else { (20.0, 26.0, 22.0) };
        let iw = cw - ph * 2.0;
        // 見出しと「とじる」は送りの外（送らなくても見える）。中身（コース・称号・ひとこと）だけを指・ホイールで送る
        let (h2, mb) = if short { (18.0, 4.0) } else { (28.0, 10.0) };
        let head_h = nlh(NUM6, "R", 13.0) + 2.0 + h2 * 1.448 + mb;
        let z = if short { PRIMARY_SHORT } else { PRIMARY };
        let (_, bh) = primary_size(pen, None, "とじる", z);
        let foot_h = 12.0 + bh;
        let body_full = self.records_body(&p, false, &d, 0.0, 0.0, iw, r.width());
        let full = pt + head_h + body_full + foot_h + pb;
        let ch = full.min(h * if short { 0.96 } else { 0.94 });
        let card = Rect::from_center_size(a.center(), vec2(cw, ch));
        let sk = if self.notrans { if on { 1.0 } else { 0.0 } } else { self.rec_k.get(self.now) };
        let sc = lerp(0.98, 1.0, sk);
        let q = p.sub(Xf::about(card.center().x, card.center().y, Xf::tr(0.0, 14.0 * (1.0 - sk)).then(Xf::scale(sc, sc))), 1.0);
        q.box_shadow(card, 22.0, 0.0, 30.0, 70.0, rgba(0, 0, 0, 0.5));
        q.rrect(card, 22.0, PAPER);
        let cx = card.center().x;
        let mut y = card.top() + pt;
        let ew = eyebrow_w(pen, "RECORDS");
        eyebrow(&q, "RECORDS", cx - ew * 0.5, y, hx(0xb0732e));
        y += nlh(NUM6, "R", 13.0) + 2.0;
        let t = pen.t("きろく", DISP, h2, 0.0);
        q.text(&t, cx - t.w * 0.5, base_n(DISP, "き", h2, y), GREEN, &[]);
        let view = Rect::from_min_max(pos2(card.left(), card.top() + pt + head_h), pos2(card.right(), card.bottom() - pb - foot_h));
        let at = self.rec_scroll.place(view, body_full - view.height());
        self.records_body(&q.clip(view), true, &d, card.left() + ph, view.top() - at, iw, r.width());
        more_cue(&q, &self.rec_scroll, PAPER, GREEN);
        // とじる（幅いっぱい）
        let (mut hits, mut tw) = (std::mem::take(&mut self.hits), std::mem::take(&mut self.tw));
        let mark = hits.mark();
        primary(&q, &mut hits, Act::RecClose, Rect::from_min_size(pos2(card.left() + ph, view.bottom() + 12.0), vec2(iw, bh)), None, "とじる", z, &mut tw);
        if !on { hits.truncate(mark); }
        (self.hits, self.tw) = (hits, tw);
    }
    /// きろくの中身（コースごと・称号・ひとこと）。高さを返す（draw が false なら測るだけ）
    #[allow(clippy::too_many_arguments)]
    fn records_body(&self, pen: &Pen, draw: bool, d: &RecordsData, x: f32, y0: f32, iw: f32, w: f32) -> f32 {
        let mut y = y0;
        let cx = x + iw * 0.5;
        // コースごと
        let cols = if w <= 560.0 { 1 } else { 3 };
        let cw = (iw - 8.0 * (cols as f32 - 1.0)) / cols as f32;
        // 狭い画面（1 列）は 1 行に並べる（名前・自己ベスト・メダル）。縦に 3 段積むと、スマホの縦持ちで送らないと入らない
        let one = cols == 1;
        let card_h = if one { 4.0 + 20.0 + 14.0 * 1.448 } else { 4.0 + 16.0 + 14.0 * 1.448 + 3.0 + 13.0 * 1.21 + 20.93 };
        for (i, c) in d.courses.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let r = Rect::from_min_size(pos2(x + col as f32 * (cw + 8.0), y + row as f32 * (card_h + 8.0)), vec2(cw, card_h));
            if draw {
                pen.rrect(r, 12.0, WHITE);
                pen.rframe(r, 12.0, 2.0, rgba(15, 61, 46, 0.14));
                let (bg, fg) = match c.medal { 1 => (hx(0xd79a68), hx(0x271b08)), 2 => (hx(0xd9dee4), hx(0x271b08)), 3 => (GOLD, hx(0x271b08)), _ => (hx(0xe8e2d2), hx(0x8a8471)) };
                let mw = tw(pen, &c.medal_name, JP9, 11.0, 0.0) + 16.0;
                let mh = 11.0 * 1.448 + 2.0;
                let bw = tw(pen, &c.best, NUM5, 13.0, 0.0);
                // メダルの札の左上と、自己ベストの左上
                let (mp, bp);
                if one {
                    let cy = r.center().y;
                    label(pen, &c.name, JP7, 14.0, 0.0, r.left() + 14.0, cy - 14.0 * 1.448 * 0.5, GREEN, &[]);
                    mp = pos2(r.right() - 12.0 - mw, cy - mh * 0.5);
                    bp = pos2(mp.x - 10.0 - bw, cy - nlh(NUM5, &c.best, 13.0) * 0.5);
                } else {
                    // .card の text-align: center が .rec-card の left より後ろにあるので、中身は中央寄せ（今の版のとおり）
                    let (cx, ty) = (r.center().x, r.top() + 10.0);
                    let nw = tw(pen, &c.name, JP7, 14.0, 0.0);
                    label(pen, &c.name, JP7, 14.0, 0.0, cx - nw * 0.5, ty, GREEN, &[]);
                    bp = pos2(cx - bw * 0.5, ty + 14.0 * 1.448 + 3.0);
                    mp = pos2(cx - mw * 0.5, ty + 14.0 * 1.448 + 3.0 + 13.0 * 1.21 + 3.0);
                }
                label(pen, &c.best, NUM5, 13.0, 0.0, bp.x, bp.y, hx(0x4b6a5d), &[]);
                let mr = Rect::from_min_size(mp, vec2(mw, mh));
                pen.rrect(mr, mh * 0.5, bg);
                label(pen, &c.medal_name, JP9, 11.0, 0.0, mp.x + 8.0, mp.y + 1.0, fg, &[]);
            }
        }
        let rows = d.courses.len().div_ceil(cols);
        y += rows as f32 * card_h + (rows.saturating_sub(1)) as f32 * 8.0 + 10.0;
        // 称号（2 列）
        let bw = (iw - 5.0) / 2.0;
        let mut i = 0;
        while i < d.badges.len() {
            let pair = &d.badges[i..(i + 2).min(d.badges.len())];
            let hs: Vec<f32> = pair.iter().map(|b| 4.0 + 10.0 + 13.0 * 1.448 + pen.lines(&b.how, JP7, 12.0, 0.0, bw - 22.0).len() as f32 * 12.0 * 1.448).collect();
            let rh = hs.iter().copied().fold(0.0, f32::max);
            for (j, b) in pair.iter().enumerate() {
                let r = Rect::from_min_size(pos2(x + j as f32 * (bw + 5.0), y), vec2(bw, rh));
                if draw {
                    let (bg, bd, fg) = if b.got { (GOLD, hx(0xd4a12e), GREEN) } else { (WHITE, rgba(15, 61, 46, 0.14), hx(0x8a8471)) };
                    pen.rrect(r, 10.0, bg);
                    pen.rframe(r, 10.0, 2.0, bd);
                    let (cx, ty) = (r.center().x, r.top() + 7.0);
                    let nw = tw(pen, &b.name, JP9, 13.0, 0.0);
                    label(pen, &b.name, JP9, 13.0, 0.0, cx - nw * 0.5, ty, fg, &[]);
                    for (k, t) in pen.lines(&b.how, JP7, 12.0, 0.0, bw - 22.0).iter().enumerate() { pen.text(t, cx - t.w * 0.5, base_n(JP7, &b.how, 12.0, ty + 13.0 * 1.448 + k as f32 * 12.0 * 1.448), fg, &[]); }
                }
            }
            y += rh + 5.0;
            i += 2;
        }
        y -= 5.0;
        // 殿堂（育成で引退した走者。賞金の多い順）
        y += 12.0;
        let ht = "殿堂（育成で引退した走者）";
        if draw { let t = pen.t(ht, JP9, 13.0, 0.0); pen.text(&t, cx - t.w * 0.5, base_n(JP9, ht, 13.0, y), GREEN, &[]); }
        y += 13.0 * 1.448 + 6.0;
        if d.hall.is_empty() {
            let s = "まだいません。育成で引退まで走ると、ここに残ります";
            for (k, t) in pen.lines(s, JP7, 12.0, 0.0, iw).iter().enumerate() { if draw { pen.text(t, cx - t.w * 0.5, base_n(JP7, s, 12.0, y + k as f32 * 12.0 * 1.448), hx(0x8a8471), &[]); } }
            y += pen.lines(s, JP7, 12.0, 0.0, iw).len() as f32 * 12.0 * 1.448;
        }
        for (k, e) in d.hall.iter().enumerate() {
            let rh = 8.0 + 14.0 * 1.448 + 2.0 + 12.0 * 1.448 + 8.0;
            let r = Rect::from_min_size(pos2(x, y), vec2(iw, rh));
            if draw {
                pen.rrect(r, 10.0, if k == 0 { hx(0xfff3c9) } else { WHITE });
                pen.rframe(r, 10.0, 2.0, if k == 0 { hx(0xd4a12e) } else { rgba(15, 61, 46, 0.14) });
                let no = (k + 1).to_string();
                label(pen, &no, NUM7, 16.0, 0.0, r.left() + 12.0, r.top() + 8.0, hx(0xb0732e), &[]);
                let nx = r.left() + 36.0;
                label(pen, &e.name, JP9, 14.0, 0.0, nx, r.top() + 8.0, GREEN, &[]);
                let tw_ = tw(pen, &e.title, JP9, 11.0, 0.0) + 14.0;
                let tr = Rect::from_min_size(pos2(r.right() - 10.0 - tw_, r.top() + 8.0 + (14.0 * 1.448 - 11.0 * 1.448 - 2.0) * 0.5), vec2(tw_, 11.0 * 1.448 + 2.0));
                pen.rrect(tr, tr.height() * 0.5, GOLD);
                label(pen, &e.title, JP9, 11.0, 0.0, tr.left() + 7.0, tr.top() + 1.0, GREEN, &[]);
                let ly = r.top() + 8.0 + 14.0 * 1.448 + 2.0;
                label(pen, &e.line, NUM5, 12.0, 0.0, nx, ly, hx(0x4b6a5d), &[]);
                // 育てた能力（右。色の丸と段）
                let mut sx = r.right() - 10.0;
                for s in sim::items::Stat::ALL.iter().rev() {
                    let ls = e.levels[s.i()].to_string();
                    let lw_ = tw(pen, &ls, NUM7, 12.0, 0.0);
                    sx -= lw_;
                    label(pen, &ls, NUM7, 12.0, 0.0, sx, ly, hx(0x4b6a5d), &[]);
                    sx -= 12.0;
                    pen.circle(pos2(sx + 4.5, ly + 12.0 * 1.448 * 0.5), 4.5, crate::hud::stat_c(*s));
                    sx -= 6.0;
                }
            }
            y += rh + 5.0;
        }
        // ひとこと
        y += 8.0;
        let rows = pen.lines(&d.note, JP7, 12.0, 0.0, iw);
        for (k, t) in rows.iter().enumerate() { if draw { pen.text(t, cx - t.w * 0.5, base_n(JP7, &d.note, 12.0, y + k as f32 * 12.0 * 1.448), hx(0x6b6652), &[]); } }
        y += rows.len() as f32 * 12.0 * 1.448;
        y - y0
    }

    // ---- 切り替えの幕（8 枠の色の帯が閉じて開く） ----
    fn wipe_ui(&mut self, pen: &Pen, r: Rect) {
        let Some(on) = self.wipe_on else { return };
        if self.reduce_motion { return; }
        self.hits.block(r);
        const COLS: [u32; 8] = [0xf7f3e8, 0x25262b, 0xe23b35, 0x2f6fd8, 0xf5cc2a, 0x2f9e5b, 0xf28a24, 0xf39ac0];
        let bw = r.width() / 8.0;
        for (i, c) in COLS.iter().enumerate() {
            let (k, from_top) = match self.wipe_out {
                None => (ease(clamp01(((self.now - on) as f32 - 0.02 * i as f32) / 0.42)), true),
                Some(o) => (1.0 - ease(clamp01(((self.now - o) as f32 - 0.02 * i as f32) / 0.5)), false),
            };
            if k <= 0.0 { continue; }
            let hgt = r.height() * k;
            let y = if from_top { r.top() } else { r.bottom() - hgt };
            pen.rect(Rect::from_min_size(pos2(r.left() + i as f32 * bw, y), vec2(bw + 0.5, hgt)), hx(*c));
        }
    }

    // ---- 一時停止の Ⅱ ----
    fn pause_button(&mut self, pen: &Pen, r: Rect) {
        // 押せる箱（56px）は見た目の丸（44px）より大きい。端の安全域の内側（配置は sim::menu）
        let (hit, c) = menu::pause_box(r.width(), self.safe);
        self.hits.add(Act::Pause, box_rect(hit), Shape::Rect);
        let c = pos2(c[0], c[1]);
        pen.circle(c, menu::PAUSE_DOT * 0.5, GLASS);
        for dx in [-4.5, 4.5] { pen.rrect(Rect::from_center_size(c + vec2(dx, 0.0), vec2(4.0, 15.0)), 2.0, CREAM); }
    }

    // ---- 指のスティックとボタン ----
    fn touch_ui(&mut self, pen: &Pen, r: Rect) {
        let (w, h) = (r.width(), r.height());
        let short = h <= 520.0;
        let v = self.touch_view;
        // 土台は入力の部品が持つ場所（指に付いてくる）。触る前は左下の、安全域の内側
        let (sx, sy) = v.stick.unwrap_or((110.0 + self.safe[3], h - 150.0 - self.safe[2]));
        let sp = pen.with_alpha(if v.stick_on { 1.0 } else { 0.55 });
        sp.circle(pos2(sx, sy), 60.0, ink(0.25));
        sp.circle_line(pos2(sx, sy), 59.0, 2.0, rgba(255, 246, 228, 0.4));
        sp.circle(pos2(sx + v.knob.0, sy + v.knob.1), 26.0, rgba(255, 246, 228, 0.85));
        let fs = if short { 12.0 } else { 14.0 };
        let (dr, jr) = self.touch_buttons(w, h);
        // スタミナは「ダッシュ」を囲む輪（使うボタンのそばに出す。下の棒は縦持ちでボタンに隠れる）。上から時計回りに減る
        let st = self.hud.stamina();
        {
            let (c, rad, wd) = (dr.center(), dr.width() * 0.5 + 5.5, 5.0);
            let edge = if st.low && !self.notrans {
                let t = ((self.now - st.low_at) as f32 / 0.42).rem_euclid(2.0);
                rgba(255, 246, 228, 0.55).lerp(hx(0xff8a5c), ease_io(if t < 1.0 { t } else { 2.0 - t }))
            } else { rgba(255, 246, 228, 0.55) };
            pen.circle_line(c, rad, wd + 3.0, edge);
            pen.circle_line(c, rad, wd, ink(0.78));
            let at = |k: f32, rr: f32| { let a = (-90.0 + 360.0 * k).to_radians(); c + vec2(a.cos(), a.sin()) * rr };
            if st.k > 0.0 {
                let n = ((72.0 * st.k).ceil() as usize).max(1);
                let pts: Vec<Pos2> = (0..=n).map(|i| at(st.k * i as f32 / n as f32, rad)).collect();
                pen.line(&pts, wd, if st.out { hx(0x7a8a82) } else if st.low { hx(0xff8a45) } else { hx(0x4fd58a) });
            }
            // 残りが少ないの境（棒の 22% の線と同じ）
            pen.line(&[at(0.22, rad - wd * 0.5), at(0.22, rad + wd * 0.5)], 2.0, rgba(255, 246, 228, 0.6));
        }
        // 「こうげき」（武器を持っている間だけ）: 武器の名前・残りの回数。輪は残りの時間、ためている間は金色に満ちる
        if let Some(a) = self.hud.armed {
            let ar = box_rect(menu::touch_attack(w, h, self.safe));
            let (c, rad) = (ar.center(), ar.width() * 0.5);
            let p = if v.attack { pen.sub(Xf::about(c.x, c.y, Xf::scale(0.92, 0.92)), 1.0) } else { pen.clone() };
            let full = a.charge >= 1.0;
            p.circle(c, rad, if full { GOLD } else if v.attack { rgba(255, 207, 77, 0.8) } else { rgba(230, 64, 47, 0.62) });
            p.circle_line(c, rad - 1.0, 2.0, rgba(255, 246, 228, 0.8));
            let at = |k: f32, rr: f32| { let an = (-90.0 + 360.0 * k).to_radians(); c + vec2(an.cos(), an.sin()) * rr };
            let ring = |k: f32, wd: f32, col: C| { if k > 0.0 { let n = ((72.0 * k).ceil() as usize).max(1); let pts: Vec<Pos2> = (0..=n).map(|i| at(k * i as f32 / n as f32, rad + 5.0)).collect(); pen.line(&pts, wd, col); } };
            ring(1.0, 5.0, ink(0.7));
            ring(a.time, 4.0, if a.time < 0.22 { hx(0xff8a5c) } else { CREAM });
            if a.charge > 0.0 { ring(a.charge, 6.0, GOLD); }
            let name = a.weapon.name();
            let nfs = if name.chars().count() > 2 { fs - 2.0 } else { fs + 2.0 };
            let t = pen.t(name, JP9, nfs, 0.0);
            let fg = if full || v.attack { GREEN } else { WHITE };
            p.text(&t, c.x - t.w * 0.5, base_n(JP9, name, nfs, c.y - nfs * 1.448 * 0.5 - 5.0), fg, &[]);
            for k in 0..a.weapon.uses() {
                let dc = pos2(c.x + (k as f32 - (a.weapon.uses() - 1) as f32 * 0.5) * 10.0, c.y + nfs * 0.724 + 3.0);
                if k < a.uses { p.circle(dc, 3.5, fg); } else { p.circle_line(dc, 3.0, 1.5, fg); }
            }
        }
        for (rr, s, on, bg) in [(dr, "ダッシュ", v.dash, rgba(255, 207, 77, 0.42)), (jr, "ジャンプ", v.jump, ink(0.42))] {
            let p = if on { pen.sub(Xf::about(rr.center().x, rr.center().y, Xf::scale(0.92, 0.92)), 1.0) } else { pen.clone() };
            p.circle(rr.center(), rr.width() * 0.5, if on { rgba(255, 207, 77, 0.8) } else { bg });
            p.circle_line(rr.center(), rr.width() * 0.5 - 1.0, 2.0, rgba(255, 246, 228, 0.7));
            let t = pen.t(s, JP9, fs, 0.0);
            p.text(&t, rr.center().x - t.w * 0.5, base_n(JP9, s, fs, rr.center().y - fs * 1.448 * 0.5), if on { GREEN } else { CREAM }, &[]);
        }
        // 息切れ・風よけ（棒の上の字と同じ）は「ダッシュ」の字の下
        if !st.label.is_empty() {
            let t = pen.t(&st.label, JP9, 10.0, 0.0);
            pen.text(&t, dr.center().x - t.w * 0.5, base_n(JP9, &st.label, 10.0, dr.center().y + fs * 0.724 + 1.0), WHITE, &[TS(0.0, 1.0, 3.0, ink(0.9))]);
        }
    }

    /// 指のスティックとボタンの当たり（game-ui の touch_pad へ渡す）。.tbtn は丸、スティックは左半分の安全域の内側で始まる
    pub fn touch_layout(&self, w: f32, h: f32) -> (Vec<game_ui::touch_pad::Button>, game_ui::touch_pad::Stick) {
        use game_ui::touch_pad::{Button, Shape, Stick};
        let (d, j) = self.touch_buttons(w, h);
        let c = |r: Rect| Shape::Circle { c: r.center(), r: r.width() * 0.5 };
        let mut buttons = vec![Button { name: "dash", shape: c(d) }, Button { name: "jump", shape: c(j) }];
        // 「こうげき」は武器を持っている間だけ押せる
        if self.hud.armed.is_some() { buttons.push(Button { name: "attack", shape: c(box_rect(menu::touch_attack(w, h, self.safe))) }); }
        (buttons,
         Stick { area: box_rect(menu::stick_area(w, h, self.safe)), radius: 60.0, four_way: false })
    }
    /// 指のボタンの場所（ダッシュ, ジャンプ）
    pub fn touch_buttons(&self, w: f32, h: f32) -> (Rect, Rect) {
        let (d, j) = menu::touch_buttons(w, h, self.safe);
        (box_rect(d), box_rect(j))
    }
}

/// sim::menu の箱 [x, y, 幅, 高さ]
pub fn box_rect(b: menu::Box4) -> Rect { Rect::from_min_size(pos2(b[0], b[1]), vec2(b[2], b[3])) }

/// .chip（枠番の四角）
pub fn chip_draw(pen: &Pen, r: Rect, gate: usize, fs: f32, shadow: f32) {
    if shadow > 0.0 { pen.box_shadow(r, 6.0, 0.0, 1.0, 4.0, ink(0.5)); }
    pen.rrect(r, 6.0, GATE_COLOR[gate - 1]);
    let s = gate.to_string();
    let t = pen.t(&s, NUM7, fs, 0.0);
    pen.text(&t, r.center().x - t.w * 0.5, base_n(NUM7, &s, fs, r.center().y - nlh(NUM7, &s, fs) * 0.5), GATE_TEXT[gate - 1], &[]);
}

impl Screens {
    /// 確かめ用: 画面に出ている文字（rust/compare が今の版の DOM の中身と比べる）
    pub fn dump(&self) -> serde_json::Value {
        use serde_json::json;
        let scr = |s: Option<Scr>| match s { Some(Scr::Loading) => "loading", Some(Scr::Title) => "title", Some(Scr::Select) => "select", Some(Scr::Intro) => "intro", Some(Scr::Result) => "result", Some(Scr::Photo) => "photo", None => "" };
        let r = &self.res;
        let (td, tj) = menu::touch_buttons(self.size.0, self.size.1, self.safe);
        let touch_boxes = json!({ "dash": td, "jump": tj, "attack": self.hud.armed.map(|_| menu::touch_attack(self.size.0, self.size.1, self.safe)), "stick": menu::stick_area(self.size.0, self.size.1, self.safe) });
        json!({
            "show": scr(self.shown), "racing": self.racing, "finishing": self.finishing, "paused": self.paused, "touch": self.touch, "records": self.records.is_some(), "wipe": self.wipe_on.is_some(),
            "ring": self.ring, "safe": self.safe,
            "touchView": { "stick": self.touch_view.stick.map(|p| [p.0, p.1]), "on": self.touch_view.stick_on, "knob": [self.touch_view.knob.0, self.touch_view.knob.1] },
            "touchButtons": touch_boxes,
            "scroll": { "rec": self.rec_scroll.dump(), "pause": self.pause.scroll.dump(), "res": self.res_scroll.dump(), "board": self.board_scroll.dump(), "strip": self.strip.dump() },
            "load": { "text": self.load_text, "k": self.load_fill.target(), "failed": self.load_failed },
            "title": { "stats": plain(&self.title_stats), "mute": if self.muted { "音: オフ" } else { "音: オン" } },
            "select": if self.sel.courses.is_empty() { json!({ "mode": [], "courses": [], "diff": -1, "heroes": 0, "heroOn": -1, "name": "", "best": "" }) } else { json!({
                "mode": [format!("単発レース{}", if self.sel.cup_mode || self.sel.career_mode { "" } else { "*" }), format!("{}{}", self.sel.cup_label, if self.sel.cup_mode { "*" } else { "" }), format!("{}{}", self.sel.career_label, if self.sel.career_mode { "*" } else { "" })],
                "career": if self.sel.career_mode { json!({ "grade": self.sel.career.grade, "race": self.sel.career.race, "sub": self.sel.career.sub, "rivals": self.sel.career.rivals, "record": self.sel.career.record, "levels": self.sel.career.levels, "running": self.sel.career.running }) } else { serde_json::Value::Null },
                "courses": self.sel.courses.iter().map(|c| format!("{}|{}|{}{}", c.best.clone().unwrap_or_default(), c.name, c.blurb, if c.on { "*" } else { "" })).collect::<Vec<_>>(),
                "diff": self.sel.diff, "heroes": self.heroes.len(), "heroOn": self.sel.hero_on, "name": self.sel.name, "best": self.sel.best,
            }) },
            "intro": self.intro,
            "photo": self.photo_res,
            "result": {
                "eyebrow": r.eyebrow, "place": r.place, "unit": r.unit, "win": r.win, "time": format!("{}{}", r.time, r.time_em.as_ref().map_or(String::new(), |e| e.0.clone())),
                "rival": r.rival, "line": r.line, "next": r.next, "pills": r.pills.iter().map(|p| format!("{}{}", p.text, if p.new { "*" } else { "" })).collect::<Vec<_>>(),
                "foot": format!("{}{}", r.pass, r.next_medal.clone().unwrap_or_default()),
                "rows": r.rows.iter().map(|x| format!("{}|{}|{}|{}|{}|{}{}", x.place, x.gate.map_or(String::new(), |g| g.to_string()), x.name, x.time.clone().unwrap_or_default(), x.margin, x.pts.clone().unwrap_or_default(), if x.me { "*" } else { "" })).collect::<Vec<_>>(),
                "cup": r.cup, "again": r.again, "focus": self.res_focus,
            },
            "cheer": { "count": self.count, "done": self.cheer_at.is_some(), "best": self.cheer_best, "pieces": self.confetti.len(), "calm": self.calm,
                "box": [self.cheer_box.left(), self.cheer_box.top(), self.cheer_box.width().max(0.0), self.cheer_box.height().max(0.0)], "floor": self.cheer_floor, "right": self.cheer_right },
            "rec": self.records.as_ref().map(|d| json!({
                "courses": d.courses.iter().map(|c| format!("{}|{}|{}", c.name, c.best, c.medal_name)).collect::<Vec<_>>(),
                "badges": d.badges.iter().map(|b| format!("{}|{}{}", b.name, b.how, if b.got { "*" } else { "" })).collect::<Vec<_>>(),
                "hall": d.hall.iter().map(|e| format!("{}|{}|{}|{:?}", e.name, e.title, e.line, e.levels)).collect::<Vec<_>>(),
                "note": d.note,
            })),
            "hud": self.hud.dump(),
            "pause": self.pause.dump(),
        })
    }
}
