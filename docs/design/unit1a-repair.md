# Unit 1a repair design (after review round 8)

- Status: revision 3, 2026-11-01. Design review 1 rejected revision 1 ([record](../reviews/2026-11-01-unit1a-repair-design-review1.md)), and design review 2 rejected revision 2 ([record](../reviews/2026-11-01-unit1a-repair-design-review2.md)). After the second rejection the work stopped (AGENTS.md), and the owner chose the split below.
- Source read: application code at `44330ee` (branch `fix/unit1a-sound-findings`); the code is the same at `707042a`. Line numbers are at that commit.
- Inputs: [review round 8](../reviews/2026-11-01-unit1a-review8.md), the [advisory](../reviews/2026-11-01-unit1a-advisory.md), the open unit 1a findings in [FINDINGS.md](../FINDINGS.md), and both design reviews.
- Owner decisions, 2026-11-01:
  - Design first, in unit 1a.
  - The CLI default view shows a fixed mask, with no secret characters and no length.
  - Clipboard cleanup is best effort, and every clipboard text says that the copy may remain.
  - Lock rule, narrowed after design review 1: lock sends its notice first, then makes one cleanup attempt that never retries and never waits for a busy clipboard. Lock may wait for a copy in progress (a recorded limit). This rule is built in unit 1h, not in 1a (split, below).
  - **Split (after design review 2):** unit 1a closes 1A-R8-01 to 1A-R8-05 with narrow fixes, each proved through its production caller. The wider hardening that the design reviews found goes to a new unit 1h, with its own design and its own authorization (FINDINGS.md, rows 1H-01 to 1H-08). The 1a acceptance review judges 1a against this scope. There is no third design review.
- Public API changes:
  - The CLI default `get` output (the mask).
  - Inside the desktop crate: the `ClipboardBackend` trait (`set_private` returns a `Publication`).
  - The CLI gets a `run_with` function that `main` calls.
  - The IPC shapes, the KDBX format, the journal format and the journal action strings do not change.

Section 1 records what the code does today. Section 2 is the design. Section 3 lists the tests. Section 4 lists the limits and the work moved to unit 1h.

## 0. Windows facts the design uses

- **W1.** "This number is incremented whenever the contents of the clipboard change or the clipboard is emptied. ... If clipboard rendering is delayed, the sequence number is not incremented until the changes are rendered." Source: Microsoft, `GetClipboardSequenceNumber`, Remarks (read 2026-11-01). Zero means no clipboard access.
- **W2.** While AKM holds the clipboard open, no other window can open it. Source: Microsoft, `OpenClipboard` (not fetched in this session). One exception is documented: a clipboard owner that answers `WM_RENDERFORMAT` sets data while the requesting program holds the clipboard open. Inside `publish`, AKM requests no other program's data before `EmptyClipboard`: production does not call `is_empty`, and the read-back after the publication reads AKM's own non-delayed data. So this exception cannot occur between the two sequence samples of section 2.1.
- **W3.** `EmptyClipboard` with a NULL open owner succeeds and sets the owner to NULL. Source: Microsoft, `EmptyClipboard`, Remarks (read 2026-11-01).

From W1 and W2: inside one open interval of `publish`, AKM compares the sequence before and after a failed `EmptyClipboard`. If the sequence is unchanged, no change was observed. If it changed, only AKM's own failed call can have changed it, and its effect is unknown. Windows does not document whether a failed call can change the contents without changing the sequence, so "unchanged" means "no change observed", not "no change". Section 2.1 uses only that weaker reading.

The cleanup path (`clear`) reads text after its sequence check and does not check the sequence again before it empties. Design review 2 showed that W2's exception and the ignored `DestroyWindow` result (`clipboard.rs:460`) leave a gap there. That gap is 1H-03, in unit 1h. Unit 1a does not change `clear`, except for the failed empty (2.2).

## 1. Inventories

### 1.1 Publisher exits (`publish`, `clipboard.rs:343`)

"Earlier copy" means the clipboard content before this copy. It can be an earlier AKM copy with its own lease. "Secret" means a format that holds bytes of the new value. Production calls `publish` with `require_empty = false` (`clipboard.rs:651`). Only the opt-in real-clipboard test uses `true`.

| # | Line | Step that fails | Reachable from production copy | Clipboard after | Earlier copy | Secret left | Today's result |
|---|---|---|---|---|---|---|---|
| 0 | 92 | value contains NUL (lease check) | yes | not opened | kept | no | `Err` |
| 1 | 350 | register an exclusion format | yes | not opened | kept | no | `Err` |
| 2 | 350, 355-362 | allocate or lock memory | yes | not opened | kept | no | `Err` |
| 3 | 353 | encode the text (`TextFormats::new`; calls Windows directly) | yes, but no fake can fault it today | not opened | kept | no | `Err` |
| 4 | 364 | create the owner window | yes | not opened | kept | no | `Err` |
| 5 | 365 | open the clipboard (busy) | yes | not opened | kept | no | `Err` |
| 6 | 366 | non-empty clipboard while `require_empty` | **no** (opt-in test only) | opened, closed | kept | no | `Err` |
| 7 | 372 | `EmptyClipboard` | yes | unknown | unknown | no | `Err` |
| 8 | 375 | set an exclusion format | yes | emptied; second empty tried, result ignored | **gone** | no | `Err` |
| 9 | 380 | exclusions not applied as zero | yes | emptied; second empty ignored | **gone** | no | `Err` |
| 10 | 394 | set an empty frame format | yes | emptied; second empty ignored | **gone** | no | `Err` |
| 11a | 401 | set the first secret format (Unicode) | yes | emptied | **gone** | no | `Ok` with `failure`, then rollback |
| 11b | 401 | set a later secret format | yes | earlier secret formats present until rollback | **gone** | yes until rollback | `Ok` with `failure`, then rollback |
| 12 | 407-415 | read-back differs or is unreadable, or sequence 0 | yes | all secret formats present until rollback | **gone** | yes until rollback | `Ok` with `failure`, then rollback |
| 13 | 421 | none | yes | published, verified, nonzero sequence | **gone** | yes (the lease) | `Ok` without `failure` |
| 14 | 254 | `CloseClipboard` (return ignored) | yes | as above | as above | as above | not observed |

Rollback after 11 and 12 (`clipboard.rs:419`) calls `EmptyClipboard` again. The sequence was sampled before the rollback (`clipboard.rs:409`). When the rollback fails, today's code keeps that earlier sample. W1 says that a failed empty can still have changed the sequence.

**Defects this shows:**
- Exits 8 to 10 return `Err`. `copy_reported` (`clipboard.rs:97`) treats every `Err` as "nothing changed", so the earlier lease stays on an emptied clipboard (1A-R8-03).
- Exit 7 is assumed unchanged.
- A failed rollback keeps a sequence that may already be stale (D3).
- Exit 12 says "The clipboard changed while copying" also when the read-back was only unreadable or the sequence was 0.

### 1.2 Cleanup exits (`clear`, `clipboard.rs:428`, and the lease, `clipboard.rs:130`)

| # | Line | Case | Reachable through Session | Today's result | Lease after |
|---|---|---|---|---|---|
| C1 | 433 | open fails (busy) | yes | `Err(clipboard)`, mapped to `clipboardUncertain` by its kind string (`clipboard.rs:141`) | kept, retried |
| C2 | 435 | captured sequence 0 | **no**: the lease rejects zero first (`clipboard.rs:132`) | `Ok(false)` | released |
| C3 | 440 | current sequence 0 | yes | `Err(clipboardUncertain)` | kept, retried |
| C4 | 444 | sequence changed | yes | `Ok(false)` | released; the newer copy stays |
| C5 | 449 | text same, `EmptyClipboard` succeeds | yes | `Ok(true)` | released |
| C5b | 450 | text same, `EmptyClipboard` fails | yes | `Err`, mapped to `clipboardUncertain` | kept, retried; if the failed empty changed the sequence, the next attempt sees C4 and releases with no warning (D3) |
| C6 | 453 | text different or unreadable | yes | `Err(clipboardUncertain)` | kept, retried |
| C7 | 132 | lease is unverifiable (sequence 0, not rolled back) | yes | `Err(clipboardUncertain)`, no clear call | released, final |
| C8 | 171 | `Drop` of the lease at exit | yes | one `clear()`, error ignored | gone with the process |

### 1.3 Session paths (`desktop/src-tauri/src/session.rs`, `desktop/src-tauri/src/lib.rs`)

| Path | Line | Cleanup today | Attempts in one call |
|---|---|---|---|
| `gate()`, idle expiry seen on entry | 177-183 | `locked_gate` → `clear()` | 1, plus whatever the caller does next |
| `gate()`, poisoned mutex | 187-192 | `locked_gate` → `clear()` | 1 |
| `observe(signal)` (OS lock, suspend, unobservable) | 212 | `gate()`, then `locked_gate` | up to 2 (expiry in `gate()`, then the signal) |
| `lock()` | 222 | `gate()`, then `locked_gate` | up to 2 |
| `lock_owned()` | 232 | `gate()`, then `locked_gate` | up to 2 |
| `tick()`, unlocked | 268 | `gate()`, then `tick(now)`: clears when due | up to 2 |
| `tick()`, locked | 270 | `gate()`, then `clear()` on every tick while a lease remains | up to 2; a retry on every tick |
| `activity()` | 276 | `gate()`; `locked_gate` if the policy locks now | up to 2 |
| `start_open()` | 314-320 | `gate()`, then `clear()` | up to 2 |
| `copy_stamped` → `copy_value` (production, both copy commands, `commands.rs:324`, `commands.rs:517`) | 592, 600 | publication; the gate is held during the publication | 1 publication |
| `copy` (test only, no presentation stamp) | 587 | as above | as above |
| window `CloseRequested`, then `Destroyed` (`lib.rs:44-52`) | — | `lock()` for each event | up to 2 per event |
| process exit | `clipboard.rs:171` | `Drop`: one `clear()`, error ignored | 1 |

In every lock path, `locked_gate` (`session.rs:198`) runs the cleanup before `retire_owner` sends the lock notice. So a slow cleanup delays the notice (D1).

### 1.4 Clipboard texts the user sees

| Where | Text today | Problem |
|---|---|---|
| `desktop/src/components/KeyDetail.tsx:102` | "Copied. AKM clears this copy after N seconds, on lock, or on normal exit. A crash can prevent cleanup." | promises cleanup that a busy clipboard can prevent (1A-R8-02) |
| `desktop/src/components/Settings.tsx:199` | "Copied. AKM clears its clipboard copy after N seconds, on lock, or on normal exit." | same |
| `desktop/src/components/Settings.tsx:348` | "Locking closes the current session and clears AKM's clipboard copy." | same, for lock |
| `README.md:84-89` | "AKM clears its current copy after 30 seconds, on lock and on normal exit, ..." | same |
| `clipboard.rs:92` | "Clipboard text cannot contain a NUL character." | accurate; keep |
| `clipboard.rs:280`, `:288` | the locale and encoding errors | accurate; keep |
| `clipboard.rs:369` | "Refusing to overwrite the user's clipboard." | opt-in path only; keep |
| `clipboard.rs:387` | "Windows did not apply clipboard history/cloud/monitoring exclusions. The copy was cancelled." | does not say that the clipboard was emptied (exit 9) |
| `clipboard.rs:413` | "The clipboard changed while copying. Copy again." | claims a cause the code did not observe (unreadable read-back, sequence 0) |
| `clipboard.rs:513` | "Windows could not create a clipboard owner." | accurate; keep |
| `clipboard.rs:525`, `:536`, `:548` | register, allocate and lock memory errors | accurate; keep |
| `clipboard.rs:561` | "The clipboard is busy. Try copying again." | accurate for a copy |
| `clipboard.rs:584` | "Windows could not clear the clipboard." | exit 7: the effect is unknown; says nothing about the earlier copy |
| `clipboard.rs:594` | "Windows could not publish private clipboard data." | exits 8, 10, 11: does not say whether the clipboard was emptied or the copy removed |
| `clipboard.rs:18` (`uncertain_cleanup`) | "AKM could not confirm that its clipboard copy was cleared. Check the clipboard before you paste, and clear it if the copy is still there." | accurate (1A-R7-03); keep |

`README.md:141` is about file-export cleanup, not the clipboard. It is out of this inventory (D9).

### 1.5 CLI output and disclosure sinks (`crates/akm-cli/src/main.rs`)

| Command | Sink | Secret-derived bytes | Record before | Today |
|---|---|---|---|---|
| `get` (default) | stdout, `print_key` (`main.rs:690`, receives the whole `ApiKey`) | **yes: `mask()` prints the first and last 4 characters of values over 8 characters** (`main.rs:774`) | none | 1A-R8-01 |
| `get --reveal` | stdout, `print_key` | full value | `reveal-requested`, fails closed | correct |
| `get --field secret` (any case, surrounding spaces; `main.rs:748`) | stdout, `print_field` (receives the whole `ApiKey`) | full value | `reveal-requested`, fails closed | correct |
| `get --field <other>` | stdout | no | none | correct, but nothing stops a new field arm from printing `key.secret` (D5) |
| `list`, `list --json` | stdout | no (`***` constant in JSON) | none | correct |
| `export` | stdout or a new file | full values | `export-requested` before, `export-completed` after | correct; no caller-level proof for a written-then-unconfirmed completion record (1A-R8-04, a coverage finding) |
| `run` | child environment | full values | `run-requested`, fails closed before launch | correct; `crates/akm-cli/README.md:29` says `run-start` (1A-R8-05) |
| `add`, `rm`, `init`, `upgrade` | stdout, stderr | no | post-commit warnings | correct since round 8 |
| `audit` | stdout | no | none | correct |

`Vault::log` (`akm-core/src/vault.rs:463`) accepts any action string, so a list of names alone does not prove what the binary records (D10).

### 1.6 Claims, kind, and the test that proves each through its real caller

| Claim | Kind | Real caller | Test today | Gap |
|---|---|---|---|---|
| A failed copy never keeps a lease on an emptied clipboard | defect (1A-R8-03) | `Session::copy_stamped` (both copy commands), real `publish`, fake Win32 API | scripted backend only (`clipboard.rs:1758`) | no production-caller test |
| A kept or shown clipboard text never promises more than an attempt | defect (1A-R8-02) | the rendered React components; `Session` with a busy clipboard at expiry, lock, close and exit | none for the success texts | all |
| The default view prints no secret-derived bytes | defect (1A-R8-01) | the CLI binary | the `mask` unit test requires fragments | all |
| A written-then-unconfirmed completion record warns and exits 0 | coverage (1A-R8-04) | the CLI export path | helper only (`main.rs:891`) | no caller-level test |
| The documented action names match the recorded ones | defect (1A-R8-05) | the CLI binary and `crates/akm-cli/README.md` | `security.rs:285` checks the journal only | no comparison |

## 2. Design

### 2.1 The publisher reports what it observed (1A-R8-03)

The defect class: the lease assumes that a publisher `Err` changed nothing. The class fix: the caller never chooses the outcome. One transaction value, `Tx`, records the phase that `publish` reached. Every failure goes through `tx.fail(error)`, and `fail` derives the outcome from the recorded phase. A new exit that calls `tx.fail` gets the outcome of its phase. No exit can name an outcome directly, because `Publication` has no public constructor outside the `Tx` module.

```rust
pub struct Publication(Outcome);
pub enum Outcome {
    Untouched(CommandError),            // phase before EmptyClipboard; or a failed empty with the sequence unchanged
    EarlierCopyUncertain(CommandError), // a failed empty with the sequence changed or 0
    Emptied(CommandError),              // the first empty succeeded; no byte of the new value remains
    Published(LeaseEvidence),           // verified, nonzero sequence from the same open interval
    Residual(CommandError, LeaseEvidence), // a secret was written, the rollback failed, the sequence is unchanged since the sample
    ResidualUntracked(CommandError),    // a secret was written, the rollback failed, and the sequence changed or was 0
}
```

| Phase recorded by `Tx` | Set at | `fail(e)` gives |
|---|---|---|
| `Preparing` | start of `publish` | `Untouched(e)` |
| `Open` (sequence sampled after `OpenClipboard`) | after `Open::acquire` | `Untouched(e)` |
| `EmptyFailed` | `EmptyClipboard` failed; the sequence is sampled again | unchanged: `Untouched(e)`; changed or 0: `EarlierCopyUncertain(e)` |
| `Emptied` | after the first `EmptyClipboard` | `Emptied(e)`; the second empty is still tried, and its result does not change the outcome, because no byte of the value was written |
| `Written` | before the first secret format is set | rollback, then: rollback ok: `Emptied(e)`; rollback failed, sequence unchanged since the sample: `Residual(e, ev)`; changed or 0: `ResidualUntracked(e)` |

- Exit 11a sets the `Written` phase before the first secret `set` call. A failed first `set` therefore rolls back. That costs one extra empty, and it is always safe.
- `rolled_back`, `CopyAttempt`, and the comment "A preparation failure (`Err`) changed nothing" (`clipboard.rs:97`) are deleted.
- `ClipboardLease::copy_reported` matches on the `Outcome` and applies the transitions in 2.3.

**Messages.** Each message states only what AKM observed:
- `Untouched`: the cause. If a lease exists, add: "AKM's earlier copy is still on the clipboard and may remain."
- `Emptied`: the cause, then "The earlier clipboard content was removed, and the new value was not copied. Copy again." This does not say that the clipboard is empty: exclusion markers or empty text formats can remain (design review 2, finding 7). It does not say that the earlier content was an AKM copy, because there may have been none.
- `Residual`: the cause, then "AKM could not remove the partial copy. Check the clipboard before you paste."
- `EarlierCopyUncertain` and `ResidualUntracked`: the cause, then the `uncertain_cleanup` text (`clipboard.rs:18`).
- Exit 12: the cause is "Windows did not confirm the copy." The old text "The clipboard changed while copying" claimed a cause that AKM did not observe.

### 2.2 A failed cleanup empty (same premise)

In `clear` (C5b), a failed `EmptyClipboard` has the same unknown effect. `clear` samples the sequence again in the same open interval:
- unchanged: `Err(clipboardUncertain)`, as today; the lease stays and is retried;
- changed or 0: a new error kind, `clipboardLost`. The lease ends, and Session records the final uncertainty warning. Cleanup never adopts the later sequence (contract 10).

The rest of `clear` does not change in 1a (1H-03).

### 2.3 Lease transitions

| Event | Lease before | Lease after | Reported |
|---|---|---|---|
| `Untouched(e)` | A or none | unchanged | error `e` |
| `EarlierCopyUncertain(e)` | A or none | none | error `e`; if A existed, the final uncertainty warning |
| `Emptied(e)` | A or none | none | error `e` |
| `Published(ev)` | A or none | new lease, due in 30 s | nothing |
| `Residual(e, ev)` | A or none | new lease, due now; one cleanup attempt at once | error `e`, then the cleanup row below |
| `ResidualUntracked(e)` | A or none | none | error `e`, and the final uncertainty warning |
| cleanup `Ok(true)` or `Ok(false)` | L | none | nothing |
| cleanup `clipboardUncertain` | L | L, retried | the uncertainty warning, once per episode |
| cleanup `clipboardLost` | L | none | the final uncertainty warning, unless this episode already warned |
| exit (`Drop`) | L | gone | nothing; the best-effort texts cover it |

**Invariants.** Each is a test assertion over every row:
1. A lease exists only with nonzero evidence from the same open interval as its publication.
2. A failure after a successful `EmptyClipboard` never leaves an earlier lease.
3. A failure with no change observed keeps an earlier lease.
4. Cleanup never empties after the sequence changed, and never adopts a later sequence.
5. One uninterrupted failing episode gives one warning. A recovery followed by a new failure gives a new warning ID.

The Session lock order, the attempt budget, and the window-event paths do not change in 1a (1H-01, 1H-02).

### 2.4 Clipboard texts (1A-R8-02)

One frontend module, `desktop/src/clipboardText.ts`, holds the two texts. `KeyDetail` and `Settings` use it:
- Copy: "Copied. AKM tries to clear its copy after N seconds, on lock and on normal exit. The copy may remain if the clipboard is busy or AKM stops unexpectedly."
- Lock (Settings): "Locking closes the current session and tries to clear AKM's clipboard copy. The copy may remain if the clipboard is busy."

`README.md:84-89` says the same. The native texts change as listed in 2.1. No text promises a warning after AKM exits, and no text promises a later retry.

### 2.5 CLI default view (1A-R8-01)

- `KeyMetadata`, built `From<&ApiKey>`, has the fields that the views print, and no secret field.
- The default `get` view and every non-secret `--field` receive only `KeyMetadata`. The default view prints the line `Secret:      ********` for every value and every length.
- `get --reveal` and `get --field secret` keep their record-first, fail-closed path and receive the `ApiKey`.
- `mask()` and its fragment test are deleted.

A boundary that also covers `Vault::env_vars`, whole-key serialization and every sink is 1H-04, in unit 1h.

### 2.6 Real callers for tests

- **Desktop.** The tests call `Session::copy_stamped`, the function that both production copy commands call (`commands.rs:324`, `commands.rs:517`), with a `Stamped` value from `session.snapshot()`. They build `Session::new` over `TransactionBackend(FakeApi)`, which wraps the real `publish` and `clear` over the fake Win32 API.

  Today that fake is private to `clipboard::windows::tests`. Commit 1 moves `FakeApi`, `FakeState`, `Fault` and `TransactionBackend` into a `pub(crate)` test-support module. This changes test code only. In the same commit, the new tests assert only observable state: the fake clipboard's contents and sequence, `owned_generation`, the warnings, and the cleanup calls. So they compile on today's production code. They are run there and must fail as stated in 3.1. Commit 2 holds the fix.
- **CLI.** `run_with(cli, io) -> i32`. `io` holds the stdout writer, the stderr writer, and the journal used for the export-completion record. `main` builds the real ones. This is a coverage seam (1A-R8-04): it is added in the fix commit, and its strength is shown by the mutants in 3.3. No fault option reaches the release binary. `record_completed_with` and its helper-only test are deleted.

### 2.7 Action names and their order (1A-R8-05)

The journal strings stay as they are. `crates/akm-cli/README.md` says that `run-requested` is recorded before the launch, `export-requested` before any export byte, and `export-completed` after the export. A caller-level test:
- runs `run` with the existing fake child (`security.rs:263`), `export`, and `get --reveal`;
- reads each journal action they recorded, in order;
- checks that the README names exactly those actions, and that the journal order matches the README: `export-requested` before `export-completed`.

## 3. Tests

A defect test fails on today's code first (commit 1), then passes after the fix (commit 2). A coverage test (1A-R8-04) shows its strength with the mutants in 3.3 (AGENTS.md).

### 3.1 Clipboard (defects 1A-R8-03 and 1A-R8-02)

The tests are split by the precondition each case needs:

- **Copy table, through `copy_stamped`.** From no lease and from an existing lease A. One fault per case, at each publisher step that a key copy reaches:
  - register, allocate, owner, open (busy);
  - `EmptyClipboard` failed, with an unchanged sequence and with a changed one;
  - an exclusion set, the exclusion check, an empty frame set;
  - the first secret set, a later secret set, the read-back;
  - a rollback that succeeds, and one that fails with and without a sequence change;
  - success.

  Each case asserts: the clipboard contents and sequence, the lease after, the error text, the warnings, the later cleanup calls, and that the fake reached the intended step.

  The NUL check (row 0) runs through a key copy only. An environment copy rejects NUL values earlier, in `Vault::env_vars` (design review 2, finding 8). Encoding (row 3) cannot be faulted by the fake; that seam is 1H-07. Row 6 is the opt-in path; it keeps its direct `publish(require_empty = true)` test.

  Today, these cases fail: exclusion set, exclusion check and empty frame set from lease A (1A-R8-03); `EmptyClipboard` failed with a changed sequence; a failed rollback with a changed sequence.
- **Cleanup, at a due tick and at lock.** Busy open; current sequence 0; text different or unreadable; a failed empty with the sequence unchanged; a failed empty with the sequence changed (`clipboardLost`). Each case asserts the lease, the warnings and the warning IDs. Today, the changed-sequence case fails. A separate case keeps a newer identical user copy: the sequence changed before cleanup, so cleanup leaves the newer copy and releases the lease.
- **Busy at every ending (1A-R8-02).** A Session with a lease and a busy clipboard at expiry, at lock, and at drop. The fake value remains, nothing panics, and the only warning is the uncertainty warning (none after drop).
- **Frontend (real components).** `KeyDetail` after a copy and `Settings` are rendered. Each shows the literal texts of 2.4, including "may remain". No rendered text contains "clears", or "will" followed by a retry. Today this fails.

### 3.2 CLI

- **Default view (defect 1A-R8-01).** `get` on fake secrets of 1 (`*`), 8, 9 and 40 characters, and a non-ASCII secret, all with the same metadata:
  - the `Secret:` line is exactly `Secret:      ********`;
  - the whole output is identical for every secret.

  Every field name that `--field` accepts, in its listed spelling, upper case and with surrounding spaces, either records `reveal-requested` first (secret) or prints output that does not differ between the secrets. With a damaged journal, `--reveal` and `--field secret` still fail closed. Today the default output differs for 9 and 40.
- **Export completion (coverage 1A-R8-04).** The `run_with` test in 2.6.
- **Action names (defect 1A-R8-05).** The test in 2.7. Today it fails on `run-start`.

### 3.3 Mutants, each restored after its run

- In `Tx::fail`, map the `Emptied` phase to `Untouched`.
- Set the `Written` phase after the first secret `set`, not before it.
- Keep the earlier lease on `Emptied`.
- Keep the pre-rollback sequence after a failed rollback.
- Map `clipboardLost` to a retry.
- Print one secret character in the default view.
- Pass the `ApiKey` to a non-secret field and print the secret for it.
- Drop the completion warning.
- Return 1 after a kept completion line.
- Put `run-start` back in the README.

Each mutant must fail a test.

## 4. Limits, and the work moved to unit 1h

**Limits of unit 1a:**
- **Failed empty.** An unchanged sequence after a failed `EmptyClipboard` means "no change observed". If Windows can change the contents without changing the sequence, AKM keeps the lease, and the later cleanup reads the text before it empties anything.
- **`CloseClipboard`.** Its result is still ignored (`clipboard.rs:254`). The outcome that AKM reports was observed before the close. This is 1H-06.
- **Fakes.** Real Win32 faults cannot be reproduced here. The fake Win32 API stands in for them.
- **Exit.** Cleanup at exit is one attempt. A busy clipboard at exit, a crash or a power loss can leave the copy, and the texts say so.
- **Sequence wrap.** The sequence is a 32-bit counter. The wrap limit in section 4 of the protocol design stays.

**Moved to unit 1h** (the design reviews' findings; FINDINGS.md has one row for each):

| ID | Work | Source |
|---|---|---|
| 1H-01 | Lock waits for a copy or a tick cleanup that holds the session gate; the owner's narrowed lock rule (notice first, one attempt) | review 1 #1, review 2 #1 |
| 1H-02 | One cleanup attempt per entry call, across nested `gate()` calls, `current()` and `opening()`; `start_open` order; window and app exit events | review 1 #2, review 2 #2 |
| 1H-03 | `clear` reads and empties without a second sequence check; the `WM_RENDERFORMAT` exception; the ignored `DestroyWindow` result | review 2 #3 |
| 1H-04 | A disclosure boundary that covers `Vault::env_vars`, `ApiKey` serialization and `Debug`, and every sink, bound to its journal action | review 1 #5, review 2 #4 |
| 1H-05 | A compile-time phase boundary for publisher outcomes (1a uses the runtime `Tx` phase) | review 1 #4, review 2 #5 |
| 1H-06 | A checked `CloseClipboard` outcome for each phase | review 1 #13, review 2 #6 |
| 1H-07 | An injectable text-encoding seam (row 3), and generated-handler tests over the real publisher | review 1 #7, review 2 #9 |
| 1H-08 | Copy-command wrappers (snapshot capture, scope construction, dispatch) tested with a delayed command and a session change | review 2 #9 |

## 5. Order, final checks and stop

1. **Commit 1:** the test-support move, and the tests in section 3 that need no new production code. Each defect test is run on today's production code and must fail as stated. If one does not fail, the design is wrong: stop.
2. **Commit 2:** the implementation, the text and action-name sweep, and the `run_with` seam with its coverage test. Then the mutants.
3. The full check run in [TESTING.md](../TESTING.md) on the final source. Then the final build: record the hashes of `akm-desktop.exe`, `akm.exe`, the MSI and the NSIS installer; run the release-CLI checks and the native checks that need no owner gate against those exact files; hash them again (protocol section 7).
4. Real Windows evidence that needs the owner (TESTING.md, "Session, clipboard or export"): the opt-in real-clipboard test (an empty clipboard, and the owner present), and the GUI copy flow. Each runs only with the owner's yes in chat. If it does not run, it is reported as not checked. R4-04 stays deferred, so unit 1a is not called complete.
5. One acceptance review, against this scope. If it rejects, stop and ask the owner.

Stop early if a defect test cannot be made to fail on today's code, or if a test reaches a substitute and not `copy_stamped` or the CLI binary.

## 6. Dispositions of both design reviews

| Finding | Where it is handled |
|---|---|
| Review 1 #1, review 2 #1 (lock waits) | 1H-01 |
| Review 1 #2, review 2 #2 (session paths, attempt budget) | 1H-02 |
| Review 1 #3 (failed rollback or cleanup empty) | 2.1 `Written` phase; 2.2 `clipboardLost` |
| Review 1 #4 (exit 7; phase enforcement) | 2.1 `EmptyFailed` phase and `Tx::fail`; compile-time boundary 1H-05 |
| Review 1 #5, review 2 #4 (disclosure class) | 2.5 for the default view; the wider boundary 1H-04 |
| Review 1 #6 (`Session::copy` is test only) | 2.6: the tests call `copy_stamped` |
| Review 1 #7, review 2 #8 (unreachable cases) | 3.1, split by precondition; encoding 1H-07 |
| Review 1 #8 (coverage forced to fail first) | 3: defect and coverage kinds |
| Review 1 #9 (texts, literal test, busy exit) | 1.4, 2.4, 3.1 |
| Review 1 #10, review 2 #10 (action names, order) | 2.7 |
| Review 1 #11 (mask oracle) | 3.2 |
| Review 1 #12 (final build, owner-gated evidence) | 5, steps 3 and 4 |
| Review 1 #13, review 2 #6 (`CloseClipboard`) | limit in 4; 1H-06 |
| Review 2 #3 (Windows facts, the `clear` fence, the owner window) | section 0 corrected; 1H-03 |
| Review 2 #5 (phase constructors) | 2.1: outcomes only through `Tx::fail`; 1H-05 |
| Review 2 #7 (`Emptied` message can be false) | 2.1 messages |
| Review 2 #9 (wrapper and baseline gaps) | 2.6 (commit 1 holds only test code); wrappers 1H-08 |
