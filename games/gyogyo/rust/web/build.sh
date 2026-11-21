#!/bin/bash
# 魚々娘（games/gyogyo）のブラウザ版を作る: リポジトリの public/gyogyo に wasm・JS・素材を置く（git に入れない。next dev がそのまま配る）。docs/gyogyo-spec.md
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
cargo build --release -p gyogyo-game --lib --target wasm32-unknown-unknown
OUT=$ROOT/../../../public/gyogyo
mkdir -p "$OUT/fonts"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" "${CARGO_TARGET_DIR:-$ROOT/target}/wasm32-unknown-unknown/release/gyogyo_game.wasm"
if [ -z "$FAST" ]; then wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue "$OUT/gyogyo_game_bg.wasm" -o "$OUT/gyogyo_game_bg.wasm"; fi
cp web/index.html "$OUT/"
cp game/assets/fonts/*.ttf "$OUT/fonts/"
cp -R game/assets/fonts/licenses "$OUT/fonts/"
# 素材と、きゃしーの配布（主役はここから読む）・魚・イカ
for d in aisns sfx models music delivery racers squid; do [ -d "../public/$d" ] && rsync -a --delete "../public/$d" "$OUT/"; done
cp ../public/favicon.svg "$OUT/"
ls -la "$OUT/gyogyo_game_bg.wasm" "$OUT/gyogyo_game.js"
