"""Check the actual verifier-key symbol from both packaged ELF libraries."""
import pathlib
import subprocess
import sys
import tempfile
import zipfile


def embedded_key(so):
    symbols = subprocess.check_output(["readelf", "--dyn-syms", "--wide", so], text=True)
    matches = [line.split() for line in symbols.splitlines() if line.endswith(" SUNNA_RELEASE_VERIFY_KEY")]
    if len(matches) != 1 or matches[0][2:4] != ["32", "OBJECT"]:
        raise ValueError("missing verifier key symbol")
    address = int(matches[0][1], 16)
    segments = subprocess.check_output(["readelf", "--program-headers", "--wide", so], text=True)
    for line in segments.splitlines():
        fields = line.split()
        if fields and fields[0] == "LOAD":
            offset, vaddr, _, length = (int(v, 16) for v in fields[1:5])
            if vaddr <= address and address + 32 <= vaddr + length:
                with so.open("rb") as f:
                    f.seek(offset + address - vaddr)
                    return f.read(32)
    raise ValueError("verifier key outside loadable ELF segments")


with tempfile.TemporaryDirectory() as directory, zipfile.ZipFile(sys.argv[1]) as apk:
    expected = pathlib.Path("crates/update/release.pub").read_bytes()
    libs = [p for p in apk.namelist() if p.endswith("/libsunna_android.so")]
    if sorted(libs) != ["lib/arm64-v8a/libsunna_android.so", "lib/x86_64/libsunna_android.so"]:
        raise ValueError("unexpected Android native inventory")
    for name in libs:
        so = pathlib.Path(apk.extract(name, directory))
        if embedded_key(so) != expected:
            raise ValueError(f"wrong verifier key in {name}")
