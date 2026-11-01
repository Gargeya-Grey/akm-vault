# Unit 1a review, round 8 (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: branch `fix/unit1a-sound-findings` at `116c638` (application source `44330ee`), the narrowed unit 1a scope, after the owner's "fix by class" round (option A)
- Build hashes checked by the reviewer (all matched STATE.md): `akm-desktop.exe` 0efbacf7…9a0b, `akm.exe` ea4fc82b…f8f, MSI 1aa51ae6…3411, NSIS 35f3d9e3…c509
- Verdict: **Source acceptance: NO. Work unit complete: NO.** 5 findings (2 P2, 3 P3), recorded as 1A-R8-01 to 1A-R8-05 in FINDINGS.md. This is the eighth rejection of unit 1a, after the owner's fix-by-class round. Patching stops; the result goes to the owner (STATE.md).

The text below is the reviewer's report. The reviewer's links to repository files became plain text with the repository path and line. Two links to files in the reviewer's local temporary folder (a preimage check and a handoff receipt) became "(local temporary file, not kept)". Other differences cannot be excluded.

---

**Source acceptance: NO. Work unit complete: NO.** The review found defects in the clipboard lifecycle and a disclosure-policy gap in the CLI. The owner's deferred R4-04 real-window proof also remains outstanding. Under the owner's round 8 decision, patching stops and the result returns to the owner.

## Ranked findings

1. **1A-R8-01, P2. Masked CLI output exposes secret fragments without checking the journal.**
   main.rs:341 (`crates/akm-cli/src/main.rs:341`) treats only an explicit secret field or `--reveal` as disclosure, while mask (`crates/akm-cli/src/main.rs:774`) prints the first and last four characters. **Reproduced:** with a damaged fake journal, default `get` exited 0 and printed `qwer...y789` for a nine-character fake key. A full reveal failed closed, and the journal remained unchanged. The masked display predates this round, but it exposes eight of nine characters despite contract 12's statement that logging before disclosure fails closed.
   **Closing test:** with a damaged journal and short and long fake keys, require either refusal before secret-derived bytes are printed or a mask that prints no secret characters. If fragments are intentionally exempt from the logging policy, that exception needs an explicit contract decision.

2. **1A-R8-02, P2. Copy-success messages promise cleanup that normal exit cannot guarantee.**
   KeyDetail.tsx:102 (`desktop/src/components/KeyDetail.tsx:102`) and Settings.tsx:199 (`desktop/src/components/Settings.tsx:199`) say AKM clears the copy on a normal exit. Settings also says locking clears it. **Reproduced with a fake backend:** two cleanup attempts and the drop cleanup (`desktop/src-tauri/src/clipboard.rs:171`) all encountered a busy clipboard, leaving the fake value present. Drop ignores the final error. The UI wording predates this round and remains in the changed callers.
   **Closing test:** drive the copy messages and a fake Session through busy expiry, lock and exit/drop. Require wording that describes cleanup as an attempt and tells the user that the copy may remain, without promising a warning after the process exits.

3. **1A-R8-03, P3. Early publication errors can leave the previous clipboard lease active.**
   clipboard.rs:97 (`desktop/src-tauri/src/clipboard.rs:97`) assumes a backend `Err` changed nothing. The native publisher can instead empty copy A and then return `Err` when an exclusion or empty text format fails to install. **Reproduced using the actual publisher and lease with a fake Win32 backend:** copy B failed at those stages, A's value was gone, but A's lease remained. A subsequent busy cleanup returned `clipboardUncertain`; Session warning creation follows by inspection. This includes both successful and failed rollback attempts.
   **Closing test:** use the fake Win32 publisher through Session with an existing lease, fail each post-empty setup stage, and require no stale lease, later cleanup call or new warning. Keep the existing preparation-failure case where A really remains owned.

4. **1A-R8-04, P3. The after-write completion test does not exercise the CLI warning and exit together.**
   main.rs:891 (`crates/akm-cli/src/main.rs:891`) tests `record_completed_with` directly. It confirms that a line can remain and that the helper returns warning text, but it does not verify that the CLI prints that warning and exits 0 for the injected after-write error. The separate integration test verifies the caller's warning and exit when the append is refused *before* writing. This is a coverage finding, not a reproduced incorrect exit.
   **Closing test:** inject an error after writing the completion line through the production CLI orchestration, then assert the retained line, displayed warning and exit 0. The test should fail a mutant that suppresses the warning or makes that outcome retryable.

5. **1A-R8-05, P3. CLI documentation names an event that new runs no longer emit.**
   README.md:29 (`crates/akm-cli/README.md:29`) says a run records `run-start`; the implementation records `run-requested`. **Found by inspection.**
   **Closing check:** run a fake child and compare its actual journal action with the documented name.

## Finding dispositions

These are source-seam dispositions. None closes while the unit review is rejected.

| Finding | Disposition |
|---|---|
| R4-01 | Supported within the inspected backup-retention seam and prior fake-data evidence. |
| R4-03 | The reported immediate cleanup failure reaches episode accounting. The wider lifecycle remains partial under 1A-R8-02 and 1A-R8-03. |
| R4-04 | Deferred proof gap. |
| R4-04-SCRIPT | Open design and proof-script gap. |
| P2-WARN-OWNERSHIP | Supported within the inspected source seam; real Tauri reload remains unverified. |
| P2-JOURNAL-CLAIMS | Journal state and completeness wording are supported within the inspected scenarios. |
| 1A-R5-02 | The named unreadable and newer-copy paths are supported by focused fake tests; the broader clipboard class remains partial. |
| 1A-R5-04 | Requested/completed pairing is implemented. After-write caller coverage remains partial under 1A-R8-04. |
| 1A-R5-06 | Supported by parser and caller inspection; the prior review's wider sequence probes were not rerun. |
| 1A-R6-01 | The named export warning path is supported by inspection and existing focused coverage. |
| 1A-R6-02 | The retained cleanup warning is corrected; other clipboard success text remains inaccurate under 1A-R8-02. |
| 1A-R6-03 | The named no-prior-lease rollback case passes; early setup errors expose 1A-R8-03. |
| 1A-R6-04 | Supported in the inspected append-error path. |
| 1A-R7-01 | The named closed-pipe regressions distinguish the affected CLI from the pinned release. The wider disclosure class has 1A-R8-01. |
| 1A-R7-02 | The named rolled-back-publication case is fixed; the wider error class has 1A-R8-03. |
| 1A-R7-03 | The retained warning no longer promises retries. Related copy-success wording remains inaccurate under 1A-R8-02. |
| 1A-R7-04 | Partial test coverage under 1A-R8-04. |
| 1A-R1-01 | Supported for typed warning classification. |
| 1A-R1-02 | Named full-disclosure append-boundary behavior is supported; see the masked-output exception in 1A-R8-01. |
| 1A-R1-03 | Supported with the real-webview-reload limit. |
| 1A-R1-04 | Supported for structural, non-echoing journal errors. |
| 1A-R1-05 | Uncertain post-commit wording is supported; see the CLI after-write coverage limit. |
| 1A-R1-06 | Runtime action names are corrected; the CLI README remains stale under 1A-R8-05. |
| 1A-R1-07 | Supported within the inspected declared-sequence paths. |
| 1A-R2-01 | Named full-disclosure whitespace-boundary behavior is supported; see 1A-R8-01. |
| 1A-R2-02 | Supported with the real-webview-reload and R4-04 limits. |
| 1A-R2-03 | Supported with the real-webview-reload limit. |
| 1A-R2-04 | Supported by the own-key action lookup. |
| 1A-R2-05 | Supported within the inspected backup wording and retention seam. |
| 1A-R2-06 | Supported by decimal-string IPC serialization. |
| 1A-R3-01 | Supported within mocked handoff scenarios; R4-04 remains open. |
| 1A-R3-02 | Ordinary episode accounting is supported; the clipboard class remains partial under the new findings. |
| 1A-R3-03 | Supported within the inspected reply fence; the owner-split unit 1c findings remain outside this review. |

## Regression and artifact checks

The checkout was clean at `116c6381d106c093061fab7d8438e42ca158196d` before and after the probes. All four hashes matched the supplied values before and after testing:

- Desktop: `0efbacf7120b1818860a401daf0e7bbcbeaab4100777b49edd79599450709a0b`
- CLI: `ea4fc82b0000e465380ceb77e84279c82925981de547a1c3370b947b7483cf8f`
- MSI: `1aa51ae69fc2d136fd1399c5ceb8fc9d2335fba4b71dd5b9dead25da247f3411`
- NSIS: `35f3d9e3193bdb941e90821d18e3e7a00e1ffc637ecef4b1f5e6876b7401c509`

The hashes identify the files; they do not independently prove build provenance.

The two named CLI output regressions failed against a temporary build of the `496aa23` CLI source linked to retained dependencies, and passed against the pinned release CLI. Focused clipboard tests passed for zero sequences, unreadable content, rollback and newer identical copies. The state-table test failed both a removed-relinquishment mutant and an old-warning-text mutant. A temporary workspace using the exact CLI Clippy configuration rejected all four standard print macros.

All 139 entries in the archived before-manifest matched the baseline tag. The snapshot diff showed no deletions. The archive corroborates the earlier icon hash; pre-unit preimages for the other 20 added-looking paths, and deletions outside the inspected inventories, remain unproved. The paths are recorded in preimage-check.json (local temporary file, not kept).

## What I did not check

I did not launch the GUI, use the real clipboard or app settings, change the Windows session, install a build, use real secrets, or run a full build or suite. I did not reproduce real Win32 clipboard faults, a real disk-sync failure, real Tauri reload or event timing, or interactive prompts in a real terminal. I did not exhaustively inspect every historical test and caller in the full-unit diff. The affected-version and mutant checks above do not establish failing-first execution for every older regression.

The shared observer requires a project-local artifact root and registry writes, which conflict with this review's temporary-folder-only restriction. The fake probes and task-local handoff receipt (local temporary file, not kept) are retained. No project file, including `STATE.md`, was edited.
