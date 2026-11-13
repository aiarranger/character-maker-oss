#!/bin/bash
# ブラウザ版の Stage を作る: public/maker-stage に wasm と JS を置く（git に入れない。next dev がそのまま配る）。docs/rust-stage-plan.md
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
cargo build --release -p character-maker-stage --lib --target wasm32-unknown-unknown
OUT=$ROOT/../public/maker-stage
mkdir -p "$OUT"
wasm-bindgen --target web --no-typescript --out-dir "$OUT" "${CARGO_TARGET_DIR:-$ROOT/target}/wasm32-unknown-unknown/release/character_maker_stage.wasm"
if [ -z "$FAST" ]; then wasm-opt -O3 --enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue "$OUT/character_maker_stage_bg.wasm" -o "$OUT/character_maker_stage_bg.wasm"; fi
ls -la "$OUT/character_maker_stage_bg.wasm" "$OUT/character_maker_stage.js"
