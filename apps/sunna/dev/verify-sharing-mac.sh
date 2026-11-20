#!/usr/bin/env bash
# Platform-independent decisions on Linux, plus a type check of the Apple calls.
set -euo pipefail
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$PATH"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
rustc --edition 2021 --test apps/sunna/src/mac_host/model.rs -o "$work/model"
"$work/model"
echo "#[path = \"$PWD/crates/capture/src/macos/stop.rs\"] mod stop;" >"$work/capture.rs"
rustc --edition 2021 --test "$work/capture.rs" -o "$work/stop"
"$work/stop"
CC_aarch64_apple_darwin="$HOME/.local/bin/zcc-aarch64-macos" CARGO_INCREMENTAL=0 \
  cargo check --target aarch64-apple-darwin -p sunna -p sunnad -p sunna-host -p sunna-capture
