# Current state and resume point

Updated 2026-10-31. This is the short operational checkpoint. Requirements live in [PROJECT.md](PROJECT.md); findings live in [REVIEW_LEDGER.md](REVIEW_LEDGER.md).

## Current task

Implement phase-two unit 1a, authorized by the owner's 2026-10-31 "Yes, please." Scope is session-owned warnings and honest journal state/outcome reporting. See the [frozen unit plan](work/phase2-1a/PLAN.md). Private storage migration, rotation/history and stale time-derived status are not included.

Context preservation and the fresh calibration review are complete. Forty-four original historical records, the new raw review, twelve frozen review-input snapshots and two recovered earlier GUI logs are saved locally with hashes. The caller requested `openai/gpt-6.1-terra#max`; the reviewer confirmed Terra but could not independently observe active effort. See the [raw verdict and disposition](reviews/2026-10-31-calibration/disposition.md).

The reviewer conditionally supports bounded engineering on isolated fake data. The owner has now authorized the first implementation unit. The main session owns the coupled core/CLI/native/frontend change in sequence; one new Terra review follows a stable candidate. This is not a whole-product approval or new numerical score.

## Last completed engineering checkpoint

- Phase-one source corrections are approved within their reviewed scope. The final P3 lock-reply feedback defect is fixed with two failing-first permanent regressions and independent follow-up verification.
- The GUI and MSI/NSIS installers were rebuilt after that frontend-only correction. The remaining exact-GUI native proof passed.
- Preserved results are 132 Rust workspace tests, strict Clippy/rustfmt, a complete 24-test frontend run before the final P3, 4 browser scenarios, real KeePassXC interchange and the repaired standalone real-Windows clipboard test.
- After the final P3, the affected App/receipt run passed 10 tests. Do not report this as a fresh full 26-test frontend run. The reviewer independently ran the two new regressions.
- The final GUI passed native save cancellation, exact filename read-back, exact destination and scoped content, clipboard exclusions and cleanup, synthetic session/power handling, normal exit and capture affinity `0x11`.
- Completed import/clone/upgrade probes were reported from a preceding GUI run. Its recovered raw output shows those passes followed by a filter-automation failure. The preceding hash is recorded only in conversation history and is not embedded in the recovered logs; exact binary binding is unavailable. `-RemainingOnly` did not rerun those flows on the final GUI.
- Startup automation succeeded on the resumed run on `WinSta0` / `Default`. The preceding missing-controls failure did not recur, and its cause remains unproven.
- At cleanup verification, no AKM or matching WebView processes remained. This is a recorded observation, not a promise about processes in a later session.

The exact artifact hashes and source manifest are preserved under [history/phase1-final](history/phase1-final/). See [VERIFICATION.md](VERIFICATION.md) for coverage and invalidation rules.

## Readiness is not established

- The last full numerical review is the historical **40.5/100, rounded to 41** baseline. There is no post-phase-one full rubric rescore. Do not assert that the current source was independently scored either 41 or above 90.
- Scoped correction acceptance is not the original three-perspective full-rubric review, and neither is a professional security audit.
- Real OS lock/suspend delivery is unverified. Synthetic messages prove handlers only. Wall-clock idle expiry is unverified; fake-clock policy tests passed.
- Installers are unsigned, built but not installed. CI is configured but has not run remotely.
- Real secrets, publication, git setup, installation and signing enrollment are not authorized by the context-preservation request.

## Important retained runtime state

An earlier server restart lost the interrupted proof's original in-memory preference snapshot. The durable recovery baseline still points to this known fake vault:

`C:\Users\Gargeya\AppData\Local\Temp\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx`

The final proof restored that baseline byte-for-byte. The user's pre-restart preference is unknown. Do not guess it, delete that fake-vault directory while it remains selected, or overwrite settings on the strength of a stale PID. New native proof runs persist `recovery.json` and prior settings bytes before launch.

## Open phase-two work

1. Private per-device journal, honest integrity claims and malformed-tail handling. The current log still exposes metadata beside a synced vault.
2. Secret-change rotation timestamps, native history and recoverable deletion/tombstone semantics.
3. External dotenv compatibility, date/rounding consistency and remaining CLI selection/memory issues.
4. Reminder privacy and delivery, remaining accessibility and documented layout debt.
5. Expanded independent-reader/CI evidence and a fresh full-rubric assessment. Higher numerical scores are not guaranteed by completing this list.

Several original phase-two IDs were already addressed during phase one. Use the ledger rather than rebuilding the old roadmap blindly.

The fresh review added three inspection-supported open P2 defects. Unit 1a addresses native warnings crossing sessions and journal UI/CLI overclaims. Stale expiry/rotation presentation remains for unit 3. All remain open until the appropriate implementation evidence and fresh acceptance are recorded.

## Context preservation checks

The [completion record](reviews/2026-10-31-calibration/completion.json) records final local-link, archive/hash, frozen-input and unchanged-application-source checks. The raw review and original 44-record archive remain immutable. This task changed project guidance, planning and evidence records only. No application tests/builds, GUI, clipboard, AppData, installation or external publication were run. The original reviewed documents are retained separately from these main-session dispositions, which were not independently re-reviewed.

## Next action

Pin the unit baseline, publish its graph and add failing-first warning/journal regressions using the frozen contracts. Implement serially and run affected checks before the stable full candidate. Update this checkpoint with exact results before handoff. Do not treat unit 1a authorization as permission for real secrets, storage migration, publication or disruptive OS tests.

## Checkpoint format for the next work unit

Replace the current-task and next-action sections when they change. Record the date, exclusive file ownership, changed paths, exact verification commands/results, reviewed source/artifact identity, unresolved findings, temporary resources still owned, decisions requiring the owner and one executable next step. Keep old evidence in history rather than growing this file into a transcript.
