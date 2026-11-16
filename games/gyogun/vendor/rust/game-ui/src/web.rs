// ブラウザでは egui-winit が使えないので、窓の出来事を egui の入力へ直す小さな部品（ボタン・ホバー・ホイールが動けばよい）。
// 積み方は raw::PointerInput（winit を使わないゲームと共通）。
use crate::raw::PointerInput;
use egui::{PointerButton, RawInput};
use std::sync::Arc;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, TouchPhase, WindowEvent};
use winit::window::Window;

pub struct WebState {
    inp: PointerInput,
    hand: bool,
}

pub struct Resp { pub consumed: bool }

impl WebState {
    pub fn new(window: &Arc<Window>, max_side: usize) -> Self {
        let _ = window;
        WebState { inp: PointerInput::new(max_side), hand: false }
    }

    pub fn on_window_event(&mut self, window: &Window, e: &WindowEvent) -> Resp {
        let ppp = window.scale_factor() as f32;
        match e {
            WindowEvent::CursorMoved { position, .. } => self.inp.moved(position.x as f32 / ppp, position.y as f32 / ppp),
            WindowEvent::CursorLeft { .. } => self.inp.left(),
            WindowEvent::MouseInput { state, button, .. } => {
                let b = match button { MouseButton::Left => PointerButton::Primary, MouseButton::Right => PointerButton::Secondary, MouseButton::Middle => PointerButton::Middle, _ => return Resp { consumed: false } };
                self.inp.button(b, *state == ElementState::Pressed);
            }
            WindowEvent::MouseWheel { delta, .. } => match delta {
                MouseScrollDelta::LineDelta(x, y) => self.inp.wheel_lines(*x, *y),
                MouseScrollDelta::PixelDelta(p) => self.inp.wheel_points(p.x as f32 / ppp, p.y as f32 / ppp),
            },
            // タッチは指 1 本をポインタとして渡す（札・ボタンを押す）
            WindowEvent::Touch(t) => {
                let (x, y) = (t.location.x as f32 / ppp, t.location.y as f32 / ppp);
                match t.phase {
                    TouchPhase::Started => self.inp.touch_start(t.id, x, y),
                    TouchPhase::Moved => self.inp.touch_move(t.id, x, y),
                    TouchPhase::Ended | TouchPhase::Cancelled => self.inp.touch_end(t.id, x, y),
                }
            }
            WindowEvent::Focused(f) => self.inp.focused(*f),
            _ => {}
        }
        Resp { consumed: false }
    }

    pub fn take_egui_input(&mut self, window: &Window) -> RawInput {
        let ppp = window.scale_factor() as f32;
        let s = window.inner_size();
        self.inp.take(s.width as f32 / ppp, s.height as f32 / ppp, ppp)
    }

    /// 今の版の button { cursor: pointer }: egui が指の形を頼んだら canvas のカーソルへ渡す
    pub fn handle_platform_output(&mut self, window: &Window, o: egui::PlatformOutput) {
        let hand = matches!(o.cursor_icon, egui::CursorIcon::PointingHand);
        if hand != self.hand {
            self.hand = hand;
            window.set_cursor(if hand { winit::window::CursorIcon::Pointer } else { winit::window::CursorIcon::Default });
        }
    }
}
