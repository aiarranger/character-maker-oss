// 仕上げのあとに画面へ直接重ねる板（drei の Hud。Engimeow の /lab/finale の文字の層）。sRGB のままブレンドするので、線形の的を通らない。
// three の MeshBasicMaterial（transparent・toneMapped なし・霧なし）と同じ: テクスチャの色を sRGB へ戻し、(SrcAlpha, OneMinusSrcAlpha / One, OneMinusSrcAlpha)。
use super::Kit;
use crate::material::TexId;
use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use std::collections::HashMap;

/// 板 1 枚。m は -0.5..0.5 の板を clip へ写す行列（proj × view × 世界 × 大きさ）。uv は three の PlaneGeometry と同じ（左下が 0, 0）
#[derive(Clone, Copy, Debug)]
pub struct OverlayQuad { pub tex: TexId, pub m: Mat4, pub opacity: f32 }

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct U { m: [f32; 16], p: [f32; 4] }

const SLOT: u64 = 256;
const WGSL: &str = r#"
struct U { m: mat4x4f, p: vec4f };
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var t: texture_2d<f32>;
@group(1) @binding(1) var s: sampler;
struct O { @builtin(position) clip: vec4f, @location(0) uv: vec2f };
@vertex fn vs(@builtin(vertex_index) i: u32) -> O {
  var c = array<vec2f, 6>(vec2f(-0.5, -0.5), vec2f(0.5, -0.5), vec2f(-0.5, 0.5), vec2f(-0.5, 0.5), vec2f(0.5, -0.5), vec2f(0.5, 0.5));
  var o: O;
  o.clip = u.m * vec4f(c[i], 0.0, 1.0);
  o.uv = c[i] + vec2f(0.5);
  return o;
}
fn to_srgb(c: vec3f) -> vec3f {
  let lo = c * 12.92;
  let hi = 1.055 * pow(c, vec3f(0.41666)) - 0.055;
  return select(hi, lo, c <= vec3f(0.0031308));
}
@fragment fn fs(i: O) -> @location(0) vec4f {
  let c = textureSample(t, s, i.uv);
  return vec4f(to_srgb(c.rgb), c.a * u.p.x);
}
"#;

struct Gpu { pipe: wgpu::RenderPipeline, t_layout: wgpu::BindGroupLayout, buf: wgpu::Buffer, u_bg: wgpu::BindGroup, slots: usize }

#[derive(Default)]
pub(crate) struct OverlayGpu { gpu: Option<Gpu>, tex_bg: HashMap<u32, wgpu::BindGroup>, draws: Vec<u32> }

impl OverlayGpu {
    pub(crate) fn draw(&self, enc: &mut wgpu::CommandEncoder, screen: &wgpu::TextureView, quads: &[OverlayQuad]) {
        let Some(g) = &self.gpu else { return };
        if quads.is_empty() { return; }
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("overlay"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: screen, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store }, depth_slice: None })],
            ..Default::default()
        });
        pass.set_pipeline(&g.pipe);
        for (i, q) in quads.iter().enumerate() {
            let Some(bg) = self.tex_bg.get(&q.tex.0) else { continue };
            pass.set_bind_group(0, &g.u_bg, &[self.draws[i]]);
            pass.set_bind_group(1, bg, &[]);
            pass.draw(0..6, 0..1);
        }
    }
}

impl Kit {
    /// 値を GPU へ（描く前に 1 回）
    pub(crate) fn prepare_overlay(&mut self) {
        if self.overlay.is_empty() { return; }
        let need = self.overlay.len().max(16);
        if self.overlay_gpu.gpu.as_ref().is_none_or(|g| g.slots < need) { self.make_overlay_gpu(need * 2); }
        for q in self.overlay.clone() {
            if self.overlay_gpu.tex_bg.contains_key(&q.tex.0) { continue; }
            let opts = self.textures[q.tex.0 as usize].opts;
            let smp = self.sampler_for(opts);
            let t_layout = &self.overlay_gpu.gpu.as_ref().unwrap().t_layout;
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("overlay tex"), layout: t_layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.textures[q.tex.0 as usize].view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&smp) },
            ] });
            self.overlay_gpu.tex_bg.insert(q.tex.0, bg);
        }
        let mut bytes = vec![0u8; self.overlay.len() * SLOT as usize];
        self.overlay_gpu.draws.clear();
        for (i, q) in self.overlay.iter().enumerate() {
            let u = U { m: q.m.to_cols_array(), p: [q.opacity, 0.0, 0.0, 0.0] };
            let at = i * SLOT as usize;
            bytes[at..at + std::mem::size_of::<U>()].copy_from_slice(bytemuck::bytes_of(&u));
            self.overlay_gpu.draws.push(at as u32);
        }
        let g = self.overlay_gpu.gpu.as_ref().unwrap();
        self.queue.write_buffer(&g.buf, 0, &bytes);
    }

    fn make_overlay_gpu(&mut self, slots: usize) {
        let d = &self.device;
        let u_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("overlay u"), entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<U>() as u64) }, count: None,
        }] });
        let t_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("overlay t"), entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
        ] });
        let shader = d.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("overlay"), source: wgpu::ShaderSource::Wgsl(WGSL.into()) });
        let layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("overlay"), bind_group_layouts: &[Some(&u_layout), Some(&t_layout)], immediate_size: 0 });
        let blend = wgpu::BlendState {
            color: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::SrcAlpha, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
            alpha: wgpu::BlendComponent { src_factor: wgpu::BlendFactor::One, dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha, operation: wgpu::BlendOperation::Add },
        };
        let pipe = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay"), layout: Some(&layout),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs"), targets: &[Some(wgpu::ColorTargetState { format: self.config.format, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let buf = d.create_buffer(&wgpu::BufferDescriptor { label: Some("overlay"), size: slots as u64 * SLOT, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        let u_bg = d.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("overlay u"), layout: &u_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &buf, offset: 0, size: wgpu::BufferSize::new(std::mem::size_of::<U>() as u64) }) }] });
        // 作り直すと t_layout も変わるので、テクスチャの組も作り直す
        self.overlay_gpu.tex_bg.clear();
        self.overlay_gpu.gpu = Some(Gpu { pipe, t_layout, buf, u_bg, slots });
    }
}
