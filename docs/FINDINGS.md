# Findings

Updated 2026-11-01. This file lists every review finding that is not closed, and the work unit that owns it. The [baseline review](reviews/2026-10-30-baseline.md) is the only scored review. New raw reviews are committed in [reviews/](reviews/). The earlier reports are in Git tag `archive/pre-cleanup-2026-11-01`, under `docs/history/reviews/` (phase one), `docs/reviews/` (calibration) and `docs/work/phase2-1a/reviews/` (unit 1a rounds 1 to 4). The standalone verdict of the first phase-one correction review was never found. `PHASE1.md` in that tag records its outcome.

## Status words

- **Fixed, not accepted:** the source has a fix and a regression test, and no review has accepted it.
- **Open:** the problem is still in the source, or a design decision must come first.
- **Partial:** part of the problem is fixed, and the remainder is listed.
- **Gap:** a proof is missing. A gap is not a defect.

## Unit 1a: warning ownership and honest journal outcomes

| ID | Severity | Status | Problem and the test that closes it |
|---|---|---|---|
| R4-01 | P2 | Fixed, not accepted ([round 5](reviews/2026-11-01-unit1a-review5.md) accepted the seam) | Backup pruning could delete the dated backup that was just made, when 5 older backups had names that sort later because of clock skew. The app still said a backup was kept. Test: 5 later-sorting backups and a blocked `.bak` refresh. The exact pre-delete vault must stay recoverable, the delete must still succeed, and the warning must match the actual result. |
| R4-03 | P3 | Partial ([round 8](reviews/2026-11-01-unit1a-review8.md): the wider lifecycle has 1A-R8-02, 1A-R8-03) | When a clipboard copy partly failed and the immediate cleanup also failed, no app warning was recorded. Test: a fake backend with a partial copy and a failed immediate clear, then a recovery, then a second episode while the first warning is still pending. Each episode must get its own warning ID. |
| R4-04 | Proof gate | Gap, deferred | No real-window test has shown that a delayed result survives hide and navigation. See the pass conditions below. **Owner decision, 2026-11-01:** R4-04 is deferred. Unit 1a gets its review without it. The source verdict can pass, and the work-unit verdict stays NO until R4-04 passes. R4-04 must pass before the release gate. |
| R4-04-SCRIPT | P1 | Open (design) | The proof script on PR #3 failed review twice ([round 1](reviews/2026-11-01-r4-04-script-review.md), [round 2](reviews/2026-11-01-r4-04-script-review-round2.md)). From outside the app, UI Automation polling cannot show that no outcome returned just before a dismissal click, or that a delayed reveal reply was delivered and discarded. Before R4-04 is attempted, decide how to observe these (STATE.md options A and B), then fix the 8 direct round-2 findings. |
| P2-WARN-OWNERSHIP | P2 | Fixed, not accepted ([round 5](reviews/2026-11-01-unit1a-review5.md): fixed with limits) | A post-commit journal warning from vault session A could appear as a warning for session B. Rounds 1 to 4 tested this area. |
| P2-JOURNAL-CLAIMS | P2 | Fixed, not accepted ([round 5](reviews/2026-11-01-unit1a-review5.md): fixed with limits; see 1A-R5-04 and 1A-R5-06) | The journal UI claimed "verified" and "complete" when it could not prove that. Missing, damaged, consistent-but-unauthenticated and incomplete logs now show different states. |
| 1A-R5-02 | P2 | Partial ([round 8](reviews/2026-11-01-unit1a-review8.md): named paths supported; the clipboard class has 1A-R8-02, 1A-R8-03) | [round 5](reviews/2026-11-01-unit1a-review5.md), `desktop/src-tauri/src/clipboard.rs:384`, `desktop/src-tauri/src/session.rs:782`. When the clipboard sequence is unchanged but its text cannot be read, the lease is dropped as if the user had copied something newer, and no warning is recorded. Reproduced with a fake backend. Test: an unreadable same-sequence clipboard keeps a safe retry and a warning, and a newer identical user copy is kept. |
| 1A-R5-04 | P2 | Partial ([round 8](reviews/2026-11-01-unit1a-review8.md): pairing implemented; caller coverage has 1A-R8-04) | [round 5](reviews/2026-11-01-unit1a-review5.md), `crates/akm-cli/src/main.rs:377`. A successful CLI file export records `export-requested` but no `export-completed`. Reproduced with the release CLI and fake data. Test: a completion record after publication, and a non-retryable warning when that record cannot be written. |
| 1A-R5-06 | P3 | Fixed, not accepted ([round 8](reviews/2026-11-01-unit1a-review8.md): supported by parser and caller inspection) | [round 5](reviews/2026-11-01-unit1a-review5.md), `crates/akm-core/src/audit.rs:200`. When one field of a record is invalid, its declared sequence number is dropped too (this keeps 1A-R1-07 open). Reproduced with the release CLI. Test: show an available declared sequence, marked untrusted and without the record contents; show none when it is absent or ambiguous. |
| 1A-R6-01 | P2 | Fixed, not accepted ([round 8](reviews/2026-11-01-unit1a-review8.md): the named export warning path is supported) | [round 6](reviews/2026-11-01-unit1a-review6.md), `crates/akm-cli/src/main.rs:393`. After a completed export, a failed completion append prints its warning with `eprintln!`. With stderr closed, that panics and the CLI exits 101, so a script can repeat a disclosure that happened. Reproduced with the release CLI and fake data. Test: completion append and stderr write both fail after publication; the exit stays 0. Keep the writable-stderr warning case. |
| 1A-R6-02 | P2 | Partial ([round 8](reviews/2026-11-01-unit1a-review8.md): kept warning corrected; copy-success text has 1A-R8-02) | [round 6](reviews/2026-11-01-unit1a-review6.md), `desktop/src-tauri/src/clipboard.rs:542`, `desktop/src-tauri/src/session.rs:798`. A busy clipboard at cleanup gives the warning "The clipboard is busy. Try copying again", which does not say that cleanup is unconfirmed or that the copy may remain. Reproduced with the fake Win32 backend at normal expiry and after a partial publication with a failed rollback. Test, at Session level: a cleanup-specific warning, safe retries, one ID per episode, and a newer identical user copy kept. |
| 1A-R6-03 | P3 | Partial ([round 8](reviews/2026-11-01-unit1a-review8.md): named case passes; early setup errors have 1A-R8-03) | [round 6](reviews/2026-11-01-unit1a-review6.md), `desktop/src-tauri/src/clipboard.rs:99`. A failed publication that rolled back still gets a lease and an immediate second clear. If that open is busy, the empty clipboard keeps a lease and a cleanup warning. Reproduced with the fake backend. Test: zero and nonzero captured sequences, rollback succeeds, the next open fails; only the publication error returns, with no lease or warning. |
| 1A-R6-04 | P3 | Fixed, not accepted ([round 8](reviews/2026-11-01-unit1a-review8.md): supported in the append-error path) | [round 6](reviews/2026-11-01-unit1a-review6.md), `crates/akm-core/src/audit.rs:272`. An append refused for a damaged journal says "sequence 7", while `akm audit --verify` says "declared sequence 7 (not verified)". The export still fails closed. Reproduced with the release CLI. Test: the append-error callers show the declared, unverified qualifier without record contents. |
| 1A-R7-01 | P2 | Fixed by class, not accepted ([round 8](reviews/2026-11-01-unit1a-review8.md): the closed-pipe regressions distinguish the affected CLI; the disclosure class has 1A-R8-01) | [round 7](reviews/2026-11-01-unit1a-review7.md), `crates/akm-cli/src/main.rs:493`. After a committed CLI mutation, the warnings use `eprintln!` and the success messages use `println!`. With stderr or stdout closed, the CLI panics and exits 101 although the change was saved, so automation can repeat it (contract 12). Reproduced with the release CLI and fake data. Test: committed mutations with a failed journal append and closed stderr, and with closed stdout; the exit stays successful and non-retryable. Round 8 fix: Every CLI print now goes to an explicit writer with `?` or to the best-effort `status!`/`warning!` macros; `crates/akm-cli/clippy.toml` bans the std print macros. Tests: `committed_changes_exit_zero_with_closed_output_pipes` (init, add, rm, upgrade; three pipe cases), `failed_primary_output_is_an_error_not_a_panic`. Mutants: a reinserted `println!` fails clippy; `unwrap` in `status!` fails the class test. |
| 1A-R7-02 | P3 | Partial ([round 8](reviews/2026-11-01-unit1a-review8.md): the rolled-back case is fixed; the wider error class has 1A-R8-03) | [round 7](reviews/2026-11-01-unit1a-review7.md), `desktop/src-tauri/src/clipboard.rs:99`. When copy A owns a lease and copy B fails and rolls back, A's lease stays although the clipboard is empty; a later busy cleanup warns. Reproduced with the fake backend. Test, at Session level: an owned lease, then a rolled-back publication; no lease, retry or new warning remains. Round 8 fix: A rolled-back failure now ends any earlier lease. Tests: the lease state table in `clipboard.rs` (every event from no lease and from an owned lease) and `a_rolled_back_publication_after_an_owned_copy_leaves_no_lease_retry_or_warning`; both failed before the fix. |
| 1A-R7-03 | P3 | Partial ([round 8](reviews/2026-11-01-unit1a-review8.md): the kept warning no longer promises retries; copy-success text has 1A-R8-02) | [round 7](reviews/2026-11-01-unit1a-review7.md), `desktop/src-tauri/src/clipboard.rs:17`, `desktop/src-tauri/src/session.rs:798`. The kept cleanup warning says "It will try again" after the retries have ended (cleared, or released to a newer copy). Test: both endings; the warning keeps its history and states the retry status truthfully. Round 8 fix: One history-only text, with no promise of a later attempt. Tests: the state table checks the text of every reported cleanup; the busy-cleanup session test ends the episode both ways (AKM's clear, a newer identical copy) and checks the kept warning. Mutant: the old "It will try again" text fails both. |
| 1A-R7-04 | P3 | Partial ([round 8](reviews/2026-11-01-unit1a-review8.md): helper-level test only; see 1A-R8-04) | [round 7](reviews/2026-11-01-unit1a-review7.md), `crates/akm-cli/src/main.rs:796`. No CLI test covers a completion append that writes its line and then fails at sync. Test: inject that failure; the line may remain, the warning shows, and the exit is 0. Round 8 fix: Seam `record_completed_with`; test `a_completion_record_written_before_a_failed_sync_is_still_a_warning`. The seam did not exist before, so this test cannot run on the old version. |
| 1A-R8-01 | P2 | Open (owner decision: mask policy) | [round 8](reviews/2026-11-01-unit1a-review8.md), `crates/akm-cli/src/main.rs:341`, `crates/akm-cli/src/main.rs:774`. Default `akm get` prints a mask with the first and last 4 characters of the secret, without a journal record; with a damaged journal it still prints them (8 of 9 characters for a 9-character fake key). A full reveal fails closed. The desktop app shows a fixed mask with no secret characters. Reproduced with fake data. Test: a damaged journal with short and long fake keys; no secret-derived bytes are printed, or the masked view is refused first. A deliberate exemption needs a contract 12 decision. |
| 1A-R8-02 | P2 | Open | [round 8](reviews/2026-11-01-unit1a-review8.md), `desktop/src/components/KeyDetail.tsx:102`, `desktop/src/components/Settings.tsx:199`, `desktop/src-tauri/src/clipboard.rs:171`. The copy-success messages say AKM clears the copy on lock and on normal exit. With a busy clipboard at expiry, lock and exit, the fake value stays, and `Drop` ignores the last error. Reproduced with a fake backend. Test: busy expiry, lock and exit through a fake Session; the text describes cleanup as an attempt and says the copy may remain, with no promise of a warning after exit. |
| 1A-R8-03 | P3 | Open | [round 8](reviews/2026-11-01-unit1a-review8.md), `desktop/src-tauri/src/clipboard.rs:97`, `desktop/src-tauri/src/clipboard.rs:372`. The lease assumes that a publisher `Err` changed nothing. The publisher empties the clipboard first, then can return `Err` when an exclusion format or an empty text format fails, so copy A is gone but its lease stays and a later busy cleanup warns. Reproduced with the real publisher and a fake Win32 backend. Test: through Session with an existing lease, fail each post-empty step; no stale lease, later cleanup call or new warning. Keep the preparation-failure case in which A stays owned. |
| 1A-R8-04 | P3 | Open (coverage) | [round 8](reviews/2026-11-01-unit1a-review8.md), `crates/akm-cli/src/main.rs:891`. The after-write completion test calls `record_completed_with` directly. No test shows that the CLI prints the warning and exits 0 when the completion line was written and its confirmation failed. Test: inject that failure through the CLI's export path; assert the kept line, the warning and exit 0, and fail a mutant that drops the warning or makes the outcome retryable. |
| 1A-R8-05 | P3 | Open (doc) | [round 8](reviews/2026-11-01-unit1a-review8.md), `crates/akm-cli/README.md:29`. The CLI README says a run records `run-start`; the CLI records `run-requested`. Found by inspection. Check: run a fake child and compare the journal action with the README. |

### R4-04 pass conditions

Run it only after the owner confirms in chat that they are ready to watch. Each confirmation allows one attempt.

**Before the launch:**

1. Check that the build inputs equal the build's source, and that the proof script is the reviewed one. The proof script is the only tool file that changed after `cfeb1b0`:
   - `git diff --quiet cfeb1b0 -- crates desktop Cargo.toml Cargo.lock tools ':(exclude)tools/verify_windows_app.ps1' ':(exclude)tools/ProofSnapshot.psm1' ':(exclude)tools/test_proof_snapshot.ps1'` must exit 0.
   - `git diff --quiet <reviewed proof commit> -- tools/verify_windows_app.ps1 tools/ProofSnapshot.psm1 tools/test_proof_snapshot.ps1` must exit 0. Record the commit and the SHA-256 of these three files in the proof folder.
   - `git status --porcelain -- crates desktop tools` must print nothing.
   - `powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_proof_snapshot.ps1` must print `all cases passed`.
2. Check all four build hashes against [STATE.md](STATE.md#unit-1a-build-under-test) with `Get-FileHash -Algorithm SHA256`. Any difference stops the attempt.
3. Create a new folder `W:\AKM-archive\proofs\<UTC time>-r4-04\` that did not exist before. Record the hashes there. The script deletes its own working folder after a pass, so this folder is the record.

**The launch,** with the full output copied to that folder:

```powershell
./tools/verify_windows_app.ps1 -DesktopExe target/release/akm-desktop.exe -CliExe target/release/akm.exe -OutcomeOnly -OwnerReadyForVisibility *>&1 | Tee-Object -FilePath W:\AKM-archive\proofs\<UTC time>-r4-04\output.txt
```

**The pass conditions.** R4-04 passes only if all of these hold, with fake data:

1. While the proof's own fake-journal lock delays a fake import, the owned AKM window really minimizes (`IsIconic`) and restores.
2. The import dialog and the busy state expire locally before the lock releases. The protected detail view expires. The user can navigate away.
3. Exactly one path-free application outcome is kept, with its acknowledgment. The CLI shows exactly one scoped fake import.
4. A native-approved exact-path file save succeeds. A native cancellation and a delayed journal error before disclosure each give the exact output or refusal.
5. Dismissal works, and no dismissed outcome comes back: not the success, the cancellation or the error.

**What the script checks for each condition.** `tools/verify_windows_app.ps1 -OutcomeOnly` has these checks. They read complete snapshots of the owned window through `tools/ProofSnapshot.psm1`. A text match stays inside one element or one outcome record. A UI Automation error, or a tree without its sentinel button, never counts as absence. After an outcome is dismissed, every later snapshot and every later click fails if it returns.
- Condition 2: the import dialog and Settings busy state expire after a real minimize and restore, and stay gone. In Key Detail (`Test-KeyDetailExpiry`), a displayed fake secret must be masked after the minimize and restore. Key Detail then stays open and masked for 5 seconds. A second reveal waits on the fake-journal lock across the minimize and restore. After the lock is released, Key Detail must stay masked, with no stale error, for 5 seconds, while the journal shows that the native reveal finished.
- Condition 3: `Assert-OutcomeRegion` counts the `status` and `alert` notices in the outcome region and requires exactly one, with the expected text.
- Condition 5: `Dismiss-Outcome` waits for each removal, then checks that the outcome stays absent. `Assert-NoOutcomeResurrection` covers the success, cancellation, error, pending, unconfirmed and completed-while-locked texts.

`tools/test_proof_snapshot.ps1` tests the snapshot logic without the app. It has 9 cases, one or more for each review finding. Each of 4 deliberately broken versions of the logic fails at least one case.

`-OutcomeRehearsal` runs the Key Detail (Hide in place of a minimize, and the delayed reveal), cancellation, count and no-resurrection checks on the real GUI. It never minimizes. On 2026-11-01 it passed on the pinned build, after the fixes for the second script review (output in `W:\AKM-archive\proofs\20261101T120804Z-outcome-rehearsal-5\`). In the real window, it counted two cancellation records as 2, and the exactly-one check rejected them. A rehearsal never passes R4-04.

Record the build hash, the result and the cleanup. On any failure, stop, keep the evidence, and return to the owner. The proof does not test Windows lock or suspend.

### Rounds 1 to 3

These are fixed in the source and remain **not accepted**. The scenario and the acceptance check for each are in `git show archive/pre-cleanup-2026-11-01:docs/REVIEW_LEDGER.md`, lines 84 to 152.

| ID | Severity | Subject |
|---|---|---|
| 1A-R1-01 | P2 | Warning classification: backup and journal warnings had one shared type |
| 1A-R1-02 | P2 | Unsafe append boundary: a final record with no newline joined the next one |
| 1A-R1-03 | P2 | Remount loss: delayed notices were lost between mounts |
| 1A-R1-04 | P2 | Malformed-record errors echoed record content |
| 1A-R1-05 | P2 | Uncertain journal outcome reported as "not recorded" |
| 1A-R1-06 | P3 | Legacy `*-start` action names still emitted |
| 1A-R1-07 | P3 | The offending sequence number was not shown. Round 5: not fixed, see 1A-R5-06. |
| 1A-R2-01 | P2 | Whitespace append boundary (form feed and other non-JSON whitespace) |
| 1A-R2-02 | P2 | `completedWhileLocked` receipts lost between roots |
| 1A-R2-03 | P3 | Application warnings lost on handoff |
| 1A-R2-04 | P3 | Inherited action keys such as `__proto__` crashed Activity |
| 1A-R2-05 | P3 | Delete promised a `.bak` backup. R4-01 covers the rest of this. |
| 1A-R2-06 | P3 | Sequence numbers above 2^53 lost precision in IPC |
| 1A-R3-01 | P2 | A dispatched import outcome was lost on hide and navigation |
| 1A-R3-02 | P3 | Clipboard cleanup episodes shared one warning ID. R4-03 covers the rest of this. Round 5: not fixed, see 1A-R5-02. |
| 1A-R3-03 | P3 | Stale replies could return session A's path to obsolete callers |

The [round 5](reviews/2026-11-01-unit1a-review5.md) review accepted the source seam of 1A-R1-01, R1-02, R1-04, R1-06, R2-01, R2-04, R2-05 and R2-06, and found the others fixed with limits, except R1-07 and R3-02. None closes before the unit passes its review. The round 4 review supported the R3-01 and R3-03 fixes, and the ordinary R3-02 cases, within its examples. A real Tauri webview reload was never tested separately. The browser reload tests use mocked IPC.

## Unit 1c: session events and import reviews (split from unit 1a)

The owner split these findings out of unit 1a on 2026-11-01, after the protocol design failed review twice ([design review 1](reviews/2026-11-01-unit1a-design-review.md), [design review 2](reviews/2026-11-01-unit1a-design-review2.md)). Sections 2 and 3 of [the design](design/unit1a-protocol.md) and the open findings of both design reviews are the input. They must pass before the release gate.

| ID | Severity | Status | Problem and the test that closes it |
|---|---|---|---|
| R4-02 | P2 | Open ([round 5](reviews/2026-11-01-unit1a-review5.md): not fixed, see 1A-R5-01) | A delayed `vault-locked` event from session A could make the UI show "locked" while the native session B stayed open. Fix: a native session ID for each open, carried by status replies and lock events. Test: old A events at each point of B's open, and real B locks at the same points. |
| 1A-R5-01 | P2 | Open | [round 5](reviews/2026-11-01-unit1a-review5.md), `desktop/src/App.tsx:159`, `desktop/src/nativeSession.ts:102`. The status request starts before the lock subscription is registered. A lock emitted in that gap is missed, and a stale unlocked status is accepted. Reproduced with mocked native calls. Test: delay the listener registration, lock after the status is computed, and require the stale status to be rejected; also cover a subscription failure and root replacement. |
| 1A-R5-03 | P2 | Open | [round 5](reviews/2026-11-01-unit1a-review5.md), `desktop/src/components/Settings.tsx:54`, `desktop/src-tauri/src/pending.rs:25`. Cancelling or editing an import clears the frontend review but sends no native cancellation, so the old token can still be confirmed. A delayed preview can install its token after the dialog closes. Test: through the real App, API and native seam, cancel or edit before confirmation, also while a preview is delayed; the old token can never be confirmed or installed, and a dispatched confirmation keeps its outcome. |
| 1A-R5-05 | P3 | Open | [round 5](reviews/2026-11-01-unit1a-review5.md), `desktop/src-tauri/src/pending.rs:46`. A confirmation with stale token A, arriving after review B is installed, is rejected but also removes B. Reproduced with the real component. Test: deliver A after B; A fails, and B stays confirmable once. |

## Later work units

| ID | Severity | Status | Unit | Problem |
|---|---|---|---|---|
| 1A-D2-09 | P3 | Open | 3 | [design review 2](reviews/2026-11-01-unit1a-design-review2.md), finding 9, `crates/akm-cli/src/export.rs:267`. A failed CLI file export ignores a failed delete request for its owned plaintext temporary file. The user is not told that a fake or real plaintext copy may remain. Test: inject a failed delete on the temporary file and require a visible "could not confirm cleanup" result. |
| M1 | Medium | Open | 1b | The journal `<vault>.audit.jsonl` is plain text beside the vault. If that folder syncs, the cloud provider gets key titles, projects, environments and times. Deleting the end of the log is not detected. |
| P2-JOURNAL-IDENTITY | P2 | Open (design) | 1b | Before the journal moves, decide its location, how it identifies a vault (rename, clone, replace, roll back, two vaults), how `akm audit` works without a password, and the ACL and reparse-point rules. |
| P2-JOURNAL-OUTCOMES | P2 | Open (design) | 1b | Keep damaged bytes, and report damage without claiming its cause. Keep logging that fails closed before a disclosure. After a commit, a logging failure must be a warning only. |
| H9 | High | Open | 2 | The rotation age comes from the last edit (`model.rs`), so a notes edit resets it. There is no rotate action. |
| M7 | Medium | Open | 2 | There is no entry history and no Recycle Bin, and deletes leave no deletion record. A KeePassXC merge can bring deleted keys back. |
| P2-HISTORY-SEMANTICS | P2 | Open (design) | 2 | The pinned `keepass` 0.17.0 tracked APIs add history even on a no-op edit, and they use one-second times. Decide the rules for no-ops, fast edits, clock rollback and restore before you build. |
| M2 | Medium | Partial | 2 or later | The 12-character minimum for new vaults is done. There is no safe change-password workflow. It needs its own plan. |
| P2-TIME-REFRESH | P2 | Open | 3 | The expired and rotation-due states are calculated when the list loads. They can go stale while the Keys view stays open. A refresh must not count as user activity. |
| M5 | Medium | Open | 3 | The dotenv parser has no defined external profile for comments, quotes, multiline values and BOM. For example, `TOKEN=abc # note` keeps the comment in the value. |
| L1 | Low | Partial | 3 | `ApiKey` and `ApiKeyInput` derive a `Debug` that includes the secret, and `Vault::list` loads secrets for callers that need only metadata. |
| L2 | Low | Partial | 3 | UUID and exact-title matching come first, and ambiguous matches are refused. The fallback search still matches notes and URL. |
| P3-REVEAL-CLOCK | P3 | Open | 3 | Found by the UI review of 1 Nov 2026 on `5fca93e`; the same code is on `main`. The Key Detail reveal countdown (`desktop/src/components/KeyDetail.tsx`) measures time with the wall clock (`Date.now()`). If the system clock moves back, a revealed fake secret stayed on screen past 30 seconds; with a 40-second rollback the text said "Hides in 39 seconds". A fix uses a monotonic clock (`performance.now()`) or a native expiry, with a test that moves the wall clock back. |
| L3 | Low | Open | 3 | The CLI stores a date-only expiry at 00:00Z and the GUI at 23:59:59Z. |
| L6 | Low | Partial | 3 | The frontend rounds expiry days up. Rust still truncates rotation days. |
| M6 | Medium | Partial | 1a, 1b | Post-commit journal failures are warnings, and the CLI logs reveals. Unit 1a adds damaged and missing journal states. Malformed-tail handling belongs to unit 1b. |
| M13 | Medium | Open | 4 | Reminders appear only inside the app. Correct rotation data and a notification privacy policy come first. |
| M12 | Medium | Partial | 4 | Shared focus handling exists. Check focus for every modal, including import. On the Keys screen, each key row's accessible name or description must announce its expiry and rotation status. Today the row button has a title-only label (`KeysView.tsx`). The Keys screen still has recorded layout debt. |
| P2-GUI-PROVENANCE | P3 | Gap | None | The raw output of an earlier partial GUI run was recovered, but which binary it ran is unknown. That run failed after its creation, import, clone and upgrade passes. Do not use those passes as evidence for any identified build. |
| H6 | High | Partial | Owner test | Idle lock, session lock and suspend handlers pass with fake clocks and synthetic messages. Real OS delivery and a real 10-minute idle wait are not tested. |
| P1 | Premise | Partial | 5 | Read-back checks and real KeePassXC tests exist. A second independent reader (kdbxweb) and a professional review do not. |
| P4 | Premise | Partial | Release | Signing, an updater, `SECURITY.md`, installation tests and a professional review are missing. CI is configured and has never run. |

## Closed

These baseline findings are fixed. Phase-one reviews accepted the fixes within their scope.

- **C1** Child processes get no `AKM_*` variable, `--password` is removed, and the vault is released before launch. The parent process can still hold `AKM_PASSWORD`.
- **C2** Import matches only the exact project, environment and name. Empty values are refused. The user reviews and confirms each import.
- **C3** KDBX 3.x and 4.0 vaults open read-only until an explicit upgrade, which keeps the original as a backup.
- **H1** Changes apply to a candidate copy. A failed save leaves the open vault unchanged.
- **H2** Entries are identified by the native KDBX UUID.
- **H3** Clipboard history, cloud and monitor exclusions are set. Cleanup checks ownership. A crash or a third-party clipboard manager can still keep a copy.
- **H4** Readers use snapshots. Writers take a short lock.
- **H5** `.cmd` shims are resolved explicitly.
- **H7** Export and run need a project and an environment, or an explicit `--all`.
- **H8** Each save is decrypted and compared before it replaces the vault. Five dated backups are kept. The same library does both the save and the check.
- **M3** The fingerprint is checked again just before replacement. A cloud client that is not AKM can still race that check.
- **M4, M8, M9, M10, M11, L4, L5** The username is preserved, the native save dialog and file ACLs are checked, group lookup ignores case, `--output` writes UTF-8, the window asks for capture protection, failed writes clean up, and reserved variable names are refused.
- **P3** Every artifact must now match its source. That is a process rule, not a guarantee.
- **P2** (thin differentiation) was answered by framing, not by code. AKM is a narrow personal Windows workflow.

## Adding a finding

Give it a stable ID, a severity, the commit it was found on, the file and line or the public seam, the exact scenario, and the work unit that owns it. Close it only as AGENTS.md says. Record a disputed finding as disputed, with the evidence.
