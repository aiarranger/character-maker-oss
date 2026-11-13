// ファンタジーの「大人の顔」の頭の形（今の版は lib/character-maker/fantasy-head.ts。式と順はそのまま、倍精度で計算して f32 で持つ）。
// 顔と体の部分の位置・法線・モーフの差分を、同じ解剖の歪みで写す（目の回転の中心と頭の上は動かさない）。
use character_actor::glb::Primitive;

fn smooth(a: f64, b: f64, x: f64) -> f64 { let t = ((x - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

/// fantasyHeadPoint
pub fn point(masculine: bool, vx: f64, y: f64, z: f64, body: bool) -> [f64; 3] {
    let (mut px, mut py, mut pz) = (vx, y, z);
    if masculine {
        let x = vx - 0.00076345;
        let ax = x.abs();
        let side = if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 };
        for s in [-1.0, 1.0] {
            let (cx, cy) = (s * 0.042, 1.7278);
            let w = (-((x - cx) / 0.039).powi(6) - ((y - cy) / 0.040).powi(6)).exp() * smooth(-0.041, -0.068, z);
            px += (x - cx) * -0.10 * w;
            py += (y - cy) * -0.12 * w;
        }
        let jaw = (-((y - 1.651) / 0.041).powi(4)).exp() * smooth(0.016, 0.057, ax) * (1.0 - smooth(0.085, 0.112, ax));
        px += side * 0.009 * jaw;
        let chin = (1.0 - smooth(1.631, 1.653, y)) * smooth(1.605, 1.622, y) * smooth(-0.052, -0.077, z);
        px += x * 0.20 * chin;
        py += (0.003 * (1.0 - smooth(0.006, 0.026, ax)) - 0.003 * smooth(0.02, 0.053, ax)) * chin;
        let bridge = (-(x / 0.018).powi(4) - ((y - 1.694) / 0.037).powi(4)).exp() * smooth(-0.070, -0.097, z);
        pz -= 0.007 * bridge;
        py -= 0.003 * bridge;
        let cheek = (-((y - 1.698) / 0.026).powi(4)).exp() * smooth(0.05, 0.07, ax) * (1.0 - smooth(0.09, 0.11, ax));
        px += side * 0.0025 * cheek;
        if body {
            let neck = (-((y - 1.627) / 0.055).powi(4)).exp() * (1.0 - smooth(0.05, 0.084, ax));
            px += x * 0.19 * neck;
            pz += (z + 0.015) * 0.10 * neck;
        }
    } else {
        let x = vx - 0.00048;
        let ax = x.abs();
        let lower = (1.0 - smooth(1.54, 1.61, y)) * smooth(1.452, 1.48, y);
        px -= x * 0.095 * lower;
        py -= 0.005 * (-((y - 1.487) / 0.045).powi(4)).exp() * (1.0 - smooth(0.025, 0.085, ax));
        for s in [-1.0, 1.0] {
            let w = (-((x - s * 0.041) / 0.039).powi(6) - ((y - 1.566) / 0.040).powi(6)).exp() * smooth(-0.035, -0.064, z);
            px += (x - s * 0.041) * -0.035 * w;
            py += (y - 1.566) * -0.075 * w;
        }
    }
    [px, py, pz]
}

/// fantasyHeadNormal（中心差分のヤコビアンの余因子で法線を運ぶ）
fn normal(masculine: bool, x: f64, y: f64, z: f64, nx: f64, ny: f64, nz: f64, body: bool) -> [f64; 3] {
    let h = 1e-7;
    let xp = point(masculine, x + h, y, z, body); let xm = point(masculine, x - h, y, z, body);
    let yp = point(masculine, x, y + h, z, body); let ym = point(masculine, x, y - h, z, body);
    let zp = point(masculine, x, y, z + h, body); let zm = point(masculine, x, y, z - h, body);
    let (ax, ay, az) = ((xp[0] - xm[0]) / (2.0 * h), (xp[1] - xm[1]) / (2.0 * h), (xp[2] - xm[2]) / (2.0 * h));
    let (bx, by, bz) = ((yp[0] - ym[0]) / (2.0 * h), (yp[1] - ym[1]) / (2.0 * h), (yp[2] - ym[2]) / (2.0 * h));
    let (cx, cy, cz) = ((zp[0] - zm[0]) / (2.0 * h), (zp[1] - zm[1]) / (2.0 * h), (zp[2] - zm[2]) / (2.0 * h));
    let (cxx, cxy, cxz) = (by * cz - bz * cy, bz * cx - bx * cz, bx * cy - by * cx);
    let det = ax * cxx + ay * cxy + az * cxz;
    let tx = nx * cxx + ny * (cy * az - cz * ay) + nz * (ay * bz - az * by);
    let ty = nx * cxy + ny * (cz * ax - cx * az) + nz * (az * bx - ax * bz);
    let tz = nx * cxz + ny * (cx * ay - cy * ax) + nz * (ax * by - ay * bx);
    let len = (tx * tx + ty * ty + tz * tz).sqrt();
    if !len.is_finite() || len < 1e-12 || !det.is_finite() || det.abs() < 1e-8 { return [nx, ny, nz]; }
    let s = det.signum() / len;
    [tx * s, ty * s, tz * s]
}

const F: fn([f64; 3]) -> [f32; 3] = |p| [p[0] as f32, p[1] as f32, p[2] as f32];

/// 1 つの部分を大人の顔の形へ写す（createFantasyHead の 1 つの mesh の分。差分は glTF の相対のまま）
pub fn warp(p: &mut Primitive, masculine: bool, body: bool) {
    let orig = p.positions.clone();
    let onorm = p.normals.clone();
    let omorph = p.morph_pos.clone();
    let base: Vec<[f64; 3]> = orig.iter().map(|v| point(masculine, v[0] as f64, v[1] as f64, v[2] as f64, body)).collect();
    // f32 に丸めた歪んだ位置（three の position は Float32Array）
    let base32: Vec<[f32; 3]> = base.iter().map(|v| F(*v)).collect();
    for d in p.morph_pos.iter_mut() {
        for (i, v) in d.iter_mut().enumerate() {
            let e = point(masculine, (v[0] + orig[i][0]) as f64, (v[1] + orig[i][1]) as f64, (v[2] + orig[i][2]) as f64, body);
            *v = [(e[0] - base32[i][0] as f64) as f32, (e[1] - base32[i][1] as f64) as f32, (e[2] - base32[i][2] as f64) as f32];
        }
    }
    if onorm.len() == orig.len() {
        let n32: Vec<[f32; 3]> = orig.iter().zip(&onorm).map(|(v, n)| F(normal(masculine, v[0] as f64, v[1] as f64, v[2] as f64, n[0] as f64, n[1] as f64, n[2] as f64, body))).collect();
        // 法線の差分（モーフの位置の先で、モーフの法線の先を写す）。位置は写す前の値で読む
        for k in 0..p.morph_nor.len() {
            for i in 0..orig.len() {
                let d = p.morph_nor[k][i];
                let m = omorph.get(k).map_or([0.0; 3], |t| t[i]);
                let at = [(m[0] + orig[i][0]) as f64, (m[1] + orig[i][1]) as f64, (m[2] + orig[i][2]) as f64];
                let n = normal(masculine, at[0], at[1], at[2], (d[0] + onorm[i][0]) as f64, (d[1] + onorm[i][1]) as f64, (d[2] + onorm[i][2]) as f64, body);
                p.morph_nor[k][i] = [(n[0] - n32[i][0] as f64) as f32, (n[1] - n32[i][1] as f64) as f32, (n[2] - n32[i][2] as f64) as f32];
            }
        }
        p.normals = n32;
    }
    p.positions = base32;
}
