// Shape・Path・二次元の曲線・ShapeUtils・ShapeGeometry。three の extras と同じ。
// 点は倍精度で持つ（earcut の判定が three と同じ結果になるように）。
use super::{Geometry, earcut};
use glam::DVec2;
use std::f64::consts::PI;
use std::ops::{Deref, DerefMut};

/// Path を作る部品（three の LineCurve・QuadraticBezierCurve・CubicBezierCurve・EllipseCurve・SplineCurve）
#[derive(Clone, Debug)]
pub enum Curve2 {
    Line(DVec2, DVec2),
    Quadratic(DVec2, DVec2, DVec2),
    Cubic(DVec2, DVec2, DVec2, DVec2),
    Ellipse { x: f64, y: f64, rx: f64, ry: f64, start: f64, end: f64, clockwise: bool, rotation: f64 },
    Spline(Vec<DVec2>),
}

fn catmull_rom(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let v0 = (p2 - p0) * 0.5;
    let v1 = (p3 - p1) * 0.5;
    let t2 = t * t;
    let t3 = t * t2;
    (2.0 * p1 - 2.0 * p2 + v0 + v1) * t3 + (-3.0 * p1 + 3.0 * p2 - 2.0 * v0 - v1) * t2 + v0 * t + p1
}
fn quadratic(t: f64, p0: f64, p1: f64, p2: f64) -> f64 {
    let k = 1.0 - t;
    k * k * p0 + 2.0 * (1.0 - t) * t * p1 + t * t * p2
}
fn cubic(t: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let k = 1.0 - t;
    k * k * k * p0 + 3.0 * k * k * t * p1 + 3.0 * (1.0 - t) * t * t * p2 + t * t * t * p3
}

impl Curve2 {
    pub fn get_point(&self, t: f64) -> DVec2 {
        match self {
            Curve2::Line(a, b) => if t == 1.0 { *b } else { (*b - *a) * t + *a },
            Curve2::Quadratic(a, b, c) => DVec2::new(quadratic(t, a.x, b.x, c.x), quadratic(t, a.y, b.y, c.y)),
            Curve2::Cubic(a, b, c, d) => DVec2::new(cubic(t, a.x, b.x, c.x, d.x), cubic(t, a.y, b.y, c.y, d.y)),
            Curve2::Ellipse { x, y, rx, ry, start, end, clockwise, rotation } => {
                let two_pi = PI * 2.0;
                let mut delta = end - start;
                let same = delta.abs() < f64::EPSILON;
                while delta < 0.0 { delta += two_pi; }
                while delta > two_pi { delta -= two_pi; }
                if delta < f64::EPSILON { delta = if same { 0.0 } else { two_pi }; }
                if *clockwise && !same { delta = if delta == two_pi { -two_pi } else { delta - two_pi }; }
                let angle = start + t * delta;
                let mut px = x + rx * angle.cos();
                let mut py = y + ry * angle.sin();
                if *rotation != 0.0 {
                    let (c, s) = (rotation.cos(), rotation.sin());
                    let (tx, ty) = (px - x, py - y);
                    px = tx * c - ty * s + x;
                    py = tx * s + ty * c + y;
                }
                DVec2::new(px, py)
            }
            Curve2::Spline(pts) => {
                let p = (pts.len() - 1) as f64 * t;
                let int = p.floor() as i64;
                let w = p - int as f64;
                let n = pts.len() as i64;
                let at = |i: i64| pts[i as usize];
                let p0 = at(if int == 0 { int } else { int - 1 });
                let p1 = at(int);
                let p2 = at(if int > n - 2 { n - 1 } else { int + 1 });
                let p3 = at(if int > n - 3 { n - 1 } else { int + 2 });
                DVec2::new(catmull_rom(w, p0.x, p1.x, p2.x, p3.x), catmull_rom(w, p0.y, p1.y, p2.y, p3.y))
            }
        }
    }
}

/// three の Path。`p.move_to(0.0, 0.0).line_to(1.0, 0.0);` のようにつなげる。数は f64・f32 のどちらでも渡せる
#[derive(Clone, Debug, Default)]
pub struct Path { pub curves: Vec<Curve2>, pub current_point: DVec2 }

fn d(v: impl Into<f64>) -> f64 { v.into() }

impl Path {
    pub fn new() -> Path { Path::default() }
    /// new THREE.Path(points)
    pub fn from_points(points: &[DVec2]) -> Path {
        let mut p = Path::new();
        p.set_from_points(points);
        p
    }
    pub fn set_from_points(&mut self, points: &[DVec2]) -> &mut Self {
        if let Some(a) = points.first() { self.current_point = *a; }
        for q in points.iter().skip(1) { self.line_to(q.x, q.y); }
        self
    }
    pub fn move_to(&mut self, x: impl Into<f64>, y: impl Into<f64>) -> &mut Self {
        self.current_point = DVec2::new(d(x), d(y));
        self
    }
    pub fn line_to(&mut self, x: impl Into<f64>, y: impl Into<f64>) -> &mut Self {
        let to = DVec2::new(d(x), d(y));
        self.curves.push(Curve2::Line(self.current_point, to));
        self.current_point = to;
        self
    }
    pub fn quadratic_curve_to(&mut self, cx: impl Into<f64>, cy: impl Into<f64>, x: impl Into<f64>, y: impl Into<f64>) -> &mut Self {
        let to = DVec2::new(d(x), d(y));
        self.curves.push(Curve2::Quadratic(self.current_point, DVec2::new(d(cx), d(cy)), to));
        self.current_point = to;
        self
    }
    pub fn bezier_curve_to(&mut self, c1x: impl Into<f64>, c1y: impl Into<f64>, c2x: impl Into<f64>, c2y: impl Into<f64>, x: impl Into<f64>, y: impl Into<f64>) -> &mut Self {
        let to = DVec2::new(d(x), d(y));
        self.curves.push(Curve2::Cubic(self.current_point, DVec2::new(d(c1x), d(c1y)), DVec2::new(d(c2x), d(c2y)), to));
        self.current_point = to;
        self
    }
    pub fn spline_thru(&mut self, pts: &[DVec2]) -> &mut Self {
        let mut all = vec![self.current_point];
        all.extend_from_slice(pts);
        self.curves.push(Curve2::Spline(all));
        if let Some(l) = pts.last() { self.current_point = *l; }
        self
    }
    pub fn arc(&mut self, x: impl Into<f64>, y: impl Into<f64>, radius: impl Into<f64>, start: impl Into<f64>, end: impl Into<f64>, clockwise: bool) -> &mut Self {
        let c = self.current_point;
        self.absarc(d(x) + c.x, d(y) + c.y, radius, start, end, clockwise)
    }
    pub fn absarc(&mut self, x: impl Into<f64>, y: impl Into<f64>, radius: impl Into<f64>, start: impl Into<f64>, end: impl Into<f64>, clockwise: bool) -> &mut Self {
        let r = d(radius);
        self.absellipse(x, y, r, r, start, end, clockwise, 0.0)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn ellipse(&mut self, x: impl Into<f64>, y: impl Into<f64>, rx: impl Into<f64>, ry: impl Into<f64>, start: impl Into<f64>, end: impl Into<f64>, clockwise: bool, rotation: impl Into<f64>) -> &mut Self {
        let c = self.current_point;
        self.absellipse(d(x) + c.x, d(y) + c.y, rx, ry, start, end, clockwise, rotation)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn absellipse(&mut self, x: impl Into<f64>, y: impl Into<f64>, rx: impl Into<f64>, ry: impl Into<f64>, start: impl Into<f64>, end: impl Into<f64>, clockwise: bool, rotation: impl Into<f64>) -> &mut Self {
        let curve = Curve2::Ellipse { x: d(x), y: d(y), rx: d(rx), ry: d(ry), start: d(start), end: d(end), clockwise, rotation: d(rotation) };
        if !self.curves.is_empty() {
            let first = curve.get_point(0.0);
            if first != self.current_point { self.line_to(first.x, first.y); }
        }
        let last = curve.get_point(1.0);
        self.curves.push(curve);
        self.current_point = last;
        self
    }
    /// three の closePath（始めと終わりが違えば線を足す）
    pub fn close_path(&mut self) -> &mut Self {
        if let (Some(a), Some(b)) = (self.curves.first(), self.curves.last()) {
            let (s, e) = (a.get_point(0.0), b.get_point(1.0));
            if s != e { self.curves.push(Curve2::Line(e, s)); }
        }
        self
    }
    /// three の CurvePath.getPoints（線は 1 分割・楕円は 2 倍・スプラインは点の数倍。続けて同じ点は入れない）
    pub fn get_points(&self, divisions: u32) -> Vec<DVec2> {
        let mut out: Vec<DVec2> = Vec::new();
        for c in &self.curves {
            let res = match c {
                Curve2::Ellipse { .. } => divisions * 2,
                Curve2::Line(..) => 1,
                Curve2::Spline(p) => divisions * p.len() as u32,
                _ => divisions,
            };
            for k in 0..=res {
                let q = c.get_point(k as f64 / res as f64);
                if out.last() == Some(&q) { continue; }
                out.push(q);
            }
        }
        out
    }
}

/// three の Shape（Path に穴を足した物）。Path のメソッドはそのまま使える
#[derive(Clone, Debug, Default)]
pub struct Shape { pub path: Path, pub holes: Vec<Path> }
impl Deref for Shape { type Target = Path; fn deref(&self) -> &Path { &self.path } }
impl DerefMut for Shape { fn deref_mut(&mut self) -> &mut Path { &mut self.path } }
impl Shape {
    pub fn new() -> Shape { Shape::default() }
    /// new THREE.Shape(points)
    pub fn from_points(points: &[DVec2]) -> Shape { Shape { path: Path::from_points(points), holes: Vec::new() } }
    /// three の extractPoints（外周・穴）
    pub fn extract_points(&self, divisions: u32) -> (Vec<DVec2>, Vec<Vec<DVec2>>) {
        (self.get_points(divisions), self.holes.iter().map(|h| h.get_points(divisions)).collect())
    }
}

// ---- ShapeUtils ----
pub fn area(contour: &[DVec2]) -> f64 {
    let n = contour.len();
    if n == 0 { return 0.0; }
    let mut a = 0.0;
    let mut p = n - 1;
    for q in 0..n {
        a += contour[p].x * contour[q].y - contour[q].x * contour[p].y;
        p = q;
    }
    a * 0.5
}
pub fn is_clock_wise(pts: &[DVec2]) -> bool { area(pts) < 0.0 }

/// ShapeUtils.triangulateShape。three と同じく、終わりが始めと同じ点は取り除く（渡した配列が変わる）
pub fn triangulate_shape(contour: &mut Vec<DVec2>, holes: &mut [Vec<DVec2>]) -> Vec<[usize; 3]> {
    fn remove_dup(points: &mut Vec<DVec2>) {
        let l = points.len();
        if l > 2 && points[l - 1] == points[0] { points.pop(); }
    }
    let mut verts = Vec::new();
    let add = |verts: &mut Vec<f64>, c: &[DVec2]| for q in c { verts.push(q.x); verts.push(q.y); };
    remove_dup(contour);
    add(&mut verts, contour);
    let mut hole_index = contour.len();
    holes.iter_mut().for_each(remove_dup);
    let mut hole_indices = Vec::new();
    for h in holes.iter() {
        hole_indices.push(hole_index);
        hole_index += h.len();
        add(&mut verts, h);
    }
    earcut(&verts, &hole_indices).chunks_exact(3).map(|t| [t[0], t[1], t[2]]).collect()
}

// ---- ShapeGeometry ----
fn add_shape(g: &mut Geometry, idx: &mut Vec<u32>, shape: &Shape, curve_segments: u32) -> u32 {
    let offset = g.positions.len() as u32;
    let (mut verts, mut holes) = shape.extract_points(curve_segments);
    if !is_clock_wise(&verts) { verts.reverse(); }
    for h in &mut holes { if is_clock_wise(h) { h.reverse(); } }
    let faces = triangulate_shape(&mut verts, &mut holes);
    for h in &holes { verts.extend_from_slice(h); }
    for v in &verts {
        g.positions.push([v.x as f32, v.y as f32, 0.0]);
        g.normals.push([0.0, 0.0, 1.0]);
        g.uvs.push([v.x as f32, v.y as f32]);
    }
    for f in &faces { for k in f { idx.push(*k as u32 + offset); } }
    faces.len() as u32 * 3
}

/// new THREE.ShapeGeometry(shape, curveSegments)（three の既定は 12）。groups は無い
pub fn shape_geometry(shape: &Shape, curve_segments: u32) -> Geometry {
    let mut g = Geometry::default();
    let mut idx = Vec::new();
    add_shape(&mut g, &mut idx, shape, curve_segments);
    g.indices = Some(idx);
    g
}

/// new THREE.ShapeGeometry([shapes], curveSegments)。形ごとに group が付く
pub fn shapes_geometry(shapes: &[Shape], curve_segments: u32) -> Geometry {
    let mut g = Geometry::default();
    let mut idx = Vec::new();
    let mut start = 0;
    for (i, s) in shapes.iter().enumerate() {
        let count = add_shape(&mut g, &mut idx, s, curve_segments);
        g.add_group(start, count, i as u32);
        start += count;
    }
    g.indices = Some(idx);
    g
}
