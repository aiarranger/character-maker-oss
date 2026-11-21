// 走者の見た目: 人の形の走者は characters の部品（character_actor）、魚は view/fish.rs、イカは view/squid.rs。
// ゲームは合図（sim::hero_signals）と、入れ物ごとの傾き（前傾・カーブの内傾）、輪郭の光・丸い影・枠番の札を持つ。かけっこグランプリ（keiba）の写し。
use super::fish::{Drive, FishBody, FishDef};
use super::squid::{SquidBody, SquidDrive};
use super::materials::blob_shadow_material;
use character_actor::actor::{ActionKind, FaceControl, MotionKind, StartOptions};
use character_actor::delivery::HeroEntry;
use character_view::ActorLook;
use glam::Vec3;
use island_kit::geometry::plane;
use island_kit::scene::Rot;
use island_kit::{Color, GeoId, Kit, MatId, Material, MatKind, NodeId, TexId};
use sim::hero_signals::{HeroCommand, HeroPrev, hero_commands, hero_rate};
use sim::race::{Runner, TUNE};
use sim::track::Track;
use sim_math::{atan2, cos, sin};
use std::cell::RefCell;
use std::collections::HashMap;

/// 日本の競走の枠の色（1 白 2 黒 3 赤 4 青 5 黄 6 緑 7 橙 8 桃）。色だけに頼らず番号も出す
pub const GATE_COLOR: [&str; 8] = ["#f7f3e8", "#25262b", "#e23b35", "#2f6fd8", "#f5cc2a", "#2f9e5b", "#f28a24", "#f39ac0"];
pub const GATE_TEXT: [&str; 8] = ["#25262b", "#f7f3e8", "#fff", "#fff", "#25262b", "#fff", "#fff", "#25262b"];

/// 走者の体の種類
#[derive(Clone)]
pub enum Body {
    /// 配布の主役（きゃしー）
    Hero,
    /// Studio で作ったキャラ（送られた .character.zip）
    Guest(std::sync::Arc<character_actor::package::CharacterAsset>),
    /// 魚（Fish Kit）
    Fish(&'static FishDef),
    /// イカ（Squid Kit）
    Squid(&'static SquidDef),
}

/// 走者にするイカ（docs/gyogyo-spec.md の「走る人たち」）。length は外套の先から腕の先までの全長（m、場の大きさ）
pub struct SquidDef { pub id: &'static str, pub name: &'static str, pub length: f32 }
pub const SQUIDS: [SquidDef; 2] = [
    SquidDef { id: "aori", name: "アオリイカ", length: 3.4 },
    SquidDef { id: "kouika", name: "コウイカ", length: 2.7 },
];

#[derive(Clone)]
pub struct HeroInfo {
    pub id: String, pub name: String,
    /// 顔の札・選ぶ画面の絵（URL）。配布の主役は配布の絵、魚とイカは racers/<id>.webp（絵全体を札に出す: portrait = true）
    pub thumb: String,
    pub portrait: bool, pub height: f32, pub select_file: Option<character_actor::delivery::DeliveryFile>,
    /// 部品の空間 → 場の倍率（配布の主役は全員共通の COMMON_SCALE。つくったキャラは背をきゃしーにそろえる倍率。魚とイカは 1）
    pub scale: f64,
    pub body: Body,
}

/// 走者の見た目の倍率。全員共通の 1 つだけ（共通ルール: 本人どうしの背の比は保つ）。当たりの大きさは全員同じで、見た目と別（企画 07）
pub const COMMON_SCALE: f64 = 1.7;
/// 背の高さの目安（印や札の位置）
pub const HERO_HEIGHT: f64 = 1.95;
/// 魚の腹と地面のすき間（m）
const FISH_HOVER: f64 = 0.35;
/// 選ぶ画面・表彰で、魚・イカを人の向きから回す角度
const CREATURE_POSE_TURN: f64 = -1.15;

/// 名簿: 配布の主役（きゃしー）→ つくったキャラ → 魚 → イカ
pub fn to_hero_list(heroes: Vec<HeroEntry>, guest: Option<HeroInfo>) -> Vec<HeroInfo> {
    let mut list: Vec<HeroInfo> = heroes.into_iter().map(|h| HeroInfo { id: h.id, name: h.name, thumb: h.select, portrait: h.select_portrait, height: h.height, select_file: h.select_file, scale: COMMON_SCALE, body: Body::Hero }).collect();
    list.extend(guest);
    for f in &super::fish::FISH {
        list.push(HeroInfo { id: f.id.to_string(), name: f.name.to_string(), thumb: format!("racers/{}.webp", f.id), portrait: true, height: 1.0, select_file: None, scale: 1.0, body: Body::Fish(f) });
    }
    for q in &SQUIDS {
        list.push(HeroInfo { id: q.id.to_string(), name: q.name.to_string(), thumb: format!("racers/{}.webp", q.id), portrait: true, height: 1.0, select_file: None, scale: 1.0, body: Body::Squid(q) });
    }
    list
}

/// 人の形でない走者の体（魚・イカ）。lean の下の body に置く
pub enum Creature { Fish(FishBody), Squid(SquidBody) }

thread_local! {
    static GATE_TEX: RefCell<HashMap<usize, TexId>> = RefCell::new(HashMap::new());
    pub static FACE_TEX: RefCell<HashMap<(String, usize), Option<TexId>>> = RefCell::new(HashMap::new());
    static SHARED_GEO: RefCell<Option<(GeoId, MatId)>> = const { RefCell::new(None) };
}

pub fn gate_texture(kit: &mut Kit, gate: usize) -> TexId {
    if let Some(t) = GATE_TEX.with(|m| m.borrow().get(&gate).copied()) { return t; }
    let t = kit.canvas_texture(128, 128, true, |g| {
        g.set_fill_style_str(GATE_COLOR[gate - 1]);
        g.begin_path();
        let _ = g.round_rect_with_f64(10.0, 10.0, 108.0, 108.0, 22.0);
        g.fill();
        g.set_line_width(6.0);
        g.set_stroke_style_str(if gate == 2 { "rgba(255,255,255,.9)" } else { "rgba(20,20,30,.55)" });
        g.stroke();
        g.set_fill_style_str(GATE_TEXT[gate - 1]);
        g.set_font("700 84px Fredoka, sans-serif");
        g.set_text_align("center");
        g.set_text_baseline("middle");
        let _ = g.fill_text(&gate.to_string(), 64.0, 70.0);
    });
    GATE_TEX.with(|m| { m.borrow_mut().insert(gate, t); });
    t
}

/// 頭の上の札: 選ぶ画面の顔の絵に枠の色の縁
pub fn face_tag_texture(kit: &mut Kit, img: &web_sys::HtmlImageElement, gate: usize) -> TexId {
    kit.canvas_texture(128, 128, true, |g| {
        g.begin_path();
        let _ = g.arc(64.0, 64.0, 62.0, 0.0, 6.3);
        g.set_fill_style_str(GATE_COLOR[gate - 1]);
        g.fill();
        g.save();
        g.begin_path();
        let _ = g.arc(64.0, 64.0, 50.0, 0.0, 6.3);
        g.clip();
        g.set_fill_style_str("#fff6e4");
        g.fill_rect(0.0, 0.0, 128.0, 128.0);
        let (w, h) = (img.natural_width() as f64, img.natural_height() as f64);
        let sq = w.min(h);
        let _ = g.draw_image_with_html_image_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(img, (w - sq) / 2.0, 0.0, sq, sq, 14.0, 14.0, 100.0, 100.0);
        g.restore();
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood { Calm, Push, Tired, Joy, Sad }
fn face(m: Mood) -> FaceControl {
    let base = FaceControl { emotion: "neutral", emotion_weight: 0.0, mouth: "neutral", mouth_weight: 0.0, look: "neutral", look_weight: 0.0 };
    match m {
        Mood::Calm => base,
        Mood::Push => FaceControl { emotion: "angry", emotion_weight: 0.35, mouth: "ih", mouth_weight: 0.4, ..base },
        Mood::Tired => FaceControl { emotion: "sad", emotion_weight: 0.55, mouth: "aa", mouth_weight: 0.65, ..base },
        Mood::Joy => FaceControl { emotion: "happy", emotion_weight: 1.0, mouth: "aa", mouth_weight: 0.45, ..base },
        Mood::Sad => FaceControl { emotion: "sad", emotion_weight: 0.8, ..base },
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Gesture { Banzai, Greet, Rest }

pub struct RunnerView {
    pub holder: NodeId,
    pub lean: NodeId,
    pub actor: NodeId,
    /// 魚・イカの体の入れ物（lean の下。宙返りなどはここを回す）
    pub body: NodeId,
    pub creature: Option<Creature>,
    /// 魚・イカの宙返り（喜び・あいさつ・攻撃）の残り秒と長さ
    trick: f64, trick_len: f64,
    pub shadow: NodeId,
    pub tag: NodeId,
    pub tag_mat: MatId,
    pub tag_seq: u32,
    scale: f64,
    lean_x: f64, lean_z: f64, yaw_now: f64, yaw_init: bool,
    // 入れ物の伸び縮み（着地の踏ん張り・ダッシュの踏み出し）。ばねで戻す
    squash: f64, squash_v: f64, was_air: bool, was_dash: bool,
    rate_now: f64,
    prev: HeroPrev,
    mood: Option<Mood>,
}

impl RunnerView {
    pub fn new(kit: &mut Kit, scale: f64) -> RunnerView {
        let holder = kit.scene.group(None);
        let lean = kit.scene.group(Some(holder));
        let actor = kit.scene.actor(Some(lean));
        let body = kit.scene.group(Some(lean));
        let (geo, mat) = match SHARED_GEO.with(|c| *c.borrow()) {
            Some(x) => x,
            None => {
                let g = kit.geometry(&plane(1.6, 1.6).rotate_x(-std::f32::consts::FRAC_PI_2).owned());
                let m = blob_shadow_material(kit, 0.72);
                SHARED_GEO.with(|c| *c.borrow_mut() = Some((g, m)));
                (g, m)
            }
        };
        let shadow = kit.scene.mesh(None, geo, mat);
        kit.scene[shadow].render_order = 1;
        let map = gate_texture(kit, 1);
        let tag_mat = kit.material(Material { kind: MatKind::Sprite { rotation: 0.0, size_attenuation: false }, depth_write: false, fog: false, ..Material::sprite().with_map(map) });
        let tag = kit.scene.sprite(None, tag_mat);
        kit.scene[tag].render_order = 5;
        kit.scene[tag].set_scale(0.03);
        kit.scene[holder].set_scale(scale as f32);
        RunnerView {
            holder, lean, actor, body, creature: None, trick: 0.0, trick_len: 1.0, shadow, tag, tag_mat, tag_seq: 0, scale,
            lean_x: 0.0, lean_z: 0.0, yaw_now: 0.0, yaw_init: false, squash: 0.0, squash_v: 0.0, was_air: false, was_dash: false, rate_now: 1.0, prev: HeroPrev::default(), mood: None,
        }
    }

    pub fn loaded(&self, kit: &Kit) -> bool { self.creature.is_some() || kit.has_actor(self.actor) }

    /// 魚の体を置く（parent の空間 = 場の大きさ。全長 def.length、腹が地面から少し浮く高さ）
    pub fn set_fish(&mut self, kit: &mut Kit, def: &FishDef, shape: &std::sync::Arc<super::fish::FishShape>) {
        let b = FishBody::new(kit, self.body, shape, def.length);
        kit.scene[self.body].position.y = (b.height() as f64 * 0.5 + FISH_HOVER) as f32;
        self.creature = Some(Creature::Fish(b));
    }

    /// イカの体を置く（全長 def.length）
    pub fn set_squid(&mut self, kit: &mut Kit, def: &SquidDef, data: &std::sync::Arc<super::squid::SquidData>) {
        let b = SquidBody::new(kit, self.body, data, def.length);
        kit.scene[self.body].position.y = (b.height() as f64 * 0.5 + FISH_HOVER) as f32;
        self.creature = Some(Creature::Squid(b));
    }

    /// 縁取り（自機・撮影用）
    pub fn set_outline(&self, kit: &mut Kit, on: bool) {
        match &self.creature {
            Some(Creature::Fish(f)) => f.set_outline(kit, on),
            Some(Creature::Squid(q)) => q.set_outline(kit, on),
            None => { if on { kit.scene[self.actor].layers |= 2 } else { kit.scene[self.actor].layers &= !2 } }
        }
    }

    /// 魚・イカを動かす（走り・選ぶ画面・表彰の共通）
    fn drive_creature(&mut self, kit: &mut Kit, dt: f64, now: f64, d: Drive) {
        // 宙返り: 体の入れ物を縦に 1 回転（喜び・あいさつは大きく、攻撃は小さく前へ）
        if self.trick > 0.0 { self.trick = (self.trick - dt).max(0.0); }
        let k = 1.0 - self.trick / self.trick_len;
        let roll = if self.trick > 0.0 { (k * k * (3.0 - 2.0 * k) * std::f64::consts::TAU) as f32 } else { 0.0 };
        let bob = (now * 2.6).sin() * 0.06 * if d.air { 0.0 } else { 1.0 };
        let base = match &self.creature { Some(Creature::Fish(f)) => f.height() as f64 * 0.5 + FISH_HOVER, Some(Creature::Squid(q)) => q.height() as f64 * 0.5 + FISH_HOVER, None => 0.0 };
        kit.scene[self.body].position.y = (base + bob) as f32;
        kit.scene[self.body].rotation = Rot::euler(-roll, 0.0, 0.0);
        match &mut self.creature {
            Some(Creature::Fish(f)) => f.update(kit, dt, now, &d),
            Some(Creature::Squid(q)) => q.update(kit, dt, now, &SquidDrive { speed: d.speed, dash: d.dash, air: d.air, hurt: d.hurt, joy: d.joy }),
            None => {}
        }
    }

    fn start_trick(&mut self, seconds: f64) { self.trick = seconds; self.trick_len = seconds; }

    /// 輪郭の光（hero.ts の RIM。pengo の主役の材質の改造を移植。keiba の式は MUSO）。kit.set_actor のあとで呼ぶ
    pub fn apply_look(&self, kit: &mut Kit) {
        if !kit.has_actor(self.actor) { return; }
        let rim = Color::css("#ffd9b0").to_array();
        let (hidden, hidden_opacity) = (kit.hidden_color.to_array(), kit.hidden_opacity);
        kit.set_actor_look(self.actor, ActorLook { rim, rim_strength: 0.32, aura: [0.0; 3], aura_strength: 0.0, top: 0.08, lift: 0.1, flash: 0.0, hidden, hidden_opacity });
    }

    /// 札（枠の番号。顔の絵は app が読み終わってから貼る）
    pub fn set_gate(&mut self, kit: &mut Kit, g: usize, big: bool) {
        let t = gate_texture(kit, g);
        kit.mat_mut(self.tag_mat).map = Some(t);
        kit.scene[self.tag].set_scale(if big { 0.042 } else { 0.036 });
    }

    /// カメラと自分の間にいるとき透かす（0..1）
    pub fn set_fade(&self, kit: &mut Kit, k: f64) { if kit.has_actor(self.actor) { kit.set_actor_fade(self.actor, k as f32); } }

    pub fn reset(&mut self, kit: &mut Kit) {
        self.squash = 0.0; self.squash_v = 0.0; self.was_air = false; self.was_dash = false;
        kit.scene[self.lean].scale = Vec3::ONE;
        self.prev = HeroPrev::default();
        self.yaw_init = false;
        self.lean_x = 0.0;
        self.lean_z = 0.0;
        self.trick = 0.0;
        self.stop_all(kit);
    }

    /// 選ぶ画面・表彰: その場で動かす
    #[allow(clippy::too_many_arguments)]
    pub fn pose(&mut self, kit: &mut Kit, x: f64, y: f64, z: f64, yaw: f64, dt: f64, now: f64, speed: f64, selection: bool) {
        let p = Vec3::new(x as f32, y as f32, z as f32);
        kit.scene[self.holder].visible = true;
        kit.scene[self.holder].position = p;
        // 魚・イカは体が長いので、選ぶ画面・表彰では斜め前を向けて全身を見せる
        let turn = match &self.creature {
            // 通常の選択画面で、片側の目・えらぶた・口がいっしょに見える角度にする。
            Some(Creature::Fish(_)) if selection => -1.65,
            Some(_) => CREATURE_POSE_TURN,
            None => 0.0,
        };
        kit.scene[self.holder].rotation = Rot::y((yaw + turn) as f32);
        kit.scene[self.lean].rotation = Rot::euler(0.0, 0.0, 0.0);
        kit.scene[self.lean].scale = Vec3::ONE;
        kit.scene[self.shadow].position = Vec3::new(x as f32, (y + 0.03) as f32, z as f32);
        kit.scene[self.shadow].visible = true;
        kit.scene[self.tag].visible = false;
        if self.creature.is_some() {
            let joy = self.mood == Some(Mood::Joy);
            self.drive_creature(kit, dt, now, Drive { speed, joy, showcase: selection, ..Default::default() });
        } else if kit.has_actor(self.actor) {
            let world = kit.actor_world(self.actor);
            let a = kit.actor_mut(self.actor);
            a.set_parent_world(world);
            a.move_(speed / self.scale, true, 0.0, None);
            let _ = a.update(dt, now);
        }
    }

    pub fn gesture(&mut self, kit: &mut Kit, kind: Gesture) {
        if self.creature.is_some() { if kind != Gesture::Rest { self.start_trick(if kind == Gesture::Banzai { 0.9 } else { 0.7 }); } return; }
        if !kit.has_actor(self.actor) { return; }
        let a = kit.actor_mut(self.actor);
        let (m, k) = match kind { Gesture::Banzai => (MotionKind::Banzai, ActionKind::Banzai), Gesture::Greet => (MotionKind::Greet, ActionKind::Greet), Gesture::Rest => (MotionKind::Rest, ActionKind::Rest) };
        if a.query(m).available { a.start(k, StartOptions::default()); }
    }

    /// 武器を振る（攻撃の動き。いぶきの一撃は必殺技の動き。その動きを持たない主役は何もしない）
    pub fn attack(&mut self, kit: &mut Kit, charged: bool) {
        if self.creature.is_some() { self.start_trick(if charged { 0.6 } else { 0.4 }); return; }
        if !kit.has_actor(self.actor) { return; }
        let a = kit.actor_mut(self.actor);
        let (m, k) = if charged && a.query(MotionKind::Skill).available { (MotionKind::Skill, ActionKind::Skill) } else { (MotionKind::Attack, ActionKind::Attack) };
        if a.query(m).available { a.start(k, StartOptions { seconds: Some(if charged { 0.6 } else { 0.4 }), ..Default::default() }); }
    }

    pub fn has_current(&self, kit: &Kit) -> bool { if self.creature.is_some() { return self.trick > 0.0; } kit.has_actor(self.actor) && kit.actor(self.actor).current().is_some() }

    /// 顔: 余裕・必死・息切れ・喜び・悔しさ（企画 07）。合図だけ出す（作るのは部品）
    pub fn set_mood(&mut self, kit: &mut Kit, m: Mood) {
        if self.creature.is_some() { self.mood = Some(m); return; }
        if Some(m) == self.mood || !kit.has_actor(self.actor) { return; }
        self.mood = Some(m);
        kit.actor_mut(self.actor).express(Some(face(m)));
    }

    pub fn stop_all(&self, kit: &mut Kit) {
        if !kit.has_actor(self.actor) { return; }
        let a = kit.actor_mut(self.actor);
        if let Some(c) = a.current() { a.stop(c); }
    }

    pub fn hide(&self, kit: &mut Kit) {
        kit.scene[self.holder].visible = false;
        kit.scene[self.shadow].visible = false;
        kit.scene[self.tag].visible = false;
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(&mut self, kit: &mut Kit, r: &Runner, track: &Track, dt: f64, now: f64, show: bool, tag: bool) {
        let p = track.pos(r.s, r.d);
        kit.scene[self.holder].visible = show;
        kit.scene[self.holder].position = Vec3::new(p.x as f32, (p.y + r.y) as f32, p.z as f32);
        let yaw = track.yaw(r.s) + atan2(r.vd * 0.55, (r.speed).max(4.0));
        if !self.yaw_init { self.yaw_now = yaw; self.yaw_init = true; }
        let mut dy = yaw - self.yaw_now;
        dy = atan2(sin(dy), cos(dy));
        self.yaw_now += dy * (dt * 14.0).min(1.0);
        kit.scene[self.holder].rotation = Rot::y(self.yaw_now as f32);
        // 傾き（入れ物ごと）: 走りの前傾はダッシュで強く（企画 04）、カーブでは内へ少し倒す、横移動でも少し
        let moving = r.speed > 1.0;
        let t_lean = if moving && !r.air { 0.05 + r.dash_k * 0.2 + if r.exhausted > 0.0 { -0.03 } else { 0.0 } } else { 0.0 };
        let t_side = if moving { -track.curv(r.s) * r.speed * 0.42 + r.vd * 0.018 } else { 0.0 };
        self.lean_x += (t_lean - self.lean_x) * (dt * 10.0).min(1.0);
        self.lean_z += (t_side - self.lean_z) * (dt * 8.0).min(1.0);
        kit.scene[self.lean].rotation = Rot::euler(self.lean_x as f32, 0.0, self.lean_z as f32);
        if self.was_air && !r.air { self.squash_v -= 3.4; }
        if !self.was_dash && r.dashing { self.squash_v += 1.6; }
        if r.stumble > 0.0 && self.squash > -0.02 && self.squash_v > -0.2 { self.squash_v -= 0.6; }
        self.was_air = r.air;
        self.was_dash = r.dashing;
        self.squash_v += (-self.squash * 170.0 - self.squash_v * 13.0) * dt;
        self.squash += self.squash_v * dt;
        let sq = self.squash.clamp(-0.28, 0.18) as f32;
        kit.scene[self.lean].scale = Vec3::new(1.0 - sq * 0.5, 1.0 + sq, 1.0 - sq * 0.5);
        // 息切れは呼吸の上下で見せる（色だけに頼らない。企画 08）。three は描くときに世界の行列を作るので、部品へ渡す前に置く
        let breath = if r.exhausted > 0.0 { 1.0 } else if r.stamina < TUNE.low_at { 0.6 } else { 0.0 };
        kit.scene[self.lean].position.y = if breath > 0.0 && !r.finished { ((now * 14.0).sin() * 0.022 * breath) as f32 } else { 0.0 };
        kit.scene[self.lean].position.z = 0.0;
        // ハンマーで飛ばされている間: 空中で縦に 1 回転する（体の真ん中を軸に）
        if r.launch > 0.0 && r.launch_len > 0.0 {
            let k = 1.0 - r.launch / r.launch_len;
            let th = (k * k * (3.0 - 2.0 * k) * std::f64::consts::TAU) as f32;
            let c = 0.6f32;
            kit.scene[self.lean].rotation = Rot::euler(-th, 0.0, 0.0);
            kit.scene[self.lean].position.y = c - c * th.cos();
            kit.scene[self.lean].position.z = -c * th.sin();
        }
        if self.creature.is_some() {
            let _ = hero_commands(r, &mut self.prev);
            let joy = r.finished && r.place <= 3;
            let hurt = if r.launch > 0.0 { 1.0 } else if r.stumble > 0.0 { 0.7 } else { 0.0 };
            self.mood = Some(if joy { Mood::Joy } else { Mood::Calm });
            self.drive_creature(kit, dt, now, Drive { speed: r.speed, dash: r.dash_k, air: r.air, hurt, joy, showcase: false });
        } else if kit.has_actor(self.actor) {
            let cmds = hero_commands(r, &mut self.prev);
            let rate = hero_rate(r.speed, r.dash_k);
            self.rate_now += (rate - self.rate_now) * (dt * 8.0).min(1.0);
            let world = kit.actor_world(self.actor);
            let a = kit.actor_mut(self.actor);
            a.set_parent_world(world);
            for c in cmds {
                match c {
                    HeroCommand::Jump { air_seconds } => { a.start(ActionKind::Jump, StartOptions { air_seconds: Some(air_seconds), ..Default::default() }); }
                    HeroCommand::Hurt { seconds } => { if let Some(c) = a.current() { a.stop(c); } a.hit(Some(seconds), None); }
                    HeroCommand::Banzai => { if a.query(MotionKind::Banzai).available { a.start(ActionKind::Banzai, StartOptions::default()); } }
                }
            }
            a.move_(r.speed / self.scale, !r.air, 0.0, Some(self.rate_now));
            let _ = a.update(dt, now);
            let m = if r.finished { if r.place <= 3 { Mood::Joy } else { Mood::Tired } }
                else if r.exhausted > 0.0 || r.stamina < TUNE.low_at { Mood::Tired }
                else if r.dash_k > 0.5 { Mood::Push } else { Mood::Calm };
            if Some(m) != self.mood { self.mood = Some(m); kit.actor_mut(self.actor).express(Some(face(m))); }
        }
        kit.scene[self.shadow].position = Vec3::new(p.x as f32, (p.y + 0.03) as f32, p.z as f32);
        let s = (1.15 / (1.0 + r.y * 0.5)) as f32;
        kit.scene[self.shadow].scale = Vec3::new(s, 1.0, s);
        kit.scene[self.shadow].visible = show;
        kit.scene[self.tag].visible = show && tag;
        kit.scene[self.tag].position = Vec3::new(p.x as f32, (p.y + r.y + HERO_HEIGHT + 0.55) as f32, p.z as f32);
    }
}
