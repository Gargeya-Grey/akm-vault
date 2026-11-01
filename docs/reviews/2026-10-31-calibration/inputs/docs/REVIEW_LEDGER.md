# Review ledger

Updated 2026-10-31. This is a disposition index, not a fresh security audit. [REVIEW.md](../REVIEW.md) is the original scored report. Raw later reviews and probes are retained in [history](history/).

## Reading the statuses

- **Addressed** means phase-one code and recorded regression/proof evidence address the original scenario. It does not mean every related risk is eliminated.
- **Partial** means a demonstrated subproblem was corrected but named behavior or coverage remains open.
- **Open** means the original concern remains in scope or current-source inspection still supports it.
- **Deferred** identifies later release work, not acceptance of the risk for real secrets.
- **Coverage gap** is not automatically a product defect. Keep it separate from a reproduced failure.

## Original critical and high findings

- **C1, child master-password exposure. Addressed for the original leak.** Password CLI argument removed, child `AKM_*` scrubbing and vault release are covered by executable tests. Parent-process `AKM_PASSWORD` exposure and quick unlock remain later work.
- **C2, cross-project/empty import overwrite. Addressed.** Exact project/environment/name matching, validation and reviewed confirmation have core, frontend and native GUI evidence.
- **C3, older/KeePassXC format write failure. Addressed.** Explicit read-only upgrade gating and original backup preservation have core, GUI and real KeePassXC evidence.
- **H1, phantom mutation after failed save. Addressed.** Candidate transactions and stale-write regressions verify no mutation of the open snapshot on failure.
- **H2, copied AKM_ID targeting. Addressed.** Native UUID identity has clone/foreign-entry and attachment-preservation regressions plus independent-reader/GUI evidence.
- **H3, clipboard history/cloud and lifecycle. Addressed for the reproduced cases.** Flags, owner-aware cleanup and post-close sequence validity passed real Windows tests and the resumed GUI proof. Crashes, hostile readers and third-party managers remain explicit limitations.
- **H4, lifetime vault locks. Addressed.** Independent reader snapshots and short writer locks permit GUI/CLI coexistence and release before child execution.
- **H5, Windows command shims. Addressed.** Explicit `.cmd` resolution and conservative executable argument tests replace bare `.exe` assumptions.
- **H6, no auto-lock. Partial with coverage gaps.** Native fake-clock policy, synthetic notifications and GUI lock cleanup passed. Actual OS delivery and ten-minute wall-clock expiry remain unverified.
- **H7, implicit all-secrets export/injection. Addressed.** Both scope fields or explicit CLI `--all` are required. GUI export remains scoped.
- **H8, unchecked saved file/one backup. Addressed within the documented model.** Decrypted read-back comparison and retained dated backups are tested. Same-library verification is not independent cryptographic assurance; same-folder backups are not disaster recovery.
- **H9, wrong rotation clock/no rotate workflow. Open.** `model.rs::rotation_due_in` still uses modification/creation time. No dedicated secret-change timestamp, complete rotation action or new history workflow is established.

## Original medium findings

- **M1, synced plaintext journal/weak integrity. Open.** `audit.rs::for_vault` still selects `<vault>.audit.jsonl`, and entries contain titles/scope/detail. Append serialization fixed corruption, not privacy, authentication or truncation detection.
- **M2, password policy/change workflow. Partial.** Creation minimum is enforced in core and tested. A safe master-password change command/workflow is not implemented; decide its phase separately from rotation of API keys.
- **M3, stale fingerprint race. Addressed as far as local refusal can promise.** Fingerprints are rechecked before replacement. A non-cooperating cloud writer can still race the final syscall; distributed CAS is not claimed or an unfinished local-lock fix.
- **M4, GUI overwrites username. Addressed.** `KeyDraft::into_input` preserves an omitted username; frontend sends the existing value. Native `omitted_username_is_preserved_on_update` and `KeyForm.security.test.tsx` cover it.
- **M5, dotenv dialect incompatibility. Open.** The current parser round-trips its own output but is line-oriented and lacks a defined external compatibility profile for comments, multiline quoting and BOMs.
- **M6, journal failure misreports committed writes. Partial.** Post-commit warning behavior, damaged-log mutation regression and CLI reveal logging were added. `AuditLog::tail`/`read` still reject malformed JSON, so journal recovery/read availability needs deliberate handling.
- **M7, history/deletion resurrection. Open.** Existing imported history is preserved, but normal update/remove do not establish the requested native history, Recycle Bin and deletion-record workflow.
- **M8, webview-path/plaintext export safety. Addressed.** Native dialog authority, local persistent ACL checks, per-handle permission verification, owned publication/cleanup and exact-path proof now exist. Crash erasure and persistent OS deletion failure are not guaranteed.
- **M9, group-name case sensitivity. Addressed.** List/target/group lookup use case-insensitive matching; the native UUID/foreign-entry core regression covers case variants.
- **M10, PowerShell UTF-16 export. Addressed.** CLI `--output` writes protected UTF-8 files. Shell redirection remains the shell's behavior, not the recommended export path.
- **M11, capture protection. Addressed as defense in depth.** Tauri content protection and native affinity `0x11` were verified. Cameras and unsupported capture paths remain outside that promise.
- **M12, dialog focus and row accessibility. Partial.** Shared focus handling and a confirmation focus/restore test exist. Full modal coverage and row status announcement remain to be checked; title-only `aria-label` is not proof of state announcement. Recorded Keys layout/accessibility debt remains.
- **M13, in-app-only reminders. Open.** Correct rotation data and an explicit notification-privacy policy should precede richer reminder delivery.

## Original low findings

- **L1, unnecessary plaintext copies/Debug/decrypted child lifetime. Partial.** Child lifetime was fixed. `ApiKey` and `ApiKeyInput` still derive secret-bearing `Debug`; `Vault::list` materializes secrets even for metadata consumers. Frontend views omit secrets and some temporary copies are zeroized, but that does not close the original concern.
- **L2, ambiguous/fuzzy CLI selection. Partial.** UUID and exact-title preference with ambiguity refusal are tested. Fallback search still includes notes and URL. Define safer selection policy before changing destructive/disclosure workflows.
- **L3, expiry instant inconsistency. Open.** CLI date-only input and GUI `T23:59:59Z` do not have one canonical instant policy.
- **L4, failed-write temp leftovers. Addressed for ordinary tested failures.** Transaction/export cleanup regressions exist. Crash recovery and OS refusal are separately bounded limitations.
- **L5, reserved process variable imports. Addressed.** Import/export/injection validation rejects reserved process-control names. Keep those denials when broadening the parser.
- **L6, day truncation. Partial.** Frontend expiry `daysUntil` uses ceiling. Rotation still derives truncated whole days in Rust. Boundary behavior needs a shared policy and clock-based tests.

## Premise/process findings

- **P1, storage assurance overclaim. Partial.** Docs distinguish established format from experimental community writer; read-back and real KeePassXC evidence exist. Independent integration/professional assurance and expanded cross-reader tests remain.
- **P2, weak differentiation. Product framing retained.** The niche is a personal Windows KeePass API-key workflow. Do not spend phase two building a hosted/team product to manufacture differentiation.
- **P3, stale-binary verification. Corrected process, not permanent immunity.** Final artifact hashes and exact-release checks are recorded. Every later binary must be tied to its source and coverage. This original process ID is unrelated to later findings whose severity was also P3.
- **P4, shareability. Partial/deferred.** Windows-only CI, local hashes and scoped independent reviews exist. Signing, updater, SECURITY.md, installation testing and professional review remain open. No remote CI run or publication has occurred.

## Subsequent adversarial review history

1. **First phase-one correction review failed.** Six supported issues concerned committed results suppressed after lock, stale unlocked frontend responses, preview/token scope mismatch, export I/O under the security gate, clipboard ownership capture and failed CLI-export cleanup. The first review's standalone verdict file was not found during consolidation; [PHASE1.md](../PHASE1.md) records the outcome and selected original scratch probes are archived. Do not invent a missing verbatim report.
2. **Second correction review failed.** [Raw report](history/reviews/correction/review-result.md) found desktop temporary-path ownership, hidden opening errors, interrupted bootstrap recovery, hidden import reviews and an incorrect Unix-export claim. These have permanent corrections/regressions.
3. **Runtime clipboard failure changed the verdict.** [Intermediate addendum](history/reviews/correction/review-addendum.md) records sequence `2350` becoming `2353`. The repaired frame materializes Windows text formats before capture. The final standalone real-clipboard test passed. An old statement that this is still runtime-blocked is historical, not current status.
4. **Cross-session receipt follow-up failed.** [Follow-up](history/reviews/correction/followup-review-result.md) reproduced A's late receipt false-locking active B. Generation-owned acknowledgments corrected it; the actual App/API regression is permanent.
5. **Fresh acceptance review found one remaining P3.** [Raw report](history/reviews/acceptance/review-result.md) reproduced an old lock reply destroying B's newer error. It found no additional supported P0-P2 blocker within its reviewed scope, not a whole-product certification.
6. **Bounded final correction approved.** [Merged verdict](history/reviews/acceptance/followup-review-result.md) independently passed both old-success/old-failure regressions. Its then-open GUI verification was later completed by the main session's resumed proof. The raw reviewer report is preserved, not rewritten to claim it ran the GUI.

## Updating this ledger

For a new finding, record a stable ID, severity, reviewed source identity, file/line or public seam, exact scenario, evidence path and disposition. Close it only with a regression and the appropriate real-surface evidence. Record a disputed claim as disputed with its evidence, not silently deleted. Preserve raw review reports and their hashes. Do not infer a new numerical score from closed IDs or projected phase totals.
