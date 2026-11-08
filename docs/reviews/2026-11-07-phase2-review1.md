# Phase two review 1 of 2 (units 1c, 1h, 1b, 2, 3, 4, tools)

- Model: Codex GPT-6.1 Terra, requested effort max (owner instruction, 2026-11-07: two reviews at most).
- Reviewed commit: `e9e5fb91b6c05a5034489cff8c6f48378aca822d` on `phase2/units-1c-to-4`, against `81f2689`.
- Build hashes in the brief: `akm.exe` `b43bf17b05b6526b8836d29d9cc6b8e8152a6a137b5e7e90c53179747e83fdf1`, `akm-desktop.exe` `bd67364cbe6deb4dadb75926a8e3198888dd65f07564a7ddf82925a9955ee855` (collect build of `f7e0da9`). The reviewer did not check them.
- Date: 2026-11-07.
- Result under the pass rule: **fail** (8 P2 findings), and incomplete for unit 3, unit 5 and parts of the other units.

The text below is the reviewer's, unchanged.

---

**Review 1 verdict: FAIL.** I found P2 issues in the import, saved-path, disclosure, journal, lifecycle, notification and tooling changes. All findings below are by inspection; I did not run reproductions. The review is also incomplete because I did not trace every changed file and caller in the 155-file diff.

Reviewed source: `e9e5fb91b6c05a5034489cff8c6f48378aca822d` on `phase2/units-1c-to-4`, against `81f2689`.

## Ranked findings

### 1. P2 — A hidden import form can leave native approval live

[Settings.tsx](/W:/API%20Keys%20Manager/desktop/src/components/Settings.tsx:84) closes and clears the form on `document.hidden`, but sends no native invalidation. Native presentation invalidation in [visibility.rs](/W:/API%20Keys%20Manager/desktop/src-tauri/src/visibility.rs:52) is driven by a resize observed as minimized. If the webview becomes hidden without that native transition, the confirmation remains open and its snapshot remains valid. Approving it can commit after the page has discarded the review, contrary to contract 7.

**Evidence:** By inspection. The native “hide” test calls `session.expire_presentation()` directly, so it does not establish that a frontend visibility change reaches that boundary. A real Windows route to this condition was not tested.

**Closing test:** Drive the production frontend and native command boundary. Hide the document without delivering a native minimize event while confirmation is pending, then approve. Assert that no vault change occurs and that the dispatched operation receives a truthful outcome. Also verify the applicable real-window transition with the owner present.

### 2. P2 — The saved-path ownership check has a write-after-retirement race

[settings.rs](/W:/API%20Keys%20Manager/desktop/src-tauri/src/settings.rs:89) holds the preferences mutex while calling `owns()`, but that mutex does not prevent `Session::start_open` from retiring the ticket. Opening A can pass `owns()`, pause before the file write, then opening B can start and retire A. A can subsequently write its path even though it no longer owns the session. If B fails or is cancelled before saving, A’s stale preference remains.

**Evidence:** By inspection. The existing production-command test pauses A before entering the inner preference store and lets B finish its save. It does not pause A between the ownership check and its write.

**Closing test:** Put a barrier after the real ownership check and before settings publication. Start B, have B fail or be cancelled, release A, and assert that A cannot change the pre-existing preference. Also retain the existing case where B successfully saves.

### 3. P2 — A disclosure proof is usable against a different vault

[disclosure.rs](/W:/API%20Keys%20Manager/crates/akm-core/src/disclosure.rs:74) binds a proof to an action and key IDs, but not to the vault or journal that recorded it. [Vault::disclose_key](/W:/API%20Keys%20Manager/crates/akm-core/src/vault.rs:814) accepts metadata from another vault without checking that the key belongs to the recording vault. A library caller can record a request in vault A using vault B’s key metadata, then use that proof with `vault_b.secret(...)`. B’s journal can be damaged or unavailable and its secret still returns. Proofs are also reusable after the original disclosure.

**Evidence:** By inspection. I did not find a current CLI or desktop caller that deliberately crosses vaults this way; this is a failure of the new public capability boundary itself.

**Closing test:** Through the public core API, use two fake vaults and make B’s journal unavailable. Show that a proof recorded by A cannot disclose B’s key or environment plan. Test proof reuse after a new disclosure request or rotation, and distinguish key and environment actions.

### 4. P2 — A stale reader can detach and split another vault’s journal

[Vault::audit_log](/W:/API%20Keys%20Manager/crates/akm-core/src/vault.rs:917) supplies the fingerprint and root of its open snapshot, while journal resolution uses the current path. Suppose reader A remains open on vault A; the file at that path is replaced by vault B; B opens and records an event; then A reveals a key from its independent snapshot. [journal::bind](/W:/API%20Keys%20Manager/crates/akm-core/src/journal.rs:564) interprets A’s stale root as another replacement, detaches B’s active binding and creates a new one. B’s next append can detach that binding in turn. The journals are retained, but the current Activity view no longer has a continuous history for either vault.

**Evidence:** By inspection. The replacement test stops after B’s first append. The concurrency tests use distinct stable paths, so neither covers an old reader operating after replacement.

**Closing test:** Keep two real `Vault` snapshots open across same-path replacement. Alternate journaled disclosures from the stale and current readers, including separate processes. Assert that stale evidence cannot detach or rebind the current vault, and that each vault’s earlier records remain discoverable without merging histories.

### 5. P2 — A failed append can permanently consume a rollback indication

[journal::bind](/W:/API%20Keys%20Manager/crates/akm-core/src/journal.rs:571) moves the observed fingerprint to the front of `seen` and stores the mapping before [AuditLog::append](/W:/API%20Keys%20Manager/crates/akm-core/src/audit.rs:430) writes the `vault-rolled-back` record. If that append fails, for example because the chain is damaged or its file cannot be opened, a later retry sees the rolled-back fingerprint as newest and omits the rollback record. The mapping has advanced despite the promised record never being written.

**Evidence:** By inspection. The rollback test covers only a successful append.

**Closing test:** Create two observed fake versions, restore the first, force the first journal append to fail after binding, restore journal access without changing the vault, and retry. Require one rollback indication followed by the requested record, without altering damaged bytes.

### 6. P2 — Rotation timestamps ignore newer foreign history

The clock floor for update, rotate and import replacement considers only the current entry’s last modification and recorded secret-change time. See [vault.rs](/W:/API%20Keys%20Manager/crates/akm-core/src/vault.rs:365). It does not consider all history versions or location times, although [unit 2’s design](/W:/API%20Keys%20Manager/docs/design/unit2.md:61) promises that ordering.

For example, a foreign editor can leave a history version with a future last-modification timestamp after a clock rollback. AKM can rotate at an earlier time. The preserved old version then satisfies `modified >= recorded` in [secret_change_of](/W:/API%20Keys%20Manager/crates/akm-core/src/vault.rs:1179), so the rotation AKM just committed is reported as “changed outside” and its due time becomes unknown.

**Evidence:** By inspection. The clock rollback test uses AKM’s own sequential timestamps, not a future foreign history version.

**Closing test:** Import a real KDBX fixture with a future-dated foreign history version and an older current entry. Rotate, update and import a new secret with a controlled earlier clock. Assert ordered timestamps, preserved history, a known new secret-change time and the correct rotation due instant.

### 7. P2 — A notification can be handed to Windows after lock

[reminders.rs](/W:/API%20Keys%20Manager/desktop/src-tauri/src/reminders.rs:213) checks `session.snapshot()` once, then separately acquires the reminders mutex and calls the notifier. A lock can occur while the command waits or before `show`; the count-only toast is still submitted after lock. It can also describe a prior session if another vault has since opened. This is acknowledged as a limit in the design, but it conflicts with the unit’s stated “never while locked” acceptance condition.

**Evidence:** By inspection. The existing locked-session test checks a session already locked before the command starts; it does not interleave a lock after the snapshot.

**Closing test:** Use a barrier in the production notification path after session validation, lock or replace the session, then release the barrier. Assert that no notification is handed to the notifier. Separately prove the final Windows delivery behavior with the owner present, or obtain an explicit change to the acceptance condition.

### 8. P2 — The KeePassXC fetch tool can recursively delete unrelated data

[fetch_keepassxc.ps1](/W:/API%20Keys%20Manager/tools/fetch_keepassxc.ps1:60) accepts any destination outside the repository and recursively removes the whole nonempty destination with `-Force`. A destination that is an archive parent or other shared folder can therefore lose unrelated evidence. The removal occurs before the download and digest verification, so a network or verification failure can leave the old contents gone without a replacement.

**Evidence:** By inspection. I did not execute the destructive path.

**Closing test:** In a newly created fake directory, exercise a destination containing unrelated sentinel files and a simulated download or digest failure. Require those files to survive. Test drive roots, archive parents and reparse-point destinations, and restrict replacement to an identified tool-owned artifact.

### 9. P3 — The threat model describes superseded behavior and tests

[THREAT_MODEL.md](/W:/API%20Keys%20Manager/docs/THREAT_MODEL.md:105) still cites removed import-token tests, says the parser has no size limit, and lists several names as unblocked that the new reserved-name table blocks. Its rollback section also says AKM does not detect rollback without distinguishing the new journal’s known-version indication.

**Evidence:** By inspection against the current parser, reserved-name implementation and journal design.

**Closing test:** Review every threat-model claim and named test against this head. Add a documentation check for stale test references where practical, and state the actual limits of rollback detection and the blocklist. This P3 needs a named later unit before a provisional source pass can become final.

## Source acceptance and work-unit verdicts

| Subject | Source acceptance | Work unit complete | Verdict |
|---|---|---|---|
| **1c** | **NO, fail** | **NO** | Findings 1 and 2 block acceptance. I checked the claimed fixes for R4-02, 1A-R5-01, 1A-R5-03 and 1A-A2-01 only to the extent described above; the native import dialog remains unproved. |
| **1h** | **NO, fail** | **NO** | Finding 3 breaks the capability’s stated vault/journal binding. The clipboard publisher and lifecycle received partial inspection, not the required complete caller review. Owner-present clipboard proof remains unrun. |
| **1b** | **NO, fail** | **NO** | Findings 4 and 5 block P2-JOURNAL-IDENTITY and the rollback portion of the journal decision. P2-JOURNAL-OUTCOMES received partial inspection; full ACL and reparse-point race coverage was not established. |
| **2** | **NO, fail** | **NO** | Finding 6 blocks the claimed H9 and P2-HISTORY-SEMANTICS closure. M7 and all foreign-data preservation cases were not independently established. |
| **3** | **NO, incomplete** | **NO** | I established no separate P2 in the inspected unit 3 code, but did not verify the complete scope or required evidence for P2-TIME-REFRESH and 3-SA-01. |
| **4** | **NO, fail** | **NO** | Finding 7 conflicts with the stated locked-notification condition. Real toast delivery, lock-screen behavior and real screen-reader behavior remain unproved. |
| **A1–A3 and tools** | **NO, fail for the fetch tool; otherwise incomplete** | **NO** | Finding 8 blocks the fetch tool. Finding 9 concerns A1. I did not independently rerun the no-network or dependency checks, nor fully review every tool and test. |
| **5** | **NO, incomplete** | **NO** | I did not independently run or fully review the second-reader suite or perform the required full rescore. |

## Coverage and limits

I verified the checked-out HEAD and inspected the contracts, designs, relevant source paths and selected tests. `git diff --check` reported whitespace in dotenv fixtures, including the CRLF and trailing-space fixtures. I did not classify those fixture bytes as a defect.

I did not run tests, build a binary, launch the GUI, touch the clipboard or alter settings. The check results in the brief are planner-reported evidence, not independent results from this review. The existing `target/release/akm.exe` hash did not match the brief’s reported build hash, so I did not use that binary as evidence for the reviewed build.

I did not complete the requested whole-diff and every-caller inspection. In particular, clipboard failure interleavings, export publication, the remainder of the CLI and frontend, real-reader tool coverage, and all earlier P0–P2 regression evidence require further review. Owner-present GUI, clipboard, OS lock and notification proofs remain unrun. No repository files or scratch artifacts were created or changed.
