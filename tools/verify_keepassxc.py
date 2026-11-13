"""Interchange regression against a real KeePassXC executable, using fake keys only."""

import argparse
import hashlib
import json
import os
import re
import shutil
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

        def akm(*words, success=True, stdin=None, target=None):
            result = subprocess.run(
                [str(args.akm.resolve()), "--vault", str(target or vault), *words],
                env=env, text=True, encoding="utf-8", capture_output=True, timeout=90, input=stdin,
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

        def exported_xml(target=None):
            return ET.fromstring(kpxc("export", "-f", "xml", target or vault))

        def group_of(entry_uuid, target=None):
            """Name of the group that holds the entry, or None when the entry is absent."""
            xml = exported_xml(target)
            for group in xml.find("./Root/Group").iter("Group"):
                for entry in group.findall("Entry"):
                    if entry.findtext("UUID") == entry_uuid:
                        return group.findtext("Name")
            return None

        def tombstones(target=None):
            return {item.findtext("UUID") for item in exported_xml(target).findall("./Root/DeletedObjects/DeletedObject")}

        def exported_entries(target=None):
            xml = exported_xml(target)
            entries = {}
            root = xml.find("./Root/Group")
            assert root is not None
            for group in root.iter("Group"):
                for entry in group.findall("Entry"):
                    fields = {field.findtext("Key"): field.findtext("Value") for field in entry.findall("String")}
                    entries[fields.get("Title")] = (entry, fields)
            return entries

        akm("init", "--name", "Interchange regression")
        akm("add", "OpenAI interop", "--project", "demo", "--env", "dev", "--secret-stdin", "--env-var", "OPENAI_API_KEY", stdin="fake-original-value\n")
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

        akm("add", "Must not be saved", "--secret-stdin", stdin="fake\n", success=False)
        assert vault.read_bytes() == before_upgrade, "Refused upgrade changed the original vault"
        assert not list(work.glob("*.tmp")), "Refused write left a temporary file"
        akm("upgrade", "--yes")
        assert int.from_bytes(vault.read_bytes()[8:12], "little") == 0x00040001
        assert Path(f"{vault}.bak").read_bytes() == before_upgrade, "Upgrade lost the original backup"

        keys = json.loads(akm("list", "--json").stdout)
        assert next(key for key in keys if key["title"] == "OpenAI interop")["id"] == original_id
        foreign = next(key for key in keys if key["title"] == "Foreign key")
        assert akm("get", foreign["id"], "--field", "secret").stdout.strip() == "fake-foreign-value"
        akm("add", "After interchange", "--secret-stdin", stdin="fake-new-value\n")
        entries = exported_entries()
        assert entries["OpenAI interop"][1]["Notes"] == "Edited in the real KeePassXC"
        assert entries["OpenAI interop"][0].find("History/Entry") is not None, "KeePassXC history was lost"
        assert entries["Foreign key"][1]["UserName"] == "foreign-user"
        assert entries["Other entry"][1]["Password"] == "fake-unrelated-value"
        extracted = work / "extracted.txt"
        kpxc("attachment-export", vault, "API Keys/OpenAI interop", "fixture.txt", extracted)
        assert extracted.read_bytes() == attachment.read_bytes(), "Attachment was changed"
        # Unit 2: rotation, native history, Recycle Bin, restore, purge and stale-copy merges.
        def history_of(title):
            return [
                {field.findtext("Key"): field.findtext("Value") for field in item.findall("String")}
                for item in exported_entries()[title][0].findall("History/Entry")
            ]

        shown = akm("get", foreign["id"]).stdout
        assert "Secret changed: unknown" in shown, shown  # a KeePassXC-written key has no known time
        shown = akm("get", original_id).stdout
        assert re.search(r"Secret changed:\s+\d{4}-", shown), shown  # AKM add set the time
        history_before = history_of("OpenAI interop")
        assert history_before, "KeePassXC history is missing before the rotation"
        akm("rotate", original_id, "--secret-stdin", stdin="fake-rotated-value\n")
        rotated = exported_entries()["OpenAI interop"][1]
        assert rotated["Password"] == "fake-rotated-value"
        assert re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ", rotated["AKM_SECRET_CHANGED"]), rotated
        history_after = history_of("OpenAI interop")
        assert len(history_after) == len(history_before) + 1, "Rotation did not add one native history version"
        # KeePassXC lists the newest history version first.
        assert history_after[1:] == history_before, "Rotation changed earlier history versions"
        assert history_after[0]["Password"] == "fake-original-value", "The old secret is not in the history"
        assert history_after[0]["Notes"] == "Edited in the real KeePassXC", "The history version is not the pre-rotation state"
        akm("rotate", original_id, "--secret-stdin", stdin="fake-rotated-value\n", success=False)
        assert len(history_of("OpenAI interop")) == len(history_after), "A refused rotation added history"
        kpxc("attachment-export", vault, "API Keys/OpenAI interop", "fixture.txt", work / "after-rotate.txt")
        assert (work / "after-rotate.txt").read_bytes() == attachment.read_bytes(), "Rotation changed the attachment"
        shown = akm("get", original_id).stdout
        assert re.search(r"Secret changed:\s+\d{4}-", shown), shown
        assert "fake-rotated-value" not in akm("history", original_id).stdout, "history printed a secret"
        assert "fake-original-value" not in akm("list").stdout

        xml_uuid = exported_entries()["Foreign key"][0].findtext("UUID")
        akm("rm", foreign["id"], "--yes")
        xml = exported_xml()
        bin_uuid = xml.findtext("./Meta/RecycleBinUUID")
        assert xml.findtext("./Meta/RecycleBinEnabled") == "True", "The Recycle Bin is not turned on"
        assert group_of(xml_uuid) == "Recycle Bin", "rm did not move the key to the Recycle Bin"
        bin_group = next(g for g in xml.find("./Root/Group").iter("Group") if g.findtext("Name") == "Recycle Bin")
        assert bin_group.findtext("UUID") == bin_uuid
        assert "Foreign key" in akm("bin").stdout and "Foreign key" not in akm("list").stdout
        assert "OpenAI interop" in exported_entries(), "Removing foreign entry removed another entry"
        akm("restore", foreign["id"])
        assert group_of(xml_uuid) == "API Keys", "restore did not return the key to its group"
        assert akm("get", foreign["id"], "--field", "secret").stdout.strip() == "fake-foreign-value"
        assert exported_entries()["Foreign key"][0].findtext("UUID") == xml_uuid, "restore changed the UUID"

        stale_live = work / "stale-live.kdbx"
        shutil.copyfile(vault, stale_live)
        akm("rm", foreign["id"], "--yes")
        stale_bin = work / "stale-bin.kdbx"
        shutil.copyfile(vault, stale_bin)
        akm("purge", foreign["id"], "--yes")
        assert "Foreign key" not in exported_entries(), "purge left the key in the vault"
        assert xml_uuid in tombstones(), "purge wrote no DeletedObjects record"
        assert "OpenAI interop" in exported_entries()
        akm("get", foreign["id"], success=False)

        # Stale-copy merges. A purge by AKM must behave exactly like a permanent delete by KeePassXC
        # itself: the same DeletedObjects record, and the same merge result in both directions.
        # KeePassXC 2.8.0 `merge` does not apply DeletedObjects in its default mode, so it re-adds a
        # key that its own delete removed. The kdbxweb tool checks a reader that does apply them.
        control = work / "kpxc-purged.kdbx"
        shutil.copyfile(stale_bin, control)
        kpxc("rm", control, "Recycle Bin/Foreign key")
        assert "Foreign key" not in exported_entries(control) and xml_uuid in tombstones(control)

        def merged_state(target):
            return sorted(exported_entries(target)), xml_uuid in tombstones(target)

        for name, stale in (("live", stale_live), ("bin", stale_bin)):
            outcomes = {}
            for label, purged in (("akm", vault), ("kpxc", control)):
                into_stale = work / f"{label}-into-stale-{name}.kdbx"
                shutil.copyfile(stale, into_stale)
                kpxc("merge", "-s", into_stale, purged)
                into_purged = work / f"{label}-into-purged-{name}.kdbx"
                shutil.copyfile(purged, into_purged)
                kpxc("merge", "-s", into_purged, stale)
                outcomes[label] = (merged_state(into_stale), merged_state(into_purged))
                assert exported_entries(into_stale)["OpenAI interop"][1]["Password"] == "fake-rotated-value"
                if label == "akm":
                    listed = json.loads(akm("list", "--json", target=into_purged).stdout)
                    assert "OpenAI interop" in [key["title"] for key in listed], "AKM cannot read the merged vault"
            assert outcomes["akm"][1][1] is True, "the tombstone was lost in a merge"
            assert outcomes["akm"][0][0] == outcomes["kpxc"][0][0], f"stale {name} copy: merge result differs from a KeePassXC delete"
            assert outcomes["akm"][1][0] == outcomes["kpxc"][1][0], f"purged <- stale {name}: merge result differs from a KeePassXC delete"
            print(f"merge, stale {name} copy: same as a KeePassXC delete (keeps Foreign key: {'Foreign key' in outcomes['akm'][0][0]} / {'Foreign key' in outcomes['akm'][1][0]})")


        binary_hash = hashlib.sha256(args.akm.read_bytes()).hexdigest()
        print(f"PASS: KeePassXC interchange, explicit upgrade, stable UUIDs, foreign-entry operations, attachment and history preservation, rotation, Recycle Bin, restore, purge and stale-copy merges in both directions. AKM SHA256={binary_hash}")


if __name__ == "__main__":
    main()
