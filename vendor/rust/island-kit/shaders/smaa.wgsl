// SMAA（kit.set_aa(Aa::Smaa)）: 縁を見る → 混ぜる重み → 混ぜる。仕上げ（fs_final）のあとの画にかける。
// postprocessing 6.39.2 の SMAAEffect（Zlib。edge-detection・smaa-weights・smaa の各シェーダー）を WGSL へ写した物（変更あり: WGSL の書き方、
// 頂点で作る値を画素で作る）。SMAA は Jorge Jimenez ほか（MIT）、area と search の表（smaa_area.rg8・smaa_search.r8）は postprocessing の
// 画像をそのまま 8 ビットの値にした物。設定は aisns の three の版（lib/color-smaa-effect.ts）と同じ HIGH・色で縁を見る・予測なし。
// uv は three と同じ下が 0（post.wgsl と同じ）。描いた的は tex_uv で読む（three の WebGL の的と同じ並びで計算する）。
// 入力の画（ldr）は sRGB の値の 8 ビット。three の版は線形の半精度の的にかけるので、sRGB の見方（読むと線形）で読み、出すときに sRGB へ戻す。
struct VOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f };

@vertex fn vs_full(@builtin(vertex_index) i: u32) -> VOut {
  let p = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  var o: VOut;
  o.pos = vec4f(p[i], 0.0, 1.0);
  o.uv = vec2f((p[i].x + 1.0) * 0.5, (p[i].y + 1.0) * 0.5);
  return o;
}
fn tex_uv(uv: vec2f) -> vec2f { return vec2f(uv.x, 1.0 - uv.y); }

// 入力の的（縁を見る・混ぜる: ldr。重み: 縁）と線形の補間
@group(0) @binding(0) var t0: texture_2d<f32>;
@group(0) @binding(1) var s0: sampler;
// 重みの表（three: area は flipY = false・線形の補間、search は flipY = true・最も近い画素）
@group(1) @binding(0) var area_tex: texture_2d<f32>;
@group(1) @binding(1) var search_tex: texture_2d<f32>;
@group(1) @binding(2) var s_near: sampler;
// 混ぜる: 重みの的
@group(1) @binding(3) var weight_tex: texture_2d<f32>;

fn rt0(uv: vec2f) -> vec4f { return textureSampleLevel(t0, s0, tex_uv(uv), 0.0); }
fn texel() -> vec2f { return 1.0 / vec2f(textureDimensions(t0)); }

// ---- 縁を見る（EdgeDetectionMaterial の EDGE_DETECTION_MODE 2 = 色） ----
const EDGE_THRESHOLD: f32 = 0.1;
const LOCAL_CONTRAST_ADAPTATION_FACTOR: f32 = 2.0;
fn max3(t: vec3f) -> f32 { return max(max(t.r, t.g), t.b); }

@fragment fn fs_edges(i: VOut) -> @location(0) vec4f {
  let ts = texel();
  let uv = i.uv;
  let c = rt0(uv).rgb;
  var delta: vec4f;
  delta.x = max3(abs(c - rt0(uv + ts * vec2f(-1.0, 0.0)).rgb));
  delta.y = max3(abs(c - rt0(uv + ts * vec2f(0.0, -1.0)).rgb));
  var edges = step(vec2f(EDGE_THRESHOLD), delta.xy);
  if (dot(edges, vec2f(1.0)) == 0.0) { discard; }
  delta.z = max3(abs(c - rt0(uv + ts * vec2f(1.0, 0.0)).rgb));
  delta.w = max3(abs(c - rt0(uv + ts * vec2f(0.0, 1.0)).rgb));
  var max_delta = max(delta.xy, delta.zw);
  delta.z = max3(abs(c - rt0(uv + ts * vec2f(-2.0, 0.0)).rgb));
  delta.w = max3(abs(c - rt0(uv + ts * vec2f(0.0, -2.0)).rgb));
  max_delta = max(max_delta.xy, delta.zw);
  let final_delta = max(max_delta.x, max_delta.y);
  edges *= step(vec2f(final_delta), LOCAL_CONTRAST_ADAPTATION_FACTOR * delta.xy);
  return vec4f(edges, 0.0, 1.0);
}

// ---- 混ぜる重み（SMAAWeightsMaterial の HIGH） ----
const MAX_SEARCH_STEPS_INT: i32 = 16;
const MAX_SEARCH_STEPS_FLOAT: f32 = 16.0;
const MAX_SEARCH_STEPS_DIAG_INT: i32 = 8;
const CORNER_ROUNDING_NORM: f32 = 0.25;
const AREATEX_MAX_DISTANCE: f32 = 16.0;
const AREATEX_MAX_DISTANCE_DIAG: f32 = 20.0;
const AREATEX_PIXEL_SIZE: vec2f = vec2f(1.0 / 160.0, 1.0 / 560.0);
const AREATEX_SUBTEX_SIZE: f32 = 1.0 / 7.0;
const SEARCHTEX_SIZE: vec2f = vec2f(66.0, 33.0);
const SEARCHTEX_PACKED_SIZE: vec2f = vec2f(64.0, 16.0);

fn off(coord: vec2f, o: vec2f) -> vec4f { return rt0(coord + o * texel()); }
fn area_at(uv: vec2f) -> vec2f { return textureSampleLevel(area_tex, s0, uv, 0.0).rg; }
fn search_at(uv: vec2f) -> f32 { return textureSampleLevel(search_tex, s_near, tex_uv(uv), 0.0).r; }

fn decode_diag2(e0: vec2f) -> vec2f { var e = e0; e.r = e.r * abs(5.0 * e.r - 5.0 * 0.75); return round(e); }
fn decode_diag4(e0: vec4f) -> vec4f {
  var e = e0;
  e.r = e.r * abs(5.0 * e.r - 5.0 * 0.75);
  e.b = e.b * abs(5.0 * e.b - 5.0 * 0.75);
  return round(e);
}

fn search_diag1(tex_coord: vec2f, dir: vec2f, e: ptr<function, vec2f>) -> vec2f {
  var coord = vec4f(tex_coord, -1.0, 1.0);
  let t = vec3f(texel(), 1.0);
  for (var i = 0; i < MAX_SEARCH_STEPS_INT; i++) {
    if (!(coord.z < f32(MAX_SEARCH_STEPS_DIAG_INT - 1) && coord.w > 0.9)) { break; }
    let n = t * vec3f(dir, 1.0) + coord.xyz;
    coord = vec4f(n, coord.w);
    *e = rt0(coord.xy).rg;
    coord.w = dot(*e, vec2f(0.5));
  }
  return coord.zw;
}
fn search_diag2(tex_coord: vec2f, dir: vec2f, e: ptr<function, vec2f>) -> vec2f {
  var coord = vec4f(tex_coord, -1.0, 1.0);
  coord.x += 0.25 * texel().x;
  let t = vec3f(texel(), 1.0);
  for (var i = 0; i < MAX_SEARCH_STEPS_INT; i++) {
    if (!(coord.z < f32(MAX_SEARCH_STEPS_DIAG_INT - 1) && coord.w > 0.9)) { break; }
    let n = t * vec3f(dir, 1.0) + coord.xyz;
    coord = vec4f(n, coord.w);
    *e = decode_diag2(rt0(coord.xy).rg);
    coord.w = dot(*e, vec2f(0.5));
  }
  return coord.zw;
}
fn area_diag(dist: vec2f, e: vec2f, offset: f32) -> vec2f {
  var tc = vec2f(AREATEX_MAX_DISTANCE_DIAG, AREATEX_MAX_DISTANCE_DIAG) * e + dist;
  tc = AREATEX_PIXEL_SIZE * tc + 0.5 * AREATEX_PIXEL_SIZE;
  tc.x += 0.5;
  tc.y += AREATEX_SUBTEX_SIZE * offset;
  return area_at(tc);
}
fn calculate_diag_weights(tex_coord: vec2f, e: vec2f, ssi: vec4f) -> vec2f {
  let ts = texel();
  var weights = vec2f(0.0);
  var d = vec4f(0.0);
  var end = vec2f(0.0);
  if (e.r > 0.0) {
    let r = search_diag1(tex_coord, vec2f(-1.0, 1.0), &end);
    d.x = r.x; d.z = r.y;
    d.x += f32(end.y > 0.9);
  } else { d.x = 0.0; d.z = 0.0; }
  let r1 = search_diag1(tex_coord, vec2f(1.0, -1.0), &end);
  d.y = r1.x; d.w = r1.y;
  if (d.x + d.y > 2.0) {
    let coords = vec4f(-d.x + 0.25, d.x, d.y, -d.y - 0.25) * ts.xyxy + tex_coord.xyxy;
    let a = off(coords.xy, vec2f(-1.0, 0.0)).rg;
    let b = off(coords.zw, vec2f(1.0, 0.0)).rg;
    // c.yxwz = decode(c.xyzw)
    let dc = decode_diag4(vec4f(a, b));
    let c = vec4f(dc.y, dc.x, dc.w, dc.z);
    var cc = vec2f(2.0) * c.xz + c.yw;
    if (d.z >= 0.9) { cc.x = 0.0; }
    if (d.w >= 0.9) { cc.y = 0.0; }
    weights += area_diag(d.xy, cc, ssi.z);
  }
  let r2 = search_diag2(tex_coord, vec2f(-1.0, -1.0), &end);
  d.x = r2.x; d.z = r2.y;
  if (off(tex_coord, vec2f(1.0, 0.0)).r > 0.0) {
    let r3 = search_diag2(tex_coord, vec2f(1.0), &end);
    d.y = r3.x; d.w = r3.y;
    d.y += f32(end.y > 0.9);
  } else { d.y = 0.0; d.w = 0.0; }
  if (d.x + d.y > 2.0) {
    let coords = vec4f(-d.x, -d.x, d.y, d.y) * ts.xyxy + tex_coord.xyxy;
    var c: vec4f;
    c.x = off(coords.xy, vec2f(-1.0, 0.0)).g;
    c.y = off(coords.xy, vec2f(0.0, -1.0)).r;
    let gr = off(coords.zw, vec2f(1.0, 0.0)).gr;
    c.z = gr.x; c.w = gr.y;
    var cc = vec2f(2.0) * c.xz + c.yw;
    if (d.z >= 0.9) { cc.x = 0.0; }
    if (d.w >= 0.9) { cc.y = 0.0; }
    weights += area_diag(d.xy, cc, ssi.w).gr;
  }
  return weights;
}

fn search_length(e: vec2f, offset: f32) -> f32 {
  var scale = SEARCHTEX_SIZE * vec2f(0.5, -1.0);
  var bias = SEARCHTEX_SIZE * vec2f(offset, 1.0);
  scale += vec2f(-1.0, 1.0);
  bias += vec2f(0.5, -0.5);
  scale *= 1.0 / SEARCHTEX_PACKED_SIZE;
  bias *= 1.0 / SEARCHTEX_PACKED_SIZE;
  return search_at(scale * e + bias);
}
fn search_x_left(tc0: vec2f, end: f32) -> f32 {
  let ts = texel();
  var tc = tc0;
  var e = vec2f(0.0, 1.0);
  for (var i = 0; i < MAX_SEARCH_STEPS_INT; i++) {
    if (!(tc.x > end && e.g > 0.8281 && e.r == 0.0)) { break; }
    e = rt0(tc).rg;
    tc = vec2f(-2.0, 0.0) * ts + tc;
  }
  let offset = -(255.0 / 127.0) * search_length(e, 0.0) + 3.25;
  return ts.x * offset + tc.x;
}
fn search_x_right(tc0: vec2f, end: f32) -> f32 {
  let ts = texel();
  var tc = tc0;
  var e = vec2f(0.0, 1.0);
  for (var i = 0; i < MAX_SEARCH_STEPS_INT; i++) {
    if (!(tc.x < end && e.g > 0.8281 && e.r == 0.0)) { break; }
    e = rt0(tc).rg;
    tc = vec2f(2.0, 0.0) * ts + tc;
  }
  let offset = -(255.0 / 127.0) * search_length(e, 0.5) + 3.25;
  return -ts.x * offset + tc.x;
}
fn search_y_up(tc0: vec2f, end: f32) -> f32 {
  let ts = texel();
  var tc = tc0;
  var e = vec2f(1.0, 0.0);
  for (var i = 0; i < MAX_SEARCH_STEPS_INT; i++) {
    if (!(tc.y > end && e.r > 0.8281 && e.g == 0.0)) { break; }
    e = rt0(tc).rg;
    tc = -vec2f(0.0, 2.0) * ts + tc;
  }
  let offset = -(255.0 / 127.0) * search_length(e.gr, 0.0) + 3.25;
  return ts.y * offset + tc.y;
}
fn search_y_down(tc0: vec2f, end: f32) -> f32 {
  let ts = texel();
  var tc = tc0;
  var e = vec2f(1.0, 0.0);
  for (var i = 0; i < MAX_SEARCH_STEPS_INT; i++) {
    if (!(tc.y < end && e.r > 0.8281 && e.g == 0.0)) { break; }
    e = rt0(tc).rg;
    tc = vec2f(0.0, 2.0) * ts + tc;
  }
  let offset = -(255.0 / 127.0) * search_length(e.gr, 0.5) + 3.25;
  return -ts.y * offset + tc.y;
}
fn area(dist: vec2f, e1: f32, e2: f32, offset: f32) -> vec2f {
  var tc = vec2f(AREATEX_MAX_DISTANCE) * round(4.0 * vec2f(e1, e2)) + dist;
  tc = AREATEX_PIXEL_SIZE * tc + 0.5 * AREATEX_PIXEL_SIZE;
  tc.y = AREATEX_SUBTEX_SIZE * offset + tc.y;
  return area_at(tc);
}
fn corner_h(w: vec2f, tc: vec4f, d: vec2f) -> vec2f {
  let left_right = step(d.xy, d.yx);
  var rounding = (1.0 - CORNER_ROUNDING_NORM) * left_right;
  rounding /= left_right.x + left_right.y;
  var factor = vec2f(1.0);
  factor.x -= rounding.x * off(tc.xy, vec2f(0.0, 1.0)).r;
  factor.x -= rounding.y * off(tc.zw, vec2f(1.0, 1.0)).r;
  factor.y -= rounding.x * off(tc.xy, vec2f(0.0, -2.0)).r;
  factor.y -= rounding.y * off(tc.zw, vec2f(1.0, -2.0)).r;
  return w * clamp(factor, vec2f(0.0), vec2f(1.0));
}
fn corner_v(w: vec2f, tc: vec4f, d: vec2f) -> vec2f {
  let left_right = step(d.xy, d.yx);
  var rounding = (1.0 - CORNER_ROUNDING_NORM) * left_right;
  rounding /= left_right.x + left_right.y;
  var factor = vec2f(1.0);
  factor.x -= rounding.x * off(tc.xy, vec2f(1.0, 0.0)).g;
  factor.x -= rounding.y * off(tc.zw, vec2f(1.0, 1.0)).g;
  factor.y -= rounding.x * off(tc.xy, vec2f(-2.0, 0.0)).g;
  factor.y -= rounding.y * off(tc.zw, vec2f(-2.0, 1.0)).g;
  return w * clamp(factor, vec2f(0.0), vec2f(1.0));
}

@fragment fn fs_weights(i: VOut) -> @location(0) vec4f {
  let ts = texel();
  let res = vec2f(textureDimensions(t0));
  let uv = i.uv;
  let pix = uv * res;
  let o0 = uv.xyxy + ts.xyxy * vec4f(-0.25, -0.125, 1.25, -0.125);
  let o1 = uv.xyxy + ts.xyxy * vec4f(-0.125, -0.25, -0.125, 1.25);
  let o2 = vec4f(o0.xz, o1.yw) + vec4f(-2.0, 2.0, -2.0, 2.0) * ts.xxyy * MAX_SEARCH_STEPS_FLOAT;
  var weights = vec4f(0.0);
  let ssi = vec4f(0.0);
  var e = rt0(uv).rg;
  if (e.g > 0.0) {
    let wd = calculate_diag_weights(uv, e, ssi);
    weights.r = wd.x; weights.g = wd.y;
    if (weights.r == -weights.g) {
      var coords: vec3f;
      coords.x = search_x_left(o0.xy, o2.x);
      coords.y = o1.y;
      var d: vec2f;
      d.x = coords.x;
      let e1 = rt0(coords.xy).r;
      coords.z = search_x_right(o0.zw, o2.y);
      d.y = coords.z;
      d = round(res.xx * d + -pix.xx);
      let sqrt_d = sqrt(abs(d));
      let e2 = off(coords.zy, vec2f(1.0, 0.0)).r;
      var w = area(sqrt_d, e1, e2, ssi.y);
      coords.y = uv.y;
      w = corner_h(w, coords.xyzy, d);
      weights.r = w.x; weights.g = w.y;
    } else { e.r = 0.0; }
  }
  if (e.r > 0.0) {
    var coords: vec3f;
    coords.y = search_y_up(o1.xy, o2.z);
    coords.x = o0.x;
    var d: vec2f;
    d.x = coords.y;
    let e1 = rt0(coords.xy).g;
    coords.z = search_y_down(o1.zw, o2.w);
    d.y = coords.z;
    d = round(res.yy * d - pix.yy);
    let sqrt_d = sqrt(abs(d));
    let e2 = off(coords.xz, vec2f(0.0, 1.0)).g;
    var w = area(sqrt_d, e1, e2, ssi.x);
    coords.x = uv.x;
    w = corner_v(w, coords.xyxz, d);
    weights.b = w.x; weights.a = w.y;
  }
  return weights;
}

// ---- 混ぜる（SMAAEffect の smaa.frag。出口は sRGB へ戻す） ----
fn weight_at(uv: vec2f) -> vec4f { return textureSampleLevel(weight_tex, s0, tex_uv(uv), 0.0); }
// three の colorspace_fragment（sRGBTransferOETF。final_common.wgsl と同じ）
fn to_srgb(c: vec3f) -> vec3f {
  let lo = c * 12.92;
  let hi = 1.055 * pow(c, vec3f(0.41666)) - 0.055;
  return select(hi, lo, c <= vec3f(0.0031308));
}

@fragment fn fs_blend(i: VOut) -> @location(0) vec4f {
  let ts = texel();
  let uv = i.uv;
  var a: vec4f;
  a.x = weight_at(uv + ts * vec2f(1.0, 0.0)).a;
  a.y = weight_at(uv + ts * vec2f(0.0, 1.0)).g;
  let rb = weight_at(uv).rb;
  a.w = rb.x; a.z = rb.y;
  var color = rt0(uv);
  if (dot(a, vec4f(1.0)) >= 1e-5) {
    let h = max(a.x, a.z) > max(a.y, a.w);
    var offset = vec4f(0.0, a.y, 0.0, a.w);
    var weight = a.yw;
    if (h) { offset = vec4f(a.x, 0.0, a.z, 0.0); weight = a.xz; }
    weight /= dot(weight, vec2f(1.0));
    let bc = offset * vec4f(ts, -ts) + uv.xyxy;
    color = weight.x * rt0(bc.xy) + weight.y * rt0(bc.zw);
  }
  return vec4f(to_srgb(max(color.rgb, vec3f(0.0))), color.a);
}
