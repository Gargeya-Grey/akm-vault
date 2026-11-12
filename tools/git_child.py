"""Start Git with a clean environment and a named repository (AGENTS.md class F, finding U4-R1-01).

Every tool in tools/ and .agents/ that starts Git does it through this module, and
tools/test_git_child.py fails when one does not.

- The child's environment is built from KEPT, a short allow-list. No inherited GIT_* variable
  reaches Git, so GIT_DIR, GIT_WORK_TREE, GIT_INDEX_FILE, GIT_OBJECT_DIRECTORY, GIT_COMMON_DIR,
  GIT_TEMPLATE_DIR, GIT_TRACE and GIT_CONFIG_* cannot point Git at another repository, change its
  configuration or write a file. The module adds its own GIT_ settings only.
- Every call names its repository with --git-dir and --work-tree. `locate` finds the repository
  whose work tree starts at the given folder, and never looks above it.
- `existing` opens a repository that the caller reads. It keeps the user's own global
  configuration (HOME), because a tool may need `user.name`. Optional locks are off, so a read does
  not refresh the index.
- `create` makes a new repository that the caller owns. Its commands see no global or system
  configuration and no template, so no hook, alias or filter of the user's runs in it.
"""

import os
import subprocess
from dataclasses import dataclass
from pathlib import Path

# Windows needs these to start a program, and Git needs HOME or USERPROFILE for the user's own configuration.
KEPT = (
    "PATH", "PATHEXT", "SYSTEMROOT", "SYSTEMDRIVE", "WINDIR", "COMSPEC", "TEMP", "TMP",
    "HOME", "HOMEDRIVE", "HOMEPATH", "USERPROFILE", "XDG_CONFIG_HOME",
    "APPDATA", "LOCALAPPDATA", "PROGRAMDATA", "PROGRAMFILES", "PROGRAMFILES(X86)", "PROGRAMW6432",
)


class GitError(Exception):
    pass


def environment(isolated=False, extra=None):
    """The child's environment: KEPT from this process, then this module's own settings, then `extra`."""
    child = {name: os.environ[name] for name in KEPT if name in os.environ}
    child["GIT_OPTIONAL_LOCKS"] = "0"
    if isolated:
        child["GIT_CONFIG_NOSYSTEM"] = "1"
        child["GIT_CONFIG_GLOBAL"] = os.devnull
    for name, value in (extra or {}).items():
        if not name.startswith(("GIT_AUTHOR_", "GIT_COMMITTER_")):
            raise GitError(f"git_child: {name} is not a variable that a caller may set")
        child[name] = value
    return child


def _start(arguments, isolated, input=None, extra=None, check=True, cwd=None):
    done = subprocess.run(["git", *arguments], input=input, capture_output=True, env=environment(isolated, extra), cwd=cwd)
    if check and done.returncode != 0:
        raise GitError(f"git {' '.join(arguments[:4])} exited {done.returncode}: {done.stderr.decode('utf-8', 'replace').strip()[-400:]}")
    return done


def locate(folder):
    """The git directory of the repository whose work tree starts at `folder`. Git never looks above it."""
    folder = Path(folder).resolve()
    done = subprocess.run(
        ["git", "-C", str(folder), "rev-parse", "--absolute-git-dir", "--show-toplevel"],
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
    work_tree: Path
    isolated: bool

    def run(self, *arguments, input=None, extra=None, check=True):
        """Run one Git command in this repository and return its standard output as bytes."""
        prefix = [f"--git-dir={self.git_dir}", f"--work-tree={self.work_tree}", "--no-replace-objects"]
        if self.isolated:
            prefix += ["-c", f"core.hooksPath={os.devnull}"]
        return _start([*prefix, *arguments], self.isolated, input, extra, check, cwd=self.work_tree).stdout

    def completed(self, *arguments, input=None):
        """Like run, but returns the finished process without checking its exit code."""
        prefix = [f"--git-dir={self.git_dir}", f"--work-tree={self.work_tree}", "--no-replace-objects"]
        return _start([*prefix, *arguments], self.isolated, input, check=False, cwd=self.work_tree)


def existing(folder):
    """A repository that the caller reads, found at the top of `folder`."""
    return Repository(locate(folder), Path(folder).resolve(), isolated=False)


def create(folder, branch="main"):
    """A new repository in `folder`, which must be empty or absent. No template, hook or user configuration applies."""
    folder = Path(folder).resolve()
    _start(["init", "-q", "--template=", f"--initial-branch={branch}", str(folder)], isolated=True)
    return Repository(folder / ".git", folder, isolated=True)
