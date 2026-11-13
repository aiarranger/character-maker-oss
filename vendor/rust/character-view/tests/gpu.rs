// character-view の GPU の口の試験（Mac の Metal）。GPU が無い所では飛ばす。
// 防ぐ故障:
//  - GPU で作るミップ（set_gpu_mips）が CPU のミップと違う値になる（切り替えると絵が変わる）
//  - 体の濃さ（set_opacity）の既定 1 と、1 へ戻したときの絵が、足す前と変わる。1 未満で不透明の体が半透明の描き方へ移らない
//  - destroy_gpu が、同じ形を使うほかの体（share_from）の画像まで返す
use character_actor::package;
use character_view::{ActorLook, ActorTargets, ActorView, Globals, GpuMips, cpu_mip_chain, globals_layout, texture_gpu_mips};
use std::path::PathBuf;

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) { return v; }
        std::thread::yield_now();
    }
}

fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: None, force_fallback_adapter: false, apply_limit_buckets: false })).ok()?;
    block_on(adapter.request_device(&wgpu::DeviceDescriptor { required_limits: wgpu::Limits { max_storage_buffers_per_shader_stage: 8, ..wgpu::Limits::default() }.using_resolution(adapter.limits()), ..Default::default() })).ok()
}

/// 1 段の画素を読む（行の詰め物を外す）
fn read_level(device: &wgpu::Device, queue: &wgpu::Queue, tex: &wgpu::Texture, mip: u32) -> Vec<u8> {
    let (w, h) = ((tex.width() >> mip).max(1), (tex.height() >> mip).max(1));
    let row = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: mip, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = buf.slice(..).get_mapped_range().unwrap();
    (0..h).flat_map(|y| data[(y * row) as usize..(y * row + w * 4) as usize].to_vec()).collect()
}

fn noise(n: usize, mut s: u64) -> Vec<u8> {
    (0..n).map(|_| { s ^= s << 13; s ^= s >> 7; s ^= s << 17; (s >> 24) as u8 }).collect()
}

#[test]
fn gpu_mips_match_cpu() {
    let Some((device, queue)) = gpu() else { println!("GPU が無いので飛ばす"); return };
    let mips = GpuMips::new(&device, &queue);
    let mut checked = 0;
    for (k, (w, h)) in [(1u32, 9u32), (7, 1), (3, 5), (64, 64), (65, 129), (256, 640), (1024, 512)].into_iter().enumerate() {
        let mut rgba = noise((w * h * 4) as usize, 0x9E37_79B9_7F4A_7C15 ^ k as u64);
        for v in rgba.iter_mut().take((w * 4) as usize) { *v = 255; }
        for srgb in [false, true] {
            let cpu = cpu_mip_chain(w, h, &rgba, srgb);
            let tex = texture_gpu_mips(&device, &queue, &mips, w, h, &rgba, srgb);
            assert_eq!(tex.mip_level_count() as usize, cpu.len());
            for (m, want) in cpu.iter().enumerate() {
                let got = read_level(&device, &queue, &tex, m as u32);
                let bad = got.iter().zip(want).filter(|(a, b)| a != b).count();
                assert_eq!(bad, 0, "{w}x{h} srgb {srgb} 段 {m}: {bad} 個違う");
                checked += got.len();
            }
        }
    }
    println!("GPU のミップ {checked} 値が CPU と一致");
}

/// 持ち込みの主役（manifest なしの .glb。aisns の見本）
fn npc_actor() -> Option<character_actor::actor::CharacterActor> {
    let aisns = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../aisns");
    let bytes = std::fs::read(aisns.join("public/character-packages/generic-cat-4392298cc474.glb")).ok()?;
    let asset = package::read_character_file(&bytes, "moka.glb").unwrap();
    Some(package::assemble_package(&asset, None).unwrap())
}

const SIZE: u32 = 96;

/// 主役を正面から 1 枚描いて画素を返す（色 → 深度。不透明 → 半透明の順。kit と同じ順）
fn render(device: &wgpu::Device, queue: &wgpu::Queue, view: &mut ActorView, actor: &character_actor::actor::CharacterActor, gl: &wgpu::BindGroupLayout) -> (Vec<u8>, u32, u32) {
    let look = ActorLook { rim: [0.0; 3], rim_strength: 0.0, aura: [0.0; 3], aura_strength: 0.0, top: 0.0, lift: 0.0, flash: 0.0, hidden: [0.0; 3], hidden_opacity: 0.0 };
    view.write_frame(queue, actor, &look);
    let eye = glam::Vec3::new(0.0, 0.6, 2.2);
    let v = glam::Mat4::look_at_rh(eye, glam::Vec3::new(0.0, 0.6, 0.0), glam::Vec3::Y);
    let p = glam::Mat4::perspective_rh(0.8, 1.0, 0.1, 10.0);
    let mut g: Globals = bytemuck::Zeroable::zeroed();
    g.view = v.to_cols_array_2d(); g.proj = p.to_cols_array_2d(); g.view_proj = (p * v).to_cols_array_2d(); g.light_vp = glam::Mat4::IDENTITY.to_cols_array_2d();
    g.cam = [eye.x, eye.y, eye.z, 1.0]; g.hemi_sky = [1.0; 4]; g.hemi_ground = [1.0; 4]; g.key_color = [1.0; 4]; g.key_dir = [0.0, 1.0, 0.0, 0.0]; g.fog = [0.0, 0.0, 0.0, 1e9];
    let gb = wgpu::util::DeviceExt::create_buffer_init(device, &wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::bytes_of(&g), usage: wgpu::BufferUsages::UNIFORM });
    let gbg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: gl, entries: &[wgpu::BindGroupEntry { binding: 0, resource: gb.as_entire_binding() }] });
    let target = |format, usage| device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] });
    let color = target(wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
    let depth = target(wgpu::TextureFormat::Depth32Float, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let (cv, dv) = (color.create_view(&Default::default()), depth.create_view(&Default::default()));
    let mut enc = device.create_command_encoder(&Default::default());
    let (opaque, transparent);
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &cv, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.2, g: 0.3, b: 0.4, a: 1.0 }), store: wgpu::StoreOp::Store }, depth_slice: None })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &dv, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            ..Default::default()
        });
        pass.set_bind_group(0, &gbg, &[]);
        view.draw_opaque(&mut pass, actor);
        opaque = view.frame_stats().draw_calls;
        view.draw_transparent(&mut pass, actor);
        transparent = view.frame_stats().draw_calls - opaque;
    }
    queue.submit([enc.finish()]);
    (read_level(device, queue, &color, 0), opaque, transparent)
}

#[test]
fn opacity_and_destroy() {
    let Some((device, queue)) = gpu() else { println!("GPU が無いので飛ばす"); return };
    let Some(mut actor) = npc_actor() else { println!("aisns の見本の模型が無いので飛ばす"); return };
    actor.update(1.0 / 60.0, 0.0);
    let gl = globals_layout(&device);
    let targets = ActorTargets { color: wgpu::TextureFormat::Rgba8Unorm, depth: wgpu::TextureFormat::Depth32Float, mask: wgpu::TextureFormat::R8Unorm, samples: 1 };
    let mut view = ActorView::new(&device, &queue, &gl, targets);
    view.set_actor(&device, &queue, &mut actor);
    let (base, o0, t0) = render(&device, &queue, &mut view, &actor, &gl);
    let covered = base.chunks_exact(4).filter(|p| p[..3] != [51, 77, 102]).count();
    assert!(covered > 200, "主役が映っていない {covered}");
    // 半透明の体: 不透明の描き方では描かず、半透明の描き方で全部描く。絵は薄くなる
    view.set_opacity(0.4);
    let (thin, o1, t1) = render(&device, &queue, &mut view, &actor, &gl);
    assert_eq!(o1, 0, "1 未満で不透明の描き方に残っている");
    // 両面の物は three と同じく裏 → 表の 2 回
    assert!(t1 >= o0 + t0, "半透明の描き方で全部描いていない {t1} < {o0} + {t0}");
    assert_ne!(thin, base);
    // 1 へ戻すと前と同じ絵（ビットで）
    view.set_opacity(1.0);
    let (back, o2, t2) = render(&device, &queue, &mut view, &actor, &gl);
    assert_eq!((o2, t2), (o0, t0));
    assert!(back == base, "濃さを 1 に戻したのに絵が違う");
    // GPU のミップで上げても同じ絵（0 段は同じで、遠くの小さい絵でも下の段が CPU と同じ）
    let mut actor2 = npc_actor().unwrap();
    actor2.update(1.0 / 60.0, 0.0);
    let mut gview = ActorView::new(&device, &queue, &gl, targets);
    gview.set_gpu_mips(true);
    gview.set_actor(&device, &queue, &mut actor2);
    let (gpu_pic, _, _) = render(&device, &queue, &mut gview, &actor2, &gl);
    assert!(gpu_pic == base, "GPU のミップで上げた絵が CPU と違う");
    // 返す: 同じ形を使う体が残っている間は形と画像を返さない。最後の 1 体が返す
    let mut other = ActorView::share_from(&view, &device);
    assert!(!view.destroy_gpu(), "share_from の体が残っているのに返した");
    // 返した物を使うと wgpu の既定の失敗の扱い（native は panic）で落ちる
    let (still, _, _) = render(&device, &queue, &mut other, &actor, &gl);
    assert!(still.chunks_exact(4).filter(|p| p[..3] != [51, 77, 102]).count() > 200);
    assert!(other.destroy_gpu(), "最後の体が返さない");
    assert!(gview.destroy_gpu());
    println!("濃さ 1: 不透明 {o0}・半透明 {t0} 回 | 0.4: 半透明 {t1} 回 | 映った画素 {covered}");
}
