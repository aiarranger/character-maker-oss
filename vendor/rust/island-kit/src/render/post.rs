// 後処理: 保護 → ブルーム（UnrealBloomPass）→ 仕上げ（ゲームが渡す FinalShader）。的の組（解像度が変わると作り直す）。renderer.ts と muso の gfx.rs と同じ積み方。
use super::{DEPTH, HDR, MASK};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct BloomU { pub dir: [f32; 2], pub inv: [f32; 2], pub threshold: f32, pub strength: f32, pub radius: f32, pub kernel: f32 }

pub(crate) struct BlurMip {
    h_view: wgpu::TextureView,
    v_view: wgpu::TextureView,
    h_u: wgpu::BindGroup,
    v_u: wgpu::BindGroup,
    h_tex: wgpu::BindGroup,
    v_tex: wgpu::BindGroup,
}

pub(crate) struct Targets {
    pub msaa: wgpu::TextureView,
    pub depth: wgpu::TextureView,
    pub resolved: wgpu::TextureView,
    resolved_bg: wgpu::BindGroup,
    clean: wgpu::TextureView,
    clean_bg: wgpu::BindGroup,
    pub mask: wgpu::TextureView,
    pub mask_depth: wgpu::TextureView,
    mask_bg: wgpu::BindGroup,
    bright: wgpu::TextureView,
    bright_bg: wgpu::BindGroup,
    blur: Vec<BlurMip>,
    comp: wgpu::TextureView,
    comp_bg: wgpu::BindGroup,
    comp_tex_bg: wgpu::BindGroup,
    pub w: u32,
    pub h: u32,
    pub mw: u32,
    pub mh: u32,
    /// SMAA の的（Aa::Smaa のときだけ）
    smaa: Option<SmaaTargets>,
}

/// SMAA の的: 仕上げの画（画面と同じ形式。読むときは sRGB の見方）・縁・重み
struct SmaaTargets {
    ldr: wgpu::TextureView,
    ldr_bg: wgpu::BindGroup,
    edges: wgpu::TextureView,
    edges_bg: wgpu::BindGroup,
    weights: wgpu::TextureView,
    weights_bg: wgpu::BindGroup,
}

/// SMAA のパイプラインと表（kit.set_aa(Aa::Smaa) で 1 回だけ作る）
struct Smaa {
    p_edges: wgpu::RenderPipeline,
    p_weights: wgpu::RenderPipeline,
    p_blend: wgpu::RenderPipeline,
    lut_bg: wgpu::BindGroup,
    weights_layout: wgpu::BindGroupLayout,
}

pub(crate) struct Post {
    /// ブルームへ渡す大きさを割る数（既定 1）
    pub bloom_div: u32,
    tex_layout: wgpu::BindGroupLayout,
    u_layout: wgpu::BindGroupLayout,
    mips_layout: wgpu::BindGroupLayout,
    mask_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    p_sanitize: wgpu::RenderPipeline,
    p_high: wgpu::RenderPipeline,
    p_blur: wgpu::RenderPipeline,
    p_composite: wgpu::RenderPipeline,
    p_blend: wgpu::RenderPipeline,
    p_final: wgpu::RenderPipeline,
    high_buf: wgpu::Buffer,
    comp_buf: wgpu::Buffer,
    high_u: wgpu::BindGroup,
    comp_u: wgpu::BindGroup,
    final_buf: wgpu::Buffer,
    final_u: wgpu::BindGroup,
    surface_format: wgpu::TextureFormat,
    smaa: Option<Smaa>,
}

pub(crate) fn target(device: &wgpu::Device, w: u32, h: u32, format: wgpu::TextureFormat, samples: u32, label: &str) -> wgpu::TextureView {
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

fn fullscreen(device: &wgpu::Device, shader: &wgpu::ShaderModule, label: &str, fs: &str, layouts: &[&wgpu::BindGroupLayout], format: wgpu::TextureFormat, blend: Option<wgpu::BlendState>) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &layouts.iter().map(|l| Some(*l)).collect::<Vec<_>>(), immediate_size: 0 });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label), layout: Some(&layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some("vs_full"), buffers: &[], compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some(fs), targets: &[Some(wgpu::ColorTargetState { format, blend, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
        primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
    })
}

/// SMAA の縁と重みの的（three の版は 8 ビットの RGBA）
const SMAA_EDGES: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;
const SMAA_WEIGHTS: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const BLEND_ADD: wgpu::BlendState = wgpu::BlendState {
    color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
    alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::One, operation: wgpu::BlendOperation::Add },
};

impl Targets {
    /// 的を今すぐ手放す（Kit::release_gpu）
    pub fn destroy(&self) {
        for v in [&self.msaa, &self.depth, &self.resolved, &self.clean, &self.mask, &self.mask_depth, &self.bright, &self.comp] { v.texture().destroy(); }
        for b in &self.blur { b.h_view.texture().destroy(); b.v_view.texture().destroy(); }
        if let Some(s) = &self.smaa { for v in [&s.ldr, &s.edges, &s.weights] { v.texture().destroy(); } }
    }
}

impl Post {
    pub fn destroy(&self) { for b in [&self.high_buf, &self.comp_buf, &self.final_buf] { b.destroy(); } }

    pub fn new(device: &wgpu::Device, surface_format: wgpu::TextureFormat) -> Post {
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("post tex"), entries: &[tex_entry(0), smp_entry(1)] });
        let u_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("post u"), entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
        }] });
        let mips_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("post mips"), entries: &[tex_entry(0), tex_entry(1), tex_entry(2), tex_entry(3)] });
        let mask_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("post mask"), entries: &[tex_entry(0)] });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("post"), source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/post.wgsl").into()) });
        let p_sanitize = fullscreen(device, &shader, "sanitize", "fs_sanitize", &[&tex_layout], HDR, None);
        let p_high = fullscreen(device, &shader, "high", "fs_high", &[&tex_layout, &u_layout], HDR, None);
        let p_blur = fullscreen(device, &shader, "blur", "fs_blur", &[&tex_layout, &u_layout], HDR, None);
        let p_composite = fullscreen(device, &shader, "composite", "fs_composite", &[&tex_layout, &u_layout, &mips_layout], HDR, None);
        let p_blend = fullscreen(device, &shader, "blend", "fs_copy", &[&tex_layout], HDR, Some(BLEND_ADD));
        let bloom_buf = || device.create_buffer(&wgpu::BufferDescriptor { label: Some("bloom"), size: std::mem::size_of::<BloomU>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let (high_buf, comp_buf) = (bloom_buf(), bloom_buf());
        let high_u = u_bg(device, &u_layout, &high_buf);
        let comp_u = u_bg(device, &u_layout, &comp_buf);
        let final_buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("final"), size: 256, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let final_u = u_bg(device, &u_layout, &final_buf);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("post"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });
        let mut post = Post {
            bloom_div: 1,
            p_final: p_sanitize.clone(),
            tex_layout, u_layout, mips_layout, mask_layout, sampler, p_sanitize, p_high, p_blur, p_composite, p_blend, high_buf, comp_buf, high_u, comp_u, final_buf, final_u, surface_format, smaa: None,
        };
        post.set_final(device, include_str!("../../shaders/final_default.wgsl"), 16);
        post
    }

    /// ゲームの仕上げ（fs_final）を差し替える。uniform_size は @group(1) の u の大きさ
    pub fn set_final(&mut self, device: &wgpu::Device, wgsl: &str, uniform_size: u64) {
        let code = format!("{}\n{}", include_str!("../../shaders/final_common.wgsl"), wgsl);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("final"), source: wgpu::ShaderSource::Wgsl(code.into()) });
        self.p_final = fullscreen(device, &shader, "final", "fs_final", &[&self.tex_layout, &self.u_layout, &self.mask_layout], self.surface_format, None);
        if uniform_size.max(16) != self.final_buf.size() {
            self.final_buf = device.create_buffer(&wgpu::BufferDescriptor { label: Some("final"), size: uniform_size.max(16), usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.final_u = u_bg(device, &self.u_layout, &self.final_buf);
        }
    }

    pub fn set_final_uniform(&self, queue: &wgpu::Queue, bytes: &[u8]) {
        let n = (self.final_buf.size() as usize).min(bytes.len());
        queue.write_buffer(&self.final_buf, 0, &bytes[..n]);
    }

    pub fn write_bloom(&self, queue: &wgpu::Queue, threshold: f32, strength: f32, radius: f32) {
        let u = BloomU { dir: [0.0, 0.0], inv: [0.0, 0.0], threshold, strength, radius, kernel: 1.0 };
        queue.write_buffer(&self.high_buf, 0, bytemuck::bytes_of(&u));
        queue.write_buffer(&self.comp_buf, 0, bytemuck::bytes_of(&u));
    }

    /// SMAA のパイプラインと表を作る（1 回だけ）
    pub fn ensure_smaa(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.smaa.is_some() { return; }
        let tex = |binding: u32| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None };
        let lut_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("smaa lut"), entries: &[tex(0), tex(1), wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None }] });
        let weights_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("smaa weights"), entries: &[tex(3)] });
        let lut = |label: &str, w: u32, h: u32, format: wgpu::TextureFormat, data: &[u8]| device.create_texture_with_data(queue, &wgpu::TextureDescriptor {
            label: Some(label), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format, usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST, view_formats: &[],
        }, wgpu::util::TextureDataOrder::LayerMajor, data).create_view(&Default::default());
        let area = lut("smaa area", 160, 560, wgpu::TextureFormat::Rg8Unorm, include_bytes!("../../shaders/smaa_area.rg8"));
        let search = lut("smaa search", 64, 16, wgpu::TextureFormat::R8Unorm, include_bytes!("../../shaders/smaa_search.r8"));
        let near = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("smaa search"), ..Default::default() });
        let lut_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &lut_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&area) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&search) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&near) },
        ] });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("smaa"), source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/smaa.wgsl").into()) });
        let p_edges = fullscreen(device, &shader, "smaa edges", "fs_edges", &[&self.tex_layout], SMAA_EDGES, None);
        let p_weights = fullscreen(device, &shader, "smaa weights", "fs_weights", &[&self.tex_layout, &lut_layout], SMAA_WEIGHTS, None);
        let p_blend = fullscreen(device, &shader, "smaa blend", "fs_blend", &[&self.tex_layout, &weights_layout], self.surface_format, None);
        self.smaa = Some(Smaa { p_edges, p_weights, p_blend, lut_bg, weights_layout });
    }

    /// 的の組。w・h は px、dpr は画素の倍率（自機の型は 1.5 倍まで。renderer.ts）、samples は主の的のサンプル数（1 なら MSAA の色は使わない 1 × 1 の控え）、
    /// smaa なら SMAA の的も作る（ensure_smaa のあと）
    pub fn make_targets(&self, device: &wgpu::Device, w: u32, h: u32, dpr: f32, samples: u32, smaa: bool) -> Targets {
        let (tl, ul, ml, kl, smp) = (&self.tex_layout, &self.u_layout, &self.mips_layout, &self.mask_layout, &self.sampler);
        let msaa = if samples > 1 { target(device, w, h, HDR, samples, "msaa") } else { target(device, 1, 1, HDR, 1, "msaa") };
        let depth = target(device, w, h, DEPTH, samples, "depth");
        let resolved = target(device, w, h, HDR, 1, "resolved");
        let clean = target(device, w, h, HDR, 1, "clean");
        let mk = (dpr.min(1.5) / dpr.max(0.01)).min(1.0);
        let (mw, mh) = (((w as f32) * mk).round() as u32, ((h as f32) * mk).round() as u32);
        let mask = target(device, mw, mh, MASK, 1, "mask");
        let mask_depth = target(device, mw, mh, DEPTH, 1, "mask depth");
        let (bw, bh) = if self.bloom_div > 1 { (((w as f32) / self.bloom_div as f32).round(), ((h as f32) / self.bloom_div as f32).round()) } else { (w as f32, h as f32) };
        let (mut rx, mut ry) = ((bw / 2.0).round() as u32, (bh / 2.0).round() as u32);
        let bright = target(device, rx, ry, HDR, 1, "bright");
        let comp = target(device, rx, ry, HDR, 1, "comp");
        let mut blur = Vec::new();
        for k in [3.0f32, 5.0, 7.0, 9.0, 11.0] {
            let (rw, rh) = (rx.max(1), ry.max(1));
            let h_view = target(device, rw, rh, HDR, 1, "blur h");
            let v_view = target(device, rw, rh, HDR, 1, "blur v");
            let make = |dir: [f32; 2]| {
                let b = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("blur"), contents: bytemuck::bytes_of(&BloomU { dir, inv: [1.0 / rw as f32, 1.0 / rh as f32], threshold: 0.0, strength: 0.0, radius: 0.0, kernel: k }), usage: wgpu::BufferUsages::UNIFORM });
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
            smaa: if smaa { self.smaa.as_ref().map(|s| self.smaa_targets(device, s, w, h)) } else { None },
        }
    }

    fn smaa_targets(&self, device: &wgpu::Device, s: &Smaa, w: u32, h: u32) -> SmaaTargets {
        let srgb = self.surface_format.add_srgb_suffix();
        let ldr_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("smaa ldr"), size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: self.surface_format, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING, view_formats: &[srgb],
        });
        let ldr = ldr_tex.create_view(&Default::default());
        let ldr_lin = ldr_tex.create_view(&wgpu::TextureViewDescriptor { format: Some(srgb), ..Default::default() });
        let edges = target(device, w, h, SMAA_EDGES, 1, "smaa edges");
        let weights = target(device, w, h, SMAA_WEIGHTS, 1, "smaa weights");
        let weights_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &s.weights_layout, entries: &[wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&weights) }] });
        SmaaTargets { ldr_bg: tex_bg(device, &self.tex_layout, &ldr_lin, &self.sampler), edges_bg: tex_bg(device, &self.tex_layout, &edges, &self.sampler), ldr, edges, weights, weights_bg }
    }

    /// 保護 → ブルーム → 仕上げ（画面へ）
    pub fn run(&self, enc: &mut wgpu::CommandEncoder, t: &Targets, screen: &wgpu::TextureView, bloom: bool) {
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
        if bloom {
            pass(enc, &t.bright, clear, &self.p_high, &[&t.clean_bg, &self.high_u]);
            for (i, m) in t.blur.iter().enumerate() {
                let src = if i == 0 { &t.bright_bg } else { &t.blur[i - 1].v_tex };
                pass(enc, &m.h_view, clear, &self.p_blur, &[src, &m.h_u]);
                pass(enc, &m.v_view, clear, &self.p_blur, &[&m.h_tex, &m.v_u]);
            }
            pass(enc, &t.comp, clear, &self.p_composite, &[&t.blur[0].v_tex, &self.comp_u, &t.comp_bg]);
            pass(enc, &t.clean, wgpu::LoadOp::Load, &self.p_blend, &[&t.comp_tex_bg]);
        }
        match (&t.smaa, &self.smaa) {
            // SMAA: 仕上げの画 → 縁（縁の無い所は描かないので 0 で消す）→ 重み → 混ぜて画面へ（SMAAEffect の update と同じ順）
            (Some(st), Some(s)) => {
                pass(enc, &st.ldr, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.p_final, &[&t.clean_bg, &self.final_u, &t.mask_bg]);
                pass(enc, &st.edges, clear, &s.p_edges, &[&st.ldr_bg]);
                pass(enc, &st.weights, clear, &s.p_weights, &[&st.edges_bg, &s.lut_bg]);
                pass(enc, screen, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &s.p_blend, &[&st.ldr_bg, &st.weights_bg]);
            }
            _ => pass(enc, screen, wgpu::LoadOp::Clear(wgpu::Color::BLACK), &self.p_final, &[&t.clean_bg, &self.final_u, &t.mask_bg]),
        }
    }
}
