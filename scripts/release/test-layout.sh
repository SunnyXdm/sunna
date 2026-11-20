#!/bin/bash
# Verify unit paths remain valid after pruning, without contacting systemd.
set -euo pipefail
repo=$(cd "$(dirname "$0")/../.." && pwd)
helper=$(realpath "${SUNNA_TEST_HELPER:-${CARGO_TARGET_DIR:-$repo/target}/debug/sunna-update}")
[[ -x "$helper" ]] || { echo 'Build sunna-update or set SUNNA_TEST_HELPER to the test release executable.' >&2; exit 1; }
# Recovery validates every parent of its private install tree.
umask 077
work=$(mktemp -d "$HOME/.sunna-upd-layout.XXXXXX")
trap 'rm -rf "$work"' EXIT
export HOME="$work/home" XDG_CONFIG_HOME="$work/config" XDG_DATA_HOME="$work/data"
unset SUNNA_UPDATE_ROOT SUNNA_DESKTOP_SIZE
root="$XDG_DATA_HOME/sunna/host"
for version in 0.1.0 0.2.0; do
  mkdir -p "$root/versions/$version/bin" "$root/versions/$version/share/sunna"
  cp "$repo/scripts/sunna-host" "$root/versions/$version/bin/"
  printf '#!/bin/sh\necho sunnad %s\n' "$version" >"$root/versions/$version/bin/sunnad"
  printf '#!/bin/sh\nprintf "%%s\\n" "$@"\n' >"$root/versions/$version/share/sunna/linux-desktop.sh"
  chmod 755 "$root/versions/$version/bin/sunnad" "$root/versions/$version/share/sunna/linux-desktop.sh"
done
ln -s versions/0.1.0 "$root/current"
# Load definitions only. $0 is the installed script, as it is during setup.
generate_units() {
  bash -c '
  source <(sed '\''/^case "${1:-help}" in/,$d'\'' "$0")
  SUNNA_MODE=virtual SUNNA_DISPLAY=:1099
  write_units
' "$root/current/bin/sunna-host"
}
units="$XDG_CONFIG_HOME/systemd/user"
generate_units
grep -Fxq "ExecStart=:/usr/bin/env -- \"$root/current/bin/sunna-host\" run" "$units/sunna-host.service"
grep -Fxq "ExecStart=:/usr/bin/env -- \"$root/current/share/sunna/linux-desktop.sh\" :1099 1710x1112" "$units/sunna-desktop.service"
grep -Fxq "ExecStop=:/usr/bin/env -- \"$root/current/share/sunna/linux-desktop.sh\" stop :1099" "$units/sunna-desktop.service"
install -m 755 "$helper" "$root/recovery"
generate_units
grep -Fxq "ExecStart=:/usr/bin/env -- \"$root/recovery\" run-user host" "$units/sunna-host.service"
grep -Fxq "ExecStart=:/usr/bin/env -- \"$root/recovery\" run-desktop host :1099 1710x1112" "$units/sunna-desktop.service"
grep -Fxq "ExecStop=:/usr/bin/env -- \"$root/recovery\" run-desktop host stop :1099" "$units/sunna-desktop.service"
printf '%s' '{"format":1,"current":"0.2.0","healthy":true}' >"$root/state.json"
printf '%s' '0.2.0' >"$root/known-good"
rm -rf "$root/versions/0.1.0"
[[ ! -e "$root/current" ]]
[[ $("$root/recovery" run-desktop host :1099 1710x1112) == $':1099\n1710x1112' ]]
[[ "$root/current" -ef "$root/versions/0.2.0" ]]
[[ $("$root/recovery" run-desktop host stop :1099) == $'stop\n:1099' ]]
ln -sfn missing/0.2.0 "$root/current"
[[ $("$root/recovery" run-desktop host :1099) == :1099 ]]
[[ "$root/current" -ef "$root/versions/0.2.0" ]]
[ -x "$root/current/bin/sunna-host" ]
[ -x "$root/current/share/sunna/linux-desktop.sh" ]
[ -x "$root/recovery" ]
echo 'PASS user and virtual desktop unit commands; real recovery repaired current after pruning.'
