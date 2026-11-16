// 音の単体確認: 主な音を順に PCM へ書き出して数字を見て、デバイスがあれば実際にも鳴らす。
// cargo run -p gyogun-game --bin audio_check [-- --offline]
use gyogun_game::audio::{Audio, PickupKind, Track};
use std::time::{Duration, Instant};

const SR: u32 = 48_000;

type Play = (&'static str, fn(&Audio));
fn sounds() -> Vec<Play> {
    vec![
        ("swing(false)", |a| a.swing(false)),
        ("swing(true)", |a| a.swing(true)),
        ("hit(1,false)", |a| a.hit(1, false)),
        ("hit(6,true)", |a| a.hit(6, true)),
        ("blast", |a| a.blast()),
        ("ring", |a| a.ring()),
        ("kill(true)", |a| a.kill(true)),
        ("chain(7,false)", |a| a.chain(7, false)),
        ("chain(10,true)", |a| a.chain(10, true)),
        ("chain_mark(10)", |a| a.chain_mark(10)),
        ("chain_mark(50)", |a| a.chain_mark(50)),
        ("chain_big(30)", |a| a.chain_big(30)),
        ("pop", |a| a.pop()),
        ("hurt(false)", |a| a.hurt(false)),
        ("hurt(true)", |a| a.hurt(true)),
        ("jump", |a| a.jump()),
        ("land(false)", |a| a.land(false)),
        ("land(true)", |a| a.land(true)),
        ("dodge", |a| a.dodge()),
        ("reflect", |a| a.reflect()),
        ("rank_up(2)", |a| a.rank_up(2)),
        ("ranbu_ready", |a| a.ranbu_ready()),
        ("ranbu_start", |a| a.ranbu_start()),
        ("ranbu_finish", |a| a.ranbu_finish()),
        ("captain", |a| a.captain()),
        ("capture", |a| a.capture()),
        ("boss", |a| a.boss()),
        ("boss_hit_ground", |a| a.boss_hit_ground()),
        ("enemy_strike", |a| a.enemy_strike()),
        ("throw_sfx", |a| a.throw_sfx()),
        ("milestone", |a| a.milestone()),
        ("pickup(Bun)", |a| a.pickup(PickupKind::Bun)),
        ("pickup(Konpeito)", |a| a.pickup(PickupKind::Konpeito)),
        ("select", |a| a.select()),
        ("confirm", |a| a.confirm()),
        ("defeat", |a| a.defeat()),
    ]
}

fn stats(p: &[f32]) -> (f32, f32, f32) {
    let peak = p.iter().fold(0.0f32, |a, b| a.max(b.abs()));
    let rms = (p.iter().map(|x| (x * x) as f64).sum::<f64>() / p.len().max(1) as f64).sqrt() as f32;
    // -60 dBFS を超えている最後の時刻
    let len = p.chunks_exact(2).rposition(|f| f[0].abs().max(f[1].abs()) > 0.001).map_or(0.0, |i| i as f32 / SR as f32);
    (peak, rms, len)
}

fn offline() {
    let a = Audio::offline(SR);
    println!("効果音 {}/{} を読めた", a.samples_loaded(), a.sample_total());
    println!("{:<20} {:>7} {:>7} {:>7}", "音（コンプレッサー後）", "peak", "rms", "長さs");
    for (name, f) in sounds() {
        a.render(3.0); // 前の音の余韻を消す
        f(&a);
        let (pk, rms, len) = stats(&a.render(4.0));
        println!("{name:<20} {pk:>7.3} {rms:>7.4} {len:>7.2}");
    }
    println!("{:<20} {:>7} {:>7} {:>7}", "曲（最初の 6 秒）", "peak", "rms", "長さs");
    for t in Track::ALL {
        a.stop_music();
        a.render(2.0);
        let s = Instant::now();
        a.play_track(t);
        let load = s.elapsed();
        let s = Instant::now();
        let p = a.render(6.0);
        let (pk, rms, len) = stats(&p);
        let dec = s.elapsed();
        println!("{:<20} {pk:>7.3} {rms:>7.4} {len:>7.2}  読み込み {:.1} ms, 6 秒の合成 {:.1} ms", format!("{t:?}"), load.as_secs_f64() * 1e3, dec.as_secs_f64() * 1e3);
    }
}

fn live() {
    let a = Audio::new();
    a.init();
    let s = Instant::now();
    while !(a.device_ready() || a.no_device()) && s.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("デバイス: {}（{:.0} ms）", if a.device_ready() { "開けた" } else { "開けない（無音で続ける）" }, s.elapsed().as_secs_f64() * 1e3);
    while a.samples_loaded() < a.sample_total() && s.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("効果音 {}/{}（{:.0} ms）", a.samples_loaded(), a.sample_total(), s.elapsed().as_secs_f64() * 1e3);
    a.play_track(Track::Field);
    std::thread::sleep(Duration::from_millis(1500));
    let mut worst = Duration::ZERO;
    for (name, f) in sounds() {
        let t = Instant::now();
        f(&a);
        worst = worst.max(t.elapsed());
        println!("鳴らす: {name}");
        std::thread::sleep(Duration::from_millis(450));
    }
    println!("呼び出しの最長 {:.1} us", worst.as_secs_f64() * 1e6);
    a.play_track(Track::Boss);
    std::thread::sleep(Duration::from_millis(2500));
    a.stop_music();
    std::thread::sleep(Duration::from_millis(1200));
    let frames = a.frames_rendered();
    println!("音のスレッドが作ったフレーム数 {frames}（{:.1} 秒ぶん相当の目安）", frames as f64 / SR as f64);
}

fn main() {
    offline();
    if !std::env::args().any(|a| a == "--offline") {
        live();
    }
}
