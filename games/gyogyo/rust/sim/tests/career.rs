// 育成（docs/spec.md「育成」）。防ぐ故障: 5 着以下でも続く・8 戦で終わらない・賞金と称号の取り違え・育てた能力が残らない・殿堂があふれる。
use sim::career::{Career, HallEntry, RACES, START_LEVELS, hall_insert, prize_of, title_of};

#[test]
fn eight_races_from_debut_to_the_retirement_race() {
    assert_eq!(RACES.len(), 8);
    assert_eq!(RACES[0].grade, "新馬");
    assert_eq!(RACES[7].grade, "GI");
    assert!(RACES.windows(2).all(|w| w[0].cpu_level <= w[1].cpu_level && w[0].purse <= w[1].purse), "後のレースほど相手が強く、賞金が高い");
}

#[test]
fn prize_goes_to_the_top_three() {
    assert_eq!(prize_of(1000, 1), 1000);
    assert_eq!(prize_of(1000, 2), 400);
    assert_eq!(prize_of(1000, 3), 250);
    assert_eq!(prize_of(1000, 4), 0);
    assert_eq!(prize_of(1000, 8), 0);
}

#[test]
fn a_top_four_finish_moves_on_and_keeps_the_grown_levels() {
    let mut c = Career::new("ame");
    assert_eq!((c.index, c.levels, c.prize, c.retired), (0, START_LEVELS, 0, false));
    let o = c.record(1, [3, 1, 2, 1, 1]);
    assert_eq!((o.prize, o.retired), (300, false));
    assert_eq!((c.index, c.levels, c.prize, c.wins), (1, [3, 1, 2, 1, 1], 300, 1));
    let o = c.record(4, [3, 2, 2, 1, 1]);
    assert_eq!((o.prize, o.retired), (0, false));
    assert_eq!((c.index, c.places.clone()), (2, vec![1, 4]));
}

#[test]
fn fifth_or_worse_retires_on_the_spot() {
    let mut c = Career::new("ame");
    c.record(2, START_LEVELS);
    let o = c.record(5, [2, 2, 2, 2, 2]);
    assert!(o.retired && c.retired);
    assert_eq!(c.index, 1, "次のレースへは進まない");
    assert_eq!(c.places, vec![2, 5]);
    assert_eq!(c.levels, [2, 2, 2, 2, 2], "引退のときの能力が残る");
}

#[test]
fn the_eighth_race_is_the_last_whatever_the_place() {
    let mut c = Career::new("ame");
    for _ in 0..7 { assert!(!c.record(1, START_LEVELS).retired); }
    assert_eq!(c.index, 7);
    let o = c.record(1, START_LEVELS);
    assert!(o.retired && c.retired);
    assert_eq!(c.wins, 8);
    assert_eq!(c.prize, 300 + 500 + 500 + 1000 + 1000 + 2000 + 4000 + 6000);
    assert_eq!(c.title(), "無敗の伝説");
}

#[test]
fn titles_follow_the_prize() {
    assert_eq!(title_of(0, 0, 1), "かけだし");
    assert_eq!(title_of(999, 1, 3), "かけだし");
    assert_eq!(title_of(1000, 1, 3), "オープンランナー");
    assert_eq!(title_of(3000, 2, 5), "重賞ランナー");
    assert_eq!(title_of(7000, 3, 8), "GI ランナー");
    assert_eq!(title_of(11000, 5, 8), "殿堂の名ランナー");
    assert_eq!(title_of(15300, 7, 8), "殿堂の名ランナー", "8 勝でなければ伝説ではない");
}

#[test]
fn the_hall_keeps_the_richest_twenty() {
    let mut hall: Vec<HallEntry> = Vec::new();
    for i in 0..25 {
        let mut c = Career::new("ame");
        c.prize = i * 100;
        c.places = vec![1];
        hall_insert(&mut hall, HallEntry::of(&c));
    }
    assert_eq!(hall.len(), 20);
    assert_eq!(hall[0].prize, 2400);
    assert_eq!(hall[19].prize, 500);
    assert!(hall.windows(2).all(|w| w[0].prize >= w[1].prize));
}
