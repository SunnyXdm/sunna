"""Draw a fixture marker as the logging-in user at Xsession startup."""
from ctypes import CDLL, c_void_p, c_ulong, c_int, c_uint, c_char_p
from pathlib import Path
import os
import time

x = CDLL("libX11.so.6")
def bind(name, result, *args):
    f = getattr(x, name)
    f.restype, f.argtypes = result, args
    return f

display = bind("XOpenDisplay", c_void_p, c_char_p)(None)
if not display:
    raise SystemExit("cannot draw")
root = bind("XDefaultRootWindow", c_ulong, c_void_p)(display)
width = bind("XDisplayWidth", c_int, c_void_p, c_int)(display, 0)
height = bind("XDisplayHeight", c_int, c_void_p, c_int)(display, 0)
create = bind("XCreateSimpleWindow", c_ulong, c_void_p, c_ulong, c_int, c_int,
              c_uint, c_uint, c_uint, c_ulong, c_ulong)
show = bind("XMapRaised", c_int, c_void_p, c_ulong)
sync = bind("XSync", c_int, c_void_p, c_int)
# Alternating red and cyan bars are identifiable in the decoded YUV planes.
windows = [create(display, root, width//4+i*width//8, height//3, width//8,
                  height//4, 0, 0, color) for i, color in
           enumerate([0xff0000, 0x00ffff, 0xff0000, 0x00ffff])]
for w in windows:
    show(display, w)
sync(display, 0)
Path(os.environ["XDG_RUNTIME_DIR"], "sunna-first-draw").write_text(
    Path("/proc/uptime").read_text().split()[0] + "\n")
for _ in range(200):
    for w in windows:
        show(display, w)
    sync(display, 0)
    time.sleep(.1)
