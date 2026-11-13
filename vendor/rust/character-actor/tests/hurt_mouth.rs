// ＞＜ の口（登録表の hurtFace.style.mouth。主役の多くは aa）が、のけぞりの無いコマに話す口・表情の口を消さないか。
// 防ぐ故障: aisns の Rust で描く主役（hero.rs。配布の版で ＞＜ がある）が広場で話しても、口の aa がいつも 0 のまま（＞＜ の口が毎コマ 0 を書いていた。2026-11-13）。
// 主役は aisns の同梱の版（catalog.json の zip。台帳の表情の無いあいちゃんと、ある カイ）。＞＜ は登録表の style を当てた版と当てない版で比べる。
use character_actor::actor::{Gait, LoadOptions, Quality, assemble, release_plan};
use character_actor::hurt_face::HurtFaceStyle;
use character_actor::mouth::CharacterSpeech;
use character_actor::package;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

fn repo() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..") }
fn aisns() -> PathBuf { std::env::var("RUNTIME_PARITY_AISNS").map(PathBuf::from).unwrap_or_else(|_| repo().join("../aisns")) }

#[test]
fn hurt_mouth_keeps_speech_mouth() {
    let catalog = aisns().join("public/character-packages/catalog.json");
    if !catalog.exists() { println!("aisns のフォルダが無いので飛ばす（RUNTIME_PARITY_AISNS）"); return; }
    let catalog: Value = serde_json::from_slice(&std::fs::read(catalog).unwrap()).unwrap();
    let registry: Value = serde_json::from_slice(&std::fs::read(repo().join("packages/character-motion/src/registry.json")).unwrap()).unwrap();
    for id in ["aichan", "kai"] {
        let row = catalog["characters"].as_array().unwrap().iter().find(|c| c["manifest"]["id"] == id).unwrap();
        let asset = package::read_character_file(&std::fs::read(aisns().join("public/character-packages").join(row["package"].as_str().unwrap())).unwrap(), "x.character.zip").unwrap();
        let style = registry["characters"].as_array().unwrap().iter().find(|c| c["id"] == id).and_then(|c| HurtFaceStyle::from_json(&c["hurtFace"]["style"])).unwrap();
        assert!(style.mouth.iter().any(|(k, _)| k == "aa"), "{id} の ＞＜ の口に aa が無い");
        let mut files = json!({ "full": { "path": "", "sha256": asset.manifest["model"]["sha256"], "bytes": asset.model.len() } });
        let mut data: HashMap<String, Arc<Vec<u8>>> = [("full".to_string(), asset.model.clone())].into_iter().collect();
        for f in ["biped-hit.glb", "biped-rest-down.glb", "biped-rest-loop.glb", "biped-rest-up.glb"] {
            files[format!("clip:{f}")] = json!({ "path": format!("/animations/{f}"), "sha256": "", "bytes": 0 });
            data.insert(format!("clip:{f}"), Arc::new(std::fs::read(aisns().join("public/animations").join(f)).unwrap()));
        }
        let manifest = json!({ "id": id, "revision": "bundled", "files": files, "actor": { "heightMeters": asset.manifest["coordinates"]["heightMeters"] }, "packageManifest": asset.manifest });
        let o = LoadOptions { id: id.to_string(), revision: None, quality: Quality::Full, gait: Gait { walk: 0.15, run: 2.5 } };
        let plain = release_plan(&manifest, Quality::Full);
        let mut hurt = plain.clone();
        hurt.hurt_face = Some(style.clone());
        let (mut a, mut b) = (assemble(&o, &plain, &manifest, &data).unwrap(), assemble(&o, &hurt, &manifest, &data).unwrap());
        // 口の aa のモーフ（台帳の表情があれば、その aa が指すモーフ）
        let aa_name = asset.manifest["motion"]["face"]["morphs"]["aa"].as_str().unwrap_or("aa").to_string();
        let aa: Vec<(usize, usize)> = b.prims.iter().filter(|p| p.morph_count > 0)
            .flat_map(|p| b.glb.meshes[b.glb.nodes[p.node].mesh.unwrap()].target_names.iter().enumerate().filter(|(_, n)| **n == aa_name).map(|(i, _)| (p.node, i)).collect::<Vec<_>>())
            .collect();
        assert!(!aa.is_empty(), "{id} に {aa_name} のモーフが無い");
        let speech = Some(CharacterSpeech::new("こんにちは、今日はいい天気ですね", 0.0));
        let (mut clock, mut most, mut hit_most) = (0.0f64, 0f32, 0f32);
        for f in 0..240 {
            let dt = 1.0 / 60.0;
            clock += dt;
            // 1.5 秒目にのけぞる（話している間。＞＜ の口が書く）
            if f == 90 { b.hit(None, None); }
            for x in [&mut a, &mut b] { x.move_(0.0, true, 0.0, None); x.set_speech(speech); x.update(dt, clock); }
            let wa = aa.iter().fold(0f32, |m, (n, i)| m.max(a.skel.morphs[*n][*i]));
            let wb = aa.iter().fold(0f32, |m, (n, i)| m.max(b.skel.morphs[*n][*i]));
            if f < 90 {
                // のけぞる前: ＞＜ のある版も無い版と同じ口
                assert!((wa - wb).abs() < 1e-6, "{id} f{f}: ＞＜ のある版の aa {wb} が無い版 {wa} と違う");
                most = most.max(wb);
            } else if f < 110 { hit_most = hit_most.max(wb); }
        }
        assert!(most > 0.3, "{id}: 話している間に aa が開かない（{most}）");
        assert!(hit_most > 0.0, "{id}: のけぞりの間に ＞＜ の口が書かれない");
        // 話し終えてのけぞりも終われば、＞＜ の口は 0（台帳の表情の無い主役は読んだときの値）
        for _ in 0..60 { clock += 1.0 / 60.0; for x in [&mut a, &mut b] { x.set_speech(None); x.update(1.0 / 60.0, clock); } }
        let wa = aa.iter().fold(0f32, |m, (n, i)| m.max(a.skel.morphs[*n][*i]));
        let wb = aa.iter().fold(0f32, |m, (n, i)| m.max(b.skel.morphs[*n][*i]));
        assert!(wb.abs() < 1e-6 && wa.abs() < 1e-6, "{id}: 話し終えたあと aa が残る（＞＜ あり {wb}・なし {wa}）");
        println!("{id}: 話す口の aa の最大 {most:.3}・のけぞりの間 {hit_most:.3}");
    }
}
