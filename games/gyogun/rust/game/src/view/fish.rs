// 浜辺の魚（docs/gyogun-spec.md）: Fish Kit（CC0。public/models/fish/）の写真の魚を、種類ごとのインスタンス描画で泳がせる。
// 遊びの計算（sim）の敵の種類ごとに魚を当てる。うねりは shaders/fish.wgsl。
// 大将（黄肌大王）も同じシェーダーで描く 1 匹の巨大なキハダ（FishBoss）。
use crate::assets::load_glb;
use crate::geom::{Geo, Vertex};
use crate::gfx::{hex, vert_layout, Gfx, MeshBuf, Pipe};
use crate::view::materials::ORDER_OPAQUE;
use bytemuck::{Pod, Zeroable};
use glam::{EulerRot, Mat4, Quat, Vec3};
use sim::battle::{Boss, BossState, Enemy, EnemyKind, EnemyState, Role, KINDS};
use std::f32::consts::{PI, TAU};
use wgpu::util::DeviceExt;

/// 敵の種類 → 魚（ファイル名）・長さ（m）・泳ぐ高さ（体の真ん中, m）・上限の数・きらめきの色
const FISH_LIST: [(EnemyKind, &str, f32, f32, u32, &str); 9] = [
    (EnemyKind::Strawberry, "aji_lo", 2.6, 1.35, 140, "#bfe8ff"),
    (EnemyKind::Macaron, "saba_lo", 2.9, 1.4, 140, "#9fe0d0"),
    (EnemyKind::Mikan, "mebaru_lo", 2.4, 1.3, 140, "#ffc890"),
    (EnemyKind::Creampuff, "isaki_lo", 2.6, 1.35, 140, "#e0e8c8"),
    (EnemyKind::Cottoncandy, "katakuchi_lo", 2.5, 1.6, 140, "#c8e0ff"),
    (EnemyKind::Donut, "kasago_lo", 2.4, 1.25, 70, "#ff9a7a"),
    (EnemyKind::Pudding, "madai", 3.6, 1.7, 60, "#ff8fa8"),
    (EnemyKind::Shortcake, "buri", 6.2, 2.7, 4, "#ffe08a"),
    (EnemyKind::Cakeking, "hiramasa", 6.6, 2.8, 4, "#ffe08a"),
];
/// 体を太らせる倍率（横幅・高さ）。写真の魚は正面から来ると薄く、夕日の砂に溶けるので、ずんぐりさせて的として見やすくする
const FAT: (f32, f32) = (1.6, 1.25);
/// 大将の魚と長さ
const BOSS_FISH: &str = "kihada";
const BOSS_LEN: f32 = 11.0;

pub fn path(name: &str) -> String { format!("models/fish/{name}.glb") }
/// 先に読んでおく模型
pub fn files() -> Vec<String> {
    let mut v: Vec<String> = FISH_LIST.iter().map(|f| path(f.1)).collect();
    v.push(path(BOSS_FISH));
    v
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct FishInst {
    model: [[f32; 4]; 4],
    fx: [f32; 4],
    swim: [f32; 4],
}
const FISH_ATTRS: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![8 => Float32x4, 9 => Float32x4, 10 => Float32x4, 11 => Float32x4, 12 => Float32x4, 13 => Float32x4];
fn fish_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<FishInst>() as u64, step_mode: wgpu::VertexStepMode::Instance, attributes: &FISH_ATTRS }
}

/// 模型と、そのうねりの形（Fish Kit の glb の swim: env = 包絡, lambda = 波長）
struct Model { geo: Geo, tex: (u32, u32, Vec<u8>), env: [f32; 4] }

/// 魚の glb を読み、口先を +Z・長さ 1・体の真ん中を原点にそろえる（Fish Kit の模型は口先が +X・メートル）
fn load_fish(name: &str) -> Result<Model, String> {
    let g = load_glb(&path(name))?;
    let ni = g.nodes.iter().position(|n| n.mesh.is_some()).ok_or_else(|| format!("{name}: メッシュが無い"))?;
    let p = g.meshes[g.nodes[ni].mesh.unwrap()].primitives.first().ok_or_else(|| format!("{name}: 面が無い"))?;
    let (lo, hi) = p.positions.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), q| (a.min(Vec3::from(*q)), b.max(Vec3::from(*q))));
    let c = (lo + hi) * 0.5;
    let len = (hi.x - lo.x).max(1e-4);
    // +X → +Z（y 軸で −90°）: (x, y, z) → (−z, y, x)
    let turn = |v: Vec3| Vec3::new(-v.z, v.y, v.x);
    let v = (0..p.positions.len()).map(|k| Vertex {
        pos: (turn(Vec3::from(p.positions[k]) - c) / len).to_array(),
        normal: turn(Vec3::from(p.normals.get(k).copied().unwrap_or([0.0, 1.0, 0.0]))).normalize_or_zero().to_array(),
        uv: p.uvs.get(k).copied().unwrap_or([0.0, 0.0]),
        color: [1.0; 4], sway: 0.0,
    }).collect();
    let tex = p.material.and_then(|m| g.materials[m].texture).map(|t| { let im = &g.images[t]; (im.width, im.height, im.rgba.clone()) }).ok_or_else(|| format!("{name}: 模様が無い"))?;
    // うねりの形は Fish Kit のアジの値（どの魚もほぼ同じ。尾の付け根から先が大きく振れる）
    Ok(Model { geo: Geo { v, i: p.indices.clone() }, tex, env: [0.015, -0.07, 0.16, 1.0] })
}

struct Pipes { pipe: wgpu::RenderPipeline, line: wgpu::RenderPipeline, shadow: wgpu::RenderPipeline, kind_layout: wgpu::BindGroupLayout }
fn pipes(gfx: &Gfx) -> Pipes {
    let shader = gfx.shader("fish", include_str!("../../shaders/fish.wgsl"));
    let kind_layout = gfx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("fish kind"), entries: &[wgpu::BindGroupLayoutEntry {
        binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
    }] });
    let vb = [vert_layout(), fish_layout()];
    let layouts = [&gfx.globals_layout, &gfx.shadow_layout, &gfx.tex_layout, &kind_layout];
    // ひれは切り抜いた板なので両面
    let pipe = Pipe::scene("fish", &shader, "vs", "fs", &layouts, &vb).double().build(&gfx.device);
    let mut line = Pipe::scene("fish line", &shader, "vs_line", "fs_line", &layouts, &vb);
    line.cull = Some(wgpu::Face::Front);
    let line = line.build(&gfx.device);
    let empty = gfx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[] });
    let shadow = Pipe::shadow("fish shadow", &shader, "vs_shadow", &[&gfx.globals_layout, &empty, &empty, &kind_layout], &vb).build(&gfx.device);
    Pipes { pipe, line, shadow, kind_layout }
}

struct Batch {
    mesh: MeshBuf,
    tex: wgpu::BindGroup,
    kind_bg: wgpu::BindGroup,
    inst: wgpu::Buffer,
    data: Vec<FishInst>,
    cap: usize,
}
fn batch(gfx: &Gfx, p: &Pipes, m: &Model, sparkle: [f32; 3], captain: bool, cap: usize) -> Batch {
    let view = gfx.texture(m.tex.0, m.tex.1, &m.tex.2, true);
    let u: [f32; 12] = [sparkle[0], sparkle[1], sparkle[2], 1.0, if captain { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0, m.env[0], m.env[1], m.env[2], m.env[3]];
    let ub = gfx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("fish kind"), contents: bytemuck::cast_slice(&u), usage: wgpu::BufferUsages::UNIFORM });
    let kind_bg = gfx.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &p.kind_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: ub.as_entire_binding() }] });
    Batch { mesh: gfx.mesh(&m.geo), tex: gfx.tex_bind(&view), kind_bg, inst: gfx.vbuf((cap * std::mem::size_of::<FishInst>()) as u64), data: Vec::with_capacity(cap), cap }
}
fn draw_batch(gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, p: &Pipes, b: &Batch) {
    if b.data.is_empty() { return; }
    pass.set_pipeline(&p.pipe);
    pass.set_bind_group(1, &gfx.shadow_bg, &[]);
    pass.set_bind_group(2, &b.tex, &[]);
    pass.set_bind_group(3, &b.kind_bg, &[]);
    pass.set_vertex_buffer(0, b.mesh.vb.slice(..));
    pass.set_vertex_buffer(1, b.inst.slice(..));
    pass.set_index_buffer(b.mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..b.mesh.n, 0, 0..b.data.len() as u32);
    pass.set_pipeline(&p.line);
    pass.draw_indexed(0..b.mesh.n, 0, 0..b.data.len() as u32);
}
fn shadow_batch(pass: &mut wgpu::RenderPass<'_>, p: &Pipes, b: &Batch, empty: &wgpu::BindGroup) {
    if b.data.is_empty() { return; }
    pass.set_pipeline(&p.shadow);
    pass.set_bind_group(1, empty, &[]);
    pass.set_bind_group(2, empty, &[]);
    pass.set_bind_group(3, &b.kind_bg, &[]);
    pass.set_vertex_buffer(0, b.mesh.vb.slice(..));
    pass.set_vertex_buffer(1, b.inst.slice(..));
    pass.set_index_buffer(b.mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..b.mesh.n, 0, 0..b.data.len() as u32);
}
fn empty_bg(gfx: &Gfx) -> wgpu::BindGroup {
    let l = gfx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[] });
    gfx.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &l, entries: &[] })
}

fn fish_of(kind: EnemyKind) -> &'static (EnemyKind, &'static str, f32, f32, u32, &'static str) {
    FISH_LIST.iter().find(|f| f.0 == kind).unwrap_or(&FISH_LIST[0])
}
/// 魚の長さ（m）。足元の丸影の大きさに使う
pub fn length_of(kind: EnemyKind) -> f32 { fish_of(kind).2 }

/// 魚の群れ
pub struct School {
    p: Pipes,
    batches: Vec<(EnemyKind, Batch)>,
    empty: wgpu::BindGroup,
}

impl School {
    pub fn new(gfx: &Gfx) -> Result<Self, String> {
        let p = pipes(gfx);
        let mut batches = Vec::new();
        for &(kind, name, _, _, cap, sp) in FISH_LIST.iter() {
            let m = load_fish(name)?;
            batches.push((kind, batch(gfx, &p, &m, hex(sp), KINDS[kind as usize].role == Role::Captain, cap as usize)));
        }
        Ok(School { p, batches, empty: empty_bg(gfx) })
    }
    pub fn clear(&mut self) { for b in &mut self.batches { b.1.data.clear(); } }
    /// 1 匹足す（fx は群れと同じ値）。上限なら足さない
    pub fn push(&mut self, e: &Enemy, time: f32, fx: [f32; 4]) {
        let Some((_, b)) = self.batches.iter_mut().find(|b| b.0 == e.kind) else { return };
        if b.data.len() >= b.cap { return; }
        let (m, swim) = pose(e, time);
        b.data.push(FishInst { model: m.to_cols_array_2d(), fx, swim });
    }
    pub fn upload(&self, gfx: &Gfx) {
        for (_, b) in &self.batches { if !b.data.is_empty() { gfx.queue.write_buffer(&b.inst, 0, bytemuck::cast_slice(&b.data)); } }
    }
    pub fn shadow(&self, pass: &mut wgpu::RenderPass<'_>) { for (_, b) in &self.batches { shadow_batch(pass, &self.p, b, &self.empty); } }
    pub fn draw(&self, gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, order: i32) {
        if order != ORDER_OPAQUE { return; }
        for (_, b) in &self.batches { draw_batch(gfx, pass, &self.p, b); }
    }
    pub fn drawn(&self) -> usize { self.batches.iter().map(|b| b.1.data.len()).sum() }
}

/// 1 匹の姿勢（体の真ん中の位置・向き・大きさ）と、うねり (cyc, 振れ, 曲げ, 0)
pub fn pose(e: &Enemy, time: f32) -> (Mat4, [f32; 4]) {
    let &(_, _, len, hover, _, _) = fish_of(e.kind);
    let id = e.id as f32;
    let lt = time + id * 0.731;
    let st = e.st as f32;
    let spin = e.spin as f32;
    let captain = KINDS[e.kind as usize].role == Role::Captain;
    let (mut yaw, mut rx, mut rz) = (e.face as f32, 0.0f32, 0.0f32);
    let mut y = e.y as f32 + hover;
    let mut fwd = 0.0f32;
    let mut s = 1.0f32;
    // うねり: 尾を振る速さ（回/秒）・振れ・曲げ
    let (mut freq, mut amp, mut bend) = (1.2f32, 0.7f32, 0.0f32);
    match e.state {
        EnemyState::Idle => { y += 0.06 * (lt * 1.7).sin(); rz = 0.05 * (lt * 1.1).sin(); }
        EnemyState::Chase => {
            freq = 2.4 + (e.id % 3) as f32 * 0.3; amp = 1.05;
            y += 0.1 * (lt * 2.3).sin();
            rx = -0.05 * (lt * 2.3).cos();
            // 逃げる: 速く大きく、身をよじって
            if e.flee >= 0 { freq = 4.2; amp = 1.5; rz = 0.25 * (lt * 9.0).sin(); }
        }
        EnemyState::Windup => {
            // 構え: 頭を上げて体を反らし、尾を巻く（後ろへ少し下がる）
            let k = (st / 0.8).min(1.0);
            rx = -0.45 * k; fwd = -0.25 * k * len; bend = 0.9 * k * if e.id % 2 == 0 { 1.0 } else { -1.0 };
            freq = 1.0; amp = 0.5; rz = 0.08 * (time * 40.0 + id).sin() * k;
        }
        EnemyState::Strike => {
            // 攻撃: 頭から突っ込む
            let k = (1.0 - st / 0.3).max(0.0);
            rx = 0.3 * k; fwd = 0.45 * k * len; freq = 5.0; amp = 1.4;
        }
        EnemyState::Recover => { rx = 0.15 * (1.0 - st / 0.5).max(0.0); freq = 1.6; }
        EnemyState::Stagger => {
            let k = (1.0 - st / 0.38).max(0.0);
            rx = -0.4 * k; rz = 0.4 * k * (st * 30.0).sin(); freq = 6.0; amp = 1.6;
        }
        // 吹き飛ぶ: 回りながら、じたばた
        EnemyState::Air => { rx = spin; rz = spin * 0.35; freq = 6.0; amp = 1.6; }
        // 巨人の体に積み上がっている: 押し合いながらその場で泳ぐ
        EnemyState::Stacked => { freq = 2.0; amp = 0.8; rz = 0.1 * (lt * 3.0).sin(); y -= hover * 0.7; }
        // 担がれている: 腹を上にして暴れる
        EnemyState::Carried => { rz = PI + 0.25 * (time * 11.0).sin(); freq = 5.0; amp = 1.8; y = e.y as f32; }
        // 倒れる: 砂の上で横になって跳ねる
        EnemyState::Down => {
            rz = PI * 0.5; y = e.y as f32 + 0.14 * len + 0.12 * len * (lt * 7.0).sin().abs() * 0.5;
            freq = 2.6; amp = 1.9;
        }
        EnemyState::Getup => {
            let k = (st / 0.35).min(1.0);
            rz = PI * 0.5 * (1.0 - k * k);
            y = e.y as f32 + 0.14 * len + (hover - 0.14 * len) * k;
            freq = 4.0; amp = 1.5;
        }
        EnemyState::Dead => {
            rx = spin; rz = spin * 0.6; freq = 7.0; amp = 2.0;
            let end = if captain { 1.6 } else { 0.95 };
            let k = e.dead_t as f32 / end;
            // 消える直前にふくらんで、ぽんと消える
            let pop = if k > 0.78 { 1.0 + (k - 0.78) * 2.6 } else { 1.0 };
            let shrink = if k > 0.9 { (1.0 - (k - 0.9) * 10.0).max(0.0) } else { 1.0 };
            s = pop * shrink;
            // 地面にめり込まない
            y = y.max(0.2 * len * s);
        }
    }
    let hit_t = e.hit_t as f32;
    if hit_t < 0.14 { let k = 1.0 - hit_t / 0.14; s *= 1.0 + 0.12 * k; amp += 1.2 * k; }
    if captain { freq *= 0.6; }
    let q = Quat::from_euler(EulerRot::YXZ, yaw, rx, rz);
    yaw = yaw.rem_euclid(TAU);
    let dir = Vec3::new(yaw.sin(), 0.0, yaw.cos());
    let p = Vec3::new(e.x as f32, y, e.z as f32) + dir * fwd;
    let m = Mat4::from_scale_rotation_translation(Vec3::new(len * s * FAT.0, len * s * FAT.1, len * s), q, p);
    (m, [lt * freq, amp, bend, 0.0])
}

// ---- 大将（黄肌大王） ----

/// 溜めはゆっくり始まり、当たりへ向けて加速する
fn windup(u: f32) -> f32 { u.clamp(0.0, 1.0).powf(2.2) }
fn settle(u: f32) -> f32 { let u = u.clamp(0.0, 1.0); u * u * (3.0 - 2.0 * u) }
/// 跳んで着地するまでの秒（sim の LEAP_T と同じ）
const LEAP_T: f32 = 1.3;
/// 宙を泳ぐ高さ（体の真ん中）
const BOSS_HOVER: f32 = 3.4;

/// 大将の姿勢: 状態と秒 → (向きのずれ, 頭の上げ下げ, 横倒し, 高さの足し, 曲げ, 尾を振る速さ, 振れ)。
/// 予告が当たる秒に当たりの姿勢が来る（尾の薙ぎ = 尾が振り抜ける、腹落ち = 着地）
pub fn boss_pose(s: BossState, t: f32, angry: bool, side: f32) -> [f32; 7] {
    let k = if angry { 0.8 } else { 1.0 };
    match s {
        BossState::Dormant | BossState::Idle => [0.0, -0.04, 0.05 * (t * 1.3).sin(), 0.25 * (t * 1.6).sin(), 0.0, 0.7, 0.6],
        BossState::Walk => [0.0, 0.0, 0.0, 0.2 * (t * 2.2).sin(), 0.0, 1.3, 0.9],
        BossState::Hit => [0.0, -0.1, 0.12 * (t * 30.0).sin(), 0.0, 0.0, 3.0, 1.4],
        // 跳ぶ間は頭を上げ、落ちながら頭を下げる。着地のあとは頭を上げて吠える
        BossState::Enter => if t < LEAP_T { [0.0, -0.6 + 1.0 * (t / LEAP_T), 0.0, 0.0, 0.0, 2.5, 1.2] } else { let r = settle((t - LEAP_T) / 0.6); [0.0, -0.9 * r, 0.0, 1.6 * r, 0.0, 2.0, 1.2] },
        // 尾の薙ぎ払い（拳の代わり）: 尾を反対へ溜めて、予告の秒に振り抜く
        BossState::PunchL | BossState::PunchR => {
            let fire = 0.95 * k;
            let sd = if s == BossState::PunchL { 1.0 } else { -1.0 } * side;
            if t < fire { let w = windup(t / fire); [sd * 0.55 * w, -0.1 * w, 0.0, 0.0, sd * 1.1 * w, 0.8, 0.4] }
            else { let a = ((t - fire) / 0.12).min(1.0); let r = 1.0 - settle((t - fire - 0.12) / (1.9 - fire - 0.12)); [sd * (0.55 - 1.4 * a) * r, -0.1 * r, 0.0, 0.0, sd * (1.1 - 2.4 * a) * r, 3.0, 1.0] }
        }
        // 腹から落ちる（足踏みの代わり）: 跳び上がって、予告の秒にちょうど着地する
        BossState::Stomp => {
            let fire = 1.35 * k;
            let u = ((t - fire * 0.35) / (fire * 0.65)).clamp(0.0, 1.0);
            let hop = 4.0 * u * (1.0 - u) * 6.5;
            let crouch = windup(t / (fire * 0.35)) * (1.0 - u);
            if t < fire { [0.0, -0.5 * crouch + 0.35 * u, 0.0, hop - 0.4 * crouch, 0.0, 2.2, 1.1] }
            else { let (a, r) = (((t - fire) / 0.08).min(1.0), settle((t - fire) / 0.6)); [0.0, 0.35 * (1.0 - r), 0.0, -0.9 * a * (1.0 - r), 0.0, 1.0, 0.6] }
        }
        // 体ごと回る（たれの代わり）
        BossState::Sauce => [TAU * settle((t - 0.2) / 1.8), 0.0, 0.35 * (t * PI / 2.4).sin(), 0.6, 0.0, 3.5, 1.3],
        // 頭を上げて立ち上がる（咆哮）
        BossState::Roar => { let r = settle(t / 0.5); [0.0, -1.1 * r, 0.06 * (t * 24.0).sin() * r, 2.2 * r, 0.0, 3.0, 1.0] }
        // 崩れる: 砂の上へ横倒し
        BossState::Down => { let r = settle(t / 1.1); [0.0, 0.0, PI * 0.5 * r, -(BOSS_HOVER - 0.11 * BOSS_LEN) * r, 0.0, 2.2, 1.6] }
        BossState::Dead => [0.0, 0.0, PI * 0.5, -(BOSS_HOVER - 0.11 * BOSS_LEN), 0.0, 1.0 / (1.0 + t), 1.2 / (1.0 + t * 2.0)],
    }
}

pub struct FishBoss {
    p: Pipes,
    b: Batch,
    empty: wgpu::BindGroup,
    pub visible: bool,
    pub pos: Vec3,
    pub yaw: f64,
    pub hop: f32,
    model: Mat4,
    state: Option<BossState>,
    side: f32,
    t: f32,
}

impl FishBoss {
    pub fn new(gfx: &Gfx) -> Result<Self, String> {
        let p = pipes(gfx);
        let m = load_fish(BOSS_FISH)?;
        let b = batch(gfx, &p, &m, hex("#ffe08a"), true, 1);
        Ok(FishBoss { p, b, empty: empty_bg(gfx), visible: false, pos: Vec3::ZERO, yaw: 0.0, hop: 0.0, model: Mat4::IDENTITY, state: None, side: 1.0, t: 0.0 })
    }
    /// 体の上の点（模型の中の位置。長さ 1・口先 +Z）
    fn at(&self, p: Vec3) -> Option<Vec3> { self.visible.then(|| self.model.transform_point3(p)) }
    /// 頭（口先の少し上）
    pub fn head(&self) -> Vec3 { self.at(Vec3::new(0.0, 0.12, 0.42)).unwrap_or(self.pos) }
    /// 胸びれ（拳の光の代わり）
    pub fn fin(&self, left: bool) -> Option<Vec3> { self.at(Vec3::new(if left { 0.12 } else { -0.12 }, -0.04, 0.22)) }
    /// 腹（足の光の代わり）
    pub fn belly(&self, left: bool) -> Option<Vec3> { self.at(Vec3::new(if left { 0.05 } else { -0.05 }, -0.2, if left { 0.05 } else { -0.15 })) }

    /// y = 跳ぶ高さ（sim の boss_y）。flash = 当たりの白い光、angry = 怒りの赤い光
    pub fn update(&mut self, gfx: &Gfx, b: &Boss, dt: f64, frozen: bool, y: f32, flash: f32, angry: f32) {
        self.visible = b.state != BossState::Dormant;
        if !self.visible { return; }
        // 尾の薙ぎは、出るたびに左右を替える
        if self.state != Some(b.state) && matches!(b.state, BossState::PunchL | BossState::PunchR) { self.side = -self.side; }
        self.state = Some(b.state);
        if !frozen { self.t += dt as f32; }
        let [yaw_off, rx, rz, dy, bend, freq, amp] = boss_pose(b.state, b.st as f32, b.hp / b.max_hp < 0.4, self.side);
        let tau = std::f64::consts::TAU;
        let mut d = (b.face - self.yaw) % tau;
        if d > std::f64::consts::PI { d -= tau; }
        if d < -std::f64::consts::PI { d += tau; }
        self.yaw += d * (dt * 6.0).min(1.0);
        self.hop = dy.max(0.0);
        self.pos = Vec3::new(b.x as f32, y + BOSS_HOVER + dy, b.z as f32);
        let hit_t = b.hit_t as f32;
        let shake = (hit_t * 80.0).sin() * 0.1 * (1.0 - hit_t / 0.15).max(0.0);
        let q = Quat::from_euler(EulerRot::YXZ, self.yaw as f32 + yaw_off, rx, rz + shake);
        self.model = Mat4::from_scale_rotation_translation(Vec3::splat(BOSS_LEN), q, self.pos);
        let inst = FishInst { model: self.model.to_cols_array_2d(), fx: [flash, angry * 0.5, 0.0, 1.0], swim: [self.t * freq * 0.7, amp, bend, 0.0] };
        self.b.data.clear();
        self.b.data.push(inst);
        gfx.queue.write_buffer(&self.b.inst, 0, bytemuck::bytes_of(&inst));
    }
    pub fn shadow(&self, pass: &mut wgpu::RenderPass<'_>) { if self.visible { shadow_batch(pass, &self.p, &self.b, &self.empty); } }
    pub fn draw(&self, gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, order: i32) {
        if self.visible && order == ORDER_OPAQUE { draw_batch(gfx, pass, &self.p, &self.b); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 尾の薙ぎは予告が当たる秒に振り抜く（溜めの向きから反対へ移り始める）。腹落ちは予告の秒に着地する
    #[test]
    fn 大将の当たりの姿勢は予告の秒に来る() {
        let before = boss_pose(BossState::PunchL, 0.94, false, 1.0);
        let after = boss_pose(BossState::PunchL, 0.95 + 0.12, false, 1.0);
        assert!(before[0] > 0.4 && after[0] < -0.5, "{} → {}", before[0], after[0]);
        let mid = boss_pose(BossState::Stomp, 0.9, false, 1.0)[3];
        let land = boss_pose(BossState::Stomp, 1.35, false, 1.0)[3];
        assert!(mid > 3.0 && land.abs() < 0.05, "{mid} {land}");
        // 体力 40% 未満は 0.8 倍に速まる
        assert!(boss_pose(BossState::Stomp, 1.35 * 0.8, true, 1.0)[3].abs() < 0.05);
    }
}
