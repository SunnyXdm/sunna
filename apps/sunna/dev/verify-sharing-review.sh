# Sourced by verify-sharing-linux.sh inside its disposable Xfce test setup.
# All status and DOM checks with credentials stay inside the container.
docker exec -i "$name" sh -c 'cat >/opt/read-sharing.py' <<'PY'
import pyatspi, sys, time
text = sys.argv[1]
for _ in range(100):
    nodes = [pyatspi.Registry.getDesktop(0)]
    while nodes:
        node = nodes.pop()
        try:
            role = node.getRoleName()
            if role == 'password text':
                continue
            nodes.extend(node)
            if text in (node.description or "") or text in (node.name or ""):
                sys.exit(0)
            if role in ('static', 'label', 'status bar', 'paragraph', 'text', 'code', 'section'):
                try:
                    value = node.queryText().getText(0, -1)
                except (LookupError, RuntimeError, NotImplementedError):
                    value = node.name
                if text in (value or ''):
                    sys.exit(0)
        except (LookupError, RuntimeError, NotImplementedError):
            pass
    time.sleep(.2)
sys.exit('Missing expected UI text: ' + text)
PY
expect_text() {
  if ! user python3 /opt/read-sharing.py "$1"; then
    user sh -c 'w=$(xdotool search --onlyvisible --name "^Sunna$" | tail -1); import -window "$w" /tmp/sunna-settings.png'
    root cat /tmp/sunna-settings.png >"/tmp/$name-status.png"
    return 1
  fi
}

# A PATH-only helper and an external user unit must remain untouched.
root sh -c 'printf "#!/bin/sh\ntouch /tmp/foreign-ran\n" >/usr/local/bin/sunna-host && chmod +x /usr/local/bin/sunna-host'
launch
if root test -e /tmp/foreign-ran; then
  echo 'The app ran the PATH-only helper.' >&2
  exit 1
fi
expect_text 'set up outside Sunna'
root test ! -e /tmp/foreign-ran
root rm /usr/local/bin/sunna-host
user sh -c 'mkdir -p ~/.config/systemd/user && printf "[Service]\nExecStart=/opt/source/sunna-host run\n" > ~/.config/systemd/user/sunna-host.service'
user systemctl --user daemon-reload
expect_text 'set up outside Sunna'
user rm /home/tester/.config/systemd/user/sunna-host.service
user systemctl --user daemon-reload

# A packaged helper emitting valid JSON on failure must not turn the switch on.
docker exec -i "$name" sh -c 'cat >/usr/bin/sunna-host && chmod 755 /usr/bin/sunna-host' <<'SH'
#!/bin/sh
echo '{"supported":true,"state":"on","managed_by":"cli","permissions":[],"starts_by_itself":true,"shows_login_screen":false}'
exit 1
SH
expect_text 'couldn’t read the host settings'

# The fixed package layout supports existing CLI installations.
root sh -c 'mkdir /opt/pkg && tar -xzf /opt/share-feed/sunna-host-linux-*.tar.gz -C /opt/pkg && install -m755 /opt/pkg/bin/sunnad /usr/bin/sunnad'
docker cp "$repo/scripts/sunna-host" "$name:/opt/package-helper"
root install -o root -g root -m755 /opt/package-helper /usr/bin/sunna-host
user /usr/bin/sunna-host setup --desktop >/dev/null
expect_text 'Sharing on your local network'
connect_viewer
user /usr/bin/sunna-host status --json | python3 -c 'import json,sys; assert json.load(sys.stdin)["managed_by"]=="cli"'
ui 'Share this computer'
sleep 1
if ! user systemctl --user is-active --quiet sunna-host; then
  expect_text 'Off'
else
  ui 'Stop sharing'
  expect_text 'Off'
fi
ui 'Share this computer'
expect_text 'Sharing on your local network'
connect_viewer

# Exercise the real generated ExecStart in systemd with a literal %n and quotes.
docker cp "$repo/scripts/sunna-host" "$name:/opt/helper-source"
docker exec -i "$name" bash <<'SH'
set -eu
sed '/^system_helper=/,$d' /opt/helper-source >/tmp/functions.sh
. /tmp/functions.sh
UNIT_DIR=/etc/systemd/system
UNIT_FILE=$UNIT_DIR/sunna-path-fixture.service
DESKTOP_UNIT_FILE=$UNIT_DIR/sunna-path-unused.service
LAUNCHER='/opt/percent%n slash\ quote"'
SELF=$LAUNCHER
SUNNA_MODE=desktop
printf '#!/bin/sh\ntouch /tmp/quoted-helper-ran\n' >"$LAUNCHER"
chmod +x "$LAUNCHER"
write_units
systemctl daemon-reload
systemctl start sunna-path-fixture.service
for _ in {1..50}; do [[ -e /tmp/quoted-helper-ran ]] && break; sleep .1; done
test -e /tmp/quoted-helper-ran
systemctl stop sunna-path-fixture.service
SH

# A real systemd invocation and child for public receipt validation.
docker exec -i "$name" bash <<'SH'
set -eu
mkdir -p /opt/sunna/host /run/sunna-host
printf '#!/bin/sh\nsleep 600 &\nwait\n' >/opt/sunna/host/recovery
chmod +x /opt/sunna/host/recovery
cat >/etc/systemd/system/sunna-host.service <<'UNIT'
[Unit]
Description=Sunna whole-machine host
[Service]
ExecStart=/opt/sunna/host/recovery system-serve host --system-host --name %H
[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable --now sunna-host.service
SH
expect_text 'Starting…'
expect_text 'hasn’t been checked yet'
check_json() {
  user /opt/helper-source status --json |
    python3 -c 'import json,sys;s=json.load(sys.stdin);assert [s["state"],s["shows_login_screen"]]==json.loads(sys.argv[1])' "$1"
}
check_json '["starting",null]'
root sh -c 'echo 127.0.0.1:48800 >/run/sunna-host/listening; echo "stale 1 frame" >/run/sunna-host/receipt'
check_json '["starting",null]'
root sh -c 'pid=$(systemctl show sunna-host -p MainPID --value); child=$(cat /proc/$pid/task/$pid/children); invocation=$(systemctl show sunna-host -p InvocationID --value); echo "$invocation $child frame" >/run/sunna-host/receipt; echo 0 >/run/sunna-host/shares-greeter'
check_json '["on",false]'
expect_text 'The login screen couldn’t be shared.'
root sh -c 'echo 1 >/run/sunna-host/shares-greeter'
check_json '["on",true]'
root systemctl stop sunna-host.service
expect_text 'Off until the next boot'
expect_text 'Copy command'
# Save a full-size WebKit capture for each appearance.
for view in gallery space; do
  ui "${view^}"
  user python3 -c 'import json,sys,pathlib; assert json.loads((pathlib.Path.home()/".sunna/config.json").read_text())["view"]==sys.argv[1]' "$view"
  expect_text 'Off until the next boot'
  user sh -c 'w=$(xdotool search --onlyvisible --name "^Sunna$" | tail -1); import -window "$w" /tmp/sunna-settings.png'
  root cat /tmp/sunna-settings.png >"$repo/apps/sunna/dev/design/shots-app/webkit-sharing-$view.png"
done
echo 'PASS real WebKit helper provenance, failed JSON, unit path, receipt, boot command.'
