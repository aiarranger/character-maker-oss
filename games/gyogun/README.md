# 魚群繚乱（games/gyogun）

魚の大群をひとりでなぎ倒す 3D アクション「魚群繚乱（ぎょぐんりょうらん / GYOGUN RYORAN）」。Rust と WebGPU で動くブラウザゲーム。
夕暮れの浜辺で砦を 4 つ落とすと、巨大なキハダの大将「黄肌大王」が出る。主役はきゃしーと、Character Studio で作ったキャラ。
仕様は [`docs/gyogun-spec.md`](../../docs/gyogun-spec.md)。

## 作り方

```bash
pnpm gyogun:build   # games/gyogun/rust/web/build.sh。public/gyogun/ に wasm・JS・素材・きゃしーの配布を置く
pnpm dev          # http://127.0.0.1:4284/gyogun/index.html を開く
```

要る物: Rust 1.99.1（`wasm32-unknown-unknown`）・`wasm-bindgen`・`wasm-opt`、WebGPU の使えるブラウザ。
`FAST=1 pnpm gyogun:build` は `wasm-opt` を省く。出来上がり（`public/gyogun/`）は git に入れない。

試験:

```bash
cd games/gyogun/rust
cargo test -p sim              # 戦いの計算
cargo test -p gyogun-game --lib  # 画面・入力・素材と配布がそろっているか、Studio のキャラを主役にできるか
```

Studio のキャラの試験は、`rust/game/tests/fixtures/studio/` の `.character.zip`（オリジナル・ファンタジー・アメコミを 1 体ずつ）を読む。ZIP はブラウザでしか作れない（Studio の書き出しは WebGPU の Stage）。作り直すとき:

```bash
# pnpm dev を動かした状態で、リポジトリの根から（Chrome を使う。16 体の ZIP が出力先に出来る）
node --import ./tests/ts-resolve.mjs lib/character-maker/ui-gyogun-handoff.e2e.mjs /tmp/maker-gyogun-handoff
cp /tmp/maker-gyogun-handoff/{generic-robot,fantasy-bright,comic-fresh}.character.zip games/gyogun/rust/game/tests/fixtures/studio/
# 16 体ぶん全部を確かめる
cd games/gyogun/rust && GYOGUN_STUDIO_ZIPS=/tmp/maker-gyogun-handoff cargo test -p gyogun-game --lib つくったキャラ -- --nocapture
# 1 体を窓なしで描いて PNG にする（395 コマ目はのけぞりの途中）
GYOGUN_STUDIO_ZIP=/tmp/maker-gyogun-handoff/generic-cat.character.zip cargo run --release -p gyogun-game --bin hero_check -- png /tmp/moka.png studio 395
```

## Studio で作ったキャラで遊ぶ

1. `pnpm dev` で http://127.0.0.1:4284/character-maker を開き、キャラを選んで髪・色・武器を決める。
2. 3D のステージの左下にある、魚群繚乱のタイトル画面が動いているカード「魚群繚乱で遊ぶ」を押す（スマホなど狭い画面では「書き出し」メニューの中）。魚群繚乱が開き、そのキャラが主役で始まる（武器は Studio で選んだ物。「持たない」はケーキの剣）。
3. 武将選びには、きゃしーと、最後に送った 1 体が並ぶ。ページを開き直しても残る（次に送ると置き換わる）。

キャラはブラウザの控え（Cache Storage の `character-maker-gyogun-v1`）に置くだけで、サーバーには送らない。
Mac で直接動かす版は、環境変数 `GYOGUN_STUDIO_ZIP` に `.character.zip` の場所を渡すと同じキャラが並ぶ（`--from studio` でそのキャラを選んで始まる）。

## みんなで

このゲームはひとり用。タイトル左上の「百菓繚乱でみんなで遊ぶ ↗」を押すと、協力プレイのある元のゲーム 百菓繚乱（https://hyakka.yukikau.jp/?stage=hama&coop）が開く。
タイトルの「キャラを作って出陣」（出陣のボタンの右。狭い画面では上）を押すと、同じ場所の Character Studio（`/character-maker`）が開く。Mac で直接動かす版は、既定のブラウザで http://127.0.0.1:4284/character-maker を開く。

## 操作

| すること | キーボード | パッド | 指 |
|---|---|---|---|
| 移動 | W A S D | 左スティック | 左のスティック |
| カメラ | Q・E／←・→／マウスのドラッグ | 右スティック | 画面の右半分をなぞる |
| 通常攻撃 | J（左クリック） | X/□ | 斬 |
| 強攻撃 | K（右クリック） | Y/△ | 強 |
| 跳ぶ（空中でもう一度でわたあめ雲） | Space | A/× | 跳 |
| 避ける | Shift | B/○ | 避 |
| 乱舞 | I | RB/R1 | 乱舞 |
| 飴糸（隊長・大将へ飛びつく。斬る → 離れる → 斬るを 3 回で猪鹿蝶） | U | LT/L2 | 糸 |
| 一時停止（技の表） | Esc | Start | Ⅱ |

## 中身

| 場所 | 内容 |
|---|---|
| `rust/sim` | 戦いの計算（描画なし） |
| `rust/game` | 描画・入力・音・画面（wgpu） |
| `rust/web` | ブラウザ版の入口（`index.html`・`build.sh`） |
| `vendor/rust` | 主役の読み込みと描画・画面の部品・計算用の数学（`SHA256SUMS` で元と同じか確かめられる。`character-actor` の `actor.rs`・`package.rs` は、Studio のキャラのジャンプ・回避・のけぞりのためにここで直してある） |
| `public/delivery` | きゃしー 1 体の配布（一覧 → 版 → ファイル。ファイル名は中身の SHA-256）。ゲームは配っている同じ場所の `delivery/` から主役を読む |
| `public/models`・`public/aisns`・`public/sfx`・`public/voice` | 魚・拾い物・武器・小物・曲・効果音・声 |

Mac で直接動かす版（`cargo run --release -p gyogun-game --bin gyogun`）は、主役を `http://127.0.0.1:4284/gyogun/delivery/…` から読む（`pnpm dev` を動かしておく。環境変数 `GYOGUN_CATALOG` で一覧の URL を替えられる）。

## 素材の出どころ

`docs/gyogun-spec.md` の「素材と条件」を見る。
