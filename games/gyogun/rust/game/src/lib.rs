// 魚群繚乱の Rust 版（docs/rust-port.md）。もとの TypeScript 版の src/ と同じ名前のモジュールに分ける（旧版は 2026-11-10 に削除）。
#![recursion_limit = "512"]
pub use character_actor::{anim, delivery, glb, raster};
pub mod app;
pub mod assets;
pub mod controls;
#[cfg(not(target_arch = "wasm32"))]
pub mod audio;
#[cfg(target_arch = "wasm32")]
#[path = "audio_web.rs"]
pub mod audio;
#[cfg(target_arch = "wasm32")]
#[path = "media_web.rs"]
pub mod media;
#[cfg(target_arch = "wasm32")]
pub mod game_api;
pub mod geom;
pub mod gfx;
pub mod hud;
pub mod input;
pub mod platform;
pub mod runner;
pub mod stage;
pub mod ui;
pub mod hero {
    pub use character_actor::{actor, hurt_face, motion};
    pub mod hero;
}
pub mod view {
    pub mod boss_fx;
    pub mod boss_view;
    pub mod horde;
    pub mod candy;
    pub mod fish;
    pub mod items;
    pub mod materials;
    pub mod mirrors;
    pub mod rank_blade;
    pub mod sky;
    pub mod vfx;
    pub mod world;
}
