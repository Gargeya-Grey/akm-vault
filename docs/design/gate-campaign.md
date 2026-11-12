# Campaign to the release gate

Written 2026-11-13. This is the plan for the work between the [full rescore](../reviews/2026-11-09-full-rescore.md) (66.25/100) and the next full review. The bar is above 90/100 ([PROJECT.md](../PROJECT.md)). This plan gives no score: only a new full review measures one.

Tier 2: 6 units, risk: auth, token, delete, schema, workflow, privacy, public API.

## Why the earlier work did not finish

1. **The review loop did not converge.** Each review was an open-scope attack on a large change. Each one found new P2 findings in new places. After two failed reviews the unit stopped and waited for the owner. Five branches stayed unmerged, and every phase two fix stayed "fixed, not accepted".
2. **Fixes were made by instance.** A fix closed the reported case and left its siblings, or added a new defect of the same class. Four classes came back in most reviews: a stale result that replaces a newer one, a layout that drops a fact, a check that is followed by an act outside the session gate, and a deny list that misses a name.
3. **New features started before the gate.** Each new screen and command is new code for a reviewer to attack.
4. **Ten points are not in the product code.** Areas H and I (verification and shareability) hold 10 of the 100 points and scored 3.25. They need a green CI run, real-window proofs, a licence package, an install test on a clean computer and an authenticated release. Without them the total cannot pass 90.
5. **A new full review finds new deductions.** The next score is not the last score plus the fixed findings.

## Rules of this campaign

- **Feature freeze.** No new feature starts before the gate. The terminal launch and the key history are finished, because the rescore deducted for their absence.
- **One branch.** All open work is merged on `gate/integration`. Each unit works in its own worktree from that branch, on its own files.
- **Fix by class.** A finding names a fault class. The fix makes the class impossible by construction where that is practical: one mechanism, every caller moved to it, the old path deleted, and a guard that fails when a new caller brings the class back. Each test has a mutant that it catches.
- **Fewer promises.** Where the operating system cannot give a guarantee, the product does not promise it. The limit is written in the contract, in the user documents and in [CLAIMS.md](../CLAIMS.md), and the review receives it.
- **Review loop.** A unit gets a class sweep by its implementer, then a review ([REVIEWING.md](../REVIEWING.md)). At most 4 rounds per unit. A class that fails two rounds gets a redesign that removes its premise, chosen by a design review, before a third fix. A question goes to the owner only when it is about money, an account, an irreversible step or a contract change.
- **Probe before the score.** Before the full review, each of the nine rubric areas gets a probe review with the full-review brief. The official full review starts only when the probes find no P2. A probe gives no score.
- **Workers leave the records alone.** Only the collector edits `docs/STATE.md`, `docs/FINDINGS.md`, `docs/ROADMAP.md`, `docs/PROJECT.md`, `AGENTS.md` and `docs/reviews/`. A worker returns its finding rows and its proposed contract wording in its report.

## Units

| Unit | Owns | Work |
|---|---|---|
| 0. Integration | merges, records | Merge the five open branches. Run the full checks. Keep the records current. |
| 1. Native session and launch | `desktop/src-tauri/**` | The commit-point mechanism (below), the terminal launch, the import write, outcome records for every dispatched request, the two new notices, the native approval for bulk disclosure (FR-03), the temporary-file report (FR-04), journal recovery (FR-14). Findings T-R1 to T-R3, P2R2-06 (desktop tests). |
| 2. Frontend | `desktop/src/**`, `desktop/e2e/**` | One read mechanism for native state, row identity from rendered facts, accessible names, the new notices, the terminal outcome. Findings LOOK-R3, LOOK-R4, UI-R2, FR-11, FR-19, FR-21. |
| 3. Core and CLI | `crates/**` | The fixes for FR-01, 02, 05, 08, 12, 15 to 18, 22, 28 and P2R2-07 exist and have no review. Review first, then fix by class. Unit 1 can add one narrow hook to `akm-core` for its commit point; unit 3 starts its fixes after unit 1 lands. |
| 4. Tools, CI and release package | `tools/**`, `.github/**`, `.agents/**`, user documents | A native-proof workflow for a disposable CI runner, an install test for that runner, pinned actions, licence texts, the claims list, a public-snapshot tool (FR-26), the release document, the verification skill. |
| 5. Probes and full review | none | Area probes, fixes, owner-present proofs, then the full review. |

### Contract between units 1 and 2 (fixed before the work starts)

- `OperationNotice.kind` gains six values: `filePermissions`, `journalLineageUnconfirmed`, `terminalStarted`, `terminalSessionEnded`, `terminalFailed`, `terminalDeclined`.
- `env_launch_terminal` gains the argument `operationId`, a UUID that the page makes, as `env_import` has. Native code keeps one outcome record with that ID for each dispatched launch, and `vault_warnings` returns it until the page acknowledges it. The reply does not change: the number of keys, or null after a decline.
- `terminalStarted`: a terminal runs and holds keys. `terminalSessionEnded`: the same, and the session ended before the reply. `terminalFailed`: no terminal runs. `terminalDeclined`: the user declined in a native window.
- The page acknowledges the record when the reply arrives, as it does for an import. After a reload, the record is the only truth, and the page shows a notice that names the terminal.

### The commit point (unit 1)

Three open findings have one premise: an effect that cannot be taken back runs after its check, with the session gate released between them, or the gate is held across an operating-system call that can stall. The mechanism that replaces that premise:

1. **Prepare, outside the gate.** Do everything that can stall and can still be taken back: create the process suspended, or write and verify the candidate vault file.
2. **Commit, under the gate.** Check that the session is current and that the vault did not change since the values were read. Then do one short act: resume the process, or replace the file.
3. **Revoke** when the check fails: end the suspended process, or delete the candidate.

A lock that arrives before the commit means that the effect never happens. A lock that arrives after it gets the truthful receipt of contract 4. Each caller of a session-scoped effect uses this one path. The state table (prepared, committed, revoked, against lock, new session, vault change, hide, reload and a lost reply) is written first and tested row by row.

## New fault classes

These rows go into the AGENTS.md table when their guards exist.

| Class | Guard |
|---|---|
| K. A late result replaces a newer one | Every read of native state in the page goes through one mechanism that owns the request identity, the loading state, the failure and the unmount. A check fails on a direct list read in a component. Test: hold two reads and finish the newer one first. |
| L. A failed read shown as an empty result | The read mechanism gives a state of loading, failed or ready. A view says "empty" only from ready with no items. |
| M. Two different things drawn the same | One function builds the facts of a row. The screen and the look-alike check both use its output, so the check compares what the user sees. |
| N. An exit after dispatch that leaves no outcome | A guard made at dispatch records an outcome on every exit path. Test: fail or drop the reply at each step. |
| O. A value checked only where it is used | Validate where input comes in. Read a stored value that breaks the rule as a controlled result, never a panic. |
| P. An effect that cannot be taken back, after a check | The commit point above. |

## Models

The routing file of the owner's harness names the models. Each node states its setting and one reason.

| Node | Setting | Reason |
|---|---|---|
| Collector and planner | The main session | It holds the context of every unit. |
| Units 1 and 2, and unit 3 fixes | The strongest build setting | Each of these units failed two or more reviews at the default build setting. |
| Unit 4 | One step above the default build setting | Its earlier unit failed two reviews; the work is bounded. |
| Reviews and probes | Codex, at max effort | The owner's rule. The first review ran on two models with one brief. Only one of them finished, and it reviews the rest ([REVIEWING.md](../REVIEWING.md)). |

## Changes after the first review (2026-11-13)

- **Unit 3 runs beside unit 1.** [Core review 1](../reviews/2026-11-13-core-review1.md) failed with 2 P1 and 7 P2. Its fix worker started at once on `crates/**`, and must keep the core items that the desktop uses compatible. The collector resolves any overlap with unit 1's one core hook at the merge.
- **Reserved names (FR-02).** The deny list failed twice, so its premise goes: AKM cannot know every variable by which a program chooses a program to run. One function gives a name one of three classes: reserved (refused everywhere, by family shape), credential (a narrow shape) or other. The path where names come from a file, `akm import`, refuses other names until the user allows each one or skips them. The desktop import confirmation marks the other names in unit 1's fix round. The contract and [CLAIMS.md](../CLAIMS.md) state the limit.
- **Backup pruning (FR-01).** A file's own name and bytes prove nothing. AKM deletes a backup only when a record that only its save code writes says that AKM made it.
- **Three more fault classes** for the AGENTS.md table, when their guards exist: a guard that reads input differently from its consumer and fails open (FR-28); arithmetic on a time value from the user without a check (FR-15); a destination chosen by an absence check, then written with a replacing rename (FR-14, core).

## Gates that need the owner

- GitHub Actions billing, so that CI can run again.
- The rule change that lets a disposable CI runner run the clipboard and real-window proofs (AGENTS.md, Safety).
- OS lock, suspend and a real idle wait, on the owner's computer.
- Each merge. Publisher signing, and the choice to publish.

## Order

1. Unit 0, then units 1, 2 and 4 in parallel, with the first review of unit 3.
2. Collect: merge, full checks, the real-surface proofs that need no owner.
3. One review for each of units 1, 2 and 4. Fix rounds. Unit 3 fixes.
4. Area probes and their fixes.
5. The owner's gates. Then the full review.
