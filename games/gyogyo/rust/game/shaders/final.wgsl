// 仕上げ（renderer.ts の FinalShader）。前に kit の final_common.wgsl が付く。
struct FinalU {
  res: vec2f, mask_px: vec2f,
  ripple: vec4f,        // xy = 中心（UV）, z = 進み 0..1, w = 強さ
  outline: vec4f,       // rgb、w = uOutlineOn
  outline_dark: vec4f,
  blur_at: vec4f,       // 背景のぼかし: ぼかさない楕円の中心・半径（UV）
  center: vec2f, zoom: f32, speed: f32,
  time: f32, vignette: f32, warm: f32, flash: f32,
  desat: f32, cool: f32, aberration: f32, gold: f32,
  red: f32, blur: f32, dim: f32, _p: f32,
};
@group(1) @binding(0) var<uniform> u: FinalU;

@fragment fn fs_final(i: VOut) -> @location(0) vec4f {
  // 12 点のポアソン円盤
  var pd = array<vec2f, 12>(
    vec2f(-0.326, -0.406), vec2f(-0.840, -0.074), vec2f(-0.696, 0.457), vec2f(-0.203, 0.621),
    vec2f(0.962, -0.195), vec2f(0.473, -0.480), vec2f(0.519, 0.767), vec2f(0.185, -0.893),
    vec2f(0.507, 0.064), vec2f(0.896, 0.412), vec2f(-0.322, -0.933), vec2f(-0.792, -0.598));
  var uv = i.uv;
  // 大きな連鎖の波紋（しずくが落ちたように画面がゆらぐ）
  if (u.ripple.w > 0.0 && u.ripple.z < 1.0) {
    let asp = vec2f(u.res.x / u.res.y, 1.0);
    let dv = (uv - u.ripple.xy) * asp;
    let d = length(dv);
    let front = u.ripple.z * 0.55;
    let rk = (d - front) * 9.0;
    let wave = sin((d - front) * 46.0) * exp(-rk * rk) * (1.0 - u.ripple.z);
    uv += normalize(dv + vec2f(1e-5)) / asp * wave * u.ripple.w * 0.012;
  }
  var col: vec3f;
  // 放射状のぶれ（重い一撃・乱舞・突進）: 主役の位置から外へ
  if (u.zoom > 0.002) {
    let dv = uv - u.center;
    let jit = hash2(uv * u.res);
    col = vec3f(0.0);
    for (var k = 0; k < 10; k++) {
      let kk = (f32(k) + jit) / 10.0;
      let o = dv * u.zoom * kk;
      col += vec3f(smp(uv - o * 1.04).r, smp(uv - o).g, smp(uv - o * 0.96).b);
    }
    col /= 10.0;
  } else {
    let d = (uv - vec2f(0.5)) * u.aberration;
    col = vec3f(smp(uv + d).r, smp(uv).g, smp(uv - d).b);
  }
  // 背景のぼかしと沈み: 主役のまわりの楕円だけ残す
  var outside = 0.0;
  if (u.blur > 0.0) {
    let fd = (uv - u.blur_at.xy) / u.blur_at.zw;
    outside = smoothstep(0.75, 1.25, length(fd));
    let rad = outside * u.blur;
    if (rad > 0.001) {
      var sum = col;
      let ang = hash2(uv * u.res) * 6.283;
      let rot = mat2x2f(cos(ang), -sin(ang), sin(ang), cos(ang));
      for (var k = 0; k < 12; k++) { sum += smp(uv + rot * pd[k] * rad * vec2f(u.res.y / u.res.x, 1.0)).rgb; }
      col = sum / 13.0;
    }
  }
  // 集中線（速さ）
  if (u.speed > 0.001) {
    let dv = (uv - u.center) * vec2f(u.res.x / u.res.y, 1.0);
    let ang = atan2(dv.y, dv.x);
    // 細く、画面の端だけ短く流す
    let cell = floor(ang * 120.0);
    let n = hash2(vec2f(cell, floor(u.time * 20.0)));
    let across = abs(fract(ang * 120.0) - 0.5) * 2.0;
    let lines = step(0.86, n) * smoothstep(0.72, 1.05, length(dv)) * (1.0 - smoothstep(0.1, 0.6, across));
    col = mix(col, vec3f(1.0, 0.98, 0.92), lines * u.speed * 0.8);
  }
  // 色の仕上げ: 少し鮮やかに・締まりを出し、影をわずかに紫へ、明るい所を杏色へ
  // 白っぽさを抑える（2026-11-15 ChatGPT の 3 回目のレビュー: 露出を少し下げ、彩度と締まりを少し上げる）
  col *= 0.93;
  let l = dot(col, vec3f(0.2126, 0.7152, 0.0722));
  col = max(mix(vec3f(l), col, 1.18 - u.desat), vec3f(0.0));
  col *= mix(vec3f(1.0), vec3f(1.06, 1.0, 0.92), u.warm);
  // 灯りが戻る前の青い宵（暗部を藍へ、全体を少し冷たく）
  col = mix(col, col * vec3f(0.78, 0.86, 1.18) + vec3f(0.0, 0.01, 0.04) * (1.0 - l), u.cool);
  // やわらかいフィルミックの肩（白飛びを丸める）→ 0..1 に収めてから S 字で締める
  col = col / (vec3f(1.0) + max(col - vec3f(0.8), vec3f(0.0)) * 0.9);
  col = clamp(col, vec3f(0.0), vec3f(1.0));
  col = mix(col, col * col * (vec3f(3.0) - 2.0 * col), 0.28);
  col *= 1.0 - outside * u.dim;
  // 周辺減光
  let q = (uv - vec2f(0.5)) * vec2f(1.15, 1.0);
  let v = 1.0 - dot(q, q) * u.vignette * 2.4;
  col *= clamp(v, 0.0, 1.0);
  // 自機の外周の縁取り（自機だけを描いた型の外側）: 内は明るいクリーム、その外に細い濃い線
  if (u.outline.w > 0.0) {
    let inside = mask_at(i.uv);
    var n1 = 0.0;
    var n2 = 0.0;
    for (var k = 0; k < 8; k++) {
      let a = f32(k) * 0.7853982;
      let d = vec2f(cos(a), sin(a)) * u.mask_px;
      n1 = max(n1, mask_at(i.uv + d * 1.7));
      n2 = max(n2, mask_at(i.uv + d * 3.1));
    }
    let inner = clamp(n1 - inside, 0.0, 1.0);
    let outer = clamp(n2 - max(n1, inside), 0.0, 1.0);
    col = mix(col, u.outline_dark.rgb, outer * 0.7 * u.outline.w);
    // 内の線は細めで少し透かす（太い白い線はシールのように見えた）
    col = mix(col, u.outline.rgb, inner * 0.85 * u.outline.w);
  }
  // 乱舞の金の色調・被弾の赤
  let lg = dot(col, vec3f(0.2126, 0.7152, 0.0722));
  col = mix(col, vec3f(lg) * vec3f(1.25, 0.95, 0.55) + col * 0.25, u.gold * 0.55);
  col = mix(col, col * vec3f(1.25, 0.55, 0.5), u.red);
  col += vec3f(u.flash);
  // 粒子（ごく弱い）
  col += vec3f((hash2(uv * u.res + fract(u.time) * 91.7) - 0.5) * 0.012);
  return vec4f(to_srgb(max(col, vec3f(0.0))), 1.0);
}
