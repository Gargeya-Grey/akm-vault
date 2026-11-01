# Phase two: correctness and trust

Status on 2026-10-31: **unit 1a authorized and beginning under its [frozen plan](docs/work/phase2-1a/PLAN.md).** The [calibration review and disposition](docs/reviews/2026-10-31-calibration/disposition.md) remain historical inputs. Other phase-two units are not started. This replaces the old projected task list, not the preserved original review.

## Goal and entry decision

Correct the remaining privacy, lifecycle-data and interoperability defects without weakening phase-one contracts. Work with fake secrets only. Phase two does not promise the original projected 86.5 points, nor automatic progress to a score above 90.

Phase-one scoped corrections and the remaining native proof passed. Actual OS delivery, wall-clock idle and full rubric rescoring remain open. The fresh review conditionally supports reversible engineering with isolated fake data after its findings are dispositioned and implementation is authorized. It identified three additional inspection-supported P2 defects in warning ownership, journal presentation and live time-derived status. They remain open and block treating those behaviors as complete.

The calibration was one documentation/handoff unit plus one fresh reviewer. The owner subsequently authorized unit 1a. Its main-session implementation is serial and a new fresh review follows the stable change. No other unit, storage migration or parallel application worker is authorized by that first-unit approval. Serialize units that share `model.rs`, `vault.rs`, DTOs or CLI command definitions.

## Preservation requirements

All [project contracts](docs/PROJECT.md#contracts-phase-two-must-preserve) remain acceptance criteria. In particular retain transactional snapshots, native UUIDs, read-back verification, explicit older-format upgrade, original encrypted backups, truthful committed receipts, generation fencing, scope-bound reviews and handle-owned exports.

Do not reimplement fixed M3/M4/M8/M9/M10/M11 or the already corrected parts of M6/M12 merely because they appear in the original phase-two roadmap. Their remaining subproblems are in the [ledger](docs/REVIEW_LEDGER.md).

## Work unit 1: private per-device journal

The reviewer agrees this should be the first area because same-folder metadata exposure is active. It is not approval of an unspecified migration design. Split it into two bounded slices; do not combine native history or visual redesign with this migration.

### Unit 1a: warning ownership and honest journal outcomes

- First reproduce `P2-WARN-OWNERSHIP` through native Session and actual App/API callers. A post-commit journal failure from A must not appear as B's saved-vault warning after lock/open. Cover both ordinary completion and a commit overtaken by locking. Preserve A's secret-free acknowledgment rather than simply dropping the queue. Distinguish operation/session warnings from app-global policy/clipboard warnings.
- Replace completeness/verification overclaims with explicit absent, corrupt, consistent-but-unauthenticated and incomplete/unknown states. A missing log does not prove that no action occurred. A consistent unanchored chain does not prove completeness or authorship.
- Record/display attempts separately from known outcomes. A logged reveal/copy/export request can fail or be cancelled before disclosure/publication. Read and verify one locked snapshot, including an append interleaving test.
- Preserve existing fail-closed logging before CLI/native disclosure, export or launch. A failure after commit or publication must instead retain truthful non-retryable completion and a warning. Do not weaken that boundary accidentally while making logging more available.
- One owner handles the native warning/command contract, App/API callers, journal report DTO, Activity and regressions. Freeze any IPC shape change before implementation. Add missing/deleted/corrupt log, failed clipboard/disclosure, late commit and concurrent append/read cases.

### Unit 1b: private storage and migration

Start storage changes only after the following contract is concrete. Unit 1a establishes the outcome/ownership semantics the migration must preserve; it does not settle vault identity automatically.

### Decisions to freeze first

- Choose private non-roaming location and vault identity/keying. Specify rename, clone, replacement, rollback, two separate vaults, ambiguous identity and `akm audit` without a password. A path-only key, save-changing content hash or copied root UUID is not automatically sufficient. Do not silently mutate an older-format or filesystem-read-only vault, or merge clone histories to force a stable ID.
- Decide exactly what is recorded. Default to minimizing data, not relocating titles, scopes, export paths and secrets to another plaintext file and calling that authenticated privacy.
- Specify secure creation and Windows ACL verification for the journal, mapping and lock, including reparse points, denied permissions and unsupported storage. No fallback to the synced sidecar. Apply the separate pre-disclosure versus post-commit failure policies from unit 1a.
- Specify legacy sidecar handling. Detect and explain historical metadata exposure. Do not silently delete, upload or import an existing log, or imply relocation removes synced copies.
- Specify malformed/truncated tail behavior and concurrent readers/writers. Preserve original bytes and report observable truncation, malformed JSON, damaged middle records or broken links without claiming to know whether a crash or deliberate alteration caused them. Never silently bless an altered prefix as complete history.
- State an honest integrity guarantee. A local hash chain without an independent authenticated anchor cannot prove completeness. Do not churn or write an older vault on every read just to anchor activity.

### Candidate ownership and checks

One owner integrates `audit.rs`, Vault logging, CLI audit/reveal/export/run callers, native Audit DTO/commands, Activity and tests. The public journal interface and migration behavior are fixed before a worker is briefed. No concurrent worker edits those consumers.

Acceptance covers private-location/permission/reparse refusal, no plaintext metadata writes beside a fresh synced vault, rename/clone/replacement/rollback/ambiguous identity, audit without a password, concurrent mapping/append/read, preserved damaged bytes, older-format and filesystem-read-only vaults, legacy-log non-destruction and both pre-disclosure and post-commit failures. Exercise actual CLI and Activity callers, not just the storage helper.

## Work unit 2: rotation, native history and recoverable deletion

Depends on stable outcome contracts, not completion of journal storage or a UI redesign. History design can proceed independently. Implementation must serialize owners of shared model/persistence/caller files.

- Introduce explicit secret-change time. Metadata edits, unchanged imports and no-op operations must not reset it. Define the honest state for an old entry with no known rotation time rather than inventing a precise historical date.
- Specify explicit rotate semantics, unchanged-value behavior, timestamps and exactly-once outcomes across lock/cancellation. Keep secrets out of metadata listings and completion receipts.
- Use native KDBX history and deletion/Recycle Bin/tombstone mechanisms supported by the library and interoperable clients. Do not invent an incompatible secret-history format to work around a missing API.
- Preserve unrelated history, attachment references, foreign groups/custom fields and KDF/cipher settings. Prove update, delete, restore and foreign-client round trips rather than only opening the file.
- Define retention and purge as separate explicit actions. No automatic irreversible purge or migration of real vaults is included.
- Account for pinned library behavior before choosing tracked APIs. Tracked no-op edits can add history, tracked moves update location time, and KDBX timestamps have second precision. Freeze no-op detection, rapid same-second edits, clock rollback, restore parent/UUID, disabled/missing Recycle Bin and tombstone timing.

Acceptance includes notes-only edits, unchanged import, no-op/rapid edits, genuine secret change, clock rollback, failed save, late committed receipt, missing legacy timestamp, native history, recycle/permanent-delete/restore, and stale foreign-copy merges in both directions with real KeePassXC. Preserve foreign history and shared/history-only attachments. Use literal expected values and controlled clocks.

Safe master-password change is a separate open workflow from M2. Its old-backup credentials, crash/cancellation and recovery implications need their own plan; do not smuggle it into API-key rotation.

## Work unit 3: dotenv, time and CLI selection correctness

Break into small sequential changes if shared model/command files overlap.

- Define and document an external dotenv compatibility profile with pinned reference fixtures. Cover comments, quote/escape rules, BOM, CRLF, multiline values and malformed input. Never execute or interpolate file contents. Keep duplicate-name, empty-secret, scope and process-control-variable protections.
- Choose one date-only expiry instant policy for CLI, GUI, exports and reminders. Test UTC/local timezone boundaries and exact expiry instants. Distinguish expiry display rounding from rotation scheduling.
- Fix `P2-TIME-REFRESH`, not just arithmetic. Time-derived expired/rotation states must update while Keys remains mounted and after hide/resume or a system-clock change. Automatic refresh must not extend the idle-activity deadline. Test exact boundary crossings with controlled clocks.
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
