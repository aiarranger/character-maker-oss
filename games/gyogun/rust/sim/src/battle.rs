// 戦いの計算（描画から独立）。主役・お菓子の大軍・砦・大将・拾い物・得点。
// 出来事（events）を積み、描画と音はそれを読んで見せる。時間は秒、長さは m、地面は y = 0 の平面。
//
// TypeScript 版 src/sim/battle.ts の写し（旧版は 2026-11-10 に削除。それまでは tests/parity.rs でビット単位の一致を確かめた）。そのための約束:
// - 乱数を呼ぶ順番、式の評価の順番、配列を回す順番を TS と同じにする。
// - 敵の参照は enemies のインデックス。`e === v` はインデックスの比較。spawn の再利用も同じ順番で探す。
// - TS の for-of は生きた長さで回るので、途中で増えたり書き換わったりする配列は while と添字で回す。
use crate::math::{self, atan2, cos, exp, hypot, or1, sin, PI};
use crate::moves::{is_charge, is_normal, same, spin_at, Hit, HitFx, HitShape, Knock, Move, CHARGES, DASH_ATTACK, HURL, HURL_RELEASE, JUMP_ATTACK, JUMP_CHARGE, RANBU, RANBU_CUTIN, NORMALS};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyKind { Strawberry, Macaron, Mikan, Creampuff, Cottoncandy, Donut, Pudding, Shortcake, Cakeking }
impl EnemyKind {
    pub fn name(self) -> &'static str {
        match self { EnemyKind::Strawberry => "strawberry", EnemyKind::Macaron => "macaron", EnemyKind::Mikan => "mikan", EnemyKind::Creampuff => "creampuff", EnemyKind::Cottoncandy => "cottoncandy", EnemyKind::Donut => "donut", EnemyKind::Pudding => "pudding", EnemyKind::Shortcake => "shortcake", EnemyKind::Cakeking => "cakeking" }
    }
    #[inline] pub fn def(self) -> &'static KindDef { &KINDS[self as usize] }
    /// 砂糖でできた敵（倒すと飴がたまる）。果物のいちご・みかんは数えない
    #[inline] pub fn sugary(self) -> bool { !matches!(self, EnemyKind::Strawberry | EnemyKind::Mikan) }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role { Grunt, Archer, Brute, Captain }
impl Role { pub fn name(self) -> &'static str { match self { Role::Grunt => "grunt", Role::Archer => "archer", Role::Brute => "brute", Role::Captain => "captain" } } }

pub struct KindDef { pub hp: f64, pub speed: f64, pub radius: f64, pub dmg: f64, pub role: Role, pub height: f64, pub name: &'static str }
/// EnemyKind の順
pub const KINDS: [KindDef; 9] = [
    KindDef { hp: 60.0, speed: 4.6, radius: 0.48, dmg: 34.0, role: Role::Grunt, height: 1.0, name: "いちご足軽" },
    KindDef { hp: 70.0, speed: 4.4, radius: 0.5, dmg: 38.0, role: Role::Grunt, height: 0.95, name: "マカロン槍兵" },
    KindDef { hp: 60.0, speed: 5.0, radius: 0.48, dmg: 34.0, role: Role::Grunt, height: 0.92, name: "みかん足軽" },
    KindDef { hp: 70.0, speed: 4.2, radius: 0.5, dmg: 36.0, role: Role::Grunt, height: 0.95, name: "シュー足軽" },
    KindDef { hp: 55.0, speed: 4.8, radius: 0.5, dmg: 32.0, role: Role::Grunt, height: 1.0, name: "わたあめ忍" },
    KindDef { hp: 70.0, speed: 4.0, radius: 0.5, dmg: 40.0, role: Role::Archer, height: 0.98, name: "ドーナツ投石兵" },
    KindDef { hp: 170.0, speed: 3.6, radius: 0.65, dmg: 80.0, role: Role::Brute, height: 1.2, name: "ぷりん重騎" },
    KindDef { hp: 1700.0, speed: 4.2, radius: 1.2, dmg: 95.0, role: Role::Captain, height: 2.6, name: "ショートケーキ隊長" },
    KindDef { hp: 2100.0, speed: 3.8, radius: 1.35, dmg: 110.0, role: Role::Captain, height: 2.9, name: "ホールケーキ将軍" },
];

/// Carried: 主役に担がれている重騎 / Stacked: 巨人の体に積み重なっている雑兵
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemyState { Idle, Chase, Windup, Strike, Recover, Stagger, Air, Down, Getup, Dead, Carried, Stacked }
impl EnemyState {
    pub fn name(self) -> &'static str {
        match self { EnemyState::Idle => "idle", EnemyState::Chase => "chase", EnemyState::Windup => "windup", EnemyState::Strike => "strike", EnemyState::Recover => "recover", EnemyState::Stagger => "stagger", EnemyState::Air => "air", EnemyState::Down => "down", EnemyState::Getup => "getup", EnemyState::Dead => "dead", EnemyState::Carried => "carried", EnemyState::Stacked => "stacked" }
    }
}

/// 隊長の技（sweep / leap）。None は TS の ''
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnemySkill { None, Sweep, Leap }
impl EnemySkill { pub fn name(self) -> &'static str { match self { EnemySkill::None => "", EnemySkill::Sweep => "sweep", EnemySkill::Leap => "leap" } } }

#[derive(Clone, Debug)]
pub struct Enemy {
    pub id: i64, pub kind: EnemyKind, pub role: Role, pub alive: bool,
    pub x: f64, pub z: f64, pub y: f64, pub vx: f64, pub vz: f64, pub vy: f64,
    pub face: f64, pub hp: f64, pub max_hp: f64, pub state: EnemyState, pub st: f64, pub cd: f64,
    /// 空中・吹き飛びの回転（見た目用）
    pub spin: f64, pub spin_v: f64,
    /// 最後に当たってからの秒（白く光る見せ方）
    pub hit_t: f64,
    pub token: bool, pub home: i32, pub awake: bool,
    /// 攻撃の構えの向き・突進の目標
    pub aim_x: f64, pub aim_z: f64,
    pub poise: f64, pub dead_t: f64,
    /// 隊長の技（sweep / leap）
    pub skill: EnemySkill,
    /// 最後に当てた技の連番（同じ技で二度当てない）
    pub last_hit_key: i64,
    /// 群衆ドミノ: 弾として飛んでいる連鎖の番号（0 = 弾ではない）・何代目の弾か・あと何体巻き込めるか・最後に巻き込まれた連鎖
    pub fling: i64, pub gen: i32, pub ammo: i32, pub chain_hit: i64,
    /// 連鎖に巻き込まれてからの秒（金に光る見せ方）
    pub chain_t: f64,
    /// 集めて撃った一撃で飛んだ弾（重騎なみに突き抜ける）
    pub boost: bool,
    /// 隊長崩しの再発動までの秒
    pub break_cd: f64,
    /// 主役が投げた重騎（着地で爆ぜる）
    pub thrown: bool,
    /// 士気崩壊: 逃げ込む砦（-1 = 逃げていない）
    pub flee: i32,
    /// 飴鏡で跳ね返った回数（弾になるたびに 0 から）
    pub mirror: i32,
    /// 曲進の弾が曲がる速さ（rad/秒、0 = まっすぐ）
    pub curve: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TelegraphKind { Circle, Ring, Cone, Line }
impl TelegraphKind { pub fn name(self) -> &'static str { match self { TelegraphKind::Circle => "circle", TelegraphKind::Ring => "ring", TelegraphKind::Cone => "cone", TelegraphKind::Line => "line" } } }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TelegraphSrc { Boss, Captain, Brute, Sauce }
impl TelegraphSrc { pub fn name(self) -> &'static str { match self { TelegraphSrc::Boss => "boss", TelegraphSrc::Captain => "captain", TelegraphSrc::Brute => "brute", TelegraphSrc::Sauce => "sauce" } } }
#[derive(Clone, Copy, Debug)]
pub struct Telegraph { pub id: i64, pub kind: TelegraphKind, pub x: f64, pub z: f64, pub r: f64, pub face: f64, pub arc: f64, pub w: f64, pub t: f64, pub fire: f64, pub dmg: f64, pub src: TelegraphSrc, pub done: bool, pub grow: Option<f64> }
#[derive(Clone, Copy, Debug)]
pub struct Projectile { pub id: i64, pub x: f64, pub y: f64, pub z: f64, pub vx: f64, pub vy: f64, pub vz: f64, pub t: f64, pub dmg: f64, pub alive: bool }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PickupKind { Bun, Konpeito }
impl PickupKind { pub fn name(self) -> &'static str { match self { PickupKind::Bun => "bun", PickupKind::Konpeito => "konpeito" } } }
#[derive(Clone, Copy, Debug)]
pub struct Pickup { pub id: i64, pub kind: PickupKind, pub x: f64, pub z: f64, pub t: f64, pub alive: bool }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owner { Enemy, Ally }
impl Owner { pub fn name(self) -> &'static str { match self { Owner::Enemy => "enemy", Owner::Ally => "ally" } } }
#[derive(Clone, Copy, Debug)]
pub struct Stronghold { pub id: i32, pub name: &'static str, pub x: f64, pub z: f64, pub r: f64, pub owner: Owner, pub captain: i64, pub spawn_t: f64, pub alarm: bool }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BossState { Dormant, Enter, Idle, Walk, PunchL, PunchR, Stomp, Sauce, Roar, Hit, Down, Dead }
impl BossState {
    pub fn name(self) -> &'static str {
        match self { BossState::Dormant => "dormant", BossState::Enter => "enter", BossState::Idle => "idle", BossState::Walk => "walk", BossState::PunchL => "punchL", BossState::PunchR => "punchR", BossState::Stomp => "stomp", BossState::Sauce => "sauce", BossState::Roar => "roar", BossState::Hit => "hit", BossState::Down => "down", BossState::Dead => "dead" }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Boss { pub x: f64, pub z: f64, pub face: f64, pub hp: f64, pub max_hp: f64, pub state: BossState, pub st: f64, pub cd: f64, pub poise: f64, pub hit_t: f64, pub combo: i64, pub next: i64, pub last_hit_key: i64 }

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
/// Wire: 飴糸に引かれて飛んでいる / Ride: わたあめ雲に乗って飛んでいる
pub enum PlayerState { Free, Attack, Dodge, Hurt, Dead, Wire, Ride }
impl PlayerState { pub fn name(self) -> &'static str { match self { PlayerState::Free => "free", PlayerState::Attack => "attack", PlayerState::Dodge => "dodge", PlayerState::Hurt => "hurt", PlayerState::Dead => "dead", PlayerState::Wire => "wire", PlayerState::Ride => "ride" } } }
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Queued { N, C }
impl Queued { pub fn name(self) -> &'static str { match self { Queued::N => "n", Queued::C => "c" } } }
#[derive(Clone, Copy, Debug)]
pub struct Player {
    pub x: f64, pub z: f64, pub y: f64, pub vy: f64, pub vx: f64, pub vz: f64, pub face: f64,
    pub hp: f64, pub max_hp: f64, pub ranbu: f64, pub state: PlayerState, pub st: f64,
    pub r#move: Option<&'static Move>, pub move_t: f64, pub chain: i32, pub queued: Option<Queued>, pub air: bool,
    pub invuln: f64, pub hurt_t: f64, pub kos: i64, pub combo: i64, pub combo_t: f64, pub max_combo: i64,
    pub dmg_taken: f64, pub move_key: i64, pub dodge_cd: f64, pub speed: f64,
    /// 担いでいる重騎の id（0 = 担いでいない）と、担いでからの秒
    pub carry: i64, pub carry_t: f64,
    /// 格（0 初陣・1 壱・2 弐・3 参）。1戦の中だけで上がる
    pub rank: i32,
    /// 飴糸: いまの（最後の）飛び方と、次に出せるまでの秒
    pub wire: Wire, pub wire_cd: f64,
    /// 飴: 持っている数（AME_MAX まで）と、次の 1 個までのたまり具合（AME_PER で 1 個）
    pub ame: i32, pub ame_fill: i32,
    /// 猪鹿蝶: 直前の糸斬りが何回目か（0 = 続いていない）・続けられる残りの秒・斬った直後の残りの秒（この間の飴糸は空へ逃げる）・最後に斬った的
    pub wire_chain: i32, pub wire_chain_t: f64, pub wire_esc: f64, pub wire_last: i64,
    /// わたあめ雲: 次に呼べるまでの秒（乗ってからの秒は st）
    pub ride_cd: f64,
}


/// 飴糸: 飛び始めの場所・糸の先（的が動けば追う）・引かれて飛ぶ秒・的（0 = 空、-1 = 大将、ほかは敵の id）
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wire { pub x0: f64, pub y0: f64, pub z0: f64, pub ax: f64, pub ay: f64, pub az: f64, pub dur: f64, pub target: i64,
    /// esc = 斬った直後に空へ逃げる飛び（新しい糸は撃たない）、flip = 的の逆へ後ろ宙返りで飛ぶ（向きは的を向いたまま）
    pub esc: bool, pub flip: bool,
    /// 斬り込む飛びの振り子（横へふくらむ大きさ m。左右の向きは符号）・三撃目の直前のスローを出したか
    pub swing: f64, pub slowed: bool }
/// わたあめ雲: 乗っていられる秒・飛ぶ高さ・速さ（m/s）・次に呼べるまでの秒
pub const RIDE_T: f64 = 6.0;
pub const RIDE_Y: f64 = 4.0;
const RIDE_SPEED: f64 = 15.0;
const RIDE_CD: f64 = 3.0;
/// この高さより上の主役には、地上の敵の斬りと突進は届かない（跳躍の頂点は約 2.1m なので、跳ぶだけでは避けられない）
const REACH_Y: f64 = 2.5;
/// 糸を撃ってから引かれ始めるまでの秒・引かれて飛ぶ速さ（m/s）
pub const WIRE_AIM: f64 = 0.1;
const WIRE_SPEED: f64 = 36.0;
/// 的を探す距離と向き（大将は遠く・広く）
const WIRE_RANGE: f64 = 34.0;
const WIRE_CONE: f64 = 50.0 * PI / 180.0;
const WIRE_BOSS_RANGE: f64 = 40.0;
const WIRE_BOSS_CONE: f64 = 60.0 * PI / 180.0;
/// 続けている間、向きを押していても「いま斬った的」へ戻る広さ
const WIRE_BACK_CONE: f64 = 70.0 * PI / 180.0;
/// 的がいないときに糸を掛ける空（何 m 先・高さ）
const WIRE_FREE: f64 = 15.0;
const WIRE_FREE_Y: f64 = 3.2;
/// 次の飴糸まで（斬ったあと・空へ飛んだあと）
const WIRE_CD_HIT: f64 = 0.3;
/// 飴: 持てる数と、1 個ぶんの撃破（足軽 1・重騎 3・隊長 8）
pub const AME_MAX: i32 = 6;
pub const AME_PER: i32 = 8;
/// 猪鹿蝶: 続けられる秒（斬ったとき・飴糸で着いたときに戻る）・回ごとの傷の倍率・3 回目の広さ
pub const WIRE_CHAIN_T: f64 = 2.0;
pub const WIRE_MULT: [f64; 3] = [1.0, 1.5, 2.5];
const WIRE_FINISH_R: f64 = 5.6;
/// 空へ逃げる飛び: 先の距離・高さ（今いる高さ + 2.5m より低くはしない）・次の飴糸まで
const WIRE_ESC: f64 = 12.0;
const WIRE_ESC_Y: f64 = 6.0;
const WIRE_CD_ESC: f64 = 0.25;
/// 続けられる間の落ち方（重力の倍率。ゆっくり落ちる）
const WIRE_FLOAT: f64 = 0.3;
const WIRE_CD_FREE: f64 = 0.5;
/// 巨人の上の大将を何回斬ると巨人が崩れるか
pub const WIRE_TOPPLE: i32 = 3;
/// 糸斬り（着いた瞬間）と、低く飛ぶ間に通り道の敵を弾く当たり
const WIRE_STRIKE: Hit = Hit { t: 0.5, shape: HitShape::Circle, r: 4.2, arc: None, w: None, off: None, dmg: 150.0, kb: Knock::Blast, power: 13.0, stop: 0.11, shake: 0.7, fx: HitFx::Heavy };
const WIRE_SWEEP: Hit = Hit { t: 0.0, shape: HitShape::Circle, r: 1.9, arc: None, w: None, off: None, dmg: 30.0, kb: Knock::Blast, power: 8.0, stop: 0.0, shake: 0.0, fx: HitFx::Slash };

/// 格が上がる武勲（壱・弐・参）
pub const RANK_AT: [i64; 3] = [450, 1150, 1900];
/// 格 弐の飛ばす勢い
pub const RANK_FLING: f64 = 1.25;
/// 育つ武器: 格の節目（初陣・壱・弐・参）での武器の大きさ。間は武勲でなめらかにつなぐ
pub const GROW_AT: [f64; 4] = [1.0, 1.5, 2.2, 3.0];

/// 武器の弾道: 強攻撃で直接飛ばした敵の飛び方（None = 当たった向きへ）
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Path { None, Straight, Spread, Curve }
impl Path { pub fn name(self) -> &'static str { match self { Path::None => "none", Path::Straight => "straight", Path::Spread => "spread", Path::Curve => "curve" } } }
pub struct Info { pub name: &'static str, pub desc: &'static str }
pub fn path_info(p: Path) -> Option<Info> {
    match p {
        Path::None => None,
        Path::Straight => Some(Info { name: "直進", desc: "そろって正面へ、細く長く貫く" }),
        Path::Spread => Some(Info { name: "拡散", desc: "扇に広がって、広く巻き込む" }),
        Path::Curve => Some(Info { name: "曲進", desc: "弧を描いて、横から回り込む" }),
    }
}
/// 直進の速さ・拡散の広がり（正面からのずれを何倍にするか、上限）・曲進の曲がる速さ（rad/秒）と飛び出しのずれ
const STRAIGHT_SPEED: f64 = 1.15;
const SPREAD_K: f64 = 2.4;
const SPREAD_MAX: f64 = 1.05;
const CURVE_RATE: f64 = 2.4;
const CURVE_START: f64 = -0.6;

/// 巨人の体の部位と、部位ごとの雑兵の数
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GiantPart { LegL, LegR, Hip, ArmL, ArmR }
pub const GIANT_COUNT: [i32; 5] = [12, 12, 8, 9, 9];
/// 巨人の積み場所（部位と、部位の中の番号）
#[derive(Clone, Copy, Debug)]
pub struct GiantSlot { pub part: GiantPart, pub i: i32 }
pub const GIANT_SLOTS: [GiantSlot; 50] = {
    let parts = [GiantPart::LegL, GiantPart::LegR, GiantPart::Hip, GiantPart::ArmL, GiantPart::ArmR];
    let mut a = [GiantSlot { part: GiantPart::LegL, i: 0 }; 50];
    let (mut k, mut n) = (0, 0);
    while k < 5 {
        let mut i = 0;
        while i < GIANT_COUNT[k] { a[n] = GiantSlot { part: parts[k], i }; n += 1; i += 1; }
        k += 1;
    }
    a
};
/// 巨人の脚の一段の高さ・腰の上に立つ大将の高さ
const LEG_ROW: f64 = 0.75;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Input {
    pub mx: f64, pub mz: f64, pub attack: bool, pub heavy: bool, pub jump: bool, pub dodge: bool, pub ranbu: bool, pub wire: bool,
}
pub const NO_INPUT: Input = Input { mx: 0.0, mz: 0.0, attack: false, heavy: false, jump: false, dodge: false, ranbu: false, wire: false };

#[derive(Clone, Debug)]
pub enum BattleEvent {
    Move { r#move: &'static Move },
    Swing { r#move: &'static Move, index: i32 },
    Hit { x: f64, y: f64, z: f64, count: i32, heavy: bool, fx: HitFx, dx: f64, dz: f64, enemy: i64, boss: bool },
    Impact { x: f64, z: f64, fx: HitFx, r: f64, face: f64, shape: HitShape, len: f64 },
    Kill { id: i64, kind: EnemyKind, x: f64, y: f64, z: f64, captain: bool },
    Vanish { id: i64, kind: EnemyKind, x: f64, z: f64 },
    Hurt { dmg: f64, x: f64, z: f64, big: bool },
    EnemyWindup { id: i64 },
    EnemyStrike { id: i64, x: f64, z: f64 },
    Throw { id: i64 },
    Jump,
    Land { heavy: bool },
    Dodge,
    RanbuStart,
    RanbuFinish,
    RanbuReady,
    CaptainAppear { id: i64, hold: i32 },
    /// fled: 逃げた守りの数 / to: 逃げ先の砦（-1 = 逃げ先なし）
    Capture { hold: i32, left: i32, fled: i32, to: i32 },
    BossGate,
    BossEnter,
    BossAttack { kind: BossState },
    BossStomp { x: f64, z: f64 },
    BossDown,
    BossDefeated,
    BossLand { x: f64, z: f64 },
    Milestone { kos: i64 },
    Pickup { kind: PickupKind, x: f64, z: f64 },
    /// tel は当たりが出た瞬間の写し（TS は生きた参照で、このあと done が true になる）
    TelegraphFire { tel: Telegraph },
    Victory,
    Defeat,
    Combo { n: i64 },
    /// 群衆ドミノ: 弾にぶつかって敵が飛んだ（n は今の連鎖の数）
    Chain { chain: i64, n: i64, victim: i64, x: f64, y: f64, z: f64, heavy: bool, ax: f64, ay: f64, az: f64 },
    /// 連鎖が終わった: 巻き込んだ数・そのうち倒した数・連鎖の得点・網の中心
    ChainEnd { chain: i64, n: i64, kills: i64, bonus: f64, x: f64, z: f64 },
    /// 乱舞の締めで金の印が爆発した（i は近い順）
    Detonate { x: f64, z: f64, i: i32 },
    /// 重騎の弾で隊長が崩れた
    CaptainBreak { id: i64, x: f64, z: f64 },
    /// 重騎の突進を回避ですり抜けて担いだ / 担いだ重騎を投げた
    Grab { id: i64 },
    Hurl { id: i64, x: f64, z: f64 },
    HurlBoom { id: i64, x: f64, z: f64 },
    GiantForm { x: f64, z: f64 },
    GiantArm { side: i32, x: f64, z: f64 },
    GiantFall { x: f64, z: f64 },
    RankUp { rank: i32 },
    Reflect { x: f64, y: f64, z: f64, mirror: i32 },
    /// 飴糸を撃った（糸の先。target = 大きな敵へ飛びつく）
    Wire { x: f64, y: f64, z: f64, target: bool },
    /// 飴糸で着いて斬った
    /// 糸斬り。chain = 何回目か（1 猪・2 鹿・3 蝶）
    WireStrike { x: f64, y: f64, z: f64, boss: bool, chain: u8 },
    /// 糸斬りを 3 回続けた（猪鹿蝶）
    Inoshikacho { x: f64, y: f64, z: f64 },
    /// 飴が 1 個増えた（count = 増えたあとの数）／飴が無くて飴糸が出なかった
    Ame { count: i32 },
    NoAme,
    /// わたあめ雲に乗った / 降りた（melted = 時間切れで溶けた）
    Ride,
    RideEnd { melted: bool },
}

pub const FIELD_R: f64 = 86.0;
#[derive(Clone, Copy, Debug)]
pub struct Point { pub x: f64, pub z: f64 }
pub const GATE: Point = Point { x: 0.0, z: -66.0 };
pub const START: Point = Point { x: 0.0, z: 62.0 };
/// 飴鏡（反射壁）: 中心・沿う向き a（rad。沿う向きは (sin a, cos a)）・長さ。出陣の地点から北への大通りの両脇に斜めに立つ
#[derive(Clone, Copy, Debug)]
pub struct Mirror { pub x: f64, pub z: f64, pub a: f64, pub len: f64 }
pub const MIRRORS: [Mirror; 2] = [
    Mirror { x: -12.0, z: 33.0, a: -0.55, len: 12.0 },
    Mirror { x: 12.0, z: 33.0, a: 0.55, len: 12.0 },
];
/// 飴鏡の厚みの半分・高さ・跳ね返りの勢い・1つの弾が跳ね返れる回数
pub const MIRROR_T: f64 = 0.35;
pub const MIRROR_H: f64 = 3.0;
pub const MIRROR_KEEP: f64 = 0.85;
pub const MIRROR_MAX: i32 = 2;
const MAX_ALIVE: i64 = 260;
pub const GRAVITY: f64 = 26.0;
/// 跳ぶ速さ（m/s）と回避の長さ（秒）。見た目の部品へ渡す合図も同じ値を使う（hero_signals.rs）
pub const JUMP_V: f64 = 10.5;
pub const DODGE_T: f64 = 0.38;
const MILESTONES: [i64; 9] = [50, 100, 200, 300, 500, 700, 1000, 1500, 2000];

/// 決まった乱数（同じ種なら同じ戦い）。mulberry32
#[derive(Clone, Debug)]
pub struct Rng { s: u32 }
impl Rng {
    #[inline]
    pub fn next(&mut self) -> f64 {
        self.s = self.s.wrapping_add(0x6d2b79f5);
        let mut t = self.s;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        (t ^ (t >> 14)) as f64 / 4294967296.0
    }
    pub fn state(&self) -> u32 { self.s }
}
pub fn rng(seed: u32) -> Rng { Rng { s: seed } }

#[inline]
fn ang_diff(a: f64, b: f64) -> f64 {
    let mut d = (a - b) % (PI * 2.0);
    if d > PI { d -= PI * 2.0; }
    if d < -PI { d += PI * 2.0; }
    d
}
#[inline] fn clamp(v: f64, a: f64, b: f64) -> f64 { if v < a { a } else if v > b { b } else { v } }

struct MirrorHit { i: i32, nx: f64, nz: f64, depth: f64 }
/// 半径 r の円が飴鏡に食い込んでいれば、その鏡と押し出す向き（法線・符号つき）と食い込み量
#[inline]
fn mirror_contact(x: f64, z: f64, r: f64) -> Option<MirrorHit> {
    for (i, m) in MIRRORS.iter().enumerate() {
        let (ux, uz) = (sin(m.a), cos(m.a));
        let (nx, nz) = (uz, -ux);
        let (dx, dz) = (x - m.x, z - m.z);
        let (u, v) = (dx * ux + dz * uz, dx * nx + dz * nz);
        if u.abs() > m.len / 2.0 + r || v.abs() > MIRROR_T + r { continue; }
        let sg = if v >= 0.0 { 1.0 } else { -1.0 };
        return Some(MirrorHit { i: i as i32, nx: nx * sg, nz: nz * sg, depth: MIRROR_T + r - v.abs() });
    }
    None
}

// ---- 空間の格子（近くの敵を速く探す） ----
const CELL: f64 = 4.0;
/// Math.ceil((FIELD_R * 2 + 20) / CELL)
const GW: i64 = 48;
/// TS の Map と同じ意味の格子。キーは同じ整数（衝突も同じ）。ふつうの範囲は平らな Vec、外れたキーは extra に置く
const KEY_MIN: i64 = -GW * 16;
const KEY_COUNT: usize = (GW * 80) as usize;
struct Grid { cells: Vec<Vec<u32>>, used: Vec<u32>, extra: Vec<(i64, Vec<u32>)> }
impl Grid {
    fn new() -> Self { Grid { cells: (0..KEY_COUNT).map(|_| Vec::new()).collect(), used: Vec::new(), extra: Vec::new() } }
    #[inline]
    fn get(&self, k: i64) -> Option<&Vec<u32>> {
        let o = k - KEY_MIN;
        if o >= 0 && (o as usize) < KEY_COUNT { Some(&self.cells[o as usize]) } else { self.extra.iter().find(|c| c.0 == k).map(|c| &c.1) }
    }
    #[inline]
    fn push(&mut self, k: i64, e: u32) {
        let o = k - KEY_MIN;
        if o >= 0 && (o as usize) < KEY_COUNT {
            let c = &mut self.cells[o as usize];
            if c.is_empty() { self.used.push(o as u32); }
            c.push(e);
        } else if let Some(c) = self.extra.iter_mut().find(|c| c.0 == k) { c.1.push(e); } else { self.extra.push((k, vec![e])); }
    }
    fn clear(&mut self) {
        for i in 0..self.used.len() { let o = self.used[i] as usize; self.cells[o].clear(); }
        self.used.clear();
        for c in &mut self.extra { c.1.clear(); }
    }
}
fn near_into(grid: &Grid, enemies: &[Enemy], x: f64, z: f64, r: f64, out: &mut Vec<u32>) {
    out.clear();
    let x0 = ((x - r + FIELD_R + 10.0) / CELL).floor();
    let x1 = ((x + r + FIELD_R + 10.0) / CELL).floor();
    let z0 = ((z - r + FIELD_R + 10.0) / CELL).floor();
    let z1 = ((z + r + FIELD_R + 10.0) / CELL).floor();
    if x0.is_nan() || x1.is_nan() || z0.is_nan() || z1.is_nan() { return; }
    let (x0, x1, z0, z1) = (x0 as i64, x1 as i64, z0 as i64, z1 as i64);
    let lim = (r + 1.5) * (r + 1.5);
    let mut gx = x0;
    while gx <= x1 {
        let mut gz = z0;
        while gz <= z1 {
            if let Some(c) = grid.get(gx * GW + gz) {
                for &i in c {
                    let e = &enemies[i as usize];
                    let (dx, dz) = (e.x - x, e.z - z);
                    if dx * dx + dz * dz <= lim { out.push(i); }
                }
            }
            gz += 1;
        }
        gx += 1;
    }
}

#[derive(Clone, Copy, Debug)]
struct Leap { x0: f64, z0: f64, x1: f64, z1: f64, landed: bool }

/// 巨人: 体力が半分を切ると雑兵が積み重なってできる（1戦に一度）。slots は GIANT_SLOTS と同じ並び
#[derive(Clone, Debug)]
pub struct Giant {
    pub on: bool, pub formed: bool, pub t: f64,
    /// enemies のインデックス
    pub slots: Vec<Option<usize>>,
    pub arm_gone: [bool; 2], pub punch_fire: f64,
    /// 積み上がる前の場所（slots と同じ並び。TS の Map<Enemy, …>）
    pub from: Vec<[f64; 3]>,
    /// 飴糸で巨人の上の大将を斬った回数（WIRE_TOPPLE 回で崩れる）
    pub wire_hits: i32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats { pub captures: i64, pub boss_time: f64, pub start_boss_at: f64, pub peak_alive: i64 }

#[derive(Clone, Copy, Debug)]
struct Chain { id: i64, n: i64, t: f64, kills: i64, sx: f64, sz: f64 }
/// 乱舞で起爆できる金の印（巻き込まれた敵。倒れたらその場所）
#[derive(Clone, Copy, Debug)]
struct Mark { uid: u64, id: i64, e: usize, x: f64, z: f64, age: f64, born: f64 }

#[derive(Clone, Debug)]
pub struct BattleResult {
    pub score: f64, pub rank: &'static str, pub time: f64, pub kos: i64, pub max_combo: i64, pub max_chain: i64, pub chain_bonus: f64,
    pub captures: i64, pub dmg_taken: f64, pub continues: i64, pub won: bool,
    /// 誉れ（武将ごとに集める目標）
    pub honors: [bool; 5],
    pub rank_reached: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase { Battle, Boss, Victory, Defeat }
impl Phase { pub fn name(self) -> &'static str { match self { Phase::Battle => "battle", Phase::Boss => "boss", Phase::Victory => "victory", Phase::Defeat => "defeat" } } }

const GRUNT_KINDS: [EnemyKind; 5] = [EnemyKind::Strawberry, EnemyKind::Macaron, EnemyKind::Mikan, EnemyKind::Creampuff, EnemyKind::Cottoncandy];
const GRUNTS: [EnemyKind; 5] = GRUNT_KINDS;
const HOLD_KIND: [EnemyKind; 4] = [EnemyKind::Strawberry, EnemyKind::Mikan, EnemyKind::Macaron, EnemyKind::Cottoncandy];
const LEAP_T: f64 = 1.3;
const FLY_MIN: f64 = 6.0;
const CHAIN_MAX_GEN: i32 = 9;
const MARK_LIFE: f64 = 4.0;
/// detonate の「爆発の写し」を指すインデックス（TS は一時オブジェクト）
const BLAST: usize = usize::MAX;

pub struct Battle {
    pub player: Player,
    pub enemies: Vec<Enemy>,
    pub holds: Vec<Stronghold>,
    pub boss: Boss,
    pub telegraphs: Vec<Telegraph>,
    pub projectiles: Vec<Projectile>,
    pub pickups: Vec<Pickup>,
    pub events: Vec<BattleEvent>,
    pub stats: Stats,
    pub time: f64,
    pub phase: Phase,
    pub gate_open: bool,
    /// 大将の跳躍の高さ（見た目用）
    pub boss_y: f64,
    pub hitstop: f64,
    pub ranbu_freeze: f64,
    /// 巨人（on = 立っている）
    pub giant: Giant,
    /// 武器の弾道
    pub path: Path,
    /// 武勲（画面には出さない）
    pub merit: i64,
    pub continues: i64,
    pub victory_t: f64,

    rand: Rng,
    god_mode: bool,
    next_id: i64,
    leap: Leap,
    debug_hits: i64,
    slow: f64,
    slow_scale: f64,
    reinforce_t: f64,
    milestone_i: usize,
    ranbu_ready_sent: bool,
    // 格: 武勲（撃破・大連鎖・隊長崩し・砦）で上がる。撃破は 1 フレームごとに撃破数の差で足す
    merit_kos: i64,
    big_chain_merits: i32,
    broke_captains: Vec<i64>,
    grid: Grid,
    tmp: Vec<u32>,
    tmp2: Vec<u32>,
    targets: Vec<u32>,
    /// いまの技の前に、通常攻撃で何回寄せたか（0 = 集めていない。六の太刀は 6）
    gathered: i32,
    chains: Vec<Chain>,
    chain_clock: f64,
    max_chain: i64,
    chain_bonus: f64,
    debug_chain: i64,
    boost_hit: bool,
    boost_pow: i32,
    marks: Vec<Mark>,
    mark_uid: u64,
    /// 乱舞を始めた時刻（それより前の印だけを起爆する。乱舞そのものの連鎖では起爆しない）
    ranbu_at: f64,
    /// detonate の爆発の写し
    blast: Enemy,
    attackers: i32,
    /// いまの当たりが飴糸の糸斬りか（巨人の上の大将への傷を減らさない）
    wire_strike: bool,
}

fn blank_enemy() -> Enemy {
    Enemy {
        id: 0, kind: EnemyKind::Strawberry, role: Role::Grunt, alive: false, x: 0.0, z: 0.0, y: 0.0, vx: 0.0, vz: 0.0, vy: 0.0, face: 0.0, hp: 0.0, max_hp: 0.0,
        state: EnemyState::Idle, st: 0.0, cd: 0.0, spin: 0.0, spin_v: 0.0, hit_t: 9.0, token: false, home: -1, awake: false, aim_x: 0.0, aim_z: 0.0, poise: 0.0, dead_t: 0.0, skill: EnemySkill::None, last_hit_key: -1,
        fling: 0, gen: 0, ammo: 0, chain_hit: 0, chain_t: 9.0, boost: false, break_cd: 0.0, thrown: false, flee: -1, mirror: 0, curve: 0.0,
    }
}

/// 連鎖の得点: 10 体を超えた分ほど大きい
pub fn chain_score(n: i64) -> f64 { if n >= 10 { math::round(math::pow((n - 9) as f64, 1.5) * 14.0) } else { 0.0 } }
#[inline] fn heavy_ammo(e: &Enemy) -> bool { e.role == Role::Brute || e.boost }

pub fn create_battle(seed: u32, god_mode: bool) -> Battle {
    let hold = |id, name, x, z| Stronghold { id, name, x, z, r: 11.0, owner: Owner::Enemy, captain: -1, spawn_t: 2.0, alarm: false };
    let mut b = Battle {
        player: Player {
            x: START.x, z: START.z, y: 0.0, vy: 0.0, vx: 0.0, vz: 0.0, face: PI, hp: 1200.0, max_hp: 1200.0, ranbu: 30.0, state: PlayerState::Free, st: 0.0,
            r#move: None, move_t: 0.0, chain: 0, queued: None, air: false, invuln: 0.0, hurt_t: 0.0, kos: 0, combo: 0, combo_t: 0.0, max_combo: 0,
            dmg_taken: 0.0, move_key: 0, dodge_cd: 0.0, speed: 0.0, carry: 0, carry_t: 0.0, rank: 0,
            wire: Wire { x0: 0.0, y0: 0.0, z0: 0.0, ax: 0.0, ay: 0.0, az: 0.0, dur: 0.0, target: 0, esc: false, flip: false, swing: 0.0, slowed: false }, wire_cd: 0.0, ame: 0, ame_fill: 0, wire_chain: 0, wire_chain_t: 0.0, wire_esc: 0.0, wire_last: 0, ride_cd: 0.0,
        },
        enemies: Vec::new(),
        holds: vec![hold(0, "苺の砦", -46.0, 30.0), hold(1, "蜜柑の砦", 48.0, 26.0), hold(2, "飴の砦", -50.0, -28.0), hold(3, "綿菓子の砦", 50.0, -32.0)],
        boss: Boss { x: GATE.x, z: GATE.z - 6.0, face: 0.0, hp: 11000.0, max_hp: 11000.0, state: BossState::Dormant, st: 0.0, cd: 2.0, poise: 1400.0, hit_t: 9.0, combo: 0, next: 0, last_hit_key: -1 },
        telegraphs: Vec::new(), projectiles: Vec::new(), pickups: Vec::new(), events: Vec::new(), stats: Stats::default(),
        time: 0.0, phase: Phase::Battle, gate_open: false, boss_y: 0.0, hitstop: 0.0, ranbu_freeze: 0.0,
        giant: Giant { on: false, formed: false, t: 0.0, slots: Vec::new(), arm_gone: [false, false], punch_fire: 0.95, from: Vec::new(), wire_hits: 0 },
        path: Path::None, merit: 0, continues: 0, victory_t: -1.0,
        rand: rng(seed), god_mode, next_id: 1, leap: Leap { x0: 0.0, z0: 0.0, x1: 0.0, z1: 0.0, landed: false }, debug_hits: 0, slow: 0.0, slow_scale: 1.0, reinforce_t: 14.0,
        milestone_i: 0, ranbu_ready_sent: false, merit_kos: 0, big_chain_merits: 0, broke_captains: Vec::new(),
        grid: Grid::new(), tmp: Vec::new(), tmp2: Vec::new(), targets: Vec::new(), gathered: 0,
        chains: Vec::new(), chain_clock: 0.0, max_chain: 0, chain_bonus: 0.0, debug_chain: 1 << 20, boost_hit: false, boost_pow: 0, marks: Vec::new(), mark_uid: 0, ranbu_at: -1.0, blast: blank_enemy(), attackers: 0, wire_strike: false,
    };
    // 初期の配置: 砦の守り・野の部隊
    for i in 0..b.holds.len() { let (x, z, id) = (b.holds[i].x, b.holds[i].z, b.holds[i].id); b.squad(x, z, 26, id, false, Some(HOLD_KIND[id as usize]), false); }
    let field_squads: [(f64, f64); 9] = [(0.0, 40.0), (-24.0, 46.0), (24.0, 44.0), (-30.0, 6.0), (30.0, 2.0), (0.0, 10.0), (-12.0, -20.0), (14.0, -24.0), (0.0, -44.0)];
    for (x, z) in field_squads { b.squad(x, z, 14, -1, false, None, false); }
    // 先鋒: 出陣してすぐ、正面から群れが押し寄せる（最初の数秒を静かにしない）
    b.squad(START.x, START.z - 16.0, 34, -1, true, None, true);
    b
}

impl Battle {
    #[inline] fn r(&mut self, a: f64, b: f64) -> f64 { a + (b - a) * self.rand.next() }
    #[inline] fn emit(&mut self, e: BattleEvent) { self.events.push(e); }

    #[inline] fn new_id(&mut self) -> i64 { let v = self.next_id; self.next_id += 1; v }
    #[inline] fn near_tmp(&mut self, x: f64, z: f64, r: f64) { near_into(&self.grid, &self.enemies, x, z, r, &mut self.tmp); }
    #[inline] fn near_tmp2(&mut self, x: f64, z: f64, r: f64) { near_into(&self.grid, &self.enemies, x, z, r, &mut self.tmp2); }
    /// 弾の参照（敵か、detonate の爆発の写し）
    #[inline] fn en(&self, a: usize) -> &Enemy { if a == BLAST { &self.blast } else { &self.enemies[a] } }
    #[inline] fn en_mut(&mut self, a: usize) -> &mut Enemy { if a == BLAST { &mut self.blast } else { &mut self.enemies[a] } }

    fn add_merit(&mut self, n: i64) {
        self.merit += n;
        while (self.player.rank as usize) < RANK_AT.len() && self.merit >= RANK_AT[self.player.rank as usize] {
            self.player.rank += 1;
            if self.player.rank == 3 { self.player.ranbu = math::min(100.0, self.player.ranbu + 50.0); }
            let rank = self.player.rank;
            self.emit(BattleEvent::RankUp { rank });
        }
    }
    /// 格 参は乱舞ゲージの溜まりが 1.5 倍
    #[inline] fn gain(&self, n: f64) -> f64 { n * if self.player.rank >= 3 { 1.5 } else { 1.0 } }
    /// 育つ武器: 武器の大きさ（1 = 元の大きさ）。武勲でなめらかに育ち、参で止まる
    pub fn weapon_grow(&self) -> f64 { Self::grow_of(self.merit) }
    fn grow_of(merit: i64) -> f64 {
        let m = merit as f64;
        let mut lo = 0.0;
        for i in 0..RANK_AT.len() {
            let hi = RANK_AT[i] as f64;
            if m < hi { return GROW_AT[i] + (GROW_AT[i + 1] - GROW_AT[i]) * (m - lo) / (hi - lo); }
            lo = hi;
        }
        GROW_AT[RANK_AT.len()]
    }
    /// 通常攻撃の当たる範囲の倍率（武器の大きさに合わせる。見た目の弧も同じ）
    pub fn weapon_reach(&self) -> f64 { 1.0 + (self.weapon_grow() - 1.0) * 0.5 }

    // ---- 敵の生成 ----
    fn spawn(&mut self, kind: EnemyKind, x: f64, z: f64, home: i32, awake: bool) -> usize {
        let d = kind.def();
        let i = match self.enemies.iter().position(|o| !o.alive && o.dead_t > 9.0) {
            Some(i) => i,
            None => { self.enemies.push(blank_enemy()); self.enemies.len() - 1 }
        };
        let id = self.new_id();
        let face = self.r(0.0, PI * 2.0);
        let st = self.r(0.0, 1.0);
        let cd = self.r(1.0, 3.0);
        self.enemies[i] = Enemy {
            id, kind, role: d.role, alive: true, x, z, y: 0.0, vx: 0.0, vz: 0.0, vy: 0.0, face, hp: d.hp, max_hp: d.hp,
            state: if awake { EnemyState::Chase } else { EnemyState::Idle }, st, cd, spin: 0.0, spin_v: 0.0, hit_t: 9.0, token: false, home, awake,
            aim_x: 0.0, aim_z: 0.0, poise: if d.role == Role::Captain { 320.0 } else if d.role == Role::Brute { 80.0 } else { 0.0 }, dead_t: 0.0, skill: EnemySkill::None, last_hit_key: -1,
            fling: 0, gen: 0, ammo: 0, chain_hit: 0, chain_t: 9.0, boost: false, break_cd: 0.0, thrown: false, flee: -1, mirror: 0, curve: 0.0,
        };
        i
    }
    fn squad(&mut self, cx: f64, cz: f64, n: i32, home: i32, awake: bool, main: Option<EnemyKind>, grunts_only: bool) {
        for _ in 0..n {
            let a = self.r(0.0, PI * 2.0);
            let r = self.rand.next().sqrt() * (2.5 + n as f64 * 0.35);
            let roll = if grunts_only { 0.2 + self.rand.next() * 0.8 } else { self.rand.next() };
            let kind = if roll < 0.1 { EnemyKind::Donut } else if roll < 0.17 { EnemyKind::Pudding } else if main.is_some() && roll < 0.7 { main.unwrap() } else { GRUNTS[(self.rand.next() * GRUNTS.len() as f64).floor() as usize] };
            self.spawn(kind, cx + cos(a) * r, cz + sin(a) * r, home, awake);
        }
    }
    pub fn alive_count(&self) -> i64 { let mut n = 0; for e in &self.enemies { if e.alive { n += 1; } } n }

    fn rebuild(&mut self) {
        self.grid.clear();
        for i in 0..self.enemies.len() {
            let e = &self.enemies[i];
            if !e.alive { continue; }
            let kx = ((e.x + FIELD_R + 10.0) / CELL).floor();
            let kz = ((e.z + FIELD_R + 10.0) / CELL).floor();
            // NaN のキーは TS でも near から引かれない
            if kx.is_nan() || kz.is_nan() { continue; }
            self.grid.push(kx as i64 * GW + kz as i64, i as u32);
        }
    }

    // ---- 主役の技 ----
    fn start_move(&mut self, m: &'static Move, input: &Input, from_chain: i32) {
        self.gathered = from_chain;
        { let p = &mut self.player; p.r#move = Some(m); p.move_t = 0.0; p.state = PlayerState::Attack; p.move_key += 1; }
        let (px, pz) = (self.player.x, self.player.z);
        // 狙い: 入力の向き（無ければ正面）から 75° 以内の、いちばん近い敵へ向く
        let want = if hypot(input.mx, input.mz) > 0.2 { atan2(input.mx, input.mz) } else { self.player.face };
        let mut best: Option<(f64, f64)> = None;
        let mut bd = 1e9;
        self.near_tmp(px, pz, 7.0);
        for k in 0..self.tmp.len() {
            let e = &self.enemies[self.tmp[k] as usize];
            if !e.alive || e.state == EnemyState::Dead { continue; }
            let (dx, dz) = (e.x - px, e.z - pz);
            let d = hypot(dx, dz);
            if d > 7.0 || ang_diff(atan2(dx, dz), want).abs() > 75.0 * PI / 180.0 { continue; }
            let score = d + ang_diff(atan2(dx, dz), want).abs() * 2.0;
            if score < bd { bd = score; best = Some((e.x, e.z)); }
        }
        if self.boss.state != BossState::Dormant && self.boss.state != BossState::Dead {
            let (dx, dz) = (self.boss.x - px, self.boss.z - pz);
            let d = hypot(dx, dz) - 3.0;
            if d < 7.0 && ang_diff(atan2(dx, dz), want).abs() < 80.0 * PI / 180.0 && d < bd { best = Some((self.boss.x, self.boss.z)); }
        }
        self.player.face = match best { Some((x, z)) => atan2(x - px, z - pz), None => want };
        self.emit(BattleEvent::Move { r#move: m });
    }

    fn hit_enemy(&mut self, i: usize, h: &Hit, dir_x: f64, dir_z: f64, move_key: i64) -> bool {
        let key = move_key * 64 + math::round(h.t * 50.0) as i64;
        if self.enemies[i].last_hit_key == key { return false; }
        self.enemies[i].last_hit_key = key;
        if self.enemies[i].state == EnemyState::Stacked { return self.hit_stacked(i, h.dmg, dir_x, dir_z); }
        let e = &mut self.enemies[i];
        let captain = e.role == Role::Captain;
        e.hp -= h.dmg * if e.state == EnemyState::Down { 1.3 } else { 1.0 };
        e.hit_t = 0.0;
        e.awake = true;
        if e.token { e.token = false; }
        let (mut kb, mut power, mut armored) = (h.kb, h.power, false);
        if captain || e.role == Role::Brute {
            e.poise -= h.dmg;
            if e.poise > 0.0 && e.hp > 0.0 { kb = Knock::Push; power = h.power * 0.15; armored = true; }
            else if e.poise <= 0.0 { e.poise = if captain { 320.0 } else { 80.0 }; }
        }
        let in_air = e.y > 0.05 || e.state == EnemyState::Air;
        if e.hp <= 0.0 {
            e.state = EnemyState::Dead; e.st = 0.0;
            // 隊長は飛ばさず、その場で崩れて弾ける（カメラへ飛んでこないように）
            let p = if captain { 0.12 } else { 1.0 };
            let pw = math::max(power, 9.0) * p;
            let vx = dir_x * pw * self.r(0.8, 1.3) + self.r(-2.0, 2.0);
            let vz = dir_z * pw * self.r(0.8, 1.3) + self.r(-2.0, 2.0);
            let vy0 = self.enemies[i].vy;
            let vy = math::max(vy0, self.r(6.0, 11.0) * (if kb == Knock::Blast || kb == Knock::Launch { 1.25 } else { 0.9 }) * p);
            let spin = self.r(8.0, 16.0);
            let sign = if self.rand.next() < 0.5 { -1.0 } else { 1.0 };
            let e = &mut self.enemies[i];
            e.vx = vx; e.vz = vz; e.vy = vy;
            e.spin_v = spin * sign * if captain { 0.15 } else { 1.0 };
            self.arm(i, kb, move_key);
            self.player.kos += 1; self.gain_ame(self.enemies[i].kind);
            let e = &self.enemies[i];
            let ev = BattleEvent::Kill { id: e.id, kind: e.kind, x: e.x, y: e.y, z: e.z, captain };
            self.emit(ev);
            self.drop_from(i);
            if captain { self.on_captain_down(i); }
            return true;
        }
        match kb {
            Knock::Push => {
                e.vx = dir_x * power; e.vz = dir_z * power;
                // 空中の追い打ちで浮かせ続けられるのは雑兵だけ。隊長・重騎（体勢が崩れていない）は勢いを戻さず落ちる
                if in_air { if !armored { e.vy = math::max(e.vy, 4.5); } e.state = EnemyState::Air; } else { e.state = EnemyState::Stagger; e.st = 0.0; }
            }
            Knock::Gather => {
                // 主役の正面へ寄せて固める（ドミノの弾込め）
                let (pl_x, pl_z, pl_face) = (self.player.x, self.player.z, self.player.face);
                let (gx, gz) = (pl_x + sin(pl_face) * 2.8 - e.x, pl_z + cos(pl_face) * 2.8 - e.z);
                let gd = or1(hypot(gx, gz));
                let v = math::min(power * 1.4, gd * 5.0);
                e.vx = gx / gd * v; e.vz = gz / gd * v;
                if in_air { e.vy = math::max(e.vy, 4.5); e.state = EnemyState::Air; } else { e.state = EnemyState::Stagger; e.st = 0.0; }
            }
            Knock::Launch => {
                let vy = power * self.r(0.9, 1.1);
                let sv = { self.enemies[i].vy = vy; self.enemies[i].vx = dir_x * 1.6; self.enemies[i].vz = dir_z * 1.6; self.enemies[i].state = EnemyState::Air; self.r(-6.0, 6.0) };
                self.enemies[i].spin_v = sv;
            }
            Knock::Spin => {
                // まわりの敵を巻き込む: 接線の向きへ
                e.vx = (dir_x * 0.7 - dir_z * 0.7) * power; e.vz = (dir_z * 0.7 + dir_x * 0.7) * power;
                if in_air { e.vy = math::max(e.vy, 5.0); e.state = EnemyState::Air; } else { e.state = EnemyState::Stagger; e.st = 0.0; }
            }
            Knock::Down => {
                if in_air { e.vy = -14.0; } else { e.state = EnemyState::Down; e.st = 0.0; e.vx = dir_x * power; e.vz = dir_z * power; }
            }
            Knock::Blast => {
                e.vx = dir_x * power; e.vz = dir_z * power; e.vy = power * 0.62; e.state = EnemyState::Air;
                let sv = self.r(8.0, 14.0);
                self.enemies[i].spin_v = sv;
            }
        }
        if kb != Knock::Down || in_air { self.arm(i, kb, move_key); }
        false
    }

    // ---- 群衆ドミノ ----
    // 強く飛ばされた敵は弾になる。低く速く飛ぶ間は、ぶつかった敵を同じ向きへ飛ばす（その敵も弾になる）。
    // 高く落ちた弾は、着地点のまわりを弾き飛ばす。雑兵の弾は一体ごとに勢いをほとんど渡し、重騎の弾は突き抜ける。
    /// 弾が巻き込める数（遠鳴りで雑兵の弾が増える）
    #[inline] fn ammo_of(&self, a: usize) -> i32 { if heavy_ammo(self.en(a)) { 7 } else { 2 } }
    fn arm(&mut self, i: usize, kb: Knock, chain: i64) {
        if self.enemies[i].role == Role::Captain || kb == Knock::Push || kb == Knock::Gather { return; }
        // 集めて撃った一撃: 重騎なみに突き抜け、勢いも強い（連鎖が遠くまで届く）。長く寄せたほど速く、多く巻き込む
        let k = (if self.boost_hit { 1.2 + 0.07 * self.boost_pow as f64 } else { 1.0 }) * if self.player.rank >= 2 { RANK_FLING } else { 1.0 };
        let boost_hit = self.boost_hit;
        { let e = &mut self.enemies[i]; e.vx *= k; e.vz *= k; e.boost = boost_hit; e.fling = chain; e.gen = 0; }
        let ammo = if boost_hit { 5 + self.boost_pow } else { self.ammo_of(i) };
        { let e = &mut self.enemies[i]; e.ammo = ammo; e.chain_hit = chain; e.chain_t = 0.0; e.mirror = 0; e.curve = 0.0; }
        // 武器の弾道: 強攻撃で直接飛ばした敵だけ。勢いの合計は変えず、向きを変える（直進だけ少し速い）
        if self.path != Path::None {
            if let Some(m) = self.player.r#move {
                if is_charge(m) {
                    let face = self.player.face;
                    let path = self.path;
                    let e = &mut self.enemies[i];
                    let sp = hypot(e.vx, e.vz);
                    let mut a = atan2(e.vx, e.vz);
                    let mut k = 1.0;
                    if path == Path::Straight { a = face; k = STRAIGHT_SPEED; }
                    else if path == Path::Spread { a = face + clamp(ang_diff(a, face) * SPREAD_K, -SPREAD_MAX, SPREAD_MAX); }
                    else { a += CURVE_START; e.curve = CURVE_RATE; }
                    e.vx = sin(a) * sp * k; e.vz = cos(a) * sp * k;
                }
            }
        }
    }
    fn chain_hit(&mut self, v: usize, a: usize, dx0: f64, dz0: f64, speed: f64) {
        let id = self.en(a).fling;
        let heavy = heavy_ammo(self.en(a));
        let dl = or1(hypot(dx0, dz0));
        let (dir_x, dir_z) = (dx0 / dl, dz0 / dl);
        { let ve = &mut self.enemies[v]; ve.chain_hit = id; ve.chain_t = 0.0; ve.awake = true; ve.hit_t = 0.0; ve.token = false; }
        let (vid, vxp, vzp) = (self.enemies[v].id, self.enemies[v].x, self.enemies[v].z);
        let clock = self.chain_clock;
        if let Some(mk) = self.marks.iter_mut().find(|m| m.id == vid && m.e == v) { mk.age = 0.0; mk.born = clock; }
        else {
            if self.marks.len() >= 80 { self.marks.remove(0); }
            let uid = self.mark_uid; self.mark_uid += 1;
            self.marks.push(Mark { uid, id: vid, e: v, x: vxp, z: vzp, age: 0.0, born: clock });
        }
        let dmg = 22.0 + (if heavy { 40.0 } else { 20.0 }) * math::min(1.6, speed / 12.0);
        self.enemies[v].hp -= dmg;
        let ci = match self.chains.iter().position(|c| c.id == id) {
            Some(i) => i,
            None => { self.chains.push(Chain { id, n: 0, t: 0.0, kills: 0, sx: 0.0, sz: 0.0 }); self.chains.len() - 1 }
        };
        { let c = &mut self.chains[ci]; c.n += 1; c.t = clock; c.sx += vxp; c.sz += vzp; }
        let cn = self.chains[ci].n;
        if cn > self.max_chain { self.max_chain = cn; }
        // 節目の間: 10・20・30…体目でほんの一瞬スロー（ほかのスローの最中は重ねない）
        if cn % 10 == 0 && self.slow <= 0.0 { self.slow = 0.14; self.slow_scale = 0.3; }
        {
            let ve = &self.enemies[v];
            let ae = self.en(a);
            let ev = BattleEvent::Chain { chain: id, n: cn, victim: vid, x: ve.x, y: ve.y + ve.kind.def().height * 0.5, z: ve.z, heavy, ax: ae.x, ay: ae.y + ae.kind.def().height * 0.5, az: ae.z };
            self.emit(ev);
        }
        let inc = self.gain(0.12);
        { let p = &mut self.player; p.combo += 1; p.combo_t = 0.0; if p.combo > p.max_combo { p.max_combo = p.combo; } p.ranbu = math::min(100.0, p.ranbu + inc); }
        // 隊長と、雑兵の弾を受けた重騎は止める（壁になる）
        // 巨人に積み重なった雑兵も、雑兵の弾では外れない（削れるだけ）。重い弾なら外れて飛ぶ
        let (vrole, vstate) = (self.enemies[v].role, self.enemies[v].state);
        let wall = vrole == Role::Captain || (vrole == Role::Brute && !heavy) || (vstate == EnemyState::Stacked && !heavy);
        if wall && vrole == Role::Captain && self.en(a).role == Role::Brute && self.enemies[v].break_cd <= 0.0 { self.captain_break(v, a); return; }
        if wall {
            self.enemies[v].poise -= dmg;
            { let ae = self.en_mut(a); ae.ammo = 0; ae.vx *= -0.25; ae.vz *= -0.25; }
            if self.enemies[v].hp <= 0.0 && vrole != Role::Captain { self.kill_by_chain(v, dir_x, dir_z, speed * 0.4, ci); }
            return;
        }
        let out = speed * if heavy { 0.9 } else { 0.8 };
        let spread = self.r(-0.42, 0.42);
        let (cs, sn) = (cos(spread), sin(spread));
        let (ox, oz) = (dir_x * cs - dir_z * sn, dir_x * sn + dir_z * cs);
        let vy = self.r(5.0, 7.5);
        let sv = self.r(8.0, 14.0) * if self.rand.next() < 0.5 { -1.0 } else { 1.0 };
        { let ve = &mut self.enemies[v]; ve.vx = ox * out; ve.vz = oz * out; ve.vy = vy; ve.spin_v = sv; }
        if self.enemies[v].hp <= 0.0 { self.kill_by_chain(v, ox, oz, out, ci); } else { let ve = &mut self.enemies[v]; ve.state = EnemyState::Air; ve.st = 0.0; }
        if out >= FLY_MIN && self.en(a).gen < CHAIN_MAX_GEN {
            let gen = self.en(a).gen + 1;
            { let ve = &mut self.enemies[v]; ve.boost = false; ve.fling = id; ve.gen = gen; }
            let ammo = self.ammo_of(v);
            let ve = &mut self.enemies[v]; ve.ammo = ammo; ve.mirror = 0; ve.curve = 0.0;
        }
        let keep = if heavy { 0.94 } else { 0.55 };
        let ae = self.en_mut(a);
        ae.ammo -= 1;
        ae.vx *= keep; ae.vz *= keep;
        if ae.ammo <= 0 { ae.fling = 0; }
    }
    /// 投げた重騎の着地: その場で爆ぜて、まわり 4.5m の敵を同じ連鎖の弾として吹き飛ばし、重騎も倒れる
    fn thrown_boom(&mut self, a: usize) {
        self.enemies[a].thrown = false;
        let id = if self.enemies[a].fling != 0 { self.enemies[a].fling } else { let v = self.debug_chain; self.debug_chain += 1; v };
        { let e = &mut self.enemies[a]; e.fling = id; e.ammo = 99; e.boost = true; }
        let (aid, ax, az) = (self.enemies[a].id, self.enemies[a].x, self.enemies[a].z);
        self.emit(BattleEvent::HurlBoom { id: aid, x: ax, z: az });
        self.hitstop = math::max(self.hitstop, 0.06);
        if self.slow <= 0.0 { self.slow = 0.16; self.slow_scale = 0.35; }
        let r = 4.5;
        self.near_tmp2(ax, az, r);
        let mut k = 0;
        while k < self.tmp2.len() {
            let v = self.tmp2[k] as usize; k += 1;
            let ve = &self.enemies[v];
            if v == a || ve.state == EnemyState::Dead || ve.state == EnemyState::Carried || ve.role == Role::Captain || ve.chain_hit == id || ve.y > 2.5 { continue; }
            let (ax, az) = (self.enemies[a].x, self.enemies[a].z);
            let (dx, dz) = (ve.x - ax, ve.z - az);
            let d = or1(hypot(dx, dz));
            if d > r + ve.kind.def().radius { continue; }
            self.chain_hit(v, a, dx / d, dz / d, 15.0);
        }
        let e = &mut self.enemies[a];
        e.fling = 0; e.ammo = 0; e.boost = false;
        e.hp = 0.0; e.state = EnemyState::Dead; e.st = 0.0; e.dead_t = 0.0; e.vy = 7.0; e.vx *= 0.2; e.vz *= 0.2;
        self.player.kos += 1; self.gain_ame(self.enemies[a].kind);
        let e = &self.enemies[a];
        let ev = BattleEvent::Kill { id: e.id, kind: e.kind, x: e.x, y: e.y, z: e.z, captain: false };
        self.emit(ev);
        self.drop_from(a);
    }
    /// 隊長崩し: 重騎の弾が隊長に当たった。隊長は 2 秒倒れ、まわり 5m の雑兵が吹き飛ぶ
    fn captain_break(&mut self, v: usize, a: usize) {
        {
            let e = &mut self.enemies[v];
            e.break_cd = 10.0; e.poise = 320.0;
            e.hp = math::max(1.0, e.hp - 250.0);
            e.state = EnemyState::Down; e.st = -1.0; e.y = 0.0; e.vy = 0.0; e.vx = 0.0; e.vz = 0.0; e.token = false;
        }
        // 構えていた技はやめる
        let (ax, az) = (self.enemies[v].aim_x, self.enemies[v].aim_z);
        for t in &mut self.telegraphs { if t.src == TelegraphSrc::Captain && hypot(t.x - ax, t.z - az) < 1.0 { t.done = true; } }
        let (vid, vx, vz) = (self.enemies[v].id, self.enemies[v].x, self.enemies[v].z);
        self.emit(BattleEvent::CaptainBreak { id: vid, x: vx, z: vz });
        if !self.broke_captains.contains(&vid) { self.broke_captains.push(vid); self.add_merit(30); }
        self.en_mut(a).ammo = 99;
        self.near_tmp2(vx, vz, 5.0);
        let mut k = 0;
        while k < self.tmp2.len() {
            let g = self.tmp2[k] as usize; k += 1;
            if self.en(a).fling == 0 { break; }
            let afling = self.en(a).fling;
            let ge = &self.enemies[g];
            if g == v || g == a || ge.state == EnemyState::Dead || ge.role == Role::Captain || ge.chain_hit == afling || ge.y > 2.0 { continue; }
            let (vx, vz) = (self.enemies[v].x, self.enemies[v].z);
            let (dx, dz) = (ge.x - vx, ge.z - vz);
            let d = or1(hypot(dx, dz));
            if d > 5.0 + ge.kind.def().radius { continue; }
            self.chain_hit(g, a, dx / d, dz / d, 14.0);
        }
        let ae = self.en_mut(a);
        ae.ammo = 0; ae.fling = 0; ae.vx *= -0.2; ae.vz *= -0.2;
    }
    /// 乱舞の締め: 主役から 30m 以内（ほぼ画面の中）の金の印を、近い順に最大 20 か所まとめて爆発させる。爆発で飛んだ敵は新しい連鎖の弾
    fn detonate(&mut self) {
        let (px, pz) = (self.player.x, self.player.z);
        let ranbu_at = self.ranbu_at;
        let mut pick: Vec<Mark> = self.marks.iter().copied().filter(|m| m.born < ranbu_at && hypot(m.x - px, m.z - pz) <= 30.0).collect();
        pick.sort_by(|a, b| hypot(a.x - px, a.z - pz).partial_cmp(&hypot(b.x - px, b.z - pz)).unwrap());
        pick.truncate(20);
        if pick.is_empty() { return; }
        let id = self.debug_chain; self.debug_chain += 1;
        for (i, m) in pick.iter().enumerate() {
            self.emit(BattleEvent::Detonate { x: m.x, z: m.z, i: i as i32 });
            // 爆発そのものを弾に見立てる（重騎なみ。爆発が爆発を呼ぶことはない）
            self.blast.clone_from(&self.enemies[m.e]);
            { let b = &mut self.blast; b.x = m.x; b.z = m.z; b.y = 0.0; b.fling = id; b.gen = 0; b.ammo = 99; b.role = Role::Grunt; b.kind = EnemyKind::Mikan; b.boost = true; b.vx = 0.0; b.vz = 0.0; }
            self.near_tmp2(m.x, m.z, 3.5);
            let mut k = 0;
            while k < self.tmp2.len() {
                let v = self.tmp2[k] as usize; k += 1;
                let ve = &self.enemies[v];
                if ve.state == EnemyState::Dead || ve.role == Role::Captain || ve.chain_hit == id || ve.y > 2.5 { continue; }
                let (dx, dz) = (ve.x - m.x, ve.z - m.z);
                let d = or1(hypot(dx, dz));
                if d > 3.5 + ve.kind.def().radius { continue; }
                self.chain_hit(v, BLAST, dx / d, dz / d, 13.0);
            }
        }
        // marks.splice(marks.indexOf(m), 1): 押し出されて見つからない印は -1 になり、TS と同じく末尾を消す
        for m in &pick {
            match self.marks.iter().position(|o| o.uid == m.uid) {
                Some(i) => { self.marks.remove(i); }
                None => { self.marks.pop(); }
            }
        }
    }
    fn kill_by_chain(&mut self, v: usize, dir_x: f64, dir_z: f64, speed: f64, ci: usize) {
        self.chains[ci].kills += 1;
        let e = &mut self.enemies[v];
        e.hp = 0.0; e.state = EnemyState::Dead; e.st = 0.0; e.dead_t = 0.0;
        e.vx = dir_x * speed; e.vz = dir_z * speed;
        let ev = BattleEvent::Kill { id: e.id, kind: e.kind, x: e.x, y: e.y, z: e.z, captain: false };
        self.player.kos += 1; self.gain_ame(self.enemies[v].kind);
        self.emit(ev);
        self.drop_from(v);
    }
    /// 曲進の弾: 飛んでいる間、向きを少しずつ回す（速さは変えない）
    #[inline]
    fn curve_step(e: &mut Enemy, dt: f64) {
        if e.curve == 0.0 || e.curve.is_nan() { return; }
        let r = e.curve * dt;
        let (c, s) = (cos(r), sin(r));
        // 向き atan2(vx, vz) を r だけ増やす回転
        let (vx, vz) = (e.vx * c + e.vz * s, -e.vx * s + e.vz * c);
        e.vx = vx; e.vz = vz;
    }
    /// 飛んでいる敵が飴鏡に当たったら跳ね返す。弾は勢い 85% で 2 回まで、3 回目は鏡で止まって落ちる
    fn hit_mirror(&mut self, i: usize) {
        let e = &mut self.enemies[i];
        if e.y > MIRROR_H { return; }
        let d = e.kind.def();
        let Some(c) = mirror_contact(e.x, e.z, d.radius) else { return };
        e.x += c.nx * c.depth; e.z += c.nz * c.depth;
        let vn = e.vx * c.nx + e.vz * c.nz;
        if vn >= 0.0 { return; } // 離れていく途中
        if e.fling != 0 && e.mirror < MIRROR_MAX {
            e.vx -= 2.0 * vn * c.nx; e.vz -= 2.0 * vn * c.nz;
            e.vx *= MIRROR_KEEP; e.vz *= MIRROR_KEEP;
            e.mirror += 1;
            let ev = BattleEvent::Reflect { x: e.x - c.nx * d.radius, y: e.y + d.height * 0.5, z: e.z - c.nz * d.radius, mirror: c.i };
            self.emit(ev);
        } else {
            // 弾でない敵・跳ね返りきった弾は、鏡で止まって落ちる
            e.vx -= 1.2 * vn * c.nx; e.vz -= 1.2 * vn * c.nz;
            e.vx *= 0.4; e.vz *= 0.4;
            if e.fling != 0 { e.fling = 0; }
        }
    }
    /// 着地した弾の処理
    fn land_shock(&mut self, a: usize, impact: f64) {
        if self.enemies[a].thrown { self.thrown_boom(a); return; }
        if self.enemies[a].fling == 0 { return; }
        if impact < 8.0 || self.enemies[a].gen >= CHAIN_MAX_GEN { self.enemies[a].fling = 0; return; }
        let heavy = heavy_ammo(&self.enemies[a]);
        let r = if heavy { 2.6 } else { 1.7 };
        self.enemies[a].ammo = if heavy { 8 } else { 3 };
        let (ax0, az0) = (self.enemies[a].x, self.enemies[a].z);
        self.near_tmp2(ax0, az0, r);
        let mut k = 0;
        while k < self.tmp2.len() {
            let v = self.tmp2[k] as usize; k += 1;
            let ae = &self.enemies[a];
            if ae.fling == 0 { break; }
            let ve = &self.enemies[v];
            if v == a || ve.state == EnemyState::Dead || ve.y > 1.5 || ve.chain_hit == ae.fling || ve.fling == ae.fling { continue; }
            let (dx, dz) = (ve.x - ae.x, ve.z - ae.z);
            let d = or1(hypot(dx, dz));
            if d > r + ve.kind.def().radius { continue; }
            self.chain_hit(v, a, dx / d, dz / d, 7.0 + math::min(6.0, impact - 8.0) + if heavy { 3.0 } else { 0.0 });
        }
        self.enemies[a].fling = 0;
    }
    fn step_chains(&mut self, dt: f64) {
        self.chain_clock += dt;
        // 金の印: 生きている敵についていく。乱舞の間は古くならない（始めた時点の印を起爆できる）
        let aging = !self.player.r#move.is_some_and(|m| same(m, &RANBU));
        let mut i = self.marks.len() as i64 - 1;
        while i >= 0 {
            let idx = i as usize;
            let ee = self.marks[idx].e;
            let (alive, eid, ex, ez) = { let e = &self.enemies[ee]; (e.alive, e.id, e.x, e.z) };
            let m = &mut self.marks[idx];
            if alive && eid == m.id { m.x = ex; m.z = ez; }
            if aging { m.age += dt; }
            if m.age > MARK_LIFE { self.marks.remove(idx); }
            i -= 1;
        }
        let mut ai = 0;
        while ai < self.enemies.len() {
            let a = ai; ai += 1;
            {
                let ae = &mut self.enemies[a];
                if !ae.alive || ae.fling == 0 { continue; }
                if ae.state != EnemyState::Air && ae.state != EnemyState::Dead { ae.fling = 0; continue; }
            }
            let ae = &self.enemies[a];
            let sp = hypot(ae.vx, ae.vz);
            if sp < FLY_MIN || ae.y > 2.4 { continue; }
            let heavy = heavy_ammo(ae);
            let ra = ae.kind.def().radius + if heavy { 0.45 } else { 0.1 };
            let (dir_x, dir_z) = (ae.vx / sp, ae.vz / sp);
            let (ax0, az0) = (ae.x, ae.z);
            self.near_tmp2(ax0, az0, ra + 0.8);
            let mut k = 0;
            while k < self.tmp2.len() {
                let v = self.tmp2[k] as usize; k += 1;
                let ae = &self.enemies[a];
                if ae.fling == 0 || ae.ammo <= 0 { break; }
                let ve = &self.enemies[v];
                // 同じ連鎖ですでに当たった敵は飛ばさない。ただし、雑兵の弾で止まった隊長でも重騎の弾なら崩せる
                let can_break = ve.role == Role::Captain && ae.role == Role::Brute && ve.break_cd <= 0.0;
                if v == a || ve.state == EnemyState::Dead || ve.y > 2.0 || (ve.chain_hit == ae.fling && !can_break) || ve.fling == ae.fling { continue; }
                let (dx, dz) = (ve.x - ae.x, ve.z - ae.z);
                let d = hypot(dx, dz);
                if d > ra + ve.kind.def().radius { continue; }
                // 進む向きの前にいる敵だけ（すれ違いざまに後ろを巻き込まない）
                if d > 0.3 && (dx * dir_x + dz * dir_z) / d < -0.2 { continue; }
                let (nx, nz) = if d > 0.01 { (dx / d, dz / d) } else { (dir_x, dir_z) };
                let spd = hypot(ae.vx, ae.vz);
                self.chain_hit(v, a, dir_x * 0.65 + nx * 0.35, dir_z * 0.65 + nz * 0.35, spd);
            }
        }
        // Map の挿入順に回す（連鎖の終わりの順と武勲に効く）
        let mut i = 0;
        while i < self.chains.len() {
            if self.chain_clock - self.chains[i].t <= 0.6 { i += 1; continue; }
            let c = self.chains.remove(i);
            let bonus = chain_score(c.n);
            self.chain_bonus += bonus;
            self.emit(BattleEvent::ChainEnd { chain: c.id, n: c.n, kills: c.kills, bonus, x: c.sx / c.n as f64, z: c.sz / c.n as f64 });
            if c.n >= 20 && self.big_chain_merits < 4 { self.big_chain_merits += 1; self.add_merit(25); }
        }
    }

    fn hit_boss(&mut self, h: &Hit, move_key: i64) -> bool {
        let k = move_key * 64 + math::round(h.t * 50.0) as i64;
        if self.boss.last_hit_key == k { return false; }
        self.boss.last_hit_key = k;
        // 巨人が立っている間は、直接の傷が3分の1（脚を崩すほうが早い）
        // 飴糸の糸斬りは頭上からの一撃なので減らない。WIRE_TOPPLE 回で巨人が崩れる（step_giant）
        let mult = if self.boss.state == BossState::Down { 1.6 } else if self.giant.on && !self.wire_strike { 1.0 / 3.0 } else { 1.0 };
        if self.wire_strike && self.giant.on && self.giant.t > 2.4 { self.giant.wire_hits += 1; }
        self.boss.hp -= h.dmg * mult;
        self.boss.hit_t = 0.0;
        self.boss.poise -= h.dmg;
        if self.boss.hp <= 0.0 {
            self.boss.hp = 0.0; self.boss.state = BossState::Dead; self.boss.st = 0.0;
            self.phase = Phase::Victory; self.victory_t = 0.0;
            self.slow = 2.2; self.slow_scale = 0.22;
            let mut i = 0;
            while i < self.enemies.len() {
                let idx = i; i += 1;
                let e = &self.enemies[idx];
                if !(e.alive && e.state != EnemyState::Dead) { continue; }
                { let e = &mut self.enemies[idx]; e.hp = 0.0; e.state = EnemyState::Dead; }
                let vy = self.r(5.0, 10.0);
                self.enemies[idx].vy = vy;
                let a = atan2(self.enemies[idx].x - self.boss.x, self.enemies[idx].z - self.boss.z);
                let vx = sin(a) * self.r(6.0, 12.0);
                let vz = cos(a) * self.r(6.0, 12.0);
                let sv = self.r(8.0, 14.0);
                let e = &mut self.enemies[idx];
                e.vx = vx; e.vz = vz; e.spin_v = sv;
                let ev = BattleEvent::Kill { id: e.id, kind: e.kind, x: e.x, y: e.y, z: e.z, captain: false };
                self.player.kos += 1; self.gain_ame(self.enemies[idx].kind);
                self.emit(ev);
            }
            self.emit(BattleEvent::BossDefeated);
            return true;
        }
        if self.boss.poise <= 0.0 && self.giant.on { self.boss.poise = 1400.0; } // 巨人の上ではひざをつかない
        if self.boss.poise <= 0.0 && self.boss.state != BossState::Down && self.boss.state != BossState::Enter {
            self.boss.state = BossState::Down; self.boss.st = 0.0; self.boss.poise = 1400.0;
            for t in &mut self.telegraphs { if t.src == TelegraphSrc::Boss { t.done = true; } }
            self.emit(BattleEvent::BossDown);
        }
        false
    }

    fn resolve_hit(&mut self, h0: &Hit) {
        // 育つ武器: 通常攻撃の当たる範囲は武器の大きさに合わせて広がる（見た目の弧も同じだけ大きくする）
        let mut h = *h0;
        if self.player.r#move.is_some_and(is_normal) { h.r = h.r * self.weapon_reach(); }
        let (fx, fz) = (sin(self.player.face), cos(self.player.face));
        let (cx, cz) = (self.player.x + fx * h.off.unwrap_or(0.0), self.player.z + fz * h.off.unwrap_or(0.0));
        let reach = if h.shape == HitShape::Line { h.r + h.w.unwrap_or(2.0) } else { h.r };
        let (mut count, mut kills) = (0, 0);
        self.near_tmp(cx, cz, reach + 1.5);
        let mut targets = std::mem::take(&mut self.targets);
        targets.clear();
        let (pface, py) = (self.player.face, self.player.y);
        for k in 0..self.tmp.len() {
            let ei = self.tmp[k];
            let e = &self.enemies[ei as usize];
            if !e.alive || e.state == EnemyState::Dead || e.state == EnemyState::Carried { continue; }
            let (dx, dz) = (e.x - cx, e.z - cz);
            let rad = e.kind.def().radius;
            let mut inside = match h.shape {
                HitShape::Circle => dx * dx + dz * dz <= math::sq(h.r + rad),
                HitShape::Arc => {
                    let d = hypot(dx, dz);
                    d <= h.r + rad && (d < 1.2 || ang_diff(atan2(dx, dz), pface).abs() <= h.arc.unwrap_or(1.0) + rad / math::max(d, 0.5))
                }
                HitShape::Line => {
                    let (along, side) = (dx * fx + dz * fz, (dx * fz - dz * fx).abs());
                    along >= -1.0 && along <= h.r + rad && side <= h.w.unwrap_or(2.0) / 2.0 + rad
                }
            };
            // 高く浮いた敵は、高さも見る（空中の追い打ちは届く）
            if inside && e.y > 5.5 + py { inside = false; }
            // 高く飛んでいる主役（飴糸のあと）の斬りは、ずっと下の敵には届かない
            if inside && py - e.y > 4.0 { inside = false; }
            if inside { targets.push(ei); }
        }
        self.boost_hit = self.gathered > 0 && targets.len() >= 5;
        self.boost_pow = self.gathered;
        for &ei in &targets {
            let i = ei as usize;
            let (dx, dz) = (self.enemies[i].x - cx, self.enemies[i].z - cz);
            let (mut dir_x, mut dir_z) = (dx, dz);
            if h.shape == HitShape::Line { dir_x = fx; dir_z = fz; }
            let l = or1(hypot(dir_x, dir_z));
            dir_x /= l; dir_z /= l;
            let move_key = self.player.move_key;
            let killed = self.hit_enemy(i, &h, dir_x, dir_z, move_key);
            count += 1;
            if killed { kills += 1; }
            if count <= 10 {
                let e = &self.enemies[i];
                let ev = BattleEvent::Hit { x: e.x, y: e.y + e.kind.def().height * 0.5, z: e.z, count, heavy: h.stop >= 0.07, fx: h.fx, dx: dir_x, dz: dir_z, enemy: e.id, boss: false };
                self.emit(ev);
            }
        }
        self.targets = targets;
        let mut boss_hit = false;
        if self.boss.state != BossState::Dormant && self.boss.state != BossState::Dead && self.boss.state != BossState::Enter {
            let (dx, dz) = (self.boss.x - cx, self.boss.z - cz);
            let d = hypot(dx, dz);
            let br = 2.6;
            let inside = match h.shape {
                HitShape::Circle => d <= h.r + br,
                HitShape::Arc => d <= h.r + br && (d < 3.0 || ang_diff(atan2(dx, dz), self.player.face).abs() <= h.arc.unwrap_or(1.0) + br / math::max(d, 1.0)),
                HitShape::Line => { let (along, side) = (dx * fx + dz * fz, (dx * fz - dz * fx).abs()); along >= -1.0 && along <= h.r + br && side <= h.w.unwrap_or(2.0) / 2.0 + br }
            };
            if inside {
                boss_hit = true;
                let move_key = self.player.move_key;
                self.hit_boss(&h, move_key);
                count += 1;
                let ev = BattleEvent::Hit { x: self.boss.x - dx / or1(d) * 2.2, y: 2.2, z: self.boss.z - dz / or1(d) * 2.2, count, heavy: h.stop >= 0.07, fx: h.fx, dx: dx / or1(d), dz: dz / or1(d), enemy: -1, boss: true };
                self.emit(ev);
            }
        }
        self.boost_hit = false;
        let face = self.player.face;
        self.emit(BattleEvent::Impact { x: cx, z: cz, fx: h.fx, r: h.r, face, shape: h.shape, len: h.r });
        if count > 0 {
            // ヒットストップ: 多く当たるほど少し長く（上限あり）
            self.hitstop = math::max(self.hitstop, h.stop * (1.0 + math::min(0.8, count as f64 * 0.04)) + if kills >= 8 { 0.03 } else { 0.0 });
            self.player.combo += count as i64; self.player.combo_t = 0.0;
            if self.player.combo > self.player.max_combo { self.player.max_combo = self.player.combo; }
            if !self.player.r#move.is_some_and(|m| same(m, &RANBU)) {
                let inc = self.gain(math::min(4.0, count as f64 * 0.4) + if boss_hit { 1.0 } else { 0.0 });
                self.player.ranbu = math::min(100.0, self.player.ranbu + inc);
            }
            let n = self.player.combo;
            self.emit(BattleEvent::Combo { n });
        }
    }

    fn drop_from(&mut self, i: usize) {
        let r = self.rand.next();
        let (captain, x, z) = { let e = &self.enemies[i]; (e.role == Role::Captain, e.x, e.z) };
        if captain || r < 0.025 { let id = self.new_id(); self.pickups.push(Pickup { id, kind: PickupKind::Bun, x, z, t: 0.0, alive: true }); }
        else if r < 0.04 { let id = self.new_id(); self.pickups.push(Pickup { id, kind: PickupKind::Konpeito, x, z, t: 0.0, alive: true }); }
        if captain { let id = self.new_id(); self.pickups.push(Pickup { id, kind: PickupKind::Konpeito, x: x + 1.2, z, t: 0.0, alive: true }); }
    }

    fn on_captain_down(&mut self, i: usize) {
        let home = self.enemies[i].home;
        if home < 0 || home as usize >= self.holds.len() { return; }
        let hi = home as usize;
        if self.holds[hi].owner == Owner::Ally { return; }
        self.holds[hi].owner = Owner::Ally;
        self.stats.captures += 1;
        let (hx, hz, hid) = (self.holds[hi].x, self.holds[hi].z, self.holds[hi].id);
        // 士気崩壊: 砦の守り（とこの砦へ逃げていた敵）は、まだ落ちていない一番近い砦へ逃げ込む。逃げ先がなければその場で崩れる
        let mut to: Option<usize> = None;
        for (k, o) in self.holds.iter().enumerate() {
            if o.owner == Owner::Enemy && (to.is_none() || hypot(o.x - hx, o.z - hz) < hypot(self.holds[to.unwrap()].x - hx, self.holds[to.unwrap()].z - hz)) { to = Some(k); }
        }
        let to_id = to.map(|k| self.holds[k].id);
        let mut fled = 0;
        let mut k = 0;
        while k < self.enemies.len() {
            let oi = k; k += 1;
            let o = &mut self.enemies[oi];
            if !o.alive || o.state == EnemyState::Dead || o.state == EnemyState::Carried || o.role == Role::Captain { continue; }
            if o.flee != hid && !(o.home == hid && hypot(o.x - hx, o.z - hz) < 26.0) { continue; }
            if let Some(tid) = to_id {
                o.flee = tid; o.awake = true; o.token = false;
                if o.state == EnemyState::Idle || o.state == EnemyState::Windup || o.state == EnemyState::Strike || o.state == EnemyState::Recover { o.state = EnemyState::Chase; o.st = 0.0; o.y = 0.0; }
                fled += 1;
                continue;
            }
            o.hp = 0.0; o.state = EnemyState::Dead;
            let vy = self.r(4.0, 9.0);
            self.enemies[oi].vy = vy;
            let a = atan2(self.enemies[oi].x - hx, self.enemies[oi].z - hz);
            let vx = sin(a) * self.r(4.0, 9.0);
            let vz = cos(a) * self.r(4.0, 9.0);
            let sv = self.r(6.0, 12.0);
            let st = self.r(-0.3, 0.0);
            let o = &mut self.enemies[oi];
            o.vx = vx; o.vz = vz; o.spin_v = sv; o.st = st; o.flee = -1;
            let ev = BattleEvent::Kill { id: o.id, kind: o.kind, x: o.x, y: o.y, z: o.z, captain: false };
            self.player.kos += 1; self.gain_ame(self.enemies[oi].kind);
            self.emit(ev);
        }
        // この砦の構えていた予告は消す
        for t in &mut self.telegraphs { if t.src != TelegraphSrc::Boss && hypot(t.x - hx, t.z - hz) < 30.0 { t.done = true; } }
        self.slow = 1.1; self.slow_scale = 0.3;
        self.player.hp = math::min(self.player.max_hp, self.player.hp + 250.0);
        let left = self.holds.iter().filter(|o| o.owner == Owner::Enemy).count() as i32;
        self.emit(BattleEvent::Capture { hold: hid, left, fled, to: to_id.unwrap_or(-1) });
        self.add_merit(80);
        if left == 0 && !self.gate_open { self.gate_open = true; self.emit(BattleEvent::BossGate); }
    }

    // ---- 主役の更新 ----
    fn damage_player(&mut self, dmg: f64, from_x: f64, from_z: f64, big: bool) {
        if self.god_mode { return; }
        let p = &self.player;
        if p.invuln > 0.0 || p.state == PlayerState::Dead || p.state == PlayerState::Dodge || p.state == PlayerState::Wire { return; }
        if p.r#move.is_some_and(|m| m.invuln) { return; }
        self.player.hp -= dmg;
        self.player.dmg_taken += dmg;
        self.leave_ride(false);
        if self.player.carry != 0 {
            let (carry, face) = (self.player.carry, self.player.face);
            if let Some(c) = self.enemies.iter_mut().find(|o| o.id == carry) {
                if c.state == EnemyState::Carried { c.state = EnemyState::Air; c.vy = 3.0; c.vx = sin(face) * 2.0; c.vz = cos(face) * 2.0; }
            }
            self.player.carry = 0;
        }
        self.player.ranbu = math::min(100.0, self.player.ranbu + dmg * 0.06);
        self.player.combo = 0;
        let (px, pz) = (self.player.x, self.player.z);
        self.emit(BattleEvent::Hurt { dmg, x: px, z: pz, big });
        if self.player.hp <= 0.0 {
            let p = &mut self.player;
            p.hp = 0.0; p.state = PlayerState::Dead; p.st = 0.0; p.r#move = None; p.y = 0.0; p.vy = 0.0; p.air = false;
            self.phase = Phase::Defeat;
            self.emit(BattleEvent::Defeat);
            return;
        }
        let p = &mut self.player;
        let armor = p.r#move.is_some_and(|m| m.armor) && !big;
        if !armor {
            let a = atan2(p.x - from_x, p.z - from_z);
            p.state = PlayerState::Hurt; p.st = 0.0; p.r#move = None; p.chain = 0; p.queued = None;
            p.vx = sin(a) * if big { 9.0 } else { 4.0 }; p.vz = cos(a) * if big { 9.0 } else { 4.0 };
            if big { p.vy = 6.0; p.air = true; }
            p.hurt_t = if big { 0.75 } else { 0.42 };
        }
        p.invuln = if armor { 0.25 } else if big { 1.3 } else { 0.7 };
    }

    fn lunge_speed(m: &Move, t: f64) -> f64 {
        let mut v = 0.0;
        for &[a, b, d] in &m.lunge { if t >= a && t < b { v += d / (b - a); } }
        v
    }

    fn step_player(&mut self, dt: f64, input0: Input) {
        let mut input = input0;
        {
            let p = &mut self.player;
            p.invuln = math::max(0.0, p.invuln - dt);
            p.dodge_cd = math::max(0.0, p.dodge_cd - dt);
            p.wire_cd = math::max(0.0, p.wire_cd - dt);
            // 猪鹿蝶: 飛んでいる間は数えない。時間が切れたら数え直し
            if p.state != PlayerState::Wire && p.wire_chain_t > 0.0 { p.wire_chain_t = math::max(0.0, p.wire_chain_t - dt); if p.wire_chain_t <= 0.0 { p.wire_chain = 0; } }
            if p.state != PlayerState::Wire { p.wire_esc = math::max(0.0, p.wire_esc - dt); }
            p.ride_cd = math::max(0.0, p.ride_cd - dt);
            p.combo_t += dt;
            if p.combo_t > 3.0 && p.combo > 0 { p.combo = 0; }
            if p.state == PlayerState::Dead { return; }
        }
        let mag = math::min(1.0, hypot(input.mx, input.mz));
        let want = if mag > 0.15 { atan2(input.mx, input.mz) } else { self.player.face };

        // 乱舞
        if input.ranbu && self.player.ranbu >= 100.0 && self.player.state != PlayerState::Hurt {
            self.leave_ride(false);
            { let p = &mut self.player; p.ranbu = 0.0; p.air = false; p.y = 0.0; p.vy = 0.0; }
            self.ranbu_ready_sent = false;
            self.start_move(&RANBU, &input, 0);
            self.player.invuln = RANBU.dur; self.ranbu_at = self.chain_clock;
            self.ranbu_freeze = RANBU_CUTIN;
            self.emit(BattleEvent::RanbuStart);
        }
        if self.player.ranbu >= 100.0 && !self.ranbu_ready_sent { self.ranbu_ready_sent = true; self.emit(BattleEvent::RanbuReady); }

        // 担いだ重騎: 通常攻撃か強攻撃で大車輪（振り回して正面へ投げる）。3 秒たつと自動で投げる
        if self.player.carry != 0 {
            let carry = self.player.carry;
            let c = self.enemies.iter().find(|o| o.id == carry).map(|c| (c.alive, c.state));
            self.player.carry_t += dt;
            let p = &self.player;
            match c {
                Some((true, EnemyState::Carried)) => {
                    if (p.state == PlayerState::Free || p.state == PlayerState::Dodge) && (input.attack || input.heavy || p.carry_t >= 3.0) {
                        self.start_move(&HURL, &input, 0);
                        input.attack = false; input.heavy = false;
                    }
                }
                _ => self.player.carry = 0,
            }
        }
        // 飴糸: 自由なとき・回避の間・技の途中（回避と同じく 0.12 秒たてば）に出せる。担いでいる間・乱舞・大車輪の間は出せない
        if input.wire && self.player.wire_cd <= 0.0 && self.player.carry == 0 {
            let p = &self.player;
            let ok = match p.state {
                PlayerState::Free | PlayerState::Dodge | PlayerState::Ride => true,
                PlayerState::Attack => p.r#move.is_some_and(|m| !same(m, &RANBU) && !same(m, &HURL)) && p.move_t > 0.12,
                _ => false,
            };
            // 飴を 1 個使う。無ければ出ない
            if ok && self.player.ame <= 0 { self.emit(BattleEvent::NoAme); }
            else if ok { self.player.ame -= 1; self.leave_ride(false); self.start_wire(want, mag > 0.15); }
        }
        if self.player.state == PlayerState::Wire {
            self.step_wire(dt);
        } else if self.player.state == PlayerState::Ride {
            // わたあめ雲: ふわりと上がり、乗ったまま飛ぶ（向きはゆっくり変わる）。通常攻撃・強攻撃で飛び降りて空中の技、跳ぶで降りる。時間切れで溶ける
            {
                let p = &mut self.player;
                p.st += dt;
                p.vy = 0.0;
                p.y += (RIDE_Y - p.y) * math::min(1.0, dt * 5.0);
                p.speed += (mag * RIDE_SPEED - p.speed) * math::min(1.0, dt * 4.0);
                if mag > 0.15 { p.face += clamp(ang_diff(want, p.face), -dt * 5.0, dt * 5.0); }
                p.x += sin(p.face) * p.speed * dt; p.z += cos(p.face) * p.speed * dt;
            }
            if input.attack { self.leave_ride(false); self.start_move(&JUMP_ATTACK, &input, 0); }
            else if input.heavy { self.leave_ride(false); self.start_move(&JUMP_CHARGE, &input, 0); }
            else if input.jump { self.leave_ride(false); self.player.vy = 5.0; }
            else if self.player.st >= RIDE_T { self.leave_ride(true); }
        } else if self.player.state == PlayerState::Hurt {
            let p = &mut self.player;
            p.st += dt;
            p.vx *= exp(-dt * 6.0); p.vz *= exp(-dt * 6.0);
            p.x += p.vx * dt; p.z += p.vz * dt;
            // 大きく吹き飛ばされたときは宙を舞って落ちる
            if p.air { p.vy -= GRAVITY * dt; p.y += p.vy * dt; if p.y <= 0.0 { p.y = 0.0; p.vy = 0.0; p.air = false; } }
            if p.st >= p.hurt_t && !p.air { p.state = PlayerState::Free; }
        } else if self.player.state == PlayerState::Dodge {
            let p = &mut self.player;
            p.st += dt;
            let sp = if p.st < 0.28 { 17.0 } else { 6.0 };
            p.x += sin(p.face) * sp * dt; p.z += cos(p.face) * sp * dt;
            if input.attack && p.st > 0.08 { p.state = PlayerState::Free; self.start_move(&DASH_ATTACK, &input, 0); }
            else if p.st >= DODGE_T { p.state = PlayerState::Free; }
        } else if self.player.state == PlayerState::Attack && self.player.r#move.is_some() {
            let m = self.player.r#move.unwrap();
            let is_ranbu = same(m, &RANBU);
            let t0 = self.player.move_t;
            self.player.move_t += dt;
            let t1 = self.player.move_t;
            {
                let p = &mut self.player;
                if t1 < m.steer && mag > 0.2 && !is_ranbu { p.face += clamp(ang_diff(want, p.face), -dt * 7.0, dt * 7.0); }
                let ls = Self::lunge_speed(m, t1);
                p.x += sin(p.face) * ls * dt; p.z += cos(p.face) * ls * dt;
                if let Some([a, b, h]) = m.hop {
                    p.y = if t1 >= a && t1 < b { sin(((t1 - a) / (b - a)) * PI) * h } else if p.air { p.y } else { 0.0 };
                }
            }
            for (i, arc) in m.arcs.iter().enumerate() { if arc.t > t0 && arc.t <= t1 { self.emit(BattleEvent::Swing { r#move: m, index: i as i32 }); } }
            for h in &m.hits { if h.t > t0 && h.t <= t1 { self.resolve_hit(h); } }
            // 乱舞の起爆: 締めの爆発が収まってから（0.4 秒あと）、第二波として金の印をまとめて爆発させる
            if is_ranbu { let td = RANBU.hits[RANBU.hits.len() - 1].t + 0.4; if t0 < td && t1 >= td { self.detonate(); } }
            if is_ranbu && t0 < RANBU.dur - 0.6 && t1 >= RANBU.dur - 0.6 { self.emit(BattleEvent::RanbuFinish); }
            if same(m, &HURL) && t0 < HURL_RELEASE && t1 >= HURL_RELEASE && self.player.carry != 0 {
                let carry = self.player.carry;
                match self.enemies.iter().position(|o| o.id == carry) {
                    Some(c) if self.enemies[c].state == EnemyState::Carried => { let face = self.player.face; self.hurl(c, face); }
                    _ => self.player.carry = 0,
                }
            }
            // 先行入力
            if input.attack { self.player.queued = Some(Queued::N); }
            if input.heavy { self.player.queued = Some(Queued::C); }
            if input.dodge && !is_ranbu && t1 > 0.12 && self.player.dodge_cd <= 0.0 { self.player.queued = None; self.do_dodge(want); return; }
            if t1 >= m.cancel && self.player.queued.is_some() {
                let q = self.player.queued.take().unwrap();
                let chain = self.player.chain as usize;
                if same(m, &JUMP_ATTACK) || same(m, &JUMP_CHARGE) { /* 空中技のあとは地上の技へつながない */ }
                else if q == Queued::N && is_normal(m) && chain < NORMALS.len() {
                    self.start_move(&NORMALS[chain], &input, if chain == NORMALS.len() - 1 { NORMALS.len() as i32 } else { 0 });
                    self.player.chain += 1;
                    return;
                }
                else if q == Queued::C && is_normal(m) { self.start_move(&CHARGES[chain], &input, chain as i32); self.player.chain = 0; return; }
                else if q == Queued::N && (same(m, &DASH_ATTACK) || is_charge(m)) { self.player.chain = 0; self.start_move(&NORMALS[0], &input, 0); self.player.chain = 1; return; }
                else if q == Queued::C && same(m, &DASH_ATTACK) { self.start_move(&CHARGES[0], &input, 0); self.player.chain = 0; return; }
            }
            if t1 >= m.dur {
                if same(m, &JUMP_CHARGE) { self.emit(BattleEvent::Land { heavy: true }); }
                let p = &mut self.player;
                p.r#move = None; p.state = PlayerState::Free; p.chain = 0;
                if p.y > 0.05 && same(m, &JUMP_ATTACK) { p.air = true; } else { p.y = 0.0; p.air = false; p.vy = 0.0; }
                if is_ranbu { p.invuln = 0.6; }
            }
        } else {
            // 自由に動ける
            {
                let p = &mut self.player;
                let target = mag * 8.2;
                p.speed += (target - p.speed) * math::min(1.0, dt * 12.0);
                if mag > 0.15 { p.face += clamp(ang_diff(want, p.face), -dt * 14.0, dt * 14.0); }
                p.x += sin(p.face) * p.speed * dt; p.z += cos(p.face) * p.speed * dt;
            }
            if self.player.air {
                let p = &mut self.player;
                // 猪鹿蝶を続けられる間は、空をゆっくり落ちる（敵の手が届かない所から次の飴糸を出せる）
                let float = p.wire_chain > 0 && p.wire_chain_t > 0.0 && p.vy < 0.0;
                p.vy -= GRAVITY * if float { WIRE_FLOAT } else { 1.0 } * dt; p.y += p.vy * dt;
                if p.y <= 0.0 { p.y = 0.0; p.vy = 0.0; p.air = false; self.emit(BattleEvent::Land { heavy: false }); }
                if input.attack { self.start_move(&JUMP_ATTACK, &input, 0); }
                else if input.heavy { self.start_move(&JUMP_CHARGE, &input, 0); }
                // わたあめ雲: 空中でもう一度跳ぶと、雲に乗る
                else if input.jump && self.player.air && self.player.ride_cd <= 0.0 && self.player.carry == 0 {
                    let p = &mut self.player;
                    p.state = PlayerState::Ride; p.st = 0.0; p.vy = 0.0;
                    self.emit(BattleEvent::Ride);
                }
            } else if input.jump { self.player.air = true; self.player.vy = JUMP_V; self.emit(BattleEvent::Jump); }
            else if input.dodge && self.player.dodge_cd <= 0.0 { self.do_dodge(want); }
            else if input.attack { self.player.chain = 0; self.start_move(&NORMALS[0], &input, 0); self.player.chain = 1; }
            else if input.heavy { self.player.chain = 0; self.start_move(&CHARGES[0], &input, 0); }
        }
        // 空中の技の間は、落ちながら
        let jump_move = self.player.state == PlayerState::Attack && self.player.r#move.is_some_and(|m| same(m, &JUMP_ATTACK) || same(m, &JUMP_CHARGE));
        if jump_move {
            let m = self.player.r#move.unwrap();
            let p = &mut self.player;
            // 墜星: 当たり（0.22 秒）までに地面へ着く。飴糸のあとの高い所からは、そのぶん速く落ちる（ふつうの跳躍からは今までどおり -34）
            if same(m, &JUMP_CHARGE) { p.vy = if p.move_t < 0.12 { 2.0 } else { -math::max(34.0, p.y / math::max(0.2 - p.move_t, 0.008)) }; }
            else { p.vy -= GRAVITY * 0.5 * dt; }
            p.y = math::max(0.0, p.y + p.vy * dt);
            if p.y <= 0.0 && same(m, &JUMP_ATTACK) && p.move_t > 0.2 { p.r#move = None; p.state = PlayerState::Free; p.air = false; p.vy = 0.0; self.emit(BattleEvent::Land { heavy: false }); }
        }
        // 跳ねる技（昇り龍・地裂・百花）の途中で回避・被弾・次の技へ移ったときは、その高さから地面へ落ちる
        {
            let p = &mut self.player;
            let hop = if p.state == PlayerState::Attack { p.r#move.and_then(|m| m.hop) } else { None };
            let in_hop = hop.is_some_and(|h| p.move_t >= h[0] && p.move_t < h[1]);
            let jump_move = p.state == PlayerState::Attack && p.r#move.is_some_and(|m| same(m, &JUMP_ATTACK) || same(m, &JUMP_CHARGE));
            if p.y > 0.0 && !p.air && !in_hop && !jump_move {
                p.vy = math::min(p.vy, 0.0) - GRAVITY * dt;
                p.y = math::max(0.0, p.y + p.vy * dt);
                if p.y == 0.0 { p.vy = 0.0; }
            }
            // 飴鏡は通り抜けない
            // （飴鏡の高さより上を飛んでいれば越えられる。跳躍の頂点は約 2.1m なので、跳ぶだけでは越えられない）
            if p.y < MIRROR_H { if let Some(c) = mirror_contact(p.x, p.z, 0.55) { p.x += c.nx * c.depth; p.z += c.nz * c.depth; } }
            // 場の外へは出ない
            let d = hypot(p.x, p.z);
            if d > FIELD_R - 2.0 { p.x *= (FIELD_R - 2.0) / d; p.z *= (FIELD_R - 2.0) / d; }
            // 大将の体は通れない
            let b = &self.boss;
            if b.state != BossState::Dormant && b.state != BossState::Dead {
                let (dx, dz) = (p.x - b.x, p.z - b.z);
                let bd = hypot(dx, dz);
                if bd < 3.2 && bd > 0.0 { p.x = b.x + dx / bd * 3.2; p.z = b.z + dz / bd * 3.2; }
            }
        }
        // 拾い物
        for k in 0..self.pickups.len() {
            let pk = self.pickups[k];
            if !pk.alive { continue; }
            if hypot(pk.x - self.player.x, pk.z - self.player.z) < 1.8 {
                self.pickups[k].alive = false;
                let p = &mut self.player;
                if pk.kind == PickupKind::Bun { p.hp = math::min(p.max_hp, p.hp + 300.0); }
                else { p.ranbu = math::min(100.0, p.ranbu + 30.0); }
                self.emit(BattleEvent::Pickup { kind: pk.kind, x: pk.x, z: pk.z });
            }
        }
        if let Some(&ms) = MILESTONES.get(self.milestone_i) {
            if self.player.kos >= ms { self.emit(BattleEvent::Milestone { kos: ms }); self.milestone_i += 1; }
        }
    }
    // ---- わたあめ雲 ----
    /// 雲から降りる（乗っていなければ何もしない）。空中に残って落ちる。このあとの状態（技・飴糸・のけぞり）は呼ぶ側が決める
    fn leave_ride(&mut self, melted: bool) {
        if self.player.state != PlayerState::Ride { return; }
        let p = &mut self.player;
        p.state = PlayerState::Free; p.air = true; p.vy = 0.0; p.ride_cd = RIDE_CD;
        self.emit(BattleEvent::RideEnd { melted });
    }

    // ---- 飴 ----
    /// 砂糖でできた敵を倒すと、飴のもとがたまる（AME_PER で 1 個。AME_MAX 個まで）
    fn gain_ame(&mut self, kind: EnemyKind) {
        if !kind.sugary() || self.player.ame >= AME_MAX { return; }
        self.player.ame_fill += match kind.def().role { Role::Captain => AME_PER, Role::Brute => 3, _ => 1 };
        while self.player.ame_fill >= AME_PER && self.player.ame < AME_MAX {
            self.player.ame_fill -= AME_PER; self.player.ame += 1;
            let count = self.player.ame;
            self.emit(BattleEvent::Ame { count });
        }
        if self.player.ame >= AME_MAX { self.player.ame_fill = 0; }
    }

    // ---- 飴糸 ----
    /// 飴糸の的: 向き want の先の武将（隊長）か大将。返すのは（的, 糸の先）。いなければ None（足軽・投石兵・重騎へは向かわない）
    /// 猪鹿蝶を続けている間は、いま斬った同じ的へ戻る（向きを押していないとき。押しているときは、その向きの先 70 度以内にいれば）
    fn wire_pick(&self, want: f64, has_dir: bool) -> Option<(i64, f64, f64, f64)> {
        let (px, pz) = (self.player.x, self.player.z);
        if self.player.wire_chain > 0 && self.player.wire_chain_t > 0.0 {
            let last = self.player.wire_last;
            let near = |x: f64, z: f64, range: f64| hypot(x - px, z - pz) <= range && (!has_dir || ang_diff(atan2(x - px, z - pz), want).abs() <= WIRE_BACK_CONE);
            let b = &self.boss;
            if last == -1 && !matches!(b.state, BossState::Dormant | BossState::Dead | BossState::Enter) && near(b.x, b.z, WIRE_BOSS_RANGE) { return Some((-1, b.x, self.boss_y + 2.6, b.z)); }
            if last > 0 {
                if let Some(e) = self.enemies.iter().find(|e| e.id == last && e.alive && !matches!(e.state, EnemyState::Dead | EnemyState::Carried | EnemyState::Stacked)) {
                    if near(e.x, e.z, WIRE_RANGE) { return Some((e.id, e.x, e.y + e.kind.def().height * 0.6, e.z)); }
                }
            }
        }
        let mut best: Option<(i64, f64, f64, f64)> = None;
        let mut bs = 1e9;
        for e in &self.enemies {
            if !e.alive || e.role != Role::Captain { continue; }
            if matches!(e.state, EnemyState::Dead | EnemyState::Carried | EnemyState::Stacked) { continue; }
            let (dx, dz) = (e.x - px, e.z - pz);
            let d = hypot(dx, dz);
            let a = ang_diff(atan2(dx, dz), want).abs();
            if d > WIRE_RANGE || d < 1.0 || a > WIRE_CONE { continue; }
            let score = d + a * 8.0;
            if score < bs { bs = score; best = Some((e.id, e.x, e.y + e.kind.def().height * 0.6, e.z)); }
        }
        let b = &self.boss;
        if !matches!(b.state, BossState::Dormant | BossState::Dead | BossState::Enter) {
            let (dx, dz) = (b.x - px, b.z - pz);
            let d = hypot(dx, dz);
            let a = ang_diff(atan2(dx, dz), want).abs();
            // 大将を優先する（近くにいる間は、手前の隊長より先に選ぶ）
            if d < WIRE_BOSS_RANGE && a < WIRE_BOSS_CONE && d * 0.5 + a * 6.0 - 8.0 < bs { best = Some((-1, b.x, self.boss_y + 2.6, b.z)); }
        }
        best
    }
    fn start_wire(&mut self, want: f64, has_dir: bool) {
        let (px, py, pz) = (self.player.x, self.player.y, self.player.z);
        // 斬った直後の飴糸は、斬り込まずに空へ逃げる。向きを押していればその向きへ、押していなければ的の逆へ後ろ宙返りで
        let esc = self.player.wire_esc > 0.0;
        let flip = esc && !has_dir;
        let (target, mut ax, ay, mut az) = if esc {
            let last = self.player.wire;
            let dir = if has_dir || hypot(px - last.ax, pz - last.az) < 0.01 { want } else { atan2(px - last.ax, pz - last.az) };
            (0, px + sin(dir) * WIRE_ESC, math::max(WIRE_ESC_Y, py + 2.5), pz + cos(dir) * WIRE_ESC)
        } else {
            self.wire_pick(want, has_dir).unwrap_or((0, px + sin(want) * WIRE_FREE, WIRE_FREE_Y, pz + cos(want) * WIRE_FREE))
        };
        // 場の外へは糸を掛けない
        let d = hypot(ax, az);
        if target == 0 && d > FIELD_R - 2.0 { ax *= (FIELD_R - 2.0) / d; az *= (FIELD_R - 2.0) / d; }
        let dist = hypot(hypot(ax - px, az - pz), ay - py);
        self.gathered = 0;
        let p = &mut self.player;
        let dir = if hypot(ax - px, az - pz) > 0.01 { atan2(ax - px, az - pz) } else { want };
        p.face = if flip { dir + PI } else { dir };
        p.state = PlayerState::Wire; p.st = 0.0; p.r#move = None; p.chain = 0; p.queued = None; p.move_key += 1;
        p.air = true; p.vy = 0.0; p.speed = 0.0; p.wire_esc = 0.0;
        // 斬り込む飛びは、横へふくらむ弧を描く（振り子）。続けている間は大きく、左右を交互に（糸が交差する）
        let flat = hypot(ax - px, az - pz);
        let swing = if target == 0 { 0.0 } else if p.wire_chain > 0 && p.wire_chain_t > 0.0 { math::min(6.0, flat * 0.32) * if p.wire_chain % 2 == 1 { 1.0 } else { -1.0 } } else { math::min(3.0, flat * 0.14) };
        p.wire = Wire { x0: px, y0: py, z0: pz, ax, ay, az, dur: math::max(0.22, dist / WIRE_SPEED) * if swing.abs() > 3.0 { 1.12 } else { 1.0 }, target, esc, flip, swing, slowed: false };
        self.emit(BattleEvent::Wire { x: ax, y: ay, z: az, target: target != 0 });
    }
    fn step_wire(&mut self, dt: f64) {
        // 的が動けば糸の先も追う（倒れた・消えた的は、最後にいた場所へ飛ぶ）
        let target = self.player.wire.target;
        if target == -1 {
            if !matches!(self.boss.state, BossState::Dormant | BossState::Dead) { let (x, y, z) = (self.boss.x, self.boss_y + 2.6, self.boss.z); let w = &mut self.player.wire; w.ax = x; w.ay = y; w.az = z; }
        } else if target > 0 {
            if let Some(e) = self.enemies.iter().find(|e| e.id == target && e.alive && e.state != EnemyState::Dead) { let (x, y, z) = (e.x, e.y + e.kind.def().height * 0.6, e.z); let w = &mut self.player.wire; w.ax = x; w.ay = y; w.az = z; }
        }
        let w = self.player.wire;
        let (dx, dz) = (w.ax - w.x0, w.az - w.z0);
        let d = or1(hypot(dx, dz));
        let p = &mut self.player;
        p.st += dt;
        p.face = if w.flip { atan2(-dx, -dz) } else { atan2(dx, dz) };
        if p.st < WIRE_AIM { return; }
        let k = math::min(1.0, (p.st - WIRE_AIM) / w.dur);
        // 引かれて加速する。的へは手前で止まる（大将は体の外）
        let e = k * (0.35 + 0.65 * k);
        let stop = if target == -1 { 3.6 } else if target != 0 { 2.2 } else { 0.0 };
        let reach = math::max(0.0, d - stop);
        let lift = if target == 0 { 2.2 } else { math::min(2.4, d * 0.12) };
        let off = w.swing * sin(k * PI);
        p.x = w.x0 + dx / d * reach * e + dz / d * off; p.z = w.z0 + dz / d * reach * e - dx / d * off;
        p.y = math::max(0.0, w.y0 + (w.ay - w.y0) * e + sin(k * PI) * lift);
        // 低く飛ぶ間は、通り道の敵を弾き飛ばす
        if p.y < 2.6 { self.wire_sweep(); }
        // 三撃目: 刃が届く直前だけ時間を割る（1 人用）
        let third = target != 0 && !w.esc && self.player.wire_chain == 2 && self.player.wire_chain_t > 0.0;
        if third && k >= 0.8 && k < 1.0 && !w.slowed { self.slow = 0.2; self.slow_scale = 0.25; self.player.wire.slowed = true; }
        if k < 1.0 { return; }
        let p = &mut self.player;
        p.state = PlayerState::Free; p.air = true; p.st = 0.0;
        if target == 0 {
            p.vy = 3.0; p.speed = if w.flip { 0.0 } else { 8.2 }; p.wire_cd = if w.esc { WIRE_CD_ESC } else { WIRE_CD_FREE }; p.invuln = math::max(p.invuln, 0.15);
            // 猪鹿蝶を続けている間は、着いたところから数え直す（空で敵の動きを見てから次を出せる）
            if p.wire_chain > 0 { p.wire_chain_t = WIRE_CHAIN_T; }
            return;
        }
        // 糸斬り: 着いた瞬間に、的とそのまわりを斬る。斬ったあとは少し跳ね上がって、的のそばの空中に残る（自分では離れない。次の飴糸で空へ逃げる）
        // 猪鹿蝶: 続けて斬るたびに強くなり、3 回目で数え直す
        let n = if p.wire_chain_t > 0.0 { p.wire_chain % 3 + 1 } else { 1 };
        p.wire_chain = if n == 3 { 0 } else { n }; p.wire_chain_t = if n == 3 { 0.0 } else { WIRE_CHAIN_T };
        p.vy = 7.5; p.speed = 0.0; p.wire_cd = WIRE_CD_HIT; p.invuln = math::max(p.invuln, 0.45);
        p.wire_esc = WIRE_CHAIN_T; p.wire_last = target;
        let hit = Hit { dmg: WIRE_STRIKE.dmg * WIRE_MULT[n as usize - 1], r: if n == 3 { WIRE_FINISH_R } else { WIRE_STRIKE.r }, power: if n == 3 { 17.0 } else { WIRE_STRIKE.power }, shake: if n == 3 { 1.0 } else { WIRE_STRIKE.shake }, stop: if n == 3 { 0.3 } else { WIRE_STRIKE.stop }, ..WIRE_STRIKE };
        self.wire_strike = true;
        self.resolve_hit(&hit);
        self.wire_strike = false;
        // 三撃目のあと: ゆっくり流して決め絵を見せる（1 人用）
        if n == 3 { self.slow = 0.7; self.slow_scale = 0.3; }
        self.emit(BattleEvent::WireStrike { x: w.ax, y: w.ay, z: w.az, boss: target == -1, chain: n as u8 });
        if n == 3 { self.emit(BattleEvent::Inoshikacho { x: w.ax, y: w.ay, z: w.az }); }
    }
    /// 飴糸で低く飛ぶ間: 通り道の敵を横へ弾く（1 回の飛びで 1 体に 1 回。的と隊長は弾かない）
    fn wire_sweep(&mut self) {
        let (px, pz, face, key, target) = (self.player.x, self.player.z, self.player.face, self.player.move_key, self.player.wire.target);
        let hit_key = key * 64 + math::round(WIRE_SWEEP.t * 50.0) as i64;
        self.near_tmp(px, pz, WIRE_SWEEP.r + 1.5);
        let mut targets = std::mem::take(&mut self.targets);
        targets.clear();
        for k in 0..self.tmp.len() {
            let e = &self.enemies[self.tmp[k] as usize];
            if !e.alive || matches!(e.state, EnemyState::Dead | EnemyState::Carried | EnemyState::Stacked) || e.role == Role::Captain || e.id == target || e.last_hit_key == hit_key { continue; }
            let (dx, dz) = (e.x - px, e.z - pz);
            if dx * dx + dz * dz > math::sq(WIRE_SWEEP.r + e.kind.def().radius) || e.y > 3.0 { continue; }
            targets.push(self.tmp[k]);
        }
        let (fs, fc) = (sin(face), cos(face));
        let mut count = 0;
        for &ei in &targets {
            let i = ei as usize;
            // 進む向きから外へ（右にいる敵は右へ）
            let side = if (self.enemies[i].x - px) * fc - (self.enemies[i].z - pz) * fs >= 0.0 { 1.0 } else { -1.0 };
            let (dir_x, dir_z) = (fs * 0.6 + fc * side * 0.8, fc * 0.6 - fs * side * 0.8);
            self.hit_enemy(i, &WIRE_SWEEP, dir_x, dir_z, key);
            count += 1;
            if count <= 6 {
                let e = &self.enemies[i];
                let ev = BattleEvent::Hit { x: e.x, y: e.y + e.kind.def().height * 0.5, z: e.z, count, heavy: false, fx: WIRE_SWEEP.fx, dx: dir_x, dz: dir_z, enemy: e.id, boss: false };
                self.emit(ev);
            }
        }
        self.targets = targets;
        if count > 0 {
            self.player.combo += count as i64; self.player.combo_t = 0.0;
            if self.player.combo > self.player.max_combo { self.player.max_combo = self.player.combo; }
            let n = self.player.combo;
            self.emit(BattleEvent::Combo { n });
        }
    }

    fn grab(&mut self, i: usize) {
        let e = &mut self.enemies[i];
        e.state = EnemyState::Carried; e.token = false; e.fling = 0; e.vx = 0.0; e.vz = 0.0; e.vy = 0.0; e.spin_v = 0.0;
        let id = e.id;
        self.player.carry = id; self.player.carry_t = 0.0;
        // つかんだ瞬間: ほんの一瞬止めて、ゆっくりにする
        self.hitstop = math::max(self.hitstop, 0.09);
        if self.slow <= 0.0 { self.slow = 0.22; self.slow_scale = 0.35; }
        self.emit(BattleEvent::Grab { id });
    }
    /// 担いだ重騎を投げる: 集めて撃った弾と同じく突き抜ける速い弾
    fn hurl(&mut self, i: usize, dir: f64) {
        let (fx, fz) = (sin(dir), cos(dir));
        self.player.face = dir; self.player.carry = 0;
        let id = self.debug_chain; self.debug_chain += 1;
        let (px, pz) = (self.player.x, self.player.z);
        let e = &mut self.enemies[i];
        e.state = EnemyState::Air; e.st = 0.0; e.thrown = true; e.mirror = 0; e.curve = 0.0;
        e.x = px + fx * 1.6; e.z = pz + fz * 1.6; e.y = 1.6;
        e.vx = fx * 24.0; e.vz = fz * 24.0; e.vy = 5.0; e.spin_v = 18.0;
        e.fling = id; e.gen = 0; e.boost = true; e.ammo = 10; e.chain_hit = id; e.chain_t = 0.0; e.hit_t = 0.0;
        let (eid, ex, ez) = (e.id, e.x, e.z);
        self.hitstop = math::max(self.hitstop, 0.07);
        self.emit(BattleEvent::Hurl { id: eid, x: ex, z: ez });
    }
    fn do_dodge(&mut self, want: f64) {
        let p = &mut self.player;
        p.face = want; p.state = PlayerState::Dodge; p.st = 0.0; p.r#move = None; p.chain = 0; p.dodge_cd = 0.45; p.invuln = math::max(p.invuln, 0.32);
        self.emit(BattleEvent::Dodge);
    }

    // ---- 敵の更新 ----
    fn step_enemies(&mut self, dt: f64) {
        self.attackers = 0;
        for e in &self.enemies { if e.alive && e.token { self.attackers += 1; } }
        let max_attack = if self.boss.state != BossState::Dormant && self.boss.state != BossState::Dead { 2 } else { 4 };
        let mut idx = 0;
        while idx < self.enemies.len() {
            let i = idx; idx += 1;
            let e = &mut self.enemies[i];
            if !e.alive { e.dead_t += dt; continue; }
            let d = e.kind.def();
            e.hit_t += dt; e.chain_t += dt; e.break_cd -= dt;
            e.st += dt;
            if e.state == EnemyState::Dead {
                e.dead_t += dt;
                e.vy -= GRAVITY * dt; e.y += e.vy * dt; e.x += e.vx * dt; e.z += e.vz * dt; e.spin += e.spin_v * dt;
                Self::curve_step(e, dt);
                self.hit_mirror(i);
                if self.enemies[i].y <= 0.0 {
                    self.enemies[i].y = 0.0; self.enemies[i].curve = 0.0;
                    let impact = -self.enemies[i].vy;
                    self.land_shock(i, impact);
                    let e = &mut self.enemies[i];
                    if e.vy < -3.0 { e.vy = -e.vy * 0.3; e.vx *= 0.5; e.vz *= 0.5; } else { e.vy = 0.0; e.vx *= 0.8; e.vz *= 0.8; }
                }
                let e = &mut self.enemies[i];
                if e.dead_t > if e.role == Role::Captain { 1.6 } else { 0.95 } {
                    e.alive = false; e.dead_t = 0.0;
                    let ev = BattleEvent::Vanish { id: e.id, kind: e.kind, x: e.x, z: e.z };
                    self.emit(ev);
                }
                continue;
            }
            // 担がれている重騎は主役の頭上についていく（持ち上げる間は下から上がる）。大車輪の間は主役のまわりを振り回される
            if e.state == EnemyState::Carried {
                e.vx = 0.0; e.vz = 0.0; e.vy = 0.0;
                let p = &self.player;
                if p.r#move.is_some_and(|m| same(m, &HURL)) {
                    let a = p.face + spin_at(&HURL, p.move_t);
                    e.x = p.x + sin(a) * 1.8; e.z = p.z + cos(a) * 1.8; e.y = p.y + 1.5; e.face = a;
                } else { e.x = p.x; e.z = p.z; e.y = p.y + math::min(2.6, 0.8 + p.carry_t * 9.0); e.face = p.face; }
                continue;
            }
            if e.state == EnemyState::Stacked { continue; } // 位置は巨人が決める
            if self.ranbu_freeze > 0.0 { continue; }
            let (px, pz) = (self.player.x, self.player.z);
            let (dx, dz) = (px - e.x, pz - e.z);
            let dist = hypot(dx, dz);
            // 目覚め: 近づく・砦の警報
            if !e.awake {
                if dist < (if e.home >= 0 { 22.0 } else { 18.0 }) || (e.home >= 0 && self.holds[e.home as usize].alarm) { e.awake = true; }
            }
            if e.state == EnemyState::Air {
                e.vy -= GRAVITY * dt; e.y += e.vy * dt; e.x += e.vx * dt; e.z += e.vz * dt; e.spin += e.spin_v * dt;
                e.vx *= exp(-dt * 0.8); e.vz *= exp(-dt * 0.8);
                Self::curve_step(e, dt);
                self.hit_mirror(i);
                if self.enemies[i].y <= 0.0 {
                    self.enemies[i].y = 0.0; self.enemies[i].curve = 0.0;
                    let impact = -self.enemies[i].vy;
                    self.land_shock(i, impact);
                    if self.enemies[i].state == EnemyState::Air {
                        let e = &mut self.enemies[i];
                        e.vy = 0.0; e.state = EnemyState::Down; e.st = 0.0; e.spin_v = 0.0;
                    }
                }
                continue;
            }
            e.spin *= exp(-dt * 10.0);
            // 地上の滑り（飴鏡は通り抜けない）
            e.x += e.vx * dt; e.z += e.vz * dt;
            e.vx *= exp(-dt * 7.0); e.vz *= exp(-dt * 7.0);
            if let Some(c) = mirror_contact(e.x, e.z, d.radius) { e.x += c.nx * c.depth; e.z += c.nz * c.depth; }
            match e.state {
                EnemyState::Stagger => { if e.st > 0.38 { e.state = EnemyState::Chase; e.st = 0.0; } }
                EnemyState::Down => { if e.st > (if e.role == Role::Captain { 1.0 } else { 0.75 }) { e.state = EnemyState::Getup; e.st = 0.0; } }
                EnemyState::Getup => { if e.st > 0.35 { e.state = EnemyState::Chase; e.st = 0.0; } }
                EnemyState::Idle => {
                    // 待機: ゆっくり首を振る・近くを歩く
                    if e.awake || e.flee >= 0 { e.state = EnemyState::Chase; e.st = 0.0; }
                }
                EnemyState::Chase => {
                    if e.flee >= 0 {
                        // 逃げる: 逃げ先の砦の中の、敵ごとに少しずらした場所へ（攻撃はしない）
                        let h = &self.holds[e.flee as usize];
                        let (a, rr) = (e.id as f64 * 2.39996, 2.0 + (e.id % 5) as f64 * 1.1);
                        let (tx, tz) = (h.x + sin(a) * rr - e.x, h.z + cos(a) * rr - e.z);
                        let tl = or1(hypot(tx, tz));
                        e.face += clamp(ang_diff(atan2(tx, tz), e.face), -dt * 8.0, dt * 8.0);
                        let sp = d.speed * 1.5 * math::min(1.0, tl / 1.5);
                        e.x += tx / tl * sp * dt; e.z += tz / tl * sp * dt;
                        if tl < 1.2 || hypot(e.x - h.x, e.z - h.z) < 7.0 {
                            e.flee = -1; e.home = h.id; e.awake = h.alarm; e.state = EnemyState::Idle; e.st = 0.0;
                            let cd = self.r(1.0, 3.0);
                            self.enemies[i].cd = cd;
                        }
                        continue;
                    }
                    if !e.awake { e.state = EnemyState::Idle; continue; }
                    e.cd -= dt;
                    let face = atan2(dx, dz);
                    e.face += clamp(ang_diff(face, e.face), -dt * 6.0, dt * 6.0);
                    let speed = d.speed;
                    let mut want_dist = 2.6;
                    if e.role == Role::Archer { want_dist = 9.0; }
                    if e.role == Role::Captain { want_dist = 2.4; }
                    // 攻撃の権利が無い雑兵は、主役のまわりで少し間合いをとる（全員が一度に来ない）
                    if e.role == Role::Grunt && !e.token { want_dist = 2.5 + (e.id % 6) as f64 * 0.42; }
                    if dist > want_dist + 0.3 {
                        let k = math::min(1.0, (dist - want_dist) / 1.5);
                        e.x += dx / dist * speed * k * dt; e.z += dz / dist * speed * k * dt;
                    } else if dist < want_dist - 0.6 && e.role != Role::Captain {
                        e.x -= dx / dist * speed * 0.4 * dt; e.z -= dz / dist * speed * 0.4 * dt;
                    }
                    // 攻撃を始める
                    let can_hit = dist < match e.role { Role::Archer => 16.0, Role::Brute => 11.0, Role::Captain => 6.0, Role::Grunt => 4.0 };
                    if e.cd <= 0.0 && can_hit && self.player.state != PlayerState::Dead {
                        if e.role == Role::Captain || self.attackers < max_attack || e.role == Role::Archer {
                            if e.role != Role::Archer && e.role != Role::Captain { e.token = true; self.attackers += 1; }
                            e.state = EnemyState::Windup; e.st = 0.0; e.aim_x = px; e.aim_z = pz;
                            let (role, ex, ez, eid) = (e.role, e.x, e.z, e.id);
                            if role == Role::Captain {
                                let leap = dist > 4.0 || self.rand.next() < 0.35;
                                self.enemies[i].skill = if leap { EnemySkill::Leap } else { EnemySkill::Sweep };
                                let id = self.new_id();
                                let tel = if leap {
                                    Telegraph { id, kind: TelegraphKind::Circle, x: px, z: pz, r: 4.6, face: 0.0, arc: 0.0, w: 0.0, t: 0.0, fire: 1.15, dmg: d.dmg, src: TelegraphSrc::Captain, done: false, grow: None }
                                } else {
                                    Telegraph { id, kind: TelegraphKind::Circle, x: ex, z: ez, r: 4.2, face: 0.0, arc: 0.0, w: 0.0, t: 0.0, fire: 0.9, dmg: d.dmg, src: TelegraphSrc::Captain, done: false, grow: None }
                                };
                                self.telegraphs.push(tel);
                                self.enemies[i].aim_x = tel.x; self.enemies[i].aim_z = tel.z;
                            } else if role == Role::Brute {
                                let a = atan2(dx, dz);
                                let id = self.new_id();
                                self.telegraphs.push(Telegraph { id, kind: TelegraphKind::Line, x: ex, z: ez, r: 9.0, face: a, arc: 0.0, w: 1.8, t: 0.0, fire: 1.05, dmg: d.dmg, src: TelegraphSrc::Brute, done: false, grow: None });
                                self.enemies[i].face = a;
                            }
                            self.emit(BattleEvent::EnemyWindup { id: eid });
                        } else {
                            let cd = self.r(0.3, 0.9);
                            self.enemies[i].cd = cd;
                        }
                    }
                }
                EnemyState::Windup => {
                    let wt = match e.role { Role::Captain => if e.skill == EnemySkill::Leap { 1.15 } else { 0.9 }, Role::Brute => 1.05, Role::Archer => 0.95, Role::Grunt => 0.72 };
                    if e.role != Role::Brute { e.face += clamp(ang_diff(atan2(dx, dz), e.face), -dt * 3.0, dt * 3.0); }
                    if e.role == Role::Captain && e.skill == EnemySkill::Leap {
                        let k = math::min(1.0, e.st / wt);
                        e.y = sin(k * PI) * 4.5;
                        e.x += (e.aim_x - e.x) * math::min(1.0, dt * 3.2); e.z += (e.aim_z - e.z) * math::min(1.0, dt * 3.2);
                    }
                    if e.st >= wt {
                        e.state = EnemyState::Strike; e.st = 0.0; e.y = 0.0;
                        if e.role == Role::Archer {
                            let a = atan2(dx, dz);
                            let (sp, tt) = (15.0, dist / 15.0);
                            let (lx, lz) = (self.player.x + self.player.vx * 0.0, self.player.z);
                            let (ax, az) = (lx - e.x, lz - e.z);
                            let al = or1(hypot(ax, az));
                            let (ex, ez, eid) = (e.x, e.z, e.id);
                            e.face = a;
                            let id = self.new_id();
                            self.projectiles.push(Projectile { id, x: ex, y: 1.2, z: ez, vx: ax / al * sp, vy: GRAVITY * 0.5 * tt * 0.5 + 1.0, vz: az / al * sp, t: 0.0, dmg: d.dmg, alive: true });
                            self.emit(BattleEvent::Throw { id: eid });
                        } else if e.role == Role::Grunt {
                            let (fx, fz) = (sin(e.face), cos(e.face));
                            e.vx = fx * 6.0; e.vz = fz * 6.0;
                            let (ex, ez, eid) = (e.x, e.z, e.id);
                            // 高く飛んでいる主役（わたあめ雲・飴糸のあと）には届かない
                            if dist < 2.6 && (dx * fx + dz * fz) / or1(dist) > 0.35 && self.player.y < REACH_Y { self.damage_player(d.dmg, ex, ez, false); }
                            self.emit(BattleEvent::EnemyStrike { id: eid, x: ex + fx * 1.2, z: ez + fz * 1.2 });
                        } else if e.role == Role::Brute {
                            e.vx = sin(e.face) * 22.0; e.vz = cos(e.face) * 22.0;
                            let (ex, ez, eid) = (e.x, e.z, e.id);
                            self.emit(BattleEvent::EnemyStrike { id: eid, x: ex, z: ez });
                        }
                        // 隊長の技は予告の円が当たりを出す（telegraph）
                    }
                }
                EnemyState::Strike => {
                    if e.role == Role::Brute && e.st < 0.5 {
                        // 突進を回避ですり抜けたら担ぐ（危ない敵ほどいい弾）
                        if self.player.state == PlayerState::Dodge && self.player.carry == 0 && dist < 2.2 { self.grab(i); continue; }
                        // 突進中: ぶつかったら大きく吹き飛ばす
                        let (ex, ez) = (e.x, e.z);
                        if dist < 1.6 && self.player.y < REACH_Y { self.damage_player(d.dmg, ex, ez, true); }
                        let e = &mut self.enemies[i];
                        e.vx *= exp(dt * 4.5); e.vz *= exp(dt * 4.5);
                    }
                    let e = &mut self.enemies[i];
                    if e.st > (if e.role == Role::Brute { 0.7 } else { 0.35 }) { e.state = EnemyState::Recover; e.st = 0.0; }
                }
                EnemyState::Recover => {
                    if e.st > (if e.role == Role::Captain { 0.8 } else { 0.55 }) {
                        e.state = EnemyState::Chase; e.st = 0.0;
                        let captain = e.role == Role::Captain;
                        let cd = if captain { self.r(1.6, 2.8) } else { self.r(2.2, 4.2) };
                        let e = &mut self.enemies[i];
                        e.cd = cd;
                        if e.token { e.token = false; }
                    }
                }
                _ => {}
            }
        }
        // 押し合い（重なりをほどく）と主役からの押し退け
        self.rebuild();
        let (px, pz, py) = (self.player.x, self.player.z, self.player.y);
        for i in 0..self.enemies.len() {
            let e = &self.enemies[i];
            if !e.alive || e.state == EnemyState::Dead || e.state == EnemyState::Stacked || e.y > 0.5 { continue; }
            if (e.x - px).abs() > 30.0 || (e.z - pz).abs() > 30.0 { continue; }
            let ra = e.kind.def().radius;
            let (ex0, ez0) = (e.x, e.z);
            near_into(&self.grid, &self.enemies, ex0, ez0, 2.6, &mut self.tmp);
            for k in 0..self.tmp.len() {
                let oi = self.tmp[k] as usize;
                let o = &self.enemies[oi];
                if oi == i || o.state == EnemyState::Dead || o.state == EnemyState::Stacked || o.y > 0.5 { continue; }
                let e = &self.enemies[i];
                let (dx, dz) = (o.x - e.x, o.z - e.z);
                let dd = dx * dx + dz * dz;
                let rr = ra + o.kind.def().radius;
                if dd < rr * rr && dd > 1e-6 {
                    let dd2 = dd.sqrt();
                    let push = (rr - dd2) * 0.5;
                    let e = &mut self.enemies[i];
                    e.x -= dx / dd2 * push; e.z -= dz / dd2 * push;
                    let o = &mut self.enemies[oi];
                    o.x += dx / dd2 * push; o.z += dz / dd2 * push;
                }
            }
            let e = &mut self.enemies[i];
            {
                let (dx, dz) = (e.x - px, e.z - pz);
                let dd = hypot(dx, dz);
                let rr = ra + 0.55;
                if dd < rr && dd > 1e-6 && py < 1.2 { e.x = px + dx / dd * rr; e.z = pz + dz / dd * rr; }
            }
            let r0 = hypot(e.x, e.z);
            if r0 > FIELD_R - 1.0 { e.x *= (FIELD_R - 1.0) / r0; e.z *= (FIELD_R - 1.0) / r0; }
        }
    }

    // ---- 砦・増援 ----
    fn step_holds(&mut self, dt: f64) {
        let alive = self.alive_count();
        self.stats.peak_alive = self.stats.peak_alive.max(alive);
        for hi in 0..self.holds.len() {
            let h = self.holds[hi];
            if h.owner == Owner::Ally { continue; }
            let (px, pz) = (self.player.x, self.player.z);
            let dist = hypot(px - h.x, pz - h.z);
            if !h.alarm && dist < h.r + 4.0 {
                self.holds[hi].alarm = true;
                let kind = if h.id % 2 == 0 { EnemyKind::Shortcake } else { EnemyKind::Cakeking };
                let c = self.spawn(kind, h.x + (px - h.x) * -0.2, h.z + (pz - h.z) * -0.2, h.id, true);
                self.enemies[c].face = atan2(px - self.enemies[c].x, pz - self.enemies[c].z);
                let cid = self.enemies[c].id;
                self.holds[hi].captain = cid;
                self.squad(h.x, h.z, 10, h.id, true, Some(HOLD_KIND[h.id as usize]), false);
                self.emit(BattleEvent::CaptainAppear { id: cid, hold: h.id });
            }
            self.holds[hi].spawn_t -= dt;
            if self.holds[hi].spawn_t <= 0.0 {
                let alarm = self.holds[hi].alarm;
                self.holds[hi].spawn_t = if alarm { 2.6 } else { 5.5 };
                if alive < MAX_ALIVE && dist < 60.0 {
                    let a = atan2(px - h.x, pz - h.z);
                    self.squad(h.x + sin(a) * 4.0, h.z + cos(a) * 4.0, if alarm { 5 } else { 4 }, h.id, dist < 40.0, Some(HOLD_KIND[h.id as usize]), false);
                }
            }
        }
        // 本陣からの増援（前へ進むほど、まわりの数を保つ）
        self.reinforce_t -= dt;
        if self.reinforce_t <= 0.0 {
            let boss_phase = self.phase == Phase::Boss;
            self.reinforce_t = if boss_phase { 6.0 } else { 4.5 };
            let (px, pz) = (self.player.x, self.player.z);
            let mut nearby = 0;
            for e in &self.enemies { if e.alive && e.state != EnemyState::Dead && (e.x - px).abs() < 24.0 && (e.z - pz).abs() < 24.0 { nearby += 1; } }
            if nearby < (if boss_phase { 30 } else { 90 }) && alive < MAX_ALIVE {
                let a = self.r(0.0, PI * 2.0);
                let (sx, sz) = (clamp(px + sin(a) * 21.0, -FIELD_R + 6.0, FIELD_R - 6.0), clamp(pz + cos(a) * 21.0, -FIELD_R + 6.0, FIELD_R - 6.0));
                self.squad(sx, sz, if boss_phase { 8 } else { 20 }, -1, true, None, false);
            }
        }
    }

    // ---- 大将 ----
    fn step_boss(&mut self, dt: f64) {
        let (px, pz) = (self.player.x, self.player.z);
        if self.boss.state == BossState::Dormant {
            if self.gate_open && hypot(px - GATE.x, pz - GATE.z) < 30.0 {
                self.boss.state = BossState::Enter; self.boss.st = 0.0; self.phase = Phase::Boss; self.stats.start_boss_at = self.time;
                // 門から跳び出し、主役の 13m 手前へ着地する
                let a = atan2(self.boss.x - px, self.boss.z - pz);
                self.leap = Leap { x0: self.boss.x, z0: self.boss.z, x1: px + sin(a) * 13.0, z1: pz + cos(a) * 13.0, landed: false };
                self.boss.face = atan2(px - self.boss.x, pz - self.boss.z);
                self.emit(BattleEvent::BossEnter);
            }
            return;
        }
        self.boss.hit_t += dt;
        self.boss.st += dt;
        if self.boss.state == BossState::Dead { return; }
        if self.ranbu_freeze > 0.0 { return; }
        let (dx, dz) = (px - self.boss.x, pz - self.boss.z);
        let dist = hypot(dx, dz);
        let face_to = atan2(dx, dz);
        match self.boss.state {
            BossState::Enter => {
                let k = math::min(1.0, self.boss.st / LEAP_T);
                let l = self.leap;
                self.boss.x = l.x0 + (l.x1 - l.x0) * k; self.boss.z = l.z0 + (l.z1 - l.z0) * k;
                self.boss_y = sin(k * PI) * 14.0;
                self.boss.face = atan2(px - self.boss.x, pz - self.boss.z);
                if k >= 1.0 && !self.leap.landed {
                    self.leap.landed = true; self.boss_y = 0.0;
                    // 着地の衝撃: まわりの雑兵を吹き飛ばし、主役は押し返すだけ（傷は負わない）
                    let (bx, bz) = (self.boss.x, self.boss.z);
                    self.near_tmp(bx, bz, 18.0);
                    for n in 0..self.tmp.len() {
                        let i = self.tmp[n] as usize;
                        let e = &self.enemies[i];
                        if e.state == EnemyState::Dead || e.role == Role::Captain { continue; }
                        let (ex, ez) = (e.x - bx, e.z - bz);
                        let el = or1(hypot(ex, ez));
                        let vx = ex / el * self.r(10.0, 16.0);
                        let vz = ez / el * self.r(10.0, 16.0);
                        let vy = self.r(8.0, 13.0);
                        let sv = self.r(6.0, 12.0);
                        let e = &mut self.enemies[i];
                        e.vx = vx; e.vz = vz; e.vy = vy; e.hp = 0.0; e.state = EnemyState::Dead; e.dead_t = 0.0; e.spin_v = sv;
                    }
                    {
                        let (dx2, dz2) = (self.player.x - bx, self.player.z - bz);
                        let dl = or1(hypot(dx2, dz2));
                        if dl < 9.0 && self.player.state != PlayerState::Dead {
                            self.leave_ride(false);
                            let p = &mut self.player;
                            p.state = PlayerState::Hurt; p.st = 0.0; p.hurt_t = 0.5; p.r#move = None; p.vx = dx2 / dl * 10.0; p.vz = dz2 / dl * 10.0;
                        }
                    }
                    self.emit(BattleEvent::BossLand { x: bx, z: bz });
                }
                if self.boss.st > LEAP_T + 2.2 { self.boss.state = BossState::Idle; self.boss.st = 0.0; self.boss.cd = 1.0; }
            }
            BossState::Down => {
                if self.boss.st > 3.6 { self.boss.state = BossState::Roar; self.boss.st = 0.0; self.emit(BattleEvent::BossAttack { kind: BossState::Roar }); }
            }
            BossState::Roar => {
                if self.giant.on && self.giant.t < 2.4 { return; } // 巨人が組み上がるまで吠え続ける
                if self.boss.st > 0.3 && self.boss.st - dt <= 0.3 {
                    // 吠える: まわりを吹き飛ばす（近すぎると押し返される）
                    let (bx, bz) = (self.boss.x, self.boss.z);
                    if hypot(self.player.x - bx, self.player.z - bz) < 7.0 { self.damage_player(60.0, bx, bz, true); }
                }
                if self.boss.st > 1.6 { self.boss.state = BossState::Idle; self.boss.st = 0.0; self.boss.cd = 0.6; }
            }
            BossState::Idle | BossState::Walk => {
                if !self.giant.formed && self.boss.hp < self.boss.max_hp * 0.5 { self.form_giant(); return; }
                self.boss.face += clamp(ang_diff(face_to, self.boss.face), -dt * 2.2, dt * 2.2);
                self.boss.cd -= dt;
                if dist > 6.2 {
                    self.boss.state = BossState::Walk;
                    let sp = if self.giant.on { 2.4 } else { 3.4 };
                    self.boss.x += sin(self.boss.face) * sp * dt; self.boss.z += cos(self.boss.face) * sp * dt;
                } else { self.boss.state = BossState::Idle; }
                if self.boss.cd <= 0.0 {
                    let hp_k = self.boss.hp / self.boss.max_hp;
                    let roll = self.rand.next();
                    let pick;
                    // 巨人は雑兵の腕で遠くまで殴る（壊れた腕では殴らない）
                    let mut arms: [BossState; 2] = [BossState::PunchL, BossState::PunchR];
                    let mut arms_len = 2usize;
                    if self.giant.on {
                        arms_len = 0;
                        for (i, s) in [BossState::PunchL, BossState::PunchR].into_iter().enumerate() { if !self.giant.arm_gone[i] { arms[arms_len] = s; arms_len += 1; } }
                    }
                    if dist < (if self.giant.on { 12.0 } else { 8.0 }) && roll < 0.55 && arms_len > 0 { pick = arms[(self.boss.combo as usize) % arms_len]; }
                    else if roll < 0.78 || dist < 10.0 { pick = BossState::Stomp; }
                    else { pick = BossState::Sauce; }
                    self.boss.state = pick; self.boss.st = 0.0; self.boss.combo += 1;
                    let fast = if hp_k < 0.4 { 0.8 } else { 1.0 };
                    let (bx, bz) = (self.boss.x, self.boss.z);
                    if pick == BossState::PunchL || pick == BossState::PunchR {
                        let id = self.new_id();
                        let on = self.giant.on;
                        self.telegraphs.push(Telegraph { id, kind: TelegraphKind::Cone, x: bx, z: bz, r: if on { 12.5 } else { 9.0 }, face: face_to, arc: if on { 0.42 } else { 0.62 }, w: 0.0, t: 0.0, fire: 0.95 * fast, dmg: 170.0, src: TelegraphSrc::Boss, done: false, grow: None });
                        self.giant.punch_fire = 0.95 * fast;
                        self.boss.face = face_to;
                    } else if pick == BossState::Stomp {
                        let id = self.new_id();
                        self.telegraphs.push(Telegraph { id, kind: TelegraphKind::Ring, x: bx, z: bz, r: 18.0, face: 0.0, arc: 0.0, w: 2.2, t: 0.0, fire: 1.35 * fast, dmg: 140.0, src: TelegraphSrc::Boss, done: false, grow: Some(1.1) });
                    } else {
                        let n = if hp_k < 0.5 { 9 } else { 6 };
                        for i in 0..n {
                            let a = self.r(0.0, PI * 2.0);
                            let r = if i == 0 { 0.0 } else { self.r(2.0, 9.0) };
                            let id = self.new_id();
                            self.telegraphs.push(Telegraph { id, kind: TelegraphKind::Circle, x: px + sin(a) * r, z: pz + cos(a) * r, r: 2.8, face: 0.0, arc: 0.0, w: 0.0, t: -(i as f64) * 0.12, fire: 1.5, dmg: 110.0, src: TelegraphSrc::Sauce, done: false, grow: None });
                        }
                        // たれを撒くと、足軽が加勢に来る
                        if self.alive_count() < MAX_ALIVE { let sx = bx + self.r(-6.0, 6.0); self.squad(sx, bz + 8.0, 5, -1, true, None, false); }
                    }
                    self.emit(BattleEvent::BossAttack { kind: pick });
                }
            }
            BossState::PunchL | BossState::PunchR => {
                if self.boss.st > 1.9 {
                    self.boss.state = BossState::Idle; self.boss.st = 0.0;
                    let v = self.r(0.6, 1.4) * if self.boss.hp / self.boss.max_hp < 0.4 { 0.6 } else { 1.0 };
                    self.boss.cd = v;
                }
            }
            BossState::Stomp => { if self.boss.st > 2.6 { self.boss.state = BossState::Idle; self.boss.st = 0.0; self.boss.cd = self.r(0.8, 1.6); } }
            BossState::Sauce => { if self.boss.st > 2.4 { self.boss.state = BossState::Idle; self.boss.st = 0.0; self.boss.cd = self.r(1.0, 1.8); } }
            BossState::Hit => { if self.boss.st > 0.4 { self.boss.state = BossState::Idle; } }
            _ => {}
        }
    }

    // ---- 巨人 ----
    fn form_giant(&mut self) {
        self.giant.formed = true; self.giant.on = true; self.giant.t = 0.0; self.giant.arm_gone = [false, false]; self.giant.from.clear(); self.giant.wire_hits = 0;
        for t in &mut self.telegraphs { if t.src == TelegraphSrc::Boss { t.done = true; } }
        self.boss.state = BossState::Roar; self.boss.st = 0.0;
        let (bx, bz) = (self.boss.x, self.boss.z);
        // 近い雑兵から呼び寄せる（足りなければ戦場の外から来る）
        let mut pool: Vec<usize> = (0..self.enemies.len()).filter(|&k| { let e = &self.enemies[k]; e.alive && e.role == Role::Grunt && e.state != EnemyState::Dead && e.state != EnemyState::Air && e.state != EnemyState::Carried && hypot(e.x - bx, e.z - bz) < 45.0 }).collect();
        pool.sort_by(|&a, &b| {
            let (ea, eb) = (&self.enemies[a], &self.enemies[b]);
            hypot(ea.x - bx, ea.z - bz).partial_cmp(&hypot(eb.x - bx, eb.z - bz)).unwrap()
        });
        let mut slots = Vec::with_capacity(GIANT_SLOTS.len());
        for i in 0..GIANT_SLOTS.len() {
            let ei = match pool.get(i) {
                Some(&e) => e,
                None => {
                    let a = self.r(0.0, PI * 2.0);
                    self.spawn(GRUNT_KINDS[i % GRUNT_KINDS.len()], clamp(bx + sin(a) * 26.0, -FIELD_R + 4.0, FIELD_R - 4.0), clamp(bz + cos(a) * 26.0, -FIELD_R + 4.0, FIELD_R - 4.0), -1, true)
                }
            };
            // 積み重なった雑兵は固い（体力 150）
            let e = &mut self.enemies[ei];
            e.state = EnemyState::Stacked; e.st = 0.0; e.token = false; e.flee = -1; e.fling = 0; e.vx = 0.0; e.vy = 0.0; e.vz = 0.0; e.awake = true; e.hp = 150.0; e.max_hp = 150.0;
            self.giant.from.push([e.x, e.y, e.z]);
            slots.push(Some(ei));
        }
        self.giant.slots = slots;
        self.emit(BattleEvent::GiantForm { x: bx, z: bz });
    }
    /// 積み重なった雑兵を斬った: 脚と腰には斬りが効かない（敵をぶつけて崩す）。腕は普通に効き、倒れたら外れて落ちる
    fn hit_stacked(&mut self, i: usize, dmg: f64, dir_x: f64, dir_z: f64) -> bool {
        let part = self.giant.slots.iter().position(|s| *s == Some(i)).map(|n| GIANT_SLOTS[n].part);
        let e = &mut self.enemies[i];
        e.hp -= dmg * if part == Some(GiantPart::ArmL) || part == Some(GiantPart::ArmR) { 1.0 } else { 0.0 };
        e.hit_t = 0.0;
        if e.hp > 0.0 { return false; }
        e.state = EnemyState::Dead; e.st = 0.0; e.dead_t = 0.0; e.vx = dir_x * 6.0; e.vz = dir_z * 6.0;
        let vy = self.r(3.0, 6.0);
        let sv = self.r(8.0, 14.0);
        let e = &mut self.enemies[i];
        e.vy = vy; e.spin_v = sv;
        let ev = BattleEvent::Kill { id: e.id, kind: e.kind, x: e.x, y: e.y, z: e.z, captain: false };
        self.player.kos += 1; self.gain_ame(self.enemies[i].kind);
        self.emit(ev);
        self.drop_from(i);
        true
    }
    /// 積み重なった雑兵を、生きたまま地上へ降らせる
    fn rain(&mut self, i: usize) {
        let a = atan2(self.enemies[i].x - self.boss.x, self.enemies[i].z - self.boss.z) + self.r(-0.4, 0.4);
        let sp = self.r(2.0, 6.0);
        let vy = self.r(2.0, 6.0);
        let sv = self.r(6.0, 12.0);
        let e = &mut self.enemies[i];
        e.state = EnemyState::Air; e.st = 0.0; e.vy = vy; e.vx = sin(a) * sp; e.vz = cos(a) * sp; e.spin_v = sv; e.fling = 0; e.awake = true;
    }
    /// 腕の形（0 = 下ろしている、1 = 振り上げ、2 = 地面へ振り下ろした）を、拳の構えの時刻から
    fn arm_pose(&self, side: f64) -> f64 {
        let (st, f) = (self.boss.st, self.giant.punch_fire);
        if self.boss.state != (if side < 0.0 { BossState::PunchL } else { BossState::PunchR }) { return 0.0; }
        if st < 0.55 { return st / 0.55; }
        if st < f { return 1.0 + (st - 0.55) / math::max(0.05, f - 0.55); }
        if st < 1.5 { return 2.0; }
        math::max(0.0, 2.0 - (st - 1.5) / 0.4 * 2.0)
    }
    fn step_giant(&mut self, dt: f64) {
        if !self.giant.on {
            if self.boss_y > 0.0 && self.boss.state != BossState::Enter { self.boss_y = math::max(0.0, self.boss_y - dt * 12.0); }
            return;
        }
        if self.boss.state == BossState::Dead { self.giant.on = false; return; }
        self.giant.t += dt;
        let mut count = [0i32; 5];
        for (i, s) in self.giant.slots.iter().enumerate() {
            if let Some(e) = *s { let e = &self.enemies[e]; if e.alive && e.state == EnemyState::Stacked { count[GIANT_SLOTS[i].part as usize] += 1; } }
        }
        let formed = self.giant.t > 2.4;
        // 片方の脚が半分外れたら崩れる
        // 飴糸で頭上の大将を WIRE_TOPPLE 回斬っても崩れる
        if formed && (count[GiantPart::LegL as usize] as f64 <= GIANT_COUNT[0] as f64 / 2.0 || count[GiantPart::LegR as usize] as f64 <= GIANT_COUNT[1] as f64 / 2.0 || self.giant.wire_hits >= WIRE_TOPPLE) {
            self.giant.on = false;
            for k in 0..self.giant.slots.len() {
                if let Some(e) = self.giant.slots[k] { if self.enemies[e].alive && self.enemies[e].state == EnemyState::Stacked { self.rain(e); } }
            }
            self.boss.state = BossState::Down; self.boss.st = -1.4; self.boss.poise = 1400.0;
            for t in &mut self.telegraphs { if t.src == TelegraphSrc::Boss { t.done = true; } }
            let (bx, bz) = (self.boss.x, self.boss.z);
            self.emit(BattleEvent::GiantFall { x: bx, z: bz });
            return;
        }
        // 腕の雑兵が 6 体倒れたら腕が壊れ、残りが降ってくる
        for (s, part) in [GiantPart::ArmL, GiantPart::ArmR].into_iter().enumerate() {
            if self.giant.arm_gone[s] || count[part as usize] > GIANT_COUNT[part as usize] - 6 { continue; }
            self.giant.arm_gone[s] = true;
            let (mut x, mut z, mut n) = (0.0, 0.0, 0);
            for k in 0..self.giant.slots.len() {
                if let Some(ei) = self.giant.slots[k] {
                    let e = &self.enemies[ei];
                    if e.alive && e.state == EnemyState::Stacked && GIANT_SLOTS[k].part == part { x += e.x; z += e.z; n += 1; self.rain(ei); }
                }
            }
            let ev = BattleEvent::GiantArm { side: if s == 0 { -1 } else { 1 }, x: if n != 0 { x / n as f64 } else { self.boss.x }, z: if n != 0 { z / n as f64 } else { self.boss.z } };
            self.emit(ev);
        }
        // 体の形: 脚は残った雑兵で下から詰め直す。腰は短いほうの脚の上、大将はその上に立つ
        let rows = (count[0].min(count[1]) as f64 / 2.0).ceil();
        let hip_y = 0.4 + rows * LEG_ROW;
        let lift = hip_y + 0.45;
        let fk = math::min(1.0, self.giant.t / 2.2);
        self.boss_y = lift * fk * fk * (3.0 - 2.0 * fk);
        let (cf, sf) = (cos(self.boss.face), sin(self.boss.face));
        let walk = if self.boss.state == BossState::Walk { sin(self.boss.st * 5.0) } else { 0.0 };
        let mut rank = [0i32; 5];
        let lerp = |a: f64, b: f64, k: f64| a + (b - a) * k;
        for i in 0..self.giant.slots.len() {
            let Some(ei) = self.giant.slots[i] else { continue };
            { let e = &self.enemies[ei]; if !e.alive || e.state != EnemyState::Stacked { continue; } }
            let GiantSlot { part, i: j } = GIANT_SLOTS[i];
            let (lx, ly, lz);
            if part == GiantPart::LegL || part == GiantPart::LegR {
                let side = if part == GiantPart::LegL { -1.0 } else { 1.0 };
                let k = rank[part as usize]; rank[part as usize] += 1;
                let row = (k as f64 / 2.0).floor();
                lx = side * 1.9 + if k % 2 != 0 { 0.5 } else { -0.5 };
                ly = 0.4 + row * LEG_ROW;
                lz = (row % 2.0) * 0.15 + walk * side * 0.35 * (row / 6.0);
            } else if part == GiantPart::Hip {
                let a = (j as f64 / GIANT_COUNT[part as usize] as f64) * PI * 2.0;
                lx = sin(a) * 1.9; lz = cos(a) * 1.2; ly = hip_y;
            } else {
                // 腕: 肩から手先へ並ぶ。下ろす → 振り上げ → 地面へ振り下ろす
                let side = if part == GiantPart::ArmL { -1.0 } else { 1.0 };
                let k = self.arm_pose(side);
                let u = (j + 1) as f64 / GIANT_COUNT[part as usize] as f64;
                let (sx, sy, sz) = (side * 2.4, self.boss_y + 4.6, 0.0);
                let rest = [side * 3.8, self.boss_y + 0.9, 0.8];
                let up = [side * 2.4, self.boss_y + 8.6, -1.2];
                let slam = [side * 0.9, 0.5, 8.5];
                let hand: [f64; 3] = if k <= 1.0 { std::array::from_fn(|n| lerp(rest[n], up[n], k)) } else { std::array::from_fn(|n| lerp(up[n], slam[n], k - 1.0)) };
                lx = lerp(sx, hand[0], u);
                ly = lerp(sy, hand[1], u) + sin(u * PI) * 0.6;
                lz = lerp(sz, hand[2], u);
            }
            let (wx, wz) = (self.boss.x + lx * cf + lz * sf, self.boss.z - lx * sf + lz * cf);
            // 組み上がる: 元の場所から放物線で跳んでくる（近い順に少しずつ）
            let delay = 0.15 + i as f64 * 0.03;
            let k = math::min(1.0, math::max(0.0, (self.giant.t - delay) / 0.7));
            let face = self.boss.face;
            let f = self.giant.from[i];
            let e = &mut self.enemies[ei];
            if k < 1.0 {
                e.x = lerp(f[0], wx, k); e.z = lerp(f[2], wz, k); e.y = lerp(f[1], ly, k) + sin(k * PI) * 6.0;
            } else { e.x = wx; e.z = wz; e.y = ly; }
            e.face = face; e.st += dt;
        }
    }

    fn step_telegraphs(&mut self, dt: f64) {
        for ti in 0..self.telegraphs.len() {
            if self.telegraphs[ti].done { continue; }
            if self.ranbu_freeze > 0.0 { continue; }
            self.telegraphs[ti].t += dt;
            let t = self.telegraphs[ti];
            if t.t < t.fire { continue; }
            // 当たりの判定
            if t.kind == TelegraphKind::Ring {
                // 外へ広がる波: 発射から grow 秒で r まで。地上にいて、波の帯に入ったら当たる
                let k = (t.t - t.fire) / t.grow.unwrap_or(1.0);
                let rr = k * t.r;
                if t.t - dt < t.fire { self.emit(BattleEvent::TelegraphFire { tel: t }); }
                {
                    let (px, pz, py) = (self.player.x, self.player.z, self.player.y);
                    let d = hypot(px - t.x, pz - t.z);
                    if (d - rr).abs() < t.w && py < 0.6 { self.damage_player(t.dmg, t.x, t.z, true); self.telegraphs[ti].done = true; }
                }
                if k >= 1.0 { self.telegraphs[ti].done = true; }
                continue;
            }
            self.emit(BattleEvent::TelegraphFire { tel: t });
            self.telegraphs[ti].done = true;
            {
                let (px, pz, py) = (self.player.x, self.player.z, self.player.y);
                let (dx, dz) = (px - t.x, pz - t.z);
                let d = hypot(dx, dz);
                let mut hit = false;
                if t.kind == TelegraphKind::Circle { hit = d < t.r + 0.4; }
                else if t.kind == TelegraphKind::Cone { hit = d < t.r && ang_diff(atan2(dx, dz), t.face).abs() < t.arc + 0.1; }
                else if t.kind == TelegraphKind::Line { let (fx, fz) = (sin(t.face), cos(t.face)); let (along, side) = (dx * fx + dz * fz, (dx * fz - dz * fx).abs()); hit = along > -1.0 && along < t.r && side < t.w / 2.0 + 0.4; }
                if t.src == TelegraphSrc::Brute { hit = false; } // 突進そのものが当たり
                if hit && py < 1.4 { self.damage_player(t.dmg, t.x, t.z, t.src != TelegraphSrc::Sauce); }
            }
            if t.src == TelegraphSrc::Boss && t.kind == TelegraphKind::Cone {
                // 拳は雑兵も巻き込む
                self.near_tmp(t.x, t.z, t.r);
                let (fs, fc) = (sin(t.face), cos(t.face));
                for n in 0..self.tmp.len() {
                    let i = self.tmp[n] as usize;
                    let e = &mut self.enemies[i];
                    let (ex, ez) = (e.x - t.x, e.z - t.z);
                    if e.state != EnemyState::Dead && e.state != EnemyState::Stacked && e.state != EnemyState::Carried && ang_diff(atan2(ex, ez), t.face).abs() < t.arc {
                        e.vy = 7.0; e.state = EnemyState::Air; e.vx = fs * 8.0; e.vz = fc * 8.0;
                    }
                }
            }
            if t.src == TelegraphSrc::Captain { self.emit(BattleEvent::BossStomp { x: t.x, z: t.z }); }
        }
        self.telegraphs.retain(|t| !(t.done && t.t > t.fire + 0.05));
    }

    fn step_projectiles(&mut self, dt: f64) {
        for k in 0..self.projectiles.len() {
            if !self.projectiles[k].alive { continue; }
            if self.ranbu_freeze > 0.0 { continue; }
            let b = &mut self.projectiles[k];
            b.t += dt; b.vy -= GRAVITY * 0.5 * dt;
            b.x += b.vx * dt; b.y += b.vy * dt; b.z += b.vz * dt;
            if self.player.state != PlayerState::Dead {
                let (px, pz, py) = (self.player.x, self.player.z, self.player.y);
                let b = &mut self.projectiles[k];
                if hypot(b.x - px, b.z - pz) < 0.9 && (b.y - (py + 1.0)).abs() < 1.2 {
                    b.alive = false;
                    let (dmg, fx, fz) = (b.dmg, b.x - b.vx, b.z - b.vz);
                    self.damage_player(dmg, fx, fz, false);
                }
            }
            let b = &mut self.projectiles[k];
            if b.y < 0.0 || b.t > 3.0 { b.alive = false; }
        }
        self.projectiles.retain(|b| b.alive);
        for k in &mut self.pickups { if k.alive { k.t += dt; if k.t > 30.0 { k.alive = false; } } }
        self.pickups.retain(|k| k.alive);
    }

    #[inline] pub fn time_scale(&self) -> f64 { if self.slow > 0.0 { self.slow_scale } else { 1.0 } }
    pub fn set_path(&mut self, p: Path) { self.path = p; }

    /// 1 フレーム進める（dt は実時間の秒）。ヒットストップ・スローもここで扱う
    pub fn step(&mut self, dt_real: f64, input: Input) {
        let dt_r = math::min(dt_real, 1.0 / 20.0);
        if self.slow > 0.0 { self.slow -= dt_r; }
        if self.hitstop > 0.0 { self.hitstop -= dt_r; return; }
        let dt = dt_r * if self.slow > 0.0 { self.slow_scale } else { 1.0 };
        if self.phase == Phase::Victory {
            self.victory_t += dt_r;
            self.step_enemies(dt);
            self.step_player(dt, NO_INPUT);
            if self.victory_t > 3.5 && self.victory_t - dt_r <= 3.5 { self.emit(BattleEvent::Victory); }
            return;
        }
        if self.phase == Phase::Defeat {
            self.step_enemies(dt);
            return;
        }
        self.time += dt;
        if self.ranbu_freeze > 0.0 { self.ranbu_freeze -= dt; }
        if self.ranbu_freeze > 0.0 {
            // 切り込みの間は主役だけが動く（敵・弾は止まる）
            self.step_player(dt, NO_INPUT);
            return;
        }
        self.step_player(dt, input);
        self.step_enemies(dt);
        self.step_chains(dt);
        self.step_holds(dt);
        self.step_boss(dt);
        self.step_giant(dt);
        self.step_telegraphs(dt);
        self.step_projectiles(dt);
        if self.player.kos > self.merit_kos { let k = self.player.kos - self.merit_kos; self.add_merit(k); self.merit_kos = self.player.kos; }
    }
    /// 敗れたあと、その場から続ける
    pub fn revive(&mut self) {
        if self.phase != Phase::Defeat { return; }
        self.continues += 1;
        { let p = &mut self.player; p.hp = p.max_hp; p.ranbu = 100.0; p.state = PlayerState::Free; p.invuln = 3.0; p.r#move = None; }
        let (px, pz, pface) = (self.player.x, self.player.z, self.player.face);
        // 立ち上がりの気合で、まわりの敵を吹き飛ばす
        self.near_tmp(px, pz, 10.0);
        for n in 0..self.tmp.len() {
            let i = self.tmp[n] as usize;
            let e = &self.enemies[i];
            if e.state == EnemyState::Dead || e.role == Role::Captain { continue; }
            let (ex, ez) = (e.x - px, e.z - pz);
            let el = or1(hypot(ex, ez));
            let sv = self.r(6.0, 12.0);
            let e = &mut self.enemies[i];
            e.vx = ex / el * 12.0; e.vz = ez / el * 12.0; e.vy = 8.0; e.state = EnemyState::Air; e.spin_v = sv; e.token = false;
        }
        self.emit(BattleEvent::Impact { x: px, z: pz, fx: HitFx::Ring, r: 8.0, face: pface, shape: HitShape::Circle, len: 8.0 });
        self.ranbu_ready_sent = false;
        self.phase = if self.boss.state != BossState::Dormant && self.boss.state != BossState::Dead { Phase::Boss } else { Phase::Battle };
    }
    /// 段位と得点
    pub fn result(&self) -> BattleResult {
        let t = self.time;
        let (kos, max_combo, dmg_taken) = (self.player.kos, self.player.max_combo, self.player.dmg_taken);
        let score = kos as f64 * 100.0 + self.stats.captures as f64 * 5000.0 + if self.boss.state == BossState::Dead { 30000.0 } else { 0.0 } + max_combo as f64 * 50.0 + self.chain_bonus
            + math::max(0.0, math::round((600.0 - t) * 40.0)) - self.continues as f64 * 15000.0 - math::round(dmg_taken * 2.0);
        // 段位の境目は連鎖の得点（ふつうの腕前で 1 戦 3〜5 万点）のぶん上げてある
        let rank = if score >= 270000.0 { "S" } else if score >= 215000.0 { "A" } else if score >= 160000.0 { "B" } else { "C" };
        let won = self.boss.state == BossState::Dead;
        // 誉れ（武将ごとに集める目標）
        let honors = [won && kos >= 1000, won && t <= 300.0, won && dmg_taken <= 1500.0, max_combo >= 300, self.max_chain >= 100];
        BattleResult { score: math::max(0.0, score), rank, time: t, kos, max_combo, max_chain: self.max_chain, chain_bonus: self.chain_bonus, captures: self.stats.captures, dmg_taken, continues: self.continues, won, honors, rank_reached: self.player.rank }
    }
    fn debug_hit(dmg: f64) -> Hit { Hit { t: 0.0, shape: HitShape::Circle, r: 1.0, arc: None, w: None, off: None, dmg, kb: Knock::Push, power: 1.0, stop: 0.0, shake: 0.0, fx: HitFx::Slash } }
    /// 確かめ用: 砦をすぐ落とす
    pub fn debug_capture(&mut self, i: usize) {
        if i < self.holds.len() && self.holds[i].owner == Owner::Enemy {
            let (x, z) = (self.holds[i].x, self.holds[i].z);
            let c = self.spawn(EnemyKind::Shortcake, x, z, i as i32, true);
            self.enemies[c].hp = 1.0;
            self.holds[i].alarm = true;
            self.holds[i].captain = self.enemies[c].id;
            self.hit_enemy(c, &Self::debug_hit(99.0), 1.0, 0.0, 999999 + i as i64);
        }
    }
    pub fn debug_teleport(&mut self, x: f64, z: f64) { self.player.x = x; self.player.z = z; }
    pub fn debug_ranbu(&mut self) { self.player.ranbu = 100.0; }
    /// 確かめ用: 敵を置く（起きていない）。返すのは enemies のインデックス
    pub fn debug_spawn(&mut self, kind: EnemyKind, x: f64, z: f64) -> usize { self.spawn(kind, x, z, -1, false) }
    /// 確かめ用: 敵を弾として飛ばす
    pub fn debug_fling(&mut self, i: usize, vx: f64, vz: f64, vy: f64) {
        let chain = self.debug_chain; self.debug_chain += 1;
        { let e = &mut self.enemies[i]; e.vx = vx; e.vz = vz; e.vy = vy; e.state = EnemyState::Air; e.fling = chain; e.gen = 0; }
        let ammo = self.ammo_of(i);
        let e = &mut self.enemies[i];
        e.ammo = ammo; e.chain_hit = chain; e.mirror = 0; e.curve = 0.0;
    }
    /// 確かめ用: 敵に直接傷を与える
    pub fn debug_hit_enemy(&mut self, i: usize, dmg: f64) {
        let key = 900000 + self.debug_hits; self.debug_hits += 1;
        self.hit_enemy(i, &Self::debug_hit(dmg), 1.0, 0.0, key);
    }
    /// 確かめ用: 武勲を足す
    pub fn debug_merit(&mut self, n: i64) { self.add_merit(n); }
}
