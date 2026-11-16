// 大将戦の飾り: 灯籠・足元の灯・光の筋・手の光。どれも加算で描き、ブルームで光らせる。
// a.rgb = 色、a.w = 強さ / b.x = 種類（1 光の玉・2 灯籠・3 光の筋・4 地面の灯）/ b.y・b.z・b.w = 種類ごとの値 / c.xy = 光の筋の傾き
struct BIn {
  @location(8) m0: vec4f,
  @location(9) m1: vec4f,
  @location(10) m2: vec4f,
  @location(11) m3: vec4f,
  @location(12) a: vec4f,
  @location(13) b: vec4f,
  @location(14) c: vec4f,
  @location(15) d: vec4f,
};
struct BOut {
  @builtin(position) pos: vec4f,
  @location(0) uv: vec2f,
  @location(1) @interpolate(flat) a: vec4f,
  @location(2) @interpolate(flat) b: vec4f,
  @location(3) depth: f32,
};

@vertex fn vs_bfx(v: VIn, i: BIn) -> BOut {
  let kind = i.b.x;
  var w: vec3f;
  if (kind > 0.5 && kind < 3.5) {
    // カメラへ向く板。大きさは行列の x・y の長さ
    let c = i.m3.xyz;
    let sx = length(i.m0.xyz);
    let sy = length(i.m1.xyz);
    var r = vec3f(g.view[0][0], g.view[1][0], g.view[2][0]);
    let u = vec3f(g.view[0][1], g.view[1][1], g.view[2][1]);
    if (kind > 2.5) {
      // 光の筋: 縦は世界の上、根元が c。上へ行くほど c.xy の向きへ傾く
      r = normalize(vec3f(r.x, 0.0, r.z) + vec3f(0.0001, 0.0, 0.0));
      let h = v.pos.y + 0.5;
      w = c + r * v.pos.x * sx + vec3f(0.0, 1.0, 0.0) * h * sy + vec3f(i.c.x, 0.0, i.c.y) * h;
    } else {
      w = c + r * v.pos.x * sx + u * v.pos.y * sy;
    }
  } else {
    w = (mat4x4f(i.m0, i.m1, i.m2, i.m3) * vec4f(v.pos, 1.0)).xyz;
  }
  var o: BOut;
  o.pos = g.view_proj * vec4f(w, 1.0);
  o.uv = v.uv;
  o.a = i.a;
  o.b = i.b;
  o.depth = view_depth(w);
  return o;
}

@fragment fn fs_bfx(i: BOut) -> @location(0) vec4f {
  let kind = i.b.x;
  let p = i.uv * 2.0 - 1.0;
  let r = length(p);
  var a = 0.0;
  var c = i.a.rgb;
  if (kind < 1.5) {
    a = pow(max(1.0 - r, 0.0), 2.2);
  } else if (kind < 2.5) {
    // 灯籠: 角の丸い紙の胴に骨の縞、上下に暗い枠
    let q = abs(p) - vec2f(0.4, 0.58);
    let d = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - 0.22;
    let body = 1.0 - smoothstep(-0.04, 0.04, d);
    let ribs = 0.82 + 0.18 * cos(p.y * 20.0);
    let cap = 1.0 - smoothstep(0.62, 0.7, abs(p.y)) * 0.75;
    a = body * ribs * cap * (0.55 + 0.6 * max(1.0 - r, 0.0));
    c = c * mix(1.0, 1.5, max(1.0 - r * 1.4, 0.0));
  } else if (kind < 3.5) {
    let e = max(1.0 - abs(p.x), 0.0);
    let band = 0.75 + 0.25 * sin(p.x * 9.0 + i.b.y * 5.0 + i.b.z * 0.25);
    a = e * e * band * smoothstep(0.0, 0.2, i.uv.y) * (1.0 - smoothstep(0.45, 1.0, i.uv.y));
  } else {
    a = pow(max(1.0 - r, 0.0), 1.6);
  }
  let fade = 1.0 - fog_factor(i.depth) * 0.85;
  return vec4f(c * a * i.a.w * fade, 1.0);
}
