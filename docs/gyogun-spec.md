# 魚群繚乱（games/gyogun）仕様

Character Studio と同じリポジトリに入れる、魚の大群をなぎ倒す 3D アクション。
名前は **魚群繚乱（ぎょぐんりょうらん / GYOGUN RYORAN）**（Owner 2026-11-16）。前の呼び名に入っていた語が、他社のゲームシリーズの名前として知られているため、ゲームの名前・コード・URL・コマンドから外した（フォルダは `games/gyogun/`、配る場所は `/gyogun/`、コマンドは `pnpm gyogun:build`、乱舞のゲージと技の内部名は `ranbu`）。
由来: AI Arranger のゲーム「百菓繚乱」の浜辺の面を写して作った。2026-11-16 Owner「魚のフィールドだけを追加する」「character-maker 側で直接直す」。

## だれが何をして、どうなるか

| だれが何をする | 結果 |
|---|---|
| `pnpm gyogun:build` のあと `pnpm dev` で http://127.0.0.1:4284/gyogun/index.html を開く | タイトルが出る。出陣すると夕暮れの浜辺で、魚の大群が泳いで押し寄せる |
| 斬る | 魚が吹き飛び、撃破と連撃の数字が増える |
| 飴糸（U）で隊長・大将へ飛びついて斬る → 離れる → 斬る、を 3 回 | 猪鹿蝶 |
| 砦を 4 つ落とす | 巨大なキハダの大将「黄肌大王」が出る |
| 主役を選ぶ | きゃしーと、Studio から送った「つくったキャラ」1 体（下の「Studio で作ったキャラで遊ぶ」） |

## 今回の範囲（1 周目）

- gyogun のソース（sim・game・web）と、動かすのに要る共通部品（vendor/rust の character-actor・character-view・game-ui・sim-math）を `games/gyogun/` に写す。以後は character-maker 側で直す（gyogun とは別々に育つ）。
- 面は浜辺だけ。お菓子の面（菓子ヶ原）と、その敵・大将・目印の模型は入れない。面を選ぶ札は出さない。
- 主役は社内の配布（characters.aiarranger.jp）を読まない。きゃしー 1 体の配布ファイルを `games/gyogun/public/delivery/` に置き、同じ場所から読む。
- 出来上がり（wasm・JS・素材）は `public/gyogun/` に出す（git に入れない。`public/maker-stage/` と同じ扱い）。
- 操作・戦いの計算・HUD・乱舞・飴糸・猪鹿蝶は元のまま持ち込む。協力プレイ（みんなで）は入れない（下の「みんなで」）。

## 素材と条件

| 素材 | 出どころ | 条件 |
|---|---|---|
| 魚 10 種 | Fish Kit | CC0 1.0 |
| 食べ物の武器 12 種 | AI Arranger（Studio の武器と同じファイル） | CC BY 4.0 |
| 小物（岩・ススキ・柵・灯籠） | AI Arranger がプログラムで作った模型（社内の道具 wilds-props-build。外の素材は使っていない） | CC BY 4.0 |
| 拾い物の模型（あんまん・金平糖） | AI Arranger（Tripo で作成） | CC BY 4.0 |
| きゃしー（模型・動作・絵・声） | AI Arranger | CC BY 4.0（Owner 2026-11-16「きゃしーだけ入れる」） |
| 効果音 | Kenney（CC0）と AI Arranger（Stable Audio 3 で作成） | CC0 1.0 / CC BY 4.0 |
| 曲 4 本（タイトル・戦闘・大将・勝利） | AI Arranger（Stable Audio 3 で作成、2026-11-16。`games/gyogun/public/music/`） | CC BY 4.0（Owner 2026-11-16「Suno の曲は OSS に入れないほうがいい。OSS 向けに新しく作曲」） |
| 花札の絵（猪・鹿・蝶） | AI Arranger（`assets/cards/make.py`） | CC BY 4.0 |
| 字体 | Zen Kaku Gothic New・Shippori Mincho B1・Dela Gothic One・Yuji Syuku | SIL OFL 1.1 |

きよごん・あいちゃんなど、きゃしー以外の主役は入れない。

## Studio で作ったキャラで遊ぶ（2 周目、2026-11-16）

Owner 2026-11-16「自分の3Dモデルをメインキャラとして登録できる機能もあってもよい」「（Studio で作ったキャラを魚群繚乱の主役にする入口を）作る」。

| だれが何をする | 結果 |
|---|---|
| Studio（`/character-maker`）でキャラを選び、髪・色・武器を変えて「魚群繚乱で遊ぶ」を押す | 魚群繚乱が開き、そのキャラが主役で浜辺に立つ。選んだ武器を持っている |
| 斬る・強攻撃・乱舞・飴糸 | 武器を構えて振る。のけぞると ＞＜ の顔になる |
| 跳ぶ・避ける | そのキャラのジャンプ・回避の動きで動く |
| 魚群繚乱の武将選びを開く | きゃしーと「つくったキャラ」（最後に Studio から送った 1 体。名前と絵は Studio の物）が並び、選び直せる |
| ページを開き直す | 送ったキャラは残っている（次に Studio から送るまで） |

### 入口のカード（2026-11-16）

Owner 2026-11-16「（上のバーのボタンが）目立たない。（ゲームの）トップ動画を出す」「この画面は空きが大きいので、大きさを縮めてカードにする」。カードの文言は「魚群繚乱で遊ぶ」（Owner 2026-11-16）。

| だれが何をする | 結果 |
|---|---|
| Studio（`/character-maker`）を開く | 3D のステージの空いている所（左下、武器の並びの上）に、魚群繚乱のタイトル画面が動いているカードが出ている。カードには「魚群繚乱で遊ぶ」と書いてある |
| カードを押す | キャラを送って魚群繚乱を開く |
| 動きを減らす設定（prefers-reduced-motion）の端末で開く | 動画は流さず、止め絵のカードになる |

- 動画は魚群繚乱のタイトル画面を録った短い繰り返し（音なし・自動再生・数百 KB）。置き場は `public/character-maker/`。
- 埋め込み用（`CharacterStudio` を yukikau.jp などへ入れるとき）には出さない（単体のページだけ）。動画と止め絵は、埋め込み用のパッケージ（`pnpm pack:vendor`）には入れない（`tools/pack.mjs`。50 MiB の上限まで余裕が約 0.17 MiB しかないため）。

作り（`components/character-maker/GameCard.tsx`、形と大きさは `lib/character-maker/game-card-fit.ts`。魚々娘のカードと共通）:

| 項目 | 決まり |
|---|---|
| 動画 | `public/character-maker/gyogun-title.mp4`（H.264・480×270・24 コマ/秒・5 秒・音なし・約 200KB。終わりの 0.6 秒を始めへ重ねて、つなぎ目を消してある）。止め絵は `gyogun-title.jpg`（動画の最初のコマ）。魚群繚乱のタイトル画面（きゃしー）を録った物 |
| 置き場所 | ステージの左下（左から 34px、武器の並びの上 34px）。最初の案内（ドラッグで回す）は、カードの右へずらす |
| 大きさ | 幅 132〜220px。ステージの幅と高さから、構えたキャラの足もと・武器に重なりにくい幅を選ぶ。狭いステージでもいちばん小さいカードは出す（重なることがある）（3 系統のキャラの「構える」を、全身の正面・横・後ろで測った値） |
| 顔の表示 | キャラがステージいっぱいになるので、同じ場所で動画を畳んだ札（「魚群繚乱で遊ぶ」と丸い矢印）にする。肩に少し掛かる（Owner 2026-11-16「札でよい」） |
| 入口は 1 つだけ | ステージのカード（顔の表示では札）だけ。上のバーのボタンと「書き出し」メニューの項目は出さない。2026-11-17 から、このカードの上に魚々娘のカードを積む（docs/gyogyo-spec.md の「Studio の入口」。作りは `GameCard.tsx`・`game-handoff.ts` を共通にした） |
| 動きを減らす設定 | 動画は読みに行かず、止め絵を出す |
| 押したとき | キャラを送って開く（作成中は「姿を作っています…」、魚群繚乱が作られていなければ通知） |

窓の大きさ: どの窓でも、ステージの左下にカードを出す（Owner 2026-11-16「幅で出さないのはやめて常に出す」）。幅はステージが広いほど大きく（132〜220px）、顔の表示では動画を畳んだ札。上のバーのボタンと「書き出し」メニューの項目は無くした。キャラの読み込み中（ステージを測れていないあいだ）は出さず、測れたら出す。
- Studio の 16 体はどれも送れる（オリジナル 4 体・ファンタジー 8 体・アメコミ 4 体）。
- 入口は単体の Studio のページ（`/character-maker`）だけに出る（`CharacterStudio` の `gyogun`。既定は出さない。埋め込み用の `onPlay`「この姿で遊ぶ」とは別）。形は上の「入口のカード」のとおり（ステージのカード → 出せなければ上のバーの「保存する」の左のボタン → バーに入らなければ「書き出し」メニューの項目）。魚群繚乱がまだ作られていない（`public/gyogun/` が無い）ときは、飛ばずに「`pnpm gyogun:build` を実行してから」と知らせる。
- 渡し方: Studio が魚群繚乱用のキャラ ZIP（`.character.zip`。待機・歩く・走る・構え・振り・のけぞり・＞＜・ジャンプ・回避・必殺技のクリップが入った GLB と、武器の id）を作り、同じ場所のブラウザの控え（Cache Storage）に置いてから `/gyogun/index.html` を開く。魚群繚乱は始めるときにその控えを読む。サーバーには送らない。
- 魚群繚乱は ZIP を主役の部品（character-actor の持ち込みの読み方）で読む。ジャンプ・回避・のけぞりは、ZIP の GLB に `jump`・`dodge`・`hit` のクリップがあればそれを使う（配布の主役は今までどおり）。
- yukikau.jp 用の書き出し（`.character.zip`）と「この姿で遊ぶ」（埋め込み用）の形は変えない。
- 今回やらないこと: 魚群繚乱の画面の中でファイルを選んで読み込むこと（Studio を通らない GLB の持ち込み）、送ったキャラを 2 体以上ためること。

### 作り（2026-11-16）

| 項目 | 決まり |
|---|---|
| ZIP の形 | yukikau.jp 用と同じ（`character.json`・`model.glb`・`thumbnail.png`。武器は `character.json` の `weapon`、GLB に武器の形は無い）。`lib/character-maker/yukikau-package.ts` の同じ作り手を使う |
| GLB（プリセット 12 体） | yukikau.jp 用と同じ書き出し（武器を外した GLB に、構える・振るとゲーム用の動作が全部入っている） |
| GLB（オリジナル 4 体） | Stage の書き出しの `variant: 'game'`（`rust/game/src/original_export.rs`）: 通常用の形（武器も握った手も無い）に、待機・歩く・走ると `sword_idle`・`sword_attack` の両方を入れる。yukikau.jp 用の通常用には構える・振るが無いため |
| クリップ（23 本） | 待機・歩く・走る（プリセットは `idle`・`walk`・`run`、オリジナルは `preset:biped:idle` など）、`sword_idle`・`sword_attack`・`hit`・`face_hurt`、ゲーム用の 16 動作（`jump`・`dodge`・`skill` ほか。`docs/game-motions-spec.md`） |
| 渡し方 | 控え（Cache Storage）の名前 `character-maker-gyogun-v1`、キー `/gyogun/studio-character.zip`（`lib/character-maker/game-handoff.ts` の `STUDIO_GAMES.gyogun` と `games/gyogun/rust/web/index.html` で同じ値）。開く先は `/gyogun/index.html?from=studio` |
| 読み込み | `index.html` が控えを読んで `window.__gyogunStudioZip` に置き、Rust（`platform::studio_zip` → `hero::guest_hero`）が名簿の段で読む。読めた 1 体を、配布の主役の後ろに並べる（id は `studio`。保存のキーは `gyogun.hero`・`gyogun.weapon.studio`・`gyogun.honors.studio`） |
| `from=studio` | Studio から来た直後の印。そのキャラと Studio で選んだ武器（`none`・知らない id はゲームの既定＝ケーキの剣）を選んだ状態で始め、`gyogun.hero`・`gyogun.weapon.studio` に残す。読み終えたら URL から外す（開き直すと、最後に選んだ主役と武器のまま） |
| 控えが無い・壊れている | きゃしーだけで普通に始まる（理由は console に出す） |
| 動き | 構え・振りは GLB の `sword_idle`・`sword_attack`、ジャンプ・回避・のけぞりは `jump`・`dodge`・`hit`。乱舞（必殺技）は今までの持ち込みと同じく武器の振り（GLB の `skill` は入っているが使っていない） |
| ＞＜ | `hit` のクリップが持つ ＞＜ の節点（`maker-hurt-face`）の大きさと、モーフの重み（ファンタジーの ＞＜ の表情・オリジナルの閉じた目）を、のけぞりの重みで重ねる。その間、顔は読んだときの姿（Studio のふつうの顔）へ戻す（＞＜ の板はその顔の形に合わせてあるため） |
| 回避の向き | Studio の `dodge` はその場で転がる（進みがほぼ 0）ので、進む向きを向いたまま転がる。きゃしーの横へ転がる回避（進み 1.15 m）は今までどおり体を回す（進みが 0.2 m 未満なら回さない） |
| 武器の持ち方 | 右手の骨（`R_Hand`・`J_Bip_R_Hand`・`hand_r`）に、持ち込みの既定の向きで持つ（配布の主役のような、振りの間の持ち替えは無い） |
| 決め台詞の声 | つくったキャラには無い（読みに行かない） |
| 名前の字 | 魚群繚乱の字体は使う字だけに絞ってある（かな・英数・記号・JIS 第一水準の漢字）。それ以外の字（第二水準の漢字・絵文字）を名前に使うと、魚群繚乱では出ない |

確かめ: `lib/character-maker/ui-gyogun-handoff.e2e.mjs`（16 体。ステージのカード → 控え → ZIP の中身）、`lib/character-maker/game-handoff.test.mjs`・`game-card-fit.test.mjs`、魚群繚乱の `hero::hero::tests`（ZIP を読んで組み立て、構え・振り・跳ぶ・避ける・のけぞり・＞＜）。

## みんなで（協力プレイは入れない、2026-11-16）

Owner 2026-11-16「みんなで戦う機能は、OSS には残さず、抜く」「みんなでを押すと、百菓繚乱に飛ばす」。

| だれが何をする | 結果 |
|---|---|
| タイトルの左上「百菓繚乱でみんなで遊ぶ ↗」を押す | 百菓繚乱（https://hyakka.yukikau.jp/）が開く。協力プレイはそちらで遊ぶ |
| ひとりで出陣する | 今までどおり遊べる |

- 魚群繚乱には、協力プレイの部屋・合言葉・通信・刻みの同期・仲間の表示と、部屋のサーバー（`server/room.mjs`）を入れない。戦いの計算（`rust/sim`）も 1 人用だけにした（席・仲間が起こす・人数で変わる斬りかかりの数・刻みの同期 `lockstep` を外した。1 人用の計算は、外す前後で同じ種・同じ操作の戦い 10 本が 1 ビットも変わらないことを確かめた）。
- 「みんなで」は、今までその項目を選べた操作（クリック・指・キーボードとパッドの上下と決定）で選べる。確認の画面は挟まない。ブラウザ版は同じタブで移る。Mac で直接動かす版は既定のブラウザで開き、URL を標準出力にも出す。
- 札には外へ出る印 ↗ を付ける（`ui::link_of`・`ui::ONLINE_URL`、開くのは `platform::open_url`）。
- 札の文言は「百菓繚乱でみんなで遊ぶ」（2026-11-16 ChatGPT Pro のレビュー「別のゲームへ移ると分からない」。前は「みんなで（オンライン）」）。出陣のボタンから離れた左上のまま。
- 開く先は `https://hyakka.yukikau.jp/?stage=hama&coop`（浜辺の面を選んだ状態で、みんなでの入口から始まる。`coop` は百菓繚乱の側で対応するまで無視されるだけ）。

## 元のゲームと違う所

- わたあめ雲（空中でもう一度跳ぶと乗る雲）は、元はスイーツ敵の模型を平たくして描いていた。スイーツ敵は入れないので、丸を寄せた桃色の形をプログラムで作って描く（仮置き。見た目は Owner 未確認）。
- 斬ったときの火花の色は、元の浜辺と同じく敵の種類ごとの色のまま。
- 協力プレイ（みんなで）は入れていない。「みんなで」は元のゲームのページを開く（上の「みんなで」）。

## 公開先（2026-11-16 決定、未公開）

Owner 2026-11-16: 魚群繚乱と Studio の公開先は **gyogun.yukikau.jp**。X の投稿（11/17 予定）から来た人が、そのまま遊び、キャラを作って出陣できるようにする（ChatGPT Pro のレビューの推奨 B）。

| だれが何をする | 結果 |
|---|---|
| `https://gyogun.yukikau.jp/` を開く | 魚群繚乱（`/gyogun/`）が開く。ログイン不要 |
| 魚群繚乱から「キャラを作って出陣」 | Studio（`/character-maker`）が開く |
| Studio のカードを押す | そのキャラで魚群繚乱が開く（今と同じ。サーバーへは送らない） |

作り（2026-11-16 作った。まだ公開していない）:

- 配り方: Cloudflare Workers の静的アセット（百菓繚乱・かけっこと同じ yukikau.jp のアカウント）。サーバーの処理は要らない（協力プレイは入れないため）。
- Studio は Next.js の静的書き出し（`output: 'export'`）で `out/` に作り、`public/gyogun/` もその中に入れる。静的書き出しでは使えない物（`/` の redirects、旧画面 `/character-maker/classic` と、それだけが使う `app/api/character-maker/assets`）は、公開用の組み立てのときだけ外す。`/` から `/gyogun/` への移動は `_redirects` で行う。
- 大きさの確認: Studio の模型は最大 25,738,144 bytes で、Workers の 1 ファイル 25 MiB（26,214,400 bytes）以内。
- 公開の操作（`wrangler deploy`）は Owner の指示があってから。

公開前に確かめること: 新しく取得した公開版で、`/` → 魚群繚乱が開く、ログインなしで出陣できる、Studio から出陣できる、主要な素材が 404 にならない、PC の Chrome で動く。スマホは確かめた範囲だけを案内に書く。

| 項目 | 決まり |
|---|---|
| 書き出し | `pnpm gyogun:site`（魚群繚乱と Stage を組み、実行時素材を同期してから `tools/gyogun-site.mjs`）。Studio の写しを `.site-build/` に組み、`GYOGUN_SITE=1` の `next build`（`next.config.mjs` が `output: 'export'`、`/` の redirects なし）で `out/` へ。旧画面 `/character-maker/classic` と `app/api` は写さない。静的書き出しは中間の `.next` を必ず使うので、写しの中で組む（起動中の `pnpm dev` の `.next` とぶつからない。作業ツリーも変えない） |
| `out/` の中身 | Studio（`character-maker.html`・`_next/`）、`public/` のうち `gyogun`・`character-maker`・`maker-assets`・`maker-stage`・`models`・`portraits`・`review-swing-2026-10-02`、`_redirects`（`/ /gyogun/ 302`）、`404.html`（Next の書き出し。無い path は全部これ） |
| 大きさ（2026-11-16） | 431 ファイル・262,764,598 bytes（250.6 MiB）。最大は Studio の模型 25,738,144 bytes。`tools/gyogun-site.mjs` が 1 ファイル 25 MiB・20,000 ファイル（Workers Free の上限）・要るファイル・入れない物を確かめる（`--check` は確かめだけ） |
| 配り方 | `wrangler.jsonc`（`name` gyogun-yukikau、`assets.directory` ./out、`not_found_handling` 404-page、`routes` gyogun.yukikau.jp の custom_domain、`main` なし）。`.html` は付けない URL（`/character-maker`）で、`/gyogun/index.html` は `/gyogun/` へ移る（引数は残る） |
| 公開 | `pnpm gyogun:deploy`（書き出し → `npx -y wrangler@4.157.0 deploy`）。Owner の指示があってから |
| 手元の確かめ | `npx -y wrangler@4.157.0 dev --port 4345 --ip 127.0.0.1`（台帳の gyogun-site-preview） |
| キャラを作って出陣 | 魚群繚乱のタイトルの副ボタン。出陣のボタンの右（狭い画面では上）。押すと同じ場所の `/character-maker` を同じタブで開く（`ui::STUDIO_PATH`。Mac 版は http://127.0.0.1:4284/character-maker）。キー・パッドでは 出陣 → キャラを作って出陣 → みんなで の順 |

## 今回やらないこと・後で仕上げること

- Studio の 16 体を始めから主役に並べること（今は Studio から送った 1 体だけ）。
- つくったキャラの乱舞に GLB の必殺技（`skill`）を使うこと、振りの間の武器の持ち替え、決め台詞の声。
- アメコミの男性の顔（ローワンなど）は、＞＜ の線が顔の面に近く、角度によって見えにくい（閉じた目の板は出る）。
- スマホでの確認、速さの測定、Mac で直接動かす版の確認。
- Studio の vendor/rust と games/gyogun の vendor/rust は版が違うので、今回は別々に持つ。そろえるのは後。
