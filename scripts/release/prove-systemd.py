"""Run as fixture in the private proof container, with a real systemd user manager."""
import json
import os
from pathlib import Path
import signal
import subprocess
import time
from switch_states import STEPS, materialize

if not Path("/.dockerenv").exists() or os.geteuid() == 0 or os.environ.get("SUNNA_FIX_CONTAINER") != "1":
    raise SystemExit("run only as the disposable container's fixture user")
releases = Path("/proof/releases")
root = Path.home() / "managed/host"
root.mkdir(parents=True, exist_ok=True)
versions = root / "versions"
A, B = "0.1.0", "0.2.0"
unit = "sunna-fix-proof.service"
address = "127.0.0.1:49104"
for name, version in [("A", A), ("B", B)]:
    dest = versions / version
    dest.mkdir(parents=True, exist_ok=True)
    subprocess.run(["tar", "-xzf", releases/name/"sunna-host-linux-x86_64.tar.gz", "-C", dest], check=True)
helper = releases/"A/sunna-update-linux-x86_64"
units = Path.home()/".config/systemd/user"
units.mkdir(parents=True, exist_ok=True)
(units/unit).write_text(f"""[Unit]
StartLimitIntervalSec=0
[Service]
Environment=SUNNA_UPDATE_ROOT={root.parent}
Environment=SUNNA_LOG_URL=
Environment=SUNNA_LOG_TOKEN=
Environment=SUNNA_UPDATE=off
ExecStart={helper} supervise host --source synthetic --codec raw --no-audio --no-clipboard --listen {address}
Restart=always
RestartSec=0.2
TimeoutStopSec=2
""")


def ctl(*args):
    return subprocess.check_output(["systemctl", "--user", *args], text=True).strip()


def wait_host(ver):
    end = time.monotonic() + 35
    while time.monotonic() < end:
        pid = int(ctl("show", unit, "-p", "MainPID", "--value"))
        children = Path(f"/proc/{pid}/task/{pid}/children")
        if children.exists():
            for child in children.read_text().split():
                exe = Path(f"/proc/{child}/exe")
                if exe.exists() and exe.resolve() == versions/ver/"bin/sunnad":
                    probe = subprocess.run([releases/"A/.proof-client", "probe", address], capture_output=True)
                    if probe.returncode == 0 and json.loads((root/"state.json").read_text())["healthy"]:
                        return pid, int(child)
        time.sleep(.1)
    raise RuntimeError(f"expected a reachable {ver}")


ctl("daemon-reload")
try:
    for victim in ["child", "supervisor"]:
        for step in STEPS:
            materialize(root,A,B,STEPS[0])
            ctl("start", unit)
            pid, child = wait_host(A)
            os.kill(pid, signal.SIGSTOP)
            os.kill(child, signal.SIGSTOP)
            expected = materialize(root,A,B,step)
            if victim == "child":
                os.kill(child, signal.SIGKILL)
                os.kill(pid, signal.SIGCONT)
            else:
                os.kill(pid, signal.SIGKILL)
                os.kill(child, signal.SIGCONT)
            new_pid, _ = wait_host(expected)
            assert new_pid != pid
            ctl("stop", unit)
            print(f"PASS systemd killed {victim} at boundary {step}", flush=True)
finally:
    ctl("stop", unit)
