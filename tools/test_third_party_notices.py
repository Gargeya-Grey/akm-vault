"""THIRD_PARTY_NOTICES.md is current, and the check fails when it is not (finding FR-25, SEC-04).

Usage: python tools/test_third_party_notices.py (from the repository root)
"""

import importlib.util
import re
import subprocess
import sys
import tempfile
from pathlib import Path

GENERATOR = [sys.executable, "tools/generate_third_party_notices.py"]
NOTICES = Path("THIRD_PARTY_NOTICES.md")
ADDRESSES = Path("tools/public_snapshot/notice-addresses.txt")
MPL_CRATES = {"cssparser", "cssparser-macros", "selectors", "dtoa-short", "option-ext"}
spec = importlib.util.spec_from_file_location("notices", Path(__file__).resolve().parent / "generate_third_party_notices.py")
notices = importlib.util.module_from_spec(spec)
spec.loader.exec_module(notices)
spec = importlib.util.spec_from_file_location("snapshot", Path(__file__).resolve().parent / "make_public_snapshot.py")
snapshot = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = snapshot
spec.loader.exec_module(snapshot)
# An independent, wider net than the generator's own rule: any file or folder whose name says licence,
# notice, copying, copyright, patent or third party, at any depth (finding U4-R1-06).
BROAD = re.compile(r"(licen[cs]|notice|copying|copyright|patent|third.?party|unlicense|^ofl)", re.IGNORECASE)
CODE = (".rs", ".js", ".mjs", ".cjs", ".ts", ".map", ".json", ".toml", ".html", ".css", ".py", ".c", ".h", ".spdx")
# A licence-like file that the notices may leave out, with the reason.
NOT_A_NOTICE = {
    "wry 0.57.1 (crate): `.license_template`": "a rustfmt pattern for source-file headers, not a licence text; wry's licences are LICENSE-APACHE and LICENSE-MIT, which the notices hold",
}

failures = 0


def case(name, ok, detail=""):
    global failures
    print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if not ok:
        failures += 1
        if detail:
            print(f"     {detail}")


def check(output, addresses=ADDRESSES):
    return subprocess.run(GENERATOR + ["--check", "--output", str(output), "--addresses", str(addresses)], capture_output=True, text=True)


run = check(NOTICES)
case("the committed file and its address list are current", run.returncode == 0, run.stdout + run.stderr)

# Finding U4-R1-04: the snapshot scan accepts exactly the generated addresses, and the check fails on any other list.
listed = [line for line in ADDRESSES.read_text(encoding="utf-8").splitlines() if line and not line.startswith("#")]
shaped = {match.group().casefold() for match in snapshot.EMAIL.finditer(NOTICES.read_text(encoding="utf-8"))}
case("the address list holds every e-mail-shaped string of the notices once, and nothing else", sorted(line.casefold() for line in listed) == sorted(shaped) and len(shaped) > 0, f"{len(listed)} listed, {len(shaped)} in the file")
with tempfile.TemporaryDirectory(prefix="akm-notices-test-") as folder:
    copy = Path(folder) / "notice-addresses.txt"
    original = ADDRESSES.read_text(encoding="utf-8")
    copy.write_bytes((original + "@".join(("someone.private", "private-mail.test")) + "\n").encode("utf-8"))
    run = check(NOTICES, copy)
    case("an address list with an added address fails the check", run.returncode == 1 and "notice-addresses.txt is out of date" in run.stdout, run.stdout + run.stderr)
    copy.write_bytes(original.replace(listed[0] + "\n", "", 1).encode("utf-8"))
    run = check(NOTICES, copy)
    case("an address list without one of its addresses fails the check", run.returncode == 1 and "notice-addresses.txt is out of date" in run.stdout, run.stdout + run.stderr)
    run = check(NOTICES, Path(folder) / "missing.txt")
    case("a missing address list fails the check", run.returncode == 1, run.stdout + run.stderr)

text = NOTICES.read_text(encoding="utf-8")
with tempfile.TemporaryDirectory(prefix="akm-notices-test-") as folder:
    stale = Path(folder) / "THIRD_PARTY_NOTICES.md"
    stale.write_bytes(text.replace("| react | ", "| react-changed | ", 1).encode("utf-8"))
    run = check(stale)
    case("an out-of-date file fails the check", run.returncode == 1 and "out of date" in run.stdout, run.stdout + run.stderr)
    run = check(Path(folder) / "missing.md")
    case("a missing file fails the check", run.returncode == 1, run.stdout + run.stderr)
    licences = text.split("## Licence texts", 1)[1]
    stale.write_bytes((text.split("## Licence texts", 1)[0] + "## Licence texts" + licences.replace("Permission is hereby granted", "Permission is granted", 1)).encode("utf-8"))
    run = check(stale)
    case("a changed licence text fails the check", run.returncode == 1 and "out of date" in run.stdout, run.stdout + run.stderr)

licences = text.split("## Licence texts", 1)[1]
carriers = {line[2:].split(" (", 1)[0] for line in licences.splitlines() if line.startswith("- ") and " (crate)" in line or line.startswith("- ") and " (npm)" in line}
tables = text.split("## Licence texts", 1)[0]
rows = set()
for heading in ("## Rust crates", "## npm packages"):
    table = tables.split(heading, 1)[1].split("\n## ", 1)[0]
    for line in table.splitlines():
        cells = [part.strip() for part in line.split("|")]
        if line.startswith("| ") and len(cells) > 3 and cells[1] not in ("Crate", "Package"):
            rows.add(f"{cells[1].replace(chr(92), '')} {cells[2]}")
missing = sorted(rows - carriers)
case(f"every shipped package ({len(rows)}) carries at least one licence text", not missing and len(rows) > 300, f"no text for {missing[:10]}")
case("the MPL-2.0 text is in the licence texts", "Mozilla Public License Version 2.0" in licences)
case("the SIL Open Font License text is in the licence texts", "SIL OPEN FONT LICENSE" in licences.upper())
case("the licence texts hold no control character but the tab", not any(ord(ch) < 32 and ch not in "\t\n" for ch in licences))

section = text.split("## MPL-2.0 crates", 1)[1].split("\n## ", 1)[0]
rows = {line.split("|")[1].strip() for line in section.splitlines() if line.startswith("| ") and not line.startswith("| Crate") and not line.startswith("|---")}
case("the MPL-2.0 section names exactly the five crates of SEC-04", rows == MPL_CRATES, f"found {sorted(rows)}")
case("every MPL-2.0 crate has a source archive address", all(f"/{name}/{name}-" in section for name in MPL_CRATES))

# Finding U4-R1-06: a licence below a package's root. regex-syntax ships its Unicode tables under the Unicode licence.
case(
    "regex-syntax carries its nested Unicode licence (src/unicode_tables/LICENSE-UNICODE)",
    re.search(r"^- regex-syntax \S+ \(crate\): `src/unicode_tables/LICENSE-UNICODE`$", licences, re.MULTILINE) is not None
    and "UNICODE, INC. LICENSE AGREEMENT" in licences,
)
with tempfile.TemporaryDirectory(prefix="akm-notices-test-") as folder:
    package = Path(folder) / "fake-package-1.0.0"
    for relative, content in {
        "LICENSE": "Fake root licence.\n",
        "vendor/zlib/NOTICE.txt": "Fake nested notice of a vendored library.\n",
        "LICENSES/Apache-2.0.txt": "Fake text in a REUSE licences folder.\n",
        "third_party/fast/PATENTS": "Fake patent grant.\n",
        "src/licence.rs": "// code, not a licence text\n",
        "node_modules/inner/LICENSE": "Another package, listed by itself.\n",
    }.items():
        (package / relative).parent.mkdir(parents=True, exist_ok=True)
        (package / relative).write_bytes(content.encode())
    found = [path.relative_to(package).as_posix() for path in notices.licence_files(package)]
    expected = ["LICENSE", "LICENSES/Apache-2.0.txt", "third_party/fast/PATENTS", "vendor/zlib/NOTICE.txt"]
    case("a synthetic package's nested notice, licence folder and patent file are found, and a nested package is not", found == expected, f"found {found}")
    fake = {"name": "fake-package", "version": "1.0.0", "license": "MIT", "authors": [], "files": notices.licence_files(package), "base": package}
    texts = notices.collect_texts([("crate", fake)])
    carriers = sorted(carrier for listed in texts.values() for carrier in listed)
    case("the synthetic nested notice is printed with its path", "fake-package 1.0.0 (crate): `vendor/zlib/NOTICE.txt`" in carriers and "Fake nested notice of a vendored library.\n" in texts, str(carriers))

    removed = re.sub(r"### Text \d+\n\nShipped by:\n\n- regex-syntax [^\n]*LICENSE-UNICODE`\n\n`{3,}text\n.*?\n`{3,}\n\n?", "", text, count=1, flags=re.DOTALL)
    stale = Path(folder) / "THIRD_PARTY_NOTICES.md"
    stale.write_bytes(removed.encode("utf-8"))
    run = check(stale)
    case("the check fails when the nested Unicode licence is removed from the file", removed != text and run.returncode == 1 and "out of date" in run.stdout, run.stdout + run.stderr)

# The wider net over every shipped package: each licence-like text file at any depth is a carrier.
carried = {line[2:] for line in licences.splitlines() if line.startswith("- ")}
missed = []
for kind, package in [("crate", crate) for crate in notices.shipped_crates()] + [("npm", item) for item in notices.shipped_npm()]:
    base = package["base"]
    for path in base.rglob("*"):
        relative = path.relative_to(base)
        if not path.is_file() or "node_modules" in relative.parts or path.name.lower().endswith(CODE):
            continue
        if BROAD.search(path.name) or any(BROAD.fullmatch(part) or BROAD.match(part) for part in relative.parts[:-1]):
            label = f"{package['name']} {package['version']} ({kind}): `{relative.as_posix()}`"
            if label not in carried and label not in NOT_A_NOTICE:
                missed.append(label)
case("every licence-like file of every shipped package, at any depth, is in the notices", not missed, f"{len(missed)} missing: {missed[:8]}")

print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
