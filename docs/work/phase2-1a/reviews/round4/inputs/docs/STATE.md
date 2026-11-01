# Current state and resume point

Updated 2026-10-31. This is the short operational checkpoint. Requirements live in [PROJECT.md](PROJECT.md); findings live in [REVIEW_LEDGER.md](REVIEW_LEDGER.md).

## Current task

Implement phase-two unit 1a, authorized by the owner's 2026-10-31 "Yes, please." Scope is session-owned warnings and honest journal state/outcome reporting. See the [frozen unit plan](work/phase2-1a/PLAN.md). Private storage migration, rotation/history and stale time-derived status are not included.

Context preservation and the fresh calibration review are complete. Forty-four original historical records, the new raw review, twelve frozen review-input snapshots and two recovered earlier GUI logs are saved locally with hashes. The caller requested `openai/gpt-6.1-terra#max`; the reviewer confirmed Terra but could not independently observe active effort. See the [raw verdict and disposition](reviews/2026-10-31-calibration/disposition.md).

After three rejected Terra rounds, the owner authorized one new bounded cycle: "Yes, please recalibrate yourself and then do it." The [recalibrated cycle plan](work/phase2-1a/corrections3/PLAN.md) freezes application-owned outcome records, lease-owned cleanup warning episodes and complete API reply fencing for 1A-R3-01/02/03. Main owns implementation serially, followed by one fresh read-only acceptance. No other phase-two unit, publication, installation or real-secret use is authorized. Earlier failed verdicts remain immutable and no new score exists.

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

The [calibration completion record](reviews/2026-10-31-calibration/completion.json) records that earlier unit's local-link, archive/hash, frozen-input and unchanged-source checks. Its raw review and original 44-record archive remain immutable. That calibration changed project guidance, planning and evidence only, with no application tests/builds, GUI, clipboard, AppData, installation or publication. Its reviewed documents are retained separately from subsequent dispositions and unit-1a implementation evidence.

## Next action

All three prior reviewers are finished. Immutable evidence lives under `docs/work/phase2-1a/reviews/round1`, `round2`, `round3`; do not overwrite failed verdicts with future approval. The owner-renewed cycle is now implementing the three remaining findings.

Current changed seams: `audit.rs` JSON whitespace/boundary checks; Session application-warning IDs; native audit serializers; frontend completion/app-warning retention, own-key action labels, delete copy and IPC types; permanent regressions and proof harness. Targeted checks pass: core8 (including 1,024 byte cases), affected CLI/native checks, frontend39 in five files. See [second-cycle evidence](work/phase2-1a/corrections2/evidence.md). The interrupted patch had applied no production edits; its failing-first tests were retained and not needlessly repeated.

The prior rejected candidate passed Rust157/frontend55/browser10 and strict lint. Its production build caught two unsupported test-only Testing Library options; corrected, affected delete/build/browser rerun. That GUI was `42f3f63afae5c6dfa426c92da7635972d5e0642c2d6e8482681588dfc14510c8`; CLI `0024f9005a68bf5b5ab9d32297c9d6eb4043ee5616a7ae6c465a054adfddcdc0`. This is historical evidence, not the renewed candidate below.

Extended JournalOnly passed on that GUI (delete/backup-warning copy, unknown action labels, exact large sequence, journal states/bytes/sessionB/dismissal). Its UI Automation discovery used the prior root-refresh recovery. JournalExportOnly passed on the same hash. Both restored settings byte-for-byte and skipped clipboard entirely. CLI KeePassXC passed. No owned AKM/WebView processes remained at cleanup. No build/proof is pending.

The rejected third candidate is pinned under `docs/work/phase2-1a/reviews/round3/`, 145 paths; source-manifest SHA-256 `28926546f3b1b604bbf99ea81086443bd060cf622c042f82d21685465222af44`. Reviewer `ses_e6edc74a6ffesMbDh6Y7lfBTJm` independently ran three mocked-native frontend probes, not full suites/native GUI. Seven raw report/probe records are archived with hashes. It verified all pinned application/build/test/proof inputs and four artifact hashes; only allowed `docs/STATE.md` differed. Caller requested Terra `#max`; active effort/token usage not exposed.

Remaining supported findings: **1A-R3-01/P2** dispatched import outcome lost after hide then navigation out of Settings (mocked successful confirmation, not a proven real commit; check dispatched export too); **1A-R3-02/P3** recovered then recurring identical safety failures are merged while the old ID remains pending (inspection-supported; no real clipboard operation); **1A-R3-03/P3** stale status successes and ordinary native errors bypass API generation fencing (old callers receive A metadata; no B-DOM display was reproduced). Full disposition is in the ledger.

Resume matched all146 frozen before-cycle paths and the rejected GUI/CLI hashes. [Current-cycle evidence](work/phase2-1a/corrections3/evidence.md) preserves33 failing-first frontend behaviors (1 pass) and6 fake native failures. The capture-helper print encoding failure occurred after its log/exit were saved, not instead of test execution.

Changed production: frontend API/sessionBoundary/App/Settings and native Session/minimal clipboard generation getter. Registry, separate live session-loss signal, success/error fences and shared lease-owned cleanup handler implemented. Full frontend105/browser18/typebuild/lint pass. Rust163 unique passes complete across a timed-out156-pass workspace attempt and exact same-workspace7-case selective resume, with the156 completed names skipped; binary/doc targets also pass. Supplementary package-only7 not counted again. Two gated ignored remain. Partial/failed logs retained; no reviewer dispatched.

New GUI/MSI/NSIS built exit0; GUI `c3b125872f26a609e63158f6ae72fb8cac9e666ca11ff83efb64aa19de0f0ec6`; CLI unchanged. OutcomeOnly **both attempts failed to observe actual minimization**, even after the same verified Tauri-handle IsIconic check. Both confirmed pending+fake committed import, but never reached handoff acknowledgment assertions: native hide/navigation remains unproven, not a product failure or pass. Settings restored exactly both times; failure evidence retained in `...\opencode\akm-app-proof-43505207-721c-4baa-a17f-2c081b425a1a` and `...94352c49-2ce4-4717-8801-bdd2aeb2158f`. Input desktop was Screen-saver; no inferred cause, unlock, desktop change or global input. No more minimize retries. Exact-GUI JournalOnly and JournalExportOnly passed (owned-root refresh used), including delete/backup warning, unknown actions, exact large sequence, journal bytes/states, A notice across B/dismissal, approved export path/content/request-completion order and refusal before publication on request-log error. Settings restored both runs. No build/proof is pending; clipboard untouched.

Next: freeze combined/cycle diffs and source identity and obtain the one fresh Terra review with the actual minimize/handoff proof gap explicit. Reviewer model `openai/gpt-6.1-terra#max` availability confirmed; none dispatched yet. Existing layout debt persists (Activity5/Keys21 per theme; no new outcome overflow/overlap reported). Do not repeat unchanged CLI/KeePassXC solely for context. Another rejected acceptance returns to owner, not another correction cycle.

## Checkpoint format for the next work unit

Replace the current-task and next-action sections when they change. Record the date, exclusive file ownership, changed paths, exact verification commands/results, reviewed source/artifact identity, unresolved findings, temporary resources still owned, decisions requiring the owner and one executable next step. Keep old evidence in history rather than growing this file into a transcript.
