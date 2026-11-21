// 確かめ用の window.__game（TS と同じ名前: debug・perf・race・mode・lineup・start・title・select・pause・result・advance・setPolicy）。
// 読むだけの race は JSON の写し（同じ名前。track の関数は無いので、track は表の代わりに def・lap・raceLen などの値）。
// Rust 版だけ: camera・cup・heroes・hero・save・views・final・music・stats（コマの時間の統計）・setInput（advance に入力を渡さず、以後の入力を固定する）・emptyEvents・ui（画面の文字）・act（画面のボタンを押す）。
// TS の __game.snap（開発サーバーへ 1 枚送る）は静的な配信に受け口が無いので持たない。撮るのは Playwright（rust/compare）。
use crate::app::{App, Shared, with};
use js_sys::{Function, JSON, Object, Reflect};
use serde_json::{Value, json};
use sim::race::{Input, Race, Runner};
use std::rc::Rc;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;

fn to_js(v: &Value) -> JsValue { JSON::parse(&v.to_string()).unwrap_or(JsValue::NULL) }
fn method(o: &Object, name: &str, f: impl FnMut(JsValue, JsValue) -> JsValue + 'static) {
    let c = Closure::<dyn FnMut(JsValue, JsValue) -> JsValue>::new(f);
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

fn runner_json(x: &Runner) -> Value {
    let s = &x.stats;
    json!({
        "i": x.i, "gate": x.gate, "isPlayer": x.is_player, "dist": x.dist, "s": x.s, "d": x.d, "vd": x.vd, "y": x.y, "vy": x.vy, "air": x.air, "speed": x.speed, "stamina": x.stamina, "dashing": x.dashing, "dashK": x.dash_k,
        "sinceDash": x.since_dash, "exhausted": x.exhausted, "stumble": x.stumble, "wet": x.wet, "blocked": x.blocked, "blockedFor": x.blocked_for, "draft": x.draft, "draftFor": x.draft_for, "slip": x.slip, "slipBurst": x.slip_burst,
        "lv": x.lv, "weapon": x.weapon.map(|w| w.id()), "weaponT": x.weapon_t, "weaponUses": x.weapon_uses, "charge": x.charge, "immune": x.immune, "hop": x.hop,
        "rankLog": x.rank_log, "corners": x.corners, "rankAtFinal": x.rank_at_final, "finished": x.finished, "finishT": x.finish_t, "place": x.place, "rank": x.rank, "jumpBuf": x.jump_buf, "nextOb": x.next_ob, "tactic": x.tactic.id(),
        "stats": { "dashEarly": s.dash_early, "dashLate": s.dash_late, "blocked": s.blocked, "outer": s.outer, "hurdles": s.hurdles, "cleared": s.cleared, "wet": s.wet, "empties": s.empties, "finalStamina": s.final_stamina, "passes": s.passes, "draft": s.draft, "laneSum": s.lane_sum, "laneN": s.lane_n, "slings": s.slings, "lateLaneSum": s.late_lane_sum, "lateLaneN": s.late_lane_n, "ledFor": s.led_for, "marks": s.marks, "hitsDealt": s.hits_dealt, "chargedHits": s.charged_hits, "hitsTaken": s.hits_taken, "dodges": s.dodges },
    })
}

pub fn race_json(r: &Race) -> Value {
    let tr = &r.track;
    json!({
        "t": r.t, "started": r.started, "done": r.done, "finishedCount": r.finished_count, "leader": r.leader, "acc": r.acc, "finalAnnounced": r.final_announced, "lapShown": r.lap_shown, "order": r.order, "difficulty": r.difficulty.id(),
        "runners": r.runners.iter().map(runner_json).collect::<Vec<_>>(),
        "player": r.player().map(runner_json),
        "obstacleList": r.obstacle_list.iter().map(|e| json!({ "dist": e.dist, "ob": { "kind": e.ob.kind.name(), "s": e.ob.s, "len": e.ob.len, "d0": e.ob.d0, "d1": e.ob.d1 } })).collect::<Vec<_>>(),
        "pickups": r.pickups.iter().map(|p| json!({ "dist": p.dist, "d": p.d, "kind": p.kind.id(), "taken": p.taken })).collect::<Vec<_>>(),
        "shots": r.shots.iter().map(|s| json!({ "dist": s.dist, "d": s.d, "owner": s.owner, "charged": s.charged })).collect::<Vec<_>>(),
        "traps": r.traps.iter().map(|t| json!({ "dist": t.dist, "d": t.d, "owner": t.owner, "r": t.r, "life": t.life })).collect::<Vec<_>>(),
        "track": { "def": { "id": tr.def.id, "name": tr.def.name, "laps": tr.def.laps, "startBack": tr.def.start_back }, "lap": tr.lap, "raceLen": tr.race_len, "width": tr.width, "finishS": tr.finish_s, "startS": tr.start_s, "N": tr.n },
    })
}

/// setPolicy の関数を呼ぶ（dt, race の写し）→ 入力
pub fn run_policy(f: &Function, dt: f64, a: &App) -> Input {
    // 引数 1 つの関数には写しを作らない（測るとき、この手間が入らないように）
    let r = if f.length() >= 2 { to_js(&race_json(&a.race)) } else { JsValue::UNDEFINED };
    let Ok(v) = f.call2(&JsValue::NULL, &JsValue::from_f64(dt), &r) else { return Input::NONE };
    let get = |k: &str| Reflect::get(&v, &k.into()).unwrap_or(JsValue::UNDEFINED);
    Input { x: get("x").as_f64().unwrap_or(0.0), dash: get("dash").is_truthy(), jump: get("jump").is_truthy(), attack: get("attack").is_truthy() }
}

fn input_of(v: &JsValue) -> Option<Input> {
    if v.is_undefined() || v.is_null() { return None; }
    let get = |k: &str| Reflect::get(v, &k.into()).unwrap_or(JsValue::UNDEFINED);
    Some(Input { x: get("x").as_f64().unwrap_or(0.0), dash: get("dash").is_truthy(), jump: get("jump").is_truthy(), attack: get("attack").is_truthy() })
}

pub fn install(shared: &Shared) {
    let o = Object::new();
    let wk = Rc::downgrade(shared);
    let _ = Reflect::set(&o, &"debug".into(), &shared.borrow().debug);
    let s = shared.clone();
    getter(&o, "mode", move || JsValue::from_str(s.borrow().mode.name()));
    let s = shared.clone();
    getter(&o, "race", move || to_js(&race_json(&s.borrow().race)));
    let s = shared.clone();
    getter(&o, "lineup", move || { let a = s.borrow(); to_js(&json!(a.lineup.iter().map(|h| json!({ "id": a.heroes[*h].id, "name": a.heroes[*h].name, "thumb": a.heroes[*h].thumb })).collect::<Vec<_>>())) });
    let s = shared.clone();
    getter(&o, "heroes", move || { let a = s.borrow(); to_js(&json!(a.heroes.iter().map(|h| h.id.clone()).collect::<Vec<_>>())) });
    let s = shared.clone();
    getter(&o, "hero", move || { let a = s.borrow(); JsValue::from_str(&a.heroes[a.hero_i].id) });
    let s = shared.clone();
    getter(&o, "views", move || { let a = s.borrow(); to_js(&json!(a.views.keys().cloned().collect::<Vec<_>>())) });
    let s = shared.clone();
    getter(&o, "courseId", move || JsValue::from_str(&s.borrow().course_id));
    let s = shared.clone();
    getter(&o, "cup", move || { let a = s.borrow(); match &a.cup { Some(c) => to_js(&json!({ "index": c.index, "rows": c.rows.iter().map(crate::save::row_to_json).collect::<Vec<_>>() })), None => JsValue::NULL } });
    let s = shared.clone();
    getter(&o, "career", move || { let a = s.borrow(); a.career.as_ref().map_or(JsValue::NULL, |c| to_js(&json!({ "index": c.index, "levels": c.levels, "prize": c.prize, "wins": c.wins, "places": c.places, "retired": c.retired, "hero": c.hero }))) });
    let s = shared.clone();
    getter(&o, "save", move || to_js(&s.borrow().save.data));
    let s = shared.clone();
    getter(&o, "camera", move || { let a = s.borrow(); let c = a.kit.camera; to_js(&json!({ "position": { "x": c.position.x, "y": c.position.y, "z": c.position.z }, "fov": c.fov, "aspect": c.aspect, "near": c.near, "far": c.far })) });
    let s = shared.clone();
    getter(&o, "final", move || { let a = s.borrow(); let f = a.final_u; to_js(&json!({ "uBlur": f.blur, "uDim": f.dim, "uGold": f.gold, "uSpeed": f.speed, "uFlash": f.flash, "uDesat": f.desat, "uWarm": f.warm, "uCenter": f.center, "uTime": f.time })) });
    let s = shared.clone();
    getter(&o, "music", move || s.borrow().audio.scene_name().map_or(JsValue::NULL, JsValue::from_str));
    let s = shared.clone();
    getter(&o, "muted", move || JsValue::from_bool(s.borrow().audio.muted()));
    let s = shared.clone();
    getter(&o, "perf", move || { let a = s.borrow(); let p = &a.perf; to_js(&json!({ "frames": p.frames, "slow": p.slow, "tickMs": p.tick_ms, "worst": p.worst, "fps": 0 })) });
    // 確かめ用: 結果の画面の祝いで鳴らした合図（tick・done・best・bestTop・encore）と、紙吹雪・数え上げの今の値
    let s = shared.clone();
    getter(&o, "cheer", move || { let a = s.borrow(); let mut v = a.ui.dump()["cheer"].clone(); v["log"] = json!(a.cheer_log); to_js(&v) });
    let s = shared.clone();
    getter(&o, "device", move || JsValue::from_str(match s.borrow().device() { island_kit::app::input::Device::Keys => "keys", island_kit::app::input::Device::Pad => "pad", island_kit::app::input::Device::Touch => "touch" }));

    let w = wk.clone();
    method(&o, "start", move |cup_on, course| {
        let cup_on = cup_on.is_truthy();
        let course = course.as_string();
        with(&w, move |a| {
            a.set_cup_mode(cup_on);
            if let Some(c) = course { a.course_id = c.clone(); a.save.s.course = c; }
            a.go();
        });
        JsValue::UNDEFINED
    });
    let w = wk.clone();
    method(&o, "title", move |_, _| { with(&w, |a| a.to_title()); JsValue::UNDEFINED });
    let w = wk.clone();
    method(&o, "select", move |_, _| { with(&w, |a| a.to_select()); JsValue::UNDEFINED });
    let w = wk.clone();
    method(&o, "pause", move |on, _| { let on = on.is_undefined() || on.is_truthy(); with(&w, move |a| a.set_paused(on)); JsValue::UNDEFINED });
    let w = wk.clone();
    method(&o, "result", move |_, _| { with(&w, |a| a.to_result()); JsValue::UNDEFINED });
    let w = wk.clone();
    method(&o, "again", move |_, _| { with(&w, |a| a.again()); JsValue::UNDEFINED });
    // 決まった刻み（1/60 秒）で sec 秒進めてから1枚描く
    let s = shared.clone();
    method(&o, "advance", move |sec, inp| {
        let mut a = s.borrow_mut();
        let n = sim_math::round(sec.as_f64().unwrap_or(0.0) * 60.0) as i64;
        let was = a.debug_input;
        if let Some(i) = input_of(&inp) { a.debug_input = Some(i); }
        for i in 0..n { a.tick(Some(1.0 / 60.0), i == n - 1); }
        a.debug_input = was;
        JsValue::UNDEFINED
    });
    // 確かめ用: レース中の入力を関数で与える（実際の規則を同じ道で通る）
    let w = wk.clone();
    method(&o, "setPolicy", move |f, _| {
        let f: Option<Function> = if f.is_function() { Some(f.into()) } else { None };
        with(&w, move |a| a.debug_policy = f);
        JsValue::UNDEFINED
    });
    let w = wk.clone();
    method(&o, "setInput", move |i, _| { let i = input_of(&i); with(&w, move |a| a.debug_input = i); JsValue::UNDEFINED });
    // 確かめ用（TS は race を直接書き換える）: 走者を置く
    let s2 = shared.clone();
    method(&o, "place", move |i, v| {
        let mut a = s2.borrow_mut();
        let i = i.as_f64().unwrap_or(0.0) as usize;
        let get = |k: &str| Reflect::get(&v, &k.into()).ok().and_then(|x| x.as_f64());
        let tr_s = get("dist").map(|d| a.race.track.s_at(d));
        let x = &mut a.race.runners[i];
        if let Some(d) = get("dist") { x.dist = d; x.s = tr_s.unwrap(); }
        if let Some(d) = get("d") { x.d = d; }
        if let Some(d) = get("speed") { x.speed = d; }
        if let Some(d) = get("stamina") { x.stamina = d; }
        if let Some(d) = get("exhausted") { x.exhausted = d; }
        if let Some(d) = get("slip") { x.slip = d; }
        if let Some(d) = get("slipBurst") { x.slip_burst = d; }
        if let Some(d) = get("y") { x.y = d; }
        if let Some(d) = get("vy") { x.vy = d; }
        if let Some(d) = get("vd") { x.vd = d; }
        if let Some(d) = get("nextOb") { x.next_ob = d as usize; }
        // 確かめ用: 噴射の残りの秒
        if let Some(d) = get("jet") { x.jet = d; }
        // 確かめ用: 武器を持たせる（"sword"・"chain"・"rose"・"bottle"）
        if let Some(w) = Reflect::get(&v, &"weapon".into()).ok().and_then(|w| w.as_string()) && let Some(w) = sim::items::Weapon::ALL.into_iter().find(|k| k.id() == w) { x.weapon = Some(w); x.weapon_t = sim::items::ITEM.weapon_time; x.weapon_uses = w.uses(); }
        if let Ok(b) = Reflect::get(&v, &"air".into()) && !b.is_undefined() { x.air = b.is_truthy(); }
        if Reflect::get(&v, &"noBrain".into()).ok().is_some_and(|b| b.is_truthy()) { a.race.clear_brain(i); }
        JsValue::UNDEFINED
    });
    // 確かめ用: 描くのに使った値・骨・中の値・寄り（比べる道具が読む）
    let s3 = shared.clone();
    method(&o, "trace", move |_, _| to_js(&s3.borrow().trace_json()));
    let s3 = shared.clone();
    method(&o, "bones", move |id, _| to_js(&s3.borrow().bones_json(&id.as_string().unwrap_or_default())));
    let s3 = shared.clone();
    method(&o, "events", move |_, _| { let mut a = s3.borrow_mut(); let v = std::mem::take(&mut a.ev_log); to_js(&json!(v)) });
    let s3 = shared.clone();
    method(&o, "trackPos", move |s_, d| { let a = s3.borrow(); let p = a.race.track.pos(s_.as_f64().unwrap_or(0.0), d.as_f64().unwrap_or(0.0)); to_js(&json!([p.x, p.y, p.z])) });
    let s3 = shared.clone();
    getter(&o, "internals", move || to_js(&s3.borrow().internals_json()));
    let w = wk.clone();
    method(&o, "camAt", move |c, _| {
        let v: Option<Vec<f64>> = if c.is_null() || c.is_undefined() { None } else { Some(js_sys::Array::from(&c).iter().map(|x| x.as_f64().unwrap_or(0.0)).collect()) };
        with(&w, move |a| a.cam_at = v.map(|v| (glam::DVec3::new(v[0], v[1], v[2]), glam::DVec3::new(v[3], v[4], v[5]))));
        JsValue::UNDEFINED
    });
    // 確かめ用: 画面の文字（今の版の DOM の中身と同じ名前で比べる）と、ボタンを押す
    let s3 = shared.clone();
    method(&o, "ui", move |_, _| to_js(&s3.borrow().ui.dump()));
    // 確かめ用: 届いている指・マウスの入力を、コマを進めずに読む（今の版の click はコマの間に効く）
    let w = wk.clone();
    method(&o, "uiInput", move |_, _| { with(&w, |a| a.pointer_now()); JsValue::UNDEFINED });
    let s3 = shared.clone();
    method(&o, "acts", move |_, _| { let mut a = s3.borrow_mut(); let v = std::mem::take(&mut a.acts_log); to_js(&json!(v)) });
    let s3 = shared.clone();
    method(&o, "ctl", move |_, _| { let mut a = s3.borrow_mut(); let c = std::mem::take(&mut a.ctl_seen); to_js(&json!({ "x": c.x, "y": c.y, "dash": c.act_dash, "holdDash": c.hold_dash, "jump": c.act_jump, "holdJump": c.hold_jump, "confirm": c.confirm_pressed })) });
    let s3 = shared.clone();
    method(&o, "uiRects", move |_, _| {
        let a = s3.borrow();
        let v: Vec<Value> = a.ui.hits.rects().iter().map(|(act, r, c)| { let r = r.intersect(*c); json!({ "act": format!("{act:?}"), "x": r.center().x, "y": r.center().y, "w": r.width(), "h": r.height() }) }).collect();
        to_js(&json!(v))
    });
    let w = wk.clone();
    method(&o, "act", move |name, i| {
        use crate::ui::Act;
        let i = i.as_f64().unwrap_or(0.0) as usize;
        let a = match name.as_string().unwrap_or_default().as_str() {
            "start" => Act::Start, "records" => Act::Records, "recClose" => Act::RecClose, "go" => Act::Go, "again" => Act::Again, "menu" => Act::Menu, "pause" => Act::Pause, "mute" => Act::Mute,
            "online" => Act::Online,
            "mode" => Act::Mode(i == 1), "career" => Act::Career, "careerNew" => Act::CareerNew, "course" => Act::Course(i), "diff" => Act::Diff(i), "hero" => Act::Hero(i), "pauseBtn" => Act::PauseBtn(i), "key" => Act::Key(i),
            _ => return JsValue::FALSE,
        };
        with(&w, move |app| app.act(a));
        JsValue::TRUE
    });
    let _ = Reflect::set(&dom_window(), &"__game".into(), &o);
}

fn dom_window() -> JsValue { web_sys::window().map_or(JsValue::NULL, JsValue::from) }
