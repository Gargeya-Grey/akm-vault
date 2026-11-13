"""The doc check fails on real past mistakes and on synthetic ones, and passes on the working tree.

Usage: python tools/test_check_docs.py [--require-tools] (from the repository root)

The full-mode cases (findings U4-R1-07, U4-R2-05) make their listings with the real test runners on
small fixtures in temporary folders: cargo on a crate without dependencies (offline), and the Vitest
and Playwright of desktop/node_modules. A runner that is not there is skipped with its reason, and
--require-tools (CI) makes that a failure.

The past mistakes are copies of the exact lines in tools/fixtures/check_docs_past_mistakes.json, so the
test needs no Git history and runs in a one-commit public snapshot too (finding U4-R1-05). When the
commit that a fixture came from is in the history, the test also checks that the copy is exact.
"""

import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

CHECK_PATH = Path(__file__).resolve().parent / "check_docs.py"
sys.path.insert(0, str(CHECK_PATH.parent))
import git_child  # noqa: E402
CHECK = [sys.executable, str(CHECK_PATH)]
REQUIRE_TOOLS = "--require-tools" in sys.argv[1:]
VITEST = Path("desktop/node_modules/vitest/vitest.mjs").resolve()
PLAYWRIGHT = Path("desktop/node_modules/@playwright/test/cli.js").resolve()
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

#[test]
// A comment between the attribute and the fn.
fn a_test_after_a_comment_line() {}

/*
#[test]
fn a_test_inside_a_block_comment() {}
/* nested */
#[test]
fn a_test_after_a_nested_comment_inside_a_block_comment() {}
*/

// #[test] fn a_test_inside_a_line_comment() {}

const TEXT: &str = "#[test] fn a_test_inside_a_string_literal() {}";
const RAW: &str = r#"#[test] fn a_test_inside_a_raw_string() {} "quoted""#;
const ODD: &str = r#"one " quote
#[test]
fn a_test_inside_a_raw_string_with_one_quote() {}
"#;
const QUOTE: char = '"';

#[test]
fn a_test_after_a_quote_char_literal() {}

fn borrow_with_a_lifetime<'a>(text: &'a str) -> &'a str { text }

#[test]
fn a_test_after_a_lifetime_function() {}

#[cfg(any())]
#[test]
fn a_test_that_cfg_any_turns_off() {}

#[cfg(any())]
mod switched_off {
    #[test]
    fn a_test_in_a_module_that_cfg_any_turns_off() {}
}

#[cfg(windows)]
#[test]
fn a_windows_test_runs_in_a_windows_build() {}
"""

# A workflow that runs every workflow proof, and the files that they name.
GOOD_WORKFLOW = """\
name: Native proofs
on: workflow_dispatch
jobs:
  install-upgrade-uninstall:
    runs-on: x
    steps:
      - name: Install test
        run: |
          # the proof
          powershell -File tools/test_install.ps1 -Setup x
  "real-clipboard":
    runs-on: x
    steps:
      - run: akm-desktop-lib-tests.exe clipboard::windows::actual_windows_private_formats_are_present --ignored
"""
BUILD_WORKFLOW = "on: push\njobs:\n  verify:\n    runs-on: x\n    steps:\n      - working-directory: desktop\n        run: npm test\n"
TIMER = "desktop/src/components/KeyDetail.timer.test.tsx"
TITLE = "the countdown text is the only timer, and it resets on each reveal"
CLIPBOARD_TEST = "#[test]\n#[ignore]\nfn actual_windows_private_formats_are_present() {}\n"
INSTALL = {"tools/test_install.ps1": "# a fake install test\n"}

CUSTOM_MANIFEST = """\
[package]
name = "app"

[[test]]
name = "custom"
path = "tests/custom.rs"
harness = false
"""

# Finding U4-R2-05: text that is not a test, which both modes must refuse.
ENV_WORKFLOW = "jobs:\n  install-upgrade-uninstall:\n    runs-on: x\n    steps:\n      - env:\n          run: powershell -File tools/test_install.ps1\n        run: echo nothing\n"
STRING_TIMER = f'const text = \'test("{TITLE}", () => {{}})\';\ntest("another test", () => {{}});\n'
SKIPPED_GROUP_TIMER = f'describe.skip("a group", () => {{\n  test("{TITLE}", () => {{}});\n}});\ntest("another test", () => {{}});\n'
NEVER_CALLED_TIMER = f'function never() {{\n  test("{TITLE}", () => {{}});\n}}\ntest("another test", () => {{}});\n'
GOOD_TIMER = f'test("{TITLE}", () => {{}});\n'

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
        "a proof name passes when its job runs its script",
        "",
        None,
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), WORKFLOW: GOOD_WORKFLOW, **INSTALL},
    ),
    (
        "a proof name fails when its workflow lacks its job",
        "",
        "docs/CLAIMS.md:5: proof `native-proof-install` needs the job `install-upgrade-uninstall` in the jobs of .github/workflows/native-proof.yml",
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), WORKFLOW: "jobs:\n  build:\n    runs-on: x\n", **INSTALL},
    ),
    (
        "a proof name fails when its workflow is missing",
        "",
        "docs/CLAIMS.md:5: proof `native-proof-install` needs the workflow .github/workflows/native-proof.yml, which is not in the checked tree",
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), **INSTALL},
    ),
    # Finding U4-R1-07: evidence resolves to things that exist and run, not to matching text.
    (
        "a proof fails when its script is tracked but deleted from the working tree",
        "",
        "docs/CLAIMS.md:5: proof `native-gui-proof` needs the file tools/verify_windows_app.ps1, which is not in the checked tree",
        {**claims("| A claim. | README.md | `native-gui-proof` | Owner only. |"), "tools/verify_windows_app.ps1": "# fake\n"},
        ["tools/verify_windows_app.ps1"],
    ),
    ("a proof passes when its script is present", "", None, {**claims("| A claim. | README.md | `native-gui-proof` | Owner only. |"), "tools/verify_windows_app.ps1": "# fake\n"}),
    (
        "a proof fails when its script is present but not tracked",
        "",
        "docs/CLAIMS.md:5: proof `native-gui-proof` needs the file tools/verify_windows_app.ps1, which is not in the checked tree",
        claims("| A claim. | README.md | `native-gui-proof` | Owner only. |"),
        (),
        {"tools/verify_windows_app.ps1": "# fake, never added\n"},
    ),
    (
        "a run command in a block scalar of another key is not a run step",
        "",
        "proof `native-proof-install` needs a run step of job `install-upgrade-uninstall` that holds 'tools/test_install.ps1'",
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), **INSTALL,
         WORKFLOW: "jobs:\n  install-upgrade-uninstall:\n    runs-on: x\n    steps:\n      - name: x\n        env:\n          NOTE: |\n            run: powershell -File tools/test_install.ps1\n        run: echo nothing\n"},
    ),
    (
        "a job name in a block scalar is not a job",
        "",
        "needs the job `install-upgrade-uninstall` in the jobs of",
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), **INSTALL,
         WORKFLOW: "name: x\ndescription: |\n  install-upgrade-uninstall:\n    runs powershell -File tools/test_install.ps1\njobs:\n  build:\n    runs-on: x\n"},
    ),
    (
        "a job name in a comment is not a job",
        "",
        "needs the job `install-upgrade-uninstall` in the jobs of",
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), **INSTALL,
         WORKFLOW: "jobs:\n  build:\n    runs-on: x\n#  install-upgrade-uninstall:\n  # install-upgrade-uninstall:\n"},
    ),
    (
        "a job name in a quoted scalar that spans lines is not a job",
        "",
        "needs the job `install-upgrade-uninstall` in the jobs of",
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), **INSTALL,
         WORKFLOW: 'jobs:\n  build:\n    runs-on: "first line\n  install-upgrade-uninstall:\n      powershell -File tools/test_install.ps1"\n'},
    ),
    (
        "a job that does not run its script fails",
        "",
        "proof `native-proof-install` needs a run step of job `install-upgrade-uninstall` that holds 'tools/test_install.ps1'",
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), **INSTALL,
         WORKFLOW: "jobs:\n  install-upgrade-uninstall:\n    runs-on: x\n    env:\n      NOTE: tools/test_install.ps1\n    steps:\n      - run: echo nothing\n"},
    ),
    (
        "a job whose script is only in a comment line of its run step fails",
        "",
        "proof `native-proof-install` needs a run step of job `install-upgrade-uninstall` that holds 'tools/test_install.ps1'",
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), **INSTALL,
         WORKFLOW: "jobs:\n  install-upgrade-uninstall:\n    steps:\n      - run: |\n          # powershell -File tools/test_install.ps1\n          echo nothing\n"},
    ),
    ("a test declaration in a block comment is not a fn", "See `a_test_inside_a_block_comment`.\n", "names `a_test_inside_a_block_comment`, which is not a fn in the Rust sources"),
    ("a test after a nested comment inside a block comment is not a fn", "See `a_test_after_a_nested_comment_inside_a_block_comment`.\n", "which is not a fn in the Rust sources"),
    ("a test declaration in a line comment is not a fn", "See `a_test_inside_a_line_comment`.\n", "names `a_test_inside_a_line_comment`, which is not a fn in the Rust sources"),
    ("a test declaration in a string literal is not a fn", "See `a_test_inside_a_string_literal`.\n", "names `a_test_inside_a_string_literal`, which is not a fn in the Rust sources"),
    ("a test declaration in a raw string is not a fn", "See `a_test_inside_a_raw_string`.\n", "names `a_test_inside_a_raw_string`, which is not a fn in the Rust sources"),
    (
        "a test declaration in a raw string with one inner quote is not a fn",
        "See `a_test_inside_a_raw_string_with_one_quote`.\n",
        "names `a_test_inside_a_raw_string_with_one_quote`, which is not a fn in the Rust sources",
    ),
    (
        "a claim whose evidence is a commented test fails",
        "",
        "docs/CLAIMS.md:5: evidence `a_test_inside_a_block_comment` is neither a test fn nor a proof name",
        claims("| A claim. | README.md | `a_test_inside_a_block_comment` | None. |"),
    ),
    ("a test after a comment line between attribute and fn passes", "See `a_test_after_a_comment_line`.\n", None),
    ("a test after a quote char literal passes", "See `a_test_after_a_quote_char_literal`.\n", None),
    ("a test after a function with lifetimes passes", "See `a_test_after_a_lifetime_function`.\n", None),
    (
        "a proof's Rust test passes when it is a test",
        "",
        None,
        {**claims("| A claim. | README.md | `native-proof-real-clipboard` | Never ran. |"), WORKFLOW: GOOD_WORKFLOW, "clip.rs": CLIPBOARD_TEST},
    ),
    (
        "a proof's Rust test fails when it is not a test",
        "",
        "proof `native-proof-real-clipboard` runs `actual_windows_private_formats_are_present`, which is not an ignored test fn in the Rust sources",
        {**claims("| A claim. | README.md | `native-proof-real-clipboard` | Never ran. |"), WORKFLOW: GOOD_WORKFLOW, "clip.rs": "// " + CLIPBOARD_TEST.replace("\n", "\n// ")},
    ),
    (
        "a frontend proof passes when a test call has its title",
        "",
        None,
        {**claims("| A claim. | README.md | `frontend-reveal-timer` | None. |"), ".github/workflows/build.yml": BUILD_WORKFLOW, TIMER: f'import {{ test }} from "vitest";\ntest("{TITLE}", async () => {{}});\n'},
    ),
    (
        "a frontend proof fails when its title is only in a comment",
        "",
        "proof `frontend-reveal-timer` needs a test( or it( call titled",
        {**claims("| A claim. | README.md | `frontend-reveal-timer` | None. |"), ".github/workflows/build.yml": BUILD_WORKFLOW,
         TIMER: f'// test("{TITLE}", async () => {{}});\n/* it("{TITLE}", () => {{}}); */\nconst url = "https://example.invalid/x";\n'},
    ),
    (
        "a frontend proof fails when its test is skipped",
        "",
        "proof `frontend-reveal-timer` needs a test( or it( call titled",
        {**claims("| A claim. | README.md | `frontend-reveal-timer` | None. |"), ".github/workflows/build.yml": BUILD_WORKFLOW, TIMER: f'test.skip("{TITLE}", async () => {{}});\n'},
    ),
    (
        "a frontend proof fails when the build workflow does not run its tests",
        "",
        "proof `frontend-reveal-timer` needs a run step of job `verify` that holds 'npm test'",
        {**claims("| A claim. | README.md | `frontend-reveal-timer` | None. |"), ".github/workflows/build.yml": BUILD_WORKFLOW.replace("npm test", "npm run build"), TIMER: f'test("{TITLE}", () => {{}});\n'},
    ),
    ("a claim row needs four cells", "", "docs/CLAIMS.md:5: a claim row needs 4 cells (claim, where stated, evidence, limit), not 3", claims("| A claim. | `a_real_test_with_four_words` | None. |")),
    ("a claim with an empty limit fails", "", "docs/CLAIMS.md:5: the limit cell is empty", claims("| A claim. | README.md | `a_real_test_with_four_words` |  |")),
    (
        "a table with another header is not a claim table",
        "",
        None,
        {"docs/CLAIMS.md": "| Example | What runs |\n|---|---|\n| `made-up-proof` | x |\n\n" + CLAIMS_HEAD.split("\n", 2)[2] + "| A claim. | README.md | `a_real_test_with_four_words` | None. |\n"},
    ),
    # The generated THIRD_PARTY_NOTICES.md holds third-party READMEs and change logs: their names are not AKM's tests.
    ("a test-like name in THIRD_PARTY_NOTICES.md passes", "", None, {"THIRD_PARTY_NOTICES.md": "See the test `with_additional_browser_args_of_a_crate`.\n"}),
    ("a control character in THIRD_PARTY_NOTICES.md still fails", "", "THIRD_PARTY_NOTICES.md:2: control character 0x0b", {"THIRD_PARTY_NOTICES.md": "one\n\x0btwo\n"}),
    # Finding U4-R2-05 in fast mode: each fixture of review 2 is refused.
    (
        "a workflow command only in an env value of a step is not a run step",
        "",
        "proof `native-proof-install` needs a run step of job `install-upgrade-uninstall` that holds 'tools/test_install.ps1'",
        {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), **INSTALL, WORKFLOW: ENV_WORKFLOW},
    ),
    (
        "a frontend title only inside a string is not a test",
        "",
        "proof `frontend-reveal-timer` needs a test( or it( call titled",
        {**claims("| A claim. | README.md | `frontend-reveal-timer` | None. |"), ".github/workflows/build.yml": BUILD_WORKFLOW, TIMER: STRING_TIMER},
    ),
    (
        "a frontend test in a skipped describe group is not a test",
        "",
        "proof `frontend-reveal-timer` needs a test( or it( call titled",
        {**claims("| A claim. | README.md | `frontend-reveal-timer` | None. |"), ".github/workflows/build.yml": BUILD_WORKFLOW, TIMER: SKIPPED_GROUP_TIMER},
    ),
    (
        "a frontend test in a file with a .only call is not a test",
        "",
        "proof `frontend-reveal-timer` needs a test( or it( call titled",
        {**claims("| A claim. | README.md | `frontend-reveal-timer` | None. |"), ".github/workflows/build.yml": BUILD_WORKFLOW, TIMER: 'test.only("another test", () => {});\n' + GOOD_TIMER},
    ),
    (
        "a Rust test that cfg(any()) turns off is not evidence",
        "",
        "docs/CLAIMS.md:5: evidence `a_test_that_cfg_any_turns_off` is a test, but `#[cfg(any())]` turns it off in a Windows test build",
        claims("| A claim. | README.md | `a_test_that_cfg_any_turns_off` | None. |"),
    ),
    (
        "a Rust test in a module that cfg(any()) turns off is not evidence",
        "",
        "docs/CLAIMS.md:5: evidence `a_test_in_a_module_that_cfg_any_turns_off` is a test, but `#[cfg(any())]` turns it off",
        claims("| A claim. | README.md | `a_test_in_a_module_that_cfg_any_turns_off` | None. |"),
    ),
    ("a Rust test under cfg(windows) is evidence", "", None, claims("| A claim. | README.md | `a_windows_test_runs_in_a_windows_build` | None. |")),
    (
        "an ignored Rust test is not evidence by itself",
        "",
        "docs/CLAIMS.md:5: evidence `an_ignored_test_with_other_attributes` is an ignored test, which `cargo test` does not run",
        claims("| A claim. | README.md | `an_ignored_test_with_other_attributes` | None. |"),
    ),
]


spec = importlib.util.spec_from_file_location("check_docs_module", CHECK_PATH)
check_docs = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = check_docs
spec.loader.exec_module(check_docs)


def proof_row(name, what=None, where=None):
    proof = check_docs.PROOFS[name]
    named = f"`{proof.job}`" if proof.workflow == check_docs.NATIVE_PROOF else f"`{proof.files[0]}`"
    return f"| `{name}` | {what or named} | {where or ('CI, each push' if proof.workflow == check_docs.BUILD else 'A disposable runner')} |\n"


def with_proof_table(*rows):
    """A docs/CLAIMS.md with a Named proofs table of these rows, and one valid claim."""
    table = "| Proof name | What runs | Where it runs |\n|---|---|---|\n" + "".join(rows)
    return {"docs/CLAIMS.md": "# Claims\n\n" + table + "\n" + CLAIMS_HEAD.split("\n", 2)[2] + "| A claim. | README.md | `a_real_test_with_four_words` | None. |\n"}


ALL_ROWS = [proof_row(name) for name in check_docs.PROOFS]
SYNTHETIC += [
    ("a Named proofs table that lists exactly the proofs passes", "", None, with_proof_table(*ALL_ROWS)),
    (
        "a Named proofs table without one proof fails",
        "",
        "PROOFS in tools/check_docs.py defines `public-snapshot-scan`, which the Named proofs table does not list",
        with_proof_table(*(row for row in ALL_ROWS if "`public-snapshot-scan`" not in row)),
    ),
    ("a Named proofs table with an unknown proof fails", "", "the Named proofs table lists `made-up-proof`, which PROOFS", with_proof_table(*ALL_ROWS, "| `made-up-proof` | `x.py` | CI, each push |\n")),
    (
        "a Named proofs row that says CI for a proof that CI does not run fails",
        "",
        'the row of `native-gui-proof` must say "CI, each push" exactly when the build workflow runs it',
        with_proof_table(*(proof_row(name, where="CI, each push") if name == "native-gui-proof" else proof_row(name) for name in check_docs.PROOFS)),
    ),
    (
        "a Named proofs row that does not name its script fails",
        "",
        "the row of `keepassxc-interchange` must name `tools/verify_keepassxc.py` in its What runs cell",
        with_proof_table(*(proof_row(name, what="`tools/other.py`") if name == "keepassxc-interchange" else proof_row(name) for name in check_docs.PROOFS)),
    ),
]


def run_synthetic(markdown, extra=None, deleted=(), untracked=None, rust=RUST, arguments=()):
    """Run the real script in a throwaway Git repository that holds one Rust file and one Markdown file.
    The `deleted` files are tracked, then removed from the working tree. The `untracked` files are
    written to the working tree and never added."""
    with tempfile.TemporaryDirectory(prefix="akm-check-docs-test-") as folder:
        root = Path(folder)
        (root / "lib.rs").write_text(rust, encoding="utf-8", newline="")
        (root / "notes.md").write_bytes(markdown.encode("utf-8"))
        files = ["lib.rs", "notes.md"]
        for name, text in (extra or {}).items():
            (root / name).parent.mkdir(parents=True, exist_ok=True)
            (root / name).write_text(text, encoding="utf-8", newline="")
            files.append(name)
        repository = git_child.create(root)
        repository.run("add", *files)
        for name in deleted:
            (root / name).unlink()
        for name, text in (untracked or {}).items():
            (root / name).parent.mkdir(parents=True, exist_ok=True)
            (root / name).write_text(text, encoding="utf-8", newline="")
        return subprocess.run([sys.executable, str(CHECK_PATH), *arguments], cwd=root, capture_output=True, text=True)


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
# The past commits are read from a copy of this repository's Git data (git_child.source_copy, class F).
with git_child.source_copy(Path.cwd()) as here:
    for fixture in PAST:
        run = run_past(fixture)
        ok = run.returncode == 1 and fixture["expected"] in run.stdout
        print(f"{'ok  ' if ok else 'FAIL'} the check fails on {fixture['origin']} ({fixture['name']}) with: {fixture['expected']}")
        failures += not ok
        commit, path, number = fixture["origin"].split(":")
        if here.completed("cat-file", "-e", f"{commit}:{path}").returncode != 0:
            print(f"skip the copy of {fixture['origin']} is not compared: commit {commit} is not in this history (for example a public snapshot)")
            continue
        past_line = here.run("cat-file", "blob", f"{commit}:{path}").decode("utf-8").split("\n")[int(number) - 1].removesuffix("\r")
        ok = past_line == fixture["line"]
        print(f"{'ok  ' if ok else 'FAIL'} the fixture is an exact copy of {fixture['origin']}")
        failures += not ok

for name, markdown, expected, *rest in SYNTHETIC:
    run = run_synthetic(markdown, *rest)
    if expected is None:
        ok = run.returncode == 0
    else:
        ok = run.returncode == 1 and expected in run.stdout
    print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if not ok:
        print(f"     expected: {expected or 'a pass'}\n     got exit {run.returncode}: {run.stdout.strip()}")
    failures += not ok

# Full mode (findings U4-R1-07, U4-R2-05): the listings come from the real runners on small fixtures.
RUST_FULL = """\
#[test]
fn a_listed_test_that_runs() {}

#[test]
#[ignore]
fn a_listed_test_that_is_ignored() {}

#[test]
#[ignore]
fn actual_windows_private_formats_are_present() {}

#[cfg(any())]
#[test]
fn a_test_that_cfg_any_turns_off() {}

#[cfg(not(windows))]
mod elsewhere;
"""
ELSEWHERE = "#[test]\nfn a_test_in_a_module_file_that_windows_leaves_out() {}\n"
FIXTURE_CRATE = '[package]\nname = "check-docs-fixture"\nversion = "0.0.0"\nedition = "2021"\n\n[lib]\npath = "lib.rs"\n\n[workspace]\n'
PLAYWRIGHT_SPEC = """\
const { test } = require("@playwright/test");
test("a runnable test", async () => {});
test.skip("a skipped test", async () => {});
test.fixme("a fixme test", async () => {});
test.describe.skip("a skipped group", () => { test("a test in a skipped group", async () => {}); });
const text = 'test("a title in a string", async () => {})';
function never() { test("a test in a function that never runs", async () => {}); }
"""


def missing(tool, reason):
    """Print why a group of cases cannot run; with --require-tools that is a failure."""
    global failures
    print(f"{'FAIL' if REQUIRE_TOOLS else 'skip'} the {tool} cases need {reason}")
    failures += REQUIRE_TOOLS


def command(kind, script=None):
    """The checker's own listing command for `kind`, with the runner's script made absolute, so a change
    to LISTINGS changes the fixtures' listings too."""
    words = check_docs.LISTINGS[kind][0]
    return [str(script) if script and word.startswith("node_modules/") else word for word in words]


def listing(command, folder, env=None):
    done = subprocess.run(command, cwd=folder, capture_output=True, text=True, encoding="utf-8", errors="replace", env=env)
    return done.stdout if done.returncode == 0 else None


def full_case(name, expected, files, arguments):
    global failures
    run = run_synthetic("", {**files, "elsewhere.rs": ELSEWHERE}, rust=RUST_FULL, arguments=("--full", *arguments))
    ok = run.returncode == 0 if expected is None else run.returncode == 1 and expected in run.stdout
    print(f"{'ok  ' if ok else 'FAIL'} {name}")
    if not ok:
        print(f"     expected: {expected or 'a pass'}\n     got exit {run.returncode}: {run.stdout.strip()[-600:]} {run.stderr.strip()[-300:]}")
    failures += not ok


with tempfile.TemporaryDirectory(prefix="akm-check-docs-lists-") as lists:
    lists = Path(lists)
    crate = lists / "crate"
    crate.mkdir()
    for name, content in {"Cargo.toml": FIXTURE_CRATE, "lib.rs": RUST_FULL, "elsewhere.rs": ELSEWHERE}.items():
        (crate / name).write_text(content, encoding="utf-8", newline="")
    env = dict(os.environ, CARGO_TARGET_DIR=str(lists / "target"))
    rust = ignored = None
    if shutil.which("cargo") and listing(["cargo", "generate-lockfile", "--offline"], crate, env) is not None:
        rust, ignored = listing(command("rust"), crate, env), listing(command("rust-ignored"), crate, env)
    if rust is None or ignored is None:
        missing("Rust full-mode", "cargo, which lists the tests of a fixture crate offline")
    else:
        (lists / "rust.txt").write_text(rust, encoding="utf-8")
        (lists / "rust-ignored.txt").write_text(ignored, encoding="utf-8")
        (lists / "empty.txt").write_text("", encoding="utf-8")
        listed, only_ignored = check_docs.rust_listing(rust), check_docs.rust_listing(ignored)
        ok = {"a_listed_test_that_runs", "a_listed_test_that_is_ignored"} <= listed and only_ignored == {"a_listed_test_that_is_ignored", "actual_windows_private_formats_are_present"}
        ok = ok and not {"a_test_that_cfg_any_turns_off", "a_test_in_a_module_file_that_windows_leaves_out"} & listed
        print(f"{'ok  ' if ok else 'FAIL'} cargo --list names every test with the ignored ones, --list --ignored only the ignored ones, and neither a test that a cfg leaves out")
        failures += not ok
        if not ok:
            print(f"     --list: {sorted(listed)}; --list --ignored: {sorted(only_ignored)}")
        rust_lists = ("--rust-list", str(lists / "rust.txt"), "--rust-ignored-list", str(lists / "rust-ignored.txt"))
        for name, expected, files in [
            ("full mode: a claim whose test cargo lists passes", None, claims("| A claim. | README.md | `a_listed_test_that_runs` | None. |")),
            ("full mode: a Rust test that cfg(any()) turns off is not evidence", "docs/CLAIMS.md:5: evidence `a_test_that_cfg_any_turns_off` is neither a test fn nor a proof name", claims("| A claim. | README.md | `a_test_that_cfg_any_turns_off` | None. |")),
            ("full mode: a test in a module file that a cfg leaves out is not evidence", "evidence `a_test_in_a_module_file_that_windows_leaves_out` is neither a test fn nor a proof name", claims("| A claim. | README.md | `a_test_in_a_module_file_that_windows_leaves_out` | None. |")),
            ("full mode: an ignored test is not evidence by itself", "evidence `a_listed_test_that_is_ignored` is an ignored test", claims("| A claim. | README.md | `a_listed_test_that_is_ignored` | None. |")),
            ("full mode: the real-clipboard proof passes when its test is in the ignored listing", None, {**claims("| A claim. | README.md | `native-proof-real-clipboard` | Never ran. |"), WORKFLOW: GOOD_WORKFLOW}),
            ("full mode: a workflow command only in an env value is not a run step", "proof `native-proof-install` needs a run step of job `install-upgrade-uninstall` that holds 'tools/test_install.ps1'", {**claims("| A claim. | README.md | `native-proof-install` | Never ran. |"), **INSTALL, WORKFLOW: ENV_WORKFLOW}),
        ]:
            full_case(name, expected, files, rust_lists)
        full_case(
            "full mode: the real-clipboard proof fails when its test is not in the ignored listing",
            "proof `native-proof-real-clipboard` runs `actual_windows_private_formats_are_present`, which is not an ignored test fn in the cargo listing",
            {**claims("| A claim. | README.md | `native-proof-real-clipboard` | Never ran. |"), WORKFLOW: GOOD_WORKFLOW},
            ("--rust-list", str(lists / "rust.txt"), "--rust-ignored-list", str(lists / "empty.txt")),
        )
        full_case("full mode: a Rust listing that names no test fails", "the Rust listing names no test", claims("| A claim. | README.md | `a_listed_test_that_runs` | None. |"), ("--rust-list", str(lists / "empty.txt"), "--rust-ignored-list", str(lists / "empty.txt")))

        if not (VITEST.is_file() and shutil.which("node")):
            missing("Vitest full-mode", "node and desktop/node_modules (npm ci in desktop)")
        else:
            frontend = {**claims("| A claim. | README.md | `frontend-reveal-timer` | None. |"), ".github/workflows/build.yml": BUILD_WORKFLOW}
            for number, (name, content, expected) in enumerate([
                ("full mode: a frontend proof passes when Vitest lists its test", GOOD_TIMER, None),
                ("full mode: a frontend title only inside a string is not in the Vitest listing", STRING_TIMER, "in the vitest listing"),
                ("full mode: a frontend test in a skipped describe group is not in the Vitest listing", SKIPPED_GROUP_TIMER, "in the vitest listing"),
                ("full mode: a test call in a function that never runs is not in the Vitest listing", NEVER_CALLED_TIMER, "in the vitest listing"),
            ]):
                desktop = lists / f"vitest-{number}" / "desktop"
                (desktop / TIMER).parent.mkdir(parents=True)
                (desktop / TIMER).write_text(content, encoding="utf-8")
                output = listing(command("vitest", VITEST) + ["--globals"], desktop)
                if output is None:
                    print(f"FAIL {name}\n     vitest list failed on the fixture")
                    failures += 1
                    continue
                (lists / f"vitest-{number}.json").write_text(output, encoding="utf-8")
                full_case(name, expected, {**frontend, TIMER: content}, (*rust_lists, "--vitest-list", str(lists / f"vitest-{number}.json")))

    if not (PLAYWRIGHT.is_file() and shutil.which("node")):
        missing("Playwright", "node and desktop/node_modules (npm ci in desktop)")
    else:
        desktop = lists / "playwright" / "desktop"
        (desktop / "e2e").mkdir(parents=True)
        (desktop / "e2e" / "probe.spec.js").write_text(PLAYWRIGHT_SPEC, encoding="utf-8")
        (desktop / "playwright.config.js").write_text('module.exports = { testDir: "./e2e" };\n', encoding="utf-8")
        output = listing(command("playwright", PLAYWRIGHT), desktop, dict(os.environ, NODE_PATH=str(PLAYWRIGHT.parents[2])))
        entries = check_docs.playwright_listing(output) if output else []
        spec = "desktop/e2e/probe.spec.js"
        for name, path, title, expected in [
            ("the Playwright listing holds a runnable test", spec, "a runnable test", True),
            ("a skipped Playwright test does not run", spec, "a skipped test", False),
            ("a fixme Playwright test does not run", spec, "a fixme test", False),
            ("a Playwright test in a skipped group does not run", spec, "a test in a skipped group", False),
            ("a Playwright title only inside a string is not a test", spec, "a title in a string", False),
            ("a Playwright test call in a function that never runs is not a test", spec, "a test in a function that never runs", False),
            ("a Playwright test of another file does not count", "desktop/e2e/other.spec.js", "a runnable test", False),
        ]:
            ok = bool(entries) and check_docs.listed_test(entries, "playwright", path, title) is expected
            print(f"{'ok  ' if ok else 'FAIL'} {name}")
            failures += not ok

run = subprocess.run(CHECK, capture_output=True, text=True)
ok = run.returncode == 0 and check_docs.FAST_NOTE in run.stdout
print(f"{'ok  ' if ok else 'FAIL'} the working tree passes, and fast mode says that it does not prove a test runs")
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
