# New look review 1 (the quiet-instrument restyle)

- Model: Codex GPT-6.1 Terra, effort max.
- Reviewed commit: `f2f66d0` on `ui/key-cabinet`, against `0962cc6`.
- Build hashes: the reviewer did not check a build.
- Date: 2026-11-08.
- Result under the pass rule: **fail** (4 P2, 4 P3).

The text below is the reviewer's, except that links to the reviewer's local checkout are written as repository paths, and the reviewer's closing note about its temporary folder is removed.

---

**Verdict: FAIL. Source acceptance is NO. Work-unit completion is NO.** The review established P2 regressions in modal behavior and accessibility, plus a material layout regression. Native desktop proof was outside the requested scope.

Ranked findings:

1. **P2 — The navigation rail remains clickable through open dialogs.** `desktop/src/styles.css:267` gives the stage a lower stacking order than the rail, trapping its fixed overlay beneath the rail. **Reproduced** with a mocked headless build: open Add key, then click Settings on the rail. The form disappears and Settings opens. On the base commit, the backdrop intercepts that click. This also lets a user leave an in-stage dialog while an operation is busy. **Closing test:** open each affected drawer and confirmation, click every rail control, and verify the backdrop intercepts the click until the dialog is dismissed.

2. **P2 — Forced colors removes control boundaries and the reminder switch.** `desktop/src/styles.css:224` replaces secondary-button borders with shadows, and line 582 removes the reminder checkbox border. In forced-colors mode, shadows are suppressed and the new switch thumb and track both render as the background color. **Reproduced** in headless Edge using the base and new CSS: the base control had a visible border; the new switch has no visible boundary or thumb. The adjacent status text still reports its state. The new input focus rule also removes the former outline. **Closing test:** exercise inputs, secondary buttons, and both switch states in forced colors with keyboard focus, verifying visible boundaries and focus indicators.

3. **P2 — A project name can push Add key out of view.** `desktop/src/styles.css:404` leaves the selected project heading unbounded inside the header flex row. **Reproduced** at 1180 px with `Project` followed by 24 `W` characters: Add key was entirely beyond the viewport on the new commit, while part of it remained visible on the base. With 20 `W` characters, it fit on the base but was clipped on the new commit. Horizontal scrolling remains possible, but the primary control is no longer discoverable in its expected position. **Closing test:** select long, unbroken project names at all four requested widths and assert the heading and Add key remain visible without horizontal scrolling.

4. **P2 — The new-user guide invites real-key use before the release gate.** `docs/GETTING_STARTED.md:5` tells any new user to try revocable real keys. The project brief and README require fake keys before the independent review and owner-testing gate; the documented early-use exception is specifically the owner's decision. **Found by inspection.** A reader following this standalone page receives a different trust instruction from the project's release condition. **Closing test:** review the guide against the release gate and make any owner-specific exception explicit and linked to its precautions.

5. **P3 — The current-section marker can become stale after text metrics change.** `desktop/src/components/NavBar.tsx:32` remeasures only on view changes, lock changes, and window resize. **Reproduced synthetically** by increasing navigation text metrics without resizing: the Settings item moved and grew while the marker retained its old position and height. **Closing test:** change font metrics after initial placement without a resize and assert that marker position and height follow the current item.

6. **P3 — A large project shelf pushes the key controls down the page.** `desktop/src/styles.css:406` makes the entire shelf wrap in the ledger. **Reproduced** at 1180 × 700 with 120 fake projects: search began at y=690 and the first key row at y=811. On the base, the separately scrolling shelf kept search and the first row near the top. The controls remain reachable by scrolling. **Closing test:** populate many projects and verify the shelf can be browsed while search and results remain practically accessible.

7. **P3 — Destructive text buttons lose their danger color.** `desktop/src/styles.css:228` assigns `.text-button` a muted color after the equally specific `.danger-text` rule. **Reproduced** with the stylesheet: "Delete for good" and the detail drawer's delete action compute to the muted color. Their wording still identifies the action. **Closing test:** check the computed danger color for both destructive text buttons in light and dark themes.

8. **P3 — The opacity test does not verify an opaque surface.** `desktop/e2e/motion-ui.spec.ts:24` checks element opacity and animation keyframes, but it would also pass if the new `--float` background had an alpha below one. **Found by inspection.** The current `--float` declarations themselves are opaque. **Closing test:** assert the computed dialog and drawer surface backgrounds are opaque in both themes from their first rendered frame.

The modified hover assertion follows the new design's brightness treatment; I did not find that assertion alone to be a weakened guarantee. The data-URL grain is permitted by the configured `img-src` CSP by inspection. I did not report the known UI-R2-01 through UI-R2-05 findings as new findings.

Checks run against isolated temporary copies of the requested revision: TypeScript **PASS**, Vitest **363 passed**, production frontend build **PASS**, mocked headless UI suite **56 passed**, and `check_docs.py` **PASS**. Targeted headless probes compared modal interception and layout with the base commit. The first TypeScript and Vitest attempts failed because my temporary copy lacked a shared fixture; both passed after I copied that fixture. The checkout remains clean and detached at the requested commit.

**Not verified:** the native Tauri/WebView2 app, real Windows forced-colors rendering, installation, clipboard, OS session behavior, and real-vault workflows. The headless probes use mocked native data and do not establish native runtime acceptance.
