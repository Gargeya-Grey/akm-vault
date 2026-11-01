"""Check proof parsing/refusal without launching AKM, reading settings or touching a desktop."""
from pathlib import Path
import subprocess
import sys

folder = Path(__file__).resolve().parent
root = folder.parents[3]
proof = root / "tools/verify_windows_app.ps1"
parse = "$tokens=$null; $errors=$null; [void][System.Management.Automation.Language.Parser]::ParseFile('" + str(proof).replace("'", "''") + "',[ref]$tokens,[ref]$errors); if($errors.Count){$errors; exit 1}"
result = subprocess.run(["powershell.exe", "-NoProfile", "-Command", parse], capture_output=True, text=True)
assert result.returncode == 0, result.stdout + result.stderr
print("PASS: native proof parses without execution")

refused = subprocess.run(["powershell.exe", "-NoProfile", "-File", str(proof), "-DesktopExe", "unused-no-file", "-CliExe", "unused-no-file", "-OutcomeOnly"], capture_output=True, text=True)
assert refused.returncode != 0 and "owner to confirm readiness first" in refused.stdout + refused.stderr, refused.stdout + refused.stderr
print("PASS: PowerShell OutcomeOnly refuses before Add-Type, path resolution, scratch/settings/process/visibility work")

refused = subprocess.run([sys.executable, str(folder / "run_native.py"), "OutcomeOnly"], capture_output=True, text=True)
assert refused.returncode != 0 and "Owner readiness confirmation is required" in refused.stdout + refused.stderr, refused.stdout + refused.stderr
print("PASS: native wrapper refuses before build/artifact/settings access without owner readiness")

for flags, message in [
    (["OutcomeOnly", "--candidate", "visibility-1"], "Owner readiness confirmation is required"),
    (["JournalOnly", "--candidate", "../invalid"], "Candidate must be a simple lowercase name"),
    (["JournalOnly", "--candidate", "guard-probe-nonexistent"], "Missing immutable candidate record"),
    (["JournalOnly", "--owner-ready-for-visibility"], "Readiness flag applies only to OutcomeOnly"),
]:
    refused = subprocess.run([sys.executable, str(folder / "run_native.py"), *flags], capture_output=True, text=True)
    assert refused.returncode != 0 and message in refused.stdout + refused.stderr, refused.stdout + refused.stderr
print("PASS: candidate namespace cannot bypass readiness, accept paths, or launch without immutable provenance")
