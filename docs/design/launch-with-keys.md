# Launch a terminal with keys, and read a key's history

- Status: reworked on `fix/terminal-rework` after two failed reviews ([review 1](../reviews/2026-11-08-terminal-review1.md), [review 2](../reviews/2026-11-08-terminal-review2.md)). The owner chose option (a) of the stop-rule question on 2026-11-09. Section 1.1 is that design.
- Tier: `Tier 2: 3 units, risk: privacy, token, public API`. The units: the native launch, the history read, and the page.
- Goal: a user can open a terminal that already holds one project's keys, and can see a key's earlier versions, with no command line.
- Public API change: two new desktop IPC commands, `env_launch_terminal` and `key_history`. No change to `akm-core`, the vault, the journal format or the CLI.

## 1. The launch

The page names a session, a project and an environment. It cannot name a program, a folder, an argument or an environment variable. Native code owns the rest.

### Data flow

1. The page calls `env_launch_terminal` with `sessionId`, `project` and `environment`. The command has no other parameter, and a test holds that.
2. On a blocking thread, native code takes `snapshot_for(session_id)`. A stale or malformed id is refused here.
3. It plans the scope under the vault access of the request. The plan holds names only. It refuses, before any dialog:
   - a scope without both a project and an environment;
   - an empty scope;
   - a reserved variable name (the same `reserved_class` rule as `akm run`);
   - a variable that the command prompt reads and can print: `PROMPT`, `DIRCMD`, `COPYCMD`;
   - more names than the review window lists (`MAX_REVIEWED_CHANGES`, 25). It never cuts the list short.
4. It checks that the main window is shown. A hidden window gets no dialog.
5. A native folder picker. Cancel returns `null`, with no record. A network folder (UNC) is refused, because a console cannot start there and would show a folder it is not in.
6. The native review window lists the project, the environment, the folder and every variable name, in full and character-exact (section 1.2). It says that the terminal keeps the keys until the user closes it, even after AKM locks, that it gets the environment AKM started with, and that a program in the folder can run by name. Its default button is Cancel. Decline, Escape and close return `null`, with no record. A window that could not be shown is an error, never a decline.
7. One more vault access, bound to the session and the revision of step 2. A lock, another session or any write since then refuses it. Inside it, in this order: a last session check; `disclose_env` with `EnvDisclosure::Run`, which journals `run-requested` ("launch requested; N keys selected"), outside the gate; then a commit (`docs/design/commit-point.md`): under the gate, a check of the session and the revision, and `env_vars`, which copies the values into a buffer that is zeroed on drop. A journal failure ends the launch. No value has left the vault.
8. The vault access ends, so the vault is released. A last session check. The launcher makes the system command prompt, suspended, in a new console window, outside the gate (`process.rs`). AKM's copy of the values is zeroed when the process exists.
9. `Session::commit` checks the session and the revision of step 2 under the gate, and resumes the process. The resume takes the `Held` witness, so a start after the gate is released does not compile. A refusal terminates the process before it ran. AKM does not wait for a started terminal and does not track it. The reply is the number of keys, or `null`. The page also keeps a notice that a terminal holds keys, because a hidden view drops the reply.

### 1.1 Locks and the session gate (owner's option (a))

Reviews 1 and 2 pulled in opposite directions because a modal window and a lock cannot be one atomic step, and a journal write can stall for as long as another process holds its lock. The owner's decision of 2026-11-09:

- A lock is always prompt. A journal stall cannot delay a lock.
- A review that lists names only can appear just after a lock. It starts nothing and discloses nothing, even if the user approves it.
- A `run-requested` record can then exist for a launch that never started.

The rule that every step follows: **take the gate only to check the session and copy what is needed; do blocking work outside the gate; take the gate again and check the session before any effect that discloses or starts.** Nothing that can block runs under the gate. The start is one `ResumeThread` call under the gate.

| Step (`LaunchStep`) | What runs | What the gate covers | What a lock just before this step causes |
|---|---|---|---|
| Request | `snapshot_for(session_id)` | The check | `locked`. Nothing else happens. |
| Plan | `access_reviewed` reads the names | The check on entry and on exit, not the read | `locked`. No dialog. |
| `Planned` | Window check, then `ensure` | The check | `locked`. No picker. |
| Picker | The native folder picker, then `ensure` | The check after it. Not the picker | `locked`. No review. |
| `FolderChecked` | `checked_folder` on the pick | Nothing | `locked` at the next check. No review. |
| Last check | `ensure` | The check | `locked`. No review. |
| `LastCheckPassed` | The review window (modal) | Nothing. A window cannot be under the gate | **Accepted limit.** The names can appear. The window starts nothing. The next step refuses an approval. |
| `Approved` | `access_reviewed`, entry check | The check | `locked`. No record, no value, no start. |
| `RecordNext` | `disclose_env`: the journal write, which can block | Nothing (the vault mutex only) | **Accepted limit.** The record exists. No value leaves the vault. No start. The lock itself does not wait for the journal. |
| `Recorded` | A commit: session and revision check, then the value copy | The check and the in-memory copy | `locked`. The record exists. No value leaves the vault. No start. |
| `ValuesCopied` | The vault access ends: exit check | The check | `locked`. The values are zeroed. No start. |
| `BeforeStart` | `ensure`, then `CreateProcessW`, suspended | The check. Not the process creation | `locked`. No process is made. |
| `Prepared` | A commit: session and revision check, then `ResumeThread` | The check and the resume | `locked`. The suspended process is terminated before it ran. A write of the same session since step 2 gives `stalePreview`, with the same revoke. |
| `Started` | The reply | Nothing | The terminal is open and stays open. The reply is the count. The page keeps the terminal notice (section 3). |

The test `a_lock_at_each_step_of_the_launch_has_the_outcome_of_the_state_table` walks the rows that have a `LaunchStep`, once with a lock and once with a lock and a new opening. It uses the hook `TerminalHost::reached`, which runs at that exact point of the production path. The import uses the same pattern: its review is presented by the same function, and an approval after a lock is refused by the write access.

Limits that stay:

- A names-only review can be on screen after a lock. The window is not closed by the lock.
- A `run-requested` record can exist for a launch that never started.
- A lock waits for one `ResumeThread` call that is in progress. A slow `CreateProcessW`, for example under an antivirus hook, no longer delays a lock: it runs outside the gate.
- Not verified: whether Windows shows the console window of a suspended process before it is resumed. If it does, a refused start shows a console that closes at once. It needs the owner present to watch.
- While a journal write stalls, the decrypted vault stays in memory until that access ends. A lock is still prompt. The access discards the vault when it returns.

### 1.2 What the review shows

- A name is shown in full, or the launch or import is refused before any window, with `tooLarge`. It is never cut short.
- A character is shown as itself only if it is on an allowlist: printable ASCII, and letters and digits of any script, without the Hangul fillers. Every other character (controls, directional marks, invisible and format characters, variation selectors, emoji) is shown as `\u{...}`. An allowlist fails safe against characters that a denylist forgets.
- A backslash in a name is shown as `\\` and a quotation mark as `\"`. Together with the rule above, two different names always give two different texts.
- A space at the start or end of a name, or next to another space, is shown as `\u{20}`.
- A folder is shown with its backslashes as they are, because a Windows path is full of them. A path that holds the literal text `\u{` or that is not valid Unicode is refused before the review, so that a shown escape can only mean a real hidden character.
- One limit remains: a look-alike letter from another script is shown as that letter. AKM does not detect look-alikes.

### The process

- The program is `cmd.exe` in the folder from `GetSystemDirectoryW`. PATH and `COMSPEC` are never read.
- The working folder is the user's pick. The only argument is `/d`, which turns off the AutoRun commands of the registry. Those would run with the keys before the user types anything. A user's AutoRun settings do not apply in this terminal.
- The child inherits the environment of AKM without any `AKM_*` name (case-insensitive), plus the selected variables. A selected variable replaces an inherited one of the same name, in any case. `launch_environment` holds the rule. A test compares its text with `child.rs`, so the two cannot drift apart. A selected name with the prefix never reaches the child.
- The journal detail holds a count. It holds no folder, name or value.

### Failure cases and their tests

The tests are in `desktop/src-tauri/src/launch.rs` and `process.rs`, apart from the IPC ones. The launcher is a fake. No test opens a window or a dialog. Two tests start a real `cmd.exe` with no window (`CREATE_NO_WINDOW`). Each child writes a file and ends, or is terminated before it runs.

| # | Case | Test |
|---|---|---|
| 1 | Journal write fails before the disclosure | `case_1_a_journal_failure_before_the_disclosure_starts_nothing_and_hands_out_no_value` |
| 2 | Lock or change between the review and the start | `case_2_a_lock_or_a_change_while_the_window_is_open_refuses_the_launch`, `a_lock_while_the_terminal_is_created_is_prompt_and_the_terminal_never_starts`, `a_same_session_rotate_or_remove_at_the_last_step_before_the_start_refuses_it`, `a_write_in_progress_at_the_start_is_ordered_by_the_gate_and_both_answers_are_true`, `a_lock_while_the_folder_picker_is_open_shows_no_review`, `a_lock_after_the_folder_check_shows_no_review_of_the_old_session`, `a_lock_after_the_access_began_stops_the_record_and_the_values`, `a_lock_at_each_step_of_the_launch_has_the_outcome_of_the_state_table`, `a_lock_just_before_the_window_appears_starts_nothing_even_if_the_user_approves` |
| 2a | Journal stall against a lock | `a_held_journal_lock_does_not_delay_a_lock_and_the_launch_then_starts_nothing` |
| 3 | The user declines the review | `case_3_declining_in_the_review_records_nothing_and_starts_nothing` |
| 4 | The folder picker is cancelled | `case_4_cancelling_the_folder_picker_shows_no_review_and_records_nothing` |
| 5 | A reserved variable name | `case_5_a_reserved_variable_name_is_refused_before_any_dialog`, `a_variable_that_the_command_prompt_prints_is_refused_before_any_dialog` |
| 6 | Empty scope, or too many names | `case_6_an_empty_scope_and_a_scope_too_big_to_list_are_refused_before_any_dialog` |
| 7 | Child environment | `case_7_the_child_environment_holds_the_selected_variables_and_no_akm_variable`, `a_real_child_gets_the_folder_and_the_selected_variables_and_no_akm_variable`, `the_block_holds_the_selected_variables_and_no_removed_name_and_ends_in_two_zeros` |
| 7a | The suspended start | `a_resumed_child_writes_its_marker_and_a_terminated_child_never_does` |
| 8 | Stale session id | `case_8_a_stale_or_malformed_session_is_refused_without_a_dialog` |
| 9 | Review text | `case_9_the_review_lists_the_scope_the_folder_and_each_name_but_no_value`, `hostile_scope_text_cannot_pass_for_a_line_of_the_review`, `names_that_differ_only_after_80_characters_read_differently_in_both_reviews`, `a_real_control_character_and_its_literal_text_read_differently_in_both_reviews`, `two_different_names_never_give_the_same_review_text`, `a_name_too_long_to_show_in_full_is_refused_before_any_dialog`, `a_folder_that_a_review_could_not_show_exactly_is_refused_before_the_review` |
| 9a | The review window fails | `a_failed_review_window_is_an_error_and_starts_nothing`, `only_the_confirm_button_approves_and_escape_cancel_and_close_decline`, `the_real_window_config_has_cancel_as_its_default_button` |
| 10 | IPC | `the_launch_command_takes_a_session_and_a_scope_and_nothing_that_names_a_program` and the launch section of `desktop/src-tauri/tests/ipc_dispatch.rs` |

The vault is released before the start: `the_vault_is_released_before_the_terminal_starts`. No step can fail after the start, so a start is never reported as a failure.

### Decisions

- **The journal write runs outside the gate.** Section 1.1 gives the rule. The vault mutex stays held, and a lock never waits for that mutex: `discard_locked_vault` uses `try_lock`. `plan_env` reads each value inside the vault to validate it, also before the dialogs. No value leaves the vault before the record.
- **What the terminal inherits.** It keeps the normal environment that AKM started with, without `AKM_*`, because programs need it. The review says so. Clearing it would break `PATH` and every tool setting.
- **No name comparison after the review.** Every change to the open vault raises its revision, and the revision is part of the binding. A second comparison of the names would never differ.
- **Handles.** `CreateProcessW` runs with `bInheritHandles` set to false, so the terminal inherits no handle of AKM, and the new console gets its own standard handles. Rust also opens files without the inherit flag, and every direct `CreateFileW` call in the tree passes either a security record with `bInheritHandle` set to 0 or no record. A test reads the flag on handles that the standard library opens.
- **The default button and the outcomes.** The dialog plugin cannot choose a default button, and it reports Escape as the second button. So `review_window.rs` calls the Windows task dialog itself, with Cancel as the default. A click on the confirm button approves. Cancel, Escape and close decline. A failed `TaskDialogIndirect` call or a panic in the window code is `Failed`, an error for the caller: it is never a decline. `review_names` is the one function that shows a review. It takes a title, a body and a list of names, and answers approved, declined or failed. The next unit reuses it for bulk approval. The list is the last part of the text, so the warnings cannot be pushed off the screen by a long list.
- **Copies of the values.** `LaunchSecrets` is zeroed on drop, and is dropped before the commit. The environment block for `CreateProcessW` is allocated once at its final size and zeroed on drop. The launch no longer uses `std::process::Command`, whose copy was not zeroed. The child's own environment holds the values until the terminal closes. The CLI keeps the `Command` limit (finding T-R1-12). See `docs/design/commit-point.md`, section 5.
- **Contract 11.** The owner amended it for the desktop on 2026-11-09 ([PROJECT.md](../PROJECT.md)). The vault lock is released before the child starts, the child inherits no vault handle and no `AKM_*` variable, and the decrypted vault stays in AKM's memory while AKM is unlocked. The sentence for the CLI is unchanged.

## 2. The history

`key_history(session_id, id)` returns the data of `Vault::history`: for each earlier version its index, its time and the names of the fields that changed, and whether the secret changed. It never returns a value. It is a read bound to the presentation, so a late reply after a hide is refused natively as well.

The drawer of a key shows a text button, "Show earlier versions", in the History row, only when the key has earlier versions. The list sits below the row, times in mono. The button, the list and a pending request clear on every expiry that clears the secret. A reply is accepted only when the presentation counter is the one the request began in.

| Case | Test |
|---|---|
| Times and field names, no value | `the_history_command_returns_times_and_changed_field_names_and_never_a_value`, and `KeyDetail.history.test.tsx` |
| Cleared on expiry, late reply ignored | `KeyDetail.history.test.tsx` |
| IPC shape and stale session | the history section of `ipc_dispatch.rs` |

## 3. The page

`Open terminal with keys…` is a secondary button in "Take keys out". It is enabled only when a project and an environment are chosen and nothing else is running. It follows the request fence of "Save .env…": a hide, a scope change, a lock or an unmount makes a late reply show nothing. `api.ts` treats the command as a mutation that the user can decline. A reply that a session change overtook keeps a `terminal` outcome, so the retained notice says that a terminal holding keys opened and stays open after AKM locks. That outcome cannot invalidate a newer session.
