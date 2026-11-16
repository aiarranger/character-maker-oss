// 戦場: 夕暮れの浜辺（砂浜の島を海が囲む。docs/gyogun-spec.md）。地面（道と砦の広場を描いた大きな絵）・海・砦（柵・幟・灯籠・陣幕）・本陣の門・岩とススキ・幟の列・かがり火。
// three は 1 つずつ Mesh にするが、ここは同じ型をまとめてインスタンス描画にする。
use crate::assets;
use crate::geom::{self, Geo, Vertex};
use crate::gfx::{self, FxInst, Gfx, MeshBuf, Pipe, hex, lerp3};
use crate::raster::{self, Canvas};
use crate::view::materials::{ORDER_OPAQUE, Soft, SoftParams, draw_fx};
use glam::{Mat4, Quat, Vec3};
use sim::battle::{FIELD_R, GATE, Owner, START, Stronghold};
use std::collections::BTreeMap;
use std::f32::consts::{PI, TAU};
use std::ops::Range;

const ENEMY: &str = "#ff4f7a";
const ALLY: &str = "#ffc94a";
const CLOTH_MARK: &str = "#fff4e6";
const CLOTH_MARK_ALLY: &str = "#3a2240";
const FIRE_ENEMY: &str = "#ff6a3a";
const FIRE_ALLY: &str = "#ffd46a";
const POLE: &str = "#3a2a26";
const TRIM: &str = "#f3e2c8";
const DOOR: &str = "#3a2228";

/// 影の範囲（app.ts の key.shadow.camera と key.position）。主役を追う
const SHADOW_HALF: f32 = 34.0;
const SHADOW_NEAR: f32 = 1.0;
const SHADOW_FAR: f32 = 160.0;
pub fn light_eye(hero: (f32, f32)) -> Vec3 { Vec3::new(hero.0 + 30.0, 50.0, hero.1 + 18.0) }

/// Math.random の代わり（置き方は毎回同じ）
struct Rng(u32);
impl Rng {
    fn next(&mut self) -> f32 {
        if let Some(v) = crate::platform::js_random() { return v; }
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / 16_777_216.0
    }
}

fn hold_xz(h: &Stronghold) -> (f32, f32) { (h.x as f32, h.z as f32) }

/// 海の始まる半径（砂浜の島を海が囲む）
const SHORE_R: f32 = 100.0;

/// 浜辺の地面の絵: 夕日の砂の濃淡・踏み固めた道・砦の広場・波打ち際の濡れた砂・貝殻
fn sand_texture(holds: &[Stronghold], rng: &mut Rng) -> Canvas {
    const N: usize = 2048;
    const W: f32 = 240.0;
    let nf = N as f32;
    let px = |x: f32| (x + W / 2.0) / W * nf;
    let mut g = Canvas::new(N, N);
    g.fill(0xc8 as f32, 0xa6 as f32, 0x7c as f32);
    // 砂の濃淡（明るい乾いた砂・湿った砂・夕日の桃色）
    for _ in 0..2600 {
        let (x, y, r) = (rng.next() * nf, rng.next() * nf, 10.0 + rng.next() * 80.0);
        let hue = if rng.next() < 0.4 { (222.0, 196.0, 152.0) } else if rng.next() < 0.5 { (170.0, 140.0, 104.0) } else if rng.next() < 0.5 { (214.0, 168.0, 150.0) } else { (196.0, 176.0, 134.0) };
        g.radial(x, y, r, &[(0.0, hue.0, hue.1, hue.2, 0.22 + rng.next() * 0.25), (1.0, hue.0, hue.1, hue.2, 0.0)]);
    }
    // 風の筋（砂紋）
    for i in 0..220 {
        let y = rng.next() * nf;
        let x = rng.next() * nf;
        let w = 40.0 + rng.next() * 160.0;
        let c = if i % 2 == 0 { (250.0, 232.0, 196.0, 0.18) } else { (190.0, 158.0, 118.0, 0.14) };
        g.polyline(&[(x, y), (x + w * 0.5, y + 3.0), (x + w, y)], 2.0, c);
    }
    // 道（踏み固めた砂）: 出陣の地 → 各砦 → 本陣
    let road = |g: &mut Canvas, pts: &[(f32, f32)], w: f32| {
        let line: Vec<(f32, f32)> = pts.iter().map(|&(x, z)| (px(x), px(z))).collect();
        for (col, k) in [((170.0, 138.0, 102.0, 0.5), 1.25), ((196.0, 164.0, 124.0, 0.6), 1.0), ((214.0, 186.0, 146.0, 0.4), 0.55)] {
            g.polyline(&line, w * k / W * nf, col);
        }
    };
    let (gate_x, gate_z, start_x, start_z) = (GATE.x as f32, GATE.z as f32, START.x as f32, START.z as f32);
    road(&mut g, &[(start_x, start_z + 10.0), (0.0, 40.0), (0.0, 0.0), (0.0, -40.0), (gate_x, gate_z)], 9.0);
    for h in holds {
        let (x, z) = hold_xz(h);
        road(&mut g, &[(x * 0.25, z * 0.4 + 6.0), (x * 0.65, z * 0.8), (x, z)], 6.0);
    }
    // 砦の広場（流木を敷いた色）
    for h in holds {
        let (x, z) = hold_xz(h);
        let r = (h.r as f32 + 3.0) / W * nf;
        g.radial(px(x), px(z), r, &[(0.0, 176.0, 140.0, 104.0, 0.85), (0.75, 168.0, 132.0, 98.0, 0.7), (1.0, 150.0, 118.0, 88.0, 0.0)]);
    }
    // 本陣の前の石畳
    {
        let (cx, cz, r) = (px(gate_x), px(gate_z + 8.0), 26.0 / W * nf);
        g.radial(cx, cz, r, &[(0.0, 160.0, 150.0, 150.0, 0.9), (0.8, 140.0, 128.0, 128.0, 0.75), (1.0, 120.0, 110.0, 110.0, 0.0)]);
    }
    // 波打ち際へ向かって濡れた砂（暗く、少し青い）
    let (c, r) = (px(0.0), SHORE_R / W * nf);
    g.radial(c, c, r * 1.02, &[(0.0, 120.0, 110.0, 100.0, 0.0), (0.82, 150.0, 128.0, 104.0, 0.0), (0.94, 150.0, 128.0, 104.0, 0.55), (1.0, 110.0, 110.0, 112.0, 0.85)]);
    // 貝殻・小石
    for _ in 0..7000 {
        let (x, y) = (rng.next() * nf, rng.next() * nf);
        let c = if rng.next() < 0.4 { (255.0, 244.0, 236.0, 0.35 + rng.next() * 0.35) } else if rng.next() < 0.5 { (150.0, 120.0, 96.0, 0.2 + rng.next() * 0.3) } else { (250.0, 200.0, 200.0, 0.3 + rng.next() * 0.3) };
        let (w, h) = (1.0 + rng.next() * 3.0, 1.0 + rng.next() * 2.0);
        g.rect(x, y, w, h, c);
    }
    g
}

// ---- 小物（assets.ts の propObject） ----

/// 風で揺れる小物
const SWAYING: [&str; 1] = ["pampas_grass"];

/// 小物の部品 1 つ（three の Mesh 1 つ）。local は小物の原点からの行列
pub struct PropPart {
    pub geo: Geo,
    pub local: Mat4,
    /// three の boundingSphere（geo の中）
    pub center: Vec3,
    pub radius: f32,
}

/// v0 / v1 … の名前か
fn is_variant(name: &str) -> bool {
    name.strip_prefix('v').is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

/// 根のノードから型を選ぶ（aisns の props は v0 / v1 … が型違い、body / glow / rotor が部品）
fn pick_roots(names: &[&str], variant: usize) -> Vec<usize> {
    let want = format!("v{variant}");
    let mut picked: Vec<usize> = (0..names.len()).filter(|&i| names[i] == want).collect();
    if picked.is_empty() { picked = (0..names.len()).filter(|&i| is_variant(names[i])).take(1).collect(); }
    if picked.is_empty() { picked = (0..names.len()).filter(|&i| !names[i].ends_with("_lod1")).collect(); }
    picked
}

/// 小物を 1 つ取り出す（propObject の木の取り方）。材質は prop_params
/// 使う小物の全部（ブラウザでは先に取っておく）
pub const PROP_NAMES: [&str; 4] = ["rock_large", "pampas_grass", "fence_segment", "lantern_stone"];

pub fn prop_parts(name: &str, variant: usize) -> Result<Vec<PropPart>, String> {
    let glb = assets::load_glb(&format!("{}models/wilds/props/{name}.glb", assets::BASE))?;
    let names: Vec<&str> = glb.scene.iter().map(|&n| glb.nodes[n].name.as_str()).collect();
    let sway = SWAYING.contains(&name);
    let mut out = Vec::new();
    fn walk(glb: &crate::glb::Glb, n: usize, m: Mat4, sway: bool, out: &mut Vec<PropPart>) {
        let node = &glb.nodes[n];
        if let Some(mesh) = node.mesh {
            for p in &glb.meshes[mesh].primitives {
                let geo = Geo {
                    v: (0..p.positions.len()).map(|i| Vertex {
                        pos: p.positions[i],
                        normal: p.normals[i],
                        uv: p.uvs[i],
                        color: p.colors.get(i).copied().unwrap_or([1.0; 4]),
                        sway: if sway { p.sway.get(i).copied().unwrap_or(0.0) } else { 0.0 },
                    }).collect(),
                    i: p.indices.clone(),
                };
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for q in &p.positions { lo = lo.min(Vec3::from(*q)); hi = hi.max(Vec3::from(*q)); }
                let center = (lo + hi) * 0.5;
                let radius = p.positions.iter().fold(0.0f32, |r, q| r.max(Vec3::from(*q).distance(center)));
                out.push(PropPart { geo, local: m, center, radius });
            }
        }
        for &c in &node.children { walk(glb, c, m * glb.nodes[c].local(), sway, out); }
    }
    for i in pick_roots(&names, variant) {
        let root = glb.scene[i];
        let node = &glb.nodes[root];
        // c.position.set(0, 0, 0): 根の位置だけ消す
        walk(&glb, root, Mat4::from_scale_rotation_translation(node.s, node.r, Vec3::ZERO), sway, &mut out);
    }
    Ok(out)
}

/// propMat / lanternMat（灯籠は石の色を温かい生成りへ寄せる）。どちらも影を受ける
pub fn prop_params(name: &str) -> SoftParams {
    if name == "lantern_stone" || name == "lamp_post" {
        SoftParams { color: hex("#ffe6cc"), rim: 1.2, receive_shadow: true, ..Default::default() }
    } else {
        SoftParams { receive_shadow: true, ..Default::default() }
    }
}

/// 同じ型の小物を集める
#[derive(Default)]
struct PropSets {
    sets: BTreeMap<(String, usize), Option<(Vec<PropPart>, Vec<Mat4>)>>,
}
impl PropSets {
    fn place(&mut self, name: &str, variant: usize, m: Mat4) {
        let e = self.sets.entry((name.to_string(), variant)).or_insert_with(|| match prop_parts(name, variant) {
            Ok(parts) => Some((parts, Vec::new())),
            Err(e) => { eprintln!("小物 {name}: {e}"); None }
        });
        if let Some((_, list)) = e { list.push(m); }
    }
}

fn trs(pos: Vec3, ry: f32, s: f32) -> Mat4 { Mat4::from_scale_rotation_translation(Vec3::splat(s), Quat::from_rotation_y(ry), pos) }
fn tr(x: f32, y: f32, z: f32) -> Mat4 { Mat4::from_translation(Vec3::new(x, y, z)) }

/// 動かない物（soft の型）の 1 つ
struct Batch {
    mesh: MeshBuf,
    range: Range<u32>,
    double: bool,
    cast: bool,
    map: Option<wgpu::BindGroup>,
}

/// 小物の部品 1 つ分の描き方
struct PropDraw {
    mesh: MeshBuf,
    range: Range<u32>,
    /// 影の範囲に入っている物（shadow_buf の中）
    shadow: Range<u32>,
}

struct HoldView {
    x: f32,
    z: f32,
    flip: f32,
    fade: f32,
    flash: f32,
    color: [f32; 3],
    mark: [f32; 3],
}

struct FlagSpot {
    x: f32,
    z: f32,
    /// 砦の幟はその砦の番号、本道の幟は None
    hold: Option<usize>,
    cloth: Mat4,
    visible: bool,
}

/// 描く量（性能の比較用）
#[derive(Clone, Copy, Debug, Default)]
pub struct WorldStats {
    pub draws: u32,
    pub shadow_draws: u32,
    pub triangles: u64,
    pub shadow_triangles: u64,
    pub prop_instances: u32,
    pub prop_kinds: u32,
}

pub struct World {
    soft: Soft,
    p_cloth: wgpu::RenderPipeline,
    p_ring: wgpu::RenderPipeline,
    p_sprite: wgpu::RenderPipeline,
    p_points: wgpu::RenderPipeline,
    glow: wgpu::BindGroup,
    holds: Vec<HoldView>,
    // 動かない物
    static_buf: wgpu::Buffer,
    batches: Vec<Batch>,
    // 小物
    prop_buf: wgpu::Buffer,
    props: Vec<PropDraw>,
    prop_inst: Vec<FxInst>,
    prop_bounds: Vec<(Vec3, f32)>,
    prop_kinds: u32,
    shadow_buf: wgpu::Buffer,
    shadow_inst: Vec<FxInst>,
    // 幟と竿
    flags: Vec<FlagSpot>,
    pole_mesh: MeshBuf,
    pole_buf: wgpu::Buffer,
    pole_inst: Vec<FxInst>,
    pole_params: SoftParams,
    flag_mesh: MeshBuf,
    flag_buf: wgpu::Buffer,
    flag_inst: Vec<FxInst>,
    // 砦の大旗（透かす竿と布）
    fade_mesh: MeshBuf,
    fade_buf: wgpu::Buffer,
    fade_inst: Vec<FxInst>,
    fade_depth: u32,
    banner_mesh: MeshBuf,
    banner_buf: wgpu::Buffer,
    banner_inst: Vec<FxInst>,
    // 陣の輪（半径が砦ごとなので、輪の数だけ描く）
    zones: Vec<MeshBuf>,
    zone_buf: wgpu::Buffer,
    zone_inst: Vec<FxInst>,
    // 門
    door_mesh: MeshBuf,
    stud_mesh: MeshBuf,
    door_buf: wgpu::Buffer,
    door_inst: Vec<FxInst>,
    door_open: f32,
    // 光のにじみ（かがり火・本陣の灯り・門の光）
    quad: MeshBuf,
    sprite_buf: wgpu::Buffer,
    sprite_inst: Vec<FxInst>,
    ember_buf: wgpu::Buffer,
    embers: u32,
}

fn fx_bytes(n: usize) -> u64 { (n * std::mem::size_of::<FxInst>()) as u64 }
fn fx(model: Mat4, a: [f32; 4], b: [f32; 4]) -> FxInst { FxInst { model: model.to_cols_array_2d(), a, b, c: [0.0; 4], d: [0.0; 4] } }
fn sprite(pos: Vec3, sx: f32, sy: f32, color: [f32; 3], opacity: f32) -> FxInst {
    fx(Mat4::from_scale_rotation_translation(Vec3::new(sx, sy, 1.0), Quat::IDENTITY, pos), [color[0], color[1], color[2], opacity], [0.0; 4])
}

impl World {
    /// 砂浜の島を夕暮れの海が囲む（森と草むらは置かない）
    pub fn new(gfx: &Gfx, holds: &[Stronghold]) -> Self {
        let mut rng = Rng(0x9e37_79b9);
        let field_r = FIELD_R as f32;
        let (gate_x, gate_z) = (GATE.x as f32, GATE.z as f32);
        let spots: Vec<(f32, f32, f32)> = holds.iter().map(|h| (h.x as f32, h.z as f32, h.r as f32)).collect();
        let soft = Soft::new(gfx);

        let mut statics: Vec<FxInst> = Vec::new();
        let mut batches: Vec<Batch> = Vec::new();
        let mut add = |geo: &Geo, insts: &[FxInst], double: bool, cast: bool, map: Option<wgpu::BindGroup>| {
            let start = statics.len() as u32;
            statics.extend_from_slice(insts);
            batches.push(Batch { mesh: gfx.mesh(geo), range: start..statics.len() as u32, double, cast, map });
        };

        // ---- 地面 ----
        let ground_tex = {
            let c = sand_texture(holds, &mut rng);
            gfx.texture(c.w as u32, c.h as u32, &c.px, true)
        };
        const W: f32 = 240.0;
        let mut ground = geom::plane(W, W, 1, 1).rotate_x(-PI / 2.0);
        // canvas の絵は上の行が v = 1（flipY）。ここのテクスチャは上の行が v = 0
        for v in &mut ground.v { v.uv[1] = 1.0 - v.uv[1]; }
        add(&ground, &[SoftParams { rim: 0.0, receive_shadow: true, ..Default::default() }.inst(Mat4::IDENTITY)], false, false, Some(gfx.tex_bind(&ground_tex)));
        // 海（地平まで）と、波打ち際の白い泡の帯
        add(&geom::ring(SHORE_R, 900.0, 96).rotate_x(-PI / 2.0), &[SoftParams { color: hex("#3a7c96"), rim: 0.0, ..Default::default() }.inst(tr(0.0, 0.04, 0.0))], false, false, None);
        add(&geom::ring(SHORE_R + 2.5, SHORE_R + 9.0, 96).rotate_x(-PI / 2.0), &[SoftParams { color: hex("#5aa0b0"), rim: 0.0, ..Default::default() }.inst(tr(0.0, 0.05, 0.0))], false, false, None);
        add(&geom::ring(SHORE_R - 0.6, SHORE_R + 1.6, 96).rotate_x(-PI / 2.0), &[SoftParams { color: hex("#f4f0e6"), rim: 0.0, ..Default::default() }.inst(tr(0.0, 0.06, 0.0))], false, false, None);
        add(&geom::ring(SHORE_R + 4.0, SHORE_R + 4.8, 96).rotate_x(-PI / 2.0), &[SoftParams { color: hex("#d8ece8"), rim: 0.0, ..Default::default() }.inst(tr(0.0, 0.07, 0.0))], false, false, None);

        // ---- 岩・ススキ（場の外周） ----
        let mut sets = PropSets::default();
        let ring = |sets: &mut PropSets, rng: &mut Rng, name: &str, count: usize, r0: f32, r1: f32, s0: f32, s1: f32, variants: usize| {
            for i in 0..count {
                let a = (i as f32 / count as f32) * TAU + rng.next() * 0.2;
                let r = r0 + rng.next() * (r1 - r0);
                let (ry, s) = (rng.next() * 6.28, s0 + rng.next() * (s1 - s0));
                sets.place(name, i % variants, trs(Vec3::new(a.cos() * r, 0.0, a.sin() * r), ry, s));
            }
        };
        // 波打ち際と海の中の岩、砂丘の草
        ring(&mut sets, &mut rng, "rock_large", 26, field_r + 2.0, SHORE_R + 2.0, 1.2, 2.6, 3);
        ring(&mut sets, &mut rng, "rock_large", 12, SHORE_R + 14.0, SHORE_R + 40.0, 2.0, 3.6, 3);
        ring(&mut sets, &mut rng, "pampas_grass", 50, field_r - 6.0, field_r + 8.0, 1.0, 1.6, 3);
        // 場の中の小さな岩・草（飾り。当たりはない）
        for i in 0..26usize {
            let (a, r) = (rng.next() * TAU, 20.0 + rng.next() * (field_r - 26.0));
            let (x, z) = (a.cos() * r, a.sin() * r);
            if spots.iter().any(|h| (h.0 - x).hypot(h.1 - z) < h.2 + 6.0) || x.abs() < 8.0 { continue; }
            // 場の中は低い物だけ（カメラを寄せたとき手前を大きく塞がない。2026-11-15）
            sets.place("pampas_grass", i % 3, trs(Vec3::new(x, 0.0, z), a, 1.0 + rng.next() * 0.6));
        }

        // 草むらは生やさない（砂を見せる）。もとのゲームは草の葉の形にここで乱数を 21 回使うので、幟とかがり火の置き方が変わらないよう同じだけ進める
        for _ in 0..21 { rng.next(); }

        // ---- 幟の列（敵の陣地のしるし） ----
        let mut flags: Vec<FlagSpot> = Vec::new();
        let mut flag_spot = |x: f32, z: f32, hold: Option<usize>| {
            let cloth = Mat4::from_translation(Vec3::new(x + 0.04, 3.2, z)) * Mat4::from_rotation_y(x.atan2(z) + PI / 2.0);
            flags.push(FlagSpot { x, z, hold, cloth, visible: true });
        };
        for (hi, h) in spots.iter().enumerate() {
            for i in 0..10 {
                let a = (i as f32 / 10.0) * TAU + 0.3;
                flag_spot(h.0 + a.cos() * (h.2 + 2.2), h.1 + a.sin() * (h.2 + 2.2), Some(hi));
            }
        }
        for i in 0..16 {
            let t = i as f32 / 15.0;
            flag_spot((if i % 2 == 1 { -1.0 } else { 1.0 }) * (7.0 + rng.next() * 3.0), 30.0 - t * 90.0, None);
        }
        let pole_params = SoftParams { color: hex(POLE), ..Default::default() };

        // ---- 砦: 柵の輪・灯籠 ----
        let mut hold_views = Vec::new();
        let mut zones = Vec::new();
        let mut fade_inst = Vec::new();
        for &(hx, hz, hr) in &spots {
            let group = tr(hx, 0.0, hz);
            let n = 18;
            for i in 0..n {
                // 出入り口（主役の来る向き）を空ける
                let a = (i as f32 / n as f32) * TAU;
                let (gx, gz) = (a.cos(), a.sin());
                let to_center = (-hx).atan2(-hz + 30.0);
                let dir_a = gx.atan2(gz);
                if (((dir_a - to_center + PI * 3.0) % TAU) - PI).abs() < 0.45 { continue; }
                sets.place("fence_segment", 0, group * trs(Vec3::new(gx * hr, 0.0, gz * hr), -a, 1.6));
            }
            for i in 0..4 {
                let a = (i as f32 / 4.0) * TAU + PI / 4.0;
                sets.place("lantern_stone", 0, group * trs(Vec3::new(a.cos() * (hr - 2.0), 0.0, a.sin() * (hr - 2.0)), 0.0, 1.3));
            }
            // 中央の大旗の竿
            fade_inst.push(pole_params.inst(group));
            // 地面の陣の輪（持ち主の色）
            zones.push(gfx.mesh(&geom::ring(hr - 0.4, hr + 0.25, 72).rotate_x(-PI / 2.0)));
            hold_views.push(HoldView { x: hx, z: hz, flip: 0.0, fade: 1.0, flash: 0.0, color: hex(ENEMY), mark: hex(CLOTH_MARK) });
        }

        // ---- 本陣（門と城壁） ----
        let castle = tr(gate_x, 0.0, gate_z);
        // 本陣の城: ビスケット色の壁に、海の色の屋根
        let wall_mat = SoftParams { color: hex("#e6c2a4"), ..Default::default() };
        let trim_mat = SoftParams { color: hex(TRIM), ..Default::default() };
        let roof_mat = SoftParams { color: hex("#3f8c9c"), ..Default::default() };
        let both = |p: SoftParams, f: &dyn Fn(f32) -> Mat4| [p.inst(castle * f(-1.0)), p.inst(castle * f(1.0))];
        add(&geom::boxg(30.0, 7.0, 3.0), &both(SoftParams { receive_shadow: true, ..wall_mat }, &|s| tr(s * 21.0, 3.5, -2.0)), false, true, None);
        add(&geom::boxg(30.6, 0.6, 3.6), &both(trim_mat, &|s| tr(s * 21.0, 7.2, -2.0)), false, false, None);
        add(&geom::cylinder(3.2, 3.6, 13.0, 16, false), &both(wall_mat, &|s| tr(s * 7.5, 6.5, -2.0)), false, true, None);
        add(&geom::cone(4.4, 4.6, 16), &both(roof_mat, &|s| tr(s * 7.5, 15.2, -2.0)), false, true, None);
        add(&geom::cylinder(3.7, 3.7, 0.6, 16, false), &both(trim_mat, &|s| tr(s * 7.5, 12.6, -2.0)), false, false, None);
        add(&geom::boxg(12.0, 1.6, 3.4), &[roof_mat.inst(castle * tr(0.0, 9.6, -2.0))], false, false, None);

        // ---- 遠くのかがり火（場の外周の夜景） ----
        let ember_color = hex("#ff9a50");
        let embers: Vec<FxInst> = (0..70).map(|_| {
            let (a, r) = (rng.next() * TAU, field_r + 30.0 + rng.next() * 120.0);
            let y = 0.5 + rng.next() * 3.0;
            fx(tr(a.cos() * r, y, a.sin() * r), [ember_color[0], ember_color[1], ember_color[2], 1.0], [3.5, 0.0, 0.0, 0.0])
        }).collect();

        // 小物を部品ごとの 1 つのインスタンス描画へ
        let mut props = Vec::new();
        let mut prop_inst = Vec::new();
        let mut prop_bounds = Vec::new();
        let prop_kinds = sets.sets.values().filter(|s| s.is_some()).count() as u32;
        for ((name, _), set) in &sets.sets {
            let Some((parts, list)) = set else { continue };
            let params = prop_params(name);
            for part in parts {
                let start = prop_inst.len() as u32;
                for m in list {
                    let world = *m * part.local;
                    prop_inst.push(params.inst(world));
                    let scale = world.x_axis.truncate().length().max(world.y_axis.truncate().length()).max(world.z_axis.truncate().length());
                    prop_bounds.push((world.transform_point3(part.center), part.radius * scale));
                }
                props.push(PropDraw { mesh: gfx.mesh(&part.geo), range: start..prop_inst.len() as u32, shadow: 0..0 });
            }
        }

        // 布・輪の型
        let shader = gfx.shader("world", include_str!("../../shaders/world.wgsl"));
        let vbufs = [gfx::vert_layout(), gfx::fx_layout()];
        let plain = [&gfx.globals_layout];
        let with_tex = [&gfx.globals_layout, &gfx.shadow_layout, &gfx.tex_layout];
        let p_cloth = Pipe::scene("cloth", &shader, "vs_cloth", "fs_cloth", &plain, &vbufs).double().build(&gfx.device);
        let p_ring = Pipe::scene("zone", &shader, "vs_basic", "fs_basic", &plain, &vbufs).transparent(gfx::BLEND_ADD).build(&gfx.device);
        let p_sprite = Pipe::scene("sprite", &shader, "vs_sprite", "fs_glow", &with_tex, &vbufs).transparent(gfx::BLEND_ADD).double().build(&gfx.device);
        let p_points = Pipe::scene("points", &shader, "vs_points", "fs_glow", &with_tex, &vbufs).transparent(gfx::BLEND_ADD).double().build(&gfx.device);
        let (gw, gh, grgba) = raster::glow_rgba();
        let glow_view = gfx.texture(gw, gh, &grgba, true);

        let upload = |list: &[FxInst]| {
            let b = gfx.vbuf(fx_bytes(list.len()));
            gfx.queue.write_buffer(&b, 0, bytemuck::cast_slice(list));
            b
        };
        let n_flags = flags.len();
        let mut world = World {
            p_cloth, p_ring, p_sprite, p_points,
            glow: gfx.tex_bind(&glow_view),
            static_buf: upload(&statics),
            batches,
            prop_buf: upload(&prop_inst),
            shadow_buf: gfx.vbuf(fx_bytes(prop_inst.len())),
            shadow_inst: Vec::with_capacity(prop_inst.len()),
            props, prop_inst, prop_bounds, prop_kinds,
            pole_mesh: gfx.mesh(&geom::cylinder(0.05, 0.06, 4.6, 6, false).translate(0.0, 2.3, 0.0)),
            pole_buf: gfx.vbuf(fx_bytes(n_flags)),
            pole_inst: Vec::with_capacity(n_flags),
            pole_params,
            flag_mesh: gfx.mesh(&geom::plane(0.9, 2.6, 10, 6).translate(0.45, 0.0, 0.0)),
            flag_buf: gfx.vbuf(fx_bytes(n_flags)),
            flag_inst: Vec::with_capacity(n_flags),
            fade_mesh: gfx.mesh(&geom::cylinder(0.12, 0.16, 9.0, 8, false).translate(0.0, 4.5, 0.0)),
            fade_buf: gfx.vbuf(fx_bytes(spots.len())),
            fade_inst,
            fade_depth: spots.len() as u32,
            banner_mesh: gfx.mesh(&geom::plane(3.4, 2.4, 16, 8).translate(1.7, 0.0, 0.0)),
            banner_buf: gfx.vbuf(fx_bytes(spots.len())),
            banner_inst: Vec::with_capacity(spots.len()),
            zones,
            zone_buf: gfx.vbuf(fx_bytes(spots.len())),
            zone_inst: Vec::with_capacity(spots.len()),
            door_mesh: gfx.mesh(&geom::boxg(5.6, 8.8, 0.5)),
            stud_mesh: gfx.mesh(&geom::boxg(5.2, 0.35, 0.6)),
            door_buf: gfx.vbuf(fx_bytes(6)),
            door_inst: Vec::with_capacity(6),
            door_open: 0.0,
            quad: gfx.mesh(&geom::plane(1.0, 1.0, 1, 1)),
            sprite_buf: gfx.vbuf(fx_bytes(spots.len() + 3)),
            sprite_inst: Vec::with_capacity(spots.len() + 3),
            ember_buf: upload(&embers),
            embers: embers.len() as u32,
            holds: hold_views,
            flags,
            soft,
        };
        world.write_doors(gfx);
        world.write_poles(gfx);
        world
    }

    fn write_doors(&mut self, gfx: &Gfx) {
        let door_mat = SoftParams { color: hex(DOOR), ..Default::default() };
        let trim_mat = SoftParams { color: hex(TRIM), ..Default::default() };
        let castle = tr(GATE.x as f32, 0.0, GATE.z as f32);
        let pivot = |side: f32| castle * tr(side * 5.6, 0.0, -0.6) * Mat4::from_rotation_y(side * self.door_open * 1.6);
        self.door_inst.clear();
        for side in [-1.0f32, 1.0] { self.door_inst.push(door_mat.inst(pivot(side) * tr(-side * 2.8, 4.4, 0.0))); }
        for side in [-1.0f32, 1.0] {
            for y in [6.2, 2.4] { self.door_inst.push(trim_mat.inst(pivot(side) * tr(-side * 2.8, y, 0.05))); }
        }
        gfx.queue.write_buffer(&self.door_buf, 0, bytemuck::cast_slice(&self.door_inst));
    }

    fn write_poles(&mut self, gfx: &Gfx) {
        self.pole_inst.clear();
        for f in &self.flags {
            if f.visible { self.pole_inst.push(self.pole_params.inst(tr(f.x, 0.0, f.z))); }
        }
        gfx.queue.write_buffer(&self.pole_buf, 0, bytemuck::cast_slice(&self.pole_inst));
    }

    /// 砦が落ちたとき: 旗と輪の色を、金へ
    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, gfx: &Gfx, holds: &[Stronghold], gate_open: bool, boss_active: bool, dt: f32, time: f32, cam: Vec3, hero: (f32, f32)) {
        let (enemy, ally) = (hex(ENEMY), hex(ALLY));
        let (ax, az) = (cam.x - hero.0, cam.z - hero.1);
        let l2 = (ax * ax + az * az).max(1e-6);
        self.zone_inst.clear();
        self.banner_inst.clear();
        self.sprite_inst.clear();
        self.fade_inst.clear();
        for (i, h) in holds.iter().enumerate() {
            let v = &mut self.holds[i];
            let is_ally = h.owner == Owner::Ally;
            let target = if is_ally { 1.0 } else { 0.0 };
            v.flip += (target - v.flip) * (dt * 2.5).min(1.0);
            v.color = lerp3(enemy, ally, v.flip);
            v.mark = hex(if is_ally { CLOTH_MARK_ALLY } else { CLOTH_MARK });
            let opacity = if h.alarm && !is_ally { 0.5 + 0.35 * (time * 6.0).sin() } else { 0.55 };
            self.zone_inst.push(fx(tr(v.x, 0.05, v.z), [v.color[0], v.color[1], v.color[2], opacity], [0.0; 4]));
            let scale = 4.2 + (time * 9.0 + i as f32).sin() * 0.3 + (time * 23.0 + i as f32 * 3.0).sin() * 0.15;
            self.sprite_inst.push(sprite(Vec3::new(v.x, 1.2, v.z), scale, scale, hex(if is_ally { FIRE_ALLY } else { FIRE_ENEMY }), 0.9));
            v.flash = (v.flash - dt * 1.5).max(0.0);
            // 旗竿がカメラと主役の間に入ったら透かす
            let (hx, hz) = hold_xz(h);
            let t = (((hx - hero.0) * ax + (hz - hero.1) * az) / l2).clamp(0.0, 1.0);
            let (px, pz) = (hero.0 + ax * t - hx, hero.1 + az * t - hz);
            let block = t > 0.05 && px * px + pz * pz < 2.2 * 2.2;
            v.fade += ((if block { 0.15 } else { 1.0 }) - v.fade) * (dt * 8.0).min(1.0);
            if v.fade > 0.5 {
                let (c, m) = (v.color, v.mark);
                self.banner_inst.push(fx(tr(v.x + 0.1, 7.4, v.z), [c[0], c[1], c[2], v.flash], [m[0], m[1], m[2], 0.0]));
            }
        }
        // 本陣の灯り・門の奥の赤い光（大将の気配）
        let lamp = hex(FIRE_ENEMY);
        let (gate_x, gate_z) = (GATE.x as f32, GATE.z as f32);
        for side in [-1.0f32, 1.0] { self.sprite_inst.push(sprite(Vec3::new(gate_x + side * 7.5, 9.0, gate_z + 1.6), 5.0, 5.0, lamp, 1.0)); }
        let glow = if gate_open { if boss_active { 0.3 } else { 0.75 + 0.2 * (time * 3.0).sin() } } else { 0.0 };
        if glow > 0.0 { self.sprite_inst.push(sprite(Vec3::new(gate_x, 4.5, gate_z - 1.5), 16.0, 12.0, hex("#ff2a40"), glow)); }

        // 透かす竿: 深度を書くものを先に（depthWrite は不透明度で切り替わる）
        for depth in [true, false] {
            for v in &self.holds {
                if (v.fade > 0.9) == depth {
                    self.fade_inst.push(SoftParams { opacity: v.fade, ..self.pole_params }.inst(tr(v.x, 0.0, v.z)));
                }
            }
        }
        self.fade_depth = self.holds.iter().filter(|v| v.fade > 0.9).count() as u32;

        // 幟がカメラと主役の間に入ったら隠す（群れの中の主役を見失わない）
        let mut dirty = false;
        self.flag_inst.clear();
        for f in &mut self.flags {
            let t = ((f.x - hero.0) * ax + (f.z - hero.1) * az) / l2;
            let tc = t.clamp(0.0, 1.0);
            let (px, pz) = (hero.0 + ax * tc - f.x, hero.1 + az * tc - f.z);
            let block = t > 0.02 && t < 1.05 && px * px + pz * pz < 1.6 * 1.6;
            if f.visible == block { f.visible = !block; dirty = true; }
            if f.visible {
                let (c, m, flash) = match f.hold { Some(h) => (self.holds[h].color, self.holds[h].mark, self.holds[h].flash), None => (enemy, hex(CLOTH_MARK), 0.0) };
                self.flag_inst.push(fx(f.cloth, [c[0], c[1], c[2], flash], [m[0], m[1], m[2], 0.0]));
            }
        }
        if dirty { self.write_poles(gfx); }

        let open = self.door_open + ((if gate_open { 1.0 } else { 0.0 }) - self.door_open) * (dt * 0.8).min(1.0);
        if open != self.door_open {
            self.door_open = open;
            self.write_doors(gfx);
        }

        // 影の範囲に入る小物だけ描く（three の frustumCulled と同じ、部品ごとの球と影のカメラの箱）
        let light = Mat4::look_at_rh(light_eye(hero), Vec3::new(hero.0, 0.0, hero.1), Vec3::Y);
        self.shadow_inst.clear();
        for d in &mut self.props {
            let start = self.shadow_inst.len() as u32;
            for i in d.range.clone() {
                let (c, r) = self.prop_bounds[i as usize];
                let p = light.transform_point3(c);
                let lim = SHADOW_HALF + r;
                if p.x.abs() < lim && p.y.abs() < lim && -p.z > SHADOW_NEAR - r && -p.z < SHADOW_FAR + r {
                    self.shadow_inst.push(self.prop_inst[i as usize]);
                }
            }
            d.shadow = start..self.shadow_inst.len() as u32;
        }

        let q = &gfx.queue;
        q.write_buffer(&self.shadow_buf, 0, bytemuck::cast_slice(&self.shadow_inst));
        q.write_buffer(&self.flag_buf, 0, bytemuck::cast_slice(&self.flag_inst));
        q.write_buffer(&self.banner_buf, 0, bytemuck::cast_slice(&self.banner_inst));
        q.write_buffer(&self.fade_buf, 0, bytemuck::cast_slice(&self.fade_inst));
        q.write_buffer(&self.zone_buf, 0, bytemuck::cast_slice(&self.zone_inst));
        q.write_buffer(&self.sprite_buf, 0, bytemuck::cast_slice(&self.sprite_inst));
    }

    pub fn flash_hold(&mut self, i: usize) { self.holds[i].flash = 1.0; }

    pub fn hold_top(&self, i: usize) -> Vec3 { Vec3::new(self.holds[i].x, 7.4, self.holds[i].z) }

    /// 影を落とす物（castShadow のものだけ）
    pub fn shadow(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.soft.shadow);
        for b in self.batches.iter().filter(|b| b.cast) { draw_fx(pass, &b.mesh, &self.static_buf, b.range.clone()); }
        for d in &self.props { draw_fx(pass, &d.mesh, &self.shadow_buf, d.shadow.clone()); }
        draw_fx(pass, &self.pole_mesh, &self.pole_buf, 0..self.pole_inst.len() as u32);
        draw_fx(pass, &self.door_mesh, &self.door_buf, 0..2);
        draw_fx(pass, &self.fade_mesh, &self.fade_buf, 0..self.holds.len() as u32);
    }

    pub fn draw(&self, gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, order: i32) {
        if order == ORDER_OPAQUE {
            self.soft.bind(gfx, pass);
            let mut double = None;
            for b in &self.batches {
                if double != Some(b.double) {
                    pass.set_pipeline(if b.double { &self.soft.double } else { &self.soft.front });
                    double = Some(b.double);
                }
                if let Some(m) = &b.map { pass.set_bind_group(2, m, &[]); }
                draw_fx(pass, &b.mesh, &self.static_buf, b.range.clone());
                if b.map.is_some() { pass.set_bind_group(2, &self.soft.white, &[]); }
            }
            pass.set_pipeline(&self.soft.front);
            for d in &self.props { draw_fx(pass, &d.mesh, &self.prop_buf, d.range.clone()); }
            draw_fx(pass, &self.pole_mesh, &self.pole_buf, 0..self.pole_inst.len() as u32);
            draw_fx(pass, &self.door_mesh, &self.door_buf, 0..2);
            draw_fx(pass, &self.stud_mesh, &self.door_buf, 2..6);
            pass.set_pipeline(&self.p_cloth);
            draw_fx(pass, &self.flag_mesh, &self.flag_buf, 0..self.flag_inst.len() as u32);
            draw_fx(pass, &self.banner_mesh, &self.banner_buf, 0..self.banner_inst.len() as u32);
        } else if order == 0 {
            // 透明: three は奥から順。竿だけ深度を書くので先に描く（光のにじみは足すだけなので順に左右されない）
            self.soft.bind(gfx, pass);
            pass.set_pipeline(&self.soft.fade);
            draw_fx(pass, &self.fade_mesh, &self.fade_buf, 0..self.fade_depth);
            pass.set_pipeline(&self.soft.fade_no_depth);
            draw_fx(pass, &self.fade_mesh, &self.fade_buf, self.fade_depth..self.fade_inst.len() as u32);
            pass.set_pipeline(&self.p_ring);
            for (i, z) in self.zones.iter().enumerate() { draw_fx(pass, z, &self.zone_buf, i as u32..i as u32 + 1); }
            pass.set_pipeline(&self.p_sprite);
            pass.set_bind_group(1, &gfx.shadow_bg, &[]);
            pass.set_bind_group(2, &self.glow, &[]);
            draw_fx(pass, &self.quad, &self.sprite_buf, 0..self.sprite_inst.len() as u32);
            pass.set_pipeline(&self.p_points);
            draw_fx(pass, &self.quad, &self.ember_buf, 0..self.embers);
        }
    }

    /// 描く量（性能の比較用）
    pub fn stats(&self) -> WorldStats {
        let tris = |m: &MeshBuf, n: u64| m.n as u64 / 3 * n;
        let mut s = WorldStats { prop_kinds: self.prop_kinds, prop_instances: self.prop_inst.len() as u32, ..Default::default() };
        for b in &self.batches {
            s.draws += 1;
            s.triangles += tris(&b.mesh, b.range.len() as u64);
            if b.cast { s.shadow_draws += 1; s.shadow_triangles += tris(&b.mesh, b.range.len() as u64); }
        }
        for d in &self.props {
            s.draws += 1;
            s.triangles += tris(&d.mesh, d.range.len() as u64);
            if !d.shadow.is_empty() { s.shadow_draws += 1; s.shadow_triangles += tris(&d.mesh, d.shadow.len() as u64); }
        }
        let (poles, flags, banners, holds) = (self.pole_inst.len() as u64, self.flag_inst.len() as u64, self.banner_inst.len() as u64, self.holds.len() as u64);
        // 竿・門（戸・金具）・布・大旗・透かす竿（2 回に分かれることがある）・輪・Sprite・Points
        s.draws += 2 + 1 + 2 + 2 + holds as u32 + 2;
        s.triangles += tris(&self.pole_mesh, poles) + tris(&self.door_mesh, 2) + tris(&self.stud_mesh, 4) + tris(&self.flag_mesh, flags) + tris(&self.banner_mesh, banners) + tris(&self.fade_mesh, holds)
            + self.zones.iter().map(|z| tris(z, 1)).sum::<u64>() + 2 * (self.sprite_inst.len() as u64 + self.embers as u64);
        s.shadow_draws += 3;
        s.shadow_triangles += tris(&self.pole_mesh, poles) + tris(&self.door_mesh, 2) + tris(&self.fade_mesh, holds);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_roots_like_prop_object() {
        let conifer = ["v0", "v1", "v2", "v0_lod1", "v1_lod1", "v2_lod1"];
        assert_eq!(pick_roots(&conifer, 1), vec![1]);
        // 無い型は最初の v
        assert_eq!(pick_roots(&conifer, 7), vec![0]);
        // big_tree: glow は入らない
        assert_eq!(pick_roots(&["v0", "glow"], 0), vec![0]);
        // lantern_stone: v が無いので _lod1 以外の部品全部
        assert_eq!(pick_roots(&["body", "glow", "body_lod1"], 0), vec![0, 1]);
    }

    #[test]
    fn reads_prop_trees() {
        let lantern = prop_parts("lantern_stone", 0).unwrap();
        assert_eq!(lantern.iter().map(|p| p.geo.i.len() / 3).collect::<Vec<_>>(), vec![1804, 12]);
        // 灯籠は揺れない
        assert!(lantern[0].geo.v.iter().all(|v| v.sway == 0.0));
        // ススキは揺れる
        let grass = prop_parts("pampas_grass", 1).unwrap();
        assert!(grass.iter().any(|p| p.geo.v.iter().any(|v| v.sway != 0.0)));
    }
}
