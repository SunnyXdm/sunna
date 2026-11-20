#!/bin/bash
# This display can produce frames but cannot deliver keyboard or pointer input.
set -euo pipefail
[[ -f /.dockerenv && ${SUNNA_FIX_CONTAINER:-} == 1 && $(id -u) != 0 ]]
if [[ $# == 0 ]]; then
  exec xvfb-run -a -s '-screen 0 800x600x24 -extension XTEST' bash "$0" in-display
fi

work=$(mktemp -d)
cleanup() {
  if [[ -n ${pid:-} ]]; then kill "$pid" 2>/dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT
tar -xzf /proof/releases/B/sunna-host-linux-x86_64.tar.gz -C "$work"
export SUNNA_LOG_URL= SUNNA_LOG_TOKEN= SUNNA_TOKEN=fixture-only
address=127.0.0.1:49108
args=(--source screen --codec raw --no-audio --no-clipboard --listen "$address")
/proof/legacy/sunnad "${args[@]}" >"$work/old.log" 2>&1 &
pid=$!
for _ in {1..25}; do
  if /proof/releases/A/.proof-client probe "$address" >/dev/null 2>&1; then break; fi
  sleep .1
done
/proof/releases/A/.proof-client probe "$address" >/dev/null
kill "$pid"
wait "$pid" || true
unset pid

status=0
SUNNA_READY_FILE="$work/ready" timeout 12 "$work/bin/sunnad" "${args[@]}" >"$work/new.log" 2>&1 || status=$?
[[ $status == 1 && ! -e "$work/ready" ]]
grep -Eiq 'XTEST|Unsupported extension' "$work/new.log"
echo 'PASS legacy host answered probes without XTEST; candidate refused readiness on the same display'
