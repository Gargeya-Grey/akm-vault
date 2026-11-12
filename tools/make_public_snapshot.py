"""Export the tracked tree of one commit as a new repository with one commit and no history (FR-26).

Usage: python tools/make_public_snapshot.py --rev <commit> --scan-only
       python tools/make_public_snapshot.py --rev <commit> --out <new folder> --author "Name <address>"

Git history keeps every file that a later commit removed, so the private repository cannot be made
public as it is. This tool makes the public start instead: the files of one commit, in one new commit
without a parent.

Before it writes anything, it scans every tracked file of that commit, line by line, for:

- a user-profile path of this computer (\\Users\\<name> or /home/<name>, where <name> is the current
  Windows user or a profile folder that exists here);
- a drive path of this computer: a path on a drive other than C: that exists here, or a C: path
  whose first folder exists here (other than Windows, Program Files, ProgramData, Users, tmp and
  temp). A synthetic test path such as C:\\fake\\vault.kdbx is neither. So run the scan on the
  computer whose paths must not leak;
- an e-mail address, other than one at example.com, example.org or example.net;
- the name and the e-mail address in the repository's git config, and the Windows user name;
- each extra pattern in the patterns file (default .scratch/public-snapshot-patterns.txt, which Git
  ignores, so the private words are never tracked). One pattern per line, matched as text without
  case; a line that starts with `re:` is a regular expression; `#` starts a comment.

A line that holds `public-snapshot: allow` is not scanned (for example a synthetic path in a test).
THIRD_PARTY_NOTICES.md is not scanned for e-mail addresses: it holds the published copyright lines of
third-party packages, and `generate_third_party_notices.py --check` proves that it is exactly the
generated text. Every other rule applies to it.

On any hit it prints file, line and rule, never the matched text, and exits 1 without writing.
It refuses a patterns file that Git tracks, an output folder that exists, and an output folder
inside the source repository. It reads the source repository with `git ls-tree` and `git cat-file`
only, and never changes it. It writes only into the new folder that it creates, adds no remote and
never pushes. After the commit it checks that the new tree is the source commit's tree, object for
object, and that the new repository has exactly one commit.
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path

ALLOW_MARK = "public-snapshot: allow"
DEFAULT_PATTERNS = Path(".scratch") / "public-snapshot-patterns.txt"
EMAIL_EXEMPT_FILES = {"THIRD_PARTY_NOTICES.md"}
PROFILE = re.compile(r"(?i)(?:\b[a-z]:)?[\\/]+(?:users|home)[\\/]+([^\\/\s\"'`<>|*?]+)")
GENERIC_PROFILES = {"public", "default", "default user", "all users"}
# A letter after a backslash is an escape (`\d:\d` in a regular expression), not a drive.
DRIVE = re.compile(r"(?i)(?<![a-z0-9\\])([a-z]):[\\/]+([^\\/\s\"'`<>|*?]+)")
# Folders that every Windows computer has or that say nothing about one computer.
SYSTEM_FOLDERS = {"windows", "program files", "program files (x86)", "programdata", "users", "tmp", "temp"}
EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.([A-Za-z]{2,})\b")
EMAIL_ALLOWED = re.compile(r"(?i)@example\.(com|org|net)$")
# `icons/128x128@2x.png` is a file name, not an address.
FILE_ENDINGS = {"png", "jpg", "jpeg", "gif", "svg", "ico", "icns", "webp", "bmp", "json", "js", "mjs", "ts", "tsx", "css", "html", "md", "txt", "rs", "toml", "lock"}


def git(repo, *args, data=None):
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, check=True, input=data).stdout


def tracked(repo, rev):
    """(mode, blob id, path) of every entry in the commit's tree."""
    entries = []
    for record in git(repo, "ls-tree", "-r", "-z", "--full-tree", rev).split(b"\0"):
        if not record:
            continue
        meta, path = record.split(b"\t", 1)
        mode, kind, blob = meta.decode().split()
        entries.append((mode, kind, blob, path.decode("utf-8")))
    return entries


def identity_patterns(repo):
    """The repository user's own name and address, and the Windows user name: (rule, compiled) pairs."""
    found = []
    for key, rule in (("user.email", "the git user's e-mail address"), ("user.name", "the git user's name")):
        result = subprocess.run(["git", "-C", str(repo), "config", key], capture_output=True)
        value = result.stdout.decode("utf-8", errors="replace").strip()
        if len(value) >= 4:
            found.append((rule, re.compile(re.escape(value), re.IGNORECASE)))
    user = os.environ.get("USERNAME", "")
    if len(user) >= 4:
        found.append(("the Windows user name", re.compile(r"(?<![A-Za-z0-9])" + re.escape(user) + r"(?![A-Za-z0-9])", re.IGNORECASE)))
    return found


def extra_patterns(repo, path):
    if path is None:
        return []
    if not path.is_file():
        try:
            shown = path.resolve().relative_to(Path(repo).resolve()).as_posix()
        except ValueError:
            shown = path.name
        print(f"NOTE: no extra patterns file at {shown}; only the built-in rules apply.")
        return []
    inside = path.resolve()
    try:
        relative = inside.relative_to(Path(repo).resolve())
        listed = git(repo, "ls-files", "--", str(relative).replace("\\", "/")).strip()
        if listed:
            raise SystemExit(f"REFUSED: the patterns file {relative} is tracked by Git, so its private words would be published. Nothing was written.")
    except ValueError:
        pass
    patterns = []
    for number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        rule = f"extra pattern {number} of the patterns file"
        if line.startswith("re:"):
            patterns.append((rule, re.compile(line[3:], re.IGNORECASE)))
        else:
            patterns.append((rule, re.compile(re.escape(line), re.IGNORECASE)))
    return patterns


def is_real_profile(name):
    """A profile name is this machine's when it is the current user's, or its folder exists here."""
    lowered = name.lower()
    if lowered in GENERIC_PROFILES:
        return False
    if lowered == os.environ.get("USERNAME", "").lower():
        return True
    return Path(os.environ.get("SystemDrive", "C:") + "\\", "Users", name).is_dir()


def is_machine_path(letter, first):
    """A drive path is one machine's when it names a drive other than C: that exists here, or a C:
    folder that exists here. A synthetic test path such as C:\\fake\\vault.kdbx or Z:\\no\\such is neither."""
    if letter.upper() != "C":
        return Path(f"{letter}:\\").exists()
    if first.lower() in SYSTEM_FOLDERS:
        return False
    return Path(f"C:\\{first}").exists()


def scan_line(path, line, patterns):
    rules = []
    if ALLOW_MARK in line:
        return rules
    if any(is_real_profile(match.group(1)) for match in PROFILE.finditer(line)):
        rules.append("a user-profile path of this computer")
    if any(is_machine_path(match.group(1), match.group(2)) for match in DRIVE.finditer(line)):
        rules.append("a drive path of this computer (a drive other than C:, or a folder that exists here)")
    if path not in EMAIL_EXEMPT_FILES:
        for match in EMAIL.finditer(line):
            if not EMAIL_ALLOWED.search(match.group()) and match.group(1).lower() not in FILE_ENDINGS:
                rules.append("an e-mail address")
                break
    for rule, pattern in patterns:
        if pattern.search(line):
            rules.append(rule)
    return rules


def scan(repo, rev, patterns):
    hits = []
    entries = tracked(repo, rev)
    for mode, kind, blob, path in entries:
        if kind != "blob":
            hits.append((path, 0, f"an unsupported tree entry ({kind})"))
            continue
        text = git(repo, "cat-file", "blob", blob).decode("utf-8", errors="replace")
        for number, line in enumerate(text.split("\n"), 1):
            for rule in scan_line(path, line, patterns):
                hits.append((path, number, rule))
        if mode == "120000":
            hits.append((path, 0, "a symbolic link"))
    return entries, hits


def write_snapshot(repo, rev, entries, out, author):
    out.mkdir()
    for mode, _kind, blob, path in entries:
        target = out / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(git(repo, "cat-file", "blob", blob))
    date = git(repo, "show", "-s", "--format=%cI", rev).decode().strip()
    source = git(repo, "rev-parse", rev).decode().strip()
    match = re.fullmatch(r"\s*(.+?)\s*<([^<>]+)>\s*", author)
    if not match:
        raise SystemExit("REFUSED: --author must be 'Name <address>'.")
    env = dict(os.environ, GIT_AUTHOR_NAME=match.group(1), GIT_AUTHOR_EMAIL=match.group(2), GIT_COMMITTER_NAME=match.group(1),
               GIT_COMMITTER_EMAIL=match.group(2), GIT_AUTHOR_DATE=date, GIT_COMMITTER_DATE=date)
    run = lambda *args: subprocess.run(["git", "-C", str(out), *args], capture_output=True, check=True, env=env)
    run("init", "-q")
    run("config", "core.autocrlf", "false")
    run("config", "core.safecrlf", "false")
    run("add", "-A")
    for mode, _kind, _blob, path in entries:
        if mode == "100755":
            run("update-index", "--chmod=+x", "--", path)
    run("commit", "-q", "--no-verify", "--no-gpg-sign", "-m", "Public snapshot\n\nThe tracked files of one commit of the private repository, without its history.")
    return source


def verify_snapshot(repo, rev, out):
    expected = git(repo, "rev-parse", f"{rev}^{{tree}}").decode().strip()
    actual = git(out, "rev-parse", "HEAD^{tree}").decode().strip()
    commits = git(out, "rev-list", "--all", "--count").decode().strip()
    parents = git(out, "rev-list", "--parents", "-n", "1", "HEAD").decode().split()
    remotes = git(out, "remote").decode().strip()
    problems = []
    if actual != expected:
        problems.append(f"the new tree {actual} is not the source tree {expected}")
    if commits != "1" or len(parents) != 1:
        problems.append(f"the new repository has {commits} commits")
    if remotes:
        problems.append("the new repository has a remote")
    return expected, problems


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--rev", required=True, help="the commit to export")
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parent.parent, help="the source repository")
    parser.add_argument("--out", type=Path, help="the new folder to create; it must not exist")
    parser.add_argument("--author", help="the author of the new commit, 'Name <address>'")
    parser.add_argument("--patterns", type=Path, help=f"the extra patterns file (default: <repo>/{DEFAULT_PATTERNS.as_posix()})")
    parser.add_argument("--scan-only", action="store_true", help="scan and report; write nothing")
    arguments = parser.parse_args()
    repo = arguments.repo.resolve()

    if not arguments.scan_only:
        if arguments.out is None or arguments.author is None:
            raise SystemExit("REFUSED: give --out and --author, or --scan-only. Nothing was written.")
        out = arguments.out.resolve()
        if out.exists():
            raise SystemExit(f"REFUSED: {out} exists. The tool writes only into a new folder that it creates. Nothing was written.")
        if out == repo or repo in out.parents:
            raise SystemExit(f"REFUSED: {out} is inside the source repository {repo}. Nothing was written.")
        if not out.parent.is_dir():
            raise SystemExit(f"REFUSED: the parent folder {out.parent} does not exist. Nothing was written.")

    patterns = identity_patterns(repo) + extra_patterns(repo, arguments.patterns or repo / DEFAULT_PATTERNS)
    entries, hits = scan(repo, arguments.rev, patterns)
    for path, number, rule in hits:
        print(f"{path}:{number}: {rule}")
    files = len({path for path, _number, _rule in hits})
    if hits:
        print(f"RESULT: REFUSED ({len(hits)} hits in {files} files of {len(entries)}; nothing was written)")
        return 1
    if arguments.scan_only:
        print(f"RESULT: CLEAN ({len(entries)} files scanned at {arguments.rev})")
        return 0
    source = write_snapshot(repo, arguments.rev, entries, out, arguments.author)
    tree, problems = verify_snapshot(repo, arguments.rev, out)
    if problems:
        print(f"RESULT: FAIL ({'; '.join(problems)}). The folder {out} stays for inspection.")
        return 1
    print(f"RESULT: WROTE {out}: one commit, tree {tree} of {source}, {len(entries)} files, no remote. Nothing was pushed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
