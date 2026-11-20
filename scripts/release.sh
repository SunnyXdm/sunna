#!/usr/bin/env bash
# Intentionally local. The owner reviews, commits, and pushes a tag separately.
set -euo pipefail
cd "$(dirname "$0")/.."
[ -z "$(git status --porcelain)" ] || { echo 'Release preparation requires a clean checkout.' >&2; exit 1; }
python3 scripts/release/version.py "${1:?usage: scripts/release.sh X.Y.Z}"
cargo check -p sunna-update
git add Cargo.toml Cargo.lock apps/sunna/tauri.conf.json apps/android/app/build.gradle.kts
git commit -m "Release $1"
git tag "v$1"
printf 'Created local tag v%s. Review it, then push it when ready to publish. Nothing was pushed.\n' "$1"
