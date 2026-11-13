// 基本の形。three の geometries/*.js と同じ頂点の並び・法線・UV・index・groups。
// 関数は three のコンストラクタと同じ引数の順。短い名前はよく使う引数だけ（残りは three の既定）、`_full` は全部。
use super::{Geometry, js_sign, norm, p3};
use glam::{DVec2, DVec3, Vec2};
use std::f64::consts::PI;

const TAU: f64 = PI * 2.0;
const TAU32: f32 = std::f32::consts::TAU;

fn p(x: f64, y: f64, z: f64) -> [f32; 3] { [x as f32, y as f32, z as f32] }
fn t(u: f64, v: f64) -> [f32; 2] { [u as f32, v as f32] }

// ---- Plane ----
pub fn plane(width: f32, height: f32) -> Geometry { plane_seg(width, height, 1, 1) }
pub fn plane_seg(width: f32, height: f32, width_segments: u32, height_segments: u32) -> Geometry {
    let (w, h) = (width as f64, height as f64);
    let (gx, gy) = (width_segments, height_segments);
    let (sw, sh) = (w / gx as f64, h / gy as f64);
    let mut g = Geometry::default();
    for iy in 0..=gy {
        let y = iy as f64 * sh - h / 2.0;
        for ix in 0..=gx {
            let x = ix as f64 * sw - w / 2.0;
            g.positions.push(p(x, -y, 0.0));
            g.normals.push([0.0, 0.0, 1.0]);
            g.uvs.push(t(ix as f64 / gx as f64, 1.0 - iy as f64 / gy as f64));
        }
    }
    let mut idx = Vec::new();
    for iy in 0..gy {
        for ix in 0..gx {
            let a = ix + (gx + 1) * iy;
            let b = ix + (gx + 1) * (iy + 1);
            let c = ix + 1 + (gx + 1) * (iy + 1);
            let d = ix + 1 + (gx + 1) * iy;
            idx.extend_from_slice(&[a, b, d, b, c, d]);
        }
    }
    g.indices = Some(idx);
    g
}

// ---- Box（`box` は Rust の予約語なので box_） ----
pub fn box_(width: f32, height: f32, depth: f32) -> Geometry { box_seg(width, height, depth, 1, 1, 1) }
pub fn box_seg(width: f32, height: f32, depth: f32, width_segments: u32, height_segments: u32, depth_segments: u32) -> Geometry {
    let (w, h, d) = (width as f64, height as f64, depth as f64);
    let (ws, hs, ds) = (width_segments, height_segments, depth_segments);
    let mut g = Geometry::default();
    let mut idx = Vec::new();
    let (mut count_v, mut group_start) = (0u32, 0u32);
    // u, v, w の軸（0 x・1 y・2 z）
    let mut build = |u: usize, v: usize, a: usize, udir: f64, vdir: f64, width: f64, height: f64, depth: f64, gx: u32, gy: u32, mat: u32| {
        let (sw, sh) = (width / gx as f64, height / gy as f64);
        let mut vc = 0;
        let mut gc = 0;
        for iy in 0..=gy {
            let y = iy as f64 * sh - height / 2.0;
            for ix in 0..=gx {
                let x = ix as f64 * sw - width / 2.0;
                let mut vec = [0.0f64; 3];
                vec[u] = x * udir;
                vec[v] = y * vdir;
                vec[a] = depth / 2.0;
                g.positions.push(p(vec[0], vec[1], vec[2]));
                let mut n = [0.0f64; 3];
                n[a] = if depth > 0.0 { 1.0 } else { -1.0 };
                g.normals.push(p(n[0], n[1], n[2]));
                g.uvs.push(t(ix as f64 / gx as f64, 1.0 - iy as f64 / gy as f64));
                vc += 1;
            }
        }
        for iy in 0..gy {
            for ix in 0..gx {
                let a = count_v + ix + (gx + 1) * iy;
                let b = count_v + ix + (gx + 1) * (iy + 1);
                let c = count_v + ix + 1 + (gx + 1) * (iy + 1);
                let d = count_v + ix + 1 + (gx + 1) * iy;
                idx.extend_from_slice(&[a, b, d, b, c, d]);
                gc += 6;
            }
        }
        g.groups.push(super::Group { start: group_start, count: gc, material: mat });
        group_start += gc;
        count_v += vc;
    };
    build(2, 1, 0, -1.0, -1.0, d, h, w, ds, hs, 0);
    build(2, 1, 0, 1.0, -1.0, d, h, -w, ds, hs, 1);
    build(0, 2, 1, 1.0, 1.0, w, d, h, ws, ds, 2);
    build(0, 2, 1, 1.0, -1.0, w, d, -h, ws, ds, 3);
    build(0, 1, 2, 1.0, -1.0, w, h, d, ws, hs, 4);
    build(0, 1, 2, -1.0, -1.0, w, h, -d, ws, hs, 5);
    g.indices = Some(idx);
    g
}

// ---- Sphere ----
pub fn sphere(radius: f32, width_segments: u32, height_segments: u32) -> Geometry {
    sphere_full(radius, width_segments, height_segments, 0.0, TAU32, 0.0, std::f32::consts::PI)
}
/// 極の判定は three では `thetaEnd === Math.PI`。引数が f32 なので π の丸めの差は π と見なす
pub fn sphere_full(radius: f32, width_segments: u32, height_segments: u32, phi_start: f32, phi_length: f32, theta_start: f32, theta_length: f32) -> Geometry {
    let (r, ps, pl, ts, tl) = (radius as f64, phi_start as f64, phi_length as f64, theta_start as f64, theta_length as f64);
    let ws = width_segments.max(3);
    let hs = height_segments.max(2);
    let te = ts + tl;
    let theta_end = if te >= PI - 1e-6 { PI } else { te };
    let mut g = Geometry::default();
    for iy in 0..=hs {
        let v = iy as f64 / hs as f64;
        let mut u_off = 0.0;
        if iy == 0 && ts == 0.0 { u_off = 0.5 / ws as f64; } else if iy == hs && theta_end == PI { u_off = -0.5 / ws as f64; }
        for ix in 0..=ws {
            let u = ix as f64 / ws as f64;
            let vtx = DVec3::new(
                -r * (ps + u * pl).cos() * (ts + v * tl).sin(),
                r * (ts + v * tl).cos(),
                r * (ps + u * pl).sin() * (ts + v * tl).sin(),
            );
            g.positions.push(p3(vtx));
            g.normals.push(p3(norm(vtx)));
            g.uvs.push(t(u + u_off, 1.0 - v));
        }
    }
    let row = ws + 1;
    let mut idx = Vec::new();
    for iy in 0..hs {
        for ix in 0..ws {
            let a = iy * row + ix + 1;
            let b = iy * row + ix;
            let c = (iy + 1) * row + ix;
            let d = (iy + 1) * row + ix + 1;
            if iy != 0 || ts > 0.0 { idx.extend_from_slice(&[a, b, d]); }
            if iy != hs - 1 || theta_end < PI { idx.extend_from_slice(&[b, c, d]); }
        }
    }
    g.indices = Some(idx);
    g
}

// ---- Cylinder（groups: 0 側面・1 上・2 下） ----
pub fn cylinder(radius_top: f32, radius_bottom: f32, height: f32, radial_segments: u32) -> Geometry {
    cylinder_full(radius_top, radius_bottom, height, radial_segments, 1, false, 0.0, TAU32)
}
pub fn cylinder_seg(radius_top: f32, radius_bottom: f32, height: f32, radial_segments: u32, height_segments: u32) -> Geometry {
    cylinder_full(radius_top, radius_bottom, height, radial_segments, height_segments, false, 0.0, TAU32)
}
#[allow(clippy::too_many_arguments)]
pub fn cylinder_full(radius_top: f32, radius_bottom: f32, height: f32, radial_segments: u32, height_segments: u32, open_ended: bool, theta_start: f32, theta_length: f32) -> Geometry {
    let (rt, rb, h, ts, tl) = (radius_top as f64, radius_bottom as f64, height as f64, theta_start as f64, theta_length as f64);
    let (rs, hs) = (radial_segments, height_segments);
    let half = h / 2.0;
    let mut g = Geometry::default();
    let mut idx: Vec<u32> = Vec::new();
    let slope = (rb - rt) / h;
    for y in 0..=hs {
        let v = y as f64 / hs as f64;
        let radius = v * (rb - rt) + rt;
        for x in 0..=rs {
            let u = x as f64 / rs as f64;
            let th = u * tl + ts;
            let (s, c) = (th.sin(), th.cos());
            g.positions.push(p(radius * s, -v * h + half, radius * c));
            g.normals.push(p3(norm(DVec3::new(s, slope, c))));
            g.uvs.push(t(u, 1.0 - v));
        }
    }
    let row = rs + 1;
    for x in 0..rs {
        for y in 0..hs {
            let a = y * row + x;
            let b = (y + 1) * row + x;
            let c = (y + 1) * row + x + 1;
            let d = y * row + x + 1;
            if rt > 0.0 || y != 0 { idx.extend_from_slice(&[a, b, d]); }
            if rb > 0.0 || y != hs - 1 { idx.extend_from_slice(&[b, c, d]); }
        }
    }
    g.add_group(0, idx.len() as u32, 0);
    let mut cap = |g: &mut Geometry, top: bool| {
        let center_start = g.positions.len() as u32;
        let radius = if top { rt } else { rb };
        let sign = if top { 1.0 } else { -1.0 };
        for _ in 1..=rs {
            g.positions.push(p(0.0, half * sign, 0.0));
            g.normals.push(p(0.0, sign, 0.0));
            g.uvs.push([0.5, 0.5]);
        }
        let center_end = g.positions.len() as u32;
        for x in 0..=rs {
            let u = x as f64 / rs as f64;
            let th = u * tl + ts;
            let (c, s) = (th.cos(), th.sin());
            g.positions.push(p(radius * s, half * sign, radius * c));
            g.normals.push(p(0.0, sign, 0.0));
            g.uvs.push(t(c * 0.5 + 0.5, s * 0.5 * sign + 0.5));
        }
        let start = idx.len() as u32;
        for x in 0..rs {
            let c = center_start + x;
            let i = center_end + x;
            if top { idx.extend_from_slice(&[i, i + 1, c]); } else { idx.extend_from_slice(&[i + 1, i, c]); }
        }
        g.add_group(start, idx.len() as u32 - start, if top { 1 } else { 2 });
    };
    if !open_ended {
        if rt > 0.0 { cap(&mut g, true); }
        if rb > 0.0 { cap(&mut g, false); }
    }
    g.indices = Some(idx);
    g
}

// ---- Cone ----
pub fn cone(radius: f32, height: f32, radial_segments: u32) -> Geometry { cylinder_full(0.0, radius, height, radial_segments, 1, false, 0.0, TAU32) }
pub fn cone_full(radius: f32, height: f32, radial_segments: u32, height_segments: u32, open_ended: bool, theta_start: f32, theta_length: f32) -> Geometry {
    cylinder_full(0.0, radius, height, radial_segments, height_segments, open_ended, theta_start, theta_length)
}

// ---- Circle ----
pub fn circle(radius: f32, segments: u32) -> Geometry { circle_full(radius, segments, 0.0, TAU32) }
pub fn circle_full(radius: f32, segments: u32, theta_start: f32, theta_length: f32) -> Geometry {
    let (r, ts, tl) = (radius as f64, theta_start as f64, theta_length as f64);
    let seg = segments.max(3);
    let mut g = Geometry::default();
    g.positions.push([0.0; 3]);
    g.normals.push([0.0, 0.0, 1.0]);
    g.uvs.push([0.5, 0.5]);
    for s in 0..=seg {
        let a = ts + s as f64 / seg as f64 * tl;
        let (x, y) = (r * a.cos(), r * a.sin());
        g.positions.push(p(x, y, 0.0));
        g.normals.push([0.0, 0.0, 1.0]);
        g.uvs.push(t((x / r + 1.0) / 2.0, (y / r + 1.0) / 2.0));
    }
    let mut idx = Vec::new();
    for i in 1..=seg { idx.extend_from_slice(&[i, i + 1, 0]); }
    g.indices = Some(idx);
    g
}

// ---- Ring ----
pub fn ring(inner_radius: f32, outer_radius: f32, theta_segments: u32) -> Geometry { ring_full(inner_radius, outer_radius, theta_segments, 1, 0.0, TAU32) }
pub fn ring_full(inner_radius: f32, outer_radius: f32, theta_segments: u32, phi_segments: u32, theta_start: f32, theta_length: f32) -> Geometry {
    let (ir, or, ts, tl) = (inner_radius as f64, outer_radius as f64, theta_start as f64, theta_length as f64);
    let (ths, phs) = (theta_segments.max(3), phi_segments.max(1));
    let mut radius = ir;
    let step = (or - ir) / phs as f64;
    let mut g = Geometry::default();
    for _ in 0..=phs {
        for i in 0..=ths {
            let a = ts + i as f64 / ths as f64 * tl;
            let (x, y) = (radius * a.cos(), radius * a.sin());
            g.positions.push(p(x, y, 0.0));
            g.normals.push([0.0, 0.0, 1.0]);
            g.uvs.push(t((x / or + 1.0) / 2.0, (y / or + 1.0) / 2.0));
        }
        radius += step;
    }
    let mut idx = Vec::new();
    for j in 0..phs {
        let lvl = j * (ths + 1);
        for i in 0..ths {
            let s = i + lvl;
            let (a, b, c, d) = (s, s + ths + 1, s + ths + 2, s + 1);
            idx.extend_from_slice(&[a, b, d, b, c, d]);
        }
    }
    g.indices = Some(idx);
    g
}

// ---- Torus ----
pub fn torus(radius: f32, tube: f32, radial_segments: u32, tubular_segments: u32) -> Geometry { torus_full(radius, tube, radial_segments, tubular_segments, TAU32) }
pub fn torus_full(radius: f32, tube: f32, radial_segments: u32, tubular_segments: u32, arc: f32) -> Geometry {
    let (r, tb, arc) = (radius as f64, tube as f64, arc as f64);
    let (rs, ts) = (radial_segments, tubular_segments);
    let mut g = Geometry::default();
    for j in 0..=rs {
        for i in 0..=ts {
            let u = i as f64 / ts as f64 * arc;
            let v = j as f64 / rs as f64 * TAU;
            let vtx = DVec3::new((r + tb * v.cos()) * u.cos(), (r + tb * v.cos()) * u.sin(), tb * v.sin());
            g.positions.push(p3(vtx));
            let center = DVec3::new(r * u.cos(), r * u.sin(), 0.0);
            g.normals.push(p3(norm(vtx - center)));
            g.uvs.push(t(i as f64 / ts as f64, j as f64 / rs as f64));
        }
    }
    let mut idx = Vec::new();
    for j in 1..=rs {
        for i in 1..=ts {
            let a = (ts + 1) * j + i - 1;
            let b = (ts + 1) * (j - 1) + i - 1;
            let c = (ts + 1) * (j - 1) + i;
            let d = (ts + 1) * j + i;
            idx.extend_from_slice(&[a, b, d, b, c, d]);
        }
    }
    g.indices = Some(idx);
    g
}

// ---- Lathe（three と同じく、終わりの点の法線は長さ 1 に直さない） ----
pub fn lathe(points: &[Vec2], segments: u32) -> Geometry { lathe_full(points, segments, 0.0, TAU32) }
pub fn lathe_full(points: &[Vec2], segments: u32, phi_start: f32, phi_length: f32) -> Geometry {
    let pts: Vec<DVec2> = points.iter().map(|v| v.as_dvec2()).collect();
    let ps = phi_start as f64;
    let pl = (phi_length as f64).clamp(0.0, TAU);
    let inv = 1.0 / segments as f64;
    let n = pts.len();
    let mut init: Vec<DVec3> = Vec::with_capacity(n);
    let mut prev = DVec3::ZERO;
    for j in 0..n {
        if j == 0 {
            let (dx, dy) = (pts[1].x - pts[0].x, pts[1].y - pts[0].y);
            let v = DVec3::new(dy, -dx, dy * 0.0);
            prev = v;
            init.push(norm(v));
        } else if j == n - 1 {
            init.push(prev);
        } else {
            let (dx, dy) = (pts[j + 1].x - pts[j].x, pts[j + 1].y - pts[j].y);
            let cur = DVec3::new(dy, -dx, dy * 0.0);
            init.push(norm(cur + prev));
            prev = cur;
        }
    }
    let mut g = Geometry::default();
    for i in 0..=segments {
        let phi = ps + i as f64 * inv * pl;
        let (s, c) = (phi.sin(), phi.cos());
        for j in 0..n {
            g.positions.push(p(pts[j].x * s, pts[j].y, pts[j].x * c));
            g.uvs.push(t(i as f64 / segments as f64, j as f64 / (n - 1) as f64));
            g.normals.push(p(init[j].x * s, init[j].y, init[j].x * c));
        }
    }
    let mut idx = Vec::new();
    let m = n as u32;
    for i in 0..segments {
        for j in 0..m - 1 {
            let base = j + i * m;
            let (a, b, c, d) = (base, base + m, base + m + 1, base + 1);
            idx.extend_from_slice(&[a, b, d, c, d, b]);
        }
    }
    g.indices = Some(idx);
    g
}

// ---- Capsule ----
pub fn capsule(radius: f32, height: f32, cap_segments: u32, radial_segments: u32) -> Geometry { capsule_full(radius, height, cap_segments, radial_segments, 1) }
pub fn capsule_full(radius: f32, height: f32, cap_segments: u32, radial_segments: u32, height_segments: u32) -> Geometry {
    let r = radius as f64;
    let h = (height as f64).max(0.0);
    let (cs, rs, hs) = (cap_segments.max(1), radial_segments.max(3), height_segments.max(1));
    let half = h / 2.0;
    let cap_arc = PI / 2.0 * r;
    let total = 2.0 * cap_arc + h;
    let num_v = cs * 2 + hs;
    let row = rs + 1;
    let mut g = Geometry::default();
    let mut idx = Vec::new();
    for iy in 0..=num_v {
        let (arc, py, pr, ny);
        if iy <= cs {
            let sp = iy as f64 / cs as f64;
            let a = sp * PI / 2.0;
            py = -half - r * a.cos();
            pr = r * a.sin();
            ny = -r * a.cos();
            arc = sp * cap_arc;
        } else if iy <= cs + hs {
            let sp = (iy - cs) as f64 / hs as f64;
            py = -half + sp * h;
            pr = r;
            ny = 0.0;
            arc = cap_arc + sp * h;
        } else {
            let sp = (iy - cs - hs) as f64 / cs as f64;
            let a = sp * PI / 2.0;
            py = half + r * a.sin();
            pr = r * a.cos();
            ny = r * a.sin();
            arc = cap_arc + h + sp * cap_arc;
        }
        let v = (arc / total).clamp(0.0, 1.0);
        let u_off = if iy == 0 { 0.5 / rs as f64 } else if iy == num_v { -0.5 / rs as f64 } else { 0.0 };
        for ix in 0..=rs {
            let u = ix as f64 / rs as f64;
            let th = u * TAU;
            let (s, c) = (th.sin(), th.cos());
            g.positions.push(p(-pr * c, py, pr * s));
            g.normals.push(p3(norm(DVec3::new(-pr * c, ny, pr * s))));
            g.uvs.push(t(u + u_off, v));
        }
        if iy > 0 {
            let pv = (iy - 1) * row;
            for ix in 0..rs {
                let (i1, i2, i3, i4) = (pv + ix, pv + ix + 1, iy * row + ix, iy * row + ix + 1);
                idx.extend_from_slice(&[i1, i2, i3, i2, i4, i3]);
            }
        }
    }
    g.indices = Some(idx);
    g
}

// ---- Polyhedron（index なし。detail 0 は面の法線、1 以上は滑らか） ----
pub fn polyhedron(vertices: &[f64], indices: &[u32], radius: f32, detail: u32) -> Geometry {
    let r = radius as f64;
    let mut vb: Vec<DVec3> = Vec::new();
    let at = |i: u32| DVec3::new(vertices[i as usize * 3], vertices[i as usize * 3 + 1], vertices[i as usize * 3 + 2]);
    // Vector3.lerp と同じ式
    let lerp = |a: DVec3, b: DVec3, k: f64| a + (b - a) * k;
    for f in indices.chunks_exact(3) {
        let (a, b, c) = (at(f[0]), at(f[1]), at(f[2]));
        let cols = detail as usize + 1;
        let mut v: Vec<Vec<DVec3>> = Vec::with_capacity(cols + 1);
        for i in 0..=cols {
            let aj = lerp(a, c, i as f64 / cols as f64);
            let bj = lerp(b, c, i as f64 / cols as f64);
            let rows = cols - i;
            let mut line = Vec::with_capacity(rows + 1);
            for j in 0..=rows {
                if j == 0 && i == cols { line.push(aj); } else { line.push(lerp(aj, bj, j as f64 / rows as f64)); }
            }
            v.push(line);
        }
        for i in 0..cols {
            for j in 0..2 * (cols - i) - 1 {
                let k = j / 2;
                if j % 2 == 0 {
                    vb.push(v[i][k + 1]);
                    vb.push(v[i + 1][k]);
                    vb.push(v[i][k]);
                } else {
                    vb.push(v[i][k + 1]);
                    vb.push(v[i + 1][k + 1]);
                    vb.push(v[i + 1][k]);
                }
            }
        }
    }
    for q in &mut vb { *q = norm(*q) * r; }
    let azimuth = |v: DVec3| v.z.atan2(-v.x);
    let inclination = |v: DVec3| (-v.y).atan2((v.x * v.x + v.z * v.z).sqrt());
    let mut uv: Vec<DVec2> = vb.iter().map(|&q| DVec2::new(azimuth(q) / 2.0 / PI + 0.5, 1.0 - (inclination(q) / PI + 0.5))).collect();
    // correctUVs
    let mut i = 0;
    while i + 2 < vb.len() {
        let centroid = (vb[i] + vb[i + 1] + vb[i + 2]) * (1.0 / 3.0);
        let azi = azimuth(centroid);
        for k in i..i + 3 {
            let was = uv[k].x;
            if azi < 0.0 && was == 1.0 { uv[k].x = was - 1.0; }
            if vb[k].x == 0.0 && vb[k].z == 0.0 { uv[k].x = azi / 2.0 / PI + 0.5; }
        }
        i += 3;
    }
    // correctSeam
    for f in uv.chunks_exact_mut(3) {
        let (x0, x1, x2) = (f[0].x, f[1].x, f[2].x);
        let (max, min) = (x0.max(x1).max(x2), x0.min(x1).min(x2));
        if max > 0.9 && min < 0.1 {
            if x0 < 0.2 { f[0].x += 1.0; }
            if x1 < 0.2 { f[1].x += 1.0; }
            if x2 < 0.2 { f[2].x += 1.0; }
        }
    }
    let mut g = Geometry {
        positions: vb.iter().map(|&q| p3(q)).collect(),
        normals: vb.iter().map(|&q| p3(q)).collect(),
        uvs: uv.iter().map(|q| t(q.x, q.y)).collect(),
        ..Default::default()
    };
    if detail == 0 { g.compute_vertex_normals(); } else { g.normalize_normals(); }
    g
}

pub fn icosahedron(radius: f32, detail: u32) -> Geometry {
    let t = (1.0 + 5f64.sqrt()) / 2.0;
    let v = [-1.0, t, 0.0, 1.0, t, 0.0, -1.0, -t, 0.0, 1.0, -t, 0.0, 0.0, -1.0, t, 0.0, 1.0, t, 0.0, -1.0, -t, 0.0, 1.0, -t, t, 0.0, -1.0, t, 0.0, 1.0, -t, 0.0, -1.0, -t, 0.0, 1.0];
    let i = [0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7, 1, 8, 3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9, 8, 1];
    polyhedron(&v, &i, radius, detail)
}
pub fn octahedron(radius: f32, detail: u32) -> Geometry {
    let v = [1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0];
    let i = [0, 2, 4, 0, 4, 3, 0, 3, 5, 0, 5, 2, 1, 2, 5, 1, 5, 3, 1, 3, 4, 1, 4, 2];
    polyhedron(&v, &i, radius, detail)
}
pub fn tetrahedron(radius: f32, detail: u32) -> Geometry {
    let v = [1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0];
    let i = [2, 1, 0, 0, 3, 2, 1, 3, 0, 2, 3, 1];
    polyhedron(&v, &i, radius, detail)
}
pub fn dodecahedron(radius: f32, detail: u32) -> Geometry {
    let t = (1.0 + 5f64.sqrt()) / 2.0;
    let r = 1.0 / t;
    let v = [
        -1.0, -1.0, -1.0, -1.0, -1.0, 1.0, -1.0, 1.0, -1.0, -1.0, 1.0, 1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, 1.0,
        0.0, -r, -t, 0.0, -r, t, 0.0, r, -t, 0.0, r, t,
        -r, -t, 0.0, -r, t, 0.0, r, -t, 0.0, r, t, 0.0,
        -t, 0.0, -r, t, 0.0, -r, -t, 0.0, r, t, 0.0, r,
    ];
    let i = [
        3, 11, 7, 3, 7, 15, 3, 15, 13, 7, 19, 17, 7, 17, 6, 7, 6, 15, 17, 4, 8, 17, 8, 10, 17, 10, 6, 8, 0, 16, 8, 16, 2, 8, 2, 10,
        0, 12, 1, 0, 1, 18, 0, 18, 16, 6, 10, 2, 6, 2, 13, 6, 13, 15, 2, 16, 18, 2, 18, 3, 2, 3, 13, 18, 1, 9, 18, 9, 11, 18, 11, 3,
        4, 14, 12, 4, 12, 0, 4, 0, 8, 11, 9, 5, 11, 5, 19, 11, 19, 7, 19, 5, 14, 19, 14, 4, 19, 4, 17, 1, 12, 14, 1, 14, 5, 1, 5, 9,
    ];
    polyhedron(&v, &i, radius, detail)
}

// ---- RoundedBox（examples/jsm/geometries/RoundedBoxGeometry.js） ----
pub fn rounded_box(width: f32, height: f32, depth: f32, segments: u32, radius: f32) -> Geometry {
    let segs = segments * 2 + 1;
    let radius = (width / 2.0).min(height / 2.0).min(depth / 2.0).min(radius) as f64;
    let boxed = box_seg(width, height, depth, segs, segs, segs);
    if segs == 1 { return boxed; }
    let mut g = boxed.to_non_indexed();
    let (w, h, d) = (width as f64, height as f64, depth as f64);
    let half_box = DVec3::new(w, h, d) * 0.5 - DVec3::splat(radius);
    let total = g.positions.len() * 3;
    let face_tris = total / 6;
    let half_seg = 0.5 / segs as f64;
    // 軸: 0 x・1 y・2 z
    let get_uv = |face: DVec3, normal: DVec3, uv_axis: usize, proj_axis: usize, side_length: f64| -> f64 {
        let tot_arc = 2.0 * PI * radius / 4.0;
        let center_len = (side_length - 2.0 * radius).max(0.0);
        let half_arc = PI / 4.0;
        let mut tn = normal;
        tn[proj_axis] = 0.0;
        let tn = norm(tn);
        let arc_uv = 0.5 * tot_arc / (tot_arc + center_len);
        // Vector3.angleTo
        let denom = (tn.length_squared() * face.length_squared()).sqrt();
        let angle = if denom == 0.0 { PI / 2.0 } else { (tn.dot(face) / denom).clamp(-1.0, 1.0).acos() };
        let arc_angle = 1.0 - angle / half_arc;
        if js_sign(tn[uv_axis]) == 1.0 {
            arc_angle * arc_uv
        } else {
            let len_uv = center_len / (tot_arc + center_len);
            len_uv + arc_uv + arc_uv * (1.0 - arc_angle)
        }
    };
    for k in 0..g.positions.len() {
        let i = k * 3;
        let q = g.positions[k];
        let pos = DVec3::new(q[0] as f64, q[1] as f64, q[2] as f64);
        let n = norm(DVec3::new(
            pos.x - js_sign(pos.x) * half_seg,
            pos.y - js_sign(pos.y) * half_seg,
            pos.z - js_sign(pos.z) * half_seg,
        ));
        g.positions[k] = p(
            half_box.x * js_sign(pos.x) + n.x * radius,
            half_box.y * js_sign(pos.y) + n.y * radius,
            half_box.z * js_sign(pos.z) + n.z * radius,
        );
        g.normals[k] = p3(n);
        const X: usize = 0;
        const Y: usize = 1;
        const Z: usize = 2;
        let uv = match i / face_tris {
            0 => { let f = DVec3::X; (get_uv(f, n, Z, Y, d), 1.0 - get_uv(f, n, Y, Z, h)) }
            1 => { let f = DVec3::NEG_X; (1.0 - get_uv(f, n, Z, Y, d), 1.0 - get_uv(f, n, Y, Z, h)) }
            2 => { let f = DVec3::Y; (1.0 - get_uv(f, n, X, Z, w), get_uv(f, n, Z, X, d)) }
            3 => { let f = DVec3::NEG_Y; (1.0 - get_uv(f, n, X, Z, w), 1.0 - get_uv(f, n, Z, X, d)) }
            4 => { let f = DVec3::Z; (1.0 - get_uv(f, n, X, Y, w), 1.0 - get_uv(f, n, Y, X, h)) }
            _ => { let f = DVec3::NEG_Z; (get_uv(f, n, X, Y, w), 1.0 - get_uv(f, n, Y, X, h)) }
        };
        g.uvs[k] = t(uv.0, uv.1);
    }
    g
}
