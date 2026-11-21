//! JavaScript の Math と同じ結果をビット単位で返す数学関数（比較の基準は Node 24 / V8 13.6 の arm64 版）。
//
// V8 の sin / cos / atan2 / exp は fdlibm の移植（src/base/ieee754.cc）だが、arm64 の Node は clang の既定
// （-ffp-contract=on）で a * b + c を融合積和（FMA）にしてビルドされている。そのため融合しない libm クレートとは
// sin / cos で 1% ほど、atan2 / exp でも 0.1% ほど最後の 1 ビットが分かれる。ここでは node の実行ファイルの
// 逆アセンブルで確かめた融合の位置どおりに mul_add（ここでは fma）を置く。mul_add は端末を問わず本物の FMA の結果を返す。
// x86-64 の Node は FMA なしでビルドされるので、TypeScript 版そのものが CPU で結果が変わる点に注意。

/// 融合積和。ブラウザ（wasm）には FMA 命令が無く mul_add はソフトウェアで遅いので、ふつうの掛け算と足し算にする。
/// そのため wasm の結果は Node（arm64）とビット単位では一致しない（一致の試験は Mac 版だけ）。
trait Fma { fn fma(self, a: f64, b: f64) -> f64; }
impl Fma for f64 {
    #[inline(always)]
    fn fma(self, a: f64, b: f64) -> f64 {
        #[cfg(target_arch = "wasm32")] { self * a + b }
        #[cfg(not(target_arch = "wasm32"))] { self.mul_add(a, b) }
    }
}

pub use std::f64::consts::PI;

// A browser may round transcendental functions differently from Node arm64.
// Opt in only where an angle boundary changes the game's turning direction.
// Native traces keep the existing Node implementation, even with this feature.
#[cfg(all(target_arch = "wasm32", feature = "browser-math"))]
pub use js_sys::Math::{atan2, cos, sin};

#[inline] fn hi(x: f64) -> u32 { (x.to_bits() >> 32) as u32 }
#[inline] fn f(bits: u64) -> f64 { f64::from_bits(bits) }

// ---- sin / cos ----
const S1: f64 = f64::from_bits(0xbfc5555555555549);
const S2: f64 = f64::from_bits(0x3f8111111110f8a6);
const S3: f64 = f64::from_bits(0xbf2a01a019c161d5);
const S4: f64 = f64::from_bits(0x3ec71de357b1fe7d);
const S5: f64 = f64::from_bits(0xbe5ae5e68a2b9ceb);
const S6: f64 = f64::from_bits(0x3de5d93a5acfd57c);
const C1: f64 = f64::from_bits(0x3fa555555555554c);
const C2: f64 = f64::from_bits(0xbf56c16c16c15177);
const C3: f64 = f64::from_bits(0x3efa01a019cb1590);
const C4: f64 = f64::from_bits(0xbe927e4f809c52ad);
const C5: f64 = f64::from_bits(0x3e21ee9ebdb4b1c4);
const C6: f64 = f64::from_bits(0xbda8fae9be8838d4);

#[inline]
fn kernel_sin(x: f64, y: f64, iy: bool) -> f64 {
    let ix = hi(x) & 0x7fffffff;
    if ix < 0x3e400000 && (x as i32) == 0 { return x; }
    let z = x * x;
    let v = z * x;
    let r = z.fma(z.fma(z.fma(z.fma(S6, S5), S4), S3), S2);
    if !iy {
        v.fma(z.fma(r, S1), x)
    } else {
        let inner = y.fma(0.5, -(v * r));
        let t = z.fma(inner, -y);
        x - (-v).fma(S1, t)
    }
}

#[inline]
fn kernel_cos(x: f64, y: f64) -> f64 {
    let ix = hi(x) & 0x7fffffff;
    if ix < 0x3e400000 && (x as i32) == 0 { return 1.0; }
    let z = x * x;
    let r = z * z.fma(z.fma(z.fma(z.fma(z.fma(C6, C5), C4), C3), C2), C1);
    let t = z.fma(r, -(x * y));
    if ix < 0x3fd33333 {
        1.0 - z.fma(0.5, -t)
    } else {
        let qx = if ix > 0x3fe90000 { 0.28125 } else { f(((ix - 0x00200000) as u64) << 32) };
        let iz = z.fma(0.5, -qx);
        (1.0 - qx) - (iz - t)
    }
}

const NPIO2_HW: [u32; 32] = [
    0x3ff921fb, 0x400921fb, 0x4012d97c, 0x401921fb, 0x401f6a7a, 0x4022d97c, 0x4025fdbb, 0x402921fb,
    0x402c463a, 0x402f6a7a, 0x4031475c, 0x4032d97c, 0x40346b9c, 0x4035fdbb, 0x40378fdb, 0x403921fb,
    0x403ab41b, 0x403c463a, 0x403dd85a, 0x403f6a7a, 0x40407e4c, 0x4041475c, 0x4042106c, 0x4042d97c,
    0x4043a28c, 0x40446b9c, 0x404534ac, 0x4045fdbb, 0x4046c6cb, 0x40478fdb, 0x404858eb, 0x404921fb,
];
const INVPIO2: f64 = f64::from_bits(0x3fe45f306dc9c883);
const PIO2_1: f64 = f64::from_bits(0x3ff921fb54400000);
const PIO2_1T: f64 = f64::from_bits(0x3dd0b4611a626331);
const PIO2_2: f64 = f64::from_bits(0x3dd0b4611a600000);
const PIO2_2T: f64 = f64::from_bits(0x3ba3198a2e037073);
const PIO2_3: f64 = f64::from_bits(0x3ba3198a2e000000);
const PIO2_3T: f64 = f64::from_bits(0x397b839a252049c1);

/// |x| が 2^19 * pi/2 以下のときの角度の折りたたみ。呼ぶ側が範囲を確かめる
#[inline]
fn rem_pio2(x: f64) -> (i32, f64, f64) {
    let hx = hi(x) as i32;
    let ix = hx as u32 & 0x7fffffff;
    if ix < 0x4002d97c {
        // |x| < 3pi/4
        if hx > 0 {
            let z = x - PIO2_1;
            if ix != 0x3ff921fb {
                let y0 = z - PIO2_1T;
                return (1, y0, (z - y0) - PIO2_1T);
            }
            let z = z - PIO2_2;
            let y0 = z - PIO2_2T;
            return (1, y0, (z - y0) - PIO2_2T);
        }
        let z = x + PIO2_1;
        if ix != 0x3ff921fb {
            let y0 = z + PIO2_1T;
            return (-1, y0, (z - y0) + PIO2_1T);
        }
        let z = z + PIO2_2;
        let y0 = z + PIO2_2T;
        return (-1, y0, (z - y0) + PIO2_2T);
    }
    let t = x.abs();
    let n = t.fma(INVPIO2, 0.5) as i32;
    let fn_ = n as f64;
    let mut r = fn_.fma(-PIO2_1, t);
    let mut w = fn_ * PIO2_1T;
    let mut y0 = r - w;
    if !(n < 32 && ix != NPIO2_HW[(n - 1) as usize]) {
        let j = ix >> 20;
        let i = j as i32 - ((hi(y0) >> 20) & 0x7ff) as i32;
        if i > 16 {
            let t = r;
            w = fn_ * PIO2_2;
            r = t - w;
            w = fn_.fma(PIO2_2T, -((t - r) - w));
            y0 = r - w;
            let i = j as i32 - ((hi(y0) >> 20) & 0x7ff) as i32;
            if i > 49 {
                let t = r;
                w = fn_ * PIO2_3;
                r = t - w;
                w = fn_.fma(PIO2_3T, -((t - r) - w));
                y0 = r - w;
            }
        }
    }
    let y1 = (r - y0) - w;
    if hx < 0 { (-n, -y0, -y1) } else { (n, y0, y1) }
}

/// Math.sin。|x| > 2^19 * pi/2（約 82 万 rad）だけは libm へ任せる（戦いの中でこの大きさの角度は出ない）
#[cfg(not(all(target_arch = "wasm32", feature = "browser-math")))]
pub fn sin(x: f64) -> f64 {
    let ix = hi(x) & 0x7fffffff;
    if ix <= 0x3fe921fb { return kernel_sin(x, 0.0, false); }
    if ix >= 0x7ff00000 { return x - x; }
    if ix > 0x413921fb { return libm::sin(x); }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_sin(y0, y1, true),
        1 => kernel_cos(y0, y1),
        2 => -kernel_sin(y0, y1, true),
        _ => -kernel_cos(y0, y1),
    }
}

/// Math.cos（範囲は sin と同じ）
#[cfg(not(all(target_arch = "wasm32", feature = "browser-math")))]
pub fn cos(x: f64) -> f64 {
    let ix = hi(x) & 0x7fffffff;
    if ix <= 0x3fe921fb { return kernel_cos(x, 0.0); }
    if ix >= 0x7ff00000 { return x - x; }
    if ix > 0x413921fb { return libm::cos(x); }
    let (n, y0, y1) = rem_pio2(x);
    match n & 3 {
        0 => kernel_cos(y0, y1),
        1 => -kernel_sin(y0, y1, true),
        2 => -kernel_cos(y0, y1),
        _ => kernel_sin(y0, y1, true),
    }
}

// ---- atan / atan2 ----
const ATANHI: [f64; 4] = [f64::from_bits(0x3fddac670561bb4f), f64::from_bits(0x3fe921fb54442d18), f64::from_bits(0x3fef730bd281f69b), f64::from_bits(0x3ff921fb54442d18)];
const ATANLO: [f64; 4] = [f64::from_bits(0x3c7a2b7f222f65e2), f64::from_bits(0x3c81a62633145c07), f64::from_bits(0x3c7007887af0cbbd), f64::from_bits(0x3c91a62633145c07)];
const AT: [f64; 11] = [
    f64::from_bits(0x3fd555555555550d), f64::from_bits(0xbfc999999998ebc4), f64::from_bits(0x3fc24924920083ff),
    f64::from_bits(0xbfbc71c6fe231671), f64::from_bits(0x3fb745cdc54c206e), f64::from_bits(0xbfb3b0f2af749a6d),
    f64::from_bits(0x3fb10d66a0d03d51), f64::from_bits(0xbfadde2d52defd9a), f64::from_bits(0x3fa97b4b24760deb),
    f64::from_bits(0xbfa2b4442c6a6c2f), f64::from_bits(0x3f90ad3ae322da11),
];

/// Math.atan（atan2 の中で使う）
fn atan(x0: f64) -> f64 {
    let hx = hi(x0) as i32;
    let ix = hx as u32 & 0x7fffffff;
    let mut x = x0;
    if ix >= 0x44100000 {
        if x.is_nan() { return x + x; }
        return if hx > 0 { ATANHI[3] + ATANLO[3] } else { -ATANHI[3] - ATANLO[3] };
    }
    let id: i32;
    if ix < 0x3fdc0000 {
        if ix < 0x3e200000 { return x; }
        id = -1;
    } else {
        x = x.abs();
        if ix < 0x3ff30000 {
            if ix < 0x3fe60000 { id = 0; x = x.fma(2.0, -1.0) / (2.0 + x); }
            else { id = 1; x = (x - 1.0) / (x + 1.0); }
        } else if ix < 0x40038000 { id = 2; x = (x - 1.5) / x.fma(1.5, 1.0); }
        else { id = 3; x = -1.0 / x; }
    }
    let z = x * x;
    let w = z * z;
    let s1 = z * w.fma(w.fma(w.fma(w.fma(w.fma(AT[10], AT[8]), AT[6]), AT[4]), AT[2]), AT[0]);
    let s2 = w * w.fma(w.fma(w.fma(w.fma(AT[9], AT[7]), AT[5]), AT[3]), AT[1]);
    if id < 0 {
        x.fma(-(s1 + s2), x)
    } else {
        let id = id as usize;
        let t = x.fma(s1 + s2, -ATANLO[id]);
        let z = ATANHI[id] - (t - x);
        if hx < 0 { -z } else { z }
    }
}

const PI_O_4: f64 = f64::from_bits(0x3fe921fb54442d18);
const PI_O_2: f64 = f64::from_bits(0x3ff921fb54442d18);
const PI_LO: f64 = f64::from_bits(0x3ca1a62633145c07);

/// Math.atan2
#[cfg(not(all(target_arch = "wasm32", feature = "browser-math")))]
pub fn atan2(y: f64, x: f64) -> f64 {
    if x.is_nan() || y.is_nan() { return x + y; }
    let (xb, yb) = (x.to_bits(), y.to_bits());
    let (hx, lx) = ((xb >> 32) as u32 as i32, xb as u32);
    let (hy, ly) = ((yb >> 32) as u32 as i32, yb as u32);
    let (ix, iy) = (hx as u32 & 0x7fffffff, hy as u32 & 0x7fffffff);
    if (hx.wrapping_sub(0x3ff00000) as u32 | lx) == 0 { return atan(y); }
    let mut m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
    if (iy | ly) == 0 {
        return match m { 0 | 1 => y, 2 => PI, _ => -PI };
    }
    if (ix | lx) == 0 { return if hy < 0 { -PI_O_2 } else { PI_O_2 }; }
    if ix == 0x7ff00000 {
        if iy == 0x7ff00000 {
            return match m { 0 => PI_O_4, 1 => -PI_O_4, 2 => 3.0 * PI_O_4, _ => -3.0 * PI_O_4 };
        }
        return match m { 0 => 0.0, 1 => -0.0, 2 => PI, _ => -PI };
    }
    if iy == 0x7ff00000 { return if hy < 0 { -PI_O_2 } else { PI_O_2 }; }
    let k = (iy as i32 - ix as i32) >> 20;
    let z;
    if k > 60 {
        z = PI_LO.fma(0.5, PI_O_2);
        m &= 1;
    } else if hx < 0 && k < -60 {
        z = 0.0;
    } else {
        z = atan((y / x).abs());
    }
    match m {
        0 => z,
        1 => -z,
        2 => PI - (z - PI_LO),
        _ => (z - PI_LO) - PI,
    }
}

// ---- exp ----
const LN2HI: f64 = f64::from_bits(0x3fe62e42fee00000);
const LN2LO: f64 = f64::from_bits(0x3dea39ef35793c76);
const INVLN2: f64 = f64::from_bits(0x3ff71547652b82fe);
const P1: f64 = f64::from_bits(0x3fc555555555553e);
const P2: f64 = f64::from_bits(0xbf66c16c16bebd93);
const P3: f64 = f64::from_bits(0x3f11566aaf25de2c);
const P4: f64 = f64::from_bits(0xbebbbd41c5d26bf1);
const P5: f64 = f64::from_bits(0x3e66376972bea4d0);

/// Math.exp
pub fn exp(x0: f64) -> f64 {
    let mut x = x0;
    let h = hi(x);
    let xsb = (h >> 31) as i32;
    let hx = h & 0x7fffffff;
    if hx >= 0x40862e42 {
        if hx >= 0x7ff00000 {
            if x.is_nan() { return x + x; }
            return if xsb == 0 { x } else { 0.0 };
        }
        if x > f64::from_bits(0x40862e42fefa39ef) { return f64::INFINITY; }
        if x < f64::from_bits(0xc0874910d52d3051) { return 0.0; }
    }
    let (mut hi_, mut lo, mut k) = (0.0, 0.0, 0i32);
    if hx > 0x3fd62e42 {
        if hx < 0x3ff0a2b2 {
            if x == 1.0 { return f64::from_bits(0x4005bf0a8b145769); }
            hi_ = x - if xsb == 0 { LN2HI } else { -LN2HI };
            lo = if xsb == 0 { LN2LO } else { -LN2LO };
            k = 1 - xsb - xsb;
        } else {
            k = x.fma(INVLN2, if xsb == 0 { 0.5 } else { -0.5 }) as i32;
            let t = k as f64;
            hi_ = t.fma(-LN2HI, x);
            lo = t * LN2LO;
        }
        x = hi_ - lo;
    } else if hx < 0x3e300000 {
        return 1.0 + x;
    }
    let t = x * x;
    let twopk = if k >= -1021 {
        f(((0x3ff00000i32 + ((k as u32) << 20) as i32) as u32 as u64) << 32)
    } else {
        f(((0x3ff00000i32 + (((k + 1000) as u32) << 20) as i32) as u32 as u64) << 32)
    };
    let poly = t.fma(t.fma(t.fma(t.fma(P5, P4), P3), P2), P1);
    let c = t.fma(-poly, x);
    if k == 0 { return 1.0 - ((x * c) / (c - 2.0) - x); }
    let y = 1.0 - ((lo - (x * c) / (2.0 - c)) - hi_);
    if k >= -1021 {
        if k == 1024 { return y * 2.0 * f64::from_bits(0x7fe0000000000000); }
        y * twopk
    } else {
        y * twopk * f64::from_bits(0x0170000000000000)
    }
}

// ---- pow ----
/// `x ** 1.5`（連鎖の得点だけで使う）。V8 の pow とは最後のビットが分かれることがあるが、
/// 得点は 14 倍して丸めるので、丸めたあとは 10 万体まで一致する（tests/math.rs の連鎖の得点の表で確かめている）
#[inline] pub fn pow(x: f64, y: f64) -> f64 { libm::pow(x, y) }

/// `x ** 2`。V8 の pow(x, 2) は x * x と一致する（2000 万点で確かめた。最適化後の V8 も x * x に置き換える）
#[inline] pub fn sq(x: f64) -> f64 { x * x }

// ---- hypot / round / max / min ----
/// Math.hypot（2 引数）: 最大値で割り、Kahan 加算で二乗和を足して sqrt し、最大値を掛ける（V8 の math.tq の MathHypot）
#[inline]
pub fn hypot(a: f64, b: f64) -> f64 {
    let (nan_a, nan_b) = (a.is_nan(), b.is_nan());
    let aa = if nan_a { 0.0 } else { a.abs() };
    let ab = if nan_b { 0.0 } else { b.abs() };
    let mut max = 0.0;
    if aa > max { max = aa; }
    if ab > max { max = ab; }
    if max == f64::INFINITY { return f64::INFINITY; }
    if nan_a || nan_b { return f64::NAN; }
    if max == 0.0 { return 0.0; }
    let (mut sum, mut compensation) = (0.0f64, 0.0f64);
    let n = aa / max;
    let summand = n * n - compensation;
    let preliminary = sum + summand;
    compensation = (preliminary - sum) - summand;
    sum = preliminary;
    let n = ab / max;
    let summand = n * n - compensation;
    sum += summand;
    sum.sqrt() * max
}

/// Math.round（V8 の Float64Round: ceil して、0.5 を超えて上げていたら 1 戻す）
#[inline]
pub fn round(x: f64) -> f64 {
    let c = x.ceil();
    if c - 0.5 > x { c - 1.0 } else { c }
}

/// Math.max（NaN が一つでもあれば NaN、+0 を -0 より大きいと見る）
#[inline]
pub fn max(a: f64, b: f64) -> f64 {
    if a > b { a } else if b > a { b } else if a == b { if a == 0.0 && a.is_sign_negative() { b } else { a } } else { f64::NAN }
}

/// Math.min（NaN が一つでもあれば NaN、-0 を +0 より小さいと見る）
#[inline]
pub fn min(a: f64, b: f64) -> f64 {
    if a < b { a } else if b < a { b } else if a == b { if a == 0.0 && b.is_sign_negative() { b } else { a } } else { f64::NAN }
}

/// `x || 1`（0 と NaN は 1）
#[inline]
pub fn or1(x: f64) -> f64 { if x == 0.0 || x.is_nan() { 1.0 } else { x } }

// ---- ここから下は V8 の組み込みをそのまま写す小さな物 ----
/// Math.sign（-0 と NaN はそのまま返す。Rust の signum は 0 で 1 を返すので使わない）
#[inline]
pub fn sign(x: f64) -> f64 {
    if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { x }
}

/// `x >>> 0`（ToUint32。NaN と無限大は 0、小数は切り捨て、2^32 で折り返す）
#[inline]
pub fn to_uint32(x: f64) -> u32 {
    if !x.is_finite() { return 0; }
    x.trunc().rem_euclid(4294967296.0) as u32
}
