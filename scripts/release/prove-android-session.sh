#!/usr/bin/env bash
# Isolated API-30 emulator; the device and adb never see the host network.
set -euo pipefail
cd "$(dirname "$0")/../.."
proof=$(realpath "${1:?proof workspace}")
sdk="${ANDROID_HOME:-$HOME/Android/Sdk}"
work="$proof/android-device"
mkdir -p "$work"
name="sunna-fix-mobile-$$"
cleanup() { docker rm -f "$name" >/dev/null 2>&1 || true; }
trap cleanup EXIT
accel=off
devices=()
if [[ -c /dev/kvm && -r /dev/kvm && -w /dev/kvm ]]; then
  accel=on
  devices=(--device /dev/kvm)
fi
docker run -d --name "$name" --network none "${devices[@]}" -v "$sdk:/sdk:ro" -v "$work:/device" \
  -e SUNNA_EMULATOR_ACCEL="$accel" \
  -e ANDROID_HOME=/sdk -e ANDROID_SDK_ROOT=/sdk -e ANDROID_AVD_HOME=/device -e HOME=/device \
  sunna-fix-builder bash -c '
    mkdir -p /device/proof.avd
    echo -e "path=/device/proof.avd\ntarget=android-30" >/device/proof.ini
    echo -e "image.sysdir.1=system-images/android-30/default/x86_64/\nabi.type=x86_64\nhw.cpu.arch=x86_64\nhw.lcd.width=480\nhw.lcd.height=800\nhw.lcd.density=160\nhw.ramSize=2048\nhw.cpu.ncore=4\ndisk.dataPartition.size=2048M" >/device/proof.avd/config.ini
    exec /sdk/emulator/emulator @proof -no-window -no-snapshot -no-boot-anim -no-audio -cores 4 -accel "$SUNNA_EMULATOR_ACCEL" -gpu swiftshader_indirect' >/dev/null
docker exec "$name" mkdir -p /proof/feed/B
docker cp "$proof/mobile/A.apk" "$name:/proof/A.apk"
docker cp "$proof/mobile/feed/B" "$name:/proof/approved"
mkdir -p "$proof/mobile/changed"
echo 'changed fixture' >"$proof/mobile/changed/sunna-android.apk"
SUNNA_RELEASE_KEY="$proof/test.key" SUNNA_TEST_PUBLIC_KEY="$proof/test.pub" \
  python3 scripts/release/manifest.py 0.2.0 "$proof/mobile/changed" http://127.0.0.1:48999/B
docker cp "$proof/mobile/changed" "$name:/proof/changed"
docker cp scripts/release/prove-android-session.py "$name:/proof/"
docker exec -d "$name" python3 -m http.server 48999 -b 127.0.0.1 -d /proof/feed
docker exec -e SUNNA_FIX_CONTAINER=1 "$name" python3 /proof/prove-android-session.py
