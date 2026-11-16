// GLB の書き出し（今の版は three の GLTFExporter。lib/character-maker/assembly.ts の exportCharacterGlb）。段 R6。
// GlbWriter は形・骨・材質・画像・動作を glTF 2.0 の GLB に書く汎用の口（オリジナル 4 体の書き出しも使う）。
// 書き方は GLTFExporter に合わせる: 節点は前順で番号を振る・TRS で書く・unlit の材質は KHR_materials_unlit・
// 画像は PNG（一辺 1024 まで）・動作の離散は STEP・モーフの重みは weights の経路。
use crate::expressions::PrimMorph;
use crate::maker::{Model, TexSrc};
use character_actor::anim::{Clip, Prop};
use character_actor::glb::{self, Glb, Interp, Primitive};
use glam::{Mat4, Quat, Vec3};
use island_kit::geometry::Geometry;
use island_kit::{Kit, MatId, NodeId, Side};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// three の BufferGeometry.computeBoundingBox（モーフの差分の箱を順に足して広げる。相対の差分）
pub fn three_geometry_box(positions: &[[f32; 3]], morphs: &[Vec<[f32; 3]>]) -> (Vec3, Vec3) {
    let bx = |v: &[[f32; 3]]| v.iter().fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(a, b), p| (a.min(Vec3::from(*p)), b.max(Vec3::from(*p))));
    let (mut lo, mut hi) = bx(positions);
    for m in morphs {
        let (dl, dh) = bx(m);
        let a = lo + dl; lo = lo.min(a); hi = hi.max(a);
        let b = hi + dh; lo = lo.min(b); hi = hi.max(b);
    }
    (lo, hi)
}

/// GLTFLoader の computeBounds（形の箱を、モーフの差分の絶対値の最大で両側へ広げる）。three の読んだ形の geometry.boundingBox
pub fn gltf_loader_box(positions: &[[f32; 3]], morphs: &[Vec<[f32; 3]>]) -> (Vec3, Vec3) {
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for p in positions { lo = lo.min(Vec3::from(*p)); hi = hi.max(Vec3::from(*p)); }
    let mut disp = Vec3::ZERO;
    for m in morphs {
        let (mut a, mut b) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        for p in m { a = a.min(Vec3::from(*p)); b = b.max(Vec3::from(*p)); }
        disp = disp.max(a.abs().max(b.abs()));
    }
    (lo - disp, hi + disp)
}

/// three の PropertyBinding.sanitizeNodeName（GLTFLoader が節点の名前に使う）
pub fn sanitize(name: &str) -> String { name.chars().filter(|c| !matches!(c, '[' | ']' | '.' | ':' | '/')).map(|c| if c.is_whitespace() { '_' } else { c }).collect() }

/// GLTFLoader の createUniqueName（読んだ GLB ごとに、同じ名前の 2 つ目から _1・_2…）
#[derive(Default)]
pub struct UniqueNames(HashMap<String, u32>);
impl UniqueNames {
    pub fn get(&mut self, name: &str) -> String {
        let s = sanitize(name);
        match self.0.get_mut(&s) { Some(n) => { *n += 1; format!("{s}_{n}") } None => { self.0.insert(s.clone(), 0); s } }
    }
}

/// GLB の高さ（yukikau-package.ts の measureGlbHeight: GLTFLoader で読んだ scene の Box3.setFromObject）
pub fn glb_height(bytes: &[u8]) -> Result<f32, String> {
    let g = glb::parse(bytes)?;
    let world = g.world_matrices();
    let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
    for (i, n) in g.nodes.iter().enumerate() {
        let Some(mi) = n.mesh else { continue };
        let mesh = &g.meshes[mi];
        for p in &mesh.primitives {
            if let Some(sk) = n.skin {
                // SkinnedMesh.computeBoundingBox: モーフ（mesh.weights）と骨を当てた全部の頂点。世界へは箱の 8 隅（ふつう同じ）
                let skin = &g.skins[sk];
                let bones: Vec<Mat4> = skin.joints.iter().zip(&skin.inverse_bind).map(|(j, ib)| world[*j] * *ib).collect();
                let sw = island_kit::glb::normalize_skin_weights(&p.weights);
                for (v, pos) in p.positions.iter().enumerate() {
                    let mut q = Vec3::from(*pos);
                    for (k, w) in mesh.weights.iter().enumerate() { if *w != 0.0 && let Some(d) = p.morph_pos.get(k) { q += Vec3::from(d[v]) * *w; } }
                    let mut o = Vec3::ZERO;
                    for c in 0..4 { let w = sw[v][c]; if w != 0.0 { o += bones[p.joints[v][c] as usize].transform_point3(q) * w; } }
                    lo = lo.min(o); hi = hi.max(o);
                }
            } else {
                let (a, b) = gltf_loader_box(&p.positions, &p.morph_pos);
                for k in 0..8 {
                    let c = Vec3::new(if k & 1 != 0 { b.x } else { a.x }, if k & 2 != 0 { b.y } else { a.y }, if k & 4 != 0 { b.z } else { a.z });
                    let w = world[i].transform_point3(c); lo = lo.min(w); hi = hi.max(w);
                }
            }
        }
    }
    if lo.y > hi.y { return Err("模型の形がありません。".into()); }
    Ok(hi.y - lo.y)
}

/// 書き出す 1 つの形（three の Mesh 1 つ）
pub struct MeshOut<'a> {
    pub name: String,
    pub prim: &'a Primitive,
    /// 位置・法線を替えた値（無ければ prim のまま）
    pub positions: Option<&'a [[f32; 3]]>,
    pub normals: Option<&'a [[f32; 3]]>,
    pub material: Option<usize>,
    pub target_names: Vec<String>,
    pub weights: Vec<f32>,
    pub skin: Option<usize>,
}

/// 動作の 1 本（節点の番号は書き出す節点の番号）
pub struct TrackOut { pub node: usize, pub path: &'static str, pub step: bool, pub times: Vec<f32>, pub values: Vec<f32>, pub size: usize }

#[derive(Default)]
pub struct GlbWriter {
    pub nodes: Vec<Value>,
    meshes: Vec<Value>,
    skins: Vec<Value>,
    materials: Vec<Value>,
    textures: Vec<Value>,
    images: Vec<Value>,
    samplers: Vec<Value>,
    accessors: Vec<Value>,
    views: Vec<Value>,
    animations: Vec<Value>,
    bin: Vec<u8>,
    /// 画像の控え（同じ元は 1 回だけ書く）
    texture_of: HashMap<u32, usize>,
}

fn pad4(v: &mut Vec<u8>, byte: u8) { while v.len() % 4 != 0 { v.push(byte); } }

impl GlbWriter {
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> usize {
        pad4(&mut self.bin, 0);
        let off = self.bin.len();
        self.bin.extend_from_slice(bytes);
        let mut v = json!({ "buffer": 0, "byteOffset": off, "byteLength": bytes.len() });
        if let Some(t) = target { v["target"] = json!(t); }
        self.views.push(v);
        self.views.len() - 1
    }
    fn floats(&mut self, data: &[f32], kind: &str, n: usize, target: Option<u32>, minmax: bool) -> usize {
        let view = self.view(bytemuck::cast_slice(data), target);
        let mut a = json!({ "bufferView": view, "componentType": 5126, "count": data.len() / n, "type": kind });
        if minmax && !data.is_empty() {
            let (mut lo, mut hi) = (vec![f32::INFINITY; n], vec![f32::NEG_INFINITY; n]);
            for c in data.chunks_exact(n) { for k in 0..n { lo[k] = lo[k].min(c[k]); hi[k] = hi[k].max(c[k]); } }
            a["min"] = json!(lo); a["max"] = json!(hi);
        }
        self.accessors.push(a);
        self.accessors.len() - 1
    }
    fn indices(&mut self, data: &[u32]) -> usize {
        let max = data.iter().copied().max().unwrap_or(0);
        let (view, comp) = if max < 65535 {
            let v: Vec<u16> = data.iter().map(|x| *x as u16).collect();
            (self.view(bytemuck::cast_slice(&v), Some(34963)), 5123)
        } else { (self.view(bytemuck::cast_slice(data), Some(34963)), 5125) };
        self.accessors.push(json!({ "bufferView": view, "componentType": comp, "count": data.len(), "type": "SCALAR" }));
        self.accessors.len() - 1
    }

    /// 節点を足す（親の children へ入れる）。返り値は番号
    pub fn node(&mut self, parent: Option<usize>, name: &str, t: Vec3, r: Quat, s: Vec3, extras: Option<Value>) -> usize {
        let mut n = Map::new();
        if !name.is_empty() { n.insert("name".into(), json!(name)); }
        if t != Vec3::ZERO { n.insert("translation".into(), json!(t.to_array())); }
        if r != Quat::IDENTITY { n.insert("rotation".into(), json!(r.to_array())); }
        if s != Vec3::ONE { n.insert("scale".into(), json!(s.to_array())); }
        if let Some(e) = extras && !e.as_object().is_some_and(|o| o.is_empty()) { n.insert("extras".into(), e); }
        self.nodes.push(Value::Object(n));
        let i = self.nodes.len() - 1;
        if let Some(p) = parent {
            let c = self.nodes[p].as_object_mut().unwrap().entry("children").or_insert(json!([]));
            c.as_array_mut().unwrap().push(json!(i));
        }
        i
    }

    /// 形を節点に付ける
    pub fn mesh(&mut self, node: usize, m: &MeshOut) {
        let p = m.prim;
        let pos = m.positions.unwrap_or(&p.positions);
        let mut attrs = Map::new();
        attrs.insert("POSITION".into(), json!(self.floats(bytemuck::cast_slice(pos), "VEC3", 3, Some(34962), true)));
        let nor = m.normals.unwrap_or(&p.normals);
        if nor.len() == pos.len() { attrs.insert("NORMAL".into(), json!(self.floats(bytemuck::cast_slice(nor), "VEC3", 3, Some(34962), false))); }
        if p.uvs.len() == pos.len() { attrs.insert("TEXCOORD_0".into(), json!(self.floats(bytemuck::cast_slice(&p.uvs), "VEC2", 2, Some(34962), false))); }
        if p.colors.len() == pos.len() { attrs.insert("COLOR_0".into(), json!(self.floats(bytemuck::cast_slice(&p.colors), "VEC4", 4, Some(34962), false))); }
        if m.skin.is_some() && p.joints.len() == pos.len() {
            let j: Vec<u16> = p.joints.iter().flat_map(|x| x.iter().copied()).collect();
            let view = self.view(bytemuck::cast_slice(&j), Some(34962));
            self.accessors.push(json!({ "bufferView": view, "componentType": 5123, "count": pos.len(), "type": "VEC4" }));
            attrs.insert("JOINTS_0".into(), json!(self.accessors.len() - 1));
            let w = island_kit::glb::normalize_skin_weights(&p.weights);
            attrs.insert("WEIGHTS_0".into(), json!(self.floats(bytemuck::cast_slice(&w), "VEC4", 4, Some(34962), false)));
        }
        let mut prim = json!({ "attributes": Value::Object(attrs), "mode": 4 });
        if !p.indices.is_empty() { prim["indices"] = json!(self.indices(&p.indices)); }
        if let Some(mat) = m.material { prim["material"] = json!(mat); }
        if !p.morph_pos.is_empty() {
            let mut targets = Vec::new();
            for (k, d) in p.morph_pos.iter().enumerate() {
                let mut t = Map::new();
                t.insert("POSITION".into(), json!(self.floats(bytemuck::cast_slice(d), "VEC3", 3, Some(34962), true)));
                if let Some(nd) = p.morph_nor.get(k) && nd.len() == d.len() { t.insert("NORMAL".into(), json!(self.floats(bytemuck::cast_slice(nd), "VEC3", 3, Some(34962), false))); }
                targets.push(Value::Object(t));
            }
            prim["targets"] = json!(targets);
        }
        // three の GLTFExporter と同じく形の名前は書かない（節点の名前で探す）
        let mut mesh = json!({ "primitives": [prim] });
        if !p.morph_pos.is_empty() {
            mesh["weights"] = json!(m.weights);
            mesh["extras"] = json!({ "targetNames": m.target_names });
        }
        self.meshes.push(mesh);
        self.nodes[node]["mesh"] = json!(self.meshes.len() - 1);
        if let Some(s) = m.skin { self.nodes[node]["skin"] = json!(s); }
    }

    /// 節点の形に、独自の頂点の値を 1 つ足す（GLTFExporter の書き方: 名前は大文字にして頭に _ を足す。_species_head → __SPECIES_HEAD）。mesh のあとに呼ぶ
    pub fn extra_attribute(&mut self, node: usize, name: &str, data: &[f32], size: usize) {
        let Some(mi) = self.nodes[node]["mesh"].as_u64().map(|m| m as usize) else { return };
        let kind = match size { 2 => "VEC2", 3 => "VEC3", 4 => "VEC4", _ => "SCALAR" };
        let acc = self.floats(data, kind, size.max(1), Some(34962), false);
        self.meshes[mi]["primitives"][0]["attributes"][name] = json!(acc);
    }

    pub fn skin(&mut self, joints: &[usize], inverse: &[Mat4]) -> usize {
        let data: Vec<f32> = inverse.iter().flat_map(|m| m.to_cols_array()).collect();
        let acc = self.floats(&data, "MAT4", 16, None, false);
        self.skins.push(json!({ "joints": joints, "inverseBindMatrices": acc }));
        self.skins.len() - 1
    }

    /// 画像（RGBA。一辺 1024 まで縮めて PNG）。key は同じ元を 1 回だけ書くための番号
    fn texture(&mut self, key: u32, im: &glb::Image, clamp: bool) -> usize {
        if let Some(t) = self.texture_of.get(&key) { return *t; }
        let (w, h) = (im.width.min(1024), im.height.min(1024));
        let buf = image::RgbaImage::from_raw(im.width, im.height, im.rgba.clone()).unwrap_or_default();
        let buf = if (w, h) != (im.width, im.height) { image::imageops::resize(&buf, w, h, image::imageops::FilterType::Triangle) } else { buf };
        let mut png = Vec::new();
        let _ = image::ImageEncoder::write_image(image::codecs::png::PngEncoder::new(&mut png), buf.as_raw(), w, h, image::ExtendedColorType::Rgba8);
        let view = self.view(&png, None);
        self.images.push(json!({ "mimeType": "image/png", "bufferView": view }));
        let wrap = if clamp { 33071 } else { 10497 };
        let sampler = json!({ "magFilter": 9729, "minFilter": 9987, "wrapS": wrap, "wrapT": wrap });
        let si = match self.samplers.iter().position(|s| *s == sampler) { Some(i) => i, None => { self.samplers.push(sampler); self.samplers.len() - 1 } };
        self.textures.push(json!({ "sampler": si, "source": self.images.len() - 1 }));
        self.texture_of.insert(key, self.textures.len() - 1);
        self.textures.len() - 1
    }

    /// unlit の材質（GLTFExporter の MeshBasicMaterial の書き方）
    pub fn material(&mut self, kit: &Kit, id: MatId, tex: &dyn Fn(u32) -> Option<(glb::Image, bool)>) -> usize {
        let m = kit.mat(id);
        let mut pbr = Map::new();
        let color = [m.color.r, m.color.g, m.color.b, m.opacity];
        if color != [1.0; 4] { pbr.insert("baseColorFactor".into(), json!(color)); }
        pbr.insert("metallicFactor".into(), json!(0));
        pbr.insert("roughnessFactor".into(), json!(1));
        if let Some(t) = m.map && let Some((im, clamp)) = tex(t.0) {
            let ti = self.texture(t.0, &im, clamp);
            pbr.insert("baseColorTexture".into(), json!({ "index": ti }));
        }
        let mut def = json!({ "pbrMetallicRoughness": Value::Object(pbr), "extensions": { "KHR_materials_unlit": {} } });
        if m.transparent { def["alphaMode"] = json!("BLEND"); } else if m.alpha_test > 0.0 { def["alphaMode"] = json!("MASK"); def["alphaCutoff"] = json!(m.alpha_test); }
        if m.side == Side::Double { def["doubleSided"] = json!(true); }
        self.materials.push(def);
        self.materials.len() - 1
    }

    pub fn animation(&mut self, name: &str, tracks: &[TrackOut]) {
        let (mut samplers, mut channels) = (Vec::new(), Vec::new());
        for t in tracks {
            let input = self.floats(&t.times, "SCALAR", 1, None, true);
            let kind = match t.size { 1 => "SCALAR", 3 => "VEC3", 4 => "VEC4", _ => "SCALAR" };
            let output = self.floats(&t.values, if t.path == "weights" { "SCALAR" } else { kind }, if t.path == "weights" { 1 } else { t.size }, None, false);
            samplers.push(json!({ "input": input, "output": output, "interpolation": if t.step { "STEP" } else { "LINEAR" } }));
            channels.push(json!({ "sampler": samplers.len() - 1, "target": { "node": t.node, "path": t.path } }));
        }
        self.animations.push(json!({ "name": name, "samplers": samplers, "channels": channels }));
    }

    pub fn finish(mut self, root: usize) -> Vec<u8> {
        pad4(&mut self.bin, 0);
        let mut j = json!({
            "asset": { "version": "2.0", "generator": "character-maker-stage (Rust)" },
            "scene": 0, "scenes": [{ "nodes": [root] }], "nodes": self.nodes,
            "buffers": [{ "byteLength": self.bin.len() }], "bufferViews": self.views, "accessors": self.accessors,
        });
        for (k, v) in [("meshes", self.meshes), ("skins", self.skins), ("materials", self.materials), ("textures", self.textures), ("images", self.images), ("samplers", self.samplers), ("animations", self.animations)] {
            if !v.is_empty() { j[k] = Value::Array(v); }
        }
        if j.get("materials").is_some() { j["extensionsUsed"] = json!(["KHR_materials_unlit"]); }
        let mut text = serde_json::to_vec(&j).unwrap_or_default();
        pad4(&mut text, b' ');
        let total = 12 + 8 + text.len() + 8 + self.bin.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(&0x4654_6C67u32.to_le_bytes());
        out.extend_from_slice(&2u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x4E4F_534Au32.to_le_bytes());
        out.extend_from_slice(&text);
        out.extend_from_slice(&(self.bin.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x004E_4942u32.to_le_bytes());
        out.extend_from_slice(&self.bin);
        out
    }
}

/// 骨の木の動作を書き出す 1 本へ（節点の番号を替える）
pub fn clip_tracks(clip: &Clip, map: &HashMap<usize, usize>) -> Vec<TrackOut> {
    clip.tracks.iter().filter_map(|t| {
        let node = *map.get(&t.node)?;
        let path = match t.prop { Prop::T => "translation", Prop::R => "rotation", Prop::S => "scale", Prop::W => return None };
        Some(TrackOut { node, path, step: t.interp == Interp::Step, times: t.times.to_vec(), values: t.values.to_vec(), size: t.size })
    }).collect()
}

fn geometry_prim(g: &Geometry) -> Primitive {
    Primitive { positions: g.positions.clone(), normals: g.normals.clone(), uvs: g.uvs.clone(), indices: g.indices.clone().unwrap_or_default(), ..Default::default() }
}

/// プリセットの書き出し（exportCharacterGlb。組み立てた直後の姿で呼ぶ）
pub fn preset(m: &mut Model, kit: &mut Kit, _v: &Value) -> Result<Vec<u8>, String> {
    let mut w = GlbWriter::default();
    let g = m.variant.glb.clone();
    let tex_src = m.tex_src.clone();
    let clamp_tex: Vec<u32> = m.hurt_face.as_ref().map_or(Vec::new(), |h| [h.cover_mat, h.marks_mat].iter().filter_map(|id| kit.mat(*id).map.map(|t| t.0)).collect());
    let tex = move |t: u32| -> Option<(glb::Image, bool)> {
        let s = tex_src.get(&t)?;
        let im = s.image();
        Some((glb::Image { width: im.width, height: im.height, rgba: im.rgba.clone() }, clamp_tex.contains(&t) || matches!(s, TexSrc::Image(_)) && clamp_tex.contains(&t)))
    };
    let mut mat_cache: HashMap<u32, usize> = HashMap::new();
    let mut mat = |w: &mut GlbWriter, id: MatId| -> usize { *mat_cache.entry(id.0).or_insert_with(|| w.material(kit, id, &tex)) };
    // 根（CharacterMaker。身長の大きさ・設定の控え）
    let c = m.applied.clone();
    let mut maker = json!({
        "assetVersion": if m.fantasy { "fantasy-maker-v1" } else { "human-maker-v1" }, "config": c,
        "playback": crate::look::playback(),
        "hurtFace": { "node": crate::hurt_face::NODE, "method": if m.fantasy { "morph" } else { "overlay" } },
    });
    if let Some(e) = &m.expressions { maker["expressionMorphs"] = Value::Object(e.profile.clone()); }
    let root = w.node(None, "CharacterMaker", Vec3::ZERO, Quat::IDENTITY, Vec3::splat(m.height), Some(json!({ "characterMaker": maker })));
    // GLB の scene（GLTFLoader の scene の Group）
    let scene_name = g.json["scenes"][0]["name"].as_str().unwrap_or("").to_string();
    let scene = w.node(Some(root), &scene_name, Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, None);
    // 骨の木の番号 → 書き出す節点
    let mut map: HashMap<usize, usize> = HashMap::new();
    // 骨付きの部分は、骨の節点がそろってから skin を作る
    let mut pending: Vec<(usize, usize, usize, usize)> = Vec::new(); // (書き出す節点, mesh, prim, skin)
    fn walk(w: &mut GlbWriter, m: &Model, g: &Glb, i: usize, parent: usize, map: &mut HashMap<usize, usize>, pending: &mut Vec<(usize, usize, usize, usize)>, names: &mut UniqueNames) {
        let n = &g.nodes[i];
        let id = w.node(Some(parent), &names.get(&n.name), m.skel.t[i], m.skel.r[i], m.skel.s[i], None);
        map.insert(i, id);
        if let Some(mi) = n.mesh {
            let prims = g.meshes[mi].primitives.len();
            for pi in 0..prims {
                let target = if prims == 1 { id } else { w.node(Some(id), &names.get(&g.meshes[mi].name), Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, None) };
                pending.push((target, mi, pi, n.skin.unwrap_or(usize::MAX)));
            }
        }
        for &ch in &n.children { walk(w, m, g, ch, id, map, pending, names); }
    }
    let mut body_names = UniqueNames::default();
    body_names.get(&scene_name);
    for &r in &g.scene { walk(&mut w, m, &g, r, scene, &mut map, &mut pending, &mut body_names); }
    // 頭の骨の子: 髪（HairStyle）と ＞＜ の節点。手の骨の子: 武器
    if let (Some((_, hg, _)), Some(h)) = (&m.hair, m.head) {
        let hair_root = w.node(Some(map[&h]), "HairStyle", Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, None);
        let base = &m.applied["base"];
        let fit = |name: &str| match name { "hairFitMasculine" => if base == "masculine" { 1.0 } else { 0.0 }, "hairFitFeminine" => if base == "masculine" { 0.0 } else { 1.0 }, _ => 0.0 };
        fn hair(w: &mut GlbWriter, kit: &Kit, m: &Model, hg: &Glb, i: usize, parent: usize, fit: &dyn Fn(&str) -> f32, mat: &mut dyn FnMut(&mut GlbWriter, MatId) -> usize, names: &mut UniqueNames) {
            let n = &hg.nodes[i];
            let id = w.node(Some(parent), &names.get(&n.name), n.t, n.r, n.s, None);
            if let Some(mi) = n.mesh {
                let mesh = &hg.meshes[mi];
                for (pi, p) in mesh.primitives.iter().enumerate() {
                    let target = if mesh.primitives.len() == 1 { id } else { w.node(Some(id), &names.get(&mesh.name), Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, None) };
                    let material = p.material.and_then(|k| m.mat_of.get(&(1, k)).copied()).map(|id| mat(w, id));
                    let weights = mesh.target_names.iter().map(|t| fit(t)).collect();
                    w.mesh(target, &MeshOut { name: mesh.name.clone(), prim: p, positions: None, normals: None, material, target_names: mesh.target_names.clone(), weights, skin: None });
                }
            }
            let _ = kit;
            for &ch in &n.children { hair(w, kit, m, hg, ch, id, fit, mat, names); }
        }
        let mut hair_names = UniqueNames::default();
        for &r in &hg.scene { hair(&mut w, kit, m, hg, r, hair_root, &fit, &mut mat, &mut hair_names); }
    }
    // 武器（kit の木をそのまま。CharacterMakerSword の scale は 0）
    if let Some(wp) = &m.weapon {
        let hand = m.skel.parent[wp.skel_index].unwrap();
        fn kit_node(w: &mut GlbWriter, kit: &Kit, n: NodeId, parent: usize, wp: &crate::weapon::Weapon, mat: &mut dyn FnMut(&mut GlbWriter, MatId) -> usize, extras: Option<Value>) -> usize {
            let k = &kit.scene[n];
            let id = w.node(Some(parent), &k.name, k.position, k.rotation.to_quat(), k.scale, extras);
            // GLB の節点の部分が 1 つなら、three と同じく節点そのものを形にする（weapon.rs は節点の下に形を置いている）
            let children = kit.scene[n].children().to_vec();
            let only = if children.len() == 1 && kit.scene[children[0]].name.is_empty() && wp.parts.iter().any(|(pn, _, _)| *pn == children[0]) { Some(children[0]) } else { None };
            if let Some((_, q, mid)) = wp.parts.iter().find(|(pn, _, _)| *pn == n || Some(*pn) == only) {
                let material = Some(mat(w, *mid));
                w.mesh(id, &MeshOut { name: k.name.clone(), prim: q, positions: None, normals: None, material, target_names: Vec::new(), weights: Vec::new(), skin: None });
            }
            if only.is_none() { for ch in children { kit_node(w, kit, ch, id, wp, mat, None); } }
            id
        }
        let sword_extras = json!({ "attachmentBone": m.skel.names[hand], "makerWeapon": m.applied["weaponId"].as_str().unwrap_or("none"), "visibilityTrack": format!("{}.scale", crate::weapon::SWORD) });
        let id = kit_node(&mut w, kit, wp.root, map[&hand], wp, &mut mat, Some(sword_extras));
        map.insert(wp.skel_index, id);
    }
    let hurt_node = match m.head {
        Some(h) => {
            let id = w.node(Some(map[&h]), crate::hurt_face::NODE, Vec3::ZERO, Quat::IDENTITY, Vec3::ZERO, Some(json!({ "makerHurtFace": if m.fantasy { "morph" } else { "overlay" } })));
            if let Some(hf) = &m.hurt_face {
                for (suffix, geo, mid) in [("cover", &hf.cover_geo, hf.cover_mat), ("marks", &hf.marks_geo, hf.marks_mat)] {
                    let Some(geo) = geo else { continue };
                    let n = w.node(Some(id), &format!("{}-{suffix}", crate::hurt_face::NODE), Vec3::ZERO, Quat::IDENTITY, Vec3::ONE, None);
                    let prim = geometry_prim(geo);
                    let material = Some(mat(&mut w, mid));
                    w.mesh(n, &MeshOut { name: format!("{}-{suffix}", crate::hurt_face::NODE), prim: &prim, positions: None, normals: None, material, target_names: Vec::new(), weights: Vec::new(), skin: None });
                }
            }
            Some(id)
        }
        None => None,
    };
    // 骨付きの部分の形（モーフの名前と今の重み）と skin
    let mut skins: HashMap<usize, usize> = HashMap::new();
    let prim_of = |gn: usize, pi: usize| m.prims.iter().find(|p| p.glb_node == gn && p.prim == pi);
    for (target, mi, pi, sk) in pending.clone() {
        let gn = g.nodes.iter().position(|n| n.mesh == Some(mi)).unwrap_or(0);
        let p = &g.meshes[mi].primitives[pi];
        let skin = if sk != usize::MAX {
            Some(*skins.entry(sk).or_insert_with(|| { let s = &g.skins[sk]; let joints: Vec<usize> = s.joints.iter().map(|j| map[j]).collect(); w.skin(&joints, &s.inverse_bind) }))
        } else { None };
        let material = p.material.and_then(|k| m.mat_of.get(&(0, k)).copied()).map(|id| mat(&mut w, id));
        let (names, weights) = prim_of(gn, pi).map_or((Vec::new(), Vec::new()), |pm| (pm.names.clone(), pm.weights.clone()));
        let name = if g.meshes[mi].primitives.len() == 1 { g.meshes[mi].name.clone() } else if pi == 0 { g.meshes[mi].name.clone() } else { format!("{}_{pi}", g.meshes[mi].name) };
        w.mesh(target, &MeshOut { name, prim: p, positions: None, normals: None, material, target_names: names, weights, skin });
    }
    // 動作: 体の動作（武器の見せる・隠す入り）→ hit → face_hurt。ファンタジーは hit・face_hurt に ＞＜ のモーフの重み
    let morph_tracks: Vec<(usize, Vec<f32>, Vec<f32>)> = match m.expressions.as_mut() {
        Some(e) => {
            let mut normal: Vec<PrimMorph> = m.prims.iter().map(|p| PrimMorph { node: p.node, glb_node: p.glb_node, mesh: p.mesh, prim: p.prim, names: p.names.clone(), weights: p.weights.clone() }).collect();
            let mut hurt: Vec<PrimMorph> = m.prims.iter().map(|p| PrimMorph { node: p.node, glb_node: p.glb_node, mesh: p.mesh, prim: p.prim, names: p.names.clone(), weights: p.weights.clone() }).collect();
            let eyes = m.applied["eyes"].as_str().unwrap_or("").to_string();
            e.apply(&mut normal, m.applied["expression"].as_str().unwrap_or("neutral"), m.applied["expressionStrength"].as_f64().unwrap_or(1.0) as f32, &eyes);
            e.apply(&mut hurt, "hurt", 1.0, &eyes);
            normal.iter().zip(&hurt).filter(|(a, b)| a.weights.iter().zip(&b.weights).any(|(x, y)| (x - y).abs() > 1e-6))
                .filter_map(|(a, b)| pending.iter().find(|(_, mi, pi, _)| *mi == a.mesh && *pi == a.prim).map(|t| (t.0, a.weights.clone(), b.weights.clone()))).collect()
        }
        None => Vec::new(),
    };
    let clips: Vec<std::sync::Arc<Clip>> = m.actions.iter().map(|(_, id)| m.mixer.actions[*id].clip.clone()).collect();
    for clip in &clips { let t = clip_tracks(clip, &map); w.animation(&clip.name, &t); }
    if let Some(layer) = &m.hit_layer {
        let mut t = clip_tracks(&layer.clip, &map);
        let d = layer.duration as f32;
        if let Some(hn) = hurt_node { t.push(TrackOut { node: hn, path: "scale", step: true, times: vec![0.0, d], values: vec![1.0, 1.0, 1.0, 0.0, 0.0, 0.0], size: 3 }); }
        for (node, normal, hurt) in &morph_tracks { t.push(TrackOut { node: *node, path: "weights", step: true, times: vec![0.0, d], values: [hurt.clone(), normal.clone()].concat(), size: 1 }); }
        w.animation("hit", &t);
        let mut f = Vec::new();
        if let Some(hn) = hurt_node { f.push(TrackOut { node: hn, path: "scale", step: false, times: vec![0.0, 1.0], values: vec![1.0; 6], size: 3 }); }
        for (node, _, hurt) in &morph_tracks { f.push(TrackOut { node: *node, path: "weights", step: false, times: vec![0.0, 1.0], values: [hurt.clone(), hurt.clone()].concat(), size: 1 }); }
        w.animation("face_hurt", &f);
    }
    Ok(w.finish(root))
}
