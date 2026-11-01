# UI review, round 3 (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: draft PR #2, branch `feat/ui-icons-states-motion` at `6aa3ee3` (the owner's 3 design decisions after round 2)
- Verdict: REJECT, 1 new finding (P3). The 3 owner decisions are applied. The finding depends on the brief's "entry only" wording, so it goes to the owner as a design question.

The text below is the reviewer's report, copied by hand. Typographic quotation marks and apostrophes became straight, and the reviewer's links to local files became plain `file:line` text or file names. Other differences cannot be excluded.

---

## 1. Round-2 findings and owner decisions

1. **Countdown bar lag: fixed.** The bar is removed. The countdown text is the only timer in `desktop/src/components/KeyDetail.tsx:249`. Independent probes confirmed this in both themes at both widths. Owner decision 1 is applied.
2. **Clock rollback overflowing the bar: fixed.** With a simulated rollback, there was no bar and no horizontal overflow in the tested dialogs.
3. **Animated color changes: fixed.** The stylesheet transitions only `transform`; the selected filter's hover border changed immediately to `--ink` in the probes. Owner decision 2 is applied, subject to the separate exit-motion finding below.
4. **Busy destructive button's clay fill: accepted by owner exception.** Enabled and busy delete buttons both used `--clay` with `--paper` text in every tested configuration. Owner decision 3 is applied.

## 2. New finding

**P3. Controls animate when interaction ends, contrary to the entry-only motion requirement.** `desktop/src/styles.css:442` puts a transform transition on the button's base state. In headless Edge, releasing Add key produced a running transition from `scale(0.97)` back to `none` in both themes at 390px and 1440px. At 1440px, the row arrow also transitioned back when the pointer left; its transition (`desktop/src/styles.css:820`) predates this unit.

**Fix:** Remove the base button and row-arrow transitions. Put the button's transform transition on the permitted `:active` state, and use a hover-entry keyframe for the arrow so leaving hover removes the animation immediately. Retain the existing reduced-motion guards. A temporary CSS override demonstrated this approach in one 1440px light-theme probe; the production source was not changed.

## 3. Checks

- **PASS** `git diff --check main...6aa3ee3`. The review checkout remained clean at `6aa3ee3`.
- **PASS** `npm test`: 191 tests. **PASS** `npm run build`. **PASS** `npm run test:ui`: 40 tests. **PASS** `npx tsc --noEmit -p tsconfig.e2e.json`.
- **PASS** Regression comparison in a temporary copy. The timer test failed 1/1 against the two `496205c` files and passed at head. The changed motion suite failed its color-transition assertion in both themes against those files and passed at head. The copied files' Git blob IDs were verified.
- **PASS** 16 independent headless Edge scenarios across both themes and widths. These covered the owner decisions, first-frame opacity, decorative icon attributes, selected-filter hover, reduced motion, fake-secret hide and expiry, mocked lock, and the prior overflow case.
- **FAIL** The independent entry-only release assertion failed in all four theme and width combinations. Its first run had a locator ambiguity; the corrected run produced the transition evidence above.
- **PASS** Contrast calculations for 22 relevant token pairs. Light-theme clay on sage was close to the text threshold at approximately **4.504:1**.
- **LIMIT** Browser checks used fake data and a mocked native boundary. Native Tauri, WebView2, the clipboard, real Windows behavior and real secrets were not tested. No project verification skill was found.
- **LIMIT** The shared observer rejected registration because the required artifact root was outside the project. Its handoff then failed because the registry entry was absent. The task-local receipt (`handoff.json`, review temporary folder) records the evidence and process shutdown.

## 4. Verdict

**REJECT.** The four round-2 findings are resolved or superseded by the owner's exception, and the requested suites passed. The new reverse transitions nevertheless violate the stated entry-only motion requirement. The finding was reproduced in rendered Edge behavior, while native runtime behavior remains outside this review.
