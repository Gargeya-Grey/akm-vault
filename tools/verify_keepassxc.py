"""Interchange regression against a real KeePassXC executable, using fake keys only."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET


PASSWORD = "interop test correct horse battery staple"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--akm", type=Path, required=True, help="Freshly built akm.exe")
    parser.add_argument("--keepassxc", type=Path, required=True, help="Real keepassxc-cli.exe")
    args = parser.parse_args()
    for path in (args.akm, args.keepassxc):
        if not path.is_file():
            parser.error(f"Executable not found: {path}")

    env = {name: value for name, value in os.environ.items() if not name.upper().startswith("AKM_")}
    env["AKM_PASSWORD"] = PASSWORD
    temp_root = Path(tempfile.gettempdir()) / "opencode"
    temp_root.mkdir(exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="akm-interop-", dir=temp_root) as raw:
        work = Path(raw)
        vault = work / "vault.kdbx"
        env["AKM_JOURNAL_ROOT"] = str(work / "akm-journal")

        def akm(*words, success=True):
            result = subprocess.run(
                [str(args.akm.resolve()), "--vault", str(vault), *words],
                env=env, text=True, encoding="utf-8", capture_output=True, timeout=90,
            )
            if success and result.returncode != 0:
                raise AssertionError(f"AKM {words[0]} failed: {result.stderr}")
            if not success and result.returncode == 0:
                raise AssertionError(f"AKM {words[0]} unexpectedly succeeded")
            return result

        def kpxc(*words, extra_input=""):
            result = subprocess.run(
                [str(args.keepassxc.resolve()), *map(str, words)],
                input=f"{PASSWORD}\n{extra_input}", env=env, text=True,
                encoding="utf-8", capture_output=True, timeout=90,
            )
            if result.returncode != 0:
                raise AssertionError(f"KeePassXC {words[0]} failed: {result.stderr}")
            return result.stdout

        def exported_entries():
            xml = ET.fromstring(kpxc("export", "-f", "xml", vault))
            entries = {}
            root = xml.find("./Root/Group")
            assert root is not None
            for group in root.iter("Group"):
                for entry in group.findall("Entry"):
                    fields = {field.findtext("Key"): field.findtext("Value") for field in entry.findall("String")}
                    entries[fields.get("Title")] = (entry, fields)
            return entries

        akm("init", "--name", "Interchange regression")
        akm("add", "OpenAI interop", "--project", "demo", "--env", "dev", "--secret", "fake-original-value", "--env-var", "OPENAI_API_KEY")
        original_keys = json.loads(akm("list", "--json").stdout)
        original_id = original_keys[0]["id"]
        info = kpxc("db-info", vault)
        assert "Argon2id" in info and "65536" in info, info
        assert exported_entries()["OpenAI interop"][1]["Password"] == "fake-original-value"

        attachment = work / "fixture.txt"
        attachment.write_bytes(b"Independent-reader attachment fixture.\r\n")
        kpxc("attachment-import", vault, "API Keys/OpenAI interop", "fixture.txt", attachment)
        kpxc("edit", "--notes", "Edited in the real KeePassXC", vault, "API Keys/OpenAI interop")
        kpxc("add", "-p", "-u", "foreign-user", vault, "API Keys/Foreign key", extra_input="fake-foreign-value\n")
        kpxc("mkdir", vault, "Unrelated")
        kpxc("add", "-p", vault, "Unrelated/Other entry", extra_input="fake-unrelated-value\n")
        before_upgrade = vault.read_bytes()
        assert int.from_bytes(before_upgrade[8:12], "little") == 0x00040000, "Expected KeePassXC KDBX 4.0 fixture"

        akm("add", "Must not be saved", "--secret", "fake", success=False)
        assert vault.read_bytes() == before_upgrade, "Refused upgrade changed the original vault"
        assert not list(work.glob("*.tmp")), "Refused write left a temporary file"
        akm("upgrade", "--yes")
        assert int.from_bytes(vault.read_bytes()[8:12], "little") == 0x00040001
        assert Path(f"{vault}.bak").read_bytes() == before_upgrade, "Upgrade lost the original backup"

        keys = json.loads(akm("list", "--json").stdout)
        assert next(key for key in keys if key["title"] == "OpenAI interop")["id"] == original_id
        foreign = next(key for key in keys if key["title"] == "Foreign key")
        assert akm("get", foreign["id"], "--field", "secret").stdout.strip() == "fake-foreign-value"
        akm("add", "After interchange", "--secret", "fake-new-value")
        entries = exported_entries()
        assert entries["OpenAI interop"][1]["Notes"] == "Edited in the real KeePassXC"
        assert entries["OpenAI interop"][0].find("History/Entry") is not None, "KeePassXC history was lost"
        assert entries["Foreign key"][1]["UserName"] == "foreign-user"
        assert entries["Other entry"][1]["Password"] == "fake-unrelated-value"
        extracted = work / "extracted.txt"
        kpxc("attachment-export", vault, "API Keys/OpenAI interop", "fixture.txt", extracted)
        assert extracted.read_bytes() == attachment.read_bytes(), "Attachment was changed"
        akm("rm", foreign["id"], "--yes")
        assert "Foreign key" not in exported_entries()
        assert "OpenAI interop" in exported_entries(), "Removing foreign entry removed another entry"

        binary_hash = hashlib.sha256(args.akm.read_bytes()).hexdigest()
        print(f"PASS: KeePassXC interchange, explicit upgrade, stable UUIDs, foreign-entry operations, attachment and history preservation. AKM SHA256={binary_hash}")


if __name__ == "__main__":
    main()
