# Cleanup review, round 2 (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: `git diff 35c8c55..399cab9` and tag `baseline/pre-unit-1a`, on branch `chore/clean-project-structure`
- Verdict: REJECT, 6 new findings. This is the second rejection of the cleanup work unit. Under AGENTS.md, the review loop stops here: the documentation fixes follow in the next commit, with no third review, and finding 3 (the R4-04 proof script) goes to the owner as a design decision (STATE.md).

The text below is the reviewer's report, copied by hand. Typographic quotation marks and apostrophes became straight, en dashes in line ranges became hyphens, and the reviewer's links to local files became plain `file:line` text. Other differences cannot be excluded.

---

## Disposition of the 12 findings

| # | Status | Evidence |
|---|---|---|
| 1 | **Fixed** | TESTING.md:53 requires process identity, current settings ownership, preservation on uncertainty, and byte-for-byte restoration checks. |
| 2 | **Fixed** | .gitignore:59 restores the named patterns; AGENTS.md:38 restores staged-diff inspection. There is a separate push-inspection gap below. |
| 3 | **Partly fixed** | REVIEWING.md:21 supplies a much better baseline diff and frozen contracts. It does not explain that a path shown as added can predate unit 1a, or require the archived candidate manifest to be checked. |
| 4 | **Partly fixed** | STATE.md:38 moves review after the attempt or an owner decision. It still permits review merely because a failed attempt finished, before the failure is dispositioned. |
| 5 | **Partly fixed** | FINDINGS.md:23 restores detailed R4-04 criteria and the required arguments. The prescribed command bypasses candidate-aware checks and cannot establish every stated predicate. |
| 6 | **Fixed** | ROADMAP.md:5 limits current implementation authorization to 1a; line 18 requires approval of the 1b decisions and the unit. |
| 7 | **Fixed** | FINDINGS.md:43 lists all 16 findings as unaccepted, identifies the superseded cases, points to the old ledger, and records the Tauri reload limit. I compared the list with ledger lines 84-152. |
| 8 | **Fixed** | FINDINGS.md:86 retains the GUI provenance limitation as a gap and prohibits attributing those passes to an identified build. |
| 9 | **Fixed** | FINDINGS.md:3 identifies the archive locations and missing verdict; REVIEWING.md:52 gives future reviews a required home. The saved report has the separate provenance issue below. |
| 10 | **Fixed** | ROADMAP.md:50 and line 58 restore the requested acceptance cases; TESTING.md:70 restores frontend verification. |
| 11 | **Fixed** | AGENTS.md:39 distinguishes code defects from design, coverage, and premise findings. |
| 12 | **Fixed** | FINDINGS.md:85 and ROADMAP.md:55 identify the Keys screen and its expiry and rotation status. |

## New findings

No P1 finding was established.

1. **P2. The baseline diff is described too broadly.** STATE.md:25 calls the diff the whole unit and describes 18 files as added. REVIEWING.md:21 does not distinguish files absent from the snapshot from files introduced by 1a. The pre-phase-one `before.zip` contains `desktop/icon.svg` with SHA-256 `bfa25065...`, identical to the current file, yet the 1a snapshot omits it and the diff shows it as added. The available snapshot does not establish whether it remained present immediately before 1a. Other omitted preimages, including files absent at both endpoints, cannot be ruled out.

   **Exact fix text:** "This diff compares `main` with the 139-file snapshot. Paths shown as added were absent from that snapshot; they are not necessarily new in unit 1a. `desktop/icon.svg` existed before phase one with the current hash. Review added-looking paths as full files, check other available pre-unit records, and record any unavailable preimages or unprovable deletions."

2. **P2. The R4-04 command bypasses the frozen candidate and evidence guards.** FINDINGS.md:28 directs a bare PowerShell invocation. The archived `NATIVE_PROOF.md:7-8` instead requires the candidate-aware runner to verify compiled inputs, build records and logs, and all four artifacts before settings access, and to create a unique proof identity. The bare script does not perform those checks. On success it also removes its working folder, including `recovery.json`, at verify_windows_app.ps1:855.

   **Exact fix text:** "Before the owner-ready attempt, use the archived candidate-aware runner or a reviewed equivalent. Verify the candidate's compiled inputs, both build records and logs, and all four artifacts before settings access. Create a new immutable attempt identity and retain the command output, result, artifact and settings-restoration checks, and cleanup record without overwriting earlier evidence. Obtain fresh owner readiness for one attempt."

3. **P2. A script pass would not prove all stated R4-04 conditions.** FINDINGS.md:34 requires protected-detail expiry, exactly one outcome, and no resurrection. By inspection, verify_windows_app.ps1:371 searches outcome text but does not count outcome records. Its final no-resurrection check at line 466 omits the cancellation outcome. The script checks the import dialog and obsolete import details; it does not open and check a separate Key Detail view.

   **Exact fix text:** "Keep R4-04 open until the fake-data proof counts the retained outcome records and checks that every dismissed success, cancellation, and error outcome remains absent. State whether 'protected detail' means the import dialog and its obsolete details; if a separate Key Detail view is required, add an explicit assertion for it. A script exit alone does not prove unchecked predicates."

4. **P2. A failed attempt can trigger review before owner disposition.** STATE.md:38 says to review after the attempt finishes. That includes a failed attempt, whereas the archived `NATIVE_PROOF.md:23-25` requires failure to return to the owner and the runtime gate to be dispositioned before acceptance review.

   **Exact fix text:** "Prepare the review inputs now. After the owner-ready R4-04 attempt passes, or the owner explicitly dispositions a failed or deferred attempt, freeze the full-unit source, artifact, and evidence package and request the one fresh review. Keep source acceptance and work-unit completion as separate verdicts."

5. **P2. The push check can miss committed private evidence.** AGENTS.md:38 requires inspection of the staged diff before a push. After a commit, that diff can be empty even when outgoing history contains a vault, screenshot, or secret. A net branch diff can also miss a file added and later removed.

   **Exact fix text:** "Before each push, identify every outgoing ref and newly reachable commit or tag. Inspect their per-commit changes and any new root tree for secrets and private evidence, including files removed by later commits. `git status` and the staged diff remain required but do not replace inspection of outgoing history."

6. **P3. The saved review's provenance header is internally inconsistent.** The header (2026-11-01-cleanup-review.md:6) says a reviewer note was wrong because the remote tag exists. The report itself says only that the reviewer **could not verify** the tag, which is consistent with its existence. The header also says quotation marks were changed, while REVIEWING.md:52 requires raw text to remain unchanged. The original is unavailable, so I cannot determine whether other changes occurred.

   **Exact fix text:** "The reviewer could not verify the remote tag. A later credentialed check reported that it exists. This copy is quotation-normalized; comparison with the original is unavailable, so other differences cannot be excluded." Preserve the original verbatim if it becomes available.

## Checks run

- **PASS.** The baseline tag is parentless. Its 139 paths equal the ZIP's 139 unique file paths. Every Git blob matches the ZIP bytes and the archived SHA-256 manifest.
- **PASS.** The four release artifacts match the hashes in `STATE.md`. All 141 archived candidate inputs match both `main` and the working tree. Both archived build records and their retained logs match their recorded hashes.
- **PASS.** The requested application scope check, `git diff --stat main..399cab9 -- crates desktop tools .github`, is empty. `git diff --check 35c8c55..399cab9` is clean.
- **PASS.** The restored ignore patterns match representative paths. All seven frozen review inputs named in `REVIEWING.md` exist in the archive tag.
- **NOT VERIFIED.** The remote availability of the new baseline tag, the original raw review text, installation status, and runtime behavior.
- **SKIPPED as requested.** Builds, test suites, app launches, clipboard and settings access, window operations, edits, staging, commits, and pushes. The proof workflow's comment-edit and delegation step was also skipped under the read-only brief.

## Verdict

**REJECT.** The baseline bytes and recorded artifact identities check out, and most original findings are addressed. The review and native-proof instructions still overstate what the snapshot and prescribed proof can establish, and the revised workflow can advance after a failed owner-gated attempt without an explicit disposition.
