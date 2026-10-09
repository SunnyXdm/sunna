#!/bin/bash
# Inside the container, as root: a system bus with a mock logind that has a
# session for tester (what GNOME Shell's own tests do).
set -e
mkdir -p /run/dbus
dbus-daemon --system --fork
python3 -m dbusmock --system --template logind > /tmp/logind-mock.log 2>&1 &
for i in $(seq 40); do gdbus introspect --system --dest org.freedesktop.login1 --object-path /org/freedesktop/login1 >/dev/null 2>&1 && break; sleep 0.25; done
gdbus call --system --dest org.freedesktop.login1 --object-path /org/freedesktop/login1 \
  --method org.freedesktop.DBus.Mock.AddSession c1 seat0 1000 tester true >/dev/null
echo "mock logind with session c1"
