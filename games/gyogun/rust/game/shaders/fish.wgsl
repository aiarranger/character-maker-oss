// 浜辺の魚（docs/gyogun-spec.md）: Fish Kit（CC0）の写真の魚を、頭から尾へうねらせて泳がせる。
// 模型は口先が +Z・背が +Y・長さ 1・体の真ん中が原点（view/fish.rs がそろえる）。
// うねりは Fish Kit の fishH と同じ式: 包絡 A(s) = e0 + e1 s + e2 s^2（s = 0 口先〜1 尾の先）× sin(2π(s/λ − cyc))。
// 1 匹ごとの値: fx は群れと同じ（x = 当たりの白い光、y = 構えの赤い光、z = 消える前の白化、w = 金の縁）、
// swim = (尾を振った回数 cyc, 振れの大きさ, 体を C の字に曲げる量, 0)
struct KindU { sparkle: vec4f, captain: vec4f, env: vec4f };
@group(2) @binding(0) var map_t: texture_2d<f32>;
@group(2) @binding(1) var map_s: sampler;
@group(3) @binding(0) var<uniform> k: KindU;

struct FIn {
  @location(8) m0: vec4f,
  @location(9) m1: vec4f,
  @location(10) m2: vec4f,
  @location(11) m3: vec4f,
  @location(12) fx: vec4f,
  @location(13) swim: vec4f,
};

struct Out {
  @builtin(position) pos: vec4f,
  @location(0) world: vec3f,
  @location(1) n: vec3f,
  @location(2) uv: vec2f,
  @location(3) fx: vec4f,
};

fn wave(s: f32, sw: vec4f) -> f32 {
  let a = k.env.x + k.env.y * s + k.env.z * s * s;
  return sw.y * a * sin(6.2831853 * (s / k.env.w - sw.x));
}

// 模型の点を泳ぐ形へ（横 = x へずらす）。返す w は横の傾き（法線を回すのに使う）
fn swim_pos(p: vec3f, sw: vec4f) -> vec4f {
  let s = clamp(0.5 - p.z, 0.0, 1.0);
  let d = 0.02;
  let x = wave(s, sw) + sw.z * s * s * 0.45;
  let x2 = wave(min(s + d, 1.0), sw) + sw.z * (s + d) * (s + d) * 0.45;
  return vec4f(p.x + x, p.y, p.z, (x2 - x) / d);
}

@vertex fn vs(v: VIn, i: FIn) -> Out {
  let m = mat4x4f(i.m0, i.m1, i.m2, i.m3);
  let sp = swim_pos(v.pos, i.swim);
  // 体が横へ曲がった分だけ、法線も回す（尾へ向かう向きは −z なので、傾きは −dx/dz）
  let nn = normalize(vec3f(v.normal.x + sp.w * v.normal.z, v.normal.y, v.normal.z));
  let w = m * vec4f(sp.xyz, 1.0);
  var o: Out;
  o.pos = g.view_proj * w;
  o.world = w.xyz;
  o.n = normal_world(m, nn);
  o.uv = v.uv;
  o.fx = i.fx;
  return o;
}

@fragment fn fs(i: Out, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  let t = textureSample(map_t, map_s, i.uv);
  // ひれは切り抜いた板（アルファで抜く。両面）
  if (t.a < 0.5) { discard; }
  let alb = t.rgb;
  var n = normalize(i.n);
  if (!front) { n = -n; }
  let v = normalize(g.cam.xyz - i.world);
  let hd = n.y * 0.5 + 0.5;
  let fill = mix(g.hemi_ground.rgb, g.hemi_sky.rgb, hd);
  let ndl = max(dot(n, g.key_dir.xyz), 0.0);
  let refl = max(luma(g.hemi_sky.rgb) + luma(g.key_color.rgb), 0.001);
  let light_k = vec3f(0.62) + 0.62 * (fill * 0.6 + g.key_color.rgb * ndl) / refl;
  // 鱗のつや（夕日の照り返し）と、輪郭の光
  let h = normalize(g.key_dir.xyz + v);
  let spec = pow(max(dot(n, h), 0.0), 40.0) * 0.55;
  let nv = clamp(dot(n, v), 0.0, 1.0);
  let rim = pow(1.0 - nv, 2.8);
  // 写真の色を少し濃くして、砂の色から離す
  let al = luma(alb);
  let alb2 = mix(vec3f(al), alb, 1.35);
  var c = alb2 * light_k + g.key_color.rgb / refl * spec + g.rim.rgb * rim * 0.6;
  // 構え: 赤く脈打つ縁
  let warn = i.fx.y * (0.6 + 0.4 * sin(g.misc.x * 28.0));
  c = mix(c, c * vec3f(1.3, 0.55, 0.5), i.fx.y * 0.5);
  c += vec3f(1.0, 0.18, 0.12) * (rim * 2.0 + 0.15) * warn;
  // 隊長: 金の縁 / 連鎖に巻き込まれた魚: 金に燃える
  let cap = k.captain.x;
  c += vec3f(1.0, 0.7, 0.3) * rim * i.fx.w * 0.8 * cap;
  c = mix(c, c * 0.55 + vec3f(1.0, 0.72, 0.28) * (0.55 + rim * 1.6), i.fx.w * (1.0 - cap) * 0.85);
  // 当たり: 白く飛ぶ
  c = mix(c, c * 0.4 + vec3f(0.75, 0.6, 0.5), clamp(i.fx.x, 0.0, 1.0) * 0.6);
  c = mix(c, k.sparkle.rgb * 0.9 + vec3f(0.15), i.fx.z * 0.7);
  return vec4f(apply_fog(c, view_depth(i.world)), 1.0);
}

// ---- 輪郭線（裏返しの殻）: 夕日の砂に溶けないように、主役と同じ濃い線で縁取る ----
@vertex fn vs_line(v: VIn, i: FIn) -> Out {
  let m = mat4x4f(i.m0, i.m1, i.m2, i.m3);
  let sp = swim_pos(v.pos, i.swim);
  let nn = normalize(vec3f(v.normal.x + sp.w * v.normal.z, v.normal.y, v.normal.z));
  // 太らせた体でも線の太さがそろうよう、模型の中で押し出す（長さ 1 に対して 1.4%。金の縁の敵は少し太く）
  let w = m * vec4f(sp.xyz + nn * (0.014 + i.fx.w * 0.006), 1.0);
  var o: Out;
  o.pos = g.view_proj * w;
  o.world = w.xyz;
  o.n = nn;
  o.uv = v.uv;
  o.fx = i.fx;
  return o;
}
@fragment fn fs_line(i: Out) -> @location(0) vec4f {
  if (textureSample(map_t, map_s, i.uv).a < 0.5) { discard; }
  return vec4f(apply_fog(vec3f(0.035, 0.03, 0.06), view_depth(i.world)), 1.0);
}

// ---- 影の型 ----
@vertex fn vs_shadow(v: VIn, i: FIn) -> @builtin(position) vec4f {
  let m = mat4x4f(i.m0, i.m1, i.m2, i.m3);
  return g.light_vp * m * vec4f(swim_pos(v.pos, i.swim).xyz, 1.0);
}
