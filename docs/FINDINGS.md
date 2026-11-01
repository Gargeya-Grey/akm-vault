# Findings

Updated 2026-11-01. This file lists every review finding that is not closed, and the work unit that owns it. The [baseline review](reviews/2026-10-30-baseline.md) is the only scored review. New raw reviews are committed in [reviews/](reviews/). The earlier reports are in Git tag `archive/pre-cleanup-2026-11-01`, under `docs/history/reviews/` (phase one), `docs/reviews/` (calibration) and `docs/work/phase2-1a/reviews/` (unit 1a rounds 1 to 4). The standalone verdict of the first phase-one correction review was never found. `PHASE1.md` in that tag records its outcome.

## Status words

- **Fixed, not accepted:** the source has a fix and a regression test, and no review has accepted it.
- **Open:** the problem is still in the source, or a design decision must come first.
- **Partial:** part of the problem is fixed, and the remainder is listed.
- **Gap:** a proof is missing. A gap is not a defect.

## Unit 1a: warning ownership and honest journal outcomes

| ID | Severity | Status | Problem and the test that closes it |
|---|---|---|---|
| R4-01 | P2 | Fixed, not accepted | Backup pruning could delete the dated backup that was just made, when 5 older backups had names that sort later because of clock skew. The app still said a backup was kept. Test: 5 later-sorting backups and a blocked `.bak` refresh. The exact pre-delete vault must stay recoverable, the delete must still succeed, and the warning must match the actual result. |
| R4-02 | P2 | Fixed, not accepted | A delayed `vault-locked` event from session A could make the UI show "locked" while the native session B stayed open. Fix: a native session ID for each open, carried by status replies and lock events. Test: old A events at each point of B's open, and real B locks at the same points. |
| R4-03 | P3 | Fixed, not accepted | When a clipboard copy partly failed and the immediate cleanup also failed, no app warning was recorded. Test: a fake backend with a partial copy and a failed immediate clear, then a recovery, then a second episode while the first warning is still pending. Each episode must get its own warning ID. |
| R4-04 | Proof gate | Gap | No real-window test has shown that a delayed result survives hide and navigation. See the pass conditions below. |
| P2-WARN-OWNERSHIP | P2 | Fixed, not accepted | A post-commit journal warning from vault session A could appear as a warning for session B. Rounds 1 to 4 tested this area. |
| P2-JOURNAL-CLAIMS | P2 | Fixed, not accepted | The journal UI claimed "verified" and "complete" when it could not prove that. Missing, damaged, consistent-but-unauthenticated and incomplete logs now show different states. |

### R4-04 pass conditions

Run it only after the owner confirms in chat that they are ready to watch. Each confirmation allows one attempt.

**Before the launch:**

1. Check that the application source equals the build's source: `git diff --quiet cfeb1b0 -- crates desktop tools Cargo.toml Cargo.lock` must exit 0, and `git status --porcelain -- crates desktop tools` must print nothing.
2. Check all four build hashes against [STATE.md](STATE.md#unit-1a-build-under-test) with `Get-FileHash -Algorithm SHA256`. Any difference stops the attempt.
3. Create a new folder `W:\AKM-archive\proofs\<UTC time>-r4-04\` that did not exist before. Record the hashes there. The script deletes its own working folder after a pass, so this folder is the record.

**The launch,** with the full output copied to that folder:

```powershell
./tools/verify_windows_app.ps1 -DesktopExe target/release/akm-desktop.exe -CliExe target/release/akm.exe -OutcomeOnly -OwnerReadyForVisibility *>&1 | Tee-Object -FilePath W:\AKM-archive\proofs\<UTC time>-r4-04\output.txt
```

**The pass conditions.** R4-04 passes only if all of these hold, with fake data:

1. While the proof's own fake-journal lock delays a fake import, the owned AKM window really minimizes (`IsIconic`) and restores.
2. The import dialog and the busy state expire locally before the lock releases. The protected detail view expires. The user can navigate away.
3. Exactly one path-free application outcome is kept, with its acknowledgment. The CLI shows exactly one scoped fake import.
4. A native-approved exact-path file save succeeds. A native cancellation and a delayed journal error before disclosure each give the exact output or refusal.
5. Dismissal works, and no dismissed outcome comes back: not the success, the cancellation or the error.

**What the script checks for each condition.** `tools/verify_windows_app.ps1 -OutcomeOnly` has these checks (branch `test/r4-04-proof-checks`):
- Condition 2: the import dialog and Settings busy state, and a revealed Key Detail secret that must be gone after a real minimize and restore (`Test-KeyDetailExpiry`).
- Condition 3: `Assert-OutcomeRegion` counts the outcome records and requires exactly one.
- Condition 5: `Assert-NoOutcomeResurrection` covers success, cancellation, error and pending texts, and requires the outcome region to be gone.

`-OutcomeRehearsal` runs the Key Detail, cancellation, count and no-resurrection checks on the real GUI with Hide and a native cancel instead of a minimize. It passed on the pinned build on 2026-11-01 (output in `W:\AKM-archive\proofs61101T114352Z-outcome-rehearsal\`). A rehearsal never passes R4-04. The count check has not yet seen a case with two records.

Record the build hash, the result and the cleanup. On any failure, stop, keep the evidence, and return to the owner. The proof does not test Windows lock or suspend.

### Rounds 1 to 3

These are fixed in the source and remain **not accepted**. The scenario and the acceptance check for each are in `git show archive/pre-cleanup-2026-11-01:docs/REVIEW_LEDGER.md`, lines 84 to 152.

| ID | Severity | Subject |
|---|---|---|
| 1A-R1-01 | P2 | Warning classification: backup and journal warnings had one shared type |
| 1A-R1-02 | P2 | Unsafe append boundary: a final record with no newline joined the next one |
| 1A-R1-03 | P2 | Remount loss: delayed notices were lost between mounts |
| 1A-R1-04 | P2 | Malformed-record errors echoed record content |
| 1A-R1-05 | P2 | Uncertain journal outcome reported as "not recorded" |
| 1A-R1-06 | P3 | Legacy `*-start` action names still emitted |
| 1A-R1-07 | P3 | The offending sequence number was not shown |
| 1A-R2-01 | P2 | Whitespace append boundary (form feed and other non-JSON whitespace) |
| 1A-R2-02 | P2 | `completedWhileLocked` receipts lost between roots |
| 1A-R2-03 | P3 | Application warnings lost on handoff |
| 1A-R2-04 | P3 | Inherited action keys such as `__proto__` crashed Activity |
| 1A-R2-05 | P3 | Delete promised a `.bak` backup. R4-01 covers the rest of this. |
| 1A-R2-06 | P3 | Sequence numbers above 2^53 lost precision in IPC |
| 1A-R3-01 | P2 | A dispatched import outcome was lost on hide and navigation |
| 1A-R3-02 | P3 | Clipboard cleanup episodes shared one warning ID. R4-03 covers the rest of this. |
| 1A-R3-03 | P3 | Stale replies could return session A's path to obsolete callers |

The round 4 review supported the R3-01 and R3-03 fixes, and the ordinary R3-02 cases, within its examples. A real Tauri webview reload was never tested separately. The browser reload tests use mocked IPC.

## Later work units

| ID | Severity | Status | Unit | Problem |
|---|---|---|---|---|
| M1 | Medium | Open | 1b | The journal `<vault>.audit.jsonl` is plain text beside the vault. If that folder syncs, the cloud provider gets key titles, projects, environments and times. Deleting the end of the log is not detected. |
| P2-JOURNAL-IDENTITY | P2 | Open (design) | 1b | Before the journal moves, decide its location, how it identifies a vault (rename, clone, replace, roll back, two vaults), how `akm audit` works without a password, and the ACL and reparse-point rules. |
| P2-JOURNAL-OUTCOMES | P2 | Open (design) | 1b | Keep damaged bytes, and report damage without claiming its cause. Keep logging that fails closed before a disclosure. After a commit, a logging failure must be a warning only. |
| H9 | High | Open | 2 | The rotation age comes from the last edit (`model.rs`), so a notes edit resets it. There is no rotate action. |
| M7 | Medium | Open | 2 | There is no entry history and no Recycle Bin, and deletes leave no deletion record. A KeePassXC merge can bring deleted keys back. |
| P2-HISTORY-SEMANTICS | P2 | Open (design) | 2 | The pinned `keepass` 0.17.0 tracked APIs add history even on a no-op edit, and they use one-second times. Decide the rules for no-ops, fast edits, clock rollback and restore before you build. |
| M2 | Medium | Partial | 2 or later | The 12-character minimum for new vaults is done. There is no safe change-password workflow. It needs its own plan. |
| P2-TIME-REFRESH | P2 | Open | 3 | The expired and rotation-due states are calculated when the list loads. They can go stale while the Keys view stays open. A refresh must not count as user activity. |
| M5 | Medium | Open | 3 | The dotenv parser has no defined external profile for comments, quotes, multiline values and BOM. For example, `TOKEN=abc # note` keeps the comment in the value. |
| L1 | Low | Partial | 3 | `ApiKey` and `ApiKeyInput` derive a `Debug` that includes the secret, and `Vault::list` loads secrets for callers that need only metadata. |
| L2 | Low | Partial | 3 | UUID and exact-title matching come first, and ambiguous matches are refused. The fallback search still matches notes and URL. |
| L3 | Low | Open | 3 | The CLI stores a date-only expiry at 00:00Z and the GUI at 23:59:59Z. |
| L6 | Low | Partial | 3 | The frontend rounds expiry days up. Rust still truncates rotation days. |
| M6 | Medium | Partial | 1a, 1b | Post-commit journal failures are warnings, and the CLI logs reveals. Unit 1a adds damaged and missing journal states. Malformed-tail handling belongs to unit 1b. |
| M13 | Medium | Open | 4 | Reminders appear only inside the app. Correct rotation data and a notification privacy policy come first. |
| M12 | Medium | Partial | 4 | Shared focus handling exists. Check focus for every modal, including import. On the Keys screen, each key row's accessible name or description must announce its expiry and rotation status. Today the row button has a title-only label (`KeysView.tsx`). The Keys screen still has recorded layout debt. |
| P2-GUI-PROVENANCE | P3 | Gap | None | The raw output of an earlier partial GUI run was recovered, but which binary it ran is unknown. That run failed after its creation, import, clone and upgrade passes. Do not use those passes as evidence for any identified build. |
| H6 | High | Partial | Owner test | Idle lock, session lock and suspend handlers pass with fake clocks and synthetic messages. Real OS delivery and a real 10-minute idle wait are not tested. |
| P1 | Premise | Partial | 5 | Read-back checks and real KeePassXC tests exist. A second independent reader (kdbxweb) and a professional review do not. |
| P4 | Premise | Partial | Release | Signing, an updater, `SECURITY.md`, installation tests and a professional review are missing. CI is configured and has never run. |

## Closed

These baseline findings are fixed. Phase-one reviews accepted the fixes within their scope.

- **C1** Child processes get no `AKM_*` variable, `--password` is removed, and the vault is released before launch. The parent process can still hold `AKM_PASSWORD`.
- **C2** Import matches only the exact project, environment and name. Empty values are refused. The user reviews and confirms each import.
- **C3** KDBX 3.x and 4.0 vaults open read-only until an explicit upgrade, which keeps the original as a backup.
- **H1** Changes apply to a candidate copy. A failed save leaves the open vault unchanged.
- **H2** Entries are identified by the native KDBX UUID.
- **H3** Clipboard history, cloud and monitor exclusions are set. Cleanup checks ownership. A crash or a third-party clipboard manager can still keep a copy.
- **H4** Readers use snapshots. Writers take a short lock.
- **H5** `.cmd` shims are resolved explicitly.
- **H7** Export and run need a project and an environment, or an explicit `--all`.
- **H8** Each save is decrypted and compared before it replaces the vault. Five dated backups are kept. The same library does both the save and the check.
- **M3** The fingerprint is checked again just before replacement. A cloud client that is not AKM can still race that check.
- **M4, M8, M9, M10, M11, L4, L5** The username is preserved, the native save dialog and file ACLs are checked, group lookup ignores case, `--output` writes UTF-8, the window asks for capture protection, failed writes clean up, and reserved variable names are refused.
- **P3** Every artifact must now match its source. That is a process rule, not a guarantee.
- **P2** (thin differentiation) was answered by framing, not by code. AKM is a narrow personal Windows workflow.

## Adding a finding

Give it a stable ID, a severity, the commit it was found on, the file and line or the public seam, the exact scenario, and the work unit that owns it. Close it only as AGENTS.md says. Record a disputed finding as disputed, with the evidence.
