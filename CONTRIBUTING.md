# Contributing / 参加のしかた

Issues と Pull Request を歓迎します。Issues and pull requests are welcome.

1. `pnpm install --frozen-lockfile && pnpm assets:sync`
2. 変更したら / before opening a PR: `pnpm typecheck` と `pnpm test` が通ること。画面を変えたら `pnpm dev` で動作を確かめ、必要なら `lib/character-maker/*.e2e.mjs` を実行してください（README「確認」）。
3. 素材（モデル・画像・動作）を追加するときは、出典とライセンスを `parts/sources/<名前>/provenance.json` などに記録し、`LICENSE-ASSETS.md` の表に追記してください。ライセンスが不明な素材は追加できません。
4. API キー、個人の会話ログ、ローカルの絶対パスを含むファイルはコミットしないでください。

By contributing, you agree that your code contributions are licensed under MIT and asset contributions under the license stated in LICENSE-ASSETS.md for their path.
