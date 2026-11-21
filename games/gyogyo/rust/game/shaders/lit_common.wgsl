// three の Lambert の光（island-kit の kit.wgsl の gather と同じ式）。走路・芝・観客（MeshLambertMaterial の改造）が使う。
// 計算はワールド空間。soft は softMaterial の回り込み。
const RECIP_PI: f32 = 0.3183098861837907;
fn re_dot(soft: bool, n: vec3f, l: vec3f) -> f32 {
  if (soft) { return smoothstep(0.18, 0.86, dot(n, l) * 0.5 + 0.5); }
  return saturate_(dot(n, l));
}
/// diffuse / π × (半球 + 環境 + Σ 光の色 × dotNL × 影)
fn lit_lambert(soft: bool, world: vec3f, n: vec3f, interp_n: vec3f, recv: f32, diffuse: vec3f) -> vec3f {
  var dd = vec3f(0.0);
  for (var k = 0u; k < lights.counts.y; k++) {
    let p = lights.point[k];
    let lv = p.pos.xyz - world;
    let d = length(lv);
    let c = p.color.rgb * distance_attenuation(d, p.pos.w, p.color.w);
    dd += re_dot(soft, n, normalize(lv)) * c * RECIP_PI * diffuse;
  }
  for (var k = 0u; k < lights.counts.x; k++) {
    var c = lights.dir[k].color.rgb;
    if (k < lights.shadow_count.x && recv > 0.5) { c *= dir_shadow(k, world, interp_n); }
    dd += re_dot(soft, n, lights.dir[k].dir.xyz) * c * RECIP_PI * diffuse;
  }
  var irr = lights.ambient.rgb;
  for (var k = 0u; k < lights.counts.w; k++) {
    let h = lights.hemi[k];
    let w = 0.5 * dot(n, h.dir.xyz) + 0.5;
    irr += mix(h.ground.rgb, h.sky.rgb, w);
  }
  return dd + irr * RECIP_PI * diffuse;
}
