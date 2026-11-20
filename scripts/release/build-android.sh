#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/../.."
out=${1:?output}; mkdir -p "$out"; out=$(cd "$out" && pwd)
# Public releases must never silently use Gradle's debug-signing fallback.
key_config=${SUNNA_ANDROID_SIGNING:-$HOME/.sunna/android/signing.properties}
[ -f "$key_config" ] || { echo 'An Android release signing configuration is required.' >&2; exit 1; }
export SUNNA_PUBLIC_RELEASE=1
export SUNNA_ANDROID_SIGNING="$key_config"
(cd apps/android && ./gradlew --no-daemon assembleRelease)
cp apps/android/app/build/outputs/apk/release/app-release.apk "$out/sunna-android.apk"
python3 scripts/release/check-apk-key.py "$out/sunna-android.apk"
