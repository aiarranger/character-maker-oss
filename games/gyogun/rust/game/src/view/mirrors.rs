// 飴鏡: 戦場の大通りの両脇に立つ、透ける飴の板（反射壁）。斜めの光の帯がゆっくり流れ、弾が当たると光る。
use crate::geom::{self, Geo};
use crate::gfx::{fx_layout, hex, vert_layout, FxInst, Gfx, MeshBuf, Pipe, BLEND_NORMAL};
use crate::view::materials::ORDER_OPAQUE;
use glam::{Mat4, Vec3};
use sim::battle::{MIRRORS, MIRROR_H as MIRROR_H64, MIRROR_T as MIRROR_T64};
use std::f32::consts::PI;
use wgpu::util::DeviceExt;

pub const SHADER: &str = include_str!("../../shaders/mirrors.wgsl");
const MIRROR_H: f32 = MIRROR_H64 as f32;
const MIRROR_T: f32 = MIRROR_T64 as f32;

struct View { time: f32, flash: f32, hit: [f32; 2], model: Mat4 }

/// 当たった点を板の上の座標（0〜1）へ
fn hit_uv(mx: f32, mz: f32, a: f32, len: f32, x: f32, y: f32, z: f32) -> [f32; 2] {
    let (ux, uz) = (a.sin(), a.cos());
    let along = (x - mx) * ux + (z - mz) * uz;
    [0.5 + along / len, (y / MIRROR_H).clamp(0.0, 1.0)]
}

fn buf(device: &wgpu::Device, g: &Geo) -> MeshBuf {
    MeshBuf {
        vb: device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&g.v), usage: wgpu::BufferUsages::VERTEX }),
        ib: device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&g.i), usage: wgpu::BufferUsages::INDEX }),
        n: g.i.len() as u32,
    }
}

pub struct Mirrors {
    views: Vec<View>,
    data: Vec<FxInst>,
    inst: wgpu::Buffer,
    glass: Vec<MeshBuf>,
    /// 金の柱・つまみ・桟をまとめたもの（世界の座標）
    gold: MeshBuf,
    /// 影を落とす柱だけ（世界の座標）
    posts: MeshBuf,
    p_glass_back: wgpu::RenderPipeline,
    p_glass_front: wgpu::RenderPipeline,
    p_gold: wgpu::RenderPipeline,
    p_shadow: wgpu::RenderPipeline,
}

impl Mirrors {
    pub fn new(gfx: &Gfx) -> Self {
        let shader = gfx.shader("mirrors", SHADER);
        Self::build(&gfx.device, &gfx.globals_layout, &shader)
    }

    pub(crate) fn build(device: &wgpu::Device, globals: &wgpu::BindGroupLayout, shader: &wgpu::ShaderModule) -> Self {
        let layouts = [globals];
        let vbufs = [vert_layout(), fx_layout()];
        // three は透ける両面の材質を裏 → 表の 2 回で描く。箱の面の重なりが変わらないよう同じにする
        let mut back = Pipe::scene("mirror glass back", shader, "vs_glass", "fs_glass", &layouts, &vbufs).transparent(BLEND_NORMAL);
        back.cull = Some(wgpu::Face::Front);
        let p_glass_back = back.build(device);
        let p_glass_front = Pipe::scene("mirror glass front", shader, "vs_glass", "fs_glass", &layouts, &vbufs).transparent(BLEND_NORMAL).build(device);
        let p_gold = Pipe::scene("mirror gold", shader, "vs_gold", "fs_gold", &layouts, &vbufs).build(device);
        let shadow_vbufs = [vert_layout()];
        let p_shadow = Pipe::shadow("mirror shadow", shader, "vs_shadow", &layouts, &shadow_vbufs).build(device);

        let mut views = Vec::new();
        let mut glass = Vec::new();
        let (mut gold, mut posts) = (Geo::default(), Geo::default());
        for m in MIRRORS.iter() {
            let (mx, mz, ma, mlen) = (m.x as f32, m.z as f32, m.a as f32, m.len as f32);
            // 沿う向き (sin a, cos a) を板の長さの向き（ローカル x）に合わせる
            let group = Mat4::from_translation(Vec3::new(mx, 0.0, mz)) * Mat4::from_rotation_y(ma - PI / 2.0);
            glass.push(buf(device, &geom::boxg(mlen, MIRROR_H, MIRROR_T * 2.0)));
            views.push(View { time: 0.0, flash: 0.0, hit: [0.5, 0.5], model: group * Mat4::from_translation(Vec3::new(0.0, MIRROR_H / 2.0, 0.0)) });
            // 両端の柱と上の桟（金の飴細工）
            for sx in [-1.0f32, 1.0] {
                let post = geom::cylinder(0.32, 0.4, MIRROR_H + 0.6, 12, false).transform(group * Mat4::from_translation(Vec3::new(sx * mlen / 2.0, (MIRROR_H + 0.6) / 2.0, 0.0)));
                gold.append(&post);
                posts.append(&post);
                gold.append(&geom::sphere(0.42, 14, 10).transform(group * Mat4::from_translation(Vec3::new(sx * mlen / 2.0, MIRROR_H + 0.75, 0.0))));
            }
            gold.append(&geom::boxg(mlen, 0.22, MIRROR_T * 2.0 + 0.12).transform(group * Mat4::from_translation(Vec3::new(0.0, MIRROR_H + 0.05, 0.0))));
        }
        // 板のあとに金の材質を 1 個（色・roughness / 発光 × 強さ・metalness）
        let (c, e) = (hex("#e8a64a"), hex("#5a2a08"));
        let gold_inst = FxInst { model: Mat4::IDENTITY.to_cols_array_2d(), a: [c[0], c[1], c[2], 0.35], b: [e[0] * 0.4, e[1] * 0.4, e[2] * 0.4, 0.4], c: [0.0; 4], d: [0.0; 4] };
        let mut data: Vec<FxInst> = views.iter().map(|v| Self::glass_inst(v)).collect();
        data.push(gold_inst);
        let inst = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&data), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST });
        Mirrors { views, data, inst, glass, gold: buf(device, &gold), posts: buf(device, &posts), p_glass_back, p_glass_front, p_gold, p_shadow }
    }

    fn glass_inst(v: &View) -> FxInst {
        FxInst { model: v.model.to_cols_array_2d(), a: [v.time, v.flash, v.hit[0], v.hit[1]], b: [0.0; 4], c: [0.0; 4], d: [0.0; 4] }
    }

    /// 弾が当たった: 当たった所から光が広がる
    pub fn flash(&mut self, i: usize, x: f32, y: f32, z: f32) {
        let (Some(v), Some(m)) = (self.views.get_mut(i), MIRRORS.get(i)) else { return };
        v.hit = hit_uv(m.x as f32, m.z as f32, m.a as f32, m.len as f32, x, y, z);
        v.flash = 1.0;
    }

    pub fn update(&mut self, gfx: &Gfx, dt: f32, time: f32) {
        self.step(dt, time);
        self.write(&gfx.queue);
    }
    pub(crate) fn step(&mut self, dt: f32, time: f32) {
        for (i, v) in self.views.iter_mut().enumerate() {
            v.time = time;
            v.flash = (v.flash - dt * 2.5).max(0.0);
            self.data[i] = Self::glass_inst(v);
        }
    }
    pub(crate) fn write(&self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.inst, 0, bytemuck::cast_slice(&self.data[..self.views.len()]));
    }

    /// 影の型（柱だけ）
    pub fn shadow(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.p_shadow);
        pass.set_vertex_buffer(0, self.posts.vb.slice(..));
        pass.set_index_buffer(self.posts.ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.posts.n, 0, 0..1);
    }

    pub fn draw(&self, _gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, order: i32) { self.draw_pass(pass, order); }

    pub(crate) fn draw_pass(&self, pass: &mut wgpu::RenderPass<'_>, order: i32) {
        if order == ORDER_OPAQUE {
            let gold_i = self.views.len() as u32;
            pass.set_pipeline(&self.p_gold);
            pass.set_vertex_buffer(0, self.gold.vb.slice(..));
            pass.set_vertex_buffer(1, self.inst.slice(..));
            pass.set_index_buffer(self.gold.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.gold.n, 0, gold_i..gold_i + 1);
        } else if order == 2 {
            pass.set_vertex_buffer(1, self.inst.slice(..));
            for (i, g) in self.glass.iter().enumerate() {
                let i = i as u32;
                pass.set_vertex_buffer(0, g.vb.slice(..));
                pass.set_index_buffer(g.ib.slice(..), wgpu::IndexFormat::Uint32);
                pass.set_pipeline(&self.p_glass_back);
                pass.draw_indexed(0..g.n, 0, i..i + 1);
                pass.set_pipeline(&self.p_glass_front);
                pass.draw_indexed(0..g.n, 0, i..i + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 当たった点が板のどこかを取り違えると、光が別の場所に出る
    #[test]
    fn hit_point_maps_onto_panel() {
        let (mx, mz, a, len) = (-12.0f32, 33.0f32, -0.55f32, 12.0f32);
        assert_eq!(hit_uv(mx, mz, a, len, mx, 1.5, mz), [0.5, 0.5]);
        // 沿う向きに長さの半分進むと端（u = 1）
        let h = hit_uv(mx, mz, a, len, mx + a.sin() * 6.0, 9.0, mz + a.cos() * 6.0);
        assert!((h[0] - 1.0).abs() < 1e-5);
        assert_eq!(h[1], 1.0, "高さは 0〜1 に収める");
    }
}
