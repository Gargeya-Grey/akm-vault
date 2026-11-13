# The commit point

- Status: unit 1 of the campaign to the release gate ([gate-campaign.md](gate-campaign.md)). Not yet reviewed. The owner may veto any decision under "Decisions for the owner's veto".
- Scope: fault classes A and P in `desktop/src-tauri`, finding T-R3-04 (a vault write that finishes after a lock), the terminal start of T-R3-01, and the sweep of every other session-scoped effect.
- Public API change: one hook in `akm-core` (`CommitGate`, section 3.3). The command line does not install it, so its behaviour does not change. Stage B adds the required argument `operationId` to `env_launch_terminal` and four record kinds (section 9). The reply of each command does not change.

## 1. The rule

A session-scoped effect has three phases.

1. **Prepare.** Do all the slow work outside the session gate: the key derivation, the temporary file, the suspended process. Nothing that a user or another program can see happens yet, or what happens can be undone.
2. **Commit.** Take the gate. Check that the session (and, where it is bound, the vault revision) is still the one that the request was made for. Then do one short act: replace the vault file, resume the process, rename the settings file, hand the toast to Windows. Release the gate.
3. **Revoke.** When the check refuses or the act fails, undo the preparation after the gate is released: delete the temporary file, terminate the suspended process.

`Session::commit` is the one function that does phase 2. It is the only code that makes a `Held` witness, and every act takes `&Held`. So code that checks first and acts after the gate is released has nothing to pass, and does not compile.

## 2. Inventory

Every effect in `desktop/src-tauri/src` that acts for a session. "Before" is the tree at `955e3c5`. "After" is this unit.

| # | Effect | Where | Binding | Before | After |
|---|---|---|---|---|---|
| 1 | Replace the vault file (every write: add, update, rotate, remove, restore, purge, import, upgrade) | `session.rs` `access_checked`; core `Vault::commit` | Session and revision | The session was checked at the start of the access and at its end. The file was replaced between them with no gate (T-R3-04). | The core calls the installed `CommitGate` just before the replacement. The gate runs the replacement in `Session::commit`. The revision moves at the commit, under the gate. |
| 2 | Start the terminal | `launch.rs` `run_launch` | Session and revision that the user reviewed | `while_current` held the gate while `CreateProcessW` ran, so a lock waited for process creation. | The process is created suspended, outside the gate (`process.rs`). `Session::commit` checks and resumes it. A refusal terminates it. |
| 3 | Copy the launch values out of the vault | `launch.rs` `disclose_for_launch` | Session and revision | `while_current` around `env_vars`. | `Session::commit` around `env_vars`. Nothing to revoke: the copy is in memory and is zeroed on drop. |
| 4 | Save the last vault path | `settings.rs` `save_vault_path`, called from `commands.rs` `open_vault` | The ticket of the opening | `while_current` around a whole write: folder creation, temporary file, rename. | The temporary file is written outside the gate. `Session::commit` renames it. After a refusal, a failed rename or a panic, the drop of the prepared file deletes it. |
| 5 | Show a notification | `reminders.rs` `notify` | The session that the page names | `while_session` around a file read, a permission query and the hand-over. | The file read and the permission query run outside the gate. `Session::commit` does the hand-over only. |
| 6 | Copy to the clipboard | `session.rs` `copy_value` | Session, revision and presentation | Check, act outside the gate, check again (unit 1h, D1). The checks did not bind the revision, and a refusal at the second check took the copy back only after a lock. | Both checks bind the revision (U1-R1-01). Every refusal at the second check takes the published copy back (U1-R1-02). See decision 3. |
| 7 | Publish an exported file | `session.rs` `publish`, `commands.rs` `env_save` | Session and revision | Check, prepare, check, publish outside the gate. Contract 9: publication may finish after a lock and reports it. | Not changed. See decision 4. |
| 8 | Reveal a value to the page | `session.rs` `reveal` | Session, revision and presentation | The check and the hand-back of the value share one gate hold. The check did not bind the revision. | The check also binds the revision (U1-R1-01). It has no outside effect: the value goes back in the reply. |
| 9 | Journal records (`run-requested`, export, recovery) | core `disclose_env`, `AuditLog` | Session, checked before | Outside the gate, because the journal takes a blocking file lock. | Not changed. Owner's option (a): a lock during the record is prompt, and the record can exist for an act that never happened. |
| 10 | Recover a damaged journal | `commands.rs` `run_recovery` | Session and revision | A read access. A lock after the recovery answered `locked`. | Not changed by the commit point. The rename of the journal runs with no gate. A lock after the rename answers `completedWhileLocked` (section 9.2). See limit 4. |

Effects that are not session-scoped, and so stay out of this mechanism: the reminder preference (`reminders.json`, `Reminders::set_enabled`), which is an application preference; vault creation, which happens before a session exists and keeps its `completedWhileLocked` receipt; and the lock notice, which the lock itself sends.

## 3. The mechanism

### 3.1 Types

- `Binding` names what the act is bound to. `Ticket(ticket)` is a session that a native step holds. `Named(id)` is the session that a page request names. `Revision(snapshot)` is a session and a vault revision.
- `Effect` is a prepared act. `act(&mut self, &Held) -> Result<Output, CommandError>` runs under the gate. `revoke(&mut self) -> Result<(), String>` runs after the gate is released, when the check refused or the act failed. A revoke error becomes an application warning with fixed text.
- `Session::commit(binding, &mut effect)` takes the gate, checks the binding, runs `act`, releases the gate, and revokes on any error.

### 3.2 The revision moves at the commit

Before this unit, the revision moved when a write access ended. A launch that committed between the file replacement and the end of that access saw the old revision, so it could start with values that the file no longer held. Now the vault replacement is itself a commit, and its act moves the revision under the same gate hold. A reload still moves the revision when its access ends: it changes what is in memory, not the file.

### 3.3 The core hook

`akm_core::vault::CommitGate` has one method, `commit(&self, replace)`. `Vault::commit` calls it after the last fingerprint check and the test hook `BeforeReplace`, and passes the replacement (the hard link of a new vault, or the rename) as `replace`. With no gate installed, the replacement runs as before, so the command line is unchanged.

The desktop installs a gate for each vault access (`VaultCommit`). A write or an upgrade gets a gate bound to the session and the revision at the start of the access. A read, a reload, and the stored vault between accesses get a closed gate, which refuses every replacement. So a write that does not go through a write access fails, and leaves the file as it was.

`VaultCommit` records whether a replacement happened and why one was refused. When the access ends:

| Session at the end | Replacement | Answer |
|---|---|---|
| Ended | Happened | `completedWhileLocked` |
| Ended | Did not happen | `locked`. The file is unchanged. |
| Current | Refused by a closed gate | `internal`. A programming error. |
| Current | Happened, or none was asked for | The result of the operation. |

### 3.4 Lock order

Vault mutex, then the lease cell, then the gate. The vault commit runs inside a vault access, so it holds the vault mutex and takes the gate: the order holds. A lock never waits for the vault mutex (`discard_locked_vault` uses `try_lock`), but it does wait for the gate, and so for one act in progress.

## 4. State table

The specification. Each row is a moment at which an event can land. The tests test it row by row.

Events: **lock** (an explicit lock, idle expiry, a Windows lock, suspend); **new session** (another unlock after a lock); **vault change** (another write of the same session); **hide** (a minimize or a hidden window); **reload** (a page load); **lost reply** (the reply does not reach the page); **stall in prepare**; **stall in commit**.

### 4.1 The vault write

| Moment | Lock or new session | Vault change | Hide or reload | Lost reply |
|---|---|---|---|---|
| Before the access starts | `locked`. Nothing written. | `stalePreview` for a reviewed write. A plain write runs on the new state. | No effect on a reviewed write. A presentation-bound write gets `stalePreview`. | Not applicable. |
| Prepared (temporary file, key derivation, backup) | The commit refuses. `locked`. The vault file is unchanged. The temporary file is removed. | Cannot happen: the vault mutex serializes writes. | No effect. | Not applicable. |
| Committed (file replaced) | `completedWhileLocked`. The file holds the change. The journal has the record, or a journal warning. | Cannot happen. | No effect. | The next status read shows the change. An import also leaves its record (unit 2 shows it). |
| Stall in prepare | A lock is prompt: no gate is held. The commit then refuses. | | | |
| Stall in commit (the rename is slow) | The lock waits for this one replacement. See limit 1. | | | |

### 4.2 The terminal

| Moment | Lock or new session | Vault change | Hide or reload | Lost reply |
|---|---|---|---|---|
| Before the review, during the review | As `launch-with-keys.md` section 1.1. | `stalePreview` at the access after the approval. | No effect after the review is open (P2R2-02). | The outcome record (section 9) says what happened. |
| Values copied, vault released | `locked`. The record exists. No process. | | | |
| Prepared (process exists, suspended) | The commit refuses. `locked`. The process is terminated before it ran one instruction of the shell. | The commit refuses. `stalePreview`. The process is terminated. | No effect. | |
| Committed (resumed) | The terminal runs. The answer is the count: the start happened. The outcome record says `terminalSessionEnded`. | The terminal runs with the values of the reviewed revision. A later write does not reach it. | No effect. | The terminal runs. The outcome record says `terminalStarted`, or `terminalSessionEnded` after a lock. |
| Stall in prepare (`CreateProcessW` is slow) | A lock is prompt: no gate is held. The commit then refuses, and the process is terminated. | Same: the commit refuses. | | |
| Stall in commit (`ResumeThread` is slow) | The lock waits for this one call. See limit 2. | | | |

### 4.3 The settings write and the notification

| Moment | Lock or new session | Vault change | Hide or reload | Lost reply |
|---|---|---|---|---|
| Prepared (settings: temporary file; notification: preference and permission read) | The commit refuses. Settings: the temporary file is deleted, the fence does not move, the answer is "superseded". Notification: nothing is shown, the answer is `locked`. | No effect: neither is bound to the revision. | No effect. | The settings write is a native step with no reply of its own. A lost notification reply loses only the delivery state. |
| Committed | The file is renamed, or the toast is handed to Windows. A lock after that changes nothing. | | | |
| Stall in prepare | A lock is prompt. | | | |
| Stall in commit | The lock waits for one rename or one hand-over. See limits 1 and 3. | | | |

## 5. Where the launch values live

1. The vault's database in memory, as long as the vault is open.
2. `LaunchSecrets`, from `env_vars` until the suspended process exists. It is zeroed on drop, and dropped before the commit, so no copy waits under the gate.
3. The environment block that `process.rs` builds for `CreateProcessW`. It is allocated once at its final size, so it never moves, and it is zeroed when `CreateProcessW` returns.
4. The environment of the child process. AKM cannot zero it. The review says so.

Gone with this unit: the copies in `std::process::Command` (its environment map and the block that it builds at spawn). They were never zeroed (T-R1-12).

## 6. Limits

1. A lock waits for one vault file replacement that is in progress. The replacement is one `rename` (or one hard link for a new vault). An antivirus or a cloud client can make it slow.
2. A lock waits for one `ResumeThread` that is in progress.
3. A lock waits for one notification hand-over to Windows that is in progress.
4. The recovery of a damaged journal renames files in the journal folder with no gate. The core hook in this unit is in the vault save path only. A lock during the rename is prompt. The answer after it is `completedWhileLocked`, because the journal moved.
5. Not verified: whether Windows shows the new console window of a suspended process before it is resumed. If it does, a refused start shows a console that closes at once. It needs the owner present to watch.
6. A clipboard take-back that fails (decision 3) is not tried again in the same call. After a lock, the next tick or lock tries again. After a revision or view change, the lease's own expiry tries again, at most 30 seconds later.

## 7. Tests

| Row or rule | Test |
|---|---|
| 4.1, a lock or a new session after the entry check of each kind of write (add, update, rotate, remove, restore, purge, import, save, upgrade) | `t_r3_04_a_lock_after_the_entry_check_writes_nothing_for_every_kind_of_write`, `a_new_session_while_a_write_waits_at_its_commit_point_refuses_the_write` |
| 4.1, a stall in prepare | `a_lock_during_the_slow_part_of_a_write_is_prompt_and_the_write_then_writes_nothing` |
| 3.2, the revision moves at the commit | `a_write_moves_the_revision_at_its_commit_so_an_act_bound_to_the_old_revision_refuses`, `every_kind_of_write_replaces_the_file_and_moves_the_revision_once` |
| 3.3, the closed gate | `a_replacement_outside_a_write_access_is_refused_and_writes_nothing` |
| 3.1, revoke after the gate, and only on refusal | `a_refused_commit_revokes_after_the_gate_is_released_and_a_done_one_never_revokes` |
| 3.1, the binding | `review1_a_session_effect_runs_only_for_the_current_owner`, `review1_a_session_effect_holds_the_gate_so_lock_and_a_new_opening_wait_for_it` |
| 4.2, every row, for a lock and for a new session | `a_lock_at_each_step_of_the_launch_has_the_outcome_of_the_state_table` |
| 4.2, a stall in prepare | `a_lock_while_the_terminal_is_created_is_prompt_and_the_terminal_never_starts` |
| 4.2, a vault change before the commit | `a_same_session_rotate_or_remove_at_the_last_step_before_the_start_refuses_it` |
| 4.2, a write in progress at the commit | `a_write_in_progress_at_the_start_is_ordered_by_the_gate_and_both_answers_are_true` |
| 4.2, the real Win32 calls | `a_resumed_child_writes_its_marker_and_a_terminated_child_never_does`, `a_suspended_child_that_is_dropped_is_terminated`, `a_real_child_gets_the_folder_and_the_selected_variables_and_no_akm_variable` |
| 5, the environment block | `the_block_holds_the_selected_variables_and_no_removed_name_and_ends_in_two_zeros`, `an_empty_block_is_two_zeros` |
| 4.3, the settings write | `a_ticket_that_no_longer_owns_the_session_writes_nothing`, `review1_2_a_retiring_opening_cannot_interleave_between_the_ownership_check_and_the_settings_write` |
| 4.3, the notification | `review1_7_lock_and_a_new_opening_wait_for_a_notification_that_passed_validation`, `review1_7_a_request_made_for_an_older_session_shows_nothing` |
| Inventory 6 and 8, the revision of the read | `u1_r1_01_a_value_that_left_the_vault_is_refused_after_a_same_session_change` |
| Decision 3, the take-back of a refused copy | `u1_r1_02_a_copy_refused_after_its_publication_is_taken_back_row_by_row` |
| 1, the structural guard | `every_session_scoped_act_goes_through_the_commit_point` |

The structural guard reads every file in `desktop/src-tauri/src` and the save path of the core. It is a tripwire for the usual spelling, not a proof: it matches source text, so a renamed import (`use std::process::Command as Child`), a re-export or a macro passes it (U1-R1-06). The compiler enforces only the `Held` witness. The guard fails when:

- a second place makes a `Held`, or the one place is not `commit_point`;
- a function that is not an act (`act`, `resume`, `show`) takes a `Held`;
- a file other than `process.rs` creates, resumes or terminates a process, or uses `std::process::Command`;
- a file other than `session.rs` installs a vault commit gate;
- the core replaces the vault file outside the `replace` closure that the gate runs.

### 7.1 Mutants

Each mutant is one change to the code. The named test must fail on it. All of them failed.

| Mutant | Test that fails |
|---|---|
| The vault commit skips the session and revision check | `t_r3_04_a_lock_after_the_entry_check_writes_nothing_for_every_kind_of_write` |
| A read access gets an open commit gate | `a_replacement_outside_a_write_access_is_refused_and_writes_nothing` |
| The revision moves at the end of the access, not at the commit | `a_write_moves_the_revision_at_its_commit_so_an_act_bound_to_the_old_revision_refuses` |
| The commit does not move the revision | `a_same_session_rotate_or_remove_at_the_last_step_before_the_start_refuses_it` |
| The start is bound to the ticket only, not to the revision | `a_same_session_rotate_or_remove_at_the_last_step_before_the_start_refuses_it` |
| The terminal is made under the gate | `a_lock_while_the_terminal_is_created_is_prompt_and_the_terminal_never_starts` |
| A refusal does not revoke | `a_refused_commit_revokes_after_the_gate_is_released_and_a_done_one_never_revokes` |
| The resume does not resume | `a_resumed_child_writes_its_marker_and_a_terminated_child_never_does` |
| The termination does not terminate | `a_resumed_child_writes_its_marker_and_a_terminated_child_never_does` |
| A dropped suspended process is not terminated | `a_suspended_child_that_is_dropped_is_terminated` |
| A selected variable does not replace an inherited one | `the_block_holds_the_selected_variables_and_no_removed_name_and_ends_in_two_zeros` |
| The prepared settings file is not deleted on drop | `a_ticket_that_no_longer_owns_the_session_writes_nothing` |
| A copy refused at the second check is not taken back | `u1_r1_02_a_copy_refused_after_its_publication_is_taken_back_row_by_row` (row "lock, TakeBackFails") |
| A second `Held::new()` in `session.rs` | `every_session_scoped_act_goes_through_the_commit_point` |
| A `Held::new()` in `launch.rs` | Does not compile: the constructor is private to `session.rs`. |
| A `std::process::Command` in `launch.rs` | `every_session_scoped_act_goes_through_the_commit_point` |
| The core renames the vault file outside the gate's closure | `every_session_scoped_act_goes_through_the_commit_point` |
| `set_commit_gate` in `launch.rs` | `every_session_scoped_act_goes_through_the_commit_point` |
| A function in `launch.rs` that takes a `Held` and is not an act | `every_session_scoped_act_goes_through_the_commit_point` |

## 8. Decisions for the owner's veto

1. **The revision moves at the commit, not when the access ends.** It removes the gap in which a launch could start with values that the file no longer holds.
2. **A closed gate on the stored vault.** Any replacement outside a write access is refused. It removes the promise that a future caller remembers to use `access`.
3. **The clipboard stays outside this mechanism.** Its act can wait for another program's clipboard lock, and unit 1h decision D1 keeps it outside the gate for that reason. It keeps its pattern: check, act, check again. Each check binds the session, the revision and the presentation of the read. When the second check refuses, the call takes the published copy back at once, with the ownership-aware cleanup: a newer copy of the user, also one with the same text, stays. That take-back is the one cleanup attempt of the call. If it fails, the reply has the cleanup's error kind and both texts. The state table is the doc comment of `copy_held`, and `u1_r1_02_a_copy_refused_after_its_publication_is_taken_back_row_by_row` tests it row by row. Before U1-R1-02, the take-back happened only when a lock owed a cleanup, so a copy refused for an expired view stayed on the clipboard.
4. **File export stays outside this mechanism.** Contract 9 lets a publication finish after a lock and report it truthfully. Moving the rename under the gate would change that contract, which this unit may not do.
5. **A write that returns success but replaced nothing answers `locked` when the session ended during it.** Before this unit, it answered `completedWhileLocked`. Nothing was written, so `locked` is the true answer.
6. **The terminal no longer inherits handles.** `CreateProcessW` runs with `bInheritHandles` false. The console gets its own standard handles.
7. **The outcome record starts at the session check.** A malformed operation ID or a stale session leaves no record: nothing ran, and the reply says so. Otherwise the record would have to name a session that is not the one of the request.
8. **No dedupe of operation IDs.** Each dispatch leaves its own record. A page that reuses an ID gets two records with that ID. Native code keeps no list of used IDs.
9. **The launch reply stays the count after a late lock.** The start happened, so the reply says so. The record says `terminalSessionEnded`. The reply shape does not change, as the contract with unit 2 requires.
10. **A recovery that moved the journal answers `completedWhileLocked` after a lock.** Before, it answered `locked`, which said that nothing moved.
11. **The FR-03 single-key budget belongs to the run of AKM, not to a session.** A lock and a new opening leave it as it is. Before, a lock cleared it, so a lock and a reopen gave a new budget. Veto effect: a budget per session, and the reopen path that resets it.
12. **The launch keeps its own check of reserved names.** The core refuses a reserved name first, so the check in `check_names` is a second guard, as in the command line. Veto effect: delete that branch and its own test. The launch boundary does not change.
13. **The desktop list of the command prompt names is deleted.** The core class `ShellStartup` holds `PROMPT`, `DIRCMD` and `COPYCMD`, and the table test asserts it. One list remains.
14. **One bound for every wait for an event that must happen, 120 seconds (class Q).** A broken run takes longer to fail. A loaded run no longer fails.
15. **A panic in a native step answers a fixed text that invites no retry.** The panic message can name a path or hold a value, so it never reaches the page. The text is "A native step failed, so AKM cannot say whether it finished. Check the result before you try again." Before U1-R1-04, the text was "A native step failed. Try again.", which invited a retry of a step that may have finished. A panic is caught at three places, so that what is known about the effect survives it (section 9.1): in the act under the gate, in the vault access, and in `dispatched`. A caught panic ends the session, because memory may no longer match the file or the process.
16. **Three error texts keep a path.** `notFound` names the vault path that the page sent. The FR-04 warning names the remnant, which the user must delete. A `journal` error names the journal file, which the user must act on. No error text holds a secret value. Veto effect: fixed texts, and the user no longer sees where to act.

## 9. An outcome for every dispatched request

Fault class N: a request whose effect outlives the page must leave a true record on every way out. The page can lose a reply: a page load, a crash of the webview, or a lock that the page handles first. The record is then the only truth about the request.

### 9.1 The rule

- `outcome::dispatched` runs `env_import` and `env_launch_terminal`. The page makes the operation ID, a UUID, for each request.
- A malformed operation ID is refused with `invalid`, and a session that is not the current one with `locked`. Both leave no record: nothing ran, and the reply says so.
- From the session check on, one guard (`Outcome`) records exactly once, in its drop. So a reply, an early return through `?` and a panic all leave one record. `dispatched` catches a panic in the request, so no panic unwinds past it. The default is "failed".
- The commit is marked where it happens. A vault write marks it in `VaultCommit` (`CommitLog::committed`), and the vault access catches a panic in its operation, so it always reads that mark. The launch marks it with `Outcome::done` right after the resume. A panic in the act itself, under the gate, is caught there: the gate is not poisoned, the effect is revoked, and the answer is the fixed text of decision 15.
- After the mark, no reply invites a retry. An error or a panic after a vault replacement, or after `done`, ends the session, and the reply is `completedWhileLocked`. The page then shows the locked screen, and the record says "session ended".
- `Session::record_operation` writes into a poisoned gate too, so a record is never skipped.
- The reply decides the kind. A value: applied or started. `null`: declined. `completedWhileLocked`: session ended. Any other error: failed.
- `Outcome::done` marks an effect that happened. The launch calls it right after the resume. A fault after that point records the start, and no later error turns the record into "failed".
- "Session ended" means that the effect happened and that the session of the request was no longer the current one when the record was written. The check is the same as a status read: it does not count as activity, and it changes nothing in the session that is current. The record holds the session of the request, so a record of session A never names session B.
- `vault_warnings` returns the record until `vault_dismiss_warnings` acknowledges it. The reply of each command does not change.

| Request | Effect happened | Session ended after it | User declined | Anything else |
|---|---|---|---|---|
| `env_import` | `importApplied` | `importSessionEnded` | `importDeclined` | `importFailed` |
| `env_launch_terminal` | `terminalStarted` | `terminalSessionEnded` | `terminalDeclined` | `terminalFailed` |

### 9.2 The other commands

- `env_save` has no operation ID, so it cannot leave a record. Its answers are true: `null` for a cancel, the path for a publication, `completedWhileLocked` for a publication that finished after a lock (contract 9), and an error that names any remnant. A lost reply loses only the answer: the file and the journal record exist. Proposal for unit 2: add `operationId` to `env_save` and give it the same guard.
- `audit_recover` moves the damaged journal outside the gate (limit 4). A lock after the move used to answer `locked`, which said that nothing moved. Now an error after the move answers `completedWhileLocked`.

### 9.3 Limits

6. A gate that some other code poisons still takes a record, but `vault_warnings` then refuses, as every other gate user does, so the page cannot read the record until AKM restarts. Since U1-R1-04 the act under the gate cannot poison it, and no other code under the gate is known to panic.
7. Records live in memory until the page acknowledges them or AKM quits. A record does not survive a restart.
8. A panic in an act under the gate (U1-R1-04) records "failed". For a terminal, the revoke has ended the process. For a vault replacement, the file may hold the change; the `importFailed` text says that a failure does not confirm that nothing was saved.

### 9.4 Tests

| Rule | Test |
|---|---|
| One record and one reply on each way out, for both operations: a value, `null`, an error, `?`, `completedWhileLocked`, a panic before and after `done`, an error after `done`, a lock after `done` | `class_n_every_way_out_of_a_dispatched_request_leaves_one_true_record` |
| A panic under the gate leaves a working gate; a poisoned gate still takes a record (U1-R1-04) | `u1_r1_04_a_panic_under_the_gate_leaves_a_working_gate_and_no_record_is_skipped` |
| An error or a panic after the vault replacement answers `completedWhileLocked` (U1-R1-04) | `u1_r1_04_an_access_that_fails_after_its_replacement_answers_completed_while_locked` |
| No record before the session check | `class_n_a_malformed_operation_or_another_session_runs_nothing_and_leaves_no_record` |
| A record of session A changes nothing in session B | `class_n_a_record_of_session_a_changes_nothing_in_session_b` |
| A hidden window at the import check leaves a failed record (T-R3-05) | `t_r3_05_a_window_that_is_not_shown_gets_no_dialog_and_leaves_a_failed_record` |
| A recovery that moved the journal answers `completedWhileLocked` after a lock | `fr14_a_lock_after_the_journal_moved_answers_completed_while_locked` |
| Through the production handlers: each way out of a launch with the reply dropped, a fault after the start and right after the resume, a page load while the review is open and then an approval, a hidden window at the window check, a lock at three steps, sessions A and B, a hidden window at the import check, and a fault right after the import's replacement | the outcome section of `desktop/src-tauri/tests/ipc_dispatch.rs` |

### 9.5 Mutants

Each mutant is one change to the code. The named test must fail on it. All of them failed.

| Mutant | Test that fails |
|---|---|
| The drop of the guard records nothing | `class_n_every_way_out_of_a_dispatched_request_leaves_one_true_record` |
| An error records "done" | `class_n_every_way_out_of_a_dispatched_request_leaves_one_true_record` |
| The launch does not mark the start after the resume | the outcome section of `ipc_dispatch.rs` (a panic at `Started`) |
| A late lock never gives "session ended" | `class_n_every_way_out_of_a_dispatched_request_leaves_one_true_record` |
| A failure leaves no record (the premise of T-R3-05) | `t_r3_05_a_window_that_is_not_shown_gets_no_dialog_and_leaves_a_failed_record` |
| The recovery answers the plain error after the journal moved | `fr14_a_lock_after_the_journal_moved_answers_completed_while_locked` |
| A malformed operation ID is accepted | `class_n_a_malformed_operation_or_another_session_runs_nothing_and_leaves_no_record` |
| The record names the current session, not the session of the request | `class_n_a_record_of_session_a_changes_nothing_in_session_b` |
| An error after the start turns the record into "failed" | `class_n_every_way_out_of_a_dispatched_request_leaves_one_true_record` |
| The vault access does not catch a panic in its operation | `u1_r1_04_an_access_that_fails_after_its_replacement_answers_completed_while_locked` |
| The commit point does not catch a panic in the act | `u1_r1_04_a_panic_under_the_gate_leaves_a_working_gate_and_no_record_is_skipped` |
| `dispatched` keeps an error reply after the effect | `class_n_every_way_out_of_a_dispatched_request_leaves_one_true_record` |
