#!/usr/bin/env bash
set -eu
work=~/ext/work/site/r4
mkdir -p "$work/tmp"
export TMPDIR="$work/tmp"
export CARGO_TARGET_DIR="$work/target"
export CARGO_PROFILE_DEV_DEBUG=0
cargo build --locked --offline -j6 -p sunnad -p sunna-cli
