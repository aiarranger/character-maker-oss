// 組み込みの材質（three の ShaderLib と ShaderChunk の式を写す）。計算はワールド空間（three はビュー空間。内積は同じ）。
struct MatU {
  color: vec4f,      // rgb, opacity
  emissive: vec4f,   // rgb（強さ込み）, alphaTest
  specular: vec4f,   // rgb, shininess（Physical は sheen × sheenColor, 環境の強さ）
  p: vec4f,          // metalness, roughness, rim の倍率, Sprite の rotation か Points の size（Physical は metalness, roughness, clearcoat, clearcoatRoughness）
  uv0: vec4f,        // uvTransform の 1 行目（Physical の w = sheenRoughness）
  uv1: vec4f,        // 2 行目
  flags: vec4u,      // x = フラグ（material.rs の F_*）
};
@group(2) @binding(0) var<uniform> m: MatU;

const F_VERTEX_COLORS: u32 = 1u;
const F_OPAQUE: u32 = 2u;
const F_FOG: u32 = 4u;
const F_PREMULT: u32 = 8u;
const F_FLAT: u32 = 16u;
const F_DOUBLE: u32 = 32u;
const F_BACK: u32 = 64u;
const F_SWAY: u32 = 128u;
const F_ATTENUATE: u32 = 256u;
const F_ENV: u32 = 512u;
const F_EMAP: u32 = 1024u;
const F_NOTONE: u32 = 2048u;
const F_VIEW_RIM: u32 = 4096u;
const F_BUMP: u32 = 8192u;

fn has(f: u32) -> bool { return (m.flags.x & f) != 0u; }
fn map_uv(uv: vec2f) -> vec2f { let v = vec3f(uv, 1.0); return vec2f(dot(m.uv0.xyz, v), dot(m.uv1.xyz, v)); }

struct MOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
  @location(2) uv: vec2f,
  @location(3) color: vec4f,
  @location(4) depth: f32,   // -mvPosition.z
  @location(5) recv: f32,    // receiveShadow
};

// ---- 形 ----
@vertex fn vs_mesh(v: VIn, i: IIn) -> MOut {
  let model = inst_model(i);
  var p = v.pos;
  // 草木の揺れ（materials.ts）
  if (has(F_SWAY)) {
    let wo = model[3].xyz;
    let s = v.sway * 0.06;
    let t = g.misc.x;
    p.x += sin(t * 1.6 + wo.x * 0.7 + v.pos.y * 0.8) * s;
    p.z += cos(t * 1.3 + wo.z * 0.6 + v.pos.y * 0.7) * s * 0.7;
  }
  let w = model * vec4f(p, 1.0);
  let mv = g.view * w;
  var o: MOut;
  o.clip = g.proj * mv;
  o.world = w.xyz;
  var n = normal_world(model, v.normal);
  if (has(F_BACK)) { n = -n; }
  o.normal = n;
  o.uv = map_uv(v.uv);
  o.color = vec4f(i.fx.rgb, 1.0);
  if (has(F_VERTEX_COLORS)) { o.color *= v.color; }
  o.depth = -mv.z;
  o.recv = i.fx.w;
  return o;
}

// 骨付き（three の skinning_vertex・skinnormal_vertex）
@vertex fn vs_mesh_skinned(v: VIn, i: IIn, @location(5) joints: vec4u, @location(6) weights: vec4f) -> MOut {
  let s = k_skin_matrix(joints, weights);
  let w = s * vec4f(v.pos, 1.0);
  let mv = g.view * w;
  var o: MOut;
  o.clip = g.proj * mv;
  o.world = w.xyz;
  var n = k_skin_normal(inst_model(i), s, v.normal);
  if (has(F_BACK)) { n = -n; }
  o.normal = n;
  o.uv = map_uv(v.uv);
  o.color = vec4f(i.fx.rgb, 1.0);
  if (has(F_VERTEX_COLORS)) { o.color *= v.color; }
  o.depth = -mv.z;
  o.recv = i.fx.w;
  return o;
}

// three の sprite_vert
@vertex fn vs_sprite(v: VIn, i: IIn) -> MOut {
  let model = inst_model(i);
  var mv = g.view * model[3];
  var scale = vec2f(length(model[0].xyz), length(model[1].xyz));
  if (!has(F_ATTENUATE)) { scale *= -mv.z; }
  let center = i.fx.xy;
  let a = (v.pos.xy - (center - vec2f(0.5))) * scale;
  let c = cos(m.p.w);
  let s = sin(m.p.w);
  mv = vec4f(mv.xy + vec2f(c * a.x - s * a.y, s * a.x + c * a.y), mv.zw);
  var o: MOut;
  o.clip = g.proj * mv;
  o.world = model[3].xyz;
  o.normal = vec3f(0.0, 0.0, 1.0);
  o.uv = map_uv(v.uv);
  o.color = vec4f(1.0);
  o.depth = -mv.z;
  o.recv = 0.0;
  return o;
}

// three の points_vert（1 点ごとの板）
@vertex fn vs_points(@location(0) pos: vec3f, i: IIn, @builtin(vertex_index) vid: u32) -> MOut {
  let model = inst_model(i);
  let w = model * vec4f(pos, 1.0);
  let mv = g.view * w;
  let dpr = k_dpr();
  var size = m.p.w * dpr;
  if (has(F_ATTENUATE)) { size *= (g.misc.y / dpr * 0.5) / -mv.z; }
  var o: MOut;
  o.clip = point_clip(g.proj * mv, size, vid);
  o.world = w.xyz;
  o.normal = vec3f(0.0, 0.0, 1.0);
  let pc = point_uv(vid);
  o.uv = map_uv(vec2f(pc.x, 1.0 - pc.y));
  o.color = vec4f(1.0);
  o.depth = -mv.z;
  o.recv = 0.0;
  return o;
}

@vertex fn vs_line(@location(0) pos: vec3f, i: IIn) -> MOut {
  let w = inst_model(i) * vec4f(pos, 1.0);
  let mv = g.view * w;
  var o: MOut;
  o.clip = g.proj * mv;
  o.world = w.xyz;
  o.normal = vec3f(0.0, 0.0, 1.0);
  o.uv = vec2f(0.0);
  o.color = vec4f(1.0);
  o.depth = -mv.z;
  o.recv = 0.0;
  return o;
}

// ---- 色の組み立て（opaque_fragment → fog_fragment → premultiplied_alpha_fragment）----
fn finish(rgb: vec3f, a0: f32, depth: f32) -> vec4f {
  var a = a0;
  if (has(F_OPAQUE)) { a = 1.0; }
  return shade_out(rgb, a, depth, has(F_FOG), !has(F_NOTONE), has(F_PREMULT));
}

fn diffuse_color(i: MOut) -> vec4f {
  return m.color * textureSample(tex0, smp0, i.uv) * i.color;
}

// three の MeshBasicMaterial・LineBasicMaterial
@fragment fn fs_basic(i: MOut, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  let d = diffuse_color(i);
  if (d.a < m.emissive.w) { discard; }
  var out = d.rgb;
  if (has(F_VIEW_RIM)) {
    // Engimeow の RimLit の basic の分岐（FinaleStage.tsx の addRim）
    let n0 = normalize((g.view * vec4f(normalize(i.normal), 0.0)).xyz);
    let rn = select(-n0, n0, front);
    let rv = normalize((g.view * vec4f(g.cam.xyz - i.world, 0.0)).xyz);
    let fr = pow(1.0 - clamp(dot(rn, rv), 0.0, 1.0), 3.0);
    let side = clamp(dot(rn, normalize(vec3f(-0.5, 0.6, -0.62))) * 0.9 + 0.35, 0.0, 1.0);
    let key = clamp(dot(rn, normalize(vec3f(0.3, 0.55, 0.78))) * 0.5 + 0.5, 0.0, 1.0);
    out *= mix(vec3f(0.6, 0.54, 0.7), vec3f(0.98, 0.9, 0.82), smoothstep(0.15, 0.85, key));
    out += m.specular.rgb * fr * side;
  }
  return finish(out, d.a, i.depth);
}

// three の SpriteMaterial（premultiplied は効かない）
@fragment fn fs_sprite(i: MOut) -> @location(0) vec4f {
  let d = diffuse_color(i);
  if (d.a < m.emissive.w) { discard; }
  var a = d.a;
  if (has(F_OPAQUE)) { a = 1.0; }
  return shade_out(d.rgb, a, i.depth, has(F_FOG), !has(F_NOTONE), false);
}

// ---- 光 ----
const MODE_LAMBERT: u32 = 0u;
const MODE_SOFT: u32 = 1u;
const MODE_PHONG: u32 = 2u;
const MODE_STANDARD: u32 = 3u;
const MODE_PHYSICAL: u32 = 4u;

struct Surf {
  n: vec3f,
  v: vec3f,
  diffuse: vec3f,
  // Phong: specular の色。Standard: f0
  spec: vec3f,
  shininess: f32,
  roughness: f32,
  // Physical だけ（clearcoat の法線は n と同じ）
  cc_rough: f32,
  sheen: vec3f,
  sheen_rough: f32,
};
struct Refl { dd: vec3f, ds: vec3f, cc: vec3f, sh: vec3f };

fn f_schlick(f0: vec3f, f90: f32, dot_vh: f32) -> vec3f {
  let fres = pow(1.0 - dot_vh, 5.0);
  return f0 * (1.0 - fres) + vec3f(f90 * fres);
}
fn v_ggx(alpha: f32, dot_nl: f32, dot_nv: f32) -> f32 {
  let a2 = pow2(alpha);
  let gv = dot_nl * sqrt(a2 + (1.0 - a2) * pow2(dot_nv));
  let gl = dot_nv * sqrt(a2 + (1.0 - a2) * pow2(dot_nl));
  return 0.5 / max(gv + gl, 1e-6);
}
fn d_ggx(alpha: f32, dot_nh: f32) -> f32 {
  let a2 = pow2(alpha);
  let denom = pow2(dot_nh) * (a2 - 1.0) + 1.0;
  return RECIPROCAL_PI * a2 / pow2(denom);
}

// ---- Physical（three の meshphysical。使う項目だけ）----
const K_PI: f32 = 3.141592653589793;
fn d_charlie(roughness: f32, dot_nh: f32) -> f32 {
  let alpha = pow2(roughness);
  let inv_alpha = 1.0 / alpha;
  let cos2h = dot_nh * dot_nh;
  let sin2h = max(1.0 - cos2h, 0.0078125);
  return (2.0 + inv_alpha) * pow(sin2h, inv_alpha * 0.5) / (2.0 * K_PI);
}
fn v_neubelt(dot_nv: f32, dot_nl: f32) -> f32 { return saturate_(1.0 / (4.0 * (dot_nl + dot_nv - dot_nl * dot_nv))); }
fn ibl_sheen_brdf(n: vec3f, v: vec3f, roughness: f32) -> f32 {
  let dot_nv = saturate_(dot(n, v));
  let r2 = roughness * roughness;
  let lo = roughness < 0.25;
  let a = select(-8.48 * r2 + 14.3 * roughness - 9.95, -339.2 * r2 + 161.4 * roughness - 25.9, lo);
  let b = select(1.97 * r2 - 3.27 * roughness + 0.72, 44.0 * r2 - 23.7 * roughness + 3.26, lo);
  let dg = exp(a * dot_nv + b) + select(0.1 * (roughness - 0.25), 0.0, lo);
  return saturate_(dg * RECIPROCAL_PI);
}
fn dfg_approx(n: vec3f, v: vec3f, roughness: f32) -> vec2f {
  let dot_nv = saturate_(dot(n, v));
  let r = roughness * vec4f(-1.0, -0.0275, -0.572, 0.022) + vec4f(1.0, 0.0425, 1.04, -0.04);
  let a004 = min(r.x * r.x, exp2(-9.28 * dot_nv)) * r.x + r.y;
  return vec2f(-1.04, 1.04) * a004 + r.zw;
}
// cube_uv_reflection_fragment（PMREM の atlas は tex1。three の v は下が 0、的は上が 0）
const CUBEUV_MIN_MIP: f32 = 4.0;
const CUBEUV_MIN_TILE: f32 = 16.0;
const CUBEUV_MAX_MIP: f32 = 8.0;
const CUBEUV_TEXEL_W: f32 = 1.0 / 768.0;
const CUBEUV_TEXEL_H: f32 = 1.0 / 1024.0;
fn cube_face(dir: vec3f) -> f32 {
  let a = abs(dir);
  if (a.x > a.z) {
    if (a.x > a.y) { return select(3.0, 0.0, dir.x > 0.0); }
    return select(4.0, 1.0, dir.y > 0.0);
  }
  if (a.z > a.y) { return select(5.0, 2.0, dir.z > 0.0); }
  return select(4.0, 1.0, dir.y > 0.0);
}
fn cube_uv(dir: vec3f, face: f32) -> vec2f {
  var uv: vec2f;
  if (face == 0.0) { uv = vec2f(dir.z, dir.y) / abs(dir.x); }
  else if (face == 1.0) { uv = vec2f(-dir.x, -dir.z) / abs(dir.y); }
  else if (face == 2.0) { uv = vec2f(-dir.x, dir.y) / abs(dir.z); }
  else if (face == 3.0) { uv = vec2f(-dir.z, dir.y) / abs(dir.x); }
  else if (face == 4.0) { uv = vec2f(-dir.x, dir.z) / abs(dir.y); }
  else { uv = vec2f(dir.x, dir.y) / abs(dir.z); }
  return 0.5 * (uv + 1.0);
}
fn bilinear_cube_uv(dir: vec3f, mip0: f32) -> vec3f {
  var face = cube_face(dir);
  let filter_int = max(CUBEUV_MIN_MIP - mip0, 0.0);
  let mip = max(mip0, CUBEUV_MIN_MIP);
  let face_size = exp2(mip);
  var uv = cube_uv(dir, face) * (face_size - 2.0) + 1.0;
  if (face > 2.0) { uv.y += face_size; face -= 3.0; }
  uv.x += face * face_size;
  uv.x += filter_int * 3.0 * CUBEUV_MIN_TILE;
  uv.y += 4.0 * (exp2(CUBEUV_MAX_MIP) - face_size);
  uv.x *= CUBEUV_TEXEL_W;
  uv.y *= CUBEUV_TEXEL_H;
  return textureSampleLevel(tex1, smp1, vec2f(uv.x, 1.0 - uv.y), 0.0).rgb;
}
fn roughness_to_mip(r: f32) -> f32 {
  if (r >= 0.8) { return (1.0 - r) * (-1.0 + 2.0) / (1.0 - 0.8) - 2.0; }
  if (r >= 0.4) { return (0.8 - r) * (2.0 + 1.0) / (0.8 - 0.4) - 1.0; }
  if (r >= 0.305) { return (0.4 - r) * (3.0 - 2.0) / (0.4 - 0.305) + 2.0; }
  if (r >= 0.21) { return (0.305 - r) * (4.0 - 3.0) / (0.305 - 0.21) + 3.0; }
  return -2.0 * log2(1.16 * r);
}
fn texture_cube_uv(dir: vec3f, roughness: f32) -> vec3f {
  let mip = clamp(roughness_to_mip(roughness), -2.0, CUBEUV_MAX_MIP);
  let mip_f = fract(mip);
  let mip_int = floor(mip);
  let c0 = bilinear_cube_uv(dir, mip_int);
  if (mip_f == 0.0) { return c0; }
  return mix(c0, bilinear_cube_uv(dir, mip_int + 1.0), mip_f);
}
// getIBLRadiance（計算はワールド空間なので inverseTransformDirection は要らない）
fn ibl_radiance(v: vec3f, n: vec3f, roughness: f32) -> vec3f {
  let rv = normalize(mix(reflect(-v, n), n, roughness * roughness));
  return texture_cube_uv(rv, roughness) * m.specular.w;
}

// three r182 以降（Engimeow は r186）の光のエネルギー。Kit::three_r186 で lights.ambient.w = 1
fn r186() -> bool { return lights.ambient.w > 0.5; }
/// computeMultiscattering の FssEss + Fms * Ems と、その成分（single, multi）
fn multi_scatter(fab: vec2f, f0: vec3f) -> array<vec3f, 2> {
  let fss_ess = f0 * fab.x + vec3f(fab.y);
  let ess = fab.x + fab.y;
  let ems = 1.0 - ess;
  let favg = f0 + (vec3f(1.0) - f0) * 0.047619;
  let fms = fss_ess * favg / (vec3f(1.0) - ems * favg);
  return array<vec3f, 2>(fss_ess, fms * ems);
}
/// r186: 直接光のつやの多重散乱の補正と、つやに取られた分を拡散から引く
fn r186_direct(r: ptr<function, Refl>, s: Surf, irr: vec3f, dot_vh: f32, spec: vec3f) -> vec3f {
  (*r).dd -= irr * RECIPROCAL_PI * s.diffuse * f_schlick(vec3f(0.04), 1.0, dot_vh);
  let fab = dfg_approx(s.n, s.v, s.roughness);
  return spec * (vec3f(1.0) + s.spec * (1.0 / (fab.x + fab.y) - 1.0));
}
// RE_Direct_*
fn re_direct(mode: u32, r: ptr<function, Refl>, s: Surf, l: vec3f, lc: vec3f) {
  var dot_nl = saturate_(dot(s.n, l));
  // softMaterial の回り込み
  if (mode == MODE_SOFT) { dot_nl = smoothstep(0.18, 0.86, dot(s.n, l) * 0.5 + 0.5); }
  let irr = dot_nl * lc;
  (*r).dd += irr * RECIPROCAL_PI * s.diffuse;
  if (mode == MODE_PHONG) {
    let h = normalize(l + s.v);
    let dot_nh = saturate_(dot(s.n, h));
    let dot_vh = saturate_(dot(s.v, h));
    let f = f_schlick(s.spec, 1.0, dot_vh);
    let dd = RECIPROCAL_PI * (s.shininess * 0.5 + 1.0) * pow(dot_nh, s.shininess);
    (*r).ds += irr * f * (0.25 * dd);
  } else if (mode == MODE_STANDARD) {
    let alpha = pow2(s.roughness);
    let h = normalize(l + s.v);
    let dot_nv = saturate_(dot(s.n, s.v));
    let dot_nh = saturate_(dot(s.n, h));
    let dot_vh = saturate_(dot(s.v, h));
    let f = f_schlick(s.spec, 1.0, dot_vh);
    var sp = irr * f * (v_ggx(alpha, dot_nl, dot_nv) * d_ggx(alpha, dot_nh));
    if (r186()) { sp = r186_direct(r, s, irr, dot_vh, sp); }
    (*r).ds += sp;
  } else if (mode == MODE_PHYSICAL) {
    // RE_Direct_Physical（clearcoat・sheen・BRDF_GGX。f90 は 1）
    let alpha = pow2(s.roughness);
    let h = normalize(l + s.v);
    let dot_nv = saturate_(dot(s.n, s.v));
    let dot_nh = saturate_(dot(s.n, h));
    let dot_vh = saturate_(dot(s.v, h));
    let cc_alpha = pow2(s.cc_rough);
    (*r).cc += irr * f_schlick(vec3f(0.04), 1.0, dot_vh) * (v_ggx(cc_alpha, dot_nl, dot_nv) * d_ggx(cc_alpha, dot_nh));
    (*r).sh += irr * s.sheen * (d_charlie(s.sheen_rough, dot_nh) * v_neubelt(dot_nv, dot_nl));
    var sp = irr * f_schlick(s.spec, 1.0, dot_vh) * (v_ggx(alpha, dot_nl, dot_nv) * d_ggx(alpha, dot_nh));
    if (r186()) { sp = r186_direct(r, s, irr, dot_vh, sp); }
    (*r).ds += sp;
  }
}

// lights_fragment_begin の組み立て。返すのは (間接の光の irradiance)
fn gather(mode: u32, r: ptr<function, Refl>, s: Surf, i: MOut, world_n: vec3f) -> vec3f {
  let world = i.world;
  for (var k = 0u; k < lights.counts.y; k++) {
    let p = lights.point[k];
    let lv = p.pos.xyz - world;
    let d = length(lv);
    let c = p.color.rgb * distance_attenuation(d, p.pos.w, p.color.w);
    re_direct(mode, r, s, normalize(lv), c);
  }
  for (var k = 0u; k < lights.counts.z; k++) {
    let p = lights.spot[k];
    let lv = p.pos.xyz - world;
    let l = normalize(lv);
    let att = smoothstep(p.cone.x, p.cone.y, dot(l, p.dir.xyz));
    if (att > 0.0) {
      let c = p.color.rgb * att * distance_attenuation(length(lv), p.pos.w, p.dir.w);
      re_direct(mode, r, s, l, c);
    }
  }
  for (var k = 0u; k < lights.counts.x; k++) {
    var c = lights.dir[k].color.rgb;
    if (k < lights.shadow_count.x && i.recv > 0.5) { c *= dir_shadow(k, world, world_n); }
    re_direct(mode, r, s, lights.dir[k].dir.xyz, c);
  }
  var irr = lights.ambient.rgb;
  for (var k = 0u; k < lights.counts.w; k++) {
    let h = lights.hemi[k];
    let w = 0.5 * dot(s.n, h.dir.xyz) + 0.5;
    irr += mix(h.ground.rgb, h.sky.rgb, w);
  }
  return irr;
}

fn cloud_hash(p: vec2f) -> f32 { return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453); }
fn cloud_noise(p: vec2f) -> f32 {
  let i = floor(p);
  var f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(cloud_hash(i), cloud_hash(i + vec2f(1.0, 0.0)), f.x), mix(cloud_hash(i + vec2f(0.0, 1.0)), cloud_hash(i + vec2f(1.0, 1.0)), f.x), f.y);
}

// meshphysical の最後（lights_fragment_maps・lights_fragment_end・RE_IndirectSpecular_Physical・sheen と clearcoat の重ね）
fn physical_out(r: ptr<function, Refl>, s: Surf, irr: vec3f) -> vec3f {
  var radiance = vec3f(0.0);
  var ibl_irr = vec3f(0.0);
  var cc_radiance = vec3f(0.0);
  if (has(F_ENV)) {
    ibl_irr = K_PI * texture_cube_uv(s.n, 1.0) * m.specular.w;
    radiance = ibl_radiance(s.v, s.n, s.roughness);
    cc_radiance = ibl_radiance(s.v, s.n, s.cc_rough);
  }
  // clearcoat の EnvironmentBRDF
  let cc_fab = dfg_approx(s.n, s.v, s.cc_rough);
  let cc_ind = cc_radiance * (vec3f(0.04) * cc_fab.x + vec3f(cc_fab.y));
  let sh_ind = ibl_irr * s.sheen * ibl_sheen_brdf(s.n, s.v, s.sheen_rough);
  // computeMultiscattering
  let fab = dfg_approx(s.n, s.v, s.roughness);
  let fss_ess = s.spec * fab.x + vec3f(fab.y);
  let ess = fab.x + fab.y;
  let ems = 1.0 - ess;
  let favg = s.spec + (vec3f(1.0) - s.spec) * 0.047619;
  let fms = fss_ess * favg / (vec3f(1.0) - ems * favg);
  let multi = fms * ems;
  let cos_irr = ibl_irr * RECIPROCAL_PI;
  let total = fss_ess + multi;
  let diffuse2 = s.diffuse * (1.0 - max(max(total.r, total.g), total.b));
  var ind_spec = radiance * fss_ess + multi * cos_irr;
  var ind_diff = irr * RECIPROCAL_PI * s.diffuse + diffuse2 * cos_irr;
  if (r186()) {
    // RE_IndirectSpecular_Physical・RE_IndirectDiffuse_Physical: 誘電体と金属を別に計算して混ぜる。拡散は誘電体の分だけ引く
    let d = multi_scatter(fab, vec3f(0.04));
    let k = max(m.p.x, 0.0);
    let base = select(s.spec, (s.spec - vec3f(0.04) * (1.0 - k)) / max(k, 1e-4), k > 0.0);
    let me = multi_scatter(fab, base);
    let single = mix(d[0], me[0], k);
    let many = mix(d[1], me[1], k);
    let keep = vec3f(1.0) - d[0] - d[1];
    ind_spec = radiance * single + many * cos_irr;
    ind_diff = (irr * RECIPROCAL_PI + cos_irr) * s.diffuse * keep;
  }
  var out = (*r).dd + ind_diff + (*r).ds + ind_spec + m.emissive.rgb;
  // sheen
  let sheen_comp = 1.0 - 0.157 * max(max(s.sheen.r, s.sheen.g), s.sheen.b);
  out = out * sheen_comp + (*r).sh + sh_ind;
  // clearcoat
  let cc = saturate_(m.p.z);
  let fcc = f_schlick(vec3f(0.04), 1.0, saturate_(dot(s.n, s.v)));
  return out * (1.0 - cc * fcc) + ((*r).cc + cc_ind) * cc;
}

// bumpmap_pars_fragment（dHdxy_fwd と perturbNormalArb。m.p.z = bumpScale、tex1）
fn bump_normal(n: vec3f, uv: vec2f, fx: vec3f, fy: vec3f, front: bool) -> vec3f {
  let dx = dpdx(uv);
  let dy = dpdy(uv);
  let hll = m.p.z * textureSampleGrad(tex1, smp1, uv, dx, dy).x;
  let dbx = m.p.z * textureSampleGrad(tex1, smp1, uv + dx, dx, dy).x - hll;
  let dby = m.p.z * textureSampleGrad(tex1, smp1, uv + dy, dx, dy).x - hll;
  let sx = normalize(fx);
  let sy = normalize(fy);
  let r1 = cross(sy, n);
  let r2 = cross(n, sx);
  let det = dot(sx, r1) * select(-1.0, 1.0, front);
  let grad = sign(det) * (dbx * r1 + dby * r2);
  return normalize(abs(det) * n - grad);
}

fn lit(mode: u32, i: MOut, front: bool) -> vec4f {
  k_frag = i.clip.xy;
  let d = diffuse_color(i);
  if (d.a < m.emissive.w) { discard; }
  // normal_fragment_begin
  let fx = dpdx(i.world);
  let fy = dpdy(i.world);
  var n: vec3f;
  if (has(F_FLAT)) {
    // cross(dFdx(vViewPosition), dFdy(vViewPosition))。WGSL の dpdy は下向きで、vViewPosition は位置の符号違い
    n = normalize(cross(fy, fx));
  } else {
    n = normalize(i.normal);
    if (has(F_DOUBLE) && !front) { n = -n; }
  }
  if (has(F_BUMP)) { n = bump_normal(n, i.uv, fx, fy, front); }
  let v = normalize(g.cam.xyz - i.world);
  var s: Surf;
  s.n = n;
  s.v = v;
  s.diffuse = d.rgb;
  if (mode == MODE_PHONG) {
    s.spec = m.specular.rgb;
    s.shininess = m.specular.w;
  } else if (mode == MODE_STANDARD) {
    // lights_physical_fragment（ior 1.5 の f0 = 0.04）
    let metal = m.p.x;
    s.diffuse = d.rgb * (1.0 - metal);
    let nv = (g.view * vec4f(n, 0.0)).xyz;
    let dxy = max(abs(dpdx(nv)), abs(dpdy(nv)));
    let geo_rough = max(max(dxy.x, dxy.y), dxy.z);
    s.roughness = min(max(m.p.y, 0.0525) + geo_rough, 1.0);
    s.spec = mix(vec3f(0.04), d.rgb, metal);
  } else if (mode == MODE_PHYSICAL) {
    // lights_physical_fragment（ior 1.5。specularF90 は 1）
    let metal = m.p.x;
    s.diffuse = d.rgb * (1.0 - metal);
    let nv = (g.view * vec4f(n, 0.0)).xyz;
    let dxy = max(abs(dpdx(nv)), abs(dpdy(nv)));
    let geo_rough = max(max(dxy.x, dxy.y), dxy.z);
    s.roughness = min(max(m.p.y, 0.0525) + geo_rough, 1.0);
    s.spec = mix(vec3f(pow2((1.5 - 1.0) / (1.5 + 1.0))), d.rgb, metal);
    s.cc_rough = min(max(m.p.w, 0.0525) + geo_rough, 1.0);
    s.sheen = m.specular.rgb;
    s.sheen_rough = clamp(m.uv0.w, 0.07, 1.0);
  }
  var r: Refl;
  // 影の法線は補間のまま（three は頂点で足す。方向は面で反転しない）
  let irr = gather(mode, &r, s, i, i.normal);
  if (mode == MODE_PHYSICAL) { return finish(physical_out(&r, s, irr), d.a, i.depth); }
  var indirect = irr * RECIPROCAL_PI * s.diffuse;
  if (r186() && mode == MODE_STANDARD) {
    let d = multi_scatter(dfg_approx(s.n, s.v, s.roughness), vec3f(0.04));
    indirect *= vec3f(1.0) - d[0] - d[1];
  }
  // emissivemap_fragment
  var emissive = m.emissive.rgb;
  if (has(F_EMAP)) { emissive *= textureSampleGrad(tex1, smp1, i.uv, dpdx(i.uv), dpdy(i.uv)).rgb; }
  if (has(F_VIEW_RIM)) {
    let nv = (g.view * vec4f(n, 0.0)).xyz;
    let vv = (g.view * vec4f(v, 0.0)).xyz;
    let fr = pow(1.0 - clamp(dot(nv, vv), 0.0, 1.0), 2.4);
    let rim_side = clamp(dot(nv, normalize(vec3f(-0.5, 0.6, -0.62))) * 0.9 + 0.35, 0.0, 1.0);
    emissive += m.specular.rgb * fr * rim_side;
  }
  var out = r.dd + indirect + r.ds + emissive;
  if (mode == MODE_SOFT) {
    // softMaterial の輪郭の光と灯りの戻り
    let rim_f = pow(1.0 - saturate_(dot(n, v)), 3.0);
    out += g.rim.rgb * rim_f * g.rim.w * m.p.z;
    var back = 0.0;
    for (var k = 0; k < 8; k++) {
      let lp = lights.lamps[k];
      if (lp.w <= 0.0) { continue; }
      let dd = length(i.world.xz - lp.xy);
      back = max(back, lp.w * (1.0 - smoothstep(lp.z * 0.55, lp.z, dd)));
    }
    let dim = lights.dim.x;
    let dim_here = dim * (1.0 - back);
    let lum = dot(out, vec3f(0.2126, 0.7152, 0.0722));
    let cool = mix(vec3f(lum), out, 0.7) * vec3f(0.86, 0.93, 1.12);
    out = mix(out, cool, dim_here);
    out *= mix(vec3f(1.0), vec3f(1.05, 1.02, 0.96), back * dim);
    // 流れる雲の影（lights.dim.y。0 なら何もしない）
    let cloud = lights.dim.y;
    if (cloud > 0.0) {
      let cp = i.world.xz * 0.045 + vec2f(g.misc.x * 0.018, g.misc.x * 0.007);
      let cl = cloud_noise(cp) * 0.65 + cloud_noise(cp * 2.3) * 0.35;
      out *= 1.0 - smoothstep(0.52, 0.72, cl) * 0.2 * cloud;
    }
  }
  return finish(out, d.a, i.depth);
}

@fragment fn fs_lambert(i: MOut, @builtin(front_facing) front: bool) -> @location(0) vec4f { return lit(MODE_LAMBERT, i, front); }
@fragment fn fs_soft(i: MOut, @builtin(front_facing) front: bool) -> @location(0) vec4f { return lit(MODE_SOFT, i, front); }
@fragment fn fs_phong(i: MOut, @builtin(front_facing) front: bool) -> @location(0) vec4f { return lit(MODE_PHONG, i, front); }
@fragment fn fs_standard(i: MOut, @builtin(front_facing) front: bool) -> @location(0) vec4f { return lit(MODE_STANDARD, i, front); }
@fragment fn fs_physical(i: MOut, @builtin(front_facing) front: bool) -> @location(0) vec4f { return lit(MODE_PHYSICAL, i, front); }

// ---- 影の型 ----
@vertex fn vs_shadow(v: VIn, i: IIn) -> @builtin(position) vec4f {
  return g.light_vp * inst_model(i) * vec4f(v.pos, 1.0);
}
struct ShadowOut { @builtin(position) clip: vec4f, @location(0) uv: vec2f };
@vertex fn vs_shadow_alpha(v: VIn, i: IIn) -> ShadowOut {
  var o: ShadowOut;
  o.clip = g.light_vp * inst_model(i) * vec4f(v.pos, 1.0);
  o.uv = map_uv(v.uv);
  return o;
}
@vertex fn vs_shadow_skinned(v: VIn, @location(5) joints: vec4u, @location(6) weights: vec4f) -> @builtin(position) vec4f {
  return g.light_vp * k_skin_matrix(joints, weights) * vec4f(v.pos, 1.0);
}
@vertex fn vs_shadow_alpha_skinned(v: VIn, @location(5) joints: vec4u, @location(6) weights: vec4f) -> ShadowOut {
  var o: ShadowOut;
  o.clip = g.light_vp * k_skin_matrix(joints, weights) * vec4f(v.pos, 1.0);
  o.uv = map_uv(v.uv);
  return o;
}
@fragment fn fs_shadow_alpha(i: ShadowOut) {
  if (textureSample(tex0, smp0, i.uv).a < m.emissive.w) { discard; }
}

// ---- 自機の型・物陰の影絵（層 1。renderer.ts の HiddenPlayerPass）----
@vertex fn vs_mask(v: VIn, i: IIn) -> @builtin(position) vec4f {
  return g.view_proj * inst_model(i) * vec4f(v.pos, 1.0);
}
@fragment fn fs_mask() -> @location(0) vec4f { return vec4f(1.0); }

struct HiddenOut { @builtin(position) clip: vec4f, @location(0) depth: f32 };
@vertex fn vs_hidden(v: VIn, i: IIn) -> HiddenOut {
  var mv = (g.view * inst_model(i) * vec4f(v.pos, 1.0)).xyz;
  mv -= normalize(mv) * 0.85;
  var o: HiddenOut;
  o.clip = g.proj * vec4f(mv, 1.0);
  o.depth = -mv.z;
  return o;
}
@fragment fn fs_hidden(i: HiddenOut) -> @location(0) vec4f {
  return vec4f(apply_fog(lights.hidden.rgb, i.depth), lights.hidden.a);
}
