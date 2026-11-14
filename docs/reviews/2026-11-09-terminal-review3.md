# Terminal launch and key history, review 3

- Model: Codex GPT-6.1 Terra, effort max.
- Reviewed commit: `20e3621` on `fix/terminal-rework`, against `f76bd5a`.
- Build hashes: the reviewer did not check a build.
- Date: 2026-11-09.
- Result under the pass rule: **fail** (6 P2, 1 P3). This is the third failed review of this work unit, so the stop rule applies (see STATE.md).

The text below is the reviewer's, except that links to the reviewer's local checkout are written as repository paths, and the reviewer's closing note about its temporary files is removed.

---

**Source acceptance: NO. Work unit complete: NO.** The review found six P2 issues at `20e3621`, including two gaps in the changed import flow. The real picker, review window and console remain owner-present tests.

## Ranked findings

1. **P2: Process creation can delay a lock.** `desktop/src-tauri/src/launch.rs:429` holds the session gate while `open_terminal` prepares and spawns `cmd.exe`. A stalled process creation, for example under an antivirus hook, makes lock wait. The existing test at `launch.rs:924` explicitly expects that wait. The design document names this exception, but the supplied owner decision says a lock is always prompt and does not name it as an accepted limit. **Found by inspection.**

   **Closing test:** Hold the production start seam before process creation completes, request lock, and prove the agreed prompt-lock and no-start-after-lock behavior. This needs a design decision if both guarantees cannot be provided by the present mechanism.

2. **P2: A selected key can become an executable Git command.** `desktop/src-tauri/src/launch.rs:277` relies on `reserved_class` plus three command-prompt names. `GIT_EDITOR` is valid and unreserved, so its value reaches the terminal. When the user later runs `git commit`, Git executes that value as an editor command with the other selected keys in its environment. **Partially reproduced:** a windowless fake Git process, with a fake `GIT_EDITOR`, executed its command and created a marker. I did not launch AKM.

   **Closing test:** Put a fake `GIT_EDITOR` and fake token in a scoped vault, exercise the production launch boundary, and verify the dangerous name is refused or otherwise cannot cause execution. Cover other program-control variables and shell shims by class.

3. **P2: A vault write can overtake the final launch check.** After values are copied and the vault mutex is released, `desktop/src-tauri/src/launch.rs:421` checks only the session ticket at start. A concurrent write can rotate or remove a selected key at `BeforeStart`; the ticket remains current and the terminal receives the old value. This contradicts the design's claim that a vault change between review and start refuses the launch. **Found by inspection.**

   **Closing test:** Pause at `BeforeStart`, complete a same-session rotation or removal, then resume. Assert that no terminal starts with the superseded selection. Also test a write already in progress at that boundary.

4. **P2: An approved import can commit after a lock that precedes the write.** `desktop/src-tauri/src/commands.rs:1015` enters `access_reviewed`, but `desktop/src-tauri/src/session.rs:786` releases the session gate before running its closure. A lock can complete after the entry check and before `vault.import_env`; the import can then commit after lock. This path predates the diff, but is a caller in the changed import-review flow and contradicts contract 7. **Found by inspection.**

   **Closing test:** Place a barrier after the write-access session check and before the import write. Lock, resume, reopen the fake vault, and assert that the import was not applied and its outcome is truthful.

5. **P2: A dispatched import can still lose its native outcome.** At `desktop/src-tauri/src/commands.rs:990`, `ensure_window_shown(confirm)?` bypasses the new `refused` recorder. If the main window is hidden at that check and the reply is lost, no native record tells the page the import stopped. The new window-check test keeps `window_live` true and locks during the probe, so it takes a different path. This leaves **T-R2-05** unresolved. **Found by inspection.**

   **Closing test:** After a valid preview, return false from `window_live`, drop the IPC reply, and assert an `importFailed` record for that operation with no vault change.

6. **P2: Reload can erase the only terminal completion notice.** `desktop/src/api.ts:56` creates the terminal receipt only after the JavaScript invocation receives success. If the page reloads while the native review is open, native work can continue because page load changes presentation rather than the session. A successful launch then has no surviving JavaScript receipt or native terminal outcome. The journal's `run-requested` cannot establish that a terminal started. **Found by inspection.**

   **Closing test:** Dispatch a fake launch, destroy or reload the page before its reply, approve the native review, and verify that the replacement page receives a terminal-specific, dismissible outcome without affecting a newer session.

7. **P3: Project records contradict the implemented and owner-approved design.** `docs/STATE.md:15` still says the contract 11 decision is pending, and its next action still asks for the option already chosen. `docs/FINDINGS.md:319` says the review and journal run under the gate. The older core documentation at `crates/akm-core/src/vault.rs:807` also retains the disproved "before any value is read" claim. **Found by inspection.**

   **Closing test:** Reconcile these records with the decision and implementation, then check their semantic claims as well as running `check_docs.py`.

## Earlier findings and state-table evidence

I checked every requested earlier finding. These are assessments by inspection, not independent red/green test runs.

- **T-R1-01:** The three named prompt variables are refused. The broader executable-variable gap is finding 2.
- **T-R1-02:** Production supplies `/d`. A controlled AutoRun fixture remains unrun.
- **T-R1-03:** The review warns about inherited environment and folder lookup. The specified unrelated-parent-secret scenario was not independently exercised.
- **T-R1-04 and T-R2-03:** The formatter escapes the inspected collision and control-character cases, and the tests cover their generated text. Actual Windows rendering and full visibility were not checked.
- **T-R1-05 and T-R2-02:** A names-only review after lock is the owner's accepted limit. The later launch and import paths recheck session ownership, subject to finding 4's separate import-write race.
- **T-R1-06 and T-R2-01:** The journal write is outside the gate and the value copy is gated. Process creation still holds the gate, as in finding 1.
- **T-R1-07:** The vault mutex is released before start, consistent with amended contract 11. Actual child handle inheritance was not independently tested.
- **T-R1-08:** The mapper allowlists field names. Its test constructs a `HistoryVersion`; the requested foreign-KDBX-to-IPC regression is still missing.
- **T-R1-09 and T-R2-04:** A delivered late success produces a terminal-specific, generation-owned notice. Finding 6 covers a lost reply across page reload.
- **T-R1-10:** The Windows config sets Cancel as default, with a test of the actual config. Real Enter and Escape behavior remains unrun.
- **T-R1-11 and T-R2-07:** The feature guide now describes the narrower journal ordering. The conflicting core comment remains in finding 7.
- **T-R1-12:** The `Command` buffer copy remains an open P3 assigned to the release review.
- **T-R2-05:** The named lock gap is recorded, but the hidden-window path remains open as finding 5.
- **T-R2-06:** Failed dialog results map to an error, while Cancel and Escape map to decline, by inspection.

The state-table test walks the `LaunchStep` hooks and checks review, journal and launcher observations. It uses reaching `ValuesCopied` as a proxy for copying; it does not independently observe every secret read or prove zeroization after cancellation. I did not establish that every table assertion fails against its corresponding broken implementation.

## Checks and limits

The checkout remained clean and detached at `20e3621`. The only runtime probe was the isolated fake Git editor test. I did not run project tests or build AKM. The implementer's `f558a03` check results are reported evidence, not checks I reran on the merge commit. **No build hash was checked.**

I did not open the GUI, picker, native review or a visible console, or touch the clipboard, app settings or OS session. Real dialog layout, `.cmd` and folder quoting through the production terminal, actual inherited handles, controlled AutoRun, foreign-KDBX history through IPC, and the owner-present workflows remain unverified.
