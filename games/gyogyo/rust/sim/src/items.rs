// 走路のマークと武器（docs/spec.md「マークと武器」）。置き場所と種類はレースの種で決まり、順位では変えない。
// 取る・振る・当たるの進め方は race.rs（走者と同じ刻み）。ここは種類・数字・置き方だけ。
use crate::race::{ObEntry, Rng};
use crate::track::Track;

/// 能力（育成で育つ。0〜9 段）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stat { Speed, Stamina, Guts, Jump, Turbo }
impl Stat {
    pub const ALL: [Stat; 5] = [Stat::Speed, Stat::Stamina, Stat::Guts, Stat::Jump, Stat::Turbo];
    pub fn i(self) -> usize { self as usize }
    pub fn id(self) -> &'static str { match self { Stat::Speed => "speed", Stat::Stamina => "stamina", Stat::Guts => "guts", Stat::Jump => "jump", Stat::Turbo => "turbo" } }
    pub fn name(self) -> &'static str { match self { Stat::Speed => "スピード", Stat::Stamina => "スタミナ", Stat::Guts => "ガッツ", Stat::Jump => "ジャンプ", Stat::Turbo => "ターボ" } }
    /// 走路のマークの 1 字
    pub fn glyph(self) -> &'static str { match self { Stat::Speed => "速", Stat::Stamina => "体", Stat::Guts => "根", Stat::Jump => "跳", Stat::Turbo => "加" } }
}

/// 能力の段（Stat::i の順）
pub type Levels = [u8; 5];
/// 単発とカップは全員ここから（この段で、マークが無かったころと同じ走り）
pub const BASE_LEVELS: Levels = [3; 5];
pub const MAX_LEVEL: u8 = 9;

fn over(lv: &Levels, s: Stat) -> f64 { lv[s.i()] as f64 - 3.0 }
/// 最高速の倍率（1 段で 1.5%）
pub fn speed_mul(lv: &Levels) -> f64 { 1.0 + 0.015 * over(lv, Stat::Speed) }
/// ダッシュの消費の倍率（1 段で 6% 減る）
pub fn drain_mul(lv: &Levels) -> f64 { 1.0 - 0.06 * over(lv, Stat::Stamina) }
/// よろける時間と、攻撃で減る息の倍率（1 段で 10% 減る）
pub fn guts_mul(lv: &Levels) -> f64 { 1.0 - 0.1 * over(lv, Stat::Guts) }
/// 跳び越えたあとの伸びの秒（1 段で 0.3 秒）
pub fn hop_time(lv: &Levels) -> f64 { 0.3 * lv[Stat::Jump.i()] as f64 }
/// ダッシュの上乗せの倍率（1 段で 12%）
pub fn turbo_mul(lv: &Levels) -> f64 { 1.0 + 0.12 * over(lv, Stat::Turbo) }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weapon { Sword, Chain, Rose, Bottle, Crossbow, Hammer }
impl Weapon {
    pub const ALL: [Weapon; 6] = [Weapon::Sword, Weapon::Chain, Weapon::Rose, Weapon::Bottle, Weapon::Crossbow, Weapon::Hammer];
    pub fn i(self) -> usize { self as usize }
    pub fn id(self) -> &'static str { match self { Weapon::Sword => "sword", Weapon::Chain => "chain", Weapon::Rose => "rose", Weapon::Bottle => "bottle", Weapon::Crossbow => "crossbow", Weapon::Hammer => "hammer" } }
    pub fn name(self) -> &'static str { match self { Weapon::Sword => "剣", Weapon::Chain => "鎖の鉄球", Weapon::Rose => "バラ", Weapon::Bottle => "ビン", Weapon::Crossbow => "ボーガン", Weapon::Hammer => "ハンマー" } }
    /// 使える回数
    pub fn uses(self) -> i32 { match self { Weapon::Sword | Weapon::Rose => 3, Weapon::Chain | Weapon::Bottle | Weapon::Hammer => 2, Weapon::Crossbow => 1 } }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickupKind {
    Mark(Stat), Breath, Poison, Arm(Weapon),
    /// 「？」の箱: 取った瞬間の順位で中身が決まる（後ろほど強い物）
    Mystery,
    /// 噴射: 少しの間、大きく速くなる（「？」の箱から出る）
    Jet,
}
impl PickupKind {
    pub fn id(self) -> &'static str { match self { PickupKind::Mark(s) => s.id(), PickupKind::Breath => "breath", PickupKind::Poison => "poison", PickupKind::Arm(w) => w.id(), PickupKind::Mystery => "mystery", PickupKind::Jet => "jet" } }
}

/// 「？」の箱の中身。q は順位の後ろさ（0 = 先頭、1 = 最下位）、r は 0..1 の乱数
pub fn mystery_content(q: f64, r: f64) -> PickupKind {
    let q = q.clamp(0.0, 1.0);
    let table = [
        (PickupKind::Arm(Weapon::Bottle), 30.0 * (1.0 - q) + 2.0),
        (PickupKind::Arm(Weapon::Rose), 25.0 * (1.0 - q) + 5.0),
        (PickupKind::Arm(Weapon::Sword), 20.0 * (1.0 - q) + 4.0),
        (PickupKind::Arm(Weapon::Chain), 15.0 * (1.0 - q) + 4.0),
        (PickupKind::Arm(Weapon::Hammer), 25.0 * q),
        (PickupKind::Jet, 30.0 * q),
        (PickupKind::Arm(Weapon::Crossbow), 30.0 * q * q),
    ];
    let total: f64 = table.iter().map(|t| t.1).sum();
    let mut x = r * total;
    for (k, w) in table { if x < w { return k; } x -= w; }
    PickupKind::Arm(Weapon::Sword)
}

/// 走路に落ちている物（スタートからの距離と、内柵からの距離）
#[derive(Clone, Copy, Debug)]
pub struct Pickup { pub dist: f64, pub d: f64, pub kind: PickupKind, pub taken: bool }

/// 飛んでいく攻撃（剣の衝撃波・バラ）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ShotKind { Wave, Rose, Bolt }
#[derive(Clone, Copy, Debug)]
pub struct Shot {
    pub id: u32, pub kind: ShotKind, pub owner: usize, pub dist: f64, pub d: f64,
    /// 速さ（m/s）と、あと進める距離
    pub v: f64, pub left: f64,
    /// 当たる幅の半分
    pub hw: f64, pub charged: bool,
    /// もう済んだ走者（当たった・よけた）の印
    pub seen: u8,
}
/// 床のわな（ビンの中身）
#[derive(Clone, Copy, Debug)]
pub struct Trap { pub id: u32, pub owner: usize, pub dist: f64, pub d: f64, pub r: f64, pub life: f64, pub seen: u8 }

pub struct ItemTune {
    /// 取れる横の幅・取れる高さ
    pub pick_r: f64, pub pick_y: f64,
    pub breath: f64, pub poison: f64,
    pub weapon_time: f64, pub swing_cd: f64,
    /// ためはじめ・満ちる・勝手に出る（押し続けた秒）と、ためる間の消費（毎秒）
    pub charge_start: f64, pub charge_full: f64, pub charge_auto: f64, pub charge_drain: f64, pub charge_mul: f64,
    pub hit_stumble: f64, pub hit_stamina: f64, pub hit_speed: f64, pub immune: f64,
    /// 当たったあと、よろけている間に重ねる速さの倍率（魚々娘で足した）
    pub hit_slow: f64,
    pub hop_mul: f64,
    /// 剣（前の長さ・横）、鎖（前後・横・はじく速さ）
    pub sword_reach: f64, pub sword_side: f64, pub chain_reach: f64, pub chain_side: f64, pub chain_push: f64,
    /// 衝撃波・バラ（上乗せの速さ・進む距離・幅の半分・よけられる高さ）
    pub wave_v: f64, pub wave_range: f64, pub wave_hw: f64, pub rose_v: f64, pub rose_range: f64, pub rose_hw: f64, pub rose_fan: f64, pub dodge_y: f64,
    /// ビン（置く後ろの距離・幅の半分・進む向きの半分・残る秒）
    pub trap_back: f64, pub trap_r: f64, pub trap_len: f64, pub trap_life: f64,
    /// ボーガンの矢の速さ（m/s。走路を 1 周して撃った本人のすぐ前で消える。進路は問わず触れた全員に当たる）
    pub bolt_v: f64,
    /// ハンマー（前の長さ・横）と、当たった相手が真上へ飛ばされている秒・高さ（ためると長く高い）
    pub hammer_reach: f64, pub hammer_side: f64, pub launch_t: f64, pub launch_h: f64,
    /// 噴射（秒・速さの倍率）
    pub jet_t: f64, pub jet_mul: f64,
}
pub const ITEM: ItemTune = ItemTune {
    pick_r: 0.95, pick_y: 0.3,
    breath: 45.0, poison: 30.0,
    weapon_time: 12.0, swing_cd: 0.5,
    charge_start: 0.2, charge_full: 0.8, charge_auto: 1.6, charge_drain: 30.0, charge_mul: 1.5,
    hit_stumble: 1.6, hit_stamina: 25.0, hit_speed: 0.45, immune: 1.6,
    hit_slow: 0.5,
    hop_mul: 1.06,
    sword_reach: 3.4, sword_side: 1.5, chain_reach: 2.6, chain_side: 3.0, chain_push: 5.5,
    wave_v: 14.0, wave_range: 30.0, wave_hw: 1.2, rose_v: 16.0, rose_range: 26.0, rose_hw: 0.9, rose_fan: 1.6, dodge_y: 0.35,
    trap_back: 1.6, trap_r: 1.1, trap_len: 0.9, trap_life: 12.0,
    bolt_v: 80.0,
    hammer_reach: 2.8, hammer_side: 1.8, launch_t: 2.4, launch_h: 9.0,
    jet_t: 3.0, jet_mul: 1.7,
};

/// 並びの間隔（m）と、空ける所
const ROW_GAP: f64 = 60.0;
const ROW_JITTER: f64 = 24.0;
pub const KEEP_START: f64 = 60.0;
pub const KEEP_GOAL: f64 = 120.0;

fn roll_kind(r: f64) -> PickupKind {
    // マーク 45%（5 種で等分）・息 12%・毒 8%・「？」の箱 35%（武器と噴射は箱から。中身は取ったときの順位で決まる）
    if r < 0.45 { return PickupKind::Mark(Stat::ALL[((r / 0.09) as usize).min(4)]); }
    if r < 0.57 { return PickupKind::Breath; }
    if r < 0.65 { return PickupKind::Poison; }
    PickupKind::Mystery
}

/// レースの始めに置く。約 70m ごとに横一列（2〜3 個）。距離の順
pub fn place_pickups(track: &Track, obstacles: &[ObEntry], rand: &mut Rng) -> Vec<Pickup> {
    let mut out = Vec::new();
    let w = track.width;
    let mut dist = KEEP_START + 6.0 + rand.next() * 20.0;
    while dist <= track.race_len - KEEP_GOAL {
        // 障害の前後は空ける（跳ぶことに集中できるように）
        if obstacles.iter().any(|e| dist > e.dist - 14.0 && dist < e.dist + e.ob.len + 12.0) { dist += 9.0; continue; }
        let n = 2 + (rand.next() * 2.0) as usize;
        let slot = (w - 2.0) / n as f64;
        for k in 0..n {
            let d = 1.0 + slot * (k as f64 + 0.2 + rand.next() * 0.6);
            out.push(Pickup { dist, d, kind: roll_kind(rand.next()), taken: false });
        }
        dist += ROW_GAP + rand.next() * ROW_JITTER;
    }
    out
}
