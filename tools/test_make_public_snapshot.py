"""The public-snapshot tool refuses private data without printing it, never changes the source
repository, and writes only one new folder with one commit (FR-26, class F).

Each case builds a throwaway source repository in a temporary folder, with sentinel files, and runs
the real tool on it. Usage: python tools/test_make_public_snapshot.py (from the repository root)
"""

import hashlib
import os
import subprocess
import sys
import tempfile
from pathlib import Path

TOOL = Path(__file__).resolve().parent / "make_public_snapshot.py"
sys.path.insert(0, str(TOOL.parent))
import git_child  # noqa: E402
USER = os.environ.get("USERNAME", "")
SECRET_WORD = "zebracorn-private-word"
failures = 0


def case(name, ok, detail=""):
    global failures
    print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if not ok:
        failures += 1
        if detail:
            print(f"     {detail}")


def git(repo, *args):
    """A Git command in a test repository, through git_child, so no inherited GIT_ variable reaches it."""
    folder = Path(repo).resolve()
    return git_child.Repository(folder / ".git", folder, isolated=True).run(*args).decode("utf-8")


def make_source(root, files, patterns=None, history=None):
    """A source repository whose HEAD holds `files`. `history` files form an earlier commit."""
    repo = root / "source"
    git_child.create(repo)
    git(repo, "config", "core.autocrlf", "false")
    git(repo, "config", "user.name", "Snapshot Tester")
    git(repo, "config", "user.email", "tester@example.com")
    git(repo, "config", "commit.gpgsign", "false")
    for stage in ([history] if history else []) + [files]:
        for old in [path for path in repo.rglob("*") if path.is_file() and ".git" not in path.parts]:
            old.unlink()
        for name, text in stage.items():
            (repo / name).parent.mkdir(parents=True, exist_ok=True)
            (repo / name).write_bytes(text.encode("utf-8"))
        git(repo, "add", "-A")
        git(repo, "commit", "-q", "-m", "stage")
    (repo / "untracked-sentinel.txt").write_text("sentinel: the tool must not touch this file\n", encoding="utf-8")
    if patterns is not None:
        (repo / ".scratch").mkdir(exist_ok=True)
        (repo / ".scratch" / "public-snapshot-patterns.txt").write_text(patterns, encoding="utf-8")
    return repo


def fingerprint(repo):
    """Every file under the source folder, .git included, by path and SHA-256."""
    return sorted((str(path.relative_to(repo)), hashlib.sha256(path.read_bytes()).hexdigest()) for path in repo.rglob("*") if path.is_file())


def run(repo, *args, env=None):
    return subprocess.run([sys.executable, "-I", str(TOOL), "--repo", str(repo), "--rev", "HEAD", *args], capture_output=True, text=True, env=env)


def hostile_git(root, repo, out):
    """Variables and a configuration that point Git at the source, change its configuration, or run hooks
    that write a marker into the source (finding U4-R1-01). Returns (environment, home, files to keep)."""
    hooks = root / "hostile-hooks"
    template = root / "hostile-template"
    (template / "hooks").mkdir(parents=True)
    hooks.mkdir()
    marker = (repo / "hook-ran.txt").as_posix()
    for name in ("reference-transaction", "pre-commit", "post-commit", "post-checkout", "post-index-change"):
        for folder in (hooks, template / "hooks"):
            (folder / name).write_bytes(f"#!/bin/sh\necho {name} >> '{marker}'\n".encode())
    config = root / "hostile.gitconfig"
    config.write_bytes(f"[core]\n\tbare = true\n\thooksPath = {hooks.as_posix()}\n[init]\n\ttemplateDir = {template.as_posix()}\n[user]\n\tname = Hostile Home\n\temail = hostile@example.com\n".encode())
    home = root / "hostile-home"
    home.mkdir()
    (home / ".gitconfig").write_bytes(config.read_bytes())
    variables = {
        "GIT_DIR": str(repo / ".git"),
        "GIT_WORK_TREE": str(out),
        "GIT_INDEX_FILE": str(repo / ".git" / "index"),
        "GIT_OBJECT_DIRECTORY": str(repo / ".git" / "objects"),
        "GIT_ALTERNATE_OBJECT_DIRECTORIES": str(repo / ".git" / "objects"),
        "GIT_COMMON_DIR": str(repo / ".git"),
        "GIT_CONFIG_GLOBAL": str(config),
        "GIT_CONFIG_SYSTEM": str(config),
        "GIT_CONFIG_COUNT": "2",
        "GIT_CONFIG_KEY_0": "core.bare",
        "GIT_CONFIG_VALUE_0": "true",
        "GIT_CONFIG_KEY_1": "core.hooksPath",
        "GIT_CONFIG_VALUE_1": hooks.as_posix(),
        "GIT_CONFIG_PARAMETERS": "'core.bare'='true'",
        "GIT_TEMPLATE_DIR": str(template),
        "GIT_TRACE": str(repo / "git-trace.txt"),
        "GIT_NAMESPACE": "hostile",
    }
    return dict(os.environ, **variables), home, [config, home / ".gitconfig", *hooks.iterdir()]


def existing_c_folder():
    """A folder at the root of C: that exists here and is not a system folder of every computer."""
    system = {"windows", "program files", "program files (x86)", "programdata", "users", "tmp", "temp"}
    for entry in sorted(Path("C:\\").iterdir(), key=lambda path: path.name.lower()):
        if entry.is_dir() and entry.name.lower() not in system and not entry.name.startswith(("$", ".")) and " " not in entry.name:
            return entry.name
    return None


MACHINE_FOLDER = existing_c_folder()
CLEAN = {"README.md": "# Fake project\n\nA synthetic path C:\\fake\\vault.kdbx and Z:\\no\\such\\folder are not private.\nIcon: icons/128x128@2x.png. Contact: someone@example.com.\n", "src/main.rs": "fn main() {}\n"}
REFUSALS = [
    ("a user-profile path of this computer", {"notes.md": f"evidence in C:\\Users\\{USER}\\proofs\n"}, None, USER),
    ("a drive path of this computer", {"notes.md": f"build in C:\\{MACHINE_FOLDER}\\akm-build\n"}, None, MACHINE_FOLDER),
    ("an e-mail address", {"notes.md": "write to private.person@private-mail.test\n"}, None, "private.person"),  # public-snapshot: allow (a synthetic address)
    ("the git user's e-mail address", {"notes.md": "author tester@example.com\n"}, None, None),
    ("extra pattern 2 of the patterns file", {"notes.md": f"the {SECRET_WORD} is here\n"}, f"# private words\n{SECRET_WORD}\n", SECRET_WORD),
    ("extra pattern 1 of the patterns file", {"notes.md": "ticket ACME-12345\n"}, "re:ACME-\\d{5}\n", "ACME-12345"),
]

with tempfile.TemporaryDirectory(prefix="akm-snapshot-test-") as folder:
    base = Path(folder)

    for index, (rule, files, patterns, hidden) in enumerate(REFUSALS):
        root = base / f"refusal-{index}"
        root.mkdir()
        repo = make_source(root, {**CLEAN, **files}, patterns)
        before = fingerprint(repo)
        out = root / "public"
        result = run(repo, "--out", str(out), "--author", "Public Author <public@example.com>")
        printed = result.stdout + result.stderr
        case(f"refuses {rule}", result.returncode == 1 and f"notes.md:1: {rule}" in printed and "REFUSED" in printed, printed.strip())
        if hidden:
            case(f"does not print the matched text ({rule})", hidden.lower() not in printed.lower(), "the output holds the private text")
        case(f"writes nothing and leaves the source unchanged ({rule})", not out.exists() and fingerprint(repo) == before)

    root = base / "tracked-patterns"
    root.mkdir()
    repo = make_source(root, {**CLEAN, ".scratch/public-snapshot-patterns.txt": "harmless\n"})
    result = run(repo, "--scan-only")
    case("refuses a patterns file that Git tracks", result.returncode != 0 and "tracked by Git" in result.stdout + result.stderr, result.stdout + result.stderr)

    root = base / "existing-out"
    root.mkdir()
    repo = make_source(root, CLEAN)
    out = root / "public"
    out.mkdir()
    (out / "sentinel.txt").write_text("sentinel\n", encoding="utf-8")
    result = run(repo, "--out", str(out), "--author", "Public Author <public@example.com>")
    case("refuses an output folder that exists, and keeps its sentinel", result.returncode != 0 and (out / "sentinel.txt").read_text(encoding="utf-8") == "sentinel\n" and len(list(out.iterdir())) == 1, result.stdout + result.stderr)
    result = run(repo, "--out", str(repo / "inside"), "--author", "Public Author <public@example.com>")
    case("refuses an output folder inside the source repository", result.returncode != 0 and not (repo / "inside").exists(), result.stdout + result.stderr)

    root = base / "allowed-line"
    root.mkdir()
    repo = make_source(root, {**CLEAN, "notes.md": f"C:\\Users\\{USER}\\x  <!-- public-snapshot: allow -->\n"})
    result = run(repo, "--scan-only")
    case("a line marked public-snapshot: allow is not scanned", result.returncode == 0 and "CLEAN" in result.stdout, result.stdout + result.stderr)

    root = base / "snapshot"
    root.mkdir()
    removed = f"an old file with {SECRET_WORD}\n"
    repo = make_source(root, CLEAN, patterns=None, history={**CLEAN, "old-evidence.md": removed})
    before = fingerprint(repo)
    out = root / "public"
    result = run(repo, "--out", str(out), "--author", "Public Author <public@example.com>")
    printed = result.stdout + result.stderr
    case("writes a snapshot of a clean commit", result.returncode == 0 and "WROTE" in printed and out.is_dir(), printed.strip())
    if out.is_dir():
        objects = git(out, "rev-list", "--all", "--objects")
        everything = git(out, "log", "--all", "-p", "--format=%an %ae %s")
        case("the snapshot has exactly one commit, without a parent", git(out, "rev-list", "--all", "--count").strip() == "1" and len(git(out, "rev-list", "--parents", "-n", "1", "HEAD").split()) == 1)
        case("the snapshot tree is the source commit's tree", git(out, "rev-parse", "HEAD^{tree}") == git(repo, "rev-parse", "HEAD^{tree}"))
        case("a file removed earlier in the source history is not in the snapshot", "old-evidence.md" not in objects and SECRET_WORD not in everything)
        case("the snapshot commit names the given author only", "Public Author public@example.com" in everything and "tester@example.com" not in everything)
        case("the snapshot has no remote", git(out, "remote").strip() == "")
        case("the untracked file of the source is not in the snapshot", not (out / "untracked-sentinel.txt").exists())
    case("the source repository is unchanged, .git included", fingerprint(repo) == before)

    for label, use_home in (("inherited Git variables", False), ("a home configuration with hooks and a template", True)):
        root = base / ("hostile-home" if use_home else "hostile-variables")
        root.mkdir()
        repo = make_source(root, CLEAN)
        out = root / "public"
        environment, home, kept = hostile_git(root, repo, out)
        if use_home:
            environment = {name: value for name, value in os.environ.items() if not name.upper().startswith("GIT_")}
            environment.update(HOME=str(home), USERPROFILE=str(home))
        before = fingerprint(repo)
        kept_before = [path.read_bytes() for path in kept]
        result = run(repo, "--out", str(out), "--author", "Public Author <public@example.com>", env=environment)
        printed = result.stdout + result.stderr
        case(f"{label}: the source is unchanged, .git included, and no hook ran", fingerprint(repo) == before and not (repo / "hook-ran.txt").exists(), printed.strip()[-600:])
        case(f"{label}: the hostile configuration and hooks are unchanged", [path.read_bytes() for path in kept] == kept_before)
        written = result.returncode == 0 and "WROTE" in printed and out.is_dir()
        case(f"{label}: the snapshot is written, with the source tree and one commit", written and git(out, "rev-parse", "HEAD^{tree}") == git(repo, "rev-parse", "HEAD^{tree}") and git(out, "rev-list", "--all", "--count").strip() == "1", printed.strip()[-600:])
        hooks = [path.name for path in (out / ".git" / "hooks").iterdir() if not path.name.endswith(".sample")] if (out / ".git" / "hooks").is_dir() else []
        case(f"{label}: the new repository holds no hook", not hooks, str(hooks))

print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
