// 主役: 骨で動かす（骨の行列・モーフの差分は storage buffer）。材質は character-runtime の光なし（MeshBasicMaterial）に hero.ts の addRim（輪郭の光・闘気・flash）を重ねたもの。
// 武器は three の MeshStandardMaterial（直接光 GGX + 半球光 + 霧。影は受けない）。
@group(2) @binding(0) var map: texture_2d<f32>;
@group(2) @binding(1) var map_smp: sampler;
@group(3) @binding(0) var<storage, read> palette: array<mat4x4f>;
@group(3) @binding(1) var<storage, read> deltas: array<Delta>;
@group(3) @binding(2) var<storage, read> morph_w: array<f32>;
@group(3) @binding(3) var<uniform> hero: HeroU;
@group(3) @binding(4) var<uniform> draw: DrawU;

struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
  @location(2) uv: vec2f,
  @location(3) color: vec4f,
  @location(4) mv: vec3f,
};

@vertex fn vs_main(v: VIn, @location(5) joints: vec4u, @location(6) weights: vec4f, @builtin(vertex_index) vid: u32) -> VOut {
  let s = skin(v, joints, weights, vid, hero, draw, 0u, 0u);
  var o: VOut;
  let mv = g.view * vec4f(s.world, 1.0);
  o.clip = g.proj * mv;
  o.world = s.world;
  o.normal = s.normal;
  o.uv = v.uv;
  o.color = v.color;
  o.mv = mv.xyz;
  return o;
}

/// 光なしの体（MeshBasicMaterial + addRim + 霧）
@fragment fn fs_basic(i: VOut, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  if (fade_out(i.clip, hero)) { discard; }
  let t = textureSample(map, map_smp, i.uv);
  let d = draw.color * t * i.color;
  if (d.a < draw.p.x) { discard; }
  let a = select(d.a, 1.0, draw.p.y > 0.5);
  let c = shade_sun(shade_rim(d.rgb, i.normal, i.mv, hero), i.normal, i.mv, front, hero);
  return shade_out(c, a, -i.mv.z, true, false, false);
}

/// 両面の半透明の裏の面（three の FLIP_SIDED: 法線を裏返す）
@fragment fn fs_basic_back(i: VOut, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  if (fade_out(i.clip, hero)) { discard; }
  let t = textureSample(map, map_smp, i.uv);
  let d = draw.color * t * i.color;
  if (d.a < draw.p.x) { discard; }
  let a = select(d.a, 1.0, draw.p.y > 0.5);
  let c = shade_sun(shade_rim(d.rgb, -i.normal, i.mv, hero), -i.normal, i.mv, front, hero);
  return shade_out(c, a, -i.mv.z, true, false, false);
}

/// ＞＜ の板（光なし・輪郭の光なし・霧あり）
@fragment fn fs_board(i: VOut) -> @location(0) vec4f {
  let t = textureSample(map, map_smp, i.uv);
  let d = draw.color * t * i.color;
  if (d.a < draw.p.x) { discard; }
  return shade_out(d.rgb, d.a, -i.mv.z, true, false, false);
}

/// 光なしの武器（aisns の loadWeaponModel が替える MeshBasicMaterial。輪郭の光なし・霧あり。set_weapon_unlit）
@fragment fn fs_weapon_unlit(i: VOut) -> @location(0) vec4f {
  if (fade_out(i.clip, hero)) { discard; }
  let t = textureSample(map, map_smp, i.uv);
  let d = draw.color * t * i.color;
  if (d.a < draw.p.x) { discard; }
  return shade_out(d.rgb, select(d.a, 1.0, draw.p.y > 0.5), -i.mv.z, true, false, false);
}

fn d_ggx(alpha: f32, dot_nh: f32) -> f32 {
  let a2 = alpha * alpha;
  let denom = dot_nh * dot_nh * (a2 - 1.0) + 1.0;
  return RECIPROCAL_PI * a2 / (denom * denom);
}
fn v_ggx(alpha: f32, dot_nl: f32, dot_nv: f32) -> f32 {
  let a2 = alpha * alpha;
  let gv = dot_nl * sqrt(a2 + (1.0 - a2) * dot_nv * dot_nv);
  let gl = dot_nv * sqrt(a2 + (1.0 - a2) * dot_nl * dot_nl);
  return 0.5 / max(gv + gl, 1e-6);
}

/// 武器（three r178 の MeshStandardMaterial。環境マップなし・影を受けない）
@fragment fn fs_std(i: VOut, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  if (fade_out(i.clip, hero)) { discard; }
  let t = textureSample(map, map_smp, i.uv);
  let d = draw.color * t * i.color;
  if (d.a < draw.p.x) { discard; }
  let n0 = normalize(i.normal);
  let dxy = max(abs(dpdx(n0)), abs(dpdy(n0)));
  let geo_rough = max(max(dxy.x, dxy.y), dxy.z);
  let n = select(-n0, n0, front);
  let v = normalize(g.cam.xyz - i.world);
  let metal = draw.p.w;
  let diffuse = d.rgb * (1.0 - metal);
  let f0 = mix(vec3f(0.04), d.rgb, metal);
  let rough = min(max(draw.p.z, 0.0525) + geo_rough, 1.0);
  let l = g.key_dir.xyz;
  let dot_nl = clamp(dot(n, l), 0.0, 1.0);
  let irr = g.key_color.rgb * dot_nl;
  let h = normalize(l + v);
  let dot_nv = clamp(dot(n, v), 0.0, 1.0);
  let dot_nh = clamp(dot(n, h), 0.0, 1.0);
  let dot_vh = clamp(dot(v, h), 0.0, 1.0);
  let alpha = rough * rough;
  let fres = pow(1.0 - dot_vh, 5.0);
  let f = f0 * (1.0 - fres) + vec3f(fres);
  let spec = f * v_ggx(alpha, dot_nl, dot_nv) * d_ggx(alpha, dot_nh);
  let lambert = diffuse * RECIPROCAL_PI;
  let c = irr * spec + irr * lambert + hemi_irradiance(n) * lambert + draw.e.rgb;
  let a = select(d.a, 1.0, draw.p.y > 0.5);
  return shade_out(c, a, -i.mv.z, true, true, false);
}

// 影の型
struct ShadowOut { @builtin(position) clip: vec4f, @location(0) uv: vec2f };
@vertex fn vs_shadow(v: VIn, @location(5) joints: vec4u, @location(6) weights: vec4f, @builtin(vertex_index) vid: u32) -> ShadowOut {
  let s = skin(v, joints, weights, vid, hero, draw, 0u, 0u);
  var o: ShadowOut;
  o.clip = g.light_vp * vec4f(s.world, 1.0);
  o.uv = v.uv;
  return o;
}
@fragment fn fs_shadow_alpha(i: ShadowOut) {
  if (textureSample(map, map_smp, i.uv).a < draw.p.x) { discard; }
}

// 自機だけの型（白で描く。縁取り用）
@vertex fn vs_mask(v: VIn, @location(5) joints: vec4u, @location(6) weights: vec4f, @builtin(vertex_index) vid: u32) -> @builtin(position) vec4f {
  let s = skin(v, joints, weights, vid, hero, draw, 0u, 0u);
  return g.view_proj * vec4f(s.world, 1.0);
}
@fragment fn fs_mask() -> @location(0) vec4f { return vec4f(1.0); }

// 物陰の影絵: 奥にあるときだけ描く。自分の体の重なりでは出ないよう、少し手前に寄せて比べる
struct HiddenOut { @builtin(position) clip: vec4f, @location(0) depth: f32 };
@vertex fn vs_hidden(v: VIn, @location(5) joints: vec4u, @location(6) weights: vec4f, @builtin(vertex_index) vid: u32) -> HiddenOut {
  let s = skin(v, joints, weights, vid, hero, draw, 0u, 0u);
  var mv = (g.view * vec4f(s.world, 1.0)).xyz;
  mv -= normalize(mv) * 0.85;
  var o: HiddenOut;
  o.clip = g.proj * vec4f(mv, 1.0);
  o.depth = -mv.z;
  return o;
}
@fragment fn fs_hidden(i: HiddenOut) -> @location(0) vec4f {
  return vec4f(apply_fog(hero.hidden.rgb, i.depth), hero.hidden.a);
}
