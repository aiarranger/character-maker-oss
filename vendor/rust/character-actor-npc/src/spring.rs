// 凍結: character-actor 0.1.14（2026-11-12）の src/spring.rs の写し。NPC・汎用（character-runtime 0.2.5 の写しの決まり）だけが使う。主役の crate の直しをここへ持ち込まない（Owner 2026-11-12「種類ごとに分ける」）
// 揺れ物（髪・服のすそ・尾などの骨を、体の動きに遅れて揺らす）。@pixiv/three-vrm-springbone 3.5.5 の VRMSpringBoneLoaderPlugin（VRMC_springBone）・VRMSpringBoneManager・
// VRMSpringBoneJoint と、character-runtime の createCharacterSpringMotion の写し。計算の順番も同じ。TS と同じく、待機・歩き・走りと顔のあと、重ねる動き（攻撃・のけぞりなど）の前に動く。
// 配布の lite・normal には揺れ物が入っていない（2026-11-05 の配布で、full だけ 5 人）。入っていないモデルでは何もしない。
// VRM 0.x の揺れ物（extensions.VRM.secondaryAnimation）は配布に無いので写していない。
use crate::anim::{Skeleton, three_decompose};
use crate::glb::Glb;
use glam::{DVec3, Mat3, Mat4, Quat, Vec3};
use serde_json::Value;

/// 1 秒に 120 回（character-runtime の spring.ts の STEP）
const STEP: f64 = 1.0 / 120.0;

#[derive(Clone, Copy)]
enum Shape {
    Sphere { radius: f32, inside: bool },
    Capsule { tail: Vec3, radius: f32, inside: bool },
    Plane { normal: Vec3 },
}

/// 節点に付いた当たりの形（節点の世界の行列の位置を offset の分ずらした行列で測る）
#[derive(Clone, Copy)]
struct Collider { node: usize, offset: Vec3, shape: Shape }

impl Collider {
    /// VRMSpringBoneColliderShape*.calculateCollision。戻すのは (距離, 押し出す向き)
    fn collide(&self, world: &Mat4, tail: Vec3, hit: f32) -> (f32, Vec3) {
        let pos = world.transform_point3(self.offset);
        let out = |distance: f32, target: Vec3, length: f32, inside: bool| {
            if distance < 0.0 {
                let t = target * (1.0 / length);
                (distance, if inside { -t } else { t })
            } else { (distance, target) }
        };
        match self.shape {
            Shape::Sphere { radius, inside } => {
                let target = tail - pos;
                let length = target.length();
                out(if inside { radius - hit - length } else { length - hit - radius }, target, length, inside)
            }
            Shape::Capsule { tail: end, radius, inside } => {
                // three は行列の位置を offset の分ずらしてから (tail - offset) を掛ける
                let mut m = *world;
                m.w_axis = pos.extend(1.0);
                let mut b = m.transform_point3(end - self.offset) - pos;
                let len_sq = b.length_squared();
                let mut target = tail - pos;
                let dot = b.dot(target);
                if dot <= 0.0 {
                } else if len_sq <= dot {
                    target -= b;
                } else {
                    b *= dot / len_sq;
                    target -= b;
                }
                let length = target.length();
                out(if inside { radius - hit - length } else { length - hit - radius }, target, length, inside)
            }
            Shape::Plane { normal } => {
                let n = norm(Mat3::from_mat4(*world).inverse().transpose() * normal);
                ((tail - pos).dot(n) - hit, n)
            }
        }
    }
}

/// VRMSpringBoneJoint
#[derive(Clone)]
struct Joint {
    bone: usize,
    child: Option<usize>,
    hit_radius: f32,
    stiffness: f32,
    gravity_power: f32,
    gravity_dir: Vec3,
    drag: f32,
    colliders: Vec<usize>,
    center: Option<usize>,
    initial_local: Mat4,
    initial_rotation: Quat,
    initial_child: Vec3,
    bone_axis: Vec3,
    tail: Vec3,
    prev_tail: Vec3,
    /// この骨を動かしたあとに世界の行列を作り直す節点（下の節点。ほかの揺れ物の骨の所で止まる。親が先）
    below: Vec<usize>,
}

/// three の Vector3.normalize（長さ 0 はそのまま）
#[inline]
fn norm(v: Vec3) -> Vec3 { let l = v.length(); if l == 0.0 { v } else { v / l } }

/// three の Quaternion.setFromUnitVectors
fn from_unit_vectors(a: Vec3, b: Vec3) -> Quat {
    let r = a.dot(b) + 1.0;
    let q = if r < f32::EPSILON {
        if a.x.abs() > a.z.abs() { Quat::from_xyzw(-a.y, a.x, 0.0, 0.0) } else { Quat::from_xyzw(0.0, -a.z, a.y, 0.0) }
    } else {
        let c = a.cross(b);
        Quat::from_xyzw(c.x, c.y, c.z, r)
    };
    q.normalize()
}

/// three の Quaternion.slerp（倍精度。t は 0〜1）
fn slerp(a: Quat, b: Quat, t: f64) -> Quat {
    if t == 0.0 { return a; }
    if t == 1.0 { return b; }
    let (a, b) = (a.as_dquat(), b.as_dquat());
    let mut cos = a.dot(b);
    let b = if cos < 0.0 { cos = -cos; -b } else { b };
    if cos >= 1.0 { return a.as_quat(); }
    let sqr_sin = 1.0 - cos * cos;
    if sqr_sin <= f64::EPSILON {
        let s = 1.0 - t;
        return (a * s + b * t).normalize().as_quat();
    }
    let sin = sqr_sin.sqrt();
    let half = sin.atan2(cos);
    let (ra, rb) = (((1.0 - t) * half).sin() / sin, (t * half).sin() / sin);
    (a * ra + b * rb).as_quat()
}

#[inline]
fn lerp(a: Vec3, b: Vec3, t: f64) -> Vec3 { let (a, b): (DVec3, DVec3) = (a.as_dvec3(), b.as_dvec3()); (a + (b - a) * t).as_vec3() }

/// 1 体の揺れ物（VRMSpringBoneManager と createCharacterSpringMotion）
#[derive(Clone)]
pub struct SpringMotion {
    joints: Vec<Joint>,
    colliders: Vec<Collider>,
    initialized: bool,
    remainder: f64,
    previous: Mat4,
    /// 計算の原点（世界の座標）
    origin: DVec3,
}

fn v3(v: Option<&Value>) -> Option<Vec3> {
    let a = v?.as_array()?;
    Some(Vec3::new(a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32))
}
fn num(v: &Value, k: &str) -> Option<f32> { v.get(k).and_then(Value::as_f64).map(|x| x as f32) }

fn read_collider(c: &Value, node_count: usize) -> Option<Collider> {
    let node = c.get("node")?.as_u64()? as usize;
    if node >= node_count { return None; }
    let zero = |v: Option<&Value>| v3(v).unwrap_or(Vec3::ZERO);
    // VRMC_springBone_extended_collider（版 1.0 だけ）を先に見る
    if let Some(ex) = c.get("extensions").and_then(|e| e.get("VRMC_springBone_extended_collider")).filter(|e| e.get("specVersion").and_then(Value::as_str) == Some("1.0")) {
        let shape = &ex["shape"];
        if let Some(s) = shape.get("sphere") {
            return Some(Collider { node, offset: zero(s.get("offset")), shape: Shape::Sphere { radius: num(s, "radius").unwrap_or(0.0), inside: s.get("inside").and_then(Value::as_bool).unwrap_or(false) } });
        }
        if let Some(s) = shape.get("capsule") {
            return Some(Collider { node, offset: zero(s.get("offset")), shape: Shape::Capsule { tail: zero(s.get("tail")), radius: num(s, "radius").unwrap_or(0.0), inside: s.get("inside").and_then(Value::as_bool).unwrap_or(false) } });
        }
        if let Some(s) = shape.get("plane") {
            return Some(Collider { node, offset: zero(s.get("offset")), shape: Shape::Plane { normal: v3(s.get("normal")).unwrap_or(Vec3::Z) } });
        }
    }
    let shape = c.get("shape")?;
    if let Some(s) = shape.get("sphere") {
        return Some(Collider { node, offset: zero(s.get("offset")), shape: Shape::Sphere { radius: num(s, "radius").unwrap_or(0.0), inside: false } });
    }
    if let Some(s) = shape.get("capsule") {
        return Some(Collider { node, offset: zero(s.get("offset")), shape: Shape::Capsule { tail: zero(s.get("tail")), radius: num(s, "radius").unwrap_or(0.0), inside: false } });
    }
    None
}

/// VRMSpringBoneManager._insertJointSort: 親と当たりの節点より上にある揺れ物の骨を先に動かす並び
fn insert_sorted(j: usize, joints: &[Joint], colliders: &[Collider], of_node: &[Vec<usize>], parent: &[Option<usize>], tried: &mut [bool], done: &mut [bool], order: &mut Vec<usize>) {
    if done[j] || tried[j] { return; }
    tried[j] = true;
    let mut deps: Vec<Option<usize>> = vec![parent[joints[j].bone]];
    for &c in &joints[j].colliders { deps.push(Some(colliders[c].node)); }
    for dep in deps {
        // 根から dep まで
        let mut path = Vec::new();
        let mut at = dep;
        while let Some(i) = at { path.push(i); at = parent[i]; }
        for &a in path.iter().rev() {
            for &other in &of_node[a] { insert_sorted(other, joints, colliders, of_node, parent, tried, done, order); }
        }
    }
    order.push(j);
    done[j] = true;
}

impl SpringMotion {
    /// VRMSpringBoneLoaderPlugin._v1Import と setInitState。揺れ物が無いモデルは None。揺れる骨は局所の行列を固定にする（three の matrixAutoUpdate = false）
    pub fn from_glb(g: &Glb, skel: &mut Skeleton) -> Option<SpringMotion> {
        let ext = g.json.get("extensions")?.get("VRMC_springBone")?;
        if !matches!(ext.get("specVersion").and_then(Value::as_str), Some("1.0") | Some("1.0-beta")) { return None; }
        let n = g.nodes.len();
        // 読めない当たりは飛ばす。番号がずれないように、読めた物の番号を控える
        let mut colliders: Vec<Collider> = Vec::new();
        let collider_index: Vec<Option<usize>> = ext.get("colliders").and_then(Value::as_array).map(|a| a.iter().map(|c| read_collider(c, n).map(|x| { colliders.push(x); colliders.len() - 1 })).collect()).unwrap_or_default();
        let groups: Vec<Vec<usize>> = ext.get("colliderGroups").and_then(Value::as_array).map(|a| a.iter().map(|g| {
            g.get("colliders").and_then(Value::as_array).map(|l| l.iter().filter_map(|c| collider_index.get(c.as_u64()? as usize).copied().flatten()).collect()).unwrap_or_default()
        }).collect()).unwrap_or_default();
        let mut joints: Vec<Joint> = Vec::new();
        for spring in ext.get("springs").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]) {
            let of_spring: Vec<usize> = spring.get("colliderGroups").and_then(Value::as_array).map(|l| l.iter().filter_map(|g| groups.get(g.as_u64()? as usize)).flatten().copied().collect()).unwrap_or_default();
            let center = spring.get("center").and_then(Value::as_u64).map(|c| c as usize).filter(|c| *c < n);
            let list = spring.get("joints").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
            for pair in list.windows(2) {
                let (Some(bone), Some(child)) = (pair[0].get("node").and_then(Value::as_u64), pair[1].get("node").and_then(Value::as_u64)) else { continue };
                let (bone, child) = (bone as usize, child as usize);
                if bone >= n || child >= n { continue; }
                let s = &pair[0];
                let (rotation, initial_child) = (skel.r[bone], skel.t[child]);
                joints.push(Joint {
                    bone, child: Some(child),
                    hit_radius: num(s, "hitRadius").unwrap_or(0.0),
                    stiffness: num(s, "stiffness").unwrap_or(1.0),
                    gravity_power: num(s, "gravityPower").unwrap_or(0.0),
                    gravity_dir: v3(s.get("gravityDir")).unwrap_or(Vec3::NEG_Y),
                    drag: num(s, "dragForce").unwrap_or(0.4),
                    colliders: of_spring.clone(), center,
                    initial_local: Mat4::from_scale_rotation_translation(skel.s[bone], rotation, skel.t[bone]),
                    initial_rotation: rotation, initial_child, bone_axis: norm(initial_child),
                    tail: Vec3::ZERO, prev_tail: Vec3::ZERO, below: Vec::new(),
                });
            }
        }
        if joints.is_empty() { return None; }
        // 動かす並び（TS は読むときに 1 回決める）
        let mut of_node = vec![Vec::new(); n];
        for (i, j) in joints.iter().enumerate() { of_node[j.bone].push(i); }
        let (mut tried, mut done, mut order) = (vec![false; joints.len()], vec![false; joints.len()], Vec::with_capacity(joints.len()));
        for j in 0..joints.len() { insert_sorted(j, &joints, &colliders, &of_node, &skel.parent, &mut tried, &mut done, &mut order); }
        let mut slots: Vec<Option<Joint>> = joints.into_iter().map(Some).collect();
        let mut sorted: Vec<Joint> = order.iter().map(|&i| slots[i].take().unwrap()).collect();
        // traverseChildrenUntilConditionMet(bone, _relevantChildrenUpdated)
        for j in &mut sorted {
            fn go(i: usize, skel: &Skeleton, of_node: &[Vec<usize>], out: &mut Vec<usize>) {
                for &c in &skel.children[i] {
                    if !of_node[c].is_empty() { continue; }
                    out.push(c);
                    go(c, skel, of_node, out);
                }
            }
            go(j.bone, skel, &of_node, &mut j.below);
        }
        for j in &sorted { skel.fixed[j.bone] = Some(j.initial_local); }
        Some(SpringMotion { joints: sorted, colliders, initialized: false, remainder: 0.0, previous: Mat4::IDENTITY, origin: DVec3::ZERO })
    }

    /// 揺れる骨の節点の番号（動かす順）
    pub fn joint_nodes(&self) -> Vec<usize> { self.joints.iter().map(|j| j.bone).collect() }
    pub fn collider_count(&self) -> usize { self.colliders.len() }

    /// bone の親の世界の行列（親が無ければ根）
    #[inline]
    fn parent_world(skel: &Skeleton, bone: usize, root: Mat4) -> Mat4 { match skel.parent[bone] { Some(p) => skel.world[p], None => root } }

    /// 根から節点まで世界の行列を作り直す（three の updateWorldMatrix(true, false)）
    fn update_chain(skel: &mut Skeleton, node: usize, root: Mat4) {
        let mut path = Vec::new();
        let mut at = Some(node);
        while let Some(i) = at { path.push(i); at = skel.parent[i]; }
        for &i in path.iter().rev() {
            let l = skel.local(i);
            skel.world[i] = match skel.parent[i] { Some(p) => skel.world[p] * l, None => root * l };
        }
    }

    /// VRMSpringBoneManager.reset: 骨を初めの向きへ戻し、尾の位置をそこから取り直す
    fn reset(&mut self, skel: &mut Skeleton, root: Mat4) {
        for j in &mut self.joints {
            let b = j.bone;
            skel.r[b] = j.initial_rotation;
            skel.fixed[b] = Some(Mat4::from_scale_rotation_translation(skel.s[b], skel.r[b], skel.t[b]));
            Self::update_chain(skel, b, root);
            let tail = skel.world[b].transform_point3(j.initial_child);
            j.tail = match j.center { Some(c) => skel.world[c].inverse().transform_point3(tail), None => tail };
            j.prev_tail = j.tail;
        }
    }

    /// VRMSpringBoneManager.update（skel.world は root で作り直してある）
    fn update(&mut self, skel: &mut Skeleton, root: Mat4, delta: f32) {
        for j in &mut self.joints {
            let b = j.bone;
            let local = Mat4::from_scale_rotation_translation(skel.s[b], skel.r[b], skel.t[b]);
            skel.fixed[b] = Some(local);
            let parent = Self::parent_world(skel, b, root);
            skel.world[b] = parent * local;
            // _calcWorldSpaceBoneLength
            let pos = skel.world[b].w_axis.truncate();
            let end = match j.child { Some(c) => skel.world[c].w_axis.truncate(), None => skel.world[b].transform_point3(j.initial_child) };
            let length = (pos - end).length();
            let axis = norm(parent.transform_vector3(norm(j.initial_local.transform_vector3(j.bone_axis))));
            let center = j.center.map(|c| skel.world[c]);
            let moved = j.tail + (j.tail - j.prev_tail) * (1.0 - j.drag);
            let mut next = match center { Some(m) => m.transform_point3(moved), None => moved };
            next += axis * (j.stiffness * delta);
            next += j.gravity_dir * (j.gravity_power * delta);
            next = norm(next - pos) * length + pos;
            // _collision
            for &c in &j.colliders {
                let col = &self.colliders[c];
                let (dist, away) = col.collide(&skel.world[col.node], next, j.hit_radius);
                if dist < 0.0 {
                    next += away * -dist;
                    next -= pos;
                    let l = next.length();
                    next = next * (length / l) + pos;
                }
            }
            j.prev_tail = j.tail;
            j.tail = match center { Some(m) => m.inverse().transform_point3(next), None => next };
            let to = norm((parent * j.initial_local).inverse().transform_point3(next));
            let q = j.initial_rotation * from_unit_vectors(j.bone_axis, to);
            skel.r[b] = q;
            let local = Mat4::from_scale_rotation_translation(skel.s[b], q, skel.t[b]);
            skel.fixed[b] = Some(local);
            skel.world[b] = parent * local;
            for k in 0..j.below.len() {
                let i = j.below[k];
                let l = skel.local(i);
                skel.world[i] = match skel.parent[i] { Some(p) => skel.world[p] * l, None => root * l };
            }
        }
    }

    /// createCharacterSpringMotion の advance。1 コマの間の入れ物の動きを、1/120 秒ごとの位置に分けて揺らす（余りは次のコマへ持ち越す）。
    /// current は入れ物の世界の行列、chain はその下の根までの行列（根の移動 × 大きさと向きをそろえる行列）。
    /// TS は倍精度で計算する。f32 の行列で遠い座標の丸めが揺れに溜まらないように、今の入れ物の位置を原点にして計算する（尾の位置だけ原点の差を足して持ち越す）。
    /// 呼び終えたあとの skel.world は計算の途中の行列が残る（呼ぶ側が作り直す）
    pub fn advance(&mut self, delta: f64, skel: &mut Skeleton, current: Mat4, chain: Mat4) { self.advance_frame(delta, skel, current, chain, false, false); }

    /// advance に createCharacterSpringMotion の frame（photoPose・resetToken）を足した物。reset は付け直しの番号か写真の姿が替わったコマ
    /// （骨を初めの向きへ戻して尾を取り直し、そのコマは揺らさない）。photo の間は揺らさずに入れ物の行列だけ控える
    pub fn advance_frame(&mut self, delta: f64, skel: &mut Skeleton, current: Mat4, chain: Mat4, photo: bool, reset: bool) {
        if reset { self.initialized = false; }
        if self.initialized && photo { self.previous = current; return; }
        if !self.initialized {
            let origin = current.w_axis.truncate();
            let root = Mat4::from_translation(-origin) * current * chain;
            skel.update_world(root);
            self.reset(skel, root);
            self.previous = current;
            self.origin = origin.as_dvec3();
            self.remainder = 0.0;
            self.initialized = true;
            return;
        }
        if delta == 0.0 { self.previous = current; return; }
        let (from_s, from_r, from_t) = three_decompose(self.previous);
        let (to_s, to_r, to_t) = three_decompose(current);
        let origin = to_t.as_dvec3();
        let moved = (self.origin - origin).as_vec3();
        for j in &mut self.joints { if j.center.is_none() { j.tail += moved; j.prev_tail += moved; } }
        self.origin = origin;
        let from_t = (from_t.as_dvec3() - origin).as_vec3();
        let total = self.remainder + delta;
        let steps = ((total + 1e-10) / STEP).floor() as usize;
        for i in 0..steps {
            let alpha = (((i + 1) as f64 * STEP - self.remainder) / delta).clamp(0.0, 1.0);
            let sample = Mat4::from_scale_rotation_translation(lerp(from_s, to_s, alpha), slerp(from_r, to_r, alpha), lerp(from_t, Vec3::ZERO, alpha));
            let root = sample * chain;
            skel.update_world(root);
            self.update(skel, root, STEP as f32);
        }
        self.remainder = (total - steps as f64 * STEP).max(0.0);
        self.previous = current;
    }
}
