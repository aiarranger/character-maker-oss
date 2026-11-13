# キャラメイクを Rust へ移す計画（three・R3F・drei をなくす）

2026-11-13。Owner の決定は「中身ごと Rust へ」。描く所だけを替える橋渡し（R3F の `gl` の差し替え）は作らない。

## ゴール

- キャラメイク（`/character-maker` の Studio と `/character-maker/classic`）の組み立て・編集・動き・GLB の書き出し・画素の読み取りを、three なしで動かす。最後に three・R3F・drei を依存から外す。
- 機能は減らさない。部品の付け替え、髪・武器の取り付け、頭の形、モーフ、配色・肌のテクスチャ、表情・視線、＞＜、のけぞりの重ね、揺れ・まばたき・口、カメラ（全身・顔・横・後ろ・構え・回す・ピンチ・ダブルタップ）、床、書き出し（JSON・GLB・yukikau.jp 用 zip）、残像、すべて今のまま。
- 見た目は今と同じ。同じ条件の静止画を今の版と並べて比べる。
- WebGPU の無い端末には対応しない（Owner 2026-11-13。前は three の版を戻り先に残す予定だった）。WebGPU が無いときは「この端末では 3D を開けません」と知らせるだけにする（aisns の /play と同じ文言）。

## どこに何を置くか

| 置き場 | 中身 |
|---|---|
| `vendor/rust/*` | island-kit・character-actor・character-view・character-actor-npc・character-view-npc。正本は characters の `crates/`。`node ../characters/scripts/game-parts.mjs --to . --rust <crate,…> --write` で写す（自分で書き換えない） |
| `rust/game/`（crate `character-maker-stage`） | 模型（GLB を読む・組み立てる・設定を当てる・動かす・測る・書き出す）と描画（island-kit の Kit）。キャラメイクの種類の物だけ（主役・NPC の部品とは分ける） |
| `rust/web/build.sh` | wasm を作って `public/maker-stage/` に置く（git に入れない） |
| `components/character-maker/rust/` | Stage の読み込み、下見の部品（今の `CharacterMakerScene`・`OriginalCharacterScene` と同じ props）、カメラと回す操作（three を読まない TS） |
| `server/` | `/maker-stage/` を届ける。配布では wasm を tgz に入れず、公開リポジトリの固定 commit から SHA-256 を確かめて取る（オリジナル 4 体のモデルと同じ形。下の「配り方」） |

分け方: 模型の計算（頂点・骨・モーフ・クリップ・測り・書き出し）は Rust、画面の操作（React の UI・カメラの決め方・回す操作・アニメの合図）は TS。TS は設定の JSON と、素材のバイト列（今と同じ URL から fetch した物）を Stage へ渡し、毎コマ dt とカメラを渡す。

### TS と Stage の口（最小の形から足していく）

- `create_stage(canvas, init)` → Stage
- `stage.asset(name, bytes)`: 素材（GLB・PNG・動きの JSON）を名前で渡す
- `stage.assemble(json)` → 返り値の JSON（立った姿の高さ・頭の位置など）。模型を作り直す
- `stage.apply(json)`: 作り直さずに当てる設定（体格・顔・目・口・表情・髪の色・身長など。今の `instance.apply(config)` と同じ範囲）
- `stage.play(json)`: 動き・のけぞり・＞＜ の合図
- `stage.frame(dt, camera)`: 1 コマ（動き → 描く）
- 測り（構えの包み・頭の箱）と書き出し（GLB のバイト列）は、要る段で足す

## 移す順と、段ごとの確かめ

| 段 | 移す物 | 確かめ |
|---|---|---|
| R0 | crate・wasm を作る手順・`/maker-stage/` を届ける・Stage を作って透明に消す | 4284 で wasm が読めて、コンソールのエラーが 0 |
| R1（最小の 1 本） | ファンタジーの女性の 1 体（体・髪・モーフ・髪の色・unlit の材質・待機の動き）、正面の全身の固定カメラ。`?engine=rust` のときだけ使う | 同じ設定・同じ時刻の静止画を今の版と並べる |
| R2 | ファンタジーの男性・アメコミの 2 体、配色（服のテクスチャ）・大人の顔（頭の形の式・肌と目のテクスチャ・頭の骨の大きさ）・表情と視線・目を閉じる補正 | 4 体のプリセット × 代表の表情の静止画 |
| R3 | 動き（歩く・走る・構える・振る、つなぎ）・のけぞりの重ね・＞＜（ファンタジーはモーフ、アメコミは顔の板）・武器（手で作る剣と登録された武器、手への合わせ） | 決めた時刻の静止画。のけぞり・振るの通しは親に見てもらう |
| R4 | Studio のカメラ（全身・顔・構え・3 方向・切り替えの弧）・回す操作（ドラッグ・ピンチ・カーソルへ寄せる拡大・減衰）・ダブルタップ・床（接地の影と輪）・`data-camera-settled` | 方向・顔・構えの静止画。操作は親に見てもらう |
| R5 | オリジナル 4 体（髪・襟・しっぽの揺れ、種族と人の部品、体型、髪、まばたき、口、武器、＞＜ の板、正射影のカメラ、`Accessory_` の光） | 4 体 × 代表の静止画 |
| R6 | 書き出し（プリセットとオリジナルの GLB、構えの版、yukikau.jp 用 zip の thumbnail）・切り替えの残像 | 今の版の GLB と同じ中身か（節点・クリップ・モーフ・材質の名前と数、画素の比べ）。yukikau.jp の読み込み（aisns の `/play?import=1`） |
| R7 | 既定を Rust にする（WebGPU が無いときだけ three の版）・classic の入口・配り方・版を上げて aisns へ取り込む | 4284 と aisns の `/play?make=1` で通しの確かめ。束の数え |
| あと | aisns の WebGL2 の Stage ができたら、three の版と three・R3F・drei の依存を消す | 束に three が 0 |

各段で動く物を出し、次の段へ進む。各段の静止画は、テスト用の Chrome（127.0.0.1:4380）を借りる道具で撮る。

## 配り方（wasm）

- tgz には入れない（今の tgz は約 49.8 MiB で、50 MiB の上限の近く）。
- 公開リポジトリ（aiarranger/character-maker-oss）の固定 commit に wasm と JS を置き、`server/` が URL・SHA-256・バイト数の一覧から取ってキャッシュし、`/maker-stage/` で届ける（`server/studio-models.mjs` と同じ形）。このリポジトリの中では `public/maker-stage/` の作った物を使う。
- 固定 commit を作るには公開リポジトリへの push が要る（Owner が行う）。

## 取り込み先への影響

- aisns: `vendor/character-maker` の版を上げる（`pnpm pack:vendor` の tgz・`SHA256SUMS`・`package.json`）。`scripts/dev.mjs` の `serveMakerAsset` と `scripts/build-pages.mjs` の `copyMakerAssets` が `/maker-stage/` も届ける・写す。
- enginyaa（今は 0.2.0）: 新しい版へ上げるとき、`scripts/sync-maker-assets.mjs` が `maker-stage` も写すように直す必要がある。

## characters の character-actor へ足したい物（一覧。今は足さない）

キャラメイクの種類の物は `rust/game` に置く（種類ごとに部品を分ける）。どの種類にも効く読み方だけを、あとで character-actor へ寄せたい。

- 骨とモーフを当てた頂点の箱・球（three の `SkinnedMesh.computeBoundingBox`・`computeBoundingSphere` と `Box3.setFromObject`）。今は `rust/game/src/maker.rs` の `rest_bounds`。主役・NPC の測りにも同じ式が要る
- GLB を書く口（three の `GLTFExporter` の代わり。R6）。持ち込み・配布の道具にも効く
- GLB の独自の頂点の値（`_SPECIES_*` など）を読む口（`character_actor::glb::Primitive` は持たない。R5 は GLB の生のバイト列を Stage へ渡して `rust/game/src/original_edit.rs` の `raw_accessor` で読む）
- GLB のサンプラー（wrapS・wrapT・minFilter）を絵に当てる（island-kit の `glb_texture` は繰り返し・ミップ付きに決め打ち。R5 は `original.rs` の `sampler_opts` で当て直す。モカの目の絵は端で止めてミップなし）
- island-kit の正射影のカメラ（今はオリジナルの下見を「遠く・狭い fov の透視」に直して描く。`components/character-maker/rust/original-camera.ts` の `write`）
- （段を進めて見つかった物を下に足していく）

## 進み（2026-11-13）

- R1 済み: `?engine=rust` で、ファンタジーの女性（コハク）を Rust で組み立てて描く。体・髪・モーフ（体格・顔・目・口）・髪の色・服の配色・視線・動き。今の版と同じ条件の静止画で、体の大きさ・位置・姿勢・色が合う。床（接地の影と輪）はまだ無い（R4）。
- R2〜R4 済み: ファンタジーの男性・アメコミ・大人の顔（頭の形の式・肌と目の画像・頭の骨）・表情と視線・目を閉じる補正、武器（登録の武器と手への合わせ）・のけぞりの重ね・＞＜（ファンタジーはモーフ、アメコミは顔の板）・振るの終わりの合図、Studio のカメラ（全身・顔・構え・3 方向・切り替えの弧・顔は頭を追う）・回す操作（`lib/engine` の OrbitControls）・床・ダブルタップ・読み込みの失敗の板。代表の静止画（5 体 × 正面、構え、顔の横、＞＜、笑顔、のけぞり）が今の版と合う。classic（`/character-maker/classic`）の下見も Rust で描ける。
- R6（プリセット）済み: GLB を Rust の模型から書く（`rust/game/src/export.rs` の GlbWriter）。yukikau.jp 用の zip は `lib/character-maker/zip.ts`（three・fflate なし）。今の版の書き出しと、節点の名前・動作の名前とチャンネル・character.json（heightMeters を含む）がそろう。見本画像と切り替えの残像は、描いた直後に読む（`__makerRedraw`）。
- 配り方: `server/stage-files.mjs`（固定コミットから SHA-256 を確かめて取る）・`tools/character-maker/release-stage.mjs`（作って distribution/maker-stage/ へ）。`STAGE_FILES_COMMIT` が未定のあいだ `pnpm pack:vendor` は止まる。
- R5 済み（2026-11-13）: `?engine=rust` の Studio で、オリジナル 4 体（ハル・ナミ・ルクス・モカ）を Rust で組み立てて描く（`rust/game/src/original.rs` と `original_edit.rs`・`original_hurt.rs`・`original_rig.rs`、`components/character-maker/rust/RustOriginalScene.tsx`・`original-camera.ts`・`original-spec.ts`）。材質（Accessory_ のほかは光なし）・種族と人の部品の形・色替え・体型・髪型と髪の色（灰色の髪の絵・肌の補正の絵）・武器と握った手・＞＜ の板（Rust で形と肌の色、絵は canvas）・のけぞりの重ね・まばたきと口・頭を回す・120 Hz の刻み・シーク・正射影のカメラ（透視に直す）・床。4 体の正面・顔・横・のけぞり・＞＜・部品と色の編集・体型の静止画が今の版と合う。髪・襟・しっぽの揺れは移したが、今の 4 体の模型に揺れの設定が無いので動きは確かめていない。毛のまぶた（originalSpeciesLid・originalSpeciesLidCoverage）は今の 4 体の模型に無く、移していない。
- R6（オリジナル）済み（2026-11-13）: オリジナル 4 体の GLB（武器用・通常用）と yukikau.jp 用 zip を Rust の模型から書く（`rust/game/src/original_export.rs`、`components/character-maker/rust/export.ts` の `exportOriginalGlbRust`・`exportOriginalYukikauPackageRust`）。今の版（three）の書き出しと、節点の名前・形（頂点の数と箱・頂点の値の種類）・skin・材質・画像・動作の名前とチャンネル・character.json（heightMeters を含む）がそろう。違いは、Rust の版は形を常に 32bit で書く（武器用でも KHR_mesh_quantization を使わない）ことと、＞＜ の板の節点の TRS（今の版は頭の逆行列を節点に、Rust の版は頂点に入れる。見え方は同じ）。
- R7 済み（2026-11-13）: 既定を Rust にし、three の下見・組み立て・書き出し（`CharacterMakerScene`・`OriginalCharacterScene`・`OriginalEditableActor`・`StageFloor`・`components/rig-actor/generic/`・`lib/viewer-material/`・`lib/character-maker/` の three の組み立ての物）を消した。`three`・`@react-three/fiber`・`@react-three/drei` は実行時の依存から外し、`three` は素材を作る道具と試験のために devDependencies に残す。WebGPU が無い（`navigator.gpu` が無い・アダプタが null・Stage を作れない・GPU を失った）ときは `StageUnsupported`（ボタンは呼ぶ側が `onExit`・`exitLabel` を渡したときだけ）。消した物を試していた単体の試験は、値の表を見る部分だけを残した。
- 残り: 公開リポジトリへの push と `STAGE_FILES_COMMIT` の記入（Owner）、そのあと `pnpm pack:vendor` と aisns の取り込み（`scripts/dev.mjs`・`scripts/build-pages.mjs` で `ensureStageFiles()`、PlayMaker で `onExit`・`exitLabel="入口へ戻る"`）。enginyaa は新しい版へ上げるとき `scripts/sync-maker-assets.mjs` が `maker-stage` も写す必要がある（このメモの「取り込み先への影響」）。three の時の検証の道具（`tools/character-maker/verify-*.mjs`・`capture-thumbnails.mjs`）と E2E（`lib/character-maker/ui-*.e2e.mjs`）は消した three の物を読むので、そのままでは動かない。
