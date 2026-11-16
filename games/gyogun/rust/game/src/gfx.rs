// 描画の土台（renderer.ts の役）: wgpu の初期化・世界の値・影の型・HDR の的・後処理（保護 → ブルーム → 仕上げ）・物陰の自機の深度比較に使う的。
use crate::geom::Geo;
use bytemuck::{Pod, Zeroable};
pub use character_view::{BLEND_NORMAL, COMMON, Globals, SKIN_ATTRS, VERT_ATTRS, skin_layout, vert_layout};
use glam::{Mat4, Vec2};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

pub const HDR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
pub const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const MASK: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
pub const SAMPLES: u32 = 4;
pub const SHADOW_SIZE: u32 = 2048;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct FinalU {
    pub res: [f32; 2],
    pub center: [f32; 2],
    pub mask_px: [f32; 2],
    pub time: f32,
    pub vignette: f32,
    pub flash: f32,
    pub desat: f32,
    pub aberration: f32,
    pub zoom: f32,
    pub speed: f32,
    pub gold: f32,
    pub red: f32,
    pub outline_on: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BloomU { dir: [f32; 2], inv: [f32; 2], threshold: f32, strength: f32, radius: f32, kernel: f32 }

/// 1 体ごとの行列と fx（horde の aFx など）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Inst {
    pub model: [[f32; 4]; 4],
    pub fx: [f32; 4],
}
impl Inst {
    pub fn new(m: Mat4, fx: [f32; 4]) -> Self { Inst { model: m.to_cols_array_2d(), fx } }
}

/// 見せ方の 1 つ（弧・輪・亀裂・柱・球・予告・札）。uniform の代わりに 1 個ごとの値で渡す
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct FxInst {
    pub model: [[f32; 4]; 4],
    pub a: [f32; 4],
    pub b: [f32; 4],
    pub c: [f32; 4],
    pub d: [f32; 4],
}

pub const INST_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4, 12 => Float32x4];
pub const FX_ATTRS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4, 12 => Float32x4, 13 => Float32x4, 14 => Float32x4, 15 => Float32x4];

pub fn inst_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Inst>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &INST_ATTRS }
}
pub fn fx_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<FxInst>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &FX_ATTRS }
}

pub struct MeshBuf {
    pub vb: wgpu::Buffer,
    pub ib: wgpu::Buffer,
    pub n: u32,
}

/// 線形の HDR 色（three の Color('#rrggbb') と同じく sRGB から直す）
pub fn hex(s: &str) -> [f32; 3] {
    let s = s.trim_start_matches('#');
    let v = u32::from_str_radix(s, 16).unwrap_or(0xffffff);
    [srgb((v >> 16 & 255) as f32 / 255.0), srgb((v >> 8 & 255) as f32 / 255.0), srgb((v & 255) as f32 / 255.0)]
}
pub fn srgb(c: f32) -> f32 { if c < 0.04045 { c * 0.0773993808 } else { (c * 0.9478672986 + 0.0521327014).powf(2.4) } }
pub fn lerp3(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] { [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k] }

pub const BLEND_ADD: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
};
pub const BLEND_MULTIPLY: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Dst, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::Zero, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
};

/// 描画パイプラインの作り方（three の Material の設定と同じ並び）
pub struct Pipe<'a> {
    pub label: &'a str,
    pub shader: &'a wgpu::ShaderModule,
    pub vs: &'a str,
    pub fs: Option<&'a str>,
    pub layouts: &'a [&'a wgpu::BindGroupLayout],
    pub vbufs: &'a [wgpu::VertexBufferLayout<'a>],
    pub blend: Option<wgpu::BlendState>,
    pub cull: Option<wgpu::Face>,
    pub depth_write: bool,
    pub depth: wgpu::CompareFunction,
    pub bias: i32,
    pub topology: wgpu::PrimitiveTopology,
    /// 色の的（None は影の型）
    pub format: Option<wgpu::TextureFormat>,
    pub samples: u32,
}
impl<'a> Pipe<'a> {
    pub fn scene(label: &'a str, shader: &'a wgpu::ShaderModule, vs: &'a str, fs: &'a str, layouts: &'a [&'a wgpu::BindGroupLayout], vbufs: &'a [wgpu::VertexBufferLayout<'a>]) -> Self {
        Pipe { label, shader, vs, fs: Some(fs), layouts, vbufs, blend: None, cull: Some(wgpu::Face::Back), depth_write: true, depth: wgpu::CompareFunction::Less, bias: 0, topology: wgpu::PrimitiveTopology::TriangleList, format: Some(HDR), samples: SAMPLES }
    }
    pub fn transparent(mut self, blend: wgpu::BlendState) -> Self { self.blend = Some(blend); self.depth_write = false; self }
    pub fn double(mut self) -> Self { self.cull = None; self }
    pub fn shadow(label: &'a str, shader: &'a wgpu::ShaderModule, vs: &'a str, layouts: &'a [&'a wgpu::BindGroupLayout], vbufs: &'a [wgpu::VertexBufferLayout<'a>]) -> Self {
        Pipe { label, shader, vs, fs: None, layouts, vbufs, blend: None, cull: None, depth_write: true, depth: wgpu::CompareFunction::Less, bias: 0, topology: wgpu::PrimitiveTopology::TriangleList, format: None, samples: 1 }
    }
    pub fn mask(label: &'a str, shader: &'a wgpu::ShaderModule, vs: &'a str, fs: &'a str, layouts: &'a [&'a wgpu::BindGroupLayout], vbufs: &'a [wgpu::VertexBufferLayout<'a>]) -> Self {
        Pipe { label, shader, vs, fs: Some(fs), layouts, vbufs, blend: None, cull: Some(wgpu::Face::Back), depth_write: true, depth: wgpu::CompareFunction::Less, bias: 0, topology: wgpu::PrimitiveTopology::TriangleList, format: Some(MASK), samples: 1 }
    }
    pub fn build(self, device: &wgpu::Device) -> wgpu::RenderPipeline {
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some(self.label),
            bind_group_layouts: &self.layouts.iter().map(|l| Some(*l)).collect::<Vec<_>>(),
            immediate_size: 0,
        });
        let vbufs: Vec<Option<wgpu::VertexBufferLayout<'a>>> = self.vbufs.iter().map(|b| Some(b.clone())).collect();
        // 影の型は色の的なし（空にしないと、的のないパスと合わない）
        let target = self.format.map(|format| wgpu::ColorTargetState { format, blend: self.blend, write_mask: wgpu::ColorWrites::ALL });
        let targets: Vec<Option<wgpu::ColorTargetState>> = target.into_iter().map(Some).collect();
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(self.label),
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: self.shader, entry_point: Some(self.vs), buffers: &vbufs, compilation_options: Default::default() },
            fragment: self.fs.map(|fs| wgpu::FragmentState { module: self.shader, entry_point: Some(fs), targets: &targets, compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState { topology: self.topology, cull_mode: self.cull, ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH,
                depth_write_enabled: Some(self.depth_write),
                depth_compare: Some(self.depth),
                stencil: Default::default(),
                bias: wgpu::DepthBiasState { constant: 0, slope_scale: self.bias as f32, clamp: 0.0 },
            }),
            multisample: wgpu::MultisampleState { count: self.samples, ..Default::default() },
            multiview_mask: None,
            cache: None,
        })
    }
}

struct Targets {
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolved_bg: wgpu::BindGroup,
    resolved: wgpu::TextureView,
    clean: wgpu::TextureView,
    clean_bg: wgpu::BindGroup,
    mask: wgpu::TextureView,
    mask_depth: wgpu::TextureView,
    mask_bg: wgpu::BindGroup,
    bright: wgpu::TextureView,
    bright_bg: wgpu::BindGroup,
    /// mip ごとの横・縦のぼかし
    blur: Vec<BlurMip>,
    comp: wgpu::TextureView,
    comp_bg: wgpu::BindGroup,
    comp_tex_bg: wgpu::BindGroup,
    w: u32,
    h: u32,
    mw: u32,
    mh: u32,
}
struct BlurMip {
    h_view: wgpu::TextureView,
    v_view: wgpu::TextureView,
    h_u: wgpu::BindGroup,
    v_u: wgpu::BindGroup,
    h_tex: wgpu::BindGroup,
    v_tex: wgpu::BindGroup,
}

pub struct Gfx {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub surface: wgpu::Surface<'static>,
    pub config: wgpu::SurfaceConfiguration,
    pub globals_buf: wgpu::Buffer,
    pub globals_bg: wgpu::BindGroup,
    pub globals_layout: wgpu::BindGroupLayout,
    pub shadow_layout: wgpu::BindGroupLayout,
    pub shadow_bg: wgpu::BindGroup,
    pub shadow_view: wgpu::TextureView,
    /// 色のテクスチャ 1 枚とサンプラー（group 2）
    pub tex_layout: wgpu::BindGroupLayout,
    pub linear: wgpu::Sampler,
    pub clamp: wgpu::Sampler,
    pub white: wgpu::TextureView,
    post_tex_layout: wgpu::BindGroupLayout,
    post_u_layout: wgpu::BindGroupLayout,
    post_mips_layout: wgpu::BindGroupLayout,
    post_mask_layout: wgpu::BindGroupLayout,
    p_sanitize: wgpu::RenderPipeline,
    p_high: wgpu::RenderPipeline,
    p_blur: wgpu::RenderPipeline,
    p_composite: wgpu::RenderPipeline,
    p_blend: wgpu::RenderPipeline,
    p_final: wgpu::RenderPipeline,
    high_u: wgpu::BindGroup,
    comp_u: wgpu::BindGroup,
    final_buf: wgpu::Buffer,
    final_u: wgpu::BindGroup,
    t: Targets,
    /// 実際の窓の大きさ（論理 px）と倍率
    pub logical: (f32, f32),
    pub scale: f32,
    /// 画素の倍率（重いときは下げる）
    pub dpr: f32,
    max_dpr: f32,
    slow: i32,
    fast: i32,
    /// 重いときに解像度を下げるか（性能の比較では固定する）
    pub adapt_on: bool,
    /// 色の的の大きさを固定する（比較用）
    pub fixed: Option<(u32, u32)>,
}

fn post_shader(device: &wgpu::Device) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("post"), source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/post.wgsl").into()) })
}

fn target(device: &wgpu::Device, w: u32, h: u32, format: wgpu::TextureFormat, samples: u32, label: &str) -> wgpu::TextureView {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label), size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: samples,
        dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
    }).create_view(&Default::default())
}

fn tex_bg(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView, smp: &wgpu::Sampler) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout, entries: &[
        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(smp) },
    ] })
}
fn u_bg(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, buf: &wgpu::Buffer) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }] })
}

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None }
}
fn smp_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }
}

impl Gfx {
    pub async fn new(window: Arc<Window>) -> Result<Gfx, String> {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        // ブラウザは WebGPU だけ（骨を storage buffer で渡すので WebGL2 では動かない）
        if cfg!(target_arch = "wasm32") { desc.backends = wgpu::Backends::BROWSER_WEBGPU; }
        let instance = wgpu::Instance::new(desc);
        let surface = instance.create_surface(window.clone()).map_err(|e| e.to_string())?;
        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: Some(&surface), force_fallback_adapter: false, apply_limit_buckets: false }).await.map_err(|e| e.to_string())?;
        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("gyogun"),
            required_features: adapter.features() & wgpu::Features::TEXTURE_ADAPTER_SPECIFIC_FORMAT_FEATURES,
            required_limits: wgpu::Limits { max_storage_buffers_per_shader_stage: 8, ..wgpu::Limits::default() }.using_resolution(adapter.limits()),
            ..Default::default()
        }).await.map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | (caps.usages & wgpu::TextureUsages::COPY_SRC), format, width: size.width.max(1), height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo, desired_maximum_frame_latency: 2, alpha_mode: caps.alpha_modes[0], view_formats: vec![], color_space: wgpu::SurfaceColorSpace::Auto,
        };
        surface.configure(&device, &config);

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("globals"), size: std::mem::size_of::<Globals>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let globals_layout = character_view::globals_layout(&device);
        let globals_bg = u_bg(&device, &globals_layout, &globals_buf);

        let shadow_view = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("shadow"), size: wgpu::Extent3d { width: SHADOW_SIZE, height: SHADOW_SIZE, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: DEPTH, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        }).create_view(&Default::default());
        let shadow_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("shadow"), entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Depth, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison), count: None },
        ] });
        let shadow_smp = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("shadow"), compare: Some(wgpu::CompareFunction::LessEqual), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let shadow_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &shadow_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&shadow_view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&shadow_smp) },
        ] });

        let tex_layout = character_view::tex_layout(&device);
        let linear = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"), address_mode_u: wgpu::AddressMode::Repeat, address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::MipmapFilterMode::Linear, anisotropy_clamp: 8, ..Default::default()
        });
        let clamp = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("clamp"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });

        let white_t = device.create_texture_with_data(&queue, &wgpu::TextureDescriptor {
            label: Some("white"), size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8UnormSrgb, usage: wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[],
        }, wgpu::wgt::TextureDataOrder::default(), &[255, 255, 255, 255]);
        let white = white_t.create_view(&Default::default());

        // 後処理
        let post_tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("post tex"), entries: &[tex_entry(0), smp_entry(1)] });
        let post_u_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("post u"), entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
        }] });
        let post_mips_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("post mips"), entries: &[tex_entry(0), tex_entry(1), tex_entry(2), tex_entry(3)] });
        let post_mask_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("post mask"), entries: &[tex_entry(0)] });
        let shader = post_shader(&device);
        let fullscreen = |label: &str, fs: &str, layouts: &[&wgpu::BindGroupLayout], format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>| {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &layouts.iter().map(|l| Some(*l)).collect::<Vec<_>>(), immediate_size: 0 });
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label), layout: Some(&layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_full"), buffers: &[], compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some(fs), targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
                primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
            })
        };
        let p_sanitize = fullscreen("sanitize", "fs_sanitize", &[&post_tex_layout], HDR, None);
        let p_high = fullscreen("high", "fs_high", &[&post_tex_layout, &post_u_layout], HDR, None);
        let p_blur = fullscreen("blur", "fs_blur", &[&post_tex_layout, &post_u_layout], HDR, None);
        let p_composite = fullscreen("composite", "fs_composite", &[&post_tex_layout, &post_u_layout, &post_mips_layout], HDR, None);
        let p_blend = fullscreen("blend", "fs_copy", &[&post_tex_layout], HDR, Some(BLEND_ADD));
        let p_final = fullscreen("final", "fs_final", &[&post_tex_layout, &post_u_layout, &post_mask_layout], format, None);
        let bloom_u = |dir: [f32; 2], inv: [f32; 2], kernel: f32| device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bloom"), contents: bytemuck::bytes_of(&BloomU { dir, inv, threshold: 1.2, strength: 0.4, radius: 0.5, kernel }), usage: wgpu::BufferUsages::UNIFORM,
        });
        let high_u = u_bg(&device, &post_u_layout, &bloom_u([0.0, 0.0], [0.0, 0.0], 1.0));
        let comp_u = u_bg(&device, &post_u_layout, &bloom_u([0.0, 0.0], [0.0, 0.0], 1.0));
        let final_buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("final"), size: std::mem::size_of::<FinalU>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let final_u = u_bg(&device, &post_u_layout, &final_buf);

        let scale = window.scale_factor() as f32;
        let logical = (size.width as f32 / scale, size.height as f32 / scale);
        let max_dpr = pixel_cap(scale, logical.0, logical.1);
        let t = make_targets(&device, (logical.0 * max_dpr) as u32, (logical.1 * max_dpr) as u32, max_dpr, &post_tex_layout, &post_u_layout, &post_mips_layout, &post_mask_layout, &clamp);
        Ok(Gfx {
            device, queue, surface, config, globals_buf, globals_bg, globals_layout, shadow_layout, shadow_bg, shadow_view, tex_layout, linear, clamp, white,
            post_tex_layout, post_u_layout, post_mips_layout, post_mask_layout, p_sanitize, p_high, p_blur, p_composite, p_blend, p_final, high_u, comp_u, final_buf, final_u, t,
            logical, scale, dpr: max_dpr, max_dpr, slow: 0, fast: 0, adapt_on: true, fixed: None,
        })
    }

    pub fn shader(&self, label: &str, src: &str) -> wgpu::ShaderModule {
        character_view::shader(&self.device, label, src)
    }

    pub fn mesh(&self, g: &Geo) -> MeshBuf {
        MeshBuf {
            vb: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&g.v), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST }),
            ib: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&g.i), usage: wgpu::BufferUsages::INDEX }),
            n: g.i.len() as u32,
        }
    }

    pub fn vbuf(&self, bytes: u64) -> wgpu::Buffer {
        self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: bytes.max(16), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
    }

    /// 色のテクスチャ（ミップ付き）。srgb は sRGB として読む
    pub fn texture(&self, w: u32, h: u32, rgba: &[u8], srgb_data: bool) -> wgpu::TextureView {
        character_view::texture(&self.device, &self.queue, w, h, rgba, srgb_data)
    }

    pub fn tex_bind(&self, view: &wgpu::TextureView) -> wgpu::BindGroup { tex_bg(&self.device, &self.tex_layout, view, &self.linear) }

    pub fn resize(&mut self, w: u32, h: u32, scale: f32) {
        if w == 0 || h == 0 { return; }
        self.config.width = w; self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.scale = scale;
        self.logical = (w as f32 / scale, h as f32 / scale);
        self.max_dpr = pixel_cap(scale, self.logical.0, self.logical.1);
        self.dpr = self.dpr.min(self.max_dpr);
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let (w, h) = self.fixed.unwrap_or(((self.logical.0 * self.dpr) as u32, (self.logical.1 * self.dpr) as u32));
        self.t = make_targets(&self.device, w, h, self.dpr, &self.post_tex_layout, &self.post_u_layout, &self.post_mips_layout, &self.post_mask_layout, &self.clamp);
    }

    pub fn set_fixed(&mut self, w: u32, h: u32) { self.fixed = Some((w, h)); self.adapt_on = false; self.rebuild(); }

    /// 重いときは解像度を下げる（直近のフレーム時間で判断）
    pub fn adapt(&mut self, frame_ms: f32) {
        if !self.adapt_on { return; }
        if frame_ms > 24.0 { self.slow += 1; self.fast = 0; } else if frame_ms < 13.0 { self.fast += 1; self.slow = 0; } else { self.slow = (self.slow - 1).max(0); self.fast = (self.fast - 1).max(0); }
        if self.slow > 90 && self.dpr > 0.75 { self.dpr = (self.dpr - 0.25).max(0.75); self.slow = 0; self.rebuild(); }
        if self.fast > 600 && self.dpr < self.max_dpr { self.dpr = (self.dpr + 0.25).min(self.max_dpr); self.fast = 0; self.rebuild(); }
    }

    /// 窓の縦横比（three の camera.aspect = w / h。的の整数の大きさからにすると 1 画素ぶんずれる）
    pub fn aspect(&self) -> f64 { self.logical.0 as f64 / (self.logical.1 as f64).max(1.0) }
    pub fn render_size(&self) -> (u32, u32) { (self.t.w, self.t.h) }

    /// 主の的（MSAA の色と深度）
    pub fn main_targets(&self) -> (&wgpu::TextureView, &wgpu::TextureView, &wgpu::TextureView) { (&self.t.msaa, &self.t.depth, &self.t.resolved) }
    pub fn mask_targets(&self) -> (&wgpu::TextureView, &wgpu::TextureView) { (&self.t.mask, &self.t.mask_depth) }

    /// 後処理を積んで、画面へ出す
    pub fn post(&self, enc: &mut wgpu::CommandEncoder, screen: &wgpu::TextureView, mut f: FinalU) {
        f.res = [self.t.w as f32, self.t.h as f32];
        f.mask_px = [1.0 / self.t.mw as f32, 1.0 / self.t.mh as f32];
        self.queue.write_buffer(&self.final_buf, 0, bytemuck::bytes_of(&f));
        let t = &self.t;
        let pass = |enc: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, load: wgpu::LoadOp<wgpu::Color>, pipe: &wgpu::RenderPipeline, bgs: &[&wgpu::BindGroup]| {
            let mut p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view, resolve_target: None, ops: wgpu::Operations { load, store: wgpu::StoreOp::Store }, depth_slice: None })],
                ..Default::default()
            });
            p.set_pipeline(pipe);
            for (i, b) in bgs.iter().enumerate() { p.set_bind_group(i as u32, Some(*b), &[]); }
            p.draw(0..3, 0..1);
        };
        let clear = wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT);
        pass(enc, &t.clean, clear, &self.p_sanitize, &[&t.resolved_bg]);
        pass(enc, &t.bright, clear, &self.p_high, &[&t.clean_bg, &self.high_u]);
        for (i, m) in t.blur.iter().enumerate() {
            let src = if i == 0 { &t.bright_bg } else { &t.blur[i - 1].v_tex };
            pass(enc, &m.h_view, clear, &self.p_blur, &[src, &m.h_u]);
            pass(enc, &m.v_view, clear, &self.p_blur, &[&m.h_tex, &m.v_u]);
        }
        pass(enc, &t.comp, clear, &self.p_composite, &[&t.blur[0].v_tex, &self.comp_u, &t.comp_bg]);
        pass(enc, &t.clean, wgpu::LoadOp::Load, &self.p_blend, &[&t.comp_tex_bg]);
        pass(enc, screen, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.p_final, &[&t.clean_bg, &self.final_u, &t.mask_bg]);
        let _ = &t.blur[0].h_tex;
    }
}

/// 画素数の上限（重い MSAA・ブルームでも 60fps を保つ）
fn pixel_cap(scale: f32, w: f32, h: f32) -> f32 { scale.min(2.0).min((2.6e6 / (w * h).max(1.0)).sqrt()) }

#[allow(clippy::too_many_arguments)]
fn make_targets(device: &wgpu::Device, w: u32, h: u32, dpr: f32, tl: &wgpu::BindGroupLayout, ul: &wgpu::BindGroupLayout, ml: &wgpu::BindGroupLayout, kl: &wgpu::BindGroupLayout, smp: &wgpu::Sampler) -> Targets {
    let msaa = target(device, w, h, HDR, SAMPLES, "msaa");
    let depth = target(device, w, h, DEPTH, SAMPLES, "depth");
    let resolved = target(device, w, h, HDR, 1, "resolved");
    let clean = target(device, w, h, HDR, 1, "clean");
    let mk = (dpr.min(1.5) / dpr.max(0.01)).min(1.0);
    let (mw, mh) = (((w as f32) * mk).round() as u32, ((h as f32) * mk).round() as u32);
    let mask = target(device, mw, mh, MASK, 1, "mask");
    let mask_depth = target(device, mw, mh, DEPTH, 1, "mask depth");
    let (mut rx, mut ry) = (((w as f32) / 2.0).round() as u32, ((h as f32) / 2.0).round() as u32);
    let bright = target(device, rx, ry, HDR, 1, "bright");
    let comp = target(device, rx, ry, HDR, 1, "comp");
    let kernels = [3.0f32, 5.0, 7.0, 9.0, 11.0];
    let mut blur = Vec::new();
    for k in kernels {
        let (rw, rh) = (rx.max(1), ry.max(1));
        let h_view = target(device, rw, rh, HDR, 1, "blur h");
        let v_view = target(device, rw, rh, HDR, 1, "blur v");
        let make = |dir: [f32; 2]| {
            let b = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("blur"), contents: bytemuck::bytes_of(&BloomU { dir, inv: [1.0 / rw as f32, 1.0 / rh as f32], threshold: 1.2, strength: 0.4, radius: 0.5, kernel: k }), usage: wgpu::BufferUsages::UNIFORM });
            u_bg(device, ul, &b)
        };
        blur.push(BlurMip { h_u: make([1.0, 0.0]), v_u: make([0.0, 1.0]), h_tex: tex_bg(device, tl, &h_view, smp), v_tex: tex_bg(device, tl, &v_view, smp), h_view, v_view });
        rx = ((rx as f32) / 2.0).round() as u32;
        ry = ((ry as f32) / 2.0).round() as u32;
    }
    let comp_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: ml, entries: &[
        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&blur[1].v_view) },
        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&blur[2].v_view) },
        wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&blur[3].v_view) },
        wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&blur[4].v_view) },
    ] });
    let mask_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: kl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&mask) }] });
    Targets {
        resolved_bg: tex_bg(device, tl, &resolved, smp), clean_bg: tex_bg(device, tl, &clean, smp), bright_bg: tex_bg(device, tl, &bright, smp), comp_tex_bg: tex_bg(device, tl, &comp, smp),
        msaa, depth, resolved, clean, mask, mask_depth, mask_bg, bright, blur, comp, comp_bg, w, h, mw, mh,
    }
}

/// カメラ（three の PerspectiveCamera の必要な分）。位置と回転は three と同じく倍精度で持つ
#[derive(Clone, Copy)]
pub struct Camera {
    pub pos: glam::DVec3,
    pub look: glam::DVec3,
    pub roll: f64,
    pub fov: f64,
    pub aspect: f64,
    /// 前のコマの回転。three の lookAt は「今の位置と前のコマの回転」で matrixWorldInverse を作るので、描く前の project（画面の文字・仕上げの中心）はこの回転を使う
    pub prev_rot: glam::DMat4,
}
impl Camera {
    pub fn new(pos: glam::DVec3, look: glam::DVec3, roll: f64, fov: f64, aspect: f64, prev_rot: glam::DMat4) -> Camera { Camera { pos, look, roll, fov, aspect, prev_rot } }
    pub fn view_d(&self) -> glam::DMat4 { glam::DMat4::from_rotation_z(-self.roll) * glam::DMat4::look_at_rh(self.pos, self.look, glam::DVec3::Y) }
    pub fn view(&self) -> Mat4 { self.view_d().as_mat4() }
    pub fn proj_d(&self) -> glam::DMat4 { glam::DMat4::perspective_rh(self.fov.to_radians(), self.aspect, 0.3, 1200.0) }
    pub fn proj(&self) -> Mat4 { self.proj_d().as_mat4() }
    /// このコマの回転（平行移動を除いた view）。次のコマの prev_rot になる
    pub fn rot(&self) -> glam::DMat4 { let mut v = self.view_d(); v.w_axis = glam::DVec4::W; v }
    /// three の Vector3.project と同じ行列（投影 × 前のコマの回転 × 今の位置）
    pub fn project_pv(&self) -> glam::DMat4 { self.proj_d() * self.prev_rot * glam::DMat4::from_translation(-self.pos) }
    /// 世界の点を画面の 0..1 へ（原点は左下）
    pub fn project(&self, p: glam::DVec3) -> Vec2 {
        let c = self.project_pv() * p.extend(1.0);
        Vec2::new((c.x / c.w * 0.5 + 0.5) as f32, (c.y / c.w * 0.5 + 0.5) as f32)
    }
    pub fn pos32(&self) -> glam::Vec3 { self.pos.as_vec3() }
}

#[cfg(test)]
mod camera_tests {
    use super::*;
    // 前のコマの回転で投影する（three の lookAt の前の matrixWorldInverse）。今のコマの回転と違う値になる
    #[test]
    fn project_uses_previous_rotation() {
        use glam::{DMat4, DVec3};
        let c0 = Camera::new(DVec3::new(0.0, 6.3, 39.94), DVec3::new(0.0, 1.3, 30.03), 0.0, 54.0, 1280.0 / 720.0, DMat4::IDENTITY);
        let c1 = Camera::new(DVec3::new(0.0, 6.3, 39.90), DVec3::new(0.0, 1.3, 30.06), 0.0, 54.0, 1280.0 / 720.0, c0.rot());
        let p = DVec3::new(0.0, 1.2, 29.9);
        let stale = c1.project(p).y;
        let cur = Camera { prev_rot: c1.rot(), ..c1 }.project(p).y;
        println!("{stale} {cur}");
        assert!((stale - cur).abs() > 1e-4);
    }
}
