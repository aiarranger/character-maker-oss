// NPC・汎用キャラを読んで動かす部品（凍結: aisns の @aiarranger/character-runtime-npc・-generic。character-runtime 0.2.5 の写し）。GPU を持たない。wasm32 と Mac で動く。
// 主役の character-actor とは別の crate（Owner 2026-11-12「種類ごとに分ける」。2026-10-01 に主役の作業で NPC の目が崩れた件）。主役の crate を直しても、ここの動きは変わらない。
// glb・anim・spring・mouth・zip は character-actor 0.1.14（2026-11-12）の写し。直すのは NPC・汎用の不具合のときだけ。
// 入口は package（read_character_file・assemble_package）と actor（NpcActor）。描くのは character-view-npc。
pub mod actor;
pub mod anim;
pub mod glb;
pub mod mouth;
pub mod package;
pub mod spring;
pub mod zip;

use bytemuck::{Pod, Zeroable};

/// 頂点 1 つ（GPU へ上げる形。頂点の並びの宣言は character-view-npc）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub sway: f32,
}
