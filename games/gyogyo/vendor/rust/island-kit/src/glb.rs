// GLB の模型を場面へ読む（three の GLTFLoader の結果と同じ木にする）。読み解くのは character_actor::glb（meshopt・webp 込み）。
use crate::geometry::{Geometry, merge_geometries};
use crate::material::{MatId, Material, Side, Standard};
use crate::math::Color;
use crate::render::Kit;
use crate::scene::{NodeId, Rot, SkeletonId};
use character_actor::glb::{self, Glb, Primitive};
use std::collections::HashMap;

/// 1 つの部分の形（頂点色・揺れの量も）
pub fn primitive_geometry(p: &Primitive) -> Geometry {
    Geometry {
        positions: p.positions.clone(), normals: p.normals.clone(), uvs: p.uvs.clone(), colors: p.colors.clone(), sway: p.sway.clone(),
        indices: if p.indices.is_empty() { None } else { Some(p.indices.clone()) }, groups: Vec::new(),
    }
}

/// three の PropertyBinding.sanitizeNodeName
fn sanitize(name: &str) -> String {
    name.chars().filter(|c| !matches!(c, '[' | ']' | '.' | ':' | '/')).map(|c| if c.is_whitespace() { '_' } else { c }).collect()
}

/// GLTFLoader の normalizeSkinWeights（足して 1 に直す。全部 0 なら最初の骨へ）。三の Vector4 は倍精度で、Float32Array へ戻す
pub fn normalize_skin_weights(w: &[[f32; 4]]) -> Vec<[f32; 4]> {
    w.iter().map(|v| {
        let (a, b, c, d) = (v[0] as f64, v[1] as f64, v[2] as f64, v[3] as f64);
        let sum = a.abs() + b.abs() + c.abs() + d.abs();
        if sum == 0.0 { return [1.0, 0.0, 0.0, 0.0]; }
        let k = 1.0 / sum;
        [(a * k) as f32, (b * k) as f32, (c * k) as f32, (d * k) as f32]
    }).collect()
}

/// 骨付きの GLB を置いた結果（three の SkeletonUtils.clone(gltf.scene) の 1 体）
#[derive(Clone, Debug)]
pub struct SkinnedModel {
    /// 入れ物（three の clone した scene）
    pub root: NodeId,
    /// GLB の節点の番号 → 場面の節点。動きの値（character_actor::anim::Skeleton の t・r・s は同じ番号）を毎コマここへ写す
    pub nodes: Vec<Option<NodeId>>,
    /// 骨付きの部分（scene.traverse と同じ順）
    pub meshes: Vec<NodeId>,
    /// GLB の skin の番号 → 骨の組
    pub skeletons: Vec<SkeletonId>,
}
/// add_node の骨付きの分
struct SkinBuild { nodes: Vec<Option<NodeId>>, meshes: Vec<NodeId>, skeletons: Vec<SkeletonId> }

/// 骨付きの GLB を 1 体置く（three の SkeletonUtils.clone の代わり。形とテクスチャは glb ごとに 1 回だけ上げて共有し、節点と骨は 1 体ごと）。
/// 骨の無い部分は普通の Mesh。材質の選び方は instantiate と同じ（骨付きの部分に Custom を使うなら vs で k_skin_matrix を読む。README）
pub fn instantiate_skinned(kit: &mut Kit, parent: Option<NodeId>, g: &Glb, mats: &mut dyn FnMut(&mut Kit, Option<&glb::Material>) -> MatId) -> SkinnedModel {
    let root = kit.scene.group(parent);
    let skeletons: Vec<SkeletonId> = g.skins.iter().map(|_| kit.scene.new_skeleton(Vec::new(), Vec::new())).collect();
    let mut b = SkinBuild { nodes: vec![None; g.nodes.len()], meshes: Vec::new(), skeletons };
    let mut used: HashMap<String, u32> = HashMap::new();
    for &n in &g.scene { add_node_with(kit, root, g, n, mats, &mut used, Some(&mut b)); }
    // シーンの外の骨は入れ物で代わりにする（番号をずらさない）
    for (i, s) in g.skins.iter().enumerate() {
        let bones = s.joints.iter().map(|j| b.nodes[*j].unwrap_or(root)).collect();
        kit.scene.set_skeleton(b.skeletons[i], bones, s.inverse_bind.clone());
    }
    SkinnedModel { root, nodes: b.nodes, meshes: b.meshes, skeletons: b.skeletons }
}

/// 節点の木を場面へ置く。返すのは入れ物（three の gltf.scene）。mats は glb の材質から場面の材質を選ぶ（None は材質なし）。
/// 形は glb ごとに 1 回だけ GPU へ上げる。
pub fn instantiate(kit: &mut Kit, parent: Option<NodeId>, g: &Glb, mats: &mut dyn FnMut(&mut Kit, Option<&glb::Material>) -> MatId) -> NodeId {
    let root = kit.scene.group(parent);
    let mut used: HashMap<String, u32> = HashMap::new();
    let tops = g.scene.clone();
    for n in tops { add_node(kit, root, g, n, mats, &mut used); }
    root
}

/// 1 つの節点から下だけを置く（three の `node.clone(true)` を親へ add。pengo の小物の v0・v1 の選び方）。位置は節点のまま。名前は g.nodes[i].name で探す
pub fn instantiate_node(kit: &mut Kit, parent: Option<NodeId>, g: &Glb, node: usize, mats: &mut dyn FnMut(&mut Kit, Option<&glb::Material>) -> MatId) -> NodeId {
    let root = kit.scene.group(parent);
    let mut used: HashMap<String, u32> = HashMap::new();
    add_node(kit, root, g, node, mats, &mut used)
}

/// GLTFLoader の assignFinalMaterial のうち頂点色の判定: COLOR_0 のある部分は vertexColors を入れた写しを使う。
/// glb::standard_material から来た材質（写しも）だけ。ゲームが自分で作った材質はそのまま（three でゲームが材質を替えたときと同じ）
pub fn wants_vertex_colors(m: &Material, p: &Primitive) -> bool { m.gltf_loader && !m.vertex_colors && !p.colors.is_empty() }

/// 部分に使う材質（wants_vertex_colors なら vertexColors の写し。同じ材質の写しは 1 つ）。instantiate は自分で呼ぶ。部分を自分で置くゲームは、standard_material のあとにこれを通す
pub fn primitive_material(kit: &mut Kit, mat: MatId, p: &Primitive) -> MatId {
    if !wants_vertex_colors(kit.mat(mat), p) { return mat; }
    if let Some(id) = kit.glb_vertex_colored.get(&mat) { return *id; }
    let mut out = kit.mat(mat).clone();
    out.vertex_colors = true;
    let id = kit.material(out);
    kit.glb_vertex_colored.insert(mat, id);
    id
}

/// Mesh か、EXT_mesh_gpu_instancing なら InstancedMesh（GLTFLoader と同じ）
fn place(kit: &mut Kit, parent: NodeId, geo: crate::material::GeoId, mat: MatId, instances: &[glam::Mat4]) -> NodeId {
    if instances.is_empty() { return kit.scene.mesh(Some(parent), geo, mat); }
    let n = kit.scene.instanced(Some(parent), geo, mat, instances.len());
    for (i, m) in instances.iter().enumerate() { kit.scene.set_instance(n, i, *m); }
    n
}

fn add_node(kit: &mut Kit, parent: NodeId, g: &Glb, i: usize, mats: &mut dyn FnMut(&mut Kit, Option<&glb::Material>) -> MatId, used: &mut HashMap<String, u32>) -> NodeId {
    add_node_with(kit, parent, g, i, mats, used, None)
}
fn add_node_with(kit: &mut Kit, parent: NodeId, g: &Glb, i: usize, mats: &mut dyn FnMut(&mut Kit, Option<&glb::Material>) -> MatId, used: &mut HashMap<String, u32>, mut skin: Option<&mut SkinBuild>) -> NodeId {
    let node = &g.nodes[i];
    let prims = node.mesh.map(|m| g.meshes[m].primitives.len()).unwrap_or(0);
    let glb_key = g as *const Glb as usize;
    let geo_of = |kit: &mut Kit, mi: usize, pi: usize| {
        if let Some(id) = kit.glb_geo.get(&(glb_key, mi, pi)) { return *id; }
        let id = kit.geometry(&primitive_geometry(&g.meshes[mi].primitives[pi]));
        kit.glb_geo.insert((glb_key, mi, pi), id);
        id
    };
    // 骨付きの部分（instantiate_skinned のときだけ。instantiate は今までどおり骨を見ない）
    let skeleton = skin.as_ref().and_then(|b| node.skin.map(|k| b.skeletons[k]));
    let place_skinned = |kit: &mut Kit, parent: NodeId, mi: usize, pi: usize, sk: SkeletonId, skin: &mut SkinBuild, mats: &mut dyn FnMut(&mut Kit, Option<&glb::Material>) -> MatId| -> NodeId {
        let p = &g.meshes[mi].primitives[pi];
        let geo = geo_of(kit, mi, pi);
        if !kit.has_geometry_skin(geo) { kit.set_geometry_skin(geo, &p.joints, &normalize_skin_weights(&p.weights)); }
        // モーフ（まばたきなど）。重みは kit.set_morph_weights（既定 0 で今までと同じ形）
        if !p.morph_pos.is_empty() && !kit.has_geometry_morphs(geo) { kit.set_geometry_morphs(geo, &primitive_geometry(p), &p.morph_pos, &p.morph_nor); }
        let mat = mats(kit, p.material.map(|k| &g.materials[k]));
        let mat = primitive_material(kit, mat, p);
        let id = kit.scene.skinned(Some(parent), geo, mat, sk);
        skin.meshes.push(id);
        id
    };
    // 1 部分なら節点そのものが Mesh、2 部分以上なら Group の下に Mesh（GLTFLoader と同じ）
    let id = if let (Some(sk), Some(b)) = (skeleton, skin.as_deref_mut()) && prims > 0 {
        let mi = node.mesh.unwrap();
        if prims == 1 { place_skinned(kit, parent, mi, 0, sk, b, mats) } else {
            let grp = kit.scene.group(Some(parent));
            for pi in 0..prims { place_skinned(kit, grp, mi, pi, sk, b, mats); }
            grp
        }
    } else if prims == 1 {
        let mi = node.mesh.unwrap();
        let m = g.meshes[mi].primitives[0].material.map(|k| &g.materials[k]);
        let geo = geo_of(kit, mi, 0);
        let mat = mats(kit, m);
        let mat = primitive_material(kit, mat, &g.meshes[mi].primitives[0]);
        place(kit, parent, geo, mat, &node.instances)
    } else if prims > 1 {
        let mi = node.mesh.unwrap();
        let grp = kit.scene.group(Some(parent));
        for pi in 0..prims {
            let m = g.meshes[mi].primitives[pi].material.map(|k| &g.materials[k]);
            let geo = geo_of(kit, mi, pi);
            let mat = mats(kit, m);
            let mat = primitive_material(kit, mat, &g.meshes[mi].primitives[pi]);
            place(kit, grp, geo, mat, &node.instances);
        }
        grp
    } else {
        kit.scene.object(Some(parent))
    };
    let mut name = sanitize(&node.name);
    if !name.is_empty() {
        let n = used.entry(name.clone()).or_insert(0);
        if *n > 0 { name = format!("{name}_{n}"); }
        *n += 1;
    }
    {
        let o = &mut kit.scene[id];
        o.name = name;
        o.position = node.t;
        o.rotation = Rot::Quat(node.r);
        o.scale = node.s;
    }
    if let Some(b) = skin.as_deref_mut() { b.nodes[i] = Some(id); }
    for &c in &node.children { add_node_with(kit, id, g, c, mats, used, skin.as_deref_mut()); }
    id
}

/// matrixWorld を掛けて 1 つの形にする（pengo の敵とたまごの型）。keep で節点を選ぶ
pub fn merged_geometry(g: &Glb, keep: &dyn Fn(&glb::Node) -> bool) -> Option<Geometry> {
    let world = g.world_matrices();
    let mut parts: Vec<Geometry> = Vec::new();
    for (i, node) in g.nodes.iter().enumerate() {
        let Some(mi) = node.mesh else { continue };
        if !keep(node) { continue; }
        for p in &g.meshes[mi].primitives {
            let mut geo = primitive_geometry(p);
            geo.apply_matrix4(world[i]);
            parts.push(geo);
        }
    }
    merge_geometries(&parts.iter().collect::<Vec<_>>(), false)
}

/// GLTFLoader の MeshStandardMaterial と同じ値の材質（ゲームが自分の材質を使わない模型に）
pub fn standard_material(kit: &mut Kit, g: &Glb, m: Option<&glb::Material>) -> MatId {
    let Some(m) = m else { let mut out = Material::standard(); out.gltf_loader = true; return kit.material(out) };
    let [r, gg, b, a] = m.base_color;
    let mut out = Material::new(crate::material::MatKind::Standard(Standard {
        emissive: Color::rgb(m.emissive[0], m.emissive[1], m.emissive[2]), emissive_intensity: 1.0, metalness: m.metallic, roughness: m.roughness, flat_shading: false,
    }));
    out.gltf_loader = true;
    out.color = Color::rgb(r, gg, b);
    out.opacity = a;
    out.map = m.texture.map(|t| kit.glb_texture(g, t));
    if m.double_sided { out.side = Side::Double; }
    if m.alpha_blend { out.transparent = true; out.depth_write = false; }
    if let Some(c) = m.alpha_cutoff { out.alpha_test = c; }
    kit.material(out)
}

#[cfg(target_arch = "wasm32")]
pub use web::load_glb;

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use std::cell::RefCell;
    use std::sync::Arc;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;

    thread_local! { static CACHE: RefCell<HashMap<String, Arc<Glb>>> = RefCell::new(HashMap::new()); }

    fn js_err(e: JsValue) -> String { e.as_string().unwrap_or_else(|| format!("{e:?}")) }

    /// GLB を読む（同じ URL は 1 回だけ。three の loadGLB）
    pub async fn load_glb(url: &str) -> Result<Arc<Glb>, String> {
        if let Some(g) = CACHE.with(|c| c.borrow().get(url).cloned()) { return Ok(g); }
        let win = web_sys::window().ok_or("window が無い")?;
        let resp: web_sys::Response = JsFuture::from(win.fetch_with_str(url)).await.map_err(js_err)?.dyn_into().map_err(js_err)?;
        if !resp.ok() { return Err(format!("{url}: HTTP {}", resp.status())); }
        let ab = JsFuture::from(resp.array_buffer().map_err(js_err)?).await.map_err(js_err)?;
        let bytes = js_sys::Uint8Array::new(&ab).to_vec();
        let g = Arc::new(glb::parse(&bytes).map_err(|e| format!("{url}: {e}"))?);
        CACHE.with(|c| c.borrow_mut().insert(url.to_string(), g.clone()));
        Ok(g)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn part(colors: usize) -> Primitive { Primitive { colors: vec![[1.0, 0.0, 0.0, 1.0]; colors], ..Default::default() } }

    /// 防ぐ故障: 足して 1 にならない重みの模型が縮む・重みが全部 0 の頂点が原点へ潰れる（GLTFLoader の normalizeSkinWeights）
    #[test]
    fn skin_weights_follow_three() {
        let out = normalize_skin_weights(&[[0.5, 0.5, 0.5, 0.5], [0.0; 4], [2.0, 0.0, 0.0, 0.0], [0.1, 0.2, 0.3, 0.4]]);
        assert_eq!(out[0], [0.25; 4]);
        assert_eq!(out[1], [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(out[2], [1.0, 0.0, 0.0, 0.0]);
        // three: 1 / (0.1 + 0.2 + 0.3 + 0.4)（倍精度）を掛けて Float32Array へ
        let k = 1.0 / (0.1f32 as f64 + 0.2f32 as f64 + 0.3f32 as f64 + 0.4f32 as f64);
        assert_eq!(out[3], [(0.1f32 as f64 * k) as f32, (0.2f32 as f64 * k) as f32, (0.3f32 as f64 * k) as f32, (0.4f32 as f64 * k) as f32]);
    }

    /// 防ぐ故障: COLOR_0 に色を持つ模型（きゃしーの茎）が白くなる／頂点色の無い模型やゲームが自分で作った材質の絵が変わる
    #[test]
    fn vertex_colors_follow_gltf_loader() {
        let mut loaded = Material::standard();
        loaded.gltf_loader = true;
        assert!(wants_vertex_colors(&loaded, &part(3)));
        assert!(!wants_vertex_colors(&loaded, &part(0)));
        assert!(!wants_vertex_colors(&Material::standard(), &part(3)));
        // 写して Physical にしても残る（Engimeow の world.rs・other_body.rs）
        assert!(wants_vertex_colors(&loaded.clone(), &part(3)));
        // すでに入れてあれば写さない（iceclimber の小屋）
        loaded.vertex_colors = true;
        assert!(!wants_vertex_colors(&loaded, &part(3)));
    }
}
