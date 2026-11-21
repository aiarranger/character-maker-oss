// 雲の海（sky.ts の SEA_VERT・SEA_FRAG）
struct SeaU { sea: vec4f, light: vec4f, sun: vec4f, sun_dir: vec4f, p: vec4f };  // p.x = uTime
@group(2) @binding(0) var<uniform> u: SeaU;
struct SeaOut { @builtin(position) clip: vec4f, @location(0) world: vec3f, @location(1) depth: f32 };

@vertex fn vs(v: VIn, i: IIn) -> SeaOut {
  let wp = inst_model(i) * vec4f(v.pos, 1.0);
  let mv = g.view * wp;
  var o: SeaOut;
  o.world = wp.xyz;
  o.clip = g.proj * mv;
  o.depth = -mv.z;
  return o;
}
fn hash(p: vec2f) -> f32 { return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453); }
fn noise(p: vec2f) -> f32 {
  let i = floor(p);
  var f = fract(p);
  f = f * f * (vec2f(3.0) - 2.0 * f);
  return mix(mix(hash(i), hash(i + vec2f(1.0, 0.0)), f.x), mix(hash(i + vec2f(0.0, 1.0)), hash(i + vec2f(1.0, 1.0)), f.x), f.y);
}
fn fbm(p0: vec2f) -> f32 {
  var p = p0;
  var a = 0.5;
  var s = 0.0;
  for (var k = 0; k < 5; k++) { s += a * noise(p); p *= 2.07; a *= 0.5; }
  return s;
}
@fragment fn fs(i: SeaOut) -> @location(0) vec4f {
  let t = u.p.x;
  let p = i.world.xz * 0.025 + vec2f(t * 0.01, t * 0.004);
  let n = fbm(p) * 0.7 + fbm(p * 2.7 - vec2f(t * 0.01)) * 0.3;
  let puff = smoothstep(0.35, 0.8, n);
  var col = mix(u.sea.rgb * 0.72, u.light.rgb, puff * 0.85);
  // 太陽の方向は明るく
  let dir = normalize(vec3f(i.world.x, 0.0, i.world.z) + vec3f(1e-4, 0.0, 0.0));
  let sdot = max(dot(dir, normalize(vec3f(u.sun_dir.x, 0.0, u.sun_dir.z))), 0.0);
  col += u.sun.rgb * pow(sdot, 4.0) * 0.25 * puff;
  return vec4f(apply_fog(col, i.depth), 1.0);
}
