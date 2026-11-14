# UI rebuild review 1 (key cabinet and setup program)

- Model: Codex GPT-6.1 Terra, effort max.
- Reviewed commit: `49a54e8bc489650e52c38a973d405ff0a4b04a9b` on `ui/key-cabinet`, against `723914f`.
- Build hashes: the reviewer did not check a build. The builds of this branch came from the tree before the last CSS cleanup.
- Date: 2026-11-08.
- Result under the pass rule: **fail** (1 P2, 4 P3).

The text below is the reviewer's, unchanged, except that absolute file links are shortened to repository paths.

---

1. **Ranked findings**

   - **P2 — Activity hides record details at supported window widths.** `desktop/src/styles.css:473` sets `.audit-detail` to `display: none` at widths of 1040 CSS pixels or less. At the configured minimum width of 900, Activity therefore omits information such as how many keys an import added or updated and how many a file export contained. The previous layout retained those details. Screen readers lose them too. **Found by inspection.**  
     **Closing test:** Render representative import and export records at 900 and 1040 pixels, plus a wider control, and assert their details are visible and accessible.

   - **P3 — The new password indicator counts UTF-16 units instead of the backend’s Unicode characters.** `desktop/src/components/Unlock.tsx:108` passes `password.length` to the pin indicator. Six supplementary-plane characters produce “12 of 12 pins set” and the open-lock graphic, although the native creation rule counts six characters and rejects the password. A small Node probe confirmed the count difference; the UI consequence is **found by inspection**. The frontend’s length validation already used UTF-16 units before this diff; the misleading indicator is new.  
     **Closing test:** Drive the create form with six and then twelve supplementary-plane characters. The indicator and validation must agree with the native Unicode-character rule.

   - **P3 — The key list can omit the project of every displayed row.** `desktop/src/components/KeysView.tsx:140` hides the group heading when filtering leaves one project, while the redesigned rows no longer show a project column. For example, a search across several projects that matches keys in only one leaves the heading “All keys” without identifying that project. The user must open a key or select a shelf to determine its project. **Found by inspection.**  
     **Closing test:** Filter a multi-project fixture down to one project and assert its name remains visible and accessible with the results.

   - **P3 — Settings can briefly show an old key count after a completed mutation.** `desktop/src/components/Settings.tsx:99` now uses the last polled `status.keyCount`; it no longer fetches the key list when Settings opens. Immediately after adding, removing, restoring or importing a key, Settings can show the previous count until a later status poll completes. **Found by inspection.**  
     **Closing test:** Complete a fake mutation, delay the next status response, navigate to Settings and assert that the displayed count reflects the completed operation.

   - **P3 — Two current instructions describe old routes or behavior.** The export verification recipe (`.agents/skills/verify-akm/features/export-and-run.md:16`) still directs the reader to Settings for desktop export. `docs/STATE.md:54` says the proof script still needs its import steps changed, although the script now calls `Confirm-NativeImport`; that adapted path remains unproved on the real GUI. **Found by inspection.**  
     **Closing test:** Check the feature recipes and current-state instructions against the controls and script, and preserve the distinction between an adapted script and an owner-run proof.

2. **Source acceptance: NO.** The Activity regression is P2 and fails the pass rule. I checked the changed frontend touchpoints for **1A-R5-03**, **1A-R8-02** and **P2R2-02**. The moved import request and expiry logic and the clipboard wording remain present by inspection, but this review does not independently accept their earlier native fixes. **R4-04-SCRIPT**, the existing open P1 proof-design finding, remains outside the claimed fix scope.

3. **Work unit complete: NO.** In addition to the failed source review, the reported GUI and NSIS proofs are for the tree before the final CSS cleanup, not the reviewed `49a54e8` commit. The affected native flows on the final build and the owner-present tests remain unproved.

4. **What the review did not cover.** I did not launch the GUI, access the clipboard or app settings, change windows or the OS session, install anything, or run full builds or suites. I did not independently verify the reported earlier-tree test results, installer hash or actual installer behavior, and did not exhaustively review unchanged native callers. `python -B tools/check_docs.py` passed, as did `git diff --check`; the documentation check does not detect the stale behavior descriptions above. No project files were edited, and no task files were created or deleted. Space freed: **0 bytes**.
