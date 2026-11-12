"""The doc check fails on real past mistakes and on synthetic ones, and passes on the working tree.

Usage: python tools/test_check_docs.py (from the repository root)

The past mistakes are copies of the exact lines in tools/fixtures/check_docs_past_mistakes.json, so the
test needs no Git history and runs in a one-commit public snapshot too (finding U4-R1-05). When the
commit that a fixture came from is in the history, the test also checks that the copy is exact.
"""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

CHECK_PATH = Path(__file__).resolve().parent / "check_docs.py"
sys.path.insert(0, str(CHECK_PATH.parent))
import git_child  # noqa: E402
CHECK = [sys.executable, "tools/check_docs.py"]
PAST = json.loads((CHECK_PATH.parent / "fixtures" / "check_docs_past_mistakes.json").read_text(encoding="utf-8"))

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

CLAIMS_HEAD = "# Claims\n\n| Claim | Where stated | Evidence | Limit |\n|---|---|---|---|\n"
WORKFLOW = ".github/workflows/native-proof.yml"


def claims(*rows):
    """A docs/CLAIMS.md with these table rows; the first row is on line 5."""
    return {"docs/CLAIMS.md": CLAIMS_HEAD + "".join(row + "\n" for row in rows)}


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
    ("a claim whose evidence is a test passes", "", None, claims("| A claim. | README.md | `a_real_test_with_four_words` | None. |")),
    (
        "a claim whose evidence is not a fn fails",
        "",
        "docs/CLAIMS.md:5: evidence `no_such_test_here_at_all` is neither a test fn nor a proof name",
        claims("| A claim. | README.md | `no_such_test_here_at_all` | None. |"),
    ),
    (
        "a claim whose evidence is a plain fn fails, also without the word test",
        "",
        "docs/CLAIMS.md:5: evidence `a_plain_function_that_is_not_a_test` is neither a test fn nor a proof name",
        claims("| A claim. | README.md | `a_plain_function_that_is_not_a_test` | None. |"),
    ),
    (
        "a second evidence item is checked too",
        "",
        "docs/CLAIMS.md:6: evidence `another_helper_with_many_words` is neither a test fn nor a proof name",
        claims("| A claim. | README.md | `a_real_test_with_four_words` | None. |", "| B claim. | README.md | `a_real_test_with_four_words`, `another_helper_with_many_words` | None. |"),
    ),
    ("a claim without evidence fails", "", "docs/CLAIMS.md:5: the evidence cell names no test fn and no proof name", claims("| A claim. | README.md | read the source | None. |")),
    ("an unknown proof name fails", "", "docs/CLAIMS.md:5: evidence `made-up-proof` is neither a test fn nor a proof name", claims("| A claim. | README.md | `made-up-proof` | None. |")),
    (
        "a proof name passes when its file holds its job",
        "",
        None,
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), WORKFLOW: "jobs:\n  install-upgrade-uninstall:\n    runs-on: x\n"},
    ),
    (
        "a proof name fails when its file lacks its job",
        "",
        "docs/CLAIMS.md:5: proof `native-proof-install` needs .github/workflows/native-proof.yml with",
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), WORKFLOW: "jobs:\n  build:\n    runs-on: x\n"},
    ),
    ("a proof name fails when its file is missing", "", "docs/CLAIMS.md:5: proof `native-proof-install` needs .github/workflows/native-proof.yml", claims("| A claim. | README.md | `native-proof-install` | Never ran. |")),
    ("a claim row needs four cells", "", "docs/CLAIMS.md:5: a claim row needs 4 cells (claim, where stated, evidence, limit), not 3", claims("| A claim. | `a_real_test_with_four_words` | None. |")),
    ("a claim with an empty limit fails", "", "docs/CLAIMS.md:5: the limit cell is empty", claims("| A claim. | README.md | `a_real_test_with_four_words` |  |")),
    (
        "a table with another header is not a claim table",
        "",
        None,
        {"docs/CLAIMS.md": "| Proof name | What runs |\n|---|---|\n| `made-up-proof` | x |\n\n" + CLAIMS_HEAD.split("\n", 2)[2] + "| A claim. | README.md | `a_real_test_with_four_words` | None. |\n"},
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
        repository = git_child.create(root)
        repository.run("add", *files)
        return subprocess.run([sys.executable, str(CHECK_PATH)], cwd=root, capture_output=True, text=True)


def run_past(fixture):
    """Run the real script on the past line, at its past line number, with the Rust tests of that commit."""
    with tempfile.TemporaryDirectory(prefix="akm-check-docs-test-") as folder:
        root = Path(folder)
        files = {fixture["path"]: "\n" * (fixture["line_number"] - 1) + fixture["line"] + "\n", **fixture["rust"]}
        for name, text in files.items():
            (root / name).parent.mkdir(parents=True, exist_ok=True)
            (root / name).write_bytes(text.encode("utf-8"))
        repository = git_child.create(root)
        repository.run("add", *files)
        return subprocess.run([sys.executable, str(CHECK_PATH)], cwd=root, capture_output=True, text=True)


failures = 0
here = git_child.existing(Path.cwd())
for fixture in PAST:
    run = run_past(fixture)
    ok = run.returncode == 1 and fixture["expected"] in run.stdout
    print(f"{'ok  ' if ok else 'FAIL'} the check fails on {fixture['origin']} ({fixture['name']}) with: {fixture['expected']}")
    failures += not ok
    commit, path, number = fixture["origin"].split(":")
    if here.completed("cat-file", "-e", f"{commit}^{{commit}}").returncode != 0:
        print(f"skip the copy of {fixture['origin']} is not compared: commit {commit} is not in this history (for example a public snapshot)")
        continue
    past_line = here.run("show", f"{commit}:{path}").decode("utf-8").split("\n")[int(number) - 1].removesuffix("\r")
    ok = past_line == fixture["line"]
    print(f"{'ok  ' if ok else 'FAIL'} the fixture is an exact copy of {fixture['origin']}")
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

# A parser that matched no row would pass every claim unchecked, so the real file must yield its rows.
claims_file = Path("docs/CLAIMS.md")
claims_text = claims_file.read_text(encoding="utf-8") if claims_file.exists() else ""
rows = 0
in_claims = False
for line in claims_text.split("\n"):
    in_claims = line.startswith("|") and (in_claims or line.startswith("| Claim |"))
    rows += in_claims and not line.startswith(("| Claim |", "|-"))
counted = f"{rows} claims checked" in run.stdout
ok = rows >= 20 and counted
print(f"{'ok  ' if ok else 'FAIL'} every row of docs/CLAIMS.md is checked ({rows} rows)")
failures += not ok
if not ok:
    print(f"     expected '{rows} claims checked' in: {run.stdout.strip()[-200:]}")
print("all cases passed" if not failures else f"{failures} case(s) failed")
sys.exit(1 if failures else 0)
