#!/usr/bin/env bash
set -euo pipefail
cd /src
export RUSTFLAGS='-C link-arg=-Wl,-rpath,$ORIGIN/../lib'
if [[ "$FEATURES" != *test-key* ]]; then export SUNNA_PUBLIC_RELEASE=1; fi
cargo build --release -p sunnad -p sunna -p sunna-update --features "$FEATURES" -j4
arch=$(uname -m)
for component in host app; do
  stage=$(mktemp -d); mkdir -p "$stage/bin" "$stage/lib" "$stage/share/sunna"
  cp -L /opt/ffmpeg/lib/libavcodec.so.* /opt/ffmpeg/lib/libavutil.so.* "$stage/lib/"
  cp -r /opt/ffmpeg/share/source "$stage/share/sunna/ffmpeg"
  cp /target/release/sunna-update "$stage/bin/"
  if [ "$component" = host ]; then
    cp /target/release/sunnad scripts/sunna-host "$stage/bin/"
    cp scripts/linux-desktop.sh scripts/wayland-desktop.sh scripts/system-host.sh "$stage/share/sunna/"
  else
    cp /target/release/sunna "$stage/bin/sunna-real"
    cat > "$stage/bin/sunna" <<'LAUNCH'
#!/bin/sh
here=$(dirname "$(readlink -f "$0")")
exec "$here/sunna-update" supervise app "$@"
LAUNCH
    chmod 755 "$stage/bin/sunna"
    cp -r apps/sunna/icons "$stage/share/sunna/"
  fi
  tar -czf "/out/sunna-$component-linux-$arch.tar.gz" -C "$stage" .
  rm -rf "$stage"
done
cp /target/release/sunna-update "/out/sunna-update-linux-$arch"
if [[ "$FEATURES" == *test-key* ]]; then
  cargo build --release -p sunna-client --example update-proof -j4
  cp /target/release/examples/update-proof /out/.proof-client
else
  python3 scripts/release/check-updaters.py /out "linux-$arch"
fi
