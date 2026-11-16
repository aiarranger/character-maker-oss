// 出来事の名前（画面の確かめ用の記録 ev_log が使う）。
use crate::battle::BattleEvent;

pub fn event_name(e: &BattleEvent) -> &'static str {
    use BattleEvent::*;
    match e {
        Move { .. } => "move", Swing { .. } => "swing", Hit { .. } => "hit", Impact { .. } => "impact", Kill { .. } => "kill", Vanish { .. } => "vanish", Hurt { .. } => "hurt",
        EnemyWindup { .. } => "enemyWindup", EnemyStrike { .. } => "enemyStrike", Throw { .. } => "throw", Jump => "jump", Land { .. } => "land", Dodge => "dodge",
        RanbuStart => "ranbuStart", RanbuFinish => "ranbuFinish", RanbuReady => "ranbuReady", CaptainAppear { .. } => "captainAppear", Capture { .. } => "capture",
        BossGate => "bossGate", BossEnter => "bossEnter", BossAttack { .. } => "bossAttack", BossStomp { .. } => "bossStomp", BossDown => "bossDown", BossDefeated => "bossDefeated", BossLand { .. } => "bossLand",
        Milestone { .. } => "milestone", Pickup { .. } => "pickup", TelegraphFire { .. } => "telegraphFire", Victory => "victory", Defeat => "defeat", Combo { .. } => "combo",
        Chain { .. } => "chain", ChainEnd { .. } => "chainEnd", Detonate { .. } => "detonate", CaptainBreak { .. } => "captainBreak", Grab { .. } => "grab", Hurl { .. } => "hurl", HurlBoom { .. } => "hurlBoom",
        GiantForm { .. } => "giantForm", GiantArm { .. } => "giantArm", GiantFall { .. } => "giantFall", RankUp { .. } => "rankUp", Reflect { .. } => "reflect",
        Wire { .. } => "wire", WireStrike { .. } => "wireStrike", Inoshikacho { .. } => "inoshikacho", Ame { .. } => "ame", NoAme => "noAme", Ride => "ride", RideEnd { .. } => "rideEnd",
    }
}
