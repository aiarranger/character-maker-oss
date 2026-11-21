// ゲームの画面の文字・枠を egui で描く共通部品（2026-11-09、百菓繚乱 muso の rust/game/src/ui.rs から切り出し）。
// CSS の transform（回転・傾き・拡大）と文字のグラデーションをそのまま写すため、egui の部品は使わず、すべて Pen（下）で頂点を組む。
// 当たり判定だけ egui の Area / interact に任せる。字体の表（役 → ファイル）はゲームが渡す。
// ブラウザ（wasm）とネイティブ（Mac・将来の iPhone）で同じ描き方になる。
use egui::epaint::{Mesh, PathShape, Vertex};
use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Galley, Order, Painter, Pos2, Rect, Shape, Stroke, TextureHandle, TextureId, pos2, vec2};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use winit::event::WindowEvent;
use winit::window::Window;

#[cfg(target_arch = "wasm32")]
pub mod web;
/// winit を使わないゲーム（island-kit など）の入口: PointerInput・RawLayer
pub mod raw;
/// 指のスティックとボタン（ゲームにも DOM にも依らない）
pub mod touch_pad;
/// ブラウザの canvas の出来事を、画面のボタンと遊びの指へ分ける
#[cfg(target_arch = "wasm32")]
pub mod dom_input;
/// ブラウザのページから取る物（ファイルをまとめて取る・端の安全域）
#[cfg(target_arch = "wasm32")]
pub mod page;
/// うれしい瞬間（結果の画面の数え上げと祝いの流れ・紙吹雪）
pub mod cheer;
/// 最後に触った機器と輪・項目を移る・結果の待ち・確かめ用の安全域
pub mod menu;

// ---- egui の組み込み（winit の出来事 → egui → wgpu） ----

pub struct UiLayer {
    ctx: Context,
    #[cfg(not(target_arch = "wasm32"))]
    state: egui_winit::State,
    #[cfg(target_arch = "wasm32")]
    state: web::WebState,
    renderer: egui_wgpu::Renderer,
    /// カーソルの場所（論理 px）。押した瞬間に UI の上か調べる
    cursor: Pos2,
    /// 確かめ用: 文字を描かない（3D だけを比べる）
    pub hide: bool,
}

/// 字のテクスチャの幅（egui は既定で GPU の上限まで広げ、高さを足すたびに全体を送り直す）
pub const MAX_ATLAS: usize = 4096;

/// その場所が UI の押せる面の上か。egui の背景の層は画面全体を覆うので数えない
/// （begin_pass を使うと egui_wants_pointer_input は常に true になる）
pub fn over_ui(ctx: &Context, p: Pos2) -> bool { ctx.layer_id_at(p).is_some_and(|l| l.order != Order::Background) }

/// 押せる面を over_ui に知らせる（Pen で描くだけの札・ボタンは egui の面が無く、指が遊びの方へ行く）。毎コマ、押せる面ごとに呼ぶ。
/// order は重なり（休む画面の札なら Foreground）。press で押す物と同じ面に重ねるなら、その面より下の order にする（2026-11-10 rainbow の blocks）
pub fn block(ctx: &Context, id: egui::Id, r: Rect, order: Order) {
    layer(id, order).fixed_pos(r.min).interactable(true).show(ctx, |ui| { ui.allocate_exact_size(r.size(), egui::Sense::hover()); });
}

/// ゲームの画面の面（egui の Area）。出始めを薄くしない（egui の既定の 0.08 秒のフェードは CSS のフェードと二重になる）・画面の外へ押し戻さない
pub fn layer(id: egui::Id, order: Order) -> egui::Area { egui::Area::new(id).order(order).constrain(false).fade_in(false) }

/// press の結果
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Press { pub hover: bool, pub down: bool, pub hit: bool }

/// 押せる物（DOM の click と同じ決まり）。egui の clicked は 6px 動くとドラッグ・0.8 秒より長いと押下にならないので、
/// 押した所から離した所まで 12px 未満で、離した所がまだ箱の上なら押した（長押しも押した）。指では離したコマに hover が無いので
/// 押した位置（interact_pointer_pos）で決める。前のコマで押していた時だけ数える（2026-11-10 digdag の press。muso の武将の帯と同じ決まり）
pub fn press(ui: &mut egui::Ui, id: egui::Id, r: Rect) -> Press {
    let resp = ui.interact(r, id, egui::Sense::click_and_drag());
    let ctx = ui.ctx().clone();
    if resp.hovered() { ctx.set_cursor_icon(egui::CursorIcon::PointingHand); }
    let down = resp.is_pointer_button_down_on();
    let pass = ctx.cumulative_pass_nr();
    let (kd, kp, kl) = (id.with("down"), id.with("press"), id.with("last"));
    let was = ctx.data_mut(|d| d.get_temp::<u64>(kd)).is_some_and(|n| n + 1 == pass);
    let mut hit = resp.clicked();
    if down {
        if let Some(p) = resp.interact_pointer_pos() {
            if !was { ctx.data_mut(|d| d.insert_temp(kp, p)); }
            ctx.data_mut(|d| d.insert_temp(kl, p));
        }
        ctx.data_mut(|d| d.insert_temp(kd, pass));
    } else {
        if was {
            let a: Option<Pos2> = ctx.data_mut(|d| d.get_temp(kp));
            let b: Option<Pos2> = ctx.data_mut(|d| d.get_temp(kl));
            if let (Some(a), Some(b)) = (a, b) { if a.distance(b) < 12.0 && r.contains(b) { hit = true; } }
        }
        ctx.data_mut(|d| d.remove_temp::<u64>(kd));
    }
    Press { hover: resp.hovered(), down, hit }
}

fn erfc(x: f32) -> f32 {
    // Abramowitz-Stegun 7.1.26
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.3275911 * z);
    let y = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429)))) * (-z * z).exp();
    if x >= 0.0 { y } else { 2.0 - y }
}

impl UiLayer {
    /// `install` で字体を入れる（`install_fonts`）。format は描く先のサーフェスの形式
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, window: &Arc<Window>, install: impl FnOnce(&Context)) -> Self {
        let ctx = Context::default();
        install(&ctx);
        setup_ctx(&ctx);
        let max_side = MAX_ATLAS.min(device.limits().max_texture_dimension_2d as usize);
        #[cfg(not(target_arch = "wasm32"))]
        let state = egui_winit::State::new(ctx.clone(), egui::ViewportId::ROOT, window, Some(window.scale_factor() as f32), None, Some(max_side));
        #[cfg(target_arch = "wasm32")]
        let state = web::WebState::new(window, max_side);
        let renderer = egui_wgpu::Renderer::new(device, format, egui_wgpu::RendererOptions::default());
        UiLayer { ctx, state, renderer, cursor: pos2(-1.0, -1.0), hide: false }
    }
    pub fn ctx(&self) -> &Context { &self.ctx }
    /// 窓の出来事を渡す。true なら UI が受けた（ゲームへ渡さない）。
    /// ボタンや表示中の画面の上で「押した」ときと、ホイールだけ。離す・動かすはゲームにも届ける（ドラッグの終わりを取りこぼさない）
    pub fn on_event(&mut self, window: &Window, e: &WindowEvent) -> bool {
        let resp = self.state.on_window_event(window, e);
        let ppp = self.ctx.pixels_per_point();
        match e {
            WindowEvent::CursorMoved { position, .. } => { self.cursor = pos2(position.x as f32 / ppp, position.y as f32 / ppp); false }
            WindowEvent::MouseInput { state, .. } => state.is_pressed() && over_ui(&self.ctx, self.cursor),
            WindowEvent::MouseWheel { .. } => over_ui(&self.ctx, self.cursor),
            WindowEvent::Touch(t) => t.phase == winit::event::TouchPhase::Started && over_ui(&self.ctx, pos2(t.location.x as f32 / ppp, t.location.y as f32 / ppp)),
            _ => resp.consumed,
        }
    }
    /// 1 コマの始まり。返した Context で画面を描き、最後に paint
    pub fn begin(&mut self, window: &Window) -> Context {
        let raw = self.state.take_egui_input(window);
        self.ctx.begin_pass(raw);
        self.ctx.clone()
    }
    /// 画面（後処理の後のサーフェス）へ重ねて描く。size はサーフェスの画素数
    pub fn paint(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: [u32; 2], encoder: &mut wgpu::CommandEncoder, screen_view: &wgpu::TextureView, window: &Window) {
        let mut out = self.ctx.end_pass();
        self.state.handle_platform_output(window, out.platform_output);
        if self.hide { out.shapes.clear(); }
        normalize_text_uv(&self.ctx, &mut out.shapes);
        let desc = egui_wgpu::ScreenDescriptor { size_in_pixels: size, pixels_per_point: out.pixels_per_point };
        let jobs = self.ctx.tessellate(out.shapes, out.pixels_per_point);
        paint_jobs(&mut self.renderer, device, queue, encoder, screen_view, &jobs, out.textures_delta, &desc);
    }
}

/// Pen の文字は UV を字のテクスチャの画素のまま持つ（コマの途中で字のテクスチャが伸びても崩れないよう、egui の文字と同じくコマの終わりに割る）
pub fn normalize_text_uv(ctx: &Context, shapes: &mut [egui::epaint::ClippedShape]) {
    let size = ctx.fonts(|f| f.font_image_size());
    let k = vec2(1.0 / size[0] as f32, 1.0 / size[1] as f32);
    for s in shapes {
        if let Shape::Mesh(m) = &mut s.shape {
            if m.texture_id != TextureId::default() { continue; }
            let Some(m) = Arc::get_mut(m) else { continue };
            // 白の画素（UV 0,0）だけのメッシュは色の面
            for v in &mut m.vertices { if v.uv.x > 0.0 || v.uv.y > 0.0 { v.uv = pos2(v.uv.x * k.x, v.uv.y * k.y); } }
        }
    }
}

/// egui の出力を的へ描く（確かめ用の画像書き出しと共通）
pub fn paint_jobs(renderer: &mut egui_wgpu::Renderer, device: &wgpu::Device, queue: &wgpu::Queue, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, jobs: &[egui::ClippedPrimitive], mut delta: egui::TexturesDelta, desc: &egui_wgpu::ScreenDescriptor) {
    for (id, list) in delta.set.drain() { for d in &list { renderer.update_texture(device, queue, id, d); } }
    renderer.update_buffers(device, queue, encoder, jobs, desc);
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ui"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store } })],
            depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None,
        }).forget_lifetime();
        renderer.render(&mut pass, jobs, desc);
    }
    for id in delta.free.drain() { renderer.free_texture(&id); }
}

/// ゲームの UI に要らない egui の働き（キーでの拡大など）を止める
pub fn setup_ctx(ctx: &Context) {
    ctx.options_mut(|o| { o.zoom_with_keyboard = false; });
    ctx.global_style_mut(|s| { s.interaction.selectable_labels = false; });
}

// ---- 字体 ----

/// 字体の役（CSS の --brush / --mincho / --jp / --num と太さ）。どのファイルを当てるかはゲームの表（FontRow）が決める
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fam { Brush, Mincho600, Mincho800, Jp500, Jp700, Jp900, Num,
    /// ゲームが名付ける役（例: `Fam::Custom("fredoka600")`）。上の 7 つで足りない字体に使う
    Custom(&'static str) }
impl Fam {
    fn key(self) -> &'static str { match self { Fam::Brush => "brush", Fam::Mincho600 => "mincho600", Fam::Mincho800 => "mincho800", Fam::Jp500 => "jp500", Fam::Jp700 => "jp700", Fam::Jp900 => "jp900", Fam::Num => "num", Fam::Custom(k) => k } }
    /// egui の字体の名前（FontId を自分で組むとき）
    pub fn family(self) -> FontFamily { FontFamily::Name(self.key().into()) }
}

/// 役ごとの字体: (役, 同梱する Web フォントのファイル名, 無いときの OS の字体のファイル名, その ttc の番号)
pub type FontRow = (Fam, &'static str, &'static str, u32);

pub const OS_FONTS: &str = "/System/Library/Fonts/";

/// 同梱する字体のファイル（ブラウザでは先に取っておく。ページからの相対）
pub fn font_files_of(table: &[FontRow]) -> Vec<String> { table.iter().map(|f| format!("fonts/{}", f.1)).collect() }

/// 字体を読む。同じファイルは 1 回だけ読む（明朝の .ttc は 18MB ある）。
/// `assets` は同梱の字体の置き場（ブラウザは "fonts"）、`read` はファイルを読む関数（ブラウザでは先に取っておいた物から返す）
pub fn install_fonts_from(ctx: &Context, table: &[FontRow], assets: &Path, read: &dyn Fn(&Path) -> Option<Vec<u8>>) {
    install_fonts_with(ctx, table, &|_| vec![], assets, read)
}

/// install_fonts_from と同じ。無い字を探す順を役ごとに先に決められる（CSS の font-family の 2 番目以降。
/// 例: 和文の無い Fredoka の役は Zen Maru Gothic の近い太さを先に探す）。`prefer` に挙げなかった役は表の順 → egui の既定
pub fn install_fonts_with(ctx: &Context, table: &[FontRow], prefer: &dyn Fn(Fam) -> Vec<Fam>, assets: &Path, read: &dyn Fn(&Path) -> Option<Vec<u8>>) {
    let mut fonts = FontDefinitions::default();
    let mut got: BTreeMap<PathBuf, &'static [u8]> = BTreeMap::new();
    let mut load = |p: PathBuf| -> Option<&'static [u8]> {
        if let Some(b) = got.get(&p) { return Some(*b); }
        let data = read(&p)?;
        let b: &'static [u8] = Box::leak(data.into_boxed_slice());
        got.insert(p, b);
        Some(b)
    };
    let mut names: Vec<(Fam, String)> = vec![];
    for (fam, asset, os, index) in table.iter().copied() {
        // 候補: 同じ Web フォント → OS の字体
        let picked = load(assets.join(asset)).map(|b| (b, 0, format!("web:{asset}"))).or_else(|| load(PathBuf::from(OS_FONTS).join(os)).map(|b| (b, index, format!("os:{os}:{index}"))));
        let Some((bytes, index, key)) = picked else { continue };
        fonts.font_data.entry(key.clone()).or_insert_with(|| Arc::new(FontData { font: std::borrow::Cow::Borrowed(bytes), index, tweak: Default::default() }));
        names.push((fam, key));
    }
    // 無い字（記号など）は、ほかの字体 → egui の既定で出す
    let all: Vec<String> = names.iter().map(|(_, k)| k.clone()).collect();
    let defaults: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for (fam, _, _, _) in table.iter().copied() {
        let own = names.iter().find(|(f, _)| *f == fam).map(|(_, k)| k.clone());
        let mut chain: Vec<String> = own.into_iter().collect();
        for f in prefer(fam) { if let Some((_, k)) = names.iter().find(|(g, _)| *g == f) { if !chain.contains(k) { chain.push(k.clone()); } } }
        for k in all.iter().chain(defaults.iter()) { if !chain.contains(k) { chain.push(k.clone()); } }
        fonts.families.insert(fam.family(), chain);
    }
    ctx.set_fonts(fonts);
}


// ---- 色・式 ----

/// CSS の色（rgb は 0..255、a は 0..1）
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct C { pub r: f32, pub g: f32, pub b: f32, pub a: f32 }
pub const fn rgba(r: u8, g: u8, b: u8, a: f32) -> C { C { r: r as f32, g: g as f32, b: b as f32, a } }
pub const fn hx(v: u32) -> C { rgba((v >> 16) as u8, (v >> 8) as u8, v as u8, 1.0) }
pub const CLEAR: C = rgba(0, 0, 0, 0.0);
pub const INK2: C = hx(0x1e1430);
pub const CREAM: C = hx(0xfff3e0);
pub const CREAM_DIM: C = rgba(255, 243, 224, 0.7);
pub const GOLD: C = hx(0xffc94a);
pub const RED: C = hx(0xff3a5a);
pub const LINE: C = rgba(255, 230, 190, 0.22);
pub const WHITE: C = hx(0xffffff);
/// 「text-shadow: 0 2px 8px rgba(0,0,0,.9)」をよく使う
pub const SHADOW8: TS = TS(0.0, 2.0, 8.0, rgba(0, 0, 0, 0.9));
/// `line-height: normal` の代わり（Zen Kaku Gothic New などの行の高さ）
pub const LH: f32 = 1.45;
/// 行の箱の中心から基線までの比。CSS は (ascent - descent) / 2 の所に置く。7 つの字体はどれも hhea が 1.16 / 0.288
pub const BASE_K: f32 = 0.436;

impl C {
    pub fn alpha(self, k: f32) -> C { C { a: self.a * k, ..self } }
    /// filter: brightness(k)
    pub fn bright(self, k: f32) -> C { C { r: (self.r * k).min(255.0), g: (self.g * k).min(255.0), b: (self.b * k).min(255.0), a: self.a } }
    pub fn lerp(self, o: C, k: f32) -> C {
        // CSS と同じく、あらかじめ掛けた色で混ぜる
        let a = self.a + (o.a - self.a) * k;
        if a <= 0.0 { return C { a: 0.0, ..o }; }
        let m = |x: f32, y: f32| (x * self.a + (y * o.a - x * self.a) * k) / a;
        C { r: m(self.r, o.r), g: m(self.g, o.g), b: m(self.b, o.b), a }
    }
    fn c32(self, alpha: f32) -> Color32 {
        let a = (self.a * alpha).clamp(0.0, 1.0);
        Color32::from_rgba_unmultiplied(self.r.clamp(0.0, 255.0) as u8, self.g.clamp(0.0, 255.0) as u8, self.b.clamp(0.0, 255.0) as u8, (a * 255.0).round() as u8)
    }
}

pub fn stops_at(stops: &[(f32, C)], t: f32) -> C {
    if t <= stops[0].0 { return stops[0].1; }
    for w in stops.windows(2) {
        if t <= w[1].0 { return w[0].1.lerp(w[1].1, (t - w[0].0) / (w[1].0 - w[0].0).max(1e-5)); }
    }
    stops[stops.len() - 1].1
}

pub fn lerp(a: f32, b: f32, k: f32) -> f32 { a + (b - a) * k }
pub fn clamp01(x: f32) -> f32 { x.clamp(0.0, 1.0) }
/// CSS の clamp(a, b, c)
pub fn cl(a: f32, b: f32, c: f32) -> f32 { b.clamp(a, c) }

/// cubic-bezier(x1, y1, x2, y2)
pub fn bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    if t <= 0.0 { return 0.0; }
    if t >= 1.0 { return 1.0; }
    let (mut lo, mut hi) = (0.0_f32, 1.0_f32);
    let f = |u: f32, a: f32, b: f32| { let m = 1.0 - u; 3.0 * m * m * u * a + 3.0 * m * u * u * b + u * u * u };
    for _ in 0..24 { let m = (lo + hi) * 0.5; if f(m, x1, x2) < t { lo = m; } else { hi = m; } }
    f((lo + hi) * 0.5, y1, y2)
}
/// --ease
pub fn ease(t: f32) -> f32 { bezier(0.22, 1.0, 0.36, 1.0, t) }
/// --snap
pub fn snap(t: f32) -> f32 { bezier(0.34, 1.7, 0.5, 1.0, t) }
/// CSS の ease（transition の既定）
pub fn css_ease(t: f32) -> f32 { bezier(0.25, 0.1, 0.25, 1.0, t) }
/// CSS の ease-in-out
pub fn ease_io(t: f32) -> f32 { bezier(0.42, 0.0, 0.58, 1.0, t) }

/// 区切りごとに timing をかける CSS の @keyframes（1 つの値）
pub fn kf(t: f32, keys: &[(f32, f32)], f: fn(f32) -> f32) -> f32 {
    if t <= keys[0].0 { return keys[0].1; }
    for w in keys.windows(2) {
        if t <= w[1].0 { return lerp(w[0].1, w[1].1, f((t - w[0].0) / (w[1].0 - w[0].0).max(1e-5))); }
    }
    keys[keys.len() - 1].1
}

/// CSS の transition（値が変わると、いまの値から動き直す）
#[derive(Clone, Copy, Debug)]
pub struct Tween { from: f32, to: f32, start: f64, dur: f32, delay: f32, f: fn(f32) -> f32 }
impl Tween {
    pub fn new(v: f32, dur: f32, delay: f32, f: fn(f32) -> f32) -> Self { Tween { from: v, to: v, start: -1e9, dur, delay, f } }
    pub fn set(&mut self, to: f32, now: f64) {
        if to == self.to { return; }
        self.from = self.get(now); self.to = to; self.start = now;
    }
    pub fn jump(&mut self, v: f32) { self.from = v; self.to = v; self.start = -1e9; }
    pub fn get(&self, now: f64) -> f32 {
        let t = ((now - self.start) as f32 - self.delay) / self.dur.max(1e-5);
        if t <= 0.0 { return self.from; }
        if t >= 1.0 { return self.to; }
        lerp(self.from, self.to, (self.f)(t))
    }
    pub fn target(&self) -> f32 { self.to }
}

/// 1 回きりの CSS アニメーション（始めた時刻を持つ）
#[derive(Clone, Copy, Debug, Default)]
pub struct Shot(pub Option<f64>);
impl Shot {
    pub fn fire(&mut self, now: f64) { self.0 = Some(now); }
    /// 0..1（動いていなければ 1）
    pub fn k(&self, now: f64, dur: f32) -> f32 { match self.0 { Some(s) => clamp01(((now - s) as f32) / dur), None => 1.0 } }
}

/// 3 桁ごとのカンマ（toLocaleString）
pub fn fmt_num(n: i64) -> String {
    let s = n.abs().to_string();
    let mut o = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 { o.push(','); }
        o.push(ch);
    }
    if n < 0 { format!("-{o}") } else { o }
}

// ---- 行列 ----

/// 2 次元の CSS transform
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Xf { pub a: f32, pub b: f32, pub c: f32, pub d: f32, pub e: f32, pub f: f32 }
impl Xf {
    pub const ID: Xf = Xf { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };
    pub fn tr(x: f32, y: f32) -> Xf { Xf { e: x, f: y, ..Xf::ID } }
    pub fn scale(x: f32, y: f32) -> Xf { Xf { a: x, d: y, ..Xf::ID } }
    pub fn rot(deg: f32) -> Xf { let (s, c) = deg.to_radians().sin_cos(); Xf { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 } }
    pub fn skew_x(deg: f32) -> Xf { Xf { c: deg.to_radians().tan(), ..Xf::ID } }
    /// CSS の順（self が外側、o が内側）
    pub fn then(self, o: Xf) -> Xf {
        Xf { a: self.a * o.a + self.c * o.b, b: self.b * o.a + self.d * o.b, c: self.a * o.c + self.c * o.d, d: self.b * o.c + self.d * o.d, e: self.a * o.e + self.c * o.f + self.e, f: self.b * o.e + self.d * o.f + self.f }
    }
    /// transform-origin を (x, y) にした o
    pub fn about(x: f32, y: f32, o: Xf) -> Xf { Xf::tr(x, y).then(o).then(Xf::tr(-x, -y)) }
    #[inline] pub fn ap(&self, p: Pos2) -> Pos2 { pos2(self.a * p.x + self.c * p.y + self.e, self.b * p.x + self.d * p.y + self.f) }
    fn is_move(&self) -> bool { self.a == 1.0 && self.b == 0.0 && self.c == 0.0 && self.d == 1.0 }
    /// 拡大の目安（文字の影など）
    fn scale_k(&self) -> f32 { (self.a * self.d - self.b * self.c).abs().sqrt() }
}

// ---- 描くもの ----

/// CSS の text-shadow（x, y, ぼかし, 色）
#[derive(Clone, Copy, Debug)]
pub struct TS(pub f32, pub f32, pub f32, pub C);

/// 文字の塗り。V は em の箱の上から下へのグラデーション（background-clip: text）
#[derive(Clone, Copy)]
pub enum Fill<'a> { S(C), V(&'a [(f32, C)]) }
impl<'a> From<C> for Fill<'a> { fn from(c: C) -> Self { Fill::S(c) } }

/// 組んだ文字
#[derive(Clone)]
pub struct T { g: Arc<Galley>, pub size: f32, pub w: f32, base0: f32 }
impl T {
    pub fn rows(&self) -> usize { self.g.rows.len() }
    pub fn is_empty(&self) -> bool { self.g.is_empty() }
}

/// CSS の行分け（line-break: auto）で折ってよい所で区切る。空白の後ろと、和文の字の間。句読点と閉じかっこの前、開きかっこの後ろでは折らない
pub fn break_chunks(text: &str) -> Vec<String> {
    let wide = |c: char| ('\u{2E80}'..='\u{9FFF}').contains(&c) || ('\u{F900}'..='\u{FAFF}').contains(&c) || ('\u{FF00}'..='\u{FFEF}').contains(&c);
    let ok = |a: char, b: char| {
        if a == ' ' { return b != ' '; }
        if b == ' ' || a == '\u{a0}' || b == '\u{a0}' { return false; }
        if "、。，．・：；？！）］｝」』】〉》…‥".contains(b) || "（［｛「『【〈《".contains(a) { return false; }
        wide(a) || wide(b)
    };
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        cur.push(c);
        if chars.get(i + 1).is_some_and(|&n| ok(c, n)) { out.push(std::mem::take(&mut cur)); }
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

/// 読み込んだ画像
#[derive(Clone)]
pub struct Tex { pub handle: TextureHandle, pub w: f32, pub h: f32 }
/// PNG / JPEG / WebP のバイト列を egui のテクスチャへ
pub fn load_image(ctx: &Context, name: &str, bytes: &[u8]) -> Option<Tex> {
    let im = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = im.dimensions();
    let color = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], im.as_raw());
    Some(Tex { handle: ctx.load_texture(name, color, egui::TextureOptions::LINEAR), w: w as f32, h: h as f32 })
}

/// object-fit: cover の UV（align は 0..1。0.5 が中央、object-position）
pub fn cover_uv(t: &Tex, box_w: f32, box_h: f32, ax: f32, ay: f32) -> Rect {
    let k = (box_w / t.w).max(box_h / t.h);
    let (uw, uh) = (box_w / (t.w * k), box_h / (t.h * k));
    let (x0, y0) = ((1.0 - uw) * ax, (1.0 - uh) * ay);
    Rect::from_min_max(pos2(x0, y0), pos2(x0 + uw, y0 + uh))
}
/// object-fit: contain の置き場所
pub fn contain_rect(t: &Tex, b: Rect) -> Rect {
    let k = (b.width() / t.w).min(b.height() / t.h);
    Rect::from_center_size(b.center(), vec2(t.w * k, t.h * k))
}

/// 角丸の四角の点（時計回り）
/// 角 1 つを何本の線で丸めるか（半径 8px までは 4、大きな丸ボタンは 24 まで。keiba の seg）
pub fn rr_seg(rad: f32) -> usize { ((rad * 0.5).ceil() as usize).clamp(4, 24) }

pub fn rr_points(r: Rect, rad: f32, seg: usize) -> Vec<Pos2> {
    // 輪を重ねるので、半径が 0 でも点の数は変えない
    let rad = rad.min(r.width() * 0.5).min(r.height() * 0.5).max(0.0);
    let mut out = Vec::with_capacity((seg + 1) * 4);
    let cs = [(r.right() - rad, r.top() + rad, -90.0_f32), (r.right() - rad, r.bottom() - rad, 0.0), (r.left() + rad, r.bottom() - rad, 90.0), (r.left() + rad, r.top() + rad, 180.0)];
    for (cx, cy, start) in cs {
        for i in 0..=seg {
            let a = (start + 90.0 * i as f32 / seg as f32).to_radians();
            out.push(pos2(cx + a.cos() * rad, cy + a.sin() * rad));
        }
    }
    out
}

/// CSS の変形と透明度を引き継いで描くペン
#[derive(Clone)]
pub struct Pen { pub painter: Painter, pub xf: Xf, pub alpha: f32, ppp: f32, pub ctx: Context }

impl Pen {
    pub fn new(ctx: &Context, painter: Painter) -> Pen {
        Pen { painter, xf: Xf::ID, alpha: 1.0, ppp: ctx.pixels_per_point(), ctx: ctx.clone() }
    }
    /// 子の要素（transform と opacity を重ねる）
    pub fn sub(&self, xf: Xf, alpha: f32) -> Pen { Pen { xf: self.xf.then(xf), alpha: self.alpha * alpha, ..self.clone() } }
    pub fn with_alpha(&self, alpha: f32) -> Pen { self.sub(Xf::ID, alpha) }
    /// overflow: hidden の代わり（回っていない箱だけ）
    pub fn clip(&self, r: Rect) -> Pen { let mut p = self.clone(); let c = p.painter.clip_rect().intersect(Rect::from_two_pos(self.xf.ap(r.min), self.xf.ap(r.max))); p.painter.set_clip_rect(c); p }

    fn add_mesh(&self, m: Mesh) { if !m.is_empty() { self.painter.add(Shape::mesh(m)); } }

    /// 凸の多角形（縁はなめらか）
    pub fn poly(&self, pts: &[Pos2], c: C) {
        if self.alpha <= 0.0 || c.a <= 0.0 { return; }
        let v: Vec<Pos2> = pts.iter().map(|p| self.xf.ap(*p)).collect();
        self.painter.add(Shape::Path(PathShape::convex_polygon(v, c.c32(self.alpha), Stroke::NONE)));
    }
    pub fn rect(&self, r: Rect, c: C) { self.poly(&[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()], c); }
    pub fn rrect(&self, r: Rect, rad: f32, c: C) { self.poly(&rr_points(r, rad, rr_seg(rad)), c); }
    pub fn circle(&self, center: Pos2, rad: f32, c: C) {
        if self.alpha <= 0.0 || c.a <= 0.0 { return; }
        self.painter.add(Shape::circle_filled(self.xf.ap(center), rad * self.xf.scale_k(), c.c32(self.alpha)));
    }
    pub fn circle_line(&self, center: Pos2, rad: f32, w: f32, c: C) {
        if self.alpha <= 0.0 || c.a <= 0.0 { return; }
        self.painter.add(Shape::circle_stroke(self.xf.ap(center), rad * self.xf.scale_k(), Stroke::new(w * self.xf.scale_k(), c.c32(self.alpha))));
    }
    /// 閉じた線（box の border。線の中心を通る点を渡す）
    pub fn closed(&self, pts: &[Pos2], w: f32, c: C) {
        if self.alpha <= 0.0 || c.a <= 0.0 { return; }
        let v: Vec<Pos2> = pts.iter().map(|p| self.xf.ap(*p)).collect();
        self.painter.add(Shape::closed_line(v, Stroke::new(w * self.xf.scale_k(), c.c32(self.alpha))));
    }
    /// 開いた線
    pub fn line(&self, pts: &[Pos2], w: f32, c: C) {
        if self.alpha <= 0.0 || c.a <= 0.0 { return; }
        let v: Vec<Pos2> = pts.iter().map(|p| self.xf.ap(*p)).collect();
        self.painter.add(Shape::line(v, Stroke::new(w * self.xf.scale_k(), c.c32(self.alpha))));
    }
    /// border: Wpx solid（box-sizing: border-box。外形が r）
    pub fn frame(&self, r: Rect, w: f32, c: C) { let i = r.shrink(w * 0.5); self.closed(&[i.left_top(), i.right_top(), i.right_bottom(), i.left_bottom()], w, c); }
    pub fn rframe(&self, r: Rect, rad: f32, w: f32, c: C) { let rr = (rad - w * 0.5).max(0.0); self.closed(&rr_points(r.shrink(w * 0.5), rr, rr_seg(rr)), w, c); }

    /// 列の頂点で作るグラデーション（上の列と下の列。周りをなめらかに）
    fn strip(&self, top: &[Pos2], bot: &[Pos2], cols: &[C]) {
        if self.alpha <= 0.0 { return; }
        let mut m = Mesh::default();
        let n = top.len();
        for i in 0..n { m.colored_vertex(self.xf.ap(top[i]), cols[i].c32(self.alpha)); }
        for i in 0..n { m.colored_vertex(self.xf.ap(bot[i]), cols[i].c32(self.alpha)); }
        for i in 0..n as u32 - 1 {
            m.add_triangle(i, i + 1, n as u32 + i);
            m.add_triangle(i + 1, n as u32 + i + 1, n as u32 + i);
        }
        // 周囲（上の列 → 下の列を逆向き）
        let mut per: Vec<u32> = (0..n as u32).collect();
        per.extend((0..n as u32).rev().map(|i| n as u32 + i));
        feather(&mut m, &per, self.ppp);
        self.add_mesh(m);
    }
    /// 横向きのグラデーション（linear-gradient(90deg, ..)）。止まりは 0..1
    pub fn hgrad(&self, r: Rect, stops: &[(f32, C)]) { self.slant(r, 0.0, stops); }
    /// 横向きのグラデーションを clip-path の平行四辺形（polygon(s 0, 100% 0, 100%-s 100%, 0 100%)）に
    pub fn slant(&self, r: Rect, s: f32, stops: &[(f32, C)]) {
        let w = r.width();
        let top: Vec<Pos2> = stops.iter().map(|(u, _)| pos2(r.left() + w * (s + (1.0 - s) * u), r.top())).collect();
        let bot: Vec<Pos2> = stops.iter().map(|(u, _)| pos2(r.left() + w * ((1.0 - s) * u), r.bottom())).collect();
        let cols: Vec<C> = stops.iter().map(|(_, c)| *c).collect();
        self.strip(&top, &bot, &cols);
    }
    /// 縦向きのグラデーション（linear-gradient(180deg, ..)）
    pub fn vgrad(&self, r: Rect, stops: &[(f32, C)]) {
        // 横の列として組む（上の列 = 左の辺、下の列 = 右の辺）
        let left: Vec<Pos2> = stops.iter().map(|(u, _)| pos2(r.left(), r.top() + r.height() * u)).collect();
        let right: Vec<Pos2> = stops.iter().map(|(u, _)| pos2(r.right(), r.top() + r.height() * u)).collect();
        let cols: Vec<C> = stops.iter().map(|(_, c)| *c).collect();
        self.strip(&left, &right, &cols);
    }
    /// 凸の多角形を斜めのグラデーションで（linear-gradient(deg, c0, c1)。box はグラデーションの箱）
    pub fn poly_grad(&self, pts: &[Pos2], bx: Rect, deg: f32, c0: C, c1: C) {
        if self.alpha <= 0.0 { return; }
        let (s, co) = deg.to_radians().sin_cos();
        let (dx, dy) = (s, -co);
        let len = (bx.width() * s).abs() + (bx.height() * co).abs();
        let cc = bx.center();
        let mut m = Mesh::default();
        for p in pts {
            let t = ((p.x - cc.x) * dx + (p.y - cc.y) * dy) / len + 0.5;
            m.colored_vertex(self.xf.ap(*p), c0.lerp(c1, clamp01(t)).c32(self.alpha));
        }
        for i in 1..pts.len() as u32 - 1 { m.add_triangle(0, i, i + 1); }
        let per: Vec<u32> = (0..pts.len() as u32).collect();
        feather(&mut m, &per, self.ppp);
        self.add_mesh(m);
    }
    /// radial-gradient(rx ry at cx cy, ..)（格子で近似する）
    pub fn radial(&self, r: Rect, c: Pos2, rx: f32, ry: f32, stops: &[(f32, C)], nx: usize, ny: usize) {
        if self.alpha <= 0.0 { return; }
        let mut m = Mesh::default();
        for j in 0..=ny {
            for i in 0..=nx {
                let p = pos2(r.left() + r.width() * i as f32 / nx as f32, r.top() + r.height() * j as f32 / ny as f32);
                let t = (((p.x - c.x) / rx).powi(2) + ((p.y - c.y) / ry).powi(2)).sqrt();
                m.colored_vertex(self.xf.ap(p), stops_at(stops, t).c32(self.alpha));
            }
        }
        let w = nx as u32 + 1;
        for j in 0..ny as u32 {
            for i in 0..nx as u32 {
                let a = j * w + i;
                m.add_triangle(a, a + 1, a + w);
                m.add_triangle(a + 1, a + w + 1, a + w);
            }
        }
        self.add_mesh(m);
    }
    /// box-shadow（箱の外だけ。ずらした影も箱の下に影の無い帯を作らない）。blur は CSS のぼかし半径（標準偏差はその半分）。
    /// ずらした角丸の箱のぼかし（erfc）を頂点の色で塗る（2026-11-10 rainbow の ui.rs から）
    pub fn box_shadow(&self, r: Rect, rad: f32, dx: f32, dy: f32, blur: f32, c: C) {
        if self.alpha <= 0.0 || c.a <= 0.0 { return; }
        let sigma = (blur * 0.5).max(0.3);
        let s = r.translate(vec2(dx, dy));
        let hw = vec2(s.width() * 0.5 - rad, s.height() * 0.5 - rad);
        let cov = |p: Pos2| {
            let q = (p - s.center()).abs() - hw;
            let sd = vec2(q.x.max(0.0), q.y.max(0.0)).length() + q.x.max(q.y).min(0.0) - rad;
            0.5 * erfc(sd / (sigma * std::f32::consts::SQRT_2))
        };
        let reach = dx.abs().max(dy.abs()) + blur * 1.6 + 2.0;
        let mut ds = vec![0.0f32];
        let mut d = 0.5f32;
        while d < reach { ds.push(d); d = (d * 1.35).max(d + 0.75); }
        ds.push(reach);
        let rings: Vec<Vec<Pos2>> = ds.iter().map(|d| rr_points(r.expand(*d), rad + d, 6)).collect();
        let n = rings[0].len() as u32;
        if rings.iter().any(|x| x.len() as u32 != n) { return; }
        let mut m = Mesh::default();
        for ring in &rings { for p in ring { m.colored_vertex(self.xf.ap(*p), c.alpha(cov(*p)).c32(self.alpha)); } }
        for j in 0..rings.len() as u32 - 1 {
            for i in 0..n {
                let (a, b) = (j * n + i, j * n + (i + 1) % n);
                m.add_triangle(a, b, a + n);
                m.add_triangle(b, b + n, a + n);
            }
        }
        self.add_mesh(m);
    }
    /// box-shadow / にじみ。blur は CSS のぼかし半径。filled なら内側も塗る（text-shadow 用）、そうでなければ外側だけ（box-shadow と同じく箱の下は抜く）
    pub fn soft(&self, r: Rect, rad: f32, blur: f32, c: C, filled: bool) {
        if self.alpha <= 0.0 || c.a <= 0.0 || blur <= 0.0 { return; }
        // 標準偏差 blur/2 のガウスの積分を、輪の頂点で近似する（-B, -B/2, 0, B/2, B で .98 .84 .5 .16 .02）
        const RINGS: [(f32, f32); 5] = [(-1.0, 0.977), (-0.5, 0.841), (0.0, 0.5), (0.5, 0.159), (1.0, 0.0)];
        let start = if filled { 0 } else { 2 };
        let seg = 4;
        let mut m = Mesh::default();
        let mut count = 0;
        for (d, k) in &RINGS[start..] {
            let d = d * blur;
            let rr = if d < 0.0 { r.expand(d.max(-r.width().min(r.height()) * 0.5 + 0.01)) } else { r.expand(d) };
            let pts = rr_points(rr, (rad + d + blur).max(blur * 0.5), seg);
            count = pts.len() as u32;
            for p in pts { m.colored_vertex(self.xf.ap(p), c.alpha(*k).c32(self.alpha)); }
        }
        let rings = (RINGS.len() - start) as u32;
        for j in 0..rings - 1 {
            for i in 0..count {
                let (a, b) = (j * count + i, j * count + (i + 1) % count);
                let (c2, d2) = (a + count, b + count);
                m.add_triangle(a, b, c2);
                m.add_triangle(b, d2, c2);
            }
        }
        if filled { for i in 1..count - 1 { m.add_triangle(0, i, i + 1); } }
        self.add_mesh(m);
    }

    // ---- 文字 ----

    fn layout(&self, text: &str, fam: Fam, size: f32, spacing: f32, wrap: Option<(f32, f32)>) -> T {
        let mut job = LayoutJob::default();
        let (w, lh) = wrap.map_or((f32::INFINITY, None), |(w, l)| (w, Some(l)));
        job.wrap.max_width = w;
        job.break_on_newline = true;
        job.append(text, 0.0, TextFormat { font_id: FontId::new(size, fam.family()), color: Color32::WHITE, extra_letter_spacing: spacing, line_height: lh, ..Default::default() });
        let g = self.ctx.fonts_mut(|f| f.layout_job(job));
        let base0 = g.rows.first().and_then(|r| r.glyphs.first().map(|gl| r.pos.y + gl.pos.y)).unwrap_or(size * 0.88);
        // CSS の箱は最後の字の後ろの字間も含む
        let w = g.rect.width() + if text.is_empty() { 0.0 } else { spacing };
        T { g, size, w, base0 }
    }
    pub fn t(&self, text: &str, fam: Fam, size: f32, spacing: f32) -> T { self.layout(text, fam, size, spacing, None) }
    /// 折り返す文字（max_w で折り、行の高さ line_h）
    pub fn tw(&self, text: &str, fam: Fam, size: f32, spacing: f32, max_w: f32, line_h: f32) -> T { self.layout(text, fam, size, spacing, Some((max_w, line_h))) }

    /// CSS の既定（white-space: normal）と同じ所で折った行。max_w に収まれば 1 行のまま
    pub fn lines(&self, text: &str, fam: Fam, size: f32, spacing: f32, max_w: f32) -> Vec<T> {
        let whole = self.t(text, fam, size, spacing);
        if whole.w <= max_w { return vec![whole]; }
        let mut rows = vec![];
        let mut line = String::new();
        for s in break_chunks(text) {
            let cand = format!("{line}{s}");
            if !line.is_empty() && self.t(cand.trim_end(), fam, size, spacing).w > max_w {
                rows.push(self.t(line.trim_end(), fam, size, spacing));
                line = s;
            } else { line = cand; }
        }
        if !line.trim_end().is_empty() { rows.push(self.t(line.trim_end(), fam, size, spacing)); }
        rows
    }
    /// 折れない塊のいちばん広い幅（CSS の min-content）
    pub fn min_w(&self, text: &str, fam: Fam, size: f32, spacing: f32) -> f32 {
        break_chunks(text).iter().map(|s| self.t(s.trim_end(), fam, size, spacing).w).fold(0.0, f32::max)
    }
    /// 太字の面が無い字体の太字（Chrome が線を太らせる分。字の大きさの 1/24〜1/32）
    pub fn text_faux_bold(&self, t: &T, x: f32, base: f32, fill: C, shadows: &[TS]) {
        for s in shadows.iter().rev() { self.text_shadow(t, x, base, *s); }
        let k = clamp01((t.size - 9.0) / 27.0);
        let r = t.size * lerp(1.0 / 24.0, 1.0 / 32.0, k) * 0.5;
        for i in 0..8 {
            let a = i as f32 / 8.0 * std::f32::consts::TAU;
            self.text_mesh(t, x + a.cos() * r, base + a.sin() * r, Fill::S(fill), 1.0);
        }
    }
    /// font-variant-numeric: tabular-nums。Dela Gothic One の等幅の数字（.tf）は幅 0.9em で、形は同じで左の余白だけ違う。字とその位置と全体の幅を返す
    pub fn tabular(&self, s: &str, size: f32) -> (Vec<(T, f32)>, f32) {
        // 等幅の字の左の余白 - 普通の字の左の余白（1000 分の 1 em）
        const SHIFT: [f32; 10] = [-9.0, 143.0, 33.0, 10.0, -12.0, 8.0, 22.0, 48.0, 12.0, 22.0];
        let v: Vec<(T, f32)> = s.chars().enumerate().map(|(i, c)| {
            let d = c.to_digit(10).unwrap_or(0) as usize;
            (self.t(&c.to_string(), Fam::Num, size, 0.0), (i as f32 * 0.9 + SHIFT[d] / 1000.0) * size)
        }).collect();
        (v, s.chars().count() as f32 * 0.9 * size)
    }
    /// 行を上から順に描く（x は左端、top は 1 行目の行の箱の上）
    pub fn text_lines(&self, rows: &[T], x: f32, top: f32, lh: f32, fill: C, shadows: &[TS]) {
        for (i, t) in rows.iter().enumerate() { self.text(t, x, top + lh * (i as f32 + 0.5) + t.size * BASE_K, fill, shadows); }
    }

    /// 文字の頂点を組む
    fn text_mesh(&self, t: &T, x: f32, base: f32, fill: Fill, alpha: f32) {
        let a = self.alpha * alpha;
        if a <= 0.0 { return; }
        let o = pos2(x, base - t.base0);
        // 変形が移動だけなら、画素に合わせて文字をくっきりさせる
        let (xf, o) = if self.xf.is_move() {
            let q = self.xf.ap(o);
            (Xf::ID, pos2((q.x * self.ppp).round() / self.ppp, (q.y * self.ppp).round() / self.ppp))
        } else { (self.xf, o) };
        let mut m = Mesh::default();
        for row in &t.g.rows {
            let vs = &row.row.visuals.mesh.vertices;
            let range = row.row.visuals.glyph_vertex_range.clone();
            for q in vs[range].chunks_exact(4) {
                let off = o.to_vec2() + row.pos.to_vec2();
                match fill {
                    Fill::S(c) => {
                        let col = c.c32(a);
                        let idx = m.vertices.len() as u32;
                        for v in q { m.vertices.push(Vertex { pos: xf.ap(v.pos + off), uv: v.uv, color: col }); }
                        m.add_triangle(idx, idx + 1, idx + 2);
                        m.add_triangle(idx + 2, idx + 1, idx + 3);
                    }
                    Fill::V(stops) => {
                        // グラデーションの止まりごとに字の四角を横に切る
                        let (lt, rt, lb, rb) = (q[0], q[1], q[2], q[3]);
                        let to_t = |y: f32| (y + off.y - (o.y + t.base0)) / t.size + 0.88;
                        let (t0, t1) = (to_t(lt.pos.y), to_t(lb.pos.y));
                        let mut cuts = vec![0.0_f32];
                        for (s, _) in stops { if *s > t0 && *s < t1 { cuts.push((*s - t0) / (t1 - t0).max(1e-5)); } }
                        cuts.push(1.0);
                        let mut idx = m.vertices.len() as u32;
                        for (i, k) in cuts.iter().enumerate() {
                            let col = stops_at(stops, lerp(t0, t1, *k)).c32(a);
                            let (pl, pr) = (lt.pos + (lb.pos - lt.pos) * *k, rt.pos + (rb.pos - rt.pos) * *k);
                            let (ul, ur) = (lt.uv + (lb.uv - lt.uv) * *k, rt.uv + (rb.uv - rt.uv) * *k);
                            m.vertices.push(Vertex { pos: xf.ap(pl + off), uv: ul, color: col });
                            m.vertices.push(Vertex { pos: xf.ap(pr + off), uv: ur, color: col });
                            if i > 0 {
                                m.add_triangle(idx - 2, idx - 1, idx);
                                m.add_triangle(idx, idx - 1, idx + 1);
                            }
                            idx += 2;
                        }
                    }
                }
            }
        }
        self.add_mesh(m);
    }
    /// 文字を基線の位置へ（x は左端、base は基線）。影は CSS と同じく先に書いたものが上
    pub fn text(&self, t: &T, x: f32, base: f32, fill: impl Into<Fill<'static>>, shadows: &[TS]) { self.text_f(t, x, base, fill.into(), shadows, 0.0) }
    /// グラデーションの塗りなど（blur は filter: blur）
    pub fn text_f(&self, t: &T, x: f32, base: f32, fill: Fill, shadows: &[TS], blur: f32) {
        for s in shadows.iter().rev() { self.text_shadow(t, x, base, *s); }
        if blur > 0.3 {
            // filter: blur の近似: ずらした写しを重ねる（文字の形がぼける）
            // blur(r) はガウスの標準偏差 r。写しを 3 つの輪に置いて、外ほど薄くする
            for (n, k, al) in [(14, 1.8, 0.035), (10, 1.1, 0.07), (6, 0.5, 0.11)] {
                for i in 0..n {
                    let a = (i as f32 + k) / n as f32 * std::f32::consts::TAU;
                    self.text_mesh(t, x + a.cos() * blur * k, base + a.sin() * blur * k, fill, al);
                }
            }
            self.text_mesh(t, x, base, fill, 0.11);
        } else { self.text_mesh(t, x, base, fill, 1.0); }
    }
    fn text_shadow(&self, t: &T, x: f32, base: f32, s: TS) {
        let TS(dx, dy, blur, c) = s;
        if blur <= 0.0 { self.text_mesh(t, x + dx, base + dy, Fill::S(c), 1.0); return; }
        // 文字の写しを渦巻きに置いて重ねる（標準偏差 blur/2 のガウスの近似）。
        // ぼかしが大きいときは、小さく組んだ文字を拡大して使う（テクスチャの補間でなめらかになり、線の筋が出ない）
        let k = (blur * self.ppp * self.xf.scale_k() / 5.0).max(1.0).min(t.size * self.ppp * self.xf.scale_k() / 9.0).max(1.0);
        // 0.25 刻みにそろえる。拡大が動く文字（得点の跳ね・お知らせなど）で k が連続に変わると、毎コマ新しい大きさの字が
        // 組まれて字のテクスチャが伸び続け、伸びるコマが 120〜240ms 止まった（2026-11-10 rainbow の指摘）
        let k = ((k * 4.0).round() / 4.0).max(1.0);
        // 円の中に一様に置くと標準偏差は半径の半分。字の線より広くぼかすときは重なりが少ないので、足して 1 に近づける
        let (n, r, sum) = if k > 1.5 { (24, blur * 0.95, 1.05) } else if blur >= 6.0 { (16, blur * 0.75, 1.35) } else { (10, blur * 0.7, 1.35) };
        let each = sum / n as f32;
        let low = if k > 1.5 {
            let f = &t.g.job.sections[0].format;
            let mut job = LayoutJob::default();
            // 折り返しは元の行のまま（小さく組むと折れ方が変わるので、改行で固定する）
            let text: Vec<String> = t.g.rows.iter().map(|r| r.row.glyphs.iter().map(|g| g.chr).collect()).collect();
            job.append(&text.join("\n"), 0.0, TextFormat { font_id: FontId::new(t.size / k, f.font_id.family.clone()), color: Color32::WHITE, extra_letter_spacing: f.extra_letter_spacing / k, line_height: f.line_height.map(|l| l / k), ..Default::default() });
            let g = self.ctx.fonts_mut(|fs| fs.layout_job(job));
            let base0 = g.rows.first().and_then(|r| r.glyphs.first().map(|gl| r.pos.y + gl.pos.y)).unwrap_or(0.0);
            let w = g.rect.width();
            Some(T { g, size: t.size / k, w, base0 })
        } else { None };
        for i in 0..n {
            let a = i as f32 * 2.399963;
            let q = r * ((i as f32 + 0.5) / n as f32).sqrt();
            let (ox, oy) = (x + dx + a.cos() * q, base + dy + a.sin() * q);
            match &low {
                Some(l) => self.sub(Xf::tr(ox, oy).then(Xf::scale(k, k)), 1.0).text_mesh(l, 0.0, 0.0, Fill::S(c), each),
                None => self.text_mesh(t, ox, oy, Fill::S(c), each),
            }
        }
    }

    // ---- 画像 ----

    /// 凸の多角形に画像を貼る。img は画像全体が載る box、uv はその box に対応する UV（cover_uv など）
    pub fn image_poly(&self, tex: TextureId, pts: &[Pos2], img: Rect, uv: Rect, tint: C) {
        if self.alpha <= 0.0 { return; }
        let mut m = Mesh::with_texture(tex);
        let col = tint.c32(self.alpha);
        for p in pts {
            let (u, v) = ((p.x - img.left()) / img.width(), (p.y - img.top()) / img.height());
            m.vertices.push(Vertex { pos: self.xf.ap(*p), uv: pos2(lerp(uv.left(), uv.right(), u), lerp(uv.top(), uv.bottom(), v)), color: col });
        }
        for i in 1..pts.len() as u32 - 1 { m.add_triangle(0, i, i + 1); }
        let per: Vec<u32> = (0..pts.len() as u32).collect();
        feather(&mut m, &per, self.ppp);
        self.add_mesh(m);
    }
    pub fn image(&self, t: &Tex, r: Rect, tint: C) {
        self.image_poly(t.handle.id(), &[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()], r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
    }
}

/// 頂点の周りに透明な縁を足して、ギザギザを消す（周囲は凸の順）
fn feather(m: &mut Mesh, per: &[u32], ppp: f32) {
    let n = per.len();
    if n < 3 { return; }
    let pts: Vec<Pos2> = per.iter().map(|i| m.vertices[*i as usize].pos).collect();
    let mut area = 0.0;
    for i in 0..n { let (a, b) = (pts[i], pts[(i + 1) % n]); area += a.x * b.y - b.x * a.y; }
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let base = m.vertices.len() as u32;
    let w = 0.9 / ppp;
    for i in 0..n {
        let (p, a, b) = (pts[i], pts[(i + n - 1) % n], pts[(i + 1) % n]);
        let (e0, e1) = ((p - a).normalized(), (b - p).normalized());
        let nrm = vec2(e0.y + e1.y, -(e0.x + e1.x)).normalized() * sign;
        let v0 = m.vertices[per[i] as usize];
        m.vertices.push(Vertex { pos: p + nrm * w, uv: v0.uv, color: Color32::TRANSPARENT });
    }
    for i in 0..n as u32 {
        let j = (i + 1) % n as u32;
        m.add_triangle(per[i as usize], per[j as usize], base + i);
        m.add_triangle(per[j as usize], base + j, base + i);
    }
}

// ---- 文字とキーの札を並べた 1 行 ----

/// .kbd
#[derive(Clone)]
pub struct Kbd { t: T, w: f32, h: f32, m: f32, color: C, border: C }
impl Kbd {
    /// em は親の文字の大きさ、spacing は親から受け継ぐ字間
    pub fn new(pen: &Pen, label: &str, em: f32, spacing: f32, color: C, border: C) -> Kbd {
        let fs = em * 0.74;
        let t = pen.t(label, Fam::Jp700, fs, spacing);
        Kbd { w: (t.w + fs * 0.8 + 2.0).max(fs * 1.8), h: fs * 1.6, m: fs * 0.2, t, color, border }
    }
    pub fn plain(pen: &Pen, label: &str, em: f32, spacing: f32) -> Kbd { Kbd::new(pen, label, em, spacing, CREAM, rgba(255, 243, 224, 0.45)) }
    pub fn gold(pen: &Pen, label: &str, em: f32, spacing: f32) -> Kbd { Kbd::new(pen, label, em, spacing, GOLD, GOLD) }
    pub fn full_w(&self) -> f32 { self.w + self.m * 2.0 }
    /// position: absolute の札（箱の右上を置く。margin の分だけ内へ）
    pub fn draw_at(&self, pen: &Pen, right: f32, top: f32) { self.draw(pen, right - self.full_w(), top + self.h * 0.5 + self.t.size * BASE_K); }
    pub fn draw(&self, pen: &Pen, x: f32, base: f32) {
        let cy = base - self.t.size * BASE_K;
        let r = Rect::from_min_size(pos2(x + self.m, cy - self.h * 0.5), vec2(self.w, self.h));
        pen.rrect(r, 4.0, rgba(255, 255, 255, 0.06));
        pen.rframe(r, 4.0, 1.0, self.border);
        // border-bottom-width: 2px
        pen.line(&[pos2(r.left() + 3.0, r.bottom() - 1.5), pos2(r.right() - 3.0, r.bottom() - 1.5)], 1.0, self.border);
        pen.text(&self.t, r.center().x - self.t.w * 0.5, base, self.color, &[]);
    }
}

/// 1 行の中身
pub enum P { T(T, C, Vec<TS>), K(Kbd), Gap(f32) }
pub fn row_w(items: &[P]) -> f32 { items.iter().map(|p| match p { P::T(t, _, _) => t.w, P::K(k) => k.full_w(), P::Gap(g) => *g }).sum() }
pub fn row_draw(pen: &Pen, items: &[P], x: f32, base: f32) {
    let mut x = x;
    for p in items {
        match p {
            P::T(t, c, s) => { pen.text(t, x, base, *c, s); x += t.w; }
            P::K(k) => { k.draw(pen, x, base); x += k.full_w(); }
            P::Gap(g) => x += *g,
        }
    }
}

