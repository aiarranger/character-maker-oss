// 環境の映り込みを作る（three の RoomEnvironment を PMREMGenerator の CubeUV の絵へ描く）。render/env.rs が使う。式は PMREMGenerator.js と cube_uv_reflection_fragment の写し。
// 絵の座標は three と同じ（v は下が 0）。的は上が 0 なので、読むときに v を反転し、描くときは viewport の y を反転する。

// ---- 部屋を描く（MeshStandardMaterial の roughness 1・metalness 0 と点の光 1 つ。MeshBasicMaterial の光る板）----
struct SceneU { vp: mat4x4f, model: mat4x4f, nm0: vec4f, nm1: vec4f, nm2: vec4f, color: vec4f };  // color.w = 1 なら Basic
@group(0) @binding(0) var<uniform> su: SceneU;
struct SOut { @builtin(position) clip: vec4f, @location(0) world: vec3f, @location(1) n: vec3f };
@vertex fn vs_scene(@location(0) pos: vec3f, @location(1) normal: vec3f) -> SOut {
  let w = su.model * vec4f(pos, 1.0);
  var o: SOut;
  o.clip = su.vp * w;
  o.world = w.xyz;
  o.n = mat3x3f(su.nm0.xyz, su.nm1.xyz, su.nm2.xyz) * normal;
  return o;
}
const RECIPROCAL_PI: f32 = 0.3183098861837907;
const LIGHT_POS: vec3f = vec3f(0.418, 16.199, 0.300);
@fragment fn fs_scene(i: SOut) -> @location(0) vec4f {
  if (su.color.w > 0.5) { return vec4f(su.color.rgb, 1.0); }
  // RE_Direct_Physical（カメラは原点。roughness 1 なので alpha 1）
  let n = normalize(i.n);
  let v = normalize(-i.world);
  let lv = LIGHT_POS - i.world;
  let d = length(lv);
  let l = normalize(lv);
  var att = 1.0 / max(pow(d, 2.0), 0.01);
  let q = d / 28.0;
  let c = clamp(1.0 - q * q * q * q, 0.0, 1.0);
  att *= c * c;
  let dot_nl = clamp(dot(n, l), 0.0, 1.0);
  let irr = dot_nl * 900.0 * att;
  let h = normalize(l + v);
  let dot_nv = clamp(dot(n, v), 0.0, 1.0);
  let dot_vh = clamp(dot(v, h), 0.0, 1.0);
  let fres = pow(1.0 - dot_vh, 5.0);
  let f = 0.04 * (1.0 - fres) + fres;
  // V_GGX_SmithCorrelated と D_GGX（alpha 1）
  let vis = 0.5 / max(dot_nl + dot_nv, 1e-6);
  let dd = RECIPROCAL_PI;
  return vec4f(vec3f(irr * RECIPROCAL_PI + irr * f * (vis * dd)), 1.0);
}

// ---- ぼかし（PMREMGenerator の SphericalGaussianBlur）----
struct BlurU { weights: array<vec4f, 5>, pole: vec4f, p: vec4f, lod: vec4f };  // p: samples, latitudinal, dTheta, mipInt。lod.x: texelSize
@group(0) @binding(0) var<uniform> bu: BlurU;
@group(0) @binding(1) var env_map: texture_2d<f32>;
@group(0) @binding(2) var env_smp: sampler;
struct BOut { @builtin(position) clip: vec4f, @location(0) dir: vec3f };
fn get_direction(uv0: vec2f, face: f32) -> vec3f {
  let uv = 2.0 * uv0 - 1.0;
  let d = vec3f(uv, 1.0);
  if (face == 0.0) { return d.zyx; }
  if (face == 1.0) { let e = d.xzy; return vec3f(-e.x, e.y, -e.z); }
  if (face == 2.0) { return vec3f(-d.x, d.y, d.z); }
  if (face == 3.0) { let e = d.zyx; return vec3f(-e.x, e.y, -e.z); }
  if (face == 4.0) { let e = d.xzy; return vec3f(-e.x, -e.y, e.z); }
  return vec3f(d.x, d.y, -d.z);
}
@vertex fn vs_blur(@builtin(vertex_index) vid: u32) -> BOut {
  var corners = array<vec2f, 6>(vec2f(0.0, 0.0), vec2f(1.0, 0.0), vec2f(1.0, 1.0), vec2f(0.0, 0.0), vec2f(1.0, 1.0), vec2f(0.0, 1.0));
  let face = vid / 6u;
  let c = corners[vid % 6u];
  let x = f32(face % 3u) * 2.0 / 3.0 - 1.0;
  let y = select(-1.0, 0.0, face > 2u);
  let t = bu.lod.x;
  let uv = mix(vec2f(-t), vec2f(1.0 + t), c);
  var o: BOut;
  o.clip = vec4f(x + c.x * 2.0 / 3.0, y + c.y, 0.0, 1.0);
  o.dir = get_direction(uv, f32(face));
  return o;
}
const CUBEUV_MAX_MIP: f32 = 8.0;
const CUBEUV_TEXEL_W: f32 = 1.0 / 768.0;
const CUBEUV_TEXEL_H: f32 = 1.0 / 1024.0;
fn cube_face(dir: vec3f) -> f32 {
  let a = abs(dir);
  if (a.x > a.z) {
    if (a.x > a.y) { return select(3.0, 0.0, dir.x > 0.0); }
    return select(4.0, 1.0, dir.y > 0.0);
  }
  if (a.z > a.y) { return select(5.0, 2.0, dir.z > 0.0); }
  return select(4.0, 1.0, dir.y > 0.0);
}
fn cube_uv(dir: vec3f, face: f32) -> vec2f {
  var uv: vec2f;
  if (face == 0.0) { uv = vec2f(dir.z, dir.y) / abs(dir.x); }
  else if (face == 1.0) { uv = vec2f(-dir.x, -dir.z) / abs(dir.y); }
  else if (face == 2.0) { uv = vec2f(-dir.x, dir.y) / abs(dir.z); }
  else if (face == 3.0) { uv = vec2f(-dir.z, dir.y) / abs(dir.x); }
  else if (face == 4.0) { uv = vec2f(-dir.x, dir.z) / abs(dir.y); }
  else { uv = vec2f(dir.x, dir.y) / abs(dir.z); }
  return 0.5 * (uv + 1.0);
}
fn bilinear_cube_uv(dir: vec3f, mip0: f32) -> vec3f {
  var face = cube_face(dir);
  let filter_int = max(4.0 - mip0, 0.0);
  let mip = max(mip0, 4.0);
  let face_size = exp2(mip);
  var uv = cube_uv(dir, face) * (face_size - 2.0) + 1.0;
  if (face > 2.0) { uv.y += face_size; face -= 3.0; }
  uv.x += face * face_size;
  uv.x += filter_int * 3.0 * 16.0;
  uv.y += 4.0 * (exp2(CUBEUV_MAX_MIP) - face_size);
  uv.x *= CUBEUV_TEXEL_W;
  uv.y *= CUBEUV_TEXEL_H;
  return textureSampleLevel(env_map, env_smp, vec2f(uv.x, 1.0 - uv.y), 0.0).rgb;
}
fn get_sample(dir: vec3f, theta: f32, axis: vec3f) -> vec3f {
  let c = cos(theta);
  let s = sin(theta);
  let d = dir * c + cross(axis, dir) * s + axis * dot(axis, dir) * (1.0 - c);
  return bilinear_cube_uv(d, bu.p.w);
}
@fragment fn fs_blur(i: BOut) -> @location(0) vec4f {
  var axis = select(cross(bu.pole.xyz, i.dir), bu.pole.xyz, bu.p.y > 0.5);
  if (all(axis == vec3f(0.0))) { axis = vec3f(i.dir.z, 0.0, -i.dir.x); }
  axis = normalize(axis);
  var col = bu.weights[0][0] * get_sample(i.dir, 0.0, axis);
  let samples = i32(bu.p.x);
  for (var k = 1; k < 20; k++) {
    if (k >= samples) { break; }
    let theta = bu.p.z * f32(k);
    let w = bu.weights[k / 4][k % 4];
    col += w * get_sample(i.dir, -1.0 * theta, axis);
    col += w * get_sample(i.dir, theta, axis);
  }
  return vec4f(col, 1.0);
}
