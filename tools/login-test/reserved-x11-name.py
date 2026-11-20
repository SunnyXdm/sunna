"""Exercise a genuine X server when its abstract name is unavailable."""
import os
from pathlib import Path
import select
import secrets
import socket
import subprocess
import sys
import tempfile
import time

assert os.geteuid() != 0 and Path("/.dockerenv").exists()
assert os.listdir("/sys/class/net") == ["lo"]
display = ":98"
path = "/tmp/.X11-unix/X98"
target = Path("/tmp/.X11-unix/X99")
assert not Path(path).exists() and not target.exists()
with socket.socket(socket.AF_UNIX) as reservation, tempfile.TemporaryDirectory() as tmp:
    # Leave the accept queue untouched. Even a connection with no data fails the test.
    reservation.bind("\0" + path)
    reservation.listen(1)
    authority = Path(tmp) / "Xauthority"
    authority.touch(mode=0o600)
    env = dict(os.environ, DISPLAY=display, XAUTHORITY=str(authority))
    cookie = secrets.token_hex(16)
    subprocess.run(["xauth", "-q"], env=env, text=True, check=True,
                   input=f"add {display} . {cookie}\nadd :99 . {cookie}\n")
    # Xvfb itself requires both names at startup. Use a filesystem alias
    # for its actual socket while keeping the tested display's name reserved.
    server = subprocess.Popen(["Xvfb", ":99", "-nolisten", "tcp",
                               "-auth", str(authority), "-screen", "0", "640x400x24"],
                              env=env, stderr=subprocess.DEVNULL)
    try:
        Path(path).symlink_to(target)
        for _ in range(40):
            if Path(path).is_socket():
                ready = subprocess.run(["xdpyinfo"], env=dict(env, DISPLAY=":99"), capture_output=True)
                if ready.returncode == 0:
                    break
            assert server.poll() is None, "Xvfb failed to start its filesystem listener"
            time.sleep(.1)
        else:
            raise RuntimeError("Xvfb did not become ready")
        # The normal Sunna input path authenticates to Xvfb and verifies the
        # result with a round trip on the real display.
        subprocess.run([sys.argv[1], "--ignored", "--exact", "linux::tests::injects_into_x"],
                       env=env, check=True, timeout=10)
        assert not select.select([reservation], [], [], 0)[0], "unexpected abstract connection"
        print("Verified X11 input with real Xauthority and an unavailable abstract name.")
    finally:
        Path(path).unlink(missing_ok=True)
        server.terminate()
        server.wait(timeout=5)
