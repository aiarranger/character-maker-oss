// 大将戦の飾り（docs/spec.md「大将の見せ方」）: 戦場の灯籠と足元の灯・残照からの光の筋・構えた手の光と火の粉。
// 戦いの計算には触れない。大将の状態（BossState・st・体力）を読んで見た目だけを決める。
use crate::geom;
use crate::gfx::{fx_layout, hex, vert_layout, FxInst, Gfx, MeshBuf, Pipe, BLEND_ADD};
use crate::view::boss_view::BossView;
use crate::view::vfx::{Burst, Vfx};
use glam::{Mat4, Vec3};
use sim::battle::{Boss, BossState};
use std::f32::consts::PI;

const LANTERNS: usize = 46;
const CANDLES: usize = 64;
const BEAMS: usize = 5;

fn hash(i: usize, salt: f32) -> f32 { ((i as f32 * 12.9898 + salt * 78.233).sin() * 43758.547).fract().abs() }
fn with(c: [f32; 3], w: f32) -> [f32; 4] { [c[0], c[1], c[2], w] }
fn inst(m: Mat4, a: [f32; 4], b: [f32; 4], c: [f32; 4]) -> FxInst { FxInst { model: m.to_cols_array_2d(), a, b, c, d: [0.0; 4] } }
/// カメラへ向く板（位置と大きさだけを行列に入れる）
fn board(p: Vec3, w: f32, h: f32, col: [f32; 3], k: f32, kind: f32) -> FxInst {
    inst(Mat4::from_translation(p) * Mat4::from_scale(Vec3::new(w, h, 1.0)), with(col, k), [kind, 0.0, 0.0, 0.0], [0.0; 4])
}

pub struct BossFx {
    pipe: wgpu::RenderPipeline,
    quad: MeshBuf,
    ground: MeshBuf,
    buf: wgpu::Buffer,
    list: Vec<FxInst>,
    /// list の先頭から n_ground 個は地面の板
    n_ground: u32,
    center: Vec3,
    /// 灯がともってからの秒（負 = 消えている）
    lit_t: f32,
    rage: f32,
    ember_t: f32,
}

impl BossFx {
    pub fn new(gfx: &Gfx) -> Self {
        let shader = gfx.shader("boss fx", include_str!("../../shaders/boss_fx.wgsl"));
        let layouts = [&gfx.globals_layout];
        let vb = [vert_layout(), fx_layout()];
        let pipe = Pipe::scene("boss fx", &shader, "vs_bfx", "fs_bfx", &layouts, &vb).transparent(BLEND_ADD).double().build(&gfx.device);
        let cap = LANTERNS * 2 + CANDLES * 2 + BEAMS + 4;
        BossFx {
            pipe,
            quad: gfx.mesh(&geom::plane(1.0, 1.0, 1, 1)),
            ground: gfx.mesh(&geom::plane(1.0, 1.0, 1, 1).rotate_x(-PI / 2.0)),
            buf: gfx.vbuf((cap * std::mem::size_of::<FxInst>()) as u64),
            list: Vec::with_capacity(cap),
            n_ground: 0,
            center: Vec3::ZERO,
            lit_t: -1.0,
            rage: 0.0,
            ember_t: 0.0,
        }
    }

    pub fn reset(&mut self) { self.lit_t = -1.0; self.rage = 0.0; self.list.clear(); self.n_ground = 0; }

    /// 灯をともす場の中心（大将の着地点のあたり）
    pub fn light_up(&mut self, x: f32, z: f32) { self.center = Vec3::new(x, 0.0, z); self.lit_t = 0.0; }

    /// 体へ渡す値: 灯の下からの光（0〜1）と怒り（0〜1）
    pub fn body_light(&self) -> (f32, f32) { ((self.lit_t * 0.6).clamp(0.0, 1.0), self.rage) }

    /// dt は実時間（スローの間も灯はゆれる）、sim_dt は戦いの時間（止まっている間は火の粉を足さない）
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, gfx: &Gfx, b: &Boss, view: &BossView, vfx: &mut Vfx, dt: f32, sim_dt: f32, time: f32) {
        self.list.clear();
        self.n_ground = 0;
        if b.state == BossState::Dormant || self.lit_t < 0.0 { return; }
        self.lit_t += dt;
        let rage_on = b.hp / b.max_hp < 0.5 && b.state != BossState::Dead;
        self.rage += ((if rage_on { 1.0 } else { 0.0 }) - self.rage) * (dt * 1.6).min(1.0);
        let lit = self.lit_t;
        let c = self.center;
        let warm = hex("#ffb060");
        let paper = hex("#ff9a48");

        // ---- 地面の灯（先に並べる）----
        for i in 0..CANDLES {
            let (a, r) = (hash(i, 1.0) * 6.2832, 8.0 + hash(i, 2.0).sqrt() * 30.0);
            let k = ((lit - r * 0.045) * 1.6).clamp(0.0, 1.0);
            if k <= 0.0 { continue; }
            let fl = 0.8 + 0.2 * (time * (7.0 + hash(i, 3.0) * 6.0) + i as f32).sin();
            let p = Vec3::new(c.x + a.sin() * r, 0.05, c.z + a.cos() * r);
            let s = 4.2 + hash(i, 4.0) * 2.4;
            self.list.push(inst(Mat4::from_translation(p) * Mat4::from_scale(Vec3::new(s, 1.0, s)), with(warm, 0.5 * k * fl), [4.0, 0.0, 0.0, 0.0], [0.0; 4]));
        }
        self.n_ground = self.list.len() as u32;
        // 足元の灯の炎
        for i in 0..CANDLES {
            let (a, r) = (hash(i, 1.0) * 6.2832, 8.0 + hash(i, 2.0).sqrt() * 30.0);
            let k = ((lit - r * 0.045) * 1.6).clamp(0.0, 1.0);
            if k <= 0.0 { continue; }
            let fl = 0.75 + 0.25 * (time * (9.0 + hash(i, 3.0) * 6.0) + i as f32 * 1.7).sin();
            let p = Vec3::new(c.x + a.sin() * r, 0.42, c.z + a.cos() * r);
            self.list.push(board(p, 0.42, 0.78 * (0.9 + 0.1 * fl), hex("#ffd890"), 2.6 * k * fl, 1.0));
        }
        // ---- 灯籠: 場のまわりをゆっくり昇る ----
        for i in 0..LANTERNS {
            let (a, r) = (hash(i, 5.0) * 6.2832, 13.0 + hash(i, 6.0) * 34.0);
            let k = ((lit - 0.6 - r * 0.04) * 0.9).clamp(0.0, 1.0);
            if k <= 0.0 { continue; }
            let span = 24.0;
            let y01 = ((lit * (0.018 + hash(i, 7.0) * 0.022)) + hash(i, 8.0)).fract();
            let edge = (y01 * 8.0).min(1.0) * ((1.0 - y01) * 5.0).min(1.0);
            let sway = (time * 0.5 + i as f32).sin() * 0.6;
            let p = Vec3::new(c.x + a.sin() * r + sway, 1.2 + y01 * span, c.z + a.cos() * r + (time * 0.4 + i as f32 * 2.0).cos() * 0.6);
            let s = 0.9 + hash(i, 9.0) * 0.5;
            let fl = 0.85 + 0.15 * (time * 3.0 + i as f32 * 0.7).sin();
            self.list.push(board(p, 4.4 * s, 4.4 * s, warm, 0.2 * k * edge * fl, 1.0));
            self.list.push(board(p, 0.95 * s, 1.3 * s, paper, 1.9 * k * edge * fl, 2.0));
        }
        // ---- 光の筋: 北の残照から、大将の後ろへ斜めに差す ----
        let beam = (lit * 0.5).clamp(0.0, 1.0) * (1.0 - self.rage * 0.4);
        for i in 0..BEAMS {
            let x = c.x + (i as f32 - (BEAMS as f32 - 1.0) * 0.5) * 9.5 + hash(i, 10.0) * 4.0;
            let z = c.z - 12.0 - hash(i, 11.0) * 14.0;
            let w = 5.0 + hash(i, 12.0) * 5.0;
            let m = Mat4::from_translation(Vec3::new(x, 0.0, z)) * Mat4::from_scale(Vec3::new(w, 46.0, 1.0));
            self.list.push(inst(m, with(hex("#ffa860"), 0.15 * beam), [3.0, i as f32, time, 0.0], [0.0, -26.0, 0.0, 0.0]));
        }
        // ---- 構えた手・足の光と火の粉 ----
        let st = b.st as f32;
        let charge = |fire: f32| (st / fire).clamp(0.0, 1.0);
        let (pts, k, col): ([Option<Vec3>; 2], f32, [f32; 3]) = match b.state {
            BossState::PunchL => ([view.hand(true), None], charge(0.95), hex("#ff7a3a")),
            BossState::PunchR => ([view.hand(false), None], charge(0.95), hex("#ff7a3a")),
            BossState::Stomp => ([view.foot(true), view.foot(false)], charge(1.35), hex("#ff6a3a")),
            BossState::Sauce => ([view.hand(true), view.hand(false)], charge(1.5), hex("#ff9a30")),
            _ => ([None, None], 0.0, [0.0; 3]),
        };
        for p in pts.into_iter().flatten() {
            let pulse = 0.85 + 0.15 * (time * 22.0).sin();
            self.list.push(board(p, 2.2 + 3.6 * k, 2.2 + 3.6 * k, col, (0.5 + 1.3 * k) * pulse, 1.0));
            if sim_dt > 0.0 {
                vfx.burst(Burst { n: 1 + (k * 2.0) as u32, x: p.x, y: p.y, z: p.z, color: col, colors: &[col, hex("#ffd27a")], type_: 0, speed: 3.0 + 4.0 * k, up: 1.0, size: 0.16, life: 0.5, gravity: -2.5, ..Default::default() });
            }
        }
        // たれ: 体ごと回る間、まわりにたれの渦が走る
        if b.state == BossState::Sauce && sim_dt > 0.0 && st > 0.25 && st < 1.5 {
            let sauce = hex("#ff9a30");
            for k in 0..2 {
                let a = st * 11.0 + k as f32 * PI;
                let r = 3.2 + st * 2.4;
                let p = view.pos() + Vec3::new(a.sin() * r, 2.2 + st * 1.6, a.cos() * r);
                vfx.burst(Burst { n: 2, x: p.x, y: p.y, z: p.z, color: sauce, colors: &[sauce, hex("#b0602a"), hex("#ffd27a")], type_: 1, speed: 1.5, size: 2.2, life: 0.45, gravity: 0.0, ..Default::default() });
                vfx.burst(Burst { n: 2, x: p.x, y: p.y, z: p.z, color: sauce, type_: 0, speed: 7.0, up: 0.8, size: 0.2, life: 0.6, ..Default::default() });
            }
        }
        // 怒り: 体から火の粉が上がり続ける
        if self.rage > 0.3 && sim_dt > 0.0 {
            self.ember_t += sim_dt;
            while self.ember_t > 0.035 {
                self.ember_t -= 0.035;
                let a = hash((time * 1000.0) as usize, 13.0) * 6.2832;
                let p = view.pos() + Vec3::new(a.sin() * 2.6, 0.6 + hash((time * 777.0) as usize, 14.0) * 5.0, a.cos() * 2.6);
                vfx.burst(Burst { n: 1, x: p.x, y: p.y, z: p.z, color: hex("#ff7a30"), colors: &[hex("#ff7a30"), hex("#ffc060"), hex("#ff4a2a")], type_: 0, speed: 1.6, up: 2.0, size: 0.2, life: 1.5, gravity: -3.2, drag: 0.6, ..Default::default() });
            }
        }
        if !self.list.is_empty() {
            gfx.queue.write_buffer(&self.buf, 0, bytemuck::cast_slice(&self.list));
        }
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, order: i32) {
        if order != 8 || self.list.is_empty() { return; }
        pass.set_pipeline(&self.pipe);
        pass.set_vertex_buffer(1, self.buf.slice(..));
        for (mesh, range) in [(&self.ground, 0..self.n_ground), (&self.quad, self.n_ground..self.list.len() as u32)] {
            if range.is_empty() { continue; }
            pass.set_vertex_buffer(0, mesh.vb.slice(..));
            pass.set_index_buffer(mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.n, 0, range);
        }
    }
}
