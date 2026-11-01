# Current state

Updated 2026-11-01. Replace this file's sections when they change. History lives in Git.

## Status

- **Phase one is complete.** Its review rounds accepted the fixes for the original critical and high findings. H9 (rotation) is still open, and H6 is partial: real Windows lock and suspend delivery is not tested.
- **Phase two, unit 1a, is in progress.** Unit 1a covers warning ownership and honest journal outcomes ([ROADMAP.md](ROADMAP.md)). Its fifth review ([round 5](reviews/2026-11-01-unit1a-review5.md)) rejected it and found 1A-R5-01 to 1A-R5-06. A protocol design then failed two design reviews. On 2026-11-01 the owner split the unit (option A): the session-event and import-review findings (R4-02, 1A-R5-01, 1A-R5-03, 1A-R5-05) moved to the new unit 1c. Unit 1a now has fixes for 1A-R5-02, 1A-R5-04 and 1A-R5-06 on branch `fix/unit1a-sound-findings`. Review round 6 rejected them with 4 new findings (1A-R6-01 to 1A-R6-04). The owner chose one more fix round (option A); review round 7 rejected it with 1A-R7-01 to 1A-R7-04. The owner then chose "fix by class" (option A): one round that closes each class at its root, then one review; if it fails, patching stops again. Round 8 was that round: the CLI output class and the clipboard lease lifecycle were fixed structurally (design note, "Implemented in unit 1a"), with 1A-R7-01 to 1A-R7-04. Review round 8 ([raw](reviews/2026-11-01-unit1a-review8.md)) rejected it: source acceptance NO, work unit complete NO, with 1A-R8-01 to 1A-R8-05. The CLI closed-pipe class and the journal findings were supported; the clipboard lifecycle was not. The owner chose a design-first repair (Next action). R4-04 is deferred by the owner ([FINDINGS.md](FINDINGS.md)).
- **Round 5 accepted the source seam of R4-01** and 8 of the round 1 to 3 findings. No finding closes before the unit passes its review.
- **No new score exists.** The only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)).

## Last full check run

This run checked branch `fix/unit1a-sound-findings` with the round 8 fixes (1A-R7-01 to 1A-R7-04) on 2026-11-01. All data was fake.

- Rust: `cargo test --workspace --offline --locked` passed 215 tests, with 2 gated tests ignored. Clippy (`-D warnings`) and `cargo fmt --check` were clean. The release CLI was rebuilt with `--locked` after the last source change. A first test run failed to link (`LNK1104`) because an earlier check run held the same test binary; the run alone passed.
- Frontend: `npm ci` passed. `npm test` passed 190 cases in 15 files. The production build passed. `npm run test:ui` passed 32 mocked-native cases.
- Build: `npm run tauri -- build -- --locked --offline` built the GUI and the unsigned MSI and NSIS installers. None is installed. The installers do not bundle the CLI.
- Final-build checks, between two hash runs that matched:
  - Release CLI, fake vault (`.scratch` script, not committed): 39 of 39 checks passed. The 15 new checks: `add` and `rm` with stdout closed, stderr closed, or both, while every journal append fails, exit 0 and the change persists; `get --field secret` with stdout closed exits 1 (not 101), warns of partial output and does not show the secret.
  - Native GUI proofs `-JournalOnly` (28 checks) and `-JournalExportOnly` (8 checks) passed. They used fake vaults, did not touch the clipboard and did not minimize or hide the window. The settings file was byte-identical before and after, and no `akm-desktop` process remained.
- Round 8 regression tests, each run on the unfixed code first: `committed_changes_exit_zero_with_closed_output_pipes` and `failed_primary_output_is_an_error_not_a_panic` (1A-R7-01) failed on `496aa23`; `the_lease_state_table_holds_for_every_event_from_every_state` and `a_rolled_back_publication_after_an_owned_copy_leaves_no_lease_retry_or_warning` (1A-R7-02) failed with only the wording fix applied. The 1A-R7-03 and 1A-R7-04 tests cannot compile on the old code (changed signature, new seam); mutants stand in for them.
- Mutant checks, each restored after its run: a reinserted `println!` fails clippy; `unwrap` in `status!` fails the CLI class test; the old "It will try again" text, a removed relinquish, and a lease that expires at once each fail at least one lease test.
- Own sibling sweep before the review: no print macro remains in CLI or desktop application code (one `eprintln!` is in the opt-in real-clipboard test); clap help, version and usage errors exit 0 or 2 with closed pipes; no other kept warning promises a later action.
- Not run: KeePassXC interchange (no KDBX format change), the full native GUI proof (it uses the clipboard), the real clipboard test, and R4-04 (deferred).

## Unit 1a build under test

- Source: branch `fix/unit1a-sound-findings`. Its application files differ from `cfeb1b0` only by the unit 1a fixes of rounds 6 to 8, their tests, the `uuid` dependency of `akm-cli` (already in the workspace), and `crates/akm-cli/clippy.toml`.
- Builds in `target/release`, all unsigned and not installed, SHA-256:
  - `akm-desktop.exe` `0efbacf7120b1818860a401daf0e7bbcbeaab4100777b49edd79599450709a0b`
  - `akm.exe` `ea4fc82b0000e465380ceb77e84279c82925981de547a1c3370b947b7483cf8f`
  - `bundle/msi/AKM_0.1.0_x64_en-US.msi` `1aa51ae69fc2d136fd1399c5ceb8fc9d2335fba4b71dd5b9dead25da247f3411`
  - `bundle/nsis/AKM_0.1.0_x64-setup.exe` `35f3d9e3193bdb941e90821d18e3e7a00e1ffc637ecef4b1f5e6876b7401c509`
- The R4-04 pass conditions in FINDINGS.md still name `cfeb1b0`; update them to the accepted source before R4-04 runs.
- Before you use a build for a proof, check its hash against this list. A different hash needs a rebuild and a new check run.

## Next action

The owner authorized implementation of unit 1a only. Each other unit needs its own authorization ([ROADMAP.md](ROADMAP.md)).

1. **R4-04 is deferred** (owner decision, 2026-11-01, option C). The proof script (PR #3) failed review twice ([round 1](reviews/2026-11-01-r4-04-script-review.md), [round 2](reviews/2026-11-01-r4-04-script-review-round2.md)). Polling UI Automation from outside cannot show two things on the pinned build: that no outcome came back just before a dismissal click, and that a delayed reveal reply was delivered and discarded (R4-04-SCRIPT in [FINDINGS.md](FINDINGS.md)). The options that remain for later: **A**, a fake-data-only proof log in the app, which needs a rebuild and must stay off in release builds; **B**, a stronger script that watches UI Automation events, with the two gaps as written limits. R4-04 must pass before the release gate.
2. **Unit 1a: design first, then one acceptance review.** Round 8 ([raw](reviews/2026-11-01-unit1a-review8.md)) rejected the unit with 1A-R8-01 to 1A-R8-05. The owner then asked GPT-6 Astra once for guidance ([advisory](reviews/2026-11-01-unit1a-advisory.md); advice, not a review). **Owner decisions, 2026-11-01:**
   - Design first, in unit 1a. The clipboard lifecycle, its texts, the CLI disclosure policy, caller-level tests and the action-name docs stay in 1a. Units 1b and 1c keep their scope.
   - The CLI default view uses a fixed mask with no secret characters and no length (1A-R8-01). `--reveal` and `--field secret` stay the only disclosures, and they record the request first.
   - Clipboard cleanup is best effort (1A-R8-02). Every clipboard text says so and says that the copy may remain. Lock rule, narrowed after design review 1: lock sends its notice first, then makes one cleanup attempt that never retries and never waits for a busy clipboard; it may wait for a copy in progress (built in unit 1h).

   Steps, in order:
   1. Inventories, from the real code: every exit of `publish` and `clear` and what it did to the clipboard; every CLI output and disclosure sink; every user-facing clipboard text (Rust, TSX, README); a map from each acceptance claim to the caller-level test that proves it.
   2. A design note (section 10 of `docs/design/unit1a-protocol.md`, or a new file) from those inventories: explicit publisher outcomes, the lease invariants, the output-intent type, and the injectable CLI run. One design review.
   3. Tests that fail first through the real callers (the real publisher with the fake Win32 API, through `Session`; the CLI run function), then the implementation, then the text and action-name sweep.
   4. The full check run on the final build, then one acceptance review. If it rejects, stop and ask the owner.
   **Status, 2026-11-01:**
   - Step 1 is done. The repair design ([design](design/unit1a-repair.md)) failed two design reviews ([review 1](reviews/2026-11-01-unit1a-repair-design-review1.md): 10 blocking; [review 2](reviews/2026-11-01-unit1a-repair-design-review2.md): 9 blocking), so the work stopped (AGENTS.md).
   - **Owner decision, 2026-11-01: split.** Unit 1a closes 1A-R8-01 to 1A-R8-05 narrowly, each through its production caller (design revision 3). The wider hardening moves to the new unit 1h (FINDINGS.md, 1H-01 to 1H-08), with its own design and authorization. The owner's narrowed lock rule is built in 1h. There is no third design review: the acceptance review judges 1a against revision 3.
   - Done, from revision 3, section 5:
     1. Commit 1 (`3e53e63`): the test-support move and the failing tests. On the unchanged production code, every defect test failed as stated (the `copy_stamped` table for the four 1A-R8-03 cases and the messages, the changed-sequence cleanup, both frontend text tests, the CLI mask test at the 9-character secret, the README action test on `run-start`).
     2. Commit 2: the fix (publisher outcomes from the `tx` phase, the lease transitions, `clipboardLost`, `KeyMetadata` and the fixed mask, `run_with` with `Io`), the text and action-name sweep (`clipboardText.ts`, both READMEs), and the tests. All 10 design mutants (section 3.3) fail a test. Mutant 2 survived at first; the new table case "rollback fails after the first secret set" catches it. Its expected row keeps the lease, because cleanup cannot read the empty frame formats and retries with the uncertainty warning.
   - Next:
     3. The full check run and the final build with its hashes.
     4. The owner-gated Windows evidence, each with the owner's yes, or reported as not checked.
     5. One acceptance review. If it rejects, stop and ask the owner.
3. **R4-04** stays deferred until the owner chooses option A or B for it (item 1).

**UI work unit (draft PR #2, worktree `W:\akm-worktrees\ui-refine`).** It failed review twice (raw reviews in `docs/reviews/2026-11-01-ui-review.md` and `-ui-review-round2.md` on that branch). The owner then decided, on 2026-11-01: remove the Key Detail timer bar, make colour changes instant (only transform animates), and keep clay destructive buttons as a recorded exception to the house forest rule. Commit `6aa3ee3` applies the decisions. Review round 3 confirmed them and found one P3: press and hover feedback eases back. The owner decided to keep the ease-back (2026-11-01), and `57909a7` records the rule in the CSS comments. Its checks: 191 unit tests, the build, 40 browser tests and the e2e type check passed, and the two changed tests fail on `496205c`. The PR stays a draft until R4-04 and the unit 1a review are done, because a rebuild would change the pinned build. The first review also found P3-REVEAL-CLOCK, which predates the UI work.

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps all the tracked evidence that the cleanup removed: phase-one records, the review rounds, probes and hash manifests.
- Git tag `baseline/pre-unit-1a` is a commit with no parent. It holds the 139-file source snapshot from before unit 1a, rebuilt from `before.zip`, and each file matches the archived `before-sha256.json`.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the 265 ignored local files that the cleanup moved out: raw logs, screenshots and source archives.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx`. An earlier restart lost the owner's previous setting, so it is unknown. Keep that folder while the app selects it.
- `%TEMP%\opencode\akm-app-proof-030c41dd-649e-44ae-8086-285677373566` holds the diagnostics of the failed R4-04 attempt.
