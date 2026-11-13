"""Start Git with a clean environment, on a named repository, with no fetch, no transport and no
program of the repository's own (AGENTS.md class F, findings U4-R1-01, U4-R2-01).

Every tool in tools/ and .agents/ that starts Git does it through this module, and
tools/test_git_child.py fails when one does not.

Every call:

- gets an environment built from KEPT, a short allow-list. No inherited GIT_* variable reaches Git,
  so GIT_DIR, GIT_WORK_TREE, GIT_INDEX_FILE, GIT_OBJECT_DIRECTORY, GIT_COMMON_DIR, GIT_TEMPLATE_DIR,
  GIT_TRACE and GIT_CONFIG_* cannot point Git at another repository, change its configuration or
  write a file;
- gets ENVIRONMENT: GIT_OPTIONAL_LOCKS=0 (a read never writes the index), GIT_NO_LAZY_FETCH=1 (an
  object that is not there is an error, never a fetch from a promisor remote),
  GIT_ALLOW_PROTOCOL=/ (no transport at all: Git allows only the listed protocols, `/` cannot be the
  name of a protocol or of a remote helper, and this variable overrides every protocol.*.allow
  setting), GIT_NO_REPLACE_OBJECTS=1 and GIT_TERMINAL_PROMPT=0;
- gets SETTINGS on its command line, which outrank every configuration file: no fsmonitor program
  or daemon, no hook, and no protocol.

Three ways to open a repository:

- `source_copy(folder)`: a bare copy of the Git data of the work tree at `folder`, made with plain
  file reads into a new temporary folder, and removed at the end. Git runs on the copy only, never on
  the source. Any tool that reads a commit's objects uses it (the snapshot tool, check_docs.py --rev,
  the docs-check test's history comparison).
- `create(folder)`: a new repository that the caller owns. Its commands see no global or system
  configuration and no template.
- `existing(folder)`: the developer's own working repository, for a read of its index or its HEAD
  only (check_docs.py lists tracked files with ls-files). Never for a commit's objects, and never
  for the snapshot tool.
"""

import os
import re
import shutil
import stat
import subprocess
import sys
import tempfile
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path

# Windows needs these to start a program, and Git needs HOME or USERPROFILE for the user's own configuration.
KEPT = (
    "PATH", "PATHEXT", "SYSTEMROOT", "SYSTEMDRIVE", "WINDIR", "COMSPEC", "TEMP", "TMP",
    "HOME", "HOMEDRIVE", "HOMEPATH", "USERPROFILE", "XDG_CONFIG_HOME",
    "APPDATA", "LOCALAPPDATA", "PROGRAMDATA", "PROGRAMFILES", "PROGRAMFILES(X86)", "PROGRAMW6432",
)
ENVIRONMENT = {
    "GIT_OPTIONAL_LOCKS": "0",
    "GIT_NO_LAZY_FETCH": "1",
    "GIT_ALLOW_PROTOCOL": "/",
    "GIT_NO_REPLACE_OBJECTS": "1",
    "GIT_TERMINAL_PROMPT": "0",
}
SETTINGS = ("-c", "core.fsmonitor=false", "-c", f"core.hooksPath={os.devnull}", "-c", "protocol.allow=never")


PROGRAM = "git"


class GitError(Exception):
    pass


def environment(isolated=False, extra=None):
    """The child's environment: KEPT from this process, then this module's own settings, then `extra`."""
    child = {name: os.environ[name] for name in KEPT if name in os.environ}
    child.update(ENVIRONMENT)
    if isolated:
        child["GIT_CONFIG_NOSYSTEM"] = "1"
        child["GIT_CONFIG_GLOBAL"] = os.devnull
        child["GIT_ATTR_NOSYSTEM"] = "1"
    for name, value in (extra or {}).items():
        if not name.startswith(("GIT_AUTHOR_", "GIT_COMMITTER_")):
            raise GitError(f"git_child: {name} is not a variable that a caller may set")
        child[name] = value
    return child


def _start(arguments, isolated, input=None, extra=None, check=True, cwd=None):
    done = subprocess.run([PROGRAM, *SETTINGS, *arguments], input=input, capture_output=True, env=environment(isolated, extra), cwd=cwd)
    if check and done.returncode != 0:
        raise GitError(f"git {' '.join(arguments[:4])} exited {done.returncode}: {done.stderr.decode('utf-8', 'replace').strip()[-400:]}")
    return done


def locate(folder):
    """The git directory of the developer's own repository whose work tree starts at `folder`. Git never looks above it."""
    folder = Path(folder).resolve()
    done = subprocess.run(
        [PROGRAM, *SETTINGS, "-C", str(folder), "rev-parse", "--absolute-git-dir", "--show-toplevel"],
        capture_output=True,
        env=dict(environment(), GIT_CEILING_DIRECTORIES=str(folder.parent)),
    )
    lines = done.stdout.decode("utf-8").splitlines()
    if done.returncode != 0 or len(lines) != 2 or Path(lines[1]).resolve() != folder:
        raise GitError(f"{folder} is not the top folder of a Git work tree")
    return Path(lines[0]).resolve()


@dataclass(frozen=True)
class Repository:
    git_dir: Path
    work_tree: Path | None
    isolated: bool

    def _prefix(self):
        prefix = [f"--git-dir={self.git_dir}"]
        if self.work_tree is not None:
            prefix.append(f"--work-tree={self.work_tree}")
        return [*prefix, "--no-replace-objects"]

    def run(self, *arguments, input=None, extra=None, check=True):
        """Run one Git command in this repository and return its standard output as bytes."""
        return _start([*self._prefix(), *arguments], self.isolated, input, extra, check, cwd=self.work_tree or self.git_dir).stdout

    def completed(self, *arguments, input=None):
        """Like run, but returns the finished process without checking its exit code."""
        return _start([*self._prefix(), *arguments], self.isolated, input, check=False, cwd=self.work_tree or self.git_dir)


def existing(folder):
    """The developer's own working repository at the top of `folder`, for a read of its index or HEAD only."""
    return Repository(locate(folder), Path(folder).resolve(), isolated=False)


def create(folder, branch="main"):
    """A new repository in `folder`, which must be empty or absent. No template, hook or user configuration applies."""
    folder = Path(folder).resolve()
    _start(["init", "-q", "--template=", f"--initial-branch={branch}", str(folder)], isolated=True)
    return Repository(folder / ".git", folder, isolated=True)


# The source copy (finding U4-R2-01).
#
# The copy holds only what the reads need, and each item is plain data:
#
# - objects/<2 hex>/<38 or 62 hex>: loose objects.
# - objects/pack/pack-<hex>.pack with its .idx: packed objects.
# - refs/** and packed-refs, without refs/replace/: the names that --rev resolves. A replace
#   reference makes Git show another object in place of a commit's own (git-replace(1)).
# - HEAD of the work tree's own Git folder (for a linked worktree, worktrees/<id>/HEAD), and the
#   per-worktree refs of that folder (refs/bisect, refs/worktree, refs/rewritten).
# - A configuration that this module writes: the format version, bare, and the object format
#   (SHA-1 or SHA-256) read from the source's configuration.
#
# Everything else stays out, because it can make Git start a program, read another file or folder,
# reach a network, or show other objects (gitrepository-layout(5), git-config(1), githooks(5),
# gitattributes(5), Documentation/technical/partial-clone, git-replace(1)):
#
# - config and config.worktree: remotes, remote helpers and url.*.insteadOf; the promisor and
#   partial-clone settings (remote.*.promisor, extensions.partialClone), which turn a missing object
#   into a lazy fetch; core.fsmonitor, core.hooksPath, core.sshCommand, core.alternateRefsCommand,
#   credential.*, gpg.program, filter.* and diff.*.textconv, which name programs; include.* and
#   includeIf.*, which read other files; alias.*. The copy gets its own configuration instead.
# - hooks/: programs.
# - objects/info/alternates and objects/info/http-alternates: other object stores, local or remote.
#   An object that only an alternate holds is missing in the copy, so the snapshot is refused.
# - objects/pack/*.promisor: marks a pack as one from a promisor remote. Without it and without a
#   promisor remote, a missing object is an error.
# - objects/info/packs, objects/info/commit-graph, objects/info/commit-graphs/,
#   objects/pack/multi-pack-index, *.bitmap, *.rev, *.mtimes: indexes that only make Git faster.
#   Git reads the objects without them, and no stale index can disagree with the copied objects.
# - objects/pack/*.keep, tmp_* and incoming-* folders: Git's own locks and unfinished writes.
# - info/grafts and shallow: they change which commits are parents. The snapshot needs no history,
#   so a revision that needs a cut-off commit is refused as missing.
# - info/attributes, info/exclude, info/sparse-checkout: attributes name filters; the copy has no
#   work tree.
# - index and sharedindex.*: the source's staging area. The reads use a commit.
# - logs/ (reflogs; HEAD@{n} does not resolve in the copy), modules/ and worktrees/ (other
#   repositories), FETCH_HEAD, ORIG_HEAD and the other pseudo-references (name a commit instead),
#   lfs/, rr-cache/, rebase-*, sequencer/ (other tools' state).
#
# Refused: a folder without a .git, a .git file without "gitdir:", a Git folder without HEAD or
# objects, a symbolic link or junction under the copied folders, a format version other than 0 or
# 1, the reftable format of references (extensions.refStorage other than "files"; Git reads a
# reftable/ folder only under that setting), and any extension that is not in KNOWN_EXTENSIONS.

LOOSE_OBJECT = re.compile(r"[0-9a-f]{38}(?:[0-9a-f]{24})?")
PACK_FILE = re.compile(r"(pack-[0-9a-f]{40}(?:[0-9a-f]{24})?)\.(pack|idx)")
OBJECT_FORMATS = {"sha1", "sha256"}
# Extensions that change nothing for a read of objects and refs in the files format. objectformat
# is carried into the copy; partialclone is dropped with the promisor remote.
KNOWN_EXTENSIONS = {"objectformat", "refstorage", "partialclone", "preciousobjects", "worktreeconfig", "relativeworktrees", "noop", "noop-v1"}
REPARSE_POINT = getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0x400)


def git_folders(folder):
    """(work tree, Git folder, common Git folder) of the work tree at `folder`, read from its files."""
    work_tree = Path(folder).resolve()
    dot = work_tree / ".git"
    if dot.is_dir():
        git_dir = dot
    elif dot.is_file():
        text = dot.read_text(encoding="utf-8").strip()
        if not text.startswith("gitdir:"):
            raise GitError(f"{dot} is a file that does not start with 'gitdir:'")
        target = Path(text[len("gitdir:"):].strip())
        git_dir = (target if target.is_absolute() else work_tree / target).resolve()
    else:
        raise GitError(f"{work_tree} is not the top folder of a Git work tree (it has no .git)")
    common = git_dir
    if (git_dir / "commondir").is_file():
        named = Path((git_dir / "commondir").read_text(encoding="utf-8").strip())
        common = (named if named.is_absolute() else git_dir / named).resolve()
    if not (git_dir / "HEAD").is_file() or not (common / "objects").is_dir():
        raise GitError(f"{git_dir} is not a Git folder (no HEAD, or no objects folder)")
    return work_tree, git_dir, common


def _plain_entries(folder):
    """The entries of `folder`, refusing a symbolic link or a junction (the copy reads no other folder)."""
    entries = []
    with os.scandir(folder) as listing:
        for entry in listing:
            details = entry.stat(follow_symlinks=False)
            if entry.is_symlink() or getattr(details, "st_file_attributes", 0) & REPARSE_POINT:
                raise GitError(f"the source Git folder holds a symbolic link or junction ({entry.name}); the tool reads no other folder")
            entries.append(entry)
    return entries


def _refuse_link(path):
    details = os.lstat(path)
    if stat.S_ISLNK(details.st_mode) or getattr(details, "st_file_attributes", 0) & REPARSE_POINT:
        raise GitError(f"the source Git folder holds a symbolic link or junction ({Path(path).name}); the tool reads no other folder")


def _copy(source, target):
    _refuse_link(source)
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, target)


def _copy_refs(folder, target):
    """Copy refs/** of `folder` into `target`, without refs/replace/."""
    def walk(current, relative):
        for entry in _plain_entries(current):
            name = f"{relative}/{entry.name}"
            if name == "refs/replace":
                continue
            if entry.is_dir(follow_symlinks=False):
                walk(Path(entry.path), name)
            elif entry.is_file(follow_symlinks=False):
                _copy(Path(entry.path), target / name)
    if (folder / "refs").is_dir():
        walk(folder / "refs", "refs")


def _packed_refs(text):
    """packed-refs without its refs/replace/ lines and the peeled lines that follow them."""
    kept, skipping = [], False
    for line in text.splitlines(keepends=True):
        if line.startswith("^"):
            if not skipping:
                kept.append(line)
            continue
        parts = line.split()
        skipping = len(parts) == 2 and not line.startswith("#") and parts[1].startswith("refs/replace/")
        if not skipping:
            kept.append(line)
    return "".join(kept)


def _remove_own(folder):
    """Remove a folder that this module created in this run. Git writes object files read-only."""
    def writable_then_retry(function, path, _error):
        os.chmod(path, stat.S_IWRITE | stat.S_IREAD)
        function(path)

    if sys.version_info >= (3, 12):
        shutil.rmtree(folder, onexc=writable_then_retry)
    else:
        shutil.rmtree(folder, onerror=writable_then_retry)


class SourceCopy(Repository):
    """A bare copy of a source repository's Git data. `root` is the temporary folder that holds it."""

    root: Path
    source_config: Path

    def __init__(self, root, git_dir, source_config):
        super().__init__(git_dir, None, True)
        object.__setattr__(self, "root", root)
        object.__setattr__(self, "source_config", source_config)

    def config_values(self, key, *scope, isolated=True):
        """Every value of `key` in one configuration scope (`--file <path>`, or `--global --includes`)."""
        done = _start([*self._prefix(), "config", *scope, "--null", "--get-all", key], isolated, check=False, cwd=self.root)
        return [value for value in done.stdout.decode("utf-8", "replace").split("\0") if value] if done.returncode == 0 else []

    def identity(self):
        """The user's name and address from the source's own configuration files and the user's
        global ones. A value that only an includeIf gives is not found (stated in the snapshot tool)."""
        values = []
        files = [path for path in (self.source_config, self.source_config.with_name("source-config.worktree")) if path.is_file()]
        for key in ("user.name", "user.email"):
            for path in files:
                values += [(key, value) for value in self.config_values(key, "--file", str(path))]
            values += [(key, value) for value in self.config_values(key, "--global", "--includes", isolated=False)]
        return values

    def missing(self, commit):
        """The objects of `commit` (the commit, its trees and its blobs, no history) that the copy lacks."""
        output = self.run("rev-list", "--objects", "--no-object-names", "--no-walk", "--missing=print", commit, "--")
        return [line[1:] for line in output.decode("ascii").split() if line.startswith("?")]


def _source_format(copy, config):
    """The object format of the source, from its configuration; refuses what the copy cannot hold."""
    values = {}
    done = _start([*copy._prefix(), "config", "--file", str(config), "--null", "--get-regexp", r"^(core\.repositoryformatversion|extensions\..*)$"], True, check=False, cwd=copy.root)
    for record in (r for r in done.stdout.decode("utf-8", "replace").split("\0") if r):
        key, _newline, value = record.partition("\n")
        values[key.lower()] = value.strip().lower()
    version = values.pop("core.repositoryformatversion", "0")
    if version not in ("0", "1"):
        raise GitError(f"the source's repository format version is {version}; the tool reads versions 0 and 1")
    unknown = sorted(key for key in values if key.removeprefix("extensions.") not in KNOWN_EXTENSIONS)
    if unknown:
        raise GitError(f"the source uses an extension that the tool does not know ({', '.join(unknown)})")
    if values.get("extensions.refstorage", "files") != "files":
        raise GitError("the source stores its references in the reftable format; the tool reads only the files format")
    object_format = values.get("extensions.objectformat", "sha1")
    if object_format not in OBJECT_FORMATS or (version == "0" and object_format != "sha1"):
        raise GitError(f"the source's object format {object_format!r} with format version {version} is not one the tool reads")
    return object_format


@contextmanager
def source_copy(folder):
    """Yield a SourceCopy of the Git data of the work tree at `folder`, made by plain file reads in a
    new temporary folder. The folder is removed at the end, also on failure; it is the only folder that
    this function removes (class F). The source is only read."""
    _work_tree, git_dir, common = git_folders(folder)
    root = Path(tempfile.mkdtemp(prefix="akm-git-copy-"))
    try:
        try:
            copy = _fill(root, git_dir, common)
        except OSError as error:
            raise GitError(f"could not read the source's Git data: {error}") from error
        yield copy
    finally:
        _remove_own(root)


def _fill(root, git_dir, common):
    """Copy the items that the comment above lists from the source into `root`, and write the configuration."""
    target = root / "copy.git"
    (target / "objects" / "pack").mkdir(parents=True)
    (target / "refs").mkdir()
    objects = common / "objects"
    for folder in (objects, common / "refs", git_dir / "refs"):
        if folder.exists():
            _refuse_link(folder)
    for entry in _plain_entries(objects):
        if len(entry.name) == 2 and all(character in "0123456789abcdef" for character in entry.name) and entry.is_dir(follow_symlinks=False):
            for loose in _plain_entries(entry.path):
                if LOOSE_OBJECT.fullmatch(loose.name) and loose.is_file(follow_symlinks=False):
                    _copy(Path(loose.path), target / "objects" / entry.name / loose.name)
    if (objects / "pack").is_dir():
        _refuse_link(objects / "pack")
        names = {entry.name for entry in _plain_entries(objects / "pack") if entry.is_file(follow_symlinks=False)}
        for name in sorted(names):
            match = PACK_FILE.fullmatch(name)
            if match and match.group(2) == "pack" and f"{match.group(1)}.idx" in names:
                for suffix in (".pack", ".idx"):
                    _copy(objects / "pack" / f"{match.group(1)}{suffix}", target / "objects" / "pack" / f"{match.group(1)}{suffix}")
    _copy_refs(common, target)
    if git_dir != common:
        _copy_refs(git_dir, target)
    if (common / "packed-refs").is_file():
        _refuse_link(common / "packed-refs")
        (target / "packed-refs").write_text(_packed_refs((common / "packed-refs").read_text(encoding="utf-8")), encoding="utf-8", newline="")
    _copy(git_dir / "HEAD", target / "HEAD")
    config = root / "source-config"
    if (common / "config").is_file():
        _copy(common / "config", config)
    else:
        config.write_bytes(b"")
    if (git_dir / "config.worktree").is_file():
        _copy(git_dir / "config.worktree", root / "source-config.worktree")
    copy = SourceCopy(root, target, config)
    object_format = _source_format(copy, config)
    lines = ["[core]", f"\trepositoryformatversion = {1 if object_format != 'sha1' else 0}", "\tbare = true"]
    if object_format != "sha1":
        lines += ["[extensions]", f"\tobjectFormat = {object_format}"]
    (target / "config").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    return copy
