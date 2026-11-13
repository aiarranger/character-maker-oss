// テクスチャ（three の CanvasTexture・DataTexture・TextureLoader と glb の画像）。決めごとは README。
use crate::material::TexId;
use crate::render::Kit;

/// 繰り返し
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Wrap { #[default] Clamp, Repeat, Mirror }

/// サンプラーの種類（three の wrapS・wrapT・magFilter・minFilter・anisotropy）
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct TexOpts {
    pub wrap_s: Wrap,
    pub wrap_t: Wrap,
    /// magFilter が NearestFilter
    pub mag_nearest: bool,
    /// minFilter が NearestFilter（ミップも使わない）
    pub min_nearest: bool,
    /// ミップを使う（minFilter が LinearMipmapLinearFilter）
    pub mipmap: bool,
    pub anisotropy: u16,
}
impl Default for TexOpts {
    fn default() -> Self { TexOpts { wrap_s: Wrap::Clamp, wrap_t: Wrap::Clamp, mag_nearest: false, min_nearest: false, mipmap: true, anisotropy: 1 } }
}
impl TexOpts {
    pub fn repeat() -> TexOpts { TexOpts { wrap_s: Wrap::Repeat, wrap_t: Wrap::Repeat, ..Default::default() } }
}

/// 読んだ画像（上下を反転済み。three の flipY）
pub struct LoadedImage { pub width: u32, pub height: u32, pub rgba: Vec<u8> }

pub(crate) struct TexEntry {
    pub view: wgpu::TextureView,
    pub tex: wgpu::Texture,
    pub opts: TexOpts,
    pub srgb: bool,
    pub w: u32,
    pub h: u32,
    /// 描いた的（three の RT は下が v = 0）
    pub flip_v: bool,
    /// 描いた的なら MSAA の色と深度
    pub rt: Option<RenderTargetViews>,
    #[cfg(target_arch = "wasm32")]
    pub canvas: Option<CanvasTexture>,
}
pub(crate) struct RenderTargetViews { pub msaa: wgpu::TextureView, pub depth: wgpu::TextureView }
impl RenderTargetViews {
    /// 的の MSAA の色と深度（kit.samples）。1 サンプルのときの色は使わない 1 × 1 の控え（frame.rs の color_att が的へ直接描く）
    pub(crate) fn new(device: &wgpu::Device, w: u32, h: u32, samples: u32) -> Self {
        let (cw, ch) = if samples > 1 { (w, h) } else { (1, 1) };
        RenderTargetViews {
            msaa: crate::render::post_target(device, cw, ch, super::render::HDR, samples, "rt msaa"),
            depth: crate::render::post_target(device, w, h, super::render::DEPTH, samples, "rt depth"),
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) struct CanvasTexture {
    canvas: web_sys::OffscreenCanvas,
    ctx: web_sys::OffscreenCanvasRenderingContext2d,
    /// Some のときだけ GPU 転送。空の配列は 1x1（ミップなし）。既定の読み戻しは None。
    gpu_levels: Option<Vec<CanvasMip>>,
}
#[cfg(target_arch = "wasm32")]
struct CanvasMip { target: wgpu::TextureView, source: wgpu::BindGroup }
#[cfg(target_arch = "wasm32")]
pub(crate) struct CanvasMips { layout: wgpu::BindGroupLayout, pipeline: wgpu::RenderPipeline }

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c < 0.04045 { c * 0.0773993808 } else { (c * 0.9478672986 + 0.0521327014).powf(2.4) }
}
fn linear_to_srgb(c: f32) -> u8 {
    let v = if c < 0.0031308 { c * 12.92 } else { 1.055 * c.powf(0.41666) - 0.055 };
    (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// linear_to_srgb の早見表（0〜1 を SRGB_BINS 等分）。区間とその両隣の端の値が全部同じ所だけ値を持ち、
/// 値の変わり目の近くは 0xFFFF（式で計算する）。式は画素ごとに powf を呼び、1248×1056 の絵のミップで約 130 万回になる。出る値は式と同じ
const SRGB_BINS: usize = 65536;
fn srgb_encode_table() -> &'static [u16] {
    static TABLE: std::sync::OnceLock<Vec<u16>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let edge: Vec<u8> = (0..=SRGB_BINS).map(|i| linear_to_srgb(i as f32 / SRGB_BINS as f32)).collect();
        (0..SRGB_BINS).map(|i| {
            let v = edge[i];
            if edge[i.saturating_sub(1)] == v && edge[i + 1] == v && edge[(i + 2).min(SRGB_BINS)] == v { v as u16 } else { 0xFFFF }
        }).collect()
    })
}
#[inline]
fn linear_to_srgb_fast(c: f32, table: &[u16]) -> u8 {
    match table.get((c * SRGB_BINS as f32) as usize) { Some(&v) if v != 0xFFFF => v as u8, _ => linear_to_srgb(c) }
}

/// 1 段小さいミップ（GL の generateMipmap と同じく、sRGB は線形にして平均する。アルファはそのまま平均）
pub(crate) fn downsample(level: &[u8], w: u32, h: u32, srgb: bool, lut: &[f32; 256]) -> (Vec<u8>, u32, u32) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut next = vec![0u8; (nw * nh * 4) as usize];
    let encode: &[u16] = if srgb { srgb_encode_table() } else { &[] };
    for y in 0..nh {
        for x in 0..nw {
            let (x0, y0) = (x * w / nw, y * h / nh);
            let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
            let idx = |xx: u32, yy: u32| ((yy * w + xx) * 4) as usize;
            let (a, b, c, d) = (idx(x0, y0), idx(x1, y0), idx(x0, y1), idx(x1, y1));
            let o = ((y * nw + x) * 4) as usize;
            for ch in 0..3 {
                next[o + ch] = if srgb {
                    linear_to_srgb_fast((lut[level[a + ch] as usize] + lut[level[b + ch] as usize] + lut[level[c + ch] as usize] + lut[level[d + ch] as usize]) * 0.25, encode)
                } else {
                    ((level[a + ch] as u32 + level[b + ch] as u32 + level[c + ch] as u32 + level[d + ch] as u32 + 2) / 4) as u8
                };
            }
            next[o + 3] = ((level[a + 3] as u32 + level[b + 3] as u32 + level[c + 3] as u32 + level[d + 3] as u32 + 2) / 4) as u8;
        }
    }
    (next, nw, nh)
}

fn srgb_lut() -> [f32; 256] {
    let mut t = [0.0; 256];
    for (i, v) in t.iter_mut().enumerate() { *v = srgb_to_linear(i as u8); }
    t
}

/// linear_to_srgb が k 以上になる最小の値（k = 1..255。0 番は使わない）。GPU のミップ（shaders/mips.wgsl の encode）が
/// この表と比べて 8 ビットを出すので、GPU の pow を使わずに CPU の式と同じ値になる。式は値の順に並ぶ（8 ビットの段で単調）
pub(crate) fn srgb_edges() -> [f32; 256] {
    let mut e = [0.0f32; 256];
    for (k, out) in e.iter_mut().enumerate().skip(1) {
        // 0..=1.0 の f32 のビットで二分探し（正の f32 はビットの順と値の順が同じ）
        let (mut lo, mut hi) = (0u32, 1.0f32.to_bits() + 1);
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if linear_to_srgb(f32::from_bits(mid)) as usize >= k { hi = mid; } else { lo = mid + 1; }
        }
        *out = f32::from_bits(lo);
    }
    e
}

/// GPU で作るミップの種類
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum MipKind { Unorm, Srgb, Float }

/// ミップを GPU で作る（shaders/mips.wgsl）。kit.set_gpu_mips で glb_texture・data_texture など、透過用の的（Float）でも使う
pub(crate) struct GpuMips { layout: wgpu::BindGroupLayout, tables: wgpu::Buffer, unorm: wgpu::RenderPipeline, srgb: wgpu::RenderPipeline, float: wgpu::RenderPipeline }

impl GpuMips {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> GpuMips {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("mips"), entries: &[
            wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None },
            wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None },
        ] });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("mips"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("mips"), source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/mips.wgsl").into()) });
        let pipe = |fs: &str, format: wgpu::TextureFormat| device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mips"), layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_full"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some(fs), targets: &[Some(wgpu::ColorTargetState { format, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
            primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
        });
        let mut data: Vec<f32> = srgb_lut().to_vec();
        data.extend_from_slice(&srgb_edges());
        let tables = device.create_buffer(&wgpu::BufferDescriptor { label: Some("mips tables"), size: (data.len() * 4) as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        queue.write_buffer(&tables, 0, bytemuck::cast_slice(&data));
        GpuMips { unorm: pipe("fs_unorm", wgpu::TextureFormat::Rgba8Unorm), srgb: pipe("fs_srgb", wgpu::TextureFormat::Rgba8Unorm), float: pipe("fs_float", crate::render::HDR), layout, tables }
    }

    /// 0 段から下の段を全部作り直す（1 つの command buffer で submit。CPU は完了を待たない）。sRGB の画像は Rgba8Unorm の view で読み書きするので、
    /// 作るときに view_formats へ Rgba8Unorm を入れ、usage に RENDER_ATTACHMENT を入れておく（gpu_mip_usage）
    pub(crate) fn generate(&self, device: &wgpu::Device, queue: &wgpu::Queue, tex: &wgpu::Texture, kind: MipKind) {
        if tex.mip_level_count() < 2 { return; }
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("mips") });
        self.record(device, &mut enc, tex, kind);
        queue.submit([enc.finish()]);
    }

    /// generate の中身を、ほかの描画と同じ command buffer へ積む（透過用の的: 描いたすぐあとに作る）
    pub(crate) fn record(&self, device: &wgpu::Device, enc: &mut wgpu::CommandEncoder, tex: &wgpu::Texture, kind: MipKind) {
        let n = tex.mip_level_count();
        if n < 2 { return; }
        let format = match kind { MipKind::Float => crate::render::HDR, _ => wgpu::TextureFormat::Rgba8Unorm };
        let views: Vec<wgpu::TextureView> = (0..n).map(|m| tex.create_view(&wgpu::TextureViewDescriptor { label: Some("mip"), format: Some(format), base_mip_level: m, mip_level_count: Some(1), ..Default::default() })).collect();
        let pipe = match kind { MipKind::Unorm => &self.unorm, MipKind::Srgb => &self.srgb, MipKind::Float => &self.float };
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
    }
}

/// GPU でミップを作れる画像か（作るときに gpu_mip_usage を入れた物）
pub(crate) fn gpu_mips_ok(tex: &wgpu::Texture) -> bool {
    tex.usage().contains(wgpu::TextureUsages::RENDER_ATTACHMENT) && matches!(tex.format(), wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb)
}

/// CPU で作るミップの全部の段（write_levels が上げる値。GPU のミップと比べる試験用）
#[doc(hidden)]
pub fn cpu_mip_chain(w: u32, h: u32, rgba: &[u8], srgb: bool) -> Vec<Vec<u8>> {
    let lut = srgb_lut();
    let mut out = vec![rgba.to_vec()];
    let (mut cw, mut ch) = (w, h);
    while cw > 1 || ch > 1 {
        let (next, nw, nh) = downsample(out.last().unwrap(), cw, ch, srgb, &lut);
        out.push(next);
        cw = nw; ch = nh;
    }
    out
}

/// 0 段の一部を書く
fn write_level0(queue: &wgpu::Queue, tex: &wgpu::Texture, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d { x, y, z: 0 }, aspect: wgpu::TextureAspect::All },
        rgba, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: Some(h) },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
}

/// 全部の段を書く（ミップは作り直す）
fn write_levels(queue: &wgpu::Queue, tex: &wgpu::Texture, w: u32, h: u32, rgba: &[u8], srgb: bool) {
    let mips = tex.mip_level_count();
    let lut = srgb_lut();
    let (mut cw, mut ch) = (w, h);
    let mut owned: Option<Vec<u8>> = None;
    for m in 0..mips {
        let level: &[u8] = owned.as_deref().unwrap_or(rgba);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: m, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            level, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(cw * 4), rows_per_image: Some(ch) },
            wgpu::Extent3d { width: cw, height: ch, depth_or_array_layers: 1 },
        );
        if m + 1 == mips { break; }
        let (next, nw, nh) = downsample(level, cw, ch, srgb, &lut);
        owned = Some(next);
        cw = nw; ch = nh;
    }
}

#[cfg(any(target_arch = "wasm32", test))]
struct PackedMip { offset: u64, row: u32, w: u32, h: u32 }

/// copyBufferToTexture の行を256バイトにそろえる。画素とミップの計算は write_levels と同じ。
#[cfg(any(target_arch = "wasm32", test))]
fn packed_size(w: u32, h: u32, mips: u32) -> u64 {
    (0..mips).map(|m| {
        let (w, h) = ((w >> m).max(1), (h >> m).max(1));
        (w as u64 * 4).div_ceil(256) * 256 * h as u64
    }).sum()
}

#[cfg(any(target_arch = "wasm32", test))]
fn pack_levels(w: u32, h: u32, rgba: &[u8], srgb: bool, mips: u32) -> (Vec<u8>, Vec<PackedMip>) {
    let mut bytes = vec![0; packed_size(w, h, mips) as usize];
    let mut copies = Vec::with_capacity(mips as usize);
    let lut = srgb_lut();
    let (mut cw, mut ch, mut offset) = (w, h, 0usize);
    let mut owned: Option<Vec<u8>> = None;
    for m in 0..mips {
        let level = owned.as_deref().unwrap_or(rgba);
        let row = (cw * 4).div_ceil(256) * 256;
        for y in 0..ch as usize {
            let src = y * cw as usize * 4;
            let dst = offset + y * row as usize;
            bytes[dst..dst + cw as usize * 4].copy_from_slice(&level[src..src + cw as usize * 4]);
        }
        copies.push(PackedMip { offset: offset as u64, row, w: cw, h: ch });
        offset += row as usize * ch as usize;
        if m + 1 == mips { break; }
        let (next, nw, nh) = downsample(level, cw, ch, srgb, &lut);
        owned = Some(next);
        cw = nw; ch = nh;
    }
    (bytes, copies)
}

#[cfg(test)]
mod packed_texture_tests {
    use super::*;

    #[test]
    fn padded_upload_keeps_every_pixel_of_every_mip() {
        for (w, h) in [(1u32, 1u32), (1, 9), (7, 1), (3, 5), (64, 64), (65, 129), (256, 640), (512, 512)] {
            let rgba: Vec<u8> = (0..w * h * 4).map(|i| ((i * 37 + i / 7) % 256) as u8).collect();
            for srgb in [false, true] {
                let mips = 32 - w.max(h).leading_zeros();
                let (packed, copies) = pack_levels(w, h, &rgba, srgb, mips);
                assert_eq!(packed.len() as u64, packed_size(w, h, mips));
                let (mut expected, mut cw, mut ch) = (rgba.clone(), w, h);
                let mut next_offset = 0;
                for (m, c) in copies.iter().enumerate() {
                    assert_eq!((c.w, c.h), (cw, ch));
                    assert_eq!(c.offset, next_offset);
                    assert_eq!(c.offset % 256, 0);
                    assert_eq!(c.row % 256, 0);
                    for y in 0..ch as usize {
                        let at = c.offset as usize + y * c.row as usize;
                        let len = cw as usize * 4;
                        assert_eq!(&packed[at..at + len], &expected[y * len..(y + 1) * len]);
                        assert!(packed[at + len..at + c.row as usize].iter().all(|v| *v == 0));
                    }
                    next_offset += c.row as u64 * ch as u64;
                    if m + 1 < copies.len() {
                        (expected, cw, ch) = downsample(&expected, cw, ch, srgb, &srgb_lut());
                    }
                }
                assert_eq!(next_offset, packed.len() as u64);
            }
        }
    }
}

/// 初回画像は全ミップを1つの転送バッファから送る。GPUが混んでいるときの
/// writeTexture ごとの同期転送を避ける。既存canvasの更新経路は変えない。
#[cfg(target_arch = "wasm32")]
fn write_initial_levels(device: &wgpu::Device, queue: &wgpu::Queue, tex: &wgpu::Texture, w: u32, h: u32, rgba: &[u8], srgb: bool) {
    use wgpu::util::DeviceExt;
    let mips = tex.mip_level_count();
    if packed_size(w, h, mips) > device.limits().max_buffer_size {
        write_levels(queue, tex, w, h, rgba, srgb);
        return;
    }
    let (bytes, copies) = pack_levels(w, h, rgba, srgb, mips);
    let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("initial texture upload"), contents: &bytes, usage: wgpu::BufferUsages::COPY_SRC,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("initial texture upload") });
    for (m, c) in copies.iter().enumerate() {
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: c.offset, bytes_per_row: Some(c.row), rows_per_image: Some(c.h) } },
            wgpu::TexelCopyTextureInfo { texture: tex, mip_level: m as u32, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::Extent3d { width: c.w, height: c.h, depth_or_array_layers: 1 },
        );
    }
    // submitがリソースを保持する。完了待ちやdestroyはせず、後続の描画と同じqueueへ並べる。
    queue.submit([encoder.finish()]);
}

/// 上下を反転する（three の flipY）
pub fn flip_rows(rgba: &mut [u8], w: u32, h: u32) {
    let row = (w * 4) as usize;
    let mut tmp = vec![0u8; row];
    for y in 0..(h as usize / 2) {
        let (a, b) = (y * row, (h as usize - 1 - y) * row);
        tmp.copy_from_slice(&rgba[a..a + row]);
        rgba.copy_within(b..b + row, a);
        rgba[b..b + row].copy_from_slice(&tmp);
    }
}

impl Kit {
    /// WebGPU の初回画像をまとめて転送する（既定off）。混雑時の場面切り替え用。
    /// 既存canvasの更新、キャッシュ済み画像、native backendには影響しない。
    #[cfg(target_arch = "wasm32")]
    pub fn set_staged_texture_uploads(&mut self, enabled: bool) { self.staged_texture_uploads = enabled; }

    /// 画像のミップを GPU で作る（既定 false は今までどおり CPU の downsample）。true のあとに作る画像（data_texture・image_data_texture・glb_texture・
    /// canvas_texture）と、その上げ直し（write_texture・redraw_canvas）が GPU で作る。出る値は CPU と同じ（tests/gpu.rs の gpu_mips_match_cpu）。
    /// 主役の 4096² の絵で、CPU のミップ作り（約 930 ms）が無くなる。画像は RENDER_ATTACHMENT と（sRGB なら）Rgba8Unorm の view を持つ
    pub fn set_gpu_mips(&mut self, on: bool) { self.gpu_mips = on; }

    pub(crate) fn gpu_mips_runner(&mut self) -> &GpuMips {
        if self.mips.is_none() { self.mips = Some(GpuMips::new(&self.device, &self.queue)); }
        self.mips.as_ref().unwrap()
    }

    /// 0 段を書いて、下の段を作る（GPU で作れる画像なら GPU、ほかは CPU の write_levels）
    pub(crate) fn upload_levels(&mut self, id: TexId, rgba: &[u8]) {
        let t = &self.textures[id.0 as usize];
        let (tex, w, h, srgb) = (t.tex.clone(), t.w, t.h, t.srgb);
        if gpu_mips_ok(&tex) {
            write_level0(&self.queue, &tex, 0, 0, w, h, rgba);
            let kind = if srgb { MipKind::Srgb } else { MipKind::Unorm };
            let (device, queue) = (self.device.clone(), self.queue.clone());
            self.gpu_mips_runner().generate(&device, &queue, &tex, kind);
        } else {
            write_levels(&self.queue, &tex, w, h, rgba, srgb);
        }
    }

    /// 画像の全体か一部を、新しい GPU の画像を作らずに上げ直す（three の texture.image を書き換えて needsUpdate。Rgba8 の画像だけ）。
    /// 全体なら作るときと同じく下の段も作り直す（GPU で作れる画像は GPU、ほかは CPU）。一部のときは 0 段を書いて、GPU で作れる画像
    /// （set_gpu_mips(true) のあとに作った物）だけ下の段を作り直す。それ以外の画像の一部の上げ直しは下の段が前のまま（ミップを使わない
    /// TexOpts { mipmap: false, .. } の画像に使う）。rgba は w × h × 4、上の行が先（反転しない。data_texture と同じ）
    pub fn write_texture(&mut self, id: TexId, x: u32, y: u32, w: u32, h: u32, rgba: &[u8]) {
        let t = &self.textures[id.0 as usize];
        assert!(t.rt.is_none() && matches!(t.tex.format(), wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Rgba8UnormSrgb), "write_texture は Rgba8 の画像だけ");
        assert!(x + w <= t.w && y + h <= t.h && rgba.len() == (w * h * 4) as usize, "write_texture: 範囲か大きさが違う");
        if w == 0 || h == 0 { return; }
        if x == 0 && y == 0 && w == t.w && h == t.h { self.upload_levels(id, rgba); return; }
        let (tex, srgb) = (t.tex.clone(), t.srgb);
        write_level0(&self.queue, &tex, x, y, w, h, rgba);
        if gpu_mips_ok(&tex) {
            let (device, queue) = (self.device.clone(), self.queue.clone());
            self.gpu_mips_runner().generate(&device, &queue, &tex, if srgb { MipKind::Srgb } else { MipKind::Unorm });
        }
    }

    fn push_texture(&mut self, w: u32, h: u32, rgba: &[u8], srgb: bool, opts: TexOpts) -> TexId {
        let format = if srgb { wgpu::TextureFormat::Rgba8UnormSrgb } else { wgpu::TextureFormat::Rgba8Unorm };
        let mips = 32 - w.max(h).leading_zeros();
        let gpu = self.gpu_mips && mips > 1;
        let unorm_view = [wgpu::TextureFormat::Rgba8Unorm];
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("kit tex"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: mips, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | if gpu { wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC } else { wgpu::TextureUsages::empty() },
            view_formats: if gpu && srgb { &unorm_view } else { &[] },
        });
        if gpu {
            let view = tex.create_view(&Default::default());
            self.textures.push(TexEntry { view, tex, opts, srgb, w, h, flip_v: false, rt: None, #[cfg(target_arch = "wasm32")] canvas: None });
            let id = TexId(self.textures.len() as u32 - 1);
            self.upload_levels(id, rgba);
            return id;
        }
        #[cfg(target_arch = "wasm32")]
        if self.staged_texture_uploads {
            write_initial_levels(&self.device, &self.queue, &tex, w, h, rgba, srgb);
        } else {
            write_levels(&self.queue, &tex, w, h, rgba, srgb);
        }
        #[cfg(not(target_arch = "wasm32"))]
        write_levels(&self.queue, &tex, w, h, rgba, srgb);
        let view = tex.create_view(&Default::default());
        self.textures.push(TexEntry { view, tex, opts, srgb, w, h, flip_v: false, rt: None, #[cfg(target_arch = "wasm32")] canvas: None });
        TexId(self.textures.len() as u32 - 1)
    }

    /// three の DataTexture（反転しない。ミップ付き・線形）
    pub fn data_texture(&mut self, w: u32, h: u32, rgba: &[u8], srgb: bool) -> TexId { self.push_texture(w, h, rgba, srgb, TexOpts::default()) }

    /// 読んだ画像から（反転済み）
    pub fn image_data_texture(&mut self, img: &LoadedImage, srgb: bool) -> TexId { self.push_texture(img.width, img.height, &img.rgba, srgb, TexOpts::default()) }

    /// glb の画像（反転なし・sRGB）。同じ画像は 1 枚だけ上げる
    pub fn glb_texture(&mut self, glb: &character_actor::glb::Glb, image: usize) -> TexId {
        let key = (glb as *const _ as usize, image);
        if let Some(t) = self.glb_tex.get(&key) { return *t; }
        let im = &glb.images[image];
        let t = self.push_texture(im.width, im.height, &im.rgba, true, TexOpts::repeat());
        self.glb_tex.insert(key, t);
        t
    }

    /// サンプラーの種類を変える（three の wrapS など。次の描画で効く）
    pub fn set_tex_opts(&mut self, id: TexId, opts: TexOpts) {
        self.textures[id.0 as usize].opts = opts;
        self.invalidate_bind_groups();
    }
    pub fn tex_opts(&self, id: TexId) -> TexOpts { self.textures[id.0 as usize].opts }
    pub fn tex_size(&self, id: TexId) -> (u32, u32) { let t = &self.textures[id.0 as usize]; (t.w, t.h) }

    /// 透過用の的（kit.transmission の別描画が描く。HDR・ミップ付き・線形ミップのサンプラー）。番号は変わらず、大きさは render が
    /// 主の的 × scale に合わせて作り直す（今の大きさは tex_size。three の transmissionSamplerSize）。Custom の材質の textures に入れて
    /// textureSampleLevel で読む（WGSL では上が v = 0 なので、three の画面の uv は y を 1 - y にする）
    pub fn transmission_target(&mut self) -> TexId {
        if let Some(t) = self.trans_tex { return t; }
        self.ensure_transmission(1, 1)
    }

    pub(crate) fn ensure_transmission(&mut self, w: u32, h: u32) -> TexId {
        if let Some(id) = self.trans_tex && self.tex_size(id) == (w, h) { return id; }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("kit transmission"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 32 - w.max(h).leading_zeros(), sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: super::render::HDR, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let view0 = tex.create_view(&wgpu::TextureViewDescriptor { label: Some("transmission 0"), base_mip_level: 0, mip_level_count: Some(1), ..Default::default() });
        let e = TexEntry { view, tex, opts: TexOpts::default(), srgb: false, w, h, flip_v: true, rt: Some(RenderTargetViews::new(&self.device, w, h, self.samples)), #[cfg(target_arch = "wasm32")] canvas: None };
        self.trans_view0 = Some(view0);
        match self.trans_tex {
            Some(id) => {
                let old = std::mem::replace(&mut self.textures[id.0 as usize], e);
                old.tex.destroy();
                if let Some(rt) = &old.rt { rt.msaa.texture().destroy(); rt.depth.texture().destroy(); }
                self.invalidate_bind_groups();
                id
            }
            None => {
                self.textures.push(e);
                let id = TexId(self.textures.len() as u32 - 1);
                self.trans_tex = Some(id);
                id
            }
        }
    }

    /// 描く的（three の WebGLRenderTarget。kit.render_to で描き、材質の map に貼る）
    pub fn render_target(&mut self, w: u32, h: u32) -> TexId {
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("kit rt"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1,
            dimension: wgpu::TextureDimension::D2, format: super::render::HDR, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC, view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        self.textures.push(TexEntry { view, tex, opts: TexOpts { mipmap: false, ..Default::default() }, srgb: false, w, h, flip_v: true, rt: Some(RenderTargetViews::new(&self.device, w, h, self.samples)), #[cfg(target_arch = "wasm32")] canvas: None });
        TexId(self.textures.len() as u32 - 1)
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;
    use wasm_bindgen::{JsCast, JsValue};
    use wasm_bindgen_futures::JsFuture;
    use web_sys::{OffscreenCanvas, OffscreenCanvasRenderingContext2d};

    fn js_err(e: JsValue) -> String { e.as_string().unwrap_or_else(|| format!("{e:?}")) }

    fn read_canvas(ctx: &OffscreenCanvasRenderingContext2d, w: u32, h: u32) -> Vec<u8> {
        let img = ctx.get_image_data(0.0, 0.0, w as f64, h as f64).expect("getImageData");
        let mut rgba = img.data().0;
        flip_rows(&mut rgba, w, h);
        rgba
    }

    fn new_canvas(w: u32, h: u32) -> Result<(OffscreenCanvas, OffscreenCanvasRenderingContext2d), String> {
        let c = OffscreenCanvas::new(w, h).map_err(js_err)?;
        let ctx: OffscreenCanvasRenderingContext2d = c.get_context("2d").map_err(js_err)?.ok_or("2d が無い")?.dyn_into().map_err(|_| "2d が無い")?;
        Ok((c, ctx))
    }

    /// willReadFrequently の OffscreenCanvas（CPU で描く。getImageData が GPU を待たない）
    fn new_canvas_cpu(w: u32, h: u32) -> Result<(OffscreenCanvas, OffscreenCanvasRenderingContext2d), String> {
        let c = OffscreenCanvas::new(w, h).map_err(js_err)?;
        let o = js_sys::Object::new();
        js_sys::Reflect::set(&o, &"willReadFrequently".into(), &JsValue::TRUE).map_err(js_err)?;
        let ctx: OffscreenCanvasRenderingContext2d = c.get_context_with_context_options("2d", &o).map_err(js_err)?.ok_or("2d が無い")?.dyn_into().map_err(|_| "2d が無い")?;
        Ok((c, ctx))
    }

    impl CanvasMips {
        fn new(device: &wgpu::Device) -> Self {
            let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("canvas mip source"), entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0, visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: false }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false }, count: None,
                }],
            });
            let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("canvas mip"), bind_group_layouts: &[Some(&layout)], immediate_size: 0,
            });
            let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("canvas mip"), source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/canvas_mips.wgsl").into()),
            });
            let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("canvas mip"), layout: Some(&pipe_layout),
                vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs_full"), buffers: &[], compilation_options: Default::default() },
                fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs_mip"), targets: &[Some(wgpu::ColorTargetState { format: wgpu::TextureFormat::Rgba8Unorm, blend: None, write_mask: wgpu::ColorWrites::ALL })], compilation_options: Default::default() }),
                primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
            });
            Self { layout, pipeline }
        }

        fn levels(&self, device: &wgpu::Device, tex: &wgpu::Texture) -> Vec<CanvasMip> {
            let views: Vec<_> = (0..tex.mip_level_count()).map(|m| tex.create_view(&wgpu::TextureViewDescriptor {
                label: Some("canvas mip level"), base_mip_level: m, mip_level_count: Some(1), ..Default::default()
            })).collect();
            (1..views.len()).map(|m| CanvasMip {
                target: views[m].clone(),
                source: device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("canvas mip source"), layout: &self.layout,
                    entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&views[m - 1]) }],
                }),
            }).collect()
        }

        fn upload(&self, device: &wgpu::Device, queue: &wgpu::Queue, tex: &wgpu::Texture, canvas: &OffscreenCanvas, levels: &[CanvasMip]) {
            // read_canvas と同じ上下反転・非乗算 alpha。Canvas の sRGB の数値を
            // Rgba8Unorm へ置く。sRGB テクスチャの線形化はこの専用経路では行わない。
            queue.copy_external_image_to_texture(
                &wgpu::CopyExternalImageSourceInfo { source: wgpu::ExternalImageSource::OffscreenCanvas(canvas.clone()), origin: wgpu::Origin2d::ZERO, flip_y: true },
                wgpu::CopyExternalImageDestInfo { texture: tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All, color_space: wgpu::PredefinedColorSpace::Srgb, premultiplied_alpha: false },
                wgpu::Extent3d { width: tex.width(), height: tex.height(), depth_or_array_layers: 1 },
            );
            if levels.is_empty() { return; }
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("canvas mips") });
            for level in levels {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("canvas mip"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &level.target, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store }, depth_slice: None })],
                    ..Default::default()
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &level.source, &[]);
                pass.draw(0..3, 0..1);
            }
            // 同じ queue の後続の水面描画より前に全段を送る。CPU では完了を待たない。
            queue.submit([enc.finish()]);
        }
    }

    /// URL の画像を読む（上下を反転して返す。three の TextureLoader）。kit を借りないので、読む間も描き続けられる
    pub async fn load_image(url: &str) -> Result<LoadedImage, String> {
        let win = web_sys::window().ok_or("window が無い")?;
        let resp: web_sys::Response = JsFuture::from(win.fetch_with_str(url)).await.map_err(js_err)?.dyn_into().map_err(js_err)?;
        if !resp.ok() { return Err(format!("{url}: HTTP {}", resp.status())); }
        let blob: web_sys::Blob = JsFuture::from(resp.blob().map_err(js_err)?).await.map_err(js_err)?.dyn_into().map_err(js_err)?;
        let bmp: web_sys::ImageBitmap = JsFuture::from(win.create_image_bitmap_with_blob(&blob).map_err(js_err)?).await.map_err(js_err)?.dyn_into().map_err(js_err)?;
        let (w, h) = (bmp.width(), bmp.height());
        let (_c, ctx) = new_canvas(w, h)?;
        ctx.draw_image_with_image_bitmap(&bmp, 0.0, 0.0).map_err(js_err)?;
        Ok(LoadedImage { width: w, height: h, rgba: read_canvas(&ctx, w, h) })
    }

    /// 字体を読み終わるまで待つ（canvas_texture の文字に使う前に。CSS の @font-face は使われるまで読まれない）。読めなくても Ok（代わりの字体で描く）
    pub async fn load_font(font: &str) {
        let Some(doc) = web_sys::window().and_then(|w| w.document()) else { return };
        let _ = JsFuture::from(doc.fonts().load(font)).await;
    }

    impl Kit {
        /// three の CanvasTexture（OffscreenCanvas に描いて上げる。上下を反転。文字はブラウザの字体のまま）。既定は Clamp・線形ミップ
        pub fn canvas_texture(&mut self, w: u32, h: u32, srgb: bool, draw: impl FnOnce(&OffscreenCanvasRenderingContext2d)) -> TexId {
            let (c, ctx) = new_canvas(w, h).expect("OffscreenCanvas");
            draw(&ctx);
            let rgba = read_canvas(&ctx, w, h);
            let id = self.push_texture(w, h, &rgba, srgb, TexOpts::default());
            self.textures[id.0 as usize].canvas = Some(CanvasTexture { canvas: c, ctx, gpu_levels: None });
            id
        }

        /// 走っている間に何度も redraw_canvas する絵（keiba の掲示板）。canvas_texture と同じで、CPU で描く。GPU の絵を読み戻すと 1 回 40〜90ms 止まる
        pub fn canvas_texture_cpu(&mut self, w: u32, h: u32, srgb: bool, draw: impl FnOnce(&OffscreenCanvasRenderingContext2d)) -> TexId {
            let (c, ctx) = new_canvas_cpu(w, h).expect("OffscreenCanvas");
            draw(&ctx);
            let rgba = read_canvas(&ctx, w, h);
            let id = self.push_texture(w, h, &rgba, srgb, TexOpts::default());
            self.textures[id.0 as usize].canvas = Some(CanvasTexture { canvas: c, ctx, gpu_levels: None });
            id
        }

        /// 非 sRGB の CanvasTexture を GPU だけで転送する（明示指定した型・マスク用）。
        /// Canvas の描き方は既定と同じ。上下を反転し、ミップは downsample の整数4点平均。
        /// sRGB の画像・文字は従来の canvas_texture / canvas_texture_cpu を使う。
        pub fn canvas_texture_gpu(&mut self, w: u32, h: u32, draw: impl FnOnce(&OffscreenCanvasRenderingContext2d)) -> TexId {
            let (c, ctx) = new_canvas(w, h).expect("OffscreenCanvas");
            draw(&ctx);
            let tex = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("kit canvas gpu"), size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 32 - w.max(h).leading_zeros(), sample_count: 1,
                dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::RENDER_ATTACHMENT, view_formats: &[],
            });
            let mips = self.canvas_mips.get_or_insert_with(|| CanvasMips::new(&self.device));
            let levels = mips.levels(&self.device, &tex);
            mips.upload(&self.device, &self.queue, &tex, &c, &levels);
            let view = tex.create_view(&Default::default());
            self.textures.push(TexEntry { view, tex, opts: TexOpts::default(), srgb: false, w, h, flip_v: false, rt: None, canvas: Some(CanvasTexture { canvas: c, ctx, gpu_levels: Some(levels) }) });
            TexId(self.textures.len() as u32 - 1)
        }

        /// 描き足して上げ直す（three の texture.needsUpdate）
        pub fn redraw_canvas(&mut self, id: TexId, draw: impl FnOnce(&OffscreenCanvasRenderingContext2d)) {
            let t = &self.textures[id.0 as usize];
            let Some(canvas) = &t.canvas else { return };
            draw(&canvas.ctx);
            if let Some(levels) = &canvas.gpu_levels {
                self.canvas_mips.as_ref().expect("canvas mip pipeline").upload(&self.device, &self.queue, &t.tex, &canvas.canvas, levels);
            } else {
                let rgba = read_canvas(&canvas.ctx, t.w, t.h);
                self.upload_levels(id, &rgba);
            }
        }

        /// URL の画像からテクスチャを作る。読む間 kit を借りるので、遊びの輪と並べるときは load_image と image_data_texture に分ける
        pub async fn image_texture(&mut self, url: &str) -> Result<TexId, String> {
            let img = load_image(url).await?;
            Ok(self.image_data_texture(&img, true))
        }
    }
}
#[cfg(target_arch = "wasm32")]
pub use web::{load_font, load_image};

#[cfg(test)]
mod tests {
    use super::*;

    fn rng(seed: &mut u64) -> u32 { *seed ^= *seed << 13; *seed ^= *seed >> 7; *seed ^= *seed << 17; (*seed >> 16) as u32 }

    /// 早見表は元の式（画素ごとの powf）と同じ値を返す: 区間の境目の前後・ミップで実際に出る 4 点平均
    #[test]
    fn srgb_table_matches_formula() {
        let table = srgb_encode_table();
        for i in 0..=SRGB_BINS {
            let mut c = i as f32 / SRGB_BINS as f32;
            for _ in 0..4 { c = f32::from_bits(c.to_bits().saturating_sub(1)); }
            for _ in 0..9 {
                assert_eq!(linear_to_srgb_fast(c, table), linear_to_srgb(c), "c = {c:e}");
                c = f32::from_bits(c.to_bits() + 1);
            }
        }
        let lut = srgb_lut();
        let mut seed = 0x9E3779B97F4A7C15u64;
        for _ in 0..4_000_000 {
            let r = rng(&mut seed);
            let c = (lut[(r & 255) as usize] + lut[(r >> 8 & 255) as usize] + lut[(r >> 16 & 255) as usize] + lut[(r >> 24) as usize]) * 0.25;
            assert_eq!(linear_to_srgb_fast(c, table), linear_to_srgb(c), "c = {c:e}");
        }
        for c in [-1.0, -0.0, 0.0, 1.0, 1.0000001, 2.0, f32::NAN, f32::INFINITY] {
            assert_eq!(linear_to_srgb_fast(c, table), linear_to_srgb(c), "c = {c:e}");
        }
        // 表が働いている（ほとんどの区間は式を呼ばない）
        assert!(table.iter().filter(|v| **v == 0xFFFF).count() < SRGB_BINS / 50);
    }

    /// 防ぐ故障: GPU のミップ（mips.wgsl の encode。境目の表を二分探し）が linear_to_srgb と違う 8 ビットを出す。
    /// 表の境目の前後・ミップで実際に出る 4 点平均・0..1 の 1/65536 刻みで、式と同じ値
    #[test]
    fn srgb_edges_match_formula() {
        let e = srgb_edges();
        let encode = |c: f32| -> u8 { let mut k = 0usize; let mut step = 128; while step > 0 { if k + step <= 255 && e[k + step] <= c { k += step; } step /= 2; } k as u8 };
        for w in e.windows(2).skip(1) { assert!(w[0] <= w[1]); }
        for &edge in &e[1..] {
            let mut c = f32::from_bits(edge.to_bits().saturating_sub(64));
            for _ in 0..128 { assert_eq!(encode(c), linear_to_srgb(c), "c = {c:e}"); c = f32::from_bits(c.to_bits() + 1); }
        }
        for i in 0..=65536 { let c = i as f32 / 65536.0; assert_eq!(encode(c), linear_to_srgb(c), "c = {c:e}"); }
        let lut = srgb_lut();
        let mut seed = 0x2545F4914F6CDD1Du64;
        for _ in 0..4_000_000 {
            let r = rng(&mut seed);
            let c = (lut[(r & 255) as usize] + lut[(r >> 8 & 255) as usize] + lut[(r >> 16 & 255) as usize] + lut[(r >> 24) as usize]) * 0.25;
            assert_eq!(encode(c), linear_to_srgb(c), "c = {c:e}");
        }
    }

    /// 0〜1 の f32 を全部（約 10 億個。ふだんは回さない: cargo test -p island-kit --release -- --ignored）
    #[test]
    #[ignore]
    fn srgb_table_matches_formula_everywhere() {
        let table = srgb_encode_table();
        for bits in 0..=1.0f32.to_bits() + 16 {
            let c = f32::from_bits(bits);
            assert_eq!(linear_to_srgb_fast(c, table), linear_to_srgb(c), "c = {c:e}");
        }
    }
}
