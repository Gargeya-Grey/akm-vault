# Unit 1a review, round 6 (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: branch `fix/unit1a-sound-findings` at `30e65b6` (application source `bdd4233`), the narrowed unit 1a scope after the owner's split (option A, 2026-11-01)
- Build hashes checked by the reviewer (all matched STATE.md): `akm-desktop.exe` 3b74690c…f5d9, `akm.exe` 4ca64f5f…12b4, MSI cbefe79c…294b, NSIS 86655ec1…ad9c
- Verdict: **Source acceptance: NO. Work unit complete: NO.** 4 new findings (2 P2, 2 P3), recorded as 1A-R6-01 to 1A-R6-04 in FINDINGS.md. This is the sixth rejection of unit 1a. The question goes to the owner (STATE.md).

The text below is the reviewer's report, copied by hand. Typographic quotation marks and apostrophes became plain ASCII, and the reviewer's links to local files became plain text with the repository path. Other differences cannot be excluded.

---

## Ranked findings

1. **P2: A completed CLI export can exit as a failure when its completion warning cannot be printed.**
   main.rs (`crates/akm-cli/src/main.rs:393`) uses `eprintln!` after a failed completion-record append. **By probe:** after the release CLI delivered all 262,160 bytes of a fake stdout export, an induced journal failure with stderr closed caused exit **101**. With stderr open, the same failure exited **0** and printed the intended warning. A script may treat 101 as a failed export and repeat a disclosure that already happened. Add a regression that makes the completion append and stderr write fail after publication, and requires a non-retryable successful exit. Also retain the writable-stderr warning case.

2. **P2: A busy clipboard produces a misleading cleanup warning.**
   clipboard.rs (`desktop/src-tauri/src/clipboard.rs:542`) uses the message "The clipboard is busy. Try copying again" for both publication and cleanup. session.rs (`desktop/src-tauri/src/session.rs:798`) forwards that message as the retained cleanup warning. **By probe:** fake Win32 backend tests reached this path both at normal expiry and after a partial publication with failed rollback; the lease remained and fake text was still present. **By inspection:** the warning does not tell the user that cleanup is unconfirmed or that the earlier copy may remain. Add a Session-level regression for both paths that requires a cleanup-specific warning, safe retries, one ID per episode, and preservation of a newer identical user copy.

3. **P3: A successful clipboard rollback can leave a needless lease and warning.**
   clipboard.rs (`desktop/src-tauri/src/clipboard.rs:99`) installs a lease and immediately attempts another clear even when `rolled_back` says the failed publication was already emptied in its original open interval. **By probe:** if the next clipboard open is busy, the fake backend showed an empty clipboard but a retained lease and cleanup failure. Add regression cases for zero and nonzero captured sequences where rollback succeeds and the next open fails. They should return the original publication error without retaining a cleanup lease or warning.

4. **P3: Append-refusal errors display an unverified sequence without identifying it as declared.**
   audit.rs (`crates/akm-core/src/audit.rs:272`) formats `broken_at` as "sequence 7" when an append is refused. **By probe:** for valid fake JSON with `seq: 7` and an invalid timestamp, `akm audit --verify` said "declared sequence 7 (not verified)," while a refused `akm export` said only "sequence 7." The export failed closed and preserved the journal bytes. Add a regression for the append-error callers that requires the declared/unverified qualifier without echoing record contents.

## Source acceptance: NO

**By inspection and probe:** the clipboard cleanup warning and CLI post-disclosure exit behavior do not fully meet the unit's warning and journal-outcome contracts. The sequence diagnostic also remains inconsistent across callers.

## Work unit complete: NO

**By inspection:** the owner deferred R4-04. Even if the source findings above were resolved, the work unit would remain incomplete until its required real-window proof passes. R4-04-SCRIPT's two observation gaps and its open review findings remain unresolved.

## Finding dispositions

These are source-seam dispositions. None closes while this review rejects the unit.

| Finding | Disposition |
|---|---|
| R4-01 | **Fixed by inspection.** The protected immediate-original backup is excluded from pruning and checked again; the prior review accepted its fake-data seam. |
| R4-03 | **Partially fixed.** By inspection, immediate cleanup failures reach episode accounting and get distinct IDs. By probe, a busy cleanup can still produce the misleading warning in finding 2. |
| R4-04 | **Deferred proof gap.** No real-window attempt was made in this review. |
| R4-04-SCRIPT | **Open design gap.** By inspection of the recorded reviews, polling cannot establish the two required timing observations. |
| P2-WARN-OWNERSHIP | **Fixed with limits by inspection.** Operation notices retain their origin and remain separate from application warnings. A real Tauri reload was not independently tested. |
| P2-JOURNAL-CLAIMS | **Partially fixed.** By inspection, journal views distinguish states and avoid claims of authenticated completeness. Findings 1 and 4 leave outcome and sequence reporting incomplete. |
| 1A-R5-02 | **Partially fixed.** By probe, the four named fake-backend regressions pass on the current clipboard module. Findings 2 and 3 expose additional cleanup paths. |
| 1A-R5-04 | **Partially fixed.** By probe, successful fake file and concurrent stdout exports produced paired records with distinct IDs. Finding 1 breaks the promised post-disclosure exit behavior. |
| 1A-R5-06 | **Partially fixed.** By probe, `audit` labels an unambiguous damaged-record sequence as unverified and suppresses ambiguous forms. Finding 4 covers another caller. |

**By inspection:** the earlier source seams 1A-R1-01, R1-02, R1-04, R1-06, R2-01, R2-04, R2-05 and R2-06 remain fixed within their recorded scenarios. R1-03, R1-05, R2-02, R2-03, R3-01 and R3-03 remain fixed with the reload, runtime and unit 1c limits recorded in the prior review. R1-07 and R3-02 remain partial for the sequence and cleanup reasons above. The four findings moved to unit 1c and 1A-D2-09 were outside this review's fix scope.

## Regression and artifact checks

**By inspection:** the four clipboard regressions would fail on the relevant old behavior in `cfeb1b0`; the old clipboard interfaces would need adaptation to compile the new tests. **By probe:** all four pass individually against the current module in a temporary harness with fake backends.

**By inspection:** `exports_pair_requested_and_completed_records_by_operation_without_the_destination` would fail on `cfeb1b0`, which emitted no completion record or operation ID. The damaged-record core and CLI tests would fail there because typed-parse failure discarded the sequence. The Activity notice assertion would fail there because its wording lacked the unverified qualifier. Their current expected behavior follows from the inspected code; the CLI scenarios were also probed with fake data. The named CLI, core and Activity tests themselves were not rerun.

**By probe:** the checkout is clean at `30e65b6e96dccd532b4ae4f1d74ccaa3c9874399`; its scoped application files match `bdd4233`. All 139 baseline files matched the archived manifest. Of the added-looking paths, the icon's earlier hash is corroborated by the phase-one archive; the two proof modules were absent from `cfeb1b0`'s tracked tree. Pre-unit preimages for the other 17, and deletions outside the inspected inventories, were not proved.

**By probe:** `Get-FileHash` matched all four `STATE.md` hashes before and after the probes:

- Desktop: `3b74690c8fbf8528e73b4c8963bf8abf72dedaa8f3c2eb0f2b0cfcc3dd6df5d9`
- CLI: `4ca64f5f36219f542f946f5b3467eca9d7a8b203c9df5906fda87ffaff5012b4`
- MSI: `cbefe79ccab1dcc8f6d6cfd5ec7006d753a955d3d1d5c2ae79caf9709b0d294b`
- NSIS: `86655ec14eb9ec342f6f64983165f10d6fddb7f8653af6fd3d17a70f181bad9c`

The hashes identify the files; they do not independently establish build provenance. The fake probe artifacts and handoff receipt (`%TEMP%\akm-unit1a-review-eahhn6op\handoff-final.json`) are retained. The shared observer requires an in-project artifact root, while this review permitted writes only under a new `%TEMP%` folder, so registration was not attempted.

## What I did not check

I did not launch the GUI, access the real clipboard or app settings, change the Windows session, install a build, use real secrets, or run full builds or suites. I did not reproduce real Win32 clipboard faults, real Tauri reload or event timing, or R4-04. Earlier-version regression failures were assessed by inspection rather than execution. The read-only brief took precedence over the proof skill's editing and delegation routes, so I did not update `STATE.md` or create a repository review file.
