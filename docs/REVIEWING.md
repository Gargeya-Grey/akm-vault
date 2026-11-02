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
2. **Source acceptance: YES (provisional) or NO**, and the reason, by the [pass rule](#pass-rule). Name each earlier P0 to P2 finding that you checked. A design review reports NO, "implementation not reviewed", and gives its design verdict here.
3. **Work unit complete: YES or NO**, with any missing runtime proof.
4. What the review did not cover.

Do not give a numerical score. A unit review is not the full rescore and not a professional audit.

## Pass rule

Owner decision, 2026-11-02. Two reviews rejected earlier text ([round 1](reviews/2026-11-02-pass-rule-review1.md), [round 2](reviews/2026-11-02-pass-rule-review2.md)). The owner then adopted the rule by decision, with the round 2 fixes as proposed. It applies from the next review.

- **Subject.** The rule applies to every new work-unit review, design reviews included. A review accepts only the subject that it reviewed. A design review gives a design verdict and reports source acceptance NO, "implementation not reviewed". It cannot complete a work unit or close a source defect.
- **Severity.** Rate each finding by its demonstrated effect, as if this rule did not exist. A finding is **P2 or higher** when it can disclose a secret, lose or corrupt vault data, change vault or app state that the user did not ask for, break a [contract](PROJECT.md#contracts-every-change-keeps), or cause a material loss of a required workflow, availability, reliability, privacy or correctness. **P3** is for low-impact defects, and for coverage or documentation gaps that do not block.
- **Earlier findings.** Before a source pass, verify the fix and the required regression evidence for every in-scope P0 to P2 finding, from earlier rounds too. An unresolved one blocks the pass. Missing required evidence makes the review incomplete. An owner-approved scope split can move a finding out of the unit.
- **Fail.** A review that establishes a P0, P1 or P2 finding fails, even when other scope or evidence is missing. It also records the missing coverage. The AGENTS.md stop rule ("fails review twice") counts failed reviews only.
- **Incomplete.** A review that could not see its required scope or evidence, and established no P0 to P2 finding, is incomplete. It reports source acceptance NO and names what was missing. It counts as neither a pass nor a failure.
- **Pending, then pass.** A complete review of the final source with no P0 to P2 finding reports source acceptance YES (provisional). Its result is **pending** until these steps are done, in order:
  1. Record its findings in FINDINGS.md.
  2. Move every P3 finding (below).
  3. Settle every severity dispute (below).

  Then record the **pass**. Only a pass counts for defect closure or landing.
- **Disputes.** The raw review keeps the reviewer's words and severities. When a severity is below the P2 floor above, or the implementer disputes it, the result stays pending until the owner decides. Record the decision next to the review, never inside it.
- **Moving a P3.** Move it to the later unit whose ROADMAP.md scope covers the affected behaviour. When no unit or more than one unit fits, ask the owner; the result stays pending until the owner names one. List the finding under that unit in FINDINGS.md with its status word (Open, or Gap for a missing proof), and in that unit's ROADMAP.md entry. It closes by the AGENTS.md rule, in that unit. A move does not authorize work on that unit.
- **Two verdicts.** A pass accepts the source. "Work unit complete" is a separate verdict: it needs a passing source review and every runtime proof that the unit's records require. A deferred proof stays unproved. For example, unit 1a stays incomplete until R4-04 passes.

## After the review

1. Commit the raw review text, unchanged, to `docs/reviews/<date>-<scope>.md`. Put a header above it that gives the model, the requested effort, the reviewed commit and build hashes, the date, and the result under the pass rule: pass, pending, fail or incomplete. Update only that result line when a pending result becomes a pass. Never rewrite a rejected review into an approval.
2. Add each new finding to FINDINGS.md, with an ID like `1A-R5-01` and a link to that review file. For a pending result, move each P3 finding and settle each dispute as the pass rule says, then record the pass.
3. Update STATE.md.
