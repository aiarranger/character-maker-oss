// 主役の見た目（src/hero/hero.ts の写し）: 主役は部品（hero/actor.rs。待機・歩き・走り・構え・振り・必殺技・回避・ジャンプ・のけぞりと ＞＜）。
// ゲームは合図（sim::hero_signals）と、技の体の回転・踏み込みの傾き・倒れる傾き（入れ物ごと）、輪郭の光・闘気の値、足もとの輪を持つ。描くのは部品（character-view の ActorView）。
// 右手に食べ物の武器を持たせ（付け方は部品）、刃の根元と先を毎フレーム出す（剣の軌跡用）。
use super::actor::{ActionKind, CharacterActor, Gait, MotionKind, StartOptions};
use character_actor::package::{self, CharacterAsset};
use crate::assets;
use crate::delivery::{DeliveryFile, HeroEntry};
use crate::geom;
use crate::gfx::{self, Gfx, Inst, MeshBuf, Pipe};
use crate::glb::Glb;
use crate::raster;
use crate::view::materials::ORDER_OPAQUE;
use character_view::{ActorLook, ActorTargets, ActorView};
use glam::{DMat4, DQuat, DVec3, Mat4, Quat, Vec3};
use serde_json::Value;
use sim::battle::{Path, Player, PlayerState, WIRE_AIM};
use sim::hero_signals::{HeroCommand, HeroPrev, HeroStartKind, hero_commands, hero_gait, hero_move};
use sim::moves::spin_at;
use std::sync::Arc;

/// 選ぶ画面に出す 1 人（hero.ts の HeroInfo）
#[derive(Clone)]
pub struct HeroInfo {
    pub id: String,
    pub name: String,
    pub thumb: String,
    pub thumb_file: Option<DeliveryFile>,
    pub select: String,
    pub select_file: Option<DeliveryFile>,
    pub portrait: bool,
    pub height: f32,
    pub manifest: Arc<Value>,
    pub tint: String,
    pub has_face: bool,
    /// Studio で作ったキャラ（配布ではなく .character.zip から読む）。配布の主役は None
    pub package: Option<Arc<CharacterAsset>>,
}

// 主役は配布（public/delivery）にいる全員が出る。今はきゃしー 1 体。表に無い主役は、並びは後ろ・色は金・武器はケーキの剣になる
/// 選ぶ画面に並べる順
pub const ORDER: [&str; 1] = ["cassie"];
/// 主役ごとの色（闘気・足もとの輪）
pub const TINTS: [(&str, &str); 1] = [("cassie", "#ff8f6a")];
/// 主役ごとの既定の武器（選ぶ画面で替えられる）
pub const DEFAULT_WEAPON: [(&str, &str); 1] = [("cassie", "gyoza_mace")];
pub fn default_weapon(id: &str) -> Option<&'static str> { DEFAULT_WEAPON.iter().find(|(k, _)| *k == id).map(|(_, v)| *v) }

/// 武器: 見た目・長さと、強攻撃の弾道（試作で 3 本だけ）
#[derive(Clone, Copy)]
pub struct WeaponDef { pub id: &'static str, pub name: &'static str, pub length: f32, pub path: Path }
pub const WEAPONS: [WeaponDef; 12] = [
    WeaponDef { id: "cake_sword", name: "ケーキの剣", length: 1.56, path: Path::None },
    WeaponDef { id: "baguette_sword", name: "バゲットの剣", length: 1.69, path: Path::None },
    WeaponDef { id: "saba_sword", name: "サバの剣", length: 1.62, path: Path::None },
    WeaponDef { id: "taiyaki_sword", name: "たい焼きの剣", length: 1.56, path: Path::None },
    WeaponDef { id: "ebi_lance", name: "エビフライの槍", length: 1.94, path: Path::Straight },
    WeaponDef { id: "ika_spear", name: "イカの槍", length: 1.94, path: Path::None },
    WeaponDef { id: "candy_axe", name: "キャンディの斧", length: 1.5, path: Path::None },
    WeaponDef { id: "daikon_club", name: "大根のこん棒", length: 1.5, path: Path::None },
    WeaponDef { id: "gyoza_mace", name: "餃子のメイス", length: 1.44, path: Path::None },
    WeaponDef { id: "momo_hammer", name: "鶏モモのハンマー", length: 1.5, path: Path::None },
    WeaponDef { id: "softserve_staff", name: "ソフトクリームの杖", length: 1.88, path: Path::Curve },
    WeaponDef { id: "tamago_pan", name: "目玉焼きのフライパン", length: 1.31, path: Path::Spread },
];

/// 主役の背の高さ（m）。雑兵（1.2m）より頭ひとつ高く
pub const HERO_HEIGHT: f32 = 2.05;
/// 主役の拡大（部品の空間 → 場）。歩き・走りに切り替わる速さもこれで直して部品へ渡す
pub fn hero_scale(height: f32) -> f32 { HERO_HEIGHT / height.max(0.5) }
/// 歩き・走りに切り替わる速さ（部品の空間。場では 0.6・5）
pub fn gait_for(height: f32) -> Gait { let g = hero_gait(hero_scale(height) as f64); Gait { walk: g.walk, run: g.run } }

pub fn to_hero_list(heroes: Vec<HeroEntry>) -> Vec<HeroInfo> {
    let mut list: Vec<HeroInfo> = heroes.into_iter().map(|h| HeroInfo {
        tint: TINTS.iter().find(|(k, _)| *k == h.id).map(|(_, v)| *v).unwrap_or("#ffcf6a").to_string(),
        has_face: h.manifest.get("motion").and_then(|m| m.get("face")).is_some_and(|f| !f.is_null()),
        id: h.id, name: h.name, thumb: h.thumb, thumb_file: h.thumb_file, select: h.select, select_file: h.select_file, portrait: h.portrait, height: h.height, manifest: h.manifest, package: None,
    }).collect();
    let rank = |id: &str| ORDER.iter().position(|o| *o == id).unwrap_or(ORDER.len());
    list.sort_by_key(|h| rank(&h.id));
    list
}

// ---- Studio で作ったキャラ（docs/gyogun-spec.md の「Studio で作ったキャラで遊ぶ」） ----

/// つくったキャラの id（保存のキー gyogun.hero・gyogun.weapon.<id>・gyogun.honors.<id> に使う。送り直しても同じ）
pub const GUEST_ID: &str = "studio";

/// Studio が送った .character.zip を読んで、選ぶ画面の 1 人にする。武器は ZIP の weapon（none・知らない id は None ＝ ゲームの既定）
pub fn guest_hero(zip: &[u8]) -> Result<(HeroInfo, Option<&'static str>), String> {
    let asset = package::read_character_file(zip, "studio.character.zip")?;
    let m = &asset.manifest;
    let weapon = m["weapon"].as_str().and_then(|w| WEAPONS.iter().find(|x| x.id == w)).map(|x| x.id);
    let info = HeroInfo {
        id: GUEST_ID.to_string(), name: m["name"].as_str().unwrap_or("").to_string(),
        thumb: String::new(), thumb_file: None, select: String::new(), select_file: None, portrait: false,
        height: m["coordinates"]["heightMeters"].as_f64().unwrap_or(1.0) as f32,
        manifest: Arc::new(m.clone()), tint: "#ffcf6a".to_string(), has_face: false, package: Some(Arc::new(asset)),
    };
    Ok((info, weapon))
}

/// つくったキャラを主役の部品にする（配布の主役の Catalog::hero の代わり）。GPU は触らない
pub fn guest_actor(asset: &CharacterAsset, gait: Gait) -> Result<CharacterActor, String> {
    let mut actor = package::assemble_package(asset, None)?;
    actor.set_gait(gait);
    Ok(actor)
}

/// 回避の間に体を回す角度: 部品の転がる向き（本人から見た向き）。その場で転がる動き（進みが 0.2 m 未満。Studio の書き出しの dodge）は向きを持たないので回さない
fn roll_turn(travel: Option<[f32; 3]>) -> Option<f64> {
    travel.filter(|r| r[0].hypot(r[2]) > 0.2).map(|r| (r[0] as f64).atan2(r[2] as f64))
}

#[derive(Clone, Copy, Default)]
pub struct HeroOpts {
    pub flash: f64, pub aura: f64, pub show: bool, pub ring: bool, pub freeze_anim: bool,
    /// 育つ武器: 武器の大きさ（1 = 元の大きさ。0 以下は 1 として扱う）
    pub grow: f64,
}

#[derive(Clone, Copy)]
pub struct Blade { pub tip: Vec3, pub base: Vec3, pub prev: Vec3, pub ok: bool }

pub struct HeroView {
    view: ActorView,
    p_blob: wgpu::RenderPipeline,
    p_ring: wgpu::RenderPipeline,
    empty_bg: wgpu::BindGroup,
    white_bg: wgpu::BindGroup,
    blob_bg: wgpu::BindGroup,
    shadow_mesh: MeshBuf,
    ring_mesh: MeshBuf,
    fx_buf: wgpu::Buffer,

    inst: Option<CharacterActor>,
    info: Option<HeroInfo>,
    weapon_id: String,
    scale: f32,
    // 向き・傾き・闘気は three と同じく倍精度（ちょうど 180 度の振り向きの回る向きを f32 の丸めで変えない）
    vis_yaw: f64,
    spin_off: f64,
    lean_x: f64,
    lean_z: f64,
    aura_i: f64,
    flash: f64,
    prev: HeroPrev,
    /// 技の時計（moveT）の前のコマの値。技の間は部品をこの時計で進める（ヒットストップで止まる時刻を遊びとそろえる）
    last_move_t: f64,
    last_key: i64,
    last_dur: f64,
    tip_w: Vec3,
    base_w: Vec3,
    prev_tip: Vec3,
    show: bool,
    ring_on: bool,
    /// 位置（holder の世界）
    pos: Vec3,
    /// 育つ武器: 武器の元の長さ・なめらかに追う大きさ・部品へ渡してある大きさ・格が上がってからの秒（一度ふくらむ）
    weapon_len: f32,
    grow: f64,
    held: f64,
    pop_t: f64,
}

fn tex_bind(gfx: &Gfx, view: &wgpu::TextureView, smp: &wgpu::Sampler) -> wgpu::BindGroup {
    gfx.device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &gfx.tex_layout, entries: &[
        wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
        wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(smp) },
    ] })
}

impl HeroView {
    pub fn new(gfx: &Gfx) -> HeroView {
        let device = &gfx.device;
        let view = ActorView::new(device, &gfx.queue, &gfx.globals_layout, ActorTargets { color: gfx::HDR, depth: gfx::DEPTH, mask: gfx::MASK, samples: gfx::SAMPLES });
        let shader = gfx.shader("hero fx", include_str!("../../shaders/hero_fx.wgsl"));
        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: Some("empty"), entries: &[] });
        let empty_bg = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &empty_layout, entries: &[] });
        let fx_layouts: [&wgpu::BindGroupLayout; 3] = [&gfx.globals_layout, &empty_layout, &gfx.tex_layout];
        let fx_vbufs = [gfx::vert_layout(), gfx::inst_layout()];
        let p_blob = with_bias(device, "hero blob", &shader, "vs_fx", "fs_blob", &fx_layouts, &fx_vbufs, gfx::BLEND_MULTIPLY, false, wgpu::DepthBiasState { constant: 0, slope_scale: -2.0, clamp: 0.0 });
        // geom::ring の面の向きは three の RingGeometry と逆なので両面で描く（上から見える面は同じ）
        let p_ring = Pipe::scene("hero ring", &shader, "vs_fx", "fs_ring", &fx_layouts, &fx_vbufs).transparent(gfx::BLEND_ADD).double().build(device);

        let smp_repeat = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("hero"), address_mode_u: wgpu::AddressMode::Repeat, address_mode_v: wgpu::AddressMode::Repeat, mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::MipmapFilterMode::Linear, ..Default::default() });
        let smp_clamp = device.create_sampler(&wgpu::SamplerDescriptor { label: Some("hero board"), mag_filter: wgpu::FilterMode::Linear, min_filter: wgpu::FilterMode::Linear, mipmap_filter: wgpu::MipmapFilterMode::Linear, ..Default::default() });
        let white_bg = tex_bind(gfx, &gfx.white, &smp_repeat);
        let (bw, bh, bp) = raster::blob_rgba();
        let blob_bg = tex_bind(gfx, &gfx.texture(bw, bh, &bp, false), &smp_clamp);
        let shadow_mesh = gfx.mesh(&geom::plane(1.6, 1.6, 1, 1).rotate_x(-std::f32::consts::FRAC_PI_2));
        let ring_mesh = gfx.mesh(&geom::ring(0.85, 1.0, 48).rotate_x(-std::f32::consts::FRAC_PI_2));
        let fx_buf = gfx.vbuf(2 * std::mem::size_of::<Inst>() as u64);
        HeroView {
            view, p_blob, p_ring, empty_bg, white_bg, blob_bg, shadow_mesh, ring_mesh, fx_buf,
            inst: None, info: None, weapon_id: String::new(), scale: 1.0, vis_yaw: 0.0, spin_off: 0.0, lean_x: 0.0, lean_z: 0.0, aura_i: 0.0, flash: 0.0,
            prev: HeroPrev { move_key: -1, state: PlayerState::Free, air: false }, last_move_t: 0.0, last_key: -1, last_dur: 0.0,
            tip_w: Vec3::ZERO, base_w: Vec3::ZERO, prev_tip: Vec3::ZERO, show: false, ring_on: false, pos: Vec3::ZERO,
            weapon_len: 0.0, grow: 1.0, held: 1.0, pop_t: 9.0,
        }
    }

    pub fn info(&self) -> Option<&HeroInfo> { self.info.as_ref() }
    pub fn loaded(&self) -> bool { self.inst.is_some() }
    /// 確かめ用: 主役の部品（いまの動作・使える動き）
    pub fn actor(&self) -> Option<&CharacterActor> { self.inst.as_ref() }
    pub fn actor_mut(&mut self) -> Option<&mut CharacterActor> { self.inst.as_mut() }
    pub fn weapon_id(&self) -> &str { &self.weapon_id }
    pub fn yaw(&self) -> f64 { self.vis_yaw }
    pub fn lean(&self) -> f64 { self.lean_x }
    pub fn aura(&self) -> f64 { self.aura_i }
    /// 育つ武器: いま部品へ渡してある武器の大きさ
    pub fn grow(&self) -> f64 { self.held }
    /// 格が上がった: 武器を一度ふくらませる
    pub fn pop(&mut self) { self.pop_t = 0.0; }
    /// 確かめ用: 向きと傾きをそろえる
    pub fn dbg_align(&mut self, yaw: f64) { self.vis_yaw = yaw; self.lean_x = 0.0; self.lean_z = 0.0; self.aura_i = 0.0; }

    /// 部品で読んだ主役に替える（前の主役は片付ける）
    pub fn set_hero(&mut self, gfx: &Gfx, hi: &HeroInfo, mut actor: CharacterActor, weapon_id: &str) {
        self.prev = HeroPrev { move_key: -1, state: PlayerState::Free, air: false };
        self.scale = hero_scale(hi.height);
        self.view.set_actor(&gfx.device, &gfx.queue, &mut actor);
        self.inst = Some(actor);
        self.info = Some(hi.clone());
        self.set_weapon(gfx, weapon_id);
    }

    /// 武器を替える。長さは主役の背に対して決める（主役の拡大 scale で割る）。持つ位置と配布の武器の大きさは部品
    pub fn set_weapon(&mut self, gfx: &Gfx, id: &str) {
        self.view.clear_weapon();
        self.weapon_id = id.to_string();
        let Some(actor) = self.inst.as_mut() else { return };
        let def = WEAPONS.iter().find(|w| w.id == id).unwrap_or(&WEAPONS[0]);
        self.weapon_id = def.id.to_string();
        let g: Arc<Glb> = match assets::load_glb(&format!("{}models/wilds/weapons/{}.glb", assets::BASE, def.id)) {
            Ok(g) => g,
            Err(e) => { eprintln!("{e}"); actor.hold_weapon(None); return; }
        };
        self.weapon_len = def.length;
        if !actor.hold_weapon(Some(def.length * self.held as f32 / self.scale)) { return; }
        self.view.set_weapon(&gfx.device, &gfx.queue, &g);
    }

    /// 刃の先と根元（世界）。武器が無ければ手のあたり
    pub fn blade(&self) -> Blade { Blade { tip: self.tip_w, base: self.base_w, prev: self.prev_tip, ok: self.view.has_weapon() } }

    pub fn update(&mut self, gfx: &Gfx, p: &Player, dt: f64, now: f64, opts: HeroOpts) {
        self.show = opts.show;
        self.pos = Vec3::new(p.x as f32, p.y as f32, p.z as f32);
        let m = p.r#move;
        // 部品への合図（技・回避・ジャンプ・被弾・中断）。当たり・無敵・割り込みは遊び（battle）が決めた結果をそのまま伝える
        // 技が終わって自由に戻ったときの「止める」は、部品を 1 コマ進めたあとに出す（同じコマで部品が自分で終われば、中断ではなく終わり）
        let cmds = if self.inst.is_some() { hero_commands(p, &mut self.prev) } else { Vec::new() };
        if let Some(inst) = self.inst.as_mut() {
            for c in &cmds {
                match *c {
                    HeroCommand::Start { kind, opts } => {
                        let k = if kind == HeroStartKind::Skill { ActionKind::Skill } else { ActionKind::Attack };
                        inst.start(k, StartOptions { seconds: Some(opts.seconds), from: opts.from, to: opts.to, ease_swing: opts.ease_swing, cycle: opts.cycle, ..Default::default() });
                    }
                    HeroCommand::Dodge { seconds } => { inst.start(ActionKind::Dodge, StartOptions { seconds: Some(seconds), ..Default::default() }); }
                    HeroCommand::Jump { air_seconds } => { inst.start(ActionKind::Jump, StartOptions { air_seconds: Some(air_seconds), ..Default::default() }); }
                    HeroCommand::Hurt { seconds } => {
                        if let Some(cur) = inst.current() { inst.stop(cur); }
                        inst.hit(seconds, None);
                    }
                    HeroCommand::Stop => {}
                }
            }
        }
        // 育つ武器: 武器の長さを部品へ渡し直す（持つ位置と配布の武器の大きさは部品）。格が上がった瞬間は一度ふくらんでから落ち着く
        let want = if opts.grow > 0.0 { opts.grow } else { 1.0 };
        self.grow += (want - self.grow) * (dt * 4.0).min(1.0);
        if (want - self.grow).abs() < 0.002 { self.grow = want; }
        self.pop_t += dt;
        let size = self.grow * if self.pop_t < 0.35 { 1.0 + 0.3 * (self.pop_t / 0.35 * std::f64::consts::PI).sin() } else { 1.0 };
        if (size - self.held).abs() > 0.008 && self.view.has_weapon() {
            if let Some(inst) = self.inst.as_mut() { inst.hold_weapon(Some(self.weapon_len * size as f32 / self.scale)); }
            self.held = size;
        }
        // 体の向き: 技の回転を足す。回避は、部品の転がる向き（本人から見た向き）が進む向きに合うように回す
        let mut target = p.face;
        let roll = self.inst.as_ref().and_then(|i| if i.current() == Some(ActionKind::Dodge) { roll_turn(i.query(MotionKind::Dodge).travel) } else { None });
        if let (Some(mv), PlayerState::Attack) = (m, p.state) {
            self.spin_off = spin_at(mv, p.move_t);
            target += self.spin_off;
            self.vis_yaw = target;
        } else if let Some(turn) = roll {
            self.vis_yaw = target - turn;
        } else {
            self.spin_off = 0.0;
            let tau = std::f64::consts::TAU;
            let mut d = (target - self.vis_yaw) % tau;
            if d > std::f64::consts::PI { d -= tau; }
            if d < -std::f64::consts::PI { d += tau; }
            self.vis_yaw += d * (dt * 16.0).min(1.0);
        }
        // 傾き（入れ物ごと）: 走りの前傾、技の踏み込み、倒れる。回避・のけぞりの体の動きは部品
        let moving = p.state == PlayerState::Free && p.speed > 0.6 && p.y < 0.05;
        let mut t_lean_x = if moving { 0.22f64.min(p.speed * 0.025) } else { 0.0 };
        if let (Some(mv), PlayerState::Attack) = (m, p.state) {
            let k = (p.move_t / mv.dur.max(0.05)).min(1.0);
            t_lean_x = 0.2 * (k * std::f64::consts::PI).sin();
        }
        // 飴糸: 糸を撃つ間は少しのけぞり、引かれて飛ぶ間は体ごと前へ倒れる（体の動きは部品の跳ぶ動き）
        if p.state == PlayerState::Wire { t_lean_x = if p.wire.flip { 0.0 } else if p.st < WIRE_AIM { -0.25 } else { 1.05 }; }
        // わたあめ雲: 進む速さのぶんだけ前へ傾く（雲の上で立って構える）
        if p.state == PlayerState::Ride { t_lean_x = 0.3f64.min(p.speed * 0.02); }
        if p.state == PlayerState::Dead { t_lean_x = -1.4; }
        self.lean_x += (t_lean_x - self.lean_x) * (dt * 14.0).min(1.0);
        // 飴糸の振り子: ふくらむ側へ体ごと傾ける（弧の内側へ倒れ込む）
        let t_lean_z = if p.state == PlayerState::Wire && p.wire.swing.abs() > 0.5 && p.st >= WIRE_AIM { -p.wire.swing.signum() * 0.55 } else { 0.0 };
        self.lean_z += (t_lean_z - self.lean_z) * (dt * 10.0).min(1.0);
        self.flash = opts.flash;
        self.aura_i += (opts.aura - self.aura_i) * (dt * 8.0).min(1.0);
        let holder = DMat4::from_scale_rotation_translation(DVec3::splat(self.scale as f64), DQuat::from_rotation_y(self.vis_yaw), DVec3::new(p.x, p.y, p.z));
        // 空へ逃げる飴糸（的の逆へ）: 飛ぶ間に、入れ物ごと後ろへ 1 回転する（体の真ん中あたりを軸に。体の動きは部品の跳ぶ動き）
        let flip = if p.state == PlayerState::Wire && p.wire.flip && p.st >= WIRE_AIM { -((p.st - WIRE_AIM) / p.wire.dur.max(0.05)).min(1.0) * std::f64::consts::TAU } else { 0.0 };
        let piv = DVec3::new(0.0, 0.8 / (self.scale as f64).max(0.01), 0.0);
        // 斬り込む飴糸: 届く前の後半で、進む向きを軸に体ごと回る（螺旋。三撃目は 2 回転）。体の動きは部品の跳ぶ動き
        let dive = p.state == PlayerState::Wire && !p.wire.esc && p.wire.target != 0 && p.st >= WIRE_AIM;
        let spin = if dive {
            let k = ((p.st - WIRE_AIM) / p.wire.dur.max(0.05)).min(1.0);
            let turns = if p.wire_chain == 2 && p.wire_chain_t > 0.0 { 2.0 } else { 1.0 };
            let u = ((k - 0.4) / 0.6).clamp(0.0, 1.0);
            -u * u * (3.0 - 2.0 * u) * turns * std::f64::consts::TAU * if p.wire.swing < 0.0 { -1.0 } else { 1.0 }
        } else { 0.0 };
        let lean = DMat4::from_translation(piv) * DMat4::from_rotation_x(flip) * DMat4::from_translation(-piv) * DMat4::from_quat(DQuat::from_rotation_x(self.lean_x) * DQuat::from_rotation_z(self.lean_z)) * DMat4::from_translation(piv) * DMat4::from_rotation_y(spin) * DMat4::from_translation(-piv);
        let scale = self.scale;
        if let Some(inst) = self.inst.as_mut() {
            inst.set_parent_world((holder * lean).as_mat4());
            let mv = hero_move(p, scale as f64);
            inst.move_(mv.speed, mv.grounded, mv.armed as f32, None);
            // ヒットストップの間は dt が 0（姿を止める）。技の間は技の時計の進みで（始めたコマは技の始まりからの時刻）
            let in_move = p.state == PlayerState::Attack && m.is_some();
            let move_t = p.move_t;
            // 技が終わりまで行って自由に戻ったコマ（ヒットストップ中でも）は、技の残り（2 コマ以内）まで進める。途中で終わった技（着地など）は止める
            let ended = !in_move && p.state == PlayerState::Free && self.last_key >= 0 && p.move_key == self.last_key && self.last_dur - self.last_move_t <= 1.0 / 30.0 + 1e-6;
            let step = if opts.freeze_anim { 0.0 }
                else if in_move { (if p.move_key == self.last_key { move_t - self.last_move_t } else { move_t }).max(0.0) }
                else if ended { dt.max(self.last_dur - self.last_move_t) }
                else { dt };
            self.last_move_t = move_t;
            self.last_key = if in_move { p.move_key } else { -1 };
            self.last_dur = m.map(|x| x.dur).unwrap_or(0.0);
            inst.update(step, now);
            if cmds.iter().any(|c| matches!(c, HeroCommand::Stop)) && let Some(cur) = inst.current() && (cur == ActionKind::Attack || cur == ActionKind::Skill) { inst.stop(cur); }
        }
        self.write_frame(gfx);
        // 刃の位置（ひとつ前のコマの先も残す: 振りの向きを見る）
        self.prev_tip = self.tip_w;
        match self.inst.as_ref().and_then(|i| if self.view.has_weapon() { i.grip_world() } else { None }) {
            Some(g) => {
                self.tip_w = g.transform_point3(Vec3::new(0.0, 1.0, 0.0));
                self.base_w = g.transform_point3(Vec3::new(0.0, 0.28, 0.0));
            }
            None => {
                let (s, c) = (self.vis_yaw as f32).sin_cos();
                self.tip_w = Vec3::new(self.pos.x + s * 1.6, self.pos.y + 1.2, self.pos.z + c * 1.6);
                self.base_w = Vec3::new(self.pos.x + s * 0.4, self.pos.y + 1.2, self.pos.z + c * 0.4);
            }
        }
        // 足もとの丸影と輪
        let s = 1.2 / (1.0 + p.y as f32 * 0.25);
        let shadow_m = Mat4::from_scale_rotation_translation(Vec3::new(s, 1.0, s), Quat::IDENTITY, Vec3::new(self.pos.x, 0.03, self.pos.z));
        // blobShadowMaterial(0.7): new Color(0.32, 0.26, 0.4).lerp(白, 0.3)
        let blob = [0.32 + (1.0 - 0.32) * 0.3, 0.26 + (1.0 - 0.26) * 0.3, 0.4 + (1.0 - 0.4) * 0.3, 1.0];
        self.ring_on = opts.show && opts.ring && p.state != PlayerState::Dead;
        let ring_m = Mat4::from_translation(Vec3::new(self.pos.x, 0.04, self.pos.z));
        let opacity = if p.invuln > 0.0 && p.state != PlayerState::Attack { (0.3 + 0.3 * (now * 14.0).sin().abs()) as f32 } else { 0.55 };
        let tint = gfx::hex(self.info.as_ref().map(|i| i.tint.as_str()).unwrap_or("#ffd76a"));
        let insts = [Inst::new(shadow_m, blob), Inst::new(ring_m, [tint[0], tint[1], tint[2], opacity])];
        gfx.queue.write_buffer(&self.fx_buf, 0, bytemuck::cast_slice(&insts));
    }

    /// 部品を直接動かしたあと、そのコマの値を GPU へ（確かめ用）
    pub fn force_frame(&mut self, gfx: &Gfx) { self.show = true; self.write_frame(gfx); }

    /// 骨の行列・モーフの重み・輪郭の光を GPU へ
    fn write_frame(&mut self, gfx: &Gfx) {
        let Some(inst) = self.inst.as_ref() else { return };
        let look = ActorLook {
            rim: gfx::hex("#ffd9b0"), rim_strength: 0.45,
            aura: gfx::hex("#ffcf6a"), aura_strength: self.aura_i as f32,
            top: 0.1, lift: 0.12, flash: self.flash as f32,
            hidden: gfx::hex("#f4d2b0"), hidden_opacity: 0.82,
        };
        self.view.write_frame(&gfx.queue, inst, &look);
    }

    /// 影の型（group 0 は呼ぶ側が設定済み）。＞＜ の板は影を落とさない
    pub fn shadow<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if !self.show { return; }
        if let Some(inst) = &self.inst { self.view.shadow(pass, inst); }
    }

    /// 不透明は ORDER_OPAQUE、半透明は order 0（＞＜ の板が先。three の renderOrder -1 と 0）、足もとの丸影は order 1、輪は order 2
    pub fn draw<'a>(&'a self, _gfx: &Gfx, pass: &mut wgpu::RenderPass<'a>, order: i32) {
        if !self.show { return; }
        let Some(inst) = &self.inst else { return };
        if order == ORDER_OPAQUE {
            self.view.draw_opaque(pass, inst);
        } else if order == 0 {
            self.view.draw_transparent(pass, inst);
        } else if order == 1 {
            pass.set_pipeline(&self.p_blob);
            pass.set_bind_group(1, &self.empty_bg, &[]);
            pass.set_bind_group(2, &self.blob_bg, &[]);
            pass.set_vertex_buffer(0, self.shadow_mesh.vb.slice(..));
            pass.set_vertex_buffer(1, self.fx_buf.slice(..));
            pass.set_index_buffer(self.shadow_mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.shadow_mesh.n, 0, 0..1);
        } else if order == 2 && self.ring_on {
            pass.set_pipeline(&self.p_ring);
            pass.set_bind_group(1, &self.empty_bg, &[]);
            pass.set_bind_group(2, &self.white_bg, &[]);
            pass.set_vertex_buffer(0, self.ring_mesh.vb.slice(..));
            pass.set_vertex_buffer(1, self.fx_buf.slice(..));
            pass.set_index_buffer(self.ring_mesh.ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.ring_mesh.n, 0, 1..2);
        }
    }

    /// 自機だけの型を白で描く（縁取り用。＞＜ の板は除く）。的は gfx.mask_targets
    pub fn draw_mask<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>) {
        if !self.show { return; }
        if let Some(inst) = &self.inst { self.view.draw_mask(pass, inst); }
    }

    /// 物陰の影絵: 段やクッキーに隠れた部分だけを、うすいクリーム色で描く（renderer.ts の HiddenPlayerPass）。主の的と深度に描く
    pub fn draw_hidden<'a>(&'a self, _gfx: &Gfx, pass: &mut wgpu::RenderPass<'a>) {
        if !self.show { return; }
        if let Some(inst) = &self.inst { self.view.draw_hidden(pass, inst); }
    }
}

/// Pipe と同じ作り方で、深度の offset（polygonOffset）を両方渡すもの
#[allow(clippy::too_many_arguments)]
fn with_bias(device: &wgpu::Device, label: &str, shader: &wgpu::ShaderModule, vs: &str, fs: &str, layouts: &[&wgpu::BindGroupLayout], vbufs: &[wgpu::VertexBufferLayout<'_>], blend: wgpu::BlendState, depth_write: bool, bias: wgpu::DepthBiasState) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some(label), bind_group_layouts: &layouts.iter().map(|l| Some(*l)).collect::<Vec<_>>(), immediate_size: 0 });
    let vb: Vec<Option<wgpu::VertexBufferLayout<'_>>> = vbufs.iter().map(|b| Some(b.clone())).collect();
    let targets = [Some(wgpu::ColorTargetState { format: gfx::HDR, blend: Some(blend), write_mask: wgpu::ColorWrites::ALL })];
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label), layout: Some(&layout),
        vertex: wgpu::VertexState { module: shader, entry_point: Some(vs), buffers: &vb, compilation_options: Default::default() },
        fragment: Some(wgpu::FragmentState { module: shader, entry_point: Some(fs), targets: &targets, compilation_options: Default::default() }),
        primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, cull_mode: if depth_write { None } else { Some(wgpu::Face::Back) }, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState { format: gfx::DEPTH, depth_write_enabled: Some(depth_write), depth_compare: Some(wgpu::CompareFunction::Less), stencil: Default::default(), bias }),
        multisample: wgpu::MultisampleState { count: gfx::SAMPLES, ..Default::default() },
        multiview_mask: None,
        cache: None,
    })
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// Studio の「魚群繚乱で遊ぶ」が作る ZIP（lib/character-maker/ui-gyogun-handoff.e2e.mjs が書き出す。種類ごとに 1 体を tests/fixtures/studio/ に置く。games/gyogun/README.md）。
    /// 環境変数 GYOGUN_STUDIO_ZIPS で別のフォルダ（E2E の出力先の 16 体など）を読める
    fn studio_zips() -> Vec<(String, Vec<u8>)> {
        let dir = std::env::var("GYOGUN_STUDIO_ZIPS").map(std::path::PathBuf::from).unwrap_or_else(|_| std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/studio")));
        let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir).into_iter().flatten().flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".character.zip"))
            .map(|e| (e.file_name().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap())).collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// ＞＜ が出ているか: 板のある体（オリジナル・アメコミ）は maker-hurt-face の大きさ、モーフの体（ファンタジー）はまばたき以外のモーフの重みの変わり
    fn hurt_shown(a: &CharacterActor, neutral: &[Vec<f32>]) -> (f32, f32) {
        let node = a.skel.find("maker-hurt-face").expect("＞＜ の節点が無い");
        let plate = if a.skel.children[node].is_empty() { 0.0 } else { a.skel.s[node].x };
        let mut morph = 0f32;
        for (i, w) in a.skel.morphs.iter().enumerate() {
            let Some(mesh) = a.glb.nodes[i].mesh else { continue };
            for (k, v) in w.iter().enumerate() {
                if a.glb.meshes[mesh].target_names.get(k).is_some_and(|n| n.starts_with("blink")) { continue; }
                morph = morph.max((v - neutral[i][k]).abs());
            }
        }
        (plate, morph)
    }

    // 防ぐ故障: Studio から送ったキャラが読めない・主役に組み立てられない。構え・振り・ジャンプ・回避・のけぞりの動きが無くて、立ったまま滑る・斬っても振らない。
    // のけぞっても ＞＜ が出ない・出たまま戻らない
    #[test]
    fn つくったキャラは構える振る跳ぶ避けるのけぞるが使える() {
        let zips = studio_zips();
        if zips.is_empty() { eprintln!("tests/fixtures/studio に ZIP が無いので確かめていません（作り方は games/gyogun/README.md）"); return; }
        for (file, bytes) in zips {
            let (info, weapon) = guest_hero(&bytes).unwrap_or_else(|e| panic!("{file}: 読めない: {e}"));
            assert_eq!(info.id, GUEST_ID);
            assert!(!info.name.is_empty() && info.height > 0.2, "{file}: 名前・背が無い");
            let asset = info.package.clone().unwrap();
            assert!(asset.thumbnail.is_some(), "{file}: 選ぶ画面の絵が無い");
            assert!(weapon.is_none() || WEAPONS.iter().any(|w| Some(w.id) == weapon));
            let gait = gait_for(info.height);
            let mut a = guest_actor(&asset, gait).unwrap_or_else(|e| panic!("{file}: 組み立てられない: {e}"));
            // 読んだときの顔（Studio で選んだ表情）
            let neutral: Vec<Vec<f32>> = a.skel.morphs.clone();
            for kind in [MotionKind::Idle, MotionKind::Walk, MotionKind::Run, MotionKind::Ready, MotionKind::Attack, MotionKind::Skill, MotionKind::Jump, MotionKind::Dodge, MotionKind::Hit] {
                assert!(a.query(kind).available, "{file}: {kind:?} が使えない");
            }
            let (jump, dodge) = (a.query(MotionKind::Jump).clone(), a.query(MotionKind::Dodge).clone());
            eprintln!("{file}: {} 背 {} 武器 {weapon:?} / 跳ぶ {:?}秒 高さ {:?} / 避ける {:?}秒 進み {:?} / のけぞり {:?}秒", info.name, info.height, jump.seconds, jump.height, dodge.seconds, dodge.travel, a.query(MotionKind::Hit).seconds);
            // その場で転がる回避は、進む向きを向いたまま（進みの測りの誤差で横や後ろを向かない）。きゃしーの横へ転がる回避（進み 1.15 m）は今までどおり回す
            assert_eq!(roll_turn(dodge.travel), None, "{file}: 回避で体が回る");
            assert_eq!(roll_turn(Some([1.15, 0.0, 0.0])), Some(std::f64::consts::FRAC_PI_2));
            // 武器は右手に持てる（人の手か前足の骨がある）
            assert!(a.hold_weapon(Some(1.0)), "{file}: 武器を持てない");
            // 歩き・走りに切り替わる速さは、ゲームの値（主役の大きさで直した値）
            a.move_(gait.walk * 1.5, true, 0.0, None);
            for _ in 0..30 { a.update(1.0 / 60.0, 0.0); }
            // 止まって構える（ゲームと同じ。構えの間は戦う顔）。のけぞると ＞＜ が出て、終わると戻る
            a.move_(0.0, true, 0.85, None);
            for _ in 0..30 { a.update(1.0 / 60.0, 0.0); }
            let before = hurt_shown(&a, &neutral);
            assert!(before.0 < 0.01, "{file}: のけぞる前から ＞＜ の板が出ている");
            // 頭の高さ（部品の空間）。のけぞり・回避の間に、体が背の半分も浮かない（体の高さを Root が持つ待機に、Hip が持つ動作を重ねると 2 重になる）
            let head = a.skel.parent[a.skel.find("maker-hurt-face").unwrap()].unwrap();
            let head_y = |a: &CharacterActor| a.skel.world[head].w_axis.y;
            let stand = head_y(&a);
            a.move_(0.0, true, 0.0, None);
            a.hit(Some(0.6), None);
            for _ in 0..18 { a.update(1.0 / 60.0, 0.0); }
            assert!((head_y(&a) - stand).abs() < 0.25 * info.height, "{file}: のけぞりで体が浮く・沈む（頭 {} → {}）", stand, head_y(&a));
            let mid = hurt_shown(&a, &neutral);
            assert!(mid.0 > 0.9 || mid.1 > 0.5, "{file}: のけぞっても ＞＜ が出ない（板 {} モーフ {}）", mid.0, mid.1);
            // アメコミの ＞＜ の板は、ふつうの顔（開いた目）の形に合わせてある。のけぞりでまぶたを閉じたり、戦う顔の眉が残ったりすると、板の前へ出て ＞＜ が隠れる
            if file.starts_with("comic-") {
                let closed = a.skel.morphs.iter().enumerate().filter_map(|(i, w)| Some((a.glb.nodes[i].mesh?, w))).flat_map(|(m, w)| a.glb.meshes[m].target_names.iter().zip(w).filter(|(n, _)| *n == "blinkLeft" || *n == "blinkRight").map(|(_, v)| *v).collect::<Vec<_>>()).fold(0f32, f32::max);
                assert!(closed < 0.05 && mid.1 < 0.05, "{file}: ＞＜ の間に顔がふつうへ戻っていない（まぶた {closed} ほかのモーフ {}）", mid.1);
            }
            for _ in 0..90 { a.update(1.0 / 60.0, 0.0); }
            let after = hurt_shown(&a, &neutral);
            assert!(after.0 < 0.01 && after.1 < 0.01, "{file}: ＞＜ が出たまま戻らない（板 {} モーフ {}）", after.0, after.1);
            // 跳ぶ・避けるを始められる
            assert!(matches!(a.start(ActionKind::Jump, StartOptions { air_seconds: Some(0.8), ..Default::default() }), crate::hero::actor::StartResult::Ok { .. }));
            assert!(matches!(a.start(ActionKind::Dodge, StartOptions { seconds: Some(0.4), ..Default::default() }), crate::hero::actor::StartResult::Ok { .. }));
            for _ in 0..3 { a.update(1.0 / 60.0, 0.0); }
            assert!(head_y(&a) < stand + 0.25 * info.height, "{file}: 回避で体が浮く（頭 {} → {}）", stand, head_y(&a));
        }
    }
}
