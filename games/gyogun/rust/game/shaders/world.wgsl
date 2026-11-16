// 戦場の小さな見せ方: 幟・旗の布、陣の輪（MeshBasicMaterial）、かがり火（Sprite）、遠くのかがり火（Points）。
struct FxIn {
  @location(8) m0: vec4f,
  @location(9) m1: vec4f,
  @location(10) m2: vec4f,
  @location(11) m3: vec4f,
  @location(12) a: vec4f,
  @location(13) b: vec4f,
};
fn fx_model(i: FxIn) -> mat4x4f { return mat4x4f(i.m0, i.m1, i.m2, i.m3); }

// ---- 幟（のぼり）・旗: 風になびく布。a = 色 rgb, uFlash。b = 紋の色 ----
struct ClothOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
  @location(1) shade: f32,
  @location(2) depth: f32,
  @location(3) @interpolate(flat) color: vec4f,
  @location(4) @interpolate(flat) mark: vec3f,
};
@vertex fn vs_cloth(v: VIn, i: FxIn) -> ClothOut {
  let m = fx_model(i);
  let wo = m[3];
  let time = g.misc.x;
  var p = v.pos;
  let w = v.uv.x;
  let wave = sin(time * 4.2 + w * 5.0 + wo.x * 0.3 + p.y * 1.2) * 0.22 * w + sin(time * 7.1 + w * 9.0 + wo.z) * 0.06 * w;
  p.z += wave;
  var o: ClothOut;
  o.shade = 0.78 + 0.22 * cos(time * 4.2 + w * 5.0 + wo.x * 0.3 + p.y * 1.2);
  let mv = g.view * m * vec4f(p, 1.0);
  o.clip = g.proj * mv;
  o.depth = -mv.z;
  o.uv = v.uv;
  o.color = i.a;
  o.mark = i.b.rgb;
  return o;
}
@fragment fn fs_cloth(o: ClothOut) -> @location(0) vec4f {
  let q = o.uv - vec2f(0.5, 0.62);
  // 紋: 丸に花びら（五弁）
  let a = atan2(q.y, q.x * 1.6);
  let r = length(q * vec2f(1.6, 1.0));
  let petal = 0.13 + 0.035 * cos(a * 5.0);
  let mark = 1.0 - smoothstep(petal - 0.01, petal + 0.01, r);
  let ring = smoothstep(0.17, 0.18, r) * (1.0 - smoothstep(0.195, 0.205, r));
  let border = step(o.uv.x, 0.06) + step(0.94, o.uv.x) + step(0.95, o.uv.y);
  var col = mix(o.color.rgb, o.mark, clamp(mark + ring + border * 0.7, 0.0, 1.0));
  col *= o.shade;
  col += o.color.w;
  return vec4f(apply_fog(col, o.depth), 1.0);
}

// ---- 陣の輪（MeshBasicMaterial の加算）: a = 色 rgb, 不透明度 ----
struct BasicOut {
  @builtin(position) clip: vec4f,
  @location(0) depth: f32,
  @location(1) @interpolate(flat) color: vec4f,
};
@vertex fn vs_basic(v: VIn, i: FxIn) -> BasicOut {
  let mv = g.view * fx_model(i) * vec4f(v.pos, 1.0);
  var o: BasicOut;
  o.clip = g.proj * mv;
  o.depth = -mv.z;
  o.color = i.a;
  return o;
}
@fragment fn fs_basic(o: BasicOut) -> @location(0) vec4f {
  return vec4f(apply_fog(o.color.rgb, o.depth), o.color.a);
}

// ---- 光のにじみ: Sprite（常にカメラへ向く板）と Points。a = 色 rgb, 不透明度。b.x = Points の size ----
@group(2) @binding(0) var glow_tex: texture_2d<f32>;
@group(2) @binding(1) var glow_smp: sampler;
struct GlowOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
  @location(1) depth: f32,
  @location(2) @interpolate(flat) color: vec4f,
};
@vertex fn vs_sprite(v: VIn, i: FxIn) -> GlowOut {
  let m = fx_model(i);
  var mv = g.view * m[3];
  let scale = vec2f(length(m[0].xyz), length(m[1].xyz));
  let aligned = v.pos.xy * scale;
  mv = vec4f(mv.xy + aligned, mv.z, mv.w);
  var o: GlowOut;
  o.clip = g.proj * mv;
  o.uv = v.uv;
  o.depth = -mv.z;
  o.color = i.a;
  return o;
}
// wgpu に点の大きさが無いので、1 辺 gl_PointSize の板にする。gl_PointSize = size * (height / 2) / -z
@vertex fn vs_points(v: VIn, i: FxIn) -> GlowOut {
  let mv = g.view * fx_model(i)[3];
  let point_size = i.b.x * (g.misc.y * 0.5) / -mv.z;
  var clip = g.proj * mv;
  clip = vec4f(clip.xy + v.pos.xy * 2.0 * point_size / vec2f(g.misc.z, g.misc.y) * clip.w, clip.z, clip.w);
  var o: GlowOut;
  o.clip = clip;
  o.uv = v.uv;
  o.depth = -mv.z;
  o.color = i.a;
  return o;
}
@fragment fn fs_glow(o: GlowOut) -> @location(0) vec4f {
  let t = textureSample(glow_tex, glow_smp, o.uv);
  return vec4f(apply_fog(o.color.rgb * t.rgb, o.depth), o.color.a * t.a);
}
