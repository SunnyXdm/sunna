#!/usr/bin/env python3
"""Make labeled contact sheets from saved Sunna client frames."""
from pathlib import Path
import subprocess

root = Path(__file__).parent / "evidence"
sequence = [
    ("machine-greeter", "Login screen"), ("transition", "During login"),
    ("same-viewer-desktop", "Desktop on original connection"), ("desktop", "Desktop"), ("locked", "Locked"), ("unlocked", "Unlocked"),
    ("second-user", "Second user"), ("switched", "After switching users"),
    ("logout", "After logout"), ("after-reboot", "Login screen after reboot"), ("restored", "After restoring packages"),
    ("updated", "Signed update"), ("rolled-back", "After rollback"),
]
for manager in root.iterdir():
    if not manager.is_dir():
        continue
    items = [(manager / (p+".png"), label) for p, label in sequence if (manager / (p+".png")).exists()]
    if not items:
        continue
    cols = min(3, len(items))
    args = ["montage", "-font", "DejaVu-Sans", "-pointsize", "16", "-fill", "white", "-background", "#202329"]
    for path, label in items:
        args.extend(["-label", label, str(path)])
    args.extend(["-geometry", "320x200+3+4", "-tile", f"{cols}x", "-quality", "88", str(manager/"contact-sheet.jpg")])
    subprocess.run(args, check=True)
