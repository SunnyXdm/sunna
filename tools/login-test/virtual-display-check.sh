#!/bin/bash
set -euo pipefail
[[ -e /.dockerenv && $(ls /sys/class/net) == lo && $(id -u) != 0 ]]
work=$(mktemp -d)
export XDG_RUNTIME_DIR="$work" SUNNA_DESKTOP=xfce PATH="$work:$PATH"
ln -s "$(realpath "$1")" "$work/sunnad"
printf '#!/bin/sh\nexec sleep 20\n' > "$work/startxfce4"
chmod 755 "$work/startxfce4"
[[ ! -e /tmp/.X11-unix/X100 ]]
bash "$2" :100 640x400 >"$work/start"
grep -q 'Starting display :100' "$work/start"
bash "$2" :100 640x400 >"$work/reuse"
grep -q 'Display :100 already running (640x400)' "$work/reuse"
[[ $(DISPLAY=:100 sunnad check-x11) == 640x400 ]]
echo 'Started Xvfb and recognized its existing verified 640x400 screen.'
[[ ! -e /tmp/.X11-unix/X101 ]]
Xvfb :101 -screen 0 640x400x8 -nolisten tcp >"$work/shallow.log" 2>&1 &
shallow=$!
trap 'kill "$shallow" 2>/dev/null || true' EXIT
for _ in {1..40}; do [[ -S /tmp/.X11-unix/X101 ]] && break; sleep .1; done
[[ -S /tmp/.X11-unix/X101 ]]
if bash "$2" :101 640x400 >"$work/refused" 2>&1; then exit 1; fi
[[ ! -e "$work/sunna-desktop/session101.pid" ]]
echo 'Rejected a live eight-bit display before starting its desktop session.'
