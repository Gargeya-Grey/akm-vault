"""Run only one approved fake-native proof and record settings/artifact identity."""
import argparse
import hashlib
import json
from pathlib import Path
import os
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument("mode", choices=["OutcomeOnly", "JournalOnly", "JournalExportOnly"])
parser.add_argument("--attempt", type=int, default=1)
args = parser.parse_args()
root = Path(__file__).resolve().parents[4]
folder = Path(__file__).resolve().parent
suffix = "" if args.attempt == 1 else "-attempt" + str(args.attempt)
record = folder / ("native-" + args.mode + suffix + "-identity.json")
if record.exists():
    raise SystemExit("Do not overwrite earlier native proof records")
settings = Path(os.environ["APPDATA"]) / "com.akm.desktop/settings.json"
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None
gui = root / "target/release/akm-desktop.exe"
cli = root / "target/release/akm.exe"
build = json.loads((folder / "stable-gui-build.json").read_text(encoding="utf-8"))
assert build["exit"] == 0, "A successful current release build is required"
assert digest(cli) == "0024f9005a68bf5b5ab9d32297c9d6eb4043ee5616a7ae6c465a054adfddcdc0", "CLI identity changed: inspect before reusing its evidence"
before = digest(settings)
artifacts = {"guiSha256": digest(gui), "cliSha256": digest(cli)}
command = ["powershell.exe", "-NoProfile", "-File", str(root / "tools/verify_windows_app.ps1"), "-DesktopExe", str(gui), "-CliExe", str(cli), "-" + args.mode]
try:
    result = subprocess.run(command, cwd=root, check=False)
finally:
    after = digest(settings)
    data = {"command": command, **artifacts, "settingsBeforeSha256": before, "settingsAfterSha256": after, "settingsRestoredByteForByte": before == after, "artifactsUnchanged": digest(gui) == artifacts["guiSha256"] and digest(cli) == artifacts["cliSha256"]}
    record.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(data, indent=2), flush=True)
if before != after or not data["artifactsUnchanged"]:
    raise SystemExit("Native proof changed settings/artifact identity: inspect ownership before recovery")
sys.exit(result.returncode)
