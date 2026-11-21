# 魚々娘（games/gyogyo）

[ブラウザで遊ぶ](https://gyogun.yukikau.jp/gyogyo/)

きゃしーと魚とイカが、雲の上の競馬場を走る 3D のかけっこ「魚々娘（ぎょぎょむすめ / GYOGYO MUSUME）」。Rust と WebGPU で動くブラウザゲーム。
進路を選び、ダッシュをためて、最後の直線で競り勝つ。走路のマークで能力を上げ、拾った武器で攻撃し、育成ではデビューから引退までの全 8 戦を走る。
AI Arranger のゲーム「かけっこグランプリ」（https://kakekko.yukikau.jp/）を写して作った。仕様は [`docs/gyogyo-spec.md`](../../docs/gyogyo-spec.md)。

## 作り方

```bash
pnpm gyogyo:build   # games/gyogyo/rust/web/build.sh。public/gyogyo/ に wasm・JS・素材・きゃしーの配布・魚・イカを置く
pnpm dev            # http://127.0.0.1:4284/gyogyo/index.html を開く
```

要る物: Rust 1.99.1（`wasm32-unknown-unknown`）・`wasm-bindgen`・`wasm-opt`、WebGPU の使えるブラウザ。
`FAST=1 pnpm gyogyo:build` は `wasm-opt` を省く。出来上がり（`public/gyogyo/`）は git に入れない。

試験:

```bash
cd games/gyogyo/rust
cargo test -p sim          # 遊びの計算
cargo test -p gyogyo-game  # イカの形と動き（three 版の Squid Kit と頂点が一致するか）
```

## 操作

| キー | 動き |
|---|---|
| A / D（← / →） | 進路（左が内、右が外） |
| Space（押している間） | ダッシュ |
| J | ジャンプ |
| K（押して離す） | 攻撃（武器を持っている間だけ。押し続けるとためる） |
| Esc / P | 一時停止・操作の説明・設定 |

ゲームパッドと指でも遊べる（一時停止の画面に出る）。

## 中身

| 場所 | 中身 |
|---|---|
| `rust/sim/` | 遊びの計算（走路・走り・マークと武器・CPU・育成・カップ） |
| `rust/game/src/view/runner.rs` | 走る人の見た目（人の形・魚・イカ） |
| `rust/game/src/view/fish.rs` | 走る魚（Fish Kit の模型を、頭から尾へうねらせる） |
| `rust/game/src/view/squid.rs` | 走るイカ（Squid Kit の three.js のコードを移した） |
| `rust/game/src/character_delivery.rs` | 走る人の名簿（きゃしー・Studio のキャラ・魚・イカ） |
| `vendor/rust/` | characters の共通部品（island-kit・character-actor・character-view・game-ui・sim-math）。character-actor は魚群繚乱と同じ、Studio のキャラのジャンプ・回避・のけぞりを読める版 |
| `public/` | 素材（`docs/gyogyo-spec.md` の「素材と条件」） |
