# 大人の顔用の頬テクスチャ

`node tools/character-maker/build-mature-skin.mjs` で、SHA 固定した `../fantasy-v1/{feminine,masculine}.glb` の FaceSkin 画像から再生成する。元 GLB、VRM、rig、morph、動作は変更しない。原本の CC0 根拠は `../../sources/fantasy-v1/provenance.json`。

左右の頬だけを滑らかな楕円マスクで限定し、すぐ下の通常の肌色を基準に、余分な赤みを弱める。R チャンネルの陰影と alpha はそのまま、唇・鼻・眼窩・耳はマスク外。女性の赤みを抑え、元から赤みの弱い男性にはごく小さな変化だけを適用する。画像はどちらも 1024 × 1024 の sRGB PNG。

公開名は `fantasy-{base}-mature-skin.png`。標準 glTF と同じ sampler・colorSpace・flipY で、assembly が新しい Texture/Source を作り、`fantasy` の `face=mature` でだけ使用する。それ以外の顔は保存した元 map へ戻す。元画像・共有 Source の書換えや、目・口の隠れた変更はしない。

入力 GLB / 埋込画像 / builder / 出力の SHA、色の参照位置、マスク・強度設定、変更画素数は `manifest.json` に記録する。
