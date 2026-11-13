// three の Color・オイラー角の式。
pub use glam::{Mat4, Quat, Vec2, Vec3, Vec4};

fn srgb_to_linear(c: f32) -> f32 { if c < 0.04045 { c * 0.0773993808 } else { (c * 0.9478672986 + 0.0521327014).powf(2.4) } }
fn linear_to_srgb(c: f32) -> f32 { if c < 0.0031308 { c * 12.92 } else { 1.055 * c.powf(0.41666) - 0.055 } }

/// 線形の色（three の Color と同じ。16 進は sRGB として直す）
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Color { pub r: f32, pub g: f32, pub b: f32 }
impl Color {
    pub const WHITE: Color = Color { r: 1.0, g: 1.0, b: 1.0 };
    pub const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0 };
    pub const fn rgb(r: f32, g: f32, b: f32) -> Color { Color { r, g, b } }
    pub fn hex(v: u32) -> Color { Color { r: srgb_to_linear((v >> 16 & 255) as f32 / 255.0), g: srgb_to_linear((v >> 8 & 255) as f32 / 255.0), b: srgb_to_linear((v & 255) as f32 / 255.0) } }
    /// "#rrggbb"
    pub fn css(s: &str) -> Color { Color::hex(u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap_or(0xffffff)) }
    pub fn lerp(self, o: Color, k: f32) -> Color { Color { r: self.r + (o.r - self.r) * k, g: self.g + (o.g - self.g) * k, b: self.b + (o.b - self.b) * k } }
    pub fn scale(self, k: f32) -> Color { Color { r: self.r * k, g: self.g * k, b: self.b * k } }
    /// three の getHexString（"rrggbb"。# なし）
    pub fn hex_string(self) -> String {
        let f = |c: f32| (linear_to_srgb(c).clamp(0.0, 1.0) * 255.0).round() as u32;
        format!("{:06x}", f(self.r) << 16 | f(self.g) << 8 | f(self.b))
    }
    pub fn to_array(self) -> [f32; 3] { [self.r, self.g, self.b] }
}

/// オイラー角の順（three の既定は XYZ）
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum EulerOrder { #[default] XYZ, YXZ, ZXY, ZYX, YZX, XZY }

/// three の Quaternion.setFromEuler と同じ結果
pub fn quat_from_euler(x: f32, y: f32, z: f32, order: EulerOrder) -> Quat {
    let (qx, qy, qz) = (Quat::from_rotation_x(x), Quat::from_rotation_y(y), Quat::from_rotation_z(z));
    match order {
        EulerOrder::XYZ => qx * qy * qz,
        EulerOrder::YXZ => qy * qx * qz,
        EulerOrder::ZXY => qz * qx * qy,
        EulerOrder::ZYX => qz * qy * qx,
        EulerOrder::YZX => qy * qz * qx,
        EulerOrder::XZY => qx * qz * qy,
    }
}
