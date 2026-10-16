# ファンタジー系キャラクターメイク素材

旧 VRoid Studio β の CC0 原本を使う独立したスタイル。既存 `assets/v1` のアメコミ系は変更しない。

再生成:

```sh
node tools/character-maker/build-fantasy-assets.mjs
```

初回取り込みは `--import --from=<展開済みの入力ディレクトリ>`。入力構成と SHA は builder の `SOURCE_DEFS` で固定している。現在の正本は `../../sources/fantasy-v1/*.vrm`、個別の公式 CC0 根拠・元ファイル名・埋め込みメタデータは同フォルダの `provenance.json`。入力 VRM はバイト単位で保存し、重複 ZIP は置かない。

## 公開契約

- 全身: `feminine.glb` / `masculine.glb`。標準 glTF 2.0、メートル、+Y 上・+Z 正面。原本の -Z 正面からの変換は `maker-fantasy-facing` の Y π 回転だけ。原本の node 名、ローカル TRS、skin joint 配列、inverse bind matrices は維持。
- アンカー: `J_Bip_C_Head`。右手は `J_Bip_R_Hand`。`extras.characterMaker` に `humanBones` 名前 map、`head`、`rightHand`、`hairAnchor`、`wrapper` を保存。
- 髪: `hair-{feminine|masculine}-{animeParted|sidePony|layered}.glb`。それぞれの頭に適合済みの Head-local 静的 mesh、ルート identity。対応する全身の Head の子に置く。追加回転や fit weight は不要。
- 髪 package は元 `Hair001` と `Body` の `HairBack` primitive 全体。後頭皮は全身に残す。元 Neck への参照も含めた rest skinning を焼き込み、男女の原本後頭皮 bounds 間の affine 対応で適合する。肌の消去・非表示や衣装のウェイト移植は行わない。
- `animeParted`: エアリーショート (Sakurada)。`sidePony`: サイドポニー (Victoria)。`layered`: レイヤーショート (HairSample_Male)。全髪を両ベースへ適合。
- 色を乗せる材質は名前が **完全一致**する `maker-hair` / `maker-brow`。`maker-hair-accessory` はリボンであり、髪色を乗せない。

## 形状・表情

顔の元 41 / 39 表情 target は元 index のまま保存し、その後ろへ `faceRound` / `faceAngular` / `eyesRound` / `eyesSlender` / `mouthSmile` / `mouthFull` を追加。元の primitive extras の targetNames を mesh extras にも移し、通常の GLTFLoader から名前で読めるようにする。Body へ `bodySlim` / `bodyBroad` を追加。標準はすべて 0、選択した maker target のみ 1。身長は利用側の全体 scale。

顔輪郭は顎・下顔面、目は眼窩と虹彩・白目・まつげを同じ連続変形場で、口は口周辺を変形する。衣装と露出する腕・脚へ同じ Body 変形場を適用する。原本の表情データや骨 rest を造形調整で書き換えない。元 VRM の表情 bind と元 mesh 名は root extras に保存するが、新 runtime が VRM の全表情 UI を提供するとは主張しない。

## 材質と揺れ

VRM0 の `_Color` / `_EmissionColor` は three-vrm 3.5.5 の標準変換と同じ gamma 2.2 で線形化し、MainTex と emission を線形色で合成して sRGB の 1024 px 以下 PNG へ保存。全材質に `KHR_materials_unlit` を設定する。アルファ方式、cutoff、cull は元 MToon を保持。眉・髪は合成色を中性の明暗へ変換して白 factor とし、選択色を乗せる。元 MToon の照明応答・輪郭線・matcap はこの標準 unlit 表示の機能ではない。

canonical の階調仕上げは builder の `TEXTURE_FINISH` に固定する。髪は線形 Rec.709 輝度で中性化し、ハイライトの上限を保ったまま sRGB の中間調へ指数 1.55 を適用して元の髪束の濃淡を出す。肌は指数 1.10 / 上限 0.995 の穏やかな輝度調整、衣装は明るく低彩度の部分だけ指数 1.32 / 上限 0.97 を連続的に混ぜ、白い袖・襟の陰影を残す。RGB 共通倍率で色相を保ち、瞳・唇・リボンは調整対象外。元の男女とも肌・衣装の emission はゼロであり、白い衣装の淡さは加算による白飛びではなく MainTex の配色に由来する。元画像・形状・rest・morph は変更しない。

Sakurada の元 `COLOR_0` には未使用の黒・alpha 0 頂点色が残り、通常 GLTFLoader では顔が黒く衣服が消える。元 MToon が使わないこの属性を canonical から除き、元画像と材質色を表示する。原本 VRM は修正しない。

髪は今回の runtime では静的。元の髪骨、ウェイト、spring 設定は原本 VRM に完全に保存し、動的な髪揺れ対応とはしない。胴体 canonical にも元骨ノードを残す。静的変換後は同じ材質の髪 primitive を標準 glTF-Transform `join` でまとめ、原本の三角形は保持する。

## 記録

`manifest.json` は builder / provenance / 出力の SHA、入力識別子、元パーツ、頭皮適合の scale・offset・最大変位、骨格 signature、morph の最大変位、材質変換を記録する。builder は原本 SHA、出力 readback の native rest / IBM、一画像 1024 px 以下、各ファイル 25 MiB 未満、unlit の保持を直接照合する。実画面・動作・書き出しの確認は利用側で別途行う。
