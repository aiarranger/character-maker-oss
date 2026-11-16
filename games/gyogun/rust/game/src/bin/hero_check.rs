// 主役だけを確かめる小さな道具（親の結合の前に、部品の見た目を確かめる）。
//   hero_check png <出力.png> <id> <f> [weapon]         f コマ目（下の場面）の主役を描いて PNG にする
//   id が studio なら、環境変数 GYOGUN_STUDIO_ZIP の .character.zip（Studio の「魚群繚乱で遊ぶ」が作る ZIP）を主役にする
use glam::{Mat4, Vec3};
use gyogun_game::delivery::Catalog;
use gyogun_game::gfx::{self, Gfx};
use gyogun_game::hero::actor::{ActionKind, CharacterActor, StartOptions};
use gyogun_game::hero::hero::{HeroInfo, HeroView, GUEST_ID, gait_for, guest_actor, guest_hero, to_hero_list};
use gyogun_game::view::materials::ORDER_OPAQUE;
use std::sync::Arc;

/// ゲームと同じ手元の配布（pnpm dev が配る場所。GYOGUN_CATALOG で替えられる）
fn catalog() -> Catalog {
    let url = std::env::var("GYOGUN_CATALOG").ok().filter(|u| !u.is_empty()).unwrap_or_else(|| gyogun_game::assets::CATALOG_URL_NATIVE.to_string());
    Catalog::open(&url, None, None).unwrap()
}

/// 主役を読む（配布の主役。studio は Studio で作ったキャラの ZIP）
fn load(id: &str) -> (HeroInfo, CharacterActor) {
    if id == GUEST_ID {
        let zip = gyogun_game::platform::studio_zip().expect("GYOGUN_STUDIO_ZIP に .character.zip の場所を渡してください");
        let (info, _) = guest_hero(&zip).unwrap();
        let actor = guest_actor(info.package.as_ref().unwrap(), gait_for(info.height)).unwrap();
        return (info, actor);
    }
    let cat = catalog();
    let info = to_hero_list(cat.heroes().unwrap()).into_iter().find(|h| h.id == id).unwrap();
    let actor = cat.hero(id, gait_for(info.height)).unwrap();
    (info, actor)
}

/// 確かめる場面
fn scene(hero: &mut CharacterActor, f: i32) {
    let sp = if f < 30 { 0.0 } else if f < 60 { 1.5 } else if f < 90 { 3.5 } else { 0.0 };
    let gr = !(335..370).contains(&f);
    hero.move_(sp, gr, if f < 320 { 0.85 } else { 0.0 }, None);
    match f {
        90 => { hero.start(ActionKind::Attack, StartOptions { seconds: Some(0.5), from: Some(0.2), to: Some(0.8), ease_swing: true, ..Default::default() }); }
        130 => { hero.start(ActionKind::Skill, StartOptions { seconds: Some(2.5), cycle: Some(1.0 / 0.9), ..Default::default() }); }
        300 => { hero.stop(ActionKind::Skill); hero.start(ActionKind::Dodge, StartOptions { seconds: Some(0.38), ..Default::default() }); }
        330 => { hero.start(ActionKind::Jump, StartOptions { air_seconds: Some(0.8), ..Default::default() }); }
        380 => hero.hit(Some(0.42), None),
        430 => { hero.start(ActionKind::Skill, StartOptions::default()); }
        560 => hero.hit(None, None),
        580 => { hero.start(ActionKind::Rest, StartOptions::default()); }
        610 => hero.stop(ActionKind::Rest),
        _ => {}
    }
}

fn png(out: &str, id: &str, frame: i32, weapon: Option<&str>) {
    use winit::application::ApplicationHandler;
    use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
    use winit::window::{Window, WindowId};
    struct App { out: String, id: String, frame: i32, weapon: Option<String> }
    impl ApplicationHandler for App {
        fn resumed(&mut self, el: &ActiveEventLoop) {
            let win = Arc::new(el.create_window(Window::default_attributes().with_visible(false).with_inner_size(winit::dpi::PhysicalSize::new(64, 64))).unwrap());
            let g = pollster::block_on(Gfx::new(win)).unwrap();
            if self.frame < 0 { run_full(&g, &self.out, &self.id); } else { run(&g, &self.out, &self.id, self.frame, self.weapon.as_deref()); }
            el.exit();
        }
        fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, _: winit::event::WindowEvent) {}
    }
    let el = EventLoop::new().unwrap();
    el.set_control_flow(ControlFlow::Poll);
    let mut app = App { out: out.to_string(), id: id.to_string(), frame, weapon: weapon.map(String::from) };
    el.run_app(&mut app).unwrap();
}

fn run(g: &Gfx, out: &str, id: &str, frame: i32, weapon: Option<&str>) {
    let (info, actor) = load(id);
    let info = &info;
    let wid = weapon.map(String::from).or_else(|| gyogun_game::hero::hero::default_weapon(id).map(String::from)).unwrap_or_else(|| "cake_sword".into());
    let mut view = HeroView::new(g);
    view.set_hero(g, info, actor, &wid);
    // 場面を部品へ直接流す（Player を介さない）。入れ物の行列は主役の拡大だけ
    let scale = gyogun_game::hero::hero::hero_scale(info.height);
    let a = view.actor_mut().unwrap();
    a.set_parent_world(Mat4::from_scale(Vec3::splat(scale)));
    for f in 0..=frame {
        scene(a, f);
        a.update(1.0 / 60.0, f as f64 / 60.0);
    }
    view.force_frame(g);
    // カメラ（寄り）
    let (w, h) = (900u32, 1100u32);
    let eye = Vec3::new(0.9, 1.5, 3.6);
    let look = Vec3::new(0.0, 1.05, 0.0);
    let v = Mat4::look_at_rh(eye, look, Vec3::Y);
    let p = Mat4::perspective_rh(38f32.to_radians(), w as f32 / h as f32, 0.3, 1200.0);
    let vp = p * v;
    let hemi = |s: &str| { let c = gfx::hex(s); [c[0], c[1], c[2], 1.0] };
    let key = gfx::hex("#ffd2a8");
    let kd = Vec3::new(0.4, 1.0, 0.5).normalize();
    let fog = gfx::hex("#6a4a78");
    let globals = gfx::Globals {
        view: v.to_cols_array_2d(), proj: p.to_cols_array_2d(), view_proj: vp.to_cols_array_2d(), light_vp: Mat4::IDENTITY.to_cols_array_2d(),
        cam: [eye.x, eye.y, eye.z, 1.0], hemi_sky: hemi("#c9c2f2"), hemi_ground: hemi("#4a3a40"), key_color: [key[0] * 2.3, key[1] * 2.3, key[2] * 2.3, 1.0], key_dir: [kd.x, kd.y, kd.z, 0.0],
        fog: [fog[0], fog[1], fog[2], 0.0085], rim: [1.0, 1.0, 1.0, 0.0], misc: [frame as f32 / 60.0, h as f32, w as f32, 0.0],
    };
    g.queue.write_buffer(&g.globals_buf, 0, bytemuck::bytes_of(&globals));
    let mk = |fmt, samples, usage| g.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: samples, dimension: wgpu::TextureDimension::D2, format: fmt, usage, view_formats: &[] });
    let msaa = mk(gfx::HDR, gfx::SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let depth = mk(gfx::DEPTH, gfx::SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let res = mk(gfx::HDR, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
    let (mv, dv, rv) = (msaa.create_view(&Default::default()), depth.create_view(&Default::default()), res.create_view(&Default::default()));
    let mut enc = g.device.create_command_encoder(&Default::default());
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &mv, resolve_target: Some(&rv), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.55, g: 0.6, b: 0.7, a: 1.0 }), store: wgpu::StoreOp::Store }, depth_slice: None })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &dv, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            ..Default::default()
        });
        pass.set_bind_group(0, &g.globals_bg, &[]);
        view.draw(g, &mut pass, ORDER_OPAQUE);
        view.draw(g, &mut pass, 0);
    }
    let row = (w * 8 + 255) & !255;
    let buf = g.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    enc.copy_texture_to_buffer(res.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
    g.queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |r| r.unwrap());
    g.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = buf.slice(..).get_mapped_range().unwrap();
    let mut img = image::RgbImage::new(w, h);
    let half = |b: &[u8]| half_to_f32(u16::from_le_bytes([b[0], b[1]]));
    let enc_s = |c: f32| { let c = c.clamp(0.0, 1.0); let v = if c < 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }; (v * 255.0 + 0.5) as u8 };
    for y in 0..h {
        for x in 0..w {
            let i = (y * row + x * 8) as usize;
            img.put_pixel(x, y, image::Rgb([enc_s(half(&data[i..])), enc_s(half(&data[i + 2..])), enc_s(half(&data[i + 4..]))]));
        }
    }
    img.save(out).unwrap();
    println!("saved {out}");
}

/// Player から HeroView を通して、全部の描き方（影・不透明・半透明・丸影・輪・型・影絵）を描く
fn run_full(g: &Gfx, out: &str, id: &str) {
    use gyogun_game::hero::hero::HeroOpts;
    use sim::battle::{PlayerState, create_battle};
    let (info, actor) = load(id);
    let info = &info;
    let mut view = HeroView::new(g);
    view.set_hero(g, info, actor, gyogun_game::hero::hero::default_weapon(id).unwrap_or("cake_sword"));
    let mut b = create_battle(1, false);
    let p = &mut b.player;
    (p.x, p.z, p.y) = (0.0, 0.0, 0.0);
    p.face = 0.5;
    let dt = 1.0 / 60.0;
    let opts = HeroOpts { flash: 0.0, aura: 0.6, show: true, ring: true, freeze_anim: false, grow: 1.0 };
    // 走る
    p.speed = 6.0;
    for f in 0..40 { view.update(g, p, dt as f64, (f as f32 * dt) as f64, opts); }
    // 技（通常の 1 段目）
    p.speed = 0.0;
    p.state = PlayerState::Attack;
    p.r#move = Some(&sim::moves::NORMALS[0]);
    p.move_key += 1;
    p.move_t = 0.0;
    let dur = p.r#move.unwrap().dur;
    for f in 0..((dur * 0.5 / dt as f64) as i32) { p.move_t += dt as f64; view.update(g, p, dt as f64, ((40 + f) as f32 * dt) as f64, opts); }
    let bl = view.blade();
    println!("blade tip {:?} base {:?} prev {:?} ok {} current {:?}", bl.tip, bl.base, bl.prev, bl.ok, view.actor().unwrap().current());
    let (w, h) = (900u32, 700u32);
    let eye = Vec3::new(1.5, 3.2, 6.0);
    let look = Vec3::new(0.0, 1.0, 0.0);
    let v = Mat4::look_at_rh(eye, look, Vec3::Y);
    let pr = Mat4::perspective_rh(40f32.to_radians(), w as f32 / h as f32, 0.3, 1200.0);
    let key = gfx::hex("#ffd2a8");
    let kd = Vec3::new(0.4, 1.0, 0.5).normalize();
    let light = Mat4::orthographic_rh(-8.0, 8.0, -8.0, 8.0, 1.0, 60.0) * Mat4::look_at_rh(kd * 30.0, Vec3::ZERO, Vec3::Y);
    let hemi = |s: &str| { let c = gfx::hex(s); [c[0], c[1], c[2], 1.0] };
    let fog = gfx::hex("#6a4a78");
    let globals = gfx::Globals {
        view: v.to_cols_array_2d(), proj: pr.to_cols_array_2d(), view_proj: (pr * v).to_cols_array_2d(), light_vp: light.to_cols_array_2d(),
        cam: [eye.x, eye.y, eye.z, 1.0], hemi_sky: hemi("#c9c2f2"), hemi_ground: hemi("#4a3a40"), key_color: [key[0] * 2.3, key[1] * 2.3, key[2] * 2.3, 1.0], key_dir: [kd.x, kd.y, kd.z, 0.0],
        fog: [fog[0], fog[1], fog[2], 0.0085], rim: [1.0, 1.0, 1.0, 0.0], misc: [1.0, h as f32, w as f32, 0.0],
    };
    g.queue.write_buffer(&g.globals_buf, 0, bytemuck::bytes_of(&globals));
    let mk = |fmt, samples, usage| g.device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: samples, dimension: wgpu::TextureDimension::D2, format: fmt, usage, view_formats: &[] });
    let msaa = mk(gfx::HDR, gfx::SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let depth = mk(gfx::DEPTH, gfx::SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let res = mk(gfx::HDR, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
    let (mv, dv, rv) = (msaa.create_view(&Default::default()), depth.create_view(&Default::default()), res.create_view(&Default::default()));
    let mask_t = mk(gfx::MASK, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
    let mask_d = mk(gfx::DEPTH, 1, wgpu::TextureUsages::RENDER_ATTACHMENT);
    let (mtv, mdv) = (mask_t.create_view(&Default::default()), mask_d.create_view(&Default::default()));
    let mut enc = g.device.create_command_encoder(&Default::default());
    {
        // 影
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None, color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &g.shadow_view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            ..Default::default()
        });
        pass.set_bind_group(0, &g.globals_bg, &[]);
        view.shadow(&mut pass);
    }
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &mtv, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store }, depth_slice: None })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &mdv, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            ..Default::default()
        });
        pass.set_bind_group(0, &g.globals_bg, &[]);
        view.draw_mask(&mut pass);
    }
    {
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &mv, resolve_target: Some(&rv), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.75, g: 0.7, b: 0.6, a: 1.0 }), store: wgpu::StoreOp::Store }, depth_slice: None })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &dv, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
            ..Default::default()
        });
        pass.set_bind_group(0, &g.globals_bg, &[]);
        view.draw(g, &mut pass, ORDER_OPAQUE);
        for o in [0, 1, 2] { view.draw(g, &mut pass, o); }
        view.draw_hidden(g, &mut pass);
    }
    let row = (w * 8 + 255) & !255;
    let buf = g.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    enc.copy_texture_to_buffer(res.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
    let mrow = (w + 255) & !255;
    let mbuf = g.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (mrow * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    enc.copy_texture_to_buffer(mask_t.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &mbuf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(mrow), rows_per_image: Some(h) } }, wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 });
    g.queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |r| r.unwrap());
    mbuf.slice(..).map_async(wgpu::MapMode::Read, |r| r.unwrap());
    g.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    let data = buf.slice(..).get_mapped_range().unwrap();
    let mdata = mbuf.slice(..).get_mapped_range().unwrap();
    let mut img = image::RgbImage::new(w * 2, h);
    let half = |b: &[u8]| half_to_f32(u16::from_le_bytes([b[0], b[1]]));
    let enc_s = |c: f32| { let c = c.clamp(0.0, 1.0); let v = if c < 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }; (v * 255.0 + 0.5) as u8 };
    for y in 0..h {
        for x in 0..w {
            let i = (y * row + x * 8) as usize;
            img.put_pixel(x, y, image::Rgb([enc_s(half(&data[i..])), enc_s(half(&data[i + 2..])), enc_s(half(&data[i + 4..]))]));
            let m = mdata[(y * mrow + x) as usize];
            img.put_pixel(w + x, y, image::Rgb([m, m, m]));
        }
    }
    img.save(out).unwrap();
    println!("saved {out}");
}

fn half_to_f32(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1f) as i32;
    let m = (h & 0x3ff) as f32;
    if e == 0 { s * m * 2f32.powi(-24) } else if e == 31 { s * f32::INFINITY } else { s * (1.0 + m / 1024.0) * 2f32.powi(e - 15) }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("png") => png(&args[2], &args[3], args[4].parse().unwrap(), args.get(5).map(|s| s.as_str())),
        _ => eprintln!("hero_check png <out.png> <id> <frame> [weapon]"),
    }
}
