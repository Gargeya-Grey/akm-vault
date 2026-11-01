# Read-only acceptance review

Historical verdict. `followup-review-result.md` now owns the merged current verdict after the bounded P3 correction. Original findings and red probe evidence are retained below.

## Verdicts

**Code correction is not fully approved.** One supported P3 frontend async-state defect remains. No additional P0-P2 security or data-loss blocker was established. The earlier committed-outcome, receipt-correlation, export-ownership, opening-error, bootstrap-retry and import-hide defects appear corrected in this candidate.

**Phase-1 verification remains incomplete.** The exact rebuilt expanded GUI proof was interrupted by the host just after fake-vault creation. Its unfinished checks and settings recovery remain with the main session. Completed workspace, frontend, release, KeePassXC and standalone real-clipboard checks need not be repeated merely because the host restarted.

## Ranked supported finding

1. **P3, low. An obsolete lock acknowledgment can erase a newer session's error.** `desktop/src/App.tsx:114-121`, especially the unconditional `setError(null)` at line 118. The lock callback invalidates the initial view but does not fence its post-await success/error writes.

   Scenario reproduced through the actual App and API wrapper with deferred native IPC. Start session A, click Lock now, and let the native lock/event take effect while holding its IPC acknowledgment. Unlock session B. Make B's Reload vault fail and display its error. Deliver A's successful lock acknowledgment. B stays unlocked, but its newer reload error disappears. The same unfenced catch can write an obsolete lock-confirmation error into a newer view.

   Concrete evidence is `receipt-and-errors.test.tsx:128-143` and `async-results.json`. The assertion first observes B's error, resolves the older lock promise, then fails because that error is gone. Five other focused App/API-wrapper assertions passed. This is a deterministic asynchronous-interleaving reproduction, not a claim that an actual Windows lock was performed. No secret disclosure, native false-lock cycle or false committed-write failure was observed here.

   Fix both post-await branches with operation/session ownership, including the transition when a new session opens. Do not clear an unrelated operation's error. Retain a lock-confirmation safety warning when it is still relevant. Add this reproduction as a permanent regression before declaring every frontend async path fenced.

## High-risk checks

- Current-source native probes confirmed that create, add, update, remove, scoped import and explicit upgrade remain committed on disk and return a secret-free `completedWhileLocked` receipt after lock. Obsolete reveal results return `locked` without plaintext. Session and command postchecks preserve the distinction. The wrong-password opening probe confirmed no extra lock event that could destroy its terminal explanation.
- Actual App/API-wrapper probes passed for both an older native receipt and an older successful export reply after B opens. Both acknowledge completion without hiding B, and another genuine lock/unlock remains usable. A current-generation receipt hides the unavailable session and permits a subsequent unlock. Obsolete reload errors and warnings are rejected. The lock callback above is the remaining supported exception.
- Import scope labels and tokens share an immutable input snapshot. Request generations invalidate edited, cancelled, hidden and unmounted reviews. Hiding an already-dispatched confirmation does not discard its acknowledgment. Source inspection and the supplied prior focused evidence support these corrections. Bootstrap interruption now offers retry, and terminal authentication/create failures no longer emit their own destructive lock transition.
- Desktop and CLI export probes passed renamed-temp/replaced-name interleavings. Cancellation removes the owned plaintext rather than the replacement. Authorized publication moves the owned bytes and preserves unrelated data. An authorized session-A publication can finish after B opens, return its completed receipt and leave native B active. CLI refuses an existing destination. Source checks cover pre-plaintext handle DACL verification, persistent-ACL/local-path restrictions, protected desktop destinations and the intended desktop-overwrite versus CLI-no-overwrite distinction. Preparation and bulk I/O are outside the security gate. Prepublication authorization remains the cancellation boundary.
- Clipboard source installs exclusions and the full Unicode/ANSI/OEM/locale frame before secret publication, captures ownership while open, and keeps the owner alive through CloseClipboard. Movable allocations transfer only on successful SetClipboardData. Partial publication retains cleanup ownership when necessary. No additional concrete defect was established. The repaired standalone real-Windows clipboard check is recorded as **passed**, including sequence validity after close/owner destruction, flags, content and cleanup. I did not access the actual clipboard.
- No additional supported regression was found in decrypted read-back comparison, attachment/history preservation, native UUID targeting, explicit upgrade settings, retained backups, short write locks, journal append serialization or CLI child-environment/batch handling. This is bounded source/probe evidence, not an integration audit.

## Evidence and remaining gates

`native-results.json` records successful compilation of the current core/helper sources against existing dependency artifacts and seven passing assertion groups from `native.rs`. Every fake vault/export is inside this scratch directory. `async-results.json` records five passing checks and the one failing finding above. No full suite or application build was duplicated. No GUI, actual clipboard, AppData settings, elevation, window station, subagent, git operation or external publication was used.

Main-session evidence, not independently rerun here, records 132 workspace tests, strict Clippy/format, 24 frontend tests, four Edge scenarios, release GUI/MSI/NSIS and CLI rebuilds, KeePassXC interchange and the repaired real-clipboard pass. The supplied recovery update confirms those artifacts and sources are unchanged.

Still required is completion and recording of the interrupted exact-release GUI proof, including concurrent CLI/stale-write/reload, rejected/scoped imports, clone isolation, native save cancellation and exact-destination/content verification, explicit upgrade confirmation, clipboard cleanup through the rebuilt GUI, display affinity and normal exit. Earlier partial/historical passes do not establish a completed current run. The main session owns safe process/settings recovery and final artifact hashes. The final PHASE1 and verification scripts were reread; PHASE1 still records the expanded proof as in progress.

Actual OS lock/suspend notification delivery remains unverified. Synthetic messages exercise handlers only. Idle expiry has fake-clock coverage, not a ten-minute wall-clock check. Installer installation, signing and an external security audit are not claimed or required retroactively by this review. No above-90 readiness claim is justified. The Prove It Works principle kept these real-artifact gates separate from source/probe success.

## Source integrity

All 132 pinned paths matched at review start. Final `hash-check.json` finds 131 still matching and no unlisted application files. Application source, manifests and lockfiles remain unchanged. Only the anticipated `tools/verify_windows_app.ps1` popup automation changed. Its pinned SHA-256 is `dc62b6ee7eacd63880744e3648c65a852241f0f911932709683cd8500e431f30`; its final reviewed SHA-256 is `4d387a0469523e3c40d1950694c5b2a32228d2c88ef79ee5bdc76d8a17900cc3`.
