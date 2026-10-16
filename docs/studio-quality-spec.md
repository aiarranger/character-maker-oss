# Character Studio 改修ビルド仕様：LOOKBOOK × ステージ優先

## 0. ゴールと範囲

- **対象**: `/character-maker`（`app/character-maker/page.tsx`、`OriginalCharacterMaker.tsx`、`MakerPresetEditor.tsx`、2つの Scene）。
- **対象外**: `/character-maker/classic`（aisns が埋め込む旧画面）。見た目も挙動も変えない。
- **ゴール**: 管理画面の見た目から、キャラクターを撮る「表紙スタジオ」に変える。
  - キャラクターが画面の主役になる。1440×900 で身長約520px（現状約150px）、スマホでも最初の画面に全身が入る。
  - キャラごとに色の世界を持つ。
  - 選ぶ、持たせる、向きを変える、保存する、のすべてで手応えが返る。
- **維持すること**: 全機能、保存形式、unlit（MeshBasicMaterial）の規則、新しい依存を足さないこと。
- **合格ライン**: §10 のルーブリック10項目がすべて8点以上。

## 1. 採用案と接ぎ木

**土台は LOOKBOOK（キャスト名鑑）** とする。番号付きのキャスト目次、透かしの巨大番号＋表示用の名前＋四隅のトンボで組む表紙の構図、キャラ色の世界、紙と墨の2色をベースにした配色を使う。

他の2案から次の要素を取り入れる。

- **Paper Cyclorama から**
  - ステージ優先の寸法。
  - 方位角に沿って弧を描くターンテーブル・カメラ。
  - カテゴリーを選ぶと顔へ寄る動き（既存の `chooseCategory` が framing を切り替えるのを使う）。
  - 初回だけの操作ヒント。
  - 無灯の接地影。
- **Gumdrop Toybox から**
  - 武器を常に見える「おやつトレイ」に並べる（丸皿のトークン1列。どれも1クリック）。
  - 押せるものに付ける厚み（リップ）。
  - 登場時の弾み。
  - 武器待ちのとき、皿が順に跳ねて知らせる演出。
- **審査員の指摘から**: canvas が150pxに潰れる問題、構図の余白、顔表示に武器が入る問題、二重の読み込み文言、メニューが閉じない問題、空の状態の点、文言の不一致、favicon、コントラスト。

**不採用にしたもの**

- 章タブで武器を隠す案。「登録した武器はすべて見えて1クリック」の要件に反する。
- 剣の軌跡を枠外へはみ出させる構図。クロップに見える。
- シャッターの閃光。カメラの補間で置き換える。
- 3段ドラッグ式のボトムシート。
- すりガラスの多用。GPU 負荷が上がる。
- 丸ゴシックとスプリンクル柄。幼く見える。

## 2. ユースケースと目標操作数

| ID | 利用者の目的 | デスクトップ | スマホ（390×844） | 現状 |
|---|---|---|---|---|
| U01 | 開いてすぐキャラを見る | 0操作。全身で canvas 高の66%以上 | 0操作。最初の画面に全身・220px以上 | スマホは画面外、デスクトップ150px |
| U02 | キャラを替える | 1クリック。表紙替えの演出あり | 横帯で1タップ | スマホはスクロール後 |
| U03 | 武器を持たせる | 1クリック。トレイは常に見える | 1タップ（端の武器でも横スワイプ1回まで） | スマホは5段のグリッド |
| U04 | 動きを替える | 1クリック | 1タップ。最初の画面に見える | 同じ |
| U05 | 大きさ・体型を変える | ドラッグ1回 | ドラッグ1回 | 同じ |
| U06 | 髪色を替える（自動で顔へ寄る） | 1〜2（≥1100px は細部が最初から開いている） | 2（細かく→色） | 3（顔は手動） |
| U07 | 表情を変える | 2〜3 | 3 | 3〜4 |
| U08 | 視点を変える | 1。ダブルクリックで全身⇄顔（P2） | 1 | 1 |
| U09 | 名前を付けて保存する | 名前をクリック→入力→保存する（1）。レイアウトのずれ0 | 同じ | 帯が差し込まれ44pxずれる |
| U10 | GLB・JSON を書き出す | 2。Esc・外側クリック・項目選択で閉じる | 2。パネルが切れない | 閉じない、左が切れる |
| U11 | 元に戻す | 1、または Ctrl/⌘+Z | 1 | 同じ |
| U12 | 保存データを開く | 2（保存データ→項目）。開いたら閉じる | 2 | 開いたまま残る |
| U13 | JSON読み込み・別名保存 | 2（•••→項目） | 2 | 同じ |
| U14 | キーボードで編集を始める | スキップリンク「編集へ移動」で Tab 2回以内 | — | Tab 40回 |

## 3. レイアウト

### 3.1 デスクトップ（幅1100px以上）

```
.selectionStudio { display:grid; grid-template-columns: var(--cast-w) minmax(0,1fr);
  grid-template-rows: 56px minmax(0,1fr); height:100dvh; overflow:hidden; background:var(--paper); position:relative }
--cast-w: 232px (1100–1439) / 248px (1440–1679) / 280px (≥1680)
--rail-w: 320px / 340px / 380px
.liveEditor { display:grid; grid-template: "bar bar" 56px "stage rail" minmax(0,1fr) / minmax(0,1fr) var(--rail-w); position:relative }
```

1440×900 での寸法は、ステージ 852×844、canvas 枠 852×752、キャラ（100%）約520px。1280×720 では、ステージ 728×664、canvas 枠 728×572、キャラ約400px。

**① ブランド枠**（1行目・1列目。page.tsx の header を置き換える）

- 中身は SVG マーク28px と、ワードマーク「Character Studio」（`<h1>`、15px W8、palt）。
- 地は `--paper-2`、下に1px の `--line`。
- タグライン「選んで、その場でカスタマイズ」と紺の帯は廃止する。

**② キャスト目次**（2行目・1列目）

- aside「キャラクターを選ぶ」、nav「キャラクター一覧」はそのまま。縦にスクロールするのはこの列だけ。
- 群見出し `<h2>`（オリジナル／ファンタジー／コミック）: 12px W6、右へ伸びる罫線、番号範囲「01–04」（display書体 11px）。`position:sticky; top:0`、高さ32。
- カード: 2列、gap 8、padding 0 12 16、縦横比4:5。1440px で幅約108px。
  - 肖像: `object-fit:cover; mix-blend-mode:multiply`。地は `color-mix(in oklab, var(--card-accent) 18%, var(--paper))` で、元画像の灰色の背景をキャラ色に染める。
  - 左上に番号「01」（display 800 13px）。
  - 下端に名前帯（`--paper-2` 92%、角丸8、W7 13px）。
- 選択中: `outline:2px solid var(--card-accent); outline-offset:2px`、translateY(-2px)、名前帯を accent で塗り白文字、番号を accent 色。
- hover: 画像 scale1.04、カード -2px。
- 列の上下に24pxのフェードマスクを付ける。
- 維持するもの: `aria-label="<名前>を選ぶ"`、`aria-current`。

**③ バー**（area bar、56px、`--paper-2`、下に1px線。ブランド枠と同じ高さ・同じ地にして、1本のマストヘッドに見せる）

- 左: 履歴のアイコンボタン（36px）が3つ。元に戻す／やり直す／リセットで、aria-label と title は現状のまま。その右に、変更があるときだけ「未保存」チップ（`--warn-bg` 地に `--warn-ink` 文字、6pxの点、12px W6）を出す。「新規作成」「保存済み」の文字と、文字のない点は出さない。
- 右:
  - 「保存する」: 主ボタン。`--ink` の塗り、白文字、高さ40、radius 999、padding 0 20。
  - 「書き出し」: `<details>/<summary>` のまま。白地、chevron の SVG。
  - 「•••」: `<details>`、40px の円、aria-label「その他の操作」。

**④ ステージ**（area stage、`position:relative; overflow:hidden; id="studio-stage"`）

層の構成は §5.1 に書く。ステージの上に置くものは次のとおり。

- **左上の名前ブロック**（z2）
  - 小見出し「01 · オリジナル」（11px W6、`--ink-2`、字間 .12em）。
  - その下に、名前入力そのものを見出しとして置く。W8、`clamp(36px,4vw,60px)/1.05`、palt、字間 -0.02em、背景は透明。
  - hover で1pxの破線の下線と鉛筆アイコン18pxを出す。focus で2pxの accent の下線にする。
  - 幅は `field-sizing:content`（非対応なら width:12ch）、max-width 46%。
  - 「名前」の span は srOnly にする。`aria-label="キャラクターの名前"` は維持する。
- **右上のカメラピル**（z3）
  - [全身|顔] と [正面|横|後ろ] を1本にまとめ、間に区切りを入れる。
  - 地は `rgba(251,249,245,.92)`、backdrop-filter blur(10px)。backdrop-filter を使うのはこのピルだけ。shadow-float、radius 999。
  - ボタンは高さ36、最小幅44。押下中は墨の塗りに紙色の文字。
  - role=group／aria-label「表示範囲」「見る方向」／aria-pressed は現状のまま。`.cameraBar` の帯は廃止する。
- **下中央の武器トレイ**（z3）: §5.4。
- **左下のヒント**（StageOverlay の中）: §6 の M8。
- **上中央の通知トースト**（z5）: §6 の M5。
- `.stageHeading`、`.actionBar` の帯、`.stageHint`、`.stageLoading` は廃止し、中身は上に書いた場所へ移す。

**⑤ レール**（area rail、`--paper-2`、左に1px線、flex column、overflow hidden）

1. **region「共通のカスタマイズ」**（padding 16 20、gap 14）
   - スライダー2本（大きさ・体型）。ラベルは13px W6。値は display 800 22px、tabular-nums、右寄せ。トラックは6px（#EDE5D8）で、塗り部分を accent にする（`style={{'--p': …}}`）。つまみは20px、白地に2pxの accent の枠、`0 2px 0 var(--lip)`。標準の位置に4pxの点を付ける。
   - 動き: ラベル行「動き」の右端に、再生／一時停止の36pxの円ボタン（SVG。文字の Ⅱ・▶ は廃止）を置く。その下に動きボタンを `grid repeat(n,1fr) gap 6` で並べる。各ボタンは高さ56で、アイコン20と11px W6 の文字（文字は現状の `label.replace('武器を','')`）。押下中は墨の塗り。
   - 「細かくカスタマイズ」: 閉じているときだけ表示する全幅のボタン。高さ48、＋アイコン付き。
   - 細部を開いている間は、コンパクト表示にする。スライダーは横2列、動きボタンはアイコンだけの40px。ただし region は DOM からも aria からも消さない。E2E が細部を開いたままこの region の「大きさ」「歩く」「待機」を押すため。
2. **閉じているとき**: ショートカットの格子を出す。3列、タイル高さ76。CategoryIcon 24px と12pxの項目名を載せ、aria-label は「<項目>を編集」。押すと `chooseCategory(項目)` で開く。レールの空きを絵で埋める役目も持つ。
3. **開いているとき**: aside「カスタマイズ項目」（flex:1、min-height:0）。
   - 見出し行: 「細かくカスタマイズ」（16px W7）と、×「詳細を閉じる」。
   - nav「編集する内容」: 3分割のセグメント。選択中は白地に影。
   - nav「見た目の項目」: flex-wrap で折り返す、アイコン付きの丸チップ。端で切れなくなる。
   - 編集欄（`.editor`）: 縦にスクロールするのはここだけ。editorHeading の説明 small は削除する。
4. **下端の保存データ**（`<details>`、高さ44）
   - 中身は cards アイコン、「保存データ」、件数バッジ。
   - パネルはレールの中で上へ開く（max-height 50%）。項目を開いたら閉じる。
   - 「このブラウザに保存・別の端末へはJSON」の1行は、このパネルの下へ移す。`.inspectorFooter` と `.libraryBar` の帯は廃止する。

**既定の開閉**: 幅1100px以上では細部を開いた状態で始める。セッションに `preview.detailsOpen` があれば、そちらを優先する。899px以下は閉じた状態で始める。

**開閉とカメラ**: 「細かくカスタマイズ」を押したら `chooseCategory(category)` を呼び、カテゴリーに合わせて顔か全身に寄せる。× で閉じたら、フォーカスを「細かくカスタマイズ」ボタンへ戻す。

### 3.2 幅900〜1099px（1024×768 など）

- キャスト列は96pxの1列にする。64pxの角丸の肖像の左上に番号バッジ、その下に名前（11px）。
- `--rail-w` は300px。ステージは約628×712。
- 武器トークンは40px。

### 3.3 幅899px以下（タブレット縦・スマホ）

1列で、スクロールは文書の1つだけにする。

1. **ブランド行**（48px。スクロールで流れる）: マーク28pxとワードマーク13px。
2. **キャスト帯**（84px）: `overflow-x:auto; scroll-snap-type:x proximity`、padding-inline 16、gap 8。
   - カードは56×70（4:5）で、番号バッジ（10px）付き。名前は選択中のカードの下にだけ11pxで出す。
   - 群の間は16pxあけ、縦の罫線を入れる。
   - 選択中のカードは、表示時に `scrollIntoView({inline:'center', block:'nearest'})` で中央へ寄せる。
   - 幅110pxの縦サイドバーは廃止する。
3. **バー**（`position:sticky; top:0`、52px）
   - 左に履歴のアイコンボタン3つ（各40）。右に「保存する」（高さ40、13px）、「書き出し」（文字を残す）、「•••」。合計は358px以内に収める。
   - 書き出しのパネルは `right:0`、`max-width:calc(100vw - 24px)` にして、左が切れないようにする。
4. **ステージ**（`position:sticky; top:52px; height:clamp(320px, 50svh, 640px)`）
   - 390×844 では422pxで、y=184〜606。
   - 名前は28px（左上、`max-width:calc(100% - 140px)`）。
   - カメラピルは右上で2段にする（全身|顔／正面|横|後ろ。各44×36以上）。
   - 武器トレイは下端の全幅の横スクロール帯にする。トークン48、左右12、snap、端のフェード。
   - canvas 枠はステージから `--tray-h` を引いた高さ。キャラは約235px。
5. **以降は通常の流れ**: 共通のカスタマイズ → ショートカットの格子または細部 → 保存データ。

その他の調整:

- `.characterList`、`.selectedWorkspace`、`.liveEditor`、`.editor` の内側の overflow は解除する。`.selectionStudio` と `.liveEditor` には position:relative を付け、`.srOnly` がページを伸ばさないようにする。
- globals.css で `html:has([data-studio-page])` に `scroll-padding-top: calc(52px + clamp(320px,50svh,640px))` を付け、sticky の下に要素が隠れないようにする。
- 820×1180: キャストのカードは64×80、ステージは約614px。

### 3.4 横向きスマホ（`max-height:500px` かつ `orientation:landscape`）

- 2列にする。左はステージ（52vw、sticky、高さ `100svh - 44px`）、上に44pxのバー。
- 右列だけが文書としてスクロールする。順番はキャスト帯 → 共通 → 細部 → 保存データ。
- `min-height:480/600` は外す。

### 3.5 幅320px

- 「書き出し」は chevron のアイコンだけにする。summary の文字は srOnly の span に残す（E2E の `hasText:'書き出し'` のため）。
- 履歴はアイコンだけにする。
- 横スクロールは0にする。

## 4. ビジュアルトークン

### 4.1 色

**ベースの色**（`.shell` / `.selectionStudio` に定義する。暗色テーマの専用モードは作らない）

| トークン | 値 | 用途・コントラスト |
|---|---|---|
| --paper | #F7F3EC | ページ |
| --paper-2 | #FBF9F5 | バー、レール、名前帯 |
| --card | #FFFFFF | 押せるものの地 |
| --ink | #17151C | 本文（paper に対して16.4:1） |
| --ink-2 | #4A4652 | 補助（8.3:1） |
| --muted | #6A6572 | 最も薄い文字（5.1:1）。#8590a3・#728097 などは全廃 |
| --line / --line-strong | #E3DCD0 / #CFC6B8 | 罫線・枠 |
| --lip | #E2D8C8 | 押せるものの厚み |
| --plate | #EFE7DD | 武器の皿 |
| --warn-bg / --warn-ink | #FFF2CC / #8A5A00 | 未保存（5.3:1） |
| --ok-ink | #1E6E42 | 成功 |
| --danger | #B4362A | エラー |

**キャラごとの `--accent`**

- `lib/character-maker/studio-palette.ts` に、選択ID（`original:<id>` と preset key）をキーにして、`{accent, look, group}` を持たせる。
- `studio-catalog.json`（aisns と共有）、保存データ、JSON には入れない。
- 表の値は実測済みで、paper に対して4.68〜7.40、白に対して5.18〜8.18。どれも文字にも塗りにも使える。

| 01 ナミ | 02 ハル | 03 ルクス | 04 モカ | 05 セレネ | 06 アズール |
|---|---|---|---|---|---|
| #B0512A | #3D6394 | #0A7686 | #A4581B | #387565 | #2F4FB0 |

| 07 コハク | 08 ジン | 09 アイリス | 10 ローワン | 11 セージ | 12 パイパー |
|---|---|---|---|---|---|
| #9A5C0E | #4F5868 | #34519E | #9C5A16 | #2E6A4E | #8E2A45 |

**派生色**

- `--accent-wash: color-mix(in oklab, var(--accent) 14%, var(--paper))`
- `--wall: color-mix(in oklab, var(--accent) 7%, #FBF9F5)`
- `--floor: color-mix(in oklab, var(--accent) 16%, #EDE6DA)`

**色の遷移**: globals.css に `@property --accent { syntax:'<color>'; inherits:true; initial-value:#B0512A }` を置き、`.selectionStudio { transition: --accent 480ms }` で色を移す。

**守ること**

- 白文字は `--accent` か `--ink` の上にだけ置く。wash の上の文字は `--ink` にする。
- グラデーションで塗った文字は使わない。
- 暗い帯は作らない。

### 4.2 文字

- `--font-ui`: "Hiragino Sans","Hiragino Kaku Gothic ProN","Noto Sans JP","Yu Gothic UI",Meiryo,system-ui,sans-serif
- `--font-display`（番号と数値の見出しだけに使う）: "Avenir Next Condensed","DIN Condensed","Bahnschrift","Roboto Condensed","Arial Narrow",var(--font-ui)
- ウェイトは400/600/700/800だけにする。合成になる650は廃止する。読み込まれていない Inter の指定と、`font-size:14px !important` はすべて削除する。

文字の大きさ（サイズ/行の高さ）:

| トークン | 値 | 用途 |
|---|---|---|
| --t-11 | 11/1.3 | 小見出し・バッジ |
| --t-12 | 12/1.4 | チップ |
| --t-13 | 13/1.5 | ラベル |
| --t-14 | 14/1.5 | UI の既定 |
| --t-16 | 16/1.5 | 入力 |
| --t-20 | 20/1.3 | パネルの見出し |
| --t-22 | 22/1 | 数値、display 800 |
| --t-name | clamp(36px,4vw,60px)/1.05 | 名前。W8 |
| --t-look | clamp(140px,26vh,300px)/.78 | 透かしの番号。display 800 |

- 見出し・ボタン・名前は `font-feature-settings:"palt"`、字間 -0.01em（名前は -0.02em）。
- 数値は `tabular-nums`。
- 改行は `line-break:strict`、ラベルは `text-wrap:balance`。`overflow-wrap:anywhere` は削除する（「鶏モモのハン/マー」のような途中改行を防ぐ）。

### 4.3 形・影・押せる見た目

- 角丸: 8（画像）、12（ボタン）、14（タイル）、22（トレイ・パネル）、999（ピル・主ボタン）。
- 影:
  - `--shadow-press: 0 2px 0 var(--lip), 0 1px 2px rgba(23,21,28,.08)`
  - `--shadow-float: inset 0 1px 0 rgba(255,255,255,.7), 0 12px 28px -12px rgba(23,21,28,.28)`
  - `--shadow-lift: 0 12px 26px -10px color-mix(in oklab, var(--accent) 55%, transparent)`
- 押せるもの: 白地、1pxの `--line-strong`、`--shadow-press`。
  - hover: translateY(-1px)。`(hover:hover)` の環境だけ。
  - :active: translateY(2px)、影なし、90ms。
  - 主ボタン: 墨の地に `0 2px 0 #000`。
- 選択中: ピルとセグメントは墨の地に紙色の文字。カードと皿は accent のリング2pxと lift。色だけで選択を示さず、✓や塗りの変化を併用する。
- フォーカス: `outline:2px solid var(--ink); outline-offset:2px; box-shadow:0 0 0 5px rgba(255,255,255,.9)`。墨の地の上では outline を accent にする。
- タッチの押せる範囲は44×44以上。余白は4の倍数にする。

### 4.4 アイコン

- `StudioIcon.tsx` に、20px・stroke 1.75・端と角が丸・currentColor の SVG を入れる。
- 種類: undo、redo、reset、play、pause、download（chevron）、more、cards、pencil、plus、close、idle、walk、run、guard、swing、orbit、pinch。
- `<title>` は入れない。ボタンの textContent を変えないため（E2E の `ready()` がメニュー項目の textContent で判定する）。
- 文字の記号 Ⅱ ▶ ↶ ↷ ⌄ は廃止する。

### 4.5 ブランド

- マーク: 28px、角丸7pxの墨の四角に、右が開いた白い「C」の輪（stroke 3.5）を描き、開いた所に accent の点（今のキャラの色）を置く。
- `app/icon.svg`: 同じ図形で、点は #B0512A に固定する。favicon の404 もこれで解消する。

## 5. ステージ

### 5.1 層（下から順に）

| z | 中身 | 担当 |
|---|---|---|
| 0 | `StageBackdrop`: ホリゾント（壁から床への曲面）＋紙の粒子＋周辺減光＋透かしの番号 | A |
| 1 | `.canvasSlot`（`position:absolute; inset:0 0 var(--tray-h) 0`）の中に Scene のルート（`absolute; inset:0`） | B が枠、A が Scene |
| 2 | 名前ブロック | B |
| 2 | `StageOverlay`（`pointer-events:none`）: ポスター、装備の輪、四隅のトンボ、ヒント | A |
| 3 | カメラピル、武器トレイ | B |
| 5 | トースト | B |

`--tray-h` は `calc(var(--token) + 40px)`。

**ホリゾント（A）**

- 背景は次の2層を重ねる。
  - `radial-gradient(55% 16% at 50% 92%, color-mix(in oklab,var(--accent) 20%,transparent), transparent 70%)`
  - `linear-gradient(180deg, var(--wall) 0%, color-mix(in oklab,var(--accent) 10%,#F4EFE6) 68%, var(--floor) 84%, color-mix(in oklab,var(--accent) 12%,#F1EBE1) 100%)`
- 硬い地平線は引かない。
- 紙の粒子: feTurbulence（baseFrequency .9、numOctaves 2）の data URI を opacity .05・multiply で重ねる。
- 周辺減光: `inset 0 0 120px rgba(23,21,28,.06)`。
- 透かしの番号: `--t-look`、色は `color-mix(in oklab,var(--accent) 15%,transparent)`、top 8・left 16、aria-hidden。

### 5.2 床と影（A、`StageFloor.tsx`。両 Scene の studio 表示で使う）

- **接地影**
  - 128pxの CanvasTexture に放射グラデーションを描く（中心 rgba(23,21,28,.38) → 45% で .16 → 透明）。
  - 水平な plane を y=+0.002 に置く。素材は `MeshBasicMaterial{transparent:true, depthWrite:false, toneMapped:false}`、renderOrder -1。
  - 大きさは足元の幅×約2.2で、キャラの size に比例させる。
  - 腰ボーン（`Hips` / `J_Bip_C_Hips`）の XZ 位置に lerp .2 で追従させる。
- **床リング**: ringGeometry、半径 .62〜.632（size に比例）、色は accent、opacity .22。
- **アルファの上限**: 追加するものはすべて最大アルファ .6 未満にする。E2E は alpha>250 の画素をキャラとして数えるため。
- **追加しないもの**: ライト、lit な素材。
- **置き換え**: 既存の円盤とリングの mesh 2枚は、studio 表示では置き換える。classic は今のまま残す。

### 5.3 カメラ（A）

**共通**

- 俯角は7°（6〜8°の範囲で調整）。
- OrbitControls: `enableDamping`、`dampingFactor .08`、`enablePan:false`。ズームは構図を基準に0.6〜4倍に制限し、`zoomToCursor` を有効にする。
- dpr は両方の Scene で [1,2]。

**オリジナル**

- 「最大サイズの枠で大きさを比べる」規則は残す。
- 全身の padding は 1.24→1.06 にする。足元は canvas の下端から6〜9%（影の入る余地）、頭上は4%以上あける。
- 目標:
  - 大きさ100%・武器なしで、キャラが canvas 高の66〜74%。
  - 構えたときは56%以上。
  - 剣の軌跡も含めて全身が枠に入る（クロップ0）。
- 顔表示: padding を 1.25→1.12 にし、頭の中心を上から45%の位置に置く。
- 武器構えの範囲計測（`measureOriginalArmedCameraBounds`）は、(キャラ, 武器, 体型, モデルtoken) をキーに Map でキャッシュする。視点や枠の大きさが変わっても計り直さない（現状は最大959msの long task）。

**プリセット（`studio` のときだけ）**

- fov は 32→28 にする。
- 全身の距離は modelHeight×1.7 を起点に調整し、オリジナルと同じ占有率（±5%）に揃える。並べたとき同じ大きさに見えることが条件。
- 武器構えの距離は現行の約0.8倍を起点にし、クロップ0を守る。`makerArmedCameraDistance` 本体は classic とテストのために変えず、係数を引数で渡す。
- 顔表示: `J_Bip_C_Head` の中心を上から45%に置く。

**顔表示では武器を隠す**: `userData.registeredWeapon` を持つ group を `visible=false` にする。表示だけの変更で、保存と GLB の書き出しには影響しない。

**補間**

- 視点（正面／横／後ろ）と表示範囲（全身／顔）を利用者が変えたときだけ補間する。position・target・zoom を、方位角に沿って弧を描くように damp で寄せる（λ≈9、約330ms）。
- 次の場合は即時に切り替える: 最初の構図、キャラの読み込み、大きさ・体型の変更、`prefers-reduced-motion: reduce`、`navigator.webdriver`（自動テスト）。
- ルートに `data-camera-settled="true|false"` を出す。

**ダブルクリック**: ダブルクリック／ダブルタップで `onFramingToggle?.()` を呼ぶ（P2）。

### 5.4 武器トレイ（B、`WeaponPicker`）

- 白い紙のカードにする（radius 22、padding 8 12、shadow-float）。
- トークンの大きさは `--token: clamp(40px, 3.6vw, 52px)`、スマホは48。13個を gap 6 で1列に並べ、収まらない幅では横スクロール（scroll-snap、端のフェード）にする。
- トークン: 円形の皿（`--plate`）に、`--token − 8px` の画像を `mix-blend-mode:multiply` で重ね、画像の台紙のベージュを消す。「持たない」は破線の円に×を描く。名前は aria-label と title にだけ持たせ、見た目の文字は出さない。
- 選択中: accent のリング2px（outline、offset 2）、皿の地を `--accent-wash`、translateY(-4px)。右上に16pxの accent の円と白い✓。
- 見出し: トレイの上辺から上へ出るタブ型のピル（12px W7）に「武器 · ケーキの剣」（選択中の名前）を出す。
- 武器待ち（`requestedMotion`）: ピルを吹き出しにして、「使う武器をクリックすると「武器を構える／振る」を始めます。」とキャンセルを出す。role=status は維持する（テストがこの文・「キャンセル」・role=status を確認する）。
- 通常時の説明文「画像をクリックして選ぶ · 選択は保存とJSONにも残ります」は削除する。help 用の small 要素と id は残し、中身を選択中の武器名にする。

### 5.5 読み込みと登場

- **Scene 内のオーバーレイ（A）**: 読み込み中は見えない `role=status`「キャラクターを準備しています…」だけにする。見える板を出すのはエラーのとき（role=alert と「もう一度読み込む」）だけ。
- **エディタの stageLoading（B）**: 文言を削除する。代わりに A の StageOverlay がポスターを出す。
  - 選んだキャラの肖像を canvas 枠の中央・下寄せ・高さ70%で置き、`blur(14px) saturate(.8)`、opacity .45 にする。
  - 足元で、2pxの accent の線が左右に往復する（1.2秒）。aria-hidden。
  - `thumbnailIsSheet` のキャラは肖像を出さず、線だけにする。
- **entered フラグ（B）**: マウント後に最初の ready で true にし、以後は戻さない。武器や髪の再読み込みで canvas を消したり、ポスターを再表示したりしない。

### 5.6 キャラ切替（表紙替え）

page.tsx の `selectCharacter` で、次の順に処理する。

1. 選択を切り替える前に `captureStageSnapshot()` を呼ぶ。`[data-stage-canvas] canvas` を半分の解像度に縮め、webp .8 で撮る（30〜40ms）。
2. `SwapGhost` を `position:fixed` で同じ矩形に重ねる。
3. 新しいエディタは key で再マウントされ、§6 の M1 で登場する。

## 6. モーション

**トークン**

- イージング: `--ease-out-expo cubic-bezier(.16,1,.3,1)`、`--ease-page cubic-bezier(.7,0,.2,1)`、`--ease-spring linear(0,.009,.035 2.1%,.141 4.4%,.723 15.5%,.938 21.4%,1.017 26.4%,1.04 30.7%,1.035 35.1%,1.007 45.6%,.998 56.1%,1)`。linear() が使えない環境では `cubic-bezier(.34,1.4,.64,1)` にする。
- 時間: `--d-fast 120ms`、`--d-base 240ms`、`--d-slow 480ms`。

| # | 瞬間 | 中身 | 担当 |
|---|---|---|---|
| M1 | 表紙替え | 残像: 420ms で opacity .9→0、blur 0→8px、saturate 1→0、translateY 0→12px。<br>色: `--accent` が480msで移る。<br>番号: translateY(16px)→0・opacity、360ms、out-expo。<br>名前: 60ms 遅れて同じ動き。<br>canvas 枠（初めて entered になったとき）: opacity 0→1、translateY(14px)→0、scale .97→1、460ms、out-expo。<br>影: scale .6→1、400ms | page は B、残像・影は A |
| M2 | ターンテーブル | カメラが弧を描く（330ms）。床リングが scale 1→1.06→1（300ms）。四隅のトンボが内側へ6px寄って戻る（240ms、spring） | A |
| M3 | 武器を持たせる | 皿: scale 1→1.15→1、rotate -10°→0、380ms、spring。<br>パン屑: 6粒（4〜6pxの円、accent／#F4C27A／#E8553A）を半径24〜36pxに飛ばし、500msで消す。<br>ステージ: 中央（canvas 高の48%）に accent の2pxの輪が scale .6→1.4・opacity .5→0、560ms。初回は出さない | 皿・粒は B、輪は A |
| M4 | 武器待ち | 皿が40msずつずれて、translateY 0→-6→0（360ms、spring）を1回 | B |
| M5 | 保存 | 保存ボタンの中に✓アイコンが出て、地が accent に変わり、1600msで戻る。aria-label と文字「保存する」は変えない。<br>トースト: ステージ上部の中央、translateY(-8px)→0、240ms。成功は4秒で自動で消え、エラーは残る。CLS 0 | B |
| M6 | 押す | :active で90ms、戻りは220ms の spring。hover は `(hover:hover)` の環境だけ | B |
| M7 | 細部を開く | inspector が opacity 0→1・translateY(8px)→0、240ms。閉じたらフォーカスを戻す | B |
| M8 | ヒント | 初回だけ、回転のアイコンと「ドラッグで回す」（`pointer:coarse` では「ドラッグで回す・ピンチで拡大」）。ステージで最初の pointerdown があったら200msで消す。localStorage `cs.hint.v1`（読み書きは try/catch） | A |
| M9 | キャストの hover | 画像 scale 1.04、カード -2px、240ms、out-expo | B |

**prefers-reduced-motion: reduce のとき**

- 移動・拡大縮小・弾み・パン屑・残像・カメラの補間は止め、opacity だけの120msの変化にする。
- `--accent` の補間は0msにする。
- キャラのモーションはコンテンツなので止めない。そのかわり、再生ボタンを常に見える位置に置く。

## 7. 文言ルール

**表記の統一**（両エディタで同じ言葉にする。WCAG 3.2.4）

| 用途 | 表記 | 備考 |
|---|---|---|
| 保存 | 保存する | プリセットの「保存」を変更する。ui-originals.e2e.mjs の1行も合わせて直す |
| 書き出しメニュー | 書き出し | プリセットの「書き出す」を変更する |
| メニュー項目 | 現状の文字のまま | 3Dモデル（GLB）／3DモデルのGLB／編集設定（JSON）／編集用のJSON。`ready()` が textContent を完全一致で見るので、アイコンも足さない |
| 細部 | 細かくカスタマイズ（開く）／×「詳細を閉じる」 | |
| 状態 | 未保存 | 変更があるときだけ出す |
| 保存データ | 保存データ | 「このキャラクターの保存データ」「保存したキャラクター」をまとめる |

**削除する文言**

- ヘッダーのタグライン。
- 武器の通常時の説明文。
- 「正面 · 全身」「自由視点」の小見出し。
- editorHeading の説明 small。
- 「新規作成」。
- 空の状態の点。
- 常に出ている「ドラッグで回転 · スクロールで拡大」。
- 二重に出ていた読み込みの文言。

**残す文言**

- 「「<名前>」を保存しました。」（role=status）。
- 武器待ちの文。
- エラーの文と再試行のボタン。
- 「別のキャラクターとして保存」「JSONを読み込む」。

**書き方**

- 短い体言止めか、動詞1語にする。
- 英語はブランドのワードマークだけにする。
- 説明が必要なものは title（ツールチップ）へ移す。

## 8. A と B の契約

**作業の順番**

- A は最初の10分で「契約コミット」を作る。中身は次の2つ。
  - `StudioStage.tsx`: 型付きの最小実装（何も描かないもの）を置く。
  - 両方の Scene に optional props を追加する。
- B はこのコミットを取り込んでから import する。マージは A→B の順に行う。

**Scene のルート要素**

- `<div data-stage-canvas data-camera-settled style={{position:'absolute', inset:0}}>` にする。studio 表示のときだけ（オリジナルは常に studio）。
- ルートの aria-label は外す。エディタの section「キャラクターの3Dプレビュー」が canvas とエラーの alert を含むので、それで名前が付く。
- `preserveDrawingBuffer:true` は維持する。

**props**

- `OriginalCharacterScene` / `CharacterMakerScene`: `accent?: string`、`onFramingToggle?: () => void`。
- `CharacterMakerScene` には `studio?: boolean` も足す。既定は false で、classic は今のまま。MakerPresetEditor は `studio={embedded}` を渡す。
- `StageBackdrop`: `{ look: string }`
- `StageOverlay`: `{ poster?: string; entered: boolean; weaponId: string; view: 'front'|'side'|'back'; framing: 'full'|'face' }`
- `captureStageSnapshot(): { src: string; rect: DOMRect } | null`
- `SwapGhost`: `{ snapshot: {src:string; rect:DOMRect} | null; onDone: () => void }`

**CSS 変数**

- B が `.shell` と `.selectionStudio` に定義する。A は `var(--x, 既定値)` で読むだけ。
- 対象: `--accent --accent-wash --wall --floor --paper --paper-2 --ink --ink-2 --muted --line --font-ui --font-display --ease-out-expo --ease-spring --t-look --tray-h --token`。

**B が組む DOM**

```
.stage[data-entered][data-view][data-framing]
  > StageBackdrop
  > .canvasSlot > Scene
  > StageOverlay
  > 名前ブロック
  > カメラピル
  > WeaponPicker
  > トースト
```

**エディタに新しく渡す props**（page から渡す。いずれも optional）: `accent`、`look`、`groupLabel`、`poster`。

## 9. 優先度と時間配分（2時間×2人）

**P0（最初の75分）**

- A:
  - 契約コミット。
  - Scene のルートを absolute に、dpr を [1,2] に。
  - 構図（padding、俯角、プリセットの距離）。
  - 顔表示で武器を隠す。
  - StageFloor。
  - 読み込みの板を見えない status にする。
  - OrbitControls の設定。
  - 武器構えの範囲のキャッシュ。
  - StageBackdrop。
- B:
  - トークン、書体、押せる見た目。
  - page のグリッド、ブランド、キャスト目次（群・番号・色）、パレット。
  - エディタのグリッドと要素の移設。
  - 武器トレイ。
  - レール（共通、ショートカット、細部、保存データ）。
  - スマホ・タブレット・横向き。
  - 文言の統一。
  - メニューの Esc・外側クリック（page で document のリスナーを1つ）。
  - トースト化。
  - app/icon.svg。

**P1（次の35分）**

- A: 弧を描く補間と settled 属性、StageOverlay（ポスター・装備の輪・トンボ・ヒント）、影の登場、captureStageSnapshot と SwapGhost。
- B: 表紙替えの配線、M3〜M7、細部を開いたら `chooseCategory` を呼ぶ・閉じたらフォーカスを戻す、スキップリンク「編集へ移動」、StudioIcon への置き換え。

**P2（時間が余れば）**

- キャストと武器のロービング tabindex。button のまま矢印キーで移動する（role=radio にはしない）。
- hover で GLB を fetch して先読み。
- ダブルクリックで全身⇄顔の配線。
- `[` `]` でキャラ送り。

**最後の10分**: 2人で統合し、§10 の計測を行う。

## 10. 受入チェックと審査ルーブリック

### 10.1 必須条件（すべて満たすこと）

**機能**: 次の操作が現状どおり動く。

- 12キャラの選択、武器13（「持たない」を含む）を1クリックで選ぶこと。
- 動き5種と再生／一時停止。
- 表情、細部の全項目。
- 保存、別名保存、保存データを開く、JSON の読み込み、JSON と GLB の書き出し。
- 元に戻す・やり直す・リセット、Ctrl/⌘+Z。

**計測**（Playwright）

- 1440×900: canvas の高さ ≥ 700px。大きさ100%・武器なしのナミの不透明画素の高さが canvas 高の66〜74%、構えたときは56%以上。クロップ0（bbox が端から2px以上離れている）。
- 1280×720: canvas の高さ ≥ 520px、キャラ ≥ 290px。
- 1024×768、820×1180、768×1024、390×844、360×740、844×390、320×640:
  - canvas の高さが `.canvasSlot` の高さと一致する（±2px。150px に固定される問題の解消）。
  - `scrollWidth == 幅`（横スクロール0）。
- 390×844: canvas が最初の画面に収まり、キャラ ≥ 220px。
- 顔表示: 武器の画素がなく、頭の中心が上から40〜50%。
- 保存と通知の前後で、canvas の `getBoundingClientRect` が変わらない。
- 書き出しと ••• が、Esc・外側クリック・項目選択で閉じる。書き出しのパネルが切れない。
- コンソールエラー0（favicon の404を含む）。

**テスト**

- `npm run typecheck`、`npm test` が通る。
- Studio の E2E（ui-originals、ui-original-swords、ui-original-appearance-render、ui-original-paused-restore、ui-original-legacy-import）が通る。
- classic の画面を1枚撮り、見た目が変わっていないことを確かめる。

### 10.2 審査ルーブリック（各0〜10。全項目8以上で合格）

採点の根拠には、1440×900／1280×720／820×1180／390×844 のスクリーンショットと 10.1 の計測値を項目ごとに添える。8点未満の項目があれば直してから採点し直す。

| # | 項目 | 8点の条件 | 10点の条件 |
|---|---|---|---|
| 1 | 主役性と構図 | 10.1 の占有率を満たす。足元が床に接地している | 表紙として撮った写真のような密度。どの幅でも余白が空白に見えない |
| 2 | アートディレクション | 12色の世界が背景・カード・選択・主ボタンで一貫する。番号・名前・トンボの構図が両エディタで同じ | キャラを替えるたびに別の表紙に見え、ブランドとして記憶に残る |
| 3 | タイポグラフィ | トークンの段階だけを使う。合成ウェイトと !important がない。途中改行がない。名前が display | 和文の palt と display 数字の組み合わせが雑誌の品質に見える |
| 4 | 色・コントラスト・ブランド | 文字4.5:1以上、非テキスト3:1以上。グラデ文字なし、暗色の帯なし。マークと favicon がある | 色の遷移とマークの点の連動まで破綻がない |
| 5 | ステージの描画品質 | 両エディタで影・レンズ・俯角が揃う。クロップ0。顔表示に武器が出ない。dpr2。読み込み文言が二重にならない | キャラ同士を並べたとき、足元の線と大きさが揃って見える |
| 6 | モーションと手応え | M1〜M5 が動き、reduced-motion では opacity だけになる | すべての操作に意味のある返答があり、どれも過剰でない |
| 7 | 操作の流れ | U01〜U14 の目標を達成。メニューが閉じる。状態表示は変更時だけ。トーストでレイアウトが動かない | 初見の利用者が説明を読まずに保存まで進める |
| 8 | レスポンシブ | 10.1 の全サイズで崩れがない。スマホはスクロールが1つ。タッチ44px以上 | 横向きと320pxでも美しい |
| 9 | アクセシビリティ | §11 の名前を維持。フォーカスが見える。スキップリンク。見出し（h1・h2） | 矢印キーでキャストと武器を移動でき、読み上げの状態も正しい |
| 10 | 性能と安定性 | 依存の追加なし。テストが通る。視点切替で200ms超の long task なし（CPU制限なし）。コンソールエラー0 | 4倍CPU制限でも、視点切替が100ms以内 |

## 11. 変えないもの

- **素材と照明**: unlit（MeshBasicMaterial）の素材。ライトや lit な素材を足さない。
- **データの既定値**: 全キャラが「ケーキの剣＋構える」で始まる（`tests/maker-cake-default` で固定）。
- **データとアセット**: 保存形式、JSON・GLB の中身、`studio-catalog.json`（aisns と共有）、モデル・テクスチャ・アニメーション。
- **旧画面**: `/character-maker/classic`（`CharacterMaker.tsx`、`character-maker.module.css`、`CharacterMakerScene` の studio=false の経路、`makerArmedCameraDistance` の既定の動作）。
- **依存とフォント**: 新しい依存パッケージを足さない。外部フォントを読み込まない。
- **編集ロジック**: 次の関数の本文は変えない。テストが本文を読むため（`chooseMotion` は AST で抜き出して実行される）。
  - `chooseMotion`、`chooseWeapon`、`resetAppearance`、`apply` / `applyCharacter`、`save`、`captureSession`、元に戻す／やり直す。
  - maker-direct-choices が探す文字列: `requestWeaponForMotion(`、`motionAfterWeaponChoice(value, pendingWeaponMotion)`、`weaponPicker.current?.focusChoices()`、`onCancelRequest={() => setPendingWeaponMotion(null)}`、`setMotion(chosenWeaponMotion.current ?? (weaponId === 'none' ? 'idle' : 'sword_idle'))`。
- **アクセシブル名と役割**
  - nav「キャラクター一覧」、button「<名前>を選ぶ」と aria-current。
  - textbox「キャラクターの名前」、button「保存する」。
  - `<details>/<summary>` の「書き出し」と「•••」（aria-label「その他の操作」）、メニュー項目の文字、「別のキャラクターとして保存」「JSONを読み込む」、file input「キャラクター設定JSON」。
  - region「共通のカスタマイズ」。中身は、スライダー「大きさ」「体型」、動きのボタン（aria-label 待機／歩く／走る／武器を構える／武器を振る）、「動作を一時停止」／「動作を再生」。
  - button「細かくカスタマイズ」。
  - complementary「カスタマイズ項目」、nav「編集する内容」（見た目／表情／動き）、nav「見た目の項目」。inspector 内の group（髪型・動作など）と、各スライダーの名前。
  - group「表示範囲」（全身／顔）、group「見る方向」（正面／横／後ろ）。
  - section「持たせる武器」、section「キャラクターの3Dプレビュー」（canvas を含む最初の要素）。
  - 「通知を閉じる」、保存の通知文（role=status）。
- **WeaponPicker の構造**: button 13、img 12、aria-pressed=true が1つ、listbox と select がない。武器待ちの文、「キャンセル」、role=status。
- **canvas の描き方**: 背景は透明のまま。canvas 内に足すものはアルファ0.6未満。

## 12. リスクと後続課題

- **カメラの補間と E2E の採寸**: 自動テスト（navigator.webdriver）と reduced-motion では即時に切り替え、`data-camera-settled` を出す。
- **細部を最初から開く変更**: 幅1100px以上で細部を開いた状態で始めるのは UI の既定値の変更。E2E の `category()` は開いた状態・閉じた状態のどちらにも対応している。
- **対応ブラウザ**: color-mix、@property、field-sizing、linear()、:has() は Safari 16.4 以降・Chrome 111 以降が前提。対応していない環境では、静的な色・即時の切り替え・固定幅になる。
- **display書体**: Avenir Next Condensed は Apple の端末にしかない。ほかの端末では DIN／Bahnschrift／Roboto Condensed で番号を描く。
- **肖像の multiply**: 白いルクスとモカの毛色が淡く染まる。目で確認し、必要ならルクスだけ混ぜる割合を10%に下げる。
- **大きく映る分、モデルの欠陥が目立つ**: ケーキの剣が板に見える、ナミの襟がにじむ、コミック4体の顔テクスチャが濁っている、ハル・ルクス・モカの待機がほぼ静止している、bodyHz 30 でコマ送りに見える。どれも表示側では直らないので、characters の PROCESS.md に沿った作業と Owner の判断に回す。
- **範囲外（別タスク）**
  - GLB の軽量化（ナミ24.5MB）。
  - 1.6MB の JSON をチャンクから分離する。
  - Canvas を常駐させ、キャラ切替のたびに WebGL を作り直さないようにする。
  - frameloop='demand'。
  - 保存データを開くと未保存の編集と履歴が消える件。現状の E2E が「確認なしで切り替わる」ことを固定しているので、Owner の判断が必要。
  - プリセットの保存データに、ほかのプリセットの分まで出る件。

## 13. 進捗記録

| 日時 | 内容 | 計測（1440×900 の canvas 高 / 武器なし / 構え / 390×844 のキャラ） |
|---|---|---|
| 2026-10-14 19:30 | 作り直し前（基準） | 328px / .59 / .45 / 68px（最初の画面に入らない） |
| 2026-10-14 21:05 | 1回目（ステージ A と画面 B を統合） | 752px / .673 / .479 / 161px |
| 2026-10-14 21:45 | 2回目（審査3名の指摘を修正） | 752px / .673 / .641 / 215px |
| 2026-10-14 22:30 | 3回目（審査3名の指摘を修正。スマホのカメラ操作を右の縦レールに、色相の重なり解消、透かし・ヒントを安全枠へ、エラー時は1回自動で読み直し） | 752px / .673 / .644 / 227px |

- 9つの画面幅（1440〜320、横向き 844×390 を含む）すべてで横スクロール0、canvas が枠と一致、最初の画面に canvas が入る、コンソールエラー0（`tools/character-maker/measure-studio.mjs`）。
- 2回目の審査（3名・§10.2）の平均は 1:5.3 2:6.7 3:6.7 4:7.3 5:6.3 6:7.0 7:6.7 8:6.0 9:8.0 10:7.7。この審査の指摘を2回目で直し、3回目の審査で採点し直す。
- 3回目の審査（修正前の 75f4adc を採点）の平均は 1:7.0 2:7.7 3:7.7 4:8.0 5:7.3 6:7.7 7:7.3 8:7.3 9:8.0 10:7.3。全項目8以上の合格ラインには届いていない。3回目の修正後は採点し直していない。
- 構えと振るを同じ構図（構えの範囲）にしたため、振りの頂点の数フレームで剣先が canvas の端に触れる（§5.3 のクロップ0と矛盾。カメラが動作ごとに動くのを避けるための判断）。
- キャストのカードに乗ったときの GLB 先読み（P2）は、同じ GLB の同時取得で本読み込みが「Failed to fetch」になったため外した。
