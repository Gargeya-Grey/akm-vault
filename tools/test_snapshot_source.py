"""The public-snapshot tool never runs Git on the source repository (AGENTS.md class F, findings U4-R1-01,
U4-R2-01).

The tool reads the source's Git folder as files, copies what it needs into a new folder of its own,
and runs Git only on that copy. These cases build throwaway sources in a temporary folder: a partial
clone with a missing object whose remote is a stand-in helper, a shallow clone, an alternate object
store, replace references, hooks, an fsmonitor program, filters, a linked worktree and a reftable
repository. Each case hashes every file of the source, .git included, before and after, and checks
that no marker of a stand-in program exists.

The guard: the in-process cases replace subprocess.Popen with a recorder for one run of the tool's
main() each (a clean scan, a written snapshot, a refusal, a missing object, a linked worktree, and two
crashes). A case fails when any recorded start is not Git, or names the source's work tree, Git folder
or common Git folder in its arguments, its working folder or its environment. A static case fails when
the tool's text names git_child.existing or git_child.locate, the two helpers that open a real
repository.

Usage: python tools/test_snapshot_source.py (from the repository root)
"""

import contextlib
import importlib.util
import io
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from snapshot_test_support import (  # noqa: E402
    PUBLIC_AUTHOR, TOOL, Results, address, fingerprint, git, git_child, make_source, marker_script,
    remove_object, run, stand_in_helper, write_files,
)

results = Results()
case = results.case
# The refusal of the completeness check (make_public_snapshot.snapshot), before any blob is read.
REASON = "missing from the source repository's own object store"
CLEAN = {"README.md": "# Fake project\n\nPlain text, no private data.\n", "src/main.rs": "fn main() {}\n"}
PRIVATE = address("private.person", "private-mail.test")
HOOKS = ("pre-commit", "post-commit", "post-checkout", "post-index-change", "reference-transaction", "pre-auto-gc", "fsmonitor-watchman")


def with_path(folder):
    return dict(os.environ, PATH=str(folder) + os.pathsep + os.environ["PATH"])


def outcome(result):
    return (result.stdout + result.stderr).strip()[-600:]


def append_config(repo_git_dir, text):
    """Add configuration by writing the file, so that no later Git call of the test meets it."""
    with open(Path(repo_git_dir) / "config", "a", encoding="utf-8", newline="\n") as config:
        config.write(text)


def partial_clone(root):
    """A partial clone of a stand-in remote, with one blob of HEAD missing. A Git that reads the blob
    in this repository starts the stand-in helper through a lazy fetch."""
    repo = make_source(root, CLEAN, name="partial")
    remove_object(repo, git(repo, "rev-parse", "HEAD:README.md").strip())
    append_config(repo / ".git", "[extensions]\n\tpartialClone = origin\n[remote \"origin\"]\n\turl = akmstandin::fake-remote\n\tpromisor = true\n\tpartialclonefilter = blob:none\n")
    git(repo, "config", "core.repositoryformatversion", "1")
    return repo


spec = importlib.util.spec_from_file_location("snapshot_tool", TOOL)
tool = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = tool
spec.loader.exec_module(tool)


class Recorder:
    """Records every process that the code under test starts, and starts it."""

    real = subprocess.Popen

    def __init__(self):
        self.starts = []

    def __enter__(self):
        recorder = self

        class Recording(Recorder.real):
            def __init__(self, args, *rest, **options):
                recorder.starts.append((list(args) if not isinstance(args, str) else [args], options.get("cwd"), options.get("env")))
                super().__init__(args, *rest, **options)

        subprocess.Popen = Recording
        return self

    def __exit__(self, *_exc):
        subprocess.Popen = Recorder.real


def source_roots(work_tree):
    """The work tree, its Git folder and its common Git folder, read from the files."""
    work_tree = Path(work_tree).resolve()
    dot = work_tree / ".git"
    git_dir = dot
    if dot.is_file():
        target = Path(dot.read_text(encoding="utf-8").split(":", 1)[1].strip())
        git_dir = (target if target.is_absolute() else work_tree / target).resolve()
    common = git_dir
    if (git_dir / "commondir").is_file():
        common = (git_dir / (git_dir / "commondir").read_text(encoding="utf-8").strip()).resolve()
    return {os.path.normcase(str(path)) for path in (work_tree, git_dir, common)}


def offending(starts, roots):
    """The recorded starts that are not Git, or that name a source folder."""
    found = []
    for args, cwd, env in starts:
        if Path(str(args[0])).stem.lower() != git_child.PROGRAM:
            found.append(f"not Git: {args[:3]}")
            continue
        texts = [str(arg) for arg in args] + ([str(cwd)] if cwd else []) + [str(value) for value in (env or {}).values()]
        for text in texts:
            normal = os.path.normcase(text.replace("/", "\\"))
            if any(root in normal for root in roots):
                found.append(f"names the source: {' '.join(str(arg) for arg in args[:6])} (cwd {cwd})")
                break
    return found


def main_in_process(*argv):
    """(exit code or the exception, printed text, recorded starts) of one in-process run of main()."""
    saved = sys.argv
    sys.argv = [str(TOOL), *argv]
    printed = io.StringIO()
    try:
        with Recorder() as recorder, contextlib.redirect_stdout(printed):
            try:
                code = tool.main()
            except BaseException as error:  # noqa: BLE001 (a crash is a result here)
                code = error
    finally:
        sys.argv = saved
    return code, printed.getvalue(), recorder.starts


with tempfile.TemporaryDirectory(prefix="akm-snapshot-source-test-") as folder:
    base = Path(folder)

    # U4-R2-01, the reviewer's closing test: a partial clone with a missing object.
    root = base / "partial"
    root.mkdir()
    bin_folder, helper_marker = stand_in_helper(root)
    repo = partial_clone(root)
    before = fingerprint(repo)
    for label, args in (("a scan", ("--scan-only",)), ("a write", ("--out", str(root / "public"), "--author", PUBLIC_AUTHOR))):
        result = run(repo, *args, env=with_path(bin_folder))
        printed = outcome(result)
        case(f"a partial clone with a missing object, {label}: refused with the completeness check's plain reason", result.returncode == 1 and "REFUSED" in printed and REASON in printed, printed)
        case(f"a partial clone with a missing object, {label}: the stand-in remote helper never started (nothing fetched)", not helper_marker.exists(), printed)
        case(f"a partial clone with a missing object, {label}: every byte of the source is unchanged, .git included", fingerprint(repo) == before)
        case(f"a partial clone with a missing object, {label}: no output folder", not (root / "public").exists())

    # A shallow clone: the revision's own objects are present, its history is cut.
    root = base / "shallow"
    root.mkdir()
    bin_folder, helper_marker = stand_in_helper(root)
    repo = make_source(root, {**CLEAN, "kept.txt": "second\n"}, history={**CLEAN, "kept.txt": "first\n"}, name="shallow")
    head = git(repo, "rev-parse", "HEAD").strip()
    remove_object(repo, git(repo, "rev-parse", "HEAD~1").strip())
    (repo / ".git" / "shallow").write_bytes(f"{head}\n".encode("ascii"))
    append_config(repo / ".git", "[remote \"origin\"]\n\turl = akmstandin::fake-remote\n")
    before = fingerprint(repo)
    result = run(repo, "--out", str(root / "public"), "--author", PUBLIC_AUTHOR, env=with_path(bin_folder))
    printed = outcome(result)
    case("a shallow clone whose revision is complete is written", result.returncode == 0 and "WROTE" in printed and (root / "public" / "kept.txt").read_bytes() == b"second\n", printed)
    result = run(repo, "--scan-only", env=with_path(bin_folder), rev="HEAD~1")
    printed = outcome(result)
    case("a shallow clone: a revision past the cut is refused", result.returncode == 1 and "REFUSED" in printed, printed)
    case("a shallow clone: no helper started and every byte of the source is unchanged", not helper_marker.exists() and fingerprint(repo) == before, printed)

    # An alternate object store: an object that only the alternate holds is not in the copy.
    root = base / "alternates"
    root.mkdir()
    shared = "a file whose blob lives only in the alternate store\n"
    store = make_source(root, {"shared.txt": shared}, name="store")
    repo = make_source(root, CLEAN, name="borrower")
    (repo / ".git" / "objects" / "info").mkdir(exist_ok=True)
    (repo / ".git" / "objects" / "info" / "alternates").write_bytes(f"{(store / '.git' / 'objects').as_posix()}\n".encode("utf-8"))
    write_files(repo, {"shared.txt": shared})
    git(repo, "add", "shared.txt")
    git(repo, "commit", "-q", "--no-gpg-sign", "-m", "borrow")
    blob = git(repo, "rev-parse", "HEAD:shared.txt").strip()
    # On Windows Git cannot refresh the read-only file in the alternate, so it writes its own copy too.
    if (repo / ".git" / "objects" / blob[:2] / blob[2:]).exists():
        remove_object(repo, blob)
    local_copy = (repo / ".git" / "objects" / blob[:2] / blob[2:]).exists()
    readable = git(repo, "cat-file", "-e", blob, check=False).returncode == 0
    before, store_before = fingerprint(repo), fingerprint(store)
    result = run(repo, "--out", str(root / "public"), "--author", PUBLIC_AUTHOR)
    printed = outcome(result)
    case("an object that only an alternate holds refuses the snapshot", readable and not local_copy and result.returncode == 1 and REASON in printed and not (root / "public").exists(), printed)
    case("an alternate: the source and the alternate store are unchanged", fingerprint(repo) == before and fingerprint(store) == store_before)

    # Replace references, loose and packed: the snapshot holds the commit's own blobs.
    root = base / "replace"
    root.mkdir()
    repo = make_source(root, {**CLEAN, "real.txt": "the real text\n", "second.txt": "the second real text\n", "decoy.txt": "a decoy\n"}, name="replace")
    oids = {name: git(repo, "rev-parse", f"HEAD:{name}").strip() for name in ("real.txt", "second.txt", "decoy.txt")}
    git(repo, "replace", oids["real.txt"], oids["decoy.txt"])
    git(repo, "pack-refs", "--all")
    git(repo, "replace", oids["second.txt"], oids["decoy.txt"])
    before = fingerprint(repo)
    out = root / "public"
    result = run(repo, "--out", str(out), "--author", PUBLIC_AUTHOR)
    printed = outcome(result)
    written = result.returncode == 0 and out.is_dir()
    case("replace references: the snapshot holds the commit's own blobs, not the replacements",
         written and (out / "real.txt").read_bytes() == b"the real text\n" and (out / "second.txt").read_bytes() == b"the second real text\n", printed)
    case("replace references: the snapshot tree is the commit's tree", written and git(out, "rev-parse", "HEAD^{tree}") == git(repo, "--no-replace-objects", "rev-parse", "HEAD^{tree}"))
    case("replace references: the source is unchanged", fingerprint(repo) == before)

    # Hooks, an fsmonitor program, filters, a textconv, attributes and grafts in the source.
    root = base / "hostile-config"
    root.mkdir()
    repo = make_source(root, {**CLEAN, ".gitattributes": "*.txt filter=akm diff=akm\n", "notes.txt": "plain\n"}, name="hostile")
    markers = []
    hooks = root / "hooks"
    hooks.mkdir()
    for hook in HOOKS:
        script, marker = marker_script(root, f"hook-{hook}")
        for target in (hooks / hook, repo / ".git" / "hooks" / hook):
            target.parent.mkdir(exist_ok=True)
            target.write_bytes(script.read_bytes())
        markers.append(marker)
    programs = {}
    for name in ("fsmonitor", "filter-clean", "filter-smudge", "filter-process", "textconv", "alternate-refs"):
        programs[name], marker = marker_script(root, name)
        markers.append(marker)
    (repo / ".git" / "info").mkdir(exist_ok=True)
    (repo / ".git" / "info" / "attributes").write_bytes(b"* filter=akm diff=akm\n")
    (repo / ".git" / "info" / "grafts").write_bytes(f"{git(repo, 'rev-parse', 'HEAD').strip()}\n".encode("ascii"))
    (repo / "notes.txt").write_bytes(b"plain, changed in the work tree so that its stat is dirty\n")
    append_config(repo / ".git", (
        f"[core]\n\thooksPath = {hooks.as_posix()}\n\tfsmonitor = {programs['fsmonitor'].as_posix()}\n\talternateRefsCommand = {programs['alternate-refs'].as_posix()}\n"
        f"[filter \"akm\"]\n\tclean = {programs['filter-clean'].as_posix()}\n\tsmudge = {programs['filter-smudge'].as_posix()}\n\tprocess = {programs['filter-process'].as_posix()}\n"
        f"[diff \"akm\"]\n\ttextconv = {programs['textconv'].as_posix()}\n"
    ))
    before = fingerprint(repo)
    out = root / "public"
    for label, args in (("a scan", ("--scan-only",)), ("a write", ("--out", str(out), "--author", PUBLIC_AUTHOR))):
        result = run(repo, *args)
        printed = outcome(result)
        case(f"hooks, fsmonitor, filters and textconv in the source, {label}: the tool succeeds", result.returncode == 0, printed)
        ran = [marker.name for marker in markers if marker.exists()]
        case(f"hooks, fsmonitor, filters and textconv in the source, {label}: none of their programs ran", not ran, str(ran))
        case(f"hooks, fsmonitor, filters and textconv in the source, {label}: the source is unchanged", fingerprint(repo) == before)
    case("hooks and filters in the source: the snapshot holds the committed bytes", out.is_dir() and (out / "notes.txt").read_bytes() == b"plain\n")

    # A linked worktree as the source, and an output folder inside its common Git folder.
    root = base / "worktree"
    root.mkdir()
    main = make_source(root, {**CLEAN, "branch.txt": "main\n"}, name="main")
    linked = root / "linked"
    git(main, "worktree", "add", "-q", "-b", "side", str(linked))
    linked_repo = git_child.Repository(main / ".git" / "worktrees" / "linked", linked, isolated=True)
    write_files(linked, {"branch.txt": "side\n"})
    linked_repo.run("add", "branch.txt")
    linked_repo.run("-c", "user.name=T", "-c", f"user.email={address('t', 'example.com')}", "commit", "-q", "--no-gpg-sign", "-m", "side")
    before_main, before_linked = fingerprint(main), fingerprint(linked)
    out = root / "public"
    result = run(linked, "--out", str(out), "--author", PUBLIC_AUTHOR)
    printed = outcome(result)
    side_tree = linked_repo.run("rev-parse", "HEAD^{tree}").decode().strip()
    case("a linked worktree: the snapshot is the worktree's HEAD", result.returncode == 0 and out.is_dir() and git(out, "rev-parse", "HEAD^{tree}").strip() == side_tree and (out / "branch.txt").read_bytes() == b"side\n", printed)
    for inside in (main / ".git" / "akm-out", main / ".git" / "worktrees" / "linked" / "akm-out"):
        result = run(linked, "--out", str(inside), "--author", PUBLIC_AUTHOR)
        case(f"a linked worktree: an output folder inside the source's Git data ({inside.parent.name}) is refused", result.returncode == 1 and not inside.exists(), outcome(result))
    case("a linked worktree: the main repository and the worktree are unchanged", fingerprint(main) == before_main and fingerprint(linked) == before_linked)

    # A reftable repository: the tool reads only the files format of references, so it refuses.
    root = base / "reftable"
    root.mkdir()
    repo = make_source(root, CLEAN, name="reftable")
    migrated = git(repo, "refs", "migrate", "--ref-format=reftable", check=False).returncode == 0
    before = fingerprint(repo)
    result = run(repo, "--scan-only")
    printed = outcome(result)
    case("a reftable source is refused with a plain reason", migrated and result.returncode == 1 and "stores its references in the reftable format" in printed, printed)
    case("a reftable source is unchanged", fingerprint(repo) == before)

    # The guard, and the byte-identity of a normal, a refused and a crashed run.
    root = base / "guard"
    root.mkdir()
    bin_folder, helper_marker = stand_in_helper(root)
    clean = make_source(root, CLEAN, name="clean")
    private = make_source(root, {**CLEAN, "notes.md": f"write to {PRIVATE}\n"}, name="private")
    partial = partial_clone(root)
    main = make_source(root, CLEAN, name="main")
    linked = root / "linked"
    git(main, "worktree", "add", "-q", "-b", "side", str(linked))
    saved_path = os.environ["PATH"]
    os.environ["PATH"] = str(bin_folder) + os.pathsep + saved_path
    runs = [
        ("a clean scan", clean, ("--scan-only",), 0),
        ("a written snapshot", clean, ("--out", str(root / "out-clean"), "--author", PUBLIC_AUTHOR), 0),
        ("the opaque suggestions", clean, ("--suggest-opaque",), 0),
        ("a refusal for private data", private, ("--out", str(root / "out-private"), "--author", PUBLIC_AUTHOR), 1),
        ("a refusal for a missing object", partial, ("--scan-only",), 1),
        ("a linked worktree", linked, ("--scan-only",), 0),
    ]
    try:
        for label, repo, args, expected in runs:
            watched = [repo, main] if repo == linked else [repo]
            roots = set().union(*(source_roots(folder) for folder in watched))
            before = [fingerprint(folder) for folder in watched]
            code, printed, starts = main_in_process("--repo", str(repo), "--rev", "HEAD", *args)
            found = offending(starts, roots)
            case(f"guard, {label}: the tool started Git, and never on the source", starts and not found, "; ".join(found[:4]) or f"no start recorded; {printed[-300:]}")
            case(f"guard, {label}: exit {expected}", code == expected, f"got {code!r}: {printed[-400:]}")
            case(f"guard, {label}: every byte of the source is unchanged", [fingerprint(folder) for folder in watched] == before)
        case("guard: the stand-in remote helper never started", not helper_marker.exists())

        # A crash while the tool copies the Git folder, and a crash while it writes the snapshot.
        made = []
        real_mkdtemp = tempfile.mkdtemp

        def recording_mkdtemp(*args, **options):
            path = real_mkdtemp(*args, **options)
            made.append(Path(path))
            return path

        crashes = [("copying the Git folder", "copyfile"), ("writing the snapshot", "write-tree")]
        for label, where in crashes:
            made.clear()
            calls = []
            patches = []
            if where == "copyfile":
                real_copyfile = shutil.copyfile

                def crashing_copyfile(source, target, *rest, **options):
                    calls.append(source)
                    if len(calls) == 3:
                        raise RuntimeError("injected crash while copying")
                    return real_copyfile(source, target, *rest, **options)

                patches.append((shutil, "copyfile", real_copyfile, crashing_copyfile))
            else:
                real_run = git_child.Repository.run

                def crashing_run(self, *arguments, **options):
                    if arguments[:1] == ("write-tree",):
                        raise RuntimeError("injected crash while writing")
                    return real_run(self, *arguments, **options)

                patches.append((git_child.Repository, "run", real_run, crashing_run))
            patches.append((tempfile, "mkdtemp", real_mkdtemp, recording_mkdtemp))
            before = fingerprint(clean)
            out = root / f"out-crash-{where}"
            for owner, name, _real, fake in patches:
                setattr(owner, name, fake)
            try:
                code, printed, starts = main_in_process("--repo", str(clean), "--rev", "HEAD", "--out", str(out), "--author", PUBLIC_AUTHOR)
            finally:
                for owner, name, real, _fake in patches:
                    setattr(owner, name, real)
            crashed = isinstance(code, RuntimeError)
            case(f"a crash while {label}: the crash reaches the caller", crashed, f"got {code!r}: {printed[-300:]}")
            case(f"a crash while {label}: the tool's own temporary copy is removed", crashed and made and not any(path.exists() for path in made), str(made))
            case(f"a crash while {label}: no output folder and no unfinished folder", not out.exists() and not list(root.glob(f".{out.name}.incomplete-*")))
            case(f"a crash while {label}: every byte of the source is unchanged", fingerprint(clean) == before)
            case(f"a crash while {label}: Git never started on the source", not offending(starts, source_roots(clean)), "; ".join(offending(starts, source_roots(clean))[:3]))
    finally:
        os.environ["PATH"] = saved_path

text = TOOL.read_text(encoding="utf-8")
case("the tool's text names no helper that opens a real repository (git_child.existing, git_child.locate)", "git_child.existing" not in text and "git_child.locate" not in text)

sys.exit(results.finish())
