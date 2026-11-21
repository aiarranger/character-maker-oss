# 浜辺の面の魚（Fish Kit）

出どころ: Fish Kit（https://cda-social.github.io/fish/gallery.html の `kit/fish-kit.zip`。2026-11-16 に取得）。
ライセンス: CC0 1.0（改変・再配布・商用利用が自由。クレジット不要）。全文は `LICENSE.txt`。

なちゃっとさんの写真ベースの模型に、元キットが実行時に作る顎・えらぶたの形、目とヒレの回転位置、頭部の写真を組み込んだ。口先は +X、背は +Y、長さの単位はメートル。[元キットからの再生成手順](../../../../../tools/fish-rigs.md)

マダイ重騎などの敵と大将のキハダは、頭から尾へうねりながら泳ぎ、口とえらぶたで呼吸する。左右の目と各ヒレも動く。カタクチイワシの口・目・えら・ヒレは元キットの設定どおり固定。`rust/game/shaders/fish.wgsl` が部位の動きと泳ぎを描画する。

| ファイル | 魚 | ゲームでの役（docs/gyogun-spec.md） |
|---|---|---|
| aji_lo.glb | アジ | アジ足軽 |
| saba_lo.glb | サバ | サバ槍兵 |
| mebaru_lo.glb | メバル | メバル足軽 |
| isaki_lo.glb | イサキ | イサキ足軽 |
| katakuchi_lo.glb | カタクチイワシ | カタクチ忍 |
| kasago_lo.glb | カサゴ | カサゴ投石兵 |
| madai.glb | マダイ | マダイ重騎 |
| buri.glb | ブリ | ブリ隊長 |
| hiramasa.glb | ヒラマサ | ヒラマサ将軍 |
| kihada.glb | キハダ | 大将 黄肌大王 |

`_lo` は元キットの遠景用の軽い模型から作り、数の多い雑兵に使う。数の少ない重騎・隊長・大将には細かい模型を使う。
