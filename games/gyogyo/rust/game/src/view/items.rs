// 走路のマークと武器の見た目（docs/spec.md「マークと武器」）。絵は canvas 2D で描いた板。数は少ないので、近くの物だけを使い回しの板に出す。
// マークは丸い札（能力の 1 字）、武器は金の枠の菱形の札（武器の絵）。色だけに頼らず、形と字で分ける。
use super::materials::{SoftOptions, glow_texture, soft_material};
use character_actor::glb::Glb;
use glam::Vec3;
use island_kit::geometry::plane;
use island_kit::scene::Rot;
use island_kit::{Color, Kit, MatId, MatKind, Material, NodeId, TexId};
use sim::items::{ITEM, PickupKind, ShotKind, Stat, Weapon};
use sim::race::Race;
use std::f64::consts::PI;
use web_sys::OffscreenCanvasRenderingContext2d as G;

const PICKS: usize = 30;
/// 3D の物の入れ物の数（種類ごと）。走路に同じ種類が見えるのは多くて 4〜5 個
const MODEL_POOL: usize = 7;
/// 3D の物のファイル名（kind_index の順。public/models/items/<名前>.glb。Tripo で作った）
/// 空の名前はまだ 3D が無い（札の絵を出す）
pub const MODEL_FILES: [&str; 15] = ["mark_speed", "mark_stamina", "mark_guts", "mark_jump", "mark_turbo", "breath", "poison", "weapon_sword", "weapon_chain", "weapon_rose", "weapon_bottle", "weapon_crossbow", "weapon_hammer", "mystery", "jet"];
const SHOTS: usize = 10;
const TRAPS: usize = 8;
/// 見せる範囲（基準の走者の後ろ・前）
const BEHIND: f64 = 14.0;
const AHEAD: f64 = 175.0;

pub fn stat_color(s: Stat) -> &'static str { match s { Stat::Speed => "#ff6b3d", Stat::Stamina => "#35b86b", Stat::Guts => "#e9a400", Stat::Jump => "#3d94f0", Stat::Turbo => "#a763f2" } }

fn disc(g: &G, fill: &str, edge: &str, glyph: &str, ink: &str) {
    g.begin_path();
    let _ = g.arc(64.0, 64.0, 56.0, 0.0, 6.3);
    g.set_fill_style_str(fill);
    g.fill();
    g.set_line_width(9.0);
    g.set_stroke_style_str(edge);
    g.stroke();
    g.set_fill_style_str(ink);
    g.set_font("900 70px 'Zen Maru Gothic', 'Hiragino Maru Gothic ProN', sans-serif");
    g.set_text_align("center");
    g.set_text_baseline("middle");
    let _ = g.fill_text(glyph, 64.0, 69.0);
}

/// 武器の絵（128×128 の中。札にも、持っている手もとにも使う）
fn weapon_art(g: &G, w: Weapon) {
    g.set_line_cap("round");
    g.set_line_join("round");
    let line = |x0: f64, y0: f64, x1: f64, y1: f64, w: f64, c: &str| { g.begin_path(); g.move_to(x0, y0); g.line_to(x1, y1); g.set_line_width(w); g.set_stroke_style_str(c); g.stroke(); };
    let dot = |x: f64, y: f64, r: f64, c: &str| { g.begin_path(); let _ = g.arc(x, y, r, 0.0, 6.3); g.set_fill_style_str(c); g.fill(); };
    match w {
        Weapon::Sword => {
            line(46.0, 82.0, 98.0, 30.0, 17.0, "#3a3550");
            line(46.0, 82.0, 98.0, 30.0, 11.0, "#e9eef6");
            line(52.0, 79.0, 95.0, 36.0, 3.0, "#ffffff");
            line(34.0, 70.0, 58.0, 94.0, 11.0, "#3a3550");
            line(34.0, 70.0, 58.0, 94.0, 6.0, "#ffcf4d");
            line(42.0, 86.0, 30.0, 98.0, 12.0, "#3a3550");
            line(42.0, 86.0, 30.0, 98.0, 7.0, "#9a5a2c");
        }
        Weapon::Chain => {
            for k in 0..4 { let t = k as f64 / 3.0; dot(30.0 + t * 26.0, 100.0 - t * 26.0, 7.5, "#3a3550"); dot(30.0 + t * 26.0, 100.0 - t * 26.0, 4.0, "#c9ced8"); }
            g.set_fill_style_str("#3a3550");
            for k in 0..8 {
                let a = k as f64 / 8.0 * 2.0 * PI;
                g.begin_path();
                g.move_to(78.0 + (a - 0.22).cos() * 22.0, 50.0 + (a - 0.22).sin() * 22.0);
                g.line_to(78.0 + a.cos() * 38.0, 50.0 + a.sin() * 38.0);
                g.line_to(78.0 + (a + 0.22).cos() * 22.0, 50.0 + (a + 0.22).sin() * 22.0);
                g.fill();
            }
            dot(78.0, 50.0, 27.0, "#3a3550");
            dot(78.0, 50.0, 22.0, "#7d8698");
            dot(71.0, 43.0, 7.0, "#c9ced8");
        }
        Weapon::Rose => {
            line(40.0, 104.0, 70.0, 56.0, 9.0, "#1f5a35");
            line(40.0, 104.0, 70.0, 56.0, 5.0, "#3fae63");
            g.begin_path();
            let _ = g.ellipse(40.0, 78.0, 13.0, 6.0, -0.6, 0.0, 6.3);
            g.set_fill_style_str("#3fae63");
            g.fill();
            // 花びら: 外の 5 枚、内の 3 枚、まんなかの巻き
            for (n, ring, r, c, edge) in [(5, 17.0, 16.0, "#e0304f", "#7a1026"), (3, 8.0, 12.5, "#f2536c", "#a81a38")] {
                for pass in 0..2 {
                    for k in 0..n {
                        let a = k as f64 / n as f64 * 2.0 * PI - 0.6 + if n == 3 { 0.5 } else { 0.0 };
                        dot(76.0 + a.cos() * ring, 44.0 + a.sin() * ring, if pass == 0 { r + 3.5 } else { r }, if pass == 0 { edge } else { c });
                    }
                }
            }
            g.set_stroke_style_str("#7a1026");
            g.set_line_width(3.5);
            g.begin_path();
            for k in 0..=24 {
                let t = k as f64 / 24.0;
                let (a, r) = (t * 2.2 * PI, 1.5 + t * 8.5);
                let (x, y) = (76.0 + a.cos() * r, 44.0 + a.sin() * r);
                if k == 0 { g.move_to(x, y); } else { g.line_to(x, y); }
            }
            g.stroke();
        }
        Weapon::Bottle => {
            g.save();
            let _ = g.translate(64.0, 66.0);
            let _ = g.rotate(0.5);
            for (grow, c) in [(4.0, "#3a3550"), (0.0, "#2f9e5b")] {
                g.set_fill_style_str(c);
                g.begin_path();
                let _ = g.round_rect_with_f64(-17.0 - grow, -14.0 - grow, 34.0 + grow * 2.0, 58.0 + grow * 2.0, 10.0);
                g.fill();
                g.begin_path();
                let _ = g.round_rect_with_f64(-7.0 - grow, -46.0 - grow, 14.0 + grow * 2.0, 38.0 + grow * 2.0, 5.0);
                g.fill();
            }
            g.set_fill_style_str("#fff6e4");
            g.fill_rect(-17.0, 2.0, 34.0, 20.0);
            g.set_fill_style_str("#b8861d");
            g.fill_rect(-8.0, -48.0, 16.0, 8.0);
            g.set_fill_style_str("rgba(255,255,255,.55)");
            g.fill_rect(-12.0, -10.0, 5.0, 10.0);
            g.restore();
        }
        Weapon::Crossbow => {
            // 弓（横の弧）と台（縦の棒）と、つがえた矢
            g.set_line_cap("round");
            for (w, c) in [(13.0, "#3a3550"), (8.0, "#9a5a2c")] { line(64.0, 104.0, 64.0, 36.0, w, c); }
            for (w, c) in [(12.0, "#3a3550"), (7.0, "#c9ced8")] {
                g.begin_path();
                let _ = g.arc(64.0, 92.0, 46.0, PI * 1.18, PI * 1.82);
                g.set_line_width(w);
                g.set_stroke_style_str(c);
                g.stroke();
            }
            line(26.0, 62.0, 102.0, 62.0, 2.5, "#fff6e4");
            for (w, c) in [(7.0, "#3a3550"), (4.0, "#ffcf4d")] { line(64.0, 96.0, 64.0, 18.0, w, c); }
            g.begin_path(); g.move_to(64.0, 10.0); g.line_to(55.0, 26.0); g.line_to(73.0, 26.0); g.close_path(); g.set_fill_style_str("#e9eef6"); g.fill();
        }
        Weapon::Hammer => {
            // 太い柄と、大きな頭（お菓子の色）
            for (w, c) in [(14.0, "#3a3550"), (9.0, "#9a5a2c")] { line(40.0, 104.0, 72.0, 52.0, w, c); }
            g.save();
            let _ = g.translate(76.0, 44.0);
            let _ = g.rotate(0.55);
            for (grow, c) in [(4.0, "#3a3550"), (0.0, "#e2554f")] {
                g.set_fill_style_str(c);
                g.begin_path();
                let _ = g.round_rect_with_f64(-34.0 - grow, -18.0 - grow, 68.0 + grow * 2.0, 36.0 + grow * 2.0, 9.0);
                g.fill();
            }
            g.set_fill_style_str("#ffcf4d");
            g.fill_rect(-34.0, -18.0, 10.0, 36.0);
            g.fill_rect(24.0, -18.0, 10.0, 36.0);
            g.set_fill_style_str("rgba(255,255,255,.5)");
            g.fill_rect(-20.0, -13.0, 36.0, 6.0);
            g.restore();
        }
    }
}

/// 走路に落ちている札
fn pickup_art(g: &G, k: PickupKind) {
    match k {
        PickupKind::Mark(s) => disc(g, stat_color(s), "#fff6e4", s.glyph(), "#ffffff"),
        PickupKind::Breath => disc(g, "#e8fff6", "#35b86b", "息", "#0f6b45"),
        PickupKind::Poison => disc(g, "#3a2150", "#b7f04a", "毒", "#b7f04a"),
        PickupKind::Arm(w) => {
            // 金の枠の菱形（マークの丸と形で分ける）
            g.save();
            let _ = g.translate(64.0, 64.0);
            let _ = g.rotate(PI / 4.0);
            g.begin_path();
            let _ = g.round_rect_with_f64(-42.0, -42.0, 84.0, 84.0, 14.0);
            g.set_fill_style_str("#fff6e4");
            g.fill();
            g.set_line_width(9.0);
            g.set_stroke_style_str("#ffcf4d");
            g.stroke();
            g.set_line_width(3.0);
            g.set_stroke_style_str("#8a5a10");
            g.stroke();
            g.restore();
            g.save();
            let _ = g.translate(64.0, 64.0);
            let _ = g.scale(0.74, 0.74);
            let _ = g.translate(-64.0, -64.0);
            weapon_art(g, w);
            g.restore();
        }
        PickupKind::Mystery => {
            // 「？」の箱: 虹色の縁の四角に大きな「？」
            g.begin_path();
            let _ = g.round_rect_with_f64(12.0, 12.0, 104.0, 104.0, 20.0);
            g.set_fill_style_str("#ffd84a");
            g.fill();
            g.set_line_width(9.0);
            g.set_stroke_style_str("#ff6fa8");
            g.stroke();
            g.set_line_width(3.0);
            g.set_stroke_style_str("#7a3a10");
            g.stroke();
            g.set_fill_style_str("#ffffff");
            g.set_font("900 82px 'Zen Maru Gothic', 'Hiragino Maru Gothic ProN', sans-serif");
            g.set_text_align("center");
            g.set_text_baseline("middle");
            g.set_stroke_style_str("#7a3a10");
            g.set_line_width(8.0);
            let _ = g.stroke_text("？", 64.0, 68.0);
            let _ = g.fill_text("？", 64.0, 68.0);
        }
        PickupKind::Jet => disc(g, "#ff7a2f", "#fff6e4", "噴", "#ffffff"),
    }
}

fn kind_index(k: PickupKind) -> usize { match k { PickupKind::Mark(s) => s.i(), PickupKind::Breath => 5, PickupKind::Poison => 6, PickupKind::Arm(w) => 7 + w.i(), PickupKind::Mystery => 13, PickupKind::Jet => 14 } }
fn kinds() -> Vec<PickupKind> {
    let mut v: Vec<PickupKind> = Stat::ALL.iter().map(|s| PickupKind::Mark(*s)).collect();
    v.push(PickupKind::Breath);
    v.push(PickupKind::Poison);
    v.extend(Weapon::ALL.iter().map(|w| PickupKind::Arm(*w)));
    v.push(PickupKind::Mystery);
    v.push(PickupKind::Jet);
    v
}

pub struct ItemView {
    /// 落ちている札の絵（kind_index の順）と、持っている武器の絵
    pick_tex: Vec<TexId>,
    held_tex: [TexId; 6],
    wave_tex: TexId,
    picks: Vec<(NodeId, MatId)>,
    /// 走者ごと: 持っている武器の板・その材質・ためている間の光
    held: Vec<(NodeId, MatId, NodeId)>,
    shots: Vec<(NodeId, MatId)>,
    traps: Vec<NodeId>,
    /// 振った時刻（走者ごと。板を回す）
    swing_at: [f64; 8],
    /// 振った弧（走者ごと。床に寝かせた板を、前へ払うように回す）と、その武器・ためたか・振った向き
    slash: Vec<(NodeId, MatId)>,
    slash_of: [(Weapon, bool, f32); 8],
    arc_tex: TexId, ring_tex: TexId,
    /// 当たった所: 床に広がる輪と、正面を向く衝撃の星（使い回し）
    hits: Vec<(NodeId, MatId, NodeId, MatId)>,
    hit_at: Vec<(f64, usize, bool)>,
    /// 3D の物（読めた種類だけ）: 走路に落ちている物の入れ物と、持っている武器の入れ物（走者ごと）
    models: Vec<Option<Vec<NodeId>>>,
    held_models: Vec<Option<Vec<NodeId>>>,
}

/// GLB を 1 つ置く: 外の入れ物（位置・向き・大きさ）の中で、形の真ん中を原点へ寄せ、いちばん長い辺を 1 にそろえる
fn place_model(kit: &mut Kit, g: &Glb, mat: &mut dyn FnMut(&mut Kit) -> MatId) -> NodeId {
    let (lo, hi) = island_kit::glb::merged_geometry(g, &|_| true).map(|geo| geo.bounding_box()).unwrap_or((Vec3::splat(-0.5), Vec3::splat(0.5)));
    let size = (hi - lo).max_element().max(1e-3);
    let outer = kit.scene.group(None);
    let inner = kit.scene.group(Some(outer));
    kit.scene[inner].set_scale(1.0 / size);
    kit.scene[inner].position = -(lo + hi) * 0.5 / size;
    let m = mat(kit);
    island_kit::glb::instantiate(kit, Some(inner), g, &mut |_, _| m);
    kit.scene[outer].visible = false;
    outer
}

/// 振った弧の長さ（秒）と、当たった輪の長さ
const SLASH_T: f64 = 0.32;
const HIT_T: f64 = 0.42;
const HITS: usize = 6;

/// 弧: 下が前（板を寝かせると、絵の下が走る向きになる）。外は淡く、芯は白い
fn arc_art(g: &G) {
    // 2 つの円のずれで三日月を作る: 真ん中が太く、両端が細くとがる。外側ほど淡い
    let crescent = |g: &G, r_out: f64, r_in: f64, c: &str| {
        g.begin_path();
        let _ = g.arc(64.0, 14.0, r_out, PI * 0.12, PI * 0.88);
        let _ = g.arc_with_anticlockwise(64.0, 2.0, r_in, PI * 0.86, PI * 0.14, true);
        g.close_path();
        g.set_fill_style_str(c);
        g.fill();
    };
    crescent(g, 104.0, 98.0, "rgba(255,255,255,.28)");
    crescent(g, 99.0, 99.0, "rgba(255,255,255,.6)");
    crescent(g, 96.0, 100.0, "rgba(255,255,255,1)");
}
/// 輪: 鎖の鉄球の払いと、当たった所の床の輪
fn ring_art(g: &G) {
    for (w, c) in [(18.0, "rgba(255,255,255,.3)"), (8.0, "rgba(255,255,255,1)")] {
        g.begin_path();
        let _ = g.arc(64.0, 64.0, 52.0, 0.0, 6.3);
        g.set_line_width(w);
        g.set_stroke_style_str(c);
        g.stroke();
    }
}
/// 衝撃の星: とがった先が 10 本
fn burst_art(g: &G) {
    g.begin_path();
    for k in 0..20 {
        let a = k as f64 / 20.0 * 2.0 * PI;
        let r = if k % 2 == 0 { 60.0 } else { 24.0 };
        let (x, y) = (64.0 + a.cos() * r, 64.0 + a.sin() * r);
        if k == 0 { g.move_to(x, y); } else { g.line_to(x, y); }
    }
    g.close_path();
    g.set_fill_style_str("rgba(255,255,255,.95)");
    g.fill();
}

impl ItemView {
    pub fn new(kit: &mut Kit) -> ItemView {
        let pick_tex: Vec<TexId> = kinds().into_iter().map(|k| kit.canvas_texture(128, 128, true, |g| pickup_art(g, k))).collect();
        let held_tex = Weapon::ALL.map(|w| kit.canvas_texture(128, 128, true, |g| weapon_art(g, w)));
        // 衝撃波: 前が明るい三日月
        let wave_tex = kit.canvas_texture(128, 128, true, |g| {
            for (w, c) in [(26.0, "rgba(255,207,77,.55)"), (14.0, "rgba(255,246,228,.95)")] {
                g.begin_path();
                let _ = g.arc(64.0, 150.0, 104.0, PI * 1.22, PI * 1.78);
                g.set_line_width(w);
                g.set_line_cap("round");
                g.set_stroke_style_str(c);
                g.stroke();
            }
        });
        let puddle_tex = kit.canvas_texture(128, 128, true, |g| {
            g.set_fill_style_str("rgba(120,40,140,.82)");
            for (x, y, r) in [(64.0, 64.0, 44.0), (34.0, 52.0, 20.0), (94.0, 78.0, 22.0), (58.0, 100.0, 16.0), (86.0, 36.0, 15.0)] { g.begin_path(); let _ = g.arc(x, y, r, 0.0, 6.3); g.fill(); }
            g.set_fill_style_str("rgba(255,255,255,.5)");
            for (x, y, r) in [(50.0, 50.0, 9.0), (78.0, 70.0, 5.0)] { g.begin_path(); let _ = g.arc(x, y, r, 0.0, 6.3); g.fill(); }
        });
        let sprite = |kit: &mut Kit, map: TexId, order: i32| {
            let m = kit.material(Material { depth_write: false, ..Material::sprite().with_map(map) });
            let n = kit.scene.sprite(None, m);
            kit.scene[n].render_order = order;
            kit.scene[n].visible = false;
            (n, m)
        };
        let picks = (0..PICKS).map(|_| sprite(kit, pick_tex[0], 3)).collect();
        let glow_t = glow_texture(kit);
        let glow_m = kit.material(Material { fog: false, ..Material::sprite().with_map(glow_t).with_color(Color::css("#ffd766")).additive() });
        let held = (0..8).map(|_| {
            let (n, m) = sprite(kit, held_tex[0], 4);
            let gl = kit.scene.sprite(None, glow_m);
            kit.scene[gl].render_order = 3;
            kit.scene[gl].visible = false;
            (n, m, gl)
        }).collect();
        let shots = (0..SHOTS).map(|_| sprite(kit, wave_tex, 4)).collect();
        let trap_g = kit.geometry(&plane(2.0, 2.0).rotate_x(-std::f32::consts::FRAC_PI_2).owned());
        let trap_m = kit.material(Material { polygon_offset: Some((-2.0, 0.0)), ..Material::basic().with_map(puddle_tex).see_through() });
        let traps = (0..TRAPS).map(|_| { let n = kit.scene.mesh(None, trap_g, trap_m); kit.scene[n].render_order = 1; kit.scene[n].visible = false; n }).collect();
        let arc_tex = kit.canvas_texture(128, 128, true, arc_art);
        let ring_tex = kit.canvas_texture(128, 128, true, ring_art);
        let burst_tex = kit.canvas_texture(128, 128, true, burst_art);
        let flat_g = kit.geometry(&plane(1.0, 1.0).rotate_x(-std::f32::consts::FRAC_PI_2).owned());
        let flat = |kit: &mut Kit, map: TexId| {
            let m = kit.material(Material { fog: false, side: island_kit::Side::Double, ..Material::basic().with_map(map).additive() });
            let n = kit.scene.mesh(None, flat_g, m);
            kit.scene[n].render_order = 5;
            kit.scene[n].visible = false;
            kit.scene[n].frustum_culled = false;
            (n, m)
        };
        let slash = (0..8).map(|_| flat(kit, arc_tex)).collect();
        let hits = (0..HITS).map(|_| {
            let (r, rm) = flat(kit, ring_tex);
            let bm = kit.material(Material { fog: false, ..Material::sprite().with_map(burst_tex).additive() });
            let b = kit.scene.sprite(None, bm);
            kit.scene[b].render_order = 6;
            kit.scene[b].visible = false;
            (r, rm, b, bm)
        }).collect();
        ItemView { pick_tex, held_tex, wave_tex, picks, held, shots, traps, swing_at: [-9.0; 8], slash, slash_of: [(Weapon::Sword, false, 1.0); 8], arc_tex, ring_tex, hits, hit_at: vec![(-9.0, 0, false); HITS], models: vec![None; 15], held_models: vec![None; 6] }
    }

    /// 3D の物が読めた（kind_index の番号）。読めるまでは今までの札を出す
    pub fn set_model(&mut self, kit: &mut Kit, k: usize, g: &Glb) {
        if k >= self.models.len() || self.models[k].is_some() { return; }
        let tex = g.materials.iter().find_map(|m| m.texture).map(|t| kit.glb_texture(g, t));
        let mat = kit.material(soft_material(SoftOptions { map: tex, rim: 1.35, ..Default::default() }));
        let mut make = |_: &mut Kit| mat;
        // 後ろに種類の色の淡い光（遠くからでも色で見つかる。札の丸の代わり）
        let col = Color::css(match kinds()[k] { PickupKind::Mark(s) => stat_color(s), PickupKind::Breath => "#8fe2ff", PickupKind::Poison => "#b45cff", PickupKind::Arm(_) | PickupKind::Mystery => "#ffd766", PickupKind::Jet => "#ff8a3c" });
        let glow_t = glow_texture(kit);
        let glow_m = kit.material(Material { fog: false, opacity: 0.5, ..Material::sprite().with_map(glow_t).with_color(col).additive() });
        let pool: Vec<NodeId> = (0..MODEL_POOL).map(|_| {
            let n = place_model(kit, g, &mut make);
            kit.scene.traverse(n, &mut |sc, c| sc[c].cast_shadow = true);
            let gl = kit.scene.sprite(Some(n), glow_m);
            kit.scene[gl].set_scale(1.7);
            kit.scene[gl].render_order = 2;
            n
        }).collect();
        self.models[k] = Some(pool);
        if (7..13).contains(&k) {
            let held: Vec<NodeId> = (0..8).map(|_| place_model(kit, g, &mut make)).collect();
            self.held_models[k - 7] = Some(held);
        }
    }

    /// 振った: 持っている板を回し、前へ払う弧を出す（左右は交互）
    pub fn swung(&mut self, i: usize, now: f64, weapon: Weapon, charged: bool) {
        if i >= 8 { return; }
        let side = if self.slash_of[i].2 > 0.0 { -1.0 } else { 1.0 };
        self.swing_at[i] = now;
        self.slash_of[i] = (weapon, charged, side);
    }
    /// 当たった: 床に輪が広がり、衝撃の星が一瞬出る
    pub fn hit(&mut self, i: usize, now: f64, charged: bool) {
        let k = (0..HITS).min_by(|a, b| self.hit_at[*a].0.total_cmp(&self.hit_at[*b].0)).unwrap_or(0);
        self.hit_at[k] = (now, i, charged);
    }

    pub fn hide(&self, kit: &mut Kit) {
        for p in self.models.iter().chain(self.held_models.iter()).flatten() { for n in p { kit.scene[*n].visible = false; } }
        for (n, _) in &self.picks { kit.scene[*n].visible = false; }
        for (n, _, g) in &self.held { kit.scene[*n].visible = false; kit.scene[*g].visible = false; }
        for (n, _) in &self.shots { kit.scene[*n].visible = false; }
        for n in &self.traps { kit.scene[*n].visible = false; }
        for (n, _) in &self.slash { kit.scene[*n].visible = false; }
        for (r, _, b, _) in &self.hits { kit.scene[*r].visible = false; kit.scene[*b].visible = false; }
    }

    /// 毎コマ: near は基準の走者のスタートからの距離（自分。見本のレースは先頭）
    pub fn update(&mut self, kit: &mut Kit, race: &Race, near: f64, now: f64) {
        let tr = &race.track;
        // 落ちている札: ふわふわ浮いて、近づくと少し大きく見える。3D の物が読めた種類は、回る 3D の物を出す
        let mut k = 0;
        let mut used = [0usize; 15];
        for (q, p) in race.pickups.iter().enumerate() {
            if k >= PICKS { break; }
            if p.taken || p.dist < near - BEHIND { continue; }
            if p.dist > near + AHEAD { break; }
            let v = tr.pos(tr.s_at(p.dist), p.d);
            // カメラのすぐ前の札は画面をふさぐので出さない（横から見せるカメラのとき）
            if (Vec3::new(v.x as f32, v.y as f32 + 0.8, v.z as f32) - kit.camera.position).length() < 3.5 { continue; }
            let ki = kind_index(p.kind);
            if let Some(pool) = &self.models[ki] && used[ki] < pool.len() {
                let n = pool[used[ki]];
                used[ki] += 1;
                let bob = (now * 2.6 + q as f64 * 1.7).sin() * 0.12;
                let arm = matches!(p.kind, PickupKind::Arm(_));
                kit.scene[n].visible = true;
                kit.scene[n].position = Vec3::new(v.x as f32, (v.y + 0.9 + bob) as f32, v.z as f32);
                kit.scene[n].rotation = Rot::euler(if arm { -0.35 } else { 0.0 }, (now * 1.8 + q as f64 * 0.9) as f32, 0.0);
                kit.scene[n].set_scale(if arm { 1.5 } else { 1.3 });
                continue;
            }
            let (n, m) = self.picks[k];
            k += 1;
            let bob = (now * 2.6 + q as f64 * 1.7).sin() * 0.09;
            kit.mat_mut(m).map = Some(self.pick_tex[kind_index(p.kind)]);
            kit.scene[n].visible = true;
            kit.scene[n].position = Vec3::new(v.x as f32, (v.y + 0.82 + bob) as f32, v.z as f32);
            let s = if matches!(p.kind, PickupKind::Arm(_) | PickupKind::Mystery) { 1.6 } else { 1.25 };
            kit.scene[n].set_scale(s);
        }
        for (n, _) in self.picks.iter().skip(k) { kit.scene[*n].visible = false; }
        for (ki, p) in self.models.iter().enumerate() { if let Some(p) = p { for n in p.iter().skip(used[ki]) { kit.scene[*n].visible = false; } } }
        // 持っている武器: 肩の外に浮かぶ。振ると回る。ためている間は光がふくらむ
        for (i, r) in race.runners.iter().enumerate() {
            let (n, m, gl) = self.held[i];
            for p in self.held_models.iter().flatten() { kit.scene[p[i]].visible = false; }
            let Some(w) = r.weapon else { kit.scene[n].visible = false; kit.scene[gl].visible = false; continue };
            let v = tr.pos(r.s, (r.d + 0.75).min(tr.width - 0.1));
            let since = now - self.swing_at[i];
            let spin = if since < 0.28 { (since / 0.28) * 2.0 * PI } else { 0.0 };
            let low = r.weapon_t < 2.5 && (now * 9.0).sin() > 0.0;
            if let MatKind::Sprite { rotation, .. } = &mut kit.mat_mut(m).kind { *rotation = -spin as f32; }
            kit.mat_mut(m).map = Some(self.held_tex[w.i()]);
            kit.mat_mut(m).opacity = if low { 0.45 } else { 1.0 };
            let y = v.y + r.y + 1.55 + (now * 5.0 + i as f64).sin() * 0.05;
            // 3D の武器が読めていれば、肩の外にゆっくり回る 3D の武器を出す（切れそうなときは点滅）
            if let Some(p) = &self.held_models[w.i()] {
                let h = p[i];
                kit.scene[n].visible = false;
                kit.scene[h].visible = !low;
                kit.scene[h].position = Vec3::new(v.x as f32, y as f32, v.z as f32);
                kit.scene[h].rotation = Rot::euler(-0.4, (tr.yaw(r.s) + now * 1.2 + spin) as f32, 0.0);
                kit.scene[h].set_scale((if since < 0.28 { 1.25 } else { 0.85 + r.charge * 0.3 }) as f32);
            } else {
                kit.scene[n].visible = true;
                kit.scene[n].position = Vec3::new(v.x as f32, y as f32, v.z as f32);
                kit.scene[n].set_scale(if since < 0.28 { 1.55 } else { 1.0 + r.charge as f32 * 0.35 });
            }
            kit.scene[gl].visible = r.charge > 0.0;
            if r.charge > 0.0 {
                kit.scene[gl].position = Vec3::new(v.x as f32, y as f32, v.z as f32);
                let pulse = if r.charge >= 1.0 { 1.0 + (now * 16.0).sin() * 0.12 } else { 1.0 };
                kit.scene[gl].set_scale((1.2 + r.charge * 2.6 * pulse) as f32);
            }
        }
        // 飛んでいる攻撃
        for (q, (n, m)) in self.shots.iter().enumerate() {
            let Some(s) = race.shots.get(q) else { kit.scene[*n].visible = false; continue };
            let v = tr.pos(tr.s_at(s.dist), s.d);
            let wave = s.kind == ShotKind::Wave;
            let bolt = s.kind == ShotKind::Bolt;
            kit.mat_mut(*m).map = Some(if wave { self.wave_tex } else if bolt { self.held_tex[Weapon::Crossbow.i()] } else { self.held_tex[Weapon::Rose.i()] });
            if let MatKind::Sprite { rotation, .. } = &mut kit.mat_mut(*m).kind { *rotation = if wave || bolt { 0.0 } else { (now * 14.0) as f32 }; }
            kit.scene[*n].visible = true;
            kit.scene[*n].position = Vec3::new(v.x as f32, (v.y + if wave { 0.55 } else if bolt { 1.4 } else { 1.0 }) as f32, v.z as f32);
            kit.scene[*n].set_scale(if wave { 2.6 } else if bolt { 1.8 } else { 1.0 });
        }
        // 振った弧: 腰の高さに寝かせた板を、後ろ横から前へ払う。ためた一撃は金色で大きい。鎖は体のまわりを一周する輪
        for (i, r) in race.runners.iter().enumerate() {
            let (n, m) = self.slash[i];
            let since = now - self.swing_at[i];
            if since < 0.0 || since > SLASH_T { kit.scene[n].visible = false; continue; }
            let (w, charged, side) = self.slash_of[i];
            let k = (since / SLASH_T) as f32;
            let ease = 1.0 - (1.0 - k) * (1.0 - k);
            let yaw = tr.yaw(r.s) as f32;
            let chain = w == Weapon::Chain;
            let size = match (chain, charged) { (true, true) => 7.0, (true, false) => 5.2, (false, true) => 6.4, (false, false) => 3.6 };
            let fwd = if chain { 0.0 } else { 1.5 };
            let p = tr.pos(r.s + fwd, r.d);
            kit.scene[n].visible = true;
            kit.scene[n].position = Vec3::new(p.x as f32, (p.y + r.y + if chain { 0.8 } else { 1.15 }) as f32, p.z as f32);
            // 払う: 横から前へ回りながら、大きく斜めに傾ける（後ろから見ても斜めの筋に見える）
            let sweep = if chain { ease * 5.0 } else { side * (-0.9 + ease * 1.5) };
            let roll = if chain { side * 0.25 } else { side * 0.95 };
            kit.scene[n].rotation = Rot::euler_order(0.0, yaw + sweep, roll, island_kit::math::EulerOrder::YXZ);
            kit.scene[n].set_scale(size * (0.75 + ease * 0.35));
            let mat = kit.mat_mut(m);
            mat.map = Some(if chain { self.ring_tex } else { self.arc_tex });
            mat.color = Color::css(match (w, charged) { (_, true) => "#ffc93c", (Weapon::Rose, _) => "#ff6fa8", (Weapon::Bottle, _) => "#7fd6ff", _ => "#dff4ff" });
            mat.opacity = (1.0 - k * k) * if charged { 1.0 } else { 0.85 };
        }
        // 当たった所
        for (q, (rn, rm, bn, bm)) in self.hits.iter().enumerate() {
            let (at, i, charged) = self.hit_at[q];
            let since = now - at;
            if since < 0.0 || since > HIT_T || i >= race.runners.len() { kit.scene[*rn].visible = false; kit.scene[*bn].visible = false; continue; }
            let r = &race.runners[i];
            let k = (since / HIT_T) as f32;
            let p = tr.pos(r.s, r.d);
            kit.scene[*rn].visible = true;
            kit.scene[*rn].position = Vec3::new(p.x as f32, (p.y + 0.08) as f32, p.z as f32);
            kit.scene[*rn].rotation = Rot::IDENTITY;
            kit.scene[*rn].set_scale((if charged { 6.0 } else { 4.0 }) * (0.25 + (1.0 - (1.0 - k).powi(3)) * 0.75));
            let rmat = kit.mat_mut(*rm);
            rmat.color = Color::css(if charged { "#ffd25a" } else { "#fff3d6" });
            rmat.opacity = (1.0 - k) * 0.95;
            // 星は最初の 0.14 秒だけ、大きく出てしぼむ
            let ks = (since / 0.14) as f32;
            kit.scene[*bn].visible = ks < 1.0;
            if ks < 1.0 {
                kit.scene[*bn].position = Vec3::new(p.x as f32, (p.y + r.y + 1.1) as f32, p.z as f32);
                kit.scene[*bn].set_scale((if charged { 3.6 } else { 2.3 }) * (1.0 - ks * 0.4));
                let bmat = kit.mat_mut(*bm);
                bmat.color = Color::css(if charged { "#ffe08a" } else { "#ffffff" });
                bmat.opacity = 1.0 - ks * ks;
                if let MatKind::Sprite { rotation, .. } = &mut bmat.kind { *rotation = (q as f32) * 0.7 + ks * 0.4; }
            }
        }
        // 床のわな
        for (q, n) in self.traps.iter().enumerate() {
            let Some(t) = race.traps.get(q) else { kit.scene[*n].visible = false; continue };
            let v = tr.pos(tr.s_at(t.dist), t.d);
            kit.scene[*n].visible = true;
            kit.scene[*n].position = Vec3::new(v.x as f32, (v.y + 0.04) as f32, v.z as f32);
            kit.scene[*n].rotation = Rot::y(tr.yaw(tr.s_at(t.dist)) as f32);
            let fade = (t.life / 1.2).min(1.0) as f32;
            kit.scene[*n].scale = Vec3::new(t.r as f32 * fade.max(0.2), 1.0, (ITEM.trap_len as f32 + 0.3) * fade.max(0.2));
        }
    }
}
