// 揺れ物（髪・服のすそなど）を TS の部品（three + @aiarranger/character-actor + @pixiv/three-vrm）と骨の値で比べる。防ぐ故障: 揺れ物の入った主役で、髪やすその揺れ方が TS と変わる。
// 配布中の主役の全員の full × 走る・止まる・攻撃（走りながら・止まって）・のけぞり（同じ）・30/144 コマ毎秒・コマ落ち・伸び縮みする入れ物・遠い座標。入れ物も動かす（揺れ物は入れ物の動きに反応する）。
// lite・normal はモデルの中の揺れる骨の数が TS と同じかを見る。TS の値は tools/spring-parity.mjs が書き出す（無い・版が変わったときはこの試験が動かす。node と通信が要る）。
use character_actor::actor::{ActionKind, CharacterActor, Gait, LoadOptions, MotionKind, Quality, StartOptions, load_character_actor};
use character_actor::delivery::{Catalog, PRODUCTION_INDEX_URL};
use glam::{DMat4, DQuat, DVec3, EulerRot, Mat4, Quat, Vec3};
use serde_json::Value;
use std::path::PathBuf;

const DT: f64 = 1.0 / 60.0;
const RUN: f32 = 4.0;
const RATE: f64 = 1.3;
const EVERY: usize = 3;
/// 位置はメートル、向きは単位ベクトルの成分（ts_parity の BONE_LIMIT と同じ。0.0025 は約 0.14 度）
const BONE_LIMIT: f32 = 2.5e-3;

fn store() -> PathBuf { PathBuf::from(std::env::var("HOME").unwrap()).join(".local/share/muso-rust/actor-parity") }
fn index_url() -> String { std::env::var("CHARACTER_INDEX_URL").unwrap_or_else(|_| PRODUCTION_INDEX_URL.to_string()) }

/// 入れ物の形（tools/spring-parity.mjs の setPose と同じ）。holder の下の stretch × twist。i は形を替えてからのコマ。
/// squash は伸び縮みするだけ（keiba の lean の形）、ball は回って伸び縮みする（pinball の玉の形）、shear は回った子を伸び縮みする親の下に置く（行列が回転と大きさに分けられなくなる）
fn pose_matrix(kind: &str, i: f64) -> DMat4 {
    let trs = |p: [f64; 3], ry: f64, rz: f64, s: [f64; 3]| DMat4::from_scale_rotation_translation(DVec3::from(s), DQuat::from_euler(EulerRot::XYZ, 0.0, ry, rz), DVec3::from(p));
    if kind == "ball" {
        let (tk, k) = ((i / 36.0).min(1.0), (1.0 - i / 42.0).max(0.0));
        let (big, q, st) = (1.0 + 0.45 * k * (2.0 - k), 0.12 * (i * 0.4).sin() * (-i * 0.02).exp(), 0.08);
        let rz = -0.2 + if i < 36.0 { std::f64::consts::PI * 2.0 * (1.0 - (1.0 - tk).powi(3)) } else { 0.0 };
        return trs([0.0; 3], (i * DT * 0.8).sin() * 0.25, rz, [big * (1.0 - st * 0.6 - q * 0.5), big * (1.0 + st + q), big * (1.0 - st * 0.6 + q * 0.3)]) * trs([0.0, -0.7, 0.0], 0.0, 0.0, [1.44; 3]);
    }
    let q = 0.2 * (i * 0.25).sin();
    let stretch = trs([0.0; 3], 0.0, 0.0, [1.0 - q * 0.5, 1.0 + q, 1.0 - q * 0.5]);
    if kind == "shear" {
        stretch * DMat4::from_quat(DQuat::from_euler(EulerRot::XYZ, 0.0, i * 0.1, 0.3 * (i * 0.1).sin()))
    } else { stretch }
}

/// 1 人ぶんの比べる先
struct Drive<'a> {
    a: &'a mut CharacterActor,
    nodes: &'a [(usize, String)],
    bin: &'a [f32],
    stride: usize,
    meta: &'a [Value],
    spring: Vec<(usize, Quat)>,
    is_spring: Vec<bool>,
    x: f64, z: f64, heading: f64, clock: f64, f: usize, mi: usize,
    pose: Option<(&'static str, usize)>,
    /// (場面, 全部の節点の最大の差, 揺れる骨の最大の差, 揺れる骨の向きの幅（Rust）)
    rows: Vec<(String, f32, f32, f32)>,
    worst_at: String,
    worst: f32,
    bad: Vec<String>,
}
impl Drive<'_> {
    fn row(&mut self, scene: &str) -> &mut (String, f32, f32, f32) {
        if !self.rows.iter().any(|r| r.0 == scene) { self.rows.push((scene.to_string(), 0.0, 0.0, 0.0)); }
        self.rows.iter_mut().find(|r| r.0 == scene).unwrap()
    }
    fn step(&mut self, scene: &str, dt: f64, speed: f32, armed: f32, rate: f64, turn: f64) {
        self.heading += turn * dt;
        self.x += self.heading.sin() * speed as f64 * dt;
        self.z += self.heading.cos() * speed as f64 * dt;
        let holder = Mat4::from_translation(Vec3::new(self.x as f32, 0.0, self.z as f32)) * Mat4::from_rotation_y(self.heading as f32);
        self.a.set_parent_world(match self.pose { Some((kind, at)) => (holder.as_dmat4() * pose_matrix(kind, (self.f - at) as f64)).as_mat4(), None => holder });
        self.a.move_(speed as f64, true, armed, Some(rate));
        self.a.update(dt, self.clock);
        self.clock += dt;
        let mut most = 0f32;
        for (b, q) in &self.spring { most = most.max(self.a.skel.r[*b].angle_between(*q)); }
        let r = self.row(scene);
        r.3 = r.3.max(most);
        let f = self.f;
        self.f += 1;
        if f % EVERY != 0 { return; }
        let m = &self.meta[self.mi];
        assert!(m[0] == scene && m[1] == f, "場面の順がずれた {m} {scene} {f}");
        let row = &self.bin[self.mi * self.stride..(self.mi + 1) * self.stride];
        self.mi += 1;
        let (mut all, mut sp) = (0f32, 0f32);
        for (k, (node, name)) in self.nodes.iter().enumerate() {
            let e = self.a.skel.world[*node];
            let vals = [e.w_axis.x, e.w_axis.y, e.w_axis.z, e.x_axis.x, e.x_axis.y, e.x_axis.z, e.y_axis.x, e.y_axis.y, e.y_axis.z];
            for (c, v) in vals.iter().enumerate() {
                let d = (v - row[k * 9 + c]).abs();
                if d > all { all = d; }
                if self.is_spring[k] && d > sp { sp = d; }
                if d > self.worst { self.worst = d; self.worst_at = format!("{scene} f{f} {name}[{c}] rust {v} ts {}", row[k * 9 + c]); }
            }
        }
        if std::env::var("SPRING_TRACE").is_ok() { println!("  {scene} f{f} 差 {all:.6} 揺れ {sp:.6} 角 {:.2}", most.to_degrees()); }
        let r = self.row(scene);
        r.1 = r.1.max(all);
        r.2 = r.2.max(sp);
    }
    fn run(&mut self, scene: &str, n: usize, turn: f64) { for _ in 0..n { self.step(scene, DT, RUN, 0.0, RATE, turn); } }
    fn stop(&mut self, scene: &str, n: usize, armed: f32) { for _ in 0..n { self.step(scene, DT, 0.0, armed, 1.0, 0.0); } }
    fn attack(&mut self) { if self.a.query(MotionKind::Attack).available { self.a.start(ActionKind::Attack, StartOptions::default()); } }
    fn hit(&mut self) { if self.a.query(MotionKind::Hit).available { self.a.hit(None, None); } }
}

/// tools/spring-parity.mjs と同じ場面
fn scenes(d: &mut Drive) {
    d.stop("idle", 60, 0.0);
    d.run("run", 90, 0.0);
    d.run("run", 60, 1.5);
    d.stop("stop", 60, 0.0);
    d.run("run-attack", 20, 0.0);
    d.attack();
    d.run("run-attack", 70, 0.5);
    d.stop("stop-attack", 20, 1.0);
    d.attack();
    d.stop("stop-attack", 70, 1.0);
    d.stop("stop-attack", 20, 0.0);
    d.run("run-hit", 20, 0.0);
    d.hit();
    d.run("run-hit", 50, -0.5);
    d.stop("stop-hit", 20, 0.0);
    d.hit();
    d.stop("stop-hit", 50, 0.0);
    for _ in 0..30 { d.step("30hz", 1.0 / 30.0, RUN, 0.0, RATE, 0.7); }
    for _ in 0..30 { d.step("30hz", 1.0 / 30.0, 0.0, 0.0, 1.0, 0.0); }
    for _ in 0..144 { d.step("144hz", 1.0 / 144.0, RUN, 0.0, RATE, -0.7); }
    for _ in 0..144 { d.step("144hz", 1.0 / 144.0, 0.0, 0.0, 1.0, 0.0); }
    let hiccup = [1.0 / 60.0, 1.0 / 60.0, 0.2, 0.001, 0.0, 1.0 / 30.0];
    for i in 0..60 { d.step("hiccup", hiccup[i % 6], RUN, 0.0, RATE, 0.0); }
    d.stop("hiccup", 30, 0.0);
    // 伸び縮みする入れ物
    d.pose = Some(("squash", d.f));
    d.run("squash", 60, 0.5);
    d.stop("squash", 60, 0.0);
    d.pose = Some(("ball", d.f));
    d.hit();
    d.stop("ball", 90, 0.0);
    d.pose = Some(("shear", d.f));
    d.run("shear", 60, 0.5);
    d.stop("shear", 60, 0.0);
    d.pose = None;
    d.stop("shear", 20, 0.0);
    d.x += 150.0;
    d.z -= 80.0;
    d.run("far", 60, 0.4);
    d.stop("far", 60, 0.0);
}

fn joints_of(cat: &Catalog, id: &str, quality: Quality) -> (usize, CharacterActor) {
    let a = load_character_actor(&cat.client, LoadOptions { id: id.to_string(), revision: None, quality, gait: Gait { walk: 0.15, run: 2.5 } }).unwrap();
    (a.spring.as_ref().map_or(0, |s| s.joint_nodes().len()), a)
}

#[test]
fn spring_bones_match_ts() {
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
    let dir = root.join(format!("ts-spring-three-0.178.0-{release_key}"));
    let changed = |p: PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let newest_source = [manifest.join("tools/spring-parity.mjs"), manifest.join("tools/actor-parity.mjs"), manifest.join("../../packages/character-actor/package.json"), manifest.join("../../packages/character-runtime/package.json"), manifest.join("../../packages/character-motion/package.json")].into_iter().filter_map(changed).max();
    let stale = ids.iter().any(|id| {
        let Ok(b) = std::fs::read(dir.join(format!("{id}.json"))) else { return true };
        if newest_source > changed(dir.join(format!("{id}.json"))) { return true; }
        let v: Value = serde_json::from_slice(&b).unwrap();
        if v["release"] != release { return true; }
        let rev = index["characters"][id]["current"].as_str();
        rev.is_some_and(|r| v["revision"].as_str() != Some(r)) || !dir.join(format!("{id}.bin")).exists()
    });
    if stale {
        let mut c = std::process::Command::new("node");
        c.arg(manifest.join("tools/spring-parity.mjs")).arg(&dir);
        if only.is_some() { c.args(&ids); }
        let st = c.status().expect("node を動かせません");
        assert!(st.success(), "TS の値を作れませんでした");
    }
    let (mut bad, mut worst_all, mut worst_spring) = (Vec::new(), 0f32, 0f32);
    let mut total = 0usize;
    for id in &ids {
        let ts: Value = serde_json::from_slice(&std::fs::read(dir.join(format!("{id}.json"))).unwrap()).unwrap();
        assert_eq!(ts["release"], release, "{id} の比較相手の配布 SHA が違います");
        assert_eq!(ts["three"].as_str(), Some("178"), "{id} の値は three {} で書いた物", ts["three"]);
        let raw = std::fs::read(dir.join(format!("{id}.bin"))).unwrap();
        let bin: Vec<f32> = raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
        for (key, q) in [("lite", Quality::Lite), ("normal", Quality::Normal)] {
            let (n, _) = joints_of(&cat, id, q);
            if ts["counts"][key].as_u64() != Some(n as u64) { bad.push(format!("{id} {key} の揺れる骨の数が違う rust {n} ts {}", ts["counts"][key])); }
        }
        let (n, mut a) = joints_of(&cat, id, Quality::Full);
        if ts["revision"].as_str() != Some(a.revision.as_str()) { bad.push(format!("{id} 版が違う")); continue; }
        if ts["counts"]["full"].as_u64() != Some(n as u64) { bad.push(format!("{id} full の揺れる骨の数が違う rust {n} ts {}", ts["counts"]["full"])); }
        // 動かす順（当たりの節点や親の揺れる骨が先）
        let sorted: Vec<String> = a.spring.as_ref().map_or(Vec::new(), |s| s.joint_nodes().iter().map(|&i| a.skel.names[i].clone()).collect());
        let ts_sorted: Vec<String> = ts["sorted"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect();
        if sorted != ts_sorted { bad.push(format!("{id} 動かす順が違う\n  rust {sorted:?}\n  ts   {ts_sorted:?}")); }
        let colliders = a.spring.as_ref().map_or(0, |s| s.collider_count());
        if ts["colliders"].as_u64() != Some(colliders as u64) { bad.push(format!("{id} 当たりの数が違う rust {colliders} ts {}", ts["colliders"])); }
        a.hold_weapon(Some(0.9));
        let joint_set = a.spring.as_ref().map_or(Vec::new(), |s| s.joint_nodes());
        let nodes: Vec<(usize, String)> = ts["nodes"].as_array().unwrap().iter().map(|n| { let n = n.as_str().unwrap(); (a.skel.find(n).unwrap_or_else(|| panic!("{id} 節点が無い {n}")), n.to_string()) }).collect();
        let is_spring: Vec<bool> = nodes.iter().map(|(i, _)| joint_set.contains(i)).collect();
        let spring: Vec<(usize, Quat)> = joint_set.iter().map(|&b| (b, a.glb.nodes[b].r)).collect();
        let stride = ts["stride"].as_u64().unwrap() as usize;
        let meta = ts["meta"].as_array().unwrap();
        let mut d = Drive { a: &mut a, nodes: &nodes, bin: &bin, stride, meta, spring, is_spring, x: 0.0, z: 0.0, heading: 0.0, clock: 0.0, f: 0, mi: 0, pose: None, rows: Vec::new(), worst_at: String::new(), worst: 0.0, bad: Vec::new() };
        scenes(&mut d);
        if d.mi != meta.len() { d.bad.push(format!("{id} 比べた点の数が違う {} {}", d.mi, meta.len())); }
        total += d.mi;
        let (all, sp) = (d.rows.iter().map(|r| r.1).fold(0.0, f32::max), d.rows.iter().map(|r| r.2).fold(0.0, f32::max));
        worst_all = worst_all.max(all);
        worst_spring = worst_spring.max(sp);
        let swing: Vec<String> = d.rows.iter().map(|r| format!("{} {:.1}度(ts {:.1})・差 {:.5}/{:.5}", r.0, r.3.to_degrees(), ts["angle"][&r.0].as_f64().unwrap_or(0.0).to_degrees(), r.2, r.1)).collect();
        println!("{id}: 揺れる骨 {n}・当たり {colliders} | lite {} normal {} | {} 点 | 全部の節点の最大の差 {all:.5}（{}）| 揺れる骨の最大の差 {sp:.5}", ts["counts"]["lite"], ts["counts"]["normal"], d.mi, d.worst_at);
        if n > 0 { println!("   {}", swing.join(" | ")); }
        if all > BONE_LIMIT { d.bad.push(format!("{id} 骨の差 {all:.5}（{}）", d.worst_at)); }
        bad.append(&mut d.bad);
    }
    println!("全員 {} 人・{total} 点・全部の節点の最大の差 {worst_all:.5}・揺れる骨の最大の差 {worst_spring:.5}", ids.len());
    assert!(bad.is_empty(), "TS の部品と違う:\n{}", bad.join("\n"));
}
