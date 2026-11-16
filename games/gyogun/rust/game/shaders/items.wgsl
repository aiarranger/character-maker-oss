// 拾い物の光の輪（Sprite・加算）と、投げられた弾（MeshBasic・霧あり）。a = 色（rgb）と不透明度。
struct FxIn {
  @location(8) m0: vec4f, @location(9) m1: vec4f, @location(10) m2: vec4f, @location(11) m3: vec4f,
  @location(12) a: vec4f, @location(13) b: vec4f, @location(14) c: vec4f, @location(15) d: vec4f,
};
@group(2) @binding(0) var glow_t: texture_2d<f32>;
@group(2) @binding(1) var glow_s: sampler;
struct Out { @builtin(position) pos: vec4f, @location(0) uv: vec2f, @location(1) col: vec4f, @location(2) depth: f32 };

@vertex fn vs_sprite(v: VIn, i: FxIn) -> Out {
  var mv = g.view * i.m3;
  let sc = vec2f(length(i.m0.xyz), length(i.m1.xyz));
  mv = vec4f(mv.xy + v.pos.xy * sc, mv.z, mv.w);
  var o: Out;
  o.pos = g.proj * mv;
  o.uv = v.uv;
  o.col = i.a;
  o.depth = -mv.z;
  return o;
}
@fragment fn fs_sprite(o: Out) -> @location(0) vec4f {
  let t = textureSample(glow_t, glow_s, o.uv);
  return vec4f(apply_fog(o.col.rgb * t.rgb, o.depth), o.col.a * t.a);
}
@vertex fn vs_basic(v: VIn, i: FxIn) -> Out {
  let w = mat4x4f(i.m0, i.m1, i.m2, i.m3) * vec4f(v.pos, 1.0);
  var o: Out;
  o.pos = g.view_proj * w;
  o.uv = v.uv;
  o.col = i.a;
  o.depth = view_depth(w.xyz);
  return o;
}
@fragment fn fs_basic(o: Out) -> @location(0) vec4f { return vec4f(apply_fog(o.col.rgb, o.depth), 1.0); }
