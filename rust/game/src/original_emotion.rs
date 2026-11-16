// オリジナル 4 体の気持ちの表情（怒り・悲しみ・驚き・穏やか）。模型に無いモーフを、読み込むときに作って足す（docs/game-motions-spec.md）。
// 模型は大きく、固定のコミットから SHA-256 を確かめて配るので、模型のファイルは変えない。足したモーフは下見にも書き出す GLB にも入る。
// - 目（blinkLeft・blinkRight のモーフを持つ部分。まつ毛・ルクスの画面の目・モカの目も）: まばたきの差分を、目頭の側・目尻の側で
//   重みを変えて使う（怒りは目頭の側を下げる、悲しみは目尻の側を下げる、穏やかは全体を少し、驚きは少し逆へ開く）
// - 眉（名前に brow を含む、モーフの無い部分。人の 2 体）: 眉の中心のまわりで回して上下に動かす（大きさは眉の長さに対する割合）
// 口は元のモーフ（happy・aa・mouthClose）の組み合わせで、original.rs の emotion_mouth が決める。
use crate::original::Part;

pub const EMOTIONS: [&str; 4] = ["angry", "sad", "surprised", "relaxed"];
/// 視線（本人から見た左右）。目のモーフとして足す
pub const LOOKS: [&str; 4] = ["lookLeft", "lookRight", "lookUp", "lookDown"];
/// 視線で動かす量（目の幅・高さに対する割合）: (本人の左へ, 上へ)
const LOOK: [(f32, f32); 4] = [(0.11, 0.0), (-0.11, 0.0), (0.0, 0.10), (0.0, -0.10)];

/// 目のモーフの重み: (目頭の側, 目尻の側)。まばたきの差分に掛ける
const EYE: [(f32, f32); 4] = [(0.50, 0.10), (0.08, 0.46), (-0.16, -0.16), (0.26, 0.26)];
/// 眉: (眉頭を上げる回転（度）, 上げる量（眉の長さに対する割合）)
const BROW: [(f32, f32); 4] = [(-15.0, -0.10), (13.0, 0.04), (0.0, 0.20), (0.0, 0.04)];

fn centre(points: &[[f32; 3]]) -> [f32; 3] {
    let n = points.len().max(1) as f32;
    let mut c = [0.0f32; 3];
    for p in points { for k in 0..3 { c[k] += p[k] / n; } }
    c
}
fn smooth(a: f32, b: f32, x: f32) -> f32 { let t = ((x - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

/// 部品に気持ちのモーフを足す。左右の目が見つからない模型では何もしない。
/// ルクス（画面の顔）の目: 光る楕円を、気持ちごとの形にする。u: 目頭の側 0 〜 目尻の側 1、v: 下 -1 〜 上 1。返り値は (横の差, 縦の差)（目の幅・高さに対する割合）
fn robot_eye(emotion: usize, u: f32, v: f32) -> (f32, f32) {
    let top = v.max(0.0);
    match emotion {
        // 怒り: 上のふちを目頭の側へ下げる（つり目）
        0 => (0.0, -0.62 * top * (1.0 - u)),
        // 悲しみ: 上のふちを目尻の側へ下げる（たれ目）
        1 => (0.0, -0.55 * top * u),
        // 驚き: ひとまわり大きく
        2 => ((u - 0.5) * 0.18, v * 0.11),
        // 穏やか: 高さを 6 割に
        _ => (0.0, -0.20 * v),
    }
}
/// ルクスの口: u は口の端 -1 〜 1、v は下 -1 〜 上 1。返り値は (横の差, 縦の差)（口の幅に対する割合）
fn robot_mouth(emotion: usize, u: f32, v: f32) -> (f32, f32) {
    match emotion {
        // 怒り: への字を浅く、少し細く
        0 => (-0.06 * u, -0.07 * u * u),
        // 悲しみ: 両端を下げる（への字）
        1 => (-0.04 * u, -0.17 * u * u),
        // 驚き: 丸く開ける
        2 => (-0.30 * u, 0.13 * v),
        // 穏やか: 両端を少し上げる
        _ => (0.0, 0.09 * u * u),
    }
}

pub fn extend(parts: &mut [Part], character: &str) {
    let robot = character == "generic-robot";
    // モカは目が大きく、目尻の側を下げすぎると瞳までゆがむ
    let eye_weights: [(f32, f32); 4] = if character == "generic-cat" { [EYE[0], (0.08, 0.32), EYE[2], EYE[3]] } else { EYE };
    let blink_index = |p: &Part, side: &str| p.targets.iter().position(|n| n == side).filter(|k| *k < p.p.morph_pos.len());
    let eye_centre = |side: &str| -> Option<[f32; 3]> {
        let all: Vec<[f32; 3]> = parts.iter().filter(|p| blink_index(p, side).is_some()).flat_map(|p| p.p.positions.iter().copied()).collect();
        if all.is_empty() { None } else { Some(centre(&all)) }
    };
    let (Some(left), Some(right)) = (eye_centre("blinkLeft"), eye_centre("blinkRight")) else { return };
    // 左右の軸（模型の横）と、顔の中心
    let lateral = if (left[0] - right[0]).abs() >= (left[2] - right[2]).abs() { 0 } else { 2 };
    let middle = (left[lateral] + right[lateral]) / 2.0;
    let eye_span = (left[lateral] - right[lateral]).abs().max(1e-6);
    for part in parts.iter_mut() {
        if part.targets.iter().any(|n| EMOTIONS.contains(&n.as_str()) || LOOKS.contains(&n.as_str())) { continue; }
        let count = part.p.positions.len();
        let side = if blink_index(part, "blinkLeft").is_some() { Some("blinkLeft") } else if blink_index(part, "blinkRight").is_some() { Some("blinkRight") } else { None };
        let robot_part = robot && (side.is_some() || part.targets.iter().any(|n| n == "happy"));
        let deltas: Option<Vec<Vec<[f32; 3]>>> = if robot_part && count > 0 {
            // ルクス: 画面の目と口は平らな形なので、横と上下の広がりから形を決める
            let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for p in &part.p.positions { for k in 0..3 { lo[k] = lo[k].min(p[k]); hi[k] = hi[k].max(p[k]); } }
            let (width, height) = ((hi[lateral] - lo[lateral]).max(1e-6), (hi[1] - lo[1]).max(1e-6));
            let (cl, cy) = ((hi[lateral] + lo[lateral]) / 2.0, (hi[1] + lo[1]) / 2.0);
            let outward = if cl >= middle { 1.0 } else { -1.0 };
            Some((0..4).map(|emotion| part.p.positions.iter().map(|p| {
                let v = (p[1] - cy) / (height / 2.0);
                let mut d = [0.0f32; 3];
                if side.is_some() {
                    let u = ((p[lateral] - cl) * outward / width + 0.5).clamp(0.0, 1.0);
                    let (dl, dy) = robot_eye(emotion, u, v);
                    d[lateral] = dl * width * outward; d[1] = dy * height;
                } else {
                    let u = (p[lateral] - cl) / (width / 2.0);
                    let (dl, dy) = robot_mouth(emotion, u, v);
                    d[lateral] = dl * width; d[1] = dy * width;
                }
                d
            }).collect()).collect())
        } else if let Some(side) = side {
            // 目: まばたきで動く頂点の、顔の中心からの横の距離で目頭〜目尻を決める
            let blink = part.p.morph_pos[blink_index(part, side).unwrap()].clone();
            let moving: Vec<usize> = (0..count).filter(|i| blink[*i].iter().any(|v| v.abs() > 1e-7)).collect();
            if moving.is_empty() { None } else {
                let dist = |i: usize| (part.p.positions[i][lateral] - middle).abs();
                let (lo, hi) = moving.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), i| (lo.min(dist(*i)), hi.max(dist(*i))));
                Some(eye_weights.iter().map(|(inner, outer)| (0..count).map(|i| {
                    let t = if hi - lo > 1e-6 { smooth(0.0, 1.0, (dist(i) - lo) / (hi - lo)) } else { 0.5 };
                    let w = inner + (outer - inner) * t;
                    [blink[i][0] * w, blink[i][1] * w, blink[i][2] * w]
                }).collect()).collect())
            }
        } else if part.targets.is_empty() && part.name.to_lowercase().contains("brow") && count > 0 {
            // 眉: 顔の中心の左右どちらにあるかで、眉頭（中心の側）を決める
            let c = centre(&part.p.positions);
            let outward = if c[lateral] >= middle { 1.0 } else { -1.0 };
            let (lo, hi) = part.p.positions.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| (lo.min(p[lateral]), hi.max(p[lateral])));
            let length = (hi - lo).max(eye_span * 0.2);
            Some(BROW.iter().map(|(degrees, lift)| {
                let (sin, cos) = degrees.to_radians().sin_cos();
                part.p.positions.iter().map(|p| {
                    // u: 眉の中心からの横の距離（目尻の側が正）、y: 上下。眉頭を上げる向きに回す
                    let (u, y) = ((p[lateral] - c[lateral]) * outward, p[1] - c[1]);
                    let (nu, ny) = (u * cos + y * sin, y * cos - u * sin);
                    let mut d = [0.0f32; 3];
                    d[lateral] = (nu - u) * outward;
                    d[1] = ny - y + lift * length;
                    d
                }).collect()
            }).collect())
        } else { None };
        // 視線: 目の部品の、まぶたの開きの中（まばたきで動かない所）を横・上下へずらす。ルクスは光る目ごと動かす
        let looks: Option<Vec<Vec<[f32; 3]>>> = side.map(|side| {
            let blink = &part.p.morph_pos[blink_index(part, side).unwrap()];
            let size = |i: usize| blink[i].iter().map(|v| v * v).sum::<f32>().sqrt();
            let peak = (0..count).map(size).fold(0.0f32, f32::max).max(1e-9);
            let moving: Vec<usize> = (0..count).filter(|i| size(*i) > peak * 0.02).collect();
            let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
            for i in if robot || moving.is_empty() { (0..count).collect::<Vec<_>>() } else { moving } { for k in 0..3 { lo[k] = lo[k].min(part.p.positions[i][k]); hi[k] = hi[k].max(part.p.positions[i][k]); } }
            let (width, height) = ((hi[lateral] - lo[lateral]).max(1e-6), (hi[1] - lo[1]).max(1e-6));
            let (cl, cy) = ((hi[lateral] + lo[lateral]) / 2.0, (hi[1] + lo[1]) / 2.0);
            // ルクスの画面は傾いた平らな面なので、面に沿って動かす（面の外へ出ると、画面の裏に隠れて欠ける）
            let plane: Option<[f32; 3]> = if robot && count >= 3 {
                let c = centre(&part.p.positions);
                let sub = |p: [f32; 3]| [p[0] - c[0], p[1] - c[1], p[2] - c[2]];
                let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
                let len = |a: [f32; 3]| (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
                let a = part.p.positions.iter().map(|p| sub(*p)).fold([0.0f32; 3], |best, v| if len(v) > len(best) { v } else { best });
                let n = part.p.positions.iter().map(|p| cross(a, sub(*p))).fold([0.0f32; 3], |best, v| if len(v) > len(best) { v } else { best });
                let l = len(n);
                if l > 1e-12 { Some([n[0] / l, n[1] / l, n[2] / l]) } else { None }
            } else { None };
            // 本人の左の向き（左目のある側）
            let leftward = if left[lateral] >= right[lateral] { 1.0 } else { -1.0 };
            LOOK.iter().map(|(dl, dy)| (0..count).map(|i| {
                let p = part.p.positions[i];
                let (u, v) = ((p[lateral] - cl) / (width * 0.5), (p[1] - cy) / (height * 0.5));
                let mut d = [0.0f32; 3];
                if robot {
                    // ルクス: 光る目は画面の決まった範囲にしか出ないので、目ごとは動かさず、見る向きと反対の端だけを寄せる（目が見る側へ寄って細くなる）
                    let (sl, sy) = (dl.signum() * leftward, dy.signum());
                    let trailing = if *dl != 0.0 { ((1.0 - u * sl) * 0.5).clamp(0.0, 1.0) } else { ((1.0 - v * sy) * 0.5).clamp(0.0, 1.0) };
                    d[lateral] = dl * leftward * width * 2.4 * trailing;
                    d[1] = dy * height * 1.1 * trailing;
                } else {
                    // 目の中心ほど大きく、まわりへなだらかに 0（まぶたのふちの外は動かさない）
                    let w = 1.0 - smooth(0.15, 1.0, (u * u + v * v).sqrt());
                    d[lateral] = dl * leftward * width * w;
                    d[1] = dy * height * w;
                }
                if let Some(n) = plane { let k = d[0] * n[0] + d[1] * n[1] + d[2] * n[2]; for a in 0..3 { d[a] -= n[a] * k; } }
                d
            }).collect()).collect()
        });
        if deltas.is_none() && looks.is_none() { continue; }
        let deltas = deltas.unwrap_or_default();
        // モーフの数と名前の数をそろえてから足す（名前の無いモーフがあっても番号がずれないように）
        while part.targets.len() < part.p.morph_pos.len() { part.targets.push(String::new()); }
        part.targets.truncate(part.p.morph_pos.len());
        let normals = !part.p.morph_nor.is_empty();
        for (name, delta) in EMOTIONS.iter().zip(deltas).chain(LOOKS.iter().zip(looks.unwrap_or_default())) {
            part.targets.push((*name).to_string());
            part.p.morph_pos.push(delta);
            if normals { part.p.morph_nor.push(vec![[0.0; 3]; count]); }
        }
    }
}
