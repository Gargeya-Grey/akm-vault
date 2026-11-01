# Current state

Updated 2026-11-01. Replace this file's sections when they change. History lives in Git.

## Status

- **Phase one is complete.** Its review rounds accepted the fixes for the original critical and high findings. H9 (rotation) is still open, and H6 is partial: real Windows lock and suspend delivery is not tested.
- **Phase two, unit 1a, is in progress.** Unit 1a covers warning ownership and honest journal outcomes ([ROADMAP.md](ROADMAP.md)). Its fourth review rejected it, with findings R4-01, R4-02 and R4-03 and one missing proof, R4-04 ([FINDINGS.md](FINDINGS.md)).
- **The fixes for R4-01, R4-02 and R4-03 are in the source on `main`.** They have regression tests. No review has checked them yet.
- **No new score exists.** The only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)).

## Last full check run

This run checked the application source that is now on `main`. Later commits changed only documentation and Git rules.

- Rust: 196 tests passed, and 2 gated tests were ignored. 9 generated IPC tests passed separately. Clippy (`-D warnings`) and rustfmt were clean.
- Frontend: 190 cases passed, in two runs. The full run passed 181 cases and then stopped on a Vitest worker start-up timeout. The 9 Settings cases that did not run then passed on their own.
- Browser: 32 mocked-native cases passed. The production and e2e type checks passed.
- Builds: the GUI and the unsigned MSI and NSIS installers built. None is installed.
- Native, with fake data: the exact-GUI journal and export proofs passed (`-JournalOnly`, `-JournalExportOnly`). Both needed the harness's UI Automation root refresh at start-up. Settings were restored.
- KeePassXC interchange: the last pass was on the same CLI binary. It was not run again.

## Unit 1a build under test

- Source: the application files on `main` at `cfeb1b0`, unchanged since. `git diff baseline/pre-unit-1a main -- crates desktop tools .github Cargo.toml Cargo.lock` compares them with the 139-file pre-unit snapshot: 30 changed files, and 18 files absent from the snapshot. An absent file is not necessarily new in unit 1a. For example, `desktop/icon.svg` existed before phase one with its current hash ([REVIEWING.md](REVIEWING.md#unit-1a-inputs)).
- Builds in `target/release`, all unsigned and not installed, SHA-256:
  - `akm-desktop.exe` `bc393ed9a5081aac2d30e4999e1537bb806c047e9a22bd025e06d2b78d3e6ea8`
  - `akm.exe` `495d72976b552746ebe170f1ab1f25280272b54bff5365b251d3cf546342011b`
  - `bundle/msi/AKM_0.1.0_x64_en-US.msi` `e29139eadb113da5d998a173ea05d7ddf10ae29f8d1bcb72798f3a7e61b6f3ac`
  - `bundle/nsis/AKM_0.1.0_x64-setup.exe` `b0455b8912350d6d1c98f271364257f47e7f6340b19e2ed6a80cbfb3c2c2f7cb`
- Before you use a build for a proof, check its hash against this list. A different hash needs a rebuild and a new check run.

## Next action

The owner authorized implementation of unit 1a only. Each other unit needs its own authorization ([ROADMAP.md](ROADMAP.md)).

1. **Owner decision: the R4-04 proof script.** The script cannot prove all five R4-04 pass conditions ([FINDINGS.md](FINDINGS.md#r4-04-pass-conditions)). The owner chooses one option:
   - Extend `tools/verify_windows_app.ps1` first. It must count the outcome records, include the cancellation in the no-resurrection check, and check Key Detail if that is in scope. The extension changes no application code, so the existing GUI build and its hashes stay valid. Test the script change in a mode that does not minimize (`-JournalOnly`) before the owner-present attempt.
   - Narrow the pass conditions to what the script proves today, and record the limit.
2. **R4-04 needs the owner present.** After that decision, ask the owner whether they are ready to watch. The last attempt minimized the window and then failed its import-dialog-expiry check. The cause was not proved. Since then, a correction moved view expiry into native code, apart from lock handling. That change targets the failed check, but no window run has tested it.
3. **Then review unit 1a.** Prepare the review inputs now ([REVIEWING.md](REVIEWING.md#unit-1a-inputs)). Request the one fresh review only after R4-04 passes, or after the owner gives an explicit decision about a failed or deferred attempt. The review gives two separate verdicts: source acceptance, and work-unit completion.
4. Unit 1a has failed review four times. If this review rejects it again, apply the AGENTS.md rule: stop and take the design question to the owner.

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps all the tracked evidence that the cleanup removed: phase-one records, the review rounds, probes and hash manifests.
- Git tag `baseline/pre-unit-1a` is a commit with no parent. It holds the 139-file source snapshot from before unit 1a, rebuilt from `before.zip`, and each file matches the archived `before-sha256.json`.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the 265 ignored local files that the cleanup moved out: raw logs, screenshots and source archives.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx`. An earlier restart lost the owner's previous setting, so it is unknown. Keep that folder while the app selects it.
- `%TEMP%\opencode\akm-app-proof-030c41dd-649e-44ae-8086-285677373566` holds the diagnostics of the failed R4-04 attempt.
