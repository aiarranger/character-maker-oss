// kit の共通（character_view の common.wgsl のあとに付く）。Custom のシェーダーもこれを読む。決まりは README。
// group 0: g（Globals）  group 1: 光と影  group 2: binding 0 が材質の値、tex0..tex5 と smp0..smp5、binding 13 が大きい値（k_storage）
struct DirL { dir: vec4f, color: vec4f };
struct PointL { pos: vec4f, color: vec4f };              // pos.w = distance、color.w = decay
struct SpotL { pos: vec4f, dir: vec4f, color: vec4f, cone: vec4f };  // pos.w = distance、dir.w = decay、cone.x = coneCos、cone.y = penumbraCos
struct HemiL { dir: vec4f, sky: vec4f, ground: vec4f };
struct DirShadow { vp: mat4x4f, p: vec4f, q: vec4f };    // p: intensity, bias, normalBias, mapSize。q.x > 0 なら three r182 以降の PCFShadowMap の半径（テクセル。kit.shadow_vogel）
struct Lights {
  ambient: vec4f,
  counts: vec4u,        // 方向・点・スポット・半球
  shadow_count: vec4u,  // x = 影のある方向の光の数（dir の先頭から）
  hidden: vec4f,        // 物陰の影絵の色と不透明度
  dim: vec4f,           // x = uDim
  lamps: array<vec4f, 8>,
  dir: array<DirL, 4>,
  point: array<PointL, 16>,
  spot: array<SpotL, 2>,
  hemi: array<HemiL, 2>,
  shadow: array<DirShadow, 2>,
};
@group(1) @binding(2) var<uniform> lights: Lights;
@group(1) @binding(3) var shadow_tex2: texture_depth_2d;

@group(2) @binding(1) var tex0: texture_2d<f32>;
@group(2) @binding(2) var smp0: sampler;
@group(2) @binding(3) var tex1: texture_2d<f32>;
@group(2) @binding(4) var smp1: sampler;
@group(2) @binding(5) var tex2: texture_2d<f32>;
@group(2) @binding(6) var smp2: sampler;
@group(2) @binding(7) var tex3: texture_2d<f32>;
@group(2) @binding(8) var smp3: sampler;
@group(2) @binding(9) var tex4: texture_2d<f32>;
@group(2) @binding(10) var smp4: sampler;
@group(2) @binding(11) var tex5: texture_2d<f32>;
@group(2) @binding(12) var smp5: sampler;
// 256 バイトより大きい材質の値（kit.set_storage。無い材質は 16 バイトの 0）
@group(2) @binding(13) var<storage, read> k_storage: array<vec4f>;

// ---- 骨付き（scene.skinned）----
// three の Skeleton の boneTexture の役。骨の世界 × boneInverse。頂点は @location(5) joints: vec4u、@location(6) weights: vec4f
@group(3) @binding(0) var<storage, read> k_bones: array<mat4x4f>;
/// skinning_vertex と skinnormal_vertex の skinMatrix。世界の位置は skinMatrix × 位置だけで決まる（AttachedBindMode: bindMatrix は単位行列、modelMatrix × bindMatrixInverse も単位行列）
fn k_skin_matrix(j: vec4u, w: vec4f) -> mat4x4f {
  return k_bones[j.x] * w.x + k_bones[j.y] * w.y + k_bones[j.z] * w.z + k_bones[j.w] * w.w;
}
/// 世界の法線。three は normalMatrix × bindMatrixInverse × skinMatrix × 法線で、世界へ直すと (M Mᵀ)⁻¹ × skinMatrix × 法線（M は節点の世界の行列）。s は k_skin_matrix
fn k_skin_normal(model: mat4x4f, s: mat4x4f, n: vec3f) -> vec3f {
  let c0 = model[0].xyz; let c1 = model[1].xyz; let c2 = model[2].xyz;
  let cof = mat3x3f(cross(c1, c2), cross(c2, c0), cross(c0, c1));
  return normalize(cof * (transpose(cof) * (s * vec4f(n, 0.0)).xyz));
}

fn pow2(x: f32) -> f32 { return x * x; }
fn pow4(x: f32) -> f32 { let y = x * x; return y * y; }
fn saturate_(x: f32) -> f32 { return clamp(x, 0.0, 1.0); }

// three の uTime
fn k_time() -> f32 { return g.misc.x; }
// 的の大きさ（px）
fn k_res() -> vec2f { return vec2f(g.misc.z, g.misc.y); }
// 画素の倍率（three の pixelRatio）
fn k_dpr() -> f32 { return g.misc.w; }
fn k_cam() -> vec3f { return g.cam.xyz; }

// ---- Points（1 点ごとの板。頂点 6 つ）----
fn point_corner(vid: u32) -> vec2f {
  var c = array<vec2f, 6>(vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(-1.0, 1.0), vec2f(-1.0, 1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0));
  return c[vid];
}
/// gl_Position と gl_PointSize（px）から、板の角の clip を出す
fn point_clip(center: vec4f, size_px: f32, vid: u32) -> vec4f {
  // gl_PointSize は 511 画素で止まる（Mac の Chrome の WebGL の実測。ALIASED_POINT_SIZE_RANGE）
  let d = point_corner(vid) * min(size_px, 511.0) / k_res();
  return vec4f(center.xy + d * center.w, center.zw);
}
/// gl_PointCoord（y は上が 0）
fn point_uv(vid: u32) -> vec2f {
  let c = point_corner(vid);
  return vec2f(c.x * 0.5 + 0.5, 0.5 - c.y * 0.5);
}

// 画素の位置（@builtin(position)）。kit.shadow_vogel の影が読む。組み込みの材質は lit の先頭で入れる
var<private> k_frag: vec2f = vec2f(0.0, 0.0);

// ---- 影（PCFSoftShadowMap の式。shadowmap_pars_fragment）----
fn shadow_cmp(k: u32, uv: vec2f, z: f32) -> f32 {
  // three の uv は下が 0。的は上が 0
  let t = vec2f(uv.x, 1.0 - uv.y);
  if (k == 0u) { return textureSampleCompareLevel(shadow_tex, shadow_smp, t, z); }
  return textureSampleCompareLevel(shadow_tex2, shadow_smp, t, z);
}
/// sampler2DShadow の LinearFilter（4 つの比較の結果を混ぜる。shadow_smp は PCFSoft の式のために Nearest）
fn shadow_cmp_linear(k: u32, uv: vec2f, z: f32, size: f32) -> f32 {
  let p = uv * size - 0.5;
  let i = floor(p);
  let f = p - i;
  let at = (i + 0.5) / size;
  let d = 1.0 / size;
  return mix(
    mix(shadow_cmp(k, at, z), shadow_cmp(k, at + vec2f(d, 0.0), z), f.x),
    mix(shadow_cmp(k, at + vec2f(0.0, d), z), shadow_cmp(k, at + vec2f(d, d), z), f.x),
    f.y,
  );
}
/// k 番目の影。world は位置、n は世界の法線（補間のまま。three は頂点で足す）
fn dir_shadow(k: u32, world: vec3f, n: vec3f) -> f32 {
  let s = lights.shadow[k];
  let c0 = s.vp * vec4f(world + n * s.p.z, 1.0);
  let c = c0.xyz / c0.w;
  let uv0 = vec2f(c.x * 0.5 + 0.5, c.y * 0.5 + 0.5);
  let z = c.z + s.p.y;
  if (uv0.x < 0.0 || uv0.x > 1.0 || uv0.y < 0.0 || uv0.y > 1.0 || z > 1.0) { return 1.0; }
  let size = s.p.w;
  let t = 1.0 / size;
  if (s.q.x > 0.0) {
    // shadowmap_pars_fragment（r186）: 5 点の Vogel の円板を、画素ごとの IGN で回す。gl_FragCoord の y は下が 0
    let radius = s.q.x * t;
    let fc = vec2f(k_frag.x, g.misc.y - k_frag.y);
    let phi = fract(52.9829189 * fract(dot(fc, vec2f(0.06711056, 0.00583715)))) * 6.283185307179586;
    var sum = 0.0;
    for (var n = 0u; n < 5u; n++) {
      let r = sqrt((f32(n) + 0.5) / 5.0);
      let th = f32(n) * 2.399963229728653 + phi;
      sum += shadow_cmp_linear(k, uv0 + vec2f(cos(th), sin(th)) * r * radius, z, size);
    }
    return mix(1.0, sum * 0.2, s.p.x);
  }
  let f = fract(uv0 * size + 0.5);
  let uv = uv0 - f * t;
  let sh = (
    shadow_cmp(k, uv, z) + shadow_cmp(k, uv + vec2f(t, 0.0), z) + shadow_cmp(k, uv + vec2f(0.0, t), z) + shadow_cmp(k, uv + vec2f(t, t), z)
    + mix(shadow_cmp(k, uv + vec2f(-t, 0.0), z), shadow_cmp(k, uv + vec2f(2.0 * t, 0.0), z), f.x)
    + mix(shadow_cmp(k, uv + vec2f(-t, t), z), shadow_cmp(k, uv + vec2f(2.0 * t, t), z), f.x)
    + mix(shadow_cmp(k, uv + vec2f(0.0, -t), z), shadow_cmp(k, uv + vec2f(0.0, 2.0 * t), z), f.y)
    + mix(shadow_cmp(k, uv + vec2f(t, -t), z), shadow_cmp(k, uv + vec2f(t, 2.0 * t), z), f.y)
    + mix(mix(shadow_cmp(k, uv + vec2f(-t, -t), z), shadow_cmp(k, uv + vec2f(2.0 * t, -t), z), f.x),
          mix(shadow_cmp(k, uv + vec2f(-t, 2.0 * t), z), shadow_cmp(k, uv + vec2f(2.0 * t, 2.0 * t), z), f.x), f.y)
  ) * (1.0 / 9.0);
  return mix(1.0, sh, s.p.x);
}

fn distance_attenuation(d: f32, cutoff: f32, decay: f32) -> f32 {
  var v = 1.0 / max(pow(d, decay), 0.01);
  if (cutoff > 0.0) { v *= pow2(saturate_(1.0 - pow4(d / cutoff))); }
  return v;
}
