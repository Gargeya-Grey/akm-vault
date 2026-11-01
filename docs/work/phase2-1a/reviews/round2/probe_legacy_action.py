import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).parent
cli = Path(r"W:\API Keys Manager\target\release\akm.exe")
with tempfile.TemporaryDirectory(prefix="fake-journal-", dir=root) as tmp:
    vault = Path(tmp) / "vault.kdbx"
    journal = Path(str(vault) + ".audit.jsonl")
    raw = "\n".join(["", "1", "2026-10-31T00:00:00+00:00", "__proto__", "", "", "", "", ""])
    entry = {"seq": 1, "ts": "2026-10-31T00:00:00Z", "action": "__proto__", "prev": "", "hash": hashlib.sha256(raw.encode()).hexdigest()}
    journal.write_text(json.dumps(entry, separators=(",", ":")) + "\n", encoding="utf-8")
    result = subprocess.run([str(cli), "--vault", str(vault), "audit", "--verify"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    output = {"cli_audit_exit": result.returncode, "reported_consistent": b"Recorded chain is consistent" in result.stdout, "displayed_action": b"__proto__" in result.stdout, "fake_journal_removed_on_exit": True}
    (root / "legacy-action-result.json").write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(output, indent=2))
