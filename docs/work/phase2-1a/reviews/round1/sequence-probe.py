import json
import os
from pathlib import Path
import subprocess

here = Path(__file__).parent
directory = here / "sequence-case"
directory.mkdir(exist_ok=False)
vault = directory / "fake.kdbx"
journal = directory / "fake.kdbx.audit.jsonl"
good = (here / "action-case/fake.kdbx.audit.jsonl").read_bytes()
first = json.loads(good.splitlines()[0])
journal.write_bytes(good.replace(b"add-completed", b"bad-completed", 1))
env = os.environ.copy()
env["TEMP"] = str(here)
env["TMP"] = str(here)
exe = Path(r"W:\API Keys Manager\target\release\akm.exe")
result = subprocess.run([str(exe), "--vault", str(vault), "audit", "--verify"], capture_output=True, text=True, env=env, cwd=here, timeout=30)
output = {"expected_offending_sequence": first["seq"], "exit": result.returncode, "stdout": result.stdout, "stderr": result.stderr}
(here / "sequence-results.json").write_text(json.dumps(output, indent=2), encoding="utf-8")
print(json.dumps(output, indent=2))
