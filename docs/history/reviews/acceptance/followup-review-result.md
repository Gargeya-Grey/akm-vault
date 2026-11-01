# Merged acceptance verdict after the bounded P3 correction

**Code-correction verdict is approved within the reviewed phase-1 scope.** The sole remaining P3 is fixed. No supported regression was found in this small change. This supersedes the earlier code-correction verdict in `review-result.md`; its wider review and completed native evidence remain applicable, subject to their stated limits.

## P3 resolution

`desktop/src/App.tsx:114-124` captures both epoch and transition after `showLocked()`. Successful lock acknowledgments no longer modify the error state. Failed acknowledgments write the safety warning only while both captured identities remain current.

The new-session callback increments transition at `desktop/src/App.tsx:213-219`. This matters even if no additional lock event changes the epoch. An older failure therefore cannot overwrite B's error after B opens. A still-current failure retains the native-lock safety warning. The API wrapper remains unchanged and intentionally exempts `vault_lock` from its generic successful-response generation check, so this ownership check belongs in App.

The permanent regression at `desktop/src/SessionReceipt.security.test.tsx:53-87` drives the actual App/API wrapper. Both old-success and old-failure cases establish B's visible reload error before delivering A's delayed lock reply, then assert that B's error and unlocked native/UI state survive.

I ran only those two new tests. Both passed. The older receipt test was filtered out. `followup-async-results.json` records two passes and zero failures. The reported pre-fix red runs are main-session evidence; my original successful-acknowledgment reproduction remains preserved in `async-results.json`. No earlier seven-group native probe, full suite, application build or wider review was repeated.

## Verification verdict remains open

The expanded exact-current-GUI proof is unfinished. Main reports that its recovered automation cannot enumerate the initial Vault file/Try again controls and is diagnosing that test surface. This is an unresolved verification gate, not enough evidence for a new product defect. Main owns the GUI diagnosis, safe process/settings recovery and final exact-artifact proof.

Completed Rust, KeePassXC and repaired standalone real-Windows clipboard proofs remain valid for their unchanged code. They were not rerun here. The previously disclosed actual OS lock/suspend delivery and wall-clock idle coverage gaps remain. Synthetic notifications do not establish real OS delivery. No above-90 readiness or audited integration claim is made.

## Scope and integrity

Only current App, the permanent receipt regression and the directly relevant API wrapper were inspected in this follow-up. `followup-scoped-hashes.json` records the expected App/test changes and unchanged API wrapper. Current App SHA-256 is `10acbc8b5083ee1814d71da4a801af7d2762966325a72599c3b1fff3710ec8b0`.

No project source edits, GUI, actual clipboard, AppData access, subagents or external actions were used. New configuration, results and this report are confined to the existing acceptance-review scratch directory.
