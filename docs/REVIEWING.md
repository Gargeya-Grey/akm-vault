# Reviews and side jobs

## Which model

Owner instruction, 2026-11-01:

- **Every review** runs on GPT-6.1 Terra at **max** effort: bug reviews, acceptance reviews, design reviews and analysis. In T3 Code, use `delegate_task` with provider `codex`, model `gpt-6.1-terra`, option `reasoningEffort: max` and role `review`. If Codex is not available, use provider `opencode`, model `openai/gpt-6.1-terra`, variant `max`. Check availability before you dispatch. If neither is available, tell the owner. Do not switch models yourself.
- **Simpler side jobs** run on GPT-6.1 Terra at an effort that fits the job: `low` for a mechanical job with a clear check, `medium` for a bounded job, `high` for a job that needs judgment. Side jobs include fixture generation, link checks, log triage and small research.
- The main session plans, edits and collects.

## The review brief

Give the reviewer this file, the scope, and nothing from the implementer's chat or reasoning.

**Scope to supply:** the work unit's plan (from [ROADMAP.md](ROADMAP.md)), a diff that contains the whole unit, the check results, the reviewed source commit and build hashes, and the open findings for that unit ([FINDINGS.md](FINDINGS.md)). Use a plain commit range only when it really contains the whole unit.

### Unit 1a inputs

Git started after unit 1a was written, so `main` alone does not show the unit. Supply these:

- The change: `git diff baseline/pre-unit-1a main -- crates desktop tools .github Cargo.toml Cargo.lock`. This compares `main` with the 139-file pre-unit snapshot. A path shown as added was absent from that snapshot. It is not necessarily new in unit 1a: `desktop/icon.svg` existed before phase one with its current hash. Review each added-looking path as a full file. Check the other pre-unit records in the archive tag, and record any preimage or deletion that cannot be proved.
- The frozen contracts, read with `git show archive/pre-cleanup-2026-11-01:<path>`:
  - `docs/work/phase2-1a/PLAN.md` (original unit plan)
  - `docs/work/phase2-1a/corrections3/PLAN.md` (round-three recalibration)
  - `docs/work/phase2-1a/corrections4/PLAN.md` (round-four recalibration)
  - `docs/work/phase2-1a/corrections4/visibility/PLAN.md` and `PUBLICATION_CONTRACT.md` (visibility correction)
  - `docs/work/phase2-1a/corrections4/visibility/NATIVE_PROOF.md` (R4-04 proof plan)
  - `docs/work/phase2-1a/reviews/round4/review-result.md` (last rejected review)
- The build hashes in [STATE.md](STATE.md#unit-1a-build-under-test). The reviewer records the source commit and the hashes that it checked.
- The R4-04 result, or the owner's decision about it.

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

1. Commit the raw review text, unchanged, to `docs/reviews/<date>-<scope>.md`. Put a header above it that gives the model, the requested effort, the reviewed commit and build hashes, and the date. Never rewrite a rejected review into an approval.
2. Add each new finding to FINDINGS.md, with an ID like `1A-R5-01` and a link to that review file.
3. Update STATE.md.
