// 音（Web Audio）。構成と数値の既定は pengo の audio.ts。曲の表・設定の保存はゲームが持つ（保存は store.rs）。
// つながり: 音源 → g → panner → sfx → master → （compressor）→ 出力。g → send → verb → verbGain → sfx。曲は music → master。
use super::media::{MediaSlots, MediaVoice};
use js_sys::{ArrayBuffer, Math};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{AudioBuffer, AudioContext, AudioContextState, AudioScheduledSourceNode, ConvolverNode, GainNode, Response};
pub use web_sys::{BiquadFilterType, OscillatorType};

/// 響き（pengo: 1.6 秒・減衰の指数 3.2・戻りの量 0.22）
#[derive(Clone, Copy, Debug)]
pub struct Reverb { pub seconds: f32, pub decay: f64, pub gain: f32 }

/// 既定は pengo。ゲームが値を渡せる
#[derive(Clone, Copy, Debug)]
pub struct AudioOpts {
    pub music: f32,
    pub sfx: f32,
    pub muted: bool,
    /// (threshold, ratio)。pengo は無し、ひな形 (-14, 4)、pinball・keiba (-12, 4)
    pub compressor: Option<(f32, f32)>,
    pub reverb: Option<Reverb>,
    /// 雑音の長さ（秒）
    pub noise_seconds: f32,
    /// music の gain に掛ける（pengo の `def.gain * 0.7`。ひな形は 1）
    pub music_scale: f32,
    /// jingle の gain に掛ける（pengo の `j.gain * 0.5`）
    pub jingle_scale: f32,
    /// duck(true) で曲をこの倍率へ（pengo 0.35）
    pub duck_level: f32,
    /// duck の時定数（pengo 0.2）
    pub duck_time: f64,
}
impl Default for AudioOpts {
    fn default() -> Self {
        AudioOpts {
            music: 0.55, sfx: 0.8, muted: false, compressor: None,
            reverb: Some(Reverb { seconds: 1.6, decay: 3.2, gain: 0.22 }),
            noise_seconds: 1.0, music_scale: 0.7, jingle_scale: 0.5, duck_level: 0.35, duck_time: 0.2,
        }
    }
}

/// osc の指定（TS の opts。既定 a 0.005・vibF 16）
#[derive(Clone, Copy, Debug)]
pub struct OscOpts { pub at: f64, pub a: f64, pub verb: f32, pub vib: f32, pub vib_f: f32, pub pan: f32 }
impl Default for OscOpts { fn default() -> Self { OscOpts { at: 0.0, a: 0.005, verb: 0.0, vib: 0.0, vib_f: 16.0, pan: 0.0 } } }

/// noise のフィルタ（q の既定は 1）
#[derive(Clone, Copy, Debug)]
pub struct NoiseFilter { pub ty: BiquadFilterType, pub f0: f32, pub f1: f32, pub q: f32 }
impl NoiseFilter { pub fn new(ty: BiquadFilterType, f0: f32, f1: f32) -> Self { NoiseFilter { ty, f0, f1, q: 1.0 } } }

/// noise の指定（既定 a 0.003）
#[derive(Clone, Copy, Debug)]
pub struct NoiseOpts { pub at: f64, pub a: f64, pub verb: f32, pub pan: f32 }
impl Default for NoiseOpts { fn default() -> Self { NoiseOpts { at: 0.0, a: 0.003, verb: 0.0, pan: 0.0 } } }

/// sample_limited の指定（ひな形の sample。同じ音の間隔 gap・高さの揺れ jitter・同時 24 音まで）
#[derive(Clone, Copy, Debug)]
pub struct LimitedOpts { pub vol: f32, pub rate: f32, pub jitter: f32, pub gap: f64, pub at: f64 }
impl Default for LimitedOpts { fn default() -> Self { LimitedOpts { vol: 1.0, rate: 1.0, jitter: 0.06, gap: 0.03, at: 0.0 } } }

struct Graph { ctx: AudioContext, master: GainNode, sfx: GainNode, music: GainNode, verb: Option<ConvolverNode> }

struct Current { url: String, voice: MediaVoice }

struct Inner {
    opts: Cell<AudioOpts>,
    graph: RefCell<Option<Rc<Graph>>>,
    samples: RefCell<HashMap<String, AudioBuffer>>,
    /// init の前に頼まれた音（init で読む）
    pending: RefCell<Vec<(String, String)>>,
    noise_buf: RefCell<Option<AudioBuffer>>,
    current: RefCell<Option<Current>>,
    /// init の前に頼まれた曲（init で鳴らす。url・gain・入り・終わり・ループ）
    want_music: RefCell<Option<(String, f32, f64, f64, bool)>>,
    /// 曲とジングルの <audio>（iPhone・iPad で最初の操作のあとも鳴らせるよう使い回す。media.rs）
    media: MediaSlots,
    last_at: RefCell<HashMap<String, f64>>,
    voices: Rc<Cell<i32>>,
}

/// 複製しても同じ物を指す
#[derive(Clone)]
pub struct AudioKit { i: Rc<Inner> }

fn env(g: &GainNode, t: f64, a: f64, peak: f32, d: f64) -> Result<(), JsValue> {
    g.gain().set_value_at_time(0.0001, t)?;
    g.gain().exponential_ramp_to_value_at_time(peak, t + a)?;
    g.gain().exponential_ramp_to_value_at_time(0.0001, t + a + d)?;
    Ok(())
}

fn build_graph(o: &AudioOpts) -> Result<Graph, JsValue> {
    let ctx = AudioContext::new()?;
    let master = ctx.create_gain()?;
    master.gain().set_value(if o.muted { 0.0 } else { 1.0 });
    if let Some((threshold, ratio)) = o.compressor {
        let comp = ctx.create_dynamics_compressor()?;
        comp.threshold().set_value(threshold);
        comp.ratio().set_value(ratio);
        master.connect_with_audio_node(&comp)?;
        comp.connect_with_audio_node(&ctx.destination())?;
    } else {
        master.connect_with_audio_node(&ctx.destination())?;
    }
    let sfx = ctx.create_gain()?;
    sfx.gain().set_value(o.sfx);
    sfx.connect_with_audio_node(&master)?;
    let music = ctx.create_gain()?;
    music.gain().set_value(o.music);
    music.connect_with_audio_node(&master)?;
    let verb = match o.reverb {
        Some(r) => {
            let verb = ctx.create_convolver()?;
            let rate = ctx.sample_rate();
            let len = (rate * r.seconds).floor() as u32;
            let ir = ctx.create_buffer(2, len, rate)?;
            for ch in 0..2 {
                let mut d: Vec<f32> = (0..len).map(|i| ((Math::random() * 2.0 - 1.0) * (1.0 - i as f64 / len as f64).powf(r.decay)) as f32).collect();
                ir.copy_to_channel(&mut d, ch)?;
            }
            verb.set_buffer(Some(&ir));
            let vg = ctx.create_gain()?;
            vg.gain().set_value(r.gain);
            verb.connect_with_audio_node(&vg)?;
            vg.connect_with_audio_node(&sfx)?;
            Some(verb)
        }
        None => None,
    };
    Ok(Graph { ctx, master, sfx, music, verb })
}

async fn fetch_decode(ctx: AudioContext, url: String) -> Result<AudioBuffer, JsValue> {
    let win = web_sys::window().ok_or("window")?;
    let res: Response = JsFuture::from(win.fetch_with_str(&url)).await?.dyn_into()?;
    if !res.ok() { return Err(JsValue::from_str(&url)); }
    let buf: ArrayBuffer = JsFuture::from(res.array_buffer()?).await?.dyn_into()?;
    JsFuture::from(ctx.decode_audio_data(&buf)?).await?.dyn_into()
}

impl AudioKit {
    pub fn new(opts: AudioOpts) -> AudioKit {
        AudioKit { i: Rc::new(Inner {
            opts: Cell::new(opts), graph: RefCell::new(None), samples: RefCell::default(), pending: RefCell::default(),
            noise_buf: RefCell::new(None), current: RefCell::new(None), last_at: RefCell::default(), voices: Rc::default(),
            want_music: RefCell::new(None), media: MediaSlots::new(6),
        }) }
    }

    fn graph(&self) -> Option<Rc<Graph>> { self.i.graph.borrow().clone() }

    /// 最初の操作のときに呼ぶ（ブラウザは操作の前に音を出させない）。2 回目からは止まっていれば起こすだけ
    pub fn init(&self) {
        if let Some(g) = self.graph() {
            // iOS は suspended のほかに interrupted でも止まる
            let st = g.ctx.state();
            if st != AudioContextState::Running && st != AudioContextState::Closed { let _ = g.ctx.resume(); }
            return;
        }
        let Ok(g) = build_graph(&self.i.opts.get()) else { return };
        self.i.media.set_ctx(&g.ctx);
        *self.i.graph.borrow_mut() = Some(Rc::new(g));
        let pending: Vec<_> = self.i.pending.borrow_mut().drain(..).collect();
        for (name, url) in pending { self.load_sample(&name, &url); }
        let want = self.i.want_music.borrow_mut().take();
        if let Some((url, gain, fade_in, fade_out, looped)) = want { self.music_with(&url, gain, fade_in, fade_out, looped); }
    }

    pub fn ready(&self) -> bool { self.i.graph.borrow().is_some() }
    /// AudioContext の時刻
    pub fn now(&self) -> f64 { self.graph().map_or(0.0, |g| g.ctx.current_time()) }
    /// ゲーム固有の音（風・歓声のループなど）を作るときの材料。init の前は None
    pub fn ctx(&self) -> Option<AudioContext> { Some(self.graph()?.ctx.clone()) }
    pub fn sfx_bus(&self) -> Option<GainNode> { Some(self.graph()?.sfx.clone()) }
    pub fn music_bus(&self) -> Option<GainNode> { Some(self.graph()?.music.clone()) }
    pub fn master_bus(&self) -> Option<GainNode> { Some(self.graph()?.master.clone()) }
    pub fn reverb(&self) -> Option<ConvolverNode> { self.graph()?.verb.clone() }

    /// 効果音を読む。init の前なら init のときに読む。読めなければ鳴らないだけ
    pub fn load_sample(&self, name: &str, url: &str) {
        let Some(g) = self.graph() else {
            self.i.pending.borrow_mut().push((name.to_string(), url.to_string()));
            return;
        };
        let (me, name, url, ctx) = (self.i.clone(), name.to_string(), url.to_string(), g.ctx.clone());
        spawn_local(async move {
            if let Ok(buf) = fetch_decode(ctx, url).await { me.samples.borrow_mut().insert(name, buf); }
        });
    }

    /// 発振（TS の osc。f0 → f1 へ指数で動かす）
    pub fn osc(&self, ty: OscillatorType, f0: f32, f1: f32, dur: f64, vol: f32, o: OscOpts) {
        if let Some(g) = self.graph() { let _ = Self::play_osc(&g, ty, f0, f1, dur, vol, o); }
    }
    fn play_osc(g: &Graph, ty: OscillatorType, f0: f32, f1: f32, dur: f64, vol: f32, o: OscOpts) -> Result<(), JsValue> {
        let ctx = &g.ctx;
        let t = o.at + ctx.current_time();
        let s = ctx.create_oscillator()?;
        s.set_type(ty);
        s.frequency().set_value_at_time(f0, t)?;
        s.frequency().exponential_ramp_to_value_at_time(f1.max(20.0), t + dur)?;
        let gain = ctx.create_gain()?;
        env(&gain, t, o.a, vol, dur)?;
        if o.vib != 0.0 {
            let l = ctx.create_oscillator()?;
            l.frequency().set_value(o.vib_f);
            let lg = ctx.create_gain()?;
            lg.gain().set_value(o.vib);
            l.connect_with_audio_node(&lg)?;
            lg.connect_with_audio_param(&s.frequency())?;
            l.start_with_when(t)?;
            l.stop_with_when(t + dur + 0.1)?;
        }
        let p = ctx.create_stereo_panner()?;
        p.pan().set_value(o.pan);
        s.connect_with_audio_node(&gain)?;
        gain.connect_with_audio_node(&p)?;
        p.connect_with_audio_node(&g.sfx)?;
        Self::send(g, &gain, o.verb)?;
        s.start_with_when(t)?;
        s.stop_with_when(t + o.a + dur + 0.05)?;
        Ok(())
    }
    fn send(g: &Graph, from: &GainNode, amount: f32) -> Result<(), JsValue> {
        if amount != 0.0 && let Some(verb) = &g.verb {
            let vg = g.ctx.create_gain()?;
            vg.gain().set_value(amount);
            from.connect_with_audio_node(&vg)?;
            vg.connect_with_audio_node(verb)?;
        }
        Ok(())
    }

    /// 雑音（TS の noise）
    pub fn noise(&self, dur: f64, vol: f32, filter: NoiseFilter, o: NoiseOpts) {
        if let Some(g) = self.graph() { let _ = self.play_noise(&g, dur, vol, filter, o); }
    }
    fn noise_buffer(&self, g: &Graph) -> Result<AudioBuffer, JsValue> {
        if let Some(b) = self.i.noise_buf.borrow().as_ref() { return Ok(b.clone()); }
        let rate = g.ctx.sample_rate();
        let len = (rate * self.i.opts.get().noise_seconds) as u32;
        let b = g.ctx.create_buffer(1, len, rate)?;
        let mut d: Vec<f32> = (0..len).map(|_| (Math::random() * 2.0 - 1.0) as f32).collect();
        b.copy_to_channel(&mut d, 0)?;
        *self.i.noise_buf.borrow_mut() = Some(b.clone());
        Ok(b)
    }
    fn play_noise(&self, g: &Graph, dur: f64, vol: f32, filter: NoiseFilter, o: NoiseOpts) -> Result<(), JsValue> {
        let ctx = &g.ctx;
        let buf = self.noise_buffer(g)?;
        let t = o.at + ctx.current_time();
        let s = ctx.create_buffer_source()?;
        s.set_buffer(Some(&buf));
        let f = ctx.create_biquad_filter()?;
        f.set_type(filter.ty);
        f.q().set_value(filter.q);
        f.frequency().set_value_at_time(filter.f0, t)?;
        f.frequency().exponential_ramp_to_value_at_time(filter.f1, t + dur)?;
        let gain = ctx.create_gain()?;
        env(&gain, t, o.a, vol, dur)?;
        let p = ctx.create_stereo_panner()?;
        p.pan().set_value(o.pan);
        s.connect_with_audio_node(&f)?;
        f.connect_with_audio_node(&gain)?;
        gain.connect_with_audio_node(&p)?;
        p.connect_with_audio_node(&g.sfx)?;
        Self::send(g, &gain, o.verb)?;
        s.start_with_when_and_grain_offset(t, Math::random() * 0.5)?;
        AudioScheduledSourceNode::stop_with_when(&s, t + dur + 0.05)?;
        Ok(())
    }

    /// 読んだ音を鳴らす。読めていなければ false（TS と同じ。合成の音へ逃がす判断に使う）
    pub fn sample(&self, name: &str, vol: f32, rate: f32, at: f64, pan: f32, wet: f32) -> bool {
        let Some(g) = self.graph() else { return false };
        let Some(buf) = self.i.samples.borrow().get(name).cloned() else { return false };
        let _ = Self::play_sample(&g, &buf, vol, rate, at, pan, wet);
        true
    }
    fn play_sample(g: &Graph, buf: &AudioBuffer, vol: f32, rate: f32, at: f64, pan: f32, wet: f32) -> Result<(), JsValue> {
        let ctx = &g.ctx;
        let s = ctx.create_buffer_source()?;
        s.set_buffer(Some(buf));
        s.playback_rate().set_value(rate);
        let gain = ctx.create_gain()?;
        gain.gain().set_value(vol);
        let p = ctx.create_stereo_panner()?;
        p.pan().set_value(pan);
        s.connect_with_audio_node(&gain)?;
        gain.connect_with_audio_node(&p)?;
        p.connect_with_audio_node(&g.sfx)?;
        Self::send(g, &gain, wet)?;
        s.start_with_when(ctx.current_time() + at)?;
        Ok(())
    }

    /// 同じ音が重なりすぎないように絞って鳴らす（ひな形の sample）。読めていなければ false
    pub fn sample_limited(&self, name: &str, o: LimitedOpts) -> bool {
        let Some(g) = self.graph() else { return false };
        let Some(buf) = self.i.samples.borrow().get(name).cloned() else { return false };
        let t = g.ctx.current_time();
        if self.i.last_at.borrow().get(name).copied().unwrap_or(-1.0) > t - o.gap { return true; }
        if self.i.voices.get() > 24 { return true; }
        self.i.last_at.borrow_mut().insert(name.to_string(), t);
        let _ = self.play_limited(&g, &buf, o, t);
        true
    }
    fn play_limited(&self, g: &Graph, buf: &AudioBuffer, o: LimitedOpts, t: f64) -> Result<(), JsValue> {
        let s = g.ctx.create_buffer_source()?;
        s.set_buffer(Some(buf));
        s.playback_rate().set_value(o.rate * (1.0 + (Math::random() as f32 - 0.5) * 2.0 * o.jitter));
        let gain = g.ctx.create_gain()?;
        gain.gain().set_value(o.vol);
        s.connect_with_audio_node(&gain)?;
        gain.connect_with_audio_node(&g.sfx)?;
        let voices = self.i.voices.clone();
        let ended = Closure::once_into_js(move || voices.set(voices.get() - 1));
        AudioScheduledSourceNode::set_onended(&s, Some(ended.unchecked_ref()));
        s.start_with_when(t + o.at)?;
        self.i.voices.set(self.i.voices.get() + 1);
        Ok(())
    }

    /// 鐘（TS の bell。基音・2.01 倍・3 倍の三角）
    pub fn bell(&self, f: f32, vol: f32, at: f64, dur: f64, pan: f32) {
        self.osc(OscillatorType::Sine, f, f, dur, vol, OscOpts { at, a: 0.004, verb: 0.6, pan, ..Default::default() });
        self.osc(OscillatorType::Sine, f * 2.01, f * 2.01, dur * 0.5, vol * 0.35, OscOpts { at, a: 0.004, verb: 0.4, pan, ..Default::default() });
        self.osc(OscillatorType::Triangle, f * 3.0, f * 3.0, dur * 0.25, vol * 0.12, OscOpts { at, pan, ..Default::default() });
    }

    /// 曲を流す（`<audio>` を MediaElementSource で。ループ）。同じ曲なら何もしない。前の曲は fade 秒で消える
    pub fn music(&self, url: &str, gain: f32, fade: f64) { self.music_with(url, gain, fade, fade, true); }
    /// 入りと終わりの長さ・ループの有無を分けて渡す
    pub fn music_with(&self, url: &str, gain: f32, fade_in: f64, fade_out: f64, looped: bool) {
        let Some(g) = self.graph() else { *self.i.want_music.borrow_mut() = Some((url.to_string(), gain, fade_in, fade_out, looped)); return };
        if self.i.current.borrow().as_ref().is_some_and(|c| c.url == url) { return; }
        self.music_stop(fade_out);
        let _ = self.start_music(&g, url, gain, fade_in, looped);
    }
    fn start_music(&self, g: &Graph, url: &str, gain: f32, fade: f64, looped: bool) -> Result<(), JsValue> {
        let voice = self.i.media.start(&g.ctx, url, looped, &g.music, 0.0)?;
        let t = g.ctx.current_time();
        voice.gain.gain().linear_ramp_to_value_at_time(gain * self.i.opts.get().music_scale, t + fade)?;
        *self.i.current.borrow_mut() = Some(Current { url: url.to_string(), voice });
        Ok(())
    }
    /// 曲を fade 秒で止める
    pub fn music_stop(&self, fade: f64) {
        let Some(g) = self.graph() else { self.i.want_music.borrow_mut().take(); return };
        let Some(old) = self.i.current.borrow_mut().take() else { return };
        let t = g.ctx.current_time();
        let p = old.voice.gain.gain();
        let _ = p.cancel_scheduled_values(t);
        let _ = p.set_value_at_time(p.value(), t);
        let _ = p.linear_ramp_to_value_at_time(0.0, t + fade);
        old.voice.stop_after((fade * 1000.0) as i32 + 100);
    }
    /// 今の曲の URL
    pub fn music_url(&self) -> Option<String> { self.i.current.borrow().as_ref().map(|c| c.url.clone()) }

    /// 短い曲（sfx へ流す。max 秒で切る。終わりの 0.6 秒で消す）
    pub fn jingle(&self, url: &str, gain: f32, max: f64) {
        if let Some(g) = self.graph() { let _ = self.play_jingle(&g, url, gain, max); }
    }
    fn play_jingle(&self, g: &Graph, url: &str, gain: f32, max: f64) -> Result<(), JsValue> {
        let v = gain * self.i.opts.get().jingle_scale;
        let voice = self.i.media.start(&g.ctx, url, false, &g.sfx, v)?;
        let gn = &voice.gain;
        let t = g.ctx.current_time();
        gn.gain().set_value_at_time(v, t + max - 0.6)?;
        gn.gain().linear_ramp_to_value_at_time(0.0, t + max)?;
        voice.stop_after((max * 1000.0) as i32 + 100);
        Ok(())
    }

    pub fn muted(&self) -> bool { self.i.opts.get().muted }
    /// 保存はゲームが store.rs でする
    pub fn set_muted(&self, m: bool) {
        let mut o = self.i.opts.get();
        o.muted = m;
        self.i.opts.set(o);
        if let Some(g) = self.graph() { g.master.gain().set_value(if m { 0.0 } else { 1.0 }); }
    }
    pub fn music_volume(&self) -> f32 { self.i.opts.get().music }
    pub fn sfx_volume(&self) -> f32 { self.i.opts.get().sfx }
    pub fn set_music_volume(&self, v: f32) {
        let mut o = self.i.opts.get();
        o.music = v;
        self.i.opts.set(o);
        if let Some(g) = self.graph() { g.music.gain().set_value(v); }
    }
    pub fn set_sfx_volume(&self, v: f32) {
        let mut o = self.i.opts.get();
        o.sfx = v;
        self.i.opts.set(o);
        if let Some(g) = self.graph() { g.sfx.gain().set_value(v); }
    }
    /// 一時停止のあいだは曲を小さく
    pub fn duck(&self, on: bool) {
        let Some(g) = self.graph() else { return };
        let o = self.i.opts.get();
        let _ = g.music.gain().set_target_at_time(if on { o.music * o.duck_level } else { o.music }, g.ctx.current_time(), o.duck_time);
    }
}
