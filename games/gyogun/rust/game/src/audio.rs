// 音: BGM はこのゲームのために Stable Audio 3 で作った 4 曲（public/music/。タイトル・戦闘・大将・勝利）、効果音は aisns の Kenney（CC0）と、このゲームのために Stable Audio 3 で作った音（public/sfx/README.md）、
// それに合成の低音（太鼓・どん）を重ねる。同じ音が重なりすぎないよう、同時に鳴る数を絞る。
//
// Web Audio のグラフ（master → compressor → 出力、sfx・music → master）は 1 本の rodio の Source（Engine）の中で計算する。
// 呼び出し側（ゲームの 1 コマ）はチャンネルへ指示を送るだけで、デコードは別スレッド、PCM の計算は音のスレッドで行う。
// デバイスが無ければ何も鳴らさず、呼び出しはそのまま何もしないで返る。
use crate::assets::{self, BASE};
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Source};
use std::f64::consts::PI;
use std::io::Cursor;
use std::num::NonZero;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI32, AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

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
    fn index(self) -> usize {
        self as usize
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
const N_SAMPLES: usize = WILDS.len() + OWN.len();

fn sample_index(name: &str) -> Option<usize> {
    WILDS.iter().chain(OWN.iter()).position(|n| *n == name)
}
fn sample_path(i: usize) -> String {
    if i < WILDS.len() { format!("{BASE}sounds/wilds/{}.mp3", WILDS[i]) } else { format!("sfx/{}.wav", OWN[i - WILDS.len()]) }
}

/// 和音階（ド・レ・ミ・ソ・ラ）の半音の数
const PENTA: [i32; 5] = [0, 2, 4, 7, 9];
/// 自己ベストの祝いの和音（ソ・シ・レ・高いソ。上向きに 1 音ずつ）
const CHEER_NOTES: [f32; 4] = [783.99, 987.77, 1174.66, 1567.98];

// ---- 保存する設定（TS の localStorage 'gyogun.audio'） ----

#[derive(Clone, Copy)]
struct Settings {
    music: f32,
    sfx: f32,
    muted: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self { music: 0.5, sfx: 0.85, muted: false }
    }
}
fn settings_path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("GYOGUN_DATA") {
        return Some(PathBuf::from(p).join("gyogun.audio.json"));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let home = PathBuf::from(home);
    let dir = if cfg!(target_os = "macos") {
        home.join("Library/Application Support/gyogun")
    } else if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or(home).join("gyogun")
    } else {
        home.join(".local/share/gyogun")
    };
    Some(dir.join("gyogun.audio.json"))
}
fn load_settings() -> Settings {
    let mut s = Settings::default();
    // 保存できなくても鳴らす
    let Some(v) = settings_path().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok()) else { return s };
    if let Some(x) = v.get("music").and_then(|x| x.as_f64()) { s.music = x as f32; }
    if let Some(x) = v.get("sfx").and_then(|x| x.as_f64()) { s.sfx = x as f32; }
    if let Some(x) = v.get("muted").and_then(|x| x.as_bool()) { s.muted = x; }
    s
}
fn save_settings(s: Settings) {
    let Some(p) = settings_path() else { return };
    if let Some(d) = p.parent() { let _ = std::fs::create_dir_all(d); }
    let _ = std::fs::write(p, serde_json::json!({ "music": s.music, "sfx": s.sfx, "muted": s.muted }).to_string());
}

// ---- 素材 ----

/// 読み込んだ効果音（AudioBuffer の役）。interleaved
struct Pcm {
    data: Vec<f32>,
    channels: usize,
    rate: f32,
    frames: usize,
}

fn decode_sample(rel: &str) -> Result<Pcm, String> {
    let bytes = assets::read(rel)?;
    let dec = rodio::Decoder::new(Cursor::new(bytes)).map_err(|e| format!("{rel}: {e}"))?;
    let channels = dec.channels().get() as usize;
    let rate = dec.sample_rate().get() as f32;
    let data: Vec<f32> = dec.collect();
    let frames = data.len() / channels;
    Ok(Pcm { data, channels, rate, frames })
}

/// BGM の中身。曲は長いので PCM に開かず、Opus のパケットのまま持つ（HTMLAudioElement と同じく鳴らしながらデコードする）
struct OpusFile {
    data: Vec<u8>,
    packets: Vec<(u32, u32)>,
    channels: usize,
    /// OpusHead の pre-skip（先頭で捨てるフレーム数）
    pre_skip: usize,
    /// 再生する総フレーム数（末尾の詰め物を除く）
    frames: u64,
    /// OpusHead の出力ゲイン
    gain: f32,
}

fn load_opus(rel: &str) -> Result<OpusFile, String> {
    use symphonia::core::formats::{FormatOptions, FormatReader};
    use symphonia::core::io::MediaSourceStream;
    let bytes = assets::read(rel)?;
    let cap = bytes.len();
    let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes)), Default::default());
    let mut r = symphonia::default::formats::IsoMp4Reader::try_new(mss, &FormatOptions::default()).map_err(|e| format!("{rel}: {e}"))?;
    let track = r.default_track().ok_or_else(|| format!("{rel}: 音声トラックがない"))?;
    if track.codec_params.codec != symphonia::core::codecs::CODEC_TYPE_OPUS {
        return Err(format!("{rel}: Opus ではない"));
    }
    let tid = track.id;
    // OpusHead: magic(8) version(1) channels(1) pre-skip(2) rate(4) gain(2)。MP4 の dOps は big-endian
    let head = track.codec_params.extra_data.as_deref().ok_or_else(|| format!("{rel}: OpusHead がない"))?;
    if head.len() < 19 {
        return Err(format!("{rel}: OpusHead が短い"));
    }
    let channels = head[9] as usize;
    let pre_skip = u16::from_be_bytes([head[10], head[11]]) as usize;
    let gain_q8 = i16::from_be_bytes([head[16], head[17]]);
    let gain = 10f32.powf(gain_q8 as f32 / 256.0 / 20.0);
    let mut data = Vec::with_capacity(cap);
    let mut packets = Vec::new();
    let mut total: u64 = 0;
    while let Ok(p) = r.next_packet() {
        if p.track_id() != tid {
            continue;
        }
        let at = data.len() as u32;
        data.extend_from_slice(p.buf());
        packets.push((at, p.buf().len() as u32));
        total += p.dur;
    }
    let frames = total.saturating_sub(pre_skip as u64);
    Ok(OpusFile { data, packets, channels, pre_skip, frames, gain })
}

/// 鳴っている曲のデコーダ（1 曲につき 1 つ。48 kHz で開いて、線形補間で出力のレートへ）
struct TrackStream {
    file: Arc<OpusFile>,
    dec: rusty_opus::OpusDecoder,
    pkt: usize,
    pcm: Vec<f32>,
    pos: usize,
    len: usize,
    skip: usize,
    left: u64,
    looped: bool,
    prev: [f32; 2],
    next: [f32; 2],
    frac: f64,
    step: f64,
    ended: bool,
}
impl TrackStream {
    fn new(file: Arc<OpusFile>, looped: bool) -> Result<Self, String> {
        let dec = rusty_opus::OpusDecoder::new(48_000, file.channels).map_err(|e| format!("{e:?}"))?;
        let pcm = vec![0.0; 5760 * file.channels];
        let (skip, left) = (file.pre_skip, file.frames);
        Ok(Self { file, dec, pkt: 0, pcm, pos: 0, len: 0, skip, left, looped, prev: [0.0; 2], next: [0.0; 2], frac: 1.0, step: 1.0, ended: false })
    }
    /// 次の 1 フレーム（48 kHz）。曲の終わりはループするか、無音
    fn pull(&mut self) -> [f32; 2] {
        loop {
            if self.left == 0 {
                if !self.looped {
                    self.ended = true;
                    return [0.0; 2];
                }
                // デコーダはそのまま使う（先頭の pre-skip を捨てるので前の余韻は聞こえない。音のスレッドで作り直さない）
                self.pkt = 0;
                self.pos = 0;
                self.len = 0;
                self.skip = self.file.pre_skip;
                self.left = self.file.frames;
                if self.left == 0 {
                    self.ended = true;
                    return [0.0; 2];
                }
            }
            if self.pos < self.len {
                let ch = self.file.channels;
                let g = self.file.gain;
                let l = self.pcm[self.pos * ch] * g;
                let r = if ch > 1 { self.pcm[self.pos * ch + 1] * g } else { l };
                self.pos += 1;
                self.left -= 1;
                return [l, r];
            }
            if self.pkt >= self.file.packets.len() {
                self.left = 0;
                continue;
            }
            let (at, n) = self.file.packets[self.pkt];
            self.pkt += 1;
            let file = self.file.clone();
            let pkt = &file.data[at as usize..(at + n) as usize];
            // 壊れたパケットは飛ばす
            let Ok(n) = self.dec.decode(pkt, 5760, &mut self.pcm) else { continue };
            let drop = self.skip.min(n);
            self.skip -= drop;
            self.pos = drop;
            self.len = n;
        }
    }
    fn tick(&mut self) -> [f32; 2] {
        while self.frac >= 1.0 {
            self.prev = self.next;
            self.next = self.pull();
            self.frac -= 1.0;
        }
        let f = self.frac as f32;
        let out = [self.prev[0] + (self.next[0] - self.prev[0]) * f, self.prev[1] + (self.next[1] - self.prev[1]) * f];
        self.frac += self.step;
        out
    }
}

// ---- 指示（ゲーム → 音のスレッド） ----

enum Cmd {
    Sample { pcm: Arc<Pcm>, rate: f32, gain: f32, at: f32 },
    Thump { f0: f32, f1: f32, dur: f32, vol: f32, noise: f32 },
    Pluck { freq: f32, vol: f32, bright: bool, at: f32 },
    Rise { dur: f32, vol: f32 },
    StartTrack { id: u64, stream: Box<TrackStream>, gain: f32 },
    /// playTrack の前の曲: 今の値から 1.2 秒で 0 へ、1.4 秒で止める
    CrossFadeOut { id: u64 },
    /// stopMusic: 0.8 秒あとに 0 になるランプを足す（予定は消さない）、0.9 秒で止める
    StopTrack { id: u64 },
    Duck { amount: f32, dur: f32, music: f32 },
    Master(f32),
}

// ---- AudioParam（曲の音量とダッキング） ----

/// 折れ線の予定を持つ AudioParam。TS と同じく linearRampToValueAtTime は「直前の予定の時刻」から線を引く
struct Param {
    pts: Vec<(f64, f32)>,
}
impl Param {
    fn new(t: f64, v: f32) -> Self {
        let mut pts = Vec::with_capacity(8);
        pts.push((t, v));
        Self { pts }
    }
    fn at(&mut self, t: f64) -> f32 {
        while self.pts.len() >= 2 && self.pts[1].0 <= t {
            self.pts.remove(0);
        }
        let (t0, v0) = self.pts[0];
        match self.pts.get(1) {
            Some(&(t1, v1)) if t >= t0 => v0 + (v1 - v0) * ((t - t0) / (t1 - t0)) as f32,
            _ => v0,
        }
    }
    fn cancel(&mut self, t: f64) {
        let keep = self.pts.iter().position(|p| p.0 >= t).unwrap_or(self.pts.len());
        // 予定が 1 つも残らないときは最初の値を残す
        self.pts.truncate(keep.max(1));
    }
    fn set_value_at(&mut self, v: f32, t: f64) {
        self.insert(t, v);
    }
    fn linear_ramp(&mut self, v: f32, t: f64) {
        self.insert(t, v);
    }
    fn insert(&mut self, t: f64, v: f32) {
        let i = self.pts.iter().position(|p| p.0 > t).unwrap_or(self.pts.len());
        self.pts.insert(i, (t, v));
    }
}

// ---- 合成の部品 ----

/// 小さな xorshift（Math.random の代わり）
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        ((x >> 40) as f32) / (1u64 << 24) as f32
    }
}

/// setValueAtTime(0.0001) → exponentialRamp(peak, up) → exponentialRamp(0.0001, down) を 1 サンプルずつ
struct Env {
    v: f64,
    up_mul: f64,
    up_n: u32,
    down_mul: f64,
    down_n: u32,
}
const ENV_FLOOR: f64 = 0.0001;
impl Env {
    fn new(peak: f32, t_up: f32, t_end: f32, sr: f32) -> Self {
        let peak = (peak as f64).max(ENV_FLOOR);
        let up_n = ((t_up * sr) as u32).max(1);
        let down_n = (((t_end - t_up) * sr) as u32).max(1);
        Self { v: ENV_FLOOR, up_mul: (peak / ENV_FLOOR).powf(1.0 / up_n as f64), up_n, down_mul: (ENV_FLOOR / peak).powf(1.0 / down_n as f64), down_n }
    }
    #[inline]
    fn step(&mut self) -> f32 {
        let v = self.v as f32;
        if self.up_n > 0 {
            self.v *= self.up_mul;
            self.up_n -= 1;
        } else if self.down_n > 0 {
            self.v *= self.down_mul;
            self.down_n -= 1;
        }
        v
    }
}

/// exponentialRampToValueAtTime の周波数。終わったら終点のまま
struct ExpRamp {
    v: f64,
    mul: f64,
    n: u32,
}
impl ExpRamp {
    fn new(from: f32, to: f32, dur: f32, sr: f32) -> Self {
        let n = ((dur * sr) as u32).max(1);
        Self { v: from as f64, mul: (to as f64 / from as f64).powf(1.0 / n as f64), n }
    }
    fn fixed(v: f32) -> Self {
        Self { v: v as f64, mul: 1.0, n: 0 }
    }
    #[inline]
    fn step(&mut self) -> f64 {
        let v = self.v;
        if self.n > 0 {
            self.v *= self.mul;
            self.n -= 1;
        }
        v
    }
}

/// BiquadFilter の lowpass（Web Audio の式。Q は既定の 1 dB）
#[derive(Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}
impl Biquad {
    fn lowpass(freq: f32, sr: f32) -> Self {
        let mut b = Self::default();
        b.set_lowpass(freq, sr);
        b
    }
    fn set_lowpass(&mut self, freq: f32, sr: f32) {
        let w0 = 2.0 * PI as f32 * (freq / sr).min(0.5);
        let (s, c) = w0.sin_cos();
        let alpha = s / (2.0 * 10f32.powf(1.0 / 20.0));
        let a0 = 1.0 + alpha;
        self.b0 = (1.0 - c) / 2.0 / a0;
        self.b1 = (1.0 - c) / a0;
        self.b2 = self.b0;
        self.a1 = -2.0 * c / a0;
        self.a2 = (1.0 - alpha) / a0;
    }
    #[inline]
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

#[derive(Clone, Copy)]
enum Wave {
    Sine,
    Triangle,
    Saw,
}
/// Web Audio の発振器の形（位相 0 で 0 から上がる）。のこぎり波は帯域制限のかわりに polyBLEP
#[inline]
fn wave(w: Wave, p: f64, dt: f64) -> f32 {
    match w {
        Wave::Sine => (2.0 * PI * p).sin() as f32,
        Wave::Triangle => (if p < 0.25 { 4.0 * p } else if p < 0.75 { 2.0 - 4.0 * p } else { 4.0 * p - 4.0 }) as f32,
        Wave::Saw => {
            let t = (p + 0.5).fract();
            let blep = if t < dt {
                let x = t / dt;
                2.0 * x - x * x - 1.0
            } else if t > 1.0 - dt {
                let x = (t - 1.0) / dt;
                x * x + 2.0 * x + 1.0
            } else {
                0.0
            };
            // Chrome の sawtooth は倍音を足した波の行き過ぎ（ギブズ）ごと 1 に正規化されるので、実効の振幅が小さい（Chrome の出力と比べた値）
            ((2.0 * t - 1.0 - blep) * 0.8515) as f32
        }
    }
}

const BLOCK: usize = 128;

/// 鳴っている 1 音
enum Voice {
    Sample { pcm: Arc<Pcm>, pos: f64, step: f64, gain: f32, delay: u32 },
    Osc { wave: Wave, phase: f64, freq: ExpRamp, env: Env, lp: Option<(Biquad, ExpRamp)>, left: u32, delay: u32 },
    Noise { i: u32, len: u32, lp: Biquad, gain: f32, seed: Rng },
}
impl Voice {
    /// BLOCK フレームをバスへ足す。終わったら false
    fn mix(&mut self, bus: &mut [f32; BLOCK * 2], sr: f32) -> bool {
        match self {
            Voice::Sample { pcm, pos, step, gain, delay } => {
                let ch = pcm.channels;
                for f in 0..BLOCK {
                    if *delay > 0 {
                        *delay -= 1;
                        continue;
                    }
                    let i = *pos as usize;
                    if i >= pcm.frames {
                        return false;
                    }
                    let fr = (*pos - i as f64) as f32;
                    let at = |c: usize, j: usize| if j < pcm.frames { pcm.data[j * ch + c] } else { 0.0 };
                    let l = at(0, i) + (at(0, i + 1) - at(0, i)) * fr;
                    let r = if ch > 1 { at(1, i) + (at(1, i + 1) - at(1, i)) * fr } else { l };
                    bus[f * 2] += l * *gain;
                    bus[f * 2 + 1] += r * *gain;
                    *pos += *step;
                }
                true
            }
            Voice::Osc { wave: w, phase, freq, env, lp, left, delay } => {
                for f in 0..BLOCK {
                    if *delay > 0 {
                        *delay -= 1;
                        continue;
                    }
                    if *left == 0 {
                        return false;
                    }
                    *left -= 1;
                    let dt = freq.step() / sr as f64;
                    let mut s = wave(*w, *phase, dt);
                    *phase = (*phase + dt).fract();
                    if let Some((bq, cut)) = lp {
                        bq.set_lowpass(cut.step() as f32, sr);
                        s = bq.run(s);
                    }
                    let out = s * env.step();
                    bus[f * 2] += out;
                    bus[f * 2 + 1] += out;
                }
                true
            }
            Voice::Noise { i, len, lp, gain, seed } => {
                for f in 0..BLOCK {
                    if *i >= *len {
                        return false;
                    }
                    let k = 1.0 - *i as f32 / *len as f32;
                    let n = (seed.next() * 2.0 - 1.0) * k * k * k * k;
                    *i += 1;
                    let out = lp.run(n) * *gain;
                    bus[f * 2] += out;
                    bus[f * 2 + 1] += out;
                }
                true
            }
        }
    }
}

struct TrackVoice {
    id: u64,
    stream: Box<TrackStream>,
    gain: Param,
    stop_at: Option<f64>,
}

// ---- DynamicsCompressor（Chromium の DynamicsCompressor をそのまま写す） ----

const RELEASE_ZONE: [f32; 4] = [0.09, 0.16, 0.42, 0.98];
const MAX_PRE_DELAY: usize = 1024;
const DIVISION: usize = 32;

fn lin_to_db(x: f32) -> f32 {
    20.0 * x.log10()
}
fn db_to_lin(x: f32) -> f32 {
    10f32.powf(0.05 * x)
}
fn finite(x: f32, d: f32) -> f32 {
    if x.is_finite() { x } else { d }
}

struct Compressor {
    linear_threshold: f32,
    slope: f32,
    knee_threshold: f32,
    db_knee_threshold: f32,
    db_yknee_threshold: f32,
    k: f32,
    post_gain: f32,
    attack_frames: f32,
    sat_release_frames: f32,
    poly: [f32; 5],
    detector: f32,
    gain: f32,
    max_attack_diff: f32,
    buf: Box<[[f32; 2]; MAX_PRE_DELAY]>,
    read: usize,
    write: usize,
    div: usize,
    rate: f32,
    scaled_desired: f32,
}
impl Compressor {
    fn new(sr: f32, db_threshold: f32, db_knee: f32, ratio: f32, attack: f32, release: f32) -> Self {
        let linear_threshold = db_to_lin(db_threshold);
        let slope = 1.0 / ratio;
        let knee_curve = |x: f32, k: f32| if x < linear_threshold { x } else { linear_threshold + (1.0 - (-k * (x - linear_threshold)).exp()) / k };
        // KAtSlope
        let db_x = db_threshold + db_knee;
        let x = db_to_lin(db_x);
        let (mut x2, mut db_x2) = (1.0, 0.0);
        if x >= linear_threshold {
            x2 = x * 1.001;
            db_x2 = lin_to_db(x2);
        }
        let (mut min_k, mut max_k, mut k) = (0.1f32, 10000.0f32, 5.0f32);
        let mut s = 1.0;
        for _ in 0..15 {
            if x >= linear_threshold {
                s = (lin_to_db(knee_curve(x2, k)) - lin_to_db(knee_curve(x, k))) / (db_x2 - db_x);
            }
            if s < slope { max_k = k; } else { min_k = k; }
            k = (min_k * max_k).sqrt();
        }
        let db_knee_threshold = db_x;
        let knee_threshold = db_to_lin(db_knee_threshold);
        let db_yknee_threshold = lin_to_db(knee_curve(knee_threshold, k));
        let release_frames = sr * release;
        let [z1, z2, z3, z4] = RELEASE_ZONE;
        let poly = [
            0.9999999999999998 * z1 + 1.8432219684323923e-16 * z2 - 1.9373394351676423e-16 * z3 + 8.824516011816245e-18 * z4,
            -1.5788320352845888 * z1 + 2.3305837032074286 * z2 - 0.9141194204840429 * z3 + 0.1623677525612032 * z4,
            0.5334142869106424 * z1 - 1.272736789213631 * z2 + 0.9258856042207512 * z3 - 0.18656310191776226 * z4,
            0.08783463138207234 * z1 - 0.1694162967925622 * z2 + 0.08588057951595272 * z3 - 0.00429891410546283 * z4,
            -0.042416883008123074 * z1 + 0.1115693827987602 * z2 - 0.09764676325265872 * z3 + 0.028494263462021576 * z4,
        ]
        .map(|c: f32| c * release_frames);
        let pre_delay = ((0.006 * sr) as usize).min(MAX_PRE_DELAY - 1);
        let mut c = Self {
            linear_threshold,
            slope,
            knee_threshold,
            db_knee_threshold,
            db_yknee_threshold,
            k,
            post_gain: 1.0,
            attack_frames: attack.max(0.001) * sr,
            sat_release_frames: 0.0025 * sr,
            poly,
            detector: 0.0,
            gain: 1.0,
            max_attack_diff: -1.0,
            buf: Box::new([[0.0; 2]; MAX_PRE_DELAY]),
            read: 0,
            write: pre_delay,
            div: 0,
            rate: 0.0,
            scaled_desired: 0.0,
        };
        c.post_gain = (1.0 / c.saturate(1.0)).powf(0.6);
        c
    }
    fn saturate(&self, x: f32) -> f32 {
        if x < self.knee_threshold {
            if x < self.linear_threshold { x } else { self.linear_threshold + (1.0 - (-self.k * (x - self.linear_threshold)).exp()) / self.k }
        } else {
            db_to_lin(self.db_yknee_threshold + self.slope * (lin_to_db(x) - self.db_knee_threshold))
        }
    }
    /// 32 フレームごとに目標と速さを決める
    fn division(&mut self) {
        self.detector = finite(self.detector, 1.0);
        let scaled = self.detector.asin() / std::f32::consts::FRAC_PI_2;
        let releasing = scaled > self.gain;
        let mut diff = if scaled == 0.0 { if releasing { -1.0 } else { 1.0 } } else { lin_to_db(self.gain / scaled) };
        self.rate = if releasing {
            self.max_attack_diff = -1.0;
            diff = finite(diff, -1.0);
            // 圧縮が強いほど速く戻す（-12..0 を 0..3 へ）
            let x = 0.25 * (diff.clamp(-12.0, 0.0) + 12.0);
            let (x2, x3, x4) = (x * x, x * x * x, x * x * x * x);
            let [a, b, c, d, e] = self.poly;
            db_to_lin(5.0 / (a + b * x + c * x2 + d * x3 + e * x4))
        } else {
            diff = finite(diff, 1.0);
            if self.max_attack_diff == -1.0 || self.max_attack_diff < diff {
                self.max_attack_diff = diff;
            }
            1.0 - (0.25 / self.max_attack_diff.max(0.5)).powf(1.0 / self.attack_frames)
        };
        self.scaled_desired = scaled;
    }
    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        if self.div == 0 {
            self.division();
        }
        self.div = (self.div + 1) % DIVISION;
        self.buf[self.write] = [l, r];
        let input = l.abs().max(r.abs());
        let attenuation = if input <= 0.0001 { 1.0 } else { self.saturate(input) / input };
        let db_att = (-lin_to_db(attenuation)).max(2.0);
        let sat_release = db_to_lin(db_att / self.sat_release_frames) - 1.0;
        let rate = if attenuation > self.detector { sat_release } else { 1.0 };
        self.detector = finite((self.detector + (attenuation - self.detector) * rate).min(1.0), 1.0);
        if self.rate < 1.0 {
            self.gain += (self.scaled_desired - self.gain) * self.rate;
        } else {
            self.gain = (self.gain * self.rate).min(1.0);
        }
        let warped = (std::f32::consts::FRAC_PI_2 * self.gain).sin();
        let total = self.post_gain * warped;
        let [dl, dr] = self.buf[self.read];
        self.read = (self.read + 1) & (MAX_PRE_DELAY - 1);
        self.write = (self.write + 1) & (MAX_PRE_DELAY - 1);
        (dl * total, dr * total)
    }
}

// ---- Engine（音のスレッドで動くグラフ全体） ----

struct Engine {
    sr: f32,
    rx: Receiver<Cmd>,
    voice_count: Arc<AtomicI32>,
    frames: Arc<AtomicU64>,
    frame: u64,
    voices: Vec<Voice>,
    tracks: Vec<TrackVoice>,
    /// 止める指示のあとに届いた古い曲は鳴らさない
    dead_below: u64,
    sfx_gain: f32,
    music_gain: Param,
    master: f32,
    comp: Compressor,
    /// 試験で素のバスを見るとき
    bypass_comp: bool,
    rng: Rng,
    sfx: [f32; BLOCK * 2],
    music: [f32; BLOCK * 2],
}
impl Engine {
    fn new(sr: f32, rx: Receiver<Cmd>, voice_count: Arc<AtomicI32>, frames: Arc<AtomicU64>, s: Settings) -> Self {
        Self {
            sr,
            rx,
            voice_count,
            frames,
            frame: 0,
            voices: Vec::with_capacity(256),
            tracks: Vec::with_capacity(8),
            dead_below: 0,
            sfx_gain: s.sfx,
            music_gain: Param::new(0.0, s.music),
            master: if s.muted { 0.0 } else { 1.0 },
            // threshold -14, knee は既定の 30, ratio 4, attack 0.003, release 0.2
            comp: Compressor::new(sr, -14.0, 30.0, 4.0, 0.003, 0.2),
            bypass_comp: false,
            rng: Rng(0x9E37_79B9_7F4A_7C15),
            sfx: [0.0; BLOCK * 2],
            music: [0.0; BLOCK * 2],
        }
    }
    fn t(&self) -> f64 {
        self.frame as f64 / self.sr as f64
    }
    fn apply(&mut self, c: Cmd) {
        let sr = self.sr;
        let t = self.t();
        match c {
            Cmd::Sample { pcm, rate, gain, at } => {
                let step = rate as f64 * pcm.rate as f64 / sr as f64;
                self.voices.push(Voice::Sample { pcm, pos: 0.0, step, gain, delay: (at * sr) as u32 });
            }
            Cmd::Thump { f0, f1, dur, vol, noise } => {
                self.voices.push(Voice::Osc {
                    wave: Wave::Sine,
                    phase: 0.0,
                    freq: ExpRamp::new(f0, f1.max(20.0), dur, sr),
                    env: Env::new(vol, 0.005, dur, sr),
                    lp: None,
                    left: ((dur + 0.05) * sr) as u32,
                    delay: 0,
                });
                if noise > 0.0 {
                    let seed = Rng(self.rng.0 ^ self.frame.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
                    self.rng.next();
                    self.voices.push(Voice::Noise { i: 0, len: (sr * dur.min(0.4)) as u32, lp: Biquad::lowpass(1400.0, sr), gain: vol * noise, seed });
                }
            }
            Cmd::Pluck { freq, vol, bright, at } => {
                let dur = if bright { 0.5 } else { 0.28 };
                let delay = (at * sr) as u32;
                for (mul, w, k) in [(1.0, Wave::Triangle, 1.0), (2.0, Wave::Sine, 0.35)] {
                    self.voices.push(Voice::Osc {
                        wave: w,
                        phase: 0.0,
                        freq: ExpRamp::fixed(freq * mul),
                        env: Env::new(vol * k, 0.004, dur, sr),
                        lp: None,
                        left: ((dur + 0.05) * sr) as u32,
                        delay,
                    });
                }
            }
            Cmd::Rise { dur, vol } => {
                for (f0, f1) in [(220.0, 880.0), (330.0, 1320.0)] {
                    self.voices.push(Voice::Osc {
                        wave: Wave::Saw,
                        phase: 0.0,
                        freq: ExpRamp::new(f0, f1, dur, sr),
                        env: Env::new(vol, dur * 0.8, dur + 0.1, sr),
                        lp: Some((Biquad::lowpass(600.0, sr), ExpRamp::new(600.0, 5000.0, dur, sr))),
                        left: ((dur + 0.2) * sr) as u32,
                        delay: 0,
                    });
                }
            }
            Cmd::StartTrack { id, mut stream, gain } => {
                if id < self.dead_below {
                    return;
                }
                stream.step = 48_000.0 / sr as f64;
                let mut g = Param::new(t, 0.0);
                g.linear_ramp(gain, t + 1.0);
                self.tracks.push(TrackVoice { id, stream, gain: g, stop_at: None });
            }
            Cmd::CrossFadeOut { id } => {
                self.dead_below = self.dead_below.max(id + 1);
                if let Some(v) = self.tracks.iter_mut().find(|v| v.id == id) {
                    let now_v = v.gain.at(t);
                    v.gain.cancel(t);
                    v.gain.set_value_at(now_v, t);
                    v.gain.linear_ramp(0.0, t + 1.2);
                    v.stop_at = Some(t + 1.4);
                }
            }
            Cmd::StopTrack { id } => {
                self.dead_below = self.dead_below.max(id + 1);
                if let Some(v) = self.tracks.iter_mut().find(|v| v.id == id) {
                    v.gain.linear_ramp(0.0, t + 0.8);
                    v.stop_at = Some(t + 0.9);
                }
            }
            Cmd::Duck { amount, dur, music } => {
                self.music_gain.cancel(t);
                self.music_gain.set_value_at(music * amount, t);
                self.music_gain.linear_ramp(music, t + dur as f64);
            }
            Cmd::Master(v) => self.master = v,
        }
    }
    /// 128 フレーム（interleaved 256）を作る
    fn block(&mut self, out: &mut [f32]) {
        while let Ok(c) = self.rx.try_recv() {
            self.apply(c);
        }
        self.sfx = [0.0; BLOCK * 2];
        self.music = [0.0; BLOCK * 2];
        let sr = self.sr;
        let mut i = 0;
        while i < self.voices.len() {
            if self.voices[i].mix(&mut self.sfx, sr) {
                i += 1;
            } else {
                if matches!(self.voices[i], Voice::Sample { .. }) {
                    self.voice_count.fetch_sub(1, Ordering::Relaxed);
                }
                self.voices.swap_remove(i);
            }
        }
        let dt = 1.0 / sr as f64;
        let t0 = self.t();
        let mut i = 0;
        while i < self.tracks.len() {
            let v = &mut self.tracks[i];
            if v.stop_at.is_some_and(|s| t0 >= s) || v.stream.ended {
                self.tracks.swap_remove(i);
                continue;
            }
            for f in 0..BLOCK {
                let g = v.gain.at(t0 + f as f64 * dt);
                let [l, r] = v.stream.tick();
                self.music[f * 2] += l * g;
                self.music[f * 2 + 1] += r * g;
            }
            i += 1;
        }
        for f in 0..BLOCK {
            let m = self.music_gain.at(t0 + f as f64 * dt);
            let l = (self.sfx[f * 2] * self.sfx_gain + self.music[f * 2] * m) * self.master;
            let r = (self.sfx[f * 2 + 1] * self.sfx_gain + self.music[f * 2 + 1] * m) * self.master;
            let (l, r) = if self.bypass_comp { (l, r) } else { self.comp.process(l, r) };
            out[f * 2] = l.clamp(-1.0, 1.0);
            out[f * 2 + 1] = r.clamp(-1.0, 1.0);
        }
        self.frame += BLOCK as u64;
        self.frames.store(self.frame, Ordering::Relaxed);
    }
}

/// デバイスの mixer へ渡す Source（1 サンプルずつ取り出す）
struct EngineSource {
    engine: Engine,
    buf: [f32; BLOCK * 2],
    i: usize,
}
impl Iterator for EngineSource {
    type Item = f32;
    #[inline]
    fn next(&mut self) -> Option<f32> {
        if self.i == BLOCK * 2 {
            self.engine.block(&mut self.buf);
            self.i = 0;
        }
        let s = self.buf[self.i];
        self.i += 1;
        Some(s)
    }
}
impl Source for EngineSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(2).unwrap()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(self.engine.sr as u32).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// 10 ms 前後（Web Audio の 'interactive' に近い）で開く。できなければ rodio の既定で開く
fn open_sink() -> Result<MixerDeviceSink, rodio::DeviceSinkError> {
    DeviceSinkBuilder::from_default_device()
        .and_then(|b| b.with_buffer_size(rodio::cpal::BufferSize::Fixed(512)).open_stream())
        .or_else(|_| DeviceSinkBuilder::open_default_sink())
}

// ---- ゲームから呼ぶ側 ----

const NOT_STARTED: u8 = 0;
const OPENING: u8 = 1;
const READY: u8 = 2;
const NO_DEVICE: u8 = 3;

struct Shared {
    samples: Vec<OnceLock<Arc<Pcm>>>,
    /// 主役ごとの決め台詞（主役の id → 声）
    lines: Mutex<std::collections::HashMap<String, Arc<Pcm>>>,
    music: [OnceLock<Option<Arc<OpusFile>>>; 4],
    voices: Arc<AtomicI32>,
    frames: Arc<AtomicU64>,
    state: AtomicU8,
}
impl Shared {
    fn load_samples(&self) {
        for i in 0..N_SAMPLES {
            // 読めない音は鳴らさない（呼ぶ側が合成音へ落とす）
            match decode_sample(&sample_path(i)) {
                Ok(p) => {
                    let _ = self.samples[i].set(Arc::new(p));
                }
                Err(e) => eprintln!("音を読めない: {e}"),
            }
        }
    }
    fn music_file(&self, t: Track) -> Option<Arc<OpusFile>> {
        self.music[t.index()]
            .get_or_init(|| match load_opus(&format!("music/{}", t.def().0)) {
                Ok(f) => Some(Arc::new(f)),
                Err(e) => {
                    eprintln!("曲を読めない: {e}");
                    None
                }
            })
            .clone()
    }
}

/// sample() の引数（既定は TS の `o.vol ?? 1` などと同じ）
#[derive(Clone, Copy)]
struct S {
    vol: f32,
    rate: f32,
    jitter: f32,
    at: f32,
    gap: f64,
}
const D: S = S { vol: 1.0, rate: 1.0, jitter: 0.06, at: 0.0, gap: 0.03 };

enum Clock {
    Live(Instant),
    Offline(Arc<AtomicU64>, f64),
}

struct Core {
    shared: Arc<Shared>,
    clock: Clock,
    tx: Option<Sender<Cmd>>,
    _stop: Option<Sender<()>>,
    settings: Settings,
    persist: bool,
    last_at: [f64; N_SAMPLES],
    last_pluck: f64,
    current: Option<(Track, u64)>,
    next_id: u64,
    rng: Rng,
}
impl Core {
    fn now(&self) -> f64 {
        match &self.clock {
            Clock::Live(s) => s.elapsed().as_secs_f64(),
            Clock::Offline(f, sr) => f.load(Ordering::Relaxed) as f64 / sr,
        }
    }
    fn ready(&self) -> bool {
        self.shared.state.load(Ordering::Relaxed) == READY
    }
    fn send(&self, c: Cmd) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(c);
        }
    }
    fn sample(&mut self, name: &str, o: S) -> bool {
        if !self.ready() {
            return false;
        }
        let Some(i) = sample_index(name) else { return false };
        let Some(buf) = self.shared.samples[i].get().cloned() else { return false };
        let t = self.now();
        if self.last_at[i] > t - o.gap {
            return true;
        }
        if self.shared.voices.load(Ordering::Relaxed) > 24 {
            return true;
        }
        self.last_at[i] = t;
        let rate = o.rate * (1.0 + (self.rng.next() - 0.5) * 2.0 * o.jitter);
        self.shared.voices.fetch_add(1, Ordering::Relaxed);
        self.send(Cmd::Sample { pcm: buf, rate, gain: o.vol, at: o.at });
        true
    }
    /// 合成の低い一撃（太鼓・どん）
    fn thump(&mut self, f0: f32, f1: f32, dur: f32, vol: f32, noise: f32) {
        if self.ready() {
            self.send(Cmd::Thump { f0, f1, dur, vol, noise });
        }
    }
    /// はじく音（三角波＋1オクターブ上のきらめき）。bright で余韻を長く。at 秒あとに鳴らすもの（和音の分散）は間引かない
    fn pluck(&mut self, freq: f32, vol: f32, bright: bool, at: f32) {
        if !self.ready() {
            return;
        }
        let t = self.now();
        if at < 0.0 {
            if t - self.last_pluck < 0.035 {
                return;
            }
            self.last_pluck = t;
        }
        self.send(Cmd::Pluck { freq, vol, bright, at: at.max(0.0) });
    }
    /// 上がっていく光の音（ゲージ満タン・乱舞）
    fn rise(&mut self, dur: f32, vol: f32) {
        if self.ready() {
            self.send(Cmd::Rise { dur, vol });
        }
    }
    fn duck(&mut self, amount: f32, dur: f32) {
        if self.ready() {
            self.send(Cmd::Duck { amount, dur, music: self.settings.music });
        }
    }
    fn blast(&mut self) {
        if !self.sample("boom", S { vol: 0.95, jitter: 0.05, ..D }) {
            self.thump(90.0, 30.0, 0.8, 0.9, 0.8);
        }
        self.thump(70.0, 28.0, 0.6, 0.6, 0.2);
    }
}

/// `createAudio()` の役。呼び出しはすべて &self で、1 コマの中で呼んでよい（指示を送るだけ）
pub struct Audio {
    core: Mutex<Core>,
    /// オフライン（試験・audio_check）のときの Engine
    offline: Option<Mutex<Engine>>,
}

pub fn create_audio() -> Audio {
    Audio::new()
}

impl Default for Audio {
    fn default() -> Self {
        Self::new()
    }
}

fn pow2(semi: f32) -> f32 {
    2f32.powf(semi / 12.0)
}

impl Audio {
    fn build(clock: Clock, frames: Arc<AtomicU64>, settings: Settings, persist: bool) -> Self {
        let shared = Arc::new(Shared {
            samples: (0..N_SAMPLES).map(|_| OnceLock::new()).collect(),
            lines: Mutex::default(),
            music: std::array::from_fn(|_| OnceLock::new()),
            voices: Arc::new(AtomicI32::new(0)),
            frames,
            state: AtomicU8::new(NOT_STARTED),
        });
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) | 1;
        let core = Core { shared, clock, tx: None, _stop: None, settings, persist, last_at: [-1.0; N_SAMPLES], last_pluck: -1.0, current: None, next_id: 1, rng: Rng(seed) };
        Self { core: Mutex::new(core), offline: None }
    }

    pub fn new() -> Self {
        Self::build(Clock::Live(Instant::now()), Arc::new(AtomicU64::new(0)), load_settings(), true)
    }

    /// デバイスを使わず、render で PCM を取り出す版（試験と audio_check）。設定は保存しない
    pub fn offline(sample_rate: u32) -> Self {
        let frames = Arc::new(AtomicU64::new(0));
        let settings = Settings::default();
        let mut a = Self::build(Clock::Offline(frames.clone(), sample_rate as f64), frames.clone(), settings, false);
        let (tx, rx) = mpsc::channel();
        let c = a.core.get_mut().unwrap();
        let voices = c.shared.voices.clone();
        c.tx = Some(tx);
        c.shared.state.store(READY, Ordering::Relaxed);
        c.shared.load_samples();
        a.offline = Some(Mutex::new(Engine::new(sample_rate as f32, rx, voices, frames, settings)));
        a
    }

    /// オフライン版で seconds 秒ぶんの PCM（interleaved stereo）を作る
    pub fn render(&self, seconds: f32) -> Vec<f32> {
        let Some(e) = &self.offline else { return Vec::new() };
        let mut e = e.lock().unwrap();
        let blocks = ((seconds * e.sr) as usize).div_ceil(BLOCK);
        let mut out = vec![0.0; blocks * BLOCK * 2];
        for b in out.chunks_exact_mut(BLOCK * 2) {
            e.block(b);
        }
        out
    }

    fn lock(&self) -> MutexGuard<'_, Core> {
        self.core.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 開く。2 回目からは何もしない。重い処理（デバイス・デコード）は別スレッド
    pub fn init(&self) {
        let c = &mut *self.lock();
        if c.shared.state.load(Ordering::Relaxed) != NOT_STARTED {
            return;
        }
        c.shared.state.store(OPENING, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        c.tx = Some(tx);
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        c._stop = Some(stop_tx);
        let settings = c.settings;
        let shared = c.shared.clone();
        let _ = std::thread::Builder::new().name("gyogun-audio-out".into()).spawn(move || match open_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                let sr = sink.config().sample_rate().get() as f32;
                let engine = Engine::new(sr, rx, shared.voices.clone(), shared.frames.clone(), settings);
                sink.mixer().add(EngineSource { engine, buf: [0.0; BLOCK * 2], i: BLOCK * 2 });
                shared.state.store(READY, Ordering::Relaxed);
                // Audio が落ちるまで出力を持っておく
                let _ = stop_rx.recv();
            }
            Err(e) => {
                eprintln!("音の出力を開けない（無音で続ける）: {e}");
                shared.state.store(NO_DEVICE, Ordering::Relaxed);
            }
        });
        let shared = c.shared.clone();
        let _ = std::thread::Builder::new().name("gyogun-audio-load".into()).spawn(move || shared.load_samples());
    }

    /// デバイスが開いて鳴らせる状態か
    pub fn device_ready(&self) -> bool {
        self.lock().ready()
    }
    /// デバイスが無いと分かった
    pub fn no_device(&self) -> bool {
        self.lock().shared.state.load(Ordering::Relaxed) == NO_DEVICE
    }
    /// 読み込めた効果音の数（全部で sample_total）
    pub fn samples_loaded(&self) -> usize {
        self.lock().shared.samples.iter().filter(|s| s.get().is_some()).count()
    }
    pub fn sample_total(&self) -> usize {
        N_SAMPLES
    }
    /// 音のスレッドが作ったフレーム数（デバイスが実際に音を引いているかの目安）
    pub fn frames_rendered(&self) -> u64 {
        self.lock().shared.frames.load(Ordering::Relaxed)
    }

    pub fn play_track(&self, track: Track) {
        self.init();
        let c = &mut *self.lock();
        if c.tx.is_none() || c.current.is_some_and(|(t, _)| t == track) {
            return;
        }
        if let Some((_, id)) = c.current {
            c.send(Cmd::CrossFadeOut { id });
        }
        let id = c.next_id;
        c.next_id += 1;
        c.current = Some((track, id));
        let (_, gain) = track.def();
        let looped = track != Track::Victory;
        let tx = c.tx.clone().unwrap();
        let shared = c.shared.clone();
        let start = move || {
            let Some(file) = shared.music_file(track) else { return };
            if let Ok(stream) = TrackStream::new(file, looped) {
                let _ = tx.send(Cmd::StartTrack { id, stream: Box::new(stream), gain });
            }
        };
        if self.offline.is_some() {
            start();
        } else {
            // 曲の読み込みと分解はコマを止めないように別スレッドで
            let _ = std::thread::Builder::new().name("gyogun-audio-track".into()).spawn(start);
        }
    }
    pub fn stop_music(&self) {
        let c = &mut *self.lock();
        if let Some((_, id)) = c.current.take() {
            c.send(Cmd::StopTrack { id });
        }
    }
    /// BGM を一瞬しぼる（乱舞の切り込み・大将の登場）
    pub fn duck(&self, amount: f32, dur: f32) {
        self.lock().duck(amount, dur);
    }
    pub fn muted(&self) -> bool {
        self.lock().settings.muted
    }
    pub fn toggle_mute(&self) -> bool {
        let c = &mut *self.lock();
        c.settings.muted = !c.settings.muted;
        c.send(Cmd::Master(if c.settings.muted { 0.0 } else { 1.0 }));
        if c.persist {
            save_settings(c.settings);
        }
        c.settings.muted
    }
    pub fn swing(&self, heavy: bool) {
        let c = &mut *self.lock();
        if heavy {
            if !c.sample("whoosh-heavy", S { vol: 0.75, jitter: 0.08, ..D }) {
                c.sample("ws", S { vol: 0.8, rate: 0.8, ..D });
            }
        } else {
            let n = if c.rng.next() < 0.5 { "slash-swing-1" } else { "slash-swing-2" };
            c.sample(n, S { vol: 0.55, jitter: 0.1, gap: 0.05, ..D });
        }
    }
    pub fn hit(&self, count: i32, heavy: bool) {
        let c = &mut *self.lock();
        let n = ["slash-hit-1", "slash-hit-2", "slash-hit-3"][((c.rng.next() * 3.0) as usize).min(2)];
        c.sample(n, S { vol: 0.5 + (count as f32 * 0.04).min(0.4), jitter: 0.12, gap: 0.04, ..D });
        if count >= 4 && !c.sample("crowd-hit", S { vol: (0.3 + count as f32 * 0.05).min(0.9), jitter: 0.08, gap: 0.06, ..D }) {
            c.sample("hit-2", S { vol: 0.5, rate: 0.8, ..D });
        }
        if heavy {
            c.sample("slash-crit", S { vol: 0.8, ..D });
            c.thump(140.0, 45.0, 0.35, 0.55, 0.4);
        }
    }
    pub fn blast(&self) {
        self.lock().blast();
    }
    pub fn ring(&self) {
        let c = &mut *self.lock();
        c.thump(110.0, 34.0, 0.55, 0.8, 0.6);
        c.sample("taiko", S { vol: 0.7, ..D });
    }
    pub fn kill(&self, captain: bool) {
        if !captain {
            return;
        }
        let c = &mut *self.lock();
        if !c.sample("shatter", S { vol: 0.9, ..D }) {
            c.sample("crit", D);
        }
        c.thump(60.0, 25.0, 1.1, 0.9, 0.5);
        c.sample("gong", S { vol: 0.8, ..D });
    }
    /// 群衆ドミノ: ぶつかるたびの打音に、和音階（ド・レ・ミ・ソ・ラ）で一段ずつ上がる音を重ねる（連鎖が旋律になる）
    pub fn chain(&self, n: i32, heavy: bool) {
        let c = &mut *self.lock();
        c.sample(if heavy { "hit-3" } else { "hit-2" }, S { vol: if heavy { 0.5 } else { 0.32 }, rate: 0.9, jitter: 0.05, gap: 0.045, ..D });
        let i = (n - 1).rem_euclid(15); // 3 オクターブで一巡り
        let semi = (i / 5) * 12 + PENTA[(i % 5) as usize];
        c.pluck(330.0 * pow2(semi as f32), if heavy { 0.16 } else { 0.12 }, n % 5 == 0, -1.0);
        if n % 10 == 0 {
            c.sample("taiko", S { vol: (0.4 + n as f32 * 0.008).min(0.9), rate: 1.0 + (n as f32 * 0.006).min(0.5), ..D });
        }
    }
    /// 節目（10・20・30…体）: 鈴の和音
    pub fn chain_mark(&self, n: i32) {
        let c = &mut *self.lock();
        let f = 660.0 * pow2(PENTA[(n.min(50) / 10 - 1).rem_euclid(5) as usize] as f32);
        c.pluck(f, 0.15, true, 0.0);
        c.pluck(f * 1.5, 0.11, true, 0.05);
        c.pluck(f * 2.0, 0.09, true, 0.1);
    }
    pub fn chain_big(&self, n: i32) {
        let c = &mut *self.lock();
        c.sample("lit", S { vol: 0.9, ..D });
        c.sample("crowd-hit", S { vol: 0.9, ..D });
        c.thump(80.0, 30.0, 0.7, (0.5 + n as f32 * 0.006).min(1.0), 0.4);
    }
    pub fn pop(&self) {
        self.lock().sample("hit-1", S { vol: 0.12, rate: 1.6, jitter: 0.2, gap: 0.05, ..D });
    }
    pub fn hurt(&self, big: bool) {
        let c = &mut *self.lock();
        c.sample("taken", S { vol: if big { 1.0 } else { 0.75 }, rate: if big { 0.85 } else { 1.0 }, ..D });
        if big {
            c.thump(120.0, 40.0, 0.4, 0.6, 0.5);
        }
    }
    pub fn jump(&self) {
        self.lock().sample("swish", S { vol: 0.5, rate: 1.2, ..D });
    }
    pub fn land(&self, heavy: bool) {
        let c = &mut *self.lock();
        if heavy {
            c.thump(100.0, 30.0, 0.5, 0.8, 0.7);
        } else {
            c.sample("hit-1", S { vol: 0.25, rate: 0.7, ..D });
        }
    }
    pub fn dodge(&self) {
        self.lock().sample("swish", S { vol: 0.65, rate: 0.9, ..D });
    }
    /// 飴鏡で跳ね返る: 硝子を弾いたような高い音
    pub fn reflect(&self) {
        let c = &mut *self.lock();
        c.pluck(1568.0, 0.11, true, 0.0);
        c.pluck(2349.0, 0.06, true, 0.03);
        c.sample("hit-1", S { vol: 0.2, rate: 1.8, gap: 0.04, ..D });
    }
    /// 格が上がる: 澄んだ音が段ごとに高くなる
    pub fn rank_up(&self, rank: i32) {
        let c = &mut *self.lock();
        c.sample("lit", S { vol: 0.75, rate: 0.9 + rank as f32 * 0.12, ..D });
        let f = 523.0 * pow2(PENTA[(rank * 2).rem_euclid(5) as usize] as f32);
        c.pluck(f, 0.14, true, 0.0);
        c.pluck(f * 1.5, 0.1, true, 0.06);
        c.pluck(f * 2.0, 0.08, true, 0.12);
    }
    pub fn ranbu_ready(&self) {
        let c = &mut *self.lock();
        c.sample("ready", S { vol: 0.9, ..D });
        c.sample("lit", S { vol: 0.6, at: 0.08, ..D });
    }
    pub fn ranbu_start(&self) {
        let c = &mut *self.lock();
        c.duck(0.25, 2.2);
        if !c.sample("ranbu-rise", D) {
            c.rise(0.8, 0.12);
        }
        c.sample("engage", S { vol: 0.9, ..D });
        c.thump(80.0, 40.0, 0.5, 0.7, 0.3);
    }
    pub fn ranbu_finish(&self) {
        let c = &mut *self.lock();
        c.blast();
        c.sample("gong", S { vol: 0.7, ..D });
    }
    pub fn captain(&self) {
        let c = &mut *self.lock();
        c.sample("notice", S { vol: 0.9, ..D });
        if !c.sample("taiko", S { vol: 0.9, ..D }) {
            c.thump(90.0, 40.0, 0.5, 0.8, 0.3);
        }
    }
    pub fn capture(&self) {
        let c = &mut *self.lock();
        c.sample("warp", S { vol: 0.8, ..D });
        if !c.sample("gong", S { vol: 0.9, at: 0.1, ..D }) {
            c.thump(70.0, 30.0, 1.2, 0.8, 0.2);
        }
    }
    pub fn boss(&self) {
        let c = &mut *self.lock();
        c.duck(0.2, 4.0);
        if !c.sample("roar", D) {
            c.thump(55.0, 25.0, 1.6, 1.0, 0.9);
        }
        c.sample("taiko", S { at: 0.4, ..D });
    }
    pub fn boss_hit_ground(&self) {
        let c = &mut *self.lock();
        c.thump(70.0, 24.0, 0.9, 1.0, 0.9);
        c.sample("boom", S { vol: 0.7, rate: 0.8, ..D });
    }
    pub fn enemy_strike(&self) {
        self.lock().sample("hit-3", S { vol: 0.2, rate: 1.3, gap: 0.08, ..D });
    }
    pub fn throw_sfx(&self) {
        self.lock().sample("swish", S { vol: 0.25, rate: 1.5, gap: 0.1, ..D });
    }
    pub fn milestone(&self) {
        let c = &mut *self.lock();
        c.sample("lit", S { vol: 0.9, ..D });
        c.sample("taiko", S { vol: 0.8, ..D });
    }
    /// 主役の決め台詞（voice/<id>/inoshikacho.wav）を読んでおく。声の無い主役は鳴らないだけ
    pub fn load_voice(&self, id: &str) {
        let shared = self.lock().shared.clone();
        if shared.lines.lock().unwrap_or_else(|e| e.into_inner()).contains_key(id) { return; }
        if let Ok(p) = decode_sample(&format!("voice/{id}/inoshikacho.wav")) { shared.lines.lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string(), Arc::new(p)); }
    }
    /// 主役の決め台詞を鳴らす
    pub fn voice(&self, id: &str, vol: f32) {
        let c = self.lock();
        if !c.ready() { return; }
        let Some(pcm) = c.shared.lines.lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned() else { return };
        c.shared.voices.fetch_add(1, Ordering::Relaxed);
        c.send(Cmd::Sample { pcm, rate: 1.0, gain: vol, at: 0.0 });
    }
    /// 飴が 1 個増えた（飴玉が鳴る）／飴が無い
    pub fn ame_gain(&self) {
        let mut c = self.lock();
        c.pluck(1318.5, 0.08, true, 0.0);
        c.pluck(1975.5, 0.06, true, 0.05);
    }
    pub fn ame_empty(&self) {
        self.lock().pluck(196.0, 0.1, false, 0.0);
    }
    /// 糸斬り（n = 1 猪・2 鹿・3 蝶。続くほど高く鳴る）
    pub fn wire_strike(&self, n: i32) {
        let mut c = self.lock();
        c.sample("slash-crit", S { vol: 0.85, rate: [1.0, 1.15, 1.32][(n.clamp(1, 3) - 1) as usize], jitter: 0.0, ..D });
        if n >= 3 { c.sample("gong", S { vol: 0.7, ..D }); c.sample("taiko", S { vol: 0.9, ..D }); }
    }
    /// 糸斬りのあと遅れて砂糖が割れる音（三撃目は低く重い）
    pub fn wire_crush(&self, n: i32) {
        let mut c = self.lock();
        c.sample("shatter", S { vol: if n >= 3 { 0.9 } else { 0.45 }, rate: if n >= 3 { 0.8 } else { 1.25 }, ..D });
        if n >= 3 { c.sample("boom", S { vol: 0.8, rate: 0.85, ..D }); }
    }
    pub fn pickup(&self, kind: PickupKind) {
        self.lock().sample(if kind == PickupKind::Bun { "spell" } else { "cast" }, S { vol: 0.8, ..D });
    }
    /// 結果の得点の数え上げの刻み（進むほど高く。k は 0..1）。今ある合成音（撥ねる音）で鳴らす
    pub fn count_tick(&self, k: f32) {
        self.lock().pluck(660.0 * pow2(k * 7.0), 0.05, false, 0.0);
    }
    /// 数え終わり（自己ベストでないとき）
    pub fn count_done(&self) {
        self.lock().pluck(880.0, 0.09, true, 0.0);
    }
    /// 自己ベスト: 上向きの短い和音。いちばん上の格（S）のときは 1 音多い
    pub fn cheer(&self, top: bool) {
        let c = &mut *self.lock();
        c.sample("lit", S { vol: 0.55, ..D });
        for (i, f) in CHEER_NOTES[..if top { 4 } else { 3 }].iter().enumerate() { c.pluck(*f, 0.13, true, i as f32 * 0.07); }
    }
    /// S のときの、上からのもう 1 回
    pub fn cheer_encore(&self) {
        let c = &mut *self.lock();
        for (i, f) in [1567.98, 1975.53, 2349.32].iter().enumerate() { c.pluck(*f, 0.08, true, i as f32 * 0.05); }
    }
    pub fn select(&self) {
        self.lock().sample("select", S { vol: 0.7, ..D });
    }
    pub fn confirm(&self) {
        self.lock().sample("engage", S { vol: 0.8, ..D });
    }
    pub fn defeat(&self) {
        self.lock().sample("defeat", S { vol: 0.9, ..D });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(p: &[f32]) -> f32 {
        p.iter().fold(0.0, |a, b| a.max(b.abs()))
    }
    /// 振幅が a を超えている最後の時刻（秒）
    fn last_above(p: &[f32], a: f32, sr: f32) -> f32 {
        p.chunks_exact(2).rposition(|f| f[0].abs() > a).map_or(0.0, |i| i as f32 / sr)
    }
    fn raw(sr: u32) -> Audio {
        let a = Audio::offline(sr);
        a.offline.as_ref().unwrap().lock().unwrap().bypass_comp = true;
        a
    }

    #[test]
    fn 全部の素材を読める() {
        let a = Audio::offline(48_000);
        assert_eq!(a.samples_loaded(), N_SAMPLES);
    }

    #[test]
    fn 同じ音は間隔より短いと重ねない() {
        let a = raw(48_000);
        // 同じ時刻に 2 回: 2 回目は間引かれるが true を返す（合成音へ落ちない）
        let c = &mut *a.lock();
        assert!(c.sample("hit-1", D));
        assert!(c.sample("hit-1", D));
        assert_eq!(c.shared.voices.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn 同時に鳴る数は25で止まる() {
        let a = raw(48_000);
        let c = &mut *a.lock();
        for i in 0..N_SAMPLES {
            c.last_at[i] = -1.0;
        }
        let names: Vec<&str> = WILDS.iter().chain(OWN.iter()).copied().collect();
        for n in names.iter() {
            c.sample(n, D);
        }
        // voices > 24 で止めるので 25 まで（TS と同じ）
        assert_eq!(c.shared.voices.load(Ordering::Relaxed), 25);
    }

    #[test]
    fn 鳴り終わると数が戻る() {
        let a = raw(48_000);
        a.select();
        assert_eq!(a.lock().shared.voices.load(Ordering::Relaxed), 1);
        a.render(3.0);
        assert_eq!(a.lock().shared.voices.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn thumpの長さと音量はtsの式() {
        let a = raw(48_000);
        a.lock().thump(140.0, 45.0, 0.35, 0.55, 0.0);
        let p = a.render(0.6);
        // gain 0.55 × sfx 0.85。頂点は 5 ms（140 Hz の sin は 5 ms ではまだ上り坂なので、少し手前で最大になる）
        let pk = peak(&p);
        assert!(pk > 0.3 && pk <= 0.55 * 0.85 + 1e-3, "peak {pk}");
        // 発振器は dur + 0.05 で止まる
        assert!(last_above(&p, 1e-6, 48_000.0) <= 0.401, "{}", last_above(&p, 1e-6, 48_000.0));
        assert!(last_above(&p, 0.001, 48_000.0) < 0.35);
    }

    #[test]
    fn thumpの周波数は指数で下がる() {
        // 0.35 秒で 140 → 45 Hz。半分の時刻では幾何平均
        let mut r = ExpRamp::new(140.0, 45.0, 0.35, 48_000.0);
        let n = (0.35 * 48_000.0) as usize;
        let mut mid = 0.0;
        for i in 0..n {
            let v = r.step();
            if i == n / 2 {
                mid = v;
            }
        }
        assert!((mid - (140.0f64 * 45.0).sqrt()).abs() < 0.05, "{mid}");
        assert!((r.step() - 45.0).abs() < 1e-6);
    }

    #[test]
    fn pluckは35ミリ秒より短い連打を間引く() {
        let a = raw(48_000);
        a.lock().pluck(660.0, 0.15, false, -1.0);
        a.lock().pluck(660.0, 0.15, false, -1.0);
        let one = a.render(0.4);
        // at を付けたものは間引かない
        let b = raw(48_000);
        b.lock().pluck(660.0, 0.15, false, 0.0);
        b.lock().pluck(660.0, 0.15, false, 0.0);
        let two = b.render(0.4);
        assert!(peak(&two) > peak(&one) * 1.8, "{} {}", peak(&two), peak(&one));
    }

    #[test]
    fn duckは下げてから直線で戻す() {
        let (tx, rx) = mpsc::channel();
        let mut e = Engine::new(48_000.0, rx, Arc::new(AtomicI32::new(0)), Arc::new(AtomicU64::new(0)), Settings::default());
        tx.send(Cmd::Duck { amount: 0.25, dur: 2.0, music: 0.5 }).unwrap();
        let mut b = [0.0; BLOCK * 2];
        e.block(&mut b);
        assert!((e.music_gain.at(0.0) - 0.125).abs() < 1e-5);
        assert!((e.music_gain.at(1.0) - 0.3125).abs() < 1e-5);
        assert!((e.music_gain.at(2.0) - 0.5).abs() < 1e-5);
        assert!((e.music_gain.at(9.0) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn ランプは直前の予定の時刻から引く() {
        // stopMusic は cancel も setValue もしないので、Web Audio と同じくフェードインの終わりから線が引かれる
        let mut g = Param::new(0.0, 0.0);
        g.linear_ramp(1.0, 1.0);
        assert!((g.at(0.5) - 0.5).abs() < 1e-6);
        assert!((g.at(100.0) - 1.0).abs() < 1e-6);
        g.linear_ramp(0.0, 100.8);
        assert!((g.at(100.0) - 0.8 / 99.8).abs() < 1e-4, "{}", g.at(100.0));
    }

    #[test]
    fn コンプレッサーのしきい値より小さい音はメイクアップだけ() {
        let mut c = Compressor::new(48_000.0, -14.0, 30.0, 4.0, 0.003, 0.2);
        // full range の式: (1 / saturate(1))^0.6
        assert!((c.post_gain - (1.0 / c.saturate(1.0)).powf(0.6)).abs() < 1e-6);
        assert!(c.post_gain > 1.0);
        let mut last = 0.0;
        for i in 0..96_000 {
            let s = (i as f32 * 0.05).sin() * 0.05; // -26 dB
            last = c.process(s, s).0.abs().max(if i > 95_000 { last } else { 0.0 });
        }
        assert!((last / 0.05 - c.post_gain).abs() < 0.03, "{} {}", last / 0.05, c.post_gain);
    }

    #[test]
    fn コンプレッサーは大きい音を抑える() {
        let mut c = Compressor::new(48_000.0, -14.0, 30.0, 4.0, 0.003, 0.2);
        let mut last = 0.0f32;
        for i in 0..96_000 {
            let s = (i as f32 * 0.05).sin();
            let o = c.process(s, s).0;
            if i > 95_000 {
                last = last.max(o.abs());
            }
        }
        // 振幅 1 の音は静特性の saturate(1)（と post_gain）まで下がる
        assert!((last - c.post_gain * c.saturate(1.0)).abs() < 0.03, "{last} {} {}", c.post_gain, c.saturate(1.0));
        assert!(last < c.post_gain * 0.95);
    }

    #[test]
    fn 曲は流れてループしクロスフェードする() {
        let a = raw(48_000);
        a.play_track(Track::Victory);
        let p = a.render(1.0);
        // 1 秒のフェードインなので、後半の方が大きい
        let (h, t) = p.split_at(p.len() / 2);
        assert!(peak(t) > peak(h) * 0.9 && peak(t) > 0.01);
        a.play_track(Track::Field);
        a.render(1.3);
        // 前の曲は 1.4 秒で止まる
        a.render(0.2);
        let e = a.offline.as_ref().unwrap().lock().unwrap();
        assert_eq!(e.tracks.len(), 1);
    }

    #[test]
    fn 止めたあとに届いた曲は鳴らさない() {
        let (tx, rx) = mpsc::channel();
        let mut e = Engine::new(48_000.0, rx, Arc::new(AtomicI32::new(0)), Arc::new(AtomicU64::new(0)), Settings::default());
        let f = Arc::new(load_opus(&format!("music/{}", Track::Victory.def().0)).unwrap());
        tx.send(Cmd::StopTrack { id: 3 }).unwrap();
        tx.send(Cmd::StartTrack { id: 3, stream: Box::new(TrackStream::new(f, false).unwrap()), gain: 1.0 }).unwrap();
        let mut b = [0.0; BLOCK * 2];
        e.block(&mut b);
        assert!(e.tracks.is_empty());
    }

    #[test]
    fn 消音は全部を黙らせる() {
        let a = raw(48_000);
        a.toggle_mute();
        assert!(a.muted());
        a.lock().thump(140.0, 45.0, 0.2, 0.8, 0.5);
        assert_eq!(peak(&a.render(0.3)), 0.0);
        a.toggle_mute();
        assert!(!a.muted());
    }

    #[test]
    fn 開く前の呼び出しは何もしない() {
        let a = Audio::build(Clock::Live(Instant::now()), Arc::new(AtomicU64::new(0)), Settings::default(), false);
        a.swing(true);
        a.hit(5, true);
        a.boss();
        a.stop_music();
        assert!(!a.lock().sample("hit-1", D));
    }

    #[test]
    fn 全部の名前が素材にある() {
        let a = raw(48_000);
        // TS の呼び出しで使う名前をひと通り鳴らす（無い名前なら false）
        for n in ["whoosh-heavy", "ws", "slash-swing-1", "slash-swing-2", "slash-hit-1", "slash-hit-2", "slash-hit-3", "crowd-hit", "hit-2", "slash-crit", "boom", "taiko", "shatter", "crit", "gong", "hit-3", "lit", "hit-1", "taken", "swish", "ready", "ranbu-rise", "engage", "notice", "warp", "roar", "spell", "cast", "select", "defeat"] {
            a.lock().shared.voices.store(0, Ordering::Relaxed);
            assert!(a.lock().sample(n, D), "{n}");
        }
    }

    /// Chrome 152 の OfflineAudioContext で同じグラフ（master → compressor、sfx 0.85）を描いた値。960 サンプルごとの RMS を 1 つおきに
    #[test]
    fn 合成音はchromeと同じ() {
        fn windows(p: &[f32]) -> Vec<f32> {
            let l: Vec<f32> = p.chunks_exact(2).map(|f| f[0]).collect();
            l.chunks_exact(960).step_by(2).map(|w| (w.iter().map(|x| x * x).sum::<f32>() / 960.0).sqrt()).collect()
        }
        let check = |t0: f32, secs: f32, f: &dyn Fn(&mut Core), want: &[f32], tol: f32| {
            let a = Audio::offline(48_000);
            if t0 > 0.0 {
                a.render(t0);
            }
            f(&mut a.lock());
            let got = windows(&a.render(secs));
            for (i, w) in want.iter().enumerate() {
                assert!((got[i] - w).abs() <= tol, "t0={t0} 窓 {i}: {} と {w}", got[i]);
            }
        };
        // コンプレッサーが動き出したばかりの立ち上がり（t = 0）
        check(0.0, 0.6, &|c| c.thump(140.0, 45.0, 0.35, 0.55, 0.0), &[0.05585, 0.07479, 0.04071, 0.01645, 0.00651, 0.00257, 0.0009], 2e-5);
        // 落ち着いたあと（t = 1）
        check(1.0, 0.6, &|c| c.thump(140.0, 45.0, 0.35, 0.55, 0.0), &[0.22494, 0.13753, 0.05153, 0.01787, 0.00663, 0.00257, 0.0009], 2e-5);
        // 強く圧縮するとき（先頭の 2 窓は出力の 1.0 で切れるので比べない）
        let a = Audio::offline(48_000);
        a.render(1.0);
        { let c = &mut *a.lock(); c.thump(60.0, 25.0, 1.1, 0.9, 0.0); c.thump(80.0, 30.0, 0.7, 1.0, 0.0); }
        let got = windows(&a.render(1.4));
        for (i, w) in [0.56306, 0.1536, 0.32617, 0.08318, 0.15226, 0.08594, 0.03965].iter().enumerate() {
            assert!((got[i + 1] - w).abs() <= 3e-5, "loud 窓 {}: {} と {w}", i + 1, got[i + 1]);
        }
        // 三角波＋サイン、鳴らす時刻のずれ
        check(1.0, 0.8, &|c| { c.pluck(660.0, 0.15, true, 0.0); c.pluck(990.0, 0.11, true, 0.05); c.pluck(1320.0, 0.09, true, 0.1); }, &[0.05788, 0.04944, 0.04979, 0.05257, 0.03086, 0.0186, 0.01094], 3e-5);
        // のこぎり波＋動く lowpass
        check(1.0, 1.2, &|c| c.rise(0.8, 0.12), &[0.00005, 0.0001, 0.00016, 0.00024, 0.00037, 0.00056, 0.00089, 0.0014, 0.00214, 0.00333, 0.00518, 0.00813, 0.01284, 0.0195, 0.03081, 0.04798, 0.06241, 0.02226, 0.00747, 0.00251], 1e-4);
    }
}
