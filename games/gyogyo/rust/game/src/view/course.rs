// 競馬場: 走路・内柵と外柵・芝・スタンドと観客・スタートゲート・ゴール・ハロン棒・障害・浮島の土台。src/view/course.ts の写し。
// すべて Track（内柵の線の表）から作るので、コースが変わっても骨組みごと変わる。
// TS は GLB を読みながら組むが、ここは先に全部読む（load_assets）。乱数 R の順番は「GLB が読み終わっている」ときの TS と同じ（scatter は順番に 1 つずつ進む）。
use super::assets::{Props, load_glb, load_prop};
use super::materials::{SoftOptions, glow_texture, soft_material};
use super::runner::{GATE_COLOR, GATE_TEXT};
use super::theme::Theme;
use character_actor::glb::Glb;
use glam::{Mat4, Quat, Vec2, Vec3};
use island_kit::geometry::*;
use island_kit::glb::primitive_geometry;
use island_kit::scene::Rot;
use island_kit::texture::{TexOpts, Wrap};
use island_kit::{Color, CustomShader, Kit, MatId, MatKind, Material, NodeId, Side, TexId};
use sim::race::Rng;
use sim::track::{ObstacleKind, P3, Track};
use std::cell::Cell;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::Arc;

const NOISE: &str = include_str!("../../shaders/noise.wgsl");
const LIT: &str = include_str!("../../shaders/lit_common.wgsl");

fn v3(p: P3) -> Vec3 { Vec3::new(p.x as f32, p.y as f32, p.z as f32) }
fn trs(p: Vec3, q: Quat, s: Vec3) -> Mat4 { Mat4::from_scale_rotation_translation(s, q, p) }
fn ry(a: f64) -> Quat { Quat::from_rotation_y(a as f32) }
fn css(s: &str) -> Color { Color::css(s) }
fn c4(c: Color) -> [f32; 4] { [c.r, c.g, c.b, 0.0] }
fn soft(kit: &mut Kit, o: SoftOptions) -> MatId { kit.material(soft_material(o)) }
fn soft_c(kit: &mut Kit, color: &str) -> MatId { soft(kit, SoftOptions { color: css(color), ..Default::default() }) }

/// 文字の板
#[derive(Clone, Copy)]
struct Text<'a> { w: u32, h: u32, font: &'a str, bg: &'a str, fg: &'a str, size: u32 }
fn text_texture(kit: &mut Kit, text: &str, o: Text) -> TexId {
    let t = kit.canvas_texture(o.w, o.h, true, |g| {
        g.set_fill_style_str(o.bg);
        g.fill_rect(0.0, 0.0, o.w as f64, o.h as f64);
        g.set_fill_style_str(o.fg);
        g.set_font(&format!("{}px {}", o.size, o.font));
        g.set_text_align("center");
        g.set_text_baseline("middle");
        let _ = g.fill_text(text, o.w as f64 / 2.0, o.h as f64 / 2.0 + 4.0);
    });
    let opts = kit.tex_opts(t);
    kit.set_tex_opts(t, TexOpts { anisotropy: 4, ..opts });
    t
}

thread_local! {
    static CLOUD: Cell<Option<TexId>> = const { Cell::new(None) };
    static CHEVRON: Cell<Option<TexId>> = const { Cell::new(None) };
}

/// もこもこの雲の絵
fn cloud_texture(kit: &mut Kit) -> TexId {
    if let Some(t) = CLOUD.get() { return t; }
    let t = kit.canvas_texture(512, 256, true, |g| {
        let mut r = Rng(5);
        for _ in 0..46 {
            let x = 70.0 + r.next() * 370.0;
            let h = ((x - 40.0) / 440.0 * PI).sin();
            let y = 180.0 - r.next() * 110.0 * h;
            let rad = (20.0 + r.next() * 46.0) * (0.5 + h * 0.6);
            let gr = g.create_radial_gradient(x, y - rad * 0.2, rad * 0.1, x, y, rad).unwrap();
            let shade = if y > 150.0 { "rgba(235,235,245," } else { "rgba(255,255,255," };
            let _ = gr.add_color_stop(0.0, &format!("{shade}1)"));
            let _ = gr.add_color_stop(0.65, &format!("{shade}.9)"));
            let _ = gr.add_color_stop(1.0, &format!("{shade}0)"));
            g.set_fill_style_canvas_gradient(&gr);
            g.begin_path();
            let _ = g.arc(x, y, rad, 0.0, 6.3);
            g.fill();
        }
    });
    CLOUD.set(Some(t));
    t
}

/// 手前の矢じり（床の予告）の絵（TS は 1 つごとに同じ絵を描く）
fn chevron_texture(kit: &mut Kit) -> TexId {
    if let Some(t) = CHEVRON.get() { return t; }
    let t = kit.canvas_texture(128, 64, false, |g| {
        g.set_fill_style_str("#fff");
        g.begin_path();
        g.move_to(0.0, 56.0); g.line_to(64.0, 6.0); g.line_to(128.0, 56.0); g.line_to(128.0, 64.0); g.line_to(100.0, 64.0); g.line_to(64.0, 34.0); g.line_to(28.0, 64.0); g.line_to(0.0, 64.0);
        g.close_path();
        g.fill();
    });
    let opts = kit.tex_opts(t);
    kit.set_tex_opts(t, TexOpts { wrap_s: Wrap::Repeat, ..opts });
    CHEVRON.set(Some(t));
    t
}

/// 池の形（丸すぎない）
fn pond_geometry() -> Geometry {
    let mut g = circle(1.0, 96).rotate_x(-std::f32::consts::FRAC_PI_2).owned();
    for i in 1..g.positions.len() {
        let [x, _, z] = g.positions[i];
        let a = (z as f64).atan2(x as f64);
        let k = 1.0 + 0.12 * (a * 3.0 + 0.7).sin() + 0.08 * (a * 5.0 + 2.1).sin();
        g.positions[i] = [(x as f64 * 60.0 * k) as f32, 0.0, (z as f64 * 26.0 * k) as f32];
    }
    g
}

/// s の範囲を、断面（d と高さ）の押し出しにする
fn sweep(tr: &Track, profile: &[[f64; 2]], s0: f64, s1: f64, step_m: f64, closed: bool) -> Geometry {
    let n = (((s1 - s0) / step_m).ceil() as usize).max(2) + 1;
    let p = profile.len();
    let mut pos = Vec::with_capacity(n * p);
    let mut idx = Vec::new();
    for i in 0..n {
        let s = s0 + (s1 - s0) * (i as f64 / (n - 1) as f64);
        for pr in profile {
            let v = tr.pos(s, pr[0]);
            pos.push([v.x as f32, (v.y + pr[1]) as f32, v.z as f32]);
        }
    }
    for i in 0..n - 1 {
        for k in 0..p - if closed { 0 } else { 1 } {
            let (a, b, c, d) = (i * p + k, i * p + (k + 1) % p, (i + 1) * p + k, (i + 1) * p + (k + 1) % p);
            idx.extend_from_slice(&[a as u32, c as u32, b as u32, b as u32, c as u32, d as u32]);
        }
    }
    let mut g = Geometry { positions: pos, indices: Some(idx), ..Default::default() };
    g.compute_vertex_normals();
    g
}

#[derive(Default)]
struct RingOpts { h_to_center: bool, noise_h: f64, edge_drop: f64, color_edge: bool }

/// 内柵の内側（ring=0 が内柵、1 が中心）と、外柵の外の地面を、高さつきの輪で作る
fn ring_ground(tr: &Track, from: f64, to: f64, rows: usize, o: RingOpts) -> Geometry {
    let n = (tr.lap / 3.0).ceil() as usize;
    let (mut cx, mut cz) = (0.0, 0.0);
    for i in 0..tr.n { cx += tr.x[i]; cz += tr.z[i]; }
    cx /= tr.n as f64;
    cz /= tr.n as f64;
    let mut verts = Vec::with_capacity((n + 1) * (rows + 1));
    let mut cols = Vec::with_capacity((n + 1) * (rows + 1));
    let mut idx = Vec::new();
    for i in 0..=n {
        let s = (i as f64 / n as f64) * tr.lap;
        for r in 0..=rows {
            let k = r as f64 / rows as f64;
            let (x, mut y, z);
            if o.h_to_center {
                let v = tr.pos(s, from);
                // 中心へ寄せる
                x = v.x + (cx - v.x) * k * to;
                z = v.z + (cz - v.z) * k * to;
                y = tr.height(s) * (1.0 - k).powf(1.5);
            } else {
                let d = from + (to - from) * k;
                let v = tr.pos(s, d);
                x = v.x;
                z = v.z;
                y = tr.height(s) * (1.0 - k * 0.65);
            }
            if o.noise_h != 0.0 {
                let bump = (x * 0.043 + 1.3).sin() * (z * 0.051).cos() + (x * 0.11 + z * 0.09).sin() * 0.45;
                let near = (k * 2.4).min(1.0);
                y += bump * o.noise_h * near;
            }
            if o.edge_drop != 0.0 && k > 0.86 { y -= (k - 0.86) * o.edge_drop; }
            verts.push([x as f32, (y - 0.02) as f32, z as f32]);
            let e = if o.color_edge && k > 0.92 { 1.0 - (k - 0.92) / 0.08 * 0.2 } else { 1.0 };
            cols.push([e as f32, e as f32, e as f32, 1.0]);
        }
    }
    for i in 0..n {
        for r in 0..rows {
            let a = i * (rows + 1) + r;
            let (b, c) = (a + 1, a + rows + 1);
            let d = c + 1;
            idx.extend_from_slice(&[a as u32, b as u32, c as u32, b as u32, d as u32, c as u32]);
        }
    }
    let mut g = Geometry { positions: verts, colors: cols, indices: Some(idx), ..Default::default() };
    g.compute_vertex_normals();
    g
}

/// 形の違うお菓子を、頂点の色つきで（プリン・マカロン・ドーナツ・いちご）。遠目には色とりどりの観客
fn tint(mut g: Geometry, c: &str) -> Geometry {
    let col = css(c);
    g.colors = vec![[col.r, col.g, col.b, 1.0]; g.positions.len()];
    g
}
fn crowd_shapes() -> [Geometry; 4] {
    let eye = |x: f32, y: f32, z: f32| tint(sphere(0.055, 5, 4).translate(x, y, z).owned(), "#23202c");
    let merge = |parts: Vec<Geometry>| merge_geometries(&parts.iter().collect::<Vec<_>>(), false).expect("merge");
    let pudding = merge(vec![
        tint(cylinder(0.3, 0.42, 0.6, 9).translate(0.0, 0.3, 0.0).owned(), "#ffe08a"),
        tint(cylinder(0.2, 0.3, 0.16, 9).translate(0.0, 0.66, 0.0).owned(), "#8c4a24"),
        eye(-0.13, 0.34, 0.37), eye(0.13, 0.34, 0.37),
    ]);
    let macaron = merge(vec![
        tint(sphere(0.42, 9, 5).scale(1.0, 0.42, 1.0).translate(0.0, 0.55, 0.0).owned(), "#ffffff"),
        tint(cylinder(0.34, 0.34, 0.1, 9).translate(0.0, 0.38, 0.0).owned(), "#fff6e4"),
        tint(sphere(0.42, 9, 5).scale(1.0, 0.42, 1.0).translate(0.0, 0.2, 0.0).owned(), "#ffffff"),
        eye(-0.13, 0.55, 0.38), eye(0.13, 0.55, 0.38),
    ]);
    let donut = merge(vec![
        tint(torus(0.3, 0.15, 6, 10).translate(0.0, 0.45, 0.0).owned(), "#ffffff"),
        tint(torus(0.3, 0.15, 6, 10).translate(0.0, 0.45, -0.02).owned(), "#c98a4a"),
        eye(-0.13, 0.62, 0.15), eye(0.13, 0.62, 0.15),
    ]);
    let berry = merge(vec![
        tint(cone(0.4, 0.75, 9).rotate_x(std::f32::consts::PI).translate(0.0, 0.42, 0.0).owned(), "#ffffff"),
        tint(cone(0.26, 0.16, 7).translate(0.0, 0.86, 0.0).owned(), "#57b05a"),
        eye(-0.13, 0.56, 0.32), eye(0.13, 0.56, 0.32),
    ]);
    [pudding, macaron, donut, berry]
}

const PROP_NAMES: [&str; 6] = ["tree_round_large", "tree_round_small", "tree_conifer", "bush_round", "windmill", "reed_clump"];
/// 外柵の前で跳ねる観客の魚（Fish Kit。走者と同じ模型。public/models/fish）
const MOBS: [&str; 8] = ["aji", "saba", "madai", "kasago", "kurodai", "suzuki", "buri", "magochi"];

/// 読んだ GLB（読めなかった物は入らない。無くても走れる）
#[derive(Default)]
pub struct Assets { pub props: HashMap<String, Arc<Glb>>, pub mobs: HashMap<String, Arc<Glb>> }

/// 全部の GLB を読む（kit を借りない）
pub async fn load_assets(a: &std::rc::Rc<std::cell::RefCell<Assets>>) {
    use wasm_bindgen::JsValue;
    let waits = js_sys::Array::new();
    for name in PROP_NAMES {
        if a.borrow().props.contains_key(name) { continue; }
        let a = a.clone();
        waits.push(&wasm_bindgen_futures::future_to_promise(async move {
            if let Ok(g) = load_prop(name).await { a.borrow_mut().props.insert(name.to_string(), g); }
            Ok(JsValue::UNDEFINED)
        }));
    }
    for id in MOBS {
        if a.borrow().mobs.contains_key(id) { continue; }
        let a = a.clone();
        waits.push(&wasm_bindgen_futures::future_to_promise(async move {
            match load_glb(&super::fish::fish_url(id)).await {
                Ok(g) => { a.borrow_mut().mobs.insert(id.to_string(), g); }
                Err(e) => web_sys::console::warn_1(&format!("{id}: {e}").into()),
            }
            Ok(JsValue::UNDEFINED)
        }));
    }
    let _ = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::all(&waits)).await;
}

/// scatter の 1 本（TS の async 関数。await の所で止まる）
struct Scatter { name: &'static str, count: usize, d_min: f64, d_max: f64, s_min: f64, s_max: f64, avoid: bool, i: usize, pending: Option<(f64, f64, usize)> }

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CrowdU { p: [f32; 4], runner: [f32; 4] }

pub struct Board { pub tex: TexId, pub live: NodeId, pub live_mat: MatId, pub live_set: bool, pub pos: (f64, f64) }

struct Balloon { o: NodeId, y: f64 }

pub struct Course {
    pub group: NodeId,
    pub board: Board,
    pub podium: NodeId,
    pub pedestal: NodeId,
    pub stage_s: f64,
    pub stage_d: f64,
    pub select_s: f64,
    pub finish: NodeId,
    pub podium_yaw: f64,
    pub podium_pos: Vec3,
    pub cheer: f64,
    pub cheer_at: [f32; 3],
    pub gate_open: f64,
    gate: NodeId,
    gate_y: f32,
    doors: Vec<(NodeId, NodeId)>,
    rem_boards: Vec<NodeId>,
    low_hide: Vec<NodeId>,
    balloons: Vec<Balloon>,
    crowd: Vec<(MatId, f32)>,
}

pub fn create_course(kit: &mut Kit, props: &mut Props, tr: &Track, theme: &Theme, assets: &Assets) -> Course {
    let group = kit.scene.group(None);
    let gp = Some(group);
    let mut low_hide: Vec<NodeId> = Vec::new();
    let w = tr.width;
    let mut rr = Rng(((tr.n as u64 * 13 + 7) & 0xffff_ffff) as u32);
    let fpi = std::f32::consts::PI;

    // ---- 走路 ----
    {
        let (step_m, rows) = (1.5, 2usize);
        let n = (tr.lap / step_m).ceil() as usize + 1;
        let mut pos = Vec::with_capacity(n * (rows + 1));
        let mut sd = Vec::with_capacity(n * (rows + 1));
        let mut idx = Vec::new();
        for i in 0..n {
            let s = (i as f64 / (n - 1) as f64) * tr.lap;
            for r in 0..=rows {
                let d = (r as f64 / rows as f64) * w;
                let v = tr.pos(s, d);
                pos.push([v.x as f32, v.y as f32, v.z as f32]);
                sd.push([s as f32, d as f32]);
            }
        }
        for i in 0..n - 1 {
            for r in 0..rows {
                let a = i * (rows + 1) + r;
                let (b, c) = (a + 1, a + rows + 1);
                let d = c + 1;
                idx.extend_from_slice(&[a as u32, b as u32, c as u32, b as u32, d as u32, c as u32]);
            }
        }
        let mut g = Geometry { positions: pos, uvs: sd, indices: Some(idx), ..Default::default() };
        g.compute_vertex_normals();
        let geo = kit.geometry(&g);
        let pool = if tr.def.id == "hill" { tr.lap / 14.0 } else { 0.0 };
        let u: [[f32; 4]; 3] = [c4(theme.dirt), c4(theme.dirt_dark), [tr.finish_s as f32, tr.start_s as f32, w as f32, pool as f32]];
        let mat = kit.material(Material {
            side: Side::Double,
            ..Material::custom(CustomShader { label: "dirt".into(), wgsl: format!("{NOISE}\n{LIT}\n{}", include_str!("../../shaders/dirt.wgsl")), uniform: bytemuck::bytes_of(&u).to_vec(), ..Default::default() })
        });
        let m = kit.scene.mesh(gp, geo, mat);
        kit.scene[m].receive_shadow = true;
    }

    // ---- 地面（内馬場と、外の芝）----
    let turf_u: [[f32; 4]; 3] = [c4(theme.grass), c4(theme.grass_dark), [7.0, 0.0, 0.0, 0.0]];
    let turf = kit.material(Material {
        side: Side::Double,
        ..Material::custom(CustomShader { label: "turf".into(), wgsl: format!("{NOISE}\n{LIT}\n{}", include_str!("../../shaders/turf.wgsl")), uniform: bytemuck::bytes_of(&turf_u).to_vec(), ..Default::default() })
    });
    let inner = kit.geometry(&ring_ground(tr, -0.4, 1.0, 18, RingOpts { h_to_center: true, noise_h: if tr.def.hills.is_some() { 0.6 } else { 0.25 }, ..Default::default() }));
    let m = kit.scene.mesh(gp, inner, turf);
    kit.scene[m].receive_shadow = true;
    let outer_span = 70.0;
    let outer = kit.geometry(&ring_ground(tr, w + 0.4, w + outer_span, 16, RingOpts { noise_h: 1.6, edge_drop: 14.0, color_edge: true, ..Default::default() }));
    let m = kit.scene.mesh(gp, outer, turf);
    kit.scene[m].receive_shadow = true;
    // 浮島の側面: 地層が見える崖
    {
        let cliff_n = (tr.lap / 4.0).ceil() as usize;
        let rows = 8usize;
        let strata = ["#7fb05c", "#a4784e", "#8a5f42", "#b08a5f", "#6d4a3b", "#90694c", "#544054", "#3c3350", "#2c2740"].map(css);
        let (mut pos, mut col, mut idx) = (Vec::new(), Vec::new(), Vec::new());
        for i in 0..=cliff_n {
            let s = (i as f64 / cliff_n as f64) * tr.lap;
            for r in 0..=rows {
                let k = (r as f64 / 7.0).min(1.0);
                let ang = s * 0.13;
                let wob = (ang * 2.1).sin() * 3.0 + (ang * 5.3 + r as f64).sin() * 1.6 + (ang * 13.0 + r as f64 * 2.0).sin() * 0.8;
                // 最後の列は内へ閉じて、底に穴を残さない
                let v = tr.pos(s, if r == 8 { -30.0 } else if r == 0 { w + outer_span - 6.0 } else { w + outer_span - k * k * 42.0 + wob * (0.4 + k) });
                let y = -14.0 + 2.0 * (ang * 3.0).sin() - k * 34.0 * (1.0 + 0.35 * (ang * 3.7 + 0.5).sin());
                pos.push([v.x as f32, (if r == 0 { 1.5 } else if r == 8 { -62.0 } else { y }) as f32, v.z as f32]);
                let c = strata[r];
                col.push([c.r, c.g, c.b, 1.0]);
            }
        }
        for i in 0..cliff_n {
            for r in 0..rows {
                let a = i * (rows + 1) + r;
                let (b, c) = (a + 1, a + rows + 1);
                let d = c + 1;
                idx.extend_from_slice(&[a as u32, b as u32, c as u32, b as u32, d as u32, c as u32]);
            }
        }
        let mut g = Geometry { positions: pos, colors: col, indices: Some(idx), ..Default::default() };
        g.compute_vertex_normals();
        let geo = kit.geometry(&g);
        let mat = soft(kit, SoftOptions { vertex_colors: true, side: Side::Double, flat_shading: true, ..Default::default() });
        kit.scene.mesh(gp, geo, mat);
    }

    // ---- 柵 ----
    let rail_mat = soft(kit, SoftOptions { color: css("#fbf7ee"), rim: 0.4, side: Side::Double, emissive: css("#ffffff"), emissive_intensity: if theme.stars > 0.4 { 0.28 } else { 0.04 }, ..Default::default() });
    for prof in [[[-0.34, 1.12], [-0.34, 1.22], [-0.2, 1.22], [-0.2, 1.12]], [[w + 0.2, 1.12], [w + 0.2, 1.22], [w + 0.34, 1.22], [w + 0.34, 1.12]]] {
        let geo = kit.geometry(&sweep(tr, &prof, 0.0, tr.lap, 2.0, true));
        let m = kit.scene.mesh(gp, geo, rail_mat);
        kit.scene[m].cast_shadow = true;
    }
    // 柵の柱（曲がった支柱）。流れて速さが見える
    {
        let post_g = kit.geometry(&box_(0.12, 1.2, 0.12).translate(0.0, 0.6, 0.0).owned());
        let span = 2.6;
        let count = (tr.lap / span).floor() as usize * 2;
        let inst = kit.scene.instanced(gp, post_g, rail_mat, count);
        let mut q = 0;
        for i in 0..count / 2 {
            let s = i as f64 * span;
            for d in [-0.27, w + 0.27] {
                let v = tr.pos(s, d);
                kit.scene.set_instance(inst, q, trs(v3(v), ry(tr.yaw(s)), Vec3::ONE));
                q += 1;
            }
        }
        kit.scene[inst].cast_shadow = true;
    }

    // ---- 柵ぞいの草むらと花（小さく、たくさん。流れて速さが見える）----
    {
        let tuft_g = kit.geometry(&icosahedron(0.34, 1).scale(1.0, 0.62, 1.0).translate(0.0, 0.14, 0.0).owned());
        let tuft_m = soft(kit, SoftOptions { color: theme.grass, rim: 0.3, ..Default::default() });
        let tufts = kit.scene.instanced(gp, tuft_g, tuft_m, 1500);
        let pet_g = kit.geometry(&icosahedron(0.13, 1));
        let pet_m = soft(kit, SoftOptions { color: css("#ffffff"), rim: 0.5, ..Default::default() });
        let petals = kit.scene.instanced(gp, pet_g, pet_m, 1500);
        let palette = ["#ff8fb0", "#ffd84a", "#ffffff", "#f4709a", "#ffb25c", "#b89bff"].map(css);
        for i in 0..1500 {
            let s = rr.next() * tr.lap;
            let d = if rr.next() < 0.6 { let (a, b) = (rr.next(), rr.next()); w + 0.75 + a * b * 8.0 } else { let (a, b) = (rr.next(), rr.next()); -0.75 - a * b * 7.0 };
            let v = tr.pos(s, d);
            let sc = 0.3 + rr.next() * 0.4;
            let sy = sc * (0.5 + rr.next() * 0.5);
            let rot = ry(rr.next() * 6.0);
            kit.scene.set_instance(tufts, i, trs(v3(v), rot, Vec3::new(sc as f32, sy as f32, sc as f32)));
            let px = v.x + (rr.next() - 0.5) * 0.3;
            let py = v.y + 0.12 + rr.next() * 0.14;
            let pz = v.z + (rr.next() - 0.5) * 0.3;
            let ps = if rr.next() < 0.35 { 0.0001 } else { 0.45 + rr.next() * 0.45 };
            kit.scene.set_instance(petals, i, trs(Vec3::new(px as f32, py as f32, pz as f32), rot, Vec3::splat(ps as f32)));
            let c = palette[(rr.next() * palette.len() as f64).floor() as usize];
            kit.scene.set_instance_color(petals, i, c);
        }
        kit.scene[tufts].frustum_culled = false;
        kit.scene[petals].frustum_culled = false;
        low_hide.push(tufts);
        low_hide.push(petals);
    }

    // ---- 内柵の小旗（12.5m ごと。手前を流れて速さが見える）----
    {
        let n = (tr.lap / 12.5).floor() as usize;
        let g = Geometry {
            positions: vec![[0.0, 1.2, 0.0], [0.0, 2.05, 0.0], [0.62, 1.82, 0.0], [0.0, 1.58, 0.0]], normals: vec![[0.0, 0.0, 1.0]; 4], indices: Some(vec![0, 3, 2, 3, 1, 2]), ..Default::default()
        };
        let flag_g = kit.geometry(&g);
        let flag_m = soft(kit, SoftOptions { color: css("#ffffff"), side: Side::Double, ..Default::default() });
        let flags = kit.scene.instanced(gp, flag_g, flag_m, n);
        let pole_g = kit.geometry(&cylinder(0.03, 0.03, 0.95, 5).translate(0.0, 1.62, 0.0).owned());
        let pole_m = soft_c(kit, "#f7f3e8");
        let pole = kit.scene.instanced(gp, pole_g, pole_m, n);
        for i in 0..n {
            let s = i as f64 * 12.5;
            let v = tr.pos(s, -0.3);
            let m = trs(v3(v), ry(tr.yaw(s) + PI), Vec3::ONE);
            kit.scene.set_instance(flags, i, m);
            kit.scene.set_instance(pole, i, m);
            kit.scene.set_instance_color(flags, i, css(GATE_COLOR[if i % 8 == 1 { 2 } else { i % 8 }]));
        }
        kit.scene[flags].frustum_culled = false;
        kit.scene[pole].frustum_culled = false;
        low_hide.push(flags);
        low_hide.push(pole);
    }

    // ---- ハロン棒（残り距離の棒。赤白）----
    // 残り距離の札。最終周だけ出す（1 周目に「200」を通ると紛らわしい）
    let mut rem_boards: Vec<NodeId> = Vec::new();
    {
        let (red, white) = (soft_c(kit, "#e23b35"), soft_c(kit, "#fff6e8"));
        let plane_g = kit.geometry(&plane(3.4, 1.7));
        let mut rem = 100;
        while rem <= 800 {
            let dist = tr.race_len - rem as f64;
            let s = tr.s_at(dist);
            let r0 = rem;
            rem += 100;
            // ゴール線のそばの棒は立てない（写真判定と紛らわしい）
            let to_finish = (((s - tr.finish_s + tr.lap * 1.5) % tr.lap) - tr.lap / 2.0).abs();
            if to_finish < 14.0 { continue; }
            let v = tr.pos(s, -0.9);
            let tall = r0 % 200 == 0;
            let h = if tall { 5.2f32 } else { 3.4 };
            let pole = kit.scene.group(gp);
            let seg_g = kit.geometry(&cylinder(0.16, 0.16, h / 6.0, 10));
            for k in 0..6 {
                let seg = kit.scene.mesh(Some(pole), seg_g, if k % 2 == 1 { white } else { red });
                kit.scene[seg].position.y = (k as f32 + 0.5) * (h / 6.0);
                kit.scene[seg].cast_shadow = true;
            }
            if tall {
                // 札は両面に（裏から見ても文字が反転しない）
                let t = text_texture(kit, &r0.to_string(), Text { w: 256, h: 128, size: 96, font: "'Fredoka', sans-serif", bg: "#e23b35", fg: "#fff" });
                let board_m = kit.material(Material::basic().with_map(t));
                for ry_ in [0.0, fpi] {
                    let board = kit.scene.mesh(Some(pole), plane_g, board_m);
                    rem_boards.push(board);
                    kit.scene[board].position = Vec3::new(0.0, h + 0.5, if ry_ != 0.0 { -0.02 } else { 0.02 });
                    kit.scene[board].rotation = Rot::y(ry_);
                }
            }
            kit.scene[pole].position = v3(v);
            kit.scene[pole].rotation = Rot::y((tr.yaw(s) + PI) as f32);
        }
    }

    // ---- ゴール: 上に渡した門と、段の見える柱 ----
    let finish = kit.scene.group(gp);
    {
        let s = tr.finish_s;
        let v = tr.pos(s, w / 2.0);
        kit.scene[finish].position = v3(v);
        kit.scene[finish].rotation = Rot::y(tr.yaw(s) as f32);
        let post_m = soft_c(kit, "#143e2e");
        let post_g = kit.geometry(&box_(0.6, 8.2, 0.6));
        for side in [-1.0, 1.0] {
            let post = kit.scene.mesh(Some(finish), post_g, post_m);
            kit.scene[post].position = Vec3::new((side * (w / 2.0 + 0.9)) as f32, 4.1, 0.0);
            kit.scene[post].cast_shadow = true;
        }
        let goal = Text { w: 1024, h: 180, size: 150, font: "'Fredoka', sans-serif", bg: "#143e2e", fg: "#ffd76a" };
        let (t1, t2) = (text_texture(kit, "GOAL", goal), text_texture(kit, "GOAL", goal));
        let (m1, m2) = (kit.material(Material::basic().with_map(t1)), kit.material(Material::basic().with_map(t2)));
        let banner_g = kit.geometry(&box_((w + 2.4) as f32, 2.2, 0.3));
        let banner = kit.scene.mesh_multi(Some(finish), banner_g, &[post_m, post_m, post_m, post_m, m1, m2]);
        kit.scene[banner].position.y = 7.6;
        kit.scene[banner].cast_shadow = true;
        // 市松の帯
        let tex = kit.canvas_texture(256, 32, true, |g| {
            for x in 0..16 { for y in 0..2 { g.set_fill_style_str(if (x + y) % 2 == 1 { "#111" } else { "#fff" }); g.fill_rect(x as f64 * 16.0, y as f64 * 16.0, 16.0, 16.0); } }
        });
        let opts = kit.tex_opts(tex);
        kit.set_tex_opts(tex, TexOpts { mag_nearest: true, ..opts });
        let strip_g = kit.geometry(&box_((w + 2.4) as f32, 0.5, 0.32));
        let strip_m = kit.material(Material::basic().with_map(tex));
        let strip = kit.scene.mesh(Some(finish), strip_g, strip_m);
        kit.scene[strip].position.y = 6.25;
    }

    // ---- スタートゲート ----
    let gate = kit.scene.group(gp);
    let mut doors = Vec::new();
    let gate_y;
    {
        let s = tr.start_s;
        let v = tr.pos(s, 0.0);
        gate_y = v.y as f32;
        kit.scene[gate].position = v3(v);
        kit.scene[gate].rotation = Rot::y(tr.yaw(s) as f32);
        let (frame, frame_l) = (soft_c(kit, "#2a7d58"), soft_c(kit, "#fff4dc"));
        // 柱の先と横木の前の面は同じ平面（奥行きが同じ）。今の版は柱が勝つ。Rust は世界の行列の桁が粗く、まだらに混ざるので、柱を勝たせる
        kit.mat_mut(frame_l).polygon_offset = Some((0.0, -1.0));
        let gate_w = (w - 1.3) / 7.0;
        let dir_x = -1.0; // gate の x は d の逆向き（yaw で回した局所の +x は左）
        let top_g = kit.geometry(&box_((w + 1.0) as f32, 0.32, 1.0));
        let top = kit.scene.mesh(Some(gate), top_g, frame);
        kit.scene[top].position = Vec3::new((dir_x * w / 2.0) as f32, 2.72, 0.1);
        kit.scene[top].cast_shadow = true;
        let post_g = kit.geometry(&box_(0.1, 2.6, 1.0));
        for i in 0..=8 {
            let d = if i == 0 { 0.1 } else if i == 8 { w - 0.1 } else { 0.65 + gate_w * (i as f64 - 0.5) };
            let post = kit.scene.mesh(Some(gate), post_g, frame_l);
            kit.scene[post].position = Vec3::new((dir_x * d) as f32, 1.3, 0.1);
            kit.scene[post].cast_shadow = true;
        }
        let door_m = soft(kit, SoftOptions { color: css("#fff4dc"), side: Side::Double, rim: 0.3, emissive: css("#fff4dc"), emissive_intensity: 0.35, ..Default::default() });
        let leaf_w = gate_w * 0.47;
        let leaf_g = kit.geometry(&box_(leaf_w as f32, 0.1, 0.06));
        let plate_g = kit.geometry(&plane(0.3, 0.3));
        for i in 0..8 {
            let d = 0.65 + gate_w * i as f64;
            let mut mk = |side: f64| {
                let pivot = kit.scene.group(Some(gate));
                kit.scene[pivot].position = Vec3::new((dir_x * d + side * gate_w * 0.47) as f32, 0.0, 0.62);
                // 体を隠さない 2 本の横木
                for y in [0.72, 1.2] {
                    let leaf = kit.scene.mesh(Some(pivot), leaf_g, door_m);
                    kit.scene[leaf].position = Vec3::new((-side * leaf_w / 2.0) as f32, y, 0.0);
                    kit.scene[leaf].cast_shadow = true;
                }
                pivot
            };
            let l = mk(-1.0);
            let r = mk(1.0);
            doors.push((l, r));
            // 枠番の札
            let tx = kit.canvas_texture(96, 96, true, |g| {
                g.set_fill_style_str(GATE_COLOR[i]);
                g.fill_rect(0.0, 0.0, 96.0, 96.0);
                g.set_fill_style_str(GATE_TEXT[i]);
                g.set_font("700 72px Fredoka, sans-serif");
                g.set_text_align("center");
                g.set_text_baseline("middle");
                let _ = g.fill_text(&(i + 1).to_string(), 48.0, 52.0);
            });
            let pm = kit.material(Material::basic().with_map(tx));
            let plate = kit.scene.mesh(Some(gate), plate_g, pm);
            kit.scene[plate].position = Vec3::new((dir_x * d) as f32, 2.72, 0.62);
            let back = kit.scene.mesh(Some(gate), plate_g, pm);
            kit.scene[back].position = Vec3::new((dir_x * d) as f32, 2.72, -0.42);
            kit.scene[back].rotation = Rot::y(fpi);
        }
    }

    // ---- スタンド（ホームストレッチの外）----
    let stand_len = (tr.finish_s - 10.0).min(170.0);
    let (stand_s0, stand_s1) = (tr.finish_s - stand_len + 26.0, tr.finish_s + 26.0);
    let stand_d = w + 7.0;
    {
        let seat_m = soft(kit, SoftOptions { color: css("#23594a"), side: Side::Double, ..Default::default() });
        let rows_n = 12usize;
        let mut profile: Vec<[f64; 2]> = Vec::new();
        for r in 0..rows_n {
            profile.push([stand_d + r as f64 * 1.5, 1.6 + r as f64]);
            profile.push([stand_d + (r + 1) as f64 * 1.5, 1.6 + r as f64]);
        }
        profile.push([stand_d + rows_n as f64 * 1.5, 0.0]);
        let tiers_g = kit.geometry(&sweep(tr, &profile, stand_s0, stand_s1, 6.0, false));
        let tiers = kit.scene.mesh(gp, tiers_g, seat_m);
        kit.scene[tiers].cast_shadow = true;
        kit.scene[tiers].receive_shadow = true;
        let front_g = kit.geometry(&sweep(tr, &[[stand_d, 0.0], [stand_d, 1.6]], stand_s0, stand_s1, 6.0, false));
        let front_m = soft(kit, SoftOptions { color: css("#143e2e"), side: Side::Double, ..Default::default() });
        kit.scene.mesh(gp, front_g, front_m);
        // 屋根: 大きな反りの白い屋根
        let roof_h = 1.6 + rows_n as f64 + 5.0;
        let rn = rows_n as f64;
        let roof_p = [[stand_d + rn * 1.5 + 1.0, roof_h - 2.2], [stand_d + rn * 0.9, roof_h + 0.2], [stand_d + rn * 0.3, roof_h + 0.1], [stand_d - 4.0, roof_h - 1.1]];
        let roof_g = kit.geometry(&sweep(tr, &roof_p, stand_s0 - 3.0, stand_s1 + 3.0, 6.0, false));
        let roof_m = soft(kit, SoftOptions { color: css("#fff9ee"), side: Side::Double, rim: 0.6, ..Default::default() });
        let roof = kit.scene.mesh(gp, roof_g, roof_m);
        kit.scene[roof].cast_shadow = true;
        // 屋根の支柱と、旗
        let col_g = kit.geometry(&cylinder(0.28, 0.34, roof_h as f32, 10).translate(0.0, (roof_h / 2.0) as f32, 0.0).owned());
        let col_m = soft_c(kit, "#2a7d58");
        let cols = kit.scene.instanced(gp, col_g, col_m, 16);
        for i in 0..8 {
            for k in 0..2 {
                let s = stand_s0 + (i as f64 + 0.5) * ((stand_s1 - stand_s0) / 8.0);
                let v = tr.pos(s, stand_d + if k == 1 { rn * 1.5 } else { 1.0 });
                kit.scene.set_instance(cols, i * 2 + k, Mat4::from_translation(v3(v)));
            }
        }
        kit.scene[cols].cast_shadow = true;
        // 三角の旗の列（屋根の端）
        let flag_n = 44;
        let g = Geometry { positions: vec![[0.0, 0.0, 0.0], [0.0, -1.1, 0.0], [0.9, -0.55, 0.0]], normals: vec![[0.0, 0.0, 1.0]; 3], indices: Some(vec![0, 1, 2]), ..Default::default() };
        let flag_g = kit.geometry(&g);
        let flag_m = soft(kit, SoftOptions { color: css("#ffffff"), side: Side::Double, ..Default::default() });
        let flags = kit.scene.instanced(gp, flag_g, flag_m, flag_n);
        for i in 0..flag_n {
            let s = stand_s0 - 3.0 + (i as f64 + 0.5) * ((stand_s1 - stand_s0 + 6.0) / flag_n as f64);
            let v = tr.pos(s, stand_d - 4.0);
            kit.scene.set_instance(flags, i, trs(Vec3::new(v.x as f32, (v.y + roof_h - 1.1) as f32, v.z as f32), ry(tr.yaw(s) + PI / 2.0), Vec3::ONE));
            kit.scene.set_instance_color(flags, i, css(GATE_COLOR[i % 8]));
        }
    }

    // ---- 観客: 列ごとに小さなお菓子のかたまり。前の列だけ本物のモデル（CHEER で跳ねる）----
    let mut crowd: Vec<(MatId, f32)> = Vec::new();
    let crowd_wgsl = format!("{LIT}\n{}", include_str!("../../shaders/crowd.wgsl"));
    {
        let shapes = crowd_shapes();
        let u = CrowdU { p: [0.0, 1.0, 0.0, 0.0], runner: [0.0, -999.0, 0.0, 0.0] };
        let vc = kit.material(Material::custom(CustomShader { label: "crowd".into(), wgsl: crowd_wgsl.clone(), uniform: bytemuck::bytes_of(&u).to_vec(), ..Default::default() }));
        crowd.push((vc, 1.0));
        let rows_n = 12usize;
        let per_row = ((stand_s1 - stand_s0) / 1.15).floor() as usize;
        let palette = ["#ff8fb0", "#ffd27a", "#c69bff", "#ffa040", "#fff0d0", "#ff6d8a", "#8fd8ff", "#ffe08a", "#a7e8b0", "#f7c2e0"].map(css);
        let cap = ((rows_n * per_row) as f64 / 3.0).ceil() as usize;
        let insts: Vec<NodeId> = shapes.iter().map(|g| { let geo = kit.geometry(g); kit.scene.instanced(gp, geo, vc, cap) }).collect();
        let mut counts = [0usize; 4];
        for r in 0..rows_n {
            for i in 0..per_row {
                if rr.next() < 0.08 { continue; }
                let k = (rr.next() * 4.0).floor() as usize;
                if counts[k] >= cap { continue; }
                let s = stand_s0 + 1.0 + i as f64 * 1.15 + rr.next() * 0.3;
                let v = tr.pos(s, stand_d + r as f64 * 1.5 + 0.75);
                let scale = 1.05 + rr.next() * 0.4;
                // 走路のほうを向く
                kit.scene.set_instance(insts[k], counts[k], trs(Vec3::new(v.x as f32, (v.y + 1.6 + r as f64) as f32, v.z as f32), ry(tr.yaw(s) - PI / 2.0), Vec3::splat(scale as f32)));
                let c = if k == 0 { Color::WHITE } else { palette[(rr.next() * palette.len() as f64).floor() as usize] };
                kit.scene.set_instance_color(insts[k], counts[k], c.scale((1.0 - (r as f64 / rows_n as f64) * 0.5) as f32));
                counts[k] += 1;
            }
        }
        for (k, inst) in insts.iter().enumerate() {
            kit.scene.set_instance_count(*inst, counts[k]);
            kit.scene[*inst].frustum_culled = false;
        }
    }
    // 外柵の前で跳ねる魚の観客（ゴール前に集まる。模型は口先が +X・メートル・実物の大きさ）
    for id in MOBS {
        let Some(glb) = assets.mobs.get(id) else { continue };
        // 最初の Mesh の形と絵
        let mut stack: Vec<usize> = glb.scene.iter().rev().copied().collect();
        let mut first = None;
        while let Some(i) = stack.pop() {
            let n = &glb.nodes[i];
            if let Some(mi) = n.mesh { first = Some(mi); break; }
            stack.extend(n.children.iter().rev().copied());
        }
        let Some(mi) = first else { continue };
        let prim = &glb.meshes[mi].primitives[0];
        let geo = kit.geometry(&primitive_geometry(prim));
        let map = prim.material.and_then(|k| glb.materials[k].texture).map(|t| kit.glb_texture(glb, t));
        // p.z = 1: ひれの切り抜き（アルファで抜く）。ひれの板は両面
        let u = CrowdU { p: [0.0, 0.0, 1.0, 0.0], runner: [0.0, -999.0, 0.0, 0.0] };
        let mat = kit.material(Material { side: island_kit::Side::Double, ..Material::custom(CustomShader { label: "crowd map".into(), wgsl: crowd_wgsl.clone(), uniform: bytemuck::bytes_of(&u).to_vec(), textures: map.into_iter().collect(), ..Default::default() }) });
        let len = prim.positions.iter().fold((f32::MAX, f32::MIN), |(a, b), q| (a.min(q[0]), b.max(q[0])));
        let len = (len.1 - len.0).max(0.05) as f64;
        crowd.push((mat, 0.0));
        let count = 14;
        let inst = kit.scene.instanced(gp, geo, mat, count);
        for i in 0..count {
            let s = tr.wrap(tr.finish_s - 90.0 + rr.next() * 112.0);
            let d = w + 1.4 + rr.next() * 3.2;
            let v = tr.pos(s, d);
            // 口先（+X）を走路へ向ける
            let rot = ry(tr.yaw(s) - PI + (rr.next() - 0.5) * 0.6);
            let sc = (1.0 + rr.next() * 0.5) / len;
            let at = v3(v) + Vec3::new(0.0, (0.7 + rr.next() * 0.6) as f32, 0.0);
            kit.scene.set_instance(inst, i, trs(at, rot, Vec3::splat(sc as f32)));
        }
        kit.scene[inst].cast_shadow = true;
        kit.scene[inst].frustum_culled = false;
        low_hide.push(inst);
    }

    // ---- 内馬場の見せ場: 大きな画面（順位の掲示）----
    let board_tex = kit.canvas_texture_cpu(480, 512, true, |_| {});
    // 実況の映像（app が描く。最初は黒）
    let live_u = [0.0f32; 4];
    let live_mat = kit.material(Material { fog: true, ..Material::custom(CustomShader { label: "live".into(), wgsl: include_str!("../../shaders/live.wgsl").into(), uniform: bytemuck::bytes_of(&live_u).to_vec(), ..Default::default() }) });
    let live;
    {
        let s = tr.wrap(tr.finish_s - 24.0);
        let v = tr.pos(s, -22.0);
        let board = kit.scene.group(gp);
        kit.scene[board].position = v3(v);
        kit.scene[board].rotation = Rot::y((tr.yaw(s) - PI / 2.0 - 0.62) as f32);
        let fg = kit.geometry(&box_(32.0, 16.0, 1.0));
        let fm = soft_c(kit, "#16211d");
        let frame = kit.scene.mesh(Some(board), fg, fm);
        kit.scene[frame].position.y = 13.0;
        kit.scene[frame].cast_shadow = true;
        let sg = kit.geometry(&plane(13.6, 14.8));
        let sm = kit.material(Material::basic().with_map(board_tex));
        let screen = kit.scene.mesh(Some(board), sg, sm);
        kit.scene[screen].position = Vec3::new(-8.3, 13.0, 0.52);
        let lg = kit.geometry(&plane(16.6, 14.8));
        live = kit.scene.mesh(Some(board), lg, live_mat);
        kit.scene[live].position = Vec3::new(7.0, 13.0, 0.52);
        let leg_g = kit.geometry(&box_(1.2, 6.0, 1.2));
        let leg_m = soft_c(kit, "#2a2f2c");
        for x in [-12.0, 12.0] {
            let leg = kit.scene.mesh(Some(board), leg_g, leg_m);
            kit.scene[leg].position = Vec3::new(x, 3.0, 0.0);
        }
    }

    // ---- 障害 ----
    let add_chevron = |kit: &mut Kit, s: f64, d: f64, width: f64, color: &str, opacity: f32| {
        let tx = chevron_texture(kit);
        let mut mt = Material { polygon_offset: Some((-2.0, 0.0)), ..Material::basic().with_map(tx).with_color(css(color)).with_opacity(opacity).see_through() };
        mt.uv.repeat = Vec2::new((width / 2.4).round().max(1.0) as f32, 1.0);
        let mat = kit.material(mt);
        let geo = kit.geometry(&plane(width as f32, 1.25).rotate_x(-std::f32::consts::FRAC_PI_2).owned());
        let m = kit.scene.mesh(gp, geo, mat);
        let v = tr.pos(s, d);
        kit.scene[m].position = Vec3::new(v.x as f32, (v.y + 0.05) as f32, v.z as f32);
        kit.scene[m].rotation = Rot::y((tr.yaw(s) + PI) as f32);
        kit.scene[m].render_order = 1;
    };
    for ob in &tr.obstacles {
        if ob.kind == ObstacleKind::Hurdle {
            let g = kit.scene.group(gp);
            let n = (((ob.d1 - ob.d0) / 1.6).round() as usize).max(2);
            let seg = (ob.d1 - ob.d0) / n as f64;
            let hedge_g = kit.geometry(&box_((seg * 0.98) as f32, 0.62, 0.7));
            let hedge_m = soft_c(kit, "#2f8d48");
            let stripes = 4usize;
            let bar_g = kit.geometry(&box_((seg / stripes as f64 * 0.99) as f32, 0.3, 0.26));
            let bar_m = [("#fffaf0"), ("#e8392c")].map(|c| soft(kit, SoftOptions { color: css(c), emissive: css(c), emissive_intensity: 0.22, ..Default::default() }));
            let post_g = kit.geometry(&box_(0.12, 0.8, 0.12));
            let post_m = soft_c(kit, "#fffaf0");
            for i in 0..n {
                let d = ob.d0 + (i as f64 + 0.5) * seg;
                let v = tr.pos(ob.s, d);
                let part = kit.scene.group(Some(g));
                kit.scene[part].position = v3(v);
                kit.scene[part].rotation = Rot::y(tr.yaw(ob.s) as f32);
                // 生け垣と、赤白の横木（低い。遠くからでも「跳ぶ物」と分かる色）
                let hedge = kit.scene.mesh(Some(part), hedge_g, hedge_m);
                kit.scene[hedge].position.y = 0.31;
                kit.scene[hedge].cast_shadow = true;
                for k in 0..stripes {
                    let bar = kit.scene.mesh(Some(part), bar_g, bar_m[(k + i) % 2]);
                    kit.scene[bar].position = Vec3::new(((k as f64 + 0.5 - stripes as f64 / 2.0) * seg / stripes as f64) as f32, 0.72, 0.0);
                    kit.scene[bar].cast_shadow = true;
                }
                for px in [-seg * 0.47, seg * 0.47] {
                    let post = kit.scene.mesh(Some(part), post_g, post_m);
                    kit.scene[post].position = Vec3::new(px as f32, 0.4, 0.0);
                }
            }
            // 手前の矢じり（床の予告）
            for k in 1..=3 { add_chevron(kit, ob.s - 3.0 - k as f64 * 7.0, (ob.d0 + ob.d1) / 2.0, ob.d1 - ob.d0, "#ff4a38", 0.95 - k as f32 * 0.15); }
        } else {
            let geo = kit.geometry(&sweep(tr, &[[ob.d0, 0.03], [ob.d1, 0.03]], ob.s, ob.s + ob.len, 0.7, false));
            let u: [[f32; 4]; 2] = [c4(theme.horizon), c4(theme.zenith)];
            let mat = kit.material(Material {
                transparent: true, side: Side::Double,
                ..Material::custom(CustomShader { label: "water".into(), wgsl: format!("{NOISE}\n{}", include_str!("../../shaders/water.wgsl")), uniform: bytemuck::bytes_of(&u).to_vec(), ..Default::default() })
            });
            kit.scene.mesh(gp, geo, mat);
            // 石のふち（どこまでが水か分かるように）
            let sg = kit.geometry(&icosahedron(0.2, 0));
            let sm = soft(kit, SoftOptions { color: css("#b9a98f"), flat_shading: true, ..Default::default() });
            let stones = kit.scene.instanced(gp, sg, sm, 80);
            for i in 0..80 {
                let on_end = i < 56;
                let a = rr.next();
                let s = if on_end { ob.s + if i % 2 == 1 { -0.18 } else { ob.len + 0.18 } } else { ob.s + a * ob.len };
                let inner = if ob.d0 < 0.5 { ob.d1 } else { ob.d0 };
                let d = if on_end { ob.d0 + a * (ob.d1 - ob.d0) } else { inner };
                let v = tr.pos(s, d);
                let sc = 0.6 + rr.next() * 0.9;
                let (rx, ryy, rz) = (rr.next(), rr.next() * 3.0, rr.next());
                kit.scene.set_instance(stones, i, trs(Vec3::new(v.x as f32, (v.y + 0.05) as f32, v.z as f32), Rot::euler(rx as f32, ryy as f32, rz as f32).to_quat(), Vec3::new(sc as f32, (sc * 0.5) as f32, sc as f32)));
            }
            for k in 1..=3 { add_chevron(kit, ob.s - 3.0 - k as f64 * 7.0, (ob.d0 + ob.d1) / 2.0, ob.d1 - ob.d0, "#35c8f0", 0.95 - k as f32 * 0.15); }
        }
    }

    // ---- 小物: 木・風車など。走路をふさがない。同じ間隔に並べない ----
    {
        let mut list = [
            Scatter { name: "tree_round_large", count: 20, d_min: w + 22.0, d_max: w + 62.0, s_min: 2.4, s_max: 3.8, avoid: true, i: 0, pending: None },
            Scatter { name: "tree_round_small", count: 26, d_min: w + 16.0, d_max: w + 62.0, s_min: 1.8, s_max: 3.0, avoid: true, i: 0, pending: None },
            Scatter { name: "tree_conifer", count: 14, d_min: w + 26.0, d_max: w + 64.0, s_min: 2.4, s_max: 3.8, avoid: true, i: 0, pending: None },
            Scatter { name: "bush_round", count: 26, d_min: w + 7.0, d_max: w + 24.0, s_min: 1.1, s_max: 1.9, avoid: true, i: 0, pending: None },
            Scatter { name: "tree_round_small", count: 8, d_min: -46.0, d_max: -22.0, s_min: 1.8, s_max: 2.8, avoid: false, i: 0, pending: None },
            Scatter { name: "bush_round", count: 12, d_min: -34.0, d_max: -8.0, s_min: 1.0, s_max: 1.7, avoid: false, i: 0, pending: None },
            Scatter { name: "windmill", count: 2, d_min: w + 34.0, d_max: w + 58.0, s_min: 3.0, s_max: 4.0, avoid: true, i: 0, pending: None },
        ];
        // 次の await まで進める
        let advance = |c: &mut Scatter, rr: &mut Rng| {
            c.pending = None;
            while c.i < c.count {
                c.i += 1;
                let s = rr.next() * tr.lap;
                if c.avoid && s > stand_s0 - 12.0 && s < stand_s1 + 12.0 { continue; }
                let d = c.d_min + rr.next() * (c.d_max - c.d_min);
                let variant = (rr.next() * 2.0).floor() as usize;
                c.pending = Some((s, d, variant));
                return;
            }
        };
        for c in list.iter_mut() { advance(c, &mut rr); }
        loop {
            let mut any = false;
            for c in list.iter_mut() {
                let Some((s, d, variant)) = c.pending else { continue };
                any = true;
                if let Some(glb) = assets.props.get(c.name) {
                    let o = props.prop_object(kit, gp, glb, c.name, variant);
                    let v = tr.pos(s, d);
                    kit.scene[o].position = Vec3::new(v.x as f32, (v.y * if d > 0.0 { 0.6 } else { 0.4 }) as f32, v.z as f32);
                    kit.scene[o].rotation = Rot::y((rr.next() * 6.28) as f32);
                    kit.scene[o].set_scale((c.s_min + rr.next() * (c.s_max - c.s_min)) as f32);
                }
                advance(c, &mut rr);
            }
            if !any { break; }
        }
    }

    let center = || { let (mut cx, mut cz) = (0.0, 0.0); for i in 0..tr.n { cx += tr.x[i]; cz += tr.z[i]; } (cx / tr.n as f64, cz / tr.n as f64) };
    // ---- 水辺コース: 内馬場の大きな池 ----
    if tr.def.id == "water" {
        let (cx, cz) = center();
        let geo = kit.geometry(&pond_geometry());
        let u: [[f32; 4]; 2] = [c4(theme.horizon), c4(theme.mid)];
        let mat = kit.material(Material::custom(CustomShader { label: "pond".into(), wgsl: format!("{NOISE}\n{}", include_str!("../../shaders/pond.wgsl")), uniform: bytemuck::bytes_of(&u).to_vec(), ..Default::default() }));
        let pond = kit.scene.mesh(gp, geo, mat);
        kit.scene[pond].position = Vec3::new(cx as f32, 0.08, cz as f32);
        if let Some(glb) = assets.props.get("reed_clump") {
            for i in 0..8 {
                let o = props.prop_object(kit, gp, glb, "reed_clump", i % 2);
                let a = (i as f64 / 8.0) * 6.28 + rr.next();
                kit.scene[o].position = Vec3::new((cx + a.cos() * 63.0) as f32, 0.05, (cz + a.sin() * 27.0) as f32);
                kit.scene[o].set_scale(2.0);
            }
        }
    }

    // ---- 丘コース: 宵なので照明塔 ----
    if tr.def.id == "hill" {
        let map = glow_texture(kit);
        let glow = kit.material(Material { fog: false, ..Material::sprite().with_map(map).with_color(css("#fff0d0")).additive() });
        let h = 16.0f32;
        let pole_g = kit.geometry(&cylinder(0.22, 0.32, h, 8));
        let pole_m = soft_c(kit, "#3c4166");
        for i in 0..14 {
            let s = (i as f64 + 0.5) / 14.0 * tr.lap;
            let v = tr.pos(s, w + 5.0);
            let pole = kit.scene.mesh(gp, pole_g, pole_m);
            kit.scene[pole].position = Vec3::new(v.x as f32, v.y as f32 + h / 2.0, v.z as f32);
            let lamp = kit.scene.sprite(gp, glow);
            kit.scene[lamp].position = Vec3::new(v.x as f32, v.y as f32 + h + 0.4, v.z as f32);
            kit.scene[lamp].set_scale(9.0);
        }
    }

    // ---- 表彰台（走路の上。結果と出走登録のときだけ出す）----
    let podium = kit.scene.group(gp);
    let pedestal = kit.scene.group(gp);
    let stage_s = tr.wrap(tr.finish_s - 34.0);
    let stage_d = w * 0.6;
    // 出走登録の立ち位置（大画面が背景に入らない所）
    let select_s = tr.wrap(tr.finish_s - 104.0);
    let podium_pos = v3(tr.pos(stage_s, stage_d));
    let podium_yaw = tr.yaw(stage_s) + PI / 2.0;
    {
        kit.scene[podium].position = podium_pos;
        // 局所の +z が内馬場（カメラの側）。背景はスタンド
        kit.scene[podium].rotation = Rot::y(podium_yaw as f32);
        let base_m = soft(kit, SoftOptions { color: css("#fff4dc"), rim: 0.6, ..Default::default() });
        let heights = [0.95f32, 0.62, 0.4];
        let xs = [0.0f32, 1.62, -1.62];
        let zs = [0.0f32, -0.55, -0.55];
        let top_g = kit.geometry(&box_(1.62, 0.07, 1.62));
        for i in 0..3 {
            let h = heights[i];
            let bg = kit.geometry(&box_(1.56, h, 1.56));
            let b = kit.scene.mesh(Some(podium), bg, base_m);
            kit.scene[b].position = Vec3::new(xs[i], h / 2.0, zs[i]);
            kit.scene[b].cast_shadow = true;
            kit.scene[b].receive_shadow = true;
            let tm = soft_c(kit, ["#ffcf4d", "#d9dee4", "#d79a68"][i]);
            let top = kit.scene.mesh(Some(podium), top_g, tm);
            kit.scene[top].position = Vec3::new(xs[i], h + 0.04, zs[i]);
            kit.scene[top].cast_shadow = true;
            kit.scene[top].receive_shadow = true;
            let t = text_texture(kit, &(i + 1).to_string(), Text { w: 128, h: 128, size: 104, font: "'Fredoka', sans-serif", bg: "#fff4dc", fg: "#0f3d2e" });
            let nm = kit.material(Material::basic().with_map(t));
            let ns = h.min(0.7) * 0.9;
            let ng = kit.geometry(&plane(ns, ns));
            let num = kit.scene.mesh(Some(podium), ng, nm);
            kit.scene[num].position = Vec3::new(xs[i], h * 0.5, zs[i] + 0.785);
        }
        kit.scene[podium].visible = false;
        let pg = kit.geometry(&cylinder(1.25, 1.35, 0.18, 48));
        let pm = soft(kit, SoftOptions { color: css("#fff4dc"), rim: 0.5, ..Default::default() });
        let p0 = kit.scene.mesh(Some(pedestal), pg, pm);
        let rg = kit.geometry(&cylinder(1.0, 1.02, 0.02, 48));
        let rm = soft_c(kit, "#ffcf4d");
        let pr = kit.scene.mesh(Some(pedestal), rg, rm);
        kit.scene[pr].position.y = 0.1;
        for c in [p0, pr] { kit.scene[c].cast_shadow = true; kit.scene[c].receive_shadow = true; }
        let v = tr.pos(select_s, stage_d);
        kit.scene[pedestal].position = Vec3::new(v.x as f32, (v.y + 0.09) as f32, v.z as f32);
        kit.scene[pedestal].visible = false;
    }

    // 空の気球: 遠くでゆっくり動く
    let mut balloons = Vec::new();
    {
        let (cx, cz) = center();
        let cols = ["#e23b35", "#ffcf4d", "#2f6fd8", "#f39ac0", "#2f9e5b"];
        let env_g = kit.geometry(&sphere(6.0, 14, 10).scale(1.0, 1.18, 1.0).owned());
        let band_g = kit.geometry(&sphere_full(6.06, 14, 4, 0.0, std::f32::consts::TAU, fpi * 0.42, fpi * 0.16).scale(1.0, 1.18, 1.0).owned());
        let basket_g = kit.geometry(&box_(1.8, 1.4, 1.8));
        let band_m = soft_c(kit, "#fff6e4");
        let basket_m = soft_c(kit, "#8c5a2c");
        for i in 0..7 {
            let g = kit.scene.group(gp);
            let env_m = soft(kit, SoftOptions { color: css(cols[i % cols.len()]), rim: 0.8, ..Default::default() });
            let env = kit.scene.mesh(Some(g), env_g, env_m);
            kit.scene[env].position.y = 9.0;
            let band = kit.scene.mesh(Some(g), band_g, band_m);
            kit.scene[band].position.y = 9.0;
            kit.scene.mesh(Some(g), basket_g, basket_m);
            let a = rr.next() * 6.28;
            let r = 190.0 + rr.next() * 120.0;
            let y = 30.0 + rr.next() * 45.0;
            kit.scene[g].position = Vec3::new((cx + a.cos() * r * 1.25) as f32, y as f32, (cz + a.sin() * r) as f32);
            let _sp = 0.004 + rr.next() * 0.006;
            balloons.push(Balloon { o: g, y });
        }
    }

    // 雲: 空にふくらむ白い雲（地平のそばに低く）
    {
        let map = cloud_texture(kit);
        let stars = theme.stars > 0.4;
        let cloud_m = kit.material(Material { depth_write: false, fog: false, ..Material::sprite().with_map(map).with_color(css(if stars { "#7a6ea8" } else { "#ffffff" })).with_opacity(if stars { 0.5 } else { 0.92 }) });
        let (cx, cz) = center();
        for i in 0..26 {
            let a = (i as f64 / 26.0) * 6.28 + rr.next() * 0.3;
            let r = 420.0 + rr.next() * 220.0;
            let sp = kit.scene.sprite(gp, cloud_m);
            let sc = 120.0 + rr.next() * 170.0;
            let y = 16.0 + rr.next() * 80.0;
            kit.scene[sp].position = Vec3::new((cx + a.cos() * r) as f32, y as f32, (cz + a.sin() * r) as f32);
            let sx = sc * (1.6 + rr.next() * 0.8);
            kit.scene[sp].scale = Vec3::new(sx as f32, (sc * 0.62) as f32, 1.0);
            kit.scene.set_sprite_center(sp, Vec2::new(0.5, 0.3));
        }
    }

    // 雲: 島の縁に漂う
    {
        let map = glow_texture(kit);
        let cm = kit.material(Material { depth_write: false, ..Material::sprite().with_map(map).with_color(theme.cloud).with_opacity(0.55) });
        for _ in 0..26 {
            let s = rr.next() * tr.lap;
            let v = tr.pos(s, w + outer_span + 14.0 + rr.next() * 45.0);
            let sp = kit.scene.sprite(gp, cm);
            let y = -8.0 - rr.next() * 22.0;
            kit.scene[sp].position = Vec3::new(v.x as f32, y as f32, v.z as f32);
            let sc = 26.0 + rr.next() * 40.0;
            kit.scene[sp].scale = Vec3::new((sc * 2.0) as f32, (sc * 0.75) as f32, 1.0);
        }
    }

    let bp = tr.pos(tr.wrap(tr.finish_s - 24.0), -22.0);
    Course {
        group, board: Board { tex: board_tex, live, live_mat, live_set: false, pos: (bp.x, bp.z) }, podium, pedestal, stage_s, stage_d, select_s, finish, podium_yaw, podium_pos,
        cheer: 0.0, cheer_at: [0.0, -999.0, 0.0], gate_open: 0.0, gate, gate_y, doors, rem_boards, low_hide, balloons, crowd,
    }
}

impl Course {
    pub fn update_sky(&self, kit: &mut Kit, time: f64) {
        for (k, b) in self.balloons.iter().enumerate() {
            let i = k + 1;
            kit.scene[b.o].position.y = (b.y + (time * 0.2 + i as f64).sin() * 3.0) as f32;
            kit.scene[b.o].rotation = Rot::y((time * 0.05 * if i % 2 == 1 { 1.0 } else { -1.0 }) as f32);
        }
    }
    /// 走り出して少しすると、ゲートは空へ持ち上がる（次の周の邪魔にならない）
    pub fn lift_gate(&self, kit: &mut Kit, t: f64) {
        let k = (t - 1.1).max(0.0);
        kit.scene[self.gate].position.y = self.gate_y + (k * k * 5.0) as f32;
        kit.scene[self.gate].visible = k < 4.0;
    }
    pub fn open_gate(&mut self, kit: &mut Kit, k: f64) {
        self.gate_open = k;
        let a = (1.0 - (1.0 - k.min(1.0)).powi(3)) * 1.6;
        for (l, r) in &self.doors {
            kit.scene[*l].rotation = Rot::y(a as f32);
            kit.scene[*r].rotation = Rot::y(-a as f32);
        }
    }
    pub fn set_gate_visible(&self, kit: &mut Kit, on: bool) { kit.scene[self.gate].visible = on; }
    pub fn show_rem_boards(&self, kit: &mut Kit, on: bool) { for b in &self.rem_boards { kit.scene[*b].visible = on; } }
    pub fn set_low(&self, kit: &mut Kit, on: bool) { for o in &self.low_hide { kit.scene[*o].visible = !on; } }
    /// 歓声の値を材質へ（TS は uniform を共有している）
    pub fn write_cheer(&self, kit: &mut Kit) {
        for (m, vc) in &self.crowd {
            let u = CrowdU { p: [self.cheer as f32, *vc, 0.0, 0.0], runner: [self.cheer_at[0], self.cheer_at[1], self.cheer_at[2], 0.0] };
            kit.set_uniform(*m, bytemuck::bytes_of(&u));
        }
    }
    /// 映像の板の色と的（mat.color・mat.map）
    pub fn set_live(&mut self, kit: &mut Kit, color: Color, rt: Option<TexId>) {
        kit.set_uniform(self.board.live_mat, bytemuck::bytes_of(&[color.r, color.g, color.b, 1.0]));
        if let Some(t) = rt && !self.board.live_set {
            self.board.live_set = true;
            if let MatKind::Custom(c) = &mut kit.mat_mut(self.board.live_mat).kind { c.textures = vec![t]; }
        }
    }
}
