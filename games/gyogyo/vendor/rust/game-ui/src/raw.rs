// winit を使わないゲーム（island-kit の島のゲームなど）のための入口。
// 入力の出来事を論理 px で受けて egui の入力に直し（PointerInput）、egui を自分のコマンドで描く（RawLayer）。
// ブラウザの WebState もこの PointerInput を使うので、指・マウスの扱いは全ゲームで同じになる。
use egui::{pos2, vec2, Context, Event, PointerButton, RawInput, Rect, ViewportId};

/// 入力の出来事を egui の RawInput に積む。座標はすべて論理 px（CSS px）
pub struct PointerInput {
    raw: RawInput,
    start: web_time::Instant,
    max_side: usize,
    pos: egui::Pos2,
    touch: Option<u64>,
}

impl PointerInput {
    pub fn new(max_side: usize) -> Self { PointerInput { raw: RawInput::default(), start: web_time::Instant::now(), max_side, pos: pos2(-1.0, -1.0), touch: None } }
    pub fn moved(&mut self, x: f32, y: f32) { self.pos = pos2(x, y); self.raw.events.push(Event::PointerMoved(self.pos)); }
    pub fn left(&mut self) { self.raw.events.push(Event::PointerGone); }
    pub fn button(&mut self, button: PointerButton, pressed: bool) { self.raw.events.push(Event::PointerButton { pos: self.pos, button, pressed, modifiers: Default::default() }); }
    pub fn wheel_lines(&mut self, dx: f32, dy: f32) { self.raw.events.push(Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta: vec2(dx, dy), phase: egui::TouchPhase::Move, modifiers: Default::default() }); }
    pub fn wheel_points(&mut self, dx: f32, dy: f32) { self.raw.events.push(Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(dx, dy), phase: egui::TouchPhase::Move, modifiers: Default::default() }); }
    /// 指は 1 本をポインタとして渡す（札・ボタンを押す）。2 本目以降は受けない
    pub fn touch_start(&mut self, id: u64, x: f32, y: f32) {
        if self.touch.is_some() { return; }
        let p = pos2(x, y);
        self.touch = Some(id); self.pos = p;
        self.raw.events.push(Event::PointerMoved(p));
        self.raw.events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Default::default() });
    }
    pub fn touch_move(&mut self, id: u64, x: f32, y: f32) {
        if self.touch != Some(id) { return; }
        self.pos = pos2(x, y); self.raw.events.push(Event::PointerMoved(self.pos));
    }
    /// 離した・取り消された。離したコマに PointerGone も届くので、押した物は `interact_pointer_pos` で決める
    pub fn touch_end(&mut self, id: u64, x: f32, y: f32) {
        if self.touch != Some(id) { return; }
        self.touch = None;
        let p = pos2(x, y);
        self.raw.events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Default::default() });
        self.raw.events.push(Event::PointerGone);
    }
    pub fn focused(&mut self, f: bool) { self.raw.events.push(Event::WindowFocused(f)); }
    /// 1 コマ分の入力を取り出す。w・h は画面の論理 px、ppp は画素比（devicePixelRatio）
    pub fn take(&mut self, w: f32, h: f32, ppp: f32) -> RawInput {
        let mut raw = std::mem::take(&mut self.raw);
        raw.screen_rect = Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h)));
        raw.time = Some(self.start.elapsed().as_secs_f64());
        raw.max_texture_side = Some(self.max_side);
        raw.focused = true;
        raw.viewport_id = ViewportId::ROOT;
        raw.viewports.entry(ViewportId::ROOT).or_default().native_pixels_per_point = Some(ppp);
        raw
    }
}

/// winit を使わない egui の層。字体を入れた Context と描く道具を持つ
pub struct RawLayer {
    ctx: Context,
    renderer: egui_wgpu::Renderer,
    /// 確かめ用: 文字を描かない
    pub hide: bool,
    /// finish で作り、paint_finished で描く物
    jobs: Vec<egui::ClippedPrimitive>,
    ppp: f32,
    /// まだ GPU へ送っていない字の絵・画像の更新（描けなかったコマの分も持ち越す）
    pending: egui::TexturesDelta,
}

impl RawLayer {
    /// `install` で字体を入れる（`install_fonts_from`）。format は描く先のサーフェスの形式（island-kit は `Kit::surface_format`）
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, install: impl FnOnce(&Context)) -> Self {
        let ctx = Context::default();
        install(&ctx);
        crate::setup_ctx(&ctx);
        RawLayer { ctx, renderer: egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions::default()), hide: false, jobs: vec![], ppp: 1.0, pending: egui::TexturesDelta::default() }
    }
    pub fn ctx(&self) -> &Context { &self.ctx }
    pub fn max_side(device: &wgpu::Device) -> usize { crate::MAX_ATLAS.min(device.limits().max_texture_dimension_2d as usize) }
    /// 1 コマの始まり。`PointerInput::take` の入力を渡す
    pub fn begin(&mut self, raw: RawInput) -> Context { self.ctx.begin_pass(raw); self.ctx.clone() }
    /// 描く（island-kit は `Kit::render_then` の中で呼ぶ）。finish と paint_finished を続けて呼ぶのと同じ。
    /// 返す PlatformOutput の cursor_icon でカーソルの形を変えられる
    pub fn paint(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, size: [u32; 2]) -> egui::PlatformOutput {
        let out = self.finish();
        self.paint_finished(device, queue, encoder, view, size);
        out
    }
    /// コマを締める（描く物を作る）。毎コマ必ず 1 回呼ぶ。描く処理が呼ばれないコマ（island-kit で画面を取れなかったコマなど）が
    /// あるゲームは、render_then の外でこれを呼び、中で paint_finished を呼ぶ。描けなかったコマの字の絵の更新は次に描くコマへ持ち越す
    pub fn finish(&mut self) -> egui::PlatformOutput {
        let mut out = self.ctx.end_pass();
        if self.hide { out.shapes.clear(); }
        crate::normalize_text_uv(&self.ctx, &mut out.shapes);
        self.ppp = out.pixels_per_point;
        self.jobs = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        self.pending.append(out.textures_delta);
        out.platform_output
    }
    /// finish で作った物を描く。持ち越した字の絵の更新もここで送る
    pub fn paint_finished(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, size: [u32; 2]) {
        let desc = egui_wgpu::ScreenDescriptor { size_in_pixels: size, pixels_per_point: self.ppp };
        let delta = std::mem::take(&mut self.pending);
        crate::paint_jobs(&mut self.renderer, device, queue, encoder, view, &self.jobs, delta, &desc);
    }
}
