# UI rebuild review 2 (re-check of the review 1 fixes)

- Model: Codex GPT-6.1 Terra, effort max.
- Reviewed commit: `d63c2a1976c9fb54bc0ee7292ca73e204ff8fac9` on `ui/key-cabinet`, against `49a54e8`.
- Build hashes: the reviewer did not check a build.
- Date: 2026-11-08.
- Result under the pass rule: **fail** (1 P2, 4 P3). This is the second failed review of this work unit, so the AGENTS.md stop rule applies (see STATE.md).

The text below is the reviewer's, unchanged, except that a link to the reviewer's local temporary folder is removed.

---

## Ranked findings

1. **P2 — Recycle Bin hides information needed to distinguish entries before permanent deletion.** At `d63c2a1:desktop/src/styles.css:328` and `desktop/src/components/RecycleBin.tsx:115`, project, environment and history are forced onto one clipped line. At 900 px, I reproduced this with two fake entries sharing a title and differing only near the end of their project names. Both details were clipped; the delete confirmation at `RecycleBin.tsx:160` identifies only the shared title. The same entries wrapped fully on `723914f`. I did **not** delete either entry. This is a rebuild defect that survives the fix commit, rather than one introduced by that commit.  
   **Closing test:** At 900 and 1040 px, render same-title recycled entries with long, differing projects and environments. Assert their complete distinguishing information is visible and that the permanent-delete confirmation identifies the selected entry.

2. **P3 — Settings can keep an outdated count indefinitely.** The fix stores the first successful list length at `d63c2a1:desktop/src/components/Settings.tsx:46` and then prefers it over every later `status.keyCount` at line 108. I reproduced the component behavior with a fake list response of one key followed by a status update reporting two: Settings continued to display one. An in-flight mutation that completes after the list read is one possible way to produce that ordering; I did not reproduce that timing through the native app. This stale-count behavior is introduced by the fix commit.  
   **Closing test:** Delay a dispatched fake mutation until after Settings reads the old list, deliver the newer status, and assert Settings converges on the new count. Also cover an older list reply arriving after the newer status.

3. **P3 — Unfiled results can still lose their project context.** At `d63c2a1:desktop/src/components/KeysView.tsx:141`, headings are suppressed when all *visible* groups have a null project. I reproduced an Attention shelf containing an unfiled key while a different project also exists: the result has neither a “No project” heading nor a project field on its row. A search or environment filter can produce the same condition. This remained from the rebuild.  
   **Closing test:** Exercise All keys, Attention, search and environment filters with mixed filed and unfiled keys. Assert each displayed row’s project or “No project” is visibly attributable to it.

4. **P3 — Long environment-variable names are truncated in the key list.** The old `.key-var` allowed long names to wrap; `d63c2a1:desktop/src/styles.css:330` clips them with an ellipsis. The complete value remains available after opening the key detail, but users lose the old list-level fact. **Found by inspection.**  
   **Closing test:** At 900 px, show two long fake variable names that differ at the end and assert both complete names can be read from the list.

5. **P3 — The key-list summary drops two old facts.** The pre-rebuild screen displayed the number of projects and an explicit dash for a key without an expiry date. The new screen omits the project total and conditionally omits the expiry marker at `d63c2a1:desktop/src/components/KeysView.tsx:220`. The screen-reader description still says “No expiry date.” **Found by inspection.**  
   **Closing test:** At 900 px, assert the project total and an explicit visible no-expiry indication for a fake key.

## Earlier findings

“Fixed” below describes the checked behavior. None is formally closed while this review fails.

- **UI-R1-01:** The original narrow Activity detail defect is fixed in the tested scenario. The new Playwright check passed at 900, 1040 and 1280 px on the fix; with the old Activity component and CSS, it failed at 900 and 1040 and passed at 1280. Its fixture uses an `add-completed` record with a generic detail, so the specifically requested separate import and export fixtures remain untested.
- **UI-R1-02:** Fixed by inspection and regression test. The new test failed on the old component at six supplementary-plane characters and passed on the fix. It checks the twelve-character indicator but does not submit that twelve-character password.
- **UI-R1-03:** The reported single *named-project* search case is fixed and its test fails on the old component. The unfiled variant remains, as finding 3.
- **UI-R1-04:** The reported count immediately after an already completed mutation is addressed by the fresh read, and the new test fails on the old component. Finding 2 shows a later update can still leave the displayed count stale.
- **UI-R1-05:** The two cited instructions now match the inspected route and script. This was verified by inspection; the native import proof remains unrun.

## Source acceptance: NO

The P2 Recycle Bin finding fails the pass rule. This is the UI rebuild’s second failed review, so the project’s stop rule applies before further patching. The test evidence above is tied to the exact source snapshots named; the working checkout itself was at `49a54e8`, so I tested an isolated archive of `d63c2a1` without changing the working tree.

The three targeted Vitest files passed **27/27** on the fix. Their three new assertions failed against the corresponding old components. The Activity Playwright file passed **9/9** against the fix using a headless browser, fake native boundary and Vite development server; no production build was made.

## Work unit complete: NO

The source review failed. The final-build GUI and setup-program proofs still need the implementer’s rerun, the six owner-present tests remain unrun, and **R4-04-SCRIPT remains open**.

## Not covered

I did not launch the native GUI, touch the clipboard or saved app settings, change windows or the OS session, install a build, run a full suite, or independently verify installer behavior. For the new Settings request, inspection shows session-scoped native reads and a mounted guard; the component probes do not prove every native expiry or transport timing.

The automatic execution policy rejected deletion of the task-owned temporary test folder as **“blocked by policy.”** It remains at the temporary review folder, containing about 193 MB of test files and a task-local handoff receipt; **0 bytes were freed**. No task process or listener on port 1438 was found before the attempt. Shared observer registration and handoff were not performed because its required artifact root is inside the project, while this review forbade project writes.
