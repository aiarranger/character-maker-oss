// ExtrudeGeometry（three の geometries/ExtrudeGeometry.js）。groups は 0 ふた・1 側面。index は無い。
// extrudePath は 7 本のどれも使わないので持たない。
use super::shape::{Shape, is_clock_wise, triangulate_shape};
use super::{Geometry, js_sign};
use glam::DVec2;
use std::f64::consts::PI;

/// three の options と同じ既定。`ExtrudeOpts { depth: 0.2, bevel_size: Some(0.1), ..Default::default() }`
#[derive(Clone, Copy, Debug)]
pub struct ExtrudeOpts {
    pub curve_segments: u32,
    pub steps: u32,
    pub depth: f32,
    pub bevel_enabled: bool,
    pub bevel_thickness: f32,
    /// None なら bevel_thickness - 0.1（three の既定）
    pub bevel_size: Option<f32>,
    pub bevel_offset: f32,
    pub bevel_segments: u32,
}
impl Default for ExtrudeOpts {
    fn default() -> Self {
        ExtrudeOpts { curve_segments: 12, steps: 1, depth: 1.0, bevel_enabled: true, bevel_thickness: 0.2, bevel_size: None, bevel_offset: 0.0, bevel_segments: 3 }
    }
}

fn merge_overlapping(points: &mut Vec<DVec2>) {
    if points.is_empty() { return; }
    const THRESHOLD_SQ: f64 = 1e-10 * 1e-10;
    let mut prev = points[0];
    let mut i = 1;
    while i <= points.len() {
        let cur_i = i % points.len();
        let cur = points[cur_i];
        let (dx, dy) = (cur.x - prev.x, cur.y - prev.y);
        let dist_sq = dx * dx + dy * dy;
        let s = cur.x.abs().max(cur.y.abs()).max(prev.x.abs()).max(prev.y.abs());
        if dist_sq <= THRESHOLD_SQ * s * s {
            points.remove(cur_i);
            if points.is_empty() { return; }
            continue;
        }
        prev = cur;
        i += 1;
    }
}

fn bevel_vec(pt: DVec2, prev: DVec2, next: DVec2) -> DVec2 {
    let (vpx, vpy) = (pt.x - prev.x, pt.y - prev.y);
    let (vnx, vny) = (next.x - pt.x, next.y - pt.y);
    let vp_lensq = vpx * vpx + vpy * vpy;
    let collinear = vpx * vny - vpy * vnx;
    let (tx, ty, shrink);
    if collinear.abs() > f64::EPSILON {
        let vp_len = vp_lensq.sqrt();
        let vn_len = (vnx * vnx + vny * vny).sqrt();
        let (ppx, ppy) = (prev.x - vpy / vp_len, prev.y + vpx / vp_len);
        let (pnx, pny) = (next.x - vny / vn_len, next.y + vnx / vn_len);
        let sf = ((pnx - ppx) * vny - (pny - ppy) * vnx) / (vpx * vny - vpy * vnx);
        tx = ppx + vpx * sf - pt.x;
        ty = ppy + vpy * sf - pt.y;
        let lensq = tx * tx + ty * ty;
        if lensq <= 2.0 { return DVec2::new(tx, ty); }
        shrink = (lensq / 2.0).sqrt();
    } else {
        let mut same = false;
        if vpx > f64::EPSILON {
            if vnx > f64::EPSILON { same = true; }
        } else if vpx < -f64::EPSILON {
            if vnx < -f64::EPSILON { same = true; }
        } else if js_sign(vpy) == js_sign(vny) {
            same = true;
        }
        if same {
            tx = -vpy;
            ty = vpx;
            shrink = vp_lensq.sqrt();
        } else {
            tx = vpx;
            ty = vpy;
            shrink = (vp_lensq / 2.0).sqrt();
        }
    }
    DVec2::new(tx / shrink, ty / shrink)
}

fn movements(c: &[DVec2]) -> Vec<DVec2> {
    let n = c.len();
    (0..n).map(|i| bevel_vec(c[i], c[(i + n - 1) % n], c[(i + 1) % n])).collect()
}

struct Out { verts: Vec<f64>, uvs: Vec<f64> }

fn add_shape(g: &mut Geometry, out: &mut Out, shape: &Shape, o: &ExtrudeOpts) {
    let steps = o.steps as usize;
    let depth = o.depth as f64;
    let bevel_enabled = o.bevel_enabled;
    let (mut bevel_thickness, mut bevel_size, mut bevel_offset, mut bevel_segments) = (
        o.bevel_thickness as f64,
        o.bevel_size.map_or(o.bevel_thickness as f64 - 0.1, |v| v as f64),
        o.bevel_offset as f64,
        o.bevel_segments as usize,
    );
    if !bevel_enabled {
        bevel_segments = 0;
        bevel_thickness = 0.0;
        bevel_size = 0.0;
        bevel_offset = 0.0;
    }
    let (mut contour, mut holes) = shape.extract_points(o.curve_segments);
    if !is_clock_wise(&contour) {
        contour.reverse();
        for h in &mut holes { if is_clock_wise(h) { h.reverse(); } }
    }
    merge_overlapping(&mut contour);
    holes.iter_mut().for_each(merge_overlapping);

    let mut vertices = contour.clone();
    for h in &holes { vertices.extend_from_slice(h); }
    let vlen = vertices.len();

    let contour_mv = movements(&contour);
    let holes_mv: Vec<Vec<DVec2>> = holes.iter().map(|h| movements(h)).collect();
    let mut verts_mv = contour_mv.clone();
    for m in &holes_mv { verts_mv.extend_from_slice(m); }

    let mut ph: Vec<f64> = Vec::new();
    let v = |ph: &mut Vec<f64>, x: f64, y: f64, z: f64| { ph.push(x); ph.push(y); ph.push(z); };

    let faces = if bevel_segments == 0 {
        triangulate_shape(&mut contour, &mut holes)
    } else {
        let mut contracted = Vec::new();
        let mut expanded = Vec::new();
        for b in 0..bevel_segments {
            let t = b as f64 / bevel_segments as f64;
            let z = bevel_thickness * (t * PI / 2.0).cos();
            let bs = bevel_size * (t * PI / 2.0).sin() + bevel_offset;
            for (i, c) in contour.iter().enumerate() {
                let q = *c + contour_mv[i] * bs;
                v(&mut ph, q.x, q.y, -z);
                if t == 0.0 { contracted.push(q); }
            }
            for (h, hole) in holes.iter().enumerate() {
                let mut one = Vec::new();
                for (i, c) in hole.iter().enumerate() {
                    let q = *c + holes_mv[h][i] * bs;
                    v(&mut ph, q.x, q.y, -z);
                    if t == 0.0 { one.push(q); }
                }
                if t == 0.0 { expanded.push(one); }
            }
        }
        triangulate_shape(&mut contracted, &mut expanded)
    };

    let bs = bevel_size + bevel_offset;
    let at = |i: usize| if bevel_enabled { vertices[i] + verts_mv[i] * bs } else { vertices[i] };
    for i in 0..vlen { let q = at(i); v(&mut ph, q.x, q.y, 0.0); }
    for s in 1..=steps {
        for i in 0..vlen { let q = at(i); v(&mut ph, q.x, q.y, depth / steps as f64 * s as f64); }
    }
    for b in (0..bevel_segments).rev() {
        let t = b as f64 / bevel_segments as f64;
        let z = bevel_thickness * (t * PI / 2.0).cos();
        let bs = bevel_size * (t * PI / 2.0).sin() + bevel_offset;
        for (i, c) in contour.iter().enumerate() {
            let q = *c + contour_mv[i] * bs;
            v(&mut ph, q.x, q.y, depth + z);
        }
        for (h, hole) in holes.iter().enumerate() {
            for (i, c) in hole.iter().enumerate() {
                let q = *c + holes_mv[h][i] * bs;
                v(&mut ph, q.x, q.y, depth + z);
            }
        }
    }

    let add_vertex = |out: &mut Out, i: usize| out.verts.extend_from_slice(&ph[i * 3..i * 3 + 3]);
    let f3 = |out: &mut Out, a: usize, b: usize, c: usize| {
        add_vertex(out, a);
        add_vertex(out, b);
        add_vertex(out, c);
        let n = out.verts.len() / 3;
        for k in n - 3..n {
            let (x, y) = (out.verts[k * 3], out.verts[k * 3 + 1]);
            out.uvs.push(x);
            out.uvs.push(y);
        }
    };
    let f4 = |out: &mut Out, a: usize, b: usize, c: usize, d: usize| {
        for i in [a, b, d, b, c, d] { add_vertex(out, i); }
        let n = out.verts.len() / 3;
        // generateSideWallUV(n-6, n-3, n-2, n-1) → a, b, c, d
        let q: Vec<[f64; 3]> = [n - 6, n - 3, n - 2, n - 1].iter().map(|&k| [out.verts[k * 3], out.verts[k * 3 + 1], out.verts[k * 3 + 2]]).collect();
        let uv: Vec<[f64; 2]> = if (q[0][1] - q[1][1]).abs() < (q[0][0] - q[1][0]).abs() {
            q.iter().map(|p| [p[0], 1.0 - p[2]]).collect()
        } else {
            q.iter().map(|p| [p[1], 1.0 - p[2]]).collect()
        };
        for i in [0, 1, 3, 1, 2, 3] { out.uvs.extend_from_slice(&uv[i]); }
    };

    // ふた
    let start = out.verts.len() / 3;
    if bevel_enabled {
        for f in &faces { f3(out, f[2], f[1], f[0]); }
        let offset = vlen * (steps + bevel_segments * 2);
        for f in &faces { f3(out, f[0] + offset, f[1] + offset, f[2] + offset); }
    } else {
        for f in &faces { f3(out, f[2], f[1], f[0]); }
        for f in &faces { f3(out, f[0] + vlen * steps, f[1] + vlen * steps, f[2] + vlen * steps); }
    }
    g.add_group(start as u32, (out.verts.len() / 3 - start) as u32, 0);

    // 側面
    let start = out.verts.len() / 3;
    let mut sidewalls = |c: &[DVec2], layer: usize| {
        for i in (0..c.len()).rev() {
            let j = i;
            let k = if i == 0 { c.len() - 1 } else { i - 1 };
            for s in 0..steps + bevel_segments * 2 {
                let (s1, s2) = (vlen * s, vlen * (s + 1));
                f4(out, layer + j + s1, layer + k + s1, layer + k + s2, layer + j + s2);
            }
        }
    };
    let mut layer = 0;
    sidewalls(&contour, layer);
    layer += contour.len();
    for h in &holes {
        sidewalls(h, layer);
        layer += h.len();
    }
    g.add_group(start as u32, (out.verts.len() / 3 - start) as u32, 1);
}

/// new THREE.ExtrudeGeometry(shape, options)
pub fn extrude(shape: &Shape, opts: &ExtrudeOpts) -> Geometry { extrude_many(std::slice::from_ref(shape), opts) }

/// new THREE.ExtrudeGeometry([shapes], options)
pub fn extrude_many(shapes: &[Shape], opts: &ExtrudeOpts) -> Geometry {
    let mut g = Geometry::default();
    let mut out = Out { verts: Vec::new(), uvs: Vec::new() };
    for s in shapes { add_shape(&mut g, &mut out, s, opts); }
    g.positions = out.verts.chunks_exact(3).map(|v| [v[0] as f32, v[1] as f32, v[2] as f32]).collect();
    g.uvs = out.uvs.chunks_exact(2).map(|v| [v[0] as f32, v[1] as f32]).collect();
    g.compute_vertex_normals();
    g
}
