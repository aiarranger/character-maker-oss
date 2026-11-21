// 走る魚（Fish Kit、CC0。public/models/fish/README.md）。走路の上を泳いで進む。
// 模型は口先が +X・背が +Y・メートル。ここで口先を +Z・体の真ん中を原点にそろえ、走者の大きさへ拡大する。
// うねりは Fish Kit の fishH と同じ式（魚群繚乱の fish.wgsl と同じ）: 包絡 A(s) = e0 + e1 s + e2 s^2（s = 0 口先〜1 尾の先）× sin(2π(s/λ − cyc))。
// 値は節点の extras.swim と extras.fishRig。CPU で各走者の可動部を動かし、体のうねりを重ねる。
use super::materials::{SoftOptions, soft_material};
use crate::fish_rig::{FishRig, RigMotion};
use character_actor::glb::Glb;
use glam::Vec3;
use island_kit::geometry::Geometry;
use island_kit::{GeoId, Kit, MatId, NodeId, Side};
use std::sync::Arc;

/// 走者にする魚（docs/gyogyo-spec.md の「走る人たち」）
pub struct FishDef { pub id: &'static str, pub name: &'static str, pub file: &'static str, /// 走者としての全長（m、場の大きさ）
    pub length: f32 }

pub const FISH: [FishDef; 10] = [
    FishDef { id: "madai", name: "マダイ", file: "madai", length: 2.6 },
    FishDef { id: "buri", name: "ブリ", file: "buri", length: 3.1 },
    FishDef { id: "kihada", name: "キハダ", file: "kihada", length: 3.4 },
    FishDef { id: "saba", name: "サバ", file: "saba", length: 2.7 },
    FishDef { id: "aji", name: "アジ", file: "aji", length: 2.5 },
    FishDef { id: "kasago", name: "カサゴ", file: "kasago", length: 2.3 },
    FishDef { id: "tachiuo", name: "タチウオ", file: "tachiuo", length: 3.9 },
    FishDef { id: "magochi", name: "マゴチ", file: "magochi", length: 2.9 },
    FishDef { id: "kurodai", name: "クロダイ", file: "kurodai", length: 2.6 },
    FishDef { id: "suzuki", name: "シーバス", file: "suzuki", length: 3.0 },
];

pub fn fish_def(id: &str) -> Option<&'static FishDef> { FISH.iter().find(|f| f.id == id) }
pub fn fish_url(file: &str) -> String { format!("./models/fish/{file}.glb") }

/// 魚・イカの材質: 写真の色を自分で少し光らせ（emissive_map ＝ 写真）、輪郭の光を強める。
/// 写真の魚は銀・灰色が多く、夕暮れ・夜のコースの青い光に引っぱられて暗く沈むため
pub const CREATURE_GLOW: f32 = 0.22;
pub fn creature_material(tex: island_kit::TexId) -> island_kit::Material {
    let mut m = soft_material(SoftOptions { map: Some(tex), rim: 1.3, emissive: island_kit::Color::WHITE, emissive_intensity: CREATURE_GLOW, ..Default::default() });
    m.emissive_map = Some(tex);
    m
}

/// 写真の色を濃くする（彩度 ×VIVID_SAT、明るさ ×VIVID_GAIN）。写真の魚は色が淡く、走路の上で背景に沈むため
const VIVID_SAT: f32 = 1.55;
const VIVID_GAIN: f32 = 1.06;
pub fn vivid(rgba: &mut [u8]) {
    for p in rgba.chunks_exact_mut(4) {
        let c = [p[0] as f32, p[1] as f32, p[2] as f32];
        let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        for k in 0..3 { p[k] = ((l + (c[k] - l) * VIVID_SAT) * VIVID_GAIN).clamp(0.0, 255.0) as u8; }
    }
}

/// 模型座標での形・可動部と、走者への換算・うねりの値
pub struct FishShape {
    rig: Option<FishRig>,
    center: Vec3,
    inv_len: f32,
    pos: Vec<Vec3>,
    nor: Vec<Vec3>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    tex: (u32, u32, Vec<u8>),
    /// 体の高さ・幅（長さ 1 に対して）
    pub height: f32,
    lambda: f32,
    freq: f32,
    amp: f32,
    env: [f32; 3],
}

fn turn(v: Vec3) -> Vec3 { Vec3::new(-v.z, v.y, v.x) }

/// glb から元キットと同じ模型座標で形を読む。
pub fn shape_of(g: &Glb) -> Result<FishShape, String> {
    let ni = g.nodes.iter().position(|n| n.mesh.is_some()).ok_or("魚: メッシュが無い")?;
    let p = g.meshes[g.nodes[ni].mesh.unwrap()].primitives.first().ok_or("魚: 面が無い")?;
    let (lo, hi) = p.positions.iter().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(a, b), q| (a.min(Vec3::from(*q)), b.max(Vec3::from(*q))));
    let c = (lo + hi) * 0.5;
    let len = (hi.x - lo.x).max(1e-4);
    let pos = p.positions.iter().copied().map(Vec3::from).collect();
    let nor = p.normals.iter().copied().map(Vec3::from).collect();
    let extras = &g.nodes[ni].extras;
    let rig = FishRig::load(p, extras)?;
    let tex = p.material.and_then(|m| g.materials[m].texture).map(|t| { let im = &g.images[t]; (im.width, im.height, im.rgba.clone()) }).ok_or("魚: 模様が無い")?;
    let sw = &extras["swim"];
    let f = |k: &str, d: f64| sw.get(k).and_then(|v| v.as_f64()).unwrap_or(d) as f32;
    let env = sw.get("env").and_then(|e| e.as_array()).map(|a| [0, 1, 2].map(|i| a.get(i).and_then(|v| v.as_f64()).unwrap_or(0.0) as f32)).unwrap_or([0.015, -0.07, 0.16]);
    Ok(FishShape {
        rig, center: c, inv_len: len.recip(), pos, nor, uvs: p.uvs.clone(), indices: p.indices.clone(), tex,
        height: (hi.y - lo.y) / len, lambda: f("lambda", 1.0).max(0.2), freq: f("freq", 1.6), amp: f("amp", 0.7), env,
    })
}

/// 走りの合図（runner.rs が走者の状態から作る）
#[derive(Clone, Copy, Default)]
pub struct Drive { pub speed: f64, pub dash: f64, pub air: bool, pub hurt: f64, pub joy: bool, pub showcase: bool }

pub struct FishBody {
    pub mesh: NodeId,
    geo: GeoId,
    pub mat: MatId,
    shape: Arc<FishShape>,
    g: Geometry,
    motion: RigMotion,
    len: f32,
    cyc: f64,
    amp: f64,
    bend: f64,
    pub length: f32,
}

impl FishBody {
    /// parent の下に体を作る。len は parent の空間での全長
    pub fn new(kit: &mut Kit, parent: NodeId, shape: &Arc<FishShape>, len: f32) -> FishBody {
        let (w, h, rgba) = &shape.tex;
        let mut rgba = rgba.clone();
        vivid(&mut rgba);
        let tex = kit.data_texture(*w, *h, &rgba, true);
        let mut m = creature_material(tex);
        m.side = Side::Double;
        // ひれは切り抜いた板（アルファで抜く）
        m.alpha_test = 0.5;
        let mat = kit.material(m);
        let g = Geometry {
            positions: shape.pos.iter().map(|p| (turn(*p - shape.center) * shape.inv_len * len).to_array()).collect(),
            normals: shape.nor.iter().map(|n| turn(*n).to_array()).collect(),
            uvs: shape.uvs.clone(), colors: Vec::new(), sway: Vec::new(),
            indices: Some(shape.indices.clone()), groups: Vec::new(),
        };
        let geo = kit.geometry(&g);
        let mesh = kit.scene.mesh(Some(parent), geo, mat);
        kit.scene[mesh].cast_shadow = true;
        kit.scene[mesh].receive_shadow = true;
        FishBody { mesh, geo, mat, shape: shape.clone(), g, motion: RigMotion::default(), len, cyc: 0.0, amp: shape.amp as f64, bend: 0.0, length: len }
    }

    /// 体の高さ（parent の空間）
    pub fn height(&self) -> f32 { self.shape.height * self.len }

    /// 毎コマ: 尾を振って泳ぐ。速いほど速く大きく、ダッシュは全力、よろけは身をよじる、喜びは小刻みに
    pub fn update(&mut self, kit: &mut Kit, dt: f64, now: f64, d: &Drive) {
        let s = self.shape.as_ref();
        let pace = (d.speed / 14.0).clamp(0.0, 1.6);
        let mut freq = s.freq as f64 * (0.55 + pace * 1.1) * (1.0 + d.dash * 0.7);
        let mut amp = s.amp as f64 * (0.8 + pace * 0.35 + d.dash * 0.45);
        let mut bend = 0.0;
        if d.air { freq *= 0.6; amp *= 0.7; bend = 0.25 * (now * 3.0).sin(); }
        if d.hurt > 0.0 { freq = 6.0; amp = s.amp as f64 * (1.4 + d.hurt); bend = 0.9 * d.hurt * (now * 13.0).sin(); }
        if d.joy { freq = 4.5; amp = s.amp as f64 * 1.5; }
        self.cyc += dt * freq;
        self.amp += (amp - self.amp) * (dt * 6.0).min(1.0);
        self.bend += (bend - self.bend) * (dt * 10.0).min(1.0);
        let (cyc, amp, bend) = (self.cyc as f32, self.amp as f32, self.bend as f32);
        let wave = |u: f32| { let a = s.env[0] + s.env[1] * u + s.env[2] * u * u; amp * a * (std::f32::consts::TAU * (u / s.lambda - cyc)).sin() + bend * u * u * 0.45 };
        let pose = s.rig.as_ref().map(|r| self.motion.advance(r, dt as f32, cyc, amp,
            d.speed as f32 / self.length, bend, d.air, d.showcase));
        for (k, (&raw, &normal)) in s.pos.iter().zip(&s.nor).enumerate() {
            let (local, normal) = match &pose {
                Some(p) => p.deform(k, raw, normal),
                None => (raw, normal),
            };
            let p = turn(local - s.center) * s.inv_len;
            let n = turn(normal);
            let u = (0.5 - p.z).clamp(0.0, 1.0);
            let x = wave(u);
            let slope = (wave((u + 0.02).min(1.0)) - x) / 0.02;
            self.g.positions[k] = [(p.x + x) * self.len, p.y * self.len, p.z * self.len];
            self.g.normals[k] = Vec3::new(n.x + slope * n.z, n.y, n.z).normalize_or_zero().to_array();
        }
        kit.update_geometry(self.geo, &self.g);
    }

    pub fn set_outline(&self, kit: &mut Kit, on: bool) { if on { kit.scene[self.mesh].layers |= 2 } else { kit.scene[self.mesh].layers &= !2 } }
}

