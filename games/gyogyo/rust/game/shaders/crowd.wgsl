// 観客（course.ts の crowdMat）: お菓子のかたまりと魚。走る人が近づくと、そのあたりだけ波のように跳ねる。MeshLambertMaterial の改造。
struct U { p: vec4f, runner: vec4f };  // p.x = uCheer、p.y = 頂点色を使うか
@group(2) @binding(0) var<uniform> u: U;
struct O { @builtin(position) clip: vec4f, @location(0) world: vec3f, @location(1) n: vec3f, @location(2) color: vec3f, @location(3) uv: vec2f, @location(4) depth: f32, @location(5) recv: f32 };
@vertex fn vs(v: VIn, i: IIn) -> O {
  let m = inst_model(i);
  let io = m * vec4f(0.0, 0.0, 0.0, 1.0);
  let ph = fract(sin(dot(io.xz, vec2f(12.9898, 78.233))) * 43758.5453) * 6.283;
  // 走る人が近づくと、そのあたりだけ波のように跳ねる
  let to_r = io.xz - u.runner.xz;
  let wave = exp(-dot(to_r, to_r) / 500.0);
  let excite = clamp(u.p.x + wave * 0.9, 0.0, 1.0);
  let t = k_time();
  let hop = abs(sin(t * (3.0 + 5.0 * excite) + ph));
  var p = v.pos;
  p.y += hop * (0.06 + 0.85 * excite * (0.4 + 0.6 * fract(ph * 3.7)));
  p.x += sin(t * 2.0 + ph) * 0.05 * (v.pos.y + 0.5);
  let w = m * vec4f(p, 1.0);
  let mv = g.view * w;
  var o: O;
  o.clip = g.proj * mv;
  o.world = w.xyz;
  o.n = normal_world(m, v.normal);
  o.color = i.fx.rgb;
  if (u.p.y > 0.5) { o.color *= v.color.rgb; }
  o.uv = v.uv;
  o.depth = -mv.z;
  o.recv = i.fx.w;
  return o;
}
@fragment fn fs(i: O) -> @location(0) vec4f {
  let t = textureSample(tex0, smp0, i.uv);
  // 魚の観客: ひれは切り抜いた板（u.p.z = 1 のときアルファで抜く）
  if (u.p.z > 0.5 && t.a < 0.5) { discard; }
  let d = t.rgb * i.color;
  let col = lit_lambert(false, i.world, normalize(i.n), i.n, i.recv, d);
  return vec4f(apply_fog(col, i.depth), 1.0);
}
