# yukikau.jp への経路（2026-10-17）

キャラクターメイクで作ったキャラを、ゲーム yukikau.jp で使えるようにする。経路は2つ。

## 1. 作る → ダウンロード → yukikau.jp で読み込む

- 書き出しメニューに **「yukikau.jp 用（.character.zip）」** を足す。中身は yukikau.jp の持ち込み（`https://yukikau.jp/play?import=1`）がそのまま読める形にする。
  - `character.json`（schemaVersion 1）: `forward: "+Z"`（書き出しの向き）、`heightMeters`、`clips`（idle / walk / run の実際のクリップ名）、作者と条件（CC BY 4.0 / CC0、参照先はこの OSS の URL）
  - `model.glb`: 標準（待機・歩く・走る）の書き出し。オリジナルは `KHR_mesh_quantization` を外す（yukikau.jp の読み込みが対応していないため）。のけぞり `hit` と ＞＜ `face_hurt` も入る
  - `thumbnail.png`: ステージの撮影
- 書き出したあと、yukikau.jp の持ち込み画面へのリンクを出す（ファイルは利用者が選ぶ）。
- yukikau.jp 側の変更は不要（実際の読み込みコードで通ることを確認済み）。

## 2. yukikau.jp にキャラクターメイクを組み込む

- Studio（16キャラ・のけぞり・＞＜）を `CharacterStudio` コンポーネントとして書き出し、パッケージの入口 `@aiarranger/character-maker/CharacterStudio` にする。
- `onPlay` を渡すと各エディタに **「この姿で遊ぶ」** が出て、1 と同じ `.character.zip` の `File` を渡す。yukikau.jp は今の持ち込みと同じ読み込みに通す。
- オリジナル4体の表示用モデル（合計約97MB）は tgz に入らない（GitHub の1ファイル100MB・tgz 50MiB の上限）。公開 OSS リポジトリの固定コミットから、SHA-256 を確かめて取得する。

## 変えないもの

- 汎用の GLB 書き出し（glTF 標準の +Z 向き）と既存のクリップ。
- 保存データ（JSON）の形式。

## 実装メモ：2 の組み込み（v0.2.0）

- 本体は `components/character-maker/CharacterStudio.tsx`。`/character-maker`（`app/character-maker/page.tsx`）は `<CharacterStudio />` を出すだけ。
  - props: `onPlay?(file)`（各エディタへそのまま渡す）、`initialName?`（プリセットを開いたときの名前。控えがあれば控えが優先。オリジナルはキャラクターの名前のまま）、`className?`。
  - Studio に要る全体の指定（`@property --accent`、Studio があるときの `html`/`body` の背景と `body` の余白 0、スマホの `scroll-padding-top`、リンクの色）は `character-studio.module.css` に移した。埋め込み先は `globals.css` を読まなくてよい。
- パッケージ `@aiarranger/character-maker@0.2.0`（`node tools/pack.mjs`）
  - 入口: `./CharacterMaker`（従来どおり）、`./CharacterStudio`（追加）、`./server`（従来の `serveMakerAsset`・`copyMakerAssets` に加えて `ensureStudioModels`・`studioModels`・`STUDIO_MODELS_COMMIT`）。
  - 追加で入れたもの: `public/portraits/`、`parts/assets/original-hit-v1/*.json`（`original-weapon.ts` が読む）、`server/studio-models.mjs`、`studio-models.json`。
  - `public/models/`（9ファイル・約97MB）は入れない。`studio-models.json` に、公開パス・SHA-256・バイト数・取得元を書く。取得元は `https://raw.githubusercontent.com/aiarranger/character-maker-oss/f5ee2e34f1cd1251159266db5e9a87cbc9d5f26c/<parts/assets/studio-runtime-v1/manifest.json の source>`。
  - `ensureStudioModels(cacheDir)`: 足りない・大きさや SHA-256 の違うものだけ取り、確かめてから一時ファイル → rename で置く。置き場所は以後の `serveMakerAsset`・`copyMakerAssets` が使う（環境変数 `CHARACTER_MAKER_MODEL_CACHE` でも指定できる。既定は `node_modules/.cache/character-maker-models`）。
  - `serveMakerAsset`: `/models/*` はパッケージ内 → キャッシュの順に探し、無ければその1ファイルを取ってから届ける。`/portraits/*` はパッケージから届ける。
  - `copyMakerAssets(outRoot)`: 従来の部品に加え、用意済みのモデルを `out/models/` へ写す。足りなければ警告して残りを写す（戻り値は従来どおり写した数）。
- tgz は 50 MiB 未満であることを pack が確かめる。v0.2.0 は 49.8 MiB で、余裕は約 0.17 MiB。

## 実装メモ：1 の書き出し（2026-10-17）

- `lib/character-maker/yukikau-package.ts` が ZIP を作る（three 付属の fflate）。id は `maker-<モデルの SHA 先頭12桁>`、作者は「AI Arranger Character Studio」、条件はオリジナルが CC BY 4.0・プリセットが CC0 1.0。
- オリジナルは剣を構えていても標準（待機・歩く・走る＋のけぞり＋＞＜）で書き出し、頂点を 32bit に戻して `KHR_mesh_quantization` を外す。プリセットは武器を外した GLB（ゲーム側の武器と二重にならないように）。
- `heightMeters` は yukikau.jp の標準の人（1.15m）に合わせた換算: 1.15 × 実測の高さ ÷ 基準（オリジナル 2.0、プリセット 1.8 モデル単位）。
- 16体すべての ZIP が yukikau.jp の読み込みコード（@aiarranger/character-runtime の readCharacterFile）で受け付けられることを確認（E2E `ui-yukikau-package`）。
- yukikau.jp 側の確認（2026-10-17、aisns ブランチ claude/character-studio-embed-20261017）: 組み込みの Studio でナミ・セレネを選んで「この姿で遊ぶ」→ 野で前向きに立つ。ダウンロードしたナミの ZIP を `/play?import=1` で読み込み → 名前と作者が出て「このキャラで歩く」へ進める。
