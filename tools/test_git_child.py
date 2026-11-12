"""Every Git process that a tool starts goes through tools/git_child.py (AGENTS.md class F, finding U4-R1-01).

git_child builds the child's environment from an allow-list, so no inherited GIT_* variable (GIT_DIR,
GIT_WORK_TREE, GIT_INDEX_FILE, GIT_OBJECT_DIRECTORY, GIT_CONFIG_*) can point Git at another
repository or change its configuration, and every call names its repository. This test fails when a
Python or PowerShell file in tools/ or .agents/ starts Git another way, and it runs the helper itself
with hostile variables against a decoy repository.

Usage: python tools/test_git_child.py (from the repository root)
"""

import ast
import hashlib
import os
import re
import sys
import tempfile
from pathlib import Path

TOOLS = Path(__file__).resolve().parent
ROOT = TOOLS.parent
sys.path.insert(0, str(TOOLS))
import git_child  # noqa: E402

HELPERS = {TOOLS / "git_child.py", TOOLS / "GitChild.psm1"}
# A PowerShell command word `git` or `git.exe`, outside a comment. The helper module is the only exception.
PS_GIT = re.compile(r"(?im)(?:^|[\s;|({&])(?:&\s*)?['\"]?git(?:\.exe)?['\"]?(?=\s)")
failures = 0


def case(name, ok, detail=""):
    global failures
    print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if not ok:
        failures += 1
        if detail:
            print(f"     {detail}")


def sources(suffixes):
    for folder in (ROOT / "tools", ROOT / ".agents"):
        for path in sorted(folder.rglob("*")):
            if path.suffix.lower() in suffixes and "node_modules" not in path.parts and path.is_file():
                yield path


def python_git_starts(path):
    """Lines where a Python file names the program git: a literal 'git' or 'git.exe', or a text that starts 'git '."""
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    lines = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Constant) and isinstance(node.value, str):
            word = node.value.strip().lower()
            if word in ("git", "git.exe") or word.startswith(("git ", "git.exe ")):
                lines.append(node.lineno)
    return sorted(set(lines))


def powershell_git_starts(path):
    lines = []
    for number, line in enumerate(path.read_text(encoding="utf-8-sig").splitlines(), 1):
        code = line.split("#", 1)[0] if not line.lstrip().startswith("<#") else ""
        if PS_GIT.search(code):
            lines.append(number)
    return lines


def static_guard(name, finder, suffixes):
    offenders = []
    for path in sources(suffixes):
        if path in HELPERS or path == Path(__file__).resolve():
            continue
        for number in finder(path):
            offenders.append(f"{path.relative_to(ROOT).as_posix()}:{number}")
    case(name, not offenders, "start Git through tools/git_child.py (Python) or tools/GitChild.psm1 (PowerShell): " + ", ".join(offenders))


def fingerprint(folder):
    return sorted((str(path.relative_to(folder)), hashlib.sha256(path.read_bytes()).hexdigest()) for path in folder.rglob("*") if path.is_file())


static_guard("no Python tool starts Git outside git_child.py", python_git_starts, {".py"})
static_guard("no PowerShell tool starts Git outside GitChild.psm1", powershell_git_starts, {".ps1", ".psm1"})

# The guard itself must see a raw start, or it proves nothing.
with tempfile.TemporaryDirectory(prefix="akm-git-child-test-") as folder:
    probe = Path(folder) / "probe.py"
    probe.write_text('import subprocess\nsubprocess.run(["git", "status"])\nsubprocess.run("git status", shell=True)\n', encoding="utf-8")
    case("the Python guard finds a raw start of Git", python_git_starts(probe) == [2, 3], str(python_git_starts(probe)))
    probe = Path(folder) / "probe.ps1"
    probe.write_text("$x = $(git -C $repo status)\n# git in a comment is not a start\n& git.exe log\n", encoding="utf-8")
    case("the PowerShell guard finds a raw start of Git", powershell_git_starts(probe) == [1, 3], str(powershell_git_starts(probe)))

    # Hostile variables point at a decoy repository; the helper must ignore every one of them.
    base = Path(folder)
    decoy = base / "decoy"
    target = base / "target"
    for repo in (decoy, target):
        made = git_child.create(repo)
        (repo / "file.txt").write_bytes(f"{repo.name}\n".encode())
        made.run("add", "file.txt")
        made.run("-c", "user.name=T", "-c", "user.email=t@example.com", "commit", "-q", "--no-gpg-sign", "-m", repo.name)
    hostile_config = base / "hostile.gitconfig"
    hostile_config.write_text("[core]\n\tbare = true\n", encoding="utf-8")
    hostile = {
        "GIT_DIR": str(decoy / ".git"),
        "GIT_WORK_TREE": str(decoy),
        "GIT_INDEX_FILE": str(decoy / ".git" / "index"),
        "GIT_OBJECT_DIRECTORY": str(decoy / ".git" / "objects"),
        "GIT_COMMON_DIR": str(decoy / ".git"),
        "GIT_CONFIG_GLOBAL": str(hostile_config),
        "GIT_CONFIG_SYSTEM": str(hostile_config),
        "GIT_CONFIG_COUNT": "1",
        "GIT_CONFIG_KEY_0": "core.bare",
        "GIT_CONFIG_VALUE_0": "true",
        "GIT_CONFIG_PARAMETERS": "'core.bare'='true'",
        "GIT_TRACE": str(decoy / "trace.txt"),
    }
    before = fingerprint(decoy)
    saved = {name: os.environ.get(name) for name in hostile}
    os.environ.update(hostile)
    environment, located, shown, refused_outside, refused_inner, error = {}, None, None, False, False, ""
    try:
        environment = git_child.environment()
        located = git_child.locate(target)
        opened = git_child.existing(target)
        head = opened.run("rev-parse", "HEAD:file.txt").decode().strip()
        shown = opened.run("cat-file", "blob", head).decode()
        try:
            git_child.locate(target / "no-such-folder")
            refused_outside = False
        except git_child.GitError:
            refused_outside = True
        (target / "inner").mkdir()
        try:
            git_child.locate(target / "inner")
            refused_inner = False
        except git_child.GitError:
            refused_inner = True
    except git_child.GitError as failed:
        error = str(failed)
    finally:
        for name, value in saved.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value
    case("the child environment holds no inherited GIT_ variable", not [name for name in environment if name.upper().startswith("GIT_") and name.upper() != "GIT_OPTIONAL_LOCKS"], str(sorted(environment)))
    case("locate finds the named repository, not GIT_DIR", located == (target / ".git").resolve(), error or str(located))
    case("a read goes to the named repository", shown == "target\n", error or repr(shown))
    case("locate refuses a folder that is not a repository's top, and does not look above it", refused_outside and refused_inner)
    case("the decoy repository is unchanged, .git included", fingerprint(decoy) == before)
    case("the hostile configuration file is unchanged", hostile_config.read_text(encoding="utf-8") == "[core]\n\tbare = true\n")

print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
