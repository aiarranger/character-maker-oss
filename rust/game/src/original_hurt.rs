// オリジナル 4 体の ＞＜ の板（今の版は lib/character-maker/hurt-face.ts の createHurtFace。characters の hurt-face の写しの、キャラメイクの版）。
// まばたきのモーフで動く頂点から目の場所を求め、目を閉じた顔の面へ前から光線を当てて、マス目の点ごとに面のすぐ上へ板を置く。
// 板の絵（肌の色の楕円と ＞ の線）は TS の canvas が描く（今の版の drawEye と同じ）。ここは形と肌の色を作る。
use crate::original::{MatInfo, Part};
use character_actor::anim::Skeleton;
use character_actor::glb::Glb;
use glam::{DVec3, Mat4, Vec2, Vec3};
use serde_json::Value;
use std::collections::HashMap;

/// 1 体ぶんの ＞＜ の見た目（ORIGINAL_HURT_FACE_STYLES。絵の値は TS が使う）
pub struct Style { pub size: f32, pub tall: f32, pub lift: f32, pub fixed_skin: bool, pub sampled: bool, pub front: f32, pub grid: (usize, usize), pub skin: String }
impl Style {
    pub fn read(v: &Value) -> Option<Style> {
        let n = |k: &str| v.get(k).and_then(Value::as_f64).map(|x| x as f32);
        Some(Style {
            size: n("size")?, tall: n("tall").unwrap_or(0.75), lift: n("lift").unwrap_or(0.0),
            fixed_skin: v["fixedSkin"].as_bool().unwrap_or(false), sampled: v["sampled"].as_bool().unwrap_or(false),
            front: n("front").unwrap_or(0.25),
            grid: v["grid"].as_array().and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize))).unwrap_or((10, 8)),
            skin: v["skin"].as_str()?.to_string(),
        })
    }
}

/// 作った板（頭の骨の下の節点に付ける。頂点は頭の骨の局所）
pub struct Plates { pub positions: Vec<[f32; 3]>, pub uvs: Vec<[f32; 2]>, pub colors: Vec<[f32; 4]>, pub indices: Vec<u32>, pub skin: String }

fn not_lid(name: &str) -> bool { let n = name.to_lowercase(); ["white", "iris", "sclera", "pupil", "lash", "highlight"].iter().any(|k| n.contains(k)) }
fn not_face(name: &str) -> bool { let n = name.to_lowercase(); ["white", "iris", "sclera", "pupil", "highlight"].iter().any(|k| n.contains(k)) }

#[derive(Clone, Copy)]
struct Box3 { min: Vec3, max: Vec3 }
impl Box3 {
    fn empty() -> Self { Box3 { min: Vec3::splat(f32::INFINITY), max: Vec3::splat(f32::NEG_INFINITY) } }
    fn expand(&mut self, p: Vec3) { self.min = self.min.min(p); self.max = self.max.max(p); }
    fn is_empty(&self) -> bool { self.max.x < self.min.x || self.max.y < self.min.y || self.max.z < self.min.z }
    fn contains(&self, p: Vec3) -> bool { !(p.x < self.min.x || p.x > self.max.x || p.y < self.min.y || p.y > self.max.y || p.z < self.min.z || p.z > self.max.z) }
    fn intersects_sphere(&self, c: Vec3, r: f32) -> bool { c.clamp(self.min, self.max).distance_squared(c) <= r * r }
}

/// 部分の頂点を、骨とモーフを通した世界（bind の姿）へ
struct Ctx<'a> { skel: &'a Skeleton, skin_mats: HashMap<usize, Vec<Mat4>> }
impl Ctx<'_> {
    fn world(&self, part: &Part, i: usize, p: Vec3) -> Vec3 {
        match part.skin {
            Some(s) => {
                let m = &self.skin_mats[&s];
                let (j, w) = (part.p.joints[i], part.weights_n[i]);
                let mut o = Vec3::ZERO;
                for k in 0..4 { if w[k] != 0.0 { o += m[j[k] as usize].transform_point3(p) * w[k]; } }
                o
            }
            None => (self.skel.world[part.node] * part.local).transform_point3(p),
        }
    }
}

struct Tri { a: Vec3, b: Vec3, c: Vec3, ua: Vec2, ub: Vec2, uc: Vec2, mat: usize }

/// three の Ray.intersectTriangle（両面）。返すのは距離と重心座標
fn ray_tri(o: DVec3, d: DVec3, t: &Tri) -> Option<(f64, Vec3)> {
    let (ta, tb, tc) = (t.a.as_dvec3(), t.b.as_dvec3(), t.c.as_dvec3());
    let (edge1, edge2) = (tb - ta, tc - ta);
    let normal = edge1.cross(edge2);
    let mut ddn = d.dot(normal);
    let sign = if ddn > 0.0 { 1.0 } else if ddn < 0.0 { ddn = -ddn; -1.0 } else { return None };
    let diff = o - ta;
    let dd_q = sign * d.dot(diff.cross(edge2));
    if dd_q < 0.0 { return None; }
    let dd_e = sign * d.dot(edge1.cross(diff));
    if dd_e < 0.0 || dd_q + dd_e > ddn { return None; }
    let qn = -sign * diff.dot(normal);
    if qn < 0.0 { return None; }
    let (u, v) = (dd_q / ddn, dd_e / ddn);
    Some((qn / ddn, Vec3::new((1.0 - u - v) as f32, u as f32, v as f32)))
}

/// 材質の絵を 256×256 に縮めて読む（今の版の canvas の drawImage(image, 0, 0, 256, 256) の代わり。透けの重みで平均）
struct Reader<'a> { g: &'a Glb, mats: &'a [MatInfo], drawn: HashMap<usize, Vec<u8>> }
const SIZE: usize = 256;
impl Reader<'_> {
    fn image(&mut self, mi: usize) -> Option<&[u8]> {
        if !self.drawn.contains_key(&mi) {
            let im = self.mats[mi].img.get(self.g)?;
            let (iw, ih) = (im.width as usize, im.height as usize);
            let mut out = vec![0u8; SIZE * SIZE * 4];
            for y in 0..SIZE {
                let (y0, y1) = (y * ih / SIZE, ((y + 1) * ih / SIZE).max(y * ih / SIZE + 1).min(ih));
                for x in 0..SIZE {
                    let (x0, x1) = (x * iw / SIZE, ((x + 1) * iw / SIZE).max(x * iw / SIZE + 1).min(iw));
                    let (mut r, mut gg, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
                    for yy in y0..y1 { for xx in x0..x1 {
                        let i = (yy * iw + xx) * 4;
                        let al = im.rgba[i + 3] as u64;
                        r += im.rgba[i] as u64 * al; gg += im.rgba[i + 1] as u64 * al; b += im.rgba[i + 2] as u64 * al; a += al; n += 1;
                    }}
                    let k = (y * SIZE + x) * 4;
                    if a > 0 { out[k] = (r / a) as u8; out[k + 1] = (gg / a) as u8; out[k + 2] = (b / a) as u8; }
                    out[k + 3] = (a / n.max(1)) as u8;
                }
            }
            self.drawn.insert(mi, out);
        }
        self.drawn.get(&mi).map(|v| v.as_slice())
    }
    /// 光線が当たった所の色（sRGB の 0..255 × 材質の色の sRGB）。読めなければ None
    fn read(&mut self, mi: usize, uv: Vec2) -> Option<[f32; 3]> {
        let c = self.mats[mi].color;
        let enc = |v: f32| if v < 0.0031308 { v * 12.92 } else { 1.055 * v.powf(0.41666) - 0.055 };
        let tint = [enc(c[0]), enc(c[1]), enc(c[2])];
        if self.mats[mi].img.get(self.g).is_none() { return Some([tint[0] * 255.0, tint[1] * 255.0, tint[2] * 255.0]); }
        let data = self.image(mi)?;
        let (u, w) = (uv.x.rem_euclid(1.0), uv.y.rem_euclid(1.0));
        let x = (SIZE - 1).min((u * SIZE as f32).floor() as usize);
        let y = (SIZE - 1).min((w * SIZE as f32).floor() as usize);
        let k = (y * SIZE + x) * 4;
        if data[k + 3] <= 200 { return None; }
        Some([data[k] as f32 * tint[0], data[k + 1] as f32 * tint[1], data[k + 2] as f32 * tint[2]])
    }
}

fn tone(c: &[f32; 3]) -> (f32, f32) {
    let (r, g, b) = (c[0] / 255.0, c[1] / 255.0, c[2] / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mn + mx) / 2.0;
    if mn == mx { return (0.0, l); }
    let d = mx - mn;
    (if l <= 0.5 { d / (mx + mn) } else { d / (2.0 - mx - mn) }, l)
}
fn srgb_to_linear(c: f32) -> f32 { if c < 0.04045 { c * 0.0773993808 } else { (c * 0.9478672986 + 0.0521327014).powf(2.4) } }

/// createHurtFace の形の側。parts は目を閉じる前の姿（形の重みは weights）。skel は bind の姿（根は単位）。head は頭の骨。
/// forward は顔の正面（模型の +X）。目や頭が無ければ None
pub fn create(parts: &[Part], weights: &[Vec<f32>], mats: &[MatInfo], g: &Glb, skel: &Skeleton, head: usize, style: &Style, forward: Vec3) -> Option<Plates> {
    let mut skin_mats = HashMap::new();
    for p in parts { if let Some(s) = p.skin && !skin_mats.contains_key(&s) { skin_mats.insert(s, g.skins[s].joints.iter().zip(&g.skins[s].inverse_bind).map(|(j, ib)| skel.world[*j] * *ib).collect::<Vec<_>>()); } }
    let ctx = Ctx { skel, skin_mats };
    let target = |p: &Part, name: &str| p.targets.iter().position(|t| t == name).filter(|i| *i < p.p.morph_pos.len());
    // 目の場所（eyeRegionsFromMorphs: まぶただけで探し、無ければ全部で）
    let regions = |lids_only: bool| -> Option<[(Vec3, f32, f32); 2]> {
        let mut front = [f32::NEG_INFINITY; 2];
        let mut boxes = [Box3::empty(); 2];
        let mut both: Vec<Vec3> = Vec::new();
        for p in parts {
            if p.targets.is_empty() || p.p.morph_pos.is_empty() || (lids_only && not_lid(&p.name)) { continue; }
            let single = target(p, "blinkLeft").is_none() && target(p, "blinkRight").is_none();
            let keys: &[&str] = if single { &["blink"] } else { &["blinkLeft", "blinkRight"] };
            for (ki, key) in keys.iter().enumerate() {
                let Some(index) = target(p, key) else { continue };
                for i in 0..p.p.positions.len() {
                    let d = Vec3::from(p.p.morph_pos[index][i]);
                    if d.length_squared() < 1e-12 { continue; }
                    let w = ctx.world(p, i, Vec3::from(p.p.positions[i]) + d);
                    if single { both.push(w); continue; }
                    boxes[ki].expand(w);
                    front[ki] = front[ki].max(w.dot(forward));
                }
            }
        }
        if !both.is_empty() {
            let side = Vec3::Y.cross(forward).normalize();
            let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
            for w in &both { let s = w.dot(side); lo = lo.min(s); hi = hi.max(s); }
            let mid = (lo + hi) / 2.0;
            for w in &both { let k = if w.dot(side) > mid { 0 } else { 1 }; boxes[k].expand(*w); front[k] = front[k].max(w.dot(forward)); }
        }
        let region = |k: usize| -> Option<(Vec3, f32, f32)> {
            let b = boxes[k];
            if b.is_empty() { return None; }
            let size = b.max - b.min;
            let mut center = (b.min + b.max) * 0.5;
            center += forward * (front[k] - center.dot(forward));
            let side = Vec3::Y.cross(forward).normalize();
            let width = (size.x * side.x).abs() + (size.z * side.z).abs();
            Some((center, width.max(1e-4), size.y))
        };
        Some([region(0)?, region(1)?])
    };
    let eyes = regions(true).or_else(|| regions(false))?;
    let (gx, gy) = style.grid;
    let between = (eyes[0].0 + eyes[1].0) * 0.5;
    struct Plan { n: Vec3, x: Vec3, y: Vec3, w: f32, h: f32, center: Vec3, eye_w: f32 }
    let plans: Vec<Plan> = eyes.iter().map(|(c, width, _)| {
        // 骨で変形する部分の法線は使わない（今の版も null）。正面のまま
        let n = forward.normalize();
        let mut x = *c - between;
        x -= n * x.dot(n);
        if x.length_squared() < 1e-12 { x = Vec3::Y.cross(n); }
        let x = x.normalize();
        let mut y = n.cross(x).normalize();
        if y.dot(Vec3::Y) < 0.0 { y = -y; }
        let w = width * style.size;
        let h = w * style.tall;
        Plan { n, x, y, w, h, center: *c + y * (h * style.lift), eye_w: *width }
    }).collect();
    let mut bx = Box3::empty();
    for p in &plans { for q in [p.center + p.x * p.w, p.center - p.x * p.w, p.center + p.y * p.h, p.center - p.y * p.h, p.center + p.n * p.w, p.center - p.n * p.w] { bx.expand(q); } }
    // 目のまわりの顔の面（目を閉じた姿）。白目・瞳は入れない。骨付きは頭とその下の骨に付いた頂点だけ
    let head_bones = |s: usize| -> Vec<bool> { g.skins[s].joints.iter().map(|j| { let mut o = Some(*j); while let Some(i) = o { if i == head { return true; } o = skel.parent[i]; } false }).collect() };
    let local = { let d = (bx.max - bx.min).length(); Box3 { min: bx.min - Vec3::splat(d), max: bx.max + Vec3::splat(d) } };
    let mut tris: Vec<Tri> = Vec::new();
    for (pi, p) in parts.iter().enumerate() {
        if not_face(&p.name) || p.p.positions.is_empty() { continue; }
        let mut infl = weights[pi].clone();
        infl.resize(p.p.morph_pos.len(), 0.0);
        for key in ["blinkLeft", "blinkRight", "blink"] { if let Some(i) = target(p, key) { infl[i] = 1.0; } }
        if p.skin.is_none() {
            let (lo, hi) = p.p.positions.iter().fold((Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)), |(a, b), v| (a.min(Vec3::from(*v)), b.max(Vec3::from(*v))));
            let c = (lo + hi) * 0.5;
            let r = p.p.positions.iter().map(|v| Vec3::from(*v).distance(c)).fold(0.0, f32::max);
            let m = skel.world[p.node] * p.local;
            let s = character_actor::anim::three_decompose(m).0.abs().max_element();
            if !bx.intersects_sphere(m.transform_point3(c), r * s) { continue; }
        }
        let hb = p.skin.map(head_bones);
        if hb.as_ref().is_some_and(|h| !h.iter().any(|x| *x)) { continue; }
        let n = p.p.positions.len();
        let mut world = vec![Vec3::ZERO; n];
        let mut computed = vec![false; n];
        let mut inside = vec![false; n];
        for i in 0..n {
            if let Some(h) = &hb {
                let (j, w) = (p.p.joints[i], p.weights_n[i]);
                let mut weight = 0.0;
                for k in 0..4 { if h.get(j[k] as usize).copied().unwrap_or(false) { weight += w[k]; } }
                if weight <= 0.01 { continue; }
            }
            let mut q = Vec3::from(p.p.positions[i]);
            for (k, wt) in infl.iter().enumerate() { if *wt != 0.0 { q += Vec3::from(p.p.morph_pos[k][i]) * *wt; } }
            world[i] = ctx.world(p, i, q);
            inside[i] = local.contains(world[i]);
            computed[i] = true;
        }
        for t in p.p.indices.chunks_exact(3) {
            let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
            if !computed[a] || !computed[b] || !computed[c] || (!inside[a] && !inside[b] && !inside[c]) { continue; }
            let centre = (world[a] + world[b] + world[c]) / 3.0;
            if !bx.contains(centre) { continue; }
            let uv = |i: usize| Vec2::from(p.p.uvs.get(i).copied().unwrap_or([0.0, 0.0]));
            tris.push(Tri { a: world[a], b: world[b], c: world[c], ua: uv(a), ub: uv(b), uc: uv(c), mat: p.mat });
        }
    }
    let mut reader = Reader { g, mats, drawn: HashMap::new() };
    let mut skin: Vec<[f32; 3]> = Vec::new();
    let (cols, count) = (gx + 1, (gx + 1) * (gy + 1));
    let mut grids: Vec<(Vec<Vec3>, Vec<f32>, Vec<Option<[f32; 3]>>, Vec<Option<[f32; 3]>>)> = Vec::new();
    for p in &plans {
        let (back, pc, px, py, pn, pw, ph) = (-p.n.as_dvec3(), p.center.as_dvec3(), p.x.as_dvec3(), p.y.as_dvec3(), p.n.as_dvec3(), p.w as f64, p.h as f64);
        let mut seen: Vec<Option<[f32; 3]>> = Vec::with_capacity(count);
        let mut depth: Vec<Option<f32>> = Vec::with_capacity(count);
        for j in 0..=gy {
            for i in 0..=gx {
                let (u, v) = (i as f64 / gx as f64, j as f64 / gy as f64);
                let o = pc + px * ((u - 0.5) * pw) + py * ((v - 0.5) * ph) + pn * pw;
                let mut best: Option<(f64, usize, Vec3)> = None;
                for (ti, t) in tris.iter().enumerate() {
                    let Some((dist, bary)) = ray_tri(o, back, t) else { continue };
                    if dist > pw * 2.0 { continue; }
                    if dist > pw * (1.0 - style.front as f64) && dist < pw * 1.6 && best.is_none_or(|b| dist < b.0) { best = Some((dist, ti, bary)); }
                }
                depth.push(best.map(|b| b.0 as f32));
                let c = best.and_then(|(_, ti, b)| { let t = &tris[ti]; reader.read(t.mat, t.ua * b.x + t.ub * b.y + t.uc * b.z) });
                if let Some(cc) = c { skin.push(cc); }
                seen.push(c);
            }
        }
        let hit_alpha: Vec<f32> = depth.iter().map(|d| if d.is_none() { 0.0 } else { 1.0 }).collect();
        let mut filled = depth.clone();
        let mut pass = 0;
        while pass < gx && filled.iter().any(|d| d.is_none()) {
            let prev = filled.clone();
            for (k, d) in prev.iter().enumerate() {
                if d.is_some() { continue; }
                let (i, j) = ((k % cols) as i32, (k / cols) as i32);
                let (mut sum, mut n) = (0.0, 0);
                for (a, b) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (ii, jj) = (i + a, j + b);
                    if ii < 0 || jj < 0 || ii > gx as i32 || jj > gy as i32 { continue; }
                    if let Some(v) = prev[jj as usize * cols + ii as usize] { sum += v; n += 1; }
                }
                if n > 0 { filled[k] = Some(sum / n as f32); }
            }
            pass += 1;
        }
        let known: Vec<f32> = filled.iter().map(|d| d.unwrap_or(p.w)).collect();
        let idx = |i: i32, j: i32| (j.clamp(0, gy as i32) as usize) * cols + (i.clamp(0, gx as i32) as usize);
        let nearest: Vec<f32> = (0..count).map(|k| { let (i, j) = ((k % cols) as i32, (k / cols) as i32); let mut m = f32::INFINITY; for b in -1..=1 { for a in -1..=1 { m = m.min(known[idx(i + a, j + b)]); } } m }).collect();
        let smooth: Vec<f32> = (0..count).map(|k| { let (i, j) = ((k % cols) as i32, (k / cols) as i32); let mut sum = 0.0; for b in -1..=1 { for a in -1..=1 { sum += nearest[idx(i + a, j + b)]; } } (sum / 9.0).min(known[k]) }).collect();
        let alpha: Vec<f32> = (0..count).map(|k| {
            if hit_alpha[k] == 0.0 { return 0.0; }
            let (i, j) = ((k % cols) as i32, (k / cols) as i32);
            let (mut sum, mut n) = (0.0, 0);
            for b in -1..=1 { for a in -1..=1 { let (ii, jj) = (i + a, j + b); if ii < 0 || jj < 0 || ii > gx as i32 || jj > gy as i32 { continue; } sum += hit_alpha[jj as usize * cols + ii as usize]; n += 1; } }
            sum / n as f32
        }).collect();
        // sampled（毛の色が目のまわりで変わる顔）: 点ごとに、まわりの目を閉じた顔の色の真ん中
        let tint: Vec<Option<[f32; 3]>> = if style.sampled { (0..count).map(|k| {
            let (i, j) = ((k % cols) as i32, (k / cols) as i32);
            let mut near: Vec<[f32; 3]> = Vec::new();
            let mut r = 1;
            while r <= gx as i32 && near.len() < 3 {
                near.clear();
                for b in -r..=r { for a in -r..=r { let (ii, jj) = (i + a, j + b); if ii < 0 || jj < 0 || ii > gx as i32 || jj > gy as i32 { continue; } if let Some(c) = seen[jj as usize * cols + ii as usize] { near.push(c); } } }
                r += 1;
            }
            if near.is_empty() { return None; }
            let median = |ch: usize| { let mut v: Vec<f32> = near.iter().map(|c| c[ch]).collect(); v.sort_by(f32::total_cmp); v[near.len() / 2] };
            Some([srgb_to_linear(median(0) / 255.0), srgb_to_linear(median(1) / 255.0), srgb_to_linear(median(2) / 255.0)])
        }).collect() } else { Vec::new() };
        let mut positions = Vec::with_capacity(count);
        for j in 0..=gy { for i in 0..=gx {
            let (u, v, k) = (i as f32 / gx as f32, j as f32 / gy as f32, j * cols + i);
            positions.push(p.center + p.x * ((u - 0.5) * p.w) + p.y * ((v - 0.5) * p.h) + p.n * (p.w - smooth[k] + p.eye_w * 0.015));
        } }
        grids.push((positions, alpha, seen, tint));
    }
    // 肌の色: 暗い所と鮮やかな所を外した、くすんだ側の真ん中
    let mut bright: Vec<[f32; 3]> = skin.iter().filter(|c| tone(c).1 > 0.45).copied().collect();
    bright.sort_by(|a, b| tone(a).0.total_cmp(&tone(b).0));
    let mut calm: Vec<[f32; 3]> = bright.iter().take(1usize.max((bright.len() as f32 * 0.5).ceil() as usize)).copied().collect();
    calm.sort_by(|a, b| (a[0] + a[1] + a[2]).total_cmp(&(b[0] + b[1] + b[2])));
    let mid: Option<[f32; 3]> = if bright.len() >= 4 { let m = calm[calm.len() / 2]; Some([m[0].min(255.0).round(), m[1].min(255.0).round(), m[2].min(255.0).round()]) } else { None };
    let face_color = mid.map(|m| format!("#{:02x}{:02x}{:02x}", m[0] as u8, m[1] as u8, m[2] as u8));
    if let Some(m) = mid && !style.fixed_skin && !style.sampled {
        for (_, alpha, seen, _) in &mut grids {
            for k in 0..count {
                let (u, v) = ((k % cols) as f32 / gx as f32 - 0.5, (k / cols) as f32 / gy as f32 - 0.5);
                let near = (u / 0.4).powi(2) + (v / 0.38).powi(2) > 1.0;
                if let (true, Some(c)) = (near, seen[k]) && (c[0] - m[0]).abs() + (c[1] - m[1]).abs() + (c[2] - m[2]).abs() > 90.0 { alpha[k] = 0.0; }
            }
        }
    }
    let skin_hex = if style.sampled { "#ffffff".to_string() } else { (if style.fixed_skin { None } else { face_color }).unwrap_or_else(|| style.skin.clone()) };
    // 頭の骨の局所へ（three の node.attach。頭の骨の下の節点は単位の変形）
    let to_head = skel.world[head].inverse();
    let (mut positions, mut uvs, mut colors, mut indices) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for (gi, (pos, alpha, _, tint)) in grids.iter().enumerate() {
        let base = (gi * count) as u32;
        for j in 0..=gy { for i in 0..=gx {
            let k = j * cols + i;
            positions.push(to_head.transform_point3(pos[k]).to_array());
            // 絵は kit.canvas_texture（three の CanvasTexture と同じく上下を返して上げる）。three と同じ uv のまま
            uvs.push([i as f32 / gx as f32, j as f32 / gy as f32]);
            let c = if style.sampled { tint[k] } else { None };
            colors.push([c.map_or(1.0, |c| c[0]), c.map_or(1.0, |c| c[1]), c.map_or(1.0, |c| c[2]), if style.sampled && c.is_none() { 0.0 } else { alpha[k] }]);
        } }
        for j in 0..gy { for i in 0..gx {
            let a = (j * cols + i) as u32 + base;
            let (b, c, d) = (a + 1, a + cols as u32, a + cols as u32 + 1);
            indices.extend_from_slice(&[a, b, d, a, d, c]);
        } }
    }
    Some(Plates { positions, uvs, colors, indices, skin: skin_hex })
}

/// 片目の板の絵（drawEye と同じ描き方。canvas の 2D）: 肌の色の楕円で閉じた目を隠し、鼻の側へとがる ＞ を描く（板の +x が目の外側）。
/// 返り値は three の CanvasTexture（flipY）と同じく上下を返した画素（kit へそのまま上げ、GLB の書き出しにもそのまま書く。GLTFExporter も flipY の絵は返して書く）
pub fn draw_eye(style: &Value, skin: &str) -> Result<character_actor::glb::Image, String> {
    use wasm_bindgen::JsCast;
    let n = |k: &str, d: f64| style.get(k).and_then(Value::as_f64).unwrap_or(d);
    let w = 256u32;
    let h = (w as f64 * n("tall", 0.75)).round() as u32;
    let (wf, hf) = (w as f64, h as f64);
    let feather = n("feather", 0.38).clamp(0.0, 1.0);
    let line = style["line"].as_str().unwrap_or("#000000").to_string();
    let (width, open, inset, glow) = (n("width", 0.09), n("open", 62.0), n("inset", 0.0), n("glow", 0.0));
    let err = |e: wasm_bindgen::JsValue| format!("＞＜ の絵を作れませんでした: {e:?}");
    let canvas = web_sys::OffscreenCanvas::new(w, h).map_err(err)?;
    let g: web_sys::OffscreenCanvasRenderingContext2d = canvas.get_context("2d").map_err(err)?.ok_or("＞＜ の絵を作れませんでした。")?.dyn_into().map_err(|_| "＞＜ の絵を作れませんでした。")?;
    if let Ok(fill) = g.create_radial_gradient(wf / 2.0, hf / 2.0, 0.0, wf / 2.0, hf / 2.0, hf * 0.5) {
        let _ = fill.add_color_stop(0.0, skin);
        for i in 0..=8 {
            let t = i as f64 / 8.0;
            let a = (255.0 * (1.0 - t * t * (3.0 - 2.0 * t))).round() as u32;
            let at = 1.0 - feather + feather * t;
            if at > 0.0 { let _ = fill.add_color_stop(at as f32, &if a == 255 { skin.to_string() } else { format!("{skin}{a:02x}") }); }
        }
        g.save();
        let _ = g.translate(wf / 2.0, hf / 2.0); let _ = g.scale(wf / hf, 1.0); let _ = g.translate(-wf / 2.0, -hf / 2.0);
        g.set_fill_style_canvas_gradient(&fill);
        g.begin_path(); let _ = g.arc(wf / 2.0, hf / 2.0, hf / 2.0, 0.0, std::f64::consts::PI * 2.0); g.fill();
        g.restore();
    }
    let half = (open / 2.0).to_radians();
    let (arm, tip, cy) = (wf * 0.3, wf * (0.32 - inset), hf / 2.0);
    let stroke = || { g.begin_path(); g.move_to(tip + arm * half.cos(), cy - arm * half.sin()); g.line_to(tip, cy); g.line_to(tip + arm * half.cos(), cy + arm * half.sin()); g.stroke(); };
    g.set_line_cap("round"); g.set_line_join("round");
    if glow != 0.0 {
        g.save(); g.set_stroke_style_str(&line); g.set_global_alpha(0.35); g.set_line_width(hf * width * (1.0 + glow)); g.set_filter(&format!("blur({}px)", (hf * 0.03).round())); stroke(); g.restore();
    }
    g.set_stroke_style_str(&line); g.set_line_width(hf * width); stroke();
    let mut rgba = g.get_image_data(0.0, 0.0, wf, hf).map_err(err)?.data().0;
    island_kit::texture::flip_rows(&mut rgba, w, h);
    Ok(character_actor::glb::Image { width: w, height: h, rgba })
}
