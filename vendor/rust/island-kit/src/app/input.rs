// 入力の材料: キーボード・マウス・ポインタ・ゲームパッド・タッチ（pengo・ひな形の input.ts の共通の所）。
// 割り当て表はゲームが持つ。毎コマ read() を 1 回呼び、Frame の生の値から自分の Controls を作る。
// 押した瞬間（pressed）は read() ごとに 1 回だけ立つ。固定の刻みへ渡すときは Latch で持ち越す。
use super::dom;
pub use crate::device::{Device, Pad};
use crate::device::{DeviceEvent, DeviceRule, PadReader, TouchRole, stick_value, touch_role};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use web_sys::{Element, Gamepad, GamepadButton, HtmlElement, KeyboardEvent, MouseEvent, PointerEvent, Touch as WebTouch, TouchEvent};

/// ゲームごとの違い。既定は pengo
#[derive(Clone, Debug)]
pub struct InputOpts {
    /// タッチを受ける要素（#touch。中の .stick・.knob・[data-btn] を使う）。無ければタッチの仮想スティックとボタンは動かない
    pub touch_root: Option<HtmlElement>,
    /// preventDefault するキー
    pub prevent_keys: Vec<String>,
    /// 仮想スティックの半径（px）。pengo 56・ひな形 60
    pub stick_radius: f64,
    /// スティックを 4 方向に吸い付かせる（マス目の遊び）
    pub stick_four_way: bool,
    /// スティックとボタン以外の指でカメラを回す（ひな形）。false なら叩いただけの扱い（pengo）
    pub touch_camera: bool,
    /// 右クリックのメニューを出さない
    pub prevent_context_menu: bool,
    /// 機器（device・touch.active・body の class touch）の決め方。今の版の input.ts がゲームごとに違う（`crate::device::DeviceRule`）。
    /// false: 追わない。最初の touchstart か (pointer: coarse) で印が付き、外れない（pengo・digdag・iceclimber・rainbow・bubble2）。
    /// true: 最後に触った機器に合わせて付け外しする。pointer_ignore が None なら keydown と canvas の mousedown でキー、touch_root の touchstart でタッチ（keiba・ひな形）。
    /// pointer_ignore が Some なら keydown でキー、pointerdown の pointerType で決める（pinball。指を離した後のまねの mousedown では戻らない）
    pub touch_class_follows_device: bool,
    /// 画面を押す遊び（pinball）: pointerdown を集める。Some はつかまえない要素のセレクタ（`"button, .card, a"`）
    pub pointer_ignore: Option<String>,
}
impl Default for InputOpts {
    fn default() -> Self {
        InputOpts {
            touch_root: None,
            prevent_keys: ["Space", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].iter().map(|s| s.to_string()).collect(),
            stick_radius: 56.0,
            stick_four_way: false,
            touch_camera: false,
            prevent_context_menu: false,
            touch_class_follows_device: false,
            pointer_ignore: None,
        }
    }
}

/// 画面を押している指・マウス
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointer { pub id: i32, pub x: f64, pub y: f64, pub is_touch: bool }

/// タッチの生の値
#[derive(Clone, Copy, Debug, Default)]
pub struct TouchValues {
    /// 指で操作している（機器が一度でもタッチになった。TS の input.touch）
    pub active: bool,
    /// 仮想スティック（x 右・y 上。-1..1）
    pub x: f64,
    pub y: f64,
    /// スティックとボタン以外を叩いた瞬間
    pub tap_pressed: bool,
    /// カメラの指が動いた量（px。touch_camera のとき）
    pub cam_dx: f64,
    pub cam_dy: f64,
}

/// 1 コマ分の材料（read() の戻り）
#[derive(Clone, Debug, Default)]
pub struct Frame {
    keys_down: HashSet<String>,
    keys_pressed: HashSet<String>,
    keys_released: HashSet<String>,
    mouse_down: [bool; 5],
    mouse_pressed: [bool; 5],
    mouse_released: [bool; 5],
    /// 押している間のマウスの動き（px）
    pub mouse_dx: f64,
    pub mouse_dy: f64,
    pub pad: Option<Pad>,
    /// つながっている全部のゲームパッド（4 つまで。pad はこの先頭と同じ。同じ画面で協力するゲーム用）
    pub pads: Vec<Pad>,
    pub touch: TouchValues,
    btn_down: HashSet<String>,
    btn_pressed: HashSet<String>,
    btn_released: HashSet<String>,
    /// 押している指・マウス（pointer_ignore が Some のとき）
    pub pointers: Vec<Pointer>,
    /// このコマで押した指・マウス
    pub pointers_pressed: Vec<Pointer>,
    pub device: Device,
}
impl Frame {
    pub fn down(&self, code: &str) -> bool { self.keys_down.contains(code) }
    pub fn pressed(&self, code: &str) -> bool { self.keys_pressed.contains(code) }
    pub fn released(&self, code: &str) -> bool { self.keys_released.contains(code) }
    pub fn any_down(&self, codes: &[&str]) -> bool { codes.iter().any(|c| self.down(c)) }
    pub fn any_pressed(&self, codes: &[&str]) -> bool { codes.iter().any(|c| self.pressed(c)) }
    /// どれかのキーを押した瞬間（TS の `pressed.size > 0`）
    pub fn any_key_pressed(&self) -> bool { !self.keys_pressed.is_empty() }
    /// canvas の上で押しているマウスのボタン（0 左・2 右）
    pub fn mouse_down(&self, button: usize) -> bool { self.mouse_down.get(button).copied().unwrap_or(false) }
    /// canvas の上で押した瞬間
    pub fn mouse_pressed(&self, button: usize) -> bool { self.mouse_pressed.get(button).copied().unwrap_or(false) }
    pub fn mouse_released(&self, button: usize) -> bool { self.mouse_released.get(button).copied().unwrap_or(false) }
    /// data-btn のボタン（タッチ）
    pub fn btn(&self, name: &str) -> bool { self.btn_down.contains(name) }
    pub fn btn_pressed(&self, name: &str) -> bool { self.btn_pressed.contains(name) }
    pub fn btn_released(&self, name: &str) -> bool { self.btn_released.contains(name) }
    pub fn any_btn_pressed(&self) -> bool { !self.btn_pressed.is_empty() }
}

/// 押した瞬間を、次に走る固定の刻みまで持ち越す（feel.md「取りこぼさない」）。1 つの刻みにだけ渡す。
/// ```ignore
/// latch.hold(frame.pressed("Space"));
/// step.add(dt);
/// while step.next() { let push = latch.take(); sim.step(step.step, push); }
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct Latch(bool);
impl Latch {
    pub fn hold(&mut self, pressed: bool) { self.0 |= pressed; }
    pub fn take(&mut self) -> bool { std::mem::take(&mut self.0) }
    pub fn peek(&self) -> bool { self.0 }
}

struct State {
    opts: InputOpts,
    rule: DeviceRule,
    frame: Frame,
    stick_id: Option<i32>,
    stick_origin: (f64, f64),
    cam_id: Option<i32>,
    cam_last: (f64, f64),
    pad_reader: PadReader,
    capture: Option<Box<dyn FnOnce(String)>>,
    stick: Option<HtmlElement>,
    knob: Option<HtmlElement>,
}

impl State {
    /// TS の use(d)。出来事で機器が変わるかは決め方の表による
    fn device_event(&mut self, ev: DeviceEvent) {
        let Some(d) = self.rule.next(ev) else { return };
        if d == Device::Touch { self.frame.touch.active = true; }
        if self.frame.device != d {
            self.frame.device = d;
            dom::class_toggle(&dom::body(), "touch", d == Device::Touch);
        }
    }
    fn key(&mut self, e: &KeyboardEvent, is_down: bool) {
        let k = e.code();
        if self.opts.prevent_keys.iter().any(|c| *c == k) { e.prevent_default(); }
        if is_down {
            if !self.frame.keys_down.contains(&k) { self.frame.keys_pressed.insert(k.clone()); }
            self.frame.keys_down.insert(k);
            self.device_event(DeviceEvent::Key);
        } else if self.frame.keys_down.remove(&k) {
            self.frame.keys_released.insert(k);
        }
    }
    fn blur(&mut self) {
        let f = &mut self.frame;
        let downs: Vec<String> = f.keys_down.drain().collect();
        f.keys_released.extend(downs);
        for i in 0..f.mouse_down.len() {
            if f.mouse_down[i] { f.mouse_released[i] = true; }
            f.mouse_down[i] = false;
        }
        f.pointers.clear();
    }
    fn touch_start(&mut self, e: &TouchEvent) {
        self.device_event(DeviceEvent::RootTouch);
        let touches = e.changed_touches();
        for i in 0..touches.length() {
            let Some(t) = touches.item(i) else { continue };
            let target: Option<Element> = t.target().and_then(|x| x.dyn_into().ok());
            if let Some(btn) = target.and_then(|el| dom::closest(&el, "[data-btn]")) {
                if let Some(name) = dom::attr(&btn, "data-btn") {
                    self.frame.btn_down.insert(name.clone());
                    self.frame.btn_pressed.insert(name);
                }
                dom::class_toggle(&btn, "on", true);
                dom::set_attr(&btn, "data-touch", &t.identifier().to_string());
                continue;
            }
            self.free_touch(&t);
        }
        if e.cancelable() { e.prevent_default(); }
    }
    fn free_touch(&mut self, t: &WebTouch) {
        let (x, y) = (t.client_x() as f64, t.client_y() as f64);
        let width = dom::window().inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
        match touch_role(x, width, self.stick_id.is_none(), self.opts.touch_camera, self.cam_id.is_none()) {
            TouchRole::Stick => {
                self.stick_id = Some(t.identifier());
                self.stick_origin = (x, y);
                if let Some(s) = &self.stick {
                    dom::set_style(s, "left", &format!("{x}px"));
                    dom::set_style(s, "top", &format!("{y}px"));
                    dom::class_toggle(s, "on", true);
                }
            }
            TouchRole::Camera => {
                self.cam_id = Some(t.identifier());
                self.cam_last = (x, y);
                self.frame.touch.tap_pressed = true;
            }
            TouchRole::Tap => self.frame.touch.tap_pressed = true,
            TouchRole::Nothing => {}
        }
    }
    fn touch_move(&mut self, e: &TouchEvent) {
        let touches = e.changed_touches();
        for i in 0..touches.length() {
            let Some(t) = touches.item(i) else { continue };
            let (x, y) = (t.client_x() as f64, t.client_y() as f64);
            if Some(t.identifier()) == self.stick_id {
                let v = stick_value(x - self.stick_origin.0, y - self.stick_origin.1, self.opts.stick_radius, self.opts.stick_four_way);
                self.frame.touch.x = v.x;
                self.frame.touch.y = v.y;
                if let Some(k) = &self.knob { dom::set_style(k, "transform", &format!("translate({}px, {}px)", v.knob_x, v.knob_y)); }
            } else if Some(t.identifier()) == self.cam_id {
                self.frame.touch.cam_dx += x - self.cam_last.0;
                self.frame.touch.cam_dy += y - self.cam_last.1;
                self.cam_last = (x, y);
            }
        }
        if e.cancelable() { e.prevent_default(); }
    }
    fn touch_end(&mut self, e: &TouchEvent, root: &HtmlElement) {
        let touches = e.changed_touches();
        for i in 0..touches.length() {
            let Some(t) = touches.item(i) else { continue };
            let id = t.identifier();
            if Some(id) == self.stick_id {
                self.stick_id = None;
                self.frame.touch.x = 0.0;
                self.frame.touch.y = 0.0;
                if let Some(k) = &self.knob { dom::set_style(k, "transform", ""); }
                if let Some(s) = &self.stick { dom::class_toggle(s, "on", false); }
            }
            if Some(id) == self.cam_id { self.cam_id = None; }
            let id_s = id.to_string();
            for btn in dom::query_all_in(root, "[data-btn]") {
                if dom::attr(&btn, "data-touch").as_deref() == Some(&id_s) {
                    dom::class_toggle(&btn, "on", false);
                    if let Some(name) = dom::attr(&btn, "data-btn") {
                        self.frame.btn_down.remove(&name);
                        self.frame.btn_released.insert(name);
                    }
                    dom::remove_attr(&btn, "data-touch");
                }
            }
        }
    }
    fn poll_pad(&mut self) {
        self.frame.pad = None;
        self.frame.pads.clear();
        let Ok(list) = dom::window().navigator().get_gamepads() else { return };
        let connected = list.iter().filter_map(|v| v.dyn_into::<Gamepad>().ok()).map(|gp| {
            let axes: Vec<f64> = gp.axes().iter().map(|a| a.as_f64().unwrap_or(0.0)).collect();
            let buttons: Vec<bool> = gp.buttons().iter().map(|b| b.dyn_into::<GamepadButton>().is_ok_and(|b| b.pressed())).collect();
            (axes, buttons)
        });
        let (pads, used) = self.pad_reader.read(connected);
        if used { self.device_event(DeviceEvent::Pad); }
        self.frame.pad = pads.first().cloned();
        self.frame.pads = pads;
    }
}

/// 入力の受け口（複製しても同じ物を指す）
#[derive(Clone)]
pub struct Input { s: Rc<RefCell<State>> }

impl Input {
    pub fn new(opts: InputOpts) -> Input {
        let root = opts.touch_root.clone();
        let stick = root.as_ref().and_then(|r| dom::query_in(r, ".stick"));
        let knob = root.as_ref().and_then(|r| dom::query_in(r, ".knob"));
        let pointer_ignore = opts.pointer_ignore.clone();
        let prevent_menu = opts.prevent_context_menu;
        let rule = DeviceRule::of(opts.touch_class_follows_device, pointer_ignore.is_some());
        let s = Rc::new(RefCell::new(State {
            opts, rule, frame: Frame::default(), stick_id: None, stick_origin: (0.0, 0.0), cam_id: None, cam_last: (0.0, 0.0),
            pad_reader: PadReader::default(), capture: None, stick, knob,
        }));
        let win = dom::window();

        let k = s.clone();
        dom::listen::<KeyboardEvent>(&win, "keydown", None, false, move |e| {
            // 割り当ての画面: 受け手がまた capture_next_key を呼べるように、借りを返してから渡す
            let capture = k.borrow_mut().capture.take();
            if let Some(f) = capture {
                e.prevent_default();
                f(e.code());
                return;
            }
            k.borrow_mut().key(&e, true);
        });
        let k = s.clone();
        dom::listen::<KeyboardEvent>(&win, "keyup", None, false, move |e| k.borrow_mut().key(&e, false));
        let k = s.clone();
        dom::listen::<web_sys::Event>(&win, "blur", None, false, move |_| k.borrow_mut().blur());

        // マウス: canvas の上で押した物だけ
        if prevent_menu { dom::listen::<web_sys::Event>(&win, "contextmenu", None, false, |e| e.prevent_default()); }
        let k = s.clone();
        dom::listen::<MouseEvent>(&win, "mousedown", None, false, move |e| {
            let on_canvas = e.target().and_then(|t| t.dyn_into::<Element>().ok()).is_some_and(|t| t.tag_name() == "CANVAS");
            if !on_canvas { return; }
            let mut st = k.borrow_mut();
            let b = e.button() as usize;
            if b < 5 {
                st.frame.mouse_pressed[b] = true;
                st.frame.mouse_down[b] = true;
            }
            st.device_event(DeviceEvent::CanvasMouse);
        });
        let k = s.clone();
        dom::listen::<MouseEvent>(&win, "mouseup", None, false, move |e| {
            let mut st = k.borrow_mut();
            let b = e.button() as usize;
            if b < 5 && st.frame.mouse_down[b] {
                st.frame.mouse_down[b] = false;
                st.frame.mouse_released[b] = true;
            }
        });
        let k = s.clone();
        dom::listen::<MouseEvent>(&win, "mousemove", None, false, move |e| {
            let mut st = k.borrow_mut();
            if st.frame.mouse_down.iter().any(|b| *b) {
                st.frame.mouse_dx += e.movement_x() as f64;
                st.frame.mouse_dy += e.movement_y() as f64;
            }
        });

        // タッチ
        let k = s.clone();
        dom::listen::<web_sys::Event>(&win, "touchstart", Some(true), true, move |_| k.borrow_mut().device_event(DeviceEvent::WindowTouch));
        if win.match_media("(pointer: coarse)").ok().flatten().is_some_and(|m| m.matches()) { s.borrow_mut().device_event(DeviceEvent::Coarse); }
        if let Some(root) = root {
            let k = s.clone();
            dom::listen::<TouchEvent>(&root, "touchstart", Some(false), false, move |e| k.borrow_mut().touch_start(&e));
            let k = s.clone();
            dom::listen::<TouchEvent>(&root, "touchmove", Some(false), false, move |e| k.borrow_mut().touch_move(&e));
            for ty in ["touchend", "touchcancel"] {
                let (k, r) = (s.clone(), root.clone());
                dom::listen::<TouchEvent>(&root, ty, None, false, move |e| k.borrow_mut().touch_end(&e, &r));
            }
        }

        // 画面を押す遊び
        if let Some(ignore) = pointer_ignore {
            let k = s.clone();
            dom::listen::<PointerEvent>(&win, "pointerdown", None, false, move |e| {
                let hit = e.target().and_then(|t| t.dyn_into::<Element>().ok()).is_some_and(|t| t.closest(&ignore).ok().flatten().is_some());
                if hit { return; }
                let is_touch = e.pointer_type() == "touch";
                let p = Pointer { id: e.pointer_id(), x: e.client_x() as f64, y: e.client_y() as f64, is_touch };
                let mut st = k.borrow_mut();
                st.device_event(DeviceEvent::Pointer { touch: is_touch });
                st.frame.pointers.push(p);
                st.frame.pointers_pressed.push(p);
            });
            for ty in ["pointerup", "pointercancel"] {
                let k = s.clone();
                dom::listen::<PointerEvent>(&win, ty, None, false, move |e| k.borrow_mut().frame.pointers.retain(|p| p.id != e.pointer_id()));
            }
        }
        Input { s }
    }

    /// このコマの材料を返し、押した瞬間の印を消す。毎コマ 1 回だけ呼ぶ
    pub fn read(&self) -> Frame {
        let mut st = self.s.borrow_mut();
        st.poll_pad();
        let f = &mut st.frame;
        let out = f.clone();
        f.keys_pressed.clear();
        f.keys_released.clear();
        f.mouse_pressed = [false; 5];
        f.mouse_released = [false; 5];
        f.mouse_dx = 0.0;
        f.mouse_dy = 0.0;
        f.btn_pressed.clear();
        f.btn_released.clear();
        f.pointers_pressed.clear();
        f.touch.tap_pressed = false;
        f.touch.cam_dx = 0.0;
        f.touch.cam_dy = 0.0;
        out
    }

    /// 最後に触った機器
    pub fn device(&self) -> Device { self.s.borrow().frame.device }

    /// 次に押されたキーの code を f へ渡し、遊びには流さない（キーの割り当ての画面。keiba）
    pub fn capture_next_key(&self, f: impl FnOnce(String) + 'static) { self.s.borrow_mut().capture = Some(Box::new(f)); }
    pub fn cancel_capture(&self) { self.s.borrow_mut().capture = None; }
}
