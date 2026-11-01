# Current state

Updated 2026-11-01. Replace this file's sections when they change. History lives in Git.

## Status

- **Phase one is complete.** Its review rounds accepted the fixes for the original critical and high findings. H9 (rotation) is still open, and H6 is partial: real Windows lock and suspend delivery is not tested.
- **Phase two, unit 1a, is in progress.** Unit 1a covers warning ownership and honest journal outcomes ([ROADMAP.md](ROADMAP.md)). Its fifth review ([round 5](reviews/2026-11-01-unit1a-review5.md)) rejected it and found 1A-R5-01 to 1A-R5-06. A protocol design then failed two design reviews. On 2026-11-01 the owner split the unit (option A): the session-event and import-review findings (R4-02, 1A-R5-01, 1A-R5-03, 1A-R5-05) moved to the new unit 1c. Unit 1a now has fixes for 1A-R5-02, 1A-R5-04 and 1A-R5-06 on branch `fix/unit1a-sound-findings`. Review round 6 rejected them with 4 new findings (1A-R6-01 to 1A-R6-04). The owner chose one more fix round (option A), and those fixes are on the same branch. R4-04 is deferred by the owner ([FINDINGS.md](FINDINGS.md)).
- **Round 5 accepted the source seam of R4-01** and 8 of the round 1 to 3 findings. No finding closes before the unit passes its review.
- **No new score exists.** The only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)).

## Last full check run

This run checked branch `fix/unit1a-sound-findings` with the round 7 fixes (1A-R6-01 to 1A-R6-04) on 2026-11-01. All data was fake.

- Rust: `cargo test --workspace --offline --locked` passed 210 tests, with 2 gated tests ignored. Clippy (`-D warnings`) and `cargo fmt --check` were clean. The release CLI was rebuilt with `--locked` after the last source change.
- Frontend: `npm test` passed 190 cases in 15 files. The production build passed. `npm run test:ui` passed 32 mocked-native cases.
- Build: `npm run tauri -- build -- --locked --offline` built the GUI and the unsigned MSI and NSIS installers. None is installed. The installers do not bundle the CLI.
- Final-build checks, between two hash runs that matched:
  - Release CLI, fake vault (`.scratch` script, not committed): 24 of 24 checks passed. They include the round 5 export-pairing and declared-sequence cases, a refused export that says "declared sequence 7 (not verified)", and a completed 1 MiB stdout export whose completion append failed: exit 0 with stderr closed, and exit 0 with the warning when stderr was open.
  - Native GUI proofs `-JournalOnly` (28 checks) and `-JournalExportOnly` (8 checks) passed. They used fake vaults, did not touch the clipboard and did not minimize or hide the window. The settings file was byte-identical before and after.
- Round 7 regression tests that fail on `30e65b6` and pass now: `an_unconfirmed_completion_keeps_a_successful_exit_even_without_stderr` (1A-R6-01), `a_busy_cleanup_warns_that_the_copy_may_remain_and_retries_in_one_episode` (1A-R6-02), `a_rolled_back_publication_keeps_no_lease_even_when_the_next_open_is_busy` (1A-R6-03), `refused_appends_label_a_damaged_records_sequence_as_declared` (1A-R6-04). One older assertion changed: after a successful rollback the clipboard is opened once, not twice.
- Not run: KeePassXC interchange (no KDBX format change), the full native GUI proof (it uses the clipboard), the real clipboard test, and R4-04 (deferred).

## Unit 1a build under test

- Source: branch `fix/unit1a-sound-findings`. Its application files differ from `cfeb1b0` only by the unit 1a fixes of rounds 6 and 7, their tests, and the `uuid` dependency of `akm-cli` (already in the workspace).
- Builds in `target/release`, all unsigned and not installed, SHA-256:
  - `akm-desktop.exe` `9ab354a814d45761f0772c3b0e90d210d42c25a45e51e962295edd3c3c3bb522`
  - `akm.exe` `051a3b71913ba486c7ca950da06e8b101d2c3de8c7bf13802973d739135a9b1c`
  - `bundle/msi/AKM_0.1.0_x64_en-US.msi` `93c786d45342e03ee00597e3f37647160b65d04ecd3e52b653fe544751fc4727`
  - `bundle/nsis/AKM_0.1.0_x64-setup.exe` `e63ccc9b0275c725a7440683be961da84ee7d727806adfe7272493eb9e6fcabc`
- The R4-04 pass conditions in FINDINGS.md still name `cfeb1b0`; update them to the accepted source before R4-04 runs.
- Before you use a build for a proof, check its hash against this list. A different hash needs a rebuild and a new check run.

## Next action

The owner authorized implementation of unit 1a only. Each other unit needs its own authorization ([ROADMAP.md](ROADMAP.md)).

1. **R4-04 is deferred** (owner decision, 2026-11-01, option C). The proof script (PR #3) failed review twice ([round 1](reviews/2026-11-01-r4-04-script-review.md), [round 2](reviews/2026-11-01-r4-04-script-review-round2.md)). Polling UI Automation from outside cannot show two things on the pinned build: that no outcome came back just before a dismissal click, and that a delayed reveal reply was delivered and discarded (R4-04-SCRIPT in [FINDINGS.md](FINDINGS.md)). The options that remain for later: **A**, a fake-data-only proof log in the app, which needs a rebuild and must stay off in release builds; **B**, a stronger script that watches UI Automation events, with the two gaps as written limits. R4-04 must pass before the release gate.
2. **Unit 1a: one more fix round** (owner decision, 2026-11-01, option A after the sixth rejection). Review round 6 ([raw](reviews/2026-11-01-unit1a-review6.md)) found 1A-R6-01 to 1A-R6-04. Their fixes and regression tests are on `fix/unit1a-sound-findings` (draft PR #5), with the check run above. **Next:** one fresh unit 1a review (round 7). If it fails, patching stops and the result goes to the owner. Unit 1a cannot be complete before R4-04 passes. Unit 1c (session events and import reviews) needs its own authorization.
3. **R4-04** stays deferred until the owner chooses option A or B for it (item 1).

**UI work unit (draft PR #2, worktree `W:\akm-worktrees\ui-refine`).** It failed review twice (raw reviews in `docs/reviews/2026-11-01-ui-review.md` and `-ui-review-round2.md` on that branch). The owner then decided, on 2026-11-01: remove the Key Detail timer bar, make colour changes instant (only transform animates), and keep clay destructive buttons as a recorded exception to the house forest rule. Commit `6aa3ee3` applies the decisions. Review round 3 confirmed them and found one P3: press and hover feedback eases back. The owner decided to keep the ease-back (2026-11-01), and `57909a7` records the rule in the CSS comments. Its checks: 191 unit tests, the build, 40 browser tests and the e2e type check passed, and the two changed tests fail on `496205c`. The PR stays a draft until R4-04 and the unit 1a review are done, because a rebuild would change the pinned build. The first review also found P3-REVEAL-CLOCK, which predates the UI work.

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps all the tracked evidence that the cleanup removed: phase-one records, the review rounds, probes and hash manifests.
- Git tag `baseline/pre-unit-1a` is a commit with no parent. It holds the 139-file source snapshot from before unit 1a, rebuilt from `before.zip`, and each file matches the archived `before-sha256.json`.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the 265 ignored local files that the cleanup moved out: raw logs, screenshots and source archives.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx`. An earlier restart lost the owner's previous setting, so it is unknown. Keep that folder while the app selects it.
- `%TEMP%\opencode\akm-app-proof-030c41dd-649e-44ae-8086-285677373566` holds the diagnostics of the failed R4-04 attempt.
