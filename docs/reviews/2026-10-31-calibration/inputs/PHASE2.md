# Phase two: correctness and trust

Status on 2026-10-31: **draft for fresh adversarial calibration; application implementation has not started.** This replaces the old projected phase-two task list, not the preserved original review.

## Goal and entry decision

Correct the remaining privacy, lifecycle-data and interoperability defects without weakening phase-one contracts. Work with fake secrets only. Phase two does not promise the original projected 86.5 points, nor automatic progress to a score above 90.

Phase-one scoped corrections and the remaining native proof passed. Actual OS delivery, wall-clock idle and full rubric rescoring remain open. These prevent trust/release claims; they need not prevent reversible engineering with fake data. A fresh read-only review will challenge that distinction before implementation starts.

This calibration is one documentation/handoff unit in the main session plus one fresh reviewer. No parallel implementation workers or application edits are authorized by this draft. When implementation begins, publish a bounded work-unit plan with fixed interfaces, exclusive file ownership and exact checks. Serialize units that share `model.rs`, `vault.rs`, DTOs or CLI command definitions.

## Preservation requirements

All [project contracts](docs/PROJECT.md#contracts-phase-two-must-preserve) remain acceptance criteria. In particular retain transactional snapshots, native UUIDs, read-back verification, explicit older-format upgrade, original encrypted backups, truthful committed receipts, generation fencing, scope-bound reviews and handle-owned exports.

Do not reimplement fixed M3/M4/M8/M9/M10/M11 or the already corrected parts of M6/M12 merely because they appear in the original phase-two roadmap. Their remaining subproblems are in the [ledger](docs/REVIEW_LEDGER.md).

## Work unit 1: private per-device journal

Recommended first unit because current same-folder metadata exposure is active and independent of a cosmetic redesign. This priority is proposed, not a reviewer-approved implementation decision.

### Decisions to freeze first

- Choose private per-device location and vault identity/keying. Moving or cloning a vault, an older read-only format and two independent vaults must not accidentally alias or require a silent vault mutation on read.
- Decide exactly what is recorded. Default to minimizing data, not relocating titles, scopes, export paths and secrets to another plaintext file and calling that authenticated privacy.
- Specify Windows permission validation and behavior when storage/permissions fail. Vault commits still must not become retryable errors after successful persistence.
- Specify legacy sidecar handling. Detect and explain historical metadata exposure. Do not silently delete, upload or import an existing log, or imply relocation removes synced copies.
- Specify malformed/truncated tail behavior and concurrent readers/writers. Preserve evidence of corruption, distinguish an interrupted trailing record from arbitrary damage and avoid silently "repairing" tampering into a trusted history.
- State an honest integrity guarantee. A local hash chain without an independent authenticated anchor cannot prove completeness. Do not churn or write an older vault on every read just to anchor activity.

### Candidate ownership and checks

One owner integrates `audit.rs`, Vault logging, CLI audit/reveal/export/run callers, native Audit DTO/commands, Activity and tests. The public journal interface and migration behavior are fixed before a worker is briefed. No concurrent worker edits those consumers.

Acceptance covers private-location/permission refusal, no plaintext metadata writes beside a fresh synced vault, distinct/relocated/cloned vault behavior, malformed-tail recovery/reporting, concurrent append/read, read-only operation, legacy-log non-destruction and a journal failure after a committed mutation. Exercise actual CLI and Activity callers, not just the storage helper.

## Work unit 2: rotation, native history and recoverable deletion

Depends on stable logging/outcome contracts, not on a UI redesign. One owner controls the shared core model/persistence changes and caller migration.

- Introduce explicit secret-change time. Metadata edits, unchanged imports and no-op operations must not reset it. Define the honest state for an old entry with no known rotation time rather than inventing a precise historical date.
- Specify explicit rotate semantics, unchanged-value behavior, timestamps and exactly-once outcomes across lock/cancellation. Keep secrets out of metadata listings and completion receipts.
- Use native KDBX history and deletion/Recycle Bin/tombstone mechanisms supported by the library and interoperable clients. Do not invent an incompatible secret-history format to work around a missing API.
- Preserve unrelated history, attachment references, foreign groups/custom fields and KDF/cipher settings. Prove update, delete, restore and foreign-client round trips rather than only opening the file.
- Define retention and purge as separate explicit actions. No automatic irreversible purge or migration of real vaults is included.

Acceptance includes notes-only edits, unchanged import, genuine secret change, failed save, late committed receipt, missing legacy timestamp, history retention, delete/restore, foreign-client merge/deletion behavior and shared/history-only attachment preservation. Use literal expected values and controlled clocks.

Safe master-password change is a separate open workflow from M2. Its old-backup credentials, crash/cancellation and recovery implications need their own plan; do not smuggle it into API-key rotation.

## Work unit 3: dotenv, time and CLI selection correctness

Break into small sequential changes if shared model/command files overlap.

- Define and document an external dotenv compatibility profile with pinned reference fixtures. Cover comments, quote/escape rules, BOM, CRLF, multiline values and malformed input. Never execute or interpolate file contents. Keep duplicate-name, empty-secret, scope and process-control-variable protections.
- Choose one date-only expiry instant policy for CLI, GUI, exports and reminders. Test UTC/local timezone boundaries and exact expiry instants. Distinguish expiry display rounding from rotation scheduling.
- Define CLI resolution safety for disclosure/destructive actions. UUID/exact-title preference and ambiguity refusal already exist; decide whether fuzzy notes/URL fallback is appropriate, rather than treating an old title-prefix bug as unfixed.
- Remove secret-bearing Debug output and unnecessary materialization for metadata-only consumers. Do not claim complete allocator/JavaScript zeroization or memory secrecy against the owner account.

Acceptance uses external parser fixtures as well as round trips, executable CLI regressions, cross-boundary date/clock fixtures, ambiguous selectors and metadata/Debug output assertions.

## Work unit 4: reminders and accessibility

Depends on correct rotation/expiry data. Freeze privacy and notification behavior before implementing background delivery.

- Specify unlock summaries and opt-in Windows notification contents. Locked-screen notifications must not disclose secret values or sensitive metadata by accident. Do not add an always-running daemon as an unexamined prerequisite.
- Verify every relevant modal's focus containment/return, including import. Confirm ledger status is announced, not hidden by a title-only accessible label.
- Resolve or explicitly retain the previously recorded Keys-screen layout/accessibility debt. Functional fixes and visual restyling are separate changes; keep the house design language.

Acceptance includes expired/overdue/unknown-date/no-keys states, locked/hidden transitions, notification denial/disabled settings, accessible status and keyboard-only paths. Mocked UI evidence does not prove native notification delivery.

## Work unit 5: independent-reader coverage and full reassessment

- Keep real KeePassXC coverage and add a pinned independent kdbxweb reader/writer suite where the format is supported. Distinguish client format support from AKM's explicit upgrade behavior.
- Run cross-reader fixtures for the new history, deletion, attachment, journal/privacy and scoped-env behavior on the exact rebuilt candidate. Renew invalidated phase-one evidence rather than blindly reusing it.
- Preserve Windows CI configuration and record an actual remote run only if one is authorized and occurs. Local checks are not a remote-CI pass.
- Reassess all nine original rubric areas with explicit evidence and no double-counted deductions. Independent reviewers must cover security, architecture/product premise and correctness. State exactly how many independent review contexts were used; a plan review is not a full rescore.
- Fix supported findings with permanent regressions, then review the changed candidate. Above 90 must be measured by a new full assessment, not extrapolated from the original projected table.

## Broader gates carried forward

Actual OS lock/suspend and wall-clock idle checks require a separate safe owner-assisted plan. Signing/updater, Windows Hello/DPAPI quick unlock, SECURITY.md, installation lifecycle testing and professional review remain explicit later release work unless the owner reprioritizes them. No publication, signing enrollment, installation, git setup or real-secret migration is included in this calibration.

## Verification and completion

Use [VERIFICATION.md](docs/VERIFICATION.md) to select affected checks and exact commands. For each stable cross-cutting candidate, collect relevant full suites, rebuild GUI/CLI/bundles, hash the artifacts and run applicable independent-reader/native flows. Record skips and coverage gaps separately from defects.

Before handoff or compaction, update [STATE.md](docs/STATE.md) and the ledger with source identity, evidence, open findings and one next action. Archive new reviewer output verbatim with provenance. Do not rewrite the historical failed reports or lower the owner's trust threshold to make the stage appear complete.
