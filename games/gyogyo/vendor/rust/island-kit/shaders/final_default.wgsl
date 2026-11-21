// 仕上げを渡さないときの既定（色をそのまま画面へ）
struct KitFinalU { pad: vec4f };
@group(1) @binding(0) var<uniform> u: KitFinalU;
@fragment fn fs_final(i: VOut) -> @location(0) vec4f {
  return vec4f(to_srgb(max(smp(i.uv).rgb, vec3f(0.0))), 1.0);
}
