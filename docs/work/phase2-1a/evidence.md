# Unit 1a verification record

## Failing-first checks

- `cargo test -p akm-desktop --lib --offline --locked warning`: 3 new tests failed on the original implementation. Undrained and locking-overtaken warnings had no ownership/classification; app-global warnings could not drain while locked.
- `npx vitest run src/Warnings.security.test.tsx src/components/Activity.security.test.tsx`: 8 new checks failed on the original implementation. Missing operation-notice preservation and absent/empty/corrupt journal overclaims were visible through real components/API wrapper.
- `cargo test -p akm-core --test audit_states --offline --locked`: missing journal incorrectly verified, so the new assertion failed.
- `cargo test -p akm-cli --test security --offline --locked missing_audit_is_not_reported_as_verified_or_proof_of_inactivity`: actual CLI returned success for missing-log verification, so the new exit-status assertion failed. An earlier harness attempt assumed a log already existed; that fixture error was corrected before counting this behavior reproduction.

These are recorded main-session results, not independent reviewer reruns. The checks below describe the rejected round-one candidate, not forthcoming corrections.

## Stable-candidate checks so far

- Frontend: `npm test` passed all 34 tests; production TypeScript/Vite build passed.
- Browser: `npm run test:ui` passed 8 scenarios, including journal request/completed/legacy states at 390/1440 pixels in both themes. Native IPC is mocked in these tests.
- Rust: the stable workspace run reported 144 passing tests, zero failed and two separately gated ignored tests. The PowerShell `2>&1 | Tee-Object` capture emitted a `NativeCommandError` around normal compiler stderr and a shell failure indication; preserve that capture caveat until a subprocess with an explicit recorded exit code confirms the run.
- Strict workspace Clippy passed after the CLI's premature audit return was corrected. Its new row-display regression first failed on that defect. The earlier 140-test run before this correction was not a clean final-candidate result.
- Tauri reported a completed GUI/MSI/NSIS build after the current application changes. Its PowerShell capture similarly formatted normal stderr as an error record. No installer was installed. Exact hashes and focused native proof remain pending.
- Graph layout reports no FAIL lines in either theme after equal-width file labels. The original measurement wrapper incorrectly treated the literal phrase `no FAIL line` as a failure; this wrapper issue is not a layout pass failure.

## Round-one final evidence and rejection

- `workspace-tests-clean.log` / `workspace-exit.json` record 144 Rust passes and explicit exit code 0. Frontend34/browser8/Clippy/rustfmt passed on that rejected source.
- Release CLI SHA-256 `ebe1f8d98a758780cb767d4f7650ac4f93f05f3b6733f37ba528f469beb9b0e0`; real KeePassXC interchange passed on it.
- `native-journal-proof.log` / `.json` record the passed focused GUI proof on SHA-256 `49a9fc9e72508107c8b2c7326b28bec8d32f627385b15c4ab2ef5da1b6affc82`: damage/missing/consistent states, unchanged damaged bytes, post-commit acknowledgment across B, CLI saved-write confirmation and exit. Clipboard was skipped entirely. Settings restored byte-for-byte; no AKM/matching WebView processes remained at cleanup.
- `gui-build-exit.json` records clean GUI/bundle build exit0 and later GUI hash `223b033bb09de3272e3fd9ea7159ecd36b1bb0215660629109468e5ec328531a`. Python failed only afterward printing a checkmark to a cp1252 console. Do not repeat that build for this wrapper error. Exact-artifact proofs must be renewed after source corrections. `-JournalExportOnly` exists but has not run.
- With CORS loading fixed, Activity layout has five unequal `.audit-head` width findings per theme (768–1440px). These are untouched, unequal data columns, not a checker pass. Graph and theme-switch/reload checks pass.
- Fresh reviewer `ses_e6f5c4454ffepv9w2kH46BB2x0`, caller-requested `openai/gpt-6.1-terra#max`, rejected the candidate with seven supported findings. Raw report/probes/original identities are under `reviews/round1/`, 10 files plus their hash manifest. No review is pending.
- Resume fingerprint check matched all application files to the rejected manifest; only expected state/proof-script edits differ. All 10 archived records verified. No correction has been implemented yet. See PLAN.md and the review ledger for correction scope and next checks.
