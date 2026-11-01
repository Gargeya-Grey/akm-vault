# Reviews and side jobs

## Which model

Owner instruction, 2026-11-01:

- **Every review** runs on GPT-6.1 Terra at **max** effort: bug reviews, acceptance reviews, design reviews and analysis. In T3 Code, use `delegate_task` with provider `codex`, model `gpt-6.1-terra`, option `reasoningEffort: max` and role `review`. If Codex is not available, use provider `opencode`, model `openai/gpt-6.1-terra`, variant `max`. Check availability before you dispatch. If neither is available, tell the owner. Do not switch models yourself.
- **Simpler side jobs** run on GPT-6.1 Terra at an effort that fits the job: `low` for a mechanical job with a clear check, `medium` for a bounded job, `high` for a job that needs judgment. Side jobs include fixture generation, link checks, log triage and small research.
- The main session plans, edits and collects.

## The review brief

Give the reviewer this file, the scope, and nothing from the implementer's chat or reasoning.

**Scope to supply:** the work unit's plan (from [ROADMAP.md](ROADMAP.md)), the commit range (`git diff <base>..<head>`), the check results, and the open findings for that unit ([FINDINGS.md](FINDINGS.md)).

**Method.** Assume that a defect or a claim without evidence exists, and find it.

- Read the whole diff and every caller of each changed function, not only the changed lines.
- Check the change against the contracts in [PROJECT.md](PROJECT.md#contracts-every-change-keeps).
- For each finding the change claims to fix, try to break the fix with a new scenario, not only the scenario that was reported.
- A check result is evidence only for the inputs and scenarios that it ran.

**Allowed actions.** Read-only. Small probes with fake data are allowed in a new folder under `%TEMP%`. Do not edit project files, and do not touch the clipboard, the app settings, real windows or the OS session. Do not launch the GUI, and do not run full builds or suites.

**Return format.**

1. Ranked findings. Each one has a severity (P0 to P3), a file and line, the exact scenario, whether it was reproduced or found by inspection, and the test that would close it.
2. **Source acceptance: YES or NO**, and the reason.
3. **Work unit complete: YES or NO**, with any missing runtime proof.
4. What the review did not cover.

Do not give a numerical score. A unit review is not the full rescore and not a professional audit.

## After the review

The main session adds each new finding to FINDINGS.md, with an ID like `1A-R5-01`, and updates STATE.md. Commit the raw review text to `docs/reviews/` only when it is a full-rubric score. Otherwise, the pull request keeps the review.
