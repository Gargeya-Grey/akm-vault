# Review of the pass rule, round 1 (raw)

- Date: 2026-11-02
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: commit `528f72e` on `fix/unit1a-sound-findings` (docs only: AGENTS.md, docs/REVIEWING.md, docs/STATE.md)
- Verdict: **Accept: NO.** 4 P2 and 3 P3 findings. All seven are fixed in the next commit. This review is about the process text, so the pass rule does not decide it.

The text below is the reviewer's report. Links to repository files became plain text with the repository path and line. Other differences cannot be excluded.

---

## Ranked findings

All findings are by inspection.

1. **P2. A deferred proof can be mistaken for completed work.** docs/REVIEWING.md:58 says work-unit completion needs the listed runtime proofs "or the owner's decision to defer them." R4-04 is already deferred, but docs/FINDINGS.md:18 explicitly says unit 1a's work-unit verdict remains **NO until R4-04 passes**. A later P3-only source review could therefore be treated as completing unit 1a without that proof.

   **Proposed fix:** "A deferred proof remains unproved. A work unit remains incomplete when its recorded acceptance conditions require that proof. For unit 1a, deferring R4-04 permits a source review, but the work-unit verdict stays NO until R4-04 passes."

2. **P2. A design or incomplete review can be presented as source acceptance.** docs/REVIEWING.md:44 requires source acceptance YES whenever there is no P0–P2 finding, and line 58 says a pass accepts the source. Yet the same process covers design reviews, and unit 1h starts with a design before implementation. A design-only review, or an attempt that could not inspect the required source, can have no P0–P2 findings without having reviewed the implementation. That risks treating a design pass as the work-unit review needed to close a defect.

   **Proposed fix:** "A pass applies only to the subject reviewed. A design-review pass accepts the design only. Source acceptance and defect closure require a completed review of the final implementation and its claimed fixes. If required scope or evidence is unavailable, report source acceptance NO and identify the review as incomplete; do not count an incomplete attempt as a failed review under the two-failure stop rule."

3. **P2. The documents disagree on when a P3-only review passes.** AGENTS.md:40 makes moving the P3 findings part of the pass rule. docs/REVIEWING.md:44 requires YES based only on severity, while lines 62–63 record the result before moving findings. If no suitable later unit has yet been named, the recorded acceptance can be treated as a pass before the owner's second condition is met.

   **Proposed fix:** "For a review with no P0–P2 findings, the reviewer may report source acceptance YES subject to P3 assignment. Record the final review result as passing only after every P3 has been recorded and moved to its named later unit. Do not treat an unassigned review as passed."

4. **P2. A severity error can bypass the stated floor.** docs/REVIEWING.md:55 sets a P2 floor for secret disclosure or vault-data loss, but line 56 says the reviewer's severity counts and the implementer keeps it. If a reviewer labels an actual secret disclosure P3, one instruction requires P2 while the other retains P3. A dispute goes to the owner, but the text does not hold the pass while that dispute is unresolved. The documents also provide no severity definitions for other effects, such as an unauthorized import.

   **Proposed fix:** "Preserve the reviewer's original wording and severity in the raw report. If a severity conflicts with the P2 floor or is disputed, do not record a pass until the owner resolves it." Add an effect-based severity rubric, with examples including unauthorized state changes, to guide future reviewers.

5. **P3. Option B is ambiguous about overriding the rejected review.** docs/STATE.md:65 correctly says the new rule starts with the next review and the recorded 1a rejection remains. It then calls applying the rule to that rejected review "option B." Option B also proposes landing three PRs, while line 72 requires the acceptance review to accept before they land. The owner has not selected B, so this does not authorize a retroactive pass or a merge.

   **Proposed fix:** "Option B asks the owner for a separate override of the rejected source verdict and the stated review-acceptance condition for landing. The recorded review remains NO. If the owner chooses B, record the override separately and retain the R4-04 completion gate."

6. **P3. P3 assignment is based on file ownership that the roadmap does not define.** docs/REVIEWING.md:57 directs a finding to the unit that owns its files, but docs/ROADMAP.md:5 says units share files and defines their scopes by behavior. docs/STATE.md:62 also sends every future P3 from option A to unit 1h, even if a finding concerns journal behavior outside 1h's stated scope.

   **Proposed fix:** "Move each P3 to the later unit whose scope covers the affected behavior. If no unit fits or more than one fits, ask the owner to name the unit. Record the assignment in FINDINGS.md and ROADMAP.md. Assignment does not authorize implementation."

7. **P3. The required `Open` status mislabels proof gaps.** docs/REVIEWING.md:57 says every moved P3 "stays Open." docs/FINDINGS.md:7 distinguishes `Open` from `Gap`, which means missing proof rather than a defect. A moved coverage finding would be classified incorrectly.

   **Proposed fix:** "The finding remains unclosed under its assigned unit, with the appropriate status defined in FINDINGS.md, including Gap for missing proof. It closes under the AGENTS.md rule."

The owner's wording has several additions in this commit. Naming a **later** unit narrows "a named unit." File-based assignment, owner selection when none fits, a roadmap entry, and closure in the new unit add tracking requirements; they need the corrections above. Effect-based severity, the secret/data-loss floor, reviewer severity authority, and owner adjudication add safeguards, but their interaction needs the hold rule in finding 4. Mapping a pass to source acceptance and allowing deferred proof for completion add consequences that need the limits in findings 1 and 2. Counting only failed reviews is consistent with the existing stop rule. Keeping raw reviews unchanged and adding a result header are safe if historical verdicts remain unchanged.

The `#pass-rule` links resolve, and `git diff --check` passed. The independent full-review score and owner-testing release gate remains stated in AGENTS.md and PROJECT.md. No files were changed. The proof skill's edit and delegation step was skipped because this was a read-only reviewer assignment.

**Accept: NO.** The completion, review-scope, assignment, and severity instructions can produce an acceptance that conflicts with the existing process or the owner's rule.
