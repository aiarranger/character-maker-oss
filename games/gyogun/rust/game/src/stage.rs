// 面（docs/gyogun-spec.md）。面は浜辺 1 つ。遊びの計算（sim）の敵の種類（Strawberry など）はそのままで、見た目と名前だけが魚。
use sim::battle::EnemyKind;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Stage {
    /// 夕暮れの浜辺（魚の大群）
    #[default]
    Hama,
}

pub const STAGES: [Stage; 1] = [Stage::Hama];
/// 今の面
pub const STAGE: Stage = Stage::Hama;

impl Stage {
    /// 面の名前（出陣の知らせ）
    pub fn name(self) -> &'static str { match self { Stage::Hama => "浜辺" } }
    pub fn holds(self) -> [&'static str; 4] { match self { Stage::Hama => ["鯵の砦", "鯖の砦", "鯛の砦", "鮪の砦"] } }
    pub fn boss(self) -> &'static str { match self { Stage::Hama => "黄肌大王" } }
    /// 巨人の形のときの呼び名の頭
    pub fn giant(self) -> &'static str { match self { Stage::Hama => "魚巨人" } }
    /// 敵の名前（隊長の札など）
    pub fn kind_name(self, k: EnemyKind) -> &'static str {
        match self {
            Stage::Hama => match k {
                EnemyKind::Strawberry => "アジ足軽",
                EnemyKind::Macaron => "サバ槍兵",
                EnemyKind::Mikan => "メバル足軽",
                EnemyKind::Creampuff => "イサキ足軽",
                EnemyKind::Cottoncandy => "カタクチ忍",
                EnemyKind::Donut => "カサゴ投石兵",
                EnemyKind::Pudding => "マダイ重騎",
                EnemyKind::Shortcake => "ブリ隊長",
                EnemyKind::Cakeking => "ヒラマサ将軍",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 防ぐ故障: 魚の面だけの版に、お菓子の面（菓子ヶ原）とその名前が残る（docs/gyogun-spec.md「面は浜辺だけ」）
    #[test]
    fn 面は浜辺ひとつ() {
        let names: Vec<&str> = STAGES.iter().map(|s| s.name()).collect();
        assert_eq!(names, ["浜辺"]);
        assert_eq!(STAGES[0].boss(), "黄肌大王");
        // 敵の呼び名は遊びの計算の名前（お菓子）のままにしない
        use EnemyKind::*;
        for k in [Strawberry, Macaron, Mikan, Creampuff, Cottoncandy, Donut, Pudding, Shortcake, Cakeking] { assert_ne!(STAGES[0].kind_name(k), k.def().name); }
    }
}
