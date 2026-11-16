# 拾い物の模型（Tripo）

2026-11-15 に Tripo Studio で作った模型（AI Arranger、CC BY 4.0。条件はリポジトリの `docs/gyogun-spec.md`「素材と条件」）。

| ファイル | 使う所 | 面の数 | 模様 |
|---|---|---|---|
| `anman.glb` | 拾い物のあんまん（体力が戻る） | 4,166 | 256 四方 |
| `konpeito.glb` | 拾い物の金平糖 | 2,131 | 256 四方 |

作り方:

1. Tripo の「3D向け画像を生成」で、白っぽい背景に 1 つだけ置いた絵を作る。
2. その絵の「3Dを生成」→ HD モデル。
3. エクスポート（GLB・2k）。
4. 面と模様を減らす（gltf-transform 4.1.1）:

```bash
npx -y @gltf-transform/cli@4.1.1 weld raw.glb a.glb
npx -y @gltf-transform/cli@4.1.1 simplify a.glb b.glb --ratio 0.05〜0.3 --error 0.002〜0.01
npx -y @gltf-transform/cli@4.1.1 resize b.glb c.glb --width 256〜1024 --height 256〜1024
npx -y @gltf-transform/cli@4.1.1 prune c.glb <名前>.glb
```

大きさと置き場所はゲームが決める（`rust/game/src/view/candy.rs` がいちばん大きい寸法をそろえる。置き方は `view/items.rs`）。
