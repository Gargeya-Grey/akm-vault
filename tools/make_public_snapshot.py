"""Export the tracked tree of one commit as a new repository with one commit and no history (FR-26).

Usage: python tools/make_public_snapshot.py --rev <commit> --scan-only
       python tools/make_public_snapshot.py --rev <commit> --out <new folder> --author "Name <address>"
       python tools/make_public_snapshot.py --rev <commit> --suggest-opaque

Git history keeps every file that a later commit removed, so the private repository cannot be made
public as it is. This tool makes the public start instead: the files of one commit, in one new commit
without a parent.

PLAIN TEXT. A file is published only when the scan reads all of it as plain text, or when the owner
listed it:

- plain text: strict UTF-8 of at most 8 MiB, in Unicode normalization form NFC, whose characters are
  all printable (str.isprintable: no character of the categories Cc, Cf, Cs, Co, Cn, Zl and Zp, and
  no Zs other than U+0020), apart from TAB and the line ends LF and CR LF. So a lone CR, U+0085,
  U+2028, U+2029, every other control character, a byte order mark, a directional mark, a
  zero-width character, a no-break space, a private-use character and an unassigned code point make
  a file that is not plain text, and lines are split at LF only. The refusal names the rule, the
  line, the column and the code point, never the text;
- a listed file: tools/public_snapshot/opaque-files.txt in the same commit lists its path and its
  SHA-256 ("<sha256>  <path>", one per line), after the owner checked the file. The list is for the
  files that are not plain text: an image, a font, a test vault, a file that is not UTF-8 or is
  larger than 8 MiB ("cannot be scanned"), and a UTF-8 file that is not plain text, such as a test
  fixture with a control character. A listed UTF-8 file is still scanned (THE SCAN VIEW).

Every other file refuses the snapshot. So does a list entry whose SHA-256 differs from the file
(stale), an entry for a file that the commit does not hold, and an entry for a plain-text file.
--suggest-opaque prints the lines for the files that need an entry, for the owner to check; it
writes nothing. A path must be plain text too.

THE SCAN VIEW. The rules read each line of each UTF-8 file, and each path, in two views. The first
view drops every format character (category Cf: a zero-width character, a directional mark, a byte
order mark) and turns every other character that plain text refuses into a space. The second view
is the first in NFKC with its case folded, so a full-width letter, colon or backslash counts as the
plain one. So an address hidden by an invisible character is a hit, also in a listed file.

THE RULES. Each rule has an ID:

- email: a run of characters that only a space or a tab ends, trimmed at both ends of every
  character that is not a letter, a mark or a digit, that holds an at sign with at least one
  character before it and a dot after it. So an address in a URL, in brackets or at any domain is a
  hit. An at sign is a character whose NFKC form holds U+0040 (U+0040, U+FE6B, U+FF20); a dot is
  one whose NFKC form holds U+002E or U+3002 (the full stop, U+FF0E, U+3002, U+FF61 and their other
  look-alikes). One narrow rule: the dot must come before the next `/` after the at sign, because a
  mail domain never holds a `/` (RFC 5321: labels of letters, digits and hyphens, or an IP address in
  brackets). So an npm package address such as registry.npmjs.org/@scope/name/-/name-1.0.0.tgz is no
  hit, and an address in a URL path still is. The hit is the trimmed run;
- profile-path: \\Users\\<name> or /home/<name>, in any case and with either slash, where the name
  runs to the next slash or the end of the line (at most 255 characters). The name, and each part of
  it that ends before a character that is not a letter, a mark or a digit, without its trailing
  spaces and dots, is a hit when it is the current Windows user or a profile folder that exists here;
- drive-path: <letter>:\\ or <letter>:/ that does not follow a letter, a digit or a backslash. A
  drive other than C: that exists here is a hit. On C:, the first folder is read like a profile
  name, and each part of it that exists at the root of C: is a hit, unless it is Windows, Program
  Files, Program Files (x86), ProgramData, Users, tmp or temp. A synthetic test path such as
  C:\\fake\\vault.kdbx is neither, so run the scan on the computer whose paths must not leak;
- identity: the name and the e-mail address in the source's git configuration and in the user's
  global one, and the Windows user name, each of at least 4 characters, anywhere in the second view,
  without case and without word boundaries;
- extra: each pattern in the patterns file (default .scratch/public-snapshot-patterns.txt, which Git
  ignores, so the private words are never tracked). One pattern per line; a line that starts with
  `re:` is a regular expression, any other line is a text; `#` starts a comment. A text is matched
  in the second view, and a regular expression in both views without case.

And these entries refuse the snapshot: a symbolic link, a submodule, a path that is not UTF-8 or that
Windows cannot write (a reserved name, a character such as `:`, a trailing dot), and two paths that
differ only in case.

THE EXCEPTIONS. There is no line marker: no text inside a file silences a rule. The only exceptions
are exact entries for the email rule, one per line, `email <text>`, in two lists of the same commit:
tools/public_snapshot/allowed-texts.txt, which the owner writes after checking each text, and
tools/public_snapshot/notice-addresses.txt, which tools/generate_third_party_notices.py writes from
the e-mail hits of THIRD_PARTY_NOTICES.md (its --check keeps the list current). An entry accepts an
e-mail hit whose text is exactly the entry's text, in the same case; it accepts no hit of another
rule. An entry that is not exactly one e-mail hit, that names another rule or that repeats an entry
of either list is "a malformed entry", and an entry that no e-mail hit outside the two lists
matches is "a stale entry". The lists are scanned like every other file.

WHAT THE SCAN CANNOT PROMISE. The scan is an aid: the owner reads the tree before it becomes public.
It is not a secrets scanner. It finds only the shapes above and the owner's words. It does not decode
text inside text (base64, percent-encoding `%40`, HTML entities, an escape such as `\\u0040`), see an
address split over two lines or two runs, an obfuscated address ("name at domain dot com"), an
address whose at sign is a look-alike that NFKC does not map to U+0040, or an address at a one-label
domain. It cannot read inside a file that is not UTF-8; the owner's check before listing it is the
only check. The character categories come from the Unicode version of the Python that runs it. It
trusts the lists of the commit.

OUTPUT. On a hit it prints a locator and the rule, never the matched text. The locator is
`<path>:<line>`, or `file #<N> (path SHA-256 <12 hex digits>)` when the path itself holds a hit,
where N is the file's position in `git ls-tree -r <commit>`. Then it exits 1 and writes nothing.

READING THE SOURCE. The tool never runs Git on the source repository (findings U4-R1-01, U4-R2-01).
git_child.source_copy reads the source's Git folder as files (for a linked worktree, the folders
that its .git file and commondir name) and copies only the objects, the references without
refs/replace/ and HEAD into a new temporary folder, with a configuration that the tool writes.
git_child.py lists what it leaves out and why: the source's configuration, hooks, alternates,
promisor and partial-clone state, grafts, the shallow file, attributes and every index. Git runs on
that copy only, with lazy fetching off and every transport refused. A revision whose objects are not
all in the copy (a partial clone, an object that only an alternate store holds, a commit past a
shallow cut) is refused, and nothing is fetched. The copy needs as much free disk space as the
source's objects folder, and it is removed at the end, also on failure. The identity rule reads
user.name and user.email from the source's own configuration files and from the user's global
configuration; a value that only an includeIf section gives is not found (put it in the patterns
file).

WRITING. It refuses a patterns file that the commit holds, an output folder that exists, and an
output folder inside the source's work tree or its Git folders. Every Git call goes through
tools/git_child.py, so no inherited GIT_ variable reaches Git, and the new repository sees no user
configuration, template or hook. It writes the files into a new folder `.<out>.incomplete-<id>`
beside --out, builds the commit from the exact list of tracked entries (hash-object, update-index
--index-info, write-tree, commit-tree), so an ignore rule cannot drop a tracked file, and checks
that the new tree is the source commit's tree, that the index holds exactly those entries, that the
work tree holds exactly those files, and that the repository has one commit, no parent and no
remote. Only then does it rename the folder to --out. On any failure it removes the unfinished
folder that it created in this run, so no partial snapshot is left that looks usable. It adds no
remote and never pushes.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import secrets
import shutil
import stat
import sys
import unicodedata
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import git_child  # noqa: E402  (every Git call goes through it: class F, finding U4-R1-01)

DEFAULT_PATTERNS = Path(".scratch") / "public-snapshot-patterns.txt"
LISTS = "tools/public_snapshot"
OPAQUE_LIST = f"{LISTS}/opaque-files.txt"
ALLOWED_LIST = f"{LISTS}/allowed-texts.txt"
NOTICE_LIST = f"{LISTS}/notice-addresses.txt"
EXCEPTION_LISTS = (ALLOWED_LIST, NOTICE_LIST)
TEXT_LIMIT = 8 * 1024 * 1024
NAME_LIMIT = 255
MESSAGE = "Public snapshot\n\nThe tracked files of one commit of the private repository, without its history.\n"

PROFILE_RULE = "a user-profile path of this computer"
DRIVE_RULE = "a drive path of this computer (a drive other than C:, or a folder that exists here)"
EMAIL_RULE = "an e-mail address"
# Plain text refuses every character that str.isprintable refuses, apart from TAB and the line ends.
REFUSED = {
    "Cc": "a control character",
    "Cf": "a format character (for example a zero-width character, a directional mark or a byte order mark)",
    "Zl": "a line separator",
    "Zp": "a paragraph separator",
    "Zs": "a space other than U+0020",
    "Co": "a private-use character",
    "Cn": "an unassigned code point",
}
LONE_CR = "a carriage return that no line feed follows"
NOT_NFC = "text that is not in Unicode normalization form NFC"

# Each pattern starts only where its run starts, so a long run of slashes or letters costs linear time.
PROFILE = re.compile(r"(?i)(?<![\\/])[\\/]+(?:users|home)[\\/]+([^\\/]{1,255})")
GENERIC_PROFILES = {"public", "default", "default user", "all users"}
# A letter after a backslash is an escape (`\d:\d` in a regular expression), not a drive.
DRIVE = re.compile(r"(?i)(?<![a-z0-9\\])([a-z]):[\\/]+([^\\/]{0,255})")
# Folders that every Windows computer has or that say nothing about one computer.
SYSTEM_FOLDERS = {"windows", "program files", "program files (x86)", "programdata", "users", "tmp", "temp"}
RESERVED_NAMES = {"con", "prn", "aux", "nul", *(f"com{n}" for n in range(1, 10)), *(f"lpt{n}" for n in range(1, 10))}
UNWRITABLE_CHARACTERS = set('<>:"\\|?*') | {chr(code) for code in range(32)}


class Refused(Exception):
    pass


class ExportFailed(Exception):
    pass


@dataclass
class Entry:
    index: int
    mode: str
    kind: str
    oid: str
    raw_path: bytes
    path: str | None
    data: bytes | None = None
    locator: str = ""


# Reading the source commit.

def tracked(source, commit):
    """Every entry of the commit's tree, in `git ls-tree -r` order, with its position from 1."""
    entries = []
    for index, record in enumerate((r for r in source.run("ls-tree", "-r", "-z", "--full-tree", commit).split(b"\0") if r), 1):
        meta, raw_path = record.split(b"\t", 1)
        mode, kind, oid = meta.decode("ascii").split()
        try:
            path = raw_path.decode("utf-8")
        except UnicodeDecodeError:
            path = None
        entries.append(Entry(index, mode, kind, oid, raw_path, path))
    return entries


def read_blobs(source, entries):
    """Fill each blob entry's bytes with one `git cat-file --batch` on the source copy."""
    blobs = [entry for entry in entries if entry.kind == "blob"]
    if not blobs:
        return
    output = source.run("cat-file", "--batch", input="".join(f"{entry.oid}\n" for entry in blobs).encode("ascii"))
    position = 0
    for entry in blobs:
        end = output.index(b"\n", position)
        header = output[position:end].decode("ascii").split()
        if len(header) != 3:
            raise Refused(f"the object {entry.oid} is missing from the source repository; the tool fetches nothing")
        oid, kind, size = header
        if oid != entry.oid or kind != "blob":
            raise Refused(f"cat-file returned {oid} {kind} for {entry.oid}")
        entry.data = output[end + 1:end + 1 + int(size)]
        position = end + 1 + int(size) + 1


def decoded(data):
    """(text, None) for strict UTF-8 within the size limit, else (None, why the scan cannot read it)."""
    if len(data) > TEXT_LIMIT:
        return None, f"larger than the {TEXT_LIMIT // (1024 * 1024)} MiB scan limit"
    if data.startswith((b"\xff\xfe", b"\xfe\xff", b"\x00\x00\xfe\xff")):
        return None, "a UTF-16 or UTF-32 byte order mark"
    if b"\0" in data:
        return None, "a NUL byte (UTF-16, UTF-32 or binary data)"
    try:
        return data.decode("utf-8"), None
    except UnicodeDecodeError:
        return None, "bytes that are not UTF-8 (another encoding, or binary data)"


def plain_problem(text):
    """None for plain text (PLAIN TEXT in the module text), else (line, column or 0, the reason)."""
    lines = text.split("\n")
    for number, line in enumerate(lines, 1):
        body = line[:-1] if line.endswith("\r") and number < len(lines) else line
        if body.replace("\t", " ").isprintable():
            continue
        for column, character in enumerate(body, 1):
            if character != "\t" and not character.isprintable():
                reason = LONE_CR if character == "\r" else REFUSED.get(unicodedata.category(character), "a character that is not printable")
                return number, column, f"U+{ord(character):04X}, {reason}"
    if not unicodedata.is_normalized("NFC", text):
        # No composition crosses a line feed, so one line holds the text that is not in NFC.
        return next(((number, 0, NOT_NFC) for number, line in enumerate(lines, 1) if not unicodedata.is_normalized("NFC", line)), (1, 0, NOT_NFC))
    return None


def where(problem):
    line, column, reason = problem
    return f"line {line}, column {column}: {reason}" if column else f"line {line}: {reason}"


def list_lines(entries_by_path, path):
    """(line number, line without its CR LF) of each line of a list file in the commit, or [] when absent."""
    entry = entries_by_path.get(path)
    if entry is None or entry.data is None:
        return []
    text, _reason = decoded(entry.data)
    if text is None or plain_problem(text) is not None:
        raise Refused(f"{path} is not plain text, so its list cannot be trusted")
    return [(number, line.removesuffix("\r")) for number, line in enumerate(text.split("\n"), 1)]


# The scan view and the rules.

def views(text):
    """The two views that the rules read (THE SCAN VIEW in the module text)."""
    if not text.replace("\t", " ").isprintable():
        text = "".join("" if unicodedata.category(c) == "Cf" else c if c == "\t" or c.isprintable() else " " for c in text)
    return text, unicodedata.normalize("NFKC", text).casefold()


def folded(text):
    return views(text)[1]


@lru_cache(maxsize=65536)
def is_at_sign(character):
    """An at sign is a character whose NFKC form holds U+0040: U+0040, U+FE6B and U+FF20 in Unicode 15.1."""
    return "@" in unicodedata.normalize("NFKC", character)


@lru_cache(maxsize=65536)
def is_dot(character):
    """A dot is a character whose NFKC form holds U+002E or U+3002, the label separators of UTS 46 among them."""
    form = unicodedata.normalize("NFKC", character)
    return "." in form or "\u3002" in form


def is_word_character(character):
    return unicodedata.category(character)[0] in "LMN"


def trimmed(run):
    """The run without the characters at either end that are not a letter, a mark or a digit."""
    start, end = 0, len(run)
    while start < end and not is_word_character(run[start]):
        start += 1
    while end > start and not is_word_character(run[end - 1]):
        end -= 1
    return run[start:end]


def email_hits(view):
    """The e-mail hits of a first view: each trimmed run with an at sign that has a character before it
    and a dot after it, before the next `/` (a mail domain never holds a `/`). An at sign is never the
    first character of a trimmed run, so each at sign has a character before it."""
    characters = set(view)
    at_signs = [character for character in characters if is_at_sign(character)]
    dots = [character for character in characters if is_dot(character)]
    if not at_signs or not dots:
        return []
    hits = []
    for run in view.replace("\t", " ").split(" "):
        if not any(sign in run for sign in at_signs):
            continue
        run = trimmed(run)
        for part in run.split("/"):
            found = [index for index in (part.find(sign) for sign in at_signs) if index >= 0]
            if found and max(part.rfind(dot) for dot in dots) > min(found):
                hits.append(run)
                break
    return hits


def name_candidates(name):
    """The name and each part of it that ends before a character that is not a letter, a mark or a digit,
    without trailing spaces and dots, which Windows ignores in a path."""
    name = name[:NAME_LIMIT]
    found = {name[:end].rstrip(" .") for end, character in enumerate(name) if not is_word_character(character)}
    found.add(name.rstrip(" ."))
    found.discard("")
    return found


def is_folder(path):
    try:
        return path.is_dir()
    except (OSError, ValueError):
        return False


@lru_cache(maxsize=None)
def is_real_profile(name):
    """A profile name is this machine's when it is the current user's, or its folder exists here."""
    if name.casefold() in GENERIC_PROFILES:
        return False
    if name.casefold() == folded(os.environ.get("USERNAME", "")):
        return True
    return is_folder(Path(os.environ.get("SystemDrive", "C:") + "\\", "Users", name))


@lru_cache(maxsize=None)
def is_machine_folder(letter, name):
    """A drive path is one machine's when it names a drive other than C: that exists here, or a C:
    folder that exists here. A synthetic test path such as C:\\fake\\vault.kdbx or Z:\\no\\such is neither."""
    if letter.upper() != "C":
        return is_folder(Path(f"{letter}:\\"))
    return name.casefold() not in SYSTEM_FOLDERS and is_folder(Path(f"C:\\{name}"))


def is_machine_path(letter, first):
    if letter.upper() != "C":
        return is_machine_folder(letter.upper(), "")
    return any(is_machine_folder("C", name) for name in name_candidates(first))


def identity_patterns(source):
    """The repository user's own name and address, and the Windows user name: (rule, matcher) pairs."""
    rules = {"user.email": "the git user's e-mail address", "user.name": "the git user's name"}
    values = [(rules[key], value) for key, value in dict.fromkeys(source.identity())]
    values.append(("the Windows user name", os.environ.get("USERNAME", "")))
    return [(rule, text_matcher(folded(value.strip()))) for rule, value in values if len(value.strip()) >= 4]


def text_matcher(text):
    return lambda _view, folded_view: text in folded_view


def pattern_matcher(pattern):
    return lambda view, folded_view: pattern.search(view) is not None or pattern.search(folded_view) is not None


def extra_patterns(path, entries_by_path, work_tree):
    if path is None:
        return []
    inside = path.resolve()
    try:
        relative = inside.relative_to(work_tree).as_posix()
    except ValueError:
        relative = None
    if relative is not None and relative in entries_by_path:
        raise Refused(f"the patterns file {relative} is tracked by Git, so its private words would be published")
    if not path.is_file():
        print(f"NOTE: no extra patterns file at {relative or path.name}; only the built-in rules apply.")
        return []
    patterns = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        rule = f"extra pattern {number} of the patterns file"
        if line.startswith("re:"):
            patterns.append((rule, pattern_matcher(re.compile(line[3:], re.IGNORECASE))))
        elif folded(line):
            patterns.append((rule, text_matcher(folded(line))))
        else:
            raise Refused(f"extra pattern {number} of the patterns file holds no character that the scan can match")
    return patterns


def rules_in(text, patterns, accepted, seen):
    """The rules that one line of text, or one path, breaks. An e-mail hit whose exact text is an entry of
    `accepted` is not a hit; each e-mail hit's text goes into `seen` unless `seen` is None."""
    view, folded_view = views(text)
    found = []
    if any(is_real_profile(name) for each in (view, folded_view) for match in PROFILE.finditer(each) for name in name_candidates(match.group(1))):
        found.append(PROFILE_RULE)
    if any(is_machine_path(match.group(1), match.group(2)) for each in (view, folded_view) for match in DRIVE.finditer(each)):
        found.append(DRIVE_RULE)
    emails = email_hits(view)
    if seen is not None:
        seen.update(emails)
    if any(hit not in accepted for hit in emails):
        found.append(EMAIL_RULE)
    for rule, matcher in patterns:
        if matcher(view, folded_view):
            found.append(rule)
    return found


def exception_entries(entries_by_path, hits):
    """{text: (list, line)} for the valid entries of the two exception lists. A malformed or repeated
    entry is a hit, and accepts nothing."""
    accepted = {}
    for path in EXCEPTION_LISTS:
        for number, line in list_lines(entries_by_path, path):
            if not line.strip() or line.startswith("#"):
                continue
            parts = line.split(" ")
            if len(parts) != 2 or parts[0] != "email" or email_hits(parts[1]) != [parts[1]]:
                hits.append((path, number, "a malformed entry: the form is `email <text>`, and the text is exactly one e-mail hit"))
            elif parts[1] in accepted:
                hits.append((path, number, "a malformed entry: it repeats an earlier entry"))
            else:
                accepted[parts[1]] = (path, number)
    return accepted


def unwritable(path):
    for part in path.split("/"):
        if part in ("", ".", "..") or part.casefold() == ".git" or part.endswith((" ", ".")):
            return True
        if any(character in UNWRITABLE_CHARACTERS for character in part) or part.split(".")[0].casefold() in RESERVED_NAMES:
            return True
    return False


def shown(locator, line):
    """How a hit names its place: <path>:<line>, or the path-free locator and the line."""
    if not line:
        return locator
    return f"{locator}, line {line}" if locator.startswith("file #") else f"{locator}:{line}"


def path_locator(entry):
    return f"file #{entry.index} (path SHA-256 {hashlib.sha256(entry.raw_path).hexdigest()[:12]})"


def scan(source, commit, patterns_path, work_tree):
    """(entries, hits, unlisted). A hit is (file locator, line or 0, rule); unlisted is a list of (entry,
    reason) for the files that are not plain text and that the opaque list does not name."""
    entries = tracked(source, commit)
    read_blobs(source, entries)
    by_path = {entry.path: entry for entry in entries if entry.path is not None}
    patterns = identity_patterns(source) + extra_patterns(patterns_path, by_path, work_tree)
    hits = []
    accepted = exception_entries(by_path, hits)
    seen = set()
    opaque = {}
    for number, line in list_lines(by_path, OPAQUE_LIST):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        match = re.fullmatch(r"([0-9a-f]{64})  (\S.*)", line)
        if not match or match.group(2) in opaque:
            hits.append((OPAQUE_LIST, number, "a malformed or repeated line in the opaque list"))
            continue
        opaque[match.group(2)] = (number, match.group(1))

    for entry in entries:
        if entry.path is None:
            entry.locator = path_locator(entry)
            hits.append((entry.locator, 0, "a path that is not UTF-8"))
            continue
        path_rules = [f"the path holds {rule}" for rule in rules_in(entry.path, patterns, accepted, seen)]
        problem = plain_problem(entry.path)
        if problem is not None:
            path_rules.append(f"a path that is not plain text: column {problem[1]}: {problem[2]}" if problem[1] else f"a path that is not plain text: {problem[2]}")
        entry.locator = path_locator(entry) if path_rules else entry.path
        hits.extend((entry.locator, 0, rule) for rule in path_rules)
        if unwritable(entry.path):
            hits.append((entry.locator, 0, "a path that Windows cannot write"))
    by_case = {}
    folders = set()
    for entry in entries:
        if entry.path is None:
            continue
        key = entry.path.casefold()
        if key in by_case:
            hits.append((entry.locator, 0, f"a path that differs only in case from {by_case[key].locator}"))
        by_case.setdefault(key, entry)
        parts = key.split("/")
        folders.update("/".join(parts[:end]) for end in range(1, len(parts)))
    for key, entry in by_case.items():
        if key in folders:
            hits.append((entry.locator, 0, "a file whose name, without case, is also a folder"))

    unlisted = []
    for entry in entries:
        if entry.kind != "blob":
            hits.append((entry.locator, 0, f"an unsupported tree entry ({entry.kind}, for example a submodule)"))
            continue
        if entry.mode == "120000":
            hits.append((entry.locator, 0, "a symbolic link"))
            continue
        if entry.mode not in ("100644", "100755"):
            hits.append((entry.locator, 0, f"an unsupported file mode {entry.mode}"))
            continue
        text, reason = decoded(entry.data)
        problem = plain_problem(text) if text is not None else None
        listed = opaque.pop(entry.path, None) if entry.path is not None else None
        if listed is not None and text is not None and problem is None:
            hits.append((entry.locator, 0, f"is plain text, but line {listed[0]} of {OPAQUE_LIST} lists it; remove the entry"))
        elif listed is not None and listed[1] != hashlib.sha256(entry.data).hexdigest():
            hits.append((entry.locator, 0, f"its entry on line {listed[0]} of {OPAQUE_LIST} has another SHA-256 (stale)"))
        elif listed is None and text is None:
            unlisted.append((entry, f"cannot be scanned: {reason}"))
            hits.append((entry.locator, 0, f"cannot be scanned: {reason}; check it, then list its path and SHA-256 in {OPAQUE_LIST}"))
        elif listed is None and problem is not None:
            unlisted.append((entry, f"not plain text: {where(problem)}"))
            hits.append((entry.locator, 0, f"not plain text: {where(problem)}; check it, then list its path and SHA-256 in {OPAQUE_LIST}"))
        if text is None:
            continue
        outside = None if entry.path in EXCEPTION_LISTS else seen
        for number, line in enumerate(text.split("\n"), 1):
            hits.extend((entry.locator, number, rule) for rule in rules_in(line, patterns, accepted, outside))
    for path, (number, _digest) in opaque.items():
        hits.append((OPAQUE_LIST, number, "names a file that this commit does not hold"))
    for text, (path, number) in accepted.items():
        if text not in seen:
            hits.append((path, number, "a stale entry: no e-mail hit outside the lists has this text"))
    return entries, hits, unlisted


# Writing the snapshot.

def remove_own(folder):
    """Remove the unfinished folder that this run created under a new unique name (class F).
    Git writes its object files read-only, and Windows refuses to delete a read-only file."""
    def writable_then_retry(function, path, _error):
        os.chmod(path, stat.S_IWRITE | stat.S_IREAD)
        function(path)

    try:
        if sys.version_info >= (3, 12):
            shutil.rmtree(folder, onexc=writable_then_retry)
        else:
            shutil.rmtree(folder, onerror=writable_then_retry)
    except OSError as error:
        print(f"WARNING: could not remove the unfinished folder {folder}: {error}. It is incomplete: do not use it.")


def verify(snapshot, tree, entries, folder):
    problems = []
    if snapshot.run("rev-parse", "HEAD^{tree}").decode().strip() != tree:
        problems.append("the new tree is not the source commit's tree")
    if snapshot.run("rev-list", "--all", "--count").decode().strip() != "1" or len(snapshot.run("rev-list", "--parents", "-n", "1", "HEAD").split()) != 1:
        problems.append("the new repository does not have exactly one commit without a parent")
    if snapshot.run("remote").strip():
        problems.append("the new repository has a remote")
    staged = set()
    for record in (r for r in snapshot.run("ls-files", "-s", "-z").split(b"\0") if r):
        meta, path = record.split(b"\t", 1)
        mode, oid, _stage = meta.decode("ascii").split()
        staged.add((mode, oid, path.decode("utf-8")))
    if staged != {(entry.mode, entry.oid, entry.path) for entry in entries}:
        problems.append("the new index does not hold exactly the tracked entries")
    written = {path.relative_to(folder).as_posix() for path in folder.rglob("*") if path.is_file() and ".git" not in path.relative_to(folder).parts[:1]}
    if written != {entry.path for entry in entries}:
        problems.append("the new work tree does not hold exactly the tracked files")
    return problems


def export(source, commit, tree, entries, out, author):
    """Write the snapshot into a new folder beside `out`, check it, then rename it to `out`."""
    match = re.fullmatch(r"\s*(.+?)\s*<([^<>]+)>\s*", author)
    committer = source.run("cat-file", "commit", commit).decode("utf-8", "replace").split("\n\n", 1)[0]
    stamp = re.search(r"^committer .* (\d+ [+-]\d{4})$", committer, re.MULTILINE)
    who = {"GIT_AUTHOR_NAME": match.group(1), "GIT_AUTHOR_EMAIL": match.group(2), "GIT_COMMITTER_NAME": match.group(1),
           "GIT_COMMITTER_EMAIL": match.group(2), "GIT_AUTHOR_DATE": stamp.group(1), "GIT_COMMITTER_DATE": stamp.group(1)}
    folder = out.parent / f".{out.name}.incomplete-{os.getpid()}-{secrets.token_hex(4)}"
    folder.mkdir()
    try:
        for entry in entries:
            target = folder / entry.path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(entry.data)
        snapshot = git_child.create(folder)
        oids = snapshot.run("hash-object", "-w", "--no-filters", "--stdin-paths", input="".join(f"{entry.path}\n" for entry in entries).encode("utf-8")).decode("ascii").split()
        if oids != [entry.oid for entry in entries]:
            raise ExportFailed("a written file does not hash to its source object")
        snapshot.run("update-index", "-z", "--index-info", input=b"".join(f"{entry.mode} {entry.oid}\t{entry.path}".encode("utf-8") + b"\0" for entry in entries))
        if snapshot.run("write-tree").decode("ascii").strip() != tree:
            raise ExportFailed("the tree written from the tracked entries is not the source commit's tree")
        new_commit = snapshot.run("commit-tree", tree, input=MESSAGE.encode("utf-8"), extra=who).decode("ascii").strip()
        snapshot.run("update-ref", "refs/heads/main", new_commit)
        snapshot.completed("update-index", "-q", "--refresh")
        problems = verify(snapshot, tree, entries, folder)
        if problems:
            raise ExportFailed("; ".join(problems))
        os.rename(folder, out)
        return new_commit
    except BaseException:
        remove_own(folder)
        raise


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--rev", required=True, help="the commit to export")
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parent.parent, help="the source repository")
    parser.add_argument("--out", type=Path, help="the new folder to create; it must not exist")
    parser.add_argument("--author", help="the author of the new commit, 'Name <address>'")
    parser.add_argument("--patterns", type=Path, help=f"the extra patterns file (default: <repo>/{DEFAULT_PATTERNS.as_posix()})")
    parser.add_argument("--scan-only", action="store_true", help="scan and report; write nothing")
    parser.add_argument("--suggest-opaque", action="store_true", help=f"print '<sha256>  <path>' for each file that is not plain text and not listed, for {OPAQUE_LIST}; write nothing")
    arguments = parser.parse_args()
    try:
        folders = git_child.git_folders(arguments.repo)
        with git_child.source_copy(arguments.repo) as source:
            return snapshot(arguments, source, folders)
    except (Refused, git_child.GitError) as error:
        print(f"REFUSED: {error}. Nothing was written.")
        return 1


def snapshot(arguments, source, folders):
    """Scan the commit in the source copy, then report, or write the snapshot. `folders` are the source's
    work tree, Git folder and common Git folder, which the tool never writes into."""
    work_tree = folders[0]
    try:
        commit = source.completed("rev-parse", "--verify", "--quiet", "--end-of-options", f"{arguments.rev}^{{commit}}").stdout.decode("ascii").strip()
        if not commit:
            raise Refused(f"{arguments.rev} is not a commit of {work_tree}, or its commit object is missing from the source repository (a shallow cut, a partial clone or an alternate store); the tool fetches nothing")
        missing = source.missing(commit)
        if missing:
            count = f"{len(missing)} object of {arguments.rev} is" if len(missing) == 1 else f"{len(missing)} objects of {arguments.rev} are"
            raise Refused(f"{count} missing from the source repository's own object store (a partial clone, an object that only an alternate store holds, or damage); the tool fetches nothing")
        tree = source.run("rev-parse", f"{commit}^{{tree}}").decode("ascii").strip()
        writing = not (arguments.scan_only or arguments.suggest_opaque)
        out = None
        if writing:
            if arguments.out is None or arguments.author is None:
                raise Refused("give --out and --author, or --scan-only")
            if not re.fullmatch(r"\s*(.+?)\s*<([^<>]+)>\s*", arguments.author):
                raise Refused("--author must be 'Name <address>'")
            out = arguments.out.resolve()
            if out.exists():
                raise Refused(f"{out} exists. The tool writes only into a new folder that it creates")
            for folder in dict.fromkeys(folders):
                if out == folder or folder in out.parents:
                    raise Refused(f"{out} is inside the source repository's folder {folder}")
            if not out.parent.is_dir():
                raise Refused(f"the parent folder {out.parent} does not exist")
        entries, hits, unlisted = scan(source, commit, arguments.patterns or work_tree / DEFAULT_PATTERNS, work_tree)
    except (Refused, git_child.GitError) as error:
        print(f"REFUSED: {error}. Nothing was written.")
        return 1

    if arguments.suggest_opaque:
        for entry, reason in unlisted:
            print(f"{hashlib.sha256(entry.data).hexdigest()}  {entry.locator}    # {reason}")
        print(f"RESULT: {len(unlisted)} files at {arguments.rev} are not plain text and not listed; check each before you list it. Nothing was written.")
        return 0
    for locator, line, rule in hits:
        print(f"{shown(locator, line)}: {rule}")
    if hits:
        files = len({locator for locator, _line, _rule in hits})
        print(f"RESULT: REFUSED ({len(hits)} hits in {files} files of {len(entries)}; nothing was written)")
        return 1
    if arguments.scan_only:
        print(f"RESULT: CLEAN ({len(entries)} files scanned at {arguments.rev})")
        return 0
    try:
        new_commit = export(source, commit, tree, entries, out, arguments.author)
    except (ExportFailed, git_child.GitError, OSError) as error:
        print(f"RESULT: FAIL ({error}). The tool removed its own unfinished folder; {out} was not created.")
        return 1
    print(f"RESULT: WROTE {out}: one commit {new_commit}, tree {tree} of {commit}, {len(entries)} files, no remote. Nothing was pushed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
