// 拾い物（あんまん・金平糖）と、投げられた弾の見た目（app.ts の「拾い物・弾の見た目」）。
use crate::geom;
use crate::gfx::{fx_layout, hex, vert_layout, FxInst, Gfx, MeshBuf, Pipe, BLEND_ADD};
use crate::raster;
use crate::view::materials::{draw_fx, Soft, SoftParams, ORDER_OPAQUE};
use glam::{Mat4, Quat, Vec3};
use sim::battle::{Pickup, PickupKind, Projectile};

const MAX_PICKUPS: usize = 256;
const MAX_SHOTS: usize = 64;

pub struct Items {
    soft: Soft,
    /// Tripo の模型の模様（あんまん・金平糖）。無ければ白
    bun_tex: Option<wgpu::BindGroup>,
    star_tex: Option<wgpu::BindGroup>,
    bun: MeshBuf,
    star: MeshBuf,
    shot: MeshBuf,
    quad: MeshBuf,
    glow: wgpu::BindGroup,
    p_sprite: wgpu::RenderPipeline,
    p_basic: wgpu::RenderPipeline,
    empty: wgpu::BindGroup,
    buns: Vec<FxInst>,
    stars: Vec<FxInst>,
    halos: Vec<FxInst>,
    shots: Vec<FxInst>,
    buf_bun: wgpu::Buffer,
    buf_star: wgpu::Buffer,
    buf_halo: wgpu::Buffer,
    buf_shot: wgpu::Buffer,
}

fn fx_buf(gfx: &Gfx, n: usize) -> wgpu::Buffer { gfx.vbuf((n * std::mem::size_of::<FxInst>()) as u64) }

impl Items {
    pub fn new(gfx: &Gfx) -> Self {
        let shader = gfx.shader("items", include_str!("../../shaders/items.wgsl"));
        let empty_l = gfx.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[] });
        let empty = gfx.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &empty_l, entries: &[] });
        let layouts = [&gfx.globals_layout, &empty_l, &gfx.tex_layout];
        let vb = [vert_layout(), fx_layout()];
        let p_sprite = Pipe::scene("sprite", &shader, "vs_sprite", "fs_sprite", &layouts, &vb).transparent(BLEND_ADD).double().build(&gfx.device);
        let p_basic = Pipe::scene("basic", &shader, "vs_basic", "fs_basic", &[&gfx.globals_layout], &vb).build(&gfx.device);
        let (w, h, d) = raster::glow_rgba();
        let view = gfx.texture(w, h, &d, true);
        // あんまん・金平糖は Tripo の模型（読めなければ前の球・八面体）
        let model = |name: &str, size: f32| match crate::view::candy::load(name, size, false) {
            Ok(m) => { let t = m.tex.as_ref().map(|(w, h, d)| gfx.tex_bind(&gfx.texture(*w, *h, d, true))); Some((gfx.mesh(&m.geo), t)) }
            Err(e) => { eprintln!("拾い物の模型 {name}: {e}"); None }
        };
        let (bun, bun_tex) = model("anman", 0.95).unwrap_or_else(|| (gfx.mesh(&geom::sphere(0.42, 16, 10).transform(Mat4::from_scale(Vec3::new(1.0, 0.8, 1.0)))), None));
        let (star, star_tex) = model("konpeito", 0.85).unwrap_or_else(|| (gfx.mesh(&geom::octahedron(0.4)), None));
        Items {
            soft: Soft::new(gfx),
            bun, bun_tex, star, star_tex,
            shot: gfx.mesh(&geom::sphere(0.32, 12, 8)),
            quad: gfx.mesh(&geom::plane(1.0, 1.0, 1, 1)),
            glow: gfx.tex_bind(&view),
            p_sprite, p_basic, empty,
            buns: Vec::with_capacity(MAX_PICKUPS), stars: Vec::with_capacity(MAX_PICKUPS), halos: Vec::with_capacity(MAX_PICKUPS), shots: Vec::with_capacity(MAX_SHOTS),
            buf_bun: fx_buf(gfx, MAX_PICKUPS), buf_star: fx_buf(gfx, MAX_PICKUPS), buf_halo: fx_buf(gfx, MAX_PICKUPS), buf_shot: fx_buf(gfx, MAX_SHOTS),
        }
    }

    pub fn update(&mut self, gfx: &Gfx, pickups: &[Pickup], shots: &[Projectile], t_all: f32) {
        self.buns.clear(); self.stars.clear(); self.halos.clear(); self.shots.clear();
        // 模型（模様つき）は色をそのまま見せ、拾える物とわかるよう輪郭を光らせる
        let bun = if self.bun_tex.is_some() { SoftParams { emissive: [0.06, 0.05, 0.04], rim: 1.4, ..Default::default() } }
            else { SoftParams { color: hex("#fff6ea"), emissive: { let e = hex("#ffd8c0"); [e[0] * 0.35, e[1] * 0.35, e[2] * 0.35] }, rim: 0.0, plain_lambert: true, ..Default::default() } };
        let star = if self.star_tex.is_some() { SoftParams { emissive: { let e = hex("#ffb030"); [e[0] * 0.25, e[1] * 0.25, e[2] * 0.25] }, rim: 1.6, ..Default::default() } }
            else { SoftParams { color: hex("#ffe080"), emissive: { let e = hex("#ffb030"); [e[0] * 0.8, e[1] * 0.8, e[2] * 0.8] }, rim: 0.0, plain_lambert: true, ..Default::default() } };
        let (halo_bun, halo_star) = (hex("#8affb0"), hex("#ffd27a"));
        for k in pickups.iter().take(MAX_PICKUPS) {
            let p = Vec3::new(k.x as f32, 0.6 + (t_all * 3.0 + k.id as f32).sin() * 0.15 + (1.5 - k.t as f32 * 4.0).max(0.0), k.z as f32);
            let m = Mat4::from_rotation_translation(Quat::from_rotation_y(t_all * 2.0), p);
            let (list, params, hc) = if k.kind == PickupKind::Bun { (&mut self.buns, &bun, halo_bun) } else { (&mut self.stars, &star, halo_star) };
            list.push(params.inst(m));
            self.halos.push(FxInst { model: Mat4::from_scale_rotation_translation(Vec3::new(2.2, 2.2, 1.0), Quat::IDENTITY, p).to_cols_array_2d(), a: [hc[0], hc[1], hc[2], 0.8], b: [0.0; 4], c: [0.0; 4], d: [0.0; 4] });
        }
        let col = hex("#ff7ab0");
        for s in shots.iter().take(MAX_SHOTS) {
            self.shots.push(FxInst { model: Mat4::from_translation(Vec3::new(s.x as f32, s.y as f32, s.z as f32)).to_cols_array_2d(), a: [col[0], col[1], col[2], 1.0], b: [0.0; 4], c: [0.0; 4], d: [0.0; 4] });
        }
        for (buf, v) in [(&self.buf_bun, &self.buns), (&self.buf_star, &self.stars), (&self.buf_halo, &self.halos), (&self.buf_shot, &self.shots)] {
            if !v.is_empty() { gfx.queue.write_buffer(buf, 0, bytemuck::cast_slice(v)); }
        }
    }

    pub fn shadow(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.buns.is_empty() && self.stars.is_empty() { return; }
        pass.set_pipeline(&self.soft.shadow);
        draw_fx(pass, &self.bun, &self.buf_bun, 0..self.buns.len() as u32);
        draw_fx(pass, &self.star, &self.buf_star, 0..self.stars.len() as u32);
    }

    pub fn draw(&self, gfx: &Gfx, pass: &mut wgpu::RenderPass<'_>, order: i32) {
        if order == ORDER_OPAQUE {
            self.soft.bind(gfx, pass);
            pass.set_pipeline(&self.soft.front);
            if let Some(t) = &self.bun_tex { pass.set_bind_group(2, t, &[]); }
            draw_fx(pass, &self.bun, &self.buf_bun, 0..self.buns.len() as u32);
            pass.set_bind_group(2, self.star_tex.as_ref().unwrap_or(&self.soft.white), &[]);
            draw_fx(pass, &self.star, &self.buf_star, 0..self.stars.len() as u32);
            pass.set_pipeline(&self.p_basic);
            draw_fx(pass, &self.shot, &self.buf_shot, 0..self.shots.len() as u32);
        } else if order == 0 && !self.halos.is_empty() {
            pass.set_pipeline(&self.p_sprite);
            pass.set_bind_group(1, &self.empty, &[]);
            pass.set_bind_group(2, &self.glow, &[]);
            draw_fx(pass, &self.quad, &self.buf_halo, 0..self.halos.len() as u32);
        }
    }
}
