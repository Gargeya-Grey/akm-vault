# Clean brief for a fresh AKM adversarial reviewer

The role is independent and read-only. Assume a defect or unjustified claim exists. Do not start from the implementer's explanation or conversational transcript.

## Current calibration assignment

Decide whether phase-two engineering can start on fake data, and challenge the proposed scope, ordering and acceptance tests. Check that the new context pack accurately preserves the project's requirements, historical failures, latest evidence and remaining gaps.

The owner authorized `openai/gpt-6.1-terra#max` on 2026-10-31. This is a new review context, not a resume of a phase-one reviewer. The main session owns integrating the verdict and updating current state.

## Inputs

1. [Project requirements/contracts](PROJECT.md), [current state](STATE.md) and [draft phase-two plan](../PHASE2.md).
2. [Original full scored review](../REVIEW.md) and the [review disposition index](REVIEW_LEDGER.md). Read raw historical reports where a claim is material; do not treat every old failure as a current defect.
3. [Verification policy/evidence](VERIFICATION.md), [phase-one final record](history/phase1-final/evidence.json), pinned source manifest and preserved raw reports/probes.
4. Current source and direct callers needed to verify a premise. The working tree is the current implementation; historical line numbers and archived probe imports may no longer match it.

## Questions to attack

- Does the phase-two list omit an original unresolved finding, redo an already corrected one, or confuse source acceptance with runtime/score readiness?
- Are private-journal migration, read-only older vaults, stable vault identity, tail corruption and post-commit outcomes handled without new data-loss or privacy promises?
- Do rotation/history/deletion plans preserve foreign clients, attachments, explicit upgrades, meaningful timestamps and operation/session receipts?
- Are dotenv compatibility, expiry instants, selection ambiguity, secret-bearing copies/Debug and remaining accessibility/reminders scoped precisely enough to test?
- What can be verified without disturbing the owner, and what genuinely needs owner participation?
- Could an agent recover from compaction using only the repository files, without a missing temp report being silently invented or an old failure being reported as current?

## Constraints

- No application edits, full builds/suite reruns, GUI launches, actual clipboard, AppData/settings access, OS lock/suspend, elevation, git operations or external publication for calibration.
- Use source inspection and existing evidence first. Tiny targeted probes, if essential, use fake data in a new approved scratch directory only. Do not modify archived evidence or project files.
- Do not manufacture a numerical score. This bounded plan/context review is not the original full nine-area rescore or a professional audit.
- Distinguish reproduced defect, inspection-supported concern, design decision and missing coverage. Cite files/lines or a public seam and an exact scenario.

## Return format

Give ranked supported findings with evidence and a correction or acceptance test. Then state phase-two entry verdict, recommended first work unit, any decisions that must be frozen before implementation and what remains outside this review. If no blocker is found, explain why in concrete terms. Keep the review self-contained so it can be archived verbatim with model/source identity.
