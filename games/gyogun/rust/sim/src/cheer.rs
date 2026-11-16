// 結果の画面の自己ベストの祝いのうち、魚群繚乱だけの決まり: いちばん上の格と、格の判を待つ秒。
// 流れそのもの（数え上げ → 段は 3 つ → もう 1 回）は共通部品 game_ui::cheer（game-ui 0.5）。描く・鳴らすのは game の ui.rs・app.rs・audio.rs。

/// いちばん上の格
pub const TOP_RANK: &str = "S";

/// 結果の格（S〜C）がいちばん上か（自己ベストのとき、祝いをもう 1 回足す）
pub fn top_rank(rank: &str) -> bool { rank == TOP_RANK }

/// 格の判が押し終わるまで（画面が出てから。ui.rs の stamp: 0.5 秒待って 0.7 秒）。その間は得点を数えない
pub const STAMP_S: f64 = 1.2;
