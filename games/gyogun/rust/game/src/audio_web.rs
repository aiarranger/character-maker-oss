// 音（ブラウザの版）: src/audio.ts を WebAudio のまま写したもの。音の計算はブラウザの音のスレッドで行う。
// BGM はこのゲームのために Stable Audio 3 で作った 4 曲（public/music/。タイトル・戦闘・大将・勝利）、効果音は aisns の Kenney（CC0）と、このゲームのために Stable Audio 3 で作った音（public/sfx/README.md）、
// それに合成の低音（太鼓・どん）を重ねる。同じ音が重なりすぎないよう、同時に鳴る数を絞る。
// 公開の名前と引数は audio.rs（Mac の版）と同じ。BGM は HTMLAudioElement を MediaElementSource でつなぐ（少しずつ読むので、最初の音が遅れず、曲全体をメモリに持たない）。<audio> は media（media_web.rs）で使い回す。
use crate::assets::BASE;
use crate::media::{MediaSlots, MediaVoice};
use game_ui::page::fetch as fetch_bytes;
use js_sys::{Math, Uint8Array};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{AudioBuffer, AudioContext, AudioContextState, AudioScheduledSourceNode, BiquadFilterType, GainNode, OscillatorType};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Track {
    Title,
    Field,
    Boss,
    Victory,
}
impl Track {
    pub const ALL: [Track; 4] = [Track::Title, Track::Field, Track::Boss, Track::Victory];
    fn def(self) -> (&'static str, f32) {
        match self {
            Track::Title => ("title.m4a", 1.0),
            Track::Field => ("field.m4a", 0.95),
            Track::Boss => ("boss.m4a", 0.9),
            Track::Victory => ("victory.m4a", 1.4),
        }
    }
}

/// 拾った物（TS の 'bun' | 'konpeito'）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickupKind {
    Bun,
    Konpeito,
}

const WILDS: [&str; 23] = ["slash-swing-1", "slash-swing-2", "slash-hit-1", "slash-hit-2", "slash-hit-3", "slash-crit", "crit", "hit-1", "hit-2", "hit-3", "taken", "ws", "swish", "engage", "ko", "ready", "warp", "defeat", "select", "lit", "notice", "cast", "spell"];
const OWN: [&str; 8] = ["boom", "whoosh-heavy", "taiko", "shatter", "roar", "ranbu-rise", "crowd-hit", "gong"];

/// 和音階（ド・レ・ミ・ソ・ラ）の半音の数
const PENTA: [i32; 5] = [0, 2, 4, 7, 9];
/// 自己ベストの祝いの和音（ソ・シ・レ・高いソ。上向きに 1 音ずつ）
const CHEER_NOTES: [f32; 4] = [783.99, 987.77, 1174.66, 1567.98];

fn pow2(semi: f32) -> f32 {
    2f32.powf(semi / 12.0)
}

// ---- 保存する設定（localStorage 'gyogun.audio'） ----

const SETTINGS_KEY: &str = "gyogun.audio";

#[derive(Clone, Copy)]
struct Settings {
    music: f64,
    sfx: f64,
    muted: bool,
}
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}
fn load_settings() -> Settings {
    let mut s = Settings { music: 0.5, sfx: 0.85, muted: false };
    // 保存できなくても鳴らす
    let Some(v) = storage().and_then(|l| l.get_item(SETTINGS_KEY).ok()?).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok()) else { return s };
    if let Some(x) = v.get("music").and_then(|x| x.as_f64()) { s.music = x; }
    if let Some(x) = v.get("sfx").and_then(|x| x.as_f64()) { s.sfx = x; }
    if let Some(x) = v.get("muted").and_then(|x| x.as_bool()) { s.muted = x; }
    s
}
fn save_settings(s: Settings) {
    if let Some(l) = storage() {
        // JSON.stringify と同じ順と書き方
        let _ = l.set_item(SETTINGS_KEY, &format!("{{\"music\":{},\"sfx\":{},\"muted\":{}}}", s.music, s.sfx, s.muted));
    }
}

// ---- 音のつながり ----

/// master → compressor → 出力、sfx・music → master
struct Graph {
    ctx: AudioContext,
    master: GainNode,
    sfx: GainNode,
    music: GainNode,
}

fn build_graph(settings: Settings) -> Result<Graph, JsValue> {
    let ctx = AudioContext::new()?;
    let master = ctx.create_gain()?;
    master.gain().set_value(if settings.muted { 0.0 } else { 1.0 });
    let comp = ctx.create_dynamics_compressor()?;
    comp.threshold().set_value(-14.0);
    comp.ratio().set_value(4.0);
    comp.attack().set_value(0.003);
    comp.release().set_value(0.2);
    master.connect_with_audio_node(&comp)?;
    comp.connect_with_audio_node(&ctx.destination())?;
    let sfx = ctx.create_gain()?;
    sfx.gain().set_value(settings.sfx as f32);
    sfx.connect_with_audio_node(&master)?;
    let music = ctx.create_gain()?;
    music.gain().set_value(settings.music as f32);
    music.connect_with_audio_node(&master)?;
    Ok(Graph { ctx, master, sfx, music })
}

/// 取って解く
async fn decode(ctx: &AudioContext, url: &str) -> Result<AudioBuffer, JsValue> {
    let bytes = fetch_bytes(url).await.map_err(|e| JsValue::from_str(&e))?;
    let buffer = Uint8Array::from(bytes.as_slice()).buffer();
    Ok(JsFuture::from(ctx.decode_audio_data(&buffer)?).await?.unchecked_into())
}

/// 鳴らしている曲
struct Current {
    track: Track,
    voice: MediaVoice,
}

/// sample の指定（TS の o）
#[derive(Clone, Copy)]
struct S {
    vol: f32,
    rate: f32,
    jitter: f32,
    at: f64,
    gap: f64,
}
const D: S = S { vol: 1.0, rate: 1.0, jitter: 0.06, at: 0.0, gap: 0.03 };

pub struct Audio {
    graph: RefCell<Option<Rc<Graph>>>,
    samples: Rc<RefCell<HashMap<&'static str, AudioBuffer>>>,
    /// 主役ごとの決め台詞（主役の id → 声）
    lines: Rc<RefCell<HashMap<String, AudioBuffer>>>,
    settings: Cell<Settings>,
    current: RefCell<Option<Current>>,
    last_at: RefCell<HashMap<&'static str, f64>>,
    voices: Rc<Cell<i32>>,
    last_pluck: Cell<f64>,
    /// 曲の <audio> を使い回す（iPhone・iPad は操作の直後に一度鳴らした <audio> でないと後から鳴らない。media_web.rs）
    media: MediaSlots,
}

pub fn create_audio() -> Audio {
    Audio::new()
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

impl Audio {
    pub fn new() -> Self {
        Self {
            graph: RefCell::new(None),
            samples: Rc::default(),
            lines: Rc::default(),
            settings: Cell::new(load_settings()),
            current: RefCell::default(),
            last_at: RefCell::default(),
            voices: Rc::default(),
            last_pluck: Cell::new(-1.0),
            media: MediaSlots::new(4),
        }
    }

    fn graph(&self) -> Option<Rc<Graph>> {
        self.graph.borrow().clone()
    }

    pub fn init(&self) {
        if let Some(g) = self.graph() {
            // iOS は suspended のほかに interrupted でも止まる。止められた曲は media が次の操作で鳴らし直す
            let st = g.ctx.state();
            if st != AudioContextState::Running && st != AudioContextState::Closed { let _ = g.ctx.resume(); }
            return;
        }
        let Ok(g) = build_graph(self.settings.get()) else { return };
        self.media.set_ctx(&g.ctx);
        let g = Rc::new(g);
        *self.graph.borrow_mut() = Some(g.clone());
        let load = |name: &'static str, url: String| {
            let (g, samples) = (g.clone(), self.samples.clone());
            spawn_local(async move {
                if let Ok(buf) = decode(&g.ctx, &url).await { samples.borrow_mut().insert(name, buf); }
            });
        };
        for f in WILDS { load(f, format!("./{BASE}sounds/wilds/{f}.mp3")); }
        for f in OWN { load(f, format!("./sfx/{f}.wav")); }
    }

    fn sample(&self, name: &'static str, o: S) -> bool {
        let Some(g) = self.graph() else { return false };
        let Some(buf) = self.samples.borrow().get(name).cloned() else { return false };
        let t = g.ctx.current_time();
        if self.last_at.borrow().get(name).copied().unwrap_or(-1.0) > t - o.gap { return true; }
        if self.voices.get() > 24 { return true; }
        self.last_at.borrow_mut().insert(name, t);
        let _ = self.play_sample(&g, &buf, o, t);
        true
    }
    fn play_sample(&self, g: &Graph, buf: &AudioBuffer, o: S, t: f64) -> Result<(), JsValue> {
        let src = g.ctx.create_buffer_source()?;
        src.set_buffer(Some(buf));
        src.playback_rate().set_value(o.rate * (1.0 + (Math::random() as f32 - 0.5) * 2.0 * o.jitter));
        let gain = g.ctx.create_gain()?;
        gain.gain().set_value(o.vol);
        src.connect_with_audio_node(&gain)?;
        gain.connect_with_audio_node(&g.sfx)?;
        let voices = self.voices.clone();
        let ended = Closure::once_into_js(move || voices.set(voices.get() - 1));
        AudioScheduledSourceNode::set_onended(&src, Some(ended.unchecked_ref()));
        src.start_with_when(t + o.at)?;
        self.voices.set(self.voices.get() + 1);
        Ok(())
    }

    /// 合成の低い一撃（太鼓・どん）
    fn thump(&self, f0: f32, f1: f32, dur: f32, vol: f32, noise: f32) {
        if let Some(g) = self.graph() { let _ = Self::play_thump(&g, f0, f1, dur, vol, noise); }
    }
    fn play_thump(g: &Graph, f0: f32, f1: f32, dur: f32, vol: f32, noise: f32) -> Result<(), JsValue> {
        let ctx = &g.ctx;
        let t = ctx.current_time();
        let d = dur as f64;
        let o = ctx.create_oscillator()?;
        o.set_type(OscillatorType::Sine);
        o.frequency().set_value_at_time(f0, t)?;
        o.frequency().exponential_ramp_to_value_at_time(f1.max(20.0), t + d)?;
        let gain = ctx.create_gain()?;
        gain.gain().set_value_at_time(0.0001, t)?;
        gain.gain().exponential_ramp_to_value_at_time(vol, t + 0.005)?;
        gain.gain().exponential_ramp_to_value_at_time(0.0001, t + d)?;
        o.connect_with_audio_node(&gain)?;
        gain.connect_with_audio_node(&g.sfx)?;
        o.start_with_when(t)?;
        o.stop_with_when(t + d + 0.05)?;
        if noise > 0.0 {
            let rate = ctx.sample_rate();
            let len = (rate * dur.min(0.4)).floor() as u32;
            let nb = ctx.create_buffer(1, len, rate)?;
            let mut data: Vec<f32> = (0..len).map(|i| (Math::random() as f32 * 2.0 - 1.0) * (1.0 - i as f32 / len as f32).powi(4)).collect();
            nb.copy_to_channel(&mut data, 0)?;
            let ns = ctx.create_buffer_source()?;
            ns.set_buffer(Some(&nb));
            let lp = ctx.create_biquad_filter()?;
            lp.set_type(BiquadFilterType::Lowpass);
            lp.frequency().set_value(1400.0);
            let ng = ctx.create_gain()?;
            ng.gain().set_value(vol * noise);
            ns.connect_with_audio_node(&lp)?;
            lp.connect_with_audio_node(&ng)?;
            ng.connect_with_audio_node(&g.sfx)?;
            ns.start_with_when(t)?;
        }
        Ok(())
    }

    /// はじく音（三角波＋1オクターブ上のきらめき）。bright で余韻を長く。at 秒あとに鳴らすもの（和音の分散）は間引かない
    fn pluck(&self, freq: f32, vol: f32, bright: bool, at: f64) {
        let Some(g) = self.graph() else { return };
        let mut t = g.ctx.current_time();
        if at < 0.0 {
            if t - self.last_pluck.get() < 0.035 { return; }
            self.last_pluck.set(t);
        } else {
            t += at;
        }
        let dur = if bright { 0.5 } else { 0.28 };
        for (mul, ty, k) in [(1.0, OscillatorType::Triangle, 1.0), (2.0, OscillatorType::Sine, 0.35)] {
            let _ = Self::play_pluck(&g, freq * mul, ty, vol * k, t, dur);
        }
    }
    fn play_pluck(g: &Graph, freq: f32, ty: OscillatorType, vol: f32, t: f64, dur: f64) -> Result<(), JsValue> {
        let o = g.ctx.create_oscillator()?;
        o.set_type(ty);
        o.frequency().set_value(freq);
        let gain = g.ctx.create_gain()?;
        gain.gain().set_value_at_time(0.0001, t)?;
        gain.gain().exponential_ramp_to_value_at_time(vol, t + 0.004)?;
        gain.gain().exponential_ramp_to_value_at_time(0.0001, t + dur)?;
        o.connect_with_audio_node(&gain)?;
        gain.connect_with_audio_node(&g.sfx)?;
        o.start_with_when(t)?;
        o.stop_with_when(t + dur + 0.05)?;
        Ok(())
    }

    /// 上がっていく光の音（ゲージ満タン・乱舞）
    fn rise(&self, dur: f64, vol: f32) {
        let Some(g) = self.graph() else { return };
        let t = g.ctx.current_time();
        for (f0, f1) in [(220.0, 880.0), (330.0, 1320.0)] {
            let _ = Self::play_rise(&g, f0, f1, t, dur, vol);
        }
    }
    fn play_rise(g: &Graph, f0: f32, f1: f32, t: f64, dur: f64, vol: f32) -> Result<(), JsValue> {
        let o = g.ctx.create_oscillator()?;
        o.set_type(OscillatorType::Sawtooth);
        o.frequency().set_value_at_time(f0, t)?;
        o.frequency().exponential_ramp_to_value_at_time(f1, t + dur)?;
        let lp = g.ctx.create_biquad_filter()?;
        lp.set_type(BiquadFilterType::Lowpass);
        lp.frequency().set_value_at_time(600.0, t)?;
        lp.frequency().exponential_ramp_to_value_at_time(5000.0, t + dur)?;
        let gain = g.ctx.create_gain()?;
        gain.gain().set_value_at_time(0.0001, t)?;
        gain.gain().exponential_ramp_to_value_at_time(vol, t + dur * 0.8)?;
        gain.gain().exponential_ramp_to_value_at_time(0.0001, t + dur + 0.1)?;
        o.connect_with_audio_node(&lp)?;
        lp.connect_with_audio_node(&gain)?;
        gain.connect_with_audio_node(&g.sfx)?;
        o.start_with_when(t)?;
        o.stop_with_when(t + dur + 0.2)?;
        Ok(())
    }

    pub fn play_track(&self, track: Track) {
        self.init();
        let Some(g) = self.graph() else { return };
        let mut current = self.current.borrow_mut();
        if current.as_ref().is_some_and(|c| c.track == track) { return; }
        if let Some(prev) = current.take() {
            let t = g.ctx.current_time();
            let p = prev.voice.gain.gain();
            let _ = p.cancel_scheduled_values(t);
            let _ = p.set_value_at_time(p.value(), t);
            let _ = p.linear_ramp_to_value_at_time(0.0, t + 1.2);
            prev.voice.stop_after(1400);
        }
        let (file, vol) = track.def();
        // 断られても黙って続け、次の操作で鳴らし直す（media）
        let Ok(voice) = self.media.start(&g.ctx, &format!("./music/{file}"), track != Track::Victory, &g.music, 0.0) else { return };
        let t = g.ctx.current_time();
        let _ = voice.gain.gain().linear_ramp_to_value_at_time(vol, t + 1.0);
        *current = Some(Current { track, voice });
    }
    pub fn stop_music(&self) {
        let Some(c) = self.current.borrow_mut().take() else { return };
        let Some(g) = self.graph() else { return };
        let t = g.ctx.current_time();
        let _ = c.voice.gain.gain().linear_ramp_to_value_at_time(0.0, t + 0.8);
        c.voice.stop_after(900);
    }
    /// BGM を一瞬しぼる（乱舞の切り込み・大将の登場）
    pub fn duck(&self, amount: f32, dur: f32) {
        let Some(g) = self.graph() else { return };
        let t = g.ctx.current_time();
        let music = self.settings.get().music as f32;
        let p = g.music.gain();
        let _ = p.cancel_scheduled_values(t);
        let _ = p.set_value_at_time(music * amount, t);
        let _ = p.linear_ramp_to_value_at_time(music, t + dur as f64);
    }
    pub fn muted(&self) -> bool {
        self.settings.get().muted
    }
    pub fn toggle_mute(&self) -> bool {
        let mut s = self.settings.get();
        s.muted = !s.muted;
        self.settings.set(s);
        if let Some(g) = self.graph() { g.master.gain().set_value(if s.muted { 0.0 } else { 1.0 }); }
        save_settings(s);
        s.muted
    }
    pub fn swing(&self, heavy: bool) {
        if heavy {
            if !self.sample("whoosh-heavy", S { vol: 0.75, jitter: 0.08, ..D }) {
                self.sample("ws", S { vol: 0.8, rate: 0.8, ..D });
            }
        } else {
            let n = if Math::random() < 0.5 { "slash-swing-1" } else { "slash-swing-2" };
            self.sample(n, S { vol: 0.55, jitter: 0.1, gap: 0.05, ..D });
        }
    }
    pub fn hit(&self, count: i32, heavy: bool) {
        let n = ["slash-hit-1", "slash-hit-2", "slash-hit-3"][((Math::random() * 3.0) as usize).min(2)];
        self.sample(n, S { vol: 0.5 + (count as f32 * 0.04).min(0.4), jitter: 0.12, gap: 0.04, ..D });
        if count >= 4 && !self.sample("crowd-hit", S { vol: (0.3 + count as f32 * 0.05).min(0.9), jitter: 0.08, gap: 0.06, ..D }) {
            self.sample("hit-2", S { vol: 0.5, rate: 0.8, ..D });
        }
        if heavy {
            self.sample("slash-crit", S { vol: 0.8, ..D });
            self.thump(140.0, 45.0, 0.35, 0.55, 0.4);
        }
    }
    pub fn blast(&self) {
        if !self.sample("boom", S { vol: 0.95, jitter: 0.05, ..D }) {
            self.thump(90.0, 30.0, 0.8, 0.9, 0.8);
        }
        self.thump(70.0, 28.0, 0.6, 0.6, 0.2);
    }
    pub fn ring(&self) {
        self.thump(110.0, 34.0, 0.55, 0.8, 0.6);
        self.sample("taiko", S { vol: 0.7, ..D });
    }
    pub fn kill(&self, captain: bool) {
        if !captain {
            return;
        }
        if !self.sample("shatter", S { vol: 0.9, ..D }) {
            self.sample("crit", D);
        }
        self.thump(60.0, 25.0, 1.1, 0.9, 0.5);
        self.sample("gong", S { vol: 0.8, ..D });
    }
    /// 群衆ドミノ: ぶつかるたびの打音に、和音階（ド・レ・ミ・ソ・ラ）で一段ずつ上がる音を重ねる（連鎖が旋律になる）
    pub fn chain(&self, n: i32, heavy: bool) {
        self.sample(if heavy { "hit-3" } else { "hit-2" }, S { vol: if heavy { 0.5 } else { 0.32 }, rate: 0.9, jitter: 0.05, gap: 0.045, ..D });
        let i = (n - 1).rem_euclid(15); // 3 オクターブで一巡り
        let semi = (i / 5) * 12 + PENTA[(i % 5) as usize];
        self.pluck(330.0 * pow2(semi as f32), if heavy { 0.16 } else { 0.12 }, n % 5 == 0, -1.0);
        if n % 10 == 0 {
            self.sample("taiko", S { vol: (0.4 + n as f32 * 0.008).min(0.9), rate: 1.0 + (n as f32 * 0.006).min(0.5), ..D });
        }
    }
    /// 節目（10・20・30…体）: 鈴の和音
    pub fn chain_mark(&self, n: i32) {
        let f = 660.0 * pow2(PENTA[(n.min(50) / 10 - 1).rem_euclid(5) as usize] as f32);
        self.pluck(f, 0.15, true, 0.0);
        self.pluck(f * 1.5, 0.11, true, 0.05);
        self.pluck(f * 2.0, 0.09, true, 0.1);
    }
    pub fn chain_big(&self, n: i32) {
        self.sample("lit", S { vol: 0.9, ..D });
        self.sample("crowd-hit", S { vol: 0.9, ..D });
        self.thump(80.0, 30.0, 0.7, (0.5 + n as f32 * 0.006).min(1.0), 0.4);
    }
    pub fn pop(&self) {
        self.sample("hit-1", S { vol: 0.12, rate: 1.6, jitter: 0.2, gap: 0.05, ..D });
    }
    pub fn hurt(&self, big: bool) {
        self.sample("taken", S { vol: if big { 1.0 } else { 0.75 }, rate: if big { 0.85 } else { 1.0 }, ..D });
        if big {
            self.thump(120.0, 40.0, 0.4, 0.6, 0.5);
        }
    }
    pub fn jump(&self) {
        self.sample("swish", S { vol: 0.5, rate: 1.2, ..D });
    }
    pub fn land(&self, heavy: bool) {
        if heavy {
            self.thump(100.0, 30.0, 0.5, 0.8, 0.7);
        } else {
            self.sample("hit-1", S { vol: 0.25, rate: 0.7, ..D });
        }
    }
    pub fn dodge(&self) {
        self.sample("swish", S { vol: 0.65, rate: 0.9, ..D });
    }
    /// 飴鏡で跳ね返る: 硝子を弾いたような高い音
    pub fn reflect(&self) {
        self.pluck(1568.0, 0.11, true, 0.0);
        self.pluck(2349.0, 0.06, true, 0.03);
        self.sample("hit-1", S { vol: 0.2, rate: 1.8, gap: 0.04, ..D });
    }
    /// 格が上がる: 澄んだ音が段ごとに高くなる
    pub fn rank_up(&self, rank: i32) {
        self.sample("lit", S { vol: 0.75, rate: 0.9 + rank as f32 * 0.12, ..D });
        let f = 523.0 * pow2(PENTA[(rank * 2).rem_euclid(5) as usize] as f32);
        self.pluck(f, 0.14, true, 0.0);
        self.pluck(f * 1.5, 0.1, true, 0.06);
        self.pluck(f * 2.0, 0.08, true, 0.12);
    }
    pub fn ranbu_ready(&self) {
        self.sample("ready", S { vol: 0.9, ..D });
        self.sample("lit", S { vol: 0.6, at: 0.08, ..D });
    }
    pub fn ranbu_start(&self) {
        self.duck(0.25, 2.2);
        if !self.sample("ranbu-rise", D) {
            self.rise(0.8, 0.12);
        }
        self.sample("engage", S { vol: 0.9, ..D });
        self.thump(80.0, 40.0, 0.5, 0.7, 0.3);
    }
    pub fn ranbu_finish(&self) {
        self.blast();
        self.sample("gong", S { vol: 0.7, ..D });
    }
    pub fn captain(&self) {
        self.sample("notice", S { vol: 0.9, ..D });
        if !self.sample("taiko", S { vol: 0.9, ..D }) {
            self.thump(90.0, 40.0, 0.5, 0.8, 0.3);
        }
    }
    pub fn capture(&self) {
        self.sample("warp", S { vol: 0.8, ..D });
        if !self.sample("gong", S { vol: 0.9, at: 0.1, ..D }) {
            self.thump(70.0, 30.0, 1.2, 0.8, 0.2);
        }
    }
    pub fn boss(&self) {
        self.duck(0.2, 4.0);
        if !self.sample("roar", D) {
            self.thump(55.0, 25.0, 1.6, 1.0, 0.9);
        }
        self.sample("taiko", S { at: 0.4, ..D });
    }
    pub fn boss_hit_ground(&self) {
        self.thump(70.0, 24.0, 0.9, 1.0, 0.9);
        self.sample("boom", S { vol: 0.7, rate: 0.8, ..D });
    }
    pub fn enemy_strike(&self) {
        self.sample("hit-3", S { vol: 0.2, rate: 1.3, gap: 0.08, ..D });
    }
    pub fn throw_sfx(&self) {
        self.sample("swish", S { vol: 0.25, rate: 1.5, gap: 0.1, ..D });
    }
    pub fn milestone(&self) {
        self.sample("lit", S { vol: 0.9, ..D });
        self.sample("taiko", S { vol: 0.8, ..D });
    }
    /// 主役の決め台詞（voice/<id>/inoshikacho.wav）を読んでおく。声の無い主役は鳴らないだけ
    pub fn load_voice(&self, id: &str) {
        let Some(g) = self.graph() else { return };
        if self.lines.borrow().contains_key(id) { return; }
        let (lines, id, url) = (self.lines.clone(), id.to_string(), format!("./voice/{id}/inoshikacho.wav"));
        spawn_local(async move {
            if let Ok(buf) = decode(&g.ctx, &url).await { lines.borrow_mut().insert(id, buf); }
        });
    }
    /// 主役の決め台詞を鳴らす（まだ読めていなければ、読みに行くだけ）
    pub fn voice(&self, id: &str, vol: f32) {
        let Some(g) = self.graph() else { return };
        let Some(buf) = self.lines.borrow().get(id).cloned() else { self.load_voice(id); return };
        let _ = self.play_sample(&g, &buf, S { vol, jitter: 0.0, ..D }, g.ctx.current_time());
    }
    /// 飴が 1 個増えた（飴玉が鳴る）／飴が無い
    pub fn ame_gain(&self) {
        self.pluck(1318.5, 0.08, true, 0.0);
        self.pluck(1975.5, 0.06, true, 0.05);
    }
    pub fn ame_empty(&self) {
        self.pluck(196.0, 0.1, false, 0.0);
    }
    /// 糸斬り（n = 1 猪・2 鹿・3 蝶。続くほど高く鳴る）
    pub fn wire_strike(&self, n: i32) {
        self.sample("slash-crit", S { vol: 0.85, rate: [1.0, 1.15, 1.32][(n.clamp(1, 3) - 1) as usize], jitter: 0.0, ..D });
        if n >= 3 { self.sample("gong", S { vol: 0.7, ..D }); self.sample("taiko", S { vol: 0.9, ..D }); }
    }
    /// 糸斬りのあと遅れて砂糖が割れる音（三撃目は低く重い）
    pub fn wire_crush(&self, n: i32) {
        self.sample("shatter", S { vol: if n >= 3 { 0.9 } else { 0.45 }, rate: if n >= 3 { 0.8 } else { 1.25 }, ..D });
        if n >= 3 { self.sample("boom", S { vol: 0.8, rate: 0.85, ..D }); }
    }
    pub fn pickup(&self, kind: PickupKind) {
        self.sample(if kind == PickupKind::Bun { "spell" } else { "cast" }, S { vol: 0.8, ..D });
    }
    /// 結果の得点の数え上げの刻み（進むほど高く。k は 0..1）。今ある合成音（撥ねる音）で鳴らす
    pub fn count_tick(&self, k: f32) {
        self.pluck(660.0 * pow2(k * 7.0), 0.05, false, 0.0);
    }
    /// 数え終わり（自己ベストでないとき）
    pub fn count_done(&self) {
        self.pluck(880.0, 0.09, true, 0.0);
    }
    /// 自己ベスト: 上向きの短い和音。いちばん上の格（S）のときは 1 音多い
    pub fn cheer(&self, top: bool) {
        self.sample("lit", S { vol: 0.55, ..D });
        for (i, f) in CHEER_NOTES[..if top { 4 } else { 3 }].iter().enumerate() { self.pluck(*f, 0.13, true, i as f64 * 0.07); }
    }
    /// S のときの、上からのもう 1 回
    pub fn cheer_encore(&self) {
        for (i, f) in [1567.98, 1975.53, 2349.32].iter().enumerate() { self.pluck(*f, 0.08, true, i as f64 * 0.05); }
    }
    pub fn select(&self) {
        self.sample("select", S { vol: 0.7, ..D });
    }
    pub fn confirm(&self) {
        self.sample("engage", S { vol: 0.8, ..D });
    }
    pub fn defeat(&self) {
        self.sample("defeat", S { vol: 0.9, ..D });
    }
}
