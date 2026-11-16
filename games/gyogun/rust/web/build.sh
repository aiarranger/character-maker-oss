#!/bin/bash
# 魚群繚乱（games/gyogun）のブラウザ版を作る: リポジトリの public/gyogun に wasm・JS・素材を置く（git に入れない。next dev がそのまま配る）。docs/gyogun-spec.md
# FAST=1 は wasm-opt を省く（開発中）
set -e
# 道具の置き場（この Mac の決まり: ~/.local/share/muso-rust）。無ければ PATH の cargo・wasm-bindgen・wasm-opt を使う
T=$HOME/.local/share/muso-rust
if [ -d "$T" ]; then
  export PATH=$T/rustup/toolchains/1.99.1-aarch64-apple-darwin/bin:$T/tools/bin:$PATH
  export CARGO_HOME=${CARGO_HOME:-$T} RUSTUP_HOME=${RUSTUP_HOME:-$T/rustup}
  export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$T/target}
fi
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT"
cargo build --release -p gyogun-game --lib --target wasm32-unknown-unknown
OUT=$ROOT/../../../public/gyogun
mkdir -p "$OUT/fonts"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" "${CARGO_TARGET_DIR:-$ROOT/target}/wasm32-unknown-unknown/release/gyogun_game.wasm"
if [ -z "$FAST" ]; then wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue "$OUT/gyogun_game_bg.wasm" -o "$OUT/gyogun_game_bg.wasm"; fi
cp web/index.html "$OUT/"
cp game/assets/fonts/*.ttf "$OUT/fonts/"
# 素材と、きゃしーの配布（主役はここから読む）
for d in aisns sfx voice models music delivery; do rsync -a --delete "../public/$d" "$OUT/"; done
ls -la "$OUT/gyogun_game_bg.wasm" "$OUT/gyogun_game.js"
