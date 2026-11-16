@group(2) @binding(0) var map: texture_2d<f32>;
@group(2) @binding(1) var map_smp: sampler;

// 足もとの丸影（MultiplyBlending + premultipliedAlpha・霧なし）と輪（加算・霧あり）
struct FxOut { @builtin(position) clip: vec4f, @location(0) uv: vec2f, @location(1) fx: vec4f, @location(2) depth: f32 };
@vertex fn vs_fx(v: VIn, inst: IIn) -> FxOut {
  let w = (inst_model(inst) * vec4f(v.pos, 1.0)).xyz;
  var o: FxOut;
  o.clip = g.view_proj * vec4f(w, 1.0);
  o.uv = v.uv;
  o.fx = inst.fx;
  o.depth = view_depth(w);
  return o;
}
@fragment fn fs_blob(i: FxOut) -> @location(0) vec4f {
  let t = textureSample(map, map_smp, i.uv);
  let a = t.a * i.fx.a;
  return vec4f(i.fx.rgb * t.rgb * a, a);
}
@fragment fn fs_ring(i: FxOut) -> @location(0) vec4f {
  return vec4f(apply_fog(i.fx.rgb, i.depth), i.fx.a);
}
