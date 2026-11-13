"""Write THIRD_PARTY_NOTICES.md from the lock files, or check that it is current (finding FR-25, SEC-04).

Usage: python tools/generate_third_party_notices.py            (write THIRD_PARTY_NOTICES.md and its address list)
       python tools/generate_third_party_notices.py --check    (exit 1 when either file is out of date)

Rust: `cargo metadata --format-version 1 --offline --locked --filter-platform x86_64-pc-windows-msvc`.
A crate is shipped when the workspace members reach it through normal dependencies only. Build
dependencies and dev dependencies are left out, and so are the workspace crates themselves.
npm: desktop/package-lock.json, every package that is not marked `dev`.

Files (findings U4-R1-06, U4-R2-04): every file of a shipped package is one of three kinds.

- a file of another package: a file below a nested node_modules, which the lock file lists by itself;
- notice material: a file whose name starts LICENSE, LICENCE, COPYING, NOTICE, UNLICENSE, COPYRIGHT,
  OFL, PATENT, THIRD-PARTY (and its spellings), AUTHORS, CONTRIBUTORS, CREDITS or README, a file
  named REUSE.toml, a file whose name holds `.spdx` (an SPDX document, also in JSON), and every file
  in a folder named LICENSES, LICENCES or THIRD-PARTY (and its spellings), at any depth and whatever
  its extension. And every file that CODE_OR_DATA does not name: a README, a change log or any other
  document is notice material too;
- code or data: a file that CODE_OR_DATA names by its extension, its name or a folder above it,
  each with the reason it holds no notice (source code, a build or tool setting, data that a program
  or a test reads, an image or a font).

Notice material is included, once for each distinct text, with the list of the packages and paths
that carry it. It must be UTF-8 text without a NUL byte. A file that is neither code or data nor such
text stops the tool with its name, so a person reads it and decides its kind. The folder is the
unpacked crate in the Cargo registry, or the package in desktop/node_modules. So the crates must be
fetched (any cargo build does it) and `npm ci` must have run in desktop. The tool cannot tell which
files a build compiles, so it takes every one: a text can cover code that AKM does not ship (for
example a Linux-only backend or test images). A package that ships no licence text (a file whose
name starts LICENSE, LICENCE, COPYING, NOTICE, UNLICENSE, COPYRIGHT, OFL, PATENT or THIRD-PARTY,
other than an SPDX document, or a file in a licence folder) also gets the standard text of each
licence in its expression, from tools/licence_texts/<SPDX id>.txt (SPDX licence list, version in that
folder's README), and the file says so for that package.

Metadata: the file also lists the authors that each manifest declares (Cargo `authors`; package.json
`author`, `contributors` and `maintainers`) and the PackageCopyrightText, FileCopyrightText,
PackageOriginator and PackageSupplier fields of each SPDX tag-value document, one line each.

The text is the file's text as plain text (PLAIN TEXT in tools/make_public_snapshot.py): line ends
made LF (a lone CR too), a leading byte order mark and every other control, format, private-use or
unassigned character removed except the tab, every other space made U+0020, a line or paragraph
separator made LF, the text put in Unicode normalization form NFC, and trailing spaces on each line
removed. The file says this once.

Address list: tools/public_snapshot/notice-addresses.txt holds one entry `email <text>` for each
e-mail hit of the generated file, once, sorted: the hits of the e-mail rule of
tools/make_public_snapshot.py, read the way the scan reads them. The snapshot scan accepts exactly
these texts (THE EXCEPTIONS in that tool; findings U4-R1-04, U4-R2-02). --check fails when the list
is not the list of the file that the lock files and the packages produce, so a hand-added entry fails.
"""

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import unicodedata
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from make_public_snapshot import email_hits, views  # noqa: E402  (one e-mail rule for the scan and this list)

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "THIRD_PARTY_NOTICES.md"
ADDRESSES = ROOT / "tools" / "public_snapshot" / "notice-addresses.txt"
STANDARD = Path(__file__).resolve().parent / "licence_texts"
NODE_MODULES = ROOT / "desktop" / "node_modules"
TARGET = "x86_64-pc-windows-msvc"
MPL = re.compile(r"\bMPL-2\.0\b")
NOTICE_NAME = re.compile(r"^(licen[cs]e|copying|notice|unlicense|copyright|ofl|patent|third[-_]?party|authors|contributors|credits|readme|reuse\.toml$)", re.IGNORECASE)
LICENCE_TEXT = re.compile(r"^(licen[cs]e|copying|notice|unlicense|copyright|ofl|patent|third[-_]?party)", re.IGNORECASE)
LICENCE_FOLDER = re.compile(r"^(licen[cs]es|third[-_]?party(?:[-_]?(?:notices|licen[cs]es))?)$", re.IGNORECASE)
# Code or data: the kinds of file that hold no notice, each with its reason. A copyright line in source
# code, a setting or data is a file header, which this file leaves out (its Limits say so). Every file
# that this table does not name is notice material.
SOURCE = "source code or a script"
SETTING = "a build, package, version-control or tool setting"
OLD_COPY = "an old copy of a source file or a manifest"
DATA = "data that a program or a test reads, or program output that a test compares"
MEDIA = "an image or a font, whose licence is in the package's licence files"
CODE_OR_DATA = {
    "extension": {
        **dict.fromkeys(("rs", "js", "mjs", "cjs", "ts", "mts", "cts", "jsx", "tsx", "c", "h", "py", "sh", "bat", "ps1", "kt", "kts", "swift", "java", "gradle", "pro", "natvis", "html", "css", "scss", "in", "nix"), SOURCE),
        **dict.fromkeys(("toml", "json", "lock", "yml", "yaml", "orig", "std", "xml", "entitlements"), SETTING),
        "old": OLD_COPY,
        **dict.fromkeys(("stderr", "beta", "x86-64", "mca", "blb", "bin", "data", "dat", "fst", "gz", "zz", "brotli", "raw", "ndjson", "snap", "map", "proptest-regressions", "wasm", "lib", "dll"), DATA),
        **dict.fromkeys(("png", "jpeg", "jpg", "gif", "ico", "icns", "bmp", "svg", "tiff", "woff", "woff2", "ttf", "otf", "eot"), MEDIA),
    },
    "name": {
        **dict.fromkeys((".cargo-ok", ".gitignore", ".gitattributes", ".editorconfig", ".editorconifg", ".envrc", ".ignore", ".nojekyll", ".npmignore", "codeowners", ".license_template"), SETTING),
        **dict.fromkeys(("pre-commit", "pre-push", "pre-rebase", "makefile"), SOURCE),
    },
    "folder": {
        **dict.fromkeys(("tests", "test", "testdata", "test-data", "benches", "bench", "fuzz", "data"), DATA),
        ".github": SETTING,
    },
}
# Plain text keeps the tab and the printable characters. These characters become a space or a line end;
# every other character that is not printable is removed.
SPACE_LIKE = {"Zs": " ", "Zl": "\n", "Zp": "\n"}
SPDX_FIELDS = ("PackageCopyrightText", "FileCopyrightText", "PackageOriginator", "PackageSupplier")
SPDX_ID = re.compile(r"[A-Za-z0-9.+-]+")
EXPRESSION_WORDS = {"AND", "OR", "WITH"}


def shipped_crates():
    raw = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--offline", "--locked", "--filter-platform", TARGET],
        cwd=ROOT,
        capture_output=True,
        check=True,
    ).stdout.decode("utf-8")
    meta = json.loads(raw)
    packages = {package["id"]: package for package in meta["packages"]}
    nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}
    members = set(meta["workspace_members"])
    seen = set()
    stack = list(members)
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        for dep in nodes[current]["deps"]:
            if any(kind["kind"] is None for kind in dep["dep_kinds"]):
                stack.append(dep["pkg"])
    crates = []
    for identifier in seen - members:
        package = packages[identifier]
        folder = Path(package["manifest_path"]).parent
        files = notice_files(folder)
        declared = set()
        if package.get("license_file"):
            extra = folder / package["license_file"]
            if extra.is_file():
                declared.add(extra.resolve())
                if extra.resolve() not in {path.resolve() for path in files}:
                    files.append(extra)
        crates.append(
            {
                "name": package["name"],
                "version": package["version"],
                "license": package["license"] or "(no licence field; read the licence file in the package)",
                "repository": package["repository"] or "",
                "authors": package.get("authors") or [],
                "files": files,
                "declared": declared,
                "base": folder,
            }
        )
    return sorted(crates, key=lambda crate: (crate["name"], crate["version"]))


def shipped_npm():
    lock = json.loads((ROOT / "desktop" / "package-lock.json").read_text(encoding="utf-8"))
    packages = []
    for key, entry in lock["packages"].items():
        if not key or entry.get("dev"):
            continue
        name = key.rsplit("node_modules/", 1)[-1]
        folder = ROOT / "desktop" / key
        installed = folder / "package.json"
        if not installed.is_file():
            raise SystemExit(f"{installed} is missing. Run `npm ci` in desktop first, then run this tool again.")
        manifest = json.loads(installed.read_text(encoding="utf-8"))
        if manifest.get("version") != entry["version"]:
            raise SystemExit(f"desktop/{key} is {manifest.get('version')}, the lock file says {entry['version']}. Run `npm ci` in desktop.")
        packages.append(
            {
                "name": name,
                "version": entry["version"],
                "license": entry.get("license") or "(none in the lock file)",
                "authors": people(manifest),
                "files": notice_files(folder),
                "declared": set(),
                "base": folder,
            }
        )
    return sorted(packages, key=lambda package: (package["name"], package["version"]))


def people(manifest):
    """The author, contributors and maintainers of a package.json, each as npm writes a person."""
    found = []
    for field in ("author", "contributors", "maintainers"):
        value = manifest.get(field)
        for person in value if isinstance(value, list) else [value] if value else []:
            if isinstance(person, str):
                found.append(person)
            elif isinstance(person, dict) and person.get("name"):
                found.append(person["name"] + "".join(f" {marks[0]}{person[key]}{marks[1]}" for key, marks in (("email", "<>"), ("url", "()")) if person.get(key)))
    return found


def kind(relative):
    """The kind of the file at `relative` inside its package, with the reason: "another package",
    "notice" or "code or data" (the module text says how)."""
    parts = [part.lower() for part in relative.parts]
    name, folders = parts[-1], parts[:-1]
    if "node_modules" in folders:
        return "another package", "a nested node_modules holds packages that the lock file lists by themselves"
    if NOTICE_NAME.match(name) or ".spdx" in name or any(LICENCE_FOLDER.match(folder) for folder in folders):
        return "notice", "its name or a folder above it says notice"
    extension = name.rsplit(".", 1)[1] if "." in name.lstrip(".") else None
    if extension in CODE_OR_DATA["extension"]:
        return "code or data", CODE_OR_DATA["extension"][extension]
    if name in CODE_OR_DATA["name"]:
        return "code or data", CODE_OR_DATA["name"][name]
    for folder in folders:
        if folder in CODE_OR_DATA["folder"]:
            return "code or data", CODE_OR_DATA["folder"][folder]
    return "notice", "CODE_OR_DATA does not name it"


def notice_files(folder):
    """Every notice file of the package in `folder`, at any depth, sorted by path."""
    found = [path for path in folder.rglob("*") if path.is_file() and kind(path.relative_to(folder))[0] == "notice"]
    return sorted(found, key=lambda path: path.relative_to(folder).as_posix().lower())


def is_licence_text(package, path):
    """A licence text is the manifest's license-file, or a notice file whose name or folder says licence,
    other than an SPDX document."""
    if path.resolve() in package["declared"]:
        return True
    parts = [part.lower() for part in Path(os.path.relpath(path, package["base"])).parts]
    return (LICENCE_TEXT.match(parts[-1]) is not None and ".spdx" not in parts[-1]) or any(LICENCE_FOLDER.match(part) for part in parts[:-1])


def plain_text(raw, path):
    """The text of a notice file as plain text (the module text says how), or stop with the file's name."""
    if b"\0" in raw:
        raise SystemExit(f"{path} is notice material, because CODE_OR_DATA does not name it, but it holds a NUL byte. Read it, then add its kind to CODE_OR_DATA with the reason.")
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        raise SystemExit(f"{path} is notice material, because CODE_OR_DATA does not name it, but it is not UTF-8 text. Read it, then add its kind to CODE_OR_DATA with the reason.") from None
    lines = []
    for line in text.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
        if not line.replace("\t", " ").isprintable():
            line = "".join(c if c == "\t" or c.isprintable() else SPACE_LIKE.get(unicodedata.category(c), "") for c in line)
        lines.append(line)
    text = unicodedata.normalize("NFC", "\n".join(lines))
    return "\n".join(line.rstrip() for line in text.split("\n")).strip("\n") + "\n"


def one_line(value):
    return " ".join(plain_text(value.encode("utf-8"), "a manifest value").split())


def spdx_fields(text):
    """(tag, value) of the copyright, originator and supplier fields of an SPDX tag-value document."""
    found = []
    lines = text.split("\n")
    index = 0
    while index < len(lines):
        tag, colon, value = lines[index].partition(":")
        index += 1
        if not colon or tag.strip() not in SPDX_FIELDS:
            continue
        value = value.strip()
        if value.startswith("<text>"):
            block = [value[len("<text>"):]]
            while "</text>" not in block[-1] and index < len(lines):
                block.append(lines[index])
                index += 1
            value = "\n".join(block).split("</text>", 1)[0]
        found.append((tag.strip(), " ".join(value.split())))
    return found


def licence_ids(expression):
    return sorted({word for word in SPDX_ID.findall(expression) if word not in EXPRESSION_WORDS})


def standard_text(identifier):
    path = STANDARD / f"{identifier}.txt"
    if not path.is_file():
        raise SystemExit(f"{path} is missing: a shipped package without a licence file uses {identifier}. Add the SPDX text.")
    return plain_text(path.read_bytes(), path)


def collect_texts(packages):
    """Map each distinct text to the packages and files that carry it."""
    texts = {}
    for kind, package in packages:
        label = f"{package['name']} {package['version']} ({kind})"
        for path in package["files"]:
            shown = Path(os.path.relpath(path, package["base"])).as_posix()
            texts.setdefault(plain_text(path.read_bytes(), path), []).append(f"{label}: `{shown}`")
        if not any(is_licence_text(package, path) for path in package["files"]):
            authors = ", ".join(one_line(author) for author in package["authors"] if author) or "none declared"
            for identifier in licence_ids(package["license"]):
                texts.setdefault(standard_text(identifier), []).append(
                    f"{label}: ships no licence text; the standard {identifier} text applies. Authors in its manifest: {authors}"
                )
    return texts


def fence(text):
    longest = max((len(run) for run in re.findall(r"`+", text)), default=0)
    return "`" * max(3, longest + 1)


def cell(text):
    return text.replace("|", "\\|")


def render(crates, npm):
    mpl = [crate for crate in crates if MPL.search(crate["license"])]
    out = []
    out.append("# Third-party notices\n")
    out.append(
        "AKM is licensed under the MIT licence ([LICENSE](LICENSE)). It ships the dependencies below. "
        "This file is generated by `tools/generate_third_party_notices.py` from `Cargo.lock` "
        f"(through `cargo metadata`, for the `{TARGET}` target) and `desktop/package-lock.json`. "
        "Do not edit it by hand. `python tools/generate_third_party_notices.py --check` fails when it is out of date.\n"
    )
    out.append(
        "**What this file holds.** Each shipped dependency with its version and its licence expression; "
        "the authors that the manifests declare and the copyright fields of the SPDX documents that the "
        "packages ship; and at the end the text of every file of those packages that is not code or data "
        "(`CODE_OR_DATA` in the tool names those kinds, each with its reason): licence, notice, copying, "
        "copyright, patent, SPDX, authors and credits files, READMEs, change logs and every other document, "
        "at any depth of the package and with its path there. A file below a package's root can cover code "
        "that AKM does not compile (a backend for another system, test data); the tool cannot tell, so it "
        "includes every one. A text that several packages ship is printed once, with the list of those "
        "packages. A package that ships no licence text is listed with the standard text of its licence and "
        "the authors that its manifest names.\n"
    )
    out.append(
        "**Limits.** The texts come from the packages as published, as plain text: line endings are LF, "
        "trailing spaces are removed, control, format, private-use and unassigned characters other than the "
        "tab are removed (for example a form feed between licence sections or a zero-width joiner), every "
        "other space is U+0020, and the text is in Unicode normalization form NFC. A copyright line that a "
        "package keeps only in its source code, its settings or its data files is not here. No lawyer has "
        "reviewed this file, and it is not legal advice.\n"
    )
    out.append(f"## Rust crates ({len(crates)})\n")
    out.append(
        "The crates that the workspace programs (`akm-cli` and `akm-desktop`) reach through normal "
        "dependencies on Windows. Build-time and test-only crates are not listed.\n"
    )
    out.append("| Crate | Version | Licence |")
    out.append("|---|---|---|")
    for crate in crates:
        out.append(f"| {cell(crate['name'])} | {crate['version']} | {cell(crate['license'])} |")
    out.append("")
    out.append("## MPL-2.0 crates: where the source is (SEC-04)\n")
    out.append(
        "The Mozilla Public License 2.0 applies to its files. AKM uses these crates unmodified, as "
        "published on crates.io. A release that ships the binaries must make the source of these "
        "crates available. The source of each is the `.crate` archive at the address below. It is also "
        "in the repository named in the last column. The licence text is in the licence texts below.\n"
    )
    out.append("| Crate | Version | Licence | Source archive | Repository |")
    out.append("|---|---|---|---|---|")
    for crate in mpl:
        archive = f"https://static.crates.io/crates/{crate['name']}/{crate['name']}-{crate['version']}.crate"
        repository = crate["repository"] or "(none declared)"
        out.append(f"| {crate['name']} | {crate['version']} | {cell(crate['license'])} | <{archive}> | {repository} |")
    out.append("")
    out.append(f"## npm packages in the desktop frontend ({len(npm)})\n")
    out.append(
        "The packages of `desktop/package-lock.json` that are not development-only. The three "
        "`@fontsource` packages ship font files under the SIL Open Font License 1.1. Its text and the "
        "font copyright lines are in the licence texts below.\n"
    )
    out.append("| Package | Version | Licence |")
    out.append("|---|---|---|")
    for package in npm:
        out.append(f"| {cell(package['name'])} | {package['version']} | {cell(package['license'])} |")
    out.append("")
    out.append("## Copyright and author metadata\n")
    out.append(
        "The authors that each package's manifest declares (Cargo `authors`; package.json `author`, "
        "`contributors` and `maintainers`), and the PackageCopyrightText, FileCopyrightText, "
        "PackageOriginator and PackageSupplier fields of each SPDX tag-value document that a package ships, "
        "one line each. A package that declares none is not listed.\n"
    )
    for kind_name, package in [("crate", crate) for crate in crates] + [("npm", package) for package in npm]:
        label = f"{package['name']} {package['version']} ({kind_name})"
        for author in package["authors"]:
            if author:
                out.append(f"- {label}: author in the manifest: {one_line(author)}")
        for path in package["files"]:
            if ".spdx" in path.name.lower():
                shown = Path(os.path.relpath(path, package["base"])).as_posix()
                for tag, value in spdx_fields(plain_text(path.read_bytes(), path)):
                    out.append(f"- {label}: `{shown}` {tag}: {value}")
    out.append("")
    texts = collect_texts([("crate", crate) for crate in crates] + [("npm", package) for package in npm])
    ordered = sorted(texts.items(), key=lambda item: (sorted(item[1])[0], hashlib.sha256(item[0].encode()).hexdigest()))
    out.append(f"## Licence texts ({len(ordered)} distinct texts)\n")
    for number, (text, carriers) in enumerate(ordered, 1):
        out.append(f"### Text {number}\n")
        out.append("Shipped by:\n")
        for carrier in sorted(carriers):
            out.append(f"- {carrier}")
        out.append("")
        marks = fence(text)
        out.append(f"{marks}text\n{text}{marks}\n")
    return "\n".join(out)


def address_list(text):
    """The entry list of a generated notices text: `email <text>` for each e-mail hit, once."""
    found = sorted({hit for line in text.split("\n") for hit in email_hits(views(line)[0])})
    lines = [
        "# Generated by tools/generate_third_party_notices.py from THIRD_PARTY_NOTICES.md. Do not edit it.",
        "# One entry for each e-mail hit of the third-party licence texts and manifests, which the public",
        "# snapshot scan (tools/make_public_snapshot.py) accepts. --check fails when this list is not current.",
        *(f"email {hit}" for hit in found),
    ]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="exit 1 when either file is out of date")
    parser.add_argument("--output", type=Path, default=OUTPUT, help="the notices file to write or check")
    parser.add_argument("--addresses", type=Path, default=ADDRESSES, help="the address list to write or check")
    arguments = parser.parse_args()
    text = render(shipped_crates(), shipped_npm())
    expected = {arguments.output: text, arguments.addresses: address_list(text)}
    if arguments.check:
        stale = [path for path, content in expected.items() if (path.read_bytes().decode("utf-8") if path.exists() else None) != content]
        for path in stale:
            print(f"{path.name} is out of date. Run: python tools/generate_third_party_notices.py")
        if stale:
            return 1
        print(f"{arguments.output.name} and {arguments.addresses.name} are current.")
        return 0
    for path, content in expected.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content.encode("utf-8"))
        print(f"wrote {path.name}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
