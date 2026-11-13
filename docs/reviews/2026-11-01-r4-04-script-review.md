# R4-04 proof script review, round 1 (raw)

> Note of 13 November 2026: the owner's archive folder is written as `<archive>`. No other text changed.

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: PR #3, branch `test/r4-04-proof-checks` at `e4b4b7a`, against `chore/clean-project-structure`
- Verdict: REJECT, 8 findings (4 P1, 3 P2, 1 P3). This is the first rejection of the proof script work unit. The fixes follow in the next commit, and one more review checks them. R4-04 stays unproved.

The text below is the reviewer's report, copied by hand. Typographic quotation marks and apostrophes became straight, and the reviewer's links to local files became plain `file:line` text. Other differences cannot be excluded.

---

Reviewed `e4b4b7a` against `chore/clean-project-structure`. The findings below concern the proof script and its claims. They do not assert that the pinned app currently exhibits the hypothetical regressions used to test the proof.

## Ranked findings

1. **P1: A dismissed outcome can return and be dismissed again before the no-resurrection check.** `tools/verify_windows_app.ps1:476`

   By inspection, the script dismisses the import, export, and cancellation outcomes without checking each one immediately. For example, a cancellation could return after the error outcome is counted at line 496 but before the dismissal at line 499. The application's dismiss handler acknowledges all displayed non-pending outcomes, so that click could remove both. The final check would then pass.

   **Fix:** Check absence after each dismissal and monitor the previously dismissed outcomes throughout subsequent operations, including before any later dismissal can remove them. Exercise a deliberately delayed recurrence in a regression test.

2. **P1: The new negative checks can fail before a correct dismissal has rendered.** `tools/verify_windows_app.ps1:412`

   `Click-Control` uses UI Automation's `Invoke`, then `Assert-NoOutcomeResurrection` immediately calls `Wait-Text`. `Wait-Text` returns true on its first matching sample, so the assertion fails if the old outcome is still visible for that sample, even if dismissal completes immediately afterward. UI Automation invocation may return before the requested action finishes. The same race exists between the rehearsal's Hide click and its secret-absence check. Microsoft's InvokePattern documentation (https://learn.microsoft.com/en-us/dotnet/api/system.windows.automation.invokepattern.invoke?view=windowsdesktop-9.0) describes the provider-dependent timing.

   **Fix:** First wait, with a bounded timeout, for the expected dismissal or masking transition. Once absence has been observed, start a separate stability check that fails on any recurrence. Test with a delayed UI update so a valid transition does not consume the owner's attempt.

3. **P1: A UI Automation failure is treated as proof that the outcome region is absent.** `tools/verify_windows_app.ps1:419`

   The empty `catch` accepts every `Find-Control` exception, including a stale root or provider failure. The preceding text scan does not cover every outcome message, such as `unconfirmed` and `completedWhileLocked`. If such a region exists and the region lookup fails, the script reports that no region remains. An in-memory synthetic probe confirmed that the catch leaves `$region` null after an unrelated exception.

   **Fix:** Give "not found after successful traversal" a distinct result. Propagate other UI Automation errors, and verify the owned root remains readable during the absence observation. Add a regression case that injects a provider failure.

4. **P1: The Key Detail check ends before a late disclosure can be observed.** `tools/verify_windows_app.ps1:389`

   The script waits until Reveal has already completed, checks for the secret for only 300 ms after restore, and then closes the detail view. A regression that restores the secret later while the detail remains open would escape this check. It also does not exercise a reveal that is still pending at minimize. The existing frontend test covers pending reveals with mocked native calls, but that is not the real-window proof.

   **Fix:** Keep Key Detail open through a defined observation period and through completion of a deliberately delayed fake reveal. The fake journal lock is a candidate for delaying that request; release it after restore and verify that neither the secret nor a stale error appears. Keep the displayed-secret case as a separate check.

5. **P2: The documented preflight rejects this exact change.** `docs/FINDINGS.md:29`

   I ran the documented `git diff --quiet cfeb1b0 -- crates desktop tools Cargo.toml Cargo.lock`. It exited **1** because `tools/verify_windows_app.ps1` changed. The same comparison excluding that one script exited **0**, and the script matched `e4b4b7a`. As written, the preflight stops before the requested proof even with this reviewed script and otherwise matching inputs. `docs/STATE.md:14` also says later commits changed only documentation and Git rules.

   **Fix:** Compare build inputs against `cfeb1b0` while excluding only the intentionally changed proof script. Separately compare that script against the reviewed proof commit and record its identity. Update STATE to describe the script change.

6. **P2: The outcome count is not yet established as a count of records.** `tools/verify_windows_app.ps1:378`

   The predicate counts immediate UI Automation children that are not buttons. It does not identify individual notices. A synthetic tree with one non-button wrapper containing two records passes the predicate. I did **not** establish that this wrapper shape occurs in the pinned WebView2. The retained rehearsal demonstrates one record only, and FINDINGS acknowledges that two have not been tried.

   **Fix:** Make the count identify individual outcome controls, then validate zero, one, and two outcomes against the real WebView2 tree. A no-minimize rehearsal can create two native-cancellation outcomes before dismissing either and verify that the one-record assertion rejects them. Until then, describe the count as unverified.

7. **P2: `Wait-Text` can combine unrelated control names into a match.** `tools/verify_windows_app.ps1:189`

   The new checks use a helper that joins all descendant names without separators. In an in-memory probe, separate names `Import ` and `completed.` matched `Import completed.` even though neither name matched individually. This could make a negative assertion fail for unrelated controls. The actual pinned UI Automation tree was not inspected in this review.

   **Fix:** Match within a defined semantic element, such as an individual outcome or the Secret value region. If text is split across descendants, reconstruct it only within that element and validate the mapping against the real UI Automation tree.

8. **P3: The rehearsal evidence path in FINDINGS is malformed.** `docs/FINDINGS.md:52`

   The recorded path contains `U+0082` after `proofs` instead of `\202`. The supplied evidence file exists at `<archive>\proofs\20261101T114352Z-outcome-rehearsal\output.txt`.

   **Fix:** Replace the malformed path with the exact path to the retained output.

## Checks run

- **PASS:** PowerShell parser reported no syntax errors. `git diff --check` passed. The working tree was clean.
- **PASS by static call-path inspection:** `-OutcomeRehearsal` rejects the visibility flag, calls Key Detail with `-Rehearse`, and returns before `Test-OutcomeHandoff`. No rehearsal path reaches `Minimize-RestoreOwnedWindow`.
- **PASS by inspection:** Key Detail runs before the fake journal lock is acquired. Both current Key Detail controls named `Close` call the same handler. I found no current filter or selected-view dependency broken by the added step.
- **FAIL:** The documented source preflight exits 1 on the reviewed commit.
- **LIMITED:** The retained rehearsal output reports a passing Hide and native-cancellation run and the expected GUI executable hash. It does not demonstrate two outcomes or the owner-present minimize proof.
- **NOT RUN:** The verification script, AKM app, clipboard, settings, and window interactions. No project verification skill was found in the prescribed locations.

## Verdict

**REJECT.** The script can miss a returned outcome, can treat an automation error as absence, and can reject a correct asynchronous dismissal. The documented preflight also blocks the proof on this commit. R4-04 remains unproved.
