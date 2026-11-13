// アメコミの ＞＜（今の版は lib/character-maker/preset-hurt-face.ts の createPresetHurtFace の comic の枝）。
// 頭の骨の子のノード maker-hurt-face（ふだんは scale 0）に、閉じた目の板（目のメッシュの形を肌の絵で覆う）と ＞ の板を置く。
// 目の形（モーフ）が変わったら板を作り直す（update）。ファンタジーは表情のモーフ（expressions.rs）なので、ここは使わない。
use character_actor::glb::Glb;
use glam::{Mat4, Vec3};
use island_kit::geometry::Geometry;
use island_kit::{Color, GeoId, Kit, MatId, Material, NodeId, Side, TexId};

pub const NODE: &str = "maker-hurt-face";
const COMIC_LIFT: f32 = 0.0012;
const COMIC_CAP: f32 = 0.004;
const MARK: (f32, f32, f32) = (1.45, 0.75, 0.0008);

pub struct HurtFace {
    pub node: NodeId,
    cover: (NodeId, GeoId),
    marks: (NodeId, GeoId),
    to_head: Mat4,
    eyes: usize, face: usize, lids: usize,
    key: String,
    /// 書き出し用の今の形
    pub cover_geo: Option<Geometry>,
    pub marks_geo: Option<Geometry>,
    pub cover_mat: MatId,
    pub marks_mat: MatId,
}

fn morphed(g: &Glb, node: usize, weights: &[f32]) -> Vec<[f32; 3]> {
    let p = &g.meshes[g.nodes[node].mesh.unwrap()].primitives[0];
    p.positions.iter().enumerate().map(|(i, v)| {
        let mut q = *v;
        for (k, w) in weights.iter().enumerate() { if *w != 0.0 && let Some(d) = p.morph_pos.get(k) { for c in 0..3 { q[c] += d[i][c] * w; } } }
        q
    }).collect()
}

impl HurtFace {
    /// head: 頭の骨の番号（GLB の節点）。head_node: 頭の骨の描く節点。cover・marks: TS が canvas で作った絵
    pub fn new(kit: &mut Kit, g: &Glb, head: usize, head_node: NodeId, cover: TexId, marks: TexId, eyes_side: Side) -> Result<HurtFace, String> {
        let find = |name: &str| g.nodes.iter().position(|n| n.name == name && n.skin.is_some() && n.mesh.is_some());
        let (eyes, face, lids) = (find("maker-eyes"), find("maker-head"), find("maker-brow"));
        let (Some(eyes), Some(face), Some(lids)) = (eyes, face, lids) else { return Err("目のメッシュが見つかりません。".into()) };
        let skin = &g.skins[g.nodes[eyes].skin.unwrap()];
        let hi = skin.joints.iter().position(|j| *j == head).ok_or("目のメッシュが頭の骨に付いていません。")?;
        // 目のメッシュは頭の骨だけに付いている（全頂点）。頭の骨の空間へ移す行列は姿勢によらない（boneInverse × bindMatrix）
        let bind = g.world_matrices()[eyes];
        let to_head = skin.inverse_bind[hi] * bind;
        let node = kit.scene.group(Some(head_node));
        kit.scene[node].name = NODE.into();
        kit.scene[node].scale = Vec3::ZERO;
        let empty = Geometry { positions: vec![[0.0; 3]; 3], normals: vec![[0.0, 0.0, 1.0]; 3], uvs: vec![[0.0; 2]; 3], colors: Vec::new(), sway: Vec::new(), indices: None, groups: Vec::new() };
        let mut cm = Material::basic();
        cm.map = Some(cover); cm.side = eyes_side; cm.tone_mapped = false; cm.polygon_offset = Some((-2.0, -2.0)); cm.color = Color { r: 1.0, g: 1.0, b: 1.0 };
        let mut mm = Material::basic();
        mm.map = Some(marks); mm.transparent = true; mm.depth_write = false; mm.side = Side::Double; mm.tone_mapped = false;
        let (cover_mat, marks_mat) = (kit.material(cm), kit.material(mm));
        let cg = kit.geometry(&empty); let mg = kit.geometry(&empty);
        let cn = kit.scene.mesh(Some(node), cg, cover_mat); kit.scene[cn].name = format!("{NODE}-cover"); kit.scene[cn].frustum_culled = false;
        let mn = kit.scene.mesh(Some(node), mg, marks_mat); kit.scene[mn].name = format!("{NODE}-marks"); kit.scene[mn].frustum_culled = false; kit.scene[mn].render_order = 2;
        Ok(HurtFace { node, cover: (cn, cg), marks: (mn, mg), to_head, eyes, face, lids, key: String::new(), cover_geo: None, marks_geo: None, cover_mat, marks_mat })
    }

    /// 目の形が変わったら板を作り直す。weights: GLB の節点 → 今のモーフの重み
    pub fn update(&mut self, kit: &mut Kit, g: &Glb, weights: &dyn Fn(usize) -> Vec<f32>) {
        let (we, wf, wl) = (weights(self.eyes), weights(self.face), weights(self.lids));
        let key = we.iter().chain(&wf).chain(&wl).map(|v| format!("{v:.4}")).collect::<Vec<_>>().join(",");
        if key == self.key { return; }
        self.key = key;
        let bind = morphed(g, self.eyes, &we);
        let surfaces = [morphed(g, self.face, &wf), morphed(g, self.lids, &wl)];
        let src = &g.meshes[g.nodes[self.eyes].mesh.unwrap()].primitives[0];
        let positions: Vec<[f32; 3]> = bind.iter().map(|v| self.to_head.transform_point3(Vec3::new(v[0], v[1], v[2] + COMIC_LIFT)).to_array()).collect();
        let n = positions.len();
        let cover = Geometry { positions, normals: vec![[0.0, 0.0, 1.0]; n], uvs: src.uvs.clone(), colors: Vec::new(), sway: Vec::new(), indices: if src.indices.is_empty() { None } else { Some(src.indices.clone()) }, groups: Vec::new() };
        kit.update_geometry(self.cover.1, &cover);
        self.cover_geo = Some(cover);
        let (mut mp, mut mu, mut mi) = (Vec::new(), Vec::new(), Vec::new());
        for side in [-1.0f32, 1.0] {
            let sign = |x: f32| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 };
            let mut front = f32::NEG_INFINITY;
            for v in &bind { if sign(v[0]) == side { front = front.max(v[2]); } }
            let (mut x0, mut x1, mut y0, mut y1) = (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, f32::NEG_INFINITY);
            for v in &bind { if sign(v[0]) != side || v[2] < front - COMIC_CAP { continue; } x0 = x0.min(v[0]); x1 = x1.max(v[0]); y0 = y0.min(v[1]); y1 = y1.max(v[1]); }
            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let width = (x1 - x0) * MARK.0; let height = width * MARK.1;
            let mut z = front;
            for s in &surfaces { for v in s { if (v[0] - cx).abs() < width * 0.42 && (v[1] - cy).abs() < height * 0.42 { z = z.max(v[2]); } } }
            z += MARK.2;
            let base = (mp.len()) as u32;
            for (x, y) in [(cx - width / 2.0, cy - height / 2.0), (cx + width / 2.0, cy - height / 2.0), (cx + width / 2.0, cy + height / 2.0), (cx - width / 2.0, cy + height / 2.0)] {
                mp.push(self.to_head.transform_point3(Vec3::new(x, y, z)).to_array());
                let u = if side < 0.0 { (x - (cx - width / 2.0)) / width } else { ((cx + width / 2.0) - x) / width };
                mu.push([u, ((cy + height / 2.0) - y) / height]);
            }
            mi.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let k = mp.len();
        let marks = Geometry { positions: mp, normals: vec![[0.0, 0.0, 1.0]; k], uvs: mu, colors: Vec::new(), sway: Vec::new(), indices: Some(mi), groups: Vec::new() };
        kit.update_geometry(self.marks.1, &marks);
        self.marks_geo = Some(marks);
    }

    pub fn show(&self, kit: &mut Kit, on: bool) { kit.scene[self.node].scale = Vec3::splat(if on { 1.0 } else { 0.0 }); }
}
