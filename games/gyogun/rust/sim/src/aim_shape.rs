// 狙いの形: 技の表から、敵を弾にする当たりとその範囲を読む（装填の数と足元の光に使う）。
use crate::math::{self, atan2, cos, hypot, sin};
use crate::moves::{Hit, HitShape, Knock, Move};

#[derive(Clone, Copy, Debug)]
pub struct AimShape {
    pub hit: &'static Hit,
    /// 当たるまでに踏み込む距離
    pub lead: f64,
}

/// 技の中で、敵を弾にするいちばん強い当たり（吹き飛ばしを打ち上げより優先）
pub fn aim_shape(m: &'static Move) -> Option<AimShape> {
    let mut hit: Option<&'static Hit> = None;
    for h in &m.hits {
        if h.kb != Knock::Blast && h.kb != Knock::Launch { continue; }
        match hit {
            None => hit = Some(h),
            Some(c) => if (h.kb == Knock::Blast && c.kb != Knock::Blast) || (h.kb == c.kb && h.power >= c.power) { hit = Some(h); },
        }
    }
    let hit = hit?;
    let mut lead = 0.0;
    for &[a, b, d] in &m.lunge { if hit.t > a { lead += d * math::min(1.0, (hit.t - a) / (b - a)); } }
    Some(AimShape { hit, lead })
}

/// (x, z) にいる半径 rad の敵が、その当たりの範囲に入るか（戦いの計算と同じ形）
pub fn in_aim(s: &AimShape, px: f64, pz: f64, face: f64, x: f64, z: f64, rad: f64) -> bool {
    let h = s.hit;
    let (fx, fz) = (sin(face), cos(face));
    let cx = px + fx * (s.lead + h.off.unwrap_or(0.0));
    let cz = pz + fz * (s.lead + h.off.unwrap_or(0.0));
    let (dx, dz) = (x - cx, z - cz);
    match h.shape {
        HitShape::Circle => dx * dx + dz * dz <= math::sq(h.r + rad),
        HitShape::Arc => {
            let d = hypot(dx, dz);
            if d > h.r + rad { return false; }
            let mut a = atan2(dx, dz) - face;
            a = atan2(sin(a), cos(a));
            d < 1.2 || a.abs() <= h.arc.unwrap_or(1.0) + rad / math::max(d, 0.5)
        }
        HitShape::Line => {
            let (along, side) = (dx * fx + dz * fz, (dx * fz - dz * fx).abs());
            along >= -1.0 && along <= h.r + rad && side <= h.w.unwrap_or(2.0) / 2.0 + rad
        }
    }
}
