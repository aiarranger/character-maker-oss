// 芝: 芝刈りの縞。世界の位置で色を決める（course.ts の turfMaterial。MeshLambertMaterial の改造・頂点色あり）
struct U { a: vec4f, b: vec4f, p: vec4f };  // p.x = uStripe
@group(2) @binding(0) var<uniform> u: U;
struct O { @builtin(position) clip: vec4f, @location(0) world: vec3f, @location(1) n: vec3f, @location(2) color: vec3f, @location(3) depth: f32, @location(4) recv: f32 };
@vertex fn vs(v: VIn, i: IIn) -> O {
  let m = inst_model(i);
  let w = m * vec4f(v.pos, 1.0);
  let mv = g.view * w;
  var o: O;
  o.clip = g.proj * mv;
  o.world = w.xyz;
  o.n = normal_world(m, v.normal);
  o.color = v.color.rgb;
  o.depth = -mv.z;
  o.recv = i.fx.w;
  return o;
}
@fragment fn fs(i: O, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  let st = step(0.5, fract((i.world.x * 0.8 + i.world.z * 0.6) / u.p.x));
  let nn = t_fbm(i.world.xz * 0.05);
  let fine = t_noise(i.world.xz * 2.4);
  var c = mix(u.a.rgb, u.b.rgb, st * 0.55 + (nn - 0.5) * 0.5);
  c *= 0.94 + fine * 0.12;
  c *= i.color;
  var n = normalize(i.n);
  if (!front) { n = -n; }
  let col = lit_lambert(false, i.world, n, i.n, i.recv, c);
  return vec4f(apply_fog(col, i.depth), 1.0);
}
