// 主役・持ち込みの読み方（@aiarranger/character-runtime 0.2.7 の package の写し。readCharacterFile・readCharacterPackage・readGlbCharacter と contract.mjs・upload-validation.mjs）。
// 配布（delivery.rs）を通らない主役: .character.zip（manifest つき）と、manifest なしの .glb（持ち込み。読むときに manifest を作る）。
// NPC・汎用（0.2.5 の写し）は別の crate（character-actor-npc。Owner 2026-11-12「種類ごとに分ける」）。
// 失敗の文言は TS と同じ（日本語の文言はそのまま、それ以外は「壊れています」。normalizeCharacterError）。
use crate::actor::{self, CharacterActor, Gait, LoadOptions, Quality, ReleasePlan};
use crate::motion::{StillBones, StillLevel};
use crate::delivery::hash;
use crate::zip;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AssetSource { Package, Glb }

/// 読んだキャラ（character-runtime の CharacterAsset）。manifest は TS と同じ形の JSON（呼ぶ側が身長・口の既定値を書き換えてよい。WelcomeNpcs と同じ）
#[derive(Clone, Debug)]
pub struct CharacterAsset {
    pub source: AssetSource,
    pub manifest: Value,
    pub model: Arc<Vec<u8>>,
    pub thumbnail: Option<Arc<Vec<u8>>>,
    /// .character.zip の stats（decodedBytes・triangles）。.glb は None
    pub stats: Option<(u64, u64)>,
}

pub const LIMIT_ARCHIVE: usize = 128 * 1024 * 1024;
pub const LIMIT_EXPANDED: u64 = 256 * 1024 * 1024;
pub const LIMIT_JSON: usize = 4 * 1024 * 1024;
pub const LIMIT_BUFFERS: f64 = 256.0 * 1024.0 * 1024.0;
pub const LIMIT_TRIANGLES: f64 = 2_000_000.0;
const GLB_SETTINGS_KEY: &str = "aiarrangerCharacter";
pub const FACE_VOWELS: [&str; 5] = ["aa", "ih", "ou", "ee", "oh"];
pub const FACE_SEMANTICS: [&str; 16] = ["aa", "ih", "ou", "ee", "oh", "happy", "angry", "sad", "relaxed", "surprised", "blinkLeft", "blinkRight", "lookLeft", "lookRight", "lookUp", "lookDown"];
const BROKEN: &str = "キャラファイルが途中で切れているか、壊れています。元のファイルを選び直してください。";

/// normalizeCharacterError: 日本語の文言はそのまま、それ以外（読み手の英語の失敗）は利用者向けの文言へ
pub fn normalize_error(message: &str) -> String {
    let m = message.trim();
    let japanese = m.chars().any(|c| matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x9fff));
    if !m.is_empty() && japanese { message.to_string() } else { BROKEN.to_string() }
}

// ---- JS の値の見方 ----

fn plain(v: &Value) -> bool { v.is_object() }
/// JS の String.prototype.trim（空白・改行・BOM）を落として空でない、長さ（UTF-16）が max 以下
fn text_ok(v: &Value, max: usize) -> bool {
    v.as_str().is_some_and(|s| !s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}').is_empty() && s.encode_utf16().count() <= max)
}
fn text(v: &Value, label: &str, max: usize) -> Result<(), String> { if text_ok(v, max) { Ok(()) } else { Err(format!("{label}を確認してください。")) } }
/// Number.isSafeInteger
fn safe_int(v: &Value) -> bool { v.as_f64().is_some_and(|x| x.fract() == 0.0 && x.abs() <= 9007199254740991.0) }
fn safe_len(v: &Value) -> bool { safe_int(v) && v.as_f64().unwrap() >= 0.0 }
fn num_is(v: &Value, x: f64) -> bool { v.as_f64() == Some(x) && v.is_number() }
fn keys_in(v: &Value, allowed: &[&str]) -> bool { v.as_object().is_some_and(|o| o.keys().all(|k| allowed.contains(&k.as_str()))) }
fn arr(v: &Value) -> &[Value] { v.as_array().map(Vec::as_slice).unwrap_or(&[]) }
fn fail<T>(m: impl Into<String>) -> Result<T, String> { Err(m.into()) }
fn is_hex64(s: &str) -> bool { s.len() == 64 && s.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)) }

// ---- GLB の検査（contract.mjs の inspectModel） ----

/// inspectModel の結果
pub struct ModelInfo {
    pub gltf: Value,
    pub decoded_bytes: f64,
    pub triangles: u64,
    /// 待機・歩き・走りの名前（'…:idle' で終わるか 'idle' そのもの）
    pub clips: Map<String, Value>,
    /// メッシュのモーフの名前（重ならない。出てきた順）
    pub expressions: Vec<String>,
}

/// TextDecoder の decode（壊れた並びは置き換え、先頭の BOM は落とす）
fn utf8(b: &[u8]) -> std::borrow::Cow<'_, str> { let b = b.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(b); String::from_utf8_lossy(b) }
/// (p.mode ?? 4) === 4
fn triangles_mode(p: &Value) -> bool { match p.get("mode") { None | Some(Value::Null) => true, Some(v) => v.as_f64() == Some(4.0) } }
/// 名前の終わりが .glb（大文字小文字を見ない）
fn ends_glb(s: &str) -> bool { s.len() >= 4 && s.get(s.len() - 4..).is_some_and(|t| t.eq_ignore_ascii_case(".glb")) }
fn u32le(b: &[u8], at: usize) -> Option<u32> { b.get(at..at + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]])) }

pub fn inspect_model(bytes: &[u8]) -> Result<ModelInfo, String> {
    if bytes.len() < 28 { return fail("GLBファイルが途中で切れています。"); }
    let (magic, version, total) = (u32le(bytes, 0).unwrap(), u32le(bytes, 4).unwrap(), u32le(bytes, 8).unwrap());
    if magic != 0x4654_6c67 || version != 2 || total as usize != bytes.len() { return fail("GLB 2.0のファイルを選んでください。"); }
    let length = u32le(bytes, 12).unwrap() as usize;
    if length > LIMIT_JSON || length + 28 > bytes.len() || u32le(bytes, 16).unwrap() != 0x4e4f_534a { return fail("モデルの構成情報を読み込めません。"); }
    let g: Value = serde_json::from_str(utf8(&bytes[20..20 + length]).as_ref()).map_err(|e| e.to_string())?;
    if g["asset"]["version"].as_str() != Some("2.0") { return fail("モデルの版が未対応です。"); }
    // 通信もデータ URI も使わない（画像・バッファは GLB の中だけ）
    fn has_uri(v: &Value) -> bool {
        match v { Value::Object(o) => o.iter().any(|(k, x)| k == "uri" || has_uri(x)), Value::Array(a) => a.iter().any(has_uri), _ => false }
    }
    if has_uri(&g) { return fail("外部ファイルを参照するモデルには対応していません。"); }
    let supported = ["EXT_meshopt_compression", "KHR_materials_unlit", "KHR_texture_transform"];
    if arr(&g["extensionsRequired"]).iter().any(|e| !e.as_str().is_some_and(|s| supported.contains(&s))) { return fail("このモデルの圧縮・材質方式は未対応です。"); }
    let decoded = arr(&g["buffers"]).iter().fold(0.0f64, |n, b| n + b["byteLength"].as_f64().unwrap_or(f64::NAN));
    if !(decoded.fract() == 0.0 && decoded.abs() <= 9007199254740991.0) || decoded < 1.0 || decoded > LIMIT_BUFFERS { return fail("モデルの展開サイズが大きすぎます。"); }
    let bin = 20 + length;
    let bin_len = u32le(bytes, bin).ok_or(BROKEN)? as usize;
    if u32le(bytes, bin + 4).ok_or(BROKEN)? != 0x004e_4942 || bin + 8 + bin_len != bytes.len() { return fail("モデルの本体が欠けています。"); }
    let mut triangles = 0f64;
    for m in arr(&g["meshes"]) {
        for p in arr(&m["primitives"]) {
            if !triangles_mode(p) { return fail("三角形以外のメッシュは未対応です。"); }
            let at = if p.get("indices").is_some_and(|v| !v.is_null()) { &p["indices"] } else { &p["attributes"]["POSITION"] };
            let count = at.as_u64().and_then(|i| g["accessors"].get(i as usize)).map(|a| &a["count"]).unwrap_or(&Value::Null);
            if !safe_len(count) { return fail("メッシュの頂点情報が不正です。"); }
            triangles += count.as_f64().unwrap() / 3.0;
        }
    }
    if triangles > LIMIT_TRIANGLES { return fail("モデルの面数が大きすぎます。"); }
    let animations = arr(&g["animations"]);
    let mut clips = Map::new();
    for name in ["idle", "walk", "run"] {
        let suffix = format!(":{name}");
        let found = animations.iter().find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(&suffix))).or_else(|| animations.iter().find(|a| a["name"].as_str() == Some(name)));
        if let Some(n) = found.and_then(|a| a["name"].as_str()).filter(|n| !n.is_empty()) { clips.insert(name.to_string(), json!(n)); }
    }
    let mut expressions: Vec<String> = Vec::new();
    for m in arr(&g["meshes"]) {
        for t in arr(&m["extras"]["targetNames"]) {
            if let Some(t) = t.as_str() && !expressions.iter().any(|e| e == t) { expressions.push(t.to_string()); }
        }
    }
    Ok(ModelInfo { gltf: g, decoded_bytes: decoded, triangles: triangles.ceil() as u64, clips, expressions })
}

// ---- glTF の JSON の検査（upload-validation.mjs の validateCharacterGltf） ----

fn require_array<'a>(v: &'a Value, label: &str, non_empty: bool) -> Result<&'a [Value], String> {
    match v.as_array() { Some(a) if !(non_empty && a.is_empty()) => Ok(a), _ => fail(format!("{label}が不完全です。")) }
}
fn safe_index(v: &Value, len: usize) -> bool { safe_int(v) && v.as_f64().unwrap() >= 0.0 && (v.as_f64().unwrap() as usize) < len }
fn require_index<'a>(v: &Value, values: &'a [Value], label: &str) -> Result<&'a Value, String> {
    if !safe_index(v, values.len()) { return fail(format!("{label}の参照先が不正です。")); }
    Ok(&values[v.as_f64().unwrap() as usize])
}
fn require_byte_range(v: &Value, buffers: &[Value], label: &str) -> Result<(), String> {
    if !plain(v) || !safe_index(&v["buffer"], buffers.len()) { return fail(format!("{label}のバッファ参照が不正です。")); }
    let offset = if v.get("byteOffset").is_none() { json!(0) } else { v["byteOffset"].clone() };
    let buf_len = buffers[v["buffer"].as_f64().unwrap() as usize]["byteLength"].as_f64().unwrap_or(f64::NAN);
    if !safe_len(&offset) || !safe_len(&v["byteLength"]) || offset.as_f64().unwrap() + v["byteLength"].as_f64().unwrap() > buf_len {
        return fail(format!("{label}の範囲が本体からはみ出しています。"));
    }
    Ok(())
}

pub fn validate_character_gltf(g: &Value) -> Result<(), String> {
    if !plain(g) || !plain(&g["asset"]) || g["asset"]["version"].as_str() != Some("2.0") { return fail("モデルの構成情報が不正です。"); }
    let buffers = require_array(&g["buffers"], "モデルのバッファ情報", true)?;
    for (i, b) in buffers.iter().enumerate() {
        if !plain(b) || !safe_len(&b["byteLength"]) || b["byteLength"].as_f64().unwrap() < 1.0 || b.get("uri").is_some() { return fail(format!("モデルのバッファ{i}が不正です。")); }
    }
    let views = require_array(&g["bufferViews"], "モデルのバッファ範囲", true)?;
    for (i, v) in views.iter().enumerate() {
        require_byte_range(v, buffers, &format!("バッファ範囲{i}"))?;
        if let Some(c) = v.get("extensions").and_then(|e| e.get("EXT_meshopt_compression")) {
            require_byte_range(c, buffers, &format!("圧縮バッファ範囲{i}"))?;
            let stride = c["byteStride"].as_f64().unwrap_or(f64::NAN);
            if !plain(c) || !safe_len(&c["count"]) || c["count"].as_f64().unwrap() < 1.0 || !safe_len(&c["byteStride"]) || !(1.0..=252.0).contains(&stride)
                || !c["mode"].as_str().is_some_and(|m| ["ATTRIBUTES", "TRIANGLES", "INDICES"].contains(&m)) {
                return fail(format!("圧縮バッファ範囲{i}が不正です。"));
            }
        }
    }
    let accessors = require_array(&g["accessors"], "モデルの頂点情報", true)?;
    for (i, a) in accessors.iter().enumerate() {
        let ct_ok = safe_int(&a["componentType"]) && [5120.0, 5121.0, 5122.0, 5123.0, 5125.0, 5126.0].contains(&a["componentType"].as_f64().unwrap());
        if !plain(a) || !safe_len(&a["count"]) || !ct_ok || !a["type"].as_str().is_some_and(|t| ["SCALAR", "VEC2", "VEC3", "VEC4", "MAT2", "MAT3", "MAT4"].contains(&t)) {
            return fail(format!("頂点情報{i}が不正です。"));
        }
        if a.get("bufferView").is_some() { require_index(&a["bufferView"], views, &format!("頂点情報{i}のバッファ"))?; }
        if a.get("byteOffset").is_some() && !safe_len(&a["byteOffset"]) { return fail(format!("頂点情報{i}の位置が不正です。")); }
        if let Some(sp) = a.get("sparse") {
            let it_ok = sp["indices"]["componentType"].as_f64().is_some_and(|c| [5121.0, 5123.0, 5125.0].contains(&c));
            if !plain(sp) || !safe_len(&sp["count"]) || sp["count"].as_f64().unwrap() < 1.0 || sp["count"].as_f64().unwrap() > a["count"].as_f64().unwrap()
                || !plain(&sp["indices"]) || !plain(&sp["values"]) || !it_ok {
                return fail(format!("頂点情報{i}の差分データが不正です。"));
            }
            require_index(&sp["indices"]["bufferView"], views, &format!("頂点情報{i}の差分インデックス"))?;
            require_index(&sp["values"]["bufferView"], views, &format!("頂点情報{i}の差分値"))?;
            for part in [&sp["indices"], &sp["values"]] { if part.get("byteOffset").is_some() && !safe_len(&part["byteOffset"]) { return fail(format!("頂点情報{i}の差分位置が不正です。")); } }
        }
    }
    let meshes = require_array(&g["meshes"], "モデルのメッシュ", true)?;
    for (mi, m) in meshes.iter().enumerate() {
        if !plain(m) { return fail(format!("メッシュ{mi}が不正です。")); }
        let prims = require_array(&m["primitives"], &format!("メッシュ{mi}の構成"), true)?;
        for p in prims {
            if !plain(p) || !triangles_mode(p) || !plain(&p["attributes"]) { return fail(format!("メッシュ{mi}の面情報が不正です。")); }
            let pos = require_index(&p["attributes"]["POSITION"], accessors, &format!("メッシュ{mi}の位置情報"))?;
            if !safe_len(&pos["count"]) || pos["count"].as_f64().unwrap() < 3.0 { return fail(format!("メッシュ{mi}に表示できる頂点がありません。")); }
            for (name, r) in p["attributes"].as_object().unwrap() { require_index(r, accessors, &format!("メッシュ{mi}の{name}"))?; }
            if p.get("indices").is_some() { require_index(&p["indices"], accessors, &format!("メッシュ{mi}のインデックス"))?; }
            if let Some(ts) = p.get("targets") {
                let Some(ts) = ts.as_array() else { return fail(format!("メッシュ{mi}の表情情報が不正です。")) };
                for (ti, t) in ts.iter().enumerate() {
                    if !plain(t) { return fail(format!("メッシュ{mi}の表情{ti}が不正です。")); }
                    for (name, r) in t.as_object().unwrap() {
                        let ta = require_index(r, accessors, &format!("メッシュ{mi}の表情{ti}/{name}"))?;
                        if ta["count"] != pos["count"] { return fail(format!("メッシュ{mi}の表情{ti}の頂点数が一致しません。")); }
                    }
                }
            }
        }
    }
    let nodes = g["nodes"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    for (i, n) in nodes.iter().enumerate() {
        if !plain(n) { return fail(format!("ノード{i}が不正です。")); }
        if n.get("mesh").is_some() { require_index(&n["mesh"], meshes, &format!("ノード{i}のメッシュ"))?; }
        if let Some(c) = n.get("children") {
            let Some(c) = c.as_array() else { return fail(format!("ノード{i}の子要素が不正です。")) };
            for x in c { require_index(x, nodes, &format!("ノード{i}の子要素"))?; }
        }
    }
    if let Some(scenes) = g["scenes"].as_array() {
        for (i, s) in scenes.iter().enumerate() {
            if !plain(s) { return fail(format!("シーン{i}が不正です。")); }
            if let Some(ns) = s.get("nodes") {
                let Some(ns) = ns.as_array() else { return fail(format!("シーン{i}のノードが不正です。")) };
                for x in ns { require_index(x, nodes, &format!("シーン{i}のノード"))?; }
            }
        }
        if g.get("scene").is_some() && !safe_index(&g["scene"], scenes.len()) { return fail("表示するシーンの参照が不正です。"); }
    }
    if let Some(anims) = g["animations"].as_array() {
        for (i, a) in anims.iter().enumerate() {
            if !plain(a) { return fail(format!("動作{i}が不正です。")); }
            let samplers = require_array(&a["samplers"], &format!("動作{i}のサンプラー"), true)?;
            for (si, s) in samplers.iter().enumerate() {
                if !plain(s) { return fail(format!("動作{i}のサンプラー{si}が不正です。")); }
                require_index(&s["input"], accessors, &format!("動作{i}の時間軸"))?;
                require_index(&s["output"], accessors, &format!("動作{i}の値"))?;
            }
            let channels = require_array(&a["channels"], &format!("動作{i}のチャンネル"), true)?;
            for (ci, c) in channels.iter().enumerate() {
                if !plain(c) || !safe_index(&c["sampler"], samplers.len()) || !plain(&c["target"]) { return fail(format!("動作{i}のチャンネル{ci}が不正です。")); }
                if c["target"].get("node").is_some() { require_index(&c["target"]["node"], nodes, &format!("動作{i}の対象ノード"))?; }
                if !c["target"]["path"].is_string() { return fail(format!("動作{i}の対象プロパティが不正です。")); }
            }
        }
    }
    Ok(())
}

/// TS には無い確かめ（Rust の読み手が範囲の外を読まないため）。アクセサがバッファ範囲の中に収まるか・圧縮の展開の大きさ・画像の範囲。
/// TS ではこれらの壊れ方は three の読み込みの失敗（英語の RangeError）になり、利用者には「壊れています」と出る。同じ文言にする
pub fn check_ranges(g: &Value, bin_len: usize) -> Result<(), String> {
    let views = arr(&g["bufferViews"]);
    let in_bin = |off: f64, len: f64| off >= 0.0 && len >= 0.0 && off + len <= bin_len as f64;
    for v in views {
        // 圧縮した範囲は GLB の本体の中。圧縮の無い範囲も本体の中（圧縮の元の置き場 fallback のバッファは読まない）
        let (off, len) = match v.get("extensions").and_then(|e| e.get("EXT_meshopt_compression")) { Some(c) => (&c["byteOffset"], &c["byteLength"]), None => (&v["byteOffset"], &v["byteLength"]) };
        if !in_bin(off.as_f64().unwrap_or(0.0), len.as_f64().unwrap_or(f64::NAN)) { return fail(BROKEN); }
    }
    // (1 要素のバイト数, 圧縮を解いた後の大きさか元の大きさ, 間隔)
    let view_len = |i: usize| -> Option<(f64, f64)> {
        let v = views.get(i)?;
        Some(match v.get("extensions").and_then(|e| e.get("EXT_meshopt_compression")) {
            Some(c) => (c["count"].as_f64()? * c["byteStride"].as_f64()?, c["byteStride"].as_f64()?),
            None => (v["byteLength"].as_f64()?, v["byteStride"].as_f64().unwrap_or(0.0)),
        })
    };
    let comps = |t: &str| match t { "SCALAR" => 1.0, "VEC2" => 2.0, "VEC3" => 3.0, "VEC4" | "MAT2" => 4.0, "MAT3" => 9.0, "MAT4" => 16.0, _ => 1.0 };
    let size = |ct: f64| if ct == 5120.0 || ct == 5121.0 { 1.0 } else if ct == 5122.0 || ct == 5123.0 { 2.0 } else { 4.0 };
    for a in arr(&g["accessors"]) {
        let (count, c, s) = (a["count"].as_f64().unwrap_or(0.0), comps(a["type"].as_str().unwrap_or("")), size(a["componentType"].as_f64().unwrap_or(5126.0)));
        if let Some(bv) = a["bufferView"].as_u64() {
            let (len, stride) = view_len(bv as usize).ok_or(BROKEN)?;
            let elem = c * s;
            let st = if stride > 0.0 { stride } else { elem };
            let end = a["byteOffset"].as_f64().unwrap_or(0.0) + if count > 0.0 { (count - 1.0) * st + elem } else { 0.0 };
            if end > len { return fail(BROKEN); }
        }
        if let Some(sp) = a.get("sparse") {
            let n = sp["count"].as_f64().unwrap_or(0.0);
            for (part, each) in [(&sp["indices"], size(sp["indices"]["componentType"].as_f64().unwrap_or(5125.0))), (&sp["values"], c * s)] {
                let (len, _) = view_len(part["bufferView"].as_u64().unwrap_or(u64::MAX) as usize).ok_or(BROKEN)?;
                if part["byteOffset"].as_f64().unwrap_or(0.0) + n * each > len { return fail(BROKEN); }
            }
        }
    }
    for im in arr(&g["images"]) {
        let Some(bv) = im["bufferView"].as_u64() else { return fail(BROKEN) };
        if view_len(bv as usize).is_none() { return fail(BROKEN); }
    }
    let accessors = arr(&g["accessors"]);
    let count_of = |r: &Value| r.as_u64().and_then(|i| accessors.get(i as usize)).and_then(|a| a["count"].as_u64());
    let (nodes, n_images, n_materials, n_skins) = (arr(&g["nodes"]), arr(&g["images"]).len(), arr(&g["materials"]).len(), arr(&g["skins"]).len());
    for t in arr(&g["textures"]) {
        for src in [&t["source"], &t["extensions"]["EXT_texture_webp"]["source"]] { if src.as_u64().is_some_and(|i| i as usize >= n_images) { return fail(BROKEN); } }
    }
    for s in arr(&g["skins"]) {
        if arr(&s["joints"]).iter().any(|j| j.as_u64().is_none_or(|j| j as usize >= nodes.len())) { return fail(BROKEN); }
        if let Some(c) = s.get("inverseBindMatrices").map(count_of) && c.is_none_or(|c| (c as usize) < arr(&s["joints"]).len()) { return fail(BROKEN); }
    }
    for n in nodes { if n["skin"].as_u64().is_some_and(|i| i as usize >= n_skins) { return fail(BROKEN); } }
    // validateLoadedCharacter（validate.js）: 頂点の数がそろっているか。three の属性の名前で文言を出す
    let three_name = |k: &str| match k { "POSITION" => "position".to_string(), "NORMAL" => "normal".into(), "TANGENT" => "tangent".into(), "TEXCOORD_0" => "uv".into(), "TEXCOORD_1" => "uv1".into(), "TEXCOORD_2" => "uv2".into(), "TEXCOORD_3" => "uv3".into(), "COLOR_0" => "color".into(), "WEIGHTS_0" => "skinWeight".into(), "JOINTS_0" => "skinIndex".into(), _ => k.to_lowercase() };
    for m in arr(&g["meshes"]) {
        for p in arr(&m["primitives"]) {
            if p["material"].as_u64().is_some_and(|i| i as usize >= n_materials) { return fail(BROKEN); }
            let Some(n) = count_of(&p["attributes"]["POSITION"]).filter(|n| *n >= 3) else { return fail("モデルの頂点データが不完全です。") };
            for (k, r) in p["attributes"].as_object().into_iter().flatten() { if count_of(r) != Some(n) { return fail(format!("モデルの{}データが不完全です。", three_name(k))); } }
            if p.get("indices").is_some_and(|i| !i.is_null()) && count_of(&p["indices"]).is_none_or(|c| c < 3) { return fail("モデルの面データが不完全です。"); }
        }
    }
    for a in arr(&g["animations"]) {
        let samplers = arr(&a["samplers"]);
        for c in arr(&a["channels"]) {
            let s = &samplers[c["sampler"].as_u64().unwrap_or(0) as usize];
            let (i, o) = (count_of(&s["input"]).unwrap_or(0), count_of(&s["output"]).unwrap_or(0));
            let k = if s["interpolation"].as_str() == Some("CUBICSPLINE") { 3 } else { 1 };
            if i == 0 || o == 0 || (c["target"]["path"].as_str() != Some("weights") && o < i * k) { return fail("モデルの動作データが不完全です。"); }
        }
    }
    Ok(())
}

// ---- 表情の対応（contract.mjs の validateFaceProfile・faceModelBindings） ----

pub fn validate_face_profile(face: &Value) -> Result<(), String> {
    let version_ok = num_is(&face["version"], 1.0) || num_is(&face["version"], 2.0);
    if !plain(face) || !version_ok || !keys_in(face, &["version", "morphs", "overrides"]) || !plain(&face["morphs"]) { return fail("表情の対応形式が未対応です。"); }
    let morphs = face["morphs"].as_object().unwrap();
    let entry_bad = morphs.iter().any(|(k, v)| !FACE_SEMANTICS.contains(&k.as_str()) || !text_ok(v, 120));
    let vowels_missing = FACE_VOWELS.iter().any(|k| !morphs.contains_key(*k));
    if morphs.is_empty() || (num_is(&face["version"], 1.0) && vowels_missing) || entry_bad { return fail("5母音・表情の対応を確認してください。"); }
    let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
    for (k, v) in morphs { let v = v.as_str().unwrap(); match groups.iter_mut().find(|(m, _)| *m == v) { Some(g) => g.1.push(k), None => groups.push((v, vec![k])) } }
    for (_, g) in &groups {
        let mut s = g.clone();
        s.sort();
        if g.len() > 1 && (g.len() != 2 || s != ["blinkLeft", "blinkRight"]) { return fail("同じmorphを共有できるのは単一blinkの左右だけです。"); }
    }
    if let Some(o) = face.get("overrides") {
        let Some(o) = o.as_object() else { return fail("表情の抑制設定が不正です。") };
        for (key, rule) in o {
            let ok = morphs.contains_key(key) && rule.as_object().is_some_and(|r| r.iter().all(|(k, v)| ["blink", "mouth", "lookAt"].contains(&k.as_str()) && v.as_str().is_some_and(|v| ["none", "blend", "block"].contains(&v))));
            if !ok { return fail("表情の抑制設定が不正です。"); }
        }
    }
    Ok(())
}

/// 表示する場面の中の、表情のモーフの持ち主を確かめる（faceModelBindings。戻り値は使わないので検査だけ）
pub fn check_face_bindings(face: &Value, g: &Value) -> Result<(), String> {
    validate_face_profile(face)?;
    let nodes = arr(&g["nodes"]);
    let mut active: Vec<usize> = Vec::new();
    fn visit(i: &Value, nodes: &[Value], active: &mut Vec<usize>) -> Result<(), String> {
        let Some(i) = i.as_u64().map(|x| x as usize).filter(|_| safe_int(i)) else { return fail("表情の対象ノードがありません。") };
        if active.contains(&i) { return Ok(()); }
        if i >= nodes.len() || nodes[i].is_null() { return fail("表情の対象ノードがありません。"); }
        active.push(i);
        for c in arr(&nodes[i]["children"]) { visit(c, nodes, active)?; }
        Ok(())
    }
    let scene = g["scene"].as_u64().unwrap_or(0) as usize;
    for i in arr(&g["scenes"].get(scene).cloned().unwrap_or(Value::Null)["nodes"]) { visit(i, nodes, &mut active)?; }
    let name_count = |name: &Value| active.iter().filter(|&&i| nodes[i]["name"] == *name).count();
    let morphs = face["morphs"].as_object().unwrap();
    let mut bound: HashMap<&str, usize> = morphs.keys().map(|k| (k.as_str(), 0)).collect();
    let mut controlled: Vec<usize> = Vec::new();
    for &node in &active {
        let n = &nodes[node];
        let Some(m) = n["mesh"].as_u64().and_then(|i| g["meshes"].get(i as usize)) else { continue };
        let targets: Vec<&str> = arr(&m["extras"]["targetNames"]).iter().map(|t| t.as_str().unwrap_or("")).collect();
        for (key, morph) in morphs {
            let morph = morph.as_str().unwrap();
            let Some(index) = targets.iter().position(|t| *t == morph) else { continue };
            let named = n["name"].as_str().is_some_and(|s| !s.is_empty());
            if targets.iter().rposition(|t| *t == morph) != Some(index) || !named || name_count(&n["name"]) != 1 { return fail("表情のノード名・morph名を一意に特定できません。"); }
            let prims = arr(&m["primitives"]);
            if prims.is_empty() || prims.iter().any(|p| p["targets"].get(index).is_none_or(|t| t.is_null())) { return fail("表情のmorph本体がありません。"); }
            let weights = if n.get("weights").is_some_and(|w| !w.is_null()) { &n["weights"] } else { &m["weights"] };
            if let Some(w) = weights.get(index) && !num_is(w, 0.0) { return fail("この表情形式は中立値0のモデルに対応します。"); }
            *bound.get_mut(key.as_str()).unwrap() += 1;
            if !controlled.contains(&node) { controlled.push(node); }
        }
    }
    if bound.values().any(|n| *n == 0) { return fail("表示モデルに指定された表情がありません。"); }
    let clash = arr(&g["animations"]).iter().any(|a| arr(&a["channels"]).iter().any(|c| c["target"]["path"].as_str() == Some("weights") && c["target"]["node"].as_u64().is_some_and(|n| controlled.contains(&(n as usize)))));
    if clash { return fail("表情とclipのmorph制御が競合しています。"); }
    Ok(())
}

// ---- manifest（contract.mjs の validateManifest） ----

pub fn validate_manifest(m: &Value) -> Result<(), String> {
    if !plain(m) || !(num_is(&m["schemaVersion"], 1.0) || num_is(&m["schemaVersion"], 2.0)) { return fail("このパッケージの版には対応していません。"); }
    for key in ["id", "name", "author", "modelVersion"] { text(&m[key], key, 160)?; }
    let id = m["id"].as_str().unwrap();
    let id_ok = id.len() <= 80 && id.bytes().enumerate().all(|(i, c)| c.is_ascii_lowercase() || c.is_ascii_digit() || (i > 0 && matches!(c, b'.' | b'_' | b'-')));
    if !id_ok { return fail("キャラIDが不正です。"); }
    let model = &m["model"];
    if model["path"].as_str() != Some("model.glb") || !model["sha256"].as_str().is_some_and(is_hex64) || !safe_int(&model["bytes"]) || model["bytes"].as_f64().unwrap() < 1.0 { return fail("モデルのファイル情報が不正です。"); }
    if !m["thumbnail"]["path"].as_str().is_some_and(|p| p == "thumbnail.png" || p == "thumbnail.jpg") || !m["thumbnail"]["sha256"].as_str().is_some_and(is_hex64) { return fail("見本画像の情報が不正です。"); }
    let c = &m["coordinates"];
    let h = c["heightMeters"].as_f64().unwrap_or(f64::NAN);
    if c["up"].as_str() != Some("+Y") || !c["forward"].as_str().is_some_and(|f| ["+X", "+Z", "-Z", "-X"].contains(&f)) || c["ground"].as_str() != Some("bounds-min-y") || !(h >= 0.2 && h <= 3.0) {
        return fail("向き・大きさ・接地の指定が未対応です。");
    }
    let motion = &m["motion"];
    if motion["controller"].as_str() != Some("morph-v1") || !(num_is(&motion["neutralMouthClose"], 0.0) || num_is(&motion["neutralMouthClose"], 1.0)) { return fail("動作の方式が未対応です。"); }
    if num_is(&m["schemaVersion"], 2.0) { validate_face_profile(&motion["face"])?; } else if motion.get("face").is_some() { return fail("表情対応を含むパッケージには版2が必要です。"); }
    if let Some(root) = motion.get("rootMotion") {
        if !plain(root) || root["mode"].as_str() != Some("local-linear-horizontal") || !keys_in(root, &["node", "mode"]) { return fail("前進を分ける動作の指定が未対応です。"); }
        text(&root["node"], "前進を分けるノード名", 200)?;
    }
    for action in ["idle", "walk"] { text(&motion["clips"][action], action, 200)?; }
    if let Some(o) = motion["clips"].as_object() { for (action, name) in o { text(name, action, 200)?; } }
    if !motion["expressions"].as_array().is_some_and(|a| a.iter().all(|x| x.as_str().is_some_and(|s| s.encode_utf16().count() <= 120))) { return fail("表情の指定が不正です。"); }
    for k in ["version", "assetLicense", "aiPerformance"] { text(&m["terms"][k], k, 4000)?; }
    if !m["terms"]["reference"].as_str().is_some_and(|r| r.starts_with("https://")) { return fail("利用条件の参照先がありません。"); }
    let mut allowed = vec!["schemaVersion", "id", "name", "author", "modelVersion", "model", "thumbnail", "coordinates", "motion", "terms"];
    allowed.push("weapon");
    if let Some(w) = m.get("weapon") && !w.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 24 && s.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')) { return fail("武器の指定が不正です。"); }
    if !keys_in(m, &allowed) { return fail("配布用でない情報が含まれています。制作側で再出力してください。"); }
    let mut motion_keys = vec!["controller", "clips", "expressions", "neutralMouthClose", "rootMotion"];
    if num_is(&m["schemaVersion"], 2.0) { motion_keys.push("face"); }
    let fields: [(&str, &[&str]); 5] = [("model", &["path", "sha256", "bytes"]), ("thumbnail", &["path", "sha256"]), ("coordinates", &["up", "forward", "ground", "heightMeters"]), ("motion", &motion_keys), ("terms", &["version", "assetLicense", "aiPerformance", "reference"])];
    for (key, keys) in fields { if !plain(&m[key]) || !keys_in(&m[key], keys) { return fail("配布用でない情報が含まれています。制作側で再出力してください。"); } }
    let clips: &[&str] = &["idle", "walk", "run", "swordIdle", "swordAttack"];
    if !plain(&motion["clips"]) || !keys_in(&motion["clips"], clips) { return fail("動作の指定が未対応です。"); }
    Ok(())
}

// ---- 読む ----

fn validate_package_files(files: &[(String, Vec<u8>)]) -> Result<CharacterAsset, String> {
    let get = |n: &str| files.iter().find(|(k, _)| k == n).map(|(_, v)| v);
    let cj = match get("character.json") { Some(b) if b.len() <= 32768 => b, _ => return fail("character.jsonがないか、大きすぎます。") };
    let m: Value = serde_json::from_str(utf8(cj).as_ref()).map_err(|e| e.to_string())?;
    validate_manifest(&m)?;
    let thumb_path = m["thumbnail"]["path"].as_str().unwrap();
    let allowed = ["character.json", "model.glb", thumb_path];
    if files.len() != 3 || files.iter().any(|(n, _)| !allowed.contains(&n.as_str())) { return fail("パッケージのファイル構成が違います。"); }
    let (model, thumbnail) = (get("model.glb"), get(thumb_path));
    let (Some(model), Some(thumbnail)) = (model, thumbnail) else { return fail("モデルの容量が一致しません。") };
    if model.len() as f64 != m["model"]["bytes"].as_f64().unwrap() || model.len() > LIMIT_ARCHIVE { return fail("モデルの容量が一致しません。"); }
    if hash(model) != m["model"]["sha256"].as_str().unwrap() || hash(thumbnail) != m["thumbnail"]["sha256"].as_str().unwrap() {
        return fail("ファイルが壊れているか、内容が変更されています。元のパッケージを選び直してください。");
    }
    let png = thumbnail.len() >= 4 && thumbnail[..4] == [137, 80, 78, 71];
    let jpg = thumbnail.len() >= 2 && thumbnail[..2] == [255, 216];
    if !(if thumb_path.ends_with(".png") { png } else { jpg }) { return fail("見本画像の形式が違います。"); }
    let stats = inspect_model(model)?;
    if let Some(face) = m["motion"].get("face") { check_face_bindings(face, &stats.gltf)?; }
    if let Some(root) = m["motion"].get("rootMotion") && arr(&stats.gltf["nodes"]).iter().filter(|n| n["name"] == root["node"]).count() != 1 {
        return fail("前進を分けるノードをモデル内で一意に特定できません。");
    }
    let names: Vec<&str> = arr(&stats.gltf["animations"]).iter().filter_map(|a| a["name"].as_str()).collect();
    for (motion, name) in m["motion"]["clips"].as_object().unwrap() {
        // 0.2.7 はモデルにその名前の動作があればよい
        let ok = names.contains(&name.as_str().unwrap_or(""));
        if !ok { return fail(format!("モデルに{motion}の動作がありません。")); }
    }
    if arr(&m["motion"]["expressions"]).iter().any(|e| !stats.expressions.iter().any(|x| Some(x.as_str()) == e.as_str())) { return fail("モデルに指定された表情がありません。"); }
    Ok(CharacterAsset { source: AssetSource::Package, manifest: m, model: Arc::new(model.clone()), thumbnail: Some(Arc::new(thumbnail.clone())), stats: Some((stats.decoded_bytes as u64, stats.triangles)) })
}

/// readCharacterPackage（.character.zip）。展開の前に、ファイルの名前・重なり・展開の大きさを確かめる
pub fn read_character_package(bytes: &[u8]) -> Result<CharacterAsset, String> {
    let run = || -> Result<CharacterAsset, String> {
        if bytes.len() > LIMIT_ARCHIVE { return fail("ファイルが大きすぎます。制作側で軽量化してください。"); }
        let mut names: Vec<String> = Vec::new();
        let mut expanded = 0u64;
        let files = zip::unzip(bytes, |e| {
            if !["character.json", "model.glb", "thumbnail.png", "thumbnail.jpg"].contains(&e.name.as_str()) || names.contains(&e.name) { return fail("パッケージのファイル構成が違います。"); }
            names.push(e.name.clone());
            expanded += e.original_size;
            if expanded > LIMIT_EXPANDED || (e.name == "character.json" && e.original_size > 32768) { return fail("ファイルの展開サイズが大きすぎます。"); }
            Ok(true)
        })?;
        let asset = validate_package_files(&files)?;
        validate_character_gltf(&inspect_model(&asset.model)?.gltf)?;
        Ok(asset)
    };
    run().map_err(|e| normalize_error(&e))
}

/// asset.extras.aiarrangerCharacter（GLB の中のキャラの設定。0.2.1〜）の根の移動
fn glb_root_motion(g: &Value) -> Result<Option<Value>, String> {
    fn misplaced(v: &Value, asset: *const Value) -> bool {
        match v {
            Value::Object(o) => {
                if !std::ptr::eq(v, asset) && o.get("extras").is_some_and(|e| e.as_object().is_some_and(|e| e.contains_key(GLB_SETTINGS_KEY))) { return true; }
                o.values().any(|x| misplaced(x, asset))
            }
            Value::Array(a) => a.iter().any(|x| misplaced(x, asset)),
            _ => false,
        }
    }
    let asset: *const Value = g.get("asset").map_or(std::ptr::null(), |a| a as *const Value);
    if misplaced(g, asset) { return fail(format!("GLBのキャラ設定（{GLB_SETTINGS_KEY}）は asset.extras に置いてください。")); }
    let extras = &g["asset"]["extras"];
    let Some(settings) = extras.as_object().and_then(|e| e.get(GLB_SETTINGS_KEY)) else { return Ok(None) };
    if !plain(settings) || !keys_in(settings, &["motion"]) || !plain(&settings["motion"]) || !keys_in(&settings["motion"], &["rootMotion"]) || settings["motion"].get("rootMotion").is_none() {
        return fail(format!("GLBのキャラ設定（{GLB_SETTINGS_KEY}）の形が未対応です。"));
    }
    let root = &settings["motion"]["rootMotion"];
    if !plain(root) || root["mode"].as_str() != Some("local-linear-horizontal") || !keys_in(root, &["node", "mode"]) { return fail("前進を分ける動作の指定が未対応です。"); }
    text(&root["node"], "前進を分けるノード名", 200)?;
    if arr(&g["nodes"]).iter().filter(|n| n["name"] == root["node"]).count() != 1 { return fail("前進を分けるノードをモデル内で一意に特定できません。"); }
    Ok(Some(json!({ "node": root["node"], "mode": root["mode"] })))
}

/// 動作の名前に action（と ing）が区切りで挟まれているか（read-glb.mjs の (?:^|[:|_.\s-])action(?:ing)?(?:$|[:|_.\s-])、大文字小文字を見ない）
fn names_action(name: &str, action: &str) -> bool {
    let chars: Vec<char> = name.chars().collect();
    let a: Vec<char> = action.chars().collect();
    let sep = |c: char| matches!(c, ':' | '|' | '_' | '.' | '-') || c.is_whitespace() || c == '\u{feff}';
    let at = |i: usize, w: &[char]| i + w.len() <= chars.len() && chars[i..i + w.len()].iter().zip(w).all(|(x, y)| x.to_ascii_lowercase() == *y);
    for i in 0..chars.len() {
        if i > 0 && !sep(chars[i - 1]) { continue; }
        if !at(i, &a) { continue; }
        let after = i + a.len();
        let ends = |j: usize| j == chars.len() || sep(chars[j]);
        if ends(after) || (at(after, &['i', 'n', 'g']) && ends(after + 3)) { return true; }
    }
    false
}

/// readGlbCharacter（manifest なしの .glb。前は +X・高さ 1 m・接地は一番下）。id は SHA の頭 24 文字、名前はファイル名
pub fn read_glb_character(bytes: &[u8], filename: &str) -> Result<CharacterAsset, String> {
    let run = || -> Result<CharacterAsset, String> {
        if bytes.len() > LIMIT_ARCHIVE { return fail("ファイルが大きすぎます。制作側で軽量化してください。"); }
        let info = inspect_model(bytes)?;
        validate_character_gltf(&info.gltf)?;
        let root_motion = glb_root_motion(&info.gltf)?;
        let animations = arr(&info.gltf["animations"]);
        let find = |action: &str| -> Option<String> {
            info.clips.get(action).and_then(Value::as_str).map(String::from)
                .or_else(|| animations.iter().find_map(|a| a["name"].as_str().filter(|n| names_action(n, action)).map(String::from)))
        };
        let (idle, walk, run) = (find("idle"), find("walk"), find("run"));
        let (Some(idle), Some(walk)) = (idle, walk) else { return fail("このGLBに待機・歩行の動作が見つかりません。動作入りのGLBを選んでください。") };
        let sha = hash(bytes);
        let base = filename.rsplit(['/', '\\']).next().unwrap_or("");
        let base = if ends_glb(base) { &base[..base.len() - 4] } else { base };
        let mut clips = json!({ "idle": idle, "walk": walk });
        if let Some(r) = run { clips["run"] = json!(r); }
        let mut motion = json!({ "clips": clips, "expressions": info.expressions });
        if let Some(r) = root_motion { motion["rootMotion"] = r; }
        let manifest = json!({
            "id": format!("local-{}", &sha[..24]),
            "name": if base.is_empty() { "持ち込んだキャラ" } else { base },
            "model": { "sha256": sha, "bytes": bytes.len() },
            "coordinates": { "up": "+Y", "forward": "+X", "ground": "bounds-min-y", "heightMeters": 1 },
            "motion": motion,
        });
        Ok(CharacterAsset { source: AssetSource::Glb, manifest, model: Arc::new(bytes.to_vec()), thumbnail: None, stats: None })
    };
    run().map_err(|e| normalize_error(&e))
}

/// readCharacterFile: 先頭が GLB の印か、名前が .glb なら manifest なしの .glb、ほかは .character.zip
pub fn read_character_file(bytes: &[u8], filename: &str) -> Result<CharacterAsset, String> {
    let glb = bytes.len() >= 4 && u32le(bytes, 0) == Some(0x4654_6c67);
    let named = ends_glb(filename);
    if glb || named { read_glb_character(bytes, filename) } else { read_character_package(bytes) }
}

// ---- 組み立てる ----

/// 読んだ主役から動かす部品を作る（character-runtime 0.2.7 の createCharacterInstance。PackageActor の体）。描くのは主役と同じ（character-view・kit.set_actor）。
/// registry は character-motion の登録表のその主役の行（aisns の registeredCharacter(id, sha)。無ければ None）。id とモデルの SHA が合い、
/// stillBones に level があれば、PackageActor の KeepHeldLevel と同じく持つ物を世界で水平に保つ（マスター・ナギのお盆）。
/// 動かし方: move_runtime(moving, running)・set_npc・set_speech・express・set_photo・reset_spring・set_parent_world・update。
/// manifest に swordIdle・swordAttack があれば、配布の主役と同じく構えと振りも使える（start・hit は主役と同じ）。
/// GLB に jump・dodge・hit という名前のクリップがあれば、ジャンプ・回避・のけぞり（hit のモーフの重みと ＞＜ の節点の大きさも）に使う（Character Studio の書き出し）
pub fn assemble_package(asset: &CharacterAsset, registry: Option<&Value>) -> Result<CharacterActor, String> {
    let pm = &asset.manifest;
    let model = &asset.model;
    // TS の読み手が確かめない範囲の壊れ方で、Rust の読み手が止まらないように先に確かめる
    let info = inspect_model(model).map_err(|e| normalize_error(&e))?;
    let jlen = u32le(model, 12).unwrap_or(0) as usize;
    let bin_len = u32le(model, 20 + jlen).unwrap_or(0) as usize;
    check_ranges(&info.gltf, bin_len)?;
    let manifest = json!({
        "id": pm["id"], "revision": "", "name": pm["name"],
        "actor": { "heightMeters": pm["coordinates"]["heightMeters"] },
        "files": {}, "packageManifest": pm,
    });
    let key = "package".to_string();
    let sha = pm["model"]["sha256"].as_str().unwrap_or("").to_string();
    let plan = ReleasePlan { model_sha256: sha.clone(), model: key.clone(), files: vec![key.clone()], still: registry.and_then(|r| still_bones(r, pm["id"].as_str().unwrap_or(""), &sha)), own_by_name: true, ..Default::default() };
    let data: HashMap<String, Arc<Vec<u8>>> = [(key, model.clone())].into_iter().collect();
    let o = LoadOptions { id: pm["id"].as_str().unwrap_or("").to_string(), revision: None, quality: Quality::Full, gait: Gait::default() };
    actor::assemble(&o, &plan, &manifest, &data).map_err(|e| normalize_error(&e))
}

/// character-motion の stillBones(id, sha)（registeredCharacter で id と配布のモデルの SHA が合う行の stillBones。forModel があるときだけ）
fn still_bones(row: &Value, id: &str, sha: &str) -> Option<StillBones> {
    if row["id"].as_str() != Some(id) || row["delivered"]["modelSha256"].as_str() != Some(sha) { return None; }
    let st = row.get("stillBones")?;
    st.get("forModel")?;
    let bones: Vec<String> = st.get("bones")?.as_array()?.iter().filter_map(|b| b.as_str().map(String::from)).collect();
    let pose_clip = st.get("poseClip")?.as_str()?.to_string();
    let level = st.get("level").and_then(|l| {
        let a = l.get("localUp")?.as_array()?;
        Some(StillLevel { bone: l.get("bone")?.as_str()?.to_string(), local_up: [a[0].as_f64()? as f32, a[1].as_f64()? as f32, a[2].as_f64()? as f32] })
    });
    Some(StillBones { bones, pose_clip, level, motions: st.get("motions").cloned() })
}

#[cfg(test)]
mod tests {
    use super::*;
    /// 防ぐ故障: 動作の名前の見つけ方（read-glb.mjs の正規表現）が TS と変わる
    #[test]
    fn clip_names_follow_read_glb() {
        assert!(names_action("Armature|Idle", "idle"));
        assert!(names_action("walking", "walk"));
        assert!(names_action("mixamo.com:Run", "run"));
        // run + ing ではない（running は run・n・ing）
        assert!(!names_action("A_RUNNING_B", "run"));
        assert!(names_action("Walking_B", "walk"));
        assert!(!names_action("idler", "idle"));
        assert!(!names_action("sidle", "idle"));
        assert!(!names_action("prerun", "run"));
        assert!(names_action("x run", "run"));
        assert!(!names_action("walkings", "walk"));
    }
    /// 防ぐ故障: 日本語でない失敗の文言を、そのまま利用者に出す
    #[test]
    fn errors_are_normalized() {
        assert_eq!(normalize_error("invalid zip data"), BROKEN);
        assert_eq!(normalize_error("モデルの版が未対応です。"), "モデルの版が未対応です。");
    }
}
