// 数学関数が Node の Math と同じ（検査値 tests/fixtures/math.txt は旧版の scripts/sim-math.ts が書き出した固定の値。生成元は 2026-11-10 に削除）。同じ入力の列を作る。
use sim::battle::{chain_score, rng};
use sim::math::{self, PI};
use std::collections::HashMap;

const FNV: u32 = 0x811c9dc5;
#[inline] fn mix32(h: u32, w: u32) -> u32 { (h ^ w).wrapping_mul(16777619) }
#[inline] fn mix_f(h: u32, x: f64) -> u32 {
    if x.is_nan() { return mix32(mix32(h, 0), 0x7ff80000); }
    let b = x.to_bits();
    mix32(mix32(h, b as u32), (b >> 32) as u32)
}

#[test]
fn math_matches_node() {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/math.txt")).unwrap();
    let mut lines = text.lines();
    let header = lines.next().unwrap();
    let n: u64 = header.rsplit("n=").next().unwrap().parse().unwrap();
    let want: HashMap<&str, u32> = lines.map(|l| { let (k, v) = l.split_once(' ').unwrap(); (k, u32::from_str_radix(v, 16).unwrap()) }).collect();

    const SCALES: [f64; 10] = [1.0, 3.0, 10.0, 100.0, 1000.0, 1e5, 700000.0, 0.01, 1e-5, 1e-10];
    const SCALES_B: [f64; 6] = [1.0, 10.0, 1e3, 1e-3, 1e-20, 1e20];
    const EXPS: [f64; 7] = [0.1, 1.0, 2.0, 4.0, 40.0, 1400.0, 1e-9];
    let special: [f64; 32] = [0.0, -0.0, 1.0, -1.0, 1e-30, -1e-30, 1e-9, 1e300, -1e300, f64::INFINITY, f64::NEG_INFINITY, f64::NAN, 0.5, PI / 2.0, -PI / 2.0, PI, 3.0 * PI / 4.0, 0.4375, 0.6875, 1.1875, 2.4375, 2f64.powi(-29), 2f64.powi(-28), 0.34657359, 1.0397207, 709.0, -745.0, 710.0, -746.0, 1e17, -1e17, 2f64.powi(66)];
    let mut r = rng(54321);
    let (mut sin, mut cos, mut atan2, mut hypot, mut exp) = (FNV, FNV, FNV, FNV, FNV);
    for &a in &special {
        sin = mix_f(sin, math::sin(a)); cos = mix_f(cos, math::cos(a)); exp = mix_f(exp, math::exp(a));
        for &b in &special { atan2 = mix_f(atan2, math::atan2(a, b)); hypot = mix_f(hypot, math::hypot(a, b)); }
    }
    for i in 0..n as usize {
        let a = (r.next() - 0.5) * 2.0 * SCALES[i % SCALES.len()];
        let b = (r.next() - 0.5) * 2.0 * SCALES_B[i % SCALES_B.len()];
        sin = mix_f(sin, math::sin(a)); cos = mix_f(cos, math::cos(a)); atan2 = mix_f(atan2, math::atan2(a, b)); hypot = mix_f(hypot, math::hypot(a, b));
        exp = mix_f(exp, math::exp((r.next() - 0.5) * EXPS[i % EXPS.len()]));
        let q = (r.next() * 2000.0).floor() * PI / 2.0 + (r.next() - 0.5) * 1e-8;
        sin = mix_f(sin, math::sin(q)); cos = mix_f(cos, math::cos(q));
    }
    let mut chain = FNV;
    for k in 10..=100000 { chain = mix_f(chain, chain_score(k)); }
    for (name, got) in [("sin", sin), ("cos", cos), ("atan2", atan2), ("hypot", hypot), ("exp", exp), ("chain", chain)] {
        assert_eq!(got, want[name], "{name} が Node と違う（{header}）");
    }
}

#[test]
fn round_max_min_follow_js() {
    assert_eq!(math::round(0.49999999999999994), 0.0);
    assert_eq!(math::round(2.5), 3.0);
    assert_eq!(math::round(-2.5), -2.0);
    assert_eq!(math::max(0.0, -0.0).to_bits(), 0f64.to_bits());
    assert_eq!(math::max(-0.0, 0.0).to_bits(), 0f64.to_bits());
    assert_eq!(math::min(0.0, -0.0).to_bits(), (-0f64).to_bits());
    assert!(math::max(1.0, f64::NAN).is_nan() && math::min(f64::NAN, 1.0).is_nan());
    assert_eq!(math::or1(f64::NAN), 1.0);
    assert_eq!(math::or1(0.0), 1.0);
}
