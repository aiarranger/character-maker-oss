// three の基本形（PlaneGeometry など）と同じ頂点・UV を作る。シェーダーが uv を使うので並びを合わせる。
pub use character_actor::Vertex;
use glam::{Mat4, Vec3};
use std::f32::consts::PI;

#[derive(Default, Clone)]
pub struct Geo {
    pub v: Vec<Vertex>,
    pub i: Vec<u32>,
}

fn vtx(p: [f32; 3], n: [f32; 3], uv: [f32; 2]) -> Vertex {
    Vertex { pos: p, normal: n, uv, color: [1.0; 4], sway: 0.0 }
}

impl Geo {
    pub fn transform(mut self, m: Mat4) -> Self {
        let nm = m.inverse().transpose();
        for v in &mut self.v {
            v.pos = m.transform_point3(Vec3::from(v.pos)).to_array();
            v.normal = nm.transform_vector3(Vec3::from(v.normal)).normalize_or_zero().to_array();
        }
        self
    }
    pub fn translate(self, x: f32, y: f32, z: f32) -> Self { self.transform(Mat4::from_translation(Vec3::new(x, y, z))) }
    pub fn rotate_x(self, a: f32) -> Self { self.transform(Mat4::from_rotation_x(a)) }
    pub fn append(&mut self, o: &Geo) {
        let base = self.v.len() as u32;
        self.v.extend_from_slice(&o.v);
        self.i.extend(o.i.iter().map(|i| i + base));
    }
}

pub fn plane(w: f32, h: f32, wseg: u32, hseg: u32) -> Geo {
    let mut g = Geo::default();
    for iy in 0..=hseg {
        let y = iy as f32 * h / hseg as f32 - h / 2.0;
        for ix in 0..=wseg {
            let x = ix as f32 * w / wseg as f32 - w / 2.0;
            g.v.push(vtx([x, -y, 0.0], [0.0, 0.0, 1.0], [ix as f32 / wseg as f32, 1.0 - iy as f32 / hseg as f32]));
        }
    }
    for iy in 0..hseg {
        for ix in 0..wseg {
            let (a, b, c, d) = (ix + (wseg + 1) * iy, ix + (wseg + 1) * (iy + 1), ix + 1 + (wseg + 1) * (iy + 1), ix + 1 + (wseg + 1) * iy);
            g.i.extend_from_slice(&[a, b, d, b, c, d]);
        }
    }
    g
}

pub fn cylinder(rt: f32, rb: f32, h: f32, rad: u32, open: bool) -> Geo {
    let mut g = Geo::default();
    let slope = (rb - rt) / h;
    for y in 0..=1u32 {
        let v = y as f32;
        let r = v * (rb - rt) + rt;
        for x in 0..=rad {
            let u = x as f32 / rad as f32;
            let t = u * PI * 2.0;
            let (s, c) = t.sin_cos();
            let n = Vec3::new(s, slope, c).normalize();
            g.v.push(vtx([r * s, -v * h + h / 2.0, r * c], n.to_array(), [u, 1.0 - v]));
        }
    }
    for x in 0..rad {
        let (a, b, c, d) = (x, x + rad + 1, x + rad + 2, x + 1);
        g.i.extend_from_slice(&[a, b, d, b, c, d]);
    }
    if !open {
        for (top, r, sign) in [(true, rt, 1.0f32), (false, rb, -1.0f32)] {
            if r <= 0.0 { continue; }
            let center = g.v.len() as u32;
            g.v.push(vtx([0.0, sign * h / 2.0, 0.0], [0.0, sign, 0.0], [0.5, 0.5]));
            let start = g.v.len() as u32;
            for x in 0..=rad {
                let t = x as f32 / rad as f32 * PI * 2.0;
                let (s, c) = t.sin_cos();
                g.v.push(vtx([r * s, sign * h / 2.0, r * c], [0.0, sign, 0.0], [c * 0.5 + 0.5, s * 0.5 * sign + 0.5]));
            }
            for x in 0..rad {
                if top { g.i.extend_from_slice(&[start + x, start + x + 1, center]); } else { g.i.extend_from_slice(&[start + x + 1, start + x, center]); }
            }
        }
    }
    g
}

pub fn cone(r: f32, h: f32, rad: u32) -> Geo { cylinder(0.0, r, h, rad, false) }

pub fn sphere(r: f32, ws: u32, hs: u32) -> Geo {
    let mut g = Geo::default();
    for iy in 0..=hs {
        let v = iy as f32 / hs as f32;
        for ix in 0..=ws {
            let u = ix as f32 / ws as f32;
            let p = Vec3::new(-(u * PI * 2.0).cos() * (v * PI).sin(), (v * PI).cos(), (u * PI * 2.0).sin() * (v * PI).sin());
            g.v.push(vtx((p * r).to_array(), p.to_array(), [u, 1.0 - v]));
        }
    }
    let row = ws + 1;
    for iy in 0..hs {
        for ix in 0..ws {
            let (a, b, c, d) = (row * iy + ix + 1, row * iy + ix, row * (iy + 1) + ix, row * (iy + 1) + ix + 1);
            if iy != 0 { g.i.extend_from_slice(&[a, b, d]); }
            if iy != hs - 1 { g.i.extend_from_slice(&[b, c, d]); }
        }
    }
    g
}

pub fn boxg(w: f32, h: f32, d: f32) -> Geo {
    let mut g = Geo::default();
    let (x, y, z) = (w / 2.0, h / 2.0, d / 2.0);
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    ];
    for (n, u, v) in faces {
        let (nv, uv, vv) = (Vec3::from(n), Vec3::from(u), Vec3::from(v));
        let s = Vec3::new(x, y, z);
        let base = g.v.len() as u32;
        for (a, b) in [(-1.0f32, 1.0f32), (-1.0, -1.0), (1.0, -1.0), (1.0, 1.0)] {
            let p = (nv + uv * a + vv * b) * s;
            g.v.push(vtx(p.to_array(), n, [(a + 1.0) / 2.0, (b + 1.0) / 2.0]));
        }
        g.i.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    g
}

/// 床に寝かせる前の輪（XY 面。uv は輪の内外を問わず x, y の 0..1）
pub fn ring(inner: f32, outer: f32, seg: u32) -> Geo {
    let mut g = Geo::default();
    for j in 0..=1u32 {
        let r = if j == 0 { inner } else { outer };
        for i in 0..=seg {
            let a = i as f32 / seg as f32 * PI * 2.0;
            let (x, y) = (r * a.cos(), r * a.sin());
            g.v.push(vtx([x, y, 0.0], [0.0, 0.0, 1.0], [(x / outer + 1.0) / 2.0, (y / outer + 1.0) / 2.0]));
        }
    }
    for i in 0..seg {
        let (a, b, c, d) = (i, i + seg + 1, i + seg + 2, i + 1);
        g.i.extend_from_slice(&[a, b, d, b, c, d]);
    }
    g
}

pub fn octahedron(r: f32) -> Geo {
    let p: [[f32; 3]; 6] = [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];
    let faces = [[0, 2, 4], [0, 4, 3], [0, 3, 5], [0, 5, 2], [1, 2, 5], [1, 5, 3], [1, 3, 4], [1, 4, 2]];
    let mut g = Geo::default();
    for f in faces {
        let (a, b, c) = (Vec3::from(p[f[0]]), Vec3::from(p[f[1]]), Vec3::from(p[f[2]]));
        let n = (b - a).cross(c - a).normalize().to_array();
        let base = g.v.len() as u32;
        for q in [a, b, c] { g.v.push(vtx((q * r).to_array(), n, [0.0, 0.0])); }
        g.i.extend_from_slice(&[base, base + 1, base + 2]);
    }
    g
}
