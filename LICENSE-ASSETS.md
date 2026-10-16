# Asset licenses / 素材のライセンス

The source code is MIT (see `LICENSE`). Models, textures, images and animation data are licensed per path as below.
コードは MIT（`LICENSE`）。モデル・テクスチャ・画像・動作データは、パスごとに次の条件です。

## 1. CC BY 4.0 — AI Arranger のオリジナル素材

Licensed under [Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/) (CC BY 4.0).
Attribution / 表示: **"Character Studio assets by AI Arranger (CC BY 4.0)"** と、可能ならこのリポジトリへのリンク。

| Path / パス | 内容 |
|---|---|
| `distribution/characters/generic-human-a/**`, `generic-human-b/**`, `generic-robot/**`, `generic-cat/**` | オリジナル4体（ハル・ナミ・ルクス・モカ）のモデル・動作・付属データ |
| `parts/assets/studio-runtime-v1/models/**` | 上の4体の表示用モデル（髪・頭の分割版を含む） |
| `parts/assets/studio-runtime-v1/original-parts/**`, `public/character-maker/original-parts/**` | 4体の髪型・パーツのサムネイルと一覧 |
| `parts/assets/studio-runtime-v1/portraits/{haru,nagi,selector-lux,selector-moka}*` | 4体の肖像 |
| `public/review-swing-2026-10-02/**` | 食べ物の武器12点（モデル・サムネイル・一覧） |

How these were made / 作り方: concept sheets were generated with ChatGPT image generation, converted to 3D with Tripo (paid plan), then rigged, animated and hand-edited by AI Arranger. 原画は ChatGPT の画像生成、3D 化は Tripo（有料プラン）で行い、AI Arranger がリグ・動作・手作業の加工を加えました。外部の画像は参照していません。

Not granted / 含まないもの: trademark rights and publicity rights. 商標権・パブリシティ権は許諾に含みません。

## 2. CC0 1.0 — 第三者の CC0 原本

| Path / パス | Source / 出典 | License file |
|---|---|---|
| `parts/sources/base/**` | Quaternius — Universal Base Characters | `parts/sources/base/License_Standard.txt` |
| `parts/sources/outfits/**` | Quaternius — Modular Character Outfits - Fantasy | `parts/sources/outfits/License_Standard.txt` |
| `parts/sources/animations/**`, `distribution/characters/_shared/character-maker/sources/animations/**` | Quaternius — Universal Animation Library (Standard) | `parts/sources/animations/License.txt` |
| `parts/sources/fantasy-v1/**` | VRoid Project (pixiv) — VRoid β CC0 samples: Victoria Rubin, 桜田史利矢 (Sakurada Fumiriya), HairSample Male | `parts/sources/fantasy-v1/README.md`, `provenance.json` |

## 3. CC0 1.0 — 上の CC0 原本から作った派生物

AI Arranger dedicates these derived files to the public domain under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), the same as their sources. 原本と同じく CC0 1.0 とします。

| Path / パス | 内容 |
|---|---|
| `parts/assets/v1/**`, `extended-v1/**`, `fantasy-v1/**`, `mature-v1/**`, `thumbnails-v2/**` | 組み立て済みの体・髪・動作・肌・サムネイル |
| `parts/assets/studio-runtime-v1/maker-assets/**`, `public/maker-assets/**` | 実行時の部品（衣装の色違い、動作 JSON、のけぞりの動作など） |
| `parts/assets/studio-runtime-v1/portraits/selector-comic-*` | アメコミ候補の肖像 |

"VRoid" and "Quaternius" are names of their respective owners; CC0 does not grant trademark rights. Do not use them to imply endorsement.
