"""Shared fixtures for the tests of tools/make_public_snapshot.py.

Every repository here is a throwaway one in a temporary folder, made through git_child, so no
inherited GIT_ variable reaches Git. Every address is built at run time by `address`, so the scan of
this file meets no address.
"""

import hashlib
import os
import stat
import subprocess
import sys
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
TOOL = TOOLS / "make_public_snapshot.py"
sys.path.insert(0, str(TOOLS))
import git_child  # noqa: E402


class Results:
    def __init__(self):
        self.failures = 0

    def case(self, name, ok, detail=""):
        print(f"{'ok  ' if ok else 'FAIL'} {name}")
        if not ok:
            self.failures += 1
            if detail:
                print(f"     {detail}")

    def finish(self):
        print("all cases passed" if not self.failures else f"{self.failures} case(s) failed")
        return 1 if self.failures else 0


def address(local, domain):
    return f"{local}@{domain}"


PUBLIC_AUTHOR = f"Public Author <{address('public', 'example.com')}>"


def git(repo, *args, data=None, check=True):
    """A Git command in a test repository, through git_child, so no inherited GIT_ variable reaches it."""
    folder = Path(repo).resolve()
    repository = git_child.Repository(folder / ".git", folder, isolated=True)
    if check:
        return repository.run(*args, input=data).decode("utf-8")
    return repository.completed(*args, input=data)


def write_files(repo, files):
    for name, content in files.items():
        (repo / name).parent.mkdir(parents=True, exist_ok=True)
        (repo / name).write_bytes(content if isinstance(content, bytes) else content.encode("utf-8"))


def make_source(root, files, patterns=None, history=None, force=(), cacheinfo=(), name="source"):
    """A source repository whose HEAD holds `files` (text or bytes). `history` files form an earlier
    commit. `force` paths are added past the ignore rules. `cacheinfo` is (mode, content or object id, path)
    entries that the work tree cannot hold, added to the index by plumbing."""
    repo = root / name
    git_child.create(repo)
    git(repo, "config", "core.autocrlf", "false")
    git(repo, "config", "user.name", "Snapshot Tester")
    git(repo, "config", "user.email", address("tester", "example.com"))
    for stage in ([history] if history else []) + [files]:
        for old in [path for path in repo.rglob("*") if path.is_file() and ".git" not in path.parts]:
            old.unlink()
        write_files(repo, stage)
        git(repo, "add", "-A")
        if stage is files:
            for forced in force:
                git(repo, "add", "-f", "--", forced)
            for mode, content, path in cacheinfo:
                oid = content if mode == "160000" else git(repo, "hash-object", "-w", "--stdin", data=content.encode()).strip()
                git(repo, "-c", "core.protectNTFS=false", "update-index", "--add", "--cacheinfo", f"{mode},{oid},{path}")
        git(repo, "commit", "-q", "--no-gpg-sign", "-m", "stage")
    (repo / "untracked-sentinel.txt").write_text("sentinel: the tool must not touch this file\n", encoding="utf-8")
    if patterns is not None:
        (repo / ".scratch").mkdir(exist_ok=True)
        (repo / ".scratch" / "public-snapshot-patterns.txt").write_text(patterns, encoding="utf-8")
    return repo


def fingerprint(folder):
    """Every file under the folder, .git included, by path and SHA-256 of its bytes."""
    return sorted((str(path.relative_to(folder)), hashlib.sha256(path.read_bytes()).hexdigest()) for path in Path(folder).rglob("*") if path.is_file())


def remove_object(repo, oid, git_dir=None):
    """Delete one loose object file (Git writes them read-only)."""
    path = Path(git_dir or repo / ".git") / "objects" / oid[:2] / oid[2:]
    os.chmod(path, stat.S_IWRITE | stat.S_IREAD)
    path.unlink()


def run(repo, *args, env=None, timeout=300, rev="HEAD"):
    """The tool's result. A run that does not end in `timeout` seconds is stopped and fails its case."""
    command = [sys.executable, "-I", str(TOOL), "--repo", str(repo), "--rev", rev, *args]
    try:
        return subprocess.run(command, capture_output=True, text=True, encoding="utf-8", env=env, timeout=timeout)
    except subprocess.TimeoutExpired:
        return subprocess.CompletedProcess(command, -1, f"TIMEOUT: the tool did not finish in {timeout} seconds", "")


def stand_in_helper(base):
    """A folder for PATH that holds `git-remote-akmstandin`, a stand-in remote helper that only appends
    a line to a marker file in `base` and fails. A test shows with the marker that Git did NOT start it."""
    folder = base / "stand-in-bin"
    folder.mkdir()
    marker = base / "stand-in-helper-started.txt"
    (folder / "git-remote-akmstandin").write_bytes(f"#!/bin/sh\necho started >> '{marker.as_posix()}'\nexit 1\n".encode())
    return folder, marker


def marker_script(base, name):
    """A shell script that only appends a line to its own marker file in `base`. Returns (script, marker)."""
    marker = base / f"{name}-ran.txt"
    script = base / f"{name}.sh"
    script.write_bytes(f"#!/bin/sh\necho {name} >> '{marker.as_posix()}'\nexit 1\n".encode())
    return script, marker


def existing_c_folder():
    """A folder at the root of C: that exists here and is not a system folder of every computer."""
    system = {"windows", "program files", "program files (x86)", "programdata", "users", "tmp", "temp"}
    for entry in sorted(Path("C:\\").iterdir(), key=lambda path: path.name.lower()):
        if entry.is_dir() and entry.name.lower() not in system and not entry.name.startswith(("$", ".")) and " " not in entry.name:
            return entry.name
    return None
