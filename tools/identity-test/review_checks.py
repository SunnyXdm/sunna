"""Guard the platform wiring that cannot all be executed on a Linux runner."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
def source(path):
    return (ROOT / path).read_text()

class Wiring(unittest.TestCase):
    def test_queued_media_checks_close_before_delivery(self):
        code = source("crates/client/src/lib.rs")
        for branch, deliver in [("Some(data) = clipboard_rx.recv()", "clipboard.receive"),
                                ("datagram = connection.read_datagram()", "player.push")]:
            before = code.split(branch, 1)[1].split(deliver, 1)[0]
            self.assertIn("while_connected(&connection, data)", before)

    def test_android_reconnects_through_authenticated_client(self):
        code = source("crates/android/src/lib.rs")
        self.assertIn("sunna_client::reconnect_for_handoff", code)
        self.assertIn("sunna_client::session_changed", code)

    def test_service_install_checks_arguments_before_registering(self):
        install = source("apps/sunnad/src/service.rs").split("pub fn install(", 1)[1]
        self.assertIn("validate_service_args", install.split("CreateServiceW(", 1)[0])
        self.assertNotIn("info!(%command", source("apps/sunnad/src/console_session.rs"))

    def test_capture_disclosure(self):
        idle = next(line for line in source("docs/security.md").splitlines() if "**Idle.**" in line)
        for word in ["local", "Linux", "Mac", "frame"]:
            self.assertIn(word, idle)

if __name__ == "__main__":
    unittest.main()
