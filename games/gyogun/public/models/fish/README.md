# 浜辺の面の魚（Fish Kit）

出どころ: Fish Kit（https://cda-social.github.io/fish/gallery.html の `kit/fish-kit.zip`。2026-11-16 に取得）。
ライセンス: CC0 1.0（改変・再配布・商用利用が自由。クレジット不要）。全文は `LICENSE.txt`。

写真から作った魚の模型。口先が +X・背が +Y・メートル。ひれは切り抜いた板（アルファで抜く・両面）。
ゲームでは `rust/game/src/view/fish.rs` が口先を +Z・長さ 1 にそろえ、`rust/game/shaders/fish.wgsl` が頭から尾へうねらせて泳がせる（キットの fishH と同じ式）。

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

`_lo` は遠景用の軽い版（約 700 三角形）で、数の多い雑兵に使う。数の少ない重騎・隊長・大将は細かい版（約 5,000 三角形）。
