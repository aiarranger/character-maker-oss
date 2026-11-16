// 派手な見せ方: 斬撃の弧（三日月の帯）・剣の軌跡・火花と粒・地面の衝撃波・亀裂・光の柱・爆発の球・予告の円・連鎖の線・格の光の刃。
// 式は src/view/vfx.ts・rank-blade.ts の GLSL をそのまま写す。uniform は FxInst の a〜d で 1 個ごとに渡す。
// GLSL で未定義の pow(負, 2.0) は x * x に、smoothstep(大, 小, x) は 1.0 - smoothstep(小, 大, x) にして同じ値を出す。
struct FxIn {
  @location(8) m0: vec4f,
  @location(9) m1: vec4f,
  @location(10) m2: vec4f,
  @location(11) m3: vec4f,
  @location(12) a: vec4f,
  @location(13) b: vec4f,
  @location(14) c: vec4f,
  @location(15) d: vec4f,
};
struct FxOut {
  @builtin(position) pos: vec4f,
  @location(0) uv: vec2f,
  @location(1) @interpolate(flat) a: vec4f,
  @location(2) @interpolate(flat) b: vec4f,
  @location(3) @interpolate(flat) c: vec4f,
  @location(4) @interpolate(flat) d: vec4f,
};
fn fx_model(i: FxIn) -> mat4x4f { return mat4x4f(i.m0, i.m1, i.m2, i.m3); }
fn fx_out(pos: vec4f, uv: vec2f, i: FxIn) -> FxOut {
  var o: FxOut;
  o.pos = pos; o.uv = uv; o.a = i.a; o.b = i.b; o.c = i.c; o.d = i.d;
  return o;
}

// ---------------- 斬撃の弧 ----------------
// a = uFrom, uTo, uR, uWidth / b = uP, uLife, uAlpha, uTime / c.rgb = uCore / d.rgb = uEdge
@vertex fn vs_arc(v: VIn, i: FxIn) -> FxOut {
  let a = mix(i.a.x, i.a.y, v.uv.x);
  // 先端ほど太く、末端は細い三日月
  let taper = 0.55 + 0.45 * sin(v.uv.x * 3.14159);
  // 帯は斜めの壁（円錐の面）: 上からも横からも幅が見える
  let w = (v.uv.y - 0.5) * i.a.w * taper;
  let r = i.a.z + w * 0.62;
  let p = vec3f(sin(a) * r, w * 0.78, cos(a) * r);
  return fx_out(g.view_proj * fx_model(i) * vec4f(p, 1.0), v.uv, i);
}
fn hash1(n: f32) -> f32 { return fract(sin(n) * 43758.5453); }
@fragment fn fs_arc(i: FxOut) -> @location(0) vec4f {
  // 先頭が u=0 から 1 へ走り、後ろに尾を引く。先頭ほど太い三日月
  let head = i.b.x * 1.3;
  let tail = 0.85;
  let d = head - i.uv.x;
  if (d < 0.0 || d > tail) { discard; }
  let along = 1.0 - d / tail;
  let thick = mix(0.12, 1.0, pow(along, 0.7));
  let vv = (i.uv.y - (1.0 - thick)) / thick;
  if (vv < 0.0) { discard; }
  let inner = smoothstep(0.0, 0.45, vv);
  let outer = 1.0 - smoothstep(0.88, 1.0, vv);
  let core = smoothstep(0.5, 0.9, vv) * outer;
  let streak = 0.82 + 0.18 * sin(i.uv.y * 70.0 + hash1(floor(i.uv.y * 18.0)) * 6.0 - i.b.w * 30.0);
  let head_glow = exp(-d * 18.0);
  let a = inner * outer * (0.3 + 0.7 * along) * streak * i.b.z * (1.0 - i.b.y);
  let col = mix(i.d.rgb, i.c.rgb, clamp(core * (0.4 + along) + head_glow * 0.6, 0.0, 1.0));
  return vec4f(col * a * 2.3, min(1.0, a * 1.2));
}

// ---------------- 地面の波・亀裂 ----------------
@vertex fn vs_basic_uv(v: VIn, i: FxIn) -> FxOut {
  return fx_out(g.view_proj * fx_model(i) * vec4f(v.pos, 1.0), v.uv, i);
}
// a.rgb = uColor, a.w = uK / b.x = uThick
@fragment fn fs_ring(i: FxOut) -> @location(0) vec4f {
  let r = length(i.uv - 0.5) * 2.0;
  let front = i.a.w;
  let x = (r - front) / max(i.b.x, 0.01);
  let band = exp(-(x * x));
  let inner = smoothstep(front - 0.4, front, r) * (1.0 - step(front, r)) * 0.25;
  let a = (band + inner) * (1.0 - i.a.w) * 1.4;
  if (r > 1.0) { discard; }
  return vec4f(i.a.rgb * a, a);
}
fn crack_hash(p: vec2f, seed: f32) -> f32 { return fract(sin(dot(p, vec2f(127.1, 311.7)) + seed) * 43758.5453); }
// a.rgb = uGlow, a.w = uA / b.x = uSeed
@fragment fn fs_crack(i: FxOut) -> @location(0) vec4f {
  let q = (i.uv - 0.5) * 2.0;
  let r = length(q);
  if (r > 1.0) { discard; }
  let ang = atan2(q.y, q.x);
  // 放射状のひび: 角度の格子ごとにゆらぐ線
  let n = 9.0;
  let cell = floor((ang + 3.14159) / 6.28318 * n);
  let center = (cell + 0.5 + (crack_hash(vec2f(cell, 1.0), i.b.x) - 0.5) * 0.6) / n * 6.28318 - 3.14159;
  let wob = sin(r * 18.0 + cell * 3.0) * 0.04;
  var line = 1.0 - smoothstep(0.0, 0.035 + r * 0.02, abs(ang - center + wob) * r);
  let len = 0.45 + crack_hash(vec2f(cell, 7.0), i.b.x) * 0.55;
  line *= 1.0 - smoothstep(len - 0.15, len, r);
  let pit = 1.0 - smoothstep(0.0, 0.28, r);
  let a = clamp(line + pit * 0.7, 0.0, 1.0) * i.a.w;
  // 割れ目の奥が光る
  let col = mix(vec3f(0.08, 0.04, 0.06), i.a.rgb * 1.1, (1.0 - r) * (1.0 - r) * 0.7 * i.a.w);
  return vec4f(col, a * 0.85);
}

// ---------------- 光の柱・爆発 ----------------
// a.rgb = uColor, a.w = uA / b.x = uTime, b.y = uTwist
@fragment fn fs_pillar(i: FxOut) -> @location(0) vec4f {
  let s = sin((i.uv.x * 6.28318 * 3.0 + i.uv.y * i.b.y * 6.0 - i.b.x * 14.0)) * 0.5 + 0.5;
  let s2 = sin((i.uv.x * 6.28318 * 7.0 - i.uv.y * i.b.y * 9.0 - i.b.x * 21.0)) * 0.5 + 0.5;
  let stripe = pow(s, 3.0) * 0.8 + pow(s2, 6.0) * 0.6;
  let fade_y = smoothstep(0.0, 0.12, i.uv.y) * (1.0 - smoothstep(0.55, 1.0, i.uv.y));
  let a = stripe * fade_y * i.a.w * 0.7;
  return vec4f(i.a.rgb * a * 1.8, a);
}
struct BallOut {
  @builtin(position) pos: vec4f,
  @location(0) n: vec3f,
  @location(1) v: vec3f,
  @location(2) @interpolate(flat) a: vec4f,
};
// a.rgb = uColor, a.w = uA。球の大きさはそろっているので、normalMatrix は同じ行列の回転で足りる
@vertex fn vs_ball(v: VIn, i: FxIn) -> BallOut {
  let mv = g.view * fx_model(i);
  let p = mv * vec4f(v.pos, 1.0);
  var o: BallOut;
  o.pos = g.proj * p;
  o.n = (mv * vec4f(v.normal, 0.0)).xyz;
  o.v = -p.xyz;
  o.a = i.a;
  return o;
}
@fragment fn fs_ball(i: BallOut) -> @location(0) vec4f {
  let f = pow(1.0 - abs(dot(normalize(i.n), normalize(i.v))), 3.0);
  let a = f * 0.75 * i.a.w;
  return vec4f(mix(i.a.rgb, vec3f(1.0), 0.4) * a * 1.6, a);
}

// ---------------- 剣の軌跡 ----------------
struct TrailIn {
  @location(0) pos: vec3f,
  @location(1) a: f32,
};
struct TrailOut {
  @builtin(position) pos: vec4f,
  @location(0) a: f32,
  @location(1) @interpolate(flat) color: vec4f,
};
// i.a.rgb = uColor, i.a.w = uOn
@vertex fn vs_trail(v: TrailIn, i: FxIn) -> TrailOut {
  var o: TrailOut;
  o.pos = g.view_proj * vec4f(v.pos, 1.0);
  o.a = v.a;
  o.color = i.a;
  return o;
}
@fragment fn fs_trail(i: TrailOut) -> @location(0) vec4f {
  let a = i.a * i.color.w;
  return vec4f(mix(i.color.rgb, vec3f(1.0, 0.97, 0.9), clamp(i.a * 1.3, 0.0, 1.0)) * a * 2.4, a);
}

// ---------------- 粒 ----------------
struct PartIn {
  @location(8) pos: vec3f,
  @location(9) dir: vec3f,
  @location(10) col: vec4f,
  @location(11) size: vec3f, // x = 大きさ, y = 伸び, z = 回転
  @location(12) ty: f32,
};
struct PartOut {
  @builtin(position) pos: vec4f,
  @location(0) uv: vec2f,
  @location(1) col: vec4f,
  @location(2) ty: f32,
};
@vertex fn vs_part(v: VIn, i: PartIn) -> PartOut {
  var mv = g.view * vec4f(i.pos, 1.0);
  let d = (g.view * vec4f(i.dir, 0.0)).xyz;
  let sd = d.xy;
  let l = length(sd);
  var ax = vec2f(cos(i.size.z), sin(i.size.z));
  if (l > 1e-4) { ax = sd / l; }
  let ay = vec2f(-ax.y, ax.x);
  let c = v.pos.xy;
  let stretch = 1.0 + i.size.y * clamp(l, 0.0, 6.0);
  let off = ax * c.x * i.size.x * stretch + ay * c.y * i.size.x;
  mv = vec4f(mv.xy + off, mv.zw);
  var o: PartOut;
  o.pos = g.proj * mv;
  o.uv = v.uv;
  o.col = i.col;
  o.ty = i.ty;
  return o;
}
@fragment fn fs_part(i: PartOut) -> @location(0) vec4f {
  let q = i.uv - 0.5;
  var a: f32;
  if (i.ty < 0.5) { // 火花: 細長い光
    a = exp(-dot(q * vec2f(2.2, 7.0), q * vec2f(2.2, 7.0)) * 6.0);
  } else if (i.ty < 1.5) { // 光の玉
    a = exp(-dot(q, q) * 14.0);
  } else if (i.ty < 2.5) { // 星（4つの角）
    let s = max(1.0 - abs(q.x) * 14.0 - abs(q.y) * 2.4, 0.0) + max(1.0 - abs(q.y) * 14.0 - abs(q.x) * 2.4, 0.0);
    a = clamp(s, 0.0, 1.0) + exp(-dot(q, q) * 40.0) * 0.8;
  } else if (i.ty < 3.5) { // 紙吹雪（お菓子のかけら）: 角の丸い四角
    let b = abs(q) - vec2f(0.32, 0.2);
    a = 1.0 - smoothstep(0.0, 0.06, length(max(b, vec2f(0.0))));
    return vec4f(i.col.rgb, a * i.col.a);
  } else { // 土煙: やわらかい丸（重ねて塗る）
    a = (1.0 - smoothstep(0.15, 0.5, length(q))) * 0.6;
    return vec4f(i.col.rgb, a * i.col.a);
  }
  return vec4f(i.col.rgb * a * i.col.a * 2.0, a * i.col.a);
}

// ---------------- 連鎖の光の線 ----------------
struct LinkIn {
  @location(8) a: vec3f,
  @location(9) b: vec3f,
  @location(10) col: vec4f,
  @location(11) p: vec2f, // x = 幅, y = 走る光の位置
};
struct LinkOut {
  @builtin(position) pos: vec4f,
  @location(0) uv: vec2f,
  @location(1) col: vec4f,
  @location(2) run: f32,
};
@vertex fn vs_link(v: VIn, i: LinkIn) -> LinkOut {
  let a = g.view * vec4f(i.a, 1.0);
  let b = g.view * vec4f(i.b, 1.0);
  var d = b.xy - a.xy;
  let l = max(length(d), 1e-4);
  d /= l;
  let n = vec2f(-d.y, d.x);
  var mv = mix(a, b, v.uv.x);
  mv = vec4f(mv.xy + n * v.pos.y * i.p.x + d * (v.uv.x - 0.5) * i.p.x * 0.8, mv.zw);
  var o: LinkOut;
  o.pos = g.proj * mv;
  o.uv = v.uv;
  o.col = i.col;
  o.run = i.p.y;
  return o;
}
@fragment fn fs_link(i: LinkOut) -> @location(0) vec4f {
  let y = (i.uv.y - 0.5) * 2.0;
  let core = exp(-y * y * 18.0);
  let glow = exp(-y * y * 3.0) * 0.45;
  let along = 0.45 + 0.55 * i.uv.x;
  let x = (i.uv.x - i.run) * 9.0;
  let run = exp(-(x * x)) * step(i.run, 1.05) * 1.6;
  let a = (core + glow) * along * i.col.a + run * core * i.col.a;
  let c = mix(i.col.rgb, vec3f(1.0), core * 0.7);
  return vec4f(c * a * 1.8, a);
}

// 飴の糸: 光らせない飴色の帯。縁は濃く、芯は明るく、上寄りに照りが入る
@fragment fn fs_taffy(i: LinkOut) -> @location(0) vec4f {
  let y = (i.uv.y - 0.5) * 2.0;
  let edge = smoothstep(1.0, 0.7, abs(y));
  let body = edge * smoothstep(0.0, 0.12, i.uv.x) * smoothstep(1.0, 0.88, i.uv.x) * 0.5 + edge * 0.5;
  // 琥珀の透明な飴: 芯は透けて明るく、縁は濃い。照りはねじれに合わせて帯の上をめぐる（run = ねじれの位相）
  let base = mix(i.col.rgb * vec3f(0.5, 0.26, 0.08), i.col.rgb * vec3f(1.08, 1.0, 0.9), 1.0 - y * y);
  let sy = 0.55 * sin(i.run);
  let shine = exp(-(y - sy) * (y - sy) * 40.0);
  let c = mix(base, vec3f(1.0, 0.97, 0.86), shine * 0.9);
  // 芯ほど透ける（地面が少し透けて見える）
  let a = body * i.col.a * mix(0.62, 1.0, y * y) + shine * 0.25;
  return vec4f(c, clamp(a, 0.0, 1.0));
}

// ---------------- 予告（地面の赤い印） ----------------
// a = uK, uKind, uArc, uFire / b.rgb = uColor, b.w = uTime
@fragment fn fs_tel(i: FxOut) -> @location(0) vec4f {
  let k = i.a.x; let kind = i.a.y; let arc = i.a.z; let fire = i.a.w; let time = i.b.w;
  let q = (i.uv - 0.5) * 2.0;
  let r = length(q);
  var a = 0.0;
  if (kind < 0.5) { // 円: 外の線 + 内から満ちる
    if (r > 1.0) { discard; }
    let edge = smoothstep(0.9, 0.97, r) * (1.0 - smoothstep(0.97, 1.0, r));
    let fill = step(r, k) * 0.32 + smoothstep(k - 0.05, k, r) * step(r, k) * 0.6;
    a = edge * 0.9 + fill + 0.08;
  } else if (kind < 1.5) { // 扇
    if (r > 1.0) { discard; }
    let ang = abs(atan2(q.x, q.y));
    if (ang > arc) { discard; }
    let edge = smoothstep(0.92, 0.98, r) + smoothstep(arc - 0.04, arc, ang);
    a = min(edge, 1.0) * 0.8 + step(r, k) * 0.3 + 0.07;
  } else if (kind < 2.5) { // 帯（uv.y が前）: うすい帯に、前へ流れる矢じり
    let side = abs(i.uv.x - 0.5) * 2.0;
    let fade = (1.0 - smoothstep(0.7, 1.0, side)) * smoothstep(0.0, 0.06, i.uv.y) * (1.0 - smoothstep(0.85, 1.0, i.uv.y));
    let chev = fract(i.uv.y * 5.0 - side * 0.35 - time * 2.5);
    let arrow = smoothstep(0.0, 0.08, chev) * (1.0 - smoothstep(0.12, 0.3, chev));
    a = (0.12 + arrow * 0.5 + step(i.uv.y, k) * 0.12) * fade;
  } else { // 外へ広がる波の予告: 内側の帯がはしる
    if (r > 1.0) { discard; }
    let w = fract(r * 3.0 - time * 2.0);
    a = (smoothstep(0.0, 0.1, w) * (1.0 - smoothstep(0.1, 0.25, w))) * 0.35 * (1.0 - r * 0.5) + smoothstep(0.95, 1.0, r) * 0.5;
    a *= fire;
  }
  let blink = 0.75 + 0.25 * sin(time * (10.0 + k * 20.0));
  return vec4f(i.b.rgb * a * blink * 1.4, a * blink);
}

// ---------------- 格の光の刃（MeshBasicMaterial 相当。霧も受ける） ----------------
struct BasicOut {
  @builtin(position) pos: vec4f,
  @location(0) world: vec3f,
  @location(1) @interpolate(flat) col: vec4f,
};
// a.rgb = color, a.w = opacity
@vertex fn vs_blade(v: VIn, i: FxIn) -> BasicOut {
  let w = fx_model(i) * vec4f(v.pos, 1.0);
  var o: BasicOut;
  o.pos = g.view_proj * w;
  o.world = w.xyz;
  o.col = i.a;
  return o;
}
@fragment fn fs_blade(i: BasicOut) -> @location(0) vec4f {
  return vec4f(apply_fog(i.col.rgb, view_depth(i.world)), i.col.a);
}
