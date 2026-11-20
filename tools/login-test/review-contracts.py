"""Guard entry points that must all use the same Linux security boundary."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]

def source(path):
    return (ROOT / path).read_text()

class Contracts(unittest.TestCase):
    def test_x11_entry_points(self):
        for path in ["apps/sunnad/src/linux_desktop.rs", "crates/capture/src/linux.rs",
                     "crates/input/src/linux.rs", "crates/clipboard/src/linux.rs",
                     "crates/capture/src/cursor.rs"]:
            with self.subTest(path=path):
                self.assertNotIn("x11rb::connect(", source(path).split("#[cfg(test)]")[0])

    def test_launcher_health(self):
        launcher = source("scripts/linux-desktop.sh")
        self.assertFalse("xdpyinfo" in launcher)
        self.assertGreaterEqual(launcher.count("display_health"), 3)

    def test_resize_unwinds_worker(self):
        self.assertFalse("process::exit" in source("apps/sunnad/src/linux_worker.rs"))

    def test_rotation_preserves_stopped_state(self):
        for script in ["scripts/system-host.sh", "scripts/sunna-host"]:
            branch = source(script).split("  new-identity)")[-1].split(";;")[0]
            self.assertTrue("try-restart" in branch, script)

    def test_proc_identity(self):
        self.assertNotIn('File::open(p.join("environ"))', source("apps/sunnad/src/linux_worker.rs"))

    def test_boot_entry(self):
        unit = source("scripts/system-host.sh").split("ExecStart=")[1].splitlines()[0]
        self.assertNotIn("/current/", unit)

    def test_migration_receipt(self):
        setup = source("scripts/system-host.sh")
        self.assertIn("InvocationID", setup)
        self.assertIn("receipt", setup)

    def test_link_instructions(self):
        self.assertIn("/opt/sunna/host/current/bin/sunna-host link", source("scripts/release/install.sh.in"))

    def test_no_frozen_network(self):
        body = source("scripts/release/install.sh.in").split("port=48800")[1].split("elevate=sudo")[0]
        self.assertNotIn('listen="$ip:$port"', body)

if __name__ == "__main__":
    unittest.main()
