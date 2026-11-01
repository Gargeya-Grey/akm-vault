# Unit 1a: owned warnings and honest journal reporting

Authorized by the owner on 2026-10-31. Tier 2 because journal state and operation outcomes cross core, CLI, native IPC and React. Goal: never attribute A's warning to B, or present requests/missing records as verified completed history.

## Ownership and sequence

The main session owns all implementation and tests. Shared commands/DTOs make parallel implementation counterproductive. Implement in sequence: warning ownership, atomic journal reporting, then caller presentation and real-surface proof. One new read-only GPT-6.1 Terra reviewer with caller-requested `max` checks the stable combined change. Host-default Sol remains the implementer. The owner's explicit Terra Max authorization overrides the old global no-max default only for this review; no global routing file changes.

No storage migration, vault identity mapping, tail repair, new cryptography, rotation/history, time-refresh fix, visual redesign, installation, git or publication. The journal remains beside the vault and its privacy limitation stays explicit.

## Frozen contracts

### Warning channel

- Keep backend policy/clipboard failures as app-global warnings, distinct from operation notices.
- Core post-commit journal warnings become typed notices carrying a unique notice ID, opaque originating native session ticket and `journal` kind. Preference-save failures use a separate `preference` kind. Do not expose raw vault paths, titles, scope or error details in these cross-session notices.
- `vault_warnings` returns `{ application: string[], operations: { id: string, originSession: string, kind: "journal" | "exportJournal" | "preference" }[] }`. `exportJournal` distinguishes a published environment file from a committed vault change. It can drain while locked because operation payloads are fixed secret-free classifications and application warnings are not vault-specific.
- App renders operation notices in an explicitly application-wide area, never as warnings about the currently open vault. They survive lock/open until dismissed, cannot false-lock B, and are retained if a warning response arrives after its frontend generation is obsolete. Polling is not activity.
- Status and all existing mutation/result contracts remain unchanged. `completedWhileLocked` still acknowledges a committed result without returning protected data. Do not merely clear an old warning queue and lose A's acknowledgment.

### Journal snapshot

- Add `AuditLog::snapshot(limit)` returning one read/verification snapshot under one shared lock. States are `absent`, `empty`, `consistent`, `corrupt`. Completeness/authorship remain unknown for every state.
- Return the displayed parsed prefix/retained entries, total valid entry count, first problem's physical line and offending sequence where available. Invalid JSON is reported structurally, not echoed as raw data or assigned a cause. A broken chain does not become a trusted prefix. No bytes are repaired, appended or deleted by a read.
- Read the file only after acquiring the shared lock. Limit affects displayed entries, not validation of the full snapshot. Existing strict `read` can remain a convenience wrapper; `verify` must not report an absent/empty log as verified.
- Native AuditReport and CLI audit use this same snapshot, not separate verify/read calls. UI/CLI say records are unauthenticated and completeness is unknown. Missing/error states never say that nothing happened.

### Event outcomes

- New pre-disclosure events use explicit `*-requested` action names. They prove only a request was recorded, not disclosure/copy/export success. Preserve fail-closed logging before disclosure/export/launch.
- New post-commit mutation/file-publication records use explicit `*-completed` action names. Their logging failure remains a warning after success, never a retryable save failure. Existing legacy action names have unknown outcomes in presentation; do not retroactively certify old events.
- No promise that every action has a record, that empty/missing history proves inactivity, or that an unanchored hash chain proves completeness.

## Acceptance and checks

- Native and actual App/API regressions for A's undrained post-commit warning after B opens; locking-overtaken commit; delayed warning reply; application warnings retained; no A-specific metadata or false lock of B.
- Core tests for missing/empty/deleted logs, malformed final/middle JSON, broken links/hash, display limits with full validation, byte preservation, and concurrent append/read snapshots.
- CLI tests for absent/corrupt/consistent audit output, unknown completeness, requested versus completed action names and unchanged pre-disclosure failure refusal/post-commit success warnings.
- Activity tests for each state, stale refresh/error handling, honest action labels, no missing-log verification or inactivity claim. Existing secret/session/import/export regressions must remain passing.
- Affected checks while editing: `cargo test -p akm-desktop --lib --offline --locked warning`; `cargo test -p akm-core --offline --locked audit`; targeted CLI security tests; targeted Vitest files.
- Stable candidate: `cargo test --workspace --offline --locked`; strict workspace Clippy; rustfmt check; `npm test`, `npm run build`, `npm run test:ui` in desktop; release CLI and Tauri GUI/bundles.
- Rebuilt real-surface proof: focused native journal/warning flow on temporary fake vaults with clipboard tests skipped; exact executable hash and owned settings recovery recorded. Run independent KeePassXC when relevant persistence callers change, without claiming a new library audit.
- Fresh reviewer receives this plan, combined before/after diff, pinned sources and verification results, not implementer reasoning. Record raw verdict and supported dispositions. No numerical rescore is implied.
