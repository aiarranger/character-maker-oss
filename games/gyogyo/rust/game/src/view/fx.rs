// 粒: 足もとの砂ぼこり・水しぶき・紙吹雪。1 つの Points に詰める（重い端末でも数を減らすだけで済む）。src/view/fx.ts の写し。
use super::materials::glow_texture;
use island_kit::{Color, CustomShader, Kit, MatId, Material, NodeId};

#[derive(Clone, Copy)]
pub struct Emit { pub life: f32, pub size: f32, pub grow: f32, pub g: f32, pub a: f32 }
impl Default for Emit { fn default() -> Self { Emit { life: 0.6, size: 0.6, grow: 1.2, g: 0.0, a: 0.7 } } }

pub struct Fx {
    points: NodeId, mat: MatId, cap: usize, head: usize,
    pos: Vec<f32>, col: Vec<f32>, size: Vec<f32>, vel: Vec<f32>, life: Vec<f32>, max_life: Vec<f32>, grav: Vec<f32>, base_size: Vec<f32>, grow: Vec<f32>, alpha: Vec<f32>,
}

impl Fx {
    pub fn new(kit: &mut Kit, cap: usize) -> Fx {
        let map = glow_texture(kit);
        let mat = kit.material(Material {
            transparent: true, depth_write: false,
            ..Material::custom(CustomShader {
                label: "fx".into(), wgsl: include_str!("../../shaders/fx.wgsl").into(), uniform: bytemuck::bytes_of(&[800.0f32, 0.0, 0.0, 0.0]).to_vec(), textures: vec![map],
                point_attrs: vec![wgpu::VertexFormat::Float32x4, wgpu::VertexFormat::Float32], ..Default::default()
            })
        });
        let points = kit.scene.points(None, cap, mat);
        kit.scene[points].frustum_culled = false;
        kit.scene[points].render_order = 3;
        kit.scene.set_point_count(points, cap);
        Fx {
            points, mat, cap, head: 0, pos: vec![0.0; cap * 3], col: vec![0.0; cap * 4], size: vec![0.0; cap], vel: vec![0.0; cap * 3], life: vec![0.0; cap], max_life: vec![0.0; cap],
            grav: vec![0.0; cap], base_size: vec![0.0; cap], grow: vec![0.0; cap], alpha: vec![0.0; cap],
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn emit(&mut self, x: f32, y: f32, z: f32, vx: f32, vy: f32, vz: f32, c: Color, o: Emit) {
        let i = self.head;
        self.head = (self.head + 1) % self.cap;
        self.pos[i * 3] = x; self.pos[i * 3 + 1] = y; self.pos[i * 3 + 2] = z;
        self.vel[i * 3] = vx; self.vel[i * 3 + 1] = vy; self.vel[i * 3 + 2] = vz;
        self.col[i * 4] = c.r; self.col[i * 4 + 1] = c.g; self.col[i * 4 + 2] = c.b; self.col[i * 4 + 3] = o.a;
        self.alpha[i] = o.a;
        self.life[i] = o.life;
        self.max_life[i] = o.life;
        self.base_size[i] = o.size;
        self.grow[i] = o.grow;
        self.grav[i] = o.g;
        self.size[i] = o.size;
    }

    pub fn set_height(&self, kit: &mut Kit, h: f32) { kit.set_uniform(self.mat, bytemuck::bytes_of(&[h, 0.0, 0.0, 0.0])); }

    /// 今の版は倍精度で計算して Float32Array へ入れる（f32 の dt だと寿命の切れるコマがずれる）
    pub fn update(&mut self, dt: f64) {
        for i in 0..self.cap {
            if self.life[i] <= 0.0 { if self.size[i] != 0.0 { self.size[i] = 0.0; } continue; }
            self.life[i] = (self.life[i] as f64 - dt) as f32;
            self.vel[i * 3 + 1] = (self.vel[i * 3 + 1] as f64 - self.grav[i] as f64 * dt) as f32;
            for a in 0..3 { self.pos[i * 3 + a] = (self.pos[i * 3 + a] as f64 + self.vel[i * 3 + a] as f64 * dt) as f32; }
            let k = (self.life[i] as f64 / self.max_life[i] as f64).max(0.0);
            self.size[i] = (self.base_size[i] as f64 * (1.0 + (1.0 - k) * self.grow[i] as f64)) as f32;
            self.col[i * 4 + 3] = (self.alpha[i] as f64 * k) as f32;
        }
    }

    /// 描く直前に渡す（three は描くときに上げるので、update のあとに出た風の筋もそのコマに写る）
    pub fn upload(&self, kit: &mut Kit) {
        kit.scene.set_point_positions(self.points, &self.pos);
        kit.scene.write_point_attr(self.points, 0, &self.col);
        kit.scene.write_point_attr(self.points, 1, &self.size);
        kit.scene.set_point_count(self.points, self.cap);
    }

    pub fn clear(&mut self) { self.life.fill(0.0); self.size.fill(0.0); }

    /// 確かめ用: 生きている粒の数と合計（比べる道具が今の版の属性から同じ物を作る）
    pub fn sums(&self) -> serde_json::Value {
        let (mut n, mut x, mut y, mut z, mut s, mut a) = (0, 0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        for i in 0..self.cap {
            if self.size[i] > 0.0 { n += 1; x += self.pos[i * 3] as f64; y += self.pos[i * 3 + 1] as f64; z += self.pos[i * 3 + 2] as f64; s += self.size[i] as f64; a += self.col[i * 4 + 3] as f64; }
        }
        serde_json::json!({ "n": n, "x": x, "y": y, "z": z, "s": s, "a": a })
    }
}
