// 空（夕暮れの階調・太陽・すじ雲・星）と、島の下に広がる雲の海。src/view/sky.ts の写し。シェーダーは shaders/sky.wgsl・sea.wgsl。
use super::theme::Theme;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use island_kit::geometry::{circle, sphere};
use island_kit::{Color, CustomShader, Kit, MatId, Material, NodeId, Side};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SkyU { zenith: [f32; 4], mid: [f32; 4], horizon: [f32; 4], sun: [f32; 4], cloud: [f32; 4], fog: [f32; 4], sun_dir: [f32; 4], p: [f32; 4] }
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SeaU { sea: [f32; 4], light: [f32; 4], sun: [f32; 4], sun_dir: [f32; 4], p: [f32; 4] }

fn c4(c: Color) -> [f32; 4] { [c.r, c.g, c.b, 0.0] }

pub struct Sky { pub sky: NodeId, pub sea: NodeId, sky_mat: MatId, sea_mat: MatId, sky_u: SkyU, sea_u: SeaU }

impl Sky {
    pub fn new(kit: &mut Kit) -> Sky {
        let sky_u = SkyU { zenith: [0.0; 4], mid: [0.0; 4], horizon: [0.0; 4], sun: [0.0; 4], cloud: [0.0; 4], fog: c4(Color::css("#b78fae")), sun_dir: [0.0, 0.1, -1.0, 0.0], p: [0.0; 4] };
        let geo = kit.geometry(&sphere(700.0, 48, 24));
        let sky_mat = kit.material(Material {
            side: Side::Back, depth_write: false,
            ..Material::custom(CustomShader { label: "sky".into(), wgsl: include_str!("../../shaders/sky.wgsl").into(), uniform: bytemuck::bytes_of(&sky_u).to_vec(), ..Default::default() })
        });
        let sky = kit.scene.mesh(None, geo, sky_mat);
        kit.scene[sky].render_order = -10;
        kit.scene[sky].frustum_culled = false;

        let sea_u = SeaU { sea: [0.0; 4], light: [0.0; 4], sun: [0.0; 4], sun_dir: sky_u.sun_dir, p: [0.0; 4] };
        let geo = kit.geometry(&circle(600.0, 64).rotate_x(-std::f32::consts::FRAC_PI_2).owned());
        let sea_mat = kit.material(Material::custom(CustomShader { label: "sea".into(), wgsl: include_str!("../../shaders/sea.wgsl").into(), uniform: bytemuck::bytes_of(&sea_u).to_vec(), ..Default::default() }));
        let sea = kit.scene.mesh(None, geo, sea_mat);
        kit.scene[sea].position.y = -34.0;
        Sky { sky, sea, sky_mat, sea_mat, sky_u, sea_u }
    }

    pub fn set_theme(&mut self, t: &Theme) {
        let u = &mut self.sky_u;
        u.zenith = c4(t.zenith);
        u.mid = c4(t.mid);
        u.horizon = c4(t.horizon);
        u.sun = c4(t.sun);
        u.cloud = c4(t.cloud);
        let d = Vec3::from(t.sun_dir).normalize();
        u.sun_dir = [d.x, d.y, d.z, 0.0];
        u.p[1] = t.stars;
        u.p[2] = 0.0;
        let s = &mut self.sea_u;
        s.sea = c4(t.sea);
        s.light = c4(t.cloud.lerp(t.horizon, 0.3));
        s.sun = c4(t.sun);
        s.sun_dir = u.sun_dir;
    }

    /// 地平のかすみの色（霧の色に合わせる）
    pub fn set_fog(&mut self, c: Color) { self.sky_u.fog = c4(c); }

    pub fn update(&mut self, kit: &mut Kit, time: f32) {
        self.sky_u.p[0] = time;
        self.sea_u.p[0] = time;
        kit.scene[self.sky].position = kit.camera.position;
        kit.set_uniform(self.sky_mat, bytemuck::bytes_of(&self.sky_u));
        kit.set_uniform(self.sea_mat, bytemuck::bytes_of(&self.sea_u));
    }
}
