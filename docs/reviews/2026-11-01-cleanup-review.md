# Cleanup review, round 1 (raw)

> Note of 13 November 2026: each link to the reviewer's local checkout is written as a repository path. No other text changed.

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: `git diff cfeb1b0..35c8c55` on branch `chore/clean-project-structure`
- Verdict: REJECT, 12 findings. The fixes are in commit 399cab9.
- The reviewer could not verify the remote tag `archive/pre-cleanup-2026-11-01`. A later check with the owner's credentials (`git ls-remote`) showed that it exists.

The text below is the reviewer's report, copied by hand with typographic quotation marks and apostrophes made straight. The original message cannot be compared byte for byte, so other differences cannot be excluded. Round 2 is in [2026-11-01-cleanup-review-round2.md](2026-11-01-cleanup-review-round2.md).

---

## Ranked findings

1. **P1. Manual settings recovery lost an ownership safeguard.** `docs/TESTING.md:52` says to check the recorded process and vault before restoring settings. It does not require checking whether the *current settings* are still owned by the proof. After an interrupted run, restoring the preimage could overwrite a preference the owner changed in the meantime. The old `cfeb1b0:docs/VERIFICATION.md:104` required that ownership check and warned that a surviving PID is not proof of ownership. The harness's normal cleanup performs a current-vault check at `tools/verify_windows_app.ps1:835`, but that does not cover an interrupted manual recovery.

   **Fix text:** "Before restoring settings, verify the process executable and creation identity, and confirm that the current settings still belong to this proof's fake vault. If ownership is uncertain or settings changed independently, leave them untouched. Never guess an unknown preference. After an authorized restore, verify the saved bytes exactly."

2. **P2. The cleanup removes sensitive-evidence ignore rules.** `.gitignore:47` no longer excludes `docs/**/*.png`, `.jpg`, `.jpeg`, `.webp`, or `.txt`, nor `*-identity.json`, `runtime-cleanup.json`, and `fake-ui-names*`. `git check-ignore --no-index` confirmed that representative files for each of those patterns are now unignored. Screenshots, UI diagnostics, and process/recovery records can therefore enter a future staged commit. The old `cfeb1b0:.gitignore:55–68` excluded them. The rewrite also drops the old requirement to inspect the exact staged diff.

   **Fix text:** Restore these patterns:
   ```gitignore
   /docs/**/*.png
   /docs/**/*.jpg
   /docs/**/*.jpeg
   /docs/**/*.webp
   /docs/**/*.txt
   **/*-identity.json
   **/runtime-cleanup.json
   **/fake-ui-names*
   ```
   Add to AGENTS.md: "Do not discard unfamiliar changes. Before committing or pushing, inspect `git status` and the exact staged diff for secrets and private evidence. Ignore rules are a guard, not a secrets scanner."

3. **P2. The prescribed whole-unit review cannot be reconstructed from a Git range.** `docs/REVIEWING.md:15` asks for a commit range, while `docs/STATE.md:25` asks for the whole of unit 1a. Git was initialized after that work. `git diff b187e55..35c8c55 -- crates desktop tools .github` is empty. I verified that the retained pre-unit ZIP matches all 139 entries in its manifest, and at least 30 source paths differ between that snapshot and the reviewed commit. A reviewer following the new instructions would miss those changes and the frozen visibility/publication refinements.

   The active state also omits the exact candidate identities formerly recorded in `cfeb1b0:docs/STATE.md:65–67`. I independently matched all 141 candidate inputs to the reviewed commit and all four present release artifacts to the archived candidate, but the new review brief does not require that binding.

   **Fix text:** "For unit 1a, supply the hash-verified pre-unit `before.zip`, its `before-sha256.json`, the current candidate and artifact manifest, the original unit plan, and the visibility plan and publication contract from tag `archive/pre-cleanup-2026-11-01`. Reconstruct and review the complete before/after change, including added files. Record the reviewed source and artifact hashes. Use a Git range alone only when it actually contains the complete work unit."

4. **P2. The next action reverses the previously authorized proof and review sequence.** `docs/STATE.md:25` directs the one fresh whole-unit review before the owner-present proof. The prior `cfeb1b0:docs/STATE.md:72` and `docs/work/phase2-1a/corrections4/visibility/NATIVE_PROOF.md:17–25` placed that review after the runtime gate was dispositioned. Starting it first can consume the reserved review without the final runtime evidence. Source acceptance and runtime completion can still have separate verdicts.

   **Fix text:** "Prepare the review inputs now. After the owner-ready R4-04 attempt is completed or explicitly dispositioned with the owner, freeze the full-unit source, artifact, and evidence package and request the one fresh review. Keep source acceptance and runtime completion as separate verdicts."

5. **P2. R4-04's acceptance criteria were reduced to a generic visibility check.** `docs/FINDINGS.md:19` and `docs/TESTING.md:50` do not state the full proof needed. The archived round-four review, lines 57–63, and native proof plan, line 21, require delayed import and approved file-save outcomes, cancellation and error continuations, protected-detail expiry, acknowledgment, and retained evidence. The command shown in STATE also omits the script's mandatory `-DesktopExe` and `-CliExe` arguments.

   **Fix text:** "After fresh owner readiness, run `./tools/verify_windows_app.ps1 -DesktopExe target/release/akm-desktop.exe -CliExe target/release/akm.exe -OutcomeOnly -OwnerReadyForVisibility`. On the exact identified candidate, demonstrate actual owned-window minimization, protected-view expiry and navigation while the fake import is delayed, exactly one scoped import and its path-free acknowledgment, approved file-save success, cancellation, delayed error, and dismissal without resurrection. Record the artifact hash, result, and cleanup. Stop and preserve evidence on failure."

6. **P2. Authorization for later phase-two work is no longer stated.** `docs/ROADMAP.md:5` presents the units as a sequence, and line 18 says the owner must have *seen* the unit 1b decisions. The previous `cfeb1b0:PHASE2.md:11` and `docs/STATE.md:13` explicitly limited the existing implementation authorization to unit 1a. Seeing a design does not establish authorization to implement another unit.

   **Fix text:** "The current implementation authorization covers unit 1a only. Obtain the owner's explicit authorization before implementing another unit. For unit 1b, obtain approval of the written identity, storage, and migration decisions before implementation."

7. **P2. Sixteen unaccepted findings lost their individual scenarios and statuses.** `docs/FINDINGS.md:23` compresses rounds 1–3 into ranges and says they are fixed. The old ledger records the individual findings and says they remain unclosed pending acceptance. In particular, unsafe append boundaries, malformed-error disclosure, remount loss, inherited action keys, and sequence precision are no longer individually usable as review targets. The old ledger also distinguishes the partially superseded R2-05 and R3-02 cases.

   **Fix text:** "The following are fixed in source but remain unaccepted: 1A-R1-01 warning classification; R1-02 unsafe append boundary; R1-03 remount loss; R1-04 malformed-error echo; R1-05 uncertain journal outcome; R1-06 legacy action names; R1-07 omitted sequence; R2-01 whitespace append boundary; R2-02 completion handoff; R2-03 application-warning handoff; R2-04 inherited action keys; R2-05 delete-backup assurance; R2-06 sequence precision; R3-01 dispatched outcome handoff; R3-02 cleanup episodes; and R3-03 stale-reply fencing. R4-01 and R4-03 cover the remaining R2-05 and R3-02 scenarios. Consult `git show archive/pre-cleanup-2026-11-01:docs/REVIEW_LEDGER.md`, lines 84–152, for each scenario and acceptance check. Actual Tauri webview reload was not separately exercised."

8. **P2. An unresolved provenance gap is marked retired.** `docs/FINDINGS.md:67` retires `P2-GUI-PROVENANCE` because the archive was moved to a tag. Archiving did not bind the earlier partial GUI run to its exact binary. The old ledger, line 79, and old verification guide, line 11, explicitly mark that binding unavailable. The earlier creation/import/clone/upgrade passes must not be attributed to the final GUI.

   **Fix text:** "`P2-GUI-PROVENANCE`, P3, **Gap**. The earlier partial GUI run's raw output was recovered, but its exact binary binding remains unavailable. It failed after earlier creation/import/clone/upgrade passes and is not a final-GUI proof. Do not use those passes as evidence for an identified final executable."

9. **P2. The archive description overstates what survived, and future raw reviews have no guaranteed home.** `docs/FINDINGS.md:3` says the full text of later reviews is in two tag directories. Retained phase-one reviews are also under `docs/history/reviews/`, and the first phase-one correction's standalone verdict was already missing, as recorded in `cfeb1b0:docs/REVIEW_LEDGER.md:62`. Separately, `docs/REVIEWING.md:37` assumes a pull request keeps every non-rubric review, although the workflow does not require a PR or require its raw review to be attached.

   **Fix text:** "Retained later reports are under `docs/history/reviews/`, `docs/reviews/`, and `docs/work/phase2-1a/reviews/` in the archive tag. The first phase-one correction's standalone verdict was not found; PHASE1.md records its outcome. For each new review, retain the raw verdict verbatim with source identity and provenance in a verified private PR record or an owner-approved local evidence location, and link its location from the findings. Do not claim a report is retained until that is verified."

10. **P2. Several acceptance checks were dropped from the roadmap and verification policy.** The rewritten unit 3 and unit 4 criteria (`docs/ROADMAP.md:42`) omit controlled exact-boundary clock tests, executable CLI and ambiguous-selector regressions, and reminder cases for unknown dates, no keys, locked/hidden transitions, denied or disabled notifications, and keyboard-only paths. The old `cfeb1b0:PHASE2.md:69–85` required them. The new `docs/TESTING.md:63` also omits the former rule that frontend changes require the affected lifecycle tests, rebuilt GUI, and affected native flow.

    **Fix text:** "Unit 3 acceptance includes controlled exact expiry and refresh boundaries, executable CLI regressions, ambiguous selectors, and metadata/Debug assertions. Unit 4 acceptance includes expired, overdue, unknown-date and no-key states; locked and hidden transitions; denied or disabled notifications; accessible status; and keyboard-only paths. For frontend changes, run affected lifecycle and interaction tests, rebuild the GUI, and renew the affected native flow."

11. **P2. The new closure rule cannot describe non-defect findings.** `AGENTS.md:38` requires a failing-before-fix permanent regression for *every* finding. R4-04 is explicitly a proof gap, while journal identity and history semantics are design gates. A missing proof or a completed design decision does not necessarily have a code fix against which a regression can fail.

    **Fix text:** "Close a reproduced code defect only after a permanent regression fails on the affected version, passes after the fix, and the work unit passes review. Close a design, coverage, or premise finding when its stated decision or evidence has been verified and reviewed, with any remaining limitation recorded."

12. **P3. "Ledger" does not identify the affected screen.** `docs/FINDINGS.md:44` and `docs/ROADMAP.md:53` say ledger rows must announce status. Both the Keys and Activity components use a `ledger` class. The reported defect is specifically the Keys row button's title-only accessible name at KeysView.tsx:260 (`desktop/src/components/KeysView.tsx:260`).

    **Fix text:** "On the Keys screen, each key row's accessible name or description must announce its status, including expiry and rotation state; a title-only label is insufficient."

## Checks run

- **PASS.** `git diff --name-status cfeb1b0..35c8c55 -- crates desktop tools .github` returned no changes. The working tree was clean after review.
- **PASS.** Compared the new records with the requested old ledger, plans, state, verification guide, agent rules, project brief, README, and round-four report. The baseline review body is unchanged apart from the added introductory paragraph.
- **PASS, scoped.** Checked 32 local Markdown links across 11 tracked Markdown files, including anchors. Searched tracked source, tests, tools, and workflow text for references to the removed paths. No broken current-tree reference was found by those checks.
- **PASS, scoped.** The local archive tag peels to `cfeb1b0`. The local evidence directory contains 265 files. The pre-unit ZIP's 139 entries match its archived manifest. I did **not** independently re-hash all 265 moved files.
- **PASS, scoped.** All 141 archived candidate inputs match the reviewed commit. The present GUI, CLI, MSI, and NSIS files match their archived candidate hashes. Hashes establish identity, not runtime correctness.
- **NOT VERIFIED.** A non-interactive `git ls-remote` returned "Repository not found." That does not establish whether the private remote tag exists.
- **SKIPPED as requested.** No builds, test suites, app launches, clipboard operations, AppData/settings access, window operations, edits, staging, commits, or pushes. The proof skill's comment-edit/delegation step was also skipped under the read-only reviewer scope.

## Verdict

**REJECT.** The change preserves the application files and the main owner gates, but weakens a settings-recovery safeguard, removes evidence ignore protections, and does not provide enough information to perform the promised complete unit review or accurately track several remaining proof and acceptance obligations.
