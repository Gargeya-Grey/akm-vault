# Unit 1a advisory after round 8 (raw)

- Date: 2026-11-01
- Model: GPT-6 Astra (`gpt-6-astra`), Codex provider, requested `reasoningEffort: max`, asked once by the owner for guidance
- Checkout: branch `fix/unit1a-sound-findings` at `d8858ef` (application source `44330ee`)
- Kind: advice, not an acceptance review. It does not replace the review that REVIEWING.md requires.
- Owner decisions on its questions, 2026-11-01: design first, in unit 1a; a fixed CLI mask; best-effort clipboard cleanup (STATE.md).

The text below is the advisor's report. Its links to repository files became the link text followed by the repository path. Other differences cannot be excluded.

---

1. **Diagnosis**

**Verified** means inspected in source, Git history or a review report this turn. I ran no behavioral tests. **Inferred** identifies my diagnosis or recommendation.

- **[Inferred, design] The clipboard interface hides the facts needed to manage ownership.** **[Verified]** `copy_reported` assumes a publisher `Err` changed nothing, but `publish` successfully empties the clipboard before several `Err` exits. `CopyAttempt { sequence, failure, rolled_back }` describes selected outcomes without consistently describing effects. This explains the progression through R4-03, 1A-R5-02, R6-03, R7-02 and R8-03. See clipboard.rs:96 (`desktop/src-tauri/src/clipboard.rs:96`), publication exits:372 (`desktop/src-tauri/src/clipboard.rs:372`) and R8-03 (`docs/reviews/2026-11-01-unit1a-review8.md:25`).

- **[Inferred, design] Outcome, ownership and user assurance are separate concerns that the implementation repeatedly conflates.** **[Verified]** Session tracks cleanup episodes separately from leases; retained warnings survive recovery; copy commands return a duration; frontend text interprets that duration as guaranteed cleanup. Drop ignores the final cleanup error. R6-02, R7-03 and R8-02 expose different consequences. See session.rs:782 (`desktop/src-tauri/src/session.rs:782`), commands.rs:325 (`desktop/src-tauri/src/commands.rs:325`), KeyDetail.tsx:102 (`desktop/src/components/KeyDetail.tsx:102`) and clipboard.rs:171 (`desktop/src-tauri/src/clipboard.rs:171`).

- **[Inferred, design] The CLI's disclosure policy lacks one authoritative definition.** **[Verified]** The orchestration classifies disclosure from flags, while the formatter independently exposes secret fragments. Its unit test explicitly requires those fragments. Contract 12 therefore conflicts with an established behavior, rather than merely missing an error check. See main.rs:338 (`crates/akm-cli/src/main.rs:338`), mask and its test:774 (`crates/akm-cli/src/main.rs:774`), contract 12 (`docs/PROJECT.md:51`) and R8-01.

- **[Inferred, process] Fixes and tests repeatedly close examples without covering their neighboring paths.** **[Verified]** History shows export warning output fixed in `8e1d34d`, while mutation output remained defective until `44330ee`—R6-01 followed by R7-01. Clipboard tests cover setup failures directly at the publisher, while the "every event" table substitutes scripted publisher results. Those tests never combine setup failure with an existing Session lease. See publisher tests:1175 (`desktop/src-tauri/src/clipboard.rs:1175`) and scripted table:1758 (`desktop/src-tauri/src/clipboard.rs:1758`).

- **[Inferred, process] Evidence is sometimes collected below the level of the acceptance claim.** **[Verified]** R7-04 requested the CLI's retained line, warning and successful exit together. The replacement test calls only `record_completed_with`; its injected error follows a successful real append. It never exercises warning emission or exit selection. R8-04 correctly remains a coverage finding, not a demonstrated incorrect exit. See main.rs:891 (`crates/akm-cli/src/main.rs:891`) and R8-04 (`docs/reviews/2026-11-01-unit1a-review8.md:29`).

- **[Inferred, process] The review scope and implementation focus have been mismatched.** **[Verified]** Review instructions cover the whole unit and every changed function's callers, including preserved behavior. Later reviews explicitly disclaim exhaustive historical caller inspection. Consequently, pre-existing mask behavior and unchanged success text can emerge late. The owner authorized the additional rounds; this is not evidence of unauthorized continuation. See REVIEWING.md:15 (`docs/REVIEWING.md:15`), round 7 limits (`docs/reviews/2026-11-01-unit1a-review7.md:77`) and STATE.md:7 (`docs/STATE.md:7`).

**[Inferred]** The implementer's clipboard hypothesis is supported. Its broader "CLI and journal converged" conclusion needs qualification: the closed-pipe repair is structural, but disclosure policy and caller-level completion proof remain unresolved. The eight rejections also contain distinct ownership, parser, backup and proof failures; they are not eight manifestations of one clipboard bug.

2. **Proper fixes for the open classes**

**[Inferred recommendation] Clipboard lifecycle: put effect observation and lease transitions inside one clipboard module.**

Keep Session's authorization gate. Inside it, the clipboard module should own publication, cleanup eligibility and episode transitions. The low-level adapter observes successful emptying, individual format transfers, readback, sequence capture and rollback. Session retains application notices until acknowledgment; React presents receipts without inventing current clipboard state.

Replace `Result<CopyAttempt, CommandError>` and its independent flags with explicit outcomes, for example:

- `RefusedWithoutMutation(error)`
- `FailedWithoutAkmSecret(error)`
- `Published(LeaseEvidence)`
- `FailedWithRetryableResidual(error, LeaseEvidence)`
- `FailedWithUntrackableResidual(error)`

`LeaseEvidence` contains a nonzero sequence captured during the publication interval and the protected comparison value. It authorizes a later ownership check; it does not assert continuous possession.

The invariants should be:

- Preparation refusal preserves the previous cleanup candidate.
- Successful initial emptying retires the previous lease immediately.
- Failure before secret transfer leaves no AKM-secret cleanup obligation, even if harmless exclusion or empty formats remain.
- Confirmed rollback leaves no lease or subsequent cleanup warning.
- Possible residual secret data produces either safe retry evidence or explicit final uncertainty.
- Cleanup never adopts a later sequence or clears a newer identical copy.
- One uninterrupted episode produces one warning; recovery and recurrence produce distinct IDs. Acknowledgment removes only the observed warning.
- Locking remains effective even when clipboard cleanup fails.

Delete the assumption that `Err` means untouched, `rolled_back` flag combinations, string-based internal cleanup classification, and Session's need to reconstruct episode transitions from separate observations.

**[Inferred recommendation] Derive the state table from production paths.** Enumerate every fallible operation and exit in `publish` and `clear`, including rollback, close/drop and Session accounting. Inject faults through the existing low-level `ClipboardApi` fake while executing the real publisher, lease and Session. Record actual format contents, sequence, successful mutations and cleanup calls.

Run those paths with no lease, an existing lease, and an outstanding historical warning. Cross relevant paths with rollback success/failure, zero/unreadable sequence, newer identical copy, expiry, lock and shutdown. Assert that each intended fault was actually reached. Derive expected states from the contract and observed effects, not from returned `CopyAttempt` values. Keep literal assertions on contents, lease disposition, warning IDs and subsequent calls.

**[Verified]** The existing fake already supports operation-level faults and format inspection, so this does not require inventing another simulated publisher. See clipboard.rs:746 (`desktop/src-tauri/src/clipboard.rs:746`).

**[Inferred recommendation] Treat every clipboard text as part of that contract.** Use shared success wording such as: "Copied. Cleanup is best effort after 30 seconds, on lock and on normal exit. The copy may remain if cleanup fails." Preserve the existing history-only uncertainty warning.

Cover both success messages, Settings' lock explanation, the README, publication errors and the generic copy-error fallback. Delete unsupported guarantees and causal claims. In particular, unreadable verification or sequence zero should not automatically say "The clipboard changed."

**[Verified]** Additional affected text exists at Settings.tsx:347 (`desktop/src/components/Settings.tsx:347`), README.md:86 (`README.md:86`) and clipboard.rs:408 (`desktop/src-tauri/src/clipboard.rs:408`). Render both actual React callers and check their messages alongside fake Session busy-expiry, lock and shutdown cases. No message should promise a warning after exit.

**[Inferred recommendation] CLI disclosure: resolve output intent once, before logging or formatting.**

Use a parsed output type such as `MetadataOnly`, `SecretField` or `RevealedDetail`. The same value selects both authorization/logging and rendering. Recommend a constant, secret-independent default mask and delete `mask(secret)` plus its fragment-preserving test.

Preserve contract 12's distinctions:

- Required request logging fails before disclosure or launch.
- Completed mutations and exports remain successful when subsequent logging or warning output fails.
- Partial stdout failure remains an error with explicit uncertainty about bytes already delivered.
- Request records do not imply successful delivery or child execution.

Test default `get`, all field forms, reveal, list/JSON, stdout/file export and run. Include short, nine-character, long and Unicode fake secrets; damaged and unwritable journals; partial writes and flush failures; closed output pipes; and child-not-started assertions. Compare default output for otherwise identical keys with different secrets, including different lengths. A constant mask must not depend on either.

**[Inferred recommendation] Caller-level coverage: move fault injection beneath production orchestration.**

Extract a production CLI entry/execution function that accepts journal and output dependencies and returns its exit decision. Both `main` and tests must use it. Inject the completion failure after the line is written, then assert—in one execution—the published export, retained completion line, stderr warning and exit 0. Repeat with stderr failing. Mutants that suppress the warning or return failure must fail these tests.

Keep fault mechanisms out of normal release options. Pair deterministic injected-fault coverage with executable CLI checks. Remove helper-only tests from the evidence claimed to prove caller behavior; retain them only where they add distinct coverage.

For desktop changes, exercise the production command registration and actual frontend argument shapes. **[Verified]** The current mock dispatch test maintains a separate handler list, the issue identified by design review 2 finding 7. See ipc_dispatch.rs:89 (`desktop/src-tauri/tests/ipc_dispatch.rs:89`).

**[Inferred recommendation] Runtime identifiers in documentation: verify producers and examples together.**

Correct the current `run-start` claim to `run-requested`. Maintain a small catalog of newly emitted actions and their meanings, using constants or a producer-side enum where useful. Keep readers tolerant of legacy and unknown action strings.

Run a fake child and exports, inspect their actual journal actions, and compare them with the documented examples. Delete stale current-behavior claims, not historical review evidence or legacy interpretation. **[Verified]** Runtime, documentation and the existing child regression currently disagree at main.rs:403 (`crates/akm-cli/src/main.rs:403`), CLI README:29 (`crates/akm-cli/README.md:29`) and security.rs:263 (`crates/akm-cli/tests/security.rs:263`), respectively.

3. **Scope, order and stop conditions**

**[Inferred recommendation] Design first, then keep these classes in unit 1a.** Split implementation into bounded changes, but do not split publisher behavior from lease management, notices or their text. Leave session-event/import work in the already authorized scope boundary for unit 1c, and leave journal migration in 1b.

Recommended order:

1. Obtain the owner's policy and continuation decisions below.
2. Produce the actual-path clipboard inventory, output-policy inventory and caller-to-test map.
3. Review the bounded design, including failure outcomes and observation limits.
4. Add failing regressions through the production callers; implement each class and its sibling cases.
5. Perform the text and runtime-identifier sweep.
6. Run required checks on the final candidate, bind applicable executable evidence to it, then request one formal acceptance review with the explicit baseline-to-candidate diff.

**[Inferred recommendation] Stop** if an effectful exit has no defined outcome, a required test only exercises a substitute implementation, a defect regression does not distinguish affected behavior, or the authorized review allowance is exhausted. A compile failure from a changed interface is not a behavioral regression result.

**[Verified]** R4-04 remains a separate deferred proof gate. Source acceptance can pass while work-unit completion remains NO; further source fixes cannot discharge that gate. See FINDINGS.md:18 (`docs/FINDINGS.md:18`).

4. **Decisions only the owner can make**

- **[Inferred recommendation] Default CLI disclosure.** Options: constant mask; retain fragments but require successful pre-disclosure logging; explicitly exempt fragments from contract 12. Recommend the constant mask. It preserves useful metadata inspection without exposing secret-derived output.
- **[Inferred recommendation] Clipboard product promise.** Options: retain copying with explicit best-effort cleanup, or disable copying if guaranteed removal is essential. Recommend best effort. Do not delay vault locking to preserve a cleanup promise.
- **[Inferred recommendation] Resume scope.** Options: authorize the bounded design-first repair within 1a, split clipboard acceptance into a separately authorized unit, or park it. Recommend keeping it in 1a with a fixed review allowance and no automatic continuation after rejection.
- **[Inferred recommendation] R4-04 evidence.** Options already recorded are a build-gated, fake-data observation log or stronger external event observation with explicit limits. Recommend keeping the proof deferred during this repair, then choosing instrumentation capable of observing the disputed events. Any real-window attempt still needs the owner's separate readiness confirmation.

**[Inferred]** Module names, enum spelling, dependency injection and test organization are engineering decisions; they do not require additional owner approval.

5. **Short pre-review checklist**

**[Inferred recommendation] Apply these before requesting review:**

- Trace every side-effecting exit with both empty and previously active state. Targets R4-03, R5-02, R6-03, R7-02 and R8-03.
- Check every user assurance against observable failure outcomes, including retained notices and shutdown. Targets R4-01, R6-02, R7-03 and R8-02.
- Inventory every disclosure, commit and output sink; inject failure before and after its irreversible step. Targets R5-04, R6-01, R7-01 and R8-01.
- Require one caller execution to prove side effect, warning and terminal result together. Targets R7-04 and R8-04.
- Exercise all consumers of diagnostic data, including malformed records with otherwise valid fields. Targets R5-06 and R6-04.
- Test ownership across delayed, missing and reordered callbacks, including cancellation before registration. Targets R4-02 and R5-01/03/05; those repairs now belong to 1c.
- Compare documented identifiers with emitted events, and distinguish source tests from observable runtime proof. Targets R8-05 and the R4-04 evidence gap.

**[Verified]** The checkout remains clean at `d8858ef`, with application files matching `44330ee`. No files were created or edited, and no builds, suites, GUI, clipboard or OS-session operations ran. This is an Astra/max advisory; it does not satisfy the repository's prescribed Terra/max acceptance-review requirement.
