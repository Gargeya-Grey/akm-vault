"""Capture a named check once, with its actual exit and source identity."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from datetime import datetime, timezone

sys.stdout.reconfigure(encoding="utf-8", errors="replace")
parser = argparse.ArgumentParser()
parser.add_argument("--name", required=True)
parser.add_argument("--cwd", default=".")
parser.add_argument("command", nargs=argparse.REMAINDER)
args = parser.parse_args()
root = Path(__file__).resolve().parents[4]
folder = Path(__file__).resolve().parent
log = folder / (args.name + ".log")
record = folder / (args.name + ".json")
if log.exists() or record.exists():
    raise SystemExit("Refusing to overwrite an earlier check: " + args.name)
command = args.command[1:] if args.command[:1] == ["--"] else args.command
pin = json.loads((folder / "before-cycle-sha256.json").read_text(encoding="utf-8-sig"))
paths = set(pin)
paths.update(str(p.relative_to(root)).replace("\\", "/") for base in ["desktop/src", "desktop/e2e"] for p in (root / base).rglob("*") if p.is_file())
sources = {p: hashlib.sha256((root / p).read_bytes()).hexdigest() for p in sorted(paths) if (root / p).is_file()}
(folder / (args.name + "-sources.json")).write_text(json.dumps(sources, indent=2) + "\n", encoding="utf-8")
started = datetime.now(timezone.utc).isoformat()
with log.open("wb") as output:
    result = subprocess.run(command, cwd=root / args.cwd, stdout=output, stderr=subprocess.STDOUT, check=False)
record.write_text(json.dumps({"command": command, "cwd": str(root / args.cwd), "started": started, "finished": datetime.now(timezone.utc).isoformat(), "exit": result.returncode, "logSha256": hashlib.sha256(log.read_bytes()).hexdigest()}, indent=2) + "\n", encoding="utf-8")
print(log.read_text(encoding="utf-8", errors="replace")[-24000:])
print("RECORDED EXIT:", result.returncode)
sys.exit(result.returncode)
