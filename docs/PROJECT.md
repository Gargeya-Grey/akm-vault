# AKM project brief

Stable requirements and contracts. Mutable progress belongs in [STATE.md](STATE.md). This is a consolidation of recorded requirements, not a verbatim transcript of the initial conversation.

## Owner's objective

Build a personal Windows API-key manager with a desktop GUI and CLI. Keep local, account-free KeePass storage while making API-key metadata, scoped environment injection, expiry and rotation practical.

Reuse established encrypted storage and cryptographic implementations. Do not invent a vault format or cryptography. The owner requires an independent adversarial score **above 90/100**, followed by testing, before trusting the result with real secrets or sharing it.

## Required workflows

- Create, unlock, search, inspect and manage API-key entries through GUI and CLI.
- Preserve metadata, issuer, project, environment, scopes, username, tags, notes and expiry.
- Provide correct rotation tracking and recoverable history, rather than treating every metadata edit as a rotation.
- Copy with Windows privacy exclusions and ownership-aware cleanup.
- Review scoped `.env` imports. Export or inject only an explicit scope, or an explicit CLI `--all`.
- Keep encrypted backups and a useful activity journal without leaking metadata into a cloud folder by default.
- Support defensive use of fully downloaded cloud-sync folders. Detect conflicts and refuse stale writes; do not silently merge them.
- Resume interrupted engineering work without losing findings or needlessly repeating valid verification.

## Non-goals and trust boundaries

- macOS, mobile platforms, team accounts, hosted secret storage and a new sync protocol are outside the current scope.
- This is a narrow personal workflow, not a replacement for an audited password manager or team secrets service.
- A trusted Windows account and OS are assumed. An unlocked app cannot protect secrets from malware running as that account.
- Clipboard exclusions, memory cleanup and capture exclusion are defense in depth, not absolute confidentiality guarantees.
- Short local locks do not provide distributed consensus or atomic cloud compare-and-swap. Other applications and sync clients can race a final local check.
- The current plaintext journal is not authenticated evidence. Its historical truncation/tampering limitations must not be hidden by a new location or hash chain.

## Established implementation

- Rust workspace with `akm-core`, `akm-cli` and a Tauri 2 native desktop layer. React and TypeScript provide the webview UI.
- `keepass = "=0.17.0"`, using `save_kdbx4`. New vaults use KDBX 4.1, AES-256, ChaCha20 inner protection, GZip and Argon2id v1.3 with 64 MiB, 3 iterations and parallelism 4.
- The KDBX format and primitives are established. This community crate labels its writer experimental. Neither library integration nor application has a professional audit.
- KDBX 3.x and 4.0 open read-only until explicit upgrade. Upgrade preserves encryption/KDF settings and an original encrypted backup.
- Entry identity is the native KDBX UUID, never the copyable `AKM_ID` field. Unrelated groups, foreign entries, attachments and existing history must survive changes.

## Contracts phase two must preserve

1. Mutations work on a candidate snapshot. A pre-commit failure must not mutate the open snapshot or leak into a later save.
2. Decrypt and compare the candidate before replacing the vault. Recheck the source fingerprint immediately before publication and retain five dated encrypted backups plus the convenience `.bak`.
3. Readers hold independent snapshots. Cooperating writers take short write locks. Journal appends use their own short lock.
4. A committed operation overtaken by locking returns a secret-free `completedWhileLocked` receipt. Do not turn success into a retryable unsaved error. Reads suppress obsolete plaintext.
5. Frontend epochs and request/transition identities reject stale results. A receipt from session A must not invalidate a newer session B.
6. Native idle/session/suspend policy is authoritative. Polling is not activity. GUI state alone is not proof that native state is locked.
7. Import matching is exact on project, environment and variable name. Review labels and tokens share an immutable scope. Edits, cancellation, hide, reload and lock invalidate pending reviews; already-dispatched confirmation keeps its outcome acknowledgment.
8. Exports validate explicit scope, permitted local persistent-ACL storage and private handle permissions before plaintext writing. Keep file-handle ownership through publication and cleanup. CLI never overwrites an existing destination; desktop overwrite requires the native dialog's approval.
9. Bulk export I/O stays outside the security gate. Prepublication authorization is the cancellation boundary. Already-authorized filesystem publication may finish after a lock and must report truthfully.
10. Clipboard exclusions and Unicode/ANSI/OEM/locale formats are installed before sequence capture in one open interval. Cleanup must not adopt a later sequence or erase a newer identical user copy.
11. Child processes never inherit `AKM_*`, including `AKM_PASSWORD`. Release the vault before starting a child. Windows command shims are resolved explicitly with conservative argument handling.
12. Post-commit journal failure is a warning, not an invitation to repeat a completed mutation. Logging failure before disclosure/export/launch currently fails closed and is covered by CLI regressions; do not silently replace it with the post-commit policy.

These contracts do not claim every adjacent seam is complete. The fresh phase-two review identified open native warning ownership, journal-presentation and stale time-status defects. See the ledger rather than inferring that the earlier scoped acceptance covered them.

## Decision history

- Phase one selected compatibility with KDBX rather than bespoke storage. Save verification and independent-reader tests compensate for some integration risk, not all of it.
- Explicit upgrades replaced automatic format changes. Password policy for newly created vaults is a minimum of 12 Unicode characters, not a claim of password strength.
- Conflict refusal and reload replaced lifetime reader locks. Automatic cross-device merging remains deliberately absent.
- Scope confirmation, generation-owned receipts, handle-owned exports and Windows clipboard synthesis handling were added after reproduced adversarial failures. They are preservation requirements, not optional refactors.
- Source corrections, runtime coverage and numerical readiness are separate gates. The last complete numerical assessment is the historical 40.5/100 baseline in [REVIEW.md](../REVIEW.md).
- The owner authorized GPT-6.1 Terra at `max` for fresh AKM adversarial reviewers on 2026-10-31. The tool catalogue resolved this to `openai/gpt-6.1-terra#max`. No global routing configuration was changed.
- On 2026-11-01 the owner separately authorized local Git and private source backup under `Gargeya-Grey/akm-vault`. Public visibility, artifact releases, remote CI and real-secret use remain gated; private backup is not product acceptance. See [repository/evidence policy](REPOSITORY.md).

## Source map

- `crates/akm-core/src/vault.rs` owns transactions, KDBX identity and persistence.
- `crates/akm-core/src/model.rs` owns API-key fields and rotation calculations.
- `crates/akm-core/src/audit.rs` and `dotenv.rs` own the current journal and dotenv dialect.
- `crates/akm-cli/src/` owns child isolation, CLI workflows and protected exports.
- `desktop/src-tauri/src/` owns session policy, commands, clipboard and native exports.
- `desktop/src/` owns frontend lifecycle, forms and reviewed workflow presentation.
- `tools/verify_keepassxc.py` and `tools/verify_windows_app.ps1` are repeatable real-surface proofs.
