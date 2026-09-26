#!/usr/bin/env bash
# Run a command (default: the codec hardware tests) and ship its output to the
# dogfood log collector, so results reach the developer without copy-paste.
#
#   scripts/check.sh                              # cargo test --release -p sunna-codec
#   scripts/check.sh cargo test --release --workspace
#
# Uses SUNNA_LOG_URL / SUNNA_LOG_TOKEN from ~/.sunna/dogfood.env.
set -uo pipefail
cd "$(dirname "$0")/.."

ENV_FILE="${SUNNA_ENV:-$HOME/.sunna/dogfood.env}"
if [ -f "$ENV_FILE" ]; then
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi

if [ "$#" -eq 0 ]; then
  set -- cargo test --release -p sunna-codec
fi

OUT="$(mktemp)"
"$@" 2>&1 | tee "$OUT"
STATUS=${PIPESTATUS[0]}

if [ -n "${SUNNA_LOG_URL:-}" ] && [ -n "${SUNNA_LOG_TOKEN:-}" ]; then
  RUN="check-$(date +%s)-$$"
  BODY="$(python3 - "$OUT" "$STATUS" "$*" "$(git rev-parse --short HEAD 2>/dev/null)" <<'PY'
import json, platform, subprocess, sys
path, status, command, git = sys.argv[1:5]
def cmd(*args):
    try:
        return subprocess.run(args, capture_output=True, text=True).stdout.strip() or None
    except Exception:
        return None
env = {"role": "check", "git": git, "command": command, "exit_status": int(status),
       "os_version": cmd("sw_vers", "-productVersion"), "hw_model": cmd("sysctl", "-n", "hw.model"),
       "cpu": cmd("sysctl", "-n", "machdep.cpu.brand_string"), "arch": platform.machine()}
print(json.dumps({"kind": "env", "env": env}))
with open(path, errors="replace") as out:
    for line in out:
        print(json.dumps({"kind": "output", "line": line.rstrip("\n")}))
PY
)"
  if printf '%s\n' "$BODY" | curl -sS -m 10 -o /dev/null -w "%{http_code}" \
      -X POST "$SUNNA_LOG_URL/ingest" \
      -H "Authorization: Bearer $SUNNA_LOG_TOKEN" \
      -H "X-Sunna-Device: $(hostname)" -H "X-Sunna-Role: check" -H "X-Sunna-Run: $RUN" \
      -H "Content-Type: application/x-ndjson" --data-binary @- | grep -q 204; then
    echo "(results shipped to the log collector)"
  else
    echo "(could not ship results; is Tailscale up?)"
  fi
fi
rm -f "$OUT"
exit "$STATUS"
