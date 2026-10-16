# hit-v1：プリセットの「のけぞり」（2026-10-16）

ファンタジー（VRoid β）とアメコミ（Quaternius）の4体ぶんの「のけぞり」（クリップ名 `hit`）の確認記録です。docs/main-character-spec.md

- 元の動き: UAL1（CC0）の `Hit_Chest`（0.33秒）。`distribution/characters/_shared/character-maker/sources/animations/UAL1_Standard.glb`
- 作り方: `node tools/character-maker/build-hit-motions.mjs`。build-extended-motions.mjs と同じ「世界回転の移し替え」で各体の骨へ移し、時間を2倍（約0.66秒）にする。
- 実行時に読むファイル: `public/maker-assets/{fantasy|comic}-{feminine|masculine}-hit-motions.json`（新しいファイル。既存の動作 JSON は変えない）
- このフォルダ: 各体の確認記録（入力の SHA、世界回転の誤差、最後の姿と待機の差、出力の SHA）

既存の動作 JSON を作り直すと、後から直した剣の動作が元に戻るので、のけぞりは別のファイルに足しています。
