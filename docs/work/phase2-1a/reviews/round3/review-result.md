# Unit 1a independent acceptance review, round 3

## Source verdict

**Reject bounded unit acceptance.** One P2 preservation-contract failure and two P3 contract failures are supported below. Findings 1 and 3 concern pre-existing direct-caller behavior rather than changes introduced by this unit. They remain relevant because this review explicitly includes the phase-one preservation and root-handoff contracts. Finding 2 concerns the new ID-based application-warning contract. This is not a numerical rescore, whole-product readiness verdict, or professional audit.

### 1. P2. A dispatched import can finish without its outcome acknowledgment

`desktop/src/components/Settings.tsx:66-73,76-90,239-268`, `desktop/src/App.tsx:271-277`, `desktop/src/api.ts:17-28`, and `docs/PROJECT.md:47`.

Confirm a reviewed import. While the native confirmation is pending, hide and restore the window. The visibility handler closes the dialog but retains the in-flight confirmation. Navigate from Settings to Keys before its successful reply. Unmount increments `importRequest`; the continuation discards the successful report. The App root remains mounted, so the API generation has not changed and no completion receipt is made. Returning to Settings shows neither the successful import result nor a retained receipt. The owner cannot tell whether the import completed, even though the preservation contract says a dispatched confirmation keeps its outcome acknowledgment. A repeated import can overwrite a later intervening change.

**Evidence.** `probe.test.tsx` and `probe-output-final.txt` exercise the copied, hash-verified actual App, Settings, and API code with a deferred mocked native confirmation. The mock resolves with one added item; the test observes no completion message or retained receipt after navigation. It does not establish a real native commit. Existing `Settings.security.test.tsx:147-164` tests hide without the subsequent navigation.

**Correction/acceptance test.** Retain the secret-free dispatched-confirmation outcome outside the short-lived Settings view. Exercise successful and failed confirmation after hide, navigation, and return. Verify exactly one truthful acknowledgment, no stale scope or secret disclosure, and no false lock of another vault. Also check dispatched native file export, whose success path uses the same view-owned invalidation pattern.

### 2. P3. A later same-message safety failure has no distinct native ID

`desktop/src-tauri/src/session.rs:171-175,225-234,531-543,580-586`; `desktop/src-tauri/src/clipboard.rs:67-90,453-456`; frozen plan `PLAN.md:17,20-21`.

An owned clipboard cleanup can fail because the clipboard is busy and produce application warning ID 1. The cleanup can subsequently succeed, while ID 1 is still visible and unacknowledged. A later, separate copy and cleanup can fail with the same native message. `warn_once` suppresses this second occurrence solely because ID 1 is still in the vector. Dismissing the already observed ID 1 therefore removes the only warning; the second occurrence never had its own ID. A later timer retry might emit another warning if it still fails, but that does not preserve the lost occurrence. This contradicts the required distinct IDs for newer recurrences and permits an old acknowledgment to cover a newer safety failure. The existing Session test at `session.rs:781-808` creates the recurrence only after dismissing the old warning, so it does not cover this ordering.

**Evidence.** Deterministic source inspection of the exact `warn_once` and cleanup call sites. This reviewer did not operate the real clipboard or run a native fault injection.

**Correction/acceptance test.** Model warning episodes so a recovered and subsequently recurring failure receives a new ID even if an older warning is pending. Do not turn each timer retry of one uninterrupted episode into an unbounded warning. Inject first cleanup failure, recovery, second same-message failure, then acknowledge only the first snapshot. The second ID must remain visible across lock/open and polling.

### 3. P3. Root teardown does not fence all old API replies

`desktop/src/App.tsx:123-130`, `desktop/src/api.ts:17-29,32-35`, `desktop/src-tauri/src/commands.rs:24-43,86-110`, and `desktop/src-tauri/src/error.rs:38-42`.

Teardown increments the frontend generation, but `invoke` exempts `vault_status` from the success-generation check and its catch block rethrows non-completion errors without checking generation. An old status success can therefore return A's path and vault name to its obsolete API caller after B's root is mounted. An old unlock failure can likewise return `No vault at <A path>` after teardown. The direct caller receives data despite the contract that teardown invalidates old API work and stale replies do not leak protected paths. The new root's React UI did **not** display A's path in the probe, so a user-visible B-screen disclosure is not established.

**Evidence.** `probe.test.tsx` and `probe-output-final.txt` mount a new unlocked root B before delivering each deferred mocked native reply. Both old API promises return the original A-bearing value. B remains rendered and its DOM does not contain the A path. The native missing-vault error shape is supported by the cited native source.

**Correction/acceptance test.** Fence obsolete status and ordinary error replies at the API boundary without losing the safe `completedWhileLocked` receipt or weakening the locked-screen startup and stale-lock protections. Test actual App/API callers with A status success and A path-bearing error delivered between and after root mounts. Assert the old caller receives only a sanitized cancellation, B remains active, and no A path or title is exposed.

## Runtime evidence and limits

The only independently executed behavioral probes were the three fake-data frontend cases in `probe.test.tsx`. The source copy used by the probes matches all 29 pinned `desktop/src` files. The three tests pass because they assert the observed defective outcomes. The initial PowerShell pipe invocation reported shell exit 1 from stderr handling despite the tests passing; the retained final `cmd.exe` invocation exited 0 and its verbose output is in `probe-output-final.txt`. No full suite, application build, GUI/native automation, real clipboard, settings access, process inspection, or OS lock/suspend was performed by this reviewer.

Main-owned records report Rust 157 passed with two separately gated ignored tests, strict Clippy/rustfmt, frontend 55, production build and browser 10. The initial production TypeScript build failed on two test-only options, followed by the affected test and build passing; these overlapping runs are not 56 frontend tests. Browser reload retains mocked native warning stores. Disk-sync and late-lock faults are injected. The reported exact-GUI JournalOnly and ExportOnly passes used GUI SHA-256 `42f3f63afae5c6dfa426c92da7635972d5e0642c2d6e8482681588dfc14510c8`; CLI SHA-256 is `0024f9005a68bf5b5ab9d32297c9d6eb4043ee5616a7ae6c465a054adfddcdc0`. JournalOnly used the recorded UI Automation root-refresh recovery. The prior failed startup proof remains preserved. The recorded GUI runs skipped clipboard and restored settings byte-for-byte. The main session also reports KeePassXC interchange passing. These are not independent reruns by this reviewer.

The inspected source otherwise supports the bounded journal seams. A single sidecar lock encloses snapshot or full validation plus append; malformed errors are structural; requested and completed actions are distinguished; native audit sequences serialize as decimal strings; operation warnings carry fixed kinds rather than raw vault errors. These observations do not establish real OS delivery, crash-durable receipts, installation, or a secure/authenticated complete journal. The current plaintext journal remains beside the vault. Existing layout debt remains reported.

## Identity and method

Requested model/variant was `openai/gpt-6.1-terra#max`. The host identifies this model as `openai/gpt-6.1-terra`. Active effort and actual token usage are not exposed to this reviewer, so `max` execution and token counts are not independently verified.

The round-three input manifest SHA-256 is `b52d434f7e22132f5619800571f5b1e810e9eba2eea7d186474fc20bf4ddf4e4`. Its pinned plan, source manifest, and diff match their declared hashes. The 139-entry baseline archive matches its manifest. Applying the combined diff in memory reconstructs all 29 changed pinned paths, including new files. `source-integrity.json` records all 145 path comparisons, the separately classified expected main-owned documentation change, the probe copy, and independently recomputed CLI, GUI, MSI, and NSIS hashes. At the recorded check, no pinned application/build/test or proof-harness file differed. `docs/STATE.md` differed, which is allowed main-owned checkpoint activity; no application input was edited by this reviewer.

The proof plugin's delegation and edit routes were skipped because the user required one read-only reviewer and no subagents. No project verification skill was found. `docs/VERIFICATION.md` supplied the project's evidence policy.

**Next action.** Return this rejected correction cycle and this report to the owner. Do not start another correction cycle or other phase-two work without the owner's decision.
