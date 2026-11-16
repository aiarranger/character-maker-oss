// 魚の大群の見た目: 敵はすべて魚（view/fish.rs の School）。ここは群れ全体の入口で、足元の丸影と、主役が乗るわたあめ雲も描く。
// 1体ごとの値（fx）: x = 当たりの白い光、y = 攻撃の構えの赤い光、z = 消える前の白化、w = 金の縁（隊長はいつも、雑兵は連鎖に巻き込まれている間）。
use crate::geom::{self, Geo};
use crate::gfx::{hex, inst_layout, vert_layout, Gfx, Inst, MeshBuf, Pipe, BLEND_NORMAL};
use crate::raster;
use crate::view::fish::{self, School};
use crate::view::materials::ORDER_OPAQUE;
use glam::{Mat4, Quat, Vec3};
use sim::battle::{Enemy, EnemyKind, EnemyState, Role, KINDS};
use std::collections::HashSet;
use std::f32::consts::PI;
use wgpu::util::DeviceExt;

/// 斬ったときの火花の色（敵の種類ごと）
const SPARKLE: [(EnemyKind, &str); 9] = [
    (EnemyKind::Strawberry, "#ff6d8a"),
    (EnemyKind::Macaron, "#c69bff"),
    (EnemyKind::Mikan, "#ffa040"),
    (EnemyKind::Creampuff, "#ffd27a"),
    (EnemyKind::Cottoncandy, "#ff9ee8"),
    (EnemyKind::Donut, "#ff8fb0"),
    (EnemyKind::Pudding, "#ffd86a"),
    (EnemyKind::Shortcake, "#fff0d0"),
    (EnemyKind::Cakeking, "#ffe08a"),
];

pub fn sparkle(kind: EnemyKind) -> &'static str {
    SPARKLE.iter().find(|k| k.0 == kind).map(|k| k.1).unwrap_or("#ffffff")
}

const SHADOW_MAX: usize = 900;
/// わたあめ雲の色（切れ端の粒と同じ桃色 #ffd6ea）
const CLOUD: [u8; 4] = [0xff, 0xd6, 0xea, 0xff];

fn kind_def(k: EnemyKind) -> &'static sim::battle::KindDef { &KINDS[k as usize] }

/// わたあめ雲の形: 丸を寄せたかたまり（高さ 1・足元が原点。大きさと向きは app が行列で決める）
fn cloud_geo() -> Geo {
    let mut g = geom::sphere(0.42, 16, 10).translate(0.0, 0.5, 0.0);
    for i in 0..6 {
        let a = i as f32 / 6.0 * 2.0 * PI;
        let (r, y) = if i % 2 == 0 { (0.28, 0.42) } else { (0.24, 0.6) };
        g.append(&geom::sphere(r, 12, 8).translate(a.cos() * 0.24, y, a.sin() * 0.24));
    }
    g.append(&geom::sphere(0.3, 12, 8).translate(0.0, 0.7, 0.0));
    g.append(&geom::sphere(0.34, 12, 8).translate(0.0, 0.34, 0.0));
    g
}

pub struct Horde {
    school: School,
    pipe: wgpu::RenderPipeline,
    pipe_shadow: wgpu::RenderPipeline,
    pipe_blob: wgpu::RenderPipeline,
    kind_bg: wgpu::BindGroup,
    // わたあめ雲（主役が乗っている間だけ 1 つ）
    cloud_mesh: MeshBuf,
    cloud_tex: wgpu::BindGroup,
    cloud_buf: wgpu::Buffer,
    cloud_on: bool,
    // 足元の丸影
    blob_mesh: MeshBuf,
    blob_tex: wgpu::BindGroup,
    blob_buf: wgpu::Buffer,
    blobs: Vec<Inst>,
}

impl Horde {
    pub fn new(gfx: &Gfx) -> Result<Self, String> {
        let shader = gfx.shader("horde", include_str!("../../shaders/horde.wgsl"));
        let kind_layout = gfx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("kind"), entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None,
        }] });
        let layouts = [&gfx.globals_layout, &gfx.shadow_layout, &gfx.tex_layout, &kind_layout];
        let vb = [vert_layout(), inst_layout()];
        let pipe = Pipe::scene("horde", &shader, "vs", "fs", &layouts, &vb).build(&gfx.device);
        let pipe_shadow = Pipe::shadow("horde shadow", &shader, "vs_shadow", &[&gfx.globals_layout], &vb).build(&gfx.device);
        let mut blob = Pipe::scene("horde blob", &shader, "vs_blob", "fs_blob", &layouts, &vb).transparent(BLEND_NORMAL);
        blob.bias = -2;
        let pipe_blob = blob.build(&gfx.device);

        // 雲の材質の値（白化の色。隊長の金の縁は使わない）
        let spk = hex(sparkle(EnemyKind::Cottoncandy));
        let u = [spk[0], spk[1], spk[2], 1.0, 0.0f32, 0.0, 0.0, 0.0];
        let ub = gfx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: None, contents: bytemuck::cast_slice(&u), usage: wgpu::BufferUsages::UNIFORM });
        let kind_bg = gfx.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &kind_layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: ub.as_entire_binding() }] });
        let cloud_view = gfx.texture(1, 1, &CLOUD, true);
        let (bw, bh, bd) = raster::blob_rgba();
        let blob_view = gfx.texture(bw, bh, &bd, true);
        Ok(Horde {
            school: School::new(gfx)?,
            pipe, pipe_shadow, pipe_blob, kind_bg,
            cloud_mesh: gfx.mesh(&cloud_geo()),
            cloud_tex: gfx.tex_bind(&cloud_view),
            cloud_buf: gfx.vbuf(std::mem::size_of::<Inst>() as u64),
            cloud_on: false,
            blob_mesh: gfx.mesh(&geom::plane(1.0, 1.0, 1, 1).rotate_x(-PI / 2.0)),
            blob_tex: gfx.tex_bind(&blob_view),
            blob_buf: gfx.vbuf(SHADOW_MAX as u64 * std::mem::size_of::<Inst>() as u64),
            blobs: Vec::with_capacity(SHADOW_MAX),
        })
    }

    /// cam: カメラの位置。すぐ前（5m 以内）の敵は描かない（主役を隠さない）。loaded: 足元を金に光らせる敵（装填）。
    /// mount: 主役が乗っているわたあめ雲の姿勢（敵ではない）
    pub fn update(&mut self, gfx: &Gfx, enemies: &[Enemy], time: f32, cam: Vec3, loaded: &HashSet<i64>, mount: Option<Mat4>) {
        self.blobs.clear();
        self.school.clear();
        // 影の色: ふだんは紫の影、強攻撃の範囲にいる敵（装填）は金に光る
        let shadow = hex("#4a3550");
        let l = hex("#ffc030");
        let lit = [l[0] * 1.6, l[1] * 1.6, l[2] * 1.6];
        for e in enemies {
            if !e.alive { continue; }
            let d = kind_def(e.kind);
            if d.role != Role::Captain {
                let (dx, dy, dz) = (e.x as f32 - cam.x, e.y as f32 + 0.5 - cam.y, e.z as f32 - cam.z);
                if dx * dx + dy * dy + dz * dz < 25.0 { continue; }
            }
            let hit_t = e.hit_t as f32;
            let flash = if hit_t < 0.05 { 1.0 } else { (0.5 - hit_t * 6.0).max(0.0) };
            let warn = if e.state == EnemyState::Windup { (e.st as f32 / 0.5).min(1.0) } else { 0.0 };
            let end = if d.role == Role::Captain { 1.6 } else { 0.95 };
            let white = if e.state == EnemyState::Dead { ((e.dead_t as f32 / end - 0.55) * 2.2).max(0.0) } else { 0.0 };
            // 金: 巻き込まれた直後は強く、そのあと 4 秒は薄く（乱舞で起爆できる印）
            let chain_t = e.chain_t as f32;
            let gold = if d.role == Role::Captain || e.fling != 0 { 1.0 } else { (1.0 - chain_t / 0.7).max(if chain_t < 4.0 { 0.3 } else { 0.0 }) };
            self.school.push(e, time, [flash, warn, white.min(1.0), gold]);
            if self.blobs.len() < SHADOW_MAX {
                let on = loaded.contains(&(e.id as i64));
                // 魚は体の長さに合わせた丸影（宙を泳ぐので少し淡く小さく）
                let r = fish::length_of(e.kind) * 0.32;
                let s = r * (if on { 3.0 } else { 2.4 }) / (1.0 + e.y as f32 * 0.3);
                let m = Mat4::from_scale_rotation_translation(Vec3::new(s, 1.0, s), Quat::IDENTITY, Vec3::new(e.x as f32, 0.02, e.z as f32));
                let c = if on { lit } else { shadow };
                self.blobs.push(Inst::new(m, [c[0], c[1], c[2], 1.0]));
            }
        }
        self.cloud_on = mount.is_some();
        if let Some(m) = mount {
            gfx.queue.write_buffer(&self.cloud_buf, 0, bytemuck::bytes_of(&Inst::new(m, [0.0, 0.0, 0.18, 0.0])));
            if self.blobs.len() < SHADOW_MAX {
                let at = m.w_axis;
                let s = 3.4 / (1.0 + at.y * 0.12);
                self.blobs.push(Inst::new(Mat4::from_scale_rotation_translation(Vec3::new(s, 1.0, s), Quat::IDENTITY, Vec3::new(at.x, 0.02, at.z)), [shadow[0], shadow[1], shadow[2], 1.0]));
            }
        }
        if !self.blobs.is_empty() { gfx.queue.write_buffer(&self.blob_buf, 0, bytemuck::cast_slice(&self.blobs)); }
        self.school.upload(gfx);
    }

    fn draw_cloud(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_vertex_buffer(0, self.cloud_mesh.vb.slice(..));
        pass.set_vertex_buffer(1, self.cloud_buf.slice(..));
        pass.set_index_buffer(self.cloud_mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..self.cloud_mesh.n, 0, 0..1);
    }

    pub fn shadow(&self, pass: &mut wgpu::RenderPass<'_>) {
        self.school.shadow(pass);
        if self.cloud_on {
            pass.set_pipeline(&self.pipe_shadow);
            self.draw_cloud(pass);
        }
    }

    pub fn draw(&self, gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, order: i32) {
        self.school.draw(gfx, pass, order);
        if order == ORDER_OPAQUE {
            if self.cloud_on {
                pass.set_pipeline(&self.pipe);
                pass.set_bind_group(1, &gfx.shadow_bg, &[]);
                pass.set_bind_group(2, &self.cloud_tex, &[]);
                pass.set_bind_group(3, &self.kind_bg, &[]);
                self.draw_cloud(pass);
            }
        } else if order == 1 && !self.blobs.is_empty() {
            pass.set_pipeline(&self.pipe_blob);
            pass.set_bind_group(1, &gfx.shadow_bg, &[]);
            pass.set_bind_group(2, &self.blob_tex, &[]);
            pass.set_bind_group(3, &self.kind_bg, &[]);
            pass.set_vertex_buffer(0, self.blob_mesh.vb.slice(..));
            pass.set_vertex_buffer(1, self.blob_buf.slice(..));
            pass.set_index_buffer(self.blob_mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.blob_mesh.n, 0, 0..self.blobs.len() as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 防ぐ故障: 雲が主役の足もとより上へはみ出す・地面に埋まる（app は「高さ 1・足元が原点」の形として置く）
    #[test]
    fn 雲の形は高さ1で足元が原点() {
        let g = cloud_geo();
        let (lo, hi) = g.v.iter().fold((f32::MAX, f32::MIN), |(a, b), v| (a.min(v.pos[1]), b.max(v.pos[1])));
        assert!(lo > -0.01 && lo < 0.1 && hi < 1.01 && hi > 0.9, "{lo} {hi}");
        let r = g.v.iter().fold(0.0f32, |r, v| r.max(v.pos[0].hypot(v.pos[2])));
        assert!(r > 0.45 && r < 0.6, "{r}");
    }
}
