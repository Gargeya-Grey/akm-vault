import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).parent
CLI = Path(r"W:\API Keys Manager\target\release\akm.exe")
FAKE_PASSWORD = "fake-acceptance-password-only"
FAKE_SECRET = "fake-acceptance-secret-only"

with tempfile.TemporaryDirectory(prefix="fake-vault-", dir=ROOT) as tmp:
    work = Path(tmp)
    vault = work / "vault.kdbx"
    journal = work / "vault.kdbx.audit.jsonl"
    env = {key: value for key, value in os.environ.items() if not key.upper().startswith("AKM_")}
    env.update({"AKM_PASSWORD": FAKE_PASSWORD, "TEMP": tmp, "TMP": tmp})

    def cli(*args, stdin=None):
        return subprocess.run([str(CLI), "--vault", str(vault), *args], input=stdin,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, check=False)

    init = cli("init")
    added = cli("add", "Fake target", "--secret-stdin", stdin=(FAKE_SECRET + "\n").encode())
    if init.returncode != 0 or added.returncode != 0:
        raise RuntimeError(f"fake setup failed: init={init.returncode}, add={added.returncode}")
    baseline = journal.read_bytes()
    results = []
    for label, suffix in [("space", b" "), ("form-feed", b"\x0c"), ("vertical-tab", b"\x0b")]:
        before = baseline + suffix
        journal.write_bytes(before)
        check_before = cli("audit", "--verify")
        reveal = cli("get", "Fake target", "--field", "secret")
        after = journal.read_bytes()
        check_after = cli("audit", "--verify")
        results.append({"trailing_byte": label, "audit_before_exit": check_before.returncode,
                        "reveal_exit": reveal.returncode,
                        "fake_secret_disclosed": FAKE_SECRET.encode() in reveal.stdout,
                        "journal_changed": after != before,
                        "audit_after_exit": check_after.returncode,
                        "audit_after_damaged": b"Journal data is damaged" in check_after.stdout})

    output = {"cli_sha256": hashlib.sha256(CLI.read_bytes()).hexdigest(),
              "runtime_data": "temporary fake vault and journal removed by TemporaryDirectory",
              "results": results}
    (ROOT / "whitespace-boundary-result.json").write_text(json.dumps(output, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(output, indent=2))
