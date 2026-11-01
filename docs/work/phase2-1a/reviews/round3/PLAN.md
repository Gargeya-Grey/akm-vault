# Unit 1a: owned warnings and honest journal reporting

Authorized by the owner on 2026-10-31. Tier 2 because journal state and operation outcomes cross core, CLI, native IPC and React. Goal: never attribute A's warning to B, or present requests/missing records as verified completed history.

## Ownership and sequence

The main session owns all implementation and tests. Shared commands/DTOs make parallel implementation counterproductive. Implement in sequence: warning ownership, atomic journal reporting, then caller presentation and real-surface proof. One new read-only GPT-6.1 Terra reviewer with caller-requested `max` checks the stable combined change. Host-default Sol remains the implementer. The owner's explicit Terra Max authorization overrides the old global no-max default only for this review; no global routing file changes.

No storage migration, vault identity mapping, tail repair, new cryptography, rotation/history, time-refresh fix, visual redesign, installation, git or publication. The journal remains beside the vault and its privacy limitation stays explicit.

## Frozen contracts

### Warning channel

- Keep backend policy/clipboard failures as app-global warnings, distinct from operation notices.
- Core warnings carry `Journal`, `BackupRefresh` or `BackupPrune` classification; keep the existing string drain API as an adapter if practical. Native notices retain a unique notice ID, opaque origin ticket and separate fixed `journal`, `backupRefresh`, `backupPrune`, `exportJournal` or `preference` kind. Do not expose raw paths, titles, scope or error details across sessions.
- `vault_warnings` returns `{ application: { id: string, message: string }[], operations: { id: string, originSession: string, kind: "journal" | "backupRefresh" | "backupPrune" | "exportJournal" | "preference" }[] }`. It is safe while locked. Both kinds are non-destructive snapshots retained natively until `vault_dismiss_warnings({ids: string[]})` explicitly acknowledges only displayed IDs. Polling, mounting, reload, locking and opening do not dismiss them. No crash-durable storage is promised.
- App renders operation notices in an explicitly application-wide area, never as warnings about the currently open vault. They survive lock/open until dismissed, cannot false-lock B, and are retained if a warning response arrives after its frontend generation is obsolete. Polling is not activity.
- Status and all existing mutation/result contracts remain unchanged. `completedWhileLocked` still acknowledges a committed result without returning protected data. Do not merely clear an old warning queue and lose A's acknowledgment.
- Frontend safe handoff/replay covers delayed replies between root mounts; native retention also covers a webview reload. Dismissal handles only observed IDs; delayed polls must not resurrect dismissed IDs or dismiss new unseen notices. Dismissal failure keeps the notices available and does not affect session B.
- Round-two contract extension: app-global warnings also have unique native IDs, `{id,message}`, are returned non-destructively and are dismissed by the same ID-only command. Keep their messages in a separate frontend cache/area from operation-origin notices. Old snapshots cannot acknowledge a newer recurrence's new ID. No vault-specific metadata enters this channel.
- Secret-free `completedWhileLocked` frontend receipts retain unique IDs and origin generations in module memory through React root handoff until explicit acknowledgment. Replay is presentation-only, not a new lock signal. Root teardown invalidates old frontend work. This is not crash-durable receipt storage or a claim of native disk failure reproduction.

### Journal snapshot

- Add `AuditLog::snapshot(limit)` returning one read/verification snapshot under one shared lock. States are `absent`, `empty`, `consistent`, `corrupt`. Completeness/authorship remain unknown for every state.
- Return the displayed parsed prefix/retained entries, total valid entry count, first problem's physical line and offending sequence where available. Invalid JSON is reported structurally, not echoed as raw data or assigned a cause. A broken chain does not become a trusted prefix. No bytes are repaired, appended or deleted by a read.
- Read the file only after acquiring the shared lock. Limit affects displayed entries, not validation of the full snapshot. Existing strict `read` can remain a convenience wrapper; `verify` must not report an absent/empty log as verified.
- Native AuditReport and CLI audit use this same snapshot, not separate verify/read calls. UI/CLI say records are unauthenticated and completeness is unknown. Missing/error states never say that nothing happened.
- A nonempty record without its final newline is an unsafe append boundary. Refuse before writing or disclosure, preserve all bytes and report structural damage without serde values/raw contents. No automatic newline insertion or repair. Present physical line and parsed offending sequence when available.
- Blank records may contain only JSON whitespace (space/tab/CR/LF); every nonempty final fragment must have its newline before append, even when whitespace-only. Non-JSON control whitespace is damage. IPC sequences (`entries[].seq` and `brokenAt`) are exact decimal strings, never lossy JavaScript numbers. Unknown action labels require an own-key lookup, not inherited object properties.

### Event outcomes

- New pre-disclosure events use explicit `*-requested` action names. They prove only a request was recorded, not disclosure/copy/export success. Preserve fail-closed logging before disclosure/export/launch.
- New post-commit mutation/file-publication records use explicit `*-completed` action names. Their logging failure remains a warning after success, never a retryable save failure. Existing legacy action names have unknown outcomes in presentation; do not retroactively certify old events.
- No promise that every action has a record, that empty/missing history proves inactivity, or that an unanchored hash chain proves completeness.
- Failed post-commit append means the entry could not be confirmed, not that it is absent: writes can finish before sync fails. Tests inject failure after complete write/before successful sync for a real mutation and native publication/late-lock path, preserving committed receipts.
- Delete confirmation/success must describe retained encrypted backups without guaranteeing that the convenience `.bak` refresh succeeded; show any backup-refresh warning separately from the successful deletion.

## Round-one correction cycle

The [first fresh acceptance failed](reviews/round1/review-result.md); its original plan/diff remain immutable. Main owns all seven corrections serially: (1) typed backup/journal warning classification; (2,4) safe append and sanitized errors; (3) ID acknowledgment and remount/reload handoff; (5) uncertain confirmation and post-write fault tests; (6,7) CLI action names and offending-sequence presentation. Add failing-first actual-caller regressions, run affected checks, then pin a corrected candidate, renew relevant full checks, rebuild/hash once and run both focused native journal/export proofs. A new fresh read-only Terra `#max` acceptance follows. Storage migration stays blocked; no extra implementation worker or global routing changes.

## Acceptance and checks

- Native and actual App/API regressions for A's undrained post-commit warning after B opens; locking-overtaken commit; delayed warning reply; application warnings retained; no A-specific metadata or false lock of B.
- Core tests for missing/empty/deleted logs, malformed final/middle JSON, broken links/hash, display limits with full validation, byte preservation, and concurrent append/read snapshots.
- CLI tests for absent/corrupt/consistent audit output, unknown completeness, requested versus completed action names and unchanged pre-disclosure failure refusal/post-commit success warnings.
- Activity tests for each state, stale refresh/error handling, honest action labels, no missing-log verification or inactivity claim. Existing secret/session/import/export regressions must remain passing.
- Affected checks while editing: `cargo test -p akm-desktop --lib --offline --locked warning`; `cargo test -p akm-core --offline --locked audit`; targeted CLI security tests; targeted Vitest files.
- Stable candidate: `cargo test --workspace --offline --locked`; strict workspace Clippy; rustfmt check; `npm test`, `npm run build`, `npm run test:ui` in desktop; release CLI and Tauri GUI/bundles.
- Rebuilt real-surface proof: focused native journal/warning flow on temporary fake vaults with clipboard tests skipped; exact executable hash and owned settings recovery recorded. Run independent KeePassXC when relevant persistence callers change, without claiming a new library audit.
- Fresh reviewer receives this plan, combined before/after diff, pinned sources and verification results, not implementer reasoning. Record raw verdict and supported dispositions. No numerical rescore is implied.
