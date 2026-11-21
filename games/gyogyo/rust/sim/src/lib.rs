//! 魚々娘の遊びの計算（描画から独立）。かけっこグランプリ（keiba）から写した（2026-11-17）。
//! 2026-11-12 にマーク・武器（items）と育成（career）を足したので、同じ種でも TypeScript 版とは違う結果になる。
pub mod ai;
pub mod career;
pub mod cup;
pub mod hero_signals;
pub mod items;
pub mod menu;
mod pow_table;
pub mod race;
pub mod result;
pub mod track;
