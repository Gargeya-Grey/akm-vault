# Current state

Updated 2026-11-02. Replace this file's sections when they change. History lives in Git.

## Status

- **Phase one is complete.** Its review rounds accepted the fixes for the original critical and high findings. H9 (rotation) is still open, and H6 is partial: real Windows lock and suspend delivery is not tested.
- **Phase two, unit 1a, is in progress.** Unit 1a covers warning ownership and honest journal outcomes ([ROADMAP.md](ROADMAP.md)). Its fifth review ([round 5](reviews/2026-11-01-unit1a-review5.md)) rejected it and found 1A-R5-01 to 1A-R5-06. A protocol design then failed two design reviews. On 2026-11-01 the owner split the unit (option A): the session-event and import-review findings (R4-02, 1A-R5-01, 1A-R5-03, 1A-R5-05) moved to the new unit 1c. Unit 1a now has fixes for 1A-R5-02, 1A-R5-04 and 1A-R5-06 on branch `fix/unit1a-sound-findings`. Review round 6 rejected them with 4 new findings (1A-R6-01 to 1A-R6-04). The owner chose one more fix round (option A); review round 7 rejected it with 1A-R7-01 to 1A-R7-04. The owner then chose "fix by class" (option A): one round that closes each class at its root, then one review; if it fails, patching stops again. Round 8 was that round: the CLI output class and the clipboard lease lifecycle were fixed structurally (design note, "Implemented in unit 1a"), with 1A-R7-01 to 1A-R7-04. Review round 8 ([raw](reviews/2026-11-01-unit1a-review8.md)) rejected it: source acceptance NO, work unit complete NO, with 1A-R8-01 to 1A-R8-05. The CLI closed-pipe class and the journal findings were supported; the clipboard lifecycle was not. The owner chose a design-first repair, then split the wider hardening into unit 1h. The repair (`2664e51`) was rejected by its [acceptance review](reviews/2026-11-01-unit1a-acceptance-review.md) with 4 P3 findings (1A-A1-01 to 1A-A1-04); the owner question is under Next action. R4-04 is deferred by the owner ([FINDINGS.md](FINDINGS.md)).
- **Round 5 accepted the source seam of R4-01** and 8 of the round 1 to 3 findings. No finding closes before the unit passes its review.
- **No new score exists.** The only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)).

## Last full check run

This run checked branch `fix/unit1a-sound-findings` at `2664e51` (unit 1a repair, commit 2) on 2026-11-01, in a separate check session. All data was fake.

- Rust: `cargo fmt --all -- --check` and Clippy (`-D warnings`) were clean. `cargo test --workspace --offline --locked` passed 224 tests, with 2 gated tests ignored. The release CLI was rebuilt with `--locked` after the GUI build.
- Frontend: `npm ci` passed. `npm test` passed 192 cases in 16 files. The production build passed. `npm run test:ui` passed 32 mocked-native cases.
- Build: `npm run tauri -- build -- --locked --offline` built the GUI and the unsigned MSI and NSIS installers. None is installed.
- Final-build checks, between two hash runs that matched:
  - Release CLI, fake vaults (`.scratch` scripts, not committed): the earlier script passed 39 of 39; the unit 1a script passed 49 of 49: the default `get` view prints `Secret:      ********` and identical output for secrets of 1, 8, 9 and 40 characters and a non-ASCII secret, with no secret fragment of 3 or more characters and no journal record; every non-secret `--field` prints no secret fragment; `--field secret` and `--reveal` record `reveal-requested` first; `run`, `export` and `--reveal` record `run-requested`, `export-requested`, `export-completed`, `reveal-requested`, the names in the CLI README.
  - Native GUI proofs `-JournalOnly` (27 checks) and `-JournalExportOnly` (9 checks) passed. They used fake vaults, did not touch the clipboard and did not minimize or hide the window. The settings file was byte-identical before and after, and no `akm-desktop` process remained.
- Commit 1 defect tests failed at `3e53e63` and pass at `2664e51`. The 10 design mutants (repair design, section 3.3) each fail a test.
- Not run: KeePassXC interchange (no KDBX format change), the full native GUI proof and the real clipboard test (they use the clipboard and need the owner present), `-OutcomeOnly` (owner gate), and R4-04 (deferred).
- CI: GitHub Actions is off, so no CI ran (`CI RESULT: none`).

## Unit 1a build under test

- Source: branch `fix/unit1a-sound-findings` at `2664e51`.
- Builds in `target/release`, all unsigned and not installed, SHA-256:
  - `akm-desktop.exe` `243b5997bb98d1c537834aa05ee1c3fad2e8b148a7ffdf083968a2dacfc8f48b`
  - `akm.exe` `8cfd85c702ccbc1168844745698a318d618315f78aec0b2053887a57f5c93819`
  - `bundle/msi/AKM_0.1.0_x64_en-US.msi` `d1d6af127d96a915a5611667c5cb2e89a0554a6f34b1637ff9f1b9795880e978`
  - `bundle/nsis/AKM_0.1.0_x64-setup.exe` `5caeb532e8061b9f04ac0a166ae96993cc84f0aa8a897d34720b52d644b1b817`
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
     3. The full check run and the final build with its hashes (above).
     4. The owner-gated Windows evidence: not run. The owner was asleep; the real clipboard test and the full native GUI copy proof need the owner present.
     5. The acceptance review ([raw](reviews/2026-11-01-unit1a-acceptance-review.md), GPT-6.1 Terra max) rejected the unit: **source acceptance NO, work unit complete NO**, with 4 P3 findings, 1A-A1-01 to 1A-A1-04. No P0, P1 or P2. The reviewer's own probe of the release CLI passed the fixed mask and the damaged-journal fail-closed cases. Work stopped here, as the rule says. Nothing is uncommitted.
   - **Owner question (open).** Two findings are wrong texts: a failed copy says "AKM's earlier copy is still on the clipboard" when AKM only holds a lease (1A-A1-01), and "AKM could not remove the partial copy" after the immediate cleanup succeeded (1A-A1-02). Both sentences come from revision 3 itself, so the design, not the code, claimed more than AKM observes. The other two are coverage gaps (1A-A1-03, 1A-A1-04). Options:
     - **A (recommended).** One small round: make those two texts depend on what AKM observed ("may still be on the clipboard"; a separate text when the immediate cleanup succeeded), with tests through `copy_stamped` that fail first; add the tick-and-lock cleanup matrix and the order test. Then one acceptance review, judged by the pass rule below. If it passes with P3 findings, each one moves to the later unit whose scope covers it.
     - **B.** Accept unit 1a with these four P3 findings open, moved to unit 1h, and land PRs #3, #4 and #5. Landing does not close a finding.
     - **C.** Pause unit 1a.
     **Pass rule (owner decision, 2026-11-02).** A review passes when it has no P0, P1 or P2 finding, and its P3 findings move to a named later unit ([REVIEWING.md](REVIEWING.md#pass-rule)). It applies from the next review. The owner gave it for later rounds, so the acceptance review above keeps its result. Option B is a separate owner override of that NO verdict and of the landing condition below. The review record stays NO, and the work-unit verdict stays NO until R4-04 passes.
3. **R4-04** stays deferred until the owner chooses option A or B for it (item 1).

**Pull requests, 2026-11-01.** The owner asked that the open PRs be landed, not left as unfinished work ("Just make sure they are landed and we are not sitting on incomplete work"). This is read as the owner's yes to merge each PR that is complete: reviewed, with its checks passing. A separate session merged:
- PR #1 (docs cleanup) as `d46bcce`. Its review loop stopped after round 2 by the AGENTS.md rule, with the round 2 fixes in `e8f2f18`.
- PR #2 (UI look) as `4656b0c`, after a merge of `main` with no conflicts: 191 unit tests, the build and 40 browser tests passed in its worktree. Its review round 3 confirmed the owner's decisions; the one P3 (ease-back) is the owner's choice. It never touched `target/` of this checkout, so the unit 1a build above is unchanged. The first UI review also found P3-REVEAL-CLOCK, which predates the UI work.
- Deleting PR #1's branch closed PR #3; the session reopened it on base `main`. Before PR #3 lands, retarget PR #4 to `main`, and PR #5 before PR #4 lands.
- PRs #3, #4 and #5 (the R4-04 script, the unit 1a protocol design, unit 1a) land in that order only after the unit 1a acceptance review accepts.

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps all the tracked evidence that the cleanup removed: phase-one records, the review rounds, probes and hash manifests.
- Git tag `baseline/pre-unit-1a` is a commit with no parent. It holds the 139-file source snapshot from before unit 1a, rebuilt from `before.zip`, and each file matches the archived `before-sha256.json`.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the 265 ignored local files that the cleanup moved out: raw logs, screenshots and source archives.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx`. An earlier restart lost the owner's previous setting, so it is unknown. Keep that folder while the app selects it.
- `%TEMP%\opencode\akm-app-proof-030c41dd-649e-44ae-8086-285677373566` holds the diagnostics of the failed R4-04 attempt.
