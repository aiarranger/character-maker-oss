// 空（夕暮れの階調・太陽・すじ雲・星）と、島の下に広がる雲の海。戦場の空気の色（theme.ts）もここに置く。
use crate::geom::{self, Geo, Vertex};
use crate::gfx::{self, Globals, Gfx, MeshBuf, Pipe, hex, lerp3};
use crate::view::materials::{ORDER_OPAQUE, RIM_I};
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

/// 戦場の空気（空・霧・光の色）。色は "#rrggbb" のまま持ち、使う時に線形へ直す
#[derive(Clone, Copy)]
pub struct Theme {
    pub zenith: &'static str,
    pub mid: &'static str,
    pub horizon: &'static str,
    pub sun: &'static str,
    /// 太陽の向き（空の中の位置）
    pub sun_dir: [f32; 3],
    pub fog: &'static str,
    pub fog_density: f32,
    pub hemi_sky: &'static str,
    pub hemi_ground: &'static str,
    pub hemi_i: f32,
    pub key: &'static str,
    pub key_i: f32,
    pub stars: f32,
    pub cloud: &'static str,
    pub sea: &'static str,
}

/// 夕暮れの浜辺（docs/gyogun-spec.md）: 桃橙の残照と、海の青い霧
pub const HAMA: Theme = Theme {
    zenith: "#33427e", mid: "#b878a4", horizon: "#ffb27a", sun: "#ffd8a0", sun_dir: [0.35, 0.05, -1.0],
    fog: "#c898a8", fog_density: 0.0042,
    hemi_sky: "#fff0e4", hemi_ground: "#a08078", hemi_i: 1.55, key: "#ffd8b4", key_i: 3.0,
    stars: 0.25, cloud: "#ff9a8a", sea: "#2a5a78",
};
/// 浜辺の大将: 日が沈みきる（藍の空と、海の上の赤い残照）。場は冷たく暗くして、灯と大将の暖色を立たせる
pub const HAMA_BOSS: Theme = Theme {
    zenith: "#101838", mid: "#4a3468", horizon: "#ff6a3a", sun: "#ff9a60", sun_dir: [0.0, 0.07, -1.0],
    fog: "#3a3458", fog_density: 0.0078,
    hemi_sky: "#9aa4e8", hemi_ground: "#4a3a48", hemi_i: 0.9, key: "#ffb080", key_i: 2.6,
    stars: 0.9, cloud: "#d0607a", sea: "#18304a",
};

const RIM_FIELD: &str = "#ffb48a";
const RIM_BOSS: &str = "#ffa060";

/// 線形へ直した Theme（app.ts の applyTheme の結果）
#[derive(Clone, Copy)]
pub struct LinearTheme {
    pub zenith: [f32; 3],
    pub mid: [f32; 3],
    pub horizon: [f32; 3],
    pub sun: [f32; 3],
    pub sun_dir: [f32; 3],
    pub fog: [f32; 3],
    pub fog_density: f32,
    pub hemi_sky: [f32; 3],
    pub hemi_ground: [f32; 3],
    pub hemi_i: f32,
    pub key: [f32; 3],
    pub key_i: f32,
    pub stars: f32,
    pub cloud: [f32; 3],
    pub sea: [f32; 3],
    /// SHARED.uRimColor
    pub rim: [f32; 3],
}

impl Theme {
    pub fn linear(&self) -> LinearTheme {
        LinearTheme {
            zenith: hex(self.zenith), mid: hex(self.mid), horizon: hex(self.horizon), sun: hex(self.sun), sun_dir: self.sun_dir,
            fog: hex(self.fog), fog_density: self.fog_density, hemi_sky: hex(self.hemi_sky), hemi_ground: hex(self.hemi_ground), hemi_i: self.hemi_i,
            key: hex(self.key), key_i: self.key_i, stars: self.stars, cloud: hex(self.cloud), sea: hex(self.sea), rim: hex(RIM_FIELD),
        }
    }
}

impl LinearTheme {
    /// 世界の値へ書く（hemi・key の光の色 × 強さ、霧、輪郭の光）
    pub fn apply(&self, g: &mut Globals) {
        let scaled = |c: [f32; 3], i: f32| [c[0] * i, c[1] * i, c[2] * i, 0.0];
        g.hemi_sky = scaled(self.hemi_sky, self.hemi_i);
        g.hemi_ground = scaled(self.hemi_ground, self.hemi_i);
        g.key_color = scaled(self.key, self.key_i);
        g.fog = [self.fog[0], self.fog[1], self.fog[2], self.fog_density];
        g.rim = [self.rim[0], self.rim[1], self.rim[2], RIM_I];
    }
}

/// three の getHexString と同じく、sRGB の 8 bit へ丸めてから線形へ戻す
fn through_hex(c: [f32; 3]) -> [f32; 3] {
    let to_srgb = |x: f32| if x < 0.0031308 { x * 12.92 } else { 1.055 * x.powf(0.41666) - 0.055 };
    c.map(|x| gfx::srgb((to_srgb(x) * 255.0).clamp(0.0, 255.0).round() / 255.0))
}

/// k = boss01。HAMA → HAMA_BOSS を線形で混ぜる。太陽の向き・海は始めの色のまま
pub fn lerp_theme(k: f32) -> LinearTheme {
    let (a, b) = (&HAMA, &HAMA_BOSS);
    let l = |x: &str, y: &str| through_hex(lerp3(hex(x), hex(y), k));
    LinearTheme {
        zenith: l(a.zenith, b.zenith), mid: l(a.mid, b.mid), horizon: l(a.horizon, b.horizon), sun: l(a.sun, b.sun), sun_dir: a.sun_dir,
        fog: l(a.fog, b.fog), fog_density: a.fog_density + (b.fog_density - a.fog_density) * k,
        hemi_sky: l(a.hemi_sky, b.hemi_sky), hemi_ground: l(a.hemi_ground, b.hemi_ground), hemi_i: a.hemi_i + (b.hemi_i - a.hemi_i) * k,
        key: l(a.key, b.key), key_i: a.key_i + (b.key_i - a.key_i) * k, stars: a.stars + (b.stars - a.stars) * k, cloud: l(a.cloud, b.cloud), sea: hex(a.sea),
        rim: hex(if k > 0.5 { RIM_BOSS } else { RIM_FIELD }),
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyU {
    zenith: [f32; 4],
    mid: [f32; 4],
    horizon: [f32; 4],
    sun: [f32; 4],
    cloud: [f32; 4],
    fog: [f32; 4],
    sun_dir: [f32; 4],
    sea: [f32; 4],
    light: [f32; 4],
    cam: [f32; 4],
}

fn v4(c: [f32; 3], w: f32) -> [f32; 4] { [c[0], c[1], c[2], w] }

/// CircleGeometry(r, seg).rotateX(-π / 2)
fn circle_flat(r: f32, seg: u32, y: f32) -> Geo {
    let mut g = Geo::default();
    let v = |x: f32, z: f32| Vertex { pos: [x, y, z], normal: [0.0, 1.0, 0.0], uv: [0.0, 0.0], color: [1.0; 4], sway: 0.0 };
    g.v.push(v(0.0, 0.0));
    for i in 0..=seg {
        let a = i as f32 / seg as f32 * std::f32::consts::TAU;
        g.v.push(v(a.cos() * r, -a.sin() * r));
    }
    for i in 1..=seg { g.i.extend_from_slice(&[i, i + 1, 0]); }
    g
}

pub struct Sky {
    p_sky: wgpu::RenderPipeline,
    p_sea: wgpu::RenderPipeline,
    sky_mesh: MeshBuf,
    sea_mesh: MeshBuf,
    buf: wgpu::Buffer,
    bg: wgpu::BindGroup,
    u: SkyU,
    /// 海（app.ts は visible = false にしているので、既定も false）
    pub sea_visible: bool,
}

impl Sky {
    pub fn new(gfx: &Gfx) -> Self {
        let shader = gfx.shader("sky", include_str!("../../shaders/sky.wgsl"));
        let layout = gfx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("sky"), entries: &[wgpu::BindGroupLayoutEntry {
            binding: 5, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
        }] });
        let mut u = SkyU::zeroed();
        u.sun_dir = [0.0, 0.1, -1.0, 0.0];
        u.fog = v4(hex("#b78fae"), 0.0);
        let buf = gfx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("sky"), contents: bytemuck::bytes_of(&u), usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST });
        let bg = gfx.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("sky"), layout: &layout, entries: &[wgpu::BindGroupEntry { binding: 5, resource: buf.as_entire_binding() }] });
        let layouts = [&gfx.globals_layout, &layout];
        let vbufs = [gfx::vert_layout()];
        // BackSide・depthWrite: false。遠くの面（xyww）を描くので three の LessEqual が要る
        let p_sky = Pipe { cull: Some(wgpu::Face::Front), depth_write: false, depth: wgpu::CompareFunction::LessEqual, ..Pipe::scene("sky", &shader, "vs_sky", "fs_sky", &layouts, &vbufs) }.build(&gfx.device);
        let p_sea = Pipe::scene("sea", &shader, "vs_sea", "fs_sea", &layouts, &vbufs).build(&gfx.device);
        Sky {
            p_sky, p_sea,
            sky_mesh: gfx.mesh(&geom::sphere(700.0, 48, 24)),
            sea_mesh: gfx.mesh(&circle_flat(600.0, 64, -34.0)),
            buf, bg, u, sea_visible: false,
        }
    }

    pub fn set_theme(&mut self, t: &Theme, lit: f32, dark: f32) { self.set_linear(&t.linear(), lit, dark); }

    /// lerp_theme の結果を入れる（app.ts は sky.setTheme(t, 0, 0)）
    pub fn set_linear(&mut self, t: &LinearTheme, lit: f32, dark: f32) {
        let u = &mut self.u;
        u.zenith = v4(t.zenith, u.zenith[3]);
        u.mid = v4(lerp3(t.mid, t.horizon, lit * 0.15), t.stars);
        u.horizon = v4(t.horizon, dark);
        u.sun = v4(t.sun, 0.0);
        u.cloud = v4(t.cloud, 0.0);
        u.sun_dir = v4(Vec3::from(t.sun_dir).normalize().to_array(), 0.0);
        u.sea = v4(t.sea, 0.0);
        u.light = v4(lerp3(t.cloud, t.horizon, 0.3), 0.0);
    }

    /// 地平のかすみの色（霧の色に合わせる。線形）
    pub fn set_fog(&mut self, c: [f32; 3]) { self.u.fog = v4(c, 0.0); }

    pub fn update(&mut self, gfx: &Gfx, time: f32, cam: Vec3) {
        self.u.zenith[3] = time;
        self.u.cam = v4(cam.to_array(), 0.0);
        gfx.queue.write_buffer(&self.buf, 0, bytemuck::bytes_of(&self.u));
    }

    /// 空。three は深度を書かず最初に描く（renderOrder -10）ので、ORDER_OPAQUE の先頭で呼ぶ
    pub fn draw_sky(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.p_sky);
        pass.set_bind_group(1, &self.bg, &[]);
        pass.set_vertex_buffer(0, self.sky_mesh.vb.slice(..));
        pass.set_index_buffer(self.sky_mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.sky_mesh.n, 0, 0..1);
    }

    /// 海（空は draw_sky）
    pub fn draw(&self, _gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, order: i32) {
        if order != ORDER_OPAQUE || !self.sea_visible { return; }
        pass.set_pipeline(&self.p_sea);
        pass.set_bind_group(1, &self.bg, &[]);
        pass.set_vertex_buffer(0, self.sea_mesh.vb.slice(..));
        pass.set_index_buffer(self.sea_mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.sea_mesh.n, 0, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_theme_matches_ends() {
        // 8 bit へ丸めても、両端は元の色のまま
        let (a, b) = (lerp_theme(0.0), lerp_theme(1.0));
        for (x, y) in [(a.zenith, hex(HAMA.zenith)), (a.fog, hex(HAMA.fog)), (b.horizon, hex(HAMA_BOSS.horizon)), (b.cloud, hex(HAMA_BOSS.cloud))] {
            for i in 0..3 { assert!((x[i] - y[i]).abs() < 1e-4, "{x:?} {y:?}"); }
        }
        // 太陽の向きは HAMA のまま、星と平行光の強さは HAMA_BOSS へ、rim は半分で切り替わる
        assert_eq!(b.sun_dir, HAMA.sun_dir);
        assert_eq!(b.stars, HAMA_BOSS.stars);
        assert_eq!(b.key_i, HAMA_BOSS.key_i);
        assert_eq!(lerp_theme(0.5).rim, hex(RIM_FIELD));
        assert_eq!(lerp_theme(0.51).rim, hex(RIM_BOSS));
    }
}
