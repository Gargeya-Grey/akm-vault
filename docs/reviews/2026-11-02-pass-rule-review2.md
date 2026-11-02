# Review of the pass rule, round 2 (raw)

- Date: 2026-11-02
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: `git diff 0268b2f 3109a1c` on `fix/unit1a-sound-findings` (docs only: AGENTS.md, docs/REVIEWING.md, docs/STATE.md, the round 1 raw review)
- Verdict: **Accept: NO.** 5 P2 and 1 P3 findings. This is the second rejection, so the work stopped (AGENTS.md). The owner then adopted the rule by decision, with these fixes as proposed (STATE.md, option A).

The text below is the reviewer's report. Links to repository files became plain text with the repository path and line. Other differences cannot be excluded.

---

## Ranked findings

All findings are by inspection of `0268b2f..3109a1c`.

1. **P2: The severity rubric makes some material failures P3.** docs/REVIEWING.md:56 says P3 is for everything outside the listed P2 effects. For example, a regression that makes every valid non-secret CLI inspection fail need not disclose a secret, change state, corrupt vault data, or violate one of the linked contracts. The rubric therefore puts a broken required workflow in P3, where it can be moved and the source can pass.

   **Proposed fix:** "Rate material loss of a required workflow, availability, reliability, privacy, or correctness as P2 or higher. Use P3 for low-impact defects and non-blocking coverage or documentation gaps. Rate by demonstrated effect."

2. **P2: Previously reported blocking defects have no explicit acceptance check.** docs/REVIEWING.md:55 bases the pass on findings in the current review. The method at line 36 requires a recheck of findings the change *claims* to fix, but does not require an unresolved, in-scope P2 from an earlier review to be addressed. A reviewer could report no new P2, allowing a pass while that known defect remains open. The completion rule at line 59 also names runtime proof without explicitly requiring a passing source review.

   **Proposed fix:** "Before a source pass, verify the fix and required regression evidence for every in-scope P0 to P2 finding, including findings from earlier rounds. An unresolved defect blocks the pass; unavailable required evidence makes the review incomplete. An owner-approved scope split may move a finding outside the unit. Work-unit completion also requires a passing source review and every required runtime proof."

3. **P2: A P3-only review must pass before its findings move, and move them before it passes.** docs/REVIEWING.md:55 requires the moves before a pass. Line 64 and docs/STATE.md:62 say to move them *after* a pass. Meanwhile, the return format (docs/REVIEWING.md:44) requires an unqualified source-acceptance YES before assignment. A reader can either get stuck or treat that YES as the acceptance needed for landing.

   **Proposed fix:** "For a complete review with no P0 to P2 findings, record source acceptance as provisional pending P3 assignment and any severity decision. Record the findings and move every P3 first. Only then record a pass and use it for defect closure or landing. If a unit is undecided, the result remains pending."

4. **P2: An incomplete review can conceal a failed round from the stop rule.** docs/REVIEWING.md:54 says every incomplete review counts as neither pass nor failure. A reviewer can reproduce a P2 defect and then discover unrelated missing evidence. If that happens twice, neither round counts as a failure, so the two-failure stop in AGENTS.md:40 can be avoided despite two known blocking results.

   **Proposed fix:** "A review that establishes a P0 to P2 finding fails for the stop rule, even if other scope or evidence is missing. Record the missing coverage as well. An incomplete review with no established blocking finding counts as neither a pass nor a failure."

5. **P2: Design reviews have conflicting pass and return instructions.** AGENTS.md:40 states the pass condition for a review without limiting it to source reviews. docs/REVIEWING.md:54 limits the rule to final-source acceptance, while its return format (docs/REVIEWING.md:44) tells a reviewer who covered the whole scope with no P0 to P2 to report **source acceptance YES**. A complete P3-only design review has no clear pass condition and can be reported as accepting source it never reviewed.

   **Proposed fix:** "Apply the pass condition to each new work-unit review, including design reviews, but accept only the subject reviewed. A design review returns a design verdict and source acceptance NO with 'implementation not reviewed.' It cannot complete the work unit or close a source defect."

6. **P3: The round 1 header claims verification that this review does not support.** docs/reviews/2026-11-02-pass-rule-review1.md:6 says all seven findings are fixed in the next commit. Findings 2 and 3 remain partly fixed, as described above.

   **Proposed fix:** Change the added header to "The next commit addresses all seven findings; their resolution is determined by round 2." Keep the raw rejection unchanged.

## Round 1 finding status

1. **Fixed.** A deferred proof remains unproved, and unit 1a remains incomplete until R4-04 passes.
2. **Partly fixed.** The rule distinguishes design from final-source review and identifies incomplete reviews, but the design return format and incomplete-review stop case remain contradictory.
3. **Partly fixed.** The pass definition requires P3 assignment, but the post-review and STATE instructions still put assignment after the pass.
4. **Partly fixed.** The floor, unauthorized-state-change example, and hold for disputes are present. The remaining rubric gap is finding 1 above.
5. **Fixed.** Option B is described as a separate owner override; the recorded review stays NO and the R4-04 gate remains.
6. **Fixed.** Assignment follows behavioral scope, uncertain assignments go to the owner, and a move does not authorize implementation.
7. **Fixed for moved findings.** The instructions permit `Gap` for missing proof rather than requiring `Open`.

The historical unit 1a acceptance review is unchanged. At `3109a1c`, all 107 local Markdown links checked across the requested files and their heading fragments resolve; `git diff --check` passed. No files were changed. The shared branch advanced to `f19cb0c` during the review; the findings above are tied to the requested `3109a1c` revision.

**Accept: NO.** The rule still permits material failures to be classified as P3, leaves prior blocking findings insufficiently gated, and contains conflicting pass and stop instructions.
