// 確かめ用の window.__game（今の版の __game と同じ名前: mode・battle の値・capture・ranbu・tp・start・title）。ブラウザ版だけ。
// 今の版は battle へ直接書けるので、Rust 版だけのもの: set（主役の値）・boss（大将の値）・spawn・enemy・fling・hitEnemy・merit・clearEnemies・closeup・setTime・setCam・auto。
// 画面を出す: pause・result（今の戦いの結果の画面）。
// 読むだけ: values（描くのに実際に使った値を 1 コマ分）・ui（HUD と画面の文字の写し）・events（前に読んでから起きた出来事の名前）・sim（遊びの値）。
use crate::app::{App, Mode};
use js_sys::{JSON, Object, Reflect};
use serde_json::{Value, json};
use sim::battle::{BossState, EnemyState, EnemyKind, Battle, PlayerState, Owner};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen::closure::Closure;
use std::cell::Cell;

thread_local! {
    static APP: Cell<*mut App> = const { Cell::new(std::ptr::null_mut()) };
    static INSTALLED: Cell<bool> = const { Cell::new(false) };
}
/// 窓の出来事のたびに App の場所を覚える（一つのスレッドで、JS から呼ばれるのはコマの外だけ）
pub fn register(a: &mut App) {
    APP.with(|c| c.set(a as *mut App));
    if !INSTALLED.with(|c| c.replace(true)) {
        install();
        // 画面が隠れているときはタイマーで進める（今の版の setInterval）
        let f = Closure::<dyn FnMut()>::new(|| app().idle_tick());
        if let Some(w) = web_sys::window() { let _ = w.set_interval_with_callback_and_timeout_and_arguments_0(f.as_ref().unchecked_ref(), 33); }
        f.forget();
    }
}
fn app() -> &'static mut App { unsafe { &mut *APP.with(|c| c.get()) } }

type F4 = dyn FnMut(JsValue, JsValue, JsValue, JsValue) -> JsValue;
fn method(o: &Object, name: &str, f: impl FnMut(JsValue, JsValue, JsValue, JsValue) -> JsValue + 'static) {
    let c = Closure::<F4>::new(f);
    let _ = Reflect::set(o, &name.into(), c.as_ref());
    c.forget();
}
fn getter(o: &Object, name: &str, f: impl FnMut() -> JsValue + 'static) {
    let c = Closure::<dyn FnMut() -> JsValue>::new(f);
    let d = Object::new();
    let _ = Reflect::set(&d, &"get".into(), c.as_ref());
    let _ = Reflect::set(&d, &"enumerable".into(), &JsValue::TRUE);
    let _ = Object::define_property(o, &name.into(), &d);
    c.forget();
}
fn to_js(v: &Value) -> JsValue { JSON::parse(&v.to_string()).unwrap_or(JsValue::NULL) }
fn from_js(v: &JsValue) -> Value {
    if v.is_undefined() || v.is_null() { return Value::Null; }
    JSON::stringify(v).ok().and_then(|s| serde_json::from_str(&String::from(s)).ok()).unwrap_or(Value::Null)
}
fn n(v: &JsValue) -> f64 { v.as_f64().unwrap_or(0.0) }

fn b_of(a: &App) -> Option<&Battle> { a.g.as_ref().map(|g| &g.battle) }
fn cam_of(a: &App) -> Option<&crate::gfx::Camera> { a.g.as_ref().map(|g| &g.camera) }
fn mode_name(a: &App) -> &'static str {
    match a.g.as_ref().map(|g| g.mode) {
        None => "loading",
        Some(Mode::Title) => "title", Some(Mode::Select) => "select", Some(Mode::Battle) => "battle",
        Some(Mode::Paused) => "paused", Some(Mode::Result) => "result", Some(Mode::Gameover) => "gameover",
    }
}

pub fn kind_of(name: &str) -> Option<EnemyKind> {
    use EnemyKind::*;
    [Strawberry, Pudding, Macaron, Mikan, Donut, Shortcake, Cottoncandy, Creampuff, Cakeking].into_iter().find(|k| k.name() == name)
}
fn enemy_state(name: &str) -> Option<EnemyState> {
    use EnemyState::*;
    [Idle, Chase, Windup, Strike, Recover, Stagger, Air, Down, Getup, Dead, Carried, Stacked].into_iter().find(|k| k.name() == name)
}
fn boss_state(name: &str) -> Option<BossState> {
    use BossState::*;
    [Dormant, Enter, Idle, Walk, PunchL, PunchR, Stomp, Sauce, Roar, Hit, Down, Dead].into_iter().find(|k| k.name() == name)
}
fn player_state(name: &str) -> Option<PlayerState> {
    use PlayerState::*;
    [Free, Attack, Dodge, Hurt, Dead, Wire, Ride].into_iter().find(|k| k.name() == name)
}

/// 遊びの値（今の版の battle と同じ名前）
fn sim_json(b: &Battle) -> Value {
    let p = &b.player;
    json!({
        "time": b.time, "phase": b.phase.name(), "hitstop": b.hitstop, "timeScale": b.time_scale(), "gateOpen": b.gate_open, "bossY": b.boss_y, "ranbuFreeze": b.ranbu_freeze,
        "merit": b.merit, "continues": b.continues, "victoryT": b.victory_t, "giantOn": b.giant.on, "giantT": b.giant.t,
        "player": { "x": p.x, "y": p.y, "z": p.z, "vx": p.vx, "vz": p.vz, "vy": p.vy, "face": p.face, "hp": p.hp, "maxHp": p.max_hp, "ranbu": p.ranbu, "state": p.state.name(), "st": p.st, "move": p.r#move.map(|m| m.id), "moveT": p.move_t, "moveKey": p.move_key, "chain": p.chain, "air": p.air, "invuln": p.invuln, "hurtT": p.hurt_t, "kos": p.kos, "combo": p.combo, "maxCombo": p.max_combo, "dmgTaken": p.dmg_taken, "speed": p.speed, "carry": p.carry, "carryT": p.carry_t, "rank": p.rank },
        "boss": { "x": b.boss.x, "z": b.boss.z, "face": b.boss.face, "hp": b.boss.hp, "maxHp": b.boss.max_hp, "state": b.boss.state.name(), "st": b.boss.st, "cd": b.boss.cd, "poise": b.boss.poise, "hitT": b.boss.hit_t },
        "holds": b.holds.iter().map(|h| json!({ "id": h.id, "owner": if h.owner == Owner::Ally { "ally" } else { "enemy" }, "x": h.x, "z": h.z, "r": h.r, "captain": h.captain, "alarm": h.alarm })).collect::<Vec<_>>(),
        "enemies": b.enemies.iter().map(|e| json!({ "id": e.id, "kind": e.kind.name(), "role": e.role.name(), "alive": e.alive, "x": e.x, "y": e.y, "z": e.z, "vx": e.vx, "vz": e.vz, "vy": e.vy, "face": e.face, "hp": e.hp, "state": e.state.name(), "st": e.st, "spin": e.spin, "hitT": e.hit_t, "fling": e.fling, "chainT": e.chain_t, "flee": e.flee, "deadT": e.dead_t })).collect::<Vec<_>>(),
        "telegraphs": b.telegraphs.iter().map(|t| json!({ "id": t.id, "kind": t.kind.name(), "x": t.x, "z": t.z, "r": t.r, "face": t.face, "t": t.t, "fire": t.fire, "src": t.src.name(), "done": t.done })).collect::<Vec<_>>(),
        "projectiles": b.projectiles.iter().map(|s| json!({ "id": s.id, "x": s.x, "y": s.y, "z": s.z })).collect::<Vec<_>>(),
        "pickups": b.pickups.iter().map(|k| json!({ "id": k.id, "kind": k.kind.name(), "x": k.x, "z": k.z, "t": k.t })).collect::<Vec<_>>(),
    })
}

/// 描くのに実際に使った値を 1 コマ分（今の版は rust/compare/scenes.py の VALS_TS が同じ形で作る）
fn values(a: &App) -> Value {
    let Some(g) = a.g.as_ref() else { return json!({ "mode": "loading" }) };
    let f = a.last_final;
    let c = &g.camera;
    let bl = g.hero.blade();
    let act = g.hero.actor().and_then(|x| x.current()).map(|k| format!("{k:?}").to_lowercase());
    let p = &g.battle.player;
    json!({
        "mode": mode_name(a), "t": g.t_all,
        "cam": { "pos": c.pos.to_array(), "fov": c.fov, "yaw": g.cam_yaw, "camPos": g.cam_pos.to_array(), "camLook": g.cam_look.to_array() },
        "fx": { "trauma": g.trauma, "zoomBlur": g.zoom_blur, "speed": g.speed, "flash": g.flash, "redK": g.red_k, "gold": g.gold, "fovKick": g.fov_kick, "chainCam": g.chain_cam, "giantCam": g.giant_cam, "boss01": g.boss01, "cutinT": g.cutin_t, "bossIntroT": g.boss_intro_t, "resultT": g.result_t, "defeatT": g.defeat_t },
        "final": { "center": f.center, "zoom": f.zoom, "speed": f.speed, "gold": f.gold, "red": f.red, "flash": f.flash, "desat": f.desat, "vignette": f.vignette, "aberration": f.aberration },
        "hero": { "id": g.hero.info().map(|i| i.id.clone()), "cur": act, "yaw": g.hero.yaw(), "lean": g.hero.lean(), "aura": g.hero.aura(), "tip": bl.tip.to_array(), "base": bl.base.to_array(), "weapon": g.hero.weapon_id() },
        "p": { "x": p.x, "y": p.y, "z": p.z, "face": p.face, "state": p.state.name(), "move": p.r#move.map(|m| m.id), "moveT": p.move_t, "hp": p.hp, "ranbu": p.ranbu, "kos": p.kos, "combo": p.combo, "rank": p.rank, "speed": p.speed, "grow": g.battle.weapon_grow(), "wireCd": p.wire_cd, "wireTarget": p.wire.target, "air": p.air, "ame": p.ame, "ameFill": p.ame_fill, "wireChain": p.wire_chain, "wireEsc": p.wire_esc },
        "vfx": g.vfx.dump(),
        "loaded": g.loaded.len(), "alive": g.battle.alive_count(),
    })
}

fn install() {
    let o = Object::new();
    getter(&o, "mode", || mode_name(app()).into());
    getter(&o, "heroes", || to_js(&json!(app().g.as_ref().map(|g| g.heroes.iter().map(|h| json!({ "id": h.id, "name": h.name, "height": h.height, "tint": h.tint })).collect::<Vec<_>>()))));
    getter(&o, "heroId", || app().g.as_ref().and_then(|g| g.hero.info().map(|i| JsValue::from_str(&i.id))).unwrap_or(JsValue::NULL));
    getter(&o, "loadingHero", || JsValue::from_bool(app().g.as_ref().is_some_and(|g| g.loading_hero)));
    getter(&o, "weaponId", || app().g.as_ref().map(|g| JsValue::from_str(&g.weapon_id)).unwrap_or(JsValue::NULL));
    method(&o, "values", |_, _, _, _| to_js(&values(app())));
    method(&o, "sim", |_, _, _, _| app().g.as_ref().map(|g| to_js(&sim_json(&g.battle))).unwrap_or(JsValue::NULL));
    method(&o, "ui", |_, _, _, _| { let a = app(); to_js(&json!({ "hud": a.hud.dump(b_of(a), cam_of(a)), "screens": a.screens.dump(), "touch": a.input.touch(), "device": format!("{:?}", a.input.device()).to_lowercase(), "ring": a.input.ring() })) });
    method(&o, "events", |_, _, _, _| { let a = app(); to_js(&json!(std::mem::take(&mut a.ev_log))) });
    method(&o, "start", |_, _, _, _| { app().go(); JsValue::UNDEFINED });
    method(&o, "title", |_, _, _, _| { app().to_title(); JsValue::UNDEFINED });
    method(&o, "select", |_, _, _, _| { app().to_select(); JsValue::UNDEFINED });
    method(&o, "pick", |i, _, _, _| { app().pick_hero(n(&i) as i32); JsValue::UNDEFINED });
    method(&o, "weapon", |d, _, _, _| { app().step_weapon(n(&d) as i32); JsValue::UNDEFINED });
    method(&o, "pause", |on, _, _, _| { app().pause(on.is_truthy()); JsValue::UNDEFINED });
    // 今の戦いの結果の画面を出す（確かめ用。勝っていなければ「退却」）
    method(&o, "result", |_, _, _, _| { if app().g.is_some() { app().show_result(); } JsValue::UNDEFINED });
    method(&o, "capture", |i, _, _, _| { if let Some(g) = app().g.as_mut() { g.battle.debug_capture(n(&i) as usize); } JsValue::UNDEFINED });
    method(&o, "ranbu", |_, _, _, _| { if let Some(g) = app().g.as_mut() { g.battle.debug_ranbu(); } JsValue::UNDEFINED });
    method(&o, "tp", |x, z, _, _| { if let Some(g) = app().g.as_mut() { g.battle.debug_teleport(n(&x), n(&z)); } JsValue::UNDEFINED });
    method(&o, "merit", |x, _, _, _| { if let Some(g) = app().g.as_mut() { g.battle.debug_merit(n(&x) as i64); } JsValue::UNDEFINED });
    method(&o, "spawn", |kind, x, z, awake| {
        let Some(g) = app().g.as_mut() else { return JsValue::NULL };
        let Some(k) = kind.as_string().and_then(|s| kind_of(&s)) else { return JsValue::NULL };
        let i = g.battle.debug_spawn(k, n(&x), n(&z));
        if awake.is_truthy() { g.battle.enemies[i].awake = true; }
        JsValue::from_f64(g.battle.enemies[i].id as f64)
    });
    method(&o, "fling", |id, vx, vz, vy| {
        if let Some(g) = app().g.as_mut() && let Some(i) = g.battle.enemies.iter().position(|e| e.id == n(&id) as i64) { g.battle.debug_fling(i, n(&vx), n(&vz), if vy.is_undefined() { 2.0 } else { n(&vy) }); }
        JsValue::UNDEFINED
    });
    method(&o, "hitEnemy", |id, dmg, _, _| {
        if let Some(g) = app().g.as_mut() && let Some(i) = g.battle.enemies.iter().position(|e| e.id == n(&id) as i64) { g.battle.debug_hit_enemy(i, n(&dmg)); }
        JsValue::UNDEFINED
    });
    method(&o, "clearEnemies", |_, _, _, _| { if let Some(g) = app().g.as_mut() { for e in &mut g.battle.enemies { e.alive = false; e.token = false; } } JsValue::UNDEFINED });
    method(&o, "enemy", |id, v, _, _| {
        let Some(g) = app().g.as_mut() else { return JsValue::UNDEFINED };
        let Some(e) = g.battle.enemies.iter_mut().find(|e| e.id == n(&id) as i64) else { return JsValue::UNDEFINED };
        let v = from_js(&v);
        let f = |k: &str| v.get(k).and_then(|x| x.as_f64());
        if let Some(x) = f("x") { e.x = x; } if let Some(x) = f("y") { e.y = x; } if let Some(x) = f("z") { e.z = x; }
        if let Some(x) = f("vx") { e.vx = x; } if let Some(x) = f("vy") { e.vy = x; } if let Some(x) = f("vz") { e.vz = x; }
        if let Some(x) = f("hp") { e.hp = x; } if let Some(x) = f("st") { e.st = x; } if let Some(x) = f("cd") { e.cd = x; } if let Some(x) = f("face") { e.face = x; }
        if let Some(x) = f("aimX") { e.aim_x = x; } if let Some(x) = f("aimZ") { e.aim_z = x; }
        if let Some(s) = v.get("state").and_then(|x| x.as_str()).and_then(enemy_state) { e.state = s; }
        if let Some(x) = v.get("awake").and_then(|x| x.as_bool()) { e.awake = x; }
        JsValue::UNDEFINED
    });
    method(&o, "set", |v, _, _, _| {
        let Some(g) = app().g.as_mut() else { return JsValue::UNDEFINED };
        let v = from_js(&v);
        let f = |k: &str| v.get(k).and_then(|x| x.as_f64());
        let p = &mut g.battle.player;
        if let Some(x) = f("x") { p.x = x; } if let Some(x) = f("y") { p.y = x; } if let Some(x) = f("z") { p.z = x; }
        if let Some(x) = f("vx") { p.vx = x; } if let Some(x) = f("vy") { p.vy = x; } if let Some(x) = f("vz") { p.vz = x; }
        if let Some(x) = f("face") { p.face = x; } if let Some(x) = f("hp") { p.hp = x; } if let Some(x) = f("ranbu") { p.ranbu = x; }
        if let Some(x) = f("invuln") { p.invuln = x; } if let Some(x) = f("kos") { p.kos = x as i64; } if let Some(x) = f("rank") { p.rank = x as i32; } if let Some(x) = f("dodgeCd") { p.dodge_cd = x; } if let Some(x) = f("maxCombo") { p.max_combo = x as _; }
        if let Some(x) = f("ame") { p.ame = x as i32; }
        if let Some(s) = v.get("state").and_then(|x| x.as_str()).and_then(player_state) { p.state = s; }
        if let Some(x) = f("time") { g.battle.time = x; }
        JsValue::UNDEFINED
    });
    method(&o, "boss", |v, _, _, _| {
        let Some(g) = app().g.as_mut() else { return JsValue::UNDEFINED };
        let v = from_js(&v);
        let f = |k: &str| v.get(k).and_then(|x| x.as_f64());
        let b = &mut g.battle.boss;
        if let Some(x) = f("x") { b.x = x; } if let Some(x) = f("z") { b.z = x; } if let Some(x) = f("hp") { b.hp = x; } if let Some(x) = f("st") { b.st = x; } if let Some(x) = f("cd") { b.cd = x; } if let Some(x) = f("face") { b.face = x; } if let Some(x) = f("poise") { b.poise = x; } if let Some(x) = f("hitT") { b.hit_t = x; }
        if let Some(s) = v.get("state").and_then(|x| x.as_str()).and_then(boss_state) { b.state = s; }
        JsValue::UNDEFINED
    });
    // 1 コマだけ別の場所から撮る（on が false で戻す）。寄りの見比べ用
    method(&o, "closeup", |on, p, q, _| {
        let v = |x: &JsValue| { let j = from_js(x); let a = j.as_array()?; Some(glam::Vec3::new(a[0].as_f64()? as f32, a[1].as_f64()? as f32, a[2].as_f64()? as f32)) };
        if let Some(g) = app().g.as_mut() { g.closeup = if on.is_truthy() { v(&p).zip(v(&q)) } else { None }; }
        JsValue::UNDEFINED
    });
    // 時計と見せ方の値をそろえる（読み込みの長さで進んだ分をなくす）
    method(&o, "align", |v, _, _, _| {
        if let Some(g) = app().g.as_mut() {
            if let Some(t) = v.as_f64() { g.t_all = t as _; }
            g.trauma = 0.0; g.zoom_blur = 0.0; g.speed = 0.0; g.flash = 0.0; g.red_k = 0.0; g.gold = 0.0; g.fov_kick = 0.0; g.chain_cam = 0.0; g.giant_cam = 0.0; g.chain_at = glam::DVec3::ZERO; g.cam_idle = 0.0;
            let f = g.battle.player.face;
            g.hero.dbg_align(f as _);
        }
        JsValue::UNDEFINED
    });
    method(&o, "camTo", |p, q, _, _| {
        let v = |x: &JsValue| { let j = from_js(x); let a = j.as_array()?; Some(glam::DVec3::new(a[0].as_f64()?, a[1].as_f64()?, a[2].as_f64()?)) };
        if let (Some(g), Some(a), Some(b)) = (app().g.as_mut(), v(&p), v(&q)) { g.cam_pos = a; g.cam_look = b; }
        JsValue::UNDEFINED
    });
    // ボタンの役（今の版は document.getElementById(...).click()）
    method(&o, "act", |name, _, _, _| {
        use crate::ui::UiAction as U;
        let a = app();
        match name.as_string().as_deref() {
            Some("start") => a.act(U::Start), Some("go") => a.act(U::Go), Some("resume") => a.act(U::Resume), Some("title") => a.act(U::ToTitle), Some("select") => a.act(U::ToSelect),
            Some("again") => a.act(U::Again), Some("continue") => a.act(U::Continue), Some("sound") => a.act(U::ToggleSound),
            _ => {}
        }
        JsValue::UNDEFINED
    });
    // 拾い物を置く（今の版は battle.pickups へ直接）
    method(&o, "pickup", |kind, id, x, z| {
        use sim::battle::{Pickup, PickupKind};
        if let Some(g) = app().g.as_mut() { g.battle.pickups.push(Pickup { id: n(&id) as i64, kind: if kind.as_string().as_deref() == Some("bun") { PickupKind::Bun } else { PickupKind::Konpeito }, x: n(&x), z: n(&z), t: 0.0, alive: true }); }
        JsValue::UNDEFINED
    });
    method(&o, "hideUi", |on, _, _, _| { app().ui.hide = on.is_truthy(); JsValue::UNDEFINED });
    method(&o, "renderSize", |_, _, _, _| { let (w, h) = app().gfx.render_size(); to_js(&json!([w, h])) });
    method(&o, "setTime", |t, _, _, _| { if let Some(g) = app().g.as_mut() { g.t_all = n(&t) as _; } JsValue::UNDEFINED });
    method(&o, "setCam", |y, _, _, _| { if let Some(g) = app().g.as_mut() { g.cam_yaw = n(&y) as _; } JsValue::UNDEFINED });
    method(&o, "auto", |on, _, _, _| { if let Some(g) = app().g.as_mut() { g.auto = on.is_truthy(); } JsValue::UNDEFINED });
    if let Some(w) = web_sys::window() { let _ = Reflect::set(&w, &"__game".into(), &o); }
}
