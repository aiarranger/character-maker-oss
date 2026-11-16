// 画面全体の後処理: 保護（NaN を消す）→ ブルーム（UnrealBloomPass と同じ）→ 仕上げ（renderer.ts の FinalShader）。
struct VOut { @builtin(position) pos: vec4f, @location(0) uv: vec2f };

@vertex fn vs_full(@builtin(vertex_index) i: u32) -> VOut {
  let p = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
  var o: VOut;
  o.pos = vec4f(p[i], 0.0, 1.0);
  // uv は three と同じ（下が 0）
  o.uv = vec2f((p[i].x + 1.0) * 0.5, (p[i].y + 1.0) * 0.5);
  return o;
}
fn tex_uv(uv: vec2f) -> vec2f { return vec2f(uv.x, 1.0 - uv.y); }

@group(0) @binding(0) var t0: texture_2d<f32>;
@group(0) @binding(1) var s0: sampler;

// ---- 保護 ----
@fragment fn fs_sanitize(i: VOut) -> @location(0) vec4f {
  var c = textureSampleLevel(t0, s0, tex_uv(i.uv), 0.0);
  // NaN・無限大は黒に
  if (any(c != c) || any(abs(c) > vec4f(1e30))) { c = vec4f(0.0, 0.0, 0.0, 1.0); }
  return min(c, vec4f(48.0));
}

// ---- ブルーム ----
struct BloomU { dir: vec2f, inv: vec2f, threshold: f32, strength: f32, radius: f32, kernel: f32 };
@group(1) @binding(0) var<uniform> b: BloomU;

@fragment fn fs_high(i: VOut) -> @location(0) vec4f {
  let t = textureSampleLevel(t0, s0, tex_uv(i.uv), 0.0);
  let v = dot(t.xyz, vec3f(0.2126729, 0.7151522, 0.072175));
  let a = smoothstep(b.threshold, b.threshold + 0.01, v);
  return mix(vec4f(0.0), t, a);
}

@fragment fn fs_blur(i: VOut) -> @location(0) vec4f {
  let uv = tex_uv(i.uv);
  let kr = i32(b.kernel);
  let sigma = b.kernel;
  var wsum = 0.39894 / sigma;
  var sum = textureSampleLevel(t0, s0, uv, 0.0).rgb * wsum;
  for (var k = 1; k < kr; k++) {
    let x = f32(k);
    let w = 0.39894 * exp(-0.5 * x * x / (sigma * sigma)) / sigma;
    let o = vec2f(b.dir.x, -b.dir.y) * b.inv * x;
    sum += (textureSampleLevel(t0, s0, uv + o, 0.0).rgb + textureSampleLevel(t0, s0, uv - o, 0.0).rgb) * w;
    wsum += 2.0 * w;
  }
  return vec4f(sum / wsum, 1.0);
}

@group(2) @binding(0) var m1: texture_2d<f32>;
@group(2) @binding(1) var m2: texture_2d<f32>;
@group(2) @binding(2) var m3: texture_2d<f32>;
@group(2) @binding(3) var m4: texture_2d<f32>;

fn lerp_factor(f: f32) -> f32 { return mix(f, 1.2 - f, b.radius); }

@fragment fn fs_composite(i: VOut) -> @location(0) vec4f {
  let uv = tex_uv(i.uv);
  let c = lerp_factor(1.0) * textureSampleLevel(t0, s0, uv, 0.0)
    + lerp_factor(0.8) * textureSampleLevel(m1, s0, uv, 0.0)
    + lerp_factor(0.6) * textureSampleLevel(m2, s0, uv, 0.0)
    + lerp_factor(0.4) * textureSampleLevel(m3, s0, uv, 0.0)
    + lerp_factor(0.2) * textureSampleLevel(m4, s0, uv, 0.0);
  return b.strength * c;
}

// three の blendMaterial: 色 × アルファを足す（アルファは合成の値のまま）
@fragment fn fs_copy(i: VOut) -> @location(0) vec4f {
  return textureSampleLevel(t0, s0, tex_uv(i.uv), 0.0);
}

// ---- 仕上げ ----
struct FinalU {
  res: vec2f, center: vec2f,
  mask_px: vec2f, time: f32, vignette: f32,
  flash: f32, desat: f32, aberration: f32, zoom: f32,
  speed: f32, gold: f32, red: f32, outline_on: f32,
};
@group(1) @binding(0) var<uniform> u: FinalU;
@group(2) @binding(0) var mask: texture_2d<f32>;

fn hash2(p: vec2f) -> f32 { return fract(sin(dot(p, vec2f(12.9898, 78.233))) * 43758.5453); }
fn smp(uv: vec2f) -> vec3f { return textureSampleLevel(t0, s0, tex_uv(uv), 0.0).rgb; }
fn mask_at(uv: vec2f) -> f32 { return textureSampleLevel(mask, s0, tex_uv(uv), 0.0).r; }
fn to_srgb(c: vec3f) -> vec3f {
  let lo = c * 12.92;
  let hi = 1.055 * pow(c, vec3f(0.41666)) - 0.055;
  return select(hi, lo, c <= vec3f(0.0031308));
}

@fragment fn fs_final(i: VOut) -> @location(0) vec4f {
  let uv = i.uv;
  var col: vec3f;
  // 放射状のぶれ（重い一撃・乱舞・突進）: 主役の位置から外へ
  if (u.zoom > 0.002) {
    let dv = uv - u.center;
    let jit = hash2(uv * u.res);
    col = vec3f(0.0);
    for (var k = 0; k < 10; k++) {
      let f = (f32(k) + jit) / 10.0;
      let o = dv * u.zoom * f;
      col += vec3f(smp(uv - o * 1.04).r, smp(uv - o).g, smp(uv - o * 0.96).b);
    }
    col /= 10.0;
  } else {
    let d = (uv - 0.5) * u.aberration;
    col = vec3f(smp(uv + d).r, smp(uv).g, smp(uv - d).b);
  }
  // 集中線（速さ）
  if (u.speed > 0.001) {
    let dv = (uv - u.center) * vec2f(u.res.x / u.res.y, 1.0);
    let ang = atan2(dv.y, dv.x);
    let cell = floor(ang * 38.0);
    let n = hash2(vec2f(cell, floor(u.time * 24.0)));
    let lines = step(0.8, n) * smoothstep(0.42, 0.98, length(dv)) * (0.5 + 0.5 * fract(ang * 38.0));
    col = mix(col, vec3f(1.0, 0.97, 0.9), lines * u.speed * 0.38);
  }
  // 色の仕上げ
  let l = luma_(col);
  col = max(mix(vec3f(l), col, 1.16 - u.desat), vec3f(0.0));
  // やわらかいフィルミックの肩 → S 字
  col = col / (vec3f(1.0) + max(col - vec3f(0.8), vec3f(0.0)) * 0.9);
  col = clamp(col, vec3f(0.0), vec3f(1.0));
  col = mix(col, col * col * (vec3f(3.0) - 2.0 * col), 0.2);
  // 周辺減光
  let q = (uv - 0.5) * vec2f(1.15, 1.0);
  let v = 1.0 - dot(q, q) * u.vignette * 2.4;
  col *= clamp(v, 0.0, 1.0);
  // 自機の外周の縁取り
  if (u.outline_on > 0.0) {
    let inside = mask_at(uv);
    var n1 = 0.0; var n2 = 0.0;
    for (var k = 0; k < 8; k++) {
      let a = f32(k) * 0.7853982;
      let d = vec2f(cos(a), sin(a)) * u.mask_px;
      n1 = max(n1, mask_at(uv + d * 1.7));
      n2 = max(n2, mask_at(uv + d * 3.1));
    }
    let inner = clamp(n1 - inside, 0.0, 1.0);
    let outer = clamp(n2 - max(n1, inside), 0.0, 1.0);
    col = mix(col, vec3f(0.0423, 0.0137, 0.0482), outer * 0.7 * u.outline_on);
    col = mix(col, vec3f(1.0, 0.807, 0.578), inner * 0.85 * u.outline_on);
  }
  // 乱舞の金の色調・被弾の赤
  let lg = luma_(col);
  col = mix(col, vec3f(lg) * vec3f(1.25, 0.95, 0.55) + col * 0.25, u.gold * 0.55);
  col = mix(col, col * vec3f(1.25, 0.55, 0.5), u.red);
  col += vec3f(u.flash);
  // 粒子（ごく弱い）
  col += vec3f((hash2(uv * u.res + fract(u.time) * 91.7) - 0.5) * 0.012);
  return vec4f(to_srgb(max(col, vec3f(0.0))), 1.0);
}
fn luma_(c: vec3f) -> f32 { return dot(c, vec3f(0.2126, 0.7152, 0.0722)); }
