#!/bin/bash
# key.sh KEYNAME: press a key (Clutter keyval name, e.g. Return, Tab, space) through a virtual keyboard.
. /tmp/env.sh
js="const C = imports.gi.Clutter, G = imports.gi.GLib; global._sunnaKb = global._sunnaKb || global.stage.context.get_backend().get_default_seat().create_virtual_device(C.InputDeviceType.KEYBOARD_DEVICE); const k = global._sunnaKb; let t = G.get_monotonic_time(); k.notify_keyval(t, C.KEY_$1, C.KeyState.PRESSED); k.notify_keyval(t + 30000, C.KEY_$1, C.KeyState.RELEASED); 'ok'"
gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval "$js"
