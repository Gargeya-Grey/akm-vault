"""Create fake cloned-entry fixtures for the native GUI regression, never real keys."""

import argparse
import base64
import copy
import os
from pathlib import Path
import subprocess
import tempfile
import uuid
import xml.etree.ElementTree as ET


PASSWORD = "fake desktop proof correct horse battery staple"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--akm", type=Path, required=True)
    parser.add_argument("--keepassxc", type=Path, required=True)
    parser.add_argument("--directory", type=Path, required=True)
    args = parser.parse_args()
    source = args.directory / "clone-source.kdbx"
    target = args.directory / "clones.kdbx"
    xml_path = args.directory / "fake-clones.xml"
    if any(path.exists() for path in (source, target, xml_path)):
        parser.error("Refusing to replace an existing fixture")
    env = {name: value for name, value in os.environ.items() if not name.upper().startswith("AKM_")}
    env["AKM_PASSWORD"] = PASSWORD
    journal = tempfile.TemporaryDirectory(prefix="akm-fixture-journal-")
    env["AKM_JOURNAL_ROOT"] = str(Path(journal.name) / "journal")

    def run(executable, words, text=None):
        result = subprocess.run([str(executable.resolve()), *map(str, words)], input=text,
                                env=env, text=True, encoding="utf-8", capture_output=True, timeout=90)
        if result.returncode:
            raise AssertionError(f"Fixture command failed: {words[0]}: {result.stderr}")
        return result.stdout

    run(args.akm, ["--vault", source, "init"])
    run(args.akm, ["--vault", source, "add", "Clone original proof", "--secret", "fake-original-kept"])
    root = ET.fromstring(run(args.keepassxc, ["export", "-f", "xml", source], PASSWORD + "\n"))
    groups = root.findall("./Root/Group//Group")
    group = next(group for group in groups if group.findtext("Name") == "API Keys")
    original = group.find("Entry")
    assert original is not None
    clone = copy.deepcopy(original)
    clone.find("UUID").text = base64.b64encode(uuid.uuid4().bytes).decode("ascii")
    original_id = None
    clone_id = None
    for entry in (original, clone):
        for field in entry.findall("String"):
            name, value = field.findtext("Key"), field.find("Value")
            if name == "AKM_ID":
                if entry is original:
                    original_id = value.text
                else:
                    clone_id = value.text
            if entry is clone and name == "Title":
                value.text = "Clone target proof"
            if entry is clone and name == "Password":
                value.text = "fake-clone-before"
    assert original_id and original_id == clone_id
    group.append(clone)
    with xml_path.open("xb") as output:
        output.write(ET.tostring(root, encoding="utf-8", xml_declaration=True))
    run(args.keepassxc, ["import", "-p", "-t", "100", xml_path, target], PASSWORD + "\n" + PASSWORD + "\n")
    run(args.akm, ["--vault", target, "upgrade", "--yes"])
    journal.cleanup()
    print("Prepared fake native GUI clone fixture with distinct native UUIDs and identical AKM_ID fields.")


if __name__ == "__main__":
    main()
