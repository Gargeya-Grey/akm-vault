# Diff review of fix round 4

- Model: Codex GPT-6.1 Sol, effort max (the campaign's review model; see REVIEWING.md). One reviewer, in its own checkout.
- Reviewed range: `dfb563b..f9b1380` on `gate/integration` (fix round 4, for the four Low defects of the [third full review](2026-11-14-full-review-3.md): FX-02 to FX-05).
- Build hashes: none. The reviewer ran selected tests and probes, and checked no build.
- Date: 2026-11-14.
- Result under the pass rule: **fail** (1 P2, 2 P3). All four fixes of the round hold. The P2 is in code that the round did not change. This review is not a score: the score stays the 91.75/100 of the third full review.

The text below is the reviewer's, except that links to the reviewer's local checkout are written as repository paths, and its closing notes about its folders and its cleanup are removed. The rows are in [FINDINGS.md](../FINDINGS.md), "Diff review of fix round 4".

---

**Fail: 0 P0, 0 P1, 1 P2 finding.** Also found 2 P3 defects. Source acceptance **NO**; work unit complete **NO**.

Reviewed `dfb563b..f9b1380` at HEAD `f9b1380`. No tracked files changed; no commit made.

| Fix | Assessment | Evidence |
|---|---|---|
| FX-02 | **Holds** | Missing creation times survived edits, rotation, import, recycle, restore, history and an independent KDBX 4.0 upgrade. Purge also worked. |
| FX-03 | **Holds** | Literal `api-key`, clearing, missing/empty fields and unchanged-file updates passed. KeePassXC and kdbxweb read the empty username field correctly. |
| FX-04 | **Holds** | Rail controls passed hit-testing and Tab navigation at enlarged sizes, in both themes and forced colours. Highlight placement and overlay coverage held. Normal-size geometry matched the previous rules. |
| FX-05 | **Holds** | All six key statuses wrapped with long adjacent columns. Drawer and Activity status checks also passed. |

**D4-01 — P2: a notes edit invents a secret-change time.**  
`crates/akm-core/src/vault.rs:439`

A foreign entry with no `Password` field becomes an edit input with an empty secret. The comparison treats `None` and `Some("")` as different, although `write_fields` leaves the password untouched. Consequently, a notes-only update writes `AKM_SECRET_CHANGED`, changes rotation tracking from unknown to known, and reports rotation due in 30 days.

**Reproduced** through the core, save/reopen, release-independent CLI execution and kdbxweb readback. The CLI simultaneously printed “The secret is unchanged.” **Class R; contract 13.**

Smallest fix: calculate `secret_changed` using the same normalized secret comparison as `write_fields`. Closing regression: remove `Password` and the secret-change record from a foreign entry, edit only notes, reopen, and require both to remain absent.

**D4-02 — P3: the theme button covers Browse after automatic scrolling.**  
`desktop/src/styles.css:382`

At 900 × 620 with 32 px text, switch to creating a vault, focus Vault file, then press Tab. Browse receives focus at approximately y=0–44, underneath the fixed theme button. Clicking Browse’s centre changes the theme instead of opening Browse.

**Reproduced in both themes**, including actual centre clicks against the mocked boundary. This is another instance of the **FW-07 enlarged-text/covered-control class**, outside the rail.

Smallest fix: reserve unobstructed scroll/focus space for the fixed control, or place it in normal flow. Closing test: Tab to Browse after scrolling and verify that its centre hits Browse.

**D4-03 — P3: long titles overflow the attention summary.**  
`desktop/src/styles.css:517`

A title consisting of `FAKE_` plus 40 `A` characters fits normally but overflows its summary column with enlarged text. At 32 px, the 808 px stage requires 916 px of horizontal space.

**Reproduced in both themes.** Another **FW-07 text-layout instance**. Adding `overflow-wrap: anywhere` to `.attention-summary` restored the stage to 808 px in the probe. Closing test: show the unlock summary with that title at 24/32 px and require wrapping without horizontal scrolling.

**Documents.** Contract 13 promises more than D4-01 currently delivers. I found no current, nonhistorical document promising the old `api-key` default username. The intentionally open FX records were not findings.

**Tests.** None of the new tests was established as vacuous or unjustifiably tied to internals. Reinstating the previous CSS declarations made the actual browser helpers detect the original rail and status defects.

I found no real cut hidden by `cutText`’s ellipsis or 1 px exemptions. Its remaining limits matter: it checks horizontal clipping, and its fixtures omit the create-vault screen and long summary titles. With the longer title, the helper detects D4-03.

**Verification and limits.** Passed 5 targeted core tests, 54 form/username/search tests, 40 headless browser tests, the complete kdbxweb interchange check, focused KeePassXC readback, and the fast documentation checker. Native Tauri handlers/windows, clipboard, saved settings, actual Windows scaling, screen readers and full suites were not exercised.
