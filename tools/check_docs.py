"""Fail when a tracked Markdown file names a test that does not exist, or holds a control character.

Usage: python tools/check_docs.py              (the working tree)
       python tools/check_docs.py --rev <rev>  (a commit, for example to show a past mistake)

A name in backticks that looks like a Rust test (snake_case with at least three underscores, for
example `a_read_creates_no_folder_and_no_file`) must exist as a test in the tracked Rust sources:
a `fn <name>` that carries a `#[test]` or `#[<runtime>::test]` attribute. A name that is only a
plain function fails too, when the line calls it a test (finding FR-24). Every fn in the file of a
`[[test]]` target with `harness = false` counts as a test, because that file runs its own steps. A production function that
a doc names as part of an API, on a line that does not say "test", is allowed. Raw reviews in
docs/reviews/ are exempt, because they are kept unchanged. A line that must keep a removed name (a history record) carries
`<!-- check-docs: historical: <reason> -->`.

Each row of a docs/CLAIMS.md table whose header starts `| Claim |` has four cells: the claim, where it is stated, the evidence and the
limit. Every name in backticks in the evidence cell must be a test fn as above, or a proof name of
PROOFS; at least one must be there, and the limit cell must not be empty. A proof name is bound to
a tracked file and a text in that file (for a workflow, its job), so a renamed job or a removed
script fails the check.

A control character includes a vertical tab and a form feed. `str.splitlines` treats both as line
breaks and would drop them before this check, so the lines are split on the newline only.
Exit 0 when clean, 1 with one line per problem.
"""

import argparse
import re
import subprocess
import sys
import tomllib
from pathlib import Path

TEST_NAME = re.compile(r"`(?:[\w/.-]+\s*::\s*)?([a-z][a-z0-9]*(?:_[a-z0-9]+){3,})`")
CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
RUST_FN = re.compile(r"\bfn\s+([a-z_][a-z0-9_]*)")
# A test is a `fn` after a `#[test]` or `#[x::test]` attribute, with other attributes and comments between.
RUST_TEST = re.compile(
    r"#\[(?:\w+::)*test\b[^\]]*\]"
    r"(?:\s*(?:#\[[^\]]*\]|//[^\n]*))*"
    r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([a-z_][a-z0-9_]*)"
)
SAYS_TEST = re.compile(r"\btests?\b", re.IGNORECASE)
HISTORICAL = re.compile(r"<!-- check-docs: historical: \S.*? -->")
EXEMPT_PREFIXES = ("docs/reviews/",)
CLAIMS = "docs/CLAIMS.md"
EVIDENCE = re.compile(r"`([^`]+)`")
NATIVE_PROOF = ".github/workflows/native-proof.yml"
# The named proofs that a claim may cite besides a Rust test: the tracked file and a text it must hold.
PROOFS = {
    "keepassxc-interchange": ("tools/verify_keepassxc.py", None),
    "kdbxweb-second-reader": ("tools/verify_kdbxweb/verify.mjs", None),
    "no-network-check": ("tools/test_check_no_network.py", None),
    "native-gui-proof": ("tools/verify_windows_app.ps1", None),
    "frontend-reveal-timer": ("desktop/src/components/KeyDetail.timer.test.tsx", None),
    "public-snapshot-scan": ("tools/test_make_public_snapshot.py", None),
    "third-party-notices-check": ("tools/test_third_party_notices.py", None),
    "native-proof-real-clipboard": (NATIVE_PROOF, "\n  real-clipboard:"),
    "native-proof-full-gui": (NATIVE_PROOF, "\n  full-gui-proof:"),
    "native-proof-install": (NATIVE_PROOF, "\n  install-upgrade-uninstall:"),
}


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, check=True).stdout.decode("utf-8")


def source(rev):
    if rev is None:
        names = git("ls-files").splitlines()
        return names, lambda path: Path(path).read_text(encoding="utf-8", errors="replace")
    names = git("ls-tree", "-r", "--name-only", rev).splitlines()
    return names, lambda path: git("show", f"{rev}:{path}")


def lines_of(text):
    """Split on the newline only, and drop one carriage return before it."""
    return [line.removesuffix("\r") for line in text.split("\n")]


def harness_free_tests(names, read):
    """Rust files of `[[test]]` targets with `harness = false`: every fn there is a step of a test."""
    files = set()
    for path in names:
        if not path.endswith("Cargo.toml"):
            continue
        try:
            manifest = tomllib.loads(read(path))
        except tomllib.TOMLDecodeError:
            continue
        for target in manifest.get("test", []):
            if target.get("harness") is False:
                rel = target.get("path") or f"tests/{target.get('name', '')}.rs"
                files.add(str(Path(path).parent / rel).replace("\\", "/"))
    return files


def claim_problems(text, tests, names, read):
    """Problems of the docs/CLAIMS.md table rows, and the number of rows checked."""
    found = []
    rows = 0
    tracked = set(names)
    in_claims = False
    for number, line in enumerate(lines_of(text), 1):
        if not line.startswith("|"):
            in_claims = False
            continue
        if line.startswith("| Claim |"):
            in_claims = True
            continue
        if not in_claims or line.startswith("|-"):
            continue
        rows += 1
        where = f"{CLAIMS}:{number}"
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        if len(cells) != 4:
            found.append(f"{where}: a claim row needs 4 cells (claim, where stated, evidence, limit), not {len(cells)}")
            continue
        evidence = EVIDENCE.findall(cells[2])
        if not evidence:
            found.append(f"{where}: the evidence cell names no test fn and no proof name")
        for item in evidence:
            if item in PROOFS:
                path, needle = PROOFS[item]
                if path not in tracked or (needle and needle not in read(path)):
                    found.append(f"{where}: proof `{item}` needs {path}" + (f" with {needle.strip()!r}" if needle else ""))
            elif item not in tests:
                found.append(f"{where}: evidence `{item}` is neither a test fn nor a proof name")
        if not cells[3]:
            found.append(f"{where}: the limit cell is empty")
    return found, rows


def problems(rev):
    names, read = source(rev)
    functions = set()
    tests = set()
    custom = harness_free_tests(names, read)
    for path in names:
        if path.endswith(".rs") and (rev or Path(path).exists()):
            text = read(path)
            functions.update(RUST_FN.findall(text))
            tests.update(RUST_TEST.findall(text))
            if path in custom:
                tests.update(RUST_FN.findall(text))
    found = []
    for path in names:
        if not path.endswith(".md") or path.startswith(EXEMPT_PREFIXES):
            continue
        if rev is None and not Path(path).exists():
            continue
        for number, line in enumerate(lines_of(read(path)), 1):
            for match in CONTROL.finditer(line):
                found.append(f"{path}:{number}: control character 0x{ord(match.group()):02x}")
            if HISTORICAL.search(line):
                continue
            for name in TEST_NAME.findall(line):
                if name not in functions:
                    found.append(f"{path}:{number}: names `{name}`, which is not a fn in the Rust sources")
                elif name not in tests and SAYS_TEST.search(line):
                    found.append(f"{path}:{number}: names `{name}`, which is a fn but not a test")
    rows = 0
    if CLAIMS in names and (rev or Path(CLAIMS).exists()):
        claim_found, rows = claim_problems(read(CLAIMS), tests, names, read)
        found.extend(claim_found)
    return found, rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--rev")
    rev = parser.parse_args().rev
    found, rows = problems(rev)
    for line in found:
        print(line)
    print(f"RESULT: {'FAIL' if found else 'PASS'} ({len(found)} problems at {rev or 'the working tree'}; {rows} claims checked)")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
