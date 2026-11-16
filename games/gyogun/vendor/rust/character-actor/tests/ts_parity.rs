// 配布中の主役の全員 ×（待機・走りながら・止まって）× 攻撃・のけぞり・回避・必殺技・バンザイ・手を振る と、のけぞりのいろいろな入り方（HIT_SCENES）の骨の値を、
// 同じ合図・同じ刻みで TS の部品（three + @aiarranger/character-actor）と比べる。防ぐ故障: ある主役・ある動きで体の動きが TS と変わる。
// HIT_SCENES はどのコマも比べ、入れ物が回って伸び縮みする場面（pinball の玉）も入れる。差が出たら、場面ごとに最大の差と最初のコマを出す。
// ゲームの入り方（tools/game-scenes.json。ゲームが読む lite で）: 動作を止めてからのけぞる・休むの途中・基準より長い seconds・60 コマ毎秒以外の刻み・pinball の玉と仲間の本当の入れ物。
// three の版はゲームが使う全部（THREE_VERSIONS）。別々に値を書き出して、どちらとも比べる。
// TS の値は tools/actor-parity.mjs が書き出す（無い・版が違うときはこの試験が動かす）。読むのは本番の共有配布（CHARACTER_INDEX_URL で替えられる）。
// 比べる値: 節点の位置と向き（部品の空間）・モーフの重み・持ち手の行列・合図・動きの一覧。
use character_actor::actor::{ActionKind, ActorEvent, CharacterActor, Cue, FaceControl, Gait, LoadOptions, MotionKind, Quality, StartOptions, load_character_actor};
use character_actor::delivery::{Catalog, PRODUCTION_INDEX_URL};
use glam::{DMat4, DQuat, DVec3, EulerRot, Mat4};
use serde_json::Value;
use std::path::PathBuf;

/// tools/actor-parity.mjs と同じ場面
const DT: f64 = 1.0 / 60.0;
const RUN: f32 = 4.0;
const RATE: f64 = 1.3;
const LEAD: i32 = 40;
const TAIL: i32 = 40;
const EVERY: i32 = 3;
/// 待機・走りながら（歩いてから走る）・止まって（走ってから、合図と同時に止まって構える）
const CONDITIONS: [&str; 3] = ["idle", "run", "stop"];
/// swing は Engimeow の壁を壊す振り（攻撃を SWING_SECONDS で）
const ACTIONS: [&str; 7] = ["attack", "swing", "hit", "dodge", "skill", "banzai", "greet"];
const SWING_SECONDS: f64 = 0.75;
/// のけぞりの入り方（tools/actor-parity.mjs の HIT_SCENES と同じ）。before: 先に始める動き。lead: のけぞるまでのコマ。grounded: false は宙（のけぞって 30 コマで地面へ戻る）。holder: 入れ物の動き。seconds・since: hit の指定（0 は省く）。again: もう一度のけぞるまでのコマ（0 は無し）
struct HitScene { name: &'static str, before: Option<&'static str>, lead: i32, speed: f32, grounded: bool, armed: f32, holder: &'static str, seconds: f64, since: f64, again: i32 }
const fn scene(name: &'static str, before: Option<&'static str>, lead: i32, speed: f32, grounded: bool, armed: f32, holder: &'static str) -> HitScene {
    HitScene { name, before, lead, speed, grounded, armed, holder, seconds: 0.0, since: 0.0, again: 0 }
}
const HIT_SCENES: [HitScene; 17] = [
    scene("ready", None, 40, 0.0, true, 1.0, "still"),
    scene("swing", Some("swing"), 20, 0.0, true, 1.0, "still"),
    scene("swing-run", Some("swing"), 20, RUN, true, 0.0, "still"),
    scene("jump", Some("jump"), 15, 0.0, false, 0.0, "still"),
    scene("air", None, 30, 0.0, false, 0.0, "still"),
    scene("dodge", Some("dodge"), 10, 0.0, true, 0.0, "still"),
    scene("skill", Some("skill"), 20, 0.0, true, 1.0, "still"),
    scene("banzai", Some("banzai"), 20, 0.0, true, 0.0, "still"),
    scene("rest", Some("rest"), 60, 0.0, true, 0.0, "still"),
    HitScene { seconds: 0.5, ..scene("short", None, 20, RUN, true, 0.0, "still") },
    HitScene { since: 0.2, ..scene("since", None, 20, 0.0, true, 0.0, "still") },
    HitScene { again: 12, ..scene("again", None, 20, 0.0, true, 0.0, "still") },
    scene("turn", None, 20, 0.0, false, 0.0, "turn"),
    scene("squash", None, 20, 0.0, false, 0.0, "squash"),
    scene("tumble", None, 20, 0.0, false, 0.0, "tumble"),
    scene("tumble-stand", None, 20, 0.0, true, 0.0, "tumble"),
    scene("tumble-swing", Some("swing"), 20, 0.0, true, 1.0, "tumble"),
];
const TUMBLE: f64 = 36.0;
/// 入れ物の世界の行列（tools/actor-parity.mjs の holderPose と同じ。pinball の玉の入れ物の形）。i は場面の始めからのコマ、hit はのけぞったコマ（まだなら -1）
fn holder_world(kind: &str, i: i32, hit: i32) -> Mat4 {
    if kind == "still" { return Mat4::IDENTITY; }
    let (n, j) = (i as f64, if hit < 0 { 0.0 } else { (i - hit) as f64 });
    let tk = (j / TUMBLE).min(1.0);
    let rz = -0.2 + if hit >= 0 && j < TUMBLE { std::f64::consts::PI * 2.0 * (1.0 - (1.0 - tk).powi(3)) } else { 0.0 };
    let ry = (n * DT * 0.8).sin() * 0.25;
    let q = 0.12 * (n * 0.4).sin() * (-n * 0.02).exp();
    let st = 0.08;
    let k = if hit < 0 { 0.0 } else { (1.0 - j / 42.0).max(0.0) };
    let big = 1.0 + 0.45 * k * (2.0 - k);
    let s = DVec3::new(big * (1.0 - st * 0.6 - q * 0.5), big * (1.0 + st + q), big * (1.0 - st * 0.6 + q * 0.3));
    let (ry, rz, s) = match kind { "turn" => (ry, rz, DVec3::ONE), "squash" => (0.0, 0.0, s), _ => (ry, rz, s) };
    DMat4::from_scale_rotation_translation(s, DQuat::from_euler(EulerRot::XYZ, 0.0, ry, rz), DVec3::new(0.5, 1.25, -0.75)).as_mat4()
}

/// 位置はメートル、向きは単位ベクトルの成分（0.0025 は約 0.14 度）。骨の連なりに溜まる f32 と倍精度の丸めの差より大きく、見て分かる差よりずっと小さい
/// （character-actor 0.1.11 の全員の最大。three 0.178: 骨 0.00045・モーフ 0.00067・持ち手 0.00055。three 0.186: 骨 0.00030・モーフ 0.00067・持ち手 0.00001）
const BONE_LIMIT: f32 = 2.5e-3;
const MORPH_LIMIT: f32 = 1e-3;
const GRIP_LIMIT: f32 = 5e-3;

/// 比べる three の版（character-actor を使うゲームの package.json の版。compared_three_versions_cover_the_games が確かめる）
const THREE_VERSIONS: [&str; 2] = ["0.178.0", "0.186.1"];

fn store() -> PathBuf { PathBuf::from(std::env::var("HOME").unwrap()).join(".local/share/muso-rust/actor-parity") }
fn index_url() -> String { std::env::var("CHARACTER_INDEX_URL").unwrap_or_else(|_| PRODUCTION_INDEX_URL.to_string()) }

fn kind(a: &str) -> MotionKind {
    match a { "attack" => MotionKind::Attack, "hit" => MotionKind::Hit, "dodge" => MotionKind::Dodge, "skill" => MotionKind::Skill, "banzai" => MotionKind::Banzai, "greet" => MotionKind::Greet, "jump" => MotionKind::Jump, "rest" => MotionKind::Rest, "ready" => MotionKind::Ready, _ => MotionKind::Run }
}
fn action(a: &str) -> ActionKind {
    match a { "attack" => ActionKind::Attack, "dodge" => ActionKind::Dodge, "skill" => ActionKind::Skill, "banzai" => ActionKind::Banzai, _ => ActionKind::Greet }
}
fn ev_text(f: i64, e: &ActorEvent) -> String {
    let a = |k: &ActionKind| format!("{k:?}").to_lowercase();
    match e {
        ActorEvent::Start(k) => format!("{f} start {}", a(k)),
        ActorEvent::Interrupt(k) => format!("{f} interrupt {}", a(k)),
        ActorEvent::End(k) => format!("{f} end {}", a(k)),
        ActorEvent::Cue(k, c) => format!("{f} cue {} {}", a(k), match c { Cue::Strike => "strike", Cue::Takeoff => "takeoff", Cue::Land => "land", Cue::Seated => "seated" }),
    }
}

struct Worst { bones: f32, at: String, morph: f32, morph_at: String, grip: f32, grip_at: String }
impl Worst { fn new() -> Worst { Worst { bones: 0.0, at: String::new(), morph: 0.0, morph_at: String::new(), grip: 0.0, grip_at: String::new() } } }

/// (条件, 動作, 最大の差, 骨の差が許容を超えた最初のコマ)
type Cell = (String, String, f32, Option<i32>);

/// 1 点を比べる（節点の位置と向き・モーフ・持ち手）。here は骨の差の最大
fn compare_row(a: &CharacterActor, nodes: &[(usize, String)], morphs: &[Option<(usize, usize)>], ts: &Value, row: &[f32], tag: &str, w: &mut Worst, here: &mut f32) {
    let mut n = 0;
    for (node, name) in nodes {
        if *node != usize::MAX {
            let e = a.skel.world[*node];
            let vals = [e.w_axis.x, e.w_axis.y, e.w_axis.z, e.x_axis.x, e.x_axis.y, e.x_axis.z, e.y_axis.x, e.y_axis.y, e.y_axis.z];
            for (k, v) in vals.iter().enumerate() {
                let d = (v - row[n + k]).abs();
                if d > *here { *here = d; }
                if d > w.bones { w.bones = d; w.at = format!("{tag} {name}[{k}] rust {v} ts {}", row[n + k]); }
            }
        }
        n += 9;
    }
    for (j, mo) in morphs.iter().enumerate() {
        if let Some((node, idx)) = mo {
            let d = (a.skel.morphs[*node][*idx] - row[n + j]).abs();
            if d > w.morph { w.morph = d; w.morph_at = format!("{tag} {}.{} rust {} ts {}", ts["morphs"][j][0], ts["morphs"][j][1], a.skel.morphs[*node][*idx], row[n + j]); }
        }
    }
    n += morphs.len();
    if let Some(g) = a.grip_world() {
        let c = g.to_cols_array();
        for (k, v) in c.iter().enumerate() {
            if row[n + k].is_finite() {
                let d = (v - row[n + k]).abs();
                if d > w.grip { w.grip = d; w.grip_at = format!("{tag}[{k}] rust {v} ts {}", row[n + k]); }
            }
        }
    }
}

struct Stepper { f: i64, events: Vec<String>, grounded: bool }
impl Stepper {
    fn step(&mut self, a: &mut CharacterActor, speed: f32, armed: f32, rate: f64) {
        a.move_(speed as f64, self.grounded, armed, Some(rate));
        let ev = a.update(DT, self.f as f64 * DT);
        for e in &ev { self.events.push(ev_text(self.f, e)); }
        self.f += 1;
    }
}

/// 動きの一覧（ゲームが読む長さ・体の移動・高さ）
fn check_motions(a: &CharacterActor, id: &str, ts: &Value, bad: &mut Vec<String>) {
    for (name, m) in ts["motions"].as_object().unwrap() {
        let q = a.query(kind(name));
        let sec_ok = match (q.seconds, m["seconds"].as_f64()) { (None, None) => true, (Some(x), Some(y)) => (x as f64 - y).abs() < 1e-3, _ => false };
        let src = q.source.map(|s| format!("{s:?}").to_lowercase());
        let travel_ok = match (q.travel, m["travel"].as_array()) { (None, None) => true, (Some(t), Some(v)) => (0..3).all(|i| (t[i] as f64 - v[i].as_f64().unwrap()).abs() < 2e-3), _ => false };
        let height_ok = match (q.height, m["height"].as_f64()) { (None, None) => true, (Some(x), Some(y)) => (x as f64 - y).abs() < 2e-3, _ => false };
        if q.available != m["available"].as_bool().unwrap() || src.as_deref() != m["source"].as_str() || !sec_ok || !travel_ok || !height_ok {
            bad.push(format!("{id} {} の {name} の一覧が違う rust {q:?} ts {m}", ts["quality"]));
        }
    }
}

/// 1 人を比べる。戻すのは問題の一覧（空なら一致）と、(条件, 動作) ごとの最大の差
fn compare(cat: &Catalog, id: &str, ts: &Value, bin: &[f32]) -> (Vec<String>, Vec<Cell>, Worst, usize) {
    let mut bad = Vec::new();
    let mut a = load_character_actor(&cat.client, LoadOptions { id: id.to_string(), revision: None, quality: Quality::Normal, gait: Gait { walk: 0.15, run: 2.5 } }).unwrap();
    if ts["revision"].as_str() != Some(a.revision.as_str()) { bad.push(format!("{id} 版が違う ts {} rust {}", ts["revision"], a.revision)); return (bad, Vec::new(), Worst::new(), 0); }
    check_motions(&a, id, ts, &mut bad);
    a.hold_weapon(Some(0.9));
    let nodes: Vec<(usize, String)> = ts["nodes"].as_array().unwrap().iter().filter_map(|n| {
        let n = n.as_str().unwrap();
        match a.skel.find(n) { Some(i) => Some((i, n.to_string())), None => { bad.push(format!("{id} 節点が無い {n}")); Some((usize::MAX, n.to_string())) } }
    }).collect();
    let morphs: Vec<Option<(usize, usize)>> = ts["morphs"].as_array().unwrap().iter().map(|m| {
        let (mesh, key) = (m[0].as_str().unwrap(), m[1].as_str().unwrap());
        let p = a.prims.iter().find(|p| p.name == mesh)?;
        let names = &a.glb.meshes[a.glb.nodes[p.node].mesh.unwrap()].target_names;
        Some((p.node, names.iter().position(|n| n == key)?))
    }).collect();
    let stride = ts["stride"].as_u64().unwrap() as usize;
    let meta = ts["meta"].as_array().unwrap();
    let mut s = Stepper { f: 0, events: Vec::new(), grounded: true };
    let mut w = Worst::new();
    let mut table = Vec::new();
    let mut mi = 0usize;
    let mut compared = 0usize;
    for cond in CONDITIONS {
        for act in ACTIONS {
            // 長さは TS の秒数で数える（f32 に丸めると切り上げがずれる。秒数そのものが同じかは上の一覧で見る）
            let q = &ts["motions"][if act == "swing" { "attack" } else { act }];
            if !q["available"].as_bool().unwrap() { continue; }
            let len = (if act == "swing" { SWING_SECONDS } else { q["seconds"].as_f64().unwrap_or(0.6) } / DT).ceil() as i32;
            for _ in 0..30 { s.step(&mut a, 0.0, 0.0, 1.0); }
            for i in 0..LEAD {
                if cond == "idle" { s.step(&mut a, 0.0, 0.0, 1.0); } else { s.step(&mut a, if i < 10 { 1.0 } else { RUN }, 0.0, if i < 10 { 1.0 } else { RATE }); }
            }
            match act {
                "hit" => a.hit(None, None),
                "swing" => { a.start(ActionKind::Attack, StartOptions { seconds: Some(SWING_SECONDS), ..Default::default() }); }
                _ => { a.start(action(act), StartOptions::default()); }
            }
            let (mut here, mut first) = (0f32, None);
            for i in 0..len + TAIL {
                match cond { "run" => s.step(&mut a, RUN, 0.0, RATE), "stop" => s.step(&mut a, 0.0, 1.0, 1.0), _ => s.step(&mut a, 0.0, 0.0, 1.0) }
                if i % EVERY != 0 { continue; }
                if let Ok(watch) = std::env::var("ACTOR_PARITY_WATCH") && mi < meta.len() {
                    // 調べるとき: ACTOR_PARITY_WATCH=条件,動作,節点 で 1 本の値を並べて出す
                    let w: Vec<&str> = watch.split(',').collect();
                    if w[0] == cond && w[1] == act && let Some(k) = nodes.iter().position(|(_, n)| n == w[2]) {
                        let (row, e) = (&bin[mi * stride + k * 9..mi * stride + k * 9 + 3], a.skel.world[nodes[k].0].w_axis);
                        println!("  f{i} rust [{:.3} {:.3} {:.3}] ts [{:.3} {:.3} {:.3}] current {:?}", e.x, e.y, e.z, row[0], row[1], row[2], a.current());
                    }
                }
                let m = &meta[mi];
                assert!(m[0] == cond && m[1] == act && m[2] == i, "{id} 場面の順がずれた {m} {cond} {act} {i}");
                let row = &bin[mi * stride..(mi + 1) * stride];
                mi += 1;
                compared += 1;
                compare_row(&a, &nodes, &morphs, ts, row, &format!("{cond} {act} f{i}"), &mut w, &mut here);
                if first.is_none() && here > BONE_LIMIT { first = Some(i); }
            }
            table.push((cond.to_string(), act.to_string(), here, first));
        }
    }
    // のけぞりの入り方。どのコマも比べる
    if ts["motions"]["hit"]["available"].as_bool().unwrap() {
        let len = (ts["motions"]["hit"]["seconds"].as_f64().unwrap() / DT).ceil() as i32;
        for sc in &HIT_SCENES {
            if let Some(b) = sc.before && !ts["motions"][if b == "swing" { "attack" } else { b }]["available"].as_bool().unwrap() { continue; }
            let rate = if sc.speed > 0.0 { RATE } else { 1.0 };
            a.set_parent_world(Mat4::IDENTITY);
            s.grounded = true;
            for _ in 0..30 { s.step(&mut a, 0.0, 0.0, 1.0); }
            s.grounded = sc.grounded;
            match sc.before {
                Some("swing") => { a.start(ActionKind::Attack, StartOptions { seconds: Some(SWING_SECONDS), ..Default::default() }); }
                Some("jump") => { a.start(ActionKind::Jump, StartOptions::default()); }
                Some("rest") => { a.start(ActionKind::Rest, StartOptions::default()); }
                Some(b) => { a.start(action(b), StartOptions::default()); }
                None => {}
            }
            for i in 0..sc.lead { a.set_parent_world(holder_world(sc.holder, i, -1)); s.step(&mut a, sc.speed, sc.armed, rate); }
            let hit = |a: &mut CharacterActor| a.hit(if sc.seconds > 0.0 { Some(sc.seconds) } else { None }, if sc.since > 0.0 { Some(sc.since) } else { None });
            hit(&mut a);
            let (mut here, mut first) = (0f32, None);
            let tag = format!("hit:{}", sc.name);
            for i in 0..len + TAIL {
                if sc.again > 0 && i == sc.again { hit(&mut a); }
                if !sc.grounded && i == 30 { s.grounded = true; }
                a.set_parent_world(holder_world(sc.holder, sc.lead + i, sc.lead));
                s.step(&mut a, sc.speed, sc.armed, rate);
                let m = &meta[mi];
                assert!(m[0] == tag.as_str() && m[1] == "hit" && m[2] == i, "{id} 場面の順がずれた {m} {tag} {i}");
                let row = &bin[mi * stride..(mi + 1) * stride];
                mi += 1;
                compared += 1;
                compare_row(&a, &nodes, &morphs, ts, row, &format!("{tag} f{i}"), &mut w, &mut here);
                if first.is_none() && here > BONE_LIMIT { first = Some(i); }
            }
            if sc.before == Some("rest") { a.stop(ActionKind::Rest); }
            table.push((tag, "hit".to_string(), here, first));
        }
    }
    let tsev: Vec<String> = ts["events"].as_array().unwrap().iter().map(|e| {
        let v = e.as_array().unwrap();
        if v[1] == "cue" { format!("{} cue {} {}", v[0], v[2].as_str().unwrap(), v[3].as_str().unwrap()) } else { format!("{} {} {}", v[0], v[1].as_str().unwrap(), v[2].as_str().unwrap()) }
    }).collect();
    if s.events != tsev { bad.push(format!("{id} 合図が違う\n  rust {:?}\n  ts   {:?}", s.events, tsev)); }
    if mi != meta.len() { bad.push(format!("{id} 比べた点の数が違う {mi} {}", meta.len())); }
    if w.bones > BONE_LIMIT { bad.push(format!("{id} 骨の差 {:.5}（{}）", w.bones, w.at)); }
    if w.morph > MORPH_LIMIT { bad.push(format!("{id} モーフの差 {:.5}（{}）", w.morph, w.morph_at)); }
    if w.grip > GRIP_LIMIT { bad.push(format!("{id} 持ち手の差 {:.5}（{}）", w.grip, w.grip_at)); }
    (bad, table, w, compared)
}

// ---- ゲームの入り方の場面（tools/game-scenes.json を actor-parity.mjs と両方が読む）。入れ物の式は pinball の cast.ts と actor-parity.mjs の ballFrame・friendFrame と同じ ----

const GAME_SCENES: &str = include_str!("../tools/game-scenes.json");
const UNEVEN: [f64; 10] = [1.0 / 60.0, 1.0 / 60.0, 0.2, 0.001, 0.0, 1.0 / 30.0, 1.0 / 144.0, 1.0 / 24.0, 0.05, 0.051];
const SCALE: f64 = 1.8;
const IN_BALL: f64 = 0.8;
const TUMBLE_S: f64 = 0.6;
const WIND: f64 = 0.3;

const fn pin_face(emotion: &'static str, emotion_weight: f32, mouth: &'static str, mouth_weight: f32, look: &'static str, look_weight: f32) -> FaceControl {
    FaceControl { emotion, emotion_weight, mouth, mouth_weight, look, look_weight }
}
const FACE_UP: FaceControl = pin_face("neutral", 0.0, "neutral", 0.0, "lookUp", 0.8);
const FACE_SURPRISED: FaceControl = pin_face("surprised", 0.9, "oh", 0.9 * 0.8, "neutral", 0.0);
const FACE_CHEER: FaceControl = pin_face("happy", 0.85, "ih", 0.85 * 0.8, "neutral", 0.0);
const FACE_SAD: FaceControl = pin_face("sad", 0.8, "oh", 0.8 * 0.8, "neutral", 0.0);
fn face_named(n: &str) -> FaceControl { match n { "surprised" => FACE_SURPRISED, "cheer" => FACE_CHEER, "sad" => FACE_SAD, _ => FACE_UP } }

fn trs(p: [f64; 3], ry: f64, rz: f64, s: [f64; 3]) -> DMat4 { DMat4::from_scale_rotation_translation(DVec3::from(s), DQuat::from_euler(EulerRot::XYZ, 0.0, ry, rz), DVec3::from(p)) }

/// pinball の玉（holder > inner > charBox > 主役）
struct Ball { x: f64, y: f64, tilt: f64, squash: f64, squash_v: f64, tumble_t: f64, tumble_dir: f64, slap_k: f64, standing: bool, face_t: f64, face_now: FaceControl }
impl Ball {
    fn new(standing: bool) -> Ball { Ball { x: 0.0, y: 1.0, tilt: 0.0, squash: 0.0, squash_v: 0.0, tumble_t: 0.0, tumble_dir: 1.0, slap_k: 0.0, standing, face_t: 0.0, face_now: FACE_UP } }
    /// 入れ物の世界の行列と、部品へ渡す宙か地面か・表情
    fn frame(&mut self, dt: f64, t: f64, height: f64) -> (Mat4, bool, Option<FaceControl>) {
        let (vx, vy) = (6.0 * (t * 2.0).sin(), 8.0 * (t * 1.5).cos());
        self.x += vx * dt;
        self.y += vy * dt;
        self.tilt += ((-vx * 0.025 - WIND * 0.25).clamp(-0.6, 0.6) - self.tilt) * (dt * 8.0).min(1.0);
        self.tumble_t = (self.tumble_t - dt).max(0.0);
        let tk = 1.0 - self.tumble_t / TUMBLE_S;
        let rz = self.tilt + if self.tumble_t > 0.0 { self.tumble_dir * std::f64::consts::PI * 2.0 * (1.0 - (1.0 - tk).powi(3)) } else { 0.0 };
        self.squash_v += (-self.squash * 260.0 - self.squash_v * 16.0) * dt;
        self.squash += self.squash_v * dt;
        let (stretch, wob) = ((vx.hypot(vy) * 0.004).min(0.14), (t * 19.0).sin() * 0.07 * WIND);
        self.slap_k = (self.slap_k - dt / 0.7).max(0.0);
        let big = 1.0 + 0.45 * self.slap_k * (2.0 - self.slap_k);
        let inner = [big * (1.0 - stretch * 0.6 - self.squash * 0.5 + wob), big * (1.0 + stretch + self.squash - wob), big * (1.0 - stretch * 0.6 + self.squash * 0.3)];
        let k = SCALE * IN_BALL;
        let m = trs([self.x, self.y, 0.0], 0.0, 0.0, [1.0; 3]) * trs([0.0; 3], (t * 0.8).sin() * 0.25, rz, inner) * trs([0.0, -height * k * 0.5, 0.0], 0.0, 0.0, [k; 3]);
        self.face_t -= dt;
        (m.as_mat4(), self.standing, Some(if self.face_t > 0.0 { self.face_now } else { FACE_UP }))
    }
}

/// pinball の仲間（holder > lean > 主役）
struct Friend { pos: [f64; 3], from: [f64; 3], target: [f64; 3], jumping: f64, dur: f64, hop: f64, hop_v: f64, sq: f64, sq_v: f64, lean_y: f64, lean_z: f64, swing_t: f64, face_t: f64, face_now: Option<FaceControl> }
impl Friend {
    fn new() -> Friend { Friend { pos: [-3.2, 0.5, 0.0], from: [0.0; 3], target: [0.0; 3], jumping: 0.0, dur: 1.0, hop: 0.0, hop_v: 0.0, sq: 0.0, sq_v: 0.0, lean_y: 0.0, lean_z: 0.0, swing_t: 9.0, face_t: 0.0, face_now: None } }
    fn frame(&mut self, dt: f64, t: f64) -> (Mat4, bool, Option<FaceControl>) {
        self.swing_t += dt;
        let mut grounded = true;
        if self.jumping > 0.0 {
            self.jumping = (self.jumping - dt).max(0.0);
            let k = 1.0 - self.jumping / self.dur;
            let e = k * k * (3.0 - 2.0 * k);
            for i in 0..3 { self.pos[i] = self.from[i] + (self.target[i] - self.from[i]) * e; }
            self.pos[1] += (k * std::f64::consts::PI).sin() * 3.5;
            grounded = false;
        }
        self.hop_v -= 30.0 * dt;
        self.hop += self.hop_v * dt;
        if self.hop < 0.0 { self.hop = 0.0; self.hop_v = 0.0; }
        self.sq_v += (-self.sq * 240.0 - self.sq_v * 14.0) * dt;
        self.sq += self.sq_v * dt;
        self.lean_z *= (1.0 - dt * 6.0).max(0.0);
        self.lean_y += (((2.5 * t.sin() - self.pos[0]) * 0.12).clamp(-0.6, 0.6) - self.lean_y) * (dt * 4.0).min(1.0);
        let m = trs(self.pos, 0.4, 0.0, [SCALE * 1.3; 3]) * trs([0.0, self.hop / SCALE, 0.0], self.lean_y, self.lean_z, [1.0 - self.sq * 0.3, 1.0 + self.sq * 0.5, 1.0 - self.sq * 0.3]);
        self.face_t -= dt;
        (m.as_mat4(), grounded, if self.face_t > 0.0 { self.face_now } else { None })
    }
}

fn kind_of(n: &str) -> ActionKind {
    match n { "attack" => ActionKind::Attack, "skill" => ActionKind::Skill, "dodge" => ActionKind::Dodge, "jump" => ActionKind::Jump, "rest" => ActionKind::Rest, "banzai" => ActionKind::Banzai, "greet" => ActionKind::Greet, _ => panic!("動作の名前 {n}") }
}
fn start_opts(o: Option<&Value>) -> StartOptions {
    let n = |k: &str| o.and_then(|v| v.get(k)).and_then(Value::as_f64);
    StartOptions { seconds: n("seconds"), air_seconds: n("airSeconds"), from: n("from"), to: n("to"), ease_swing: o.and_then(|v| v.get("ease")).is_some(), cycle: n("cycle"), since: n("since") }
}

struct GameDrive { f: i64, events: Vec<String> }
impl GameDrive {
    fn update(&mut self, a: &mut CharacterActor, dt: f64, now: f64, speed: f64, grounded: bool, armed: f32, rate: f64) {
        a.move_(speed, grounded, armed, Some(rate));
        for e in &a.update(dt, now) { self.events.push(ev_text(self.f, e)); }
        self.f += 1;
    }
}

/// ゲームの入り方の場面を全部（lite）。戻すのは compare と同じ形
fn compare_game(cat: &Catalog, id: &str, ts: &Value, bin: &[f32]) -> (Vec<String>, Vec<Cell>, Worst, usize) {
    let mut bad = Vec::new();
    let mut a = load_character_actor(&cat.client, LoadOptions { id: id.to_string(), revision: None, quality: Quality::Lite, gait: Gait { walk: 0.15, run: 2.5 } }).unwrap();
    if ts["revision"].as_str() != Some(a.revision.as_str()) { bad.push(format!("{id} lite の版が違う ts {} rust {}", ts["revision"], a.revision)); return (bad, Vec::new(), Worst::new(), 0); }
    check_motions(&a, id, ts, &mut bad);
    a.hold_weapon(Some(0.9));
    let nodes: Vec<(usize, String)> = ts["nodes"].as_array().unwrap().iter().map(|n| { let n = n.as_str().unwrap(); (a.skel.find(n).unwrap_or_else(|| { bad.push(format!("{id} lite に節点が無い {n}")); usize::MAX }), n.to_string()) }).collect();
    let morphs: Vec<Option<(usize, usize)>> = ts["morphs"].as_array().unwrap().iter().map(|m| {
        let (mesh, key) = (m[0].as_str().unwrap(), m[1].as_str().unwrap());
        let p = a.prims.iter().find(|p| p.name == mesh)?;
        let names = &a.glb.meshes[a.glb.nodes[p.node].mesh.unwrap()].target_names;
        Some((p.node, names.iter().position(|n| n == key)?))
    }).collect();
    let stride = ts["stride"].as_u64().unwrap() as usize;
    let meta = ts["meta"].as_array().unwrap();
    let scenes: Value = serde_json::from_str(GAME_SCENES).unwrap();
    let height = a.height_meters as f64;
    let (mut d, mut w, mut table, mut mi, mut compared) = (GameDrive { f: 0, events: Vec::new() }, Worst::new(), Vec::new(), 0usize, 0usize);
    let once = [serde_json::json!(60)];
    for sc in scenes.as_array().unwrap() {
        let name = sc["name"].as_str().unwrap();
        for dt_kind in sc["dts"].as_array().map(Vec::as_slice).unwrap_or(&once) {
            a.set_parent_world(Mat4::IDENTITY);
            a.express(None);
            if let Some(c) = a.current() { a.stop(c); }
            let (mut i, mut n) = (0, 0);
            while (i < 30 || a.current().is_some()) && n < 900 {
                let now = d.f as f64 * DT;
                d.update(&mut a, DT, now, 0.0, true, 0.0, 1.0);
                if a.current().is_none() { i += 1; }
                n += 1;
            }
            let kind_text = dt_kind.as_str().map(String::from).unwrap_or_else(|| dt_kind.to_string());
            let (pattern, base_every): (&[f64], i64) = match kind_text.as_str() { "60" => (&[DT], 1), "120" => (&[1.0 / 120.0], 2), "144" => (&[1.0 / 144.0], 3), "uneven" => (&UNEVEN, 1), k => panic!("刻みの名前 {k}") };
            let every = base_every * sc["every"].as_i64().unwrap_or(1);
            let tag = format!("game:{name}@{kind_text}");
            let num = |k: &str, or: f64| sc[k].as_f64().unwrap_or(or);
            let (mut speed, mut rate, mut armed, mut grounded) = (num("speed", 0.0), num("rate", 1.0), num("armed", 0.0) as f32, sc["grounded"].as_bool().unwrap_or(true));
            let mut ball = (sc["holder"] == "ball").then(|| Ball::new(grounded));
            let mut friend = (sc["holder"] == "friend").then(Friend::new);
            let list = sc["do"].as_array().unwrap();
            let mut done = vec![false; list.len()];
            let (mut t, mut k, mut here, mut first) = (0f64, 0i64, 0f32, None);
            while t < num("seconds", 0.0) - 1e-9 {
                let dt = pattern[k as usize % pattern.len()];
                for (ci, c) in list.iter().enumerate() {
                    if done[ci] || c.get("stopAfter").is_some() || t + 1e-9 < c["at"].as_f64().unwrap() { continue; }
                    done[ci] = true;
                    if c.get("stopCurrent").is_some() && let Some(cur) = a.current() { a.stop(cur); }
                    if let Some(s) = c["stop"].as_str() { a.stop(kind_of(s)); }
                    if let Some(s) = c["start"].as_str() { a.start(kind_of(s), start_opts(c.get("opts"))); }
                    if let Some(h) = c.get("hit") { a.hit(h["seconds"].as_f64(), h["since"].as_f64()); }
                    if let Some(v) = c["speed"].as_f64() { speed = v; }
                    if let Some(v) = c["rate"].as_f64() { rate = v; }
                    if let Some(v) = c["armed"].as_f64() { armed = v as f32; }
                    if let Some(v) = c["grounded"].as_bool() { grounded = v; }
                    if let Some(b) = &mut ball {
                        if let Some(dir) = c["slap"].as_f64() { a.hit(None, None); b.tumble_t = TUMBLE_S; b.tumble_dir = dir; b.slap_k = 1.0; }
                        if c.get("ouch").is_some() { a.hit(None, None); }
                        if let Some(v) = c["kick"].as_f64() { b.squash_v += v; }
                        if let Some(n) = c["face"].as_str() { b.face_now = face_named(n); b.face_t = c["faceSeconds"].as_f64().unwrap(); }
                    }
                    if let Some(fr) = &mut friend {
                        if let Some(side) = c["swing"].as_f64() {
                            if fr.swing_t > 0.12 { a.start(ActionKind::Attack, StartOptions { seconds: Some(0.34), ..Default::default() }); fr.swing_t = 0.0; }
                            fr.hop_v += 3.2;
                            fr.sq_v -= 6.0;
                            fr.lean_z = -side * 0.18;
                        }
                        if c.get("catch").is_some() { fr.sq_v += 8.0; fr.face_now = Some(FACE_SURPRISED); fr.face_t = 0.3; }
                        if let Some(v) = c["cheer"].as_f64() { fr.face_now = Some(FACE_CHEER); fr.face_t = v; fr.hop_v += 4.5; }
                        if c.get("nod").is_some() { fr.sq_v += 3.0; }
                        if let Some(v) = c["worry"].as_f64() { fr.face_now = Some(FACE_SAD); fr.face_t = v; }
                        if let Some(j) = c["jumpTo"].as_array() {
                            let v: Vec<f64> = j.iter().map(|x| x.as_f64().unwrap()).collect();
                            fr.from = fr.pos;
                            fr.target = [v[0], v[1], v[2]];
                            fr.jumping = v[3];
                            fr.dur = v[3];
                        }
                    }
                }
                let look = match (&mut ball, &mut friend) { (Some(b), _) => Some(b.frame(dt, t, height)), (_, Some(fr)) => Some(fr.frame(dt, t)), _ => None };
                if let Some((m, g, face)) = look {
                    a.set_parent_world(m);
                    a.express(face);
                    grounded = g;
                }
                d.update(&mut a, dt, t, speed, grounded, armed, rate);
                for (ci, c) in list.iter().enumerate() {
                    if done[ci] || c.get("stopAfter").is_none() || t + 1e-9 < c["at"].as_f64().unwrap() { continue; }
                    done[ci] = true;
                    if let Some(cur) = a.current() { a.stop(cur); }
                }
                t += dt;
                if k % every == 0 {
                    let m = &meta[mi];
                    assert!(m[0] == tag.as_str() && m[2] == k, "{id} 場面の順がずれた {m} {tag} {k}");
                    let row = &bin[mi * stride..(mi + 1) * stride];
                    mi += 1;
                    compared += 1;
                    let mut this = 0f32;
                    compare_row(&a, &nodes, &morphs, ts, row, &format!("{tag} f{k}"), &mut w, &mut this);
                    here = here.max(this);
                    if first.is_none() && here > BONE_LIMIT { first = Some(k as i32); }
                    // 調べるとき: ACTOR_PARITY_TRACE=場面の名前 でコマごとの差を出す
                    if std::env::var("ACTOR_PARITY_TRACE").is_ok_and(|n| tag.contains(&n)) { println!("  {tag} f{k} t {t:.4} 骨の差 {this:.5} current {:?}", a.current()); }
                }
                k += 1;
            }
            table.push((tag, "game".to_string(), here, first));
        }
    }
    let tsev: Vec<String> = ts["events"].as_array().unwrap().iter().map(|e| {
        let v = e.as_array().unwrap();
        if v[1] == "cue" { format!("{} cue {} {}", v[0], v[2].as_str().unwrap(), v[3].as_str().unwrap()) } else { format!("{} {} {}", v[0], v[1].as_str().unwrap(), v[2].as_str().unwrap()) }
    }).collect();
    if d.events != tsev {
        let at = d.events.iter().zip(&tsev).position(|(x, y)| x != y).unwrap_or(d.events.len().min(tsev.len()));
        bad.push(format!("{id} lite の合図が違う（{at} 番目）\n  rust {:?}\n  ts   {:?}", &d.events[at..(at + 6).min(d.events.len())], &tsev[at..(at + 6).min(tsev.len())]));
    }
    if mi != meta.len() { bad.push(format!("{id} lite の比べた点の数が違う {mi} {}", meta.len())); }
    if w.bones > BONE_LIMIT { bad.push(format!("{id} lite の骨の差 {:.5}（{}）", w.bones, w.at)); }
    if w.morph > MORPH_LIMIT { bad.push(format!("{id} lite のモーフの差 {:.5}（{}）", w.morph, w.morph_at)); }
    if w.grip > GRIP_LIMIT { bad.push(format!("{id} lite の持ち手の差 {:.5}（{}）", w.grip, w.grip_at)); }
    (bad, table, w, compared)
}

/// three の版のフォルダ（このリポジトリか、隣のゲームの node_modules）
fn three_folder(version: &str) -> Option<PathBuf> {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lab = repo.join("..");
    let mut roots = vec![repo];
    if let Ok(list) = std::fs::read_dir(&lab) { roots.extend(list.flatten().map(|d| d.path())); }
    roots.into_iter().map(|r| r.join(format!("node_modules/.pnpm/three@{version}/node_modules/three"))).find(|p| p.join("package.json").exists())
}

/// 防ぐ故障: ゲームが three の版を変えたのに、その版で比べないまま「同じ」と言い続ける
#[test]
fn compared_three_versions_cover_the_games() {
    let lab = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let Ok(list) = std::fs::read_dir(&lab) else { return };
    let mut seen = Vec::new();
    for d in list.flatten() {
        let Ok(b) = std::fs::read(d.path().join("package.json")) else { continue };
        let Ok(p) = serde_json::from_slice::<Value>(&b) else { continue };
        let of = |name: &str| ["dependencies", "devDependencies", "peerDependencies"].iter().find_map(|k| p[*k][name].as_str().map(String::from));
        if of("@aiarranger/character-actor").is_none() { continue; }
        let Some(v) = of("three") else { continue };
        let v = v.trim_start_matches(['^', '~']).to_string();
        assert!(THREE_VERSIONS.contains(&v.as_str()), "{} の three は {v}。THREE_VERSIONS に足して比べる", d.path().display());
        seen.push(v);
    }
    println!("character-actor を使うゲーム {} 本の three: {:?}", seen.len(), { seen.sort(); seen.dedup(); seen });
}

#[test]
fn all_heroes_match_ts() {
    let root = store();
    let cat = Catalog::open(&index_url(), Some(root.join("delivery")), Some(root.join("files"))).unwrap();
    let index = cat.client.refresh().unwrap();
    let only: Option<Vec<String>> = std::env::var("ACTOR_PARITY_IDS").ok().map(|s| s.split(',').map(String::from).collect());
    let ids: Vec<String> = index["characters"].as_object().unwrap().keys().filter(|k| only.as_ref().is_none_or(|o| o.contains(k))).cloned().collect();
    assert!(!ids.is_empty());
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let released = std::process::Command::new("node").arg(manifest.join("tools/actor-parity.mjs")).arg("--release-info").output().expect("配布 SHA を読めません");
    assert!(released.status.success(), "配布 SHA の照合に失敗: {}", String::from_utf8_lossy(&released.stderr));
    let release: Value = serde_json::from_slice(&released.stdout).expect("配布 SHA の情報が JSON ではありません");
    let release_key = release["key"].as_str().expect("配布 SHA のキーがありません");
    let changed = |p: PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let newest_source = [manifest.join("tools/actor-parity.mjs"), manifest.join("tools/game-scenes.json"), manifest.join("../../packages/character-actor/package.json"), manifest.join("../../packages/character-runtime/package.json"), manifest.join("../../packages/character-motion/package.json")].into_iter().filter_map(changed).max();
    let (mut bad, mut total, mut cells, mut worst) = (Vec::new(), 0usize, 0usize, 0f32);
    let read = |path: PathBuf| -> Vec<f32> { std::fs::read(path).unwrap().chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect() };
    for version in THREE_VERSIONS {
        let dir = root.join(format!("ts-three-{version}-{release_key}"));
        // TS の値が無い・配布の版が変わった・TS の部品の版や書き出す道具が変わったら作り直す
        let stale = ids.iter().any(|id| {
            let Ok(b) = std::fs::read(dir.join(format!("{id}.json"))) else { return true };
            if newest_source > changed(dir.join(format!("{id}.json"))) { return true; }
            let v: Value = serde_json::from_slice(&b).unwrap();
            if v["release"] != release { return true; }
            let Ok(lite) = std::fs::read(dir.join(format!("{id}.lite.json"))) else { return true };
            let lite: Value = serde_json::from_slice(&lite).unwrap();
            if lite["release"] != release { return true; }
            let rev = index["characters"][id]["revision"].as_str().or_else(|| index["characters"][id]["current"].as_str());
            rev.is_some_and(|r| v["revision"].as_str() != Some(r)) || !dir.join(format!("{id}.bin")).exists() || !dir.join(format!("{id}.lite.bin")).exists()
        });
        if stale {
            let three = three_folder(version).unwrap_or_else(|| panic!("three {version} が見つかりません（このリポジトリか隣のゲームで pnpm install する）"));
            let mut c = std::process::Command::new("node");
            c.arg(manifest.join("tools/actor-parity.mjs")).arg(&dir).env("ACTOR_PARITY_THREE", three);
            if only.is_some() { c.args(&ids); }
            let st = c.status().expect("node を動かせません");
            assert!(st.success(), "TS の値を作れませんでした（three {version}）");
        }
        let revision = version.split('.').nth(1).unwrap();
        let (mut version_worst, mut version_morph, mut version_grip, mut version_points) = (0f32, 0f32, 0f32, 0usize);
        for id in &ids {
            let ts: Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{id}.json"))).unwrap()).unwrap();
            assert_eq!(ts["release"], release, "{id} の比較相手の配布 SHA が違います");
            assert_eq!(ts["three"].as_str(), Some(revision), "{id} の値は three {} で書いた物（{version} ではない）", ts["three"]);
            let (b, table, w, n) = compare(&cat, id, &ts, &read(dir.join(format!("{id}.bin"))));
            let lite: Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{id}.lite.json"))).unwrap()).unwrap();
            assert_eq!(lite["release"], release, "{id} lite の比較相手の配布 SHA が違います");
            assert_eq!(lite["three"].as_str(), Some(revision), "{id} の lite の値は three {} で書いた物（{version} ではない）", lite["three"]);
            let (b2, table2, w2, n2) = compare_game(&cat, id, &lite, &read(dir.join(format!("{id}.lite.bin"))));
            total += n + n2;
            cells += table.len() + table2.len();
            version_points += n + n2;
            version_worst = version_worst.max(w.bones).max(w2.bones);
            version_morph = version_morph.max(w.morph).max(w2.morph);
            version_grip = version_grip.max(w.grip).max(w2.grip);
            worst = worst.max(version_worst);
            println!("three {version} {id}: {} 通り {n} 点 | 骨の最大の差 {:.5}（{}）| モーフ {:.5} | 持ち手 {:.5}", table.len(), w.bones, w.at, w.morph, w.grip);
            println!("three {version} {id} lite: {} 通り {n2} 点 | 骨の最大の差 {:.5}（{}）| モーフ {:.5}（{}）| 持ち手 {:.5}（{}）", table2.len(), w2.bones, w2.at, w2.morph, w2.morph_at, w2.grip, w2.grip_at);
            if !b.is_empty() { for (c, act, d, first) in &table { if *d > BONE_LIMIT { println!("   {c} {act}: {d:.4}（最初 f{}）", first.unwrap_or(-1)); } } }
            if !b2.is_empty() { for (c, _, d, first) in &table2 { if *d > BONE_LIMIT { println!("   {c}: {d:.4}（最初 f{}）", first.unwrap_or(-1)); } } }
            bad.extend(b.into_iter().chain(b2).map(|m| format!("three {version}: {m}")));
        }
        println!("three {version}: 全員 {} 人・{version_points} 点・骨の最大の差 {version_worst:.5}・モーフ {version_morph:.5}・持ち手 {version_grip:.5}", ids.len());
    }
    println!("three {} 版・全員 {} 人・{cells} 通り・{total} 点・骨の最大の差 {worst:.5}", THREE_VERSIONS.len(), ids.len());
    assert!(bad.is_empty(), "TS の部品と違う:\n{}", bad.join("\n"));
}
