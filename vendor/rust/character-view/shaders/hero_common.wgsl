// Shared body math. Legacy uses zero palette/morph offsets; instancing uses each actor slot.
struct HeroU {
  rim_p: vec4f,   // rgb = uRimP、w = uRimPI
  aura: vec4f,    // rgb = uAura、w = uAuraI
  misc: vec4f,    // x = uTopI、y = uLift、z = uFlash、w = uFade（網目に抜く量）
  hidden: vec4f,  // 物陰の影絵の色と不透明度
  style: vec4f,   // x = 全体を持ち上げるか、y = rimK の指数、z = 鮮やかさ、w = 上の面の自分の色の量（ActorStyle）
  nfix0: vec4f,   // 法線の直し（入れ物の (P Pᵀ)⁻¹。actor_view.rs の write_frame）
  nfix1: vec4f,
  nfix2: vec4f,
  sun: vec4f,     // rgb = 夕日の縁の色、w = 強さ（0 で切る。Engimeow の RimLit）
};
struct DrawU {
  color: vec4f,   // rgb = 材質の色、a = opacity
  p: vec4f,       // x = alphaTest、y = 不透明か、z = roughness、w = metalness
  e: vec4f,       // rgb = emissive
  morph: vec4u,   // x = 差分の先頭、y = モーフの数、z = 重みの先頭、w = 頂点の数
  info: vec4u,    // x = 頂点の先頭、y = 骨の表の先頭、z = 骨なしの節点か
};
struct Delta { v: array<f32, 6> };

struct Skinned { world: vec3f, normal: vec3f };

fn skin(v: VIn, joints: vec4u, weights: vec4f, vid: u32, hero: HeroU, draw: DrawU, palette_base: u32, morph_base: u32) -> Skinned {
  var pos = v.pos;
  var nor = v.normal;
  let rigid = draw.info.z != 0u;
  if (draw.morph.y > 0u) {
    let at = vid - draw.info.x;
    for (var k = 0u; k < draw.morph.y; k++) {
      let w = morph_w[morph_base + draw.morph.z + k];
      if (w == 0.0) { continue; }
      let i = draw.morph.x + k * draw.morph.w + at;
      pos += vec3f(deltas[i].v[0], deltas[i].v[1], deltas[i].v[2]) * w;
      // three の MeshBasicMaterial は骨の無いメッシュの法線にモーフを足さない
      if (!rigid) { nor += vec3f(deltas[i].v[3], deltas[i].v[4], deltas[i].v[5]) * w; }
    }
  }
  let base = palette_base + draw.info.y;
  var m = palette[base + joints.x] * weights.x;
  if (weights.y != 0.0) { m += palette[base + joints.y] * weights.y; }
  if (weights.z != 0.0) { m += palette[base + joints.z] * weights.z; }
  if (weights.w != 0.0) { m += palette[base + joints.w] * weights.w; }
  var out: Skinned;
  out.world = (m * vec4f(pos, 1.0)).xyz;
  let nfix = mat3x3f(hero.nfix0.xyz, hero.nfix1.xyz, hero.nfix2.xyz);
  out.normal = select(normalize(nfix * (m * vec4f(nor, 0.0)).xyz), normal_world(m, nor), rigid);
  return out;
}

/// カメラと自分の間にいる走者は、網目に抜いて透かす（keiba の addRim。gl_FragCoord の y は下が 0）
fn fade_out(frag: vec4f, hero: HeroU) -> bool {
  if (hero.misc.w <= 0.0) { return false; }
  let dp = floor((vec2f(frag.x, g.misc.y - frag.y) % vec2f(4.0)));
  let th = ((dp.x * 2.0 + dp.y * 3.0) % 4.0) / 4.0 + 0.125;
  return th < hero.misc.w;
}

fn shade_rim(c0: vec3f, n_world: vec3f, mv: vec3f, hero: HeroU) -> vec3f {
  let lift = hero.misc.y;
  var lit: vec3f;
  if (hero.style.x > 0.5) {
    // 明るさと鮮やかさを一緒に上げる（pengo）
    lit = c0 * (1.0 + lift * 0.55);
  } else {
    // 暗い色ほど持ち上げる（明るい服は白く飛ばさない）
    let lum0 = dot(c0, vec3f(0.299, 0.587, 0.114));
    lit = c0 * (1.0 + lift * 2.2 * (1.0 - smoothstep(0.15, 0.6, lum0)));
  }
  let lum = dot(lit, vec3f(0.299, 0.587, 0.114));
  var c = max(mix(vec3f(lum), lit, 1.0 + lift * hero.style.z), vec3f(0.0));
  // 光が全部 0 なら法線を読まない（three の MeshBasicMaterial と同じ。まぶたの閉じる頂点など法線が 0 の所で normalize が NaN になり得る）
  if (hero.rim_p.w <= 0.0 && hero.misc.x <= 0.0 && hero.aura.w <= 0.0) { return mix(c, vec3f(1.0, 0.35, 0.3), hero.misc.z * 0.6); }
  let n = normalize((g.view * vec4f(n_world, 0.0)).xyz);
  let rim_k = pow(1.0 - clamp(dot(n, normalize(-mv)), 0.0, 1.0), hero.style.y);
  c += hero.rim_p.rgb * rim_k * hero.rim_p.w;
  let up = normalize((g.view * vec4f(0.0, 1.0, 0.0, 0.0)).xyz);
  let top_k = pow(max(dot(n, up), 0.0), 2.0);
  c += hero.rim_p.rgb * top_k * hero.misc.x + c * top_k * hero.misc.x * hero.style.w;
  // 闘気: 縁から金色に燃える（乱舞・強攻撃）
  c += hero.aura.rgb * (rim_k * 1.6 + 0.03) * hero.aura.w;
  c = mix(c, vec3f(1.0, 0.35, 0.3), hero.misc.z * 0.6);
  return c;
}

/// 夕日の陰と縁の光（Engimeow の RimLit の basic の分岐。カメラから見た向きで計算する）
fn shade_sun(c0: vec3f, n_world: vec3f, mv: vec3f, front: bool, hero: HeroU) -> vec3f {
  if (hero.sun.w == 0.0) { return c0; }
  // w が負: 色を掛けるだけ（set_tint）
  if (hero.sun.w < 0.0) { return c0 * hero.sun.rgb; }
  var rn = normalize((g.view * vec4f(n_world, 0.0)).xyz);
  if (!front) { rn = -rn; }
  let rv = normalize(-mv);
  let fr = pow(1.0 - clamp(dot(rn, rv), 0.0, 1.0), 3.0);
  let side = clamp(dot(rn, normalize(vec3f(-0.5, 0.6, -0.62))) * 0.9 + 0.35, 0.0, 1.0);
  let key = clamp(dot(rn, normalize(vec3f(0.3, 0.55, 0.78))) * 0.5 + 0.5, 0.0, 1.0);
  var c = c0 * mix(vec3f(0.6, 0.54, 0.7), vec3f(0.98, 0.9, 0.82), smoothstep(0.15, 0.85, key));
  c += hero.sun.rgb * fr * side * hero.sun.w;
  return c;
}
