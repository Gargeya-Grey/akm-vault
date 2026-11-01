# Current state

Updated 2026-11-01. Replace this file's sections when they change. History lives in Git.

## Status

- **Phase one is complete.** Its review rounds accepted the fixes for the original critical and high findings. H9 (rotation) is still open, and H6 is partial: real Windows lock and suspend delivery is not tested.
- **Phase two, unit 1a, is in progress.** Unit 1a covers warning ownership and honest journal outcomes ([ROADMAP.md](ROADMAP.md)). Its fifth review ([round 5](reviews/2026-11-01-unit1a-review5.md)) rejected it and found 1A-R5-01 to 1A-R5-06. A protocol design then failed two design reviews. On 2026-11-01 the owner split the unit (option A): the session-event and import-review findings (R4-02, 1A-R5-01, 1A-R5-03, 1A-R5-05) moved to the new unit 1c. Unit 1a now has fixes for 1A-R5-02, 1A-R5-04 and 1A-R5-06 on branch `fix/unit1a-sound-findings`, not yet reviewed. R4-04 is deferred by the owner ([FINDINGS.md](FINDINGS.md)).
- **Round 5 accepted the source seam of R4-01** and 8 of the round 1 to 3 findings. No finding closes before the unit passes its review.
- **No new score exists.** The only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)).

## Last full check run

This run checked branch `fix/unit1a-sound-findings` (the three unit 1a fixes) on 2026-11-01. All data was fake.

- Rust: `cargo test --workspace --offline --locked` passed 206 tests, with 2 gated tests ignored. Clippy (`-D warnings`) and `cargo fmt --check` were clean. The release CLI built with `--locked`.
- Frontend: `npm ci`, then `npm test` passed 190 cases in 15 files. The production build passed. `npm run test:ui` passed 32 mocked-native cases.
- Build: `npm run tauri -- build -- --locked --offline` built the GUI and the unsigned MSI and NSIS installers. None is installed.
- Final-build checks, between two hash runs that matched:
  - Release CLI, fake vault (`.scratch` script, not committed): 17 of 17 checks passed. Both exports wrote paired `export-requested` and `export-completed` records with one operation ID per export and no destination path. An export to an existing file failed, kept its bytes, and wrote no completion record. A damaged record showed "declared sequence 7 (not verified)" without its contents. A duplicate `seq` showed no sequence.
  - Native GUI proofs `-JournalOnly` and `-JournalExportOnly` passed. They used fake vaults. They did not touch the clipboard and did not minimize or hide the window. The settings file was byte-identical before and after.
- Regression tests that fail on the earlier source and pass now: 4 clipboard tests (1A-R5-02), `exports_pair_requested_and_completed_records_by_operation_without_the_destination` (1A-R5-04), and `a_damaged_record_reports_only_an_unambiguous_declared_sequence`, `audit_shows_a_damaged_records_declared_sequence_without_its_contents` and the Activity notice test (1A-R5-06).
- Not run: KeePassXC interchange (no KDBX format change), the full native GUI proof (it uses the clipboard), the real clipboard test, and R4-04 (deferred).

## Unit 1a build under test

- Source: branch `fix/unit1a-sound-findings`. Its application files differ from `cfeb1b0` only by the three fixes, their tests, and the `uuid` dependency of `akm-cli` (already in the workspace).
- Builds in `target/release`, all unsigned and not installed, SHA-256:
  - `akm-desktop.exe` `3b74690c8fbf8528e73b4c8963bf8abf72dedaa8f3c2eb0f2b0cfcc3dd6df5d9`
  - `akm.exe` `4ca64f5f36219f542f946f5b3467eca9d7a8b203c9df5906fda87ffaff5012b4`
  - `bundle/msi/AKM_0.1.0_x64_en-US.msi` `cbefe79ccab1dcc8f6d6cfd5ec7006d753a955d3d1d5c2ae79caf9709b0d294b`
  - `bundle/nsis/AKM_0.1.0_x64-setup.exe` `86655ec14eb9ec342f6f64983165f10d6fddb7f8653af6fd3d17a70f181bad9c`
- The earlier build of `cfeb1b0` (`akm-desktop.exe` `bc393ed9…6ea8`) is replaced. The R4-04 pass conditions in FINDINGS.md still name `cfeb1b0`; update them to the accepted source before R4-04 runs.
- Before you use a build for a proof, check its hash against this list. A different hash needs a rebuild and a new check run.

## Next action

The owner authorized implementation of unit 1a only. Each other unit needs its own authorization ([ROADMAP.md](ROADMAP.md)).

1. **R4-04 is deferred** (owner decision, 2026-11-01, option C). The proof script (PR #3) failed review twice ([round 1](reviews/2026-11-01-r4-04-script-review.md), [round 2](reviews/2026-11-01-r4-04-script-review-round2.md)). Polling UI Automation from outside cannot show two things on the pinned build: that no outcome came back just before a dismissal click, and that a delayed reveal reply was delivered and discarded (R4-04-SCRIPT in [FINDINGS.md](FINDINGS.md)). The options that remain for later: **A**, a fake-data-only proof log in the app, which needs a rebuild and must stay off in release builds; **B**, a stronger script that watches UI Automation events, with the two gaps as written limits. R4-04 must pass before the release gate.
2. **Unit 1a, split** (owner decision, 2026-11-01, option A). The protocol design ([docs/design/unit1a-protocol.md](design/unit1a-protocol.md), draft PR #4) failed two design reviews ([review 1](reviews/2026-11-01-unit1a-design-review.md), [review 2](reviews/2026-11-01-unit1a-design-review2.md)). Its clipboard, CLI export and journal-sequence sections were sound, and those three fixes are on `fix/unit1a-sound-findings` with the checks above. The session-event and import-review findings are in unit 1c, which needs its own authorization. Finding 1A-D2-09 (an unreported temporary-file cleanup failure) is in unit 3. **Next:** one fresh unit 1a review of that branch and build, with the REVIEWING.md inputs and the design note "Implemented in unit 1a".
3. **R4-04** stays deferred until the owner chooses option A or B for it (item 1).

**UI work unit (draft PR #2, worktree `W:\akm-worktrees\ui-refine`).** It failed review twice (raw reviews in `docs/reviews/2026-11-01-ui-review.md` and `-ui-review-round2.md` on that branch). The owner then decided, on 2026-11-01: remove the Key Detail timer bar, make colour changes instant (only transform animates), and keep clay destructive buttons as a recorded exception to the house forest rule. Commit `6aa3ee3` applies the decisions. Review round 3 confirmed them and found one P3: press and hover feedback eases back. The owner decided to keep the ease-back (2026-11-01), and `57909a7` records the rule in the CSS comments. Its checks: 191 unit tests, the build, 40 browser tests and the e2e type check passed, and the two changed tests fail on `496205c`. The PR stays a draft until R4-04 and the unit 1a review are done, because a rebuild would change the pinned build. The first review also found P3-REVEAL-CLOCK, which predates the UI work.

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps all the tracked evidence that the cleanup removed: phase-one records, the review rounds, probes and hash manifests.
- Git tag `baseline/pre-unit-1a` is a commit with no parent. It holds the 139-file source snapshot from before unit 1a, rebuilt from `before.zip`, and each file matches the archived `before-sha256.json`.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the 265 ignored local files that the cleanup moved out: raw logs, screenshots and source archives.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx`. An earlier restart lost the owner's previous setting, so it is unknown. Keep that folder while the app selects it.
- `%TEMP%\opencode\akm-app-proof-030c41dd-649e-44ae-8086-285677373566` holds the diagnostics of the failed R4-04 attempt.
