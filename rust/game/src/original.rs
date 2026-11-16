// オリジナル 4 体（ハル・ナミ・ルクス・モカ）の模型（今の版は components/character-maker/OriginalEditableActor.tsx・OriginalCharacterScene.tsx と
// lib/character-maker/original-*.ts・hurt-face.ts・components/rig-actor/generic/*）。段 R5（docs/rust-stage-plan.md）。
// 組み立ての順は今の版の useMemo と同じ: 材質（Accessory_ のほかは光なし）→ 種族の部品 → 人の部品 → 体型の場 → 髪 → 武器（握った手）→ ＞＜ の板。
// 動きは 120 Hz の刻み（体・頭を回す・髪の揺れ・襟・しっぽ）と、顔のモーフ（まばたき・口・＞＜ の口）。
use crate::actor::Actor;
use crate::maker::{Assets, TexSrc};
// 部品は同じ所の別のファイル（lib.rs を替えずに足すため、この下の mod にする）
#[path = "original_edit.rs"]
mod edit;
#[path = "original_hurt.rs"]
mod hurt;
#[path = "original_rig.rs"]
mod rig;
#[path = "original_export.rs"]
mod export_glb;
use rig::{CollarAttachment, HAIR_STEP, HairSprings, TailMotion};
use crate::weapon::{self, Weapon};
use character_actor::anim::{ActionId, Clip, Mixer, Prop, Skeleton, three_decompose};
use character_actor::glb::{self, Glb};
use glam::{Mat4, Quat, Vec3};
use island_kit::geometry::Geometry;
use island_kit::glb::normalize_skin_weights;
use island_kit::{Color, GeoId, Kit, MatId, MatKind, Material, NodeId, Rot, Side, SkeletonId, Standard};
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

// ---- 材質と部分（組み立ての途中の、1 体ぶんの写し） ----

/// 材質の絵の出どころ（元の GLB の画像か、色替えなどで作った画像）
#[derive(Clone)]
pub enum ImgRef { None, Src(usize), Derived(Arc<Glb>) }
impl ImgRef {
    pub fn get<'a>(&'a self, g: &'a Glb) -> Option<&'a glb::Image> {
        match self { ImgRef::None => None, ImgRef::Src(i) => g.images.get(*i), ImgRef::Derived(h) => h.images.first() }
    }
}

thread_local! {
    /// 作った画像（同じ鍵は 1 回だけ作る）。kit は絵を GLB の場所で控えるので、入れ物は捨てずに持ち続ける
    static DERIVED: RefCell<HashMap<String, Arc<Glb>>> = RefCell::new(HashMap::new());
}

/// 1 つの材質（three は部分ごとに genericViewerMaterial の写しを作る）
#[derive(Clone)]
pub struct MatInfo {
    pub name: String,
    pub extras: Value,
    /// 線形の色
    pub color: [f32; 3],
    pub opacity: f32,
    pub img: ImgRef,
    /// 今の絵の鍵（作った画像の控えの鍵に使う）
    pub image_key: String,
    pub double: bool,
    pub transparent: bool,
    pub alpha_test: f32,
    pub depth_write: bool,
    /// Accessory_ の材質（光を受ける）。unlit の拡張なら光なしのまま tone mapping だけ効く
    pub accessory: Option<bool>,
    pub gltf: Option<usize>,
}
impl MatInfo {
    /// 色替えした絵に替える（鍵が同じなら作り直さない）
    pub fn derive_with(&mut self, g: &Glb, key: String, make: impl FnOnce(&glb::Image) -> Result<glb::Image, String>) -> Result<(), String> {
        if let Some(h) = DERIVED.with(|d| d.borrow().get(&key).cloned()) { self.img = ImgRef::Derived(h); self.image_key = key; return Ok(()); }
        let src = self.img.clone();
        let Some(im) = src.get(g) else { return Ok(()) };
        let made = make(im)?;
        let holder = Arc::new(Glb { json: Value::Null, nodes: Vec::new(), meshes: Vec::new(), skins: Vec::new(), animations: Vec::new(), materials: Vec::new(), images: vec![made], scene: Vec::new() });
        DERIVED.with(|d| d.borrow_mut().insert(key.clone(), holder.clone()));
        self.img = ImgRef::Derived(holder);
        self.image_key = key;
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PartKind { Glb, NativeHand, ClosedHand }

/// three の Mesh 1 つ（GLB の 1 部分。握った手・元の手も）
#[derive(Clone)]
pub struct Part {
    pub kind: PartKind,
    /// GLB の節点（骨の木の番号）。握った手は付ける骨
    pub node: usize,
    pub mesh: usize,
    pub prim: usize,
    /// three の名前（1 部分なら節点の名前、2 つ以上なら mesh の名前）
    pub name: String,
    /// object.userData（mesh の extras。1 部分なら節点の extras も重ねる）
    pub user: Value,
    /// geometry.userData（部分の extras）
    pub geo_user: Value,
    pub p: glb::Primitive,
    pub targets: Vec<String>,
    pub skin: Option<usize>,
    /// 足して 1 に直した重み（GLTFLoader の normalizeSkinWeights）
    pub weights_n: Vec<[f32; 4]>,
    pub mat: usize,
    /// 骨付き: GLB の中の部分の世界（bindMatrix）。焼いたあとは単位
    pub bind: Mat4,
    /// 焼く前の bind（GLB の書き出しで形の局所へ戻す）
    pub bind0: Mat4,
    /// 骨なし: 節点からの局所（握った手は大きさ 0 から）
    pub local: Mat4,
    /// 体型の場（元の位置と 1 あたりの差）
    pub body: Option<(Vec<[f32; 3]>, Vec<[f32; 3]>)>,
    /// 書き出しで写す独自の頂点の値（_SPECIES_* など。GLB の名前・値・幅）
    pub custom: Vec<(String, Vec<f32>, usize)>,
    /// GLB に TEXCOORD_0 があったか（読むときに無ければ 0 で埋まる。書き出しでは書かない）
    pub has_uv: bool,
}

fn sanitize(name: &str) -> String { character_actor::anim::sanitize_name(name) }
fn merge(a: &Value, b: &Value) -> Value {
    let mut out = a.as_object().cloned().unwrap_or_default();
    if let Some(o) = b.as_object() { for (k, v) in o { out.insert(k.clone(), v.clone()); } }
    Value::Object(out)
}

/// GLTFLoader の材質（genericViewerMaterial の前の値）
fn mat_info(g: &Glb, model: &str, index: Option<usize>) -> MatInfo {
    let Some(i) = index else {
        return MatInfo { name: String::new(), extras: Value::Null, color: [1.0; 3], opacity: 1.0, img: ImgRef::None, image_key: String::new(), double: false, transparent: false, alpha_test: 0.0, depth_write: true, accessory: None, gltf: None };
    };
    let m = &g.materials[i];
    let unlit = g.json["materials"][i]["extensions"].get("KHR_materials_unlit").is_some();
    MatInfo {
        name: m.name.clone(), extras: m.extras.clone(), color: [m.base_color[0], m.base_color[1], m.base_color[2]], opacity: m.base_color[3],
        img: m.texture.map_or(ImgRef::None, ImgRef::Src), image_key: m.texture.map_or(String::new(), |t| format!("{model}#{t}")),
        double: m.double_sided, transparent: m.alpha_blend, alpha_test: m.alpha_cutoff.unwrap_or(0.0), depth_write: !m.alpha_blend,
        accessory: if m.name.starts_with("Accessory_") { Some(unlit) } else { None }, gltf: Some(i),
    }
}

// ---- 動きの値 ----

#[derive(Clone, Copy, PartialEq, Default)]
struct Face { blink_left: f64, blink_right: f64, mouth_close: f64, aa: f64, ee: f64, ou: f64, happy: f64, angry: f64, sad: f64, surprised: f64, relaxed: f64, look_left: f64, look_right: f64, look_up: f64, look_down: f64 }
impl Face {
    fn read(v: &Value) -> Face {
        let n = |k: &str| v[k].as_f64().unwrap_or(0.0);
        Face { blink_left: n("blinkLeft"), blink_right: n("blinkRight"), mouth_close: if v["mouthClose"].is_null() { 1.0 } else { n("mouthClose") }, aa: n("aa"), ee: n("ee"), ou: n("ou"), happy: n("happy"), angry: n("angry"), sad: n("sad"), surprised: n("surprised"), relaxed: n("relaxed"), look_left: n("lookLeft"), look_right: n("lookRight"), look_up: n("lookUp"), look_down: n("lookDown") }
    }
    fn get(&self, k: &str) -> Option<f64> {
        Some(match k { "blinkLeft" => self.blink_left, "blinkRight" => self.blink_right, "mouthClose" => self.mouth_close, "aa" => self.aa, "ee" => self.ee, "ou" => self.ou, "happy" => self.happy, "angry" => self.angry, "sad" => self.sad, "surprised" => self.surprised, "relaxed" => self.relaxed, "lookLeft" => self.look_left, "lookRight" => self.look_right, "lookUp" => self.look_up, "lookDown" => self.look_down, _ => return None })
    }
    fn set(&mut self, k: &str, v: f64) {
        match k { "blinkLeft" => self.blink_left = v, "blinkRight" => self.blink_right = v, "mouthClose" => self.mouth_close = v, "aa" => self.aa = v, "ee" => self.ee = v, "ou" => self.ou = v, "happy" => self.happy = v, "angry" => self.angry = v, "sad" => self.sad = v, "surprised" => self.surprised = v, "relaxed" => self.relaxed = v, "lookLeft" => self.look_left = v, "lookRight" => self.look_right = v, "lookUp" => self.look_up = v, "lookDown" => self.look_down = v, _ => {} }
    }
    /// 気持ちの表情の口: 元の口のモーフの組み合わせで出す（怒りと悲しみは口角を下げる＝happy を負に、驚きは口を少し開ける、穏やかは少し口角を上げる）。
    /// keep: 気持ちを残す量（＞＜ の間は 0 へ）
    /// own_mouth: 口に気持ちのモーフを持つ模型（ルクス）。元の口のモーフは足さない
    pub(crate) fn with_emotion_mouth(mut self, keep: f64, own_mouth: bool) -> Face {
        self.angry *= keep; self.sad *= keep; self.surprised *= keep; self.relaxed *= keep;
        self.look_left *= keep; self.look_right *= keep; self.look_up *= keep; self.look_down *= keep;
        if own_mouth { return self; }
        self.happy += -0.45 * self.angry - 0.9 * self.sad + 0.3 * self.relaxed;
        self.aa = self.aa.max(0.6 * self.surprised);
        self.mouth_close *= 1.0 - self.surprised;
        self
    }
}
const NEUTRAL: Face = Face { blink_left: 0.0, blink_right: 0.0, mouth_close: 1.0, aa: 0.0, ee: 0.0, ou: 0.0, happy: 0.0, angry: 0.0, sad: 0.0, surprised: 0.0, relaxed: 0.0, look_left: 0.0, look_right: 0.0, look_up: 0.0, look_down: 0.0 };
/// のけぞりの重みの立ち上がり・戻り（秒）
const HIT_IN: f64 = 0.06;
const HIT_OUT: f64 = 0.15;

/// 描いた部分（kit の節点・形・今のモーフの重み）
struct Live { node: NodeId, geo: GeoId, geometry: Geometry, weights: Vec<f32>, mat: MatId }

/// ＞＜ の板
struct Plates { mesh: NodeId, mat: MatId, local: Vec<[f32; 3]>, weight: f32, skin: String, geometry: Geometry, per_eye: usize, tex: u32, image: Arc<glb::Image> }

pub struct Original {
    character: String,
    root: NodeId,
    lights: Vec<NodeId>,
    g: Arc<Glb>,
    parts: Vec<Part>,
    mats: Vec<MatInfo>,
    live: Vec<Live>,
    kit_nodes: Vec<Option<NodeId>>,
    skeletons: Vec<SkeletonId>,
    skel: Skeleton,
    mixer: Mixer,
    clips: Vec<(String, ActionId)>,
    hit_source: Option<Arc<Clip>>,
    hit_actions: HashMap<String, ActionId>,
    equipped: bool,
    weapon: Weapon,
    closed: Option<(usize, NodeId)>,
    native: Option<usize>,
    plates: Option<Plates>,
    hurt_mouth: Vec<(String, f64)>,
    springs: HairSprings,
    collar: CollarAttachment,
    tail: TailMotion,
    head: Option<usize>,
    head_pose: Quat,
    // 外側の大きさ（config.size）と、模型の置き方（viewer の modelOffsetY・modelScale と -90° の向き）
    size: f32,
    offset_y: f32,
    model_scale: f32,
    body_hz: Option<f64>,
    body_step: f64,
    // 設定
    motion: String,
    attack_revision: i64,
    playing: bool,
    speed: f64,
    face: Face,
    auto_blink: bool,
    speaking: bool,
    hair_motion: bool,
    head_turn: f64,
    build: f64,
    hurt_prop: bool,
    // 動きの状態
    active: Option<ActionId>,
    hit: Option<ActionId>,
    attack_paused: bool,
    elapsed: f64,
    last_report: f64,
    accumulator: f64,
    body_accumulator: f64,
    head_yaw: f64,
    head_settling: f64,
    face_tick: i64,
    face_input: Option<(Face, bool, bool, f32)>,
    events: Vec<Value>,
    hurt_skin: Option<String>,
    // 書き出し（段 R6）
    raw_clips: Vec<Arc<Clip>>,
    tex_src: HashMap<u32, (TexSrc, bool)>,
    /// 部分ごとの GLB の mesh.weights（three の読んだ直後の morphTargetInfluences）
    init: Vec<Vec<f32>>,
    closed_bone: Option<String>,
    hand_bone: String,
}

fn f(v: &Value, d: f64) -> f64 { v.as_f64().unwrap_or(d) }

/// spec の kind が 'original' のときの組み立て。spec: { character, model, raw?, clips: [名前…], weapon（プリセットと同じ形・武器なしは null）,
/// closedHand（hand.json・人の 2 体で武器ありのとき）, bodyMaps, hurtStyle, viewer: { bodyHz, modelOffsetY, modelScale }, appearance, config }
pub fn assemble(kit: &mut Kit, assets: &mut Assets, spec: &Value) -> Result<Box<dyn Actor>, String> {
    Ok(Box::new(Original::assemble(kit, assets, spec)?))
}

impl Original {
    fn base(&self) -> Mat4 {
        Mat4::from_scale(Vec3::splat(self.size)) * Mat4::from_translation(Vec3::new(0.0, self.offset_y, 0.0)) * Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2) * Mat4::from_scale(Vec3::splat(self.model_scale))
    }

    pub fn assemble(kit: &mut Kit, assets: &mut Assets, spec: &Value) -> Result<Original, String> {
        let character = spec["character"].as_str().unwrap_or("").to_string();
        let model = spec["model"].as_str().unwrap_or("").to_string();
        let g = assets.glb(&model)?;
        let raw = spec["raw"].as_str().and_then(|n| assets.image_opt(n));
        let appearance = &spec["appearance"];
        let config = &spec["config"];
        let mut skel = Skeleton::from_glb(&g);
        // 部分（model.traverse の順: 節点のあとに子）
        let mut parts: Vec<Part> = Vec::new();
        let mut mats: Vec<MatInfo> = Vec::new();
        let world = g.world_matrices();
        for &i in &skel.order.clone() {
            let node = &g.nodes[i];
            let Some(mi) = node.mesh else { continue };
            let mesh = &g.meshes[mi];
            let mesh_extras = g.json["meshes"][mi].get("extras").cloned().unwrap_or(Value::Null);
            let single = mesh.primitives.len() == 1;
            for (pi, p) in mesh.primitives.iter().enumerate() {
                let user = if single { merge(&mesh_extras, &node.extras) } else { mesh_extras.clone() };
                let name = if single { skel.names[i].clone() } else { sanitize(&mesh.name) };
                mats.push(mat_info(&g, &model, p.material));
                parts.push(Part {
                    kind: PartKind::Glb, node: i, mesh: mi, prim: pi, name, user,
                    geo_user: g.json["meshes"][mi]["primitives"][pi].get("extras").cloned().unwrap_or(Value::Null),
                    p: p.clone(), targets: mesh.target_names.clone(), skin: node.skin,
                    weights_n: if node.skin.is_some() { normalize_skin_weights(&p.weights) } else { Vec::new() },
                    mat: mats.len() - 1, bind: if node.skin.is_some() { world[i] } else { Mat4::IDENTITY }, bind0: if node.skin.is_some() { world[i] } else { Mat4::IDENTITY }, local: Mat4::IDENTITY, body: None,
                    custom: custom_attributes(&g, raw.as_ref().map(|r| r.rgba.as_slice()), mi, pi), has_uv: g.json["meshes"][mi]["primitives"][pi]["attributes"].get("TEXCOORD_0").is_some(),
                });
            }
        }
        // 種族の部品（ネコ・ロボット）と人の部品（顔の形・目・口）
        let species = if appearance["parts"].is_null() { None } else { Some(edit::SpeciesParts::read(&appearance["parts"])) };
        edit::species(&mut parts, &mut mats, &g, &character, species.as_ref(), raw.as_ref().map(|r| r.rgba.as_slice()))?;
        let human = edit::SpeciesParts::read(if appearance["parts"].is_null() { &Value::Null } else { &appearance["parts"] });
        edit::human_parts(&mut parts, &character, &human);
        // 気持ちの表情のモーフ（眉と目。模型に無いので、ここで作って足す）
        crate::original_emotion::extend(&mut parts, &character);
        // 体型の場（骨は bind の姿。根は単位）
        edit::body_field(&mut parts, &skel, &g, &character)?;
        // 髪型と髪の色
        let hairstyle = appearance["hairstyle"].as_str().unwrap_or("original").to_string();
        let hair_color = appearance["hairColor"].as_str().unwrap_or("source").to_lowercase();
        hair_textures(&mut parts, &mut mats, &g, &character, &hairstyle, &hair_color)?;
        // 握った手（人の 2 体で武器があるとき）
        let equipped = !spec["weapon"].is_null();
        let mut closed_prim = None;
        if equipped {
            let maps: Vec<Value> = spec["bodyMaps"].as_array().cloned().unwrap_or_default();
            closed_prim = edit::closed_hand(&mut parts, &spec["closedHand"], &maps)?;
        }
        for p in parts.iter_mut() { if p.skin.is_some() { edit::bake_bind(p); } }
        let head = skel.find("Head").or_else(|| skel.find("J_Bip_C_Head"));
        // ＞＜ の板（bind の姿で。顔の正面は模型の +X）
        let init: Vec<Vec<f32>> = parts.iter().map(|p| { let mut w = g.meshes[p.mesh].weights.clone(); w.resize(p.p.morph_pos.len(), 0.0); w }).collect();
        let style = hurt::Style::read(&spec["hurtStyle"]);
        let hurt_head = head.or_else(|| skel.find("mixamorigHead"));
        let plates = match (&style, skel.find("J_Bip_C_Head").or_else(|| skel.find("Head")).or_else(|| skel.find("mixamorigHead")).or(hurt_head)) {
            (Some(st), Some(h)) => hurt::create(&parts, &init, &mats, &g, &skel, h, st, Vec3::X).map(|p| (h, p)),
            _ => None,
        };
        let hurt_mouth = spec["hurtStyle"]["mouth"].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(0.0))).collect()).unwrap_or_default();

        // ---- 場面へ置く ----
        let root = kit.scene.group(None);
        kit.scene[root].name = "OriginalCharacter".into();
        let inner = kit.scene.group(Some(root));
        let viewer = &spec["viewer"];
        let (offset_y, model_scale) = (f(&viewer["modelOffsetY"], 0.0) as f32, f(&viewer["modelScale"], 2.0) as f32);
        {
            let n = &mut kit.scene[inner];
            n.position = Vec3::new(0.0, offset_y, 0.0);
            n.rotation = Rot::Quat(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2));
            n.scale = Vec3::splat(model_scale);
        }
        let mut kit_mats: Vec<Option<MatId>> = vec![None; mats.len()];
        // 書き出し: kit の絵の番号 → 元の画像と、端で止めるか
        let mut tex_src: HashMap<u32, (TexSrc, bool)> = HashMap::new();
        let any_accessory = mats.iter().any(|m| m.accessory.is_some());
        let mut material = |kit: &mut Kit, mats: &[MatInfo], mi: usize, vertex_colors: bool| -> MatId {
            if let Some(id) = kit_mats[mi] && kit.mat(id).vertex_colors == vertex_colors { return id; }
            let m = &mats[mi];
            let mut out = match m.accessory {
                Some(false) => {
                    let gm = &g.materials[m.gltf.unwrap()];
                    let mut s = Material::new(MatKind::Standard(Standard { emissive: Color::rgb(gm.emissive[0], gm.emissive[1], gm.emissive[2]), emissive_intensity: 1.0, metalness: gm.metallic, roughness: gm.roughness, flat_shading: false }));
                    s.tone_mapped = true;
                    s
                }
                Some(true) => { let mut b = Material::basic(); b.tone_mapped = true; b }
                None => { let mut b = Material::basic(); b.tone_mapped = false; b }
            };
            out.map = match &m.img { ImgRef::None => None, ImgRef::Src(i) => Some(kit.glb_texture(&g, *i)), ImgRef::Derived(h) => Some(kit.glb_texture(h, 0)) };
            // GLTFLoader と同じサンプラー（kit の glb_texture は繰り返し・ミップ付きのまま。モカの目の絵は端で止めてミップなし）
            if let (Some(t), Some(gi)) = (out.map, m.gltf) {
                let opts = sampler_opts(&g.json, gi);
                kit.set_tex_opts(t, opts);
                let src = match &m.img { ImgRef::Src(i) => Some(TexSrc::Glb(g.clone(), *i)), ImgRef::Derived(h) => Some(TexSrc::Glb(h.clone(), 0)), ImgRef::None => None };
                if let Some(src) = src { tex_src.insert(t.0, (src, opts.wrap_s == island_kit::texture::Wrap::Clamp)); }
            }
            out.color = Color::rgb(m.color[0], m.color[1], m.color[2]);
            out.opacity = m.opacity;
            out.transparent = m.transparent;
            out.depth_write = m.depth_write;
            out.alpha_test = m.alpha_test;
            if m.double { out.side = Side::Double; }
            out.vertex_colors = vertex_colors;
            let id = kit.material(out);
            kit_mats[mi] = Some(id);
            id
        };
        let skeletons: Vec<SkeletonId> = g.skins.iter().map(|_| kit.scene.new_skeleton(Vec::new(), Vec::new())).collect();
        let mut kit_nodes: Vec<Option<NodeId>> = vec![None; g.nodes.len()];
        let mut live: Vec<Option<Live>> = (0..parts.len()).map(|_| None).collect();
        // 外した髪の節点（部分が全部外れた mesh の節点。子も外れる）
        let kept: std::collections::HashSet<usize> = parts.iter().map(|p| p.node).collect();
        fn add(kit: &mut Kit, g: &Glb, skel: &Skeleton, i: usize, parent: NodeId, parts: &[Part], mats: &[MatInfo], kept: &std::collections::HashSet<usize>, skeletons: &[SkeletonId],
               material: &mut dyn FnMut(&mut Kit, &[MatInfo], usize, bool) -> MatId, kit_nodes: &mut [Option<NodeId>], live: &mut [Option<Live>]) {
            let node = &g.nodes[i];
            if node.mesh.is_some() && !kept.contains(&i) { return; }
            let mine: Vec<usize> = parts.iter().enumerate().filter(|(_, p)| p.node == i && p.kind == PartKind::Glb).map(|(k, _)| k).collect();
            let single = node.mesh.is_some_and(|m| g.meshes[m].primitives.len() == 1);
            let mut place = |kit: &mut Kit, parent: NodeId, k: usize| -> NodeId {
                let p = &parts[k];
                let geometry = geometry_of(&p.p);
                let vc = !p.p.colors.is_empty();
                let mat = material(kit, mats, p.mat, vc);
                let (id, geo) = match p.skin {
                    Some(s) => {
                        let geo = kit.geometry_skinned(&geometry, &p.p.joints, &p.weights_n);
                        if !p.p.morph_pos.is_empty() { kit.set_geometry_morphs(geo, &geometry, &p.p.morph_pos, &p.p.morph_nor); }
                        (kit.scene.skinned(Some(parent), geo, mat, skeletons[s]), geo)
                    }
                    None => { let geo = kit.geometry(&geometry); (kit.scene.mesh(Some(parent), geo, mat), geo) }
                };
                kit.scene[id].frustum_culled = false;
                kit.scene[id].name = p.name.clone();
                live[k] = Some(Live { node: id, geo, geometry, weights: Vec::new(), mat });
                id
            };
            let id = if single && mine.len() == 1 { place(kit, parent, mine[0]) } else if !mine.is_empty() {
                let grp = kit.scene.group(Some(parent));
                for k in &mine { place(kit, grp, *k); }
                grp
            } else { kit.scene.object(Some(parent)) };
            {
                let n = &mut kit.scene[id];
                if !(single && mine.len() == 1) { n.name = skel.names[i].clone(); }
                n.position = node.t; n.rotation = Rot::Quat(node.r); n.scale = node.s;
            }
            kit_nodes[i] = Some(id);
            // 元の手（体の部分の隣）
            for (k, p) in parts.iter().enumerate() {
                if p.kind != PartKind::NativeHand || p.node != i { continue; }
                place(kit, parent, k);
            }
            for &c in &node.children { add(kit, g, skel, c, id, parts, mats, kept, skeletons, material, kit_nodes, live); }
        }
        for &n in &g.scene.clone() { add(kit, &g, &skel, n, inner, &parts, &mats, &kept, &skeletons, &mut material, &mut kit_nodes, &mut live); }
        for (si, s) in g.skins.iter().enumerate() {
            let bones = s.joints.iter().map(|j| kit_nodes[*j].unwrap_or(inner)).collect();
            kit.scene.set_skeleton(skeletons[si], bones, s.inverse_bind.clone());
        }
        // 握った手（右手の骨の子。ふだんは大きさ 0）
        let mut closed = None;
        let closed_bone_name = closed_prim.as_ref().map(|(_, b)| b.clone());
        if let Some((prim, bone)) = closed_prim {
            let b = skel.find(&bone).ok_or("武器を持つ手の骨を読み込めません。")?;
            let parent = kit_nodes[b].ok_or("武器を持つ手の骨を読み込めません。")?;
            let mut hand_mat = Material::basic();
            hand_mat.vertex_colors = true; hand_mat.tone_mapped = false;
            let geometry = geometry_of(&prim);
            let geo = kit.geometry(&geometry);
            let mat = kit.material(hand_mat);
            let id = kit.scene.mesh(Some(parent), geo, mat);
            kit.scene[id].name = "OriginalSwordRightHand".into();
            kit.scene[id].scale = Vec3::ZERO;
            kit.scene[id].frustum_culled = false;
            mats.push(MatInfo { name: String::new(), extras: Value::Null, color: [1.0; 3], opacity: 1.0, img: ImgRef::None, image_key: String::new(), double: false, transparent: false, alpha_test: 0.0, depth_write: true, accessory: None, gltf: None });
            parts.push(Part { kind: PartKind::ClosedHand, node: b, mesh: 0, prim: 0, name: "OriginalSwordRightHand".into(), user: Value::Null, geo_user: Value::Null, p: prim, targets: Vec::new(), skin: None, weights_n: Vec::new(), mat: mats.len() - 1, bind: Mat4::IDENTITY, bind0: Mat4::IDENTITY, local: Mat4::from_scale(Vec3::ZERO), body: None, custom: Vec::new(), has_uv: false });
            live.push(Some(Live { node: id, geo, geometry, weights: Vec::new(), mat }));
            closed = Some((parts.len() - 1, id));
        }
        let native = parts.iter().position(|p| p.kind == PartKind::NativeHand);
        // 武器（右手の骨の子。武器なしでも見せる・隠すの節点は作る）
        let bone = spec["weapon"]["socket"]["bone"].as_str().unwrap_or("R_Hand").to_string();
        let mut weapon = weapon::attach(kit, assets, &mut skel, &kit_nodes, &spec["weapon"], &bone)?;
        // fitMakerWeaponScale(sword, hand, model, modelScale): 手の骨の大きさに加えて、模型を 2 倍にした分も打ち消す
        if let Some(&grip) = kit.scene[weapon.root].children().first() && let Some(hand) = skel.find(&bone) {
            skel.update_world(Mat4::IDENTITY);
            let s = three_decompose(skel.world[hand]).0 * model_scale;
            if s.x.abs() < 1e-8 || s.y.abs() < 1e-8 || s.z.abs() < 1e-8 { return Err("武器の大きさを手の骨に合わせられません。".into()); }
            kit.scene[grip].scale = Vec3::new(1.0 / s.x, 1.0 / s.y, 1.0 / s.z);
            weapon.measure = weapon.parts.iter().map(|(node, q, _)| {
                let mut m = Mat4::IDENTITY;
                let mut n = Some(*node);
                while let Some(id) = n { if id == weapon.root { break; } m = kit.scene[id].local() * m; n = kit.scene[id].parent(); }
                let used: std::collections::HashSet<u32> = q.indices.iter().copied().collect();
                (m, q.positions.iter().enumerate().filter(|(i, _)| used.is_empty() || used.contains(&(*i as u32))).map(|(_, v)| *v).collect())
            }).collect();
        }
        // ＞＜ の板（頭の骨の下の maker-hurt-face。光なし・半透明・両面・少し手前へ）
        let plates = match plates {
            Some((h, made)) => {
                let parent = kit_nodes[h].ok_or("モデルの頭の接続点が見つかりません。")?;
                let node = kit.scene.group(Some(parent));
                kit.scene[node].name = crate::hurt_face::NODE.into();
                let geometry = Geometry { positions: made.positions.clone(), normals: vec![[1.0, 0.0, 0.0]; made.positions.len()], uvs: made.uvs, colors: made.colors, sway: Vec::new(), indices: Some(made.indices), groups: Vec::new() };
                let geo = kit.geometry(&geometry);
                let image = Arc::new(hurt::draw_eye(&spec["hurtStyle"], &made.skin)?);
                let tex = kit.data_texture(image.width, image.height, &image.rgba, true);
                let mut m = Material::basic();
                m.map = Some(tex);
                m.vertex_colors = true; m.transparent = true; m.depth_write = true; m.alpha_test = 0.02; m.side = Side::Double; m.tone_mapped = false; m.opacity = 0.0; m.polygon_offset = Some((-2.0, -2.0));
                let mat = kit.material(m);
                let mesh = kit.scene.mesh(Some(node), geo, mat);
                kit.scene[mesh].name = format!("{}-eye", crate::hurt_face::NODE);
                kit.scene[mesh].render_order = -1;
                kit.scene[mesh].frustum_culled = false;
                kit.scene[mesh].visible = false;
                let per_eye = made.positions.len() / 2;
                Some(Plates { mesh, mat, local: made.positions, weight: 0.0, skin: made.skin, geometry, per_eye, tex: tex.0, image })
            }
            None => None,
        };
        // 光（Accessory_ の材質だけが受ける）
        let mut lights = Vec::new();
        if any_accessory {
            lights.push(kit.scene.ambient_light(None, Color::WHITE, 1.0));
            let a = kit.scene.dir_light(None, Color::WHITE, 2.5); kit.scene[a].position = Vec3::new(3.0, 5.0, 4.0); lights.push(a);
            let b = kit.scene.dir_light(None, Color::WHITE, 0.6); kit.scene[b].position = Vec3::new(-3.0, 2.0, -2.0); lights.push(b);
        }
        // ---- 動作（GLB の動作 + 武器の 2 つ + のけぞり。髪・襟・しっぽの骨は揺れの計算が持つ） ----
        let sword = weapon.skel_index;
        let skip = |skel: &Skeleton, node: usize| { let n = skel.names[node].as_str(); n.starts_with("Hair_") || n == "Collar_Mount" || n.starts_with("TailCurl_") || n.starts_with("KiyogonTail_") };
        let finish = |mut c: Clip, skel: &Skeleton| -> Arc<Clip> {
            c.tracks.retain(|t| t.node != sword && !skip(skel, t.node) && t.prop != Prop::W);
            c.tracks.push(weapon::weapon_track(sword, &c.name, c.duration, equipped));
            Arc::new(c)
        };
        // 書き出しは手を入れていない動作（GLB の動作と、武器の 2 つ・のけぞり）を使う
        let mut raw_clips: Vec<Arc<Clip>> = g.animations.iter().map(|a| Arc::new(Clip::from_glb(a, &skel))).collect();
        let mut body_clips: Vec<Arc<Clip>> = g.animations.iter().filter(|a| !["sword_idle", "sword_attack", "hit"].contains(&a.name.as_str())).map(|a| finish(Clip::from_glb(a, &skel), &skel)).collect();
        let mut hit_source = None;
        for name in spec["clips"].as_array().into_iter().flatten() {
            for c in assets.clips(name.as_str().unwrap_or(""))? {
                raw_clips.push(Arc::new(c.bind(&skel)));
                let bound = finish(c.bind(&skel), &skel);
                if bound.name == "hit" { hit_source = Some(bound.clone()); }
                body_clips.push(bound);
            }
        }
        let mut mixer = Mixer::new();
        let clips: Vec<(String, ActionId)> = body_clips.iter().filter(|c| c.name != "hit").map(|c| (c.name.clone(), mixer.add(c.clone()))).collect();
        // 揺れ・襟・しっぽ（骨の木の順）
        let order = skel.order.clone();
        let mut springs = HairSprings::new(&g, &skel, &order)?;
        let collar = CollarAttachment::new(&g, &skel, &order)?;
        let tail = TailMotion::new(&g, &order);
        let body_hz = viewer["bodyHz"].as_f64();
        let size = f(&config["size"], 1.0) as f32;
        let base = Mat4::from_scale(Vec3::splat(size)) * Mat4::from_translation(Vec3::new(0.0, offset_y, 0.0)) * Mat4::from_rotation_y(-std::f32::consts::FRAC_PI_2) * Mat4::from_scale(Vec3::splat(model_scale));
        springs.reset(&mut skel, base);
        let live: Vec<Live> = live.into_iter().enumerate().map(|(k, l)| { let mut l = l.expect("部分を置けませんでした"); l.weights = init.get(k).cloned().unwrap_or_default(); l }).collect();
        let head_pose = head.map_or(Quat::IDENTITY, |h| skel.r[h]);
        let hurt_skin = plates.as_ref().map(|p| p.skin.clone());
        if let Some(wg) = &weapon.glb { for (t, i) in &weapon.tex { tex_src.insert(*t, (TexSrc::Glb(wg.clone(), *i), false)); } }
        if let Some(p) = &plates { tex_src.insert(p.tex, (TexSrc::Image(p.image.clone()), true)); }
        let mut o = Original {
            character, root, lights, g: g.clone(), parts, mats, live, kit_nodes, skeletons, skel, mixer, clips, hit_source, hit_actions: HashMap::new(), equipped,
            weapon, closed, native, plates, hurt_mouth, springs, collar, tail, head, head_pose,
            size, offset_y, model_scale, body_hz,
            body_step: 1.0 / if spec["smoothBody"].as_bool().unwrap_or(true) { 120.0 } else { body_hz.unwrap_or(120.0) },
            motion: String::new(), attack_revision: 0, playing: true, speed: 1.0, face: Face::read(&config["face"]), auto_blink: false, speaking: false, hair_motion: false, head_turn: 0.0, build: f64::NAN, hurt_prop: false,
            active: None, hit: None, attack_paused: false, elapsed: 0.0, last_report: 0.0, accumulator: 0.0, body_accumulator: 0.0, head_yaw: 0.0, head_settling: 0.8, face_tick: -1, face_input: None, events: Vec::new(), hurt_skin,
            raw_clips, tex_src, init, closed_bone: closed_bone_name, hand_bone: bone.clone(),
        };
        o.apply_config(kit, config);
        o.write_pose(kit);
        Ok(o)
    }

    // ---- 設定 ----

    /// 作り直さずに当てる設定（体型・大きさ・表情・まばたき・話す・髪の揺れ・顔の向き）
    fn apply_config(&mut self, kit: &mut Kit, c: &Value) {
        self.size = f(&c["size"], self.size as f64) as f32;
        kit.scene[self.root].scale = Vec3::splat(self.size);
        let build = f(&c["appearance"]["build"], 0.0).clamp(-1.0, 1.0);
        if build != self.build {
            self.build = build;
            let b = build as f32;
            for (k, part) in self.parts.iter_mut().enumerate() {
                let Some((base, delta)) = &part.body else { continue };
                for (v, (p, d)) in part.p.positions.iter_mut().zip(base.iter().zip(delta)) { *v = [p[0] + b * d[0], p[1] + b * d[1], p[2] + b * d[2]]; }
                let l = &mut self.live[k];
                l.geometry.positions.clone_from(&part.p.positions);
                kit.update_geometry(l.geo, &l.geometry);
                if !part.p.morph_pos.is_empty() {
                    kit.set_geometry_morphs(l.geo, &l.geometry, &part.p.morph_pos, &part.p.morph_nor);
                    // 形の元が替わったので、同じ重みでも頂点を作り直させる
                    kit.set_morph_weights(l.node, &[]);
                    kit.set_morph_weights(l.node, &l.weights);
                }
            }
        }
        let face = Face::read(&c["face"]);
        self.face = face;
        self.auto_blink = c["autoBlink"].as_bool().unwrap_or(false);
        self.speaking = c["speaking"].as_bool().unwrap_or(false);
        let (hair_motion, head_turn) = (c["hairMotion"].as_bool().unwrap_or(false), f(&c["headTurn"], 0.0));
        if hair_motion != self.hair_motion || head_turn != self.head_turn { self.head_settling = 0.8; }
        self.hair_motion = hair_motion;
        self.head_turn = head_turn;
    }

    fn kind(&self) -> &'static str { match self.motion.as_str() { "walk" => "walk", "run" => "run", _ => "idle" } }

    fn report(&mut self) {
        if let Some(a) = self.active { let act = &self.mixer.actions[a]; self.events.push(json!({ "t": "report", "time": act.time, "duration": act.clip.duration })); }
    }

    /// 頭を動作の姿へ戻す・控える
    fn head_restore(&mut self) { if let Some(h) = self.head { self.skel.r[h] = self.head_pose; } }
    fn head_keep(&mut self) { if let Some(h) = self.head { self.head_pose = self.skel.r[h]; } }

    /// 動作を始める（OriginalEditableActor の motion の useEffect）。resume: 作り直す前の同じ動作の時刻
    fn set_motion(&mut self, kit: &mut Kit, motion: &str, attack_revision: i64, resume: Option<f64>) {
        self.motion = motion.to_string();
        self.attack_revision = attack_revision;
        // 基準姿勢: 動作を全部止めて、骨を素の姿（休止の姿）へ戻す（キャラクター一覧の「基準姿勢」と同じ）
        if motion == "rest" {
            self.mixer.stop_all(&mut self.skel);
            self.active = None; self.attack_paused = false;
            self.head_keep();
            let base = self.base();
            self.collar.update(&mut self.skel, base);
            self.springs.reset(&mut self.skel, base);
            self.report();
            return;
        }
        let Some(&(_, id)) = self.clips.iter().find(|(n, _)| n.ends_with(&format!(":{motion}"))).or_else(|| self.clips.iter().find(|(n, _)| n == motion)) else { return };
        let previous = self.active;
        if !self.playing { self.mixer.stop_all(&mut self.skel); }
        if previous == Some(id) { self.mixer.stop(id, &mut self.skel); }
        self.mixer.reset(id);
        let once = plays_once(motion);
        self.mixer.actions[id].once = once;
        self.mixer.actions[id].clamp_when_finished = once;
        self.mixer.play(id, &self.skel);
        self.mixer.actions[id].time = if previous.is_none() { resume.unwrap_or(0.0) } else { 0.0 };
        if self.playing && let Some(p) = previous && p != id { self.mixer.fade_out(p, 0.18); self.mixer.fade_in(id, 0.18); }
        else { self.mixer.set_effective_weight(id, 1.0); }
        self.active = Some(id);
        self.attack_paused = false;
        self.head_restore();
        self.mixer.advance(0.0, &mut self.skel);
        self.head_keep();
        let base = self.base();
        self.collar.update(&mut self.skel, base);
        self.springs.reset(&mut self.skel, base);
        self.accumulator = 0.0; self.body_accumulator = 0.0;
        self.report();
        let _ = kit;
    }

    /// 動作の途中へ（seek の useEffect）。value は 0..1
    fn seek(&mut self, value: f64) {
        let Some(a) = self.active else { return };
        self.head_restore();
        self.mixer.stop_all(&mut self.skel);
        self.mixer.set_effective_weight(a, 1.0);
        self.mixer.play(a, &self.skel);
        self.mixer.actions[a].time = value * self.mixer.actions[a].clip.duration as f64;
        self.mixer.advance(0.0, &mut self.skel);
        self.attack_paused = self.mixer.actions[a].paused;
        self.report();
        self.head_keep();
        self.head_yaw = 0.0;
        let base = self.base();
        self.collar.update(&mut self.skel, base);
        self.springs.reset(&mut self.skel, base);
        let (time, kind) = (self.mixer.actions[a].time, self.kind());
        self.tail.seek(&mut self.skel, time, kind);
        self.accumulator = 0.0; self.body_accumulator = 0.0; self.face_tick = -1;
    }

    /// 止める・動かす（playing の useEffect。動作が替わったときも走る）
    fn playing_effect(&mut self) {
        let Some(a) = self.active else { return };
        if !self.playing {
            let time = self.mixer.actions[a].time;
            self.head_restore();
            self.mixer.stop_all(&mut self.skel);
            self.mixer.set_effective_weight(a, 1.0);
            self.mixer.play(a, &self.skel);
            self.mixer.actions[a].time = time;
            self.mixer.advance(0.0, &mut self.skel);
            self.head_keep();
            self.head_settling = 0.8;
            let base = self.base();
            self.collar.update(&mut self.skel, base);
            self.springs.reset(&mut self.skel, base);
            let kind = self.kind();
            self.tail.seek(&mut self.skel, time, kind);
            self.accumulator = 0.0; self.body_accumulator = 0.0; self.face_tick = -1;
            self.report();
        } else {
            let act = &mut self.mixer.actions[a];
            if plays_once(&self.motion) && act.time >= act.clip.duration as f64 { act.time = 0.0; }
            act.paused = false;
            self.attack_paused = false;
        }
    }

    /// のけぞりを最初から（hitRevision の useEffect）
    fn start_hit(&mut self) {
        let Some(source) = self.hit_source.clone() else { self.events.push(json!({ "t": "hitComplete" })); return };
        let key = self.motion.clone();
        let id = match self.hit_actions.get(&key) {
            Some(id) => *id,
            None => {
                // 武器の見せる・隠すのトラックは今の動作と同じ値にする（重みで混ざっても変わらない）
                let mut c = (*source).clone();
                c.name = format!("hit:{key}");
                let sword = self.weapon.skel_index;
                c.tracks.retain(|t| t.node != sword);
                let v = if self.equipped && key.starts_with("sword_") { 1.0 } else { 0.0 };
                c.tracks.push(character_actor::anim::Track { node: sword, prop: Prop::S, interp: glb::Interp::Linear, times: Arc::new(vec![0.0, c.duration]), values: Arc::new(vec![v; 6]), size: 3 });
                let id = self.mixer.add(Arc::new(c));
                self.hit_actions.insert(key, id);
                id
            }
        };
        if let Some(h) = self.hit && h != id { self.mixer.stop(h, &mut self.skel); }
        self.mixer.stop(id, &mut self.skel);
        self.mixer.reset(id);
        self.mixer.actions[id].once = true;
        self.mixer.actions[id].clamp_when_finished = true;
        self.mixer.actions[id].weight = 0.0;
        self.mixer.play(id, &self.skel);
        self.hit = Some(id);
        if !self.playing && let Some(a) = self.active { self.mixer.actions[a].paused = true; }
    }

    /// のけぞりの重み（入り HIT_IN・戻り HIT_OUT）。下の動作は残りの重み。続いているか
    fn step_hit(&mut self) -> bool {
        let Some(h) = self.hit else { return false };
        let (duration, time, paused, active) = { let a = &self.mixer.actions[h]; (a.clip.duration as f64, a.time, a.paused, a.enabled) };
        let scheduled = active;
        let finished = !scheduled || paused || time >= duration - 1e-6;
        let weight = if finished { 0.0 } else { (time / HIT_IN).min((duration - time) / HIT_OUT).clamp(0.0, 1.0) } as f32;
        self.mixer.actions[h].weight = weight;
        for &(_, id) in &self.clips { self.mixer.actions[id].weight = 1.0 - weight; }
        if !finished { return true; }
        self.mixer.stop(h, &mut self.skel);
        self.hit = None;
        self.events.push(json!({ "t": "hitComplete" }));
        false
    }

    // ---- 1 コマ ----

    fn frame(&mut self, kit: &mut Kit, delta: f64) {
        let advancing = self.playing || self.hit.is_some();
        if advancing || self.head_settling > 0.0 || (self.head_yaw - self.head_turn).abs() > 0.001 {
            self.accumulator += delta.min(0.08);
            let base = self.base();
            while self.accumulator >= HAIR_STEP {
                self.head_restore();
                if advancing {
                    self.body_accumulator += HAIR_STEP;
                    if self.body_accumulator + 1e-8 >= self.body_step {
                        if self.step_hit() || self.playing {
                            self.mixer.advance(self.body_accumulator * self.speed, &mut self.skel);
                            if let Some(a) = self.active && plays_once(&self.motion) {
                                let p = self.mixer.actions[a].paused;
                                if p && !self.attack_paused { self.events.push(json!({ "t": "motionComplete" })); }
                                self.attack_paused = p;
                            }
                        }
                        self.body_accumulator = 0.0;
                    }
                    self.elapsed += HAIR_STEP;
                }
                if let Some(h) = self.head {
                    self.head_keep();
                    self.head_yaw += (self.head_turn - self.head_yaw) * (1.0 - (-9.0 * HAIR_STEP).exp());
                    // 体の縦の軸のまわりに、頭の親の空間で回す
                    self.skel.update_world(base);
                    let model_rot = three_decompose(base).1;
                    let axis = model_rot * Vec3::Y;
                    let parent_rot = self.skel.parent[h].map_or(Quat::IDENTITY, |p| three_decompose(self.skel.world[p]).1);
                    let axis = parent_rot.inverse() * axis;
                    self.skel.r[h] = Quat::from_axis_angle(axis.normalize_or_zero(), self.head_yaw.to_radians() as f32) * self.skel.r[h];
                }
                self.collar.update(&mut self.skel, base);
                self.springs.update(&mut self.skel, base, HAIR_STEP, if self.hair_motion { 1.0 } else { 0.0 });
                if self.playing { let kind = self.kind(); self.tail.update(&mut self.skel, HAIR_STEP, kind, self.speed); }
                self.head_settling = (self.head_settling - HAIR_STEP).max(0.0);
                self.accumulator -= HAIR_STEP;
            }
        }
        // ＞＜: 切り替えなら出し続け、のけぞりの間はすぐ出して、のけぞりの最後の HIT_OUT 秒で消える
        let hit_w = self.hit.map_or(0.0, |h| { let a = &self.mixer.actions[h]; ((a.clip.duration as f64 - a.time) / HIT_OUT).clamp(0.0, 1.0) });
        let hurt_weight = (if self.hurt_prop { 1.0 } else { 0.0f64 }).max(hit_w);
        if let Some(p) = self.plates.as_mut() && p.weight != hurt_weight as f32 {
            p.weight = hurt_weight as f32;
            kit.mat_mut(p.mat).opacity = p.weight.clamp(0.0, 1.0);
            kit.scene[p.mesh].visible = p.weight > 0.01;
        }
        self.update_face(kit, hurt_weight);
        // 手は選んだ動作のまま（構える・振るは握った手）
        self.apply_grip(kit, self.motion.starts_with("sword_"));
        if self.elapsed - self.last_report > 0.1 && self.active.is_some() { self.last_report = self.elapsed; self.report(); }
        self.write_pose(kit);
    }

    fn update_face(&mut self, kit: &mut Kit, hurt_weight: f64) {
        let mut shown = self.face.with_emotion_mouth(1.0 - hurt_weight.clamp(0.0, 1.0), self.character == "generic-robot");
        if hurt_weight > 0.0 { for (k, v) in &self.hurt_mouth { if let Some(cur) = shown.get(k) { shown.set(k, cur + (v - cur) * hurt_weight); } } }
        let next_tick = (self.elapsed * 60.0 + 1e-8).floor() as i64;
        let input = (self.face, self.auto_blink, self.speaking, hurt_weight as f32);
        let changed = self.face_input != Some(input);
        if !(self.body_hz.is_none() || next_tick != self.face_tick || changed) { return; }
        self.face_tick = next_tick;
        self.face_input = Some(input);
        let phase = self.elapsed % 4.1;
        let auto = if self.auto_blink { if self.character == "aichan" { rig::automatic_blink(self.elapsed) } else if phase > 3.72 { ((phase - 3.72) / 0.38 * std::f64::consts::PI).sin() } else { 0.0 } } else { 0.0 };
        let blink = hurt_weight.max(auto);
        let talk = if self.speaking && hurt_weight < 0.5 { Some(rig::speaking_mouth(self.elapsed)) } else { None };
        let talk_get = |k: &str| talk.as_ref().and_then(|t| t.iter().find(|(n, _)| *n == k).map(|(_, v)| *v));
        for (k, part) in self.parts.iter().enumerate() {
            if part.targets.is_empty() || part.p.morph_pos.is_empty() { continue; }
            let n = part.p.morph_pos.len();
            let mut w = vec![0f32; n];
            for (index, name) in part.targets.iter().enumerate().take(n) {
                if name == "makerGrip" { w[index] = self.live[k].weights.get(index).copied().unwrap_or(0.0); continue; }
                if let Some(rest) = name.strip_prefix("mouthCloseInbetween") && let Ok(c) = rest.parse::<f64>() && [25.0, 50.0, 75.0].contains(&c) {
                    let closure = talk_get("mouthClose").unwrap_or(shown.mouth_close);
                    w[index] = (1.0 - (closure - c / 100.0).abs() / 0.25).max(0.0) as f32;
                    continue;
                }
                let blink_key = if name.starts_with("blinkLeft") { "blinkLeft" } else { "blinkRight" };
                if let Some(c) = rig::blink_corrective(name, shown.get(blink_key).unwrap_or(0.0).max(blink)) { w[index] = c as f32; continue; }
                if let Some(key) = name.strip_suffix("Arc") {
                    let base = talk_get(key).or_else(|| shown.get(key)).or_else(|| NEUTRAL.get(key)).unwrap_or(0.0);
                    let closure = if key.starts_with("blink") { base.max(blink) } else { base };
                    w[index] = (std::f64::consts::PI * closure).sin() as f32;
                    continue;
                }
                let value = if self.character == "pyonmi" && self.speaking && name == "happy" { 0.0 } else { shown.get(name).unwrap_or(0.0) };
                w[index] = (if name.starts_with("blink") { value.max(blink) } else { talk_get(name).unwrap_or(value) }) as f32;
            }
            let l = &mut self.live[k];
            if l.weights != w { kit.set_morph_weights(l.node, &w); l.weights = w; }
        }
    }

    /// 元の手と握った手を入れ替える（applyGrip。kit の骨付きの部分は節点の大きさでは消えないので visible で）
    fn apply_grip(&mut self, kit: &mut Kit, grip: bool) {
        if let (Some(n), Some((ci, c))) = (self.native, self.closed) {
            kit.scene[self.live[n].node].visible = !grip;
            kit.scene[c].scale = if grip { Vec3::ONE } else { Vec3::ZERO };
            self.parts[ci].local = Mat4::from_scale(if grip { Vec3::ONE } else { Vec3::ZERO });
        }
    }

    fn write_pose(&self, kit: &mut Kit) {
        for (i, node) in self.kit_nodes.iter().enumerate() {
            let Some(node) = *node else { continue };
            let n = &mut kit.scene[node];
            n.position = self.skel.t[i];
            n.rotation = Rot::Quat(self.skel.r[i]);
            n.scale = self.skel.s[i];
        }
        kit.scene[self.weapon.root].scale = self.skel.s[self.weapon.skel_index];
    }

    // ---- 測り（measureOriginalCameraBounds） ----

    /// 今の姿の描く頂点の箱。返り値: (体, 頭, 武器を除いた体)。grip: 手の入れ替えを決めて測る（構えの包み）
    fn bounds(&self, kit: &Kit, skel: &Skeleton, grip: Option<bool>) -> [(Vec3, Vec3); 3] {
        let empty = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        let mut out = [empty; 3];
        let add = |b: &mut (Vec3, Vec3), p: Vec3| { b.0 = b.0.min(p); b.1 = b.1.max(p); };
        let head = self.head;
        let within = |mut i: usize, top: usize| loop { if i == top { break true; } match skel.parent[i] { Some(p) => i = p, None => break false } };
        let g = &self.g;
        for (k, part) in self.parts.iter().enumerate() {
            let m = &self.mats[part.mat];
            if m.transparent && m.opacity <= 0.0 { continue; }
            let shown = match part.kind {
                PartKind::NativeHand => grip.map_or(kit.scene[self.live[k].node].visible, |g| !g),
                PartKind::ClosedHand => grip.unwrap_or(part.local.determinant().abs() >= 1e-12),
                PartKind::Glb => true,
            };
            if !shown { continue; }
            let weights = &self.live[k].weights;
            let mut used = vec![false; part.p.positions.len()];
            for i in &part.p.indices { if let Some(u) = used.get_mut(*i as usize) { *u = true; } }
            match part.skin {
                Some(s) => {
                    let skin = &g.skins[s];
                    let bones: Vec<Mat4> = skin.joints.iter().zip(&skin.inverse_bind).map(|(j, ib)| skel.world[*j] * *ib).collect();
                    let head_bones: Vec<bool> = skin.joints.iter().map(|j| head.is_some_and(|h| within(*j, h))).collect();
                    for v in 0..part.p.positions.len() {
                        if !used[v] { continue; }
                        let mut q = Vec3::from(part.p.positions[v]);
                        for (t, w) in weights.iter().enumerate() { if *w != 0.0 && let Some(d) = part.p.morph_pos.get(t) { q += Vec3::from(d[v]) * *w; } }
                        let (j, w) = (part.p.joints[v], part.weights_n[v]);
                        let mut o = Vec3::ZERO; let mut hw = 0.0;
                        for c in 0..4 { if w[c] != 0.0 { o += bones[j[c] as usize].transform_point3(q) * w[c]; if head_bones[j[c] as usize] { hw += w[c]; } } }
                        if !o.is_finite() { continue; }
                        add(&mut out[0], o); add(&mut out[2], o);
                        if hw >= 0.25 { add(&mut out[1], o); }
                    }
                }
                None => {
                    let local = if part.kind == PartKind::ClosedHand { Mat4::from_scale(Vec3::splat(if grip.unwrap_or(true) { 1.0 } else { 0.0 })) } else { part.local };
                    let mw = skel.world[part.node] * local;
                    if mw.determinant().abs() < 1e-12 { continue; }
                    let on_head = head.is_some_and(|h| within(part.node, h));
                    for v in 0..part.p.positions.len() {
                        if !used[v] { continue; }
                        let o = mw.transform_point3(Vec3::from(part.p.positions[v]));
                        if !o.is_finite() { continue; }
                        add(&mut out[0], o); add(&mut out[2], o);
                        if on_head { add(&mut out[1], o); }
                    }
                }
            }
        }
        // ＞＜ の板（見せている間だけ）
        if let (Some(p), Some(h)) = (&self.plates, head) && p.weight > 0.01 {
            let mw = skel.world[h];
            for v in &p.local { let o = mw.transform_point3(Vec3::from(*v)); add(&mut out[0], o); add(&mut out[2], o); add(&mut out[1], o); }
        }
        // 登録された武器（顔の表示では隠す）
        let w = &self.weapon;
        if w.content.is_some_and(|c| kit.scene[c].visible) {
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

    fn posed(&self) -> Skeleton { let mut s = self.skel.clone(); s.update_world(self.base()); s }

    /// 構えの包み（measureOriginalArmedCameraBounds: 動作を 12 Hz で通して測る。今の大きさのまま）
    fn armed(&self, kit: &Kit, names: &[String]) -> Value {
        let mut skel = self.skel.clone();
        let mut mixer = Mixer::new();
        let ids: Vec<ActionId> = names.iter().filter_map(|n| self.clips.iter().find(|(a, _)| a == n).map(|(_, id)| mixer.add(self.mixer.actions[*id].clip.clone()))).collect();
        if ids.len() != names.len() { return Value::Null; }
        let (mut lo, mut hi) = (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY));
        let (mut blo, mut bhi) = (lo, hi);
        let base = self.base();
        let grip = if self.closed.is_some() { Some(true) } else { None };
        for id in ids {
            mixer.stop_all(&mut skel);
            mixer.reset(id);
            mixer.actions[id].once = true; mixer.actions[id].clamp_when_finished = true;
            mixer.play(id, &skel);
            let d = mixer.actions[id].clip.duration as f64;
            let steps = (d * 12.0).ceil() as usize;
            for k in 0..=steps {
                mixer.actions[id].time = d * k as f64 / steps.max(1) as f64;
                mixer.advance(0.0, &mut skel);
                skel.update_world(base);
                let [body, _, bare] = self.bounds(kit, &skel, grip);
                if body.0.x <= body.1.x { lo = lo.min(body.0); hi = hi.max(body.1); }
                if bare.0.x <= bare.1.x { blo = blo.min(bare.0); bhi = bhi.max(bare.1); }
            }
        }
        if lo.x > hi.x { return Value::Null; }
        json!({ "min": lo.to_array(), "max": hi.to_array(), "bodyMin": blo.to_array(), "bodyMax": bhi.to_array() })
    }

    fn info_json(&self, kit: &Kit) -> Value {
        let [body, _, bare] = self.bounds(kit, &self.posed(), None);
        let unarmed = if bare.0.x <= bare.1.x { bare } else { body };
        json!({ "hasHead": self.head.is_some(), "restHeight": (unarmed.1.y - unarmed.0.y) / self.size.max(1e-3), "hurtSkin": self.hurt_skin, "hasHurt": self.plates.is_some() })
    }

    pub fn remove_all(self, kit: &mut Kit) {
        for s in &self.skeletons { kit.scene.remove_skeleton(*s); }
        for l in &self.live { kit.destroy_geometry(l.geo); }
        for l in &self.lights { kit.scene.remove(*l); }
        kit.scene.remove(self.root);
    }
}

/// 1 回で止まる動作か（振ると、ゲーム用の 1 回の動作。名前の正本は lib/character-maker/actions.ts。終わると motionComplete を出す）
fn plays_once(motion: &str) -> bool { ["sword_attack", "jump", "dodge", "skill", "punch", "interact", "down", "restDown", "restUp", "banzai", "greet", "spinKick"].contains(&motion) }

/// glTF の材質の色の絵のサンプラー（GLTFLoader: 無ければ繰り返し・LinearMipmapLinear・Linear）
fn sampler_opts(json: &Value, material: usize) -> island_kit::texture::TexOpts {
    use island_kit::texture::{TexOpts, Wrap};
    let tex = json["materials"][material]["pbrMetallicRoughness"]["baseColorTexture"]["index"].as_u64();
    let sampler = tex.and_then(|t| json["textures"][t as usize]["sampler"].as_u64()).map(|s| &json["samplers"][s as usize]);
    let wrap = |v: Option<u64>| match v { Some(33071) => Wrap::Clamp, Some(33648) => Wrap::Mirror, _ => Wrap::Repeat };
    let min = sampler.and_then(|s| s["minFilter"].as_u64());
    let mag = sampler.and_then(|s| s["magFilter"].as_u64());
    TexOpts {
        wrap_s: wrap(sampler.and_then(|s| s["wrapS"].as_u64())), wrap_t: wrap(sampler.and_then(|s| s["wrapT"].as_u64())),
        mag_nearest: mag == Some(9728), min_nearest: min == Some(9728), mipmap: !matches!(min, Some(9728) | Some(9729)), anisotropy: 1,
    }
}

/// 部分の独自の頂点の値（名前が _ で始まる物。生のバイト列が届いているときだけ）
fn custom_attributes(g: &Glb, raw: Option<&[u8]>, mesh: usize, prim: usize) -> Vec<(String, Vec<f32>, usize)> {
    let Some(raw) = raw else { return Vec::new() };
    let Some(attrs) = g.json["meshes"][mesh]["primitives"][prim]["attributes"].as_object() else { return Vec::new() };
    attrs.iter().filter(|(k, _)| k.starts_with('_')).filter_map(|(k, v)| {
        let idx = v.as_u64()? as usize;
        let size = match g.json["accessors"][idx]["type"].as_str()? { "VEC2" => 2, "VEC3" => 3, "VEC4" => 4, _ => 1 };
        Some((k.clone(), edit::raw_accessor(raw, &g.json, idx)?, size))
    }).collect()
}

fn geometry_of(p: &glb::Primitive) -> Geometry {
    Geometry { positions: p.positions.clone(), normals: p.normals.clone(), uvs: p.uvs.clone(), colors: p.colors.clone(), sway: Vec::new(), indices: if p.indices.is_empty() { None } else { Some(p.indices.clone()) }, groups: Vec::new() }
}

/// 髪（外す部分・顔の画像の手直し・髪の色の絵）。edit::hair を、作った画像の控えを通して呼ぶ
fn hair_textures(parts: &mut Vec<Part>, mats: &mut [MatInfo], g: &Glb, character: &str, hairstyle: &str, hair_color: &str) -> Result<(), String> {
    edit::hair(parts, mats, g, character, hairstyle, hair_color)
}

fn box_json(b: (Vec3, Vec3)) -> Value { if b.0.x > b.1.x { Value::Null } else { json!([b.0.to_array(), b.1.to_array()]) } }

impl Actor for Original {
    fn info(&self, kit: &Kit) -> Value { self.info_json(kit) }
    fn apply(&mut self, kit: &mut Kit, config: &Value) { self.apply_config(kit, config); }
    /// 合図: { speed, playing, motion, attackRevision, resume, seek, hit, hurtFace, weaponVisible }
    fn signal(&mut self, kit: &mut Kit, v: &Value) {
        if let Some(s) = v["speed"].as_f64() { self.speed = s; }
        let mut playing_changed = false;
        if let Some(p) = v["playing"].as_bool() && p != self.playing { self.playing = p; playing_changed = true; }
        let mut motion_ran = false;
        if let Some(m) = v["motion"].as_str() {
            let m = m.to_string();
            self.set_motion(kit, &m, v["attackRevision"].as_i64().unwrap_or(0), v["resume"].as_f64());
            motion_ran = true;
        }
        if let Some(s) = v["seek"].as_f64() { self.seek(s); }
        if playing_changed || motion_ran { self.playing_effect(); }
        if v["hit"].as_bool() == Some(true) { self.start_hit(); }
        if let Some(on) = v["hurtFace"].as_bool() { self.hurt_prop = on; }
        if let Some(on) = v["weaponVisible"].as_bool() && let Some(c) = self.weapon.content { kit.scene[c].visible = on; }
    }
    fn step(&mut self, kit: &mut Kit, dt: f64) { self.frame(kit, dt); }
    fn query(&self, kit: &Kit, v: &Value) -> Value {
        match v["q"].as_str().unwrap_or("") {
            "bounds" => { let [a, b, c] = self.bounds(kit, &self.posed(), None); json!({ "body": box_json(a), "head": box_json(b), "unarmed": box_json(c) }) }
            "armed" => self.armed(kit, &v["names"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect::<Vec<_>>()),
            "head" => self.head.map_or(Value::Null, |h| json!(self.posed().world[h].to_cols_array())),
            // 床は Hips・J_Bip_C_Hips を追う（オリジナルの腰は Hip なので追わない。今の版の StageFloor と同じ）
            "bone" => { let i = if v["name"].as_str() == Some("head") { self.head } else { self.skel.find("Hips").or_else(|| self.skel.find("J_Bip_C_Hips")) }; i.map_or(Value::Null, |i| json!(self.posed().world[i].w_axis.truncate().to_array())) }
            _ => Value::Null,
        }
    }
    fn take_events(&mut self) -> Vec<Value> { std::mem::take(&mut self.events) }
    fn debug(&self, kit: &Kit) -> Value {
        let morphs: Vec<Value> = self.parts.iter().zip(&self.live).filter(|(p, _)| !p.targets.is_empty()).map(|(p, l)| {
            let on: serde_json::Map<String, Value> = p.targets.iter().zip(&l.weights).filter(|(_, w)| **w != 0.0).map(|(n, w)| (n.clone(), json!(w))).collect();
            json!({ "name": p.name, "on": on, "kit": kit.morph_weights(l.node).len() })
        }).collect();
        let posed = self.posed();
        let probe: Vec<Value> = self.parts.iter().map(|p| {
            let nan = p.p.positions.iter().filter(|v| !v[0].is_finite() || !v[1].is_finite() || !v[2].is_finite()).count();
            let body_nan = p.body.as_ref().map_or(0, |(_, d)| d.iter().filter(|v| !v[0].is_finite() || !v[1].is_finite() || !v[2].is_finite()).count());
            let body_max = p.body.as_ref().map_or(0.0, |(_, d)| d.iter().map(|v| Vec3::from(*v).length()).fold(0.0f32, f32::max));
            json!({ "name": p.name, "n": p.p.positions.len(), "idx": p.p.indices.len(), "nan": nan, "bodyNan": body_nan, "bodyMax": body_max, "build": self.build, "first": p.p.positions.first(), "joints": p.p.joints.len(), "w": p.weights_n.len(), "skin": p.skin })
        }).collect();
        let [a, b, c] = self.bounds(kit, &posed, None);
        let hip = self.skel.find("Hip").map(|i| posed.world[i].to_cols_array());
        let grip = kit.scene[self.weapon.root].children().first().map(|g| kit.scene[*g].scale.to_array());
        json!({ "probe": probe, "bounds": [box_json(a), box_json(b), box_json(c)], "hip": hip, "grip": grip, "size": self.size, "base": self.base().to_cols_array(),
            "parts": self.parts.iter().map(|p| format!("{:?}:{}", p.kind, p.name)).collect::<Vec<_>>(), "morphs": morphs, "clips": self.clips.iter().map(|c| c.0.clone()).collect::<Vec<_>>(),
            "active": self.active.map(|a| json!({ "time": self.mixer.actions[a].time, "paused": self.mixer.actions[a].paused })), "hit": self.hit.is_some(), "motion": self.motion, "elapsed": self.elapsed })
    }
    /// GLB の書き出し（exportOriginalCharacterGlb）。v: { variant: 'armed'|'standard', extras（characterMaker の値）, swordExtras }
    fn export(&mut self, kit: &mut Kit, v: &Value) -> Result<Vec<u8>, String> { export_glb::export(self, kit, v) }
    fn look(&self, _kit: &Kit) -> crate::look::Subject {
        // 光る画面（ルクスの顔）には陰影を付けない
        let flat = self.live.iter().zip(&self.parts).filter(|(_, p)| self.mats.get(p.mat).is_some_and(|m| m.name.starts_with("Lux display"))).map(|(l, _)| l.mat).collect();
        // ルクスは頭の球を使わない（白い外装の面の向きを、そのまま陰に出す。顔の画面は flat）
        let head = if self.character == "generic-robot" { None } else { self.head.map(|h| self.posed().world[h]) };
        crate::look::Subject { root: self.root, head, flat }
    }
    fn head_box(&self, kit: &Kit) -> Option<(Vec3, Vec3)> { let [_, head, _] = self.bounds(kit, &self.posed(), None); Some(head) }
    fn remove(self: Box<Self>, kit: &mut Kit) { self.remove_all(kit); }
}

