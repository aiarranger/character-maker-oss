// Only opaque shared body primitives. All skin/shading arithmetic is in hero_common.wgsl.
@group(2) @binding(0) var map: texture_2d<f32>;
@group(2) @binding(1) var map_smp: sampler;
@group(3) @binding(0) var<storage, read> palette: array<mat4x4f>;
@group(3) @binding(1) var<storage, read> deltas: array<Delta>;
@group(3) @binding(2) var<storage, read> morph_w: array<f32>;
// Uniform binding offsets can have padding; this is not an array<HeroU>.
@group(3) @binding(3) var<storage, read> hero_data: array<vec4f>;
@group(3) @binding(4) var<uniform> draw: DrawU;

fn instance_hero(base: u32) -> HeroU {
  return HeroU(hero_data[base], hero_data[base + 1u], hero_data[base + 2u],
    hero_data[base + 3u], hero_data[base + 4u], hero_data[base + 5u],
    hero_data[base + 6u], hero_data[base + 7u], hero_data[base + 8u]);
}
struct VOut {
  @builtin(position) clip: vec4f,
  @location(0) normal: vec3f,
  @location(1) uv: vec2f,
  @location(2) color: vec4f,
  @location(3) mv: vec3f,
  @location(4) @interpolate(flat) hero_base: u32,
};
@vertex fn vs_main(v: VIn, @location(5) joints: vec4u, @location(6) weights: vec4f,
    @location(7) slot: vec4u, @builtin(vertex_index) vid: u32) -> VOut {
  let hero = instance_hero(slot.z);
  let s = skin(v, joints, weights, vid, hero, draw, slot.x, slot.y);
  let mv = g.view * vec4f(s.world, 1.0);
  var o: VOut;
  o.clip = g.proj * mv;
  o.normal = s.normal;
  o.uv = v.uv;
  o.color = v.color;
  o.mv = mv.xyz;
  o.hero_base = slot.z;
  return o;
}
@fragment fn fs_basic(i: VOut, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  let hero = instance_hero(i.hero_base);
  if (fade_out(i.clip, hero)) { discard; }
  let t = textureSample(map, map_smp, i.uv);
  let d = draw.color * t * i.color;
  if (d.a < draw.p.x) { discard; }
  let a = select(d.a, 1.0, draw.p.y > 0.5);
  let c = shade_sun(shade_rim(d.rgb, i.normal, i.mv, hero), i.normal, i.mv, front, hero);
  return shade_out(c, a, -i.mv.z, true, false, false);
}
