//! 魚群繚乱の戦いの計算（描画から独立）。同じ種なら同じ結果になる（もとは TypeScript 版 `src/sim` の写し。旧版は 2026-11-10 に削除）。
pub mod aim_shape;
pub mod battle;
pub mod bot;
pub mod cheer;
pub mod hero_signals;
pub use sim_math as math;
pub mod moves;
pub mod trace;
