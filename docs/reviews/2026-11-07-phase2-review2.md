# Phase two review 2 of 2 (units 1c, 1h, 1b, 2, 3, 4, tools)

- Model: Codex GPT-6.1 Terra, effort max (owner instruction, 2026-11-07: two reviews at most). A first attempt was refused by the provider's safety filter before it produced a review; this is the second attempt, with the brief's wording softened ("adversarial" and "attack" removed) and its content unchanged.
- Reviewed commit: `3391b94c0ce8a77c734e0c8f5dbcefbf342e129a` on `phase2/units-1c-to-4`, against `81f2689`.
- Date: 2026-11-07.
- Result under the pass rule: **fail** (4 P2, 1 P3). Units 1c, 1b and the tools failed both phase two reviews, so the AGENTS.md stop rule applies (see STATE.md).

The text below is the reviewer's, unchanged, except that two absolute file links are shortened to repository paths and the reviewer's local temp paths are removed.

---

**Review 2 result: FAIL.** On the requested revision, I found four P2 defects: the fetch tool can delete added user files, a hidden import can still commit, a delayed discard can affect a newer session, and a same-path clone replacement mixes journal histories. The fetch and journal findings were reproduced with fake data. The two session interleavings were found by inspection.

Reviewed source: `3391b94c0ce8a77c734e0c8f5dbcefbf342e129a`, against `81f2689`. During review, the shared checkout advanced to `d628130` and `docs/STATE.md` acquired a concurrent edit. I preserved those changes. Core files used for the reported core tests were unchanged from the requested revision. The later CLI test results are identified separately below.

## Ranked findings

### P2. The fetch tool deletes foreign files inside a fetched directory

**Unit:** Tools. **Fault class:** F. **Reproduced with fake data.**

`tools/fetch_keepassxc.ps1:47` checks only the destination's immediate children. Any directory named `KeePassXC-*-x64` is treated as tool-owned, regardless of what was subsequently placed inside it. Line 113 recursively deletes that directory during a forced replacement.

I fetched a fake archive into a new marked destination, added a fake `user-evidence.txt` inside its extracted directory, and fetched again with `-Force`. The tool succeeded and the sentinel was gone. The existing test adds `notes.txt` at the destination's top level, so it does not catch this case. The check also does not inspect reparse points in ancestor paths or within accepted children.

**Closing test:** Fetch into a new fake directory, add nested files and nested directories that the tool did not create, then force replacement and require them to survive. Include a reparse-point ancestor and child, and verify that a failed download or digest leaves all prior content intact. The ownership scheme must distinguish tool-created content from later additions.

### P2. An import can commit after its document was hidden

**Unit:** 1c. **Fault class:** A. **Found by inspection.**

`desktop/src/App.tsx:231` sends `view_discard` asynchronously when the document hides. If that IPC is delayed, the document becomes visible again, and the user approves the still-open native dialog before the discard reaches native code, `run_import` (`desktop/src-tauri/src/commands.rs:843`) can see a currently visible, non-minimized window and an unchanged presentation snapshot. It then commits the import even though the page discarded its review on hide.

The native `window_live` check observes the window's *current* visibility, not whether the document hid since review. The frontend test asserts that a discard was sent and supplies a mocked stale-preview error; it does not exercise this ordering across the production boundary. This leaves contract 7 and the original P2R1-01 scenario incompletely guarded.

**Closing test:** With a fake vault and the production command boundary, hold delivery of `view_discard` after the frontend has handled hide. Restore visibility, approve the native confirmation, and then release the discard. Assert no import is committed and the dispatched operation has a truthful outcome. Also cover a failed or hung discard and the owner-present real-window transition.

### P2. A delayed discard from session A can expire or lock session B

**Unit:** 1c. **Fault class:** B. **Found by inspection.**

`view_discard` (`desktop/src-tauri/src/commands.rs:904`) carries no session or presentation identity. If A's discard is delayed until after B opens, native expires B's current presentation and emits a notice naming B. That can dismiss B's protected view or invalidate B's pending import. On an error, `discardView` (`desktop/src/api.ts:112`) calls `lockCurrent`, which can instead lock B after A has retired.

This is a separate cross-session effect from the import race above and conflicts with contract 5's ownership rule.

**Closing test:** Hold A's discard at the IPC boundary, open B and start a protected view or import in B, then deliver A's success and failure in separate cases. Assert B's presentation, pending operation and unlocked state remain intact, while A's view remains revoked.

### P2. Replacing a vault with its previously recorded clone mixes journal histories

**Unit:** 1b. **Fault class:** B. **Reproduced with real fake KDBX vaults.**

`crates/akm-core/src/journal.rs:363` resolves a same-path, unseen fingerprint as the existing journal when the root UUID matches. Clones share that UUID. The decision does not check whether the new fingerprint is already associated with another active clone binding.

I created fake vault A, recorded `original-marker`, copied it to B, changed B and recorded `clone-marker`, then copied B over A's path and opened it there. A record from the replacement appeared in A's journal alongside `original-marker`; B's earlier `clone-marker` remained in B's separate journal. Thus the replacement displays another vault instance's history and splits its own history. This contradicts the design's promise that clone histories are never merged. The passing stale-reader tests use replacements with different roots and do not cover this case.

**Closing test:** Use two real fake vault snapshots with a shared root and independently recorded histories. Replace one path with the already-observed clone, then alternate reads and disclosures from stale and current sessions, including separate processes. Require no cross-instance records or silent merging, and make ambiguous identity fail closed or otherwise follow an explicitly reviewed decision.

### P3. The dotenv design misstates the reference reader's comment behavior

**Unit:** 3 documentation. **Fault class:** D. **Reproduced.**

`docs/design/unit3.md:49` says python-dotenv 1.2.2 keeps `abc # note` for `TOKEN=abc # note`. The installed pinned reader returned `abc`, matching the committed `comments.dotenv` expectation. The fixture-note test compares the recorded JSON values; it does not check this prose claim.

**Closing test:** Correct the table and compare each stated reader difference against the pinned reader and fixtures. Add a documentation check where practical. A named later unit is required before any provisional pass containing this P3 can become final.

## Review 1 finding status

These are review statuses, not formal defect closures. The project requires the affected work unit to pass review before a code defect closes.

| Finding | Status | Evidence and remaining issue |
|---|---|---|
| **P2R1-01** hidden import | **Partly closed** | The discard command and window checks cover the tested ordered cases. The delayed-discard interleaving above remains open. |
| **P2R1-02** saved-path race | **Partly closed** | The write now takes a `Held` witness under `while_current`; the barrier test is present. I did not independently run its fail-first or desktop test. Unit 1c fails this review for the session findings above. |
| **P2R1-03** disclosure proof | **Partly closed** | Session/state binding and single-use checks are present. All 13 disclosure tests passed, including cross-vault and reuse cases. I did not repeat the reported old-version or mutant runs, and unit 1h has not received complete acceptance coverage here. |
| **P2R1-04** stale reader | **Partly closed** | Four selected stale-reader tests passed for the covered cases. Same-root clone replacement still mixes histories, as reproduced above. |
| **P2R1-05** rollback indication | **Partly closed** | The mapping advances after the owed append, and the selected repeated-failure/retry test passed. I did not independently repeat its fail-first run; unit 1b fails on the clone finding. |
| **P2R1-06** clock floor | **Partly closed** | The shared floor includes entry and history times. Ten selected foreign-history/location lifecycle tests passed. The real-reader and complete unit-2 evidence remain unverified in this review. |
| **P2R1-07** notification after lock | **Partly closed** | The notifier is called under `while_session`, and the barrier test exists. I did not run the desktop test or show a real Windows toast. Windows delivery after a previously accepted handoff remains the stated limit. |
| **P2R1-08** fetch tool | **Open** | The original top-level sentinel and digest-failure cases are guarded, but the reproduced nested-sentinel deletion leaves the ownership class open. |
| **P2R1-09** threat-model documentation | **Partly closed** | The named threat-model claims and test references were revised, and the documentation checker passed on the working tree. It does not establish semantic accuracy across all documents; the unit-3 reference-reader claim above is wrong. |
| **P2R1-10** incomplete review | **Open** | Coverage improved, but this review still did not establish the complete scope and earlier regression evidence. The unit-5 full rescore was explicitly outside scope. |

## Source acceptance and work-unit verdicts

| Subject | Source acceptance | Work unit complete | Verdict |
|---|---|---|---|
| **1c** | **NO, fail** | **NO** | Two P2 session and import findings. Real hide and native-dialog proof is also unrun. |
| **1h** | **NO, incomplete** | **NO** | Disclosure tests passed, but clipboard failure interleavings, all callers and earlier regressions were not completely reviewed. Owner-present clipboard and native GUI proof remain unrun. |
| **1b** | **NO, fail** | **NO** | The reproduced clone replacement mixes journal histories. Full ACL and real Activity-path proof was not established. |
| **2** | **NO, incomplete** | **NO** | Selected clock tests passed, but I did not establish every history, foreign-data and independent-reader acceptance case. |
| **3** | **NO, incomplete** | **NO** | Core dotenv, reserved-name and time tests passed. CLI tests ran against the later checkout. I did not complete the frontend, real export-cleanup and all earlier regression review. |
| **4** | **NO, incomplete** | **NO** | The notification gate was inspected, but desktop interleavings, real toast delivery and real screen-reader behavior were not established. |
| **A1–A3** | **NO, incomplete** | **NO** | The doc checker passed, but I did not independently establish the full no-network, dependency and threat-model scope. |
| **Tools** | **NO, fail** | **NO** | The fetch tool deleted the nested fake sentinel. |
| **5** | **NO, incomplete** | **NO** | I inspected the reader scripts but did not run them on an exact-revision build or perform the separately scoped full rescore. |

## Coverage and limits

The independently run core checks passed: 13 disclosure, 8 dotenv-profile, 6 reserved-name, 7 schedule, 4 stale-reader, 1 rollback-retry and 10 foreign-history/location lifecycle tests. I also ran the fake fetch and KDBX clone probes described above. The CLI selection tests passed 7 cases and time tests passed 3, but their binary was built after the checkout advanced and is **not exact-revision evidence**.

I inspected the tests and code around earlier findings **P2-TIME-REFRESH, 3-SA-01, P2-HISTORY-SEMANTICS, P2-JOURNAL-IDENTITY, 1A-A2-01, 1A-A2-02 and 1A-A2-03** to the extent stated above. I did not verify every earlier P0–P2 regression, every changed caller, or the entire diff. I read a sample of the implementers' fail-first and mutant reports, but did not independently repeat those runs; consequently, I cannot establish that every closing test fails with its fix removed.

By inspection, the KeePassXC script covers the documented lifecycle and merge comparison, and the kdbxweb script covers the documented KDBX interchange and lifecycle paths. Neither was executed in this review. The roadmap's complete cross-reader journal and scoped-environment evidence remains unestablished. The planner's broader check results remain planner-reported evidence.

No GUI was launched, and I did not touch the clipboard, app settings, real journal or OS session. The required real-window, clipboard, notification, lock and suspend proofs were not run. I made no repository edits and did not update `STATE.md`, as the review was explicitly read-only.
