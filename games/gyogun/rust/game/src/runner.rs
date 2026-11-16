// ウィンドウを開いて App を回す。Mac 版は引数、ブラウザ版は URL の引数で同じ設定を受ける。
// --auto --start --seed N --fixed --size WxH --window WxH --hero id --weapon id --from studio --bench 秒 --bench-frames N --out file --no-vsync --god --capture n --ranbu 秒 --result 秒 --teleport x,z --safe 上,右,下,左 --shot 秒:path
use crate::app::{App, Options};
use crate::gfx::Gfx;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

pub fn apply_arg(opts: &mut Options, size: &mut (u32, u32), k: &str, v: Option<String>) {
    fn pair<A: std::str::FromStr, B: std::str::FromStr>(v: &Option<String>, sep: char) -> Option<(A, B)> {
        let (a, b) = v.as_ref()?.split_once(sep)?;
        Some((a.parse().ok()?, b.parse().ok()?))
    }
    match k {
        "auto" => opts.auto = true,
        "start" => opts.start = true,
        "seed" => opts.seed = v.and_then(|x| x.parse().ok()),
        "fixed" => opts.fixed = true,
        "size" => if let Some(p) = pair(&v, 'x') { opts.size = Some(p); },
        "window" => if let Some(p) = pair(&v, 'x') { *size = p; },
        "hero" => opts.hero = v,
        "weapon" => opts.weapon = v,
        // Studio の「魚群繚乱で遊ぶ」から来た直後（?from=studio）: 送られたキャラを選んだ状態で始める
        "from" => opts.from_studio = v.as_deref() == Some("studio"),
        "bench" => opts.bench_secs = v.and_then(|x| x.parse().ok()),
        "bench-frames" => opts.bench_frames = v.and_then(|x| x.parse().ok()),
        "out" => opts.bench_out = v,
        "no-vsync" => opts.no_vsync = true,
        "god" => opts.god = true,
        // 確かめ用: 見た目の乱数を Math.random にする（今の版と同じ値を入れて比べる）
        "jsrand" => crate::platform::set_js_random(true),
        "capture" => opts.capture = v.and_then(|x| x.parse().ok()).unwrap_or(0),
        "ranbu" => opts.ranbu_at = v.and_then(|x| x.parse().ok()),
        "result" => opts.result_at = v.and_then(|x| x.parse().ok()),
        "teleport" => if let Some(p) = pair(&v, ',') { opts.teleport = Some(p); },
        // 確かめ用: Mac 版の端の安全域（上,右,下,左 の論理 px）。ブラウザ版の ?safe= は共通部品の page::safe_area が読む（ここへは来ない）
        "safe" => opts.safe = v.as_deref().and_then(crate::ui::Safe::parse),
        "shot" => if let Some((t, p)) = v.as_ref().and_then(|x| x.split_once(':')) { opts.shots.push((t.parse().unwrap_or(0.0), p.to_string())); },
        other => eprintln!("知らない引数: {other}"),
    }
}

/// 引数を持つ設定か（--seed N など）
pub fn takes_value(k: &str) -> bool { !matches!(k, "auto" | "start" | "fixed" | "no-vsync" | "god" | "jsrand") }

pub struct Runner {
    pub opts: Options,
    pub size: (u32, u32),
    window: Option<Arc<Window>>,
    app: Option<App>,
    #[cfg(target_arch = "wasm32")]
    pending: std::rc::Rc<std::cell::RefCell<Option<Result<Gfx, String>>>>,
}

impl Runner {
    pub fn new(opts: Options, size: (u32, u32)) -> Self {
        Runner { opts, size, window: None, app: None, #[cfg(target_arch = "wasm32")] pending: Default::default() }
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.window.is_some() { return; }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let attrs = Window::default_attributes().with_title("魚群繚乱").with_inner_size(winit::dpi::LogicalSize::new(self.size.0, self.size.1));
            let window = Arc::new(el.create_window(attrs).expect("ウィンドウを開けません"));
            let mut gfx = pollster::block_on(Gfx::new(window.clone())).expect("描画の準備ができません");
            if self.opts.no_vsync {
                gfx.config.present_mode = wgpu::PresentMode::AutoNoVsync;
                gfx.surface.configure(&gfx.device, &gfx.config);
            }
            self.app = Some(App::new(window.clone(), gfx, self.opts.clone()));
            self.window = Some(window);
        }
        #[cfg(target_arch = "wasm32")]
        {
            use wasm_bindgen::JsCast;
            use winit::platform::web::WindowAttributesExtWebSys;
            let canvas = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("gl")).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
            let attrs = Window::default_attributes().with_title("魚群繚乱").with_canvas(canvas).with_prevent_default(false).with_focusable(true).with_active(true);
            crate::platform::web::keep_browser_keys();
            let window = Arc::new(el.create_window(attrs).expect("描く場所を作れません"));
            self.window = Some(window.clone());
            let pending = self.pending.clone();
            let w2 = window.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let g = Gfx::new(window).await;
                *pending.borrow_mut() = Some(g);
                w2.request_redraw();
            });
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, e: WindowEvent) {
        #[cfg(target_arch = "wasm32")]
        if self.app.is_none() {
            let got = self.pending.borrow_mut().take();
            match got {
                Some(Ok(gfx)) => {
                    let window = self.window.clone().unwrap();
                    self.app = Some(App::new(window, gfx, self.opts.clone()));
                }
                Some(Err(e)) => crate::platform::web::fatal(&format!("描画の準備ができません: {e}")),
                None => {}
            }
        }
        let Some(app) = self.app.as_mut() else {
            if let (WindowEvent::RedrawRequested, Some(w)) = (&e, &self.window) { w.request_redraw(); }
            return;
        };
        #[cfg(target_arch = "wasm32")]
        crate::game_api::register(app);
        match e {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::RedrawRequested => {
                app.frame();
                if app.quit { el.exit(); return; }
                if let Some(w) = &self.window { w.request_redraw(); }
            }
            e => app.on_event(&e),
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(w) = &self.window { w.request_redraw(); }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn run_native() {
    let mut opts = Options::default();
    let mut size = (1280u32, 720u32);
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let k = a.trim_start_matches('-').to_string();
        let v = if takes_value(&k) { args.next() } else { None };
        apply_arg(&mut opts, &mut size, &k, v);
    }
    let el = EventLoop::new().expect("イベントループを作れません");
    el.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut r = Runner::new(opts, size);
    el.run_app(&mut r).expect("終了に失敗");
}

/// ブラウザの入口
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn run_web() {
    console_error_panic_hook::set_once();
    let mut opts = Options::default();
    let mut size = (1280u32, 720u32);
    for (k, v) in crate::platform::web::query() {
        if k == "simbench" { sim_bench(); return; }
        // ?safe= は共通部品（game_ui::page::safe_area）が読む
        if k == "safe" { continue; }
        apply_arg(&mut opts, &mut size, &k, if v.is_empty() { None } else { Some(v) });
    }
    wasm_bindgen_futures::spawn_local(async move {
        // 字体と素材は同期で読めないので、先に全部取る
        let mut files = crate::ui::font_files();
        files.extend(crate::assets::startup_files());
        crate::assets::preload(&files).await;
        use winit::platform::web::EventLoopExtWebSys;
        let el = EventLoop::new().expect("イベントループを作れません");
        el.set_control_flow(winit::event_loop::ControlFlow::Wait);
        el.spawn_app(Runner::new(opts, size));
    });
}

/// 戦いの計算だけの速さを測る（rust/sim/src/bin/bench.rs と同じ手順。結果は window.__gyogun_result）
#[cfg(target_arch = "wasm32")]
fn sim_bench() {
    use sim::battle::{create_battle, BattleEvent, Phase};
    use sim::bot::create_bot;
    let dt = 1.0 / 60.0;
    let mut rounds = vec![];
    for _ in 0..3 {
        let (mut ms, mut all) = (0.0, 0u64);
        for i in 0..8u32 {
            let seed = 100 + i;
            let mut b = create_battle(seed, false);
            let mut bot = create_bot(&b, 1.0, seed + 1000);
            let mut steps = 0u64;
            let t0 = crate::platform::Instant::now();
            while steps < 60 * 60 * 15 {
                let input = bot.think(&b, dt);
                b.step(dt, input);
                let mut k = 0;
                while k < b.events.len() {
                    if matches!(b.events[k], BattleEvent::Defeat) { b.revive(); }
                    k += 1;
                }
                b.events.clear();
                steps += 1;
                if b.phase == Phase::Victory && b.victory_t > 4.0 { break; }
            }
            ms += t0.elapsed().as_secs_f64() * 1000.0;
            all += steps;
        }
        rounds.push(serde_json::json!({ "steps": all, "total_ms": ms, "us_per_step": ms * 1000.0 / all as f64 }));
    }
    let text = serde_json::json!({ "runtime": "rust-wasm-sim", "rounds": rounds }).to_string();
    if let Some(w) = web_sys::window() { let _ = js_sys::Reflect::set(&w, &"__gyogun_result".into(), &text.as_str().into()); }
}
