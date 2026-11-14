"""THIRD_PARTY_NOTICES.md is current, and the check fails when it is not (findings FR-25, SEC-04, U4-R1-06, U4-R2-04).

Usage: python tools/test_third_party_notices.py (from the repository root)
"""

import importlib.util
import json
import re
import subprocess
import sys
import tempfile
import tomllib
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
# The independent inventory, written apart from the generator's kinds (finding U4-R2-04). A name net with
# no extension exemption, and a content net: a file that holds a copyright line and is not source code.
BROAD = re.compile(r"(licen[cs]|notice|copying|copyright|patent|third.?party|unlicense|^ofl|authors|contributors|credits|readme|\.spdx)", re.IGNORECASE)
TEST_SOURCE = (".rs", ".js", ".mjs", ".cjs", ".ts", ".c", ".h", ".py", ".kt", ".kts", ".swift", ".java", ".html", ".css", ".scss", ".sh")
COPYRIGHT_LINE = re.compile(r"copyright\s*(?:\(c\)|\u00a9)?\s*\d{4}|\u00a9\s*\d{4}|\(c\)\s*\d{4}|PackageCopyrightText|SPDX-FileCopyrightText", re.IGNORECASE)
# The files that the notices may leave out, with the reason. A key is the start of a file's label
# ("<package> <version> (<kind>): `<path>`"), so a key that ends in "/" covers a folder. A key that
# covers no file fails, so the tables stay exact.
# A file whose name or folder the name net matches, but that holds no notice.
NOT_A_NOTICE = {
    "wry 0.57.1 (crate): `.license_template`": "a rustfmt pattern for source-file headers, not a licence text; wry's licences are LICENSE-APACHE and LICENSE-MIT, which the notices hold",
    "windows 0.62.2 (crate): `src/Windows/Win32/System/DeveloperLicensing/mod.rs`": "Rust bindings of the Windows developer licensing API: the folder names the API, not a licence",
    "windows 0.62.2 (crate): `src/Windows/Win32/Security/LicenseProtection/mod.rs`": "Rust bindings of the Windows licence protection API: the folder names the API, not a licence",
    "windows-sys 0.48.0 (crate): `src/Windows/Win32/System/DeveloperLicensing/mod.rs`": "Rust bindings of the Windows developer licensing API: the folder names the API, not a licence",
    "windows-sys 0.48.0 (crate): `src/Windows/Win32/Security/LicenseProtection/mod.rs`": "Rust bindings of the Windows licence protection API: the folder names the API, not a licence",
    "windows-sys 0.61.2 (crate): `src/Windows/Win32/System/DeveloperLicensing/mod.rs`": "Rust bindings of the Windows developer licensing API: the folder names the API, not a licence",
    "windows-sys 0.61.2 (crate): `src/Windows/Win32/Security/LicenseProtection/mod.rs`": "Rust bindings of the Windows licence protection API: the folder names the API, not a licence",
    "zerocopy 0.8.63 (crate): `ci/check_readme.sh`": "a CI script that checks the README; the README itself is in the notices",
}
UNICODE_HOLDER = "the holder is Unicode, Inc., the crate's own holder, and the crate's LICENSE (Unicode-3.0) is in the notices"
TOOL_HEADER = "the header of a tool setting or a build script; the holder is the package's own authors, and the package's licence texts are in the notices"
# A file that holds a copyright line, is not source code by TEST_SOURCE and is not in the notices.
COPYRIGHT_IN_DATA = {
    "icu_collections 2.4.0 (crate): `tests/data/": f"test inputs of the crate in TOML; {UNICODE_HOLDER}",
    "icu_normalizer 2.4.0 (crate): `tests/data/NormalizationTest.txt`": f"a test input of the crate (Unicode normalization test data); {UNICODE_HOLDER}",
    "icu_properties 2.4.0 (crate): `data/": f"Unicode property data files of the crate; {UNICODE_HOLDER}",
    "idna 1.1.0 (crate): `tests/IdnaTestV2.txt`": "a test input of the crate (Unicode UTS 46 conformance data), which only the crate's tests read; the Unicode licence text is in the notices",
    "urlpattern 0.6.0 (crate): `.rustfmt.toml`": TOOL_HEADER,
    "window-vibrancy 0.8.1 (crate): `.gitignore`": TOOL_HEADER,
    "window-vibrancy 0.8.1 (crate): `examples/tauri/.gitignore`": TOOL_HEADER,
    "window-vibrancy 0.8.1 (crate): `.github/workflows/": TOOL_HEADER,
    "wry 0.57.1 (crate): `.gitignore`": TOOL_HEADER,
    "wry 0.57.1 (crate): `src/android/kotlin/proguard-wry.pro`": TOOL_HEADER,
    "zerocopy 0.8.63 (crate): `Cargo.toml.orig`": TOOL_HEADER,
    "zerocopy 0.8.63 (crate): `clippy.toml`": TOOL_HEADER,
    "zerocopy 0.8.63 (crate): `win-cargo.bat`": TOOL_HEADER,
    "zerocopy-derive 0.8.63 (crate): `Cargo.toml.orig`": TOOL_HEADER,
    "@fontsource-variable/instrument-sans 5.3.0 (npm): `metadata.json`": "package metadata that repeats the font copyright of the package's LICENSE, which the notices hold",
    "@fontsource-variable/literata 5.3.0 (npm): `metadata.json`": "package metadata that repeats the font copyright of the package's LICENSE, which the notices hold",
    "@fontsource/ibm-plex-mono 5.3.0 (npm): `metadata.json`": "package metadata that repeats the font copyright of the package's LICENSE, which the notices hold",
}


def excused(label, table, used):
    """The key of `table` that covers `label`, recorded in `used`; None when no key covers it."""
    for key in table:
        if label.startswith(key):
            used.add(key)
            return key
    return None

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

# Findings U4-R1-04 and U4-R2-02: the snapshot scan accepts exactly the generated entries, and the check fails on any other list.
listed = [line for line in ADDRESSES.read_text(encoding="utf-8").splitlines() if line and not line.startswith("#")]
hits = {hit for line in NOTICES.read_text(encoding="utf-8").split("\n") for hit in snapshot.email_hits(snapshot.views(line)[0])}
case("the address list holds one entry `email <text>` for each e-mail hit of the notices, and nothing else", sorted(listed) == sorted(f"email {hit}" for hit in hits) and len(hits) > 0, f"{len(listed)} listed, {len(hits)} in the file")
with tempfile.TemporaryDirectory(prefix="akm-notices-test-") as folder:
    copy = Path(folder) / "notice-addresses.txt"
    original = ADDRESSES.read_text(encoding="utf-8")
    copy.write_bytes((original + "email " + "@".join(("someone.private", "private-mail.test")) + "\n").encode("utf-8"))
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
        "LICENSE.spdx": "SPDXVersion: SPDX-2.1\nPackageCopyrightText: <text>2020-2021, Fake\n  Authors</text>\n",
        "AUTHORS": "Fake Author\n",
        "README.md": "# Fake\n\nCopyright 2020 Fake Author\n",
        "docs/guide.md": "A fake guide.\n",
        "vendor/zlib/NOTICE.txt": "Fake nested notice of a vendored library.\n",
        "LICENSES/Apache-2.0.txt": "Fake text in a REUSE licences folder.\n",
        "third_party/fast/PATENTS": "Fake patent grant.\n",
        "third_party_licenses/manifest.json": '{"fake": "MIT"}\n',
        "REUSE.toml": "version = 1\n",
        "sbom/fake.spdx.json": '{"spdxVersion": "SPDX-2.3"}\n',
        "src/licence.rs": "// a file named licence is notice material, whatever its extension\n",
        "src/lib.rs": "// Copyright 2020 Fake Author: a source-file header\n",
        "Cargo.toml": "[package]\nname = \"fake-package\"\n",
        "tests/data/input.txt": "plain test input\n",
        "images/logo.png": b"\x89PNG\r\n\x1a\n\x00",
        "node_modules/inner/LICENSE": "Another package, listed by itself.\n",
    }.items():
        (package / relative).parent.mkdir(parents=True, exist_ok=True)
        (package / relative).write_bytes(content if isinstance(content, bytes) else content.encode())
    found = [path.relative_to(package).as_posix() for path in notices.notice_files(package)]
    expected = [
        "AUTHORS", "docs/guide.md", "LICENSE", "LICENSE.spdx", "LICENSES/Apache-2.0.txt", "README.md", "REUSE.toml", "sbom/fake.spdx.json",
        "src/licence.rs", "third_party/fast/PATENTS", "third_party_licenses/manifest.json", "vendor/zlib/NOTICE.txt",
    ]
    case("a synthetic package: every file that is not code or data is notice material, at any depth, and a nested package is not", found == expected, f"found {found}")
    kinds = {name: notices.kind(Path(name))[0] for name in ("src/lib.rs", "Cargo.toml", "tests/data/input.txt", "images/logo.png", "node_modules/inner/LICENSE", "docs/guide.md", "LICENSE.spdx")}
    case("each file has one kind", kinds == {"src/lib.rs": "code or data", "Cargo.toml": "code or data", "tests/data/input.txt": "code or data", "images/logo.png": "code or data", "node_modules/inner/LICENSE": "another package", "docs/guide.md": "notice", "LICENSE.spdx": "notice"}, str(kinds))
    fake = {"name": "fake-package", "version": "1.0.0", "license": "MIT", "authors": ["Fake Author"], "repository": "", "files": notices.notice_files(package), "declared": set(), "base": package}
    texts = notices.collect_texts([("crate", fake)])
    carriers = sorted(carrier for listed in texts.values() for carrier in listed)
    case("the synthetic nested notice is printed with its path", "fake-package 1.0.0 (crate): `vendor/zlib/NOTICE.txt`" in carriers and "Fake nested notice of a vendored library.\n" in texts, str(carriers))
    case("the synthetic README, guide and SPDX document are printed", all(f"fake-package 1.0.0 (crate): `{name}`" in carriers for name in ("README.md", "docs/guide.md", "LICENSE.spdx")), str(carriers))
    case("a package with a licence text gets no standard text", not any("standard" in carrier for carrier in carriers), str(carriers))
    rendered = notices.render([fake], [])
    case("the synthetic SPDX copyright field is in the metadata, on one line", "- fake-package 1.0.0 (crate): `LICENSE.spdx` PackageCopyrightText: 2020-2021, Fake Authors" in rendered.splitlines())
    case("the synthetic manifest author is in the metadata", "- fake-package 1.0.0 (crate): author in the manifest: Fake Author" in rendered.splitlines())

    bare = Path(folder) / "bare-package"
    for relative, content in {"README.md": "# Bare\n", "LICENSE.spdx": "PackageCopyrightText: 2022, Bare\n"}.items():
        (bare / relative).parent.mkdir(parents=True, exist_ok=True)
        (bare / relative).write_bytes(content.encode())
    lone = {"name": "bare-package", "version": "1.0.0", "license": "MIT", "authors": [], "files": notices.notice_files(bare), "declared": set(), "base": bare}
    carriers = [carrier for listed in notices.collect_texts([("npm", lone)]).values() for carrier in listed]
    case("a package whose notice material holds no licence text gets the standard text", any("ships no licence text; the standard MIT text applies" in carrier for carrier in carriers), str(carriers))
    (bare / "legal.txt").write_bytes(b"Fake licence text under another name.\n")
    lone = {**lone, "files": notices.notice_files(bare), "declared": {(bare / "legal.txt").resolve()}}
    carriers = [carrier for listed in notices.collect_texts([("crate", lone)]).values() for carrier in listed]
    case("a crate whose manifest declares its license-file gets no standard text", not any("standard" in carrier for carrier in carriers), str(carriers))
    person = notices.people({"author": {"name": "Ann", "email": "@".join(("ann", "fake.test"))}, "contributors": ["Bo"], "maintainers": [{"name": "Cy", "url": "https://cy.test"}]})
    case("a package.json author, contributor and maintainer are each one person", person == ["Ann <" + "@".join(("ann", "fake.test")) + ">", "Bo", "Cy (https://cy.test)"], str(person))

    odd = Path(folder) / "odd-package"
    for relative, content in {"vendor/blob.xyz": b"\x00\x01\x02", "notes/latin1.txt": "caf\u00e9".encode("latin-1")}.items():
        (odd / relative).parent.mkdir(parents=True, exist_ok=True)
        (odd / relative).write_bytes(content)
        only = {"name": "odd-package", "version": "1.0.0", "license": "MIT", "authors": [], "files": [odd / relative], "declared": set(), "base": odd}
        try:
            notices.collect_texts([("npm", only)])
            stopped = ""
        except SystemExit as error:
            stopped = str(error)
        case(f"a file that is neither code or data nor text stops the tool and names it ({relative})", Path(relative).name in stopped and "notice material" in stopped, stopped)

    sample = "\ufeffA\u200dB\u00a0C\u2028D\r\nE\rF\x0cG\ue000H cafe\u0301 \t end  \n"
    cleaned = notices.plain_text(sample.encode("utf-8"), "sample")
    case("a notice text becomes plain text", cleaned == "AB C\nD\nE\nFGH caf\u00e9 \t end\n" and snapshot.plain_problem(cleaned) is None, repr(cleaned))

    removed = re.sub(r"### Text \d+\n\nShipped by:\n\n- regex-syntax [^\n]*LICENSE-UNICODE`\n\n`{3,}text\n.*?\n`{3,}\n\n?", "", text, count=1, flags=re.DOTALL)
    stale = Path(folder) / "THIRD_PARTY_NOTICES.md"
    stale.write_bytes(removed.encode("utf-8"))
    run = check(stale)
    case("the check fails when the nested Unicode licence is removed from the file", removed != text and run.returncode == 1 and "out of date" in run.stdout, run.stdout + run.stderr)

case("THIRD_PARTY_NOTICES.md is plain text for the public-snapshot scan", snapshot.plain_problem(text) is None, str(snapshot.plain_problem(text)))

# The independent inventory over every shipped package.
carried = {line[2:] for line in licences.splitlines() if line.startswith("- ")}
packages = [("crate", crate) for crate in notices.shipped_crates()] + [("npm", item) for item in notices.shipped_npm()]
named, holding, used = [], [], set()
for kind, package in packages:
    base = package["base"]
    for path in base.rglob("*"):
        relative = path.relative_to(base)
        if not path.is_file() or "node_modules" in relative.parts:
            continue
        label = f"{package['name']} {package['version']} ({kind}): `{relative.as_posix()}`"
        if label in carried:
            continue
        if any(BROAD.search(part) for part in relative.parts):
            if not excused(label, NOT_A_NOTICE, used):
                named.append(label)
            continue
        data = path.read_bytes()
        if path.name.lower().endswith(TEST_SOURCE) or b"\0" in data:
            continue
        if COPYRIGHT_LINE.search(data.decode("utf-8", "replace")) and not excused(label, COPYRIGHT_IN_DATA, used):
            holding.append(label)
case("every file whose name or folder says notice, licence, copyright, authors, credits, README or SPDX is in the notices", not named, f"{len(named)} missing: {named[:8]}")
case("every file that holds a copyright line and is not source code is in the notices, or the test says why not", not holding, f"{len(holding)} missing: {holding[:8]}")
unused = sorted((set(NOT_A_NOTICE) | set(COPYRIGHT_IN_DATA)) - used)
case("every entry of NOT_A_NOTICE and COPYRIGHT_IN_DATA covers a file that the notices leave out", not unused, f"stale: {unused}")


def missing_metadata(notices_text):
    """The manifest authors and SPDX PackageCopyrightText values of the shipped packages that the notices do
    not hold, read without the generator: Cargo.toml with tomllib, package.json with json, SPDX by its tag."""
    lines = notices_text.splitlines()
    missing = []
    for kind, package in packages:
        label = f"{package['name']} {package['version']} ({kind})"
        base = package["base"]
        if kind == "crate":
            people = tomllib.loads((base / "Cargo.toml").read_text(encoding="utf-8")).get("package", {}).get("authors", [])
        else:
            manifest = json.loads((base / "package.json").read_text(encoding="utf-8"))
            people = [person for field in ("author", "contributors", "maintainers") for person in (manifest.get(field) if isinstance(manifest.get(field), list) else [manifest.get(field)]) if person]
        for person in people:
            name = " ".join((person if isinstance(person, str) else person.get("name", "")).split())
            if name and not any(line.startswith(f"- {label}: author in the manifest: ") and name in line for line in lines):
                missing.append(f"{label}: the author {name}")
        for path in base.rglob("*"):
            if ".spdx" not in path.name.lower() or "node_modules" in path.relative_to(base).parts:
                continue
            for raw in path.read_text(encoding="utf-8").splitlines():
                if raw.startswith("PackageCopyrightText:"):
                    value = " ".join(raw.split(":", 1)[1].replace("<text>", "").replace("</text>", "").split())
                    if not any(line.startswith(f"- {label}: `") and f"PackageCopyrightText: {value}" in line for line in lines):
                        missing.append(f"{label}: the SPDX copyright {value}")
    return missing


absent = missing_metadata(text)
case("the independent inventory finds every manifest author and SPDX copyright value in the notices", not absent, f"{len(absent)} missing: {absent[:8]}")

# Finding U4-R2-04: @tauri-apps/plugin-dialog ships LICENSE.spdx with a PackageCopyrightText line.
dialog = [package for kind, package in packages if kind == "npm" and package["name"] == "@tauri-apps/plugin-dialog"]
label = f"@tauri-apps/plugin-dialog {dialog[0]['version']} (npm)" if dialog else "@tauri-apps/plugin-dialog"
line = f"- {label}: `LICENSE.spdx` PackageCopyrightText: 2019-2022, The Tauri Programme in the Commons Conservancy"
case("@tauri-apps/plugin-dialog: its LICENSE.spdx text is in the notices", f"{label}: `LICENSE.spdx`" in carried, label)
case("@tauri-apps/plugin-dialog: its SPDX PackageCopyrightText line is in the metadata", line in text.splitlines(), line)
removed = text.replace(line + "\n", "", 1)
case("the independent inventory reports the plugin-dialog copyright when it is removed", removed != text and any(item.startswith(label) for item in missing_metadata(removed)), str(missing_metadata(removed)[:3]))
with tempfile.TemporaryDirectory(prefix="akm-notices-test-") as folder:
    stale = Path(folder) / "THIRD_PARTY_NOTICES.md"
    stale.write_bytes(removed.encode("utf-8"))
    run = check(stale)
    case("--check fails when the plugin-dialog copyright line is removed", run.returncode == 1 and "out of date" in run.stdout, run.stdout + run.stderr)

# Finding FV-20: the setup program and the installed app carry the licence and the notices. The bundle
# settings name each file as a Tauri resource with its installed name at the top of the install folder.
CONF = Path("desktop/src-tauri/tauri.conf.json")
SHIPPED = {"LICENSE": Path("LICENSE"), "THIRD_PARTY_NOTICES.md": NOTICES}


def missing_from_bundle(conf):
    """The files of SHIPPED that the bundle settings do not carry under their own name, or that do not exist."""
    resources = conf.get("bundle", {}).get("resources", {})
    carried_files = {target: (CONF.parent / source).resolve() for source, target in resources.items()} if isinstance(resources, dict) else {}
    return [name for name, path in SHIPPED.items() if carried_files.get(name) != path.resolve() or not path.is_file()]


bundle_settings = json.loads(CONF.read_text(encoding="utf-8"))
case("the bundle settings carry LICENSE and THIRD_PARTY_NOTICES.md, and both files exist", not missing_from_bundle(bundle_settings), str(missing_from_bundle(bundle_settings)))
lost = json.loads(CONF.read_text(encoding="utf-8"))
lost.get("bundle", {}).get("resources", {}).pop("../../LICENSE", None)
case("the packaging check fails when the bundle settings lose the licence", missing_from_bundle(lost) == ["LICENSE"], str(missing_from_bundle(lost)))

print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
