"""Decode the Linux host command's QR. Requires Pillow and zxing-cpp in the fixture."""
import os
from pathlib import Path
import secrets
import subprocess
import tempfile
from urllib.parse import parse_qs, urlsplit

from PIL import Image
import zxingcpp

repo = Path(__file__).resolve().parents[2]
with tempfile.TemporaryDirectory(prefix="sunna-id-qr-") as tmp:
    home = Path(tmp)
    folder = home/"config"/"sunna"
    folder.mkdir(parents=True, mode=0o700)
    key = secrets.token_hex(16)
    (folder/"host.env").write_text(f"SUNNA_TOKEN={key}\nSUNNA_LISTEN=127.0.0.1:48802\n")
    env = dict(os.environ, HOME=tmp, XDG_CONFIG_HOME=str(home/"config"),
               PATH=str(repo/"target/debug")+os.pathsep+os.environ["PATH"])
    out = subprocess.check_output([repo/"scripts/sunna-host", "link"], env=env, text=True)
    link = next(line.strip() for line in out.splitlines() if line.strip().startswith("sunna://"))
    pin = subprocess.check_output([repo/"target/debug/sunnad", "identity"], env=env, text=True).strip()
    assert parse_qs(urlsplit(link).query) == {"id": [pin], "key": [key]}
    rows = [line for line in out.splitlines() if line and set(line) <= set(" ▀▄█")]
    size = max(map(len, rows))
    image = Image.new("L", (size, len(rows)*2), "white")
    for y, row in enumerate(rows):
        for x, block in enumerate(row):
            if block in "▀█":
                image.putpixel((x, y*2), 0)
            if block in "▄█":
                image.putpixel((x, y*2+1), 0)
    decoded = zxingcpp.read_barcode(image.resize((size*6, image.height*6), Image.Resampling.NEAREST))
    assert decoded is not None and decoded.text == link
    print("The QR decoded to the same address, sharing key and public identity as the printed link.")
