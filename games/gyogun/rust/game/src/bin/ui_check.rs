// 画面の文字・枠の見た目を確かめる小さな書き出し（ウィンドウを開かず、各画面を PNG にする）。
// 使い方: cargo run -p gyogun-game --bin ui_check -- <出力先> [幅] [高さ] [倍率] [場面] [機器 key|pad|touch] [安全域 上,右,下,左]
use egui::{Context, RawInput, Rect, pos2, vec2};
use gyogun_game::gfx::Camera;
use gyogun_game::controls::{self, Device};
use gyogun_game::hud::{Hud, Tone};
use gyogun_game::ui::*;
use sim::battle::{EnemyKind, Battle, BossState, create_battle};

struct Off { device: wgpu::Device, queue: wgpu::Queue, tex: wgpu::Texture, view: wgpu::TextureView, renderer: egui_wgpu::Renderer, w: u32, h: u32, ppp: f32 }
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

impl Off {
    fn new(w: u32, h: u32, ppp: f32) -> Off {
        let (pw, ph) = ((w as f32 * ppp) as u32, (h as f32 * ppp) as u32);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
        let tex = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: pw, height: ph, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: FORMAT, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
        let view = tex.create_view(&Default::default());
        let renderer = egui_wgpu::Renderer::new(&device, FORMAT, egui_wgpu::RendererOptions::default());
        Off { device, queue, tex, view, renderer, w, h, ppp }
    }
    fn raw(&self, time: f64, pointer: Option<(f32, f32)>) -> RawInput {
        let mut r = RawInput { screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(self.w as f32, self.h as f32))), time: Some(time), predicted_dt: 1.0 / 60.0, max_texture_side: Some(MAX_ATLAS), ..Default::default() };
        r.viewports.get_mut(&r.viewport_id).unwrap().native_pixels_per_point = Some(self.ppp);
        if let Some((x, y)) = pointer { r.events.push(egui::Event::PointerMoved(pos2(x, y))); }
        r
    }
    /// 1 コマ描く。save なら PNG に残す
    fn frame(&mut self, ctx: &Context, time: f64, f: &mut dyn FnMut(&Context), save: Option<&str>) {
        ctx.begin_pass(self.raw(time, None));
        f(ctx);
        let mut out = ctx.end_pass();
        normalize_text_uv(ctx, &mut out.shapes);
        let desc = egui_wgpu::ScreenDescriptor { size_in_pixels: [self.tex.width(), self.tex.height()], pixels_per_point: out.pixels_per_point };
        let jobs = ctx.tessellate(out.shapes, out.pixels_per_point);
        let mut enc = self.device.create_command_encoder(&Default::default());
        {
            // 背景の代わりの色（戦場の夕空くらいの明るさ）
            enc.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &self.view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.42, g: 0.35, b: 0.48, a: 1.0 }), store: wgpu::StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
        }
        paint_jobs(&mut self.renderer, &self.device, &self.queue, &mut enc, &self.view, &jobs, out.textures_delta, &desc);
        let Some(path) = save else { self.queue.submit([enc.finish()]); return };
        let (pw, ph) = (self.tex.width(), self.tex.height());
        let bpr = (pw * 4).div_ceil(256) * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (bpr * ph) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        enc.copy_texture_to_buffer(self.tex.as_image_copy(), wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(bpr), rows_per_image: Some(ph) } }, wgpu::Extent3d { width: pw, height: ph, depth_or_array_layers: 1 });
        self.queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        self.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        let data = buf.slice(..).get_mapped_range().unwrap();
        let mut px = Vec::with_capacity((pw * ph * 4) as usize);
        for y in 0..ph as usize { px.extend_from_slice(&data[y * bpr as usize..y * bpr as usize + (pw * 4) as usize]); }
        for p in px.chunks_exact_mut(4) { p[3] = 255; }
        image::save_buffer(path, &px, pw, ph, image::ColorType::Rgba8).unwrap();
        println!("{path}");
    }
}

fn weapon(ctx: &Context, id: &str, name: &str, path: Option<PathCard>) -> WeaponCard {
    let p = format!("{}/../../public/aisns/models/wilds/weapons/{id}.png", option_env!("GYOGUN_GAME_DIR").unwrap_or(env!("CARGO_MANIFEST_DIR")));
    let image = std::fs::read(p).ok().and_then(|b| load_image(ctx, id, &b));
    WeaponCard { name: name.into(), image, path }
}

/// 戦いの見本（砦・隊長・大将のいる場面）
fn sample_battle(boss: bool) -> Battle {
    let mut b = create_battle(7, false);
    b.player.hp = b.player.max_hp * 0.64;
    b.player.ranbu = 100.0;
    b.player.kos = 327;
    b.player.combo = 88;
    b.holds[1].owner = sim::battle::Owner::Ally;
    b.debug_spawn(EnemyKind::Shortcake, 4.0, b.player.z - 8.0);
    let n = b.enemies.len();
    let c = n - 1;
    b.enemies[c].role = sim::battle::Role::Captain;
    b.enemies[c].awake = true;
    if boss { b.boss.state = BossState::Idle; b.boss.x = b.player.x; b.boss.z = b.player.z - 14.0; b.boss.hp = b.boss.max_hp * 0.7; }
    b
}
fn sample_camera(b: &Battle, w: f32, h: f32) -> Camera {
    let (x, z) = (b.player.x, b.player.z);
    let c = Camera::new(glam::DVec3::new(x, 6.3, z + 9.4), glam::DVec3::new(x, 1.3, z), 0.0, 52.0, (w / h) as f64, glam::DMat4::IDENTITY);
    Camera { prev_rot: c.rot(), ..c }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let dir = args.get(1).cloned().unwrap_or_else(|| ".".into());
    let w: u32 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1280);
    let h: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(800);
    let ppp: f32 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(2.0);
    let only = args.get(5).cloned().unwrap_or_default();
    let device = match args.get(6).map(String::as_str) { Some("touch") => Device::Touch, Some("pad") => Device::Pad, _ => Device::Key };
    let touch = device == Device::Touch;
    let safe = args.get(7).and_then(|s| Safe::parse(s)).unwrap_or_default();
    let mut off = Off::new(w, h, ppp);
    let ctx = Context::default();
    install_fonts(&ctx);
    setup_ctx(&ctx);
    let want = |n: &str| only.is_empty() || only == n;
    let mut time = 10.0_f64;
    let dt = 1.0 / 60.0_f64;
    let (fw, fh) = (w as f32, h as f32);

    // 字の準備のため 1 コマ空で回す
    off.frame(&ctx, time, &mut |_c| {}, None);
    let mut screens = Screens::new(&ctx);
    let mut hud = Hud::new();
    // 輪はキーボードとパッドのとき
    screens.set_device(device, !touch);
    screens.set_safe(safe);
    screens.touch_on(touch);
    hud.set_device(device);
    hud.set_safe(safe);
    screens.set_touch_view(TouchView { stick: None, knob: (18.0, -22.0), on: [false, false, true, false, false, false, false] });
    let save = |n: &str| format!("{dir}/{n}.png");

    // 武将の札（絵の代わりに武器の PNG）
    let heroes: Vec<HeroCard> = ["cake_sword", "ebi_lance", "candy_axe", "tamago_pan", "saba_sword", "daikon_club", "ika_spear"].iter().enumerate().map(|(i, id)| HeroCard { name: format!("武将{i}"), select: weapon(&ctx, id, id, None).image }).collect();
    let thumb = heroes[0].select.clone();
    screens.set_heroes(heroes);

    let run = |off: &mut Off, time: &mut f64, secs: f64, name: Option<String>, f: &mut dyn FnMut(&Context)| {
        let n = (secs / dt).ceil() as usize;
        for i in 0..n.max(1) {
            *time += dt;
            let last = i + 1 == n.max(1);
            off.frame(&ctx, *time, f, if last { name.as_deref() } else { None });
        }
    };

    // ---- 読み込み ----
    if want("loading") {
        screens.progress(0.55, Some("武将の名簿を読んでいます…"));
        run(&mut off, &mut time, 1.2, Some(save("loading")), &mut |c| { screens.ui(c); });
        // 主役の一覧が読めなかったとき
        screens.load_failed(Some("キャラクターの配布に接続できません。通信を確かめて、読み込み直してください。"));
        run(&mut off, &mut time, 0.6, Some(save("loading_failed")), &mut |c| { screens.ui(c); });
        screens.load_failed(None);
    }
    // ---- タイトル ----
    if want("title") {
        screens.show(Some(Screen::Title));
        screens.set_title_best(format!("最高 S ／ {} 点 ／ 1204 人斬り ／ 誉れ 7 / 35", fmt_num(1284500)));
        run(&mut off, &mut time, 3.6, Some(save("title")), &mut |c| { screens.ui(c); });
    }
    // ---- 武将選び ----
    if want("select") {
        screens.show(Some(Screen::Select));
        screens.pick_hero(2);
        screens.set_sel_name("あいあれんじゃー");
        screens.set_weapon(weapon(&ctx, "ebi_lance", "エビフライの槍", Some(PathCard { kind: PathKind::Straight, name: "直進".into(), desc: "そろって正面へ、細く長く貫く".into() })));
        run(&mut off, &mut time, 1.0, Some(save("select")), &mut |c| { screens.ui(c); });
        screens.set_weapon(weapon(&ctx, "tamago_pan", "目玉焼きのフライパン", Some(PathCard { kind: PathKind::Spread, name: "拡散".into(), desc: "扇に広がって、広く巻き込む".into() })));
        run(&mut off, &mut time, 0.1, Some(save("select_spread")), &mut |c| { screens.ui(c); });
        screens.set_weapon(weapon(&ctx, "softserve_staff", "ソフトクリームの杖", Some(PathCard { kind: PathKind::Curve, name: "曲進".into(), desc: "弧を描いて、横から回り込む".into() })));
        run(&mut off, &mut time, 0.1, Some(save("select_curve")), &mut |c| { screens.ui(c); });
        screens.set_weapon(weapon(&ctx, "cake_sword", "ケーキの剣", None));
        run(&mut off, &mut time, 0.1, Some(save("select_plain")), &mut |c| { screens.ui(c); });
    }
    screens.show(None);
    run(&mut off, &mut time, 0.7, None, &mut |c| { screens.ui(c); });

    // ---- 戦い ----
    if want("hud") {
        run(&mut off, &mut time, 0.02, None, &mut |c| { screens.ui(c); });
        let b = sample_battle(false);
        let cam = sample_camera(&b, fw, fh);
        hud.show(true);
        hud.set_hero("あいあれんじゃー", thumb.clone());
        hud.set_rank(2);
        hud.set_load(7);
        run(&mut off, &mut time, 0.7, None, &mut |c| { hud.ui(c, &b, dt as f64, &cam, 0.4, true); });
        hud.chain(3, 24);
        hud.chain_pop(b.player.x + 3.0, 1.0, b.player.z - 2.0, 20);
        hud.chain_pop(b.player.x - 3.0, 1.0, b.player.z - 1.0, 35);
        hud.chain_reward(b.player.x - 1.0, b.player.z - 4.0, 18, 3200);
        hud.say("砦 制圧", "苺の砦 を落とした ── 残り 3", Tone::Gold);
        hud.set_hint(&controls::carry_hint(device));
        run(&mut off, &mut time, 1.0, Some(save("hud")), &mut |c| { hud.ui(c, &b, dt as f64, &cam, 0.4, true); screens.ui(c); });
    }
    // ---- 切り込み ----
    if want("cutin") {
        let b = sample_battle(false);
        let cam = sample_camera(&b, fw, fh);
        screens.set_cutin_hero(thumb.clone(), "あいあれんじゃー");
        run(&mut off, &mut time, 0.02, None, &mut |c| { screens.ui(c); });
        screens.cutin_start();
        screens.letterbox(true);
        run(&mut off, &mut time, 0.5, Some(save("cutin")), &mut |c| { hud.ui(c, &b, dt as f64, &cam, 0.0, true); screens.ui(c); });
        screens.cutin_stop();
        screens.letterbox(false);
    }
    hud.show(false);
    // ---- 一時停止 ----
    if want("pause") {
        screens.show(Some(Screen::Pause));
        run(&mut off, &mut time, 0.8, Some(save("pause")), &mut |c| { screens.ui(c); });
    }
    // ---- 敗北 ----
    if want("gameover") {
        screens.show(Some(Screen::Gameover));
        run(&mut off, &mut time, 0.8, Some(save("gameover")), &mut |c| { screens.ui(c); });
    }
    // ---- 結果 ----
    if want("result") {
        let row = |a: &str, b: &str, c: &str| ResultRow { label: a.into(), value: b.into(), best: c.into() };
        let honor = |n: &str, on: bool, fresh: bool| Honor { name: n.into(), on, fresh };
        screens.set_result(ResultData {
            eyebrow: "VICTORY".into(), title: "勝鬨".into(), rank: "S".into(),
            rows: vec![row("撃破", "1204", "NEW"), row("最大連撃", "312", "最高 340"), row("最大連鎖", "88", "NEW"), row("連鎖の得点", &fmt_num(84200), ""), row("時間", "6:42", ""), row("砦", "4 / 4", ""), row("格", "参", ""), row("受けた傷", "1320", ""), row("得点", &fmt_num(1284500), "NEW")],
            honors: vec![honor("千人斬り", true, true), honor("五分で決着", false, false), honor("傷は浅い（1500以下）", true, false), honor("三百連撃", true, true), honor("百連鎖", false, false)],
            main: Some(9),
        });
        screens.show(Some(Screen::Result));
        run(&mut off, &mut time, 1.6, Some(save("result")), &mut |c| { screens.ui(c); });
        // 自己ベストの祝い（流れは sim::cheer。ここでは見た目だけ）: 数え上げの途中 → 数え終わり（紙吹雪の盛り）→ S のもう 1 回
        screens.cheer_begin(false);
        screens.cheer_count(612_300);
        run(&mut off, &mut time, 0.1, Some(save("result_count")), &mut |c| { screens.ui(c); });
        screens.cheer_count(1_284_500);
        screens.cheer_done(true);
        run(&mut off, &mut time, 0.35, Some(save("result_cheer")), &mut |c| { screens.ui(c); });
        screens.cheer_encore();
        run(&mut off, &mut time, 0.45, Some(save("result_top")), &mut |c| { screens.ui(c); });
        run(&mut off, &mut time, 3.0, Some(save("result_after")), &mut |c| { screens.ui(c); });
    }
}
