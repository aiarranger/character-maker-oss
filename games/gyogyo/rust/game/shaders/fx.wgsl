// 粒: 足もとの砂ぼこり・水しぶき・紙吹雪（fx.ts）
struct U { p: vec4f };  // p.x = uH
@group(2) @binding(0) var<uniform> u: U;
struct O { @builtin(position) clip: vec4f, @location(0) c: vec4f, @location(1) uv: vec2f };
@vertex fn vs(@location(0) pos: vec3f, i: IIn, @location(13) a_color: vec4f, @location(14) a_size: f32, @builtin(vertex_index) vid: u32) -> O {
  let mv = g.view * inst_model(i) * vec4f(pos, 1.0);
  // カメラのすぐ前の粒は大きなぼけの丸になるので、上限を抑えて薄くする
  let size = min(a_size * u.p.x / max(0.5, -mv.z), u.p.x * 0.05);
  var o: O;
  o.c = vec4f(a_color.rgb, a_color.a * clamp((-mv.z - 0.8) / 1.6, 0.0, 1.0));
  o.clip = point_clip(g.proj * mv, size, vid);
  o.uv = point_uv(vid);
  return o;
}
@fragment fn fs(i: O) -> @location(0) vec4f {
  let l = textureSample(tex0, smp0, i.uv).r;
  return vec4f(i.c.rgb, i.c.a * l);
}
