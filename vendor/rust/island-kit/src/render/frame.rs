// 1 コマの流れ（three の WebGLRenderer.render と EffectComposer）: 節点を集める → 並べる → 影の型 → 自機の型 → 主の的 → 後処理。
use super::pipes::{Pass, VertKind};
use super::{Kit, Output, RenderOpts};
use crate::material::{MatId, TexId};
use crate::math::Color;
use crate::scene::{Camera, NodeId, NodeKind, RawView};
use bytemuck::{Pod, Zeroable};
use character_view::Globals;
use glam::{Mat4, Vec3, Vec4};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ItemKind { Mesh, Instanced, Sprite, Points, Lines, ActorOpaque, ActorTrans, Batched, BatchRun, Skinned }

/// 描く 1 つ（three の RenderItem）
#[derive(Clone, Copy, Debug)]
pub(crate) struct Item {
    pub node: u32,
    pub uid: u32,
    pub mat: u32,
    pub geo: u32,
    pub start: u32,
    pub count: u32,
    pub kind: ItemKind,
    pub group_order: i32,
    pub render_order: i32,
    pub z: f32,
    /// matrixWorld の行列式が負（裏返し。three の frontFaceCW）
    pub cw: bool,
    /// インスタンスの流れの位置と数
    pub first: u32,
    pub inst: u32,
    pub pipe: u32,
    pub bg: u32,
    /// 骨の組の番号（Skinned だけ）
    pub skel: u32,
}

/// 主役の不透明は、同じ renderOrder の中で材質の列の最後
const MAT_ACTOR: u32 = u32::MAX;

#[derive(Clone, Copy)]
pub(crate) struct ShadowInfo { vp: Mat4, planes: Frustum, map_size: u32, redraw: bool }

#[derive(Default)]
pub(crate) struct Lists {
    pub opaque: Vec<Item>,
    pub trans: Vec<Item>,
    pub mask: Vec<Item>,
    pub casters: Vec<Item>,
    pub actor_mask: Vec<NodeId>,
    pub actor_cast: Vec<NodeId>,
    pub shadows: Vec<ShadowInfo>,
    pub shadow_items: [Vec<Item>; 2],
    /// 1 コマのインスタンスの流れ
    pub stream: Vec<InstRaw>,
    /// 節点ごとの流れの位置（コマの番号, 位置）
    pub stream_at: Vec<(u32, u32)>,
    /// 主のカメラの視錐台（BatchedMesh のインスタンスごとの間引き）
    pub view_planes: Option<Frustum>,
    pub batch: super::batch::Scratch,
}

/// three の render のあと変わる GPU の物（節点ごと）
pub(crate) enum DynGpu {
    Inst { uid: u32, buf: wgpu::Buffer, attrs: Vec<wgpu::Buffer>, world_gen: u32, recv: bool },
    Points { uid: u32, pos: wgpu::Buffer, attrs: Vec<wgpu::Buffer>, uploaded: usize, uploaded_attr: Vec<usize> },
    Lines { uid: u32, pos: wgpu::Buffer, n: u32 },
}

/// インスタンスの流れの 1 つ（IIn と同じ並び）
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct InstRaw { pub m: [f32; 16], pub fx: [f32; 4] }

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShadowU { vp: [f32; 16], p: [f32; 4], q: [f32; 4] }

/// kit_common.wgsl の Lights と同じ並び
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct LightsU {
    ambient: [f32; 4],
    counts: [u32; 4],
    shadow_count: [u32; 4],
    hidden: [f32; 4],
    dim: [f32; 4],
    lamps: [[f32; 4]; 8],
    dir: [[[f32; 4]; 2]; 4],
    point: [[[f32; 4]; 2]; 16],
    spot: [[[f32; 4]; 4]; 2],
    hemi: [[[f32; 4]; 3]; 2],
    shadow: [ShadowU; 2],
}

/// 視錐台（three の Frustum.setFromProjectionMatrix。投影の形が 0..1 でも同じ面になる）
#[derive(Clone, Copy)]
pub(crate) struct Frustum([Vec4; 6]);
impl Frustum {
    pub fn new(vp: Mat4) -> Frustum {
        let (r0, r1, r2, r3) = (vp.row(0), vp.row(1), vp.row(2), vp.row(3));
        let n = |p: Vec4| p / p.truncate().length();
        Frustum([n(r3 + r0), n(r3 - r0), n(r3 + r1), n(r3 - r1), n(r3 - r2), n(r2)])
    }
    pub fn intersects(&self, c: Vec3, r: f32) -> bool {
        for p in &self.0 { if p.truncate().dot(c) + p.w < -r { return false; } }
        true
    }
}

pub(crate) fn max_scale(m: Mat4) -> f32 {
    m.x_axis.truncate().length_squared().max(m.y_axis.truncate().length_squared()).max(m.z_axis.truncate().length_squared()).sqrt()
}

/// three の computeBoundingSphere（箱の中心と、そこから最も遠い点）
fn bounds_of(p: &[f32]) -> (Vec3, f32) {
    let mut lo = Vec3::splat(f32::MAX);
    let mut hi = Vec3::splat(f32::MIN);
    for v in p.chunks_exact(3) { let a = Vec3::new(v[0], v[1], v[2]); lo = lo.min(a); hi = hi.max(a); }
    if p.len() < 3 { return (Vec3::ZERO, 0.0); }
    let c = (lo + hi) * 0.5;
    let mut r = 0.0f32;
    for v in p.chunks_exact(3) { r = r.max(Vec3::new(v[0], v[1], v[2]).distance_squared(c)); }
    (c, r.sqrt())
}

/// 前に描いた影の型をそのまま使うときの光の行列（three の shadow.matrix は描いたときだけ進む）。None なら今の光で描き直す。的を作り直したら中身が無いので描き直す
fn kept_shadow(redraw: bool, drawn: Option<(Mat4, u32)>, map_size: u32) -> Option<Mat4> {
    if redraw { return None; }
    drawn.filter(|d| d.1 == map_size).map(|d| d.0)
}

/// 並べ替えの奥行き。NaN は「同じ」にすると並びの決まりが矛盾し、Rust の sort が panic する（pinball で時計 6 倍の登頂時）。
/// NaN だけを最も奥（無限大）として扱い、ほかの値の並び方は変えない
fn sort_z(z: f32) -> f32 { if z.is_nan() { f32::INFINITY } else { z } }

/// 不透明の並び（painterSortStable）
fn opaque_order(a: &Item, b: &Item) -> std::cmp::Ordering {
    a.group_order.cmp(&b.group_order).then(a.render_order.cmp(&b.render_order)).then(a.mat.cmp(&b.mat)).then(sort_z(a.z).partial_cmp(&sort_z(b.z)).unwrap_or(std::cmp::Ordering::Equal)).then(a.uid.cmp(&b.uid))
}
/// 半透明の並び（reversePainterSortStable）
fn trans_order(a: &Item, b: &Item) -> std::cmp::Ordering {
    a.group_order.cmp(&b.group_order).then(a.render_order.cmp(&b.render_order)).then(sort_z(b.z).partial_cmp(&sort_z(a.z)).unwrap_or(std::cmp::Ordering::Equal)).then(a.uid.cmp(&b.uid))
}

impl Kit {
    /// 節点を集める（three の projectObject）。光も集めて uniform を作る。shadow_redraw が false なら影は前に描いた型と光の行列のまま
    fn collect(&mut self, cam: &RawView, shadow_redraw: bool) -> (LightsU, Globals) {
        let view = cam.view;
        let proj = cam.proj;
        let vp = proj * view;
        let planes = Frustum::new(vp);
        let mut l = std::mem::take(&mut self.lists);
        l.opaque.clear(); l.trans.clear(); l.mask.clear(); l.casters.clear(); l.actor_mask.clear(); l.actor_cast.clear(); l.shadows.clear();
        let mut u = LightsU::zeroed();
        let mut key: Option<([f32; 4], [f32; 4])> = None;
        let mut hemi0: Option<([f32; 4], [f32; 4])> = None;
        let mut dirs_shadow: Vec<[[f32; 4]; 2]> = Vec::new();
        let mut dirs_plain: Vec<[[f32; 4]; 2]> = Vec::new();
        let mut shadow_u: Vec<ShadowU> = Vec::new();
        let vogel = self.shadow_vogel;
        let z_row = view.row(2);
        let main_layers = cam.layers;

        // 節点を描く列へ入れる
        #[allow(clippy::too_many_arguments)]
        fn push(l: &mut Lists, it: Item, transparent: bool, in_view: bool, layers: u32, main_layers: u32, cast: bool) {
            let main = layers & main_layers != 0;
            if main && in_view {
                if transparent { l.trans.push(it); } else { l.opaque.push(it); }
            }
            if layers & 2 != 0 && in_view && matches!(it.kind, ItemKind::Mesh | ItemKind::Instanced) { l.mask.push(it); }
            if cast && main && matches!(it.kind, ItemKind::Mesh | ItemKind::Instanced | ItemKind::Batched | ItemKind::Skinned) { l.casters.push(it); }
        }

        let mut stack: Vec<(u32, i32)> = vec![(self.scene.root.0, 0)];
        while let Some((i, mut go)) = stack.pop() {
            let n = &self.scene.nodes()[i as usize];
            if !n.visible || !n.alive { continue; }
            if matches!(n.kind, NodeKind::Group) { go = n.render_order; }
            let world = n.world;
            let (uid, layers, ro, culled, cast) = (n.uid, n.layers, n.render_order, n.frustum_culled, n.cast_shadow);
            let main_on = layers & main_layers != 0;
            let cw = world.determinant() < 0.0;
            for c in n.children.iter().rev() { stack.push((c.0, go)); }
            let sphere = |c: Vec3, r: f32| (world.transform_point3(c), r * max_scale(world));
            let view_z = |c: Vec3| -z_row.dot(c.extend(1.0));
            let blank = Item { node: i, uid, mat: 0, geo: 0, start: 0, count: 0, kind: ItemKind::Mesh, group_order: go, render_order: ro, z: 0.0, cw, first: 0, inst: 1, pipe: 0, bg: 0, skel: 0 };
            match &n.kind {
                NodeKind::Object | NodeKind::Group => {}
                NodeKind::Mesh { geo, mats, multi } => {
                    let g = &self.geos[geo.0 as usize];
                    let (c, r) = sphere(g.center, g.radius);
                    let in_view = !culled || planes.intersects(c, r);
                    let z = view_z(c);
                    let (dr_start, dr_count) = g.draw_range.unwrap_or((0, g.n));
                    let dr_end = (dr_start.saturating_add(dr_count)).min(g.n);
                    let one = |m: MatId, start: u32, end: u32, l: &mut Lists| {
                        let s = &self.mats[m.0 as usize].m;
                        if !s.visible || end <= start { return; }
                        let it = Item { mat: m.0, geo: geo.0, start, count: end - start, z, ..blank };
                        push(l, it, s.transparent, in_view, layers, main_layers, cast);
                    };
                    if *multi {
                        for gr in &g.groups {
                            let Some(m) = mats.get(gr.material as usize) else { continue };
                            one(*m, gr.start.max(dr_start), (gr.start + gr.count).min(dr_end), &mut l);
                        }
                    } else {
                        one(mats[0], dr_start, dr_end, &mut l);
                    }
                }
                NodeKind::Instanced(s) => {
                    if s.count == 0 { continue; }
                    let g = &self.geos[s.geo.0 as usize];
                    let (geo, mat, count) = (s.geo, s.mat, s.count);
                    let (gc, gr, n_idx, dr) = (g.center, g.radius, g.n, g.draw_range);
                    // three の InstancedMesh.computeBoundingSphere（作ったら更新しない）
                    let (bc, br) = match s.bounds {
                        Some(b) => b,
                        None => {
                            let mut pts: Vec<(Vec3, f32)> = Vec::with_capacity(count);
                            for m in &s.matrices[..count] { pts.push((m.transform_point3(gc), gr * max_scale(*m))); }
                            let mut box_lo = Vec3::splat(f32::MAX);
                            let mut box_hi = Vec3::splat(f32::MIN);
                            for (p, r) in &pts { box_lo = box_lo.min(*p - Vec3::splat(*r)); box_hi = box_hi.max(*p + Vec3::splat(*r)); }
                            let c = (box_lo + box_hi) * 0.5;
                            let mut rr = 0.0f32;
                            for (p, r) in &pts { rr = rr.max(p.distance(c) + r); }
                            if let NodeKind::Instanced(s2) = &mut self.scene.node_mut(i).kind { s2.bounds = Some((c, rr)); }
                            (c, rr)
                        }
                    };
                    let (c, r) = sphere(bc, br);
                    let in_view = !culled || planes.intersects(c, r);
                    let z = view_z(c);
                    let ms = &self.mats[mat.0 as usize].m;
                    let (start, end) = dr.map_or((0, n_idx), |(a, b)| (a, (a + b).min(n_idx)));
                    if !ms.visible || end <= start { continue; }
                    let it = Item { mat: mat.0, geo: geo.0, start, count: end - start, z, kind: ItemKind::Instanced, inst: count as u32, ..blank };
                    push(&mut l, it, ms.transparent, in_view, layers, main_layers, cast);
                }
                NodeKind::Skinned(sk) => {
                    let g = &self.geos[sk.geo.0 as usize];
                    let (bc, br) = sk.bounds.unwrap_or((g.center, g.radius));
                    let (c, r) = sphere(bc, br);
                    let in_view = !culled || planes.intersects(c, r);
                    let (start, end) = g.draw_range.map_or((0, g.n), |(a, b)| (a, (a + b).min(g.n)));
                    let ms = &self.mats[sk.mat.0 as usize].m;
                    if !ms.visible || end <= start { continue; }
                    // 骨の組を消したのに節点が残っていたら描かない
                    if !self.scene.has_skeleton(sk.skeleton) { continue; }
                    let it = Item { mat: sk.mat.0, geo: sk.geo.0, start, count: end - start, z: view_z(c), kind: ItemKind::Skinned, skel: sk.skeleton.0, ..blank };
                    push(&mut l, it, ms.transparent, in_view, layers, main_layers, cast);
                }
                NodeKind::Batched(b) => {
                    // 節点の frustum_culled は見ない（three の使い方は frustumCulled = false で、間引きはインスタンスごと）。形ごとに分けるのは prepare_list と影の列で
                    let ms = &self.mats[b.mat.0 as usize].m;
                    if !ms.visible || b.is_empty() { continue; }
                    let it = Item { mat: b.mat.0, kind: ItemKind::Batched, z: view_z(world.w_axis.truncate()), inst: 0, ..blank };
                    push(&mut l, it, ms.transparent, true, layers, main_layers, cast);
                }
                NodeKind::Sprite { mat, .. } => {
                    let ms = &self.mats[mat.0 as usize].m;
                    if !ms.visible || !main_on { continue; }
                    let (c, r) = sphere(Vec3::ZERO, std::f32::consts::FRAC_1_SQRT_2);
                    let in_view = !culled || planes.intersects(c, r);
                    let geo = self.sprite_geo.0;
                    let it = Item { mat: mat.0, geo, start: 0, count: 6, kind: ItemKind::Sprite, z: view_z(world.w_axis.truncate()), ..blank };
                    push(&mut l, it, ms.transparent, in_view, layers, main_layers, false);
                }
                NodeKind::Points(s) => {
                    if s.count == 0 || !main_on { continue; }
                    let (mat, count) = (s.mat, s.count);
                    let ms = &self.mats[mat.0 as usize].m;
                    if !ms.visible { continue; }
                    let (bc, br) = match s.bounds {
                        Some(b) => b,
                        None => {
                            let b = bounds_of(&s.positions);
                            if let NodeKind::Points(s2) = &mut self.scene.node_mut(i).kind { s2.bounds = Some(b); }
                            b
                        }
                    };
                    let (c, r) = sphere(bc, br);
                    let in_view = !culled || planes.intersects(c, r);
                    let it = Item { mat: mat.0, kind: ItemKind::Points, inst: count as u32, z: view_z(c), ..blank };
                    let t = self.mats[mat.0 as usize].m.transparent;
                    push(&mut l, it, t, in_view, layers, main_layers, false);
                }
                NodeKind::Lines(s) => {
                    if s.positions.is_empty() || !main_on { continue; }
                    let mat = s.mat;
                    let vcount = (s.positions.len() / 3) as u32;
                    let ms = &self.mats[mat.0 as usize].m;
                    if !ms.visible { continue; }
                    let (bc, br) = match s.bounds {
                        Some(b) => b,
                        None => {
                            let b = bounds_of(&s.positions);
                            if let NodeKind::Lines(s2) = &mut self.scene.node_mut(i).kind { s2.bounds = Some(b); }
                            b
                        }
                    };
                    let (c, r) = sphere(bc, br);
                    let in_view = !culled || planes.intersects(c, r);
                    let it = Item { mat: mat.0, kind: ItemKind::Lines, count: vcount, z: view_z(c), ..blank };
                    let t = self.mats[mat.0 as usize].m.transparent;
                    push(&mut l, it, t, in_view, layers, main_layers, false);
                }
                NodeKind::Actor => {
                    if !self.actors.get(&NodeId(i)).is_some_and(|s| s.actor.is_some() && s.uid == uid) && !self.npc_actors.get(&NodeId(i)).is_some_and(|s| s.actor.is_some() && s.uid == uid) { continue; }
                    let z = view_z(world.w_axis.truncate());
                    if main_on {
                        l.opaque.push(Item { mat: MAT_ACTOR, kind: ItemKind::ActorOpaque, z, ..blank });
                        // 半透明の体と ＞＜ の板は renderOrder 0・入れ物の z
                        l.trans.push(Item { mat: MAT_ACTOR, kind: ItemKind::ActorTrans, z, render_order: 0, group_order: 0, ..blank });
                        if cast { l.actor_cast.push(NodeId(i)); }
                    }
                    if layers & 2 != 0 { l.actor_mask.push(NodeId(i)); }
                }
                NodeKind::Dir(d) => {
                    if !main_on { continue; }
                    let pos = world.w_axis.truncate();
                    let target = self.scene.target_position(d.target);
                    let dir = (pos - target).normalize_or_zero();
                    let c = d.color.scale(d.intensity);
                    let e = [[dir.x, dir.y, dir.z, 0.0], [c.r, c.g, c.b, 0.0]];
                    if key.is_none() { key = Some((e[1], e[0])); }
                    if cast && shadow_u.len() < 2 {
                        let s = d.shadow;
                        let kept = kept_shadow(shadow_redraw, self.shadow_drawn[shadow_u.len()], s.map_size);
                        let lvp = kept.unwrap_or_else(|| {
                            let rot = crate::scene::look_at_rotation(pos, target, Vec3::Y);
                            let lv = Mat4::from_mat3_translation(rot, pos).inverse();
                            let lp = Mat4::orthographic_rh(s.left, s.right, s.bottom, s.top, s.near, s.far);
                            lp * lv
                        });
                        shadow_u.push(ShadowU { vp: lvp.to_cols_array(), p: [s.intensity, s.bias, s.normal_bias, s.map_size as f32], q: [if vogel { s.radius } else { 0.0 }, 0.0, 0.0, 0.0] });
                        l.shadows.push(ShadowInfo { vp: lvp, planes: Frustum::new(lvp), map_size: s.map_size, redraw: kept.is_none() });
                        dirs_shadow.push(e);
                    } else {
                        dirs_plain.push(e);
                    }
                }
                NodeKind::Hemi(h) => {
                    if !main_on { continue; }
                    let dir = world.w_axis.truncate().normalize_or_zero();
                    let (sk, gr) = (h.sky.scale(h.intensity), h.ground.scale(h.intensity));
                    let k = u.counts[3] as usize;
                    if k < 2 {
                        u.hemi[k] = [[dir.x, dir.y, dir.z, 0.0], [sk.r, sk.g, sk.b, 0.0], [gr.r, gr.g, gr.b, 0.0]];
                        u.counts[3] += 1;
                        if hemi0.is_none() { hemi0 = Some(([sk.r, sk.g, sk.b, 0.0], [gr.r, gr.g, gr.b, 0.0])); }
                    }
                }
                NodeKind::Ambient(a) => {
                    if !main_on { continue; }
                    u.ambient[0] += a.color.r * a.intensity;
                    u.ambient[1] += a.color.g * a.intensity;
                    u.ambient[2] += a.color.b * a.intensity;
                }
                NodeKind::Point(p) => {
                    if !main_on { continue; }
                    let k = u.counts[1] as usize;
                    if k < 16 {
                        let w = world.w_axis.truncate();
                        let c = p.color.scale(p.intensity);
                        u.point[k] = [[w.x, w.y, w.z, p.distance], [c.r, c.g, c.b, p.decay]];
                        u.counts[1] += 1;
                    }
                }
                NodeKind::Spot(p) => {
                    if !main_on { continue; }
                    let k = u.counts[2] as usize;
                    if k < 2 {
                        let w = world.w_axis.truncate();
                        let dir = (w - self.scene.target_position(p.target)).normalize_or_zero();
                        let c = p.color.scale(p.intensity);
                        u.spot[k] = [[w.x, w.y, w.z, p.distance], [dir.x, dir.y, dir.z, p.decay], [c.r, c.g, c.b, 0.0], [p.angle.cos(), (p.angle * (1.0 - p.penumbra)).cos(), 0.0, 0.0]];
                        u.counts[2] += 1;
                    }
                }
            }
        }
        // 影のある方向の光を先に（three の shadowCastingAndTexturingLightsFirst）
        u.shadow_count[0] = dirs_shadow.len() as u32;
        for (k, e) in dirs_shadow.iter().chain(dirs_plain.iter()).take(4).enumerate() { u.dir[k] = *e; u.counts[0] += 1; }
        for (k, s) in shadow_u.iter().enumerate() { u.shadow[k] = *s; }
        let hc = self.hidden_color;
        u.hidden = [hc.r, hc.g, hc.b, self.hidden_opacity];
        u.dim = [self.shared.dim, self.shared.cloud_k, 0.0, 0.0];
        u.lamps = self.shared.lamps;
        l.opaque.sort_by(opaque_order);
        l.trans.sort_by(trans_order);
        l.view_planes = Some(planes);
        self.lists = l;

        u.ambient[3] = if self.three_r186 { 1.0 } else { 0.0 };
        let direct = self.output != Output::Linear;
        // 直接描くときの霧の色は sRGB（three の getUnlitUniformColorSpace）
        let fog_rgb = |c: Color| -> [f32; 3] { if direct { [srgb_of(c.r), srgb_of(c.g), srgb_of(c.b)] } else { [c.r, c.g, c.b] } };
        let mut fog = self.fog.map_or([0.0; 4], |f| { let c = fog_rgb(f.color); [c[0], c[1], c[2], f.density] });
        let rc = self.shared.rim_color;
        let (k_color, k_dir) = key.unwrap_or(([0.0; 4], [0.0, 1.0, 0.0, 0.0]));
        let (mut h_sky, mut h_ground) = hemi0.unwrap_or(([0.0; 4], [0.0; 4]));
        if let Some(f) = self.fog_linear { let c = fog_rgb(f.color); fog = [c[0], c[1], c[2], -1.0]; h_sky[3] = f.near; h_ground[3] = f.far; }
        let lvp0 = self.lists.shadows.first().map_or(Mat4::IDENTITY, |s| s.vp);
        let globals = Globals {
            view: view.to_cols_array_2d(), proj: proj.to_cols_array_2d(), view_proj: vp.to_cols_array_2d(), light_vp: lvp0.to_cols_array_2d(),
            cam: [cam.position.x, cam.position.y, cam.position.z, match self.output { Output::Linear => 1.0, Output::DirectFlat => 2.0, Output::DirectAces => 3.0 }],
            hemi_sky: h_sky, hemi_ground: h_ground, key_color: k_color, key_dir: k_dir, fog, rim: [rc.r, rc.g, rc.b, self.shared.rim_i],
            misc: [self.shared.time, self.t.h as f32, self.t.w as f32, self.dpr],
        };
        (u, globals)
    }

    /// 節点の行列をインスタンスの流れへ（1 コマ 1 回）
    fn stream_slot(&mut self, node: u32, fx: [f32; 4]) -> u32 {
        let frame = self.scene.frame;
        let l = &mut self.lists;
        if l.stream_at.len() <= node as usize { l.stream_at.resize(node as usize + 1, (0, 0)); }
        let e = l.stream_at[node as usize];
        if e.0 == frame { return e.1; }
        let at = l.stream.len() as u32;
        l.stream.push(InstRaw { m: self.scene.nodes()[node as usize].world.to_cols_array(), fx });
        self.lists.stream_at[node as usize] = (frame, at);
        at
    }

    /// InstancedMesh の GPU（世界 × インスタンスを CPU で掛けて、変わったときだけ描く数だけ送る）
    fn upload_instanced(&mut self, node: u32) {
        let i = node as usize;
        if self.dyn_gpu.len() <= i { self.dyn_gpu.resize_with(i + 1, || None); }
        let n = &self.scene.nodes()[i];
        let NodeKind::Instanced(s) = &n.kind else { return };
        let (uid, recv, world, wg) = (n.uid, n.receive_shadow, n.world, n.world_gen);
        let formats = self.mats[s.mat.0 as usize].m.custom_shader().map_or_else(Vec::new, |c| c.instance_attrs.clone());
        let fresh = !matches!(&self.dyn_gpu[i], Some(DynGpu::Inst { uid: u, .. }) if *u == uid);
        if fresh {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("instanced"), size: (s.capacity.max(1) * 80) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let attrs = formats.iter().map(|f| self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("instance attr"), size: (s.capacity.max(1) as u64) * f.size(), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })).collect();
            self.dyn_gpu[i] = Some(DynGpu::Inst { uid, buf, attrs, world_gen: 0, recv });
        }
        let Some(DynGpu::Inst { buf, attrs, world_gen, recv: last_recv, .. }) = &mut self.dyn_gpu[i] else { return };
        let dirty = fresh || s.dirty || *world_gen != wg || *last_recv != recv;
        if dirty {
            *world_gen = wg;
            *last_recv = recv;
            let r = if recv { 1.0 } else { 0.0 };
            let raw: Vec<InstRaw> = (0..s.count).map(|k| {
                let c = s.colors.as_ref().map_or([1.0; 3], |c| c[k]);
                InstRaw { m: (world * s.matrices[k]).to_cols_array(), fx: [c[0], c[1], c[2], r] }
            }).collect();
            self.queue.write_buffer(buf, 0, bytemuck::cast_slice(&raw));
        }
        // 独自の属性: 描く数だけ送る
        for (k, a) in s.attrs.iter().enumerate() {
            if !s.attr_dirty.get(k).copied().unwrap_or(false) { continue; }
            let Some(b) = attrs.get(k) else { continue };
            let floats = (formats[k].size() / 4) as usize * s.count;
            let n = floats.min(a.len());
            self.queue.write_buffer(b, 0, bytemuck::cast_slice(&a[..n]));
        }
        if let NodeKind::Instanced(s) = &mut self.scene.node_mut(node).kind {
            s.dirty = false;
            for d in &mut s.attr_dirty { *d = false; }
        }
    }

    fn upload_points(&mut self, node: u32) {
        let i = node as usize;
        if self.dyn_gpu.len() <= i { self.dyn_gpu.resize_with(i + 1, || None); }
        let n = &self.scene.nodes()[i];
        let NodeKind::Points(s) = &n.kind else { return };
        let uid = n.uid;
        let formats = self.mats[s.mat.0 as usize].m.custom_shader().map_or_else(Vec::new, |c| c.point_attrs.clone());
        if !matches!(&self.dyn_gpu[i], Some(DynGpu::Points { uid: u, .. }) if *u == uid) {
            let pos = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("points"), size: (s.capacity.max(1) * 12) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let attrs = formats.iter().map(|f| self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("point attr"), size: (s.capacity.max(1) as u64) * f.size(), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false })).collect();
            self.dyn_gpu[i] = Some(DynGpu::Points { uid, pos, attrs, uploaded: 0, uploaded_attr: vec![0; formats.len()] });
        }
        let Some(DynGpu::Points { pos, attrs, uploaded, uploaded_attr, .. }) = &mut self.dyn_gpu[i] else { return };
        if s.pos_dirty || s.count > *uploaded {
            let n = (s.count * 3).min(s.positions.len());
            self.queue.write_buffer(pos, 0, bytemuck::cast_slice(&s.positions[..n]));
            *uploaded = s.count;
        }
        for (k, a) in s.attrs.iter().enumerate() {
            let Some(b) = attrs.get(k) else { continue };
            if !s.attr_dirty.get(k).copied().unwrap_or(false) && s.count <= uploaded_attr[k] { continue; }
            let n = ((formats[k].size() / 4) as usize * s.count).min(a.len());
            self.queue.write_buffer(b, 0, bytemuck::cast_slice(&a[..n]));
            uploaded_attr[k] = s.count;
        }
        if let NodeKind::Points(s) = &mut self.scene.node_mut(node).kind {
            s.pos_dirty = false;
            for d in &mut s.attr_dirty { *d = false; }
        }
    }

    fn upload_lines(&mut self, node: u32) {
        let i = node as usize;
        if self.dyn_gpu.len() <= i { self.dyn_gpu.resize_with(i + 1, || None); }
        let n = &self.scene.nodes()[i];
        let NodeKind::Lines(s) = &n.kind else { return };
        let uid = n.uid;
        let cap = s.capacity.max(s.positions.len() / 3).max(1);
        let need_new = match &self.dyn_gpu[i] { Some(DynGpu::Lines { uid: u, pos, .. }) => *u != uid || pos.size() < (cap * 12) as u64, _ => true };
        if need_new {
            let pos = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("lines"), size: (cap * 12) as u64, usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            self.dyn_gpu[i] = Some(DynGpu::Lines { uid, pos, n: 0 });
        }
        let Some(DynGpu::Lines { pos, n, .. }) = &mut self.dyn_gpu[i] else { return };
        if s.dirty || need_new {
            self.queue.write_buffer(pos, 0, bytemuck::cast_slice(&s.positions));
            *n = (s.positions.len() / 3) as u32;
        }
        if let NodeKind::Lines(s) = &mut self.scene.node_mut(node).kind { s.dirty = false; }
    }

    /// 骨の行列を GPU へ（1 コマ 1 回）
    fn upload_skeleton(&mut self, id: u32) {
        let frame = self.scene.frame;
        if self.skel_gpu.get(&id).is_some_and(|g| g.frame == frame) { return; }
        let mut pal = std::mem::take(&mut self.palette);
        self.scene.bone_matrices(crate::scene::SkeletonId(id), &mut pal);
        let bytes = (pal.len().max(1) * 64) as u64;
        if self.skel_gpu.get(&id).is_none_or(|g| g.buf.size() < bytes) {
            let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("bones"), size: bytes, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("bones"), layout: &self.bones_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }] });
            self.skel_gpu.insert(id, super::SkelGpu { buf, bg, frame });
        }
        let g = self.skel_gpu.get_mut(&id).unwrap();
        g.frame = frame;
        self.queue.write_buffer(&g.buf, 0, bytemuck::cast_slice(&pal));
        self.palette = pal;
    }

    /// BatchedMesh の節点を、この視野で見える形ごとの描く列へ分ける（インスタンスは流れへ入る）
    fn batch_items(&mut self, it: &Item, planes: &Frustum) -> Vec<Item> {
        let n = &self.scene.nodes()[it.node as usize];
        let NodeKind::Batched(b) = &n.kind else { return Vec::new() };
        let custom = matches!(self.mats[it.mat as usize].m.kind, crate::material::MatKind::Custom(_));
        let fx_w = if custom { None } else { Some(if n.receive_shadow { 1.0 } else { 0.0 }) };
        let bounds: Vec<(Vec3, f32)> = b.shapes.iter().map(|g| { let g = &self.geos[g.0 as usize]; (g.center, g.radius) }).collect();
        let mut runs = Vec::new();
        super::batch::fill(b, n.world, planes, &bounds, fx_w, &mut self.lists.stream, &mut runs, &mut self.lists.batch);
        runs.iter().filter_map(|r| {
            let geo = b.shape_geo(r.shape);
            let g = &self.geos[geo.0 as usize];
            let (start, end) = g.draw_range.map_or((0, g.n), |(a, c)| (a, (a + c).min(g.n)));
            (end > start).then_some(Item { kind: ItemKind::BatchRun, geo: geo.0, start, count: end - start, first: r.first, inst: r.count, ..*it })
        }).collect()
    }

    /// 並んだ列へパイプラインと流れの位置を入れる。同じ形・材質が続く所は 1 つにまとめる
    fn prepare_list(&mut self, list: Vec<Item>) -> Vec<Item> {
        let mut out: Vec<Item> = Vec::with_capacity(list.len());
        let planes = self.lists.view_planes;
        for it in list {
            if it.kind == ItemKind::Batched {
                let Some(p) = &planes else { continue };
                for run in self.batch_items(&it, p) { self.prepare_one(run, &mut out); }
            } else {
                self.prepare_one(it, &mut out);
            }
        }
        out
    }

    fn prepare_one(&mut self, mut it: Item, out: &mut Vec<Item>) {
        match it.kind {
            ItemKind::ActorOpaque | ItemKind::ActorTrans => { out.push(it); return; }
            ItemKind::Instanced => self.upload_instanced(it.node),
            ItemKind::Points => self.upload_points(it.node),
            ItemKind::Lines => self.upload_lines(it.node),
            ItemKind::Skinned => { self.upload_skeleton(it.skel); self.upload_morph(it.node, it.geo); }
            ItemKind::Mesh | ItemKind::Sprite | ItemKind::BatchRun | ItemKind::Batched => {}
        }
        let vert = match it.kind { ItemKind::Points => VertKind::Points, ItemKind::Lines => VertKind::Line, ItemKind::Skinned => VertKind::Skinned, _ => VertKind::Mesh };
        it.pipe = self.main_pipe(it.mat, vert, it.cw);
        // 作れなかった（または確かめ中の）シェーダーの材質は描かない
        if !self.pipe_ready(it.pipe) { return; }
        it.bg = self.mat_bind(it.mat);
        match it.kind {
            ItemKind::Mesh | ItemKind::Sprite | ItemKind::Lines | ItemKind::Points | ItemKind::Skinned => {
                let n = &self.scene.nodes()[it.node as usize];
                let fx = match &n.kind {
                    NodeKind::Sprite { center, .. } => [center.x, center.y, 0.0, 0.0],
                    _ => [1.0, 1.0, 1.0, if n.receive_shadow { 1.0 } else { 0.0 }],
                };
                it.first = self.stream_slot(it.node, fx);
            }
            _ => {}
        }
        if matches!(it.kind, ItemKind::Mesh | ItemKind::Sprite)
            && let Some(p) = out.last_mut()
            && p.kind == it.kind && p.pipe == it.pipe && p.geo == it.geo && p.mat == it.mat && p.start == it.start && p.count == it.count && p.cw == it.cw && p.first + p.inst == it.first
        {
            p.inst += 1;
            return;
        }
        // 両面の半透明は three と同じく裏の面 → 表の面の 2 回に分ける（閉じた形で手前の面が必ず後に重なる）
        let m = &self.mats[it.mat as usize].m;
        if m.transparent && m.side == crate::material::Side::Double && matches!(it.kind, ItemKind::Mesh | ItemKind::Instanced | ItemKind::BatchRun | ItemKind::Skinned) {
            let mut back = it;
            back.pipe = self.side_pipe(it.mat, vert, wgpu::Face::Front, it.cw);
            let front = self.side_pipe(it.mat, vert, wgpu::Face::Back, it.cw);
            if !self.pipe_ready(back.pipe) || !self.pipe_ready(front) { return; }
            out.push(back);
            it.pipe = front;
        }
        out.push(it);
    }

    /// 透過用の的へ重ねる裏の面の列（prepare_list のあと、パイプラインを表の面を間引く物に替える）。
    /// 両面の半透明は prepare_one が裏・表の 2 つに分けるので、表の方は捨てる
    fn prepare_back(&mut self, list: Vec<Item>) -> Vec<Item> {
        let mut out = Vec::with_capacity(list.len());
        for mut it in self.prepare_list(list) {
            let vert = if it.kind == ItemKind::Skinned { VertKind::Skinned } else { VertKind::Mesh };
            let back = self.side_pipe(it.mat, vert, wgpu::Face::Front, it.cw);
            let split = self.mats[it.mat as usize].m.transparent;
            if split && it.pipe != back { continue; }
            if !self.pipe_ready(back) { continue; }
            it.pipe = back;
            out.push(it);
        }
        out
    }

    /// 深度だけの列（影の型・自機の型）を整える
    fn prepare_plain(&mut self, list: &mut [Item], pass: Pass) {
        for it in list.iter_mut() {
            if it.kind == ItemKind::Instanced { self.upload_instanced(it.node); }
            else if it.kind != ItemKind::BatchRun {
                let n = &self.scene.nodes()[it.node as usize];
                let fx = [1.0, 1.0, 1.0, if n.receive_shadow { 1.0 } else { 0.0 }];
                it.first = self.stream_slot(it.node, fx);
            }
            let m = &self.mats[it.mat as usize].m;
            let side = m.side;
            let alpha = pass == Pass::Shadow && m.map.is_some() && m.alpha_test > 0.0;
            let vert = if it.kind == ItemKind::Skinned { VertKind::Skinned } else { VertKind::Mesh };
            if it.kind == ItemKind::Skinned { self.upload_skeleton(it.skel); self.upload_morph(it.node, it.geo); }
            it.pipe = self.plain_pipe(if alpha { Pass::ShadowAlpha } else { pass }, vert, side, it.cw);
            it.bg = if alpha { self.mat_bind(it.mat) } else { u32::MAX };
        }
    }

    /// 影の的を揃える（大きさが変わると作り直す）
    fn ensure_shadow_maps(&mut self) {
        let mut changed = false;
        for k in 0..self.lists.shadows.len().min(2) {
            let size = self.lists.shadows[k].map_size;
            if self.shadow_maps[k].size != size {
                self.shadow_maps[k] = super::ShadowMap { view: super::depth_map(&self.device, size), size };
                changed = true;
            }
        }
        if changed {
            self.lights_bg = self.device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("lights"), layout: &self.lights_layout, entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&self.shadow_maps[0].view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.shadow_smp) },
                wgpu::BindGroupEntry { binding: 2, resource: self.lights_buf.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&self.shadow_maps[1].view) },
            ] });
        }
    }

    /// 流れを GPU へ（足りなければ広げる）
    fn upload_stream(&mut self) {
        let bytes: &[u8] = bytemuck::cast_slice(&self.lists.stream);
        if bytes.len() as u64 > self.stream.size() {
            self.stream = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("instances"), size: (bytes.len() as u64 * 2).max(80 * 1024), usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
        }
        if !bytes.is_empty() { self.queue.write_buffer(&self.stream, 0, bytes); }
    }

    fn write_actor_frames(&mut self) {
        let frame_alive: Vec<NodeId> = self.actors.keys().copied().filter(|id| !self.scene.is_alive(*id) || self.scene.uid(*id) != self.actors[id].uid).collect();
        for id in frame_alive { if let Some(mut s) = self.actors.remove(&id) { s.view.destroy_gpu(); } }
        let npc_gone: Vec<NodeId> = self.npc_actors.keys().copied().filter(|id| !self.scene.is_alive(*id) || self.scene.uid(*id) != self.npc_actors[id].uid).collect();
        for id in npc_gone { if let Some(mut s) = self.npc_actors.remove(&id) { s.view.destroy_gpu(); } }
        let (hc, ho) = (self.hidden_color.to_array(), self.hidden_opacity);
        for s in self.actors.values_mut() {
            if let Some(a) = &s.actor {
                let mut look = s.look;
                look.hidden = hc;
                look.hidden_opacity = ho;
                s.view.write_frame(&self.queue, a, &look);
            }
        }
        for s in self.npc_actors.values_mut() {
            if let Some(a) = &s.actor {
                let mut look = s.look;
                look.hidden = hc;
                look.hidden_opacity = ho;
                s.view.write_frame(&self.queue, a, &super::npc::npc_look(&look));
            }
        }
    }

    /// 主の的の中身（不透明 → 半透明 → 影絵）。three の render の列の順
    fn draw_main<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, opaque: &[Item], trans: &[Item], hidden: Option<&[Item]>) {
        pass.set_bind_group(0, &self.globals_bg, &[]);
        pass.set_bind_group(1, &self.lights_bg, &[]);
        let mut st = DrawState::new();
        self.draw_items(pass, opaque, &mut st);
        self.draw_items(pass, trans, &mut st);
        if let Some(mask) = hidden {
            // 物陰の影絵（renderer.ts の HiddenPlayerPass。層 1 の物）
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_bind_group(1, &self.lights_bg, &[]);
            st.pipe = u32::MAX;
            st.geo = u32::MAX;
            for it in mask { self.draw_plain(pass, it, &mut st); }
            for id in &self.lists.actor_mask {
                if let Some(s) = self.actors.get(id) && let Some(a) = &s.actor { s.view.draw_hidden(pass, a); }
                if let Some(s) = self.npc_actors.get(id) && let Some(a) = &s.actor { s.view.draw_hidden(pass, a); }
            }
        }
    }

    fn draw_items<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, items: &[Item], st: &mut DrawState) {
        for it in items {
            match it.kind {
                ItemKind::ActorOpaque | ItemKind::ActorTrans => {
                    if let Some(s) = self.actors.get(&NodeId(it.node)) && let Some(a) = &s.actor {
                        if it.kind == ItemKind::ActorOpaque { s.view.draw_opaque(pass, a); } else { s.view.draw_transparent(pass, a); }
                    }
                    if let Some(s) = self.npc_actors.get(&NodeId(it.node)) && let Some(a) = &s.actor {
                        if it.kind == ItemKind::ActorOpaque { s.view.draw_opaque(pass, a); } else { s.view.draw_transparent(pass, a); }
                    }
                    // 主役は group 1..3 を自分の物に替える
                    pass.set_bind_group(0, &self.globals_bg, &[]);
                    pass.set_bind_group(1, &self.lights_bg, &[]);
                    *st = DrawState::new();
                }
                _ => {
                    if st.pipe != it.pipe { pass.set_pipeline(&self.pipes[it.pipe as usize]); st.pipe = it.pipe; }
                    let off = it.mat * super::MAT_SLOT as u32;
                    if st.bg != it.bg || st.off != off { pass.set_bind_group(2, &self.bgs[it.bg as usize], &[off]); st.bg = it.bg; st.off = off; }
                    match it.kind {
                        ItemKind::Mesh | ItemKind::Sprite | ItemKind::BatchRun => {
                            self.bind_geo(pass, it.geo, st);
                            if !st.stream { pass.set_vertex_buffer(1, self.stream.slice(..)); st.stream = true; }
                            pass.draw_indexed(it.start..it.start + it.count, 0, it.first..it.first + it.inst);
                        }
                        ItemKind::Skinned => {
                            self.bind_geo(pass, it.geo, st);
                            if let Some(b) = self.morph_vb(it.node, it.geo) { pass.set_vertex_buffer(0, b.slice(..)); st.geo = u32::MAX; }
                            if !st.stream { pass.set_vertex_buffer(1, self.stream.slice(..)); st.stream = true; }
                            if let (Some(skin), Some(sk)) = (&self.geos[it.geo as usize].skin, self.skel_gpu.get(&it.skel)) {
                                pass.set_vertex_buffer(2, skin.slice(..));
                                pass.set_bind_group(3, &sk.bg, &[]);
                                pass.draw_indexed(it.start..it.start + it.count, 0, it.first..it.first + 1);
                            }
                        }
                        ItemKind::Instanced => {
                            self.bind_geo(pass, it.geo, st);
                            if let Some(DynGpu::Inst { buf, attrs, .. }) = &self.dyn_gpu[it.node as usize] {
                                pass.set_vertex_buffer(1, buf.slice(..));
                                for (k, a) in attrs.iter().enumerate() { pass.set_vertex_buffer(2 + k as u32, a.slice(..)); }
                                st.stream = false;
                                pass.draw_indexed(it.start..it.start + it.count, 0, 0..it.inst);
                            }
                        }
                        ItemKind::Points => {
                            if let Some(DynGpu::Points { pos, attrs, .. }) = &self.dyn_gpu[it.node as usize] {
                                pass.set_vertex_buffer(0, pos.slice(..));
                                pass.set_vertex_buffer(1, self.stream.slice(it.first as u64 * 80..(it.first as u64 + 1) * 80));
                                for (k, a) in attrs.iter().enumerate() { pass.set_vertex_buffer(2 + k as u32, a.slice(..)); }
                                st.geo = u32::MAX;
                                st.stream = false;
                                pass.draw(0..6, 0..it.inst);
                            }
                        }
                        ItemKind::Lines => {
                            if let Some(DynGpu::Lines { pos, n, .. }) = &self.dyn_gpu[it.node as usize] {
                                pass.set_vertex_buffer(0, pos.slice(..));
                                if !st.stream { pass.set_vertex_buffer(1, self.stream.slice(..)); st.stream = true; }
                                st.geo = u32::MAX;
                                pass.draw(0..*n, it.first..it.first + 1);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn bind_geo<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, geo: u32, st: &mut DrawState) {
        if st.geo != geo {
            let g = &self.geos[geo as usize];
            pass.set_vertex_buffer(0, g.vb.slice(..));
            pass.set_index_buffer(g.ib.slice(..), wgpu::IndexFormat::Uint32);
            st.geo = geo;
        }
    }

    /// 深度だけの 1 つ（影の型・自機の型・影絵）。頂点の並びは形とインスタンスだけ
    fn draw_plain<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, it: &Item, st: &mut DrawState) {
        if st.pipe != it.pipe { pass.set_pipeline(&self.pipes[it.pipe as usize]); st.pipe = it.pipe; }
        if it.bg != u32::MAX {
            let off = it.mat * super::MAT_SLOT as u32;
            pass.set_bind_group(1, &self.empty_bg, &[]);
            pass.set_bind_group(2, &self.bgs[it.bg as usize], &[off]);
        }
        self.bind_geo(pass, it.geo, st);
        if it.kind == ItemKind::Instanced {
            if let Some(DynGpu::Inst { buf, .. }) = &self.dyn_gpu[it.node as usize] {
                pass.set_vertex_buffer(1, buf.slice(..));
                st.stream = false;
                pass.draw_indexed(it.start..it.start + it.count, 0, 0..it.inst);
            }
        } else if it.kind == ItemKind::Skinned {
            if it.bg == u32::MAX {
                pass.set_bind_group(1, &self.empty_bg, &[]);
                pass.set_bind_group(2, &self.empty_bg, &[]);
            }
            if let Some(b) = self.morph_vb(it.node, it.geo) { pass.set_vertex_buffer(0, b.slice(..)); st.geo = u32::MAX; }
            if !st.stream { pass.set_vertex_buffer(1, self.stream.slice(..)); st.stream = true; }
            if let (Some(skin), Some(sk)) = (&self.geos[it.geo as usize].skin, self.skel_gpu.get(&it.skel)) {
                pass.set_vertex_buffer(2, skin.slice(..));
                pass.set_bind_group(3, &sk.bg, &[]);
                pass.draw_indexed(it.start..it.start + it.count, 0, it.first..it.first + 1);
            }
        } else {
            if !st.stream { pass.set_vertex_buffer(1, self.stream.slice(..)); st.stream = true; }
            pass.draw_indexed(it.start..it.start + it.count, 0, it.first..it.first + it.inst);
        }
    }

    /// 1 コマ描いて画面へ出す。順: 影の型 → 自機の型（outline）→ 主の的 → 保護 → ブルーム → 仕上げ
    pub fn render(&mut self, o: &RenderOpts) { self.render_then(o, |_, _, _, _, _| {}); }

    /// render と同じ順で描き、仕上げ（overlay）の後・submit の前に `after` を呼ぶ。画面の文字とボタン（game-ui の egui）を
    /// サーフェスへ重ねるための入口。`after` には device・queue・コマンド・サーフェスの view・サーフェスの画素数を渡す。
    /// egui の出力はガンマのまま・アルファ掛け済みなので、線形→sRGB に直す overlay には渡さず、ここで直接描く
    pub fn render_then(&mut self, o: &RenderOpts, after: impl FnOnce(&wgpu::Device, &wgpu::Queue, &mut wgpu::CommandEncoder, &wgpu::TextureView, [u32; 2])) {
        if self.released { return; }
        #[cfg(target_arch = "wasm32")]
        if let Some(win) = web_sys::window() {
            if let Some((mw, mh)) = self.measured() { if mw != self.logical.0 || mh != self.logical.1 || win.device_pixel_ratio() as f32 != self.win_dpr { self.resize(); } }
        }
        self.scene.update_world();
        self.drain_shader_checks();
        // 消した骨の組の GPU を片付ける
        { let scene = &self.scene; self.skel_gpu.retain(|id, _| scene.has_skeleton(crate::scene::SkeletonId(*id))); }
        self.last_clear = o.clear;
        let cam = self.camera;
        // three の WebGLShadowMap.render の最初の判定
        let (lights, globals) = self.collect(&RawView::from_camera(&cam), self.shadow_auto_update || self.shadow_needs_update);
        self.ensure_shadow_maps();

        self.lists.stream.clear();
        let mut mask = std::mem::take(&mut self.lists.mask);
        let (o_list, t_list) = (std::mem::take(&mut self.lists.opaque), std::mem::take(&mut self.lists.trans));
        // 透過用の別描画の列（まとめ描きの前に節点で分ける）
        let trans_pass = self.transmission.clone().map(|tr| {
            let size = (((self.t.w as f32) * tr.scale).floor().max(1.0) as u32, ((self.t.h as f32) * tr.scale).floor().max(1.0) as u32);
            let id = self.ensure_transmission(size.0, size.1);
            let marks = self.subtree_marks(&tr.exclude);
            let src: Vec<Item> = o_list.iter().filter(|it| {
                !marks.get(it.node as usize).copied().unwrap_or(false)
                    && (it.mat == MAT_ACTOR || !self.mats[it.mat as usize].m.tex_slots().contains(&Some(id)))
            }).copied().collect();
            // three: 両面の透過する物（transmission > 0 で DoubleSide）は、不透明な物のあとに裏の面だけを透過用の的へ重ねる（renderTransmissionPass の BackSide）
            let back_src: Vec<Item> = o_list.iter().chain(t_list.iter()).filter(|it| {
                it.mat != MAT_ACTOR && matches!(it.kind, ItemKind::Mesh | ItemKind::Instanced | ItemKind::Batched | ItemKind::BatchRun | ItemKind::Skinned) && {
                    let m = &self.mats[it.mat as usize].m;
                    m.side == crate::material::Side::Double && (m.tex_slots().contains(&Some(id)) || marks.get(it.node as usize).copied().unwrap_or(false))
                }
            }).copied().collect();
            let _ = self.gpu_mips_runner();
            let items = self.prepare_list(src);
            let back = self.prepare_back(back_src);
            (id, items, back)
        });
        let opaque = self.prepare_list(o_list);
        let trans = self.prepare_list(t_list);
        // 影の型の列（光の視錐台で間引く）
        let casters = std::mem::take(&mut self.lists.casters);
        let mut shadow_items: [Vec<Item>; 2] = std::mem::take(&mut self.lists.shadow_items);
        for (k, list) in shadow_items.iter_mut().enumerate() {
            list.clear();
            let Some(info) = self.lists.shadows.get(k).copied() else { continue };
            if !info.redraw { continue; }
            for it in &casters {
                // BatchedMesh は光の視錐台でインスタンスごとに間引く（three の onBeforeShadow）
                if it.kind == ItemKind::Batched { let runs = self.batch_items(it, &info.planes); list.extend(runs); continue; }
                let n = &self.scene.nodes()[it.node as usize];
                let (c, r) = if it.kind == ItemKind::Instanced {
                    match &n.kind { NodeKind::Instanced(s) => s.bounds.unwrap_or((Vec3::ZERO, f32::MAX)), _ => (Vec3::ZERO, f32::MAX) }
                } else if let NodeKind::Skinned(sk) = &n.kind && let Some(b) = sk.bounds {
                    b
                } else {
                    let g = &self.geos[it.geo as usize];
                    (g.center, g.radius)
                };
                if !n.frustum_culled || r == f32::MAX || info.planes.intersects(n.world.transform_point3(c), r * max_scale(n.world)) { list.push(*it); }
            }
            self.prepare_plain(list, Pass::Shadow);
        }
        if o.outline { self.prepare_plain(&mut mask, Pass::Mask); }
        let mut hidden_list: Vec<Item> = Vec::new();
        if o.outline && o.hidden {
            hidden_list = mask.clone();
            self.prepare_plain(&mut hidden_list, Pass::Hidden);
        }
        self.upload_stream();
        self.flush_materials();
        self.write_actor_frames();
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        self.queue.write_buffer(&self.lights_buf, 0, bytemuck::bytes_of(&lights));
        for (k, info) in self.lists.shadows.iter().enumerate().take(2) {
            if !info.redraw { continue; }
            let mut g = globals;
            g.light_vp = info.vp.to_cols_array_2d();
            self.queue.write_buffer(&self.shadow_globals[k].0, 0, bytemuck::bytes_of(&g));
        }
        self.post.write_bloom(&self.queue, self.bloom.threshold, self.bloom.strength, self.bloom.radius);
        self.prepare_overlay();

        let (tex, screen) = if let Some(surface) = &self.surface {
            let tex = match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => { surface.configure(&self.device, &self.config); self.restore_lists(opaque, trans, mask, casters, shadow_items); return; }
                _ => { self.restore_lists(opaque, trans, mask, casters, shadow_items); return; }
            };
            let screen = tex.texture.create_view(&Default::default());
            (Some(tex), screen)
        } else {
            let Some(h) = &self.headless else { self.restore_lists(opaque, trans, mask, casters, shadow_items); return; };
            (None, h.create_view(&Default::default()))
        };
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kit") });
        {
            let this: &Kit = &*self;
            // 影の型
            for (k, list) in shadow_items.iter().enumerate() {
                if k >= this.lists.shadows.len() { break; }
                if !this.lists.shadows[k].redraw { continue; }
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("shadow"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &this.shadow_maps[k].view, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }), stencil_ops: None }),
                    ..Default::default()
                });
                pass.set_bind_group(0, &this.shadow_globals[k].1, &[]);
                let mut st = DrawState::new();
                for it in list { this.draw_plain(&mut pass, it, &mut st); }
                for id in &this.lists.actor_cast {
                    if let Some(s) = this.actors.get(id) && let Some(a) = &s.actor { s.view.shadow(&mut pass, a); }
                    if let Some(s) = this.npc_actors.get(id) && let Some(a) = &s.actor { s.view.shadow(&mut pass, a); }
                }
            }
            // 自機の型
            if o.outline {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mask"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &this.t.mask, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Store }, depth_slice: None })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &this.t.mask_depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                    ..Default::default()
                });
                pass.set_bind_group(0, &this.globals_bg, &[]);
                let mut st = DrawState::new();
                for it in &mask { this.draw_plain(&mut pass, it, &mut st); }
                for id in &this.lists.actor_mask {
                    if let Some(s) = this.actors.get(id) && let Some(a) = &s.actor { s.view.draw_mask(&mut pass, a); }
                    if let Some(s) = this.npc_actors.get(id) && let Some(a) = &s.actor { s.view.draw_mask(&mut pass, a); }
                }
            }
            // 透過用の的（不透明な物だけ）とそのミップ。両面の透過する物があれば、その裏の面を重ねて解決し直し、ミップも作り直す（three と同じ順）
            if let Some((id, items, back)) = &trans_pass && let Some(view0) = &this.trans_view0 {
                let t = &this.textures[id.0 as usize];
                let rt = t.rt.as_ref().expect("透過用の的");
                // three: clear alpha が 1 未満なら白・0.5 で消す
                let clear = if this.clear_alpha < 1.0 { wgpu::Color { r: 1.0, g: 1.0, b: 1.0, a: 0.5 } } else { clear_of(o.clear) };
                // 裏の面を重ねるときは MSAA の色と奥行きを残す（three の MSAA の的は解決のあとも中身を持つ）
                let keep = if back.is_empty() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store };
                {
                    let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("transmission"),
                        color_attachments: &[Some(color_att(this.samples, &rt.msaa, view0, true, wgpu::LoadOp::Clear(clear), keep))],
                        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &rt.depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: keep }), stencil_ops: None }),
                        ..Default::default()
                    });
                    this.draw_main(&mut pass, items, &[], None);
                }
                if let Some(m) = &this.mips { m.record(&this.device, &mut enc, &t.tex, crate::texture::MipKind::Float); }
                if !back.is_empty() {
                    // 裏の面（材質は今の透過用の的を読む。的は解決先にしないので、同じ画像を読みながら書かない）
                    {
                        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("transmission back"),
                            color_attachments: &[Some(color_att(this.samples, &rt.msaa, view0, false, wgpu::LoadOp::Load, wgpu::StoreOp::Store))],
                            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &rt.depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                            ..Default::default()
                        });
                        this.draw_main(&mut pass, back, &[], None);
                    }
                    // 解決だけ（three の updateMultisampleRenderTarget。1 サンプルなら裏の面を的へ直接描いた）
                    if this.samples > 1 {
                        drop(enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                            label: Some("transmission resolve"),
                            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &rt.msaa, resolve_target: Some(view0), ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }, depth_slice: None })],
                            ..Default::default()
                        }));
                    }
                    if let Some(m) = &this.mips { m.record(&this.device, &mut enc, &t.tex, crate::texture::MipKind::Float); }
                }
            }
            // 主の的
            {
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("main"),
                    color_attachments: &[Some(color_att(this.samples, &this.t.msaa, &this.t.resolved, true, wgpu::LoadOp::Clear(this.main_clear(o.clear)), wgpu::StoreOp::Discard))],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &this.t.depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                    ..Default::default()
                });
                let hid = if o.outline && o.hidden { Some(hidden_list.as_slice()) } else { None };
                this.draw_main(&mut pass, &opaque, &trans, hid);
            }
            this.post.run(&mut enc, &this.t, &screen, this.bloom_enabled);
            this.overlay_gpu.draw(&mut enc, &screen, &this.overlay);
            after(&this.device, &this.queue, &mut enc, &screen, [this.config.width, this.config.height]);
        }
        self.queue.submit(Some(enc.finish()));
        if let Some(tex) = tex { self.queue.present(tex); }
        // three の shadow.matrix と scope.needsUpdate = false（影を落とす光が無いコマは頼みを残す）
        for (k, info) in self.lists.shadows.iter().enumerate().take(2) {
            if info.redraw { self.shadow_drawn[k] = Some((info.vp, info.map_size)); self.shadow_draws += 1; }
        }
        if !self.lists.shadows.is_empty() { self.shadow_needs_update = false; }
        self.restore_lists(opaque, trans, mask, casters, shadow_items);
    }

    /// 節点とその子の印（透過用の別描画で除く物）
    fn subtree_marks(&self, roots: &[NodeId]) -> Vec<bool> {
        let nodes = self.scene.nodes();
        let mut marks = vec![false; nodes.len()];
        let mut stack: Vec<u32> = roots.iter().filter(|r| self.scene.is_alive(**r)).map(|r| r.0).collect();
        while let Some(i) = stack.pop() {
            if std::mem::replace(&mut marks[i as usize], true) { continue; }
            stack.extend(nodes[i as usize].children().iter().map(|c| c.0));
        }
        marks
    }

    /// 影の型を描いた回数（方向の光 1 枚につき 1 回）と、1 枚目の型を描いたときの光の行列（奥行きは 0..1）。値の記録で three の shadowMap.render と比べる
    pub fn shadow_read(&self) -> (u64, Option<Mat4>) { (self.shadow_draws, self.shadow_drawn[0].map(|d| d.0)) }

    /// 列の入れ物を返す（毎コマの割り当てを減らす）
    fn restore_lists(&mut self, opaque: Vec<Item>, trans: Vec<Item>, mask: Vec<Item>, casters: Vec<Item>, shadow_items: [Vec<Item>; 2]) {
        self.lists.opaque = opaque;
        self.lists.trans = trans;
        self.lists.mask = mask;
        self.lists.casters = casters;
        self.lists.shadow_items = shadow_items;
    }

    /// 別のカメラで的へ描く（three の setRenderTarget と render。後処理なし・影の型はそのまま）。材質の map に貼れる
    pub fn render_to(&mut self, target: TexId, camera: &Camera) {
        let clear = clear_of(self.last_clear);
        self.draw_to_target(target, &RawView::from_camera(camera), clear, self.shadow_auto_update, None);
    }

    /// 行列を直接渡したカメラで場面を的（render_target）へ描く（three の Reflector の onBeforeRender の中の renderer.render。鏡・水面の映り込み）。
    /// 後処理なし・MSAA 4 倍（主の的と同じ。three の multisample 2 も 4 で描く）・色は線形のまま（kit.output によらない。three の的への描画と同じ）。
    /// 影の型は描かず、主の描画で前に描いた型とそのときの光の行列を使う（Reflector が shadowMap.autoUpdate を切るのと同じ）。
    /// opts.hide の節点（子も）はこの描画だけ隠す（鏡の板・その入れ物）。描く間隔はゲームが決める（呼んだときだけ描く）。
    /// 描いた的は Custom の材質の textures に入れて読む（tex0..tex5。WGSL では上が v = 0 なので、three の textureMatrix の uv は y を 1 - y にする）
    pub fn render_view_to(&mut self, target: TexId, view: &RawView, opts: &ViewDraw) {
        let mut kept = Vec::with_capacity(opts.hide.len());
        for &id in opts.hide { if self.scene.is_alive(id) { kept.push((id, self.scene[id].visible)); self.scene[id].visible = false; } }
        let c = opts.clear.unwrap_or(self.last_clear);
        let clear = wgpu::Color { r: c.r as f64, g: c.g as f64, b: c.b as f64, a: opts.clear_alpha as f64 };
        self.draw_to_target(target, view, clear, false, Some(1.0));
        for (id, v) in kept { self.scene[id].visible = v; }
    }

    /// render_to・render_view_to の中身。cam_mode は Globals.cam.w（None なら kit.output のまま）
    fn draw_to_target(&mut self, target: TexId, camera: &RawView, clear: wgpu::Color, shadow_redraw: bool, cam_mode: Option<f32>) {
        if self.released { return; }
        self.scene.update_world();
        self.drain_shader_checks();
        let (lights, mut globals) = self.collect(camera, shadow_redraw);
        self.ensure_shadow_maps();
        let (w, h) = self.tex_size(target);
        globals.misc[1] = h as f32;
        globals.misc[2] = w as f32;
        if let Some(m) = cam_mode { globals.cam[3] = m; }
        self.lists.stream.clear();
        let (o_list, t_list) = (std::mem::take(&mut self.lists.opaque), std::mem::take(&mut self.lists.trans));
        let opaque = self.prepare_list(o_list);
        let trans = self.prepare_list(t_list);
        self.upload_stream();
        self.flush_materials();
        self.write_actor_frames();
        self.queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        self.queue.write_buffer(&self.lights_buf, 0, bytemuck::bytes_of(&lights));
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("kit rt") });
        {
            let this: &Kit = &*self;
            let t = &this.textures[target.0 as usize];
            let rt = t.rt.as_ref().expect("render_target で作った的だけに描ける");
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rt"),
                color_attachments: &[Some(color_att(this.samples, &rt.msaa, &t.view, true, wgpu::LoadOp::Clear(clear), wgpu::StoreOp::Discard))],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &rt.depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                ..Default::default()
            });
            this.draw_main(&mut pass, &opaque, &trans, None);
        }
        self.queue.submit(Some(enc.finish()));
        self.lists.opaque = opaque;
        self.lists.trans = trans;
    }

    /// 的の画素を読む（three の readRenderTargetPixels。上の行が先。8 ビットの線形で 0..1 に切る）
    #[cfg(target_arch = "wasm32")]
    pub fn read_target(&self, target: TexId) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + use<> {
        let t = &self.textures[target.0 as usize];
        let (w, h) = (t.w, t.h);
        let row = (w * 8 + 255) / 256 * 256;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor { label: Some("read"), size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("read") });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &t.tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: Some(h) } },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(enc.finish()));
        async move {
            let (promise, resolve) = {
                let mut res: Option<js_sys::Function> = None;
                let p = js_sys::Promise::new(&mut |ok, _| { res = Some(ok); });
                (p, res.unwrap())
            };
            buf.slice(..).map_async(wgpu::MapMode::Read, move |_| { let _ = resolve.call0(&wasm_bindgen::JsValue::NULL); });
            wasm_bindgen_futures::JsFuture::from(promise).await.map_err(|e| format!("{e:?}"))?;
            let data = buf.slice(..).get_mapped_range().map_err(|e| format!("{e:?}"))?;
            let mut out = vec![0u8; (w * h * 4) as usize];
            for y in 0..h as usize {
                let src = &data[y * row as usize..];
                for x in 0..w as usize {
                    for c in 0..4 {
                        let v = half_to_f32(u16::from_le_bytes([src[(x * 4 + c) * 2], src[(x * 4 + c) * 2 + 1]]));
                        out[(y * w as usize + x) * 4 + c] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                    }
                }
            }
            drop(data);
            buf.unmap();
            Ok(out)
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn half_to_f32(h: u16) -> f32 {
    let s = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = ((h >> 10) & 0x1f) as i32;
    let f = (h & 0x3ff) as f32;
    match e {
        0 => s * f * 2f32.powi(-24),
        31 => if f == 0.0 { s * f32::INFINITY } else { f32::NAN },
        _ => s * (1.0 + f / 1024.0) * 2f32.powi(e - 15),
    }
}

fn srgb_of(c: f32) -> f32 { if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(0.41666) - 0.055 } }
impl Kit {
    /// 主の的のクリア（直接描くときは sRGB の値。three の setClear と同じ。premultiplied の alpha）
    fn main_clear(&self, c: Color) -> wgpu::Color {
        let a = self.clear_alpha as f64;
        if self.output == Output::Linear { let k = clear_of(c); return wgpu::Color { a, ..k }; }
        wgpu::Color { r: srgb_of(c.r) as f64 * a, g: srgb_of(c.g) as f64 * a, b: srgb_of(c.b) as f64 * a, a }
    }
}
fn clear_of(c: Color) -> wgpu::Color { wgpu::Color { r: c.r as f64, g: c.g as f64, b: c.b as f64, a: 1.0 } }

/// render_view_to の指定
#[derive(Clone, Copy, Debug, Default)]
pub struct ViewDraw<'a> {
    /// この描画だけ隠す節点（子も）
    pub hide: &'a [NodeId],
    /// 消す色（線形。None は主の描画の clear）
    pub clear: Option<Color>,
    /// 消す alpha（既定 0。three の Reflector の的は alpha 0 で消える。1 にすれば不透明）
    pub clear_alpha: f32,
}

struct DrawState { pipe: u32, bg: u32, off: u32, geo: u32, stream: bool }
impl DrawState { fn new() -> DrawState { DrawState { pipe: u32::MAX, bg: u32::MAX, off: u32::MAX, geo: u32::MAX, stream: false } } }

#[cfg(test)]
mod tests {
    use super::*;

    fn item(uid: u32, mat: u32, ro: i32, go: i32, z: f32) -> Item {
        Item { node: uid, uid, mat, geo: 0, start: 0, count: 0, kind: ItemKind::Mesh, group_order: go, render_order: ro, z, cw: false, first: 0, inst: 1, pipe: 0, bg: 0, skel: 0 }
    }

    /// three の painterSortStable と reversePainterSortStable
    #[test]
    fn sort_matches_three() {
        let mut a = vec![item(1, 5, 0, 0, 3.0), item(2, 4, 0, 0, 9.0), item(3, 4, 0, 0, 1.0), item(4, 1, 1, 0, 0.0), item(5, 9, -1, 0, 0.0), item(6, 4, 0, 0, 1.0), item(7, 0, 0, 1, 0.0)];
        a.sort_by(opaque_order);
        assert_eq!(a.iter().map(|i| i.uid).collect::<Vec<_>>(), vec![5, 3, 6, 2, 1, 4, 7]);
        let mut t = vec![item(1, 5, 0, 0, 3.0), item(2, 4, 0, 0, 9.0), item(3, 4, 0, 0, 1.0), item(4, 1, 1, 0, 0.0), item(5, 9, -1, 0, 0.0), item(6, 4, 0, 0, 9.0)];
        t.sort_by(trans_order);
        assert_eq!(t.iter().map(|i| i.uid).collect::<Vec<_>>(), vec![5, 2, 6, 1, 3, 4]);
    }

    /// 奥行きが NaN の物が混ざっても並びの決まりが矛盾しない（Rust の sort が panic しない）。NaN は最も奥
    #[test]
    fn sort_with_nan_depth() {
        let mut a: Vec<Item> = (0..40).map(|i| item(i, i % 3, 0, 0, if i % 5 == 0 { f32::NAN } else { (i * 7 % 11) as f32 })).collect();
        a.sort_by(opaque_order);
        let mut t = a.clone();
        t.sort_by(trans_order);
        // 同じ材質の中では、NaN は不透明では最後、半透明では最初
        let m0: Vec<f32> = a.iter().filter(|i| i.mat == 0).map(|i| i.z).collect();
        assert!(m0.last().unwrap().is_nan());
        assert!(t.first().unwrap().z.is_nan());
        let mut b = vec![item(1, 0, 0, 0, 2.0), item(2, 0, 0, 0, 1.0)];
        b.sort_by(opaque_order);
        assert_eq!(b.iter().map(|i| i.uid).collect::<Vec<_>>(), vec![2, 1]);
    }

    #[test]
    fn frustum_culls_spheres() {
        let mut c = Camera::new(60.0, 1.0, 0.5, 100.0);
        c.position = Vec3::new(0.0, 0.0, 10.0);
        c.look_at(Vec3::ZERO);
        let f = Frustum::new(c.view_proj());
        assert!(f.intersects(Vec3::ZERO, 1.0));
        assert!(!f.intersects(Vec3::new(0.0, 0.0, 20.0), 1.0));
        assert!(!f.intersects(Vec3::new(0.0, 0.0, -200.0), 1.0));
        assert!(!f.intersects(Vec3::new(100.0, 0.0, 0.0), 1.0));
        assert!(f.intersects(Vec3::new(100.0, 0.0, 0.0), 200.0));
    }

    /// 描き直さないコマは前に描いたときの光の行列のまま。頼まれたら今の光で描く。まだ描いていない・的の大きさが変わった物は頼みがなくても描く
    #[test]
    fn kept_shadow_matrix() {
        let m = Mat4::from_scale(Vec3::splat(2.0));
        assert_eq!(kept_shadow(false, Some((m, 1024)), 1024), Some(m));
        assert_eq!(kept_shadow(true, Some((m, 1024)), 1024), None);
        assert_eq!(kept_shadow(false, None, 1024), None);
        assert_eq!(kept_shadow(false, Some((m, 1024)), 2048), None);
    }

    /// 影の行列: 視錐台の中心が的の中心（0, 0）になり、near・far が 0..1
    #[test]
    fn shadow_matrix_range() {
        let (eye, target) = (Vec3::new(6.0, 16.0, 9.0), Vec3::ZERO);
        let lv = Mat4::from_mat3_translation(crate::scene::look_at_rotation(eye, target, Vec3::Y), eye).inverse();
        let lp = Mat4::orthographic_rh(-17.0, 17.0, -15.0, 15.0, 1.0, 60.0);
        let vp = lp * lv;
        let c = vp.project_point3(target);
        assert!(c.x.abs() < 1e-4 && c.y.abs() < 1e-4);
        let d = eye.length();
        assert!((c.z - (d - 1.0) / 59.0).abs() < 1e-4);
    }
}

/// 色の的の付け方（kit.samples）。MSAA なら MSAA の色へ描き、resolve なら target へ解く（store は MSAA の色を残すか）。
/// 1 サンプル（Aa::Off・Smaa）なら target へ直接描いて残す
fn color_att<'a>(samples: u32, msaa: &'a wgpu::TextureView, target: &'a wgpu::TextureView, resolve: bool, load: wgpu::LoadOp<wgpu::Color>, store: wgpu::StoreOp) -> wgpu::RenderPassColorAttachment<'a> {
    if samples > 1 {
        wgpu::RenderPassColorAttachment { view: msaa, resolve_target: resolve.then_some(target), ops: wgpu::Operations { load, store }, depth_slice: None }
    } else {
        wgpu::RenderPassColorAttachment { view: target, resolve_target: None, ops: wgpu::Operations { load, store: wgpu::StoreOp::Store }, depth_slice: None }
    }
}
