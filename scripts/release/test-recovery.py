"""Run a real supervisor against disposable version trees, without contacting a service."""
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import unittest
from switch_states import STEPS, materialize

HELPER = Path(os.environ.get("SUNNA_TEST_HELPER", "target/debug/sunna-update")).resolve()
A, B = "1.0.0", "1.1.0"


class Recovery(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(dir=Path.home())
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name) / "host"
        self.env = dict(os.environ, SUNNA_UPDATE_ROOT=self.temp.name, SUNNA_UPDATE="off",
                        SUNNA_TEST_START_MS="250")
        self.child(A, 'touch "$SUNNA_UPDATE_ROOT/ran-old"; exit 0')
        self.child(B, 'exit 42')
        self.pointer(B)
        self.save(dict(current=B, previous=A, healthy=False))
        (self.root / "known-good").write_text(A)

    def child(self, version, script):
        f = self.root / "versions" / version / "bin/sunnad"
        f.parent.mkdir(parents=True, exist_ok=True)
        f.write_text(f'#!/bin/sh\nif [ "$1" = --version ]; then echo sunnad {version}; exit; fi\n{script}\n')
        f.chmod(0o700)

    def pointer(self, version):
        link = self.root / "current"
        link.unlink(missing_ok=True)
        link.symlink_to(Path("versions") / version)

    def save(self, state, name="state.json"):
        (self.root / name).write_text(json.dumps(state))

    def supervise(self):
        p = subprocess.Popen([HELPER, "supervise", "host"], env=self.env, start_new_session=True)
        try:
            p.wait(timeout=6)
        finally:
            try:
                os.killpg(p.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            p.wait()

    def old_runs(self):
        self.supervise()
        self.assertTrue((self.root.parent / "ran-old").exists())
        self.assertEqual((self.root / "current").resolve().name, A)

    def test_nonzero(self):
        self.old_runs()

    def test_early_success(self):
        self.child(B, 'exit 0')
        self.old_runs()

    def test_wrong_child_cannot_confirm(self):
        self.child(B, 'echo -n "999999 $SUNNA_SELECTED" > "$SUNNA_READY_FILE"; exec sleep 100')
        self.old_runs()

    def test_hang(self):
        self.child(B, 'exec sleep 100')
        self.old_runs()

    def test_corrupt_state(self):
        (self.root / "state.json").write_text('{')
        self.old_runs()
        self.assertTrue(list(self.root.glob("state.json.invalid*")))

    @unittest.skipIf(os.geteuid() == 0, "requires read-only directory permissions")
    def test_readonly_corrupt_state_still_serves_known_good(self):
        for name in ("daemon.lock","update.lock"):
            (self.root/name).touch()
        (self.root/"state.json").write_text("{")
        self.root.chmod(0o500)
        try:
            self.supervise()
            self.assertTrue((self.root.parent/"ran-old").exists())
        finally:
            self.root.chmod(0o700)

    def test_unknown_format(self):
        self.save(dict(format=99, current=B, healthy=True))
        self.old_runs()
        self.assertTrue(list(self.root.glob("state.json.invalid*")))

    def test_health_write_retries_after_lock_contention(self):
        self.child(B, """
exec 7>"$SUNNA_UPDATE_ROOT/host/update.lock"
flock -x 7
echo -n "$$ $SUNNA_SELECTED" > "$SUNNA_READY_FILE"
sleep .5
flock -u 7
sleep .7
exit 0
""")
        self.supervise()
        state = json.loads((self.root / "state.json").read_text())
        self.assertTrue(state["healthy"])
        self.assertEqual(state["current"], B)

    def test_missing_stage(self):
        self.pointer(A)
        self.save(dict(current=A, healthy=True))
        self.save(dict(current="2.0.0", previous=A), "transaction.json")
        self.old_runs()

    def test_app_launch_policy(self):
        app_root = self.root.parent / "app"
        for v in [A, B]:
            binary = app_root / "versions" / v / "bin/sunna-real"
            binary.parent.mkdir(parents=True)
            binary.write_text(f'#!/bin/sh\necho {v} > "$SUNNA_UPDATE_ROOT/opened"\nif [ -n "$SUNNA_READY_FILE" ]; then printf "%s %s" "$$" "$SUNNA_SELECTED" > "$SUNNA_READY_FILE"; sleep .3; fi\n')
            binary.chmod(0o700)
        for policy in ["notify", "off", "auto"]:
            with self.subTest(policy=policy):
                (app_root/"current").unlink(missing_ok=True)
                (app_root/"current").symlink_to("versions/"+A)
                (app_root/"state.json").write_text(json.dumps(dict(current=A, healthy=True, pending=B, policy=policy)))
                env = dict(self.env, SUNNA_UPDATE=policy)
                subprocess.run([HELPER, "supervise", "app"], env=env, timeout=5, check=True)
                self.assertEqual((self.root.parent/"opened").read_text().strip(), B if policy == "auto" else A)

    def test_switch_boundaries(self):
        for step in STEPS:
            with self.subTest(step=step):
                materialize(self.root, A, B, step)
                (self.root.parent / "ran-old").unlink(missing_ok=True)
                self.old_runs()


if __name__ == "__main__":
    unittest.main()
