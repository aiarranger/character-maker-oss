// 操作の割り当て表と入力（src/controls.ts・src/input.ts の写し）。入力・一時停止の操作の説明・タッチのボタンは、すべてこの表から作る。
// 走る向きは自動。プレイヤーは進路（左右）・ダッシュ・ジャンプだけを操作する（企画 03）。キー配置は変更でき、端末に残す。
use island_kit::app::input::{Device, Frame, Input, InputOpts};
use island_kit::app::store;
use game_ui::touch_pad::PadFrame;
use serde_json::{Map, Value, json};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rebindable { Left, Right, Dash, Jump, Attack }

pub const DASH_PAD: [usize; 2] = [0, 7];
pub const JUMP_PAD: [usize; 2] = [2, 5];
/// 攻撃: B / ○ と LB（レース中だけ。画面の「もどる」と同じボタンだが、走っている間は戻る先が無い）
pub const ATTACK_PAD: [usize; 2] = [1, 4];
const ATTACK_LABEL: &str = "攻撃（武器を持っている間。押し続けると息をためる）";
const DASH_LABEL: &str = "ダッシュ（押している間。スタミナを使う）";
const JUMP_LABEL: &str = "ジャンプ（ハードル・水たまり）";
const PAUSE_PAD: usize = 9;
const STORE: &str = "gyogyo.keys";

fn s(a: &[&str]) -> Vec<String> { a.iter().map(|x| x.to_string()).collect() }

pub struct Keys { pub dash: Vec<String>, pub jump: Vec<String>, pub attack: Vec<String>, pub left: Vec<String>, pub right: Vec<String> }

impl Keys {
    /// loadKeys（保存できなくても遊べる）
    pub fn load() -> Keys {
        let mut k = Keys { dash: s(&["Space"]), jump: s(&["KeyJ"]), attack: s(&["KeyK"]), left: s(&["KeyA", "ArrowLeft"]), right: s(&["KeyD", "ArrowRight"]) };
        if let Some(Value::Object(v)) = store::get_json(STORE) {
            let list = |x: &Value| x.as_array().map(|a| a.iter().filter_map(|e| e.as_str().map(String::from)).collect::<Vec<_>>());
            if let Some(a) = v.get("dash").and_then(list) { k.dash = a; }
            if let Some(a) = v.get("jump").and_then(list) { k.jump = a; }
            if let Some(a) = v.get("attack").and_then(list) { k.attack = a; }
            if let Some(a) = v.get("left").and_then(list) { k.left = a; }
            if let Some(a) = v.get("right").and_then(list) { k.right = a; }
        }
        k
    }
    pub fn set_key(&mut self, id: Rebindable, code: &str) {
        // 同じキーをほかの操作から外す
        self.dash.retain(|k| k != code);
        self.jump.retain(|k| k != code);
        self.attack.retain(|k| k != code);
        self.left.retain(|k| k != code);
        self.right.retain(|k| k != code);
        match id {
            Rebindable::Left => { let mut v = vec![code.to_string()]; v.extend(self.left.iter().filter(|k| *k == "ArrowLeft").cloned()); self.left = v; }
            Rebindable::Right => { let mut v = vec![code.to_string()]; v.extend(self.right.iter().filter(|k| *k == "ArrowRight").cloned()); self.right = v; }
            Rebindable::Dash => self.dash = vec![code.to_string()],
            Rebindable::Jump => self.jump = vec![code.to_string()],
            Rebindable::Attack => self.attack = vec![code.to_string()],
        }
        let mut all = Map::new();
        all.insert("left".into(), json!(self.left));
        all.insert("right".into(), json!(self.right));
        all.insert("dash".into(), json!(self.dash));
        all.insert("jump".into(), json!(self.jump));
        all.insert("attack".into(), json!(self.attack));
        store::set_json(STORE, &Value::Object(all));
    }
    /// keyOf（無ければ空）
    pub fn key_of(&self, id: Rebindable) -> String {
        let l = match id { Rebindable::Left => &self.left, Rebindable::Right => &self.right, Rebindable::Dash => &self.dash, Rebindable::Jump => &self.jump, Rebindable::Attack => &self.attack };
        l.first().cloned().unwrap_or_default()
    }
}

pub fn key_name(code: &str) -> String {
    match code {
        "Space" => "Space".into(), "Escape" => "Esc".into(), "ShiftLeft" | "ShiftRight" => "Shift".into(), "ArrowLeft" => "←".into(), "ArrowRight" => "→".into(), "ArrowUp" => "↑".into(), "ArrowDown" => "↓".into(), "Enter" => "Enter".into(),
        _ => code.strip_prefix("Key").or_else(|| code.strip_prefix("Digit")).unwrap_or(code).to_string(),
    }
}
fn pad_name(i: usize) -> String {
    match i { 0 => "A / ×".into(), 1 => "B / ○".into(), 2 => "X / □".into(), 3 => "Y / △".into(), 4 => "LB / L1".into(), 5 => "RB / R1".into(), 6 => "LT / L2".into(), 7 => "RT / R2".into(), 9 => "Start".into(), _ => format!("ボタン{i}") }
}
fn uniq(mut a: Vec<String>) -> Vec<String> { let mut seen = Vec::new(); a.retain(|x| { if seen.contains(x) { false } else { seen.push(x.clone()); true } }); a }

/// 操作の説明の行（機器ごとの表記）。[操作, 説明]
pub fn help_rows(device: Device, k: &Keys) -> Vec<(String, String)> {
    let names = |l: &[String]| uniq(l.iter().map(|c| key_name(c)).collect()).join("・");
    let pads = |l: &[usize]| uniq(l.iter().map(|i| pad_name(*i)).collect()).join("・");
    match device {
        Device::Pad => vec![
            ("左スティック・十字".into(), "進路（左で内・右で外）".into()),
            (pads(&DASH_PAD), DASH_LABEL.into()),
            (pads(&JUMP_PAD), JUMP_LABEL.into()),
            (pads(&ATTACK_PAD), ATTACK_LABEL.into()),
            (pad_name(PAUSE_PAD), "一時停止・操作の説明".into()),
        ],
        Device::Touch => vec![
            ("左のスティック".into(), "進路（左で内・右で外）".into()),
            ("「ダッシュ」".into(), DASH_LABEL.into()),
            ("「ジャンプ」".into(), JUMP_LABEL.into()),
            ("「こうげき」".into(), ATTACK_LABEL.into()),
            ("右上の Ⅱ".into(), "一時停止・操作の説明".into()),
        ],
        Device::Keys => vec![
            (format!("{}　{}", names(&k.left), names(&k.right)), "進路（左で内・右で外）".into()),
            (names(&k.dash), DASH_LABEL.into()),
            (names(&k.jump), JUMP_LABEL.into()),
            (names(&k.attack), ATTACK_LABEL.into()),
            (uniq(vec!["Esc".into(), "P".into(), "右上の Ⅱ".into()]).join("・"), "一時停止・操作の説明".into()),
        ],
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Controls {
    /// 画面の向きの移動（x 右、y 奥）
    pub x: f64, pub y: f64,
    pub act_dash: bool, pub act_jump: bool,
    pub hold_dash: bool, pub hold_jump: bool,
    /// 攻撃（押した瞬間・押している間）
    pub act_attack: bool, pub hold_attack: bool,
    pub pause_pressed: bool, pub confirm_pressed: bool, pub back_pressed: bool,
    pub left_pressed: bool, pub right_pressed: bool, pub up_pressed: bool, pub down_pressed: bool,
    /// 一時停止の項目を押す（Enter・Space・パッドの A。割り当てを変えても同じ）
    pub menu_confirm: bool,
    /// このコマに使った機器（最後に触った機器を決める。app.rs）: キー・ゲームパッド・レース中の指のスティックとボタン
    pub key_used: bool, pub pad_used: bool, pub touch_used: bool,
}

pub fn create_input() -> Input {
    // 指のスティックとボタンは game-ui の touch_pad（app.rs が場所を渡し、ui.rs が描く）。DOM の #touch は無い
    let prevent_keys = ["Space", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Tab"].iter().map(|s| s.to_string()).collect();
    Input::new(InputOpts { touch_root: None, prevent_keys, stick_radius: 60.0, prevent_context_menu: true, touch_class_follows_device: true, ..Default::default() })
}

/// pad はレース中の指のスティックとボタン（game-ui の touch_pad。毎コマ 1 回読んだ物）。touch は最後に触った機器が指か
pub fn read(f: &Frame, k: &Keys, pad: &PadFrame, touch: bool) -> Controls {
    let dn = |l: &[String]| l.iter().any(|c| f.down(c));
    let pr = |l: &[String]| l.iter().any(|c| f.pressed(c));
    let b2f = |b: bool| if b { 1.0 } else { 0.0 };
    let mut c = Controls {
        x: b2f(dn(&k.right)) - b2f(dn(&k.left)),
        y: b2f(f.down("ArrowUp")) - b2f(f.down("ArrowDown")),
        act_dash: pr(&k.dash), act_jump: pr(&k.jump), hold_dash: dn(&k.dash), hold_jump: dn(&k.jump), act_attack: pr(&k.attack), hold_attack: dn(&k.attack),
        pause_pressed: f.any_pressed(&["Escape", "KeyP"]),
        confirm_pressed: f.any_pressed(&["Enter", "Space"]) || pr(&k.dash) || pr(&k.jump),
        back_pressed: f.any_pressed(&["Escape", "Backspace"]),
        left_pressed: f.pressed("ArrowLeft") || pr(&k.left),
        right_pressed: f.pressed("ArrowRight") || pr(&k.right),
        up_pressed: f.pressed("ArrowUp"),
        down_pressed: f.pressed("ArrowDown"),
        menu_confirm: f.any_pressed(&["Enter", "Space"]),
        key_used: f.any_key_pressed(),
        pad_used: false,
        touch_used: !pad.pressed.is_empty() || !pad.down.is_empty() || pad.tap || pad.x != 0.0 || pad.y != 0.0,
    };
    if let Some(gp) = &f.pad {
        let (ax, ay) = (gp.axis(0), gp.axis(1));
        if sim_math::hypot(ax, ay) > 0.2 { c.x = ax; c.y = -ay; }
        if gp.b(12) { c.y = 1.0; }
        if gp.b(13) { c.y = -1.0; }
        if gp.b(14) { c.x = -1.0; }
        if gp.b(15) { c.x = 1.0; }
        c.act_dash = c.act_dash || DASH_PAD.iter().any(|i| gp.edge(*i));
        c.hold_dash = c.hold_dash || DASH_PAD.iter().any(|i| gp.b(*i));
        c.act_jump = c.act_jump || JUMP_PAD.iter().any(|i| gp.edge(*i));
        c.hold_jump = c.hold_jump || JUMP_PAD.iter().any(|i| gp.b(*i));
        c.act_attack = c.act_attack || ATTACK_PAD.iter().any(|i| gp.edge(*i));
        c.hold_attack = c.hold_attack || ATTACK_PAD.iter().any(|i| gp.b(*i));
        c.pause_pressed = c.pause_pressed || gp.edge(PAUSE_PAD);
        c.confirm_pressed = c.confirm_pressed || gp.edge(0) || gp.edge(PAUSE_PAD);
        c.back_pressed = c.back_pressed || gp.edge(1);
        c.left_pressed = c.left_pressed || gp.edge(14) || gp.axis_edge(0, -1.0, 0.6);
        c.right_pressed = c.right_pressed || gp.edge(15) || gp.axis_edge(0, 1.0, 0.6);
        c.up_pressed = c.up_pressed || gp.edge(12) || gp.axis_edge(1, -1.0, 0.6);
        c.down_pressed = c.down_pressed || gp.edge(13) || gp.axis_edge(1, 1.0, 0.6);
        c.menu_confirm = c.menu_confirm || gp.edge(0);
        // island-kit が機器をパッドにする条件と同じ（ボタンを押した瞬間か、スティックの 0.5 超）
        c.pad_used = gp.any_edge() || sim_math::hypot(ax, ay) > 0.5;
    }
    if touch {
        let (tx, ty) = (pad.x as f64, pad.y as f64);
        if sim_math::hypot(tx, ty) > 0.15 { c.x = tx; c.y = ty; }
        c.act_dash = c.act_dash || pad.was_pressed("dash");
        c.hold_dash = c.hold_dash || pad.is_down("dash");
        c.act_jump = c.act_jump || pad.was_pressed("jump");
        c.hold_jump = c.hold_jump || pad.is_down("jump");
        c.act_attack = c.act_attack || pad.was_pressed("attack");
        c.hold_attack = c.hold_attack || pad.is_down("attack");
        c.confirm_pressed = c.confirm_pressed || pad.tap;
    }
    let len = sim_math::hypot(c.x, c.y);
    if len > 1.0 { c.x /= len; c.y /= len; }
    c
}
