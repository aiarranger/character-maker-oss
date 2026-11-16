// 派手な見せ方: 斬撃の弧（三日月の帯）・剣の軌跡・火花と粒・地面の衝撃波・亀裂・光の柱・爆発の球・予告の円。
// どれも加算の色で描き、ブルームで光らせる。数の多いもの（粒）は 1 つのインスタンス描画にまとめる。
// 弧・輪・亀裂・柱・球・予告は three の Mesh 1 個ごとの uniform を FxInst の値にして 1 つの buffer から描く。
use crate::geom::{self, Geo};
use crate::gfx::{fx_layout, hex, lerp3, vert_layout, FxInst, Gfx, MeshBuf, Pipe, BLEND_ADD, BLEND_NORMAL};
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use sim::battle::{Telegraph, TelegraphKind, TelegraphSrc};
use sim::moves::{Arc, ArcKind};
use std::collections::VecDeque;
use std::f32::consts::PI;
use std::ops::Range;
use wgpu::util::DeviceExt;

pub const SHADER: &str = include_str!("../../shaders/vfx.wgsl");

const PMAX: usize = 2600;
const LMAX: usize = 320;
const TAFFY_MAX: usize = 96;
const TRAIL_N: usize = 18;
/// link の life の既定（TS の既定値）
pub const LINK_LIFE: f64 = 1.1;

/// 小さな xorshift（Math.random の代わり）。返す値は 0 以上 1 未満
pub(crate) struct Rng(u32);
impl Rng {
    pub(crate) fn next(&mut self) -> f32 {
        if let Some(v) = crate::platform::js_random() { return v; }
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// TS の Burst。省いた値は TS の既定値になる（`Burst { n, x, y, z, color, type_, ..Default::default() }`）。
/// gravity・stretch・drag の既定は type_ で変わるので、NaN を「未指定」とする。
#[derive(Clone, Copy)]
pub struct Burst<'a> {
    pub n: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub color: [f32; 3],
    pub type_: u8,
    pub speed: f32,
    pub up: f32,
    pub size: f32,
    pub life: f32,
    pub spread: f32,
    pub dir: Option<[f32; 3]>,
    pub gravity: f32,
    pub stretch: f32,
    pub drag: f32,
    /// 空なら color だけ。スタックの配列を渡せるので、毎コマのアロケーションはない
    pub colors: &'a [[f32; 3]],
}
impl Default for Burst<'_> {
    fn default() -> Self {
        Burst { n: 0, x: 0.0, y: 0.0, z: 0.0, color: [1.0; 3], type_: 0, speed: 6.0, up: 0.6, size: 0.3, life: 0.5, spread: 0.5, dir: None, gravity: f32::NAN, stretch: f32::NAN, drag: f32::NAN, colors: &[] }
    }
}
fn or_nan(v: f32, d: f32) -> f32 { if v.is_nan() { d } else { v } }

#[derive(Clone, Copy)]
struct Particle { x: f32, y: f32, z: f32, vx: f32, vy: f32, vz: f32, r: f32, g: f32, b: f32, size: f32, life: f32, age: f32, type_: u8, grav: f32, stretch: f32, drag: f32, rot: f32, rv: f32 }

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PartInst { pos: [f32; 3], dir: [f32; 3], col: [f32; 4], size: [f32; 3], type_: f32 }
const PART_ATTRS: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![8 => Float32x3, 9 => Float32x3, 10 => Float32x4, 11 => Float32x3, 12 => Float32];

#[derive(Clone, Copy)]
struct Link { ax: f32, ay: f32, az: f32, bx: f32, by: f32, bz: f32, r: f32, g: f32, b: f32, age: f64, life: f64, w: f32, chain: i64, boost: f64 }

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LinkInst { a: [f32; 3], b: [f32; 3], col: [f32; 4], p: [f32; 2] }
const LINK_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![8 => Float32x3, 9 => Float32x3, 10 => Float32x4, 11 => Float32x2];

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TrailV { pos: [f32; 3], a: f32 }
const TRAIL_ATTRS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32];

// 時計は three と同じく倍精度（f32 だと 1.2 秒の柱が 1 コマ長く残り、回転もずれる）
struct ArcFx { model: Mat4, t: f64, dur: f64, alive: bool, p: f32, life: f32, from: f32, to: f32, r: f32, width: f32, alpha: f32, core: [f32; 3], edge: [f32; 3] }

#[derive(Clone, Copy, PartialEq)]
enum Kind { Ring, Crack, Pillar, Ball }
/// 地面の波・亀裂・柱・球。Three の Mesh と同じく、使い終わった枠を種類ごとに使い回す（柱の回転は前の値を引き継ぐ）
struct Timed { kind: Kind, t: f64, dur: f64, alive: bool, size: f32, pos: Vec3, scale: Vec3, rot: f64, color: [f32; 3], thick: f32, seed: f32, twist: f32, k: f32, a: f32, time: f32 }

/// 粒を足す（TS の burst の中身。乱数を引く順も同じ）
fn emit(parts: &mut VecDeque<Particle>, rng: &mut Rng, o: &Burst) {
    for _ in 0..o.n {
        if parts.len() >= PMAX * 2 - 10 { parts.pop_front(); }
        let sp = o.speed * (0.4 + rng.next() * 0.8);
        let (dx, dy, dz);
        if let Some(dir) = o.dir {
            let s = o.spread;
            dx = dir[0] + (rng.next() - 0.5) * 2.0 * s;
            dy = dir[1] + (rng.next() - 0.5) * 2.0 * s;
            dz = dir[2] + (rng.next() - 0.5) * 2.0 * s;
        } else {
            let a = rng.next() * PI * 2.0;
            let u = rng.next() * 2.0 - 1.0;
            let r = (1.0 - u * u).sqrt();
            dx = a.cos() * r;
            dy = u.abs() * o.up + o.up * 0.3;
            dz = a.sin() * r;
        }
        let l = (dx * dx + dy * dy + dz * dz).sqrt();
        let l = if l > 0.0 { l } else { 1.0 };
        let col = if o.colors.is_empty() { o.color } else { o.colors[(rng.next() * o.colors.len() as f32) as usize % o.colors.len()] };
        let size = o.size * (0.6 + rng.next() * 0.8);
        let life = o.life * (0.6 + rng.next() * 0.7);
        let rot = rng.next() * 6.28;
        let rv = (rng.next() - 0.5) * 16.0;
        parts.push_back(Particle {
            x: o.x, y: o.y, z: o.z, vx: dx / l * sp, vy: dy / l * sp, vz: dz / l * sp, r: col[0], g: col[1], b: col[2],
            size, life, age: 0.0, type_: o.type_,
            grav: or_nan(o.gravity, match o.type_ { 3 => 14.0, 4 => -0.6, _ => 9.0 }),
            stretch: or_nan(o.stretch, if o.type_ == 0 { 0.22 } else { 0.0 }),
            drag: or_nan(o.drag, if o.type_ == 4 { 3.0 } else { 1.2 }),
            rot, rv,
        });
    }
}

/// 粒を進めて、加算の束 add と紙吹雪・土煙の束 norm へ詰める（TS と同じく新しい粒から）
fn step_parts(parts: &mut VecDeque<Particle>, add: &mut Vec<PartInst>, norm: &mut Vec<PartInst>, dt: f32) {
    add.clear();
    norm.clear();
    for p in parts.iter_mut().rev() {
        p.age += dt;
        if p.age >= p.life { continue; }
        p.vy -= p.grav * dt;
        let dr = (-p.drag * dt).exp();
        p.vx *= dr; p.vy *= dr; p.vz *= dr;
        p.x += p.vx * dt; p.y += p.vy * dt; p.z += p.vz * dt;
        if p.type_ == 3 && p.y < 0.03 { p.y = 0.03; p.vy *= -0.3; p.vx *= 0.6; p.vz *= 0.6; p.rv *= 0.5; }
        p.rot += p.rv * dt;
        let k = p.age / p.life;
        let out = if p.type_ <= 2 { &mut *add } else { &mut *norm };
        if out.len() >= PMAX { continue; }
        let dir = if p.stretch > 0.0 { [p.vx * p.stretch, p.vy * p.stretch, p.vz * p.stretch] } else { [0.0; 3] };
        let fade = match p.type_ { 4 => (1.0 - k) * (k * 6.0).min(1.0), 3 => if k > 0.8 { (1.0 - k) * 5.0 } else { 1.0 }, _ => (1.0 - k) * (1.0 - k) };
        let sz = match p.type_ { 4 => p.size * (1.0 + k * 2.2), 2 => p.size * (1.0 + (k * PI).sin() * 0.6), _ => p.size };
        out.push(PartInst { pos: [p.x, p.y, p.z], dir, col: [p.r, p.g, p.b, fade], size: [sz, if p.stretch > 0.0 { 1.0 } else { 0.0 }, p.rot], type_: p.type_ as f32 });
    }
    parts.retain(|p| p.age < p.life);
}

fn flash_links(links: &mut [Link], chain: i64) {
    for l in links {
        if l.chain == chain { l.boost = 1.0; l.age = l.age.min(l.life * 0.4); }
    }
}

fn step_links(links: &mut Vec<Link>, inst: &mut Vec<LinkInst>, dt: f64) {
    inst.clear();
    links.retain_mut(|l| {
        l.age += dt;
        l.boost = (l.boost - dt * 2.2).max(0.0);
        if l.age >= l.life { return false; }
        let k = l.age / l.life;
        let a = (l.age / 0.04).min(1.0) * (1.0 - k).powf(1.4) + l.boost;
        inst.push(LinkInst { a: [l.ax, l.ay, l.az], b: [l.bx, l.by, l.bz], col: [l.r, l.g, l.b, a as f32], p: [l.w * (1.0 + l.boost as f32 * 0.6), (l.age / 0.16) as f32] });
        true
    });
}

fn step_timed(s: &mut Timed, dt: f64, time: f32) {
    s.t += dt;
    let k = s.t / s.dur;
    if k >= 1.0 { s.alive = false; return; }
    match s.kind {
        Kind::Ring => s.k = (1.0 - (1.0 - k).powf(2.2)) as f32,
        Kind::Crack => s.a = (if k < 0.7 { 1.0 } else { (1.0 - k) / 0.3 }) as f32,
        Kind::Pillar => {
            s.time = time;
            s.a = (((k * 4.0).min(1.0) * std::f64::consts::PI / 2.0).sin() * (1.0 - k * k)) as f32;
            s.rot += dt * 9.0;
        }
        Kind::Ball => {
            let e = 1.0 - (1.0 - k).powf(3.0);
            s.scale = Vec3::splat(((s.size as f64 * e).max(0.1)) as f32);
            s.a = ((1.0 - k) * (1.0 - k)) as f32;
        }
    }
}

fn fx(model: Mat4, a: [f32; 4], b: [f32; 4], c: [f32; 4], d: [f32; 4]) -> FxInst { FxInst { model: model.to_cols_array_2d(), a, b, c, d } }
fn with(c: [f32; 3], w: f32) -> [f32; 4] { [c[0], c[1], c[2], w] }

fn timed_inst(s: &Timed) -> FxInst {
    let m = Mat4::from_translation(s.pos) * Mat4::from_rotation_y(s.rot as f32) * Mat4::from_scale(s.scale);
    match s.kind {
        Kind::Ring => fx(m, with(s.color, s.k), [s.thick, 0.0, 0.0, 0.0], [0.0; 4], [0.0; 4]),
        Kind::Crack => fx(m, with(s.color, s.a), [s.seed, 0.0, 0.0, 0.0], [0.0; 4], [0.0; 4]),
        Kind::Pillar => fx(m, with(s.color, s.a), [s.time, s.twist, 0.0, 0.0], [0.0; 4], [0.0; 4]),
        Kind::Ball => fx(m, with(s.color, s.a), [0.0; 4], [0.0; 4], [0.0; 4]),
    }
}

/// 予告 1 つの見た目。描かないなら None（放たれた波の予告など）
fn tel_inst(t: &Telegraph, time: f32, sauce: [f32; 3], red: [f32; 3]) -> Option<FxInst> {
    let is_ring = matches!(t.kind, TelegraphKind::Ring);
    if t.done && !is_ring { return None; }
    if is_ring && t.t >= t.fire { return None; }
    let color = if matches!(t.src, TelegraphSrc::Sauce) { sauce } else { red };
    let k = (t.t / t.fire).clamp(0.0, 1.0) as f32;
    let (x, z, r, face, w) = (t.x as f32, t.z as f32, t.r as f32, t.face as f32, t.w as f32);
    let mut pos = Vec3::new(x, 0.07, z);
    let (kind, arc, fire, rot, scale);
    match t.kind {
        TelegraphKind::Circle => { kind = 0.0; arc = 0.6; fire = 1.0; rot = 0.0; scale = Vec3::new(r * 2.0, 1.0, r * 2.0); }
        TelegraphKind::Cone => { kind = 1.0; arc = t.arc as f32; fire = 1.0; rot = face + PI; scale = Vec3::new(r * 2.0, 1.0, r * 2.0); }
        TelegraphKind::Line => {
            kind = 2.0; arc = 0.6; fire = 1.0; rot = face + PI; scale = Vec3::new(w, 1.0, r);
            pos = Vec3::new(x + face.sin() * r / 2.0, 0.07, z + face.cos() * r / 2.0);
        }
        TelegraphKind::Ring => { kind = 3.0; arc = 0.6; fire = 1.0; rot = 0.0; scale = Vec3::new(r * 2.0, 1.0, r * 2.0); }
    }
    let m = Mat4::from_translation(pos) * Mat4::from_rotation_y(rot) * Mat4::from_scale(scale);
    Some(fx(m, [k, kind, arc, fire], with(color, time), [0.0; 4], [0.0; 4]))
}

fn mesh_buf(device: &wgpu::Device, g: &Geo) -> MeshBuf {
    MeshBuf {
        vb: device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&g.v), usage: wgpu::BufferUsages::VERTEX }),
        ib: device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&g.i), usage: wgpu::BufferUsages::INDEX }),
        n: g.i.len() as u32,
    }
}
fn dyn_buf(device: &wgpu::Device, bytes: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor { label: None, size: bytes.max(16), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })
}

/// FxInst の列を buffer へ。足りなければ倍に作り直す
fn write_fx(device: &wgpu::Device, queue: &wgpu::Queue, buf: &mut wgpu::Buffer, data: &[FxInst]) {
    if data.is_empty() { return; }
    let bytes = std::mem::size_of_val(data) as u64;
    if buf.size() < bytes { *buf = dyn_buf(device, bytes.max(buf.size() * 2)); }
    queue.write_buffer(buf, 0, bytemuck::cast_slice(data));
}

fn draw_mesh(pass: &mut wgpu::RenderPass<'_>, pipe: &wgpu::RenderPipeline, mesh: &MeshBuf, inst: &wgpu::Buffer, range: Range<u32>) {
    if range.is_empty() { return; }
    pass.set_pipeline(pipe);
    pass.set_vertex_buffer(0, mesh.vb.slice(..));
    pass.set_vertex_buffer(1, inst.slice(..));
    pass.set_index_buffer(mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..mesh.n, 0, range);
}

pub struct Vfx {
    rng: Rng,
    arcs: Vec<ArcFx>,
    timed: Vec<Timed>,
    parts: VecDeque<Particle>,
    links: Vec<Link>,
    // CPU 側の詰め先（毎コマ使い回す）
    add_inst: Vec<PartInst>,
    norm_inst: Vec<PartInst>,
    link_inst: Vec<LinkInst>,
    /// 飴の糸（毎コマ出し直す）。taffy_n = いま描く本数
    taffy_inst: Vec<LinkInst>,
    taffy_n: u32,
    fx_inst: Vec<FxInst>,
    tel_list: Vec<FxInst>,
    r_arcs: Range<u32>,
    r_crack: Range<u32>,
    r_ring: Range<u32>,
    r_pillar: Range<u32>,
    r_ball: Range<u32>,
    // 軌跡
    hist_tip: [Vec3; TRAIL_N],
    hist_base: [Vec3; TRAIL_N],
    trail_verts: [TrailV; TRAIL_N * 2],
    trail_on: f32,
    // 予告の色
    tel_sauce: [f32; 3],
    tel_red: [f32; 3],
    // 既定の亀裂の色
    crack_glow: [f32; 3],
    // GPU
    p_arc: wgpu::RenderPipeline,
    p_ring: wgpu::RenderPipeline,
    p_crack: wgpu::RenderPipeline,
    p_pillar: wgpu::RenderPipeline,
    p_ball: wgpu::RenderPipeline,
    p_tel: wgpu::RenderPipeline,
    p_trail: wgpu::RenderPipeline,
    p_part_add: wgpu::RenderPipeline,
    p_part_norm: wgpu::RenderPipeline,
    p_link: wgpu::RenderPipeline,
    p_taffy: wgpu::RenderPipeline,
    arc_geo: MeshBuf,
    ring_geo: MeshBuf,
    pillar_geo: MeshBuf,
    ball_geo: MeshBuf,
    quad: MeshBuf,
    fx_buf: wgpu::Buffer,
    tel_buf: wgpu::Buffer,
    add_buf: wgpu::Buffer,
    norm_buf: wgpu::Buffer,
    link_buf: wgpu::Buffer,
    taffy_buf: wgpu::Buffer,
    trail_vb: wgpu::Buffer,
    trail_ib: wgpu::Buffer,
    trail_i_n: u32,
    trail_u: wgpu::Buffer,
}

impl Vfx {
    pub fn new(gfx: &Gfx) -> Self {
        let shader = gfx.shader("vfx", SHADER);
        Self::build(&gfx.device, &gfx.globals_layout, &shader)
    }

    pub(crate) fn build(device: &wgpu::Device, globals: &wgpu::BindGroupLayout, shader: &wgpu::ShaderModule) -> Self {
        let layouts = [globals];
        let fx_vbufs = [vert_layout(), fx_layout()];
        let part_vbufs = [vert_layout(), wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<PartInst>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &PART_ATTRS }];
        let link_vbufs = [vert_layout(), wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<LinkInst>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &LINK_ATTRS }];
        let trail_vbufs = [wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<TrailV>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &TRAIL_ATTRS }, fx_layout()];
        let p_arc = Pipe::scene("arc", shader, "vs_arc", "fs_arc", &layouts, &fx_vbufs).transparent(BLEND_ADD).double().build(device);
        let p_ring = Pipe::scene("ring", shader, "vs_basic_uv", "fs_ring", &layouts, &fx_vbufs).transparent(BLEND_ADD).build(device);
        // TS は polygonOffsetFactor -1（傾きの分だけ手前へ）。Pipe の bias では傾きの向きが写せないので、地面から 0.035 浮かせるだけにする
        let p_crack = Pipe::scene("crack", shader, "vs_basic_uv", "fs_crack", &layouts, &fx_vbufs).transparent(BLEND_NORMAL).build(device);
        let p_pillar = Pipe::scene("pillar", shader, "vs_basic_uv", "fs_pillar", &layouts, &fx_vbufs).transparent(BLEND_ADD).double().build(device);
        let p_ball = Pipe::scene("ball", shader, "vs_ball", "fs_ball", &layouts, &fx_vbufs).transparent(BLEND_ADD).build(device);
        let p_tel = Pipe::scene("telegraph", shader, "vs_basic_uv", "fs_tel", &layouts, &fx_vbufs).transparent(BLEND_ADD).build(device);
        let p_trail = Pipe::scene("trail", shader, "vs_trail", "fs_trail", &layouts, &trail_vbufs).transparent(BLEND_ADD).double().build(device);
        let p_part_add = Pipe::scene("part add", shader, "vs_part", "fs_part", &layouts, &part_vbufs).transparent(BLEND_ADD).build(device);
        // 紙吹雪・土煙は加算しない（色を見せる）: 別の束
        let p_part_norm = Pipe::scene("part norm", shader, "vs_part", "fs_part", &layouts, &part_vbufs).transparent(BLEND_NORMAL).build(device);
        // 連鎖の光の線: 敵の陰でも見えるよう奥行きを無視して重ねる
        let mut link_pipe = Pipe::scene("link", shader, "vs_link", "fs_link", &layouts, &link_vbufs).transparent(BLEND_ADD);
        link_pipe.depth = wgpu::CompareFunction::Always;
        let p_link = link_pipe.build(device);
        // 飴の糸: 光らせず、飴色の帯として重ねる
        let p_taffy = Pipe::scene("taffy", shader, "vs_link", "fs_taffy", &layouts, &link_vbufs).transparent(BLEND_NORMAL).build(device);

        let mut trail_idx: Vec<u32> = Vec::with_capacity((TRAIL_N - 1) * 6);
        for i in 0..(TRAIL_N as u32 - 1) {
            let (a, b, c, d) = (i * 2, i * 2 + 1, i * 2 + 2, i * 2 + 3);
            trail_idx.extend_from_slice(&[a, b, c, b, d, c]);
        }
        Vfx {
            rng: Rng(0x9e37_79b9),
            arcs: Vec::new(),
            timed: Vec::new(),
            parts: VecDeque::with_capacity(PMAX * 2),
            links: Vec::with_capacity(LMAX),
            add_inst: Vec::with_capacity(PMAX),
            norm_inst: Vec::with_capacity(PMAX),
            link_inst: Vec::with_capacity(LMAX),
            taffy_inst: Vec::with_capacity(TAFFY_MAX), taffy_n: 0,
            fx_inst: Vec::with_capacity(64),
            tel_list: Vec::with_capacity(16),
            r_arcs: 0..0, r_crack: 0..0, r_ring: 0..0, r_pillar: 0..0, r_ball: 0..0,
            hist_tip: [Vec3::ZERO; TRAIL_N],
            hist_base: [Vec3::ZERO; TRAIL_N],
            trail_verts: [TrailV { pos: [0.0; 3], a: 0.0 }; TRAIL_N * 2],
            trail_on: 0.0,
            tel_sauce: hex("#ff8a30"),
            tel_red: hex("#ff2a3a"),
            crack_glow: hex("#ff9a40"),
            p_arc, p_ring, p_crack, p_pillar, p_ball, p_tel, p_trail, p_part_add, p_part_norm, p_link, p_taffy,
            arc_geo: mesh_buf(device, &geom::plane(1.0, 1.0, 64, 2)),
            ring_geo: mesh_buf(device, &geom::plane(1.0, 1.0, 1, 1).rotate_x(-PI / 2.0)),
            pillar_geo: mesh_buf(device, &geom::cylinder(1.0, 1.25, 1.0, 40, true).translate(0.0, 0.5, 0.0)),
            ball_geo: mesh_buf(device, &geom::sphere(1.0, 32, 16)),
            quad: mesh_buf(device, &geom::plane(1.0, 1.0, 1, 1)),
            fx_buf: dyn_buf(device, 64 * std::mem::size_of::<FxInst>() as u64),
            tel_buf: dyn_buf(device, 16 * std::mem::size_of::<FxInst>() as u64),
            add_buf: dyn_buf(device, (PMAX * std::mem::size_of::<PartInst>()) as u64),
            norm_buf: dyn_buf(device, (PMAX * std::mem::size_of::<PartInst>()) as u64),
            link_buf: dyn_buf(device, (LMAX * std::mem::size_of::<LinkInst>()) as u64),
            taffy_buf: dyn_buf(device, (TAFFY_MAX * std::mem::size_of::<LinkInst>()) as u64),
            trail_vb: dyn_buf(device, (TRAIL_N * 2 * std::mem::size_of::<TrailV>()) as u64),
            trail_ib: device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&trail_idx), usage: wgpu::BufferUsages::INDEX }),
            trail_i_n: trail_idx.len() as u32,
            trail_u: dyn_buf(device, std::mem::size_of::<FxInst>() as u64),
        }
    }

    /// 斬撃の弧を出す。x,y,z は主役の足元、face は主役の向き。scale の既定は 1
    pub fn slash(&mut self, arc: &Arc, x: f32, y: f32, z: f32, face: f32, tint: [f32; 3], scale: f32) {
        let i = match self.arcs.iter().position(|a| !a.alive) {
            Some(i) => i,
            None => {
                self.arcs.push(ArcFx { model: Mat4::IDENTITY, t: 0.0, dur: 0.2, alive: false, p: 0.0, life: 0.0, from: 0.0, to: 1.0, r: 3.0, width: 1.0, alpha: 1.0, core: [1.0; 3], edge: [1.0; 3] });
                self.arcs.len() - 1
            }
        };
        let a = &mut self.arcs[i];
        a.alive = true; a.t = 0.0; a.dur = arc.dur; a.p = 0.0; a.life = 0.0;
        a.model = Mat4::from_translation(Vec3::new(x, y + arc.y as f32, z)) * Mat4::from_rotation_y(face) * Mat4::from_rotation_z(arc.tilt as f32);
        a.from = arc.from as f32; a.to = arc.to as f32; a.r = arc.r as f32 * scale * 1.05; a.width = arc.width as f32 * scale * 1.7;
        let ranbu = matches!(arc.kind, ArcKind::Ranbu);
        let heavy = !matches!(arc.kind, ArcKind::Slash);
        a.alpha = if ranbu { 1.05 } else if heavy { 0.95 } else { 0.78 };
        // 飴細工の刃（2026-11-15 見た目の磨き）: 芯はほんのり桃色の白、縁は桃色（ふつう）・蜂蜜色（強い・乱舞）の飴の色。白だけだとブルームで白いもやに見える
        a.core = hex(if ranbu { "#fff1d8" } else if heavy { "#fff3e6" } else { "#fff0f5" });
        let e = lerp3(tint, hex(if ranbu { "#ffa020" } else if heavy { "#ffae40" } else { "#ff6fa8" }), if ranbu { 0.65 } else { 0.55 });
        a.edge = [e[0] * 0.85, e[1] * 0.85, e[2] * 0.85];
    }

    /// 確かめ用: 生きている斬撃・輪・柱・球・粒の値（今の版は rust/compare/scenes.py の VALS_TS が同じ形で作る）
    #[cfg(target_arch = "wasm32")]
    pub fn dump(&self) -> serde_json::Value {
        use serde_json::json;
        let name = |k: Kind| match k { Kind::Ring => "ring", Kind::Crack => "crack", Kind::Pillar => "pillar", Kind::Ball => "ball" };
        let (mut sx, mut sy, mut sz) = (0.0f64, 0.0f64, 0.0f64);
        for p in &self.parts { sx += p.x as f64; sy += p.y as f64; sz += p.z as f64; }
        json!({
            "parts": self.parts.len(), "sum": [sx, sy, sz], "links": self.links.len(),
            "arcs": self.arcs.iter().filter(|a| a.alive).map(|a| [a.from, a.to, a.r, a.width, a.p, a.life, a.alpha]).collect::<Vec<_>>(),
            "timed": self.timed.iter().filter(|o| o.alive).map(|o| json!([name(o.kind), o.t, o.dur, o.pos.x, o.pos.y, o.pos.z, o.scale.x, o.scale.y, o.scale.z, o.rot])).collect::<Vec<_>>(),
        })
    }

    /// 紙吹雪（type 3）は 6 割に減らす（画面がいつも散らかって見えないように。2026-11-15 見た目の磨き）
    pub fn burst(&mut self, mut o: Burst) { if o.type_ == 3 && o.n > 3 { o.n = (o.n * 3 + 4) / 5; } emit(&mut self.parts, &mut self.rng, &o); }

    fn slot(&mut self, kind: Kind) -> &mut Timed {
        let i = match self.timed.iter().position(|o| !o.alive && o.kind == kind) {
            Some(i) => i,
            None => {
                self.timed.push(Timed { kind, t: 0.0, dur: 1.0, alive: false, size: 1.0, pos: Vec3::ZERO, scale: Vec3::ONE, rot: 0.0, color: [1.0; 3], thick: 0.08, seed: 0.0, twist: 1.0, k: 0.0, a: 1.0, time: 0.0 });
                self.timed.len() - 1
            }
        };
        let s = &mut self.timed[i];
        s.alive = true; s.t = 0.0;
        s
    }

    /// 地面の波（高さは TS の既定 0.06）
    pub fn ring(&mut self, x: f32, z: f32, r: f32, color: [f32; 3], dur: f64, thick: f32) { self.ring_at(x, z, r, color, dur, thick, 0.06); }
    pub fn ring_at(&mut self, x: f32, z: f32, r: f32, color: [f32; 3], dur: f64, thick: f32, y: f32) {
        let s = self.slot(Kind::Ring);
        s.dur = dur; s.size = r;
        s.pos = Vec3::new(x, y, z); s.scale = Vec3::new(r * 2.0, 1.0, r * 2.0);
        s.color = color; s.thick = thick; s.k = 0.0;
    }
    /// 亀裂。color が None なら TS の既定 #ff9a40
    pub fn crack(&mut self, x: f32, z: f32, r: f32, color: Option<[f32; 3]>) {
        let glow = color.unwrap_or(self.crack_glow);
        let rot = self.rng.next() * 6.28;
        let seed = self.rng.next() * 100.0;
        let s = self.slot(Kind::Crack);
        s.dur = 2.6; s.size = r;
        s.pos = Vec3::new(x, 0.035, z); s.scale = Vec3::new(r * 2.0, 1.0, r * 2.0); s.rot = rot as f64;
        s.seed = seed; s.color = glow; s.a = 1.0;
    }
    pub fn pillar(&mut self, x: f32, z: f32, r: f32, h: f32, color: [f32; 3], dur: f64, twist: f32) {
        let s = self.slot(Kind::Pillar);
        s.dur = dur; s.size = r;
        s.pos = Vec3::new(x, 0.0, z); s.scale = Vec3::new(r, h, r);
        s.color = color; s.twist = twist;
    }
    pub fn ball(&mut self, x: f32, y: f32, z: f32, r: f32, color: [f32; 3], dur: f64) {
        let s = self.slot(Kind::Ball);
        s.dur = dur; s.size = r;
        s.pos = Vec3::new(x, y, z); s.scale = Vec3::splat(0.1);
        s.color = color;
    }

    /// 連鎖の光の線（弾 → 巻き込まれた敵）。線は巻き込まれた側ほど明るく、生まれた瞬間に光の粒が弾から敵へ走る。life の既定は LINK_LIFE
    #[allow(clippy::too_many_arguments)]
    pub fn link(&mut self, ax: f32, ay: f32, az: f32, bx: f32, by: f32, bz: f32, color: [f32; 3], chain: i64, w: f32, life: f64) {
        if self.links.len() >= LMAX { self.links.remove(0); }
        self.links.push(Link { ax, ay, az, bx, by, bz, r: color[0], g: color[1], b: color[2], age: 0.0, life, w, chain, boost: 0.0 });
    }
    /// 飴の糸の 1 節（a → b、太さ w、濃さ alpha）。そのコマだけ描くので、毎コマ出し直す
    /// twist = ねじれの位相（照りの位置が帯の上をめぐる）
    pub fn taffy(&mut self, a: Vec3, b: Vec3, color: [f32; 3], w: f32, alpha: f32, twist: f32) {
        if self.taffy_inst.len() < TAFFY_MAX { self.taffy_inst.push(LinkInst { a: a.into(), b: b.into(), col: [color[0], color[1], color[2], alpha], p: [w, twist] }); }
    }
    /// 連鎖が終わった: その網をもう一度光らせる
    pub fn link_flash(&mut self, chain: i64) { flash_links(&mut self.links, chain); }

    /// 剣の軌跡（刃の根元と先の履歴の帯）
    pub fn update_trail(&mut self, gfx: &Gfx, tip: Vec3, base: Vec3, on: bool, dt: f32, color: [f32; 3]) {
        self.step_trail(tip, base, on, dt);
        self.upload_trail(&gfx.queue, color);
    }
    pub(crate) fn step_trail(&mut self, tip: Vec3, base: Vec3, on: bool, dt: f32) {
        self.hist_tip.copy_within(0..TRAIL_N - 1, 1);
        self.hist_base.copy_within(0..TRAIL_N - 1, 1);
        self.hist_tip[0] = tip;
        self.hist_base[0] = base;
        self.trail_on += ((if on { 1.0 } else { 0.0 }) - self.trail_on) * (dt * if on { 30.0 } else { 10.0 }).min(1.0);
        for i in 0..TRAIL_N {
            let k = 1.0 - i as f32 / (TRAIL_N - 1) as f32;
            // 刃の先を少し外へ伸ばす（遠くからでも弧が読める）
            let (t, b) = (self.hist_tip[i], self.hist_base[i]);
            self.trail_verts[i * 2] = TrailV { pos: (t + (t - b) * 0.25).to_array(), a: k * k * k * 0.55 };
            self.trail_verts[i * 2 + 1] = TrailV { pos: b.to_array(), a: 0.0 };
        }
    }
    pub(crate) fn upload_trail(&self, queue: &wgpu::Queue, color: [f32; 3]) {
        if self.trail_on <= 0.01 { return; }
        queue.write_buffer(&self.trail_vb, 0, bytemuck::cast_slice(&self.trail_verts));
        let u = fx(Mat4::IDENTITY, with(color, self.trail_on), [0.0; 4], [0.0; 4], [0.0; 4]);
        queue.write_buffer(&self.trail_u, 0, bytemuck::bytes_of(&u));
    }

    /// 予告（地面の赤い印）
    pub fn update_telegraphs(&mut self, gfx: &Gfx, list: &[Telegraph], time: f32) {
        self.step_telegraphs(list, time);
        self.upload_telegraphs(&gfx.device, &gfx.queue);
    }
    pub(crate) fn upload_telegraphs(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        write_fx(device, queue, &mut self.tel_buf, &self.tel_list);
    }
    pub(crate) fn step_telegraphs(&mut self, list: &[Telegraph], time: f32) {
        self.tel_list.clear();
        for t in list {
            if let Some(i) = tel_inst(t, time, self.tel_sauce, self.tel_red) { self.tel_list.push(i); }
        }
    }

    pub fn update(&mut self, gfx: &Gfx, dt: f64, time: f32) {
        self.step(dt, time);
        self.upload(&gfx.device, &gfx.queue);
    }

    pub(crate) fn step(&mut self, dt: f64, time: f32) {
        for a in &mut self.arcs {
            if !a.alive { continue; }
            a.t += dt;
            let k = a.t / a.dur;
            a.p = k.min(1.0) as f32;
            a.life = ((a.t - a.dur) / 0.14).max(0.0) as f32;
            if a.t > a.dur + 0.14 { a.alive = false; }
        }
        step_parts(&mut self.parts, &mut self.add_inst, &mut self.norm_inst, dt as f32);
        step_links(&mut self.links, &mut self.link_inst, dt);
        for s in &mut self.timed {
            if s.alive { step_timed(s, dt, time); }
        }
    }

    pub(crate) fn upload(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        // 弧・亀裂・輪・柱・球を 1 つの buffer へ種類ごとに並べる
        self.fx_inst.clear();
        for a in &self.arcs {
            if !a.alive { continue; }
            // uTime は TS でも更新していない（筋の模様は止まったまま）ので 0 のまま
            self.fx_inst.push(fx(a.model, [a.from, a.to, a.r, a.width], [a.p, a.life, a.alpha, 0.0], with(a.core, 0.0), with(a.edge, 0.0)));
        }
        self.r_arcs = 0..self.fx_inst.len() as u32;
        let mut ranges = [0..0u32, 0..0, 0..0, 0..0];
        for (n, kind) in [Kind::Crack, Kind::Ring, Kind::Pillar, Kind::Ball].into_iter().enumerate() {
            let start = self.fx_inst.len() as u32;
            for s in &self.timed {
                if s.alive && s.kind == kind { self.fx_inst.push(timed_inst(s)); }
            }
            ranges[n] = start..self.fx_inst.len() as u32;
        }
        let [c, r, p, b] = ranges;
        self.r_crack = c; self.r_ring = r; self.r_pillar = p; self.r_ball = b;
        write_fx(device, queue, &mut self.fx_buf, &self.fx_inst);
        if !self.add_inst.is_empty() { queue.write_buffer(&self.add_buf, 0, bytemuck::cast_slice(&self.add_inst)); }
        if !self.norm_inst.is_empty() { queue.write_buffer(&self.norm_buf, 0, bytemuck::cast_slice(&self.norm_inst)); }
        if !self.link_inst.is_empty() { queue.write_buffer(&self.link_buf, 0, bytemuck::cast_slice(&self.link_inst)); }
        if !self.taffy_inst.is_empty() { queue.write_buffer(&self.taffy_buf, 0, bytemuck::cast_slice(&self.taffy_inst)); }
        self.taffy_n = self.taffy_inst.len() as u32;
        self.taffy_inst.clear();
    }

    pub fn draw(&self, _gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, order: i32) { self.draw_pass(pass, order); }

    pub(crate) fn draw_pass(&self, pass: &mut wgpu::RenderPass<'_>, order: i32) {
        match order {
            2 => draw_mesh(pass, &self.p_crack, &self.ring_geo, &self.fx_buf, self.r_crack.clone()),
            3 => draw_mesh(pass, &self.p_tel, &self.ring_geo, &self.tel_buf, 0..self.tel_list.len() as u32),
            4 => {
                draw_mesh(pass, &self.p_ring, &self.ring_geo, &self.fx_buf, self.r_ring.clone());
                draw_mesh(pass, &self.p_pillar, &self.pillar_geo, &self.fx_buf, self.r_pillar.clone());
                draw_mesh(pass, &self.p_ball, &self.ball_geo, &self.fx_buf, self.r_ball.clone());
            }
            5 => draw_mesh(pass, &self.p_part_norm, &self.quad, &self.norm_buf, 0..self.norm_inst.len() as u32),
            6 => draw_mesh(pass, &self.p_arc, &self.arc_geo, &self.fx_buf, self.r_arcs.clone()),
            7 => {
                if self.trail_on > 0.01 {
                    pass.set_pipeline(&self.p_trail);
                    pass.set_vertex_buffer(0, self.trail_vb.slice(..));
                    pass.set_vertex_buffer(1, self.trail_u.slice(..));
                    pass.set_index_buffer(self.trail_ib.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..self.trail_i_n, 0, 0..1);
                }
            }
            8 => draw_mesh(pass, &self.p_part_add, &self.quad, &self.add_buf, 0..self.add_inst.len() as u32),
            9 => {
                draw_mesh(pass, &self.p_taffy, &self.quad, &self.taffy_buf, 0..self.taffy_n);
                draw_mesh(pass, &self.p_link, &self.quad, &self.link_buf, 0..self.link_inst.len() as u32);
            }
            _ => {}
        }
    }

    pub fn particle_count(&self) -> usize { self.parts.len() }
    pub fn arcs_alive(&self) -> usize { self.arcs.iter().filter(|a| a.alive).count() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn burst(type_: u8) -> Burst<'static> { Burst { n: 1, color: [1.0, 0.5, 0.25], type_, ..Default::default() } }

    // 省いた値が 0 になると粒が動かない・すぐ消える。type_ ごとの既定と、明示した 0 の両方を守る
    #[test]
    fn burst_defaults_match_ts() {
        let mut rng = Rng(1);
        let mut parts = VecDeque::new();
        for (t, g, s, d) in [(0u8, 9.0, 0.22, 1.2), (1, 9.0, 0.0, 1.2), (3, 14.0, 0.0, 1.2), (4, -0.6, 0.0, 3.0)] {
            parts.clear();
            emit(&mut parts, &mut rng, &burst(t));
            let p = parts[0];
            assert_eq!((p.grav, p.stretch, p.drag), (g, s, d));
            assert!(p.life >= 0.5 * 0.6 && p.life < 0.5 * 1.3);
        }
        parts.clear();
        emit(&mut parts, &mut rng, &Burst { gravity: 0.0, ..burst(0) });
        assert_eq!(parts[0].grav, 0.0);
    }

    // 上限で古い粒から消える（TS の parts.shift）。束は各 PMAX までしか詰めない
    #[test]
    fn particle_cap_drops_oldest() {
        let mut rng = Rng(7);
        let mut parts = VecDeque::new();
        emit(&mut parts, &mut rng, &Burst { n: PMAX as u32 * 2 - 10, x: 1.0, life: 100.0, ..burst(0) });
        emit(&mut parts, &mut rng, &Burst { n: 5, x: 2.0, life: 100.0, ..burst(0) });
        assert_eq!(parts.len(), PMAX * 2 - 10);
        assert_eq!(parts.iter().filter(|p| p.x == 2.0).count(), 5);
        let (mut a, mut b) = (Vec::new(), Vec::new());
        step_parts(&mut parts, &mut a, &mut b, 0.001);
        assert_eq!(a.len(), PMAX);
        assert!(a[0].pos[0] > 1.5, "新しい粒から詰める");
        assert!(b.is_empty());
    }

    // 粒は寿命で消え、紙吹雪は加算の束へ入らず、床で跳ねる
    #[test]
    fn particles_die_and_sort_into_bundles() {
        let mut rng = Rng(3);
        let mut parts = VecDeque::new();
        emit(&mut parts, &mut rng, &Burst { n: 3, y: 0.0, speed: 0.0, life: 1.0, ..burst(3) });
        emit(&mut parts, &mut rng, &Burst { n: 2, life: 0.01, ..burst(0) });
        let (mut a, mut b) = (Vec::new(), Vec::new());
        step_parts(&mut parts, &mut a, &mut b, 0.1);
        assert_eq!((a.len(), b.len(), parts.len()), (0, 3, 3));
        assert!(parts.iter().all(|p| p.y >= 0.03));
    }

    // 連鎖が終わると同じ chain の線だけ明るくなり、寿命が巻き戻る
    #[test]
    fn link_flash_only_touches_chain() {
        let mk = |chain| Link { ax: 0.0, ay: 0.0, az: 0.0, bx: 1.0, by: 0.0, bz: 0.0, r: 1.0, g: 1.0, b: 1.0, age: 1.0, life: 1.1, w: 0.3, chain, boost: 0.0 };
        let mut links = vec![mk(1), mk(2)];
        flash_links(&mut links, 1);
        let mut inst = Vec::new();
        step_links(&mut links, &mut inst, 0.2);
        assert_eq!(links.len(), 1, "光らなかった線は寿命で消える");
        assert_eq!(links[0].chain, 1);
    }

    #[test]
    fn rng_stays_in_unit_range() {
        let mut rng = Rng(0x9e37_79b9);
        for _ in 0..10_000 {
            let v = rng.next();
            assert!((0.0..1.0).contains(&v));
        }
    }
}
