# Round-one corrections: evidence, not acceptance

Main-session serial implementation; no implementation delegates. Failed review remains under `../reviews/round1/`.

## Red checks on the rejected source

- `red-core.log`: two failures in `cargo test -p akm-core --test audit_states --offline --locked` reproduced unsafe no-newline append and raw malformed-value error echo (four older state tests passed).
- `red-native.log`: `cargo test -p akm-desktop --lib --offline --locked warning` reproduced backup-only misclassification and destructive repeated polling (three older tests passed).
- `red-native-append.log`: actual requested-key helper returned protected text after unsafe append.
- `red-cli.log`: actual CLI disclosed fake text after an unterminated valid record.
- `red-cli-actions.log`: actual CLI emitted `export-start` instead of `export-requested`.
- Frontend targeted run: 6 failures / 9 passes reproduced delayed between-mount loss, missing distinct backup text, categorical journal failure wording and omitted known sequence. Tests are permanent in Warnings/Activity security files. The result was observed directly; no separate red frontend stdout file was saved.

## Corrected implementation and targeted checks

- Core warnings now have `Journal`, `BackupRefresh`, `BackupPrune` records; string drain remains an adapter. Native maps kinds without raw details. Session test covers each alone and all together, ordinary completion and lock-overtaken commit, then opening B. Pruning failure is induced with test-owned backup-shaped directories, not altered ACLs on user data.
- Native operation snapshots retain IDs until `vault_dismiss_warnings` acknowledgment. Frontend in-memory safe replay covers between-root delivery; acknowledged-ID tombstones reject stale polls without erasing unseen new IDs. Dismissal failure retains notices. Native polling/ID and actual App/API lifecycle tests pass.
- Append validates one full snapshot under its exclusive lock; unterminated record, malformed JSON and broken chain refuse with structural, sanitized errors, without repair. Core, CLI disclosure/export/launch and native helpers preserve bytes and withhold protected values. Targeted `green-core-states`, `green-cli` and `green-native-append` logs pass.
- A test-only core hook fails after complete journal write/before sync in an actual mutation. Native `publish_and_record` uses an injectable recorder; the test writes real-encoder completion bytes then injects failure before syncing that handle. Both preserve the saved output and uncertain-warning classification. Native publication additionally races lock and opening B, retaining `completedWhileLocked` and A-origin secret-free notice. These are deterministic injected faults, not an observed failing Windows disk sync.
- CLI emits new `run-requested` / `export-requested`; Activity preserves unknown legacy-start outcomes. CLI/Activity show the known offending sequence alongside line, with no invented sequence for malformed JSON.
- Targeted logs `green-*.log` passed. Affected App/API/Activity suite passed 28 tests before the extra legacy-label case. Full frontend now passes 43 tests; production build passes; browser10 passes, including real browser document reload against retained mocked-native notices in both themes. This is not a claim of an actual Tauri webview reload.

## Stable corrected candidate

- Full workspace: **153 passing tests**, zero failed, two separately gated ignored tests, actual exit0 (`workspace.log`, `rust-checks.json`). Clippy initially found one owned-string comparison in the new test assertion; a test-only comparison correction was made. Strict Clippy and rustfmt then passed (`lint-checks.json`). This did not alter production code; the full workspace was not redundantly rerun for it.
- Full frontend: **43 tests**, TypeScript/Vite and **10 Edge browser scenarios**, all exit0 (`frontend-checks.json`). Browser reload proves a fresh frontend document with a retained mocked-native store, not actual WebView2 reload.
- Release CLI and Tauri GUI/MSI/NSIS builds exit0 (`build-exits.json`). Nothing installed. Final artifacts in `artifacts-sha256.json`: GUI `c5ed1a150d97eca63687a9a2d923f522d7245bb9580124b87e82e2663c128aca`; CLI `382422d789ded2216aa8f3d344b8be72c27bd76a88f77fff88b76c9d2a5983e7`.
- Exact-GUI `-JournalOnly` proof exit0: consistent/damaged/missing snapshots, preserved damaged bytes, saved-write confirmation by CLI, A notice across opening B, registered ID acknowledgment, subsequent polls do not resurrect dismissed notices, normal exit. `native-proofs.json` records settings restored byte-for-byte; clipboard skipped entirely.
- The first `-JournalExportOnly` attempt stopped at startup control discovery (exit1); its log, diagnostics and recovery identity are preserved. Saved full-tree diagnostics showed normal unlock controls. Harness now reacquires the owned root and rechecks the expected control before invoking an app retry. The export-only retry on the **same GUI hash** passed after that UI Automation root refresh: exact approved destination, selected fake content, requested/completed ordering, unchanged encrypted vault, pre-disclosure journal refusal/no plaintext file and normal exit (`native-export-retry.json`). Settings restored byte-for-byte. This is an observed UI Automation discovery failure, not a proven application startup defect or proof of its underlying cause.
- Real KeePassXC2.8.0 interchange passed on the final CLI (`keepassxc.log`, `.json`), preserving explicit upgrades, stable UUIDs, foreign entries, attachments and existing history.
- Layout checks ran in both themes/eight widths: graph no findings; Activity retains five unequal data-column findings/theme. The new notice+Keys fixture adds existing Keys findings: unequal stat/filter/header widths, empty header track, clipped screen-reader label (21 finding lines/theme). No notice overflow/overlap finding was reported, and 390/1440 screenshots were inspected. These are reported findings, **not a layout-checker pass**. No visual redesign was made to suppress them.

Fresh scoped review remains pending. No actual clipboard, OS lock/suspend, wall-clock idle, installed lifecycle or real-secret test is included. Actual Tauri webview reload is not independently exercised. Failed review and failed native startup attempt remain evidence, not overwritten by later passes.
