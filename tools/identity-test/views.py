"""Capture five real launcher layouts against private synthetic hosts on an owned X display."""
import argparse
import contextlib
import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile
import time

p = argparse.ArgumentParser()
p.add_argument("old", type=Path)
p.add_argument("new", type=Path)
p.add_argument("out", type=Path)
args = p.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(prefix="sunna-id-views-") as tmp, contextlib.ExitStack() as processes:
    root = Path(tmp)
    def spawn(cmd, env):
        proc = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        def stop():
            if proc.poll() is None:
                proc.terminate()
                proc.wait(timeout=15)
        processes.callback(stop)
        return proc

    env = dict(os.environ, HOME=tmp, XDG_CONFIG_HOME=str(root/"config"), XDG_DATA_HOME=str(root/"data"),
               GDK_BACKEND="x11", WEBKIT_DISABLE_DMABUF_RENDERER="1", LIBGL_ALWAYS_SOFTWARE="1",
               SUNNA_LOG_URL="", SUNNA_LOG_TOKEN="", SUNNA_TOKEN=secrets.token_hex(16))
    def host(path):
        with socket.socket(type=socket.SOCK_DGRAM) as s:
            s.bind(("127.0.0.1", 0))
            addr = f"127.0.0.1:{s.getsockname()[1]}"
        spawn([path, "--listen", addr, "--source", "synthetic", "--codec", "raw", "--no-audio", "--no-clipboard"], env)
        return addr
    a, b = host(args.old), host(args.new/"sunnad")
    folder = root/".sunna"
    folder.mkdir(mode=0o700, exist_ok=True)
    machines = [
        dict(id="01", name="Office", address=b, identity="A"*43),
        dict(id="02", name="Studio", address=a, allow_older=True),
        dict(id="03", name="Laptop", address=a),
    ]
    for m in machines:
        m.update(key=env["SUNNA_TOKEN"], os="Linux", kind="desktop")
    (folder/"identities").mkdir()
    (folder/"identities"/"02").write_text("allow-older")
    (folder/"machines.json").write_text(json.dumps(dict(machines=machines)))
    (folder/"config.json").write_text(json.dumps(dict(view="devices", windowed=True)))
    spawn(["openbox"], env)
    spawn([args.new/"sunna"], env)
    def xd(*argv):
        return subprocess.run(["xdotool", *argv], env=env, capture_output=True, text=True)
    for _ in range(100):
        wins = xd("search", "--onlyvisible", "--name", "^Sunna$").stdout.splitlines()
        if wins: break
        time.sleep(.25)
    assert wins
    window = wins[0]
    xd("windowfocus", window)
    time.sleep(6)
    for i, name in enumerate(["devices", "wall", "space", "type", "gallery"], 1):
        xd("key", f"ctrl+{i}")
        time.sleep(1.5)
        assert json.loads((folder/"config.json").read_text()).get("view") == name
        subprocess.run(["import", "-window", window, args.out/f"identity-{name}.png"], env=env, check=True)
        if name in ("devices", "space"):
            xd("key", "Left", "Left")
            time.sleep(.7)
            subprocess.run(["import", "-window", window, args.out/f"identity-{name}-changed.png"], env=env, check=True)
            for kind in ("pin", "legacy"):
                if kind == "legacy":
                    xd("key", "Right")
                xd("key", "ctrl+e")
                time.sleep(1)
                subprocess.run(["import", "-window", window, args.out/f"identity-{name}-edit-{kind}.png"], env=env, check=True)
                xd("key", "Escape")
                time.sleep(.4)
