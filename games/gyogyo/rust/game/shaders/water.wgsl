// 水たまり（course.ts）: 空を映す静かな水。細かいさざ波だけ光る
struct U { horizon: vec4f, zenith: vec4f };
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
  let n = t_fbm(i.w.xz * 0.5 + vec2f(t * 0.12, t * 0.09));
  let n2 = t_noise(i.w.xz * 3.2 + vec2f(t * 0.9, -t * 0.7));
  var c = mix(u.zenith.rgb * 0.65 + vec3f(0.05, 0.3, 0.42), u.horizon.rgb * 0.6 + vec3f(0.25, 0.42, 0.5), smoothstep(0.32, 0.72, n));
  c += vec3f(1.0, 0.98, 0.9) * smoothstep(0.8, 0.9, n2) * 0.35;
  return vec4f(c, 0.96);
}
