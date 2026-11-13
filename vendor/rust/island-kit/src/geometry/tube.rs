// CatmullRomCurve3 と Curve の弧長・Frenet 枠、TubeGeometry。three の extras/curves/CatmullRomCurve3.js・extras/core/Curve.js・geometries/TubeGeometry.js と同じ。
use super::{Geometry, norm, p3};
use glam::{DVec3, Vec3};
use std::cell::OnceCell;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum CurveType { #[default] Centripetal, Chordal, CatmullRom }

/// new THREE.CatmullRomCurve3(points, closed, curveType, tension)。
/// 弧長の表は最初に使うときに作って持つ（three の cacheArcLengths と同じ）
#[derive(Clone, Debug)]
pub struct CatmullRomCurve3 {
    points: Vec<DVec3>,
    pub closed: bool,
    pub curve_type: CurveType,
    pub tension: f64,
    arc: OnceCell<Vec<f64>>,
}

/// computeFrenetFrames の結果
#[derive(Clone, Debug)]
pub struct FrenetFrames { pub tangents: Vec<Vec3>, pub normals: Vec<Vec3>, pub binormals: Vec<Vec3> }

const ARC_DIVISIONS: usize = 200;

/// CubicPoly の init → calc
fn poly(x0: f64, x1: f64, t0: f64, t1: f64, t: f64) -> f64 {
    let (c0, c1, c2, c3) = (x0, t0, -3.0 * x0 + 3.0 * x1 - 2.0 * t0 - t1, 2.0 * x0 - 2.0 * x1 + t0 + t1);
    let (t2, t3) = (t * t, t * t * t);
    c0 + c1 * t + c2 * t2 + c3 * t3
}
fn nonuniform(x0: f64, x1: f64, x2: f64, x3: f64, dt0: f64, dt1: f64, dt2: f64, t: f64) -> f64 {
    let mut t1 = (x1 - x0) / dt0 - (x2 - x0) / (dt0 + dt1) + (x2 - x1) / dt1;
    let mut t2 = (x2 - x1) / dt1 - (x3 - x1) / (dt1 + dt2) + (x3 - x2) / dt2;
    t1 *= dt1;
    t2 *= dt1;
    poly(x1, x2, t1, t2, t)
}

/// Matrix4.makeRotationAxis を掛けた結果
fn rotate_axis(v: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    let (c, s) = (angle.cos(), angle.sin());
    let t = 1.0 - c;
    let (x, y, z) = (axis.x, axis.y, axis.z);
    let (tx, ty) = (t * x, t * y);
    DVec3::new(
        (tx * x + c) * v.x + (tx * y - s * z) * v.y + (tx * z + s * y) * v.z,
        (tx * y + s * z) * v.x + (ty * y + c) * v.y + (ty * z - s * x) * v.z,
        (tx * z - s * y) * v.x + (ty * z + s * x) * v.y + (t * z * z + c) * v.z,
    )
}

impl CatmullRomCurve3 {
    /// new THREE.CatmullRomCurve3(points)（開いた centripetal）
    pub fn new(points: &[Vec3]) -> Self { Self::with(points, false, CurveType::Centripetal, 0.5) }
    pub fn with(points: &[Vec3], closed: bool, curve_type: CurveType, tension: f32) -> Self {
        Self { points: points.iter().map(|p| p.as_dvec3()).collect(), closed, curve_type, tension: tension as f64, arc: OnceCell::new() }
    }

    fn point(&self, t: f64) -> DVec3 {
        let pts = &self.points;
        let l = pts.len() as i64;
        let p = (l - if self.closed { 0 } else { 1 }) as f64 * t;
        let mut int = p.floor() as i64;
        let mut w = p - int as f64;
        if self.closed {
            int += if int > 0 { 0 } else { (int.abs() / l + 1) * l };
        } else if w == 0.0 && int == l - 1 {
            int = l - 2;
            w = 1.0;
        }
        let at = |i: i64| pts[(i % l) as usize];
        let extra0 = !(self.closed || int > 0);
        let extra3 = !(self.closed || int + 2 < l);
        let mut p0 = if extra0 { pts[0] - pts[1] + pts[0] } else { at(int - 1) };
        let p1 = at(int);
        let p2 = at(int + 1);
        let p3 = if extra3 { pts[l as usize - 1] - pts[l as usize - 2] + pts[l as usize - 1] } else { at(int + 2) };
        // three は両方の外挿で同じ tmp を使うので、点が 2 つのときは p0 も p3 と同じ値になる
        if extra0 && extra3 { p0 = p3; }
        match self.curve_type {
            CurveType::Centripetal | CurveType::Chordal => {
                let pow = if self.curve_type == CurveType::Chordal { 0.5 } else { 0.25 };
                let mut dt0 = p0.distance_squared(p1).powf(pow);
                let mut dt1 = p1.distance_squared(p2).powf(pow);
                let mut dt2 = p2.distance_squared(p3).powf(pow);
                if dt1 < 1e-4 { dt1 = 1.0; }
                if dt0 < 1e-4 { dt0 = dt1; }
                if dt2 < 1e-4 { dt2 = dt1; }
                DVec3::new(
                    nonuniform(p0.x, p1.x, p2.x, p3.x, dt0, dt1, dt2, w),
                    nonuniform(p0.y, p1.y, p2.y, p3.y, dt0, dt1, dt2, w),
                    nonuniform(p0.z, p1.z, p2.z, p3.z, dt0, dt1, dt2, w),
                )
            }
            CurveType::CatmullRom => {
                let k = self.tension;
                let c = |a: f64, b: f64, c: f64, d: f64| poly(b, c, k * (c - a), k * (d - b), w);
                DVec3::new(c(p0.x, p1.x, p2.x, p3.x), c(p0.y, p1.y, p2.y, p3.y), c(p0.z, p1.z, p2.z, p3.z))
            }
        }
    }

    fn lengths(&self) -> &[f64] {
        self.arc.get_or_init(|| {
            let mut cache = Vec::with_capacity(ARC_DIVISIONS + 1);
            let mut last = self.point(0.0);
            let mut sum = 0.0;
            cache.push(0.0);
            for p in 1..=ARC_DIVISIONS {
                let cur = self.point(p as f64 / ARC_DIVISIONS as f64);
                sum += cur.distance(last);
                cache.push(sum);
                last = cur;
            }
            cache
        })
    }

    /// Curve.getUtoTmapping
    fn u_to_t(&self, u: f64) -> f64 {
        let arc = self.lengths();
        let il = arc.len() as i64;
        let target = u * arc[il as usize - 1];
        let (mut low, mut high) = (0i64, il - 1);
        while low <= high {
            let i = (low as f64 + ((high - low) as f64 / 2.0).floor()) as i64;
            let cmp = arc[i as usize] - target;
            if cmp < 0.0 {
                low = i + 1;
            } else if cmp > 0.0 {
                high = i - 1;
            } else {
                high = i;
                break;
            }
        }
        let i = high;
        if arc[i as usize] == target { return i as f64 / (il - 1) as f64; }
        let before = arc[i as usize];
        let after = arc[i as usize + 1];
        (i as f64 + (target - before) / (after - before)) / (il - 1) as f64
    }

    fn tangent(&self, t: f64) -> DVec3 {
        const DELTA: f64 = 0.0001;
        let t1 = (t - DELTA).max(0.0);
        let t2 = (t + DELTA).min(1.0);
        norm(self.point(t2) - self.point(t1))
    }

    pub fn get_point(&self, t: f32) -> Vec3 { self.point(t as f64).as_vec3() }
    pub fn get_point_at(&self, u: f32) -> Vec3 { self.point(self.u_to_t(u as f64)).as_vec3() }
    pub fn get_tangent(&self, t: f32) -> Vec3 { self.tangent(t as f64).as_vec3() }
    pub fn get_tangent_at(&self, u: f32) -> Vec3 { self.tangent(self.u_to_t(u as f64)).as_vec3() }
    pub fn get_length(&self) -> f32 { *self.lengths().last().unwrap() as f32 }
    pub fn get_points(&self, divisions: u32) -> Vec<Vec3> {
        (0..=divisions).map(|d| self.point(d as f64 / divisions as f64).as_vec3()).collect()
    }
    pub fn get_spaced_points(&self, divisions: u32) -> Vec<Vec3> {
        (0..=divisions).map(|d| self.point(self.u_to_t(d as f64 / divisions as f64)).as_vec3()).collect()
    }

    fn frames(&self, segments: usize, closed: bool) -> (Vec<DVec3>, Vec<DVec3>, Vec<DVec3>) {
        let tangents: Vec<DVec3> = (0..=segments).map(|i| self.tangent(self.u_to_t(i as f64 / segments as f64))).collect();
        let mut normals = vec![DVec3::ZERO; segments + 1];
        let mut binormals = vec![DVec3::ZERO; segments + 1];
        let t0 = tangents[0];
        let mut min = f64::MAX;
        let (tx, ty, tz) = (t0.x.abs(), t0.y.abs(), t0.z.abs());
        let mut normal = DVec3::ZERO;
        if tx <= min { min = tx; normal = DVec3::X; }
        if ty <= min { min = ty; normal = DVec3::Y; }
        if tz <= min { normal = DVec3::Z; }
        let vec = norm(t0.cross(normal));
        normals[0] = t0.cross(vec);
        binormals[0] = t0.cross(normals[0]);
        for i in 1..=segments {
            normals[i] = normals[i - 1];
            let v = tangents[i - 1].cross(tangents[i]);
            if v.length() > f64::EPSILON {
                let v = norm(v);
                let theta = tangents[i - 1].dot(tangents[i]).clamp(-1.0, 1.0).acos();
                normals[i] = rotate_axis(normals[i], v, theta);
            }
            binormals[i] = tangents[i].cross(normals[i]);
        }
        if closed {
            let mut theta = normals[0].dot(normals[segments]).clamp(-1.0, 1.0).acos();
            theta /= segments as f64;
            if t0.dot(normals[0].cross(normals[segments])) > 0.0 { theta = -theta; }
            for i in 1..=segments {
                normals[i] = rotate_axis(normals[i], tangents[i], theta * i as f64);
                binormals[i] = tangents[i].cross(normals[i]);
            }
        }
        (tangents, normals, binormals)
    }

    /// Curve.computeFrenetFrames
    pub fn compute_frenet_frames(&self, segments: u32, closed: bool) -> FrenetFrames {
        let (t, n, b) = self.frames(segments as usize, closed);
        let f = |v: Vec<DVec3>| v.iter().map(|q| q.as_vec3()).collect();
        FrenetFrames { tangents: f(t), normals: f(n), binormals: f(b) }
    }
}

/// new THREE.TubeGeometry(path, tubularSegments, radius, radialSegments)（closed は false）
pub fn tube(path: &CatmullRomCurve3, tubular_segments: u32, radius: f32, radial_segments: u32) -> Geometry {
    tube_full(path, tubular_segments, radius, radial_segments, false)
}

pub fn tube_full(path: &CatmullRomCurve3, tubular_segments: u32, radius: f32, radial_segments: u32, closed: bool) -> Geometry {
    let (ts, rs) = (tubular_segments as usize, radial_segments as usize);
    let r = radius as f64;
    let (_, frame_n, frame_b) = path.frames(ts, closed);
    let mut g = Geometry::default();
    let segment = |g: &mut Geometry, i: usize| {
        let p = path.point(path.u_to_t(i as f64 / ts as f64));
        let (n, b) = (frame_n[i], frame_b[i]);
        for j in 0..=rs {
            let v = j as f64 / rs as f64 * PI * 2.0;
            let (sin, cos) = (v.sin(), -v.cos());
            let normal = norm(n * cos + b * sin);
            g.normals.push(p3(normal));
            g.positions.push(p3(p + normal * r));
        }
    };
    for i in 0..ts { segment(&mut g, i); }
    segment(&mut g, if closed { 0 } else { ts });
    for i in 0..=ts {
        for j in 0..=rs { g.uvs.push([(i as f64 / ts as f64) as f32, (j as f64 / rs as f64) as f32]); }
    }
    let mut idx = Vec::new();
    let row = radial_segments + 1;
    for j in 1..=tubular_segments {
        for i in 1..=radial_segments {
            let a = row * (j - 1) + (i - 1);
            let b = row * j + (i - 1);
            let c = row * j + i;
            let d = row * (j - 1) + i;
            idx.extend_from_slice(&[a, b, d, b, c, d]);
        }
    }
    g.indices = Some(idx);
    g
}
