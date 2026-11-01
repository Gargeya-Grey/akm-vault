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

## Fresh phase-two calibration, 31 October 2026

The [raw Terra verdict](reviews/2026-10-31-calibration/review-result.md) conditionally supports reversible fake-data engineering, not real-secret use or a numerical rescore. The [main-session disposition](reviews/2026-10-31-calibration/disposition.md) records all eight findings. The caller requested `#max`; active effort was not independently visible to the reviewer.

- **P2-WARN-OWNERSHIP. Open P2, inspection-supported current defect.** `session.rs` retains a shared warning queue across lock/open. A's post-commit journal warning can be drained by B. Bind warnings to operation/session ownership, retain a secret-free acknowledgment for A and test ordinary and `completedWhileLocked` native/App interleavings. Do not mistake a frontend stale-reply regression for native queue isolation.
- **P2-JOURNAL-CLAIMS. Open P2, inspection-supported current defect.** Missing logs can appear verified, UI copy implies complete events, pre-action records can display as completed actions, and verify/read can use different snapshots. Define absent, corrupt, consistent-but-unauthenticated and incomplete/unknown states; distinguish attempts/outcomes and verify the displayed snapshot under one lock.
- **P2-JOURNAL-IDENTITY. Open P2 design gate.** Freeze rename/clone/replacement/rollback/ambiguity and audit-without-password semantics plus private journal/mapping/lock ACL and reparse policies. Do not silently mutate read-only vaults, merge clone histories or fall back to synced sidecars.
- **P2-JOURNAL-OUTCOMES. Open P2 preservation/design gate.** Preserve bytes and classify damage without asserting its cause. Preserve fail-closed pre-disclosure logging; after commit/publication, failures are warnings with truthful non-retryable completion. These policies are not interchangeable.
- **P2-HISTORY-SEMANTICS. Open P2 design/test risk.** Tracked library APIs need deliberate no-op, same-second edit, clock rollback, recycle/delete/restore and tombstone behavior. Test stale copies and merges both directions with foreign history and shared/history-only attachments.
- **P2-TIME-REFRESH. Open P2, inspection-supported current defect.** Key statuses calculated at fetch time can stay stale across expiry/rotation boundaries. Test the mounted view, hidden/resumed state and clock changes without counting refresh as activity.
- **P2-GUI-PROVENANCE. P3 evidence limitation, partially documented.** Earlier partial-run and build stdout were recovered in [the supplement](history/phase1-supplement/manifest.json). The preceding GUI hash has only secondary conversation provenance and is not independently bound to that raw run. That run failed after its earlier passes; it is not a complete final-GUI proof. The exact binding remains unavailable.
- **Finding 8 maps to existing H9, M5/M12/M13 and L1/L2/L3/L6.** Their remaining scenarios and acceptance tests were tightened, not claimed fixed or counted as new independent deductions.

All three new application defects remain open by inspection. No reproducer, implementation or runtime verification was run in this calibration. The earlier scoped phase-one approvals remain historical evidence for their scopes, not a claim that these newly identified seams were covered.

## Unit 1a implementation acceptance, round one

**Rejected.** The [raw Terra review](work/phase2-1a/reviews/round1/review-result.md), original plan/diff/source identities and fake-data probes are preserved in the [archive manifest](work/phase2-1a/reviews/round1/manifest.json). Reviewed source manifest SHA-256 `075367639931c07212c3ffa5660f5a7bd1b91c0478c161aaab2cfcef52c1a6a5`. All findings below are accepted as supported and pending correction; earlier 144/34/8 passing checks do not close them.

- **1A-R1-01, P2, warning classification. Open.** Core backup refresh/pruning and journal warnings share a string vector; native Session labels every warning as journal failure, including when the completed event exists. Preserve distinct typed core/native classifications and test each alone/together across lock/open.
- **1A-R1-02, P2, unsafe append boundary. Open, reproduced CLI.** A valid JSON final record without newline is concatenated with the next request, permitting disclosure before later damage is detected. Refuse before append/disclosure and preserve bytes; no tail repair.
- **1A-R1-03, P2, remount loss. Open, reproduced synthetic root lifecycle.** Destructive native drain plus event-only delivery loses delayed notices between mounts. Retain notices until ID dismissal, replay safe frontend state and cover webview reload without changing B's active state.
- **1A-R1-04, P2, malformed error echo. Open, reproduced CLI.** Direct append/tail errors include serde values such as `FAKE_JOURNAL_MARKER`. Sanitize structural errors across CLI/native disclosure callers and preserve damaged bytes.
- **1A-R1-05, P2, uncertain journal outcome. Open, inspection-supported.** Complete line writes can precede failed sync; categorical "not recorded" notices overstate absence. Use "could not be confirmed" and inject after-write/before-sync failures for mutation/native publication, preserving non-retryable success.
- **1A-R1-06, P3, legacy emission names. Open, reproduced CLI export.** New run/export events still emit `*-start`. Emit `run-requested`/`export-requested`, retain honest legacy labels and fail-closed ordering.
- **1A-R1-07, P3, omitted offending sequence. Open, reproduced CLI.** Snapshot supplies a known `broken_at` but CLI/Activity show only physical line. Display both when known and no invented sequence for malformed data.

The reviewer ran focused fake-data CLI probes and one root-lifecycle regression, not GUI/clipboard/full suites. Caller requested Terra `#max`; active effort was not independently observable. No numeric rescore or readiness approval is implied.

### Corrected candidate, awaiting new independent acceptance

Main-session corrections address all seven IDs without rewriting their failed verdict above. [Evidence](work/phase2-1a/corrections/evidence.md) ties the new checks to source and final artifact hashes. Findings remain unclosed until fresh acceptance:

- **01:** typed core records/native kinds; Session covers refresh/prune/journal alone/together through ordinary and lock-overtaken completion into B, App/API fixed-copy checks pass.
- **02/04:** one exclusive-lock snapshot validates before append, refuses unsafe boundaries/chain damage and returns sanitized structural errors; core/actual CLI/native request regressions preserve bytes and withhold fake secrets/markers.
- **03:** non-destructive native ID snapshots + explicit registered acknowledgment; safe frontend replay and stale-poll dismissal tests pass. Exact GUI proved dismissal through subsequent native polls. Fresh browser-document reload uses mocked retained IPC; actual Tauri webview reload is not separately exercised.
- **05:** uncertain-confirmation copy and real-mutation/native-publication after-write fault injections pass, including late lock/openB. These are injected faults, not an observed failing Windows disk sync.
- **06/07:** requested CLI emissions and unknown legacy-start labels; CLI/Activity physical line + known sequence tests pass; malformed records do not invent a sequence.

Full Rust153/frontend43/browser10, strict lint/builds, exact GUI journal/export proofs and final-CLI KeePassXC pass are main-session evidence, not reviewer reruns or whole-product approval. First export-only startup-discovery attempt failed and is preserved; the same binary passed after an owned UI Automation root refresh. Layout debt is explicitly reported rather than waived as a checker pass.

## Unit 1a acceptance, round two

**Rejected**, raw [report and eleven archived records](work/phase2-1a/reviews/round2/manifest.json). Source manifest SHA-256 `72202c318c7a9c1b3d7bf057a89bd31277b921c9bdcd8c3cdc10e1267442d066`. No pinned application/build input changed during review. The reviewer supports the seven earlier corrected seams but establishes these additional gaps; all accepted as supported and pending correction:

- **1A-R2-01, P2, whitespace append boundary. Open, release-CLI reproduced.** ASCII whitespace skipping includes non-JSON form-feed; an unterminated skipped fragment can prefix the next object, permitting disclosure with no valid request. Validate every byte/separator before append, preserve bytes and test all control/JSON whitespace at empty/nonempty boundaries in core/CLI/native callers.
- **1A-R2-02, P2, completion handoff. Open, actual App/API mocked-native reproduced.** `completedWhileLocked` receipts still use an event-only channel and disappear between roots without a separate warning. Retain secret-free generation-owned receipts until explicit acknowledgment, replay without false-locking B and test both between/after remount delivery.
- **1A-R2-03, P3, application warning handoff. Open, actual App/API mocked-native reproduced.** Native application warnings drain while frontend retains only operation notices. Retain app-global safety warnings separately with explicit dismissal, not as B's vault-specific warnings.
- **1A-R2-04, P3, inherited action keys. Open, valid journal plus Activity reproduced.** `__proto__` resolves to an inherited object in the action dictionary and crashes React. Own-key or Map lookup; test `__proto__`, `constructor`, `toString` as unknown outcomes.
- **1A-R2-05, P3, delete backup promise. Open, pre-existing direct caller.** Delete success/confirmation guarantee `.bak` despite a committed deletion with failed refresh and a dated encrypted original retained. Correct both messages and test immediate success plus the refresh warning.
- **1A-R2-06, P3, sequence precision. Open, JS rounding reproduced/native seam inspection-supported.** `u64` offending sequence serialized as JSON number rounds beyond `2^53`. Use exact decimal strings for IPC audit sequences and test nearby boundaries/malformed records.

Runtime suites/builds/native proofs were not independently repeated. First export startup discovery failure remains preserved. No numeric rescore is implied. The failed second verdict does not erase the earlier corrections or certify the rest of the product.

### Second correction cycle, not independently accepted yet

[Second-cycle evidence](work/phase2-1a/corrections2/evidence.md) records failing-first and corrected actual-caller checks for all six IDs. Blank-record tests cover 1,024 byte-boundary cases; completion receipts retain in-memory IDs/generations across roots; application warnings retain native IDs through explicit acknowledgment; action lookup is own-key; delete copy no longer guarantees `.bak`; native audit serializers send exact decimal strings while leaving the journal file format unchanged.

Main-session full Rust157/frontend55/browser10/lint/builds passed. Exact-GUI journal proof additionally passed real delete-with-backup-failure copy, literal unknown actions and the large sequence through actual IPC; export proof and final-CLI KeePassXC passed. GUI journal startup needed the existing owned-root refresh recovery. These are correction evidence, not closure of the rejected verdict or numerical readiness. A new independent bounded verdict is required.

## Unit 1a acceptance, round three: returned to owner

**Rejected; implementation paused for the owner's decision.** The [raw report and seven archived records](work/phase2-1a/reviews/round3/manifest.json) retain the final bounded review. Source-manifest SHA-256 `28926546f3b1b604bbf99ea81086443bd060cf622c042f82d21685465222af44`, 145 paths. Reviewer independently verified the baseline, reconstructed all 29 changed pinned paths from the combined diff and matched the four final artifacts. Only the allowed main-owned state document differed. No application change followed this verdict.

- **1A-R3-01, P2, dispatched outcome across view handoff. Open, mocked-native App/Settings/API reproduced.** Confirm import, hide/restore, navigate from Settings before reply, then return: a successful reply is dropped by the view request fence while the App root stays mounted, so no retained acknowledgment exists. This is a pre-existing direct caller of the preservation contract, not a claimed new regression. Probe did not perform a real import. Preserve secret-free outcomes outside Settings, cover success and failure, avoid stale scope/secret display or false-lock of B; inspect dispatched export for the same lifetime pattern.
- **1A-R3-02, P3, distinct safety-warning episodes. Open, inspection-supported.** `warn_once` deduplicates by message while an earlier ID remains pending. Failure, recovery, then a separate identical failure can all share ID1; dismissing the first observed ID loses the newer occurrence. Distinguish episodes from retries of one uninterrupted failure. Acceptance needs injected failure/recovery/failure before old acknowledgment, with a new ID retained across poll/lock/open. No real clipboard fault was induced.
- **1A-R3-03, P3, incomplete stale-reply API fence. Open, mocked-native App/API reproduced.** Old `vault_status` success and ordinary native rejection can return A path/name to obsolete callers after teardown because status bypasses success fencing and catch only handles completion receipts. B remained rendered and its DOM did not show A metadata; do not label this as a reproduced B-screen disclosure. Return sanitized cancellation to obsolete callers while preserving safe completion receipts, startup and stale-lock behavior; test between and after roots.

The reviewer supports the bounded journal, typed operation warning, action-label and sequence seams by inspection but rejects whole-unit acceptance. Its only executed behavior probes were three fake-data frontend cases; full157/55/10, native GUI and KeePassXC results remain main-session evidence. Requested Terra `#max`, active effort/token usage unavailable. All three failed contexts remain immutable. Do not begin another correction cycle without the owner; no readiness score changes.

### Owner-authorized recalibration after round three

The owner requested recalibration and implementation of one new bounded cycle. [Cycle plan](work/phase2-1a/corrections3/PLAN.md) separates protected view results from application-owned dispatched outcomes, message delivery IDs from owned cleanup episodes, and sanitized stale cancellation from actionable current errors. [Evidence](work/phase2-1a/corrections3/evidence.md) preserves failing-first frontend33/native6 and stable Rust163 (timeout/selective exact-workspace resume), frontend105/browser18/lint/builds. Exact-GUI journal/export proofs pass with settings restored. **Actual native hide/navigation handoff is unproven:** both minimized-state attempts failed before those assertions; this is a proof gap, not a demonstrated dropped receipt or a synthetic pass. R3 findings await fresh scoped acceptance; no whole-unit/runtime readiness claim yet. No other phase-two implementation or readiness score is authorized.

## Updating this ledger

For a new finding, record a stable ID, severity, reviewed source identity, file/line or public seam, exact scenario, evidence path and disposition. Close it only with a regression and the appropriate real-surface evidence. Record a disputed claim as disputed with its evidence, not silently deleted. Preserve raw review reports and their hashes. Do not infer a new numerical score from closed IDs or projected phase totals.
