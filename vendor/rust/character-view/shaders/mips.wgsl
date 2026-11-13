// island-kit の shaders/mips.wgsl の写し（character-view は island-kit を読めないので同じ物を置く。直すときは両方）。表は character-view の lib.rs の GpuMips が作る
// ミップを GPU で作る（texture.rs の GpuMips。kit.set_gpu_mips）。1 段ごとに 1 回描く。source は 1 つ上の段だけの view。
// 4 点の取り方は texture.rs::downsample と同じ（x0 = x * w / nw、x1 = min(x0 + 1, w - 1)）。
//  fs_unorm: 線形の 8 ビット。整数で平均して丸める（downsample の srgb = false と同じ値）
//  fs_srgb:  sRGB の 8 ビット（Rgba8Unorm の view で生の値を読み書きする）。線形にする表 lut と、8 ビットへ戻す境目の表 edge は
//            CPU の式（srgb_to_linear・linear_to_srgb）から作って渡すので、GPU の pow を使わずに downsample の srgb = true と同じ値になる
//  fs_float: 16 ビットの浮動小数（透過用の的）。4 点の平均
@group(0) @binding(0) var source: texture_2d<f32>;
struct MipU { lut: array<vec4f, 64>, edge: array<vec4f, 64> };
@group(0) @binding(1) var<uniform> u: MipU;

@vertex fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}

struct Taps { a: vec2i, b: vec2i, c: vec2i, d: vec2i };
fn taps(p: vec4f) -> Taps {
    let size = textureDimensions(source, 0);
    let next = max(size / 2u, vec2u(1u));
    let lo = vec2u(p.xy) * size / next;
    let hi = min(lo + vec2u(1u), size - vec2u(1u));
    return Taps(vec2i(lo), vec2i(vec2u(hi.x, lo.y)), vec2i(vec2u(lo.x, hi.y)), vec2i(hi));
}
fn rgba8(p: vec2i) -> vec4u { return vec4u(textureLoad(source, p, 0) * 255.0 + 0.5); }

@fragment fn fs_unorm(@builtin(position) p: vec4f) -> @location(0) vec4f {
    let t = taps(p);
    let sum = rgba8(t.a) + rgba8(t.b) + rgba8(t.c) + rgba8(t.d);
    return vec4f((sum + vec4u(2u)) / vec4u(4u)) / 255.0;
}

fn lin(v: u32) -> f32 { return u.lut[v / 4u][v % 4u]; }
fn edge(k: u32) -> f32 { return u.edge[k / 4u][k % 4u]; }
/// linear_to_srgb と同じ 8 ビット（edge[k] は linear_to_srgb が k 以上になる最小の値。単調なので二分探しで k を出す）
fn encode(c: f32) -> f32 {
    var k = 0u;
    for (var step = 128u; step > 0u; step = step / 2u) {
        if (k + step <= 255u && edge(k + step) <= c) { k = k + step; }
    }
    return f32(k) / 255.0;
}

@fragment fn fs_srgb(@builtin(position) p: vec4f) -> @location(0) vec4f {
    let t = taps(p);
    let a = rgba8(t.a); let b = rgba8(t.b); let c = rgba8(t.c); let d = rgba8(t.d);
    var o: vec4f;
    for (var ch = 0u; ch < 3u; ch = ch + 1u) {
        // downsample と同じ順に足す（a + b + c + d）× 0.25
        o[ch] = encode((((lin(a[ch]) + lin(b[ch])) + lin(c[ch])) + lin(d[ch])) * 0.25);
    }
    o.w = f32((a.w + b.w + c.w + d.w + 2u) / 4u) / 255.0;
    return o;
}

@fragment fn fs_float(@builtin(position) p: vec4f) -> @location(0) vec4f {
    let t = taps(p);
    return (textureLoad(source, t.a, 0) + textureLoad(source, t.b, 0) + textureLoad(source, t.c, 0) + textureLoad(source, t.d, 0)) * 0.25;
}
