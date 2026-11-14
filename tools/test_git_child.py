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
import stat
import subprocess
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
    case("the child environment holds no inherited GIT_ variable", not [name for name in environment if name.upper().startswith("GIT_") and name not in git_child.ENVIRONMENT], str(sorted(environment)))
    case("locate finds the named repository, not GIT_DIR", located == (target / ".git").resolve(), error or str(located))
    case("a read goes to the named repository", shown == "target\n", error or repr(shown))
    case("locate refuses a folder that is not a repository's top, and does not look above it", refused_outside and refused_inner)
    case("the decoy repository is unchanged, .git included", fingerprint(decoy) == before)
    case("the hostile configuration file is unchanged", hostile_config.read_text(encoding="utf-8") == "[core]\n\tbare = true\n")

# Finding U4-R2-01: no fetch, no transport, no fsmonitor program and no hook on any call, in Python and in
# PowerShell. Each stand-in program only appends to its own marker file in the temporary folder.
def plain_git(repo, *args, env):
    """Git without the helper, as a control: it shows that the fixture really starts the stand-in."""
    return subprocess.run(["git", "-C", str(repo), *args], capture_output=True, env=env)


def remove_loose(repo, oid):
    path = repo / ".git" / "objects" / oid[:2] / oid[2:]
    os.chmod(path, stat.S_IWRITE | stat.S_IREAD)
    path.unlink()


with tempfile.TemporaryDirectory(prefix="akm-git-child-test-", ignore_cleanup_errors=True) as folder:
    base = Path(folder)
    stand_in = base / "bin"
    stand_in.mkdir()
    helper_marker = base / "helper-started.txt"
    (stand_in / "git-remote-akmstandin").write_bytes(f"#!/bin/sh\necho started >> '{helper_marker.as_posix()}'\nexit 1\n".encode())
    monitor_marker = base / "fsmonitor-ran.txt"
    monitor = base / "fsmonitor.sh"
    monitor.write_bytes(f"#!/bin/sh\necho ran >> '{monitor_marker.as_posix()}'\nexit 1\n".encode())
    repo = base / "partial"
    made = git_child.create(repo)
    (repo / "file.txt").write_bytes(b"content\n")
    made.run("add", "file.txt")
    made.run("-c", "user.name=T", "-c", "user.email=" + "@".join(("t", "example.com")), "commit", "-q", "--no-gpg-sign", "-m", "x")
    blob = made.run("rev-parse", "HEAD:file.txt").decode().strip()
    remove_loose(repo, blob)
    other = base / "other"
    git_child.create(other)
    with open(repo / ".git" / "config", "a", encoding="utf-8", newline="\n") as config:
        config.write(f"[core]\n\trepositoryformatversion = 1\n\tfsmonitor = {monitor.as_posix()}\n[extensions]\n\tpartialClone = origin\n"
                     f"[remote \"origin\"]\n\turl = akmstandin::fake\n\tpromisor = true\n[protocol \"file\"]\n\tallow = always\n")
    plain = {name: value for name, value in os.environ.items() if not name.upper().startswith("GIT_")}
    plain.update(PATH=str(stand_in) + os.pathsep + os.environ["PATH"], GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_GLOBAL=os.devnull)
    plain_git(repo, "cat-file", "-p", blob, env=plain)
    plain_git(repo, "status", "--porcelain", env=plain)
    controls = helper_marker.exists() and monitor_marker.exists()
    case("control: without the helper, Git starts the stand-in remote helper and the fsmonitor program", controls)
    for marker in (helper_marker, monitor_marker):
        if marker.exists():
            marker.unlink()

    saved_path = os.environ["PATH"]
    os.environ["PATH"] = str(stand_in) + os.pathsep + saved_path
    try:
        opened = git_child.existing(repo)
        lazy = opened.completed("cat-file", "-p", blob)
        listed = opened.completed("status", "--porcelain")
        transport = opened.completed("ls-remote", other.resolve().as_uri())
        helper = opened.completed("ls-remote", "akmstandin::x")
    finally:
        os.environ["PATH"] = saved_path
    case("Python: a missing object is an error, and no lazy fetch starts the remote helper", lazy.returncode != 0 and not helper_marker.exists(), lazy.stderr[-200:])
    case("Python: a status read starts no fsmonitor program", not monitor_marker.exists())
    case("Python: the file transport is refused, also against protocol.file.allow=always", transport.returncode != 0 and b"not allowed" in transport.stderr, transport.stderr[-200:])
    case("Python: a remote-helper transport is refused", helper.returncode != 0 and not helper_marker.exists(), helper.stderr[-200:])

    # Each layer alone. With the transport settings out for this one probe, the lazy-fetch setting
    # must still stop the fetch; otherwise the cases above prove only the transport refusal.
    layered = {name: value for name, value in git_child.environment().items() if name != "GIT_ALLOW_PROTOCOL"}
    layered["PATH"] = str(stand_in) + os.pathsep + os.environ["PATH"]
    alone = subprocess.run([git_child.PROGRAM, "-c", "core.fsmonitor=false", "-C", str(repo), "cat-file", "-p", blob], capture_output=True, env=layered)
    case("Python: GIT_NO_LAZY_FETCH alone stops the fetch (transports allowed for this probe)", alone.returncode != 0 and not helper_marker.exists(), alone.stderr[-200:])
    for marker in (helper_marker, monitor_marker):
        if marker.exists():
            marker.unlink()

    for marker in (helper_marker, monitor_marker):
        if marker.exists():
            marker.unlink()
    probe = base / "probe.ps1"
    probe.write_text("\n".join([
        f"Import-Module '{TOOLS / 'GitChild.psm1'}' -Force",
        f"$env:PATH = '{stand_in};' + $env:PATH",
        "$env:GIT_DIR = 'C:\\no\\such\\repository'",
        f"$said = Invoke-CleanGit '{repo}' @('rev-parse', '--sq-quote', '-p', '--', '-e', 'x')",
        "\"arguments=$said\"",
        f"$null = Invoke-CleanGit '{repo}' @('cat-file', '-p', '{blob}') 2>&1",
        "\"lazy=$LASTEXITCODE\"",
        f"$null = Invoke-CleanGit '{repo}' @('status', '--porcelain') 2>&1",
        f"$said = Invoke-CleanGit '{repo}' @('ls-remote', '{other.resolve().as_uri()}') 2>&1",
        "\"transport=$LASTEXITCODE $said\"",
        "\"after=$env:GIT_DIR|$env:GIT_NO_LAZY_FETCH|$env:GIT_ALLOW_PROTOCOL\"",
        "",
    ]), encoding="utf-8")
    shell = subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(probe)], capture_output=True, text=True)
    said = dict(line.split("=", 1) for line in shell.stdout.splitlines() if "=" in line)
    case("PowerShell: Git receives each argument as written, -p, -- and -e included", said.get("arguments", "").strip() == "'-p' '--' '-e' 'x'", shell.stdout + shell.stderr[-300:])
    case("PowerShell: a missing object is an error (128, not a usage error), and no lazy fetch starts the remote helper", said.get("lazy") == "128" and not helper_marker.exists(), shell.stdout + shell.stderr[-300:])
    case("PowerShell: a status read starts no fsmonitor program", not monitor_marker.exists())
    case("PowerShell: the file transport is refused", said.get("transport", "0").startswith("128 ") and "not allowed" in said.get("transport", ""), shell.stdout)
    case("PowerShell: the caller's GIT_ variables come back and the helper's own settings go", said.get("after") == "C:\\no\\such\\repository||", shell.stdout)

    # source_copy holds only what git_child.py lists, and the source stays unchanged.
    source = base / "full"
    made = git_child.create(source)
    for name, content in (("a.txt", b"one\n"), ("b.txt", b"two\n")):
        (source / name).write_bytes(content)
    made.run("add", "-A")
    made.run("-c", "user.name=T", "-c", "user.email=" + "@".join(("t", "example.com")), "commit", "-q", "--no-gpg-sign", "-m", "x")
    made.run("tag", "v1")
    head = made.run("rev-parse", "HEAD").decode().strip()
    made.run("replace", made.run("rev-parse", "HEAD:a.txt").decode().strip(), made.run("rev-parse", "HEAD:b.txt").decode().strip())
    made.run("repack", "-a", "-d", "-q")
    pack = next((source / ".git" / "objects" / "pack").glob("pack-*.pack"))
    pack.with_suffix(".promisor").write_bytes(b"")
    pack.with_suffix(".keep").write_bytes(b"")
    (source / ".git" / "hooks").mkdir(exist_ok=True)
    (source / ".git" / "hooks" / "post-checkout").write_bytes(b"#!/bin/sh\nexit 0\n")
    (source / ".git" / "info").mkdir(exist_ok=True)
    for name, content in (("grafts", f"{head}\n"), ("attributes", "* filter=x\n"), ("exclude", "*.log\n")):
        (source / ".git" / "info" / name).write_bytes(content.encode())
    (source / ".git" / "objects" / "info").mkdir(exist_ok=True)
    (source / ".git" / "objects" / "info" / "alternates").write_bytes(f"{(other / '.git' / 'objects').as_posix()}\n".encode())
    (source / ".git" / "shallow").write_bytes(f"{head}\n".encode())
    with open(source / ".git" / "config", "a", encoding="utf-8", newline="\n") as config:
        config.write("[remote \"origin\"]\n\turl = akmstandin::fake\n\tpromisor = true\n[core]\n\tfsmonitor = true\n")
    before = fingerprint(source)
    with git_child.source_copy(source) as copy:
        names = {path.relative_to(copy.git_dir).as_posix() for path in copy.git_dir.rglob("*") if path.is_file()}
        copied_config = (copy.git_dir / "config").read_text(encoding="utf-8")
        resolved = copy.run("rev-parse", "HEAD", "refs/tags/v1").decode().split()
        shown = copy.run("cat-file", "blob", "HEAD:a.txt")
        root = copy.root
    left_out = sorted(name for name in names if name.startswith(("hooks/", "info/", "logs/", "refs/replace/", "objects/info/")) or name in ("shallow", "index", "config.worktree", "FETCH_HEAD", "ORIG_HEAD") or name.endswith((".promisor", ".keep", ".bitmap", ".rev", ".mtimes")))
    case("source_copy: the copy resolves HEAD and a tag", resolved == [head, head], str(resolved))
    case("source_copy: a replace reference does not reach the copy", shown == b"one\n", repr(shown))
    case("source_copy: the copy holds no hook, info file, reflog, replace reference, alternates, shallow file, index or promisor mark", not left_out and "HEAD" in names, str(left_out))
    case("source_copy: the copy's configuration is the helper's own", copied_config == "[core]\n\trepositoryformatversion = 0\n\tbare = true\n", repr(copied_config))
    case("source_copy: the copy folder is removed at the end", not root.exists())
    case("source_copy: the source is unchanged, .git included", fingerprint(source) == before)

    sha256 = base / "sha256"
    git_child._start(["init", "-q", "--template=", "--object-format=sha256", str(sha256)], isolated=True)
    made = git_child.Repository(sha256 / ".git", sha256, isolated=True)
    (sha256 / "a.txt").write_bytes(b"one\n")
    made.run("add", "a.txt")
    made.run("-c", "user.name=T", "-c", "user.email=" + "@".join(("t", "example.com")), "commit", "-q", "--no-gpg-sign", "-m", "x")
    with git_child.source_copy(sha256) as copy:
        long_head = copy.run("rev-parse", "HEAD").decode().strip()
        long_config = (copy.git_dir / "config").read_text(encoding="utf-8")
    case("source_copy: a SHA-256 repository is copied with its object format", len(long_head) == 64 and "objectFormat = sha256" in long_config, long_config)

    refusals = []
    junction_source = base / "junction"
    made = git_child.create(junction_source)
    elsewhere = base / "elsewhere"
    elsewhere.mkdir()
    (elsewhere / "sentinel.txt").write_bytes(b"sentinel\n")
    subprocess.run(["cmd", "/c", "mklink", "/J", str(junction_source / ".git" / "refs" / "linked"), str(elsewhere)], capture_output=True)
    refusals.append(("a junction under refs", junction_source, "symbolic link or junction"))
    unknown = base / "unknown-extension"
    made = git_child.create(unknown)
    with open(unknown / ".git" / "config", "a", encoding="utf-8", newline="\n") as config:
        config.write("[core]\n\trepositoryformatversion = 1\n[extensions]\n\tsomethingNew = true\n")
    refusals.append(("an extension that the helper does not know", unknown, "does not know"))
    refusals.append(("a folder without .git", elsewhere, "no .git"))
    for label, folder_path, expected in refusals:
        try:
            with git_child.source_copy(folder_path):
                refused = ""
        except git_child.GitError as error:
            refused = str(error)
        case(f"source_copy refuses {label}", expected in refused, refused or "not refused")
    case("source_copy: the junction's target is unchanged", (elsewhere / "sentinel.txt").read_bytes() == b"sentinel\n" and len(list(elsewhere.iterdir())) == 1)

# The settings that every Git child must see, written here and not read from the helper, so a change
# to the helper's tables cannot change what this test expects.
EXPECTED_ENVIRONMENT = {"GIT_OPTIONAL_LOCKS": "0", "GIT_NO_LAZY_FETCH": "1", "GIT_ALLOW_PROTOCOL": "/", "GIT_NO_REPLACE_OBJECTS": "1", "GIT_TERMINAL_PROMPT": "0"}
EXPECTED_SETTINGS = ("core.fsmonitor=false", "core.hooksPath=", "protocol.allow=never")


def settings_missing(lines):
    """The expected settings that a Git child did not see, from its `env` output. Git hands each -c
    setting to its children in GIT_CONFIG_PARAMETERS; the hooks path is the null device (NUL or nul)."""
    seen = dict(line.split("=", 1) for line in lines if line.startswith("GIT_") and "=" in line)
    given = seen.get("GIT_CONFIG_PARAMETERS", "").replace("'", "").lower()
    missing = [f"{name}={value}" for name, value in EXPECTED_ENVIRONMENT.items() if seen.get(name) != value]
    for setting in EXPECTED_SETTINGS:
        wanted = setting.lower() + ("nul" if setting.endswith("=") else "")
        if wanted not in given:
            missing.append(setting)
    return missing


# The PowerShell helper must give Git the same settings as git_child.py; a shell alias prints the
# environment that a Git child sees.
with tempfile.TemporaryDirectory(prefix="akm-git-child-test-", ignore_cleanup_errors=True) as folder:
    repo = Path(folder) / "settings"
    git_child.create(repo)
    shown = git_child.existing(repo).run("-c", "alias.akmenv=!env", "akmenv").decode("utf-8", "replace").splitlines()
    case("Python: a Git child sees every setting of ENVIRONMENT and SETTINGS", not settings_missing(shown), str(settings_missing(shown)))
    probe = Path(folder) / "settings.ps1"
    probe.write_text("\n".join([
        f"Import-Module '{TOOLS / 'GitChild.psm1'}' -Force",
        f"Invoke-CleanGit '{repo}' @('-c', 'alias.akmenv=!env', 'akmenv')",
        "",
    ]), encoding="utf-8")
    shell = subprocess.run(["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(probe)], capture_output=True, text=True)
    lines = shell.stdout.splitlines()
    case("PowerShell: a Git child sees every setting of git_child.py's ENVIRONMENT and SETTINGS", shell.returncode == 0 and not settings_missing(lines), str(settings_missing(lines)) + shell.stderr[-300:])

print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
