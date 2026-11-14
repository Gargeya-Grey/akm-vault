"""Fail when a tracked Markdown file names a test that does not exist or holds a control character, or
when a docs/CLAIMS.md row names evidence that does not run.

Usage: python tools/check_docs.py              (fast mode: the working tree, read as text)
       python tools/check_docs.py --rev <rev>  (fast mode on a commit, for example to show a past mistake)
       python tools/check_docs.py --full [--rust-list FILE] [--rust-ignored-list FILE]
                                         [--vitest-list FILE] [--playwright-list FILE]
                                       (full mode: the working tree and the test runners' own listings)

TWO MODES (findings U4-R1-07, U4-R2-05). Fast mode is the default. It reads the sources as text and
needs no build, so it cannot prove that a test runs, and its output says so. Full mode asks the test
runners which tests they collect and would run:

- Rust: `cargo test --workspace --locked --offline -- --list`, and the same with `--ignored` for the
  ignored tests. A test runs under `cargo test` when the first listing holds it and the second does
  not. A doc test is not a name. The steps of a `[[test]]` target with `harness = false` are read from
  its source (every fn of its file counts), because cargo cannot list them: the listing starts that
  binary, which runs its steps;
- Vitest: `vitest list --json --staticParse=false` in desktop. Vitest then collects the tests by
  running each test file; its static parse (the default) also lists a test call inside a function
  that never runs. The listing holds only the tests that would run: a skipped or todo test, a test of
  a skipped group, a skipIf or runIf test that is off and a test call that never runs are not in it;
- Playwright: `playwright test --list --reporter=json` in desktop. A test whose expected status is
  "skipped" does not run. The listing does not start the configured web server.

An option names a file that holds a listing (CI passes none and the commands reuse its builds; a local
run can list from its own target folder). Without the option, full mode runs the command. It reads
each listing once, and a frontend listing only when a proof needs it.

WORKFLOWS. Both modes read a workflow with PyYAML (pinned in CI; docs/TESTING.md): a job is a key of
the workflow's `jobs` mapping, and a command is the `run` value of one of the job's steps, outside its
whole-line shell comments. So a command in an `env` value, a job name in a comment and a key inside a
scalar are not evidence.

FAST MODE reads Rust as code: every comment, string, raw string and char literal is blanked first. A
test is a fn with a `#[test]` or `#[<runtime>::test]` attribute, with other attributes around it.
`#[ignore]` makes it an ignored test. A `#[cfg(...)]` on the fn, on a `mod` block around it or as
`#![cfg(...)]` on its file turns it off when it is false for a Windows test build, or when fast mode
cannot evaluate it: only test, windows, unix, debug_assertions, target_os, target_family, all, any and
not are known. JavaScript and TypeScript are read with comments and the inside of every string
blanked: a frontend test is a `test(` or `it(` call whose first argument is a string that is the
title, not inside a skipped group (describe.skip, todo, fixme, skipIf or runIf, xdescribe), in a file
without an `.only(` call. Fast mode does not see a runtime skip, a test call in a function that never
runs, a test that a loop or a helper makes,
a configuration that leaves a file out, a cfg on an out-of-line module (`mod x;`) or one that Cargo
features decide. Full mode sees all of them.

NAMES. A name in backticks that looks like a Rust test (snake_case with at least three underscores,
for example `a_read_creates_no_folder_and_no_file`) must be a fn in the tracked Rust sources. When its
line says "test", it must be a test, ignored or not (finding FR-24). A production function that a doc
names as part of an API, on a line that does not say "test", is allowed. Raw reviews in docs/reviews/
are exempt, because they are kept unchanged. THIRD_PARTY_NOTICES.md is exempt from this name rule only,
because it holds the shipped packages' own READMEs and change logs; its control characters are still
checked. A line that must keep a removed name (a history record) carries
`<!-- check-docs: historical: <reason> -->`.

CLAIMS. Each row of a docs/CLAIMS.md table whose header starts `| Claim |` has four cells: the claim,
where it is stated, the evidence and the limit. Every name in backticks in the evidence cell must be a
Rust test that `cargo test` runs (not an ignored test, and not one that a cfg turns off), or a proof
name of PROOFS. At least one must be there, and the limit cell must not be empty. Each proof resolves
to things that exist and run:

- each of its files is tracked and, in the working tree, present on disk;
- a workflow proof's job is a job of its workflow, and the job's commands hold the proof's command;
- a frontend proof's title is a test of its file that its runner runs;
- a proof's Rust test is a test, or an ignored test when the proof runs it with `--ignored`.

The Named proofs table of docs/CLAIMS.md (header `| Proof name |`) lists exactly the names of PROOFS,
each row's "What runs" cell names the proof's first file or its job, and a row's "Where it runs" cell
starts "CI, each push" exactly when the proof runs in the build workflow.

Git (class F): in the working tree the check lists the tracked files with `git ls-files`, a read of
this checkout's index through git_child.existing. With --rev it reads the commit from
git_child.source_copy, a copy of the Git data in a temporary folder, so Git never reads objects of the
real repository, and a commit whose objects are missing fails plainly instead of being fetched.

A control character includes a vertical tab and a form feed. `str.splitlines` treats both as line
breaks and would drop them before this check, so the lines are split on the newline only.
Exit 0 when clean, 1 with one line per problem.
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass, field
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import git_child  # noqa: E402  (every Git call goes through it: class F)

try:
    import yaml
except ImportError:  # main() refuses to run without it and says how to install it
    yaml = None

PYYAML = "PyYAML==6.0.3"
TEST_NAME = re.compile(r"`(?:[\w/.-]+\s*::\s*)?([a-z][a-z0-9]*(?:_[a-z0-9]+){3,})`")
CONTROL = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
RUST_FN = re.compile(r"\bfn\s+([a-z_][a-z0-9_]*)")
# A fn or a mod block with the attributes before it, in Rust whose comments and literals are blanked.
RUST_ITEM = re.compile(r"((?:#\[[^\]]*\]\s*)+)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([a-z_][a-z0-9_]*)")
RUST_MOD = re.compile(r"((?:#\[[^\]]*\]\s*)+)(?:pub(?:\([^)]*\))?\s+)?mod\s+\w+\s*\{")
ATTRIBUTE = re.compile(r"#!?\[[^\]]*\]")
TEST_ATTRIBUTE = re.compile(r"#\[\s*(?:\w+\s*::\s*)*test\s*[\](]")
IGNORE_ATTRIBUTE = re.compile(r"#\[\s*ignore\b")
CFG_ATTRIBUTE = re.compile(r"#!?\[\s*cfg\s*\((.*)\)\s*\]$", re.DOTALL)
FILE_CFG = re.compile(r"#!\[\s*cfg\s*\([^\]]*\)\s*\]")
CFG_TOKEN = re.compile(r'\s*(?:([A-Za-z_]\w*)|("(?:[^"\\]|\\.)*")|(\S))')
CFG_NAMES = {"test": True, "windows": True, "debug_assertions": True, "unix": False}
CFG_VALUES = {"target_os": "windows", "target_family": "windows"}
RAW_STRING = re.compile(r'(?:b|c)?r(#*)"')
FRONTEND_CALL = re.compile(r"(?<![\w.$])(?:it|test)\s*\(\s*(?=[\"'`])")
GROUP_OFF = re.compile(r"(?<![\w$])(?:(?:test\s*\.\s*)?describe\s*\.\s*(?:skip|todo|fixme|skipIf|runIf)\b|xdescribe\b)")
ONLY = re.compile(r"\.\s*only\s*\(")
SAYS_TEST = re.compile(r"\btests?\b", re.IGNORECASE)
HISTORICAL = re.compile(r"<!-- check-docs: historical: \S.*? -->")
EXEMPT_PREFIXES = ("docs/reviews/",)
# Generated from the shipped packages' READMEs and change logs: the names in it are not AKM's tests.
THIRD_PARTY_TEXTS = "THIRD_PARTY_NOTICES.md"
CLAIMS = "docs/CLAIMS.md"
EVIDENCE = re.compile(r"`([^`]+)`")
BUILD = ".github/workflows/build.yml"
NATIVE_PROOF = ".github/workflows/native-proof.yml"
FAST_NOTE = "NOTE: fast mode reads the sources as text; it does not prove that a test runs. Run --full for the test runners' own listings."


@dataclass(frozen=True)
class Proof:
    """What a proof name stands for. `files[0]` is the file that the Named proofs table names. A
    frontend proof has a title and the runner that runs it (vitest or playwright)."""

    files: tuple[str, ...]
    workflow: str | None = None
    job: str | None = None
    runs: tuple[str, ...] = ()
    rust_test: str | None = None
    ignored: bool = False
    title: str | None = None
    runner: str | None = None


PROOFS = {
    "keepassxc-interchange": Proof(("tools/verify_keepassxc.py",), BUILD, "verify", ("tools/verify_keepassxc.py",)),
    "kdbxweb-second-reader": Proof(("tools/verify_kdbxweb/verify.mjs",), BUILD, "verify", ("node verify.mjs",)),
    "no-network-check": Proof(("tools/check_no_network.py", "tools/test_check_no_network.py"), BUILD, "verify", ("tools/check_no_network.py", "tools/test_check_no_network.py")),
    "native-gui-proof": Proof(("tools/verify_windows_app.ps1",)),
    "frontend-reveal-timer": Proof(
        ("desktop/src/components/KeyDetail.timer.test.tsx",), BUILD, "verify", ("npm test",),
        title="the countdown text is the only timer, and it resets on each reveal", runner="vitest",
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
        (NATIVE_PROOF,), NATIVE_PROOF, "real-clipboard", ("actual_windows_private_formats_are_present", "--ignored"),
        rust_test="actual_windows_private_formats_are_present", ignored=True,
    ),
    "native-proof-full-gui": Proof(("tools/verify_windows_app.ps1",), NATIVE_PROOF, "full-gui-proof", ("tools/verify_windows_app.ps1",)),
    "native-proof-install": Proof(("tools/test_install.ps1",), NATIVE_PROOF, "install-upgrade-uninstall", ("tools/test_install.ps1",)),
}

# Full mode: the listing commands, and the folder each runs in.
LISTINGS = {
    "rust": (("cargo", "test", "--workspace", "--locked", "--offline", "--", "--list"), "."),
    "rust-ignored": (("cargo", "test", "--workspace", "--locked", "--offline", "--", "--list", "--ignored"), "."),
    "vitest": (("node", "node_modules/vitest/vitest.mjs", "list", "--json", "--staticParse=false"), "desktop"),
    "playwright": (("node", "node_modules/@playwright/test/cli.js", "test", "--list", "--reporter=json"), "desktop"),
}


class ListingError(Exception):
    """A listing that full mode cannot read or make."""


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


# Reading Rust as code (fast mode).

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


def cfg_value(predicate):
    """True, False or None (fast mode cannot tell) for a cfg predicate in a Windows test build."""
    tokens = [next(group for group in match.groups() if group) for match in CFG_TOKEN.finditer(predicate) if any(match.groups())]
    position = 0

    def parse():
        nonlocal position
        name = tokens[position]
        position += 1
        if tokens[position:position + 1] == ["="]:
            value = tokens[position + 1]
            position += 2
            return value == f'"{CFG_VALUES[name]}"' if name in CFG_VALUES and value.startswith('"') else None
        if tokens[position:position + 1] == ["("]:
            position += 1
            items = []
            while tokens[position] != ")":
                items.append(parse())
                if tokens[position] == ",":
                    position += 1
            position += 1
            if name == "not" and len(items) == 1:
                return None if items[0] is None else not items[0]
            if name == "all":
                return False if False in items else None if None in items else True
            if name == "any":
                return True if True in items else None if None in items else False
            return None
        return CFG_NAMES.get(name)

    try:
        value = parse()
    except (IndexError, KeyError):
        return None
    return value if position == len(tokens) else None


def cfg_off(text, code, start, end):
    """Why the attributes in text[start:end] turn a test off: the first cfg there that is not true for a
    Windows test build. None when every cfg there is true."""
    for attribute in ATTRIBUTE.finditer(code, start, end):
        written = text[attribute.start():attribute.end()]
        cfg = CFG_ATTRIBUTE.match(written)
        value = cfg_value(cfg.group(1)) if cfg else True
        if value is False:
            return f"`{' '.join(written.split())}` turns it off in a Windows test build"
        if value is None:
            return f"fast mode cannot evaluate `{' '.join(written.split())}`; run --full"
    return None


def closing(code, start, opening, closing_character):
    """The index after the bracket that closes the one at code[start]."""
    depth = 0
    for index in range(start, len(code)):
        if code[index] == opening:
            depth += 1
        elif code[index] == closing_character:
            depth -= 1
            if depth == 0:
                return index + 1
    return len(code)


def rust_tests(text):
    """(tests that run, ignored tests, {test that a cfg turns off: why}) of one Rust file."""
    code = rust_code(text)
    file_off = next((off for match in FILE_CFG.finditer(code) if (off := cfg_off(text, code, match.start(), match.end()))), None)
    blocks = []
    for match in RUST_MOD.finditer(code):
        off = cfg_off(text, code, match.start(1), match.end(1))
        if off:
            blocks.append((match.end() - 1, closing(code, match.end() - 1, "{", "}"), off))
    runs, ignored, off_tests = set(), set(), {}
    for match in RUST_ITEM.finditer(code):
        attributes = code[match.start(1):match.end(1)]
        if not TEST_ATTRIBUTE.search(attributes):
            continue
        name = match.group(2)
        off = file_off or cfg_off(text, code, match.start(1), match.end(1))
        off = off or next((cfg for start, end, cfg in blocks if start < match.start() < end), None)
        if off:
            off_tests[name] = off
        elif IGNORE_ATTRIBUTE.search(attributes):
            ignored.add(name)
        else:
            runs.add(name)
    return runs, ignored, off_tests


# Reading JavaScript and TypeScript (fast mode).

def script_strings(text):
    """(code, strings): the text with every comment and the inside of every string and template literal
    blanked to spaces (offsets kept), and {offset of an opening quote: the literal's text}."""
    out = list(text)
    strings = {}
    size = len(text)

    def blank(start, end):
        for position in range(start, min(end, size)):
            if out[position] != "\n":
                out[position] = " "

    index = 0
    while index < size:
        if text[index] in "\"'`":
            quote, end = text[index], index + 1
            while end < size and text[end] != quote and (quote == "`" or text[end] != "\n"):
                end += 2 if text[end] == "\\" else 1
            strings[index] = text[index + 1:end]
            blank(index + 1, end)
            index = end + 1
        elif text.startswith("//", index):
            end = text.find("\n", index)
            end = size if end < 0 else end
            blank(index, end)
            index = end
        elif text.startswith("/*", index):
            end = text.find("*/", index + 2)
            end = size if end < 0 else end + 2
            blank(index, end)
            index = end
        else:
            index += 1
    return "".join(out), strings


def frontend_test_runs(text, title):
    """Fast mode: the file has a `test(` or `it(` call titled `title` that is not skipped (module text)."""
    code, strings = script_strings(text)
    if ONLY.search(code):
        return False
    off = []
    for match in GROUP_OFF.finditer(code):
        start = position = match.end()
        while True:
            while position < len(code) and code[position].isspace():
                position += 1
            if position >= len(code) or code[position] != "(":
                break
            position = closing(code, position, "(", ")")
        off.append((start, position))
    return any(
        strings.get(match.end()) == title and not any(start <= match.start() < end for start, end in off)
        for match in FRONTEND_CALL.finditer(code)
    )


# Workflows (both modes).

def turned_off(item):
    """A job or a step whose `if` is the literal false (FV-15). Any other condition counts as on:
    the check reads command text, and does not prove that a step runs."""
    condition = item.get("if")
    return condition is False or (isinstance(condition, str) and condition.strip().lower() in ("false", "${{ false }}", "${{false}}"))


def workflow_jobs(text):
    """{job id: commands} of a workflow, read with PyYAML: the keys of its `jobs` mapping, and for each
    job the `run` values of its steps, without their whole-line shell comments. A job or a step that
    `if: false` turns off gives no command."""
    document = yaml.safe_load(text)
    jobs = document.get("jobs") if isinstance(document, dict) else None
    found = {}
    for job, body in (jobs.items() if isinstance(jobs, dict) else ()):
        steps = body.get("steps") if isinstance(body, dict) and not turned_off(body) else None
        runs = [step["run"] for step in (steps if isinstance(steps, list) else ()) if isinstance(step, dict) and isinstance(step.get("run"), str) and not turned_off(step)]
        found[str(job)] = "\n".join(line for run in runs for line in run.split("\n") if not line.strip().startswith("#"))
    return found


# Full mode: the listings.

def rust_listing(text):
    """The test names of a `cargo test -- --list` output: the last path part of each `<path>: test`
    line. Doc tests (`<file> - <item> (line N): test`) are left out."""
    return {line[:-len(": test")].rsplit("::", 1)[-1].strip() for line in lines_of(text) if line.endswith(": test") and " - " not in line}


def vitest_listing(text):
    """[(file, full name)] of a `vitest list --json --staticParse=false` output: the tests that would run."""
    return [(Path(entry["file"]).as_posix(), entry["name"]) for entry in json.loads(text)]


def playwright_listing(text):
    """[(file, title path)] of the tests in a `playwright test --list --reporter=json` output whose
    expected status is not "skipped"."""
    report = json.loads(text)
    root = report.get("config", {}).get("rootDir", "")
    found = []

    def walk(suite, titles):
        for spec in suite.get("specs", []):
            if any(test.get("expectedStatus") != "skipped" for test in spec.get("tests", [])):
                found.append((Path(root, spec["file"]).as_posix(), titles + [spec["title"]]))
        for child in suite.get("suites", []):
            walk(child, titles + [child["title"]])

    for suite in report.get("suites", []):
        walk(suite, [])
    return found


def listed_test(entries, runner, path, title):
    """Whether a runner's listing holds a test of `path` (relative to the repository) titled `title`."""
    for file, name in entries:
        if not file.endswith("/" + path):
            continue
        if runner == "vitest" and (name == title or name.endswith(" > " + title)):
            return True
        if runner == "playwright" and name and name[-1] == title:
            return True
    return False


class Listings:
    """Full mode: each runner's listing, read from its file or made by its command, once, when first needed."""

    def __init__(self, files):
        self.files = files
        self.texts = {}

    def text(self, kind):
        if kind not in self.texts:
            if self.files.get(kind):
                try:
                    self.texts[kind] = Path(self.files[kind]).read_text(encoding="utf-8")
                except OSError as error:
                    raise ListingError(f"the {kind} listing {self.files[kind]} cannot be read: {error}") from None
            else:
                command, folder = LISTINGS[kind]
                try:
                    done = subprocess.run(command, cwd=folder, stdout=subprocess.PIPE)
                except OSError as error:
                    raise ListingError(f"`{' '.join(command)}` cannot start in {folder}: {error}") from None
                if done.returncode != 0:
                    raise ListingError(f"`{' '.join(command)}` failed in {folder} with exit {done.returncode}; pass --{kind}-list with a listing file instead")
                self.texts[kind] = done.stdout.decode("utf-8", errors="replace")
        return self.texts[kind]

    def rust(self):
        """(tests that run, ignored tests) of the two cargo listings."""
        listed, ignored = rust_listing(self.text("rust")), rust_listing(self.text("rust-ignored"))
        if not listed:
            raise ListingError("the Rust listing names no test; it must be the output of `cargo test --workspace -- --list`")
        return listed - ignored, ignored

    def frontend(self, runner, path, title):
        try:
            entries = vitest_listing(self.text(runner)) if runner == "vitest" else playwright_listing(self.text(runner))
        except (ValueError, KeyError, TypeError) as error:
            raise ListingError(f"the {runner} listing is not the JSON that the runner writes: {error}") from None
        return listed_test(entries, runner, path, title)


@dataclass
class Inventory:
    """The Rust tests that the checked tree runs, ignores or turns off, and how to ask whether a runner
    runs a frontend test. Fast mode fills it from the sources; full mode from the listings."""

    runs: set[str] = field(default_factory=set)
    ignored: set[str] = field(default_factory=set)
    off: dict[str, str] = field(default_factory=dict)
    listings: Listings | None = None


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


def proof_problems(name, proof, inventory, tracked, read, exists):
    """Why proof `name` does not resolve to files, a job, a command and a test that exist and run; [] when it does."""
    found = []
    for path in proof.files:
        if path not in tracked or not exists(path):
            found.append(f"proof `{name}` needs the file {path}, which is not in the checked tree")
    if proof.workflow:
        if proof.workflow not in tracked or not exists(proof.workflow):
            return found + [f"proof `{name}` needs the workflow {proof.workflow}, which is not in the checked tree"]
        try:
            jobs = workflow_jobs(read(proof.workflow))
        except yaml.YAMLError as error:
            return found + [f"proof `{name}` needs the workflow {proof.workflow}, which PyYAML cannot read: {' '.join(str(error).split())}"]
        if proof.job not in jobs:
            return found + [f"proof `{name}` needs the job `{proof.job}` in the jobs of {proof.workflow}"]
        for command in proof.runs:
            if command not in jobs[proof.job]:
                found.append(f"proof `{name}` needs a run step of job `{proof.job}` that holds {command!r}")
    where = "the cargo listing" if inventory.listings else "the Rust sources"
    if proof.rust_test and proof.rust_test not in (inventory.ignored if proof.ignored else inventory.runs):
        kind = "an ignored test" if proof.ignored else "a test"
        found.append(f"proof `{name}` runs `{proof.rust_test}`, which is not {kind} fn in {where}")
    if proof.title and not found and proof.files[0] in tracked and exists(proof.files[0]):
        if inventory.listings:
            runs = inventory.listings.frontend(proof.runner, proof.files[0], proof.title)
            how = f"in the {proof.runner} listing"
        else:
            runs = frontend_test_runs(read(proof.files[0]), proof.title)
            how = "outside comments, strings and skipped groups"
        if not runs:
            found.append(f"proof `{name}` needs a test( or it( call titled {proof.title!r} in {proof.files[0]}, {how}")
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


def claim_problems(text, inventory, names, read, exists):
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
                found.extend(f"{where}: {problem}" for problem in proof_problems(item, PROOFS[item], inventory, tracked, read, exists))
            elif item in inventory.ignored:
                found.append(f"{where}: evidence `{item}` is an ignored test, which `cargo test` does not run; name the proof that runs it")
            elif item in inventory.off:
                found.append(f"{where}: evidence `{item}` is a test, but {inventory.off[item]}")
            elif item not in inventory.runs:
                found.append(f"{where}: evidence `{item}` is neither a test fn nor a proof name")
        if not cells[3]:
            found.append(f"{where}: the limit cell is empty")
    if any(line.startswith("| Proof name |") for line in lines_of(text)):
        found.extend(named_proof_problems(text))
    return found, len(rows)


def problems(rev, copy=None, listings=None):
    names, read, exists = source(rev, copy)
    functions = set()
    inventory = Inventory(listings=listings)
    custom = harness_free_tests(names, read)
    for path in names:
        if path.endswith(".rs") and exists(path):
            text = read(path)
            declared = RUST_FN.findall(rust_code(text))
            functions.update(declared)
            if path in custom:
                inventory.runs.update(declared)
            elif listings is None:
                runs, ignored, off = rust_tests(text)
                inventory.runs |= runs
                inventory.ignored |= ignored
                inventory.off.update(off)
    if listings is not None:
        runs, ignored = listings.rust()
        inventory.runs |= runs
        inventory.ignored |= ignored
    tests = inventory.runs | inventory.ignored
    found = []
    for path in names:
        if not path.endswith(".md") or path.startswith(EXEMPT_PREFIXES) or not exists(path):
            continue
        for number, line in enumerate(lines_of(read(path)), 1):
            for match in CONTROL.finditer(line):
                found.append(f"{path}:{number}: control character 0x{ord(match.group()):02x}")
            if path == THIRD_PARTY_TEXTS or HISTORICAL.search(line):
                continue
            for name in TEST_NAME.findall(line):
                if name not in functions:
                    found.append(f"{path}:{number}: names `{name}`, which is not a fn in the Rust sources")
                elif name not in tests and SAYS_TEST.search(line):
                    found.append(f"{path}:{number}: names `{name}`, which is a fn but not a test")
    rows = 0
    if CLAIMS in names and exists(CLAIMS):
        claim_found, rows = claim_problems(read(CLAIMS), inventory, names, read, exists)
        found.extend(claim_found)
    return found, rows


def main():
    parser = argparse.ArgumentParser(description="Check the docs' test names, control characters and claims (module text).")
    parser.add_argument("--rev", help="check a commit instead of the working tree (fast mode only)")
    parser.add_argument("--full", action="store_true", help="resolve tests through the test runners' own listings")
    for kind in LISTINGS:
        parser.add_argument(f"--{kind}-list", metavar="FILE", help=f"the {kind} listing to read instead of running `{' '.join(LISTINGS[kind][0])}`")
    arguments = parser.parse_args()
    files = {kind: getattr(arguments, f"{kind.replace('-', '_')}_list") for kind in LISTINGS}
    if any(files.values()) and not arguments.full:
        parser.error("a listing file needs --full")
    if arguments.full and arguments.rev:
        parser.error("--full reads the working tree and its build; it cannot check --rev")
    if yaml is None:
        print(f"RESULT: FAIL (the check reads the workflows with PyYAML, which is not installed: python -m pip install {PYYAML})")
        return 1
    rev = arguments.rev
    listings = Listings(files) if arguments.full else None
    try:
        if rev is None:
            found, rows = problems(None, listings=listings)
        else:
            with git_child.source_copy(Path.cwd()) as copy:
                found, rows = problems(rev, copy)
    except (git_child.GitError, ListingError) as error:
        print(f"RESULT: FAIL ({error})")
        return 1
    for line in found:
        print(line)
    if not arguments.full:
        print(FAST_NOTE)
    mode = "full mode" if arguments.full else "fast mode"
    print(f"RESULT: {'FAIL' if found else 'PASS'} ({len(found)} problems at {rev or 'the working tree'}; {rows} claims checked; {mode})")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
