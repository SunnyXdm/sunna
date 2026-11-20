#!/bin/bash
# Exercise the deployed host's ordinary refusal with a current client.
set -euo pipefail
[[ -f /.dockerenv && $(id -u) -ne 0 && $(ls /sys/class/net) == lo ]]
old=${1:?old sunnad}
client=${2:?current login-test}
export DISPLAY=:96 XDG_SESSION_TYPE=x11
export SUNNA_TOKEN=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')
addr=127.0.0.1:49112
work=$(mktemp -d)
Xvfb "$DISPLAY" -screen 0 640x400x24 -nolisten tcp >"$work/x.log" 2>&1 &
x=$!
daemon=
trap 'if [[ -n $daemon ]]; then kill "$daemon" 2>/dev/null || true; fi; if [[ -n $x ]]; then kill "$x" 2>/dev/null || true; fi; rm -rf "$work"' EXIT
for _ in {1..30}; do xdpyinfo >/dev/null 2>&1 && break; sleep .2; done
"$old" --source screen --codec h264 --listen "$addr" --no-audio --no-clipboard >"$work/host.log" 2>&1 &
daemon=$!
for _ in {1..30}; do "$client" "$addr" check | grep -q '^ready' && break; sleep .2; done
"$client" "$addr" "$work/before.y4m"
kill "$x"
wait "$x" || true
x=
for _ in {1..12}; do
    state=$("$client" "$addr" check)
    [[ $state != busy* ]] && break
    sleep .25
done
printf 'Old host probe after display loss: %s\n' "${state%% *}"
[[ $state == ready* ]]
if "$client" "$addr" "$work/after.y4m" 2>"$work/refusal"; then exit 1; fi
grep -qE 'host refused the session|session ended before fixture steps finished' "$work/refusal"
echo 'Old host still reports Ready after display loss. New viewer reports an ordinary refusal or session end.'
