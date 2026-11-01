# Unit 1a: frontend–native state protocol

- Outcome, 2026-11-01: revision 2 failed its second design review ([design review 2](../reviews/2026-11-01-unit1a-design-review2.md)). The owner then split the unit (option A). Sections 4 to 6 are implemented in unit 1a, with the changes listed under "Implemented in unit 1a" below. Sections 2 and 3 move to unit 1c ([ROADMAP.md](../ROADMAP.md)), together with the open findings of both design reviews. Sections 1, 7 and 8 still describe the whole protocol; unit 1c revisits them.
- Status of revision 2: reviewed, not ready, 2026-11-01. Revision 1 (`dc64e43`) failed its design review with 13 findings ([design review](../reviews/2026-11-01-unit1a-design-review.md)). The table in section 9 maps each finding to its change.
- Owner decision, 2026-11-01: design first (option B in STATE.md), after the fifth unit 1a review ([round 5](../reviews/2026-11-01-unit1a-review5.md)).
- Scope: the six round-5 findings, 1A-R5-01 to 1A-R5-06 ([FINDINGS.md](../FINDINGS.md)), and the rules that stop the same class of defect. Nothing else in unit 1a changes.
- Public API change: yes. `env_import_preview` changes shape. Two IPC commands are added: `env_import_open` and `env_import_close` (section 3). The CLI gains one journal record (section 5). The journal file format and the KDBX format do not change.

## 1. The rules

These rules apply to the exchanges this unit changes: the session status and its events, import reviews, clipboard cleanup, CLI export and the damaged-record display. A reviewer checks each changed exchange against them. Other exchanges keep their current contracts in [PROJECT.md](../PROJECT.md).

1. **Subscribe, then read.** The frontend registers the `vault-locked` and `vault-view-expired` listeners before it reads the session status. A status reply that started before the listeners were ready is discarded.
2. **A failed subscription fails closed.** If either listener cannot be registered, the frontend shows the locked view, attempts a native lock, and refuses to open a vault until AKM restarts.
3. **Protected results belong to their session.** A reply that shows vault data or the unlocked state is accepted only for the current native owner and frontend generation. Origin-owned receipts (`completedWhileLocked`, the outcome records in `sessionBoundary.ts`) and application warnings are not protected results. They survive a session change, as PROJECT.md contracts 4, 5 and 12 require.
4. **Cancellable native state is revoked natively.** In this unit, the one cancellable native state is a pending import review. The frontend revokes it natively on every abandonment path. Work that the native side has already accepted (an import confirmation that took its review, an authorized file publication) is not cancellable. It reports its true outcome.
5. **A rejected request never consumes valid state.** A stale or wrong token fails without removing the state that belongs to the current, valid request.
6. **Uncertain is not success.** When the native side cannot confirm a cleanup or an outcome, it reports uncertainty with a warning and a safe retry where a retry can help. It never reports an unknown result as "done" or as "someone else owns it now".
7. **Record the outcomes the program can know.** An export (GUI and CLI) records `export-requested` before the disclosure and `export-completed` after the program knows the write succeeded. Reveal and copy keep their request-only records, because the program cannot know that the user saw or pasted the value.

## 2. Session status and lock events (1A-R5-01)

**Today.** `App.tsx` starts `loadStatus()` in one effect and registers `vault-locked` in another. `listen()` is asynchronous. If the native side computes an unlocked status, then locks and emits `vault-locked` before registration finishes, the event is lost, and `acceptNativeStatus` accepts the unlocked status. A failed `vault-locked` subscription only sets an error message. In `lib.rs`, `let _ = handle.emit("vault-locked", notice);` discards an emit failure. A successful registration does not prove that a later event reaches the frontend.

**Change, frontend.**

- The listener effect owns a root object: `{ ready: boolean, failed: boolean, disposed: boolean }`. It sets `ready` only after both `listen()` calls resolve. It sets `failed` if either call rejects, also when the other succeeded. Its cleanup sets `disposed`, removes the listeners that resolved, and removes any listener that resolves later (today's `if (disposed) cleanup()` stays).
- `loadStatus()` reads the status only when the root is ready, not failed and not disposed. The effect that registers the listeners calls `loadStatus()` once after `ready` is set. The status poll, the activity fallback, reload and upgrade go through the same check.
- `api.status()` takes a guard, `() => root.ready && !root.disposed`. `invoke` calls the guard after the native reply and **before** `acceptNativeStatus` changes the shared owner state. When the guard fails, the reply is dropped without any change to shared state. This stops a disposed root from adopting an owner after a new root started.
- A failed subscription calls `lock(NATIVE_PROTOCOL_ERROR)`. With no known owner, `api.lock()` sends `sessionId: null`, and `lock_owned(None)` refuses unless the native side has no owner. The frontend then shows "Could not confirm the native lock. Close AKM before leaving this computer." The unlock and create forms are disabled while the root has failed, with the text "AKM could not watch the session. Restart AKM."
- **Health bound.** While the view is unlocked, the frontend records the time of the last status reply that it accepted. A timer checks it every second. If no status reply was accepted for 6 seconds (two poll periods), the frontend shows the locked view and attempts a native lock, with the same unconfirmed-lock message if that fails. A hung status call counts as no reply.

**Change, native.** `lib.rs` keeps the emit result. A failed emit records an application warning, "A lock notice could not reach the window. AKM shows the locked view at the next status check.", after the gate is released. The warning is application-wide (rule 3), so it does not lock a later session. The status reply stays the replayable source of truth: a locked status with `sessionId: null` makes the frontend confirm the lock of its known owner (today's `acceptNativeStatus`).

**Bound.** When the event channel loses a lock event but IPC works, the locked view shows within one poll period (3 seconds) plus one status round trip. When status calls fail or hang, the locked view shows within 6 seconds plus one timer tick. Native commands refuse protected access as soon as the native side locks, whatever the view shows. Until the view catches up, data that the view already holds stays on screen (section 8).

**Failure cases.**

| Case | Expected |
|---|---|
| Registration is delayed; native locks after it computed an unlocked status | The status was never requested before `ready`; the first status after `ready` is locked. |
| Both listeners registered; the status is computed unlocked; native locks and the emit fails | The emit failure becomes an application warning; the next poll shows the locked view within the bound. |
| A lock event is delayed until after the status reply | The reply is accepted; the late event locks its own owner only. |
| `vault-locked` registration fails while `vault-view-expired` succeeds, and the reverse | Both: locked view, a native lock attempt, the forms disabled, the error shown. |
| Registration fails with no known owner; `lock(None)` is refused | The unconfirmed-lock message shows. |
| Root A's gate is ready; A unmounts before its status continuation runs | A sends no status request. A reply already in flight is dropped by the guard; the owner state does not change. |
| Root B is still registering when A's late reply arrives | The reply is dropped; B reads status after B is ready. |
| Old A lock events arrive before `vault_prepare_open` replies, between the reservation and the unlock reply, and after B is adopted | None locks B (the existing invalidated set and `beginNativeOpen`). |
| Status calls fail or hang for 6 seconds while unlocked | The locked view shows, and a native lock is attempted. |

## 3. Import reviews (1A-R5-03, 1A-R5-05)

**Today.** `env_import_preview` stores one pending review natively and returns a token. Closing or editing the import in `Settings.tsx` clears only the frontend copy, so the native review stays confirmable. A preview IPC that is still in flight when the dialog closes installs its token later. A preview IPC that is delayed until after a lock and a new unlock takes its snapshot from the new session B at native entry, and can install A's text in B. `PendingImports::take` removes the current review before it checks the token, so a stale token also destroys a newer valid review.

**Change: one native review lease.** The native side holds at most one import lease per session. The lease is the only thing a preview can install into. Anything that is not the current lease is refused, so there is no list of cancelled IDs to evict.

- **`env_import_open { sessionId } -> leaseId`.** Under the native gate, the native side checks that `sessionId` is the current owner, else it returns `locked`. It creates a new lease (a random UUID) bound to the current ticket and presentation, and replaces any earlier lease and its review.
- **`env_import_preview { leaseId, text, project, environment }`.** Under the gate, at entry and again at install, the native side checks that `leaseId` is the current lease and that the current ticket and presentation equal the lease's. If not, it returns `stalePreview` and installs nothing. One lease holds one review; a second preview under the same lease replaces the first.
- **`env_import_close { leaseId }`.** Under the gate, it removes the lease and its review if `leaseId` is current. It succeeds when nothing matches.
- **Native invalidation.** `locked_gate`, `open_with`, `expire_presentation` and the publication refusal in `read_for_publication` already clear `pending`. They now clear the lease too. A webview page load (Tauri's page-load event, `Started`) also clears the lease and the review, so a reload cannot leave a confirmable review behind.
- **Frontend.** `Settings.tsx` opens a new lease before each preview. It closes the lease when the user closes the dialog, edits the text, project or environment after a preview, when the view expires, and when Settings unmounts. A preview reply for a lease that the frontend has closed is ignored.
- **A failed close fails closed.** If `env_import_close` fails, the frontend retries it once. If the retry fails, the frontend locks the native session (`lock(...)` with the message "AKM could not cancel the import review. The vault was locked."). Lock clears the review natively. If the lock is also unconfirmed, the unconfirmed-lock message shows (section 2).
- **`PendingImports::take`** checks the token, ticket, revision and fingerprint first, and removes the review only when all match (rule 5).

**The confirm/cancel order.** The native linearization point is `take_preview` under the gate.

- Close or invalidation reaches the gate first: the confirm gets `stalePreview`, nothing is imported, and its outcome record shows a reported error.
- Confirm reaches the gate first: it has taken the review and is accepted work (rule 4). The close finds nothing and succeeds. The confirm's outcome record shows its true result: success, a reported error, or `completedWhileLocked` when a lock overtakes a committed import (PROJECT.md contract 4).

The frontend does not promise that a confirm dispatched before the close will import. It promises that the outcome record tells the truth (PROJECT.md contract 7: the dispatched confirmation keeps its outcome acknowledgment).

**Failure cases.**

| Case | Expected |
|---|---|
| Preview, then close the dialog, then confirm with the old token | `stalePreview`; nothing is imported. |
| Preview, then edit the text, then confirm with the old token | `stalePreview`. |
| Preview A is in flight; the dialog closes; A arrives | A's lease is not current; nothing is installed; the frontend shows nothing. |
| Close reaches native before the preview it cancels | The lease is gone; the preview is refused. |
| Preview A's IPC is delayed past a lock and unlock of session B; B has installed review B | A's lease was cleared by the lock; A is refused; B's token stays confirmable once. |
| `env_import_open` from A arrives after B is open | `locked`; B's lease is untouched. |
| Twenty previews opened and closed while the first is stalled; the first then arrives | Refused. Only the current lease is valid. |
| Review B is installed; a delayed confirm with old token A arrives | A fails; B stays and can be confirmed once. |
| Close the same lease twice | Both succeed; no other lease changes. |
| Close before confirm, and confirm before close, at the native gate | Close first: `stalePreview`, nothing imported. Confirm first: the import completes, and its outcome record shows it. |
| A lock while an accepted confirm is writing | The confirm reports `completedWhileLocked` if it committed, else its error. |
| Settings unmounts, the view expires, the page reloads | The lease and review are cleared natively. |
| Close fails twice | The vault is locked; the review is cleared; an unconfirmed lock shows its message. |

## 4. Clipboard cleanup uncertainty (1A-R5-02)

**Today.** `clear()` in `clipboard.rs` returns `Ok(false)`, which means "someone else owns the clipboard now", when the captured sequence is 0, when the sequence changed, or when `matches_text` returns false. `matches_text` returns false both when the text differs and when `GetClipboardData` or `GlobalLock` fails. A partial publication can capture sequence 0 and keep a fake secret on the clipboard if its rollback fails. `GetClipboardSequenceNumber` returns 0 when the caller has no access to the window station's clipboard ([Microsoft documentation](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber)). In each case the lease is dropped without a warning.

**Change.**

- `matches_text` returns `Same`, `Different` or `Unreadable`.
- A sequence of 0 is never ownership proof, and AKM never clears on it.
- `clear()` decides under the one clipboard lock:

| Captured sequence | Current sequence | Text | Result |
|---|---|---|---|
| nonzero | 0 | any | **Uncertain, retry.** Keep the lease. Return `clipboardUncertain`. |
| nonzero | changed, nonzero | any | Another program wrote. Release the lease; do not clear. This keeps a newer user copy, including one with identical text. |
| nonzero | same | `Same` | Empty the clipboard, as today. |
| nonzero | same | `Different` or `Unreadable` | **Uncertain, retry.** Keep the lease. Return `clipboardUncertain`. |
| 0 (partial publication, rollback failed) | any | any | **Uncertain, final.** AKM cannot tell its copy from a later one, so it does not clear. It drops the lease and records the warning "AKM could not confirm that the clipboard was cleared. Check the clipboard." |

- The episode handler in `session.rs` turns `clipboardUncertain` into a cleanup warning with its own ID and schedules the normal retry. A later retry that clears the clipboard settles the warning, as R4-03 does today. A second episode while the first warning is pending gets its own warning ID.

**Limits.** The sequence is a 32-bit counter. Equality after 2^32 changes during one lease would be taken as ownership. AKM does not prove the absence of that wrap. It is judged impractical within a lease's lifetime. Retries stop when AKM exits; a lease that is pending at exit stays uncleared, and the warning does not survive the process.

**Failure cases** (fake backend): unreadable text with an unchanged sequence on normal expiry; sequence 0 at cleanup; sequence 0 at capture with a failed rollback; a newer user copy with identical text (sequence changed); an injected sequence reuse (the same value after a foreign write), which shows the limit above; a retry that succeeds and settles the warning; a second episode with its own warning ID during the first warning; recovery followed by recurrence with separate warning IDs. Real Win32 fault injection is not possible here; this limit is recorded.

## 5. CLI export completion record (1A-R5-04)

**Today.** `akm export` records `export-requested`, then writes the file or stdout, and records nothing after it. The desktop app records `export-completed`.

**Change.**

- The CLI creates an opaque operation ID (a random UUID) for each export. Both records carry it in the existing `detail` field: `operation <id>; disclosure requested; N keys selected` and `operation <id>; export completed; N keys`. Neither record contains the destination path. The journal schema does not change.
- After `export::write_new` returns `Ok`, or after the stdout write and flush return `Ok`, the CLI appends `export-completed` through `AuditLog::for_vault(path)`. The vault does not need to stay open.
- If that append returns an error, the CLI prints to stderr: "The export was written, but its completion record could not be confirmed in the activity log. Do not repeat the export." It exits 0, because the disclosure happened. The record may still be present: `append` can write the line and then fail at `sync_data`.
- A failed file export keeps today's behaviour: no completion record, a non-zero exit, and no file at the destination.
- A failed stdout export keeps the non-zero exit and writes no completion record. Its error adds: "Some output may already have been written. Check it before you export again." A stdout write can deliver bytes before a later write or flush fails.
- A failed `export-requested` append keeps today's behaviour: nothing is disclosed, non-zero exit (PROJECT.md contract 12).

**Failure cases.** A file export records both records, with the same operation ID, in order. A stdout export does the same. Two interleaved exports record two distinct IDs, and each completion pairs with its request. A failed file write records only the request. A stdout writer that fails after it accepted bytes, and one that fails on flush, give the partial-output message, a non-zero exit, and no completion record. A completion append that fails before its write, and one that fails after its write at sync, both print the warning and exit 0; the second leaves the record in the journal. No record contains the destination.

## 6. Declared sequence of a damaged record (1A-R5-06)

**Today.** When a record fails typed parsing (`audit.rs`), the snapshot keeps only the physical line. A declared `seq` in otherwise valid JSON is lost.

**Change.**

- When typed parsing fails, the line is read with a small serde visitor over the top-level JSON object. The visitor decodes each member name (escapes included, so `"seq"` counts as `seq`) and counts the members named `seq`. It does not build a map, so a duplicate cannot hide.
- The result is `problem_declared_seq: Option<u64>`. It is `Some(n)` only when the line is a JSON object with exactly one `seq` member whose value is an integer in 0 to `u64::MAX`, written without a fraction or exponent. In every other case it is `None`: no `seq`, two or more `seq` members, a string, a float, a negative number, a value above `u64::MAX`, nested only, or invalid JSON.
- The CLI and the Activity view show "declares sequence N (not verified)". Nothing else from the record is shown. The IPC field is a decimal string, like the other sequence fields (1A-R2-06).

**Failure cases.** `seq: 7` with a bad timestamp shows 7. `seq: 0`, `9007199254740993` (above 2^53) and `18446744073709551615` show exactly. `18446744073709551616`, duplicate `seq` members (also through an escaped name), a string, a float, `7e0`, a negative number, a missing `seq` and invalid JSON show none. Record contents never appear.

## 7. How the work is checked

- Each finding gets a regression test that fails on `cfeb1b0` and passes after the change (AGENTS.md). Frontend tests use the real `App`, `api.ts` and `nativeSession.ts` with a mocked native boundary. Native tests use the real `Session`, `PendingImports` and clipboard lease with fake backends. CLI tests use the release CLI with fake vaults.
- One real dispatch test uses Tauri's mock runtime (the `test` feature is already a dev dependency). It calls `env_import_open`, `env_import_preview`, `env_import_close` and `env_import_confirm` through the generated handler with JSON arguments, and checks the argument names and the reply shapes that `api.ts` uses.
- The full check run in [TESTING.md](../TESTING.md) on the final source.
- Then the final build: build once, record the hashes of `akm-desktop.exe`, `akm.exe`, the MSI and the NSIS installer, run the release-CLI checks of section 5 and the native boundary checks that need no owner gate against those exact files, and hash them again. The hashes go into STATE.md only if both hash runs match.
- One fresh unit 1a review with the REVIEWING.md inputs, plus this design.
- R4-04 and the other owner-gated window tests stay deferred. The new build changes the hashes that the R4-04 proof uses.

## 8. Limits

- When a lock event is lost, the view can show data that it already holds (key names, and a revealed secret during its 30 seconds) until the bound in section 2. Native commands refuse new protected access at once.
- If the webview's script stops running, no frontend bound holds. Native locking and native refusal still apply.
- Real Windows clipboard faults, real event-delivery failure and real IPC timing are not testable here. The fake backends, the mocked boundary and the mock-runtime dispatch test are the evidence.
- Clipboard sequence wrap (section 4) and cleanup after process exit are not covered.
- A webview reload loses document-level state, as today (1A-R1-03 and related limits). The native page-load hook clears the import review; outcome records held in document memory are still lost.
- The 3-second status poll is part of the bound in section 2. It is a second defence behind the event, not a replacement for it.

## Implemented in unit 1a

These differences from sections 4 to 6 answer design review 2 or simplify the change:

- **Section 4, warnings.** A successful retry ends the cleanup episode; it does not remove the warning. The warning stays until the user acknowledges it, and a later episode gets its own warning ID (the R4-03 behaviour, unchanged). A publication that failed but emptied the clipboard in its own open interval (`rolled_back`) holds no unverifiable lease.
- **Section 5, failed file export.** "No file at the destination" means no new destination file: a pre-existing destination keeps its exact bytes. A failed delete request for the owned plaintext temporary file is still not reported; it is finding 1A-D2-09 (unit 3).
- **Section 6, field.** The declared sequence uses the existing `broken_at` field and its decimal-string IPC field `brokenAt`, not a new `problem_declared_seq` field. Every damaged record's sequence is unverified, so the CLI and the Activity view now both say "declared sequence N (not verified)". The IPC shape does not change.
- **Section 7, checks.** The release-CLI checks for sections 5 and 6 ran against the final `akm.exe`, between two matching hash runs. The cases that need an injected fault (a partial stdout write, a failed completion append) are unit tests of the CLI functions, not release-binary checks.

## 9. Design review findings and changes

| Finding (design review) | Change |
|---|---|
| 1. Registration does not guarantee delivery | Section 2: emit failure becomes a warning; status is the replayable truth; poll and health bound stated. |
| 2. A resolved gate can outlive its root | Section 2: root object, dispatch check, guard before `acceptNativeStatus`, failed and no-owner cases. |
| 3. A delayed preview can be rebound to session B | Section 3: the lease is bound to the owner, ticket and presentation, and checked under the gate. |
| 4. A bounded cancelled set can resurrect a preview | Section 3: one current lease; everything else is refused; no eviction. |
| 5. Cancellation across failure and lifecycle | Section 3: unmount, expiry and page load clear the lease; a failed close locks the vault. |
| 6. Confirm/cancel order | Section 3: `take_preview` is the linearization point; both orders defined. |
| 7. Clipboard sequence 0 and reuse | Section 4: 0 is never proof; the decision table; the wrap limit. |
| 8. Partial stdout disclosure | Section 5: partial-output message. |
| 9. Rules too universal | Section 1: rules 3, 4 and 7 narrowed; receipts and warnings exempt. |
| 10. Proof not bound to the final build | Section 7: hash, check, hash again; mock-runtime dispatch test. |
| 11. Duplicate and out-of-range `seq` | Section 6: visitor counts names; range and form defined. |
| 12. "could not be recorded" overstates | Section 5: "could not be confirmed". |
| 13. Pairing and destination metadata | Section 5: operation ID; no destination. |
