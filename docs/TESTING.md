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

`check_docs.py` fails when a tracked Markdown file names a test that is not a `fn` in the Rust sources, or holds a control character. A history line that must keep a removed name carries `<!-- check-docs: historical: <reason> -->`. Test the check with `python tools/test_check_docs.py`.

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

## Security checks

These checks add no product code. Run them from the repository root:

```powershell
python tools/check_no_network.py
powershell -NoProfile -ExecutionPolicy Bypass -File tools/check_dependencies.ps1
```

- `check_no_network.py` fails when a networking crate is in the Windows dependency tree, or when the Tauri content security policy, the capabilities, the frontend packages or the sources allow a network path. It cannot prove that AKM makes no connection. Test it with `python tools/test_check_no_network.py`.
- `check_dependencies.ps1` runs `cargo audit`, `cargo deny check` (settings in `deny.toml`) and `npm audit --omit=dev`. It needs network access for the advisory databases. If `cargo-audit` or `cargo-deny` is missing, install them into the ignored scratch folder: `cargo install --locked --root .scratch/cargo-tools cargo-deny cargo-audit`. The script finds them there.
- A warning from `cargo audit` (an unmaintained or unsound crate) does not fail the script. Read each one and record it in [FINDINGS.md](FINDINGS.md) if AKM reaches the code.

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

**kdbxweb second reader.** This test uses a second, independent KDBX library (kdbxweb with hash-wasm for Argon2) and temporary vaults with fake keys only. It reads an AKM vault (KDF, cipher, version, AKM custom fields), then writes a vault with a foreign group, a foreign entry with history and an attachment, lets AKM change keys, and checks that the foreign data is identical. Part C (unit 2) runs rotate, rm, restore and purge and merges stale copies in both directions; kdbxweb applies `DeletedObjects` on merge. It prints one `PASS` or `FAIL` line per check and exits non-zero on any failure:

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
| `install-upgrade-uninstall` | `tools/test_install.ps1`: silent install, a start of the installed app through `-JournalOnly`, a second install of the same version, a silent uninstall, and the check that the vault, the settings and the journal stay byte-identical while the program files go. | `native-proof-install-<commit>` |

Each proof job starts on a new runner. It first runs `tools/ci_native_preflight.ps1`, which checks every file against `SHA256SUMS.txt` and records the session ID, the input desktop, the screen size, the clipboard format count and the WebView2 version in `preflight.json`. Each job uploads its evidence also when the proof fails.

**What a run does not prove.**

- **R4-04.** The proof script failed review twice (FINDINGS.md R4-04-SCRIPT). From outside the app, UI Automation polling cannot show that no outcome returned just before a dismissal click, or that a delayed reveal reply was delivered and discarded. The job keeps the evidence, and its precondition record says that the run does not pass R4-04.
- **An upgrade from an older version.** The install job installs one version twice.
- **The owner's computer.** A Windows Server runner image is not a Windows 11 desktop with the owner's settings, theme, screen reader or security software.

**Runner facts.** From the [Windows Server 2025 image readme](https://github.com/actions/runner-images/blob/main/images/windows/Windows2025-VS2026-Readme.md) (image `20261103.298.1`, read 2026-11-13): Microsoft Edge 156 and Node 22.23.4 are installed. The readme does not list the WebView2 runtime, and no image build script installs it. The readme says nothing about an interactive session or the clipboard. So the preflight measures all three. It stops a job without an interactive `Default` input desktop, or with a non-empty clipboard. Where a job needs WebView2 and it is missing, the preflight installs Microsoft's bootstrapper after it checks the Authenticode signature. The install job does not, because the setup program must install WebView2 itself on a clean computer.

**Guards (class F).** `tools/test_install.ps1` and `tools/ci_native_preflight.ps1` run only when `GITHUB_ACTIONS` is `true`, `RUNNER_ENVIRONMENT` is `github-hosted` and the switch `-DisposableRunner` is passed (`tools/DisposableRunner.psm1`). The install test also refuses when AKM state exists: the install folder `%LOCALAPPDATA%\AKM`, which is the same folder as the journal's parent `%LOCALAPPDATA%\akm` because Windows ignores case, the settings folder, the WebView2 data folder, the uninstall key or a Start menu entry. Every refusal comes before the first change. Neither tool removes anything itself. Test the refusals with `powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_native_proof_guards.ps1`. It runs each tool against a fake profile with sentinel files, installs nothing, and checks that the fake profile is unchanged.

## Which checks a change needs

- **Core data or persistence:** core regressions, the CLI security tests, the KeePassXC test, and the GUI flows that use the change. Keep the KDF, cipher, history and attachment checks.
- **Journal:** append, read and failure tests across sessions, plus the CLI and Activity callers. A journal warning after a commit must never look like an unsaved change. Every process that a test or a proof script starts gets its own `AKM_JOURNAL_ROOT` (an absolute temporary folder), so no test touches `%LOCALAPPDATA%\akm`. `.cargo/config.toml` sets a default for processes that Cargo starts. The tests that use `icacls` and `mklink /J` change only folders that they create. `tools/verify_windows_app.ps1`, `tools/verify_keepassxc.py`, `tools/prepare_gui_fixture.py` and `tools/verify_kdbxweb/verify.mjs` set the variable too; the first one reads the journal through `mapping.json` and `j-<id>.jsonl`.
- **Session, clipboard or export:** deterministic interleaving tests, plus real Windows evidence. Mocked browser tests do not prove native clipboard or OS behaviour.
- **IPC shapes shared by Rust and TypeScript:** the TypeScript checks and the real frontend-native boundary.
- **Frontend:** the affected lifecycle and interaction tests, a rebuilt GUI, and the affected native flow.
- **Parser:** a written external dialect, and fixtures from another reader. Round trips through AKM alone are not enough.
- **Documentation only:** `python tools/check_docs.py`, and check that the links resolve and the documents agree. No build is needed.

## Not tested yet

Real Windows toast delivery of the opt-in notification (unit 4: only a mocked native boundary and fake notifiers ran), real OS lock and suspend delivery, a real 10-minute idle wait, install, upgrade and uninstall (a CI job exists and has not run), publisher signing, updater security, broad dotenv compatibility, a full rescore after phase one, and a professional audit.
