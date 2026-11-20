"""Exercise the real supervisor's timeout with a previous confirmed release."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "scripts/release"))
import importlib.util
spec = importlib.util.spec_from_file_location("recovery", Path(sys.path[0]) / "test-recovery.py")
recovery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(recovery)

class Probation(recovery.Recovery):
    def test_waiting_candidate_has_a_deadline_with_known_good(self):
        self.child(recovery.B, '''
n=0
while true; do
  printf "%s %s waiting %d" "$$" "$SUNNA_SELECTED" "$n" > "$SUNNA_READY_FILE"
  n=$((n + 1))
  sleep .05
done
''')
        self.old_runs()

if __name__ == "__main__":
    unittest.main(defaultTest="Probation.test_waiting_candidate_has_a_deadline_with_known_good")
