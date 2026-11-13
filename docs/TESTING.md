# Testing

## Requirements

Windows with Rust 1.90 or later, Node 22.12 or later, Visual Studio C++ Build Tools, the Windows SDK, WebView2 and Microsoft Edge (for the headless UI tests).

## Commands that define done

Run these from the repository root:

```powershell
cargo test --workspace --offline --locked
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo fmt --all -- --check
cargo build --release -p akm-cli --offline --locked
python tools/check_docs.py
```

`check_docs.py` fails when a tracked Markdown file names a test that is not a `fn` in the Rust sources, or holds a control character. It reads the Rust sources as code, so a declaration in a comment or a string is not a fn. For docs/CLAIMS.md it also resolves each named proof to files that exist, a workflow job and its run command, a frontend test title, or a Rust test (docs/CLAIMS.md, "Named proofs"). A history line that must keep a removed name carries `<!-- check-docs: historical: <reason> -->`. Test the check with `python tools/test_check_docs.py`. Its two real past mistakes are exact copies of the past lines in `tools/fixtures/check_docs_past_mistakes.json`, so the test needs no Git history. When the past commits are in the history, it also checks that the copies are exact; without them it says so.

Run these in `desktop`:

```powershell
npm ci
npm test
npm run build
npm run test:ui
npm run tauri -- build -- --locked --offline
```

Leave out `--offline` the first time, when dependencies are not yet fetched. While you edit, run only the tests for the files you changed. Run the full set once before you push a change that is ready for review.

The setup program goes to `target\release\bundle\nsis`. It is an unsigned development build. Do not install it.

## What CI pins

`.github/workflows/build.yml` runs the checks above on each push to `main` and on each pull request. `.github/workflows/native-proof.yml` runs the native proofs (below). Both pin these inputs:

| Input | Pin | What can still move |
|---|---|---|
| Each action | A full commit SHA, with its version in a comment: `actions/checkout` v7.0.1, `actions/setup-node` v7.1.0, `actions/cache` v6.1.0, `actions/upload-artifact` v7.0.2, `actions/download-artifact` v8.0.2, `Swatinem/rust-cache` v2.9.3, `dtolnay/rust-toolchain` at its `master` commit of 2026-11-13. Each declares Node 24; GitHub removed Node 20 from its runners on 2026-09-23. | Nothing in the action's own code. An action can still fetch other code when it runs, for example the Rust toolchain or Node. |
| Runner image | The label `windows-2025-vs2026`, not `windows-latest`. | GitHub updates the image under a label each week. The log of the step "Set up job" names the image version, and `native-proof.yml` writes it to `build-info.txt`. |
| Rust | 1.90.0, through `dtolnay/rust-toolchain`. | Nothing in the version. rustup fetches it at each run. |
| Node | 22.23.4, through `actions/setup-node`. | Nothing in the version. |
| Dependencies | `Cargo.lock` with `--locked`, and `desktop/package-lock.json` with `npm ci`. | Nothing in the versions. |
| Check tools | `cargo-deny` 0.20.2 and `cargo-audit` 0.22.2, with `--locked`. KeePassXC 2.8.0 by its SHA-256. | The advisory databases and the npm audit data change every day, by design. |
| Python | Not pinned: the image's Python 3.12. The tools need 3.11 or later (`tomllib`). | The patch version, with the image. |
| WebView2 | Not pinned. The native-proof preflight records the version. | With the image, or with Microsoft's current runtime when the preflight installs it. |

## Reproducible builds

AKM does not promise a reproducible build. Two builds of the same commit can differ in bytes. On Windows, the MSVC linker writes a time stamp and the absolute path of the debug file into each `.exe`, and AKM sets no flag that removes them (no `rustflags`, no `[profile]` change, no `/Brepro`). So a user cannot check a published binary by building it again. A user can check a published binary only against its `SHA256SUMS.txt`, and that file only against a signature (see [RELEASING.md](RELEASING.md)).

CI measures the difference without failing. After `Build CLI`, the step "Rebuild the CLI in a second target folder and compare (report only)" builds `akm.exe` again in a new target folder and prints one line:

```text
REBUILD akm.exe first=<sha256> second=<sha256> same=<True|False>
```

The step has not run yet. The expected result is `same=False`, for the two reasons above; that is an inference from how the linker works, not a measurement. A change that makes the build reproducible would set the linker flag `/Brepro`, remap the source paths (`--remap-path-prefix`), and then make this step fail on `same=False`.

## Security checks

These checks add no product code. Run them from the repository root:

```powershell
python tools/check_no_network.py
powershell -NoProfile -ExecutionPolicy Bypass -File tools/check_dependencies.ps1
```

- `check_no_network.py` fails when a networking crate is in the Windows dependency tree, or when the Tauri content security policy, the capabilities, the frontend packages or the sources allow a network path. It cannot prove that AKM makes no connection. Test it with `python tools/test_check_no_network.py`.
- `check_dependencies.ps1` runs `cargo audit`, `cargo deny check` (settings in `deny.toml`) and `npm audit --omit=dev`. It needs network access for the advisory databases. If `cargo-audit` or `cargo-deny` is missing, install them into the ignored scratch folder: `cargo install --locked --root .scratch/cargo-tools cargo-deny cargo-audit`. The script finds them there.
- A warning from `cargo audit` (an unmaintained or unsound crate) does not fail the script. Read each one and record it in [FINDINGS.md](FINDINGS.md) if AKM reaches the code.

## Private data before a public push

`python tools/make_public_snapshot.py --rev <commit> --scan-only` reads every tracked entry of one commit. It never starts Git on the source repository. It copies the source's Git folder with plain file reads (a linked worktree's too) into a temporary folder of its own, leaves out every hook, configuration file, alternate store, replace reference, graft, shallow file and remote, and runs Git only on that copy, with lazy fetching off and every transport refused. A commit whose objects are not all in the source's own object store (a partial clone, an object that only an alternate store holds) is refused, and nothing is fetched. A repository in the reftable format, or with a repository extension that `tools/git_child.py` does not know, is refused. The copy is removed at the end, also on failure. Test this with `python tools/test_snapshot_source.py`. It publishes a file only when it can read all of it: strict UTF-8 text of at most 8 MiB, or an opaque file (an image, a test vault) whose path and SHA-256 are on `tools/public_snapshot/opaque-files.txt`, which the owner writes after checking the file. Any other file is refused as "cannot be scanned" (UTF-16, UTF-32, another encoding, compressed data), and so is an opaque entry that is stale, that names a text file or that names no file of the commit. Each line of each text file, and each path, is checked against five rules: `profile-path` (a user-profile path of this computer), `drive-path` (a drive path of this computer), `email` (an e-mail address, international ones included; only an exact string of `tools/public_snapshot/public-addresses.txt`, written by hand, or of `tools/public_snapshot/notice-addresses.txt`, which `generate_third_party_notices.py` writes and its `--check` keeps current, passes), `identity` (the git user's name and address and the Windows user name) and `extra` (the patterns in `.scratch/public-snapshot-patterns.txt`, which Git ignores, one pattern per line, so the private words are never tracked). No file is exempt by its name. A symbolic link, a submodule, a path that Windows cannot write and two paths that differ only in case refuse the snapshot too. The line marker `public-snapshot: allow <rule ID>` silences only `email`, `profile-path` or `drive-path` on its own line: never `identity` or `extra`, and never a path. A hit prints a locator and the rule, never the matched text. When the path itself holds a hit, the locator is the file's number and a hash of the path. `--suggest-opaque` prints the list lines for the files that the scan cannot read, for the owner to check; it writes nothing. Without `--scan-only`, and only when the scan is clean, it writes the commit's tracked entries into a new folder as a repository with one commit and no history (`--out <new folder> --author "<name> <address>"`). It builds the commit by plumbing from the exact entry list, so an ignore rule cannot drop a tracked file, checks the tree, the index, the work tree and the single parentless commit, and only then renames its unfinished folder to `--out`; on any failure it removes that folder. It never changes the source repository and never pushes. The scan is not a secrets scanner: it does not decode base64, percent-encoding or HTML entities, see an address split over two lines or written in words, or read inside an opaque file (the tool's header lists every limit). Run the scan on the computer whose paths must not leak, because the path rules ask whether a drive or a folder exists there. Test it with `python tools/test_make_public_snapshot.py`.

**The snapshot runs its own checks.** `python tools/test_snapshot_runs_its_checks.py` writes the tracked entries of `HEAD` into a temporary repository with one commit and no history, through the snapshot tool's export without its scan, and runs inside it every required check that needs no build: the docs check and its test, the Git helper test, the snapshot tool test, the no-network check and its test, and the PowerShell tool tests. It fails when one fails, so a check that needs a past commit, an ignored file or the private repository's configuration cannot pass CI. It names each check that it skips (the notices check needs `npm ci`; the builds and the interchange proofs need a build) and the reason. Commit your change first: uncommitted changes are not in the snapshot. CI runs it after the no-network check, because that check reads the fetched crates.

**Git calls (class F).** Every tool in `tools/` and `.agents/` starts Git through `tools/git_child.py` (Python) or `tools/GitChild.psm1` (PowerShell), and `python tools/test_git_child.py` fails when a tool starts Git another way. Every call gets the same settings. The child environment comes from a short allow-list, so no inherited `GIT_` variable (`GIT_DIR`, `GIT_WORK_TREE`, `GIT_INDEX_FILE`, `GIT_CONFIG_*` and the rest) can point Git at another repository or change its configuration, and each call names its repository. `GIT_NO_LAZY_FETCH=1` makes a missing object an error, never a fetch. `GIT_ALLOW_PROTOCOL=/` and `-c protocol.allow=never` refuse every transport, also against a repository's own `protocol.*.allow`. `GIT_NO_REPLACE_OBJECTS=1` ignores replace references. `-c core.fsmonitor=false` and `-c core.hooksPath=` the null device stop a program of the repository's own from starting. A tool that reads a commit reads it from `git_child.source_copy`: a copy of the Git data in a temporary folder, with only the objects, the references and HEAD, and the helper's own configuration. These tools do: the public-snapshot tool, `check_docs.py --rev`, the history comparison of `test_check_docs.py`, and `test_snapshot_runs_its_checks.py`. Two reads stay on the developer's own checkout. `check_docs.py` lists the tracked files of the working tree with `git ls-files`, which reads the index and no object. The verification doctor reads HEAD, the branch and the status of its checkout; the status can run a clean filter that the checkout's own configuration names, on a file that changed. In PowerShell the Git arguments are one array (`Invoke-CleanGit $repo @('status', '--porcelain')`), because PowerShell takes a loose `-p` for its own parameter. A repository that a tool creates sees no global or system configuration and no template, so no hook of the user's runs in it.

**Evidence locations.** Proof evidence stays outside Git, in a folder of the owner's choice. A tracked file names that folder `<archive>`, for example `<archive>\proofs\<UTC time>-r4-04\`, and never its real path. The verification skill writes its transcripts to its evidence root (Real-surface tests, below).

## Real-surface tests

For a manual proof of one feature, use the verification skill in [.agents/skills/verify-akm/](../.agents/skills/verify-akm/SKILL.md). It runs a doctor check first, uses a temporary fake vault and journal, and writes a transcript under the evidence root: `AKM_EVIDENCE_ROOT` when it is set, else `%LOCALAPPDATA%\akm-verify\proofs\`. Its ownership records go to `AKM_VERIFY_STATE_ROOT`, else `%LOCALAPPDATA%\akm-verify\state\`. No default depends on a drive letter.

**KeePassXC interchange.** This test uses temporary vaults and fake keys only:

```powershell
python tools/verify_keepassxc.py --akm target/release/akm.exe --keepassxc "C:/path/to/keepassxc-cli.exe"
```

Since unit 2 it also covers rotation, native history, the Recycle Bin, restore, purge and stale-copy merges. KeePassXC 2.8.0 `merge` applies no `DeletedObjects` record, so the merge check compares an AKM purge with a KeePassXC permanent delete (finding 2-W-01).

Use the portable KeePassXC 2.8.0. `tools/fetch_keepassxc.ps1` downloads the official x64 zip from the KeePassXC GitHub release, checks its SHA-256 against the `.DIGEST` file of that release, and, after that check, adds one new folder `KeePassXC-<version>-x64-<first 12 digest characters>` under the destination: `-Destination`, else `AKM_KEEPASSXC_DIR` when it is set, else `%LOCALAPPDATA%\akm-tools\keepassxc\`. The destination must be outside the repository. It never deletes or replaces anything: a second fetch of the same release changes nothing, and an unmarked folder of that name is refused. Remove old folders by hand. It refuses a drive root and a destination at or below a junction. Test it with `powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_fetch_keepassxc.ps1` (no network):

```powershell
./tools/fetch_keepassxc.ps1
```

**kdbxweb second reader.** This test uses a second, independent KDBX library (kdbxweb with hash-wasm for Argon2) and temporary vaults with fake keys only. It reads an AKM vault (KDF, cipher, version, AKM custom fields), then writes a vault with a foreign group, a foreign entry with history and an attachment, lets AKM change keys, and checks that the compared foreign properties are unchanged: the foreign entry's fields with their protected flags, its tags, its creation and modification times, its attachment bytes and its history, and the foreign group's name, notes, creation time and entries. It does not compare the file bytes, the XML layout, unknown elements or other times (FINDINGS.md FR-24). Part C (unit 2) runs rotate, rm, restore and purge and merges stale copies in both directions; kdbxweb applies `DeletedObjects` on merge. It prints one `PASS` or `FAIL` line per check and exits non-zero on any failure:

```powershell
cd tools/verify_kdbxweb
npm ci
node verify.mjs --akm ../../target/release/akm.exe
```

**Native GUI proof.** This proof drives only its own AKM process, with fake vaults:

```powershell
./tools/verify_windows_app.ps1 -DesktopExe target/release/akm-desktop.exe -CliExe target/release/akm.exe -KeePassXC "C:/path/to/keepassxc-cli.exe"
```

- The proof refuses to start while another AKM process runs or the clipboard has content. `-SkipClipboard` skips the clipboard checks and keeps the user's clipboard. The skipped checks stay unproved.
- `-JournalOnly` and `-JournalExportOnly` run the journal, backup and export flows only. They do not touch the clipboard or minimize the window.
  - Known limit (2026-11-02): the filter step (`Select-FilterOnce`) could fail before AKM was ready, with `Missing select option` or `Unsupported Pattern`. The step now waits, with a bounded deadline, for the combo box, its ExpandCollapse pattern and the option, retries Expand, and re-opens the list when the option is missing. On a final failure it prints `DIAG:` lines (supported patterns, option names, enabled state, and a hint that separates a UI Automation race from an app failure) before it throws. Since 2026-11-08 the field opens AKM's own list, and the step reads the field after each selection and tries again if the value did not stick.
- `-OutcomeOnly -OwnerReadyForVisibility` minimizes and restores the AKM window that the proof owns. Run it only after the owner confirms in chat (AGENTS.md). Run it once per confirmation, and stop if it fails. Its full command and pass conditions are under R4-04 in [FINDINGS.md](FINDINGS.md).
- `-OutcomeRehearsal` runs the outcome-record, cancellation, no-resurrection and Key Detail checks of `-OutcomeOnly` without any minimize. It uses Hide, a delayed reveal and two native Cancels. It needs no owner confirmation, and it never passes R4-04. Run it after any change to the outcome checks.
- `powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_proof_snapshot.ps1` tests the snapshot logic of those checks (`tools/ProofSnapshot.psm1`) without the app. Run it after any change to that module.
- `-RemainingOnly` re-runs only the export, clipboard, synthetic session, exit and capture checks. It does not replace the full proof after a large change.
- Before it launches the app, the proof writes `recovery.json` and the previous settings bytes to its scratch folder. If the proof stops early, keep that folder.
- Before you restore settings by hand, verify the process executable and its creation identity. A PID that still exists is not proof of identity. Also confirm that the current settings still select this proof's fake vault. If ownership is uncertain, or the settings changed for another reason, leave them as they are and tell the owner. Never guess an unknown preference. After a restore, compare the saved bytes exactly with the preimage.

**Real clipboard test.** This test is opt-in. The owner must be present and the clipboard must be empty:

```powershell
$env:AKM_TEST_REAL_CLIPBOARD = '1'
cargo test -p akm-desktop --lib --offline --locked clipboard::windows::actual_windows_private_formats_are_present -- --ignored --exact
```

Remove the variable afterwards.

## Native proofs on a disposable runner

`.github/workflows/native-proof.yml` runs the proofs that need a real desktop, a real clipboard and a real installer on GitHub-hosted Windows runners. GitHub discards each runner after its job, so no proof meets the owner's data. The workflow has a manual start only (`workflow_dispatch`). It has not run yet: it waits for two owner gates, the AGENTS.md rule for disposable runners and GitHub Actions billing.

| Job | What it runs | Evidence (artifact) |
|---|---|---|
| `build` | The setup program, `akm-desktop.exe`, `akm.exe` and the desktop library tests, from the lock files, with no cache. `SHA256SUMS.txt` and `build-info.txt` (commit, run, image, `rustc`, `node`). | `native-proof-build` |
| `real-clipboard` | `clipboard::windows::actual_windows_private_formats_are_present` with `AKM_TEST_REAL_CLIPBOARD=1`, from the built test executable. | `native-proof-real-clipboard-<commit>` |
| `full-gui-proof` | `tools/verify_windows_app.ps1` with no skip switch, and KeePassXC 2.8.0 pinned by its SHA-256. | `native-proof-full-gui-<commit>` |
| `r4-04-attempt` | The R4-04 preconditions (recorded, not enforced), then `-OutcomeOnly -OwnerReadyForVisibility`. | `native-proof-r4-04-<commit>` |
| `install-upgrade-uninstall` | `tools/test_install.ps1`: silent install, the check that the installed `akm-desktop.exe` is the hashed build with only the bundler's bundle-type mark, a start of the installed app through `-JournalOnly`, a second install of the same version, a silent uninstall, and the check that the vault, the settings and the journal stay byte-identical while the program files go. | `native-proof-install-<commit>` |

Each proof job starts on a new runner. It first runs `tools/ci_native_preflight.ps1`, which checks every file against `SHA256SUMS.txt` and records the session ID, the input desktop, the screen size, the clipboard format count and the WebView2 version in `preflight.json`. Each job uploads its evidence also when the proof fails.

**What a run does not prove.**

- **R4-04.** The proof script failed review twice (FINDINGS.md R4-04-SCRIPT). From outside the app, UI Automation polling cannot show that no outcome returned just before a dismissal click, or that a delayed reveal reply was delivered and discarded. The job keeps the evidence, and its precondition record says that the run does not pass R4-04.
- **An upgrade from an older version.** No earlier release exists. The install job installs one version a second time over an install with user data.
- **The "Delete app data" box.** A silent uninstall does not show it. In an interactive uninstall it removes `%APPDATA%\com.akm.desktop` (the settings) and `%LOCALAPPDATA%\com.akm.desktop` (the WebView2 data), from the bundler's template; no test ticks it. The journal is in neither folder.
- **The owner's computer.** A Windows Server runner image is not a Windows 11 desktop with the owner's settings, theme, screen reader or security software.

**Runner facts.** From the [Windows Server 2025 image readme](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-VS2026-Readme.md) (image `20261103.298.1`, read 2026-11-13): Microsoft Edge 156 and Node 22.23.4 are installed. The readme does not list the WebView2 runtime, and no image build script installs it. The readme says nothing about an interactive session or the clipboard. So the preflight measures all three. It stops a job without an interactive `Default` input desktop, or with a non-empty clipboard. Where a job needs WebView2 and it is missing, the preflight installs Microsoft's bootstrapper after it checks the Authenticode signature. The install job does not, because the setup program must install WebView2 itself on a clean computer.

**Guards (class F).** `tools/test_install.ps1` and `tools/ci_native_preflight.ps1` run only when `GITHUB_ACTIONS` is `true`, `RUNNER_ENVIRONMENT` is `github-hosted` and the switch `-DisposableRunner` is passed (`tools/DisposableRunner.psm1`). The install test also refuses when AKM state exists: the install folder `%LOCALAPPDATA%\AKM`, which is the same folder as the journal's parent `%LOCALAPPDATA%\akm` because Windows ignores case, the settings folder, the WebView2 data folder, the uninstall key or a Start menu entry. Every refusal comes before the first change. Neither tool removes anything itself. Test the refusals with `powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_native_proof_guards.ps1`. It runs each tool against a fake profile with sentinel files, installs nothing, and checks that the fake profile is unchanged. `powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_install_binding.ps1` tests the check that binds the installed program to the hashed build (`tools/InstallBinding.psm1`) on fake program files, and that the install test and the workflow use it. It installs nothing.

## Which checks a change needs

- **Core data or persistence:** core regressions, the CLI security tests, the KeePassXC test, and the GUI flows that use the change. Keep the KDF, cipher, history and attachment checks.
- **Journal:** append, read and failure tests across sessions, plus the CLI and Activity callers. A journal warning after a commit must never look like an unsaved change. Every process that a test or a proof script starts gets its own `AKM_JOURNAL_ROOT` (an absolute temporary folder), so no test touches `%LOCALAPPDATA%\akm`. The one exception is the install test (`tools/test_install.ps1`): it runs the installed app on a disposable runner without the variable, so it writes that runner's default journal. `.cargo/config.toml` sets a default for processes that Cargo starts. The tests that use `icacls` and `mklink /J` change only folders that they create. `tools/verify_windows_app.ps1`, `tools/verify_keepassxc.py`, `tools/prepare_gui_fixture.py` and `tools/verify_kdbxweb/verify.mjs` set the variable too; the first one reads the journal through `mapping.json` and `j-<id>.jsonl`.
- **Session, clipboard or export:** deterministic interleaving tests, plus real Windows evidence. Mocked browser tests do not prove native clipboard or OS behaviour.
- **IPC shapes shared by Rust and TypeScript:** the TypeScript checks and the real frontend-native boundary.
- **Frontend:** the affected lifecycle and interaction tests, a rebuilt GUI, and the affected native flow.
- **Parser:** a written external dialect, and fixtures from another reader. Round trips through AKM alone are not enough.
- **Documentation only:** `python tools/check_docs.py`, and check that the links resolve and the documents agree. No build is needed.

## Not tested yet

Real Windows toast delivery of the opt-in notification (unit 4: only a mocked native boundary and fake notifiers ran), real OS lock and suspend delivery, a real 10-minute idle wait, install, upgrade and uninstall (a CI job exists and has not run), publisher signing, updater security, broad dotenv compatibility, a full rescore after phase one, and a professional audit.
