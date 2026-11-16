// 戦いの計算だけの速さを測る: cargo run --release -p sim --bin bench [回数] [最初の種] [周回]
// 自動プレイを画面なしで最後まで動かす。scripts/sim-bench.ts と同じ種・同じ乱数・同じ手順。
use sim::battle::{create_battle, BattleEvent, Phase};
use sim::bot::create_bot;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let runs: u32 = args.first().map_or(8, |v| v.parse().unwrap());
    let first: u32 = args.get(1).map_or(100, |v| v.parse().unwrap());
    let rounds: u32 = args.get(2).map_or(3, |v| v.parse().unwrap());
    let dt = 1.0 / 60.0;
    println!("rust round seed steps total_ms us_per_step score kos");
    for round in 0..rounds {
        let (mut all_ms, mut all_steps) = (0.0, 0u64);
        for i in 0..runs {
            let seed = first + i;
            let mut b = create_battle(seed, false);
            let mut bot = create_bot(&b, 1.0, seed + 1000);
            let mut steps = 0u64;
            let t0 = Instant::now();
            while steps < 60 * 60 * 15 {
                let input = bot.think(&b, dt);
                b.step(dt, input);
                let mut k = 0;
                while k < b.events.len() {
                    if matches!(b.events[k], BattleEvent::Defeat) { b.revive(); }
                    k += 1;
                }
                b.events.clear();
                steps += 1;
                if b.phase == Phase::Victory && b.victory_t > 4.0 { break; }
            }
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            let r = b.result();
            println!("rust {round} {seed} {steps} {ms:.1} {:.2} {} {}", ms * 1000.0 / steps as f64, r.score, r.kos);
            all_ms += ms; all_steps += steps;
        }
        println!("rust {round} all {all_steps} {all_ms:.1} {:.2} - -", all_ms * 1000.0 / all_steps as f64);
    }
}
