# このゲームのために作った効果音

2026-11-02、Stable Audio 3（medium モデル、steps 8・cfg 1）で AI Arranger が作り、候補から立ち上がりと余韻で選んだ。
先頭と末尾の無音を切り、モノラル・ピーク −1 dBFS・16bit WAV にした。

| ファイル | seed | 使う場面 |
|---|---|---|
| boom.wav | 996984202 | 大爆発・乱舞の締め・衝撃波 |
| whoosh-heavy.wav | 1205991015 | 強攻撃の風切り |
| taiko.wav | 1291013652 | 叩きつけ・隊長の出現・節目 |
| crowd-hit.wav | 529803661 | まとめて斬った音 |
| shatter.wav | 1272557470 | 敵将を倒した |
| roar.wav | 1632951280 | 大将の登場 |
| ranbu-rise.wav | 1100781997 | 乱舞の発動 |
| gong.wav | 259048027 | 砦の制圧・敵将を倒した |

ほかの効果音は Kenney（CC0、`public/aisns/sounds/wilds/`。出どころは同じフォルダの CREDITS.txt）と、合成音（`rust/game/src/audio.rs`・`audio_web.rs`）。
