// 主役の見た目（GPU の側）: 骨の行列・モーフの重みを GPU へ渡し、光なしの材質に輪郭の光・闘気を重ねて描く。右手に持たせる武器の模型と ＞＜ の板もここ。
// 主役は character-actor の CharacterActor（動かすのはゲームの合図）。色の値・的の形式・武器の出どころはゲームが渡す。
use crate::{BLEND_NORMAL, GpuMips, Vertex, make_texture, shader, skin_layout, tex_layout, vert_layout};
use bytemuck::{Pod, Zeroable};
use character_actor::actor::{CharacterActor, SkinVertex};
use character_actor::glb::Glb;
use glam::Mat4;
use std::cell::Cell;
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;

#[path = "opaque_batch.rs"]
mod opaque_batch;
pub use opaque_batch::ActorOpaqueBatch;

const HERO_COMMON: &str = include_str!("../shaders/hero_common.wgsl");
fn hero_source(entry: &str) -> String { format!("{HERO_COMMON}\n{entry}") }

/// 描く的の形式（ゲームが決める）。color と samples は主の的、mask は自機だけの型の的（1 サンプル）。depth はどの的も同じ。影の型の的は色なし・1 サンプル
#[derive(Clone, Copy)]
pub struct ActorTargets {
    pub color: wgpu::TextureFormat,
    pub depth: wgpu::TextureFormat,
    pub mask: wgpu::TextureFormat,
    pub samples: u32,
}

/// 見せ方の値（ゲームが決める。色は線形）
#[derive(Clone, Copy)]
pub struct ActorLook {
    /// 輪郭の光の色と強さ
    pub rim: [f32; 3],
    pub rim_strength: f32,
    /// 闘気の色と強さ（縁から燃える）
    pub aura: [f32; 3],
    pub aura_strength: f32,
    /// 上から当たる光の強さ
    pub top: f32,
    /// 暗い色を持ち上げる量
    pub lift: f32,
    /// 被弾の赤い光（0..1）
    pub flash: f32,
    /// 物陰の影絵の色と不透明度
    pub hidden: [f32; 3],
    pub hidden_opacity: f32,
}

/// 輪郭の光の式の違い（ゲームごとの addRim）。既定は muso とひな形・keiba の式
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorStyle {
    /// 全体を一緒に持ち上げる（pengo の litC = c * (1 + lift * 0.55)）か、暗い色だけ持ち上げる（lift * 2.2 * (1 - smoothstep)）か
    pub flat_lift: bool,
    /// rimK の指数
    pub rim_pow: f32,
    /// 鮮やかさ（1 + lift * sat）
    pub sat: f32,
    /// 上の面が自分の色も明るくする量（pengo の outgoingLight * topK * uTopI * 0.6）
    pub top_self: f32,
}
impl ActorStyle {
    /// muso・ひな形・keiba
    pub const MUSO: ActorStyle = ActorStyle { flat_lift: false, rim_pow: 3.0, sat: 0.45, top_self: 0.0 };
    /// pengo・digdag・iceclimber・rainbow・bubble2
    pub const PENGO: ActorStyle = ActorStyle { flat_lift: true, rim_pow: 3.4, sat: 0.45, top_self: 0.6 };
    /// pinball
    pub const PINBALL: ActorStyle = ActorStyle { flat_lift: true, rim_pow: 3.4, sat: 0.4, top_self: 0.0 };
}
impl Default for ActorStyle { fn default() -> Self { ActorStyle::MUSO } }

/// 輪郭の光（pengo の主役の材質の改造を移植）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct HeroU { rim_p: [f32; 4], aura: [f32; 4], misc: [f32; 4], hidden: [f32; 4], style: [f32; 4], nfix: [[f32; 4]; 3], sun: [f32; 4] }

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
struct DrawU { color: [f32; 4], p: [f32; 4], e: [f32; 4], morph: [u32; 4], info: [u32; 4], _pad: [f32; 12] }

/// 描く 1 つ（three の Mesh 1 つ）
#[derive(Clone)]
struct DrawItem {
    prim: Option<usize>,
    istart: u32,
    icount: u32,
    base_vertex: i32,
    offset: u32,
    tex: usize,
    double: bool,
    transparent: bool,
    alpha_shadow: bool,
    /// 材質の色（a は opacity）と p（x = alphaTest・y = 不透明か）。体の濃さ（set_opacity）を掛ける元
    base_color: [f32; 4],
    base_p: [f32; 4],
}

struct WeaponGpu {
    vb: wgpu::Buffer,
    skin: wgpu::Buffer,
    ib: wgpu::Buffer,
    items: Vec<DrawItem>,
    /// 節点の、武器の根からの行列
    nodes: Vec<Mat4>,
    tex: Vec<wgpu::BindGroup>,
    scratch: Vec<Mat4>,
    /// この武器の画像（destroy_gpu で返す）
    textures: Vec<wgpu::Texture>,
}

/// 同じ人の何体か（share_from）で共有する GPU の物（形と画像）。destroy_gpu は最後に持っている 1 体だけが返す
struct SharedGpu { textures: Vec<wgpu::Texture>, buffers: Vec<wgpu::Buffer> }

struct HeroGpu {
    vb: wgpu::Buffer,
    skin: wgpu::Buffer,
    ib: wgpu::Buffer,
    deltas: wgpu::Buffer,
    palette: wgpu::Buffer,
    weights: wgpu::Buffer,
    palette_offset: u64,
    palette_size: u64,
    weights_offset: u64,
    weights_size: u64,
    draw_u: wgpu::Buffer,
    /// draw_u の最初の中身（同じ人をもう 1 体描くときに写す）
    draw_init: Arc<Vec<u8>>,
    data_bg: wgpu::BindGroup,
    tex: Vec<wgpu::BindGroup>,
    items: Vec<DrawItem>,
    board: Option<Board>,
    align: u32,
    next_slot: u32,
    palette_len: u32,
    shared: Arc<SharedGpu>,
}

#[derive(Clone)]
struct Board { vb: wgpu::Buffer, skin: wgpu::Buffer, ib: wgpu::Buffer, n: u32, tex: wgpu::BindGroup, offset: u32, color: [f32; 4] }

/// 主役を描くパイプラインと、それが読む並び・サンプラー（的の形式ごとに 1 組。何体描いても共有する）
pub struct ActorPipelines {
    p_opaque: [wgpu::RenderPipeline; 2],
    /// 両面の半透明の裏の面（three の BackSide。表の面の前に描く。法線は裏返す）
    p_trans_back: wgpu::RenderPipeline,
    p_board_back: wgpu::RenderPipeline,
    p_trans: [wgpu::RenderPipeline; 2],
    p_std: [wgpu::RenderPipeline; 2],
    p_weapon_unlit: [wgpu::RenderPipeline; 2],
    p_board: wgpu::RenderPipeline,
    p_shadow: [wgpu::RenderPipeline; 2],
    p_shadow_alpha: [wgpu::RenderPipeline; 2],
    p_mask: wgpu::RenderPipeline,
    p_hidden: wgpu::RenderPipeline,
    /// ミップを GPU で作る部品（set_gpu_mips(true) の体が最初に画像を上げるときに作る）
    mips: std::sync::OnceLock<GpuMips>,
    data_layout: wgpu::BindGroupLayout,
    tex_layout: wgpu::BindGroupLayout,
    empty_bg: wgpu::BindGroup,
    white: wgpu::TextureView,
    white_bg: wgpu::BindGroup,
    smp_repeat: wgpu::Sampler,
    smp_clamp: wgpu::Sampler,
}

/// Actual API calls since the most recent write_frame. These are CPU-side
/// submission counters, not GPU execution times or physical memory usage.
#[derive(Clone, Copy, Debug, Default)]
pub struct ActorFrameStats {
    pub draw_calls: u32,
    /// Includes body bones and, when present, weapon-node matrices.
    pub palette_upload_bytes: u64,
    pub morph_upload_bytes: u64,
    pub uniform_upload_bytes: u64,
}

pub struct ActorView {
    pipes: Arc<ActorPipelines>,
    hero_u: wgpu::Buffer,
    hero_offset: u64,
    style: ActorStyle,
    fade: f32,
    sun: [f32; 4],
    /// セル画の塗りの線の番号（0 は切。set_cel）
    cel_ink: f32,
    gpu: Option<HeroGpu>,
    weapon: Option<WeaponGpu>,
    weapon_unlit: bool,
    frame_stats: Cell<ActorFrameStats>,
    /// 体の濃さ（set_opacity。1 は今までどおり）と、draw_u に書いてある濃さ
    opacity: f32,
    written_opacity: f32,
    gpu_mips: bool,
}

fn tex_bind(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView, smp: &wgpu::Sampler) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout, entries: &[
        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(smp) },
    ] })
}

fn slot_entry(binding: u32, vis: wgpu::ShaderStages, ty: wgpu::BufferBindingType, dynamic: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: vis, ty: wgpu::BindingType::Buffer { ty, has_dynamic_offset: dynamic, min_binding_size: None }, count: None }
}

const WEAPON_NODES: u64 = 16;

fn buffer_range((buffer, offset, size): (&wgpu::Buffer, u64, u64)) -> wgpu::BindingResource<'_> {
    wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer, offset, size: wgpu::BufferSize::new(size) })
}

fn data_bind(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, palette: (&wgpu::Buffer, u64, u64), deltas: &wgpu::Buffer, weights: (&wgpu::Buffer, u64, u64), hero_u: (&wgpu::Buffer, u64), draw_u: &wgpu::Buffer) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("hero data"), layout, entries: &[
        wgpu::BindGroupEntry { binding: 0, resource: buffer_range(palette) },
        wgpu::BindGroupEntry { binding: 1, resource: deltas.as_entire_binding() },
        wgpu::BindGroupEntry { binding: 2, resource: buffer_range(weights) },
        wgpu::BindGroupEntry { binding: 3, resource: buffer_range((hero_u.0, hero_u.1, std::mem::size_of::<HeroU>() as u64)) },
        wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: draw_u, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<DrawU>() as u64) }) },
    ] })
}

fn rgb_w(c: [f32; 3], w: f32) -> [f32; 4] { [c[0], c[1], c[2], w] }

/// 描画パイプライン。target は色の的（None は影の型）
#[allow(clippy::too_many_arguments)]
fn pipeline(device: &wgpu::Device, label: &str, shader: &wgpu::ShaderModule, vs: &str, fs: Option<&str>, layouts: &[&wgpu::BindGroupLayout], target: Option<(wgpu::TextureFormat, Option<wgpu::BlendState>)>, cull: Option<wgpu::Face>, depth_format: wgpu::TextureFormat, depth_write: bool, depth_compare: wgpu::CompareFunction, bias: wgpu::DepthBiasState, samples: u32) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &layouts.iter().map(|l| Some(*l)).collect::<Vec<_>>(), immediate_size: 0 });
    let vb = [Some(vert_layout()), Some(skin_layout())];
    // 影の型は色の的なし（空にしないと、的のないパスと合わない）
    let targets: Vec<Option<wgpu::ColorTargetState>> = target.map(|(format, blend)| wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL }).into_iter().map(Some).collect();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label), layout: Some(&layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some(vs), buffers: &vb, compilation_options: Default::default() },
        fragment: fs.map(|fs| wgpu::FragmentState { module: shader, entry_point: Some(fs), targets: &targets, compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: cull, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: depth_format, depth_write_enabled: Some(depth_write), depth_compare: Some(depth_compare), stencil: Default::default(), bias }),
        multisample: wgpu::MultisampleState { count: samples, ..Default::default() },
        multiview_mask: None,
        cache: None,
    })
}

fn hero_u_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: Some("hero u"), size: std::mem::size_of::<HeroU>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

impl ActorPipelines {
    /// globals_layout は世界の値（group 0。character_view::globals_layout で作った物）
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, globals_layout: &wgpu::BindGroupLayout, t: ActorTargets) -> ActorPipelines {
        let hero_shader = shader(device, "hero", &hero_source(include_str!("../shaders/hero.wgsl")));
        let ro = wgpu::BufferBindingType::Storage { read_only: true };
        let u = wgpu::BufferBindingType::Uniform;
        let (v, vf) = (wgpu::ShaderStages::VERTEX, wgpu::ShaderStages::VERTEX_FRAGMENT);
        let data_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("hero data"), entries: &[
            slot_entry(0, v, ro, false), slot_entry(1, v, ro, false), slot_entry(2, v, ro, false), slot_entry(3, vf, u, false), slot_entry(4, vf, u, true),
        ] });
        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("empty"), entries: &[] });
        let empty_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &empty_layout, entries: &[] });
        let tex_l = tex_layout(device);
        let layouts: [&wgpu::BindGroupLayout; 4] = [globals_layout, &empty_layout, &tex_l, &data_layout];
        let less = wgpu::CompareFunction::LessEqual; // three の LessEqualDepth
        let no_bias = wgpu::DepthBiasState { constant: 0, slope_scale: 0.0, clamp: 0.0 };
        let back = Some(wgpu::Face::Back);
        let mk = |label: &str, fs: &str, double: bool, blend: Option<wgpu::BlendState>| {
            pipeline(device, label, &hero_shader, "vs_main", Some(fs), &layouts, Some((t.color, blend)), if double { None } else { back }, t.depth, blend.is_none(), less, no_bias, t.samples)
        };
        let p_opaque = [mk("hero", "fs_basic", false, None), mk("hero 2s", "fs_basic", true, None)];
        let p_trans = [mk("hero blend", "fs_basic", false, Some(BLEND_NORMAL)), mk("hero blend 2s", "fs_basic", true, Some(BLEND_NORMAL))];
        let p_trans_back = pipeline(device, "hero blend back", &hero_shader, "vs_main", Some("fs_basic_back"), &layouts, Some((t.color, Some(BLEND_NORMAL))), Some(wgpu::Face::Front), t.depth, false, less, no_bias, t.samples);
        let p_std = [mk("weapon", "fs_std", false, None), mk("weapon 2s", "fs_std", true, None)];
        let p_weapon_unlit = [mk("weapon unlit", "fs_weapon_unlit", false, None), mk("weapon unlit 2s", "fs_weapon_unlit", true, None)];
        // 板: 半透明の物の中で最初に描き、奥行きも書く（polygonOffset -2, -2）
        let p_board_back = pipeline(device, "hero board back", &hero_shader, "vs_main", Some("fs_board"), &layouts, Some((t.color, Some(BLEND_NORMAL))), Some(wgpu::Face::Front), t.depth, true, less, wgpu::DepthBiasState { constant: -2, slope_scale: -2.0, clamp: 0.0 }, t.samples);
        let p_board = pipeline(device, "hero board", &hero_shader, "vs_main", Some("fs_board"), &layouts, Some((t.color, Some(BLEND_NORMAL))), Some(wgpu::Face::Back), t.depth, true, less, wgpu::DepthBiasState { constant: -2, slope_scale: -2.0, clamp: 0.0 }, t.samples);
        // three の影は材質の面の裏を描く（両面の材質は両面）
        let shadow_p = |label: &str, fs: Option<&str>, double: bool| {
            pipeline(device, label, &hero_shader, "vs_shadow", fs, &layouts, None, if double { None } else { Some(wgpu::Face::Front) }, t.depth, true, less, no_bias, 1)
        };
        let p_shadow = [shadow_p("hero shadow", None, false), shadow_p("hero shadow 2s", None, true)];
        let p_shadow_alpha = [shadow_p("hero shadow a", Some("fs_shadow_alpha"), false), shadow_p("hero shadow a 2s", Some("fs_shadow_alpha"), true)];
        let p_mask = pipeline(device, "hero mask", &hero_shader, "vs_mask", Some("fs_mask"), &layouts, Some((t.mask, None)), back, t.depth, true, less, no_bias, 1);
        let p_hidden = pipeline(device, "hero hidden", &hero_shader, "vs_hidden", Some("fs_hidden"), &layouts, Some((t.color, Some(BLEND_NORMAL))), back, t.depth, false, wgpu::CompareFunction::Greater, no_bias, t.samples);

        let smp_repeat = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("hero"), address_mode_u: wgpu::AddressMode::Repeat, address_mode_v: wgpu::AddressMode::Repeat, mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::MipmapFilterMode::Linear, ..Default::default() });
        let smp_clamp = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("hero board"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::MipmapFilterMode::Linear, ..Default::default() });
        let white_t = device.create_texture_with_data(queue, &wgpu::TextureDescriptor {
            label: Some("hero white"), size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8UnormSrgb, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        }, wgpu::wgt::TextureDataOrder::default(), &[255, 255, 255, 255]);
        let white = white_t.create_view(&Default::default());
        let white_bg = tex_bind(device, &tex_l, &white, &smp_repeat);
        ActorPipelines { p_opaque, p_trans, p_trans_back, p_board_back, p_std, p_weapon_unlit, p_board, p_shadow, p_shadow_alpha, p_mask, p_hidden, mips: std::sync::OnceLock::new(), data_layout, tex_layout: tex_l, empty_bg, white, white_bg, smp_repeat, smp_clamp }
    }
}

impl ActorView {
    /// globals_layout は世界の値（group 0。character_view::globals_layout で作った物）。呼ぶ側が描く前に group 0 を設定する
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, globals_layout: &wgpu::BindGroupLayout, t: ActorTargets) -> ActorView {
        ActorView::with_pipelines(device, Arc::new(ActorPipelines::new(device, queue, globals_layout, t)))
    }

    /// パイプラインを共有して作る（何体も描くとき）
    pub fn with_pipelines(device: &wgpu::Device, pipes: Arc<ActorPipelines>) -> ActorView {
        ActorView { pipes, hero_u: hero_u_buffer(device), hero_offset: 0, style: ActorStyle::default(), fade: 0.0, sun: [0.0; 4], cel_ink: 0.0, gpu: None, weapon: None, weapon_unlit: false, frame_stats: Cell::new(ActorFrameStats::default()), opacity: 1.0, written_opacity: 1.0, gpu_mips: false }
    }

    /// 使っているパイプライン（別の Kit へ持ち回った主役のパイプラインを、そこで描くほかの主役にも使い回すとき）
    pub fn pipelines(&self) -> Arc<ActorPipelines> { self.pipes.clone() }

    /// 同じ人をもう 1 体描く: GPU の形（頂点・index・テクスチャ・モーフの差分）は共有し、骨・モーフの重み・見せ方の値だけ別に持つ。
    /// 動かす部品は別に読んだ CharacterActor（形は要らない）。武器は持ち越さない（要るなら set_weapon）
    pub fn share_from(other: &ActorView, device: &wgpu::Device) -> ActorView {
        let mut out = ActorView::with_pipelines(device, other.pipes.clone());
        out.style = other.style;
        out.gpu_mips = other.gpu_mips;
        let Some(hg) = other.gpu.as_ref() else { return out };
        let palette = device.create_buffer(&wgpu::BufferDescriptor { label: Some("hero palette"), size: hg.palette_size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let weights = device.create_buffer(&wgpu::BufferDescriptor { label: Some("hero weights"), size: hg.weights_size, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let draw_u = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("hero draws"), contents: &hg.draw_init, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST });
        let data_bg = data_bind(device, &out.pipes.data_layout, (&palette, 0, hg.palette_size), &hg.deltas, (&weights, 0, hg.weights_size), (&out.hero_u, 0), &draw_u);
        out.gpu = Some(HeroGpu {
            vb: hg.vb.clone(), skin: hg.skin.clone(), ib: hg.ib.clone(), deltas: hg.deltas.clone(), palette, weights, palette_offset: 0, palette_size: hg.palette_size, weights_offset: 0, weights_size: hg.weights_size, draw_u, draw_init: hg.draw_init.clone(), data_bg,
            tex: hg.tex.clone(), items: hg.items.clone(), board: hg.board.clone(), align: hg.align, next_slot: hg.next_slot, palette_len: hg.palette_len, shared: hg.shared.clone(),
        });
        out
    }

    /// GPU へ上げた形はもう使わない（three が CPU 側の写しを持たないのと同じ量にする）。share_from で 2 体目を作ったときも呼ぶ
    pub fn release_shapes(actor: &mut CharacterActor) {
        actor.vertices = Vec::new();
        actor.skin = Vec::new();
        actor.indices = Vec::new();
        actor.deltas = Vec::new();
        if let Some(h) = &mut actor.hurt { h.vertices = Vec::new(); h.indices = Vec::new(); h.tex.2 = Vec::new(); }
        if let Some(g) = Arc::get_mut(&mut actor.glb) { g.images = Vec::new(); g.meshes = Vec::new(); }
    }

    /// 輪郭の光の式を選ぶ（既定は MUSO）
    pub fn set_style(&mut self, style: ActorStyle) { self.style = style; }
    /// カメラと自分の間にいる主役を網目に抜いて透かす（0..1。keiba の uFade。並び替えが要らない）。既定 0 は今までと同じ絵
    pub fn set_fade(&mut self, k: f32) { self.fade = k; }
    /// セル画の塗り（painted-world の絵画調の世界に立つとき）。不透明な体を、日なた（key_color）か陰（hemi_sky）かの 2 色で塗り、
    /// アルファへ線のための番号（ink + 部位ごとの刻み。0.03〜0.21）を書く。輪郭の光・夕日の縁・霧は掛けない。None で今までの塗り（既定）
    pub fn set_cel(&mut self, ink: Option<f32>) { self.cel_ink = ink.unwrap_or(0.0).max(0.0); }
    /// 夕日の陰と縁の光（Engimeow の RimLit の addRim。k = 0 で切る）
    /// 体の色に一律に掛ける（three の材質の outgoingLight *= 色）
    pub fn set_tint(&mut self, color: [f32; 3]) { self.sun = [color[0], color[1], color[2], -1.0]; }
    pub fn set_sun_rim(&mut self, color: [f32; 3], k: f32) { self.sun = [color[0], color[1], color[2], k]; }

    /// 体の濃さ（0..1。three の材質の opacity に掛ける。aisns の owned-fade: 仲間がカメラを遮ったとき）。1 未満の間は、不透明の体も
    /// 半透明の描き方へ移す（draw_opaque では描かず draw_transparent で描く。奥行きは書かない。alphaTest にも同じ値を掛ける）。
    /// 武器・影・自機の型・物陰の影絵は変えない。＞＜ の板の濃さにも掛ける。値は次の write_frame で GPU へ。既定 1 は今までと同じ絵
    pub fn set_opacity(&mut self, k: f32) { self.opacity = if k.is_finite() { k.clamp(0.0, 1.0) } else { 1.0 }; }
    pub fn opacity(&self) -> f32 { self.opacity }
    /// 体を半透明の描き方で描いているか（set_opacity が 1 未満）。描く順を半透明の側へ移すときに見る
    pub fn translucent(&self) -> bool { self.opacity < 1.0 }

    /// 画像のミップを GPU で作る（既定 false は今までの CPU）。set_actor・set_weapon の前に呼ぶ。出る値は CPU とビットで同じ（tests/gpu_mips.rs）。
    /// 4096² の主役の画像で CPU が約 1 秒止まるのを無くす
    pub fn set_gpu_mips(&mut self, on: bool) { self.gpu_mips = on; }

    fn make_tex(&self, device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32, rgba: &[u8]) -> wgpu::Texture {
        let mips = if self.gpu_mips { Some(self.pipes.mips.get_or_init(|| GpuMips::new(device, queue))) } else { None };
        make_texture(device, queue, w, h, rgba, true, mips)
    }

    /// GPU の物を返す（wgpu の destroy。web の wgpu は手放しても GPU の物を返さない）。体・武器・＞＜ の板の画像と、骨・モーフの重み・描く値の列。
    /// share_from で同じ形を使う体がまだあるときは、形と画像は返さずに手放すだけ（最後の 1 体が返す）。戻り値は形と画像を返したか。
    /// 呼んだあとは主役を持たない（もう一度描くなら set_actor）
    pub fn destroy_gpu(&mut self) -> bool {
        self.drop_weapon();
        let Some(hg) = self.gpu.take() else { return false };
        for b in [&hg.palette, &hg.weights, &hg.draw_u] { b.destroy(); }
        let HeroGpu { shared, .. } = hg;
        match Arc::try_unwrap(shared) {
            Ok(s) => { for t in &s.textures { t.destroy(); } for b in &s.buffers { b.destroy(); } true }
            Err(_) => false,
        }
    }

    /// 武器の模型を持っているか
    pub fn has_weapon(&self) -> bool { self.weapon.is_some() }
    /// 武器の模型を外す（持つ位置は部品の hold_weapon）。武器の画像と列は destroy で返す
    pub fn clear_weapon(&mut self) { self.drop_weapon(); }
    /// 武器の GPU の物を返して外す（この view だけが持つ物。share_from は武器を持ち越さない）
    fn drop_weapon(&mut self) {
        if let Some(w) = self.weapon.take() {
            for t in &w.textures { t.destroy(); }
            for b in [&w.vb, &w.skin, &w.ib] { b.destroy(); }
        }
    }
    /// 武器を光なしで描く（three の MeshBasicMaterial。aisns の loadWeaponModel）。既定は光あり（MeshStandardMaterial。muso）
    pub fn set_weapon_unlit(&mut self, on: bool) { self.weapon_unlit = on; }

    /// 部品で読んだ主役を GPU へ上げる（前の主役と武器は片付ける）。上げた形は actor から消す（three が CPU 側の写しを持たないのと同じ量にする）
    pub fn set_actor(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, actor: &mut CharacterActor) {
        let align = device.limits().min_uniform_buffer_offset_alignment.max(std::mem::size_of::<DrawU>() as u32);
        let init = |contents: &[u8], usage: wgpu::BufferUsages| device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("hero"), contents, usage });
        let vb = init(bytemuck::cast_slice(&actor.vertices), wgpu::BufferUsages::VERTEX);
        let skin = init(bytemuck::cast_slice(&actor.skin), wgpu::BufferUsages::VERTEX);
        let ib = init(bytemuck::cast_slice(&actor.indices), wgpu::BufferUsages::INDEX);
        // 骨の表（主役の分 + 武器の節点の分）
        let palette_len = actor.palette.len() as u32;
        let palette = device.create_buffer(&wgpu::BufferDescriptor { label: Some("hero palette"), size: (palette_len as u64 + WEAPON_NODES) * 64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let zero_delta = [0f32; 6];
        let deltas = if actor.deltas.is_empty() { init(bytemuck::cast_slice(&zero_delta), wgpu::BufferUsages::STORAGE) } else { init(bytemuck::cast_slice(&actor.deltas), wgpu::BufferUsages::STORAGE) };
        let weights = device.create_buffer(&wgpu::BufferDescriptor { label: Some("hero weights"), size: (actor.morph_weights.len().max(1) * 4) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

        // テクスチャ（色は sRGB。材質ごとの画像を 1 回だけ載せる）
        let pipes = self.pipes.clone();
        let mut tex = vec![tex_bind(device, &pipes.tex_layout, &pipes.white, &pipes.smp_repeat)];
        let mut tex_of: HashMap<usize, usize> = HashMap::new();
        let mut textures: Vec<wgpu::Texture> = Vec::new();
        let mats = actor.materials.clone();
        for m in &mats {
            if let Some(i) = m.map && !tex_of.contains_key(&i) {
                let im = &actor.glb.images[i];
                let t = self.make_tex(device, queue, im.width, im.height, &im.rgba);
                tex.push(tex_bind(device, &pipes.tex_layout, &t.create_view(&Default::default()), &pipes.smp_repeat));
                textures.push(t);
                tex_of.insert(i, tex.len() - 1);
            }
        }

        let slots = actor.prims.len() as u32 + 1 + WEAPON_NODES as u32;
        let mut draw_init = vec![0u8; (slots * align) as usize];
        let mut put = |offset: u32, u: &DrawU| draw_init[offset as usize..offset as usize + std::mem::size_of::<DrawU>()].copy_from_slice(bytemuck::bytes_of(u));
        let mut items = Vec::with_capacity(actor.prims.len());
        let mut slot = 0u32;
        for (pi, p) in actor.prims.iter().enumerate() {
            let m = &mats[p.material];
            let offset = slot * align;
            slot += 1;
            let (base_color, base_p) = ([m.color[0], m.color[1], m.color[2], m.opacity], [m.alpha_test, if m.transparent { 0.0 } else { 1.0 }, 1.0, 0.0]);
            let u = DrawU {
                color: base_color,
                p: base_p,
                // e の a は空きなので、セル画の線の番号の部位ごとの刻みを載せる（セル画の塗りのときだけ読む）
                e: [0.0, 0.0, 0.0, (p.material % 7) as f32 * 0.03],
                morph: [p.morph_base, p.morph_count, p.weight_base, p.vcount],
                info: [p.vstart, 0, p.rigid as u32, 0],
                ..Default::default()
            };
            put(offset, &u);
            items.push(DrawItem {
                prim: Some(pi), istart: p.istart, icount: p.icount, base_vertex: p.vstart as i32, offset, tex: m.map.map(|i| tex_of[&i]).unwrap_or(0),
                double: m.double_sided, transparent: m.transparent, alpha_shadow: m.map.is_some() && m.alpha_test > 0.0, base_color, base_p,
            });
        }
        // three の透明の並びは z が同じなら作った順（節点の順）。骨のあるメッシュは全部同じ位置にあるので、並びはこのまま
        let board = actor.hurt.as_ref().map(|h| {
            let vb = init(bytemuck::cast_slice(&h.vertices), wgpu::BufferUsages::VERTEX);
            let s: Vec<SkinVertex> = h.vertices.iter().map(|_| SkinVertex { joints: [actor.hurt_joint, 0, 0, 0], weights: [1.0, 0.0, 0.0, 0.0] }).collect();
            let skin = init(bytemuck::cast_slice(&s), wgpu::BufferUsages::VERTEX);
            let ib = init(bytemuck::cast_slice(&h.indices), wgpu::BufferUsages::INDEX);
            let (w, hh, px) = &h.tex;
            let offset = slot * align;
            slot += 1;
            let t = self.make_tex(device, queue, *w, *hh, px);
            let b = Board { vb, skin, ib, n: h.indices.len() as u32, tex: tex_bind(device, &pipes.tex_layout, &t.create_view(&Default::default()), &pipes.smp_clamp), offset, color: [1.0, 1.0, 1.0, 0.0] };
            textures.push(t);
            b
        });
        if let Some(b) = &board {
            let u = DrawU { color: b.color, p: [0.02, 0.0, 1.0, 0.0], info: [0, 0, 1, 0], ..Default::default() };
            put(b.offset, &u);
        }
        let draw_u = init(&draw_init, wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST);
        let palette_size = palette.size();
        let weights_size = weights.size();
        let data_bg = data_bind(device, &pipes.data_layout, (&palette, 0, palette_size), &deltas, (&weights, 0, weights_size), (&self.hero_u, self.hero_offset), &draw_u);
        ActorView::release_shapes(actor);
        let mut buffers = vec![vb.clone(), skin.clone(), ib.clone(), deltas.clone()];
        if let Some(b) = &board { buffers.extend([b.vb.clone(), b.skin.clone(), b.ib.clone()]); }
        let shared = Arc::new(SharedGpu { textures, buffers });
        self.gpu = Some(HeroGpu { vb, skin, ib, deltas, palette, weights, palette_offset: 0, palette_size, weights_offset: 0, weights_size, draw_u, draw_init: Arc::new(draw_init), data_bg, tex, items, board, align, next_slot: slot, palette_len, shared });
        self.drop_weapon();
        self.written_opacity = 1.0;
    }

    /// 武器の模型を GPU へ上げる（どの模型か・長さ・持つ位置はゲームと部品。set_actor のあと）
    pub fn set_weapon(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, g: &Glb) {
        self.drop_weapon();
        let Some(hg) = self.gpu.as_ref() else { return };
        let world = g.world_matrices();
        let (mut verts, mut skin, mut idx): (Vec<Vertex>, Vec<SkinVertex>, Vec<u32>) = (Vec::new(), Vec::new(), Vec::new());
        let mut items = Vec::new();
        let mut nodes = Vec::new();
        let pipes = self.pipes.clone();
        let mut tex: Vec<wgpu::BindGroup> = vec![tex_bind(device, &pipes.tex_layout, &pipes.white, &pipes.smp_repeat)];
        let mut tex_of: HashMap<usize, usize> = HashMap::new();
        let mut textures: Vec<wgpu::Texture> = Vec::new();
        let mut slot = hg.next_slot;
        for (ni, node) in g.nodes.iter().enumerate() {
            let Some(mi) = node.mesh else { continue };
            if nodes.len() as u64 >= WEAPON_NODES { break; }
            let at = nodes.len() as u32;
            nodes.push(world[ni]);
            for p in &g.meshes[mi].primitives {
                let vstart = verts.len() as u32;
                let istart = idx.len() as u32;
                let has_color = p.colors.len() == p.positions.len();
                for i in 0..p.positions.len() {
                    verts.push(Vertex { pos: p.positions[i], normal: p.normals[i], uv: p.uvs[i], color: if has_color { p.colors[i] } else { [1.0; 4] }, sway: 0.0 });
                    skin.push(SkinVertex { joints: [0; 4], weights: [1.0, 0.0, 0.0, 0.0] });
                }
                idx.extend_from_slice(&p.indices);
                let m = p.material.map(|i| &g.materials[i]);
                let tex_i = match m.and_then(|m| m.texture) {
                    Some(i) => match tex_of.get(&i) {
                        Some(t) => *t,
                        None => {
                            let im = &g.images[i];
                            let t = self.make_tex(device, queue, im.width, im.height, &im.rgba);
                            tex.push(tex_bind(device, &pipes.tex_layout, &t.create_view(&Default::default()), &pipes.smp_repeat));
                            textures.push(t);
                            tex_of.insert(i, tex.len() - 1);
                            tex.len() - 1
                        }
                    },
                    None => 0,
                };
                let (color, rough, metal, e, double, blend, cut) = match m {
                    Some(m) => (m.base_color, m.roughness, m.metallic, m.emissive, m.double_sided, m.alpha_blend, m.alpha_cutoff.unwrap_or(0.0)),
                    None => ([1.0; 4], 1.0, 1.0, [0.0; 3], false, false, 0.0),
                };
                let offset = slot * hg.align;
                if ((offset + hg.align) as u64) > hg.draw_u.size() { break; }
                slot += 1;
                let u = DrawU {
                    color, p: [cut, if blend { 0.0 } else { 1.0 }, rough, metal], e: [e[0], e[1], e[2], 0.0],
                    info: [vstart, hg.palette_len + at, 1, 0],
                    ..Default::default()
                };
                queue.write_buffer(&hg.draw_u, offset as u64, bytemuck::bytes_of(&u));
                items.push(DrawItem { prim: None, istart, icount: p.indices.len() as u32, base_vertex: vstart as i32, offset, tex: tex_i, double, transparent: blend, alpha_shadow: false, base_color: color, base_p: u.p });
            }
        }
        let init = |contents: &[u8], usage: wgpu::BufferUsages| device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("weapon"), contents, usage });
        self.weapon = Some(WeaponGpu {
            vb: init(bytemuck::cast_slice(&verts), wgpu::BufferUsages::VERTEX),
            skin: init(bytemuck::cast_slice(&skin), wgpu::BufferUsages::VERTEX),
            ib: init(bytemuck::cast_slice(&idx), wgpu::BufferUsages::INDEX),
            scratch: nodes.clone(), items, nodes, tex, textures,
        });
    }

    /// 骨の行列・モーフの重み・見せ方の値を GPU へ（毎コマ、actor.update のあと・描く前）
    pub fn write_frame(&mut self, queue: &wgpu::Queue, actor: &CharacterActor, look: &ActorLook) {
        self.frame_stats.set(ActorFrameStats::default());
        // 法線を入れ物の伸び縮みに合わせる: three の normalMatrix × 骨 = (P Pᵀ)⁻¹ × 骨の世界の行列。P が回転と一様な大きさだけなら定数倍で、normalize で消える
        let p = glam::Mat3::from_mat4(actor.parent_world());
        let a = p * p.transpose();
        let nf = if a.determinant().abs() > 1e-12 { a.inverse() } else { glam::Mat3::IDENTITY };
        let u = HeroU {
            rim_p: rgb_w(look.rim, look.rim_strength),
            aura: rgb_w(look.aura, look.aura_strength),
            misc: [look.top, look.lift, look.flash, self.fade],
            hidden: rgb_w(look.hidden, look.hidden_opacity),
            style: [if self.style.flat_lift { 1.0 } else { 0.0 }, self.style.rim_pow, self.style.sat, self.style.top_self],
            // nfix の 1 行めの w は空きなので、セル画の線の番号を載せる（0 は切）
            nfix: [nf.x_axis.extend(self.cel_ink).to_array(), nf.y_axis.extend(0.0).to_array(), nf.z_axis.extend(0.0).to_array()],
            sun: self.sun,
        };
        queue.write_buffer(&self.hero_u, self.hero_offset, bytemuck::bytes_of(&u));
        self.frame_stats.get_mut().uniform_upload_bytes += std::mem::size_of_val(&u) as u64;
        let Some(hg) = self.gpu.as_mut() else { return };
        let palette_data: &[u8] = bytemuck::cast_slice(&actor.palette.mats);
        queue.write_buffer(&hg.palette, hg.palette_offset, palette_data);
        self.frame_stats.get_mut().palette_upload_bytes += palette_data.len() as u64;
        if !actor.morph_weights.is_empty() {
            let morph_data: &[u8] = bytemuck::cast_slice(&actor.morph_weights);
            queue.write_buffer(&hg.weights, hg.weights_offset, morph_data);
            self.frame_stats.get_mut().morph_upload_bytes += morph_data.len() as u64;
        }
        if let (Some(w), Some(g)) = (self.weapon.as_mut(), actor.grip_world()) {
            for (o, n) in w.scratch.iter_mut().zip(&w.nodes) { *o = g * *n; }
            let weapon_data: &[u8] = bytemuck::cast_slice(&w.scratch);
            queue.write_buffer(&hg.palette, hg.palette_offset + hg.palette_len as u64 * 64, weapon_data);
            self.frame_stats.get_mut().palette_upload_bytes += weapon_data.len() as u64;
        }
        if self.opacity != self.written_opacity {
            // 体の濃さ: 色の a と alphaTest に掛け、不透明の印を外す（1 に戻せば材質のまま）
            let k = self.opacity;
            for it in hg.items.iter().filter(|it| it.prim.is_some()) {
                let c = [it.base_color[0], it.base_color[1], it.base_color[2], it.base_color[3] * k];
                let p = if k < 1.0 { [it.base_p[0] * k, 0.0, it.base_p[2], it.base_p[3]] } else { it.base_p };
                let mut head = [0f32; 8];
                head[..4].copy_from_slice(&c);
                head[4..].copy_from_slice(&p);
                queue.write_buffer(&hg.draw_u, it.offset as u64, bytemuck::cast_slice(&head));
                self.frame_stats.get_mut().uniform_upload_bytes += 32;
            }
            self.written_opacity = k;
            if let Some(b) = hg.board.as_mut() { b.color[3] = -1.0; }
        }
        if let (Some(b), Some(h)) = (hg.board.as_mut(), actor.hurt.as_ref()) && b.color[3] != h.opacity * self.opacity {
            b.color[3] = h.opacity * self.opacity;
            queue.write_buffer(&hg.draw_u, b.offset as u64, bytemuck::cast_slice(&b.color));
            self.frame_stats.get_mut().uniform_upload_bytes += std::mem::size_of_val(&b.color) as u64;
        }
    }

    pub fn frame_stats(&self) -> ActorFrameStats { self.frame_stats.get() }

    fn record_draw(&self) {
        let mut stats = self.frame_stats.get();
        stats.draw_calls += 1;
        self.frame_stats.set(stats);
    }

    fn visible(actor: &CharacterActor, it: &DrawItem) -> bool {
        match it.prim { Some(i) => actor.prims[i].visible, None => true }
    }

    fn draw_items<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, hg: &'a HeroGpu, actor: &CharacterActor, pick: impl Fn(&DrawItem) -> Option<&'a wgpu::RenderPipeline>, textured: bool) {
        pass.set_bind_group(1, &self.pipes.empty_bg, &[]);
        if !textured { pass.set_bind_group(2, &self.pipes.white_bg, &[]); }
        pass.set_vertex_buffer(0, hg.vb.slice(..));
        pass.set_vertex_buffer(1, hg.skin.slice(..));
        pass.set_index_buffer(hg.ib.slice(..), wgpu::IndexFormat::Uint32);
        for it in &hg.items {
            if !Self::visible(actor, it) { continue; }
            let Some(pipe) = pick(it) else { continue };
            pass.set_pipeline(pipe);
            if textured { pass.set_bind_group(2, &hg.tex[it.tex], &[]); }
            pass.set_bind_group(3, &hg.data_bg, &[it.offset]);
            pass.draw_indexed(it.istart..it.istart + it.icount, it.base_vertex, 0..1);
            self.record_draw();
        }
    }

    fn draw_weapon<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, hg: &'a HeroGpu, w: &'a WeaponGpu, pick: impl Fn(&DrawItem) -> Option<&'a wgpu::RenderPipeline>, textured: bool) {
        pass.set_bind_group(1, &self.pipes.empty_bg, &[]);
        if !textured { pass.set_bind_group(2, &self.pipes.white_bg, &[]); }
        pass.set_vertex_buffer(0, w.vb.slice(..));
        pass.set_vertex_buffer(1, w.skin.slice(..));
        pass.set_index_buffer(w.ib.slice(..), wgpu::IndexFormat::Uint32);
        for it in &w.items {
            let Some(pipe) = pick(it) else { continue };
            pass.set_pipeline(pipe);
            if textured { pass.set_bind_group(2, &w.tex[it.tex], &[]); }
            pass.set_bind_group(3, &hg.data_bg, &[it.offset]);
            pass.draw_indexed(it.istart..it.istart + it.icount, it.base_vertex, 0..1);
            self.record_draw();
        }
    }

    /// 影の型（group 0 は呼ぶ側が設定済み）。＞＜ の板は影を落とさない
    pub fn shadow<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, actor: &CharacterActor) {
        let Some(hg) = &self.gpu else { return };
        pass.set_bind_group(1, &self.pipes.empty_bg, &[]);
        pass.set_bind_group(2, &self.pipes.white_bg, &[]);
        pass.set_vertex_buffer(0, hg.vb.slice(..));
        pass.set_vertex_buffer(1, hg.skin.slice(..));
        pass.set_index_buffer(hg.ib.slice(..), wgpu::IndexFormat::Uint32);
        for it in &hg.items {
            if !Self::visible(actor, it) { continue; }
            let d = it.double as usize;
            if it.alpha_shadow {
                pass.set_pipeline(&self.pipes.p_shadow_alpha[d]);
                pass.set_bind_group(2, &hg.tex[it.tex], &[]);
            } else {
                pass.set_pipeline(&self.pipes.p_shadow[d]);
                pass.set_bind_group(2, &self.pipes.white_bg, &[]);
            }
            pass.set_bind_group(3, &hg.data_bg, &[it.offset]);
            pass.draw_indexed(it.istart..it.istart + it.icount, it.base_vertex, 0..1);
            self.record_draw();
        }
        if let Some(w) = &self.weapon { self.draw_weapon(pass, hg, w, |it| Some(&self.pipes.p_shadow[it.double as usize]), false); }
    }

    /// 不透明の体と武器（group 0 は呼ぶ側が設定済み。主の的）
    pub fn draw_opaque<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, actor: &CharacterActor) {
        let Some(hg) = &self.gpu else { return };
        let translucent = self.translucent();
        self.draw_items(pass, hg, actor, |it| if it.transparent || translucent { None } else { Some(&self.pipes.p_opaque[it.double as usize]) }, true);
        let weapon_pipes = if self.weapon_unlit { &self.pipes.p_weapon_unlit } else { &self.pipes.p_std };
        if let Some(w) = &self.weapon { self.draw_weapon(pass, hg, w, |it| Some(&weapon_pipes[it.double as usize]), true); }
    }

    /// 半透明の物（＞＜ の板が先。three の renderOrder -1 と 0）。不透明のあとに
    pub fn draw_transparent<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, actor: &CharacterActor) {
        let Some(hg) = &self.gpu else { return };
        if let Some(b) = &hg.board && actor.hurt.as_ref().is_some_and(|h| h.visible) {
            pass.set_bind_group(1, &self.pipes.empty_bg, &[]);
            pass.set_bind_group(2, &b.tex, &[]);
            pass.set_bind_group(3, &hg.data_bg, &[b.offset]);
            pass.set_vertex_buffer(0, b.vb.slice(..));
            pass.set_vertex_buffer(1, b.skin.slice(..));
            pass.set_index_buffer(b.ib.slice(..), wgpu::IndexFormat::Uint32);
            // 両面の半透明（裏 → 表）
            pass.set_pipeline(&self.pipes.p_board_back);
            pass.draw_indexed(0..b.n, 0, 0..1);
            self.record_draw();
            pass.set_pipeline(&self.pipes.p_board);
            pass.draw_indexed(0..b.n, 0, 0..1);
            self.record_draw();
        }
        // 両面の半透明は three と同じく裏の面 → 表の面（まぶたのように 2 重の面が、作った順で上書きされない）
        pass.set_vertex_buffer(0, hg.vb.slice(..));
        pass.set_vertex_buffer(1, hg.skin.slice(..));
        pass.set_index_buffer(hg.ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.set_bind_group(1, &self.pipes.empty_bg, &[]);
        let translucent = self.translucent();
        for it in &hg.items {
            if !(it.transparent || translucent) || !Self::visible(actor, it) { continue; }
            pass.set_bind_group(2, &hg.tex[it.tex], &[]);
            pass.set_bind_group(3, &hg.data_bg, &[it.offset]);
            if it.double {
                pass.set_pipeline(&self.pipes.p_trans_back);
                pass.draw_indexed(it.istart..it.istart + it.icount, it.base_vertex, 0..1);
                self.record_draw();
            }
            pass.set_pipeline(&self.pipes.p_trans[0]);
            pass.draw_indexed(it.istart..it.istart + it.icount, it.base_vertex, 0..1);
            self.record_draw();
        }
    }

    /// 自機だけの型を白で描く（縁取り用。＞＜ の板は除く）。的は mask の形式・1 サンプル
    pub fn draw_mask<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, actor: &CharacterActor) {
        let Some(hg) = &self.gpu else { return };
        self.draw_items(pass, hg, actor, |_| Some(&self.pipes.p_mask), false);
        if let Some(w) = &self.weapon { self.draw_weapon(pass, hg, w, |_| Some(&self.pipes.p_mask), false); }
    }

    /// 物陰の影絵: 段やクッキーに隠れた部分だけを、うすいクリーム色で描く（renderer.ts の HiddenPlayerPass）。主の的と深度に描く
    pub fn draw_hidden<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, actor: &CharacterActor) {
        let Some(hg) = &self.gpu else { return };
        self.draw_items(pass, hg, actor, |_| Some(&self.pipes.p_hidden), false);
        if let Some(w) = &self.weapon { self.draw_weapon(pass, hg, w, |_| Some(&self.pipes.p_hidden), false); }
    }
}
