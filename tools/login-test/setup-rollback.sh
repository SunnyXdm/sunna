#!/bin/bash
# Disposable container with no systemd, network or host configuration mounts.
set -eu
[[ -f /.dockerenv && ! -d /run/systemd/system && ! -e /etc/sunna ]]
unit=/etc/systemd/system/sunna-host.service
mkdir -p /opt/sunna/host/current/bin /etc/systemd/system
ln -s /bin/true /opt/sunna/host/recovery
ln -s /bin/true /opt/sunna/host/current/bin/sunnad
cat >/usr/bin/systemctl <<'STUB'
#!/bin/bash
case "$1" in
 is-enabled) test -f /tmp/enabled && echo enabled || { echo disabled; exit 1; } ;;
 is-active) test -f /tmp/active ;;
 restart) if test -f /tmp/fail; then rm /tmp/fail; exit 19; fi; touch /tmp/active ;;
 start) touch /tmp/active ;;
 stop) rm -f /tmp/active ;;
 enable) touch /tmp/enabled; if [[ -e /tmp/fail-enable ]]; then exit 23; fi ;;
 disable) rm -f /tmp/enabled ;;
 show) if [[ $4 == InvocationID ]]; then echo fixture; else cat /tmp/mainpid; fi ;;
 daemon-reload) ;;
 *) exit 20 ;;
esac
STUB
chmod +x /usr/bin/systemctl
mkdir -p /run/sunna-host
sleep 120 &
child=$!
trap 'kill "$child" 2>/dev/null || true' EXIT
echo $$ >/tmp/mainpid
echo "fixture $child frame" >/run/sunna-host/receipt
echo bound >/run/sunna-host/listening
for prior in absent enable-failure disabled enabled; do
  if [[ $prior == disabled || $prior == enabled ]]; then
    mkdir -p -m700 /etc/sunna
    echo old >/etc/sunna/host.env
    chmod 600 /etc/sunna/host.env
    echo 'Description=Sunna whole-machine host' >"$unit"
    cp "$unit" /tmp/original
  fi
  if [[ $prior == enabled ]]; then touch /tmp/enabled /tmp/active; fi
  expected=19
  if [[ $prior == enable-failure ]]; then
    touch /tmp/fail-enable
    expected=23
  else
    rm -f /tmp/fail-enable
    touch /tmp/fail
  fi
  rc=0
  printf '\nlan\n48800\n\n' | bash /src/scripts/system-host.sh setup || rc=$?
  [[ $rc == "$expected" && ! -e /etc/sunna/host.key ]]
  if [[ $prior == absent || $prior == enable-failure ]]; then
    [[ ! -e "$unit" && ! -e /etc/sunna ]]
  else
    cmp "$unit" /tmp/original
    [[ $(cat /etc/sunna/host.env) == old ]]
  fi
  if [[ $prior == enabled ]]; then
    [[ -e /tmp/enabled && -e /tmp/active ]]
  else
    [[ ! -e /tmp/enabled && ! -e /tmp/active ]]
  fi
done

# Exercise the system and per-user dispatchers against the same service model.
rm -f "$unit" /tmp/active
cat >/usr/bin/systemctl <<'STUB'
#!/bin/bash
[[ $1 != --user ]] || shift
case $1 in
 restart) touch /tmp/active ;;
 try-restart) test ! -e /tmp/reload-error ;;
 *) exit 2 ;;
esac
STUB
install -d /tmp/bin
cp /src/scripts/sunna-host /tmp/bin/
for scope in system user; do
    if [[ $scope == system ]]; then
        daemon=/opt/sunna/host/current/bin/sunnad
        command=(bash /src/scripts/system-host.sh)
    else
        daemon=/tmp/bin/sunnad
        command=(bash /tmp/bin/sunna-host)
    fi
    rm -f "$daemon"
    printf '#!/bin/sh\ntouch /tmp/rotated\n' >"$daemon"
    chmod +x "$daemon"
    rm -f /tmp/rotated /tmp/reload-error
    "${command[@]}" new-identity
    [[ -f /tmp/rotated && ! -f /tmp/active ]]
    touch /tmp/reload-error
    if "${command[@]}" new-identity 2>/tmp/diagnostic; then exit 1; fi
    grep -qi 'identity rotated' /tmp/diagnostic
done
echo 'Migration rollback and stopped-host rotation passed.'
