// 入力: キーボード・マウス・ゲームパッド・タッチをまとめる。押した瞬間（Pressed）はフレームごとに1回だけ立つ。
use crate::controls::{Act, Device, Mouse, PAUSE_ID, PAUSE_KEYS, PAUSE_PAD, bind};
use gilrs::{Axis, Button, Gilrs, GilrsBuilder};
use std::collections::HashSet;
use winit::event::{ElementState, MouseButton, Touch, TouchPhase, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    /// 画面の向きの移動（x 右、y 奥）
    pub x: f64,
    pub y: f64,
    /// カメラの回転（-1..1、押している間）
    pub cam: f64,
    /// マウスのドラッグで回した量（rad）
    pub cam_drag: f64,
    pub attack: bool,
    pub heavy: bool,
    pub jump: bool,
    pub dodge: bool,
    pub ranbu: bool,
    /// 飴糸（押した瞬間）
    pub wire: bool,
    pub pause_pressed: bool,
    pub confirm_pressed: bool,
    pub back_pressed: bool,
    pub left_pressed: bool,
    pub right_pressed: bool,
    pub up_pressed: bool,
    pub down_pressed: bool,
    pub any_pressed: bool,
}

/// タッチのボタンの当たり（論理ピクセル。egui の Rect からも作れる）
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TouchRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}
impl TouchRect {
    fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
    /// 縁までの距離（中は 0）
    fn distance(&self, x: f32, y: f32) -> f32 {
        let dx = (self.x0 - x).max(x - self.x1).max(0.0);
        let dy = (self.y0 - y).max(y - self.y1).max(0.0);
        dx.hypot(dy)
    }
}
impl From<egui::Rect> for TouchRect {
    fn from(r: egui::Rect) -> Self {
        Self { x0: r.min.x, y0: r.min.y, x1: r.max.x, y1: r.max.y }
    }
}

/// 仮想スティックの見た目（TS の .stick の位置と .knob の translate。論理ピクセル）
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StickView {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
}

/// 標準マッピング（Gamepad API の buttons の番号）の状態
#[derive(Clone, Copy, Default)]
struct PadState {
    ax: f64,
    ay: f64,
    rx: f64,
    buttons: [bool; 17],
}
/// 標準マッピングの番号 → gilrs のボタン名
const PAD_BUTTONS: [Button; 17] = [
    Button::South,         // 0 A
    Button::East,          // 1 B
    Button::West,          // 2 X
    Button::North,         // 3 Y
    Button::LeftTrigger,   // 4 LB
    Button::RightTrigger,  // 5 RB
    Button::LeftTrigger2,  // 6 LT
    Button::RightTrigger2, // 7 RT
    Button::Select,        // 8
    Button::Start,         // 9
    Button::LeftThumb,     // 10
    Button::RightThumb,    // 11
    Button::DPadUp,        // 12
    Button::DPadDown,      // 13
    Button::DPadLeft,      // 14
    Button::DPadRight,     // 15
    Button::Mode,          // 16
];

/// スティックを倒しきる距離（px）
const R: f64 = 60.0;
/// 指のボタンの外側の、押したことにする余白（px。game-ui 0.3 の touch_pad と同じ）
const SLOP: f32 = 10.0;

pub struct Input {
    down: HashSet<PhysicalKey>,
    pressed: HashSet<PhysicalKey>,
    // マウス: 左 = 通常、右 = 強攻撃、ドラッグ（中ボタンか、押しながら動かす）= カメラ
    mouse_l: bool,
    mouse_r: bool,
    mouse_held: bool,
    drag: f64,
    dragging: bool,
    last_x: f64,
    moved: f64,
    cursor_x: f64,
    over_ui: bool,
    stick_seen: bool,
    /// 最後に触った機器（キーボード・マウス／パッド／指）
    device: Device,
    /// 最後に触ったのがキーボードかパッド（選んでいる項目の輪を出す。マウス・指で触ると消す）
    ring: bool,
    // タッチ: 左の仮想スティック、右のボタン、右半分のスワイプでカメラ
    touch_on: bool,
    touch_x: f64,
    touch_y: f64,
    tap_p: bool,
    pause_p: bool,
    btn: Vec<(&'static str, u64)>,
    btn_p: Vec<&'static str>,
    buttons: Vec<(&'static str, TouchRect)>,
    stick_id: Option<u64>,
    sx: f64,
    sy: f64,
    /// その指が触り始めた所（離したら土台をここへ戻す。付いていった先に残すと、右のボタンの上に土台が残る）
    home: (f64, f64),
    stick_dx: f64,
    stick_dy: f64,
    cam_id: Option<u64>,
    cam_x: f64,
    // 位置はすべて論理ピクセル（TS の clientX と同じ単位）
    scale: f64,
    width: f64,
    height: f64,
    /// 端の安全域 [上, 右, 下, 左]（論理 px）。スティックを始められるのはこの内側
    safe: [f64; 4],
    pad_prev: [bool; 17],
    /// TS の padPrev[100..=103]（スティックを ←→↑↓ へ倒しているか）
    pad_prev_dir: [bool; 4],
    gilrs: Option<Gilrs>,
}

impl Default for Input {
    fn default() -> Self {
        Self::new()
    }
}

impl Input {
    pub fn new() -> Self {
        // 値は Gamepad API と同じく素のまま読む（デッドゾーンは read の式で）。十字キーが軸のパッドは poll_pad で見る
        let gilrs = match GilrsBuilder::new().with_default_filters(false).build() {
            Ok(g) => Some(g),
            Err(gilrs::Error::NotImplemented(g)) => Some(g),
            Err(e) => {
                eprintln!("ゲームパッドを使えない: {e}");
                None
            }
        };
        let mut i = Self::without_gamepad();
        i.gilrs = gilrs;
        // 今の版: matchMedia('(pointer: coarse)') なら最初からタッチの画面（キーの札を隠す）
        #[cfg(target_arch = "wasm32")]
        if web_sys::window().and_then(|w| w.match_media("(pointer: coarse)").ok().flatten()).is_some_and(|m| m.matches()) { i.device = Device::Touch; }
        i
    }

    fn without_gamepad() -> Self {
        Self {
            down: HashSet::new(),
            pressed: HashSet::new(),
            mouse_l: false,
            mouse_r: false,
            mouse_held: false,
            drag: 0.0,
            dragging: false,
            last_x: 0.0,
            moved: 0.0,
            cursor_x: 0.0,
            over_ui: false,
            stick_seen: false,
            device: Device::Key,
            ring: false,
            touch_on: true,
            touch_x: 0.0,
            touch_y: 0.0,
            tap_p: false,
            pause_p: false,
            btn: Vec::with_capacity(8),
            btn_p: Vec::with_capacity(8),
            buttons: Vec::with_capacity(8),
            stick_id: None,
            sx: 0.0,
            sy: 0.0,
            home: (0.0, 0.0),
            stick_dx: 0.0,
            stick_dy: 0.0,
            cam_id: None,
            cam_x: 0.0,
            scale: 1.0,
            width: 0.0,
            height: 0.0,
            safe: [0.0; 4],
            pad_prev: [false; 17],
            pad_prev_dir: [false; 4],
            gilrs: None,
        }
    }

    /// ウィンドウを作ったときに 1 回（scale_factor と inner_size()）。以後は on_event が追う
    pub fn set_viewport(&mut self, scale_factor: f64, physical_width: u32, physical_height: u32) {
        self.scale = scale_factor;
        self.width = physical_width as f64 / scale_factor;
        self.height = physical_height as f64 / scale_factor;
    }

    /// 端の安全域 [上, 右, 下, 左]（論理 px）
    pub fn set_safe(&mut self, safe: [f32; 4]) {
        self.safe = safe.map(|v| v as f64);
    }

    /// 最後に触った機器
    pub fn device(&self) -> Device {
        self.device
    }

    /// 選んでいる項目の輪を出すか（最後に触ったのがキーボードかパッド）
    pub fn ring(&self) -> bool {
        self.ring
    }

    fn touched(&mut self, device: Device, ring: bool) {
        self.device = device;
        self.ring = ring;
    }

    /// 最後に触った機器を覚える。UI が受けた出来事（画面のボタンの上のクリック・指）も含めて、窓の出来事を全部渡す
    pub fn note_device(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, is_synthetic: false, .. } if event.state == ElementState::Pressed && !event.repeat => self.touched(Device::Key, true),
            WindowEvent::MouseInput { state: ElementState::Pressed, .. } => self.touched(Device::Key, false),
            WindowEvent::Touch(Touch { phase: TouchPhase::Started, .. }) => self.touched(Device::Touch, false),
            _ => {}
        }
    }

    /// UI の上にポインタがあるか（egui の wants_pointer_input など）。true の間はマウスの押下とスティック・スワイプの始まりを拾わない
    pub fn set_pointer_over_ui(&mut self, over: bool) {
        self.over_ui = over;
    }

    /// タッチのボタンの当たり。hud が描く位置を毎コマ渡してよい（後ろの要素ほど手前）
    pub fn set_touch_buttons<T: Into<TouchRect> + Copy>(&mut self, buttons: &[(&'static str, T)]) {
        self.buttons.clear();
        self.buttons.extend(buttons.iter().map(|(n, r)| (*n, (*r).into())));
    }

    /// TS の `$('touch').classList.add/remove('on')`。false の間は新しいタッチを操作に取らない（既定は true）
    pub fn set_touch_on(&mut self, on: bool) {
        self.touch_on = on;
    }

    /// タッチで遊んでいるか（TS の body.touch）。最後に触ったのが指
    pub fn touch(&self) -> bool {
        self.device == Device::Touch
    }

    /// 仮想スティックの見た目（一度も触っていなければ None。離すと、最後に触り始めた場所に残る。倒している間は指に付いてくる）
    pub fn stick_view(&self) -> Option<StickView> {
        self.stick_seen.then(|| StickView { x: self.sx as f32, y: self.sy as f32, dx: self.stick_dx as f32, dy: self.stick_dy as f32 })
    }

    /// タッチのボタンを押しているか（TS の .on）
    pub fn touch_button_on(&self, name: &str) -> bool {
        self.btn.iter().any(|(n, _)| *n == name)
    }

    pub fn on_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, is_synthetic, .. } => {
                // フォーカスが戻ったときの合成イベントは、ブラウザの keydown には無いので取らない
                if !*is_synthetic {
                    self.key(event.physical_key, event.state == ElementState::Pressed);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => self.mouse_button(*button, *state == ElementState::Pressed),
            WindowEvent::CursorMoved { position, .. } => self.cursor_moved(position.x / self.scale),
            WindowEvent::Touch(Touch { id, phase, location, .. }) => self.touch_event(*id, *phase, location.x / self.scale, location.y / self.scale),
            WindowEvent::Focused(false) => self.release_all(),
            WindowEvent::Resized(size) => {
                self.width = size.width as f64 / self.scale;
                self.height = size.height as f64 / self.scale;
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.width = self.width * self.scale / *scale_factor;
                self.height = self.height * self.scale / *scale_factor;
                self.scale = *scale_factor;
            }
            _ => {}
        }
    }

    fn key(&mut self, k: PhysicalKey, is_down: bool) {
        if is_down {
            if !self.down.contains(&k) {
                self.pressed.insert(k);
                self.touched(Device::Key, true);
            }
            self.down.insert(k);
        } else {
            self.down.remove(&k);
        }
    }

    fn mouse_button(&mut self, button: MouseButton, is_down: bool) {
        if is_down {
            self.touched(Device::Key, false);
            if self.over_ui {
                return;
            }
            if button == MouseButton::Left {
                self.mouse_l = true;
                self.mouse_held = true;
            }
            if button == MouseButton::Right {
                self.mouse_r = true;
            }
            self.dragging = true;
            self.last_x = self.cursor_x;
            self.moved = 0.0;
        } else {
            self.dragging = false;
            if button == MouseButton::Left {
                self.mouse_held = false;
            }
        }
    }

    fn cursor_moved(&mut self, x: f64) {
        self.cursor_x = x;
        if !self.dragging {
            return;
        }
        let dx = x - self.last_x;
        self.last_x = x;
        self.moved += dx.abs();
        if self.moved > 12.0 {
            self.drag += dx * 0.006;
        }
    }

    fn touch_event(&mut self, id: u64, phase: TouchPhase, x: f64, y: f64) {
        match phase {
            TouchPhase::Started => {
                self.touched(Device::Touch, false);
                if !self.touch_on {
                    return;
                }
                if let Some(b) = self.button_at(x as f32, y as f32) {
                    if b == PAUSE_ID {
                        self.pause_p = true;
                    }
                    // 同じボタンに 2 本目が乗ったら、離す判定は後の指で（TS の dataset.touch と同じ）
                    match self.btn.iter_mut().find(|(n, _)| *n == b) {
                        Some(e) => e.1 = id,
                        None => self.btn.push((b, id)),
                    }
                    self.btn_p.push(b);
                    return;
                }
                if self.over_ui {
                    return;
                }
                if self.stick_id.is_none() && self.in_stick_area(x, y) {
                    self.stick_id = Some(id);
                    self.stick_seen = true;
                    self.sx = x;
                    self.sy = y;
                    self.home = (x, y);
                    self.stick_dx = 0.0;
                    self.stick_dy = 0.0;
                } else if self.cam_id.is_none() {
                    self.cam_id = Some(id);
                    self.cam_x = x;
                    self.tap_p = true;
                }
            }
            TouchPhase::Moved => {
                if Some(id) == self.stick_id {
                    let (mut dx, mut dy) = (x - self.sx, y - self.sy);
                    let d = dx.hypot(dy);
                    if d > R {
                        dx *= R / d;
                        dy *= R / d;
                        // 倒しきった先へ指が進んだら、中心を指から倒しきる距離の所まで寄せる（戻すとすぐ値が減る。画面の端で始めても倒しきれる）
                        self.sx = x - dx;
                        self.sy = y - dy;
                    }
                    self.touch_x = dx / R;
                    self.touch_y = -dy / R;
                    self.stick_dx = dx;
                    self.stick_dy = dy;
                } else if Some(id) == self.cam_id {
                    self.drag += (x - self.cam_x) * 0.008;
                    self.cam_x = x;
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if Some(id) == self.stick_id {
                    self.release_stick();
                }
                if Some(id) == self.cam_id {
                    self.cam_id = None;
                }
                self.btn.retain(|(_, i)| *i != id);
            }
        }
    }

    /// 指が触れた所のボタン。中に入っているボタンが先（後ろの要素ほど手前）。無ければ、外側の余白の中でいちばん近いボタン
    fn button_at(&self, x: f32, y: f32) -> Option<&'static str> {
        if let Some(&(b, _)) = self.buttons.iter().rev().find(|(_, r)| r.contains(x, y)) {
            return Some(b);
        }
        self.buttons.iter().map(|(b, r)| (*b, r.distance(x, y))).filter(|(_, d)| *d <= SLOP).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(b, _)| b)
    }

    /// スティックを始められる場所: 左半分の、端の安全域の内側
    fn in_stick_area(&self, x: f64, y: f64) -> bool {
        let [top, _, bottom, left] = self.safe;
        x < self.width * 0.5 && x >= left && y >= top && (self.height <= 0.0 || y <= self.height - bottom)
    }

    /// スティックの指を離した: 値を 0 にして、土台を触り始めた所へ戻す
    fn release_stick(&mut self) {
        if self.stick_id.take().is_some() {
            (self.sx, self.sy) = self.home;
        }
        self.touch_x = 0.0;
        self.touch_y = 0.0;
        self.stick_dx = 0.0;
        self.stick_dy = 0.0;
    }

    /// ウィンドウがフォーカスを失った: 押しているものを全部離す
    fn release_all(&mut self) {
        self.down.clear();
        self.mouse_held = false;
        self.dragging = false;
        self.release_stick();
        self.cam_id = None;
        self.btn.clear();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn poll_pad(&mut self) -> Option<PadState> {
        let g = self.gilrs.as_mut()?;
        while g.next_event().is_some() {}
        let (_, pad) = g.gamepads().next()?;
        let mut s = PadState { ax: pad.value(Axis::LeftStickX) as f64, ay: -pad.value(Axis::LeftStickY) as f64, rx: pad.value(Axis::RightStickX) as f64, ..Default::default() };
        for (i, b) in PAD_BUTTONS.iter().enumerate() {
            // LT・RT はアナログ。Gamepad API の pressed と同じく少し押せば押した扱い
            s.buttons[i] = pad.is_pressed(*b) || (matches!(i, 6 | 7) && pad.button_data(*b).is_some_and(|d| d.value() > 0.12));
        }
        // 十字キーを軸で返すパッド（ハットスイッチ）
        let (dx, dy) = (pad.value(Axis::DPadX), pad.value(Axis::DPadY));
        s.buttons[12] |= dy > 0.5;
        s.buttons[13] |= dy < -0.5;
        s.buttons[14] |= dx < -0.5;
        s.buttons[15] |= dx > 0.5;
        Some(s)
    }

    #[cfg(target_arch = "wasm32")]
    fn poll_pad(&mut self) -> Option<PadState> {
        use wasm_bindgen::JsCast;
        // Browser axes are doubles. gilrs narrows them to f32, which changes
        // camera motion and eventually world positions relative to the TS game.
        let list = web_sys::window()?.navigator().get_gamepads().ok()?;
        let pad = list.iter().find_map(|value| value.dyn_into::<web_sys::Gamepad>().ok())?;
        let axes = pad.axes();
        let axis = |i| axes.get(i).as_f64().unwrap_or(0.0);
        let mut state = PadState { ax: axis(0), ay: axis(1), rx: axis(2), ..Default::default() };
        let buttons = pad.buttons();
        for (i, down) in state.buttons.iter_mut().enumerate() {
            *down = buttons.get(i as u32).dyn_into::<web_sys::GamepadButton>().is_ok_and(|button| button.pressed());
        }
        Some(state)
    }

    /// 1 コマに 1 回呼ぶ。押した瞬間のものはここで消える
    pub fn read(&mut self) -> Controls {
        let pad = self.poll_pad();
        self.read_with(pad)
    }

    fn read_with(&mut self, pad: Option<PadState>) -> Controls {
        use KeyCode::*;
        let d = &self.down;
        let pr = &self.pressed;
        let k = |c: KeyCode| d.contains(&PhysicalKey::Code(c));
        let p = |c: KeyCode| pr.contains(&PhysicalKey::Code(c));
        let f = |b: bool| if b { 1.0f64 } else { 0.0 };
        let mut x = f(k(KeyD)) - f(k(KeyA));
        let mut y = f(k(KeyW)) - f(k(KeyS));
        let mut cam = f(k(ArrowRight) || k(KeyE)) - f(k(ArrowLeft) || k(KeyQ));
        // 攻撃・強・跳・避・乱舞のキーとボタンは割り当ての表（controls.rs）から
        let key_p = |a: Act| bind(a).keys.iter().any(|c| p(*c));
        let key_d = |a: Act| bind(a).keys.iter().any(|c| k(*c));
        let mouse_p = |a: Act| match bind(a).mouse { Some(Mouse::Left) => self.mouse_l, Some(Mouse::Right) => self.mouse_r, None => false };
        // 押し続けると、通常攻撃をつなぎ続ける
        let mut attack = key_p(Act::Attack) || mouse_p(Act::Attack) || key_d(Act::Attack) || (self.mouse_held && self.moved < 12.0);
        let mut heavy = key_p(Act::Heavy) || mouse_p(Act::Heavy);
        let mut jump = key_p(Act::Jump);
        let mut dodge = key_p(Act::Dodge);
        let mut ranbu = key_p(Act::Ranbu);
        let mut wire = key_p(Act::Wire);
        let mut pause_pressed = PAUSE_KEYS.iter().any(|c| p(*c));
        let mut confirm_pressed = p(Enter) || p(Space) || p(KeyJ);
        let mut back_pressed = p(Escape) || p(Backspace);
        let mut left_pressed = p(ArrowLeft) || p(KeyA);
        let mut right_pressed = p(ArrowRight) || p(KeyD);
        let mut up_pressed = p(ArrowUp) || p(KeyW);
        let mut down_pressed = p(ArrowDown) || p(KeyS);
        let mut any_pressed = !pr.is_empty() || self.mouse_l;
        if let Some(s) = pad {
            let (ax, ay, rx) = (s.ax, s.ay, s.rx);
            if ax.hypot(ay) > 0.2 {
                x = ax;
                y = -ay;
            }
            if rx.abs() > 0.2 {
                cam = rx;
            }
            let b = |i: usize| s.buttons[i];
            if b(12) { y = 1.0; }
            if b(13) { y = -1.0; }
            if b(14) { x = -1.0; }
            if b(15) { x = 1.0; }
            let prev = self.pad_prev;
            let dir = self.pad_prev_dir;
            let edge = |i: usize| b(i) && !prev[i];
            let pad_p = |a: Act| bind(a).pad.iter().any(|i| edge(*i));
            attack = attack || bind(Act::Attack).pad.iter().any(|i| b(*i));
            heavy = heavy || pad_p(Act::Heavy);
            jump = jump || pad_p(Act::Jump);
            dodge = dodge || pad_p(Act::Dodge);
            ranbu = ranbu || pad_p(Act::Ranbu);
            wire = wire || pad_p(Act::Wire);
            pause_pressed = pause_pressed || edge(PAUSE_PAD);
            confirm_pressed = confirm_pressed || edge(0) || edge(9);
            back_pressed = back_pressed || edge(1);
            left_pressed = left_pressed || edge(14) || (ax < -0.6 && !dir[0]);
            right_pressed = right_pressed || edge(15) || (ax > 0.6 && !dir[1]);
            up_pressed = up_pressed || edge(12) || (ay < -0.6 && !dir[2]);
            down_pressed = down_pressed || edge(13) || (ay > 0.6 && !dir[3]);
            any_pressed = any_pressed || (0..17).any(edge);
            // パッドに触った: ボタンを押した瞬間か、スティックをはっきり倒したとき（少しの揺れでは替えない）
            if (0..17).any(edge) || ax.hypot(ay) > 0.5 || rx.abs() > 0.5 {
                self.device = Device::Pad;
                self.ring = true;
            }
            self.pad_prev = s.buttons;
            self.pad_prev_dir = [ax < -0.6, ax > 0.6, ay < -0.6, ay > 0.6];
        }
        {
            if self.touch_x.hypot(self.touch_y) > 0.15 {
                x = self.touch_x;
                y = self.touch_y;
            }
            let tp = |a: Act| self.btn_p.contains(&bind(a).id);
            attack = attack || tp(Act::Attack) || self.btn.iter().any(|(n, _)| *n == bind(Act::Attack).id);
            heavy = heavy || tp(Act::Heavy);
            jump = jump || tp(Act::Jump);
            dodge = dodge || tp(Act::Dodge);
            ranbu = ranbu || tp(Act::Ranbu);
            wire = wire || tp(Act::Wire);
            any_pressed = any_pressed || self.tap_p || !self.btn_p.is_empty();
            confirm_pressed = confirm_pressed || self.tap_p;
            pause_pressed = pause_pressed || self.pause_p;
            self.btn_p.clear();
            self.tap_p = false;
            self.pause_p = false;
        }
        self.pressed.clear();
        self.mouse_l = false;
        self.mouse_r = false;
        let cam_drag = self.drag;
        self.drag = 0.0;
        let len = x.hypot(y);
        if len > 1.0 {
            x /= len;
            y /= len;
        }
        Controls { x, y, cam, cam_drag, attack, heavy, jump, dodge, ranbu, wire, pause_pressed, confirm_pressed, back_pressed, left_pressed, right_pressed, up_pressed, down_pressed, any_pressed }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controls::BINDS;

    fn key(c: KeyCode) -> PhysicalKey {
        PhysicalKey::Code(c)
    }
    fn input() -> Input {
        let mut i = Input::without_gamepad();
        i.set_viewport(2.0, 1600, 1200); // 論理 800×600
        i
    }
    fn pad(f: impl FnOnce(&mut PadState)) -> Option<PadState> {
        let mut s = PadState::default();
        f(&mut s);
        Some(s)
    }

    #[test]
    fn 押した瞬間は1コマに1回だけ立つ() {
        let mut i = input();
        i.key(key(KeyCode::Space), true);
        let c = i.read_with(None);
        assert!(c.jump && c.confirm_pressed && c.any_pressed);
        // キーリピート（離さずに来る keydown）では立たない
        i.key(key(KeyCode::Space), true);
        let c = i.read_with(None);
        assert!(!c.jump && !c.confirm_pressed && !c.any_pressed);
        i.key(key(KeyCode::Space), false);
        i.key(key(KeyCode::Space), true);
        assert!(i.read_with(None).jump);
    }

    #[test]
    fn 同じコマで押して離しても拾う() {
        let mut i = input();
        i.key(key(KeyCode::KeyK), true);
        i.key(key(KeyCode::KeyK), false);
        assert!(i.read_with(None).heavy);
        assert!(!i.read_with(None).heavy);
    }

    #[test]
    fn 斜めの移動は長さ1に丸める() {
        let mut i = input();
        i.key(key(KeyCode::KeyW), true);
        i.key(key(KeyCode::KeyD), true);
        let c = i.read_with(None);
        assert!((c.x - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
        assert!((c.y - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12);
    }

    #[test]
    fn 攻撃は押し続けるとつながる() {
        let mut i = input();
        i.key(key(KeyCode::KeyJ), true);
        assert!(i.read_with(None).attack);
        assert!(i.read_with(None).attack);
        i.key(key(KeyCode::KeyJ), false);
        assert!(!i.read_with(None).attack);
    }

    #[test]
    fn 右の矢印でカメラが回り右が押される() {
        let mut i = input();
        i.key(key(KeyCode::ArrowRight), true);
        let c = i.read_with(None);
        assert_eq!(c.cam, 1.0);
        assert!(c.right_pressed);
    }

    #[test]
    fn マウスの左は押すと攻撃で12px動かすとカメラになる() {
        let mut i = input();
        i.cursor_moved(100.0);
        i.mouse_button(MouseButton::Left, true);
        let c = i.read_with(None);
        assert!(c.attack && c.any_pressed);
        assert!(i.read_with(None).attack); // 押している間つながる
        i.cursor_moved(110.0); // 累計 10px: まだ回らない
        let c = i.read_with(None);
        assert_eq!(c.cam_drag, 0.0);
        assert!(c.attack);
        i.cursor_moved(120.0); // 累計 20px: この動き（10px）から回る
        let c = i.read_with(None);
        assert!((c.cam_drag - 10.0 * 0.006).abs() < 1e-6);
        assert!(!c.attack);
        assert_eq!(i.read_with(None).cam_drag, 0.0);
        i.mouse_button(MouseButton::Left, false);
        i.cursor_moved(200.0);
        assert_eq!(i.read_with(None).cam_drag, 0.0);
    }

    #[test]
    fn 右は強攻撃で1回だけ() {
        let mut i = input();
        i.mouse_button(MouseButton::Right, true);
        assert!(i.read_with(None).heavy);
        assert!(!i.read_with(None).heavy);
    }

    #[test]
    fn uiの上の押下は拾わない() {
        let mut i = input();
        i.set_pointer_over_ui(true);
        i.mouse_button(MouseButton::Left, true);
        i.cursor_moved(500.0);
        let c = i.read_with(None);
        assert!(!c.attack && !c.any_pressed && c.cam_drag == 0.0);
        // 離すのは UI の上でも効く
        i.set_pointer_over_ui(false);
        i.mouse_button(MouseButton::Left, true);
        i.set_pointer_over_ui(true);
        i.mouse_button(MouseButton::Left, false);
        i.read_with(None);
        assert!(!i.read_with(None).attack);
    }

    #[test]
    fn フォーカスを失うと全部離す() {
        let mut i = input();
        i.key(key(KeyCode::KeyW), true);
        i.mouse_button(MouseButton::Left, true);
        i.touch_event(1, TouchPhase::Started, 100.0, 100.0);
        i.touch_event(1, TouchPhase::Moved, 100.0, 40.0);
        i.set_touch_buttons(&[("attack", TouchRect { x0: 700.0, y0: 0.0, x1: 800.0, y1: 100.0 })]);
        i.touch_event(2, TouchPhase::Started, 750.0, 50.0);
        i.read_with(None);
        i.on_event(&WindowEvent::Focused(false));
        let c = i.read_with(None);
        assert_eq!((c.x, c.y), (0.0, 0.0));
        assert!(!c.attack);
        assert_eq!(i.stick_view().map(|v| (v.dx, v.dy)), Some((0.0, 0.0)));
        assert!(!i.touch_button_on("attack"));
    }

    #[test]
    fn パッドのボタンは押した瞬間だけ() {
        let mut i = input();
        let a = pad(|s| s.buttons[0] = true);
        let c = i.read_with(a);
        assert!(c.jump && c.confirm_pressed && c.any_pressed);
        let c = i.read_with(a);
        assert!(!c.jump && !c.confirm_pressed && !c.any_pressed);
        i.read_with(pad(|_| {}));
        assert!(i.read_with(a).jump);
    }

    #[test]
    fn パッドのxは押している間ずっと攻撃() {
        let mut i = input();
        let x = pad(|s| s.buttons[2] = true);
        assert!(i.read_with(x).attack);
        assert!(i.read_with(x).attack);
    }

    #[test]
    fn パッドの番号とtsの対応() {
        let mut i = input();
        assert!(i.read_with(pad(|s| s.buttons[3] = true)).heavy);
        i.read_with(pad(|_| {}));
        assert!(i.read_with(pad(|s| s.buttons[1] = true)).dodge);
        i.read_with(pad(|_| {}));
        assert!(i.read_with(pad(|s| s.buttons[1] = true)).back_pressed);
        i.read_with(pad(|_| {}));
        assert!(i.read_with(pad(|s| s.buttons[4] = true)).dodge);
        i.read_with(pad(|_| {}));
        assert!(i.read_with(pad(|s| s.buttons[5] = true)).ranbu);
        i.read_with(pad(|_| {}));
        assert!(i.read_with(pad(|s| s.buttons[7] = true)).ranbu);
        i.read_with(pad(|_| {}));
        assert!(i.read_with(pad(|s| s.buttons[9] = true)).pause_pressed);
        // どれでも any。ボタン 16 も
        i.read_with(pad(|_| {}));
        assert!(i.read_with(pad(|s| s.buttons[16] = true)).any_pressed);
    }

    #[test]
    fn パッドのスティック() {
        let mut i = input();
        // 0.2 以下は無視
        let c = i.read_with(pad(|s| { s.ax = 0.15; s.ay = 0.1; s.rx = 0.2; }));
        assert_eq!((c.x, c.y, c.cam), (0.0, 0.0, 0.0));
        // y は上が正へ反転、右スティックはカメラ
        let c = i.read_with(pad(|s| { s.ax = 0.3; s.ay = -0.4; s.rx = -0.5; }));
        assert!((c.x - 0.3).abs() < 1e-6 && (c.y - 0.4).abs() < 1e-6);
        assert_eq!(c.cam, -0.5);
        // 長さ 1 を超えたら丸める
        let c = i.read_with(pad(|s| { s.ax = 1.0; s.ay = -1.0; }));
        assert!((c.x.hypot(c.y) - 1.0).abs() < 1e-6);
        // 十字キーはスティックより強い
        let c = i.read_with(pad(|s| { s.ax = 0.5; s.buttons[14] = true; }));
        assert_eq!(c.x, -1.0);
    }

    #[test]
    fn パッドのスティックを倒した瞬間だけ方向が立つ() {
        let mut i = input();
        let l = pad(|s| s.ax = -0.7);
        assert!(i.read_with(l).left_pressed);
        assert!(!i.read_with(l).left_pressed);
        i.read_with(pad(|_| {}));
        assert!(i.read_with(l).left_pressed);
        assert!(i.read_with(pad(|s| s.ay = 0.7)).down_pressed);
    }

    #[test]
    fn 左半分のタッチはスティックで半径60で丸める() {
        let mut i = input();
        i.touch_event(1, TouchPhase::Started, 100.0, 300.0);
        assert!(i.touch());
        i.touch_event(1, TouchPhase::Moved, 100.0 + 60.0, 300.0);
        let c = i.read_with(None);
        assert_eq!((c.x, c.y), (1.0, 0.0));
        // 倒しきる距離の中では、中心は動かない
        i.touch_event(1, TouchPhase::Moved, 100.0 + 30.0, 300.0 - 40.0);
        let c = i.read_with(None);
        assert!((c.x - 0.5).abs() < 1e-6 && (c.y - 40.0 / 60.0).abs() < 1e-6);
        let v = i.stick_view().unwrap();
        assert_eq!((v.x, v.y, v.dx, v.dy), (100.0, 300.0, 30.0, -40.0));
        // 0.15 以下は無視
        i.touch_event(1, TouchPhase::Moved, 100.0 + 6.0, 300.0);
        assert_eq!(i.read_with(None).x, 0.0);
        i.touch_event(1, TouchPhase::Ended, 0.0, 0.0);
        let v = i.stick_view().unwrap();
        assert_eq!((v.x, v.y, v.dx, v.dy), (100.0, 300.0, 0.0, 0.0));
    }

    /// 倒しきった先へ指が進むと、中心が付いてくる（戻すとすぐ値が減る。game-ui 0.3 の touch_pad と同じ動き）
    #[test]
    fn スティックの中心は指に付いてくる() {
        let mut i = input();
        i.touch_event(1, TouchPhase::Started, 100.0, 300.0);
        i.touch_event(1, TouchPhase::Moved, 300.0, 300.0);
        assert_eq!(i.read_with(None).x, 1.0);
        let v = i.stick_view().unwrap();
        assert_eq!((v.x, v.y, v.dx), (240.0, 300.0, 60.0));
        // 30px 戻すと、すぐ半分になる（付いてこないと 1.0 のまま）
        i.touch_event(1, TouchPhase::Moved, 270.0, 300.0);
        assert_eq!(i.read_with(None).x, 0.5);
        i.touch_event(1, TouchPhase::Ended, 270.0, 300.0);
        // 画面の左端で始めて右へ倒す: 中心は動かない
        i.touch_event(2, TouchPhase::Started, 4.0, 300.0);
        i.touch_event(2, TouchPhase::Moved, 64.0, 300.0);
        assert_eq!(i.read_with(None).x, 1.0);
        assert_eq!(i.stick_view().unwrap().x, 4.0);
    }

    /// 離したら、土台は触り始めた所へ戻る（付いていった先に残すと、右のボタンの上に土台が残る）
    #[test]
    fn 離すと土台は触り始めた所へ戻る() {
        let mut i = input();
        i.touch_event(1, TouchPhase::Started, 100.0, 300.0);
        i.touch_event(1, TouchPhase::Moved, 300.0, 300.0);
        assert_eq!(i.stick_view().unwrap().x, 240.0);
        i.touch_event(1, TouchPhase::Ended, 300.0, 300.0);
        let v = i.stick_view().unwrap();
        assert_eq!((v.x, v.y, v.dx, v.dy), (100.0, 300.0, 0.0, 0.0));
        // 取り消し・フォーカスを失ったときも同じ
        i.touch_event(2, TouchPhase::Started, 120.0, 320.0);
        i.touch_event(2, TouchPhase::Moved, 120.0, 100.0);
        i.touch_event(2, TouchPhase::Cancelled, 120.0, 100.0);
        assert_eq!((i.stick_view().unwrap().x, i.stick_view().unwrap().y), (120.0, 320.0));
        i.touch_event(3, TouchPhase::Started, 80.0, 400.0);
        i.touch_event(3, TouchPhase::Moved, 380.0, 400.0);
        i.on_event(&WindowEvent::Focused(false));
        assert_eq!((i.stick_view().unwrap().x, i.stick_view().unwrap().y), (80.0, 400.0));
    }

    /// ボタンの少し外（10px まで）も押せる。2 つに近いときは近い方。中に入っているボタンが先
    #[test]
    fn ボタンは外側10pxまで押せる() {
        let mut i = input();
        let r = |x0: f32| TouchRect { x0, y0: 400.0, x1: x0 + 80.0, y1: 480.0 };
        i.set_touch_buttons(&[("attack", r(600.0)), ("heavy", r(692.0))]);
        // 上の縁の 8px 外
        i.touch_event(1, TouchPhase::Started, 640.0, 392.0);
        let c = i.read_with(None);
        assert!(c.attack && !c.confirm_pressed, "縁の 8px 外は押したことにする");
        i.touch_event(1, TouchPhase::Ended, 640.0, 392.0);
        i.read_with(None);
        // 12px 外は押さない（カメラの指・叩いた扱い）
        i.touch_event(2, TouchPhase::Started, 640.0, 388.0);
        let c = i.read_with(None);
        assert!(!c.attack && c.confirm_pressed);
        i.touch_event(2, TouchPhase::Ended, 640.0, 388.0);
        // attack の縁から 7px・heavy の縁から 5px → heavy
        i.touch_event(3, TouchPhase::Started, 687.0, 440.0);
        let c = i.read_with(None);
        assert!(c.heavy && !c.attack);
    }

    /// スティックを始められるのは、端の安全域の内側（ノッチの下では始めない）
    #[test]
    fn スティックは安全域の内側で始める() {
        let mut i = input();
        i.set_safe([0.0, 47.0, 21.0, 47.0]);
        i.touch_event(1, TouchPhase::Started, 20.0, 300.0);
        assert!(i.stick_view().is_none());
        i.touch_event(1, TouchPhase::Ended, 20.0, 300.0);
        i.touch_event(2, TouchPhase::Started, 100.0, 590.0);
        assert!(i.stick_view().is_none(), "ホームバーの帯（下 21px）");
        i.touch_event(2, TouchPhase::Ended, 100.0, 590.0);
        i.touch_event(3, TouchPhase::Started, 100.0, 300.0);
        assert!(i.stick_view().is_some());
    }

    /// 最後に触った機器を追う（指 → キー → パッド → マウス）。輪はキーボードとパッドのときだけ
    #[test]
    fn 最後に触った機器を追う() {
        let mut i = input();
        assert_eq!((i.device(), i.ring()), (Device::Key, false));
        i.touch_event(1, TouchPhase::Started, 600.0, 300.0);
        assert_eq!((i.device(), i.touch(), i.ring()), (Device::Touch, true, false));
        // 指に触ったあとでキーを押すと、キーに戻る
        i.key(key(KeyCode::KeyW), true);
        assert_eq!((i.device(), i.touch(), i.ring()), (Device::Key, false, true));
        // パッド: ボタンを押した瞬間
        i.read_with(pad(|s| s.buttons[0] = true));
        assert_eq!((i.device(), i.ring()), (Device::Pad, true));
        // マウスで押すと輪は消える
        i.mouse_button(MouseButton::Left, true);
        assert_eq!((i.device(), i.ring()), (Device::Key, false));
        // スティックの少しの揺れ（0.3）では替えない。はっきり倒すと替わる
        i.read_with(pad(|s| s.ax = 0.3));
        assert_eq!(i.device(), Device::Key);
        i.read_with(pad(|s| s.ax = 0.8));
        assert_eq!(i.device(), Device::Pad);
        i.touch_event(2, TouchPhase::Started, 600.0, 300.0);
        assert_eq!(i.device(), Device::Touch);
    }

    /// 割り当ての表の先頭のキー・パッドのボタン・指のボタンが、その操作を出す（表と入力がずれない）
    #[test]
    fn 割り当ての表のとおりに出る() {
        let got = |c: &Controls, a: Act| match a { Act::Attack => c.attack, Act::Heavy => c.heavy, Act::Jump => c.jump, Act::Dodge => c.dodge, Act::Ranbu => c.ranbu, Act::Wire => c.wire };
        for b in &BINDS {
            for code in b.keys {
                let mut i = input();
                i.key(key(*code), true);
                assert!(got(&i.read_with(None), b.act), "{:?} {code:?}", b.act);
            }
            for n in b.pad {
                let mut i = input();
                assert!(got(&i.read_with(pad(|s| s.buttons[*n] = true)), b.act), "{:?} pad {n}", b.act);
            }
            let mut i = input();
            i.set_touch_buttons(&[(b.id, TouchRect { x0: 700.0, y0: 0.0, x1: 760.0, y1: 60.0 })]);
            i.touch_event(1, TouchPhase::Started, 730.0, 30.0);
            assert!(got(&i.read_with(None), b.act), "{:?} 指", b.act);
        }
    }

    #[test]
    fn 右半分のタッチはタップとスワイプのカメラ() {
        let mut i = input();
        i.touch_event(5, TouchPhase::Started, 600.0, 300.0);
        let c = i.read_with(None);
        assert!(c.confirm_pressed && c.any_pressed);
        assert!(!i.read_with(None).confirm_pressed);
        i.touch_event(5, TouchPhase::Moved, 650.0, 300.0);
        assert!((i.read_with(None).cam_drag - 50.0 * 0.008).abs() < 1e-6);
        i.touch_event(5, TouchPhase::Cancelled, 650.0, 300.0);
        i.touch_event(5, TouchPhase::Moved, 700.0, 300.0);
        assert_eq!(i.read_with(None).cam_drag, 0.0);
    }

    #[test]
    fn タッチのボタン() {
        let mut i = input();
        let r = |x0: f32| TouchRect { x0, y0: 0.0, x1: x0 + 50.0, y1: 50.0 };
        i.set_touch_buttons(&[("attack", r(600.0)), ("heavy", r(660.0)), ("pause", r(720.0))]);
        i.touch_event(1, TouchPhase::Started, 610.0, 10.0);
        let c = i.read_with(None);
        assert!(c.attack && c.any_pressed && !c.confirm_pressed);
        // 斬は押している間つながる
        let c = i.read_with(None);
        assert!(c.attack && !c.any_pressed);
        assert!(i.touch_button_on("attack"));
        i.touch_event(1, TouchPhase::Ended, 610.0, 10.0);
        assert!(!i.read_with(None).attack);
        // 強は押した瞬間だけ
        i.touch_event(2, TouchPhase::Started, 670.0, 10.0);
        assert!(i.read_with(None).heavy);
        assert!(!i.read_with(None).heavy);
        i.touch_event(2, TouchPhase::Ended, 670.0, 10.0);
        i.touch_event(3, TouchPhase::Started, 730.0, 10.0);
        assert!(i.read_with(None).pause_pressed);
        assert!(!i.read_with(None).pause_pressed);
    }

    #[test]
    fn 同じボタンの2本目を離すまで押したまま() {
        let mut i = input();
        i.set_touch_buttons(&[("attack", TouchRect { x0: 0.0, y0: 0.0, x1: 50.0, y1: 50.0 })]);
        i.touch_event(1, TouchPhase::Started, 10.0, 10.0);
        i.touch_event(2, TouchPhase::Started, 20.0, 10.0);
        // TS と同じく、当たりの持ち主は後の指
        i.touch_event(1, TouchPhase::Ended, 10.0, 10.0);
        assert!(i.touch_button_on("attack"));
        i.touch_event(2, TouchPhase::Ended, 20.0, 10.0);
        assert!(!i.touch_button_on("attack"));
    }

    #[test]
    fn タッチの操作層が消えている間は取らない() {
        let mut i = input();
        i.set_touch_on(false);
        i.touch_event(1, TouchPhase::Started, 600.0, 300.0);
        assert!(i.touch());
        assert!(!i.read_with(None).confirm_pressed);
        i.set_touch_on(true);
        i.set_pointer_over_ui(true);
        i.touch_event(2, TouchPhase::Started, 100.0, 300.0);
        assert!(i.stick_view().is_none());
    }

    #[test]
    fn タッチを始めるまではタッチの処理をしない() {
        let mut i = input();
        i.key(key(KeyCode::KeyA), true);
        assert!(!i.touch());
        assert_eq!(i.read_with(None).x, -1.0);
    }
    #[test]
    fn パッドが無くても読める() {
        let mut i = Input::new();
        let c = i.read();
        assert_eq!((c.x, c.y), (0.0, 0.0));
        assert!(!c.any_pressed);
    }
}
