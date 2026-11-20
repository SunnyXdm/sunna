"""Root recovery entry point under a disposable container's system manager."""
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import time
from switch_states import STEPS, materialize

assert Path("/.dockerenv").exists() and os.geteuid() == 0
assert os.environ["SUNNA_FIX_CONTAINER"] == "1" and os.listdir("/sys/class/net") == ["lo"]
root = Path("/opt/sunna/host")
assert not root.exists(), "requires a fresh system installation"
source = Path("/proof/releases")
unit = "sunna-fix2-recovery.service"
address = "127.0.0.1:49118"
for label, version in [("A", "0.1.0"), ("B", "0.2.0")]:
    dest = root / "versions" / version
    dest.mkdir(parents=True)
    subprocess.run(["tar", "--no-same-owner", "-xzf", source / label / "sunna-host-linux-x86_64.tar.gz", "-C", dest], check=True)
shutil.copy2(root / "versions/0.1.0/bin/sunna-update", root / "recovery")
unitfile = Path("/etc/systemd/system") / unit
unitfile.write_text(f"""[Unit]
StartLimitIntervalSec=0
[Service]
Environment=SUNNA_UPDATE_ROOT=/opt/sunna SUNNA_LISTEN_CHOICE={address} SUNNA_PORT=49118 SUNNA_UPDATE=off
ExecStart=/opt/sunna/host/recovery system-serve host --source synthetic --codec raw --no-audio --no-clipboard
Restart=always
RestartSec=0.2
""")

def ctl(*args):
    return subprocess.check_output(["systemctl", *args], text=True).strip()

def serving(expected):
    deadline = time.monotonic() + 40
    while time.monotonic() < deadline:
        pid = int(ctl("show", unit, "-p", "MainPID", "--value"))
        children = Path(f"/proc/{pid}/task/{pid}/children")
        try:
            for child in children.read_text().split():
                if Path(f"/proc/{child}/exe").resolve() == root / "versions" / expected / "bin/sunnad":
                    probe = subprocess.run([source / "A/.proof-client", "probe", address], capture_output=True)
                    if probe.returncode == 0 and json.loads((root / "state.json").read_text())["healthy"]:
                        return pid, int(child)
        except FileNotFoundError:
            pass
        time.sleep(.15)
    raise RuntimeError(f"system manager did not recover {expected}")

ctl("daemon-reload")
try:
    for victim in ["child", "supervisor"]:
        for step in STEPS:
            materialize(root, "0.1.0", "0.2.0", STEPS[0])
            ctl("start", unit)
            pid, child = serving("0.1.0")
            os.kill(pid, signal.SIGSTOP)
            os.kill(child, signal.SIGSTOP)
            expected = materialize(root, "0.1.0", "0.2.0", step)
            if victim == "child":
                os.kill(child, signal.SIGKILL)
                os.kill(pid, signal.SIGCONT)
            else:
                os.kill(pid, signal.SIGKILL)
                os.kill(child, signal.SIGCONT)
            new_pid, _ = serving(expected)
            assert pid != new_pid
            ctl("stop", unit)
            print(f"PASS {victim}: {step}", flush=True)
finally:
    ctl("stop", unit)
    unitfile.unlink()
    ctl("daemon-reload")
