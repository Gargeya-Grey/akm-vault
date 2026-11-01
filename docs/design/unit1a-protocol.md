# Unit 1a: frontend–native state protocol

- Status: draft for review, 2026-11-01
- Owner decision, 2026-11-01: design first (option B in STATE.md), after the fifth unit 1a review ([round 5](../reviews/2026-11-01-unit1a-review5.md)).
- Scope: the six round-5 findings, 1A-R5-01 to 1A-R5-06 ([FINDINGS.md](../FINDINGS.md)), and the rules that stop the same class of defect. Nothing else in unit 1a changes.
- Public API change: yes. Two IPC commands change shape and one is added (section 3). The CLI gains one journal record (section 5). The journal file format does not change.

## 1. The rules

Every frontend–native exchange in the desktop app follows these rules. A reviewer checks each new or changed command against them.

1. **Subscribe, then read.** The frontend registers every native event listener it depends on before it sends the first request whose reply those events can make stale. A reply that started before the listeners were registered is discarded.
2. **A failed subscription fails closed.** If a listener for `vault-locked` or `vault-view-expired` cannot be registered, the frontend shows the locked state and attempts a native lock for the session it knows. It does not keep an unlocked view without the listener.
3. **Every reply carries its session.** A reply or event that can change what the user sees names the native session ID it belongs to. The frontend accepts it only for the current owner and generation. This rule exists today (R4-02); rules 1 and 2 close the gap before the listener exists.
4. **Native state is cancelled natively.** When the frontend abandons a review, a preview or a pending choice, it tells the native side. A native result that arrives after the cancellation is discarded by the native side, not only by the frontend.
5. **A rejected request never consumes someone else's state.** A stale or wrong token fails without removing the state that belongs to the current, valid request.
6. **Uncertain is not success.** When the native side cannot confirm a cleanup or an outcome, it reports uncertainty: it keeps a safe retry and records a warning. It never reports an unknown result as "done" or as "someone else owns it now".
7. **Each disclosure has a request and a known outcome.** Every path that writes secrets out (GUI and CLI) records `*-requested` before the disclosure and `*-completed` after it succeeds. If the completion record cannot be written, the user gets a warning that the disclosure happened and must not be repeated.

## 2. Lock before the listener (1A-R5-01)

**Today.** `App.tsx` starts `loadStatus()` in one effect and registers `vault-locked` in another. `listen()` is asynchronous. If the native side computes an unlocked status, then locks and emits `vault-locked` before registration finishes, the event is lost and the unlocked status is accepted (`nativeSession.ts` `acceptNativeStatus`). The 3-second status poll corrects the view later, but the view is wrong until then. A failed `vault-locked` subscription only sets an error message.

**Change.**

- A new module-level gate in `nativeSession.ts`: `nativeListenersReady(): Promise<void>`. The App's listener effect resolves it after both `listen()` calls have resolved. A new root creates a new gate; disposal of a root invalidates its gate.
- `loadStatus()` awaits the gate before it calls `api.status()`. A status request that started before the gate resolved is discarded by the existing `request` counter.
- A failed `vault-locked` or `vault-view-expired` subscription calls `lock(NATIVE_PROTOCOL_ERROR)` (rule 2). Today only the view-expiry failure does this.

**Failure cases the tests must cover.**

| Case | Expected |
|---|---|
| Registration is delayed; native locks after it computed an unlocked status | The stale status is discarded; the locked view shows. |
| Registration of `vault-locked` fails | The locked view shows, a native lock is attempted, and the error is shown. |
| A root is replaced while the old root's status request is in flight | The old reply cannot set the new root's state; the new root reads status only after its own listeners are ready. |
| A lock event arrives between the new root's registration and its first status reply | The lock wins; the reply is discarded. |

## 3. Import reviews (1A-R5-03, 1A-R5-05)

**Today.** `env_import_preview` stores one pending review natively and returns a token. Closing or editing the import in `Settings.tsx` clears only the frontend copy, so the native review stays confirmable. A preview that is still in flight when the dialog closes installs its token later. `PendingImports::take` removes the current review before it checks the token, so a stale token also destroys a newer valid review.

**Change.**

- `env_import_preview` gains a `requestId` argument (a UUID the frontend creates for each preview). The native side stores it with the pending review.
- New command `env_import_cancel { requestId }`. It removes the pending review if it has that request ID, and records the ID in a small bounded set of cancelled IDs (for example the last 16). It succeeds when nothing matches.
- `store_preview` refuses to install a review whose request ID is in the cancelled set. The preview reply then has the `stalePreview` error, and the frontend ignores it because the dialog is closed.
- `Settings.tsx` calls `env_import_cancel` when the user closes the import dialog, edits the text, project or environment after a preview, or when the view expires. A failed cancel call is shown as an error and the dialog stays closed; the native review still expires on lock, on a new vault open and on view expiry (`locked_gate`, `open_with` and `expire_presentation` in `session.rs` clear it today).
- `PendingImports::take` checks the token, ticket, revision and fingerprint first and removes the review only when all match (rule 5). A wrong token leaves the current review in place.
- A confirmation that was already dispatched keeps its outcome record (`sessionBoundary.ts`), unchanged.

**Failure cases.**

| Case | Expected |
|---|---|
| Preview, then close the dialog, then confirm with the old token | `stalePreview`; nothing is imported. |
| Preview, then edit the text, then confirm with the old token | `stalePreview`. |
| Preview in flight; dialog closes; preview reply arrives | The native side does not install it; the frontend shows nothing. |
| Review B is installed; a delayed confirm with old token A arrives | A fails; B stays and can be confirmed once. |
| Confirm dispatched, then the dialog closes | The confirm completes and its outcome record stays. |
| Cancel IPC fails | The error is shown; the review still expires on the next lock or view expiry. |

## 4. Clipboard cleanup uncertainty (1A-R5-02)

**Today.** `clear()` in `clipboard.rs` returns `Ok(false)`, which means "someone else owns the clipboard now", when the sequence number is unchanged but `matches_text` returns false. `matches_text` returns false both when the text differs and when `GetClipboardData` or `GlobalLock` fails. So a read failure drops the lease without a warning, and the fake secret can stay on the clipboard.

**Change.**

- `matches_text` returns a three-way result: `Same`, `Different` or `Unreadable`.
- `clear()` decides by sequence first, under the one clipboard lock:
  - sequence changed: another program wrote. Release the lease, do not clear. This keeps a newer user copy, including one with identical text.
  - sequence unchanged and `Same`: empty the clipboard, as today.
  - sequence unchanged and `Unreadable` or `Different`: **uncertain**. Keep the lease, return a new error kind `clipboardUncertain`, and let the episode handler in `session.rs` record a cleanup warning and schedule the normal retry. It never reports this as recovery.
- After a partial publication failure followed by a failed rollback, the same rule applies: the lease is kept and the episode gets its warning.

**Failure cases.** Unreadable text with an unchanged sequence on the normal expiry; the same during a partial-publication rollback; a newer user copy with identical text (sequence changed); a later retry that succeeds and settles the warning; a second episode while the first warning is still pending (its own warning ID, as in R4-03). All run against the fake backend. Real Win32 fault injection is not possible in tests; this limit is recorded.

## 5. CLI export completion record (1A-R5-04)

**Today.** `akm export` records `export-requested`, then writes the file or stdout, and records nothing after it. The desktop app records `export-completed`.

**Change.** After `export::write_new` or the stdout write returns `Ok`, and stdout is flushed, the CLI appends `export-completed` through the vault's `AuditLog` (`AuditLog::for_vault(path)`), with the key count and, for a file, the destination. The vault does not need to stay open. If the append fails, the CLI prints to stderr: "The export was written, but its completion could not be recorded in the activity log. Do not repeat the export." It exits 0, because the disclosure happened. A failed write keeps today's behaviour: no completion record, and a non-zero exit.

**Failure cases.** A file export records both records in order; a stdout export records both; a failed write records only the request; a failed completion append prints the warning and exits 0.

## 6. Declared sequence of a damaged record (1A-R5-06)

**Today.** When a record fails typed parsing (`audit.rs`), the snapshot keeps only the physical line. A declared `seq` in otherwise valid JSON is lost.

**Change.** When typed parsing fails, parse the line as generic JSON. If it is an object whose `seq` is a non-negative integer, store it as `problem_declared_seq: Option<u64>`. The CLI and the Activity view show it as "declares sequence N (not verified)". Nothing else from the record is shown. When `seq` is missing, not an integer, or the line is not JSON, the field stays empty. The IPC field is a decimal string, like the other sequence fields (1A-R2-06).

**Failure cases.** Valid JSON with `seq: 7` and a bad timestamp shows "declares sequence 7 (not verified)"; `seq` as a string, a float, a negative number or missing shows none; invalid JSON shows none; record contents never appear.

## 7. How the work is checked

- Each finding gets a regression test that fails on `cfeb1b0` and passes after the change (AGENTS.md). The frontend tests use the real `App`, `api.ts` and `nativeSession.ts` with a mocked native boundary. The native tests use the real `Session`, `PendingImports` and clipboard lease with fake backends. The CLI tests use the release CLI with fake vaults.
- The full check run in [TESTING.md](../TESTING.md), then a rebuild. The new build hashes replace the pinned ones in STATE.md.
- One fresh unit 1a review with the REVIEWING.md inputs, plus this design.
- R4-04 stays deferred. The new build changes the hashes that the R4-04 proof uses.

## 8. Limits

- The 3-second status poll stays as a second defence. It is not the fix.
- Real Windows clipboard faults and real event-delivery timing are not testable here; the fake backends and mocked boundary are the evidence.
- A webview reload loses document-level state, as today (1A-R1-03 and related limits).
