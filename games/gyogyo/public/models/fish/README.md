# 走る魚（Fish Kit）

出どころ: Fish Kit（https://cda-social.github.io/fish/gallery.html の `kit/fish-kit.zip`。2026-11-17 に取得）。
ライセンス: CC0 1.0（改変・再配布・商用利用が自由。クレジット不要）。全文は `LICENSE.txt`。

なちゃっとさんの写真ベースの模型に、元キットが実行時に作る顎・えらぶたの形、目とヒレの回転位置、頭部の写真を組み込んだ。口先は +X、背は +Y、長さの単位はメートル。[元キットからの再生成手順](../../../../../tools/fish-rigs.md)

出走登録では、マダイなどを選ぶと口・えらぶた・ヒレが大きく開く様子を見られる。レースでは呼吸に合わせて口とえらぶたが動き、左右の目が別々に向きを変え、ヒレが付け根から動く。`rust/game/src/view/fish.rs` がこれらの動きと頭から尾までのうねりを合成する。

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
