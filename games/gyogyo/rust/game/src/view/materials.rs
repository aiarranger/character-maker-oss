// 島・小物の材質。式は island-kit の Soft（kit.wgsl）。SHARED は kit.shared。src/view/materials.ts の写し。
use island_kit::material::Soft;
use island_kit::{Blending, Color, Kit, MatId, MatKind, Material, Side, TexId};
use std::cell::Cell;

#[derive(Clone, Copy)]
pub struct SoftOptions {
    pub color: Color,
    pub map: Option<TexId>,
    pub vertex_colors: bool,
    pub sway: bool,
    pub emissive: Color,
    pub emissive_intensity: f32,
    pub side: Side,
    pub rim: f32,
    pub flat_shading: bool,
}
impl Default for SoftOptions {
    fn default() -> Self { SoftOptions { color: Color::WHITE, map: None, vertex_colors: false, sway: false, emissive: Color::BLACK, emissive_intensity: 1.0, side: Side::Front, rim: 1.0, flat_shading: false } }
}

pub fn soft_material(o: SoftOptions) -> Material {
    Material {
        kind: MatKind::Soft(Soft { emissive: o.emissive, emissive_intensity: o.emissive_intensity, rim: o.rim, sway: o.sway, flat_shading: o.flat_shading }),
        color: o.color, map: o.map, vertex_colors: o.vertex_colors, side: o.side,
        ..Material::soft()
    }
}

thread_local! {
    static BLOB: Cell<Option<TexId>> = const { Cell::new(None) };
    static GLOW: Cell<Option<TexId>> = const { Cell::new(None) };
}

/// 雲・泡の影などに使う、柔らかい丸の濃淡テクスチャ
pub fn blob_texture(kit: &mut Kit) -> TexId {
    if let Some(t) = BLOB.get() { return t; }
    let n = 64usize;
    let mut d = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let r = (x as f64 + 0.5 - n as f64 / 2.0).hypot(y as f64 + 0.5 - n as f64 / 2.0) / (n as f64 / 2.0);
            let k = ((r - 0.3) / 0.7).clamp(0.0, 1.0);
            let i = (y * n + x) * 4;
            d[i] = 255; d[i + 1] = 255; d[i + 2] = 255;
            d[i + 3] = ((1.0 - k * k * (3.0 - 2.0 * k)) * 255.0 + 0.5).floor() as u8;
        }
    }
    let t = kit.data_texture(n as u32, n as u32, &d, false);
    BLOB.set(Some(t));
    t
}

/// 地面を暗くする丸影（乗算）。1 つの材質を全員で使い、濃さは scale で変える
pub fn blob_shadow_material(kit: &mut Kit, strength: f32) -> MatId {
    let map = blob_texture(kit);
    kit.material(Material {
        color: Color::rgb(0.32, 0.26, 0.4).lerp(Color::rgb(1.0, 1.0, 1.0), 1.0 - strength),
        blending: Blending::Multiply, premultiplied_alpha: true, polygon_offset: Some((-2.0, 0.0)), fog: false,
        ..Material::basic().with_map(map).see_through()
    })
}

/// 光のにじみ（加算）の丸
pub fn glow_texture(kit: &mut Kit) -> TexId {
    if let Some(t) = GLOW.get() { return t; }
    let t = kit.canvas_texture(128, 128, true, |g| {
        let gr = g.create_radial_gradient(64.0, 64.0, 0.0, 64.0, 64.0, 64.0).unwrap();
        let _ = gr.add_color_stop(0.0, "rgba(255,255,255,1)");
        let _ = gr.add_color_stop(0.18, "rgba(255,255,255,0.75)");
        let _ = gr.add_color_stop(0.45, "rgba(255,255,255,0.22)");
        let _ = gr.add_color_stop(1.0, "rgba(255,255,255,0)");
        g.set_fill_style_canvas_gradient(&gr);
        g.fill_rect(0.0, 0.0, 128.0, 128.0);
    });
    GLOW.set(Some(t));
    t
}
