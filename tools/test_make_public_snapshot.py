"""The public-snapshot tool refuses private data without printing it, never changes the source
repository, and writes only one new folder with one commit (FR-26, class F, findings U4-R1-01 to 04, 09).

Each case builds a throwaway source repository in a temporary folder, with sentinel files, and runs
the real tool on it. Usage: python tools/test_make_public_snapshot.py (from the repository root)

Every address in this file is built at run time by `address`, so that the real scan of this file
meets no address.
"""

import gzip
import hashlib
import importlib.util
import os
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

TOOL = Path(__file__).resolve().parent / "make_public_snapshot.py"
sys.path.insert(0, str(TOOL.parent))
import git_child  # noqa: E402

USER = os.environ.get("USERNAME", "")
SECRET_WORD = "zebracorn-private-word"
LISTS = "tools/public_snapshot"
failures = 0


def address(local, domain):
    return f"{local}@{domain}"


PRIVATE = address("private.person", "private-mail.test")
PUBLIC_AUTHOR = f"Public Author <{address('public', 'example.com')}>"


def case(name, ok, detail=""):
    global failures
    print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if not ok:
        failures += 1
        if detail:
            print(f"     {detail}")


def git(repo, *args, data=None):
    """A Git command in a test repository, through git_child, so no inherited GIT_ variable reaches it."""
    folder = Path(repo).resolve()
    return git_child.Repository(folder / ".git", folder, isolated=True).run(*args, input=data).decode("utf-8")


def make_source(root, files, patterns=None, history=None, force=(), cacheinfo=()):
    """A source repository whose HEAD holds `files` (text or bytes). `history` files form an earlier
    commit. `force` paths are added past the ignore rules. `cacheinfo` is (mode, content or object id, path)
    entries that the work tree cannot hold, added to the index by plumbing."""
    repo = root / "source"
    git_child.create(repo)
    git(repo, "config", "core.autocrlf", "false")
    git(repo, "config", "user.name", "Snapshot Tester")
    git(repo, "config", "user.email", address("tester", "example.com"))
    for stage in ([history] if history else []) + [files]:
        for old in [path for path in repo.rglob("*") if path.is_file() and ".git" not in path.parts]:
            old.unlink()
        for name, content in stage.items():
            (repo / name).parent.mkdir(parents=True, exist_ok=True)
            (repo / name).write_bytes(content if isinstance(content, bytes) else content.encode("utf-8"))
        git(repo, "add", "-A")
        if stage is files:
            for name in force:
                git(repo, "add", "-f", "--", name)
            for mode, content, name in cacheinfo:
                oid = content if mode == "160000" else git(repo, "hash-object", "-w", "--stdin", data=content.encode()).strip()
                git(repo, "-c", "core.protectNTFS=false", "update-index", "--add", "--cacheinfo", f"{mode},{oid},{name}")
        git(repo, "commit", "-q", "--no-gpg-sign", "-m", "stage")
    (repo / "untracked-sentinel.txt").write_text("sentinel: the tool must not touch this file\n", encoding="utf-8")
    if patterns is not None:
        (repo / ".scratch").mkdir(exist_ok=True)
        (repo / ".scratch" / "public-snapshot-patterns.txt").write_text(patterns, encoding="utf-8")
    return repo


def fingerprint(repo):
    """Every file under the source folder, .git included, by path and SHA-256."""
    return sorted((str(path.relative_to(repo)), hashlib.sha256(path.read_bytes()).hexdigest()) for path in repo.rglob("*") if path.is_file())


def run(repo, *args, env=None, timeout=300):
    """The tool's result. A run that does not end in `timeout` seconds is stopped and fails its case."""
    command = [sys.executable, "-I", str(TOOL), "--repo", str(repo), "--rev", "HEAD", *args]
    try:
        return subprocess.run(command, capture_output=True, text=True, encoding="utf-8", env=env, timeout=timeout)
    except subprocess.TimeoutExpired:
        return subprocess.CompletedProcess(command, -1, f"TIMEOUT: the tool did not finish in {timeout} seconds", "")


def existing_c_folder():
    """A folder at the root of C: that exists here and is not a system folder of every computer."""
    system = {"windows", "program files", "program files (x86)", "programdata", "users", "tmp", "temp"}
    for entry in sorted(Path("C:\\").iterdir(), key=lambda path: path.name.lower()):
        if entry.is_dir() and entry.name.lower() not in system and not entry.name.startswith(("$", ".")) and " " not in entry.name:
            return entry.name
    return None


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
    config.write_bytes(f"[core]\n\tbare = true\n\thooksPath = {hooks.as_posix()}\n[init]\n\ttemplateDir = {template.as_posix()}\n[user]\n\tname = Hostile Home\n\temail = {address('hostile', 'example.com')}\n".encode())
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


MACHINE_FOLDER = existing_c_folder()
PUBLIC_LIST = f"# Exact strings that the e-mail rule accepts.\n{address('someone', 'example.com')}\n{address('128x128', '2x.png')}\n"
CLEAN = {
    "README.md": f"# Fake project\n\nA synthetic path C:\\fake\\vault.kdbx and Z:\\no\\such\\folder are not private.\nIcon: icons/{address('128x128', '2x.png')}. Contact: {address('someone', 'example.com')}.\n",
    "src/main.rs": "fn main() {}\n",
    f"{LISTS}/public-addresses.txt": PUBLIC_LIST,
}
OTHER_USER = dict(os.environ, USERNAME="zz-not-this-user")
PNG = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR" + bytes(range(256))


def opaque_line(data, path):
    return f"{hashlib.sha256(data).hexdigest()}  {path}\n"


# (case name, files, patterns, text that must not be printed, expected output, environment)
REFUSALS = [
    ("a user-profile path of this computer", {"notes.md": f"evidence in C:\\Users\\{USER}\\proofs\n"}, None, USER, "notes.md:1: a user-profile path of this computer", None),
    ("a drive path of this computer", {"notes.md": f"build in C:\\{MACHINE_FOLDER}\\akm-build\n"}, None, MACHINE_FOLDER, "notes.md:1: a drive path of this computer", None),
    ("an e-mail address", {"notes.md": f"write to {PRIVATE}\n"}, None, "private.person", "notes.md:1: an e-mail address", None),
    ("the git user's e-mail address", {"notes.md": f"author {address('tester', 'example.com')}\n"}, None, None, "notes.md:1: the git user's e-mail address", None),
    ("extra pattern 2 of the patterns file", {"notes.md": f"the {SECRET_WORD} is here\n"}, f"# private words\n{SECRET_WORD}\n", SECRET_WORD, "notes.md:1: extra pattern 2 of the patterns file", None),
    ("extra pattern 1 of the patterns file", {"notes.md": "ticket ACME-12345\n"}, "re:ACME-\\d{5}\n", "ACME-12345", "notes.md:1: extra pattern 1 of the patterns file", None),
    # U4-R1-02: a file the scan cannot read is refused, never published.
    ("UTF-16 with a byte order mark", {"notes.txt": f"write to {PRIVATE}\n".encode("utf-16")}, None, "private.person", "notes.txt: cannot be scanned", None),
    ("UTF-16 without a byte order mark", {"notes.txt": f"write to {PRIVATE}\n".encode("utf-16-be")}, None, "private.person", "notes.txt: cannot be scanned", None),
    ("UTF-32", {"notes.txt": f"write to {PRIVATE}\n".encode("utf-32")}, None, "private.person", "notes.txt: cannot be scanned", None),
    ("a compressed archive named archive.dat", {"archive.dat": gzip.compress(f"write to {PRIVATE}\n".encode(), mtime=0)}, None, "private.person", "archive.dat: cannot be scanned", None),
    ("Latin-1 text", {"notes.txt": f"café, write to {PRIVATE}\n".encode("latin-1")}, None, "private.person", "notes.txt: cannot be scanned", None),
    ("a text file larger than the 8 MiB limit", {"big.txt": b"a" * (8 * 1024 * 1024 + 1)}, None, None, "big.txt: cannot be scanned: larger than the 8 MiB scan limit", None),
    # U4-R1-03: paths are scanned with every rule, and a private path is never printed.
    ("an address in a file name", {f"docs/review.owner@{'private.invalid'}.txt": "harmless\n"}, None, "review.owner", "the path holds an e-mail address", None),
    ("an extra pattern in a folder name", {"notes/review-owner-zz/a.txt": "harmless\n"}, "review-owner-zz\n", "review-owner-zz", "the path holds extra pattern 1 of the patterns file", None),
    ("the Windows user name in a path", {f"notes/{USER}-diary.md": "harmless\n"}, None, USER, "the path holds the Windows user name", None),
    ("a content hit in a file with a private path", {"notes/review-owner-zz/b.md": f"write to {PRIVATE}\n"}, "review-owner-zz\n", "review-owner-zz", ", line 1: an e-mail address", None),
    # U4-R1-04: no suffix or domain is exempt, and no file is exempt by its name.
    ("an address that ends in .rs", {"notes.md": f"mail {address('john.doe', 'company.rs')}\n"}, None, "john.doe", "notes.md:1: an e-mail address", None),
    ("an internationalized domain", {"notes.md": f"mail {address('user', 'bücher.de')}\n"}, None, "bücher", "notes.md:1: an e-mail address", None),
    ("an internationalized local part and domain", {"notes.md": f"mail {address('пример', 'почта.рф')}\n"}, None, "пример", "notes.md:1: an e-mail address", None),
    ("a punycode top-level domain", {"notes.md": f"mail {address('user', 'example.xn--p1ai')}\n"}, None, "xn--p1ai", "notes.md:1: an e-mail address", None),
    ("an address in capitals with a plus tag", {"notes.md": f"mail {address('PRIVATE.PERSON+TAG', 'MAIL.PRIVATE-MAIL.TEST')}\n"}, None, "PRIVATE.PERSON", "notes.md:1: an e-mail address", None),
    ("an address at example.com that no list names", {"notes.md": f"mail {address('real.name', 'example.com')}\n"}, None, "real.name", "notes.md:1: an e-mail address", None),
    ("a fabricated THIRD_PARTY_NOTICES.md", {"THIRD_PARTY_NOTICES.md": f"# Third-party notices\n\nCopyright (c) {PRIVATE}\n"}, None, "private.person", "THIRD_PARTY_NOTICES.md:3: an e-mail address", None),
    # The allow marker silences only the rules it names, and never identity or extra.
    ("an e-mail address under a marker that names no rule", {"notes.md": f"{PRIVATE}  # public-snapshot: allow\n"}, None, "private.person", "notes.md:1: an e-mail address", None),
    ("an e-mail address under a marker for another rule", {"notes.md": f"{PRIVATE}  # public-snapshot: allow drive-path\n"}, None, "private.person", "notes.md:1: an e-mail address", None),
    ("an extra pattern under an e-mail marker", {"notes.md": f"{SECRET_WORD} {PRIVATE}  # public-snapshot: allow email\n"}, f"{SECRET_WORD}\n", SECRET_WORD, "notes.md:1: extra pattern 1 of the patterns file", None),
    ("the git user's address under an e-mail marker", {"notes.md": f"{address('tester', 'example.com')}  # public-snapshot: allow email\n"}, None, None, "notes.md:1: the git user's e-mail address", None),
    ("a profile path without its marker", {"notes.md": f"C:\\Users\\{USER}\\x\n"}, None, USER, "notes.md:1: a user-profile path of this computer", OTHER_USER),
    # The opaque list: a stale hash, a missing entry, a text file, a file that is not there, a bad line.
    ("an opaque file with a stale hash", {"logo.png": PNG, f"{LISTS}/opaque-files.txt": opaque_line(PNG + b"old", "logo.png")}, None, None, "logo.png: its entry on line 1 of tools/public_snapshot/opaque-files.txt has another SHA-256 (stale)", None),
    ("an opaque file without an entry", {"logo.png": PNG}, None, None, "logo.png: cannot be scanned: a NUL byte", None),
    ("a text file listed as opaque", {"notes.md": "text\n", f"{LISTS}/opaque-files.txt": opaque_line(b"text\n", "notes.md")}, None, None, "notes.md: is text, but line 1 of tools/public_snapshot/opaque-files.txt lists it as opaque", None),
    ("an opaque entry for a file the commit does not hold", {f"{LISTS}/opaque-files.txt": opaque_line(PNG, "gone.png")}, None, None, "tools/public_snapshot/opaque-files.txt:1: names a file that this commit does not hold", None),
    ("a malformed opaque line", {f"{LISTS}/opaque-files.txt": "not a hash  logo.png\n"}, None, None, "tools/public_snapshot/opaque-files.txt:1: a malformed or repeated line", None),
]

with tempfile.TemporaryDirectory(prefix="akm-snapshot-test-") as folder:
    base = Path(folder)

    def refusal(index, rule, files, patterns, hidden, expected, environment):
        root = base / f"refusal-{index}"
        root.mkdir()
        repo = make_source(root, {**CLEAN, **files}, patterns)
        before = fingerprint(repo)
        out = root / "public"
        result = run(repo, "--out", str(out), "--author", PUBLIC_AUTHOR, env=environment)
        printed = result.stdout + result.stderr
        found = [(f"refuses {rule}", result.returncode == 1 and expected in printed and "REFUSED" in printed, printed.strip()[-500:])]
        if hidden:
            found.append((f"does not print the matched text ({rule})", hidden.lower() not in printed.lower(), "the output holds the private text"))
        found.append((f"writes nothing and leaves the source unchanged ({rule})", not out.exists() and not list(root.glob(".public.incomplete-*")) and fingerprint(repo) == before, ""))
        return found

    # Lines and files that must pass.
    CLEAN_CASES = [
        ("an address that public-addresses.txt names, in any case", {"notes.md": f"mail {address('SOMEONE', 'EXAMPLE.COM')}\n"}, None),
        ("an address that notice-addresses.txt names", {"notes.md": f"Copyright {address('author', 'third-party.test')}\n", f"{LISTS}/notice-addresses.txt": f"# generated\n{address('author', 'third-party.test')}\n"}, None),
        ("an e-mail address under its marker", {"notes.md": f"{PRIVATE}  # public-snapshot: allow email (a synthetic address)\n"}, None),
        ("a profile path under its marker", {"notes.md": f"C:\\Users\\{USER}\\x  <!-- public-snapshot: allow profile-path -->\n"}, OTHER_USER),
        ("a drive path under its marker", {"notes.md": f"C:\\{MACHINE_FOLDER}\\x  # public-snapshot: allow drive-path, email\n"}, None),
        ("an opaque file whose path and SHA-256 are listed", {"logo.png": PNG, f"{LISTS}/opaque-files.txt": f"# checked by hand\n{opaque_line(PNG, 'logo.png')}"}, None),
        ("a UTF-8 file with a byte order mark", {"notes.md": "\ufeffplain text\n"}, None),
    ]

    def clean(index, name, files, environment):
        root = base / f"clean-{index}"
        root.mkdir()
        repo = make_source(root, {**CLEAN, **files})
        result = run(repo, "--scan-only", env=environment)
        return [(f"passes {name}", result.returncode == 0 and "CLEAN" in result.stdout, (result.stdout + result.stderr).strip()[-500:])]

    # Entries that Windows or the snapshot cannot hold, added by plumbing.
    ENTRY_CASES = [
        ("a symbolic link", [("120000", "target.txt", "link")], "link: a symbolic link"),
        ("a submodule", [("160000", "0123456789abcdef0123456789abcdef01234567", "sub")], "sub: an unsupported tree entry (commit"),
        ("two paths that differ only in case", [("100644", "one\n", "Notes.md"), ("100644", "two\n", "notes.md")], "a path that differs only in case"),
        ("a path that Windows cannot write", [("100644", "x\n", "aux.txt")], "aux.txt: a path that Windows cannot write"),
    ]

    def entry(index, name, cacheinfo, expected):
        root = base / f"entry-{index}"
        root.mkdir()
        repo = make_source(root, CLEAN, cacheinfo=cacheinfo)
        out = root / "public"
        result = run(repo, "--out", str(out), "--author", PUBLIC_AUTHOR)
        printed = result.stdout + result.stderr
        return [(f"refuses {name}", result.returncode == 1 and expected in printed and not out.exists(), printed.strip()[-400:])]

    # Each case has its own folders and passes its own environment to its own processes, so threads only save time.
    jobs = [(refusal, (index, *row)) for index, row in enumerate(REFUSALS)]
    jobs += [(clean, (index, *row)) for index, row in enumerate(CLEAN_CASES)]
    jobs += [(entry, (index, *row)) for index, row in enumerate(ENTRY_CASES)]
    with ThreadPoolExecutor(max_workers=8) as pool:
        for found in pool.map(lambda job: job[0](*job[1]), jobs):
            for name, ok, detail in found:
                case(name, ok, detail)

    # A line under the size limit is scanned in linear time: a rule that backtracks over a run of slashes took
    # 8 s for 20,000 characters, so it would never finish this line.
    root = base / "long-line"
    root.mkdir()
    MiB = 1024 * 1024
    line = " ".join(("\\" * MiB, "/" * MiB, "a@a." * (MiB // 4), "a@" + "a1." * (MiB // 3) + "1", "/users" * (MiB // 6), "c:\\" * (MiB // 3)))
    repo = make_source(root, {**CLEAN, "long.txt": line + "\n"})
    result = run(repo, "--scan-only", timeout=120)
    case("scans a 6 MiB line of slashes, dots and at signs within 120 seconds", result.returncode == 0 and "CLEAN" in result.stdout, (result.stdout + result.stderr).strip()[-300:])

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
    result = run(repo, "--out", str(out), "--author", PUBLIC_AUTHOR)
    case("refuses an output folder that exists, and keeps its sentinel", result.returncode != 0 and (out / "sentinel.txt").read_text(encoding="utf-8") == "sentinel\n" and len(list(out.iterdir())) == 1, result.stdout + result.stderr)
    result = run(repo, "--out", str(repo / "inside"), "--author", PUBLIC_AUTHOR)
    case("refuses an output folder inside the source repository", result.returncode != 0 and not (repo / "inside").exists(), result.stdout + result.stderr)

    result = subprocess.run([sys.executable, "-I", str(TOOL), "--help"], capture_output=True, text=True, encoding="utf-8")
    case("the help states what the allow marker silences", "It never silences" in result.stdout and "identity or extra" in result.stdout and "cannot be scanned" in result.stdout, result.stdout[-300:])

    root = base / "snapshot"
    root.mkdir()
    removed = f"an old file with {SECRET_WORD}\n"
    opaque = {"logo.png": PNG, f"{LISTS}/opaque-files.txt": opaque_line(PNG, "logo.png")}
    # U4-R1-09: a tracked file that an ignore rule matches belongs in the snapshot; an untracked one does not.
    tracked_files = {**CLEAN, **opaque, ".gitignore": "*.log\n", "kept.log": "a tracked file that an ignore rule matches\n", "tools/run.sh": "#!/bin/sh\n"}
    repo = make_source(root, tracked_files, patterns=None, history={**CLEAN, "old-evidence.md": removed}, force=["kept.log"])
    git(repo, "update-index", "--chmod=+x", "--", "tools/run.sh")
    git(repo, "commit", "-q", "--no-gpg-sign", "-m", "executable")
    (repo / "ignored.log").write_text("an untracked file that an ignore rule matches\n", encoding="utf-8")
    before = fingerprint(repo)
    out = root / "public"
    result = run(repo, "--out", str(out), "--author", PUBLIC_AUTHOR)
    printed = result.stdout + result.stderr
    case("writes a snapshot of a clean commit", result.returncode == 0 and "WROTE" in printed and out.is_dir(), printed.strip())
    if out.is_dir():
        objects = git(out, "rev-list", "--all", "--objects")
        everything = git(out, "log", "--all", "-p", "--format=%an %ae %s")
        case("the snapshot has exactly one commit, without a parent", git(out, "rev-list", "--all", "--count").strip() == "1" and len(git(out, "rev-list", "--parents", "-n", "1", "HEAD").split()) == 1)
        case("the snapshot tree is the source commit's tree, modes included", git(out, "rev-parse", "HEAD^{tree}") == git(repo, "rev-parse", "HEAD^{tree}") and "100755" in git(out, "ls-tree", "HEAD", "tools/run.sh"))
        case("a force-tracked file that an ignore rule matches is in the snapshot", (out / "kept.log").read_bytes() == (repo / "kept.log").read_bytes() and "kept.log" in git(out, "ls-files"))
        case("an untracked ignored file is not", not (out / "ignored.log").exists())
        case("an opaque file is copied byte for byte", (out / "logo.png").read_bytes() == PNG)
        case("a file removed earlier in the source history is not in the snapshot", "old-evidence.md" not in objects and SECRET_WORD not in everything)
        case("the snapshot commit names the given author only", f"Public Author {address('public', 'example.com')}" in everything and address("tester", "example.com") not in everything)
        case("the snapshot has no remote", git(out, "remote").strip() == "")
        case("the untracked file of the source is not in the snapshot", not (out / "untracked-sentinel.txt").exists())
        case("the snapshot's work tree is clean", git(out, "status", "--porcelain").strip() == "", git(out, "status", "--porcelain"))
        case("no unfinished folder is left beside it", not list(root.glob(".public.incomplete-*")))
    case("the source repository is unchanged, .git included", fingerprint(repo) == before)

    # A failure after the tool made its folder leaves no snapshot that looks usable, and keeps what it did not make.
    spec = importlib.util.spec_from_file_location("snapshot_tool", TOOL)
    tool = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = tool
    spec.loader.exec_module(tool)
    root = base / "failure"
    root.mkdir()
    repo = make_source(root, CLEAN)
    (root / "sentinel.txt").write_text("sentinel\n", encoding="utf-8")
    source = git_child.existing(repo)
    commit = source.run("rev-parse", "HEAD").decode().strip()
    tree = source.run("rev-parse", "HEAD^{tree}").decode().strip()
    entries, hits, _unscannable = tool.scan(source, commit, None)
    original = git_child.Repository.run

    def failing(self, *arguments, **options):
        if arguments[:1] == ("write-tree",):
            raise git_child.GitError("injected failure in write-tree")
        return original(self, *arguments, **options)

    git_child.Repository.run = failing
    try:
        tool.export(source, commit, tree, entries, root / "public", PUBLIC_AUTHOR)
        raised = False
    except git_child.GitError:
        raised = True
    finally:
        git_child.Repository.run = original
    case("a failed export raises, creates no output folder and removes its own unfinished folder", raised and not hits and not (root / "public").exists() and not list(root.glob(".public.incomplete-*")))
    case("a failed export keeps the sentinel beside it", (root / "sentinel.txt").read_text(encoding="utf-8") == "sentinel\n")

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
        result = run(repo, "--out", str(out), "--author", PUBLIC_AUTHOR, env=environment)
        printed = result.stdout + result.stderr
        case(f"{label}: the source is unchanged, .git included, and no hook ran", fingerprint(repo) == before and not (repo / "hook-ran.txt").exists(), printed.strip()[-600:])
        case(f"{label}: the hostile configuration and hooks are unchanged", [path.read_bytes() for path in kept] == kept_before)
        written = result.returncode == 0 and "WROTE" in printed and out.is_dir()
        case(f"{label}: the snapshot is written, with the source tree and one commit", written and git(out, "rev-parse", "HEAD^{tree}") == git(repo, "rev-parse", "HEAD^{tree}") and git(out, "rev-list", "--all", "--count").strip() == "1", printed.strip()[-600:])
        hooks = [path.name for path in (out / ".git" / "hooks").iterdir() if not path.name.endswith(".sample")] if (out / ".git" / "hooks").is_dir() else []
        case(f"{label}: the new repository holds no hook", not hooks, str(hooks))

print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
