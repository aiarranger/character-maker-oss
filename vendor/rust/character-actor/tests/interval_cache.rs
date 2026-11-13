// Self-contained synthetic GLB: no network or private delivery assets. The
// locomotion cache must leave the complete actor output, including overlays,
// morph weights and gameplay cues, bit-identical to the default legacy path.
use character_actor::actor::{assemble, ActionKind, ActorEvent, CharacterActor, Cue, Gait, LoadOptions, Quality, ReleasePlan, StartOptions, StartResult};
use glam::{Mat4, Quat, Vec3};
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
struct Fixture {
    bytes: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
}

impl Fixture {
    fn floats(&mut self, values: &[f32], kind: &str, components: usize) -> usize {
        let offset = self.bytes.len();
        for v in values { self.bytes.extend(v.to_le_bytes()); }
        let view = self.views.len();
        self.views.push(json!({"buffer":0,"byteOffset":offset,"byteLength":values.len()*4}));
        let index = self.accessors.len();
        self.accessors.push(json!({"bufferView":view,"componentType":5126,"count":values.len()/components,"type":kind}));
        index
    }

    fn glb(mut self, mut document: Value) -> Vec<u8> {
        document["buffers"] = json!([{"byteLength":self.bytes.len()}]);
        document["bufferViews"] = json!(self.views);
        document["accessors"] = json!(self.accessors);
        let mut text = serde_json::to_vec(&document).unwrap();
        while text.len() % 4 != 0 { text.push(b' '); }
        while self.bytes.len() % 4 != 0 { self.bytes.push(0); }
        let mut out = Vec::new();
        for n in [0x46546c67u32, 2, (12 + 8 + text.len() + 8 + self.bytes.len()) as u32, text.len() as u32, 0x4e4f534a] { out.extend(n.to_le_bytes()); }
        out.extend(text);
        out.extend((self.bytes.len() as u32).to_le_bytes());
        out.extend(0x004e4942u32.to_le_bytes());
        out.extend(self.bytes);
        out
    }
}

fn actor() -> CharacterActor {
    let mut f = Fixture::default();
    let positions = f.floats(&[0.,0.,0., 1.,0.,0., 0.,1.,0.], "VEC3", 3);
    let normals = f.floats(&[0.,0.,1., 0.,0.,1., 0.,0.,1.], "VEC3", 3);
    let uv = f.floats(&[0.,0., 1.,0., 0.,1.], "VEC2", 2);
    let morph = f.floats(&[0.,0.,0., 0.,0.,0., 0.,0.1,0.], "VEC3", 3);
    let mut animations = Vec::new();
    for (index, name) in ["idle", "walk", "run", "attack"].iter().enumerate() {
        let duration = [1.0, 0.75, 0.5, 1.5333333][index];
        let times = [0.0, duration * 0.125, duration * 0.375, duration * 0.75, duration];
        let input = f.floats(&times, "SCALAR", 1);
        let mut samplers = Vec::new();
        let mut channels = Vec::new();
        for (path, kind, components) in [("translation", "VEC3", 3), ("rotation", "VEC4", 4), ("scale", "VEC3", 3), ("weights", "SCALAR", 1)] {
            let mut values = Vec::new();
            for k in 0..times.len() {
                let phase = ((k + index) % 5) as f32 * 0.1;
                match path {
                    "translation" => values.extend([phase, phase * 0.3, -phase * 0.1]),
                    "rotation" => values.extend(Quat::from_rotation_y(phase).to_array()),
                    "scale" => values.extend([1.0 + phase * 0.1, 1.0, 1.0]),
                    _ => values.push(phase),
                }
            }
            let output = f.floats(&values, kind, components);
            channels.push(json!({"sampler":samplers.len(),"target":{"node":0,"path":path}}));
            samplers.push(json!({"input":input,"output":output,"interpolation":"LINEAR"}));
        }
        animations.push(json!({"name":name,"samplers":samplers,"channels":channels}));
    }
    let model = f.glb(json!({
        "asset":{"version":"2.0"},"scene":0,"scenes":[{"nodes":[0]}],
        "nodes":[{"name":"FixtureBody","mesh":0}],
        "meshes":[{"name":"FixtureMesh","weights":[0.0],"extras":{"targetNames":["fixtureMorph"]},"primitives":[{
            "attributes":{"POSITION":positions,"NORMAL":normals,"TEXCOORD_0":uv},"targets":[{"POSITION":morph}]
        }]}],"animations":animations
    }));
    let manifest = json!({
        "id":"interval-fixture","revision":"test","actor":{"heightMeters":1.0},
        "packageManifest":{"coordinates":{"heightMeters":1.0,"forward":"+Z"},
            "motion":{"clips":{"idle":"idle","walk":"walk","run":"run","swordAttack":"attack"}}}
    });
    let plan = ReleasePlan { model: "lite".into(), ..Default::default() };
    let data = HashMap::from([("lite".into(), Arc::new(model))]);
    assemble(&LoadOptions { id:"interval-fixture".into(), revision:None, quality:Quality::Lite, gait:Gait::default() }, &plan, &manifest, &data).unwrap()
}

fn compare(a: &CharacterActor, b: &CharacterActor, tick: usize) {
    assert_eq!(a.skel.t, b.skel.t, "position at {tick}");
    assert_eq!(a.skel.r, b.skel.r, "rotation at {tick}");
    assert_eq!(a.skel.s, b.skel.s, "scale at {tick}");
    assert_eq!(a.skel.world, b.skel.world, "world at {tick}");
    assert_eq!(a.palette.mats, b.palette.mats, "palette at {tick}");
    assert_eq!(a.skel.morphs, b.skel.morphs, "node morphs at {tick}");
    assert_eq!(a.morph_weights, b.morph_weights, "upload morphs at {tick}");
    assert_eq!(a.current(), b.current(), "action at {tick}");
}

#[test]
fn cached_actor_preserves_fixed_tick_pose_morph_and_attack_cues() {
    let mut legacy = actor();
    let mut cached = actor();
    cached.set_locomotion_interval_cache(true);
    let mut events = Vec::new();
    for tick in 0..1200 {
        // Fresh actors model a fresh run; game reset/door below preserve their
        // clocks, cancel attack, move to idle, and only replace parent world.
        if tick == 900 {
            legacy = actor(); cached = actor();
            cached.set_locomotion_interval_cache(true);
        }
        let speed = [0.0, 1.0, 4.0, 0.0][(tick / 100) % 4];
        let mut step_events = Vec::new();
        for a in [&mut legacy, &mut cached] {
            if [40, 270, 520, 740, 940].contains(&tick) {
                assert!(matches!(a.start(ActionKind::Attack, StartOptions::default()), StartResult::Ok { .. }));
            }
            if [560, 780].contains(&tick) {
                a.stop(ActionKind::Attack);
                a.move_(0.0, true, 0.0, None);
                a.set_parent_world(Mat4::from_translation(Vec3::new(-5.0, 0.0, 0.0)));
                a.refresh();
            } else {
                a.move_(speed, true, if speed == 0.0 { 1.0 } else { 0.0 }, Some(1.0));
                a.set_parent_world(Mat4::from_rotation_translation(Quat::from_rotation_y(tick as f32 * 0.02), Vec3::new(tick as f32 * 0.001, 0.0, 0.0)));
            }
            // Host pause and dead actors skip update; a death tick refreshes
            // world only. Resuming/reusing that actor must retain its pose.
            if (180..200).contains(&tick) || (650..680).contains(&tick) {
                a.refresh();
                step_events.push(Vec::new());
            } else {
                step_events.push(a.update(1.0 / 60.0, tick as f64 / 60.0));
            }
        }
        assert_eq!(step_events[0], step_events[1], "events at {tick}");
        events.extend(step_events.remove(0));
        compare(&legacy, &cached, tick);
    }
    assert_eq!(events.iter().filter(|e| **e == ActorEvent::Cue(ActionKind::Attack, Cue::Strike)).count(), 5);
    assert_eq!(events.iter().filter(|e| **e == ActorEvent::End(ActionKind::Attack)).count(), 3);
    assert_eq!(events.iter().filter(|e| **e == ActorEvent::Interrupt(ActionKind::Attack)).count(), 2);
}

fn assert_runtime_only(a: &CharacterActor) {
    assert!(a.vertices.is_empty() && a.skin.is_empty() && a.indices.is_empty() && a.deltas.is_empty());
    assert_eq!((a.vertices.capacity(), a.skin.capacity(), a.indices.capacity(), a.deltas.capacity()), (0, 0, 0, 0));
    assert!(a.glb.images.is_empty() && a.glb.meshes.is_empty() && a.glb.animations.is_empty());
    if let Some(h) = &a.hurt {
        assert!(h.vertices.is_empty() && h.indices.is_empty() && h.tex.2.is_empty());
    }
}

fn template_matches_assembled(mut source: CharacterActor, mut legacy: CharacterActor) {
    let template = source.runtime_template().unwrap();
    let mut instance = template.spawn();
    let mut untouched = template.spawn();
    assert_runtime_only(&instance);
    assert_eq!(Arc::strong_count(&source.glb), 1, "the full GLB must remain uniquely owned for GPU upload/release");
    assert!(!Arc::ptr_eq(&source.glb, &instance.glb));
    compare(&legacy, &instance, 0);
    let untouched_pose = untouched.palette.mats.clone();
    let untouched_morphs = untouched.morph_weights.clone();
    let untouched_visible: Vec<_> = untouched.prims.iter().map(|p| p.visible).collect();
    legacy.set_locomotion_interval_cache(true);
    instance.set_locomotion_interval_cache(true);
    for tick in 0..420 {
        let mut events = Vec::new();
        for a in [&mut legacy, &mut instance] {
            if tick == 30 { assert!(matches!(a.start(ActionKind::Attack, StartOptions::default()), StartResult::Ok { .. })); }
            if tick == 160 { a.hit(Some(0.4), None); }
            let speed = [0.0, 1.0, 4.0][(tick / 60) % 3];
            a.move_(speed, true, 0.0, None);
            a.set_parent_world(Mat4::from_rotation_translation(Quat::from_rotation_y(tick as f32 * 0.01), Vec3::new(1.0, 0.0, -2.0)));
            events.push(a.update(1.0 / 60.0, 0.0));
        }
        assert_eq!(events[0], events[1], "template events at {tick}");
        compare(&legacy, &instance, tick);
        assert_eq!(legacy.prims.iter().map(|p| p.visible).collect::<Vec<_>>(), instance.prims.iter().map(|p| p.visible).collect::<Vec<_>>());
        assert_eq!(legacy.hurt.as_ref().map(|h| (h.visible, h.opacity)), instance.hurt.as_ref().map(|h| (h.visible, h.opacity)));
    }
    // Advancing one actor, including its cursor/action/display state, must not
    // mutate another actor or the pristine template used for later spawns.
    instance.prims[0].visible = !instance.prims[0].visible;
    assert_eq!(untouched.palette.mats, untouched_pose);
    assert_eq!(untouched.morph_weights, untouched_morphs);
    assert_eq!(untouched.prims.iter().map(|p| p.visible).collect::<Vec<_>>(), untouched_visible);
    assert_eq!(untouched.current(), None);
    compare(&source, &template.spawn(), 0);
    assert_eq!(source.update(1.0 / 60.0, 0.0), untouched.update(1.0 / 60.0, 0.0));
    compare(&source, &untouched, 1);
    // A source GLB can actually release its uploaded payload while all runtime
    // instances/template still exist; no full-asset Arc retained by spawn.
    let full = Arc::get_mut(&mut source.glb).unwrap();
    full.images.clear(); full.meshes.clear();
    let _ = instance.update(1.0 / 60.0, 0.0);
    assert_runtime_only(&template.spawn());
}

#[test]
fn runtime_template_matches_assembled_and_keeps_mutable_instances_independent() {
    template_matches_assembled(actor(), actor());
}

#[test]
fn runtime_template_rejects_started_or_modified_runtime() {
    let mut a = actor();
    a.update(0.0, 0.0);
    assert!(a.runtime_template().is_err(), "even update(0) samples the initial pose");
    let mut a = actor();
    a.move_(1.0, true, 0.0, None);
    assert!(a.runtime_template().is_err());
    let mut a = actor();
    a.start(ActionKind::Attack, StartOptions::default());
    assert!(a.runtime_template().is_err());
}

#[test]
#[ignore = "explicit local canonical delivery check; set CHARACTER_TEMPLATE_DELIVERY_ROOT; never fetches"]
fn delivered_kai_runtime_template_matches_assembled_offline() {
    use character_actor::{actor::release_plan, delivery::{checked_file, hash}};
    use std::{fs, path::PathBuf};
    let root = PathBuf::from(std::env::var("CHARACTER_TEMPLATE_DELIVERY_ROOT").expect("local shared delivery root"));
    let index: Value = serde_json::from_slice(&fs::read(root.join("channels/stable/v1/index.json")).unwrap()).unwrap();
    let revision = "44402bf7e0e71d40d4dc111f"; // game-slice's pinned canonical Kai.
    let read_checked = |descriptor: &Value| {
        let file = checked_file(descriptor).unwrap();
        let bytes = fs::read(root.join(file.path)).unwrap();
        assert_eq!(bytes.len() as u64, file.bytes);
        assert_eq!(hash(&bytes), file.sha256);
        bytes
    };
    let manifest: Value = serde_json::from_slice(&read_checked(&index["characters"]["kai"]["releases"][revision])).unwrap();
    let plan = release_plan(&manifest, Quality::Lite);
    let data = plan.files.iter().map(|key| (key.clone(), Arc::new(read_checked(&manifest["files"][key])))).collect();
    let options = LoadOptions { id: "kai".into(), revision: Some(revision.into()), quality: Quality::Lite, gait: Gait::default() };
    let source = assemble(&options, &plan, &manifest, &data).unwrap();
    let legacy = assemble(&options, &plan, &manifest, &data).unwrap();
    template_matches_assembled(source, legacy);
}
