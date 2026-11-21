// three.js の代わりの小さな土台（浮島のゲーム共通）。Chrome の WebGPU 専用。使い方と決めごとは README.md。
pub mod device;
pub mod geometry;
pub mod glb;
pub mod material;
pub mod math;
pub mod render;
pub mod scene;
pub mod texture;
#[cfg(target_arch = "wasm32")]
pub mod app;

pub use material::{Blending, CustomShader, GeoId, Lambert, MatId, MatKind, Material, Phong, Physical, Side, Soft, Standard, TexId, UvTransform};
pub use math::Color;
pub use render::{Aa, Bloom, FINAL_DIRECT, FogExp2, FogLinear, Gpu, KIT_COMMON, Kit, KitActor, Output, OverlayQuad, RenderOpts, Shared, Transmission, ViewDraw};
pub use scene::{Camera, RawView, Skeleton, SkeletonId, DirLight, HemiLight, Node, NodeId, NodeKind, PointLight, Rot, Scene, Shadow, SpotLight, Target};
