// 結果の画面の自己ベストの祝いのうち、魚群繚乱だけの決まり（流れの試験は共通部品 game-ui の cheer にある）。
use sim::cheer::{top_rank, STAMP_S, TOP_RANK};

/// いちばん上の格は S だけ（得点 27 万点以上。battle.rs の result）。祝いをもう 1 回足すのは S のときだけ
#[test]
fn only_s_is_the_top_rank() {
    assert_eq!(TOP_RANK, "S");
    assert!(top_rank("S"));
    for r in ["A", "B", "C", "", "s"] { assert!(!top_rank(r), "{r}"); }
}

/// 格の判は、画面が出てから 0.5 秒待って 0.7 秒で押し終わる（ui.rs の stamp）。得点はそのあとで数え始める
#[test]
fn count_waits_for_the_stamp() {
    assert_eq!(STAMP_S, 0.5 + 0.7);
}
