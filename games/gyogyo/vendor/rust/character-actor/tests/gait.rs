// 減速の途中で速さがちょうど切り替えの値に丸まっても、今の版（倍精度）と同じ判定にする
use character_actor::actor::Gait;

#[test]
fn speed_just_over_the_switch_counts_as_over() {
    let g = Gait { walk: 0.4, run: 1.25 };
    let just_over_walk = 0.4 + 1e-12;
    assert_eq!(just_over_walk as f32, 0.4f32, "f32 では区別がつかない値で確かめる");
    assert_eq!(g.flags(true, just_over_walk), (true, false));
    assert_eq!(g.flags(true, 0.4), (false, false));
    let just_over_run = 1.25 + 1e-12;
    assert_eq!(just_over_run as f32, 1.25f32);
    assert_eq!(g.flags(true, just_over_run), (true, true));
    assert_eq!(g.flags(true, 1.25), (true, false));
    assert_eq!(g.flags(false, 5.0), (false, false));
}
