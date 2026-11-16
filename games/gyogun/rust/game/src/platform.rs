// Mac で直接動かす版とブラウザの版で違うところ（ファイル・保存・通信）をここにまとめる。
pub use web_time::{Instant, SystemTime, UNIX_EPOCH};

// 確かめ用（?jsrand）: 見た目の乱数を Math.random にする。今の版と同じ仕掛けの乱数で比べるため
static JS_RANDOM: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn set_js_random(on: bool) { JS_RANDOM.store(on, std::sync::atomic::Ordering::Relaxed); }
#[inline]
pub fn js_random() -> Option<f32> {
    if !JS_RANDOM.load(std::sync::atomic::Ordering::Relaxed) { return None; }
    #[cfg(target_arch = "wasm32")]
    { Some(js_sys::Math::random() as f32) }
    #[cfg(not(target_arch = "wasm32"))]
    { None }
}

// ---- 自己ベストなどの保存 ----
// キーと書き方は今の版の localStorage と同じ（gyogun.hero・gyogun.weapon.<id>・gyogun.best・gyogun.bestChain・gyogun.honors.<id>）。Owner の保存を引き継ぐ
#[cfg(not(target_arch = "wasm32"))]
pub mod store {
    use serde_json::{Map, Value};
    use std::path::PathBuf;

    fn path() -> PathBuf {
        let dir = std::env::var("GYOGUN_DATA").map(PathBuf::from).unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
            PathBuf::from(home).join("Library/Application Support/gyogun")
        });
        let _ = std::fs::create_dir_all(&dir);
        dir.join("store.json")
    }
    fn load() -> Map<String, Value> {
        std::fs::read(path()).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|v| v.as_object().cloned()).unwrap_or_default()
    }
    pub fn get(k: &str) -> Option<String> { load().get(k).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())) }
    pub fn set(k: &str, v: &str) {
        let mut m = load();
        m.insert(k.to_string(), Value::String(v.to_string()));
        if let Ok(b) = serde_json::to_vec(&m) { let _ = std::fs::write(path(), b); }
    }
}

#[cfg(target_arch = "wasm32")]
pub mod store {
    fn ls() -> Option<web_sys::Storage> { web_sys::window()?.local_storage().ok()? }
    pub fn get(k: &str) -> Option<String> { ls()?.get_item(k).ok()? }
    pub fn set(k: &str, v: &str) { if let Some(s) = ls() { let _ = s.set_item(k, v); } }
}

/// Studio（/character-maker）の「魚群繚乱で遊ぶ」が送ったキャラの .character.zip（docs/gyogun-spec.md）。無ければ None。
/// ブラウザ: index.html が控え（Cache Storage）から読んで window.__gyogunStudioZip に置いた物。Mac 版: 環境変数 GYOGUN_STUDIO_ZIP のファイル（確かめ用）
#[cfg(target_arch = "wasm32")]
pub fn studio_zip() -> Option<Vec<u8>> {
    use wasm_bindgen::JsCast;
    let window: wasm_bindgen::JsValue = web_sys::window()?.into();
    let v = js_sys::Reflect::get(&window, &"__gyogunStudioZip".into()).ok()?;
    Some(v.dyn_into::<js_sys::Uint8Array>().ok()?.to_vec())
}
#[cfg(not(target_arch = "wasm32"))]
pub fn studio_zip() -> Option<Vec<u8>> { std::fs::read(std::env::var("GYOGUN_STUDIO_ZIP").ok().filter(|p| !p.is_empty())?).ok() }

/// ゲームの外のページを開く（タイトルの「みんなで」）。ブラウザは同じタブで移る。Mac 版は既定のブラウザで開き、URL を標準出力にも出す
#[cfg(target_arch = "wasm32")]
pub fn open_url(url: &str) { if let Some(w) = web_sys::window() { let _ = w.location().assign(url); } }
#[cfg(not(target_arch = "wasm32"))]
pub fn open_url(url: &str) {
    let url = native_url(url);
    println!("{url}");
    let _ = std::process::Command::new("open").arg(&url).spawn();
}
/// Mac で直接動かす版: 同じ場所のページ（/ で始まる）は、pnpm dev が配る場所（http://127.0.0.1:4284）のページにする
pub fn native_url(url: &str) -> String { if url.starts_with('/') { format!("http://127.0.0.1:4284{url}") } else { url.to_string() } }

/// performance.now() と同じ時計（ms）。今の版の dt は (今 - 前) / 1000 の倍精度
#[cfg(target_arch = "wasm32")]
pub fn clock_ms() -> f64 { web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0) }
#[cfg(not(target_arch = "wasm32"))]
pub fn clock_ms() -> f64 {
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

/// ディスクの控えの場所（~/Library/Caches/gyogun/）
#[cfg(not(target_arch = "wasm32"))]
pub fn cache_dir() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(p) = std::env::var("GYOGUN_CACHE") { return PathBuf::from(p); }
    match std::env::var("HOME") { Ok(h) => PathBuf::from(h).join("Library/Caches/gyogun"), Err(_) => std::env::temp_dir().join("gyogun-cache") }
}

// ---- ブラウザの通信 ----
#[cfg(target_arch = "wasm32")]
pub mod web {
    // ファイルを取る・まとめて取るのは共通部品（game_ui::page::fetch・fetch_all）

    /// 動けないことを画面に出す
    /// 今の版の input.ts と同じ: 既定の動きを止めるのはこのキーと右クリックのメニューだけ。
    /// winit の prevent_default は全部のキーを止めるので、再読み込み（⌘R・F5）や拡大（⌘+）まで効かなくなる
    pub fn keep_browser_keys() {
        use wasm_bindgen::{closure::Closure, JsCast};
        let Some(w) = web_sys::window() else { return };
        let keys = Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(|e: web_sys::KeyboardEvent| {
            if matches!(e.code().as_str(), "Space" | "ArrowUp" | "ArrowDown" | "ArrowLeft" | "ArrowRight" | "Tab") { e.prevent_default(); }
        });
        let _ = w.add_event_listener_with_callback("keydown", keys.as_ref().unchecked_ref());
        keys.forget();
        let menu = Closure::<dyn FnMut(web_sys::Event)>::new(|e: web_sys::Event| e.prevent_default());
        let _ = w.add_event_listener_with_callback("contextmenu", menu.as_ref().unchecked_ref());
        menu.forget();
    }

    pub fn fatal(msg: &str) {
        web_sys::console::error_1(&msg.into());
        if let Some(e) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("fatal")) {
            e.set_text_content(Some(msg));
            let _ = e.set_attribute("style", "display:flex");
        }
    }

    /// ページの URL の引数（?auto&seed=1 など）
    pub use character_actor::delivery::page_query as query;
}
