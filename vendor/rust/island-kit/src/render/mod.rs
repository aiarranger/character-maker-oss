// Kit::new はブラウザだけなので、Mac の試験では使わない物がある
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
// 描画の核（three の WebGLRenderer と EffectComposer の役）。Kit が節点の場面・形・材質・テクスチャ・主役を持ち、render で 1 コマ描く。
// 描く順と決めごとは README。frame.rs が 1 コマの流れ、pipes.rs がパイプライン、post.rs が後処理。

mod batch;
mod env;
mod frame;
mod morph;
mod npc;
mod overlay;
mod pipes;
mod post;

pub(crate) use post::target as post_target;

pub use overlay::OverlayQuad;
pub use frame::ViewDraw;
use crate::geometry::{Geometry, Group};
use crate::material::{GeoId, MatId, MatKind, Material, TexId};
use crate::math::Color;
use crate::scene::{Camera, NodeId, Scene};
use crate::texture::{TexEntry, TexOpts, Wrap};
use character_actor::actor::CharacterActor;
use character_actor::glb::Glb;
use character_view::{ActorLook, ActorPipelines, ActorStyle, ActorTargets, ActorView, Globals, Vertex};
use glam::{Mat4, Vec3};
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const MASK: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
/// 既定の MSAA のサンプル数（Aa::Msaa4）
pub const SAMPLES: u32 = 4;

/// 縁のなめらかさ（kit.set_aa。既定 Msaa4）。Off と Smaa は主の的・別のカメラの的・透過用の的を 1 サンプルで描く。
/// Smaa は仕上げのあとに SMAA（postprocessing の SMAAEffect の HIGH・色で縁を見る。three の版の aisns の野と同じ設定）をかける
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aa { Msaa4, Off, Smaa }

/// Custom のシェーダーの先頭に付く共通（character_view の COMMON のあと）。kit が付けるので、ゲームは書かない
pub const KIT_COMMON: &str = include_str!("../../shaders/kit_common.wgsl");

/// 材質の値の 1 枠（kit.wgsl の MatU・Custom の uniform の上限）
const MAT_SLOT: u64 = 256;

/// three の FogExp2（scene.fog）
#[derive(Clone, Copy, Debug)]
pub struct FogExp2 { pub color: Color, pub density: f32 }
/// three の Fog（線形。scene.fog。kit.fog_linear。kit.fog より優先）
#[derive(Clone, Copy, Debug)]
pub struct FogLinear { pub color: Color, pub near: f32, pub far: f32 }
/// 色の出口（kit.output）。Linear は既定（線形の HDR の的 → 後処理の最後に sRGB。three の EffectComposer の形）。
/// DirectFlat・DirectAces は、three が composer なしで画面へ直接描く形（flat の Canvas と ACES の Canvas）: 材質ごとに sRGB へ変えてからブレンドする。
/// このときは kit.set_final(kit::FINAL_DIRECT, 16) と kit.bloom_enabled = false で使う
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Output { Linear, DirectFlat, DirectAces }
/// 色をそのまま画面へ（Output::DirectFlat・DirectAces の仕上げ。alpha も残す）
pub const FINAL_DIRECT: &str = "struct KitFinalU { pad: vec4f };\n@group(1) @binding(0) var<uniform> u: KitFinalU;\n@fragment fn fs_final(i: VOut) -> @location(0) vec4f { return smp(i.uv); }\n";

/// やわらかい材質の共有の値（three の SHARED）
#[derive(Clone, Copy, Debug)]
pub struct Shared {
    /// uTime
    pub time: f32,
    pub rim_color: Color,
    pub rim_i: f32,
    /// 灯りの戻り（uDim）
    pub dim: f32,
    /// 灯籠（x, z, 半径, 灯った度合い。uLamps）
    pub lamps: [[f32; 4]; 8],
    /// 流れる雲の影の濃さ（digdag の uCloudK。0 なら何も足さない）
    pub cloud_k: f32,
}
impl Default for Shared {
    fn default() -> Self { Shared { time: 0.0, rim_color: Color::hex(0xffb48a), rim_i: 0.35, dim: 0.0, lamps: [[0.0; 4]; 8], cloud_k: 0.0 } }
}

/// UnrealBloomPass の値（毎コマ変えられる）
#[derive(Clone, Copy, Debug)]
pub struct Bloom { pub strength: f32, pub radius: f32, pub threshold: f32 }
impl Default for Bloom { fn default() -> Self { Bloom { strength: 0.4, radius: 0.5, threshold: 1.2 } } }

/// 透過用の別描画（three の transmission の renderTransmissionPass）。kit.transmission に入れると、render が主の描画の前に、
/// 不透明な物だけを主のカメラで透過用の的（kit.transmission_target()。HDR・ミップ付き）へ描き、ミップを作る
#[derive(Clone, Debug)]
pub struct Transmission {
    /// 的の大きさ = 主の的 × scale（three の transmissionResolutionScale。既定 1）
    pub scale: f32,
    /// 描かない節点（子も）。透過する物そのもの（three の transmission > 0 の物）を入れる。透過用の的を textures に持つ材質は入れなくても描かない
    pub exclude: Vec<NodeId>,
}
impl Default for Transmission { fn default() -> Self { Transmission { scale: 1.0, exclude: Vec::new() } } }

/// render の指定
#[derive(Clone, Copy, Debug)]
pub struct RenderOpts {
    /// 層 1 の型を描く（仕上げの縁取り）
    pub outline: bool,
    /// 物陰の影絵を描く（renderer.ts の hiddenOn。outline のときだけ効く）
    pub hidden: bool,
    /// three の setClearColor（線形）
    pub clear: Color,
}
impl Default for RenderOpts { fn default() -> Self { RenderOpts { outline: false, hidden: false, clear: Color::BLACK } } }

pub(crate) struct MatSlot {
    pub m: Material,
    /// 節点の種類と裏返しごとのパイプライン（pipes::vert_slot）
    pub pipes: [u32; 8],
    /// 両面の半透明の 2 回描きのパイプライン（pipes::side_pipe）
    pub side_pipes: [u32; 8],
    pub bg: u32,
    pub dirty: bool,
}

/// 骨の組の GPU（three の boneTexture の役。1 組に 1 つ。1 コマ 1 回だけ送る）
pub(crate) struct SkelGpu { pub buf: wgpu::Buffer, pub bg: wgpu::BindGroup, pub frame: u32 }

pub(crate) struct GeoGpu {
    pub vb: wgpu::Buffer,
    /// 骨付きの形の JOINTS_0・WEIGHTS_0（geometry_skinned）
    pub skin: Option<wgpu::Buffer>,
    pub ib: wgpu::Buffer,
    pub n: u32,
    pub vcount: u32,
    pub groups: Vec<Group>,
    pub center: Vec3,
    pub radius: f32,
    pub bbox: (Vec3, Vec3),
    pub draw_range: Option<(u32, u32)>,
}

pub(crate) struct ActorSlot {
    pub uid: u32,
    pub view: ActorView,
    pub actor: Option<CharacterActor>,
    pub look: ActorLook,
}

/// Kit から取り出した主役（描く物と GPU の物ごと。`kit.take_actor`）。同じ `Gpu` で作った別の Kit へ `kit.put_actor` で入れると、
/// 組み立て直さず・GPU へ上げ直さずに描ける（場所を移るたびに Kit を作り直すゲームが、主役を持ち回る）。
/// 入れずに捨てるなら `destroy_gpu`（wgpu の web の版は drop で GPU の物を返さない）
pub struct KitActor {
    view: ActorView,
    /// 動かす部品（入れ替えてよい。同じ人を runtime_template で作り直した物なら、組み立てた直後と同じ状態から始まる）
    pub actor: Option<CharacterActor>,
    /// 見せ方の値（kit.set_actor_look）
    pub look: ActorLook,
}
impl KitActor {
    /// GPU の物（体・武器・＞＜ の板の画像と列）を返して捨てる
    pub fn destroy_gpu(mut self) { self.view.destroy_gpu(); }
}

pub(crate) struct ShadowMap { pub view: wgpu::TextureView, pub size: u32 }

pub struct Kit {
    /// 場面
    pub scene: Scene,
    pub camera: Camera,
    pub fog: Option<FogExp2>,
    pub fog_linear: Option<FogLinear>,
    /// 色の出口（既定 Output::Linear）
    pub output: Output,
    /// クリアの alpha（既定 1。透ける canvas は set_canvas_alpha とあわせて 0）
    pub clear_alpha: f32,
    /// 窓でなく canvas の表示の大きさに合わせる（set_size_from_canvas）
    pub(crate) canvas_size: bool,
    /// 画面（サーフェス・canvas）を端末の画素比で取り、3D は dpr の的に描いて仕上げで広げる（set_full_dpr_surface）
    pub(crate) full_dpr_surface: bool,
    pub shared: Shared,
    pub bloom: Bloom,
    /// three の bloom.enabled（false ならブルームの計算を飛ばす。既定 true）
    pub bloom_enabled: bool,
    /// three r182 以降の光のエネルギーの式で描く（直接光のつやの多重散乱の補正・つやの分を拡散から引く。Engimeow は r186）。既定 false（three r178 の式）
    pub three_r186: bool,
    /// 物陰の影絵の色と不透明度（renderer.ts の HiddenPlayerPass）
    pub hidden_color: Color,
    pub hidden_opacity: f32,
    /// three r182 以降の PCFShadowMap（5 点の Vogel の円板を IGN で回す。半径は shadow.radius。既定 false は r180 までの PCFSoftShadowMap の式）
    pub shadow_vogel: bool,
    /// three の shadowMap.autoUpdate（既定 true で毎コマ影の型を描く）。false の間は shadow_needs_update のコマだけ描き、ほかのコマは前の型とそのときの光の行列をそのまま使う
    pub shadow_auto_update: bool,
    /// three の shadowMap.needsUpdate（描くと false に戻る）
    pub shadow_needs_update: bool,
    pub(crate) shadow_drawn: [Option<(Mat4, u32)>; 2],
    pub(crate) shadow_draws: u64,
    /// 透過用の別描画（None で描かない。既定 None）
    pub transmission: Option<Transmission>,
    pub(crate) trans_tex: Option<TexId>,
    pub(crate) trans_view0: Option<wgpu::TextureView>,
    /// 仕上げのあとに画面へ直接重ねる板（drei の Hud。sRGB のままブレンドする。毎コマ入れ直す）
    pub overlay: Vec<OverlayQuad>,
    pub(crate) overlay_gpu: overlay::OverlayGpu,

    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    /// 画面（canvas）。None は画面なしで描く（Kit::headless。Mac の試験）
    pub(crate) surface: Option<wgpu::Surface<'static>>,
    pub(crate) config: wgpu::SurfaceConfiguration,
    /// 画面なしのときの出口（config の大きさと形式）
    pub(crate) headless: Option<wgpu::Texture>,
    /// release_gpu のあと（描かない）
    pub(crate) released: bool,
    #[cfg(target_arch = "wasm32")]
    pub(crate) canvas: web_sys::HtmlCanvasElement,

    pub(crate) globals_layout: wgpu::BindGroupLayout,
    pub(crate) lights_layout: wgpu::BindGroupLayout,
    pub(crate) mat_layout: wgpu::BindGroupLayout,
    pub(crate) layout_main: wgpu::PipelineLayout,
    pub(crate) bones_layout: wgpu::BindGroupLayout,
    pub(crate) layout_main_skinned: wgpu::PipelineLayout,
    pub(crate) layout_depth_skinned: wgpu::PipelineLayout,
    pub(crate) layout_shadow_alpha_skinned: wgpu::PipelineLayout,
    pub(crate) layout_globals: wgpu::PipelineLayout,
    pub(crate) layout_shadow_alpha: wgpu::PipelineLayout,
    pub(crate) layout_hidden: wgpu::PipelineLayout,
    pub(crate) kit_shader: wgpu::ShaderModule,

    pub(crate) globals_buf: wgpu::Buffer,
    pub(crate) globals_bg: wgpu::BindGroup,
    pub(crate) shadow_globals: [(wgpu::Buffer, wgpu::BindGroup); 2],
    pub(crate) lights_buf: wgpu::Buffer,
    pub(crate) lights_bg: wgpu::BindGroup,
    pub(crate) empty_bg: wgpu::BindGroup,
    pub(crate) shadow_maps: [ShadowMap; 2],
    pub(crate) shadow_smp: wgpu::Sampler,

    pub(crate) textures: Vec<TexEntry>,
    #[cfg(target_arch = "wasm32")]
    pub(crate) staged_texture_uploads: bool,
    #[cfg(target_arch = "wasm32")]
    pub(crate) canvas_mips: Option<crate::texture::CanvasMips>,
    /// 画像のミップを GPU で作る（set_gpu_mips。既定 false は CPU の downsample）
    pub(crate) gpu_mips: bool,
    pub(crate) mips: Option<crate::texture::GpuMips>,
    pub(crate) geos: Vec<GeoGpu>,
    pub(crate) mats: Vec<MatSlot>,
    pub(crate) mat_buf: wgpu::Buffer,
    /// 材質ごとの大きい値（set_storage。group 2 binding 13 の k_storage）。無い材質は dummy_store
    pub(crate) stores: HashMap<u32, wgpu::Buffer>,
    pub(crate) dummy_store: wgpu::Buffer,
    pub(crate) samplers: HashMap<TexOpts, wgpu::Sampler>,
    /// テクスチャの組と大きい値（材質の番号。無ければ u32::MAX）ごとの bind group
    pub(crate) bg_ids: HashMap<([u32; 6], u32), u32>,
    pub(crate) bg_opts: Vec<[TexOpts; 6]>,
    pub(crate) bgs: Vec<wgpu::BindGroup>,
    pub(crate) customs: Vec<pipes::CustomEntry>,
    pub(crate) custom_ids: HashMap<(String, String, String), u32>,
    pub(crate) custom_modules: HashMap<String, wgpu::ShaderModule>,
    pub(crate) pipes: Vec<wgpu::RenderPipeline>,
    pub(crate) pipe_ids: HashMap<pipes::PipeKey, u32>,
    /// パイプラインごとの確かめの状態と、Custom のシェーダーの番号（組み込みは u32::MAX）
    pub(crate) pipe_state: Vec<pipes::PipeState>,
    pub(crate) pipe_custom: Vec<u32>,
    pub(crate) shader_sink: pipes::CheckSink,
    /// 作れなかった Custom のシェーダー（番号 → エラーの文）
    pub(crate) failed_modules: HashMap<u32, String>,
    pub(crate) reported: std::collections::HashSet<u32>,
    pub(crate) shader_errors: Vec<String>,
    pub(crate) wait_shader_check: bool,
    pub(crate) glb_tex: HashMap<(usize, usize), TexId>,
    pub(crate) glb_geo: HashMap<(usize, usize, usize), GeoId>,
    pub(crate) glb_vertex_colored: HashMap<MatId, MatId>,
    pub(crate) sprite_geo: GeoId,

    pub(crate) post: post::Post,
    pub(crate) t: post::Targets,
    /// 縁のなめらかさと、主の的のサンプル数（kit.set_aa）
    pub(crate) aa: Aa,
    pub(crate) samples: u32,

    pub(crate) stream: wgpu::Buffer,
    pub(crate) lists: frame::Lists,
    pub(crate) dyn_gpu: Vec<Option<frame::DynGpu>>,

    pub(crate) skel_gpu: HashMap<u32, SkelGpu>,
    /// 骨付きの部分のモーフ（morph.rs）: 形ごとの差分・節点ごとの重みと頂点の列
    pub(crate) morph_src: HashMap<u32, morph::MorphSrc>,
    pub(crate) morph_nodes: HashMap<u32, morph::MorphNode>,
    pub(crate) morph_scratch: Vec<Vertex>,
    pub(crate) palette: Vec<Mat4>,
    pub(crate) actors: HashMap<NodeId, ActorSlot>,
    pub(crate) actor_pipes: Option<Arc<ActorPipelines>>,
    pub(crate) actor_style: ActorStyle,
    /// NPC・汎用（character-actor-npc・character-view-npc。npc.rs）
    pub(crate) npc_actors: HashMap<NodeId, npc::NpcSlot>,
    pub(crate) npc_pipes: Option<Arc<character_view_npc::NpcPipelines>>,

    pub(crate) logical: (f32, f32),
    pub(crate) win_dpr: f32,
    pub(crate) dpr: f32,
    pub(crate) max_dpr: f32,
    pixel_budget: f32,
    user_max: f32,
    slow: i32,
    fast: i32,
    pub(crate) last_clear: Color,
}

/// 画素数の上限（重い MSAA・ブルームでも 60fps を保つ。renderer.ts）
fn pixel_cap(scale: f32, user_max: f32, budget: f32, w: f32, h: f32) -> f32 { scale.min(user_max).min((budget / (w * h).max(1.0)).sqrt()) }

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None }
}
fn smp_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }
}
fn pl(device: &wgpu::Device, label: &str, layouts: &[&wgpu::BindGroupLayout]) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &layouts.iter().map(|l| Some(*l)).collect::<Vec<_>>(), immediate_size: 0 })
}
fn depth_map(device: &wgpu::Device, size: u32) -> wgpu::TextureView {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("shadow"), size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
        dimension: wgpu::TextureDimension::D2, format: DEPTH, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
    }).create_view(&Default::default())
}

/// GPU の Device と Queue（三の WebGLRenderer の GL コンテキストに当たる）。場所を移るたびに Kit を作り直すゲームが、
/// Device を作り直さずに使い回すための物（Kit::with_gpu）。Clone は同じ Device を指す
#[derive(Clone, Debug)]
pub struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

fn instance_desc() -> wgpu::InstanceDescriptor {
    #[allow(unused_mut)]
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    #[cfg(target_arch = "wasm32")]
    { desc.backends = wgpu::Backends::BROWSER_WEBGPU; }
    desc
}

async fn request_device(adapter: &wgpu::Adapter) -> Result<(wgpu::Device, wgpu::Queue), String> {
    adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("island-kit"),
        required_features: adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
        required_limits: wgpu::Limits { max_storage_buffers_per_shader_stage: 8, ..wgpu::Limits::default() }.using_resolution(adapter.limits()),
        ..Default::default()
    }).await.map_err(|e| e.to_string())
}

impl Gpu {
    /// Kit::new と同じ求め方で Device を作る（ブラウザは WebGPU、Mac は Metal）。WebGPU が無ければ Err
    pub async fn new() -> Result<Gpu, String> {
        let instance = wgpu::Instance::new(instance_desc());
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: None, force_fallback_adapter: false, apply_limit_buckets: false }).await.map_err(|e| e.to_string())?;
        let (device, queue) = request_device(&adapter).await?;
        Ok(Gpu { instance, adapter, device, queue })
    }
}

impl Kit {
    /// canvas で始める。WebGPU が無ければ Err（画面に知らせるのはゲーム）
    #[cfg(target_arch = "wasm32")]
    pub async fn new(canvas: web_sys::HtmlCanvasElement) -> Result<Kit, String> {
        let instance = wgpu::Instance::new(instance_desc());
        let surface = instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone())).map_err(|e| e.to_string())?;
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), force_fallback_adapter: false, apply_limit_buckets: false }).await.map_err(|e| e.to_string())?;
        let (device, queue) = request_device(&adapter).await?;
        Kit::on_canvas(&adapter, device, queue, surface, canvas)
    }

    /// 前に作った Gpu（Device と Queue）を使い回して canvas で始める。場所を移るたびに Kit を作り直すゲーム（aisns の Stage）が、
    /// Device を作り直して GPU のプロセスにメモリを積まないための入口。Kit を手放す前に `release_gpu` で、この Kit が作った GPU の物を手放す
    /// （Device は destroy しない。Gpu を持っているほかの Kit と、次の Kit が使う）
    #[cfg(target_arch = "wasm32")]
    pub fn with_gpu(canvas: web_sys::HtmlCanvasElement, gpu: &Gpu) -> Result<Kit, String> {
        let surface = gpu.instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone())).map_err(|e| e.to_string())?;
        Kit::on_canvas(&gpu.adapter, gpu.device.clone(), gpu.queue.clone(), surface, canvas)
    }

    #[cfg(target_arch = "wasm32")]
    fn on_canvas(adapter: &wgpu::Adapter, device: wgpu::Device, queue: wgpu::Queue, surface: wgpu::Surface<'static>, canvas: web_sys::HtmlCanvasElement) -> Result<Kit, String> {
        let caps = surface.get_capabilities(adapter);
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let win = web_sys::window().ok_or("window が無い")?;
        let scale = win.device_pixel_ratio() as f32;
        let logical = (win.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32, win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32);
        let dpr = pixel_cap(scale, 2.0, 2.6e6, logical.0, logical.1);
        let (w, h) = ((logical.0 * dpr) as u32, (logical.1 * dpr) as u32);
        canvas.set_width(w.max(1));
        canvas.set_height(h.max(1));
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | (caps.usages & wgpu::TextureUsages::COPY_SRC), format, width: w.max(1), height: h.max(1),
            present_mode: wgpu::PresentMode::Fifo, desired_maximum_frame_latency: 2, alpha_mode: caps.alpha_modes[0], view_formats: vec![], color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);
        let mut kit = Kit::with_device(device, queue, Some(surface), config, logical, scale, dpr, canvas);
        kit.camera.aspect = logical.0 / logical.1.max(1.0);
        Ok(kit)
    }

    /// 画面なしで始める（Mac の試験。仕上げは w × h の Rgba8Unorm の画像へ描く。窓の大きさは読まない）
    #[cfg(not(target_arch = "wasm32"))]
    pub fn headless(gpu: &Gpu, w: u32, h: u32) -> Kit {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, format: wgpu::TextureFormat::Rgba8Unorm, width: w.max(1), height: h.max(1),
            present_mode: wgpu::PresentMode::Fifo, desired_maximum_frame_latency: 2, alpha_mode: wgpu::CompositeAlphaMode::Auto, view_formats: vec![], color_space: wgpu::SurfaceColorSpace::Auto,
        };
        let mut kit = Kit::with_device(gpu.device.clone(), gpu.queue.clone(), None, config, (w as f32, h as f32), 1.0, 1.0);
        kit.headless = Some(headless_texture(&kit.device, &kit.config));
        kit
    }
    /// 画面なしの出口の画像（Kit::headless の仕上げの結果。読み戻しは試験が自分で）
    pub fn headless_output(&self) -> Option<&wgpu::Texture> { self.headless.as_ref() }
    /// テクスチャの wgpu の画像（試験の読み戻しや、ゲームが自分で写すとき）
    pub fn wgpu_texture(&self, id: TexId) -> &wgpu::Texture { &self.textures[id.0 as usize].tex }

    /// この Kit が作った GPU の物（テクスチャ・描く的・影の型・形・材質の値・インスタンスの列・骨）を今すぐ destroy する。Device は残す。
    /// Kit::with_gpu で Device を使い回すとき、Kit を手放す前に呼ぶ（wgpu の web の版は drop で GPU の物を手放さない）。
    /// 呼んだあとの Kit は描かない（render・render_to・render_view_to は何もしない）。主役（character-view の ActorView）の画像と列も destroy する
    /// （持ち回る主役は、その前に take_actor で取り出す）
    pub fn release_gpu(&mut self) {
        if self.released { return; }
        self.released = true;
        for s in self.actors.values_mut() { s.view.destroy_gpu(); }
        self.actors.clear();
        self.actor_pipes = None;
        for s in self.npc_actors.values_mut() { s.view.destroy_gpu(); }
        self.npc_actors.clear();
        self.npc_pipes = None;
        for t in &self.textures {
            t.tex.destroy();
            if let Some(rt) = &t.rt { rt.msaa.texture().destroy(); rt.depth.texture().destroy(); }
        }
        for g in &self.geos { g.vb.destroy(); g.ib.destroy(); if let Some(s) = &g.skin { s.destroy(); } }
        for d in self.dyn_gpu.iter().flatten() {
            match d {
                frame::DynGpu::Inst { buf, attrs, .. } => { buf.destroy(); for a in attrs { a.destroy(); } }
                frame::DynGpu::Points { pos, attrs, .. } => { pos.destroy(); for a in attrs { a.destroy(); } }
                frame::DynGpu::Lines { pos, .. } => pos.destroy(),
            }
        }
        for s in self.skel_gpu.values() { s.buf.destroy(); }
        self.destroy_morphs();
        for b in self.stores.values() { b.destroy(); }
        for b in [&self.mat_buf, &self.dummy_store, &self.stream, &self.globals_buf, &self.lights_buf, &self.shadow_globals[0].0, &self.shadow_globals[1].0] { b.destroy(); }
        for m in &self.shadow_maps { m.view.texture().destroy(); }
        self.t.destroy();
        self.post.destroy();
        if let Some(h) = &self.headless { h.destroy(); }
        self.overlay_gpu = overlay::OverlayGpu::default();
        self.dyn_gpu.clear();
        self.skel_gpu.clear();
        self.invalidate_bind_groups();
    }

    #[allow(clippy::too_many_arguments)]
    fn with_device(device: wgpu::Device, queue: wgpu::Queue, surface: Option<wgpu::Surface<'static>>, config: wgpu::SurfaceConfiguration, logical: (f32, f32), win_dpr: f32, dpr: f32, #[cfg(target_arch = "wasm32")] canvas: web_sys::HtmlCanvasElement) -> Kit {
        let globals_layout = character_view::globals_layout(&device);
        let u_buf = |label: &str, size: u64| device.create_buffer(&wgpu::BufferDescriptor { label: Some(label), size, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let globals_size = std::mem::size_of::<Globals>() as u64;
        let globals_bind = |buf: &wgpu::Buffer| device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &globals_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }] });
        let globals_buf = u_buf("globals", globals_size);
        let globals_bg = globals_bind(&globals_buf);
        let shadow_globals = [0, 1].map(|_| { let b = u_buf("shadow globals", globals_size); let g = globals_bind(&b); (b, g) });

        let depth_entry = |binding: u32| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None };
        let lights_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("lights"), entries: &[
            depth_entry(0),
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None },
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
            depth_entry(3),
        ] });
        let mat_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("material"), entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: None }, count: None },
            tex_entry(1), smp_entry(2), tex_entry(3), smp_entry(4), tex_entry(5), smp_entry(6), tex_entry(7), smp_entry(8), tex_entry(9), smp_entry(10), tex_entry(11), smp_entry(12),
            wgpu::BindGroupLayoutEntry { binding: 13, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
        ] });
        let dummy_store = device.create_buffer(&wgpu::BufferDescriptor { label: Some("no storage"), size: 16, usage: wgpu::BufferUsages::STORAGE, mapped_at_creation: false });
        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("empty"), entries: &[] });
        let empty_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &empty_layout, entries: &[] });
        let layout_main = pl(&device, "kit main", &[&globals_layout, &lights_layout, &mat_layout]);
        let bones_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("bones"), entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Storage { read_only: true }, has_dynamic_offset: false, min_binding_size: None }, count: None },
        ] });
        let layout_main_skinned = pl(&device, "kit main skinned", &[&globals_layout, &lights_layout, &mat_layout, &bones_layout]);
        let layout_depth_skinned = pl(&device, "kit depth skinned", &[&globals_layout, &empty_layout, &empty_layout, &bones_layout]);
        let layout_shadow_alpha_skinned = pl(&device, "kit shadow a skinned", &[&globals_layout, &empty_layout, &mat_layout, &bones_layout]);
        let layout_globals = pl(&device, "kit depth", &[&globals_layout]);
        let layout_shadow_alpha = pl(&device, "kit shadow a", &[&globals_layout, &empty_layout, &mat_layout]);
        let layout_hidden = pl(&device, "kit hidden", &[&globals_layout, &lights_layout]);
        let code = format!("{}\n{}\n{}", character_view::COMMON, KIT_COMMON, include_str!("../../shaders/kit.wgsl"));
        let kit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("kit"), source: wgpu::ShaderSource::Wgsl(code.into()) });

        let shadow_smp = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("shadow"), compare: Some(wgpu::CompareFunction::LessEqual), mag_filter: wgpu::FilterMode::Nearest, min_filter: wgpu::FilterMode::Nearest, ..Default::default() });
        let shadow_maps = [ShadowMap { view: depth_map(&device, 1), size: 1 }, ShadowMap { view: depth_map(&device, 1), size: 1 }];
        let lights_buf = u_buf("lights", std::mem::size_of::<frame::LightsU>() as u64);
        let lights_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("lights"), layout: &lights_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&shadow_maps[0].view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&shadow_smp) },
            wgpu::BindGroupEntry { binding: 2, resource: lights_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&shadow_maps[1].view) },
        ] });

        let post = post::Post::new(&device, config.format);
        let t = post.make_targets(&device, config.width, config.height, dpr, SAMPLES, false);
        let mat_buf = u_buf("materials", MAT_SLOT * 256);
        let stream = device.create_buffer(&wgpu::BufferDescriptor { label: Some("instances"), size: 80 * 1024, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });

        let mut kit = Kit {
            scene: Scene::new(),
            camera: Camera { aspect: logical.0 / logical.1.max(1.0), ..Camera::default() },
            three_r186: false, fog: None, fog_linear: None, output: Output::Linear, clear_alpha: 1.0, canvas_size: false, full_dpr_surface: false, shared: Shared::default(), bloom: Bloom::default(), bloom_enabled: true, hidden_color: Color::hex(0xf4d2b0), hidden_opacity: 0.82, shadow_vogel: false, shadow_auto_update: true, shadow_needs_update: false, shadow_drawn: [None; 2], shadow_draws: 0, transmission: None, trans_tex: None, trans_view0: None, overlay: Vec::new(), overlay_gpu: overlay::OverlayGpu::default(),
            device, queue, surface, config, headless: None, released: false, #[cfg(target_arch = "wasm32")] canvas,
            globals_layout, lights_layout, mat_layout, layout_main, bones_layout, layout_main_skinned, layout_depth_skinned, layout_shadow_alpha_skinned, layout_globals, layout_shadow_alpha, layout_hidden, kit_shader,
            globals_buf, globals_bg, shadow_globals, lights_buf, lights_bg, empty_bg, shadow_maps, shadow_smp,
            textures: Vec::new(), geos: Vec::new(), mats: Vec::new(), mat_buf, stores: HashMap::new(), dummy_store, samplers: HashMap::new(), bg_ids: HashMap::new(), bg_opts: Vec::new(), bgs: Vec::new(),
            #[cfg(target_arch = "wasm32")] canvas_mips: None, gpu_mips: false, mips: None,
            #[cfg(target_arch = "wasm32")] staged_texture_uploads: false,
            customs: Vec::new(), custom_ids: HashMap::new(), custom_modules: HashMap::new(), pipes: Vec::new(), pipe_ids: HashMap::new(), pipe_state: Vec::new(), pipe_custom: Vec::new(), shader_sink: Default::default(), failed_modules: HashMap::new(), reported: Default::default(), shader_errors: Vec::new(), wait_shader_check: false, glb_tex: HashMap::new(), glb_geo: HashMap::new(), glb_vertex_colored: HashMap::new(),
            sprite_geo: GeoId(0),
            post, t, aa: Aa::Msaa4, samples: SAMPLES, stream, lists: frame::Lists::default(), dyn_gpu: Vec::new(), skel_gpu: HashMap::new(), morph_src: HashMap::new(), morph_nodes: HashMap::new(), morph_scratch: Vec::new(), palette: Vec::new(), actors: HashMap::new(), actor_pipes: None, actor_style: ActorStyle::PENGO, npc_actors: HashMap::new(), npc_pipes: None,
            logical, win_dpr, dpr, max_dpr: dpr, user_max: 2.0, pixel_budget: 2.6e6, slow: 0, fast: 0, last_clear: Color::BLACK,
        };
        // 白（map の無い材質）と Sprite の板（three の Sprite の形）
        kit.data_texture(1, 1, &[255, 255, 255, 255], true);
        kit.sprite_geo = kit.geometry(&Geometry {
            positions: vec![[-0.5, -0.5, 0.0], [0.5, -0.5, 0.0], [0.5, 0.5, 0.0], [-0.5, 0.5, 0.0]],
            normals: vec![[0.0, 0.0, 1.0]; 4],
            uvs: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
            indices: Some(vec![0, 1, 2, 0, 2, 3]),
            ..Default::default()
        });
        kit
    }

    pub fn device(&self) -> &wgpu::Device { &self.device }
    pub fn queue(&self) -> &wgpu::Queue { &self.queue }
    /// 画面のサーフェスの形式（game-ui の UiLayer・egui の Renderer を作るとき）
    pub fn surface_format(&self) -> wgpu::TextureFormat { self.config.format }

    // ---- 形・材質 ----

    /// 形を GPU へ上げる（character_view::Vertex の並びへ変換。index が無ければ作る）
    pub fn geometry(&mut self, g: &Geometry) -> GeoId {
        let (verts, idx) = vertices_of(g);
        let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("kit vb"), contents: bytemuck::cast_slice(&verts), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST });
        let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("kit ib"), contents: bytemuck::cast_slice(&idx), usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST });
        let (center, radius) = g.bounding_sphere();
        self.geos.push(GeoGpu { vb, skin: None, ib, n: idx.len() as u32, vcount: verts.len() as u32, groups: g.groups.clone(), center, radius, bbox: g.bounding_box(), draw_range: None });
        GeoId(self.geos.len() as u32 - 1)
    }

    /// 骨付きの形（three の skinIndex・skinWeight を持つ BufferGeometry）。joints は骨の組の番号、weights は足して 1（GLB なら glb::normalize_skin_weights）。scene.skinned で使う
    pub fn geometry_skinned(&mut self, g: &Geometry, joints: &[[u16; 4]], weights: &[[f32; 4]]) -> GeoId {
        let id = self.geometry(g);
        self.set_geometry_skin(id, joints, weights);
        id
    }
    pub(crate) fn set_geometry_skin(&mut self, id: GeoId, joints: &[[u16; 4]], weights: &[[f32; 4]]) {
        let skin: Vec<character_actor::actor::SkinVertex> = joints.iter().zip(weights).map(|(j, w)| character_actor::actor::SkinVertex { joints: *j, weights: *w }).collect();
        self.geos[id.0 as usize].skin = Some(self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("kit skin"), contents: bytemuck::cast_slice(&skin), usage: wgpu::BufferUsages::VERTEX }));
    }
    pub(crate) fn has_geometry_skin(&self, id: GeoId) -> bool { self.geos[id.0 as usize].skin.is_some() }

    /// 形を書き換える（three の attribute.needsUpdate。頂点と index の数が増えたら作り直す）
    pub fn update_geometry(&mut self, id: GeoId, g: &Geometry) {
        let (verts, idx) = vertices_of(g);
        let (center, radius) = g.bounding_sphere();
        let e = &mut self.geos[id.0 as usize];
        if verts.len() as u32 > e.vcount || idx.len() as u32 > e.n {
            e.vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("kit vb"), contents: bytemuck::cast_slice(&verts), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST });
            e.ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("kit ib"), contents: bytemuck::cast_slice(&idx), usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST });
            e.vcount = verts.len() as u32;
        } else {
            self.queue.write_buffer(&e.vb, 0, bytemuck::cast_slice(&verts));
            self.queue.write_buffer(&e.ib, 0, bytemuck::cast_slice(&idx));
        }
        e.n = idx.len() as u32;
        e.groups = g.groups.clone();
        e.center = center;
        e.radius = radius;
        e.bbox = g.bounding_box();
    }

    /// three の `new Box3().setFromObject(node)`（子も入れる。隠れている物も数える。何も形が無ければ None）
    pub fn box_of(&self, node: NodeId) -> Option<(Vec3, Vec3)> {
        let mut out: Option<(Vec3, Vec3)> = None;
        let mut grow = |w: Mat4, b: (Vec3, Vec3)| {
            for k in 0..8 {
                let p = Vec3::new(if k & 1 == 0 { b.0.x } else { b.1.x }, if k & 2 == 0 { b.0.y } else { b.1.y }, if k & 4 == 0 { b.0.z } else { b.1.z });
                let q = w.transform_point3(p);
                out = Some(match out { Some((lo, hi)) => (lo.min(q), hi.max(q)), None => (q, q) });
            }
        };
        let mut stack = vec![node];
        while let Some(id) = stack.pop() {
            let n = &self.scene[id];
            let w = self.scene.world_matrix(id);
            match &n.kind {
                crate::scene::NodeKind::Mesh { geo, .. } => grow(w, self.geos[geo.0 as usize].bbox),
                // バインドの姿勢の箱（three は今の姿勢の頂点を読む。使うゲームが出たら合わせる）
                crate::scene::NodeKind::Skinned(s) => grow(w, self.geos[s.geo.0 as usize].bbox),
                crate::scene::NodeKind::Instanced(s) => { let b = self.geos[s.geo.0 as usize].bbox; for k in 0..s.count { grow(w * self.scene.instance(id, k), b); } }
                crate::scene::NodeKind::Batched(s) => { for k in 0..s.len() { let (shape, visible, m, _) = s.instance(k); if visible { grow(w * m, self.geos[s.shape_geo(shape).0 as usize].bbox); } } }
                _ => {}
            }
            stack.extend_from_slice(n.children());
        }
        out
    }

    /// three の geometry.setDrawRange（index の範囲）
    pub fn set_draw_range(&mut self, id: GeoId, start: u32, count: u32) { self.geos[id.0 as usize].draw_range = Some((start, count)); }

    pub fn material(&mut self, m: Material) -> MatId {
        let i = self.mats.len() as u64;
        if (i + 1) * MAT_SLOT > self.mat_buf.size() {
            self.mat_buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("materials"), size: self.mat_buf.size() * 2, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.invalidate_bind_groups();
            for s in &mut self.mats { s.dirty = true; }
        }
        self.mats.push(MatSlot { m, pipes: [u32::MAX; 8], side_pipes: [u32::MAX; 8], bg: u32::MAX, dirty: true });
        MatId(i as u32)
    }
    pub fn mat(&self, id: MatId) -> &Material { &self.mats[id.0 as usize].m }
    /// 形の添字の数（three の index.count。値の記録など）
    pub fn index_count(&self, g: GeoId) -> u32 { self.geos[g.0 as usize].n }
    /// 値を変える（次の描画で GPU へ）
    pub fn mat_mut(&mut self, id: MatId) -> &mut Material {
        let s = &mut self.mats[id.0 as usize];
        s.dirty = true;
        s.pipes = [u32::MAX; 8];
        s.bg = u32::MAX;
        &mut s.m
    }
    /// Custom の uniform を変える（毎コマ。パイプラインは作り直さない）
    pub fn set_uniform(&mut self, id: MatId, bytes: &[u8]) {
        let s = &mut self.mats[id.0 as usize];
        if let MatKind::Custom(c) = &mut s.m.kind {
            c.uniform.clear();
            c.uniform.extend_from_slice(bytes);
            s.dirty = true;
        }
    }
    /// 256 バイトより大きい材質の値（Custom の WGSL の `k_storage: array<vec4f>`。group 2 binding 13 の読むだけの storage）。
    /// 材質ごとに 1 つ。大きさが同じか小さければ書き直すだけ（毎コマ呼んでよい）、大きくなったら作り直す。4 バイトの倍数でない分は 0 で埋める
    pub fn set_storage(&mut self, mat: MatId, bytes: &[u8]) {
        let len = (bytes.len().max(16) as u64).div_ceil(16) * 16;
        let grow = self.stores.get(&mat.0).is_none_or(|b| b.size() < len);
        if grow {
            if let Some(old) = self.stores.remove(&mat.0) { old.destroy(); }
            let b = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("material storage"), size: len, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.stores.insert(mat.0, b);
            self.bg_ids.retain(|k, _| k.1 != mat.0);
            self.mats[mat.0 as usize].bg = u32::MAX;
        }
        let buf = &self.stores[&mat.0];
        if bytes.len() % 4 == 0 { self.queue.write_buffer(buf, 0, bytes); } else {
            let mut v = bytes.to_vec();
            v.resize(bytes.len().div_ceil(4) * 4, 0);
            self.queue.write_buffer(buf, 0, &v);
        }
    }

    /// 形を手放す（three の geometry.dispose()）。GPU の頂点と添字を今すぐ destroy し、この形を使う節点は描かれなくなる（番号は再利用しない）。
    /// あとで update_geometry すると作り直して、また描ける
    pub fn destroy_geometry(&mut self, id: GeoId) {
        let e = &mut self.geos[id.0 as usize];
        e.vb.destroy();
        e.ib.destroy();
        if let Some(s) = e.skin.take() { s.destroy(); }
        e.n = 0;
        e.vcount = 0;
        e.groups.clear();
        e.draw_range = None;
        self.glb_geo.retain(|_, g| *g != id);
        self.morph_src.remove(&id.0);
    }

    /// 作れなかった Custom のシェーダーの知らせ（前に呼んでから増えた分。シェーダー 1 つにつき 1 回。ブラウザのコンソールにも出る）。
    /// 作れなかった材質は描かない（ほかの物は描く。前はそのコマの command buffer ごと無効になって、画面がまるごと止まった）。
    /// ブラウザでは確かめの結果があとのコマで届くので、届くまでの 1〜数コマは今までどおり画面が出ないことがある（set_wait_for_shader_check で避けられる）
    pub fn take_shader_errors(&mut self) -> Vec<String> { self.drain_shader_checks(); std::mem::take(&mut self.shader_errors) }
    /// 新しい Custom のシェーダーの材質を、確かめが終わるまで描かない（既定 false: 作ったコマから描く）。true なら作れない材質が
    /// 1 コマも画面を止めないが、新しい材質が出るのが確かめの分（ブラウザで 1〜数コマ）遅れる
    pub fn set_wait_for_shader_check(&mut self, on: bool) { self.wait_shader_check = on; }
    /// この材質のシェーダーを作れなかったか（描いたことのある材質だけ分かる）
    pub fn material_failed(&self, id: MatId) -> bool {
        let s = &self.mats[id.0 as usize];
        s.pipes.iter().chain(s.side_pipes.iter()).any(|p| *p != u32::MAX && self.pipe_state[*p as usize] == pipes::PipeState::Failed)
    }

    /// three の material.clone()
    pub fn clone_material(&mut self, id: MatId) -> MatId { let m = self.mats[id.0 as usize].m.clone(); self.material(m) }

    pub(crate) fn invalidate_bind_groups(&mut self) {
        self.bg_ids.clear();
        self.bg_opts.clear();
        self.bgs.clear();
        for s in &mut self.mats { s.bg = u32::MAX; }
    }

    fn sampler_for(&mut self, o: TexOpts) -> wgpu::Sampler {
        if let Some(s) = self.samplers.get(&o) { return s.clone(); }
        let addr = |w: Wrap| match w { Wrap::Clamp => wgpu::AddressMode::ClampToEdge, Wrap::Repeat => wgpu::AddressMode::Repeat, Wrap::Mirror => wgpu::AddressMode::MirrorRepeat };
        let f = |n: bool| if n { wgpu::FilterMode::Nearest } else { wgpu::FilterMode::Linear };
        let aniso_ok = !o.mag_nearest && !o.min_nearest && o.mipmap;
        let s = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("kit"), address_mode_u: addr(o.wrap_s), address_mode_v: addr(o.wrap_t),
            mag_filter: f(o.mag_nearest), min_filter: f(o.min_nearest),
            mipmap_filter: if o.mipmap && !o.min_nearest { wgpu::MipmapFilterMode::Linear } else { wgpu::MipmapFilterMode::Nearest },
            lod_max_clamp: if o.mipmap && !o.min_nearest { 32.0 } else { 0.0 },
            anisotropy_clamp: if aniso_ok { o.anisotropy.max(1) } else { 1 },
            ..Default::default()
        });
        self.samplers.insert(o, s.clone());
        s
    }

    /// 材質の group 2（テクスチャの組ごとに 1 つ。値は動的オフセット）
    pub(crate) fn mat_bind(&mut self, mat: u32) -> u32 {
        let cached = self.mats[mat as usize].bg;
        if cached != u32::MAX { return cached; }
        let slots = self.mats[mat as usize].m.tex_slots();
        let ids: [u32; 6] = slots.map(|t| t.map_or(0, |t| t.0));
        let opts: [TexOpts; 6] = ids.map(|i| self.textures[i as usize].opts);
        let store_key = if self.stores.contains_key(&mat) { mat } else { u32::MAX };
        let key = (ids, store_key);
        if let Some(&b) = self.bg_ids.get(&key) && self.bg_opts[b as usize] == opts {
            self.mats[mat as usize].bg = b;
            return b;
        }
        let smps: Vec<wgpu::Sampler> = opts.iter().map(|o| self.sampler_for(*o)).collect();
        let e = |b: u32, i: usize| wgpu::BindGroupEntry { binding: b, resource: wgpu::BindingResource::TextureView(&self.textures[ids[i] as usize].view) };
        let sm = |b: u32, i: usize| wgpu::BindGroupEntry { binding: b, resource: wgpu::BindingResource::Sampler(&smps[i]) };
        let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("material"), layout: &self.mat_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &self.mat_buf, offset: 0, size: wgpu::BufferSize::new(MAT_SLOT) }) },
            e(1, 0), sm(2, 0), e(3, 1), sm(4, 1), e(5, 2), sm(6, 2), e(7, 3), sm(8, 3), e(9, 4), sm(10, 4), e(11, 5), sm(12, 5),
            wgpu::BindGroupEntry { binding: 13, resource: self.stores.get(&mat).unwrap_or(&self.dummy_store).as_entire_binding() },
        ] });
        let id = self.bgs.len() as u32;
        self.bgs.push(bg);
        self.bg_opts.push(opts);
        self.bg_ids.insert(key, id);
        self.mats[mat as usize].bg = id;
        id
    }

    /// 変わった材質の値を GPU へ
    pub(crate) fn flush_materials(&mut self) {
        for (i, s) in self.mats.iter_mut().enumerate() {
            if !s.dirty { continue; }
            s.dirty = false;
            let off = i as u64 * MAT_SLOT;
            match &s.m.kind {
                MatKind::Custom(c) => { if !c.uniform.is_empty() { let n = c.uniform.len().min(MAT_SLOT as usize); self.queue.write_buffer(&self.mat_buf, off, &c.uniform[..n]); } }
                _ => {
                    let flip = s.m.map.is_some_and(|t| self.textures[t.0 as usize].flip_v);
                    self.queue.write_buffer(&self.mat_buf, off, bytemuck::bytes_of(&s.m.mat_u(flip)));
                }
            }
        }
    }

    // ---- 後処理 ----

    /// ゲームの仕上げ（fs_final）を渡す。決まりは README。uniform_size は @group(1) の u の大きさ
    pub fn set_final(&mut self, wgsl: &str, uniform_size: u64) { self.post.set_final(&self.device, wgsl, uniform_size); }
    /// 仕上げの値を毎コマ渡す
    pub fn set_final_uniform(&self, bytes: &[u8]) { self.post.set_final_uniform(&self.queue, bytes); }

    // ---- 大きさ ----

    /// 論理 px（three の view.size）
    pub fn size(&self) -> (f32, f32) { self.logical }
    /// 今の画素の倍率（renderer.getPixelRatio）
    pub fn dpr(&self) -> f32 { self.dpr }
    /// 色の的の大きさ（px）。仕上げの uResolution
    pub fn render_size(&self) -> (u32, u32) { (self.t.w, self.t.h) }
    /// 自機の型の 1 画素（uMaskPx）
    pub fn mask_px(&self) -> [f32; 2] { [1.0 / self.t.mw.max(1) as f32, 1.0 / self.t.mh.max(1) as f32] }

    /// 窓の大きさと倍率を読み直す（窓が変わると render が自分で呼ぶ。renderer.ts の resize）
    /// 窓でなく canvas の表示の大きさ（clientWidth・clientHeight）で描く（three の Canvas が親に合わせるのと同じ）。ページの一部だけの canvas に使う
    pub fn set_size_from_canvas(&mut self, on: bool) { self.canvas_size = on; self.resize(); }
    /// 画面（サーフェス・canvas）を端末の画素比（devicePixelRatio）で取る。3D は今までどおり dpr（画素の上限で抑えた値）の的へ描き、
    /// 仕上げで画面へ広げる（今の版で three の canvas をブラウザが CSS で広げるのと同じ）。画面の文字とボタン（game-ui）を
    /// 端末の細かさで描くため。既定は false（画面も dpr。今までと同じ）
    pub fn set_full_dpr_surface(&mut self, on: bool) { self.full_dpr_surface = on; self.apply_size(); }
    /// 画面（サーフェス）の画素比。game-ui の pixels_per_point に渡す
    pub fn surface_dpr(&self) -> f32 { if self.full_dpr_surface { self.win_dpr } else { self.dpr } }
    /// 透ける canvas（three の alpha: true）。clear_alpha とあわせて使う
    pub fn set_canvas_alpha(&mut self, on: bool) {
        self.config.alpha_mode = if on { wgpu::CompositeAlphaMode::PreMultiplied } else { wgpu::CompositeAlphaMode::Auto };
        if let Some(s) = &self.surface { s.configure(&self.device, &self.config); }
    }
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn measured(&self) -> Option<(f32, f32)> {
        if self.canvas_size {
            // 小数のまま（three の setSize は floor(幅 × dpr)。clientWidth は四捨五入なので 1 画素ずれる）。CSS の transform を受けない計算済みの値を読む
            let css = |k: &str| web_sys::window().and_then(|w| w.get_computed_style(&self.canvas).ok().flatten()).and_then(|st| st.get_property_value(k).ok()).and_then(|v| v.trim_end_matches("px").parse::<f32>().ok());
            return Some((css("width").unwrap_or(self.canvas.client_width() as f32).max(1.0), css("height").unwrap_or(self.canvas.client_height() as f32).max(1.0)));
        }
        let win = web_sys::window()?;
        Some((win.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32, win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32))
    }
    pub fn resize(&mut self) {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(win) = web_sys::window() {
                self.win_dpr = win.device_pixel_ratio() as f32;
                if self.canvas_size { if let Some(m) = self.measured() { self.logical = m; } self.max_dpr = pixel_cap(self.win_dpr, self.user_max, self.pixel_budget, self.logical.0, self.logical.1); self.dpr = self.max_dpr.min(self.dpr.max(self.max_dpr)); self.apply_size(); return; }
                self.logical = (win.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32, win.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(1.0) as f32);
            }
        }
        self.max_dpr = pixel_cap(self.win_dpr, self.user_max, self.pixel_budget, self.logical.0, self.logical.1);
        self.dpr = self.dpr.min(self.max_dpr);
        self.apply_size();
    }

    /// renderer.ts の pixelCap の画素数（既定 2.6e6。pinball は 2.0e6）
    pub fn set_pixel_budget(&mut self, px: f32) {
        self.pixel_budget = px;
        self.resize();
    }
    /// ブルームの解像度を下げる（pinball の `bloom.setSize(w / 2, h / 2)`。既定 1）。形の細い光がにじむ量が変わる
    pub fn set_bloom_div(&mut self, div: u32) {
        self.post.bloom_div = div.max(1);
        self.apply_size();
    }

    fn apply_size(&mut self) {
        let (w, h) = (((self.logical.0 * self.dpr) as u32).max(1), ((self.logical.1 * self.dpr) as u32).max(1));
        // 画面の大きさ（full_dpr_surface のときは端末の画素比。3D の的は w・h のまま）
        let (sw, sh) = if self.full_dpr_surface { (((self.logical.0 * self.win_dpr) as u32).max(1), ((self.logical.1 * self.win_dpr) as u32).max(1)) } else { (w, h) };
        #[cfg(target_arch = "wasm32")]
        { self.canvas.set_width(sw); self.canvas.set_height(sh); }
        self.config.width = sw;
        self.config.height = sh;
        if let Some(s) = &self.surface { s.configure(&self.device, &self.config); }
        if self.headless.is_some() { self.headless = Some(headless_texture(&self.device, &self.config)); }
        self.camera.aspect = self.logical.0 / self.logical.1.max(1.0);
        self.t = self.post.make_targets(&self.device, w, h, self.dpr, self.samples, self.aa == Aa::Smaa);
    }

    /// 縁のなめらかさ（既定 Aa::Msaa4）。Kit を作ってすぐ、主役（set_actor・set_npc_actor・put_actor）を入れる前に呼ぶ
    /// （主役のパイプラインは入れたときのサンプル数のまま。持ち回る主役は、同じ設定の Kit の間で回す）。
    /// 主の的と、もう作った別のカメラの的・透過用の的を作り直し、材質のパイプラインは次に描くときに作り直す
    pub fn set_aa(&mut self, aa: Aa) {
        if aa == self.aa { return; }
        debug_assert!(self.actors.is_empty() && self.npc_actors.is_empty(), "set_aa は主役を入れる前に呼ぶ");
        self.aa = aa;
        self.samples = if aa == Aa::Msaa4 { SAMPLES } else { 1 };
        if aa == Aa::Smaa { self.post.ensure_smaa(&self.device, &self.queue); }
        self.pipe_ids.clear();
        for m in &mut self.mats { m.pipes = [u32::MAX; 8]; m.side_pipes = [u32::MAX; 8]; }
        self.actor_pipes = None;
        self.npc_pipes = None;
        let samples = self.samples;
        for t in &mut self.textures {
            if let Some(rt) = &mut t.rt {
                rt.msaa.texture().destroy();
                rt.depth.texture().destroy();
                *rt = crate::texture::RenderTargetViews::new(&self.device, t.w, t.h, samples);
            }
        }
        self.apply_size();
    }
    /// 今の縁のなめらかさ
    pub fn aa(&self) -> Aa { self.aa }

    /// 画質の設定: 画素の上限（renderer.ts の setMaxPixel。keiba の「軽い」。既定 2）
    pub fn set_max_pixel(&mut self, v: f32) {
        self.user_max = v;
        self.max_dpr = pixel_cap(self.win_dpr, v, self.pixel_budget, self.logical.0, self.logical.1);
        self.dpr = self.max_dpr.min(v);
        self.apply_size();
    }

    /// 重いときは解像度を下げる（直近のフレーム時間で判断。0.75 倍まで。renderer.ts の adapt）
    pub fn adapt(&mut self, frame_ms: f32) {
        if frame_ms > 24.0 { self.slow += 1; self.fast = 0; } else if frame_ms < 13.0 { self.fast += 1; self.slow = 0; } else { self.slow = (self.slow - 1).max(0); self.fast = (self.fast - 1).max(0); }
        if self.slow > 90 && self.dpr > 0.75 { self.dpr = (self.dpr - 0.25).max(0.75); self.slow = 0; self.apply_size(); }
        if self.fast > 600 && self.dpr < self.max_dpr { self.dpr = (self.dpr + 0.25).min(self.max_dpr); self.fast = 0; self.apply_size(); }
    }

    // ---- 主役 ----

    fn actor_pipelines(&mut self) -> Arc<ActorPipelines> {
        if let Some(p) = &self.actor_pipes { return p.clone(); }
        let p = Arc::new(ActorPipelines::new(&self.device, &self.queue, &self.globals_layout, ActorTargets { color: HDR, depth: DEPTH, mask: MASK, samples: self.samples }));
        self.actor_pipes = Some(p.clone());
        p
    }

    fn default_look(&self) -> ActorLook {
        ActorLook { rim: [0.0; 3], rim_strength: 0.0, aura: [0.0; 3], aura_strength: 0.0, top: 0.0, lift: 0.0, flash: 0.0, hidden: self.hidden_color.to_array(), hidden_opacity: self.hidden_opacity }
    }

    /// 主役を入れる（GPU へ上げる。上げた形は actor から消える。前の主役と武器は片付く）
    pub fn set_actor(&mut self, node: NodeId, mut actor: CharacterActor) {
        let pipes = self.actor_pipelines();
        let look = self.actors.get(&node).map_or_else(|| self.default_look(), |s| s.look);
        let mut view = ActorView::with_pipelines(&self.device, pipes);
        view.set_style(self.actor_style);
        view.set_gpu_mips(self.gpu_mips); // kit.set_gpu_mips(true) のあとなら、主役の画像のミップも GPU で作る（character-view の GpuMips）
        view.set_actor(&self.device, &self.queue, &mut actor);
        self.npc_actors.remove(&node);
        self.put_slot(node, ActorSlot { uid: self.scene.uid(node), view, actor: Some(actor), look });
    }
    /// 節点の主役を入れ替える（前の主役の GPU の物は返す。同じ形を share_actor で使う体がまだあれば、形と画像はその体が持つ）
    fn put_slot(&mut self, node: NodeId, slot: ActorSlot) {
        if let Some(mut old) = self.actors.insert(node, slot) { old.view.destroy_gpu(); }
    }
    /// 主役を取り出す（GPU の物は返さない。入れ物の節点は残る）。同じ Gpu の別の Kit へ put_actor で入れて持ち回る
    pub fn take_actor(&mut self, node: NodeId) -> Option<KitActor> {
        self.actors.remove(&node).map(|s| KitActor { view: s.view, actor: s.actor, look: s.look })
    }
    /// take_actor で取り出した主役を入れる（同じ Gpu で作った Kit どうし。組み立て直さず・上げ直さない）。
    /// 輪郭の光の式はこの Kit の物にする。前の主役は片付く
    pub fn put_actor(&mut self, node: NodeId, a: KitActor) {
        let KitActor { mut view, actor, look } = a;
        if self.actor_pipes.is_none() { self.actor_pipes = Some(view.pipelines()); }
        view.set_style(self.actor_style);
        self.npc_actors.remove(&node);
        self.put_slot(node, ActorSlot { uid: self.scene.uid(node), view, actor, look });
    }
    /// 同じ人をもう 1 体（from の GPU の形を共有する。actor は同じ人を読み直した物）
    pub fn share_actor(&mut self, from: NodeId, node: NodeId, mut actor: CharacterActor) {
        let look = self.actors.get(&node).map_or_else(|| self.default_look(), |s| s.look);
        let view = ActorView::share_from(&self.actors[&from].view, &self.device);
        ActorView::release_shapes(&mut actor);
        self.put_slot(node, ActorSlot { uid: self.scene.uid(node), view, actor: Some(actor), look });
    }
    /// 輪郭の光の式（既定は PENGO。このあと入れる主役と、今の主役に効く）
    pub fn set_actor_style(&mut self, style: ActorStyle) {
        self.actor_style = style;
        for s in self.actors.values_mut() { s.view.set_style(style); }
        for s in self.npc_actors.values_mut() { s.view.set_style(character_view_npc::NpcStyle { flat_lift: style.flat_lift, rim_pow: style.rim_pow, sat: style.sat, top_self: style.top_self }); }
    }
    /// 主役の体の濃さ（0..1。既定 1。1 未満は半透明の描き方へ移る。武器は薄くしない。character-view の ActorView::set_opacity。aisns の仲間がカメラを遮るとき）
    pub fn set_actor_opacity(&mut self, node: NodeId, k: f32) { if let Some(s) = self.actors.get_mut(&node) { s.view.set_opacity(k); } }
    /// カメラと自分の間にいる主役を網目に抜いて透かす（0..1。既定 0。keiba の uFade）
    pub fn set_actor_fade(&mut self, node: NodeId, k: f32) { if let Some(s) = self.actors.get_mut(&node) { s.view.set_fade(k); } }
    /// 光なしの体に夕日の陰と縁の光を重ねる（Engimeow の RimLit。k = 0 で切る。既定 0）
    /// 主役の体の色に一律に掛ける（ActorView::set_tint）
    pub fn set_actor_tint(&mut self, node: NodeId, r: f32, g: f32, b: f32) { if let Some(s) = self.actors.get_mut(&node) { s.view.set_tint([r, g, b]); } }
    pub fn set_actor_sun_rim(&mut self, node: NodeId, color: Color, k: f32) { if let Some(s) = self.actors.get_mut(&node) { s.view.set_sun_rim([color.r, color.g, color.b], k); } }
    /// 主役を外す（入れ物は残る）
    pub fn clear_actor(&mut self, node: NodeId) { if let Some(mut s) = self.actors.remove(&node) { s.view.destroy_gpu(); } }
    pub fn has_actor(&self, node: NodeId) -> bool { self.actors.get(&node).is_some_and(|s| s.actor.is_some()) }
    /// 武器の模型（どの模型か・長さはゲーム。持たせるのは actor.hold_weapon）
    pub fn set_actor_weapon(&mut self, node: NodeId, g: &Glb) {
        if let Some(s) = self.actors.get_mut(&node) { s.view.set_weapon(&self.device, &self.queue, g); }
    }
    pub fn clear_actor_weapon(&mut self, node: NodeId) { if let Some(s) = self.actors.get_mut(&node) { s.view.clear_weapon(); } }
    /// 武器を光なしで描く（three の MeshBasicMaterial）。既定は光あり
    pub fn set_actor_weapon_unlit(&mut self, node: NodeId, on: bool) { if let Some(s) = self.actors.get_mut(&node) { s.view.set_weapon_unlit(on); } }
    /// 動かす部品（入っていなければ止まる。has_actor で確かめる）
    pub fn actor_mut(&mut self, node: NodeId) -> &mut CharacterActor { self.actors.get_mut(&node).and_then(|s| s.actor.as_mut()).expect("主役が入っていない") }
    pub fn actor(&self, node: NodeId) -> &CharacterActor { self.actors.get(&node).and_then(|s| s.actor.as_ref()).expect("主役が入っていない") }
    /// 見せ方の値（輪郭の光・闘気など。物陰の影絵の色は kit.hidden_color）
    pub fn set_actor_look(&mut self, node: NodeId, look: ActorLook) { if let Some(s) = self.actors.get_mut(&node) { s.look = look; } }
    /// 入れ物の世界の行列（毎コマ set_parent_world に渡す）
    pub fn actor_world(&self, node: NodeId) -> Mat4 { self.scene.world_matrix(node) }
}

fn headless_texture(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("headless"), size: wgpu::Extent3d { width: config.width, height: config.height, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
        dimension: wgpu::TextureDimension::D2, format: config.format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
    })
}

fn vertices_of(g: &Geometry) -> (Vec<Vertex>, Vec<u32>) {
    let n = g.positions.len();
    let mut verts = Vec::with_capacity(n);
    for i in 0..n {
        verts.push(Vertex {
            pos: g.positions[i],
            normal: g.normals.get(i).copied().unwrap_or([0.0; 3]),
            uv: g.uvs.get(i).copied().unwrap_or([0.0; 2]),
            color: g.colors.get(i).copied().unwrap_or([1.0; 4]),
            sway: g.sway.get(i).copied().unwrap_or(0.0),
        });
    }
    let idx = g.indices.clone().unwrap_or_else(|| (0..n as u32).collect());
    (verts, idx)
}
