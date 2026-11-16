// 画面の文字・枠（index.html / style.css / app.ts の DOM の部分）を egui で描く。
// 描く道具（Pen・字体・色・動き・当たり）は characters の共通部品 game-ui（vendor/rust/game-ui）。ここは魚群繚乱の画面と字体の表。
// 配色・大きさ・配置・アニメーションの数値は style.css と同じ。
pub use game_ui::*;
use crate::controls::{self, Act, Device, MoveRow, Tok};
use egui::{Area, Context, Id, LayerId, Order, Pos2, Rect, Sense, Ui, pos2, vec2};
use std::path::{Path, PathBuf};

/// 役ごとの字体: index.html が読む Web フォントが rust/game/assets/fonts/ にあればそれを、無ければ CSS の次の候補のヒラギノを使う（ファイル, ttc の番号）。
/// 太さはウェイト別のファイルで出す（Mincho の 2 は HiraMinProN-W6）。
pub const FONT_TABLE: [FontRow; 7] = [
    (Fam::Brush, "YujiSyuku-Regular.ttf", "ヒラギノ明朝 ProN.ttc", 2),
    (Fam::Mincho600, "ShipporiMinchoB1-SemiBold.ttf", "ヒラギノ明朝 ProN.ttc", 2),
    (Fam::Mincho800, "ShipporiMinchoB1-ExtraBold.ttf", "ヒラギノ明朝 ProN.ttc", 2),
    (Fam::Jp500, "ZenKakuGothicNew-Medium.ttf", "ヒラギノ角ゴシック W5.ttc", 0),
    (Fam::Jp700, "ZenKakuGothicNew-Bold.ttf", "ヒラギノ角ゴシック W6.ttc", 0),
    (Fam::Jp900, "ZenKakuGothicNew-Black.ttf", "ヒラギノ角ゴシック W8.ttc", 0),
    (Fam::Num, "DelaGothicOne-Regular.ttf", "ヒラギノ角ゴシック W9.ttc", 0),
];

/// 字体のファイル（ブラウザでは先に取っておく）
pub fn font_files() -> Vec<String> { font_files_of(&FONT_TABLE) }

/// 魚群繚乱の字体を入れる（Mac 版は rust/game/assets/fonts、ブラウザ版は先に取っておいた fonts/）
pub fn install_fonts(ctx: &Context) {
    #[cfg(not(target_arch = "wasm32"))]
    let (assets, read): (PathBuf, &dyn Fn(&Path) -> Option<Vec<u8>>) = (PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/fonts")), &|p: &Path| std::fs::read(p).ok());
    #[cfg(target_arch = "wasm32")]
    let (assets, read): (PathBuf, &dyn Fn(&Path) -> Option<Vec<u8>>) = (PathBuf::from("fonts"), &|p: &Path| crate::assets::read(p.to_str()?).ok());
    install_fonts_from(ctx, &FONT_TABLE, &assets, read);
}

// ---- 画面の流れ ----

/// app が受け取るボタンの結果
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    Start, ToggleSound, PickHero(usize), StepWeapon(i32), Go, Resume, ToTitle, ToSelect, Again, Continue, Reload,
    /// みんなで（タイトル）: 協力プレイのある元のゲームのページを開く（このゲームに協力プレイは無い。docs/gyogun-spec.md）
    Online,
    /// キャラを作って出陣（タイトル）: 同じ場所の Character Studio を開く
    MakeChar,
}

/// 「みんなで」で開くページ（浜辺の面を選んだ状態の、協力プレイの入口）
pub const ONLINE_URL: &str = "https://hyakka.yukikau.jp/?stage=hama&coop";
/// 「みんなで」の札の文言（別のゲームへ移ると分かるように、行き先の名前を出す。ChatGPT Pro のレビュー 2026-11-16）
pub const ONLINE_LABEL: &str = "百菓繚乱でみんなで遊ぶ";
/// 「キャラを作って出陣」で開くページ（同じ場所の Character Studio。Mac 版は platform::native_url が pnpm dev の場所にする）
pub const STUDIO_PATH: &str = "/character-maker";
pub const MAKE_LABEL: &str = "キャラを作って出陣";
/// 押すとゲームの外のページを開く項目と、その行き先
pub fn link_of(a: UiAction) -> Option<&'static str> { match a { UiAction::Online => Some(ONLINE_URL), UiAction::MakeChar => Some(STUDIO_PATH), _ => None } }

/// .screen の種類（show の引数）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen { Loading, Title, Select, Result, Gameover, Pause }
const SCREENS: [Screen; 6] = [Screen::Loading, Screen::Title, Screen::Select, Screen::Result, Screen::Gameover, Screen::Pause];

fn scr_i(s: Screen) -> usize { SCREENS.iter().position(|x| *x == s).unwrap() }

/// 武将選びの札
#[derive(Clone)]
pub struct HeroCard { pub name: String, pub select: Option<Tex> }
/// 弾道の図の種類（PATH_SVG）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind { Straight, Spread, Curve }
#[derive(Clone)]
pub struct PathCard { pub kind: PathKind, pub name: String, pub desc: String }
/// 武器の札（syncWeapon）
#[derive(Clone)]
pub struct WeaponCard { pub name: String, pub image: Option<Tex>, pub path: Option<PathCard> }
/// 結果の 1 行（best は ''・'NEW'・'最高 N'）
#[derive(Clone)]
pub struct ResultRow { pub label: String, pub value: String, pub best: String }
/// 誉れ 1 つ
#[derive(Clone)]
pub struct Honor { pub name: String, pub on: bool, pub fresh: bool }
/// 結果の画面の中身（app.ts の showResult が作る）
#[derive(Clone, Default)]
pub struct ResultData {
    pub eyebrow: String, pub title: String, pub rank: String, pub rows: Vec<ResultRow>, pub honors: Vec<Honor>,
    /// 主な数（得点）の行。自己ベストの祝いで数え上げる（rows の番号）
    pub main: Option<usize>,
}
/// 紙吹雪の色（魚群繚乱の配色から: 金・淡い金・生成り・紅・薄紅・橙）
const CONFETTI: [C; 6] = [GOLD, hx(0xfff3b0), CREAM, RED, hx(0xff6a7a), hx(0xff7a3a)];
/// 紙吹雪の枚数（和の画面に合わせて控えめ）: 左右の角から 1 回ずつ・S のときの上からのもう 1 回
const CONFETTI_SIDE: usize = 44;
const CONFETTI_TOP: usize = 48;

/// 端の安全域（ノッチ・角の丸み・ホームバー。論理 px）。指の部品・Ⅱ・端の HUD・下の端のボタンをこの分だけ内へ寄せる
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Safe { pub top: f32, pub right: f32, pub bottom: f32, pub left: f32 }
impl Safe {
    /// [上, 右, 下, 左]（game_ui::page::safe_area と同じ並び）
    pub fn from_array(a: [f32; 4]) -> Safe { Safe { top: a[0].max(0.0), right: a[1].max(0.0), bottom: a[2].max(0.0), left: a[3].max(0.0) } }
    pub fn to_array(self) -> [f32; 4] { [self.top, self.right, self.bottom, self.left] }
    /// 安全域の内側
    pub fn inner(self, r: Rect) -> Rect { Rect::from_min_max(pos2(r.left() + self.left, r.top() + self.top), pos2(r.right() - self.right, r.bottom() - self.bottom)) }
    /// Mac 版の --safe 上,右,下,左 の値（読み方は共通部品。ブラウザ版の ?safe= と同じ: 4 つの 0 以上の数でなければ無視）
    pub fn parse(s: &str) -> Option<Safe> { game_ui::menu::parse_safe(s).map(Safe::from_array) }
}

/// タッチのボタンの名前（data-btn）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchBtn { Ranbu, Heavy, Attack, Jump, Dodge, Wire, Pause }
pub const TOUCH_BTNS: [TouchBtn; 7] = [TouchBtn::Ranbu, TouchBtn::Heavy, TouchBtn::Attack, TouchBtn::Jump, TouchBtn::Dodge, TouchBtn::Wire, TouchBtn::Pause];
/// タッチの見た目の状態（input が持つ値を渡す）
#[derive(Clone, Copy, Debug, Default)]
pub struct TouchView {
    /// スティックの中心（触れた所。倒しきった先へ指が進むと付いてくる。触っていなければ既定の位置 left 22% top 70%）
    pub stick: Option<(f32, f32)>,
    /// ノブのずれ（px）
    pub knob: (f32, f32),
    /// 押されているボタン（TOUCH_BTNS の順）
    pub on: [bool; 7],
}
/// 押せる箱の一辺の下限（px）。見た目がこれより小さいボタン（Ⅱ・低い画面の「避」）は箱だけ広げる
pub const HIT_MIN: f32 = 48.0;
/// 小地図の大きさ（hud.rs が描く。Ⅱ はそのそばに置く）
pub fn map_size(w: f32, h: f32) -> f32 { if h <= 520.0 { 96.0 } else if w <= 760.0 { 120.0 } else { 180.0 } }
/// 小地図の左上
pub fn map_origin(w: f32, h: f32, safe: Safe) -> Pos2 { pos2(w - safe.right - 18.0 - map_size(w, h), safe.top + 14.0) }
/// Ⅱ を小地図の下に置くか（幅が足りないと、小地図の左では上の目標の札と重なる）。低い画面と広い画面は小地図の左
pub fn pause_under_map(w: f32, h: f32) -> bool { h > 520.0 && w < 920.0 }
/// Ⅱ の押せる箱（48px）。右上の小地図のそば
pub fn pause_hit_rect(w: f32, h: f32, safe: Safe) -> Rect {
    let (o, size) = (map_origin(w, h, safe), map_size(w, h));
    if pause_under_map(w, h) { Rect::from_min_size(pos2(o.x + size - HIT_MIN, o.y + size + 8.0), vec2(HIT_MIN, HIT_MIN)) }
    else { Rect::from_min_size(pos2(o.x - 10.0 - HIT_MIN, o.y), vec2(HIT_MIN, HIT_MIN)) }
}
/// ボタンの見た目の場所。px は論理ピクセル
pub fn touch_button_rect(b: TouchBtn, w: f32, h: f32, safe: Safe) -> Rect {
    let short = h <= 520.0;
    if b == TouchBtn::Pause { return Rect::from_center_size(pause_hit_rect(w, h, safe).center(), vec2(40.0, 40.0)); }
    let k = if short { 0.82 } else { 1.0 };
    let (r, bt, s) = match b { TouchBtn::Attack => (0.0, 70.0, 84.0), TouchBtn::Heavy => (90.0, 120.0, 70.0), TouchBtn::Jump => (90.0, 30.0, 70.0), TouchBtn::Dodge => (10.0, 0.0, 56.0), TouchBtn::Wire => (92.0, 196.0, 58.0), _ => (20.0, 165.0, 64.0) };
    // .tbtns の右下を原点に縮める。原点は端の安全域の内側
    let (ox, oy) = (w - 18.0 - safe.right, h - 18.0 - safe.bottom);
    Rect::from_min_size(pos2(ox - (r + s) * k, oy - (bt + s) * k), vec2(s * k, s * k))
}
/// ボタンの押せる箱（input の当たり判定）: 見た目の箱を、一辺 48px 以上へ広げる。さらに外側 10px までは input が押したことにする
pub fn touch_hit_rect(b: TouchBtn, w: f32, h: f32, safe: Safe) -> Rect {
    let r = touch_button_rect(b, w, h, safe);
    r.expand2(vec2(((HIT_MIN - r.width()) * 0.5).max(0.0), ((HIT_MIN - r.height()) * 0.5).max(0.0)))
}
/// スティックの既定の位置（触る前に見せる場所。安全域の内側）
pub fn stick_rest(w: f32, h: f32, safe: Safe) -> Pos2 {
    pos2((w * 0.22).max(safe.left + 68.0), (h * 0.7).min(h - safe.bottom - 68.0))
}

impl TouchBtn {
    /// input が使う名前（data-btn）。攻撃・強・跳・避・乱舞は割り当ての表から
    pub fn name(self) -> &'static str { self.act().map_or(controls::PAUSE_ID, |a| controls::bind(a).id) }
    pub fn act(self) -> Option<Act> { match self { TouchBtn::Ranbu => Some(Act::Ranbu), TouchBtn::Heavy => Some(Act::Heavy), TouchBtn::Attack => Some(Act::Attack), TouchBtn::Jump => Some(Act::Jump), TouchBtn::Dodge => Some(Act::Dodge), TouchBtn::Wire => Some(Act::Wire), TouchBtn::Pause => None } }
}
/// input の当たり判定へ渡す一覧（Input::set_touch_buttons）
pub fn touch_buttons(w: f32, h: f32, safe: Safe) -> [(&'static str, Rect); 7] { TOUCH_BTNS.map(|b| (b.name(), touch_hit_rect(b, w, h, safe))) }

fn layer_id(order: Order, id: &str) -> LayerId { LayerId::new(order, Id::new(id)) }

/// 押せる全画面（.screen.show は pointer-events: auto で、下のキャンバスへクリックを通さない）
fn surface<R>(ctx: &Context, id: &str, order: Order, f: impl FnOnce(&mut Ui) -> R) -> R {
    let r = ctx.content_rect();
    Area::new(Id::new(id)).order(order).fixed_pos(r.min).constrain(false).movable(false).interactable(true).show(ctx, |ui| {
        ui.set_min_size(r.size());
        ui.set_clip_rect(r);
        f(ui)
    }).inner
}

/// 押せる場所を調べる。隠れかけの画面では押せない
struct Hit<'a> { ui: Option<&'a mut Ui> }
impl Hit<'_> {
    /// (上に乗っている, 押した)
    fn btn(&mut self, id: &str, r: Rect) -> (bool, bool) {
        let Some(ui) = self.ui.as_mut() else { return (false, false) };
        let resp = ui.interact(r, Id::new(("ui-btn", id)), Sense::CLICK);
        if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
        (resp.hovered(), resp.clicked())
    }
}

/// .primary の中身
fn primary(pen: &Pen, hit: &mut Hit, id: &str, center_x: f32, top: f32, key: &str, label: &str) -> (Rect, bool) {
    let size = primary_size(pen, key, label);
    let r = Rect::from_min_size(pos2(center_x - size.0 * 0.5, top), vec2(size.0, size.1));
    let (over, clicked) = hit.btn(id, r);
    let b = if over { 1.15 } else { 1.0 };
    pen.slant(r, 0.06, &[(0.0, hx(0xff3a5a).bright(b)), (1.0, hx(0xff7a3a).bright(b))]);
    let items = primary_row(pen, key, label);
    let w = row_w(&items);
    row_draw(pen, &items, r.center().x - w * 0.5, r.center().y + 15.0 * BASE_K);
    (r, clicked)
}
/// key は決定の表記（controls::confirm_label。空なら札を添えない）
fn primary_row(pen: &Pen, key: &str, label: &str) -> Vec<P> {
    let sp = 15.0 * 0.25;
    let mut items = vec![];
    if !key.is_empty() {
        items.push(P::K(Kbd::plain(pen, key, 15.0, sp)));
        items.push(P::T(pen.t(" ", Fam::Jp900, 15.0, sp), CREAM, vec![]));
    }
    items.push(P::T(pen.t(label, Fam::Jp900, 15.0, sp), CREAM, vec![]));
    items
}
fn primary_size(pen: &Pen, key: &str, label: &str) -> (f32, f32) { (row_w(&primary_row(pen, key, label)) + 68.0, 15.0 * LH + 26.0) }

/// 選んでいる項目の輪（キーボード・パッドで選ぶとき）
fn focus_ring(pen: &Pen, r: Rect) { pen.frame(r.expand(5.0), 2.0, GOLD); }

/// .res-actions button:not(.primary) / .menu button:not(.primary)
fn plain_btn(pen: &Pen, hit: &mut Hit, id: &str, center_x: f32, top: f32, label: &str) -> (Rect, bool) {
    let t = pen.t(label, Fam::Jp500, 14.0, 0.0);
    let r = Rect::from_min_size(pos2(center_x - (t.w + 46.0) * 0.5, top), vec2(t.w + 46.0, 14.0 * LH + 26.0));
    let (_, clicked) = hit.btn(id, r);
    pen.rect(r, rgba(18, 12, 34, 0.6));
    pen.frame(r, 1.0, LINE);
    pen.text(&t, r.left() + 23.0, r.center().y + 14.0 * BASE_K, CREAM, &[]);
    (r, clicked)
}
fn plain_w(pen: &Pen, label: &str) -> f32 { pen.t(label, Fam::Jp500, 14.0, 0.0).w + 46.0 }

/// .eyebrow（中央寄せ）。高さを返す
fn eyebrow(pen: &Pen, text: &str, cx: f32, top: f32) -> f32 {
    let t = pen.t(text, Fam::Jp700, 11.0, 5.5);
    pen.text(&t, cx - t.w * 0.5, top + 11.0 * LH * 0.5 + 11.0 * BASE_K, GOLD, &[]);
    11.0 * LH + 5.5
}

/// 結果の誉れを段に分けたときの、段の間（px）
const HONOR_ROW_GAP: f32 = 8.0;

/// @keyframes full（.5s / .6s infinite alternate）。0..1
fn full_k(now: f64, dur: f32) -> f32 {
    let t = (now as f32 / dur).rem_euclid(2.0);
    css_ease(if t < 1.0 { t } else { 2.0 - t })
}

pub struct Screens {
    now: f64,
    t0: f64,
    fade: [Tween; 6],
    shown: Option<Screen>,
    shown_at: f64,
    /// 最後に触った機器（表記と、指の部品の出し入れ）
    device: Device,
    /// 選んでいる項目の輪を出すか（最後に触ったのがキーボードかパッド）
    ring: bool,
    /// キーボード・パッドで選んでいる項目（menu() の番号）
    focus: usize,
    safe: Safe,
    touch_on: bool,
    touch_view: TouchView,
    /// 窓の大きさ（論理。確かめ用の写しに使う）
    size: (f32, f32),
    // 読み込み
    load_fill: Tween,
    load_text: String,
    load_failed: bool,
    // タイトル
    title_best: String,
    muted: bool,
    // 武将選び
    heroes: Vec<HeroCard>,
    hero_on: usize,
    hero_k: Vec<Tween>,
    strip_scroll: f32,
    strip_target: f32,
    strip_center: Option<usize>,
    strip_press: Option<Pos2>,
    sel_name: String,
    weapon: Option<WeaponCard>,
    // 一時停止
    moves: Vec<MoveRow>,
    // 結果・敗北
    result: ResultData,
    // 自己ベストの祝い（流れは sim::cheer。app が出来事を渡す）
    confetti: game_ui::cheer::Confetti,
    /// 数え上げ中の得点（None は流れが無い・打ち切った → 本当の値の字をそのまま出す）
    count: Option<i64>,
    /// 数え終わった時刻と、自己ベストだったか
    cheer_at: Option<f64>,
    cheer_best: bool,
    /// 動きを減らす設定（弾みなし・紙吹雪は控えめ）
    calm: bool,
    /// 結果の中身の箱と 2 つのボタン（紙吹雪を出す角・確かめ用の写し）
    result_card: std::cell::Cell<Option<Rect>>,
    result_btns: std::cell::Cell<[Rect; 2]>,
    /// タイトルの「キャラを作って出陣」の場所（確かめ用。描いていないときは NOTHING）
    pub(crate) title_make: std::cell::Cell<Rect>,
    // 切り込み・帯・閃き
    cutin_at: Option<f64>,
    ci_face: Option<Tex>,
    ci_name: String,
    letter: Tween,
    flash: f32,
}

impl Screens {
    pub fn new(ctx: &Context) -> Self {
        let t0 = ctx.input(|i| i.time);
        let mut fade = [Tween::new(0.0, 0.5, 0.0, ease); 6];
        fade[0].jump(1.0);
        Screens {
            now: t0, t0, fade, shown: Some(Screen::Loading), shown_at: t0, device: Device::Key, ring: false, focus: 0, safe: Safe::default(), touch_on: false, touch_view: TouchView::default(), size: (1.0, 1.0),
            load_fill: Tween::new(0.0, 0.4, 0.0, ease), load_text: "陣を敷いています…".into(), load_failed: false,
            title_best: String::new(), muted: false,
            heroes: vec![], hero_on: 0, hero_k: vec![], strip_scroll: 0.0, strip_target: 0.0, strip_center: None, strip_press: None, sel_name: String::new(), weapon: None,
            moves: controls::move_rows(), result: ResultData::default(),
            confetti: game_ui::cheer::Confetti::new(), count: None, cheer_at: None, cheer_best: false, calm: false, result_card: Default::default(), result_btns: std::cell::Cell::new([Rect::NOTHING; 2]), title_make: std::cell::Cell::new(Rect::NOTHING),
            cutin_at: None, ci_face: None, ci_name: String::new(), letter: Tween::new(0.0, 0.45, 0.0, ease), flash: 0.0,
        }
    }
    /// TS の show(id)。None で全部隠す
    pub fn show(&mut self, s: Option<Screen>) {
        let now = self.now;
        if s != self.shown { self.focus = 0; }
        self.shown = s;
        self.shown_at = now;
        for (i, x) in SCREENS.iter().enumerate() { self.fade[i].set(if Some(*x) == s { 1.0 } else { 0.0 }, now); }
    }
    pub fn progress(&mut self, k: f32, text: Option<&str>) {
        self.load_fill.set(k, self.now);
        if let Some(t) = text { self.load_text = t.to_string(); }
    }
    pub fn set_title_best(&mut self, s: String) { self.title_best = s; }
    /// 音の札（音: オン / オフ）
    pub fn set_muted(&mut self, m: bool) { self.muted = m; }
    /// 読み込みに失敗した: 知らせの文と「読み込み直す」を出す。false で読み込み中の見た目へ戻す
    pub fn load_failed(&mut self, text: Option<&str>) {
        self.load_failed = text.is_some();
        if let Some(t) = text { self.load_text = t.to_string(); self.load_fill.set(1.0, self.now); }
    }
    /// 最後に触った機器（今の版の body.touch の役）。表記を機器に合わせ、指のときだけ指の部品を出す。ring は選んでいる項目の輪
    pub fn set_device(&mut self, d: Device, ring: bool) { self.device = d; self.ring = ring; }
    pub fn device(&self) -> Device { self.device }
    /// 今出ている画面（無ければ戦いの間）
    pub fn shown(&self) -> Option<Screen> { self.shown }
    fn touch(&self) -> bool { self.device == Device::Touch }
    /// 端の安全域
    pub fn set_safe(&mut self, s: Safe) { self.safe = s; }
    /// 決定の表記（ボタンの札に添える）
    fn confirm(&self) -> &'static str { controls::confirm_label(self.device) }
    /// キーボード・パッドで選べる項目（上から・左から）
    pub fn menu(&self) -> Vec<UiAction> {
        match self.shown {
            Some(Screen::Pause) => vec![UiAction::Resume, UiAction::ToggleSound, UiAction::ToTitle],
            Some(Screen::Title) => vec![UiAction::Start, UiAction::MakeChar, UiAction::Online],
            Some(Screen::Result) => vec![UiAction::Again, UiAction::ToSelect],
            Some(Screen::Gameover) => vec![UiAction::Continue, UiAction::ToTitle],
            Some(Screen::Loading) if self.load_failed => vec![UiAction::Reload],
            _ => vec![],
        }
    }
    /// 結果と敗北は、出てから 0.6 秒は決定を受けない（攻撃の連打で飛ばさない）
    pub fn accepts(&self) -> bool { !matches!(self.shown, Some(Screen::Result | Screen::Gameover)) || game_ui::menu::result_accepts(self.now - self.shown_at) }
    /// キーボード・パッドで選ぶ: prev / next で移り（端は回る）、confirm で押す。押した項目を返す
    pub fn nav(&mut self, prev: bool, next: bool, confirm: bool) -> Option<UiAction> {
        let items = self.menu();
        let n = items.len();
        if n == 0 { return None; }
        if prev { self.focus = (self.focus + n - 1) % n; }
        if next { self.focus = (self.focus + 1) % n; }
        self.focus = self.focus.min(n - 1);
        (confirm && self.accepts()).then(|| items[self.focus])
    }
    pub fn focus(&self) -> usize { self.focus }
    /// 今の画面でこの項目に輪を出すか
    fn focused(&self, s: Screen, a: UiAction) -> bool { self.ring && self.shown == Some(s) && self.menu().get(self.focus) == Some(&a) }
    /// #touch.on（戦いの間だけ）
    pub fn touch_on(&mut self, on: bool) { self.touch_on = on; }
    pub fn set_touch_view(&mut self, v: TouchView) { self.touch_view = v; }
    pub fn set_heroes(&mut self, heroes: Vec<HeroCard>) {
        self.hero_k = heroes.iter().map(|_| Tween::new(0.0, 0.3, 0.0, snap)).collect();
        self.heroes = heroes;
    }
    /// pickHero の見た目（札の .on とスクロール）
    pub fn pick_hero(&mut self, i: usize) {
        let now = self.now;
        self.hero_on = i;
        for (j, k) in self.hero_k.iter_mut().enumerate() { k.set(if j == i { 1.0 } else { 0.0 }, now); }
        self.strip_center = Some(i);
    }
    /// #sel-name（読み込み中は「名前…」など）
    pub fn set_sel_name(&mut self, s: &str) { self.sel_name = s.to_string(); }
    pub fn set_weapon(&mut self, w: WeaponCard) { self.weapon = Some(w); }
    pub fn set_result(&mut self, r: ResultData) { self.result = r; self.cheer_clear(); }
    /// 自己ベストの祝いを始める: 得点は 0 から数え上がる（値は app が cheer_count で渡す）。calm は動きを減らす設定
    pub fn cheer_begin(&mut self, calm: bool) {
        self.cheer_clear();
        self.calm = calm;
        self.confetti.set_calm(calm);
        self.count = Some(0);
    }
    pub fn cheer_count(&mut self, v: i64) { if self.count.is_some() { self.count = Some(v); } }
    /// 数え終わった。自己ベストなら、結果の中身の左右の下の角から内向き・上向きに紙吹雪（上がる高さは中身の上の端まで）
    pub fn cheer_done(&mut self, best: bool) {
        self.cheer_at = Some(self.now);
        self.cheer_best = best;
        if !best { return; }
        let card = self.result_card.get().unwrap_or(Rect::from_min_size(pos2(0.0, 0.0), vec2(self.size.0, self.size.1)));
        // 向きは箱の形から: 上の端の真ん中あたりへ届く角度（横長の箱は寝かせ、縦長の箱は立てる。横長の低い画面でも真ん中へ届く）
        let up = card.height().atan2(card.width() * 0.5).to_degrees().clamp(38.0, 78.0);
        let speed = game_ui::cheer::speed_to_rise(card.height()) / up.to_radians().sin();
        self.confetti.burst(pos2(card.left() + 12.0, card.bottom() - 12.0), -up, 40.0, CONFETTI_SIDE, speed);
        self.confetti.burst(pos2(card.right() - 12.0, card.bottom() - 12.0), up - 180.0, 40.0, CONFETTI_SIDE, speed);
    }
    /// 自己ベストでいちばん上の格（S）: 上の中央からもう 1 回
    pub fn cheer_encore(&mut self) { self.confetti.burst(pos2(self.size.0 * 0.5, self.safe.top + 4.0), 90.0, 150.0, CONFETTI_TOP, 520.0); }
    /// 確かめ用: (数え上げ中の得点, 数え終わったか, 紙吹雪の枚数)
    pub fn cheer_state(&self) -> (Option<i64>, bool, usize) { (self.count, self.cheer_at.is_some(), self.confetti.len()) }
    /// 打ち切る（ボタンを押した・画面を離れた）: 紙吹雪を消し、得点は本当の値
    pub fn cheer_clear(&mut self) { self.confetti.clear(); self.count = None; self.cheer_at = None; self.cheer_best = false; }
    /// 切り込みの顔と名前（go で入れる）
    pub fn set_cutin_hero(&mut self, face: Option<Tex>, name: &str) { self.ci_face = face; self.ci_name = name.to_string(); }
    /// #cutin に on を付ける（アニメーションをやり直す）
    pub fn cutin_start(&mut self) { self.cutin_at = Some(self.now); }
    pub fn cutin_stop(&mut self) { self.cutin_at = None; }
    /// #letterbox の on
    pub fn letterbox(&mut self, on: bool) { self.letter.set(if on { 1.0 } else { 0.0 }, self.now); }
    /// #flash の opacity
    pub fn set_flash(&mut self, v: f32) { self.flash = v; }
    /// コマの始めに時計を進める（遊びの中で出す文字や札も、このコマの時刻から数える。今の版の CSS と同じ）
    pub fn advance(&mut self, d: f64) { self.now += d; }

    /// 毎コマ描く。押されたボタンを返す
    pub fn ui(&mut self, ctx: &Context) -> Vec<UiAction> {
        let mut acts = vec![];
        let now = ctx.input(|i| i.time);
        self.now = now;
        let screen = ctx.content_rect();
        self.size = (screen.width(), screen.height());
        self.confetti.step(ctx.input(|i| i.stable_dt).min(0.05), screen.height());
        // DOM の重なり順: 読み込み・タイトル・武将選び（Middle）< HUD（Foreground）< 切り込み・帯（Tooltip）< 結果・敗北・一時停止 < タッチ・閃き（Debug）
        for (s, id) in [(Screen::Loading, "loading"), (Screen::Title, "title"), (Screen::Select, "select")] {
            self.one(ctx, s, Order::Middle, id, now, screen, &mut acts);
        }
        let over = Pen::new(ctx, ctx.layer_painter(layer_id(Order::Tooltip, "cutin")));
        self.cutin(&over, now, screen);
        self.letterbox_ui(&over, now, screen);
        for (s, id) in [(Screen::Result, "result"), (Screen::Gameover, "gameover"), (Screen::Pause, "pause")] {
            self.one(ctx, s, Order::Debug, id, now, screen, &mut acts);
        }
        // 押さない面は同じ層へ順に描く（Area のない層どうしの順は決まらない）
        let top = Pen::new(ctx, ctx.layer_painter(layer_id(Order::Debug, "top")));
        self.touch_ui(&top, screen);
        if self.flash > 0.0 { top.rect(screen, WHITE.alpha(self.flash)); }
        acts
    }

    fn one(&mut self, ctx: &Context, s: Screen, order: Order, id: &str, now: f64, screen: Rect, acts: &mut Vec<UiAction>) {
        let op = self.fade[scr_i(s)].get(now);
        if op <= 0.0 && self.shown != Some(s) { return; }
        if self.shown == Some(s) {
            // 結果と敗北は、出てから 0.6 秒は押せない（面はクリックを下へ通さないまま）
            let live = self.accepts();
            surface(ctx, id, order, |ui| {
                let pen = Pen::new(ctx, ui.painter().clone()).with_alpha(op);
                let mut hit = Hit { ui: if live { Some(ui) } else { None } };
                self.draw(s, &pen, &mut hit, now, screen, acts);
            });
        } else {
            let pen = Pen::new(ctx, ctx.layer_painter(layer_id(order, id))).with_alpha(op);
            let mut hit = Hit { ui: None };
            self.draw(s, &pen, &mut hit, now, screen, acts);
        }
    }

    fn draw(&mut self, s: Screen, pen: &Pen, hit: &mut Hit, now: f64, r: Rect, acts: &mut Vec<UiAction>) {
        match s {
            Screen::Loading => self.loading(pen, hit, now, r, acts),
            Screen::Title => self.title(pen, hit, now, r, acts),
            Screen::Select => self.select(pen, hit, now, r, acts),
            Screen::Result => self.result_ui(pen, hit, now, r, acts),
            Screen::Gameover => self.gameover(pen, hit, r, acts),
            Screen::Pause => self.pause(pen, hit, r, acts),
        }
    }

    // ---- 読み込み ----
    fn loading(&self, pen: &Pen, hit: &mut Hit, now: f64, r: Rect, acts: &mut Vec<UiAction>) {
        let (w, h) = (r.width(), r.height());
        let area = self.safe.inner(r);
        pen.radial(r, pos2(w * 0.5, h * 1.2), w * 1.1, h * 0.8, &[(0.0, hx(0x4a1a3a)), (0.5, hx(0x1e1430)), (1.0, hx(0x0a0614))], 48, 28);
        // 縦に並べて中央へ（間 22px）。文は窓に収まらないと折り返す
        let mark_h = 96.0 * LH;
        let rows = pen.lines(&self.load_text, Fam::Jp500, 13.0, 3.9, area.width() - 48.0);
        let text_h = rows.len() as f32 * 13.0 * LH;
        let pw = primary_size(pen, self.confirm(), "読み込み直す");
        let tail_h = if self.load_failed { pw.1 } else { 2.0 };
        let total = mark_h + 22.0 + text_h + 22.0 + tail_h;
        let cx = area.center().x;
        let mut y = area.top() + (area.height() - total) * 0.5;
        // breathe: 1.6s ease-in-out infinite
        let t = ((now - self.t0) as f32 / 1.6).rem_euclid(1.0);
        let b = if t < 0.5 { ease_io(t * 2.0) } else { 1.0 - ease_io((t - 0.5) * 2.0) };
        let (op, sc) = (lerp(0.5, 1.0, b), lerp(0.96, 1.04, b));
        let cy = y + mark_h * 0.5;
        let m = pen.sub(Xf::about(cx, cy, Xf::scale(sc, sc)), op);
        let tm = m.t("魚", Fam::Brush, 96.0, 0.0);
        m.text(&tm, cx - tm.w * 0.5, cy + 96.0 * BASE_K, GOLD, &[TS(0.0, 0.0, 30.0, rgba(255, 180, 60, 0.6))]);
        y += mark_h + 22.0;
        for (i, tt) in rows.iter().enumerate() {
            pen.text(tt, cx - tt.w * 0.5, y + 13.0 * LH * (i as f32 + 0.5) + 13.0 * BASE_K, if self.load_failed { CREAM } else { CREAM_DIM }, &[]);
        }
        y += text_h + 22.0;
        if self.load_failed {
            // 読めなかった: 文だけで止めず、読み込み直すボタンを出す
            let (br, clicked) = primary(pen, hit, "reload", cx, y, self.confirm(), "読み込み直す");
            if clicked { acts.push(UiAction::Reload); }
            if self.focused(Screen::Loading, UiAction::Reload) { focus_ring(pen, br); }
            return;
        }
        let bar = Rect::from_min_size(pos2(cx - 100.0, y), vec2(200.0, 2.0));
        pen.rect(bar, rgba(255, 255, 255, 0.12));
        let k = clamp01(self.load_fill.get(now));
        if k > 0.0 { pen.hgrad(Rect::from_min_size(bar.min, vec2(200.0 * k, 2.0)), &[(0.0, RED), (1.0, GOLD)]); }
    }

    // ---- タイトル ----
    fn title(&self, pen: &Pen, hit: &mut Hit, now: f64, r: Rect, acts: &mut Vec<UiAction>) {
        let (w, h) = (r.width(), r.height());
        let short = h <= 520.0;
        pen.vgrad(r, &[(0.0, rgba(10, 6, 20, 0.5)), (0.3, CLEAR), (0.6, CLEAR), (1.0, rgba(10, 6, 20, 0.85))]);
        pen.radial(r, pos2(w * 0.5, h * 0.45), w * 0.8, h * 0.6, &[(0.0, rgba(10, 6, 20, 0.0)), (1.0, rgba(10, 6, 20, 0.55))], 48, 28);
        // アニメーションはページを開いた時から 1 回（戻ってきても繰り返さない）
        let t = (now - self.t0) as f32;
        let fade_up = |delay: f32| { let p = ease(clamp01((t - delay) / 1.0)); (p, 14.0 * (1.0 - p)) };
        let logo = if short { 72.0 } else { cl(84.0, w * 0.15, 210.0) };
        let (en_fs, en_m, tg_fs, tg_m) = if short { (11.0, 6.0, 13.0, 10.0) } else { (15.0, 14.0, cl(15.0, w * 0.018, 19.0), 22.0) };
        // 幅が足りないと折り返す。そのとき箱は窓いっぱいになり、文字は左へ寄る
        let pre_rows = pen.lines("ONE AGAINST A THOUSAND FISH", Fam::Jp500, 11.0, 7.7, w);
        let tg_rows = pen.lines("一騎当千。魚の大群を、ひとりでなぎ倒せ。", Fam::Mincho600, tg_fs, tg_fs * 0.2, w);
        let box_x = |rows: &[T]| if rows.len() > 1 { 0.0 } else { w * 0.5 - rows[0].w * 0.5 };
        let wrap_h = pre_rows.len() as f32 * 11.0 * LH + 10.0 + logo + en_m + en_fs * LH + tg_m + tg_rows.len() as f32 * tg_fs * LH;
        let mt = if short { -0.14 } else { -0.06 } * h;
        let mut y = (h - (wrap_h + mt)) * 0.5 + mt;
        // ONE AGAINST ...
        {
            let (p, dy) = fade_up(0.2);
            let q = pen.sub(Xf::tr(0.0, dy), p);
            q.text_lines(&pre_rows, box_x(&pre_rows), y, 11.0 * LH, GOLD, &[]);
            y += pre_rows.len() as f32 * 11.0 * LH + 10.0;
        }
        // 四文字
        let chars = [("魚", CREAM, 0.35), ("群", GOLD, 0.5), ("繚", CREAM, 0.65), ("乱", hx(0xff6a7a), 0.8)];
        let gap = logo * 0.02;
        let ts: Vec<T> = chars.iter().map(|(c, _, _)| pen.t(c, Fam::Brush, logo, 0.0)).collect();
        let total: f32 = ts.iter().map(|x| x.w).sum::<f32>() + gap * 3.0;
        let mut x = w * 0.5 - total * 0.5;
        let cy = y + logo * 0.5;
        for (i, (_, col, delay)) in chars.iter().enumerate() {
            let k = clamp01((t - delay) / 1.1);
            // inkIn: 拡大と回転は全体に snap、透明度とぼかしは 60% まで
            let op = kf(k, &[(0.0, 0.0), (0.6, 1.0)], snap).clamp(0.0, 1.0);
            let blur = kf(k, &[(0.0, 18.0), (0.6, 0.0)], snap).max(0.0);
            let e = snap(k);
            let (sc, rot) = (lerp(1.8, 1.0, e), lerp(-8.0, 0.0, e));
            let ccx = x + ts[i].w * 0.5;
            let q = pen.sub(Xf::about(ccx, cy, Xf::rot(rot).then(Xf::scale(sc, sc))), op);
            q.text_f(&ts[i], x, cy + logo * BASE_K, Fill::S(*col), &[TS(0.0, 0.0, 40.0, rgba(255, 90, 60, 0.45)), TS(0.0, 6.0, 0.0, rgba(90, 20, 40, 0.6))], blur);
            x += ts[i].w + gap;
        }
        y += logo;
        // GYOGUN  RYORAN（padding-left で字間の分を中央へ）
        {
            y += en_m;
            let (p, dy) = fade_up(1.1);
            let q = pen.sub(Xf::tr(0.0, dy), p);
            let sp = en_fs * 0.9;
            let tx = q.t("GYOGUN\u{a0}\u{a0}RYORAN", Fam::Mincho800, en_fs, sp);
            q.text(&tx, w * 0.5 - (tx.w + sp) * 0.5 + sp, y + en_fs * LH * 0.5 + en_fs * BASE_K, CREAM_DIM, &[]);
            y += en_fs * LH;
        }
        {
            y += tg_m;
            let (p, dy) = fade_up(1.3);
            let q = pen.sub(Xf::tr(0.0, dy), p);
            q.text_lines(&tg_rows, box_x(&tg_rows), y, tg_fs * LH, CREAM, &[TS(0.0, 2.0, 14.0, rgba(0, 0, 0, 0.9))]);
        }
        // 出陣のボタン
        {
            let (fs, pv, ph, bottom) = if short { (14.0, 10.0, 28.0, h * 0.07 + self.safe.bottom) } else { (16.0, 15.0, 38.0, h * 0.14 + self.safe.bottom) };
            let sp = fs * 0.3;
            let mut items = vec![];
            match self.device {
                Device::Key => { items.push(P::K(Kbd::plain(pen, self.confirm(), fs, sp))); items.push(P::T(pen.t("かクリック", Fam::Jp900, fs, sp), CREAM, vec![])); }
                Device::Pad => items.push(P::K(Kbd::plain(pen, self.confirm(), fs, sp))),
                Device::Touch => items.push(P::T(pen.t("タップ", Fam::Jp900, fs, sp), CREAM, vec![])),
            }
            items.push(P::T(pen.t("で出陣", Fam::Jp900, fs, sp), CREAM, vec![]));
            let bw = row_w(&items) + ph * 2.0;
            let bh = fs * LH + pv * 2.0;
            let rect = Rect::from_min_size(pos2(w * 0.5 - bw * 0.5, h - bottom - bh), vec2(bw, bh));
            let (_, clicked) = hit.btn("start", rect);
            if clicked { acts.push(UiAction::Start); }
            let (p, dy) = fade_up(1.6);
            let q = pen.sub(Xf::tr(0.0, dy), p);
            // キャラを作って出陣（副ボタン）: 横に余裕があれば出陣の右、無ければ出陣の上。出陣は真ん中のまま
            {
                let mfs = if short { 12.0 } else { 13.0 };
                let mt = q.t(MAKE_LABEL, Fam::Jp700, mfs, mfs * 0.1);
                let (mw, mh) = (mt.w + 36.0, mfs * LH + 18.0);
                let beside = rect.right() + 16.0 + mw <= w - 18.0 - self.safe.right;
                let mr = if beside { Rect::from_min_size(pos2(rect.right() + 16.0, rect.center().y - mh * 0.5), vec2(mw, mh)) }
                    else { Rect::from_min_size(pos2(w * 0.5 - mw * 0.5, rect.top() - 12.0 - mh), vec2(mw, mh)) };
                let (over, clicked) = hit.btn("make", mr);
                if clicked { acts.push(UiAction::MakeChar); }
                q.rect(mr, if over { rgba(40, 26, 60, 0.75) } else { rgba(18, 12, 34, 0.6) });
                q.frame(mr, 1.0, LINE);
                q.text(&mt, mr.left() + 18.0, mr.center().y + mfs * BASE_K, CREAM, &[]);
                if self.focused(Screen::Title, UiAction::MakeChar) { focus_ring(&q, mr); }
                self.title_make.set(mr);
            }
            // glow: 2s ease-in-out 2.6s infinite
            let gt = ((t - 2.6) / 2.0).rem_euclid(1.0);
            let gk = if t < 2.6 { 0.0 } else if gt < 0.5 { ease_io(gt * 2.0) } else { 1.0 - ease_io((gt - 0.5) * 2.0) };
            let br = lerp(1.0, 1.3, gk);
            q.slant(rect, 0.04, &[(0.0, CLEAR), (0.18, rgba(255, 58, 90, 0.85).bright(br)), (0.82, rgba(255, 120, 60, 0.85).bright(br)), (1.0, CLEAR)]);
            row_draw(&q, &items, rect.left() + ph, rect.center().y + fs * BASE_K);
        }
        // 最高記録
        if !self.title_best.is_empty() {
            let bottom = if short { h * 0.02 } else { h * 0.14 - 36.0 } + self.safe.bottom;
            let tx = pen.t(&self.title_best, Fam::Jp500, 12.0, 2.4);
            let lh = 12.0 * LH;
            pen.text(&tx, w * 0.5 - tx.w * 0.5, h - bottom - lh * 0.5 + 12.0 * BASE_K, CREAM_DIM, &[]);
        }
        // みんなで（左上。右上の音の札と対）: 協力プレイのある元のゲームのページを開く。外へ出る印 ↗ を付ける（この印のある字体は数字用だけ）
        {
            let tx = pen.t(ONLINE_LABEL, Fam::Jp500, 12.0, 0.0);
            let ar = pen.t("↗", Fam::Num, 11.0, 0.0);
            let bw = tx.w + 6.0 + ar.w + 28.0;
            let bh = 12.0 * LH + 16.0;
            let rect = Rect::from_min_size(pos2(18.0 + self.safe.left, 18.0 + self.safe.top), vec2(bw, bh));
            let (_, clicked) = hit.btn("online", rect);
            if clicked { acts.push(UiAction::Online); }
            pen.rect(rect, rgba(18, 12, 34, 0.6));
            pen.frame(rect, 1.0, GOLD);
            pen.text(&tx, rect.left() + 14.0, rect.center().y + 12.0 * BASE_K, CREAM, &[]);
            pen.text(&ar, rect.left() + 14.0 + tx.w + 6.0, rect.center().y + 11.0 * BASE_K, GOLD, &[]);
            if self.focused(Screen::Title, UiAction::Online) { focus_ring(pen, rect); }
        }
        // 音の札
        {
            let label = if self.muted { "音: オフ" } else { "音: オン" };
            let tx = pen.t(label, Fam::Jp500, 12.0, 0.0);
            let bw = tx.w + 28.0;
            let bh = 12.0 * LH + 16.0;
            let rect = Rect::from_min_size(pos2(w - 18.0 - self.safe.right - bw, 18.0 + self.safe.top), vec2(bw, bh));
            let (_, clicked) = hit.btn("sound-title", rect);
            if clicked { acts.push(UiAction::ToggleSound); }
            pen.rect(rect, rgba(18, 12, 34, 0.6));
            pen.frame(rect, 1.0, LINE);
            pen.text(&tx, rect.left() + 14.0, rect.center().y + 12.0 * BASE_K, CREAM, &[]);
        }
        // 出典
        if !short {
            // 左右 18px の箱の中で中央寄せ。下から 12px
            let rows = pen.lines("きゃしー・武器・小物・音楽 AI Arranger（CC BY 4.0）／ 魚 Fish Kit（CC0）／ 効果音 Kenney（CC0）・Stable Audio 3", Fam::Jp500, 10.0, 0.0, w - 36.0);
            let (lh, n) = (10.0 * LH, rows.len() as f32);
            for (i, tx) in rows.iter().enumerate() {
                pen.text(tx, w * 0.5 - tx.w * 0.5, h - 12.0 - self.safe.bottom - lh * (n - i as f32 - 0.5) + 10.0 * BASE_K, rgba(255, 243, 224, 0.45), &[]);
            }
        }
    }

    // ---- 武将選び ----
    fn select(&mut self, pen: &Pen, hit: &mut Hit, now: f64, r: Rect, acts: &mut Vec<UiAction>) {
        let (w, h) = (r.width(), r.height());
        let (short, narrow) = (h <= 520.0, w <= 760.0);
        pen.hgrad(r, &[(0.4, rgba(10, 6, 20, 0.0)), (1.0, rgba(10, 6, 20, 0.7))]);
        pen.vgrad(r, &[(0.0, rgba(10, 6, 20, 0.5)), (0.25, CLEAR), (0.65, CLEAR), (1.0, rgba(10, 6, 20, 0.85))]);
        let safe = self.safe;
        let (pad_t, pad_b) = if short { (h * 0.02 + safe.top, h * 0.02 + safe.bottom) } else { (h * 0.05 + safe.top, h * 0.04 + safe.bottom) };
        let dev = self.device;
        let kbd = |a: Act| P::K(Kbd::plain(pen, controls::label(a, dev), 13.0, 0.0));
        // 見出し
        let mut y = pad_t;
        if !short { y += eyebrow(pen, "CHOOSE YOUR WARRIOR", w * 0.5, y); }
        let h2 = if short { 26.0 } else { cl(36.0, w * 0.05, 56.0) };
        let th = pen.t("誰が出る？", Fam::Brush, h2, 0.0);
        pen.text(&th, w * 0.5 - th.w * 0.5, y + h2 * LH * 0.5 + h2 * BASE_K, CREAM, &[TS(0.0, 2.0, 20.0, rgba(0, 0, 0, 0.8))]);
        // 下の帯（札の列と出陣）
        let (cell, strip_gap) = if short { (50.0, 6.0) } else { (72.0, 14.0) };
        let foot_h = 15.0 * LH + 26.0;
        let strip_h = cell + 20.0;
        let foot_top = h - pad_b - foot_h;
        let strip_top = foot_top - strip_gap - strip_h;
        self.select_strip(pen, hit, now, Rect::from_min_max(pos2(safe.left, strip_top), pos2(w - safe.right, strip_top + strip_h)), cell, acts);
        // 出陣の行
        {
            let primary_w = primary_size(pen, self.confirm(), "出陣").0;
            let mut items = vec![];
            if !self.touch() {
                let sp = 0.0;
                items.push(P::K(Kbd::plain(pen, "←", 13.0, sp)));
                items.push(P::K(Kbd::plain(pen, "→", 13.0, sp)));
                items.push(P::T(pen.t(" 武将　", Fam::Jp500, 13.0, sp), CREAM_DIM, vec![]));
                items.push(P::K(Kbd::plain(pen, "↑", 13.0, sp)));
                items.push(P::K(Kbd::plain(pen, "↓", 13.0, sp)));
                items.push(P::T(pen.t(" 武器", Fam::Jp500, 13.0, sp), CREAM_DIM, vec![]));
            }
            let lw = row_w(&items);
            let gap = if self.touch() { 0.0 } else { 22.0 };
            let x0 = w * 0.5 - (lw + gap + primary_w) * 0.5;
            row_draw(pen, &items, x0, foot_top + foot_h * 0.5 + 13.0 * BASE_K);
            let (_, clicked) = primary(pen, hit, "go", x0 + lw + gap + primary_w * 0.5, foot_top, self.confirm(), "出陣");
            if clicked { acts.push(UiAction::Go); }
        }
        // 右の欄（名前・武器・技）
        let side_w = if narrow && !short { w * 0.9 } else if short { (w - safe.left - safe.right) * 0.42 } else { 380.0_f32.min(w * 0.4) };
        let side_gap = if short { 8.0 } else { 18.0 };
        let name_fs = if short { 30.0 } else { cl(42.0, w * 0.05, 68.0) };
        let tn = pen.tw(&self.sel_name, Fam::Brush, name_fs, 0.0, side_w, name_fs * 1.1);
        let name_h = name_fs * 1.1 * tn.rows().max(1) as f32;
        let (arrow_w, arrow_h, img_s, pad_v, pad_h) = if short { (30.0, 48.0, 44.0, 6.0, 10.0) } else { (38.0, 64.0, 64.0, 10.0, 14.0) };
        let card_w = side_w - (arrow_w + 8.0) * 2.0;
        let text_w = (card_w - pad_h * 2.0 - 2.0 - img_s - 12.0).max(40.0);
        // 武器の文字の高さを先に測る
        let (path_name, path_desc, path_below) = match self.weapon.as_ref().and_then(|x| x.path.as_ref()) {
            Some(p) => {
                // 図と名前（直進・拡散・曲進）は縮めない。説明だけが残りの幅で折り返す（名前まで縮めると、狭い縦画面で 1 字ずつ縦に折れた）
                let n = pen.t(&p.name, Fam::Jp900, 12.0, 0.0);
                let rest = (text_w - 38.0 - 12.0 - n.w).max(48.0);
                let d = pen.tw(&p.desc, Fam::Jp500, 12.0, 0.0, rest + 0.01, 12.0 * LH);
                // 横に置くと 3 行以上に折れるときは、説明を図と名前の下の段へ（文字の欄いっぱいの幅で折り返す）
                if d.rows() > 2 { (Some(n), Some(pen.tw(&p.desc, Fam::Jp500, 12.0, 0.0, text_w + 0.01, 12.0 * LH)), true) } else { (Some(n), Some(d), false) }
            }
            None => (None, None, false),
        };
        let path_h = path_desc.as_ref().map_or(0.0, |d| 3.0 + if path_below { 22.0 + d.rows() as f32 * 12.0 * LH } else { 22.0_f32.max(d.rows() as f32 * 12.0 * LH) });
        // 武器の名前（<b>）: 文字の欄に収まらないときは、字を少し小さくして 1 行に収める（13px まで。それでも長ければ折り返す。狭い縦画面で「目玉焼きのフライ／パン」と折れていた）
        let wp_fs = self.weapon.as_ref().map_or(18.0, |wp| {
            let fs0 = if short { 14.0 } else { 18.0 };
            let nat = pen.t(&wp.name, Fam::Jp900, fs0, 0.0).w;
            (fs0 * (text_w / nat.max(1.0)).min(1.0)).max(13.0)
        });
        let wp_name = self.weapon.as_ref().map(|wp| pen.tw(&wp.name, Fam::Jp900, wp_fs, 0.0, text_w + 0.5, 18.0 * LH));
        let col_h = 10.0 * LH + 18.0 * LH * wp_name.as_ref().map_or(1, |t| t.rows().max(1)) as f32 + path_h;
        let card_h = col_h.max(img_s) + pad_v * 2.0 + 2.0;
        let row_h = card_h.max(arrow_h);
        let moves_h = if short { 0.0 } else { 13.0 * LH * 2.0 + 6.0 };
        let side_h = name_h + side_gap + row_h + if short { 0.0 } else { side_gap + moves_h };
        let side_c = if short { h * 0.46 } else { h * 0.5 };
        let (side_x, mut y) = if narrow && !short {
            // 縦持ち: 欄は武将の帯のすぐ上へ寄せる（主役は見出しと欄の間に見せる。app.rs の武将選びのカメラ）
            let head_bottom = y + h2 * LH;
            ((w - side_w) * 0.5, (strip_top - 10.0 - side_h).max(head_bottom))
        } else { (w - safe.right - if short { w * 0.03 } else { w * 0.06 } - side_w, (side_c - side_h * 0.5).min(strip_top - 6.0 - side_h)) };
        // 名前
        pen.text(&tn, side_x, y + name_fs * 0.55 + name_fs * BASE_K, CREAM, &[TS(0.0, 0.0, 30.0, rgba(255, 120, 60, 0.5))]);
        y += name_h + side_gap;
        // 武器の段
        let cy = y + row_h * 0.5;
        for (id, x, label, d) in [("wp-prev", side_x, "‹", -1), ("wp-next", side_x + side_w - arrow_w, "›", 1)] {
            let rect = Rect::from_min_size(pos2(x, cy - arrow_h * 0.5), vec2(arrow_w, arrow_h));
            let (over, clicked) = hit.btn(id, rect);
            if clicked { acts.push(UiAction::StepWeapon(d)); }
            pen.rect(rect, if over { rgba(255, 201, 74, 0.2) } else { rgba(18, 12, 34, 0.5) });
            pen.frame(rect, 1.0, LINE);
            let t = pen.t(label, Fam::Jp500, 26.0, 0.0);
            pen.text(&t, rect.center().x - t.w * 0.5, cy + 26.0 * BASE_K, CREAM, &[]);
        }
        let card = Rect::from_min_size(pos2(side_x + arrow_w + 8.0, cy - card_h * 0.5), vec2(card_w, card_h));
        pen.rect(card, rgba(18, 12, 34, 0.65));
        pen.frame(card, 1.0, LINE);
        if let Some(wp) = &self.weapon {
            let ib = Rect::from_min_size(pos2(card.left() + 1.0 + pad_h, cy - img_s * 0.5), vec2(img_s, img_s));
            if let Some(t) = &wp.image {
                let ir = contain_rect(t, ib);
                // drop-shadow(0 4px 10px rgba(0,0,0,.6)): 黒く塗った写しを少しずつずらす
                for i in 0..8 {
                    let a = i as f32 / 8.0 * std::f32::consts::TAU;
                    pen.image(t, ir.translate(vec2(a.cos() * 5.0, 4.0 + a.sin() * 5.0)), rgba(0, 0, 0, 0.09));
                }
                pen.image(t, ir, WHITE);
            }
            let tx = ib.right() + 12.0;
            let mut ty = cy - col_h * 0.5;
            let ts = pen.t("武器", Fam::Jp500, 10.0, 4.0);
            pen.text(&ts, tx, ty + 10.0 * LH * 0.5 + 10.0 * BASE_K, GOLD, &[]);
            ty += 10.0 * LH;
            if let Some(tb) = &wp_name {
                pen.text(tb, tx, ty + 18.0 * LH * 0.5 + wp_fs * BASE_K, CREAM, &[]);
                ty += 18.0 * LH * tb.rows().max(1) as f32;
            }
            if let (Some(p), Some(n), Some(d)) = (&wp.path, &path_name, &path_desc) {
                ty += 3.0;
                let ph = if path_below { 22.0 } else { path_h - 3.0 };
                let pc = ty + ph * 0.5;
                draw_path_svg(pen, p.kind, pos2(tx, pc - 11.0));
                let nx = tx + 38.0 + 6.0;
                let nh = n.rows() as f32 * 12.0 * LH;
                pen.text(n, nx, pc - nh * 0.5 + 12.0 * LH * 0.5 + 12.0 * BASE_K, GOLD, &[]);
                let dh = d.rows() as f32 * 12.0 * LH;
                let (dx, dy) = if path_below { (tx, ty + 22.0) } else { (nx + n.w + 6.0, pc - dh * 0.5) };
                pen.text(d, dx, dy + 12.0 * LH * 0.5 + 12.0 * BASE_K, rgba(255, 243, 224, 0.8), &[]);
            }
        }
        y += row_h + side_gap;
        // 技の案内
        if !short {
            let dim = CREAM_DIM;
            let base = y + 13.0 * LH * 0.5 + 13.0 * BASE_K;
            // 技の案内の入力は、最後に触った機器の表記（指は画面のボタンの字）
            let mut items = vec![kbd(Act::Attack)];
            items.push(P::T(pen.t("×n → ", Fam::Jp500, 13.0, 0.0), dim, vec![]));
            items.push(kbd(Act::Heavy));
            items.push(P::Gap(8.0));
            items.push(P::T(pen.t("つなぐ数で強攻撃が変わる", Fam::Jp500, 13.0, 0.0), dim, vec![]));
            row_draw(pen, &items, side_x, base);
            y += 13.0 * LH + 6.0;
            let base = y + 13.0 * LH * 0.5 + 13.0 * BASE_K;
            let mut items = vec![kbd(Act::Ranbu)];
            items.push(P::Gap(8.0));
            items.push(P::T(pen.t("ゲージ満タンで乱舞", Fam::Jp500, 13.0, 0.0), dim, vec![]));
            row_draw(pen, &items, side_x, base);
        }
    }

    /// 札の列（横にはみ出すとスクロール）
    fn select_strip(&mut self, pen: &Pen, hit: &mut Hit, now: f64, band: Rect, cell: f32, acts: &mut Vec<UiAction>) {
        let n = self.heroes.len();
        if n == 0 { return; }
        let w = band.width();
        let content = n as f32 * cell + (n as f32 - 1.0) * 10.0 + 20.0;
        let view_w = content.min(w * 0.94);
        let view = Rect::from_min_size(pos2(band.left() + (w - view_w) * 0.5, band.top()), vec2(view_w, band.height()));
        let max_scroll = (content - view_w).max(0.0);
        if let Some(i) = self.strip_center.take() {
            // scrollIntoView({ inline: 'center', behavior: 'smooth' })
            self.strip_target = (10.0 + i as f32 * (cell + 10.0) + cell * 0.5 - view_w * 0.5).clamp(0.0, max_scroll);
        }
        let dt = pen.ctx.input(|i| i.stable_dt).min(0.05);
        self.strip_scroll += (self.strip_target - self.strip_scroll) * clamp01(dt * 12.0);
        let center = |i: usize, scroll: f32| pos2(view.left() + 10.0 + i as f32 * (cell + 10.0) + cell * 0.5 - scroll, view.center().y);
        // 押す・はみ出した分を動かす
        if let Some(ui) = hit.ui.as_mut() {
            let resp = ui.interact(view, Id::new("sel-strip"), Sense::CLICK | Sense::DRAG);
            if resp.hovered() {
                let dx = pen.ctx.input(|i| i.smooth_scroll_delta.x + i.smooth_scroll_delta.y);
                self.strip_target = (self.strip_target - dx).clamp(0.0, max_scroll);
            }
            if resp.dragged() { self.strip_target = (self.strip_target - resp.drag_delta().x).clamp(0.0, max_scroll); self.strip_scroll = self.strip_target; }
            let at = |p: Pos2, scroll: f32| (0..n).find(|i| {
                let c = center(*i, scroll);
                // 札は 45 度に回した正方形
                (p.x - c.x).abs() + (p.y - c.y).abs() <= cell * std::f32::consts::FRAC_1_SQRT_2 * 1.0
            });
            if resp.hover_pos().and_then(|p| at(p, self.strip_scroll)).is_some() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
            // 指では、離したコマに PointerGone も来て hover_pos が無い。押した所（interact_pointer_pos）で決める。
            // 少し動いた指は egui ではドラッグになるので、12pt 未満の動きなら札を押したことにする（今の版の button の click と同じ扱い）
            if resp.drag_started() { self.strip_press = resp.interact_pointer_pos(); }
            let small_drag = resp.drag_stopped() && match (self.strip_press, resp.interact_pointer_pos()) { (Some(a), Some(b)) => a.distance(b) < 12.0, _ => false };
            if resp.clicked() || small_drag {
                if let Some(i) = resp.interact_pointer_pos().or(self.strip_press).and_then(|p| at(p, self.strip_scroll)) { acts.push(UiAction::PickHero(i)); }
            }
            if resp.drag_stopped() || resp.clicked() { self.strip_press = None; }
        }
        // overflow-x: auto は縦にも切る（回した札の先が少し切れる）
        let clip = pen.clip(view);
        let half = cell * 0.5;
        for i in 0..n {
            let c = center(i, self.strip_scroll);
            if c.x < view.left() - cell || c.x > view.right() + cell { continue; }
            let k = self.hero_k[i].get(now);
            let on = self.hero_on == i;
            let sc = lerp(0.86, 1.0, k);
            // 枠と地は 45 度回した箱
            let scaled = clip.sub(Xf::about(c.x, c.y, Xf::scale(sc, sc)), 1.0);
            let bx = scaled.sub(Xf::about(c.x, c.y, Xf::rot(45.0)), 1.0);
            let box_r = Rect::from_center_size(c, vec2(cell, cell));
            if on { bx.soft(box_r, 0.0, 22.0, rgba(255, 201, 74, 0.5), false); }
            bx.rect(box_r, rgba(18, 12, 34, 0.7));
            // 絵は回らないまま、菱形に切る（内のりの菱形）
            let d = (half - 2.0) * std::f32::consts::SQRT_2;
            if let Some(t) = &self.heroes[i].select {
                let diamond = [pos2(c.x, c.y - d), pos2(c.x + d, c.y), pos2(c.x, c.y + d), pos2(c.x - d, c.y)];
                let img = Rect::from_center_size(c, vec2((cell - 4.0) * 1.4, (cell - 4.0) * 1.4));
                let uv = cover_uv(t, img.width(), img.height(), 0.5, 0.5);
                scaled.image_poly(t.handle.id(), &diamond, img, uv, WHITE);
            } else {
                let t = bx.t(&self.heroes[i].name, Fam::Jp500, 12.0, 0.0);
                bx.text(&t, c.x - t.w * 0.5, c.y + 12.0 * BASE_K, CREAM, &[]);
            }
            bx.frame(box_r, 2.0, if on { GOLD } else { rgba(255, 243, 224, 0.2) });
        }
        // 続きがある側の端を暗くぼかして、小さな矢じりを出す（切れているだけでは、まだ武将がいると分からない）
        if max_scroll > 0.5 {
            let dark = rgba(10, 6, 20, 0.75);
            let fw = 30.0_f32.min(view_w * 0.2);
            let cue = |left: bool| {
                let fr = if left { Rect::from_min_size(view.left_top(), vec2(fw, view.height())) } else { Rect::from_min_size(pos2(view.right() - fw, view.top()), vec2(fw, view.height())) };
                pen.hgrad(fr, &if left { [(0.0, dark), (1.0, CLEAR)] } else { [(0.0, CLEAR), (1.0, dark)] });
                let t = pen.t(if left { "‹" } else { "›" }, Fam::Jp500, 22.0, 0.0);
                let x = if left { fr.left() + 3.0 } else { fr.right() - 3.0 - t.w };
                pen.text(&t, x, fr.center().y + 22.0 * BASE_K - 2.0, CREAM, &[]);
            };
            if self.strip_scroll > 1.0 { cue(true); }
            if self.strip_scroll < max_scroll - 1.0 { cue(false); }
        }
    }

    // ---- 結果 ----
    fn result_ui(&self, pen: &Pen, hit: &mut Hit, now: f64, r: Rect, acts: &mut Vec<UiAction>) {
        let (w, h) = (r.width(), r.height());
        let (narrow, short) = (w <= 760.0, h <= 520.0);
        pen.radial(r, pos2(w * 0.5, h * 0.5), w * 0.7, h * 0.6, &[(0.0, rgba(10, 6, 20, 0.5)), (1.0, rgba(10, 6, 20, 0.9))], 32, 20);
        let area = self.safe.inner(r);
        let d = &self.result;
        // 自己ベスト（NEW・最高 N）: 広い画面は値の右の列。幅 760 以下・高さ 520 以下は、項目名の下に小さく出す（列を足す幅が無い）
        let under = narrow || short;
        let (fs_k, fs_v, fs_b, gap_kv, row_gap) = if short { (12.0, 16.0, 10.0, 14.0, 4.0) } else { (15.0, 20.0, 11.0, 26.0, 8.0) };
        let row_h = fs_v * LH;
        let since = now - self.shown_at;
        // 自己ベストの祝い: 数え終わってからの秒（まだなら None）。弾みは「動きを減らす」設定では出さない
        let cheer_t = self.cheer_at.map(|at| (now - at) as f32);
        let bounce = |dur: f32, amp: f32| match cheer_t { Some(u) if !self.calm && u < dur => 1.0 + amp * (u / dur * std::f32::consts::PI).sin(), _ => 1.0 };
        struct RowT { k: T, v: Vec<T>, b: T, best: bool, new: bool, main: bool }
        struct ColT { rows: Vec<RowT>, col_k: f32, col_v: f32, col_b: f32, w: f32, h: f32 }
        // 表の 1 列を測る: max_w に収まらないと値の列が縮んで、長い値は折り返す
        let measure = |list: &[ResultRow], max_w: f32, first: usize| -> ColT {
            let best_t = |x: &ResultRow| pen.t(&x.best, if x.best == "NEW" { Fam::Jp900 } else { Fam::Jp500 }, fs_b, 0.0);
            let k_w = list.iter().map(|x| pen.t(&x.label, Fam::Jp500, fs_k, fs_k * 0.15).w).fold(0.0, f32::max);
            let b_w = list.iter().map(|x| best_t(x).w).fold(0.0, f32::max);
            let (col_k, col_b, extra) = if under { (k_w.max(b_w), 0.0, 0.0) } else { (k_w, b_w, 26.0 + b_w) };
            let col_v_full = list.iter().map(|x| pen.t(&x.value, Fam::Num, fs_v, 0.0).w).fold(0.0, f32::max);
            let col_v = col_v_full.min(max_w - col_k - gap_kv - extra).max(40.0);
            let rows: Vec<RowT> = list.iter().enumerate().map(|(i, x)| RowT {
                k: pen.t(&x.label, Fam::Jp500, fs_k, fs_k * 0.15), v: pen.lines(&x.value, Fam::Num, fs_v, 0.0, col_v + 0.01),
                b: best_t(x), best: !x.best.is_empty(), new: x.best == "NEW", main: d.main == Some(first + i),
            }).collect();
            let h = rows.iter().map(|x| x.v.len().max(1) as f32 * row_h).sum::<f32>() + (rows.len().saturating_sub(1)) as f32 * row_gap;
            ColT { rows, col_k, col_v, col_b, w: col_k + gap_kv + col_v + extra, h }
        };
        let draw_col = |c: &ColT, x0: f32, top: f32| {
            let mut y = top;
            for row in &c.rows {
                let xv = x0 + c.col_k + gap_kv;
                // NEW の印の出し方: 得点の NEW は数え終わりに弾んで出る（数えている間は出さない）。ほかの NEW は最初から出て、数え終わりに小さく弾む
                let counting = row.main && row.new && self.count.is_some();
                let (b_sc, b_op) = if counting {
                    match cheer_t { None => (1.0, 0.0), Some(_) if self.calm => (1.0, 1.0), Some(u) => (lerp(0.3, 1.0, snap(clamp01(u / 0.35))), clamp01(u / 0.12)) }
                } else if row.new { (bounce(0.3, 0.2), 1.0) } else { (1.0, 1.0) };
                if under && row.best {
                    // 項目名を上へ寄せ、そのすぐ下に自己ベスト
                    pen.text(&row.k, x0, y + fs_k * 0.5 - 1.0 + fs_k * BASE_K, CREAM_DIM, &[]);
                    let by = y + fs_k + fs_b * 0.5;
                    let q = pen.sub(Xf::about(x0 + row.b.w * 0.5, by, Xf::scale(b_sc, b_sc)), b_op);
                    if row.new {
                        let g = full_k(since, 0.5);
                        q.soft(Rect::from_min_size(pos2(x0, by - fs_b * 0.6), vec2(row.b.w, fs_b * 1.2)), 0.0, 16.0 * g, rgba(255, 160, 40, g), false);
                    }
                    q.text(&row.b, x0, by + fs_b * BASE_K, if row.new { hx(0xff6a7a) } else { GOLD }, &[]);
                } else {
                    pen.text(&row.k, x0, y + fs_k * LH * 0.5 + fs_k * BASE_K, CREAM_DIM, &[]);
                }
                match self.count.filter(|_| row.main) {
                    // 得点: 数え上げ中の値（終わりは本当の値）。自己ベストでないときは、数え終わりに 1 回だけ小さく弾む
                    Some(n) => {
                        let v = pen.t(&fmt_num(n), Fam::Num, fs_v, 0.0);
                        let s = if self.cheer_best { 1.0 } else { bounce(0.32, 0.16) };
                        let q = pen.sub(Xf::about(xv + c.col_v - v.w * 0.5, y + row_h * 0.5, Xf::scale(s, s)), 1.0);
                        q.text(&v, xv + c.col_v - v.w, y + row_h * 0.5 + fs_v * BASE_K, CREAM, &[]);
                    }
                    None => for (i, v) in row.v.iter().enumerate() { pen.text(v, xv + c.col_v - v.w, y + row_h * (i as f32 + 0.5) + fs_v * BASE_K, CREAM, &[]); },
                }
                if !under && row.best {
                    let xb = xv + c.col_v + 26.0;
                    let cy = y + row_h * 0.5;
                    let q = pen.sub(Xf::about(xb + row.b.w * 0.5, cy, Xf::scale(b_sc, b_sc)), b_op);
                    if row.new {
                        let g = full_k(since, 0.5);
                        q.soft(Rect::from_min_size(pos2(xb, cy - 8.0), vec2(c.col_b, 16.0)), 0.0, 30.0 * g, rgba(255, 160, 40, g), false);
                    }
                    q.text(&row.b, xb, cy + fs_b * BASE_K, if row.new { hx(0xff6a7a) } else { GOLD }, &[]);
                }
                y += row.v.len().max(1) as f32 * row_h + row_gap;
            }
        };
        // 誉れの並び: 幅に収まらないと、札も菱形も字も同じ割合に縮む（flex-shrink）。字は折り返し、菱形は細くなる
        struct HonorT<'a> { rows: Vec<T>, hn: &'a Honor, li_w: f32, i_w: f32 }
        // 返す物: 段ごとの（札, 段の幅）・いちばん広い段の幅・全体の高さ
        let honors_of = |max_w: f32| -> (Vec<(Vec<HonorT>, f32)>, f32, f32) {
            let names: Vec<f32> = d.honors.iter().map(|x| pen.t(&x.name, Fam::Jp500, 12.0, 0.0).w).collect();
            let base: f32 = names.iter().map(|t| 14.0 + 6.0 + t).sum();
            let gaps = d.honors.len().saturating_sub(1) as f32 * 16.0;
            // 1 列に収まらないとき（縦持ち）は、縮めて語の途中で折らずに、段を分ける（「千人／斬り」「百連／鎖」と折れていた）
            if base + gaps > max_w && names.iter().all(|t| 14.0 + 6.0 + t <= max_w) {
                let mut rows: Vec<(Vec<HonorT>, f32)> = vec![];
                for (hn, t) in d.honors.iter().zip(&names) {
                    let li_w = 14.0 + 6.0 + t;
                    let item = HonorT { rows: vec![pen.t(&hn.name, Fam::Jp500, 12.0, 0.0)], hn, li_w, i_w: 14.0 };
                    match rows.last_mut() {
                        Some((r, w)) if *w + 16.0 + li_w <= max_w => { r.push(item); *w += 16.0 + li_w; }
                        _ => rows.push((vec![item], li_w)),
                    }
                }
                let hw = rows.iter().map(|r| r.1).fold(0.0, f32::max);
                let hh = rows.len() as f32 * 12.0 * LH + (rows.len() - 1) as f32 * HONOR_ROW_GAP;
                return (rows, hw, hh);
            }
            let k = if base + gaps > max_w { (max_w - gaps) / base } else { 1.0 };
            let list: Vec<HonorT> = d.honors.iter().zip(&names).map(|(hn, t)| {
                let li_w = (14.0 + 6.0 + t) * k;
                let (over, sum) = ((14.0 + t) - (li_w - 6.0), 14.0 + t);
                let (i_w, t_w) = if over > 0.0 { (14.0 * (1.0 - over / sum), t * (1.0 - over / sum)) } else { (14.0, *t) };
                HonorT { rows: pen.lines(&hn.name, Fam::Jp500, 12.0, 0.0, t_w + 0.01), hn, li_w, i_w }
            }).collect();
            let hw = list.iter().map(|x| x.li_w).sum::<f32>() + gaps;
            let hh = list.iter().map(|x| x.rows.len()).max().unwrap_or(1) as f32 * 12.0 * LH;
            (vec![(list, hw)], hw, hh)
        };
        let draw_honors = |rows: &[(Vec<HonorT>, f32)], cx: f32, top: f32| {
            let lh = 12.0 * LH;
            let mut y = top;
            for (honors, hw) in rows {
                let row_h = honors.iter().map(|x| x.rows.len()).max().unwrap_or(1) as f32 * lh;
                let mut x = cx - hw * 0.5;
                let cy = y + row_h * 0.5;
                for h in honors {
                    let hn = h.hn;
                    if hn.fresh {
                        let g = full_k(since, 0.5);
                        pen.soft(Rect::from_min_size(pos2(x, y), vec2(h.li_w, row_h)), 0.0, 30.0 * g, rgba(255, 160, 40, g), false);
                    }
                    let c = pos2(x + h.i_w * 0.5, cy);
                    let q = pen.sub(Xf::about(c.x, c.y, Xf::rot(45.0)), 1.0);
                    let b = Rect::from_center_size(c, vec2(h.i_w, 14.0));
                    if hn.on { q.soft(b, 0.0, 10.0, rgba(255, 201, 74, 0.8), false); q.rect(b, GOLD); }
                    else { q.frame(b, 1.0, rgba(255, 243, 224, 0.35)); }
                    let col = if hn.on { GOLD } else { rgba(255, 243, 224, 0.4) };
                    pen.text_lines(&h.rows, x + h.i_w + 6.0, cy - h.rows.len() as f32 * lh * 0.5, lh, col, &[]);
                    x += h.li_w + 16.0;
                }
                y += row_h + HONOR_ROW_GAP;
            }
        };
        // 格の判: stamp .7s var(--snap) .5s both（出るたびにやり直す）
        let stamp = |cx: f32, cy: f32, fs: f32| {
            let k = clamp01((since as f32 - 0.5) / 0.7);
            let e = snap(k);
            let (sc, rot, op) = (lerp(3.0, 1.0, e), lerp(-20.0, -6.0, e), if k <= 0.0 { 0.0 } else { clamp01(e) });
            let q = pen.sub(Xf::about(cx, cy, Xf::rot(rot).then(Xf::scale(sc, sc))), op);
            let t = q.t(&d.rank, Fam::Brush, fs, 0.0);
            q.text(&t, cx - t.w * 0.5, cy + fs * BASE_K, GOLD, &[TS(0.0, 0.0, fs / 3.0, rgba(255, 160, 40, 0.8)), TS(0.0, fs * 0.053, 0.0, hx(0x5a0a1a))]);
        };
        // ボタン（横に並べる。キーボード・パッドでは ←→ で移る）
        let again = "もう一度";
        let pw = primary_size(pen, self.confirm(), again).0;
        let bw = plain_w(pen, "武将を選び直す");
        let actions_h = 15.0 * LH + 26.0;
        let actions_w = pw + 14.0 + bw;
        let mut buttons = |hit: &mut Hit, cx: f32, y: f32| {
            let x = cx - actions_w * 0.5;
            // 紙吹雪は中身の前・ボタンの後ろ（ボタンの字はいつも読める）
            self.confetti.draw(pen, &CONFETTI);
            let (br, a) = primary(pen, hit, "again", x + pw * 0.5, y, self.confirm(), again);
            if a { acts.push(UiAction::Again); }
            if self.focused(Screen::Result, UiAction::Again) { focus_ring(pen, br); }
            let top = y + (actions_h - (14.0 * LH + 26.0)) * 0.5;
            let (br2, a) = plain_btn(pen, hit, "title1", x + pw + 14.0 + bw * 0.5, top, "武将を選び直す");
            if a { acts.push(UiAction::ToSelect); }
            if self.focused(Screen::Result, UiAction::ToSelect) { focus_ring(pen, br2); }
            self.result_btns.set([br, br2]);
        };
        let eb_h = 11.0 * LH + 5.5;
        if short {
            // 低い画面: 題と格を左、表（2 列）・誉れ・ボタンを右に並べる（縦に積むと、ボタンが画面の下へ出て押せなかった）
            let (h2, rank_fs) = (44.0, 104.0);
            let t = pen.t(&d.title, Fam::Brush, h2, 0.0);
            let left_w = t.w.max(pen.t(&d.rank, Fam::Brush, rank_fs, 0.0).w).max(pen.t(&d.eyebrow, Fam::Jp700, 11.0, 5.5).w) + 12.0;
            let right_max = (area.width() - 24.0 - left_w - 28.0).max(200.0);
            let split = d.rows.len().div_ceil(2);
            let col_max = (right_max - 24.0) * 0.5;
            let (c1, c2) = (measure(&d.rows[..split], col_max, 0), measure(&d.rows[split..], col_max, split));
            let (stats_w, stats_h) = (c1.w + 24.0 + c2.w, c1.h.max(c2.h));
            let (honors, hw, honors_h) = honors_of(right_max);
            let right_w = stats_w.max(hw).max(actions_w);
            let right_h = stats_h + 12.0 + honors_h + 14.0 + actions_h;
            let left_h = eb_h + h2 + 4.0 + rank_fs;
            let x0 = area.center().x - (left_w + 28.0 + right_w) * 0.5;
            let cy = area.center().y;
            // 紙吹雪を出す角: 中身の箱（題と格の左端から、表とボタンの右端まで）
            let block_h = left_h.max(right_h);
            self.result_card.set(Some(Rect::from_min_size(pos2(x0, cy - block_h * 0.5), vec2(left_w + 28.0 + right_w, block_h))));
            let lx = x0 + left_w * 0.5;
            let mut y = cy - left_h * 0.5;
            y += eyebrow(pen, &d.eyebrow, lx, y);
            pen.text(&t, lx - t.w * 0.5, y + h2 * 0.5 + h2 * BASE_K, CREAM, &[TS(0.0, 0.0, 30.0, rgba(255, 90, 60, 0.6))]);
            y += h2 + 4.0;
            stamp(lx, y + rank_fs * 0.5, rank_fs);
            let rx = x0 + left_w + 28.0 + right_w * 0.5;
            let mut y = cy - right_h * 0.5;
            draw_col(&c1, rx - stats_w * 0.5, y);
            draw_col(&c2, rx - stats_w * 0.5 + c1.w + 24.0, y);
            y += stats_h + 12.0;
            draw_honors(&honors, rx, y);
            y += honors_h + 14.0;
            buttons(hit, rx, y);
            return;
        }
        let h2_max = cl(56.0, w * 0.08, 96.0);
        let col = measure(&d.rows, area.width(), 0);
        let (honors, _, honors_h) = honors_of(area.width() - 24.0);
        // 縦に収まらないときは、格の判（150 → 96px）、次に題（→ 56px）を小さくして、上下に 16px の余白を残す
        // （1280×720 では題が上へ、ボタンが下へ切れていた。安全域のある縦持ちも同じ）
        let fixed = eb_h + 10.0 + 6.0 + 10.0 + col.h + 26.0 - 10.0 + honors_h + 24.0 + actions_h;
        let room = area.height() - 32.0 - fixed;
        let rank_fs = (room - h2_max).clamp(96.0, 150.0);
        let h2 = (room - rank_fs).clamp(56.0_f32.min(h2_max), h2_max);
        let total = fixed + h2 + rank_fs;
        let mut y = area.top() + ((area.height() - total) * 0.5).max(0.0);
        let cx = w * 0.5;
        // 紙吹雪を出す角: 中身の箱（いちばん広い段の幅 × 題からボタンまで）
        let card_w = col.w.max(honors.iter().map(|r| r.1).fold(0.0, f32::max)).max(actions_w);
        self.result_card.set(Some(Rect::from_min_size(pos2(cx - card_w * 0.5, y), vec2(card_w, total))));
        y += eyebrow(pen, &d.eyebrow, cx, y);
        let t = pen.t(&d.title, Fam::Brush, h2, 0.0);
        pen.text(&t, cx - t.w * 0.5, y + h2 * 0.5 + h2 * BASE_K, CREAM, &[TS(0.0, 0.0, 40.0, rgba(255, 90, 60, 0.6))]);
        y += h2 + 10.0;
        stamp(cx, y + rank_fs * 0.5, rank_fs);
        y += rank_fs + 6.0 + 10.0;
        draw_col(&col, cx - col.w * 0.5, y);
        y += col.h + 26.0 - 10.0;
        draw_honors(&honors, cx, y);
        y += honors_h + 24.0;
        buttons(hit, cx, y);
    }

    /// .card の枠と背景（backdrop-filter の代わりはしない）
    fn card(pen: &Pen, r: Rect) {
        pen.rect(r, rgba(14, 9, 26, 0.88));
        pen.frame(r, 1.0, LINE);
    }

    // ---- 敗北 ----
    fn gameover(&self, pen: &Pen, hit: &mut Hit, r: Rect, acts: &mut Vec<UiAction>) {
        let area = self.safe.inner(r);
        // 狭い・低い画面は札が HUD の上に重なるので、後ろを暗くする
        if r.width() <= 760.0 || r.height() <= 520.0 { pen.rect(r, rgba(10, 6, 20, 0.6)); }
        // 札の中の広さ（安全域の内側 - 余白 40 × 2 - 枠 1 × 2）より長いと折り返す。そのとき箱はいっぱいになり、文字は左へ寄る
        let avail = area.width() - 82.0;
        // 一言の中の操作は、最後に触った機器の表記
        let hint = pen.lines(&controls::defeat_hint(self.device), Fam::Jp500, 14.0, 0.0, avail);
        let hint_h = hint.len() as f32 * 14.0 * LH;
        let hint_w = if hint.len() > 1 { avail } else { hint[0].w };
        let t = pen.t("無念…", Fam::Brush, 48.0, 0.0);
        let pw = primary_size(pen, self.confirm(), "立ち上がる");
        let bw = plain_w(pen, "退く（タイトルへ）");
        let eb = pen.t("FALLEN", Fam::Jp700, 11.0, 5.5);
        let inner_w = hint_w.max(t.w).max(pw.0).max(bw).max(eb.w);
        let bh = 14.0 * LH + 26.0;
        // eyebrow, h2, hint, menu の間は gap 6
        let inner_h = (11.0 * LH + 5.5) + 6.0 + (48.0 * LH + 12.0) + 6.0 + (hint_h + 16.0) + 6.0 + (pw.1 + 14.0 + bh);
        let (cw, ch) = (inner_w + 82.0, inner_h + 62.0);
        let card = Rect::from_center_size(area.center(), vec2(cw, ch));
        Self::card(pen, card);
        let cx = card.center().x;
        let mut y = card.top() + 31.0;
        y += eyebrow(pen, "FALLEN", cx, y) + 6.0;
        pen.text(&t, cx - t.w * 0.5, y + 48.0 * LH * 0.5 + 48.0 * BASE_K, CREAM, &[]);
        y += 48.0 * LH + 12.0 + 6.0;
        pen.text_lines(&hint, cx - hint_w * 0.5, y, 14.0 * LH, CREAM_DIM, &[]);
        y += hint_h + 16.0 + 6.0;
        let (br, a) = primary(pen, hit, "continue", cx, y, self.confirm(), "立ち上がる");
        if a { acts.push(UiAction::Continue); }
        if self.focused(Screen::Gameover, UiAction::Continue) { focus_ring(pen, br); }
        y += pw.1 + 14.0;
        let (br, a) = plain_btn(pen, hit, "title2", cx, y, "退く（タイトルへ）");
        if a { acts.push(UiAction::ToTitle); }
        if self.focused(Screen::Gameover, UiAction::ToTitle) { focus_ring(pen, br); }
    }

    // ---- 一時停止と技の一覧 ----
    fn pause(&self, pen: &Pen, hit: &mut Hit, r: Rect, acts: &mut Vec<UiAction>) {
        let (w, h) = (r.width(), r.height());
        let (narrow, short) = (w <= 760.0, h <= 520.0);
        // 札は安全域の内側（外に 8px の余白）
        let area = self.safe.inner(r).shrink(8.0);
        let sound = if self.muted { "音: オフ" } else { "音: オン" };
        // 狭い・低い画面は札が HUD の上に重なるので、後ろを暗くする（HUD の字が透けて表が読みにくい）
        if short || narrow { pen.rect(r, rgba(10, 6, 20, 0.6)); }
        let (px, py) = if short || narrow { (18.0, 16.0) } else { (41.0, 31.0) };
        let title_fs = if short { 32.0 } else if narrow { 40.0 } else { 48.0 };
        let gap = if short { 10.0 } else { 14.0 };
        let pw = primary_size(pen, "", "戦いに戻る");
        let bh = 14.0 * LH + 26.0;
        let menu_w = pw.0.max(plain_w(pen, sound)).max(plain_w(pen, "退く（タイトルへ）"));
        let menu_h = pw.1 + gap + bh + gap + bh;
        let t = pen.t("軍議", Fam::Brush, title_fs, 0.0);
        let head_h = (11.0 * LH + 5.5) + 6.0 + (title_fs * LH + 12.0);
        // 低い画面は、技の表を左・見出しとボタンを右に並べる（縦に積むと、ボタンが画面の外へ出て押せなかった）
        let side_w = menu_w.max(t.w);
        let (tw_max, th_max) = if short { (area.width() - px * 2.0 - side_w - 28.0, area.height() - py * 2.0) } else { (area.width() - px * 2.0, area.height() - py * 2.0 - head_h - 6.0 - 16.0 - 6.0 - menu_h) };
        // 表が高さに収まるまで、行の間と字を詰める
        let tries: &[(f32, f32)] = if short { &[(13.0, 6.0), (12.0, 5.0), (12.0, 3.0), (11.0, 2.0)] } else { &[(13.0, 9.0), (13.0, 6.0), (12.0, 5.0), (12.0, 3.0), (11.0, 2.0)] };
        let mut tb = move_table(pen, &self.moves, self.device, tw_max, tries[0].0, tries[0].1);
        for (fs, pad) in &tries[1..] { if tb.h <= th_max { break; } tb = move_table(pen, &self.moves, self.device, tw_max, *fs, *pad); }
        let mut menu = |hit: &mut Hit, cx: f32, top: f32| {
            let mut y = top;
            let (br, a) = primary(pen, hit, "resume", cx, y, "", "戦いに戻る");
            if a { acts.push(UiAction::Resume); }
            if self.focused(Screen::Pause, UiAction::Resume) { focus_ring(pen, br); }
            y += pw.1 + gap;
            let (br, a) = plain_btn(pen, hit, "sound", cx, y, sound);
            if a { acts.push(UiAction::ToggleSound); }
            if self.focused(Screen::Pause, UiAction::ToggleSound) { focus_ring(pen, br); }
            y += bh + gap;
            let (br, a) = plain_btn(pen, hit, "title3", cx, y, "退く（タイトルへ）");
            if a { acts.push(UiAction::ToTitle); }
            if self.focused(Screen::Pause, UiAction::ToTitle) { focus_ring(pen, br); }
        };
        let head = |cx: f32, top: f32| {
            let y = top + eyebrow(pen, "PAUSE", cx, top) + 6.0;
            pen.text(&t, cx - t.w * 0.5, y + title_fs * LH * 0.5 + title_fs * BASE_K, CREAM, &[]);
        };
        if short {
            let right_h = head_h + 6.0 + menu_h;
            let (iw, ih) = (tb.w + 28.0 + side_w, tb.h.max(right_h));
            let card = Rect::from_center_size(area.center(), vec2(iw + px * 2.0, ih + py * 2.0));
            Self::card(pen, card);
            tb.draw(pen, card.left() + px, card.center().y - tb.h * 0.5);
            let cx = card.right() - px - side_w * 0.5;
            let top = card.center().y - right_h * 0.5;
            head(cx, top);
            menu(hit, cx, top + head_h + 6.0);
            return;
        }
        let inner_h = head_h + 6.0 + (tb.h + 16.0) + 6.0 + menu_h;
        let inner_w = tb.w.max(menu_w);
        let card = Rect::from_center_size(area.center(), vec2(inner_w + px * 2.0, inner_h + py * 2.0));
        Self::card(pen, card);
        let cx = card.center().x;
        let mut y = card.top() + py;
        head(cx, y);
        y += head_h + 6.0;
        tb.draw(pen, cx - tb.w * 0.5, y);
        y += tb.h + 16.0 + 6.0;
        menu(hit, cx, y);
    }

    // ---- 乱舞の切り込み ----
    fn cutin(&self, pen: &Pen, now: f64, r: Rect) {
        let Some(at) = self.cutin_at else { return };
        let (w, h) = (r.width(), r.height());
        let t = (now - at) as f32;
        let k = clamp01(t / 0.9);
        // 帯: 左右に 10% はみ出して、中央で回る
        let (bw, bh) = (w * 1.2, h * 0.34);
        let sy = kf(k, &[(0.0, 0.0), (0.14, 1.0), (0.82, 1.0), (1.0, 1.4)], ease);
        let op = kf(k, &[(0.82, 1.0), (1.0, 0.0)], ease);
        let band = pen.sub(Xf::tr(w * 0.5, h * 0.5).then(Xf::rot(-8.0)).then(Xf::scale(1.0, sy)).then(Xf::tr(-bw * 0.5, -bh * 0.5)), op);
        let br = Rect::from_min_size(pos2(0.0, 0.0), vec2(bw, bh));
        band.soft(br, 0.0, 60.0, rgba(255, 90, 40, 0.6), false);
        band.rect(br.expand(3.0), GOLD);
        band.hgrad(br, &[(0.0, rgba(20, 6, 14, 0.96)), (0.5, rgba(120, 20, 40, 0.94)), (1.0, rgba(20, 6, 14, 0.96))]);
        // 流れる線
        let off = -88.0 * (t / 0.25).rem_euclid(1.0);
        let mut x = off;
        while x < bw {
            let (l, rr) = ((x + 40.0).max(0.0), (x + 44.0).min(bw));
            if rr > l { band.rect(Rect::from_min_max(pos2(l, 0.0), pos2(rr, bh)), rgba(255, 220, 160, 0.12)); }
            x += 44.0;
        }
        // 肖像の札
        {
            let (fw, fh) = (bw * 0.3, bh * 0.88);
            let tx = kf(k, &[(0.0, -0.8), (0.18, 0.0), (0.82, 0.04), (1.0, 0.1)], ease) * fw;
            let fop = kf(k, &[(0.0, 0.0), (0.18, 1.0), (0.82, 1.0), (1.0, 0.0)], ease);
            let f = band.sub(Xf::tr(bw * 0.08 + tx, bh * 0.06), fop);
            let bx = Rect::from_min_size(pos2(0.0, 0.0), vec2(fw, fh));
            let poly = [pos2(fw * 0.14, 0.0), pos2(fw, 0.0), pos2(fw * 0.86, fh), pos2(0.0, fh)];
            f.poly_grad(&poly, bx, 160.0, hx(0xfff6e6), hx(0xffd8a8));
            if let Some(img) = &self.ci_face {
                let ib = Rect::from_min_size(pos2(0.0, 0.0), vec2(fw, fh * 1.7));
                let uv = cover_uv(img, ib.width(), ib.height(), 0.5, 0.0);
                f.image_poly(img.handle.id(), &poly, ib, uv, WHITE);
            }
            f.slant(bx, 0.14, &[(0.0, rgba(255, 201, 74, 0.0)), (0.6, rgba(255, 201, 74, 0.0)), (1.0, rgba(255, 201, 74, 0.5))]);
        }
        // 乱・舞
        {
            let fs = cl(90.0, w * 0.14, 200.0);
            let tt = [band.t("乱", Fam::Brush, fs, 0.0), band.t("舞", Fam::Brush, fs, 0.0)];
            let gap = fs * 0.05;
            let tw = tt[0].w + gap + tt[1].w;
            let right = bw * 0.9;
            let (cxx, cyy) = (right - tw * 0.5, bh * 0.5);
            let group = band.sub(Xf::about(cxx, cyy, Xf::rot(8.0)), 1.0);
            let mut x = right - tw;
            for (i, tx) in tt.iter().enumerate() {
                let delay = if i == 0 { 0.0 } else { 0.08 };
                let kk = clamp01((t - delay) / 0.9);
                let sc = kf(kk, &[(0.0, 2.4), (0.25, 1.0)], snap);
                let cop = kf(kk, &[(0.0, 0.0), (0.25, 1.0), (0.85, 1.0), (1.0, 0.0)], snap).clamp(0.0, 1.0);
                let blur = kf(kk, &[(0.0, 10.0), (0.25, 0.0)], snap).max(0.0);
                let col = if i == 0 { CREAM } else { GOLD };
                let q = group.sub(Xf::about(x + tx.w * 0.5, cyy, Xf::scale(sc, sc)), cop);
                q.text_f(tx, x, cyy + fs * BASE_K, Fill::S(col), &[TS(0.0, 0.0, 30.0, rgba(255, 180, 60, 0.9)), TS(0.0, 6.0, 0.0, hx(0x5a0a1a))], blur);
                x += tx.w + gap;
            }
        }
        // 名前
        if !self.ci_name.is_empty() {
            let tn = band.t(&self.ci_name, Fam::Mincho800, 14.0, 7.0);
            let (x1, cy) = (bw * 0.88, bh * 0.88 - 14.0 * LH * 0.5);
            let q = band.sub(Xf::about(x1 - tn.w * 0.5, cy, Xf::rot(8.0)), 1.0);
            q.text(&tn, x1 - tn.w, cy + 14.0 * BASE_K, GOLD, &[]);
        }
    }

    fn letterbox_ui(&self, pen: &Pen, now: f64, r: Rect) {
        let k = self.letter.get(now);
        if k <= 0.0 { return; }
        let bar = r.height() * 0.1 * k;
        pen.rect(Rect::from_min_size(r.min, vec2(r.width(), bar)), hx(0x000000));
        pen.rect(Rect::from_min_max(pos2(r.left(), r.bottom() - bar), r.max), hx(0x000000));
    }

    // ---- タッチのボタン ----
    fn touch_ui(&self, pen: &Pen, r: Rect) {
        // 指の部品は、最後に触ったのが指で、戦いの間で、上に画面（一時停止・結果・敗北など）が出ていないときだけ
        if !self.touch() || !self.touch_on || self.shown.is_some() { return; }
        let (w, h) = (r.width(), r.height());
        let v = &self.touch_view;
        // スティックの土台は毎コマ、input の中心（指に付いてくる）に描く
        let c = v.stick.map_or(stick_rest(w, h, self.safe), |(x, y)| pos2(x, y));
        pen.circle(c, 60.0, rgba(10, 6, 20, 0.25));
        pen.circle_line(c, 59.0, 2.0, rgba(255, 243, 224, 0.25));
        pen.circle(c + vec2(v.knob.0, v.knob.1), 25.0, rgba(255, 243, 224, 0.45));
        let k = if h <= 520.0 { 0.82 } else { 1.0 };
        for (i, b) in TOUCH_BTNS.iter().enumerate() {
            let rect = touch_button_rect(*b, w, h, self.safe);
            let Some(act) = b.act() else {
                // Ⅱ（見た目は 40px、押せる箱は 48px）
                pen.rect(rect, rgba(10, 6, 20, 0.5));
                pen.frame(rect, 1.0, LINE);
                let cc = rect.center();
                for dx in [-4.5, 4.5] { pen.rect(Rect::from_center_size(cc + vec2(dx, 0.0), vec2(4.0, 16.0)), CREAM); }
                continue;
            };
            // 字は割り当ての表から（一時停止の技の表と同じ字）
            let label = controls::label(act, Device::Touch);
            let (fs, border) = match b {
                TouchBtn::Attack => (30.0, rgba(255, 243, 224, 0.4)),
                TouchBtn::Heavy | TouchBtn::Jump => (24.0, rgba(255, 243, 224, 0.4)),
                TouchBtn::Dodge => (18.0, rgba(255, 243, 224, 0.4)),
                TouchBtn::Wire => (20.0, rgba(255, 243, 224, 0.4)),
                _ => (18.0, GOLD),
            };
            let cc = rect.center();
            let rad = rect.width() * 0.5;
            pen.circle(cc, rad, if v.on[i] { rgba(255, 201, 74, 0.4) } else { rgba(10, 6, 20, 0.45) });
            pen.circle_line(cc, rad - k, 2.0 * k, border);
            let t = pen.t(label, Fam::Brush, fs * k, 0.0);
            pen.text(&t, cc.x - t.w * 0.5, cc.y + fs * k * BASE_K, CREAM, &[]);
        }
    }
}

/// 一時停止の技の表（入力・名前・説明）。入力の列は最後に触った機器の表記
struct MoveTable { rows: Vec<MoveLine>, c0: f32, c1: f32, fs: f32, pad: f32, w: f32, h: f32 }
struct MoveLine { input: Vec<P>, n: Vec<T>, d: Vec<T>, h: f32 }
/// 表を測る（table の幅の決め方: avail に収まらないと、列ごとに最小と最大の間で同じ割合に縮める。入力の列は折らない）
fn move_table(pen: &Pen, moves: &[MoveRow], dev: Device, avail: f32, fs: f32, pad: f32) -> MoveTable {
    // キーボードと指は 1 枚の札（J×2→K・斬×2→強）。パッドはボタンごとの札（X/□ ×2 → Y/△）
    let cell = |m: &MoveRow| -> Vec<P> {
        if dev != Device::Pad { return vec![P::K(Kbd::plain(pen, &controls::input_text(&m.input, dev), fs, 0.0))]; }
        m.input.iter().map(|t| match t {
            Tok::In(a) => P::K(Kbd::plain(pen, controls::label(*a, dev), fs, 0.0)),
            Tok::Txt(s) => P::T(pen.t(s, Fam::Jp700, fs * 0.74, 0.0), CREAM, vec![]),
        }).collect()
    };
    let cells: Vec<Vec<P>> = moves.iter().map(cell).collect();
    let c0 = cells.iter().map(|c| row_w(c)).fold(0.0, f32::max) + 24.0;
    let (n_fam, d_fam) = (Fam::Mincho800, Fam::Jp500);
    let widest = |f: &dyn Fn(&MoveRow) -> f32| moves.iter().map(f).fold(0.0, f32::max) + 24.0;
    let (n_max, n_min) = (widest(&|m| pen.t(m.name, n_fam, fs, 0.0).w), widest(&|m| pen.min_w(m.name, n_fam, fs, 0.0)));
    let (d_max, d_min) = (widest(&|m| pen.t(m.desc, d_fam, fs, 0.0).w), widest(&|m| pen.min_w(m.desc, d_fam, fs, 0.0)));
    let (sum_max, sum_min) = (c0 + n_max + d_max, c0 + n_min + d_min);
    let k = if sum_max <= avail { 1.0 } else { clamp01((avail - sum_min) / (sum_max - sum_min).max(1.0)) };
    let (c1, c2) = (n_min + (n_max - n_min) * k, d_min + (d_max - d_min) * k);
    let lh = fs * LH;
    let rows: Vec<MoveLine> = moves.iter().zip(cells).map(|(m, input)| {
        let (n, d) = (pen.lines(m.name, n_fam, fs, 0.0, c1 - 24.0 + 0.01), pen.lines(m.desc, d_fam, fs, 0.0, c2 - 24.0 + 0.01));
        let h = n.len().max(d.len()).max(1) as f32 * lh + pad;
        MoveLine { input, n, d, h }
    }).collect();
    let h = rows.iter().map(|r| r.h).sum();
    MoveTable { rows, c0, c1, fs, pad, w: c0 + c1 + c2, h }
}
impl MoveTable {
    fn draw(&self, pen: &Pen, x0: f32, top: f32) {
        let lh = self.fs * LH;
        let mut y = top;
        for row in &self.rows {
            // 枠は縦の真ん中
            let mid = y + (row.h - 1.0) * 0.5;
            row_draw(pen, &row.input, x0 + 12.0, mid + self.fs * BASE_K);
            pen.text_lines(&row.n, x0 + self.c0 + 12.0, mid - row.n.len() as f32 * lh * 0.5, lh, GOLD, &[]);
            pen.text_lines(&row.d, x0 + self.c0 + self.c1 + 12.0, mid - row.d.len() as f32 * lh * 0.5, lh, CREAM_DIM, &[]);
            pen.rect(Rect::from_min_size(pos2(x0, y + row.h - 1.0), vec2(self.w, 1.0)), rgba(255, 243, 224, 0.08));
            y += row.h;
        }
        let _ = self.pad;
    }
}

/// PATH_SVG の 3 種の図（viewBox 0 0 38 22）を同じ線で
pub fn draw_path_svg(pen: &Pen, kind: PathKind, o: Pos2) {
    let at = |x: f32, y: f32| pos2(o.x + x, o.y + y);
    pen.circle(at(4.0, 11.0), 3.0, GOLD);
    let stroke = |pts: &[Pos2]| {
        pen.line(pts, 2.0, GOLD);
        // stroke-linecap: round
        pen.circle(pts[0], 1.0, GOLD);
        pen.circle(pts[pts.len() - 1], 1.0, GOLD);
    };
    let cubic = |a: (f32, f32), b: (f32, f32), c: (f32, f32), d: (f32, f32)| -> Vec<Pos2> {
        (0..=20).map(|i| {
            let t = i as f32 / 20.0;
            let m = 1.0 - t;
            let f = |p: f32, q: f32, r: f32, s: f32| m * m * m * p + 3.0 * m * m * t * q + 3.0 * m * t * t * r + t * t * t * s;
            at(f(a.0, b.0, c.0, d.0), f(a.1, b.1, c.1, d.1))
        }).collect()
    };
    match kind {
        PathKind::Straight => { for y in [7.0, 11.0, 15.0] { stroke(&[at(9.0, y), at(34.0, y)]); } }
        PathKind::Spread => {
            stroke(&[at(9.0, 11.0), at(34.0, 2.0)]);
            stroke(&[at(9.0, 11.0), at(35.0, 11.0)]);
            stroke(&[at(9.0, 11.0), at(34.0, 20.0)]);
        }
        PathKind::Curve => {
            stroke(&cubic((9.0, 13.0), (18.0, 22.0), (30.0, 18.0), (34.0, 4.0)));
            stroke(&cubic((9.0, 9.0), (16.0, 16.0), (26.0, 14.0), (30.0, 2.0)));
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    /// 画面を 1 コマ回す（押す操作を混ぜられる）
    pub(crate) fn pass<R>(ctx: &Context, time: f64, events: Vec<egui::Event>, f: impl FnOnce(&Context) -> R) -> R {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))), time: Some(time), events, ..Default::default() };
        ctx.begin_pass(raw);
        let r = f(ctx);
        let mut out = ctx.end_pass();
        out.textures_delta.clear();
        r
    }
    pub(crate) fn press(x: f32, y: f32, pressed: bool) -> Vec<egui::Event> {
        vec![egui::Event::PointerMoved(pos2(x, y)), egui::Event::PointerButton { pos: pos2(x, y), button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }]
    }
    pub(crate) fn test_ctx() -> Context { let c = Context::default(); install_fonts(&c); setup_ctx(&c); c }
    // 防ぐ故障: 「みんなで」を押しても何も起きない・違う場所へ飛ぶ（このゲームに協力プレイは無く、元のゲームのページへ送る。docs/gyogun-spec.md）
    #[test]
    fn みんなでを選ぶと元のゲームの協力プレイの入口を開く() {
        let ctx = test_ctx();
        pass(&ctx, 0.0, vec![], |_| {});
        let mut s = Screens::new(&ctx);
        s.show(Some(Screen::Title));
        let mut t = 1.0;
        for _ in 0..3 { pass(&ctx, t, vec![], |c| s.ui(c)); t += 0.016; }
        // 左上の札を押す（クリック・指）
        pass(&ctx, t, vec![egui::Event::PointerMoved(pos2(60.0, 36.0))], |c| s.ui(c)); t += 0.016;
        pass(&ctx, t, press(60.0, 36.0, true), |c| s.ui(c)); t += 0.016;
        let a = pass(&ctx, t, press(60.0, 36.0, false), |c| s.ui(c));
        assert_eq!(a, vec![UiAction::Online]);
        assert_eq!(link_of(a[0]), Some("https://hyakka.yukikau.jp/?stage=hama&coop"));
        // キーボード・パッドでも同じ項目を選べる
        assert!(s.menu().contains(&UiAction::Online));
        // ほかの項目は外へ出ない
        for other in [UiAction::Start, UiAction::Go, UiAction::ToTitle, UiAction::Again, UiAction::Continue] { assert_eq!(link_of(other), None); }
    }

    // 防ぐ故障: 「みんなで」が別のゲームへ移ると分からない（行き先の名前を札に出す）
    #[test]
    fn みんなでの札は行き先のゲームの名前を出す() {
        assert_eq!(ONLINE_LABEL, "百菓繚乱でみんなで遊ぶ");
    }

    // 防ぐ故障: タイトルからキャラを作りに行けない（Studio を開く副ボタン。主ボタンの出陣は今までどおり先頭）
    #[test]
    fn キャラを作って出陣を選ぶと同じ場所のstudioを開く() {
        let ctx = test_ctx();
        pass(&ctx, 0.0, vec![], |_| {});
        let mut s = Screens::new(&ctx);
        s.show(Some(Screen::Title));
        let mut t = 1.0;
        for _ in 0..3 { pass(&ctx, t, vec![], |c| s.ui(c)); t += 0.016; }
        let r = s.title_make.get();
        assert!(r.width() > 0.0, "副ボタンが描かれていない");
        let c = r.center();
        pass(&ctx, t, vec![egui::Event::PointerMoved(c)], |c| s.ui(c)); t += 0.016;
        pass(&ctx, t, press(c.x, c.y, true), |c| s.ui(c)); t += 0.016;
        let a = pass(&ctx, t, press(c.x, c.y, false), |c| s.ui(c));
        assert_eq!(a, vec![UiAction::MakeChar]);
        assert_eq!(link_of(UiAction::MakeChar), Some("/character-maker"));
        assert_eq!(crate::platform::native_url("/character-maker"), "http://127.0.0.1:4284/character-maker");
        assert_eq!(crate::platform::native_url(ONLINE_URL), ONLINE_URL);
        // キーボード・パッド: 出陣が先頭のまま、次が副ボタン
        assert_eq!(s.menu(), vec![UiAction::Start, UiAction::MakeChar, UiAction::Online]);
    }

    // ボタンが押せないと画面を進められない（押した場所と結果の対応を確かめる）
    #[test]
    fn title_buttons_click() {
        let ctx = test_ctx();
        pass(&ctx, 0.0, vec![], |_| {});
        let mut s = Screens::new(&ctx);
        s.show(Some(Screen::Title));
        let mut t = 1.0;
        for _ in 0..3 { pass(&ctx, t, vec![], |c| s.ui(c)); t += 0.016; }
        // 音の札（右上）
        pass(&ctx, t, vec![egui::Event::PointerMoved(pos2(1230.0, 36.0))], |c| s.ui(c)); t += 0.016;
        pass(&ctx, t, press(1230.0, 36.0, true), |c| s.ui(c)); t += 0.016;
        let a = pass(&ctx, t, press(1230.0, 36.0, false), |c| s.ui(c)); t += 0.016;
        assert_eq!(a, vec![UiAction::ToggleSound]);
        // 出陣（bottom 14vh の中央）
        let y = 800.0 - 800.0 * 0.14 - 25.0;
        pass(&ctx, t, vec![egui::Event::PointerMoved(pos2(640.0, y))], |c| s.ui(c)); t += 0.016;
        assert!(over_ui(&ctx, pos2(640.0, y)) && over_ui(&ctx, pos2(10.0, 400.0)), "表示中の画面はクリックをゲームへ通さない");
        pass(&ctx, t, press(640.0, y, true), |c| s.ui(c)); t += 0.016;
        let a = pass(&ctx, t, press(640.0, y, false), |c| s.ui(c));
        assert_eq!(a, vec![UiAction::Start]);
    }
    // 隠した画面のボタンは押せない（pointer-events: none）
    #[test]
    fn hidden_screen_does_not_click() {
        let ctx = test_ctx();
        pass(&ctx, 0.0, vec![], |_| {});
        let mut s = Screens::new(&ctx);
        s.show(Some(Screen::Title));
        let mut t = 1.0;
        for _ in 0..3 { pass(&ctx, t, vec![], |c| s.ui(c)); t += 0.016; }
        s.show(None);
        pass(&ctx, t, vec![egui::Event::PointerMoved(pos2(1230.0, 36.0))], |c| s.ui(c)); t += 0.016;
        pass(&ctx, t, press(1230.0, 36.0, true), |c| s.ui(c)); t += 0.016;
        let a = pass(&ctx, t, press(1230.0, 36.0, false), |c| s.ui(c));
        assert!(a.is_empty());
        assert!(!over_ui(&ctx, pos2(1230.0, 36.0)));
    }
    /// 指の部品と Ⅱ は端の安全域の内側。押せる箱は 48px 以上で、見た目を含む。Ⅱ は小地図・ほかのボタン・目標の札の場所と重ならない
    #[test]
    fn touch_parts_inside_safe_area() {
        for (w, h, safe) in [(844.0, 390.0, [0.0, 47.0, 21.0, 47.0]), (390.0, 844.0, [47.0, 0.0, 34.0, 0.0]), (1180.0, 820.0, [0.0; 4]), (820.0, 1180.0, [24.0, 0.0, 20.0, 0.0]), (667.0, 375.0, [0.0; 4])] {
            let s = Safe::from_array(safe);
            let inner = s.inner(Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h)));
            let hits: Vec<Rect> = TOUCH_BTNS.iter().map(|b| touch_hit_rect(*b, w, h, s)).collect();
            for (i, b) in TOUCH_BTNS.iter().enumerate() {
                let look = touch_button_rect(*b, w, h, s);
                assert!(inner.contains_rect(look) && inner.contains_rect(hits[i]), "{b:?} が安全域の外 {w}x{h}");
                assert!(hits[i].width() >= HIT_MIN && hits[i].height() >= HIT_MIN && hits[i].contains_rect(look), "{b:?} の押せる箱 {w}x{h}");
            }
            let pause = hits[6];
            let map = Rect::from_min_size(map_origin(w, h, s), vec2(map_size(w, h), map_size(w, h)));
            assert!(!pause.intersects(map), "Ⅱ が小地図に重なる {w}x{h}");
            assert!(hits[..6].iter().all(|r| !r.intersects(pause)), "Ⅱ がボタンに重なる {w}x{h}");
            // 「糸」は、ほかのボタンの見た目に重ならない
            let wire = touch_button_rect(TouchBtn::Wire, w, h, s);
            assert!(TOUCH_BTNS.iter().filter(|b| **b != TouchBtn::Wire).all(|b| !touch_button_rect(*b, w, h, s).intersects(wire)), "糸 がボタンに重なる {w}x{h}");
            // 小地図の左に置くときは、上の真ん中の目標の札（いちばん長い文で幅 390px）に掛からない
            if !pause_under_map(w, h) && h > 520.0 { assert!(pause.left() >= w * 0.5 + 195.0, "Ⅱ が目標の札に掛かる {w}x{h}"); }
            let rest = stick_rest(w, h, s);
            assert!(inner.contains_rect(Rect::from_center_size(rest, vec2(120.0, 120.0))), "スティックの既定の位置 {w}x{h}");
        }
        assert_eq!(Safe::parse("0,47,21,47"), Some(Safe { top: 0.0, right: 47.0, bottom: 21.0, left: 47.0 }));
        assert_eq!(Safe::parse("1,2,3"), None);
        assert_eq!(Safe::parse("0,-47,21,47"), None, "負の数は無視（共通部品の読み方）");
    }
    /// 指のボタンの字は割り当ての表から（一時停止の技の表の入力の列と同じ字）
    #[test]
    fn touch_button_names_from_bindings() {
        assert_eq!(TOUCH_BTNS.map(|b| b.name()), ["ranbu", "heavy", "attack", "jump", "dodge", "wire", "pause"]);
        assert_eq!(TOUCH_BTNS.map(|b| b.act().map_or("Ⅱ", |a| controls::label(a, Device::Touch))), ["乱舞", "強", "斬", "跳", "避", "糸", "Ⅱ"]);
    }
    /// 一時停止・結果・敗北・読み込みの失敗は、キーボードだけ・パッドだけで全部の項目を選べる。結果と敗北は出てから 0.6 秒は決定を受けない
    #[test]
    fn keyboard_nav() {
        let ctx = test_ctx();
        pass(&ctx, 0.0, vec![], |_| {});
        let mut s = Screens::new(&ctx);
        // 読み込み中は項目なし。失敗すると「読み込み直す」
        assert_eq!(s.nav(false, false, true), None);
        s.load_failed(Some("読めません"));
        assert_eq!(s.nav(false, false, true), Some(UiAction::Reload));
        s.show(Some(Screen::Pause));
        assert_eq!(s.nav(false, false, true), Some(UiAction::Resume));
        assert_eq!((s.nav(false, true, false), s.focus()), (None, 1));
        assert_eq!(s.nav(false, false, true), Some(UiAction::ToggleSound));
        assert_eq!(s.nav(false, true, true), Some(UiAction::ToTitle));
        // 端は回る
        assert_eq!((s.nav(false, true, false), s.focus()), (None, 0));
        assert_eq!((s.nav(true, false, false), s.focus()), (None, 2));
        // 画面が替わると先頭へ戻る。結果は 0.6 秒たつまで押せない（移るのはできる）
        s.show(Some(Screen::Result));
        assert_eq!((s.focus(), s.nav(false, false, true)), (0, None));
        s.advance(0.5);
        assert_eq!(s.nav(false, false, true), None);
        s.advance(0.11);
        assert_eq!(s.nav(false, false, true), Some(UiAction::Again));
        assert_eq!(s.nav(false, true, true), Some(UiAction::ToSelect));
        s.show(Some(Screen::Gameover));
        assert_eq!(s.nav(false, false, true), None, "敗北の画面は出てすぐには立ち上がらない");
        s.advance(0.61);
        assert_eq!(s.nav(false, false, true), Some(UiAction::Continue));
        assert_eq!(s.nav(false, true, true), Some(UiAction::ToTitle));
        // 画面が無いとき（戦いの間）は何も返さない
        s.show(None);
        assert_eq!(s.nav(false, true, true), None);
    }
    /// 敗北の画面のボタンは、出てから 0.6 秒は押しても効かない（0.6 秒たてば効く）
    #[test]
    fn gameover_click_waits() {
        let ctx = test_ctx();
        pass(&ctx, 0.0, vec![], |_| {});
        let mut s = Screens::new(&ctx);
        let mut t = 1.0;
        for _ in 0..3 { pass(&ctx, t, vec![], |c| s.ui(c)); t += 0.016; }
        s.show(Some(Screen::Gameover));
        // 札は窓の真ん中。上から eyebrow・無念…・一言・立ち上がる の順で、立ち上がるは真ん中の少し下
        let (x, y) = (640.0, 452.0);
        let click = |s: &mut Screens, t: &mut f64| {
            let mut got = vec![];
            for ev in [vec![egui::Event::PointerMoved(pos2(x, y))], press(x, y, true), press(x, y, false), vec![]] { got.extend(pass(&ctx, *t, ev, |c| s.ui(c))); *t += 0.016; }
            got
        };
        assert!(click(&mut s, &mut t).is_empty(), "出てすぐは押せない");
        t += 0.7;
        assert_eq!(click(&mut s, &mut t), vec![UiAction::Continue]);
    }
    /// 自己ベストの祝いの紙吹雪: 自己ベストでないときは出ない。自己ベストは左右の角から各 44 枚、S のもう 1 回で 48 枚。
    /// 動きを減らす設定では 3 分の 1。出す角は結果の中身の箱の左右の下（画面の中）。打ち切ると消えて、得点は本当の値の字に戻る
    #[test]
    fn cheer_confetti_by_level() {
        let ctx = test_ctx();
        pass(&ctx, 0.0, vec![], |_| {});
        let mut s = Screens::new(&ctx);
        let row = |a: &str, b: &str, c: &str| ResultRow { label: a.into(), value: b.into(), best: c.into() };
        s.set_result(ResultData { eyebrow: "VICTORY".into(), title: "勝鬨".into(), rank: "S".into(), rows: vec![row("撃破", "1204", "NEW"), row("得点", "284,500", "NEW")], honors: vec![], main: Some(1) });
        s.show(Some(Screen::Result));
        let mut t = 1.0;
        for _ in 0..3 { pass(&ctx, t, vec![], |c| s.ui(c)); t += 0.016; }
        let card = s.result_card.get().expect("結果の中身の箱");
        assert!(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0)).contains_rect(card) && card.width() > 200.0 && card.height() > 300.0, "{card:?}");
        // 自己ベストでない
        s.cheer_begin(false);
        s.cheer_count(120_000);
        assert_eq!(s.cheer_state(), (Some(120_000), false, 0));
        s.cheer_done(false);
        assert_eq!(s.cheer_state(), (Some(120_000), true, 0));
        // 自己ベスト → S のもう 1 回
        s.cheer_begin(false);
        s.cheer_done(true);
        assert_eq!(s.cheer_state().2, CONFETTI_SIDE * 2);
        s.cheer_encore();
        assert_eq!(s.cheer_state().2, CONFETTI_SIDE * 2 + CONFETTI_TOP);
        // 描いても落ちない（紙吹雪は中身の前・ボタンの後ろ）。ボタンは今までどおり押した物を返す
        pass(&ctx, t, vec![], |c| s.ui(c)); t += 0.016;
        // 打ち切り
        s.cheer_clear();
        assert_eq!(s.cheer_state(), (None, false, 0));
        // 動きを減らす
        s.cheer_begin(true);
        s.cheer_done(true);
        assert_eq!(s.cheer_state().2, CONFETTI_SIDE.div_ceil(3) * 2);
        s.cheer_encore();
        assert_eq!(s.cheer_state().2, CONFETTI_SIDE.div_ceil(3) * 2 + CONFETTI_TOP.div_ceil(3));
        // 5 秒たてば紙吹雪は残らない
        for _ in 0..400 { pass(&ctx, t, vec![], |c| s.ui(c)); t += 0.016; }
        assert_eq!(s.cheer_state().2, 0);
    }
    /// 結果の画面の流れ（共通部品 game_ui::cheer）への魚群繚乱の渡し方: 格の判（STAMP_S）を待つ間は 0 のままで刻まず、
    /// そこから 0.8 秒で数え終わる。S はその 0.35 秒後にもう 1 回
    #[test]
    fn result_flow_waits_for_the_stamp() {
        use game_ui::cheer::{cheer_of, Cheer, CheerEvent, ResultShow, COUNT_S, ENCORE_S};
        use sim::cheer::{top_rank, STAMP_S};
        assert_eq!((cheer_of(true, top_rank("S")), cheer_of(true, top_rank("A")), cheer_of(false, top_rank("S"))), (Cheer::BestTop, Cheer::Best, Cheer::None));
        let mut s = ResultShow::new(284_500, cheer_of(true, top_rank("S")), false).wait(STAMP_S);
        let (dt, mut t, mut ev) = (1.0 / 60.0, 0.0, vec![]);
        while t < 4.0 {
            t += dt;
            for e in s.step(dt) { ev.push((t, e)); }
            if t < STAMP_S - 0.02 { assert!(ev.is_empty() && s.value() == 0, "判の間は数えない"); }
        }
        let at = |want: CheerEvent| ev.iter().find(|(_, e)| *e == want).map(|(t, _)| *t).unwrap();
        assert!((at(CheerEvent::Done(Cheer::BestTop)) - (STAMP_S + COUNT_S)).abs() < 0.03);
        assert!((at(CheerEvent::Encore) - (STAMP_S + COUNT_S + ENCORE_S)).abs() < 0.03);
        assert_eq!(ev.iter().filter(|(_, e)| matches!(e, CheerEvent::Tick(_))).count(), 11);
        assert_eq!(s.value(), 284_500);
    }
    #[test]
    fn bezier_ends() { assert!(ease(0.0).abs() < 1e-5 && (ease(1.0) - 1.0).abs() < 1e-5); assert!(snap(0.5) > 1.0); }
    #[test]
    fn commas() { assert_eq!(fmt_num(1234567), "1,234,567"); assert_eq!(fmt_num(12), "12"); assert_eq!(fmt_num(-1000), "-1,000"); }
    #[test]
    fn tween_restarts_from_current() {
        let mut t = Tween::new(0.0, 1.0, 0.0, |x| x);
        t.set(1.0, 10.0);
        assert!((t.get(10.5) - 0.5).abs() < 1e-4);
        t.set(0.0, 10.5);
        assert!((t.get(10.5) - 0.5).abs() < 1e-4 && t.get(11.5).abs() < 1e-4);
    }
    #[test]
    fn xf_about_keeps_origin() {
        let x = Xf::about(10.0, 20.0, Xf::rot(37.0).then(Xf::scale(3.0, 2.0)));
        let p = x.ap(pos2(10.0, 20.0));
        assert!((p.x - 10.0).abs() < 1e-3 && (p.y - 20.0).abs() < 1e-3);
    }
}

// ---- 確かめ用: 画面の文字の写し（今の版は DOM から同じ形を作る。rust/compare/scenes.py の UI_TS） ----
#[cfg(target_arch = "wasm32")]
impl Screens {
    pub fn dump(&self) -> serde_json::Value {
        use serde_json::json;
        let rr = |r: Rect| [r.left(), r.top(), r.right(), r.bottom()];
        let btns = self.result_btns.get();
        let name = |s: Screen| match s { Screen::Loading => "loading", Screen::Title => "title", Screen::Select => "select", Screen::Result => "result", Screen::Gameover => "gameover", Screen::Pause => "pause" };
        json!({
            "fade": SCREENS.iter().enumerate().map(|(i, x)| (name(*x), self.fade[i].get(self.now))).collect::<std::collections::HashMap<_, _>>(), "show": self.shown.map(name).unwrap_or(""), "loadText": self.load_text, "loadFill": self.load_fill.target(), "titleBest": self.title_best, "sound": if self.muted { "音: オフ" } else { "音: オン" },
            "heroes": self.heroes.iter().map(|h| h.name.clone()).collect::<Vec<_>>(), "heroOn": self.hero_on, "selName": self.sel_name, "titleMake": rr(self.title_make.get()),
            "wpName": self.weapon.as_ref().map(|w| w.name.clone()), "wpPath": self.weapon.as_ref().and_then(|w| w.path.as_ref().map(|p| format!("{}{}", p.name, p.desc))).unwrap_or_default(),
            "resEyebrow": self.result.eyebrow, "resTitle": self.result.title, "resRank": self.result.rank,
            "resRows": self.result.rows.iter().map(|r| [r.label.clone(), r.value.clone(), r.best.clone()]).collect::<Vec<_>>(),
            "honors": self.result.honors.iter().map(|h| json!({ "name": h.name, "on": h.on, "fresh": h.fresh })).collect::<Vec<_>>(),
            "goHint": controls::defeat_hint(self.device), "moves": self.moves.iter().map(|m| [controls::input_text(&m.input, self.device), m.name.to_string(), m.desc.to_string()]).collect::<Vec<_>>(),
            "device": match self.device { Device::Key => "key", Device::Pad => "pad", Device::Touch => "touch" }, "ring": self.ring, "focus": self.focus, "menu": self.menu().len(), "loadFailed": self.load_failed,
            "safe": self.safe.to_array(), "size": [self.size.0, self.size.1], "touchShown": self.touch() && self.touch_on && self.shown.is_none(),
            "touchRects": TOUCH_BTNS.iter().enumerate().map(|(i, b)| { let (l, h) = (touch_button_rect(*b, self.size.0, self.size.1, self.safe), touch_hit_rect(*b, self.size.0, self.size.1, self.safe)); (b.name(), json!({ "look": [l.left(), l.top(), l.right(), l.bottom()], "hit": [h.left(), h.top(), h.right(), h.bottom()], "on": self.touch_view.on[i] })) }).collect::<std::collections::HashMap<_, _>>(),
            "stick": self.touch_view.stick.map(|(x, y)| [x, y]), "knob": [self.touch_view.knob.0, self.touch_view.knob.1],
            "cheer": { "count": self.count, "done": self.cheer_at.is_some(), "best": self.cheer_best, "pieces": self.confetti.len(), "calm": self.calm },
            "resultRects": { "card": self.result_card.get().map(rr), "again": rr(btns[0]), "select": rr(btns[1]) },
            "letterbox": self.letter.target() > 0.5, "cutin": self.cutin_at.is_some(), "ciName": self.ci_name, "flash": self.flash, "touchOn": self.touch_on,
        })
    }
}
