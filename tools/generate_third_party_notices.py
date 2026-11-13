"""Write THIRD_PARTY_NOTICES.md from the lock files, or check that it is current (finding FR-25, SEC-04).

Usage: python tools/generate_third_party_notices.py            (write THIRD_PARTY_NOTICES.md and its address list)
       python tools/generate_third_party_notices.py --check    (exit 1 when either file is out of date)

Rust: `cargo metadata --format-version 1 --offline --locked --filter-platform x86_64-pc-windows-msvc`.
A crate is shipped when the workspace members reach it through normal dependencies only. Build
dependencies and dev dependencies are left out, and so are the workspace crates themselves.
npm: desktop/package-lock.json, every package that is not marked `dev`.

Licence texts: each shipped package's own licence files at any depth of its folder (finding
U4-R1-06): a file whose name starts LICENSE, LICENCE, COPYING, NOTICE, UNLICENSE, COPYRIGHT, OFL,
PATENTS or THIRD-PARTY / THIRD_PARTY / THIRDPARTY, every file in a folder named LICENSES, LICENCES or
THIRD-PARTY (and their spellings), and the manifest's `license-file`. Source code files are not
texts (NOT_A_TEXT). The folder is the unpacked crate in the Cargo registry, or the package in
desktop/node_modules without the packages nested in its own node_modules, which are listed by
themselves. So the crates must be fetched (any cargo build does it) and `npm ci` must have run in
desktop. The tool cannot tell which files a build compiles, so it takes every one: a text can cover
code that AKM does not ship (for example a Linux-only backend or test images). Identical texts are
printed once, with the list of packages and paths that carry them. A package that ships no licence
file gets the standard text of each licence in its expression, from tools/licence_texts/<SPDX id>.txt
(SPDX licence list, version in that folder's README), and the file says so for that package.

The text is the file's text with line endings made LF, trailing spaces on each line removed, and
control characters other than the tab removed (tools/check_docs.py refuses them in Markdown). The
file says this once.

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
LICENCE_FILE = re.compile(r"^(licen[cs]e|copying|notice|unlicense|copyright|ofl|patents?|third[-_]?party)", re.IGNORECASE)
LICENCE_FOLDER = re.compile(r"^(licen[cs]es|third[-_]?party(?:[-_]?(?:notices|licen[cs]es))?)$", re.IGNORECASE)
NOT_A_TEXT = (".spdx", ".json", ".rs", ".js", ".mjs", ".cjs", ".ts", ".map", ".toml", ".html", ".css", ".py", ".c", ".h")
CONTROL = re.compile(r"[\x00-\x08\x0b-\x1f\x7f]")
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
        files = licence_files(folder)
        if package.get("license_file"):
            extra = folder / package["license_file"]
            if extra.is_file() and extra.resolve() not in {path.resolve() for path in files}:
                files.append(extra)
        crates.append(
            {
                "name": package["name"],
                "version": package["version"],
                "license": package["license"] or "(no licence field; read the licence file in the package)",
                "repository": package["repository"] or "",
                "authors": package.get("authors") or [],
                "files": files,
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
        version = json.loads(installed.read_text(encoding="utf-8")).get("version")
        if version != entry["version"]:
            raise SystemExit(f"desktop/{key} is {version}, the lock file says {entry['version']}. Run `npm ci` in desktop.")
        author = json.loads(installed.read_text(encoding="utf-8")).get("author")
        packages.append(
            {
                "name": name,
                "version": entry["version"],
                "license": entry.get("license") or "(none in the lock file)",
                "authors": [author if isinstance(author, str) else (author or {}).get("name", "")] if author else [],
                "files": licence_files(folder),
                "base": folder,
            }
        )
    return sorted(packages, key=lambda package: (package["name"], package["version"]))


def licence_files(folder):
    """Every licence text of the package in `folder`, at any depth, sorted by path. A nested node_modules
    holds other packages, which the lock file lists by themselves."""
    found = []
    for path in folder.rglob("*"):
        relative = path.relative_to(folder)
        if "node_modules" in relative.parts or not path.is_file() or path.name.lower().endswith(NOT_A_TEXT):
            continue
        if LICENCE_FILE.match(path.name) or any(LICENCE_FOLDER.match(part) for part in relative.parts[:-1]):
            found.append(path)
    return sorted(found, key=lambda path: path.relative_to(folder).as_posix().lower())


def clean(raw):
    text = raw.decode("utf-8", errors="replace").replace("\r\n", "\n").replace("\r", "\n")
    text = CONTROL.sub("", text)
    return "\n".join(line.rstrip() for line in text.split("\n")).strip("\n") + "\n"


def licence_ids(expression):
    return sorted({word for word in SPDX_ID.findall(expression) if word not in EXPRESSION_WORDS})


def standard_text(identifier):
    path = STANDARD / f"{identifier}.txt"
    if not path.is_file():
        raise SystemExit(f"{path} is missing: a shipped package without a licence file uses {identifier}. Add the SPDX text.")
    return clean(path.read_bytes())


def collect_texts(packages):
    """Map each distinct text to the packages and files that carry it."""
    texts = {}
    for kind, package in packages:
        label = f"{package['name']} {package['version']} ({kind})"
        if package["files"]:
            for path in package["files"]:
                raw = path.read_bytes()
                if b"\0" in raw:
                    raise SystemExit(f"{path} looks like a licence file but holds a NUL byte. Read it, then extend NOT_A_TEXT or this rule.")
                text = clean(raw)
                shown = Path(os.path.relpath(path, package["base"])).as_posix()
                texts.setdefault(text, []).append(f"{label}: `{shown}`")
        else:
            authors = ", ".join(author for author in package["authors"] if author) or "none declared"
            for identifier in licence_ids(package["license"]):
                text = standard_text(identifier)
                texts.setdefault(text, []).append(
                    f"{label}: ships no licence file; the standard {identifier} text applies. Authors in its manifest: {authors}"
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
        "**What this file holds.** Each shipped dependency with its version and its licence expression, "
        "and at the end the text of every licence file that those packages ship, at any depth of the "
        "package and with its path there, with the copyright lines that the files hold. A file below a "
        "package's root can cover code that AKM does not compile (a backend for another system, test "
        "data); the tool cannot tell, so it includes every one. A text that several packages ship is "
        "printed once, with the list of those packages. A package that ships no licence file is listed "
        "with the standard text of its licence and the authors that its manifest names.\n"
    )
    out.append(
        "**Limits.** The texts come from the packages as published. Line endings are LF, trailing spaces "
        "are removed, and control characters other than the tab are removed (for example a form feed "
        "between licence sections). A copyright line that a package keeps only in its source files, and "
        "not in a licence file, is not here. No lawyer has reviewed this file, and it is not legal advice.\n"
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
