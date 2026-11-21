// 材質の設定（three の Material の役）。GPU の側は render/。値を変えるときは kit.mat_mut(id)。式は shaders/kit.wgsl。
use crate::math::Color;
use glam::{Mat3, Vec2};

/// 形（kit.geometry が返す）
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GeoId(pub u32);
/// 材質（kit.material が返す。番号は作った順で、不透明の並びに使う。three の material.id）
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct MatId(pub u32);
/// テクスチャ（kit の canvas_texture など）
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TexId(pub u32);

/// 面（three の FrontSide・BackSide・DoubleSide）
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Side { #[default] Front, Back, Double }

/// ブレンド（three の NormalBlending など）。Custom の係数はアルファにも同じものを使う（three の blendSrcAlpha が null のとき）
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Blending {
    /// transparent でなければブレンドしない
    #[default]
    Normal,
    Additive,
    Multiply,
    Custom { src: wgpu::BlendFactor, dst: wgpu::BlendFactor },
    None,
}

/// map の変換（three の texture.repeat・offset・rotation・center。three ではテクスチャの値だが、kit では材質の値）
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct UvTransform { pub repeat: Vec2, pub offset: Vec2, pub rotation: f32, pub center: Vec2 }
impl Default for UvTransform {
    fn default() -> Self { UvTransform { repeat: Vec2::ONE, offset: Vec2::ZERO, rotation: 0.0, center: Vec2::ZERO } }
}
impl UvTransform {
    /// three の Matrix3.setUvTransform
    pub fn matrix(&self) -> Mat3 {
        let (c, s) = (self.rotation.cos(), self.rotation.sin());
        let (sx, sy) = (self.repeat.x, self.repeat.y);
        let (cx, cy) = (self.center.x, self.center.y);
        Mat3::from_cols_array(&[
            sx * c, -sy * s, 0.0,
            sx * s, sy * c, 0.0,
            -sx * (c * cx + s * cy) + cx + self.offset.x, -sy * (-s * cx + c * cy) + cy + self.offset.y, 1.0,
        ])
    }
}

/// ゲームが書くシェーダー（three の ShaderMaterial）。先頭に KIT_COMMON が付く。書き方は README
#[derive(Clone, Debug)]
pub struct CustomShader {
    pub label: String,
    pub wgsl: String,
    pub vs: String,
    pub fs: String,
    /// group 2 binding 0 の中身（最大 256 バイト）。毎コマ変えるなら kit.set_uniform
    pub uniform: Vec<u8>,
    /// tex0..tex5（6 枚まで）
    pub textures: Vec<TexId>,
    /// インスタンスごとの属性の形式（three の InstancedBufferAttribute）。@location は 13, 14, 15, 5, 6, 7 の順
    pub instance_attrs: Vec<wgpu::VertexFormat>,
    /// Points の点ごとの属性の形式。@location は 13, 14, 15, 5, 6, 7 の順（位置は @location(0)）
    pub point_attrs: Vec<wgpu::VertexFormat>,
}
impl Default for CustomShader {
    fn default() -> Self { CustomShader { label: "custom".into(), wgsl: String::new(), vs: "vs".into(), fs: "fs".into(), uniform: Vec::new(), textures: Vec::new(), instance_attrs: Vec::new(), point_attrs: Vec::new() } }
}

#[derive(Clone, Copy, Debug)]
pub struct Lambert { pub emissive: Color, pub emissive_intensity: f32, pub flat_shading: bool }
impl Default for Lambert { fn default() -> Self { Lambert { emissive: Color::BLACK, emissive_intensity: 1.0, flat_shading: false } } }

/// pengo の softMaterial（Lambert の回り込み＋輪郭の光＋草木の揺れ＋灯りの戻り）
#[derive(Clone, Copy, Debug)]
pub struct Soft { pub emissive: Color, pub emissive_intensity: f32, pub rim: f32, pub sway: bool, pub flat_shading: bool }
impl Default for Soft { fn default() -> Self { Soft { emissive: Color::BLACK, emissive_intensity: 1.0, rim: 1.0, sway: false, flat_shading: false } } }

#[derive(Clone, Copy, Debug)]
pub struct Phong { pub emissive: Color, pub emissive_intensity: f32, pub specular: Color, pub shininess: f32, pub flat_shading: bool }
impl Default for Phong { fn default() -> Self { Phong { emissive: Color::BLACK, emissive_intensity: 1.0, specular: Color::hex(0x111111), shininess: 30.0, flat_shading: false } } }

#[derive(Clone, Copy, Debug)]
pub struct Standard { pub emissive: Color, pub emissive_intensity: f32, pub metalness: f32, pub roughness: f32, pub flat_shading: bool }
impl Default for Standard { fn default() -> Self { Standard { emissive: Color::BLACK, emissive_intensity: 1.0, metalness: 0.0, roughness: 1.0, flat_shading: false } } }

/// three の MeshPhysicalMaterial（使う項目だけ: 光沢のコート・つや・環境の映り込み）。式は kit.wgsl の `MODE_PHYSICAL`。
/// three の scene.environment と environmentIntensity は `env`（kit.room_environment が返す的）と `env_intensity`
#[derive(Clone, Copy, Debug)]
pub struct Physical {
    pub emissive: Color, pub emissive_intensity: f32, pub metalness: f32, pub roughness: f32,
    pub clearcoat: f32, pub clearcoat_roughness: f32,
    pub sheen: f32, pub sheen_color: Color, pub sheen_roughness: f32,
    pub env: Option<TexId>, pub env_intensity: f32, pub flat_shading: bool,
}
impl Default for Physical {
    fn default() -> Self { Physical { emissive: Color::BLACK, emissive_intensity: 1.0, metalness: 0.0, roughness: 1.0, clearcoat: 0.0, clearcoat_roughness: 0.0, sheen: 0.0, sheen_color: Color::BLACK, sheen_roughness: 1.0, env: None, env_intensity: 1.0, flat_shading: false } }
}

#[derive(Clone, Debug)]
pub enum MatKind {
    Basic,
    Lambert(Lambert),
    Soft(Soft),
    Phong(Phong),
    Standard(Standard),
    Physical(Physical),
    Sprite { rotation: f32, size_attenuation: bool },
    Points { size: f32, size_attenuation: bool },
    LineBasic,
    Custom(CustomShader),
}

#[derive(Clone, Debug)]
pub struct Material {
    pub kind: MatKind,
    pub color: Color,
    pub opacity: f32,
    pub map: Option<TexId>,
    /// three の emissiveMap（Lambert・Soft・Phong・Standard。map と同じ UV。tex1 を使う）
    pub emissive_map: Option<TexId>,
    pub transparent: bool,
    pub depth_write: bool,
    pub depth_test: bool,
    pub depth_func: wgpu::CompareFunction,
    pub side: Side,
    pub blending: Blending,
    pub premultiplied_alpha: bool,
    /// (factor, units)
    pub polygon_offset: Option<(f32, f32)>,
    pub fog: bool,
    pub vertex_colors: bool,
    /// GLTFLoader が作った材質か（glb::standard_material）。three と同じく、COLOR_0 のある部分には vertexColors を入れる（glb::primitive_material）。写しても残る
    pub gltf_loader: bool,
    pub alpha_test: f32,
    pub visible: bool,
    pub color_write: bool,
    pub uv: UvTransform,
    /// three の toneMapped（既定 true。kit.output が DirectAces のときだけ効く）
    pub tone_mapped: bool,
    /// three の bumpMap（Standard だけ。map と同じ UV。tex1 を使うので emissive_map とは同時に使えない）
    pub bump_map: Option<TexId>,
    pub bump_scale: f32,
    /// 面の向きで決まる縁の光（Standard だけ。ゲームの onBeforeCompile で totalEmissiveRadiance へ足す式。色 × 強さ）。カメラから見た向きで計算する
    pub view_rim: Option<(Color, f32)>,
}

impl Material {
    pub fn new(kind: MatKind) -> Material {
        Material {
            kind, color: Color::WHITE, opacity: 1.0, map: None, emissive_map: None, transparent: false, depth_write: true, depth_test: true, depth_func: wgpu::CompareFunction::LessEqual,
            side: Side::Front, blending: Blending::Normal, premultiplied_alpha: false, polygon_offset: None, fog: true, vertex_colors: false, gltf_loader: false, alpha_test: 0.0, visible: true, color_write: true, uv: UvTransform::default(), tone_mapped: true, bump_map: None, bump_scale: 1.0, view_rim: None,
        }
    }
    pub fn basic() -> Material { Material::new(MatKind::Basic) }
    pub fn lambert() -> Material { Material::new(MatKind::Lambert(Lambert::default())) }
    pub fn soft() -> Material { Material::new(MatKind::Soft(Soft::default())) }
    pub fn phong() -> Material { Material::new(MatKind::Phong(Phong::default())) }
    pub fn standard() -> Material { Material::new(MatKind::Standard(Standard::default())) }
    pub fn physical(p: Physical) -> Material { Material::new(MatKind::Physical(p)) }
    /// three の SpriteMaterial（transparent が既定で true）
    pub fn sprite() -> Material { Material { transparent: true, ..Material::new(MatKind::Sprite { rotation: 0.0, size_attenuation: true }) } }
    /// three の PointsMaterial
    pub fn points(size: f32) -> Material { Material::new(MatKind::Points { size, size_attenuation: true }) }
    /// three の LineBasicMaterial
    pub fn line_basic() -> Material { Material::new(MatKind::LineBasic) }
    /// three の ShaderMaterial（fog は式を自分で書く）
    pub fn custom(c: CustomShader) -> Material { Material { fog: false, ..Material::new(MatKind::Custom(c)) } }

    /// 鎖で書く手短な設定（`Material::soft().with_color(c)`）
    pub fn with_color(mut self, c: Color) -> Material { self.color = c; self }
    pub fn with_map(mut self, t: TexId) -> Material { self.map = Some(t); self }
    pub fn with_opacity(mut self, o: f32) -> Material { self.opacity = o; self }
    pub fn with_side(mut self, s: Side) -> Material { self.side = s; self }
    /// transparent: true・depth_write: false（よく使う組）
    pub fn see_through(mut self) -> Material { self.transparent = true; self.depth_write = false; self }
    pub fn additive(mut self) -> Material { self.blending = Blending::Additive; self.transparent = true; self.depth_write = false; self }

    pub(crate) fn custom_shader(&self) -> Option<&CustomShader> { if let MatKind::Custom(c) = &self.kind { Some(c) } else { None } }
    /// three の OPAQUE の定義
    pub(crate) fn opaque_define(&self) -> bool { !self.transparent && self.blending == Blending::Normal }
    /// 使うテクスチャ（tex0..tex5。Custom だけが tex4・tex5 を使う）
    pub(crate) fn tex_slots(&self) -> [Option<TexId>; 6] {
        match &self.kind {
            MatKind::Custom(c) => [c.textures.first().copied(), c.textures.get(1).copied(), c.textures.get(2).copied(), c.textures.get(3).copied(), c.textures.get(4).copied(), c.textures.get(5).copied()],
            // 環境の映り込みは tex1
            MatKind::Physical(p) => [self.map, p.env, None, None, None, None],
            _ => [self.map, self.emissive_map.or(self.bump_map), None, None, None, None],
        }
    }
    /// three の setBlending（色とアルファの係数）
    pub(crate) fn blend_state(&self) -> Option<wgpu::BlendState> {
        use wgpu::BlendFactor as F;
        let mk = |s, d, sa, da| Some(wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: s, dst_factor: d, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: sa, dst_factor: da, operation: wgpu::BlendOperation::Add },
        });
        let pre = self.premultiplied_alpha;
        match self.blending {
            Blending::None => None,
            Blending::Normal if !self.transparent => None,
            Blending::Normal => mk(if pre { F::One } else { F::SrcAlpha }, F::OneMinusSrcAlpha, F::One, F::OneMinusSrcAlpha),
            Blending::Additive => mk(if pre { F::One } else { F::SrcAlpha }, F::One, F::One, F::One),
            Blending::Multiply => mk(F::Dst, F::OneMinusSrcAlpha, F::Zero, F::One),
            Blending::Custom { src, dst } => mk(src, dst, src, dst),
        }
    }
}

// フラグ（kit.wgsl の MatU.flags.x と同じ）
pub(crate) const F_VERTEX_COLORS: u32 = 1;
pub(crate) const F_OPAQUE: u32 = 2;
pub(crate) const F_FOG: u32 = 4;
pub(crate) const F_PREMULT: u32 = 8;
pub(crate) const F_FLAT: u32 = 16;
pub(crate) const F_DOUBLE: u32 = 32;
pub(crate) const F_BACK: u32 = 64;
pub(crate) const F_SWAY: u32 = 128;
pub(crate) const F_ATTENUATE: u32 = 256;
pub(crate) const F_ENV: u32 = 512;
pub(crate) const F_EMAP: u32 = 1024;
pub(crate) const F_NOTONE: u32 = 2048;
pub(crate) const F_VIEW_RIM: u32 = 4096;
pub(crate) const F_BUMP: u32 = 8192;

/// 材質の値の 1 枠（kit.wgsl の MatU と同じ並び）
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct MatU {
    pub color: [f32; 4],
    pub emissive: [f32; 4],
    pub specular: [f32; 4],
    pub p: [f32; 4],
    pub uv0: [f32; 4],
    pub uv1: [f32; 4],
    pub flags: [u32; 4],
}

impl Material {
    /// 組み込みの材質の値（flip_v は描いた的を貼るとき）
    pub(crate) fn mat_u(&self, flip_v: bool) -> MatU {
        let mut flags = 0;
        if self.vertex_colors { flags |= F_VERTEX_COLORS; }
        if self.opaque_define() { flags |= F_OPAQUE; }
        if self.fog { flags |= F_FOG; }
        if self.premultiplied_alpha { flags |= F_PREMULT; }
        if self.emissive_map.is_some() { flags |= F_EMAP; }
        if !self.tone_mapped { flags |= F_NOTONE; }
        match self.side { Side::Double => flags |= F_DOUBLE, Side::Back => flags |= F_BACK, Side::Front => {} }
        let (mut emissive, mut specular, mut p) = ([0.0, 0.0, 0.0], [0.0; 4], [0.0f32; 4]);
        let mut uv0w = 0.0;
        let em = |c: Color, i: f32| [c.r * i, c.g * i, c.b * i];
        match &self.kind {
            MatKind::Lambert(l) => { emissive = em(l.emissive, l.emissive_intensity); if l.flat_shading { flags |= F_FLAT; } }
            MatKind::Soft(s) => {
                emissive = em(s.emissive, s.emissive_intensity);
                p[2] = s.rim;
                if s.flat_shading { flags |= F_FLAT; }
                if s.sway { flags |= F_SWAY; }
            }
            MatKind::Phong(ph) => {
                emissive = em(ph.emissive, ph.emissive_intensity);
                specular = [ph.specular.r, ph.specular.g, ph.specular.b, ph.shininess];
                if ph.flat_shading { flags |= F_FLAT; }
            }
            MatKind::Standard(st) => {
                emissive = em(st.emissive, st.emissive_intensity);
                p[0] = st.metalness;
                p[1] = st.roughness;
                if st.flat_shading { flags |= F_FLAT; }
                if self.bump_map.is_some() { flags |= F_BUMP; p[2] = self.bump_scale; }
                if let Some((c, k)) = self.view_rim { flags |= F_VIEW_RIM; specular = [c.r * k, c.g * k, c.b * k, 0.0]; }
            }
            MatKind::Physical(ph) => {
                // specular = つやの色（sheen × sheenColor）と環境の強さ、p = metalness・roughness・clearcoat・clearcoatRoughness、uv0.w = sheenRoughness
                emissive = em(ph.emissive, ph.emissive_intensity);
                specular = [ph.sheen_color.r * ph.sheen, ph.sheen_color.g * ph.sheen, ph.sheen_color.b * ph.sheen, ph.env_intensity];
                p = [ph.metalness, ph.roughness, ph.clearcoat, ph.clearcoat_roughness];
                uv0w = ph.sheen_roughness;
                if ph.flat_shading { flags |= F_FLAT; }
                if ph.env.is_some() { flags |= F_ENV; }
            }
            MatKind::Sprite { rotation, size_attenuation } => { p[3] = *rotation; if *size_attenuation { flags |= F_ATTENUATE; } }
            MatKind::Points { size, size_attenuation } => { p[3] = *size; if *size_attenuation { flags |= F_ATTENUATE; } }
            MatKind::Basic => { if let Some((c, k)) = self.view_rim { flags |= F_VIEW_RIM; specular = [c.r * k, c.g * k, c.b * k, 0.0]; } }
            MatKind::LineBasic | MatKind::Custom(_) => {}
        }
        let mut m = self.uv.matrix();
        // 描いた的は上下が逆に入る（three の RT は下が v = 0）
        if flip_v { m = Mat3::from_cols_array(&[1.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, 1.0]) * m; }
        let (c0, c1, c2) = (m.x_axis, m.y_axis, m.z_axis);
        MatU {
            color: [self.color.r, self.color.g, self.color.b, self.opacity],
            emissive: [emissive[0], emissive[1], emissive[2], self.alpha_test],
            specular, p,
            uv0: [c0.x, c1.x, c2.x, uv0w],
            uv1: [c0.y, c1.y, c2.y, 0.0],
            flags: [flags, 0, 0, 0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// three の Matrix3.setUvTransform と同じ
    #[test]
    fn uv_transform_matches_three() {
        let t = UvTransform { repeat: Vec2::new(2.0, 3.0), offset: Vec2::new(0.25, 0.5), rotation: 0.5, center: Vec2::new(0.5, 0.5) };
        let m = t.matrix();
        let (c, s) = (0.5f32.cos(), 0.5f32.sin());
        let u = 0.1 * (2.0 * c) + 0.2 * (2.0 * s) + (-2.0 * (c * 0.5 + s * 0.5) + 0.5 + 0.25);
        let v = 0.1 * (-3.0 * s) + 0.2 * (3.0 * c) + (-3.0 * (-s * 0.5 + c * 0.5) + 0.5 + 0.5);
        let r = m * glam::Vec3::new(0.1, 0.2, 1.0);
        assert!((r.x - u).abs() < 1e-5 && (r.y - v).abs() < 1e-5);
    }

    #[test]
    fn blend_follows_three() {
        assert!(Material::basic().blend_state().is_none());
        let n = Material::basic().see_through().blend_state().unwrap();
        assert_eq!(n.color.src_factor, wgpu::BlendFactor::SrcAlpha);
        let a = Material::basic().additive().blend_state().unwrap();
        assert_eq!((a.color.src_factor, a.color.dst_factor, a.alpha.src_factor), (wgpu::BlendFactor::SrcAlpha, wgpu::BlendFactor::One, wgpu::BlendFactor::One));
        let mut m = Material::basic().see_through();
        m.blending = Blending::Multiply;
        m.premultiplied_alpha = true;
        let b = m.blend_state().unwrap();
        assert_eq!((b.color.src_factor, b.alpha.src_factor, b.alpha.dst_factor), (wgpu::BlendFactor::Dst, wgpu::BlendFactor::Zero, wgpu::BlendFactor::One));
        assert!(!m.opaque_define());
    }
}
