// 形（three の BufferGeometry の役）。各形の頂点・法線・UV の式は three と同じ。担当: README の「形」。
// three は倍精度で計算して Float32Array に入れるので、ここも内側は f64 で計算して f32 に入れる。
use glam::{DMat3, DMat4, DVec3, Mat4, Vec3};

mod earcut;
mod extrude;
mod primitives;
mod shape;
mod tube;

pub use earcut::earcut;
pub use extrude::{ExtrudeOpts, extrude, extrude_many};
pub use primitives::*;
pub use shape::{Curve2, Path, Shape, area, is_clock_wise, shape_geometry, shapes_geometry, triangulate_shape};
pub use tube::{CatmullRomCurve3, CurveType, FrenetFrames, tube, tube_full};

/// 材質の割り当ての区切り（three の geometry.groups。CylinderGeometry は 0 側面・1 上・2 下）
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Group { pub start: u32, pub count: u32, pub material: u32 }

/// 形。indices が None なら頂点の順に 3 つずつ（toNonIndexed の結果など）
#[derive(Clone, Debug, Default)]
pub struct Geometry {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// 頂点色（線形）。無ければ空
    pub colors: Vec<[f32; 4]>,
    /// 草木の揺れの量（_sway）。無ければ空
    pub sway: Vec<f32>,
    pub indices: Option<Vec<u32>>,
    pub groups: Vec<Group>,
}

/// Math.sign（0 は 0 のまま）
pub(crate) fn js_sign(x: f64) -> f64 { if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { x } }
/// Vector3.normalize（長さ 0 はそのまま）
pub(crate) fn norm(v: DVec3) -> DVec3 { let l = v.length(); v * (1.0 / if l == 0.0 || l.is_nan() { 1.0 } else { l }) }
pub(crate) fn p3(v: DVec3) -> [f32; 3] { [v.x as f32, v.y as f32, v.z as f32] }
fn d3(a: [f32; 3]) -> DVec3 { DVec3::new(a[0] as f64, a[1] as f64, a[2] as f64) }

impl Geometry {
    /// 頂点の数
    pub fn vertex_count(&self) -> usize { self.positions.len() }

    /// 鎖の終わりで持ち主へ渡す（`let g = plane(1.0, 1.0).rotate_x(a).owned();`）。TS の `new THREE.X().rotateX(a)` に当たる
    pub fn owned(&mut self) -> Geometry { std::mem::take(self) }

    /// three の addGroup
    pub fn add_group(&mut self, start: u32, count: u32, material: u32) { self.groups.push(Group { start, count, material }); }

    /// three の applyMatrix4（法線は法線行列をかけて長さ 1 に直す）
    pub fn apply_matrix4(&mut self, m: Mat4) -> &mut Self { self.apply_d(m.as_dmat4()) }

    fn apply_d(&mut self, m: DMat4) -> &mut Self {
        for p in &mut self.positions {
            let v = d3(*p);
            let w = 1.0 / (m.x_axis.w * v.x + m.y_axis.w * v.y + m.z_axis.w * v.z + m.w_axis.w);
            let r = DVec3::new(
                m.x_axis.x * v.x + m.y_axis.x * v.y + m.z_axis.x * v.z + m.w_axis.x,
                m.x_axis.y * v.x + m.y_axis.y * v.y + m.z_axis.y * v.z + m.w_axis.y,
                m.x_axis.z * v.x + m.y_axis.z * v.y + m.z_axis.z * v.z + m.w_axis.z,
            ) * w;
            *p = p3(r);
        }
        if !self.normals.is_empty() {
            // Matrix3.getNormalMatrix: 行列式 0 は全部 0 にする
            let m3 = DMat3::from_mat4(m);
            let nm = if m3.determinant() == 0.0 { DMat3::ZERO } else { m3.inverse().transpose() };
            for n in &mut self.normals { *n = p3(norm(nm * d3(*n))); }
        }
        self
    }

    pub fn rotate_x(&mut self, a: f32) -> &mut Self { self.apply_d(DMat4::from_rotation_x(a as f64)) }
    pub fn rotate_y(&mut self, a: f32) -> &mut Self { self.apply_d(DMat4::from_rotation_y(a as f64)) }
    pub fn rotate_z(&mut self, a: f32) -> &mut Self { self.apply_d(DMat4::from_rotation_z(a as f64)) }
    pub fn translate(&mut self, x: f32, y: f32, z: f32) -> &mut Self { self.apply_d(DMat4::from_translation(DVec3::new(x as f64, y as f64, z as f64))) }
    pub fn scale(&mut self, x: f32, y: f32, z: f32) -> &mut Self { self.apply_d(DMat4::from_scale(DVec3::new(x as f64, y as f64, z as f64))) }

    fn box_center(&self) -> DVec3 {
        if self.positions.is_empty() { return DVec3::ZERO; }
        let (mut lo, mut hi) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        for p in &self.positions { let v = d3(*p); lo = lo.min(v); hi = hi.max(v); }
        (lo + hi) * 0.5
    }

    /// three の computeBoundingBox の (min, max)。頂点が無ければ (0, 0)
    pub fn bounding_box(&self) -> (Vec3, Vec3) {
        if self.positions.is_empty() { return (Vec3::ZERO, Vec3::ZERO); }
        let (mut lo, mut hi) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
        for p in &self.positions { let v = Vec3::from(*p); lo = lo.min(v); hi = hi.max(v); }
        (lo, hi)
    }

    /// three の center（箱の中心を引く）
    pub fn center(&mut self) -> &mut Self {
        let c = -self.box_center();
        self.apply_d(DMat4::from_translation(c))
    }

    /// three の computeBoundingSphere（箱の中心と、そこから一番遠い頂点まで）
    pub fn bounding_sphere(&self) -> (Vec3, f32) {
        let c = self.box_center();
        let mut r2 = 0.0f64;
        for p in &self.positions { r2 = r2.max(c.distance_squared(d3(*p))); }
        (c.as_vec3(), r2.sqrt() as f32)
    }

    /// three の normalizeNormals
    pub fn normalize_normals(&mut self) -> &mut Self {
        for n in &mut self.normals { *n = p3(norm(d3(*n))); }
        self
    }

    /// three の computeVertexNormals（index あり: 面の外積を足して長さ 1。index なし: 3 頂点に同じ面の向き）
    pub fn compute_vertex_normals(&mut self) -> &mut Self {
        self.normals.clear();
        self.normals.resize(self.positions.len(), [0.0; 3]);
        let face = |p: &[[f32; 3]], a: usize, b: usize, c: usize| (d3(p[c]) - d3(p[b])).cross(d3(p[a]) - d3(p[b]));
        if let Some(index) = &self.indices {
            for t in index.chunks_exact(3) {
                let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
                let cb = face(&self.positions, a, b, c);
                for i in [a, b, c] { self.normals[i] = p3(d3(self.normals[i]) + cb); }
            }
        } else {
            let mut i = 0;
            while i + 2 < self.positions.len() {
                let cb = p3(face(&self.positions, i, i + 1, i + 2));
                self.normals[i] = cb;
                self.normals[i + 1] = cb;
                self.normals[i + 2] = cb;
                i += 3;
            }
        }
        self.normalize_normals()
    }

    /// three の toNonIndexed（groups はそのまま）。index が無ければ写しを返す
    pub fn to_non_indexed(&self) -> Geometry {
        let Some(index) = &self.indices else { return self.clone() };
        fn pick<T: Copy>(src: &[T], index: &[u32]) -> Vec<T> {
            if src.is_empty() { return Vec::new(); }
            index.iter().map(|&i| src[i as usize]).collect()
        }
        Geometry {
            positions: pick(&self.positions, index),
            normals: pick(&self.normals, index),
            uvs: pick(&self.uvs, index),
            colors: pick(&self.colors, index),
            sway: pick(&self.sway, index),
            indices: None,
            groups: self.groups.clone(),
        }
    }
}

/// BufferGeometryUtils.mergeGeometries。index の有無と属性の種類がそろわなければ None
pub fn merge_geometries(geometries: &[&Geometry], use_groups: bool) -> Option<Geometry> {
    let first = geometries.first()?;
    let kinds = |g: &Geometry| (g.indices.is_some(), !g.normals.is_empty(), !g.uvs.is_empty(), !g.colors.is_empty(), !g.sway.is_empty());
    let mut out = Geometry::default();
    if first.indices.is_some() { out.indices = Some(Vec::new()); }
    let (mut offset, mut base) = (0u32, 0u32);
    for (i, g) in geometries.iter().enumerate() {
        if kinds(g) != kinds(first) { return None; }
        if let (Some(dst), Some(src)) = (&mut out.indices, &g.indices) { dst.extend(src.iter().map(|&v| v + base)); }
        if use_groups {
            let count = g.indices.as_ref().map_or(g.positions.len(), Vec::len) as u32;
            out.add_group(offset, count, i as u32);
            offset += count;
        }
        base += g.positions.len() as u32;
        out.positions.extend_from_slice(&g.positions);
        out.normals.extend_from_slice(&g.normals);
        out.uvs.extend_from_slice(&g.uvs);
        out.colors.extend_from_slice(&g.colors);
        out.sway.extend_from_slice(&g.sway);
    }
    Some(out)
}
