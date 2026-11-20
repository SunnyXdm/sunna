import pathlib
import os
import re
import subprocess
import sys
import tempfile
import unittest

from inventory import JOBS, files, collect

class Publication(unittest.TestCase):
    def test_documented_bootstrap_requires_complete_nonempty_download(self):
        section = pathlib.Path("README.md").read_text().split("## Updating", 1)[1]
        script = re.search(r"```sh\n(.*?)\n```", section, re.S).group(1)
        with tempfile.TemporaryDirectory() as temp:
            work = pathlib.Path(temp)
            curl = work / "curl"
            curl.write_text("""#!/bin/sh
while [ "$1" != -o ]; do shift; done
case "$RESPONSE" in
  empty) : > "$2";;
  complete) echo 'touch "$MARKER"' > "$2";;
  truncated) echo 'touch "$MARKER"' > "$2"; exit 18;;
esac
""")
            curl.chmod(0o700)
            marker = work / "ran"
            for response in ["empty", "truncated", "complete"]:
                with self.subTest(response=response):
                    env = dict(os.environ, PATH=temp + os.pathsep + os.environ["PATH"],
                               SUNNA_RELEASE_URL="https://fixture.invalid", RESPONSE=response,
                               MARKER=str(marker), TMPDIR=temp)
                    result = subprocess.run(["sh", "-c", script], env=env, timeout=5)
                    self.assertEqual(result.returncode == 0, response == "complete")
                    self.assertEqual(marker.exists(), response == "complete")

    def test_wrong_job_even_if_global_inventory_correct(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp) / "jobs"
            for job, target in JOBS.items():
                directory = root / job
                directory.mkdir(parents=True)
                for name in files(target):
                    (directory / name).write_bytes(b"fixture")
            (root / "android/sunna-android.apk").rename(root / "macos-macos-15/sunna-android.apk")
            with self.assertRaises(ValueError):
                collect(root, root.parent / "dist")

    def test_mismatched_tag(self):
        with tempfile.TemporaryDirectory() as tmp:
            import shutil
            root = pathlib.Path(tmp)
            for name in ["Cargo.toml", "apps/sunna/tauri.conf.json", "apps/android/app/build.gradle.kts"]:
                (root/name).parent.mkdir(parents=True, exist_ok=True)
                shutil.copy(name, root/name)
            r = subprocess.run([sys.executable, "scripts/release/version.py", "99.2.3", "--check", "--root", tmp], capture_output=True)
            self.assertNotEqual(r.returncode, 0)
            self.assertIn(b"version mismatch", r.stderr)

    def test_signer_must_match_publisher(self):
        with tempfile.TemporaryDirectory() as tmp:
            work = pathlib.Path(tmp)
            key = work/'fixture.key'
            subprocess.run(['openssl','genpkey','-algorithm','ED25519','-out',key],check=True)
            dist=work/'dist';dist.mkdir()
            for target in JOBS.values():
                for name in files(target): (dist/name).write_bytes(b'fixture')
            env=dict(os.environ,SUNNA_RELEASE_KEY=str(key))
            env.pop('SUNNA_TEST_PUBLIC_KEY',None)
            r=subprocess.run([sys.executable,'scripts/release/manifest.py','1.0.0',dist,'https://example.org/release'],env=env,capture_output=True)
            self.assertNotEqual(r.returncode,0)
            self.assertIn(b'signing key differs',r.stderr)
            self.assertFalse((dist/'manifest.json.sig').exists())
            self.assertFalse((dist/'install.sh').exists())

    def test_unknown_helper_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            (pathlib.Path(tmp)/"sunna-update-unlisted").write_text("fixture")
            r = subprocess.run([sys.executable, "scripts/release/manifest.py", "1.0.0", tmp, "https://example.org/release"], env=dict(os.environ, SUNNA_RELEASE_KEY="/nonexistent", SUNNA_TEST_PUBLIC_KEY="/nonexistent"), capture_output=True)
            self.assertNotEqual(r.returncode, 0)
            self.assertFalse((pathlib.Path(tmp)/"manifest.json").exists())

if __name__ == "__main__":
    unittest.main()
