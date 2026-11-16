// characters の island-kit（crates/island-kit/src/app/media.rs、0.1.31）の写し。gyogun は island-kit を使わないので写して持つ。直すときは island-kit と揃える。
// 曲を鳴らす <audio> を使い回す。
// iPhone・iPad の WebKit（iPad の Chrome も同じ）は、<audio> ごとに「操作の直後に play した」ことがないと、
// あとから play させない（NotAllowedError。PC の Chrome は一度どこかを触れば、以後はいつでも鳴る）。
// そこで <audio> を決まった数だけ先に作り、最初の操作（押す・離す・キー・クリック）の中で、
// 空いている物は無音を鳴らして止め、曲を待っている物（操作の前に頼まれて止められた曲）はその曲を鳴らす。
// 以後はその <audio> の src を差し替えて使う。操作のたびに AudioContext も、動いていなければ起こす
// （iOS は止まった状態に suspended のほか interrupted がある）。2026-11-11 iPad のシミュレータで確かめた。
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{AddEventListenerOptions, AudioContext, AudioContextState, AudioNode, GainNode, HtmlAudioElement, MediaElementAudioSourceNode};

/// 0.01 秒の無音（8000Hz・16bit・1ch の WAV）
fn silent_url() -> String {
    const HEAD: &str = "UklGRsQAAABXQVZFZm10IBAAAAABAAEAQB8AAIA+AAACABAAZGF0YaA";
    format!("data:audio/wav;base64,{HEAD}{}", "A".repeat(272 - HEAD.len()))
}

fn now_ms() -> f64 { web_sys::window().and_then(|w| w.performance()).map_or(0.0, |p| p.now()) }

struct Slot {
    el: HtmlAudioElement,
    /// 一度 play が通った（WebKit がこの <audio> を以後 play させる）
    unlocked: Cell<bool>,
    trying: Cell<bool>,
    /// 曲が鳴っているはず（止められていたら、最初の操作で鳴らし直す）
    want: Cell<bool>,
    /// 使い始めた回数（止める予約が、次に使った曲を止めないように）
    turn: Cell<u32>,
    /// この時刻（performance.now の ms）までは使用中
    busy_until: Cell<f64>,
    node: RefCell<Option<(MediaElementAudioSourceNode, GainNode)>>,
}

impl Slot {
    fn new() -> Rc<Slot> {
        let el = HtmlAudioElement::new().unwrap();
        el.set_cross_origin(Some("anonymous"));
        el.set_preload("auto");
        Rc::new(Slot { el, unlocked: Cell::new(false), trying: Cell::new(false), want: Cell::new(false), turn: Cell::new(0), busy_until: Cell::new(0.0), node: RefCell::new(None) })
    }
    /// play して、通ったら印を付ける。after は通ったあとに呼ぶ
    fn play(self: &Rc<Slot>, after: impl FnOnce(&Slot) + 'static) {
        let Ok(p) = self.el.play() else { self.trying.set(false); return };
        let s = self.clone();
        spawn_local(async move {
            let ok = JsFuture::from(p).await.is_ok();
            s.trying.set(false);
            if ok { s.unlocked.set(true); after(&s); }
        });
    }
}

struct Inner { slots: RefCell<Vec<Rc<Slot>>>, ctx: RefCell<Option<AudioContext>> }

/// 複製しても同じ物を指す
#[derive(Clone)]
pub struct MediaSlots { i: Rc<Inner> }

/// 鳴らしている 1 本。gain の音量はゲームが動かす
pub struct MediaVoice { pub el: HtmlAudioElement, pub gain: GainNode, slot: Rc<Slot>, turn: u32 }

impl MediaVoice {
    /// ms ミリ秒たったら止める（そのあいだに別の曲に使われたら止めない）
    pub fn stop_after(&self, ms: i32) {
        let s = self.slot.clone();
        let turn = self.turn;
        s.want.set(false);
        s.busy_until.set(now_ms() + ms as f64 + 50.0);
        let c = Closure::once_into_js(move || { if s.turn.get() == turn { let _ = s.el.pause(); } });
        if let Some(w) = web_sys::window() { let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(c.unchecked_ref(), ms.max(0)); }
    }
}

impl MediaSlots {
    /// n 本の <audio> を作り、最初の操作で鳴らせるようにする
    pub fn new(n: usize) -> MediaSlots {
        let me = MediaSlots { i: Rc::new(Inner { slots: RefCell::new((0..n).map(|_| Slot::new()).collect()), ctx: RefCell::new(None) }) };
        if let Some(win) = web_sys::window() {
            // ゲームの画面のボタンが伝わりを止めても届くよう、window の capture で受ける。指は押した時ではなく離した時が「操作」
            for ty in ["pointerdown", "pointerup", "touchend", "keydown", "click"] {
                let m = me.clone();
                let c = Closure::<dyn FnMut(JsValue)>::new(move |_| m.on_gesture());
                let o = AddEventListenerOptions::new();
                o.set_capture(true);
                o.set_passive(true);
                let _ = win.add_event_listener_with_callback_and_add_event_listener_options(ty, c.as_ref().unchecked_ref(), &o);
                c.forget();
            }
        }
        me
    }

    /// 操作のたびに起こす AudioContext
    pub fn set_ctx(&self, ctx: &AudioContext) { *self.i.ctx.borrow_mut() = Some(ctx.clone()); }

    fn on_gesture(&self) {
        if let Some(ctx) = self.i.ctx.borrow().as_ref() {
            let st = ctx.state();
            if st != AudioContextState::Running && st != AudioContextState::Closed { let _ = ctx.resume(); }
        }
        let now = now_ms();
        for s in self.i.slots.borrow().iter() {
            if s.unlocked.get() || s.trying.get() { continue; }
            if s.want.get() {
                // 操作の前に頼まれて止められた曲（オープニングなど）を、今鳴らす
                if s.el.paused() { s.trying.set(true); s.play(|_| {}); }
            } else if s.busy_until.get() <= now {
                s.trying.set(true);
                let turn = s.turn.get();
                s.el.set_loop(false);
                s.el.set_src(&silent_url());
                s.play(move |s| { if s.turn.get() == turn && !s.want.get() { let _ = s.el.pause(); } });
            }
        }
    }

    /// 空いている <audio> で url を鳴らし始める。音は target へ流す。gain の初めの音量は gain0
    pub fn start(&self, ctx: &AudioContext, url: &str, looped: bool, target: &AudioNode, gain0: f32) -> Result<MediaVoice, JsValue> {
        let now = now_ms();
        // 無音で鳴らせるようにしている最中の物は後回し（操作の直後なら、上書きしても曲の play が通る）
        let free = |s: &&Rc<Slot>| !s.want.get() && s.busy_until.get() <= now;
        let found = { let v = self.i.slots.borrow(); v.iter().filter(free).find(|s| !s.trying.get()).or_else(|| v.iter().find(free)).cloned() };
        // 足りなければ足す（PC では鳴る。iPhone・iPad では最初の操作の後に足した物は、次の操作まで鳴らない）
        let s = match found { Some(s) => s, None => { let s = Slot::new(); self.i.slots.borrow_mut().push(s.clone()); s } };
        s.turn.set(s.turn.get() + 1);
        s.want.set(true);
        s.busy_until.set(f64::INFINITY);
        let gain = {
            let mut node = s.node.borrow_mut();
            if node.is_none() {
                let src = ctx.create_media_element_source(&s.el)?;
                let g = ctx.create_gain()?;
                src.connect_with_audio_node(&g)?;
                *node = Some((src, g));
            }
            node.as_ref().unwrap().1.clone()
        };
        let _ = gain.disconnect();
        gain.connect_with_audio_node(target)?;
        let p = gain.gain();
        p.cancel_scheduled_values(0.0)?;
        p.set_value_at_time(gain0, ctx.current_time())?;
        s.el.set_loop(looped);
        s.el.set_src(url);
        s.play(|_| {});
        Ok(MediaVoice { el: s.el.clone(), gain, turn: s.turn.get(), slot: s })
    }
}
