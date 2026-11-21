// 画面の流れ: 読み込み → タイトル（裏で CPU のレース）→ 出走登録 → 発走前 → レース ⇄ 一時停止 → ゴール → 結果 → もう一度。src/app.ts の写し。
// 確かめ用の window.__game は game_api.rs。
use crate::audio::{Audio, Scene};
use crate::character_delivery::{GUEST_ID, guest_actor, guest_hero, hero, heroes as list_heroes, open_browser_catalog, studio_zip};
use crate::controls::{self, Controls, Keys, Rebindable, key_name};
use crate::ui::{Act, CareerCard, CourseBtn, HeroBtn, Pill, RecBadge, RecCourse, RecHall, RecordsData, ResRow, ResultData, Scr, Screens, SelectData, Tex};
use game_ui::dom_input::DomInput;
use game_ui::raw::RawLayer;
use crate::save::{Save, career_to_json, hall_to_json, row_of_json, row_to_json};
use crate::view::assets::Props;
use crate::view::course::{Assets, Course, create_course, load_assets};
use crate::hud::PopKind;
use crate::view::fx::{Emit, Fx};
use crate::view::items::{ItemView, stat_color};
use crate::view::runner::{Body, FACE_TEX, GATE_COLOR, GATE_TEXT, Gesture, HERO_HEIGHT, HeroInfo, Mood, RunnerView, face_tag_texture, to_hero_list};
use crate::view::sky::Sky;
use crate::view::theme::{Theme, theme};
use bytemuck::{Pod, Zeroable};
use character_actor::delivery::Catalog;
use character_view::ActorStyle;
use glam::{DVec3, Quat, Vec3};
use island_kit::app::input::{Device, Input as KitInput};
use island_kit::app::{dom, raf};
use island_kit::geometry::ring;
use island_kit::scene::Rot;
use island_kit::{Camera, Color, CustomShader, FogExp2, Kit, MatId, MatKind, Material, NodeId, RenderOpts, Target, TexId};
use js_sys::Math::random;
use serde_json::{Value, json};
use sim::ai::Difficulty;
use sim::cup::{CupRow, POINTS, add_race, empty_cup, standings};
use game_ui::cheer::{Cheer, CheerEvent, ResultShow, cheer_of};
use game_ui::menu::{Kind, Seen, Used, result_accepts, step};
use game_ui::page::safe_area;
use sim::career::{Career, HallEntry, RACES, hall_insert};
use sim::items::{ITEM, PickupKind, Stat, Weapon};
use sim::race::{Input, Race, RaceEvent, RaceOptions, Runner, TUNE};
use sim::result::{BADGES, badge, earned_badges, margin_text, medal_of, medal_times, reflect, time_text, to_fixed};
use sim::track::{CourseDef, ObstacleKind, P3, build_track, courses};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::rc::{Rc, Weak};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};


pub type Shared = Rc<RefCell<App>>;

/// 比べる道具と同じ形の出来事の名前（今の版は PATCH_TS が同じ文字を作る）
fn event_name(e: &RaceEvent, live: bool) -> String {
    use RaceEvent as E;
    let l = if live { "L" } else { "-" };
    match e {
        E::Count { n } => format!("{l} count n{n}"), E::Go => format!("{l} go"), E::Jump { i } => format!("{l} jump i{i}"), E::Land { i, clean } => format!("{l} land i{i} {clean}"),
        E::Clear { i, perfect, kind } => format!("{l} clear i{i} {perfect} {}", kind.name()), E::Stumble { i, kind } => format!("{l} stumble i{i} {}", kind.name()), E::Splash { i } => format!("{l} splash i{i}"),
        E::Bump { i, j, side } => format!("{l} bump i{i} j{j} {side}"), E::Rail { i } => format!("{l} rail i{i}"), E::DashStart { i } => format!("{l} dashStart i{i}"), E::DashEnd { i } => format!("{l} dashEnd i{i}"),
        E::Low { i } => format!("{l} low i{i}"), E::Empty { i } => format!("{l} empty i{i}"), E::Recovered { i } => format!("{l} recovered i{i}"), E::SlipReady { i } => format!("{l} slipReady i{i}"), E::Sling { i } => format!("{l} sling i{i}"),
        E::Pass { i, j } => format!("{l} pass i{i} j{j}"), E::Lap { lap } => format!("{l} lap {lap}"), E::FinalStraight => format!("{l} finalStraight"), E::Lead { i } => format!("{l} lead i{i}"), E::Finish { i, place, .. } => format!("{l} finish i{i} p{place}"),
        E::Pick { i, kind } => format!("{l} pick i{i} {}", kind.id()), E::ChargeStart { i } => format!("{l} chargeStart i{i}"), E::ChargeFull { i } => format!("{l} chargeFull i{i}"),
        E::Swing { i, weapon, charged } => format!("{l} swing i{i} {} {charged}", weapon.id()), E::Hit { by, i, weapon, charged } => format!("{l} hit by{by} i{i} {} {charged}", weapon.id()),
        E::Dodge { i } => format!("{l} dodge i{i}"), E::WeaponGone { i } => format!("{l} weaponGone i{i}"),
        E::Launch { i, by } => format!("{l} launch i{i} by{by}"), E::BoltDone { owner, .. } => format!("{l} boltDone i{owner}"),
    }
}

fn action_name(k: character_actor::actor::ActionKind) -> String { format!("{k:?}").to_lowercase() }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode { Loading, Title, Select, Intro, Race, Finish, Photo, Result, Paused }
impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Loading => "loading", Mode::Title => "title", Mode::Select => "select", Mode::Intro => "intro", Mode::Race => "race",
            Mode::Finish => "finish", Mode::Photo => "photo", Mode::Result => "result", Mode::Paused => "paused"
        }
    }
}

// 仕上げの値（final.wgsl の FinalU）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct FinalU {
    pub res: [f32; 2], pub mask_px: [f32; 2], pub ripple: [f32; 4], pub outline: [f32; 4], pub outline_dark: [f32; 4], pub blur_at: [f32; 4],
    pub center: [f32; 2], pub zoom: f32, pub speed: f32,
    pub time: f32, pub vignette: f32, pub warm: f32, pub flash: f32,
    pub desat: f32, pub cool: f32, pub aberration: f32, pub gold: f32,
    pub red: f32, pub blur: f32, pub dim: f32, pub _p: f32,
}

/// 出走前の長さ（そのうち最初の INTRO_FLY 秒は、島の外の空からゲートへ降りてくる）
const INTRO_S: f64 = 3.8;
const INTRO_FLY: f64 = 1.2;
const PHOTO_DT: f64 = 0.06;
/// ためた一撃が当たったとき、横から見せる秒
const HIT_CAM_T: f64 = 0.9;
/// ハンマーで飛ばしたとき・飛ばされたとき、横から見せる秒（飛んでいる間をおおよそ見せる）
const LAUNCH_CAM_T: f64 = 2.2;
const GHOST_DT: f64 = 0.5;
const KEY_ROWS: [(Rebindable, &str); 5] = [(Rebindable::Left, "内へ"), (Rebindable::Right, "外へ"), (Rebindable::Dash, "ダッシュ"), (Rebindable::Jump, "ジャンプ"), (Rebindable::Attack, "攻撃")];
const DIFFS: [(Difficulty, &str); 3] = [(Difficulty::Easy, "やさしい"), (Difficulty::Normal, "ふつう"), (Difficulty::Hard, "むずかしい")];

fn f3(p: DVec3) -> Vec3 { p.as_vec3() }
/// 賞金の字（万円。3 桁ごとに区切る）
fn money(n: i32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (k, ch) in s.chars().enumerate() { if k > 0 && (s.len() - k) % 3 == 0 { out.push(','); } out.push(ch); }
    format!("{out} 万")
}
/// 読み込みの帯（今の版は width を整数の % で書く）
fn pct(k: f64) -> f32 { sim_math::round(k * 100.0) as f32 / 100.0 }
fn lerp3(a: &mut DVec3, b: DVec3, k: f64) { *a += (b - *a) * k; }

pub struct CupState { pub index: usize, pub rows: Vec<CupRow> }
pub struct PhotoState { pub win: usize, pub dt: f64 }
pub struct Perf { pub frames: f64, pub slow: f64, pub tick_ms: f64, pub last: f64, pub worst: f64 }

pub struct App {
    me: Weak<RefCell<App>>,
    pub kit: Kit,
    pub input: KitInput,
    pub audio: Audio,
    pub keys: Keys,
    /// 画面の文字・枠・ボタン（ui.rs。HUD と一時停止の札もここ）
    pub ui: Screens,
    layer: RawLayer,
    /// canvas の指・マウス（画面のボタンへ）と、レース中の指のスティック・ボタン（game-ui の dom_input・touch_pad）
    pointer: DomInput,
    canvas: web_sys::HtmlCanvasElement,
    cursor_hand: bool,
    /// このコマの指・マウスの入力（描くときに egui へ渡す）
    ui_raw: Option<egui::RawInput>,
    /// キャラの選びの絵（heroes の番号）
    hero_tex: Rc<RefCell<Vec<Option<Tex>>>>,
    pub fx: Fx,
    /// 走路のマークと武器の見た目
    items: ItemView,
    /// 当てた・当てられたときに一瞬止まる残りの秒
    hitstop: f64,
    /// ためた一撃が当たったとき、横から見せる残りの秒と、当てた・当てられた走者
    hit_cam: f64, hit_cam_len: f64, hit_pair: (usize, usize),
    /// ボーガンの矢の字を最後に出した時刻
    last_bolt_pop: f64,
    sky: Sky,
    pub save: Save,
    hemi: NodeId,
    key: NodeId,
    sun_dir: DVec3,
    courses_defs: Vec<CourseDef>,
    courses: HashMap<String, Course>,
    props: Props,
    assets: Rc<RefCell<Assets>>,
    /// TS の course（今見えている物）
    cur_course: String,
    pub course_id: String,
    catalog: Rc<Catalog>,
    pub heroes: Vec<HeroInfo>,
    pub views: HashMap<String, RunnerView>,
    loading: HashMap<String, js_sys::Promise>,
    pub hero_i: usize,
    /// 枠番 → キャラ（heroes の番号。プレイヤー以外は毎レース並びを変える）
    pub lineup: Vec<usize>,
    ring: NodeId, ring_mat: MatId, slip_ring: NodeId, slip_mat: MatId, me_mark: NodeId,
    ghost_ring: NodeId, ghost_mat: MatId, ghost_tag: NodeId,
    photo_rt: TexId, live_rt: TexId, live_frame: u32,
    // 状態
    pub debug_policy: Option<js_sys::Function>,
    pub debug_input: Option<Input>,
    pub debug: js_sys::Object,
    pub mode: Mode,
    before_pause: Mode,
    pub race_seed: i64,
    pub race: Race,
    pub cup: Option<CupState>,
    is_cup_mode: bool,
    /// 育成: 走っている途中経過（育成のレースの間だけ Some）・出走登録で育成を選んでいるか・「最初から育て直す」を 1 回押したか
    pub career: Option<Career>,
    is_career_mode: bool,
    reset_armed: bool,
    finish_timer: f64, intro_t: f64, first_race: bool,
    go_flash: f64, sling_fx: f64, burst_t: f64,
    cam_pos: DVec3, cam_look: DVec3,
    /// lookAt の前の向き（three の vector.project は前の lookAt の向きの逆行列を読む）
    cam_prev_q: Quat,
    cam_s: f64, cam_d: f64, cam_y: f64, look_d: f64, cam_up: f64, pull: f64,
    trauma: f64, cheer_target: f64, cheer_now: f64,
    tips_shown: HashSet<String>,
    wipe_busy: bool,
    last_say: f64,
    pub photo: Option<PhotoState>,
    photo_t: f64,
    called_close: bool, called_half: bool, split_done: bool, lap_split: f64,
    ghost_run: Vec<f64>, ghost_best: Option<Vec<f64>>, ghost_clock: f64,
    shot_t: f64, shot: usize,
    last_greet: f64, select_snap: bool, picked_at: f64, cheer_clock: f64,
    board_t: f64,
    pick_seq: u32, loading_hero: bool, start_seq: u32,
    last: f64, pub last_tick: f64, pub t_all: f64,
    pub perf: Perf,
    /// 最後に触った機器（操作の説明の表記・指の部品・輪）。キー・パッド・画面を押した指かマウス・レース中の指のうち最後に使った物
    seen: Seen,
    /// 結果の画面が出てからの秒（0.6 秒は決定を受けない）
    result_t: f64,
    /// 結果の画面の自己ベストの祝いの流れ（タイムの数え上げ → 祝い）と、端末の「動きを減らす」設定
    show: ResultShow,
    calm: bool,
    /// 確かめ用: 祝いで鳴らした合図（__game.cheer.log）
    pub cheer_log: Vec<&'static str>,
    ui_size: (f32, f32),
    pub final_u: FinalU,
    pub started: bool,
    /// 確かめ用の寄り（位置, 見る所）。そのコマのカメラの最後の lookAt のあとで置き直す（比べる道具の CAM_TS と同じ）
    pub cam_at: Option<(DVec3, DVec3)>,
    /// 確かめ用: 起きた出来事の列（__game.events で読むと空になる）
    pub ev_log: Vec<String>,
    /// 確かめ用: 押された画面のボタン（__game.acts で読むと空になる）
    pub acts_log: Vec<String>,
    /// 確かめ用: 前に読んでからの操作（__game.ctl()。指のスティックとボタンが効いたか）
    pub ctl_seen: Controls,
}

/// 借りられるときに実行する（DOM の出来事は刻みの外で来るので、ふつうはすぐ）
pub fn with(me: &Weak<RefCell<App>>, f: impl FnOnce(&mut App) + 'static) {
    let Some(s) = me.upgrade() else { return };
    match s.try_borrow_mut() {
        Ok(mut a) => f(&mut a),
        Err(_) => { let w = me.clone(); raf::after_ms(0, move || with(&w, f)); }
    }
}

/// 共通部品の表記の種類（game_ui::menu::Kind）と、island-kit の機器の読み替え（ここ 1 か所）
fn device_of(d: Kind) -> Device { match d { Kind::Keys => Device::Keys, Kind::Pad => Device::Pad, Kind::Touch => Device::Touch } }
fn dev_of(d: Device) -> Kind { match d { Device::Keys => Kind::Keys, Device::Pad => Kind::Pad, Device::Touch => Kind::Touch } }

impl App {
    /// 最後に触った機器
    pub fn device(&self) -> Device { device_of(self.seen.kind) }
    /// 機器を使った: 操作の説明の表記・「キー設定」の入口・輪・指の部品を合わせる
    fn used(&mut self, u: Used) {
        let was = self.seen.kind;
        self.seen.used(u);
        if self.seen.kind != was {
            let (d, l) = (self.device(), self.pause_labels());
            self.ui.pause.set_device(d, &l, &self.keys);
        }
        self.ui.ring = self.seen.ring;
        self.ui.touch = self.seen.kind == Kind::Touch;
    }
    fn th(&self) -> Theme { theme(&self.course_id) }
    fn def_of(&self, id: &str) -> &CourseDef { self.courses_defs.iter().find(|c| c.id == id).unwrap() }
    fn course(&mut self) -> &mut Course { self.courses.get_mut(&self.cur_course).unwrap() }
    pub fn debug_get(&self, k: &str) -> JsValue { js_sys::Reflect::get(&self.debug, &k.into()).unwrap_or(JsValue::UNDEFINED) }

    // ---- 光と空気 ----
    fn apply_theme(&mut self, id: &str) {
        let t = theme(id);
        self.sky.set_theme(&t);
        self.kit.fog = Some(FogExp2 { color: t.fog, density: t.fog_density });
        self.sky.set_fog(t.fog);
        if let Some(h) = self.kit.scene.hemi_light_mut(self.hemi) { h.sky = t.hemi_sky; h.ground = t.hemi_ground; h.intensity = t.hemi_i; }
        if let Some(k) = self.kit.scene.dir_light_mut(self.key) { k.color = t.key; k.intensity = t.key_i; }
        let mut d = DVec3::new(t.sun_dir[0] as f64, t.sun_dir[1] as f64, t.sun_dir[2] as f64).normalize();
        if d.y < 0.28 { d.y = 0.28; d = d.normalize(); }
        self.sun_dir = d;
        self.kit.shared.rim_color = t.rim_color;
        self.final_u.warm = t.warm;
    }

    /// コース（使うときに作り、作ったものは残す）。GLB は先に読んでおく（ensure_assets）
    fn use_course_now(&mut self, id: &str) {
        if !self.courses.contains_key(id) {
            let def = self.def_of(id).clone();
            let assets = self.assets.clone();
            let tr = build_track(&def);
            let t = theme(id);
            let c = create_course(&mut self.kit, &mut self.props, &tr, &t, &assets.borrow());
            c.set_low(&mut self.kit, self.save.s.low);
            self.courses.insert(id.to_string(), c);
        }
        let ids: Vec<(NodeId, bool)> = self.courses.iter().map(|(k, v)| (v.group, k == id)).collect();
        for (g, on) in ids { self.kit.scene[g].visible = on; }
        self.apply_theme(id);
        self.cur_course = id.to_string();
    }

    // ---- 走者 ----
    fn load_view(&mut self, i: usize) -> js_sys::Promise {
        let id = self.heroes[i].id.clone();
        if let Some(p) = self.loading.get(&id) { return p.clone(); }
        let (cat, me, id2, info) = (self.catalog.clone(), self.me.clone(), id.clone(), self.heroes[i].clone());
        let p = wasm_bindgen_futures::future_to_promise(async move {
            let err = |e: String| JsValue::from_str(&e);
            // 人の形は部品（GPU は触らない）、魚は形を先に読む
            enum Got { Actor(character_actor::actor::CharacterActor), Fish(&'static crate::view::fish::FishDef, std::sync::Arc<crate::view::fish::FishShape>), Squid(&'static crate::view::runner::SquidDef, std::sync::Arc<crate::view::squid::SquidData>) }
            let got = match &info.body {
                Body::Hero => Got::Actor(hero(&cat, &id2).await.map_err(err)?),
                Body::Guest(asset) => Got::Actor(guest_actor(asset, info.scale).map_err(err)?),
                Body::Fish(def) => {
                    let g = island_kit::glb::load_glb(&crate::view::fish::fish_url(def.file)).await.map_err(err)?;
                    Got::Fish(def, std::sync::Arc::new(crate::view::fish::shape_of(&g).map_err(err)?))
                }
                Body::Squid(def) => Got::Squid(def, crate::view::squid::load_squid(def.id).await.map_err(err)?),
            };
            let s = me.upgrade().ok_or(JsValue::NULL)?;
            let mut a = s.borrow_mut();
            let mut v = RunnerView::new(&mut a.kit, info.scale);
            match got {
                Got::Actor(actor) => { a.kit.set_actor(v.actor, actor); v.apply_look(&mut a.kit); }
                Got::Fish(def, shape) => v.set_fish(&mut a.kit, def, &shape),
                Got::Squid(def, data) => v.set_squid(&mut a.kit, def, &data),
            }
            a.kit.scene[v.holder].visible = false;
            a.views.insert(id2, v);
            Ok(JsValue::UNDEFINED)
        });
        self.loading.insert(id, p.clone());
        p
    }

    fn pick_lineup(&self, player_gate: Option<usize>, seed: i64) -> Vec<usize> {
        let mut others: Vec<usize> = (0..self.heroes.len()).filter(|i| player_gate.is_none() || *i != self.hero_i).collect();
        let mut a = (seed as u64) & 0xffff_ffff;
        for i in (1..others.len()).rev() {
            a = (a * 1664525 + 1013904223) & 0xffff_ffff;
            others.swap(i, (a % (i as u64 + 1)) as usize);
        }
        let mut q = 0;
        (0..8).map(|g| if Some(g) == player_gate { self.hero_i } else { let h = others[q % others.len()]; q += 1; h }).collect()
    }

    /// 札: 番号をすぐ出し、顔の絵が読めたら替える（読めなければ番号のまま）
    fn set_gate_tag(&mut self, gate: usize, big: bool, hero_i: usize) {
        let info = self.heroes[hero_i].clone();
        let me = self.me.clone();
        let Some(v) = self.views.get_mut(&info.id) else { return };
        v.set_gate(&mut self.kit, gate, big);
        v.tag_seq += 1;
        let (seq, tag_mat, thumb, id) = (v.tag_seq, v.tag_mat, info.thumb.clone(), info.id.clone());
        let cached = FACE_TEX.with(|m| m.borrow().get(&(thumb.clone(), gate)).copied());
        if let Some(t) = cached {
            if let Some(t) = t { self.kit.mat_mut(tag_mat).map = Some(t); }
            return;
        }
        spawn_local(async move {
            let img = web_sys::HtmlImageElement::new().unwrap();
            img.set_cross_origin(Some("anonymous"));
            img.set_src(&thumb);
            let ok = JsFuture::from(img.decode()).await.is_ok();
            let Some(s) = me.upgrade() else { return };
            let mut a = s.borrow_mut();
            let t = if ok { Some(face_tag_texture(&mut a.kit, &img, gate)) } else { None };
            FACE_TEX.with(|m| { m.borrow_mut().insert((thumb, gate), t); });
            if let Some(t) = t && a.views.get(&id).is_some_and(|v| v.tag_seq == seq) { a.kit.mat_mut(tag_mat).map = Some(t); }
        });
    }

    // ---- 一時停止 ----
    fn pause_labels(&self) -> Vec<String> {
        let s = &self.save.s;
        let onoff = |b: bool| if b { "オン" } else { "オフ" };
        vec![
            "つづける".into(), if self.career.is_some() { "やり直しなし（育成）".into() } else { "このレースをやり直す".into() },
            format!("自動ジャンプ: {}", onoff(s.auto_jump)), format!("ジャンプの受付を広く: {}", onoff(s.wide_jump)),
            format!("画質: {}", if s.low { "軽い" } else { "標準" }), "キー設定".into(),
            format!("音: {}", if self.audio.muted() { "オフ" } else { "オン" }), "タイトルへ".into(),
        ]
    }
    fn pause_sync(&mut self) { let l = self.pause_labels(); self.ui.pause.sync(&l); }
    pub fn pause_run(&mut self, i: usize) {
        match i {
            0 => self.set_paused(false),
            // 育成はやり直せない（負けたら引退）
            1 => { if self.career.is_some() { self.audio.bump(true); } else { self.set_paused(false); self.start_race(true); } }
            2 => { self.save.s.auto_jump = !self.save.s.auto_jump; self.save.write(); }
            3 => { self.save.s.wide_jump = !self.save.s.wide_jump; self.save.write(); }
            4 => { self.save.s.low = !self.save.s.low; self.save.write(); self.apply_quality(); }
            5 => { self.ui.pause.keys_shown = !self.ui.pause.keys_shown; self.render_keys(); }
            6 => { self.audio.toggle_mute(); }
            _ => { self.set_paused(false); self.to_title(); }
        }
        self.pause_sync();
    }
    pub fn set_paused(&mut self, on: bool) {
        if on && (self.mode == Mode::Race || self.mode == Mode::Intro) {
            self.before_pause = self.mode;
            self.mode = Mode::Paused;
            let l = self.pause_labels();
            self.ui.pause.show(true, &l, &self.keys);
            self.audio.duck(true);
            self.input.cancel_capture();
        } else if !on && self.mode == Mode::Paused {
            self.mode = self.before_pause;
            let l = self.pause_labels();
            self.ui.pause.show(false, &l, &self.keys);
            self.audio.duck(false);
            self.ui.pause.keys_shown = false;
            self.input.cancel_capture();
        }
        self.ui.set_paused(self.mode == Mode::Paused);
    }
    /// キー設定の札（#pause-keys の中身を作り直す。押すのを待つ印も消える）
    fn render_keys(&mut self) {
        self.ui.pause.key_labels = KEY_ROWS.iter().map(|(id, name)| {
            let kn = key_name(&self.keys.key_of(*id));
            (format!("{name}: {}", if kn.is_empty() { "未設定".to_string() } else { kn }), false)
        }).collect();
    }
    /// キー設定の札を押した: 次に押したキーを割り当てる
    fn key_wait(&mut self, j: usize) {
        let Some((id, name)) = KEY_ROWS.get(j).copied() else { return };
        if let Some(l) = self.ui.pause.key_labels.get_mut(j) { *l = (format!("{name}: キーを押す"), true); }
        let me = self.me.clone();
        self.input.capture_next_key(move |code| {
            with(&me, move |a| {
                a.keys.set_key(id, &code);
                a.render_keys();
                let l = a.pause_labels();
                let d = a.device();
                a.ui.pause.set_device(d, &l, &a.keys);
            });
        });
    }
    fn apply_quality(&mut self) {
        let low = self.save.s.low;
        if let Some(k) = self.kit.scene.dir_light_mut(self.key) { k.shadow.map_size = if low { 1024 } else { 2048 }; }
        self.kit.bloom_enabled = !low;
        self.kit.set_max_pixel(if low { 1.0 } else { 2.0 });
        for c in self.courses.values() { c.set_low(&mut self.kit, low); }
    }

    // ---- レースを組む ----
    fn setup_race(&mut self, player_gate: Option<usize>, seed: i64) {
        let def = self.def_of(&self.course_id).clone();
        let s = &self.save.s;
        let mut difficulty = if player_gate.is_none() { Difficulty::Normal } else if self.first_race { Difficulty::Easy } else { s.difficulty };
        // 育成: 相手の難しさと能力はレースで決まり、自分は育てた能力で走る
        let mut levels = None;
        if let (Some(c), Some(g)) = (&self.career, player_gate) { difficulty = c.race().difficulty; levels = Some(c.levels_for(g)); }
        self.race = Race::new(RaceOptions { course: &def, seed: seed as u32, difficulty, player_gate, auto_jump: s.auto_jump, wide_jump: s.wide_jump, levels, humans: Vec::new() });
        self.hitstop = 0.0;
        self.hit_cam = 0.0;
        self.ui.hud.set_track(&self.race);
        self.ui.hud.reset();
        self.fx.clear();
        // showViews
        for v in self.views.values() { self.kit.scene[v.holder].visible = false; }
        self.lineup = self.pick_lineup(player_gate, seed);
        self.reset_views_for_lineup();
        let kit = &mut self.kit;
        let c = self.courses.get_mut(&self.cur_course).unwrap();
        c.open_gate(kit, 0.0);
        kit.scene[c.podium].visible = false;
        kit.scene[c.pedestal].visible = false;
        c.set_gate_visible(kit, true);
        self.cam_s = -8.0;
        self.cam_up = 4.8;
        self.cam_d = self.race.runners[player_gate.unwrap_or(3)].d;
        self.cam_y = 0.0;
        self.look_d = self.cam_d;
        self.set_finishing(false);
        self.called_close = false;
        self.called_half = false;
        self.split_done = false;
        self.lap_split = 0.0;
        self.ghost_run.clear();
        self.ghost_clock = 0.0;
        self.kit.scene[self.ghost_ring].visible = false;
        self.kit.scene[self.ghost_tag].visible = false;
        if player_gate.is_some() {
            let bk = self.save.best_key(&self.heroes[self.hero_i].id, &self.course_id);
            // 育成は能力が違うので、自己ベストの影と比べない
            self.ghost_best = if self.career.is_some() { None } else { self.save.ghost(&bk) };
        }
        self.tips_shown.clear();
        self.photo = None;
        self.cheer_target = 0.2;
        self.cheer_now = 0.2;
        self.set_player_layers(player_gate);
    }
    /// 自機だけを層 1 に（縁取り・物陰の影絵）
    fn set_player_layers(&mut self, player_gate: Option<usize>) {
        let me = player_gate.and_then(|g| self.lineup.get(g).map(|h| self.heroes[*h].id.clone()));
        // 撮影用（?video）は全員に縁取り（明るい背景とエフェクトの中でも形が読める）
        let all = self.ui.hud.video;
        for (id, v) in &self.views { v.set_outline(&mut self.kit, all || me.as_deref() == Some(id.as_str())); }
    }
    fn reset_views_for_lineup(&mut self) {
        for i in 0..8 {
            let (gate, is_player, hi) = (self.race.runners[i].gate, self.race.runners[i].is_player, self.lineup[i]);
            let id = self.heroes[hi].id.clone();
            if let Some(v) = self.views.get_mut(&id) { v.reset(&mut self.kit); self.set_gate_tag(gate, is_player, hi); }
        }
    }

    pub fn to_title(&mut self) {
        self.leave_result();
        self.mode = Mode::Title;
        self.cup = None;
        self.is_cup_mode = false;
        self.career = None;
        self.is_career_mode = false;
        self.ui.show(Some(Scr::Title));
        self.ui.set_racing(false);
        self.audio.play(Scene::Title);
        self.race_seed += 1;
        self.setup_race(None, self.race_seed);
        let _ = self.load_lineup(None);
        self.race.t = 0.0;
        self.race.started = true;
        let kit = &mut self.kit;
        self.courses.get_mut(&self.cur_course).unwrap().open_gate(kit, 1.0);
        self.shot = 0;
        self.shot_t = 0.0;
        // 見本はレースの途中から見せる
        for _ in 0..120 * 18 { self.race.step(1.0 / 120.0, Input::NONE); }
        self.race.events.clear();
        let d = &self.save.data;
        let badge_n = d["badges"].as_object().map_or(0, |o| o.len());
        let html = if self.save.num("races") > 0 { format!("{} レース・{} 勝<br>称号 {} / {}", self.save.num("races"), self.save.num("wins"), badge_n, BADGES.len()) } else { String::new() };
        self.ui.title_stats = html;
    }

    /// 今の lineup を読む（読み終わったら on_done）。一度に全部（TS の Promise.all）
    fn load_lineup(&mut self, on_each: Option<Rc<dyn Fn(usize, usize)>>) -> js_sys::Promise {
        let mut uniq: Vec<usize> = Vec::new();
        for h in &self.lineup { if !uniq.contains(h) { uniq.push(*h); } }
        let total = uniq.len();
        let done = Rc::new(std::cell::Cell::new(0usize));
        let waits = js_sys::Array::new();
        for h in uniq {
            let (p, id) = (self.load_view(h), self.heroes[h].id.clone());
            let (done, each) = (done.clone(), on_each.clone());
            waits.push(&wasm_bindgen_futures::future_to_promise(async move {
                if let Err(e) = JsFuture::from(p).await {
                    web_sys::console::error_2(&id.into(), &e);
                }
                done.set(done.get() + 1);
                if let Some(f) = &each { f(done.get(), total); }
                Ok(JsValue::UNDEFINED)
            }));
        }
        js_sys::Promise::all(&waits)
    }

    /// 画面の切り替え: 8 枠の色の帯が閉じて開く（ゲートのよう）
    fn wipe(&mut self, mid: impl FnOnce(&mut App) + 'static) {
        if self.wipe_busy { return; }
        if notrans() || self.ui.wipe_shown() { mid(self); return; }
        self.wipe_busy = true;
        self.ui.wipe_start();
        let me = self.me.clone();
        raf::after_ms(460, move || {
            with(&me, move |a| {
                a.wipe_busy = false;
                mid(a);
                a.ui.wipe_open();
                let me = a.me.clone();
                raf::after_ms(560, move || with(&me, |a| a.ui.wipe_end()));
            });
        });
    }
    fn set_finishing(&mut self, on: bool) { self.ui.finishing = on; self.ui.hud.set_finishing(on); }

    pub fn to_select(&mut self) {
        self.leave_result();
        self.mode = Mode::Select;
        self.career = None;
        self.ui.show(Some(Scr::Select));
        self.ui.set_racing(false);
        self.audio.select();
        self.build_select();
        self.setup_race(None, self.race_seed);
        for v in self.views.values() { v.hide(&mut self.kit); }
        let kit = &mut self.kit;
        let c = self.courses.get_mut(&self.cur_course).unwrap();
        kit.scene[c.pedestal].visible = true;
        c.set_gate_visible(kit, false);
        self.last_greet = -99.0;
        self.select_snap = true;
        let p = self.load_view(self.hero_i);
        spawn_local(async move { let _ = JsFuture::from(p).await; });
    }

    fn build_select(&mut self) {
        let saved_cup_index = self.save.data["cup"].get("index").and_then(|x| x.as_i64());
        let cup_label = match saved_cup_index { Some(i) => format!("カップ 第{}戦", i + 1), None => "3戦カップ".to_string() };
        let hid = self.heroes[self.hero_i].id.clone();
        // 育成の札: 途中経過があればその続き、無ければ今のキャラのデビュー戦
        let saved = self.save.career();
        let career_label = match &saved { Some(c) => format!("育成 第{}戦", c.index + 1), None => "育成".to_string() };
        let c = saved.clone().unwrap_or_else(|| Career::new(&hid));
        let race = c.race();
        let career = CareerCard {
            grade: race.grade.to_string(), race: race.name.to_string(),
            sub: format!("第 {} 戦 / 全 {} 戦・{}", c.index + 1, RACES.len(), self.courses_defs.iter().find(|x| x.id == race.course).map_or("", |x| x.name)),
            rivals: format!("相手: {}・能力 {} 段", DIFFS.iter().find(|(d, _)| *d == race.difficulty).map_or("", |x| x.1), race.cpu_level),
            record: if c.places.is_empty() { "デビュー前・賞金 0 万".to_string() } else { format!("{} 戦 {} 勝・賞金 {}", c.places.len(), c.wins, money(c.prize)) },
            levels: c.levels, running: saved.is_some(), reset_label: if self.reset_armed { "もう一度押すと、途中経過が消えます".to_string() } else { "最初から育て直す".to_string() },
        };
        let career_on = self.is_career_mode;
        let courses = self.courses_defs.iter().enumerate().map(|(ci, c)| {
            let best = self.save.best(&self.save.best_key(&hid, c.id));
            let on = if self.is_cup_mode { ci as i64 == saved_cup_index.unwrap_or(0) } else { c.id == self.course_id };
            CourseBtn { name: c.name.to_string(), blurb: c.blurb.to_string(), best: best.filter(|x| *x != 0.0).map(time_text), on }
        }).collect();
        let diff = DIFFS.iter().position(|(d, _)| *d == self.save.s.difficulty).unwrap_or(1);
        let best = self.save.best(&self.save.best_key(&hid, &self.course_id));
        let tex = self.hero_tex.borrow();
        self.ui.heroes = self.heroes.iter().enumerate().map(|(i, h)| HeroBtn { tex: tex.get(i).cloned().flatten(), full: !h.portrait }).collect();
        drop(tex);
        self.ui.set_select(SelectData {
            cup_mode: self.is_cup_mode, cup_label, career_mode: career_on, career_label, career, courses, diff, hero_on: self.hero_i,
            name: self.heroes[self.hero_i].name.clone(),
            best: if career_on { if saved.is_some() { "育成中".to_string() } else { String::new() } } else { best.filter(|x| *x != 0.0).map_or(String::new(), |x| format!("自己ベスト {}", time_text(x))) },
        });
    }

    pub fn pick_hero(&mut self, i: i64) {
        let n = self.heroes.len() as i64;
        let to = (((i + n) % n) + n) as usize % n as usize;
        // 育成の途中は、育てているキャラから替えられない（替えるには「最初から育て直す」）
        if self.is_career_mode && self.save.career().is_some_and(|c| c.hero != self.heroes[to].id) { self.audio.bump(true); return; }
        self.hero_i = to;
        self.save.s.hero = self.heroes[self.hero_i].id.clone();
        self.save.write();
        self.pick_seq += 1;
        let seq = self.pick_seq;
        self.audio.select();
        self.build_select();
        self.loading_hero = true;
        for v in self.views.values() { v.hide(&mut self.kit); }
        let p = self.load_view(self.hero_i);
        let id = self.heroes[self.hero_i].id.clone();
        let me = self.me.clone();
        spawn_local(async move {
            let r = JsFuture::from(p).await;
            with(&me, move |a| {
                match r {
                    Ok(_) => {
                        if seq == a.pick_seq {
                            a.picked_at = a.t_all;
                            a.last_greet = a.t_all - 5.0;
                            if let Some(v) = a.views.get(&id) { v.stop_all(&mut a.kit); }
                        }
                    }
                    Err(e) => web_sys::console::error_1(&e),
                }
                if seq == a.pick_seq { a.loading_hero = false; }
            });
        });
    }

    pub fn pick_course(&mut self, id: &str) {
        if id == self.course_id { return; }
        self.course_id = id.to_string();
        self.save.s.course = id.to_string();
        self.save.write();
        self.audio.select();
        self.build_select();
        // GLB は読み込み済み（TS の await useCourse）
        self.use_course_now(id);
        self.setup_race(None, self.race_seed);
        for v in self.views.values() { v.hide(&mut self.kit); }
        let kit = &mut self.kit;
        let c = self.courses.get_mut(&self.cur_course).unwrap();
        kit.scene[c.pedestal].visible = true;
        c.set_gate_visible(kit, false);
        self.select_snap = true;
    }

    pub fn go(&mut self) {
        if self.loading_hero || self.wipe_busy { return; }
        self.career = None;
        if self.is_career_mode {
            let c = self.save.career().filter(|c| self.heroes.iter().any(|h| h.id == c.hero)).unwrap_or_else(|| Career::new(&self.heroes[self.hero_i].id));
            if let Some(hi) = self.heroes.iter().position(|h| h.id == c.hero) { self.hero_i = hi; }
            self.career = Some(c);
            self.cup = None;
        } else if self.is_cup_mode {
            let sc = self.save.data["cup"].clone();
            if sc.is_object() {
                let rows: Vec<CupRow> = sc["rows"].as_array().map_or(Vec::new(), |r| r.iter().filter_map(row_of_json).collect());
                self.cup = Some(CupState { index: sc["index"].as_i64().unwrap_or(0) as usize, rows });
                if let Some(hi) = sc["hero"].as_str().and_then(|h| self.heroes.iter().position(|x| x.id == h)) { self.hero_i = hi; }
                if let Some(d) = sc["difficulty"].as_str().and_then(Difficulty::parse) { self.save.s.difficulty = d; }
            } else { self.cup = Some(CupState { index: 0, rows: Vec::new() }); }
            self.course_id = self.courses_defs[self.cup.as_ref().unwrap().index].id.to_string();
        } else { self.cup = None; }
        self.wipe(|a| a.start_race(false));
    }

    pub fn start_race(&mut self, skip_intro: bool) {
        self.start_seq += 1;
        let seq = self.start_seq;
        self.ui.show(None);
        self.ui.set_racing(false);
        self.audio.confirm();
        if let Some(c) = &self.cup { self.course_id = self.courses_defs[c.index].id.to_string(); }
        if let Some(c) = &self.career { self.course_id = c.race().course.to_string(); }
        let id = self.course_id.clone();
        self.use_course_now(&id);
        self.race_seed += 1;
        let seed = self.race_seed;
        let player_gate = match &self.cup { Some(c) => (c.index as i64 * 3 + 2 + (seed & 1)) % 8, None => seed % 8 } as usize;
        let mut lineup = self.pick_lineup(Some(player_gate), seed);
        // カップは同じ 8 人で走る（枠だけ入れ替える）
        if let Some(c) = &self.cup && !c.rows.is_empty() {
            let me_id = &self.heroes[self.hero_i].id;
            let ids: Vec<&String> = c.rows.iter().map(|r| &r.id).filter(|x| *x != me_id).collect();
            let mut q = ((seed * 3) % ids.len() as i64) as usize;
            for (g, h) in lineup.iter_mut().enumerate() {
                if g == player_gate { continue; }
                let target = ids[q % ids.len()];
                q += 1;
                if let Some(x) = self.heroes.iter().position(|x| &x.id == target) { *h = x; }
            }
        }
        self.lineup = lineup.clone();
        // 読み込みが要るときだけ読み込みの画面を出す（もう一度はすぐ走れる）
        if lineup.iter().any(|h| !self.views.contains_key(&self.heroes[*h].id)) { self.ui.show(Some(Scr::Loading)); self.ui.progress(pct(0.2), Some("走者がゲートへ向かっています…")); }
        let me = self.me.clone();
        let wait = self.load_lineup(Some(Rc::new(move |d, t| with(&me, move |a| a.ui.progress(pct(0.2 + 0.8 * d as f64 / t as f64), None)))));
        let me = self.me.clone();
        spawn_local(async move {
            let _ = JsFuture::from(wait).await;
            with(&me, move |a| a.start_race_loaded(seq, player_gate, seed, lineup, skip_intro));
        });
    }

    fn start_race_loaded(&mut self, seq: u32, player_gate: usize, seed: i64, keep: Vec<usize>, skip_intro: bool) {
        if seq != self.start_seq { return; }
        self.setup_race(Some(player_gate), seed);
        self.lineup = keep;
        self.reset_views_for_lineup();
        let tex = self.hero_tex.borrow();
        let faces: Vec<Option<Tex>> = self.lineup.iter().map(|h| tex.get(*h).cloned().flatten()).collect();
        drop(tex);
        self.ui.hud.set_faces(faces);
        self.set_player_layers(Some(player_gate));
        if let Some(c) = &mut self.cup && c.rows.is_empty() {
            let ids: Vec<String> = self.lineup.iter().map(|h| self.heroes[*h].id.clone()).collect();
            c.rows = empty_cup(&ids);
        }
        self.audio.silence();
        let sub = match &self.cup { Some(c) => format!("第{}戦 / 3戦カップ", c.index + 1), None => "発走".to_string() };
        let (name, laps) = (self.race.track.def.name, self.race.track.def.laps);
        self.ui.intro = [sub, name.to_string(), format!("{}m・{}周・{}番枠", sim_math::round(self.race.track.race_len) as i64, laps, self.race.player().unwrap().gate)];
        // 育成: レースの名前を大きく、コースは下の行へ
        if let Some(c) = &self.career {
            let r = c.race();
            self.ui.intro = [format!("育成 第{}戦 / 全{}戦　{}", c.index + 1, RACES.len(), r.grade), r.name.to_string(), format!("{}・{}m・{}周・{}番枠", name, sim_math::round(self.race.track.race_len) as i64, laps, self.race.player().unwrap().gate)];
        }
        if skip_intro { self.mode = Mode::Race; self.ui.show(None); self.ui.set_racing(true); }
        else { self.mode = Mode::Intro; self.intro_t = 0.0; self.ui.show(Some(Scr::Intro)); self.audio.fanfare(); }
    }

    // ---- 出来事 → 音・粒・実況 ----
    fn name_of(&self, i: usize) -> String { self.lineup.get(i).map_or(String::new(), |h| self.heroes[*h].name.clone()) }
    fn say(&mut self, text: &str, gap: f64, sec: f64) {
        if self.race.t - self.last_say < gap { return; }
        self.last_say = self.race.t;
        self.ui.hud.say(text, sec);
    }
    fn handle(&mut self, e: RaceEvent, live: bool) {
        use RaceEvent as E;
        self.ev_log.push(event_name(&e, live));
        let me = self.race.player_gate;
        let is_me = |i: usize| me == Some(i);
        match e {
            E::Count { n } => { if live && (1..=3).contains(&n) { self.ui.hud.count(&n.to_string(), false); self.audio.count(); } }
            E::Go => {
                let kit = &mut self.kit;
                self.courses.get_mut(&self.cur_course).unwrap().open_gate(kit, 0.001);
                if live {
                    self.ui.hud.count("GO!", true);
                    self.audio.go();
                    self.audio.play(Scene::Play);
                    self.trauma = self.trauma.max(0.22);
                    self.go_flash = 1.0;
                    self.say("スタートしました！", 0.0, 2.4);
                }
            }
            E::Jump { i } => { if is_me(i) { self.audio.jump(); } }
            E::Land { i, .. } => {
                self.dust_burst(i, 7, 1.2);
                if is_me(i) { self.audio.land(); self.trauma = self.trauma.max(0.14); }
            }
            E::Clear { i, perfect, .. } => {
                if is_me(i) {
                    self.audio.clear(perfect);
                    if perfect { self.ui.hud.tip("<b>ナイスジャンプ！</b>", 1.1); }
                    self.cheer_kick(0.22);
                }
            }
            E::Stumble { i, .. } => {
                self.splash_stars(i);
                if is_me(i) { self.audio.stumble(); self.trauma = (self.trauma + 0.28).min(1.0); self.say("あっと！ よろけた！", 0.0, 2.4); }
                else { let n = self.name_of(i); self.say(&format!("{n}、よろけた！"), 3.0, 2.4); }
            }
            E::Splash { i } => { self.water_burst(i); if is_me(i) { self.audio.splash(); } }
            E::Bump { i, j, side } => { if let Some(m) = me && (i == m || j == m) { self.audio.bump(side); } }
            E::Rail { .. } => self.audio.bump(true),
            E::DashStart { i } => { if is_me(i) { self.audio.dash(); } }
            E::DashEnd { .. } => {}
            E::Low { i } => { if is_me(i) { self.audio.low(); } }
            E::Empty { i } => { if is_me(i) { self.audio.empty(); self.ui.hud.tip("息切れ。<b>離すと回復</b>", 2.2); } }
            E::Recovered { i } => { if is_me(i) { self.audio.recovered(); } }
            E::SlipReady { i } => {
                if is_me(i) {
                    self.audio.slip_ready();
                    if self.first_race || !self.tips_shown.contains("slip") { self.tips_shown.insert("slip".into()); self.ui.hud.tip("風よけ満タン！ <b>横へ出ると伸びる</b>", 2.0); }
                }
            }
            E::Sling { i } => {
                if is_me(i) { self.audio.sling(); self.ui.hud.tip("<b>抜け出し！</b>", 0.9); self.sling_fx = 1.0; self.burst_t = 0.0; self.cheer_kick(0.1); }
            }
            E::Pass { i, j } => {
                if let Some(m) = me {
                    if i == m {
                        let rank = self.race.runners[m].rank;
                        self.audio.pass(rank);
                        self.cheer_kick(0.12);
                        if rank <= 3 {
                            let n = self.name_of(m);
                            let t = if rank == 1 { format!("{n}、先頭に立った！") } else { format!("{n}、{rank}番手に上がった！") };
                            self.say(&t, 2.0, 2.4);
                        }
                    } else if j == m { self.audio.passed(); }
                }
            }
            E::Lead { i } => { if me != Some(i) { let n = self.name_of(i); self.say(&format!("先頭は{n}！"), 4.0, 2.4); } }
            E::Lap { lap } => {
                let laps = self.race.track.def.laps;
                self.say(&if lap == laps { "最終周に入った！".to_string() } else { format!("{lap}周目") }, 0.0, 2.4);
                if lap == laps { self.audio.bell(); }
            }
            E::FinalStraight => { self.say("最後の直線！", 0.0, 3.0); self.cheer_target = 0.9; self.audio.final_straight(); }
            E::Pick { i, kind } => {
                self.pick_burst(i, kind);
                if live && is_me(i) {
                    match kind {
                        PickupKind::Mark(s) => { self.audio.pick(); let lv = self.race.runners[i].lv[s.i()]; self.pop_at(i, &format!("{}アップ！ {lv}", s.name()), PopKind::Mark(s)); }
                        PickupKind::Breath => { self.audio.pick(); self.pop_at(i, &format!("息 +{}", ITEM.breath), PopKind::Good); }
                        PickupKind::Poison => { self.audio.poison(); self.pop_at(i, &format!("毒！ 息 −{}", ITEM.poison), PopKind::Bad); self.trauma = self.trauma.max(0.2); }
                        PickupKind::Jet => { self.audio.weapon_get(); self.pop_at(i, "噴射！！ 速さ大アップ", PopKind::Big); self.trauma = self.trauma.max(0.25); self.go_flash = self.go_flash.max(0.4); }
                        PickupKind::Mystery => {}
                        PickupKind::Arm(w) => {
                            self.audio.weapon_get();
                            let ak = self.attack_label();
                            if !self.tips_shown.contains("arm") { self.tips_shown.insert("arm".into()); self.ui.hud.tip(&format!("<b>{}</b>！ {ak}で攻撃・長押しでため", w.name()), 3.2); }
                            else { self.pop_at(i, w.name(), PopKind::Good); }
                        }
                    }
                }
            }
            E::ChargeStart { i } => { if live && is_me(i) { self.audio.charge(); } }
            E::ChargeFull { i } => { if live && is_me(i) { self.audio.charge_full(); self.pop_at(i, "ゴゴゴゴ…", PopKind::Hit); } }
            E::Swing { i, weapon, charged } => {
                self.items.swung(i, self.t_all, weapon, charged);
                if charged { self.swing_burst(i); }
                if let Some(h) = self.lineup.get(i) && let Some(v) = self.views.get_mut(&self.heroes[*h].id) { v.attack(&mut self.kit, charged); }
                if live && (is_me(i) || self.near_me(i, 22.0)) { self.audio.swing(charged); }
                if live && is_me(i) && charged { self.pop_at(i, "いぶきの一撃！！", PopKind::Big); self.go_flash = self.go_flash.max(0.7); }
            }
            E::Hit { by, i, weapon, charged } => {
                self.splash_stars(i);
                self.items.hit(i, self.t_all, charged);
                self.candy_burst(i, charged);
                if live {
                    let word = match (weapon, charged) {
                        (Weapon::Sword, false) => "ズバッ！！", (Weapon::Sword, true) => "ズバババッ！！",
                        (Weapon::Chain, false) => "ドゴォッ！！", (Weapon::Chain, true) => "ドッゴォォン！！",
                        (Weapon::Rose, false) => "チクッ！！", (Weapon::Rose, true) => "チクチクッ！！",
                        (Weapon::Bottle, _) => "ツルッ！！",
                        (Weapon::Crossbow, _) => "シュパーン！！",
                        (Weapon::Hammer, _) => "ドッカーン！！",
                    };
                    // 字は自分が関わる当たりと、自分のそばの当たりだけ（遠くの当たりの字で画面が埋まらない）
                    // 矢は集団を一度に貫くので、字は 0.5 秒に 1 つだけ
                    let bolt_quiet = weapon == Weapon::Crossbow && self.t_all - self.last_bolt_pop < 0.5;
                    if (is_me(by) || is_me(i) || self.near_me(i, 30.0)) && !bolt_quiet {
                        if weapon == Weapon::Crossbow { self.last_bolt_pop = self.t_all; }
                        self.pop_at(i, word, if charged { PopKind::Big } else { PopKind::Hit });
                    }
                    if is_me(by) || is_me(i) {
                        self.hitstop = if charged { 0.22 } else { 0.12 };
                        // 自分のためた一撃が当たった: 一瞬、横から当てた所を見せる（みんなで走るときは時間を変えないので出さない）
                        if charged && is_me(by) && weapon != Weapon::Hammer && weapon != Weapon::Crossbow { self.hit_cam = HIT_CAM_T; self.hit_cam_len = HIT_CAM_T; self.hit_pair = (by, i); }
                        self.trauma = (self.trauma + if is_me(i) { 0.5 } else { 0.3 }).min(1.0);
                        self.go_flash = self.go_flash.max(if charged { 0.9 } else { 0.5 });
                        self.audio.hit(is_me(i), charged);
                        if is_me(i) { let n = self.name_of(by); self.say(&format!("{n}の{}！ よろけた！", weapon.name()), 0.0, 2.4); }
                        else { let n = self.name_of(i); self.say(&format!("{}が命中！ {n}がよろけた！", weapon.name()), 0.0, 2.4); self.cheer_kick(0.25); }
                    } else if self.near_me(i, 30.0) { let (a, b) = (self.name_of(by), self.name_of(i)); self.say(&format!("{a}の{}が{b}に！", weapon.name()), 3.0, 2.2); }
                }
            }
            E::Dodge { i } => {
                if live && is_me(i) { self.audio.dodge(); self.pop_at(i, "ヒラリ！", PopKind::Good); self.ui.hud.tip("<b>ナイスよけ！</b> 少し伸びる", 1.2); self.cheer_kick(0.2); }
            }
            E::WeaponGone { .. } => {}
            // ハンマーで真上へ飛ばされた: 自分が関わるときは、横から飛んでいく所を長めに見せる
            E::Launch { i, by } => {
                if live && (is_me(i) || is_me(by)) { self.hit_cam = LAUNCH_CAM_T; self.hit_cam_len = LAUNCH_CAM_T; self.hit_pair = (by, i); }
                if live && (is_me(i) || self.near_me(i, 30.0)) { self.trauma = (self.trauma + 0.4).min(1.0); }
                if live && is_me(i) { let n = self.name_of(by); self.say(&format!("{n}のハンマー！ 空へ吹っ飛んだ！"), 0.0, 2.6); }
                else if live && is_me(by) { let n = self.name_of(i); self.say(&format!("ハンマーで{n}を空へ吹っ飛ばした！"), 0.0, 2.6); self.cheer_kick(0.4); }
            }
            E::BoltDone { owner, .. } => { if live && is_me(owner) { self.say("矢が走路を 1 周して戻ってきた！", 0.0, 2.2); } }
            E::Finish { i, place, time } => {
                // 僅差は写真判定（プレイヤーが関わるときだけ）
                if let Some(m) = me && place >= 2 {
                    let prev = self.race.runners.iter().position(|x| x.place == place - 1);
                    if let Some(pr) = prev {
                        let close = time - self.race.runners[pr].finish_t < PHOTO_DT && (i == m || pr == m);
                        if close && self.photo.is_none() { let dt = time - self.race.runners[pr].finish_t; self.capture_photo(pr, i, dt); }
                    }
                }
                // 1 着と自分のゴールの瞬間: ゲートの両の柱から紙吹雪と光の粒が吹き上がる
                if place == 1 || me == Some(i) { self.goal_burst(place == 1); }
                if me == Some(i) { self.on_player_finish(place); }
                else if place == 1 { let n = self.name_of(i); self.say(&format!("{n}、1着でゴールイン！"), 0.0, 3.0); self.cheer_kick(0.6); }
            }
        }
    }

    // ---- 写真判定: ゴール線の真横から 1 枚 ----
    fn capture_photo(&mut self, win: usize, lose: usize, dt: f64) {
        self.photo = Some(PhotoState { win, dt });
        let tr = &self.race.track;
        // 走路の中から、ゴール線の真横を撮る。走者から遠い側の柵のそばに置く
        let near_outer = (self.race.runners[win].d + self.race.runners[lose].d) / 2.0 < tr.width / 2.0;
        let p = tr.pos(tr.finish_s, if near_outer { tr.width - 0.2 } else { 0.2 });
        let mut cam = Camera::new(42.0, 16.0 / 9.0, 0.5, 300.0);
        cam.position = Vec3::new(p.x as f32, (p.y + 1.05) as f32, p.z as f32);
        let q = tr.pos(tr.finish_s, if near_outer { 0.0 } else { tr.width });
        cam.look_at(Vec3::new(q.x as f32, (q.y + 1.0) as f32, q.z as f32));
        // ゴールの門は写さない（柱が走者を隠す）
        let finish = self.course().finish;
        let mut hidden: Vec<NodeId> = vec![self.ring, self.me_mark, self.slip_ring, self.ghost_ring, self.ghost_tag, finish];
        for v in self.views.values() { hidden.push(v.tag); }
        let vis: Vec<bool> = hidden.iter().map(|o| self.kit.scene[*o].visible).collect();
        for o in &hidden { self.kit.scene[*o].visible = false; }
        self.kit.render_to(self.photo_rt, &cam);
        let read = self.kit.read_target(self.photo_rt);
        for (o, v) in hidden.iter().zip(vis) { self.kit.scene[*o].visible = v; }
        let me = self.me.clone();
        spawn_local(async move {
            let Ok(px) = read.await else { return };
            let out = photo_pixels(&px);
            with(&me, move |a| {
                let img = egui::ColorImage::from_rgba_unmultiplied([640, 360], &out);
                let handle = a.layer.ctx().load_texture("photo", img, egui::TextureOptions::LINEAR);
                a.ui.photo = Some(Tex { handle, w: 640.0, h: 360.0 });
            });
        });
    }
    fn cheer_kick(&mut self, k: f64) { self.cheer_now = (self.cheer_now + k).min(1.0); }
    /// 攻撃の操作の表記（最後に触った機器）
    fn attack_label(&self) -> String {
        match self.device() { Device::Touch => "「こうげき」".to_string(), Device::Pad => r#"<span class="kbd">B</span>"#.to_string(), Device::Keys => format!(r#"<span class="kbd">{}</span>"#, key_name(&self.keys.key_of(Rebindable::Attack))) }
    }
    /// 走者 i が自分の近く（前後 m 以内）にいるか
    fn near_me(&self, i: usize, m: f64) -> bool { self.race.player().is_some_and(|p| (self.race.runners[i].dist - p.dist).abs() < m) }
    /// 走者 i の頭の上に、弾む字を出す（画面に写っているときだけ）
    fn pop_at(&mut self, i: usize, text: &str, kind: PopKind) {
        let Some(me) = self.race.player() else { return };
        let r = &self.race.runners[i];
        let gap = r.dist - me.dist;
        if !(-3.5..=60.0).contains(&gap) { return; }
        let p = self.race.track.pos(r.s, r.d);
        let v = self.kit.camera.project(Vec3::new(p.x as f32, (p.y + r.y + HERO_HEIGHT + 0.5) as f32, p.z as f32));
        let (w, h) = self.kit.size();
        self.ui.hud.pop(text, (v.x * 0.5 + 0.5) * w, (0.5 - v.y * 0.5) * h, kind);
    }
    fn pick_burst(&mut self, i: usize, kind: PickupKind) {
        let p = self.runner_pos(i);
        let y = self.race.runners[i].y;
        let c = Color::css(match kind { PickupKind::Mark(s) => stat_color(s), PickupKind::Breath => "#8fe27a", PickupKind::Poison => "#a34be0", PickupKind::Arm(_) | PickupKind::Mystery => "#ffcf4d", PickupKind::Jet => "#ff8a3c" });
        for _ in 0..12 {
            let (vx, vy, vz) = ((random() - 0.5) * 4.0, 1.8 + random() * 3.0, (random() - 0.5) * 4.0);
            self.fx.emit(p.x as f32, (p.y + y + 0.9) as f32, p.z as f32, vx as f32, vy as f32, vz as f32, c, Emit { life: 0.5, size: 0.26, grow: 0.2, a: 1.0, g: 6.0 });
        }
    }
    fn runner_pos(&self, i: usize) -> P3 { let r = &self.race.runners[i]; self.race.track.pos(r.s, r.d) }
    fn dust_burst(&mut self, i: usize, n: usize, power: f64) {
        let p = self.runner_pos(i);
        let dirt = self.th().dirt;
        for _ in 0..n {
            let (x, z) = (p.x + (random() - 0.5) * 0.7, p.z + (random() - 0.5) * 0.7);
            let (vx, vy, vz) = ((random() - 0.5) * 2.4 * power, 0.7 + random() * 1.4 * power, (random() - 0.5) * 2.4 * power);
            let life = 0.4 + random() * 0.3;
            // TS の引数の順（x, y, z, vx, vy, vz, 色, 指定）で乱数を読む
            self.fx.emit(x as f32, (p.y + 0.12) as f32, z as f32, vx as f32, vy as f32, vz as f32, dirt, Emit { life: life as f32, size: 0.26, grow: 2.4, a: 0.42, g: 2.0 });
        }
    }
    fn water_burst(&mut self, i: usize) {
        let p = self.runner_pos(i);
        let c = Color::css("#d8f4ff");
        for _ in 0..26 {
            let (vx, vy, vz) = ((random() - 0.5) * 6.0, 2.6 + random() * 3.8, (random() - 0.5) * 6.0);
            self.fx.emit(p.x as f32, (p.y + 0.1) as f32, p.z as f32, vx as f32, vy as f32, vz as f32, c, Emit { life: 0.5, size: 0.24, grow: 0.3, a: 0.95, g: 16.0 });
        }
    }
    fn splash_stars(&mut self, i: usize) {
        let p = self.runner_pos(i);
        let c = Color::css("#ffe07a");
        for _ in 0..14 {
            let (vx, vy, vz) = ((random() - 0.5) * 6.0, 1.5 + random() * 3.0, (random() - 0.5) * 6.0);
            self.fx.emit(p.x as f32, (p.y + 1.1) as f32, p.z as f32, vx as f32, vy as f32, vz as f32, c, Emit { life: 0.55, size: 0.3, grow: 0.1, a: 1.0, g: 10.0 });
        }
    }
    /// 当たった相手からお菓子の色の破片が飛び散る（ためた一撃は多く、遠くまで）
    fn candy_burst(&mut self, i: usize, charged: bool) {
        let p = self.runner_pos(i);
        let r = &self.race.runners[i];
        let (y, yaw, sp) = (r.y, self.race.track.yaw(r.s), r.speed);
        let n = if charged { 34 } else { 18 };
        let pow = if charged { 1.6 } else { 1.0 };
        for k in 0..n {
            let c = Color::css(["#ff8fbf", "#7fe0c4", "#ffe066", "#8fc8ff", "#ffffff", "#ffb36b"][k % 6]);
            let a = random() * 6.28;
            let h = (random() * 5.0 + 2.0) * pow;
            let (vx, vz) = (a.cos() * h + yaw.sin() * sp * 0.6, a.sin() * h + yaw.cos() * sp * 0.6);
            let vy = (2.5 + random() * 4.5) * pow;
            self.fx.emit(p.x as f32, (p.y + y + 1.0) as f32, p.z as f32, vx as f32, vy as f32, vz as f32, c, Emit { life: (0.55 + random() * 0.35) as f32, size: if k % 3 == 0 { 0.42 } else { 0.28 }, grow: -0.3, a: 1.0, g: 14.0 });
        }
    }
    /// ゴールのゲートの両の柱の上から、紙吹雪（お菓子の色）と光の粒を吹き上げる。1 着のときは多い
    fn goal_burst(&mut self, first: bool) {
        let tr = &self.race.track;
        let posts = [tr.pos(tr.finish_s, -0.4), tr.pos(tr.finish_s, tr.width + 0.4)];
        let yaw = tr.yaw(tr.finish_s);
        let cols = ["#f7d64a", "#f48fb1", "#82d7ff", "#9be7a1", "#ffb366", "#ffffff"];
        let n = if first { 90 } else { 55 };
        for (k, p) in posts.iter().enumerate() {
            // 柱から走路の内へ向けて吹く
            let inward = if k == 0 { 1.0 } else { -1.0 };
            for q in 0..n {
                let c = Color::css(cols[q % cols.len()]);
                let side = inward * (1.5 + random() * 3.5);
                let fwd = (random() - 0.5) * 3.5;
                let (vx, vz) = (yaw.cos() * side + yaw.sin() * fwd, -yaw.sin() * side + yaw.cos() * fwd);
                let life = 1.4 + random() * 0.8;
                self.fx.emit(p.x as f32, (p.y + 2.8 + random() * 0.8) as f32, p.z as f32, vx as f32, (1.8 + random() * 1.6) as f32, vz as f32, c, Emit { life: life as f32, size: (0.26 + random() * 0.14) as f32, grow: 0.0, a: 1.0, g: 3.6 });
            }
            for _ in 0..18 {
                let c = Color::css(if random() < 0.5 { "#fff6c8" } else { "#ffe36a" });
                self.fx.emit(p.x as f32, (p.y + 0.3 + random() * 2.5) as f32, p.z as f32, ((random() - 0.5) * 1.2) as f32, (2.0 + random() * 2.0) as f32, ((random() - 0.5) * 1.2) as f32, c, Emit { life: 0.8, size: 0.22, grow: -0.6, a: 1.0, g: 1.0 });
            }
        }
        self.cheer_kick(if first { 0.7 } else { 0.4 });
    }
    /// ためた一撃を放った瞬間: 体から前へ金の火花が噴き出す
    fn swing_burst(&mut self, i: usize) {
        let p = self.runner_pos(i);
        let r = &self.race.runners[i];
        let (y, yaw, sp) = (r.y, self.race.track.yaw(r.s), r.speed);
        for k in 0..26 {
            let c = Color::css(if k % 3 == 0 { "#ffffff" } else { "#ffc93c" });
            let side = (random() - 0.5) * 7.0;
            let fwd = sp + 6.0 + random() * 10.0;
            let (vx, vz) = (yaw.sin() * fwd + yaw.cos() * side, yaw.cos() * fwd - yaw.sin() * side);
            self.fx.emit(p.x as f32, (p.y + y + 0.9 + random() * 0.6) as f32, p.z as f32, vx as f32, ((random() - 0.3) * 3.0) as f32, vz as f32, c, Emit { life: 0.3, size: 0.3, grow: -0.6, a: 1.0, g: 0.0 });
        }
    }

    fn on_player_finish(&mut self, place: i32) {
        self.mode = Mode::Finish;
        self.finish_timer = 0.0;
        self.set_finishing(true);
        self.ui.hud.show_place(&format!("{place}着"), place == 1);
        self.cheer_now = 1.0;
        if place <= 3 { self.confetti(if place == 1 { 240 } else { 90 }); }
        self.audio.finish(place);
        self.final_u.gold = if place == 1 { 0.5 } else { 0.0 };
    }
    fn confetti(&mut self, n: usize) {
        let m = self.race.player().unwrap();
        let p = self.race.track.pos(m.s + 6.0, m.d);
        for _ in 0..n {
            let col = Color::css(GATE_COLOR[(random() * 8.0).floor() as usize]);
            let (x, y, z) = (p.x + (random() - 0.5) * 16.0, p.y + 9.0 + random() * 7.0, p.z + (random() - 0.5) * 16.0);
            let (vx, vy, vz) = ((random() - 0.5) * 2.0, -1.5 - random() * 2.0, (random() - 0.5) * 2.0);
            let life = 2.4 + random() * 1.8;
            self.fx.emit(x as f32, y as f32, z as f32, vx as f32, vy as f32, vz as f32, col, Emit { life: life as f32, size: 0.2, grow: 0.0, a: 1.0, g: 0.6 });
        }
    }
    fn show_photo(&mut self) {
        self.mode = Mode::Photo;
        self.photo_t = 0.0;
        self.ui.set_racing(false);
        self.ui.photo_res.clear();
        self.ui.show(Some(Scr::Photo));
        self.audio.bell();
    }

    pub fn to_result(&mut self) {
        if !self.race.done { self.race.fast_forward_finish(); }
        let pi = self.race.player_gate.unwrap();
        self.mode = Mode::Result;
        self.result_t = 0.0;
        self.ui.set_racing(false);
        self.set_finishing(false);
        self.final_u.gold = 0.0;
        self.kit.scene[self.ghost_ring].visible = false;
        self.kit.scene[self.ghost_tag].visible = false;
        // 表彰台へ切り替える
        {
            let kit = &mut self.kit;
            let c = self.courses.get_mut(&self.cur_course).unwrap();
            kit.scene[c.podium].visible = true;
            c.set_gate_visible(kit, false);
        }
        for v in self.views.values() { v.stop_all(&mut self.kit); v.hide(&mut self.kit); }
        let a = self.stage_point(0.5, 9.0);
        self.cam_pos = DVec3::new(a.x, a.y + 2.1, a.z);
        let b = self.stage_point(0.0, 0.0);
        self.cam_look = DVec3::new(b.x, b.y + 1.9, b.z);
        self.cheer_clock = 0.35;
        self.go_flash = 0.8;
        self.cheer_target = 0.7;
        self.ui.show(Some(Scr::Result));
        self.audio.play(Scene::Clear);
        // 記録
        let p = self.race.runners[pi].clone();
        let hero_id = self.heroes[self.hero_i].id.clone();
        let cid = self.course_id.clone();
        let races = self.save.num("races") + 1;
        self.save.data["races"] = json!(races);
        if p.place == 1 { let w = self.save.num("wins") + 1; self.save.data["wins"] = json!(w); }
        let bk = self.save.best_key(&hero_id, &cid);
        let prev = self.save.best(&bk).filter(|x| *x != 0.0);
        // 育成のタイムは自己ベスト・メダル・影に数えない（能力が違うので比べられない）
        let in_career = self.career.is_some();
        let is_best = !in_career && prev.is_none_or(|x| p.finish_t < x);
        // 祝いの段: 自己ベストで、そのレースが 1 着・新しい金メダル・カップ優勝（下で足す）なら、いちばん上
        let mut cheer_top = p.place == 1;
        if is_best {
            self.save.data["bests"][&bk] = json!(p.finish_t);
            if self.lap_split != 0.0 { self.save.put("splits", &bk, json!(self.lap_split)); }
            self.save.put("ghosts", &bk, json!(self.ghost_run));
        }
        let mut unlocks: Vec<&str> = Vec::new();
        for (key, cond, label) in [("finish", true, "はじめての完走"), ("podium", p.place <= 3, "はじめての入賞"), ("win", p.place == 1, "はじめての優勝")] {
            if cond && !self.save.data["firsts"][key].as_bool().unwrap_or(false) { self.save.data["firsts"][key] = json!(true); unlocks.push(label); }
        }
        self.save.s.first_done = true;
        self.first_race = false;
        // マークと武器の案内は、武器を当てるか 3 レース走るまで
        if p.stats.hits_dealt > 0 || races >= 3 { self.save.data["firsts"]["items"] = json!(true); }
        let got = earned_badges(&p, &self.race);
        let new_badges: Vec<&str> = got.iter().copied().filter(|b| self.save.data["badges"].get(*b).and_then(|x| x.as_i64()).unwrap_or(0) == 0).collect();
        for b in &got { let n = self.save.data["badges"].get(*b).and_then(|x| x.as_i64()).unwrap_or(0) + 1; self.save.data["badges"][*b] = json!(n); }
        let medal = if in_career { 0 } else { medal_of(&cid, p.finish_t) };
        let mk = format!("{cid}|{}", self.save.s.difficulty.id());
        let new_medal = medal as i64 > self.save.data["medals"].get(&mk).and_then(|x| x.as_i64()).unwrap_or(0);
        if new_medal { self.save.data["medals"][&mk] = json!(medal); }
        cheer_top |= new_medal && medal == 3;
        self.save.write();
        // 左
        let track_name = self.race.track.def.name;
        let mut d = ResultData { eyebrow: match &self.cup { Some(c) => format!("第{}戦 {}", c.index + 1, track_name), None => track_name.to_string() }, ..Default::default() };
        d.place = p.place.to_string();
        d.win = p.place == 1;
        d.unit = "着".into();
        d.time = time_text(p.finish_t);
        d.time_s = p.finish_t;
        d.time_em = if in_career { None } else if is_best { Some(("自己ベスト！".to_string(), false)) } else { prev.map(|x| (format!("ベスト {}", time_text(x)), true)) };
        let (line, next) = reflect(&p, &self.race);
        {
            let ahead = self.race.runners.iter().find(|x| x.place == p.place - 1);
            let behind = self.race.runners.iter().find(|x| x.place == p.place + 1);
            d.rival = if let Some(a) = ahead { format!("次の相手: {}（あと {} 秒）", self.name_of(a.i), to_fixed(p.finish_t - a.finish_t, 2)) }
                else if let Some(b) = behind { format!("2 着の {} に {} 秒差", self.name_of(b.i), to_fixed(b.finish_t - p.finish_t, 2)) } else { String::new() };
        }
        d.line = line;
        d.next = next;
        // 「金」だけでは何か分からないので「金メダル」と書く（きろくの札も同じ）
        const MEDAL_NAME: [&str; 4] = ["", "銅メダル", "銀メダル", "金メダル"];
        if medal != 0 { d.pills.push(Pill { text: format!("{}{}", MEDAL_NAME[medal as usize], if new_medal { " NEW" } else { "" }), medal: Some(medal as i64), new: false }); }
        // 称号は新しいものを優先して 2 つだけ。残りは「きろく」で見られる
        let mut ord: Vec<&str> = new_badges.clone();
        ord.extend(got.iter().copied().filter(|b| !new_badges.contains(b)));
        for b in ord.iter().take(2) { d.pills.push(Pill { text: badge(b).unwrap().1.to_string(), medal: None, new: new_badges.contains(b) }); }
        if got.is_empty() && let Some(u) = unlocks.first() { d.pills.push(Pill { text: u.to_string(), medal: None, new: false }); }
        // 順位の流れ（viewBox 300×56 の点を、横は 0..1 に）
        let log = &p.rank_log;
        d.chart = log.iter().enumerate().map(|(i, r)| {
            let x: f32 = to_fixed(i as f64 / ((log.len() as f64 - 1.0).max(1.0)) * 300.0, 1).parse().unwrap_or(0.0);
            let y: f32 = to_fixed((*r as f64 - 1.0) / 7.0 * (56.0 - 8.0) + 4.0, 1).parse().unwrap_or(0.0);
            (x / 300.0, y)
        }).collect();
        let next_medal = if medal < 3 && !in_career { medal_times(&cid).map(|m| m[(2 - medal) as usize]) } else { None };
        let corners: Vec<String> = p.corners.iter().map(|x| x.to_string()).collect();
        d.pass = format!("通過順 {}-{}", corners.join("-"), p.place);
        d.next_medal = next_medal.map(|m| format!("次のメダル {}", time_text(m)));
        // 掲示板
        let mut ordered: Vec<Runner> = self.race.runners.clone();
        ordered.sort_by_key(|x| x.place);
        let mut places: HashMap<String, i32> = HashMap::new();
        for r in &ordered { places.insert(self.heroes[self.lineup[r.i]].id.clone(), r.place); }
        if let Some(c) = &mut self.cup { add_race(&mut c.rows, &places); }
        let cup_on = self.cup.is_some();
        d.rows = ordered.iter().enumerate().map(|(k, r)| ResRow {
            me: r.i == pi, place: r.place.to_string(), gate: Some(r.gate), name: self.name_of(r.i), time: Some(time_text(r.finish_t)),
            margin: if k == 0 { String::new() } else { margin_text(r.finish_t - ordered[k - 1].finish_t).to_string() },
            pts: cup_on.then(|| format!("+{}", POINTS[r.place as usize - 1])),
        }).collect();
        if let Some(c) = &self.cup {
            let st = standings(&c.rows);
            let my = st.iter().position(|r| r.id == hero_id).map_or(0, |x| x + 1);
            let last = c.index >= self.courses_defs.len() - 1;
            let (index, rows) = (c.index, c.rows.clone());
            if last {
                // 最終戦のあとはカップの順位表を見せる
                d.eyebrow = "3戦カップ 最終結果".into();
                d.place = my.to_string();
                d.win = my == 1;
                d.unit = "位".into();
                let name_by = |id: &str| self.heroes.iter().find(|h| h.id == id).map_or(String::new(), |h| h.name.clone());
                d.rows = st.iter().enumerate().map(|(k, r)| {
                    let g = self.lineup.iter().position(|h| self.heroes[*h].id == r.id).map_or(0, |x| x + 1);
                    let pl: Vec<String> = r.places.iter().map(|x| x.to_string()).collect();
                    ResRow { me: r.id == hero_id, place: (k + 1).to_string(), gate: (g >= 1).then_some(g), name: name_by(&r.id), time: None, margin: pl.join("-"), pts: Some(format!("{}点", r.points)) }
                }).collect();
                d.cup = if my == 1 { "カップ優勝！".to_string() } else { format!("優勝まで {} 点", st[0].points - st[my - 1].points) };
                cheer_top |= my == 1;
                if my == 1 && !self.save.data["firsts"]["cupWin"].as_bool().unwrap_or(false) {
                    self.save.data["firsts"]["cupWin"] = json!(true);
                    d.pills.push(Pill { text: "はじめてのカップ優勝".into(), medal: None, new: true });
                }
                self.save.data["cup"] = Value::Null;
            } else {
                let name_by = |id: &str| self.heroes.iter().find(|h| h.id == id).map_or(String::new(), |h| h.name.clone());
                let me_pts = rows.iter().find(|r| r.id == hero_id).map_or(0, |r| r.points);
                let top: Vec<String> = st.iter().take(3).enumerate().map(|(i, r)| format!("{}. {} {}点", i + 1, name_by(&r.id), r.points)).collect();
                d.cup = format!("いまの順位: {my}位（{me_pts}点）　{}", top.join("　"));
                self.save.data["cup"] = json!({ "index": index + 1, "rows": rows.iter().map(row_to_json).collect::<Vec<_>>(), "hero": hero_id, "difficulty": self.save.s.difficulty.id() });
            }
            d.again = if last { "もう一度カップ".to_string() } else { format!("第{}戦へ", index + 2) };
            self.save.write();
        } else {
            d.again = "もう一度".into();
        }
        // 育成: 育った能力・賞金・次のレース。5 着以下か最後のレースなら引退して、殿堂へ
        let mut career_done = false;
        if let Some(mut c) = self.career.take() {
            let (race, idx, before) = (c.race(), c.index, c.levels);
            let out = c.record(p.place, p.lv);
            d.eyebrow = format!("第{}戦 {} {}", idx + 1, race.grade, race.name);
            let mut pills: Vec<Pill> = Stat::ALL.iter().filter(|s| p.lv[s.i()] > before[s.i()]).map(|s| Pill { text: format!("{} +{}", s.name(), p.lv[s.i()] - before[s.i()]), medal: None, new: true }).collect();
            pills.append(&mut d.pills);
            d.pills = pills;
            for (row, r) in d.rows.iter_mut().zip(&ordered) { let z = sim::career::prize_of(race.purse, r.place); row.pts = (z > 0).then(|| format!("+{z}万")); }
            if out.retired {
                career_done = idx + 1 == RACES.len();
                let head = if career_done { "引退レースを走りきった！" } else { "5 着以下のため、ここで引退。" };
                d.cup = format!("{head} 通算 {} 戦 {} 勝・賞金 {}。称号「{}」で殿堂に残りました", c.places.len(), c.wins, money(c.prize), c.title());
                d.pills.insert(0, Pill { text: format!("称号「{}」", c.title()), medal: None, new: true });
                // 左の欄にも大きく出す（引退はこのモードの山場）
                d.eyebrow = format!("引退　{}", d.eyebrow);
                d.line = format!("引退。通算 {} 戦 {} 勝・賞金 {}", c.places.len(), c.wins, money(c.prize));
                d.next = if career_done { "全 8 戦を走りきった。殿堂に名前が残る".to_string() } else { "5 着以下は、その場で引退。殿堂に名前が残る".to_string() };
                d.rival = String::new();
                let mut hall = self.save.hall();
                hall_insert(&mut hall, HallEntry::of(&c));
                self.save.data["hall"] = json!(hall.iter().map(hall_to_json).collect::<Vec<_>>());
                self.save.data["career"] = Value::Null;
                d.again = "新しく育てる".into();
                cheer_top = career_done && p.place == 1;
            } else {
                let next = c.race();
                d.cup = format!("賞金 +{}（合計 {}）。次は第{}戦 {} {}（相手の能力 {} 段）", money(out.prize), money(c.prize), c.index + 1, next.grade, next.name, next.cpu_level);
                self.save.data["career"] = career_to_json(&c);
                d.again = format!("第{}戦へ", c.index + 1);
            }
            self.save.write();
            self.career = Some(c);
        }
        self.ui.set_result(d);
        // タイムの数え上げ → 数え終わりの祝い（見た目と音だけ。上の記録と遊びの計算には触れない）。育成を走りきったときも祝う
        self.show = ResultShow::new_f64(p.finish_t, cheer_of(is_best || career_done, cheer_top), self.calm);
        self.cheer_log.clear();
    }

    /// 結果の画面から先へ進む: 数え上げと紙吹雪を打ち切る（タイムは本当の値になる）
    fn leave_result(&mut self) {
        self.show.cancel();
        self.ui.cheer_clear();
    }
    /// 結果の画面の祝いを進める（数え上げの刻み・数え終わりの祝い・いちばん上のもう 1 回）。音と見た目だけ
    fn cheer_tick(&mut self, dt: f64) {
        for e in self.show.step(dt) {
            let name = match e {
                CheerEvent::Tick(k) => { self.audio.count_tick(k); "tick" }
                CheerEvent::Done(Cheer::None) => { self.audio.count_done(); self.ui.cheer_done(false); "done" }
                CheerEvent::Done(c) => { self.audio.cheer(c == Cheer::BestTop); self.ui.cheer_done(true); if c == Cheer::BestTop { "bestTop" } else { "best" } }
                CheerEvent::Encore => { self.audio.cheer_encore(); self.ui.cheer_encore(); "encore" }
            };
            self.cheer_log.push(name);
        }
        if self.show.counting() { self.ui.cheer_count(self.show.value_f64()); }
    }

    pub fn again(&mut self) {
        self.leave_result();
        let n = self.courses_defs.len();
        if let Some(c) = &self.career {
            // 引退したあとは、同じキャラで新しく育てる
            if c.retired { self.career = Some(Career::new(&self.heroes[self.hero_i].id)); }
            self.start_race(false);
        } else if let Some(c) = &mut self.cup {
            if c.index >= n - 1 {
                self.cup = Some(CupState { index: 0, rows: Vec::new() });
                self.save.data["cup"] = Value::Null;
                self.save.write();
            } else { c.index += 1; }
            self.start_race(false);
        } else { self.start_race(true); }
    }

    // ---- きろく（コースごとの自己ベストとメダル・称号のコレクション）----
    fn show_records(&mut self) {
        let d = &self.save.data;
        let name = ["メダルなし", "銅メダル", "銀メダル", "金メダル"];
        let courses = self.courses_defs.iter().map(|c| {
            let mut bests: Vec<f64> = d["bests"].as_object().map_or(Vec::new(), |o| o.iter().filter(|(k, _)| k.split('|').nth(1) == Some(c.id)).filter_map(|(_, v)| v.as_f64()).collect());
            bests.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let m = DIFFS.iter().map(|(x, _)| d["medals"].get(format!("{}|{}", c.id, x.id())).and_then(|v| v.as_i64()).unwrap_or(0)).max().unwrap_or(0).max(0);
            let best = bests.first().filter(|x| **x != 0.0).map_or("--:--.--".to_string(), |x| time_text(*x));
            RecCourse { name: c.name.to_string(), best, medal: m, medal_name: name[m as usize].to_string() }
        }).collect();
        let badges = BADGES.iter().map(|(k, nm, how)| {
            let n = d["badges"].get(k).and_then(|x| x.as_i64()).unwrap_or(0);
            RecBadge { name: format!("{nm}{}", if n != 0 { format!(" ×{n}") } else { String::new() }), how: how.to_string(), got: n != 0 }
        }).collect();
        let note = format!("{} レース・{} 勝。記録はこの端末に残ります（端末を変えると共有されません）", self.save.num("races"), self.save.num("wins"));
        let hall = self.save.hall().iter().map(|e| RecHall {
            name: self.heroes.iter().find(|h| h.id == e.hero).map_or(e.hero.clone(), |h| h.name.clone()), title: e.title.clone(),
            line: format!("{} 戦 {} 勝・{}", e.places.len(), e.wins, money(e.prize)), levels: e.levels,
        }).collect();
        self.ui.show_records(RecordsData { courses, badges, hall, note });
        self.audio.select();
    }

    // ---- 自己ベストの影: 足もとの光の輪。前のベストの自分がいま走っている所 ----
    fn ghost_update(&mut self, dt: f64) {
        let hide = |kit: &mut Kit, a: NodeId, b: NodeId| { kit.scene[a].visible = false; kit.scene[b].visible = false; };
        let (gr, gt) = (self.ghost_ring, self.ghost_tag);
        let Some(pi) = self.race.player_gate else { hide(&mut self.kit, gr, gt); return };
        if self.race.t < 0.0 { hide(&mut self.kit, gr, gt); return; }
        // 撮影用（?video）は自己ベストの影を出さない（記録は続ける）
        self.ghost_clock += dt;
        let (pd, pdd, finished) = { let p = &self.race.runners[pi]; (p.dist, p.d, p.finished) };
        if self.ghost_clock >= GHOST_DT && !finished {
            self.ghost_clock -= GHOST_DT;
            self.ghost_run.push(sim_math::round(pd * 10.0) / 10.0);
            self.ghost_run.push(sim_math::round(pdd * 10.0) / 10.0);
        }
        let Some(b) = &self.ghost_best else { hide(&mut self.kit, gr, gt); return };
        if b.len() < 4 { hide(&mut self.kit, gr, gt); return; }
        let u = self.race.t / GHOST_DT;
        let i = u.floor() as usize * 2;
        if i + 3 >= b.len() { hide(&mut self.kit, gr, gt); return; }
        let f = u - u.floor();
        let (dist, d) = (b[i] + (b[i + 2] - b[i]) * f, b[i + 1] + (b[i + 3] - b[i + 1]) * f);
        let gap = dist - pd;
        // 撮影用（?video）は自己ベストの影を出さない（記録は続ける）
        let on = gap.abs() < 45.0 && gap > -9.0 && !self.ui.hud.video;
        self.kit.scene[gr].visible = on;
        self.kit.scene[gt].visible = on;
        if on {
            let tr = &self.race.track;
            let p = tr.pos(tr.s_at(dist), d);
            self.kit.scene[gr].position = Vec3::new(p.x as f32, (p.y + 0.06) as f32, p.z as f32);
            self.kit.scene[gt].position = Vec3::new(p.x as f32, (p.y + 1.0) as f32, p.z as f32);
            let op = 0.45 + 0.35 * (self.race.t * 3.0).sin().abs();
            self.kit.mat_mut(self.ghost_mat).opacity = op as f32;
        }
    }
    // ---- 1 周目の通過: 自己ベストのときとの差 ----
    fn split_check(&mut self) {
        let Some(p) = self.race.player() else { return };
        if self.career.is_some() || self.split_done || p.dist < self.race.track.def.start_back + self.race.track.lap { return; }
        self.split_done = true;
        self.lap_split = self.race.t;
        let bk = self.save.best_key(&self.heroes[self.hero_i].id, &self.course_id);
        if let Some(prev) = self.save.split(&bk).filter(|x| *x != 0.0) {
            let dt = self.race.t - prev;
            self.ui.hud.tip(&format!("1周通過 <b>{}{}</b> 秒（自己ベストのとき）", if dt <= 0.0 { "−" } else { "+" }, to_fixed(dt.abs(), 2)), 2.6);
        }
    }
    // ---- 実況の一言（状況から）----
    fn commentary(&mut self) {
        let lead = &self.race.runners[self.race.order[0]];
        let (lead_i, lead_d) = (lead.i, lead.dist);
        let len = self.race.track.race_len;
        let rem = len - lead_d;
        if !self.called_half && lead_d > len / 2.0 {
            self.called_half = true;
            let n = self.name_of(lead_i);
            self.say(&format!("半分を過ぎて、先頭は{n}"), 1.5, 2.4);
        }
        if !self.called_close && rem < 170.0 {
            let b = &self.race.runners[self.race.order[1]];
            let (bi, bd) = (b.i, b.dist);
            if lead_d - bd < 1.6 {
                self.called_close = true;
                let (x, y) = (self.name_of(lead_i), self.name_of(bi));
                self.say(&format!("{x}と{y}、並んだ！"), 0.0, 2.6);
            }
        }
    }
    fn once(&mut self, id: &str, html: &str, sec: f64) { if self.tips_shown.contains(id) { return; } self.tips_shown.insert(id.to_string()); self.ui.hud.tip(html, sec); }
    // ---- 案内（最初のレースの場面ごとに一言。読むより走る）----
    fn coach(&mut self) {
        let Some(pi) = self.race.player_gate else { return };
        if self.race.t < 0.0 { return; }
        let p = self.race.runners[pi].clone();
        let dev = self.device();
        let kbd = |c: Rebindable| format!(r#"<span class="kbd">{}</span>"#, key_name(&self.keys.key_of(c)));
        let lk = match dev { Device::Touch => "スティック".to_string(), Device::Pad => "左スティック".to_string(), Device::Keys => format!("{}{}", kbd(Rebindable::Left), kbd(Rebindable::Right)) };
        let dk = match dev { Device::Touch => "「ダッシュ」".to_string(), Device::Pad => r#"<span class="kbd">A</span>"#.to_string(), Device::Keys => kbd(Rebindable::Dash) };
        let jk = match dev { Device::Touch => "「ジャンプ」".to_string(), Device::Pad => r#"<span class="kbd">X</span>"#.to_string(), Device::Keys => kbd(Rebindable::Jump) };
        let t = self.race.t;
        if self.first_race {
            if t > 0.4 { self.once("dash", &format!("{dk}を押している間 ダッシュ"), 3.0); }
            if t > 5.0 && p.vd.abs() < 1.0 { self.once("lane", &format!("{lk}で進路を変える"), 3.0); }
        }
        if p.blocked_for > 0.45 { self.once("blocked", &format!("前が詰まった！ {lk}で一人分ずれる"), 2.6); }
        if self.first_race && p.stamina < 45.0 && p.dashing { self.once("rest", "離すと少しあとから回復する", 2.6); }
        if self.first_race && p.draft && p.draft_for > 1.0 && !p.dashing { self.once("draft", "前の人の後ろは<b>風よけ</b>。回復が速い", 3.0); }
        // マークと武器: はじめて近づいたとき（最初の何レースか）
        if !self.save.data["firsts"]["items"].as_bool().unwrap_or(false) && self.race.pickups.iter().any(|k| !k.taken && k.dist > p.dist && k.dist - p.dist < 34.0) {
            self.once("marks", "<b>丸い札</b>で能力アップ。<b>金の札</b>は武器", 3.4);
        }
        if p.weapon.is_some() && p.weapon_t < 9.0 && p.stats.hits_dealt == 0 && !self.save.data["firsts"]["items"].as_bool().unwrap_or(false) { let ak = self.attack_label(); self.once("attack", &format!("{ak}で攻撃。相手が<b>近いとき</b>に"), 2.6); }
        if let Some(next) = self.race.obstacle_list.iter().find(|e| e.dist > p.dist).copied() && next.dist - p.dist < 42.0 && p.d > next.ob.d0 && p.d < next.ob.d1 {
            let hurdle = next.ob.kind == ObstacleKind::Hurdle;
            self.once(if hurdle { "ob-hurdle" } else { "ob-water" }, &if hurdle { format!("ハードル！ {jk}で跳ぶか、空いた所へ") } else { format!("水たまり！ {jk}で跳ぶか、水のない所へ") }, 2.4);
        }
    }

    fn look(&mut self, target: DVec3) {
        self.cam_prev_q = self.kit.camera.quaternion;
        self.kit.camera.look_at(f3(target));
        if let Some((p, q)) = self.cam_at { self.kit.camera.position = f3(p); self.cam_prev_q = self.kit.camera.quaternion; self.kit.camera.look_at(f3(q)); }
    }

    // ---- カメラ: 後方斜め上から。自分は画面の下寄り、カーブの少し先を見る。急に振らない ----
    fn race_camera(&mut self, dt: f64, now: f64) {
        let p = self.race.player().unwrap().clone();
        let (back, up, ahead) = (if self.race.t < 1.5 { 8.0 } else { 5.3 }, if self.race.t < 1.5 { 4.8 } else { 3.3 }, 19.0);
        // ダッシュと抜け出しで、カメラが少し置いていかれる（画角は変えない）
        let ex = |k: f64| 1.0 - sim_math::exp(-dt * k);
        self.pull += ((p.dash_k * 1.05 + if p.slip_burst > 0.0 { 0.55 } else { 0.0 }) - self.pull) * ex(2.2);
        let target_s = p.dist - back - self.pull;
        if dt > 0.0 { self.cam_s += (target_s - self.cam_s) * ex(6.5); }
        let tr = &self.race.track;
        self.cam_d += (p.d * 0.82 + tr.width / 2.0 * 0.18 - self.cam_d) * ex(3.4);
        self.look_d += (p.d * 0.55 + tr.width / 2.0 * 0.45 - self.look_d) * ex(2.6);
        let s_cam = tr.s_at(self.cam_s);
        let c = tr.pos(s_cam, self.cam_d);
        let ground_y = tr.height(s_cam);
        self.cam_y += (ground_y - self.cam_y) * ex(5.0);
        self.cam_up += (up - self.cam_up) * ex(1.6);
        self.cam_pos = DVec3::new(c.x, self.cam_y + self.cam_up, c.z);
        let l = tr.pos(tr.s_at(p.dist + ahead), self.look_d);
        self.cam_look = DVec3::new(l.x, tr.height(tr.s_at(p.dist + ahead)) * 0.8 + self.cam_y * 0.2 + 0.05, l.z);
        // 自分が飛ばされている間は、見上げて追う
        if p.launch > 0.0 { self.cam_pos.y += p.y * 0.55; self.cam_look.y += p.y * 0.9; }
        let s = self.trauma * self.trauma;
        let mut pos = self.cam_pos;
        let mut look = self.cam_look;
        // ためた一撃が当たった: 走路の横の低い所から、当てた走者と相手を一緒に見る（入りは速く、戻りはなめらか）
        if self.hit_cam > 0.0 {
            let k = 1.0 - self.hit_cam / self.hit_cam_len;
            let w = if k < 0.12 { k / 0.12 } else if k > 0.7 { (1.0 - k) / 0.3 } else { 1.0 };
            let w = w * w * (3.0 - 2.0 * w);
            let (a, b) = (&self.race.runners[self.hit_pair.0], &self.race.runners[self.hit_pair.1]);
            let mid_s = (a.s + b.s) / 2.0;
            let mid_d = (a.d + b.d) / 2.0;
            let side = if mid_d > tr.width / 2.0 { -1.0 } else { 1.0 };
            let c = tr.pos(mid_s + 1.2, (mid_d + side * 5.6).clamp(-1.8, tr.width + 1.8));
            let l = tr.pos(mid_s, mid_d);
            // 飛ばされた相手を追って見上げる
            let up = a.y.max(b.y);
            let sp = DVec3::new(c.x, c.y + 1.45 + up * 0.35, c.z);
            let sl = DVec3::new(l.x, l.y + 1.0 + up * 0.8, l.z);
            pos = pos + (sp - pos) * w;
            look = look + (sl - look) * w;
        }
        pos.x += (now * 61.0).sin() * s * 0.2;
        pos.y += (now * 53.0).sin() * s * 0.16;
        self.kit.camera.position = f3(pos);
        self.look(look);
        self.trauma = (self.trauma - dt * 1.9).max(0.0);
    }
    fn finish_camera(&mut self, dt: f64) {
        let p = self.race.player().unwrap().clone();
        let tr = &self.race.track;
        let k = (self.finish_timer / 0.85).min(1.0);
        let e = k * k * (3.0 - 2.0 * k);
        // 走路の中で、後ろ → 正面へ回り込む（柵の外へ出ない）。顔を見せる
        let fwd = -5.3 + e * 9.6;
        // 内柵に寄っているときは外を回る（体を突き抜けない）
        let room = p.d > 4.4;
        let side = (if room { -1.0 } else { 1.0 }) * (e * PI).sin() * 3.6 - e * if room { 2.0 } else { 0.0 };
        let a = tr.pos(p.s + fwd, (p.d + side).min(tr.width - 0.7).max(0.7));
        lerp3(&mut self.cam_pos, DVec3::new(a.x, a.y + 3.3 - e * 2.0, a.z), (dt * 7.0).min(1.0));
        let b = tr.pos(p.s, p.d);
        lerp3(&mut self.cam_look, DVec3::new(b.x, b.y + 1.2, b.z), (dt * 6.0).min(1.0));
        self.kit.camera.position = f3(self.cam_pos);
        self.look(self.cam_look);
    }
    /// タイトルの見本: 競技場の全景と、走る人たちの寄りを切り替える
    fn title_camera(&mut self, dt: f64, now: f64) {
        self.shot_t += dt;
        // 開いてすぐ、魚たちが正面から泳いでくる所を見せる。
        // 並び: 正面の低い所 → 真横で並走 → 外側の斜め上 → 島の全景（全景は最後）
        const KINDS: [usize; 4] = [2, 1, 3, 0];
        const DURS: [f64; 4] = [6.0, 5.5, 5.0, 6.5];
        if self.shot_t > DURS[self.shot % 4] { self.shot_t = 0.0; self.shot += 1; self.go_flash = 0.35; }
        let tr = &self.race.track;
        // 追う相手: 集団の前の方（2〜5 番手）の魚かイカ。いなければ 3 番手
        let order = &self.race.order;
        let creature = |i: usize| self.lineup.get(i).is_some_and(|h| matches!(self.heroes[*h].body, Body::Fish(_) | Body::Squid(_)));
        let pick = order.iter().skip(1).take(4).copied().find(|i| creature(*i)).unwrap_or(order[2.min(order.len() - 1)]);
        let focus = &self.race.runners[pick];
        let kind = KINDS[self.shot % 4];
        let st = self.shot_t;
        match kind {
            0 => {
                // 島の外から: 雲の上に浮かぶ競馬場の全景
                let (mut cx, mut cz) = (0.0, 0.0);
                let mut i = 0;
                while i < tr.n { cx += tr.x[i]; cz += tr.z[i]; i += 8; }
                cx /= tr.n as f64 / 8.0;
                cz /= tr.n as f64 / 8.0;
                let a = 0.55 + now * 0.012;
                self.cam_pos = DVec3::new(cx + a.sin() * 330.0, 26.0 + st * 1.2, cz + a.cos() * 300.0);
                self.cam_look = DVec3::new(cx, 2.0, cz);
            }
            1 => {
                // 真横より少し前から、走る人たちと並走
                let a = tr.pos(focus.s + 3.2 + st * 0.5, 0.75);
                self.cam_pos = DVec3::new(a.x, a.y + 1.0, a.z);
                let b = tr.pos(focus.s, tr.width * 0.4);
                self.cam_look = DVec3::new(b.x, b.y + 1.05, b.z);
            }
            2 => {
                // 正面の低い所から、集団の顔を見る
                let a = tr.pos(focus.s + 8.5 - st * 0.3, focus.d);
                self.cam_pos = DVec3::new(a.x, a.y + 0.75, a.z);
                let b = tr.pos(focus.s - 2.0, focus.d);
                self.cam_look = DVec3::new(b.x, b.y + 1.15, b.z);
            }
            _ => {
                // 外側の斜め上から、集団の形とスタンドを見る
                let a = tr.pos(focus.s - 10.0, tr.width + 2.0);
                self.cam_pos = DVec3::new(a.x, a.y + 7.5, a.z);
                let b = tr.pos(focus.s + 2.0, tr.width / 2.0);
                self.cam_look = DVec3::new(b.x, b.y + 0.6, b.z);
            }
        }
        self.kit.camera.position = f3(self.cam_pos);
        self.look(self.cam_look);
        if dt > 0.0 { self.cheer_target = 0.35; }
    }
    /// 表彰台の前の位置: stage 上の x（台の並びの向き）・z（カメラの側）を世界へ
    fn stage_point(&self, x: f64, z: f64) -> P3 {
        let c = &self.courses[&self.cur_course];
        let yaw = c.podium_yaw;
        let o = c.podium_pos;
        P3 { x: o.x as f64 + yaw.cos() * x + yaw.sin() * z, z: o.z as f64 - yaw.sin() * x + yaw.cos() * z, y: o.y as f64 }
    }
    fn select_camera(&mut self, dt: f64, now: f64) {
        let id = self.heroes[self.hero_i].id.clone();
        let (s0, d0) = { let c = &self.courses[&self.cur_course]; (c.select_s, c.stage_d) };
        let tr = &self.race.track;
        let p = tr.pos(s0, d0);
        let yaw = tr.yaw(s0) - PI / 2.0;
        // 選んだら少し走って見せてから、手を振る（正面と走り姿。企画 07）
        let run_prev = if now - self.picked_at < 1.9 { 9.0 } else { 0.0 };
        if let Some(v) = self.views.get_mut(&id) && v.loaded(&self.kit) {
            v.pose(&mut self.kit, p.x, p.y + 0.19, p.z, yaw + 0.22 + (now * 0.5).sin() * 0.08, dt, now, run_prev, true);
            if run_prev == 0.0 && now - self.last_greet > 7.0 && !v.has_current(&self.kit) { self.last_greet = now; v.gesture(&mut self.kit, Gesture::Greet); }
        }
        // 外側の低い所から、空と内馬場を背に。主役は画面の右寄り
        let tr = &self.race.track;
        let a = tr.pos(s0 - 1.05, d0 + 3.9);
        lerp3(&mut self.cam_pos, DVec3::new(a.x, a.y + 0.72, a.z), if self.select_snap { 1.0 } else { (dt * 3.5).min(1.0) });
        let b = tr.pos(s0 - 1.05, d0);
        lerp3(&mut self.cam_look, DVec3::new(b.x, b.y + 1.24, b.z), if self.select_snap { 1.0 } else { (dt * 5.0).min(1.0) });
        self.select_snap = false;
        self.kit.camera.position = f3(self.cam_pos);
        self.look(self.cam_look);
    }
    /// 結果: 表彰台に 3 人。プレイヤーが入れなかったら手前で座り込む
    fn podium_scene(&mut self, dt: f64, now: f64) {
        let yaw = self.courses[&self.cur_course].podium_yaw;
        const X: [f64; 3] = [0.0, 1.62, -1.62];
        const Z: [f64; 3] = [0.0, -0.55, -0.55];
        const H: [f64; 3] = [0.95, 0.62, 0.4];
        self.cheer_clock -= dt;
        let gest = self.cheer_clock <= 0.0;
        if gest { self.cheer_clock = 2.6; }
        // カップの最後は総合順位で表彰する
        let cup_order: Option<Vec<String>> = self.cup.as_ref().filter(|c| c.index >= self.courses_defs.len() - 1 && !c.rows.is_empty()).map(|c| standings(&c.rows).into_iter().map(|x| x.id).collect());
        for i in 0..self.race.runners.len() {
            let (place, is_player) = (self.race.runners[i].place, self.race.runners[i].is_player);
            let Some(hi) = self.lineup.get(i).copied() else { continue };
            let id = self.heroes[hi].id.clone();
            if !self.views.get(&id).is_some_and(|v| v.loaded(&self.kit)) { continue; }
            let podium = match &cup_order { Some(o) => o.iter().position(|x| *x == id).map_or(0, |x| x as i32 + 1), None => place };
            if (1..=3).contains(&podium) {
                let k = podium as usize - 1;
                let p = self.stage_point(X[k], Z[k]);
                let v = self.views.get_mut(&id).unwrap();
                v.pose(&mut self.kit, p.x, p.y + H[k] + 0.08, p.z, yaw + if podium == 2 { -0.12 } else if podium == 3 { 0.12 } else { 0.0 }, dt, now, 0.0, false);
                v.set_mood(&mut self.kit, Mood::Joy);
                if gest && !v.has_current(&self.kit) { v.gesture(&mut self.kit, if podium == 1 || is_player { Gesture::Banzai } else { Gesture::Greet }); }
            } else if is_player {
                // 手前で座り込む（台の数字を隠さない所）
                let p = self.stage_point(0.95, 2.75);
                let v = self.views.get_mut(&id).unwrap();
                v.pose(&mut self.kit, p.x, p.y, p.z, yaw - 0.2, dt, now, 0.0, false);
                v.set_mood(&mut self.kit, Mood::Sad);
                if !v.has_current(&self.kit) { v.gesture(&mut self.kit, Gesture::Rest); }
            } else { self.views[&id].hide(&mut self.kit); }
        }
        let a = self.stage_point(0.0, 6.4);
        lerp3(&mut self.cam_pos, DVec3::new(a.x, a.y + 1.55, a.z), (dt * 2.5).min(1.0));
        let b = self.stage_point(0.0, 0.0);
        lerp3(&mut self.cam_look, DVec3::new(b.x, b.y + 1.5, b.z), (dt * 3.0).min(1.0));
        self.kit.camera.position = f3(self.cam_pos);
        self.look(self.cam_look);
    }
    fn intro_camera(&mut self) {
        // 島の外の空から、雲の上に浮かぶ競馬場を見下ろしてゲートの正面へ急降下する（INTRO_FLY 秒）。
        // そのあと、ゲートの正面の低い所から全員の顔を見て、持ち上がって後ろへ回り、自分の枠の後ろへ降りる
        let pd = self.race.player().unwrap().d;
        let tr = &self.race.track;
        if self.intro_t < INTRO_FLY {
            let a = self.intro_t / INTRO_FLY;
            let e = a * a * (3.0 - 2.0 * a);
            let (mut cx, mut cz) = (0.0, 0.0);
            let mut i = 0;
            while i < tr.n { cx += tr.x[i]; cz += tr.z[i]; i += 8; }
            cx /= tr.n as f64 / 8.0;
            cz /= tr.n as f64 / 8.0;
            let gate = tr.pos(tr.start_s + 7.2, tr.width / 2.0);
            let out = DVec3::new(gate.x - cx, 0.0, gate.z - cz).normalize_or_zero();
            // スタンドの裏を通らないよう、島の向こう側の空から内馬場の上を越えてくる
            let far = DVec3::new(cx, gate.y, cz) - out * 330.0 + DVec3::new(0.0, 70.0, 0.0);
            let near = DVec3::new(gate.x, gate.y + 1.05, gate.z);
            // 高さは早く下がりすぎないよう、最後に一気に降りる
            let mut p = far + (near - far) * e;
            p.y = far.y + (near.y - far.y) * (a * a * a * (a * (a * 6.0 - 15.0) + 10.0));
            self.cam_pos = p;
            let q = tr.pos(tr.start_s, tr.width / 2.0);
            let look_far = DVec3::new(gate.x, gate.y - 4.0, gate.z);
            let look_near = DVec3::new(q.x, q.y + 1.35, q.z);
            self.cam_look = look_far + (look_near - look_far) * e;
            self.kit.camera.position = f3(self.cam_pos);
            self.look(self.cam_look);
            return;
        }
        let k = ((self.intro_t - INTRO_FLY) / (INTRO_S - INTRO_FLY)).min(1.0);
        if k < 0.45 {
            let a = k / 0.45;
            let p = tr.pos(tr.start_s + 7.2 - a * 1.2, tr.width / 2.0 + (pd - tr.width / 2.0) * 0.35 * a);
            self.cam_pos = DVec3::new(p.x, p.y + 1.05, p.z);
            let q = tr.pos(tr.start_s, tr.width / 2.0 + (pd - tr.width / 2.0) * 0.45 * a);
            self.cam_look = DVec3::new(q.x, q.y + 1.35, q.z);
        } else {
            let a = (k - 0.45) / 0.55;
            let e = a * a * (3.0 - 2.0 * a);
            let p = tr.pos(tr.start_s + 6.0 - e * 14.0, tr.width / 2.0 + (pd * 0.82 + tr.width * 0.09 - tr.width / 2.0) * (0.35 + e * 0.65));
            self.cam_pos = DVec3::new(p.x, p.y + 1.05 + e * 3.75 + (e * PI).sin() * 5.5, p.z);
            let q = tr.pos(tr.start_s + e * 19.0, pd * e + tr.width / 2.0 * (1.0 - e));
            self.cam_look = DVec3::new(q.x, q.y + 1.3 - e * 1.25, q.z);
        }
        self.kit.camera.position = f3(self.cam_pos);
        self.look(self.cam_look);
    }

    // ---- 掲示板（内馬場の大画面）----
    fn draw_board(&mut self) {
        let tex = self.course().board.tex;
        let (w, h) = self.kit.tex_size(tex);
        let (w, h) = (w as f64, h as f64);
        let race = &self.race;
        let (lineup, heroes) = (&self.lineup, &self.heroes);
        self.kit.redraw_canvas(tex, |g| {
            g.set_fill_style_str("#071410");
            g.fill_rect(0.0, 0.0, w, h);
            g.set_fill_style_str("#ffcf4d");
            g.set_font("700 40px Fredoka, sans-serif");
            g.set_text_baseline("middle");
            g.set_text_align("center");
            let _ = g.fill_text(&time_text(race.t.max(0.0)), w / 2.0, 34.0);
            g.set_text_align("left");
            for (k, &i) in race.order.iter().enumerate() {
                let y = 66.0 + k as f64 * 55.0;
                let r = &race.runners[i];
                g.set_fill_style_str(GATE_COLOR[r.gate - 1]);
                g.fill_rect(18.0, y, 48.0, 48.0);
                g.set_fill_style_str(GATE_TEXT[r.gate - 1]);
                g.set_font("700 38px Fredoka, sans-serif");
                g.set_text_align("center");
                let _ = g.fill_text(&r.gate.to_string(), 42.0, y + 26.0);
                g.set_text_align("left");
                g.set_fill_style_str(if r.is_player { "#ffcf4d" } else { "#fff6e4" });
                g.set_font("700 34px 'Zen Maru Gothic', sans-serif");
                let name = lineup.get(i).map_or("", |h| heroes[*h].name.as_str());
                let _ = g.fill_text(&format!("{} {name}", k + 1), 78.0, y + 26.0);
            }
        });
    }
    // 大画面の映像: 走る人を前から映す（ホームストレッチで自分の顔が見える）
    fn draw_live(&mut self) {
        let f = match self.race.player() { Some(p) => p.clone(), None => self.race.runners[self.race.order[0]].clone() };
        let cp = self.kit.camera.position;
        let (bx, bz) = self.course().board.pos;
        let d = sim_math::hypot(cp.x as f64 - bx, cp.z as f64 - bz);
        let low = self.save.s.low;
        let kit = &mut self.kit;
        let c = self.courses.get_mut(&self.cur_course).unwrap();
        if d > 190.0 || low { c.set_live(kit, Color::css("#1a2a26"), None); return; }
        c.set_live(kit, Color::WHITE, None);
        self.live_frame += 1;
        if self.live_frame % 3 != 0 { return; }
        let tr = &self.race.track;
        // 胸から上を、少し下から
        let a = tr.pos(f.s + 3.6, f.d - 0.3);
        let mut cam = Camera::new(28.0, 16.6 / 14.8, 1.0, 240.0);
        cam.position = Vec3::new(a.x as f32, (a.y + f.y + 1.25) as f32, a.z as f32);
        let b = tr.pos(f.s, f.d);
        cam.look_at(Vec3::new(b.x as f32, (b.y + f.y + 1.55) as f32, b.z as f32));
        let hide = [self.ring, self.me_mark, self.slip_ring];
        let vis: Vec<bool> = hide.iter().map(|o| self.kit.scene[*o].visible).collect();
        for o in hide { self.kit.scene[o].visible = false; }
        let live = self.course().board.live;
        self.kit.scene[live].visible = false;
        self.kit.render_to(self.live_rt, &cam);
        self.kit.scene[live].visible = true;
        for (o, v) in hide.iter().zip(vis) { self.kit.scene[*o].visible = v; }
        let rt = self.live_rt;
        let kit = &mut self.kit;
        self.courses.get_mut(&self.cur_course).unwrap().set_live(kit, Color::WHITE, Some(rt));
    }

    // ---- 主ループ ----
    pub fn tick(&mut self, fixed: Option<f64>, draw: bool) {
        let now_ms = raf::now_ms();
        self.last_tick = now_ms;
        self.ui.set_now(now_ms / 1000.0);
        // 画面のボタン: いま出ている面で押した物を、このコマの計算の前に効かせる（今の版の click はコマの間に効く）
        self.pointer_now();
        let scale = self.debug_get("scale").as_f64().unwrap_or(1.0);
        let dt = fixed.unwrap_or_else(|| ((now_ms - self.last) / 1000.0).min(0.05) * scale);
        self.last = now_ms;
        self.t_all += dt;
        self.kit.adapt((dt * 1000.0) as f32);
        let want_fov = if self.kit.camera.aspect < 1.0 { 74.0 } else { 50.0 };
        if self.kit.camera.fov != want_fov { self.kit.camera.fov = want_fov; }
        let frame = self.input.read();
        let pad = self.pointer.read_pad();
        let c: Controls = controls::read(&frame, &self.keys, &pad, self.seen.kind == Kind::Touch);
        // 最後に触った機器: 画面を押した指かマウス（pointer_now）→ レース中の指 → パッド → キー の順に見て、後の物が勝つ
        if c.touch_used { self.used(Used::Touch); }
        if c.pad_used { self.used(Used::Pad); }
        if c.key_used { self.used(Used::Key); }
        { let s = &mut self.ctl_seen; if c.x.abs() > s.x.abs() { s.x = c.x; } if c.y.abs() > s.y.abs() { s.y = c.y; } s.act_dash |= c.act_dash; s.hold_dash |= c.hold_dash; s.act_jump |= c.act_jump; s.hold_jump |= c.hold_jump; s.confirm_pressed |= c.confirm_pressed; }
        let mut sim_input = Input::NONE;
        if self.ui.records_shown() {
            if c.confirm_pressed || c.back_pressed { self.ui.close_records(); }
        } else {
            match self.mode {
                Mode::Title => { if c.confirm_pressed { self.audio.init(); self.wipe(|a| a.to_select()); } }
                Mode::Select => {
                    if c.left_pressed { self.pick_hero(self.hero_i as i64 - 1); }
                    if c.right_pressed { self.pick_hero(self.hero_i as i64 + 1); }
                    if (c.up_pressed || c.down_pressed) && !self.is_career_mode {
                        let i = self.courses_defs.iter().position(|x| x.id == self.course_id).unwrap_or(0);
                        let n = self.courses_defs.len();
                        let id = self.courses_defs[(i + if c.down_pressed { 1 } else { n - 1 }) % n].id;
                        self.pick_course(id);
                    }
                    if c.confirm_pressed && self.mode == Mode::Select { self.go(); }
                    if c.back_pressed { self.to_title(); }
                }
                Mode::Intro => {
                    if c.pause_pressed { self.set_paused(true); }
                    self.intro_t += dt;
                    if self.intro_t >= INTRO_S || c.confirm_pressed { self.mode = Mode::Race; self.ui.show(None); self.ui.set_racing(true); }
                }
                Mode::Race => {
                    if c.pause_pressed { self.set_paused(true); }
                    sim_input = Input { x: c.x, dash: c.hold_dash, jump: c.act_jump, attack: c.hold_attack || c.act_attack };
                    if let Some(inp) = self.debug_input.as_mut() { sim_input = *inp; inp.jump = false; }
                    if let Some(f) = self.debug_policy.clone() { sim_input = crate::game_api::run_policy(&f, dt, self); }
                    self.coach();
                    self.commentary();
                    self.split_check();
                    self.ghost_update(dt);
                }
                Mode::Finish => {
                    self.finish_timer += dt;
                    if self.photo.is_some() && self.finish_timer > 1.1 { self.show_photo(); }
                    else if self.finish_timer > 2.9 || (self.finish_timer > 0.6 && c.confirm_pressed) { self.to_result(); }
                }
                Mode::Photo => {
                    self.photo_t += dt;
                    if self.photo_t > 1.15 && self.ui.photo_res.is_empty() {
                        let (w, m) = { let f = self.photo.as_ref().unwrap(); (f.win, f.dt) };
                        self.ui.photo_res = format!("{}番 {}、{}差！", self.race.runners[w].gate, self.name_of(w), margin_text(m));
                        self.audio.nice();
                    }
                    if self.photo_t > 3.1 || (self.photo_t > 1.3 && c.confirm_pressed) { self.photo = None; self.to_result(); }
                }
                Mode::Result => {
                    // 上下左右で「もう一度」「コース・キャラを変える」を移り、決定で押す。出てから 0.6 秒は決定を受けない（ゴール前の連打で飛ばさない）
                    self.result_t += dt;
                    self.cheer_tick(dt);
                    self.ui.res_focus = step(self.ui.res_focus, 2, c.up_pressed || c.left_pressed, c.down_pressed || c.right_pressed);
                    if c.confirm_pressed && result_accepts(self.result_t) { if self.ui.res_focus == 0 { self.again(); } else { self.act(Act::Menu); } }
                    else if c.back_pressed { self.to_select(); }
                }
                Mode::Paused => {
                    if c.pause_pressed || c.back_pressed { self.set_paused(false); }
                    else {
                        // 上下左右で項目を移り、Enter・Space・パッドの A で押す
                        let run = self.ui.pause.nav(c.up_pressed || c.left_pressed, c.down_pressed || c.right_pressed, c.menu_confirm);
                        if let Some(a) = run { self.act(a); }
                    }
                }
                Mode::Loading => {}
            }
        }
        let mode = self.mode;
        let running = matches!(mode, Mode::Title | Mode::Race | Mode::Finish | Mode::Result | Mode::Photo);
        // ゴールの瞬間のあとだけ、少しゆっくり（操作を奪わない）
        let mut sim_dt = if mode == Mode::Finish && self.finish_timer < 1.2 { dt * (0.25 + (self.finish_timer / 1.2) * 0.75) } else { dt };
        // 当てた・当てられた瞬間は、ほんの少し止まる（見せ場。操作は受け続ける）
        if self.hitstop > 0.0 && mode == Mode::Race { self.hitstop -= dt; sim_dt *= 0.06; }
        // 横から見せている間はゆっくり（止まりのあと、徐々に戻る）
        else if self.hit_cam > 0.0 && mode == Mode::Race { sim_dt *= 0.3 + 0.7 * (1.0 - self.hit_cam / self.hit_cam_len).powi(2); }
        if mode == Mode::Race { self.hit_cam = (self.hit_cam - dt).max(0.0); }
        if running && !(mode == Mode::Result && self.race.done) { self.race.step(sim_dt, sim_input); }
        // TS は出来事ごとに mode を読む（ゴールの出来事で mode が変わると、次の出来事から live が変わる）
        let evs = std::mem::take(&mut self.race.events);
        for e in evs {
            let l = matches!(self.mode, Mode::Race | Mode::Finish);
            self.handle(e, l);
        }
        self.race.events.clear();
        if matches!(self.mode, Mode::Title) && self.race.done {
            self.race_seed += 1;
            self.setup_race(None, self.race_seed);
            let _ = self.load_lineup(None);
            self.race.t = 0.0;
            self.race.started = true;
            let kit = &mut self.kit;
            self.courses.get_mut(&self.cur_course).unwrap().open_gate(kit, 1.0);
        }

        // 見た目
        self.kit.shared.time = self.t_all as f32;
        let mode = self.mode;
        let anim = if mode == Mode::Paused { 0.0 } else { dt };
        {
            let kit = &mut self.kit;
            let c = self.courses.get_mut(&self.cur_course).unwrap();
            if c.gate_open > 0.0 && c.gate_open < 1.0 { let k = (c.gate_open + anim * 2.8).min(1.0); c.open_gate(kit, k); }
            if mode != Mode::Select && mode != Mode::Result { c.lift_gate(kit, self.race.t); }
            let pl = self.race.player();
            c.show_rem_boards(kit, matches!(mode, Mode::Title) || pl.is_none() || pl.is_some_and(|p| self.race.track.race_len - p.dist < self.race.track.lap - 5.0));
            c.update_sky(kit, self.t_all);
        }
        let racing = matches!(mode, Mode::Race | Mode::Finish | Mode::Intro | Mode::Paused);
        let in_race_view = racing || matches!(mode, Mode::Title | Mode::Photo);
        if in_race_view {
            let t_all = self.t_all;
            let dirt = self.th().dirt;
            for i in 0..self.race.runners.len() {
                let Some(hi) = self.lineup.get(i).copied() else { continue };
                let id = self.heroes[hi].id.clone();
                let Some(v) = self.views.get_mut(&id) else { continue };
                if !v.loaded(&self.kit) { continue; }
                let r = &self.race.runners[i];
                let tag = mode == Mode::Race && !r.is_player && !self.ui.hud.video && self.race.player().is_none_or(|p| r.dist > p.dist - 1.2);
                v.update(&mut self.kit, r, &self.race.track, if mode == Mode::Finish { sim_dt } else { anim }, t_all, true, tag);
                // 蹴り上げる細かい粒（自分のまわりの走者だけ。ダッシュ中は多い）: 走っている足が見える
                let near_cam = self.race.player().is_none_or(|p| (r.dist - p.dist).abs() < 25.0);
                if anim > 0.0 && !r.air && r.speed > 5.0 && near_cam {
                    let p = self.race.track.pos(r.s, r.d);
                    let yaw = self.race.track.yaw(r.s);
                    let n = if r.dash_k > 0.5 { 1 } else if random() < 0.35 { 1 } else { 0 };
                    for _ in 0..n {
                        let (ox, oz) = ((random() - 0.5) * 0.35, (random() - 0.5) * 0.35);
                        let back = r.speed * (0.22 + random() * 0.18);
                        let c = Color::css(if random() < 0.3 { "#fff6e6" } else { "#e9d3ae" });
                        self.fx.emit((p.x + ox) as f32, (p.y + 0.08) as f32, (p.z + oz) as f32, (yaw.sin() * (r.speed - back)) as f32, (1.6 + random() * 1.8) as f32, (yaw.cos() * (r.speed - back)) as f32, c, Emit { life: (0.22 + random() * 0.1) as f32, size: (0.05 + random() * 0.04) as f32, grow: -0.2, a: 0.95, g: 10.0 });
                    }
                }
                // 足もとの砂
                if anim > 0.0 && !r.air && r.speed > 5.0 && random() < if r.dash_k > 0.5 { 0.42 } else { 0.16 } {
                    let p = self.race.track.pos(r.s, r.d);
                    let yaw = self.race.track.yaw(r.s);
                    let (x, z) = (p.x + (random() - 0.5) * 0.5, p.z + (random() - 0.5) * 0.5);
                    let vy = 0.5 + random() * 0.9;
                    let (dk, sp) = (r.dash_k, r.speed);
                    self.fx.emit(x as f32, (p.y + 0.1) as f32, z as f32, (-yaw.sin() * sp * 0.18) as f32, vy as f32, (-yaw.cos() * sp * 0.18) as f32, dirt, Emit { life: 0.42, size: if dk > 0.5 { 0.3 } else { 0.2 }, grow: 2.6, a: if dk > 0.5 { 0.42 } else { 0.3 }, g: 0.0 });
                }
            }
        }
        // マークと武器（基準は自分。見本のレースは先頭）。ためている走者からは気が立ちのぼる
        if in_race_view {
            let near = self.race.player().map_or(self.race.runners[self.race.order[0]].dist, |p| p.dist);
            self.items.update(&mut self.kit, &self.race, near, self.t_all);
            // ボーガンの矢: 走路を回る光の尾（金と白）
            if anim > 0.0 {
                let bolts: Vec<(f64, f64)> = self.race.shots.iter().filter(|s| s.kind == sim::items::ShotKind::Bolt).map(|s| (s.dist, s.d)).collect();
                for (dist, d) in bolts {
                    let tr = &self.race.track;
                    let s_ = tr.s_at(dist);
                    let yaw = tr.yaw(s_);
                    for k in 0..4 {
                        let p = tr.pos(tr.s_at(dist - k as f64 * 0.5), d);
                        let c = Color::css(if k == 0 { "#ffffff" } else { "#ffd24a" });
                        self.fx.emit(p.x as f32, (p.y + 1.4) as f32, p.z as f32, (-yaw.sin() * 3.0) as f32, 0.3, (-yaw.cos() * 3.0) as f32, c, Emit { life: 0.35, size: 0.32 - k as f32 * 0.05, grow: -0.7, a: 1.0, g: 0.0 });
                    }
                }
            }
            if anim > 0.0 {
                // 噴射の間: 体の後ろから炎の尾を引く（どの走者でも。噴射していると一目で分かる）
                for i in 0..self.race.runners.len() {
                    let r = &self.race.runners[i];
                    if r.jet <= 0.0 || r.finished { continue; }
                    let tr = &self.race.track;
                    let yaw = tr.yaw(r.s);
                    let back = tr.pos(tr.s_at(r.dist - 1.3), r.d);
                    for k in 0..3 {
                        let c = Color::css(["#ffffff", "#ffd24a", "#ff7a2e"][k]);
                        let (sx, sz) = ((random() - 0.5) * 0.5, (random() - 0.5) * 0.5);
                        let v = -(4.0 + random() * 4.0);
                        self.fx.emit((back.x + sx) as f32, (back.y + r.y + 0.9 + random() * 0.4) as f32, (back.z + sz) as f32, (yaw.sin() * v) as f32, (0.4 + random() * 0.8) as f32, (yaw.cos() * v) as f32, c, Emit { life: 0.42, size: 0.5 - k as f32 * 0.1, grow: -0.6, a: 1.0, g: -1.0 });
                    }
                }
                // ためている間: 足もとのまわりから光の粒が武器へ吸い込まれる（ためるほど多い）。満ちると体のまわりに光の柱が立つ
                let near_pl = self.race.player().map(|p| p.dist);
                for i in 0..self.race.runners.len() {
                    let r = &self.race.runners[i];
                    if r.charge <= 0.0 { continue; }
                    if near_pl.is_some_and(|d| (r.dist - d).abs() > 30.0) { continue; }
                    let tr = &self.race.track;
                    let p = tr.pos(r.s, r.d);
                    let w = tr.pos(r.s, (r.d + 0.75).min(tr.width - 0.1));
                    let yaw = tr.yaw(r.s);
                    let (fx, fz) = (yaw.sin() * r.speed, yaw.cos() * r.speed);
                    let full = r.charge >= 1.0;
                    let n = 1 + (r.charge * 3.0) as usize;
                    for _ in 0..n {
                        let (a, rad) = (random() * 6.28, 1.3 + random() * 0.9);
                        let (sx, sy, sz) = (p.x + a.cos() * rad, p.y + r.y + 0.1 + random() * 1.4, p.z + a.sin() * rad);
                        let life = 0.32;
                        let (tx, ty, tz) = (w.x, w.y + r.y + 1.55, w.z);
                        let c = Color::css(if full { "#fff6c8" } else { "#ffcf4d" });
                        self.fx.emit(sx as f32, sy as f32, sz as f32, ((tx - sx) / life + fx) as f32, ((ty - sy) / life) as f32, ((tz - sz) / life + fz) as f32, c, Emit { life: life as f32, size: 0.2, grow: -0.7, a: 1.0, g: 0.0 });
                    }
                    if full {
                        for _ in 0..3 {
                            let (a, rad) = (random() * 6.28, 0.45 + random() * 0.35);
                            let c = Color::css(if random() < 0.5 { "#ffd84a" } else { "#ff9a3c" });
                            self.fx.emit((p.x + a.cos() * rad) as f32, (p.y + r.y + 0.1) as f32, (p.z + a.sin() * rad) as f32, fx as f32, (5.5 + random() * 3.0) as f32, fz as f32, c, Emit { life: 0.42, size: 0.34, grow: -0.5, a: 0.9, g: -2.0 });
                        }
                    }
                }
            }
        } else { self.items.hide(&mut self.kit); }
        // カメラと自分の間にいる走者を透かす
        if mode == Mode::Race && self.race.player().is_some() {
            let me = self.race.player().unwrap().clone();
            for i in 0..self.race.runners.len() {
                if i == me.i { continue; }
                let Some(hi) = self.lineup.get(i).copied() else { continue };
                let Some(v) = self.views.get(&self.heroes[hi].id) else { continue };
                let r = &self.race.runners[i];
                // カメラから自分へ引いた線の近くにいて、自分より手前のときだけ。隣に並んだ相手は消さない
                let g = me.dist - r.dist;
                let line = self.cam_d + (me.d - self.cam_d) * (1.0 - g / 6.0).max(0.0);
                let l = (r.d - line).abs();
                let near = if g > 1.1 && g < 6.6 && l < 1.5 { (g - 1.1).min(1.0) * (1.0 - (l - 0.8).max(0.0) / 0.7) } else { 0.0 };
                v.set_fade(&mut self.kit, near * 0.6);
            }
            // 空中の小さな綿毛: 世界に止まっていて、カメラのそばを飛び去る（速さが見える。画角は変えない）
            if anim > 0.0 && me.speed > 5.0 {
                let chance = 0.5 + me.dash_k * 0.5;
                if random() < chance {
                    let ox = (if random() < 0.5 { -1.0 } else { 1.0 }) * (1.6 + random() * 3.6);
                    let p = self.race.track.pos(me.s + 3.0 + random() * 7.0, me.d + ox);
                    let c = Color::css(if self.th().stars > 0.4 { "#cfd6ff" } else { "#fffaf0" });
                    let y = p.y + 0.4 + random() * 2.8;
                    self.fx.emit(p.x as f32, y as f32, p.z as f32, 0.0, 0.04, 0.0, c, Emit { life: 0.85, size: 0.05, grow: 0.0, a: 0.85, g: 0.0 });
                }
            }
        } else { for v in self.views.values() { v.set_fade(&mut self.kit, 0.0); } }
        self.fx.update(anim);
        let h = self.kit.size().1;
        self.fx.set_height(&mut self.kit, h * 1.2);

        // 歓声
        self.cheer_now += (self.cheer_target - self.cheer_now) * (dt * 0.8).min(1.0);
        if matches!(mode, Mode::Race | Mode::Finish) && let Some(m) = self.race.player() {
            let p = self.race.track.pos(m.s, m.d);
            self.course_at_cheer([p.x as f32, 0.0, p.z as f32]);
        }
        let cheer = self.cheer_now;
        {
            let kit = &mut self.kit;
            let c = self.courses.get_mut(&self.cur_course).unwrap();
            c.cheer = cheer;
            c.write_cheer(kit);
        }
        self.audio.crowd(cheer, matches!(mode, Mode::Race | Mode::Finish | Mode::Title));

        // 自分の印と輪
        let me = self.race.player().cloned();
        let show_ring = me.is_some() && matches!(mode, Mode::Race | Mode::Paused);
        self.kit.scene[self.ring].visible = show_ring;
        self.kit.scene[self.me_mark].visible = show_ring;
        let sv = self.kit.scene[self.slip_ring].visible;
        self.kit.scene[self.slip_ring].visible = show_ring && sv;
        let focus: P3;
        if let (Some(m), true) = (&me, show_ring) {
            let p = self.race.track.pos(m.s, m.d);
            self.kit.scene[self.ring].position = Vec3::new(p.x as f32, (p.y + 0.05) as f32, p.z as f32);
            self.kit.scene[self.ring].rotation = Rot::y(self.race.track.yaw(m.s) as f32);
            let u = [(m.stamina / TUNE.stamina_max) as f32, if m.stamina < TUNE.low_at { 1.0 } else { 0.0 }, if m.exhausted > 0.0 { 1.0 } else { 0.0 }, 0.0];
            self.kit.set_uniform(self.ring_mat, bytemuck::bytes_of(&u));
            self.kit.scene[self.me_mark].position = Vec3::new(p.x as f32, (p.y + m.y + HERO_HEIGHT + 1.15 + (self.t_all * 5.0).sin() * 0.05) as f32, p.z as f32);
            self.sling_fx = (self.sling_fx - dt * 1.6).max(0.0);
            // 噴射の間は、画面のまわりの速さの流れを強く出す（効いていると分かるように）
            self.final_u.speed = (m.dash_k * 0.3 + self.sling_fx * 0.45 + if m.jet > 0.0 { 0.6 } else { 0.0 }) as f32;
            self.burst_t += dt;
            self.kit.scene[self.slip_ring].position = self.kit.scene[self.ring].position;
            if self.burst_t < 0.45 {
                // 抜け出した瞬間: 輪が弾ける
                let b = self.burst_t / 0.45;
                let sb = 1.0 + b * b * 3.2;
                self.kit.scene[self.slip_ring].visible = true;
                self.kit.scene[self.slip_ring].scale = Vec3::new(sb as f32, 1.0, sb as f32);
                self.kit.mat_mut(self.slip_mat).opacity = ((1.0 - b) * 0.95) as f32;
            } else {
                self.kit.scene[self.slip_ring].visible = m.slip > 0.05;
                let sk = if m.slip >= 1.0 { 1.02 + (self.t_all * 12.0).sin() * 0.04 } else { 0.8 + m.slip * 0.2 };
                self.kit.scene[self.slip_ring].scale = Vec3::new(sk as f32, 1.0, sk as f32);
                self.kit.mat_mut(self.slip_mat).opacity = (if m.slip >= 1.0 { 0.95 } else { m.slip * 0.5 }) as f32;
            }
            // 風の筋: ダッシュ・風よけ・抜け出しで、体のまわりを後ろへ流れる
            if anim > 0.0 {
                let yaw = self.race.track.yaw(m.s);
                let n = (if m.dash_k > 0.4 { 1 } else { 0 }) + (if m.slip_burst > 0.0 { 2 } else { 0 }) + (if m.draft { 1 } else { 0 }) + (if m.slip >= 1.0 { 1 } else { 0 });
                let c = Color::css(if m.slip_burst > 0.0 { "#ffe9a0" } else { "#ffffff" });
                // 1 本は、前後に並べた 4 つの小さな粒（流れると線に見える）。体の左右の後ろへ流す
                for _ in 0..n {
                    let side = if random() < 0.5 { -1.0 } else { 1.0 };
                    let (ox, oy) = (side * (0.55 + random() * 0.7), 0.3 + random() * 1.6);
                    for j in 0..4 {
                        let back = j as f64 * 0.22;
                        self.fx.emit((p.x + ox * yaw.cos() - yaw.sin() * back) as f32, (p.y + oy + m.y) as f32, (p.z - ox * yaw.sin() - yaw.cos() * back) as f32, (-yaw.sin() * 9.0) as f32, 0.0, (-yaw.cos() * 9.0) as f32, c, Emit { life: 0.26, size: 0.07 - j as f32 * 0.012, grow: -0.3, a: 0.75 - j as f32 * 0.15, g: 0.0 });
                    }
                }
            }
            self.audio.run(m.speed, m.dash_k, m.air, m.stamina / TUNE.stamina_max, m.exhausted > 0.0);
        } else {
            self.final_u.speed = 0.0;
            self.audio.run(0.0, 0.0, false, 1.0, false);
        }
        if matches!(mode, Mode::Race | Mode::Finish | Mode::Paused) {
            self.ui.hud.update(&self.race, dt);
            self.ui.hud.attack_key = match self.device() { Device::Touch => String::new(), Device::Pad => "B".to_string(), Device::Keys => key_name(&self.keys.key_of(Rebindable::Attack)) };
        }

        // カメラ
        let t_all = self.t_all;
        match mode {
            Mode::Title => self.title_camera(dt, t_all),
            Mode::Select => self.select_camera(dt, t_all),
            Mode::Intro => self.intro_camera(),
            Mode::Race => self.race_camera(dt, t_all),
            Mode::Finish | Mode::Photo => self.finish_camera(dt),
            Mode::Result => self.podium_scene(dt, t_all),
            Mode::Paused => { if self.before_pause == Mode::Race { self.race_camera(0.0, t_all); } else { self.intro_camera(); } }
            Mode::Loading => {}
        }
        if self.debug_get("closeup").is_truthy() && let Some(m) = &me {
            let p = self.race.track.pos(m.s, m.d);
            let a = self.race.track.yaw(m.s) + 0.52;
            self.kit.camera.position = Vec3::new((p.x + a.sin() * 2.6) as f32, (p.y + m.y + 1.7) as f32, (p.z + a.cos() * 2.6) as f32);
            self.look(DVec3::new(p.x, p.y + m.y + 1.1, p.z));
        }
        self.sky.update(&mut self.kit, t_all as f32);
        // 影の範囲は見ている所へ
        focus = match &me { Some(m) => self.race.track.pos(m.s, m.d), None => self.race.track.pos(self.race.runners[self.race.order[0]].s, 6.0) };
        let near = if matches!(mode, Mode::Title) { P3 { x: self.cam_look.x, y: 0.0, z: self.cam_look.z } } else { focus };
        if let Some(k) = self.kit.scene.dir_light_mut(self.key) { k.target = Target::Pos(Vec3::new(near.x as f32, 0.0, near.z as f32)); }
        let sd = self.sun_dir;
        self.kit.scene[self.key].position = Vec3::new((near.x + sd.x * 110.0) as f32, (sd.y * 110.0) as f32, (near.z + sd.z * 110.0) as f32);

        // 掲示板
        self.board_t -= dt;
        if self.board_t <= 0.0 && matches!(mode, Mode::Race | Mode::Title) { self.board_t = 0.5; self.draw_board(); }
        self.fx.upload(&mut self.kit);
        if draw && matches!(mode, Mode::Race | Mode::Title) { self.draw_live(); }
        // 画面の仕上げ
        if me.is_some() {
            let mut c = self.kit.camera;
            c.quaternion = self.cam_prev_q;
            let v = c.project(f3(self.cam_look));
            self.final_u.center = [v.x * 0.5 + 0.5, v.y * 0.5 + 0.5];
        }
        self.go_flash = (self.go_flash - dt * 3.0).max(0.0);
        self.final_u.flash = (self.go_flash * 0.16) as f32;
        self.final_u.desat = if mode == Mode::Paused { 0.45 } else { 0.0 };
        // 結果と出走登録: 背景をぼかして沈め、主役を立たせる
        let select_fish = mode == Mode::Select && matches!(&self.heroes[self.hero_i].body, Body::Fish(_));
        let want_blur = if mode == Mode::Result { 0.022 } else if select_fish { 0.003 } else if mode == Mode::Select { 0.012 } else { 0.0 };
        self.final_u.blur += (want_blur - self.final_u.blur) * (dt as f32 * 5.0).min(1.0);
        self.final_u.dim = if mode == Mode::Result { 0.42 } else if mode == Mode::Select { 0.12 } else { 0.0 };
        // 結果: 表彰台と、手前の自分（右下）がぼけないよう、ぼかさない楕円を下と横へ広げる
        if mode == Mode::Result { self.final_u.blur_at = [0.52, 0.56, 0.3, 0.62]; }
        // 横向きの魚は頭と尾まで広い。口・眼・えらを楕円内に収める。
        else if select_fish { self.final_u.blur_at = [0.62, 0.5, 0.56, 0.8]; }
        else if mode == Mode::Select { self.final_u.blur_at = [0.62, 0.5, 0.2, 0.55]; }
        let live_on = matches!(mode, Mode::Race | Mode::Finish | Mode::Paused) && me.is_some();
        if draw {
            let (rw, rh) = self.kit.render_size();
            self.final_u.res = [rw as f32, rh as f32];
            self.final_u.mask_px = self.kit.mask_px();
            self.final_u.time = t_all as f32;
            self.final_u.outline[3] = if live_on { 1.0 } else { 0.0 };
            self.kit.set_final_uniform(bytemuck::bytes_of(&self.final_u));
            let opts = RenderOpts { outline: live_on, hidden: live_on, clear: Color::hex(0x2a2650) };
            let (w, h) = self.kit.size();
            // 端の安全域は、窓の大きさが変わったとき（端末の向き）に読み直す
            if (w, h) != self.ui_size { self.ui_size = (w, h); self.ui.safe = safe_area(); }
            self.ui.muted = self.audio.muted();
            // 指の部品はタッチの機器でレース中（一時停止でない）だけ。スティックは左半分、ボタンはダッシュとジャンプ。
            // ゴールの後（順位が出ている間）は部品を描かず、どこを叩いても「叩いた」（先へ進む）
            let (buttons, stick) = self.ui.touch_layout(w, h);
            let fin = self.ui.finishing;
            self.pointer.set_pad(self.ui.touch && self.ui.racing && !self.ui.paused, if fin { Vec::new() } else { buttons }, if fin { None } else { Some(stick) });
            let pv = self.pointer.pad_view();
            self.ui.touch_view = crate::ui::TouchView { stick: pv.stick_at.map(|p| (p.x, p.y)), stick_on: pv.stick_on, knob: (pv.knob.x, pv.knob.y), dash: pv.on.contains(&"dash"), jump: pv.on.contains(&"jump"), attack: pv.on.contains(&"attack") };
            let raw = self.ui_raw.take().unwrap_or_default();
            let hand = ui_frame(&mut self.kit, &mut self.layer, raw, &mut self.ui, &opts);
            if hand != self.cursor_hand { self.cursor_hand = hand; let _ = self.canvas.style().set_property("cursor", if hand { "pointer" } else { "" }); }
        }
    }

    /// 届いている指・マウスの入力を読み、押したボタンをすぐ効かせる（入力は描くときに egui へ渡す）
    pub fn pointer_now(&mut self) {
        let (w, h) = self.kit.size();
        let mut raw = self.pointer.take_ui(w, h, self.kit.surface_dpr());
        let touch = self.pointer.last_was_touch();
        if raw.events.iter().any(|e| matches!(e, egui::Event::PointerButton { pressed: true, .. })) {
            self.used(if touch { Used::Touch } else { Used::Mouse });
            // 指・マウスで触ったら、選んでいる項目は最初（つづける・もう一度）へ戻す（輪が消えている間に決定を押しても驚かない）
            self.ui.pause.focus = 0;
            self.ui.res_focus = 0;
        }
        let acts = self.ui.feed(&raw.events, touch);
        if let Some(old) = self.ui_raw.take() { let mut ev = old.events; ev.append(&mut raw.events); raw.events = ev; }
        self.ui_raw = Some(raw);
        for a in acts { self.act(a); }
    }

    /// 画面のボタンを押した（今の版の click の受け手）
    pub fn act(&mut self, a: Act) {
        if a != Act::Block { self.acts_log.push(format!("{a:?}")); }
        if a != Act::CareerNew { self.reset_armed = false; }
        match a {
            Act::Start => { self.audio.init(); self.wipe(|a| a.to_select()); }
            Act::Records => { self.audio.init(); self.show_records(); }
            Act::RecClose => self.close_records(),
            Act::Go => self.go(),
            Act::Again => self.again(),
            // みんなで走るときは「部屋を出る」
            Act::Menu => { self.leave_result(); self.wipe(|a| a.to_select()); }
            // みんなで走る: このゲームには入れない。かけっこグランプリ（同じ遊び・人の主役で、最大 4 人）を同じタブで開く
            Act::Online => { if self.mode == Mode::Title { let _ = dom::window().location().set_href(crate::ui::ONLINE_URL); } }
            Act::Pause => self.set_paused(true),
            Act::Reload => { let _ = dom::window().location().reload(); }
            Act::Mute => { self.audio.init(); self.audio.toggle_mute(); self.pause_sync(); }
            Act::Mode(on) => { self.is_cup_mode = on; self.is_career_mode = false; self.audio.select(); self.build_select(); }
            Act::Career => {
                self.is_career_mode = true;
                self.is_cup_mode = false;
                self.audio.select();
                // 途中経過があれば、育てているキャラへ
                match self.save.career().and_then(|c| self.heroes.iter().position(|h| h.id == c.hero)) { Some(hi) if hi != self.hero_i => self.pick_hero(hi as i64), _ => self.build_select() }
            }
            Act::CareerNew => {
                // 1 回目は確かめ（字が変わる）。2 回目で途中経過を消す
                if self.reset_armed { self.reset_armed = false; self.save.data["career"] = Value::Null; self.save.write(); self.audio.confirm(); } else { self.reset_armed = true; self.audio.select(); }
                self.build_select();
            }
            Act::Course(i) => { if !self.is_cup_mode && !self.is_career_mode && let Some(c) = self.courses_defs.get(i) { let id = c.id; self.pick_course(id); } }
            Act::Diff(i) => { self.save.s.difficulty = DIFFS[i].0; self.save.write(); self.audio.select(); self.build_select(); }
            Act::Hero(i) => { if i == self.hero_i { self.go(); } else { self.pick_hero(i as i64); } }
            Act::PauseBtn(i) => self.pause_run(i),
            Act::Key(j) => self.key_wait(j),
            Act::Block => {}
        }
    }

    /// 走路の物の 3D（Tripo で作った GLB）を読む。読めた種類から札と入れ替わる（読めなければ札のまま）
    fn load_item_models(&mut self) {
        for (k, name) in crate::view::items::MODEL_FILES.iter().enumerate() {
            let me = self.me.clone();
            let url = format!("./models/items/{name}.glb");
            spawn_local(async move {
                match island_kit::glb::load_glb(&url).await {
                    Ok(g) => with(&me, move |a| { let kit = &mut a.kit; a.items.set_model(kit, k, &g); }),
                    Err(e) => web_sys::console::warn_1(&e.into()),
                }
            });
        }
    }

    /// キャラの選びの絵を読む（読めたら帯と通過順の顔に出す。今の版の background-image と同じく、読めた物から）
    fn load_pictures(&mut self) {
        for (i, h) in self.heroes.iter().enumerate() {
            // 絵の出どころ: 配布の絵 → つくったキャラは ZIP の見本の絵 → 魚・イカは racers/<id>.webp
            enum Src { Delivery(character_actor::delivery::DeliveryFile), Bytes(std::sync::Arc<Vec<u8>>), Url(String) }
            let src = match (&h.select_file, &h.body) {
                (Some(f), _) => Src::Delivery(f.clone()),
                (None, Body::Guest(a)) => match &a.thumbnail { Some(t) => Src::Bytes(t.clone()), None => continue },
                _ if !h.thumb.is_empty() => Src::Url(h.thumb.clone()),
                _ => continue,
            };
            let (cat, me) = (self.catalog.clone(), self.me.clone());
            spawn_local(async move {
                let bytes = match src {
                    Src::Delivery(f) => cat.read_picture_web(&f).await.ok(),
                    Src::Bytes(b) => Some(b.as_ref().clone()),
                    Src::Url(u) => crate::character_delivery::fetch_bytes(&u).await.ok(),
                };
                let Some(bytes) = bytes else { return };
                with(&me, move |a| {
                    let t = crate::ui::load_image(a.layer.ctx(), &format!("hero-{i}"), &bytes);
                    a.hero_tex.borrow_mut()[i] = t.clone();
                    if let Some(b) = a.ui.heroes.get_mut(i) { b.tex = t; }
                    let tex = a.hero_tex.borrow();
                    let faces: Vec<Option<Tex>> = a.lineup.iter().map(|h| tex.get(*h).cloned().flatten()).collect();
                    drop(tex);
                    a.ui.hud.set_faces(faces);
                });
            });
        }
    }

    fn course_at_cheer(&mut self, at: [f32; 3]) { self.course().cheer_at = at; }

    pub fn set_cup_mode(&mut self, on: bool) { self.is_cup_mode = on; }

    /// 確かめ用: 描くのに使った値（比べる道具の TRACE_TS と同じ形）
    pub fn trace_json(&self) -> Value {
        let k = &self.kit;
        let f = &self.final_u;
        let arr = |n: NodeId| { let x = &k.scene[n]; json!([x.position.x, x.position.y, x.position.z, x.scale.x, if x.visible { 1 } else { 0 }]) };
        let m16 = |n: NodeId| k.scene.world_matrix(n).to_cols_array().to_vec();
        let runners: Vec<Value> = self.lineup.iter().map(|h| {
            let id = &self.heroes[*h].id;
            match self.views.get(id) {
                None => json!({ "id": id, "loaded": false }),
                Some(v) => json!({ "id": id, "loaded": v.loaded(k), "vis": k.scene[v.holder].visible, "holder": m16(v.holder), "lean": m16(v.lean), "shadow": arr(v.shadow), "tag": arr(v.tag), "cur": if k.has_actor(v.actor) { k.actor(v.actor).current().map(action_name) } else { None } }),
            }
        }).collect();
        let c = k.camera;
        let (ku, lu, ou) = { let m = self.race.player(); (m.map_or(1.0, |m| m.stamina / TUNE.stamina_max), m.map_or(0.0, |m| if m.stamina < TUNE.low_at { 1.0 } else { 0.0 }), m.map_or(0.0, |m| if m.exhausted > 0.0 { 1.0 } else { 0.0 })) };
        json!({
            "mode": self.mode.name(), "tAll": self.t_all,
            "cam": { "p": [c.position.x, c.position.y, c.position.z], "q": [c.quaternion.x, c.quaternion.y, c.quaternion.z, c.quaternion.w], "fov": c.fov, "aspect": c.aspect },
            "final": { "speed": f.speed, "flash": f.flash, "desat": f.desat, "blur": f.blur, "dim": f.dim, "gold": f.gold, "warm": f.warm, "center": f.center, "blurAt": f.blur_at, "outline": f.outline[3] },
            "ring": arr(self.ring), "ringU": if k.scene[self.ring].visible { json!([ku, lu, ou]) } else { Value::Null }, "slip": arr(self.slip_ring), "slipA": k.mat(self.slip_mat).opacity, "mark": arr(self.me_mark), "ghost": arr(self.ghost_ring), "ghostA": k.mat(self.ghost_mat).opacity, "ghostTag": arr(self.ghost_tag),
            "fx": self.fx.sums(), "runners": runners,
        })
    }
    /// 確かめ用: 主役の骨（名前 → 世界の位置）とモーフの重み
    pub fn bones_json(&self, id: &str) -> Value {
        let Some(v) = self.views.get(id) else { return Value::Null };
        if !self.kit.has_actor(v.actor) { return Value::Null; }
        let a = self.kit.actor(v.actor);
        let mut bones = serde_json::Map::new();
        for (i, n) in a.skel.names.iter().enumerate() {
            if n.is_empty() || !a.skel.morphs[i].is_empty() { continue; }
            let p = a.skel.world[i].w_axis;
            bones.insert(n.clone(), json!([p.x, p.y, p.z]));
        }
        let mut seen: Vec<Vec<f32>> = Vec::new();
        for m in &a.skel.morphs { if !m.is_empty() && !seen.contains(m) { seen.push(m.clone()); } }
        let key = |m: &Vec<f32>| m.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",");
        seen.sort_by_key(key);
        json!({ "bones": bones, "morph": seen, "cur": a.current().map(action_name) })
    }
    /// 確かめ用: 今の版の中の値（道具の PATCH_TS の internals と同じ名前）
    pub fn internals_json(&self) -> Value {
        let tips: Vec<&String> = self.tips_shown.iter().collect();
        json!({ "firstRace": self.first_race, "heroI": self.hero_i, "trauma": self.trauma, "goFlash": self.go_flash, "cheerNow": self.cheer_now, "cheerTarget": self.cheer_target, "camS": self.cam_s, "camD": self.cam_d, "camY": self.cam_y, "lookD": self.look_d, "camUp": self.cam_up, "pull": self.pull, "tAll": self.t_all, "finishTimer": self.finish_timer, "introT": self.intro_t, "photoT": self.photo_t, "shot": self.shot, "shotT": self.shot_t, "tips": tips, "lastSay": self.last_say, "ghostRun": self.ghost_run, "lapSplit": self.lap_split })
    }

    pub fn close_records(&mut self) { self.ui.close_records(); }
}

/// 起動の間（App ができるまで）も読み込みの画面を描く
struct Boot { kit: Option<Kit>, ui: Screens, layer: RawLayer, pointer: DomInput, done: bool }

/// UI の 1 コマ: 入力を渡して画面を描く。カーソルを指の形にするかを返す
fn ui_frame(kit: &mut Kit, layer: &mut RawLayer, raw: egui::RawInput, ui: &mut Screens, opts: &RenderOpts) -> bool {
    let ctx = layer.begin(raw);
    ui.notrans = notrans();
    ui.ui(&ctx);
    // egui のコマは毎コマ締める。画面を取れなかったコマ（after が呼ばれない）の字の絵の更新は、次に描くコマへ持ち越す（game-ui 0.1.3）
    let out = layer.finish();
    kit.render_then(opts, |device, queue, enc, view, size| layer.paint_finished(device, queue, enc, view, size));
    matches!(out.cursor_icon, egui::CursorIcon::PointingHand)
}

/// body.notrans（比べる道具が付ける）
fn notrans() -> bool { dom::class_has(&dom::body(), "notrans") }

/// 写真判定の 1 枚（線形の画素 → sRGB・真ん中の白い線・filter: sepia(.6) contrast(1.1)）
fn photo_pixels(px: &[u8]) -> Vec<u8> {
    let mut gamma = [0u8; 256];
    for (i, x) in gamma.iter_mut().enumerate() { *x = sim_math::round(255.0 * (i as f64 / 255.0).powf(1.0 / 2.2)) as u8; }
    let mut out = vec![0u8; 640 * 360 * 4];
    let a = 0.6f32;
    let m = [
        [0.393 + 0.607 * (1.0 - a), 0.769 - 0.769 * (1.0 - a), 0.189 - 0.189 * (1.0 - a)],
        [0.349 - 0.349 * (1.0 - a), 0.686 + 0.314 * (1.0 - a), 0.168 - 0.168 * (1.0 - a)],
        [0.272 - 0.272 * (1.0 - a), 0.534 - 0.534 * (1.0 - a), 0.131 + 0.869 * (1.0 - a)],
    ];
    for y in 0..360 {
        for x in 0..640 {
            let i = y * 640 + x;
            let mut c = [gamma[px[i * 4] as usize] as f32, gamma[px[i * 4 + 1] as usize] as f32, gamma[px[i * 4 + 2] as usize] as f32];
            // stroke: rgba(255,255,255,.75)、幅 2px（x = 319・320 の 2 列）
            if x == 319 || x == 320 { for v in c.iter_mut() { *v = *v * 0.25 + 255.0 * 0.75; } }
            let s = [0, 1, 2].map(|k| (m[k][0] * c[0] + m[k][1] * c[1] + m[k][2] * c[2]).min(255.0));
            for k in 0..3 { out[i * 4 + k] = (((s[k] / 255.0 - 0.5) * 1.1 + 0.5) * 255.0).round().clamp(0.0, 255.0) as u8; }
            out[i * 4 + 3] = 255;
        }
    }
    out
}

pub async fn run() -> Result<(), String> {
    let keys = Keys::load();
    let audio = Audio::new("gyogyo.audio");
    // 最初の操作で音を始める（ブラウザの決まり）
    // pointerdown は canvas で受ける（画面のボタン・出ている画面の上の押下は game-ui の dom_input が window へ届けない）
    let canvas: web_sys::HtmlCanvasElement = dom::el("gl").unchecked_into();
    for (t, ty) in [(canvas.unchecked_ref::<web_sys::EventTarget>().clone(), "pointerdown"), (dom::window().unchecked_into(), "keydown")] {
        let a = audio.clone();
        dom::listen::<web_sys::Event>(&t, ty, None, true, move |_| a.init());
    }
    let mut kit = Kit::new(canvas.clone()).await?;
    // 画面（サーフェス）は端末の画素比で取り、3D は今までどおり画素の上限の的に描いて仕上げで広げる（今の版の three の canvas を CSS で広げるのと同じ）。
    // 画面の文字とボタンを今の版の DOM と同じ細かさで描くため（island-kit 0.1.29）
    kit.set_full_dpr_surface(true);
    kit.camera.fov = 50.0;
    kit.camera.near = 0.3;
    kit.camera.far = 1400.0;
    kit.set_final(include_str!("../shaders/final.wgsl"), std::mem::size_of::<FinalU>() as u64);
    kit.set_actor_style(ActorStyle::MUSO);
    kit.hidden_color = Color::css("#fff6e4");
    kit.hidden_opacity = 0.34;
    kit.bloom = island_kit::Bloom { strength: 0.4, radius: 0.5, threshold: 1.2 };
    let mut save = Save::load();

    // ---- 光と空気 ----
    let hemi = kit.scene.hemi_light(None, Color::WHITE, Color::WHITE, 1.0);
    let key = kit.scene.dir_light(None, Color::WHITE, 1.0);
    kit.scene[key].cast_shadow = true;
    {
        let l = kit.scene.dir_light_mut(key).unwrap();
        l.shadow.map_size = 2048;
        (l.shadow.left, l.shadow.right, l.shadow.top, l.shadow.bottom, l.shadow.near, l.shadow.far) = (-34.0, 34.0, 34.0, -34.0, 1.0, 220.0);
        l.shadow.bias = -0.0006;
        l.shadow.normal_bias = 0.05;
    }
    let sky = Sky::new(&mut kit);
    kit.scene[sky.sea].position.y = -66.0;
    kit.fog = Some(FogExp2 { color: Color::hex(0xffffff), density: 0.003 });

    let input = controls::create_input();
    // 粒の入れ物（使い回しの輪）。蹴り上げの粒を足して 900 では、ゴールの紙吹雪や当たりの破片が 1 秒ほどで上書きされた
    let fx = Fx::new(&mut kit, 2600);
    island_kit::texture::load_font("900 70px 'Zen Maru Gothic'").await;
    let _ = JsFuture::from(dom::document().fonts().load_with_text("900 70px 'Zen Maru Gothic'", "速体根跳加息毒")).await;
    let items = ItemView::new(&mut kit);
    let props = Props::new(&mut kit);

    // ---- 画面の文字（字体を読んでから。読む間は index.html の #boot が出ている）----
    let names = crate::ui::font_files();
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    // 字体は同時に取る（game-ui の page::fetch_all）
    let got = game_ui::page::fetch_all(&names).await;
    for (n, b) in names.iter().zip(got) { match b { Ok(b) => { files.insert(n.clone(), b); } Err(e) => web_sys::console::error_1(&e.into()) } }
    let layer = RawLayer::new(kit.device(), kit.surface_format(), |ctx| crate::ui::install_fonts(ctx, &|n| files.get(n).cloned()));
    drop(files);
    let pointer = DomInput::new(&canvas, layer.ctx(), RawLayer::max_side(kit.device()));
    let mut ui = Screens::new(raf::now_ms() / 1000.0);
    ui.safe = safe_area();
    // 端末の「動きを減らす」設定（幕を出さない・紙吹雪は控えめ・数え上げは短く）
    let calm = game_ui::page::reduced_motion();
    ui.reduce_motion = calm;
    ui.set_calm(calm);
    ui.touch = input.device() == Device::Touch;
    let boot = Rc::new(RefCell::new(Boot { kit: Some(kit), ui, layer, pointer: pointer.clone(), done: false }));
    {
        let b = boot.clone();
        raf::start(move |now| {
            let Ok(mut b) = b.try_borrow_mut() else { return };
            if b.done { return; }
            let Boot { kit, ui, layer, pointer, .. } = &mut *b;
            let Some(kit) = kit.as_mut() else { return };
            ui.set_now(now / 1000.0);
            let (w, h) = kit.size();
            let raw = pointer.take_ui(w, h, kit.surface_dpr());
            // 読み込めなかったときの「読み込み直す」
            for a in ui.feed(&raw.events, pointer.last_was_touch()) { if a == Act::Reload { let _ = dom::window().location().reload(); } }
            let _ = ui_frame(kit, layer, raw, ui, &RenderOpts { outline: false, hidden: false, clear: Color::hex(0x0f3d2e) });
        });
    }
    let _ = dom::window().dispatch_event(&web_sys::Event::new("gyogyo-ready").unwrap());
    let progress = |k: f64, t: Option<&str>| { let mut b = boot.borrow_mut(); b.ui.progress(pct(k), t); };
    // 主役の一覧が読めなかった: 知らせの文と「読み込み直す」を出して待つ（押す・Enter・Space でページを読み直す）
    let fail = || {
        boot.borrow_mut().ui.fail("キャラを読み込めませんでした。\n通信を確かめてください");
        dom::listen::<web_sys::KeyboardEvent>(&dom::window(), "keydown", None, false, |e| if matches!(e.code().as_str(), "Enter" | "Space") { let _ = dom::window().location().reload(); });
    };

    // ---- コース ----
    progress(0.08, Some("競馬場を用意しています…"));
    if let Some(doc) = web_sys::window().and_then(|w| w.document()) && let Ok(p) = doc.fonts().ready() { let _ = JsFuture::from(p).await; }
    island_kit::texture::load_font("32px 'Dela Gothic One'").await;
    island_kit::texture::load_font("700 32px Fredoka").await;
    let defs = courses();
    let course_id = if defs.iter().any(|c| c.id == save.s.course) { save.s.course.clone() } else { defs[0].id.to_string() };
    let assets = Rc::new(RefCell::new(Assets::default()));
    load_assets(&assets).await;

    // ---- キャラの名簿と走者 ----
    progress(0.4, Some("走る人たちの名簿を読んでいます…"));
    let catalog = match open_browser_catalog() { Ok(c) => c, Err(e) => { web_sys::console::error_1(&e.clone().into()); fail(); return Ok(()); } };
    let mut delivered = Vec::new();
    match list_heroes(&catalog).await { Ok(h) => delivered = h, Err(e) => web_sys::console::error_1(&e.into()) }
    // Studio が送ったキャラ（無い・読めないときは、きゃしーと魚とイカだけ）
    let guest = studio_zip().and_then(|b| guest_hero(&b).map_err(|e| web_sys::console::error_1(&format!("Studio のキャラを読めません: {e}").into())).ok());
    let heroes: Vec<HeroInfo> = to_hero_list(delivered, guest);
    // Studio から来た直後（?from=studio）は、つくったキャラを選んだ状態で始める
    let from_studio = js_sys::Reflect::get(&dom::window(), &"__gyogyoFromStudio".into()).is_ok_and(|v| v.is_truthy());
    if from_studio && heroes.iter().any(|h| h.id == GUEST_ID) { save.s.hero = GUEST_ID.to_string(); save.write(); }
    let hero_i = heroes.iter().position(|h| h.id == save.s.hero).unwrap_or(0);

    let mut kit = boot.borrow_mut().kit.take().unwrap();
    // 足もとのスタミナの輪（目を走路から離さずに残量が読める）
    let ring_g = kit.geometry(&ring(0.78, 1.0, 64).rotate_x(-std::f32::consts::FRAC_PI_2).owned());
    let ring_mat = kit.material(Material {
        transparent: true, depth_write: false,
        ..Material::custom(CustomShader { label: "ring".into(), wgsl: include_str!("../shaders/ring.wgsl").into(), uniform: bytemuck::bytes_of(&[1.0f32, 0.0, 0.0, 0.0]).to_vec(), ..Default::default() })
    });
    let ring_n = kit.scene.mesh(None, ring_g, ring_mat);
    kit.scene[ring_n].render_order = 2;
    kit.scene[ring_n].visible = false;
    let slip_g = kit.geometry(&ring(1.03, 1.16, 64).rotate_x(-std::f32::consts::FRAC_PI_2).owned());
    let slip_mat = kit.material(Material::basic().with_color(Color::css("#ffffff")).with_opacity(0.9).see_through());
    let slip_ring = kit.scene.mesh(None, slip_g, slip_mat);
    kit.scene[slip_ring].render_order = 2;
    kit.scene[slip_ring].visible = false;
    // 自分の上の矢じり
    let mark_t = kit.canvas_texture(64, 64, false, |g| {
        g.set_fill_style_str("#fff6e4");
        g.begin_path();
        g.move_to(8.0, 10.0); g.line_to(56.0, 10.0); g.line_to(32.0, 54.0);
        g.close_path();
        g.fill();
        g.set_stroke_style_str("#0f3d2e");
        g.set_line_width(5.0);
        g.stroke();
    });
    let mark_m = kit.material(Material { depth_test: false, fog: false, ..Material::sprite().with_map(mark_t) });
    let me_mark = kit.scene.sprite(None, mark_m);
    kit.scene[me_mark].set_scale(0.42);
    kit.scene[me_mark].render_order = 6;
    kit.scene[me_mark].visible = false;
    // 自己ベストの影
    let gh_g = kit.geometry(&ring(0.62, 0.92, 40).rotate_x(-std::f32::consts::FRAC_PI_2).owned());
    let ghost_mat = kit.material(Material::basic().with_color(Color::css("#ffcf4d")).with_opacity(0.8).see_through());
    let ghost_ring = kit.scene.mesh(None, gh_g, ghost_mat);
    kit.scene[ghost_ring].render_order = 2;
    kit.scene[ghost_ring].visible = false;
    let tag_t = kit.canvas_texture(128, 44, true, |g| {
        g.set_fill_style_str("#ffcf4d");
        g.begin_path();
        let _ = g.round_rect_with_f64(0.0, 0.0, 128.0, 44.0, 22.0);
        g.fill();
        g.set_fill_style_str("#0f3d2e");
        g.set_font("900 26px 'Zen Maru Gothic', sans-serif");
        g.set_text_align("center");
        g.set_text_baseline("middle");
        let _ = g.fill_text("ベスト", 64.0, 24.0);
    });
    let tag_m = kit.material(Material { kind: MatKind::Sprite { rotation: 0.0, size_attenuation: false }, depth_write: false, fog: false, ..Material::sprite().with_map(tag_t) });
    let ghost_tag = kit.scene.sprite(None, tag_m);
    kit.scene[ghost_tag].scale = Vec3::new(0.06, 0.02, 1.0);
    kit.scene[ghost_tag].render_order = 5;
    kit.scene[ghost_tag].visible = false;
    let photo_rt = kit.render_target(640, 360);
    let live_rt = kit.render_target(320, 256);

    let debug = js_sys::Object::new();
    for (k, v) in [("scale", JsValue::from_f64(1.0)), ("auto", JsValue::FALSE), ("noAutoPause", JsValue::FALSE), ("closeup", JsValue::FALSE)] { let _ = js_sys::Reflect::set(&debug, &k.into(), &v); }
    let race_seed = (js_sys::Date::now() as u64 & 0xffff) as i64 + 1;
    let race = Race::new(RaceOptions { course: defs.iter().find(|c| c.id == course_id).unwrap(), seed: race_seed as u32, difficulty: Difficulty::Normal, player_gate: None, auto_jump: false, wide_jump: false, levels: None, humans: Vec::new() });
    let first_race = !save.s.first_done;
    let now = raf::now_ms();
    let device = input.device();
    let final_u = FinalU {
        res: [1.0, 1.0], mask_px: [1.0, 1.0], ripple: [0.5, 0.5, 1.0, 0.0],
        outline: { let c = Color::css("#ffe8c8"); [c.r, c.g, c.b, 0.0] }, outline_dark: { let c = Color::css("#3a1f3e"); [c.r, c.g, c.b, 0.0] },
        blur_at: [0.5, 0.5, 0.2, 0.42], center: [0.5, 0.5], zoom: 0.0, speed: 0.0, time: 0.0, vignette: 0.32, warm: 0.0, flash: 0.0, desat: 0.0, cool: 0.0, aberration: 0.0015, gold: 0.0, red: 0.0, blur: 0.0, dim: 0.0, _p: 0.0,
    };
    let cam_q = kit.camera.quaternion;
    let (ui, layer) = { let mut b = boot.borrow_mut(); b.done = true; let b = &mut *b; (std::mem::replace(&mut b.ui, Screens::new(0.0)), std::mem::replace(&mut b.layer, RawLayer::new(kit.device(), kit.surface_format(), |_| {}))) };
    let hero_tex = Rc::new(RefCell::new(vec![None; heroes.len()]));
    let shared: Shared = Rc::new_cyclic(|me| RefCell::new(App {
        me: me.clone(), kit, input, audio, keys, ui, layer, pointer, canvas, cursor_hand: false, ui_raw: None, hero_tex, fx, items, hitstop: 0.0, hit_cam: 0.0, hit_cam_len: HIT_CAM_T, hit_pair: (0, 0), last_bolt_pop: -9.0, sky, save, hemi, key, sun_dir: DVec3::new(0.5, 0.5, -0.7).normalize(), courses_defs: defs, courses: HashMap::new(), props, assets, cur_course: course_id.clone(), course_id: course_id.clone(), catalog: Rc::new(catalog),
        heroes, views: HashMap::new(), loading: HashMap::new(), hero_i, lineup: Vec::new(), ring: ring_n, ring_mat, slip_ring, slip_mat, me_mark, ghost_ring, ghost_mat, ghost_tag, photo_rt, live_rt, live_frame: 0,
        debug_policy: None, debug_input: None, debug, mode: Mode::Loading, before_pause: Mode::Race, race_seed, race, cup: None, is_cup_mode: false, career: None, is_career_mode: false, reset_armed: false, finish_timer: 0.0, intro_t: 0.0, first_race, go_flash: 0.0, sling_fx: 0.0, burst_t: 9.0,
        cam_pos: DVec3::new(0.0, 40.0, 80.0), cam_look: DVec3::ZERO, cam_prev_q: cam_q, cam_s: 0.0, cam_d: 6.0, cam_y: 0.0, look_d: 6.0, cam_up: 4.6, pull: 0.0, trauma: 0.0, cheer_target: 0.15, cheer_now: 0.15, tips_shown: HashSet::new(), wipe_busy: false, last_say: -99.0,
        photo: None, photo_t: 0.0, called_close: false, called_half: false, split_done: false, lap_split: 0.0, ghost_run: Vec::new(), ghost_best: None, ghost_clock: 0.0, shot_t: 0.0, shot: 0, last_greet: 0.0, select_snap: false, picked_at: -99.0, cheer_clock: 0.0, board_t: 0.0,
        pick_seq: 0, loading_hero: false, start_seq: 0, last: now, last_tick: now, t_all: 0.0, perf: Perf { frames: 0.0, slow: 0.0, tick_ms: 0.0, last: now, worst: 0.0 }, seen: Seen::new(dev_of(device)), result_t: 0.0, show: ResultShow::idle(), calm, cheer_log: Vec::new(), ui_size: (0.0, 0.0), final_u, started: false, cam_at: None, ev_log: Vec::new(), acts_log: Vec::new(), ctl_seen: Controls::default(),
    }));
    let wk = Rc::downgrade(&shared);
    {
        let mut a = shared.borrow_mut();
        let a = &mut *a;
        let id = a.course_id.clone();
        a.use_course_now(&id);
        // 一時停止
        let l = a.pause_labels();
        a.ui.pause.set_device(device, &l, &a.keys);
        a.apply_quality();
        a.load_pictures();
        a.load_item_models();
    }
    let w = wk.clone();
    // みんなで走るレースは止められないので、画面を離れても止めない（戻ると追いつくまで早送り）
    dom::listen::<web_sys::Event>(&dom::document(), "visibilitychange", None, false, move |_| {
        if dom::document().hidden() { with(&w, |a| if !a.debug_get("noAutoPause").is_truthy() { a.set_paused(true); }); }
    });
    let w = wk.clone();
    dom::listen::<web_sys::Event>(&dom::window(), "blur", None, false, move |_| with(&w, |a| if !a.debug_get("noAutoPause").is_truthy() { a.set_paused(true); }));

    crate::game_api::install(&shared);

    // 走者を読む間も画面を描く（輪を先に回す。読み込みの画面が全体を覆う）
    start_loops(&shared);
    let wait = {
        let mut a = shared.borrow_mut();
        a.ui.progress(pct(0.55), Some("走者を呼んでいます…"));
        a.race_seed += 1;
        let seed = a.race_seed;
        a.lineup = a.pick_lineup(None, seed);
        let w = wk.clone();
        a.load_lineup(Some(Rc::new(move |d, t| with(&w, move |a| a.ui.progress(pct(0.55 + 0.45 * d as f64 / t as f64), Some("走者を呼んでいます…"))))))
    };
    let _ = JsFuture::from(wait).await;
    shared.borrow_mut().to_title();
    let search = dom::window().location().search().unwrap_or_default();
    // 撮影用（?video）: HUD を順位・残り・武器だけにし、Ⅱ も出さない
    if search.trim_start_matches('?').split('&').any(|kv| kv == "video" || kv.starts_with("video=")) { shared.borrow_mut().ui.hud.video = true; }
    Ok(())
}

fn start_loops(shared: &Shared) {

    // 隠れたタブでも進める（TS の setInterval(() => { if (now - lastTick > 120) tick(); }, 33)）
    let s = shared.clone();
    raf::drive(33, move |now| {
        if let Ok(mut a) = s.try_borrow_mut() && now - a.last_tick > 120.0 { a.tick(None, true); }
    });
    let s = shared.clone();
    raf::start(move |now| {
        let mut a = s.borrow_mut();
        a.started = true;
        let gap = now - a.perf.last;
        a.perf.last = now;
        a.tick(None, true);
        a.perf.frames += 1.0;
        a.perf.tick_ms += raf::now_ms() - now;
        if gap > 22.0 { a.perf.slow += 1.0; }
        a.perf.worst = a.perf.worst.max(gap);
    });
}
