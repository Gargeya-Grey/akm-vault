# New look review 2 (the fixes of review 1 and the select list)

- Model: Codex GPT-6.1 Terra, effort max.
- Reviewed commit: `c0a7d3bedc7306df260cc4cd91dbcbfd4c2b7ef8` on `ui/key-cabinet`, against `f2f66d0`; the whole unit is `0962cc6..c0a7d3b`.
- Build hashes: the reviewer did not check a build. `akm-desktop.exe` of this commit: `5d6bd9cb4e5495691d74fab2b40fefc587cbc02689d5086b97804955774a49bd`.
- Date: 2026-11-09.
- Result under the pass rule: **fail** (2 P2, 2 P3). This is the second failed review of this work unit, so the AGENTS.md stop rule applies (see STATE.md).

The text below is the reviewer's, except that links to the reviewer's local checkout are written as repository paths, and the reviewer's closing note about its temporary folder is removed.

---

**Review result: FAIL. Source acceptance: NO. Work unit complete: NO.** Two P2 defects remain in the new select. This is the second failed review of the look unit, so the project's stop rule calls for patching to stop and the design question to go to the owner.

1. **P2: An option refresh can change what Enter selects.** In `desktop/src/components/FilterSelect.tsx:63`, the active option is stored as an index, while `choices` can change during a refresh. **Reproduced** with a mocked native boundary: after an import, I opened Project while the key refresh was pending and highlighted `project-44`. The refresh added an alphabetically earlier project; the highlight moved to `project-43`, and Enter selected `project-43`. A shrinking list also left `aria-activedescendant` pointing to a nonexistent option and Enter selected nothing. **Closing test:** delay a refresh while the list is open, then insert, remove, and reorder options. Verify that the intended option remains active or is safely reset, and that Enter never selects a different option unexpectedly.

2. **P2: Mouse-wheel scrolling is pulled back to the active option.** In `desktop/src/components/FilterSelect.tsx:75`, scrolling the menu triggers the captured scroll listener, which updates placement and causes the active option to be scrolled back into view. **Reproduced** in headless Edge with 45 environments: five wheel movements left the menu at a scroll offset of 6 px, with the first option active. Keyboard End could reach the bottom. **Closing test:** with a list taller than its menu, scroll repeatedly by wheel and scrollbar without moving the pointer; assert that later options remain reachable and clickable while keyboard navigation still keeps its active option visible.

3. **P3: An extremely long project name moves Add key below the viewport.** `desktop/src/styles.css:432` wraps the heading but aligns the button with its bottom. **Reproduced** at 900 × 620 with a 1,000-character fake project name: the button started at y=1466. It remains reachable by vertical scrolling. **Closing test:** use long valid names and assert both the wrapped heading and Add key remain in the viewport.

4. **P3: Set and unset lock pins have the same forced-colours fill.** In `desktop/src/styles.css:652`, the later `.lock-pin` rule overrides `.lock-pin-set`. **Reproduced** in Edge forced-colours emulation: both computed to `GrayText`. Pin position and the visible count still communicate progress. **Closing test:** create a password with some pins set and assert distinct system-colour fills for set and unset pins.

I checked the earlier P2 findings individually. **LOOK-R1-01** held in additional mocked rotation, add, import and upgrade-dialog hit tests. **LOOK-R1-02** has the pin issue above; the tested field boundary and list highlight were visible in forced-colours emulation. **LOOK-R1-03** has the long-name edge case above. **LOOK-R1-04** now directs new users to fake keys only. I also inspected the fixes and guards for LOOK-R1-05 through LOOK-R1-08. The reported fail-first and mutant results were not independently rerun.

`check_docs.py` and `check_no_network.py` passed. The select probes used a copied frontend distribution with fake data and a mocked native boundary; its build provenance was not independently established. I did not launch the desktop GUI or run the full suites. Real Windows forced colours, screen-reader and UI Automation behavior, installation, clipboard, and owner-gated OS tests remain unverified. The implementer's native proof and suite results remain reported evidence.
