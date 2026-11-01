# Clean brief for a fresh AKM adversarial reviewer

The role is independent and read-only. Assume a defect or unjustified claim exists. Do not start from the implementer's explanation or conversational transcript.

## Choosing the review scope

The 2026-10-31 calibration is complete. Its [raw verdict and disposition](reviews/2026-10-31-calibration/disposition.md) are preserved. Do not restart it just because another context window opens.

For a new implementation review, the caller supplies the bounded work-unit plan, combined diff, source identity and verification policy/results. Review changed functions and their callers against requirements and prior contracts. Previous reports are evidence on demand, not a substitute for fresh inspection or permission to repeat every historical probe.

For **implementation acceptance**, independently attack the complete frozen work unit and its preservation boundaries, not just the last patch or previously reproduced examples. Follow its original plan and any explicitly preserved contract refinements; test claims against source/direct callers rather than the implementer's rationale. A reported check is evidence only for its exact inputs/scenarios. A passing selective collection with a preserved failed/incomplete run is not an all-first-try suite pass.

For a specifically requested new plan calibration, challenge engineering entry, scope, ordering, acceptance and the context pack's accuracy. Do not confuse that task with an implementation verdict or full numerical rescore.

The owner authorized `openai/gpt-6.1-terra#max` on 2026-10-31. Check current instructions and model availability. Record requested model/variant separately from any independently observed model/effort. The main session owns integrating the verdict and updating current state.

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
- For implementation acceptance, remain read-only too: no GUI/real clipboard/settings/OS operations or full build/suite reruns. Essential tiny fake-data probes must use a new approved scratch directory and the supplied exact source/artifact identity, preserving their raw output. Never retry an owner's native proof or turn a mocked boundary into actual-window evidence.
- Use source inspection and existing evidence first. Tiny targeted probes, if essential, use fake data in a new approved scratch directory only. Do not modify archived evidence or project files.
- Do not manufacture a numerical score. This bounded plan/context review is not the original full nine-area rescore or a professional audit.
- Distinguish reproduced defect, inspection-supported concern, design decision and missing coverage. Cite files/lines or a public seam and an exact scenario.

## Return format

Give ranked supported findings with evidence and a correction or acceptance test. Then state phase-two entry verdict, recommended first work unit, any decisions that must be frozen before implementation and what remains outside this review. If no blocker is found, explain why in concrete terms. Keep the review self-contained so it can be archived verbatim with model/source identity.

For **implementation acceptance**, replace the calibration-entry portion with separate **source acceptance YES/NO** and **overall work-unit/runtime completion YES/NO** verdicts, explicit supporting evidence, unresolved proof limitations and exclusions. Distinguish independently executed checks from inspected main-session records. Do not score the product, claim a professional audit or infer above-90 readiness. Report requested model/variant separately from independently observable model/effort; unavailable telemetry stays unavailable. Another rejection returns to the owner rather than authorizing another correction/review loop.
