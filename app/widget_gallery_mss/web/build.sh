#!/usr/bin/env bash
# Build the WASM widget gallery demo and emit ES modules into ./pkg.
#
# Prerequisites (one-time):
#   rustup target add wasm32-unknown-unknown
#   cargo install wasm-bindgen-cli
#
# Then serve locally:
#   ./build.sh && python3 -m http.server --directory .
# and open http://localhost:8000
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo_root="$(cd "$here/../../.." && pwd)"

# The gallery's default features (map, terminal, ffmpeg) pull desktop-only
# backends — a browser has no PTY, no system libav, no native clipboard — so
# build with none of them; the wasm syngui feature set is pinned in Cargo.toml.
cargo build --release --target wasm32-unknown-unknown \
    --manifest-path "$repo_root/Cargo.toml" -p widget_gallery_mss \
    --no-default-features

wasm-bindgen --target web --no-typescript \
    --out-dir "$here/pkg" \
    "$repo_root/target/wasm32-unknown-unknown/release/widget_gallery_mss.wasm"

echo "Built → $here/pkg. Serve with: python3 -m http.server --directory \"$here\""
