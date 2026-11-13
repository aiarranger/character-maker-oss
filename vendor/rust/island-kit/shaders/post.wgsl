// 画面全体の後処理: 保護（NaN を消す）→ ブルーム（UnrealBloomPass と同じ）。仕上げはゲームが渡す（final_common.wgsl のあとにゲームの fs_final）。
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
