// 内馬場の池（course.ts）: 夕空を映す静かな水面。遠目で色の斑にならないよう、濃淡は弱く
struct U { horizon: vec4f, mid: vec4f };
@group(2) @binding(0) var<uniform> u: U;
struct O { @builtin(position) clip: vec4f, @location(0) w: vec3f };
@vertex fn vs(v: VIn, i: IIn) -> O {
  let w = inst_model(i) * vec4f(v.pos, 1.0);
  var o: O;
  o.w = w.xyz;
  o.clip = g.view_proj * w;
  return o;
}
@fragment fn fs(i: O) -> @location(0) vec4f {
  let t = k_time();
  let n = t_fbm(i.w.xz * 0.06 + vec2f(t * 0.02, t * 0.015));
  var c = mix(vec3f(0.16, 0.42, 0.55), mix(u.mid.rgb, u.horizon.rgb, 0.5), 0.32 + (n - 0.5) * 0.3);
  c += u.horizon.rgb * smoothstep(0.86, 0.93, t_noise(i.w.xz * 1.8 + vec2f(t * 0.25, 0.0))) * 0.4;
  return vec4f(c, 1.0);
}
