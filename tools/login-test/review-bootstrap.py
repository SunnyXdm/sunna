"""Installer transaction fixture. Run as an ordinary user in a test container."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]

class Migration(unittest.TestCase):
    def test_failed_worker_restores_the_user_unit_and_its_config(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            bindir = home / "bin"
            bindir.mkdir()
            config = home / ".config/sunna/host.env"
            config.parent.mkdir(parents=True)
            config.write_text("SUNNA_MODE=desktop\nSUNNA_TOKEN=x\nSUNNA_LISTEN=lan\n")
            calls = home / "calls"
            # Model the authenticated install stage and a setup that fails
            # before it can issue a receipt. Record actions only, never stdin.
            for name, body in {
                "sudo": 'case "$*" in *system-host.sh*) cat >/dev/null; exit 1;; esac',
                "systemctl": 'echo "$*" >> "$CALLS"',
            }.items():
                path = bindir / name
                path.write_text("#!/bin/sh\n" + body + "\n")
                path.chmod(0o755)
            script = home / "install.sh"
            template = (ROOT / "scripts/release/install.sh.in").read_text()
            script.write_text(template.replace("@HASH_CASES@", "linux-*) hash=test;;"))
            result = subprocess.run(["sh", script, "host", "--system"],
                env=dict(PATH=f"{bindir}:/usr/bin:/bin", HOME=str(home), CALLS=str(calls)),
                capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(calls.read_text().splitlines()[-2:],
                ["--user stop sunna-host.service", "--user start sunna-host.service"])
            self.assertTrue(config.is_file())

if __name__ == "__main__":
    if os.geteuid() == 0 or not Path("/.dockerenv").exists():
        raise SystemExit("run as a fixture user in a disposable container")
    unittest.main()
