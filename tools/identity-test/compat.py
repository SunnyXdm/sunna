"""Exercise real protocol-8 binaries on loopback. Build the old binaries from git archive HEAD."""
import argparse
import contextlib
import os
from pathlib import Path
import secrets
import socket
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument("old_target", type=Path, help="old build's target/debug")
parser.add_argument("new_target", type=Path, help="new build's target/debug")
args = parser.parse_args()
old, new = args.old_target.resolve(), args.new_target.resolve()
with tempfile.TemporaryDirectory(prefix="sunna-id-pairs-", dir=Path.home()) as work:
    root = Path(work)
    env = {**os.environ, "HOME": str(root), "XDG_CONFIG_HOME": str(root/"config"),
           "SUNNA_TOKEN": secrets.token_hex(16), "SUNNA_LOG_URL": "", "SUNNA_LOG_TOKEN": ""}
    with socket.socket(type=socket.SOCK_DGRAM) as sock:
        sock.bind(("127.0.0.1", 0))
        address = f"127.0.0.1:{sock.getsockname()[1]}"

    @contextlib.contextmanager
    def serving(target, strict=False, listen=None, config=None):
        proc = subprocess.Popen([target/"sunnad", "--listen", listen or address, "--source", "synthetic", "--codec", "raw",
                                 "--no-audio", "--no-clipboard"], env=dict(config or env, SUNNA_REQUIRE_PROOF=str(int(strict))),
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        try:
            time.sleep(.7)
            assert proc.poll() is None
            yield
        finally:
            proc.terminate()
            proc.wait(timeout=10)

    def run(target, verb, *, fail=None, older=False, pin=None, upgraded=False):
        client = "old-viewer" if target == old else "update-proof"
        config = dict(env, SUNNA_TEST_ALLOW_OLDER=str(int(older)), SUNNA_TEST_UPGRADED=str(int(upgraded)))
        if pin is not None:
            config["SUNNA_TEST_PIN"] = pin
        result = subprocess.run([target/"examples"/client, verb, address, "2"], env=config,
                                capture_output=True, text=True, timeout=20)
        if fail is None:
            error = result.stderr.replace(env["SUNNA_TOKEN"], "[redacted]")
            assert result.returncode == 0, f"{client} {verb} exited {result.returncode}: {error}"
        else:
            assert result.returncode != 0 and fail in result.stderr, f"{client} {verb} expected {fail}"
        return result.stdout

    with serving(old):
        run(old, "probe")
        run(old, "hold")
        print("Two deployed binaries completed their normal probe and session.", flush=True)
        for verb in ["probe", "hold", "status"]:
            run(new, verb, fail="Needs an update")
        run(new, "probe", older=True, upgraded=True, fail="Needs an update")
        assert "not fully verified" in run(new, "probe", older=True)
        run(new, "hold", older=True)
        run(new, "update", older=True, fail="predates remote updates")
        wrong = dict(env, SUNNA_TEST_ALLOW_OLDER="1", SUNNA_TOKEN=secrets.token_hex(16))
        state = subprocess.check_output([new/"examples/update-proof", "check-state", address],
                                        env=wrong, text=True, timeout=10)
        assert state.strip() == "wrong-key false"
        print("Old host requires the saved exception for probe and session. Remote updates unsupported.", flush=True)
    # Hold a real old-host viewer after it loaded the enabled exception. A
    # separate process either disables it or proves the real host first.
    import selectors
    with socket.socket(type=socket.SOCK_DGRAM) as sock:
        sock.bind(("127.0.0.1", 0))
        modern_address = f"127.0.0.1:{sock.getsockname()[1]}"
    with serving(old), serving(new, listen=modern_address):
        for change, expected in [("policy", "Needs an update"), ("probe", "computer you added")]:
            run(new, "policy", older=True)
            config = dict(env, SUNNA_TEST_ALLOW_OLDER="1", SUNNA_TEST_SNAPSHOT_BARRIER="1")
            with subprocess.Popen([new/"examples/update-proof", "probe", address], env=config,
                                  stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                  text=True) as stale:
                try:
                    with selectors.DefaultSelector() as ready:
                        ready.register(stale.stdout, selectors.EVENT_READ)
                        assert ready.select(10) and stale.stdout.readline().strip() == "snapshot loaded"
                    subprocess.run([new/"examples/update-proof", change, modern_address],
                                   env=dict(env, SUNNA_TEST_ALLOW_OLDER=str(int(change == "probe"))),
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15, check=True)
                    _, err = stale.communicate("\n", timeout=15)
                    assert stale.returncode != 0 and expected in err
                finally:
                    if stale.poll() is None:
                        stale.kill()
        print("Stale viewer snapshots refused after Edit disabled the exception and after first proof.", flush=True)
    with serving(new):
        run(old, "probe")
        run(old, "hold")
        assert "probe ok" in run(new, "probe", older=True)
        run(new, "status")
        run(new, "hold")
        print("New host accepted the deployed client and verified the new client.", flush=True)
    with serving(old):
        for verb in ["probe", "hold", "update"]:
            run(new, verb, fail="computer you added", older=True)
        print("A previously upgraded record refused the old endpoint at the same address.", flush=True)
    with serving(new, strict=True):
        for verb in ["probe", "hold"]:
            run(old, verb, fail="")
        run(new, "probe")
        print("Proof-required host refused the old app and kept the same pin after restart.", flush=True)
    subprocess.run([new/"sunnad", "new-identity"], env=env, check=True, stdout=subprocess.DEVNULL)
    with serving(new):
        run(new, "probe", fail="computer you added")
        pin = subprocess.check_output([new/"sunnad", "identity"], env=env, text=True).strip()
        run(new, "add-link", pin=pin)
        run(new, "probe")
        marker = root/".sunna"/"identities"/"release-fixture"
        marker.write_text("allow-older")
        run(new, "probe", pin=pin)
        assert marker.read_text() == pin
        print("Rotation invalidated the saved record. Re-adding the new pin restored the connection.", flush=True)

    # Endpoint substitution through the public launcher check and daemon. Give
    # the other endpoint its own identity and sharing key. Each row starts
    # with independent private viewer state.
    alternate = dict(env, XDG_CONFIG_HOME=str(root/"other-host"), SUNNA_TOKEN=secrets.token_hex(16))
    for implementation in [old, new]:
        for label, pinned, upgraded, older in [
            ("pinned", True, True, False),
            ("upgraded", False, True, False),
            ("upgraded-stale-switch", False, True, True),
            ("new", False, False, False),
            ("exception", False, False, True),
        ]:
            # A row measures admission policy independently of previous rows'
            # intentional wrong-key attempts and source backoff.
            with serving(implementation, config=alternate):
                profile = root/f"matrix-{int(implementation == new)}-{label}"
                profile.mkdir(exist_ok=True)
                viewer = dict(env, HOME=str(profile), SUNNA_TEST_ALLOW_OLDER=str(int(older)),
                              SUNNA_TEST_UPGRADED=str(int(upgraded)))
                if pinned:
                    viewer["SUNNA_TEST_PIN"] = pin
                result = subprocess.check_output([new/"examples/update-proof", "check-state", address],
                                                 env=viewer, text=True, timeout=10).strip()
                expected = ("identity-changed" if pinned else
                            "update-needed" if implementation == old and (upgraded or not older) else "wrong-key")
                assert result == expected + " false", f"{label}: expected {expected}, got {result}"
    print("All saved-record policies reported the expected state against independent old and new endpoints.", flush=True)
