#!/usr/bin/env bash
# Two debug APKs for a disposable emulator. Only the fixture selects the loopback feed.
set -euo pipefail
cd "$(dirname "$0")/../.."
proof=$(realpath "${1:?proof directory with test.pub}")
work="$proof/mobile"
mkdir -p "$work/apps/android" "$work/feed/B"
export ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}"
export ANDROID_NDK_HOME="$ANDROID_HOME/ndk/27.1.12297006"
export SUNNA_TEST_PUBLIC_KEY="$proof/test.pub" SUNNA_ANDROID_SIGNING="$work/no-release-configuration"
cargo ndk -t x86_64 --platform 29 -o "$work/apps/android/app/build/rust/jniLibs" build --release -p sunna-android --features sunna-update/test-key
tar -C apps/android --exclude=build --exclude=.gradle --exclude=local.properties -cf - . | tar -C "$work/apps/android" -xf -
# Only fixture setup changes. All consent, verification, session and callback code comes from the working tree.
python3 - "$work" <<'PY'
from pathlib import Path
import sys
p=Path(sys.argv[1])/'apps/android/app/src/main/java/dev/sunna/app/MainActivity.kt'
p.write_text(p.read_text().replace('super.onCreate(savedInstanceState)', 'super.onCreate(savedInstanceState)\n        android.system.Os.setenv("SUNNA_UPDATE_URL", "http://127.0.0.1:48999/B", true)'))
PY
export ANDROID_USER_HOME="$work/android-user"
mkdir -p "$ANDROID_USER_HOME"
for spec in A:0.1.0:1000 B:0.2.0:2000; do
  IFS=: read -r label ver code <<<"$spec"
  sed -i -E "s/versionName = \"[^\"]+\"/versionName = \"$ver\"/;s/versionCode = [0-9]+/versionCode = $code/" "$work/apps/android/app/build.gradle.kts"
  (cd "$work/apps/android" && ./gradlew --no-daemon -x :app:buildRust assembleDebug)
  cp "$work/apps/android/app/build/outputs/apk/debug/app-debug.apk" "$work/$label.apk"
done
cp "$work/B.apk" "$work/feed/B/sunna-android.apk"
SUNNA_RELEASE_KEY="$proof/test.key" python3 scripts/release/manifest.py 0.2.0 "$work/feed/B" http://127.0.0.1:48999/B
