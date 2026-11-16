// Studio の床（今の版は components/character-maker/StageFloor.tsx）: 接地の影（放射の濃淡の板）と色の輪。どちらも unlit・透明・depthWrite なし・renderOrder -1。
// 動き（腰を追う・入りで大きくなる・向きを替えたときの輪の脈）は TS が決めて、毎コマ値を渡す。
use glam::Vec3;
use island_kit::geometry::{plane, ring};
use island_kit::{Color, GeoId, Kit, MatId, Material, NodeId, Rot, TexId};

pub struct Floor { shadow: NodeId, shadow_mat: MatId, ring_group: NodeId, ring: NodeId, ring_geo: GeoId, ring_mat: MatId, radius: f32 }

impl Floor {
    pub fn new(kit: &mut Kit, shadow_tex: TexId) -> Floor {
        let pg = kit.geometry(&plane(1.0, 1.0));
        let mut sm = Material::basic();
        sm.map = Some(shadow_tex); sm.transparent = true; sm.depth_write = false; sm.tone_mapped = false; sm.opacity = 0.0;
        let shadow_mat = kit.material(sm);
        let shadow = kit.scene.mesh(None, pg, shadow_mat);
        { let n = &mut kit.scene[shadow]; n.rotation = Rot::euler(-std::f32::consts::FRAC_PI_2, 0.0, 0.0); n.render_order = -1; n.position = Vec3::new(0.0, 0.002, 0.0); }
        let ring_group = kit.scene.group(None);
        kit.scene[ring_group].visible = false;
        let ring_geo = kit.geometry(&ring(1e-3, 1e-3 * 1.0194, 96));
        let mut rm = Material::basic();
        rm.color = Color::hex(0xB0512A); rm.transparent = true; rm.opacity = 0.22; rm.depth_write = false; rm.tone_mapped = false;
        let ring_mat = kit.material(rm);
        let ring_node = kit.scene.mesh(Some(ring_group), ring_geo, ring_mat);
        { let n = &mut kit.scene[ring_node]; n.rotation = Rot::euler(-std::f32::consts::FRAC_PI_2, 0.0, 0.0); n.position = Vec3::new(0.0, 0.001, 0.0); n.render_order = -1; }
        Floor { shadow, shadow_mat, ring_group, ring: ring_node, ring_geo, ring_mat, radius: 1e-3 }
    }
    /// v: 影の x・z・直径・濃さ、輪の半径・大きさ・見せるか（0/1）。color: 輪の色（'#RRGGBB'）
    pub fn set(&mut self, kit: &mut Kit, v: &[f64], color: Option<Color>) {
        if v.len() < 7 { return; }
        let d = (v[2] as f32).max(1e-4);
        { let n = &mut kit.scene[self.shadow]; n.position = Vec3::new(v[0] as f32, 0.002, v[1] as f32); n.scale = Vec3::new(d, d, 1.0); }
        kit.mat_mut(self.shadow_mat).opacity = v[3] as f32;
        let r = (v[4] as f32).max(1e-3);
        if (r - self.radius).abs() > 1e-7 { kit.update_geometry(self.ring_geo, &ring(r, r * 1.0194, 96)); self.radius = r; }
        kit.scene[self.ring_group].scale = Vec3::splat(v[5] as f32);
        kit.scene[self.ring_group].visible = v[6] != 0.0;
        if let Some(c) = color { kit.mat_mut(self.ring_mat).color = c; }
        let _ = self.ring;
    }
}

/// classic の床（前の three の版の CharacterMakerScene の studio でない枝: 丸い板と細い輪。動かない）
pub fn classic(kit: &mut Kit) {
    use island_kit::geometry::circle;
    let flat = |kit: &mut Kit, geo: island_kit::geometry::Geometry, hex: u32, opacity: f32, y: f32| {
        let g = kit.geometry(&geo);
        let mut m = Material::basic();
        m.color = Color::hex(hex); m.transparent = true; m.opacity = opacity; m.tone_mapped = false;
        let mid = kit.material(m);
        let n = kit.scene.mesh(None, g, mid);
        let node = &mut kit.scene[n];
        node.rotation = Rot::euler(-std::f32::consts::FRAC_PI_2, 0.0, 0.0); node.position = Vec3::new(0.0, y, 0.0);
    };
    flat(kit, circle(0.72, 64), 0xb8cfcd, 0.24, -0.035);
    flat(kit, ring(0.77, 0.775, 64), 0xa5b4a9, 0.4, -0.034);
}
