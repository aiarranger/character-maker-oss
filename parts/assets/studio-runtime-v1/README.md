# Character Studio runtime assets (studio-runtime-v1)

`/character-maker` が読む `public/models`・`public/maker-assets`・`public/portraits`・`public/character-maker/original-parts` の正本置き場。

- SHA が既存の `assets/v1`・`fantasy-v1`・`mature-v1`・`thumbnails-v2` や各キャラの experiments と一致するものは、そちらを source にする（二重保管しない）。
- 一致しない新規 GLB / 画像だけをこのフォルダへ置く。
- `public/*` は従来どおりバイナリ ignore。クローン後は同期する。

```sh
node tools/character-maker/sync-studio-runtime-assets.mjs
node tools/character-maker/sync-studio-runtime-assets.mjs --check
```

`manifest.json` の `entries[].sha256` が照合の正。原本の再圧縮・再生成はしない。
