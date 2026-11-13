# Unit 1: verified findings and remaining rows

- Status: unit 1 of the campaign to the release gate ([gate-campaign.md](gate-campaign.md)). Not yet reviewed.
- Scope: the verification of FR-03, FR-04 and FR-14 (stage D), and the remaining rows of the unit (stage E). The commit point and the outcome records are in [commit-point.md](commit-point.md).

## 1. FR-03, native approval before values leave the vault (`d9c541c`)

Read, tested, attacked and broken with mutants. One defect was found and fixed: the single-key budget belonged to a session.

### 1.1 The questions of the brief

1. **Can the page change, reset or exhaust the budget?**
   - Change: no. The count (5) and the window (60 seconds) are native constants, and the clock is native. No command sets them.
   - Reset: before this unit, yes. A lock and a new opening cleared the log of the budget. The page types the password, so a page that knows it could lock and unlock to start a new budget. Now the log belongs to the run of AKM. A lock and a new opening leave it as it is. Test: `fr03_an_approved_single_key_counts_against_the_budget_and_a_new_session_does_not_reset_it`.
   - Exhaust: yes, and this unit accepts it. A page can spend the free requests, so the next request of the user asks in a native window. That costs the user one click and discloses nothing. A request is counted when it is admitted, before the read, so a request for a key that does not exist also spends one.
2. **Is the approval bound to its scope, key and revision, and used once?** The window names the key that the vault holds at the session and the revision of the request. The read after the approval is bound to the same session and revision, so a write or another session between the window and the read refuses the read. The approval is not stored: each request over the budget asks again (the budget test asserts it). A single key has no scope. The bulk window is bound in the same way (`fr03_a_change_while_the_window_is_open_refuses_the_disclosure`).
3. **Can the page make the window say something false?** The body is native text, and its count is the native constant. The key title comes from the vault and is shown by `review_names`, which shows a hidden character as an escape (class K). One limit stays: a page can rename a key, so the window shows the title that the page gave it. The window still discloses that one key only.
4. **Does a decline leave no value anywhere?** Yes. A decline returns before the read, so the value never leaves the vault: no copy in memory, no journal record, no reply value and no clipboard write. Only the title was read.
5. **Does a lock while the window is open refuse the disclosure?** Yes. `disclosure::ask` checks the session after the window, and the read is bound to the session as well. Tests: `fr03_a_lock_while_a_single_key_window_is_open_discloses_nothing`, `fr03_a_lock_while_the_window_is_open_discloses_nothing`.

### 1.2 Mutants

Each mutant is one change to the code. The named test must fail on it. All of them failed.

| Mutant | Test that fails |
|---|---|
| A lock clears the budget | `fr03_an_approved_single_key_counts_against_the_budget_and_a_new_session_does_not_reset_it` |
| Every single request is free | `fr03_single_keys_are_free_up_to_the_budget_then_need_the_user` |
| An approved request is not counted | `fr03_an_approved_request_fills_the_rolling_minute_too` |
| A declined single key is still read | `fr03_an_approved_single_key_counts_against_the_budget_and_a_new_session_does_not_reset_it` and the other single-key tests |
| No session check after the window | `fr03_a_new_session_while_the_window_is_open_answers_locked_and_discloses_nothing` |
| A hidden window still gets the review | `fr03_a_hidden_window_shows_no_dialog_and_discloses_nothing` |

The check after the window first survived its mutant: every caller reads the vault through an access that is bound to the session, so a lock is refused there as well. Class A requires the check after a modal window, so it stays. The new test shows what it adds: a new session opened while the window was open ends the request as `locked`, not as `stalePreview`.

### 1.3 Tests that changed

- The earlier budget test asserted that a lock clears the budget. Its premise is the defect, so `fr03_an_approved_single_key_counts_against_the_budget_and_a_new_session_does_not_reset_it` replaces it.
- The FR-14 check of `ipc_dispatch.rs` runs after the FR-03 check, which spends the budget. Before it asserts that a damaged journal still blocks a reveal, it now lets a budget window approve, so that the journal block itself must refuse the reveal (`journal`).

## 2. FR-04, a plaintext remnant whose delete is not confirmed (`3964946`)

Read, tested and broken with mutants. The delete was already failed at the sync, validation and publication steps and at a cancel. This unit adds the write step: `fr04_a_refused_delete_after_a_failed_write_names_the_remnant`. Part of the plaintext reaches the file, the write fails, the delete is refused, and the warning names the remnant and holds no value.

The warning reaches the page twice: in the error of the failed export, and as an application warning that outlives a lock (`fr04_a_failed_export_with_a_remnant_names_it_in_the_error_and_in_the_warnings`). The warning names the path of the remnant by design, because the user must find the file and delete it. It holds no value.

### 2.1 Mutants

Each mutant is one change to the code. The named test must fail on it. All of them failed.

| Mutant | Test that fails |
|---|---|
| A refused delete is not recorded | `fr04_a_refused_delete_after_a_failed_write_names_the_remnant` |
| The remnant gives no application warning | `fr04_a_failed_export_with_a_remnant_names_it_in_the_error_and_in_the_warnings` |

## 3. FR-14, a new journal after damage (`2d8e013`, core `cd7e754`)

Read, tested, attacked and broken with mutants.

- A page can call `audit_recover`, but the recovery asks in a native window first. A structure test holds the order (`fr14_the_command_is_registered_and_takes_the_session_and_the_native_window`).
- A decline, a lock while the window is open, a failed window and a hidden window move nothing (`fr14_a_decline_moves_nothing_and_disclosure_stays_blocked`, `fr14_a_lock_while_the_window_is_open_moves_nothing`, `fr14_a_window_that_fails_is_an_error_and_moves_nothing`, `fr14_a_hidden_window_shows_no_dialog_and_moves_nothing`).
- A hide while the window is open does not revoke the approval (P2R2-02, `fr14_a_hide_while_the_window_is_open_does_not_revoke_what_the_user_approved`).
- Defect fixed in stage B: a lock after the journal moved answered `locked`, which said that nothing moved. It now answers `completedWhileLocked` (`fr14_a_lock_after_the_journal_moved_answers_completed_while_locked`).
- The core keeps the damaged bytes, starts the new journal with the gap, and leaves the damaged journal in place when a step fails (`fr14_a_damaged_journal_is_kept_byte_for_byte_and_a_new_one_starts_with_the_gap`, `fr14_a_failure_at_each_step_leaves_the_damaged_journal_in_place_and_blocked`). These core tests pass unchanged.

### 3.1 Mutants

Each mutant is one change to the code. The named test must fail on it. All of them failed.

| Mutant | Test that fails |
|---|---|
| The recovery skips the native review | `fr14_a_decline_moves_nothing_and_disclosure_stays_blocked` and the other `fr14_` tests |
| The recovery answers the plain error after the journal moved | `fr14_a_lock_after_the_journal_moved_answers_completed_while_locked` |

## 4. T-R3-02, a reserved variable reaches the terminal

The core reserves `GIT_EDITOR` at the base of this unit (`ReservedClass::HelperProgram`). This unit adds the test through the production launch boundary: `t_r3_02_every_reserved_class_and_every_shell_name_is_refused_at_the_launch_boundary`. It calls `run_launch` with a fake vault. The vault holds one key whose variable is reserved. The test has a table of names for each reserved class. The match on the class has no wildcard, so a new class in the core does not compile until the table has a row for it. Some names are in lower or mixed case. For each name, the launch answers `invalid` before any dialog: nothing is journaled, no value is copied and no terminal is made. The error holds no value. Two registry tokens and an ordinary name still start a terminal.

The shell rows hold the names that the command prompt reads and can print (`PROMPT`, `DIRCMD`, `COPYCMD`) and the names that PowerShell and the POSIX shells read at start. The desktop had its own list of the three command prompt names. The core class `ShellStartup` holds them, and the table asserts that, so this unit deletes the desktop list.

### 4.1 Mutants

| Mutant | Test that fails |
|---|---|
| The core forgets `GIT_EDITOR` (the affected version) | `t_r3_02_every_reserved_class_and_every_shell_name_is_refused_at_the_launch_boundary` |
| The core forgets `PROMPT` | `a_variable_that_the_command_prompt_prints_is_refused_before_any_dialog` |
| The launch skips its own reserved check | `a_list_of_names_is_checked_for_the_same_faults_as_the_command_line_checks` |

The third mutant does not change what the launch boundary shows. The core refuses a reserved name when it plans the scope, before the launch checks the names. The launch keeps its own check as a second guard, as the command line does, and its own test catches the mutant.

## 5. T-R1-08, a custom field name in the history of a foreign entry

The fix of T-R1-08 shows only the known KDBX field names. Any other name shows as "Other". The unit test `a_custom_field_name_in_the_history_never_reaches_the_page` holds this for an entry that AKM wrote. This unit adds a check through IPC in `tests/ipc_dispatch.rs`, for an entry that another writer made. The check writes a KDBX file with the community `keepass` writer, not with AKM. The file holds one entry with one history entry. The entry has a custom field whose name is fake secret text. Between the history entry and the current entry, the password, the custom field and the title change, and a second custom field is added. The check opens the file through `vault_prepare_open` and `vault_unlock` and reads `key_history`. The reply has one change. It says that the secret changed, and it names the changed fields `Password`, `Title` and `Other`. The reply holds no fake text: no value and no custom field name.

### 5.1 Mutants

| Mutant | Test that fails |
|---|---|
| The history shows every field name (the affected version) | The IPC check of the foreign entry |

## 6. T-R1-12, copies of a value that are not zeroed

The desktop no longer uses `std::process::Command`. It builds the environment block of the terminal itself (`process.rs`, `EnvironmentBlock`). The block is allocated once at its final size, so no reallocation leaves a copy behind. A debug assertion checks the final length. The block is zeroed when it is dropped. The values of the launch are zeroed when the launch ends (`LaunchSecrets`). `the_block_holds_the_selected_variables_and_no_removed_name_and_ends_in_two_zeros` tests the content of the block.

Limits that remain:

- No test observes the zeroing. A test would have to read freed memory.
- The command line still uses `std::process::Command` (`crates/akm-cli/src/child.rs`), which keeps copies that are not zeroed. That limit stays for the command line.
- The terminal holds its own copy of each value until the window closes. That is the purpose of the launch.
- This unit did not audit the copies inside the core and inside the `keepass` crate.

## 7. P2R2-06, tests that depend on the machine

The four desktop tests of the row compare canonical paths (`test_support::canonical`):

- `complete_export_record_with_failed_sync_keeps_publication_and_late_lock_receipt`;
- `overwriting_an_existing_file_keeps_the_export_private`;
- `publication_moves_the_owned_file_after_its_temp_name_is_replaced`;
- `native_presentation_expiry_preserves_approved_publication_and_its_truthful_completion`.

The first and the last open a vault and write its journal. Each now uses a journal root inside its own temporary folder (`test_support::own_root`), so it does not share the Cargo root with other tests. The two export tests write no journal.

Fail first: the mutant runner ran each test with `TEMP` and `TMP` set to a folder whose path holds a short name (8.3) on this machine. On that path, each mutant that puts back one raw comparison makes its test fail.

### 7.1 Mutants

| Mutant | Test that fails |
|---|---|
| The export receipt test compares raw paths | `complete_export_record_with_failed_sync_keeps_publication_and_late_lock_receipt` |
| The publication test compares raw paths | `publication_moves_the_owned_file_after_its_temp_name_is_replaced` |
| The overwrite test compares raw paths | `overwriting_an_existing_file_keeps_the_export_private` |
| The presentation test compares raw paths | `native_presentation_expiry_preserves_approved_publication_and_its_truthful_completion` |

Limit: the other desktop tests still journal in the root that `.cargo/config.toml` sets. The row names these four only.

## 8. T-R3-07 and T-R2-07, statements that contradict the design

The decided design (owner's option (a)): the `run-requested` record is written before any value leaves the vault. A value is read inside the vault earlier, to validate it. A lock during the record is prompt, and the record can then exist for a launch that never started.

Fixed in the files of this unit:

- `unit4.md` said that a notification is handed over inside `Session::while_session`. That function is gone. The note now names `Session::commit`.
- `unit1h.md` said that a lock waits for the gate behind a copy. The copy acts outside the gate, so the note now says that it does not.

A search of `desktop/src-tauri` and `docs/design` finds no other statement that a record comes before any value is read, or that a lock waits for process creation, except in the text that describes the old design as old.

Stale statements outside the files of this unit:

- `crates/akm-core/src/vault.rs`, the comment of the function that journals a planned scope, says "before any value is read". It should say "before any value leaves the vault".
- The verification guide (`.agents/skills/verify-akm/features/terminal-and-history.md`) already gives the decided design. The T-R2-07 row is stale.
- The FINDINGS rows T-R1-01, T-R1-05, T-R1-06, T-R1-07, T-R1-12, T-R2-07, T-R3-02 and T-R3-07 describe earlier states. The report of this unit proposes new text for each.
- A search of `docs/STATE.md` for gate and record statements finds no contradiction at the base of this unit.

## 9. Class Q, a wait that a loaded run exceeds

The full desktop run failed in two tests at stage E, and in other tests at stages B to D. Each failed test passes alone in about 6 seconds. Each failure was a wait for an event that must happen, with a bound of 20 seconds. Each test vault that `Vault::create` makes uses the production key derivation (Argon2id, 64 MiB, 4 lanes), so parallel tests slow each other. The full run takes about 12 minutes on this machine.

Class Q: a wait for an event that must happen, with a bound that a loaded run exceeds. Guard: every such wait passes one shared bound, `PATIENCE` (120 seconds, in `test_support.rs`, and the same value in `tests/ipc_dispatch.rs`). The bound changes only how long a broken run waits before it fails. Each assertion stays as it was. A wait that proves that an event is prompt, or that it does not happen, keeps its own short bound. Test: `class_q_every_wait_for_an_event_that_must_happen_uses_the_shared_patience` reads the test code of every source file and of the integration tests. It fails on a timed receive that unwraps or expects its event with another bound, and on a deadline with another bound.

All instances: 32 waits in the library tests (commands, session, launch, reminders and process) and 14 in `tests/ipc_dispatch.rs`. Two of the 32 wait for a test process to end (`child.wait`). They use `PATIENCE` too, but the guard test does not read that form. The waits that keep their own bound prove promptness or absence. The production wait in `windows_session.rs` is not a test.

### 9.1 Mutants

| Mutant | Test that fails |
|---|---|
| A deadline keeps its own bound of 20 seconds | `class_q_every_wait_for_an_event_that_must_happen_uses_the_shared_patience` |
| An IPC wait keeps its own bound of 60 seconds | `class_q_every_wait_for_an_event_that_must_happen_uses_the_shared_patience` |

Limit: a wait that proves promptness can still fail on a machine that is slow enough. Those bounds are 300 milliseconds to 10 seconds, for an act that takes milliseconds alone.

## 10. Class sweeps

Each sweep reads every instance in the production code of `desktop/src-tauri/src`, before its test module. The line numbers are at the stage F commit.

### 10.1 Classes A and P, the gate and each access function

- `Held::new` has one production caller, in `Session::commit_point` (`session.rs:786`). It runs after `bound` checked the binding under the gate (`session.rs:785`). Every act that takes `&Held` runs there: the vault replacement (the core `CommitGate`), the terminal resume (`launch.rs:472`), the rename of the saved path (`commands.rs:207`) and the notification hand-over (`reminders.rs:248`).
- The other 26 functions of `session.rs` that take the gate read or change native state. None of them acts outside AKM, except the lock notice, which the lock itself sends.
- `Session::access` (`session.rs:1029`) and `Session::access_reviewed` (`session.rs:1053`) both run `access_checked` (`session.rs:1062`). Under the gate it checks that the stored vault belongs to the current session. With an expected snapshot, it also checks the ticket, the revision and, when bound, the presentation (`session.rs:1074` to `1095`). A write replaces the file only through the `VaultCommit` that the access installs.
- After each native window, the caller checks the session again: the folder window (`launch.rs:427` and `429`), the launch review (`launch.rs:443`, then the reviewed access at `launch.rs:456`), the disclosure review (`disclosure.rs:168` and `178`), the save window (`commands.rs:995`) and the import review (`commands.rs:1223`).
- The copy, the publication and the reveal stay outside the commit point (decisions 3 and 4, inventory rows 6 to 8). The copy checks before and after its act (`session.rs:1216` and `1230`). The reveal checks and hands the value back under one gate hold (`session.rs:1233`).

### 10.2 Class B, each binding is checked where it is used

- `bound` checks each binding of a commit (`session.rs:800`): the ticket, the named session, or the ticket and the revision.
- A stamped value carries its snapshot. The reveal (`session.rs:1236` to `1241`) and the copy (`session.rs:1205` to `1215`) check its ticket and its presentation.
- The status read of an opening passes no snapshot (`commands.rs:190`). Its caller checks the ticket of the result where it uses it (`commands.rs:200`).
- An operation record names the session where the request started (`session.rs:1292`). The class N test shows that a record of session A changes nothing in session B.

### 10.3 Class C, no state moves before its record is durable

- The revision moves in `commit_point` only after the act succeeded (`session.rs:786` to `789`). The act is the file replacement.
- The fence of the saved path moves only after the rename succeeded (`settings.rs:114`).
- An operation record is written when the guard drops, after the act. `done` follows the commit (`launch.rs:474`).
- The terminal values leave the vault only after the `run-requested` record is durable: the record at `launch.rs:497`, then the commit that copies the values at `launch.rs:503`.

### 10.4 A hostile value gives a controlled error

- Production code has two `expect` calls, in `GateGuard` (`session.rs:224` and `230`). They state that the guard lives until its drop. No input reaches them. `clipboard_fake.rs` and `test_support.rs` compile for tests only (`lib.rs:2` and `21`).
- Each slice index has a guard before it: `launch.rs:289` after the length check at `launch.rs:283`; `clipboard.rs:857`, whose end is a position in the same slice; `private_export.rs:20` to `29` after the length check; `private_export.rs:602`, `637` and `645` after `validate_destination` checked the drive form (`private_export.rs:100` and `117`, called at `private_export.rs:281`).
- A poisoned mutex gives a fixed error: the gate (`session.rs:508`), the vault (`session.rs:1069`) and the settings fence (`settings.rs:102`).
- A panic in a native step gives a fixed error (stage F, below). A panic in a review window gives a fixed error too (`review_window.rs:93`).

### 10.5 Error texts that reach the page

- Fixed in stage F: a native step that panicked passed its panic message to the page. On the code before the fix, the page got `task 17 panicked with message "C:\\fake-folder\\fake-secret-value"`. `blocking` (`commands.rs:327`) now maps a failed step to a fixed text, and it is the one place that starts a blocking thread. Test: `a_panic_in_a_native_step_reaches_the_page_as_a_fixed_text`. It also counts the blocking threads in the production code of `commands.rs`.
- Texts that keep a path, by decision 16: `notFound` (`error.rs:41`), the FR-04 remnant (`private_export.rs:196`) and `journal` (`error.rs:50`).
- Texts from the operating system: `io` errors carry the message of the system (`error.rs:51`, `private_export.rs:14`, `settings.rs` and `reminders.rs`). The standard library puts no path in it. A `save` error carries the text of the KDBX library about the file structure (`error.rs:46`). A failed folder pick carries the text of the dialog plugin (`launch.rs:180`).
- No error text holds a secret value. The tests of FR-04, T-R1-08, T-R3-02 and class N assert that no fake value is in the reply.

### 10.6 Mutants

| Mutant | Test that fails |
|---|---|
| A panic text reaches the page (the code before stage F) | `a_panic_in_a_native_step_reaches_the_page_as_a_fixed_text` |
| The opening starts its own blocking thread | `a_panic_in_a_native_step_reaches_the_page_as_a_fixed_text` |
