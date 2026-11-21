// コースごとの空気（空・霧・光の色）。朝の芝・夕映えの水辺・夜へ向かう丘。src/view/theme.ts の写し。
use island_kit::Color;

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub zenith: Color, pub mid: Color, pub horizon: Color, pub sun: Color,
    /// 太陽の向き（空の中の位置）
    pub sun_dir: [f32; 3],
    pub fog: Color, pub fog_density: f32,
    pub hemi_sky: Color, pub hemi_ground: Color, pub hemi_i: f32,
    pub key: Color, pub key_i: f32,
    pub stars: f32,
    pub cloud: Color,
    pub sea: Color,
    /// 走路・芝の色
    pub dirt: Color, pub dirt_dark: Color, pub grass: Color, pub grass_dark: Color,
    pub rim_color: Color,
    pub warm: f32,
}

fn c(s: &str) -> Color { Color::css(s) }

pub fn theme(id: &str) -> Theme {
    match id {
        // 朝。青い空に桃色の雲。走路は明るい砂
        "turf" => Theme {
            zenith: c("#3f86d8"), mid: c("#8fc3ee"), horizon: c("#ffe6cf"), sun: c("#fff0d8"), sun_dir: [0.62, 0.3, -0.72],
            fog: c("#d8e6ea"), fog_density: 0.0021,
            hemi_sky: c("#dfeeff"), hemi_ground: c("#a39a6e"), hemi_i: 1.25, key: c("#ffe2bd"), key_i: 3.5,
            stars: 0.0, cloud: c("#ffe9dc"), sea: c("#a8cde6"),
            dirt: c("#f0cf9e"), dirt_dark: c("#d4aa78"), grass: c("#7fbb58"), grass_dark: c("#62a24a"), rim_color: c("#ffe6c0"), warm: 0.5,
        },
        // 夕映え。金色の光が横から差し、影が長い
        "water" => Theme {
            zenith: c("#3c4d9a"), mid: c("#c0749b"), horizon: c("#ffb35f"), sun: c("#ffc27a"), sun_dir: [0.72, 0.17, -0.67],
            fog: c("#e6a584"), fog_density: 0.0027,
            hemi_sky: c("#ffd9c4"), hemi_ground: c("#6f6a52"), hemi_i: 1.4, key: c("#ffd3a3"), key_i: 3.0,
            stars: 0.05, cloud: c("#ff9f86"), sea: c("#9a6d9c"),
            dirt: c("#ecc9a2"), dirt_dark: c("#c99b74"), grass: c("#8bb350"), grass_dark: c("#6c9a44"), rim_color: c("#ffc491"), warm: 0.75,
        },
        // 宵。空は藍、地平に茜。照明塔が走路を照らす
        _ => Theme {
            zenith: c("#141b52"), mid: c("#4d3a86"), horizon: c("#ff7f62"), sun: c("#ff9a72"), sun_dir: [-0.7, 0.07, -0.71],
            fog: c("#5f4a80"), fog_density: 0.0032,
            hemi_sky: c("#a9b6ff"), hemi_ground: c("#454660"), hemi_i: 1.3, key: c("#fff0dc"), key_i: 3.0,
            stars: 0.85, cloud: c("#c9607e"), sea: c("#352a5e"),
            dirt: c("#e6c4a8"), dirt_dark: c("#b48d76"), grass: c("#4f8c62"), grass_dark: c("#3d7656"), rim_color: c("#b8c6ff"), warm: 0.1,
        },
    }
}
