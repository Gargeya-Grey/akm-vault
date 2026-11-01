# Owner-participating native outcome proof

Status: **not run**. Requires rebuilt artifact identity, completed safe checks and the owner's readiness confirmation. No earlier failed minimize observation is superseded by this plan.

## Before asking for readiness

- Finish stable source/type/browser/lint checks and CLI/GUI/bundle builds. Store the exact four artifact hashes and relevant build-source fingerprints. Renew applicable KeePassXC and native backup/journal/export evidence on these binaries; none implies actual hide coverage.
- Use `tools/verify_windows_app.ps1 -OutcomeOnly -OwnerReadyForVisibility` only **after** the owner confirms they can observe the fake-data test and briefly leave that AKM window alone. The switch is an execution guard, not a substitute for consent. The script refuses OutcomeOnly without it before loading UI Automation, creating scratch paths, launching AKM or reading settings.
- Ask the owner to close any other AKM instance themselves. Do not close unrelated instances, dismiss a screen saver, change the active desktop, enter credentials for Windows, or lock/suspend the computer. Interactive screen-control tools are not available in this session; do not claim a screenshot or an observed desktop. The existing local Windows integration harness supplies scoped native observations, with the owner watching.
- No clipboard calls: OutcomeOnly implies JournalOnly and SkipClipboard. All values are fixed fake test values, all vaults/exports under a new `Temp\opencode\akm-app-proof-<uuid>` directory. No production delay/debug capability is added.

## Identity and recovery

The harness refuses an existing `akm-desktop` process, saves `recovery.json` and original settings bytes **before launch**, and records executable SHA-256 plus process creation time. Prior selected fake baseline and unknown older preference remain as in STATE.md; do not guess a different baseline.

Its journal byte-range lock belongs only to the fake vault and delays actual IPC outcomes. Cleanup releases that lock in `finally`. Native Save/Cancel and UIA commands target only the verified owned process/controls. Visibility targets the `Tauri Window` handle, checks process ownership and actual `IsIconic`, restores that same window in `finally`, and never sends global keystrokes. Existing code inspection identifies what it requests, not why the two earlier minimize observations failed. Do not infer a cause from the old input-desktop label.

Normal exit is preferred. On failure retain diagnostics, exact log, fake files and recovery metadata; recover only the still-matching owned process identity. Settings restore is allowed only while their current selected value is still an owned fake path, or they already match the saved preimage. Verify restoration byte-for-byte and absence of owned AKM/WebView descendants. A recycled PID is not ownership.

## One bounded attempt

1. Open the new fake vault using the current reservation protocol. Dispatch a scoped fake import while holding its journal lock. Confirm encrypted bytes changed, then actually minimize/restore the owned window. Confirm the local dialog expired, navigate to Keys, release the lock, inspect one secret-free application outcome and verify exactly one imported key/scope with the CLI. Acknowledge explicitly.
2. Approve an exact new fake `.env` path in the real native dialog while its fake journal lock delays completion. Actually minimize/restore, navigate to Keys, release the lock, confirm a single retained completed outcome, exact destination/content and no replayed path/scope details outside Settings. Acknowledge explicitly.
3. Cancel a real save dialog; confirm the cancellation outcome states no file was saved by that request, not completion. Acknowledge explicitly.
4. Induce a pre-disclosure journal error using only the owned fake journal path and byte-range lock. Approve a distinct new fake destination, actually minimize/restore before the delayed error, navigate away, release the lock and confirm a retained error classification (not a claim that nothing was saved). Verify this test destination was not published; restore the fake journal bytes. Acknowledge explicitly and check subsequent native polls do not resurrect outcomes.
5. Exit normally and verify process/settings recovery. Record exactly which steps completed, not a blanket GUI/OS/installer pass.

If readiness is unavailable, stop before visibility work. If the single attempt fails, keep it as a proof gap and ask the owner before any retry or changed mechanism. A synthetic `document.hidden` signal, mocked IPC pass or successful ordinary export does not replace actual hide/navigation evidence. No OS lock/suspend, wall-clock idle or real-clipboard coverage is claimed.

Fresh scoped Terra acceptance follows the frozen combined/cycle source/artifact/evidence package. Missing native proof remains an explicit gate; no full-rubric score or real-secret readiness is inferred.
