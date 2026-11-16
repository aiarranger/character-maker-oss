// 拾い物（あんまん・金平糖）の模型（public/models/candy/<名前>.glb。Tripo で作成）。作り方は public/models/candy/README.md
use crate::assets;
use crate::geom::{Geo, Vertex};
use glam::Vec3;

/// 起動時に先に取る模型
pub const NAMES: [&str; 2] = ["anman", "konpeito"];
pub fn path(name: &str) -> String { format!("models/candy/{name}.glb") }

/// 形（ノードの行列を掛けた 1 つの Geo）と、最初の模様（幅・高さ・RGBA）
pub struct Model {
    pub geo: Geo,
    pub tex: Option<(u32, u32, Vec<u8>)>,
}

/// 模型を読み、いちばん大きい寸法を size m にそろえる。base = true なら底を y = 0（置く物）、false なら中心を原点（浮かぶ物）
pub fn load(name: &str, size: f32, base: bool) -> Result<Model, String> {
    let g = assets::load_glb(&path(name))?;
    let w = g.world_matrices();
    let mut geo = Geo::default();
    let mut tex = None;
    for (i, n) in g.nodes.iter().enumerate() {
        let Some(mi) = n.mesh else { continue };
        let nm = w[i].inverse().transpose();
        for p in &g.meshes[mi].primitives {
            let o = geo.v.len() as u32;
            for k in 0..p.positions.len() {
                geo.v.push(Vertex {
                    pos: w[i].transform_point3(Vec3::from(p.positions[k])).to_array(),
                    normal: p.normals.get(k).map(|nv| nm.transform_vector3(Vec3::from(*nv)).normalize_or_zero().to_array()).unwrap_or([0.0, 1.0, 0.0]),
                    uv: p.uvs.get(k).copied().unwrap_or([0.0, 0.0]),
                    color: [1.0; 4],
                    sway: 0.0,
                });
            }
            geo.i.extend(p.indices.iter().map(|x| x + o));
            if tex.is_none() {
                if let Some(t) = p.material.and_then(|m| g.materials[m].texture) {
                    let im = &g.images[t];
                    tex = Some((im.width, im.height, im.rgba.clone()));
                }
            }
        }
    }
    if geo.v.is_empty() { return Err(format!("{name}: メッシュが無い")); }
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for v in &geo.v { lo = lo.min(Vec3::from(v.pos)); hi = hi.max(Vec3::from(v.pos)); }
    let ext = hi - lo;
    let k = size / ext.max_element().max(1e-4);
    let c = (lo + hi) * 0.5;
    let origin = if base { Vec3::new(c.x, lo.y, c.z) } else { c };
    for v in &mut geo.v { v.pos = ((Vec3::from(v.pos) - origin) * k).to_array(); }
    Ok(Model { geo, tex })
}
