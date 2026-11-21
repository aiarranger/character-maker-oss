# Fish Kit の可動部をゲーム用 GLB に焼く

魚々娘と魚群繚乱は、なちゃっとさんの Fish Kit から、各ゲームが使う魚種と詳細度の GLB を読み込みます。ゲームをビルドする際はリポジトリの生成済み GLB をそのまま使用します。以下はモデルを再生成する手順です。

1. [Fish Kit の配布 ZIP](https://cda-social.github.io/fish/kit/fish-kit.zip) をダウンロードして展開します。使用版の ZIP の SHA-256 は `8644827d5f48b2ca7ef8cc72166cd8c2e0cb131a7acd5e789ca5e73081fb3c45` です。[作者のギャラリー](https://cda-social.github.io/aquarium/models-gallery.html) で配布中の最新版は変更されることがあります。
2. このリポジトリのルートで `pnpm install --frozen-lockfile` を実行します。生成には `three` と `sharp`（実行環境に合うネイティブバイナリを含む）を使用します。
3. `node tools/bake-fish-rigs.mjs /path/to/fish-kit` を実行します。引数は `fish-kit.js` と `fish/` があるディレクトリです。スクリプトは使用版の `fish-kit.js` の SHA-256 を照合してから、両ゲームの `public/models/fish/*.glb` を再生成します。コマンドの末尾に `--check` を付けると、再生成したデータを既存 GLB と照合できます。

元のGLBのテクスチャ、頂点、種別設定と同梱の `fish-kit.js` を使用し、原版と同じ順序でヒレの付け根を補い、えらぶた・下あご・上あごの板を作り、部位ごとのウェイトと左右の目の回転中心を準備します。頭部とえらぶたの写真（`_head.webp`、`_cover.webp`）、口が開いたときに下に残る面の切り抜きを、ゲームで読める1枚のテクスチャに焼きます。カタクチイワシは元キットで部位の動きを止める魚種です。各魚種の動きは [原版の JavaScript](https://cda-social.github.io/fish/kit/fish-kit.js) と [README](https://cda-social.github.io/fish/kit/README.md) を参照してください。元のコード・モデル・写真は CC0 1.0 です。

## GLB の読み方

形状は口先が +X、上が +Y、単位はメートルです。すべての頂点属性は Float32 です。部位の番号・ウェイト・補助値は `_FIN`（VEC3）、回転中心は `_PIV`（VEC3、元キットの並びから GLB 座標への変換は `[x,z,-y]`）に入ります。下あごのウェイトは `_JAW`（VEC2）、ヒレのはためきは `_FLUTTER`（VEC3）、付け根からの高さは `_FINH`（SCALAR）です。部位 6・7・8・9 が順に目・えらぶた・下あご・上あごです。

元モデルの `node.extras.swim` に加え、`node.extras.fishRig` に元キットの口の支点と開く角度、目の大きさ、ヒレの開き方などを保存します。GLBの部位は中立姿勢です。実行時に変形と体の泳ぎを適用します。
