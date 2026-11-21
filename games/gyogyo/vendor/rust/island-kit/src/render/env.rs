// 環境の映り込み（three の `new PMREMGenerator(renderer).fromScene(new RoomEnvironment(), sigma).texture`）。式は shaders/env.wgsl。
// 256 の面の CubeUV の絵（768×1024・Rgba16Float）を作り、材質 Physical の `env` に渡す。作るのは起動時に 1 回だけ。
use super::{DEPTH, HDR, Kit};
use crate::material::TexId;
use crate::texture::{TexEntry, TexOpts};
use glam::{EulerRot, Mat3, Mat4, Quat, Vec3};
use wgpu::util::DeviceExt;

const LOD_MIN: u32 = 4;
const LOD_MAX: u32 = 8;
const CUBE: u32 = 256;
const WIDTH: u32 = 768;
const HEIGHT: u32 = 1024;
const EXTRA_LOD_SIGMA: [f64; 6] = [0.125, 0.215, 0.35, 0.446, 0.526, 0.582];
const MAX_SAMPLES: usize = 20;
const SLOT: u64 = 256;
/// 12 面体の頂点（向かい合う物を除く）。軸の向きを散らす
const PHI: f64 = 1.618033988749895;
const INV_PHI: f64 = 1.0 / PHI;
const AXIS: [[f64; 3]; 10] = [[-PHI, INV_PHI, 0.0], [PHI, INV_PHI, 0.0], [-INV_PHI, 0.0, PHI], [INV_PHI, 0.0, PHI], [0.0, PHI, -INV_PHI], [0.0, PHI, INV_PHI], [-1.0, 1.0, -1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, 1.0], [1.0, 1.0, 1.0]];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SceneU { vp: [[f32; 4]; 4], model: [[f32; 4]; 4], nm: [[f32; 4]; 3], color: [f32; 4], pad: [[f32; 4]; 4] }
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct BlurU { weights: [f32; 20], pole: [f32; 4], p: [f32; 4], lod: [f32; 4], pad: [[f32; 4]; 8] }

/// 部屋の中身（RoomEnvironment.js の値）。(位置, y の回転, 大きさ, Basic の色か 0)
const ROOM: (Vec3, Vec3) = (Vec3::new(-0.757, 13.219, 0.717), Vec3::new(31.713, 28.305, 28.591));
const BOXES: [([f32; 3], f32, [f32; 3]); 6] = [
    ([-10.906, 2.009, 1.846], -0.195, [2.328, 7.905, 4.651]),
    ([-5.607, -0.754, -0.758], 0.994, [1.970, 1.534, 3.955]),
    ([6.167, 0.857, 7.803], 0.561, [3.927, 6.285, 3.687]),
    ([-2.017, 0.018, 6.124], 0.333, [2.002, 4.566, 2.064]),
    ([2.291, -0.756, -2.621], -0.286, [1.546, 1.552, 1.496]),
    ([-2.193, -0.369, -5.547], 0.516, [3.875, 3.487, 2.986]),
];
const LIGHTS: [([f32; 3], [f32; 3], f32); 6] = [
    ([-16.116, 14.37, 8.208], [0.1, 2.428, 2.739], 50.0),
    ([-16.109, 18.021, -8.207], [0.1, 2.425, 2.751], 50.0),
    ([14.904, 12.198, -1.832], [0.15, 4.265, 6.331], 17.0),
    ([-0.462, 8.89, 14.520], [4.38, 5.441, 0.088], 43.0),
    ([3.235, 11.486, -12.541], [2.5, 2.0, 0.1], 20.0),
    ([0.0, 20.0, 0.0], [1.0, 0.1, 1.0], 100.0),
];

/// three の _setViewport（下が 0）を、上が 0 の的へ
fn viewport(p: &mut wgpu::RenderPass<'_>, x: u32, y: u32, w: u32, h: u32) {
    let top = HEIGHT - y - h;
    p.set_viewport(x as f32, top as f32, w as f32, h as f32, 0.0, 1.0);
    p.set_scissor_rect(x, top, w, h);
}

impl Kit {
    /// three の `pmrem.fromScene(new RoomEnvironment(), sigma).texture`。返す的を材質 Physical の `env` に渡す
    pub fn room_environment(&mut self, sigma: f64) -> TexId {
        let device = &self.device;
        let tex = |label: &str| device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label), size: wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: HDR, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
        });
        let atlas = tex("pmrem");
        let ping = tex("pmrem ping");
        let (atlas_view, ping_view) = (atlas.create_view(&Default::default()), ping.create_view(&Default::default()));
        let depth = super::post_target(device, WIDTH, HEIGHT, DEPTH, 1, "pmrem depth");
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("env"), source: wgpu::ShaderSource::Wgsl(include_str!("../../shaders/env.wgsl").into()) });
        let smp = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("pmrem"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, ..Default::default() });

        // ---- 部屋を 6 面へ（_sceneToCubeUV）----
        let dyn_u = |binding: u32, size: u64| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: true, min_binding_size: wgpu::BufferSize::new(size) }, count: None };
        let scene_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("env scene"), entries: &[dyn_u(0, SLOT)] });
        let scene_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("env scene"), bind_group_layouts: &[Some(&scene_layout)], immediate_size: 0 });
        let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
        let scene_pipe = |cull: wgpu::Face| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("env scene"), layout: Some(&scene_pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_scene"), buffers: &[Some(wgpu::VertexBufferLayout { array_stride: 24, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs })], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_scene"), targets: &[Some(HDR.into())], compilation_options: Default::default() }),
            primitive: wgpu::PrimitiveState { cull_mode: Some(cull), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(true), depth_compare: Some(wgpu::CompareFunction::LessEqual), stencil: Default::default(), bias: Default::default() }),
            multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let (front_pipe, back_pipe) = (scene_pipe(wgpu::Face::Back), scene_pipe(wgpu::Face::Front));
        let b = crate::geometry::box_(1.0, 1.0, 1.0);
        let verts: Vec<f32> = b.positions.iter().zip(b.normals.iter()).flat_map(|(p, n)| [p[0], p[1], p[2], n[0], n[1], n[2]]).collect();
        let idx: Vec<u32> = b.indices.clone().unwrap_or_default();
        let vb = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("env box"), contents: bytemuck::cast_slice(&verts), usage: wgpu::BufferUsages::VERTEX });
        let ib = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("env box"), contents: bytemuck::cast_slice(&idx), usage: wgpu::BufferUsages::INDEX });
        // (model, 面の向きを裏返すか, Basic の色か 0)
        let mut parts: Vec<(Mat4, bool, f32)> = vec![(Mat4::from_translation(ROOM.0) * Mat4::from_scale(ROOM.1), true, 0.0)];
        for (p, ry, s) in BOXES {
            parts.push((Mat4::from_scale_rotation_translation(Vec3::from(s), Quat::from_euler(EulerRot::XYZ, 0.0, ry, 0.0), Vec3::from(p)), false, 0.0));
        }
        for (p, s, c) in LIGHTS { parts.push((Mat4::from_scale_rotation_translation(Vec3::from(s), Quat::IDENTITY, Vec3::from(p)), false, c)); }
        const UP: [f32; 6] = [1.0, -1.0, 1.0, 1.0, 1.0, 1.0];
        const FWD: [f32; 6] = [1.0, 1.0, 1.0, -1.0, -1.0, -1.0];
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 0.1, 100.0);
        let mut data: Vec<u8> = Vec::new();
        for i in 0..6 {
            let col = i % 3;
            let (up, tgt) = match col {
                0 => (Vec3::new(0.0, UP[i], 0.0), Vec3::new(FWD[i], 0.0, 0.0)),
                1 => (Vec3::new(0.0, 0.0, UP[i]), Vec3::new(0.0, FWD[i], 0.0)),
                _ => (Vec3::new(0.0, UP[i], 0.0), Vec3::new(0.0, 0.0, FWD[i])),
            };
            let vp = proj * Mat4::look_at_rh(Vec3::ZERO, tgt, up);
            for (m, flip, c) in &parts {
                let nm = Mat3::from_mat4(*m).inverse().transpose() * if *flip { -1.0 } else { 1.0 };
                let u = SceneU {
                    vp: vp.to_cols_array_2d(), model: m.to_cols_array_2d(),
                    nm: [nm.x_axis.extend(0.0).to_array(), nm.y_axis.extend(0.0).to_array(), nm.z_axis.extend(0.0).to_array()],
                    color: [*c, *c, *c, if *c > 0.0 { 1.0 } else { 0.0 }], pad: [[0.0; 4]; 4],
                };
                data.extend_from_slice(bytemuck::bytes_of(&u));
            }
        }
        let scene_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("env scene"), contents: &data, usage: wgpu::BufferUsages::UNIFORM });
        let scene_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("env scene"), layout: &scene_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &scene_buf, offset: 0, size: wgpu::BufferSize::new(SLOT) }) }] });

        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pmrem") });
        {
            let mut p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pmrem scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &atlas_view, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                ..Default::default()
            });
            p.set_vertex_buffer(0, vb.slice(..));
            p.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            let mut off = 0u32;
            for i in 0..6u32 {
                viewport(&mut p, (i % 3) * CUBE, if i > 2 { CUBE } else { 0 }, CUBE, CUBE);
                for (_, flip, _) in &parts {
                    p.set_pipeline(if *flip { &back_pipe } else { &front_pipe });
                    p.set_bind_group(0, &scene_bg, &[off]);
                    p.draw_indexed(0..idx.len() as u32, 0, 0..1);
                    off += SLOT as u32;
                }
            }
        }

        // ---- ぼかし（_blur・_applyPMREM）----
        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("env blur"), entries: &[
            dyn_u(0, SLOT),
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 2, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
        ] });
        let blur_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("env blur"), bind_group_layouts: &[Some(&blur_layout)], immediate_size: 0 });
        let blur_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("env blur"), layout: Some(&blur_pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_blur"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_blur"), targets: &[Some(HDR.into())], compilation_options: Default::default() }),
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
        });
        // _createPlanes
        let total = (LOD_MAX - LOD_MIN + 1) as usize + EXTRA_LOD_SIGMA.len();
        let (mut size_lods, mut sigmas) = (vec![], vec![]);
        let mut lod = LOD_MAX;
        for i in 0..total {
            let size = 1u32 << lod;
            size_lods.push(size);
            let mut s = 1.0 / size as f64;
            if i as u32 > LOD_MAX - LOD_MIN { s = EXTRA_LOD_SIGMA[i - (LOD_MAX - LOD_MIN) as usize - 1]; } else if i == 0 { s = 0.0; }
            sigmas.push(s);
            if lod > LOD_MIN { lod -= 1; }
        }
        // (lodIn, lodOut, sigma, poleAxis)
        let mut steps: Vec<(usize, usize, f64, [f64; 3])> = vec![];
        if sigma > 0.0 { steps.push((0, 0, sigma, [0.0, 1.0, 0.0])); }
        for i in 1..total {
            let s = (sigmas[i] * sigmas[i] - sigmas[i - 1] * sigmas[i - 1]).sqrt();
            steps.push((i - 1, i, s, AXIS[(total - i - 1) % AXIS.len()]));
        }
        let mut blur_data: Vec<u8> = vec![];
        // _halfBlur の順: 緯度（atlas → ping）、経度（ping → atlas）
        let mut passes: Vec<(bool, usize)> = vec![];
        for (lod_in, lod_out, sig, pole) in &steps {
            for lat in [true, false] {
                let lod_in_now = if lat { *lod_in } else { *lod_out };
                let pixels = (size_lods[lod_in_now] - 1) as f64;
                let rad = std::f64::consts::PI / (2.0 * pixels);
                let sp = sig / rad;
                let samples = 1 + (3.0 * sp).floor() as usize;
                let mut w = [0f64; MAX_SAMPLES];
                let mut sum = 0.0;
                for (k, v) in w.iter_mut().enumerate() {
                    let x = k as f64 / sp;
                    *v = (-x * x / 2.0).exp();
                    if k == 0 { sum += *v; } else if k < samples { sum += 2.0 * *v; }
                }
                let mut weights = [0f32; 20];
                for k in 0..MAX_SAMPLES { weights[k] = (w[k] / sum) as f32; }
                let t = 1.0 / (size_lods[*lod_out] as f64 - 2.0);
                let u = BlurU {
                    weights, pole: [pole[0] as f32, pole[1] as f32, pole[2] as f32, 0.0],
                    p: [samples as f32, if lat { 1.0 } else { 0.0 }, rad as f32, LOD_MAX as f32 - lod_in_now as f32],
                    lod: [t as f32, 0.0, 0.0, 0.0], pad: [[0.0; 4]; 8],
                };
                blur_data.extend_from_slice(bytemuck::bytes_of(&u));
                passes.push((lat, *lod_out));
            }
        }
        let blur_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("env blur"), contents: &blur_data, usage: wgpu::BufferUsages::UNIFORM });
        let blur_bg = |view: &wgpu::TextureView| device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("env blur"), layout: &blur_layout, entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding { buffer: &blur_buf, offset: 0, size: wgpu::BufferSize::new(SLOT) }) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&smp) },
        ] });
        let (from_atlas, from_ping) = (blur_bg(&atlas_view), blur_bg(&ping_view));
        for (k, (lat, lod_out)) in passes.iter().enumerate() {
            let target = if *lat { &ping_view } else { &atlas_view };
            let mut p = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pmrem blur"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: target, resolve_target: None, depth_slice: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
                ..Default::default()
            });
            let out = size_lods[*lod_out];
            let x = 3 * out * if *lod_out as u32 > LOD_MAX - LOD_MIN { *lod_out as u32 - LOD_MAX + LOD_MIN } else { 0 };
            let y = 4 * (CUBE - out);
            viewport(&mut p, x, y, 3 * out, 2 * out);
            p.set_pipeline(&blur_pipe);
            p.set_bind_group(0, if *lat { &from_atlas } else { &from_ping }, &[k as u32 * SLOT as u32]);
            p.draw(0..36, 0..1);
        }
        self.queue.submit([enc.finish()]);
        self.textures.push(TexEntry { view: atlas_view, tex: atlas, opts: TexOpts { mipmap: false, ..Default::default() }, srgb: false, w: WIDTH, h: HEIGHT, flip_v: false, rt: None, #[cfg(target_arch = "wasm32")] canvas: None });
        TexId(self.textures.len() as u32 - 1)
    }
}
