// 島・小物の材質: ランバートを改造したやわらかい陰影（回り込み）＋輪郭の光＋草木の揺れ。
// three は材質ごとに uniform を持つが、ここは 1 個ごとの値（FxInst）にして、同じ型で描く。
// uDim・uLamps はこのゲームでは効かない（uDim = 0）ので持たない。uTime・uRimColor・uRimI は common.wgsl の g にある。
use crate::gfx::{self, FxInst, Gfx, MeshBuf, Pipe};
use glam::Mat4;
use std::ops::Range;

/// SHARED.uRimI
pub const RIM_I: f32 = 0.35;

/// softMaterial の選択肢（色は線形。gfx::hex で直す）
#[derive(Clone, Copy)]
pub struct SoftParams {
    pub color: [f32; 3],
    /// 色 × emissiveIntensity
    pub emissive: [f32; 3],
    pub rim: f32,
    pub receive_shadow: bool,
    pub opacity: f32,
    /// 改造前の MeshLambertMaterial（dotNL は saturate）。輪郭の光は rim: 0 で消す。拾い物（あんまん・金平糖）用
    pub plain_lambert: bool,
}
impl Default for SoftParams {
    fn default() -> Self {
        SoftParams { color: [1.0; 3], emissive: [0.0; 3], rim: 1.0, receive_shadow: false, opacity: 1.0, plain_lambert: false }
    }
}
impl SoftParams {
    pub fn inst(&self, m: Mat4) -> FxInst {
        let (c, e) = (self.color, self.emissive);
        FxInst {
            model: m.to_cols_array_2d(),
            a: [c[0], c[1], c[2], self.rim],
            b: [e[0], e[1], e[2], if self.receive_shadow { 1.0 } else { 0.0 }],
            c: [self.opacity, if self.plain_lambert { 1.0 } else { 0.0 }, 0.0, 0.0],
            d: [0.0; 4],
        }
    }
}

/// soft の型。頂点は geom::Vertex（slot 0）、1 個ごとの値は FxInst（slot 1）。
/// 絵なし（map: null）は white を、絵ありは gfx.tex_bind の物を group 2 へ。
pub struct Soft {
    /// FrontSide
    pub front: wgpu::RenderPipeline,
    /// DoubleSide
    pub double: wgpu::RenderPipeline,
    /// transparent: true（深度を書く）
    pub fade: wgpu::RenderPipeline,
    /// transparent: true（depthWrite: false）
    pub fade_no_depth: wgpu::RenderPipeline,
    /// 影の型（three と同じく FrontSide の物は裏面を描く）
    pub shadow: wgpu::RenderPipeline,
    pub white: wgpu::BindGroup,
}

impl Soft {
    pub fn new(gfx: &Gfx) -> Self {
        let shader = gfx.shader("soft", include_str!("../../shaders/soft.wgsl"));
        let layouts = [&gfx.globals_layout, &gfx.shadow_layout, &gfx.tex_layout];
        let vbufs = [gfx::vert_layout(), gfx::fx_layout()];
        let scene = |label| Pipe::scene(label, &shader, "vs_soft", "fs_soft", &layouts, &vbufs);
        let shadow_layouts = [&gfx.globals_layout];
        Soft {
            front: scene("soft").build(&gfx.device),
            double: scene("soft double").double().build(&gfx.device),
            fade: Pipe { depth_write: true, ..scene("soft fade").transparent(gfx::BLEND_NORMAL) }.build(&gfx.device),
            fade_no_depth: scene("soft fade no depth").transparent(gfx::BLEND_NORMAL).build(&gfx.device),
            shadow: Pipe { cull: Some(wgpu::Face::Front), ..Pipe::shadow("soft shadow", &shader, "vs_soft_shadow", &shadow_layouts, &vbufs) }.build(&gfx.device),
            white: gfx.tex_bind(&gfx.white),
        }
    }

    /// 影（group 1）と絵なし（group 2）を設定する
    pub fn bind<'a>(&'a self, gfx: &'a Gfx, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(1, &gfx.shadow_bg, &[]);
        pass.set_bind_group(2, &self.white, &[]);
    }
}

/// mesh を buf の FxInst の range 番目だけインスタンス描画する（描く型は呼ぶ側が設定済み）
pub fn draw_fx(pass: &mut wgpu::RenderPass<'_>, mesh: &MeshBuf, buf: &wgpu::Buffer, range: Range<u32>) {
    if range.is_empty() { return; }
    let size = std::mem::size_of::<FxInst>() as u64;
    pass.set_vertex_buffer(0, mesh.vb.slice(..));
    pass.set_vertex_buffer(1, buf.slice(range.start as u64 * size..range.end as u64 * size));
    pass.set_index_buffer(mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..mesh.n, 0, 0..range.len() as u32);
}

/// three の renderOrder に当たる描く順
pub const ORDER_OPAQUE: i32 = i32::MIN;
pub const ORDERS: [i32; 11] = [-10, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
