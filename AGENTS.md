# AKM agent rules

AKM is a Windows API-key manager. It has a Rust core and CLI, and a Tauri + React desktop app, over a KeePass (KDBX) vault. The project will become open source later. Write every tracked file as if it were public.

## Start here

1. [docs/STATE.md](docs/STATE.md) gives the current status and the next action. Read it at the start of every session.
2. [docs/PROJECT.md](docs/PROJECT.md) gives the goals, the non-goals and the contracts that every change must keep. Read it before you change behaviour.
3. [docs/FINDINGS.md](docs/FINDINGS.md) lists every open review finding. Read it before you plan a fix or a review.
4. [docs/ROADMAP.md](docs/ROADMAP.md) lists the remaining work units in order.
5. [docs/TESTING.md](docs/TESTING.md) gives the commands that define done, and the tests that need the owner.
6. [docs/REVIEWING.md](docs/REVIEWING.md) gives the review brief and the models for reviews and side jobs.

## Safety

- Use fake data only: temporary vaults and fake keys. Real secrets wait for the release gate. That gate is an independent full review that scores above 90/100, followed by the owner's own testing.
- Keep the owner's clipboard intact. A test that writes the clipboard needs its empty-clipboard guard and the owner present.
- Lock, suspend or sign out of Windows, or minimize or hide a real window, only after the owner confirms in chat that they are ready to watch. Each confirmation allows one attempt. A failed attempt stops, and the result goes back to the owner.
- The desktop app's saved settings point at a fake vault from an older proof run (see STATE.md). Keep that setting until the owner gives the value to restore.
- Keep KDBX compatibility. The format is established. This integration and the community `keepass` writer are not audited.

## Owner gates

These need the owner's yes in chat each time:

- making the repository public, or publishing a release or installer;
- installing a build, or code signing;
- using real secrets;
- turning on GitHub Actions;
- merging a pull request;
- the visible-window and OS tests in Safety.

You can do these without asking: branches, commits, pushes and pull requests on the private `Gargeya-Grey/akm-vault` repository.

## How work runs

- Git is the record. Each commit or pull request says what changed and which checks ran, with their results. Put disposable scripts, probes and logs in `.scratch/`, which Git ignores. Commit only what becomes a real test or tool.
- Before each commit, read `git status` and the exact staged diff. Before each push, list every outgoing branch and tag, and read each new commit's changes (`git log -p <remote>..<ref>`), including any parentless commit. A file that a later commit removes is still in the pushed history. Look for secrets, vaults, screenshots and private evidence. Ignore rules are a guard, not a secrets scanner. Keep unfamiliar changes that you did not make. Ask the owner about them.
- A code defect closes only when two things are true: a permanent regression test fails on the affected version and passes after the fix, and a later review that covers it (a unit review or the full review) verifies the fix. A design, coverage or premise finding closes when its decision or evidence is verified and reviewed. Write down any limit that remains.
- Each work unit gets one fresh review (REVIEWING.md). A review passes when it establishes no P0, P1 or P2 finding and its P3 findings have moved to a named later unit ([pass rule](docs/REVIEWING.md#pass-rule)). After that review, the meter is the full review: it gives the score, and no further unit review is a gate ([design/gate-campaign.md](docs/design/gate-campaign.md), "Root cause of the loop").
- If one fault class fails review twice, stop patching it. Narrow the promise that it breaks, or remove the feature. The planner decides and writes the decision in STATE.md for the owner's veto. Ask the owner only about money, an account, an irreversible step, a Safety rule or an owner gate.
- A fix is the smallest change that makes the regression test of its finding pass. It adds no module, mechanism or design section, unless two P2 findings need the same one. A round that adds more than 400 lines outside tests for one unit says why.
- Prove a change on the real surface with the verification skill in [.agents/skills/verify-akm/](.agents/skills/verify-akm/SKILL.md). It keeps to the fake-data and clipboard rules in Safety.
- The latest measured score is 66.25/100, from the full rescore of 9 Nov 2026 on `c0a7d3b` (docs/reviews/2026-11-09-full-rescore.md). The baseline of 30 Oct 2026 was 40.5/100. Never add projected points to a score. A new score needs a new full review.
- The score is one of three measures, and none replaces another (owner, 13 Nov 2026; [STATE.md](docs/STATE.md), "Destination, measures and path"): defects, by the full review and the kinds of its deductions; completion, by the table of required workflows; and behaviour on a real desktop, by the owner-present tests. Weigh a removal of a feature against the required workflows, not against the points.
- A defect in work that was called done names a fault class. Find every instance of the class in the tree, and apply the same small fix to each. Use a guard from the table below where one exists. A guard is a tripwire for the usual spelling of the mistake, not a proof: a way around it is a P3 finding at most, and the guard is not rebuilt for it. A new class gets a new row only when its guard is a few lines.
- Before you stop, update STATE.md: what changed, the checks you ran, the open findings, and one next action.

## Fault classes

From phase two review 1 ([FINDINGS.md](docs/FINDINGS.md), P2R1 rows) and the collect step.

| Class | Guard |
|---|---|
| A. A check, then an act after the gate is released | Run a session-scoped effect through `Session::commit` ([design/commit-point.md](docs/design/commit-point.md)); the act takes the `Held` witness. Where the act cannot run under the gate (a modal dialog), check again after it. Tests: a barrier between the check and the act, and `every_session_scoped_act_goes_through_the_commit_point`. |
| B. A capability or snapshot used outside its source | Bind it to its source (vault, journal, session, fingerprint) and check that binding where it is used. |
| C. State advanced before its record is durable | Write the record first; advance the state only after the write succeeds. Test: fail the write, then retry. |
| D. A doc that names a stale test or behaviour | `python tools/check_docs.py`. |
| E. Control characters in a doc | `python tools/check_docs.py`. Write a doc-editing script as a file, never as a shell here-document. |
| F. A tool that deletes or writes what it does not own | It touches only what it created or marked, and only after its checks pass. Test: sentinel files survive. Tests never write the real journal folder (`JournalRoot::from_environment`). |
| G. An invisible encoding mark stored as data | Where text enters from a stream or a file (`secret_value`, `dotenv::parse`), drop one leading byte order mark. Test: feed the mark and read the stored value. |
| H. A new layout drops a fact the old screen showed | A media query moves a fact, and never removes it. A redesign compares the fields of the old and new screens. Test: the fact is visible at the minimum window width (900 px). |
| I. A stacking context traps a dialog below a sibling | Every dialog must cover every control outside it. Test: `look-guards.spec.ts` hit-tests the rail through each dialog, in both themes. |
| J. A boundary, a mark or a state drawn only with a shadow or a fill | Forced colours remove both. Add the element to the forced-colours block at the end of `styles.css`. Test: the `forced colours` group in `look-guards.spec.ts`. |
| K. Text from the vault or the page in a native review | Pass it through `review_name` or `review_path`. Show it with `review_names`, with Cancel as its default button (`review_window.rs`). A failed window is an error, never a decline. Under the owner's option (a) a names-only window can appear just after a lock: nothing that discloses or starts may follow it without a new session check. FINDINGS.md T-R rows call this class I. Test: a name with a newline and a directional mark stays on its line. |
| N. An exit after dispatch that leaves no outcome | A request whose effect outlives the page runs through `outcome::dispatched`: one guard records one outcome on every way out, also a panic. Test: `class_n_every_way_out_of_a_dispatched_request_leaves_one_true_record`. |
| P. An effect that cannot be taken back, after a check | Prepare outside the gate, commit one short act under it (`Session::commit`), revoke after a refusal. A terminal starts suspended and is resumed or ended at the commit. Test: the state table of [design/commit-point.md](docs/design/commit-point.md), row by row. |
| Q. A wait for an event that must happen, with a bound that a loaded run exceeds | Use the one bound `PATIENCE` (`test_support.rs`). A wait that proves promptness or absence keeps its own short bound. Test: `class_q_every_wait_for_an_event_that_must_happen_uses_the_shared_patience`. |
