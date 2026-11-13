// texture.rs::downsample(srgb=false) と同じ4点・整数丸め。
// 各 view は1段だけなので、source の level 0 は直前の段を指す。
@group(0) @binding(0) var source: texture_2d<f32>;

@vertex fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    let p = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4f(p * 2.0 - 1.0, 0.0, 1.0);
}

fn rgba8(p: vec2u) -> vec4u {
    return vec4u(textureLoad(source, vec2i(p), 0) * 255.0 + 0.5);
}

@fragment fn fs_mip(@builtin(position) p: vec4f) -> @location(0) vec4f {
    let size = textureDimensions(source, 0);
    let next = max(size / 2u, vec2u(1u));
    let lo = vec2u(p.xy) * size / next;
    let hi = min(lo + vec2u(1u), size - vec2u(1u));
    let sum = rgba8(lo) + rgba8(vec2u(hi.x, lo.y)) + rgba8(vec2u(lo.x, hi.y)) + rgba8(hi);
    return vec4f((sum + vec4u(2u)) / vec4u(4u)) / 255.0;
}
