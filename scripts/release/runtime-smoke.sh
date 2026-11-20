#!/bin/bash
set -euo pipefail
for tool in cargo rustc cc; do
  if command -v "$tool" >/dev/null; then echo "unexpected toolchain: $tool" >&2; exit 1; fi
done
export HOME=/root/sunna-fix-home
export PATH="$HOME/.local/bin:$PATH"
mkdir -p "$HOME"
TMPDIR=/noexec sh /release/install.sh host
sunnad --version
# Synthetic source exercises the listener and encoder without GPU/display access.
sunna-update supervise host --source synthetic --codec h264 --no-audio --no-clipboard --listen 127.0.0.1:0 > /tmp/host.log 2>&1 &
host=$!
sleep 12
kill -0 "$host"
grep -q 'sunnad listening' /tmp/host.log
kill "$host"
TMPDIR=/noexec sh /release/install.sh app
sunna --version
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
set +e
xvfb-run -a dbus-run-session -- timeout 12 "$HOME/.local/bin/sunna" >/tmp/app.log 2>&1
status=$?
set -e
if [ "$status" != 124 ]; then cat /tmp/app.log; echo "App exited unexpectedly: $status" >&2; exit 1; fi
grep -q '"healthy": true' "$HOME/.local/share/sunna/app/state.json"
grep -q '"healthy": true' "$HOME/.local/share/sunna/host/state.json"
echo 'PASS: host installed/listened; app installed/ran under Xvfb; no toolchain.'
