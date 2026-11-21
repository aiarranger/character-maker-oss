// canvas の指とマウスの出来事を、画面のボタン（egui。raw::PointerInput）と遊びの指（touch_pad::TouchPad）へ分ける（ブラウザ用の共通部品。2026-11-10 iceclimber の dom_input.rs（c9a7cd3）から取り込み）。
// 遊びの指が有効な間（set_pad(true, ..)）は、指は TouchPad へ。ただし egui の押せる面（Area）の上で始まった指は画面のボタンへ渡す
// （iceclimber の右上の「休む」。今の版は #touch が被さって押せなかった）。無効な間は、指 1 本をボタンへ渡す。
// マウスは常にボタンへ。左で押した所が押せる面の上だったかを覚えておく（canvas の mousedown を遊びの操作にしないため）。
use crate::touch_pad::{Button, PadFrame, PadView, Stick, TouchPad};
use egui::{Context, PointerButton, pos2};
use crate::raw::PointerInput;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

struct State {
    ui: PointerInput,
    pad: TouchPad,
    ctx: Context,
    pad_on: bool,
    ui_touch: Option<i32>,
    pad_touches: Vec<i32>,
    mouse_on_ui: bool,
    last_touch: bool,
    used: Option<crate::menu::Used>,
}

impl State {
    fn touch_start(&mut self, id: i32, x: f32, y: f32) {
        if self.pad_on && !(self.ui_touch.is_none() && crate::over_ui(&self.ctx, pos2(x, y))) {
            self.pad_touches.push(id);
            self.pad.start(id as u64, pos2(x, y));
        } else if self.ui_touch.is_none() {
            self.ui_touch = Some(id);
            self.ui.touch_start(id as u64, x, y);
        }
    }
    fn touch_move(&mut self, id: i32, x: f32, y: f32) {
        if self.ui_touch == Some(id) { self.ui.touch_move(id as u64, x, y); }
        else if self.pad_touches.contains(&id) { self.pad.moved(id as u64, pos2(x, y)); }
    }
    fn touch_end(&mut self, id: i32, x: f32, y: f32) {
        if self.ui_touch == Some(id) { self.ui_touch = None; self.ui.touch_end(id as u64, x, y); }
        else if let Some(i) = self.pad_touches.iter().position(|t| *t == id) { self.pad_touches.remove(i); self.pad.end(id as u64); }
    }
}

#[derive(Clone)]
pub struct DomInput(Rc<RefCell<State>>);

impl DomInput {
    /// canvas に出来事を付ける。ctx は画面の egui（押せる面の場所を調べる）
    pub fn new(canvas: &web_sys::HtmlCanvasElement, ctx: &Context, max_side: usize) -> DomInput {
        let me = DomInput(Rc::new(RefCell::new(State { ui: PointerInput::new(max_side), pad: TouchPad::new(), ctx: ctx.clone(), pad_on: false, ui_touch: None, pad_touches: vec![], mouse_on_ui: false, last_touch: false, used: None })));
        let on = |ty: &str, f: Box<dyn FnMut(web_sys::Event)>| {
            let c = Closure::<dyn FnMut(web_sys::Event)>::wrap(f);
            let _ = canvas.add_event_listener_with_callback(ty, c.as_ref().unchecked_ref());
            c.forget();
        };
        // マウス・ペン（指は touch の出来事で受ける）
        for ty in ["pointerdown", "pointermove", "pointerup", "pointercancel"] {
            let s = me.0.clone();
            let kind = ty.to_string();
            on(ty, Box::new(move |e| {
                let e: web_sys::PointerEvent = e.unchecked_into();
                let (x, y) = (e.client_x() as f32, e.client_y() as f32);
                let mut st = s.borrow_mut();
                if kind == "pointerdown" {
                    st.last_touch = e.pointer_type() == "touch";
                    st.used = Some(if st.last_touch { crate::menu::Used::Touch } else { crate::menu::Used::Mouse });
                    // 画面のボタンの上で押した物は window（island-kit の Input）へ届けない。今の版は button が受けて canvas の押下にならない
                    if crate::over_ui(&st.ctx, pos2(x, y)) { e.stop_propagation(); }
                }
                if e.pointer_type() == "touch" { return; }
                st.ui.moved(x, y);
                let b = match e.button() { 0 => Some(PointerButton::Primary), 1 => Some(PointerButton::Middle), 2 => Some(PointerButton::Secondary), _ => None };
                match kind.as_str() {
                    "pointerdown" => {
                        if e.button() == 0 && crate::over_ui(&st.ctx, pos2(x, y)) { st.mouse_on_ui = true; }
                        if let Some(b) = b { st.ui.button(b, true); }
                    }
                    "pointerup" | "pointercancel" => { if let Some(b) = b { st.ui.button(b, false); } }
                    _ => {}
                }
            }));
        }
        {
            // まねでないマウスの mousedown も同じく（island-kit は window の mousedown でマウスの押下・カメラのドラッグを受ける）
            let s = me.0.clone();
            on("mousedown", Box::new(move |e| {
                let e: web_sys::MouseEvent = e.unchecked_into();
                if crate::over_ui(&s.borrow().ctx, pos2(e.client_x() as f32, e.client_y() as f32)) { e.stop_propagation(); }
            }));
        }
        {
            let s = me.0.clone();
            on("pointerleave", Box::new(move |e| {
                let e: web_sys::PointerEvent = e.unchecked_into();
                if e.pointer_type() != "touch" { s.borrow_mut().ui.left(); }
            }));
        }
        {
            let s = me.0.clone();
            on("wheel", Box::new(move |e| {
                let e: web_sys::WheelEvent = e.unchecked_into();
                let mut st = s.borrow_mut();
                // deltaMode 0 は px、1 は行
                if e.delta_mode() == 1 { st.ui.wheel_lines(-e.delta_x() as f32, -e.delta_y() as f32); } else { st.ui.wheel_points(-e.delta_x() as f32, -e.delta_y() as f32); }
            }));
        }
        for ty in ["touchstart", "touchmove", "touchend", "touchcancel"] {
            let s = me.0.clone();
            let kind = ty.to_string();
            on(ty, Box::new(move |e| {
                let e: web_sys::TouchEvent = e.unchecked_into();
                let list = e.changed_touches();
                let mut st = s.borrow_mut();
                for i in 0..list.length() {
                    let Some(t) = list.item(i) else { continue };
                    let (id, x, y) = (t.identifier(), t.client_x() as f32, t.client_y() as f32);
                    match kind.as_str() {
                        "touchstart" => { st.last_touch = true; st.used = Some(crate::menu::Used::Touch); st.touch_start(id, x, y) }
                        "touchmove" => st.touch_move(id, x, y),
                        _ => st.touch_end(id, x, y),
                    }
                }
                // 長押しのメニュー・拡大鏡・まねのマウスの出来事を出さない
                if (kind == "touchstart" || kind == "touchmove") && e.cancelable() { e.prevent_default(); }
            }));
        }
        me
    }
    /// 遊びの指を受けるか（受けない間に触っていた指は、離すまで今の受け手のまま）とボタン・スティックの場所
    pub fn set_pad(&self, on: bool, buttons: Vec<Button>, stick: Option<Stick>) {
        let mut st = self.0.borrow_mut();
        st.pad_on = on;
        st.pad.set_layout(buttons, stick);
    }
    /// ボタンにもスティックにも当たらなかった最初の指の動きを PadFrame::drag で返す（TouchPad::set_drag）
    pub fn set_drag(&self, on: bool) { self.0.borrow_mut().pad.set_drag(on); }
    /// スティックの中心が指に付いてくるか（TouchPad::set_follow。既定は付いてくる）
    pub fn set_follow(&self, on: bool) { self.0.borrow_mut().pad.set_follow(on); }
    /// ボタンの外側の、押したことにする余白（TouchPad::set_slop。既定 10px）
    pub fn set_slop(&self, px: f32) { self.0.borrow_mut().pad.set_slop(px); }
    /// 1 コマ分の egui の入力（w・h は論理 px、ppp は描く先の画素比）
    pub fn take_ui(&self, w: f32, h: f32, ppp: f32) -> egui::RawInput { self.0.borrow_mut().ui.take(w, h, ppp) }
    /// 遊びの指の値（押した・離した瞬間はここで消える。毎コマ 1 回）
    pub fn read_pad(&self) -> PadFrame { self.0.borrow_mut().pad.read() }
    pub fn pad_view(&self) -> PadView { self.0.borrow().pad.view() }
    /// このコマに、マウスの左を画面の押せる面の上で押したか（読むと消える）
    pub fn take_mouse_on_ui(&self) -> bool { std::mem::take(&mut self.0.borrow_mut().mouse_on_ui) }
    /// 最後に触ったのは指か（マウス・ペンなら false）。指とマウスで出す案内を替えるとき
    pub fn last_was_touch(&self) -> bool { self.0.borrow().last_touch }
    /// 今、指が canvas に触れているか（遊びの指・画面のボタンの指のどちらでも）。触っている間ずっと「指」と数えたいゲーム向け
    pub fn touching(&self) -> bool { let st = self.0.borrow(); st.ui_touch.is_some() || !st.pad_touches.is_empty() }
    /// 前に読んでから canvas を押した物（指かマウス。押していなければ None。読むと消える）。menu::Seen::used へ渡す
    pub fn take_used(&self) -> Option<crate::menu::Used> { self.0.borrow_mut().used.take() }
}
