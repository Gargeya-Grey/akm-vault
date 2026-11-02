# Testing

## Requirements

Windows with Rust 1.89 or later, Node 22.12 or later, Visual Studio C++ Build Tools, the Windows SDK, WebView2 and Microsoft Edge (for the headless UI tests).

## Commands that define done

Run these from the repository root:

```powershell
cargo test --workspace --offline --locked
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo fmt --all -- --check
cargo build --release -p akm-cli --offline --locked
```

Run these in `desktop`:

```powershell
npm ci
npm test
npm run build
npm run test:ui
npm run tauri -- build -- --locked --offline
```

Leave out `--offline` the first time, when dependencies are not yet fetched. While you edit, run only the tests for the files you changed. Run the full set once before you push a change that is ready for review.

The installers go to `target\release\bundle\msi` and `target\release\bundle\nsis`. They are unsigned development builds. Do not install them.

## Real-surface tests

**KeePassXC interchange.** This test uses temporary vaults and fake keys only:

```powershell
python tools/verify_keepassxc.py --akm target/release/akm.exe --keepassxc "C:/path/to/keepassxc-cli.exe"
```

Use the portable KeePassXC 2.8.0.

**Native GUI proof.** This proof drives only its own AKM process, with fake vaults:

```powershell
./tools/verify_windows_app.ps1 -DesktopExe target/release/akm-desktop.exe -CliExe target/release/akm.exe -KeePassXC "C:/path/to/keepassxc-cli.exe"
```

- The proof refuses to start while another AKM process runs or the clipboard has content. `-SkipClipboard` skips the clipboard checks and keeps the user's clipboard. The skipped checks stay unproved.
- `-JournalOnly` and `-JournalExportOnly` run the journal, backup and export flows only. They do not touch the clipboard or minimize the window.
  - Known limit (2026-11-02): the filter step (`Select-FilterOnce`) can fail before AKM is ready, with `Missing select option` or `Unsupported Pattern`. Run the proof again before you suspect AKM. The fix is open: wait for the combo box pattern and its options, and retry the Expand call.
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

## Which checks a change needs

- **Core data or persistence:** core regressions, the CLI security tests, the KeePassXC test, and the GUI flows that use the change. Keep the KDF, cipher, history and attachment checks.
- **Journal:** append, read and failure tests across sessions, plus the CLI and Activity callers. A journal warning after a commit must never look like an unsaved change.
- **Session, clipboard or export:** deterministic interleaving tests, plus real Windows evidence. Mocked browser tests do not prove native clipboard or OS behaviour.
- **IPC shapes shared by Rust and TypeScript:** the TypeScript checks and the real frontend-native boundary.
- **Frontend:** the affected lifecycle and interaction tests, a rebuilt GUI, and the affected native flow.
- **Parser:** a written external dialect, and fixtures from another reader. Round trips through AKM alone are not enough.
- **Documentation only:** check that the links resolve and the documents agree. No build is needed.

## Not tested yet

Real OS lock and suspend delivery, a real 10-minute idle wait, install, upgrade and uninstall, a remote CI run, publisher signing, updater security, broad dotenv compatibility, a full rescore after phase one, and a professional audit.
