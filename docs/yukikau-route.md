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
