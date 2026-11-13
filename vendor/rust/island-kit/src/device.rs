// 入力の判定のうち、ブラウザが無くても試験できる所（受け口は app::input。ブラウザだけ）。
// 最後に触った機器の決め方（各ゲームの src/input.ts で違うので、出来事と決め方の表にする）・ゲームパッドの押した瞬間と読む台・仮想スティックの値・指の役。

/// 最後に触った機器（操作の説明の表記を合わせる）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Device { #[default] Keys, Pad, Touch }

/// 機器を変えるかもしれない出来事
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceEvent {
    /// (pointer: coarse) の端末で立ち上げた
    Coarse,
    /// keydown
    Key,
    /// canvas の上の mousedown。スマホは指を離した後にブラウザがまねの mousedown を出すので、本当のマウスとは限らない
    CanvasMouse,
    /// window の touchstart（どの要素でも）
    WindowTouch,
    /// touch_root の touchstart
    RootTouch,
    /// pointer_ignore に当たらない pointerdown。touch は pointerType が touch か
    Pointer { touch: bool },
    /// つながっている最初のパッドの、ボタンを押した瞬間かスティックの 0.5 超
    Pad,
}

/// 機器の決め方（今の版の src/input.ts）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceRule {
    /// pengo・digdag・iceclimber・rainbow・bubble2（input.touch）: 機器を追わない。最初の touchstart か (pointer: coarse) で touch.active と body.touch が付き、外れない。
    /// Frame.device はこの印の写し（Touch か Keys）で、キー・マウス・パッドでは変わらない
    TouchOnce,
    /// keiba・ひな形（input.device）: keydown と canvas の mousedown でキー、touch_root の touchstart でタッチ、最初のパッドでパッド。
    /// まねの mousedown が canvas に来るとキーへ戻るのも今の版と同じ（遊びの中は touch_root が canvas を覆うので、canvas には来ない）
    LastUsedMouse,
    /// pinball（input.device）: keydown でキー、pointer_ignore に当たらない pointerdown で pointerType のとおり、最初のパッドでパッド。
    /// mousedown と touchstart は見ない（まねの mousedown では戻らない）
    LastUsedPointer,
}

impl DeviceRule {
    /// InputOpts の touch_class_follows_device と pointer_ignore から
    pub fn of(follows: bool, pointer: bool) -> DeviceRule {
        match (follows, pointer) {
            (false, _) => DeviceRule::TouchOnce,
            (true, false) => DeviceRule::LastUsedMouse,
            (true, true) => DeviceRule::LastUsedPointer,
        }
    }

    /// この出来事で機器がどれになるか（None は変えない）
    pub fn next(self, ev: DeviceEvent) -> Option<Device> {
        use DeviceEvent as E;
        match (self, ev) {
            (_, E::Coarse) => Some(Device::Touch),
            (DeviceRule::TouchOnce, E::WindowTouch) => Some(Device::Touch),
            (DeviceRule::TouchOnce, _) => None,
            (_, E::Key) => Some(Device::Keys),
            (_, E::Pad) => Some(Device::Pad),
            (DeviceRule::LastUsedMouse, E::CanvasMouse) => Some(Device::Keys),
            (DeviceRule::LastUsedMouse, E::RootTouch) => Some(Device::Touch),
            (DeviceRule::LastUsedPointer, E::Pointer { touch }) => Some(if touch { Device::Touch } else { Device::Keys }),
            _ => None,
        }
    }
}

/// ゲームパッドの生の値（つながっている 1 台）
#[derive(Clone, Debug, Default)]
pub struct Pad { pub axes: Vec<f64>, pub buttons: Vec<bool>, prev_axes: Vec<f64>, prev_buttons: Vec<bool> }
impl Pad {
    pub fn axis(&self, i: usize) -> f64 { self.axes.get(i).copied().unwrap_or(0.0) }
    pub fn b(&self, i: usize) -> bool { self.buttons.get(i).copied().unwrap_or(false) }
    /// 押した瞬間
    pub fn edge(&self, i: usize) -> bool { self.b(i) && !self.prev_buttons.get(i).copied().unwrap_or(false) }
    /// どれかのボタンを押した瞬間
    pub fn any_edge(&self) -> bool { (0..self.buttons.len()).any(|i| self.edge(i)) }
    /// 軸が sign の向き（-1 か 1）へ threshold を超えた瞬間（TS の `ax < -0.6 && !padPrev[100]`）
    pub fn axis_edge(&self, i: usize, sign: f64, threshold: f64) -> bool {
        self.axis(i) * sign > threshold && self.prev_axes.get(i).copied().unwrap_or(0.0) * sign <= threshold
    }
    /// 機器をパッドにする操作か（TS の `if (anyPad || Math.hypot(ax, ay) > 0.5) use('pad')`）
    fn is_use(&self) -> bool { self.any_edge() || self.axis(0).hypot(self.axis(1)) > 0.5 }
}

/// 同じ画面で読むパッドの数（協力のゲーム用の Frame.pads の上限）
pub const MAX_PADS: usize = 4;

/// navigator.getGamepads を毎コマ読んで Pad にする。前のコマの値はつながっている順に控える
#[derive(Clone, Debug, Default)]
pub struct PadReader { prev_axes: Vec<Vec<f64>>, prev_buttons: Vec<Vec<bool>> }
impl PadReader {
    /// connected はつながっている台の (axes, buttons)（空きの番号は飛ばす）。戻すのは MAX_PADS 台までの Pad と、機器をパッドにするか。
    /// 機器を変えるのは最初の 1 台だけ（今の版の TS は最初の 1 台を読んで break する）
    pub fn read(&mut self, connected: impl IntoIterator<Item = (Vec<f64>, Vec<bool>)>) -> (Vec<Pad>, bool) {
        let mut pads: Vec<Pad> = Vec::new();
        for (i, (axes, buttons)) in connected.into_iter().take(MAX_PADS).enumerate() {
            if self.prev_axes.len() <= i { self.prev_axes.push(Vec::new()); self.prev_buttons.push(Vec::new()); }
            let pad = Pad { axes, buttons, prev_axes: std::mem::take(&mut self.prev_axes[i]), prev_buttons: std::mem::take(&mut self.prev_buttons[i]) };
            self.prev_axes[i] = pad.axes.clone();
            self.prev_buttons[i] = pad.buttons.clone();
            pads.push(pad);
        }
        let used = pads.first().is_some_and(Pad::is_use);
        (pads, used)
    }
}

/// 仮想スティックの値。x 右・y 上（-1..1）、knob はつまみの位置（px）
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StickValue { pub x: f64, pub y: f64, pub knob_x: f64, pub knob_y: f64 }

/// 指が押した所からの動き（px）から。four_way はマス目の遊び（pengo）で 4 方向に吸い付かせる。radius を超えた分は縁で止める
pub fn stick_value(mut dx: f64, mut dy: f64, radius: f64, four_way: bool) -> StickValue {
    if four_way {
        if dx.abs() > dy.abs() { dy = 0.0; } else { dx = 0.0; }
    }
    let d = dx.hypot(dy);
    if d > radius { dx *= radius / d; dy *= radius / d; }
    StickValue { x: dx / radius, y: -dy / radius, knob_x: dx, knob_y: dy }
}

/// data-btn に当たらなかった指の役
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchRole { Stick, Camera, Tap, Nothing }

/// 画面の左半分でスティックが空いていればスティック。それ以外は、カメラを回す遊び（ひな形）ならカメラ（空いていれば。叩いた扱いも付く）、ほかは叩いただけ
pub fn touch_role(x: f64, width: f64, stick_free: bool, touch_camera: bool, camera_free: bool) -> TouchRole {
    if stick_free && x < width * 0.5 { return TouchRole::Stick; }
    if !touch_camera { return TouchRole::Tap; }
    if camera_free { TouchRole::Camera } else { TouchRole::Nothing }
}
