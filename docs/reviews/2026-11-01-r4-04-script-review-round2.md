# R4-04 proof script review, round 2 (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: PR #3, branch `test/r4-04-proof-checks` at `284d666` (the fixes for round 1)
- Verdict: REJECT, 10 new findings (6 P1, 3 P2, 1 P3). This is the second rejection of the proof script work unit. Under AGENTS.md, patching stops here, and the design question goes to the owner (STATE.md). The script is not ready for the owner-present attempt. R4-04 stays unproved.

The text below is the reviewer's report, copied by hand. Typographic quotation marks became straight, and the reviewer's links to local files became plain `file:line` text or file names. Other differences cannot be excluded.

---

## 1. Disposition of the original findings

| # | Disposition | Evidence |
|---|---|---|
| 1. Dismissed outcomes can return before a later dismissal | **Partly fixed.** Later snapshots check dismissed patterns, but a recurrence can still arrive between the last sample and the dismissal click. A synthetic probe reproduced that sequence using the actual `Click-Control` function. |
| 2. Correct asynchronous dismissal or Hide can fail immediately | **Fixed for the reported checks.** Dismissal and masking now wait for the transition before checking stability. The self-test catches a mutant that returns before waiting. Separate immediate busy-state checks remain a new finding below. |
| 3. UI Automation errors can count as absence | **Partly fixed.** Reader exceptions propagate, and the wait and stability helpers require a sentinel. The pre-click sample does not require one, so an incomplete tree can still authorize a later click. |
| 4. Key Detail observation ends before a late disclosure | **Partly fixed.** The script adds a displayed-secret check and a delayed reveal across minimize. It still does not establish that the delayed reveal response was delivered before closing Key Detail. |
| 5. Documented preflight rejects the change | **Fixed.** Both documented comparisons exited 0 on this tree, the scoped status was clean, and the self-test passed. |
| 6. Outcome count is not established as a record count | **Partly fixed.** The retained rehearsal reports one and two cancellation records, and the exactly-one check rejected two. The snapshot converter nevertheless ignores a second outcome region. |
| 7. `Wait-Text` joins unrelated names | **Partly fixed.** The new snapshot helper does not join unrelated nodes, and its self-test catches that mutant. It also cannot recognize a secret split across nodes within the same secret region. |
| 8. Rehearsal evidence path is malformed | **Fixed.** The path now points to the existing `20261101T120804Z-outcome-rehearsal-5` output. |

## 2. New findings

These are proof-script findings. The synthetic cases do not assert that the pinned application currently has the corresponding application defects.

1. **P1. A recurrence can be erased in the gap before dismissal.** `tools/verify_windows_app.ps1:187`, `tools/ProofSnapshot.psm1:72`. `Click-Control` samples the tree, then finds and invokes the button. A dismissed outcome can return between those operations; the button acknowledges all displayed non-pending outcomes. A probe using the actual helper reproduced both records being dismissed without an error. The pre-click read also accepts a snapshot without a sentinel, which a second probe demonstrated. **Fix:** require a readable owned-tree sentinel for every monitoring sample, and add a reliable observation mechanism that retains outcome arrivals across the sampling-to-dismissal gap and fails closed if observations are lost. Test the exact interleaving.

2. **P1. The delayed reveal check does not wait for the reveal response.** `tools/verify_windows_app.ps1:500`, `desktop/src-tauri/src/commands.rs:291`. The five-second stability observation runs before the journal count check. `reveal-requested` is written before `session.reveal` and before the response reaches the UI. A probe using the actual `Test-KeyDetailExpiry` function passed and closed the dialog while its simulated response was still undelivered. **Fix:** obtain a reliable, request-specific completion signal for the fake reveal, then keep Key Detail open and masked through delivery and a defined stability period. A journal request entry alone cannot be that signal.

3. **P1. Negative checks can miss a visible secret or an open import dialog.** `tools/ProofSnapshot.psm1:54`, `tools/verify_windows_app.ps1:470`, `tools/verify_windows_app.ps1:560`. A synthetic secret split into `fake-prod-` and `must-stay` within the Secret value region satisfies the current masked predicate. An open dialog also satisfies the import-gone predicate if no individual node is named exactly `Import keys`. **Fix:** reconstruct text only within the identified Secret value element, and test the import dialog's semantic presence independently of its heading text. Add split-text and surviving-dialog cases.

4. **P1. A second outcome region is ignored.** `tools/ProofSnapshot.psm1:37`. The converter selects only the first `Operation outcomes` region. The actual `Assert-OutcomeRegion` accepted a synthetic tree with two regions and one record in each. If a shared dismissal clears both, the later absence check can also pass. **Fix:** reject duplicate regions or count records across all of them before asserting exactly one.

5. **P1. The path-free check accepts a leaked path.** `tools/verify_windows_app.ps1:451`. It checks a small, case-sensitive substring blacklist. The actual assertion accepted `C:/FAKE-PROOF-ROOT/vault.kdbx` when the fake work directory was `C:\fake-proof-root`. **Fix:** validate the complete expected outcome text within its record, or normalize and check every relevant fake path and value. Test case and separator variations as well as additional text.

6. **P1. Restore is requested but never verified.** `tools/verify_windows_app.ps1:385`. The helper proves `IsIconic` after minimizing, but after requesting `Normal` it does not check that the window actually left the minimized state. UI Automation may still expose a minimized window's tree. **Fix:** wait for and assert native restoration on the same owned handle before continuing, with a bounded failure if restoration does not occur.

7. **P2. Cancellation checks the message but not the claimed file outcome.** `tools/verify_windows_app.ps1:511`. The test does not set a known destination before Cancel or inspect the filesystem afterward. A defective cancellation that writes a file while reporting "No file was saved" can pass. **Fix:** enter a unique fake destination for each cancellation and assert that it and the dedicated export directory contain no resulting file.

8. **P2. Busy-state checks can reject a correct slow transition.** `tools/verify_windows_app.ps1:561`, `tools/verify_windows_app.ps1:584`. Both read `IsEnabled` once after restore. The save check does so immediately. A correct UI that clears busy state after that read wastes the owner's attempt. **Fix:** wait for the enabled state with a bounded timeout, then check the required stability.

9. **P2. The filter retry does not retry selection errors.** `tools/verify_windows_app.ps1:254`. `Select-FilterOnce` runs outside a retry handler. A transient missing option or stale control ends the function on attempt one. A probe using the actual function reproduced that result. This helper is also used by journal-export, remaining and full proof modes. **Fix:** retry recoverable selection and stale-control failures, reacquire the combo on each attempt, and propagate the failure after the final attempt.

10. **P3. The preflight description overstates its coverage.** `docs/FINDINGS.md:29`, `docs/FINDINGS.md:56`. The first sentence says only the proof script changed after `cfeb1b0`, but the module and self-test were also added. The self-test description says it has cases for every original finding, although it does not test the preflight or evidence path. **Fix:** name all three tools and describe the self-test's actual scope.

## 3. Checks run

- **PASS:** `git diff --check chore/clean-project-structure...284d666`. No whitespace errors.
- **PASS:** PowerShell `Parser::ParseFile` on `tools/verify_windows_app.ps1` and `tools/ProofSnapshot.psm1`. No parse errors. An initial parser command had a quoting error; the corrected invocation passed.
- **PASS:** `powershell -NoProfile -ExecutionPolicy Bypass -File tools/test_proof_snapshot.ps1`. All nine cases passed.
- **PASS:** The FINDINGS step 1 commands. `git diff --quiet cfeb1b0 -- crates desktop Cargo.toml Cargo.lock tools ':(exclude)tools/verify_windows_app.ps1' ':(exclude)tools/ProofSnapshot.psm1' ':(exclude)tools/test_proof_snapshot.ps1'` and the proof-tool comparison against `284d666` both exited 0. `git status --porcelain -- crates desktop tools` was empty. The three tool hashes are in the temporary receipt.
- **PASS:** `Get-FileHash -Algorithm SHA256` for the four builds listed in STATE. All matched the pinned hashes.
- **PASS:** Six independently created module mutants were caught by the self-test. They covered missing sentinel acceptance, swallowed reader errors, omitted dismissed checks, joining unrelated nodes, direct-child-only counting and returning before a transition. The mutant driver (`mutant-review.ps1`, review temporary folder) and copies are retained.
- **FAIL:** The synthetic probes (`synthetic-probes.ps1`), actual-function probes (`actual-function-probes.ps1`) and dismissal-race probe (`dismissal-race-probe.ps1`), all in the review temporary folder, reproduced the false-pass and retry cases described above.
- **LIMIT:** Static tracing shows rehearsal returns before the minimize, clipboard and synthetic OS-signal paths. It identifies itself as rehearsal only. The settings restoration and cleanup block is unchanged in the diff. Other modes were traced but not run; their runtime behavior remains unverified.
- **LIMIT:** Read the original rehearsal output and retained runs 2 through 5, including run 4 diagnostics. Run 5 reports the pinned GUI hash and the two-record check. Its output does not independently bind the run to the reviewed proof-tool hashes.
- **SKIPPED:** AKM, the proof script, owner-present visibility testing, clipboard, settings and real-window interactions, as requested. No project verification skill was found.

## 4. Verdict

**REJECT. The script is not ready for the single owner-present attempt.** It still has reproducible false-pass paths for outcome resurrection, delayed reveal completion, semantic absence and outcome validation, as well as false-failure paths for slow transitions. This is the unit's second rejection. The owner's design question is how the proof will reliably observe delayed reveal delivery and outcome arrivals across dismissal while preserving the pinned-build and safety constraints. R4-04 remains unproved. I did not update STATE or the review ledger because this review was explicitly read-only.

The shared observer rejected registration because the required temporary artifact root is outside the project; its handoff also could not be recorded without that registration. The real task-local handoff receipt (`handoff-receipt.json`, review temporary folder) records the gap and retained files.
