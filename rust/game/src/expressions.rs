// ファンタジーの表情（今の版は lib/character-maker/expressions.ts の createMakerExpressions。式と順はそちら）。
// 形の側（build）: 顔の部分へ、目を閉じたときの補正・男性の目の面を奥へ下げる・男性の ＞＜ の板のモーフを足す。
// 当てる側（Expressions::apply）: 選んだ表情の重み・補正・目の面・視線。
use character_actor::glb::{Glb, Primitive};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// 男性の ＞＜ の板を動かすモーフ（女性の EyeExtra_On の実測に合わせた値）
const HURT_BOARD: (&str, [f64; 3], [f64; 3]) = ("makerHurtBoard", [0.0077, 0.0035, -0.037], [2.25, 1.85, 1.0]);
const SOURCE_PRESETS: [(&str, &str); 12] = [
    ("happy", "joy"), ("angry", "angry"), ("sad", "sorrow"), ("relaxed", "fun"),
    ("aa", "a"), ("ih", "i"), ("ou", "u"), ("ee", "e"), ("oh", "o"),
    ("blink", "blink"), ("blinkLeft", "blink_l"), ("blinkRight", "blink_r"),
];

/// 部分ごとのモーフの名前（three の morphTargetDictionary。GLB の targetNames のあとに足した名前）
pub type PrimNames = HashMap<(usize, usize), Vec<String>>;

/// 描く節点 1 つ（three の Mesh 1 つ＝GLB の部分 1 つ）のモーフの名前と重み
pub struct PrimMorph { pub node: island_kit::NodeId, pub glb_node: usize, pub mesh: usize, pub prim: usize, pub names: Vec<String>, pub weights: Vec<f32> }

/// 部分の材質の名前
fn mat_name(g: &Glb, p: &Primitive) -> String { p.material.map_or(String::new(), |m| g.materials[m].name.clone()) }
fn is_eye_surface(name: &str) -> bool {
    ["_EyeIris_", "_EyeHighlight_", "_EyeWhite_", "_EyeExtra_"].iter().any(|k| name.contains(k)) && name.ends_with("_EYE")
}
fn used(p: &Primitive) -> Vec<usize> {
    if p.indices.is_empty() { return (0..p.positions.len()).collect(); }
    let mut seen = vec![false; p.positions.len()];
    for i in &p.indices { seen[*i as usize] = true; }
    // three の Set の順（添字の現れた順）。値の計算は順によらない（中心は平均、範囲は min・max）
    let mut out = Vec::new();
    let mut done = vec![false; p.positions.len()];
    for i in &p.indices { let i = *i as usize; if seen[i] && !done[i] { done[i] = true; out.push(i); } }
    out
}
fn push_target(p: &mut Primitive, pos: Vec<[f32; 3]>) {
    let n = p.positions.len();
    p.morph_pos.push(pos);
    // 法線の差分を持つ形には 0 の差分を足す（three の zeroNormals）
    if !p.morph_nor.is_empty() { p.morph_nor.push(vec![[0.0; 3]; n]); }
}

/// 形の側の結果（当てる側が使う）
#[derive(Default, Clone)]
pub struct Built {
    /// (名前, 目の形 'round'|'slender', 左右)
    pub corrections: Vec<(String, String, char)>,
    /// (名前, 左右, ＞＜ の板の部分か)
    pub occlusions: Vec<(String, char, bool)>,
    /// ＞＜ の板のモーフを持つ部分がある（男性）
    pub hurt_board: bool,
}

/// 顔の部分へ補正のモーフを足す（GLB の写しを書き換える）。names は部分ごとのモーフの名前（ここで足す）
pub fn build(g: &mut Glb, names: &mut PrimNames, masculine: bool) -> Built {
    let mut out = Built::default();
    let extra_group = g.json["extras"]["sourceVrmExpressions"]["blendShapeGroups"].as_array().is_some_and(|a| a.iter().any(|x| x["name"] == "Extra"));
    let mat_names: Vec<Vec<String>> = g.meshes.iter().map(|m| m.primitives.iter().map(|p| mat_name(g, p)).collect()).collect();
    for (mi, mesh) in g.meshes.iter_mut().enumerate() {
        if !mesh.target_names.iter().any(|n| n == "eyesRound") { continue; }
        let k_round = mesh.target_names.iter().position(|n| n == "eyesRound");
        let k_slender = mesh.target_names.iter().position(|n| n == "eyesSlender");
        for (pi, p) in mesh.primitives.iter_mut().enumerate() {
            let list = names.entry((mi, pi)).or_default();
            for (eyes, key) in [("round", k_round), ("slender", k_slender)] {
                let Some(key) = key else { continue };
                for side in ['L', 'R'] {
                    let delta = p.morph_pos[key].clone();
                    let vals: Vec<[f32; 3]> = p.positions.iter().zip(&delta).map(|(q, d)| {
                        let t = ((q[0] as f64 + 0.006) / 0.012).clamp(0.0, 1.0);
                        let right = t * t * (3.0 - 2.0 * t);
                        let mask = if side == 'L' { 1.0 - right } else { right };
                        [(-(d[0] as f64) * mask) as f32, (-(d[1] as f64) * mask) as f32, (-(d[2] as f64) * mask) as f32]
                    }).collect();
                    let name = format!("makerClosed{}{side}", if eyes == "round" { "eyesRound" } else { "eyesSlender" });
                    push_target(p, vals);
                    list.push(name.clone());
                    if pi == 0 || !out.corrections.iter().any(|c| c.0 == name) { out.corrections.push((name, eyes.to_string(), side)); }
                }
            }
            let mname = &mat_names[mi][pi];
            let eye_surface = is_eye_surface(mname);
            let board = !extra_group && mname.contains("_EyeExtra_") && mname.ends_with("_EYE");
            if masculine && eye_surface {
                let ids = used(p);
                for side in ['L', 'R'] {
                    let mut vals = vec![[0.0f32; 3]; p.positions.len()];
                    for &i in &ids {
                        let t = ((p.positions[i][0] as f64 + 0.006) / 0.012).clamp(0.0, 1.0);
                        let right = t * t * (3.0 - 2.0 * t);
                        let mask = if side == 'L' { 1.0 - right } else { right };
                        vals[i] = [0.0, (0.035 * mask) as f32, (0.025 * mask) as f32];
                    }
                    let name = format!("makerClosedEyeSurface{side}");
                    push_target(p, vals);
                    list.push(name.clone());
                    out.occlusions.push((name, side, board));
                }
            }
            if board {
                let ids = used(p);
                let mid = ids.iter().map(|&i| p.positions[i][0] as f64).sum::<f64>() / (ids.len().max(1) as f64);
                let mut vals = vec![[0.0f32; 3]; p.positions.len()];
                for side in [1.0f64, -1.0] {
                    let mine: Vec<usize> = ids.iter().copied().filter(|&i| ((p.positions[i][0] as f64) - mid).signum() == side && (p.positions[i][0] as f64) != mid).collect();
                    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
                    for &i in &mine { for k in 0..3 { let v = p.positions[i][k] as f64; lo[k] = lo[k].min(v); hi[k] = hi[k].max(v); } }
                    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
                    let goal = [c[0] + side * HURT_BOARD.1[0], c[1] + HURT_BOARD.1[1], c[2] + HURT_BOARD.1[2]];
                    for &i in &mine { for k in 0..3 { let v = p.positions[i][k] as f64; vals[i][k] = (goal[k] + (v - c[k]) * HURT_BOARD.2[k] - v) as f32; } }
                }
                push_target(p, vals);
                list.push(HURT_BOARD.0.to_string());
                out.hurt_board = true;
            }
        }
    }
    out
}

/// 表情 1 つ: (モーフの名前, 値)・0/1 だけか
#[derive(Clone)]
struct Expr { binds: Vec<(String, f32)>, binary: bool }

/// 当てる側
#[derive(Clone)]
pub struct Expressions {
    map: HashMap<String, Expr>,
    /// 表情が書くモーフの名前（当てる前に 0 にする）
    owned: HashSet<String>,
    built: Built,
    /// 書き出し用（今の版の profile: 表情 → モーフの名前 → 値）
    pub profile: serde_json::Map<String, Value>,
}

impl Expressions {
    /// GLB の extras（sourceVrmExpressions・sourceMeshNames）から読む
    pub fn read(g: &Glb, built: &Built) -> Result<Expressions, String> {
        let groups = g.json["extras"]["sourceVrmExpressions"]["blendShapeGroups"].as_array().ok_or("表情データを読み込めませんでした。")?;
        let mesh_names = g.json["extras"]["sourceMeshNames"].as_array().ok_or("表情データを読み込めませんでした。")?;
        let target_of = |bind: &Value| -> Option<String> {
            let name = mesh_names.get(bind["mesh"].as_u64()? as usize)?.as_str()?;
            let def = g.json["meshes"].as_array()?.iter().find(|m| m["name"].as_str() == Some(name))?;
            def["extras"]["targetNames"].get(bind["index"].as_u64()? as usize)?.as_str().map(str::to_string)
        };
        let exists = |name: &str| g.meshes.iter().any(|m| m.target_names.iter().any(|n| n == name));
        let mut map = HashMap::new();
        let mut owned = HashSet::new();
        let mut profile = serde_json::Map::new();
        let keys: Vec<(&str, Option<&str>)> = SOURCE_PRESETS.iter().map(|(k, p)| (*k, Some(*p))).chain(std::iter::once(("surprised", None))).collect();
        for (key, preset) in keys {
            let group = groups.iter().find(|x| match preset { Some(p) => x["presetName"].as_str() == Some(p), None => x["name"].as_str() == Some("Surprised") })
                .ok_or(format!("対応する表情を読み込めませんでした: {key}"))?;
            if group["materialValues"].as_array().is_some_and(|a| !a.is_empty()) { return Err(format!("対応する表情を読み込めませんでした: {key}")); }
            let mut binds = Vec::new();
            let mut prof = serde_json::Map::new();
            for b in group["binds"].as_array().into_iter().flatten() {
                let t = target_of(b).ok_or(format!("表情の対応先がありません: {key}"))?;
                if !exists(&t) { return Err(format!("表情の描画先がありません: {key}")); }
                let v = b["weight"].as_f64().unwrap_or(0.0) / 100.0;
                owned.insert(t.clone());
                prof.insert(t.clone(), serde_json::json!(v));
                binds.push((t, v as f32));
            }
            profile.insert(key.to_string(), Value::Object(prof));
            map.insert(key.to_string(), Expr { binds, binary: group["isBinary"].as_bool() == Some(true) });
        }
        // ＞＜（保存しない一時の顔）。女性は原本の Extra、男性は目を閉じる＋板を前へ
        let mut binds = Vec::new();
        let mut prof = serde_json::Map::new();
        if let Some(extra) = groups.iter().find(|x| x["name"] == "Extra") {
            for b in extra["binds"].as_array().into_iter().flatten() {
                let t = target_of(b).ok_or("表情の対応先がありません: hurt")?;
                let v = b["weight"].as_f64().unwrap_or(0.0) / 100.0;
                if exists(&t) { owned.insert(t.clone()); binds.push((t.clone(), v as f32)); }
                prof.insert(t, serde_json::json!(v));
            }
        } else {
            let blink = map.get("blink").cloned().ok_or("＞＜ の表情を読み込めませんでした。")?;
            binds.extend(blink.binds);
            if let Some(Value::Object(b)) = profile.get("blink") { prof.extend(b.clone()); }
            if built.hurt_board { owned.insert(HURT_BOARD.0.to_string()); binds.push((HURT_BOARD.0.to_string(), 1.0)); prof.insert(HURT_BOARD.0.into(), serde_json::json!(1)); }
        }
        if binds.is_empty() { return Err("＞＜ の表情を読み込めませんでした。".into()); }
        profile.insert("hurt".into(), Value::Object(prof));
        map.insert("hurt".into(), Expr { binds, binary: false });
        Ok(Expressions { map, owned, built: built.clone(), profile })
    }

    /// apply(config, hurt): 部分ごとの重みを書き換える（key は ＞＜ の間は 'hurt'、strength は ＞＜ の間は 1）
    pub fn apply(&mut self, prims: &mut [PrimMorph], key: &str, strength: f32, eyes: &str) {
        for p in prims.iter_mut() { for (k, n) in p.names.iter().enumerate() { if self.owned.contains(n) && k < p.weights.len() { p.weights[k] = 0.0; } } }
        if let Some(e) = self.map.get(key) {
            let value = if e.binary { if strength >= 0.5 { 1.0 } else { 0.0 } } else { strength };
            for (t, v) in &e.binds {
                for p in prims.iter_mut() { if let Some(k) = p.names.iter().position(|n| n == t) && k < p.weights.len() { p.weights[k] += value * v; } }
            }
        }
        let closes_for = |side: char| key == "blink" || key == "happy" || key == "hurt" || key == if side == 'L' { "blinkLeft" } else { "blinkRight" };
        for (name, shape, side) in &self.built.corrections {
            let v = if closes_for(*side) && eyes == shape { strength } else { 0.0 };
            for p in prims.iter_mut() { if let Some(k) = p.names.iter().position(|n| n == name) && k < p.weights.len() { p.weights[k] = v; } }
        }
        for (name, side, board) in &self.built.occlusions {
            let closes = key == "blink" || key == "happy" || (key == "hurt" && !board) || key == if *side == 'L' { "blinkLeft" } else { "blinkRight" };
            let t = if closes { ((strength - 0.75) / 0.25).clamp(0.0, 1.0) } else { 0.0 };
            let v = t * t * (3.0 - 2.0 * t);
            // 目の面のモーフは部分ごと（板の部分とそうでない部分で同じ名前）。板かどうかで分ける
            for p in prims.iter_mut() {
                let is_board = p.names.iter().any(|n| n == HURT_BOARD.0);
                if is_board != *board { continue; }
                if let Some(k) = p.names.iter().position(|n| n == name) && k < p.weights.len() { p.weights[k] = v; }
            }
        }
        // 書き出しの profile（全部の強さの表情に、同じ目ごとの補正と目の面を入れる）
        for expression in ["blink", "blinkLeft", "blinkRight", "happy", "hurt"] {
            let Some(Value::Object(p)) = self.profile.get_mut(expression) else { continue };
            for (name, shape, side) in &self.built.corrections {
                let active = expression == "blink" || expression == "happy" || expression == "hurt" || expression == if *side == 'L' { "blinkLeft" } else { "blinkRight" };
                if active && eyes == shape { p.insert(name.clone(), serde_json::json!(1)); } else { p.remove(name); }
            }
            for (name, side, board) in &self.built.occlusions {
                let active = expression == "blink" || expression == "happy" || (expression == "hurt" && !board) || expression == if *side == 'L' { "blinkLeft" } else { "blinkRight" };
                if active { p.insert(name.clone(), serde_json::json!(1)); } else if !(expression == "hurt" && *board) { p.remove(name); }
            }
        }
    }
}
