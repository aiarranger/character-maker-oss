// 操作の割り当て表（魚群繚乱）。入力（input.rs）・指のボタンの字（ui.rs）・一時停止の技の表（ui.rs）・案内の文（hud.rs・ui.rs）は、すべてこの表から作る。
// キーを変えるのはここだけ（表記も一緒に変わる）。描画を読まないので cargo test で確かめる。
use sim::moves::{CHARGES, RANBU, NORMALS};
use winit::keyboard::KeyCode;

/// 最後に触った機器（表記と、指の部品の出し入れを決める）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Device {
    /// キーボード・マウス
    #[default]
    Key,
    Pad,
    Touch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act { Attack, Heavy, Jump, Dodge, Ranbu, Wire }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mouse { Left, Right }

#[derive(Clone, Copy, Debug)]
pub struct Bind {
    pub act: Act,
    /// 指のボタンの名前（今の版の data-btn）
    pub id: &'static str,
    /// 案内に出す短い名前
    pub name: &'static str,
    /// キー（先頭を表記に出す）
    pub keys: &'static [KeyCode],
    pub key: &'static str,
    pub mouse: Option<Mouse>,
    /// ゲームパッドのボタン（標準の並び。先頭を表記に出す）
    pub pad: &'static [usize],
    pub pad_name: &'static str,
    /// 指のボタンの字
    pub touch: &'static str,
}

use KeyCode::*;
pub const BINDS: [Bind; 6] = [
    Bind { act: Act::Attack, id: "attack", name: "通常", keys: &[KeyJ, KeyZ], key: "J", mouse: Some(Mouse::Left), pad: &[2], pad_name: "X/□", touch: "斬" },
    Bind { act: Act::Heavy, id: "heavy", name: "強", keys: &[KeyK, KeyX], key: "K", mouse: Some(Mouse::Right), pad: &[3], pad_name: "Y/△", touch: "強" },
    Bind { act: Act::Jump, id: "jump", name: "跳ぶ", keys: &[Space], key: "Space", mouse: None, pad: &[0], pad_name: "A/×", touch: "跳" },
    Bind { act: Act::Dodge, id: "dodge", name: "避ける", keys: &[ShiftLeft, ShiftRight, KeyL, KeyC], key: "Shift", mouse: None, pad: &[1, 4], pad_name: "B/○", touch: "避" },
    Bind { act: Act::Ranbu, id: "ranbu", name: "乱舞", keys: &[KeyI, KeyF, KeyV], key: "I", mouse: None, pad: &[5, 7], pad_name: "RB/R1", touch: "乱舞" },
    // 飴糸（試作、2026-11-12）: 飴の糸を撃って飛ぶ
    Bind { act: Act::Wire, id: "wire", name: "飴糸", keys: &[KeyU, KeyR], key: "U", mouse: None, pad: &[6], pad_name: "LT/L2", touch: "糸" },
];
/// 一時停止
pub const PAUSE_KEYS: [KeyCode; 2] = [Escape, KeyP];
pub const PAUSE_PAD: usize = 9;
/// 指の Ⅱ の名前
pub const PAUSE_ID: &str = "pause";

pub fn bind(a: Act) -> &'static Bind { BINDS.iter().find(|b| b.act == a).unwrap() }

/// その機器での表記（キーの名前・パッドのボタン・画面のボタンの字）
pub fn label(a: Act, d: Device) -> &'static str {
    let b = bind(a);
    match d { Device::Key => b.key, Device::Pad => b.pad_name, Device::Touch => b.touch }
}
pub fn pause_label(d: Device) -> &'static str { match d { Device::Key => "Esc", Device::Pad => "Start", Device::Touch => "Ⅱ" } }
/// 決定（ボタンの札に添える。指は添えない）
pub fn confirm_label(d: Device) -> &'static str { match d { Device::Key => "Enter", Device::Pad => "A/×", Device::Touch => "" } }

/// 巨人が立っている間の案内
pub fn giant_hint(d: Device) -> String { format!("巨人の脚へ 敵を吹き飛ばしてぶつけろ ／ 飴糸（{}）で頭上へ", label(Act::Wire, d)) }
/// わたあめ雲に乗っている間の案内
/// 猪鹿蝶: 斬った直後（飴糸で空へ逃げる）と、空へ逃げたあと（飴糸で斬り込む）の案内
pub fn wire_esc_hint(d: Device) -> String { format!("飴糸（{}）で空へ逃げる", label(Act::Wire, d)) }
pub fn wire_dive_hint(d: Device) -> String { format!("飴糸（{}）で斬り込む", label(Act::Wire, d)) }
pub fn ride_hint(d: Device) -> String { format!("わたあめ雲 ── {} 飛燕・{} 墜星・{} 降りる", label(Act::Attack, d), label(Act::Heavy, d), label(Act::Jump, d)) }
/// 担いだときの案内
pub fn carry_hint(d: Device) -> String { format!("担いだ ── {}・{} で投げる", label(Act::Attack, d), label(Act::Heavy, d)) }
/// 敗北の一言
pub fn defeat_hint(d: Device) -> String {
    format!("その場から立ち上がれる（得点は下がる）。予告の赤い印は、回避（{}）か跳躍（{}）でかわせる", label(Act::Dodge, d), label(Act::Jump, d))
}

/// 出陣の直後に画面の下へ出す操作の説明: (札の字, 説明)。指は画面のボタンが説明なので出さない
pub fn help_groups(d: Device) -> Vec<(Vec<&'static str>, String)> {
    match d {
        Device::Touch => vec![],
        Device::Pad => {
            let mut v = vec![(vec!["左スティック"], "移動".to_string())];
            v.extend(BINDS.iter().map(|b| (vec![b.pad_name], b.name.to_string())));
            v.push((vec!["右スティック"], "視点".to_string()));
            v
        }
        Device::Key => {
            let mut v = vec![(vec!["WASD"], "移動".to_string())];
            for b in &BINDS {
                let m = match b.mouse { Some(Mouse::Left) => "/左クリック ", Some(Mouse::Right) => "/右クリック ", None => "" };
                v.push((vec![b.key], format!("{m}{}", b.name)));
            }
            v.push((vec!["Q", "E"], "/ドラッグ 視点".to_string()));
            v
        }
    }
}

/// 技の表の入力の 1 かけら
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok { In(Act), Txt(&'static str) }
/// 技の表の 1 行（一時停止の画面）
#[derive(Clone, Debug)]
pub struct MoveRow { pub input: Vec<Tok>, pub name: &'static str, pub desc: &'static str }

/// 技の一覧。入力は割り当ての表から、名前は技の表（sim::moves）から
pub fn move_rows() -> Vec<MoveRow> {
    use Act::*;
    use Tok::*;
    let r = |input: &[Tok], name: &'static str, desc: &'static str| MoveRow { input: input.to_vec(), name, desc };
    let chain = ["", "", "×2", "×3", "×4", "×5", "×6"];
    let descs = ["突進して打ち上げる", "真上へ打ち上げる（空中の敵は追い打ちできる）", "まわりを巻き込む回転斬り", "前へ走る衝撃波", "跳んで叩きつける大波動", "進みながら竜巻", "跳び上がって大爆発"];
    let mut rows = vec![r(&[In(Attack)], NORMALS[0].name, "通常攻撃。押し続けると六段までつながる")];
    for n in 0..7 {
        let input: Vec<Tok> = if n == 0 { vec![In(Heavy)] } else if n == 1 { vec![In(Attack), Txt("→"), In(Heavy)] } else { vec![In(Attack), Txt(chain[n]), Txt("→"), In(Heavy)] };
        rows.push(MoveRow { input, name: CHARGES[n].name, desc: descs[n] });
    }
    rows.push(r(&[In(Jump), Txt("→"), In(Attack), Txt("/"), In(Heavy)], "飛燕 / 墜星", "空中斬り / 落下の叩きつけ"));
    rows.push(r(&[In(Dodge), Txt("→"), In(Attack)], "疾風", "回避から回転斬り"));
    rows.push(r(&[In(Jump), Txt("→"), In(Jump)], "わたあめ雲", "空中でもう一度跳ぶと、雲に乗って飛ぶ"));
    rows.push(r(&[In(Wire)], "飴糸", "飴を 1 個使って空を飛ぶ。武将と大将には飛びついて斬る"));
    rows.push(r(&[In(Wire), Txt("×5")], "猪鹿蝶", "斬る・空へ逃げるを交互に。3 回斬ると決まる"));
    rows.push(r(&[In(Ranbu)], RANBU.name, "ゲージ満タンで、まわりを一掃する"));
    rows
}

/// 入力の列の文字（その機器の表記でつなぐ）
pub fn input_text(input: &[Tok], d: Device) -> String {
    input.iter().map(|t| match t { Tok::In(a) => label(*a, d), Tok::Txt(s) => s }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(d: Device) -> Vec<String> { move_rows().iter().map(|r| input_text(&r.input, d)).collect() }

    /// 技の表の入力の列は、機器ごとの表記で出る（キーは今までと同じ字）
    #[test]
    fn 技の表の入力は機器の表記() {
        assert_eq!(col(Device::Key), ["J", "K", "J→K", "J×2→K", "J×3→K", "J×4→K", "J×5→K", "J×6→K", "Space→J/K", "Shift→J", "Space→Space", "U", "U×5", "I"]);
        assert_eq!(col(Device::Touch), ["斬", "強", "斬→強", "斬×2→強", "斬×3→強", "斬×4→強", "斬×5→強", "斬×6→強", "跳→斬/強", "避→斬", "跳→跳", "糸", "糸×5", "乱舞"]);
        let pad = col(Device::Pad);
        assert_eq!((pad[0].as_str(), pad[3].as_str(), pad[9].as_str()), ("X/□", "X/□×2→Y/△", "B/○→X/□"));
        // 指の列が空にならない（2026-11-11 までは指のとき列ごと消えていた）
        assert!(col(Device::Touch).iter().all(|s| !s.is_empty()));
    }

    #[test]
    fn 案内の文は機器の表記() {
        assert_eq!(carry_hint(Device::Key), "担いだ ── J・K で投げる");
        assert_eq!(carry_hint(Device::Touch), "担いだ ── 斬・強 で投げる");
        assert_eq!(ride_hint(Device::Key), "わたあめ雲 ── J 飛燕・K 墜星・Space 降りる");
        assert_eq!(ride_hint(Device::Touch), "わたあめ雲 ── 斬 飛燕・強 墜星・跳 降りる");
        assert!(defeat_hint(Device::Key).contains("回避（Shift）か跳躍（Space）"));
        assert!(defeat_hint(Device::Touch).contains("回避（避）か跳躍（跳）"));
        assert!(defeat_hint(Device::Pad).contains("回避（B/○）か跳躍（A/×）"));
    }

    #[test]
    fn 下の説明は指では出さない() {
        assert!(help_groups(Device::Touch).is_empty());
        let key = help_groups(Device::Key);
        assert_eq!(key.iter().map(|(k, t)| format!("{}{t}", k.join(" "))).collect::<Vec<_>>(), ["WASD移動", "J/左クリック 通常", "K/右クリック 強", "Space跳ぶ", "Shift避ける", "I乱舞", "U飴糸", "Q E/ドラッグ 視点"]);
        assert_eq!(help_groups(Device::Pad).len(), 8);
    }
}
