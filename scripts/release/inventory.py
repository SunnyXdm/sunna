"""Names accepted from each build job, also used by the manifest generator."""
from pathlib import Path
import shutil
import sys


def files(target):
    if target == "android":
        return {"sunna-android.apk"}
    if target.startswith("macos"):
        return {f"Sunna-{target}.zip", f"sunna-update-{target}"}
    else:
        app = f"sunna-app-{target}.tar.gz"
    return {app, f"sunna-host-{target}.tar.gz", f"sunna-update-{target}"}


JOBS = {
    "linux-ubuntu-22.04": "linux-x86_64",
    "linux-ubuntu-24.04-arm": "linux-aarch64",
    "macos-macos-15": "macos-aarch64",
    "macos-macos-15-intel": "macos-x86_64",
    "android": "android",
}
ALLOWED = set.union(*(files(target) for target in JOBS.values()),
                    {"sunna-update-windows-x86_64.exe", "sunna-host-windows-x86_64.zip"})


def collect(source, dest):
    if set(p.name for p in source.iterdir()) != JOBS.keys():
        raise ValueError("incorrect set of build jobs")
    for job, target in JOBS.items():
        folder = source / job
        if folder.is_symlink() or set(p.name for p in folder.iterdir()) != files(target):
            raise ValueError(f"incorrect inventory for {job}")
        for artifact in folder.iterdir():
            if artifact.is_symlink() or not artifact.is_file() or not artifact.stat().st_size:
                raise ValueError(f"invalid artifact in {job}")
    dest.mkdir()
    for job in JOBS:
        for artifact in (source / job).iterdir():
            shutil.copyfile(artifact, dest / artifact.name)


if __name__ == "__main__":
    collect(Path(sys.argv[1]), Path(sys.argv[2]))
