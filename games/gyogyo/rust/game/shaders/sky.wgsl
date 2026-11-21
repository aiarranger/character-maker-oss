// 空（sky.ts の SKY_VERT・SKY_FRAG）
struct SkyU { zenith: vec4f, mid: vec4f, horizon: vec4f, sun: vec4f, cloud: vec4f, fog: vec4f, sun_dir: vec4f, p: vec4f };  // p: uTime, uStars, uDark
@group(2) @binding(0) var<uniform> u: SkyU;
struct SkyOut { @builtin(position) clip: vec4f, @location(0) dir: vec3f };

@vertex fn vs(v: VIn, i: IIn) -> SkyOut {
  var o: SkyOut;
  o.dir = normalize(v.pos);
  let p = g.view_proj * inst_model(i) * vec4f(v.pos, 1.0);
  o.clip = p.xyww;
  return o;
}
fn hash3(p0: vec3f) -> f32 {
  var p = fract(p0 * 0.3183099 + vec3f(0.1));
  p *= 17.0;
  return fract(p.x * p.y * p.z * (p.x + p.y + p.z));
}
fn noise3(x: vec3f) -> f32 {
  let i = floor(x);
  var f = fract(x);
  f = f * f * (vec3f(3.0) - 2.0 * f);
  return mix(mix(mix(hash3(i), hash3(i + vec3f(1.0, 0.0, 0.0)), f.x), mix(hash3(i + vec3f(0.0, 1.0, 0.0)), hash3(i + vec3f(1.0, 1.0, 0.0)), f.x), f.y),
             mix(mix(hash3(i + vec3f(0.0, 0.0, 1.0)), hash3(i + vec3f(1.0, 0.0, 1.0)), f.x), mix(hash3(i + vec3f(0.0, 1.0, 1.0)), hash3(i + vec3f(1.0, 1.0, 1.0)), f.x), f.y), f.z);
}
fn fbm3(p0: vec3f) -> f32 {
  var p = p0;
  var a = 0.5;
  var s = 0.0;
  for (var k = 0; k < 5; k++) { s += a * noise3(p); p *= 2.03; a *= 0.5; }
  return s;
}
@fragment fn fs(i: SkyOut) -> @location(0) vec4f {
  let t = u.p.x;
  let d = normalize(i.dir);
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
  let cl = fbm3(vec3f(cp * vec2f(0.55, 1.6) + vec2f(t * 0.006, 0.0), t * 0.002));
  let band = smoothstep(0.0, 0.08, h + 0.02) * (1.0 - smoothstep(0.22, 0.55, h));
  let cloud = smoothstep(0.52, 0.78, cl) * band;
  let cc = mix(u.cloud.rgb, u.sun.rgb * 1.1, pow(sdot, 3.0) * 0.8);
  col = mix(col, cc, cloud * 0.75);
  // 星
  if (u.p.y > 0.0) {
    let sp = d * 220.0;
    let st = step(0.9975, hash3(floor(sp))) * smoothstep(0.05, 0.4, h);
    let tw = 0.6 + 0.4 * sin(t * 2.0 + hash3(floor(sp) + vec3f(3.1)) * 40.0);
    col += vec3f(0.9, 0.92, 1.0) * st * tw * u.p.y;
  }
  col *= 1.0 - u.p.z * 0.55;
  // 地平のかすみ: 雲の海の遠くの霧の色とつなげる（空と海の境目が一直線に切れないように）
  let haze = (1.0 - smoothstep(0.0, 0.09, h)) * 0.8 + (1.0 - smoothstep(-0.05, 0.0, h)) * 0.2;
  col = mix(col, u.fog.rgb, clamp(haze, 0.0, 1.0));

  return vec4f(col, 1.0);
}
