"""Build an isolated machine list for the Linux launcher checks."""
import json
from pathlib import Path


def seed_machines(home: Path, host_key: str = "", host_address: str = "100.108.56.3:48850") -> None:
    folder = home / ".sunna"
    folder.mkdir(mode=0o700)
    machines = []
    for index, (name, kind) in enumerate([
        ("Test host", "desktop"), ("Studio", "desktop"), ("Laptop", "laptop"),
        ("Server", "server"), ("Virtual", "vm"), ("Mini", "mini"),
    ]):
        machines.append({
            "id": f"proof{index}", "name": name, "kind": kind, "os": "Linux",
            "address": host_address if index == 0 and host_key else f"127.0.0.1:{50000+index}",
            "key": host_key if index == 0 else "",
        })
    store = folder / "machines.json"
    store.touch(mode=0o600)
    store.write_text(json.dumps({"machines": machines}))
    (folder / "config.json").write_text(json.dumps({"windowed": True}))
