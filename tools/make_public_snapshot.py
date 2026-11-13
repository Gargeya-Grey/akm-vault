"""Export the tracked tree of one commit as a new repository with one commit and no history (FR-26).

Usage: python tools/make_public_snapshot.py --rev <commit> --scan-only
       python tools/make_public_snapshot.py --rev <commit> --out <new folder> --author "Name <address>"
       python tools/make_public_snapshot.py --rev <commit> --suggest-opaque

Git history keeps every file that a later commit removed, so the private repository cannot be made
public as it is. This tool makes the public start instead: the files of one commit, in one new commit
without a parent.

WHAT THE SCAN READS. A file is published only when the tool can read all of it:

- a text file: its bytes are strict UTF-8 (one leading UTF-8 byte order mark is allowed), it holds no
  NUL byte, and it is at most 8 MiB. Every line is scanned;
- an opaque file: an image, a font, a test vault, any file that is not such text, or a text file
  larger than 8 MiB. It is published only when tools/public_snapshot/opaque-files.txt in the same
  commit lists its path and its SHA-256 ("<sha256>  <path>", one per line). The owner checks the
  file before listing it. The tool cannot read inside it.

Every other file refuses the snapshot: "cannot be scanned". So do an opaque-list entry whose SHA-256
differs from the file (stale), an entry for a file that the commit does not hold, and an entry for a
file that is text (the text is scanned instead). --suggest-opaque prints the lines for the files
that cannot be scanned, for the owner to check; it writes nothing.

THE RULES. Each rule has an ID. Each applies to every line of every text file and to every path:

- profile-path: a user-profile path of this computer (\\Users\\<name> or /home/<name>, where <name>
  is the current Windows user or a profile folder that exists here);
- drive-path: a drive path of this computer: a path on a drive other than C: that exists here, or a
  C: path whose first folder exists here (other than Windows, Program Files, ProgramData, Users, tmp
  and temp). A synthetic test path such as C:\\fake\\vault.kdbx is neither. So run the scan on the
  computer whose paths must not leak;
- email: an e-mail address, with an internationalized local part or domain too, in any case, with
  a plus tag or a subdomain, and a punycode (xn--) top-level domain. No domain and no ending is
  exempt. An exact string in tools/public_snapshot/public-addresses.txt (written by hand: a public
  address, or a name that looks like one, such as an icon `128x128@2x.png`) or in
  tools/public_snapshot/notice-addresses.txt (written by tools/generate_third_party_notices.py and
  checked by its --check: the third-party addresses of THIRD_PARTY_NOTICES.md) is accepted. Both
  lists are read from the same commit. No file is exempt by its name;
- identity: the name and the e-mail address in the repository's git config, and the Windows user
  name;
- extra: each pattern in the patterns file (default .scratch/public-snapshot-patterns.txt, which Git
  ignores, so the private words are never tracked). One pattern per line, matched as text without
  case; a line that starts with `re:` is a regular expression; `#` starts a comment.

And these entries refuse the snapshot: a symbolic link, a submodule, a path that is not UTF-8 or that
Windows cannot write (a reserved name, a character such as `:`, a trailing dot), and two paths that
differ only in case.

THE ALLOW MARKER. `public-snapshot: allow <rule ID>[, <rule ID>]` on a line silences the named rules
on that line, and nothing else. It can name email, profile-path and drive-path. It never silences
identity or extra, it never applies to a path, and a marker that names none of the three silences
nothing. Use it for synthetic data in a test.

WHAT THE SCAN CANNOT PROMISE. It is not a secrets scanner. It finds only the shapes above and the
owner's words. It does not decode text inside text (base64, percent-encoding `%40`, HTML entities),
an address split over two lines, an obfuscated address ("name at domain dot com"), a quoted local
part, an address at an IP literal or at a one-label domain. It cannot read inside an opaque file;
the owner's check before listing it is the only check. It trusts the lists of the commit; CI's
--check shows that notice-addresses.txt is the generator's output.

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
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import git_child  # noqa: E402  (every Git call goes through it: class F, finding U4-R1-01)

DEFAULT_PATTERNS = Path(".scratch") / "public-snapshot-patterns.txt"
LISTS = "tools/public_snapshot"
OPAQUE_LIST = f"{LISTS}/opaque-files.txt"
PUBLIC_LIST = f"{LISTS}/public-addresses.txt"
NOTICE_LIST = f"{LISTS}/notice-addresses.txt"
TEXT_LIMIT = 8 * 1024 * 1024
MESSAGE = "Public snapshot\n\nThe tracked files of one commit of the private repository, without its history.\n"

PROFILE_RULE = "a user-profile path of this computer"
DRIVE_RULE = "a drive path of this computer (a drive other than C:, or a folder that exists here)"
EMAIL_RULE = "an e-mail address"
SILENCEABLE = {"email": EMAIL_RULE, "profile-path": PROFILE_RULE, "drive-path": DRIVE_RULE}
MARK = re.compile(r"public-snapshot:\s*allow\s+([a-z-]+(?:\s*,\s*[a-z-]+)*)")

# Each pattern starts only where its run starts, so a long run of slashes or letters costs linear time.
PROFILE = re.compile(r"(?i)(?<![\\/])[\\/]+(?:users|home)[\\/]+([^\\/\s\"'`<>|*?]+)")
GENERIC_PROFILES = {"public", "default", "default user", "all users"}
# A letter after a backslash is an escape (`\d:\d` in a regular expression), not a drive.
DRIVE = re.compile(r"(?i)(?<![a-z0-9\\])([a-z]):[\\/]+([^\\/\s\"'`<>|*?]+)")
# Folders that every Windows computer has or that say nothing about one computer.
SYSTEM_FOLDERS = {"windows", "program files", "program files (x86)", "programdata", "users", "tmp", "temp"}
# \w is Unicode, so an internationalized local part or domain matches. The top-level domain is letters, or punycode.
EMAIL = re.compile(r"(?<![\w.%+-])[\w.%+-]+@(?:[\w-]+\.)+(?:[^\W\d_]{2,}|xn--[\w-]+)(?![\w-])")
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


def text_of(data):
    """(text, None) for strict UTF-8 text the scan can read, else (None, the reason)."""
    if len(data) > TEXT_LIMIT:
        return None, f"larger than the {TEXT_LIMIT // (1024 * 1024)} MiB scan limit"
    if data.startswith((b"\xff\xfe", b"\xfe\xff", b"\x00\x00\xfe\xff")):
        return None, "a UTF-16 or UTF-32 byte order mark"
    if b"\0" in data:
        return None, "a NUL byte (UTF-16, UTF-32 or binary data)"
    try:
        return data.decode("utf-8").removeprefix("\ufeff"), None
    except UnicodeDecodeError:
        return None, "bytes that are not UTF-8 (another encoding, or binary data)"


def list_lines(entries_by_path, path):
    """(line number, text) of the non-comment lines of a list file in the commit, or [] when absent."""
    entry = entries_by_path.get(path)
    if entry is None or entry.data is None:
        return []
    text, _reason = text_of(entry.data)
    if text is None:
        raise Refused(f"{path} cannot be scanned, so its list cannot be trusted")
    return [(number, line.strip()) for number, line in enumerate(text.split("\n"), 1) if line.strip() and not line.strip().startswith("#")]


# The rules.

def identity_patterns(source):
    """The repository user's own names and addresses, and the Windows user name: (rule, compiled) pairs."""
    found = []
    rules = {"user.email": "the git user's e-mail address", "user.name": "the git user's name"}
    for key, value in dict.fromkeys(source.identity()):
        if len(value.strip()) >= 4:
            found.append((rules[key], re.compile(re.escape(value.strip()), re.IGNORECASE)))
    user = os.environ.get("USERNAME", "")
    if len(user) >= 4:
        found.append(("the Windows user name", re.compile(r"(?<![A-Za-z0-9])" + re.escape(user) + r"(?![A-Za-z0-9])", re.IGNORECASE)))
    return found


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
        patterns.append((rule, re.compile(line[3:] if line.startswith("re:") else re.escape(line), re.IGNORECASE)))
    return patterns


@lru_cache(maxsize=None)
def is_real_profile(name):
    """A profile name is this machine's when it is the current user's, or its folder exists here."""
    lowered = name.lower()
    if lowered in GENERIC_PROFILES:
        return False
    if lowered == os.environ.get("USERNAME", "").lower():
        return True
    return Path(os.environ.get("SystemDrive", "C:") + "\\", "Users", name).is_dir()


@lru_cache(maxsize=None)
def is_machine_path(letter, first):
    """A drive path is one machine's when it names a drive other than C: that exists here, or a C:
    folder that exists here. A synthetic test path such as C:\\fake\\vault.kdbx or Z:\\no\\such is neither."""
    if letter.upper() != "C":
        return Path(f"{letter}:\\").exists()
    if first.lower() in SYSTEM_FOLDERS:
        return False
    return Path(f"C:\\{first}").exists()


def silenced_by(line):
    """The rule IDs that an allow marker on this line names and may silence."""
    found = set()
    for match in MARK.finditer(line):
        found.update(word for word in re.split(r"\s*,\s*", match.group(1)) if word in SILENCEABLE)
    return found


def rules_in(text, patterns, public, silenced=frozenset()):
    """The rules that one line of text, or one path, breaks."""
    found = []
    if "profile-path" not in silenced and any(is_real_profile(match.group(1)) for match in PROFILE.finditer(text)):
        found.append(PROFILE_RULE)
    if "drive-path" not in silenced and any(is_machine_path(match.group(1), match.group(2)) for match in DRIVE.finditer(text)):
        found.append(DRIVE_RULE)
    if "email" not in silenced and any(match.group().casefold() not in public for match in EMAIL.finditer(text)):
        found.append(EMAIL_RULE)
    for rule, pattern in patterns:
        if pattern.search(text):
            found.append(rule)
    return found


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
    """(entries, hits, unscannable). A hit is (file locator, line or 0, rule); unscannable is a list of (entry, reason)."""
    entries = tracked(source, commit)
    read_blobs(source, entries)
    by_path = {entry.path: entry for entry in entries if entry.path is not None}
    patterns = identity_patterns(source) + extra_patterns(patterns_path, by_path, work_tree)
    public = {line.casefold() for _number, line in list_lines(by_path, PUBLIC_LIST) + list_lines(by_path, NOTICE_LIST)}
    opaque = {}
    hits = []
    for number, line in list_lines(by_path, OPAQUE_LIST):
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
        path_rules = rules_in(entry.path, patterns, public)
        entry.locator = path_locator(entry) if path_rules else entry.path
        hits.extend((entry.locator, 0, f"the path holds {rule}") for rule in path_rules)
        if unwritable(entry.path):
            hits.append((entry.locator, 0, "a path that Windows cannot write"))
    folded = {}
    folders = set()
    for entry in entries:
        if entry.path is None:
            continue
        key = entry.path.casefold()
        if key in folded:
            hits.append((entry.locator, 0, f"a path that differs only in case from {folded[key].locator}"))
        folded.setdefault(key, entry)
        parts = key.split("/")
        folders.update("/".join(parts[:end]) for end in range(1, len(parts)))
    for key, entry in folded.items():
        if key in folders:
            hits.append((entry.locator, 0, "a file whose name, without case, is also a folder"))

    unscannable = []
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
        text, reason = text_of(entry.data)
        listed = opaque.pop(entry.path, None) if entry.path is not None else None
        if text is None:
            digest = hashlib.sha256(entry.data).hexdigest()
            if listed is None:
                unscannable.append((entry, reason))
                hits.append((entry.locator, 0, f"cannot be scanned: {reason}; check it, then list its path and SHA-256 in {OPAQUE_LIST}"))
            elif listed[1] != digest:
                hits.append((entry.locator, 0, f"its entry on line {listed[0]} of {OPAQUE_LIST} has another SHA-256 (stale)"))
            continue
        if listed is not None:
            hits.append((entry.locator, 0, f"is text, but line {listed[0]} of {OPAQUE_LIST} lists it as opaque; remove the entry"))
        for number, line in enumerate(text.split("\n"), 1):
            for rule in rules_in(line, patterns, public, silenced_by(line)):
                hits.append((entry.locator, number, rule))
    for path, (number, _digest) in opaque.items():
        hits.append((OPAQUE_LIST, number, "names a file that this commit does not hold"))
    return entries, hits, unscannable


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
    parser.add_argument("--suggest-opaque", action="store_true", help=f"print '<sha256>  <path>' for each file that cannot be scanned, for {OPAQUE_LIST}; write nothing")
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
        entries, hits, unscannable = scan(source, commit, arguments.patterns or work_tree / DEFAULT_PATTERNS, work_tree)
    except (Refused, git_child.GitError) as error:
        print(f"REFUSED: {error}. Nothing was written.")
        return 1

    if arguments.suggest_opaque:
        for entry, reason in unscannable:
            print(f"{hashlib.sha256(entry.data).hexdigest()}  {entry.locator}    # {reason}")
        print(f"RESULT: {len(unscannable)} files cannot be scanned at {arguments.rev}; check each before you list it. Nothing was written.")
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
