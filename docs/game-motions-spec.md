# 見た目の底上げと、ゲーム用の動作（2026-11-15）

Owner の依頼「全体的に見た目の品質が低い。OSS として公開しているものを改善し、この PC 上の各種ゲームで使えるように動きや表情も追加」に対する範囲と決まり。改善案は ChatGPT Pro と作った（順番: ①描画と画角を全キャラでそろえる → ②動作を足す → ③コミック 4 体の顔を作り直す → ④残りの動作と表情）。

## 利用者が見ること

- Studio（`/character-maker`）の 3D に、やわらかい陰と縁の光が付く。色と絵は今までと同じ。顔のまわりは陰を弱く、なめらかにする。
- 「動き」の下に、ゲーム用の動作のボタンが並ぶ。**どのキャラクターにも同じ動作を入れる（キャラによって違わない。Owner 2026-11-15）**。
  - characters のキャラクター一覧の共通動作（`lib/main-character-defaults.ts` の `COMMON_MOTIONS` と必殺技）と同じ名前・同じ呼び名: **ジャンプ・回避・座る・休む・立つ・バンザイ・手を振る・回転キック・食べる・必殺技**（待機・歩く・走る・のけぞる・武器を構える・武器を振るは「動き」の並びにある）。
  - このメーカーだけの追加: パンチ・調べる・やられ・ダンス・話す・しゃがむ。
  - 「座る」は終わると「休む」へ続く。「立つ」で立ち上がる。
  - 1 回の動作（ジャンプ・回避・必殺技・パンチ・調べる）は、終わると選んでいた動き（待機・歩くなど）へ戻る。やられは倒れたまま。ダンス・話す・しゃがむは、ほかの動きを選ぶまで続く。
  - その場の再生で、保存データ（JSON）には入れない（のけぞり・＞＜ と同じ）。保存形式は変えない。
- 書き出す GLB に、上の 16 動作が同じ名前で入る（16 体すべて。オリジナル 4 体は通常用・武器用のどちらにも）。
- 旧画面（`/character-maker/classic`）の見た目は変えない（陰影なし）。書き出す GLB の動作は増える。

## 今回やらないこと（あとで）

- 表情ごとのクリップ（`face_happy` など。今はモーフの重みで渡す）
- この PC のほかの Rust の浮島ゲーム（pengo など）が Studio の GLB を読む口（characters の character-actor 側の変更が要る。今は登録済みの主役だけを読む）。このリポジトリの魚群繚乱（`games/gyogun`）だけは、Studio の「魚群繚乱で遊ぶ」で読める（2026-11-16。`docs/gyogun-spec.md`）
- 配布の tgz（`pnpm pack:vendor`）: 動作の JSON の分だけ 50 MiB の上限を超える見込み。Stage の wasm と同じく、公開リポジトリの固定コミットから取る形にする

## GLB の契約（ゲームで使う人向け）

名前の正本は `lib/character-maker/actions.ts`。原本は UAL1（Quaternius Universal Animation Library、CC0）。

| クリップ名 | 原本（UAL1） | 再生 |
|---|---|---|
| `jump` | Jump_Start → Jump_Loop（0.28 秒）→ Jump_Land | 1 回（約 2.9 秒）→ idle |
| `dodge` | Roll | 1 回（約 1.5 秒）→ idle |
| `skill` | Spell_Simple_Enter → Shoot → Idle_Loop（0.5 秒）→ Exit | 1 回（約 2.0 秒）→ idle |
| `punch` | Punch_Jab → Punch_Cross | 1 回（約 1.9 秒）→ idle |
| `interact` | Interact | 1 回（2.0 秒）→ idle |
| `down` | Death01 | 1 回（2.4 秒）。最後の姿で止める |
| `dance` | Dance_Loop | ループ（1.0 秒） |
| `talk` | Idle_Talking_Loop | ループ（約 2.9 秒） |
| `crouch` | Crouch_Idle_Loop | ループ（約 2.9 秒） |
| `banzai` | 姿勢から作る（軽く沈む → 両手を V 字に上げて 3 回はずむ → 戻る） | 1 回（2.4 秒）→ idle |
| `greet` | 姿勢から作る（本人の右手を顔の横へ上げて振る） | 1 回（2.2 秒）→ idle |
| `restDown` | 姿勢から作る（しゃがんで、地面にあぐらで座る） | 1 回（1.2 秒）→ `restLoop` |
| `restLoop` | 姿勢から作る（座ったまま、ゆっくり息をする） | ループ（3.2 秒） |
| `restUp` | 姿勢から作る（座った姿から立つ） | 1 回（1.2 秒）→ idle |
| `eat` | 姿勢から作る（座ったまま、右手 → 左手の順に口へ運ぶ） | ループ（4 秒） |
| `spinKick` | 姿勢から作る（体を 1 回まわし、本人の右脚を横へ伸ばす） | 1 回（1.1 秒）→ idle |

- 頭の大きい体（ルクス・モカ）のバンザイは、腕が頭に隠れないように外へ広い V 字にする（`wideArms`）。
- 姿勢から作る動作（`tools/character-maker/pose-motions.mjs`）: UAL1 の骨の上で、待機の最初の姿から「骨を世界のこの向きへ向ける」で姿勢を作り、姿勢の間をなめらかにつなぐ。出来たコマは UAL1 のコマと同じ形なので、同じ道具が各体の骨へ移す。座る動作は、毎コマいちばん低い所を床へ合わせる（脚の長さが違っても浮かない・沈まない）。

- 骨の回転と腰の位置だけのクリップ（15 コマ／秒）。その場の動きで、前へ進む量・無敵時間・当たりの時刻はゲーム側で決める。
- 武器を持たせた GLB では、`jump`・`dodge`・`skill`・`down`・`crouch` の間は武器を見せ、`punch`・`interact`・`dance`・`talk` の間は隠す（`rust/game/src/weapon.rs` の `weapon_track`）。
- プリセットの GLB の extras（`characterMaker.playback`）に、1 回か・終わったら何へ戻るかを書く（`rust/game/src/look.rs` の `playback`）。
- 既存のクリップ（idle・walk・run・sword_idle・sword_attack・hit・face_hurt）は変えない。
- yukikau.jp 用の zip の `character.json` は変えない（`motion.clips` に書けるキーは決まっているため）。GLB の中の知らない名前のクリップは、読む側が無視する。

## 作り方

| 系統 | 道具 | 出力 |
|---|---|---|
| ファンタジー・アメコミ | `node tools/character-maker/build-action-motions.mjs` | `public/maker-assets/{fantasy\|comic}-{feminine\|masculine}-action-motions.json`、確認の記録 `parts/assets/action-v1/` |
| オリジナル 4 体 | `node tools/character-maker/build-original-action-motions.mjs`（`--check` で保存済みと同じか確かめる） | `parts/assets/original-action-v1/<id>-actions.json` と `manifest.json` |

動作を足すときは `build-action-motions.mjs` の `ACTIONS`（名前 → つなぐ原本クリップ）か `pose-motions.mjs`（姿勢から作る動作）と `lib/character-maker/actions.ts`、Rust の名前の表（`original.rs` の `plays_once`・`original_export.rs` の `ACTIONS`・`look.rs` の `playback`・`weapon.rs` の `weapon_track`）に足して、2 つの道具を回す。

## 陰影（Studio の見え方）

- 模型が作った光なしの材質（Basic・toneMapped なし）を、同じ色と絵のまま陰影つきの材質（`rust/game/shaders/look.wgsl`）に差し替えて描く（`rust/game/src/look.rs`）。元の材質は残し、模型と書き出しは元の材質を読み書きする。**書き出す GLB は今までどおり光なし（KHR_materials_unlit）**。
- 光はカメラから見た左上の手前（どの向きから見ても同じ当たり方）。陰は少し青く、体は 34%、頭のまわりは 13% まで。縁の光は光の反対側だけ。
- 頭のまわり（頭の骨と頭の箱から測った球の中）は、面の向きの代わりに球の向きで陰を付ける。顔の細かい凹凸と、重ねた顔の部品（肌の当て布・目・眉）の継ぎ目に陰が出ない。
- ルクスの顔の画面（`Lux display …`）と、`Accessory_` の光る材質には付けない。ルクスは頭の球を使わず、白い外装の面の向きをそのまま陰に出す。
- Stage の `init.look` が `'studio'` のときだけ付ける。旧画面 classic と書き出し用の Stage は付けない。

## アメコミの肌（ならした絵）

- アメコミ 4 体は、肌の絵（`maker-skin`）の細かいむら（毛穴・無精ひげ・筋の線）と、目のくぼみ・鼻の横の濃い陰で顔が濁って見える。形は変えず、絵だけを直した版を実行時に差し替える（元の GLB は変えない。ファンタジーの「大人びた」顔の肌と同じやり方）。
- 道具: `python3 tools/character-maker/build-comic-skin.py`（Pillow・numpy）→ `public/maker-assets/comic-{feminine|masculine}-soft-skin.png`、記録 `parts/assets/comic-skin-v1/manifest.json`。
- 差し替えは `presetSwapFiles`（`lib/character-maker/preset-assets.ts`）→ Stage の `spec.swap`（材質の名前 → 画像）。Studio とその書き出し（GLB・yukikau.jp 用 zip）だけ。旧画面 classic は元の絵のまま。
- 眉の太さ・半分閉じた目・鼻と口の形は形（メッシュ）なので、絵では変わらない。下の「アメコミの顔の形」で直す。
- 考え方は、スキル `texture-3d-with-generated-images`（形は増やさず、面に貼る絵で見た目を上げる）。

## アメコミの顔の形（眉・まぶた・鼻・唇）

- 作り方は ChatGPT Pro と決めた（2026-11-15）: 全身を Tripo で作り直すと 1 枚のメッシュになり、髪型・顔の形・目・口・服の色の編集が全部なくなる。Tripo の頭の形を今の頭へ写す案も、頭蓋・首・目の位置を保つ条件では得る物が少ない。目と眉が別のメッシュなので、**今の形を直接直す**。
- 元の形の測り: 眉（`maker-brow` の大きい部品、太さ 20mm）の下の端が目玉の上 1/4 にかぶさり、まつ毛の線（同じメッシュの小さい部品）が目玉の中央の少し上を横切る。まぶたの開きは目玉 30mm に対して 10mm。これで「眉をひそめた半目」に見えていた。
- 直し（`python3 tools/character-maker/build-comic-face.py`。値は道具の `SETTINGS`）:
  - 眉: 6.5〜7mm 上げ、太さを女性 65%・男性 78%（太い眉は個性として残す）、眉頭を 3.5〜4° 上げる
  - まぶた: 上のふちを 3.6mm 上げ、下のふちを 1mm 下げる（まつ毛の線も同じだけ）。目玉は動かさない
  - 鼻先を 3.5〜4mm、唇の張り出しを 1.5mm 奥へ引く（まわりへなだらかに 0）
- 頂点の数・UV・骨の重み・モーフの差分はそのまま。位置だけを動かした体の GLB の写し（`parts/assets/comic-face-v1/{base}.glb` → `/maker-assets/comic-soft-{base}.glb`）を、Studio とその書き出しで使う（`presetBodyFile(config, soft)`）。元の GLB と旧画面 classic は変えない。
- 口の中（`maker-mouth`）: 元の頭には口の中が無く、唇も開かない。頭の骨に付けた暗い色の楕円（ふだんは 1 点につぶれて見えない）を足し、口の形（あ・い・う・え・お）と驚きのモーフで広げる。同時に下唇とあごを下げる。
- 目 3 種・口 3 種・顔の形 3 種・のけぞり（＞＜）と組み合わせて、ローワンで崩れが無いことを写真で確かめた。
- ＞＜ の閉じた目の板は、Studio では肌の絵から取った肌の色で塗る（前は目の絵のまわりの桃色で、板が浮いて見えた。`comicCoverImage(bodyGlb, skinPng)`）。旧画面 classic は今までどおり。

## 表情はどのキャラクターも同じ（Owner 2026-11-15）

characters のキャラクター一覧の共通の表情（`lib/main-character-defaults.ts` の `COMMON_EXPRESSIONS`）と同じ物を、全キャラで選べるようにする。

| 種類 | 一覧の名前 | ファンタジー | アメコミ | オリジナル 4 体 |
|---|---|---|---|---|
| 気持ち | happy・angry・sad・relaxed・surprised | VRM の表情 | 足したモーフ | happy は元のモーフ、ほかは読み込むときに作るモーフ |
| 痛がる | hurt（＞＜） | VRM の表情 | 顔の板 | 顔の板 |
| まばたき | blinkLeft・blinkRight | VRM の表情 | 足したモーフ | 元のモーフ |
| 口 | aa・ih・ou・ee・oh | VRM の表情 | 足したモーフと、足した口の中のメッシュ（`maker-mouth`。暗い色の楕円が口の形ごとに開く） | あ = aa、い = ee、う = ou は元のモーフ。え・お は元のモーフの組み合わせ |
| 視線 | lookLeft・lookRight・lookUp・lookDown | 目の骨 | 目玉を回すモーフ | 読み込むときに作るモーフ（目の中心をずらす。ルクスは光る目の片端を寄せる） |

- 画面の並び: 気持ち（笑顔・怒り・悲しみ・穏やか・驚き）→ 目を閉じる（両目・左目・右目）→ 口の動き（あ・い・う・え・お）→ 視線（本人の左・本人の右・上・下）。
- まだ違う所: オリジナルの GLB の口のモーフの名前は元のまま（`aa`・`ee`・`ou`。一覧の `ih`・`oh` という名前のモーフは無い）。オリジナルは表情の強さのスライダーが無い（気持ちは 100%）。

## アメコミの表情

- Studio の「表情」で、アメコミ 4 体もファンタジーと同じ 18 種を選べる（気持ち 5・目を閉じる 3・口 5・視線 4 と、ふつう）。強さも変えられる。保存データに入る（ファンタジーと同じ `expression`・`expressionStrength`）。
- 作り方: 直した顔の GLB の写しに、位置の差分のモーフを足す（`build-comic-face.py` の `EXPRESSIONS`）。眉の上下と眉頭の回転、まぶたの開け閉じ、口角、元の口のモーフ（`mouthSmile`・`mouthFull`）の組み合わせ。頭（`maker-head`）と眉・まつ毛（`maker-brow`）に同じ名前で入る。
- **モーフの名前は `happy`・`angry`・`sad`・`relaxed`・`surprised`・`blinkLeft`・`blinkRight`・`aa`・`ih`・`ou`・`ee`・`oh`・`lookLeft`・`lookRight`・`lookUp`・`lookDown`**（視線は目玉 `maker-eyes` を中心のまわりに回す）。この PC のゲームが使う表情の名前（characters の character-actor の `COMBAT_KEYS`）と同じなので、名前で引く読み手はそのまま使える。書き出す GLB にも入り、選んだ表情は重みとして書かれる。
- ＞＜（のけぞり）の間は、表情をふつうへ戻す。
- 保存形式: アメコミで表情を選んだ設定は schemaVersion 4 以上で書く（表情なしは今までどおり 1。武器つきは 5）。前の版の読み手は、アメコミの表情つきの設定を「この表情はファンタジースタイルで使えます」と断る（表情だけ黙って落とさない。ファンタジーの schema 3 と同じ考え方）。
- 旧画面 classic は元の GLB を使うので、表情の欄は今までどおりファンタジーだけ。

## オリジナル 4 体の気持ちの表情

- Studio の「表情」に **怒り・悲しみ・驚き・穏やか** のボタンが出る（ナミ・ハル・ルクス・モカ。笑顔は今までの「にっこり」）。押すと、ほかの表情を戻してその気持ちになる。
- 模型（13〜25MB。固定のコミットから SHA-256 を確かめて配る）は変えない。模型に無い眉と目のモーフを、**読み込むときに Rust が作って足す**（`rust/game/src/original_emotion.rs`）。
  - 目（`blinkLeft`・`blinkRight` のモーフを持つ部分。まつ毛・ルクスの画面の目・モカの目も）: まばたきの差分を、目頭の側と目尻の側で重みを変えて使う。怒りは目頭の側を下げてつり目、悲しみは目尻の側を下げてたれ目、穏やかは全体を少し閉じ、驚きは少し逆へ開く。
  - 眉（人の 2 体）: 眉の中心のまわりで回して上下へ動かす（量は眉の長さに対する割合）。
  - 口: 元のモーフの組み合わせ（怒り・悲しみは `happy` を負にして口角を下げる、驚きは `aa` で少し開ける、穏やかは少し口角を上げる）。
  - ルクス（画面の顔）だけは、光る目と口を気持ちごとの形にする専用のモーフ（つり目・たれ目・への字・丸い口）。共通の作りでは大きさが変わるだけで読み分けにくかった（ChatGPT Pro のレビュー）。
- 視線（本人の左・右・上・下）も同じ仕組みで足す（`lookLeft`・`lookRight`・`lookUp`・`lookDown`。保存も気持ちと同じ扱い）。
- 足したモーフの名前は `angry`・`sad`・`surprised`・`relaxed`。書き出す GLB にも入り、選んだ気持ちは重みとして書かれる（口は `happy`・`aa`・`mouthClose` の重みに出る）。
- ＞＜（のけぞり）の間は、気持ちをふつうへ戻す。
- 保存形式: 気持ちは `config.face` の `angry`・`sad`・`surprised`・`relaxed`（0〜1）。0 のときは書かない。書くときは schemaVersion 7（無ければ今までどおり 6）。前の版の読み手は 7 を断るので、気持ちだけ黙って落ちない。

## 頭を起こす（プリセット）

- UAL1 の立ち姿の動作は首を前へ倒していて、顔の大きい体では、下を向いた不機嫌な顔に見えていた（顔のアップで頭頂が見える）。
- 下見で、立って動く動作（待機・歩く・走る・構える・話す）の間だけ、顔の正面が水平より下を向いた分を頭の骨で戻す（最大 35°。ほかの動作へは約 0.15 秒でなだらかに切り替える）。`rust/game/src/maker.rs` の `apply_head_lift`。
- 顔の正面は、素の姿で「頭の骨 → 目（ファンタジーは目の骨、アメコミは目のメッシュ）」の水平の向きから測る。骨の軸の向きは系統で逆（ファンタジー −Z、アメコミ +Z）なので、軸には頼らない。
- ファンタジーの男性は素の姿の顔が骨に対して下を向いているので、水平より 13° 上まで起こす（目で合わせた値）。
- 下見だけの直しで、書き出す GLB の動作は変えない（ゲーム側で同じ見え方にするには、頭の骨に同じ直しを入れる）。

## ステージの背景（撮影スタジオの写真）

- 背景の色の上に、何も置いていない撮影スタジオの紙のホリゾントの写真風の画像（無彩色）を乗算で重ねる。光のたまり・四隅の落ち込み・壁から床への曲面が出る。キャラごとの色は今までどおり CSS が決める。
- 画像は ChatGPT の画像生成で作り、無彩色にして明るさをそろえた物（`parts/assets/studio-backdrop-v1/`。約 32KB）。`/maker-assets/studio-backdrop.jpg` で届ける。

## 確かめ

- `pnpm typecheck`・`pnpm test`・`pnpm assets:check`
- 写真: `node tools/character-maker/capture-review.mjs <出力先>`（12 体の全身と顔の一覧）
- 書き出した GLB を読み直し、動作の名前と、材質が全部 KHR_materials_unlit であることを確かめる

## キャラクター一覧と呼び名・項目をそろえる（2026-11-16）

`../characters` のキャラクター一覧（`lib/main-character-defaults.ts` と `components/CatalogControlPanel.tsx`）と違っていた所を合わせた。Studio にだけある物（にっこり・目と口を閉じる・両目・追加の 6 動作など）は残す。

| 項目 | 前 | 今（一覧と同じ） |
|---|---|---|
| 動作の呼び名 | 歩く・走る・のけぞる | 歩行・走行・被弾 |
| 被弾 | 押すと 1 回のけぞって戻る（約 0.7 秒） | 選んでいる間、のけぞりを繰り返し、＞＜ の顔を出したままにする。動作の並び（回避の次）にも入れる。もう一度押すか、ほかの動きを選ぶと終わる |
| 基準姿勢 | 無し | 動作を全部止めて、骨の素の姿を見せる（`rest`。下見だけで、GLB のクリップにはならない） |
| 気持ちの呼び名 | ふつう（ふだんの表情）・笑顔・怒り・悲しみ・驚き | 通常・笑う・怒る・悲しむ・驚く |
| 痛がる | 「＞＜」という名前。オリジナルは表情の一番上の並び | 「痛がる」。気持ちの並びに入れる（下見の表示で、保存しない。被弾で自動でも出る） |
| まばたきの呼び名 | 左目・右目 | 左まばたき・右まばたき |
| 表情の強さ・視線の強さ | プリセットだけ（表情の強さ） | オリジナルにも足す（選んでいる気持ち・視線の値を動かす） |

まだ違う所（一覧にあって Studio に無い操作）: 自動チェック、視線の斜め 4 方向と「正面へ戻す」、再生の「最初へ・周回の手前へ・1 コマ進む」、GLB/VRM の表示の切り替え。
