# 走る魚（Fish Kit）

出どころ: Fish Kit（https://cda-social.github.io/fish/gallery.html の `kit/fish-kit.zip`。2026-11-17 に取得）。
ライセンス: CC0 1.0（改変・再配布・商用利用が自由。クレジット不要）。全文は `LICENSE.txt`。

写真から作った魚の模型（約 5,000 三角形）。口先が +X・背が +Y・メートル・体の真ん中が原点。ひれは切り抜いた板（アルファで抜く・両面）。
ゲームでは `rust/game/src/view/fish.rs` が走者の大きさにそろえ、頭から尾へうねらせて泳がせる（キットの fishH と同じ式。値は各 glb の節点の `extras.swim`）。

| ファイル | 魚 |
|---|---|
| madai.glb | マダイ |
| buri.glb | ブリ |
| kihada.glb | キハダ |
| saba.glb | サバ |
| aji.glb | アジ |
| kasago.glb | カサゴ |
| tachiuo.glb | タチウオ |
| magochi.glb | マゴチ |
| kurodai.glb | クロダイ |
| suzuki.glb | シーバス（スズキ） |
