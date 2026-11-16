// 画面の流れ（読み込み → タイトル（自動の見本の戦い）→ 武将選び → 戦い → 結果）と、毎フレームの更新。
// 戦いの出来事（sim の events）を、斬撃・火花・揺れ・ぶれ・音・知らせへ変える。
use crate::assets::{self, BASE};
use crate::audio::{Audio, Track};
use crate::controls;
use crate::delivery::Catalog;
use crate::gfx::{hex, Camera, FinalU, Gfx, Globals};
use crate::hero::actor::CharacterActor;
use crate::hero::hero::{default_weapon, gait_for, guest_actor, guest_hero, to_hero_list, HeroInfo, HeroOpts, HeroView, GUEST_ID, WEAPONS};
use crate::hud::{Hud, Tone};
use crate::input::Input;
use crate::ui::{fmt_num, load_image, Honor, HeroCard, PathCard, PathKind, ResultData, ResultRow, Safe, Screen, Screens, Tex, UiAction, UiLayer, WeaponCard};
use crate::view::boss_fx::BossFx;
use crate::view::boss_view::BossView;
use crate::view::horde::{sparkle, Horde};
use crate::view::items::Items;
use crate::view::materials::{ORDERS, ORDER_OPAQUE};
use crate::view::mirrors::Mirrors;
use crate::view::rank_blade::RankBlade;
use crate::view::sky::{lerp_theme, LinearTheme, Sky};
use crate::stage::STAGE;
use crate::view::vfx::{Burst, Vfx, LINK_LIFE};
use crate::view::world::{light_eye, World};
use glam::{DVec3, Mat4, Vec3};
use serde_json::{json, Value};
use sim::aim_shape::{aim_shape, in_aim, AimShape};
use sim::battle::{create_battle, path_info, BattleEvent, BossState, EnemyState, Input as SimInput, Battle, Owner, Path, PickupKind, PlayerState, Phase, Role, TelegraphKind, TelegraphSrc, KINDS, NO_INPUT, RIDE_T, START, WIRE_AIM, WIRE_TOPPLE};
use sim::bot::{create_bot, Bot};
use game_ui::cheer::{cheer_of, Cheer, CheerEvent, ResultShow};
use sim::cheer::{top_rank, STAMP_S};
use sim::moves::{is_charge, is_normal, same, ArcKind, Arc as MoveArc, HitFx, CHARGES, HURL, JUMP_CHARGE, RANBU, RANBU_CUTIN};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use crate::platform::{clock_ms, store, Instant, SystemTime, UNIX_EPOCH};
use winit::event::WindowEvent;
use winit::window::Window;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode { Title, Select, Battle, Paused, Result, Gameover }

fn kanji_ko(n: i64) -> String {
    match n { 50 => "五十人斬り", 100 => "百人斬り", 200 => "二百人斬り", 300 => "三百人斬り", 500 => "五百人斬り", 700 => "七百人斬り", 1000 => "千人斬り", 1500 => "千五百人斬り", 2000 => "二千人斬り", _ => return format!("{n}人斬り") }.to_string()
}

/// 比較・確かめ用の起動の設定（コマンドライン）
#[derive(Clone, Default)]
pub struct Options {
    /// 自動で出陣して、bot に戦わせる（__game.debug.auto と同じ）
    pub auto: bool,
    /// 自動で出陣する（bot は動かさない。見た目の比較用）
    pub start: bool,
    /// 種を固定する（比較用）
    pub seed: Option<u32>,
    /// 1/60 秒の決まった刻みで進める（__game.advance と同じ）
    pub fixed: bool,
    /// 色の的の大きさを固定（解像度を下げる処理も止める）
    pub size: Option<(u32, u32)>,
    /// 主役の id
    pub hero: Option<String>,
    /// 武器の id
    pub weapon: Option<String>,
    /// Studio の「魚群繚乱で遊ぶ」から来た直後（送られたキャラを選んだ状態で始める）
    pub from_studio: bool,
    /// この秒だけ記録して JSON へ書いて終わる
    pub bench_secs: Option<f32>,
    /// このコマ数だけ記録して終わる（--fixed と組み合わせると、同じ量の戦いを描き切る時間を測れる）
    pub bench_frames: Option<usize>,
    pub bench_out: Option<String>,
    /// 垂直同期を切る
    pub no_vsync: bool,
    /// 画面を PNG に残す: (経過秒, 保存先)
    pub shots: Vec<(f32, String)>,
    /// 砦を落としておく数（確かめ用）
    pub capture: usize,
    pub ranbu_at: Option<f32>,
    /// この秒に、今の戦いの結果の画面を出す（確かめ用。Mac 版で結果の画面の流れを見る。ブラウザは __game.result()）
    pub result_at: Option<f32>,
    pub teleport: Option<(f64, f64)>,
    /// 無敵
    pub god: bool,
    /// 端の安全域（確かめ用。Mac 版の --safe 上,右,下,左。無ければ 0）。ブラウザ版は使わない（ページの値と ?safe= を共通部品の page::safe_area が読む）
    pub safe: Option<Safe>,
}

// ---- 自己ベストなどの保存（localStorage の役。platform::store） ----
#[derive(Default)]
pub struct Store;
impl Store {
    fn open() -> Self { Store }
    fn get_str(&self, k: &str) -> Option<String> { store::get(k) }
    /// 今の版の JSON.parse。読めなければ無い扱い
    fn get_json(&self, k: &str) -> Option<Value> { store::get(k).and_then(|t| serde_json::from_str(&t).ok()) }
    fn set(&mut self, k: &str, v: &str) { store::set(k, v); }
}
/// 今の版の String(数): 整数は .0 を付けない（f64 の Display は JS の数の書き方と同じ）
fn num(v: f64) -> String { format!("{v}") }

/// setTimeout で遅らせていた見せ方
pub(crate) enum Delayed {
    Wave { x: f32, z: f32 },
    /// 糸斬りのあと遅れて砂糖が割れる（斬った向き dx・dz、何回目か）
    Shard { x: f32, y: f32, z: f32, dx: f32, dz: f32, n: u8, live: bool },
    /// 猪鹿蝶の見出しと一閃（衝撃のコマ → 斬線 → 砂糖が割れる、のあと）
    Finale,
    Detonate { x: f32, z: f32, i: i32, live: bool },
    BossTrack,
}

/// xorshift（見た目だけの乱数）
pub(crate) struct Xs(u32);
impl Xs { fn f(&mut self) -> f32 { if let Some(v) = crate::platform::js_random() { return v; } let mut x = self.0; x ^= x << 13; x ^= x >> 17; x ^= x << 5; self.0 = x; x as f32 / u32::MAX as f32 } }

fn lerpv(a: DVec3, b: DVec3, k: f64) -> DVec3 { a + (b - a) * k }

pub(crate) struct LoadedActor { id: String, actor: Result<CharacterActor, String> }

/// 主役を部品で読む: 配布の主役は一覧から、Studio で作ったキャラは送られた ZIP から。GPU は触らない
#[cfg(not(target_arch = "wasm32"))]
fn load_actor(cat: &Catalog, h: &HeroInfo) -> Result<CharacterActor, String> {
    let gait = gait_for(h.height);
    match &h.package { Some(asset) => guest_actor(asset, gait), None => cat.hero(&h.id, gait) }
}
#[cfg(target_arch = "wasm32")]
async fn load_actor(cat: &Catalog, h: &HeroInfo) -> Result<CharacterActor, String> {
    let gait = gait_for(h.height);
    match &h.package { Some(asset) => guest_actor(asset, gait), None => cat.hero_web(&h.id, gait).await }
}
/// 選ぶ画面と名札の絵が配布に無い主役（Studio で作ったキャラ）は、ZIP の見本の絵
fn package_picture(h: &HeroInfo) -> Option<Vec<u8>> { h.package.as_ref().and_then(|a| a.thumbnail.as_ref()).map(|t| t.as_ref().clone()) }


/// 飴の糸: たるんで伸びる、太さがむらのある飴色の糸（短い節をつないで描く。毎コマ出し直す）。sag = 真ん中のたるみ（m）、thin = 太さの倍率
fn taffy(vfx: &mut Vfx, a: Vec3, b: Vec3, sag: f32, thin: f32, t: f32) {
    const N: usize = 28;
    let len = a.distance(b);
    if len < 0.05 || thin <= 0.02 { return; }
    let side = (b - a).cross(Vec3::Y).normalize_or_zero();
    let mut prev = a;
    for i in 1..=N {
        let u = i as f32 / N as f32;
        // 真ん中がたるみ、ゆっくり波打つ
        let droop = (u * std::f32::consts::PI).sin();
        let q = a.lerp(b, u) - Vec3::Y * sag * droop + side * (u * 9.0 - t * 7.0).sin() * 0.16 * droop * (len / 8.0).min(1.0);
        // 太さ: 両端は太く付き、途中を飴の玉がゆっくり流れる
        let um = u - 0.5 / N as f32;
        let ends = 1.0 + 0.7 * (1.0 - (um * std::f32::consts::PI).sin()).powi(3);
        let bead = 1.0 + 0.8 * (0.5 + 0.5 * (um * 11.0 - t * 4.0).sin()).powi(3);
        vfx.taffy(prev, q, hex("#ffae38"), 0.34 * ends * bead * thin, 0.82, u * 14.0 - t * 5.0);
        prev = q;
    }
}

/// 読み込みの途中（1 コマに 1 段ずつ進めて、進み具合を見せる）
#[derive(Default)]
pub(crate) struct Loader {
    step: u32,
    world: Option<World>,
    horde: Option<Horde>,
    boss_view: Option<BossView>,
    vfx: Option<Vfx>,
    hero: Option<HeroView>,
    rank_blade: Option<RankBlade>,
    mirrors: Option<Mirrors>,
    items: Option<Items>,
    sky: Option<Sky>,
    catalog: Option<Arc<Catalog>>,
    heroes: Vec<HeroInfo>,
    hero_i: usize,
    error: bool,
    heroes_rx: Option<Receiver<Result<(Catalog, Vec<crate::delivery::HeroEntry>), String>>>,
    actor_rx: Option<Receiver<Result<CharacterActor, String>>>,
}

/// 比較の記録
#[derive(Default)]
pub(crate) struct Perf {
    start: Option<Instant>,
    load_ms: f64,
    frames: Vec<[f32; 5]>,
    enemies_sum: u64,
    particles_sum: u64,
    cpu_start: f64,
}

pub struct App {
    pub gfx: Gfx,
    pub(crate) window: Arc<Window>,
    pub(crate) ui: UiLayer,
    pub(crate) input: Input,
    pub(crate) audio: Audio,
    pub(crate) hud: Hud,
    pub(crate) screens: Screens,
    pub(crate) opts: Options,
    pub(crate) store: Store,
    pub(crate) loader: Loader,
    pub(crate) g: Option<Game>,
    pub(crate) t0: Instant,
    pub(crate) last_ms: f64,
    /// このコマの刻み（HUD の時計）
    pub(crate) frame_dt: f64,
    pub(crate) perf: Perf,
    pub quit: bool,
    pub(crate) pending_shot: Option<String>,
    pub(crate) last_cam: (f64, f64),
    pub(crate) wait_ms: f32,
    in_tick: bool,
    /// 確かめ用: 前に読んでから起きた出来事の名前（window.__game.events）
    #[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
    pub(crate) ev_log: Vec<&'static str>,
    /// 確かめ用: このコマの仕上げの値
    pub(crate) last_final: FinalU,
    /// 端の安全域（ノッチ・ホームバー）
    pub(crate) safe: Safe,
    /// 結果の画面の自己ベストの祝いの流れ（格の判 → 得点の数え上げ → 祝い）。結果の画面が出ていない間は何も起こさない
    show: ResultShow,
    /// このコマの実時間（秒）。祝いの流れは戦いの刻みではなく実時間で進める
    real_dt: f64,
}

/// 読み込みが終わってからの全部
pub(crate) struct Game {
    pub(crate) world: World,
    pub(crate) horde: Horde,
    pub(crate) boss_view: BossView,
    pub(crate) boss_fx: BossFx,
    pub(crate) vfx: Vfx,
    pub(crate) hero: HeroView,
    pub(crate) rank_blade: RankBlade,
    pub(crate) mirrors: Mirrors,
    pub(crate) items: Items,
    pub(crate) sky: Sky,
    pub(crate) catalog: Arc<Catalog>,
    pub(crate) heroes: Vec<HeroInfo>,
    pub(crate) cards: Vec<HeroCard>,
    pub(crate) thumbs: Vec<Option<Tex>>,
    pub(crate) pics_rx: Receiver<(usize, Option<Vec<u8>>, Option<Vec<u8>>)>,
    pub(crate) actor_tx: Sender<LoadedActor>,
    pub(crate) actor_rx: Receiver<LoadedActor>,
    pub(crate) weapon_tex: HashMap<String, Tex>,
    pub(crate) hero_i: usize,
    pub(crate) weapon_id: String,
    pub(crate) loading_hero: bool,
    // 今の版の tick は始めに const b = battle で持つので、キーで出陣したコマは前の戦いのまま最後まで進んで描く。新しい戦いが動くのは次のコマから
    battle_old: Option<Battle>,
    battle_next: Option<Battle>,
    pub(crate) battle: Battle,
    pub(crate) bot: Bot,
    pub(crate) mode: Mode,
    pub(crate) theme: LinearTheme,
    pub(crate) boss01: f64,
    pub(crate) cam_yaw: f64,
    pub(crate) cam_idle: f64,
    pub(crate) cam_pos: DVec3,
    pub(crate) cam_look: DVec3,
    pub(crate) camera: Camera,
    pub(crate) trauma: f64,
    pub(crate) zoom_blur: f64,
    pub(crate) speed: f64,
    pub(crate) flash: f64,
    pub(crate) red_k: f64,
    pub(crate) gold: f64,
    pub(crate) fov_kick: f64,
    pub(crate) chain_cam: f64,
    pub(crate) giant_cam: f64,
    pub(crate) chain_at: DVec3,
    pub(crate) cutin_t: f64,
    pub(crate) boss_intro_t: f64,
    /// 体力が半分を切ったときの見上げのカット（負 = していない）
    pub(crate) boss_cut_t: f64,
    pub(crate) result_t: f64,
    pub(crate) defeat_t: f64,
    pub(crate) title_t: f64,
    pub(crate) t_all: f64,
    pub(crate) tint: [f32; 3],
    pub(crate) loaded: HashSet<i64>,
    /// setTimeout の役: 鳴らす時刻（ms）と中身
    pub(crate) delayed: Vec<(f64, Delayed)>,
    pub(crate) rng: Xs,
    pub(crate) auto: bool,
    pub(crate) bot_seed: u32,
    pub(crate) frame_i: u64,
    /// このコマの const live（入力のあと、敗北・結果へ移る前に決まる。今の版の tick の live）
    pub(crate) live: bool,
    /// 確かめ用: 1 コマだけ別の場所から撮る（位置・見る点）
    pub(crate) closeup: Option<(Vec3, Vec3)>,
    /// 糸斬りのあと、的と主役の間に伸びて残る飴の糸（的の場所・斬ってからの秒）
    taffy: Option<(Vec3, f64)>,
    /// 猪鹿蝶の決め絵のカメラ（的のまわりを回る）: 的の場所・始めてからの秒・始めの向き
    fin_cam: Option<(DVec3, f64, f64)>,
    /// 飴糸で飛ぶ間のカメラの寄せ具合（0..1）
    wire_cam: f64,
    /// 三撃目へ斬り込む飛びで、カメラを切り替えた飛び（move_key）
    wire_cut: i64,
    /// 連続で倒している間の寄り（0..1）。主役を大きく見せる
    close_cam: f64,
}

fn cam_init() -> (DVec3, DVec3) { (DVec3::new(0.0, 8.0, START.z + 12.0), DVec3::new(0.0, 1.0, START.z)) }

impl App {
    pub fn new(window: Arc<Window>, mut gfx: Gfx, opts: Options) -> App {
        if let Some((w, h)) = opts.size { gfx.set_fixed(w, h); }
        if opts.bench_secs.is_some() || opts.bench_frames.is_some() { gfx.adapt_on = false; }
        let ui = UiLayer::new(&gfx.device, gfx.config.format, &window, crate::ui::install_fonts);
        let mut input = Input::new();
        input.set_viewport(window.scale_factor(), window.inner_size().width, window.inner_size().height);
        let screens = Screens::new(ui.ctx());
        let audio = Audio::new();
        let now = Instant::now();
        let mut app = App { in_tick: false, gfx, window, ui, input, audio, hud: Hud::new(), screens, opts, store: Store::open(), loader: Loader::default(), g: None, t0: now, last_ms: clock_ms(), frame_dt: 0.0, perf: Perf::default(), quit: false, pending_shot: None, last_cam: (0.0, 0.0), wait_ms: 0.0, ev_log: Vec::new(), last_final: FinalU::default(), safe: Safe::default(), show: ResultShow::idle(), real_dt: 0.0 };
        app.read_safe();
        app
    }

    pub fn on_event(&mut self, e: &WindowEvent) {
        // 最後に触った機器は、UI が受けた出来事（画面のボタンの上のクリック・指）も含めて追う
        self.input.note_device(e);
        if !self.ui.on_event(&self.window, e) { self.input.on_event(e); }
        if let WindowEvent::Resized(s) = e {
            self.gfx.resize(s.width, s.height, self.window.scale_factor() as f32);
            self.input.set_viewport(self.window.scale_factor(), s.width, s.height);
            // 端末の向きで安全域が変わる
            self.read_safe();
        }
    }

    /// 端の安全域を読んで、入力・HUD・画面へ渡す（起動のときと、窓の大きさが変わったとき。毎コマは読まない）。
    /// ブラウザはページから（確かめ用の ?safe=上,右,下,左 も共通部品の page::safe_area が読む）。Mac 版は --safe（無ければ 0）
    fn read_safe(&mut self) {
        #[cfg(target_arch = "wasm32")]
        let safe = Safe::from_array(game_ui::page::safe_area());
        #[cfg(not(target_arch = "wasm32"))]
        let safe = self.opts.safe.unwrap_or_default();
        self.set_safe(safe);
    }
    pub(crate) fn set_safe(&mut self, safe: Safe) {
        self.safe = safe;
        self.input.set_safe(safe.to_array());
        self.hud.set_safe(safe);
        self.screens.set_safe(safe);
    }

    /// 結果の画面の自己ベストの祝いを進める（毎コマ）。結果の画面を離れたら打ち切る（ボタンを押した・確かめ用の口で移った）
    fn cheer_tick(&mut self) {
        if self.g.as_ref().is_none_or(|g| g.mode != Mode::Result) {
            self.show.cancel();
            self.screens.cheer_clear();
            return;
        }
        for e in self.show.step(self.real_dt) {
            let name = match e {
                CheerEvent::Tick(k) => { self.audio.count_tick(k); "cheerTick" }
                CheerEvent::Done(Cheer::None) => { self.audio.count_done(); self.screens.cheer_done(false); "cheerDone" }
                CheerEvent::Done(c) => { self.audio.cheer(c == Cheer::BestTop); self.screens.cheer_done(true); if c == Cheer::BestTop { "cheerBestTop" } else { "cheerBest" } }
                CheerEvent::Encore => { self.audio.cheer_encore(); self.screens.cheer_encore(); "cheerEncore" }
            };
            #[cfg(target_arch = "wasm32")]
            self.ev_log.push(name);
            #[cfg(not(target_arch = "wasm32"))]
            let _ = name;
        }
        self.screens.cheer_count(self.show.value());
    }

    /// 最後に触った機器を、HUD と画面の表記・指の部品へ伝える
    fn sync_device(&mut self) {
        let (d, ring) = (self.input.device(), self.input.ring());
        self.hud.set_device(d);
        self.screens.set_device(d, ring);
    }

    /// 読み込み直す: ブラウザはページの読み直し。Mac 版は止まった段からやり直す
    fn reload(&mut self) {
        #[cfg(target_arch = "wasm32")]
        if let Some(w) = web_sys::window() { let _ = w.location().reload(); }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let l = &mut self.loader;
            l.error = false; l.heroes_rx = None; l.actor_rx = None;
            self.screens.load_failed(None);
            self.screens.progress(0.5, Some("読み込み直しています…"));
        }
    }

    /// 読み込みの 1 段。Game が出来たら true
    fn load_step(&mut self) -> bool {
        let gfx = &self.gfx;
        let l = &mut self.loader;
        if l.error { return false; }
        // main.ts の startApp().catch: 読み込みに失敗しました: 理由
        let fail = |s: &mut Screens, e: String| { eprintln!("{e}"); s.load_failed(Some(&format!("読み込みに失敗しました: {e}"))); };
        match l.step {
            0 => { self.screens.progress(0.08, Some("魚の大群を集めています…")); }
            1 => {
                let b = create_battle(1, false);
                l.sky = Some(Sky::new(gfx));
                l.world = Some(World::new(gfx, &b.holds));
                self.screens.progress(0.3, None);
            }
            2 => {
                match Horde::new(gfx) { Ok(h) => l.horde = Some(h), Err(e) => { l.error = true; fail(&mut self.screens, e); return false; } }
                self.screens.progress(0.45, Some("大将を呼んでいます…"));
            }
            3 => {
                match BossView::new(gfx) { Ok(h) => l.boss_view = Some(h), Err(e) => { l.error = true; fail(&mut self.screens, e); return false; } }
                l.vfx = Some(Vfx::new(gfx));
                l.hero = Some(HeroView::new(gfx));
                l.rank_blade = Some(RankBlade::new(gfx));
                l.mirrors = Some(Mirrors::new(gfx));
                l.items = Some(Items::new(gfx));
                self.screens.progress(0.55, Some("武将の名簿を読んでいます…"));
            }
            4 => {
                // 名簿は裏で取る（届くまでこの段で待つ）
                if l.heroes_rx.is_none() {
                    let (tx, rx) = channel();
                    #[cfg(not(target_arch = "wasm32"))]
                    std::thread::spawn(move || {
                        // 主役は手元の配布から読む（docs/gyogun-spec.md）。控えは残さない
                        let url = std::env::var("GYOGUN_CATALOG").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| assets::CATALOG_URL_NATIVE.to_string());
                        let _ = tx.send(Catalog::open(&url, None, None).and_then(|c| { let list = c.heroes()?; Ok((c, list)) }));
                    });
                    #[cfg(target_arch = "wasm32")]
                    wasm_bindgen_futures::spawn_local(async move {
                        // 主役は、ゲームを配っている同じ場所の配布から読む（docs/gyogun-spec.md）
                        let r = async {
                            let href = web_sys::window().and_then(|w| w.location().href().ok()).ok_or("ページの場所が分かりません")?;
                            let c = Catalog::open_index_web(&assets::catalog_url_for_page(&href), false)?;
                            let list = c.heroes_web().await?;
                            Ok((c, list))
                        }.await;
                        let _ = tx.send(r);
                    });
                    l.heroes_rx = Some(rx);
                }
                match l.heroes_rx.as_ref().unwrap().try_recv() {
                    Err(_) => return false,
                    Ok(Ok((c, list))) if !list.is_empty() => {
                        l.catalog = Some(Arc::new(c));
                        l.heroes = to_hero_list(list);
                        // Studio で作ったキャラ（最後に送った 1 体）を並べる。控えが無い・読めないときは、配布の主役だけで始める
                        let guest = crate::platform::studio_zip().and_then(|b| guest_hero(&b).map_err(|e| eprintln!("Studio のキャラを読めません: {e}")).ok());
                        let fresh = self.opts.from_studio && guest.is_some();
                        if let Some((info, weapon)) = guest {
                            // Studio から来た直後: そのキャラと Studio で選んだ武器（持たない・知らない武器はゲームの既定）で始める。開き直しても同じ
                            if fresh {
                                self.store.set("gyogun.hero", GUEST_ID);
                                self.store.set(&format!("gyogun.weapon.{GUEST_ID}"), weapon.or(default_weapon(GUEST_ID)).unwrap_or(WEAPONS[0].id));
                            }
                            l.heroes.push(info);
                        }
                        let saved = self.store.get_str("gyogun.hero");
                        let want = if fresh { Some(GUEST_ID.to_string()) } else { self.opts.hero.clone().or(saved) };
                        if let Some(w) = want { if let Some(i) = l.heroes.iter().position(|h| h.id == w) { l.hero_i = i; } }
                        let name = l.heroes[l.hero_i].name.clone();
                        self.screens.progress(0.65, Some(&format!("{name} が支度をしています…")));
                    }
                    Ok(other) => {
                        if let Err(e) = other { eprintln!("{e}"); }
                        l.error = true;
                        self.screens.load_failed(Some("キャラクターの配布に接続できません。通信を確かめて、読み込み直してください。"));
                        return false;
                    }
                }
            }
            5 => {
                let h = l.heroes[l.hero_i].clone();
                if l.actor_rx.is_none() {
                    let (tx, rx) = channel();
                    let (cat, who) = (l.catalog.as_ref().unwrap().clone(), h.clone());
                    #[cfg(not(target_arch = "wasm32"))]
                    std::thread::spawn(move || { let _ = tx.send(load_actor(&cat, &who)); });
                    #[cfg(target_arch = "wasm32")]
                    wasm_bindgen_futures::spawn_local(async move { let _ = tx.send(load_actor(&cat, &who).await); });
                    l.actor_rx = Some(rx);
                }
                let actor = match l.actor_rx.as_ref().unwrap().try_recv() { Ok(a) => a, Err(_) => return false };
                let weapon = self.opts.weapon.clone().unwrap_or_else(|| weapon_of(&self.store, &h.id));
                // 今の版は最初の主役の武器が読めないと始まらない（await hero.setHero が startApp を止める）
                if let Err(e) = assets::load_glb(&format!("{BASE}models/wilds/weapons/{}.glb", WEAPONS.iter().find(|w| w.id == weapon).unwrap_or(&WEAPONS[0]).id)) { l.error = true; fail(&mut self.screens, e); return false; }
                match actor {
                    Ok(a) => l.hero.as_mut().unwrap().set_hero(gfx, &h, a, &weapon),
                    Err(e) => { l.error = true; fail(&mut self.screens, e); return false; }
                }
                self.screens.progress(1.0, None);
            }
            _ => return true,
        }
        l.step += 1;
        false
    }

    fn finish_load(&mut self) {
        let l = std::mem::take(&mut self.loader);
        let heroes = l.heroes;
        let hero_i = l.hero_i;
        let cat = l.catalog.unwrap();
        // 武将の絵は裏で取りに行く（ブラウザの img と同じく、読めたものから出る）
        let (ptx, pics_rx) = channel();
        {
            let (cat, list) = (cat.clone(), heroes.clone());
            #[cfg(not(target_arch = "wasm32"))]
            std::thread::spawn(move || {
                for (i, h) in list.iter().enumerate() {
                    let s = h.select_file.as_ref().and_then(|f| cat.read_picture(f).ok()).or_else(|| package_picture(h));
                    let t = h.thumb_file.as_ref().and_then(|f| cat.read_picture(f).ok()).or_else(|| package_picture(h));
                    if ptx.send((i, s, t)).is_err() { break; }
                }
            });
            #[cfg(target_arch = "wasm32")]
            wasm_bindgen_futures::spawn_local(async move {
                for (i, h) in list.iter().enumerate() {
                    let s = match &h.select_file { Some(f) => cat.read_picture_web(f).await.ok(), None => package_picture(h) };
                    let t = match &h.thumb_file { Some(f) => cat.read_picture_web(f).await.ok(), None => package_picture(h) };
                    if ptx.send((i, s, t)).is_err() { break; }
                }
            });
        }
        let (actor_tx, actor_rx) = channel();
        let cards: Vec<HeroCard> = heroes.iter().map(|h| HeroCard { name: h.name.clone(), select: None }).collect();
        self.screens.set_heroes(cards.clone());
        let weapon_id = self.opts.weapon.clone().unwrap_or_else(|| weapon_of(&self.store, &heroes[hero_i].id));
        let (cam_pos, cam_look) = cam_init();
        let battle = create_battle(7, false);
        let bot = create_bot(&battle, 1.1, 7);
        let aspect = self.gfx.aspect();
        let tint = hex(&heroes[hero_i].tint);
        self.g = Some(Game {
            world: l.world.unwrap(), horde: l.horde.unwrap(), boss_view: l.boss_view.unwrap(), boss_fx: BossFx::new(&self.gfx), vfx: l.vfx.unwrap(), hero: l.hero.unwrap(), rank_blade: l.rank_blade.unwrap(),
            mirrors: l.mirrors.unwrap(), items: l.items.unwrap(), sky: l.sky.unwrap(), catalog: cat, thumbs: vec![None; heroes.len()], heroes, cards, pics_rx, actor_tx, actor_rx,
            weapon_tex: HashMap::new(), hero_i, weapon_id, loading_hero: false, battle_old: None, battle_next: None, battle, bot, mode: Mode::Title, theme: lerp_theme(0.0), boss01: 0.0,
            cam_yaw: 0.0, cam_idle: 0.0, cam_pos, cam_look, camera: Camera::new(DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 0.0, -1.0), 0.0, 52.0, aspect, glam::DMat4::IDENTITY),
            trauma: 0.0, zoom_blur: 0.0, speed: 0.0, flash: 0.0, red_k: 0.0, gold: 0.0, fov_kick: 0.0, chain_cam: 0.0, giant_cam: 0.0, chain_at: DVec3::ZERO,
            cutin_t: -1.0, boss_intro_t: -1.0, boss_cut_t: -1.0, result_t: -1.0, defeat_t: -1.0, title_t: 0.0, t_all: 0.0, tint, loaded: HashSet::new(), delayed: Vec::new(), rng: Xs(0x9e3779b9), auto: self.opts.auto, bot_seed: 7, frame_i: 0, live: false, closeup: None, taffy: None, fin_cam: None, wire_cam: 0.0, wire_cut: -1, close_cam: 0.0,
        });
        self.perf.load_ms = self.t0.elapsed().as_secs_f64() * 1000.0;
        // ブラウザはページを開いてから（素材の先読みも含む）の時間
        #[cfg(target_arch = "wasm32")]
        if let Some(p) = web_sys::window().and_then(|w| w.performance()) { self.perf.load_ms = p.now(); }
        self.apply_theme(0.0);
        self.to_title();
        if self.opts.auto || self.opts.start { self.go(); }
    }

    fn apply_theme(&mut self, k: f64) {
        let g = self.g.as_mut().unwrap();
        g.theme = lerp_theme(k as f32);
        g.sky.set_linear(&g.theme, 0.0, 0.0);
        g.sky.set_fog(g.theme.fog);
    }

    /// このコマの残りは前の戦いで進める（コマの終わりに新しい戦いへ替える）
    fn keep_old_battle(&mut self) {
        let g = self.g.as_mut().unwrap();
        if let Some(old) = g.battle_old.take() { g.battle_next = Some(std::mem::replace(&mut g.battle, old)); }
    }

    pub(crate) fn new_battle(&mut self, demo: bool) {
        let g = self.g.as_mut().unwrap();
        let seed = if demo { 7 } else { self.opts.seed.unwrap_or_else(|| SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(1) % 1_000_000_000) };
        let old = std::mem::replace(&mut g.battle, create_battle(seed, self.opts.god));
        if self.in_tick && g.battle_old.is_none() { g.battle_old = Some(old); }
        g.bot_seed = g.bot_seed.wrapping_add(1);
        g.taffy = None; g.fin_cam = None; g.wire_cam = 0.0;
        // Studio で作ったキャラには決め台詞の声が無い（読みに行かない）
        if !demo && g.heroes[g.hero_i].package.is_none() { self.audio.load_voice(&g.heroes[g.hero_i].id); }
        g.bot = create_bot(&g.battle, 1.1, seed ^ 0x5bd1e995);
        if crate::platform::js_random().is_some() { g.bot.source = Some(|| crate::platform::js_random().unwrap_or(0.0) as f64); }
        g.cam_yaw = 0.0; g.boss01 = 0.0;
        g.cutin_t = -1.0; g.boss_intro_t = -1.0; g.boss_cut_t = -1.0; g.result_t = -1.0; g.defeat_t = -1.0;
        g.boss_fx.reset();
        g.cam_pos = cam_init().0;
        self.hud.reset();
        self.screens.letterbox(false);
        self.apply_theme(0.0);
    }

    // ---- タイトル ----
    fn best(&self) -> Option<Value> { self.store.get_json("gyogun.best").filter(|v| v.is_object()) }
    pub(crate) fn to_title(&mut self) {
        self.new_battle(true);
        self.hud.show(false);
        self.screens.show(Some(Screen::Title));
        let b = self.best();
        let g = self.g.as_mut().unwrap();
        g.mode = Mode::Title; g.title_t = 0.0;
        let mut honors = 0;
        for h in &g.heroes {
            if let Some(a) = self.store.get_json(&format!("gyogun.honors.{}", h.id)).as_ref().and_then(|v| v.as_array()) { honors += a.iter().filter(|x| x.as_bool() == Some(true)).count(); }
        }
        let text = b.map(|b| format!("最高 {} ／ {} 点 ／ {} 人斬り ／ 誉れ {} / {}", b["rank"].as_str().unwrap_or(""), fmt_num(b["score"].as_i64().unwrap_or(0)), b["kos"].as_i64().unwrap_or(0), honors, g.heroes.len() * 5)).unwrap_or_default();
        self.screens.set_title_best(text);
        self.screens.set_muted(self.audio.muted());
        self.audio.play_track(Track::Title);
    }

    // ---- 武将選び ----
    pub(crate) fn pick_hero(&mut self, i: i32) {
        let weapon = {
            let g = self.g.as_mut().unwrap();
            let n = g.heroes.len() as i32;
            g.hero_i = ((i + n) % n) as usize;
            g.heroes[g.hero_i].id.clone()
        };
        let w = weapon_of(&self.store, &weapon);
        let g = self.g.as_mut().unwrap();
        let h = g.heroes[g.hero_i].clone();
        self.screens.pick_hero(g.hero_i);
        self.screens.set_sel_name(&h.name);
        g.weapon_id = w;
        self.sync_weapon();
        self.audio.select();
        let g = self.g.as_mut().unwrap();
        if g.hero.info().map(|x| x.id.as_str()) == Some(h.id.as_str()) { return; }
        g.loading_hero = true;
        self.screens.set_sel_name(&format!("{}…", h.name));
        let (cat, tx) = (g.catalog.clone(), g.actor_tx.clone());
        #[cfg(not(target_arch = "wasm32"))]
        std::thread::spawn(move || { let actor = load_actor(&cat, &h); let _ = tx.send(LoadedActor { id: h.id, actor }); });
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move { let actor = load_actor(&cat, &h).await; let _ = tx.send(LoadedActor { id: h.id, actor }); });
    }

    fn sync_weapon(&mut self) {
        let ctx = self.ui.ctx().clone();
        let g = self.g.as_mut().unwrap();
        let w = WEAPONS.iter().find(|x| x.id == g.weapon_id).unwrap_or(&WEAPONS[0]);
        if !g.weapon_tex.contains_key(w.id) {
            if let Some(t) = assets::read(&format!("{BASE}models/wilds/weapons/{}.png", w.id)).ok().and_then(|b| load_image(&ctx, w.id, &b)) { g.weapon_tex.insert(w.id.to_string(), t); }
        }
        let kind = match w.path { Path::Straight => Some(PathKind::Straight), Path::Spread => Some(PathKind::Spread), Path::Curve => Some(PathKind::Curve), Path::None => None };
        let path = kind.and_then(|k| path_info(w.path).map(|i| PathCard { kind: k, name: i.name.to_string(), desc: i.desc.to_string() }));
        self.screens.set_weapon(WeaponCard { name: w.name.to_string(), image: g.weapon_tex.get(w.id).cloned(), path });
    }

    pub(crate) fn step_weapon(&mut self, d: i32) {
        let g = self.g.as_mut().unwrap();
        let n = WEAPONS.len() as i32;
        let i = WEAPONS.iter().position(|x| x.id == g.weapon_id).unwrap_or(0) as i32;
        g.weapon_id = WEAPONS[((i + d + n) % n) as usize].id.to_string();
        let (id, hero) = (g.weapon_id.clone(), g.heroes[g.hero_i].id.clone());
        g.hero.set_weapon(&self.gfx, &id);
        self.store.set(&format!("gyogun.weapon.{hero}"), &id);
        self.sync_weapon();
        self.audio.select();
    }

    pub(crate) fn to_select(&mut self) {
        self.new_battle(true);
        self.screens.show(Some(Screen::Select));
        self.hud.show(false);
        let g = self.g.as_mut().unwrap();
        g.mode = Mode::Select;
        let i = g.hero_i as i32;
        self.pick_hero(i);
    }

    pub(crate) fn go(&mut self) {
        if self.g.as_ref().unwrap().loading_hero { return; }
        let (id, name, thumb, weapon) = { let g = self.g.as_ref().unwrap(); let h = &g.heroes[g.hero_i]; (h.id.clone(), h.name.clone(), g.thumbs[g.hero_i].clone(), g.weapon_id.clone()) };
        self.store.set("gyogun.hero", &id);
        self.audio.confirm();
        self.new_battle(false);
        let g = self.g.as_mut().unwrap();
        g.battle.set_path(WEAPONS.iter().find(|x| x.id == weapon).map(|x| x.path).unwrap_or(Path::None));
        g.mode = Mode::Battle;
        g.trauma = 0.3;
        for i in 0..self.opts.capture { g.battle.debug_capture(i); }
        if let Some((x, z)) = self.opts.teleport { g.battle.debug_teleport(x, z); }
        self.screens.show(None);
        self.hud.show(true);
        self.hud.set_hero(&name, thumb.clone());
        self.hud.set_rank(0);
        let place = STAGE.name();
        self.hud.say("出陣", &format!("{place} ── {name}、敵の砦を四つ落とし、大将を討て"), Tone::Gold);
        self.audio.play_track(Track::Field);
        self.screens.set_cutin_hero(thumb, &name);
    }

    pub(crate) fn pause(&mut self, on: bool) {
        let g = self.g.as_mut().unwrap();
        if on && g.mode == Mode::Battle { g.mode = Mode::Paused; self.screens.show(Some(Screen::Pause)); }
        else if !on && g.mode == Mode::Paused { g.mode = Mode::Battle; self.screens.show(None); }
    }

    pub(crate) fn do_continue(&mut self) {
        let g = self.g.as_mut().unwrap();
        g.battle.revive();
        g.mode = Mode::Battle;
        let boss = g.battle.phase == Phase::Boss;
        self.screens.show(None);
        self.hud.say("再起", "", Tone::Gold);
        self.audio.play_track(if boss { Track::Boss } else { Track::Field });
    }

    pub(crate) fn show_result(&mut self) {
        let prev = self.best();
        let g = self.g.as_mut().unwrap();
        let r = g.battle.result();
        let hero_id = g.heroes[g.hero_i].id.clone();
        let gi = |k: &str| prev.as_ref().and_then(|p| p[k].as_i64());
        let is_new = gi("score").map(|s| r.score as i64 > s).unwrap_or(true);
        // JSON.stringify と同じ順と書き方
        if is_new { self.store.set("gyogun.best", &format!("{{\"score\":{},\"kos\":{},\"rank\":\"{}\",\"time\":{},\"combo\":{}}}", num(r.score), r.kos, r.rank, num(r.time), r.max_combo)); }
        // 最大連鎖は得点と別に、いちばん大きいものを残す
        // Number(保存 ?? -1)。壊れた保存は NaN（今の版のまま）
        let prev_chain = self.store.get_str("gyogun.bestChain").map_or(-1.0, |t| t.trim().parse::<f64>().unwrap_or(f64::NAN));
        if (r.max_chain as f64) > prev_chain { self.store.set("gyogun.bestChain", &r.max_chain.to_string()); }
        let mmss = format!("{}:{:02}", (r.time / 60.0).floor() as i64, (r.time % 60.0).floor() as i64);
        let row = |k: &str, v: String, b: String| ResultRow { label: k.to_string(), value: v, best: b };
        let cmp = |now: i64, p: Option<i64>| match p { Some(p) if now > p => "NEW".to_string(), Some(p) => format!("最高 {p}"), None => String::new() };
        let rows = vec![
            row("撃破", r.kos.to_string(), cmp(r.kos, gi("kos"))),
            row("最大連撃", r.max_combo.to_string(), cmp(r.max_combo, gi("combo"))),
            row("最大連鎖", r.max_chain.to_string(), if prev_chain < 0.0 { String::new() } else if (r.max_chain as f64) > prev_chain { "NEW".into() } else { format!("最高 {}", num(prev_chain)) }),
            row("連鎖の得点", fmt_num(r.chain_bonus as i64), String::new()),
            row("時間", mmss, String::new()),
            row("砦", format!("{} / 4", r.captures), String::new()),
            row("格", ["初陣", "壱", "弐", "参"][r.rank_reached.clamp(0, 3) as usize].to_string(), String::new()),
            row("受けた傷", format!("{}", r.dmg_taken as i64), String::new()),
            row("得点", fmt_num(r.score as i64), if is_new { "NEW".into() } else { format!("最高 {}", fmt_num(gi("score").unwrap_or(0))) }),
        ];
        // 誉れ: 武将ごとに集める（一度取れば残る）
        let names = ["千人斬り", "五分で決着", "傷は浅い（1500以下）", "三百連撃", "百連鎖"];
        let key = format!("gyogun.honors.{hero_id}");
        let saved: Vec<bool> = self.store.get_json(&key).as_ref().and_then(|v| v.as_array()).map(|a| a.iter().map(|x| x.as_bool() == Some(true)).collect()).unwrap_or_default();
        let got: Vec<bool> = (0..5).map(|i| saved.get(i).copied().unwrap_or(false) || r.honors[i]).collect();
        let honors = (0..5).map(|i| Honor { name: names[i].to_string(), on: got[i], fresh: r.honors[i] && !saved.get(i).copied().unwrap_or(false) }).collect();
        self.store.set(&key, &json!(got).to_string());
        // 主な数は得点（rows の最後）。自己ベストの祝いの条件は、得点に NEW を出す条件と同じ（is_new）
        let main = rows.len() - 1;
        self.screens.set_result(ResultData { eyebrow: if r.won { "VICTORY" } else { "RETREAT" }.into(), title: if r.won { "勝鬨" } else { "退却" }.into(), rank: r.rank.to_string(), rows, honors, main: Some(main) });
        self.g.as_mut().unwrap().mode = Mode::Result;
        self.screens.show(Some(Screen::Result));
        // 端末の「動きを減らす」設定はブラウザだけ。Mac 版はいつも通常の動き
        #[cfg(target_arch = "wasm32")]
        let calm = game_ui::page::reduced_motion();
        #[cfg(not(target_arch = "wasm32"))]
        let calm = false;
        // 流れは共通部品（game_ui::cheer）。格の判（STAMP_S）を待ってから数える。いちばん上の評価は格の S
        self.show = ResultShow::new(r.score as i64, cheer_of(is_new, top_rank(r.rank)), calm).wait(STAMP_S);
        self.screens.cheer_begin(calm);
        self.hud.show(false);
    }

    pub(crate) fn act(&mut self, a: UiAction) {
        match a {
            UiAction::Start => { self.audio.init(); self.to_select(); }
            UiAction::ToggleSound => { self.audio.init(); self.audio.toggle_mute(); self.screens.set_muted(self.audio.muted()); }
            UiAction::PickHero(i) => self.pick_hero(i as i32),
            UiAction::StepWeapon(d) => self.step_weapon(d),
            UiAction::Go => self.go(),
            UiAction::Resume => self.pause(false),
            UiAction::ToTitle => self.to_title(),
            UiAction::ToSelect => self.to_select(),
            UiAction::Again => self.go(),
            UiAction::Continue => self.do_continue(),
            UiAction::Reload => self.reload(),
            // みんなで: 協力プレイのある元のゲームのページを開く（確認の画面は挟まない）
            UiAction::Online | UiAction::MakeChar => if let Some(url) = crate::ui::link_of(a) { crate::platform::open_url(url); },
        }
    }

    // ---- 主ループ ----
    /// 画面が隠れていて requestAnimationFrame が止まるときは、タイマーで進める（今の版の setInterval。33ms ごとに見て、120ms 進んでいなければ 1 コマ）
    #[cfg(target_arch = "wasm32")]
    pub fn idle_tick(&mut self) { if clock_ms() - self.last_ms > 120.0 { self.frame(); } }

    pub fn frame(&mut self) {
        // 今の版と同じ式: dt = min(0.05, (今 - 前) / 1000)。倍精度のまま
        let now_ms = clock_ms();
        let real_d = (now_ms - self.last_ms) / 1000.0;
        self.last_ms = now_ms;
        self.real_dt = real_d;
        let real = real_d as f32;
        if self.g.is_none() {
            // 読み込みに失敗したときの「読み込み直す」は、キーボード・パッドでも押せる
            let c = self.input.read();
            self.sync_device();
            if let Some(a) = self.screens.nav(false, false, c.confirm_pressed) { self.act(a); }
            if self.load_step() { self.finish_load(); }
            self.paint_loading();
            return;
        }
        let fixed = self.opts.fixed;
        let dt = if fixed { 1.0 / 60.0 } else { real_d.min(0.05) };
        self.frame_dt = dt;
        let t_tick = Instant::now();
        self.hud.advance(real_d);
        self.screens.advance(real_d);
        self.in_tick = true;
        let sim_ms = self.tick(dt);
        self.in_tick = false;
        let tick_ms = t_tick.elapsed().as_secs_f32() * 1000.0;
        let t_draw = Instant::now();
        self.render();
        { let g = self.g.as_mut().unwrap(); if let Some(n) = g.battle_next.take() { g.battle = n; } }
        let draw_ms = t_draw.elapsed().as_secs_f32() * 1000.0 - self.wait_ms;
        self.gfx.adapt(real * 1000.0);
        self.record(real, sim_ms, tick_ms, draw_ms);
    }

    fn record(&mut self, real: f32, sim_ms: f32, tick_ms: f32, draw_ms: f32) {
        if self.opts.bench_secs.is_none() && self.opts.bench_frames.is_none() {
            self.take_shots();
            return;
        }
        let secs = self.opts.bench_secs.unwrap_or(f32::MAX);
        if self.perf.start.is_none() { self.perf.cpu_start = cpu_s(); }
        let start = *self.perf.start.get_or_insert_with(Instant::now);
        let el = start.elapsed().as_secs_f32();
        let g = self.g.as_ref().unwrap();
        self.perf.frames.push([el, real * 1000.0, sim_ms, tick_ms, draw_ms]);
        self.perf.enemies_sum += g.battle.alive_count() as u64;
        self.perf.particles_sum += g.vfx.particle_count() as u64;
        self.take_shots();
        if el >= secs || self.opts.bench_frames.is_some_and(|f| self.perf.frames.len() >= f) {
            let n = self.perf.frames.len().max(1);
            let list = |i: usize| -> Vec<f32> { let mut v: Vec<f32> = self.perf.frames.iter().skip(n / 20).map(|f| f[i]).collect(); v.sort_by(|a, b| a.total_cmp(b)); v };
            let pct = |v: &Vec<f32>, p: f32| v.get(((v.len() as f32 - 1.0) * p) as usize).copied().unwrap_or(0.0);
            let avg = |v: &Vec<f32>| v.iter().sum::<f32>() / v.len().max(1) as f32;
            let summary = |i: usize| { let v = list(i); json!({ "avg": avg(&v), "p50": pct(&v, 0.5), "p95": pct(&v, 0.95), "p99": pct(&v, 0.99), "max": pct(&v, 1.0) }) };
            let (rw, rh) = self.gfx.render_size();
            let g = self.g.as_ref().unwrap();
            let out = json!({
                "runtime": if cfg!(target_arch = "wasm32") { "rust-wasm" } else { "rust-native" }, "frames": n, "seconds": el, "fps": n as f32 / el,
                "render": [rw, rh], "surface": [self.gfx.config.width, self.gfx.config.height],
                "load_ms": self.perf.load_ms,
                "frame_ms": summary(1), "sim_ms": summary(2), "tick_ms": summary(3), "draw_ms": summary(4),
                "avg_alive": self.perf.enemies_sum as f64 / n as f64, "avg_particles": self.perf.particles_sum as f64 / n as f64,
                "kos": g.battle.player.kos, "time": g.battle.time, "rss_kb": rss_kb(), "cpu_s": cpu_s() - self.perf.cpu_start,
            });
            let text = serde_json::to_string_pretty(&out).unwrap_or_default();
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Some(p) = &self.opts.bench_out { let _ = std::fs::write(p, &text); }
                println!("{text}");
            }
            #[cfg(target_arch = "wasm32")]
            if let Some(w) = web_sys::window() { let _ = js_sys::Reflect::set(&w, &"__gyogun_result".into(), &text.as_str().into()); }
            self.quit = true;
        }
    }

    fn take_shots(&mut self) {
        let t = self.g.as_ref().map(|g| g.t_all as f32).unwrap_or(0.0);
        if self.pending_shot.is_none() {
            if let Some(i) = self.opts.shots.iter().position(|s| t >= s.0) { self.pending_shot = Some(self.opts.shots.remove(i).1); }
            
        }
    }

    fn paint_loading(&mut self) {
        let tex = match self.gfx.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => { self.gfx.surface.configure(&self.gfx.device, &self.gfx.config); return; }
            _ => return,
        };
        let ctx = self.ui.begin(&self.window);
        let acts = self.screens.ui(&ctx);
        let view = tex.texture.create_view(&Default::default());
        let mut enc = self.gfx.device.create_command_encoder(&Default::default());
        {
            let _p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.02, g: 0.015, b: 0.05, a: 1.0 }), store: wgpu::StoreOp::Store }, depth_slice: None })],
                ..Default::default()
            });
        }
        self.ui.paint(&self.gfx.device, &self.gfx.queue, [self.gfx.config.width, self.gfx.config.height], &mut enc, &view, &self.window);
        self.gfx.queue.submit([enc.finish()]);
        self.gfx.queue.present(tex);
        for a in acts { if a == UiAction::Reload { self.reload(); } }
    }

    /// 1 コマ。戻り値は戦いの計算にかかった ms
    fn tick(&mut self, dt: f64) -> f32 {
        // 遅らせた見せ方（setTimeout）: 今の版はコマの間に鳴る。出したコマには鳴らない
        self.run_delayed();
        let c = self.input.read();
        self.last_cam = (c.cam, c.cam_drag);
        #[cfg(not(target_arch = "wasm32"))]
        if std::env::var_os("GYOGUN_TRACE").is_some() && (c.confirm_pressed || c.left_pressed || c.right_pressed || c.down_pressed || c.pause_pressed) { eprintln!("t={:.2} mode={:?} confirm={} right={} down={} pause={}", self.g.as_ref().unwrap().t_all, self.g.as_ref().unwrap().mode, c.confirm_pressed, c.right_pressed, c.down_pressed, c.pause_pressed); }
        // 届いた絵・主役
        self.receive();
        let mode = self.g.as_ref().unwrap().mode;
        let auto = self.g.as_ref().unwrap().auto;
        let mut sim_input: SimInput = NO_INPUT;
        {
            let g = self.g.as_mut().unwrap();
            g.t_all += dt;
            g.frame_i += 1;
            if mode == Mode::Title { g.title_t += dt; sim_input = g.bot.think(&g.battle, dt); }
        }
        match mode {
            // タイトル: 出陣 / みんなで を選んで決定（最初は出陣を選んでいる）
            Mode::Title => { if let Some(a) = self.screens.nav(c.up_pressed || c.left_pressed, c.down_pressed || c.right_pressed, c.confirm_pressed) { self.act(a); } }
            Mode::Select => {
                let i = self.g.as_ref().unwrap().hero_i as i32;
                if c.left_pressed { self.pick_hero(i - 1); }
                if c.right_pressed { self.pick_hero(i + 1); }
                if c.up_pressed { self.step_weapon(-1); }
                if c.down_pressed { self.step_weapon(1); }
                if c.confirm_pressed { self.go(); }
                if c.back_pressed { self.to_title(); }
            }
            Mode::Battle => {
                if c.pause_pressed { self.pause(true); }
                let g = self.g.as_mut().unwrap();
                let (fx, fy) = (-g.cam_yaw.sin(), -g.cam_yaw.cos());
                sim_input = SimInput { mx: fx * c.y + (-fy) * c.x, mz: fy * c.y + fx * c.x, attack: c.attack, heavy: c.heavy, jump: c.jump, dodge: c.dodge, ranbu: c.ranbu, wire: c.wire };
                // 走る向きへ、カメラがゆっくり付いていく（自分で回したあと 1.5 秒は触らない）
                if c.cam.abs() > 0.05 || c.cam_drag.abs() > 0.0005 { g.cam_idle = 0.0; } else { g.cam_idle += dt; }
                let mv = (sim_input.mx * sim_input.mx + sim_input.mz * sim_input.mz).sqrt();
                if !auto && g.cam_idle > 1.5 && mv > 0.5 && g.battle.player.state == PlayerState::Free {
                    let want = (-sim_input.mx).atan2(-sim_input.mz);
                    let mut d = want - g.cam_yaw; d = d.sin().atan2(d.cos());
                    // 真後ろへ走るときは回さない（ぐるっと回って酔うのを避ける）
                    if d.abs() < 2.4 { g.cam_yaw += d * (dt * 0.55).min(1.0) * d.sin().abs(); }
                }
                if auto {
                    if self.opts.bench_secs.is_some() || self.opts.bench_frames.is_some() { g.battle.player.hp = g.battle.player.max_hp; }
                    sim_input = g.bot.think(&g.battle, dt);
                    // 自動のときは、カメラを進む向きの後ろへ寄せる
                    let m = (sim_input.mx * sim_input.mx + sim_input.mz * sim_input.mz).sqrt();
                    if m > 0.3 { let want = (-sim_input.mx).atan2(-sim_input.mz); let mut d = want - g.cam_yaw; d = d.sin().atan2(d.cos()); g.cam_yaw += d * (dt * 1.2).min(1.0); }
                    if g.battle.phase == Phase::Defeat { g.battle.revive(); }
                }
                if let Some(t) = self.opts.ranbu_at { if g.t_all >= t as f64 { g.battle.debug_ranbu(); self.opts.ranbu_at = None; } }
                if self.opts.result_at.is_some_and(|t| g.t_all >= t as f64) { self.opts.result_at = None; self.show_result(); }
            }
            // 一時停止・結果・敗北は、キーボードだけ・パッドだけで選べる（上下で移り、決定で押す。結果と敗北は出てから 0.6 秒は受けない）
            Mode::Paused => {
                if c.pause_pressed || c.back_pressed { self.pause(false); }
                else if let Some(a) = self.screens.nav(c.up_pressed, c.down_pressed, c.confirm_pressed) { self.act(a); }
            }
            Mode::Result => { if let Some(a) = self.screens.nav(c.left_pressed || c.up_pressed, c.right_pressed || c.down_pressed, c.confirm_pressed) { self.act(a); } }
            Mode::Gameover => { if let Some(a) = self.screens.nav(c.up_pressed, c.down_pressed, c.confirm_pressed) { self.act(a); } }
        }
        // 自己ベストの祝い（入力のあと: ボタンで結果の画面を離れたコマに打ち切る）
        self.cheer_tick();
        self.keep_old_battle();
        let mode = self.g.as_ref().unwrap().mode;
        let live = mode == Mode::Battle;
        self.g.as_mut().unwrap().live = live;
        let running = matches!(mode, Mode::Title | Mode::Battle | Mode::Result | Mode::Gameover);
        let t_sim = Instant::now();
        if running { self.g.as_mut().unwrap().battle.step(dt, sim_input); }
        let sim_ms = t_sim.elapsed().as_secs_f32() * 1000.0;
        // 出来事
        let mut events = std::mem::take(&mut self.g.as_mut().unwrap().battle.events);
        for e in events.drain(..) {
            #[cfg(target_arch = "wasm32")]
            self.ev_log.push(sim::trace::event_name(&e));
            self.handle(e, live);
        }
        self.g.as_mut().unwrap().battle.events = events;
        // タイトルの見本は勝ったら・敗れたら作り直す
        {
            let g = self.g.as_ref().unwrap();
            if mode == Mode::Title && (g.battle.phase == Phase::Victory && g.battle.victory_t > 5.0 || g.battle.phase == Phase::Defeat || g.title_t > 240.0) {
                self.new_battle(true);
                self.g.as_mut().unwrap().title_t = 0.0;
                self.keep_old_battle();
            }
        }
        {
            let g = self.g.as_mut().unwrap();
            let mut show_res = false;
            if g.result_t >= 0.0 { g.result_t += dt; if g.result_t > 1.2 && g.mode == Mode::Battle { show_res = true; } }
            let mut show_over = false;
            if g.defeat_t >= 0.0 && g.mode == Mode::Battle { g.defeat_t += dt; if g.defeat_t > 1.8 { g.defeat_t = -1.0; g.mode = Mode::Gameover; show_over = true; } }
            if g.cutin_t >= 0.0 { g.cutin_t += dt; if g.cutin_t > RANBU_CUTIN + 0.2 { g.cutin_t = -1.0; self.screens.cutin_stop(); } }
            // 大将の登場: 時計は大将の状態の秒に合わせる（スローの間もカメラの切り替えと着地がずれない）
            if g.boss_intro_t >= 0.0 {
                if g.battle.boss.state == BossState::Enter { g.boss_intro_t = g.battle.boss.st; } else { g.boss_intro_t = -1.0; self.screens.letterbox(false); }
            }
            self.hud.set_boss_intro(if g.live && g.boss_intro_t >= 0.0 { Some((g.boss_intro_t - BOSS_CUT_AT) as f32) } else { None });
            if g.boss_cut_t >= 0.0 { g.boss_cut_t += dt; if g.boss_cut_t > 1.8 { g.boss_cut_t = -1.0; if g.boss_intro_t < 0.0 && g.battle.phase != Phase::Victory { self.screens.letterbox(false); } } }
            if show_over { self.screens.show(Some(Screen::Gameover)); }
            if show_res { self.show_result(); }
        }
        // 空気: 大将が出ると夕景へ
        {
            let g = self.g.as_mut().unwrap();
            let want = if g.battle.phase == Phase::Boss || g.battle.boss.state != BossState::Dormant { 1.0 } else { 0.0 };
            if (g.boss01 - want).abs() > 0.001 {
                g.boss01 += (want - g.boss01) * (dt * 0.6).min(1.0);
                let k = g.boss01;
                self.apply_theme(k);
            }
        }
        self.update_views(dt);
        sim_ms
    }

    fn receive(&mut self) {
        let ctx = self.ui.ctx().clone();
        let g = self.g.as_mut().unwrap();
        let mut changed = false;
        while let Ok((i, s, t)) = g.pics_rx.try_recv() {
            if let Some(b) = s { g.cards[i].select = load_image(&ctx, &format!("sel{i}"), &b); changed = true; }
            if let Some(b) = t {
                g.thumbs[i] = load_image(&ctx, &format!("thumb{i}"), &b);
                // 戦いの途中で届いたら、名札と切り込みの絵にも入れる（今の版はどちらも出陣で決まる）
                if i == g.hero_i && !matches!(g.mode, Mode::Title | Mode::Select) { self.hud.set_hero(&g.heroes[i].name, g.thumbs[i].clone()); self.screens.set_cutin_hero(g.thumbs[i].clone(), &g.heroes[i].name); }
            }
        }
        if changed { self.screens.set_heroes(g.cards.clone()); self.screens.pick_hero(g.hero_i); }
        while let Ok(l) = g.actor_rx.try_recv() {
            let h = g.heroes[g.hero_i].clone();
            // pickHero の finally: 読み終わった物が今の主役でなくても「読み込み中」は解ける（今の版のまま）
            g.loading_hero = false;
            if h.id != l.id { continue; }
            match l.actor {
                Ok(a) => {
                    let w = g.weapon_id.clone();
                    g.hero.set_hero(&self.gfx, &h, a, &w);
                    g.tint = hex(&h.tint);
                    self.screens.set_sel_name(&h.name);
                }
                Err(e) => { eprintln!("{e}"); self.screens.set_sel_name(&format!("{}（読み込めません）", h.name)); }
            }
        }
    }

    fn run_delayed(&mut self) {
        let Some(g) = self.g.as_mut() else { return };
        if g.delayed.is_empty() { return; }
        let now = clock_ms();
        let mut due: Vec<(f64, Delayed)> = Vec::new();
        let mut i = 0;
        while i < g.delayed.len() { if g.delayed[i].0 <= now { due.push(g.delayed.remove(i)); } else { i += 1; } }
        due.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (mut boss_track, mut blasts) = (false, 0);
        for (_, d) in due {
            match d {
                Delayed::Wave { x, z } => {
                    g.vfx.burst(Burst { n: 8, x, y: 0.4, z, color: hex("#ffd27a"), type_: 0, speed: 12.0, up: 1.2, size: 0.14, life: 0.4, stretch: 0.05, ..Default::default() });
                    g.vfx.burst(Burst { n: 5, x, y: 0.3, z, color: hex("#a89080"), type_: 4, speed: 4.0, up: 0.4, size: 1.2, life: 0.8, ..Default::default() });
                    g.vfx.ring(x, z, 2.4, hex("#ffc060"), 0.3, 0.12);
                }
                Delayed::Detonate { x, z, i, live } => {
                    g.vfx.ball(x, 1.0, z, 2.2, hex("#ffd27a"), 0.35);
                    g.vfx.ring(x, z, 4.2, hex("#fff0c0"), 0.45, 0.06);
                    g.vfx.ring(x, z, 2.6, hex("#ff9a3a"), 0.35, 0.12);
                    g.vfx.burst(Burst { n: 16, x, y: 0.8, z, color: hex("#ffd27a"), colors: &[hex("#ffd27a"), hex("#ffffff"), hex("#ff9a3a")], type_: 0, speed: 18.0, up: 0.8, size: 0.14, life: 0.45, stretch: 0.05, ..Default::default() });
                    g.vfx.burst(Burst { n: 6, x, y: 0.3, z, color: hex("#8a7466"), type_: 4, speed: 8.0, up: 0.1, size: 1.1, life: 0.7, drag: 3.0, ..Default::default() });
                    g.trauma = (g.trauma + 0.12).min(1.0);
                    g.flash = g.flash.max(0.05);
                    if live && i % 4 == 0 { blasts += 1; }
                }
                Delayed::Shard { x, y, z, dx, dz, n, live } => {
                    // 斬った向きへ、砂糖のかけらと粉が弾ける（三撃目は大きく、低い破砕音）
                    let big = 1.0 + (n as f32 - 1.0) * 0.45;
                    g.vfx.burst(Burst { n: (26.0 * big) as u32, x, y, z, color: hex("#fff6e8"), colors: &[hex("#fff6e8"), hex("#ffd98a"), hex("#ffb84a"), hex("#cfe8ff")], type_: 0, speed: 20.0 * big, up: 0.5, size: 0.16, life: 0.55, dir: Some([dx, 0.25, dz]), spread: 0.8, gravity: 9.0, drag: 2.0, stretch: 0.07, ..Default::default() });
                    g.vfx.burst(Burst { n: (4.0 * big) as u32, x, y, z, color: hex("#b9aea2"), type_: 4, speed: 5.0 * big, up: 0.4, size: 0.6 * big, life: 0.55, dir: Some([dx, 0.2, dz]), spread: 1.2, drag: 3.0, ..Default::default() });
                    g.trauma = (g.trauma + 0.1 * big as f64).min(1.0);
                    if live { self.audio.wire_crush(n as i32); }
                }
                Delayed::Finale => { self.hud.say("猪鹿蝶", "糸斬り 三連", Tone::Gold); self.hud.issen(); }
                Delayed::BossTrack => boss_track = true,
            }
        }
        for _ in 0..blasts { self.audio.blast(); }
        if boss_track { self.audio.play_track(Track::Boss); }
    }

    fn update_loaded(&mut self) {
        let g = self.g.as_mut().unwrap();
        g.loaded.clear();
        let p = &g.battle.player;
        let mut s: Option<AimShape> = None;
        if g.mode == Mode::Battle && p.state == PlayerState::Attack {
            if let Some(m) = p.r#move {
                if !same(m, &RANBU) {
                    if let Some(own) = aim_shape(m).filter(|o| p.move_t < o.hit.t + 0.06) { s = Some(own); }
                    else if is_normal(m) { s = aim_shape(&CHARGES[(p.chain as usize).min(CHARGES.len() - 1)]); }
                }
            }
        }
        if let Some(s) = s {
            for e in &g.battle.enemies {
                if !e.alive || e.state == EnemyState::Dead || e.role == Role::Captain || e.y > 1.5 { continue; }
                if (e.x - p.x).abs() > 22.0 || (e.z - p.z).abs() > 22.0 { continue; }
                if in_aim(&s, p.x, p.z, p.face, e.x, e.z, KINDS[e.kind as usize].radius) { g.loaded.insert(e.id); }
            }
        }
        self.hud.set_load(g.loaded.len());
    }

    fn update_views(&mut self, dt: f64) {
        let g = self.g.as_mut().unwrap();
        let live = g.live;
        let b = &mut g.battle;
        let sim_dt = if b.hitstop > 0.0 { 0.0 } else { dt * b.time_scale() };
        let t_all = g.t_all;
        let cur = b.player.r#move;
        // 寄せている間に強攻撃の範囲の敵が 5 体以上 → 体がほのかに光る（集めて撃てば弾が重くなる合図）
        let ready = if cur.map(is_normal).unwrap_or(false) && g.loaded.len() >= 5 { 0.28 + 0.1 * (t_all * 14.0).sin() } else { 0.0 };
        let aura = if cur.map(|m| same(m, &RANBU)).unwrap_or(false) { 1.0 }
            else if b.player.carry != 0 { 0.45 + 0.15 * (t_all * 16.0).sin() }
            else if b.player.ranbu >= 100.0 { 0.35 + 0.15 * (t_all * 6.0).sin() }
            else if cur.map(is_charge).unwrap_or(false) { 0.35 } else { ready };
        // 武将選び: 主役を正面から見せる（戦いは止める）
        if g.mode == Mode::Select {
            let p = &mut b.player;
            p.state = PlayerState::Free; p.r#move = None; p.speed = 0.0; p.y = 0.0;
            p.face = g.cam_yaw - 0.4;
        }
        let p = &b.player;
        let flash = if p.state == PlayerState::Hurt { (1.0 - p.st * 4.0).max(0.0) } else { 0.0 };
        // 育つ武器（試作）: 武勲で武器そのものが大きくなる（一時停止・敗北の間も大きさは変えない）。武将選びでは元の大きさ
        let grow = if g.mode == Mode::Select { 1.0 } else { b.weapon_grow() };
        g.hero.update(&self.gfx, p, sim_dt, t_all, HeroOpts { flash, aura, show: true, ring: live, freeze_anim: false, grow });
        let blade = g.hero.blade();
        let tip_speed = if sim_dt > 0.0 { blade.tip.distance(blade.prev) as f64 / sim_dt } else { 0.0 };
        // 育つ武器の試作の間は、光の刃を出さない（格 0 で渡す。採用が決まったら rank_blade を消す）
        g.rank_blade.update(&self.gfx, blade.base, blade.tip, 0, t_all as f32, sim_dt as f32, g.tint, true);
        // 飴糸: 手から糸の先へ、ねばる飴の糸。撃つ間（WIRE_AIM）は先へ伸び、引かれる間はたるみながら縮む。しずくが垂れる
        if p.state == PlayerState::Wire && !p.wire.esc {
            let w = p.wire;
            let k = (p.st / WIRE_AIM).min(1.0) as f32;
            let end = blade.base.lerp(glam::Vec3::new(w.ax as f32, w.ay as f32, w.az as f32), k);
            let pull = (((p.st - WIRE_AIM) / w.dur) as f32).clamp(0.0, 1.0);
            let len = blade.base.distance(end);
            // 撃つ間はたるみ、張った瞬間（発進）にピンと張る。斬り込むときは左右の腰から二本
            let sag = if p.st < WIRE_AIM { len * 0.22 + 0.3 } else { len * (0.015 + 0.07 * pull) + 0.05 };
            if w.target != 0 {
                let side = Vec3::new(p.face.cos() as f32, 0.0, -p.face.sin() as f32) * 0.5;
                let hip = Vec3::new(p.x as f32, p.y as f32 + 0.75, p.z as f32);
                taffy(&mut g.vfx, hip + side, end + side * 0.25, sag, 0.42, t_all as f32);
                taffy(&mut g.vfx, hip - side, end - side * 0.25, sag * 0.9, 0.42, t_all as f32 + 1.7);
            } else {
                taffy(&mut g.vfx, blade.base, end, sag, 1.0, t_all as f32);
            }
            // 粉砂糖の噴射（立体機動のガスの代わり）: 飛ぶ間、腰の後ろから粉が尾を引く。斬り込むときは主役の色の光の粒も道に残る
            if sim_dt > 0.0 && p.st >= WIRE_AIM && w.target != 0 {
                g.vfx.burst(Burst { n: 2, x: p.x as f32, y: p.y as f32 + 0.9, z: p.z as f32, color: g.tint, colors: &[g.tint, hex("#fff2c8")], type_: 1, speed: 0.6, size: 0.2, life: 0.45, gravity: 0.0, drag: 3.0, ..Default::default() });
            }
            if sim_dt > 0.0 && p.st >= WIRE_AIM {
                let (bx, bz) = (-(p.face.sin() as f32), -(p.face.cos() as f32));
                g.vfx.burst(Burst { n: 1, x: p.x as f32 + bx * 0.7, y: p.y as f32 + 0.8, z: p.z as f32 + bz * 0.7, color: hex("#cfc6bc"), colors: &[hex("#cfc6bc"), hex("#bfb3a6")], type_: 4, speed: 3.0, up: 0.2, size: 0.28, life: 0.35, dir: Some([bx, 0.1, bz]), spread: 0.5, drag: 5.0, ..Default::default() });
            }
            if sim_dt > 0.0 {
                let u = (t_all as f32 * 7.3).fract();
                let d = blade.base.lerp(end, u) - Vec3::Y * (u * std::f32::consts::PI).sin() * len * 0.1;
                g.vfx.burst(Burst { n: 1, x: d.x, y: d.y, z: d.z, color: hex("#ffae38"), colors: &[hex("#ffae38"), hex("#ffd98a")], type_: 1, speed: 0.6, size: 0.16, life: 0.6, gravity: 14.0, ..Default::default() });
            }
            g.taffy = None;
        }
        // 斬った的には糸が付いたまま残る: 続けられる間は、空へ逃げても細く伸びてつながっている。続けられなくなるとちぎれる
        if let Some((mut at, age)) = g.taffy {
            let age = age + sim_dt;
            if p.wire_last == -1 {
                if !matches!(b.boss.state, BossState::Dormant | BossState::Dead) { at = Vec3::new(b.boss.x as f32, b.boss_y as f32 + 2.6, b.boss.z as f32); }
            } else if let Some(e) = b.enemies.iter().find(|e| e.id == p.wire_last && e.alive && e.state != EnemyState::Dead) {
                at = Vec3::new(e.x as f32, (e.y + e.kind.def().height * 0.6) as f32, e.z as f32);
            }
            let len = blade.base.distance(at);
            let going = p.wire_chain > 0 && (p.wire_chain_t > 0.0 || p.state == PlayerState::Wire);
            if !(going || age < 0.5) || len > 44.0 {
                let mid = blade.base.lerp(at, 0.5) - Vec3::Y * len * 0.12;
                g.vfx.burst(Burst { n: 10, x: mid.x, y: mid.y, z: mid.z, color: hex("#ffae38"), colors: &[hex("#ffae38"), hex("#ffd98a"), hex("#ff9a2e")], type_: 1, speed: 3.0, up: 0.3, size: 0.18, life: 0.7, gravity: 14.0, ..Default::default() });
                g.taffy = None;
            } else {
                taffy(&mut g.vfx, blade.base, at, len * 0.12 + 0.2, (2.4 / len.max(2.4)).powf(0.8).max(0.3), t_all as f32);
                g.taffy = Some((at, age));
            }
        }
        let ranbu_move = cur.map(|m| same(m, &RANBU)).unwrap_or(false);
        g.vfx.update_trail(&self.gfx, g.rank_blade.tip(), blade.base, (p.state == PlayerState::Attack && tip_speed > 5.0) || ranbu_move, dt as f32, g.tint);
        self.update_loaded();
        let g = self.g.as_mut().unwrap();
        let b = &g.battle;
        let p = &b.player;
        let giant_on = b.giant.on && b.giant.t > 2.4;
        // 担いだときの案内は、最後に触った機器の表記
        let carry = if p.carry != 0 { controls::carry_hint(self.input.device()) } else { String::new() };
        let giant = if giant_on { controls::giant_hint(self.input.device()) } else { String::new() };
        let ride = if p.state == PlayerState::Ride { controls::ride_hint(self.input.device()) } else { String::new() };
        // 猪鹿蝶: 斬った直後は「空へ逃げる」、逃げたあと続けられる間は「斬り込む」
        let wire_next = if p.wire_esc > 0.0 && p.wire_chain > 0 { controls::wire_esc_hint(self.input.device()) } else if p.wire_chain > 0 && p.wire_chain_t > 0.0 && p.state != PlayerState::Wire { controls::wire_dive_hint(self.input.device()) } else { String::new() };
        self.hud.set_hint(if !live { "" } else if p.carry != 0 { &carry } else if p.state == PlayerState::Ride { &ride } else if !wire_next.is_empty() { &wire_next } else if giant_on { &giant } else { "" });
        // わたあめ雲: 主役の足元に、大きく平たいわたあめ忍。出るときにふくらみ、溶ける 1.5 秒前から縮む。後ろへ切れ端が流れる
        let mount = (p.state == PlayerState::Ride).then(|| {
            let (t, st, left) = (t_all as f32, p.st as f32, (RIDE_T - p.st) as f32);
            let k = (st / 0.25).min(1.0) * if left < 1.5 { 0.5 + 0.5 * left / 1.5 + 0.06 * (t * 30.0).sin() } else { 1.0 };
            let wob = 0.04 * (t * 5.0).sin();
            let (sx, sy) = (2.7 * k * (1.0 + wob), 1.05 * k * (1.0 - wob));
            glam::Mat4::from_scale_rotation_translation(glam::Vec3::new(sx, sy, sx), glam::Quat::from_rotation_y(p.face as f32), glam::Vec3::new(p.x as f32, p.y as f32 + 0.12 - sy, p.z as f32))
        });
        if mount.is_some() && sim_dt > 0.0 && g.rng.f() < 0.6 {
            let (fs, fc) = ((p.face as f32).sin(), (p.face as f32).cos());
            let back = 0.4 + (p.speed as f32) * 0.08;
            g.vfx.burst(Burst { n: 1, x: p.x as f32 - fs * back, y: p.y as f32 - 0.5, z: p.z as f32 - fc * back, color: hex("#ffd6ea"), colors: &[hex("#ffd6ea"), hex("#ffb8d8"), hex("#ffc2dd")], type_: 4, speed: 0.8, up: 0.2, size: 0.9, life: 0.7, ..Default::default() });
        }
        g.horde.update(&self.gfx, &b.enemies, t_all as f32, g.camera.pos32(), &g.loaded, mount);
        // 群衆ドミノ: 弾として飛んでいる敵に光の尾（どこを連鎖が走っているか見えるように）
        if sim_dt > 0.0 {
            let (big_c, nor_c, th_c) = (hex("#ff8ac8"), hex("#ffd27a"), [hex("#ffe08a"), hex("#ff9ac8"), hex("#ffffff")]);
            for en in &b.enemies {
                if !en.alive || en.fling == 0 || en.y > 2.4 || (en.vx * en.vx + en.vz * en.vz) < 36.0 { continue; }
                let big = en.role == Role::Brute || en.boost;
                // 格 弐からは、技で飛ばした敵が長い光の尾を引く
                let long = p.rank >= 2 && en.r#gen == 0;
                let (x, z) = (en.x as f32, en.z as f32);
                let h = KINDS[en.kind as usize].height as f32;
                g.vfx.burst(Burst { n: 1, x, y: en.y as f32 + h * 0.5, z, color: if big { big_c } else { nor_c }, type_: 1, speed: 0.0, size: if en.thrown { 2.8 } else if big { 1.7 } else if long { 1.3 } else { 0.95 }, life: if en.thrown { 0.4 } else if long { 0.5 } else { 0.24 }, gravity: 0.0, ..Default::default() });
                if en.thrown { g.vfx.burst(Burst { n: 3, x, y: en.y as f32 + 1.0, z, color: th_c[0], colors: &th_c, type_: 3, speed: 3.0, up: 0.6, size: 0.3, life: 0.6, ..Default::default() }); }
            }
        }
        // 大車輪: 振り回している重騎の通り道に光の渦
        if sim_dt > 0.0 && p.carry != 0 && cur.map(|m| same(m, &HURL)).unwrap_or(false) {
            if let Some(c) = b.enemies.iter().find(|o| o.id == p.carry) {
                g.vfx.burst(Burst { n: 2, x: c.x as f32, y: c.y as f32 + 0.8, z: c.z as f32, color: hex("#ff9ac8"), colors: &[hex("#ff9ac8"), hex("#ffe08a"), hex("#ffffff")], type_: 1, speed: 1.0, size: 1.6, life: 0.3, gravity: 0.0, ..Default::default() });
            }
        }
        g.boss_view.update(&self.gfx, &b.boss, sim_dt, b.hitstop > 0.0, b.boss_y as f32, g.boss_fx.body_light());
        g.boss_fx.update(&self.gfx, &b.boss, &g.boss_view, &mut g.vfx, dt as f32, sim_dt as f32, t_all as f32);
        g.vfx.update_telegraphs(&self.gfx, &b.telegraphs, t_all as f32);
        g.vfx.update(&self.gfx, sim_dt, t_all as f32);
        g.world.update(&self.gfx, &b.holds, b.gate_open, b.boss.state != BossState::Dormant, dt as f32, t_all as f32, g.camera.pos32(), (p.x as f32, p.z as f32));
        g.mirrors.update(&self.gfx, dt as f32, t_all as f32);
        // 拾い物
        for k in &b.pickups {
            if k.kind == PickupKind::Konpeito && (g.rng.f() as f64) < dt * 6.0 {
                g.vfx.burst(Burst { n: 1, x: k.x as f32, y: 0.8, z: k.z as f32, color: hex("#ffd27a"), type_: 2, speed: 1.0, up: 2.0, size: 0.4, life: 0.5, gravity: -1.0, ..Default::default() });
            }
        }
        for s in b.projectiles.iter().take(64) {
            if g.rng.f() < 0.5 { g.vfx.burst(Burst { n: 1, x: s.x as f32, y: s.y as f32, z: s.z as f32, color: hex("#ff7ab0"), type_: 1, speed: 0.3, size: 0.5, life: 0.25, gravity: 0.0, ..Default::default() }); }
        }
        g.items.update(&self.gfx, &b.pickups, &b.projectiles, t_all as f32);
        // カメラ
        self.update_camera_all(dt);
        let g = self.g.as_mut().unwrap();
        g.sky.update(&self.gfx, g.t_all as f32, g.camera.pos32());
        // 画面の仕上げ（減衰）
        let p = &g.battle.player;
        let ranbu_on = p.r#move.map(|m| same(m, &RANBU)).unwrap_or(false);
        let mo = if ranbu_on { 1.0 } else { 0.0 };
        g.zoom_blur = (g.zoom_blur - dt * 0.6).max(mo * 0.035);
        g.speed = (g.speed - dt * 2.2).max(mo * (if p.move_t > RANBU_CUTIN { 0.7 } else { 0.3 })).max(if p.state == PlayerState::Dodge { 0.5 } else if p.state == PlayerState::Wire && p.st >= WIRE_AIM && p.st - WIRE_AIM < p.wire.dur * 0.55 { 0.55 } else if p.state == PlayerState::Ride { (p.speed / 15.0 * 0.4) as _ } else { 0.0 });
        g.flash = (g.flash - dt * 2.8).max(0.0);
        g.red_k = (g.red_k - dt * 1.4).max(0.0);
        g.gold += (mo - g.gold) * (dt * 4.0).min(1.0);
        let kick = p.r#move.map(|m| m.id == "c0").unwrap_or(false) || p.state == PlayerState::Dodge || p.state == PlayerState::Wire;
        g.fov_kick += ((if kick { 1.0 } else { 0.0 }) - g.fov_kick) * (dt * 8.0).min(1.0);
        self.screens.set_flash((g.flash - 0.25).max(0.0) as f32);
    }

    fn update_camera_all(&mut self, dt: f64) {
        let aspect = self.gfx.aspect();
        let (lw, lh) = self.gfx.logical;
        let g = self.g.as_mut().unwrap();
        let b = &g.battle;
        let p = &b.player;
        let t_all = g.t_all;
        // three の camera.lookAt は、まず今の位置と前のコマの回転で matrixWorldInverse を作る（project はこれを読む）
        let prev_rot = g.camera.rot();
        match g.mode {
            Mode::Title => {
                let a = t_all * 0.12;
                let target = DVec3::new(p.x + a.sin() * 13.0, 4.5 + (t_all * 0.3).sin() * 1.2, p.z + a.cos() * 13.0);
                g.cam_pos = lerpv(g.cam_pos, target, (dt * 2.0).min(1.0));
                g.cam_look = lerpv(g.cam_look, DVec3::new(p.x, 1.6, p.z), (dt * 3.0).min(1.0));
                g.camera = Camera::new(g.cam_pos, g.cam_look, 0.0, g.camera.fov, aspect, prev_rot);
            }
            Mode::Select => {
                let y = g.cam_yaw;
                // 主役を寄りで見せる（右の欄のぶん左へ寄せる）。縦持ちは横に欄を置けないので、引いて上半分の真ん中に見せる（今までは左の端へ切れていた）。
                // 低い画面は、見る点を下げて主役を上へ（脚が武将の帯に隠れていた）
                let (dist, side_pos, side_look, cam_h, look_h) = if lw <= 760.0 && lh > 520.0 { (8.0, 0.0, 0.0, 1.4, -0.45) } else if lh <= 520.0 { (4.6, 1.4, 1.3, 1.7, 0.2) } else { (4.6, 1.4, 1.3, 1.7, 1.25) };
                let target = DVec3::new(p.x + y.sin() * dist + y.cos() * side_pos, cam_h, p.z + y.cos() * dist - y.sin() * side_pos);
                g.cam_pos = lerpv(g.cam_pos, target, (dt * 3.0).min(1.0));
                g.cam_look = lerpv(g.cam_look, DVec3::new(p.x + y.cos() * side_look, look_h, p.z - y.sin() * side_look), (dt * 4.0).min(1.0));
                g.camera = Camera::new(g.cam_pos, g.cam_look, 0.0, g.camera.fov, aspect, prev_rot);
            }
            _ => {
                let c = if g.mode == Mode::Battle { Some(self.last_cam) } else { None };
                update_camera(g, dt, c, aspect, prev_rot);
            }
        }
    }

    // ---- 描画 ----
    fn render(&mut self) {
        // 画面の順番待ち（垂直同期）は記録の CPU 時間から外す
        let t_wait = Instant::now();
        let tex = match self.gfx.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => { self.gfx.surface.configure(&self.gfx.device, &self.gfx.config); return; }
            _ => return,
        };
        self.wait_ms = t_wait.elapsed().as_secs_f32() * 1000.0;
        // 表記と指の部品を、最後に触った機器に合わせる。指の部品は戦いの間だけ（一時停止・結果・敗北の間は取らない・描かない）
        self.sync_device();
        let battle_on = self.g.as_ref().unwrap().mode == Mode::Battle;
        self.screens.touch_on(battle_on);
        self.input.set_touch_on(battle_on);
        // 画面が上に出ている間は、HUD の下の操作の説明の列を描かない（一時停止の札の下からのぞいていた）
        self.hud.set_covered(self.screens.shown().is_some());
        let ctx = self.ui.begin(&self.window);
        let (mut acts, over_ui);
        {
            let g = self.g.as_ref().unwrap();
            let adv = g.live || g.mode == Mode::Paused;
            acts = self.hud.ui(&ctx, &g.battle, if adv { self.frame_dt } else { 0.0 }, &g.camera, g.cam_yaw, adv);
            acts.extend(self.screens.ui(&ctx));
            over_ui = ctx.pointer_hover_pos().is_some_and(|p| crate::ui::over_ui(&ctx, p));
        }
        self.input.set_pointer_over_ui(over_ui);
        // タッチ: ボタンの当たりと、見た目（スティック・押している札）
        {
            let (lw, lh) = self.gfx.logical;
            self.input.set_touch_buttons(&crate::ui::touch_buttons(lw, lh, self.safe));
            let sv = self.input.stick_view();
            let on = crate::ui::TOUCH_BTNS.map(|b| self.input.touch_button_on(b.name()));
            self.screens.set_touch_view(crate::ui::TouchView { stick: sv.as_ref().map(|s| (s.x, s.y)), knob: sv.as_ref().map(|s| (s.dx, s.dy)).unwrap_or((0.0, 0.0)), on });
        }
        let g = self.g.as_ref().unwrap();
        let view_s = tex.texture.create_view(&Default::default());
        let gfx = &self.gfx;
        let p = &g.battle.player;
        let draw_cam = match g.closeup { Some((pos, look)) => Camera { pos: pos.as_dvec3(), look: look.as_dvec3(), roll: 0.0, ..g.camera }, None => g.camera };
        let (pv, vm, pm) = { let (v, m) = (draw_cam.view(), draw_cam.proj()); (m * v, v, m) };
        // 影の範囲は主役を追う
        let eye = light_eye((p.x as f32, p.z as f32));
        let light_vp = Mat4::orthographic_rh(-34.0, 34.0, -34.0, 34.0, 1.0, 160.0) * Mat4::look_at_rh(eye, Vec3::new(p.x as f32, 0.0, p.z as f32), Vec3::Y);
        let (rw, rh) = gfx.render_size();
        let mut gl = Globals {
            view: vm.to_cols_array_2d(), proj: pm.to_cols_array_2d(), view_proj: pv.to_cols_array_2d(), light_vp: light_vp.to_cols_array_2d(),
            cam: draw_cam.pos32().extend(1.0).to_array(),
            hemi_sky: [0.0; 4], hemi_ground: [0.0; 4], key_color: [0.0; 4],
            key_dir: (eye - Vec3::new(p.x as f32, 0.0, p.z as f32)).normalize().extend(0.0).to_array(),
            fog: [0.0; 4], rim: [0.0; 4], misc: [g.t_all as f32, rh as f32, rw as f32, 0.0],
        };
        g.theme.apply(&mut gl);
        gfx.queue.write_buffer(&gfx.globals_buf, 0, bytemuck::bytes_of(&gl));
        let live = g.live;
        let mut enc = gfx.device.create_command_encoder(&Default::default());
        // 影
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("shadow"),
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &gfx.shadow_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                ..Default::default()
            });
            pass.set_bind_group(0, &gfx.globals_bg, &[]);
            g.world.shadow(&mut pass);
            g.horde.shadow(&mut pass);
            g.boss_view.shadow(&mut pass);
            g.mirrors.shadow(&mut pass);
            g.items.shadow(&mut pass);
            g.hero.shadow(&mut pass);
        }
        // 自機だけを白で描く型（縁取り用）
        let (mask, mask_d) = gfx.mask_targets();
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mask"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: mask, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store }, depth_slice: None })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: mask_d, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                ..Default::default()
            });
            pass.set_bind_group(0, &gfx.globals_bg, &[]);
            if live { g.hero.draw_mask(&mut pass); }
        }
        // 主の絵
        let (msaa, depth, resolved) = gfx.main_targets();
        {
            let clear = hex("#2a2650");
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: msaa, resolve_target: Some(resolved), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: clear[0] as f64, g: clear[1] as f64, b: clear[2] as f64, a: 1.0 }), store: wgpu::StoreOp::Discard }, depth_slice: None })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                ..Default::default()
            });
            pass.set_bind_group(0, &gfx.globals_bg, &[]);
            g.sky.draw_sky(&mut pass);
            for order in std::iter::once(ORDER_OPAQUE).chain(ORDERS) {
                // 主役が先（＞＜ の板は TS の renderOrder -1）
                g.hero.draw(gfx, &mut pass, order);
                g.sky.draw(gfx, &mut pass, order);
                g.world.draw(gfx, &mut pass, order);
                g.boss_view.draw(gfx, &mut pass, order);
                g.horde.draw(gfx, &mut pass, order);
                g.mirrors.draw(gfx, &mut pass, order);
                g.items.draw(gfx, &mut pass, order);
                g.rank_blade.draw(gfx, &mut pass, order);
                g.vfx.draw(gfx, &mut pass, order);
                g.boss_fx.draw(&mut pass, order);
            }
            // 物陰の自機の影絵
            if live { g.hero.draw_hidden(gfx, &mut pass); }
        }
        // 画面の仕上げ
        let c = g.camera.project(DVec3::new(p.x, p.y + 1.2, p.z));
        let ranbu_on = p.r#move.map(|m| same(m, &RANBU)).unwrap_or(false);
        let hp_low = live && p.hp / p.max_hp < 0.25;
        let f = FinalU {
            center: [c.x, c.y], time: g.t_all as f32, vignette: 0.26 + if ranbu_on { 0.12 } else { 0.0 } + if g.boss_intro_t >= 0.0 || g.boss_cut_t >= 0.0 { 0.16 } else { 0.0 },
            flash: (g.flash * 0.35) as f32, desat: if g.battle.time_scale() < 1.0 { 0.35 } else { 0.0 }, aberration: (0.0015 + g.zoom_blur * 0.03) as f32, zoom: g.zoom_blur as f32,
            speed: g.speed as f32, gold: (g.gold * 0.45) as f32, red: (g.red_k * 0.45 + if hp_low { 0.08 + 0.05 * (g.t_all * 6.0).sin() } else { 0.0 }) as f32, outline_on: if live { 1.0 } else { 0.0 },
            ..Default::default()
        };
        self.last_final = f;
        gfx.post(&mut enc, &view_s, f);
        self.ui.paint(&gfx.device, &gfx.queue, [gfx.config.width, gfx.config.height], &mut enc, &view_s, &self.window);
        // 画面を PNG に残す（確かめ用。ブラウザではブラウザの道具で撮る）
        #[cfg(not(target_arch = "wasm32"))]
        let shot = self.pending_shot.take().map(|path| {
            let (w, h) = (gfx.config.width, gfx.config.height);
            let bpr = (w * 4 + 255) & !255;
            let buf = gfx.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bpr * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            enc.copy_texture_to_buffer(tex.texture.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
            (path, buf, w, h, bpr)
        });
        gfx.queue.submit([enc.finish()]);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((path, buf, w, h, bpr)) = shot {
            buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
            let _ = gfx.device.poll(wgpu::PollType::wait_indefinitely());
            let data = buf.slice(..).get_mapped_range().unwrap();
            let bgra = matches!(gfx.config.format, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb);
            let mut px = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h as usize { for x in 0..w as usize { let i = y * bpr as usize + x * 4; if bgra { px.extend_from_slice(&[data[i + 2], data[i + 1], data[i], 255]); } else { px.extend_from_slice(&[data[i], data[i + 1], data[i + 2], 255]); } } }
            let _ = image::save_buffer(&path, &px, w, h, image::ColorType::Rgba8);
            println!("shot {path}");
        }
        let t_present = Instant::now();
        self.gfx.queue.present(tex);
        self.wait_ms += t_present.elapsed().as_secs_f32() * 1000.0;
        for a in acts { self.act(a); }
    }

    // ---- 出来事 → 見せ方 ----
    /// 出来事を見せる
    fn handle(&mut self, e: BattleEvent, live: bool) {
        let g = self.g.as_mut().unwrap();
        let audio = &self.audio;
        let hud = &mut self.hud;
        let screens = &mut self.screens;
        let (px, py, pz, pface, prank) = { let p = &g.battle.player; (p.x as f32, p.y as f32, p.z as f32, p.face as f32, p.rank) };
        let reach = g.battle.weapon_reach();
        let (face_d, now_ms) = (g.battle.player.face, clock_ms());
        let tint = g.tint;
        macro_rules! shake { ($k:expr) => { g.trauma = (g.trauma + $k).min(1.0) } }
        match e {
            BattleEvent::Move { r#move: m } => {
                let heavy = is_charge(m) || same(m, &RANBU) || same(m, &JUMP_CHARGE);
                if live && heavy { audio.swing(true); }
                if m.id == "c0" || m.id == "da" { g.speed = g.speed.max(0.55); g.zoom_blur = g.zoom_blur.max(0.05); }
                if m.id == "c5" { g.vfx.pillar(px, pz, 4.2, 6.0, tint, 1.2, 1.4); }
            }
            BattleEvent::Swing { r#move: m, index } => {
                let mut arc: MoveArc = m.arcs[index as usize];
                // 弧の向きを、実際の刃の動き（左右）に合わせる。体ごと回る技（一周以上）は決めた向きのまま
                let sweep = arc.to - arc.from;
                if sweep.abs() < std::f64::consts::PI * 1.9 {
                    let bl = g.hero.blade();
                    let lat = (bl.tip.x - bl.prev.x) * pface.cos() - (bl.tip.z - bl.prev.z) * pface.sin();
                    if lat.abs() > 0.01 && (lat as f64) * sweep < 0.0 { arc.from = -arc.from; arc.to = -arc.to; arc.tilt = -arc.tilt; }
                }
                // 育つ武器: 通常攻撃の弧は武器の大きさに合わせて大きくなる（当たりと同じだけ）。参では乱舞の斬撃も大きく
                if is_normal(m) { arc.r *= reach; arc.width *= 1.0 + (reach - 1.0) * 0.8; }
                if prank >= 3 && same(m, &RANBU) { arc.r *= 1.2; arc.width *= 1.2; }
                g.vfx.slash(&arc, px, py, pz, pface, tint, 1.0);
                if live { audio.swing(arc.kind != ArcKind::Slash); }
            }
            BattleEvent::Hit { x, y, z, count, heavy, dx, dz, boss, .. } => {
                let (x, y, z) = (x as f32, y as f32, z as f32);
                g.vfx.burst(Burst { n: if heavy { 10 } else { 6 }, x, y, z, color: hex("#fff2c8"), colors: &[hex("#fff2c8"), hex("#ff9cc6"), hex("#ffd37a"), hex("#a8f0d4")], type_: 0, speed: if heavy { 22.0 } else { 15.0 }, size: 0.14, life: 0.28, dir: Some([dx as f32, 0.35, dz as f32]), spread: 0.9, gravity: 4.0, drag: 4.0, stretch: 0.05, ..Default::default() });
                if count <= 3 || heavy { g.vfx.burst(Burst { n: 1, x, y, z, color: hex(if heavy { "#ffd27a" } else { "#ffffff" }), type_: 2, speed: 0.0, size: if heavy { 2.6 } else { 1.5 }, life: 0.16, gravity: 0.0, ..Default::default() }); }
                g.vfx.burst(Burst { n: 1, x, y, z, color: hex("#ff8a40"), type_: 1, speed: 0.0, size: if heavy { 1.4 } else { 0.8 }, life: 0.14, gravity: 0.0, ..Default::default() });
                if boss { g.vfx.burst(Burst { n: 8, x, y, z, color: hex("#c8743a"), type_: 3, speed: 9.0, size: 0.3, life: 1.0, up: 1.0, ..Default::default() }); }
                if live && count == 1 { audio.hit(1, heavy); }
                if live && count == 4 { audio.hit(6, false); }
                g.zoom_blur = g.zoom_blur.max(if heavy { 0.045 } else { 0.01 });
                shake!(if heavy { 0.05 } else { 0.012 });
            }
            BattleEvent::Impact { x, z, fx, r, face, .. } => {
                let (x, z, r, face) = (x as f32, z as f32, r as f32, face as f32);
                match fx {
                    HitFx::Ring => {
                        g.vfx.ring(x, z, r * 1.25, hex("#ffd27a"), 0.5, 0.06);
                        g.vfx.ring(x, z, r * 0.8, hex("#ff8a3a"), 0.35, 0.12);
                        g.vfx.crack(x, z, r * 0.75, None);
                        g.vfx.burst(Burst { n: 14, x, y: 0.25, z, color: hex("#8a7466"), type_: 4, speed: 11.0, up: 0.08, size: 1.1, life: 0.8, gravity: -0.5, drag: 3.0, ..Default::default() });
                        g.vfx.burst(Burst { n: 24, x, y: 0.4, z, color: hex("#ffd27a"), type_: 0, speed: 18.0, up: 0.8, size: 0.12, life: 0.5, stretch: 0.04, ..Default::default() });
                        shake!(0.55); g.zoom_blur = g.zoom_blur.max(0.06); g.flash = g.flash.max(0.1);
                        if live { audio.ring(); }
                    }
                    HitFx::Wave => {
                        let (fx_, fz_) = (face.sin(), face.cos());
                        for i in 1..=7 { let d = i as f32 * 2.0; g.delayed.push((now_ms + i as f64 * 22.0, Delayed::Wave { x: x + fx_ * d, z: z + fz_ * d })); }
                        g.vfx.crack(x + fx_ * 3.0, z + fz_ * 3.0, 3.4, None);
                        shake!(0.4); g.zoom_blur = g.zoom_blur.max(0.06);
                        if live { audio.blast(); }
                    }
                    HitFx::Blast => {
                        g.vfx.ball(x, 1.2, z, r * 0.55, tint, 0.35);
                        g.vfx.ring(x, z, r * 1.4, hex("#fff0c0"), 0.6, 0.05);
                        g.vfx.ring(x, z, r * 0.9, tint, 0.45, 0.14);
                        g.vfx.pillar(x, z, r * 0.4, 14.0, hex("#ffe0a0"), 0.6, 0.4);
                        g.vfx.crack(x, z, r * 0.8, None);
                        g.vfx.burst(Burst { n: 40, x, y: 1.0, z, color: hex("#ffd27a"), type_: 0, speed: 26.0, up: 0.7, size: 0.16, life: 0.6, stretch: 0.04, ..Default::default() });
                        g.vfx.burst(Burst { n: 14, x, y: 0.3, z, color: hex("#8a7466"), type_: 4, speed: 14.0, up: 0.08, size: 1.2, life: 0.9, drag: 3.0, ..Default::default() });
                        shake!(0.8); g.zoom_blur = g.zoom_blur.max(0.09); g.flash = g.flash.max(0.15);
                        if live { audio.blast(); }
                    }
                    HitFx::Heavy => { shake!(0.15); }
                    _ => {}
                }
            }
            BattleEvent::Kill { kind, x, y, z, captain, .. } => {
                let (x, y, z) = (x as f32, y as f32, z as f32);
                let col = hex(sparkle(kind));
                g.vfx.burst(Burst { n: if captain { 60 } else { 7 }, x, y: y + 0.6, z, color: col, colors: &[col, hex("#fff3e0"), hex("#ffd27a")], type_: 3, speed: if captain { 14.0 } else { 8.0 }, up: 1.1, size: if captain { 0.42 } else { 0.26 }, life: 1.4, ..Default::default() });
                if captain {
                    g.vfx.ball(x, 1.5, z, 3.2, hex("#ffb040"), 0.45); g.vfx.ring(x, z, 12.0, hex("#ffd27a"), 0.8, 0.05); g.vfx.pillar(x, z, 1.2, 26.0, hex("#ffc860"), 1.2, 0.3);
                    g.flash = g.flash.max(0.2); shake!(0.7);
                    if live { audio.kill(true); }
                }
            }
            BattleEvent::Chain { chain, n, x, y, z, heavy, ax, ay, az, .. } => {
                // 群衆ドミノ: 弾から巻き込まれた敵へ光の線（網のように広がって、どこを連鎖が走ったか残る）・足元の輪・その場の連鎖の数
                let (xd, yd, zd) = (x, y, z);
                let (x, y, z) = (x as f32, y as f32, z as f32);
                let hot = n >= 30;
                let col = hex(if heavy { "#ff7ac0" } else if hot { "#ff9ad0" } else { "#ffc040" });
                g.vfx.link(ax as f32, ay as f32, az as f32, x, y, z, col, chain, if heavy { 0.5 } else { 0.32 }, LINK_LIFE);
                g.vfx.ring(x, z, (if heavy { 2.6 } else { 1.5 }) * (if prank >= 2 { 1.4 } else { 1.0 }), col, 0.35, if heavy { 0.12 } else { 0.1 });
                g.vfx.burst(Burst { n: if heavy { 12 } else { 7 }, x, y, z, color: hex("#ffe08a"), colors: &[hex("#ffe08a"), hex("#ffffff"), hex("#ff9a3a")], type_: 0, speed: if heavy { 16.0 } else { 11.0 }, up: 0.6, size: 0.13, life: 0.3, stretch: 0.05, drag: 3.0, ..Default::default() });
                g.vfx.burst(Burst { n: 1, x, y, z, color: hex(if hot { "#ff9ad0" } else { "#ffd27a" }), type_: 2, speed: 0.0, size: if heavy { 2.4 } else { 1.6 }, life: 0.18, gravity: 0.0, ..Default::default() });
                shake!(if heavy { 0.05 } else { 0.018 });
                if live { audio.chain(n as i32, heavy); hud.chain(chain, n); hud.chain_pop(xd, yd, zd, n); }
                if n % 10 == 0 {
                    // 節目の間: その場に大きな光の輪と星（計算側がほんの一瞬スローにする）
                    g.vfx.ring(x, z, 5.5, hex(if hot { "#ff9ad0" } else { "#ffe08a" }), 0.55, 0.06);
                    g.vfx.ring(x, z, 3.2, hex("#ffffff"), 0.35, 0.1);
                    g.vfx.burst(Burst { n: 1, x, y: y + 0.3, z, color: hex("#ffffff"), type_: 2, speed: 0.0, size: 4.2, life: 0.3, gravity: 0.0, ..Default::default() });
                    g.vfx.burst(Burst { n: 18, x, y, z, color: hex("#ffe08a"), colors: &[hex("#ffe08a"), hex("#ffffff"), hex("#ff9ad0")], type_: 3, speed: 9.0, up: 1.2, size: 0.3, life: 1.0, ..Default::default() });
                    g.flash = g.flash.max(0.06); g.zoom_blur = g.zoom_blur.max(0.05); shake!(0.12);
                    if live { audio.chain_mark(n as i32); }
                }
                if n >= 6 { g.chain_cam = (g.chain_cam + 0.12).min(1.0); g.chain_at = lerpv(g.chain_at, DVec3::new(xd, 1.2, zd), 0.25); } else if n == 1 { g.chain_at = DVec3::new(xd, 1.2, zd); }
            }
            BattleEvent::Detonate { x, z, i } => {
                // 乱舞の起爆: 近い順に少しずつずらして弾ける（音は数回にまとめる）
                if i == 0 && live { hud.say("起爆", "金の敵がまとめて弾けた", Tone::Gold); audio.duck(0.2, 1.4); }
                g.delayed.push((now_ms + 60.0 + i as f64 * 45.0, Delayed::Detonate { x: x as f32, z: z as f32, i, live }));
            }
            BattleEvent::CaptainBreak { x, z, .. } => {
                let (x, z) = (x as f32, z as f32);
                g.vfx.ring(x, z, 9.0, hex("#ff9ac8"), 0.6, 0.05);
                g.vfx.ring(x, z, 5.0, hex("#ffffff"), 0.4, 0.1);
                g.vfx.crack(x, z, 4.0, None);
                g.vfx.burst(Burst { n: 40, x, y: 1.5, z, color: hex("#ffd27a"), colors: &[hex("#ffd27a"), hex("#ff9ac8"), hex("#ffffff")], type_: 3, speed: 12.0, up: 1.2, size: 0.35, life: 1.2, ..Default::default() });
                shake!(0.6); g.flash = g.flash.max(0.12); g.zoom_blur = g.zoom_blur.max(0.08);
                if live { hud.say("隊長崩し", "重騎をぶつけた ── 今が好機", Tone::Gold); audio.ring(); audio.kill(true); }
            }
            BattleEvent::Grab { .. } => {
                // つかんだ瞬間: 足元から光の柱と土煙、白い閃き（計算側が一瞬止めてゆっくりにする）
                g.vfx.burst(Burst { n: 1, x: px, y: 1.6, z: pz, color: hex("#ffffff"), type_: 2, speed: 0.0, size: 4.5, life: 0.22, gravity: 0.0, ..Default::default() });
                g.vfx.burst(Burst { n: 30, x: px, y: 2.4, z: pz, color: hex("#ffd27a"), colors: &[hex("#ffd27a"), hex("#ff9ac8"), hex("#ffffff")], type_: 3, speed: 9.0, up: 1.4, size: 0.34, life: 1.0, ..Default::default() });
                g.vfx.burst(Burst { n: 12, x: px, y: 0.3, z: pz, color: hex("#a89080"), type_: 4, speed: 7.0, up: 0.2, size: 1.2, life: 0.7, drag: 3.0, ..Default::default() });
                g.vfx.pillar(px, pz, 1.4, 7.0, hex("#ff9ac8"), 0.5, 1.2);
                g.vfx.ring(px, pz, 4.5, hex("#ff9ac8"), 0.45, 0.08);
                g.vfx.crack(px, pz, 2.2, None);
                shake!(0.45); g.flash = g.flash.max(0.08); g.zoom_blur = g.zoom_blur.max(0.07);
                if live { audio.land(true); audio.ring(); }
            }
            BattleEvent::Hurl { x, z, .. } => {
                // 放つ: 白い閃き・正面への光の筋・衝撃の輪
                let at = DVec3::new(x + face_d.sin() * 8.0, 1.2, z + face_d.cos() * 8.0);
                let (x, z) = (x as f32, z as f32);
                let (fx_, fz_) = (pface.sin(), pface.cos());
                g.vfx.burst(Burst { n: 1, x, y: 1.6, z, color: hex("#ffffff"), type_: 2, speed: 0.0, size: 5.0, life: 0.2, gravity: 0.0, ..Default::default() });
                g.vfx.burst(Burst { n: 36, x, y: 1.5, z, color: hex("#ffe08a"), colors: &[hex("#ffe08a"), hex("#ff9ac8"), hex("#ffffff")], type_: 0, speed: 30.0, up: 0.2, size: 0.17, life: 0.4, dir: Some([fx_, 0.05, fz_]), spread: 0.35, stretch: 0.08, ..Default::default() });
                g.vfx.ring(px, pz, 5.0, hex("#ffe08a"), 0.4, 0.06);
                g.speed = g.speed.max(1.0); g.zoom_blur = g.zoom_blur.max(0.1); shake!(0.4); g.flash = g.flash.max(0.06);
                g.chain_cam = (g.chain_cam + 0.35).min(1.0); g.chain_at = at;
                if live { audio.swing(true); audio.blast(); }
            }
            BattleEvent::HurlBoom { x, z, .. } => {
                // 着弾: 大きな火の玉・光の柱・地割れ
                let (x, z) = (x as f32, z as f32);
                g.vfx.ball(x, 1.2, z, 3.2, hex("#ff9ac8"), 0.4);
                g.vfx.ring(x, z, 8.0, hex("#fff0c0"), 0.6, 0.05);
                g.vfx.ring(x, z, 5.0, hex("#ff9ac8"), 0.45, 0.12);
                g.vfx.pillar(x, z, 2.0, 16.0, hex("#ffd0e8"), 0.7, 0.5);
                g.vfx.crack(x, z, 4.2, None);
                g.vfx.burst(Burst { n: 50, x, y: 1.0, z, color: hex("#ffd27a"), colors: &[hex("#ffd27a"), hex("#ff9ac8"), hex("#ffffff")], type_: 0, speed: 28.0, up: 0.8, size: 0.18, life: 0.6, stretch: 0.05, ..Default::default() });
                g.vfx.burst(Burst { n: 18, x, y: 0.3, z, color: hex("#8a7466"), type_: 4, speed: 14.0, up: 0.1, size: 1.4, life: 1.0, drag: 3.0, ..Default::default() });
                shake!(0.9); g.flash = g.flash.max(0.16); g.zoom_blur = g.zoom_blur.max(0.1);
                if live { audio.blast(); audio.ring(); }
            }
            BattleEvent::Reflect { x, y, z, mirror } => {
                // 飴鏡で跳ね返った: 当たった所が光って、澄んだ音
                let (x, y, z) = (x as f32, y as f32, z as f32);
                g.mirrors.flash(mirror as usize, x, y, z);
                g.vfx.burst(Burst { n: 1, x, y, z, color: hex("#fff2e0"), type_: 2, speed: 0.0, size: 3.2, life: 0.2, gravity: 0.0, ..Default::default() });
                g.vfx.burst(Burst { n: 14, x, y, z, color: hex("#ffd0e8"), colors: &[hex("#ffd0e8"), hex("#ffffff"), hex("#ffb060")], type_: 0, speed: 14.0, up: 0.5, size: 0.13, life: 0.35, stretch: 0.05, drag: 3.0, ..Default::default() });
                shake!(0.06);
                if live { audio.reflect(); }
            }
            BattleEvent::RankUp { rank } => {
                // 格が上がる: 武器が一度ふくらんでから落ち着く（スローや見出しは出さない）
                g.hero.pop();
                g.vfx.ring(px, pz, 3.0 + rank as f32, hex("#ffd27a"), 0.4, 0.08);
                g.vfx.burst(Burst { n: 16 + rank as u32 * 8, x: px, y: 1.4, z: pz, color: hex("#ffd27a"), colors: &[hex("#ffd27a"), hex("#ffffff"), tint], type_: 3, speed: 6.0, up: 1.4, size: 0.28, life: 0.9, ..Default::default() });
                if live { hud.set_rank(rank); audio.rank_up(rank); }
            }
            BattleEvent::ChainEnd { chain, n, kills, bonus, x, z } => {
                if n >= 5 { g.vfx.link_flash(chain); }
                // お菓子の花火（2026-11-15 見た目の磨き）: 10 体以上の連鎖が終わると、網の中心から桃・ミント・蜂蜜の粒が空へ開き、色の輪が広がる
                if n >= 10 {
                    let (cx, cz) = (x as f32, z as f32);
                    let big = (n as f32 / 10.0).min(3.0);
                    for (k, col) in ["#ff8fbf", "#9fe8cc", "#ffd27a", "#c8a8ff"].iter().enumerate() {
                        g.vfx.burst(Burst { n: (10.0 * big) as u32, x: cx, y: 2.5 + k as f32 * 0.6, z: cz, color: hex(col), colors: &[hex(col), hex("#fffaf0")], type_: 1, speed: 9.0 + big * 2.0, up: 1.6, size: 0.42, life: 1.4, gravity: 3.5, drag: 1.2, ..Default::default() });
                    }
                    g.vfx.ring(cx, cz, 6.0 + big * 2.0, hex("#ff8fbf"), 0.7, 0.08);
                    g.vfx.ring(cx, cz, 4.0 + big * 1.5, hex("#9fe8cc"), 0.55, 0.1);
                }
                if live {
                    hud.chain_end(chain, n);
                    // 大連鎖は大見出しに得点を添える（網の上の表示と重ねない）
                    if (5..30).contains(&n) { hud.chain_reward(x, z, kills, bonus as i64); }
                    if n >= 30 { hud.say("大連鎖", &format!("{n} 体を巻き込んだ ／ +{} 点", fmt_num(bonus as i64)), Tone::Gold); audio.chain_big(n as i32); shake!(0.3); g.zoom_blur = g.zoom_blur.max(0.06); }
                }
            }
            BattleEvent::Vanish { kind, x, z, .. } => {
                let (x, z) = (x as f32, z as f32);
                g.vfx.burst(Burst { n: 2, x, y: 0.5, z, color: hex("#c8a8b8"), type_: 4, speed: 2.0, up: 0.6, size: 0.45, life: 0.35, gravity: -1.5, ..Default::default() });
                g.vfx.burst(Burst { n: 4, x, y: 0.7, z, color: hex(sparkle(kind)), type_: 1, speed: 3.0, up: 1.0, size: 0.5, life: 0.4, gravity: -2.0, ..Default::default() });
                if live { audio.pop(); }
            }
            BattleEvent::Hurt { x, z, big, .. } => {
                g.red_k = g.red_k.max(if big { 0.55 } else { 0.35 }); shake!(if big { 0.5 } else { 0.25 });
                g.vfx.burst(Burst { n: 10, x: x as f32, y: 1.2, z: z as f32, color: hex("#ff5a5a"), type_: 0, speed: 10.0, size: 0.12, life: 0.3, stretch: 0.05, ..Default::default() });
                if live { audio.hurt(big); }
            }
            BattleEvent::Wire { x, y, z, target } => {
                // 飴糸を撃った: 足元の土ぼこり、腰から粉砂糖の噴射、糸の先の光。低い引き音（風切り）と張る音
                if live { audio.swing(true); audio.reflect(); }
                g.zoom_blur = g.zoom_blur.max(0.05);
                if py < 0.5 { g.vfx.burst(Burst { n: 10, x: px, y: 0.2, z: pz, color: hex("#b8a090"), type_: 4, speed: 5.0, up: 0.1, size: 0.8, life: 0.5, ..Default::default() }); }
                let (bx, bz) = (-pface.sin(), -pface.cos());
                g.vfx.burst(Burst { n: 10, x: px + bx * 0.6, y: py + 0.9, z: pz + bz * 0.6, color: hex("#cfc6bc"), colors: &[hex("#cfc6bc"), hex("#bfb3a6")], type_: 4, speed: 7.0, up: 0.3, size: 0.45, life: 0.45, dir: Some([bx, 0.15, bz]), spread: 0.6, drag: 4.0, ..Default::default() });
                if !g.battle.player.wire.esc { g.taffy = None; }
                g.vfx.ball(x as f32, y as f32, z as f32, if target { 1.4 } else { 0.9 }, hex("#ffb84a"), 0.25);
            }
            BattleEvent::WireStrike { x, y, z, boss, chain } => {
                // 糸斬り: 着いた所で飴がはじける（当たりの白い光・音は Hit が出す）。続くほど大きい。札を 1 枚出す
                let (x, y, z) = (x as f32, y as f32, z as f32);
                let big = 1.0 + (chain as f32 - 1.0) * 0.35;
                shake!(0.4 + chain as f64 * 0.12); g.zoom_blur = g.zoom_blur.max(0.06 + chain as f64 * 0.02); g.flash = g.flash.max(0.12);
                g.vfx.ball(x, y, z, 1.3 * big, hex("#fff2c8"), 0.16);
                g.vfx.burst(Burst { n: (16.0 * big) as u32, x, y, z, color: hex("#ffb84a"), colors: &[hex("#ffb84a"), hex("#ffd98a"), hex("#ffffff"), tint], type_: 3, speed: 8.0 * big, up: 0.8, size: 0.26, life: 0.7, gravity: 12.0, ..Default::default() });
                if y < 3.0 { g.vfx.ring(x, z, if chain >= 3 { 5.6 } else { 4.2 }, hex("#ffb84a"), 0.35, 0.1); }
                // 一閃: 敵を横切る薄い刃の弧。二撃目は逆の傾き、三撃目は十字と、まわりを囲む螺旋の弧
                let tilts: &[f64] = match chain { 1 => &[28.0], 2 => &[-32.0, 70.0], _ => &[38.0, -38.0, 0.0, 75.0] };
                for (i, t) in tilts.iter().enumerate() {
                    let ring = chain >= 3 && i >= 2;
                    let arc = MoveArc { t: 0.0, tilt: t.to_radians(), from: if ring { 3.0 } else { 2.4 }, to: if ring { -3.0 } else { -2.4 }, r: if ring { 4.6 } else { 3.4 + chain as f64 * 0.3 }, y: -0.6, dur: if ring { 0.22 } else { 0.13 }, width: if ring { 0.7 } else { 0.45 }, kind: if chain >= 3 { ArcKind::Ranbu } else { ArcKind::Heavy } };
                    g.vfx.slash(&arc, x, y, z, pface + if ring { i as f32 * 0.9 } else { 0.0 }, tint, 1.0);
                }
                g.hero.pop();
                if chain >= 3 {
                    let hip = Vec3::new(px, py + 0.75, pz);
                    let to = Vec3::new(x, y, z);
                    for i in 0..14 {
                        let q = hip.lerp(to, i as f32 / 13.0) - Vec3::Y * (i as f32 / 13.0 * std::f32::consts::PI).sin() * 0.4;
                        g.vfx.burst(Burst { n: 2, x: q.x, y: q.y, z: q.z, color: hex("#ffae38"), colors: &[hex("#ffae38"), hex("#ffd98a"), hex("#fff2c8")], type_: 1, speed: 2.5, up: 0.6, size: 0.2, life: 0.9, gravity: 10.0, drag: 1.5, ..Default::default() });
                    }
                    g.taffy = None;
                    if live { audio.reflect(); }
                }
                g.delayed.push((now_ms + 95.0, Delayed::Shard { x, y, z, dx: pface.sin(), dz: pface.cos(), n: chain, live }));
                if live { audio.wire_strike(chain as i32); if chain < 3 { audio.blast(); } }
                if live { hud.impact(if chain >= 3 { 1.0 } else { 0.55 }); }
                g.taffy = Some((Vec3::new(x, y, z), 0.0)); if live { hud.fuda(chain, g.battle.player.wire.target, [x as f64, y as f64, z as f64]); }
                // 巨人の上の大将: あと何回で崩れるか
                let (on, hits) = (g.battle.giant.on, g.battle.giant.wire_hits);
                if live && boss && on && hits < WIRE_TOPPLE { hud.say("頭上の一撃", &format!("あと {} 回で巨人が崩れる", WIRE_TOPPLE - hits), Tone::Gold); }
            }
            BattleEvent::Inoshikacho { x, y, z } => {
                // 猪鹿蝶: 札の色（朱・山吹・白）の花びらが舞い、主役が叫ぶ
                let (x, y, z) = (x as f32, y as f32, z as f32);
                shake!(0.6); g.flash = g.flash.max(0.16); g.zoom_blur = g.zoom_blur.max(0.1);
                g.vfx.burst(Burst { n: 60, x, y, z, color: hex("#e2553f"), colors: &[hex("#e2553f"), hex("#e2ab2f"), hex("#fff3d6"), hex("#ef7d95")], type_: 1, speed: 11.0, up: 1.2, size: 0.5, life: 1.3, gravity: 3.0, ..Default::default() });
                g.vfx.ring(x, z, 9.0, hex("#e2ab2f"), 0.6, 0.1);
                let id = Some(g.heroes[g.hero_i].id.clone()).filter(|_| g.heroes[g.hero_i].package.is_none());
                if let Some(id) = id { if live { audio.voice(&id, 1.0); } }
                if live {
                    g.delayed.push((now_ms + 330.0, Delayed::Finale)); audio.duck(0.35, 1.6);
                    // 決め絵: 的のまわりを回るカメラへ切り替える
                    g.fin_cam = Some((DVec3::new(x as f64, y as f64, z as f64), 0.0, if g.rng.f() < 0.5 { -1.0 } else { 1.0 }));
                }
            }
            BattleEvent::Ame { .. } => { if live { audio.ame_gain(); } }
            BattleEvent::NoAme => { if live { audio.ame_empty(); hud.ame_empty(); } }
            BattleEvent::Ride => {
                // わたあめ雲に乗った: 足元にわたあめがふくらむ（専用の音はまだ無いので、跳ぶ音と金平糖の音を借りる）
                if live { audio.jump(); audio.pickup(crate::audio::PickupKind::Konpeito); }
                g.vfx.burst(Burst { n: 16, x: px, y: py - 0.3, z: pz, color: hex("#ffd6ea"), colors: &[hex("#ffd6ea"), hex("#ffb8d8"), hex("#ffc2dd")], type_: 4, speed: 3.5, up: 0.2, size: 1.0, life: 0.6, ..Default::default() });
            }
            BattleEvent::RideEnd { melted } => {
                // 雲が消える: わたあめがちぎれて散る
                if live { audio.pop(); }
                g.vfx.burst(Burst { n: if melted { 22 } else { 14 }, x: px, y: py - 0.5, z: pz, color: hex("#ffd6ea"), colors: &[hex("#ffd6ea"), hex("#ffb8d8"), hex("#ffc2dd")], type_: 4, speed: 4.5, up: 0.3, size: 1.1, life: 0.7, ..Default::default() });
            }
            BattleEvent::Jump => { if live { audio.jump(); } g.vfx.burst(Burst { n: 8, x: px, y: 0.2, z: pz, color: hex("#b8a090"), type_: 4, speed: 4.0, up: 0.1, size: 0.9, life: 0.6, ..Default::default() }); }
            BattleEvent::Land { heavy } => { if heavy { shake!(0.2); } if live { audio.land(heavy); } }
            BattleEvent::Dodge => { if live { audio.dodge(); } g.speed = g.speed.max(0.3); g.vfx.burst(Burst { n: 10, x: px, y: 0.2, z: pz, color: hex("#b8a090"), type_: 4, speed: 5.0, up: 0.1, size: 0.8, life: 0.5, ..Default::default() }); }
            BattleEvent::RanbuReady => { if live { audio.ranbu_ready(); } g.vfx.burst(Burst { n: 30, x: px, y: 1.0, z: pz, color: hex("#ffd27a"), type_: 1, speed: 6.0, up: 1.4, size: 0.4, life: 0.8, gravity: -3.0, ..Default::default() }); }
            BattleEvent::RanbuStart => {
                g.cutin_t = 0.0;
                if live { audio.ranbu_start(); screens.cutin_start(); screens.letterbox(true); }
                g.flash = 0.25; g.gold = 1.0;
                g.vfx.pillar(px, pz, 2.6, 22.0, hex("#ff9a20"), RANBU_CUTIN + 0.4, 0.5);
                g.vfx.ring(px, pz, 10.0, hex("#ffd27a"), 0.9, 0.06);
            }
            BattleEvent::RanbuFinish => {
                g.flash = 0.45; shake!(1.0); g.zoom_blur = 0.2;
                if live { audio.ranbu_finish(); screens.letterbox(false); }
            }
            BattleEvent::CaptainAppear { id, hold } => {
                if live {
                    audio.captain();
                    let k = g.battle.enemies.iter().find(|x| x.id == id).map(|x| STAGE.kind_name(x.kind)).unwrap_or(STAGE.kind_name(sim::battle::EnemyKind::Shortcake));
                    hud.say("敵将 出現", &format!("{} ── {k}", STAGE.holds()[hold as usize]), Tone::Red);
                }
                let h = &g.battle.holds[hold as usize];
                g.vfx.ring(h.x as f32, h.z as f32, 14.0, hex("#ff3a5a"), 1.0, 0.04);
            }
            BattleEvent::Capture { hold, left, fled, to } => {
                if live {
                    audio.capture();
                    let sub = if to >= 0 && fled > 0 { format!("守り {fled} 体が {} へ逃げる ── 残り {left}", STAGE.holds()[to as usize]) } else if left > 0 { format!("{} を落とした ── 残り {left}", STAGE.holds()[hold as usize]) } else { "全ての砦を落とした".to_string() };
                    hud.say("砦 制圧", &sub, Tone::Gold);
                }
                g.world.flash_hold(hold as usize);
                let (hx, hz) = { let h = &g.battle.holds[hold as usize]; (h.x as f32, h.z as f32) };
                g.vfx.pillar(hx, hz, 1.4, 34.0, hex("#ffc860"), 1.6, 0.3);
                g.vfx.burst(Burst { n: 70, x: hx, y: 8.0, z: hz, color: hex("#ffd27a"), colors: &[hex("#ffd27a"), hex("#fff3e0"), hex("#ff9a40")], type_: 3, speed: 12.0, up: 1.2, size: 0.4, life: 2.0, ..Default::default() });
            }
            BattleEvent::BossGate => { if live { hud.say("本陣の門が開く", "北の本陣へ攻め込め", Tone::Red); audio.captain(); } }
            BattleEvent::BossEnter => {
                g.boss_intro_t = 0.0;
                // 灯をともす場の中心は、大将が着地するあたり（主役の 13m 手前）
                {
                    let (dx, dz) = (g.battle.boss.x as f32 - px, g.battle.boss.z as f32 - pz);
                    let dl = dx.hypot(dz).max(0.001);
                    g.boss_fx.light_up(px + dx / dl * 13.0, pz + dz / dl * 13.0);
                }
                // 名前の札は、着地して顔の寄りへ切り替わるのに合わせて出す
                if live { audio.boss(); screens.letterbox(true); g.delayed.push((now_ms + 900.0, Delayed::BossTrack)); }
                shake!(0.6);
            }
            BattleEvent::TelegraphFire { tel: t } => {
                let (tx, tz, tr) = (t.x as f32, t.z as f32, t.r as f32);
                if t.kind == TelegraphKind::Ring {
                    let grow = t.grow.unwrap_or(1.0);
                    g.vfx.ring(tx, tz, tr, hex("#ff6a3a"), grow, 0.05);
                    g.vfx.ring(tx, tz, tr * 0.95, hex("#ffd27a"), grow * 1.05, 0.03);
                    g.vfx.crack(tx, tz, 6.0, Some(hex("#ff5a2a")));
                    g.vfx.burst(Burst { n: 40, x: tx, y: 0.4, z: tz, color: hex("#9a7060"), type_: 4, speed: 14.0, up: 0.1, size: 2.0, life: 1.3, drag: 2.0, ..Default::default() });
                    // 踏み潰し: 光の柱・火花・もう一重の輪・画面の光とぶれで、必殺技らしく見せる
                    g.vfx.pillar(tx, tz, 3.2, 20.0, hex("#ff9a5a"), 0.9, 0.5);
                    g.vfx.ring(tx, tz, tr * 0.6, hex("#fff0c0"), grow * 0.8, 0.09);
                    g.vfx.crack(tx, tz, 10.0, Some(hex("#ff5a2a")));
                    g.vfx.burst(Burst { n: 60, x: tx, y: 0.5, z: tz, color: hex("#ffb060"), type_: 0, speed: 26.0, up: 0.9, size: 0.2, life: 0.8, stretch: 0.05, ..Default::default() });
                    g.flash = g.flash.max(0.28); g.zoom_blur = g.zoom_blur.max(0.16);
                    shake!(1.0); if live { audio.boss_hit_ground(); }
                } else if t.kind == TelegraphKind::Cone {
                    let (fx_, fz_) = ((t.face as f32).sin(), (t.face as f32).cos());
                    g.vfx.burst(Burst { n: 30, x: tx + fx_ * 5.0, y: 0.5, z: tz + fz_ * 5.0, color: hex("#9a7060"), type_: 4, speed: 10.0, up: 0.3, size: 1.6, life: 1.0, ..Default::default() });
                    g.vfx.crack(tx + fx_ * 5.0, tz + fz_ * 5.0, 4.0, Some(hex("#ff5a2a")));
                    shake!(0.6); if live { audio.boss_hit_ground(); }
                } else if t.src == TelegraphSrc::Sauce {
                    g.vfx.burst(Burst { n: 14, x: tx, y: 0.3, z: tz, color: hex("#7a3a1a"), colors: &[hex("#7a3a1a"), hex("#b0602a"), hex("#e09a40")], type_: 3, speed: 9.0, up: 1.3, size: 0.35, life: 0.9, ..Default::default() });
                    g.vfx.ring(tx, tz, tr * 1.2, hex("#ff8a3a"), 0.4, 0.1);
                    shake!(0.15); if live { audio.hit(2, true); }
                }
            }
            BattleEvent::BossStomp { x, z } => {
                let (x, z) = (x as f32, z as f32);
                g.vfx.ring(x, z, 5.5, hex("#ff6a3a"), 0.45, 0.08); g.vfx.crack(x, z, 3.4, Some(hex("#ff5a2a")));
                g.vfx.burst(Burst { n: 18, x, y: 0.3, z, color: hex("#9a7060"), type_: 4, speed: 8.0, up: 0.2, size: 1.4, life: 1.0, ..Default::default() });
                shake!(0.45); if live { audio.boss_hit_ground(); }
            }
            BattleEvent::BossLand { x, z } => {
                let (x, z) = (x as f32, z as f32);
                g.vfx.ring(x, z, 16.0, hex("#ff6a3a"), 0.9, 0.05); g.vfx.ring(x, z, 9.0, hex("#ffd27a"), 0.6, 0.1); g.vfx.crack(x, z, 8.0, Some(hex("#ff5a2a")));
                g.vfx.burst(Burst { n: 60, x, y: 0.5, z, color: hex("#9a7060"), type_: 4, speed: 16.0, up: 0.15, size: 2.4, life: 1.6, drag: 2.0, ..Default::default() });
                g.vfx.burst(Burst { n: 40, x, y: 0.5, z, color: hex("#ffb060"), type_: 0, speed: 24.0, up: 0.9, size: 0.18, life: 0.7, stretch: 0.04, ..Default::default() });
                shake!(1.0); g.flash = g.flash.max(0.3); g.zoom_blur = g.zoom_blur.max(0.15);
                if live { audio.boss_hit_ground(); }
            }
            BattleEvent::GiantForm { x, z } => {
                // 巨人が組み上がる: 大将のまわりから光の柱と輪
                let (x, z) = (x as f32, z as f32);
                g.vfx.ring(x, z, 20.0, hex("#ff6a3a"), 1.2, 0.05); g.vfx.ring(x, z, 10.0, hex("#ffd27a"), 0.9, 0.1);
                g.vfx.pillar(x, z, 4.0, 22.0, hex("#ff9a5a"), 2.4, 0.6);
                shake!(0.7); g.flash = g.flash.max(0.12);
                g.boss_cut_t = 0.0;
                if live { hud.say(STAGE.giant(), "雑兵が積み重なって巨人になる", Tone::Red); audio.boss(); screens.letterbox(true); }
            }
            BattleEvent::GiantArm { x, z, .. } => {
                let (x, z) = (x as f32, z as f32);
                g.vfx.ring(x, z, 7.0, hex("#ffd27a"), 0.6, 0.06);
                g.vfx.burst(Burst { n: 40, x, y: 3.0, z, color: hex("#ffd27a"), colors: &[hex("#ffd27a"), hex("#ff9ac8"), hex("#ffffff")], type_: 3, speed: 10.0, up: 1.0, size: 0.35, life: 1.2, ..Default::default() });
                shake!(0.5); g.flash = g.flash.max(0.08);
                if live { audio.ring(); audio.kill(true); }
            }
            BattleEvent::GiantFall { x, z } => {
                // 巨人が崩れる: 土煙と大きな輪、雑兵の雨
                let (x, z) = (x as f32, z as f32);
                g.vfx.ring(x, z, 22.0, hex("#fff0c0"), 1.0, 0.05); g.vfx.ring(x, z, 12.0, hex("#ff9a3a"), 0.8, 0.1); g.vfx.crack(x, z, 10.0, Some(hex("#ff5a2a")));
                g.vfx.burst(Burst { n: 70, x, y: 1.0, z, color: hex("#9a7060"), type_: 4, speed: 14.0, up: 0.2, size: 2.6, life: 1.8, drag: 2.0, ..Default::default() });
                g.vfx.burst(Burst { n: 60, x, y: 6.0, z, color: hex("#ffd27a"), colors: &[hex("#ffd27a"), hex("#ff9ac8"), hex("#ffffff")], type_: 3, speed: 12.0, up: 1.2, size: 0.4, life: 1.6, ..Default::default() });
                shake!(1.0); g.flash = g.flash.max(0.25); g.zoom_blur = g.zoom_blur.max(0.12);
                if live { hud.say("巨人 崩落", "大将が落ちた ── 叩き込め", Tone::Gold); audio.boss_hit_ground(); audio.chain_big(60); }
            }
            BattleEvent::BossDown => { if live { hud.say("崩れた", "今が好機 ── 叩き込め", Tone::Gold); audio.capture(); } }
            BattleEvent::BossDefeated => {
                if live { hud.say("大将 討ち取ったり", "", Tone::Gold); audio.stop_music(); audio.kill(true); screens.letterbox(true); }
                g.flash = 0.5; shake!(1.0); g.zoom_blur = 0.18;
                let (bx, bz) = (g.battle.boss.x as f32, g.battle.boss.z as f32);
                g.vfx.ball(bx, 3.0, bz, 5.0, hex("#ff9a40"), 0.7); g.vfx.ring(bx, bz, 20.0, hex("#ffd27a"), 1.2, 0.04);
                g.vfx.burst(Burst { n: 160, x: bx, y: 4.0, z: bz, color: hex("#ffd27a"), colors: &[hex("#ffd27a"), hex("#fff3e0"), hex("#ff6a7a"), hex("#c8743a")], type_: 3, speed: 20.0, up: 1.2, size: 0.5, life: 2.4, ..Default::default() });
            }
            BattleEvent::Milestone { kos } => { if live { hud.say(&kanji_ko(kos), "", Tone::Plain); audio.milestone(); } }
            BattleEvent::Pickup { kind, x, z } => {
                if live { audio.pickup(if kind == PickupKind::Bun { crate::audio::PickupKind::Bun } else { crate::audio::PickupKind::Konpeito }); }
                g.vfx.burst(Burst { n: 20, x: x as f32, y: 0.8, z: z as f32, color: hex(if kind == PickupKind::Bun { "#8affb0" } else { "#ffd27a" }), type_: 1, speed: 4.0, up: 2.0, size: 0.35, life: 0.8, gravity: -4.0, ..Default::default() });
            }
            BattleEvent::EnemyStrike { x, z, .. } => { if live && ((x as f32 - px).powi(2) + (z as f32 - pz).powi(2)).sqrt() < 6.0 { audio.enemy_strike(); } }
            BattleEvent::Throw { .. } => { if live { audio.throw_sfx(); } }
            BattleEvent::Victory => { if live { g.result_t = 0.0; audio.play_track(Track::Victory); } }
            BattleEvent::Defeat => { if live { g.defeat_t = 0.0; audio.defeat(); hud.say("無念", "", Tone::Red); } }
            _ => {}
        }
        let _ = Owner::Ally;
    }
}

fn weapon_of(store: &Store, id: &str) -> String {
    store.get_str(&format!("gyogun.weapon.{id}")).or_else(|| default_weapon(id).map(str::to_string)).unwrap_or_else(|| "cake_sword".to_string())
}

/// 大将の登場で、跳ぶ絵から顔の寄りへ切り替える秒（着地 1.3 秒の少しあと）
const BOSS_CUT_AT: f64 = 1.45;

/// 戦いの間のカメラ（app.ts の updateCamera）。倍精度のまま
fn update_camera(g: &mut Game, dt: f64, c: Option<(f64, f64)>, aspect: f64, prev_rot: glam::DMat4) {
    let b = &g.battle;
    let p = &b.player;
    let now = g.t_all;
    if let Some((cam, drag)) = c { g.cam_yaw += cam * dt * 2.4 + drag; }
    let (mut dist, mut h) = (9.4f64, 5.0f64);
    // 連続で斬っている間（連撃 8 以上・技の最中）は低く寄って、主役を大きく見せる（2026-11-15 見た目の磨き）
    let streak = p.combo >= 8 && matches!(p.state, PlayerState::Attack | PlayerState::Free) && !p.air;
    g.close_cam += ((if streak { 1.0 } else { 0.0 }) - g.close_cam) * (dt * if streak { 1.6 } else { 0.8 }).min(1.0);
    { let k = g.close_cam * g.close_cam * (3.0 - 2.0 * g.close_cam); dist -= k * 2.6; h -= k * 1.6; }
    let mut look = DVec3::new(p.x, p.y * 0.6 + 1.3, p.z);
    // わたあめ雲: 少し引いて、下の戦場も見せる
    if p.state == PlayerState::Ride { dist += 2.5; h += 1.0; }
    let boss_on = b.boss.state != BossState::Dormant && b.boss.state != BossState::Dead;
    if boss_on {
        let d = (b.boss.x - p.x).hypot(b.boss.z - p.z);
        dist += (10.0 - d).max(0.0) * 0.35 + 2.0; h += 2.2;
        // 巨人は見上げるほど大きいので、引いて上も見せる
        g.giant_cam += ((if b.giant.on { 1.0 } else { 0.0 }) - g.giant_cam) * (dt * 1.5).min(1.0);
        dist += g.giant_cam * 7.0; h += g.giant_cam * 3.0;
        // 跳び上がって踏み潰すときは、少し引いて見上げる（大将が画面の上へ切れない）
        let hop = g.boss_view.hop() as f64;
        dist += hop * 0.9; h -= hop * 0.35;
        look = lerpv(look, DVec3::new(b.boss.x, 3.0 + g.giant_cam * 5.0 + hop * 0.75, b.boss.z), 0.22 + g.giant_cam * 0.18 + (hop * 0.06).min(0.3));
    }
    // 飴糸: 斬り込む間は、主役と的を一画面に収める斜め後ろへ寄せる。空へ逃げる間は引いて的も見せる
    let mut snap = false;
    let w = p.wire;
    let dive = p.state == PlayerState::Wire && !w.esc && w.target != 0;
    let esc = p.state == PlayerState::Wire && w.esc;
    let chain_on = p.wire_chain > 0 && p.wire_chain_t > 0.0;
    g.wire_cam += ((if dive || esc || chain_on { 1.0 } else { 0.0 }) - g.wire_cam) * (dt * 4.0).min(1.0);
    if g.wire_cam > 0.01 {
        let wk = g.wire_cam * g.wire_cam * (3.0 - 2.0 * g.wire_cam);
        let at = DVec3::new(w.ax, w.ay, w.az);
        // 何撃目へ斬り込むか（1: 横から急接近、2: 上空から急降下、3: 武将の向こうから正面に受ける）
        let n_next = if p.wire_chain > 0 && p.wire_chain_t > 0.0 { p.wire_chain + 1 } else { 1 };
        dist += wk * if esc { 1.6 } else if dive { match n_next { 2 => -1.4, 3 => 0.0, _ => -0.9 } } else { 0.4 };
        h += wk * if dive { match n_next { 2 => 5.5, 3 => 1.4, _ => -0.6 } } else { 0.6 };
        look = lerpv(look, at, wk * if dive { match n_next { 2 => 0.5, 3 => 0.35, _ => 0.3 } } else { 0.25 });
        if dive && n_next == 3 {
            // 武将の向こう側に置く（主役が画面の奥から飛び込んでくる）
            dist = dist.max((look - at).length() + 6.5);
        }
        // 空へ逃げる・空にいる間は、低い所から見上げて主役を空に抜く（武将は画面の下に残る）
        if !dive && p.air && p.y > 2.0 {
            h -= wk * 3.6;
            look.y = look.y + wk * (p.y * 0.55);
        }
        if dive {
            // 進む向きの斜め後ろへ、ゆっくり回り込む（振り子のふくらむ側の逆から見る）
            let want = match n_next { 3 => p.face + w.swing.signum() * 0.35, 2 => p.face + std::f64::consts::PI - w.swing.signum() * 0.3, _ => p.face + std::f64::consts::PI - w.swing.signum() * 1.15 };
            let mut d = (want - g.cam_yaw) % std::f64::consts::TAU;
            if d > std::f64::consts::PI { d -= std::f64::consts::TAU; } else if d < -std::f64::consts::PI { d += std::f64::consts::TAU; }
            // 三撃目は切り替え（カット）。ほかは回り込む
            if n_next == 3 && g.wire_cut != p.move_key { g.wire_cut = p.move_key; g.cam_yaw = want; snap = true; }
            else { g.cam_yaw += d * (dt * if n_next == 2 { 3.0 } else { 2.2 }).min(1.0); }
        }
    }
    g.chain_cam = (g.chain_cam - dt * 0.7).max(0.0);
    if g.chain_cam > 0.0 && !boss_on {
        let k = g.chain_cam * g.chain_cam * (3.0 - 2.0 * g.chain_cam);
        dist += k * 4.5; h += k * 3.0;
        look = lerpv(look, g.chain_at, k * 0.35);
    }
    let mut yaw = g.cam_yaw;
    // 乱舞の切り込み: 寄って、ゆっくり回る
    let m = p.r#move;
    let is_ranbu = m.map(|m| same(m, &RANBU)).unwrap_or(false);
    if is_ranbu && p.move_t < RANBU_CUTIN + 0.2 { dist = 5.2; h = 1.6; yaw = g.cam_yaw + 0.6 + p.move_t * 0.5; look.y = p.y + 1.4; }
    else if is_ranbu { dist = 13.0; h = 6.5; }
    // 猪鹿蝶の決め絵: 的のまわりを低くゆっくり回る（実時間で 1.1 秒）
    let fin = g.fin_cam.map(|(c, t, y0)| (c, t + dt, y0)).filter(|f| f.1 < 1.1);
    g.fin_cam = fin;
    if let Some((c, t, side)) = fin {
        // 主役を手前、斬った的を奥に。主役の肩ごしの低い所から、横へゆっくり回り込む
        let hero = DVec3::new(p.x, p.y + 1.0, p.z);
        let (ax, az) = (p.x - c.x, p.z - c.z);
        let al = ax.hypot(az).max(0.5);
        let (ux, uz) = (ax / al, az / al);
        let e = 1.0 - (1.0 - (t / 1.1)).powi(2);
        let a = side.signum() * (0.85 + e * 0.45);
        let (rx, rz) = (ux * a.cos() + uz * a.sin(), uz * a.cos() - ux * a.sin());
        // 低すぎると周りの敵の中へ入るので、敵の背より上から（主役の高さ + 1.6m、地面から 2.6m 以上）
        let target = DVec3::new(hero.x + rx * (3.3 + e * 1.6), (hero.y + 0.5 + e * 0.7).max(2.4), hero.z + rz * (3.3 + e * 1.6));
        let k = if t <= dt + 1e-6 { 1.0 } else { (dt * 10.0).min(1.0) };
        g.cam_pos = lerpv(g.cam_pos, target, k);
        g.cam_look = lerpv(g.cam_look, lerpv(hero, DVec3::new(c.x, c.y.max(1.0) + 0.6, c.z), 0.38), k);
        if t + dt >= 1.1 { g.cam_yaw = (target.x - p.x).atan2(target.z - p.z); }
    } else if g.boss_intro_t >= 0.0 {
        let (bx, bz) = (b.boss.x, b.boss.z);
        let (ax, az) = (p.x - bx, p.z - bz);
        let al = ax.hypot(az);
        let al = if al == 0.0 { 1.0 } else { al };
        // 大将 → 主役の向き
        let (ux, uz) = (ax / al, az / al);
        if g.boss_intro_t < BOSS_CUT_AT {
            // 跳ぶ: 主役の肩ごしの低い位置から、残照を背に跳んでくる大将を見上げる
            let target = DVec3::new(p.x + ux * 6.5 + uz * 2.8, 1.5, p.z + uz * 6.5 - ux * 2.8);
            g.cam_pos = lerpv(g.cam_pos, target, (dt * 4.0).min(1.0));
            g.cam_look = lerpv(g.cam_look, DVec3::new(bx, 4.5 + b.boss_y, bz), (dt * 6.0).min(1.0));
        } else {
            // 着地: 顔の寄りへ切り替える（切り替えの最初のコマは飛ぶ）。足元からゆっくり寄る
            let t = g.boss_intro_t - BOSS_CUT_AT;
            let d = 9.6 - t * 0.9;
            let target = DVec3::new(bx + ux * d + uz * 3.2, 1.3 + t * 0.2, bz + uz * d - ux * 3.2);
            let look = DVec3::new(bx - uz * 0.6, 5.6, bz + ux * 0.6);
            let k = if t < 0.05 { 1.0 } else { (dt * 3.0).min(1.0) };
            g.cam_pos = lerpv(g.cam_pos, target, k);
            g.cam_look = lerpv(g.cam_look, look, k);
        }
    } else if g.boss_cut_t >= 0.0 && boss_on {
        // 体力が半分: 主役の肩ごしの低い位置から、赤く焼ける大将を見上げる
        let (ax, az) = (p.x - b.boss.x, p.z - b.boss.z);
        let al = ax.hypot(az);
        let al = if al == 0.0 { 1.0 } else { al };
        let (ux, uz) = (ax / al, az / al);
        let target = DVec3::new(p.x + ux * 5.5 + uz * 2.6, 1.2, p.z + uz * 5.5 - ux * 2.6);
        g.cam_pos = lerpv(g.cam_pos, target, (dt * 5.0).min(1.0));
        g.cam_look = lerpv(g.cam_look, DVec3::new(b.boss.x, 6.2, b.boss.z), (dt * 6.0).min(1.0));
    } else if b.phase == Phase::Victory {
        // 勝鬨: 倒れた大将を背に、主役を正面からゆっくり回って見せる（カメラは大将と反対側）
        let away = (p.x - b.boss.x).atan2(p.z - b.boss.z);
        let a = away + (now * 0.3).sin() * 0.6;
        g.cam_pos = lerpv(g.cam_pos, DVec3::new(p.x + a.sin() * 8.0, 3.2, p.z + a.cos() * 8.0), (dt * 2.0).min(1.0));
        g.cam_look = lerpv(g.cam_look, DVec3::new((p.x * 2.0 + b.boss.x) / 3.0, 2.2, (p.z * 2.0 + b.boss.z) / 3.0), (dt * 3.0).min(1.0));
    } else {
        let target = DVec3::new(look.x + yaw.sin() * dist, look.y + h, look.z + yaw.cos() * dist);
        let k = if (is_ranbu && p.move_t < RANBU_CUTIN) || snap { 1.0 } else { (dt * (7.0 + 7.0 * g.wire_cam)).min(1.0) };
        g.cam_pos = lerpv(g.cam_pos, target, k);
        g.cam_look = lerpv(g.cam_look, look, if snap { 1.0 } else { (dt * 12.0).min(1.0) });
    }
    // 揺れ（大きさは trauma の二乗）
    let s = g.trauma * g.trauma;
    let mut pos = g.cam_pos;
    pos.x += ((now * 61.0).sin() + (now * 37.0).sin() * 0.5) * s * 0.45;
    pos.y += ((now * 53.0).sin() + (now * 29.0).sin() * 0.5) * s * 0.35;
    // 差が小さいときは今の版も投影を作り直さない
    let want = 52.0 + g.fov_kick * 8.0 + g.speed * 4.0;
    let fov = if (g.camera.fov - want).abs() > 0.05 { want } else { g.camera.fov };
    g.camera = Camera::new(pos, g.cam_look, (now * 43.0).sin() * s * 0.03, fov, aspect, prev_rot);
    g.trauma = (g.trauma - dt * 1.6).max(0.0);
}

#[cfg(not(target_arch = "wasm32"))]
fn rss_kb() -> i64 {
    std::process::Command::new("ps").args(["-o", "rss=", "-p", &std::process::id().to_string()]).output().ok()
        .and_then(|o| String::from_utf8(o.stdout).ok()).and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

/// このプロセスが使った CPU の秒
#[cfg(not(target_arch = "wasm32"))]
fn cpu_s() -> f64 {
    let out = std::process::Command::new("ps").args(["-o", "cputime=", "-p", &std::process::id().to_string()]).output().ok().and_then(|o| String::from_utf8(o.stdout).ok()).unwrap_or_default();
    let mut t = 0.0;
    for p in out.trim().split(':') { t = t * 60.0 + p.parse::<f64>().unwrap_or(0.0); }
    t
}

// ブラウザではプロセスの値が取れない（外の道具で Chrome 全体を測る）
#[cfg(target_arch = "wasm32")]
fn rss_kb() -> i64 { 0 }
#[cfg(target_arch = "wasm32")]
fn cpu_s() -> f64 { 0.0 }
