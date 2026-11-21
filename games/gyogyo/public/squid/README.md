# イカの素材（Squid Kit）

- 出どころ: https://cda-social.github.io/fish/gallery#squid の `kit/squid-kit.zip`（2026-11-17 取得）
- ライセンス: CC0 1.0（`LICENSE.txt`。three.js はこの置き場に入っていない）
- 置いた物: `squid/` の `aori`（アオリイカ）・`kouika`（コウイカ）の形の JSON と、背・腹・横の写真（webp）。エギ（`egi/`）・`example.html`・`squid-kit.js` は置いていない。
- 組み立て（`rust/web/build.sh`）が `public/gyogyo/squid/` へ写し、ゲームはページから相対の `squid/<id>.json` などを読む。

## 移植した物（`rust/game/src/view/squid_shape.rs`・`squid.rs`）

- `squid-kit.js` の `// src/game/squid/squid-species.ts`（種の値）と `// src/game/squid/squid.ts` の `formOf`・格子の形・`Tube`・`bakePhoto`、`Squid.update` の `buildMantle`・`buildFins`・`buildArms`・`buildTentacles`・`bendFunnel`。three 版と同じ細かさで、部品ごとの頂点が 1e-8 m の差で一致する（試験 `matches_three_reference`）。
- 背・腹・横の写真を頂点ごとに混ぜる作りは、読み込みのときに部品ごとの 1 枚の模様へ焼く（ひれの透け、コウイカの腕の白さ、腕の吸盤も焼く）。
- 動きは `SquidBrain` の `hover`・`backward`（外套の先を前にひれで進む）・`escape`（ジェット）と `updateJet` の姿勢の値を、走者の速さ・ダッシュ・跳ぶ・よろけ・喜びから決める。

省いた物: 皮の色の変化（`SKIN_FRAG`・`SKIN_DASH`）、墨と水しぶき（`SquidFx`）、エギと抱く・触腕を伸ばす動き、地面に沿わせる処理。
