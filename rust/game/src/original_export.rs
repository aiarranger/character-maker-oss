// オリジナル 4 体の GLB の書き出し（今の版は lib/character-maker/original-assembly.ts の exportOriginalCharacterGlb）。段 R6。
// 書く口は crate::export の GlbWriter（プリセットの preset() と同じ書き方）。木は今の版の書き出しと同じ:
//   CharacterMakerOriginal（大きさ・characterMaker の控え）> CharacterMakerOriginalCalibration（-90° と 2 倍）> GLB の scene > 骨と形
//   武器用（armed）は右手の骨の下に握った手（OriginalSwordRightHand）と武器（CharacterMakerSword。scale 1）。元の手は書かない。
//   通常用（standard）は武器も握った手も無い。頭の骨の下に ＞＜ の節点（maker-hurt-face。scale 0）と板 2 枚。
// 形は組み立てたときの値（部品・体型・髪）。表情は設定の値を静止のモーフの重みに（applyStaticFace）、顔の向きは頭の骨の静止の姿へ（applyStaticHeadTurn）。
// 動作は手を入れていない元の動作（武器用: 構える・振る、通常用: 待機・歩く・走る）＋ hit（＞＜ の節点の scale と、目を閉じるモーフ）＋ face_hurt。
// ゲーム用（game。魚群繚乱へ渡す）は通常用の木のまま、動作だけ 待機・歩く・走る＋構える・振る＋ゲーム用の動作＋hit・face_hurt。
// 位置・法線は 32bit 浮動小数で書く（KHR_mesh_quantization を使わない。今の版の yukikau.jp 用の dequantize と同じ形）。
use super::{Original, PartKind};
use crate::export::{GlbWriter, MeshOut, TrackOut, UniqueNames, clip_tracks};
use character_actor::anim::{Clip, Skeleton, three_decompose};
use character_actor::glb::{self, Glb, Primitive};
use glam::{Mat3, Mat4, Quat, Vec3};
use island_kit::{Kit, MatId, NodeId};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

const HURT_FACE_CLIP_DURATION: f32 = 1.0;
const FACE_KEYS: [&str; 15] = ["blinkLeft", "blinkRight", "mouthClose", "aa", "ee", "ou", "happy", "angry", "sad", "surprised", "relaxed", "lookLeft", "lookRight", "lookUp", "lookDown"];

fn blink_corrective(name: &str, closure: f64) -> Option<f64> { super::rig::blink_corrective(name, closure) }

/// applyStaticFace（設定の表情を静止の重みに。自動のまばたき・話す口は入れない）
fn static_face(o: &Original, names: &[String], init: &[f32]) -> Vec<f32> {
    let mut w = init.to_vec();
    w.resize(names.len(), 0.0);
    // 気持ちの表情の口（元の口のモーフの組み合わせ）も、下見と同じに書く
    let face = o.face.with_emotion_mouth(1.0, o.character == "generic-robot");
    for (i, name) in names.iter().enumerate() {
        if i >= w.len() { break; }
        if let Some(rest) = name.strip_prefix("mouthCloseInbetween") && let Ok(c) = rest.parse::<f64>() && [25.0, 50.0, 75.0].contains(&c) {
            w[i] = (1.0 - (face.mouth_close - c / 100.0).abs() / 0.25).max(0.0) as f32;
            continue;
        }
        let blink = if name.starts_with("blinkLeft") { face.blink_left } else { face.blink_right };
        if let Some(c) = blink_corrective(name, blink) { w[i] = c as f32; continue; }
        let arc = name.ends_with("Arc");
        let key = name.strip_suffix("Arc").unwrap_or(name);
        if !FACE_KEYS.contains(&key) { continue; }
        let f = face.get(key).unwrap_or(0.0);
        w[i] = (if arc { (std::f64::consts::PI * f).sin() } else { f }) as f32;
    }
    w
}

/// 骨付きの形を、焼いた bind から形の局所へ戻す（ナミの Armature の小さなずれ。ほかは単位）
fn local_prim(p: &Primitive, bind0: Mat4) -> Primitive {
    let mut q = p.clone();
    if bind0 == Mat4::IDENTITY { return q; }
    let inv = bind0.inverse();
    let lin = Mat3::from_mat4(inv);
    let nm = lin.inverse().transpose();
    for v in q.positions.iter_mut() { *v = inv.transform_point3(Vec3::from(*v)).to_array(); }
    for v in q.normals.iter_mut() { *v = (nm * Vec3::from(*v)).normalize_or_zero().to_array(); }
    for t in q.morph_pos.iter_mut() { for v in t.iter_mut() { *v = (lin * Vec3::from(*v)).to_array(); } }
    for t in q.morph_nor.iter_mut() { for v in t.iter_mut() { *v = (nm * Vec3::from(*v)).to_array(); } }
    q
}

/// 武器の kit の木をそのまま書く（GLB の節点の部分が 1 つなら、three と同じく節点そのものを形にする）
fn sword_node(w: &mut GlbWriter, kit: &Kit, n: NodeId, parent: usize, wp: &crate::weapon::Weapon, mat: &mut dyn FnMut(&mut GlbWriter, MatId) -> usize, root: Option<(Value, Vec3)>) -> usize {
    let k = &kit.scene[n];
    let scale = root.as_ref().map_or(k.scale, |r| r.1);
    let id = w.node(Some(parent), &k.name, k.position, k.rotation.to_quat(), scale, root.map(|r| r.0));
    let children = kit.scene[n].children().to_vec();
    let only = if children.len() == 1 && kit.scene[children[0]].name.is_empty() && wp.parts.iter().any(|(pn, _, _)| *pn == children[0]) { Some(children[0]) } else { None };
    if let Some((_, q, mid)) = wp.parts.iter().find(|(pn, _, _)| *pn == n || Some(*pn) == only) {
        let material = Some(mat(w, *mid));
        w.mesh(id, &MeshOut { name: k.name.clone(), prim: q, positions: None, normals: None, material, target_names: Vec::new(), weights: Vec::new(), skin: None });
    }
    if only.is_none() { for ch in children { sword_node(w, kit, ch, id, wp, mat, None); } }
    id
}

/// ゲーム用の動作の名前（lib/character-maker/actions.ts と同じ）
const ACTIONS: [&str; 16] = ["jump", "dodge", "restDown", "restLoop", "restUp", "banzai", "greet", "spinKick", "eat", "skill", "punch", "interact", "down", "dance", "talk", "crouch"];

pub fn export(o: &mut Original, kit: &mut Kit, v: &Value) -> Result<Vec<u8>, String> {
    let armed = v["variant"].as_str() == Some("armed");
    // ゲーム用（魚群繚乱。docs/gyogun-spec.md）: 待機・歩く・走ると構える・振るを 1 つの GLB に入れる。武器と握った手は入れない（武器はゲームが持たせる）
    let game = v["variant"].as_str() == Some("game");
    let g: Arc<Glb> = o.g.clone();
    let motions: Vec<String> = v["motions"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_else(|| ["idle", "walk", "run", "sword_idle", "sword_attack"].iter().map(|s| s.to_string()).collect());
    // ---- 動作の選び（selectOriginalGlbClips） ----
    let glb_count = g.animations.len();
    let candidates: Vec<&Arc<Clip>> = o.raw_clips.iter().enumerate().filter(|(i, c)| {
        let from_glb = *i < glb_count;
        if armed || game { !(from_glb && ["sword_idle", "sword_attack", "hit"].contains(&c.name.as_str())) }
        else { if from_glb { c.name != "hit" } else { c.name == "hit" || ACTIONS.contains(&c.name.as_str()) } }
    }).map(|(_, c)| c).collect();
    let families: Vec<&str> = (if game { vec!["idle", "walk", "run", "sword_idle", "sword_attack"] } else if armed { vec!["sword_idle", "sword_attack"] } else { vec!["idle", "walk", "run"] }).into_iter().filter(|f| motions.iter().any(|m| m == f)).collect();
    // ゲーム用の動作（あれば全部。通常用・武器用のどちらにも入れる）
    let actions: Vec<&str> = ACTIONS.iter().copied().filter(|f| candidates.iter().any(|c| c.name == *f)).collect();
    let has = |name: &str, family: &str| name == family || name.ends_with(&format!(":{family}"));
    let missing = families.iter().chain(std::iter::once(&"hit")).any(|f| candidates.iter().filter(|c| has(&c.name, f)).count() != 1);
    if families.is_empty() || missing { return Err(format!("{}GLBに必要な動作が揃っていません。", if game { "ゲーム用" } else if armed { "武器用" } else { "通常用" })); }
    let selected: Vec<Arc<Clip>> = families.iter().chain(actions.iter()).flat_map(|f| candidates.iter().filter(|c| has(&c.name, f)).map(|c| (*c).clone()).collect::<Vec<_>>()).collect();
    let hit = candidates.iter().find(|c| c.name == "hit").map(|c| (*c).clone()).ok_or("のけぞりの動作がありません。")?;
    let mut clip_names: Vec<String> = selected.iter().map(|c| c.name.clone()).collect();
    clip_names.push("hit".into());
    clip_names.push("face_hurt".into());

    // ---- 木 ----
    let mut w = GlbWriter::default();
    let tex_src = o.tex_src.clone();
    let tex = move |t: u32| -> Option<(glb::Image, bool)> { let (s, clamp) = tex_src.get(&t)?; let im = s.image(); Some((glb::Image { width: im.width, height: im.height, rgba: im.rgba.clone() }, *clamp)) };
    // ＞＜ の板は書き出しでは不透明度 1（prepareForExport）
    let plate_opacity = o.plates.as_ref().map(|p| (p.mat, kit.mat(p.mat).opacity));
    if let Some((m, _)) = plate_opacity { kit.mat_mut(m).opacity = 1.0; }
    let mut mat_cache: HashMap<u32, usize> = HashMap::new();
    let kit_ro: &Kit = kit;
    let mut mat = |w: &mut GlbWriter, id: MatId| -> usize { *mat_cache.entry(id.0).or_insert_with(|| w.material(kit_ro, id, &tex)) };
    let mut extras = v["extras"].clone();
    if extras.is_object() {
        if !extras["export"].is_object() { extras["export"] = json!({}); }
        extras["export"]["clips"] = json!(clip_names);
        if extras["export"]["sourceProvenance"].is_object() { extras["export"]["sourceProvenance"]["originalModular"] = g.json["extras"]["originalModular"].clone(); }
    }
    let root = w.node(None, "CharacterMakerOriginal", Vec3::ZERO, Quat::IDENTITY, Vec3::splat(o.size), Some(json!({ "characterMaker": extras })));
    let calibration = w.node(Some(root), "CharacterMakerOriginalCalibration", Vec3::new(0.0, o.offset_y, 0.0), Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2), Vec3::splat(o.model_scale), None);
    let scene_name = g.json["scenes"][0]["name"].as_str().unwrap_or("").to_string();
    let mut names = UniqueNames::default();
    let scene = w.node(Some(calibration), &if scene_name.is_empty() { String::new() } else { names.get(&scene_name) }, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, None);
    // 顔の向き（applyStaticHeadTurn: 模型の縦の軸のまわりに、頭の親の空間で）
    let mut rest = Skeleton::from_glb(&g);
    let base = Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2) * Mat4::from_scale(Vec3::splat(o.model_scale));
    rest.update_world(Mat4::from_scale(Vec3::splat(o.size)) * Mat4::from_translation(Vec3::new(0.0, o.offset_y, 0.0)) * base);
    let head_turn: Option<(usize, Quat)> = o.head.filter(|_| o.head_turn != 0.0).map(|h| {
        let axis = three_decompose(base).1 * Vec3::Y;
        let parent = rest.parent[h].map_or(Quat::IDENTITY, |p| three_decompose(rest.world[p]).1);
        let axis = (parent.inverse() * axis).normalize_or_zero();
        (h, Quat::from_axis_angle(axis, (o.head_turn as f32).to_radians()) * g.nodes[h].r)
    });
    let kept: std::collections::HashSet<usize> = o.parts.iter().filter(|p| p.kind == PartKind::Glb).map(|p| p.node).collect();
    let hand = o.skel.find(&o.hand_bone);
    let mut map: HashMap<usize, usize> = HashMap::new();
    // (書き出す節点, 部分の番号)
    let mut pending: Vec<(usize, usize)> = Vec::new();
    let mut hurt_node = None;
    #[allow(clippy::too_many_arguments)]
    fn walk(w: &mut GlbWriter, o: &Original, kit: &Kit, g: &Glb, i: usize, parent: usize, write: bool, names: &mut UniqueNames, kept: &std::collections::HashSet<usize>, head_turn: Option<(usize, Quat)>,
            map: &mut HashMap<usize, usize>, pending: &mut Vec<(usize, usize)>, armed: bool, hand: Option<usize>, hurt_node: &mut Option<usize>, mat: &mut dyn FnMut(&mut GlbWriter, MatId) -> usize, sword_extras: &Value) {
        let n = &g.nodes[i];
        // 外した髪も、読み込んだときに名前は取ってある（GLTFLoader の createUniqueName）
        let name = if n.name.is_empty() { String::new() } else { names.get(&n.name) };
        let write = write && !(n.mesh.is_some() && !kept.contains(&i));
        let prims = n.mesh.map_or(0, |m| g.meshes[m].primitives.len());
        let mut id = usize::MAX;
        if write {
            let r = head_turn.filter(|(h, _)| *h == i).map_or(n.r, |(_, q)| q);
            // three の userData（1 部分の形は mesh の extras のうえに name と節点の extras。GLTFExporter が extras に書く）
            let mut ud = serde_json::Map::new();
            if prims == 1 && let Some(mi) = n.mesh && let Some(e) = g.json["meshes"][mi]["extras"].as_object() { ud.extend(e.clone()); }
            if !n.name.is_empty() { ud.insert("name".into(), json!(n.name)); }
            if let Some(e) = n.extras.as_object() { ud.extend(e.clone()); }
            id = w.node(Some(parent), &name, n.t, r, n.s, Some(Value::Object(ud)));
            map.insert(i, id);
        }
        if let Some(mi) = n.mesh {
            for pi in 0..prims {
                let child = if prims > 1 { names.get(&g.meshes[mi].name) } else { String::new() };
                if !write { continue; }
                let target = if prims == 1 { id } else { w.node(Some(id), &child, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, g.json["meshes"][mi].get("extras").cloned()) };
                if let Some(k) = o.parts.iter().position(|p| p.kind == PartKind::Glb && p.node == i && p.prim == pi) { pending.push((target, k)); }
            }
        }
        for &c in &n.children { walk(w, o, kit, g, c, id, write, names, kept, head_turn, map, pending, armed, hand, hurt_node, mat, sword_extras); }
        if !write { return; }
        // 右手の骨の子: 握った手と武器（武器用だけ）
        if armed && Some(i) == hand {
            if let Some((ci, _)) = o.closed {
                let cn = w.node(Some(id), "OriginalSwordRightHand", Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, Some(json!({ "originalWeaponHand": "closed" })));
                pending.push((cn, ci));
            }
            let sid = sword_node(w, kit, o.weapon.root, id, &o.weapon, mat, Some((sword_extras.clone(), Vec3::ONE)));
            map.insert(o.weapon.skel_index, sid);
        }
        // 頭の骨の子: ＞＜ の節点と板 2 枚（scale 0。hit・face_hurt の scale のトラックで 1）
        if Some(i) == o.head {
            let hn = w.node(Some(id), crate::hurt_face::NODE, Vec3::ZERO, Quat::IDENTITY, Vec3::ZERO, None);
            *hurt_node = Some(hn);
            if let Some(p) = &o.plates {
                let mid = mat(w, p.mat);
                let geo = &p.geometry;
                let all = geo.indices.clone().unwrap_or_default();
                for (e, side) in ["left", "right"].iter().enumerate() {
                    let (lo, hi) = (e * p.per_eye, (e + 1) * p.per_eye);
                    let prim = Primitive {
                        positions: geo.positions[lo..hi].to_vec(), normals: Vec::new(), uvs: geo.uvs[lo..hi].to_vec(), colors: geo.colors[lo..hi].to_vec(),
                        indices: all.chunks_exact(3).filter(|t| (t[0] as usize) >= lo && (t[0] as usize) < hi).flat_map(|t| t.iter().map(|x| x - lo as u32)).collect(),
                        ..Default::default()
                    };
                    let pn = w.node(Some(hn), &format!("{}-eye-{side}", crate::hurt_face::NODE), Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, None);
                    w.mesh(pn, &MeshOut { name: String::new(), prim: &prim, positions: None, normals: None, material: Some(mid), target_names: Vec::new(), weights: Vec::new(), skin: None });
                }
            }
        }
    }
    let mut sword_extras = json!({ "attachmentBone": o.hand_bone, "makerWeapon": v["weaponId"].as_str().unwrap_or("none"), "visibilityTrack": format!("{}.scale", crate::weapon::SWORD) });
    if let (Some(a), Some(b)) = (sword_extras.as_object_mut(), v["swordExtras"].as_object()) { for (k, x) in b { a.insert(k.clone(), x.clone()); } }
    if let Some(len) = o.weapon.measured_length && sword_extras["makerWeaponFit"].is_object() { sword_extras["makerWeaponFit"]["measuredLengthAtDefaultSize"] = json!(len); }
    for &r in &g.scene { walk(&mut w, o, kit_ro, &g, r, scene, true, &mut names, &kept, head_turn, &mut map, &mut pending, armed, hand, &mut hurt_node, &mut mat, &sword_extras); }
    let _ = &o.closed_bone;
    // ---- 形（three の書き出しは骨付きの形ごとに skin を 1 つ） ----
    let mut eye_nodes: Vec<(usize, Vec<f32>, Vec<f32>)> = Vec::new();
    for (target, k) in pending {
        let p = &o.parts[k];
        let n = p.p.morph_pos.len();
        let names: Vec<String> = p.targets.iter().take(n).cloned().collect();
        let weights = if n > 0 { static_face(o, &names, o.init.get(k).map_or(&[][..], |v| v.as_slice())) } else { Vec::new() };
        let skin = p.skin.map(|s| { let sk = &g.skins[s]; let joints: Vec<usize> = sk.joints.iter().map(|j| map[j]).collect(); w.skin(&joints, &sk.inverse_bind) });
        let mut prim = if p.skin.is_some() { local_prim(&p.p, p.bind0) } else { p.p.clone() };
        if !p.has_uv { prim.uvs.clear(); }
        let material = Some(mat(&mut w, o.live[k].mat));
        // hurtEyeTracks: まばたきのモーフのある形は、＞＜ の間 目を閉じる（途中の補正は 0、Arc は 0、ほかの blink は 1）
        if names.iter().any(|t| t.starts_with("blink")) {
            let mut closed = weights.clone();
            for (i, t) in names.iter().enumerate() {
                if !t.starts_with("blink") { continue; }
                closed[i] = match blink_corrective(t, 1.0) { Some(c) => c as f32, None => if t.ends_with("Arc") { 0.0 } else { 1.0 } };
            }
            eye_nodes.push((target, weights.clone(), closed));
        }
        w.mesh(target, &MeshOut { name: p.name.clone(), prim: &prim, positions: None, normals: None, material, target_names: names, weights, skin });
        // three は _species_head のような独自の値をそのまま持ち、GLTFExporter が __SPECIES_HEAD として書く
        for (name, data, size) in &p.custom { w.extra_attribute(target, &format!("_{}", name.to_uppercase()), data, *size); }
    }
    // ---- 動作 ----
    let sword = o.weapon.skel_index;
    let tracks = |c: &Clip| -> Vec<TrackOut> {
        let mut c2 = c.clone();
        c2.tracks.retain(|t| t.node != sword);
        clip_tracks(&c2, &map)
    };
    for c in &selected { let t = tracks(c); w.animation(&c.name, &t); }
    let d = hit.duration;
    let mut t = tracks(&hit);
    if let Some(hn) = hurt_node { t.push(TrackOut { node: hn, path: "scale", step: true, times: vec![0.0, d], values: vec![1.0, 1.0, 1.0, 0.0, 0.0, 0.0], size: 3 }); }
    for (node, open, closed) in &eye_nodes { t.push(TrackOut { node: *node, path: "weights", step: true, times: vec![0.0, d], values: [closed.clone(), open.clone()].concat(), size: 1 }); }
    w.animation("hit", &t);
    let fd = HURT_FACE_CLIP_DURATION;
    let mut f = Vec::new();
    if let Some(hn) = hurt_node { f.push(TrackOut { node: hn, path: "scale", step: true, times: vec![0.0, fd], values: vec![1.0; 6], size: 3 }); }
    for (node, _, closed) in &eye_nodes { f.push(TrackOut { node: *node, path: "weights", step: true, times: vec![0.0, fd], values: [closed.clone(), closed.clone()].concat(), size: 1 }); }
    w.animation("face_hurt", &f);
    drop(mat);
    let out = w.finish(root);
    if let Some((m, op)) = plate_opacity { kit.mat_mut(m).opacity = op; }
    Ok(out)
}
