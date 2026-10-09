#!/usr/bin/env python3
"""GNOME counterpart of tools/wayland-poc.py: start a RemoteDesktop +
ScreenCast portal session that asks to be remembered, keep the restore token
(~/restore-token), and optionally grab a frame and send input.

Run it in the test bed (up.sh), inside the session:
  docker exec -d sunna-gnome bash -c '. /tmp/env.sh; HOLD=30 python3 /test/gnome-poc.py --frame --keys'
The first run shows GNOME's dialog (approve it with click.sh: switch on Allow
Remote Interaction, then Share); later runs restore without it.

What it established on GNOME 51 (research/10-gnome-wayland.md):
- The dialog: Remember This Selection (on), Allow Remote Interaction (off by
  default: without it, no input devices), the screens, Share.
- With the token, Start returns in under 0.1 s with keyboard and pointer and a
  new token (tokens are single use).
- Frames: BGRx through PipeWire (media.type=Video needed), the first in 30 ms.
- Input: NotifyPointerMotionAbsolute (stream pixels), NotifyPointerButton
  (evdev BTN_*), NotifyKeyboardKeycode (evdev codes).
Options: --frame (one frame to /tmp/frame.raw), --keys (Super, then "files"),
--input (click the top middle, type "settings"); HOLD=seconds before closing.
"""
import os, sys, time, json, subprocess
import dbus, dbus.mainloop.glib
from gi.repository import GLib

APP_ID = "dev.sunna.Host"
TOKEN_FILE = os.path.expanduser("~/restore-token")
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
portal = bus.get_object("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop")
loop = GLib.MainLoop()
sender = bus.get_unique_name()[1:].replace(".", "_")
counter = [0]

def request(method, *args, options=None, timeout=120):
    counter[0] += 1
    token = f"sunna{counter[0]}"
    options = dict(options or {})
    options["handle_token"] = token
    path = f"/org/freedesktop/portal/desktop/request/{sender}/{token}"
    result = {}
    def on_response(code, results):
        result["code"], result["results"] = int(code), results
        loop.quit()
    match = bus.add_signal_receiver(on_response, "Response", "org.freedesktop.portal.Request", path=path)
    method(*args, options)
    GLib.timeout_add_seconds(timeout, loop.quit)
    loop.run()
    match.remove()
    if result.get("code") != 0:
        print(f"{method._method_name} -> {result.get('code', 'timeout')}", flush=True)
        sys.exit(1)
    return result["results"]

def prop(iface, name):
    return portal.Get(iface, name, dbus_interface="org.freedesktop.DBus.Properties")

t0 = time.time()
try:
    dbus.Interface(portal, "org.freedesktop.host.portal.Registry").Register(APP_ID, {})
    print("registered as", APP_ID, flush=True)
except dbus.DBusException as error:
    print("Registry:", error.get_dbus_message(), flush=True)
rd = dbus.Interface(portal, "org.freedesktop.portal.RemoteDesktop")
sc = dbus.Interface(portal, "org.freedesktop.portal.ScreenCast")
print("RemoteDesktop v%d devices=%d, ScreenCast v%d sources=%d cursor_modes=%d" % (
    prop("org.freedesktop.portal.RemoteDesktop", "version"), prop("org.freedesktop.portal.RemoteDesktop", "AvailableDeviceTypes"),
    prop("org.freedesktop.portal.ScreenCast", "version"), prop("org.freedesktop.portal.ScreenCast", "AvailableSourceTypes"),
    prop("org.freedesktop.portal.ScreenCast", "AvailableCursorModes")), flush=True)
session = request(rd.CreateSession, options={"session_handle_token": "sunnasession"})["session_handle"]
devices = {"types": dbus.UInt32(1 | 2), "persist_mode": dbus.UInt32(2)}
if os.path.exists(TOKEN_FILE):
    devices["restore_token"] = open(TOKEN_FILE).read().strip()
    print("restoring with the saved token", flush=True)
request(rd.SelectDevices, session, options=devices)
request(sc.SelectSources, session, options={"types": dbus.UInt32(1), "multiple": False, "cursor_mode": dbus.UInt32(4)})
print("starting: waiting for approval...", flush=True)
started = request(rd.Start, session, "")
streams = started["streams"]
node, props = int(streams[0][0]), streams[0][1]
token = started.get("restore_token")
print(f"started after {time.time()-t0:.1f}s: devices={int(started['devices'])} node={node} props={ {str(k): str(v) for k, v in props.items()} } restore_token={'yes' if token else 'no'} persist_mode={int(started.get('persist_mode', -1))}", flush=True)
if token:
    open(TOKEN_FILE, "w").write(str(token))
if "--frame" in sys.argv:
    import gi
    gi.require_version("Gst", "1.0")
    from gi.repository import Gst
    Gst.init(None)
    fd = sc.OpenPipeWireRemote(session, {}).take()
    pipeline = Gst.parse_launch(f"pipewiresrc fd={fd} path={node} always-copy=true name=src ! video/x-raw ! fakesink name=sink signal-handoffs=true")
    pipeline.get_by_name("src").set_property("stream-properties", Gst.Structure.new_from_string("props,media.type=Video,media.category=Capture,media.role=Screen"))
    got = {}
    def handoff(sink, buf, pad):
        if "data" in got: return
        s = pad.get_current_caps().get_structure(0)
        ok, info = buf.map(Gst.MapFlags.READ)
        got.update(data=bytes(info.data), caps=pad.get_current_caps().to_string())
        buf.unmap(info)
        GLib.idle_add(loop.quit)
    pipeline.get_by_name("sink").connect("handoff", handoff)
    t = time.time()
    pipeline.set_state(Gst.State.PLAYING)
    GLib.timeout_add_seconds(10, loop.quit)
    loop.run()
    pipeline.set_state(Gst.State.NULL)
    print(f"frame after {time.time()-t:.2f}s: {len(got.get('data', b''))} bytes, caps {got.get('caps', 'none')[:200]}", flush=True)
    if "data" in got:
        open("/tmp/frame.raw", "wb").write(got["data"])
if "--keys" in sys.argv:
    for code in [125]:  # Super: the overview
        rd.NotifyKeyboardKeycode(session, {}, code, dbus.UInt32(1)); time.sleep(0.05)
        rd.NotifyKeyboardKeycode(session, {}, code, dbus.UInt32(0))
    time.sleep(1.0)
    for code in [33, 23, 38, 18, 31]:  # f i l e s
        rd.NotifyKeyboardKeycode(session, {}, code, dbus.UInt32(1)); time.sleep(0.03)
        rd.NotifyKeyboardKeycode(session, {}, code, dbus.UInt32(0)); time.sleep(0.05)
    print("keys sent", flush=True)
if "--input" in sys.argv:
    w, h = (int(v) for v in props.get("size", (1920, 1080)))
    rd.NotifyPointerMotionAbsolute(session, {}, dbus.UInt32(node), float(w // 2), 62.0)
    time.sleep(0.2)
    rd.NotifyPointerButton(session, {}, 0x110, dbus.UInt32(1)); time.sleep(0.05)
    rd.NotifyPointerButton(session, {}, 0x110, dbus.UInt32(0)); time.sleep(0.5)
    for code in [31, 18, 20, 20, 23, 49, 34, 31]:  # s e t t i n g s
        rd.NotifyKeyboardKeycode(session, {}, code, dbus.UInt32(1)); time.sleep(0.03)
        rd.NotifyKeyboardKeycode(session, {}, code, dbus.UInt32(0)); time.sleep(0.05)
    print("input sent", flush=True)
time.sleep(float(os.environ.get("HOLD", "2")))
dbus.Interface(bus.get_object("org.freedesktop.portal.Desktop", session), "org.freedesktop.portal.Session").Close()
print("closed", flush=True)
