"""Fail when a tracked Markdown file names a test that does not exist, or holds a control character.

Usage: python tools/check_docs.py              (the working tree)
       python tools/check_docs.py --rev <rev>  (a commit, for example to show a past mistake)

Git (class F): in the working tree the check lists the tracked files with `git ls-files`, a read of
this checkout's index through git_child.existing. With --rev it reads the commit from
git_child.source_copy, a copy of the Git data in a temporary folder, so Git never reads objects of the
real repository, and a commit whose objects are missing fails plainly instead of being fetched.

A name in backticks that looks like a Rust test (snake_case with at least three underscores, for
example `a_read_creates_no_folder_and_no_file`) must exist as a test in the tracked Rust sources:
a `fn <name>` that carries a `#[test]` or `#[<runtime>::test]` attribute. A name that is only a
plain function fails too, when the line calls it a test (finding FR-24). Every fn in the file of a
`[[test]]` target with `harness = false` counts as a test, because that file runs its own steps. A production function that
a doc names as part of an API, on a line that does not say "test", is allowed. Raw reviews in
docs/reviews/ are exempt, because they are kept unchanged. A line that must keep a removed name (a history record) carries
`<!-- check-docs: historical: <reason> -->`. The Rust sources are read as code: a declaration inside a
comment, a string, a raw string or a char literal is not a fn and not a test (finding U4-R1-07).

Each row of a docs/CLAIMS.md table whose header starts `| Claim |` has four cells: the claim, where it is stated, the evidence and the
limit. Every name in backticks in the evidence cell must be a test fn as above, or a proof name of
PROOFS; at least one must be there, and the limit cell must not be empty. Each proof is resolved to
things that exist and can run (finding U4-R1-07):

- each of its files is tracked and, in the working tree, present on disk;
- a workflow proof's job is a key of the workflow's `jobs` mapping, read by indentation, so a job name
  in a comment, a block scalar or a quoted scalar is not a job; and the job's `run` steps hold the
  proof's command, outside whole-line comments;
- a frontend proof's test title is the first argument of a `test(` or `it(` call in its file, outside
  comments (so a `test.skip(` call does not count);
- a proof that runs a Rust test names a test fn as above.

The Named proofs table of docs/CLAIMS.md (header `| Proof name |`) lists exactly the names of PROOFS,
each row's "What runs" cell names the proof's first file or its job, and a row's "Where it runs" cell
starts "CI, each push" exactly when the proof runs in the build workflow.

A control character includes a vertical tab and a form feed. `str.splitlines` treats both as line
breaks and would drop them before this check, so the lines are split on the newline only.
Exit 0 when clean, 1 with one line per problem.
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import git_child  # noqa: E402  (every Git call goes through it: class F)

TEST_NAME = re.compile(r"`(?:[\w/.-]+\s*::\s*)?([a-z][a-z0-9]*(?:_[a-z0-9]+){3,})`")
CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
RUST_FN = re.compile(r"\bfn\s+([a-z_][a-z0-9_]*)")
# A test is a `fn` after a `#[test]` or `#[x::test]` attribute, with other attributes between. Comments
# and literals are blanked first (rust_code), so they cannot hold or hide one.
RUST_TEST = re.compile(
    r"#\[(?:\w+::)*test\b[^\]]*\]"
    r"(?:\s*#\[[^\]]*\])*"
    r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([a-z_][a-z0-9_]*)"
)
RAW_STRING = re.compile(r'(?:b|c)?r(#*)"')
SAYS_TEST = re.compile(r"\btests?\b", re.IGNORECASE)
HISTORICAL = re.compile(r"<!-- check-docs: historical: \S.*? -->")
EXEMPT_PREFIXES = ("docs/reviews/",)
CLAIMS = "docs/CLAIMS.md"
EVIDENCE = re.compile(r"`([^`]+)`")
BUILD = ".github/workflows/build.yml"
NATIVE_PROOF = ".github/workflows/native-proof.yml"


@dataclass(frozen=True)
class Proof:
    """What a proof name stands for. `files[0]` is the file that the Named proofs table names."""

    files: tuple[str, ...]
    workflow: str | None = None
    job: str | None = None
    runs: tuple[str, ...] = ()
    rust_test: str | None = None
    title: str | None = None


PROOFS = {
    "keepassxc-interchange": Proof(("tools/verify_keepassxc.py",), BUILD, "verify", ("tools/verify_keepassxc.py",)),
    "kdbxweb-second-reader": Proof(("tools/verify_kdbxweb/verify.mjs",), BUILD, "verify", ("node verify.mjs",)),
    "no-network-check": Proof(("tools/check_no_network.py", "tools/test_check_no_network.py"), BUILD, "verify", ("tools/check_no_network.py", "tools/test_check_no_network.py")),
    "native-gui-proof": Proof(("tools/verify_windows_app.ps1",)),
    "frontend-reveal-timer": Proof(
        ("desktop/src/components/KeyDetail.timer.test.tsx",), BUILD, "verify", ("npm test",),
        title="the countdown text is the only timer, and it resets on each reveal",
    ),
    "public-snapshot-scan": Proof(
        ("tools/test_make_public_snapshot.py", "tools/test_snapshot_source.py"), BUILD, "verify",
        ("tools/test_make_public_snapshot.py", "tools/test_snapshot_source.py"),
    ),
    "third-party-notices-check": Proof(
        ("tools/test_third_party_notices.py", "tools/generate_third_party_notices.py"), BUILD, "verify",
        ("tools/generate_third_party_notices.py --check", "tools/test_third_party_notices.py"),
    ),
    "native-proof-real-clipboard": Proof(
        (NATIVE_PROOF,), NATIVE_PROOF, "real-clipboard", ("actual_windows_private_formats_are_present",),
        rust_test="actual_windows_private_formats_are_present",
    ),
    "native-proof-full-gui": Proof(("tools/verify_windows_app.ps1",), NATIVE_PROOF, "full-gui-proof", ("tools/verify_windows_app.ps1",)),
    "native-proof-install": Proof(("tools/test_install.ps1",), NATIVE_PROOF, "install-upgrade-uninstall", ("tools/test_install.ps1",)),
}


def source(rev, copy=None):
    """(tracked names, read, exists). In the working tree a tracked file must also be on disk. A commit
    is read from `copy`, a git_child.source_copy, with one cat-file for all of its blobs."""
    if rev is None:
        listing = git_child.existing(Path.cwd()).run("ls-files", "-z").decode("utf-8")
        names = [name for name in listing.split("\0") if name]
        return names, lambda path: Path(path).read_text(encoding="utf-8", errors="replace"), lambda path: Path(path).is_file()
    blobs = {}
    for record in (r for r in copy.run("ls-tree", "-r", "-z", "--full-tree", rev).split(b"\0") if r):
        meta, path = record.split(b"\t", 1)
        mode, kind, oid = meta.decode("ascii").split()
        if kind == "blob":
            blobs[path.decode("utf-8")] = oid
    output = copy.run("cat-file", "--batch", input="".join(f"{oid}\n" for oid in blobs.values()).encode("ascii"))
    texts, position = {}, 0
    for path, oid in blobs.items():
        end = output.index(b"\n", position)
        header = output[position:end].decode("ascii").split()
        if len(header) != 3:
            raise git_child.GitError(f"the object of {path} at {rev} is missing; nothing is fetched")
        size = int(header[2])
        texts[path] = output[end + 1:end + 1 + size].decode("utf-8", errors="replace")
        position = end + 1 + size + 1
    return list(texts), texts.__getitem__, texts.__contains__


def lines_of(text):
    """Split on the newline only, and drop one carriage return before it."""
    return [line.removesuffix("\r") for line in text.split("\n")]


# Reading sources as code.

def rust_code(text):
    """The Rust text with every comment and every string, raw string and char literal blanked to
    spaces. Newlines stay, so line numbers and offsets do not move."""
    out = list(text)
    size = len(text)

    def blank(start, end):
        for index in range(start, min(end, size)):
            if out[index] != "\n":
                out[index] = " "

    index = 0
    while index < size:
        if text.startswith("//", index):
            end = text.find("\n", index)
            end = size if end < 0 else end
        elif text.startswith("/*", index):
            depth, end = 1, index + 2
            while end < size and depth:
                if text.startswith("/*", end):
                    depth, end = depth + 1, end + 2
                elif text.startswith("*/", end):
                    depth, end = depth - 1, end + 2
                else:
                    end += 1
        elif (raw := RAW_STRING.match(text, index)) and (index == 0 or not (text[index - 1].isalnum() or text[index - 1] == "_")):
            close = text.find('"' + raw.group(1), raw.end())
            end = size if close < 0 else close + 1 + len(raw.group(1))
        elif text[index] == '"':
            end = index + 1
            while end < size and text[end] != '"':
                end += 2 if text[end] == "\\" else 1
            end += 1
        elif text[index] == "'" and index + 1 < size and text[index + 1] == "\\":
            close = text.find("'", index + 3)
            end = size if close < 0 else close + 1
        elif text[index] == "'" and index + 2 < size and text[index + 2] == "'":
            end = index + 3
        else:
            index += 1
            continue
        blank(index, end)
        index = end
    return "".join(out)


def script_code(text):
    """JavaScript or TypeScript with its comments blanked; strings and template literals stay."""
    out = list(text)
    size = len(text)
    index = 0
    while index < size:
        character = text[index]
        if character in "\"'`":
            end = index + 1
            while end < size and text[end] != character and (character == "`" or text[end] != "\n"):
                end += 2 if text[end] == "\\" else 1
            index = end + 1
            continue
        if text.startswith("//", index):
            end = text.find("\n", index)
            end = size if end < 0 else end
        elif text.startswith("/*", index):
            end = text.find("*/", index + 2)
            end = size if end < 0 else end + 2
        else:
            index += 1
            continue
        for position in range(index, end):
            if out[position] != "\n":
                out[position] = " "
        index = end
    return "".join(out)


YAML_KEY = re.compile(r"""("(?:[^"\\]|\\.)*"|'(?:[^']|'')*'|[^\s#"'-][^:#]*?|-[^\s:#][^:#]*?)\s*:(?:\s+(.*))?$""")
YAML_BLOCK = re.compile(r"[|>][0-9+-]*\s*(?:#.*)?$")


def closing_quote(text, quote):
    """The index of the quote that closes a quoted YAML scalar in `text`, or -1."""
    index = 0
    while index < len(text):
        if quote == '"' and text[index] == "\\":
            index += 2
            continue
        if text[index] == quote:
            if quote == "'" and text[index + 1:index + 2] == "'":
                index += 2
                continue
            return index
        index += 1
    return -1


def yaml_value(value):
    """The scalar after `key:`, without its comment: (text, the quote that is still open or None)."""
    value = (value or "").strip()
    if value.startswith("#"):
        return "", None
    if value.startswith(("'", '"')):
        end = closing_quote(value[1:], value[0])
        return (value[1:], value[0]) if end < 0 else (value[1:end + 1], None)
    return re.split(r"\s+#", value, maxsplit=1)[0].strip(), None


def workflow_jobs(text):
    """{job id: run lines} of a workflow: the keys of its top-level `jobs` mapping, read by
    indentation, and the text of each job's `run` steps. A key inside a comment, a block scalar or a
    quoted scalar that spans lines is not a key."""
    jobs = {}
    in_jobs = False
    job_column = None
    current = None
    block = None  # (column of the key that opened a block scalar, the list it fills or None)
    quote = None  # (quote character, the list it fills or None) of a quoted scalar that spans lines
    for raw in lines_of(text):
        stripped = raw.strip()
        if quote is not None:
            if quote[1] is not None:
                quote[1].append(raw)
            if closing_quote(raw, quote[0]) >= 0:
                quote = None
            continue
        column = len(raw) - len(raw.lstrip(" "))
        if block is not None:
            if not stripped or column > block[0]:
                if block[1] is not None:
                    block[1].append(raw)
                continue
            block = None
        if not stripped or stripped.startswith("#"):
            continue
        content = raw[column:]
        while content == "-" or content.startswith("- "):
            rest = content[1:]
            column += 1 + len(rest) - len(rest.lstrip(" "))
            content = rest.lstrip(" ")
        match = YAML_KEY.match(content)
        if not match:
            continue
        key = match.group(1)
        if key[:1] in "\"'":
            key = key[1:-1].replace("''", "'") if key[0] == "'" else key[1:-1]
        value = match.group(2)
        target = None
        if column == 0:
            in_jobs = key == "jobs" and not yaml_value(value)[0]
            current = None
        elif in_jobs:
            if job_column is None:
                job_column = column
            if column == job_column:
                current = key
                jobs[current] = []
            elif current is not None and column > job_column and key == "run":
                target = jobs[current]
        if value is not None and YAML_BLOCK.match(value.strip()):
            block = (column, target)
            continue
        text_value, open_quote = yaml_value(value)
        if target is not None and text_value:
            target.append(text_value)
        if open_quote:
            quote = (open_quote, target)
    return jobs


# The checks.

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


def proof_problems(name, proof, tests, tracked, read, exists):
    """Why proof `name` does not resolve to files, a job, a command and a test that exist; [] when it does."""
    found = []
    for path in proof.files:
        if path not in tracked or not exists(path):
            found.append(f"proof `{name}` needs the file {path}, which is not in the checked tree")
    if proof.workflow:
        if proof.workflow not in tracked or not exists(proof.workflow):
            return found + [f"proof `{name}` needs the workflow {proof.workflow}, which is not in the checked tree"]
        jobs = workflow_jobs(read(proof.workflow))
        if proof.job not in jobs:
            return found + [f"proof `{name}` needs the job `{proof.job}` in the jobs of {proof.workflow}"]
        code = "\n".join(line for line in jobs[proof.job] if not line.strip().startswith("#"))
        for command in proof.runs:
            if command not in code:
                found.append(f"proof `{name}` needs a run step of job `{proof.job}` that holds {command!r}")
    if proof.rust_test and proof.rust_test not in tests:
        found.append(f"proof `{name}` runs `{proof.rust_test}`, which is not a test fn in the Rust sources")
    if proof.title and not found and proof.files[0] in tracked and exists(proof.files[0]):
        call = re.compile(r"(?<![\w.$])(?:it|test)\s*\(\s*([\"'`])" + re.escape(proof.title) + r"\1")
        if not call.search(script_code(read(proof.files[0]))):
            found.append(f"proof `{name}` needs a test( or it( call titled {proof.title!r} in {proof.files[0]}, outside comments")
    return found


def table_rows(text, header):
    """(line number, cells) of the rows of each table whose header line starts with `header`."""
    rows = []
    inside = False
    for number, line in enumerate(lines_of(text), 1):
        if not line.startswith("|"):
            inside = False
            continue
        if line.startswith(header):
            inside = True
            continue
        if inside and not line.startswith("|-"):
            rows.append((number, [cell.strip() for cell in line.strip().strip("|").split("|")]))
    return rows


def named_proof_problems(text):
    """The Named proofs table must list exactly PROOFS, name each proof's file or job, and say where it runs."""
    found = []
    listed = set()
    for number, cells in table_rows(text, "| Proof name |"):
        where = f"{CLAIMS}:{number}"
        names = EVIDENCE.findall(cells[0]) if cells else []
        if len(cells) != 3 or len(names) != 1:
            found.append(f"{where}: a Named proofs row needs 3 cells and one proof name in backticks")
            continue
        name = names[0]
        listed.add(name)
        proof = PROOFS.get(name)
        if proof is None:
            found.append(f"{where}: the Named proofs table lists `{name}`, which PROOFS in tools/check_docs.py does not define")
            continue
        named = f"`{proof.job}`" if proof.workflow == NATIVE_PROOF else f"`{proof.files[0]}`"
        if named not in cells[1]:
            found.append(f"{where}: the row of `{name}` must name {named} in its What runs cell")
        if cells[2].startswith("CI, each push") != (proof.workflow == BUILD):
            found.append(f"{where}: the row of `{name}` must say \"CI, each push\" exactly when the build workflow runs it")
    for name in sorted(set(PROOFS) - listed):
        found.append(f"{CLAIMS}: PROOFS in tools/check_docs.py defines `{name}`, which the Named proofs table does not list")
    return found


def claim_problems(text, tests, names, read, exists):
    """Problems of the docs/CLAIMS.md table rows, and the number of rows checked."""
    found = []
    tracked = set(names)
    rows = table_rows(text, "| Claim |")
    for number, cells in rows:
        where = f"{CLAIMS}:{number}"
        if len(cells) != 4:
            found.append(f"{where}: a claim row needs 4 cells (claim, where stated, evidence, limit), not {len(cells)}")
            continue
        evidence = EVIDENCE.findall(cells[2])
        if not evidence:
            found.append(f"{where}: the evidence cell names no test fn and no proof name")
        for item in evidence:
            if item in PROOFS:
                found.extend(f"{where}: {problem}" for problem in proof_problems(item, PROOFS[item], tests, tracked, read, exists))
            elif item not in tests:
                found.append(f"{where}: evidence `{item}` is neither a test fn nor a proof name")
        if not cells[3]:
            found.append(f"{where}: the limit cell is empty")
    if any(line.startswith("| Proof name |") for line in lines_of(text)):
        found.extend(named_proof_problems(text))
    return found, len(rows)


def problems(rev, copy=None):
    names, read, exists = source(rev, copy)
    functions = set()
    tests = set()
    custom = harness_free_tests(names, read)
    for path in names:
        if path.endswith(".rs") and exists(path):
            code = rust_code(read(path))
            functions.update(RUST_FN.findall(code))
            tests.update(RUST_TEST.findall(code))
            if path in custom:
                tests.update(RUST_FN.findall(code))
    found = []
    for path in names:
        if not path.endswith(".md") or path.startswith(EXEMPT_PREFIXES) or not exists(path):
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
    if CLAIMS in names and exists(CLAIMS):
        claim_found, rows = claim_problems(read(CLAIMS), tests, names, read, exists)
        found.extend(claim_found)
    return found, rows


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--rev")
    rev = parser.parse_args().rev
    try:
        if rev is None:
            found, rows = problems(None)
        else:
            with git_child.source_copy(Path.cwd()) as copy:
                found, rows = problems(rev, copy)
    except git_child.GitError as error:
        print(f"RESULT: FAIL ({error})")
        return 1
    for line in found:
        print(line)
    print(f"RESULT: {'FAIL' if found else 'PASS'} ({len(found)} problems at {rev or 'the working tree'}; {rows} claims checked)")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
