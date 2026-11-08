"""Fail when a tracked Markdown file names a test that does not exist, or holds a control character.

Usage: python tools/check_docs.py              (the working tree)
       python tools/check_docs.py --rev <rev>  (a commit, for example to show a past mistake)

A name in backticks that looks like a Rust test (snake_case with at least three underscores, for
example `a_read_creates_no_folder_and_no_file`) must exist as `fn <name>` in the tracked Rust
sources. Raw reviews in docs/reviews/ are exempt, because they are kept unchanged. A line that
must keep a removed name (a history record) carries `<!-- check-docs: historical: <reason> -->`.
Exit 0 when clean, 1 with one line per problem.
"""

import argparse
import re
import subprocess
import sys
from pathlib import Path

TEST_NAME = re.compile(r"`(?:[\w/.-]+\s*::\s*)?([a-z][a-z0-9]*(?:_[a-z0-9]+){3,})`")
CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
RUST_FN = re.compile(r"\bfn\s+([a-z_][a-z0-9_]*)")
HISTORICAL = re.compile(r"<!-- check-docs: historical: \S.*? -->")
EXEMPT_PREFIXES = ("docs/reviews/",)


def git(*args):
    return subprocess.run(["git", *args], capture_output=True, check=True).stdout.decode("utf-8")


def source(rev):
    if rev is None:
        names = git("ls-files").splitlines()
        return names, lambda path: Path(path).read_text(encoding="utf-8", errors="replace")
    names = git("ls-tree", "-r", "--name-only", rev).splitlines()
    return names, lambda path: git("show", f"{rev}:{path}")


def problems(rev):
    names, read = source(rev)
    functions = set()
    for path in names:
        if path.endswith(".rs") and (rev or Path(path).exists()):
            functions.update(RUST_FN.findall(read(path)))
    found = []
    for path in names:
        if not path.endswith(".md") or path.startswith(EXEMPT_PREFIXES):
            continue
        if rev is None and not Path(path).exists():
            continue
        for number, line in enumerate(read(path).splitlines(), 1):
            for match in CONTROL.finditer(line):
                found.append(f"{path}:{number}: control character 0x{ord(match.group()):02x}")
            if HISTORICAL.search(line):
                continue
            for name in TEST_NAME.findall(line):
                if name not in functions:
                    found.append(f"{path}:{number}: names `{name}`, which is not a fn in the Rust sources")
    return found


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--rev")
    rev = parser.parse_args().rev
    found = problems(rev)
    for line in found:
        print(line)
    print(f"RESULT: {'FAIL' if found else 'PASS'} ({len(found)} problems at {rev or 'the working tree'})")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
