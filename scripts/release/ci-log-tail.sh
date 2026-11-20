#!/bin/bash
# Put the errors and the end of a build log on the run's summary page, so a
# failed build says why without opening the full log.
set -euo pipefail
log=${1:?log file}
[ -f "$log" ] || { echo "::error::No build log was written."; exit 0; }
python3 - "$log" <<'PY'
import re, sys
lines = open(sys.argv[1], errors="replace").read().splitlines()
# Compiler errors with the few lines that follow each, then the last lines.
picked = []
for i, line in enumerate(lines):
    if re.match(r"\s*(error(\[E\d+\])?:|ld: |Undefined symbols|fatal error)", line):
        picked += lines[i:i + 8] + ["…"]
text = "\n".join(picked[:120] + ["", "End of the log:"] + lines[-40:])[-12000:]
escaped = text.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
print("::error title=Why the build failed::" + escaped)
PY
