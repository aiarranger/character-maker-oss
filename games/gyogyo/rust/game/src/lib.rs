#![recursion_limit = "512"]
// 魚々娘の画面（Rust 版。かけっこグランプリの写し）。入口は app.rs（画面の流れ）。ui.rs が画面の文字・ボタン（egui。HUD は hud.rs、一時停止は pause.rs）、audio.rs が音、controls.rs が入力、save.rs が保存、view/ が 3D の見た目、game_api.rs が確かめ用の window.__game。
#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod audio;
#[cfg(target_arch = "wasm32")]
pub mod character_delivery;
#[cfg(target_arch = "wasm32")]
mod controls;
#[cfg(target_arch = "wasm32")]
mod game_api;
#[cfg(target_arch = "wasm32")]
mod hud;
#[cfg(target_arch = "wasm32")]
mod pause;
#[cfg(target_arch = "wasm32")]
mod save;
#[cfg(target_arch = "wasm32")]
mod ui;
#[cfg(target_arch = "wasm32")]
pub mod view;
#[path = "view/squid_shape.rs"] pub mod squid_shape; // イカの形と動きのブラウザに頼らない部分（ネイティブで試験する。view/squid.rs が使う）

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    use wasm_bindgen::JsCast;
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        if let Err(e) = app::run().await {
            web_sys::console::error_1(&e.clone().into());
            // 失敗の表示は index.html（#fatal と「読み込み直す」）。主役の一覧が読めないときは、Rust の読み込みの画面が知らせの文と「読み込み直す」を出す（app.rs の fail）
            if let Some(w) = web_sys::window() && let Ok(f) = js_sys::Reflect::get(&w, &"__gyogyoFatal".into()) && let Ok(f) = f.dyn_into::<js_sys::Function>() { let _ = f.call1(&wasm_bindgen::JsValue::NULL, &format!("読み込みに失敗しました: {e}").into()); }
        }
    });
}
