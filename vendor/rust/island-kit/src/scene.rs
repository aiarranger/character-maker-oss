// 節点の木（three の Object3D の木を小さく）。値は pub のフィールドに直接書く。GPU を持たない。
use crate::material::{GeoId, MatId};
use crate::math::{Color, EulerOrder, quat_from_euler};
use glam::{Mat3, Mat4, Quat, Vec2, Vec3};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct NodeId(pub u32);

/// 回転（three の rotation か quaternion）
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Rot {
    Euler { x: f32, y: f32, z: f32, order: EulerOrder },
    Quat(Quat),
}
impl Default for Rot { fn default() -> Self { Rot::IDENTITY } }
impl Rot {
    pub const IDENTITY: Rot = Rot::Euler { x: 0.0, y: 0.0, z: 0.0, order: EulerOrder::XYZ };
    pub fn euler(x: f32, y: f32, z: f32) -> Rot { Rot::Euler { x, y, z, order: EulerOrder::XYZ } }
    pub fn euler_order(x: f32, y: f32, z: f32, order: EulerOrder) -> Rot { Rot::Euler { x, y, z, order } }
    pub fn x(a: f32) -> Rot { Rot::euler(a, 0.0, 0.0) }
    pub fn y(a: f32) -> Rot { Rot::euler(0.0, a, 0.0) }
    pub fn z(a: f32) -> Rot { Rot::euler(0.0, 0.0, a) }
    pub fn to_quat(self) -> Quat {
        match self {
            Rot::Euler { x, y, z, order } => quat_from_euler(x, y, z, order),
            Rot::Quat(q) => q,
        }
    }
    /// three の `o.rotation.x = a`（ほかの軸は保つ。四元数のときはオイラーへ直す）
    pub fn set_x(&mut self, a: f32) { let (_, y, z, order) = self.parts(); *self = Rot::Euler { x: a, y, z, order }; }
    pub fn set_y(&mut self, a: f32) { let (x, _, z, order) = self.parts(); *self = Rot::Euler { x, y: a, z, order }; }
    pub fn set_z(&mut self, a: f32) { let (x, y, _, order) = self.parts(); *self = Rot::Euler { x, y, z: a, order }; }
    /// オイラー角（x, y, z, 順）。四元数なら XYZ に直す（three の Euler.setFromQuaternion）
    pub fn parts(self) -> (f32, f32, f32, EulerOrder) {
        match self {
            Rot::Euler { x, y, z, order } => (x, y, z, order),
            Rot::Quat(q) => {
                let m = Mat3::from_quat(q);
                let m13 = m.z_axis.x;
                let y = m13.clamp(-1.0, 1.0).asin();
                let (x, z) = if m13.abs() < 0.9999999 { ((-m.z_axis.y).atan2(m.z_axis.z), (-m.y_axis.x).atan2(m.x_axis.x)) } else { (m.y_axis.z.atan2(m.y_axis.y), 0.0) };
                (x, y, z, EulerOrder::XYZ)
            }
        }
    }
}

/// three の Matrix4.lookAt の回転（eye から target を向く。-Z が前になるカメラ・光の形）
pub fn look_at_rotation(eye: Vec3, target: Vec3, up: Vec3) -> Mat3 {
    let mut z = eye - target;
    if z.length_squared() == 0.0 { z.z = 1.0; }
    z = z.normalize();
    let mut x = up.cross(z);
    if x.length_squared() == 0.0 {
        if up.z.abs() == 1.0 { z.x += 0.0001; } else { z.z += 0.0001; }
        z = z.normalize();
        x = up.cross(z);
    }
    x = x.normalize();
    let y = z.cross(x);
    Mat3::from_cols(x, y, z)
}

fn quat_of(m: Mat3) -> Quat { Quat::from_mat3(&m).normalize() }

/// カメラ（three の PerspectiveCamera の必要な分）
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub quaternion: Quat,
    pub fov: f32,
    pub aspect: f32,
    pub near: f32,
    pub far: f32,
    pub up: Vec3,
    /// 描く層（three の camera.layers。ビット）
    pub layers: u32,
    /// three の setViewOffset（全体と同じ大きさの窓をずらす形だけ）: (offsetX / fullWidth, offsetY / fullHeight)。既定 0
    pub view_offset: (f32, f32),
}
impl Default for Camera {
    fn default() -> Self { Camera { position: Vec3::ZERO, quaternion: Quat::IDENTITY, fov: 50.0, aspect: 1.0, near: 0.1, far: 2000.0, up: Vec3::Y, layers: 1, view_offset: (0.0, 0.0) } }
}
impl Camera {
    pub fn new(fov: f32, aspect: f32, near: f32, far: f32) -> Camera { Camera { fov, aspect, near, far, ..Default::default() } }
    pub fn look_at(&mut self, target: Vec3) { self.quaternion = quat_of(look_at_rotation(self.position, target, self.up)); }
    pub fn world(&self) -> Mat4 { Mat4::from_rotation_translation(self.quaternion, self.position) }
    pub fn view(&self) -> Mat4 { self.world().inverse() }
    /// 描く投影（奥行き 0..1）
    pub fn proj(&self) -> Mat4 {
        let mut m = Mat4::perspective_rh(self.fov.to_radians(), self.aspect, self.near, self.far);
        m.z_axis.x += 2.0 * self.view_offset.0;
        m.z_axis.y -= 2.0 * self.view_offset.1;
        m
    }
    pub fn view_proj(&self) -> Mat4 { self.proj() * self.view() }
    /// three の Vector3.project（NDC。z も three と同じ -1..1）
    pub fn project(&self, p: Vec3) -> Vec3 {
        let v = self.view().transform_point3(p);
        let t = (self.fov.to_radians() * 0.5).tan();
        let (n, f) = (self.near, self.far);
        let w = -v.z;
        Vec3::new(v.x / (t * self.aspect) / w - 2.0 * self.view_offset.0, v.y / t / w + 2.0 * self.view_offset.1, (-(f + n) / (f - n) * v.z - 2.0 * f * n / (f - n)) / w)
    }
    pub fn right(&self) -> Vec3 { self.quaternion * Vec3::X }
    pub fn upward(&self) -> Vec3 { self.quaternion * Vec3::Y }
    pub fn forward(&self) -> Vec3 { self.quaternion * Vec3::NEG_Z }
}

/// 行列で決めるカメラ（three の camera.matrixWorldInverse と projectionMatrix をそのまま使う。kit.render_view_to）。
/// proj は WebGPU の奥行き 0..1。three の投影（-1..1）は from_three で直す
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RawView {
    pub view: Mat4,
    pub proj: Mat4,
    /// cameraPosition（霧・つや・縁の光の向き）
    pub position: Vec3,
    /// 描く層（three の camera.layers）
    pub layers: u32,
}
impl RawView {
    pub fn from_camera(c: &Camera) -> RawView { RawView { view: c.view(), proj: c.proj(), position: c.position, layers: c.layers } }
    /// three の行列から（proj は three の -1..1 の奥行き。Reflector の斜めの near を入れた行列もそのまま渡せる）。
    /// 奥行きだけ z' = (z + w) / 2 で 0..1 へ直す（x・y・w と、三の視錐台の 6 面は同じ）。position は matrixWorld の位置。層は 1
    pub fn from_three(view: Mat4, proj: Mat4, position: Vec3) -> RawView {
        let fix = Mat4::from_cols(glam::Vec4::X, glam::Vec4::Y, glam::Vec4::new(0.0, 0.0, 0.5, 0.0), glam::Vec4::new(0.0, 0.0, 0.5, 1.0));
        RawView { view, proj: fix * proj, position, layers: 1 }
    }
}

/// 影の設定（three の LightShadow。pengo の値は 2048 と ±17・±15 など）
#[derive(Clone, Copy, Debug)]
pub struct Shadow {
    pub map_size: u32,
    pub left: f32, pub right: f32, pub top: f32, pub bottom: f32, pub near: f32, pub far: f32,
    pub bias: f32,
    pub normal_bias: f32,
    pub radius: f32,
    pub intensity: f32,
}
impl Default for Shadow {
    fn default() -> Self { Shadow { map_size: 512, left: -5.0, right: 5.0, top: 5.0, bottom: -5.0, near: 0.5, far: 500.0, bias: 0.0, normal_bias: 0.0, radius: 1.0, intensity: 1.0 } }
}

/// 光が向く先
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target { Pos(Vec3), Node(NodeId) }

#[derive(Clone, Copy, Debug)]
pub struct DirLight { pub color: Color, pub intensity: f32, pub target: Target, pub shadow: Shadow }
#[derive(Clone, Copy, Debug)]
pub struct HemiLight { pub sky: Color, pub ground: Color, pub intensity: f32 }
#[derive(Clone, Copy, Debug)]
pub struct AmbientLight { pub color: Color, pub intensity: f32 }
#[derive(Clone, Copy, Debug)]
pub struct PointLight { pub color: Color, pub intensity: f32, pub distance: f32, pub decay: f32 }
#[derive(Clone, Copy, Debug)]
pub struct SpotLight { pub color: Color, pub intensity: f32, pub distance: f32, pub angle: f32, pub penumbra: f32, pub decay: f32, pub target: Target }

/// InstancedMesh の中身
#[derive(Clone, Debug)]
pub struct Instances {
    pub geo: GeoId,
    pub mat: MatId,
    pub capacity: usize,
    pub count: usize,
    pub(crate) matrices: Vec<Mat4>,
    pub(crate) colors: Option<Vec<[f32; 3]>>,
    pub(crate) attrs: Vec<Vec<f32>>,
    pub(crate) dirty: bool,
    pub(crate) attr_dirty: Vec<bool>,
    /// three の computeBoundingSphere（作ったら更新しない。three と同じ）
    pub(crate) bounds: Option<(Vec3, f32)>,
}

/// Points の中身（three の Points と動的な BufferAttribute）
#[derive(Clone, Debug)]
pub struct PointSet {
    pub mat: MatId,
    pub capacity: usize,
    pub count: usize,
    pub(crate) positions: Vec<f32>,
    pub(crate) attrs: Vec<Vec<f32>>,
    pub(crate) pos_dirty: bool,
    pub(crate) attr_dirty: Vec<bool>,
    pub(crate) bounds: Option<(Vec3, f32)>,
}

/// LineSegments の中身
#[derive(Clone, Debug)]
pub struct LineSet {
    pub mat: MatId,
    pub capacity: usize,
    pub(crate) positions: Vec<f32>,
    pub(crate) dirty: bool,
    pub(crate) bounds: Option<(Vec3, f32)>,
}

/// 骨の組の番号（three の Skeleton 1 つ）
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SkeletonId(pub u32);

/// three の Skeleton。骨は場面の節点なので、動かすのは節点の TRS。bones[j]・inverses[j] の j が頂点の JOINTS_0
#[derive(Clone, Debug, Default)]
pub struct Skeleton { pub bones: Vec<NodeId>, pub inverses: Vec<Mat4> }

/// three の SkinnedMesh（1 つの形と 1 つの材質。GLTFLoader は部分ごとに 1 つ作る）
#[derive(Clone, Copy, Debug)]
pub struct SkinnedMesh {
    pub geo: GeoId,
    pub mat: MatId,
    pub skeleton: SkeletonId,
    /// three の mesh.boundingSphere（節点の局所。無ければ形の球＝バインドの姿勢。動きで大きく外れるなら frustum_culled = false か set_skinned_bounds）
    pub(crate) bounds: Option<(Vec3, f32)>,
}

/// BatchedMesh の 1 インスタンス
#[derive(Clone, Copy, Debug)]
pub(crate) struct BatchInst {
    pub shape: u32,
    pub visible: bool,
    pub matrix: Mat4,
    pub color: [f32; 4],
}

/// BatchedMesh の中身（形の表とインスタンスの表。材質は 1 つ）。見える物は毎コマ視野ごとに集める（render/batch.rs）
#[derive(Clone, Debug)]
pub struct Batch {
    pub mat: MatId,
    /// three の perObjectFrustumCulled（既定 true）
    pub per_object_culled: bool,
    pub(crate) shapes: Vec<GeoId>,
    pub(crate) insts: Vec<BatchInst>,
}
impl Batch {
    /// 描く印のあるインスタンスの数（視錐台で間引く前。three の _instanceInfo の visible）
    pub fn visible_count(&self) -> usize { self.insts.iter().filter(|i| i.visible).count() }
    pub fn len(&self) -> usize { self.insts.len() }
    pub fn is_empty(&self) -> bool { self.insts.is_empty() }
    /// 読むだけ（値の記録など）: (形の番号, 描く印, 行列, 色)
    pub fn instance(&self, i: usize) -> (u32, bool, Mat4, [f32; 4]) { let k = &self.insts[i]; (k.shape, k.visible, k.matrix, k.color) }
    /// 形の番号 → 形
    pub fn shape_geo(&self, shape: u32) -> GeoId { self.shapes[shape as usize] }
}

#[derive(Clone, Debug)]
pub enum NodeKind {
    /// 何も描かない（three の Object3D）
    Object,
    /// three の Group（renderOrder が子の並びの先頭の鍵になる）
    Group,
    /// mats が 1 つなら全体に、2 つ以上なら geometry.groups の material 番号で引く
    Mesh { geo: GeoId, mats: Vec<MatId>, multi: bool },
    Instanced(Instances),
    /// three の BatchedMesh
    Batched(Batch),
    /// three の SkinnedMesh
    Skinned(SkinnedMesh),
    Points(PointSet),
    Sprite { mat: MatId, center: Vec2 },
    Lines(LineSet),
    Dir(DirLight),
    Hemi(HemiLight),
    Ambient(AmbientLight),
    Point(PointLight),
    Spot(SpotLight),
    /// 主役の入れ物（kit.set_actor）
    Actor,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub position: Vec3,
    pub rotation: Rot,
    pub scale: Vec3,
    pub visible: bool,
    pub render_order: i32,
    /// 層のビット（0 と 1 を使う。three の layers.mask）
    pub layers: u32,
    pub cast_shadow: bool,
    pub receive_shadow: bool,
    pub frustum_culled: bool,
    pub name: String,
    pub up: Vec3,
    /// 行列を直接置く（three の matrixAutoUpdate = false）
    pub matrix: Option<Mat4>,
    pub kind: NodeKind,
    pub(crate) parent: Option<NodeId>,
    pub(crate) children: Vec<NodeId>,
    /// 作った順の番号（three の object.id。半透明の並びの最後の鍵）
    pub(crate) uid: u32,
    pub(crate) alive: bool,
    pub(crate) world: Mat4,
    /// 世界の行列が変わったコマ
    pub(crate) world_gen: u32,
    cached: (Vec3, Rot, Vec3, Option<Mat4>),
}

impl Node {
    fn new(kind: NodeKind, uid: u32) -> Node {
        Node {
            position: Vec3::ZERO, rotation: Rot::IDENTITY, scale: Vec3::ONE, visible: true, render_order: 0, layers: 1, cast_shadow: false, receive_shadow: false, frustum_culled: true,
            name: String::new(), up: Vec3::Y, matrix: None, kind, parent: None, children: Vec::new(), uid, alive: true, world: Mat4::IDENTITY, world_gen: 0,
            cached: (Vec3::NAN, Rot::IDENTITY, Vec3::NAN, None),
        }
    }
    pub fn local(&self) -> Mat4 {
        self.matrix.unwrap_or_else(|| Mat4::from_scale_rotation_translation(self.scale, self.rotation.to_quat(), self.position))
    }
    /// three の `o.scale.setScalar(s)`
    pub fn set_scale(&mut self, s: f32) { self.scale = Vec3::splat(s); }
    pub fn parent(&self) -> Option<NodeId> { self.parent }
    pub fn children(&self) -> &[NodeId] { &self.children }
}

pub struct Scene {
    nodes: Vec<Node>,
    skeletons: Vec<Option<Skeleton>>,
    free: Vec<u32>,
    next_uid: u32,
    /// 場面の根（three の scene）
    pub root: NodeId,
    pub(crate) frame: u32,
}

impl Default for Scene { fn default() -> Self { Scene::new() } }

impl std::ops::Index<NodeId> for Scene {
    type Output = Node;
    fn index(&self, id: NodeId) -> &Node { &self.nodes[id.0 as usize] }
}
impl std::ops::IndexMut<NodeId> for Scene {
    fn index_mut(&mut self, id: NodeId) -> &mut Node { &mut self.nodes[id.0 as usize] }
}

impl Scene {
    pub fn new() -> Scene {
        Scene { nodes: vec![Node::new(NodeKind::Object, 0)], skeletons: Vec::new(), free: Vec::new(), next_uid: 1, root: NodeId(0), frame: 1 }
    }

    fn add(&mut self, parent: Option<NodeId>, kind: NodeKind) -> NodeId {
        let uid = self.next_uid;
        self.next_uid += 1;
        let node = Node::new(kind, uid);
        let id = if let Some(i) = self.free.pop() { self.nodes[i as usize] = node; NodeId(i) } else { self.nodes.push(node); NodeId(self.nodes.len() as u32 - 1) };
        let p = parent.unwrap_or(self.root);
        self.nodes[id.0 as usize].parent = Some(p);
        self.nodes[p.0 as usize].children.push(id);
        id
    }

    /// three の Object3D（並びに影響しない入れ物）
    pub fn object(&mut self, parent: Option<NodeId>) -> NodeId { self.add(parent, NodeKind::Object) }
    /// three の Group
    pub fn group(&mut self, parent: Option<NodeId>) -> NodeId { self.add(parent, NodeKind::Group) }
    pub fn mesh(&mut self, parent: Option<NodeId>, geo: GeoId, mat: MatId) -> NodeId { self.add(parent, NodeKind::Mesh { geo, mats: vec![mat], multi: false }) }
    /// 材質の配列（geometry.groups の material 番号で引く）
    pub fn mesh_multi(&mut self, parent: Option<NodeId>, geo: GeoId, mats: &[MatId]) -> NodeId { self.add(parent, NodeKind::Mesh { geo, mats: mats.to_vec(), multi: true }) }
    /// three の InstancedMesh（count は capacity、行列は単位行列から）
    pub fn instanced(&mut self, parent: Option<NodeId>, geo: GeoId, mat: MatId, capacity: usize) -> NodeId {
        self.add(parent, NodeKind::Instanced(Instances { geo, mat, capacity, count: capacity, matrices: vec![Mat4::IDENTITY; capacity], colors: None, attrs: Vec::new(), dirty: true, attr_dirty: Vec::new(), bounds: None }))
    }
    /// three の BatchedMesh（batch_add_shape が addGeometry、batch_add_instance が addInstance。使い方は README）
    pub fn batched(&mut self, parent: Option<NodeId>, mat: MatId) -> NodeId {
        self.add(parent, NodeKind::Batched(Batch { mat, per_object_culled: true, shapes: Vec::new(), insts: Vec::new() }))
    }
    /// three の SkinnedMesh。形は kit.geometry_skinned（または glb::instantiate_skinned）で作る。世界の位置は骨だけで決まり、節点の行列は法線と並びに使う（three の AttachedBindMode と同じ）
    pub fn skinned(&mut self, parent: Option<NodeId>, geo: GeoId, mat: MatId, skeleton: SkeletonId) -> NodeId {
        self.add(parent, NodeKind::Skinned(SkinnedMesh { geo, mat, skeleton, bounds: None }))
    }
    /// three の mesh.boundingSphere = new Sphere(c, r)（節点の局所。視錐台の間引きと並びに使う）
    pub fn set_skinned_bounds(&mut self, id: NodeId, c: Vec3, r: f32) {
        if let NodeKind::Skinned(s) = &mut self[id].kind { s.bounds = Some((c, r)); }
    }
    /// 骨の組を作る（SkeletonUtils.clone なら、複製した節点で 1 体に 1 つ作る）
    pub fn new_skeleton(&mut self, bones: Vec<NodeId>, inverses: Vec<Mat4>) -> SkeletonId {
        self.skeletons.push(Some(Skeleton { bones, inverses }));
        SkeletonId(self.skeletons.len() as u32 - 1)
    }
    /// 骨を入れ直す（節点を全部作ってから骨を決める GLB の読み込み）
    pub fn set_skeleton(&mut self, id: SkeletonId, bones: Vec<NodeId>, inverses: Vec<Mat4>) { self.skeletons[id.0 as usize] = Some(Skeleton { bones, inverses }); }
    /// 骨の組を消す（skeleton.dispose。番号は使い回さない。GPU の分は次の描画で kit が片付ける）。節点は別に remove する
    pub fn remove_skeleton(&mut self, id: SkeletonId) { self.skeletons[id.0 as usize] = None; }
    pub fn has_skeleton(&self, id: SkeletonId) -> bool { self.skeletons.get(id.0 as usize).is_some_and(Option::is_some) }
    pub fn skeleton(&self, id: SkeletonId) -> Option<&Skeleton> { self.skeletons.get(id.0 as usize)?.as_ref() }
    /// three の Skeleton.update（骨の世界 × boneInverse）。update_world のあとの値を読む
    pub fn bone_matrices(&self, id: SkeletonId, out: &mut Vec<Mat4>) {
        out.clear();
        if let Some(s) = self.skeleton(id) { out.extend(s.bones.iter().zip(&s.inverses).map(|(b, i)| self.nodes[b.0 as usize].world * *i)); }
    }
    /// three の Points（位置と属性は動的。set_point_positions・write_point_attr・set_point_count）
    pub fn points(&mut self, parent: Option<NodeId>, capacity: usize, mat: MatId) -> NodeId {
        self.add(parent, NodeKind::Points(PointSet { mat, capacity, count: 0, positions: Vec::new(), attrs: Vec::new(), pos_dirty: false, attr_dirty: Vec::new(), bounds: None }))
    }
    pub fn sprite(&mut self, parent: Option<NodeId>, mat: MatId) -> NodeId { self.add(parent, NodeKind::Sprite { mat, center: Vec2::splat(0.5) }) }
    /// three の LineSegments（capacity は頂点の数。set_line_positions は 2 つで 1 本）
    pub fn lines(&mut self, parent: Option<NodeId>, capacity: usize, mat: MatId) -> NodeId {
        self.add(parent, NodeKind::Lines(LineSet { mat, capacity, positions: Vec::new(), dirty: false, bounds: None }))
    }
    /// three の DirectionalLight（既定の位置は (0, 1, 0)、向く先は原点）
    pub fn dir_light(&mut self, parent: Option<NodeId>, color: Color, intensity: f32) -> NodeId {
        let id = self.add(parent, NodeKind::Dir(DirLight { color, intensity, target: Target::Pos(Vec3::ZERO), shadow: Shadow::default() }));
        self[id].position = Vec3::Y;
        id
    }
    /// three の HemisphereLight（既定の位置は (0, 1, 0)。位置の向きが「上」）
    pub fn hemi_light(&mut self, parent: Option<NodeId>, sky: Color, ground: Color, intensity: f32) -> NodeId {
        let id = self.add(parent, NodeKind::Hemi(HemiLight { sky, ground, intensity }));
        self[id].position = Vec3::Y;
        id
    }
    pub fn ambient_light(&mut self, parent: Option<NodeId>, color: Color, intensity: f32) -> NodeId { self.add(parent, NodeKind::Ambient(AmbientLight { color, intensity })) }
    /// three の PointLight(color, intensity, distance, decay)
    pub fn point_light(&mut self, parent: Option<NodeId>, color: Color, intensity: f32, distance: f32, decay: f32) -> NodeId {
        self.add(parent, NodeKind::Point(PointLight { color, intensity, distance, decay }))
    }
    /// three の SpotLight(color, intensity, distance, angle, penumbra, decay)
    pub fn spot_light(&mut self, parent: Option<NodeId>, color: Color, intensity: f32, distance: f32, angle: f32, penumbra: f32, decay: f32) -> NodeId {
        let id = self.add(parent, NodeKind::Spot(SpotLight { color, intensity, distance, angle, penumbra, decay, target: Target::Pos(Vec3::ZERO) }));
        self[id].position = Vec3::Y;
        id
    }
    /// 主役の入れ物。kit.set_actor で主役を入れる
    pub fn actor(&mut self, parent: Option<NodeId>) -> NodeId {
        let id = self.add(parent, NodeKind::Actor);
        self[id].cast_shadow = true;
        id
    }

    pub fn is_alive(&self, id: NodeId) -> bool { self.nodes.get(id.0 as usize).is_some_and(|n| n.alive) }
    pub(crate) fn uid(&self, id: NodeId) -> u32 { self.nodes[id.0 as usize].uid }

    /// three の parent.add(child)（前の親から外す）
    pub fn set_parent(&mut self, id: NodeId, parent: Option<NodeId>) {
        if let Some(old) = self[id].parent { self[old].children.retain(|c| *c != id); }
        let p = parent.unwrap_or(self.root);
        self[id].parent = Some(p);
        self[p].children.push(id);
        self[id].cached.0 = Vec3::NAN;
    }

    /// 外す（子ごと）
    pub fn remove(&mut self, id: NodeId) {
        if id == self.root { return; }
        if let Some(p) = self[id].parent { self[p].children.retain(|c| *c != id); }
        self.kill(id);
    }
    fn kill(&mut self, id: NodeId) {
        let children = std::mem::take(&mut self[id].children);
        for c in children { self.kill(c); }
        let n = &mut self[id];
        n.alive = false;
        n.kind = NodeKind::Object;
        n.parent = None;
        self.free.push(id.0);
    }
    pub fn clear_children(&mut self, id: NodeId) {
        let children = std::mem::take(&mut self[id].children);
        for c in children { self.kill(c); }
    }

    /// three の clone(true)（形と材質は共有。主役は入れ物だけ写す）
    pub fn clone_subtree(&mut self, id: NodeId, parent: Option<NodeId>) -> NodeId {
        let src = self[id].clone();
        let new_id = self.add(parent, src.kind.clone());
        {
            let n = &mut self[new_id];
            n.position = src.position; n.rotation = src.rotation; n.scale = src.scale; n.visible = src.visible; n.render_order = src.render_order; n.layers = src.layers;
            n.cast_shadow = src.cast_shadow; n.receive_shadow = src.receive_shadow; n.frustum_culled = src.frustum_culled; n.name = src.name.clone(); n.up = src.up; n.matrix = src.matrix;
        }
        for c in &src.children { self.clone_subtree(*c, Some(new_id)); }
        new_id
    }

    /// three の getObjectByName（自分を含む）
    pub fn get_by_name(&self, root: NodeId, name: &str) -> Option<NodeId> {
        if self[root].name == name { return Some(root); }
        self[root].children.iter().find_map(|c| self.get_by_name(*c, name))
    }

    /// three の traverse（自分を含む。深さ優先の前順）
    pub fn traverse(&mut self, root: NodeId, f: &mut dyn FnMut(&mut Scene, NodeId)) {
        f(self, root);
        let children = self[root].children.clone();
        for c in children { self.traverse(c, f); }
    }

    /// その節点の世界の行列（今の値から親をたどって計算する。visible に関係なし）
    pub fn world_matrix(&self, id: NodeId) -> Mat4 {
        let n = &self[id];
        match n.parent {
            Some(p) => self.world_matrix(p) * n.local(),
            None => n.local(),
        }
    }
    pub fn world_position(&self, id: NodeId) -> Vec3 { self.world_matrix(id).w_axis.truncate() }

    /// three の Object3D.lookAt（光は -Z、ほかは +Z を向ける）
    pub fn look_at(&mut self, id: NodeId, target: Vec3) {
        let pos = self.world_position(id);
        let n = &self[id];
        let m = match n.kind {
            NodeKind::Dir(_) | NodeKind::Spot(_) | NodeKind::Hemi(_) | NodeKind::Point(_) | NodeKind::Ambient(_) => look_at_rotation(pos, target, n.up),
            _ => look_at_rotation(target, pos, n.up),
        };
        let mut q = quat_of(m);
        if let Some(p) = n.parent {
            let pw = self.world_matrix(p);
            let (_, pr, _) = pw.to_scale_rotation_translation();
            q = pr.inverse() * q;
        }
        self[id].rotation = Rot::Quat(q);
    }

    // ---- インスタンス ----

    /// InstancedMesh.setMatrixAt
    pub fn set_instance(&mut self, id: NodeId, i: usize, m: Mat4) {
        if let NodeKind::Instanced(s) = &mut self[id].kind && i < s.capacity { s.matrices[i] = m; s.dirty = true; }
    }
    /// InstancedMesh.getMatrixAt
    pub fn instance(&self, id: NodeId, i: usize) -> Mat4 {
        if let NodeKind::Instanced(s) = &self[id].kind { s.matrices.get(i).copied().unwrap_or(Mat4::IDENTITY) } else { Mat4::IDENTITY }
    }
    /// InstancedMesh.setColorAt（最初に呼ぶと全部を白で作る。three と同じ）
    pub fn set_instance_color(&mut self, id: NodeId, i: usize, c: Color) {
        if let NodeKind::Instanced(s) = &mut self[id].kind && i < s.capacity {
            let cap = s.capacity;
            s.colors.get_or_insert_with(|| vec![[1.0; 3]; cap])[i] = c.to_array();
            s.dirty = true;
        }
    }
    /// InstancedMesh.count
    pub fn set_instance_count(&mut self, id: NodeId, n: usize) {
        if let NodeKind::Instanced(s) = &mut self[id].kind { let n = n.min(s.capacity); if n > s.count { s.dirty = true; } s.count = n; }
    }
    /// 独自の属性（material の instance_attrs の attr_index 番目）。値は f32 の列。描く数の分だけ GPU へ送る
    pub fn write_instance_attr(&mut self, id: NodeId, attr_index: usize, data: &[f32]) {
        if let NodeKind::Instanced(s) = &mut self[id].kind {
            while s.attrs.len() <= attr_index { s.attrs.push(Vec::new()); s.attr_dirty.push(false); }
            s.attrs[attr_index].clear();
            s.attrs[attr_index].extend_from_slice(data);
            s.attr_dirty[attr_index] = true;
        }
    }
    /// InstancedMesh.computeBoundingSphere（作り直す）
    pub fn reset_instance_bounds(&mut self, id: NodeId) {
        if let NodeKind::Instanced(s) = &mut self[id].kind { s.bounds = None; }
    }

    // ---- BatchedMesh ----

    /// addGeometry。返すのは形の番号
    pub fn batch_add_shape(&mut self, id: NodeId, geo: GeoId) -> u32 {
        if let NodeKind::Batched(b) = &mut self[id].kind { b.shapes.push(geo); return b.shapes.len() as u32 - 1; }
        0
    }
    /// addInstance（行列は単位行列、色は白、描く印あり。three と同じ）。返すのはインスタンスの番号
    pub fn batch_add_instance(&mut self, id: NodeId, shape: u32) -> usize {
        if let NodeKind::Batched(b) = &mut self[id].kind { b.insts.push(BatchInst { shape, visible: true, matrix: Mat4::IDENTITY, color: [1.0; 4] }); return b.insts.len() - 1; }
        0
    }
    /// setMatrixAt
    pub fn set_batch_matrix(&mut self, id: NodeId, i: usize, m: Mat4) {
        if let NodeKind::Batched(b) = &mut self[id].kind && let Some(k) = b.insts.get_mut(i) { k.matrix = m; }
    }
    /// setGeometryIdAt
    pub fn set_batch_shape(&mut self, id: NodeId, i: usize, shape: u32) {
        if let NodeKind::Batched(b) = &mut self[id].kind && let Some(k) = b.insts.get_mut(i) { k.shape = shape; }
    }
    /// setVisibleAt
    pub fn set_batch_visible(&mut self, id: NodeId, i: usize, visible: bool) {
        if let NodeKind::Batched(b) = &mut self[id].kind && let Some(k) = b.insts.get_mut(i) { k.visible = visible; }
    }
    /// setColorAt（4 つの値。Custom の材質は i.fx でそのまま読む。組み込みの材質は rgb だけ掛ける）
    pub fn set_batch_color(&mut self, id: NodeId, i: usize, c: [f32; 4]) {
        if let NodeKind::Batched(b) = &mut self[id].kind && let Some(k) = b.insts.get_mut(i) { k.color = c; }
    }

    // ---- Points・線 ----

    /// 位置（x, y, z の列）
    pub fn set_point_positions(&mut self, id: NodeId, data: &[f32]) {
        if let NodeKind::Points(s) = &mut self[id].kind { s.positions.clear(); s.positions.extend_from_slice(data); s.pos_dirty = true; }
    }
    /// geo.setDrawRange(0, n)
    pub fn set_point_count(&mut self, id: NodeId, n: usize) {
        if let NodeKind::Points(s) = &mut self[id].kind { s.count = n.min(s.capacity); }
    }
    /// 独自の属性（material の point_attrs の attr_index 番目）
    pub fn write_point_attr(&mut self, id: NodeId, attr_index: usize, data: &[f32]) {
        if let NodeKind::Points(s) = &mut self[id].kind {
            while s.attrs.len() <= attr_index { s.attrs.push(Vec::new()); s.attr_dirty.push(false); }
            s.attrs[attr_index].clear();
            s.attrs[attr_index].extend_from_slice(data);
            s.attr_dirty[attr_index] = true;
        }
    }
    /// 頂点の位置（x, y, z の列。2 つで 1 本）
    pub fn set_line_positions(&mut self, id: NodeId, data: &[f32]) {
        // 包む球も作り直す（視錐台の間引き。前は最初の頂点の球のままで、動く線が画面の中でも消えた）
        if let NodeKind::Lines(s) = &mut self[id].kind { s.positions.clear(); s.positions.extend_from_slice(data); s.dirty = true; s.bounds = None; }
    }

    // ---- 値の手短な入口 ----

    pub fn set_material(&mut self, id: NodeId, mat: MatId) {
        match &mut self[id].kind {
            NodeKind::Mesh { mats, multi, .. } => { *mats = vec![mat]; *multi = false; }
            NodeKind::Skinned(s) => s.mat = mat,
            NodeKind::Instanced(s) => s.mat = mat,
            NodeKind::Points(s) => s.mat = mat,
            NodeKind::Lines(s) => s.mat = mat,
            NodeKind::Sprite { mat: m, .. } => *m = mat,
            _ => {}
        }
    }
    pub fn set_geometry(&mut self, id: NodeId, g: GeoId) {
        match &mut self[id].kind {
            NodeKind::Mesh { geo, .. } => *geo = g,
            NodeKind::Skinned(s) => s.geo = g,
            NodeKind::Instanced(s) => s.geo = g,
            _ => {}
        }
    }
    /// sprite.center
    pub fn set_sprite_center(&mut self, id: NodeId, c: Vec2) {
        if let NodeKind::Sprite { center, .. } = &mut self[id].kind { *center = c; }
    }
    pub fn dir_light_mut(&mut self, id: NodeId) -> Option<&mut DirLight> { if let NodeKind::Dir(l) = &mut self[id].kind { Some(l) } else { None } }
    pub fn hemi_light_mut(&mut self, id: NodeId) -> Option<&mut HemiLight> { if let NodeKind::Hemi(l) = &mut self[id].kind { Some(l) } else { None } }
    pub fn ambient_light_mut(&mut self, id: NodeId) -> Option<&mut AmbientLight> { if let NodeKind::Ambient(l) = &mut self[id].kind { Some(l) } else { None } }
    pub fn point_light_mut(&mut self, id: NodeId) -> Option<&mut PointLight> { if let NodeKind::Point(l) = &mut self[id].kind { Some(l) } else { None } }
    pub fn spot_light_mut(&mut self, id: NodeId) -> Option<&mut SpotLight> { if let NodeKind::Spot(l) = &mut self[id].kind { Some(l) } else { None } }

    pub(crate) fn target_position(&self, t: Target) -> Vec3 {
        match t { Target::Pos(p) => p, Target::Node(n) => if self.is_alive(n) { self.world_position(n) } else { Vec3::ZERO } }
    }

    /// 世界の行列を親から計算する（変わった所だけ。visible に関係なし）。描く前に kit が呼ぶ
    pub fn update_world(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        let frame = self.frame;
        let mut stack: Vec<(u32, Mat4, bool)> = vec![(self.root.0, Mat4::IDENTITY, false)];
        while let Some((i, pw, pc)) = stack.pop() {
            let n = &mut self.nodes[i as usize];
            let same = n.cached.0 == n.position && n.cached.1 == n.rotation && n.cached.2 == n.scale && n.cached.3 == n.matrix;
            let changed = pc || !same;
            if changed {
                n.cached = (n.position, n.rotation, n.scale, n.matrix);
                n.world = pw * n.local();
                n.world_gen = frame;
            }
            let w = n.world;
            for c in &self.nodes[i as usize].children { stack.push((c.0, w, changed)); }
        }
    }
    pub(crate) fn nodes(&self) -> &[Node] { &self.nodes }
    pub(crate) fn node_mut(&mut self, i: u32) -> &mut Node { &mut self.nodes[i as usize] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::material::{GeoId, MatId};

    fn close(a: Vec3, b: Vec3) -> bool { (a - b).length() < 1e-4 }

    /// 防ぐ故障: 頂点を書き換えた線が、最初の頂点の包む球のまま間引かれて画面の中で消える（aisns のボスの予兆の線）
    #[test]
    fn line_positions_reset_bounds() {
        let mut s = Scene::new();
        let l = s.lines(None, 4, MatId(0));
        s.set_line_positions(l, &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        if let NodeKind::Lines(ls) = &mut s[l].kind { ls.bounds = Some((Vec3::ZERO, 1.0)); }
        s.set_line_positions(l, &[50.0, 0.0, 0.0, 51.0, 0.0, 0.0]);
        let NodeKind::Lines(ls) = &s[l].kind else { panic!() };
        assert!(ls.bounds.is_none());
    }

    /// 親の回転と大きさが子へ伝わる
    #[test]
    fn world_follows_parents() {
        let mut s = Scene::new();
        let a = s.group(None);
        s[a].position = Vec3::new(1.0, 2.0, 3.0);
        s[a].rotation = Rot::y(std::f32::consts::FRAC_PI_2);
        s[a].scale = Vec3::splat(2.0);
        let b = s.object(Some(a));
        s[b].position = Vec3::new(1.0, 0.0, 0.0);
        assert!(close(s.world_position(b), Vec3::new(1.0, 2.0, 1.0)));
        s.update_world();
        assert!(close(s[b].world.w_axis.truncate(), Vec3::new(1.0, 2.0, 1.0)));
        // 親が隠れていても世界の行列は計算する
        s[a].visible = false;
        s[a].position.x = 5.0;
        s.update_world();
        assert!(close(s[b].world.w_axis.truncate(), Vec3::new(5.0, 2.0, 1.0)));
    }

    /// カメラは -Z、物は +Z を向ける（three と同じ）
    #[test]
    fn look_at_matches_three() {
        let mut s = Scene::new();
        let o = s.object(None);
        s.look_at(o, Vec3::new(5.0, 0.0, 0.0));
        let q = s[o].rotation.to_quat();
        assert!(close(q * Vec3::Z, Vec3::X));
        let mut c = Camera::new(34.0, 1.0, 0.3, 900.0);
        c.position = Vec3::new(0.0, 0.0, 10.0);
        c.look_at(Vec3::ZERO);
        assert!(close(c.forward(), Vec3::NEG_Z));
        // 真上から見下ろしても破綻しない（three は z に 0.0001 足す）
        c.position = Vec3::new(0.0, 10.0, 0.0);
        c.look_at(Vec3::ZERO);
        assert!((c.forward() - Vec3::NEG_Y).length() < 1e-3);
    }

    /// three の project: 画面の中心は 0、near は -1、far は 1
    #[test]
    fn project_matches_three() {
        let mut c = Camera::new(90.0, 2.0, 1.0, 100.0);
        c.position = Vec3::new(0.0, 0.0, 10.0);
        c.look_at(Vec3::ZERO);
        assert!(close(c.project(Vec3::new(0.0, 0.0, 9.0)), Vec3::new(0.0, 0.0, -1.0)));
        assert!(close(c.project(Vec3::new(0.0, 0.0, -90.0)), Vec3::new(0.0, 0.0, 1.0)));
        let p = c.project(Vec3::new(1.0, 1.0, 9.0));
        assert!((p.x - 0.5).abs() < 1e-4 && (p.y - 1.0).abs() < 1e-4);
    }

    /// 防ぐ故障: 骨の行列の掛け順の間違い（骨の世界 × boneInverse。three の Skeleton.update）・同じ形の 2 体が同じ骨を読んで同じ動きになる（SkeletonUtils.clone の役）
    #[test]
    fn bone_matrices_are_per_skeleton() {
        let mut s = Scene::new();
        let holder = s.group(None);
        s[holder].position = Vec3::new(10.0, 0.0, 0.0);
        let inverse = Mat4::from_translation(Vec3::new(0.0, -1.0, 0.0));
        let mut one = || {
            let b = s.object(Some(holder));
            s[b].position = Vec3::new(0.0, 1.0, 0.0);
            let sk = s.new_skeleton(vec![b], vec![inverse]);
            (b, sk, s.skinned(Some(holder), GeoId(0), MatId(0), sk))
        };
        let (bone_a, a, mesh_a) = one();
        let (_, b, _) = one();
        s[bone_a].rotation = Rot::y(std::f32::consts::FRAC_PI_2);
        s.update_world();
        let (mut pa, mut pb) = (Vec::new(), Vec::new());
        s.bone_matrices(a, &mut pa);
        s.bone_matrices(b, &mut pb);
        // 頂点 (1, 1, 0) は骨の位置から +X に 1。骨が Y に 90 度回ると -Z へ、入れ物の分 +X に 10
        assert!(close(pa[0].transform_point3(Vec3::new(1.0, 1.0, 0.0)), Vec3::new(10.0, 1.0, -1.0)));
        // もう 1 体は動かしていない
        assert!(close(pb[0].transform_point3(Vec3::new(1.0, 1.0, 0.0)), Vec3::new(11.0, 1.0, 0.0)));
        // 消した骨の組は読めない（kit が GPU の分も片付ける）
        assert!(s.has_skeleton(a));
        s.remove_skeleton(a);
        assert!(!s.has_skeleton(a));
        s.remove(mesh_a);
    }

    #[test]
    fn remove_and_clone() {
        let mut s = Scene::new();
        let a = s.group(None);
        let b = s.sprite(Some(a), MatId(3));
        s[b].name = "glow".into();
        let c = s.clone_subtree(a, None);
        let g = s.get_by_name(c, "glow").unwrap();
        assert_ne!(g, b);
        assert!(matches!(s[g].kind, NodeKind::Sprite { mat: MatId(3), .. }));
        s.remove(a);
        assert!(!s.is_alive(a) && !s.is_alive(b) && s.is_alive(g));
        s.clear_children(c);
        assert!(s[c].children().is_empty());
    }
}
