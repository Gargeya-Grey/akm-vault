# Findings

Updated 2026-11-01. This file lists every review finding that is not closed, and the work unit that owns it. The [baseline review](reviews/2026-10-30-baseline.md) is the only scored review. The full text of the later reviews is in Git tag `archive/pre-cleanup-2026-11-01`, under `docs/work/phase2-1a/reviews/` and `docs/reviews/`.

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
| R4-04 | Proof gate | Gap | No real-window test has shown a delayed result that survives hide and navigation. The test needs the owner present (TESTING.md). |
| P2-WARN-OWNERSHIP | P2 | Fixed, not accepted | A post-commit journal warning from vault session A could appear as a warning for session B. Rounds 1 to 4 tested this area. |
| P2-JOURNAL-CLAIMS | P2 | Fixed, not accepted | The journal UI claimed "verified" and "complete" when it could not prove that. Missing, damaged, consistent-but-unauthenticated and incomplete logs now show different states. |

The round 1 to 3 findings (1A-R1-01 to 07, 1A-R2-01 to 06, 1A-R3-01 to 03) are fixed in the source. The round 4 review supported those fixes within its examples. R4-01 and R4-03 replace the parts that remained open.

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
| M12 | Medium | Partial | 4 | Shared focus handling exists. Check focus for every modal, including import. Ledger rows must announce their status. The Keys screen still has recorded layout debt. |
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
- **P2-GUI-PROVENANCE** is retired. It concerned the phase-one evidence archive, which is now in the archive tag.

## Adding a finding

Give it a stable ID, a severity, the commit it was found on, the file and line or the public seam, the exact scenario, and the work unit that owns it. Close it only as AGENTS.md says. Record a disputed finding as disputed, with the evidence.
