// 足もとのスタミナの輪（app.ts）: 目を走路から離さずに残量が読める
struct U { p: vec4f };  // uK, uLow, uOut
@group(2) @binding(0) var<uniform> u: U;
struct O { @builtin(position) clip: vec4f, @location(0) p: vec3f };
@vertex fn vs(v: VIn, i: IIn) -> O {
  var o: O;
  o.p = v.pos;
  o.clip = g.view_proj * inst_model(i) * vec4f(v.pos, 1.0);
  return o;
}
@fragment fn fs(i: O) -> @location(0) vec4f {
  let a = atan2(i.p.x, -i.p.z) / 6.2831853 + 0.5;
  let on = step(a, u.p.x);
  let full = mix(vec3f(0.2, 0.86, 0.56), vec3f(1.0, 0.45, 0.2), u.p.y);
  var c = mix(vec3f(0.05, 0.14, 0.11), full, on);
  var al = mix(0.42, 0.95, on);
  if (u.p.z > 0.5) { c = vec3f(0.9, 0.3, 0.2); al = 0.45 + 0.4 * abs(sin(k_time() * 10.0)); }
  return vec4f(c, al);
}
