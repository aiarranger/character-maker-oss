// 浜辺の魚（docs/gyogun-spec.md）: Fish Kit（CC0）の写真の魚を、頭から尾へうねらせて泳がせる。
// 模型は口先が +Z・背が +Y・長さ 1・体の真ん中が原点（view/fish.rs がそろえる）。
// うねりは Fish Kit の fishH と同じ式: 包絡 A(s) = e0 + e1 s + e2 s^2（s = 0 口先〜1 尾の先）× sin(2π(s/λ − cyc))。
// 1 匹ごとの値: fx は群れと同じ（x = 当たりの白い光、y = 構えの赤い光、z = 消える前の白化、w = 金の縁）、
// swim = (尾を振った回数 cyc, 振れの大きさ, 体を C の字に曲げる量, 0)
struct KindU {
  sparkle: vec4f, captain: vec4f, env: vec4f, center_len: vec4f,
  jaw: vec4f, upper: vec4f, upper2: vec4f, eye_fin: vec4f, fin_style: vec4f,
};
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
  @location(14) face: vec4f,
};

struct Out {
  @builtin(position) pos: vec4f,
  @location(0) world: vec3f,
  @location(1) n: vec3f,
  @location(2) uv: vec2f,
  @location(3) fx: vec4f,
  @location(4) part_rim: vec2f,
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

// 元の GLB 座標 (+X が口先) で、Fish Kit の関節とひれの重みを適用する。
struct Anatomy {
  @location(5) fin_jaw: vec4f, @location(6) pivot_height: vec4f, @location(7) flutter: vec3f,
};
struct FishPose { pos: vec3f, normal: vec3f };
const FISH_TAU = 6.2831853;
fn rotate_about(v: vec3f, axis: vec3f, angle: f32) -> vec3f {
  let c = cos(angle);
  return v*c + cross(axis, v)*sin(angle) + axis*dot(axis,v)*(1.-c);
}
fn rot_xy(v: vec2f, angle: f32) -> vec2f {
  let c = cos(angle); let s = sin(angle);
  return vec2f(c*v.x-s*v.y,s*v.x+c*v.y);
}
fn fish_hash(n: f32) -> f32 { return fract(sin(n*12.9898+4.1414)*43758.5453); }
fn eye_jump(id: f32, period: f32, t: f32) -> vec2f {
  let z = t/period + fish_hash(id+1.)*7.;
  let seg = floor(z);
  let a = vec2f(fish_hash((seg-1.)*1.7+id), fish_hash((seg-1.)*2.3+id+11.));
  let b = vec2f(fish_hash(seg*1.7+id), fish_hash(seg*2.3+id+11.));
  return mix(a,b,smoothstep(0.,0.08/period,fract(z)))-0.5;
}
fn whim_value(id: f32, seg: f32) -> f32 {
  return select(mix(-0.6,1.,fish_hash(seg*1.3+id*23.+5.)),0.,fish_hash(seg*3.7+id*11.)<0.45);
}
fn fin_whim(id: f32, t: f32) -> f32 {
  let period = 1.4 + 2.6*fish_hash(id*17.+0.3);
  let z = t/period + fish_hash(id*5.+1.7)*9.; let seg = floor(z);
  return mix(whim_value(id,seg-1.),whim_value(id,seg),smoothstep(0.,mix(0.12,0.3,fish_hash(id+seg*0.37)),fract(z)));
}

fn fish_pose(v: VIn, a: Anatomy, i: FIn) -> FishPose {
  let id = round(a.fin_jaw.x); let fw = a.fin_jaw.y; let pivot = a.pivot_height.xyz;
  let hinged = dot(pivot,pivot)>1e-10;
  let side = sign(pivot.z); let t = i.face.x; let seed = i.face.w;
  let len = k.center_len.w; let still = k.eye_fin.w>0.5;
  let slow = clamp((1.5-i.swim.y)/1.,0.,1.);
  let fin_clock = t*(1.+0.6*(1.-slow));
  let fold = mix(0.35,0.05,slow); let wobble = 0.12*k.fin_style.z;
  var p = v.pos; var n = v.normal;
  if (id==6. && hinged) {
    let eye_id = seed*7.13 + select(3.7,0.,side>0.);
    let gaze = eye_jump(eye_id,0.35+0.85*fish_hash(eye_id),t)
      + 0.3*eye_jump(eye_id+5.3,0.2+0.3*fish_hash(eye_id+2.),t)
      + 0.06*vec2f(sin(t*1.7+eye_id*5.),sin(t*2.3+eye_id*3.));
    let pitch = -side*gaze.y*0.5; let yaw = side*gaze.x*0.9 + clamp(i.swim.z,-1.,1.)*0.3;
    var radial = (p-pivot)*k.eye_fin.x+vec3f(0.,0.,side*k.eye_fin.y);
    radial = rotate_about(radial,vec3f(1.,0.,0.),pitch);
    n = rotate_about(n,vec3f(1.,0.,0.),pitch);
    p = pivot+rotate_about(radial,vec3f(0.,1.,0.),yaw);
    n = rotate_about(n,vec3f(0.,1.,0.),yaw);
  } else if (id==7.) {
    let angle = i.face.z;
    p += vec3f(fw*(1.-cos(angle)),0.,sign(p.z)*fw*sin(angle));
    n = rotate_about(n,vec3f(0.,1.,0.),sign(p.z)*angle*clamp(fw/(0.05*len),0.,1.));
  } else if (id>=1. && id<=5. && hinged && !still) {
    var axis = vec3f(0.,0.,1.); var angle = 0.;
    if (id==4.) {
      let hz = mix(0.2,0.5,fish_hash(side*3.1+seed*7.));
      let pulse = 0.5 - 0.5*cos(FISH_TAU*i.swim.x*0.8-fw*0.9);
      let whim = fin_whim(select(0.57,0.13,side>0.)+seed*3.1,fin_clock);
      let ang = k.jaw.w+0.03+slow*0.08+slow*0.3*pulse+0.6*(0.25-0.15*fold)
        +wobble*0.36*sin(FISH_TAU*(hz*fin_clock+select(0.7,0.2,side>0.))) - 0.55*0.45*whim;
      axis = normalize(vec3f(0.2,1.,0.)); angle = side*fw*max(ang,0.);
    } else if (id==5.) {
      axis=vec3f(1.,0.,0.); angle=-side*fw*(mix(-0.12,0.55,slow*0.5)+0.04*sin(4.4*t+seed));
    } else if (id==3.) { angle=a.fin_jaw.z*0.22*(1.-clamp(i.swim.y*1.5,0.,1.)); }
    else { angle=a.fin_jaw.z*(1.-(0.15+slow*0.6)); }
    p=pivot+rotate_about(p-pivot,axis,angle); n=rotate_about(n,axis,angle);
  }
  let h=a.pivot_height.w; let fl=a.flutter;
  if (h!=0. && id!=4. && fl.z>=0. && !still) {
    let mhz=mix(0.35,0.8,fish_hash(fl.z*5.+seed*11.));
    let phase=FISH_TAU*(mhz*fin_clock+fish_hash(fl.z*3.+seed*5.));
    let tilt=sign(h)*(0.17*(sin(phase)+0.5*sin(2.3*phase+1.1))+clamp(i.swim.z*0.3,-0.35,0.35));
    let hz=mix(0.2,0.6,fish_hash(fl.z*9.+seed*13.));
    let fp=FISH_TAU*(hz*fin_clock+fish_hash(fl.z*4.+seed*3.));
    let fold_angle=clamp(fold+wobble*(0.7*sin(fp)+0.3*sin(2.6*fp+0.8))+0.55*fin_whim(fl.z+seed*3.1,fin_clock),0.,1.)*k.eye_fin.z;
    p+=vec3f(-abs(h)*sin(fold_angle),h*(cos(fold_angle)*cos(tilt)-1.),h*cos(fold_angle)*sin(tilt));
    n=rotate_about(n,vec3f(1.,0.,0.),tilt);
  }
  if (fl.x>0. && !still) {
    let along=(k.center_len.x+0.5*len-p.x)/len;
    let phase=FISH_TAU*(2.5*fin_clock-along*2.5-p.y/len*1.5);
    let amp=len*k.fin_style.x*pow(fl.x,1.5)*select(1.,0.1,id==4.);
    var delta=amp*(sin(phase)+0.35*sin(1.7*phase+1.3));
    let fid=select(fl.z,select(0.77,0.31,pivot.z>0.),id==4.);
    if (fid>=0.) {
      let hz=mix(1.1,2.3,fish_hash(fid*13.+seed*31.));
      let ph=FISH_TAU*(hz*fin_clock+fish_hash(fid*7.+seed*17.+3.)-fl.y*0.8-fl.x*0.3);
      let w=fl.x*(0.35+0.65*fl.y);
      delta+=len*k.fin_style.y*w*w*select(1.,0.6,id==4.)*(sin(ph)+0.45*sin(1.73*ph+2.1+FISH_TAU*0.37*fin_clock));
    }
    if (id==4.) {p+=n*delta;} else {p.z+=delta;}
  }
  let mouth=(k.jaw.z+0.35)*i.face.y;
  let jaw_weight=select(select(0.,a.fin_jaw.w,id==0.),1.,id==8.);
  if (jaw_weight>0.) {
    let angle=(k.jaw.z-min(mouth,k.jaw.z+0.35))*jaw_weight;
    p=vec3f(k.jaw.xy+rot_xy(p.xy-k.jaw.xy,angle),p.z); n=vec3f(rot_xy(n.xy,angle),n.z);
  } else if (id==9.) {
    let open=clamp(mouth/max(k.upper2.y,1e-6),0.,1.); let angle=k.upper2.x*open*open-k.upper.z;
    let shifted=p.xy+vec2f(0.94,-0.34)*k.upper2.w*open*open;
    p=vec3f(k.upper.xy+rot_xy(shifted-k.upper.xy,angle),p.z); n=vec3f(rot_xy(n.xy,angle),n.z);
  }
  let local=(p-k.center_len.xyz)/len;
  let sp=swim_pos(vec3f(-local.z,local.y,local.x),i.swim);
  n=vec3f(-n.z,n.y,n.x);
  return FishPose(sp.xyz,normalize(vec3f(n.x+sp.w*n.z,n.y,n.z)));
}

@vertex fn vs(v: VIn, a: Anatomy, i: FIn) -> Out {
  let m = mat4x4f(i.m0, i.m1, i.m2, i.m3);
  let fish = fish_pose(v,a,i);
  let nn = fish.normal;
  let w = m * vec4f(fish.pos, 1.0);
  var o: Out;
  o.pos = g.view_proj * w;
  o.world = w.xyz;
  o.n = normal_world(m, nn);
  o.uv = v.uv;
  o.fx = i.fx;
  o.part_rim = a.fin_jaw.xz;
  return o;
}

@fragment fn fs(i: Out, @builtin(front_facing) front: bool) -> @location(0) vec4f {
  let t = textureSample(map_t, map_s, i.uv);
  // ひれは切り抜いた板（アルファで抜く。両面）
  if (t.a < 0.5) { discard; }
  var alb = t.rgb;
  if (!front && i.part_rim.x>7.5) { alb=vec3f(0.75,0.77,0.81); }
  if (!front && i.part_rim.x>6.5 && i.part_rim.x<7.5) { alb=vec3f(0.16,0.025,0.033); }
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
@vertex fn vs_line(v: VIn, a: Anatomy, i: FIn) -> Out {
  let m = mat4x4f(i.m0, i.m1, i.m2, i.m3);
  let fish = fish_pose(v,a,i);
  let nn = fish.normal;
  // 太らせた体でも線の太さがそろうよう、模型の中で押し出す（長さ 1 に対して 1.4%。金の縁の敵は少し太く）
  let w = m * vec4f(fish.pos + nn * (0.014 + i.fx.w * 0.006), 1.0);
  var o: Out;
  o.pos = g.view_proj * w;
  o.world = w.xyz;
  o.n = nn;
  o.uv = v.uv;
  o.fx = i.fx;
  o.part_rim = a.fin_jaw.xz;
  return o;
}
@fragment fn fs_line(i: Out) -> @location(0) vec4f {
  if (textureSample(map_t, map_s, i.uv).a < 0.5 || i.part_rim.x >= 6.) { discard; }
  return vec4f(apply_fog(vec3f(0.035, 0.03, 0.06), view_depth(i.world)), 1.0);
}

// ---- 影の型 ----
@vertex fn vs_shadow(v: VIn, a: Anatomy, i: FIn) -> @builtin(position) vec4f {
  let m = mat4x4f(i.m0, i.m1, i.m2, i.m3);
  return g.light_vp * m * vec4f(fish_pose(v,a,i).pos, 1.0);
}
