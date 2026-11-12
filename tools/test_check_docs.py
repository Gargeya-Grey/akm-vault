"""The doc check fails on real past mistakes and on synthetic ones, and passes on the working tree.

Usage: python tools/test_check_docs.py (from the repository root)
"""

import subprocess
import sys
import tempfile
from pathlib import Path

CHECK_PATH = Path(__file__).resolve().parent / "check_docs.py"
CHECK = [sys.executable, "tools/check_docs.py"]
PAST = {
    # The threat model as the security branch first wrote it: "%APPDATA%\akm" lost its backslashes to escapes.
    "cfce854": "docs/THREAT_MODEL.md:60: control character 0x07",
    # Review 1 finding 9: the threat model named a test that unit 1h had removed.
    "e9e5fb9": "names `locking_clears_only_the_sessions_clipboard_and_invalidates_imports`",
}

RUST = """\
#[test]
fn a_real_test_with_four_words() {}

#[test]
#[ignore]
fn an_ignored_test_with_other_attributes() {}

#[tokio::test]
async fn an_async_test_from_a_runtime_attribute() {}

pub fn a_plain_function_that_is_not_a_test() {}

fn another_helper_with_many_words() {}

mod tests {
    #[test]
    fn a_nested_test_is_a_test() {}
}
"""

CUSTOM_MANIFEST = """\
[package]
name = "app"

[[test]]
name = "custom"
path = "tests/custom.rs"
harness = false
"""

# (name, markdown text, expected substring of the output or None when the check must pass, extra files)
SYNTHETIC = [
    ("a vertical tab is a control character", "line one\n\x0bline two\n", "notes.md:2: control character 0x0b"),
    ("a form feed is a control character", "line one\nline two\x0c\nline three\n", "notes.md:2: control character 0x0c"),
    ("a vertical tab does not shift the line numbers", "a\x0bb\nc\nd `no_such_test_here_at_all`\n", "notes.md:3: names `no_such_test_here_at_all`"),
    ("a plain function is not a test", "The test `a_plain_function_that_is_not_a_test` covers it.\n", "names `a_plain_function_that_is_not_a_test`, which is a fn but not a test"),
    ("a helper in a test module is not a test", "Tests: `a_real_test_with_four_words`, `another_helper_with_many_words`.\n", "names `another_helper_with_many_words`, which is a fn but not a test"),
    ("a missing name still fails", "See `no_such_test_here_at_all`.\n", "names `no_such_test_here_at_all`, which is not a fn in the Rust sources"),
    ("a function named as an API, not as a test, passes", "Vault gains `a_plain_function_that_is_not_a_test`.\n", None),
    ("a test passes", "See `a_real_test_with_four_words`.\n", None),
    ("a test with another attribute passes", "See `an_ignored_test_with_other_attributes`.\n", None),
    ("an async runtime test passes", "See `an_async_test_from_a_runtime_attribute`.\n", None),
    ("a nested test passes", "See `a_nested_test_is_a_test`.\n", None),
    ("a historical line may keep a removed name", "See `no_such_test_here_at_all`. <!-- check-docs: historical: removed in unit 1 -->\n", None),
    ("a CRLF file with a test name passes", "See `a_real_test_with_four_words`.\r\nMore.\r\n", None),
    (
        "a step of a harness = false target is a test",
        "The test `a_step_of_a_custom_harness` runs it.\n",
        None,
        {"app/Cargo.toml": CUSTOM_MANIFEST, "app/tests/custom.rs": "fn a_step_of_a_custom_harness() {}\nfn main() {}\n"},
    ),
    (
        "a fn in a test file with the default harness is not a test",
        "The test `a_step_of_a_custom_harness` runs it.\n",
        "names `a_step_of_a_custom_harness`, which is a fn but not a test",
        {"app/Cargo.toml": CUSTOM_MANIFEST.replace("harness = false", "harness = true"), "app/tests/custom.rs": "fn a_step_of_a_custom_harness() {}\n"},
    ),
]


def run_synthetic(markdown, extra=None):
    """Run the real script in a throwaway Git repository that holds one Rust file and one Markdown file."""
    with tempfile.TemporaryDirectory(prefix="akm-check-docs-test-") as folder:
        root = Path(folder)
        (root / "lib.rs").write_text(RUST, encoding="utf-8", newline="")
        (root / "notes.md").write_bytes(markdown.encode("utf-8"))
        files = ["lib.rs", "notes.md"]
        for name, text in (extra or {}).items():
            (root / name).parent.mkdir(parents=True, exist_ok=True)
            (root / name).write_text(text, encoding="utf-8", newline="")
            files.append(name)
        subprocess.run(["git", "init", "-q"], cwd=root, check=True)
        subprocess.run(["git", "config", "core.autocrlf", "false"], cwd=root, check=True)
        subprocess.run(["git", "add", *files], cwd=root, check=True)
        return subprocess.run([sys.executable, str(CHECK_PATH)], cwd=root, capture_output=True, text=True)


failures = 0
for rev, expected in PAST.items():
    run = subprocess.run(CHECK + ["--rev", rev], capture_output=True, text=True)
    ok = run.returncode == 1 and expected in run.stdout
    print(f"{'ok  ' if ok else 'FAIL'} the check fails at {rev} with: {expected}")
    failures += not ok

for name, markdown, expected, *rest in SYNTHETIC:
    run = run_synthetic(markdown, rest[0] if rest else None)
    if expected is None:
        ok = run.returncode == 0
    else:
        ok = run.returncode == 1 and expected in run.stdout
    print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if not ok:
        print(f"     expected: {expected or 'a pass'}\n     got exit {run.returncode}: {run.stdout.strip()}")
    failures += not ok

run = subprocess.run(CHECK, capture_output=True, text=True)
ok = run.returncode == 0
print(f"{'ok  ' if ok else 'FAIL'} the working tree passes")
failures += not ok
if not ok:
    print(run.stdout)
print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
