// 被弾した主役の ＞＜ の顔（character-motion の hurt-face の写し）。目をまばたきで閉じ、その上に、閉じた目を隠す肌色と ＞＜ の線を描いた板を、頭の骨に付けて重ねる。
// 板の場所は、まばたきのモーフで動く頂点（閉じた目のまぶた）から求める。見た目は配布の登録表（registry の hurtFace）で 1 人ずつ決まっている。
use crate::anim::Skeleton;
use crate::Vertex;
use crate::glb::{Glb, Primitive};
use crate::raster::Canvas;
use glam::{DVec3, Mat3, Mat4, Vec2, Vec3};
use serde_json::Value;
use std::collections::HashMap;

/// 1 人ぶんの ＞＜ の見た目（registry.hurtFace.style）
#[derive(Clone, Debug)]
pub struct HurtFaceStyle {
    pub line: String,
    pub skin: String,
    pub width: f32,
    pub size: f32,
    pub open: f32,
    pub lift: f32,
    pub inset: f32,
    pub mouth: Vec<(String, f32)>,
    pub tall: Option<f32>,
    pub fixed_skin: bool,
    pub hug: Option<f32>,
    pub grid: Option<(usize, usize)>,
    pub hug_eye: Option<f32>,
    pub front: Option<f32>,
    pub feather: Option<f32>,
    pub hide: Option<String>,
    pub attachment_node: Option<String>,
}

impl HurtFaceStyle {
    pub fn from_json(v: &Value) -> Option<HurtFaceStyle> {
        let n = |k: &str| v.get(k).and_then(Value::as_f64).map(|x| x as f32);
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(|x| x.to_string());
        Some(HurtFaceStyle {
            line: s("line")?, skin: s("skin")?, width: n("width")?, size: n("size")?, open: n("open")?, lift: n("lift")?, inset: n("inset")?,
            mouth: v.get("mouth").and_then(Value::as_object).map(|o| o.iter().map(|(k, x)| (k.clone(), x.as_f64().unwrap_or(0.0) as f32)).collect()).unwrap_or_default(),
            tall: n("tall"),
            fixed_skin: v.get("fixedSkin").and_then(Value::as_bool).unwrap_or(false),
            hug: n("hug"),
            grid: v.get("grid").and_then(Value::as_array).and_then(|a| Some((a.first()?.as_u64()? as usize, a.get(1)?.as_u64()? as usize))),
            hug_eye: n("hugEye"), front: n("front"), feather: n("feather"), hide: s("hide"), attachment_node: s("attachmentNode"),
        })
    }
}

/// モデルの 1 プリミティブ（three の Mesh 1 つ）を読むための参照
pub struct PrimRef<'a> {
    pub name: String,
    pub node: usize,
    pub p: &'a Primitive,
    /// 足して 1 に直した重み（three の normalizeSkinWeights）
    pub weights: Vec<[f32; 4]>,
    pub skin: Option<usize>,
    pub material: usize,
    pub targets: &'a [String],
}

/// 光なしの材質（character-runtime の convertCharacterMaterial の結果）
#[derive(Clone, Debug)]
pub struct BasicMaterial {
    pub map: Option<usize>,
    pub color: [f32; 3],
    pub opacity: f32,
    pub double_sided: bool,
    pub transparent: bool,
    pub depth_write: bool,
    pub alpha_test: f32,
}

pub struct Ctx<'a> {
    pub g: &'a Glb,
    pub skel: &'a Skeleton,
    /// skin ごとの 骨の世界 × 逆バインド（アニメを当てる前の姿）
    pub skin_mats: Vec<Vec<Mat4>>,
    pub prims: &'a [PrimRef<'a>],
    pub materials: &'a [BasicMaterial],
}

impl PrimRef<'_> {
    fn target(&self, name: &str) -> Option<usize> { self.targets.iter().position(|t| t == name).filter(|&i| i < self.p.morph_pos.len()) }
}

/// 頂点の世界の位置（three の getVertexPosition + matrixWorld。骨で変形するメッシュは骨を通す）
fn to_world(c: &Ctx, pr: &PrimRef, i: usize, p: Vec3) -> Vec3 {
    match pr.skin {
        Some(s) => {
            let mats = &c.skin_mats[s];
            let (j, w) = (pr.p.joints[i], pr.weights[i]);
            let mut o = Vec3::ZERO;
            for k in 0..4 { if w[k] != 0.0 { o += mats[j[k] as usize].transform_point3(p) * w[k]; } }
            o
        }
        None => c.skel.world[pr.node].transform_point3(p),
    }
}

/// 白目・瞳・まつ毛・ハイライトのメッシュ（three の /white|iris|sclera|pupil|lash|highlight/i）
fn not_lid(name: &str) -> bool {
    let n = name.to_lowercase();
    ["white", "iris", "sclera", "pupil", "lash", "highlight"].iter().any(|k| n.contains(k))
}
fn not_face(name: &str) -> bool {
    let n = name.to_lowercase();
    ["white", "iris", "sclera", "pupil", "highlight"].iter().any(|k| n.contains(k))
}
/// style.hide（「a|b」の形。大文字小文字を区別しない）
fn hide_match(pat: &str, name: &str) -> bool {
    let n = name.to_lowercase();
    pat.split('|').any(|k| !k.is_empty() && n.contains(&k.to_lowercase()))
}

#[derive(Clone, Copy)]
struct Box3 { min: Vec3, max: Vec3 }
impl Box3 {
    fn empty() -> Self { Box3 { min: Vec3::splat(f32::INFINITY), max: Vec3::splat(f32::NEG_INFINITY) } }
    fn expand(&mut self, p: Vec3) { self.min = self.min.min(p); self.max = self.max.max(p); }
    fn is_empty(&self) -> bool { self.max.x < self.min.x || self.max.y < self.min.y || self.max.z < self.min.z }
    fn contains(&self, p: Vec3) -> bool { !(p.x < self.min.x || p.x > self.max.x || p.y < self.min.y || p.y > self.max.y || p.z < self.min.z || p.z > self.max.z) }
}

#[derive(Clone, Copy)]
struct EyeRegion { center: Vec3, width: f32, normal: Option<Vec3> }

fn moves_back(c: &Ctx, pr: &PrimRef, index: usize, forward: Vec3) -> bool {
    let turn = Mat3::from_mat4(c.skel.world[pr.node]);
    let (mut along, mut up) = (0.0, 0.0);
    for d in &pr.p.morph_pos[index] {
        let d = Vec3::from(*d);
        if d.length_squared() < 1e-12 { continue; }
        let d = turn * d;
        along += d.dot(forward).abs();
        up += d.y.abs();
    }
    along > up
}

fn eye_regions_of(c: &Ctx, forward: Vec3, lids_only: bool) -> Option<(EyeRegion, EyeRegion)> {
    let mut front = [f32::NEG_INFINITY; 2];
    let mut boxes = [Box3::empty(); 2];
    let mut normals = [Vec3::ZERO; 2];
    let mut both: Vec<(Vec3, Option<Vec3>)> = Vec::new();
    for pr in c.prims {
        if pr.p.morph_pos.is_empty() || (lids_only && not_lid(&pr.name)) { continue; }
        let normal_m = Mat3::from_mat4(c.skel.world[pr.node]).inverse().transpose();
        let single = pr.target("blinkLeft").is_none() && pr.target("blinkRight").is_none();
        if single && let Some(i) = pr.target("blink") && moves_back(c, pr, i, forward) { continue; }
        let keys: &[&str] = if single { &["blink"] } else { &["blinkLeft", "blinkRight"] };
        for (ki, key) in keys.iter().enumerate() {
            let Some(index) = pr.target(key) else { continue };
            let target = &pr.p.morph_pos[index];
            for i in 0..pr.p.positions.len() {
                let d = Vec3::from(target[i]);
                if d.length_squared() < 1e-12 { continue; }
                let p = to_world(c, pr, i, Vec3::from(pr.p.positions[i]) + d);
                // 法線は骨の変形を通していないので、骨で変形するメッシュでは使わない
                let vn = if pr.skin.is_none() { Some((normal_m * Vec3::from(pr.p.normals[i])).normalize()) } else { None };
                if single { both.push((p, vn)); continue; }
                boxes[ki].expand(p);
                front[ki] = front[ki].max(p.dot(forward));
                if let Some(n) = vn { normals[ki] += n; }
            }
        }
    }
    if !both.is_empty() {
        // キャラの左 = 上 × 正面。左右の端の真ん中より左側を左目にする
        let side = Vec3::Y.cross(forward).normalize();
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for (p, _) in &both { let s = p.dot(side); lo = lo.min(s); hi = hi.max(s); }
        let mid = (lo + hi) / 2.0;
        for (p, n) in &both {
            let k = if p.dot(side) > mid { 0 } else { 1 };
            boxes[k].expand(*p);
            front[k] = front[k].max(p.dot(forward));
            if let Some(n) = n { normals[k] += *n; }
        }
    }
    let region = |k: usize| -> Option<EyeRegion> {
        let b = boxes[k];
        if b.is_empty() { return None; }
        let size = b.max - b.min;
        let mut center = (b.min + b.max) * 0.5;
        center += forward * (front[k] - center.dot(forward));
        Some(EyeRegion { center, width: size.x.max(size.z), normal: if normals[k].length_squared() > 1e-12 { Some(normals[k].normalize()) } else { None } })
    };
    Some((region(0)?, region(1)?))
}

/// 三角形（目を閉じた姿の世界の座標）
struct Tri { a: Vec3, b: Vec3, c: Vec3, ua: Vec2, ub: Vec2, uc: Vec2, material: usize }

fn face_proxy(c: &Ctx, b: &Box3, skip: Option<&str>) -> Vec<Tri> {
    let mut out = Vec::new();
    for pr in c.prims {
        if not_face(&pr.name) || skip.is_some_and(|s| hide_match(s, &pr.name)) { continue; }
        let mut infl: Vec<f32> = c.skel.morphs[pr.node].clone();
        infl.resize(pr.p.morph_pos.len(), 0.0);
        for key in ["blinkLeft", "blinkRight", "blink"] { if let Some(i) = pr.target(key) { infl[i] = 1.0; } }
        let world: Vec<Vec3> = (0..pr.p.positions.len()).map(|i| {
            let mut p = Vec3::from(pr.p.positions[i]);
            for (k, w) in infl.iter().enumerate() { if *w != 0.0 { p += Vec3::from(pr.p.morph_pos[k][i]) * *w; } }
            to_world(c, pr, i, p)
        }).collect();
        for t in pr.p.indices.chunks_exact(3) {
            let (ia, ib, ic) = (t[0] as usize, t[1] as usize, t[2] as usize);
            let center = (world[ia] + world[ib] + world[ic]) / 3.0;
            if !b.contains(center) { continue; }
            let uv = |i: usize| Vec2::from(pr.p.uvs[i]);
            out.push(Tri { a: world[ia], b: world[ib], c: world[ic], ua: uv(ia), ub: uv(ib), uc: uv(ic), material: pr.material });
        }
    }
    out
}

/// three の Ray.intersectTriangle（両面）。返すのは距離と重心座標
fn ray_tri(o: DVec3, d: DVec3, t: &Tri) -> Option<(f64, Vec3)> {
    // three の Vector3 と同じ倍精度で（ごく小さい三角が f32 の丸めで当たったり外れたりすると、板の縁の深さと透けが変わる）
    let (ta, tb, tc) = (t.a.as_dvec3(), t.b.as_dvec3(), t.c.as_dvec3());
    let edge1 = tb - ta;
    let edge2 = tc - ta;
    let normal = edge1.cross(edge2);
    let mut ddn = d.dot(normal);
    let sign;
    if ddn > 0.0 { sign = 1.0; } else if ddn < 0.0 { sign = -1.0; ddn = -ddn; } else { return None; }
    let diff = o - ta;
    let dd_q = sign * d.dot(diff.cross(edge2));
    if dd_q < 0.0 { return None; }
    let dd_e = sign * d.dot(edge1.cross(diff));
    if dd_e < 0.0 || dd_q + dd_e > ddn { return None; }
    let qn = -sign * diff.dot(normal);
    if qn < 0.0 { return None; }
    let dist = qn / ddn;
    let (u, v) = (dd_q / ddn, dd_e / ddn);
    Some((dist, Vec3::new((1.0 - u - v) as f32, u as f32, v as f32)))
}

/// 材質のテクスチャを 256×256 に縮めて読む（three 版の canvas の drawImage の代わり）
struct TexReader<'a> { g: &'a Glb, drawn: HashMap<usize, Vec<u8>> }
const SIZE: usize = 256;
impl TexReader<'_> {
    fn image(&mut self, idx: usize) -> &[u8] {
        let g = self.g;
        self.drawn.entry(idx).or_insert_with(|| {
            let im = &g.images[idx];
            let (iw, ih) = (im.width as usize, im.height as usize);
            let mut out = vec![0u8; SIZE * SIZE * 4];
            for y in 0..SIZE {
                let (y0, y1) = (y * ih / SIZE, ((y + 1) * ih / SIZE).max(y * ih / SIZE + 1).min(ih));
                for x in 0..SIZE {
                    let (x0, x1) = (x * iw / SIZE, ((x + 1) * iw / SIZE).max(x * iw / SIZE + 1).min(iw));
                    let (mut r, mut gg, mut b, mut a) = (0u64, 0u64, 0u64, 0u64);
                    let mut n = 0u64;
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
            out
        })
    }
    /// 顔の面の、光線が当たった所のテクスチャの色（読めなければ None）。glTF のテクスチャは flipY なし
    fn read(&mut self, m: &BasicMaterial, uv: Vec2) -> Option<[f32; 3]> {
        let idx = m.map?;
        let data = self.image(idx);
        let (u, w) = (uv.x.rem_euclid(1.0), uv.y.rem_euclid(1.0));
        let x = (SIZE - 1).min((u * SIZE as f32).floor() as usize);
        let y = (SIZE - 1).min((w * SIZE as f32).floor() as usize);
        let k = (y * SIZE + x) * 4;
        if data[k + 3] <= 200 { return None; }
        // 光なしの表示はテクスチャの色 × 材質の色（白でなければ掛ける）
        let tint = |c: f32| if c < 0.0031308 { c * 12.92 } else { 1.055 * c.powf(0.41666) - 0.055 };
        Some([data[k] as f32 * tint(m.color[0]), data[k + 1] as f32 * tint(m.color[1]), data[k + 2] as f32 * tint(m.color[2])])
    }
}

/// 閉じたまぶたの肌の色をテクスチャから読む。まつ毛の線や透明な所は外し、明るい側の中央の色を使う。読めなければ None
fn lid_color(c: &Ctx, read: &mut TexReader) -> Option<String> {
    let mut samples: Vec<[u8; 3]> = Vec::new();
    for pr in c.prims {
        let m = &c.materials[pr.material];
        if pr.p.morph_pos.is_empty() || m.map.is_none() || not_lid(&pr.name) { continue; }
        let idx = m.map.unwrap();
        let data = read.image(idx).to_vec();
        for key in ["blinkLeft", "blinkRight", "blink"] {
            let Some(ti) = pr.target(key) else { continue };
            for i in 0..pr.p.positions.len() {
                if Vec3::from(pr.p.morph_pos[ti][i]).length_squared() < 1e-12 { continue; }
                let uv = pr.p.uvs[i];
                let (u, v) = (uv[0].rem_euclid(1.0), uv[1].rem_euclid(1.0));
                let (x, y) = ((SIZE - 1).min((u * SIZE as f32).floor() as usize), (SIZE - 1).min((v * SIZE as f32).floor() as usize));
                let k = (y * SIZE + x) * 4;
                if data[k + 3] > 200 { samples.push([data[k], data[k + 1], data[k + 2]]); }
            }
        }
    }
    if samples.len() < 8 { return None; }
    samples.sort_by_key(|s| s[0] as u32 + s[1] as u32 + s[2] as u32);
    let s = samples[(samples.len() as f32 * 0.6).floor() as usize];
    Some(format!("#{:02x}{:02x}{:02x}", s[0], s[1], s[2]))
}

fn hex_rgb(s: &str) -> (f32, f32, f32) {
    let s = s.trim_start_matches('#');
    let v = u32::from_str_radix(s, 16).unwrap_or(0xffffff);
    ((v >> 16 & 255) as f32, (v >> 8 & 255) as f32, (v & 255) as f32)
}

/// 片目の板の絵: まぶたの上を肌の色の楕円でぼかして隠し、鼻の側へとがる ＞ を描く（板の +x が目の外側）。色は sRGB のまま（canvas と同じ）
pub fn draw_eye(style: &HurtFaceStyle, skin: &str) -> (u32, u32, Vec<u8>) {
    let w = 256usize;
    let h = (w as f32 * style.tall.unwrap_or(0.75)).round() as usize;
    let mut cv = Canvas::new(w, h);
    let (wf, hf) = (w as f32, h as f32);
    let (sr, sg, sb) = hex_rgb(skin);
    // 楕円（円を横へ W/H に伸ばす）を、半径に沿ったグラデーションで塗る
    let stops: Vec<(f32, f32)> = match style.feather {
        None => vec![(0.0, 1.0), (0.62, 1.0), (1.0, 0.0)],
        Some(fe) => {
            let f = fe.clamp(0.0, 1.0);
            let mut v = vec![(0.0, 1.0)];
            for i in 0..=8 {
                let t = i as f32 / 8.0;
                let a = (255.0 * (1.0 - t * t * (3.0 - 2.0 * t))).round() / 255.0;
                let at = 1.0 - f + f * t;
                if at > 0.0 { v.push((at, a)); }
            }
            v
        }
    };
    let (r0, r1) = if style.feather.is_none() { (hf * 0.18, hf * 0.5) } else { (0.0, hf * 0.5) };
    let alpha_at = |t: f32| -> f32 {
        let mut prev = stops[0];
        for s in &stops {
            if t <= s.0 {
                let k = if s.0 > prev.0 { (t - prev.0) / (s.0 - prev.0) } else { 0.0 };
                return prev.1 + (s.1 - prev.1) * k;
            }
            prev = *s;
        }
        prev.1
    };
    for y in 0..h {
        for x in 0..w {
            let ex = (x as f32 + 0.5 - wf / 2.0) * hf / wf;
            let ey = y as f32 + 0.5 - hf / 2.0;
            let d = (ex * ex + ey * ey).sqrt();
            let cover = (hf / 2.0 - d + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 { continue; }
            let t = ((d - r0) / (r1 - r0)).clamp(0.0, 1.0);
            let a = alpha_at(t) * cover;
            if a > 0.0 { cv.blend(x, y, sr, sg, sb, a); }
        }
    }
    let half = (style.open / 2.0).to_radians();
    let (arm, tip_x, cy) = (wf * 0.3, wf * (0.32 - style.inset), hf / 2.0);
    let (lr, lg, lb) = hex_rgb(&style.line);
    cv.polyline(&[(tip_x + arm * half.cos(), cy - arm * half.sin()), (tip_x, cy), (tip_x + arm * half.cos(), cy + arm * half.sin())], hf * style.width, (lr, lg, lb, 1.0));
    (w as u32, h as u32, cv.px)
}

/// 色の鮮やかさと明るさ（three の Color.getHSL。値は 0..255 のまま持つ色を /255 して読む）
fn tone(c: &[f32; 3]) -> (f32, f32) {
    let (r, g, b) = (c[0] / 255.0, c[1] / 255.0, c[2] / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mn + mx) / 2.0;
    if mn == mx { return (0.0, l); }
    let d = mx - mn;
    let s = if l <= 0.5 { d / (mx + mn) } else { d / (2.0 - mx - mn) };
    (s, l)
}

/// ＞＜ の板（頭の骨に付く。頂点はできたときの世界の座標 = 骨の逆行列を掛けて付ける）
pub struct HurtFace {
    pub head: usize,
    /// 骨の世界の逆行列（three の head.attach）
    pub head_inverse: Mat4,
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub tex: (u32, u32, Vec<u8>),
    /// 板を見せている間に隠すプリミティブの番号（style.hide）
    pub hidden: Vec<usize>,
    pub opacity: f32,
    pub visible: bool,
}

impl HurtFace {
    /// 見せる強さ（0〜1）。戻り値は隠すプリミティブを隠すか
    pub fn set(&mut self, weight: f32) -> bool {
        let v = weight.clamp(0.0, 1.0);
        self.opacity = v;
        self.visible = v > 0.01;
        v >= 0.5
    }
}

/// ＞＜ の板を 2 枚作る（bind の姿のまま、アニメを当てる前に呼ぶ）。forward は顔の正面。
/// 板は平らにせず、マス目の点ごとに、目を閉じた顔の面へ前から光線を当てて、その面のすぐ上に置く
pub fn create_hurt_face(c: &Ctx, style: &HurtFaceStyle, forward: Vec3) -> Option<HurtFace> {
    let (left, right) = eye_regions_of(c, forward, true).or_else(|| eye_regions_of(c, forward, false))?;
    let head = match &style.attachment_node {
        Some(n) => c.skel.find(n),
        None => c.skel.find("J_Bip_C_Head").or_else(|| c.skel.find("Head")).or_else(|| c.skel.find("mixamorigHead")),
    }?;
    let (gx, gy) = style.grid.unwrap_or((10, 8));
    let between = (left.center + right.center) * 0.5;
    struct Plan { eye: EyeRegion, n: Vec3, x: Vec3, y: Vec3, w: f32, h: f32, center: Vec3 }
    let plans: Vec<Plan> = [left, right].iter().map(|eye| {
        let n = eye.normal.unwrap_or(forward).lerp(forward, 0.5).normalize();
        let mut x = eye.center - between;
        x -= n * x.dot(n);
        if x.length_squared() < 1e-12 { x = Vec3::Y.cross(n); }
        let x = x.normalize();
        let mut y = n.cross(x).normalize();
        if y.dot(Vec3::Y) < 0.0 { y = -y; }
        let w = eye.width * style.size;
        let h = w * style.tall.unwrap_or(0.75);
        Plan { eye: *eye, n, x, y, w, h, center: eye.center + y * (h * style.lift) }
    }).collect();
    let mut bx = Box3::empty();
    for p in &plans {
        for q in [p.center + p.x * p.w, p.center - p.x * p.w, p.center + p.y * p.h, p.center - p.y * p.h, p.center + p.n * p.w, p.center - p.n * p.w] { bx.expand(q); }
    }
    // 板を見せている間に隠すメッシュ（style.hide）。板は乗せない
    let skip = style.hide.as_deref();
    let hidden: Vec<usize> = c.prims.iter().enumerate().filter(|(_, pr)| skip.is_some_and(|s| hide_match(s, &pr.name))).map(|(i, _)| i).collect();
    let tris = face_proxy(c, &bx, skip);
    let mut reader = TexReader { g: c.g, drawn: HashMap::new() };
    let mut skin: Vec<[f32; 3]> = Vec::new();
    let (cols, count) = (gx + 1, (gx + 1) * (gy + 1));
    let mut grids: Vec<(Vec<Vec3>, Vec<f32>, Vec<Option<[f32; 3]>>)> = Vec::new();
    for p in &plans {
        let (back, pc, px, py, pn, pw, ph) = (-p.n.as_dvec3(), p.center.as_dvec3(), p.x.as_dvec3(), p.y.as_dvec3(), p.n.as_dvec3(), p.w as f64, p.h as f64);
        let mut seen: Vec<Option<[f32; 3]>> = Vec::with_capacity(count);
        // マス目の点ごとに、前から光線を当てた深さ。前髪のように目より 0.25 枚ぶん（style.front があればその割合）以上手前の面は飛ばす
        let mut depth: Vec<Option<f32>> = Vec::with_capacity(count);
        let front_k = style.front.unwrap_or(0.25);
        for j in 0..=gy {
            for i in 0..=gx {
                let (u, v) = (i as f64 / gx as f64, j as f64 / gy as f64);
                let o = pc + px * ((u - 0.5) * pw) + py * ((v - 0.5) * ph) + pn * pw;
                let far = pw * 2.0;
                let front_d = front_k as f64;
                let mut best: Option<(f64, usize, Vec3)> = None;
                for (ti, t) in tris.iter().enumerate() {
                    let Some((dist, bary)) = ray_tri(o, back, t) else { continue };
                    if dist > far { continue; }
                    if dist > pw * (1.0 - front_d) && dist < pw * 1.6 && best.is_none_or(|b| dist < b.0) { best = Some((dist, ti, bary)); }
                }
                depth.push(best.map(|b| b.0 as f32));
                // 肌の色は、板の下の顔全体から集める
                let col = best.and_then(|(_, ti, b)| {
                    let t = &tris[ti];
                    reader.read(&c.materials[t.material], t.ua * b.x + t.ub * b.y + t.uc * b.z)
                });
                if let Some(cc) = col { skin.push(cc); }
                seen.push(col);
            }
        }
        // 顔に当たらなかった点は透明にし、深さは当たった近くの点から埋める
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
        // まつ毛の細い帯のすき間で板がへこまないよう、近くの点のいちばん手前を取ってから、ならす
        let front: Vec<f32> = (0..count).map(|k| {
            let (i, j) = ((k % cols) as i32, (k / cols) as i32);
            let mut m = f32::INFINITY;
            for b in -1..=1 { for a in -1..=1 { m = m.min(known[idx(i + a, j + b)]); } }
            m
        }).collect();
        let smooth: Vec<f32> = (0..count).map(|k| {
            let (i, j) = ((k % cols) as i32, (k / cols) as i32);
            let (mut sum, mut n) = (0.0, 0);
            for b in -1..=1 { for a in -1..=1 { sum += front[idx(i + a, j + b)]; n += 1; } }
            let d = (sum / n as f32).min(known[k]);
            let (u, v) = (i as f32 / gx as f32 - 0.5, j as f32 / gy as f32 - 0.5);
            let limit = if (u / 0.4).powi(2) + (v / 0.38).powi(2) <= 1.0 { style.hug_eye } else { style.hug };
            match limit { None => d, Some(l) => d.max(known[k] - l * p.eye.width) }
        }).collect();
        // 透けの強さ: 当たった点は 1、当たらない点は 0。縁でなめらかに消えるよう、近くの点とならす
        let alpha: Vec<f32> = (0..count).map(|k| {
            if hit_alpha[k] == 0.0 { return 0.0; }
            let (i, j) = ((k % cols) as i32, (k / cols) as i32);
            let (mut sum, mut n) = (0.0, 0);
            for b in -1..=1 { for a in -1..=1 {
                let (ii, jj) = (i + a, j + b);
                if ii < 0 || jj < 0 || ii > gx as i32 || jj > gy as i32 { continue; }
                sum += hit_alpha[jj as usize * cols + ii as usize];
                n += 1;
            }}
            sum / n as f32
        }).collect();
        let mut positions = Vec::with_capacity(count);
        for j in 0..=gy {
            for i in 0..=gx {
                let (u, v, k) = (i as f32 / gx as f32, j as f32 / gy as f32, j * cols + i);
                positions.push(p.center + p.x * ((u - 0.5) * p.w) + p.y * ((v - 0.5) * p.h) + p.n * (p.w - smooth[k] + p.eye.width * 0.015));
            }
        }
        grids.push((positions, alpha, seen));
    }
    // 肌の色: 暗い所と鮮やかな所を外し、残りのくすんだ側の真ん中。読めなければ、閉じたまぶたの色・表の色
    let mut bright: Vec<[f32; 3]> = skin.iter().filter(|c| tone(c).1 > 0.6).copied().collect();
    bright.sort_by(|a, b| tone(a).0.total_cmp(&tone(b).0));
    let mut calm: Vec<[f32; 3]> = bright.iter().take(1usize.max((bright.len() as f32 * 0.5).ceil() as usize)).copied().collect();
    calm.sort_by(|a, b| (a[0] + a[1] + a[2]).total_cmp(&(b[0] + b[1] + b[2])));
    let mid: Option<[f32; 3]> = if bright.len() >= 4 { let m = calm[calm.len() / 2]; Some([m[0].min(255.0).round(), m[1].min(255.0).round(), m[2].min(255.0).round()]) } else { None };
    let face_color = mid.map(|m| format!("#{:02x}{:02x}{:02x}", m[0] as u8, m[1] as u8, m[2] as u8));
    // 当たった面の色が肌から大きく離れている所（顔の縁にかかる髪など）は透明にする。目のまわりは覆いたいので除く
    if let Some(m) = mid {
        for (_, alpha, seen) in &mut grids {
            for k in 0..count {
                let (i, j) = (k % cols, k / cols);
                let (u, v) = (i as f32 / gx as f32 - 0.5, j as f32 / gy as f32 - 0.5);
                let near = (u / 0.4).powi(2) + (v / 0.38).powi(2) > 1.0;
                if let (true, Some(cc)) = (near, seen[k]) && (cc[0] - m[0]).abs() + (cc[1] - m[1]).abs() + (cc[2] - m[2]).abs() > 90.0 { alpha[k] = 0.0; }
            }
        }
    }
    let skin_hex = if style.fixed_skin { None } else { face_color.or_else(|| lid_color(c, &mut reader)) }.unwrap_or_else(|| style.skin.clone());
    let tex = draw_eye(style, &skin_hex);
    let mut vertices = Vec::with_capacity(count * 2);
    let mut indices = Vec::new();
    for (g, (positions, alpha, _)) in grids.iter().enumerate() {
        let base = (g * count) as u32;
        for j in 0..=gy {
            for i in 0..=gx {
                let k = j * cols + i;
                // CanvasTexture は flipY（絵の上が v = 1）。テクスチャは上の行から載せるので v を返す
                vertices.push(Vertex { pos: positions[k].to_array(), normal: forward.to_array(), uv: [i as f32 / gx as f32, 1.0 - j as f32 / gy as f32], color: [1.0, 1.0, 1.0, alpha[k]], sway: 0.0 });
            }
        }
        for j in 0..gy {
            for i in 0..gx {
                let a = (j * cols + i) as u32 + base;
                let (b, cc, d) = (a + 1, a + cols as u32, a + cols as u32 + 1);
                indices.extend_from_slice(&[a, b, d, a, d, cc]);
            }
        }
    }
    Some(HurtFace { head, head_inverse: c.skel.world[head].inverse(), vertices, indices, tex, hidden, opacity: 0.0, visible: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ray_and_tone() {
        let t = Tri { a: Vec3::new(-1.0, -1.0, 0.0), b: Vec3::new(1.0, -1.0, 0.0), c: Vec3::new(0.0, 1.0, 0.0), ua: Vec2::ZERO, ub: Vec2::X, uc: Vec2::Y, material: 0 };
        let (d, b) = ray_tri(DVec3::new(0.0, 0.0, 2.0), DVec3::new(0.0, 0.0, -1.0), &t).unwrap();
        assert!((d - 2.0).abs() < 1e-6 && (b.x + b.y + b.z - 1.0).abs() < 1e-6);
        assert!(ray_tri(DVec3::new(5.0, 0.0, 2.0), DVec3::new(0.0, 0.0, -1.0), &t).is_none());
        // three: new Color(.5,.25,.25).getHSL → s=.3333, l=.375
        let (s, l) = tone(&[127.5, 63.75, 63.75]);
        assert!((s - 1.0 / 3.0).abs() < 1e-5 && (l - 0.375).abs() < 1e-5);
    }
}
