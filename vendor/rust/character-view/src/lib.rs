// 主役を wgpu で描く部品（共通ルール「主役キャラとゲームの境目」の characters の側）。動きは character-actor、世界の位置・カメラ・光の値はゲーム。
// ここに世界の値の型 Globals と common.wgsl、頂点の並びも置く（ゲームのシェーダーも同じものを読む）。
mod actor_view;
pub use actor_view::{ActorFrameStats, ActorLook, ActorOpaqueBatch, ActorPipelines, ActorStyle, ActorTargets, ActorView};

use bytemuck::{Pod, Zeroable};
pub use character_actor::Vertex;

pub const COMMON: &str = include_str!("../shaders/common.wgsl");

/// どのシェーダーからも読む世界の値（common.wgsl の Globals）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    pub view: [[f32; 4]; 4],
    pub proj: [[f32; 4]; 4],
    pub view_proj: [[f32; 4]; 4],
    pub light_vp: [[f32; 4]; 4],
    pub cam: [f32; 4],
    pub hemi_sky: [f32; 4],
    pub hemi_ground: [f32; 4],
    pub key_color: [f32; 4],
    pub key_dir: [f32; 4],
    pub fog: [f32; 4],
    pub rim: [f32; 4],
    pub misc: [f32; 4],
}

pub const VERT_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x4, 4 => Float32];
pub const SKIN_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![5 => Uint16x4, 6 => Float32x4];

pub fn vert_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<Vertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &VERT_ATTRS }
}
pub fn skin_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout { array_stride: 24, step_mode: wgpu::VertexStepMode::Vertex, attributes: &SKIN_ATTRS }
}

pub const BLEND_NORMAL: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
};

/// シェーダーを作る（先頭に common.wgsl を付ける）
pub fn shader(device: &wgpu::Device, label: &str, src: &str) -> wgpu::ShaderModule {
    let code = format!("{COMMON}\n{src}");
    device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some(label), source: wgpu::ShaderSource::Wgsl(code.into()) })
}

/// 世界の値（group 0）の並び
pub fn globals_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("globals"), entries: &[wgpu::BindGroupLayoutEntry {
        binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
    }] })
}

/// 色のテクスチャ 1 枚とサンプラー（group 2）の並び
pub fn tex_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("tex"), entries: &[
        wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
        wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
    ] })
}

/// 色のテクスチャ（ミップ付き。ミップは CPU で作る）。srgb は sRGB として読む
pub fn texture(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32, rgba: &[u8], srgb_data: bool) -> wgpu::TextureView {
    make_texture(device, queue, w, h, rgba, srgb_data, None).create_view(&Default::default())
}

/// sRGB の 8 ビット → 線形（ミップの平均に使う式。WebGL の generateMipmap は SRGB8_ALPHA8 を線形で平均する）
fn srgb_decode(i: u8) -> f32 { let c = i as f32 / 255.0; if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } }
/// 線形 → sRGB の 8 ビット（指数は 1/2.4。island-kit の downsample は three の式の 0.41666）
fn srgb_encode(l: f32) -> u8 { let v = if l <= 0.0031308 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 }; (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8 }

/// CPU で作るミップの全部の段（0 段は元の絵。GPU のミップと比べる試験用。texture が上げる値）
pub fn cpu_mip_chain(w: u32, h: u32, rgba: &[u8], srgb_data: bool) -> Vec<Vec<u8>> {
    let mips = 32 - w.max(h).leading_zeros();
    let dec: Vec<f32> = (0..256).map(|i| srgb_decode(i as u8)).collect();
    let (mut cw, mut ch) = (w, h);
    let mut out = vec![rgba.to_vec()];
    for _ in 1..mips {
        let level = out.last().unwrap();
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let mut next = vec![0u8; (nw * nh * 4) as usize];
        for y in 0..nh {
            for x in 0..nw {
                let (x0, y0) = (x * cw / nw, y * ch / nh);
                let (x1, y1) = ((x0 + 1).min(cw - 1), (y0 + 1).min(ch - 1));
                for c in 0..4usize {
                    let at = |xx: u32, yy: u32| level[((yy * cw + xx) * 4) as usize + c];
                    let o = ((y * nw + x) * 4) as usize + c;
                    next[o] = if srgb_data && c < 3 {
                        srgb_encode((dec[at(x0, y0) as usize] + dec[at(x1, y0) as usize] + dec[at(x0, y1) as usize] + dec[at(x1, y1) as usize]) * 0.25)
                    } else {
                        ((at(x0, y0) as u32 + at(x1, y0) as u32 + at(x0, y1) as u32 + at(x1, y1) as u32 + 2) / 4) as u8
                    };
                }
            }
        }
        out.push(next);
        cw = nw; ch = nh;
    }
    out
}

/// 色のテクスチャを作る。gpu を渡すと 0 段だけ上げて下の段を GPU で作る（GpuMips。出る値は CPU と同じ）
pub(crate) fn make_texture(device: &wgpu::Device, queue: &wgpu::Queue, w: u32, h: u32, rgba: &[u8], srgb_data: bool, gpu: Option<&GpuMips>) -> wgpu::Texture {
    let format = if srgb_data { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm };
    let mips = 32 - w.max(h).leading_zeros();
    let gpu = gpu.filter(|_| mips > 1);
    let usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | if gpu.is_some() { wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC } else { wgpu::TextureUsages::empty() };
    let view_formats: &[wgpu::TextureFormat] = if gpu.is_some() && srgb_data { &[wgpu::TextureFormat::Rgba8Unorm] } else { &[] };
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: mips, sample_count: 1,
        dimension: wgpu::TextureDimension::D2, format, usage, view_formats,
    });
    let write = |m: u32, level: &[u8], cw: u32, ch: u32| queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: &tex, mip_level: m, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        level, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(cw * 4), rows_per_image: Some(ch) },
        wgpu::Extent3d { width: cw, height: ch, depth_or_array_layers: 1 },
    );
    if let Some(g) = gpu {
        write(0, rgba, w, h);
        g.generate(device, queue, &tex, srgb_data);
        return tex;
    }
    // ミップは色の明るさのまま平均する（WebGL の generateMipmap は SRGB8_ALPHA8 を線形で平均する。符号値のまま平均すると、小さく映ったときに細い暗い線が濃くなる）
    let (mut cw, mut ch) = (w, h);
    for (m, level) in cpu_mip_chain(w, h, rgba, srgb_data).iter().enumerate() {
        write(m as u32, level, cw, ch);
        cw = (cw / 2).max(1); ch = (ch / 2).max(1);
    }
    tex
}

/// ミップを GPU で作る（island-kit の GpuMips と同じシェーダー shaders/mips.wgsl。1 段ごとに 1 回描く）。
/// sRGB は CPU の式（srgb_decode・srgb_encode）から作った表で計算するので、出る値は CPU のミップ（cpu_mip_chain）とビットで同じ
pub struct GpuMips { layout: wgpu::BindGroupLayout, tables: wgpu::Buffer, unorm: wgpu::RenderPipeline, srgb: wgpu::RenderPipeline }

impl GpuMips {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuMips {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("actor mips"), entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
        ] });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("actor mips"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("actor mips"), source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/mips.wgsl").into()) });
        let pipe = |fs: &str| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("actor mips"), layout: Some(&pl),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_full"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &module, entry_point: Some(fs), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
        });
        // 表: 線形にする値 256 個と、srgb_encode が k 以上になる最小の値 256 個（0 番は使わない）
        let mut data: Vec<f32> = (0..256).map(|i| srgb_decode(i as u8)).collect();
        data.push(0.0);
        for k in 1..256usize {
            let (mut lo, mut hi) = (0u32, 1.0f32.to_bits() + 1);
            while lo < hi { let mid = lo + (hi - lo) / 2; if srgb_encode(f32::from_bits(mid)) as usize >= k { hi = mid; } else { lo = mid + 1; } }
            data.push(f32::from_bits(lo));
        }
        let tables = device.create_buffer(&wgpu::BufferDescriptor { label: Some("actor mips tables"), size: (data.len() * 4) as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&tables, 0, bytemuck::cast_slice(&data));
        GpuMips { unorm: pipe("fs_unorm"), srgb: pipe("fs_srgb"), layout, tables }
    }

    /// 0 段から下の段を全部作る（1 つの command buffer で submit。CPU は完了を待たない）。画像は make_texture の GPU の形で作った物
    pub fn generate(&self, device: &wgpu::Device, queue: &wgpu::Queue, tex: &wgpu::Texture, srgb_data: bool) {
        let n = tex.mip_level_count();
        if n < 2 { return; }
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("actor mips") });
        let views: Vec<wgpu::TextureView> = (0..n).map(|m| tex.create_view(&wgpu::TextureViewDescriptor { label: Some("mip"), format: Some(wgpu::TextureFormat::Rgba8Unorm), base_mip_level: m, mip_level_count: Some(1), ..Default::default() })).collect();
        let pipe = if srgb_data { &self.srgb } else { &self.unorm };
        for m in 1..n as usize {
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("mip"), layout: &self.layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[m - 1]) },
                wgpu::BindGroupEntry { binding: 1, resource: self.tables.as_entire_binding() },
            ] });
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mip"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &views[m], resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store }, depth_slice: None })],
                ..Default::default()
            });
            pass.set_pipeline(pipe);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1);
        }
        queue.submit([enc.finish()]);
    }
}

/// 試験・ゲームが GPU のミップの画像を作る（make_texture の GPU の形）
pub fn texture_gpu_mips(device: &wgpu::Device, queue: &wgpu::Queue, mips: &GpuMips, w: u32, h: u32, rgba: &[u8], srgb_data: bool) -> wgpu::Texture {
    make_texture(device, queue, w, h, rgba, srgb_data, Some(mips))
}
