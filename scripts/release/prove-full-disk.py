"""Run in the isolated container on a private tmpfs with nr_inodes=64."""
import json
import os
from pathlib import Path
import subprocess
import sys

base = Path(sys.argv[1])
assert Path("/.dockerenv").exists() and base.is_mount() and os.statvfs(base).f_files <= 128
root = base / "host"
for version in ["1.0.0", "1.1.0"]:
    bindir = root / "versions" / version / "bin"
    bindir.mkdir(parents=True)
    (bindir/"sunnad").write_text("#!/bin/sh\nexit 0\n")
    (bindir/"sunnad").chmod(0o700)
(root/"highest").write_text("1.1.0")
(root/"current").symlink_to("versions/1.1.0")
(root/"state.json").write_text(json.dumps(dict(current="1.1.0", previous="1.0.0", healthy=True)))
for lock in ["daemon.lock", "session.lock", "update.lock"]:
    (root/lock).touch()
fill = []
while os.statvfs(base).f_favail > 1:
    f = base / str(len(fill))
    f.touch()
    fill.append(f)
cmd = [sys.argv[2], "update", "host", "--rollback"]
env = dict(os.environ, SUNNA_UPDATE_ROOT=str(base))
r = subprocess.run(cmd, env=env, capture_output=True)
assert r.returncode and b"No space left" in r.stderr, r.stderr
assert (root/"current").resolve().name == "1.1.0"
assert json.loads((root/"state.json").read_text())["current"] == "1.1.0"
for f in fill:
    f.unlink()
subprocess.run(cmd, env=env, check=True, stdout=subprocess.DEVNULL)
assert (root/"current").resolve().name == "1.0.0"
print("PASS kernel ENOSPC between journal and state save, then successful retry")
