# Current state

Updated 2026-11-01. Replace this file's sections when they change. History lives in Git.

## Status

- **Phase one is complete.** Its review rounds accepted the fixes for the original critical and high findings. H9 (rotation) is still open, and H6 is partial: real Windows lock and suspend delivery is not tested.
- **Phase two, unit 1a, is in progress.** Unit 1a covers warning ownership and honest journal outcomes ([ROADMAP.md](ROADMAP.md)). Its fifth review ([round 5](reviews/2026-11-01-unit1a-review5.md)) rejected it: source acceptance NO, work unit complete NO. It found 1A-R5-01 to 1A-R5-06 and kept R4-02 and R4-03 open. R4-04 is deferred by the owner ([FINDINGS.md](FINDINGS.md)).
- **Round 5 accepted the source seam of R4-01** and 8 of the round 1 to 3 findings. No finding closes before the unit passes its review.
- **No new score exists.** The only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)).

## Last full check run

This run checked the application source that is now on `main`. Later commits changed documentation, Git rules and the R4-04 proof tools (`tools/verify_windows_app.ps1`, `tools/ProofSnapshot.psm1`, `tools/test_proof_snapshot.ps1`). They did not change any build input.

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

1. **R4-04 is deferred** (owner decision, 2026-11-01, option C). The proof script (PR #3) failed review twice ([round 1](reviews/2026-11-01-r4-04-script-review.md), [round 2](reviews/2026-11-01-r4-04-script-review-round2.md)). Polling UI Automation from outside cannot show two things on the pinned build: that no outcome came back just before a dismissal click, and that a delayed reveal reply was delivered and discarded (R4-04-SCRIPT in [FINDINGS.md](FINDINGS.md)). The options that remain for later: **A**, a fake-data-only proof log in the app, which needs a rebuild and must stay off in release builds; **B**, a stronger script that watches UI Automation events, with the two gaps as written limits. R4-04 must pass before the release gate.
2. **Design question for the owner: how does unit 1a reach acceptance?** It has failed review five times, so patching stopped (AGENTS.md). Each round found new defects. Three of the six round-5 findings are ordering gaps between the frontend and the native side: a lock emitted before the listener exists (1A-R5-01), and an import review that the frontend cancels but the native side keeps (1A-R5-03, 1A-R5-05). The others are a clipboard read failure taken as success (1A-R5-02), a missing CLI export completion record (1A-R5-04) and a dropped sequence number (1A-R5-06). The options:
   - **A. One more fix round.** Fix the six findings, each with a regression test that fails first. Then rebuild, run the full checks, pin new hashes, and request one review.
   - **B. Protocol design first.** Write a short design for the frontend-native state protocol: subscribe before the first status read, a session generation on every reply and event, and import reviews bound to a session and cancelled natively. Also define how clipboard uncertainty is handled. A design review checks it. Then fix all six findings against it, and request one unit review.
   - **C. Narrow unit 1a.** Move the clipboard and import-review findings to later units, and review 1a on a smaller contract. This changes the unit plan and leaves those defects open for longer.
3. **R4-04** stays deferred until the owner chooses option A or B for it (item 1).

**UI work unit (draft PR #2, worktree `W:\akm-worktrees\ui-refine`).** It failed review twice (raw reviews in `docs/reviews/2026-11-01-ui-review.md` and `-ui-review-round2.md` on that branch). The owner then decided, on 2026-11-01: remove the Key Detail timer bar, make colour changes instant (only transform animates), and keep clay destructive buttons as a recorded exception to the house forest rule. Commit `6aa3ee3` applies the decisions. Review round 3 confirmed them and found one P3: press and hover feedback eases back. The owner decided to keep the ease-back (2026-11-01), and `57909a7` records the rule in the CSS comments. Its checks: 191 unit tests, the build, 40 browser tests and the e2e type check passed, and the two changed tests fail on `496205c`. The PR stays a draft until R4-04 and the unit 1a review are done, because a rebuild would change the pinned build. The first review also found P3-REVEAL-CLOCK, which predates the UI work.

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps all the tracked evidence that the cleanup removed: phase-one records, the review rounds, probes and hash manifests.
- Git tag `baseline/pre-unit-1a` is a commit with no parent. It holds the 139-file source snapshot from before unit 1a, rebuilt from `before.zip`, and each file matches the archived `before-sha256.json`.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the 265 ignored local files that the cleanup moved out: raw logs, screenshots and source archives.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx`. An earlier restart lost the owner's previous setting, so it is unknown. Keep that folder while the app selects it.
- `%TEMP%\opencode\akm-app-proof-030c41dd-649e-44ae-8086-285677373566` holds the diagnostics of the failed R4-04 attempt.
