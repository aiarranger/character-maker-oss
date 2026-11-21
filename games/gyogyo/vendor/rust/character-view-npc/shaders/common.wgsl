// どのシェーダーからも読む世界の値（three の camera・lights・fog・SHARED の役）。光の計算はワールド空間で行う。
struct Globals {
  view: mat4x4f,
  proj: mat4x4f,
  view_proj: mat4x4f,
  light_vp: mat4x4f,
  cam: vec4f,
  hemi_sky: vec4f,    // rgb = 色 × 強さ
  hemi_ground: vec4f,
  key_color: vec4f,
  key_dir: vec4f,     // 光のある向き（正規化済み）
  fog: vec4f,         // rgb = 霧の色、w = 濃さ
  rim: vec4f,         // rgb = 輪郭の光の色、w = 強さ（SHARED.uRimColor / uRimI）
  misc: vec4f,        // x = uTime、y = 画面の高さ（px）、z = 画面の幅（px）
};
@group(0) @binding(0) var<uniform> g: Globals;

const RECIPROCAL_PI: f32 = 0.3183098861837907;

// 霧の濃さ。g.fog.w が負のときは three の Fog（線形。hemi_sky.w = near、hemi_ground.w = far）、それ以外は FogExp2
fn fog_factor(depth: f32) -> f32 {
  if (g.fog.w < 0.0) { return smoothstep(g.hemi_sky.w, g.hemi_ground.w, depth); }
  return 1.0 - exp(-g.fog.w * g.fog.w * depth * depth);
}
fn apply_fog(col: vec3f, depth: f32) -> vec3f {
  return mix(col, g.fog.rgb, fog_factor(depth));
}

// three の colorspace_fragment（sRGBTransferOETF）
fn linear_to_srgb(c: vec3f) -> vec3f {
  let lo = c * 12.92;
  let hi = 1.055 * pow(max(c, vec3f(0.0)), vec3f(0.41666)) - 0.055;
  return select(hi, lo, c <= vec3f(0.0031308));
}
// three の ACESFilmicToneMapping（露出 1）
fn aces_filmic(c0: vec3f) -> vec3f {
  let input_m = mat3x3f(vec3f(0.59719, 0.07600, 0.02840), vec3f(0.35458, 0.90834, 0.13383), vec3f(0.04823, 0.01566, 0.83777));
  let output_m = mat3x3f(vec3f(1.60475, -0.10208, -0.00327), vec3f(-0.53108, 1.10813, -0.07276), vec3f(-0.07367, -0.00605, 1.07602));
  var c = input_m * (c0 / 0.6);
  let a = c * (c + vec3f(0.0245786)) - vec3f(0.000090537);
  let b = c * (0.983729 * c + vec3f(0.4329510)) + vec3f(0.238081);
  c = output_m * (a / b);
  return clamp(c, vec3f(0.0), vec3f(1.0));
}
// 色の出口。g.cam.w: 1 = 線形のまま的へ（既定。後処理の最後に sRGB）。2・3 = three が composer なしで画面へ直接描くときと同じ（EffectComposer を使わない Canvas）:
// 2 は flat（色調なし）、3 は ACES。色調 → sRGB → 霧（色も sRGB）→ premultiplied の順で、ブレンドと MSAA は sRGB の値のまま行われる
fn shade_out(c0: vec3f, a: f32, depth: f32, fog: bool, tone: bool, premult: bool) -> vec4f {
  var c = c0;
  if (g.cam.w > 1.5) {
    if (tone && g.cam.w > 2.5) { c = aces_filmic(c); }
    c = linear_to_srgb(c);
    if (fog) { c = apply_fog(c, depth); }
    if (premult) { c *= a; }
    return vec4f(clamp(c, vec3f(0.0), vec3f(1.0)), a);
  }
  if (fog) { c = apply_fog(c, depth); }
  if (premult) { c *= a; }
  return vec4f(c, a);
}

fn view_depth(world: vec3f) -> f32 {
  return -(g.view * vec4f(world, 1.0)).z;
}

fn hemi_irradiance(n: vec3f) -> vec3f {
  let w = 0.5 * n.y + 0.5;
  return mix(g.hemi_ground.rgb, g.hemi_sky.rgb, w);
}

fn luma(c: vec3f) -> f32 { return dot(c, vec3f(0.2126, 0.7152, 0.0722)); }

struct VIn {
  @location(0) pos: vec3f,
  @location(1) normal: vec3f,
  @location(2) uv: vec2f,
  @location(3) color: vec4f,
  @location(4) sway: f32,
};
struct IIn {
  @location(8) m0: vec4f,
  @location(9) m1: vec4f,
  @location(10) m2: vec4f,
  @location(11) m3: vec4f,
  @location(12) fx: vec4f,
};
fn inst_model(i: IIn) -> mat4x4f { return mat4x4f(i.m0, i.m1, i.m2, i.m3); }
fn normal_world(m: mat4x4f, n: vec3f) -> vec3f {
  // 大きさがそろわない行列でも面の向きを保つ（three の normalMatrix と同じ）
  let c0 = m[0].xyz; let c1 = m[1].xyz; let c2 = m[2].xyz;
  let inv = mat3x3f(cross(c1, c2), cross(c2, c0), cross(c0, c1));
  let s = sign(dot(c0, cross(c1, c2)));
  return normalize(inv * n * s);
}

// 影
@group(1) @binding(0) var shadow_tex: texture_depth_2d;
@group(1) @binding(1) var shadow_smp: sampler_comparison;
fn shadow_at(world: vec3f, n: vec3f) -> f32 {
  // normalBias 0.04 / bias -0.0006（three の値）
  let p = g.light_vp * vec4f(world + n * 0.04, 1.0);
  let c = p.xyz / p.w;
  let uv = vec2f(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
  if (uv.x < 0.0 || uv.x > 1.0 || uv.y < 0.0 || uv.y > 1.0 || c.z > 1.0) { return 1.0; }
  let d = c.z - 0.0006;
  // PCFSoft に近いやわらかさ（3×3 の比較をハードウェアの線形で）
  let t = 1.0 / 2048.0;
  var s = 0.0;
  for (var y = -1; y <= 1; y++) {
    for (var x = -1; x <= 1; x++) {
      s += textureSampleCompareLevel(shadow_tex, shadow_smp, uv + vec2f(f32(x), f32(y)) * t, d);
    }
  }
  return s / 9.0;
}
