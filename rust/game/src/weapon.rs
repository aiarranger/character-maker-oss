// プリセットの武器（今の版は lib/character-maker/weapon.ts の createMakerSword・fitMakerWeaponScale・enlargeWeaponOutsideGrip と
// lib/character-maker/registered-weapon.ts の読み込み）。木の形・名前・値はそちらと同じ。
//   CharacterMakerSword（手の骨の子。scale は動作のトラックで 0/1）> MakerSwordGrip > MakerWeaponVisualScale > MakerWeaponFit
//   > MakerWeaponHandlePlacement > RegisteredWeapon_<id>（登録の grip）> GLB の scene（length × grip.scale）
use crate::maker::Assets;
use character_actor::anim::{Prop, Skeleton, Track, three_decompose};
use character_actor::glb::Interp;
use character_actor::glb::{self, Glb};
use glam::{Mat3, Mat4, Quat, Vec3};
use island_kit::glb::primitive_geometry;
use island_kit::{Color, Kit, Material, NodeId, Rot, Side};
use serde_json::Value;
use std::sync::Arc;

pub const SWORD: &str = "CharacterMakerSword";

/// 置いた武器
pub struct Weapon {
    /// 骨の木に足した CharacterMakerSword の番号
    pub skel_index: usize,
    pub root: NodeId,
    /// 登録された武器の中身（顔の表示では隠す。setRegisteredWeaponsVisible）
    pub content: Option<NodeId>,
    /// 書き出しの値（測った長さ）
    pub measured_length: Option<f32>,
    /// 中身の部分（書き出し: 節点・写した部分・材質の番号）
    pub parts: Vec<(NodeId, glb::Primitive, island_kit::MatId)>,
    /// 上げた画像（TexId の番号, GLB の画像の番号）
    pub tex: Vec<(u32, usize)>,
    /// 測り: 部分ごとに CharacterMakerSword から部分までの行列と、描く頂点
    pub measure: Vec<(Mat4, Vec<[f32; 3]>)>,
    pub glb: Option<Arc<Glb>>,
}

fn v3(v: &Value) -> Vec3 { Vec3::new(v[0].as_f64().unwrap_or(0.0) as f32, v[1].as_f64().unwrap_or(0.0) as f32, v[2].as_f64().unwrap_or(0.0) as f32) }

/// 骨の木に節点を 1 つ足す（動作の scale のトラックが当たるように）
pub fn add_skel_node(skel: &mut Skeleton, name: &str, parent: usize, scale: Vec3) -> usize {
    let i = skel.names.len();
    skel.names.push(name.to_string());
    skel.parent.push(Some(parent));
    skel.children.push(Vec::new());
    skel.children[parent].push(i);
    skel.order.push(i);
    skel.t.push(Vec3::ZERO);
    skel.r.push(Quat::IDENTITY);
    skel.s.push(scale);
    skel.morphs.push(Vec::new());
    skel.fixed.push(None);
    skel.world.push(Mat4::IDENTITY);
    i
}

/// 動作に武器の見せる・隠すトラックを足す（addWeaponTracks。構える・振る・のけぞりと、武器を持ったまま動くゲーム用の動作は武器があれば 1、ほかは 0）
pub fn weapon_track(node: usize, clip: &str, duration: f32, equipped: bool) -> Track {
    let v = if equipped && ["sword_idle", "sword_attack", "hit", "jump", "dodge", "skill", "down", "crouch", "spinKick"].contains(&clip) { 1.0 } else { 0.0 };
    Track { node, prop: Prop::S, interp: Interp::Linear, times: Arc::new(vec![0.0, duration]), values: Arc::new(vec![v; 6]), size: 3 }
}

/// 武器を手の骨に付ける。spec は lib/character-maker/weapon-assets.ts の presetWeaponSpec（武器なしは null）
pub fn attach(kit: &mut Kit, assets: &Assets, skel: &mut Skeleton, body_nodes: &[Option<NodeId>], spec: &Value, bone: &str) -> Result<Weapon, String> {
    let hand = skel.find(bone).ok_or("武器を持つ手の接続点が見つかりません。")?;
    let hand_node = body_nodes[hand].ok_or("武器を持つ手の接続点が見つかりません。")?;
    let skel_index = add_skel_node(skel, SWORD, hand, Vec3::ZERO);
    let root = kit.scene.group(Some(hand_node));
    kit.scene[root].name = SWORD.into();
    kit.scene[root].scale = Vec3::ZERO;
    // 手の向きの枠（刃の向き y・手のひら z）
    let socket = &spec["socket"];
    let grip = kit.scene.group(Some(root));
    {
        let y = v3(&socket["blade"]).normalize();
        let mut z = v3(&socket["palm"]);
        z = (z - y * z.dot(y)).normalize();
        let x = y.cross(z).normalize();
        let z = x.cross(y).normalize();
        let n = &mut kit.scene[grip];
        n.name = "MakerSwordGrip".into();
        n.position = v3(&socket["point"]);
        n.rotation = Rot::Quat(Quat::from_mat3(&Mat3::from_cols(x, y, z)));
        // fitMakerWeaponScale: 手の骨の世界の大きさを打ち消す（大きさの設定は効かせる）
        skel.update_world(Mat4::IDENTITY);
        let s = three_decompose(skel.world[hand]).0;
        n.scale = Vec3::new(1.0 / s.x, 1.0 / s.y, 1.0 / s.z);
    }
    if spec.is_null() { return Ok(Weapon { skel_index, root, content: None, measured_length: None, parts: Vec::new(), tex: Vec::new(), measure: Vec::new(), glb: None }); }
    let g = assets.glb(spec["asset"].as_str().unwrap_or(""))?;
    let fit = &spec["fit"];
    let (size_scale, visual_scale) = (fit["sizeScale"].as_f64().unwrap_or(1.0) as f32, fit["visualScale"].as_f64().unwrap_or(1.0) as f32);
    let (radius, transition) = (fit["radius"].as_f64().unwrap_or(0.045), fit["transition"].as_f64().unwrap_or(0.085));
    let offset = v3(&fit["gripOffset"]);
    let visual = kit.scene.group(Some(grip)); kit.scene[visual].name = "MakerWeaponVisualScale".into(); kit.scene[visual].scale = Vec3::splat(visual_scale);
    let size = kit.scene.group(Some(visual)); kit.scene[size].name = "MakerWeaponFit".into(); kit.scene[size].scale = Vec3::splat(size_scale);
    let place = kit.scene.group(Some(size)); kit.scene[place].name = "MakerWeaponHandlePlacement".into(); kit.scene[place].position = offset;
    let id = spec["id"].as_str().unwrap_or("");
    let gs = &spec["grip"];
    let content = kit.scene.group(Some(place));
    let adjust = Mat4::from_rotation_translation(Quat::from_euler(glam::EulerRot::XYZ, gs["rot"][0].as_f64().unwrap_or(0.0) as f32, gs["rot"][1].as_f64().unwrap_or(0.0) as f32, gs["rot"][2].as_f64().unwrap_or(0.0) as f32), v3(&gs["pos"]));
    {
        let n = &mut kit.scene[content];
        n.name = format!("RegisteredWeapon_{id}");
        n.position = v3(&gs["pos"]);
        n.rotation = Rot::Quat(Quat::from_euler(glam::EulerRot::XYZ, gs["rot"][0].as_f64().unwrap_or(0.0) as f32, gs["rot"][1].as_f64().unwrap_or(0.0) as f32, gs["rot"][2].as_f64().unwrap_or(0.0) as f32));
    }
    let scene_scale = spec["length"].as_f64().unwrap_or(1.0) as f32 * gs["scale"].as_f64().unwrap_or(1.0) as f32;
    let scene = kit.scene.group(Some(content));
    kit.scene[scene].scale = Vec3::splat(scene_scale);
    let to_place = Mat4::from_translation(offset);
    let comp = |y: f64| -> f64 {
        let t = ((y.abs() - radius) / transition).clamp(0.0, 1.0);
        let s = t * t * (3.0 - 2.0 * t);
        (1.0 - s) / visual_scale as f64 + s / size_scale as f64
    };
    let (mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut parts = Vec::new();
    let mut tex: Vec<(u32, usize)> = Vec::new();
    fn walk(kit: &mut Kit, g: &Glb, i: usize, parent: NodeId, parent_m: Mat4, out: &mut Vec<(usize, NodeId, Mat4)>) {
        let node = &g.nodes[i];
        let id = kit.scene.group(Some(parent));
        { let n = &mut kit.scene[id]; n.name = node.name.clone(); n.position = node.t; n.rotation = Rot::Quat(node.r); n.scale = node.s; }
        let m = parent_m * node.local();
        if node.mesh.is_some() { out.push((i, id, m)); }
        for &c in &node.children { walk(kit, g, c, id, m, out); }
    }
    let mut meshes = Vec::new();
    for &n in &g.scene { walk(kit, &g, n, scene, adjust * Mat4::from_scale(Vec3::splat(scene_scale)), &mut meshes); }
    for (i, holder, m) in meshes {
        let to_grip = to_place * m;
        let from_grip = to_grip.inverse();
        for p in &g.meshes[g.nodes[i].mesh.unwrap()].primitives {
            let mut q = p.clone();
            for v in q.positions.iter_mut() {
                let a = to_grip.as_dmat4().transform_point3(glam::DVec3::new(v[0] as f64, v[1] as f64, v[2] as f64));
                let k = comp(a.y * size_scale as f64);
                let b = a * k;
                min_y = min_y.min(b.y); max_y = max_y.max(b.y);
                let back = from_grip.as_dmat4().transform_point3(b);
                *v = [back.x as f32, back.y as f32, back.z as f32];
            }
            q.normals.clear();
            glb::compute_normals(&mut q);
            let mut mat = Material::basic();
            if let Some(mi) = q.material {
                let m = &g.materials[mi];
                mat.map = m.texture.map(|t| kit.glb_texture(&g, t));
                if let (Some(t), Some(i)) = (mat.map, m.texture) && !tex.iter().any(|x: &(u32, usize)| x.0 == t.0) { tex.push((t.0, i)); }
                mat.color = Color { r: m.base_color[0], g: m.base_color[1], b: m.base_color[2] };
                mat.opacity = m.base_color[3];
                mat.transparent = m.alpha_blend;
                mat.depth_write = !m.alpha_blend;
                if let Some(c) = m.alpha_cutoff { mat.alpha_test = c; }
                if m.double_sided { mat.side = Side::Double; }
            }
            mat.vertex_colors = !q.colors.is_empty();
            mat.tone_mapped = false;
            let mid = kit.material(mat);
            let geo = kit.geometry(&primitive_geometry(&q));
            let node = kit.scene.mesh(Some(holder), geo, mid);
            kit.scene[node].frustum_culled = false;
            parts.push((node, q, mid));
        }
    }
    // CharacterMakerSword から部分までの行列（剣の scale は動作が決めるので入れない）
    let measure = parts.iter().map(|(node, q, _)| {
        let mut m = Mat4::IDENTITY;
        let mut n = Some(*node);
        while let Some(id) = n { if id == root { break; } m = kit.scene[id].local() * m; n = kit.scene[id].parent(); }
        let used: std::collections::HashSet<u32> = q.indices.iter().copied().collect();
        (m, q.positions.iter().enumerate().filter(|(i, _)| used.is_empty() || used.contains(&(*i as u32))).map(|(_, v)| *v).collect())
    }).collect();
    let measured = if max_y.is_finite() { Some(((max_y - min_y) * size_scale as f64 * visual_scale as f64) as f32) } else { None };
    Ok(Weapon { skel_index, root, content: Some(content), measured_length: measured, parts, tex, measure, glb: Some(g) })
}
