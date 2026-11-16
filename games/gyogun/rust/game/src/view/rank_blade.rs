// 格の光の刃: 武器の根元から先へ、格が上がるほど長く太い光の刃を重ねる。上がった瞬間は根元から先へ光が走り、刃がふくらむ。
use crate::geom::{self, Geo};
use crate::gfx::{fx_layout, hex, vert_layout, FxInst, Gfx, MeshBuf, Pipe, BLEND_ADD};
use glam::{Mat4, Quat, Vec3};
use std::f32::consts::PI;
use wgpu::util::DeviceExt;

pub const SHADER: &str = include_str!("../../shaders/vfx.wgsl");

/// 格ごとの刃の伸び（武器の長さに対する倍率）と太さ
const REACH: [f32; 4] = [0.0, 0.6, 1.0, 1.4];
const WIDTH: [f32; 4] = [0.0, 0.26, 0.34, 0.44];

/// TS は刃を Group.renderOrder = 3 にしている。three は Group の値を groupOrder として先に比べるので、
/// groupOrder 0 のほかの透明な物すべて（renderOrder 9 まで）のあとに描かれる。同じ結果になるよう最後の 9 で描く。
const ORDER_BLADE: i32 = 9;
/// 走る光は Group の外で renderOrder = 4
const ORDER_RUN: i32 = 4;

/// ふくらみ: 0.3 秒で 1.6 倍 → 1 倍へ
fn swell_at(surge_t: f32) -> f32 { if surge_t < 0.3 { 1.0 + 0.6 * (surge_t / 0.3 * PI).sin() } else { 1.0 } }
/// 刃の全長
fn total_len(len: f32, rank: usize, swell: f32) -> f32 { len * (1.0 + REACH[rank] * swell) }

fn buf(device: &wgpu::Device, g: &Geo) -> MeshBuf {
    MeshBuf {
        vb: device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&g.v), usage: wgpu::BufferUsages::VERTEX }),
        ib: device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&g.i), usage: wgpu::BufferUsages::INDEX }),
        n: g.i.len() as u32,
    }
}

pub struct RankBlade {
    surge_t: f32,
    tip_x: Vec3,
    visible: bool,
    run_visible: bool,
    core_color: [f32; 3],
    data: [FxInst; 3],
    inst: wgpu::Buffer,
    geo: MeshBuf,
    run: MeshBuf,
    p_outer: wgpu::RenderPipeline,
    p_front: wgpu::RenderPipeline,
}

impl RankBlade {
    pub fn new(gfx: &Gfx) -> Self {
        let shader = gfx.shader("rank blade", SHADER);
        Self::build(&gfx.device, &gfx.globals_layout, &shader)
    }

    pub(crate) fn build(device: &wgpu::Device, globals: &wgpu::BindGroupLayout, shader: &wgpu::ShaderModule) -> Self {
        let layouts = [globals];
        let vbufs = [vert_layout(), fx_layout()];
        let p_outer = Pipe::scene("blade outer", shader, "vs_blade", "fs_blade", &layouts, &vbufs).transparent(BLEND_ADD).double().build(device);
        let p_front = Pipe::scene("blade core", shader, "vs_blade", "fs_blade", &layouts, &vbufs).transparent(BLEND_ADD).build(device);
        let zero = FxInst { model: Mat4::IDENTITY.to_cols_array_2d(), a: [0.0; 4], b: [0.0; 4], c: [0.0; 4], d: [0.0; 4] };
        let data = [zero; 3];
        RankBlade {
            surge_t: 9.0,
            tip_x: Vec3::ZERO,
            visible: false,
            run_visible: false,
            core_color: hex("#fff6e0"),
            data,
            inst: device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&data), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST }),
            geo: buf(device, &geom::cylinder(1.0, 0.35, 1.0, 14, true).translate(0.0, 0.5, 0.0)),
            // 走る光: 根元から先へ動く明るい玉
            run: buf(device, &geom::sphere(1.0, 12, 8)),
            p_outer,
            p_front,
        }
    }

    /// 格が上がった瞬間（光が走って刃がふくらむ）
    pub fn surge(&mut self) { self.surge_t = 0.0; }
    /// 刃の先（光の刃の先端。剣の軌跡もここまで伸ばす）
    pub fn tip(&self) -> Vec3 { self.tip_x }

    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, gfx: &Gfx, base: Vec3, tip: Vec3, rank: i32, time: f32, dt: f32, color: [f32; 3], show: bool) {
        self.step(base, tip, rank, time, dt, color, show);
        self.write(&gfx.queue);
    }
    pub(crate) fn write(&self, queue: &wgpu::Queue) {
        if self.visible { queue.write_buffer(&self.inst, 0, bytemuck::cast_slice(&self.data)); }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn step(&mut self, base: Vec3, tip: Vec3, rank: i32, time: f32, dt: f32, color: [f32; 3], show: bool) {
        self.surge_t += dt;
        let d = tip - base;
        let len = d.length();
        if len < 1e-4 || rank <= 0 || !show {
            self.visible = false;
            self.run_visible = false;
            self.tip_x = tip;
            return;
        }
        let dir = d / len;
        let r = (rank as usize).min(3);
        // ふくらみ: 0.3 秒で 1.6 倍 → 1 倍へ。参は脈打つ
        let swell = swell_at(self.surge_t);
        let pulse = if rank >= 3 { 1.0 + 0.12 * (time * 9.0).sin() } else { 1.0 };
        let total = total_len(len, r, swell);
        self.tip_x = base + dir * total;
        self.visible = true;
        let group = Mat4::from_translation(base) * Mat4::from_quat(Quat::from_rotation_arc(Vec3::Y, dir));
        let w = WIDTH[r] * swell * pulse;
        let surging = self.surge_t < 0.3;
        let opacity = 0.26 + 0.08 * rank as f32 + if surging { 0.3 } else { 0.0 };
        self.data[0].model = (group * Mat4::from_scale(Vec3::new(w, total, w))).to_cols_array_2d();
        self.data[0].a = [color[0], color[1], color[2], opacity];
        self.data[1].model = (group * Mat4::from_scale(Vec3::new(w * 0.35, total * 0.97, w * 0.35))).to_cols_array_2d();
        self.data[1].a = [self.core_color[0], self.core_color[1], self.core_color[2], 0.55];
        self.run_visible = surging;
        if surging {
            let at = base + dir * (total * (self.surge_t / 0.3));
            self.data[2].model = (Mat4::from_translation(at) * Mat4::from_scale(Vec3::splat(0.3 + w))).to_cols_array_2d();
            self.data[2].a = [1.0, 1.0, 1.0, 0.9];
        }
    }

    pub fn draw(&self, _gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, order: i32) { self.draw_pass(pass, order); }

    pub(crate) fn draw_pass(&self, pass: &mut wgpu::RenderPass<'_>, order: i32) {
        if order == ORDER_BLADE && self.visible {
            pass.set_vertex_buffer(0, self.geo.vb.slice(..));
            pass.set_vertex_buffer(1, self.inst.slice(..));
            pass.set_index_buffer(self.geo.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_pipeline(&self.p_outer);
            pass.draw_indexed(0..self.geo.n, 0, 0..1);
            pass.set_pipeline(&self.p_front);
            pass.draw_indexed(0..self.geo.n, 0, 1..2);
        } else if order == ORDER_RUN && self.run_visible {
            pass.set_pipeline(&self.p_front);
            pass.set_vertex_buffer(0, self.run.vb.slice(..));
            pass.set_vertex_buffer(1, self.inst.slice(..));
            pass.set_index_buffer(self.run.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.run.n, 0, 2..3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 軌跡の先端もこの長さを使うので、格ごとの伸びを取り違えると軌跡と刃がずれる
    #[test]
    fn reach_and_swell_match_ts() {
        assert_eq!(total_len(2.0, 1, 1.0), 2.0 * 1.6);
        assert_eq!(total_len(2.0, 3, 1.0), 2.0 * 2.4);
        assert_eq!(swell_at(9.0), 1.0);
        assert!((swell_at(0.15) - 1.6).abs() < 1e-5, "0.15 秒で最大 1.6 倍");
    }
}
