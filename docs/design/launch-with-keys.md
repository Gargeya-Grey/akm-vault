# Launch a terminal with keys, and read a key's history

- Status: implemented on the branch for the key cabinet UI. Not yet reviewed.
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
   - more names than the review window lists (`MAX_REVIEWED_CHANGES`, 25). It never cuts the list short.
4. It checks that the main window is shown. A hidden window gets no dialog.
5. A native folder picker. Cancel returns `null`, with no record. A network folder (UNC) is refused, because a console cannot start there and would show a folder it is not in.
6. The native review window lists the project, the environment, the folder and every variable name. It says that the terminal keeps the keys until the user closes it, even after AKM locks. Decline returns `null`, with no record.
7. One more vault access, bound to the session and the revision of step 2. A lock, another session or any write since then refuses it. Inside it, in this order:
   1. `disclose_env` with `EnvDisclosure::Run` journals `run-requested` ("launch requested; N keys selected"). A journal failure ends the launch here. No value has been read.
   2. `env_vars` reads the values into a buffer that is zeroed on drop.
   3. `Session::while_current` runs the start with the `Held` witness. The launcher takes the witness, so a start after the gate is released does not compile.
8. The launcher opens the system command prompt in a new console window. AKM does not wait for it and does not track it. The reply is the number of keys, or `null`.

### The process

- The program is `cmd.exe` in the folder from `GetSystemDirectoryW`. PATH and `COMSPEC` are never read.
- The working folder is the user's pick. The command has no argument.
- The child inherits the environment of AKM without any `AKM_*` name (case-insensitive), plus the selected variables. `prepare_command` holds the rule. A test compares its text with `child.rs`, so the two cannot drift apart. A selected name with the prefix never reaches the child.
- The journal detail holds a count. It holds no folder, name or value.

### Failure cases and their tests

All tests are in `desktop/src-tauri/src/launch.rs`, apart from the two IPC ones. The launcher is a fake. No test opens a window, a dialog or a process.

| # | Case | Test |
|---|---|---|
| 1 | Journal write fails before the disclosure | `case_1_a_journal_failure_before_the_disclosure_starts_nothing_and_reads_no_value` |
| 2 | Lock or change between the review and the start | `case_2_a_lock_or_a_change_while_the_window_is_open_refuses_the_launch`, `case_2b_the_start_runs_under_the_session_gate_so_a_lock_waits_for_it`, `a_lock_while_the_folder_picker_is_open_shows_no_review` |
| 3 | The user declines the review | `case_3_declining_in_the_review_records_nothing_and_starts_nothing` |
| 4 | The folder picker is cancelled | `case_4_cancelling_the_folder_picker_shows_no_review_and_records_nothing` |
| 5 | A reserved variable name | `case_5_a_reserved_variable_name_is_refused_before_any_dialog` |
| 6 | Empty scope, or too many names | `case_6_an_empty_scope_and_a_scope_too_big_to_list_are_refused_before_any_dialog` |
| 7 | Child environment | `case_7_the_child_environment_holds_the_selected_variables_and_no_akm_variable` |
| 8 | Stale session id | `case_8_a_stale_or_malformed_session_is_refused_without_a_dialog` |
| 9 | Review text | `case_9_the_review_lists_the_scope_the_folder_and_each_name_but_no_value` |
| 10 | IPC | `the_launch_command_takes_a_session_and_a_scope_and_nothing_that_names_a_program` and the launch section of `desktop/src-tauri/tests/ipc_dispatch.rs` |

A lock that arrives after the start does not turn into a retryable failure: `a_start_that_a_lock_overtook_is_reported_as_completed_not_as_locked`.

### Decisions

- **The journal write and the value read run under the vault access, not under the gate.** The `Held` witness is for an effect that is short and does not call back into the session. The vault is behind its own mutex, and the lock order is vault, then gate. So the gate cannot be held while the vault is read. The check and the act are still one step: the start runs under the gate, for the ticket that was reviewed. A lock between the journal write and the start leaves a `run-requested` record with no start, the same as a CLI start that fails.
- **No name comparison after the review.** Every change to the open vault raises its revision, and the revision is part of the binding. A second comparison of the names would never differ.
- **Handles.** Rust opens files without the inherit flag. Every direct `CreateFileW` call in the tree passes either a security record with `bInheritHandle` set to 0 or no record, so no file that AKM opens is inheritable. A test reads the flag on handles that the standard library opens. A release build is a GUI program, so its standard handles are null and the new console gets its own. A debug build started from a terminal passes that terminal's handles on.
- **The default button.** The dialog plugin has no way to choose the default button. The Windows task dialog selects the first button, which is "Open terminal". The wording of the review and its warning icon are the only guard.
- **Contract 11.** PROJECT.md says to release the vault before starting a child. The desktop session stays open while the terminal runs. This document does not change the contract. The owner decides (see the report of the unit).

## 2. The history

`key_history(session_id, id)` returns the data of `Vault::history`: for each earlier version its index, its time and the names of the fields that changed, and whether the secret changed. It never returns a value. It is a read bound to the presentation, so a late reply after a hide is refused natively as well.

The drawer of a key shows a text button, "Show earlier versions", in the History row, only when the key has earlier versions. The list sits below the row, times in mono. The button, the list and a pending request clear on every expiry that clears the secret. A reply is accepted only when the presentation counter is the one the request began in.

| Case | Test |
|---|---|
| Times and field names, no value | `the_history_command_returns_times_and_changed_field_names_and_never_a_value`, and `KeyDetail.history.test.tsx` |
| Cleared on expiry, late reply ignored | `KeyDetail.history.test.tsx` |
| IPC shape and stale session | the history section of `ipc_dispatch.rs` |

## 3. The page

`Open terminal with keys…` is a secondary button in "Take keys out". It is enabled only when a project and an environment are chosen and nothing else is running. It follows the request fence of "Save .env…": a hide, a scope change, a lock or an unmount makes a late reply show nothing. `api.ts` treats the command as a mutation that the user can decline. A reply that a session change overtook becomes a "completed, do not repeat" notice, because a terminal that opened cannot be taken back. The notice has the generic name "Operation".
