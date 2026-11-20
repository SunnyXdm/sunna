#!/bin/bash
# Inside the private systemd proof container as the fixture user.
set -euo pipefail
[[ -f /.dockerenv && ${SUNNA_FIX_CONTAINER:-} == 1 && $(id -u) != 0 ]]
mkdir -p "$HOME/.local/bin" "$HOME/.config/sunna" "$HOME/.config/systemd/user"
export PATH="$HOME/.local/bin:$PATH"
export SUNNA_LOG_URL= SUNNA_LOG_TOKEN=
cp /proof/legacy/{sunnad,sunna-host} "$HOME/.local/bin/"
cat >"$HOME/.config/sunna/host.env" <<'EOF'
SUNNA_NAME=fixture
SUNNA_MODE=desktop
SUNNA_DISPLAY=:93
SUNNA_LISTEN=127.0.0.1:49106
SUNNA_TOKEN=fixture-only
SUNNA_UPDATE=off
EOF
cat >"$HOME/.config/systemd/user/sunna-host.service" <<EOF
[Service]
ExecStart=$HOME/.local/bin/sunna-host run
Environment=SUNNA_LOG_URL=
Environment=SUNNA_LOG_TOKEN=
Restart=on-failure
EOF
Xvfb :93 -screen 0 800x600x24 >/tmp/sunna-fix-xvfb-log 2>&1 &
display_pid=$!
python3 -m http.server 48993 -b 127.0.0.1 -d /proof/releases >/tmp/sunna-fix-feed-log 2>&1 &
feed_pid=$!
trap 'systemctl --user stop sunna-host.service; kill "$display_pid" "$feed_pid"' EXIT
systemctl --user daemon-reload
systemctl --user start sunna-host.service
for _ in $(seq 30); do SUNNA_TOKEN=fixture-only /proof/releases/A/.proof-client probe 127.0.0.1:49106 >/dev/null 2>&1 && break; sleep .2; done
old=$(systemctl --user show sunna-host.service -p MainPID --value)
[[ /proc/$old/exe -ef "$HOME/.local/bin/sunnad" ]]
sh /proof/releases/A/install.sh host >/tmp/sunna-fix-first-install
grep -Fq 'sunna-host migrate-service' /tmp/sunna-fix-first-install
[[ $(systemctl --user show sunna-host.service -p MainPID --value) == "$old" ]]
sh /proof/releases/A/install.sh host --restart >/tmp/sunna-fix-second-install
SUNNA_TOKEN=fixture-only /proof/releases/A/.proof-client status 127.0.0.1:49106 | python3 -c 'import json,sys; s=json.load(sys.stdin); assert s["supported"] and s["state"]["healthy"] and s["version"]=="0.1.0"'
echo 'PASS original live service migrated through signed entry point and reported update support'
