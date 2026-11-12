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
