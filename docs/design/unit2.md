# Unit 2: rotation, native history and recoverable delete

- Status: implemented on branch `phase2/units-1c-to-4`. Not yet reviewed (the independent review follows the whole chain). The owner may veto any decision under "Decisions for the owner's veto".
- Tier: Tier 2, 6 units, risk: schema (a KDBX custom field, the Recycle Bin, `DeletedObjects`), delete (a recoverable delete and an explicit permanent delete), public API (crate APIs and the IPC shapes), privacy (journal actions). The sub-units: core model and clock seam, core vault rules, CLI, desktop commands, frontend, real-reader tools and docs.
- Scope: H9, M7 and P2-HISTORY-SEMANTICS ([FINDINGS.md](../FINDINGS.md)). M2 (change master password) gets a plan only (section 6). Inputs: [unit 1h](unit1h.md) (secret sinks are bound to journal records; this unit adds none) and [unit 1b](unit1b.md) (a journal record holds an id, an action and counts; this unit keeps that).
- Public API change: yes, inside the workspace crates.
  - `akm-core`: new `lifecycle` module (`Clock`, `SystemClock`, `SecretChange`, `HistoryVersion`, `RecycledKey`, `Rotation`). `ApiKey` gains `secret_changed_at`, `secret_change` and `history_count`. `Vault` gains `with_clock`, `rotate`, `history`, `recycled`, `find_recycled`, `restore`, `purge` and `purge_recycled_older_than`. `Vault::remove` now moves to the Recycle Bin. New constant `META_SECRET_CHANGED`.
  - `akm-cli`: new commands `rotate`, `history`, `bin`, `restore`, `purge`. `rm` now moves to the Recycle Bin.
  - `akm-desktop` IPC: `KeyView` gains `secretChangedAt`, `secretChange`, `historyCount`. New commands `key_rotate`, `recycled_list`, `key_restore`, `key_purge`. The frontend `View` gains `bin`.
  - Not changed: the KDBX format version, KDF and cipher, disclosure capability, journal format.

## 1. Facts the design uses

Checked in the `keepass` 0.17.0 sources (read only) and by tests.

- The tracked edit APIs (`track_changes()`) use the wall clock, store times to the second, and add a history version even when nothing changed. AKM therefore builds history itself with `History::add_entry` and stamps times from an injected clock.
- `Database::deleted_objects` is public (`HashMap<Uuid, Option<NaiveDateTime>>`). So are `meta.recyclebin_uuid`, `meta.recyclebin_enabled` and `meta.recyclebin_changed`. `Database::recycle_bin()` looks the group up by UUID.
- `EntryMut::move_to` sets `previous_parent_group` and does not set `location_changed`. A history version keeps the `parent` it had when it was saved, so it is stale after a move.
- KeePassXC reads `RecycleBinUUID`, `RecycleBinEnabled` and `DeletedObjects` (checked by `keepassxc-cli export`).
- A vault from an older AKM, or one that KeePassXC edited, has no `AKM_SECRET_CHANGED` field.

## 2. Decisions for the owner's veto

### D1. The secret-change time is a custom field, and "unknown" is a real value

`AKM_SECRET_CHANGED` holds `YYYY-MM-DDTHH:MM:SSZ` (UTC, whole seconds) as an unprotected custom field. KeePassXC keeps custom fields. `ApiKey.secret_change` is one of:

| Value | Meaning | `secret_changed_at` |
|---|---|---|
| `Known` | the field holds a time and nothing contradicts it | that time |
| `Unknown` | no field (a legacy or foreign entry) or an unreadable field | none |
| `ChangedOutside` | a history version at or after the recorded time holds another secret than the entry now: another program changed the secret after AKM wrote the field | none |

AKM never invents a date. It does not fall back to the creation or edit time. The CLI and the desktop show `unknown`. `rotation_due_in` counts from `secret_changed_at` and is `None` when the time is unknown. The text then reads "unknown", not "-" and not a number of days. A key with a rotation interval and no known time never shows "overdue" or "on schedule". The desktop shows its state as "Rotation unknown" and counts it as needing attention.

Rejected: using the last-modified time as a fallback. This is H9 itself. Rejected: back-filling the field on open. It would write the vault on a read and would still invent the date.

### D2. What counts as a rotation

A rotation is a committed change of the Password field. These set the time and keep the old version in the history: `rotate`, an `update` whose input secret differs from the stored secret, and an `import` that changes a secret. `add` sets it to the creation time.

These do not set it: a change of notes, title, tags, scopes, URL, expiry or any other metadata; an `import` that finds the same secret; an `update` that changes nothing (D4).

### D3. The rotate action

- Core: `Vault::rotate(id, Secret) -> Rotation { key, changed_at, previous_changed_at }`. CLI: `akm rotate <key>` (hidden prompt, or `--secret-stdin`; never an argument). Desktop: `key_rotate(id, secret)` from the Rotate secret dialog.
- An empty secret is refused. A secret equal to the stored secret is refused with a message that says an earlier attempt may have been saved. This is the exactly-once rule: a repeat of a rotation whose reply was lost cannot rotate twice and cannot add a history version. It needs no operation ID because the second request is a visible no-op.
- The receipt (`Rotation`, `RotationView`, the CLI output) holds the key's metadata, the new time and the previous time. It holds no secret. `list`, `get`, `history` and `bin` never print a secret value (`get` masks as before).
- Lock and cancel: the desktop command uses the existing `Access::Write` path. A rotation that commits while the session locks returns `completedWhileLocked` with the standard "Do not repeat" text and no data. The dialog cannot be closed or cancelled while the request runs, so a cancel never hides a request that may still commit. The typed value is cleared when the request ends, whatever the result.
- Journal: `rotate-completed` with the key id only (unit 1b rule). A journal failure after the vault commit is a warning, not a failure (contract 12). The CLI exit code stays zero.
- Rotating in AKM does not change the secret at the provider. The CLI and the dialog say so.

### D4. No-op detection before the library's tracked APIs

`Vault::update` applies the input to a clone of the entry first. If the clone equals the stored entry, `update` returns at once: no save, no history version, no stamp, no journal record. Otherwise it saves the old entry as a history version, writes the input, and (only if the secret changed) writes `AKM_SECRET_CHANGED`. The comparison covers every field, tag and time that `write_entry` sets, so a changed expiry or scope list is a real change.

Rejected: the library's tracked edit (history on a no-op, wall-clock seconds). Rejected: comparing only the secret (a metadata edit must still save).

### D5. Same-second edits and clock rollback

Every time that AKM writes for one key comes from `next_stamp(now, entry)`: the clock time cut to the second, or one second after the newest time already recorded for that key, whichever is later. "Recorded" is `recorded_times(entry)`: creation, last modification, location change and the secret-change field of the entry, and the same four of every history version, whoever wrote them. `update`, `rotate`, `import`, `remove`, `restore` and `purge` all use it (review 1, finding 6: before, rotate, update and import ignored history and location, so a history version that a foreign editor dated in the future made AKM's own rotation read back as "changed outside"). A floor that has no later second to use (the last second the date type holds) refuses the change with an error and saves nothing. So:

- two edits in one second get two different times and two history versions, in order;
- a clock that goes back never moves a time back, and never makes the history order wrong;
- a purge record is later than every known version of the key (D9).

The cost: after a rollback, a key's times can run ahead of the clock by the size of the rollback. This is the safe side: history order and the merge rule stay correct.

Tests use `Vault::with_clock` with a manual clock and literal times. The CLI reads `AKM_TEST_CLOCK` in debug builds only; a release build ignores it.

### D6. Native KDBX history

History is the standard KDBX `<History>` list. AKM adds one version before each real change and never edits, reorders or removes an existing version. Everything else stays as the file had it: foreign history, shared attachments, foreign groups, custom fields, entry and meta custom data, KDF and cipher.

The decrypt-and-compare step before replacing the vault (contract 2) compares history versions field by field and ignores the stale group pointer that the library keeps in a version after a move. Without this a moved entry with history failed the save check (found while building this unit; covered by two unit tests in `vault.rs`).

`history` lists versions newest first with the time and the field names that differ. It lists names, never values. A UI shows only the count.

### D7. The Recycle Bin

- `remove` moves the entry to the group named by `Meta/RecycleBinUUID`. If the vault has no such group (none, or a stale pointer), AKM creates a "Recycle Bin" group in the way KeePassXC does (icon 43, no auto-type, no search) and sets `RecycleBinUUID`, `RecycleBinEnabled` and `RecycleBinChanged`.
- If the vault sets `RecycleBinEnabled` to false, `remove` is refused and nothing changes. The owner of that setting is the user, and AKM does not turn it back on. `purge --live` is the explicit way out.
- The entry keeps its UUID, secret, history and times. `LocationChanged` is the time of the move, and it is the "removed at" time that `bin` shows. The previous group is kept in `PreviousParentGroup`.
- AKM lists only keys it recognises (an `AKM_ID` field, or a previous group named "API Keys"). Other programs' entries in the bin are left alone. A recycled key is never exported, injected, matched by `find` or overwritten by an import.
- A second delete of a recycled key is not an `rm`. The only way to delete for good is `purge` (D9).

### D8. Restore: parent group and UUID

`restore` moves the entry back to its previous group if that group still exists and is the AKM group; otherwise to the AKM group, which it creates if needed. The UUID, secret, history and times stay. `LocationChanged` gets a new stamp. Restore is refused when a live key uses the same variable name in the same project and environment (the key would break the injection rules), or when the key is not in the bin.

### D9. Permanent delete, `DeletedObjects` and retention

- `purge` removes the entry (with its history and any attachment that nothing else shares) and writes `DeletedObjects[uuid] = stamp`, where `stamp = next_stamp(now, [every time of the key])`. The record is later than every version of the key, which is the rule that merge tools use to decide that a deletion wins over an old copy.
- `purge` of a recycled key needs `--yes` without a terminal. `purge --live` deletes a live key (CLI only; the desktop `key_purge` accepts only a key that is in the bin).
- Retention is a separate explicit action: `akm purge --older-than-days N --yes` purges recycled keys whose `LocationChanged` is older than N days at that moment. Nothing runs it by itself. No timer, no startup sweep, no save hook.
- Journal: `remove-completed`, `restore-completed`, `purge-completed` with the key id; a retention purge adds the detail `retention`.

Limit, measured with the real tools (section 5): KeePassXC 2.8.0 `keepassxc-cli merge` does not apply `DeletedObjects` in either direction. A control run, where KeePassXC itself deletes a key for good in a vault that KeePassXC created, shows the same result: the merge brings the key back. AKM writes the record that the format defines, and kdbxweb (which applies records on merge) keeps the key deleted in both directions, with a control that shows the record is the cause. A user who merges an old copy with KeePassXC 2.8.0 must expect the key to come back. The desktop purge dialog says so.

### D10. Surfaces

- CLI: stays a frontend of the same core calls. No rule lives only in the CLI.
- Desktop backend: four commands (`key_rotate`, `recycled_list`, `key_restore`, `key_purge`). The mutating ones are in the frontend `mutations` set, so a reply that arrives after a lock becomes the standard completion receipt.
- Frontend: Key Detail shows "Secret changed" (a time, or "unknown", or "unknown · changed outside AKM"), the history count, Rotate secret, and Move to Recycle Bin (it replaced "Delete key"). A Recycle Bin view lists recycled keys with Restore and Delete for good (with a confirmation). The Activity view labels `rotate-completed`, `restore-completed` and `purge-completed`; `remove-completed` reads "Moved to Recycle Bin". Styles reuse the existing tokens in `desktop/src/styles.css`.

### D11. M2 is not implemented here

See section 6. Reason: the safe version needs a new crash-recovery design for the backups, and the old-password backups cannot be made safe by this unit.

## 3. Data shapes

| Shape | Where | Content |
|---|---|---|
| `AKM_SECRET_CHANGED` | entry custom field, unprotected | `YYYY-MM-DDTHH:MM:SSZ` |
| `SecretChange` | `ApiKey.secret_change` | `known`, `unknown`, `changed_outside` |
| `Rotation` | `Vault::rotate` | key, `changed_at`, `previous_changed_at` |
| `HistoryVersion` | `Vault::history` | `index`, `modified_at`, `changed` (field names), `secret_changed` |
| `RecycledKey` | `Vault::recycled` | key, `deleted_at` |
| `RotationView`, `RecycledView` | desktop IPC | the same, with `KeyView` for the key |
| `DeletedObjects` | KDBX | `uuid -> stamp`, written only by `purge` |
| journal actions | unit 1b journal | `rotate-completed`, `restore-completed`, `purge-completed`, `remove-completed`; key id only |

## 4. Failure cases

| Case | Result |
|---|---|
| Notes-only edit | saved; secret-change time unchanged; one history version |
| Edit that changes nothing | no save, no history, no record, no new time |
| Unchanged import | no change to the time; a changed secret sets it |
| Two edits in one second | two history versions, times one second apart |
| Clock goes back | times never move back |
| Save fails | no time changed, no history added (candidate snapshot, contract 1) |
| Rotation committed, then lock | `completedWhileLocked`, no secret, saved once |
| Rotation repeated with the same value | refused; no history added |
| Journal fails after commit | warning; the change stays; exit code zero |
| Legacy entry, no field | `unknown`; rotation age `unknown` |
| Secret changed by KeePassXC after AKM wrote the field | `changed_outside`; no time shown |
| Vault has no Recycle Bin | AKM creates one on `rm` |
| Recycle Bin turned off | `rm` refused, nothing changed |
| Restore into a name a live key uses | refused |
| Purge a key | entry gone, `DeletedObjects` record later than all its times |
| Merge of an old copy | see the D9 limit |

## 5. Verification with real readers

Both tools ran against `cargo build --release -p akm-cli --offline --locked`.

- `tools/verify_keepassxc.py` (KeePassXC 2.8.0 CLI). New checks: a KeePassXC-written key shows `Secret changed: unknown`; `rotate` adds one history version at the newest position and leaves the earlier ones, the attachment and the foreign key alone; a repeat is refused and adds none; `rm` puts the key in a group named "Recycle Bin" that `RecycleBinUUID` names with `RecycleBinEnabled` true; `restore` returns it to "API Keys" with the same UUID; `purge` leaves a `DeletedObjects` record. Stale-copy merges in both directions (key live in the stale copy, and key in the Recycle Bin of the stale copy) give the same result for an AKM purge and for a KeePassXC permanent delete.
- `tools/verify_kdbxweb/verify.mjs` part C and the extended part B: the same life cycle read by kdbxweb; the foreign entry (history, attachment, custom fields) is byte-equal after a move to the bin and a purge; `merge` in both directions keeps the purged key deleted, and a control without the record brings it back.

## 6. M2: change master password (plan only; left open)

Why not now. A password change can keep contracts 1 and 2 for the vault file: build the re-keyed candidate, save it to a temporary file, decrypt it with the new password and compare it to the intended database, check that the old password no longer opens it, then replace the vault atomically. The part that cannot be made safe here is the backups. Contract 2 keeps five dated encrypted backups and the `.bak` beside the vault. All of them keep the old password and the old key material after the change. Those files stay a way to open the vault with the old password.

Plan for the later unit:

1. The command shows what will stay: the `.bak` and the dated backups keep the old password until they are pruned or removed. It needs an explicit confirmation of that sentence. No part of the tool may say that the old password is "gone".
2. Offer two explicit choices, never a default: (a) re-key every retained backup in the same way as the vault (each one decrypt-and-compare verified), or (b) delete the retained backups after the new vault is verified. Without a choice the backups stay and the notice stays.
3. Crash safety: the vault replace is one atomic rename, so the vault is either old or new. A marker file (a name beside the vault, no secret in it) records "re-keying backups" so that the next open can finish or report. Tests: crash after the temporary file, after the replace, between backups.
4. The journal records `password-changed` with no password data. The desktop needs a dialog that asks for the old password again; a stale session never changes a password.
5. Tests with real KeePassXC: the new password opens it, the old does not; the retained backups follow the chosen option.

Left open as "Open (plan written)".

## 7. Limits

- A KeePassXC 2.8.0 merge with an old copy brings a purged key back (D9). AKM cannot change what KeePassXC does.
- The history holds secrets. A user who wants an old secret gone must purge the key; there is no "forget one version" action in this unit.
- Rotation times can run ahead of the clock after a clock rollback (D5).
- Purge removes the key's history with the key. Older encrypted backups can still hold it.
- AKM does not repair a vault that another program left with a `RecycleBinUUID` that names a group holding other programs' entries; it uses that group.
- Not checked: the owner's own KeePassXC GUI behaviour for merge settings. Only the 2.8.0 CLI was run.
