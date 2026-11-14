# Unit 1c: session events and import reviews

- Status: implemented on branch `phase2/units-1c-to-4`. Review 2 found P2R2-02 and P2R2-03. The owner decided P2R2-02 ("the native dialog is the review"); decisions 4, 5, 9, 10 and 11 below record the result. No review follows these two fixes: the fail-first tests and mutants named below are their proof, and the owner's own testing is the next gate. The owner may veto any decision under "Decisions for the owner's veto".
- Scope: R4-02, 1A-R5-01, 1A-R5-03, 1A-R5-05, 1A-A2-01 ([FINDINGS.md](../FINDINGS.md)). Inputs: [unit 1a protocol](unit1a-protocol.md) sections 2 and 3 and its two design reviews ([review 1](../reviews/2026-11-01-unit1a-design-review.md), [review 2](../reviews/2026-11-01-unit1a-design-review2.md)).
- Public API change: yes. `env_import_preview` and `env_import_confirm` are removed. One IPC command, `env_import`, replaces them. `vault_dismiss_warnings` and `vault_warnings` gain four record kinds. No KDBX, journal-file or CLI change.

## 1. The design question

Unit 1a proposed a two-step protocol: the frontend asks for a preview, receives a token, and later confirms or cancels it. Both design reviews rejected that shape. Every rule they raised (cancel on edit, cancel on hide, cancel on reload, cancel that fails, a late preview, a stale token that removes a newer one) was a rule about **state that lives between two calls**. A token that can be abandoned needs a revoke on every path, and a revoke can itself fail or hang.

The ROADMAP suggests removing that state. This unit does so.

**Decision: one native command, one native confirmation, no pending state.**

- The frontend sends the pasted text, the project, the environment, the session id and an operation id in one call: `env_import`.
- Native code checks the session, parses the text and computes the plan against the vault snapshot it just read.
- Native code shows a system dialog (`tauri-plugin-dialog`) that lists the entry names, the target scope, and which existing secrets would be replaced. It never shows secret values.
- If the user approves, the same command re-reads the vault, requires the same plan and the same snapshot, and applies it. If anything changed, it refuses with `stalePreview`.
- If the user declines, nothing is written. The command answers `null`.

No token exists. There is nothing to cancel, edit or expire. The dialog is the only authority, and it belongs to native code, so the webview cannot confirm on the user's behalf. After the user answers, a lock, another session or a change of the vault since the preview still refuses the write. A hide, a page load or a reload does not: the user's click stands (decision 9).

### What the user sees

1. In Settings, the user chooses "Import .env…", pastes text, and fills project and environment.
2. The button reads "Review import". On click, the form locks (all fields, Cancel, Close and Escape are disabled) and a note says "Waiting for your answer in the AKM confirmation window. Nothing is saved until you confirm there."
3. A native window opens over the main window. Its text is:

   ```
   Import into project "demo", environment "dev"?

   Add 2:
     OPENAI_API_KEY
     STRIPE_KEY

   Replace the secret of 1:
     GITHUB_TOKEN (entry "GitHub")

   Keep unchanged: 3 (not listed).

   Secret values are not shown.
   ```

   Buttons: Import, Cancel. The window uses the warning style when a secret is replaced.
4. On Import, the form closes and the page shows "Imported: N added, M updated." On Cancel, the form stays open with the text intact and the note "Nothing was imported. You declined it in the confirmation window."
5. A lock closes the form at once. A window hide or a view expiry also closes the form, because the page discards its views. The native dialog is modal over the main window and stays until the user answers it. If the user clicks Import, native code applies the import unless a lock, another session or a vault change since the preview has refused it (the check runs under the session gate, at the write). The page shows the true outcome: a success notice when the import was applied, an outcome notice for a refusal. The page never shows a success that did not happen, and never an error for an import that did.

### Rejected options

| Option | Why it was rejected |
|---|---|
| Two-step lease with revoke on each path (unit 1a revision 2) | Rejected twice by design review. Each path needs a native revoke that can fail or hang, and the revoke itself needs an acknowledgment protocol. |
| Cancel floor (monotone "everything before N is cancelled") | Removes the stale-token-consumes-new problem but keeps a pending token and a cancel call that can fail. |
| Stateless digest (frontend holds a signed plan) | The webview could replay the plan after the user left. It also needs a key and an expiry, which are new state. |
| In-page confirm with the old preview | The webview is the thing contract 7 must not trust; a script in the page could click Confirm. |

## 2. Contracts

- **Confirmation is native and per request.** The session id is checked before any dialog. A request for an older or unknown session fails with `locked` and shows no dialog.
- **Every request that acts on the open vault names its session (P2R2-03, fault class B).** The 18 commands that read or change the vault, or renew its idle deadline (`vault_activity` is one of them), take the session id that the page read when it made the request. Native refuses a request for a session that is not the current one with `locked`, so a late request cannot act on a vault that opened after it. `view_discard` is the exception in its answer: for a session that is not current it does nothing and replies with success (decision 10). A structural test lists every command and the `sessionScoped` set in `native.ts` (the set moved there from `api.ts` in unit 2 fix round 1).
- **The write is bound to the reviewed snapshot, not to the presentation.** The write access checks, under the session gate, that the session and the vault revision are those of the preview. The plan that the user saw is compared with a fresh plan inside the same access. A mismatch is `stalePreview`. The presentation counter is not part of the binding, so a hide, a page load or a discard does not refuse an approved import.
- **At most 25 changes.** A larger plan fails with `tooLarge` before any dialog, so the dialog always lists every changed name. A larger import is split by the user.
- **Page load bumps the presentation.** `on_page_load` with `Started` calls `document_started()`. This revokes the protected views of the old page (a reveal, an export preview). It does not refuse an import that the user approved in the dialog (decision 9).
- **Outcome ledger (contract 7, "dispatched confirmation keeps its outcome").** Every request that passes validation gets a native record keyed by its operation id: `importApplied`, `importSessionEnded` (committed while the session ended), `importFailed` or `importDeclined`. Records hold no names, no scope and no values. The frontend folds a record into the waiting receipt, so a lost reply still ends as a true outcome. A reply that does arrive acknowledges its own record. A transport failure leaves the record in place.
- **Wording of PROJECT.md contract 7.** The old text spoke of "pending reviews" and "tokens", which describe the removed design. Contract 7 now says: the desktop import is a native confirmation that lists the exact project, environment and names; the user's click is the approval; hide, reload, page load and a discard do not cancel it; a lock, another session or a vault change since the preview refuses it, checked under the session gate at the write; a dispatched import keeps a native outcome record.

## 3. Session events (R4-02, 1A-R5-01)

**Registration order.** `App` registers `vault-locked` and `vault-view-expired` first. It requests the first status only after both registrations resolve (rule 1 of unit 1a).

**Session identity.** Each native vault opening has an id. Status replies and lock events carry it. `nativeSession.ts` keeps the current owner and a tombstone set. A lock event for an older id is ignored. A lock event for the current id always locks, even while a status call is in flight. A status for a tombstoned id is rejected.

**Failure cases and their result**

| Case | Result |
|---|---|
| Old session A emits lock while B opens (before status, during status, after status) | Ignored. B stays unlocked. |
| Real lock of B at the same points | B locks. |
| Lock emitted between status computation and listener registration | Not possible to miss: the status is read after registration, so the lock event or the later status shows it. |
| Either registration rejects | Locked view, `lockCurrent` (status, then an owned native lock), blocking notice "AKM could not watch the session. Restart AKM." |
| Either registration never resolves | Same, after a 3 s deadline. |
| Native lock cannot be confirmed after a failure | The notice adds "Could not confirm the native lock. Close AKM before leaving this computer." |
| React root replaced during registration or status | A per-effect `disposed` flag removes late listeners and drops the late status. |
| Status polling stops being accepted | After 9 s without an accepted status, an unlocked view locks natively. |

**Stated bound (best effort).** Registration deadline 3 s, native-call deadline 3 s, status poll 3 s, health limit 9 s. These use `setTimeout` and `setInterval`. A browser may delay timers in a throttled or frozen page, so the bound is "about 9 s while the webview runs timers". It is not a hard guarantee. A frozen webview is covered by the native idle timer, which this unit does not change.

## 4. Saved vault path (1A-A2-01)

`settings.rs` now has a `PreferenceStore` trait. The production store writes the settings file under a mutex with a **fencing ticket**:

1. Take the lock.
2. Refuse if the ticket is not newer than the newest ticket already saved, or if the opening no longer owns the session (`session.ensure(ticket)` fails).
3. Write the file. Only then record the ticket.

An older opening that pauses after its ownership check cannot overwrite a newer opening's path, because the newer opening's ticket is already recorded. A failed write does not advance the fence. A refused save is silent: the older opening is no longer the one the user sees, so it has no path to report.

Tests drive the production `open_vault` command with a paused opening A and a completed opening B, and an isolated fake store. `ipc_dispatch` also checks that the production handler list writes the saved path to a fake settings file.

## 5. Decisions for the owner's veto

1. **Import is confirmed in a native window, not in the page.** The user answers in a system dialog. The page has no Confirm button. Veto effect: go back to a two-step flow, which two design reviews rejected.
2. **The dialog lists names and counts, never values.** It lists entry names, the scope, and which secrets are replaced. It does not show titles of unchanged entries. Veto effect: add detail in `import_review.rs` message.
3. **25 changes at most per import.** Larger imports fail before any dialog and the user splits them. The cap keeps every changed name visible in the dialog. Veto effect: raise `MAX_REVIEWED_CHANGES` and let the dialog scroll, or list only counts.
4. **A reload does not cancel an open import dialog's effect (changed by decision 9).** The page-load hook still bumps the presentation, which revokes the old page's protected views. The dialog stays on screen until answered, and the user's answer decides the import. Veto effect: make a reload refuse the import again, as before decision 9.
5. **The page cannot be edited or closed while the dialog is open.** This keeps the page and the dialog about the same text. A lock, a hide and a view expiry still close the form. After a hide, the page shows the result of the import as a notice (decision 9). Veto effect: allow Cancel and show the late outcome as a notice.
6. **Native outcome records for imports.** Four new `OperationNotice` kinds hold no private data. They make a lost reply a true outcome. Veto effect: show uncertainty ("unconfirmed") instead.
7. **The saved path is guarded by a fencing ticket plus ownership.** Veto effect: serialize openings instead, which blocks a second open for the length of the first.
8. **Fail closed on registration failure.** AKM asks for a restart instead of retrying. Veto effect: add a bounded retry that keeps the locked view.
9. **The native dialog is the review (P2R2-02, owner decision).**
   - *Decision.* The user's click in the native dialog stands, even when the page was hidden, discarded its form or loaded again, and even when the window was hidden when the dialog returned. After the dialog returns, native refuses only for three reasons: the session was locked, the session changed, or the vault changed since the preview snapshot. All three are checked under the session gate at the moment of the write: the session and revision of the preview must still be current, and a fresh plan must equal the plan the user was shown. The post-dialog `window_live` refusal and the presentation check are gone, so a presentation expiry (hide, page load, discard) no longer refuses an approved import.
   - *Reason.* The user answered a question about exact names and a scope in a window that the page cannot script. A hide that happens while the dialog is open does not change that answer. Refusing it produced a refusal that the user had not asked for, after the user had read the names and clicked Import. The page now tells the truth in both directions: a success when the import happened, an error when it was refused.
   - *Kept before the dialog.* (1) No dialog while the main window is not shown (`window_live`): a dialog over a window that nobody sees would ask a question that nobody can place. (2) No dialog if the session or the revision changed after the preview was read: the question would be about a vault that no longer exists. These checks only decide whether to ask. They do not decide the answer.
   - *Truthful outcome after a discarded form.* The command still returns its reply, and it records `importApplied`, `importSessionEnded`, `importFailed` or `importDeclined` under the operation id (section 2). A page that discarded its form shows the result as a notice, from the reply or from the record. It never shows an undo, and never an error for an import that was applied.
   - *Rejected: "keep: a hide cancels, with a sequence number".* Each hide would carry a sequence number, and a hide after the preview would cancel the import even when the user approved. It would keep the refusal that decision 9 removes. It needs a sequence number in the page, in native code and in the reply, and the page can still lose a hide. The owner chose the simpler rule.
   - *Limits.* The check and the write run in one access under one gate hold, so a lock cannot land between them. A lock that lands after the write commits and before the reply reaches the page gives `completedWhileLocked` with the record `importSessionEnded`: the import happened, and the page says so without details. A user may approve an import after the window was hidden for a long time, because the dialog stays on screen; the dialog itself is the visible proof that the user acted. Real-window behavior (a real Windows hide while the real dialog is open) is not proved here. Only the owner can prove it (AGENTS.md safety rules).
   - *Veto effect:* go back to a presentation-bound write, which makes a hide refuse the approval.
10. **A discard names its session, and a request names its session (P2R2-03, fault class B).**
   - *Decision.* `view_discard` takes the session id that the page read when it handled the hide. Native does nothing for a session that is not the current one, and replies with success. A discard that fails (no notice delivered) locks only the session it names (`lock_owned`), never the current one. The expiry, the notice and the lock decision run under the session gate. The page sends no discard when it has no native session.
   - *Reason.* A discard that arrives late must not expire the views of, or lock, a vault that opened after it. The same fault class (a capability used outside the source that issued it) applies to every command that acts on the open vault, so the id is required for all of them (section 2).
   - *Sweep.* The 18 commands that read or change the vault or renew its idle deadline now take the id. `vault_activity` no longer renews the deadline of a session it does not name, so a late activity of an old page cannot keep a new vault awake. The structural test `p2r2_03_every_command_that_acts_on_the_vault_names_its_session` fails for a new command that omits the id or ignores it.
   - *Public API change.* IPC commands: `view_discard` and the 18 commands above gain a `sessionId` argument. The page adds it in `api.ts`. A request with an empty or unknown id is refused with `locked`.
   - *Limits.* A request that names the current session but arrives long after the page last acted still acts, as before: the id binds a request to a session, not to a moment. Idle time stays the job of the native idle timer.
11. **Residual points that stay open.** (a) The export file dialog (`env_save`) is not bound to the presentation; it checks the ticket after its save dialog and the revision in the read. This is unchanged and the owner has not been asked about it. (b) `tools/verify_windows_app.ps1` still reflects the old design (section 6).

## 6. Limits

- Timers are best effort (section 3).
- A hidden document discards the page's views (review 1, finding 1). The page sends `view_discard`, with the session id it read at that moment, when `document.hidden` turns true, from one listener in `App` for every view. Native expires the presentation of that session, as a minimize does, so a reveal, a preview or an export result of the discarded view can no longer apply. Under decision 9 this does not refuse an approved import. If the call is refused for any reason other than `locked`, the page locks the session it named. Native asks `ImportConfirm::window_live` (main window shown and not minimized) only before it shows the dialog, so no dialog opens over a window that nobody sees: that gives `viewChanged` and writes nothing. Before the dialog it also re-reads the snapshot, so a lock or a write that landed after the preview read shows no dialog. A modal dialog cannot be shown under the session gate, so a lock in the last instant before it opens can still show the names on screen. The write stays refused.
- The dialog and `view_discard` are separate calls. If the user approves, the import commits unless a lock, another session or a vault change got there first, whether or not the page hid, and the page shows the true outcome (decision 9).
- Not proved here: a real Windows hide that raises `document.hidden` without a native minimize. The tests use a mocked `document.hidden` and a scripted native boundary. That needs the owner present.
- The dialog is a native window. A test cannot click it in CI, so tests use a scripted confirm. A real dialog needs an owner-present proof (OS-level test in AGENTS.md safety rules).
- `tools/verify_windows_app.ps1` still clicks "Confirm import". It predates this design and is not run in this unit. Recorded as 1C-W-01 in FINDINGS.md.
- The permanent regression tests for 1A-R5-03 and 1A-R5-05 call `env_import`. They cannot run on the old code (the command does not exist). The defects were reproduced on the old code before the change, and mutants of the new code show that the tests catch the same class (FINDINGS.md lists them).
