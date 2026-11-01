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

## Next action

1. **Review unit 1a.** Run one fresh review of the whole of unit 1a on current `main`, as [REVIEWING.md](REVIEWING.md) describes. This review does not need the window proof.
2. **R4-04 needs the owner.** It is the owner-present proof `tools/verify_windows_app.ps1 -OutcomeOnly -OwnerReadyForVisibility` ([TESTING.md](TESTING.md)). Ask the owner whether they are ready to watch. The last attempt minimized the window and then failed its import-dialog-expiry check. The cause was not proved. Since then, a correction moved view expiry into native code, apart from lock handling. That change targets the failed check, but no window run has tested it.
3. Unit 1a has failed review four times. If this review rejects it again, apply the AGENTS.md rule: stop and take the design question to the owner.

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps all the tracked evidence that the cleanup removed: phase-one records, the review rounds, probes and hash manifests.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the 265 ignored local files that the cleanup moved out: raw logs, screenshots and source archives.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx`. An earlier restart lost the owner's previous setting, so it is unknown. Keep that folder while the app selects it.
- `%TEMP%\opencode\akm-app-proof-030c41dd-649e-44ae-8086-285677373566` holds the diagnostics of the failed R4-04 attempt.
