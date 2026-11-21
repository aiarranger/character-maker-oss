// 防ぐ故障: three の式を写し損ねて形や UV・並びがずれる。fixtures/gen-geometry.mjs が three で作った値と、同じ引数で作った形を比べる。
// fixture の項目はすべてここで作る（作り方の無い項目があれば失敗する）。
use glam::{DVec2, Mat4, Vec2, Vec3};
use island_kit::geometry::*;
use serde_json::Value;
use std::f32::consts::{PI, TAU};
use std::f64::consts::PI as PI64;

const EPS: f64 = 1e-4;

fn v2(x: f64, y: f64) -> DVec2 { DVec2::new(x, y) }
fn lathe_pts() -> Vec<Vec2> { vec![Vec2::new(0.0, -1.0), Vec2::new(1.0, -0.5), Vec2::new(0.8, 0.5), Vec2::new(0.0, 1.0)] }

fn chevron() -> Shape { let mut s = Shape::new(); s.move_to(-0.26, 0.0).line_to(0.0, 0.2).line_to(0.26, 0.0).line_to(0.26, 0.09).line_to(0.0, 0.29).line_to(-0.26, 0.09); s }
fn star(rr: f64, r0: f64, fx: &Value) -> Shape {
    let mut s = Shape::new();
    let cs = &fx["star_input"]["cossin"];
    let pt = |i: i32, q: f64| { let c = &cs[i as usize]; v2(c[0].as_f64().unwrap() * q, c[1].as_f64().unwrap() * q) };
    let lerp = |a: DVec2, b: DVec2, k: f64| a + (b - a) * k;
    for i in 0..10 {
        let cur = pt(i, if i % 2 != 0 { r0 } else { rr });
        let nxt = pt(i + 1, if (i + 1) % 2 != 0 { r0 } else { rr });
        let k = if i % 2 != 0 { 0.0 } else { 0.22 };
        let a = lerp(cur, nxt, k);
        let b = lerp(cur, nxt, 1.0 - if (i + 1) % 2 != 0 { 0.0 } else { 0.22 });
        if i == 0 { s.move_to(a.x, a.y); } else { s.quadratic_curve_to(cur.x, cur.y, a.x, a.y); }
        s.line_to(b.x, b.y);
    }
    let p0 = pt(0, rr);
    let a0 = lerp(p0, pt(1, r0), 0.22);
    s.quadratic_curve_to(p0.x, p0.y, a0.x, a0.y);
    s
}
fn bird() -> Shape { let mut s = Shape::new(); s.move_to(0.0, 0.18); s.bezier_curve_to(0.8, 0.5, 1.7, 0.45, 2.3, 0.15); s.line_to(2.1, -0.05); s.bezier_curve_to(1.0, -0.35, 0.4, -0.3, 0.0, -0.18); s.close_path(); s }
fn arch() -> Shape { let mut s = Shape::new(); s.move_to(-0.45, 0.0).line_to(-0.45, 1.1).absarc(0.0, 1.1, 0.45, PI64, 0.0, true).line_to(0.45, 0.0).close_path(); s }
fn holed() -> Shape {
    let mut s = Shape::new();
    s.move_to(-1.0, -1.0).line_to(1.0, -1.0).line_to(1.0, 1.0).line_to(-1.0, 1.0).close_path();
    let mut h = Path::new();
    h.absarc(0.2, 0.1, 0.4, 0.0, PI64 * 2.0, false);
    s.holes.push(h);
    let mut h2 = Path::new();
    h2.move_to(-0.8, -0.8).line_to(-0.5, -0.8).line_to(-0.5, -0.5).line_to(-0.8, -0.5).close_path();
    s.holes.push(h2);
    s
}
fn disc(r: f64) -> Shape { let mut s = Shape::new(); s.absarc(0.0, 0.0, r, 0.0, PI64 * 2.0, false); s }
fn disc_holed() -> Shape {
    let mut s = disc(2.0);
    let mut h = Path::new();
    h.absarc(0.5, 0.0, 0.6, 0.0, PI64 * 2.0, true);
    s.holes.push(h);
    let mut h2 = Path::new();
    h2.absarc(-0.9, 0.2, 0.5, 0.0, PI64 * 2.0, false);
    s.holes.push(h2);
    s
}
fn bowtie() -> Shape { Shape::from_points(&[v2(0.0, 0.0), v2(2.0, 2.0), v2(2.0, 0.0), v2(0.0, 2.0), v2(-1.0, 1.0)]) }
fn self_cross() -> Shape { Shape::from_points(&[v2(0.0, 0.0), v2(4.0, 0.0), v2(1.0, 3.0), v2(2.0, -1.0), v2(3.0, 3.0), v2(0.0, 1.0), v2(4.0, 1.5), v2(-1.0, 2.0)]) }
fn cw() -> Shape { Shape::from_points(&[v2(0.0, 0.0), v2(0.0, 1.0), v2(1.0, 1.0), v2(1.0, 0.0)]) }
fn splined() -> Shape { let mut s = Shape::new(); s.move_to(0.0, 0.0).spline_thru(&[v2(1.0, 0.5), v2(2.0, 0.0), v2(1.5, 1.5)]).line_to(0.0, 1.0).ellipse(0.0, -0.5, 0.5, 0.3, PI64 / 2.0, PI64 * 1.5, false, 0.3); s }
fn rr(ww: f64, hh: f64, r: f64) -> Shape {
    let mut s = Shape::new();
    let (x, y) = (-ww / 2.0, 0.0);
    s.move_to(x + r, y).line_to(x + ww - r, y).quadratic_curve_to(x + ww, y, x + ww, y + r).line_to(x + ww, y + hh - r).quadratic_curve_to(x + ww, y + hh, x + ww - r, y + hh).line_to(x + r, y + hh).quadratic_curve_to(x, y + hh, x, y + hh - r).line_to(x, y + r).quadratic_curve_to(x, y, x + r, y);
    s
}
fn frame() -> Shape {
    let mut outer = rr(2.0, 3.0, 0.2);
    let inner = rr(1.6, 2.6, 0.15);
    let pts: Vec<DVec2> = inner.get_points(8).iter().map(|p| v2(p.x, p.y + 0.13)).collect();
    outer.holes.push(Path::from_points(&pts));
    outer
}
fn cr_pts() -> Vec<Vec3> { vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(1.0, 2.0, 0.5), Vec3::new(2.0, 0.5, 1.0), Vec3::new(4.0, 1.0, -1.0), Vec3::new(3.0, -1.0, 0.0)] }
fn crv() -> CatmullRomCurve3 { CatmullRomCurve3::new(&cr_pts()) }
fn crv_chordal() -> CatmullRomCurve3 { CatmullRomCurve3::with(&cr_pts(), false, CurveType::Chordal, 0.5) }
fn crv_closed() -> CatmullRomCurve3 { CatmullRomCurve3::with(&cr_pts(), true, CurveType::Centripetal, 0.5) }
fn crv_cr() -> CatmullRomCurve3 { CatmullRomCurve3::with(&cr_pts(), false, CurveType::CatmullRom, 0.1) }
fn crv_two() -> CatmullRomCurve3 { CatmullRomCurve3::new(&[Vec3::ZERO, Vec3::new(1.0, 1.0, 0.0)]) }
fn box_s() -> Geometry { box_seg(1.0, 2.0, 3.0, 1, 1, 1) }

fn test_matrix(fixture: &Value) -> Mat4 {
    let e = fixture["matrix"]["points"].as_array().unwrap();
    let a: Vec<f32> = e.iter().map(|v| v.as_f64().unwrap() as f32).collect();
    Mat4::from_cols_slice(&a)
}

fn build(name: &str, fx: &Value) -> Option<Geometry> {
    let ex = |depth, thick, size, segs, curve| ExtrudeOpts { depth, bevel_enabled: true, bevel_thickness: thick, bevel_size: Some(size), bevel_segments: segs, curve_segments: curve, ..Default::default() };
    Some(match name {
        "plane" => plane(2.0, 3.0),
        "plane_seg" => plane_seg(2.0, 3.0, 2, 3),
        "box" => box_(1.0, 1.0, 1.0),
        "box_seg" => box_seg(2.0, 3.0, 4.0, 1, 2, 3),
        "sphere" => sphere(1.0, 8, 6),
        "sphere_part" => sphere_full(0.5, 6, 4, 0.3, 2.0, 0.4, 1.2),
        "sphere_top" => sphere_full(1.0, 8, 4, 0.0, TAU, 0.0, PI / 2.0),
        "sphere_bottom" => sphere_full(1.0, 8, 4, 0.0, TAU, PI / 2.0, PI / 2.0),
        "cylinder" => cylinder(1.0, 1.0, 2.0, 8),
        "cylinder_seg" => cylinder_seg(0.5, 1.0, 2.0, 6, 2),
        "cylinder_open" => cylinder_full(1.0, 0.5, 2.0, 6, 1, true, 0.0, TAU),
        "cylinder_part" => cylinder_full(1.0, 1.0, 1.0, 6, 1, false, 0.5, 3.0),
        "cylinder_tip" => cylinder_seg(0.0, 1.0, 2.0, 5, 2),
        "cone" => cone(1.0, 2.0, 6),
        "cone_full" => cone_full(0.5, 1.0, 8, 2, true, 0.2, 4.0),
        "circle" => circle(1.0, 8),
        "circle_part" => circle_full(2.0, 5, 0.5, 2.0),
        "ring" => ring(0.5, 1.0, 8),
        "ring_full" => ring_full(0.3, 1.2, 6, 3, 0.4, 3.0),
        "torus" => torus(1.0, 0.4, 6, 8),
        "torus_arc" => torus_full(1.0, 0.2, 4, 6, 3.0),
        "lathe" => lathe(&lathe_pts(), 8),
        "lathe_part" => lathe_full(&lathe_pts(), 5, 0.5, 2.0),
        "capsule" => capsule(0.5, 1.0, 3, 6),
        "capsule_full" => capsule_full(1.0, 2.0, 2, 5, 3),
        "icosa" => icosahedron(1.0, 0),
        "icosa_1" => icosahedron(2.0, 1),
        "icosa_2" => icosahedron(1.0, 2),
        "octa" => octahedron(1.0, 0),
        "octa_2" => octahedron(1.5, 2),
        "tetra" => tetrahedron(1.0, 0),
        "tetra_1" => tetrahedron(1.0, 1),
        "dodeca" => dodecahedron(1.0, 0),
        "dodeca_1" => dodecahedron(1.0, 1),
        "rounded" => rounded_box(1.0, 1.0, 1.0, 2, 0.1),
        "rounded_wide" => rounded_box(2.0, 1.0, 0.5, 3, 0.2),
        "rounded_big_r" => rounded_box(1.0, 1.0, 1.0, 1, 5.0),
        "rounded_0" => rounded_box(1.0, 2.0, 3.0, 0, 0.1),
        "shape_chevron" => shape_geometry(&chevron(), 12),
        "shape_star" => shape_geometry(&star(0.62, 0.32, fx), 6),
        "shape_bird" => shape_geometry(&bird(), 8),
        "shape_arch" => shape_geometry(&arch(), 8),
        "shape_holed" => shape_geometry(&holed(), 10),
        "shape_disc_big" => shape_geometry(&disc(1.5), 48),
        "shape_disc_holed_big" => shape_geometry(&disc_holed(), 32),
        "shape_bowtie" => shape_geometry(&bowtie(), 12),
        "shape_cross" => shape_geometry(&self_cross(), 12),
        "shape_cw" => shape_geometry(&cw(), 12),
        "shape_spline" => shape_geometry(&splined(), 6),
        "shape_array" => shapes_geometry(&[chevron(), arch()], 4),
        "extrude_default" => extrude(&chevron(), &ExtrudeOpts::default()),
        "extrude_star" => extrude(&star(0.62, 0.32, fx), &ex(0.2, 0.12, 0.1, 3, 4)),
        "extrude_flat" => extrude(&arch(), &ExtrudeOpts { depth: 0.5, bevel_enabled: false, curve_segments: 6, ..Default::default() }),
        "extrude_steps" => extrude(&chevron(), &ExtrudeOpts { depth: 1.0, steps: 3, bevel_enabled: false, ..Default::default() }),
        "extrude_steps_bevel" => extrude(&chevron(), &ExtrudeOpts { depth: 1.0, steps: 2, bevel_thickness: 0.05, bevel_size: Some(0.04), bevel_offset: 0.01, bevel_segments: 2, ..Default::default() }),
        "extrude_holed" => extrude(&holed(), &ex(0.3, 0.04, 0.03, 2, 8)),
        "extrude_frame" => extrude(&frame(), &ex(0.16, 0.04, 0.04, 2, 6)),
        "extrude_big" => extrude(&disc(1.0), &ExtrudeOpts { depth: 0.1, bevel_enabled: false, curve_segments: 48, ..Default::default() }),
        "extrude_cw" => extrude(&cw(), &ex(1.0, 0.1, 0.05, 1, 12)),
        "extrude_size_default" => extrude(&cw(), &ExtrudeOpts { depth: 1.0, bevel_thickness: 0.3, bevel_segments: 2, ..Default::default() }),
        "extrude_array" => extrude_many(&[chevron(), cw()], &ExtrudeOpts { depth: 0.5, bevel_enabled: false, ..Default::default() }),
        "tube" => tube(&crv(), 8, 0.2, 4),
        "tube_chordal" => tube(&crv_chordal(), 6, 0.1, 5),
        "tube_catmullrom" => tube(&crv_cr(), 10, 0.12, 5),
        "tube_closed" => tube_full(&crv_closed(), 12, 0.3, 3, true),
        "tube_two" => tube(&crv_two(), 4, 0.1, 4),
        "rotate_x" => box_s().rotate_x(0.7).owned(),
        "rotate_y" => box_s().rotate_y(-1.1).owned(),
        "rotate_z" => box_s().rotate_z(2.3).owned(),
        "translate" => box_s().translate(1.0, -2.0, 3.0).owned(),
        "scale" => sphere(1.0, 6, 4).scale(2.0, 0.5, 3.0).owned(),
        "center" => sphere_full(1.0, 6, 4, 0.0, PI, 0.0, PI / 2.0).translate(3.0, 4.0, 5.0).center().owned(),
        "translate_lathe" => lathe(&lathe_pts(), 4).translate(0.0, 1.0, 0.0).owned(),
        "apply_matrix4" => cylinder(1.0, 0.5, 2.0, 5).apply_matrix4(test_matrix(fx)).owned(),
        "normals_indexed" => sphere(1.0, 6, 4).compute_vertex_normals().owned(),
        "normals_soup" => sphere(1.0, 6, 4).to_non_indexed().compute_vertex_normals().owned(),
        "normals_new" => {
            let mut g = Geometry { positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [1.0, 1.0, 0.5]], indices: Some(vec![0, 1, 2, 2, 1, 3]), ..Default::default() };
            g.compute_vertex_normals();
            g
        }
        "non_indexed" => box_seg(1.0, 1.0, 1.0, 2, 1, 1).to_non_indexed(),
        "merge" => merge_geometries(&[&box_(1.0, 1.0, 1.0), plane(2.0, 2.0).translate(0.0, 0.0, 2.0)], false).unwrap(),
        "merge_groups" => merge_geometries(&[&box_(1.0, 1.0, 1.0), &plane(2.0, 2.0)], true).unwrap(),
        "merge_soup" => merge_geometries(&[&icosahedron(1.0, 0), &tetrahedron(1.0, 0)], true).unwrap(),
        _ => return None,
    })
}

fn nums(v: &Value) -> Vec<f64> { v.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() }

fn close(name: &str, what: &str, want: &[f64], got: &[f64]) -> Result<(), String> {
    if want.len() != got.len() { return Err(format!("{name}.{what}: 数が違う three={} rust={}", want.len(), got.len())); }
    for (i, (a, b)) in want.iter().zip(got).enumerate() {
        if (a - b).abs() > EPS { return Err(format!("{name}.{what}[{i}]: three={a} rust={b}")); }
    }
    Ok(())
}

fn check(name: &str, g: &Geometry, c: &Value) -> Result<(), String> {
    close(name, "position", &nums(&c["position"]), &g.positions.iter().flatten().map(|v| *v as f64).collect::<Vec<_>>())?;
    close(name, "normal", &nums(&c["normal"]), &g.normals.iter().flatten().map(|v| *v as f64).collect::<Vec<_>>())?;
    close(name, "uv", &nums(&c["uv"]), &g.uvs.iter().flatten().map(|v| *v as f64).collect::<Vec<_>>())?;
    let index: Option<Vec<u32>> = c["index"].as_array().map(|a| a.iter().map(|x| x.as_u64().unwrap() as u32).collect());
    if index != g.indices { return Err(format!("{name}.index: 違う\n three={index:?}\n rust ={:?}", g.indices)); }
    let groups: Vec<Group> = c["groups"].as_array().unwrap().iter().map(|x| Group { start: x[0].as_u64().unwrap() as u32, count: x[1].as_u64().unwrap() as u32, material: x[2].as_u64().unwrap() as u32 }).collect();
    if groups != g.groups { return Err(format!("{name}.groups: three={groups:?} rust={:?}", g.groups)); }
    let (center, radius) = g.bounding_sphere();
    close(name, "sphere", &nums(&c["sphere"]), &[center.x as f64, center.y as f64, center.z as f64, radius as f64])?;
    Ok(())
}

#[test]
fn same_as_three() {
    let fx: Value = serde_json::from_str(include_str!("fixtures/geometry.json")).unwrap();
    let mut errors = Vec::new();
    let flat3 = |p: Vec<Vec3>| p.iter().flat_map(|v| [v.x as f64, v.y as f64, v.z as f64]).collect::<Vec<_>>();
    let flat2 = |p: Vec<DVec2>| p.iter().flat_map(|v| [v.x, v.y]).collect::<Vec<_>>();
    let mut n = 0;
    for (name, c) in fx.as_object().unwrap() {
        n += 1;
        let us = [0.0, 0.1, 0.33, 0.5, 0.77, 1.0];
        let points: Option<Vec<f64>> = match name.as_str() {
            "path_points_arch" => Some(flat2(arch().get_points(8))),
            "path_points_bird" => Some(flat2(bird().get_points(6))),
            "path_points_spline" => Some(flat2(splined().get_points(4))),
            "curve_points" => Some(flat3(crv().get_points(10))),
            "curve_points_at" => Some(flat3(us.iter().map(|u| crv().get_point_at(*u)).collect())),
            "curve_tangents_at" => Some(flat3(us.iter().map(|u| crv().get_tangent_at(*u)).collect())),
            "curve_spaced" => Some(flat3(crv_chordal().get_spaced_points(7))),
            "curve_closed" => Some(flat3(crv_closed().get_points(11))),
            "curve_catmullrom" => Some(flat3(crv_cr().get_points(9))),
            "curve_two" => Some(flat3(crv_two().get_points(4))),
            "curve_length" => Some(vec![crv().get_length() as f64, crv_closed().get_length() as f64]),
            _ => None,
        };
        if let Some(got) = points {
            if let Err(e) = close(name, "points", &nums(&c["points"]), &got) { errors.push(e); }
            continue;
        }
        if name == "matrix" || name == "star_input" { continue; }
        if name == "merge_bad" {
            assert_eq!(c["isNull"], Value::Bool(true));
            assert!(merge_geometries(&[&box_(1.0, 1.0, 1.0), &icosahedron(1.0, 0)], false).is_none());
            continue;
        }
        match build(name, &fx) {
            Some(g) => if let Err(e) = check(name, &g, c) { errors.push(e); },
            None => errors.push(format!("{name}: Rust 側の作り方が無い")),
        }
    }
    assert!(errors.is_empty(), "{} / {n} 件ずれ:\n{}", errors.len(), errors.join("\n"));
}

// 防ぐ故障: 変形や結合で頂点色・揺れの量が落ちる（three の属性は全部一緒に動く）
#[test]
fn colors_and_sway_follow() {
    let mut a = plane(1.0, 1.0);
    a.colors = vec![[1.0, 0.0, 0.0, 1.0], [0.0, 1.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0], [1.0, 1.0, 1.0, 1.0]];
    a.sway = vec![0.0, 0.1, 0.2, 0.3];
    let flat = a.to_non_indexed();
    assert_eq!(flat.colors.len(), 6);
    assert_eq!(flat.colors[0], [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(flat.colors[1], [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(flat.sway, vec![0.0, 0.2, 0.1, 0.2, 0.3, 0.1]);
    let m = merge_geometries(&[&a, &a], false).unwrap();
    assert_eq!(m.colors.len(), 8);
    assert_eq!(m.sway.len(), 8);
    let plain = plane(1.0, 1.0);
    assert!(merge_geometries(&[&a, &plain], false).is_none());
    let mut moved = a.clone();
    moved.translate(1.0, 0.0, 0.0);
    assert_eq!(moved.colors, a.colors);
    assert_eq!(moved.sway, a.sway);
}
