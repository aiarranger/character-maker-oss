// 飴鏡: 透ける飴の板と、柱・つまみ・桟の金（MeshStandardMaterial 相当）。式は src/view/mirrors.ts の GLSL をそのまま写す。
struct FxIn {
  @location(8) m0: vec4f,
  @location(9) m1: vec4f,
  @location(10) m2: vec4f,
  @location(11) m3: vec4f,
  @location(12) a: vec4f,
  @location(13) b: vec4f,
};
fn fx_model(i: FxIn) -> mat4x4f { return mat4x4f(i.m0, i.m1, i.m2, i.m3); }

struct GlassOut {
  @builtin(position) pos: vec4f,
  @location(0) uv: vec2f,
  @location(1) @interpolate(flat) a: vec4f, // uTime, uFlash, uHit
};
@vertex fn vs_glass(v: VIn, i: FxIn) -> GlassOut {
  var o: GlassOut;
  o.pos = g.view_proj * fx_model(i) * vec4f(v.pos, 1.0);
  o.uv = v.uv;
  o.a = i.a;
  return o;
}
@fragment fn fs_glass(i: GlassOut) -> @location(0) vec4f {
  let time = i.a.x; let flash = i.a.y; let hit_at = i.a.zw;
  // 飴の色: 下は濃い琥珀、上は桃色に透ける
  let col = mix(vec3f(1.0, 0.55, 0.25), vec3f(1.0, 0.65, 0.85), i.uv.y);
  // 斜めに流れる光の帯
  let band = 1.0 - smoothstep(0.0, 0.08, abs(fract(i.uv.x * 1.4 - i.uv.y * 0.6 - time * 0.12) - 0.5));
  // 縁は濃く
  let edge = 1.0 - smoothstep(0.0, 0.06, min(min(i.uv.x, 1.0 - i.uv.x), min(i.uv.y, 1.0 - i.uv.y)));
  // 当たった所から広がる光
  let d = distance(i.uv * vec2f(4.0, 1.0), hit_at * vec2f(4.0, 1.0));
  let hit = flash * (1.0 - smoothstep(0.0, 0.9, d));
  let c = col * (0.55 + 0.6 * band + 0.6 * edge) + vec3f(1.0, 0.95, 0.85) * hit * 1.6;
  return vec4f(c, 0.38 + 0.25 * edge + 0.3 * band + 0.5 * hit);
}

struct GoldOut {
  @builtin(position) pos: vec4f,
  @location(0) world: vec3f,
  @location(1) n: vec3f,
  @location(2) @interpolate(flat) albedo: vec4f,   // rgb = 色, w = roughness
  @location(3) @interpolate(flat) emissive: vec4f, // rgb = 発光 × 強さ, w = metalness
};
@vertex fn vs_gold(v: VIn, i: FxIn) -> GoldOut {
  let m = fx_model(i);
  let w = m * vec4f(v.pos, 1.0);
  var o: GoldOut;
  o.pos = g.view_proj * w;
  o.world = w.xyz;
  o.n = normal_world(m, v.normal);
  o.albedo = i.a;
  o.emissive = i.b;
  return o;
}
// three の BRDF_GGX（D_GGX・V_GGX_SmithCorrelated・F_Schlick）
fn brdf_ggx(l: vec3f, v: vec3f, n: vec3f, f0: vec3f, alpha: f32) -> vec3f {
  let h = normalize(l + v);
  let nl = clamp(dot(n, l), 0.0, 1.0);
  let nv = clamp(dot(n, v), 0.0, 1.0);
  let nh = clamp(dot(n, h), 0.0, 1.0);
  let vh = clamp(dot(v, h), 0.0, 1.0);
  let fresnel = pow(1.0 - vh, 5.0);
  let f = f0 * (1.0 - fresnel) + vec3f(fresnel);
  let a2 = alpha * alpha;
  let gv = nl * sqrt(a2 + (1.0 - a2) * nv * nv);
  let gl = nv * sqrt(a2 + (1.0 - a2) * nl * nl);
  let vis = 0.5 / max(gv + gl, 1e-6);
  let den = nh * nh * (a2 - 1.0) + 1.0;
  let dd = RECIPROCAL_PI * a2 / (den * den);
  return f * (vis * dd);
}
@fragment fn fs_gold(i: GoldOut) -> @location(0) vec4f {
  let n = normalize(i.n);
  let v = normalize(g.cam.xyz - i.world);
  let l = g.key_dir.xyz;
  let metal = i.emissive.w;
  let rough = i.albedo.w;
  let diffuse = i.albedo.rgb * (1.0 - metal);
  let f0 = mix(vec3f(0.04), i.albedo.rgb, metal);
  let nl = clamp(dot(n, l), 0.0, 1.0);
  var col = hemi_irradiance(n) * diffuse * RECIPROCAL_PI;
  col += g.key_color.rgb * nl * (diffuse * RECIPROCAL_PI + brdf_ggx(l, v, n, f0, rough * rough));
  col += i.emissive.rgb;
  return vec4f(apply_fog(col, view_depth(i.world)), 1.0);
}

// 影の型（柱だけが影を落とす）
@vertex fn vs_shadow(v: VIn) -> @builtin(position) vec4f {
  return g.light_vp * vec4f(v.pos, 1.0);
}
