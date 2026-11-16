// 島・小物の材質（materials.ts の softMaterial）: three r178 の MeshLambertMaterial に、回り込みの陰影・輪郭の光・草木の揺れを足したもの。
// 値は 1 個ごとに FxInst で受ける:
//   a = 色 rgb, rim（uRimK）
//   b = emissive rgb（色 × 強さ）, 影を受けるか（receiveShadow）
//   c = 不透明度, 改造前のランバートか（1 なら dotNL は saturate。拾い物の MeshLambertMaterial 用）
struct SoftIn {
  @location(8) m0: vec4f,
  @location(9) m1: vec4f,
  @location(10) m2: vec4f,
  @location(11) m3: vec4f,
  @location(12) a: vec4f,
  @location(13) b: vec4f,
  @location(14) c: vec4f,
};

struct SoftOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
  @location(2) uv: vec2f,
  @location(3) diffuse: vec3f,
  @location(4) @interpolate(flat) emissive: vec4f, // rgb, 影を受けるか
  @location(5) @interpolate(flat) misc: vec3f,     // rim, 不透明度, 改造前のランバートか
};

@group(2) @binding(0) var map_tex: texture_2d<f32>;
@group(2) @binding(1) var map_smp: sampler;

@vertex fn vs_soft(v: VIn, i: SoftIn) -> SoftOut {
  let m = mat4x4f(i.m0, i.m1, i.m2, i.m3);
  // 揺れ（_sway が 0 の頂点は動かない）。ずらす向きは行列の前なので、回転・大きさも掛かる
  let wo = m[3].xyz;
  let time = g.misc.x;
  let s = v.sway * 0.06;
  var p = v.pos;
  p.x += sin(time * 1.6 + wo.x * 0.7 + v.pos.y * 0.8) * s;
  p.z += cos(time * 1.3 + wo.z * 0.6 + v.pos.y * 0.7) * s * 0.7;
  let world = m * vec4f(p, 1.0);
  var o: SoftOut;
  o.clip = g.view_proj * world;
  o.world = world.xyz;
  o.normal = normal_world(m, v.normal);
  o.uv = v.uv;
  o.diffuse = i.a.rgb * v.color.rgb;
  o.emissive = i.b;
  o.misc = vec3f(i.a.w, i.c.x, i.c.y);
  return o;
}

@fragment fn fs_soft(o: SoftOut, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  let diffuse = o.diffuse * textureSample(map_tex, map_smp, o.uv).rgb;
  // DoubleSide: 裏から見た面は法線を返す（FrontSide は裏を描かないので常に表）
  let vn = normalize(o.normal);
  let n = select(-vn, vn, front);
  // 平行光（dotNL は回り込むように改造）。影の normalBias は頂点の法線のまま
  var key = g.key_color.rgb;
  if (o.emissive.w > 0.5) { key *= shadow_at(o.world, vn); }
  let nl = dot(n, g.key_dir.xyz);
  let dot_nl = select(smoothstep(0.18, 0.86, nl * 0.5 + 0.5), saturate(nl), o.misc.z > 0.5);
  // BRDF_Lambert = 色 / π
  var col = (dot_nl * key + hemi_irradiance(n)) * (RECIPROCAL_PI * diffuse) + o.emissive.rgb;
  // 輪郭の光
  let vv = normalize(g.cam.xyz - o.world);
  let rim_f = pow(1.0 - saturate(dot(n, vv)), 3.0);
  col += g.rim.rgb * rim_f * g.rim.w * o.misc.x;
  return vec4f(apply_fog(col, view_depth(o.world)), o.misc.y);
}

// 影の型。three の深度の材質には揺れの改造が無いので、揺らさない
@vertex fn vs_soft_shadow(v: VIn, i: SoftIn) -> @builtin(position) vec4f {
  return g.light_vp * mat4x4f(i.m0, i.m1, i.m2, i.m3) * vec4f(v.pos, 1.0);
}
