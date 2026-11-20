#!/usr/bin/env bash
# An app built with sunna-update/test-key, a feed signed for loopback :49062,
# and crates/client/examples/update-proof. The fixture never echoes the host key.
# docker build -t sunna-fix3-sharing -f apps/sunna/dev/sharing-test.Dockerfile apps/sunna/dev
# verify-sharing-linux.sh --build-fixture EMPTY_PRIVATE_DIRECTORY
# verify-sharing-linux.sh xfce|gnome APP FEED_DIRECTORY PROOF_CLIENT
set -Eeuo pipefail
trap 'echo "Fixture command failed at line $LINENO" >&2' ERR
if [[ ${1:-} = --build-fixture ]]; then
  repo=$(cd "$(dirname "$0")/../../.." && pwd)
  cd "$repo"
  work=$(realpath "${2:?empty private fixture directory}")
  [[ -d "$work" && -z "$(ls -A "$work")" ]]
  umask 077
  export SUNNA_RELEASE_KEY="$work/test.key" SUNNA_TEST_PUBLIC_KEY="$work/test.pub"
  export CARGO_TARGET_DIR="$work/target" PATH="$HOME/.cargo/bin:$PATH"
  trap 'rm -f "$SUNNA_RELEASE_KEY"' EXIT
  scripts/release/keygen.sh "$SUNNA_TEST_PUBLIC_KEY"
  cargo build -j4 -p sunna -p sunnad -p sunna-update --features custom-protocol,sunna-update/test-key
  cargo build -j4 -p sunna-client --example update-proof
  mkdir -p "$work/stage/bin" "$work/stage/share/sunna" "$work/feed"
  for bin in sunnad sunna-update; do strip -o "$work/stage/bin/$bin" "$work/target/debug/$bin"; done
  strip -o "$work/app" "$work/target/debug/sunna"
  strip -o "$work/viewer" "$work/target/debug/examples/update-proof"
  cp scripts/sunna-host "$work/stage/bin/"
  cp scripts/{linux-desktop,wayland-desktop}.sh "$work/stage/share/sunna/"
  # Archives are extracted under the container user's UID.
  chmod -R u+rwX,go+rX "$work/stage"
  chmod a+rx "$work/app" "$work/viewer"
  tar -czf "$work/feed/sunna-host-linux-$(uname -m).tar.gz" -C "$work/stage" .
  version=$("$work/stage/bin/sunna-update" --version | awk '{print $2}')
  python3 scripts/release/manifest.py "$version" "$work/feed" http://127.0.0.1:49062
  exit 0
fi
desktop=${1:?xfce or gnome}
[[ $desktop = xfce || $desktop = gnome ]]
app=$(realpath "${2:?test-key app}")
feed=$(realpath "${3:?signed feed}")
viewer=$(realpath "${4:?update-proof viewer}")
repo=$(cd "$(dirname "$0")/../../.." && pwd)
name=sunna-fix3-proof-$desktop-$$
docker run -d --name "$name" --privileged --cgroupns=private --shm-size=1g \
  --tmpfs /run --tmpfs /run/lock sunna-fix3-sharing >/dev/null
trap 'docker rm -f "$name" >/dev/null' EXIT
root() { docker exec "$name" "$@"; }
session=(DISPLAY=:72 XDG_CURRENT_DESKTOP=XFCE XDG_SESSION_TYPE=x11)
if [[ $desktop = gnome ]]; then
  session=(GDK_BACKEND=wayland XDG_CURRENT_DESKTOP=GNOME XDG_SESSION_TYPE=wayland
    XDG_SESSION_ID=c1 WAYLAND_DISPLAY=wayland-0 DBUS_SYSTEM_BUS_ADDRESS=unix:path=/run/share-test/bus)
fi
user() {
  docker exec -u tester "$name" env HOME=/home/tester XDG_RUNTIME_DIR=/run/user/1000 \
    DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus WEBKIT_DISABLE_DMABUF_RENDERER=1 \
    LIBGL_ALWAYS_SOFTWARE=1 SUNNA_UPDATE_URL=http://127.0.0.1:49062 \
    SUNNA_LOG_URL= SUNNA_LOG_TOKEN= "${session[@]}" "$@"
}
docker cp "$app" "$name:/opt/share-app"
docker cp "$viewer" "$name:/opt/test-viewer"
docker cp "$feed" "$name:/opt/share-feed"
root chmod a+rx /opt/share-app /opt/test-viewer
docker cp "$repo/tools/gnome-test" "$name:/opt/test"
docker exec -i "$name" sh -c 'cat >/opt/action.py' <<'PY'
import pyatspi, sys, time
from collections import deque
for label in sys.argv[1:]:
    deadline = time.monotonic() + 75
    while time.monotonic() < deadline:
        nodes = deque([pyatspi.Registry.getDesktop(0)])
        while nodes:
            node = nodes.popleft()
            try:
                if node.getRoleName() == 'password text':
                    continue
                if node.name == label and node.getState().contains(pyatspi.STATE_SENSITIVE):
                    action = node.queryAction()
                    if action.nActions and action.doAction(0):
                        break
                nodes.extend(node)
            except (LookupError, RuntimeError, NotImplementedError):
                pass
        else:
            time.sleep(.3)
            continue
        break
    else:
        sys.exit('Missing desktop control: ' + label)
    time.sleep(.5)
PY
ui() { user python3 /opt/action.py "$@"; }
shell_eval() {
  user gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell \
    --method org.gnome.Shell.Eval "$1"
}
login() {
  for _ in {1..60}; do root systemctl start user@1000.service 2>/dev/null && break; sleep .5; done
  user systemctl --user import-environment SUNNA_UPDATE_URL
  if [[ $desktop = xfce ]]; then
    user sh -c 'umask 077; touch "$HOME/.Xauthority"; printf "add :72 . %s\n" "$(mcookie)" | xauth -q; Xvfb :72 -auth "$HOME/.Xauthority" -screen 0 1100x800x24 -nolisten tcp >/tmp/xvfb.log 2>&1 &'
    sleep 1
    user dbus-update-activation-environment --systemd DISPLAY XDG_CURRENT_DESKTOP XDG_SESSION_TYPE
    user sh -c 'xfce4-session >/tmp/xfce.log 2>&1 &'
  else
    # Use the repository headless shell with its private mock logind and a real
    # systemd user bus. The login target pulls in generated XDG autostart units.
    root sh -c 'mkdir -p /run/share-test && dbus-daemon --system --fork --nopidfile --address=unix:path=/run/share-test/bus &&
      sed "/^dbus-daemon --system --fork/d" /opt/test/system.sh >/opt/test/mock.sh &&
      DBUS_SYSTEM_BUS_ADDRESS=unix:path=/run/share-test/bus bash /opt/test/mock.sh' >/dev/null
    user dbus-update-activation-environment --systemd DBUS_SYSTEM_BUS_ADDRESS XDG_CURRENT_DESKTOP XDG_SESSION_TYPE XDG_SESSION_ID WAYLAND_DISPLAY
    root sh -c 'sed -e "/^dbus-daemon --session/d" -e "/^pipewire >/d" -e "/^wireplumber >/d" -e "s@/tmp/xdg-1000@/run/user/1000@g" /opt/test/session.sh >/opt/test/share-session.sh'
    user env SIZE=1100x800 bash /opt/test/share-session.sh >/dev/null
    user systemd-run --user --unit=share-fixture-login -p BindsTo=graphical-session.target sleep infinity
    user systemctl --user start xdg-desktop-portal-gnome xdg-desktop-portal
    user systemd-run --user --unit=share-fixture-autostart -p Wants=xdg-desktop-autostart.target sleep infinity
    shell_eval "import('resource:///org/gnome/shell/ui/main.js').then(m=>m.overview.hide())" >/dev/null
  fi
}
launch() {
  user sh -c '/opt/share-app >/tmp/share-app.log 2>&1 & echo $! >"$HOME/app.pid"'
  ui Settings
}
connect_viewer() {
  for _ in {1..4}; do
    user sh -c 'set -a; . "$HOME/.config/sunna/host.env"; set +a
    addr=$(ip -4 route get 1.1.1.1 | awk "{for(i=1;i<NF;i++)if(\$i==\"src\")print \$(i+1)}")
    timeout 10 /opt/test-viewer hold "$addr:$SUNNA_PORT" 1' && return
    sleep 1
  done
  return 1
}
wait_host() {
  for _ in {1..90}; do
    if user sh -c '[ -x ~/.local/share/sunna/host/current/bin/sunna-update ] &&
       ~/.local/share/sunna/host/current/bin/sunna-update running host 2>/dev/null &&
       systemctl --user is-active --quiet sunna-host.service && pgrep -x sunnad >/dev/null'; then return; fi
    sleep .5
  done
  user systemctl --user show sunna-host.service -p ActiveState -p SubState -p Result -p MainPID -p NRestarts
  user sh -c 'python3 -c "import json,pathlib; state=json.loads((pathlib.Path.home()/\".local/share/sunna/host/state.json\").read_text()); print({k:state.get(k) for k in (\"current\",\"healthy\",\"starts\",\"failed\")})"'
  echo 'Host did not start' >&2; return 1
}

login
if [[ ${SUNNA_SHARING_REVIEW_ONLY:-0} == 1 ]]; then
  [[ $desktop == xfce ]]
  source "$repo/apps/sunna/dev/verify-sharing-review.sh"
  exit 0
fi
docker exec -d "$name" python3 -m http.server 49062 --bind 127.0.0.1 --directory /opt/share-feed
user sh -c 'test ! -e ~/.local/share/sunna/host/current && ! command -v sunna-host'
launch
ui 'Share this computer'
if [[ $desktop = gnome ]]; then
  for _ in {1..90}; do
    shell_eval "global.get_window_actors().some(a=>a.meta_window.title==='Remote Desktop')" | grep -q "'true'" && break
    sleep .5
  done
  shell_eval "import('resource:///org/gnome/shell/ui/main.js').then(m=>m.overview.hide())" >/dev/null
  for _ in {1..10}; do
    user test -s /home/tester/.config/sunna/portal-token && break
    shell_eval "global.get_window_actors().find(a=>a.meta_window.title==='Remote Desktop')?.meta_window.activate(global.get_current_time())" >/dev/null
    sleep 1
    if ! shell_eval "global.display.focus_window?.title" | grep -q 'Remote Desktop'; then continue; fi
    # GNOME 50's 660x500 dialog, centered on the fixture's 1100x800 monitor.
    if ! user python3 -c 'import pyatspi,sys;q=[pyatspi.Registry.getDesktop(0)]
while q:
 n=q.pop()
 if n.getRoleName()=="switch" and n.name=="Allow Remote Interaction" and n.getState().contains(pyatspi.STATE_CHECKED):sys.exit(0)
 q.extend(n)
sys.exit(1)'; then user bash /opt/test/click.sh 806 324; fi
    user bash /opt/test/click.sh 833 189
    sleep 1
  done
  user test -s /home/tester/.config/sunna/portal-token
fi
wait_host
user sh -c '~/.local/share/sunna/host/current/bin/sunna-host status --json' |
  python3 -c 'import json,sys;s=json.load(sys.stdin);assert s["state"]=="on" and s["managed_by"]=="app" and s["reach"]=="lan" and s["starts_by_itself"] and s["link"]'
connect_viewer
if [[ $desktop = xfce ]]; then
  user sh -c 'w=$(xdotool search --onlyvisible --name "^Sunna$" | tail -1); xdotool windowclose "$w"'
else
  shell_eval "global.get_window_actors().find(a=>a.meta_window.title==='Sunna').meta_window.delete(global.get_current_time())" >/dev/null
fi
sleep 1
user sh -c '! pgrep -x share-app >/dev/null'
connect_viewer
docker restart "$name" >/dev/null
login
wait_host
connect_viewer
# The final viewer sent Bye. Wait for its last heartbeat to expire before
# exercising the no-viewer stop path. Confirmation is a separate UI check.
for _ in {1..30}; do
  if user sh -c '~/.local/share/sunna/host/current/bin/sunna-host status --json' |
     python3 -c 'import sys,json;sys.exit(json.load(sys.stdin)["viewer"] is not None)'; then break; fi
  sleep .5
done
launch
confirm_stop_if_needed() {
  sleep 1
  if user python3 -c 'import pyatspi,sys
q=[pyatspi.Registry.getDesktop(0)]
while q:
 n=q.pop()
 if n.name=="Stop sharing" and n.getRoleName()=="button": sys.exit(0)
 q.extend(n)
sys.exit(1)'; then ui 'Stop sharing'; fi
}
if [[ $desktop == xfce ]]; then
  # Simulate a login entry Sunna cannot remove. The host must still accept a
  # viewer and the real WebKit switch must retain its last confirmed value.
  root chmod 0555 /home/tester/.config/autostart
  ui 'Share this computer'
  confirm_stop_if_needed
  sleep 1
  user python3 -c 'import pyatspi
q=[pyatspi.Registry.getDesktop(0)]
while q:
 n=q.pop()
 if n.name=="Share this computer":
  assert n.getState().contains(pyatspi.STATE_CHECKED)
  assert "still running" in n.description
  break
 q.extend(n)
else: raise AssertionError("Missing sharing switch")'
  connect_viewer
  # The viewer releases its host slot asynchronously.
  for _ in {1..30}; do
    if user sh -c '~/.local/share/sunna/host/current/bin/sunna-host status --json' |
       python3 -c 'import sys,json;sys.exit(json.load(sys.stdin)["viewer"] is not None)'; then break; fi
    sleep .5
  done
  root chmod 0755 /home/tester/.config/autostart
fi
ui 'Share this computer'
confirm_stop_if_needed
stopped='! pgrep -x sunnad >/dev/null && ! systemctl --user is-active --quiet sunna-host &&
  ! systemctl --user is-enabled --quiet sunna-host && test ! -e ~/.config/autostart/sunna-host.desktop'
for _ in {1..60}; do user sh -c "$stopped" && break; sleep .5; done
user sh -c "$stopped"
if user systemctl --user cat 'app-sunna\x2dhost@autostart.service' >/dev/null 2>&1; then
  echo 'The generated login service still exists after turning sharing off.' >&2
  exit 1
fi
echo "PASS $desktop: installed, connected, app exited, reconnected, restarted session, reconnected, disabled."
