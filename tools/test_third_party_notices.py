"""THIRD_PARTY_NOTICES.md is current, and the check fails when it is not (finding FR-25, SEC-04).

Usage: python tools/test_third_party_notices.py (from the repository root)
"""

import subprocess
import sys
import tempfile
from pathlib import Path

GENERATOR = [sys.executable, "tools/generate_third_party_notices.py"]
NOTICES = Path("THIRD_PARTY_NOTICES.md")
MPL_CRATES = {"cssparser", "cssparser-macros", "selectors", "dtoa-short", "option-ext"}

failures = 0


def case(name, ok, detail=""):
    global failures
    print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if not ok:
        failures += 1
        if detail:
            print(f"     {detail}")


def check(output):
    return subprocess.run(GENERATOR + ["--check", "--output", str(output)], capture_output=True, text=True)


run = check(NOTICES)
case("the committed file is current", run.returncode == 0, run.stdout + run.stderr)

text = NOTICES.read_text(encoding="utf-8")
with tempfile.TemporaryDirectory(prefix="akm-notices-test-") as folder:
    stale = Path(folder) / "THIRD_PARTY_NOTICES.md"
    stale.write_bytes(text.replace("| react | ", "| react-changed | ", 1).encode("utf-8"))
    run = check(stale)
    case("an out-of-date file fails the check", run.returncode == 1 and "out of date" in run.stdout, run.stdout + run.stderr)
    run = check(Path(folder) / "missing.md")
    case("a missing file fails the check", run.returncode == 1, run.stdout + run.stderr)

section = text.split("## MPL-2.0 crates", 1)[1].split("\n## ", 1)[0]
rows = {line.split("|")[1].strip() for line in section.splitlines() if line.startswith("| ") and not line.startswith("| Crate") and not line.startswith("|---")}
case("the MPL-2.0 section names exactly the five crates of SEC-04", rows == MPL_CRATES, f"found {sorted(rows)}")
case("every MPL-2.0 crate has a source archive address", all(f"/{name}/{name}-" in section for name in MPL_CRATES))

print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
