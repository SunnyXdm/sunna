#!/bin/bash
# key.sh KEYNAME: press a key (a Clutter keyval name: Return, Tab, space, Escape...) through a virtual keyboard.
. /tmp/env.sh
device='const C = imports.gi.Clutter, G = imports.gi.GLib; global._sunnaKb = global._sunnaKb || (global.stage.context ? global.stage.context.get_backend() : C.get_default_backend()).get_default_seat().create_virtual_device(C.InputDeviceType.KEYBOARD_DEVICE); const k = global._sunnaKb;'
call() { gdbus call --session --dest org.gnome.Shell --object-path /org/gnome/Shell --method org.gnome.Shell.Eval "$device $1 'ok'" >/dev/null; }
call "k.notify_keyval(G.get_monotonic_time(), C.KEY_$1, C.KeyState.PRESSED);"
sleep 0.05
call "k.notify_keyval(G.get_monotonic_time(), C.KEY_$1, C.KeyState.RELEASED);"
echo "pressed $1"
