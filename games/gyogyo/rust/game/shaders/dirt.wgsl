// 走路: 砂に、ならした筋・足跡の濃淡・ゴールの市松・スタートの線を描く（course.ts の dirtMaterial。MeshLambertMaterial の改造）。uv に aSD。
struct U { dirt: vec4f, dirt_dark: vec4f, p: vec4f };  // p: uFinish, uStart, uWidth, uPool
@group(2) @binding(0) var<uniform> u: U;
struct O { @builtin(position) clip: vec4f, @location(0) world: vec3f, @location(1) n: vec3f, @location(2) sd: vec2f, @location(3) depth: f32, @location(4) recv: f32 };
@vertex fn vs(v: VIn, i: IIn) -> O {
  let m = inst_model(i);
  let w = m * vec4f(v.pos, 1.0);
  let mv = g.view * w;
  var o: O;
  o.clip = g.proj * mv;
  o.world = w.xyz;
  o.n = normal_world(m, v.normal);
  o.sd = v.uv;
  o.depth = -mv.z;
  o.recv = i.fx.w;
  return o;
}
@fragment fn fs(i: O, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  let S = i.sd.x;
  let D = i.sd.y;
  let u_finish = u.p.x;
  let u_start = u.p.y;
  let u_width = u.p.z;
  let u_pool = u.p.w;
  // 大きなむら
  let big = t_fbm(vec2f(S * 0.035, D * 0.12));
  // 進む向きの筋（ならした跡）。速さが見えるように細かい濃淡を s 方向に刻む
  let comb = sin(D * 16.0 + t_noise(vec2f(S * 0.05, D)) * 3.0) * 0.5 + 0.5;
  let grain = t_noise(vec2f(S * 2.2, D * 7.0));
  let kicks = smoothstep(0.62, 0.8, t_noise(vec2f(S * 0.9, D * 1.7)));
  // よく踏まれる内側は濃く
  let worn = (1.0 - smoothstep(0.0, u_width * 0.75, D)) * 0.25;
  var c = mix(u.dirt.rgb, u.dirt_dark.rgb, clamp(big * 0.32 + comb * 0.12 + worn * 0.8 + kicks * 0.2 + (grain - 0.5) * 0.28, 0.0, 1.0));
  // 6m ごとに、薄い帯（速さの目印）
  let band = smoothstep(0.86, 1.0, cos(S * 6.2831853 / 6.0));
  c *= 1.0 - band * 0.2;
  // 縁のそば（内と外の 0.3〜1.3m）に、お菓子のかけら（色つきの小さな粒）を散らす。真ん中には置かない
  let near_edge = (1.0 - smoothstep(1.1, 1.4, D)) * smoothstep(0.25, 0.35, D) + smoothstep(u_width - 1.4, u_width - 1.1, D) * (1.0 - smoothstep(u_width - 0.35, u_width - 0.25, D));
  if (near_edge > 0.0) {
    let cell = floor(vec2f(S * 3.0, D * 3.0));
    let r = t_hash(cell);
    if (r > 0.82) {
      let ctr = (cell + vec2f(0.3 + 0.4 * t_hash(cell + 7.1), 0.3 + 0.4 * t_hash(cell + 3.7))) / 3.0;
      let dd = length(vec2f(S, D) - ctr);
      let k = (r - 0.82) / 0.18;
      var sc = vec3f(0.97, 0.56, 0.69);
      if (k > 0.2) { sc = vec3f(0.51, 0.84, 1.0); }
      if (k > 0.4) { sc = vec3f(0.61, 0.91, 0.63); }
      if (k > 0.6) { sc = vec3f(0.97, 0.84, 0.29); }
      if (k > 0.8) { sc = vec3f(1.0, 0.98, 0.94); }
      c = mix(c, sc, (1.0 - smoothstep(0.045, 0.06, dd)) * near_edge);
    }
  }
  // 照明塔の明かりのたまり（宵のコース）
  if (u_pool > 0.0) { let pl = cos((S / u_pool - 0.5) * 6.2831853) * 0.5 + 0.5; c *= 0.82 + pl * pl * 0.75; }
  // ゴールの市松
  let fd = S - u_finish;
  if (abs(fd) < 1.5) {
    // GLSL の mod（床で切る）
    let q = floor((fd + 1.5) / 0.75) + floor(D / 0.75);
    let ck = q - 2.0 * floor(q / 2.0);
    c = mix(vec3f(0.08, 0.08, 0.1), vec3f(0.97, 0.95, 0.9), ck);
  }
  // スタートの線
  let sd = S - u_start;
  if (abs(sd) < 0.18) { c = vec3f(0.96, 0.94, 0.88); }
  // 縁は白い線
  let edge = (1.0 - smoothstep(0.1, 0.18, D)) + smoothstep(u_width - 0.18, u_width - 0.1, D);
  c = mix(c, vec3f(0.95, 0.93, 0.86), clamp(edge, 0.0, 1.0) * 0.9);
  var n = normalize(i.n);
  if (!front) { n = -n; }
  let col = lit_lambert(false, i.world, n, i.n, i.recv, c);
  return vec4f(apply_fog(col, i.depth), 1.0);
}
