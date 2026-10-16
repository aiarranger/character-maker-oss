# キャラクターメイクの共通部品

characters の `characters/_shared/character-maker/` から 2026-10-14 に移した。パスは `parts/` に読み替える（過去の evidence・reviews 内の記録は移す前のパスのまま）。

`assets/v1/` はアメコミスタイル（Quaternius CC0）、`assets/fantasy-v1/` はファンタジースタイル（旧β VRoid の個別CC0原本）の専用 canonical GLB。既存のナミ・ハル等の採用版を変更しない。

## Character Studio 実行時素材

12人選択型 `/character-maker` のモデル・髪型追加分・肖像・original-parts は `assets/studio-runtime-v1/` を正本にし、`public/` へ同期する。既存の `v1` / `fantasy-v1` / experiments と SHA が一致するものはそちらを参照する。

```sh
node tools/character-maker/sync-studio-runtime-assets.mjs
node tools/character-maker/sync-studio-runtime-assets.mjs --check
```

## 使う

`pnpm dev` で起動し、[キャラクターメイク](http://127.0.0.1:4284/character-maker/classic)（旧画面）を開く。Character Studio は `/character-maker`。

スタイルを選び、体型・髪型・髪色・顔の形・目・口を編集して名前を付けて保存する。ファンタジーは「表情」も選べる。「別のキャラとして保存」は別の作品を追加する。保存先は同じブラウザの `127.0.0.1:4284` の localStorage。JSONに書き出すと、別ブラウザでも読み込んで再編集できる。GLBには表示中の形・色・表情・骨・待機／歩行／走行／剣の構え／剣を振る動作を含める。「剣を振る」は一振り後に構えへ戻り、再度押して繰り返せる。自動瞬き・音声リップシンクと、AISNSへの組み込みは含まない。

新規作成はファンタジーから始まる。前回の編集があればそちらを復元する。アメコミのJSON・保存データは従来の schema1 / human-maker-v1 のまま。通常のファンタジーは schema2 / fantasy-maker-v1、表情を選んだファンタジーは schema3 / fantasy-maker-v1 を使う。旧保存に表情がなければ通常の顔として読み、旧実装はschema3を拒否するので表情だけ黙って失われない。スタイルの選択だけでは保存済みキャラを変更せず、「保存」で明示的に更新する。

ファンタジーの開始候補は女性4人・男性4人の計8人。「おだやか」は黒系の短髪で、「やわらか」のサイドポニーと見分けられる。「凛とした」「落ち着き」は「大人びた」顔を使う。候補の変更は保存済みの設定を書き換えない。

身長は頭身を保つ全体の倍率（85〜112%）。全身カメラは最大身長を収める距離を基準に固定し、身長を変更しても自動で拡大・縮小しない。顔表示では頭の上下移動に追従し、手動の回転・ズームを維持する。

「大人びた」はファンタジー専用。インスタンスの元Head scaleへ体のベースごとの比率（男性 1.035・1・1.025、女性 0.94・1.01・0.98）を掛け、輪郭は `lib/character-maker/fantasy-head.ts` の専用形状（mature-v2）を使う（angular morph は使わない）。目・口は独立して選べる。頬だけ赤みを抑えた `assets/mature-v1/` の肌画像を使い、従来の顔では元scale・元画像へ戻す。画像の再生成は `node tools/character-maker/build-mature-skin.mjs`。入力・出力SHAと局所補正の設定は同フォルダのmanifestに記録する。元GLB・テクスチャ・動作データは変更しない。

「表情」は通常・笑顔・怒り・悲しみ・穏やか・驚きの6種に、両目を閉じる・左右ウィンク・5母音・上下左右の視線を加えた18候補。表情を1つ選び、強さを調整する。目や口の恒常的な形とは別に保存する。左右はキャラクター本人から見た向き。スタイル別の表情記憶は同一編集セッション内で、保存対象は現在のスタイルと見えている顔だけ。

ファンタジーの表情は既存VRoidの `sourceVrmExpressions.blendShapeGroups` の全bindを、元mesh名・target名から解決して重みを設定する。男女のtarget番号を共有しない。`Surprised`はVRM0のcustom名で解決する。今回の対象群に材質bindはない。`lib/character-maker/expressions.ts` が表情targetだけを所有し、体型・輪郭・目・口のappearance morphには触れない。視線は元の眼球骨と内8°/外12°/上下10°を使い、元quaternionから毎回計算してmixer後に適用する。GLBは選んだ重み・眼球回転と全source morphを保持するが、VRM表情マネージャーや自動表情切替を追加するものではない。アメコミ原本に表情morph・眼球骨はなく、従来の編集機能を保持する。

丸い目・細い目と閉眼を単純加算すると瞳が残るため、閉じる側だけ元の開眼差分を打ち消す4本の補正targetをinstanceに追加する。笑顔・両目閉じる・各ウィンクの強さに追従し、反対の目と選択済みの目の設定は維持する。原本geometryは変更せず、複数の顔描画部品で専用配列を共有する。通常顔へ戻すと補正は0になり、GLBにも同じ補正と重みを含める。

設定の正本は `lib/character-maker/config.ts`。スキーマと素材版をJSONに含め、未対応版を別素材へ黙って置き換えない。画面と書き出しは `lib/character-maker/assembly.ts` の同じ組み立てを使う。

確認用コマンド:

```sh
pnpm typecheck
node --test lib/character-maker/*.test.mjs
node lib/character-maker/ui.e2e.mjs
node lib/character-maker/ui-fantasy.e2e.mjs
node lib/character-maker/ui-presets.e2e.mjs
node lib/character-maker/ui-height.e2e.mjs
node lib/character-maker/ui-expressions.e2e.mjs
node tools/character-maker/verify-mature.mjs
node tools/character-maker/verify-expressions.mjs
node tools/character-maker/verify-export.mjs /absolute/path/to/character.glb --config /absolute/path/to/character.character.json
```

E2Eと書き出し確認は、既存の検査台と同じ認証なしのheadless Chromeを使い、結果を一時フォルダへ保存する。スマートフォン実機での確認とは別。

2026-10-03の画面・保存互換・書き出し確認は checks.json、ChatGPT（極高）の手順・実画面レビューは review.json に保存している。

表情追加のUI操作は ui-checks.json、描画・補正・動作・GLB再読込は runtime-checks.json、ChatGPT（極高）の手順・最終レビューは review.json に保存する。

JSON再読込で組立完了後も空画面になる事象を確認したため、通常のモデル切替ではCanvasを維持し、モデル・mixer・カメラ操作だけを交換する。旧非同期結果はcleanup flagで破棄し、エラー後のプリセット選び直しは引き続き再試行できる。E2Eは組立完了の通知とは別に描画画素を待ち、再読込後のCanvas同一性と表示継続も確認する。修正後の操作は ui-checks-with-render.json、身長・回転・剣の画面内表示は height-checks.json を参照。

## 再構築

### ファンタジーと剣

```sh
node tools/character-maker/build-fantasy-assets.mjs
node tools/character-maker/build-extended-motions.mjs
node tools/character-maker/capture-thumbnails.mjs
```

表情カード12枚だけの再生成は `node tools/character-maker/capture-thumbnails.mjs --expressions-only`。

旧βの Victoria Rubin・桜田史利矢・HairSample Male の個別CC0原本を使う。現行の VRoid Studio 一般素材や AvatarSample A〜Z とは別の許諾であり、ファイル名だけで取り替えない。原本・元の表情・揺れ骨の情報は `sources/fantasy-v1/` に保持する。表示とGLBは標準glTFのunlit材質とraw骨を使い、原本のMToon照明や髪の物理揺れを再現したとは扱わない。

顔・首・服の元のつながりとskinを保持し、形状morphを既存表情targetの末尾へ追加する。髪は Hair001 と後頭部の HairBack をまとめてHead-localへ静的ベイクし、男女の頭皮へ事前適合する。元の表情targetの番号を外観パラメータに転用しない。

剣の動作原本は既存CC0 UALの `Sword_Idle` / `Sword_Attack`。アメコミの既存3clipと素材10ファイルは変更せず、剣2clipを `assets/extended-v1/` に追加する。ファンタジーは別のhumanoid mappingで5clipをraw骨へ変換する。剣は自作の小さなmeshで、手への取付情報は `lib/character-maker/weapon.ts` に集約。専用のscale trackで通常動作中は非表示、剣動作中は表示する。GLBのclipには一回再生のUI制御を保存できないため、利用側では `sword_attack` を一回再生後 `sword_idle` へ遷移する。

### アメコミ素材

```sh
node tools/character-maker/build-assets.mjs
```

初回の選択取り込みのみ、作者の公式 UI で取得済みの ZIP を使う。

```sh
node tools/character-maker/build-assets.mjs --import --downloads=/absolute/path/to/downloads
```

`sources/manifest.json` が原本 ZIP と取り込んだ各ファイルの SHA-256、作者、CC0、配布 URL を固定する。元 ZIP とコピー済み原本は書き換えない。異なる SHA の ZIP は、別版として判断する必要がある。

人体・衣装・髪で保存する原本は、人体男女、Peasant 衣装男女、使用する髪5種の glTF/bin、使用する BaseColor、ライセンスだけ。glTF の原文を保つため、そこには保存対象外の normal/ORM 等の参照も残る。再構築工具の `readUnlit` が使用しない材質スロットを除き、使用画像だけを `NodeIO.readJSON` に渡す。この原本サブセットは汎用ビューア用の配布物ではない。再実行に Downloads や `/tmp` のファイルは不要。

### 待機・歩行・走行の再構築

動作の原本は [Universal Animation Library — Standard](https://quaternius.itch.io/universal-animation-library)（Quaternius、CC0 1.0）。作者の root motion 無効版 `UAL1_Standard.glb` の原本は characters の `characters/_shared/character-maker/sources/animations/` にあり（各キャラの再現記録が参照する）、ここでは `distribution/` に SHA 照合付きで取り込んだ写しを読む（2026-10-14）。`_RM` 版は使わない。[独立した原本マニフェスト](sources/animations/provenance.json)に作者・配布URL・取得ZIPのSHA-256・GLB/License/README各原本のSHA-256とZIP内パスを固定し、許諾原文は [License.txt](sources/animations/License.txt) に保持する。

上の `build-assets.mjs` で canonical GLB を作った後、repo の根から次を実行する。保存済みの原本だけを読み、Downloads や `/tmp`、再ダウンロードには依存しない。

```sh
node tools/character-maker/build-motions.mjs \
  --masculine 'parts/sources/outfits/Exports/glTF (Godot-Unreal)/Outfits/Male_Peasant.gltf' \
  --feminine 'parts/sources/outfits/Exports/glTF (Godot-Unreal)/Outfits/Female_Peasant.gltf' \
  --canonical-dir parts/assets/v1
```

`Idle_Loop` → `idle`、`Walk_Loop` → `walk`、`Jog_Fwd_Loop` → `run` の3動作だけを、`assets/v1/{masculine,feminine}-motions.json`（Three.js `AnimationClip` のJSON配列）へ書き出す。原本と Peasant の rest pose 差を世界回転で変換し、target の骨・local TRS・inverse bind は保持する。移動は pelvis の差分だけで、上下成分を rest pelvis の高さ比で調整する。全身の身長変更は表示側の root 一様scaleに任せる。

工具は原本GLBのSHAを確認し、`A_TPose` からのtarget rest復元、動作中の回転・足底、canonicalとのTRS/逆bind一致を [retarget-validation.json](sources/animations/retarget-validation.json) に記録する。人体・衣服の `sources/manifest.json` は `build-assets.mjs --import` が生成するため、アニメ原本の記録をそこへ手で混ぜない。独立した `sources/animations/provenance.json` は両builderの生成対象外であり、再生成後も維持する。動作builderが更新するのは派生JSON2本と検証結果だけで、原本・ライセンス・独立マニフェストは更新しない。

## 表示側との契約

- `masculine.glb` / `feminine.glb`: Peasant の `maker-root` 以下に65骨・1 skin と、服、腕、脚、靴、頭・首、眼球、眉。+Y が上、+Z が正面、単位m。rootはidentity。高さ・足底・Headの行列は `assets/v1/manifest.json` に実測値を保存する。
- rest pose、骨の名前・親子関係・local TRS、inverse bind、残した頂点の skin weights を保存する。GLB writer が省略する微小 TRS も書き戻し、読込後に比較する。親ノード Armature の表示名だけ `maker-root` へ変更する。
- 髪は `hair-{parted,long,buns,crop,pixie}.glb`。元の skin を rest pose で評価し、元 Head のローカル空間へ焼き込んだ静的mesh。GLB rootはidentity。そのまま現在の `Head` ボーンへ子として付け、二重の高さoffsetを加えない。source髪の実ウェイトがHead100%であることを工具が確かめる。
- 全髪には `hairFitMasculine` / `hairFitFeminine` の適合morphがある。表示側は選択baseのtargetを1、もう一方を0にする。元donorと同じbase用の差分は0。異なるbase用は男女の元頭皮の三角形をHead-localで比較し、その表面差を髪へ転写する。首・肩を対応先にしないため、交点はHead-local Yが0.045mより上だけとし、0.045〜0.085mの下端で滑らかに減衰させる。頭皮からの距離の絶対値を使い、離れた束・長い毛先と内部キャップの両方で補正を減らす。髪の選択肢や頭皮の表示を制限しない。
- 材質名は `maker-hair`、`maker-brow`、`maker-skin`、`maker-eyes`、`maker-outfit`。髪と眉は中性のグレースケール画像＋白のbaseColorFactorで、同じ色を掛けられる。肌は作者のLight画像、衣装の露出肌にも同じ画像を使う。目の画像には白目と茶色の虹彩が共存する。
- すべて `KHR_materials_unlit`、画像は1024px以下の埋込PNG。法線・ORM画像、照明、tone mappingは不要。頂点colorはマスク用途の複数channelを含むため配布しない。色の詳細は元BaseColorに残す。
- 形状はglTF標準morphで事前生成し、中立値はすべて0。実行時は重みを変えるだけ。体型: `bodySlim` / `bodyBroad`、輪郭: `faceRound` / `faceAngular`、目: `eyesRound` / `eyesSlender`、口: `mouthSmile` / `mouthFull`。口角・唇の恒常的な形状パラメータであり、表情アニメーションとは別。
- `maker-head` は全8morph、衣装4meshは体型2morph、眼球・眉は目2morph。目の周囲と眼球は同じ変形場を使う。顔は主に下半分へ作用し、頭頂・髪アンカーを動かさない。

## 頭・首と衣服の境界

Peasant は頭と首の肌を持たない。Base の全身から、Head/neck_01 の実ウェイトを持つ頂点で構成される三角形を取り出す。そのままでは上胸・肩が歩行時に服を突き抜けるため、衣服の首開口の実際の境界辺を抽出し、その10mm内側までで頭・首を切り分ける。前のV字も元の開口線に従い、単一の水平面では切らない。境界を横切る三角形は辺上に新頂点を作り、UVと骨ウェイトを補間する。weldや骨の付け直しはしない。男性は身体全体の bind が衣服と異なるが、取り出す頭・首・spine・pelvis のbindは一致することを工具が検査する。女性は全65骨が一致する。

形状スライダーは作者の頭を元にした小さい範囲の変形。元は成人向けの欧米ゲーム風の造形であり、ナミ・ハルの顔と同一ではない。見た目の採用判定は実画面の確認に依存する。

## 出典

- [Universal Base Characters](https://quaternius.itch.io/universal-base-characters)
- [Modular Character Outfits - Fantasy](https://quaternius.itch.io/modular-character-outfits-fantasy)
- [Universal Animation Library — Standard](https://quaternius.itch.io/universal-animation-library)
- [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/)

原作者: Quaternius。元素材のCC0ライセンスは `sources/base/License_Standard.txt`、`sources/outfits/License_Standard.txt`、`sources/animations/License.txt` を参照する。アニメーションの出典と再現入力は [独立マニフェスト](sources/animations/provenance.json) に記録する。
