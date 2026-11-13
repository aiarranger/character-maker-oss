// 配布を通らない主役（.character.zip・manifest なしの持ち込みの .glb）の読み方と動きを、TS の配布済みの部品（character-runtime 0.2.7）と比べる。
// 防ぐ故障: aisns の広場・島の主役（PackageActor）・ほかの人（RemotePlayers の isNpc）・持ち込みを Rust で描いたとき、読める・読めないの判断と文言、
//   manifest、待機・歩き・走り・間引き・話す口・表情・写真の姿・揺れ物の付け直し・持つ物を水平に保つ（KeepHeldLevel）が TS と変わる。
// NPC・汎用（0.2.5 の写し）は character-actor-npc の tests/npc_parity.rs。
// TS の値は tools/runtime-parity.mjs が書き出す（無い・部品の配布や場面の表や道具が変わったときはこの試験が動かす。node と aisns のフォルダが要る。通信は要らない）。
// 場面は tools/runtime-scenes.json を両方が読む。入れ物の動き・合図の当て方は runtime-parity.mjs の frame と同じ。
use character_actor::actor::{CharacterActor, FaceControl, PhotoPose};
use character_actor::mouth::CharacterSpeech;
use character_actor::package::{self, CharacterAsset};
use glam::{DMat4, DQuat, DVec3};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// 位置はメートル、向きは単位ベクトルの成分（ts_parity.rs と同じ許容）
const BONE_LIMIT: f32 = 2.5e-3;
const MORPH_LIMIT: f32 = 1e-3;
const DT: f64 = 1.0 / 60.0;
const EVERY: i64 = 2;
const UNEVEN: [f64; 10] = [1.0 / 60.0, 1.0 / 60.0, 0.2, 0.001, 0.0, 1.0 / 30.0, 1.0 / 144.0, 1.0 / 24.0, 0.05, 0.051];
const SCENES: &str = include_str!("../tools/runtime-scenes.json");

fn repo() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..") }
fn aisns() -> PathBuf { std::env::var("RUNTIME_PARITY_AISNS").map(PathBuf::from).unwrap_or_else(|_| repo().join("../aisns")) }
fn store() -> PathBuf { PathBuf::from(std::env::var("HOME").unwrap()).join(".local/share/muso-rust/runtime-parity") }

fn face(name: &str) -> FaceControl {
    match name {
        "happy" => FaceControl { emotion: "happy", emotion_weight: 0.8, mouth: "aa", mouth_weight: 0.5, look: "lookUp", look_weight: 0.6 },
        "sad" => FaceControl { emotion: "sad", emotion_weight: 1.0, mouth: "neutral", mouth_weight: 1.0, look: "neutral", look_weight: 1.0 },
        _ => FaceControl { emotion: "surprised", emotion_weight: 0.9, mouth: "oh", mouth_weight: 0.7, look: "lookLeft", look_weight: 0.5 },
    }
}

/// readCharacterFile の結果を TS の書き出しと同じ形に
fn read_doc(r: &Result<CharacterAsset, String>) -> Value {
    match r {
        Ok(a) => json!({
            "ok": true, "manifest": a.manifest, "source": if a.source == package::AssetSource::Package { "package" } else { "glb" },
            "stats": a.stats.map(|(d, t)| json!({ "decodedBytes": d, "triangles": t })),
            "thumbnail": a.thumbnail.as_ref().map(|t| character_actor::delivery::hash(t)),
        }),
        Err(e) => json!({ "ok": false, "error": e }),
    }
}

/// runtime-parity.mjs の brokenInputs と同じ作り方
fn broken_inputs() -> Vec<(&'static str, Vec<u8>, &'static str)> {
    let zip = std::fs::read(aisns().join("public/character-packages/generic-human-b-d8b26fa368e7.character.zip")).unwrap();
    let glb = std::fs::read(aisns().join("public/models/welcome/socrates-3a8d975e7fea.glb")).unwrap();
    let layout = std::fs::read(repo().join("public/hero-weapons/kiyogon/kiyogon-ba45496e1d0c.character.zip")).unwrap();
    let flip = |b: &[u8], at: usize| { let mut c = b.to_vec(); c[at] ^= 0xff; c };
    vec![
        ("zip-half", zip[..zip.len() >> 1].to_vec(), "a.character.zip"),
        ("zip-eocd", flip(&zip, zip.len() - 22), "a.character.zip"),
        ("zip-data", flip(&zip, 2000), "a.character.zip"),
        ("zip-layout", layout, "kiyogon.character.zip"),
        ("glb-short", glb[..1000].to_vec(), "socrates.glb"),
        ("glb-magic", flip(&glb, 0), "socrates.glb"),
        ("glb-json", flip(&glb, 40), "socrates.glb"),
        ("glb-bin", glb[..glb.len() - 4].to_vec(), "socrates.glb"),
        ("empty", Vec::new(), "x.character.zip"),
    ]
}

struct Worst { bones: f32, at: String, morph: f32, morph_at: String }

/// 1 人を TS の場面どおりに動かして比べる
#[allow(unused_assignments)]
fn compare_motion(a: &mut CharacterActor, ts: &Value, bin: &[f32], bad: &mut Vec<String>, name: &str) -> (Worst, usize) {
    let nodes: Vec<Option<usize>> = ts["nodes"].as_array().unwrap().iter().map(|n| {
        let n = n.as_str().unwrap();
        let f = a.skel.find(n);
        if f.is_none() { bad.push(format!("{name} 節点が無い {n}")); }
        f
    }).collect();
    // モーフは three の物の並び（traverse の順・物ごとにモーフの番号の順）で合わせる。three の物の名前は GLTFLoader が節点とメッシュの名前を
    // 読み込みの終わった順に重ならないようにした物（_1, _2 …）で、Rust の物の名前と合わないことがある
    let mut ours: Vec<(usize, usize, String)> = Vec::new();
    for p in a.prims.iter().filter(|p| p.morph_count > 0) {
        for (i, n) in a.glb.meshes[a.glb.nodes[p.node].mesh.unwrap()].target_names.iter().enumerate() { ours.push((p.node, i, n.clone())); }
    }
    let theirs = ts["morphs"].as_array().unwrap();
    if ours.len() != theirs.len() { bad.push(format!("{name} モーフの数が違う rust {} ts {}", ours.len(), theirs.len())); }
    let morphs: Vec<Option<(usize, usize)>> = theirs.iter().enumerate().map(|(i, m)| {
        let (node, idx, key) = ours.get(i)?;
        if m[1].as_str() != Some(key.as_str()) { bad.push(format!("{name} {i} 番目のモーフが違う rust {key} ts {m}")); return None; }
        Some((*node, *idx))
    }).collect();
    let stride = ts["stride"].as_u64().unwrap() as usize;
    let meta = ts["meta"].as_array().unwrap();
    let scenes: Value = serde_json::from_str(SCENES).unwrap();
    let mut w = Worst { bones: 0.0, at: String::new(), morph: 0.0, morph_at: String::new() };
    let (mut x, mut z, mut heading, mut clock) = (0f64, 0f64, 0f64, 0f64);
    let (mut moving, mut running, mut npc) = (false, false, false);
    let (mut speech, mut look, mut photo): (Option<CharacterSpeech>, Option<FaceControl>, Option<PhotoPose>) = (None, None, None);
    let mut reset = false;
    let frame = |a: &mut CharacterActor, dt: f64, moving: bool, running: bool, npc: bool, speech: Option<CharacterSpeech>, look: Option<FaceControl>, photo: Option<PhotoPose>, reset: &mut bool, x: &mut f64, z: &mut f64, heading: &mut f64, clock: &mut f64| {
        let speed = if moving { if running { 3.5 } else { 1.0 } } else { 0.0 };
        *heading += if moving { 0.6 } else { 0.0 } * dt;
        *x += heading.sin() * speed * dt;
        *z += heading.cos() * speed * dt;
        a.set_parent_world(DMat4::from_rotation_translation(DQuat::from_rotation_y(*heading), DVec3::new(*x, 0.0, *z)).as_mat4());
        *clock += dt;
        a.move_runtime(moving, running);
        a.set_npc(npc);
        a.set_speech(speech);
        a.express(look);
        a.set_photo(photo);
        if std::mem::take(reset) { a.reset_spring(); }
        a.update(dt, *clock);
    };
    let (mut mi, mut compared) = (0usize, 0usize);
    let once = [json!("60")];
    for sc in scenes.as_array().unwrap() {
        for dt_kind in sc["dts"].as_array().map(Vec::as_slice).unwrap_or(&once) {
            (moving, running, speech, look, photo, npc) = (false, false, None, None, None, sc["npc"].as_bool().unwrap_or(false));
            for _ in 0..30 { frame(a, DT, moving, running, npc, speech, look, photo, &mut reset, &mut x, &mut z, &mut heading, &mut clock); }
            let kind = dt_kind.as_str().unwrap();
            let pattern: &[f64] = match kind { "60" => &[DT], "144" => &[1.0 / 144.0], _ => &UNEVEN };
            let tag = format!("{}@{kind}", sc["name"].as_str().unwrap());
            let list = sc["do"].as_array().unwrap();
            let mut done = vec![false; list.len()];
            let (mut t, mut k) = (0f64, 0i64);
            while t < sc["seconds"].as_f64().unwrap() - 1e-9 {
                let dt = pattern[k as usize % pattern.len()];
                for (ci, c) in list.iter().enumerate() {
                    if done[ci] || t + 1e-9 < c["at"].as_f64().unwrap() { continue; }
                    done[ci] = true;
                    if let Some(v) = c["moving"].as_bool() { moving = v; }
                    if let Some(v) = c["running"].as_bool() { running = v; }
                    if let Some(v) = c.get("speech") { speech = v.as_str().map(|s| CharacterSpeech::new(s, clock)); }
                    if let Some(v) = c.get("face") { look = v.as_str().map(face); }
                    if let Some(v) = c.get("photo") { photo = v.as_str().map(|p| if p == "walk" { PhotoPose::Walk } else { PhotoPose::Idle }); }
                    if let Some(p) = c["teleport"].as_array() { x = p[0].as_f64().unwrap(); z = p[1].as_f64().unwrap(); }
                    if c.get("springReset").is_some() { reset = true; }
                }
                frame(a, dt, moving, running, npc, speech, look, photo, &mut reset, &mut x, &mut z, &mut heading, &mut clock);
                t += dt;
                if k % EVERY == 0 {
                    let m = &meta[mi];
                    assert!(m[0] == tag.as_str() && m[1] == k, "{name} 場面の順がずれた {m} {tag} {k}");
                    let row = &bin[mi * stride..(mi + 1) * stride];
                    mi += 1;
                    compared += 1;
                    let mut n = 0;
                    for (j, node) in nodes.iter().enumerate() {
                        if let Some(node) = node {
                            let e = a.skel.world[*node];
                            let vals = [e.w_axis.x, e.w_axis.y, e.w_axis.z, e.x_axis.x, e.x_axis.y, e.x_axis.z, e.y_axis.x, e.y_axis.y, e.y_axis.z];
                            for (q, v) in vals.iter().enumerate() {
                                let d = (v - row[n + q]).abs();
                                if !(d <= w.bones) { w.bones = d; w.at = format!("{tag} f{k} {}[{q}] rust {v} ts {}", ts["nodes"][j], row[n + q]); }
                            }
                        }
                        n += 9;
                    }
                    for (j, mo) in morphs.iter().enumerate() {
                        if let Some((node, idx)) = mo {
                            let v = a.skel.morphs[*node][*idx];
                            let d = (v - row[n + j]).abs();
                            if !(d <= w.morph) { w.morph = d; w.morph_at = format!("{tag} f{k} {} rust {v} ts {}", ts["morphs"][j], row[n + j]); }
                        }
                    }
                }
                k += 1;
            }
        }
    }
    if mi != meta.len() { bad.push(format!("{name} 比べた点の数が違う {mi} {}", meta.len())); }
    (w, compared)
}

/// TS の値のフォルダ（無ければ 1 回だけ作る。2 つの試験が同時に作らない）
fn values() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let release = tool_release();
        let dir = store().join(release["key"].as_str().unwrap());
        if !dir.join("items.json").exists() || !dir.join("materials.json").exists() {
            let st = std::process::Command::new("node").arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/runtime-parity.mjs")).arg(&dir).env("RUNTIME_PARITY_AISNS", aisns()).status().expect("node を動かせません");
            assert!(st.success(), "TS の値を作れませんでした");
        }
        dir
    }).clone()
}

fn tool_release() -> Value {
    let out = std::process::Command::new("node").arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/runtime-parity.mjs")).arg("--release-info")
        .env("RUNTIME_PARITY_AISNS", aisns()).output().expect("node を動かせません");
    assert!(out.status.success(), "部品の配布の照合に失敗: {}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn hero_packages_match_ts() {
    if !aisns().join("lib/welcome-npcs.json").exists() { println!("aisns のフォルダが無いので飛ばす（RUNTIME_PARITY_AISNS）"); return; }
    let dir = values();
    let mut bad: Vec<String> = Vec::new();
    // 壊れた入力の文言（主役用と汎用の読み手）
    let errors: Value = serde_json::from_slice(&std::fs::read(dir.join("errors.json")).unwrap()).unwrap();
    let inputs = broken_inputs();
    for e in errors["errors"].as_array().unwrap() {
        let (_, bytes, filename) = inputs.iter().find(|(n, _, _)| *n == e["name"].as_str().unwrap()).unwrap();
        let got = read_doc(&package::read_character_file(bytes, filename));
        if got["ok"] != e["ok"] || got["error"] != e["error"] { bad.push(format!("壊れた入力 {} {}: rust {} ts {}", e["name"], e["kind"], got["error"], e["error"])); }
    }
    let items: Value = serde_json::from_slice(&std::fs::read(dir.join("items.json")).unwrap()).unwrap();
    let (mut total, mut worst_bone, mut worst_morph) = (0usize, 0f32, 0f32);
    for name in items["items"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        let ts: Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{name}.json"))).unwrap()).unwrap();
        let bytes = std::fs::read(aisns().join(ts["file"].as_str().unwrap())).unwrap();
        let filename = ts["filename"].as_str().unwrap();
        let read = package::read_character_file(&bytes, filename);
        let doc = read_doc(&read);
        if doc != ts["read"] { bad.push(format!("{name} の読み方が違う\n  rust {doc}\n  ts   {}", ts["read"])); continue; }
        if ts["readOnly"].as_bool().unwrap_or(false) { continue; }
        let Ok(asset) = read else { continue };
        let registry = ts["registry"].is_object().then_some(&ts["registry"]);
        let mut a = match package::assemble_package(&asset, registry) { Ok(a) => a, Err(e) => { bad.push(format!("{name} を組み立てられない: {e}")); continue } };
        if a.spring.is_some() != ts["spring"].as_bool().unwrap() { bad.push(format!("{name} 揺れ物の有無が違う")); }
        let bin: Vec<f32> = std::fs::read(dir.join(format!("{name}.bin"))).unwrap().chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        let (w, n) = compare_motion(&mut a, &ts, &bin, &mut bad, name);
        total += n;
        worst_bone = worst_bone.max(w.bones);
        worst_morph = worst_morph.max(w.morph);
        println!("{name}: {n} 点 | 骨 {:.6}（{}）| モーフ {:.6}（{}）", w.bones, w.at, w.morph, w.morph_at);
        if !(w.bones <= BONE_LIMIT) { bad.push(format!("{name} 骨の差 {:.5}（{}）", w.bones, w.at)); }
        if !(w.morph <= MORPH_LIMIT) { bad.push(format!("{name} モーフの差 {:.5}（{}）", w.morph, w.morph_at)); }
    }
    println!("{} 人・{total} 点・骨の最大の差 {worst_bone:.6}・モーフ {worst_morph:.6}・壊れた入力 {} 通り", items["items"].as_array().unwrap().len(), errors["errors"].as_array().unwrap().len());
    assert!(bad.is_empty(), "TS の部品と違う:\n{}", bad.join("\n"));
}

/// 防ぐ故障: 地域の敵の透過 8 体などの KHR_materials_transmission・volume・ior の値を、three と違う値で読む（描くのは island-kit）
#[test]
fn transmission_values_match_three() {
    if !aisns().join("lib/welcome-npcs.json").exists() { println!("aisns のフォルダが無いので飛ばす（RUNTIME_PARITY_AISNS）"); return; }
    let dir = values();
    let doc: Value = serde_json::from_slice(&std::fs::read(dir.join("materials.json")).unwrap()).unwrap();
    let mut bad = Vec::new();
    let mut physical = 0;
    for f in doc["materials"].as_array().unwrap() {
        let g = character_actor::glb::parse(&std::fs::read(aisns().join("public").join(f["file"].as_str().unwrap())).unwrap()).unwrap();
        let ts = f["materials"].as_array().unwrap();
        if ts.len() != g.materials.len() { bad.push(format!("{} 材質の数が違う", f["file"])); continue; }
        for (m, t) in g.materials.iter().zip(ts) {
            let close = |a: f32, b: &Value| b.as_f64().is_some_and(|b| (a as f64 - b).abs() < 1e-6);
            match (&m.transmission, t["type"].as_str() == Some("MeshPhysicalMaterial")) {
                (None, false) => {}
                (Some(x), true) => {
                    physical += 1;
                    let dist_ok = if t["attenuationDistance"] == "inf" { x.attenuation_distance.is_infinite() } else { close(x.attenuation_distance, &t["attenuationDistance"]) };
                    let color_ok = (0..3).all(|i| close(x.attenuation_color[i], &t["attenuationColor"][i]));
                    if !close(x.transmission, &t["transmission"]) || !close(x.thickness, &t["thickness"]) || !close(x.ior, &t["ior"]) || !dist_ok || !color_ok {
                        bad.push(format!("{} {} rust {x:?} ts {t}", f["file"], m.name));
                    }
                }
                _ => bad.push(format!("{} {} の材質の種類が違う rust {:?} ts {}", f["file"], m.name, m.transmission, t["type"])),
            }
        }
    }
    println!("透過の材質 {physical} 個");
    assert!(physical >= 10, "透過の材質が少ない {physical}");
    assert!(bad.is_empty(), "three と違う:\n{}", bad.join("\n"));
}

const ACTOR_SCENES: &str = include_str!("../tools/runtime-actor-scenes.json");

fn ev_text(f: i64, e: &character_actor::actor::ActorEvent) -> String {
    use character_actor::actor::{ActorEvent, Cue};
    let a = |k: &character_actor::actor::ActionKind| format!("{k:?}").to_lowercase();
    match e {
        ActorEvent::Start(k) => format!("{f} start {}", a(k)),
        ActorEvent::Interrupt(k) => format!("{f} interrupt {}", a(k)),
        ActorEvent::End(k) => format!("{f} end {}", a(k)),
        ActorEvent::Cue(k, c) => format!("{f} cue {} {}", a(k), match c { Cue::Strike => "strike", Cue::Takeoff => "takeoff", Cue::Land => "land", Cue::Seated => "seated" }),
    }
}

/// 防ぐ故障: 主役の部品で、写真の間に動作・のけぞり・構え・必殺技の振りを止める決まり（休むは止めない）、話す口と表情、npc の間引きが TS（character-actor）と変わる
#[test]
#[allow(unused_assignments)]
fn actors_photo_speech_npc_match_ts() {
    use character_actor::actor::{ActionKind, Quality, StartOptions, LoadOptions, Gait, assemble, release_plan};
    if !aisns().join("lib/welcome-npcs.json").exists() { println!("aisns のフォルダが無いので飛ばす（RUNTIME_PARITY_AISNS）"); return; }
    let dir = values();
    let items: Value = serde_json::from_slice(&std::fs::read(dir.join("items.json")).unwrap()).unwrap();
    let scenes: Value = serde_json::from_str(ACTOR_SCENES).unwrap();
    let mut bad = Vec::new();
    let kind_of = |n: &str| match n { "attack" => ActionKind::Attack, "skill" => ActionKind::Skill, "rest" => ActionKind::Rest, _ => panic!("{n}") };
    for name in items["actors"].as_array().unwrap() {
        let name = name.as_str().unwrap();
        let ts: Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{name}.json"))).unwrap()).unwrap();
        let bin: Vec<f32> = std::fs::read(dir.join(format!("{name}.bin"))).unwrap().chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        // aisns の hero.rs と同じ: TS が組んだ版の manifest と、そのファイルから組み立てる
        let pkg = package::read_character_file(&std::fs::read(aisns().join("public/character-packages").join(ts["package"].as_str().unwrap())).unwrap(), "x.character.zip").unwrap();
        let mut data: std::collections::HashMap<String, std::sync::Arc<Vec<u8>>> = [("full".to_string(), pkg.model.clone())].into_iter().collect();
        for f in ts["shared"].as_array().unwrap() { let f = f.as_str().unwrap(); data.insert(format!("clip:{f}"), std::sync::Arc::new(std::fs::read(aisns().join("public/animations").join(f)).unwrap())); }
        let manifest = &ts["manifest"];
        let plan = release_plan(manifest, Quality::Full);
        let o = LoadOptions { id: ts["id"].as_str().unwrap().to_string(), revision: None, quality: Quality::Full, gait: Gait { walk: 0.15, run: 2.5 } };
        let mut a = assemble(&o, &plan, manifest, &data).unwrap();
        a.set_npc(ts["npc"].as_bool().unwrap());
        a.hold_weapon(Some(0.9));
        let nodes: Vec<Option<usize>> = ts["nodes"].as_array().unwrap().iter().map(|n| a.skel.find(n.as_str().unwrap())).collect();
        if nodes.iter().any(Option::is_none) { bad.push(format!("{name} 節点が足りない")); }
        let mut ours: Vec<(usize, usize, String)> = Vec::new();
        for p in a.prims.iter().filter(|p| p.morph_count > 0) {
            for (i, n) in a.glb.meshes[a.glb.nodes[p.node].mesh.unwrap()].target_names.iter().enumerate() { ours.push((p.node, i, n.clone())); }
        }
        let theirs = ts["morphs"].as_array().unwrap();
        if ours.len() != theirs.len() || ours.iter().zip(theirs).any(|(o, t)| t[1].as_str() != Some(o.2.as_str())) { bad.push(format!("{name} モーフの並びが違う")); continue; }
        let stride = ts["stride"].as_u64().unwrap() as usize;
        let meta = ts["meta"].as_array().unwrap();
        let (mut f, mut clock, mut mi) = (0i64, 0f64, 0usize);
        let (mut look, mut speech, mut photo): (Option<FaceControl>, Option<CharacterSpeech>, Option<PhotoPose>) = (None, None, None);
        let mut events: Vec<String> = Vec::new();
        let (mut wb, mut wb_at, mut wm, mut wm_at, mut wg) = (0f32, String::new(), 0f32, String::new(), 0f32);
        let once = [json!("60")];
        for sc in scenes.as_array().unwrap() {
            for dt_kind in sc["dts"].as_array().map(Vec::as_slice).unwrap_or(&once) {
                (look, speech, photo) = (None, None, None);
                if let Some(c) = a.current() { a.stop(c); }
                let step = |a: &mut CharacterActor, dt: f64, speed: f64, armed: f32, rate: f64, look: Option<FaceControl>, speech: Option<CharacterSpeech>, photo: Option<PhotoPose>, f: &mut i64, clock: &mut f64, events: &mut Vec<String>| {
                    a.move_(speed, true, armed, Some(rate));
                    a.set_photo(photo);
                    a.express(look);
                    a.set_speech(speech);
                    *clock += dt;
                    for e in a.update(dt, *clock) { events.push(ev_text(*f, &e)); }
                    *f += 1;
                };
                let (mut i, mut n) = (0, 0);
                while (i < 30 || a.current().is_some()) && n < 900 { step(&mut a, DT, 0.0, 0.0, 1.0, look, speech, photo, &mut f, &mut clock, &mut events); if a.current().is_none() { i += 1; } n += 1; }
                let kind = dt_kind.as_str().unwrap();
                let pattern: &[f64] = match kind { "60" => &[DT], "144" => &[1.0 / 144.0], _ => &UNEVEN };
                let tag = format!("{}@{kind}", sc["name"].as_str().unwrap());
                let every = sc["every"].as_i64().unwrap_or(1);
                let (speed, rate, armed) = (sc["speed"].as_f64().unwrap_or(0.0), sc["rate"].as_f64().unwrap_or(1.0), sc["armed"].as_f64().unwrap_or(0.0) as f32);
                let list = sc["do"].as_array().unwrap();
                let mut done = vec![false; list.len()];
                let (mut t, mut k) = (0f64, 0i64);
                while t < sc["seconds"].as_f64().unwrap() - 1e-9 {
                    let dt = pattern[k as usize % pattern.len()];
                    for (ci, c) in list.iter().enumerate() {
                        if done[ci] || t + 1e-9 < c["at"].as_f64().unwrap() { continue; }
                        done[ci] = true;
                        if let Some(s) = c["stop"].as_str() { a.stop(kind_of(s)); }
                        if let Some(s) = c["start"].as_str() { a.start(kind_of(s), StartOptions { seconds: c["opts"]["seconds"].as_f64(), ..Default::default() }); }
                        if let Some(h) = c.get("hit") { a.hit(h["seconds"].as_f64(), h["since"].as_f64()); }
                        if let Some(v) = c.get("speech") { speech = v.as_str().map(|s| CharacterSpeech::new(s, clock)); }
                        if let Some(v) = c.get("face") { look = v.as_str().map(face); }
                        if let Some(v) = c.get("photo") { photo = v.as_str().map(|p| if p == "walk" { PhotoPose::Walk } else { PhotoPose::Idle }); }
                    }
                    step(&mut a, dt, speed, armed, rate, look, speech, photo, &mut f, &mut clock, &mut events);
                    t += dt;
                    if k % every == 0 {
                        let m = &meta[mi];
                        assert!(m[0] == tag.as_str() && m[1] == k, "{name} 場面の順がずれた {m} {tag} {k}");
                        let row = &bin[mi * stride..(mi + 1) * stride];
                        mi += 1;
                        let mut c = 0;
                        for (j, node) in nodes.iter().enumerate() {
                            if let Some(node) = node {
                                let e = a.skel.world[*node];
                                for (q, v) in [e.w_axis.x, e.w_axis.y, e.w_axis.z, e.x_axis.x, e.x_axis.y, e.x_axis.z, e.y_axis.x, e.y_axis.y, e.y_axis.z].iter().enumerate() {
                                    let d = (v - row[c + q]).abs();
                                    if !(d <= wb) { wb = d; wb_at = format!("{tag} f{k} {}[{q}] rust {v} ts {}", ts["nodes"][j], row[c + q]); }
                                }
                            }
                            c += 9;
                        }
                        for (j, (node, idx, _)) in ours.iter().enumerate() {
                            let d = (a.skel.morphs[*node][*idx] - row[c + j]).abs();
                            if !(d <= wm) { wm = d; wm_at = format!("{tag} f{k} {} rust {} ts {}", theirs[j], a.skel.morphs[*node][*idx], row[c + j]); }
                        }
                        c += ours.len();
                        if let Some(g) = a.grip_world() { for (q, v) in g.to_cols_array().iter().enumerate() { if row[c + q].is_finite() { wg = wg.max((v - row[c + q]).abs()); } } }
                    }
                    k += 1;
                }
            }
        }
        let tsev: Vec<String> = ts["events"].as_array().unwrap().iter().map(|e| {
            let v = e.as_array().unwrap();
            if v[1] == "cue" { format!("{} cue {} {}", v[0], v[2].as_str().unwrap(), v[3].as_str().unwrap()) } else { format!("{} {} {}", v[0], v[1].as_str().unwrap(), v[2].as_str().unwrap()) }
        }).collect();
        println!("{name}: {mi} 点 | 骨 {wb:.6}（{wb_at}）| モーフ {wm:.6}（{wm_at}）| 持ち手 {wg:.6} | 合図 {}", events.len());
        if events != tsev { bad.push(format!("{name} 合図が違う\n  rust {events:?}\n  ts   {tsev:?}")); }
        if mi != meta.len() { bad.push(format!("{name} 比べた点の数が違う")); }
        if !(wb <= BONE_LIMIT) { bad.push(format!("{name} 骨の差 {wb:.5}（{wb_at}）")); }
        if !(wm <= MORPH_LIMIT) { bad.push(format!("{name} モーフの差 {wm:.5}（{wm_at}）")); }
        if !(wg <= 5e-3) { bad.push(format!("{name} 持ち手の差 {wg:.5}")); }
    }
    assert!(bad.is_empty(), "TS の部品と違う:\n{}", bad.join("\n"));
}
