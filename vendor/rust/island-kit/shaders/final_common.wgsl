// 仕上げの前置き。ゲームはこのあとに `struct ..; @group(1) @binding(0) var<uniform> u: ..;` と `@fragment fn fs_final(i: VOut) -> @location(0) vec4f` を書く。
struct VOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f };

@vertex fn vs_full(@builtin(vertex_index) i: u32) -> VOut {
  let p = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  var o: VOut;
  o.pos = vec4f(p[i], 0.0, 1.0);
  o.uv = vec2f((p[i].x + 1.0) * 0.5, (p[i].y + 1.0) * 0.5);
  return o;
}
fn tex_uv(uv: vec2f) -> vec2f { return vec2f(uv.x, 1.0 - uv.y); }

@group(0) @binding(0) var t0: texture_2d<f32>;
@group(0) @binding(1) var s0: sampler;
@group(2) @binding(0) var mask: texture_2d<f32>;

// three の tDiffuse（uv は下が 0）
fn smp(uv: vec2f) -> vec4f { return textureSampleLevel(t0, s0, tex_uv(uv), 0.0); }
// 自機の型（outline のときだけ描く）
fn mask_at(uv: vec2f) -> f32 { return textureSampleLevel(mask, s0, tex_uv(uv), 0.0).r; }
fn hash2(p: vec2f) -> f32 { return fract(sin(dot(p, vec2f(12.9898, 78.233))) * 43758.5453); }
// three の colorspace_fragment（sRGBTransferOETF）
fn to_srgb(c: vec3f) -> vec3f {
  let lo = c * 12.92;
  let hi = 1.055 * pow(c, vec3f(0.41666)) - 0.055;
  return select(hi, lo, c <= vec3f(0.0031308));
}
