"""A one-commit public snapshot of HEAD passes its own required checks that need no build (finding U4-R1-05).

Usage: python tools/test_snapshot_runs_its_checks.py (from the repository root)

It writes the tracked entries of HEAD into a temporary folder as a repository with one commit and no
history, through the export of tools/make_public_snapshot.py but without its scan (the scan refuses
review records that are not public yet). Inside that folder it runs each check in CHECKS and fails when
one fails. So a check that needs a past commit, a file that Git ignores or the private repository's
configuration fails here. Uncommitted changes are not in the snapshot: commit them first. SKIPPED names
each required check that this test does not run, with the reason. It does not run itself.
"""

import importlib.util
import subprocess
import sys
import tempfile
import time
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS))
import git_child  # noqa: E402

AUTHOR = "Snapshot Check <snapshot@example.invalid>"  # public-snapshot: allow email (a synthetic author)
PYTHON = [sys.executable]
POWERSHELL = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]
CHECKS = [
    ("docs check", PYTHON + ["tools/check_docs.py"]),
    ("docs check test", PYTHON + ["tools/test_check_docs.py"]),
    ("git_child helper test", PYTHON + ["tools/test_git_child.py"]),
    ("public-snapshot tool test", PYTHON + ["tools/test_make_public_snapshot.py"]),
    ("no-network check", PYTHON + ["tools/check_no_network.py"]),
    ("no-network check test", PYTHON + ["tools/test_check_no_network.py"]),
    ("KeePassXC fetch tool test", POWERSHELL + ["tools/test_fetch_keepassxc.ps1"]),
    ("verification helper test", POWERSHELL + ["tools/test_akm_verify.ps1"]),
    ("native-proof guard test", POWERSHELL + ["tools/test_native_proof_guards.ps1"]),
    ("install binding test", POWERSHELL + ["tools/test_install_binding.ps1"]),
    ("proof snapshot test", POWERSHELL + ["tools/test_proof_snapshot.ps1"]),
]
SKIPPED = [
    ("third-party notices check and test", "they read desktop/node_modules, which only `npm ci` makes"),
    ("npm test, npm run test:ui, cargo test, cargo clippy, cargo fmt", "they need installed packages or a build"),
    ("KeePassXC and kdbxweb interchange", "they need the built CLI and a downloaded reader"),
    ("dependency checks", "they need cargo-audit, cargo-deny and the network"),
]

spec = importlib.util.spec_from_file_location("snapshot_tool", TOOLS / "make_public_snapshot.py")
tool = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = tool
spec.loader.exec_module(tool)

source = git_child.existing(TOOLS.parent)
commit = source.run("rev-parse", "HEAD").decode("ascii").strip()
tree = source.run("rev-parse", "HEAD^{tree}").decode("ascii").strip()
failures = 0
with tempfile.TemporaryDirectory(prefix="akm-snapshot-checks-") as folder:
    entries = tool.tracked(source, commit)
    tool.read_blobs(source, entries)
    out = Path(folder) / "snapshot"
    tool.export(source, commit, tree, entries, out, AUTHOR)
    snapshot = git_child.existing(out)
    count = snapshot.run("rev-list", "--all", "--count").decode().strip()
    print(f"{'ok  ' if count == '1' else 'FAIL'} the snapshot of {commit[:12]} has one commit and no history")
    failures += count != "1"
    for name, command in CHECKS:
        start = time.monotonic()
        done = subprocess.run(command, cwd=out, capture_output=True, text=True, encoding="utf-8", errors="replace")
        ok = done.returncode == 0
        print(f"{'ok  ' if ok else 'FAIL'} {name} passes inside the snapshot ({time.monotonic() - start:.0f} s)")
        if not ok:
            failures += 1
            lines = (done.stdout + done.stderr).strip().splitlines()
            print("     " + "\n     ".join([line for line in lines if line.startswith("FAIL")][:12] + lines[-3:]))
    for name, reason in SKIPPED:
        print(f"skip {name}: {reason}")
print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
