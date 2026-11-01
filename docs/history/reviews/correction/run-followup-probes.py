import json
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent
common = ["--offline", "--locked", "--manifest-path", str(root / "Cargo.toml"), "--target-dir", str(root / "target")]
checks = [
    ("followup-ownership", ["cargo", "run", "--bin", "corrected_ownership", *common]),
    ("followup-receipt-native", ["cargo", "run", "--bin", "late_receipt_native", *common]),
    ("followup-clipboard-model", ["cargo", "test", "--bin", "clipboard_model", *common,
        "clipboard::windows::transaction_tests::captured_lease_survives_windows_close_synthesis_and_owner_destruction", "--", "--exact"]),
]
results = []
for name, command in checks:
    result = subprocess.run(command, cwd=root, capture_output=True, text=True, timeout=90)
    (root / f"{name}.txt").write_text(result.stdout + result.stderr, encoding="utf-8")
    results.append({"name": name, "exit_code": result.returncode, "stdout": result.stdout, "stderr": result.stderr})
(root / "followup-native-results.json").write_text(json.dumps(results, indent=2), encoding="utf-8")
print(json.dumps(results, indent=2))
raise SystemExit(int(any(result["exit_code"] for result in results)))
