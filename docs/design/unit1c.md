# Unit 1c: session events and import reviews

- Status: implemented on branch `phase2/units-1c-to-4`. Not yet reviewed. The owner may veto any decision under "Decisions for the owner's veto".
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

No token exists. There is nothing to cancel, edit, expire, hide, reload or lock away. The dialog is the only authority, and it belongs to native code, so the webview cannot confirm on the user's behalf.

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
5. A lock, a window hide or a view expiry closes the form at once. The native dialog is modal over the main window and stays until the user answers it. Native code then finds the snapshot changed (lock, page reload and every write bump it) and refuses to write. The page shows an outcome notice, never a success.

### Rejected options

| Option | Why it was rejected |
|---|---|
| Two-step lease with revoke on each path (unit 1a revision 2) | Rejected twice by design review. Each path needs a native revoke that can fail or hang, and the revoke itself needs an acknowledgment protocol. |
| Cancel floor (monotone "everything before N is cancelled") | Removes the stale-token-consumes-new problem but keeps a pending token and a cancel call that can fail. |
| Stateless digest (frontend holds a signed plan) | The webview could replay the plan after the user left. It also needs a key and an expiry, which are new state. |
| In-page confirm with the old preview | The webview is the thing contract 7 must not trust; a script in the page could click Confirm. |

## 2. Contracts

- **Confirmation is native and per request.** The session id is checked before any dialog. A request for an older or unknown session fails with `locked` and shows no dialog.
- **The write is bound to the reviewed snapshot.** The plan that the user saw is compared with a fresh plan inside the write access. A mismatch is `stalePreview`.
- **At most 25 changes.** A larger plan fails with `tooLarge` before any dialog, so the dialog always lists every changed name. A larger import is split by the user.
- **Page load bumps the presentation.** `on_page_load` with `Started` calls `document_started()`. A reload while the dialog is open therefore invalidates the reviewed snapshot.
- **Outcome ledger (contract 7, "dispatched confirmation keeps its outcome").** Every request that passes validation gets a native record keyed by its operation id: `importApplied`, `importSessionEnded` (committed while the session ended), `importFailed` or `importDeclined`. Records hold no names, no scope and no values. The frontend folds a record into the waiting receipt, so a lost reply still ends as a true outcome. A reply that does arrive acknowledges its own record. A transport failure leaves the record in place.
- **Wording of PROJECT.md contract 7.** Its text speaks of "pending reviews" and "tokens". Those words describe the removed design. The owner should replace the sentence with: "Import review is a native confirmation that lists the exact project, environment and names. The write is bound to the reviewed vault snapshot. Edits, hide, reload and lock cannot confirm it. A dispatched import keeps a native outcome record." This unit does not edit PROJECT.md.

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
4. **Reload cancels an open import dialog's effect.** The page-load hook bumps the presentation. The dialog stays on screen until answered, but the import is refused afterwards. Veto effect: leave reload out and accept that a reload does not invalidate.
5. **The page cannot be edited or closed while the dialog is open.** This keeps the page and the dialog about the same text. Hide, view expiry and lock still close the form. Veto effect: allow Cancel and show the late outcome as a notice.
6. **Native outcome records for imports.** Four new `OperationNotice` kinds hold no private data. They make a lost reply a true outcome. Veto effect: show uncertainty ("unconfirmed") instead.
7. **The saved path is guarded by a fencing ticket plus ownership.** Veto effect: serialize openings instead, which blocks a second open for the length of the first.
8. **Fail closed on registration failure.** AKM asks for a restart instead of retrying. Veto effect: add a bounded retry that keeps the locked view.

## 6. Limits

- Timers are best effort (section 3).
- A hidden document no longer leaves the dialog live (review 1, finding 1). The page sends `view_discard` when `document.hidden` turns true, from one listener in `App` for every view. Native expires the presentation, as a minimize does. The import then fails with `stalePreview`, and the outcome record says `importFailed`. If the call is refused for any reason other than `locked`, the page locks the vault. Native also asks `ImportConfirm::window_live` (main window shown and not minimized) before it shows the dialog and again when the dialog returns. A dead window gives `viewChanged` and writes nothing. Before the dialog it also re-reads the snapshot, so a lock or a write that landed after the preview read shows no dialog. A modal dialog cannot be shown under the session gate, so a lock in the last instant before it opens can still show the names on screen. The write stays refused.
- The dialog and `view_discard` are separate calls. If the user approves while the window is visible and the page never hid, the import commits, and the page shows its outcome. That is the truthful outcome: the user approved it.
- Not proved here: a real Windows hide that raises `document.hidden` without a native minimize. The tests use a mocked `document.hidden` and a scripted native boundary. That needs the owner present.
- The dialog is a native window. A test cannot click it in CI, so tests use a scripted confirm. A real dialog needs an owner-present proof (OS-level test in AGENTS.md safety rules).
- `tools/verify_windows_app.ps1` still clicks "Confirm import". It predates this design and is not run in this unit. Recorded as 1C-W-01 in FINDINGS.md.
- The permanent regression tests for 1A-R5-03 and 1A-R5-05 call `env_import`. They cannot run on the old code (the command does not exist). The defects were reproduced on the old code before the change, and mutants of the new code show that the tests catch the same class (FINDINGS.md lists them).
