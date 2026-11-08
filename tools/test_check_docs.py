"""The doc check fails on two real past mistakes and passes on the working tree.

Usage: python tools/test_check_docs.py (from the repository root)
"""

import subprocess
import sys

CHECK = [sys.executable, "tools/check_docs.py"]
PAST = {
    # The threat model as the security branch first wrote it: "%APPDATA%\akm" lost its backslashes to escapes.
    "cfce854": "docs/THREAT_MODEL.md:60: control character 0x07",
    # Review 1 finding 9: the threat model named a test that unit 1h had removed.
    "e9e5fb9": "names `locking_clears_only_the_sessions_clipboard_and_invalidates_imports`",
}

failures = 0
for rev, expected in PAST.items():
    run = subprocess.run(CHECK + ["--rev", rev], capture_output=True, text=True)
    ok = run.returncode == 1 and expected in run.stdout
    print(f"{'ok  ' if ok else 'FAIL'} the check fails at {rev} with: {expected}")
    failures += not ok

run = subprocess.run(CHECK, capture_output=True, text=True)
ok = run.returncode == 0
print(f"{'ok  ' if ok else 'FAIL'} the working tree passes")
failures += not ok
if not ok:
    print(run.stdout)
print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
