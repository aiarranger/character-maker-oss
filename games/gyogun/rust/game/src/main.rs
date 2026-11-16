// 魚群繚乱の Rust 版（Mac で直接動かす版）の入口。ブラウザ版の入口は runner.rs の run_web。
#[cfg(not(target_arch = "wasm32"))]
fn main() { gyogun_game::runner::run_native(); }
#[cfg(target_arch = "wasm32")]
fn main() {}
