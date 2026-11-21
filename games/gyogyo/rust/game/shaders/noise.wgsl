// course.ts の NOISE
fn t_hash(p: vec2f) -> f32 { return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453); }
fn t_noise(p: vec2f) -> f32 {
  let i = floor(p);
  var f = fract(p);
  f = f * f * (vec2f(3.0) - 2.0 * f);
  return mix(mix(t_hash(i), t_hash(i + vec2f(1.0, 0.0)), f.x), mix(t_hash(i + vec2f(0.0, 1.0)), t_hash(i + vec2f(1.0, 1.0)), f.x), f.y);
}
fn t_fbm(p0: vec2f) -> f32 {
  var p = p0;
  var a = 0.5;
  var s = 0.0;
  for (var k = 0; k < 4; k++) { s += a * t_noise(p); p *= 2.03; a *= 0.5; }
  return s;
}
