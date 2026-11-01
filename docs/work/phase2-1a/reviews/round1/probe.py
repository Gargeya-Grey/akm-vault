import hashlib
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(r"W:\API Keys Manager")
HERE = Path(__file__).parent
EXE = ROOT / "target/release/akm.exe"
MANIFEST = ROOT / "docs/work/phase2-1a/source-sha256.json"
BEFORE = ROOT / "docs/work/phase2-1a/before-sha256.json"
DIFF = ROOT / "docs/work/phase2-1a/changes.diff"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
before = json.loads(BEFORE.read_text(encoding="utf-8"))
changed = []
missing = []
for relative, expected in manifest.items():
    path = ROOT / relative
    if not path.is_file():
        missing.append(relative)
    elif sha(path) != expected:
        changed.append(relative)
application_roots = [ROOT / "crates", ROOT / "desktop/src", ROOT / "desktop/src-tauri/src", ROOT / "desktop/e2e"]
unlisted = sorted(path.relative_to(ROOT).as_posix() for folder in application_roots for path in folder.rglob("*") if path.is_file() and path.relative_to(ROOT).as_posix() not in manifest)
integrity = {
    "source_manifest_sha256": sha(MANIFEST),
    "before_manifest_sha256": sha(BEFORE),
    "diff_sha256": sha(DIFF),
    "release_cli_sha256": sha(EXE),
    "manifest_path_count": len(manifest),
    "current_changed": changed,
    "current_missing": missing,
    "unlisted_application_files": unlisted,
    "manifest_before_to_candidate_changed": sorted(path for path in manifest if before.get(path) != manifest[path]),
}
(HERE / "integrity.json").write_text(json.dumps(integrity, indent=2), encoding="utf-8")

env = os.environ.copy()
env["AKM_PASSWORD"] = "fake-review-password-123!"
env["TEMP"] = str(HERE)
env["TMP"] = str(HERE)


def call(vault, *args, input=None):
    result = subprocess.run([str(EXE), "--vault", str(vault), *args], input=input, text=True, capture_output=True, env=env, cwd=HERE, timeout=60)
    return {"exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr}


results = {}
backup_dir = HERE / "backup-case"
backup_dir.mkdir(exist_ok=False)
vault = backup_dir / "fake.kdbx"
results["backup_init"] = call(vault, "init")
(backup_dir / "fake.kdbx.bak").mkdir()
results["backup_add"] = call(vault, "add", "FAKE_KEY", "--secret-stdin", input="fake-secret-for-review\n")
results["backup_audit"] = call(vault, "audit", "--verify")

newline_dir = HERE / "unterminated-case"
newline_dir.mkdir(exist_ok=False)
vault = newline_dir / "fake.kdbx"
results["newline_init"] = call(vault, "init")
results["newline_add"] = call(vault, "add", "FAKE_KEY", "--secret-stdin", input="fake-secret-for-review\n")
journal = newline_dir / "fake.kdbx.audit.jsonl"
initial = journal.read_bytes()
assert initial.endswith(b"\n")
journal.write_bytes(initial[:-1])
results["newline_before_audit"] = call(vault, "audit", "--verify")
results["newline_reveal"] = call(vault, "get", "FAKE_KEY", "--field", "secret")
results["newline_after_audit"] = call(vault, "audit", "--verify")
results["newline_after_raw"] = {"physical_line_count": len(journal.read_bytes().splitlines()), "contains_concatenated_objects": b"}{" in journal.read_bytes(), "contains_requested_event": b"reveal-requested" in journal.read_bytes()}

echo_dir = HERE / "malformed-case"
echo_dir.mkdir(exist_ok=False)
vault = echo_dir / "fake.kdbx"
results["malformed_init"] = call(vault, "init")
results["malformed_add"] = call(vault, "add", "FAKE_KEY", "--secret-stdin", input="fake-secret-for-review\n")
journal = echo_dir / "fake.kdbx.audit.jsonl"
damaged = b'{"seq":"FAKE_JOURNAL_MARKER"}\n'
journal.write_bytes(damaged)
results["malformed_audit"] = call(vault, "audit", "--verify")
results["malformed_reveal"] = call(vault, "get", "FAKE_KEY", "--field", "secret")
results["malformed_preserved"] = journal.read_bytes() == damaged

actions_dir = HERE / "action-case"
actions_dir.mkdir(exist_ok=False)
vault = actions_dir / "fake.kdbx"
results["actions_init"] = call(vault, "init")
results["actions_add"] = call(vault, "add", "FAKE_KEY", "--secret-stdin", input="fake-secret-for-review\n")
results["actions_export"] = call(vault, "export", "--all")
results["actions_audit"] = call(vault, "audit", "--verify")
results["actions_records"] = [json.loads(line)["action"] for line in (actions_dir / "fake.kdbx.audit.jsonl").read_text(encoding="utf-8").splitlines()]

(HERE / "probe-results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
print(json.dumps({"integrity": integrity, "probe_results": results}, indent=2))
