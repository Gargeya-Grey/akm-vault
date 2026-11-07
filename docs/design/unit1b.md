# Unit 1b: private per-device journal

- Status: implemented on branch `phase2/units-1c-to-4`. Not yet reviewed (the independent review follows the whole chain). The owner may veto any decision under "Decisions for the owner's veto".
- Tier: Tier 2, 6 units, risk: privacy, schema (journal and mapping format), public API (crate APIs and the Activity IPC shape), delete (never done, see D6). The sub-units: location and permission layer, vault identity and mapping, record format and damage report, legacy sidecar notice, callers (CLI, desktop, proof tools), tests and test isolation.
- Scope: M1, P2-JOURNAL-IDENTITY, P2-JOURNAL-OUTCOMES, and the malformed-tail part of M6 ([FINDINGS.md](../FINDINGS.md)). Inputs: [unit 1h](unit1h.md) (the journal binds every secret sink; this unit keeps that) and contract 12 ([PROJECT.md](../PROJECT.md)).
- Public API change: yes, inside the workspace crates.
  - `akm-core`: new `journal` module (`JournalRoot`, `VaultEvidence`). `AuditLog` is bound to a root and to evidence about one vault. `AuditLog::append` loses the title, project and environment arguments. `AuditEntry` loses those three fields. New `ChainFile` (the old file-level chain code). `AuditSnapshot` gains `damage`, `identity`, `notices` and `location`. New `Error::Journal`. `Vault::open_in` and `Vault::create_in` take a `JournalRoot`.
  - `akm-desktop`: `AuditReport` gets `damage`, `identity` and `notices`. `AuditView` gets `keyId`. The error kind `journal` is new.
  - Not changed: KDBX format, the vault file (this unit never writes it), rotation and history fields, disclosure capability (unit 1h).

## 1. Facts the design uses

- Today `<vault>.audit.jsonl` and its `.lock` sit beside the vault. If that folder syncs, the cloud gets key titles, projects, environments and times (M1).
- `akm audit` takes no password today. It must keep working without one.
- A vault save replaces the file (temporary file, then rename). The file content, the file ID and the header seeds change on every save. A clone copies everything, including the root group UUID inside the encrypted body.
- `Vault` knows the SHA-256 of the bytes that it last read or wrote (`fingerprint`), and, once unlocked, the root group UUID.
- `std` cannot create a file with a security descriptor, and it exposes no file ID on stable Rust. The Windows API (`windows-sys`) can do both.
- The previous journal code is a plain append-only hash chain over one file. It reads the whole file under a shared lock.

## 2. Decisions for the owner's veto

### D1. Location: one private folder per Windows user, never beside the vault

Default root: `%LOCALAPPDATA%\akm\journal\`. `LOCALAPPDATA` is the non-roaming profile folder. If the variable is missing, empty or relative, the journal is **unavailable** and nothing falls back to another place.

Contents (all created by AKM, all private, see D4):

| File | Purpose |
|---|---|
| `mapping.json` | the vault to journal bindings (D2) |
| `mapping.lock` | lock for the mapping |
| `j-<32 hex>.jsonl` | one journal |
| `j-<32 hex>.jsonl.lock` | lock for that journal |
| `mapping.json.tmp` | exists only during a mapping replace |

- The environment variable `AKM_JOURNAL_ROOT` replaces the root. **Only tests and the proof scripts set it.** The value must be an absolute path. A set but invalid value makes the journal unavailable (no fallback to the default). The folder is still checked by every rule in D4, so the variable cannot send a journal to a readable place. Contract 11 already removes `AKM_*` from child processes.
- Library callers pass a root explicitly (`Vault::open_in`, `JournalRoot::at`). `Vault::open` and `Vault::create` read the environment variable.
- `.cargo/config.toml` sets `AKM_JOURNAL_ROOT` to `target/akm-test-journal` for every process that Cargo starts (tests, `cargo run`). A test that forgets to choose a root writes there, never to the real `%LOCALAPPDATA%`. Tests that care pass their own temporary root.
- Rejected: a path next to the vault with a different name. Any name beside the vault syncs.
- Rejected: `%APPDATA%`. It roams.
- Rejected: asking the user to choose a place. It adds a way to choose a synced place.

### D2. Vault identity: a device-local binding, never a write to the vault

A journal is named by a random 128-bit ID that exists only on this device. The mapping binds a vault to one journal ID by evidence. Nothing is ever written to the vault to give it an ID: older-format (KDBX 3.x, 4.0) and read-only vaults get journals like any other, and the vault bytes stay unchanged.

Evidence about a vault (`VaultEvidence`):

| Field | Source | Meaning |
|---|---|---|
| `path` | the path, made absolute, symlinks and junctions resolved, compared without case | where it is |
| `file_id` | volume serial number and file index of the open file, if the file system gives them | which file object (survives rename) |
| `sha` | SHA-256 of the vault bytes: for an open `Vault`, the bytes it last read or wrote; for `akm audit`, the bytes on disk now | which exact content |
| `root` | the root group UUID inside the vault, only for an unlocked `Vault` | which vault family (clones share it) |

A binding keeps: `id`, `path`, `file_id`, `root` (unknown until an unlocked append), `seen` (the last 32 `sha` values, newest first) and a state (`active` or `detached` with a reason and a time).

No single field decides. The rules, first match wins, over the active bindings. `P` is the binding with the same path.

| # | Case | Result |
|---|---|---|
| R1 | `P` exists, `sha` is in `P.seen` | **Same.** If `sha` is not the newest, the vault went back to an earlier version that this device saw (**rollback**). |
| R2 | `P` exists, `sha` unseen, both roots known and equal (or `P.root` unknown and the new root known: adopt it) | **Same.** Another process or device changed the vault. A rollback to a version this device never saw looks the same, and cannot be told apart. |
| R3 | `P` exists, `sha` unseen, both roots known and different | **Replaced.** A different vault is now at this path. `P` becomes `detached` (reason `replaced`, its file stays). A new journal starts. |
| R4 | `P` exists, otherwise (no root: a locked read by `akm audit`, or the vault file is unreadable) | **Unconfirmed.** The journal of `P` is shown with a notice. The mapping is not changed. An append with this evidence is refused. |
| R5 | no `P`; a binding `F` has the same `file_id`, and `sha` is in `F.seen` or the roots are equal | **Moved.** Rename or move on the same volume. `F.path` is updated. |
| R6 | anything else | **New.** A new binding and journal. |

How each requested case falls out:

| Case | Rows | Result |
|---|---|---|
| Rename or move on one volume | R5 | Same journal, path updated. |
| Copy (clone) to another path | R6 | New journal. The clone has another file ID and another path. A notice names the related journal. **Histories are never merged**, not even when content and root are equal. |
| Copy to a new volume, then delete the original | R6 | New journal. A move across volumes cannot be told from a copy. The old journal stays under its old path. |
| Another vault written over the same path | R3 (unlocked), R4 (locked) | New journal when unlocked. Shown as unconfirmed when locked. |
| In-place overwrite with the same file ID | R2, R3, R4 | The file ID is not used for a same-path decision. Root and content decide. |
| Rollback to a copy this device saw (backup restore) | R1 | Same journal. A `vault-rolled-back` record is appended before the next record. |
| Rollback to a copy this device never saw | R2 | Same journal, no event. This is a recorded limit. |
| Two separate vaults | R6 twice | Two journals. |
| File ID reuse by NTFS after a delete | R6 | The R5 rule needs matching content or root as well as the file ID. |
| Unclear identity | R4 | Read-only view with a notice. No write. A disclosure with this evidence fails closed. |
| `akm audit` without a password | R1, R4, R5, R6 | Evidence has no root. It resolves, never changes the mapping, and never creates a file. |

- A path in the mapping is local metadata in a private folder. It is the minimum that makes the rules work. It is not in the journal.
- The mapping changes only when a binding changes (new, moved, new `sha`, detached). A read never writes it.
- The vault is never written to anchor or number anything on a read.
- Rejected: a root UUID or content hash alone (a clone copies the first, a save changes the second). A UUID written into the vault (writes older-format and read-only vaults, and roams to every clone). A file ID alone (a save replaces the file; NTFS reuses IDs).

### D3. What to record: the minimum

A record is `seq`, `ts`, `action`, `key_id` (the native entry UUID, if the action is about one key), `detail`, `prev`, `hash`.

- **Dropped:** `key_title`, `project`, `environment`. The Activity view reads the title, project and environment of a key from the unlocked vault by `key_id`. A key that is gone shows its short ID. `akm audit` runs without a password and shows IDs only.
- **Details are counts and operation IDs only.** No scope names, no variable names, no export paths. The desktop file export used to write `N keys to <path>`; it now writes `file export completed; N keys`. The CLI export already kept the destination out.
- The first record of a journal chains from `sha256("akm-journal-v1\n" + journal id)`, not from an empty string. A journal file renamed to another journal's name fails at record 1.
- Moving titles, scopes or paths into this private file would copy the problem. Not recording them is the privacy gain.
- Veto effect: to keep titles in the journal, restore the three fields. The location and identity decisions stay valid.

### D4. Permissions, storage and reparse points

Creation (Windows API, before the first byte exists):

- The folder and every file get a **protected** security descriptor: owner is the current user, and one allow ACE for the current user's SID. No inheritance.
- Files are opened with `FILE_FLAG_OPEN_REPARSE_POINT`. The folder is held open without delete sharing while an operation runs, so it cannot be swapped.

Verification, on every open of the folder, the mapping, a journal and a lock (not only on creation):

| Check | Refusal |
|---|---|
| Owner is the current user or `BUILTIN\Administrators` | owner of another account |
| The DACL exists and has only allow ACEs for the current user's SID | a null DACL, an ACE for any other SID, a deny ACE |
| The object and every existing ancestor are not reparse points (junction, symlink, mount point, cloud placeholder) | a reparse point |
| The volume is local (fixed, removable or RAM disk) and supports persistent ACLs | a network drive, FAT/exFAT |
| The open succeeds | access denied, sharing violation after retry, any other I/O error |

- Every refusal is `Error::Journal` with the cause named and **no fallback**: not to the sidecar, not to a temporary folder, not to the default when `AKM_JOURNAL_ROOT` is wrong.
- **SYSTEM and Administrators are not allowed in the DACL.** Reason: the point is to catch a changed or inherited ACL, and an extra ACE for either one would also hide a real extra ACE. Both can override an ACL anyway, so allowing them protects nothing. Veto effect: add their SIDs to the allow list. A folder that a tool created with inherited ACEs (the default for a hand-made folder) is refused with a message that names the extra ACE.
- A folder that already exists with a wrong ACL is **never repaired** automatically.
- Reads create nothing. A missing root means "no journal yet".
- The folder lives on the local disk. AKM cannot see whether a sync client watches it. The default non-roaming place is outside what the usual clients sync. Recorded limit.

### D5. The legacy sidecar: detect, explain, never touch

On every journal read (CLI and Activity), AKM checks for `<vault>.audit.jsonl` with one metadata call. It never opens, reads, moves, deletes, uploads or imports it, and never creates its `.lock`.

The notice (kind `legacySidecar`) says: the file exists with this many bytes; earlier versions wrote key titles, projects, environments and times to it; if the folder syncs, those copies may already be in the cloud; moving or deleting this file does not remove copies that synced; AKM did not change it and does not import it; delete it yourself after you check it. The new journal starts empty.

Rejected: an automatic move or delete (destroys data the user may want, and gives a false sense that the cloud copy is gone). Rejected: an automatic import (puts old metadata in the new file, and a hostile sidecar is untrusted input).

### D6. Damage: keep the bytes, say what, not why

AKM never rewrites, truncates, rotates or repairs a journal. A damaged journal refuses appends (so a disclosure fails closed, contract 12) until the owner deals with the file. The snapshot reports one of these, with the physical line, and no cause:

| `damage` | When |
|---|---|
| `truncatedTail` | the last line has no line terminator |
| `malformedJson` | a terminated line is not a record (not JSON, wrong shape) |
| `brokenLink` | the sequence number or the previous-hash link does not follow the record before |
| `hashMismatch` | a record's hash does not match its fields |

- Reading stops at the first damaged record. Records after it are not shown or counted, even if they chain among themselves. An altered or removed early record therefore never leaves a longer "history" that looks complete.
- The declared sequence of a damaged record stays untrusted (unit 1a rule, kept).
- A damaged mapping (`mapping.json` not valid) is reported the same way, refuses appends and is never overwritten.

### D7. The integrity claim

A hash chain stored with no independent anchor cannot prove that records are complete. Someone who can write the folder can delete the end of the log, or rewrite the whole chain, and the result is consistent. The UI and the CLI say so, as before: "Recorded chain is consistent, not authenticated. Completeness unknown."

- This unit does not add an anchor. A checkpoint in the mapping lives in the same trust boundary and adds a two-file commit order. A hash written into the vault changes the vault on every read or append, which D2 forbids and the synced file would expose to rollback games.
- M1's second sentence (deleting the end is not detected) is therefore a **documented limit**, not a fixed defect.

### D8. Failure policy (unchanged in kind, new in cause)

| Moment | Journal problem | Result |
|---|---|---|
| Before a disclosure, export or launch | location refused, mapping damaged, identity unconfirmed, journal damaged or locked out | The action fails. No secret is read (unit 1h capability), no file is written, no child starts. |
| After a commit or publication | the same | A warning. The result is non-retryable ("Do not repeat"). Contract 12. |

New causes: a refused folder, a damaged mapping, `Unconfirmed` evidence. They use the same two paths as before.

### D9. Test seam

`JournalRoot::at(path)` and `AKM_JOURNAL_ROOT` are the only ways to move the root. Windows-only ACL calls are covered by real folders in temporary directories with `icacls` and `mklink /J`. The storage check is a pure function with literal drive types and flags, because a test cannot make a network drive.

### D10. Stale readers (review 1, finding 4)

An open `Vault` holds a snapshot: the bytes it last read or wrote (`sha`) and its vault ID. The file at the path can change after that (a sync client, a restore, another program). Evidence from a snapshot whose `sha` is not the file's `sha` now is **stale**. Before this decision, stale evidence ran through R1 to R6 like fresh evidence. Reader A, open on a vault that vault B replaced, detached B's active binding by R3, and B's next record detached A's new binding in turn. A stale reader of the same vault moved an old `sha` to the front of `seen`, so the next current writer looked like a rollback.

Decision: `bind` and `locate` read the file at the path again, under the mapping lock, and compare its SHA-256 with the evidence. A different value, or a file that cannot be read, makes the evidence stale. **Stale evidence never changes the mapping.** Only the active binding at the path whose vault ID equals the evidence's vault ID can vouch for it:

| Stale evidence, and the active binding at its path | Result |
|---|---|
| has the same known vault ID | **Observed.** The record goes to that binding's journal. `seen` and the binding are not changed and no rollback is checked. A read shows the journal as confirmed. |
| has another vault ID, has none, or there is no binding | **Superseded.** An append is refused (`the file at this vault's path is no longer the one this session opened`). A read shows no journal, `identity` unconfirmed and the standard notice. Nothing is created or changed. |

- **What the records of A go to: a refusal, not A's own journal.** A's journal is bound to a vault that is no longer at the path. To write there, the mapping would need a "detached but still writable" state, and a way to create it for a reader that never recorded anything. A refusal is fail-closed (contract 12): the reader reopens the vault. A's earlier records stay in its detached journal. They are not merged, and B's reads list that journal in the `earlierJournal` notice.
- Only evidence of an unlocked vault (with a vault ID) can be stale. Evidence without a vault ID (`akm audit`, `VaultEvidence::from_file`) is read-only by D2 and keeps R1 to R6, which show an unseen file as unconfirmed with the journal visible. Limit: a caller that appends with such evidence is not protected from the false rollback record described above. No production caller does.
- The common case is unchanged: a sync client replaces the file with a newer version of the same vault. The roots are equal, so the reader keeps recording in its journal.
- Veto effect: send Superseded to the detached binding with the same path and vault ID, and create a detached binding when there is none.
- Rejected: leaving stale evidence in R1 to R6 (the ping-pong above). Rejected: deciding by file ID (the path's file ID belongs to the file now, not to the snapshot).
- Limit: the check is a point in time. A file that changes between the check and the append is not seen. The record then goes to the journal that the check chose, which is the journal of the evidence's own vault.
- Cost: one more read and hash of the vault file per journal operation.

### D11. An owed record is written before the mapping advances (review 1, finding 5)

The rollback indication is the only record that the mapping owes: it moves the observed `sha` to the front of `seen`, and the next observation of that `sha` is then "not a rollback". `journal::record` is the only function that stores a mapping for a record. When a record is owed, it writes the records first and stores the mapping after they succeeded. If the append fails (damaged chain, journal cannot be opened, any I/O error), the mapping is as it was, and a retry writes the rollback record once, then the requested record. Every other change (a new binding, a move, a new `sha`, a replacement) is stored first: a binding without a journal file is harmless, a record without a binding is not. `journal::place`, which only creates a binding for tests, never consumes an owed record. The mapping lock is held across the append only when a record is owed.

- Limit: if the append succeeds and storing the mapping fails, or the append fails at the final `sync` after its bytes were written, the retry writes the indication again. A second `vault-rolled-back` record is true (the vault was in that state at both attempts). A missing one is not.

## 3. Failure cases

| Case | Result |
|---|---|
| Root has an extra ACE (any other SID, or a deny ACE) | `Error::Journal`; nothing written; no sidecar |
| Root, or an ancestor, is a junction | refused |
| Root is a file, or cannot be created | refused |
| Root denies the current user | refused |
| Fresh synced vault folder | only the vault and its backups and lock; no `*.audit*` file and no title, project or environment text in any file there, after every journaled action |
| Legacy sidecar exists | byte-identical and same timestamps after every command; notice shown |
| Two threads or processes create the first journal for different vaults at once | both bindings survive; no lost update |
| Many appends and reads at once | one chain, sequence numbers `1..n`, reads always consistent |
| Truncated tail, malformed line, broken link, altered early record | reported by kind; bytes unchanged; append refused |
| KDBX 3.x, 4.0 and read-only vaults | journaled; vault bytes and read-only attribute unchanged |
| Journal fails before a disclosure | action refused |
| Journal fails after a commit | warning, "Do not repeat" |

## 4. Limits

- Completeness and authenticity of records are not proved (D7).
- A rollback to a vault version that this device never saw is not detected (D2).
- A move across volumes starts a new journal (D2). The old journal remains and is listed in a notice.
- A sync client that watches `%LOCALAPPDATA%` is not detected (D4). Backup tools that run as SYSTEM can read the folder.
- Moving the log does not remove copies of the old sidecar that already synced (D5).
- A Windows local administrator, or any process of the same user, can read, change or delete the folder. The unit protects against sync and other accounts, not against malware in the user's account (see the trust boundary in [PROJECT.md](../PROJECT.md)).
- The proof script `tools/verify_windows_app.ps1` was edited to find the journal in the new place and was **not run** in this unit (no GUI run without the owner). The journal-export filter failure (1A-A2-04) is not investigated here.
