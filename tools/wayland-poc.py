#!/usr/bin/env python3
"""Proof of concept for Sunna on Wayland: capture the screen and send input
through the xdg-desktop-portal RemoteDesktop + ScreenCast portals.

Run it inside a Wayland session, e.g. scripts/wayland-desktop.sh run python3
tools/wayland-poc.py. It saves /tmp/wl/before.png, clicks the launcher at the
bottom left, types "konsole", and saves /tmp/wl/after.png.

What it established (KDE Plasma 6.7, xdg-desktop-portal 1.22, PipeWire 1.6):
- The host app identifies itself with org.freedesktop.host.portal.Registry,
  which needs a .desktop file for the id whose Exec program exists.
- KDE skips the approval dialog for apps pre-authorized in the permission
  store: table "kde-authorized", id "remote-desktop", app id -> ["yes"].
  Start then returns in ~10 ms, with a restore token for later sessions.
- The consuming PipeWire stream must say media.type=Video; otherwise
  WirePlumber won't link it to the screencast ("target not found").
- KWin offers DMA-BUF frames (DMA_DRM) first; asking for video/x-raw gets
  CPU-readable BGRA.
- Input: NotifyPointerMotionAbsolute (per stream, in stream pixels),
  NotifyPointerButton (evdev BTN_*), NotifyKeyboardKeycode (evdev codes).
"""
import os, sys, struct, zlib, time
import dbus, dbus.mainloop.glib
import gi
gi.require_version("Gst", "1.0")
from gi.repository import GLib, Gst

APP_ID = "dev.sunna.Host"
dbus.mainloop.glib.DBusGMainLoop(set_as_default=True)
bus = dbus.SessionBus()
portal = bus.get_object("org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop")
loop = GLib.MainLoop()
sender = bus.get_unique_name()[1:].replace(".", "_")
counter = [0]

def request(method, *args, options=None):
    """Call a portal method that answers through a Request object; wait for it."""
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
    GLib.timeout_add_seconds(30, loop.quit)
    loop.run()
    match.remove()
    if result.get("code") != 0:
        sys.exit(f"{method._method_name} failed: {result}")
    return result["results"]

t0 = time.time()
# 1. Tell the portal who we are (needs a .desktop file for the id).
dbus.Interface(portal, "org.freedesktop.host.portal.Registry").Register(APP_ID, {})
# 2. KDE: pre-authorize remote desktop for this app (no dialog on a headless box).
store = bus.get_object("org.freedesktop.impl.portal.PermissionStore", "/org/freedesktop/impl/portal/PermissionStore")
dbus.Interface(store, "org.freedesktop.impl.portal.PermissionStore").SetPermission(
    "kde-authorized", True, "remote-desktop", APP_ID, ["yes"])

rd = dbus.Interface(portal, "org.freedesktop.portal.RemoteDesktop")
sc = dbus.Interface(portal, "org.freedesktop.portal.ScreenCast")
print("RemoteDesktop version", portal.Get("org.freedesktop.portal.RemoteDesktop", "version", dbus_interface="org.freedesktop.DBus.Properties"),
      "ScreenCast version", portal.Get("org.freedesktop.portal.ScreenCast", "version", dbus_interface="org.freedesktop.DBus.Properties"))
session = request(rd.CreateSession, options={"session_handle_token": "sunnasession"})["session_handle"]
request(rd.SelectDevices, session, options={"types": dbus.UInt32(1 | 2), "persist_mode": dbus.UInt32(2)})
request(sc.SelectSources, session, options={"types": dbus.UInt32(1), "multiple": False, "cursor_mode": dbus.UInt32(2)})
started = request(rd.Start, session, "")
streams = started["streams"]
node, props = int(streams[0][0]), streams[0][1]
print(f"started in {time.time()-t0:.2f}s: devices={int(started['devices'])} node={node} size={tuple(int(v) for v in props.get('size', (0, 0)))} restore_token={'yes' if 'restore_token' in started else 'no'}")
fd = sc.OpenPipeWireRemote(session, {}).take()
print("stream props:", {str(k): (v if not isinstance(v, dbus.Struct) else tuple(v)) for k, v in props.items()}, flush=True)

# 3. Frames: PipeWire -> GStreamer (core elements only) -> one raw frame.
Gst.init(None)
import json, subprocess
serial = next(o["info"]["props"]["object.serial"] for o in json.loads(subprocess.run(["pw-dump"], capture_output=True, text=True).stdout) if o.get("id") == node)
print("node", node, "serial", serial, flush=True)
def grab(path):
    pipeline = Gst.parse_launch(f"pipewiresrc fd={os.dup(fd)} path={node} always-copy=true name=src ! video/x-raw ! fakesink name=sink signal-handoffs=true")
    pipeline.get_by_name("src").set_property("stream-properties", Gst.Structure.new_from_string("props,media.type=Video,media.category=Capture,media.role=Screen"))
    bus_ = pipeline.get_bus(); bus_.add_signal_watch()
    bus_.connect("message::error", lambda b, m: print("GST ERROR", m.parse_error()))
    bus_.connect("message::warning", lambda b, m: print("GST WARNING", m.parse_warning()))
    got = {}
    def handoff(sink, buf, pad):
        if "data" in got: return
        s = pad.get_current_caps().get_structure(0)
        ok, info = buf.map(Gst.MapFlags.READ)
        got.update(data=bytes(info.data), fmt=s.get_value("format"), w=s.get_value("width"), h=s.get_value("height"))
        buf.unmap(info)
        GLib.idle_add(loop.quit)
    pipeline.get_by_name("sink").connect("handoff", handoff)
    pipeline.set_state(Gst.State.PLAYING)
    GLib.timeout_add_seconds(10, loop.quit)
    t = time.time(); loop.run()
    pipeline.set_state(Gst.State.NULL)
    if "data" not in got: sys.exit("no frame")
    w, h, data = got["w"], got["h"], got["data"]
    stride = len(data) // h
    order = {"BGRx": (2, 1, 0), "BGRA": (2, 1, 0), "RGBx": (0, 1, 2), "RGBA": (0, 1, 2)}[got["fmt"]]
    rows = b"".join(b"\0" + bytes(c for i in range(w) for c in (data[y*stride+4*i+order[0]], data[y*stride+4*i+order[1]], data[y*stride+4*i+order[2]])) for y in range(h))
    chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)
    open(path, "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows, 6)) + chunk(b"IEND", b""))
    print(f"frame {w}x{h} {got['fmt']} in {time.time()-t:.2f}s -> {path}")
    return w, h

w, h = grab("/tmp/wl/before.png")
# 4. Input: move to the bottom-left launcher and click it, then type.
rd.NotifyPointerMotionAbsolute(session, {}, dbus.UInt32(node), 24.0, float(h - 22))
time.sleep(0.2)
BTN_LEFT = 0x110
rd.NotifyPointerButton(session, {}, BTN_LEFT, dbus.UInt32(1)); time.sleep(0.05)
rd.NotifyPointerButton(session, {}, BTN_LEFT, dbus.UInt32(0))
time.sleep(1.2)
# Type "konsole" (evdev keycodes: k o n s o l e) into the launcher's search.
for code in [37, 24, 49, 31, 24, 38, 18]:
    rd.NotifyKeyboardKeycode(session, {}, code, dbus.UInt32(1)); time.sleep(0.03)
    rd.NotifyKeyboardKeycode(session, {}, code, dbus.UInt32(0)); time.sleep(0.05)
time.sleep(1.2)
grab("/tmp/wl/after.png")
