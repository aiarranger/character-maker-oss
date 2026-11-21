// 主役を読み、動かす部品（共通ルール「主役キャラとゲームの境目」の characters の側）。GPU を持たない。wasm32 と Mac のどちらでも動く。
// 入口は actor（load_character_actor・CharacterActor）と delivery（共有配布）、配布を通らないキャラ（.character.zip・.glb・NPC）は package（read_character_file・assemble_package）。描くのは character-view。
pub mod actor;
pub mod anim;
pub mod delivery;
pub mod glb;
pub mod hurt_face;
pub mod motion;
pub mod mouth;
pub mod package;
pub mod raster;
pub mod spring;
pub mod zip;

use bytemuck::{Pod, Zeroable};

/// 頂点 1 つ（GPU へ上げる形。頂点の並びの宣言は character-view）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Default)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
    pub sway: f32,
}
