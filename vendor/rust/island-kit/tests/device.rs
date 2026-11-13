// 機器の決め方が、各ゲームの src/input.ts と同じか。出来事の並びは本当の Chrome の順
// （指: pointerdown → touchstart → 離す → mousedown。touch_root の空きは preventDefault するのでまねの mousedown は来ない）。
use island_kit::device::{Device, DeviceEvent as E, DeviceRule as R, MAX_PADS, PadReader, StickValue, TouchRole, stick_value, touch_role};

fn run(rule: R, start: Device, evs: &[E]) -> Device {
    evs.iter().fold(start, |d, e| rule.next(*e).unwrap_or(d))
}
const FINGER: E = E::Pointer { touch: true };
const MOUSE: E = E::Pointer { touch: false };

/// 指で canvas を押して離す（#touch の外）
const TAP_CANVAS: [E; 3] = [FINGER, E::WindowTouch, E::CanvasMouse];
/// 指で #touch の空きを押して離す（preventDefault するのでまねの mousedown は来ない）
const TAP_ROOT: [E; 3] = [FINGER, E::RootTouch, E::WindowTouch];
/// 指でボタンを押して離す（pointer_ignore に当たるので pointerdown は渡らない）
const TAP_BUTTON: [E; 1] = [E::WindowTouch];

// pinball で見つかった故障: 指を離した後のまねの mousedown でキーの表記に戻り、body の touch も外れた
#[test]
fn pinball_finger_stays_touch_after_fake_mousedown() {
    assert_eq!(run(R::LastUsedPointer, Device::Touch, &TAP_CANVAS), Device::Touch);
    assert_eq!(run(R::LastUsedPointer, Device::Keys, &TAP_CANVAS), Device::Touch);
}

// pinball の input.ts は touchstart を見ない: 指でボタンを押しただけでは説明の表記を替えない
#[test]
fn pinball_ignored_button_does_not_switch() {
    assert_eq!(run(R::LastUsedPointer, Device::Keys, &TAP_BUTTON), Device::Keys);
    assert_eq!(run(R::LastUsedPointer, Device::Pad, &TAP_BUTTON), Device::Pad);
}

#[test]
fn pinball_mouse_and_key_go_back_to_keys() {
    assert_eq!(run(R::LastUsedPointer, Device::Touch, &[MOUSE, E::CanvasMouse]), Device::Keys);
    assert_eq!(run(R::LastUsedPointer, Device::Touch, &[E::Key]), Device::Keys);
}

// keiba・ひな形の input.ts は window の touchstart を見ない: #touch の外を指で押しただけではタッチにならない
#[test]
fn keiba_window_touch_does_not_switch() {
    assert_eq!(run(R::LastUsedMouse, Device::Keys, &TAP_BUTTON), Device::Keys);
    assert_eq!(run(R::LastUsedMouse, Device::Pad, &TAP_BUTTON), Device::Pad);
}

// keiba・ひな形の今の版は canvas の mousedown でキーへ戻す（まねの mousedown でも）。今の版と同じ
#[test]
fn keiba_follows_input_ts() {
    assert_eq!(run(R::LastUsedMouse, Device::Keys, &TAP_ROOT), Device::Touch);
    assert_eq!(run(R::LastUsedMouse, Device::Touch, &TAP_CANVAS), Device::Keys);
    assert_eq!(run(R::LastUsedMouse, Device::Touch, &[E::Key]), Device::Keys);
    assert_eq!(run(R::LastUsedMouse, Device::Touch, &[E::Pad]), Device::Pad);
}

// pengo・digdag などの input.touch は一度立つと外れない。キー・マウス・パッドでも戻らない
#[test]
fn touch_once_never_goes_back() {
    let all = [E::Key, E::Pad, E::CanvasMouse, MOUSE, FINGER, E::RootTouch];
    assert_eq!(run(R::TouchOnce, Device::Touch, &all), Device::Touch);
    assert_eq!(run(R::TouchOnce, Device::Touch, &TAP_CANVAS), Device::Touch);
    assert_eq!(run(R::TouchOnce, Device::Keys, &[E::WindowTouch]), Device::Touch);
    // 指に触れるまでは Keys のまま（パッドでも）
    assert_eq!(run(R::TouchOnce, Device::Keys, &[E::Key, E::Pad, E::CanvasMouse]), Device::Keys);
}

#[test]
fn coarse_starts_touch_in_every_rule() {
    for r in [R::TouchOnce, R::LastUsedMouse, R::LastUsedPointer] { assert_eq!(r.next(E::Coarse), Some(Device::Touch)); }
}

// 各ゲームの InputOpts がどの決め方になるか
#[test]
fn rule_from_opts() {
    assert_eq!(R::of(false, false), R::TouchOnce);   // pengo など
    assert_eq!(R::of(false, true), R::TouchOnce);    // pinball（bdd9f19 の形。機器は controls.rs が持つ）
    assert_eq!(R::of(true, false), R::LastUsedMouse); // keiba・ひな形
    assert_eq!(R::of(true, true), R::LastUsedPointer); // pinball（kit に任せる形）
}

// ---- ゲームパッド（各ゲームの src/input.ts の for (const gp of pads) { if (!gp) continue; ...; break; }） ----

fn pad(axes: [f64; 2], pressed: &[usize]) -> (Vec<f64>, Vec<bool>) {
    let mut b = vec![false; 4];
    for i in pressed { b[*i] = true; }
    (axes.to_vec(), b)
}

// 防ぐ故障: ボタンを押し続けている間ずっと押した瞬間になる（決定が毎コマ走る）
#[test]
fn pad_edge_is_one_read_only() {
    let mut r = PadReader::default();
    let (p, _) = r.read([pad([0.0, 0.0], &[1])]);
    assert!(p[0].edge(1) && p[0].b(1) && p[0].any_edge());
    let (p, used) = r.read([pad([0.0, 0.0], &[1])]);
    assert!(!p[0].edge(1) && p[0].b(1) && !p[0].any_edge() && !used);
    let (p, _) = r.read([pad([0.0, 0.0], &[])]);
    assert!(!p[0].edge(1) && !p[0].b(1));
    let (p, _) = r.read([pad([0.0, 0.0], &[1])]);
    assert!(p[0].edge(1));
}

// 防ぐ故障: 倒し続けている間、メニューの左右が毎コマ動く（padPrev[100]・[101] の役）
#[test]
fn pad_axis_edge_is_one_read_only() {
    let mut r = PadReader::default();
    let (p, _) = r.read([pad([-0.7, 0.0], &[])]);
    assert!(p[0].axis_edge(0, -1.0, 0.6) && !p[0].axis_edge(0, 1.0, 0.6));
    let (p, _) = r.read([pad([-0.7, 0.0], &[])]);
    assert!(!p[0].axis_edge(0, -1.0, 0.6));
    let (p, _) = r.read([pad([-0.6, 0.0], &[])]);
    assert!(!p[0].axis_edge(0, -1.0, 0.6), "ちょうど 0.6 は超えていない");
    let (p, _) = r.read([pad([-0.61, 0.0], &[])]);
    assert!(p[0].axis_edge(0, -1.0, 0.6));
}

// 今の版は最初の 1 台だけが機器を変える。2 台目のボタンやスティックで操作の説明の表記を替えない
#[test]
fn only_the_first_pad_switches_the_device() {
    let mut r = PadReader::default();
    assert!(!r.read([pad([0.0, 0.0], &[]), pad([0.9, 0.0], &[0])]).1);
    assert!(r.read([pad([0.0, 0.0], &[0]), pad([0.9, 0.0], &[0])]).1);
    assert!(!r.read([pad([0.0, 0.0], &[0]), pad([0.0, 0.0], &[])]).1, "押し続けは操作に数えない");
    assert!(r.read([pad([0.3, 0.41], &[0])]).1, "スティックは長さ 0.5 を超えたら");
    assert!(!r.read([pad([0.3, 0.4], &[0])]).1, "ちょうど 0.5 は超えていない");
    assert!(r.read([]).0.is_empty());
    assert!(!PadReader::default().read([]).1);
}

// 協力のゲーム用に 4 台まで読む。前のコマの値は台ごと（1 台目の押しっぱなしで 2 台目の押した瞬間が消えない）
#[test]
fn pads_are_read_separately_up_to_the_limit() {
    let mut r = PadReader::default();
    let five = || (0..5).map(|_| pad([0.0, 0.0], &[]));
    assert_eq!(r.read(five()).0.len(), MAX_PADS);
    let (p, _) = r.read([pad([0.0, 0.0], &[0]), pad([0.0, 0.0], &[])]);
    assert!(p[0].edge(0) && !p[1].edge(0));
    let (p, _) = r.read([pad([0.0, 0.0], &[0]), pad([0.0, 0.0], &[0])]);
    assert!(!p[0].edge(0) && p[1].edge(0));
}

// ---- 仮想スティック・指の役（pengo・keiba・ひな形の src/input.ts） ----

#[test]
fn stick_stops_at_the_rim_and_up_is_positive() {
    assert_eq!(stick_value(30.0, -40.0, 60.0, false), StickValue { x: 0.5, y: 40.0 / 60.0, knob_x: 30.0, knob_y: -40.0 });
    let v = stick_value(90.0, 120.0, 60.0, false);
    assert!((v.knob_x - 36.0).abs() < 1e-12 && (v.knob_y - 48.0).abs() < 1e-12 && (v.x.hypot(v.y) - 1.0).abs() < 1e-12 && v.y < 0.0);
    assert_eq!(stick_value(0.0, 0.0, 60.0, false), StickValue { x: 0.0, y: 0.0, knob_x: 0.0, knob_y: 0.0 });
}

// pengo: 長い方の軸に吸い付く。同じ長さなら縦
#[test]
fn stick_four_way_snaps_to_the_longer_axis() {
    assert_eq!(stick_value(20.0, 5.0, 56.0, true), StickValue { x: 20.0 / 56.0, y: 0.0, knob_x: 20.0, knob_y: 0.0 });
    assert_eq!(stick_value(5.0, -20.0, 56.0, true), StickValue { x: 0.0, y: 20.0 / 56.0, knob_x: 0.0, knob_y: -20.0 });
    let tie = stick_value(10.0, 10.0, 56.0, true);
    assert_eq!((tie.knob_x, tie.knob_y), (0.0, 10.0));
    assert!((stick_value(100.0, 5.0, 56.0, true).x - 1.0).abs() < 1e-12);
}

#[test]
fn touch_role_follows_input_ts() {
    // pengo・keiba（カメラの指が無い）
    assert_eq!(touch_role(100.0, 800.0, true, false, true), TouchRole::Stick);
    assert_eq!(touch_role(400.0, 800.0, true, false, true), TouchRole::Tap, "真ん中ちょうどは右の扱い");
    assert_eq!(touch_role(100.0, 800.0, false, false, true), TouchRole::Tap, "スティックが埋まっていれば叩いた扱い");
    // ひな形（残りの指でカメラ）
    assert_eq!(touch_role(100.0, 800.0, true, true, true), TouchRole::Stick);
    assert_eq!(touch_role(600.0, 800.0, true, true, true), TouchRole::Camera);
    assert_eq!(touch_role(600.0, 800.0, true, true, false), TouchRole::Nothing, "カメラの指が埋まっていれば何もしない");
    assert_eq!(touch_role(100.0, 800.0, false, true, true), TouchRole::Camera);
}
