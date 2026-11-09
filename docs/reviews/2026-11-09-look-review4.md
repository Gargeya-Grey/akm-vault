# New look and UI rebuild, review 4 (last)

- Model: Codex GPT-6.1 Terra, effort max.
- Reviewed commit: `daf160d` on `ui/key-cabinet`, against `4763da0` (the whole change: `e1e2c94..daf160d`).
- Build hashes: the reviewer did not check a build.
- Date: 2026-11-09.
- Result under the pass rule: **fail** (2 P2, 2 P3). The owner chose one last review and a report if it failed, so patching stopped (see STATE.md).

The text below is the reviewer's, except that links to the reviewer's local checkout are written as repository paths, and the reviewer's closing note about its temporary folder is removed.

---

**Source acceptance: NO.** The review found two P2 defects. The Recycle Bin identity changes address the original same-metadata case, but the Keys view still has a visual identity collision, and the new retry path permits stale reads to replace newer results.

**Ranked findings**

1. **P2: Keys rows can still be visually indistinguishable.** `desktop/src/components/KeysView.tsx:155` compares raw issuer values when deciding whether to show IDs. An unset issuer and the literal issuer `No issuer` compare differently but render identically. Two otherwise matching entries therefore show no ID. I reproduced this at 900 px in both themes and forced-colours emulation with two valid UUIDs whose key glyphs also match. Their screen-reader descriptions do contain distinct IDs. A sighted user can select the wrong key for a later operation.

   **Closing test:** Render both fallback-value collisions and identical raw metadata; assert visually matching rows display distinct IDs, including when their glyphs match.

2. **P2: An older retry can replace a newer key list.** `desktop/src/components/UseKeys.tsx:92` has no request identity or in-flight guard, and the retry button at `UseKeys.tsx:276` remains enabled without a loading indication. I reproduced two overlapping fake retries where the newer response showed `new-project`, then the older response replaced it with `old-project`. A late failure can likewise restore an obsolete error after success. This breaks the project's stale-result contract and can hide a newly available export scope.

   **Closing test:** Hold two reads, complete the newer one first, then resolve or reject the older one. Assert the newer options and error state survive. Also cover a retry overlapping the post-import refresh, loading indication and unmount.

3. **P3: IDs appear when rows differ in other shown facts.** `desktop/src/components/KeysView.tsx:158` omits displayed scope counts and renewal/status information from its comparison. A fake-data component probe showed IDs on two otherwise matching rows even though one visibly said `1 scope` and the other did not. That does not follow the owner's "only when" decision.

   **Closing test:** Vary each displayed fact independently and assert IDs appear only for rows that still match in every shown fact.

4. **P3: The permanent-delete regression guard does not cover the complete closing case.** `desktop/src/components/RecycleBin.facts.test.tsx:61` uses different issuers and checks confirmation text through DOM text content. It does not permanently guard the case where *all* ordinary metadata, including issuer, matches, or assert the confirmation's target facts in the accessibility tree. My temporary browser probe checked same-issuer rows and their action descriptions, but it is not a permanent regression test.

   **Closing test:** Use same-issuer entries with only their IDs different; check both row actions and each confirmation visually and through accessibility queries, then confirm the intended ID is sent.

The six review-3 findings were checked as follows:

- **LOOK-R3-01:** Recycle Bin rows expose issuer and ID, and the selected-ID component guard passed. The Keys-row collision and permanent-guard gap remain as findings above.
- **LOOK-R3-02:** The pending-read guard passed on the reviewed source and failed against the prior component. I did not independently cover every requested unmount and repeated-status interleaving.
- **LOOK-R3-03:** Empty and all-unfiled cases passed component checks and 900 px headless checks in both themes.
- **LOOK-R3-04:** The selected shelf and confirmation checks passed at 900 px in both themes. A synthetic before/after CSS probe showed the old oversized shelf item and confirmation overflow. The capped label was keyboard-scrollable in the probe.
- **LOOK-R3-05:** The basic retry guard passed and failed against the prior component; the overlapping-read defect remains above.
- **LOOK-R3-06:** The permanent-delete confirmation passed the rail hit tests in both themes. This was a coverage addition, so removing the new test would not itself make the previously working UI fail; I did not run a stacking mutant.

I also checked the earlier P2 cases **UI-R2-01**, **LOOK-R2-01**, **LOOK-R2-02**, and the affected **LOOK-R1-01** and **LOOK-R1-02** paths. The select's wheel test passed; its refresh behavior was inspected, but I did not independently rerun every earlier fail-first or mutant check.

**Work unit complete: NO.** Source review fails. No build was supplied, and **no build hash was checked**. Native GUI, installed build, real Windows forced colours, real screen-reader behavior and owner-gated runtime proof remain unrun. The implementer's full-suite and build results remain reported evidence. The checkout was clean after the review.
