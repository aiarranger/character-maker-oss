// 音: 曲は public/music/tracks.json の場面ごと。効果音は public/sfx の生成音と、Web Audio の合成。src/audio.ts の写し。
// 足音・息・風は走りの状態から合成する（スタミナを見なくても状況が伝わる。企画 04）。歓声は生成した輪の音量を動かす。
// つなぎ（master → compressor → 出力、sfx → master、music → master）も値も TS と同じ。island-kit の AudioKit は pengo の形なので、ここは Web Audio を直に使う。
use island_kit::app::media::{MediaSlots, MediaVoice};
use island_kit::app::{raf, store};
use js_sys::Math::random;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{AudioBuffer, AudioContext, AudioContextState, AudioScheduledSourceNode, BiquadFilterNode, BiquadFilterType as F, GainNode, OscillatorType as O, Response};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scene { Title, Play, Final, Clear }
impl Scene { fn id(self) -> &'static str { match self { Scene::Title => "title", Scene::Play => "play", Scene::Final => "final", Scene::Clear => "clear" } } }

const SFX: [&str; 8] = ["gate", "crowd", "crowd_roar", "fanfare", "splash", "hurdle", "whoosh", "bell"];

struct Current { scene: Scene, voice: MediaVoice }
struct Graph { ctx: AudioContext, master: GainNode, sfx: GainNode, music: GainNode, noise: AudioBuffer, wind_gain: GainNode, wind_filter: BiquadFilterNode }

struct Inner {
    storage_key: String,
    music: Cell<f64>, sfx: Cell<f64>, muted: Cell<bool>,
    graph: RefCell<Option<Rc<Graph>>>,
    samples: RefCell<HashMap<String, AudioBuffer>>,
    tracks: RefCell<Option<Value>>,
    current: RefCell<Option<Current>>,
    last_at: RefCell<HashMap<String, f64>>,
    voices: Rc<Cell<i32>>,
    crowd: RefCell<Option<(GainNode, GainNode)>>,
    step_phase: Cell<f64>, breath_phase: Cell<f64>, last_run: Cell<f64>, last_bump: Cell<f64>,
    /// 曲の <audio> を使い回す（iPhone・iPad は操作の直後に一度鳴らした <audio> でないと後から鳴らない。island-kit の media）
    media: MediaSlots,
}

/// 複製しても同じ物を指す
#[derive(Clone)]
pub struct Audio { i: Rc<Inner> }

impl Audio {
    pub fn new(storage_key: &str) -> Audio {
        let (mut music, mut sfx, mut muted) = (0.5f64, 0.9f64, false);
        // 保存できなくても鳴らす
        if let Some(v) = store::get_json(storage_key) {
            if let Some(x) = v.get("music").and_then(|x| x.as_f64()) { music = x; }
            if let Some(x) = v.get("sfx").and_then(|x| x.as_f64()) { sfx = x; }
            if let Some(x) = v.get("muted").and_then(|x| x.as_bool()) { muted = x; }
        }
        let a = Audio { i: Rc::new(Inner {
            storage_key: storage_key.to_string(), music: Cell::new(music), sfx: Cell::new(sfx), muted: Cell::new(muted), graph: RefCell::new(None), samples: RefCell::default(), tracks: RefCell::new(None),
            current: RefCell::new(None), last_at: RefCell::default(), voices: Rc::new(Cell::new(0)), crowd: RefCell::new(None), step_phase: Cell::new(0.0), breath_phase: Cell::new(0.0), last_run: Cell::new(0.0), last_bump: Cell::new(0.0),
            media: MediaSlots::new(4),
        }) };
        let t = a.clone();
        spawn_local(async move {
            if let Some(v) = fetch_json("./music/tracks.json").await { *t.i.tracks.borrow_mut() = Some(v.get("tracks").cloned().unwrap_or(json!({}))); }
            else { *t.i.tracks.borrow_mut() = Some(json!({})); }
        });
        a
    }

    fn graph(&self) -> Option<Rc<Graph>> { self.i.graph.borrow().clone() }

    pub fn init(&self) {
        if let Some(g) = self.graph() {
            // iOS は suspended のほかに interrupted でも止まる。止められた曲は media が次の操作で鳴らし直す
            let st = g.ctx.state();
            if st != AudioContextState::Running && st != AudioContextState::Closed { let _ = g.ctx.resume(); }
            return;
        }
        let Ok(ctx) = AudioContext::new() else { return };
        self.i.media.set_ctx(&ctx);
        let gain = |v: f32| { let g = ctx.create_gain().unwrap(); g.gain().set_value(v); g };
        let master = gain(if self.i.muted.get() { 0.0 } else { 1.0 });
        let comp = ctx.create_dynamics_compressor().unwrap();
        comp.threshold().set_value(-12.0);
        comp.ratio().set_value(4.0);
        let _ = master.connect_with_audio_node(&comp);
        let _ = comp.connect_with_audio_node(&ctx.destination());
        let sfx = gain(self.i.sfx.get() as f32);
        let _ = sfx.connect_with_audio_node(&master);
        let music = gain(self.i.music.get() as f32);
        let _ = music.connect_with_audio_node(&master);
        let n = (ctx.sample_rate() * 2.0) as u32;
        let noise = ctx.create_buffer(1, n, ctx.sample_rate()).unwrap();
        let mut d = vec![0f32; n as usize];
        for x in d.iter_mut() { *x = (random() * 2.0 - 1.0) as f32; }
        let _ = noise.copy_to_channel(&d, 0);
        let src = ctx.create_buffer_source().unwrap();
        src.set_buffer(Some(&noise));
        src.set_loop(true);
        let wind_filter = ctx.create_biquad_filter().unwrap();
        wind_filter.set_type(F::Bandpass);
        wind_filter.frequency().set_value(900.0);
        wind_filter.q().set_value(0.6);
        let wind_gain = gain(0.0);
        let _ = src.connect_with_audio_node(&wind_filter);
        let _ = wind_filter.connect_with_audio_node(&wind_gain);
        let _ = wind_gain.connect_with_audio_node(&sfx);
        let _ = src.start();
        let g = Rc::new(Graph { ctx: ctx.clone(), master, sfx, music, noise, wind_gain, wind_filter });
        *self.i.graph.borrow_mut() = Some(g);
        for f in SFX {
            let me = self.clone();
            let ctx = ctx.clone();
            spawn_local(async move {
                let Some(ab) = fetch_bytes(&format!("./sfx/{f}.mp3")).await else { return };
                let Ok(p) = ctx.decode_audio_data(&ab) else { return };
                let Ok(buf) = JsFuture::from(p).await else { return };
                me.i.samples.borrow_mut().insert(f.to_string(), buf.unchecked_into());
                me.start_crowd();
            });
        }
    }

    fn start_crowd(&self) {
        let Some(g) = self.graph() else { return };
        if self.i.crowd.borrow().is_some() { return; }
        let (a, b) = { let s = self.i.samples.borrow(); (s.get("crowd").cloned(), s.get("crowd_roar").cloned()) };
        let (Some(a), Some(b)) = (a, b) else { return };
        let mk = |buf: &AudioBuffer| {
            let s = g.ctx.create_buffer_source().unwrap();
            s.set_buffer(Some(buf));
            s.set_loop(true);
            let gn = g.ctx.create_gain().unwrap();
            gn.gain().set_value(0.0);
            let _ = s.connect_with_audio_node(&gn);
            let _ = gn.connect_with_audio_node(&g.sfx);
            let _ = s.start();
            gn
        };
        *self.i.crowd.borrow_mut() = Some((mk(&a), mk(&b)));
    }

    fn now(&self) -> f64 { self.graph().map_or(0.0, |g| g.ctx.current_time()) }

    fn sample(&self, name: &str, vol: f32, rate: f32, jitter: f64, gap: f64) -> bool {
        let Some(g) = self.graph() else { return false };
        let Some(buf) = self.i.samples.borrow().get(name).cloned() else { return false };
        let t = g.ctx.current_time();
        if self.i.last_at.borrow().get(name).copied().unwrap_or(-1.0) > t - gap { return true; }
        if self.i.voices.get() > 24 { return true; }
        self.i.last_at.borrow_mut().insert(name.to_string(), t);
        let Ok(src) = g.ctx.create_buffer_source() else { return true };
        src.set_buffer(Some(&buf));
        src.playback_rate().set_value(rate * (1.0 + ((random() - 0.5) * 2.0 * jitter) as f32));
        let gn = g.ctx.create_gain().unwrap();
        gn.gain().set_value(vol);
        let _ = src.connect_with_audio_node(&gn);
        let _ = gn.connect_with_audio_node(&g.sfx);
        self.i.voices.set(self.i.voices.get() + 1);
        let voices = self.i.voices.clone();
        let cb = Closure::once_into_js(move || voices.set(voices.get() - 1));
        let sn: &AudioScheduledSourceNode = src.as_ref();
        sn.set_onended(Some(cb.unchecked_ref()));
        let _ = src.start_with_when(t);
        true
    }

    /// 音程の滑る短い音（低い一撃・合図）
    fn tone(&self, f0: f32, f1: f32, dur: f64, vol: f32, ty: O, at: f64) {
        let Some(g) = self.graph() else { return };
        let t = g.ctx.current_time() + at;
        let Ok(o) = g.ctx.create_oscillator() else { return };
        o.set_type(ty);
        let _ = o.frequency().set_value_at_time(f0, t);
        let _ = o.frequency().exponential_ramp_to_value_at_time(f1.max(20.0), t + dur);
        let gn = g.ctx.create_gain().unwrap();
        let _ = gn.gain().set_value_at_time(0.0001, t);
        let _ = gn.gain().exponential_ramp_to_value_at_time(vol, t + 0.006);
        let _ = gn.gain().exponential_ramp_to_value_at_time(0.0001, t + dur);
        let _ = o.connect_with_audio_node(&gn);
        let _ = gn.connect_with_audio_node(&g.sfx);
        let _ = o.start_with_when(t);
        let _ = o.stop_with_when(t + dur + 0.05);
    }
    fn noise_hit(&self, freq: f32, q: f32, dur: f64, vol: f32, ty: F) {
        let Some(g) = self.graph() else { return };
        let t = g.ctx.current_time();
        let Ok(s) = g.ctx.create_buffer_source() else { return };
        s.set_buffer(Some(&g.noise));
        s.set_loop(true);
        s.set_loop_start(random());
        let f: BiquadFilterNode = g.ctx.create_biquad_filter().unwrap();
        f.set_type(ty);
        f.frequency().set_value(freq);
        f.q().set_value(q);
        let gn = g.ctx.create_gain().unwrap();
        let _ = gn.gain().set_value_at_time(0.0001, t);
        let _ = gn.gain().exponential_ramp_to_value_at_time(vol, t + 0.008);
        let _ = gn.gain().exponential_ramp_to_value_at_time(0.0001, t + dur);
        let _ = s.connect_with_audio_node(&f);
        let _ = f.connect_with_audio_node(&gn);
        let _ = gn.connect_with_audio_node(&g.sfx);
        let _ = s.start_with_when_and_grain_offset(t, random());
        let sn: &AudioScheduledSourceNode = s.as_ref();
        let _ = sn.stop_with_when(t + dur + 0.05);
    }

    pub fn play(&self, scene: Scene) {
        self.init();
        let me = self.clone();
        spawn_local(async move {
            // await tracksReady
            while me.i.tracks.borrow().is_none() { sleep(30).await; }
            me.start_scene(scene);
        });
    }

    fn start_scene(&self, scene: Scene) {
        let Some(g) = self.graph() else { return };
        if self.i.current.borrow().as_ref().is_some_and(|c| c.scene == scene) { return; }
        if let Some(prev) = self.i.current.borrow_mut().take() {
            let t = g.ctx.current_time();
            let pg = prev.voice.gain.gain();
            let _ = pg.cancel_scheduled_values(t);
            let _ = pg.set_value_at_time(pg.value(), t);
            let _ = pg.linear_ramp_to_value_at_time(0.0, t + 0.9);
            prev.voice.stop_after(1100);
        }
        let def = self.i.tracks.borrow().as_ref().and_then(|t| t.get(scene.id()).cloned());
        let Some(def) = def else { return };
        let Some(src) = def.get("src").and_then(|s| s.as_str()) else { return };
        let looped = def.get("loop").and_then(|x| x.as_bool()).unwrap_or(scene != Scene::Clear);
        let Ok(voice) = self.i.media.start(&g.ctx, &format!("./{src}"), looped, &g.music, 0.0) else { return };
        let base = def.get("gain").and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
        let _ = voice.gain.gain().linear_ramp_to_value_at_time(base, g.ctx.current_time() + 0.7);
        *self.i.current.borrow_mut() = Some(Current { scene, voice });
    }

    /// 曲を止める（発走前の静けさ。ファンファーレと歓声だけにする）
    pub fn silence(&self) {
        let Some(prev) = self.i.current.borrow_mut().take() else { return };
        let Some(g) = self.graph() else { return };
        let t = g.ctx.current_time();
        let pg = prev.voice.gain.gain();
        let _ = pg.cancel_scheduled_values(t);
        let _ = pg.set_value_at_time(pg.value(), t);
        let _ = pg.linear_ramp_to_value_at_time(0.0, t + 0.5);
        prev.voice.stop_after(700);
    }

    pub fn muted(&self) -> bool { self.i.muted.get() }
    pub fn toggle_mute(&self) -> bool {
        let m = !self.i.muted.get();
        self.i.muted.set(m);
        if let Some(g) = self.graph() { g.master.gain().set_value(if m { 0.0 } else { 1.0 }); }
        // 今の版の JSON.stringify と同じ文字（並びも桁も）
        store::set(&self.i.storage_key, &format!(r#"{{"music":{},"sfx":{},"muted":{m}}}"#, self.i.music.get(), self.i.sfx.get()));
        m
    }
    pub fn duck(&self, on: bool) {
        if let Some(g) = self.graph() {
            let t = g.ctx.current_time();
            let _ = g.music.gain().set_target_at_time((self.i.music.get() * if on { 0.35 } else { 1.0 }) as f32, t, 0.15);
            let _ = g.sfx.gain().set_target_at_time((self.i.sfx.get() * if on { 0.25 } else { 1.0 }) as f32, t, 0.1);
        }
    }
    pub fn select(&self) { self.tone(880.0, 1100.0, 0.08, 0.16, O::Triangle, 0.0); }
    pub fn confirm(&self) { self.tone(660.0, 880.0, 0.12, 0.2, O::Triangle, 0.0); self.tone(990.0, 1320.0, 0.16, 0.16, O::Triangle, 0.07); }
    /// 合図の音（3・2・1 は同じ高さ、GO は高く）
    pub fn count(&self) { self.tone(660.0, 660.0, 0.14, 0.3, O::Square, 0.0); }
    pub fn go(&self) { self.tone(1320.0, 1320.0, 0.42, 0.34, O::Square, 0.0); self.sample("gate", 1.0, 1.0, 0.04, 0.03); self.noise_hit(300.0, 0.6, 0.35, 0.5, F::Lowpass); }
    pub fn jump(&self) { self.noise_hit(1500.0, 0.8, 0.14, 0.25, F::Bandpass); self.tone(320.0, 620.0, 0.12, 0.09, O::Sine, 0.0); }
    pub fn land(&self) { self.tone(170.0, 60.0, 0.13, 0.42, O::Sine, 0.0); self.noise_hit(420.0, 0.7, 0.12, 0.28, F::Bandpass); }
    pub fn clear(&self, perfect: bool) { self.tone(760.0, 1140.0, 0.1, 0.13, O::Triangle, 0.0); if perfect { self.tone(1140.0, 1520.0, 0.14, 0.13, O::Triangle, 0.07); } }
    pub fn stumble(&self) { if !self.sample("hurdle", 0.9, 1.0, 0.04, 0.03) { self.noise_hit(800.0, 1.0, 0.2, 0.5, F::Bandpass); } self.tone(190.0, 70.0, 0.28, 0.5, O::Sine, 0.0); }
    pub fn splash(&self) { if !self.sample("splash", 0.85, 1.0, 0.1, 0.03) { self.noise_hit(2200.0, 0.5, 0.35, 0.5, F::Bandpass); } }
    pub fn bump(&self, side: bool) {
        let t0 = self.now();
        if t0 - self.i.last_bump.get() < 0.22 { return; }
        self.i.last_bump.set(t0);
        self.tone(if side { 190.0 } else { 150.0 }, 90.0, 0.08, if side { 0.16 } else { 0.3 }, O::Sine, 0.0);
        self.noise_hit(600.0, 1.0, 0.06, if side { 0.08 } else { 0.16 }, F::Bandpass);
    }
    pub fn dash(&self) { if !self.sample("whoosh", 0.5, 1.0, 0.08, 0.2) { self.noise_hit(1800.0, 0.7, 0.3, 0.26, F::Bandpass); } self.tone(300.0, 480.0, 0.2, 0.07, O::Sawtooth, 0.0); }
    pub fn low(&self) { self.tone(520.0, 420.0, 0.12, 0.18, O::Triangle, 0.0); self.tone(520.0, 420.0, 0.12, 0.18, O::Triangle, 0.16); }
    pub fn empty(&self) { self.tone(300.0, 110.0, 0.42, 0.32, O::Triangle, 0.0); }
    pub fn recovered(&self) { self.tone(620.0, 830.0, 0.1, 0.1, O::Triangle, 0.0); }
    /// 抜いた: 順位が上ほど音が上がる
    pub fn pass(&self, rank: i32) { let f = 520.0 * 2f32.powf((8 - rank) as f32 / 12.0 * 2.0); self.tone(f, f * 1.5, 0.1, 0.13, O::Triangle, 0.0); self.tone(f * 1.5, f * 2.0, 0.12, 0.1, O::Triangle, 0.06); }
    pub fn passed(&self) { self.tone(300.0, 240.0, 0.1, 0.08, O::Triangle, 0.0); }
    pub fn bell(&self) { if !self.sample("bell", 0.8, 1.0, 0.04, 0.03) { for a in [0.0, 0.2, 0.4] { self.tone(1250.0, 1240.0, 0.3, 0.16, O::Sine, a); } } }
    pub fn final_straight(&self) { self.play(Scene::Final); }
    pub fn fanfare(&self) { self.sample("fanfare", 0.75, 1.0, 0.04, 0.03); }
    // マークと武器
    pub fn pick(&self) { self.tone(880.0, 1320.0, 0.07, 0.13, O::Triangle, 0.0); self.tone(1320.0, 1760.0, 0.1, 0.1, O::Triangle, 0.05); }
    pub fn poison(&self) { self.tone(330.0, 150.0, 0.3, 0.2, O::Sawtooth, 0.0); self.tone(311.0, 140.0, 0.3, 0.14, O::Square, 0.03); }
    pub fn weapon_get(&self) { for (k, n) in [523.25f32, 783.99, 1046.5].iter().enumerate() { self.tone(*n, *n, 0.12, 0.14, O::Square, k as f64 * 0.06); } self.noise_hit(3200.0, 2.0, 0.18, 0.2, F::Bandpass); }
    pub fn charge(&self) { self.tone(110.0, 440.0, 0.6, 0.12, O::Sawtooth, 0.0); }
    pub fn charge_full(&self) { self.tone(880.0, 1760.0, 0.16, 0.16, O::Square, 0.0); self.tone(1760.0, 1760.0, 0.22, 0.1, O::Triangle, 0.08); }
    pub fn swing(&self, charged: bool) { if !self.sample("whoosh", if charged { 0.9 } else { 0.55 }, if charged { 0.8 } else { 1.6 }, 0.02, 0.03) { self.noise_hit(2400.0, 0.8, 0.16, 0.3, F::Bandpass); } if charged { self.tone(220.0, 60.0, 0.35, 0.3, O::Sawtooth, 0.0); } }
    /// 当たった（me: 自分が受けた）
    pub fn hit(&self, me: bool, charged: bool) {
        self.noise_hit(if me { 500.0 } else { 1400.0 }, 0.9, 0.22, 0.55, F::Bandpass);
        self.tone(if me { 160.0 } else { 240.0 }, 50.0, if charged { 0.4 } else { 0.24 }, 0.5, O::Square, 0.0);
        if !me { self.tone(1320.0, 1980.0, 0.1, 0.12, O::Triangle, 0.05); }
    }
    pub fn dodge(&self) { self.tone(700.0, 1500.0, 0.12, 0.12, O::Sine, 0.0); self.tone(1500.0, 2100.0, 0.1, 0.09, O::Sine, 0.08); }
    pub fn slip_ready(&self) { self.tone(980.0, 1400.0, 0.1, 0.09, O::Sine, 0.0); }
    pub fn sling(&self) { self.sample("whoosh", 0.8, 1.25, 0.04, 0.03); self.tone(500.0, 1100.0, 0.18, 0.1, O::Triangle, 0.0); }
    /// 結果のタイムの数え上げの刻み（進むほど高く。k は 0..1）
    pub fn count_tick(&self, k: f32) { self.tone(660.0 + 440.0 * k, 700.0 + 460.0 * k, 0.035, 0.05, O::Sine, 0.0); }
    /// 数え終わり（自己ベストでないとき）
    pub fn count_done(&self) { self.tone(1046.5, 1046.5, 0.12, 0.1, O::Triangle, 0.0); }
    /// 自己ベスト: 上向きの短い和音。いちばん上の評価のときは 1 音多い
    pub fn cheer(&self, top: bool) {
        let notes: &[f32] = if top { &[783.99, 987.77, 1174.66, 1567.98] } else { &[783.99, 987.77, 1174.66] };
        for (k, n) in notes.iter().enumerate() { self.tone(*n, *n, 0.32, 0.14, O::Triangle, k as f64 * 0.07); }
    }
    /// いちばん上の評価のもう 1 回（高いきらめき）
    pub fn cheer_encore(&self) { for (k, n) in [1567.98f32, 2093.0, 2637.0].iter().enumerate() { self.tone(*n, *n, 0.16, 0.08, O::Sine, k as f64 * 0.05); } }
    pub fn nice(&self) { self.tone(990.0, 1320.0, 0.09, 0.11, O::Triangle, 0.0); }
    pub fn finish(&self, place: i32) {
        if place == 1 { for (i, f) in [523.0, 659.0, 784.0, 1047.0].iter().enumerate() { self.tone(*f, *f, 0.34, 0.2, O::Triangle, i as f64 * 0.11); } }
        else if place <= 3 { for (i, f) in [523.0, 659.0, 784.0].iter().enumerate() { self.tone(*f, *f, 0.26, 0.16, O::Triangle, i as f64 * 0.1); } }
        else { self.tone(400.0, 320.0, 0.3, 0.14, O::Triangle, 0.0); }
    }
    /// 歓声の大きさ（0..1）
    pub fn crowd(&self, level: f64, on: bool) {
        let crowd = self.i.crowd.borrow();
        let Some((c, r)) = crowd.as_ref() else { return };
        let t = self.now();
        let _ = c.gain().set_target_at_time(if on { (0.14 + level * 0.14) as f32 } else { 0.0 }, t, 0.3);
        let _ = r.gain().set_target_at_time(if on { ((level - 0.3).max(0.0).powf(1.6) * 0.75) as f32 } else { 0.0 }, t, 0.3);
    }
    /// 走りの音: 足音（呼吸で息切れ）・風（ダッシュ）。毎コマ呼ぶ
    pub fn run(&self, speed: f64, dash_k: f64, air: bool, stamina: f64, out: bool) {
        let Some(g) = self.graph() else { return };
        let t = g.ctx.current_time();
        let last = self.i.last_run.get();
        let dt = if last != 0.0 { (t - last).min(0.1) } else { 0.0 };
        self.i.last_run.set(t);
        let _ = g.wind_gain.gain().set_target_at_time(if speed > 2.0 { (0.015 + dash_k * 0.1) as f32 } else { 0.0 }, t, 0.09);
        let _ = g.wind_filter.frequency().set_target_at_time((700.0 + dash_k * 1500.0) as f32, t, 0.1);
        if speed > 2.0 && !air {
            // 足音: 回転に合わせる。余裕があるほど軽い
            let mut p = self.i.step_phase.get() + dt * (1.7 + speed * 0.22 + dash_k * 0.9);
            if p >= 1.0 {
                p -= 1.0;
                let heavy = 1.0 - stamina;
                self.noise_hit((520.0 - heavy * 200.0) as f32, 1.1, 0.055, (0.1 + heavy * 0.06) as f32, F::Bandpass);
                self.tone((110.0 + random() * 16.0) as f32, 62.0, 0.05, (0.13 + heavy * 0.05) as f32, O::Sine, 0.0);
            }
            self.i.step_phase.set(p);
        }
        // 呼吸: 少なくなると聞こえてくる
        if speed > 2.0 && (stamina < 0.42 || out) {
            let mut p = self.i.breath_phase.get() + dt * (if out { 3.1 } else if stamina < 0.22 { 2.5 } else { 1.9 });
            if p >= 1.0 {
                p -= 1.0;
                self.noise_hit(if out { 1500.0 } else { 1900.0 }, 1.4, 0.13, if out { 0.19 } else if stamina < 0.22 { 0.14 } else { 0.06 }, F::Bandpass);
            }
            self.i.breath_phase.set(p);
        }
    }
    /// 確かめ用: 今の曲の場面
    pub fn scene_name(&self) -> Option<&'static str> { self.i.current.borrow().as_ref().map(|c| c.scene.id()) }
}

async fn sleep(ms: i32) {
    let p = js_sys::Promise::new(&mut |ok, _| { raf::after_ms(ms, move || { let _ = ok.call0(&JsValue::NULL); }); });
    let _ = JsFuture::from(p).await;
}
async fn fetch_response(url: &str) -> Option<Response> {
    let r: Response = JsFuture::from(web_sys::window()?.fetch_with_str(url)).await.ok()?.dyn_into().ok()?;
    if r.ok() { Some(r) } else { None }
}
async fn fetch_json(url: &str) -> Option<Value> {
    let r = fetch_response(url).await?;
    let t = JsFuture::from(r.text().ok()?).await.ok()?;
    serde_json::from_str(&t.as_string()?).ok()
}
async fn fetch_bytes(url: &str) -> Option<js_sys::ArrayBuffer> {
    let r = fetch_response(url).await?;
    JsFuture::from(r.array_buffer().ok()?).await.ok()?.dyn_into().ok()
}
