// 遊びの輪: requestAnimationFrame・確かめ用の setInterval・固定の刻み。
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

fn window() -> web_sys::Window { web_sys::window().expect("window") }

/// performance.now()（ミリ秒）
pub fn now_ms() -> f64 { window().performance().map_or(0.0, |p| p.now()) }

/// 輪を回す。f には performance.now() を渡す（TS の frame と同じ時計。drive と混ぜても時刻が戻らない）。止めない
pub fn start(mut f: impl FnMut(f64) + 'static) {
    let slot: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::default();
    let again = slot.clone();
    *slot.borrow_mut() = Some(Closure::new(move |_: f64| {
        if let Some(c) = again.borrow().as_ref() { let _ = window().request_animation_frame(c.as_ref().unchecked_ref()); }
        f(now_ms());
    }));
    if let Some(c) = slot.borrow().as_ref() { let _ = window().request_animation_frame(c.as_ref().unchecked_ref()); }
}

/// 隠れたタブでも進める確かめ用（TS の `__game.drive = () => setInterval(frame, 16)`）。戻りは stop_drive に渡す番号
pub fn drive(interval_ms: i32, mut f: impl FnMut(f64) + 'static) -> i32 {
    let c = Closure::<dyn FnMut()>::new(move || f(now_ms()));
    let id = window().set_interval_with_callback_and_timeout_and_arguments_0(c.as_ref().unchecked_ref(), interval_ms).unwrap_or(0);
    c.forget();
    id
}
pub fn stop_drive(id: i32) { window().clear_interval_with_handle(id); }

/// setTimeout
pub fn after_ms(ms: i32, f: impl FnOnce() + 'static) {
    let c = Closure::once_into_js(f);
    let _ = window().set_timeout_with_callback_and_timeout_and_arguments_0(c.unchecked_ref(), ms);
}

/// 1 コマの秒数（TS の `Math.min(0.05, (now - last) / 1000)`）
pub struct FrameClock { last: f64, pub max_dt: f64 }
impl FrameClock {
    pub fn new() -> Self { FrameClock { last: now_ms(), max_dt: 0.05 } }
    pub fn tick(&mut self, now: f64) -> f64 {
        let dt = ((now - self.last) / 1000.0).min(self.max_dt);
        self.last = now;
        dt
    }
}
impl Default for FrameClock { fn default() -> Self { Self::new() } }

/// 1/120 秒の固定の刻み（TS の acc）。
/// ```ignore
/// step.add(dt);
/// while step.next() { sim.step(step.step); if hitstop { step.clear(); break; } }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct FixedStep { pub step: f64, pub acc: f64 }
impl FixedStep {
    pub const HZ120: f64 = 1.0 / 120.0;
    pub fn new() -> Self { FixedStep { step: Self::HZ120, acc: 0.0 } }
    pub fn with_step(step: f64) -> Self { FixedStep { step, acc: 0.0 } }
    pub fn add(&mut self, dt: f64) { self.acc += dt; }
    /// 刻みが 1 つ走れるなら acc から引いて true
    pub fn next(&mut self) -> bool {
        if self.acc >= self.step { self.acc -= self.step; true } else { false }
    }
    /// 余りを捨てる（ヒットストップの `acc = 0`）
    pub fn clear(&mut self) { self.acc = 0.0; }
}
impl Default for FixedStep { fn default() -> Self { Self::new() } }
