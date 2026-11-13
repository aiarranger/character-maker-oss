// GPU を使う確かめ（Mac の Metal で Kit::headless を描いて読み戻す）。GPU の鍵を取って流す:
// python3 ../scripts/gpulock.py run 'island-kit gpu' -- cargo test -p island-kit --test gpu
use glam::{Mat4, Vec3};
use island_kit::geometry::*;
use island_kit::{Color, Gpu, Kit, Material, RenderOpts};

fn block_on<F: std::future::Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = f.as_mut().poll(&mut cx) { return v; }
        std::thread::yield_now();
    }
}

fn gpu() -> Gpu { block_on(Gpu::new()).expect("GPU が無い") }

/// 1 段の画素を読む（行の詰め物を外す。bytes は 1 画素の大きさ）
fn read_level(gpu: &Gpu, tex: &wgpu::Texture, mip: u32, bytes: u32) -> (Vec<u8>, u32, u32) {
    let (w, h) = ((tex.width() >> mip).max(1), (tex.height() >> mip).max(1));
    let row = (w * bytes).div_ceil(256) * 256;
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut enc = gpu.device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo { texture: tex, mip_level: mip, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    gpu.queue.submit(Some(enc.finish()));
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    gpu.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = buf.slice(..).get_mapped_range().unwrap();
    let mut out = Vec::with_capacity((w * h * bytes) as usize);
    for y in 0..h as usize { out.extend_from_slice(&data[y * row as usize..y * row as usize + (w * bytes) as usize]); }
    (out, w, h)
}

fn half(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1f) as i32;
    let f = (h & 0x3ff) as f32;
    match e { 0 => s * f * 2f32.powi(-24), 31 => f32::NAN, _ => s * (1.0 + f / 1024.0) * 2f32.powi(e - 15) }
}
/// Rgba16Float の 1 画素
fn px16(data: &[u8], w: u32, x: u32, y: u32) -> [f32; 4] {
    let o = ((y * w + x) * 8) as usize;
    [0, 1, 2, 3].map(|c| half(u16::from_le_bytes([data[o + c * 2], data[o + c * 2 + 1]])))
}

/// 原点を見る箱 1 つの場面
fn red_box(kit: &mut Kit) -> island_kit::NodeId {
    kit.camera.position = Vec3::new(0.0, 0.0, 5.0);
    kit.camera.look_at(Vec3::ZERO);
    let g = kit.geometry(&box_(1.0, 1.0, 1.0));
    let m = kit.material(Material::basic().with_color(Color::rgb(1.0, 0.0, 0.0)));
    kit.scene.mesh(None, g, m)
}

/// 防ぐ故障: 画面なしの Kit が描けない（以下の確かめの土台）。箱が真ん中に赤く、隅は消した色で出る
#[test]
fn headless_draws_a_box() {
    let gpu = gpu();
    let mut kit = Kit::headless(&gpu, 64, 64);
    red_box(&mut kit);
    kit.render(&RenderOpts { clear: Color::rgb(0.0, 0.0, 1.0), ..Default::default() });
    let (px, w, _) = read_level(&gpu, kit.headless_output().unwrap(), 0, 4);
    let at = |x: u32, y: u32| { let o = ((y * w + x) * 4) as usize; [px[o], px[o + 1], px[o + 2]] };
    assert_eq!(at(32, 32), [255, 0, 0]);
    assert_eq!(at(1, 1), [0, 0, 255]);
}

/// 防ぐ故障: Device を使い回すと、前の Kit の物が残る・次の Kit が描けない。release_gpu のあとの Kit は描かず、同じ Gpu で作った次の Kit は描ける
#[test]
fn shared_gpu_survives_release() {
    let gpu = gpu();
    let mut a = Kit::headless(&gpu, 32, 32);
    red_box(&mut a);
    a.render(&RenderOpts::default());
    a.release_gpu();
    a.render(&RenderOpts::default());
    let mut b = Kit::headless(&gpu, 32, 32);
    red_box(&mut b);
    b.render(&RenderOpts::default());
    let (px, w, _) = read_level(&gpu, b.headless_output().unwrap(), 0, 4);
    let o = ((16 * w + 16) * 4) as usize;
    assert_eq!(&px[o..o + 3], &[255, 0, 0]);
    let _ = Mat4::IDENTITY;
    let _ = px16;
}

fn noise(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed;
    (0..n).map(|_| { x ^= x << 13; x ^= x >> 7; x ^= x << 17; (x >> 24) as u8 }).collect()
}

/// 防ぐ故障: GPU で作ったミップが CPU の downsample と違い、set_gpu_mips(true) で遠くの絵の色が変わる。全部の段の全部の画素が同じ
#[test]
fn gpu_mips_match_cpu() {
    let gpu = gpu();
    let mut kit = Kit::headless(&gpu, 8, 8);
    let mut checked = 0usize;
    for (k, (w, h)) in [(1u32, 9u32), (7, 1), (3, 5), (64, 64), (65, 129), (256, 640), (1024, 512)].into_iter().enumerate() {
        let mut rgba = noise((w * h * 4) as usize, 0x9E37_79B9_7F4A_7C15 ^ k as u64);
        // 平らな所と境目（同じ値の 4 点・端の値）も混ぜる
        for v in rgba.iter_mut().take((w * 4) as usize) { *v = 255; }
        for srgb in [false, true] {
            let cpu = island_kit::texture::cpu_mip_chain(w, h, &rgba, srgb);
            kit.set_gpu_mips(true);
            let on_gpu = kit.data_texture(w, h, &rgba, srgb);
            let b = kit.wgpu_texture(on_gpu).clone();
            assert!(b.usage().contains(wgpu::TextureUsages::RENDER_ATTACHMENT));
            assert_eq!(b.mip_level_count() as usize, cpu.len());
            for m in 0..b.mip_level_count() {
                let pa = &cpu[m as usize];
                let (pb, lw, lh) = read_level(&gpu, &b, m, 4);
                assert_eq!(pa.len(), pb.len());
                let bad = pa.iter().zip(&pb).filter(|(x, y)| x != y).count();
                assert_eq!(bad, 0, "{w}x{h} srgb={srgb} mip {m} ({lw}x{lh}): {bad} 個の値が違う");
                checked += pa.len();
            }
        }
    }
    assert!(checked > 3_000_000);
}

/// 真ん中の 1 画素（画面なしの出口。sRGB の 8 ビット）
fn center(gpu: &Gpu, kit: &Kit) -> [u8; 3] {
    let (px, w, h) = read_level(gpu, kit.headless_output().unwrap(), 0, 4);
    let o = (((h / 2) * w + w / 2) * 4) as usize;
    [px[o], px[o + 1], px[o + 2]]
}

/// 防ぐ故障: 一部の上げ直しで、ほかの所が消える・下の段が古いまま（遠くで前の絵が出る）。全体の上げ直しも作るときと同じ段になる
#[test]
fn write_texture_updates_region_and_mips() {
    let gpu = gpu();
    let mut kit = Kit::headless(&gpu, 8, 8);
    kit.set_gpu_mips(true);
    let (w, h) = (16u32, 8u32);
    for srgb in [false, true] {
        let mut img = noise((w * h * 4) as usize, 7);
        let id = kit.data_texture(w, h, &img, srgb);
        let patch = noise(5 * 3 * 4, 99);
        kit.write_texture(id, 4, 2, 5, 3, &patch);
        for yy in 0..3 { for xx in 0..5 { for c in 0..4 { img[(((2 + yy) * w + 4 + xx) * 4 + c) as usize] = patch[((yy * 5 + xx) * 4 + c) as usize]; } } }
        let want = island_kit::texture::cpu_mip_chain(w, h, &img, srgb);
        let tex = kit.wgpu_texture(id).clone();
        for m in 0..tex.mip_level_count() { assert_eq!(read_level(&gpu, &tex, m, 4).0, want[m as usize], "srgb={srgb} mip {m}"); }
        let full = noise((w * h * 4) as usize, 3);
        kit.write_texture(id, 0, 0, w, h, &full);
        let want = island_kit::texture::cpu_mip_chain(w, h, &full, srgb);
        for m in 0..tex.mip_level_count() { assert_eq!(read_level(&gpu, &tex, m, 4).0, want[m as usize], "full srgb={srgb} mip {m}"); }
    }
    // GPU でミップを作らない画像も、全体の上げ直しは検証のエラーなく通る
    kit.set_gpu_mips(false);
    let id = kit.data_texture(4, 4, &[9; 64], true);
    kit.write_texture(id, 0, 0, 4, 4, &[200; 64]);
    kit.write_texture(id, 1, 1, 2, 2, &[10; 16]);
}

/// 防ぐ故障: 手放した形の節点が描こうとして検証のエラーになり、その画面が全部消える。手放すと描かれず、update_geometry で戻る
#[test]
fn destroyed_geometry_is_skipped() {
    let gpu = gpu();
    let mut kit = Kit::headless(&gpu, 32, 32);
    let n = red_box(&mut kit);
    let clear = RenderOpts { clear: Color::rgb(0.0, 0.0, 1.0), ..Default::default() };
    kit.render(&clear);
    assert_eq!(center(&gpu, &kit), [255, 0, 0]);
    let geo = match &kit.scene[n].kind { island_kit::NodeKind::Mesh { geo, .. } => *geo, _ => panic!() };
    kit.scene[n].cast_shadow = true;
    kit.destroy_geometry(geo);
    kit.render(&clear);
    assert_eq!(center(&gpu, &kit), [0, 0, 255]);
    kit.update_geometry(geo, &box_(1.0, 1.0, 1.0));
    kit.render(&clear);
    assert_eq!(center(&gpu, &kit), [255, 0, 0]);
}

/// 防ぐ故障: 256 バイトを超える値が Custom の材質に渡らない。k_storage の 21 個目（336 バイト目）の色で描ける。大きくしても読める
#[test]
fn storage_reaches_custom_material() {
    let gpu = gpu();
    let mut kit = Kit::headless(&gpu, 32, 32);
    kit.camera.position = Vec3::new(0.0, 0.0, 5.0);
    kit.camera.look_at(Vec3::ZERO);
    let wgsl = "struct U { p: vec4f };\n@group(2) @binding(0) var<uniform> u: U;\n\
        @vertex fn vs(v: VIn, i: IIn) -> @builtin(position) vec4f { return g.view_proj * inst_model(i) * vec4f(v.pos, 1.0); }\n\
        @fragment fn fs() -> @location(0) vec4f { return k_storage[20u + u32(u.p.x)]; }\n";
    let m = kit.material(Material::custom(island_kit::CustomShader { wgsl: wgsl.into(), uniform: vec![0; 16], ..Default::default() }));
    let plain = kit.material(Material::basic().with_color(Color::rgb(1.0, 0.0, 0.0)));
    let g = kit.geometry(&box_(1.0, 1.0, 1.0));
    kit.scene.mesh(None, g, m);
    let side = kit.scene.mesh(None, g, plain);
    kit.scene[side].position = Vec3::new(30.0, 0.0, 0.0);
    let mut data = vec![0f32; 4 * 24];
    data[80..84].copy_from_slice(&[0.0, 1.0, 0.0, 1.0]);
    kit.set_storage(m, bytemuck::cast_slice(&data));
    kit.render(&RenderOpts::default());
    assert_eq!(center(&gpu, &kit), [0, 255, 0]);
    // 大きくしたら作り直す（1 つ先の値を読む）
    let mut data = vec![0f32; 4 * 64];
    data[84..88].copy_from_slice(&[1.0, 1.0, 0.0, 1.0]);
    kit.set_storage(m, bytemuck::cast_slice(&data));
    kit.set_uniform(m, bytemuck::cast_slice(&[1.0f32, 0.0, 0.0, 0.0]));
    kit.render(&RenderOpts::default());
    assert_eq!(center(&gpu, &kit), [255, 255, 0]);
}

/// 赤い箱（原点）と、その奥の緑の大きな板（z = -10）。カメラは z = 5 から原点を見る
fn two_boxes(kit: &mut Kit) -> (island_kit::NodeId, island_kit::NodeId) {
    let red = red_box(kit);
    let g = kit.geometry(&box_(30.0, 30.0, 1.0));
    let m = kit.material(Material::basic().with_color(Color::rgb(0.0, 1.0, 0.0)));
    let green = kit.scene.mesh(None, g, m);
    kit.scene[green].position = Vec3::new(0.0, 0.0, -10.0);
    (red, green)
}

/// three の Reflector と同じ式で、斜めの near（clip 面）を入れた three の投影（奥行き -1..1）
fn oblique_gl(view: Mat4, plane_world: glam::Vec4, aspect: f32) -> Mat4 {
    let mut p = Mat4::perspective_rh_gl(50f32.to_radians(), aspect, 0.1, 2000.0);
    let mut c = view.inverse().transpose() * plane_world;
    let q = glam::Vec4::new((c.x.signum() + p.z_axis.x) / p.x_axis.x, (c.y.signum() + p.z_axis.y) / p.y_axis.y, -1.0, (1.0 + p.z_axis.z) / p.w_axis.z);
    c *= 2.0 / c.dot(q);
    p.x_axis.z = c.x;
    p.y_axis.z = c.y;
    p.z_axis.z = c.z + 1.0;
    p.w_axis.z = c.w;
    p
}

/// 防ぐ故障: 鏡（three の Reflector）の写しで、鏡の手前の物が写る・隠す節点が写る・的が Custom の材質で読めない。
/// 行列を渡したカメラで的へ描き、隠す節点（赤い箱）は写らず、斜めの near の手前（赤い箱）は切れ、的は Custom の tex0 で読める
#[test]
fn render_view_to_with_hide_and_oblique_near() {
    let gpu = gpu();
    let mut kit = Kit::headless(&gpu, 32, 32);
    let (red, _) = two_boxes(&mut kit);
    let rt = kit.render_target(16, 16);
    let cam = kit.camera;
    let center_of = |kit: &Kit| { let (d, w, h) = read_level(&gpu, kit.wgpu_texture(rt), 0, 8); px16(&d, w, w / 2, h / 2) };
    // ふつうの投影: 赤い箱が真ん中
    let plain = island_kit::RawView::from_camera(&cam);
    kit.render_view_to(rt, &plain, &island_kit::ViewDraw::default());
    let c = center_of(&kit);
    assert!(c[0] > 0.99 && c[1] < 0.01, "{c:?}");
    // 隠す: 奥の緑
    kit.render_view_to(rt, &plain, &island_kit::ViewDraw { hide: &[red], ..Default::default() });
    let c = center_of(&kit);
    assert!(c[0] < 0.01 && c[1] > 0.99, "{c:?}");
    assert!(kit.scene[red].visible);
    // three の投影と同じ行列（奥行き -1..1 を直す）。斜めの near を z = -2（-z 向き）に置くと、手前の赤い箱は写らない
    let gl = Mat4::perspective_rh_gl(50f32.to_radians(), cam.aspect, cam.near, cam.far);
    kit.render_view_to(rt, &island_kit::RawView::from_three(cam.view(), gl, cam.position), &island_kit::ViewDraw::default());
    let c = center_of(&kit);
    assert!(c[0] > 0.99 && c[1] < 0.01, "three の投影: {c:?}");
    let ob = oblique_gl(cam.view(), glam::Vec4::new(0.0, 0.0, -1.0, -2.0), cam.aspect);
    kit.render_view_to(rt, &island_kit::RawView::from_three(cam.view(), ob, cam.position), &island_kit::ViewDraw { clear: Some(Color::rgb(0.0, 0.0, 1.0)), clear_alpha: 1.0, ..Default::default() });
    let c = center_of(&kit);
    assert!(c[0] < 0.01 && c[1] > 0.99, "斜めの near: {c:?}");
    // 的を Custom の tex0 で読む板を画面いっぱいに
    let wgsl = "struct O { @builtin(position) clip: vec4f, @location(0) uv: vec2f };\n\
        @vertex fn vs(v: VIn, i: IIn) -> O { var o: O; o.clip = vec4f(v.pos.xy * 2.0, 0.0, 1.0); o.uv = v.uv; return o; }\n\
        @fragment fn fs(i: O) -> @location(0) vec4f { return vec4f(textureSampleLevel(tex0, smp0, vec2f(i.uv.x, 1.0 - i.uv.y), 0.0).rgb, 1.0); }\n";
    let m = kit.material(Material::custom(island_kit::CustomShader { wgsl: wgsl.into(), textures: vec![rt], ..Default::default() }));
    let g = kit.geometry(&plane(1.0, 1.0));
    let q = kit.scene.mesh(None, g, m);
    kit.scene[q].frustum_culled = false;
    kit.scene[q].render_order = 10;
    kit.render(&RenderOpts::default());
    assert_eq!(center(&gpu, &kit), [0, 255, 0]);
}

/// 防ぐ故障: 透過する物（three の transmission）の後ろが写らない・透過する物そのものや除く物が透過用の的に入る・ミップが無い。
/// 赤い箱（不透明）の手前に除く青い箱、その手前に透過用の的を読む板。的は赤（青は除く・板は自分を読むので入らない）で、ミップの段も赤、画面の板も赤
#[test]
fn transmission_pass_draws_opaque_only() {
    let gpu = gpu();
    let mut kit = Kit::headless(&gpu, 32, 32);
    red_box(&mut kit);
    let g = kit.geometry(&box_(0.5, 0.5, 0.5));
    let m = kit.material(Material::basic().with_color(Color::rgb(0.0, 0.0, 1.0)));
    let blue = kit.scene.mesh(None, g, m);
    kit.scene[blue].position = Vec3::new(0.0, 0.0, 1.0);
    let t = kit.transmission_target();
    let wgsl = "@vertex fn vs(v: VIn, i: IIn) -> @builtin(position) vec4f { return g.view_proj * inst_model(i) * vec4f(v.pos, 1.0); }\n\
        @fragment fn fs(@builtin(position) p: vec4f) -> @location(0) vec4f { return vec4f(textureSampleLevel(tex0, smp0, p.xy / k_res(), 1.0).rgb, 1.0); }\n";
    let gm = kit.material(Material::custom(island_kit::CustomShader { wgsl: wgsl.into(), textures: vec![t], ..Default::default() }));
    let pg = kit.geometry(&plane(0.4, 0.4));
    let glass = kit.scene.mesh(None, pg, gm);
    kit.scene[glass].position = Vec3::new(0.0, 0.0, 2.0);
    kit.transmission = Some(island_kit::Transmission { scale: 0.5, exclude: vec![blue] });
    kit.render(&RenderOpts { clear: Color::rgb(0.0, 0.0, 0.0), ..Default::default() });
    assert_eq!(kit.tex_size(t), (16, 16));
    let tex = kit.wgpu_texture(t).clone();
    assert_eq!(tex.mip_level_count(), 5);
    for m in 0..2 {
        let (d, w, h) = read_level(&gpu, &tex, m, 8);
        let c = px16(&d, w, w / 2, h / 2);
        assert!(c[0] > 0.99 && c[1] < 0.01 && c[2] < 0.01, "mip {m}: {c:?}");
    }
    // 4 × 4 の段は箱の赤と周りの黒の平均（ミップを作った）
    let (d, w, h) = read_level(&gpu, &tex, 2, 8);
    let c = px16(&d, w, w / 2, h / 2);
    assert!(c[0] > 0.1 && c[0] < 0.9 && c[2] < 0.01, "mip 2: {c:?}");
    assert_eq!(center(&gpu, &kit), [255, 0, 0]);
    // 切ると描かない（的はそのまま）
    kit.transmission = None;
    kit.render(&RenderOpts::default());
}

/// 防ぐ故障: 両面の透過する物の裏の面が透過用の的に入らない（three の renderTransmissionPass は不透明な物のあとに BackSide で重ねる）。
/// 赤い箱の手前に両面の「ガラス」の箱（裏の面は緑・表の面は透過用の的を読む）。的の真ん中は裏の面の緑（ミップも）、画面の真ん中も緑。
/// 半透明（main では裏 → 表の 2 回に分ける）でも、的に入るのは裏の面だけ（表の面が入ると手前の赤になる）
#[test]
fn transmission_draws_double_sided_back_faces() {
    let gpu = gpu();
    for transparent in [false, true] {
        let mut kit = Kit::headless(&gpu, 32, 32);
        red_box(&mut kit);
        let t = kit.transmission_target();
        let wgsl = "@vertex fn vs(v: VIn, i: IIn) -> @builtin(position) vec4f { return g.view_proj * inst_model(i) * vec4f(v.pos, 1.0); }\n\
            @fragment fn fs(@builtin(position) p: vec4f, @builtin(front_facing) f: bool) -> @location(0) vec4f {\n\
              let s = textureSampleLevel(tex0, smp0, p.xy / k_res(), 0.0).rgb;\n\
              if (f) { return vec4f(s, 1.0); }\n\
              return vec4f(0.0, 1.0, s.b + 0.25, 1.0);\n\
            }\n";
        let mut m = Material::custom(island_kit::CustomShader { wgsl: wgsl.into(), textures: vec![t], ..Default::default() });
        m.side = island_kit::Side::Double;
        m.transparent = transparent;
        let gm = kit.material(m);
        let g = kit.geometry(&box_(0.6, 0.6, 0.6));
        let glass = kit.scene.mesh(None, g, gm);
        kit.scene[glass].position = Vec3::new(0.0, 0.0, 2.0);
        kit.transmission = Some(island_kit::Transmission::default());
        kit.render(&RenderOpts { clear: Color::rgb(0.0, 0.0, 0.0), ..Default::default() });
        let tex = kit.wgpu_texture(t).clone();
        for mip in 0..2 {
            let (d, w, h) = read_level(&gpu, &tex, mip, 8);
            let c = px16(&d, w, w / 2, h / 2);
            assert!(c[0] < 0.01 && c[1] > 0.99 && (c[2] - 0.25).abs() < 0.01, "transparent {transparent} mip {mip}: {c:?}");
        }
        let c = center(&gpu, &kit);
        assert!(c[0] == 0 && c[1] == 255, "transparent {transparent} 画面: {c:?}");
    }
}

/// 防ぐ故障: 1 つの材質のシェーダーの作成エラーで、そのコマの描画が全部止まる。作れない材質は描かずに飛ばし、エラーを 1 回だけ返し、ほかの物は描く
#[test]
fn broken_shader_is_isolated() {
    let gpu = gpu();
    let mut kit = Kit::headless(&gpu, 32, 32);
    red_box(&mut kit);
    let bad = kit.material(Material::custom(island_kit::CustomShader { label: "こわれた".into(), wgsl: "@vertex fn vs() -> @builtin(position) vec4f { return 1.0; }\n@fragment fn fs() -> @location(0) vec4f { return vec4f(0.0); }\n".into(), ..Default::default() }));
    let g = kit.geometry(&box_(0.2, 0.2, 0.2));
    let n = kit.scene.mesh(None, g, bad);
    kit.scene[n].position = Vec3::new(1.0, 0.0, 0.0);
    // 入口の名前が無い（パイプラインだけ作れない）物も
    let bad2 = kit.material(Material::custom(island_kit::CustomShader { label: "入口なし".into(), wgsl: "@fragment fn fs() -> @location(0) vec4f { return vec4f(0.0); }\n".into(), vs: "nope".into(), ..Default::default() }));
    let n2 = kit.scene.mesh(None, g, bad2);
    kit.scene[n2].position = Vec3::new(-1.0, 0.0, 0.0);
    for _ in 0..3 { kit.render(&RenderOpts { clear: Color::rgb(0.0, 0.0, 1.0), ..Default::default() }); }
    assert_eq!(center(&gpu, &kit), [255, 0, 0]);
    let errs = kit.take_shader_errors();
    assert_eq!(errs.len(), 2, "{errs:?}");
    assert!(errs.iter().any(|e| e.contains("こわれた")) && errs.iter().any(|e| e.contains("入口なし")), "{errs:?}");
    assert!(kit.material_failed(bad) && kit.material_failed(bad2));
    assert!(kit.take_shader_errors().is_empty());
    // 確かめ終わるまで描かない形でも、よい材質は次のコマから描く
    kit.set_wait_for_shader_check(true);
    let ok = kit.material(Material::custom(island_kit::CustomShader { wgsl: "@vertex fn vs(v: VIn, i: IIn) -> @builtin(position) vec4f { return g.view_proj * inst_model(i) * vec4f(v.pos, 1.0); }\n@fragment fn fs() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }\n".into(), ..Default::default() }));
    let big = kit.geometry(&box_(3.0, 3.0, 0.1));
    let n3 = kit.scene.mesh(None, big, ok);
    kit.scene[n3].position = Vec3::new(0.0, 0.0, 1.0);
    kit.render(&RenderOpts::default());
    kit.render(&RenderOpts::default());
    assert_eq!(center(&gpu, &kit), [0, 255, 0]);
    assert!(!kit.material_failed(ok));
}

/// 防ぐ故障: 骨付きの部分（NPC の顔）のモーフの重みを変えても形が変わらない（まばたきしない）・影の型が元の形のまま。
/// 骨 1 本の箱に「右へ 1.5 動く」差分を持たせ、重み 1 で真ん中から右へ動き、0 に戻すと真ん中に戻る
#[test]
fn skinned_morph_weights_move_vertices() {
    let gpu = gpu();
    let mut kit = Kit::headless(&gpu, 32, 32);
    kit.camera.position = Vec3::new(0.0, 0.0, 5.0);
    kit.camera.look_at(Vec3::ZERO);
    let shape = box_(0.8, 0.8, 0.8);
    let n = shape.positions.len();
    let geo = kit.geometry_skinned(&shape, &vec![[0, 0, 0, 0]; n], &vec![[1.0, 0.0, 0.0, 0.0]; n]);
    kit.set_geometry_morphs(geo, &shape, &[vec![[1.5, 0.0, 0.0]; n]], &[vec![[0.0; 3]; n]]);
    let holder = kit.scene.group(None);
    let bone = kit.scene.object(Some(holder));
    let sk = kit.scene.new_skeleton(vec![bone], vec![Mat4::IDENTITY]);
    let m = kit.material(Material::basic().with_color(Color::rgb(1.0, 0.0, 0.0)));
    let mesh = kit.scene.skinned(Some(holder), geo, m, sk);
    kit.scene[mesh].frustum_culled = false;
    kit.scene[mesh].cast_shadow = true;
    let sun = kit.scene.dir_light(None, Color::WHITE, 1.0);
    kit.scene[sun].cast_shadow = true;
    let at = |kit: &Kit, x: u32| { let (px, w, h) = read_level(&gpu, kit.headless_output().unwrap(), 0, 4); let o = (((h / 2) * w + x) * 4) as usize; [px[o], px[o + 1], px[o + 2]] };
    let opts = RenderOpts { clear: Color::rgb(0.0, 0.0, 1.0), ..Default::default() };
    kit.render(&opts);
    assert_eq!(at(&kit, 16), [255, 0, 0]);
    assert_eq!(at(&kit, 26), [0, 0, 255]);
    kit.set_morph_weights(mesh, &[1.0]);
    kit.render(&opts);
    assert_eq!(at(&kit, 16), [0, 0, 255]);
    assert_eq!(at(&kit, 26), [255, 0, 0]);
    assert_eq!(kit.morph_weights(mesh), &[1.0]);
    kit.set_morph_weights(mesh, &[0.0]);
    kit.render(&opts);
    assert_eq!(at(&kit, 16), [255, 0, 0]);
}

/// 防ぐ故障: 縁のなめらかさ（kit.set_aa）を切り替えると描けない（的とパイプラインのサンプル数が食い違って wgpu の検証エラー）・切り替えが効かない。
/// 既定は MSAA 4 倍。斜めの赤い箱の縁は、なし（Off）だと赤と青の 2 色だけ、MSAA 4 倍と SMAA は間の色が出る。
/// 切り替えの前に作った別のカメラの的・透過用の的にも描ける（的を今のサンプル数で作り直す）
#[test]
fn aa_modes_switch_and_smooth_edges() {
    use island_kit::{Aa, Rot};
    let gpu = gpu();
    for aa in [Aa::Msaa4, Aa::Off, Aa::Smaa] {
        let mut kit = Kit::headless(&gpu, 96, 96);
        assert_eq!(kit.aa(), Aa::Msaa4);
        let rt = kit.render_target(16, 16);
        kit.transmission_target();
        kit.set_aa(aa);
        assert_eq!(kit.aa(), aa);
        let b = red_box(&mut kit);
        kit.scene[b].rotation = Rot::Euler { x: 0.0, y: 0.0, z: 0.35, order: island_kit::math::EulerOrder::XYZ };
        let cam = kit.camera;
        kit.render_to(rt, &cam);
        kit.transmission = Some(island_kit::Transmission::default());
        kit.render(&RenderOpts { clear: Color::rgb(0.0, 0.0, 1.0), ..Default::default() });
        let (px, w, h) = read_level(&gpu, kit.headless_output().unwrap(), 0, 4);
        let at = |x: u32, y: u32| { let o = ((y * w + x) * 4) as usize; [px[o], px[o + 1], px[o + 2]] };
        assert_eq!(at(w / 2, h / 2), [255, 0, 0], "{aa:?}");
        assert_eq!(at(1, 1), [0, 0, 255], "{aa:?}");
        let mixed = (0..w * h).filter(|i| { let c = at(i % w, i / w); c[0] > 20 && c[0] < 235 }).count();
        match aa {
            Aa::Off => assert_eq!(mixed, 0, "なしで間の色"),
            _ => assert!(mixed > 20, "{aa:?}: 間の色が {mixed} 画素"),
        }
        let (d, rw, rh) = read_level(&gpu, kit.wgpu_texture(rt), 0, 8);
        let c = px16(&d, rw, rw / 2, rh / 2);
        assert!(c[0] > 0.99 && c[2] < 0.01, "{aa:?} 別のカメラの的: {c:?}");
    }
}
