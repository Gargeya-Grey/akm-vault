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
- **Fix by class (replaced on 2026-11-13, see "Root cause of the loop").** A finding names a fault class. The fix makes the class impossible by construction where that is practical: one mechanism, every caller moved to it, the old path deleted, and a guard that fails when a new caller brings the class back. Each test has a mutant that it catches.
- **Fewer promises.** Where the operating system cannot give a guarantee, the product does not promise it. The limit is written in the contract, in the user documents and in [CLAIMS.md](../CLAIMS.md), and the review receives it.
- **Review loop (replaced on 2026-11-13, see "Root cause of the loop").** A unit gets a class sweep by its implementer, then a review ([REVIEWING.md](../REVIEWING.md)). At most 4 rounds per unit. A class that fails two rounds gets a redesign that removes its premise, chosen by a design review, before a third fix. A question goes to the owner only when it is about money, an account, an irreversible step or a contract change.
- **Probe before the score (replaced on 2026-11-13, see "Root cause of the loop").** Before the full review, each of the nine rubric areas gets a probe review with the full-review brief. The official full review starts only when the probes find no P2. A probe gives no score.
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
- **Reserved names (FR-02).** The deny list failed twice, so its premise goes: AKM cannot know every variable by which a program chooses a program to run. One function gives a name one of three classes: reserved (refused everywhere, by family shape), credential (a narrow shape) or other. Names come from a file on one path only, the desktop import (the CLI has no import command, and none is added). Its native confirmation lists the other names apart from the credential names; unit 1's fix round does that. The contract and [CLAIMS.md](../CLAIMS.md) state the limit.
- **Backup pruning (FR-01).** A file's own name and bytes prove nothing. AKM deletes a backup only when a record that only its save code writes says that AKM made it.
- **Look-alike rows (unit 2).** [Unit 2 review 1](../reviews/2026-11-13-unit2-review1.md) broke the look-alike check a third time (collapsed spaces, and a separator inside a value). The premise goes: no code can decide when two rows look the same to a person, because look-alike letters and invisible marks also exist. Every key row always shows a short ID, and each confirmation lists every fact of its key and the ID.
- **Plaintext in the page (unit 2).** A plaintext value in page state is bound to the session, the scope and the generation of the list that it came from. Any change of one of them clears it, and only a new request shows it again.
- **Error provenance (units 1 and 2).** A native error reply carries a `kind` from a closed list that both sides hold. A rejection with any other shape is a transport failure, and the page keeps the outcome record.
- **Unit 4 after its second failed review.** Four classes failed twice, and each gets a design that removes its premise ([STATE.md](../STATE.md), "Unit 4 after two failed reviews"): the snapshot tool reads the source repository as files and runs Git only on its own copy; a text file may hold only plain characters, an address is any non-space run around `@`, and one list of exact allowed texts replaces the inline marker; the notices generator sorts every file of a package or fails; the claims check asks the test runners which tests exist.
- **Unit 3 after its second failed review.** Four classes failed again, and each gets a design that removes its premise ([STATE.md](../STATE.md), "Unit 3 after two failed reviews"): a backup is deleted only when the file object has the identity that the vault recorded; no rule sorts a name by its shape, so a name is reserved or it is not, and the import confirmation says one caution about every name; only a recorded fingerprint confirms a vault for its journal; AKM refuses to write a vault file that has more than one name. One new class: each parser of bytes that are not authenticated has a size limit and a count limit.
- **Unit 1 after its first review.** [Review 1](../reviews/2026-11-13-unit1-review1.md) failed with 5 P2 and 2 P3. One design for each class, for the owner's veto: (B) a value that left the vault can be opened only by one function that checks its whole stamp (session, vault revision and view) under the gate, so no caller can drop a part of it; (P) a clipboard copy owns its publication, and every refusal after the publication takes it back, whatever the cause; (A) the hand-over of a notification to Windows is itself the act under the gate, and its real result is the delivery state; (N) the step that cannot be taken back marks its outcome record under the gate, so every later way out, an unwind too, reports a commit as a commit, and no reply after a commit invites a retry; (F) a temporary file has a new random name, is created only when no file has that name, and is the only file that its owner deletes; the guard of the commit point uses names that the compiler resolves; an operation ID has one accepted spelling.
- **Unit 2 after its second failed review.** Two classes failed again, and each gets a design that removes its premise ([STATE.md](../STATE.md), "Unit 2 after two failed reviews"): every message from the native code changes the data generation unless its kind is listed as changing nothing, and the lists are read again from that generation; the source guards take resolved modules and decoded texts from the TypeScript compiler.
- **Three more fault classes** for the AGENTS.md table, when their guards exist: a guard that reads input differently from its consumer and fails open (FR-28); arithmetic on a time value from the user without a check (FR-15); a destination chosen by an absence check, then written with a replacing rename (FR-14, core).

## Root cause of the loop, and the changed rules (2026-11-13)

The owner asked for the cause of the problem and told the planner to act as the tech lead and the product manager. This section is the answer. Each rule and each decision here is for the owner's veto.

### What the numbers say

- **The score was not measured since 9 Nov.** It is still 66.25/100. The campaign added 45,652 lines and removed 4,194 (`main..gate/integration`) with no new measurement.
- **Each fix round wrote a new subsystem.** Lines added and removed by round: unit 1, 3,657 and 694 (native code); unit 2 round 1, 3,593 and 779; unit 3 round 1, 4,674 and 500; unit 4 round 1, 2,143 and 312.
- **Each review after a round found about as many P2 findings as the one before.** Unit 1: 5. Unit 3: 6. Unit 4: 4. Unit 2: 4, then 1.
- **Old fixes hold.** Unit 1 review 1 tried a second scenario on about 35 earlier rows and found no defect. Unit 2 review 2 found 8 fixes sound. Unit 3 review 2 verified 7 of 11.
- **New findings are in new code.** Of the 16 open P2 findings, 5 are in the new native code of unit 1, 4 are in tools (3 of them in one tool, the public-snapshot scan), and 3 are a third failure of one promise each (backup ownership, name classes, journal confirmation). Six findings are against the guards that earlier fixes added (U1-R1-06, U2-R1-05, U2-R1-06, U2-R2-03, U2-R2-04, U4-R2-05).

So the loop does not converge for one reason: a round fixes N findings and writes enough new code for about N new ones.

### Causes

1. **Fix size.** The rule "fix by class: one mechanism, a guard, a mutant" turned each finding into a mechanism, a structural guard and a design section. Example: for a notification that shows a count only, the round in progress added a second notification library and a lint rule.
2. **Universal promises.** "Only", "never", "every" and "confirmed" need one counter-example to break, and the pass rule rates a broken contract P2. Three promises failed three reviews each. The product needs none of them in that strength.
3. **The wrong meter.** A unit passed only with no P2 finding from a reviewer at maximum effort who assumes a defect. The release gate is the score of a full review, which counts deductions. The plan held back the full review until every unit passed, so nothing measured the distance to the gate.
4. **Tools and guards with strong claims.** They are not the product, and each became review surface. The public-snapshot tool answers a deduction of about 0.5 points and has had about 10 P2 findings.
5. **The planner's briefs.** After each failed review the planner wrote a larger design, also for P3 findings. This is the planner's fault, not the workers'.
6. **Owner gates hold points.** About 3.25 points of the 33.75 deducted on 9 Nov need the owner (an estimate, not a score): signing, the updater, the install test, the native proofs, the redesigns that the owner has not accepted, and CI.

### Rules from now on

These replace "Fix by class", "Review loop" and "Probe before the score" above.

- **Smallest fix.** A fix is the smallest change that makes the regression test of its finding pass. A class sweep finds the other places with the same defect and applies the same small fix. A round adds no module, mechanism, structural guard or design section, unless two P2 findings need the same one and the brief names it. Budget: 400 added lines outside tests for one unit and one round. A worker that needs more says for which finding, and why.
- **Narrow the promise first.** When a finding breaks a promise that the product does not need in that strength, the planner narrows the promise in the contract, the threat model, the claims list and the screen text. The code stays. A promise names what AKM does, and avoids "only", "never" and "every" where the operating system or a file can make it false.
- **Remove before you build.** A feature or a tool that failed review twice and has small value for the user is removed or narrowed. It does not get a third mechanism.
- **A guard is a tripwire.** A structural guard reads the usual spelling of a mistake and says so in its comment. A way around a guard is a P3 finding at most, and the guard is not rebuilt for it. No new structural guard, unless it is a few lines.
- **Measure with the meter of the gate.** The four rounds in flight are collected. Then one full review with the official brief runs on the integrated tree and gives a measured score. No further unit review is a gate. After the full review, fixes go in the order of the size of their deduction, under the rules above, and a second full review follows. If two full reviews in a row do not bring the deductions near 10, the planner tells the owner that the score is not reachable with this surface, with the features to cut.
- **Short records.** One row for each finding, the raw review file, and the next action in STATE.md. No table of design questions for each round.

### Product decisions (planner, for the owner's veto)

- **The Windows notification goes away.** It shows a count, and only while AKM is open and unlocked, where the Keys view already shows the same count. The plugin hands the notification to Windows on its own task, so "never after a lock" cannot be true without a second library (U1-R1-03). Removing it also removes a dependency, a settings switch and an owner-present proof. Done on 2026-11-13 (`eb9f390` to `296c4ec`, merged as `785748a`): 508 lines of code outside tests and 923 lines of tests removed, and five crates left the Windows dependency tree.
- **Backups: the promise is narrowed, and no file identity record is built.** AKM keeps the five newest backups that the vault's list names beside the vault, and deletes older listed ones. A copy of a vault folder carries the list, so the rotation applies to the copy too (CORE-R2-01). A file that the list does not name is never deleted.
- **Tools.** The public-snapshot tool, the notices generator and the claims check get no further review round before the full review. Their use is at publication, which is an owner gate; their open findings move to that gate unless the full review deducts for them.
- **Unit 2 round 2** was done by the planner as six small fixes, in place of the two designs of "Unit 2 after its second failed review" above (the data clock for every native message, and guards that ask the compiler). Those two designs are withdrawn.

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
