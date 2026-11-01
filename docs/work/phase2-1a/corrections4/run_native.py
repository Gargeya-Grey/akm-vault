"""Run one approved fake-data native proof, pinning build/settings identity before launch."""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument("mode", choices=["OutcomeOnly", "JournalOnly", "JournalExportOnly"])
parser.add_argument("--owner-ready-for-visibility", action="store_true")
parser.add_argument("--candidate", help="Immutable candidate name under visibility; never reuses a previous proof identity")
args = parser.parse_args()
if args.mode == "OutcomeOnly" and not args.owner_ready_for_visibility:
    parser.error("Owner readiness confirmation is required before any actual visibility attempt.")
if args.mode != "OutcomeOnly" and args.owner_ready_for_visibility:
    parser.error("Readiness flag applies only to OutcomeOnly.")
if args.candidate is not None and not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,63}", args.candidate):
    parser.error("Candidate must be a simple lowercase name, not a path.")

folder = Path(__file__).resolve().parent
root = folder.parents[3]
namespace = folder / "visibility" if args.candidate else folder
prefix = args.candidate + "-" if args.candidate else ""
record = namespace / ("native-" + prefix + args.mode + "-identity.json")
if record.exists():
    raise SystemExit("Refusing to overwrite or automatically repeat an earlier native proof.")

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None

def project_path(relative):
    path = (root / relative).resolve()
    if not path.is_relative_to(root) or Path(relative).is_absolute():
        raise SystemExit("Candidate paths must stay inside the project.")
    return path

candidate_sha = None
if args.candidate:
    candidate_file = namespace / (args.candidate + "-candidate.json")
    if not candidate_file.is_file():
        raise SystemExit("Missing immutable candidate record: " + str(candidate_file))
    candidate_sha = digest(candidate_file)
    candidate = json.loads(candidate_file.read_text(encoding="utf-8"))
    assert candidate["candidate"] == args.candidate and candidate["schemaVersion"] == 1, "Candidate identity mismatch"
    assert set(candidate["builds"]) == {"gui", "cli"}, "GUI and CLI build provenance required"
    for build in candidate["builds"].values():
        build_record = project_path(build["record"])
        assert digest(build_record) == build["recordSha256"], "Build record changed"
        build_data = json.loads(build_record.read_text(encoding="utf-8"))
        assert build_data["exit"] == 0, "Successful release builds required"
        assert digest(build_record.with_suffix(".log")) == build_data["logSha256"], "Build log changed"
    assert candidate["compiledInputs"], "Candidate input fingerprint required"
    assert all(digest(project_path(p)) == h for p, h in candidate["compiledInputs"].items()), "Candidate compiled input changed"
    artifacts = candidate["artifacts"]
else:
    # Historical namespace remains read-only in practice: its existing identity
    # files refuse a repeated proof before any build/artifact/settings access.
    for name in ["release-gui-bundles-1.json", "release-cli-1.json"]:
        assert json.loads((folder / name).read_text())["exit"] == 0, "Current successful release builds required"
    artifacts = json.loads((folder / "artifacts-sha256.json").read_text())
assert {"target/release/akm-desktop.exe", "target/release/akm.exe"} <= set(artifacts), "Exact launched artifacts required"
assert all(digest(project_path(p)) == h for p, h in artifacts.items()), "Built artifact identity changed"
settings = Path(os.environ["APPDATA"]) / "com.akm.desktop/settings.json"
before = digest(settings)
command = ["powershell.exe", "-NoProfile", "-File", str(root / "tools/verify_windows_app.ps1"),
           "-DesktopExe", str(root / "target/release/akm-desktop.exe"), "-CliExe", str(root / "target/release/akm.exe"), "-" + args.mode]
if args.owner_ready_for_visibility:
    command.append("-OwnerReadyForVisibility")
data = {"command": command, "started": datetime.now(timezone.utc).isoformat(), "artifacts": artifacts,
        "candidate": args.candidate, "candidateRecordSha256": candidate_sha,
        "settingsBeforeSha256": before, "exit": None, "state": "launching"}
with record.open("x", encoding="utf-8") as output:
    output.write(json.dumps(data, indent=2) + "\n")
result = None
try:
    result = subprocess.run(command, cwd=root, check=False)
    data["exit"] = result.returncode
finally:
    after = digest(settings)
    data.update({"finished": datetime.now(timezone.utc).isoformat(), "state": "finished" if result else "interrupted",
                 "settingsAfterSha256": after, "settingsRestoredByteForByte": before == after,
                 "artifactsUnchanged": all(digest(root / p) == h for p, h in artifacts.items())})
    record.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(data, indent=2), flush=True)
if not data["settingsRestoredByteForByte"] or not data["artifactsUnchanged"]:
    raise SystemExit("Proof changed settings/artifact identity: inspect ownership before any recovery or retry.")
sys.exit(result.returncode)
