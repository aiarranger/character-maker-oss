// 凍結: character-actor 0.1.14（2026-11-12）の src/anim.rs の写し。NPC・汎用（character-runtime 0.2.5 の写しの決まり）だけが使う。主役の crate の直しをここへ持ち込まない（Owner 2026-11-12「種類ごとに分ける」）
// 骨と動作の小さな部品（three の Object3D の TRS・AnimationMixer・PropertyMixer・KeyframeTrack の写し）。主役（actor.rs）とゲームの骨のある敵が使う。
// 値の混ぜ方・時刻の進み方は three r178 と同じ。毎コマ呼ばれるので、作ったあとは確保しない。
use crate::glb::{self, Glb, Interp, Path};
use glam::{DVec3, DVec4, Mat4, Quat, Vec3};
use serde_json::Value;
use std::sync::Arc;

/// three の PropertyBinding.sanitizeNodeName
pub fn sanitize_name(s: &str) -> String {
    s.chars().filter_map(|c| if c.is_whitespace() { Some('_') } else if matches!(c, '[' | ']' | '.' | ':' | '/') { None } else { Some(c) }).collect()
}

/// GLTFLoader が節点に付ける名前（記号を落とし、重なれば _1, _2 ...）
fn three_names(g: &Glb) -> Vec<String> {
    let mut used = std::collections::HashSet::new();
    let mut out = vec![String::new(); g.nodes.len()];
    fn go(g: &Glb, i: usize, used: &mut std::collections::HashSet<String>, out: &mut [String]) {
        if !g.nodes[i].name.is_empty() {
            let base = sanitize_name(&g.nodes[i].name);
            let mut name = base.clone();
            let mut k = 1;
            while used.contains(&name) { name = format!("{base}_{k}"); k += 1; }
            used.insert(name.clone());
            out[i] = name;
        }
        for &c in &g.nodes[i].children { go(g, c, used, out); }
    }
    for &r in &g.scene { go(g, r, &mut used, &mut out); }
    out
}

/// 節点の木（親子と局所の TRS と世界の行列）。番号は GLB の節点の番号のまま
#[derive(Clone)]
pub struct Skeleton {
    pub names: Vec<String>,
    pub parent: Vec<Option<usize>>,
    pub children: Vec<Vec<usize>>,
    /// 親が先に来る並び（シーンにある節点だけ）
    pub order: Vec<usize>,
    pub t: Vec<Vec3>,
    pub r: Vec<Quat>,
    pub s: Vec<Vec3>,
    /// three の morphTargetInfluences（モーフを持つ節点だけ長さがある）
    pub morphs: Vec<Vec<f32>>,
    /// three の matrixAutoUpdate = false の節点（揺れ物の骨）の局所の行列。TRS を書いても描く姿は変わらない（spring.rs が書く）
    pub fixed: Vec<Option<Mat4>>,
    pub world: Vec<Mat4>,
}

impl Skeleton {
    pub fn from_glb(g: &Glb) -> Skeleton {
        let n = g.nodes.len();
        let mut parent = vec![None; n];
        for (i, node) in g.nodes.iter().enumerate() { for &c in &node.children { parent[c] = Some(i); } }
        let mut order = Vec::with_capacity(n);
        fn go(g: &Glb, i: usize, out: &mut Vec<usize>) { out.push(i); for &c in &g.nodes[i].children { go(g, c, out); } }
        for &r in &g.scene { go(g, r, &mut order); }
        let morphs = g.nodes.iter().map(|node| match node.mesh {
            Some(m) => {
                let mesh = &g.meshes[m];
                let count = mesh.primitives.iter().map(|p| p.morph_pos.len()).max().unwrap_or(0);
                let mut w = vec![0.0; count];
                for (i, v) in mesh.weights.iter().enumerate() { if i < count { w[i] = *v; } }
                w
            }
            None => Vec::new(),
        }).collect();
        let mut s = Skeleton {
            names: three_names(g),
            parent,
            children: g.nodes.iter().map(|x| x.children.clone()).collect(),
            order,
            t: g.nodes.iter().map(|x| x.t).collect(),
            r: g.nodes.iter().map(|x| x.r).collect(),
            s: g.nodes.iter().map(|x| x.s).collect(),
            morphs,
            fixed: vec![None; n],
            world: vec![Mat4::IDENTITY; n],
        };
        s.update_world(Mat4::IDENTITY);
        s
    }

    /// three の getObjectByName（名前は three の読んだ名前）
    pub fn find(&self, name: &str) -> Option<usize> {
        if name.is_empty() { return None; }
        self.order.iter().copied().find(|&i| self.names[i] == name)
    }

    #[inline]
    pub fn local(&self, i: usize) -> Mat4 { match self.fixed[i] { Some(m) => m, None => Mat4::from_scale_rotation_translation(self.s[i], self.r[i], self.t[i]) } }

    /// 世界の行列を親から順に作り直す。root はシーンの親の世界の行列
    pub fn update_world(&mut self, root: Mat4) {
        for k in 0..self.order.len() {
            let i = self.order[k];
            let l = self.local(i);
            self.world[i] = match self.parent[i] { Some(p) => self.world[p] * l, None => root * l };
        }
    }

    /// 節点とその下の世界の行列だけ作り直す（three の bone.updateMatrixWorld(true)）
    pub fn update_subtree(&mut self, i: usize, root: Mat4) {
        let l = self.local(i);
        self.world[i] = match self.parent[i] { Some(p) => self.world[p] * l, None => root * l };
        for k in 0..self.children[i].len() { let c = self.children[i][k]; self.update_subtree(c, root); }
    }

    /// 節点の世界の回転（行列から。three の getWorldQuaternion）
    pub fn world_rotation(&self, i: usize) -> Quat { three_rotation(self.world[i]) }
}

/// three の Matrix4.decompose の回転（列の長さで割ってから Quaternion.setFromRotationMatrix）。
/// 親が回って伸び縮みしていると行列は純粋な回転でなくなり、読む式で値が変わる。glam の to_scale_rotation_translation は別の式なので使わない（返す四元数の長さも 1 ではない）
pub fn three_rotation(m: Mat4) -> Quat { three_decompose(m).1 }

/// three の Matrix4.decompose（大きさ・回転・位置）
pub fn three_decompose(m: Mat4) -> (Vec3, Quat, Vec3) {
    let c = m.to_cols_array().map(f64::from);
    let (x, y, z) = (DVec3::new(c[0], c[1], c[2]), DVec3::new(c[4], c[5], c[6]), DVec3::new(c[8], c[9], c[10]));
    let t = Vec3::new(m.w_axis.x, m.w_axis.y, m.w_axis.z);
    let det = x.dot(y.cross(z));
    if det == 0.0 { return (Vec3::ONE, Quat::IDENTITY, t); }
    let (sx, sy, sz) = (if det < 0.0 { -x.length() } else { x.length() }, y.length(), z.length());
    let (m11, m21, m31) = (x.x / sx, x.y / sx, x.z / sx);
    let (m12, m22, m32) = (y.x / sy, y.y / sy, y.z / sy);
    let (m13, m23, m33) = (z.x / sz, z.y / sz, z.z / sz);
    let trace = m11 + m22 + m33;
    let q = if trace > 0.0 {
        let s = 0.5 / (trace + 1.0).sqrt();
        DVec4::new((m32 - m23) * s, (m13 - m31) * s, (m21 - m12) * s, 0.25 / s)
    } else if m11 > m22 && m11 > m33 {
        let s = 2.0 * (1.0 + m11 - m22 - m33).sqrt();
        DVec4::new(0.25 * s, (m12 + m21) / s, (m13 + m31) / s, (m32 - m23) / s)
    } else if m22 > m33 {
        let s = 2.0 * (1.0 + m22 - m11 - m33).sqrt();
        DVec4::new((m12 + m21) / s, 0.25 * s, (m23 + m32) / s, (m13 - m31) / s)
    } else {
        let s = 2.0 * (1.0 + m33 - m11 - m22).sqrt();
        DVec4::new((m13 + m31) / s, (m23 + m32) / s, 0.25 * s, (m21 - m12) / s)
    };
    (Vec3::new(sx as f32, sy as f32, sz as f32), Quat::from_xyzw(q.x as f32, q.y as f32, q.z as f32, q.w as f32), t)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Prop { T, R, S, W }

/// 1 本のキーフレーム
#[derive(Clone)]
pub struct Track {
    pub node: usize,
    pub prop: Prop,
    pub interp: Interp,
    pub times: Arc<Vec<f32>>,
    pub values: Arc<Vec<f32>>,
    /// 1 キーの値の数（Cubic は接線を除いた数）
    pub size: usize,
}

/// three の Quaternion.slerpFlat（値の並びが同じなら結果も同じ）
#[inline]
pub fn slerp_flat(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    if t == 0.0 { return a; }
    if t == 1.0 { return b; }
    if a == b { return a; }
    let mut s = 1.0 - t;
    let cos = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let dir = if cos >= 0.0 { 1.0 } else { -1.0 };
    let sqr_sin = 1.0 - cos * cos;
    let mut tt = t;
    if sqr_sin > f32::EPSILON {
        let sin = sqr_sin.sqrt();
        let len = sin.atan2(cos * dir);
        s = (s * len).sin() / sin;
        tt = (t * len).sin() / sin;
    }
    let td = tt * dir;
    let mut o = [a[0] * s + b[0] * td, a[1] * s + b[1] * td, a[2] * s + b[2] * td, a[3] * s + b[3] * td];
    if s == 1.0 - t {
        let f = 1.0 / (o[0] * o[0] + o[1] * o[1] + o[2] * o[2] + o[3] * o[3]).sqrt();
        for v in &mut o { *v *= f; }
    }
    o
}

impl Track {
    /// 時刻 t の値を out へ（three の Interpolant.evaluate。範囲の外は端の値）
    pub fn eval(&self, t: f32, out: &mut [f32]) {
        self.eval_interval(t, out, None);
    }

    // The optional cursor is the upper key index. Only the search changes;
    // endpoint handling and interpolation stay shared with the uncached path.
    fn eval_interval(&self, t: f32, out: &mut [f32], cursor: Option<&mut usize>) {
        let n = self.times.len();
        let size = self.size;
        let stride = if self.interp == Interp::Cubic { size * 3 } else { size };
        let first = if self.interp == Interp::Cubic { size } else { 0 };
        let at = |i: usize| &self.values[i * stride + first..i * stride + first + size];
        if n == 1 || t < self.times[0] { out[..size].copy_from_slice(at(0)); return; }
        if t >= self.times[n - 1] { out[..size].copy_from_slice(at(n - 1)); return; }
        let i1 = if let Some(cursor) = cursor {
            let i = *cursor;
            let contains = |i: usize| i > 0 && i < n && self.times[i - 1] <= t && t < self.times[i];
            let next = if contains(i) { i } else if contains(i + 1) { i + 1 }
                else { self.times.partition_point(|x| *x <= t) };
            *cursor = next;
            next
        } else { self.times.partition_point(|x| *x <= t) };
        let (t0, t1) = (self.times[i1 - 1], self.times[i1]);
        match self.interp {
            Interp::Step => out[..size].copy_from_slice(at(i1 - 1)),
            Interp::Linear => {
                let k = (t - t0) / (t1 - t0);
                if self.prop == Prop::R {
                    let (a, b) = (at(i1 - 1), at(i1));
                    let o = slerp_flat([a[0], a[1], a[2], a[3]], [b[0], b[1], b[2], b[3]], k);
                    out[..4].copy_from_slice(&o);
                } else {
                    let (a, b) = (at(i1 - 1), at(i1));
                    for c in 0..size { out[c] = a[c] * (1.0 - k) + b[c] * k; }
                }
            }
            Interp::Cubic => {
                // three の GLTFCubicSplineInterpolant（[入りの接線, 値, 出の接線] の並び）
                let td = t1 - t0;
                let p = (t - t0) / td;
                let (pp, ppp) = (p * p, p * p * p);
                let s2 = -2.0 * ppp + 3.0 * pp;
                let s3 = ppp - pp;
                let s0 = 1.0 - s2;
                let s1 = s3 - pp + p;
                let (o1, o0) = (i1 * stride, (i1 - 1) * stride);
                for c in 0..size {
                    let p0 = self.values[o0 + c + size];
                    let m0 = self.values[o0 + c + size * 2] * td;
                    let p1 = self.values[o1 + c + size];
                    let m1 = self.values[o1 + c] * td;
                    out[c] = s0 * p0 + s1 * m0 + s2 * p1 + s3 * m1;
                }
                if self.prop == Prop::R {
                    let f = 1.0 / (out[0] * out[0] + out[1] * out[1] + out[2] * out[2] + out[3] * out[3]).sqrt();
                    for c in 0..4 { out[c] *= f; }
                }
            }
        }
    }
}

/// 節点に結びついた動作（three の AnimationClip + PropertyBinding）
#[derive(Clone)]
pub struct Clip {
    pub name: String,
    pub duration: f32,
    /// three の clip.duration そのまま（JSON の動きの長さは倍精度。動作の終わりのコマを TS とそろえる）
    pub seconds: f64,
    pub tracks: Vec<Track>,
}

impl Clip {
    /// GLB の動作をそのモデルの骨へ。weights はその節点とその下のモーフを持つ節点の全部へ（GLTFLoader と同じ）
    pub fn from_glb(a: &glb::Animation, skel: &Skeleton) -> Clip {
        let mut tracks = Vec::with_capacity(a.channels.len());
        for ch in &a.channels {
            let (times, values) = (Arc::new(ch.times.clone()), Arc::new(ch.values.clone()));
            let keys = ch.times.len().max(1) * if ch.interp == Interp::Cubic { 3 } else { 1 };
            let (prop, size) = match ch.path {
                Path::Translation => (Prop::T, 3),
                Path::Rotation => (Prop::R, 4),
                Path::Scale => (Prop::S, 3),
                Path::Weights => (Prop::W, ch.values.len() / keys),
            };
            if prop == Prop::W {
                fn targets(s: &Skeleton, i: usize, out: &mut Vec<usize>) {
                    if !s.morphs[i].is_empty() { out.push(i); }
                    for &c in &s.children[i] { targets(s, c, out); }
                }
                let mut ts = Vec::new();
                targets(skel, ch.node, &mut ts);
                for node in ts { tracks.push(Track { node, prop, interp: ch.interp, times: times.clone(), values: values.clone(), size }); }
            } else {
                tracks.push(Track { node: ch.node, prop, interp: ch.interp, times: times.clone(), values: values.clone(), size });
            }
        }
        Clip { name: a.name.clone(), duration: a.duration, seconds: a.duration as f64, tracks }
    }

    pub fn find<'a>(clips: &'a [Arc<Clip>], name: &str) -> Option<&'a Arc<Clip>> { clips.iter().find(|c| c.name == name) }

    /// そのモデルの全部の動作
    pub fn all_from_glb(g: &Glb, skel: &Skeleton) -> Vec<Arc<Clip>> { g.animations.iter().map(|a| Arc::new(Clip::from_glb(a, skel))).collect() }
}

/// 骨の名前でつながる動作（配布の JSON・別の GLB。three の AnimationClip.parse の結果）。骨へ当てるまで
#[derive(Clone)]
pub struct NamedTrack {
    pub node: String,
    pub prop: Prop,
    pub interp: Interp,
    pub times: Arc<Vec<f32>>,
    pub values: Arc<Vec<f32>>,
    pub size: usize,
}

#[derive(Clone)]
pub struct NamedClip {
    pub name: String,
    pub duration: f32,
    /// three の clip.duration そのまま（Clip の seconds）
    pub seconds: f64,
    pub tracks: Vec<NamedTrack>,
}

fn floats(v: &Value) -> Vec<f32> { v.as_array().map(|a| a.iter().map(|x| x.as_f64().unwrap_or(f64::NAN) as f32).collect()).unwrap_or_default() }

impl NamedClip {
    /// three の AnimationClip.parse。壊れた行は null
    pub fn from_json(c: &Value) -> Option<NamedClip> {
        let name = c.get("name")?.as_str()?.to_string();
        let mut tracks = Vec::new();
        for t in c.get("tracks")?.as_array()? {
            let full = t.get("name")?.as_str()?;
            let dot = full.rfind('.')?;
            let (node, prop) = (&full[..dot], &full[dot + 1..]);
            let (prop, size) = match prop { "position" => (Prop::T, 3), "quaternion" => (Prop::R, 4), "scale" => (Prop::S, 3), _ => continue };
            let times = floats(t.get("times")?);
            let values = floats(t.get("values")?);
            let interp = match t.get("interpolation").and_then(Value::as_i64) { Some(2300) => Interp::Step, _ => Interp::Linear };
            tracks.push(NamedTrack { node: node.to_string(), prop, interp, times: Arc::new(times), values: Arc::new(values), size });
        }
        let d = c.get("duration").and_then(Value::as_f64).unwrap_or(-1.0);
        let seconds = if d < 0.0 { tracks.iter().map(|t| t.times.last().copied().unwrap_or(0.0)).fold(0.0, f32::max) as f64 } else { d };
        Some(NamedClip { name, duration: seconds as f32, seconds, tracks })
    }

    /// 別の GLB の動作を名前で読む（動作だけのファイル）
    pub fn from_glb(g: &Glb, name: &str) -> Option<NamedClip> {
        let names = three_names(g);
        let a = g.animations.iter().find(|a| a.name == name)?;
        let mut tracks = Vec::with_capacity(a.channels.len());
        for ch in &a.channels {
            let (prop, size) = match ch.path { Path::Translation => (Prop::T, 3), Path::Rotation => (Prop::R, 4), Path::Scale => (Prop::S, 3), Path::Weights => continue };
            tracks.push(NamedTrack { node: names[ch.node].clone(), prop, interp: ch.interp, times: Arc::new(ch.times.clone()), values: Arc::new(ch.values.clone()), size });
        }
        Some(NamedClip { name: name.to_string(), duration: a.duration, seconds: a.duration as f64, tracks })
    }
}

// ---- three の AnimationMixer ----

/// 動作の番号（Mixer::add が返す）
pub type ActionId = usize;

#[derive(Clone)]
struct Binding { node: usize, prop: Prop, off: usize, size: usize, weight: f32, uses: u32 }

/// Mutable sampling state belongs to an Action, never to the shared Clip.
#[derive(Clone)]
struct IntervalCache {
    upper_keys: Vec<usize>,
    sampled_time: Option<f64>,
}

/// three の AnimationAction のうち使う分
#[derive(Clone)]
pub struct Action {
    pub clip: Arc<Clip>,
    pub time: f64,
    pub time_scale: f32,
    pub weight: f32,
    pub enabled: bool,
    pub paused: bool,
    pub once: bool,
    pub clamp_when_finished: bool,
    pub effective_weight: f32,
    loop_count: i64,
    /// (t0, v0, t1, v1) の重みの変化
    fade: Option<(f64, f32, f64, f32)>,
    /// トラックごとの Binding の番号
    bind: Vec<u32>,
    active: bool,
    interval_cache: Option<IntervalCache>,
}

#[derive(Clone)]
pub struct Mixer {
    pub time: f64,
    /// 倍精度（TS と同じ。f32 に丸めると時刻がわずかに遅れ、クリップの終わりを越えるコマが 1 つずれる。始めと終わりの姿が違うクリップで骨が 1 コマ跳ぶ）
    pub time_scale: f64,
    pub actions: Vec<Action>,
    active: Vec<ActionId>,
    bindings: Vec<Binding>,
    acc: Vec<f32>,
    orig: Vec<f32>,
    scratch: Vec<f32>,
}

impl Default for Mixer { fn default() -> Self { Self::new() } }

impl Mixer {
    pub fn new() -> Mixer { Mixer { time: 0.0, time_scale: 1.0, actions: Vec::new(), active: Vec::new(), bindings: Vec::new(), acc: Vec::new(), orig: Vec::new(), scratch: Vec::new() } }

    /// three の mixer.clipAction（再生はまだ）
    pub fn add(&mut self, clip: Arc<Clip>) -> ActionId {
        let mut bind = Vec::with_capacity(clip.tracks.len());
        for t in &clip.tracks {
            let found = self.bindings.iter().position(|b| b.node == t.node && b.prop == t.prop);
            let i = found.unwrap_or_else(|| {
                let off = self.acc.len();
                self.acc.resize(off + t.size, 0.0);
                self.orig.resize(off + t.size, 0.0);
                if self.scratch.len() < t.size { self.scratch.resize(t.size, 0.0); }
                self.bindings.push(Binding { node: t.node, prop: t.prop, off, size: t.size, weight: 0.0, uses: 0 });
                self.bindings.len() - 1
            });
            bind.push(i as u32);
        }
        self.actions.push(Action { clip, time: 0.0, time_scale: 1.0, weight: 1.0, enabled: true, paused: false, once: false, clamp_when_finished: false, effective_weight: 1.0, loop_count: -1, fade: None, bind, active: false, interval_cache: None });
        self.actions.len() - 1
    }

    /// Opt-in interval reuse for this Action only. Default is the original search.
    /// Switching modes never changes playback time, weights, or pose.
    pub fn set_interval_cache(&mut self, id: ActionId, enabled: bool) {
        let a = &mut self.actions[id];
        if enabled && a.interval_cache.is_none() {
            a.interval_cache = Some(IntervalCache { upper_keys: vec![0; a.clip.tracks.len()], sampled_time: None });
        } else if !enabled {
            a.interval_cache = None;
        }
    }

    /// action.play()。節点の今の値を「元の姿」として控える（最初に使うときだけ）
    pub fn play(&mut self, id: ActionId, skel: &Skeleton) {
        if self.actions[id].active { return; }
        self.actions[id].active = true;
        self.active.push(id);
        for k in 0..self.actions[id].bind.len() {
            let i = self.actions[id].bind[k] as usize;
            if self.bindings[i].uses == 0 {
                let b = &self.bindings[i];
                read_prop(skel, b.node, b.prop, &mut self.orig[b.off..b.off + b.size]);
            }
            self.bindings[i].uses += 1;
        }
    }

    /// action.stop()（three の _deactivateAction と reset。使う動作が無くなった骨は、play のときに控えた元の姿へ戻す）
    pub fn stop(&mut self, id: ActionId, skel: &mut Skeleton) {
        if self.actions[id].active {
            self.actions[id].active = false;
            self.active.retain(|a| *a != id);
            for k in 0..self.actions[id].bind.len() {
                let b = &mut self.bindings[self.actions[id].bind[k] as usize];
                b.uses -= 1;
                if b.uses == 0 { write_prop(skel, b.node, b.prop, &self.orig[b.off..b.off + b.size]); }
            }
        }
        self.reset(id);
    }
    /// mixer.stopAllAction()
    pub fn stop_all(&mut self, skel: &mut Skeleton) {
        while let Some(&id) = self.active.last() { self.stop(id, skel); }
    }

    /// action.reset()
    pub fn reset(&mut self, id: ActionId) {
        let a = &mut self.actions[id];
        a.paused = false; a.enabled = true; a.time = 0.0; a.loop_count = -1; a.fade = None;
        if let Some(c) = &mut a.interval_cache { c.sampled_time = None; }
    }

    /// action.setEffectiveWeight(w)
    pub fn set_effective_weight(&mut self, id: ActionId, w: f32) {
        let a = &mut self.actions[id];
        a.weight = w;
        a.effective_weight = if a.enabled { w } else { 0.0 };
        a.fade = None;
    }

    /// action.fadeIn(duration)
    pub fn fade_in(&mut self, id: ActionId, dur: f32) { let now = self.time; self.actions[id].fade = Some((now, 0.0, now + dur as f64, 1.0)); }
    /// action.fadeOut(duration)
    pub fn fade_out(&mut self, id: ActionId, dur: f32) { let now = self.time; self.actions[id].fade = Some((now, 1.0, now + dur as f64, 0.0)); }
    /// to.crossFadeFrom(from, duration, false)
    pub fn cross_fade_from(&mut self, to: ActionId, from: ActionId, dur: f32) { self.fade_out(from, dur); self.fade_in(to, dur); }

    /// mixer.update(dt)。動作の値を混ぜて骨へ書く（世界の行列は呼ぶ側が update_world）
    pub fn update(&mut self, dt: f32, skel: &mut Skeleton) { self.advance(dt as f64, skel); }

    /// update と同じ（時刻を f64 で渡す）
    pub fn advance(&mut self, dt: f64, skel: &mut Skeleton) {
        let dt = dt * self.time_scale;
        self.time += dt;
        let time = self.time;
        for k in 0..self.active.len() {
            let id = self.active[k];
            let a = &mut self.actions[id];
            if !a.enabled { update_weight(a, time); continue; }
            let scaled = dt * if a.paused { 0.0 } else { a.time_scale as f64 };
            let previous_time = a.time;
            let clip_time = update_time(a, scaled);
            let w = update_weight(a, time);
            if w > 0.0 {
                // A loop (including a whole-loop jump), rewind, explicit seek or
                // unsampled gap starts with the original binary search. Within
                // continuous playback each track tries just its current/next interval.
                let reuse = a.interval_cache.as_ref().is_some_and(|c|
                    c.sampled_time == Some(previous_time) && scaled >= 0.0
                    && previous_time + scaled < a.clip.duration as f64);
                for (j, t) in a.clip.tracks.iter().enumerate() {
                    let b = &mut self.bindings[a.bind[j] as usize];
                    if let Some(c) = &mut a.interval_cache {
                        if !reuse {
                            // Seed from the existing search, even when the old
                            // cursor happens to lie in this loop's first interval.
                            c.upper_keys[j] = t.times.partition_point(|x| *x <= clip_time as f32);
                        }
                        t.eval_interval(clip_time as f32, &mut self.scratch, Some(&mut c.upper_keys[j]));
                    } else {
                        t.eval(clip_time as f32, &mut self.scratch);
                    }
                    let acc = &mut self.acc[b.off..b.off + b.size];
                    if b.weight == 0.0 {
                        acc.copy_from_slice(&self.scratch[..b.size]);
                        b.weight = w;
                    } else {
                        b.weight += w;
                        mix_region(b.prop, acc, &self.scratch[..b.size], w / b.weight);
                    }
                }
                if let Some(c) = &mut a.interval_cache { c.sampled_time = Some(clip_time); }
            }
        }
        for b in &mut self.bindings {
            if b.uses == 0 { continue; }
            let acc = &mut self.acc[b.off..b.off + b.size];
            let w = b.weight;
            b.weight = 0.0;
            if w < 1.0 { mix_region(b.prop, acc, &self.orig[b.off..b.off + b.size], 1.0 - w); }
            write_prop(skel, b.node, b.prop, acc);
        }
    }
}

fn mix_region(prop: Prop, acc: &mut [f32], src: &[f32], t: f32) {
    if prop == Prop::R {
        let o = slerp_flat([acc[0], acc[1], acc[2], acc[3]], [src[0], src[1], src[2], src[3]], t);
        acc[..4].copy_from_slice(&o);
    } else {
        let s = 1.0 - t;
        for (a, b) in acc.iter_mut().zip(src) { *a = *a * s + *b * t; }
    }
}

fn read_prop(s: &Skeleton, node: usize, prop: Prop, out: &mut [f32]) {
    match prop {
        Prop::T => out[..3].copy_from_slice(&s.t[node].to_array()),
        Prop::S => out[..3].copy_from_slice(&s.s[node].to_array()),
        Prop::R => out[..4].copy_from_slice(&s.r[node].to_array()),
        Prop::W => { let n = out.len().min(s.morphs[node].len()); out[..n].copy_from_slice(&s.morphs[node][..n]); }
    }
}

fn write_prop(s: &mut Skeleton, node: usize, prop: Prop, v: &[f32]) {
    match prop {
        Prop::T => s.t[node] = Vec3::new(v[0], v[1], v[2]),
        Prop::S => s.s[node] = Vec3::new(v[0], v[1], v[2]),
        Prop::R => s.r[node] = Quat::from_xyzw(v[0], v[1], v[2], v[3]),
        Prop::W => { let n = v.len().min(s.morphs[node].len()); s.morphs[node][..n].copy_from_slice(&v[..n]); }
    }
}

fn update_weight(a: &mut Action, time: f64) -> f32 {
    let mut w = 0.0;
    if a.enabled {
        w = a.weight;
        if let Some((t0, v0, t1, v1)) = a.fade {
            let v = if time < t0 { v0 } else if time >= t1 { v1 } else { v0 + (v1 - v0) * ((time - t0) / (t1 - t0)) as f32 };
            w *= v;
            if time > t1 {
                a.fade = None;
                if v == 0.0 { a.enabled = false; }
            }
        }
    }
    a.effective_weight = w;
    w
}

fn update_time(a: &mut Action, dt: f64) -> f64 {
    let duration = a.clip.duration as f64;
    let mut time = a.time + dt;
    if dt == 0.0 { return time; }
    if a.once {
        if a.loop_count == -1 { a.loop_count = 0; }
        if time >= duration { time = duration; } else if time < 0.0 { time = 0.0; } else { a.time = time; return time; }
        if a.clamp_when_finished { a.paused = true; } else { a.enabled = false; }
        a.time = time;
    } else {
        let mut lc = a.loop_count;
        if lc == -1 && dt >= 0.0 { lc = 0; }
        if time >= duration || time < 0.0 {
            let d = (time / duration).floor();
            time -= duration * d;
            lc += d.abs() as i64;
            a.loop_count = lc;
            a.time = time;
        } else {
            a.time = time;
        }
    }
    time
}

// ---- 骨の行列の表 ----

/// スキンの骨の行列（骨の世界 × 逆バインド）。同じ骨と逆バインドは 1 つにまとめる（主役はメッシュごとに skin があるが中身は同じ）
#[derive(Clone, Default)]
pub struct Palette {
    entries: Vec<(u32, Mat4)>,
    pub mats: Vec<Mat4>,
}

impl Palette {
    pub fn new() -> Palette { Palette::default() }
    /// 表に足して番号を返す
    pub fn add(&mut self, node: usize, ibm: Mat4) -> u16 {
        let bits = ibm.to_cols_array();
        if let Some(i) = self.entries.iter().position(|(n, m)| *n as usize == node && m.to_cols_array() == bits) { return i as u16; }
        self.entries.push((node as u32, ibm));
        self.mats.push(Mat4::IDENTITY);
        (self.entries.len() - 1) as u16
    }
    /// 頂点の JOINTS_0（skin の番号）を表の番号へ直す。重みが 0 の骨は 0 番にする
    pub fn remap_skin(&mut self, skin: &glb::Skin, joints: &[[u16; 4]], weights: &[[f32; 4]]) -> Vec<[u16; 4]> {
        joints.iter().zip(weights).map(|(j, w)| {
            let mut o = [0u16; 4];
            for k in 0..4 { if w[k] != 0.0 { o[k] = self.add(skin.joints[j[k] as usize], skin.inverse_bind[j[k] as usize]); } }
            o
        }).collect()
    }
    /// 今の骨で行列を作り直す
    pub fn update(&mut self, skel: &Skeleton) {
        for (i, (n, ibm)) in self.entries.iter().enumerate() { self.mats[i] = skel.world[*n as usize] * *ibm; }
    }
    pub fn len(&self) -> usize { self.entries.len() }
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
}

impl NamedClip {
    /// このモデルの骨へ当てる（three の PropertyBinding。骨に無いトラックは効かない）
    pub fn bind(&self, skel: &Skeleton) -> Clip {
        let tracks = self.tracks.iter().filter_map(|t| {
            let node = skel.find(&t.node)?;
            Some(Track { node, prop: t.prop, interp: t.interp, times: t.times.clone(), values: t.values.clone(), size: t.size })
        }).collect();
        Clip { name: self.name.clone(), duration: self.duration, seconds: self.seconds, tracks }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(prop: Prop, size: usize, times: &[f32], values: &[f32]) -> Track {
        Track { node: 0, prop, interp: Interp::Linear, times: Arc::new(times.to_vec()), values: Arc::new(values.to_vec()), size }
    }

    /// 2 本の骨（どちらも位置だけ）と、それを動かす動作
    fn two_bones() -> Skeleton {
        Skeleton {
            names: vec!["a".into(), "b".into()], parent: vec![None, None], children: vec![Vec::new(), Vec::new()], order: vec![0, 1],
            t: vec![Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 5.0, 6.0)], r: vec![Quat::IDENTITY; 2], s: vec![Vec3::ONE; 2], morphs: vec![Vec::new(); 2], fixed: vec![None; 2], world: vec![Mat4::IDENTITY; 2],
        }
    }
    fn clip_of(nodes: &[usize]) -> Arc<Clip> {
        let tracks = nodes.iter().map(|&node| Track { node, ..track(Prop::T, 3, &[0.0, 1.0], &[0.0, 0.0, 0.0, 10.0, 20.0, 30.0]) }).collect();
        Arc::new(Clip { name: "c".into(), duration: 1.0, seconds: 1.0, tracks })
    }

    /// 防ぐ故障: 動作を替えたとき、次の動作に無い骨が前の動作の姿勢のまま残る（three の stopAllAction は使う物が無くなった骨を元の姿へ戻す。BossActor の select のコメント）
    #[test]
    fn stop_all_restores_bones_the_next_clip_does_not_move() {
        let mut skel = two_bones();
        let mut mixer = Mixer::new();
        let (a, b) = (mixer.add(clip_of(&[0, 1])), mixer.add(clip_of(&[0])));
        mixer.play(a, &skel);
        mixer.actions[a].time = 0.5;
        mixer.advance(0.0, &mut skel);
        assert_eq!(skel.t[1], Vec3::new(5.0, 10.0, 15.0));
        mixer.stop_all(&mut skel);
        assert_eq!((skel.t[0], skel.t[1]), (Vec3::new(1.0, 2.0, 3.0), Vec3::new(4.0, 5.0, 6.0)));
        assert_eq!(mixer.actions[a].time, 0.0);
        mixer.play(b, &skel);
        mixer.actions[b].time = 1.0;
        mixer.advance(0.0, &mut skel);
        assert_eq!((skel.t[0], skel.t[1]), (Vec3::new(10.0, 20.0, 30.0), Vec3::new(4.0, 5.0, 6.0)));
        // 同じ動作をもう一度始めても、元の姿の控えは始めの姿のまま
        mixer.stop_all(&mut skel);
        mixer.play(a, &skel);
        mixer.advance(0.0, &mut skel);
        mixer.stop_all(&mut skel);
        assert_eq!(skel.t[1], Vec3::new(4.0, 5.0, 6.0));
    }

    #[test]
    fn slerp_and_vector_match_three() {
        // three: new QuaternionKeyframeTrack('.q',[0,1],[0,0,0,1, 0,0.7071068,0,0.7071068]).createInterpolant().evaluate(0.5)
        let t = track(Prop::R, 4, &[0.0, 1.0], &[0.0, 0.0, 0.0, 1.0, 0.0, 0.7071068, 0.0, 0.7071068]);
        let mut o = [0.0; 4];
        t.eval(0.5, &mut o);
        assert!((o[1] - 0.38268343).abs() < 1e-5 && (o[3] - 0.9238795).abs() < 1e-5, "{o:?}");
        let v = track(Prop::T, 3, &[1.0, 2.0], &[0.0, 0.0, 0.0, 2.0, 4.0, 6.0]);
        let mut o = [0.0; 3];
        v.eval(0.0, &mut o); assert_eq!(o, [0.0, 0.0, 0.0]);
        v.eval(1.5, &mut o); assert_eq!(o, [1.0, 2.0, 3.0]);
        v.eval(9.0, &mut o); assert_eq!(o, [2.0, 4.0, 6.0]);
    }

    fn varied_track(prop: Prop, interp: Interp) -> Track {
        let times = vec![0.125, 0.25, 0.25, 0.5, 0.75, 1.0];
        let size = match prop { Prop::R => 4, Prop::W => 2, _ => 3 };
        let mut values = Vec::new();
        for k in 0..times.len() {
            let v = if prop == Prop::R { Quat::from_rotation_y(k as f32 * 0.17).to_array().to_vec() }
                else { (0..size).map(|c| (k + c) as f32 * 0.2 + 0.1).collect() };
            if interp == Interp::Cubic { values.extend(vec![0.01; size]); }
            values.extend(v);
            if interp == Interp::Cubic { values.extend(vec![-0.02; size]); }
        }
        Track { interp, ..track(prop, size, &times, &values) }
    }

    #[test]
    fn cached_tracks_match_interpolation_at_boundaries_and_seeks() {
        for prop in [Prop::T, Prop::R, Prop::S, Prop::W] {
            for interp in [Interp::Linear, Interp::Step, Interp::Cubic] {
                let t = varied_track(prop, interp);
                let mut cursor = 0;
                // Includes duplicate-key upper-bound semantics, a same-interval
                // step, next interval, skipped intervals, rewind and endpoints.
                for time in [-1.0, 0.125, 0.126, 0.249, 0.25, 0.251, 0.5, 0.9, 1.0, 4.0, 0.13, 0.7] {
                    let (mut legacy, mut cached) = ([0.0; 4], [0.0; 4]);
                    t.eval(time, &mut legacy);
                    t.eval_interval(time, &mut cached, Some(&mut cursor));
                    assert_eq!(legacy.map(f32::to_bits), cached.map(f32::to_bits), "{prop:?} {interp:?} at {time}");
                }
                let mut one = t.clone();
                one.times = Arc::new(vec![0.125]);
                for time in [-1.0, 0.125, 4.0] {
                    let (mut legacy, mut cached) = ([0.0; 4], [0.0; 4]);
                    one.eval(time, &mut legacy);
                    one.eval_interval(time, &mut cached, Some(&mut cursor));
                    assert_eq!(legacy, cached);
                }
            }
        }
    }

    #[test]
    fn cached_mixer_matches_loops_switches_pause_reset_and_independent_actions() {
        let clips: Vec<_> = (0..3).map(|i| Arc::new(Clip {
            name: ["idle", "walk", "run"][i].into(), duration: 1.0 + i as f32 * 0.25, seconds: 1.0 + i as f64 * 0.25,
            tracks: [Prop::T, Prop::R, Prop::S, Prop::W].into_iter().enumerate().map(|(j, p)| {
                let mut t = varied_track(p, [Interp::Linear, Interp::Step, Interp::Cubic][i]);
                t.node = j % 2;
                t
            }).collect(),
        })).collect();
        let mut legacy = Mixer::new();
        let mut cached = Mixer::new();
        let mut a = two_bones();
        a.morphs = vec![vec![0.0; 2]; 2];
        let mut b = a.clone();
        let mut pa = Palette::new();
        for i in 0..2 { pa.add(i, Mat4::from_translation(Vec3::new(-0.2, 0.3, 0.1))); }
        let mut pb = pa.clone();
        for (i, clip) in clips.iter().enumerate() {
            assert_eq!(legacy.add(clip.clone()), i);
            assert_eq!(cached.add(clip.clone()), i);
            assert!(cached.actions[i].interval_cache.is_none(), "legacy is the default");
            cached.set_interval_cache(i, true);
            for m in [&mut legacy, &mut cached] {
                m.actions[i].time = i as f64 * 0.173;
                m.play(i, &a);
            }
        }
        for tick in 0..600 {
            for m in [&mut legacy, &mut cached] {
                for i in 0..3 {
                    m.set_effective_weight(i, if (tick / 80) % 3 == i { 0.8 } else { 0.1 });
                    m.actions[i].paused = (100..115).contains(&tick) && i == 1;
                }
                if tick == 160 { m.actions[0].time = 0.2; }
                if tick == 180 { m.actions[2].time = 0.95; }
                if tick == 220 { m.reset(1); }
                m.time_scale = if tick == 260 { 170.0 } else if tick == 280 { -1.0 } else { 1.0 };
            }
            if tick == 320 { cached.set_interval_cache(0, false); }
            if tick == 340 { cached.set_interval_cache(0, true); }
            let dt = if (400..415).contains(&tick) { 0.0 } else { 1.0 / 60.0 };
            legacy.advance(dt, &mut a);
            cached.advance(dt, &mut b);
            let root = Mat4::from_rotation_y(tick as f32 * 0.01);
            a.update_world(root); b.update_world(root);
            pa.update(&a); pb.update(&b);
            assert_eq!(a.t, b.t, "translation at {tick}");
            assert_eq!(a.r, b.r, "rotation at {tick}");
            assert_eq!(a.s, b.s, "scale at {tick}");
            assert_eq!(a.morphs, b.morphs, "morph at {tick}");
            assert_eq!(pa.mats, pb.mats, "palette at {tick}");
        }
        // The shared Clip has no cursor; a second Action starts independently.
        let extra = cached.add(clips[0].clone());
        cached.set_interval_cache(extra, true);
        assert_eq!(cached.actions[extra].interval_cache.as_ref().unwrap().sampled_time, None);
        assert!(cached.actions[0].interval_cache.as_ref().unwrap().sampled_time.is_some());
    }

    #[test]
    fn cloned_mixer_shares_clips_but_owns_playback_and_cursors() {
        let mut a = Mixer::new();
        let mut sa = two_bones();
        let id = a.add(clip_of(&[0, 1]));
        a.set_interval_cache(id, true);
        a.play(id, &sa);
        let b = a.clone();
        assert!(Arc::ptr_eq(&a.actions[id].clip, &b.actions[id].clip));
        assert!(Arc::ptr_eq(&a.actions[id].clip.tracks[0].values, &b.actions[id].clip.tracks[0].values));
        a.advance(0.25, &mut sa);
        assert_eq!(b.actions[id].time, 0.0);
        assert_eq!(b.actions[id].interval_cache.as_ref().unwrap().sampled_time, None);
        assert_eq!(b.actions[id].interval_cache.as_ref().unwrap().upper_keys, vec![0, 0]);
        assert_eq!(b.time, 0.0);
    }
}

