#!/usr/bin/env python3
"""Write app/src/main/assets/licenses.txt: the open source software in the
Android app's native library, each with its own license files, for Settings ->
Open Source Software. Run it again when the Rust dependencies change:

    python3 apps/android/licenses.py
"""

import json
import pathlib
import subprocess

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parent.parent
OUT = HERE / "app/src/main/assets/licenses.txt"
TARGET = "aarch64-linux-android"

# Prose without hard line breaks: the phone wraps it.
CREDITS = """\
Drawings
--------
The penguin is drawn after Tux by Larry Ewing (lewing@isc.tamu.edu), who drew him with The GIMP.

The interface icons are drawn after Feather (https://feathericons.com), Copyright (c) 2013-2017 Cole Bemis, under the MIT License.
"""


def packages():
    """(name, version) of every crate compiled into the library."""
    tree = subprocess.run(
        ["cargo", "tree", "-p", "sunna-android", "--target", TARGET, "-e", "normal",
         "--prefix", "none", "--format", "{p}"],
        cwd=REPO, check=True, capture_output=True, text=True).stdout
    found = set()
    for line in tree.splitlines():
        parts = line.split()
        if len(parts) >= 2 and not parts[0].startswith("sunna"):
            found.add((parts[0], parts[1].lstrip("v")))
    return sorted(found)


def main():
    metadata = json.loads(subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--filter-platform", TARGET],
        cwd=REPO, check=True, capture_output=True, text=True).stdout)
    by_id = {(p["name"], p["version"]): p for p in metadata["packages"]}
    sections = []
    listing = []
    for name, version in packages():
        package = by_id.get((name, version))
        if not package:
            continue
        license = package.get("license") or "see below"
        listing.append(f"  {name} {version}  ({license})")
        folder = pathlib.Path(package["manifest_path"]).parent
        texts = []
        for path in sorted(folder.iterdir()):
            if path.is_file() and path.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")):
                texts.append(path.read_text(errors="replace").strip())
        body = "\n\n".join(texts) if texts else f"Licensed {license}."
        sections.append(f"{name} {version}\n{'-' * len(name + version + ' ')}\n{body}\n")
    # The standard texts, for crates that name a license without shipping it.
    standard = []
    anyhow = by_id.get(next((key for key in by_id if key[0] == "anyhow"), None))
    if anyhow:
        folder = pathlib.Path(anyhow["manifest_path"]).parent
        for name, title in (("LICENSE-APACHE", "Apache License 2.0"), ("LICENSE-MIT", "MIT License")):
            path = folder / name
            if path.exists():
                standard.append(f"{title}\n{'-' * len(title)}\n{path.read_text().strip()}\n")
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(
        "Sunna includes the open source software below; thank you to everyone who made it.\n\n" + CREDITS + "\nRust crates\n-----------\n" + "\n".join(listing) +
        "\n\n\nLicenses\n========\n\n" + "\n\n".join(sections) +
        "\n\nThe standard texts, for the crates above that name a license without including it\n" +
        "=" * 70 + "\n\n" + "\n\n".join(standard))
    print(f"{OUT.relative_to(REPO)}: {len(listing)} crates, {OUT.stat().st_size // 1024} KB")


if __name__ == "__main__":
    main()
