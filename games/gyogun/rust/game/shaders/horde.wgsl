// 群れの共通の描き方: 主役が乗るわたあめ雲のトゥーン調の材質と、敵の足元の丸影（魚の体は fish.wgsl）。
// aFx: x = 当たりの白い光、y = 攻撃の構えの赤い光、z = 消える前の白化、w = 金の縁。
struct KindU { sparkle: vec4f, captain: vec4f };
@group(2) @binding(0) var map_t: texture_2d<f32>;
@group(2) @binding(1) var map_s: sampler;
@group(3) @binding(0) var<uniform> k: KindU;

struct Out {
  @builtin(position) pos: vec4f,
  @location(0) world: vec3f,
  @location(1) n: vec3f,
  @location(2) uv: vec2f,
  @location(3) fx: vec4f,
};

@vertex fn vs(v: VIn, i: IIn) -> Out {
  let m = inst_model(i);
  let w = m * vec4f(v.pos, 1.0);
  var o: Out;
  o.pos = g.view_proj * w;
  o.world = w.xyz;
  o.n = normal_world(m, v.normal);
  o.uv = v.uv;
  o.fx = i.fx;
  return o;
}

@fragment fn fs(i: Out) -> @location(0) vec4f {
  let alb = textureSample(map_t, map_s, i.uv).rgb;
  let n = normalize(i.n);
  let v = normalize(g.cam.xyz - i.world);
  let hd = n.y * 0.5 + 0.5;
  let fill = mix(g.hemi_ground.rgb, g.hemi_sky.rgb, hd);
  let hl = dot(n, g.key_dir.xyz) * 0.5 + 0.5;
  let key = g.key_color.rgb * smoothstep(0.3, 0.8, hl);
  let refl = max(luma(g.hemi_sky.rgb) + luma(g.key_color.rgb), 0.001);
  let light_k = vec3f(0.55) + 0.45 * (fill + key) / refl;
  let nv = clamp(dot(n, v), 0.0, 1.0);
  let rim = pow(1.0 - nv, 2.6);
  var c = alb * light_k * 0.95 + vec3f(1.0, 0.82, 0.7) * rim * 0.35;
  // 構え: 赤く脈打つ縁
  let warn = i.fx.y * (0.6 + 0.4 * sin(g.misc.x * 28.0));
  c = mix(c, c * vec3f(1.3, 0.55, 0.5), i.fx.y * 0.5);
  c += vec3f(1.0, 0.18, 0.12) * (rim * 2.0 + 0.15) * warn;
  // 隊長: 金の縁 / 連鎖に巻き込まれた敵: 金に燃える（どこを連鎖が走っているか見えるように）
  let cap = k.captain.x;
  c += vec3f(1.0, 0.7, 0.3) * rim * i.fx.w * 0.6 * cap;
  c = mix(c, c * 0.55 + vec3f(1.0, 0.72, 0.28) * (0.55 + rim * 1.6), i.fx.w * (1.0 - cap) * 0.85);
  // 当たり: 白く飛ぶ（インパクトの1コマ）
  c = mix(c, c * 0.4 + vec3f(0.75, 0.55, 0.4), clamp(i.fx.x, 0.0, 1.0) * 0.6);
  c = mix(c, k.sparkle.rgb * 0.9 + vec3f(0.15), i.fx.z * 0.7);
  return vec4f(apply_fog(c, view_depth(i.world)), 1.0);
}

// ---- 影の型 ----
@vertex fn vs_shadow(v: VIn, i: IIn) -> @builtin(position) vec4f {
  return g.light_vp * inst_model(i) * vec4f(v.pos, 1.0);
}

// ---- 足元の丸影（fx.rgb = 色） ----
struct SOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f, @location(1) col: vec3f, @location(2) world: vec3f };
@vertex fn vs_blob(v: VIn, i: IIn) -> SOut {
  let w = inst_model(i) * vec4f(v.pos, 1.0);
  var o: SOut;
  o.pos = g.view_proj * w;
  o.uv = v.uv;
  o.col = i.fx.rgb;
  o.world = w.xyz;
  return o;
}
@fragment fn fs_blob(i: SOut) -> @location(0) vec4f {
  let t = textureSample(map_t, map_s, i.uv);
  return vec4f(apply_fog(i.col * t.rgb, view_depth(i.world)), t.a * 0.55);
}
