// 空（夕暮れの階調・太陽・すじ雲・星）と、島の下に広がる雲の海。
struct SkyU {
  zenith: vec4f,  // w = uTime
  mid: vec4f,     // w = uStars
  horizon: vec4f, // w = uDark
  sun: vec4f,
  cloud: vec4f,
  fog: vec4f,
  sun_dir: vec4f,
  sea: vec4f,
  light: vec4f,
  cam: vec4f,
};
@group(1) @binding(5) var<uniform> u: SkyU;

struct SkyOut {
  @builtin(position) clip: vec4f,
  @location(0) dir: vec3f,
};
@vertex fn vs_sky(v: VIn) -> SkyOut {
  let p = g.view_proj * vec4f(v.pos + u.cam.xyz, 1.0);
  var o: SkyOut;
  o.dir = normalize(v.pos);
  o.clip = p.xyww;
  return o;
}

fn hash3(p0: vec3f) -> f32 {
  var p = fract(p0 * 0.3183099 + 0.1);
  p *= 17.0;
  return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}
fn noise3(x: vec3f) -> f32 {
  let i = floor(x);
  var f = fract(x);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(mix(hash3(i), hash3(i + vec3f(1, 0, 0)), f.x), mix(hash3(i + vec3f(0, 1, 0)), hash3(i + vec3f(1, 1, 0)), f.x), f.y),
             mix(mix(hash3(i + vec3f(0, 0, 1)), hash3(i + vec3f(1, 0, 1)), f.x), mix(hash3(i + vec3f(0, 1, 1)), hash3(i + vec3f(1, 1, 1)), f.x), f.y), f.z);
}
fn fbm3(p0: vec3f) -> f32 {
  var a = 0.5; var s = 0.0; var p = p0;
  for (var i = 0; i < 5; i++) { s += a * noise3(p); p *= 2.03; a *= 0.5; }
  return s;
}

@fragment fn fs_sky(o: SkyOut) -> @location(0) vec4f {
  let time = u.zenith.w; let stars = u.mid.w; let dark = u.horizon.w;
  let d = normalize(o.dir);
  let h = d.y;
  // 地平の下は雲の海の色へ
  var col = mix(u.horizon.rgb, u.mid.rgb, smoothstep(0.0, 0.32, h));
  col = mix(col, u.zenith.rgb, smoothstep(0.25, 0.85, h));
  col = mix(col, mix(u.horizon.rgb, u.mid.rgb, 0.5) * 0.8, smoothstep(0.0, 0.25, -h));
  let sd = normalize(u.sun_dir.xyz);
  let sdot = max(dot(d, sd), 0.0);
  col += u.sun.rgb * (pow(sdot, 6.0) * 0.3 + pow(sdot, 40.0) * 0.4);
  // 太陽の円盤（白飛びさせない）
  col = mix(col, vec3f(1.0, 0.95, 0.86) * 1.05, smoothstep(0.9988, 0.9995, sdot) * clamp(dot(u.sun.rgb, vec3f(0.6)), 0.0, 1.0));
  // すじ雲: 地平に近い横長の帯
  let cp = d.xz / max(d.y + 0.12, 0.05);
  let cl = fbm3(vec3f(cp * vec2f(0.55, 1.6) + vec2f(time * 0.006, 0.0), time * 0.002));
  let band = smoothstep(0.0, 0.08, h + 0.02) * (1.0 - smoothstep(0.22, 0.55, h));
  let cloud = smoothstep(0.52, 0.78, cl) * band;
  let cc = mix(u.cloud.rgb, u.sun.rgb * 1.1, pow(sdot, 3.0) * 0.8);
  col = mix(col, cc, cloud * 0.75);
  // 星
  if (stars > 0.0) {
    let sp = d * 220.0;
    let st = step(0.9975, hash3(floor(sp))) * smoothstep(0.05, 0.4, h);
    let tw = 0.6 + 0.4 * sin(time * 2.0 + hash3(floor(sp) + 3.1) * 40.0);
    col += vec3f(0.9, 0.92, 1.0) * st * tw * stars;
  }
  col *= 1.0 - dark * 0.55;
  // 地平のかすみ: 雲の海の遠くの霧の色とつなげる（空と海の境目が一直線に切れないように）
  let haze = (1.0 - smoothstep(0.0, 0.09, h)) * 0.8 + (1.0 - smoothstep(-0.05, 0.0, h)) * 0.2;
  col = mix(col, u.fog.rgb, clamp(haze, 0.0, 1.0));
  return vec4f(col, 1.0);
}

// ---- 雲の海 ----
struct SeaOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
};
@vertex fn vs_sea(v: VIn) -> SeaOut {
  var o: SeaOut;
  o.world = v.pos;
  o.clip = g.view_proj * vec4f(v.pos, 1.0);
  return o;
}
fn hash2(p: vec2f) -> f32 { return fract(sin(dot(p, vec2f(127.1, 311.7))) * 43758.5453); }
fn noise2(p: vec2f) -> f32 {
  let i = floor(p);
  var f = fract(p);
  f = f * f * (3.0 - 2.0 * f);
  return mix(mix(hash2(i), hash2(i + vec2f(1, 0)), f.x), mix(hash2(i + vec2f(0, 1)), hash2(i + vec2f(1, 1)), f.x), f.y);
}
fn fbm2(p0: vec2f) -> f32 {
  var a = 0.5; var s = 0.0; var p = p0;
  for (var i = 0; i < 5; i++) { s += a * noise2(p); p *= 2.07; a *= 0.5; }
  return s;
}
@fragment fn fs_sea(o: SeaOut) -> @location(0) vec4f {
  let time = u.zenith.w;
  let p = o.world.xz * 0.025 + vec2f(time * 0.01, time * 0.004);
  let n = fbm2(p) * 0.7 + fbm2(p * 2.7 - vec2f(time * 0.01)) * 0.3;
  let puff = smoothstep(0.35, 0.8, n);
  var col = mix(u.sea.rgb * 0.72, u.light.rgb, puff * 0.85);
  // 太陽の方向は明るく
  let dir = normalize(vec3f(o.world.x, 0.0, o.world.z) + vec3f(1e-4, 0.0, 0.0));
  let sdot = max(dot(dir, normalize(vec3f(u.sun_dir.x, 0.0, u.sun_dir.z))), 0.0);
  col += u.sun.rgb * pow(sdot, 4.0) * 0.25 * puff;
  return vec4f(apply_fog(col, view_depth(o.world)), 1.0);
}
