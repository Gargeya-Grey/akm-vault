# New look and UI rebuild, review 3

- Model: Codex GPT-6.1 Terra, effort max.
- Reviewed commit: `4763da0` on `ui/key-cabinet`, against `e1e2c94`.
- Build hashes: the reviewer did not check a build.
- Date: 2026-11-09.
- Result under the pass rule: **fail** (1 P2, 5 P3). This is the third failed review of the UI rebuild and of fault class H, so the stop rule applies (see STATE.md).

The text below is the reviewer's, except that links to the reviewer's local checkout are written as repository paths, and the reviewer's closing note about its temporary folder is removed.

---

## Ranked findings

1. **P2: Recycle Bin can still make the permanent-delete target indistinguishable.** In `desktop/src/components/RecycleBin.tsx:126`, two entries can have the same title, project, environment, variable, displayed removal time and history count while differing in issuer. Neither the rows nor the confirmations show the issuer or another unique identifier. A user cannot tell which entry they are about to delete. A targeted component probe **reproduced** this with fake API responses; it did not delete a key or establish native-vault reachability. The same fixture also reproduced identical accessible descriptions for the two Keys-view buttons, despite their different visible issuers.

   **Closing test:** Render two such entries with distinct IDs and issuers. Verify that each row action and its confirmation identifies the target visually and in the accessibility tree, including when all ordinary metadata matches. Verify that confirming sends the intended ID.

2. **P3: Settings retains an old count while its replacement read is pending.** `desktop/src/components/Settings.tsx:48` starts a new list read when the status count changes, but continues to prefer the previous successful list count. A component probe **reproduced** `1 in this vault` after status changed to 2 while the new read remained unresolved. It remains stale for as long as that read is pending.

   **Closing test:** Hold the replacement read pending after a status change, then exercise success, failure, another status change and unmount. Assert the count or loading indication is truthful throughout.

3. **P3: The rebuilt key list still omits old project facts in zero-project cases.** By inspection, `desktop/src/components/KeysView.tsx:151` suppresses the "No project" group heading when every key is unfiled. At line 289, an empty vault also has no explicit project total. The old screen showed "No project" per row and a zero project count.

   **Closing test:** At 900 px, check an all-unfiled vault and an empty vault. Assert the project context and project total are visible.

4. **P3: Extreme text can hide neighboring shelf controls and overflow the deletion confirmation.** A **synthetic headless Edge layout probe** using the reviewed stylesheet at 900 px made a 1,000-character unbroken project button 1,376 px tall inside the 152 px shelf. A neighboring project button was vertically centered outside the initially visible shelf area. The same probe made a long unbroken confirmation title overflow horizontally. The new shelf test in `desktop/e2e/look-guards.spec.ts:266` checks the label's own width, not clipping by the shelf.

   **Closing test:** Use extreme unbroken names at 900 px in both themes. Check the visible intersections of shelf labels and neighboring controls, and verify the full confirmation content is readable without horizontal overflow.

5. **P3: Use keys has no in-place recovery after its list read fails.** By inspection, `desktop/src/components/UseKeys.tsx:92` leaves the project list empty with an error and offers no retry. A later successful refresh after an import also leaves the old error displayed. Navigating away and back provides a workaround.

   **Closing test:** Fail the initial fake read, retry it in the view, and verify a loading state, restored options and removal of the error after success.

6. **P3: The stacking guard does not cover every affected confirmation.** `desktop/e2e/look-guards.spec.ts:37` does not include the Recycle Bin permanent-delete confirmation in its rail hit tests. This is a **coverage gap found by inspection**, not a demonstrated stacking failure.

   **Closing test:** Open that confirmation and hit-test every rail control through its backdrop in both themes, alongside the other required dialogs.

## Source acceptance

- **UI rebuild: NO, fail.** Finding 1 leaves the destructive-selection problem unresolved for another valid component input. I checked **UI-R1-01** and **UI-R2-01**. The prior Activity guard still lacks the separately requested import and export fixtures. I also checked **UI-R2-02 through UI-R2-05** and **FR-11, FR-19 and FR-21**. The reported cases are addressed in part, with the remaining cases above.
- **New look: NO, incomplete.** I checked **LOOK-R1-01 through LOOK-R1-04** and **LOOK-R2-01 and LOOK-R2-02**. The select now tracks a value and the supplied tests cover the reported refresh and wheel cases, but I did not independently run their fail-first evidence. The required dialog-by-dialog stacking guard remains incomplete as described in finding 6. I also inspected **LOOK-R2-03 and LOOK-R2-04**.

A targeted fake-data probe found that duplicate option values, including an empty value, can trap ArrowDown on the first matching option. Both current production callers obtain options through `uniqueOptions`, which removes empty and duplicate values, so I did not rate that as a current workflow defect. The accessible-description tests do use description-aware queries; their fixtures do not cover the collision in finding 1.

## Work unit complete

**NO for both units.** The UI rebuild fails source review, and the new-look review is incomplete. No build hash was checked. Native GUI, final-build and installer proof, real Windows forced colours, screen-reader behavior and owner-gated runtime tests remain unrun.

## Coverage and cleanup

Five targeted component probes passed; their assertions reproduced the stated defects. The synthetic CSS probe and `git diff --check` also passed. The implementer's suite, fail-first and mutant results remain **reported evidence**, not independently rerun. I did not run full builds or suites, launch AKM, use real data, touch the clipboard or settings, or change the OS session. The checkout remains clean.
