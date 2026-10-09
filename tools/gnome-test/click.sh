#!/bin/bash
# click.sh X Y: a click at screen pixel (X, Y) through a virtual pointer (test harness, unsafe mode).
. /tmp/env.sh
js="const C = imports.gi.Clutter, G = imports.gi.GLib; global._sunnaPtr = global._sunnaPtr || global.stage.context.get_backend().get_default_seat().create_virtual_device(C.InputDeviceType.POINTER_DEVICE); const p = global._sunnaPtr; let t = G.get_monotonic_time(); p.notify_absolute_motion(t, $1, $2); p.notify_button(t + 20000, C.BUTTON_PRIMARY, C.ButtonState.PRESSED); p.notify_button(t + 60000, C.BUTTON_PRIMARY, C.ButtonState.RELEASED); 'ok'"
gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval "$js"
