// 大画面の映像の板（course.ts の live。MeshBasicMaterial）: three の的は 8 ビットで 1 に切れる。kit の的は HDR なので、貼るときに切る
struct U { color: vec4f };
@group(2) @binding(0) var<uniform> u: U;
struct O { @builtin(position) clip: vec4f, @location(0) uv: vec2f, @location(1) depth: f32 };
@vertex fn vs(v: VIn, i: IIn) -> O {
  let mv = g.view * inst_model(i) * vec4f(v.pos, 1.0);
  var o: O;
  o.clip = g.proj * mv;
  o.uv = v.uv;
  o.depth = -mv.z;
  return o;
}
@fragment fn fs(i: O) -> @location(0) vec4f {
  // 描いた的は上が v = 0（three の的は下が 0）
  let t = textureSample(tex0, smp0, vec2f(i.uv.x, 1.0 - i.uv.y)).rgb;
  return vec4f(apply_fog(u.color.rgb * clamp(t, vec3f(0.0), vec3f(1.0)), i.depth), 1.0);
}
