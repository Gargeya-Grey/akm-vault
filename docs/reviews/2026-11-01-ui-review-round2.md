# UI review, round 2 (raw)

- Date: 2026-11-01
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: draft PR #2, branch `feat/ui-icons-states-motion` at `496205c` (the fixes for round 1)
- Verdict: REJECT, 4 findings. This is the second rejection of the UI work unit. Under AGENTS.md, patching stops here, and the design questions go to the owner (STATE.md).
- The reviewer could not find `FINDINGS.md`, `ROADMAP.md`, `TESTING.md` and `REVIEWING.md` because this branch starts from `main`, where the cleanup (PR #1) is not merged yet.

The text below is the reviewer's report, copied by hand. Typographic quotation marks became straight, and the reviewer's links to local files became plain `file:line` text or file names. Other differences cannot be excluded.

---

## 1. Original findings

1. **Fixed: dialog and notice opacity.** At the first animation frame, the import notice and the add, detail, and delete-confirmation dialogs were opaque in both themes at 390 px and 1440 px. The focused controls were also visible in the tested views.

2. **Partly fixed: remaining-time bar.** The bar now derives its target from the countdown and resets on re-reveal. Edge still renders an interpolated value that disagrees with the text. A clock rollback also makes the bar overflow its container. Both problems are detailed below.

3. **Fixed: reduced-motion press and hover movement.** In both themes and at both widths, the pressed Add key button and the hovered and pressed row arrow computed to `transform: none` with reduced motion enabled.

4. **Fixed: selected-filter hover border.** The selected Project filter settled on `--ink` when hovered in both themes. Its color transition creates a separate issue below.

5. **Fixed: mixed hover fills.** The primary button retained `--forest` on hover in both themes. A separate destructive-button fill issue remains below.

## 2. Findings

- **P2. The rendered countdown bar lags the text.** `desktop/src/styles.css:1682` transitions the bar's transform for one second. In all four viewport/theme probes, when the text first read "Hides in 29 seconds," Edge rendered the bar at approximately `scaleX(0.9998)` instead of `29/30`, or approximately `0.9667`. The new unit test reads the inline target style, so it misses the rendered discrepancy. **Fix:** Remove the bar's transform transition and add a browser regression that compares the computed transform with the displayed countdown immediately after a tick.

- **P3. A clock rollback makes the new bar overflow the dialog.** `desktop/src/components/KeyDetail.tsx:252` divides the countdown by 30 without handling values above 30. With a simulated 40-second rollback, the text read 68 seconds and the bar grew beyond twice its normal width. At 390 px, the dialog's scroll width grew to 734 px against a 364 px client width. The countdown's clock sensitivity predates this unit, but this horizontal overflow is new. **Fix:** Remove the bar, or hide it whenever the countdown is outside its 0-30 second range and leave the text as the indicator.

- **P3. Controls animate colors despite the transform-only requirement.** `desktop/src/styles.css:440` and the following control rules transition color, border, background, and decoration colors. During a selected-filter hover, Edge rendered intermediate colors outside the token palette before reaching `--ink`. **Fix:** Make those color changes immediate and restrict animation transitions to permitted transforms, including the row-arrow rules.

- **P3. A busy destructive button has a clay fill.** The new busy rule in `desktop/src/styles.css:570` gives the disabled "Working..." delete button a `--clay` fill in both themes, contrary to the stated forest-fill rule. The enabled destructive button's clay fill predates this unit; the busy fill is new. **Fix:** Use `--forest` and `--on-forest` for both enabled and busy filled destructive buttons, retaining the warning in their text and dialog.

## 3. Checks

- **PASS:** `git diff --check main...496205c`. The review checkout remained clean at `496205cd8155c96fa6a9140165696203198ed1a0`.
- **PASS:** `npm test`, 191 tests; `npm run build`; `npm run test:ui`, 38 headless Edge scenarios; and `npx tsc --noEmit -p tsconfig.e2e.json`.
- **PASS:** Both new regression files passed at the reviewed head. Against the `5fca93e` fixed-file versions, the timer test failed and all six motion scenarios failed as expected. The combined hover test stopped at its filter assertion on the baseline, so a separate probe measured the old and new button fills.
- **FAIL:** The independent rendered-timer assertion failed in all four width/theme combinations. The other independent dialog, reduced-motion, hover, reset, and mocked-lock probes passed.
- **PASS:** The checked new token pairs met their applicable contrast thresholds in both themes. The light clay-on-sage text pair measured approximately 4.504:1.
- **LIMIT:** Tests used fake data and a mocked native boundary in headless Edge. Native Tauri/WebView2, Rust, clipboard, real-window, and OS behavior were not tested. No project verification skill was found. The instructed `FINDINGS.md`, `ROADMAP.md`, `TESTING.md`, and `REVIEWING.md` files were absent from this checkout; the available ledger and verification guide were consulted.
- **LIMIT:** An initial temporary archive setup failed. Tests ran on a fresh archive, with the relevant head and baseline files checked against their Git blob identities. The shared observer rejected the `%TEMP%` artifact root because it requires a root inside the project. The task-local receipt (`handoff.json`) and probe log (`reviewer-probes.log`) in the review temporary folder retain the evidence. No matching review processes or listeners remained.

## 4. Verdict

**REJECT.** The rendered timer still contradicts its text, and the new bar can introduce horizontal scrolling after a clock rollback. The color transitions and destructive busy fill also conflict with the stated style requirements. This is the unit's second rejection, so it should return to the owner with the design question of whether to remove the bar or retain an immediate, range-limited indicator.
