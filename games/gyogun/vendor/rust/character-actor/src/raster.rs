// キャンバスの代わりの小さなソフトウェア描画（materials.ts の丸・光のにじみ、world.ts の地面の絵）。色は sRGB のまま混ぜる（canvas 2D と同じ）。
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

/// 色の止まり（位置, r, g, b, a）。a は 0..1
pub type Stop = (f32, f32, f32, f32, f32);

impl Canvas {
    pub fn new(w: usize, h: usize) -> Self { Canvas { w, h, px: vec![0; w * h * 4] } }

    #[inline]
    pub fn blend(&mut self, x: usize, y: usize, r: f32, g: f32, b: f32, a: f32) {
        let i = (y * self.w + x) * 4;
        let (sa, da) = (a, self.px[i + 3] as f32 / 255.0);
        let oa = sa + da * (1.0 - sa);
        if oa <= 0.0 { return; }
        let mix = |s: f32, d: u8| (s * sa + d as f32 * da * (1.0 - sa)) / oa + 0.5;
        self.px[i] = mix(r, self.px[i]) as u8;
        self.px[i + 1] = mix(g, self.px[i + 1]) as u8;
        self.px[i + 2] = mix(b, self.px[i + 2]) as u8;
        self.px[i + 3] = (oa * 255.0) as u8;
    }

    pub fn fill(&mut self, r: f32, g: f32, b: f32) {
        for p in self.px.chunks_exact_mut(4) { p[0] = r as u8; p[1] = g as u8; p[2] = b as u8; p[3] = 255; }
    }

    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, c: (f32, f32, f32, f32)) {
        // 端は覆う割合で薄く塗る（canvas の fillRect と同じ）
        let (x0, y0, x1, y1) = (x.max(0.0) as usize, y.max(0.0) as usize, ((x + w).ceil() as usize).min(self.w), ((y + h).ceil() as usize).min(self.h));
        for yy in y0..y1 {
            let cy = ((yy as f32 + 1.0).min(y + h) - (yy as f32).max(y)).clamp(0.0, 1.0);
            for xx in x0..x1 {
                let cx = ((xx as f32 + 1.0).min(x + w) - (xx as f32).max(x)).clamp(0.0, 1.0);
                self.blend(xx, yy, c.0, c.1, c.2, c.3 * cx * cy);
            }
        }
    }

    /// 円形のグラデーションを丸く塗る（canvas の createRadialGradient(x,y,0,x,y,r)）
    pub fn radial(&mut self, cx: f32, cy: f32, r: f32, stops: &[Stop]) {
        let (x0, y0) = ((cx - r).floor().max(0.0) as usize, (cy - r).floor().max(0.0) as usize);
        let (x1, y1) = (((cx + r).ceil() as isize).clamp(0, self.w as isize) as usize, ((cy + r).ceil() as isize).clamp(0, self.h as isize) as usize);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt() / r;
                if d >= 1.0 { continue; }
                let (cr, cg, cb, ca) = sample_stops(stops, d);
                if ca > 0.0 { self.blend(x, y, cr, cg, cb, ca); }
            }
        }
    }

    /// 太い線（端は丸）。1 本の折れ線を塗る（重なりも 1 回だけ）
    pub fn polyline(&mut self, pts: &[(f32, f32)], width: f32, c: (f32, f32, f32, f32)) {
        let hw = width / 2.0;
        let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in pts { minx = minx.min(p.0); miny = miny.min(p.1); maxx = maxx.max(p.0); maxy = maxy.max(p.1); }
        let (x0, y0) = ((minx - hw - 1.0).max(0.0) as usize, (miny - hw - 1.0).max(0.0) as usize);
        let (x1, y1) = (((maxx + hw + 1.0).max(0.0) as usize).min(self.w), ((maxy + hw + 1.0).max(0.0) as usize).min(self.h));
        for y in y0..y1 {
            for x in x0..x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let mut best = f32::MAX;
                for s in pts.windows(2) {
                    let (ax, ay, bx, by) = (s[0].0, s[0].1, s[1].0, s[1].1);
                    let (dx, dy) = (bx - ax, by - ay);
                    let t = (((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy).max(1e-6)).clamp(0.0, 1.0);
                    best = best.min((px - ax - dx * t).powi(2) + (py - ay - dy * t).powi(2));
                }
                let d = best.sqrt();
                let cov = (hw + 0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 { self.blend(x, y, c.0, c.1, c.2, c.3 * cov); }
            }
        }
    }
}

fn sample_stops(stops: &[Stop], d: f32) -> (f32, f32, f32, f32) {
    let mut prev = stops[0];
    for s in stops {
        if d <= s.0 {
            let k = if s.0 > prev.0 { (d - prev.0) / (s.0 - prev.0) } else { 0.0 };
            return (prev.1 + (s.1 - prev.1) * k, prev.2 + (s.2 - prev.2) * k, prev.3 + (s.3 - prev.3) * k, prev.4 + (s.4 - prev.4) * k);
        }
        prev = *s;
    }
    (prev.1, prev.2, prev.3, prev.4)
}

/// 雲・泡の影などに使う、柔らかい丸の濃淡（白・アルファだけ）
pub fn blob_rgba() -> (u32, u32, Vec<u8>) {
    let n = 64usize;
    let mut d = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let r = ((x as f32 + 0.5 - n as f32 / 2.0).powi(2) + (y as f32 + 0.5 - n as f32 / 2.0).powi(2)).sqrt() / (n as f32 / 2.0);
            let k = ((r - 0.3) / 0.7).clamp(0.0, 1.0);
            let i = (y * n + x) * 4;
            d[i] = 255; d[i + 1] = 255; d[i + 2] = 255;
            d[i + 3] = ((1.0 - k * k * (3.0 - 2.0 * k)) * 255.0).round() as u8;
        }
    }
    (n as u32, n as u32, d)
}

/// 光のにじみ（加算）の丸
pub fn glow_rgba() -> (u32, u32, Vec<u8>) {
    let mut c = Canvas::new(128, 128);
    c.radial(64.0, 64.0, 64.0, &[(0.0, 255.0, 255.0, 255.0, 1.0), (0.18, 255.0, 255.0, 255.0, 0.75), (0.45, 255.0, 255.0, 255.0, 0.22), (1.0, 255.0, 255.0, 255.0, 0.0)]);
    (128, 128, c.px)
}
