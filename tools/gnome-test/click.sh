#!/bin/bash
# click.sh X Y: a click at screen pixel (X, Y) through a virtual pointer (test harness, unsafe mode).
# Every event is stamped with the current time: one stamped ahead makes mutter
# refuse focus changes that come before it (and assert when a window closes).
. /tmp/env.sh
device='const C = imports.gi.Clutter, G = imports.gi.GLib; global._sunnaPtr = global._sunnaPtr || (global.stage.context ? global.stage.context.get_backend() : C.get_default_backend()).get_default_seat().create_virtual_device(C.InputDeviceType.POINTER_DEVICE); const p = global._sunnaPtr;'
call() { gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval "$device $1 'ok'" >/dev/null; }
call "p.notify_absolute_motion(G.get_monotonic_time(), $1, $2);"
sleep 0.05
call "p.notify_button(G.get_monotonic_time(), C.BUTTON_PRIMARY, C.ButtonState.PRESSED);"
sleep 0.08
call "p.notify_button(G.get_monotonic_time(), C.BUTTON_PRIMARY, C.ButtonState.RELEASED);"
echo "clicked $1,$2"
