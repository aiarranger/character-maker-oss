// オリジナル 4 体の組み立ての前の手直し（今の版は lib/character-maker/original-species.ts・original-human-parts.ts・original-body.ts・
// original-hair.ts・original-skin-patch.mjs・original-weapon.ts の attachClosedHand）。式と順はそちらと同じ。
// 形は 1 体ごとの写し（Part の p）を書き換える。元の GLB とその画像は変えない。
use crate::original::{MatInfo, Part, PartKind};
use character_actor::glb::{self, Glb};
use glam::{Mat3, Mat4, Vec3};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

fn smooth(a: f64, b: f64, v: f64) -> f64 { let t = ((v - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }
fn clamp01(v: f64) -> f64 { v.clamp(0.0, 1.0) }
fn num(v: &Value) -> f64 { v.as_f64().unwrap_or(0.0) }

/// 種族の部品の選び（OriginalSpeciesParts）
pub struct SpeciesParts { pub head: String, pub ears: String, pub eyes: String, pub mouth: String, pub coat: String, pub accent: String, pub eye_color: String }
impl SpeciesParts {
    pub fn read(v: &Value) -> SpeciesParts {
        let s = |k: &str, d: &str| v[k].as_str().unwrap_or(d).to_lowercase();
        SpeciesParts { head: s("head", "original"), ears: s("ears", "original"), eyes: s("eyes", "original"), mouth: s("mouth", "original"), coat: s("coatColor", "source"), accent: s("accentColor", "source"), eye_color: s("eyeColor", "source") }
    }
}

/// '#rrggbb' → sRGB の 0..255（three の Color を通した値: sRGB → 線形 → sRGB で丸める）
fn srgb_bytes(hex: &str) -> [f64; 3] {
    let c = island_kit::Color::css(hex);
    let enc = |v: f32| { let v = v as f64; (255.0 * if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }).round() };
    [enc(c.r), enc(c.g), enc(c.b)]
}

fn tint(rgb: [f64; 3], region: usize, colors: &[Option<[f64; 3]>; 5], robot: bool) -> [f64; 3] {
    let Some(target) = colors[region] else { return rgb };
    let light = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    let normal = if robot { 210.0 } else { match region { 2 => 66.0, 3 => 224.0, 4 => 130.0, _ => 139.0 } };
    let shade = (light / normal).clamp(0.32, 1.4);
    let f = |c: f64| ((if region == 3 { 192.0 + c * 0.25 } else { c }) * shade).clamp(0.0, 255.0);
    [f(target[0]), f(target[1]), f(target[2])]
}

fn palette(p: &SpeciesParts) -> [Option<[f64; 3]>; 5] {
    let c = |s: &str| if s != "source" { Some(srgb_bytes(s)) } else { None };
    [None, c(&p.coat), c(&p.accent), c(&p.coat), c(&p.eye_color)]
}

/// coloredMap（決まった塗り分けの帯ごとに色を替える。帯は作る道具が測った物）
pub fn colored_map(src: &glb::Image, mask: &Value, robot: bool, parts: &SpeciesParts) -> Result<glb::Image, String> {
    if mask["version"].as_str() != Some("original-species-mask-v1") || mask["width"].as_u64() != Some(src.width as u64) || mask["height"].as_u64() != Some(src.height as u64) {
        return Err("パーツの色画像を読み込めませんでした。".into());
    }
    let colors = palette(parts);
    let original = &src.rgba;
    let mut out = src.rgba.clone();
    let px = |at: usize| [original[at] as f64, original[at + 1] as f64, original[at + 2] as f64];
    for run in mask["runs"].as_array().into_iter().flatten() {
        let (start, count, region) = (num(&run[0]) as usize, num(&run[1]) as usize, num(&run[2]) as usize);
        if region >= 5 || colors[region].is_none() { continue; }
        for pixel in start..start + count {
            let at = pixel * 4;
            if at + 3 >= out.len() { break; }
            let c = tint(px(at), region, &colors, robot);
            for k in 0..3 { out[at + k] = c[k].round() as u8; }
        }
    }
    for run in mask["softFurRuns"].as_array().into_iter().flatten() {
        let (start, count) = (num(&run[0]) as usize, num(&run[1]) as usize);
        let w = [num(&run[2]), num(&run[3]), num(&run[4])];
        for pixel in start..start + count {
            let at = pixel * 4;
            if at + 3 >= out.len() { break; }
            let rgb = px(at);
            let mut result = [0.0; 3];
            for (i, region) in [1usize, 2, 3].iter().enumerate() {
                let c = tint(rgb, *region, &colors, robot);
                for k in 0..3 { result[k] += c[k] * w[i] / 255.0; }
            }
            for k in 0..3 { out[at + k] = result[k].round().clamp(0.0, 255.0) as u8; }
        }
    }
    Ok(glb::Image { width: src.width, height: src.height, rgba: out })
}

/// GLB の生のバイト列から accessor を f32 の列で読む（_SPECIES_* のような独自の頂点の値。meshopt は使っていない模型だけ）
pub fn raw_accessor(raw: &[u8], json: &Value, idx: usize) -> Option<Vec<f32>> {
    let jlen = u32::from_le_bytes(raw.get(12..16)?.try_into().ok()?) as usize;
    let bin_at = 20 + jlen;
    let bin = raw.get(bin_at + 8..)?;
    let a = &json["accessors"][idx];
    let comps = match a["type"].as_str()? { "SCALAR" => 1, "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, _ => return None };
    let ct = a["componentType"].as_u64()? as usize;
    let count = a["count"].as_u64()? as usize;
    let norm = a["normalized"].as_bool().unwrap_or(false);
    let view = &json["bufferViews"][a["bufferView"].as_u64()? as usize];
    if view.get("extensions").and_then(|e| e.get("EXT_meshopt_compression")).is_some() { return None; }
    let size = match ct { 5120 | 5121 => 1, 5122 | 5123 => 2, _ => 4 };
    let stride = view["byteStride"].as_u64().map(|s| s as usize).filter(|s| *s > 0).unwrap_or(comps * size);
    let off = view["byteOffset"].as_u64().unwrap_or(0) as usize + a["byteOffset"].as_u64().unwrap_or(0) as usize;
    let mut out = Vec::with_capacity(count * comps);
    for i in 0..count {
        for c in 0..comps {
            let at = off + i * stride + c * size;
            let d = bin.get(at..at + size)?;
            out.push(match ct {
                5120 => { let v = d[0] as i8 as f32; if norm { (v / 127.0).max(-1.0) } else { v } }
                5121 => { let v = d[0] as f32; if norm { v / 255.0 } else { v } }
                5122 => { let v = i16::from_le_bytes([d[0], d[1]]) as f32; if norm { (v / 32767.0).max(-1.0) } else { v } }
                5123 => { let v = u16::from_le_bytes([d[0], d[1]]) as f32; if norm { v / 65535.0 } else { v } }
                5125 => u32::from_le_bytes([d[0], d[1], d[2], d[3]]) as f32,
                _ => f32::from_le_bytes([d[0], d[1], d[2], d[3]]),
            });
        }
    }
    Some(out)
}

/// three の computeVertexNormals（index のある形）。形の差分の法線は変えない
fn recompute_normals(p: &mut glb::Primitive) {
    if p.normals.is_empty() { return; }
    p.normals.clear();
    glb::compute_normals(p);
}

/// configureOriginalSpecies（ネコとロボットだけ）。形（頭・目・耳・口の歪み）と色（塗り分け・画面の色）。
/// 返り値: 材質ごとの色替えした画像（材質の番号 → 画像）
pub fn species(parts_list: &mut [Part], mats: &mut [MatInfo], g: &Glb, character: &str, parts: Option<&SpeciesParts>, raw: Option<&[u8]>) -> Result<(), String> {
    let Some(parts) = parts else { return Ok(()) };
    let (cat, robot) = (character == "generic-cat", character == "generic-robot");
    if !cat && !robot { return Ok(()); }
    let changed_shape = [&parts.head, &parts.ears, &parts.eyes, &parts.mouth].iter().any(|s| s.as_str() != "original");
    let changed_color = [&parts.coat, &parts.accent, &parts.eye_color].iter().any(|s| s.as_str() != "source");
    if !changed_shape && !changed_color && !cat { return Ok(()); }
    // 画面は曲がっている。動かした目と口が画面の面から出ないように、画面の三角形（頭と同じ歪みを当てた物）の奥行きを読む
    let panel = parts_list.iter().find(|p| p.name == "Lux_DisplayOverlay").map(|p| (p.p.positions.clone(), p.p.indices.clone()));
    let head_kind = parts.head.clone();
    let panel_depth = |y: f64, z: f64| -> f64 {
        let Some((pos, idx)) = &panel else { return f64::NEG_INFINITY };
        let point = |i: usize| {
            let v = pos[i];
            let (mut py, mut pz) = (v[1] as f64, v[2] as f64);
            if head_kind == "rounded" { py += (py - 0.705) * 0.15; pz *= 0.83; }
            if head_kind == "wide" { py -= (py - 0.705) * 0.12; pz *= 1.14; }
            [v[0] as f64, py, pz]
        };
        let mut depth = f64::NEG_INFINITY;
        for t in idx.chunks_exact(3) {
            let (a, b, c) = (point(t[0] as usize), point(t[1] as usize), point(t[2] as usize));
            let den = (b[2] - c[2]) * (a[1] - c[1]) + (c[1] - b[1]) * (a[2] - c[2]);
            if den.abs() < 1e-10 { continue; }
            let u = ((b[2] - c[2]) * (y - c[1]) + (c[1] - b[1]) * (z - c[2])) / den;
            let v = ((c[2] - a[2]) * (y - c[1]) + (a[1] - c[1]) * (z - c[2])) / den;
            let w = 1.0 - u - v;
            if u >= -0.001 && v >= -0.001 && w >= -0.001 { depth = depth.max(u * a[0] + v * b[0] + w * c[0]); }
        }
        depth
    };
    let mut found = false;
    for part in parts_list.iter_mut() {
        let Some(role) = part.user["originalSpeciesPart"].as_str().map(str::to_string) else { continue };
        found = true;
        if changed_shape {
            // _species_* の重み（作る道具が付けた、頭・目・口・耳の場所）
            let names = ["_SPECIES_HEAD", "_SPECIES_EYE_L", "_SPECIES_EYE_R", "_SPECIES_MOUTH", "_SPECIES_EAR_L", "_SPECIES_EAR_R"];
            let prim_json = &g.json["meshes"][part.mesh]["primitives"][part.prim];
            let weights: Vec<Option<Vec<f32>>> = names.iter().map(|n| {
                let idx = prim_json["attributes"].get(*n).or_else(|| prim_json["attributes"].get(n.to_lowercase())).and_then(Value::as_u64)?;
                raw_accessor(raw?, &g.json, idx as usize)
            }).collect();
            if raw.is_none() { return Err("このキャラクターの編集用パーツを読み込めませんでした。".into()); }
            let w = |k: usize, i: usize| weights[k].as_ref().and_then(|v| v.get(i)).copied().unwrap_or(0.0) as f64;
            let transform = |x: f64, y: f64, z: f64, vertex: usize| -> [f64; 3] {
                let (mut nx, mut ny, mut nz) = (x, y, z);
                let head = w(0, vertex);
                if head != 0.0 && parts.head != "original" {
                    if cat {
                        if parts.head == "slim" { nz += z * -0.15 * head; ny += (y - 0.78) * 0.035 * head; }
                        if parts.head == "cheeks" { let cheeks = (-((y - 0.747) / 0.085).powi(2)).exp(); nz += z * 0.19 * cheeks * head; nx += 0.008 * cheeks * head; }
                    } else {
                        let (cy, hy) = (0.705, y.min(0.868));
                        if parts.head == "rounded" { nz += z * -0.17 * head; ny += ((hy - cy) * 0.15) * head; }
                        if parts.head == "wide" { nz += z * 0.14 * head; ny -= (hy - cy) * 0.12 * head; }
                    }
                }
                for side in 0..2 {
                    let eye = w(1 + side, vertex);
                    if eye != 0.0 && parts.eyes != "original" {
                        let cy = if cat { 0.802 } else { 0.704 };
                        let cz = (if side == 0 { 1.0 } else { -1.0 }) * if cat { if side == 0 { 0.0875 } else { 0.0825 } } else { 0.100 };
                        let sx = if parts.eyes == "large" { 1.15 } else if robot { 1.45 } else { 1.25 };
                        let sy = if parts.eyes == "large" { 1.20 } else if robot { 0.40 } else { 0.66 };
                        nz += (z - cz) * (sx - 1.0) * eye; ny += (y - cy) * (sy - 1.0) * eye;
                    }
                    let ear = w(4 + side, vertex);
                    if ear != 0.0 && parts.ears != "original" {
                        let t = clamp01((y - 0.874) / 0.126);
                        let cz = (if side == 0 { 1.0 } else { -1.0 }) * 0.142;
                        if parts.ears == "rounded" { ny -= 0.052 * t * t * ear; nz += (z - cz) * 0.17 * ear; }
                        if parts.ears == "folded" { nx += 0.085 * t * t * ear; ny -= 0.070 * t * t * ear; }
                    }
                }
                let mouth = w(3, vertex);
                if mouth != 0.0 && parts.mouth != "original" {
                    let cy = if cat { 0.704 } else { 0.615 };
                    nz += z * (if parts.mouth == "small" { -0.28 } else { 0.27 }) * mouth;
                    ny += (y - cy) * (if parts.mouth == "small" { -0.22 } else { 0.10 }) * mouth;
                }
                if robot && (role.starts_with("display-eye-") && parts.eyes != "original" || role == "display-mouth" && parts.mouth != "original") { nx = nx.max(panel_depth(ny, nz) + 0.002); }
                [nx, ny, nz]
            };
            let source = part.p.clone();
            for i in 0..source.positions.len() {
                let v = source.positions[i];
                let (x, y, z) = (v[0] as f64, v[1] as f64, v[2] as f64);
                let point = transform(x, y, z, i);
                part.p.positions[i] = [point[0] as f32, point[1] as f32, point[2] as f32];
                for k in 0..source.morph_pos.len() {
                    let d = source.morph_pos[k][i];
                    let target = transform(x + d[0] as f64, y + d[1] as f64, z + d[2] as f64, i);
                    part.p.morph_pos[k][i] = [(target[0] - point[0]) as f32, (target[1] - point[1]) as f32, (target[2] - point[2]) as f32];
                }
            }
            recompute_normals(&mut part.p);
        }
        let mi = part.mat;
        if let Some(mask) = mats[mi].extras.get("originalSpeciesColorMask").cloned() && changed_color {
            let key = format!("species|{}|{}|{}|{}|{}", mats[mi].image_key, character, parts.coat, parts.accent, parts.eye_color);
            mats[mi].derive_with(g, key, |src| colored_map(src, &mask, robot, parts))?;
        }
        if robot && role.starts_with("display-") && role != "display-panel" && parts.accent != "source" {
            let c = island_kit::Color::css(&parts.accent);
            mats[mi].color = [c.r, c.g, c.b];
        }
        // 毛のまぶた（originalSpeciesLid・originalSpeciesLidCoverage）は今の 4 体の模型に無い。lid- の役の頂点の色だけ写す
        let has_fur_lid = !part.user["originalSpeciesLid"].is_null();
        if !has_fur_lid && cat && role.starts_with("lid-") && (parts.coat != "source" || parts.accent != "source") {
            let chosen = if role == "lid-left" { &parts.coat } else { &parts.accent };
            if chosen != "source" && !part.p.colors.is_empty() {
                let c = island_kit::Color::css(chosen);
                let base = if role == "lid-left" { [0.670588, 0.462745, 0.290196] } else { [0.294118, 0.258824, 0.239216] };
                let normal = 0.2126 * base[0] + 0.7152 * base[1] + 0.0722 * base[2];
                for v in part.p.colors.iter_mut() {
                    let light = 0.2126 * v[0] as f64 + 0.7152 * v[1] as f64 + 0.0722 * v[2] as f64;
                    let scale = light / normal;
                    *v = [clamp01(c.r as f64 * scale) as f32, clamp01(c.g as f64 * scale) as f32, clamp01(c.b as f64 * scale) as f32, v[3]];
                }
            }
        }
    }
    if !found { return Err("このキャラクターの編集用パーツを読み込めませんでした。".into()); }
    Ok(())
}

/// configureOriginalHumanParts（人の 2 体。独立した頭と髪の形を、頭の形・目・口の選びで歪める。F(元 + 差分) − F(元)）
pub fn human_parts(parts_list: &mut [Part], character: &str, parts: &SpeciesParts) {
    if !["generic-human-a", "generic-human-b"].contains(&character) { return; }
    if parts.head == "original" && parts.eyes == "original" && parts.mouth == "original" { return; }
    for part in parts_list.iter_mut() {
        let modular = part.user["originalModularPart"].as_str().unwrap_or("");
        if !["head", "hair"].contains(&modular) { continue; }
        let (fit, profile) = (part.user["originalModularFit"].clone(), part.user["originalModularFaceProfile"].clone());
        if fit.is_null() || profile.is_null() { continue; }
        let eye_role = part.geo_user["originalMakerEyeRole"].as_str().or(part.user["originalMakerEyeRole"].as_str()).unwrap_or("").to_string();
        let preserve_iris = eye_role == "iris-sclera" && parts.eyes == "narrow";
        let fs = [num(&fit["scale"][0]), num(&fit["scale"][1]), num(&fit["scale"][2])];
        let fo = [num(&fit["offset"][0]), num(&fit["offset"][1]), num(&fit["offset"][2])];
        let env = [num(&profile["headEnvelope"][0]), num(&profile["headEnvelope"][1]), num(&profile["headEnvelope"][2]), num(&profile["headEnvelope"][3])];
        let eyes: Vec<([f64; 2], [f64; 2])> = profile["eyes"].as_array().into_iter().flatten().map(|e| ([num(&e["center"][0]), num(&e["center"][1])], [num(&e["radius"][0]), num(&e["radius"][1])])).collect();
        let mouth = ([num(&profile["mouth"]["center"][0]), num(&profile["mouth"]["center"][1])], [num(&profile["mouth"]["radius"][0]), num(&profile["mouth"]["radius"][1])]);
        let transform = |x: f64, y: f64, z: f64| -> [f64; 3] {
            // 元の模型は +X が正面で、下見の半分の大きさ
            let mut px = (-2.0 * z - fo[0]) / fs[0];
            let mut py = (2.0 * y - fo[1]) / fs[1];
            let pz = (2.0 * x - fo[2]) / fs[2];
            let (ox, oy) = (px, py);
            let head_env = smooth(env[0], env[1], py) * (1.0 - smooth(env[2], env[3], py));
            if parts.head == "rounded" { px *= 1.0 + 0.105 * head_env; }
            if parts.head == "slim" { px *= 1.0 - 0.10 * head_env; }
            if parts.eyes != "original" && !preserve_iris && !eyes.is_empty() {
                let mut eye = eyes[0];
                for e in &eyes[1..] { if !((ox - eye.0[0]).abs() < (ox - e.0[0]).abs()) { eye = *e; } }
                let (cx, cy) = (eye.0[0], eye.0[1]);
                let radius = ((ox - cx) / eye.1[0]).hypot((oy - cy) / eye.1[1]);
                let amount = (1.0 - smooth(0.68, 1.2, radius)) * smooth(0.02, 0.19, pz);
                let (sx, sy) = if parts.eyes == "large" { (1.13, 1.14) } else { (1.045, 0.77) };
                px += (ox - cx) * (sx - 1.0) * amount;
                py += (oy - cy) * (sy - 1.0) * amount;
            }
            if parts.mouth != "original" {
                let radius = ((ox - mouth.0[0]) / mouth.1[0]).hypot((oy - mouth.0[1]) / mouth.1[1]);
                let amount = (1.0 - smooth(0.72, 1.35, radius)) * smooth(0.02, 0.17, pz);
                let scale = if parts.mouth == "wide" { 1.17 } else { 0.82 };
                px += (ox - mouth.0[0]) * (scale - 1.0) * amount;
            }
            [(pz * fs[2] + fo[2]) / 2.0, (py * fs[1] + fo[1]) / 2.0, -(px * fs[0] + fo[0]) / 2.0]
        };
        let names = part.targets.clone();
        let source = part.p.clone();
        for i in 0..source.positions.len() {
            let v = source.positions[i];
            let (bx, by, bz) = (v[0] as f64, v[1] as f64, v[2] as f64);
            let p = transform(bx, by, bz);
            let before: Vec<[f64; 3]> = source.morph_pos.iter().map(|t| [t[i][0] as f64, t[i][1] as f64, t[i][2] as f64]).collect();
            for index in 0..before.len() {
                let [tx, ty, tz] = before[index];
                let name = names.get(index).map(String::as_str).unwrap_or("");
                // まばたきの途中の補正は、本体のまばたきを f だけ入れた姿からの差
                let corrective = ["blinkLeft", "blinkRight"].iter().find_map(|main| {
                    let rest = name.strip_prefix(main)?.strip_prefix("Inbetween")?;
                    let f: f64 = match rest { "25" => 0.25, "50" => 0.5, "75" => 0.75, _ => return None };
                    Some((*main, f))
                });
                if let Some((main, f)) = corrective && let Some(mi) = names.iter().position(|n| n == main) && mi < before.len() {
                    let m = before[mi];
                    let full = transform(bx + m[0], by + m[1], bz + m[2]);
                    let mid = transform(bx + f * m[0] + tx, by + f * m[1] + ty, bz + f * m[2] + tz);
                    part.p.morph_pos[index][i] = [(mid[0] - p[0] - f * (full[0] - p[0])) as f32, (mid[1] - p[1] - f * (full[1] - p[1])) as f32, (mid[2] - p[2] - f * (full[2] - p[2])) as f32];
                    continue;
                }
                let t = transform(tx + bx, ty + by, tz + bz);
                part.p.morph_pos[index][i] = [(t[0] - p[0]) as f32, (t[1] - p[1]) as f32, (t[2] - p[2]) as f32];
            }
            part.p.positions[i] = [p[0] as f32, p[1] as f32, p[2] as f32];
        }
        recompute_normals(&mut part.p);
    }
}

/// 体型の場（createOriginalBody）。骨付きの部分ごとに、元の位置と 1 あたりの差（形の局所）を返す。骨と動きは変えない
pub struct BodyProfile { width: f64, depth: f64, arm: f64, leg: f64, depth_envelope: Option<(f64, f64)> }
pub fn body_profile(character: &str) -> Result<BodyProfile, String> {
    Ok(match character {
        "generic-human-a" => BodyProfile { width: 0.24, depth: 0.25, arm: 0.18, leg: 0.19, depth_envelope: None },
        "generic-human-b" => BodyProfile { width: 0.22, depth: 0.23, arm: 0.17, leg: 0.18, depth_envelope: None },
        "generic-robot" => BodyProfile { width: 0.16, depth: 0.16, arm: 0.12, leg: 0.13, depth_envelope: None },
        "generic-cat" => BodyProfile { width: 0.24, depth: 0.23, arm: 0.14, leg: 0.16, depth_envelope: Some((0.65, 1.1)) },
        _ => return Err("このキャラクターの体型設定が見つかりません。".into()),
    })
}

pub fn body_field(parts_list: &mut [Part], skel: &character_actor::anim::Skeleton, g: &Glb, character: &str) -> Result<(), String> {
    let profile = body_profile(character)?;
    let joint = |name: &str| -> Result<glam::DVec3, String> { let i = skel.find(name).ok_or("体型を調整する骨を読み込めませんでした。")?; Ok(skel.world[i].w_axis.truncate().as_dvec3()) };
    let (hip, neck) = (joint("Hip")?, joint("NeckTwist01")?);
    let mut up = neck - hip;
    let torso = up.length();
    if torso < 1e-5 { return Err("体型を調整する基準位置を読み込めませんでした。".into()); }
    up = up.normalize();
    let mut right = joint("R_Upperarm")? - joint("L_Upperarm")?;
    right -= up * right.dot(up);
    let right = right.normalize();
    let forward = right.cross(up).normalize();
    struct Limb { start: glam::DVec3, axis: glam::DVec3, length: f64, strength: f64, taper: bool }
    let mut limbs: HashMap<String, usize> = HashMap::new();
    let mut list: Vec<Limb> = Vec::new();
    for side in ["L", "R"] {
        for (part, end, strength, taper) in [("Upperarm", "Forearm", profile.arm, false), ("Forearm", "Hand", profile.arm, true), ("Thigh", "Calf", profile.leg, false), ("Calf", "Foot", profile.leg, true)] {
            let (Ok(start), Ok(e)) = (joint(&format!("{side}_{part}")), joint(&format!("{side}_{end}"))) else { return Err("体型を調整する骨を読み込めませんでした。".into()) };
            let axis = e - start;
            let length = axis.length();
            if length < 1e-5 { continue; }
            list.push(Limb { start, axis: axis / length, length, strength, taper });
            for suffix in ["", "Twist01", "Twist02"] { limbs.insert(format!("{side}_{part}{suffix}"), list.len() - 1); }
        }
    }
    let torso_bones = ["Hip", "Pelvis", "Waist", "Spine01", "Spine02", "L_Clavicle", "R_Clavicle"];
    for part in parts_list.iter_mut() {
        let Some(skin) = part.skin else { continue };
        if part.p.joints.is_empty() || part.p.weights.is_empty() { continue; }
        let joints = &g.skins[skin].joints;
        // 形の局所 → 模型（mesh の世界。模型の根は単位）
        let mesh_to_model = part.bind.as_dmat4();
        let model_to_mesh = glam::DMat3::from_mat4(mesh_to_model.inverse());
        let n = part.p.positions.len();
        let base = part.p.positions.clone();
        let mut delta = vec![[0f32; 3]; n];
        for v in 0..n {
            let point = mesh_to_model.transform_point3(Vec3::from(base[v]).as_dvec3());
            let mut d = glam::DVec3::ZERO;
            // three の skinWeight は GLTFLoader で足して 1 に直した値
            let w = part.weights_n[v];
            let mut relative = glam::DVec3::ZERO;
            for k in 0..4 {
                let weight = w[k] as f64;
                if weight <= 0.0 { continue; }
                let Some(&bone) = joints.get(part.p.joints[v][k] as usize) else { continue };
                let name = skel.names[bone].as_str();
                if torso_bones.contains(&name) {
                    relative = point - hip;
                    let height = relative.dot(up) / torso;
                    let fade = 1.0 - smooth(0.82, 1.0, height);
                    let offset = right * (relative.dot(right) * profile.width) + forward * (relative.dot(forward) * profile.depth);
                    d += offset * (weight * fade);
                } else if let Some(&li) = limbs.get(name) {
                    let limb = &list[li];
                    relative = point - limb.start;
                    let projection = relative.dot(limb.axis);
                    let fade = if limb.taper { 1.0 - smooth(0.72, 1.0, projection / limb.length) } else { 1.0 };
                    let offset = relative - limb.axis * projection;
                    d += offset * (weight * fade * limb.strength);
                }
            }
            let _ = relative;
            if let Some((a, b)) = profile.depth_envelope {
                let depth = (point - hip).dot(forward).abs() / torso;
                d *= 1.0 - smooth(a, b, depth);
            }
            let local = model_to_mesh * d;
            delta[v] = [local.x as f32, local.y as f32, local.z as f32];
        }
        part.body = Some((base, delta));
    }
    Ok(())
}

/// 灰色の髪の絵（colorableMap: 毛の陰を残したまま明るい色も乗るように）
pub fn grayscale(src: &glb::Image) -> glb::Image {
    let mut out = src.rgba.clone();
    for px in out.chunks_exact_mut(4) {
        let value = 0.2126 * px[0] as f64 + 0.7152 * px[1] as f64 + 0.0722 * px[2] as f64;
        let neutral = (255.0 * (0.47 + 0.72 * value / 255.0).min(1.0)).round() as u8;
        px[0] = neutral; px[1] = neutral; px[2] = neutral;
    }
    glb::Image { width: src.width, height: src.height, rgba: out }
}

/// applyOriginalSkinPatch（髪型を替えたときの顔の画像の手直し。決まった多角形を、きれいな肌の行の平均でぼかして塗る）
pub fn skin_patch(src: &glb::Image, profile: &Value) -> Result<glb::Image, String> {
    let (width, height) = (src.width as usize, src.height as usize);
    if profile["version"].as_str() != Some("original-skin-patch-v1") || profile["width"].as_u64() != Some(width as u64) || profile["height"].as_u64() != Some(height as u64) || src.rgba.len() != width * height * 4 {
        return Err("Character skin correction does not match its source atlas.".into());
    }
    let pixels = &src.rgba;
    let mut output = pixels.clone();
    let (row_start, row_end) = (num(&profile["samples"]["rowStart"]) as usize, num(&profile["samples"]["rowEnd"]) as usize);
    let count = (row_end - row_start + 1) as f64;
    let feather = num(&profile["feather"]);
    for polygon in profile["polygons"].as_array().into_iter().flatten() {
        let poly: Vec<(f64, f64)> = polygon.as_array().into_iter().flatten().map(|p| (num(&p[0]), num(&p[1]))).collect();
        if poly.is_empty() { continue; }
        let inside = |x: f64, y: f64| {
            let mut hit = false;
            let mut j = poly.len() - 1;
            for i in 0..poly.len() {
                let ((ax, ay), (bx, by)) = (poly[i], poly[j]);
                if (ay > y) != (by > y) && x < (bx - ax) * (y - ay) / (by - ay) + ax { hit = !hit; }
                j = i;
            }
            hit
        };
        let edge = |x: f64, y: f64| {
            let mut d = f64::INFINITY;
            for i in 0..poly.len() {
                let ((ax, ay), (bx, by)) = (poly[i], poly[(i + 1) % poly.len()]);
                let (dx, dy) = (bx - ax, by - ay);
                let l2 = dx * dx + dy * dy;
                let t = if l2 != 0.0 { (((x - ax) * dx + (y - ay) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
                d = d.min((x - ax - t * dx).hypot(y - ay - t * dy));
            }
            d
        };
        let min_x = poly.iter().map(|p| p.0).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
        let max_x = (poly.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max).ceil() as i64).min(width as i64 - 1);
        let min_y = poly.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).floor().max(0.0) as usize;
        let max_y = (poly.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max).ceil() as i64).min(height as i64 - 1);
        if max_x < 0 || max_y < 0 { continue; }
        for y in min_y..=max_y as usize {
            for x in min_x..=max_x as usize {
                if !inside(x as f64 + 0.5, y as f64 + 0.5) { continue; }
                let t = (edge(x as f64 + 0.5, y as f64 + 0.5) / feather).min(1.0);
                let blend = t * t * (3.0 - 2.0 * t);
                let at = (y * width + x) * 4;
                for c in 0..3 {
                    let mut skin = 0.0;
                    for sy in row_start..=row_end { skin += pixels[(sy * width + x) * 4 + c] as f64; }
                    output[at + c] = (pixels[at + c] as f64 * (1.0 - blend) + skin / count * blend).round() as u8;
                }
            }
        }
    }
    Ok(glb::Image { width: src.width, height: src.height, rgba: output })
}

/// configureOriginalHair: 選んでいない髪の部分を外し、髪型を替えたら顔の画像を直し、髪の色を当てる
pub fn hair(parts_list: &mut Vec<Part>, mats: &mut [MatInfo], g: &Glb, character: &str, hairstyle: &str, hair_color: &str) -> Result<(), String> {
    let removed = |user: &Value| {
        if user["originalModularPart"].as_str() == Some("hair") && user["hairstyleId"].as_str() != Some(hairstyle) { return true; }
        let part = user["originalMakerPart"].as_str().unwrap_or("");
        part == "source-hair" && hairstyle != "original" || part == "scalp" && hairstyle == "original" || part == "short" && hairstyle != "short" || part == "ponytail" && hairstyle != "ponytail"
    };
    parts_list.retain(|p| !removed(&p.user) && !removed(&g.nodes[p.node].extras));
    let default_color = match character { "generic-human-a" => "#383338", "generic-human-b" => "#654b40", "generic-robot" => "#4a737b", "generic-cat" => "#87614b", _ => "#614439" };
    let mut done: HashSet<usize> = HashSet::new();
    for part in parts_list.iter() {
        let mi = part.mat;
        // three は部分ごとに材質を写す。同じ材質を 2 回直さない（結果は同じ）
        if !done.insert(mi) { continue; }
        let m = &mut mats[mi];
        if hairstyle != "original" && !matches!(m.img, crate::original::ImgRef::None) && !m.extras["originalMakerSkinPatch"].is_null() {
            let profile = m.extras["originalMakerSkinPatch"].clone();
            let key = format!("patch|{}|{}", m.image_key, profile);
            m.derive_with(g, key, |src| skin_patch(src, &profile))?;
        }
        let flag = &m.extras["originalMakerHair"];
        if flag.as_bool() == Some(false) || (!flag.as_bool().unwrap_or(false) && !m.name.starts_with("maker-original-hair-")) { continue; }
        let original = m.name.starts_with("maker-original-hair-source") || part.user["originalModularPart"].as_str() == Some("hair");
        if hair_color == "source" && original { continue; }
        let color = if hair_color == "source" { m.extras["defaultHairColor"].as_str().unwrap_or(default_color).to_string() } else { hair_color.to_string() };
        if original && !matches!(m.img, crate::original::ImgRef::None) {
            let key = format!("gray|{}", m.image_key);
            m.derive_with(g, key, |src| Ok(grayscale(src)))?;
        }
        let c = island_kit::Color::css(&color);
        m.color = [c.r, c.g, c.b];
    }
    Ok(())
}

/// attachClosedHand の形の側（人の 2 体で武器を持つとき）: 体の形から右手の三角形を分けて「元の手」の部分にし、
/// 握った手の形（hand.json）を作る。返り値: 握った手の形（位置・色・添字）と付ける骨の名前
pub fn closed_hand(parts_list: &mut Vec<Part>, profile: &Value, body_maps: &[Value]) -> Result<Option<(glb::Primitive, String)>, String> {
    if profile.is_null() { return Ok(None); }
    let node_prefix = profile["source"]["node"].as_str().unwrap_or("");
    let vertex_count = profile["source"]["vertexCount"].as_u64().unwrap_or(0) as usize;
    let source_sha = profile["source"]["sha256"].as_str().unwrap_or("");
    let native_tris: Vec<[usize; 3]> = profile["nativeTriangles"].as_array().into_iter().flatten().map(|t| [num(&t[0]) as usize, num(&t[1]) as usize, num(&t[2]) as usize]).collect();
    let output_count = |m: &Value| -> Result<usize, String> {
        let mut set = HashSet::new();
        for list in m["sourceToOutput"].as_array().into_iter().flatten() { for i in list.as_array().into_iter().flatten() { set.insert(i.as_i64().unwrap_or(-1)); } }
        for i in m["unmatchedOutput"].as_array().into_iter().flatten() { set.insert(i.as_i64().unwrap_or(-1)); }
        let (lo, hi) = (set.iter().min().copied().unwrap_or(0), set.iter().max().copied().unwrap_or(-1));
        if lo != 0 || (hi + 1) as usize != set.len() { return Err("通常の手の頂点対応表が正しくありません。".into()); }
        Ok(set.len())
    };
    let mut native: Option<Part> = None;
    let mut insert_at = 0;
    for (pi, part) in parts_list.iter_mut().enumerate() {
        if part.skin.is_none() || !part.name.starts_with(node_prefix) || part.kind != PartKind::Glb { continue; }
        let count = part.p.positions.len();
        let compact = count != vertex_count;
        let mut map: Option<&Value> = None;
        if compact {
            for m in body_maps { if m["sourceSha256"].as_str() == Some(source_sha) && output_count(m)? == count { map = Some(m); break; } }
            if map.is_none() { continue; }
        }
        let mut keys: HashSet<[u32; 3]> = HashSet::new();
        for tri in &native_tris {
            let ids: Vec<Vec<u32>> = tri.iter().map(|i| match map {
                Some(m) => m["sourceToOutput"][*i].as_array().into_iter().flatten().map(|x| x.as_u64().unwrap_or(0) as u32).collect(),
                None => vec![*i as u32],
            }).collect();
            for a in &ids[0] { for b in &ids[1] { for c in &ids[2] { let mut k = [*a, *b, *c]; k.sort(); keys.insert(k); } } }
        }
        let (mut hand, mut remaining) = (Vec::new(), Vec::new());
        for t in part.p.indices.chunks_exact(3) {
            let mut k = [t[0], t[1], t[2]]; k.sort();
            if keys.contains(&k) { hand.extend_from_slice(t); } else { remaining.extend_from_slice(t); }
        }
        if hand.is_empty() { continue; }
        if native.is_some() || hand.len() != native_tris.len() * 3 { return Err("通常の手の三角形と保存された原本が一致しません。".into()); }
        let mut n = part.clone();
        n.kind = PartKind::NativeHand;
        n.name = "OriginalNativeRightHand".into();
        n.p.indices = hand;
        part.p.indices = remaining;
        native = Some(n);
        insert_at = pi + 1;
    }
    let Some(n) = native else { return Err("通常の手と武器用の手を組み合わせられません。".into()) };
    parts_list.insert(insert_at, n);
    let f = |k: &str| -> Vec<f32> { profile[k].as_array().into_iter().flatten().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect() };
    let (pos, col) = (f("positions"), f("colors"));
    let indices: Vec<u32> = profile["indices"].as_array().into_iter().flatten().map(|x| x.as_u64().unwrap_or(0) as u32).collect();
    let mut p = glb::Primitive { positions: pos.chunks_exact(3).map(|v| [v[0], v[1], v[2]]).collect(), colors: col.chunks_exact(3).map(|v| [v[0], v[1], v[2], 1.0]).collect(), indices, ..Default::default() };
    p.uvs = vec![[0.0, 0.0]; p.positions.len()];
    glb::compute_normals(&mut p);
    Ok(Some((p, "R_Hand".into())))
}

/// 骨付きの部分の形の局所を、bind（GLB の中の部分の世界）を掛けた形へ焼く。kit は骨だけで頂点の世界を決める（three の AttachedBindMode の bindMatrix の分）
pub fn bake_bind(part: &mut Part) {
    let m = part.bind;
    if m == Mat4::IDENTITY { return; }
    let lin = Mat3::from_mat4(m);
    let nm = lin.inverse().transpose();
    for v in part.p.positions.iter_mut() { *v = m.transform_point3(Vec3::from(*v)).to_array(); }
    for v in part.p.normals.iter_mut() { *v = (nm * Vec3::from(*v)).normalize_or_zero().to_array(); }
    for t in part.p.morph_pos.iter_mut() { for v in t.iter_mut() { *v = (lin * Vec3::from(*v)).to_array(); } }
    for t in part.p.morph_nor.iter_mut() { for v in t.iter_mut() { *v = (nm * Vec3::from(*v)).to_array(); } }
    if let Some((base, delta)) = part.body.as_mut() {
        for v in base.iter_mut() { *v = m.transform_point3(Vec3::from(*v)).to_array(); }
        for v in delta.iter_mut() { *v = (lin * Vec3::from(*v)).to_array(); }
    }
    part.bind = Mat4::IDENTITY;
}
