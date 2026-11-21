// GLB の読み込みと、aisns の小物の取り出し。読み解くのは island_kit::glb（meshopt・webp 込み）。src/view/assets.ts の写し。
use super::materials::{SoftOptions, soft_material};
use character_actor::glb::Glb;
use island_kit::geometry::Geometry;
use island_kit::glb::primitive_geometry;
use island_kit::scene::Rot;
use island_kit::{Color, GeoId, Kit, MatId, NodeId};
use std::collections::HashMap;
use std::sync::Arc;

pub const BASE: &str = "./aisns/";

pub use island_kit::glb::load_glb;

/// 小物の GLB を読む（propObject の前半。await の間 kit を借りない）
pub async fn load_prop(name: &str) -> Result<Arc<Glb>, String> { load_glb(&format!("{BASE}models/wilds/props/{name}.glb")).await }

/// assets.ts の preload（全部読み終わるまで待つ。読んだ数を知らせる）
pub async fn preload(urls: &[String], mut on_progress: Option<&mut dyn FnMut(usize, usize)>) -> Result<(), String> {
    let waits = js_sys::Array::new();
    for u in urls {
        let u = u.clone();
        waits.push(&wasm_bindgen_futures::future_to_promise(async move {
            load_glb(&u).await.map(|_| wasm_bindgen::JsValue::UNDEFINED).map_err(|e| wasm_bindgen::JsValue::from_str(&e))
        }));
    }
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::all(&waits)).await.map_err(|e| format!("{e:?}"))?;
    if let Some(f) = on_progress.as_mut() { f(urls.len(), urls.len()); }
    Ok(())
}

const SWAYING: [&str; 14] = ["tree_round_small", "tree_round_large", "tree_conifer", "tree_hero", "tree_twisted", "big_tree", "bush_round",
    "flower_white", "flower_yellow", "flower_lavender", "pampas_grass", "reed_clump", "clover_patch", "hedge"];

/// 小物の材質と、上げ終わった形（assets.ts のモジュールの値。1 つ作って持ち回る）
pub struct Props {
    prop: MatId,
    // 灯籠は石の色を温かい生成りへ寄せる（お菓子の島の彩度にそろえる）
    lantern: MatId,
    sway: MatId,
    geos: HashMap<(usize, usize, usize), GeoId>,
}

fn is_variant(name: &str) -> bool { name.len() > 1 && name.starts_with('v') && name[1..].bytes().all(|b| b.is_ascii_digit()) }

impl Props {
    pub fn new(kit: &mut Kit) -> Props {
        Props {
            prop: kit.material(soft_material(SoftOptions { vertex_colors: true, ..Default::default() })),
            lantern: kit.material(soft_material(SoftOptions { vertex_colors: true, color: Color::css("#ffe6cc"), rim: 1.2, ..Default::default() })),
            sway: kit.material(soft_material(SoftOptions { vertex_colors: true, sway: true, ..Default::default() })),
            geos: HashMap::new(),
        }
    }

    /// 小物を 1 つ取り出す（aisns の props は v0 / v1 … が型違い、body / glow / rotor が部品）。返す Group の子は get_by_name で引ける（glow・rotor）
    pub fn prop_object(&mut self, kit: &mut Kit, parent: Option<NodeId>, g: &Arc<Glb>, name: &str, variant: usize) -> NodeId {
        let out = kit.scene.group(parent);
        kit.scene[out].name = name.to_string();
        let v_name = format!("v{variant}");
        let mut picked: Vec<usize> = g.scene.iter().copied().filter(|&n| g.nodes[n].name == v_name).collect();
        if picked.is_empty() { picked = g.scene.iter().copied().filter(|&n| is_variant(&g.nodes[n].name)).take(1).collect(); }
        if picked.is_empty() { picked = g.scene.iter().copied().filter(|&n| !g.nodes[n].name.ends_with("_lod1")).collect(); }
        let sway = SWAYING.contains(&name);
        let lantern = name == "lantern_stone" || name == "lamp_post";
        for n in picked {
            let c = self.add_node(kit, out, g, n, sway, lantern);
            kit.scene[c].position = glam::Vec3::ZERO;
        }
        out
    }

    fn add_node(&mut self, kit: &mut Kit, parent: NodeId, g: &Arc<Glb>, i: usize, sway: bool, lantern: bool) -> NodeId {
        let node = &g.nodes[i];
        let prims = node.mesh.map(|m| g.meshes[m].primitives.len()).unwrap_or(0);
        let mut mesh_of = |kit: &mut Kit, parent: NodeId, mi: usize, pi: usize| {
            let p = &g.meshes[mi].primitives[pi];
            let key = (Arc::as_ptr(g) as usize, mi, pi);
            let geo = *self.geos.entry(key).or_insert_with(|| { let pg: Geometry = primitive_geometry(p); kit.geometry(&pg) });
            let mat = if sway && !p.sway.is_empty() { self.sway } else if lantern { self.lantern } else { self.prop };
            let m = kit.scene.mesh(Some(parent), geo, mat);
            kit.scene[m].cast_shadow = true;
            kit.scene[m].receive_shadow = true;
            m
        };
        // 1 部分なら節点そのものが Mesh、2 部分以上なら Group の下に Mesh（GLTFLoader と同じ）
        let id = if prims == 1 {
            mesh_of(kit, parent, node.mesh.unwrap(), 0)
        } else if prims > 1 {
            let grp = kit.scene.group(Some(parent));
            for pi in 0..prims { mesh_of(kit, grp, node.mesh.unwrap(), pi); }
            grp
        } else {
            kit.scene.object(Some(parent))
        };
        {
            let o = &mut kit.scene[id];
            o.name = node.name.clone();
            o.position = node.t;
            o.rotation = Rot::Quat(node.r);
            o.scale = node.s;
        }
        for &c in &node.children { self.add_node(kit, id, g, c, sway, lantern); }
        id
    }
}
