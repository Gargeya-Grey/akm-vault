---
name: verify-akm
description: Prove AKM behaviour on its real surfaces (the akm CLI and the akm-desktop Tauri app on Windows) with fake vaults. Use before you call an AKM change done, to reproduce a reported defect, or to collect evidence for a review. It gives the safe launch, doctor, drive, evidence and cleanup steps, and a feature map.
---

# Verify AKM

AKM is a Windows API-key manager: a Rust CLI (`akm.exe`) and a Tauri desktop app (`akm-desktop.exe`) over a KeePass (KDBX) vault. This skill drives both the way a user does, with fake data only. Read `AGENTS.md` first; its Safety and Owner gates sections win over this file.

## Rules that stop a run

- Fake vaults and fake keys only, in folders that the run creates. Never open a real vault.
- Never write the real journal folder `%LOCALAPPDATA%\akm\journal\`. Every CLI run sets `AKM_JOURNAL_ROOT` to a run folder (the helper does). Run `cargo` only from the repository root, where `.cargo/config.toml` sets the test root.
- Never change `%APPDATA%\com.akm.desktop\settings.json`. The GUI proof saves and restores it; check its hash before and after.
- Owner present, and the owner's yes in chat for each attempt: anything that uses the clipboard, minimizes or hides a window, locks or suspends Windows, or shows a native dialog that must be clicked or seen. That covers `-OutcomeOnly` (R4-04), the full GUI proof, and the real clipboard test.
- Never drive an `akm-desktop` process that this run did not start. Never kill a process by name.
- The owner-present proofs above also exist as `.github/workflows/native-proof.yml`, for a disposable GitHub-hosted runner (docs/TESTING.md, "Native proofs on a disposable runner"). Never start that workflow, and never run `tools/test_install.ps1` or `tools/ci_native_preflight.ps1` on a computer: they refuse outside a GitHub-hosted runner, and starting the workflow is the owner's decision.

## Launch

The CLI is short-lived: build once, then run each command on its own.

```powershell
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path (Get-Location) 'target' }
cargo build --release -p akm-cli --offline --locked
cd desktop; npm ci; npm run tauri -- build -- --locked --offline; cd ..
```

A build needs about 10 GB. When the repository's drive has less free space, set `CARGO_TARGET_DIR` to a folder on a drive with more space before the build. Otherwise Cargo uses `target\` in the repository. The doctor warns when the space is short.

Ready: `& "$target\release\akm.exe" --version` prints `akm 0.1.0`. The desktop build is ready when `$target\release\akm-desktop.exe` exists. Do not install the setup program (owner gate). There is nothing to tear down after a build.

The desktop app is launched only by `tools/verify_windows_app.ps1`, which starts its own process with a fake vault, a test journal root and a saved copy of the settings, and closes it at the end.

## Doctor

Run it first, and again whenever something looks wrong:

```powershell
./.agents/skills/verify-akm/scripts/doctor.ps1 -Akm "$target\release\akm.exe" -Desktop "$target\release\akm-desktop.exe"
```

It prints one `OK`, `WARN` or `STOP` line per check: the commit, uncommitted source, the build hashes, a running `akm-desktop`, the settings hash, the real journal folder, `AKM_PASSWORD` in the shell and free disk space. It exits 1 on any `STOP`. Add `-NeedClipboard` before an owner-present clipboard flow; it counts clipboard formats without opening the clipboard.

## Drive

**CLI.** Use the helper. It makes a fake vault folder under `%TEMP%`, sets `AKM_VAULT`, a fake `AKM_PASSWORD` and `AKM_JOURNAL_ROOT`, and appends each command with its stdout, stderr and exit code to the evidence transcript.

```powershell
$h = './.agents/skills/verify-akm/scripts/akm-verify.ps1'
$s = & $h new -Feature vault-and-keys -Akm "$target\release\akm.exe"
& $h cli -Session $s -- init --name 'Fake vault'
& $h cli -Session $s -Stdin 'fake-secret-0123456789' -- add 'Fake OpenAI' --project demo --env dev --env-var OPENAI_API_KEY --secret-stdin
& $h cli -Session $s -- list --json
& $h clean -Session $s
```

`cli` returns an object with `ExitCode`, `Stdout` and `Stderr`. Commands and flags are in `akm.exe --help` and `crates/akm-cli/README.md`. A single-key command takes an id or an exact title.

**Desktop GUI.** Drive it only through `tools/verify_windows_app.ps1`. It uses UI Automation with accessible names (buttons such as `Move to Recycle Bin`, `Review import`, `Refresh`; fields such as `Vault file`, `Master password`). These focused modes need no owner, because they never touch the clipboard and never minimize:

```powershell
./tools/verify_windows_app.ps1 -DesktopExe "$target\release\akm-desktop.exe" -CliExe "$target\release\akm.exe" -JournalOnly
./tools/verify_windows_app.ps1 -DesktopExe "$target\release\akm-desktop.exe" -CliExe "$target\release\akm.exe" -JournalExportOnly
```

Each prints `PASS:` lines and ends with `SKIPPED:` for what it did not cover. Any other mode is in `docs/TESTING.md` and needs the owner.

**Interchange.** `python tools/verify_keepassxc.py --akm <akm.exe> --keepassxc <keepassxc-cli.exe>` (fetch KeePassXC 2.8.0 with `tools/fetch_keepassxc.ps1`: it adds one folder under `AKM_KEEPASSXC_DIR`, else `%LOCALAPPDATA%\akm-tools\keepassxc`, and `keepassxc-cli.exe` is in that new folder) and `node tools/verify_kdbxweb/verify.mjs --akm <akm.exe>` (after `npm ci` in that folder).

## Evidence

- The helper writes `<evidence root>\<UTC time>-<feature>\transcript.txt`, with the build hash on its first line. The evidence root is `-EvidenceRoot`, else the variable `AKM_EVIDENCE_ROOT`, else `%LOCALAPPDATA%\akm-verify\proofs`. The helper's ownership records go to `AKM_VERIFY_STATE_ROOT`, else `%LOCALAPPDATA%\akm-verify\state`. Set both variables to keep the records somewhere else, for example in a folder under the repository's ignored `.scratch\`. Copy GUI proof output there with `*>&1 | Tee-Object -FilePath <that folder>\output.txt`.
- Prove a change through the real path that a user takes: the CLI binary or the GUI proof. A unit test is not evidence for this skill.
- Record the action and the result: the command, its exit code, and a second read-only view of any stored change (for example `list --json` or `history` after `rotate`, `bin` after `rm`, `audit --verify` after a disclosure).
- Check the side effects: no plaintext file beside the vault except backups, no new file in `%LOCALAPPDATA%\akm`, the settings hash unchanged, no fake secret in a transcript where the feature must not print it.

## Cleanup

`akm-verify.ps1 clean -Session <folder>` removes only a folder that it created under `%TEMP%` (its name starts with `akm-verify-`), and keeps the evidence folder. `tools/verify_windows_app.ps1` closes the app that it started, restores the settings and deletes its own working folder after a pass; after a failure it keeps that folder and prints its path. Then run the doctor again: no `akm-desktop` process, the same settings hash.

## Feature map

[features/README.md](features/README.md) lists each user-facing feature with its entry points and its proof. When you add or change a feature, update its file and the index in the same change.
