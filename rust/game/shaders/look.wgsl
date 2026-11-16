// 下見の陰影（スタジオの見え方）。材質の色と絵はそのままで、カメラから見た左上の光のやわらかい陰・縁の光を足す。
// 頭のまわり（球の中）は、面の向きの代わりに球の向きで陰を付ける（顔の細かい凹凸や重ねた部品の継ぎ目に陰が出ない）。
// 書き出す GLB の材質は光なし（KHR_materials_unlit）のまま（rust/game/src/look.rs）。
struct LookU {
  color: vec4f,    // rgb（線形）, 不透明度
  p: vec4f,        // x = alphaTest、y = 頂点の色を掛けるか、z = 透けるか、w = premultiplied か
  head: vec4f,     // 頭の球の中心（世界）と半径（0 なら球なし）
  shade: vec4f,    // x = 体の陰の濃さ、y = 頭の陰の濃さ、z = 縁の光の強さ、w = 陰のぼかしの幅
  shadow: vec4f,   // rgb = 陰の色（掛ける値。濃さ 1 のとき）
  rim: vec4f,      // rgb = 縁の光の色
  light: vec4f,    // カメラから見た光のある向き
};
@group(2) @binding(0) var<uniform> u: LookU;

struct LOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
  @location(2) uv: vec2f,
  @location(3) color: vec4f,
  @location(4) depth: f32,
};
fn emit(w: vec4f, n: vec3f, v: VIn) -> LOut {
  let mv = g.view * w;
  var o: LOut;
  o.clip = g.proj * mv; o.world = w.xyz; o.normal = n; o.uv = v.uv; o.color = v.color; o.depth = -mv.z;
  return o;
}
@vertex fn vs_look(v: VIn, i: IIn) -> LOut {
  let model = inst_model(i);
  return emit(model * vec4f(v.pos, 1.0), normal_world(model, v.normal), v);
}
@vertex fn vs_look_skinned(v: VIn, i: IIn, @location(5) joints: vec4u, @location(6) weights: vec4f) -> LOut {
  let s = k_skin_matrix(joints, weights);
  return emit(s * vec4f(v.pos, 1.0), k_skin_normal(inst_model(i), s, v.normal), v);
}
@fragment fn fs_look(i: LOut, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  var d = u.color * textureSample(tex0, smp0, i.uv);
  if (u.p.y > 0.5) { d *= i.color; }
  if (d.a < u.p.x) { discard; }
  // 面の向き（法線が 0 の頂点は上向きにする）
  var n = i.normal;
  if (dot(n, n) < 1e-8) { n = vec3f(0.0, 1.0, 0.0); }
  n = normalize(n);
  if (!front) { n = -n; }
  // 頭の球: 中は球の向き、外へ出るほど面の向きへ戻す
  var k = 0.0;
  if (u.head.w > 0.0) {
    let to = i.world - u.head.xyz;
    let dist = length(to);
    k = 1.0 - smoothstep(u.head.w * 0.92, u.head.w * 1.35, dist);
    n = normalize(mix(n, to / max(dist, 1e-5), k));
  }
  let nv = normalize((g.view * vec4f(n, 0.0)).xyz);
  let vv = normalize((g.view * vec4f(g.cam.xyz - i.world, 0.0)).xyz);
  let ndl = dot(nv, normalize(u.light.xyz));
  let s = smoothstep(-u.shade.w, u.shade.w + 0.35, ndl);
  let amount = mix(u.shade.x, u.shade.y, k);
  var c = d.rgb * mix(mix(vec3f(1.0), u.shadow.rgb, amount), vec3f(1.0), s);
  // 縁の光: 光の反対側の上寄りだけ（全周を光らせない）
  let fr = pow(1.0 - clamp(dot(nv, vv), 0.0, 1.0), 4.0);
  let side = clamp(dot(nv, normalize(vec3f(0.55, 0.5, -0.45))) * 0.9 + 0.3, 0.0, 1.0);
  c += u.rim.rgb * fr * side * u.shade.z * mix(1.0, 0.5, k);
  var a = d.a;
  if (u.p.z < 0.5) { a = 1.0; }
  return shade_out(c, a, i.depth, false, false, u.p.w > 0.5);
}
