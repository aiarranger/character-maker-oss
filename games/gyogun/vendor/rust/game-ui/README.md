# game-ui

ゲームの画面の文字・枠・ボタンを egui で描く共通部品。2026-11-09 に百菓繚乱（muso）の `rust/game/src/ui.rs` から切り出した。Owner の決定（2026-11-09）で、全ゲームと雛形の画面はこの描き方（Rust が文字とボタンも描く。muso 式）にそろえる。ブラウザ（wasm＋WebGPU）とネイティブ（Mac、将来の iPhone アプリ）で同じ描き方になる。

## 入っている物

- `UiLayer`: winit の出来事 → egui → wgpu。ボタンの上で押した出来事だけを UI が受け、ゲームへ渡さない。
- 字体: 役（`Fam`）ごとに、ゲームが字体の表（`FontRow`）を渡す。`install_fonts_from`・`font_files_of`。
- 描く道具 `Pen`: CSS の transform・文字のグラデーション・text-shadow・ぼかし・角丸など。egui の部品は使わず頂点を組む。
- 色（`C`・`rgba`・`hx`）、動き（`Tween`・`ease` など CSS の cubic-bezier）、行列（`Xf`）、文字（`T`・折り返し `break_chunks`）、画像（`load_image`・`cover_uv`）、キーの札の行（`Kbd`・`row_draw`）。
- ブラウザの入力（`web::WebState`）: egui-winit がブラウザで使えないため、窓の出来事を egui の入力へ直す。
- winit を使わないゲームの入口（`raw::PointerInput`・`raw::RawLayer`）: island-kit の島のゲームなど。入力の出来事を論理 px で渡し、`Kit::render_then`（island-kit 0.1.27 以上）の中で `RawLayer::paint` を呼んで画面に重ねる。画面を取れずに描く処理が呼ばれないコマがあるので、毎コマ render_then の外で `RawLayer::finish` を呼び、中で `paint_finished` を呼ぶ形にする（描けなかったコマの字の絵の更新は次に描くコマへ持ち越す。0.1.3）。WebState も同じ PointerInput を使う。

- 指のスティックとボタン（`touch_pad::TouchPad`）: ゲームにも DOM にも依らない。ボタン → スティック → 叩いた の順に指を受け、指の id ごとに down・pressed・released・tap を返す。場所はゲームが毎コマ `set_layout` で渡し、描くのは `view()` の値を Pen で。
- ブラウザの canvas の出来事（`dom_input::DomInput`）: pointer・touch・wheel を、画面のボタン（PointerInput）と遊びの指（TouchPad）へ分ける。遊びの指が有効でも、egui の押せる面の上で始まった指は画面のボタンへ渡す。touchstart・touchmove の既定の動作（長押しのメニューなど）は止める。

（touch_pad と dom_input は 2026-11-10 に iceclimber の commit `c9a7cd3` から取り込んだ。）

0.2（2026-11-10。7 本の移行の報告から共通にした物）:
- 押す: `press(ui, id, rect) -> Press { hover, down, hit }`。DOM の click と同じ決まり（押した所から 12px 未満で、離した所が箱の上なら押下。長押しも押下）。ボタンはこれで決める（digdag）。
- 押せる面を知らせる: `block(ctx, id, rect, order)`。Pen で描くだけの札・ボタンは egui の面が無いので、指が遊びの方へ行く（rainbow）。
- 画面の面: `layer(id, order)`。出始めを薄くしない Area（egui の既定のフェードは CSS のフェードと二重になる。digdag）。
- 指: `TouchPad::set_drag(true)`・`DomInput::set_drag(true)` で、ボタンにもスティックにも当たらない最初の指の動きを `PadFrame::drag` に返す（右の指でカメラを回す。叩いた印もその指だけ。rainbow）。`DomInput::last_was_touch()`（pinball）。画面のボタンの上の pointerdown・mousedown は window（island-kit の Input）へ届けない（今の版は button が受けて canvas の押下にならない）。
- 描く: `Pen::box_shadow`（箱の外だけの影。rainbow）。`rrect`・`rframe` の角は半径で 4〜24 本に分ける（大きな丸ボタンが角ばらない。keiba。半径 8px までは前と同じ）。
- 字体: `Fam::Custom("名前")` で役を足せる。`Fam::family()` は公開。`install_fonts_with(ctx, table, prefer, ..)` で無い字を探す順を役ごとに先に決められる（和文の無い Fredoka → Zen Maru Gothic の近い太さ。keiba）。
- 軽さ: egui-wgpu の既定の機能を外した（既定は wgpu の WebGL の層を wasm に入れる。muso の wasm は 8.9MB → 6.1MB）。ゲームの Cargo.toml に `egui-wgpu = "0.36"` を直接書いているときは、同じく `default-features = false, features = ["fragile-send-sync-non-atomic-wasm"]` にする（Cargo の機能はまとめて効くので、どこか 1 つが既定のままだと戻る）。

0.3（2026-11-11。指で遊びやすく・起動を軽く）:
- 指のスティック: 倒しきった先へ指が進むと、中心が指に付いてくる（戻すとすぐ値が減る。画面の端で始めても倒しきれる）。`PadView::stick_at` も一緒に動くので、土台はその場所に描く。今までの「始めた所に固定」は `TouchPad::set_follow(false)`・`DomInput::set_follow(false)`。
- 指のボタン: 外側 10px までを押したことにする（中に入っているボタンが先。無ければ余白の中でいちばん近いボタン）。変えるのは `set_slop(px)`。画面のボタン（`press`。Ⅱ など）は、ゲームが渡す箱を見た目より大きくする（指では 44px 以上）。
- ページから取る物 `page`（ブラウザだけ）: `page::fetch_all(&urls)` はまとめて取る（取りに行くのは同時。字体を 1 本ずつ待たない）。`page::safe_area()` は端の安全域（ノッチ・ホームバー）を [上, 右, 下, 左] の論理 px で返す（ページの viewport に `viewport-fit=cover` が要る。窓の大きさが変わったら呼び直す）。指のボタン・Ⅱ・下の端のボタンはこの分だけ内へ寄せる。
- 0.3.1: 指を離すと、スティックの土台は触り始めた所へ戻る（0.3.0 は付いていった先に残り、ボタンや主役の上に残ることがあった）。

0.4（2026-11-11。うれしい瞬間）:
- 紙吹雪 `cheer::Confetti`（画面の座標。3D の世界には置かない）: 節目で `burst(場所, 向き, 広がり, 枚数, 速さ)`、毎コマ `step(dt, 画面の下の端)` と `draw(&pen, &色の並び)`。乱数は自前（遊びの乱数を進めない）。同時に 320 枚まで。使う節目はゲームが決める（毎回の操作では出さない。自己ベスト・全部の星などの大きな節目だけ）。
- 0.4.1: `cheer::speed_to_rise(px)` は、出した所から px だけ上がる速さを返す（`burst` の速さに渡す）。画面の高さはゲームと向きで違うので、速さの数字を直に書かず「札の上の端まで」のように高さで決める。出す場所は**結果の札の左右の下の角**にする（画面の角から出すと、横長の画面で真ん中へ届かない）。手本は pengo の `rust/sim/src/cheer.rs` と `rust/game/src/ui.rs`（commit c29b0b9 以降）。
- `page::reduced_motion()`（ブラウザだけ）: 端末の「動きを減らす」設定。真なら `Confetti::set_calm(true)`（枚数は 3 分の 1・回さない・揺らさない。祝いそのものは出す）。
- 数の数え上げは今ある `Tween`、弾んで出る動きは `snap`、1 回きりの時計は `Shot` を使う。

0.5（2026-11-11。各ゲームとひな形が同じ形で別々に持っていた物を 1 つに）:
- 結果の画面の流れ `cheer::ResultShow`（描画にもブラウザにも依らない）: `ResultShow::new(本当の値, cheer_of(自己ベストか, いちばん上の評価か), 動きを減らすか)`、小数（タイム）は `new_f64`。毎コマ `step(dt)` が出来事（`Tick`＝数え上げの刻み・`Done(段)`＝数え終わり・`Encore`＝いちばん上の評価のもう 1 回）を返すので、音と紙吹雪はそれを受けて出す。表示する数は `value()`／`value_f64()`（終わりは必ず本当の値）。先に出る演出を待つなら `.wait(秒)`（格の判など）か `.end_at(秒)`（この時刻より前に数え終わらない。星が灯りきる時刻など）。ボタンを押したら `cancel()`。結果の画面が出ていない間は `idle()`、祝いを入れない結果は `idle_at(値)`。
- `Confetti::burst_to(from, to, 広がり度, 枚数)`: from から to をねらって飛ばす（いちばん速い 1 枚が to の高さのあたりまで上がる）。札の左右の下の角から札の上の辺の真ん中へ、のように場所で決める。描く範囲を切るのは `pen.clip(箱)`、薄くするのは `pen.with_alpha(k)`。
- 項目の計算 `menu`（描画にもブラウザにも依らない）: `Used`（このコマに使った物）と `Seen`（最後に触った機器の表記 `Kind` と、輪を出すか。同じコマに複数触ったら `used` を呼んだ順の最後が勝つ）、`step(今, 個数, 前, 次)`（端はつながる）、`RESULT_WAIT`（0.6 秒）と `result_accepts`、`parse_safe`・`safe_from_query`。
- `page::safe_area()` は、URL に `?safe=上,右,下,左` があればその値を返す（確かめ用。ゲームが自分で URL を読まなくてよい）。
- `DomInput::take_used()`: 前に読んでから canvas を押した物（指かマウス）。`Seen::used` へ渡す（canvas に自前の受け口を付けなくてよい）。
- 0.5.1: `Confetti::burst_corners(札, 寄せる px, 広がり度, 枚数)`（札の左右の下の角から上の辺の真ん中へ、左右同じ枚数）。`DomInput::touching()`（今、指が canvas に触れているか。触っている間ずっと指と数えたいとき）。
- 共通にしていない物: 画面の並べ方（ボタン・札の場所と大きさ）、いちばん上の評価の決め方、紙吹雪の色と枚数、決定を受けない秒がゲームで長いとき（1.2 秒など）の値。

まだ共通にしていない物（各ゲームが持つ）: 字体ごとの行の高さと基線（Chrome の line-height: normal の丸め）、キーの札（`Kbd` は muso の見た目）、札の後ろのぼかし（各ゲームの final.wgsl。DPR 2 で粒が今の版より目立つ件は、縮めてからぼかす段を island-kit に足すときに直す）。

画面（タイトル・選ぶ・結果など）と配色の決まりはゲーム側に置く。

## 使い方

ゲームの `rust/game/Cargo.toml` に `game-ui = { path = "../../vendor/rust/game-ui" }` を足し、`node <characters>/scripts/game-parts.mjs --to <ゲーム> --write` で `vendor/rust/game-ui` へ写す（他の共通部品も最新へそろう。game-ui だけ写すなら `--rust game-ui`）。

```rust
pub const FONT_TABLE: [game_ui::FontRow; 7] = [/* (Fam::Brush, "同梱のファイル", "OS の字体", ttc の番号), … */];
let ui = game_ui::UiLayer::new(&device, surface_format, &window, |ctx| game_ui::install_fonts_from(ctx, &FONT_TABLE, &assets, &read));
// 1 コマごと: let ctx = ui.begin(&window); …Pen で描く…; ui.paint(&device, &queue, [w, h], &mut encoder, &view, &window);
```

## 指で押すボタンの注意

指では、離したコマに PointerGone も届いて `hover_pos` が無くなる。押した物は `interact_pointer_pos` で決める（muso の武将の帯で、指では選べない不具合が出た）。少し動いた指は egui ではドラッグになるので、小さな動きは押下として扱う。0.2 からは `press` がこの決まりで押下を返すので、ボタンはそれを使う。確認は `muso/rust/compare/strip_touch.py` のように、CDP の指（止めたタップ・少し動くタップ・長押し）で行う。
