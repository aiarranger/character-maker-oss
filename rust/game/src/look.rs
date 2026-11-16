// 下見の陰影（スタジオの見え方）。模型が作った光なしの材質（Basic・toneMapped なし）を、同じ色と絵のまま陰影つきの材質（shaders/look.wgsl）に
// 差し替えて描く。元の材質は残し、模型と書き出しは元の材質を読み書きする（書き出す GLB は今までどおり KHR_materials_unlit）。
// 毎コマ: 模型の下の節点を見て、まだ差し替えていない物を差し替え、元の材質の値が変わっていたら写し、頭の球の位置を渡す。
use glam::{Mat4, Vec3};
use island_kit::{Blending, CustomShader, Kit, MatId, MatKind, Material, NodeId, NodeKind, TexId};
use std::collections::{HashMap, HashSet};

const WGSL: &str = include_str!("../shaders/look.wgsl");
/// 体の陰の濃さ・頭の陰の濃さ・縁の光の強さ・陰のぼかしの幅
const SHADE: [f32; 4] = [0.34, 0.13, 0.10, 0.22];
/// 陰の色（少し青い）
const SHADOW: [f32; 3] = [0.60, 0.62, 0.80];
const RIM: [f32; 3] = [1.0, 0.96, 0.90];
/// カメラから見た光のある向き（左上の手前）
const LIGHT: [f32; 3] = [-0.48, 0.52, 0.71];

/// 模型が Stage に教えること
pub struct Subject {
    pub root: NodeId,
    /// 頭の骨の世界の行列（無ければ頭の球なし）
    pub head: Option<Mat4>,
    /// 陰影を付けない材質（光る画面など）
    pub flat: Vec<MatId>,
}

/// 差し替えた材質が写した元の値（変わったら作り直す）
#[derive(PartialEq, Clone)]
struct Sig { color: [f32; 3], opacity: f32, map: Option<u32>, transparent: bool, depth_write: bool, alpha_test: f32, side: u8, vertex_colors: bool, visible: bool, premult: bool, offset: Option<(f32, f32)>, additive: bool }
fn sig(m: &Material) -> Sig {
    Sig { color: [m.color.r, m.color.g, m.color.b], opacity: m.opacity, map: m.map.map(|t| t.0), transparent: m.transparent, depth_write: m.depth_write, alpha_test: m.alpha_test,
        side: m.side as u8, vertex_colors: m.vertex_colors, visible: m.visible, premult: m.premultiplied_alpha, offset: m.polygon_offset, additive: !matches!(m.blending, Blending::Normal) }
}

#[derive(Default)]
pub struct Look {
    /// (元の材質, 骨付きか) → (差し替えた材質, 写した値)
    twins: HashMap<(u32, bool), (MatId, Sig)>,
    /// 差し替えた材質 → 元の材質
    origin: HashMap<u32, u32>,
    white: Option<TexId>,
    /// 頭の球（頭の骨から見た中心と半径）。模型を作り直す・設定を当てるたびに測り直す
    head: Option<(Vec3, f32)>,
    pub measure: bool,
}

impl Look {
    /// 模型を作り直した（前の模型の材質はもう使わない）
    pub fn reset(&mut self) { self.twins.clear(); self.origin.clear(); self.head = None; self.measure = true; }

    /// 頭の球を測り直す（head_box は今の姿の頭の箱。世界）
    pub fn set_head(&mut self, head: Option<Mat4>, head_box: Option<(Vec3, Vec3)>) {
        self.measure = false;
        self.head = match (head, head_box) {
            (Some(m), Some((lo, hi))) if lo.x <= hi.x => {
                let size = hi - lo;
                // 髪で箱が伸びても顔を包む大きさ: いちばん短い辺の半分
                let r = 0.5 * size.x.min(size.y).min(size.z);
                // 中心は頭の骨の高さから半径ぶん上（長い髪で箱の中心が下がっても顔に合う）
                let bone = m.w_axis.truncate();
                let centre = Vec3::new((lo.x + hi.x) * 0.5, (bone.y + r * 0.85).min(hi.y - r * 0.9).max(bone.y), bone.z);
                Some((m.inverse().transform_point3(centre), r))
            }
            _ => None,
        };
    }

    fn uniform(&self, m: &Material, head: [f32; 4], flat: bool) -> Vec<u8> {
        let shade = if flat { [0.0, 0.0, 0.0, SHADE[3]] } else { SHADE };
        let v: [[f32; 4]; 7] = [
            [m.color.r, m.color.g, m.color.b, m.opacity],
            [m.alpha_test, if m.vertex_colors { 1.0 } else { 0.0 }, if m.transparent { 1.0 } else { 0.0 }, if m.premultiplied_alpha { 1.0 } else { 0.0 }],
            head, shade, [SHADOW[0], SHADOW[1], SHADOW[2], 0.0], [RIM[0], RIM[1], RIM[2], 0.0], [LIGHT[0], LIGHT[1], LIGHT[2], 0.0],
        ];
        v.iter().flatten().flat_map(|f| f.to_le_bytes()).collect()
    }

    fn twin_of(&self, kit: &mut Kit, src: &Material, skinned: bool, head: [f32; 4], flat: bool, white: TexId) -> Material {
        let mut m = src.clone();
        m.kind = MatKind::Custom(CustomShader {
            label: if skinned { "look skinned".into() } else { "look".into() }, wgsl: WGSL.into(),
            vs: if skinned { "vs_look_skinned".into() } else { "vs_look".into() }, fs: "fs_look".into(),
            uniform: self.uniform(src, head, flat), textures: vec![src.map.unwrap_or(white)], ..CustomShader::default()
        });
        m.fog = false;
        let _ = kit;
        m
    }

    /// 毎コマ（描く前）
    pub fn refresh(&mut self, kit: &mut Kit, s: &Subject) {
        let white = *self.white.get_or_insert_with(|| kit.data_texture(1, 1, &[255, 255, 255, 255], true));
        let head = match (self.head, s.head) {
            (Some((c, r)), Some(m)) => { let p = m.transform_point3(c); let scale = m.x_axis.truncate().length(); [p.x, p.y, p.z, r * if scale > 1e-4 { 1.0 } else { 0.0 }] }
            _ => [0.0; 4],
        };
        let flat: HashSet<u32> = s.flat.iter().map(|m| m.0).collect();
        // 模型の下の、形のある節点
        let mut nodes: Vec<(NodeId, MatId, bool)> = Vec::new();
        kit.scene.traverse(s.root, &mut |scene, id| match &scene[id].kind {
            NodeKind::Mesh { mats, multi: false, .. } if mats.len() == 1 => nodes.push((id, mats[0], false)),
            NodeKind::Skinned(sk) => nodes.push((id, sk.mat, true)),
            _ => {}
        });
        for (node, current, skinned) in nodes {
            let orig = self.origin.get(&current.0).copied().unwrap_or(current.0);
            let src = kit.mat(MatId(orig)).clone();
            if !matches!(src.kind, MatKind::Basic) || src.tone_mapped { continue; }
            let now = sig(&src);
            let twin = match self.twins.get(&(orig, skinned)).cloned() {
                Some((id, old)) => {
                    if old != now { let m = self.twin_of(kit, &src, skinned, head, flat.contains(&orig), white); *kit.mat_mut(id) = m; self.twins.insert((orig, skinned), (id, now)); }
                    id
                }
                None => {
                    let m = self.twin_of(kit, &src, skinned, head, flat.contains(&orig), white);
                    let id = kit.material(m);
                    self.twins.insert((orig, skinned), (id, now));
                    self.origin.insert(id.0, orig);
                    id
                }
            };
            if twin != current {
                match &mut kit.scene[node].kind {
                    NodeKind::Mesh { mats, .. } => mats[0] = twin,
                    NodeKind::Skinned(sk) => sk.mat = twin,
                    _ => {}
                }
            }
        }
        // 頭の球の位置を渡す（毎コマ）
        let ids: Vec<(u32, MatId)> = self.twins.iter().map(|(k, v)| (k.0, v.0)).collect();
        for (orig, id) in ids {
            let bytes = self.uniform(kit.mat(MatId(orig)), head, flat.contains(&orig));
            kit.set_uniform(id, &bytes);
        }
    }
}

/// 書き出す GLB に書く、動作の再生のしかた（1 回で終わるか・終わったら何へ戻るか）。名前の正本は lib/character-maker/actions.ts
pub fn playback() -> serde_json::Value {
    serde_json::json!({
        "sword_attack": { "loop": false, "next": "sword_idle" }, "hit": { "loop": false }, "face_hurt": { "loop": true },
        "jump": { "loop": false, "next": "idle" }, "dodge": { "loop": false, "next": "idle" }, "skill": { "loop": false, "next": "idle" },
        "punch": { "loop": false, "next": "idle" }, "interact": { "loop": false, "next": "idle" }, "down": { "loop": false, "hold": true },
        "dance": { "loop": true }, "talk": { "loop": true }, "crouch": { "loop": true },
        "restDown": { "loop": false, "next": "restLoop" }, "restLoop": { "loop": true }, "restUp": { "loop": false, "next": "idle" }, "eat": { "loop": true },
        "banzai": { "loop": false, "next": "idle" }, "greet": { "loop": false, "next": "idle" }, "spinKick": { "loop": false, "next": "idle" },
    })
}
