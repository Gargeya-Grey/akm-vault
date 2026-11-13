# Unit 1h: clipboard and disclosure hardening

- Status: implemented on branch `phase2/units-1c-to-4`. Not yet reviewed (the independent review follows the whole chain). The owner may veto any decision under "Decisions for the owner's veto".
- Tier: Tier 2, 8 units, risk: privacy, public API (crate APIs), hook-like window events. The sub-units: disclosure capability (core, CLI, desktop), publication typestate, clipboard release and cleanup, encoding seam and fake extraction, session lock order, generated-handler tests, copy table, CLI export observation.
- Scope: 1H-01 to 1H-08, 1A-A2-02, 1A-A2-03, and L1 ([FINDINGS.md](../FINDINGS.md)). Inputs: [unit 1a repair design](unit1a-repair.md) (section 4), its two design reviews ([1](../reviews/2026-11-01-unit1a-repair-design-review1.md), [2](../reviews/2026-11-01-unit1a-repair-design-review2.md)), and [unit 1c](unit1c.md) (not undone).
- Public API change: yes, inside the workspace crates.
  - `akm-core`: `ApiKey` has no secret field. New `Disclosure`, `EnvPlan`, `Secret`. `Vault::env_vars` takes a plan and a proof. `Vault::secret` is new. `ApiKeyInput.secret` is a `Secret`.
  - `akm-cli`: `Io` gets a `files` publisher.
  - `akm-desktop` lib: new `publication` and `error` modules (public, doc-hidden, for `compile_fail` doctests). `Session` gets `shutdown`. `ClipboardApi` gets `encode` and `destroy`.
  - Not changed: IPC shapes, KDBX format, journal format and action strings, rotation fields.

## 1. Facts the design uses

- Lock order today: `Session::gate` (a mutex) holds the clipboard lease. `locked_gate` cleans up first and sends the lock notice second, both under the gate. A tick cleanup holds the gate too (1H-01).
- `OpenClipboard` fails at once when the clipboard is busy. It does not wait. So "never waits for a busy clipboard" means: no retry loop, no sleep in a cleanup attempt.
- Windows: the sequence number does not change for a delayed-rendered format until the owner renders it ([GetClipboardSequenceNumber](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber)). An owner that answers `WM_RENDERFORMAT` sets data while the requesting program holds the clipboard open. So the sequence can change between a read and the empty that follows it.
- `Publication` and `Outcome` can be built by any code inside `clipboard.rs`, because a private field is visible to the whole defining module and its children (design review 2, finding 5).

## 2. Decisions for the owner's veto

### D1. The lease leaves the gate (1H-01)

`Session` keeps the clipboard lease in its own mutex, `clipboard`, with the cleanup-episode bookkeeping that belongs to it. Lock order is **vault, then clipboard, then gate**. Code that holds the gate never waits for the clipboard mutex.

- Rejected: a cleanup worker thread with generation fences. More moving parts, and the lock notice does not need it.
- Rejected: `try_lock` on the lease in lock. A lock that finds a running not-due tick would skip its attempt and leave a secret for a later tick.

### D2. Notice first, then one attempt, by a transition flag (1H-01, 1H-02)

A lock transition (`locked_gate`, expiry seen in any gate call, a poisoned gate, `start_open`) does two things under the gate: it sends the lock notice (`retire_owner`) and sets `gate.cleanup_owed`. It never touches the clipboard. When a gate guard is dropped and the flag is set, the guard takes the flag and calls `settle` after it released the gate. `settle` takes the clipboard mutex, then the gate, and decides:

| State under both locks | Action |
|---|---|
| lease held, and the session is locked or the lease belongs to an older ticket | one cleanup attempt (`clear`) |
| lease held, session active, lease due (tick only) | one attempt (`tick`) |
| otherwise | none |

- The ticket rule is the fence against a stale settle: a late settle from session A cannot clean session B's new copy.
- A tick keeps the rule that unit 1a accepted: while the session is not locked, a tick attempts only when the lease is due, even if the lease belongs to an older ticket (`cleanup_episode_a_successful_not_due_tick_is_not_recovery`). While the session is locked, a tick cleans up.
- The flag is a boolean, so two transitions inside one call give one attempt. A call that sets the flag after its attempt would need a second unlock in the same call; `begin_open` is the only path from locked to unlocked, and `start_open` settles once at its end. Tests count attempts at every public entry.
- `start_open` retires the old owner first, then settles.
- A lock no longer waits for the gate behind a copy in progress. The copy acts outside the gate and checks again after it (`commit-point.md`, row 6). A lock never waits for the clipboard mutex (FW-01, 13 Nov 2026). Its attempt only tries the lease, and when a cleanup or a copy holds the lease, the holder makes the owed attempt when it lets the lease go. The Windows message thread only delivers signals; a worker thread runs the timer's cleanup. Only `shutdown` waits for a copy in progress, after its lock, because nothing cleans after the exit. The notice never waits for a cleanup.
- Reading of "never retries": one public call makes at most one attempt and never loops. A busy attempt keeps the lease, and a later `tick` (a new public call, every second) tries again. This is the retry behaviour that unit 1a tests and reviews accepted (`a_busy_cleanup_warns_that_the_copy_may_remain_and_retries_in_one_episode`). Veto effect: drop the lease after one failed attempt, and warn once. That makes a busy clipboard at lock leave the copy for good.

### D3. Window and app exit events (1H-02)

`Session::shutdown()` replaces the four `lock()` calls in `lib.rs` (`CloseRequested`, `Destroyed`, `ExitRequested`, `Exit`). It locks and makes one attempt on the first call. Later calls do nothing until the next `start_open`. It marks the lease's generation as attempted for exit, so `ClipboardLease::drop` does not attempt again. A new copy has a new generation and a new exit attempt. A unit test checks that `lib.rs` calls `shutdown` and never `lock` on these events.

### D4. Cleanup rechecks the sequence before it empties (1H-03)

`clear` reads the text, then samples the sequence again, in the same open interval, immediately before `EmptyClipboard`. A changed or zero sequence means a newer or unknown owner (an owner that rendered during our read): cleanup empties nothing and the lease ends (`Ok(false)` for a changed sequence, `Err(clipboardUncertain)` for zero, as before).

The fake gets `render_on_read`: the next `read_text` makes a fake delayed-render owner replace the content with a user copy and advance the sequence. Without the recheck, cleanup would empty that copy. With a text identical to AKM's text, only the sequence tells them apart (contract 10).

### D5. `Release`: a checked close and a checked owner destruction (1H-03, 1H-06)

`Open::close` and the owner's destruction become explicit, consuming steps with a result. `Drop` stays as a safety net for unwinding only. A `Release { close_failed, owner_leaked }` value goes through `Publication::released` (in the `publication` module) or into the `clear` result. The treatment is a table, not scattered conditionals.

Publication, by the outcome reached before the release:

| Outcome before release | `CloseClipboard` failed | `DestroyWindow` failed |
|---|---|---|
| `Untouched`, `EarlierCopyUncertain`, `Emptied` | outcome unchanged, message gets "Windows did not confirm that AKM released the clipboard." Lease as the outcome says. | outcome and lease unchanged |
| `Published` | becomes `ResidualUntracked`: an error, no lease, the final uncertainty warning. Reason: a failed close can leave the clipboard held, so the captured sequence is not proof of the final state, and nothing may clean it up on a sequence it cannot trust. | outcome and lease unchanged |
| `Residual` | becomes `ResidualUntracked` | outcome unchanged |
| `ResidualUntracked` | unchanged | unchanged |

- A failed owner destruction cannot change the content or the sequence (the data is not delayed-rendered). It leaves a hidden message-only window. The lease stays. The lease collects a note, and `Session` turns it into one secret-free application warning: "AKM could not remove a hidden window it used to copy. The clipboard was not affected."
- Cleanup (`clear`), by its result before the release:

| `clear` result | `CloseClipboard` failed |
|---|---|
| `Ok(true)`, `Ok(false)` | new error kind `clipboardNotReleased`: the lease ends, one warning: "Windows did not confirm that AKM released the clipboard. Other programs may not be able to use it until AKM restarts." |
| `Err(clipboardUncertain)` | unchanged (retry later) |
| `Err(clipboardLost)` | unchanged |

### D6. Publisher outcomes are built by consuming transitions in a separate module (1H-05)

A new file, `publication.rs`, owns `Publication`, `Outcome`, `LeaseEvidence` and the typestate chain:

`begin() -> Preparing -> opened(seq) -> Opened -> emptied() -> Emptied -> writing() -> Written -> published(seq) -> Publication`

Each step consumes its state. `fail` exists on every state and builds only the outcome that the state allows (`Preparing` and `Opened`: `Untouched`; `Emptied`: `Emptied`; `Written`: rollback, then `Emptied`, `Residual` or `ResidualUntracked`). `Opened::empty_failed` gives `Untouched` or `EarlierCopyUncertain`. After `emptied()` the `Opened` value is moved, so an outcome from before the empty cannot be built. Fields are private to the module; `clipboard.rs` is a sibling and cannot reach them. `compile_fail` doctests (in the `akm-desktop` library crate) prove it, each next to a passing control that differs by one line. Test-only constructors (`for_test`) live in the same module under `cfg(test)`. The debug-only `fake_published` back door is deleted: `ipc_dispatch` uses the real publisher over the fake Win32 API.

### D7. Encoding is a call of the API trait (1H-07)

`ClipboardApi::encode(text) -> Result<TextFormats>` replaces the direct `TextFormats::new(.., GetSystemDefaultLCID())` in `publish`. `Win32` encodes with the system locale as before. The fake encodes with locale 0x0409 (so tests do not depend on the machine) and has `Fault::Encode`. The pre-existing test of the real conversion functions stays.

### D8. The fake Win32 API moves to `clipboard_fake.rs` (1H-07, 1H-08)

`FakeApi`, `FakeState`, `Fault`, `TransactionBackend` move out of `clipboard::windows::transaction_tests` into `src/clipboard_fake.rs`. The library includes it under `cfg(test)`. `tests/ipc_dispatch.rs` includes it with `#[path]`, so the generated handlers run over the real publisher. The `windows` module items it needs become `pub(crate)`.

### D9. Disclosure capability in `akm-core` (1H-04, L1)

- `ApiKey` has **no secret field**. `list`, `get_by_id`, `find`, `expiring`, `add`, `update` and `remove` therefore never copy a secret to the caller. `Debug` and `Serialize` of a key cannot contain it, by structure. The CLI `list --json` adds its legacy `"secret": "***"` field itself.
- `ApiKeyInput.secret` is a `Secret` (`Zeroizing<String>` inside). Its `Debug` prints `Secret(<redacted>)`, its `Serialize` prints `"***"`. `ApiKeyInput` loses `Deserialize` (nothing used it; a redacted round trip would be a trap). `ApiKeyInput::new` and `.into()` from `&str` and `String` keep the call sites short.
- `Disclosure` is a proof that the journal recorded a request. Only `Vault::disclose_key` and `Vault::disclose_env` build one, and they build it after `log` succeeded. A journal failure returns the error and no proof exists (contract 12: fail closed before disclosure). A proof names its action and the key IDs it covers.
- `Vault::secret(id, &Disclosure) -> Zeroizing<String>` needs a key-action proof that covers `id`.
- `Vault::plan_env(project, environment, all) -> EnvPlan` validates the scope, duplicates, names and values, and returns names and key IDs, no values. `Vault::env_vars(&plan, &Disclosure)` needs an export or run proof that covers exactly the plan's keys. The caller can therefore write the request record with the key count **before** any secret is materialized. Today the strings exist before the record.
- **A proof belongs to one vault session and one state (review 1, finding 3).** `Vault` has a random `session` ID (one per `Vault` value, never derived from the file) and a `state` counter that every commit and every reload advances. `Disclosure` and `EnvPlan` carry that stamp. `disclose_key` refuses a key that this vault does not hold (before it writes a record). `disclose_env` refuses a plan from another session or an earlier state. `secret` and `env_vars` go through `Disclosure::redeem_key` and `redeem_env`, the only readers of a proof. They check, in this order: plan stamp, proof stamp (session, then state), the kind and the covered keys, and last that the proof is unused. Success uses it up (an `AtomicBool`), so a proof discloses once. A refused presentation does not use it up. A proof from vault A therefore never opens vault B, also when B is a byte copy of A with the same key IDs and its journal is unavailable.
- The proof is bound to the journal through the session: a session records in the journal that its own evidence selects, and stale evidence never rebinds a journal (unit 1b, D10). A separate journal ID in the proof would add no check.
- Rejected: taking the proof by value (the compiler would enforce single use). It changes every call site in the CLI and the desktop, and a runtime check is needed anyway for the stamp. Rejected: a lifetime that borrows the vault (it would forbid holding a proof across a save, which is the case to refuse, but it spreads a lifetime through the callers).
- Actions stay: `reveal-requested`, `copy-requested`, `export-requested`, `run-requested`. Details are the callers' strings, unchanged.
- `Vault::edit_input(id)` returns an `ApiKeyInput` with the stored secret, for an update that keeps it. It is not a disclosure: the secret goes back into the vault and is never a plain `String` outside.
- `compile_fail` doctests in `akm-core`: no `secret` field on `ApiKey`; `Disclosure` cannot be built; `env_vars` needs a proof; `Secret` has no text accessor.
- Rejected: a `Secret` type inside `ApiKey` with an accessor that takes a proof. It keeps the secret loaded for every list call, which L1 names as a defect.
- Rejected: only redacting `Debug` and `Serialize` by hand. A public `secret: String` field stays readable by any caller.

### D10. CLI file export is observable (1A-A2-03)

`Io` gets `files: &mut dyn FilePublisher`. `main` passes the real one (`write_new_with` over `SystemExportIo`). The test passes one that wraps the same real file code with an instrumented `ExportIo`: it reads the journal at the first temporary-file byte (through a writer wrapper around the real file) and at publication, just before the real rename. An early-completion mutant fails.

### D11. Copy table asserts sequence, warning IDs and count (1A-A2-02)

Every row of the table reads the fake sequence and the exact application warnings (count, IDs) after the copy, after a tick, and after lock. A second failure episode runs while the first warning is pending and not acknowledged.

## 3. Failure cases

| Case | Result |
|---|---|
| Tick cleanup stalled in the backend, lock arrives | The notice is delivered. The lock call returns after the stalled cleanup ends. |
| Tick cleanup stalled, a copy arrives, lock arrives | The copy waits for the clipboard mutex, holding no gate. The notice is delivered. |
| Session A lock settles after session B copied | The ticket rule skips B's lease. |
| Idle expiry seen by `copy_stamped` | The copy answers `locked`. One attempt, after the gate is released. |
| A lock arrives while a copy runs | The lock sends its notice at once. The copy finishes, then answers `locked` (it checks the ticket again). The lock's attempt removes what the copy published. Before this unit the lock waited for the copy and the copy answered `Ok`; the test `closing_serializes_with_a_copy_in_progress_and_clears_it` now accepts both answers and still requires an empty clipboard. |
| Window `CloseRequested`, `Destroyed`, `ExitRequested`, `Exit` | One attempt in all. |
| Cleanup read triggers a delayed render | Nothing is emptied. The lease ends. |
| `CloseClipboard` fails after a verified copy | Error, no lease, final warning. |
| Journal fails before a disclosure | Error, no proof, no secret read, no clipboard or file write. |

## 4. Limits

- The lock waits for a copy or a cleanup already in progress (D2).
- Real Windows cannot be made to fail `CloseClipboard`, `DestroyWindow` or a delayed render here. The fake API stands in. The opt-in real-clipboard test and the owner's GUI test are not run (AGENTS.md safety).
- `Secret` is `Zeroizing<String>`. Clones inside the KDBX library, the OS and the webview are not zeroized. This unit does not claim full memory zeroization.
- `shutdown` is a latch, not a guarantee that the process exits. A close that the user cancels leaves it set until the next `start_open`.
- The settle for a lock transition runs in the caller's thread. A caller that holds the vault mutex (`access`) waits there for a running cleanup.
