# Unit 1a review, round 7 (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: branch `fix/unit1a-sound-findings` at `1d08581` (application source `8e1d34d`), the narrowed unit 1a scope
- Build hashes checked by the reviewer (all matched STATE.md): `akm-desktop.exe` 9ab354a8…b522, `akm.exe` 051a3b71…9b1c, MSI 93c786d4…4727, NSIS e63ccc9b…fabc
- Verdict: **Source acceptance: NO. Work unit complete: NO.** 4 findings (1 P2, 3 P3), recorded as 1A-R7-01 to 1A-R7-04 in FINDINGS.md. This is the seventh rejection of unit 1a, after the owner's extra fix round. Patching stops; the result goes to the owner (STATE.md).

The text below is the reviewer's report, copied by hand. Typographic quotation marks and apostrophes became plain ASCII, and the reviewer's links to local files became plain text with the repository path. Other differences cannot be excluded.

---

## Ranked findings

1. **1A-R7-01, P2. A completed CLI mutation can exit 101 when its output pipe is closed.**
   **By probe:** With the pinned release CLI and a fake vault, `add` committed a fake key while a damaged journal caused a post-commit warning. With stderr open, it exited 0; with stderr closed, it exited 101. Reopening the vault showed the key in both cases. A separate fake-data probe produced exit 101 after a committed `add` when stdout was closed.
   **By inspection:** main.rs (`crates/akm-cli/src/main.rs:493`) still uses `eprintln!` for mutation warnings, and the success messages use `println!`. This pre-existing caller remains relevant to the changed warning path and violates contract 12: automation can interpret a committed change as failed and repeat it. Add regressions for committed mutations with a failed journal append and closed stderr, and for closed stdout after commit. Require a successful, non-retryable exit.

2. **1A-R7-02, P3. A successful rollback leaves an earlier clipboard lease active.**
   **By probe:** The actual `ClipboardLease` with a fake backend copied A, then attempted B, whose publication failed and rolled back successfully. The fake clipboard was empty, but A's lease remained. A later busy cleanup returned `clipboardUncertain`. **By inspection:** clipboard.rs (`desktop/src-tauri/src/clipboard.rs:99`) returns on `rolled_back` without relinquishing the pre-existing lease; session.rs (`desktop/src-tauri/src/session.rs:782`) can turn its later cleanup failure into an application warning. Add a Session-level regression that starts with an owned lease, rolls back a second publication, and requires no lease, cleanup retry or new warning.

3. **1A-R7-03, P3. A retained clipboard warning promises retries after they have stopped.**
   **By inspection:** clipboard.rs (`desktop/src-tauri/src/clipboard.rs:17`) says "It will try again." After a successful retry or relinquishment to a newer user copy, session.rs (`desktop/src-tauri/src/session.rs:798`) ends the episode but keeps that warning unchanged until acknowledgment. The user can therefore see a future retry promised when AKM no longer owns a lease. Preserve the warning's history while making its retry status truthful. Test both successful clearance and preservation of a newer identical copy.

4. **1A-R7-04, P3 coverage gap. The CLI's completion-append failure after writing the record is not covered.**
   **By inspection:** The protocol (`docs/design/unit1a-protocol.md:127`) requires both failure before the completion write and failure after the write at sync. The CLI unit test (`crates/akm-cli/src/main.rs:796`) covers an invalid parent path and a successful append; the closed-stderr regression damages the journal before the completion append. The core and desktop sync-failure tests do not exercise the CLI completion caller. Add a CLI regression that injects failure after the completion line is written, verifies that the line can remain, and requires the warning and exit 0.

## Source acceptance: NO

**By inspection and probe:** The CLI can report failure after a committed mutation, and the clipboard warning lifecycle still has the two defects above. The required CLI after-write sync-failure case also lacks regression coverage. Patching stops under the project's review rule; the result goes back to the owner.

## Work unit complete: NO

**By inspection:** The owner deferred R4-04. Even with source acceptance, unit 1a remains incomplete until its required real-window proof passes. R4-04-SCRIPT's observation gaps remain open.

## Finding dispositions

The following are **by inspection**, except where a probe is stated. "Fixed" describes the inspected source seam; no finding closes while the unit fails review.

| Finding | Disposition |
|---|---|
| R4-01 | The protected-backup and wording fixes are supported. Completion is partial because 1A-R7-01 can turn a committed operation with a backup warning into exit 101. |
| R4-03 | Partial. Immediate cleanup failures reach episode accounting, but 1A-R7-02 and 1A-R7-03 leave warning behavior inaccurate. |
| R4-04 | Deferred proof gap. |
| R4-04-SCRIPT | Open design gap. |
| P2-WARN-OWNERSHIP | Fixed within the inspected source seam, with the real-webview-reload limit. |
| P2-JOURNAL-CLAIMS | Fixed within the inspected journal-display scenarios; completeness and authentication remain explicitly unknown. |
| 1A-R5-02 | The reported unreadable-sequence and newer-copy paths are addressed; clipboard warning and lease handling remain partial under 1A-R7-02 and 1A-R7-03. |
| 1A-R5-04 | The requested/completed pairing is implemented. Acceptance remains partial because the required after-write sync-failure regression is missing. |
| 1A-R5-06 | Fixed in the inspected parser and callers. **By probe:** additional escaped-name, duplicate, maximum-u64, negative-zero and trailing-JSON cases behaved as specified. |
| 1A-R6-01 | Fixed for the named export path. The separate committed-mutation callers remain defective under 1A-R7-01. |
| 1A-R6-02 | The busy-cleanup message is corrected while retrying. The retained message becomes inaccurate after retry ends under 1A-R7-03. |
| 1A-R6-03 | Partial. The named no-prior-lease case is corrected; an existing lease exposes 1A-R7-02. |
| 1A-R6-04 | Fixed in the inspected append-error path. **By probe:** audit and refused-export diagnostics identified unambiguous sequences as declared and unverified without echoing the fake record. |

**By inspection:** The recorded scenarios for 1A-R1-01, R1-02, R1-04, R1-06, R2-01, R2-04, R2-05 and R2-06 remain fixed. R1-03, R2-02, R2-03, R3-01 and R3-03 remain fixed with the recorded reload and runtime limits. R1-05 is partial under 1A-R7-01. R1-07 is fixed within the sequence scenarios above. R3-02 is partial under the clipboard findings.

## Regression and artifact checks

**By inspection:** The named regressions for R6-01 through R6-04 would fail on `30e65b6` for their stated assertions and are satisfied by the current code for those scenarios. The R5-02, R5-04 and R5-06 regressions similarly distinguish the behavior at `cfeb1b0` from the current implementation. The R4-03 immediate-cleanup tests distinguish the archived pre-fix behavior from the current episode handler. Some older clipboard interfaces require adaptation before the newer tests could compile on the old version; these are behavioral comparisons, not executed old-version test runs.

**By inspection:** `a_failed_file_export_records_only_its_request` is a preservation test, not a failing-first regression for R5-04; the earlier implementation already emitted no completion record for that failed export.

**By probe:** The checkout was clean at `1d08581d1496c81ac44415ab660a2a5d93683192`; application paths match `8e1d34d`. All 139 archived pre-unit manifest entries matched `baseline/pre-unit-1a`. The four hashes matched STATE.md before and after the probes:

- Desktop: `9ab354a814d45761f0772c3b0e90d210d42c25a45e51e962295edd3c3c3bb522`
- CLI: `051a3b71913ba486c7ca950da06e8b101d2c3de8c7bf13802973d739135a9b1c`
- MSI: `93c786d45342e03ee00597e3f37647160b65d04ecd3e52b653fe544751fc4727`
- NSIS: `e63ccc9b0275c725a7440683be961da84ee7d727806adfe7272493eb9e6fcabc`

**By inspection:** Matching hashes identify the files; they do not independently establish build provenance. The fake probes and their handoff receipt (`%TEMP%\akm-unit1a-review7-7cb6435292ba48c8b37c4e8b2fd030e1\handoff.json`) are retained. Shared-observer registration was not attempted because its required project-local artifact root and registry writes conflict with this review's temporary-folder-only write restriction.

## What I did not check

**Not checked:** I did not launch the GUI, access the real clipboard or app settings, change the Windows session, install a build, use real secrets, or run a full build or suite. I did not reproduce real Win32 clipboard or disk-sync faults, real Tauri reload or event delivery, or R4-04.

**Not fully checked:** I did not complete an exhaustive inspection of every changed test and every caller across the older parts of the full unit diff. I did not establish pre-unit preimages for the other 17 added-looking paths or deletions outside the available snapshot inventories. The named regression tests were assessed by inspection rather than run on both versions. I did not update `STATE.md` because the review brief required a read-only checkout.
