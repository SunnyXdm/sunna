"""Exercise the helper actually shipped, and match every embedded launcher copy."""
import argparse
import hashlib
from pathlib import Path
import subprocess
import tarfile
import tempfile
import zipfile

p = argparse.ArgumentParser()
p.add_argument("directory", type=Path)
p.add_argument("target")
args = p.parse_args()
root = args.directory.resolve()
helper = root / ("sunna-update-" + args.target)
components = ["app"] if args.target.startswith("macos-") else ["host", "app"]
expected = hashlib.sha256(helper.read_bytes()).digest()
for component in components:
    if component == "app" and args.target.startswith("macos-"):
        with zipfile.ZipFile(root / ("Sunna-" + args.target + ".zip")) as archive:
            for path in ["bin/sunna-update", "Sunna.app/Contents/MacOS/Sunna"]:
                if hashlib.sha256(archive.read(path)).digest() != expected:
                    raise ValueError("unexpected bundled macOS launcher")
    else:
        with tarfile.open(root / f"sunna-{component}-{args.target}.tar.gz") as archive:
            members = [m for m in archive if m.name.removeprefix("./") == "bin/sunna-update"]
            if len(members) != 1 or not members[0].isfile() or hashlib.sha256(archive.extractfile(members[0]).read()).digest() != expected:
                raise ValueError("unexpected packaged helper")
with tempfile.TemporaryDirectory() as tmp:
    work = Path(tmp)
    body = work / "manifest.json"
    body.write_bytes(b'{"version":"1.0.0","date":"","notes":"","minimum_protocol":8,"files":[]}')
    key = work / "test.key"
    subprocess.run(["openssl", "genpkey", "-algorithm", "ED25519", "-out", key], check=True)
    sig = work / "manifest.sig"
    subprocess.run(["openssl", "pkeyutl", "-sign", "-rawin", "-inkey", key, "-in", body, "-out", sig], check=True)
    programs = [helper]
    for component in components:
        dest = work / component
        dest.mkdir()
        if component == "app" and args.target.startswith("macos-"):
            subprocess.run(["unzip", "-q", root / f"Sunna-{args.target}.zip", "-d", dest], check=True)
            programs.append(dest / "Sunna.app/Contents/MacOS/Sunna-real")
        else:
            subprocess.run(["tar", "-xzf", root / f"sunna-{component}-{args.target}.tar.gz", "-C", dest], check=True)
            programs.append(dest / "bin" / ("sunnad" if component == "host" else "sunna-real"))
    for binary in programs:
        result = subprocess.run([binary, "--verify-release-fixture", "crates/update/release.pub", body, sig],
                                capture_output=True, timeout=20)
        if result.returncode == 0 or b"release signature is invalid" not in result.stderr:
            raise ValueError(f"{binary.name} did not reject the fixture with the expected verifier key")
