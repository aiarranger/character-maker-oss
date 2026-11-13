// キャラメイクのプリセット（ファンタジー・アメコミ）の模型: GLB から組み立て、設定を当て、動かす。今の版（three）は
// lib/character-maker/assembly.ts と components/character-maker/CharacterMakerScene.tsx の Model。式と順はそちらに合わせる。
// 計画と段は docs/rust-stage-plan.md。
use crate::actor::Actor;
use crate::expressions::{self, Built, Expressions, PrimMorph, PrimNames};
use crate::head;
use crate::hurt_face::HurtFace;
use crate::weapon::{self, Weapon};
use character_actor::anim::{ActionId, Clip, Mixer, NamedClip, Prop, Skeleton};
use character_actor::glb::{self, Glb};
use glam::{EulerRot, Mat4, Quat, Vec3};
use island_kit::geometry::Geometry;
use island_kit::glb::{SkinnedModel, instantiate_skinned, normalize_skin_weights, primitive_geometry};
use island_kit::texture::TexOpts;
use island_kit::{Color, Kit, MatId, Material, NodeId, Rot, Side, TexId};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

/// 届いた素材（TS が今と同じ URL から fetch して名前で渡す）
pub enum Asset { Glb(Arc<Glb>), Clips(Vec<NamedClip>), Image(Arc<glb::Image>) }

/// 体の GLB の写し（大人の顔の形・表情の補正のモーフを足した物）。画像は持たない（元の GLB から上げる）。
/// kit は形を GLB の場所で控えるので、作った写しは捨てずに持ち続ける
pub struct Variant { pub glb: Arc<Glb>, pub names: PrimNames, pub built: Option<Built> }

/// 読んだ素材を名前で持つ（同じ名前は 1 回だけ読む。今の版の templates・motions の控えと同じ）
#[derive(Default)]
pub struct Assets { map: HashMap<String, Asset>, variants: HashMap<String, Arc<Variant>> }
impl Assets {
    pub fn has(&self, name: &str) -> bool { self.map.contains_key(name) }
    pub fn put(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let asset = if name.ends_with(".glb") {
            Asset::Glb(Arc::new(glb::parse(bytes)?))
        } else if name.ends_with(".json") {
            let v: Value = serde_json::from_slice(bytes).map_err(|e| format!("{name}: {e}"))?;
            // 今の版: (Array.isArray(data) ? data : data.clips).map(AnimationClip.parse)
            let list = if v.is_array() { v.as_array() } else { v.get("clips").and_then(Value::as_array) };
            let clips = list.ok_or(format!("{name}: 動作の並びがありません"))?.iter().filter_map(NamedClip::from_json).collect();
            Asset::Clips(clips)
        } else if name.ends_with(".png") || name.ends_with(".jpg") {
            let im = image::load_from_memory(bytes).map_err(|e| format!("{name}: {e}"))?.to_rgba8();
            Asset::Image(Arc::new(glb::Image { width: im.width(), height: im.height(), rgba: im.into_raw() }))
        } else {
            return Err(format!("{name}: 知らない素材の種類です"));
        };
        self.map.insert(name.to_string(), asset);
        Ok(())
    }
    pub fn put_image(&mut self, name: &str, w: u32, h: u32, rgba: Vec<u8>) { self.map.insert(name.to_string(), Asset::Image(Arc::new(glb::Image { width: w, height: h, rgba }))); }
    pub fn glb(&self, name: &str) -> Result<Arc<Glb>, String> {
        match self.map.get(name) { Some(Asset::Glb(g)) => Ok(g.clone()), _ => Err(format!("{name} が届いていません")) }
    }
    pub fn clips(&self, name: &str) -> Result<&[NamedClip], String> {
        match self.map.get(name) { Some(Asset::Clips(c)) => Ok(c), _ => Err(format!("{name} が届いていません")) }
    }
    pub fn image_opt(&self, name: &str) -> Option<Arc<glb::Image>> { self.image(name).ok() }
    pub fn image(&self, name: &str) -> Result<Arc<glb::Image>, String> {
        match self.map.get(name) { Some(Asset::Image(i)) => Ok(i.clone()), _ => Err(format!("{name} が届いていません")) }
    }
    /// 体の写し（createFantasyHead・createMakerExpressions の形の側）
    fn variant(&mut self, name: &str, fantasy: bool, masculine: bool, mature: bool) -> Result<Arc<Variant>, String> {
        let key = format!("{name}|{fantasy}|{mature}");
        if let Some(v) = self.variants.get(&key) { return Ok(v.clone()); }
        let src = self.glb(name)?;
        let mut g = Glb { json: src.json.clone(), nodes: src.nodes.clone(), meshes: src.meshes.clone(), skins: src.skins.clone(), animations: src.animations.clone(), materials: src.materials.clone(), images: Vec::new(), scene: src.scene.clone() };
        let mut names: PrimNames = HashMap::new();
        for (mi, m) in g.meshes.iter().enumerate() { for pi in 0..m.primitives.len() { names.insert((mi, pi), m.target_names.clone()); } }
        if fantasy && mature {
            for mesh in g.meshes.iter_mut() {
                let body = mesh.target_names.iter().any(|n| n == "bodySlim");
                let face = mesh.target_names.iter().any(|n| n == "eyesRound");
                if !body && !face { continue; }
                // 同じ頂点を持つ部分（GLB の同じ accessor）は 1 回だけ写す（今の版の templateWarps の控え）。(元の部分, 写した部分)
                let mut done: Vec<(glb::Primitive, glb::Primitive)> = Vec::new();
                for p in mesh.primitives.iter_mut() {
                    let same = |a: &glb::Primitive, b: &glb::Primitive| a.positions == b.positions && a.normals == b.normals && a.morph_pos == b.morph_pos && a.morph_nor == b.morph_nor;
                    if let Some((_, w)) = done.iter().find(|(o, _)| same(o, p)) {
                        p.positions = w.positions.clone(); p.normals = w.normals.clone(); p.morph_pos = w.morph_pos.clone(); p.morph_nor = w.morph_nor.clone();
                        continue;
                    }
                    let original = p.clone();
                    head::warp(p, masculine, body);
                    done.push((original, p.clone()));
                }
            }
        }
        let built = if fantasy { Some(expressions::build(&mut g, &mut names, masculine)) } else { None };
        let v = Arc::new(Variant { glb: Arc::new(g), names, built });
        self.variants.insert(key, v.clone());
        Ok(v)
    }
}

/// 画像の元（書き出しで PNG にする。GLB の画像か、届いた・作った画像）
#[derive(Clone)]
pub enum TexSrc { Glb(Arc<Glb>, usize), Image(Arc<glb::Image>) }
impl TexSrc {
    pub fn image(&self) -> &glb::Image { match self { TexSrc::Glb(g, i) => &g.images[*i], TexSrc::Image(im) => im } }
}

/// 1 つの材質（three は 1 体ごとに clone した MeshBasicMaterial）
pub(crate) struct Mat { pub(crate) id: MatId, pub(crate) name: String }

/// 測りに使う、骨付きの部分
pub(crate) struct SkinPart { pub(crate) mesh: usize, pub(crate) prim: usize, pub(crate) skin: usize, pub(crate) node: usize }

pub struct Model {
    pub(crate) root: NodeId,
    pub(crate) variant: Arc<Variant>,
    pub(crate) body: SkinnedModel,
    pub skel: Skeleton,
    pub(crate) mixer: Mixer,
    pub(crate) actions: Vec<(String, ActionId)>,
    pub(crate) current: Option<(String, ActionId)>,
    pub(crate) mats: Vec<Mat>,
    pub(crate) prims: Vec<PrimMorph>,
    pub(crate) skin_parts: Vec<SkinPart>,
    /// 髪（頭の骨の下の節点、その GLB、部分ごとの (節点・モーフを当てた形の箱)）
    pub(crate) hair: Option<(NodeId, Arc<Glb>, Vec<(usize, Vec3, Vec3, Vec<[f32; 3]>)>)>,
    pub(crate) head: Option<usize>,
    pub(crate) head_rest_scale: Vec3,
    /// 視線の骨（ファンタジーの J_Adj_L/R_FaceEye）: 骨の番号・左右・元の回転・今の向き（expressions.ts の eyes）
    pub(crate) eyes: Vec<(usize, char, Quat, Quat)>,
    pub(crate) expressions: Option<Expressions>,
    pub(crate) fantasy: bool,
    pub(crate) masculine: bool,
    /// 当てた設定（＞＜ を切り替えるときに使う）
    pub(crate) applied: Value,
    pub(crate) hurt_on: bool,
    /// ＞＜ の切り替え（押している間）
    pub(crate) hurt_prop: bool,
    /// アメコミの ＞＜ の板
    pub(crate) hurt_face: Option<HurtFace>,
    pub(crate) weapon: Option<Weapon>,
    /// のけぞり（今の動作の上に重ねる一回再生）と、その時刻
    pub(crate) hit_layer: Option<HitLayer>,
    pub(crate) hit: Option<f64>,
    /// 振るが終わったことを知らせたか
    pub(crate) attack_done: bool,
    /// TS へ知らせる出来事（motionComplete・hitComplete）
    pub events: Vec<Value>,
    /// 画像の元（TexId → 元）
    pub(crate) tex_src: HashMap<u32, TexSrc>,
    /// GLB の材質 → 描く材質（(0 体・1 髪, 材質の番号)）
    pub(crate) mat_of: HashMap<(u8, usize), MatId>,
    pub(crate) height: f32,
    pub playing: bool,
    pub speed: f32,
}

/// '#RRGGBB' → three の Color.set（sRGB → 線形）
fn css_color(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 { return None; }
    u32::from_str_radix(h, 16).ok().map(Color::hex)
}

/// 今の版の ownedUnlitMaterial（GLTFLoader の KHR_materials_unlit の MeshBasicMaterial を写した、toneMapped なしの材質）。
/// BLEND は transparent で depthWrite なし、MASK は alphaTest（GLTFLoader と同じ）。画像は tex_glb（画像を持つ元の GLB）から
fn unlit(kit: &mut Kit, tex_glb: &Glb, m: Option<&glb::Material>) -> Material {
    let mut mat = Material::basic();
    if let Some(m) = m {
        mat.map = m.texture.map(|t| kit.glb_texture(tex_glb, t));
        mat.color = Color { r: m.base_color[0], g: m.base_color[1], b: m.base_color[2] };
        mat.opacity = m.base_color[3];
        mat.transparent = m.alpha_blend;
        mat.depth_write = !m.alpha_blend;
        if let Some(c) = m.alpha_cutoff { mat.alpha_test = c; }
        if m.double_sided { mat.side = Side::Double; }
    }
    mat.tone_mapped = false;
    mat
}

/// canvas の絵（three の CanvasTexture: 繰り返さない）
fn image_texture_clamp(kit: &mut Kit, im: &glb::Image) -> TexId { kit.data_texture(im.width, im.height, &im.rgba, true) }

fn image_texture(kit: &mut Kit, im: &glb::Image) -> TexId {
    let t = kit.data_texture(im.width, im.height, &im.rgba, true);
    kit.set_tex_opts(t, TexOpts::repeat());
    t
}

/// 骨の無い GLB（髪）を親の下に置く（three の clone(hair.scene)）。モーフの重み（名前 → 値）は形へ焼く（形の差分 × 重み。three の morphtarget_vertex と同じ足し方）。
/// 返り値: 入れ物と、部分ごとの (GLB の節点・形の箱)
fn place_rigid(kit: &mut Kit, parent: NodeId, g: &Glb, morph: &dyn Fn(&str) -> f32, mat: &mut dyn FnMut(&mut Kit, Option<&glb::Material>) -> MatId) -> (NodeId, Vec<(usize, Vec3, Vec3, Vec<[f32; 3]>)>) {
    let root = kit.scene.group(Some(parent));
    let mut boxes = Vec::new();
    fn add(kit: &mut Kit, parent: NodeId, g: &Glb, i: usize, morph: &dyn Fn(&str) -> f32, mat: &mut dyn FnMut(&mut Kit, Option<&glb::Material>) -> MatId, boxes: &mut Vec<(usize, Vec3, Vec3, Vec<[f32; 3]>)>) {
        let node = &g.nodes[i];
        let id = match node.mesh {
            Some(mi) => {
                let mesh = &g.meshes[mi];
                let parts: Vec<(NodeId, usize)> = mesh.primitives.iter().enumerate().map(|(pi, p)| {
                    let mut geo: Geometry = primitive_geometry(p);
                    for (k, name) in mesh.target_names.iter().enumerate() {
                        let w = morph(name);
                        if w == 0.0 { continue; }
                        if let Some(d) = p.morph_pos.get(k) { for (v, d) in geo.positions.iter_mut().zip(d) { for c in 0..3 { v[c] += d[c] * w; } } }
                        if let Some(d) = p.morph_nor.get(k) { for (v, d) in geo.normals.iter_mut().zip(d) { for c in 0..3 { v[c] += d[c] * w; } } }
                    }
                    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
                    for v in &geo.positions { lo = lo.min(Vec3::from(*v)); hi = hi.max(Vec3::from(*v)); }
                    let gid = kit.geometry(&geo);
                    let m = mat(kit, p.material.map(|k| &g.materials[k]));
                    (kit.scene.mesh(None, gid, m), pi)
                }).collect();
                if parts.len() == 1 {
                    let (n, _) = parts[0];
                    kit.scene.set_parent(n, Some(parent));
                    n
                } else {
                    let grp = kit.scene.group(Some(parent));
                    for (n, _) in &parts { kit.scene.set_parent(*n, Some(grp)); }
                    grp
                }
            }
            None => kit.scene.object(Some(parent)),
        };
        if let Some(mi) = node.mesh {
            // 形の箱（モーフを当てた位置）。部分ごと
            let mesh = &g.meshes[mi];
            for p in &mesh.primitives {
                // 箱は three の読んだ形の geometry.boundingBox（GLTFLoader の computeBounds）
                let (lo, hi) = crate::export::gltf_loader_box(&p.positions, &p.morph_pos);
                let mut drawn = Vec::new();
                let used: std::collections::HashSet<u32> = p.indices.iter().copied().collect();
                for (vi, v) in p.positions.iter().enumerate() {
                    let mut q = Vec3::from(*v);
                    for (k, name) in mesh.target_names.iter().enumerate() { let w = morph(name); if w != 0.0 && let Some(d) = p.morph_pos.get(k) { q += Vec3::from(d[vi]) * w; } }
                    if used.is_empty() || used.contains(&(vi as u32)) { drawn.push(q.to_array()); }
                }
                boxes.push((i, lo, hi, drawn));
            }
        }
        { let n = &mut kit.scene[id]; n.name = node.name.clone(); n.position = node.t; n.rotation = Rot::Quat(node.r); n.scale = node.s; }
        for &c in &node.children { add(kit, id, g, c, morph, mat, boxes); }
    }
    for &n in &g.scene { add(kit, root, g, n, morph, mat, &mut boxes); }
    (root, boxes)
}

/// のけぞりの重ね（CharacterMakerScene の hitLayer）。体の骨の回転と位置のトラックだけ
pub(crate) struct HitLayer { pub(crate) clip: Arc<Clip>, pub(crate) duration: f64, pub(crate) binds: Vec<(usize, Prop, usize, Option<[f32; 4]>)> }

/// のけぞりの重みの入り・戻り（秒）
const HIT_FADE_IN: f64 = 0.06;
const HIT_FADE_OUT: f64 = 0.2;
fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 { let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

/// three の Quaternion.slerp（this ← this と qb の間の t）
pub fn three_slerp(a: Quat, b: Quat, t: f32) -> Quat {
    if t == 0.0 { return a; }
    if t == 1.0 { return b; }
    let (x, y, z, w) = (a.x as f64, a.y as f64, a.z as f64, a.w as f64);
    let (mut bx, mut by, mut bz, mut bw) = (b.x as f64, b.y as f64, b.z as f64, b.w as f64);
    let mut cos = w * bw + x * bx + y * by + z * bz;
    if cos < 0.0 { bx = -bx; by = -by; bz = -bz; bw = -bw; cos = -cos; }
    if cos >= 1.0 { return a; }
    let t = t as f64;
    let sqr = 1.0 - cos * cos;
    if sqr <= f64::EPSILON {
        let s = 1.0 - t;
        let q = glam::DQuat::from_xyzw(s * x + t * bx, s * y + t * by, s * z + t * bz, s * w + t * bw).normalize();
        return Quat::from_xyzw(q.x as f32, q.y as f32, q.z as f32, q.w as f32);
    }
    let sin = sqr.sqrt();
    let half = sin.atan2(cos);
    let (ra, rb) = (((1.0 - t) * half).sin() / sin, (t * half).sin() / sin);
    Quat::from_xyzw((x * ra + bx * rb) as f32, (y * ra + by * rb) as f32, (z * ra + bz * rb) as f32, (w * ra + bw * rb) as f32)
}

impl Model {
    /// assembleCharacter。spec: { body, hair, motions: [名前…], outfit: { 材質の名前: 画像の名前 }, mature: { skin, eyelash, eyeline }, config }
    pub fn assemble(kit: &mut Kit, assets: &mut Assets, spec: &Value) -> Result<Model, String> {
        let c = &spec["config"];
        let fantasy = c["style"].as_str() == Some("fantasy");
        let masculine = c["base"].as_str() == Some("masculine");
        let mature = fantasy && c["face"].as_str() == Some("mature");
        let body_name = spec["body"].as_str().unwrap_or("");
        let src = assets.glb(body_name)?;
        let variant = assets.variant(body_name, fantasy, masculine, mature)?;
        let g = variant.glb.clone();
        let hair_glb = match spec["hair"].as_str() { Some(n) if !n.is_empty() => Some(assets.glb(n)?), _ => None };
        let mut swaps: HashMap<&str, TexId> = HashMap::new();
        let mut outfit: HashMap<String, TexId> = HashMap::new();
        let mut tex_src: HashMap<u32, TexSrc> = HashMap::new();
        if let Some(o) = spec["outfit"].as_object() {
            // 今の版: 元の map の設定（sRGB・flipY なし・GLB のサンプラー）のまま画像だけを替える
            for (mat, file) in o { let im = assets.image(file.as_str().unwrap_or(""))?; let t = image_texture(kit, &im); tex_src.insert(t.0, TexSrc::Image(im)); outfit.insert(mat.clone(), t); }
        }
        if mature {
            for (suffix, key) in [("Face_00_SKIN", "skin"), ("FaceEyelash_00_FACE", "eyelash"), ("FaceEyeline_00_FACE", "eyeline")] {
                if let Some(file) = spec["mature"][key].as_str() { let im = assets.image(file)?; let t = image_texture(kit, &im); tex_src.insert(t.0, TexSrc::Image(im)); swaps.insert(suffix, t); }
            }
        }
        let root = kit.scene.group(None);
        kit.scene[root].name = "CharacterMaker".into();
        let mut mats: Vec<Mat> = Vec::new();
        let mut made: HashMap<(u8, usize), MatId> = HashMap::new();
        let mut pick = |kit: &mut Kit, from: &Glb, tex: &Arc<Glb>, which: u8, m: Option<&glb::Material>, mats: &mut Vec<Mat>, tex_src: &mut HashMap<u32, TexSrc>| -> MatId {
            let index = m.and_then(|x| from.materials.iter().position(|y| std::ptr::eq(x, y))).unwrap_or(usize::MAX);
            if let Some(id) = made.get(&(which, index)) { return *id; }
            let mut mat = unlit(kit, tex, m);
            if let (Some(t), Some(img)) = (mat.map, m.and_then(|m| m.texture)) { tex_src.insert(t.0, TexSrc::Glb(tex.clone(), img)); }
            let name = m.map_or(String::new(), |m| m.name.clone());
            if let Some(t) = outfit.get(&name) && (name == "maker-outfit" || name.ends_with("_CLOTH")) && mat.map.is_some() { mat.map = Some(*t); }
            for (suffix, t) in &swaps { if name.ends_with(suffix) && mat.map.is_some() { mat.map = Some(*t); } }
            let id = kit.material(mat);
            made.insert((which, index), id);
            mats.push(Mat { id, name });
            id
        };
        let body = instantiate_skinned(kit, Some(root), &g, &mut |kit, m| pick(kit, &g, &src, 0, m, &mut mats, &mut tex_src));
        // 見た目を編集すると形の端が変わるので、three の版も frustumCulled = false
        for m in &body.meshes { kit.scene[*m].frustum_culled = false; }
        let mut skel = Skeleton::from_glb(&g);
        let head = skel.find(if fantasy { "J_Bip_C_Head" } else { "Head" }).or_else(|| skel.find("J_Bip_C_Head")).or_else(|| skel.find("Head"));
        let hair = match hair_glb {
            Some(hg) => {
                let head_node = head.and_then(|h| body.nodes[h]).ok_or("モデルの頭の接続点が見つかりません。")?;
                let fit = |name: &str| match name { "hairFitMasculine" => if masculine { 1.0 } else { 0.0 }, "hairFitFeminine" => if masculine { 0.0 } else { 1.0 }, _ => 0.0 };
                let (n, boxes) = place_rigid(kit, head_node, &hg, &fit, &mut |kit, m| pick(kit, &hg, &hg, 1, m, &mut mats, &mut tex_src));
                kit.scene[n].name = "HairStyle".into();
                Some((n, hg, boxes))
            }
            None => None,
        };
        // モーフ（描く節点ごと＝GLB の部分ごと。1 部分なら節点そのもの、2 つ以上なら Group の下の部分）
        let mut prims = Vec::new();
        let mut skin_parts = Vec::new();
        for (i, node) in g.nodes.iter().enumerate() {
            let Some(mi) = node.mesh else { continue };
            let Some(id) = body.nodes[i] else { continue };
            let mesh = &g.meshes[mi];
            if let Some(skin) = node.skin { for p in 0..mesh.primitives.len() { skin_parts.push(SkinPart { mesh: mi, prim: p, skin, node: i }); } }
            let nodes = if mesh.primitives.len() == 1 { vec![id] } else { kit.scene[id].children().to_vec() };
            for (pi, n) in nodes.into_iter().enumerate() {
                let names = variant.names.get(&(mi, pi)).cloned().unwrap_or_default();
                let count = mesh.primitives[pi].morph_pos.len();
                if count == 0 { continue; }
                let mut weights = vec![0.0; count];
                for (k, w) in mesh.weights.iter().enumerate() { if k < count { weights[k] = *w; } }
                prims.push(PrimMorph { node: n, glb_node: i, mesh: mi, prim: pi, names, weights });
            }
        }
        // 武器（手の骨の子。武器なしでも見せる・隠すトラックの節点は作る。createMakerSword）
        let bone = spec["weapon"]["socket"]["bone"].as_str().map(str::to_string)
            .unwrap_or_else(|| if fantasy { "J_Bip_R_Hand".into() } else { "hand_r".into() });
        let weapon = Some(weapon::attach(kit, assets, &mut skel, &body.nodes, &spec["weapon"], &bone)?);
        if let Some(w) = &weapon && let Some(wg) = &w.glb { for (t, i) in &w.tex { tex_src.insert(*t, TexSrc::Glb(wg.clone(), *i)); } }
        let sword_index = weapon.as_ref().map(|w| w.skel_index);
        let equipped = !spec["weapon"].is_null();
        let mut mixer = Mixer::new();
        let mut actions = Vec::new();
        let mut clips: Vec<Arc<Clip>> = Vec::new();
        for name in spec["motions"].as_array().into_iter().flatten() {
            for clip in assets.clips(name.as_str().unwrap_or(""))? {
                let mut bound = clip.bind(&skel);
                if let Some(si) = sword_index {
                    bound.tracks.retain(|t| t.node != si);
                    bound.tracks.push(weapon::weapon_track(si, &bound.name, bound.duration, equipped));
                }
                clips.push(Arc::new(bound));
            }
        }
        // のけぞりは最後の物（presetHurtClips は hit を最後へ置く）。ほかの動作を Mixer へ
        let hit_clip = clips.iter().rev().find(|c| c.name == "hit").cloned();
        for clip in &clips { if clip.name != "hit" { actions.push((clip.name.clone(), mixer.add(clip.clone()))); } }
        let hit_layer = hit_clip.map(|clip| {
            let animated: std::collections::HashSet<(usize, u8)> = clips.iter().filter(|c| c.name != "hit").flat_map(|c| c.tracks.iter().map(|t| (t.node, t.prop as u8))).collect();
            let binds = clip.tracks.iter().enumerate().filter(|(_, t)| t.prop == Prop::R || t.prop == Prop::T).map(|(k, t)| {
                let rest = if animated.contains(&(t.node, t.prop as u8)) { None } else if t.prop == Prop::R { Some(skel.r[t.node].to_array()) } else { let v = skel.t[t.node]; Some([v.x, v.y, v.z, 0.0]) };
                (t.node, t.prop, k, rest)
            }).collect();
            HitLayer { duration: clip.seconds, clip, binds }
        });
        // アメコミの ＞＜ の板
        let hurt_face = if !fantasy {
            let (cover, marks) = (assets.image(spec["hurtFace"]["cover"].as_str().unwrap_or(""))?, assets.image(spec["hurtFace"]["marks"].as_str().unwrap_or(""))?);
            let (ct, mt) = (image_texture_clamp(kit, &cover), image_texture_clamp(kit, &marks));
            tex_src.insert(ct.0, TexSrc::Image(cover.clone())); tex_src.insert(mt.0, TexSrc::Image(marks.clone()));
            let h = head.ok_or("モデルの頭の接続点が見つかりません。")?;
            let hn = body.nodes[h].ok_or("モデルの頭の接続点が見つかりません。")?;
            Some(HurtFace::new(kit, &g, h, hn, ct, mt, Side::Double)?)
        } else { None };
        let expressions = match &variant.built { Some(b) => Some(Expressions::read(&src, b)?), None => None };
        let eyes = if fantasy { ['L', 'R'].iter().filter_map(|side| skel.find(&format!("J_Adj_{side}_FaceEye")).map(|i| (i, *side, skel.r[i], skel.r[i]))).collect::<Vec<_>>() } else { Vec::new() };
        if fantasy && eyes.len() != 2 { return Err("視線の接続点がありません。".into()); }
        let head_rest_scale = head.map_or(Vec3::ONE, |h| skel.s[h]);
        let mut model = Model { root, variant, body, skel, mixer, actions, current: None, mats, prims, skin_parts, hair, head, head_rest_scale, eyes, expressions, fantasy, masculine, applied: c.clone(), hurt_on: false, hurt_prop: false, hurt_face, weapon, hit_layer, hit: None, attack_done: false, events: Vec::new(), tex_src, mat_of: made, height: 1.0, playing: true, speed: 1.0 };
        model.apply(kit, c);
        Ok(model)
    }

    /// instance.apply(config)
    pub fn apply(&mut self, kit: &mut Kit, c: &Value) {
        self.applied = c.clone();
        let num = |k: &str| c[k].as_f64().unwrap_or(0.0) as f32;
        let s = |k: &str| c[k].as_str().unwrap_or("").to_string();
        self.height = c["height"].as_f64().unwrap_or(1.0) as f32;
        kit.scene[self.root].scale = Vec3::splat(self.height);
        let mature = self.fantasy && s("face") == "mature";
        // 頭の骨の大きさ（大人の顔。顔・頭皮・髪がいっしょに動く）。ほかは元の大きさ
        if let Some(h) = self.head {
            let mut sc = self.head_rest_scale;
            if mature { sc *= if self.masculine { Vec3::new(1.035, 1.0, 1.025) } else { Vec3::new(0.94, 1.01, 0.98) }; }
            self.skel.s[h] = sc;
        }
        let (base, face, eyes, mouth, build) = (s("base"), s("face"), s("eyes"), s("mouth"), num("build"));
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        let values: [(&str, f32); 10] = [
            ("hairFitMasculine", on(base == "masculine")), ("hairFitFeminine", on(base == "feminine")),
            ("bodySlim", (-build).max(0.0)), ("bodyBroad", build.max(0.0)),
            ("faceRound", on(face == "round")), ("faceAngular", on(face == "angular")),
            ("eyesRound", on(eyes == "round")), ("eyesSlender", on(eyes == "slender")),
            ("mouthSmile", on(mouth == "smile")), ("mouthFull", on(mouth == "full")),
        ];
        for p in &mut self.prims {
            for (name, v) in values { if let Some(k) = p.names.iter().position(|n| n == name) && k < p.weights.len() { p.weights[k] = v; } }
        }
        self.apply_expression(c);
        for p in &self.prims { kit.set_morph_weights(p.node, &p.weights); }
        self.update_hurt_face(kit);
        if let Some(color) = css_color(&s("hairColor")) {
            for m in &self.mats {
                if m.name == "maker-hair" || m.name == "maker-brow" {
                    let mut col = color;
                    if mature && self.masculine && m.name == "maker-brow" { col = Color { r: col.r * 0.62, g: col.g * 0.62, b: col.b * 0.62 }; }
                    kit.mat_mut(m.id).color = col;
                }
            }
        }
        self.apply_eye_gaze();
        self.write_pose(kit);
    }

    /// expression.apply(config, hurt) と視線
    fn apply_expression(&mut self, c: &Value) {
        let hurt = self.hurt_on;
        let key = if hurt { "hurt".to_string() } else { c["expression"].as_str().unwrap_or("neutral").to_string() };
        let strength = if hurt { 1.0 } else { c["expressionStrength"].as_f64().unwrap_or(1.0) as f32 };
        if let Some(e) = self.expressions.as_mut() { e.apply(&mut self.prims, &key, strength, c["eyes"].as_str().unwrap_or("")); }
        for e in &mut self.eyes {
            let yaw: f32 = match key.as_str() { "lookLeft" => if e.1 == 'L' { 12.0 } else { 8.0 }, "lookRight" => if e.1 == 'L' { -8.0 } else { -12.0 }, _ => 0.0 };
            let pitch: f32 = match key.as_str() { "lookUp" => 10.0, "lookDown" => -10.0, _ => 0.0 };
            e.3 = e.2 * Quat::from_euler(EulerRot::YXZ, (yaw * strength).to_radians(), (pitch * strength).to_radians(), 0.0);
        }
    }

    /// setHurtFace（＞＜。保存しない一時の表示。ファンタジーは表情のモーフ）
    fn set_hurt_face(&mut self, kit: &mut Kit, on: bool) {
        if let Some(h) = &self.hurt_face { h.show(kit, on); }
        if on == self.hurt_on { return; }
        self.hurt_on = on;
        let c = self.applied.clone();
        self.apply_expression(&c);
        for p in &self.prims { kit.set_morph_weights(p.node, &p.weights); }
    }
    /// ＞＜ の切り替え（押している間。のけぞりの間は自動で出る）
    pub fn set_hurt_prop(&mut self, on: bool) { self.hurt_prop = on; }
    /// のけぞりを最初から（連打は最初から再生し直す）
    pub fn start_hit(&mut self) { if self.hit_layer.is_some() { self.hit = Some(0.0); } }
    /// 顔の表示では登録された武器を隠す（setRegisteredWeaponsVisible）
    pub fn set_weapon_visible(&self, kit: &mut Kit, on: bool) { if let Some(c) = self.weapon.as_ref().and_then(|w| w.content) { kit.scene[c].visible = on; } }
    /// アメコミの ＞＜ の板を今の目の形に合わせる（hurtFace.update）
    fn update_hurt_face(&mut self, kit: &mut Kit) {
        let Some(h) = self.hurt_face.as_mut() else { return };
        let prims = &self.prims;
        h.update(kit, &self.variant.glb, &|node| prims.iter().find(|p| p.glb_node == node).map_or(Vec::new(), |p| p.weights.clone()));
    }

    /// applyEyeGaze: 動作のあと、目の骨を選んだ向きへ戻す（毎コマ）
    fn apply_eye_gaze(&mut self) { for e in &self.eyes { self.skel.r[e.0] = e.3; } }

    /// 動作を始める（CharacterMakerScene の Model の useEffect と同じ: 同じ動作を押し直したら止めて始め直す。振るは 1 回で止める）
    pub fn play(&mut self, motion: &str) {
        let Some(&(_, id)) = self.actions.iter().find(|(n, _)| n == motion) else { return };
        let previous = self.current.as_ref().map(|(_, a)| *a);
        if previous == Some(id) { self.mixer.stop(id, &mut self.skel); }
        self.mixer.reset(id);
        self.mixer.actions[id].time_scale = 1.0;
        self.mixer.set_effective_weight(id, 1.0);
        let once = motion == "sword_attack";
        self.mixer.actions[id].once = once;
        self.mixer.actions[id].clamp_when_finished = once;
        self.mixer.play(id, &self.skel);
        if let Some(p) = previous && p != id { self.mixer.cross_fade_from(id, p, 0.16); }
        self.current = Some((motion.to_string(), id));
        self.attack_done = false;
        self.mixer.advance(0.0, &mut self.skel);
        self.apply_eye_gaze();
    }

    /// 1 コマ（useFrame: 止めていなければ min(dt, 0.05) × speed だけ進める）
    pub fn frame(&mut self, kit: &mut Kit, dt: f64) {
        let step = if self.playing { dt.min(0.05) * self.speed as f64 } else { 0.0 };
        // のけぞり中は止めていても下の動作を書き直す（重ねた値が積み重ならないように）
        if self.playing || self.hit.is_some() { self.mixer.advance(step, &mut self.skel); }
        if let (Some(time), Some(layer)) = (self.hit.as_mut(), self.hit_layer.as_ref()) {
            *time += step;
            let t = time.min(layer.duration);
            let w = (smoothstep(0.0, HIT_FADE_IN, t) * (1.0 - smoothstep(layer.duration - HIT_FADE_OUT, layer.duration, t))) as f32;
            let mut v = [0.0f32; 4];
            for (node, prop, k, rest) in &layer.binds {
                if let Some(r) = rest { if *prop == Prop::R { self.skel.r[*node] = Quat::from_array(*r); } else { self.skel.t[*node] = Vec3::new(r[0], r[1], r[2]); } }
                layer.clip.tracks[*k].eval(t as f32, &mut v);
                if *prop == Prop::R { self.skel.r[*node] = three_slerp(self.skel.r[*node], Quat::from_array(v), w); }
                else { let a = self.skel.t[*node]; self.skel.t[*node] = a + (Vec3::new(v[0], v[1], v[2]) - a) * w; }
            }
            if *time >= layer.duration {
                self.hit = None;
                for (node, prop, _, rest) in &layer.binds { if let Some(r) = rest { if *prop == Prop::R { self.skel.r[*node] = Quat::from_array(*r); } else { self.skel.t[*node] = Vec3::new(r[0], r[1], r[2]); } } }
                self.events.push(json!({ "t": "hitComplete" }));
            }
        }
        // ＞＜：切り替えを押している間と、のけぞっている間
        let face = self.hurt_prop || self.hit.is_some();
        if face != self.hurt_on { self.set_hurt_face(kit, face); }
        // 振るが終わった（一回再生で止まった）
        if let Some((name, id)) = &self.current && name == "sword_attack" && !self.attack_done && self.mixer.actions[*id].paused {
            self.attack_done = true;
            self.events.push(json!({ "t": "motionComplete" }));
        }
        self.apply_eye_gaze();
        self.write_pose(kit);
    }

    fn write_pose(&self, kit: &mut Kit) {
        for (i, node) in self.body.nodes.iter().enumerate() {
            let Some(node) = *node else { continue };
            let n = &mut kit.scene[node];
            n.position = self.skel.t[i];
            n.rotation = Rot::Quat(self.skel.r[i]);
            n.scale = self.skel.s[i];
        }
        if let Some(w) = &self.weapon { kit.scene[w.root].scale = self.skel.s[w.skel_index]; }
    }

    /// 立った姿の箱（今の版: apply のあとの Box3.setFromObject(root)）。骨付きの部分は骨とモーフを当てた頂点、
    /// 骨の無い部分（髪）は形の箱の 8 隅を世界へ写す（three の Box3.expandByObject と同じ）
    pub fn rest_bounds(&self) -> (Vec3, Vec3) {
        let root = Mat4::from_scale(Vec3::splat(self.height));
        let mut skel = self.skel.clone();
        skel.update_world(root);
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        let g = &self.variant.glb;
        for part in &self.skin_parts {
            let p = &g.meshes[part.mesh].primitives[part.prim];
            let skin = &g.skins[part.skin];
            let bones: Vec<Mat4> = skin.joints.iter().zip(&skin.inverse_bind).map(|(j, ib)| skel.world[*j] * *ib).collect();
            let weights = self.prims.iter().find(|m| m.glb_node == part.node && m.prim == part.prim).map_or(&[][..], |m| m.weights.as_slice());
            let sw = normalize_skin_weights(&p.weights);
            for (v, pos) in p.positions.iter().enumerate() {
                let mut q = Vec3::from(*pos);
                for (k, w) in weights.iter().enumerate() { if *w != 0.0 && let Some(d) = p.morph_pos.get(k) { q += Vec3::from(d[v]) * *w; } }
                let (j, w) = (p.joints[v], sw[v]);
                let mut out = Vec3::ZERO;
                for c in 0..4 { if w[c] != 0.0 { out += bones[j[c] as usize].transform_point3(q) * w[c]; } }
                lo = lo.min(out); hi = hi.max(out);
            }
        }
        if let (Some((_, hg, boxes)), Some(h)) = (&self.hair, self.head) {
            let base = skel.world[h];
            let mats = hg.world_matrices();
            for (i, a, b, _) in boxes {
                let m = base * mats[*i];
                for k in 0..8 {
                    let c = Vec3::new(if k & 1 != 0 { b.x } else { a.x }, if k & 2 != 0 { b.y } else { a.y }, if k & 4 != 0 { b.z } else { a.z });
                    let w = m.transform_point3(c);
                    lo = lo.min(w); hi = hi.max(w);
                }
            }
        }
        (lo, hi)
    }

    /// 今の姿の描く頂点の箱（measureOriginalCameraBounds）。skel の世界は呼ぶ側が作る。
    /// 返り値: (体, 頭, 武器を除いた体)。頭は頭の骨の下の部分（髪）と、頭の骨の側の重みが 0.25 以上の頂点
    pub fn pose_bounds(&self, kit: &Kit, skel: &Skeleton) -> [(Vec3, Vec3); 3] {
        let empty = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        let mut out = [empty; 3];
        let add = |b: &mut (Vec3, Vec3), p: Vec3| { b.0 = b.0.min(p); b.1 = b.1.max(p); };
        let g = &self.variant.glb;
        let within = |mut i: usize, top: usize| loop { if i == top { break true; } match skel.parent[i] { Some(p) => i = p, None => break false } };
        for part in &self.skin_parts {
            let p = &g.meshes[part.mesh].primitives[part.prim];
            if let Some(m) = p.material && g.materials[m].alpha_blend && g.materials[m].base_color[3] <= 0.0 { continue; }
            let skin = &g.skins[part.skin];
            let bones: Vec<Mat4> = skin.joints.iter().zip(&skin.inverse_bind).map(|(j, ib)| skel.world[*j] * *ib).collect();
            let head_bones: Vec<bool> = skin.joints.iter().map(|j| self.head.is_some_and(|h| within(*j, h))).collect();
            let weights = self.prims.iter().find(|m| m.glb_node == part.node && m.prim == part.prim).map_or(&[][..], |m| m.weights.as_slice());
            let sw = normalize_skin_weights(&p.weights);
            let used: Vec<usize> = if p.indices.is_empty() { (0..p.positions.len()).collect() } else { let mut seen = vec![false; p.positions.len()]; for i in &p.indices { seen[*i as usize] = true; } (0..p.positions.len()).filter(|i| seen[*i]).collect() };
            for v in used {
                let mut q = Vec3::from(p.positions[v]);
                for (k, w) in weights.iter().enumerate() { if *w != 0.0 && let Some(d) = p.morph_pos.get(k) { q += Vec3::from(d[v]) * *w; } }
                let (j, w) = (p.joints[v], sw[v]);
                let mut o = Vec3::ZERO; let mut hw = 0.0;
                for c in 0..4 { if w[c] != 0.0 { o += bones[j[c] as usize].transform_point3(q) * w[c]; if head_bones[j[c] as usize] { hw += w[c]; } } }
                if !o.is_finite() { continue; }
                add(&mut out[0], o); add(&mut out[2], o);
                if hw >= 0.25 { add(&mut out[1], o); }
            }
        }
        if let (Some((_, hg, parts)), Some(h)) = (&self.hair, self.head) {
            let mats = hg.world_matrices();
            for (i, _, _, drawn) in parts {
                let m = skel.world[h] * mats[*i];
                for v in drawn { let o = m.transform_point3(Vec3::from(*v)); add(&mut out[0], o); add(&mut out[2], o); add(&mut out[1], o); }
            }
        }
        if let Some(w) = &self.weapon && w.content.is_some_and(|c| kit.scene[c].visible) {
            let base = skel.world[w.skel_index];
            if base.determinant().abs() >= 1e-12 {
                for (m, drawn) in &w.measure { let mm = base * *m; for v in drawn { add(&mut out[0], mm.transform_point3(Vec3::from(*v))); } }
            }
        }
        if out[1].0.x > out[1].1.x && out[0].0.x <= out[0].1.x {
            // 頭の無い模型の近い姿（上の 32%）
            out[1] = out[0];
            out[1].0.y = out[0].1.y - (out[0].1.y - out[0].0.y) * 0.32;
        }
        out
    }

    /// 今の skel と身長で世界の行列を作った写し
    fn posed(&self) -> Skeleton { let mut s = self.skel.clone(); s.update_world(Mat4::from_scale(Vec3::splat(self.height))); s }

    /// 顔の表示の箱（読み込んだ直後の姿で 1 回だけ測る。頭の骨からの差を身長 1 で）と、頭の骨の位置
    pub fn portrait(&self, kit: &Kit) -> Value {
        let skel = self.posed();
        let [_, head, _] = self.pose_bounds(kit, &skel);
        let anchor = self.head.map_or((head.0 + head.1) * 0.5, |h| skel.world[h].w_axis.truncate());
        let s = self.height.max(1e-3);
        json!({ "min": ((head.0 - anchor) / s).to_array(), "max": ((head.1 - anchor) / s).to_array(), "hasHead": self.head.is_some() })
    }

    /// 構えの包み（measureOriginalArmedCameraBounds: 動作を 12 Hz で通して測る。身長 1 の値）。names の動作が無ければ null
    pub fn armed(&self, kit: &Kit, names: &[String]) -> Value {
        let mut skel = self.skel.clone();
        let mut mixer = Mixer::new();
        let ids: Vec<ActionId> = names.iter().filter_map(|n| self.actions.iter().find(|(a, _)| a == n).map(|(_, id)| mixer.add(self.mixer.actions[*id].clip.clone()))).collect();
        if ids.len() != names.len() { return Value::Null; }
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        let (mut blo, mut bhi) = (lo, hi);
        for id in ids {
            mixer.stop_all(&mut skel);
            mixer.reset(id);
            mixer.actions[id].once = true; mixer.actions[id].clamp_when_finished = true;
            mixer.play(id, &skel);
            let d = mixer.actions[id].clip.duration as f64;
            let steps = (d * 12.0).ceil() as usize;
            for f in 0..=steps {
                mixer.actions[id].time = d * f as f64 / steps as f64;
                mixer.advance(0.0, &mut skel);
                skel.update_world(Mat4::from_scale(Vec3::splat(self.height)));
                let [body, _, bare] = self.pose_bounds(kit, &skel);
                lo = lo.min(body.0); hi = hi.max(body.1); blo = blo.min(bare.0); bhi = bhi.max(bare.1);
            }
        }
        let s = self.height.max(1e-3);
        json!({ "min": (lo / s).to_array(), "max": (hi / s).to_array(), "bodyMin": (blo / s).to_array(), "bodyMax": (bhi / s).to_array() })
    }

    /// 頭の骨・腰の骨の世界の位置（今のコマ）
    pub fn bone_position(&self, name: &str) -> Option<[f32; 3]> {
        let i = match name { "head" => self.head, _ => self.skel.find("Hips").or_else(|| self.skel.find("J_Bip_C_Hips")) }?;
        Some(self.posed().world[i].w_axis.truncate().to_array())
    }

    /// assemble の返り値（TS の restBounds と同じ形: 身長 1 の高さと頭の上）
    pub fn info(&self) -> Value {
        let (lo, hi) = self.rest_bounds();
        // 頭の骨の高さ（身長 1・動かす前の姿。classic のカメラの restHeadY）
        let mut rest = self.skel.clone();
        rest.update_world(Mat4::IDENTITY);
        let head_y = self.head.map_or(hi.y / self.height, |h| rest.world[h].w_axis.y);
        json!({ "restHeight": (hi.y - lo.y) / self.height, "restTop": hi.y / self.height, "min": [lo.x, lo.y, lo.z], "max": [hi.x, hi.y, hi.z], "height": self.height, "headY": head_y })
    }

    /// 開発用の読み出し
    pub fn debug(&self, kit: &Kit) -> Value {
        let m: Vec<Value> = self.prims.iter().map(|m| {
            let on: serde_json::Map<String, Value> = m.names.iter().zip(&m.weights).filter(|(_, w)| **w != 0.0).map(|(n, w)| (n.clone(), json!(w))).collect();
            json!({ "mesh": m.mesh, "prim": m.prim, "on": on, "kit": kit.morph_weights(m.node).iter().filter(|w| **w != 0.0).count() })
        }).collect();
        json!({ "morphs": m, "clips": self.actions.iter().map(|a| a.0.clone()).collect::<Vec<_>>(), "current": self.current.as_ref().map(|c| c.0.clone()) })
    }

    pub fn remove(self, kit: &mut Kit) {
        for s in &self.body.skeletons { kit.scene.remove_skeleton(*s); }
        kit.scene.remove(self.root);
    }
}

impl Actor for Model {
    fn info(&self, kit: &Kit) -> Value { let mut v = Model::info(self); v["portrait"] = self.portrait(kit); v }
    fn apply(&mut self, kit: &mut Kit, config: &Value) { Model::apply(self, kit, config); }
    fn signal(&mut self, kit: &mut Kit, v: &Value) {
        if let Some(b) = v["playing"].as_bool() { self.playing = b; }
        if let Some(s) = v["speed"].as_f64() { self.speed = s as f32; }
        if let Some(motion) = v["motion"].as_str() { self.play(motion); }
        if let Some(on) = v["hurtFace"].as_bool() { self.set_hurt_prop(on); }
        if v["hit"].as_bool() == Some(true) { self.start_hit(); }
        if let Some(on) = v["weaponVisible"].as_bool() { self.set_weapon_visible(kit, on); }
    }
    fn step(&mut self, kit: &mut Kit, dt: f64) { self.frame(kit, dt); }
    fn query(&self, kit: &Kit, v: &Value) -> Value {
        match v["q"].as_str().unwrap_or("") {
            "armed" => self.armed(kit, &v["names"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect::<Vec<_>>()),
            "bone" => json!(self.bone_position(v["name"].as_str().unwrap_or(""))),
            _ => Value::Null,
        }
    }
    fn take_events(&mut self) -> Vec<Value> { std::mem::take(&mut self.events) }
    fn debug(&self, kit: &Kit) -> Value { Model::debug(self, kit) }
    fn export(&mut self, kit: &mut Kit, v: &Value) -> Result<Vec<u8>, String> { crate::export::preset(self, kit, v) }
    fn remove(self: Box<Self>, kit: &mut Kit) { Model::remove(*self, kit); }
}
