# Windows hardening, phase 1

Goal: fix the demonstrated credential leaks and data-loss blockers before any real-secret testing.

This is Tier 2 work because it changes persistence, process spawning and desktop lifecycle together. The three implementation units own separate files. Frontend integration follows the agreed native contracts. A fresh reviewer checks the combined result.

## Ownership and acceptance

- Core worker owns `crates/akm-core/**`. Transactional mutations, scoped and validated imports with a preview, KDBX UUID identity, short write locks, explicit 4.1 upgrade, decrypted read-back verification, five retained backups and password policy. Public core tests prove no phantom state, no wrong-project changes, no incorrect clone targeting, no unconfirmed upgrade, and conflict refusal.
- CLI worker owns `crates/akm-cli/**`. Remove the password argument, scrub AKM variables in children, resolve Windows command shims, require explicit export scope, release vault before spawning, explicit upgrade, UTF-8 file export and executable regression tests.
- Desktop native worker owns `desktop/src-tauri/**`. Clipboard exclusions and cleanup, backend-enforced idle/session/suspend locking, safe asynchronous unlock, scoped exports, import preview confirmation, upgrade/reload commands and screenshot exclusion. Native tests exercise lifecycle policies without real secrets.
- Planner owns `desktop/src/**`, documentation, workspace metadata and Windows build verification. Integrate native contracts without redesigning the visual style. Test the real release and independent KeePassXC interchange.
- Fresh reviewer is read-only. Input is this plan, the before/after diff and test commands, not worker reasoning.

Implementation and review use the host's default agent model and effort. This harness does not expose the routing presets or effort controls, and its tool policy forbids selecting models without an explicit user request. No substitute model IDs are guessed.

## Fixed core contracts

Existing CRUD returns are unchanged. `ApiKey.id` becomes the native KDBX UUID.

```rust
Vault::needs_upgrade(&self) -> bool
Vault::upgrade(&mut self) -> Result<()>
Vault::reload(&mut self) -> Result<()>
Vault::take_warnings(&mut self) -> Vec<String>
Vault::env_vars(&self, project: Option<&str>, environment: Option<&str>, all: bool)
    -> Result<Vec<(String, String)>>
Vault::preview_import_env(&self, entries: &[(String, String)],
    project: Option<&str>, environment: Option<&str>) -> Result<ImportPreview>
Vault::import_env(&mut self, entries: &[(String, String)],
    project: Option<&str>, environment: Option<&str>) -> Result<(usize, usize)>
```

`ImportPreview` is re-exported from akm-core and serializes with camelCase. It has `added`, `updated`, `unchanged` counts and `changes: Vec<ImportChange>`. Each change has `name`, `title`, and `action` (`add`, `update`, `unchanged`). No secret appears in the preview. Imports require non-empty project and environment and reject empty secrets, invalid/reserved names and duplicate names. Matching is exact on project, environment and variable name. Unchanged values are skipped. Export/injection requires both scope values or `all=true` without filters. Reserved process-control variables are rejected for injection/export.

`Error::UpgradeRequired` means an explicit upgrade is required before writing a 3.x/4.0 snapshot. No silent KDF changes. `save` and mutations do not change in-memory state on pre-commit failure. Post-commit audit failures become retrievable warnings, not false save failures. Writers verify decrypted temporary data, recheck source bytes immediately before replacement, and retain backups. Non-cooperating sync clients can still race; no impossible cross-process atomic compare-and-swap guarantee is claimed.

## Fixed desktop IPC contracts

- Status retains its fields and adds `needsUpgrade: boolean`.
- `vault_upgrade` and `vault_reload` return Status.
- `vault_warnings` returns `string[]` and drains core write warnings.
- `vault_activity` refreshes the backend's idle deadline; status polling does not.
- `vault-locked` event notifies the UI. The backend is authoritative; frontend timers are not the security boundary.
- Export commands add `all: boolean`. `env_save` opens its native save dialog in Rust, takes no path from JS, and returns `string | null` (saved path or cancel).
- `env_import_preview(text, project, environment)` returns the ImportPreview fields plus `token: string`. Rust retains the pending input; `env_import_confirm(token)` applies only that preview and returns `{added, updated}`. One pending preview per session. Lock/reload/upgrade invalidates it. Token is single-use and cannot apply to a different session.

## Verification policy

Public seams are Vault operations, the actual `akm.exe` process, Tauri command/lifecycle helpers, and the desktop UI. Regressions first reproduce the existing failures, then assert the corrected behavior. Only temporary test vaults and fake secrets are allowed.

1. `cargo test -p akm-core --offline`.
2. `cargo test -p akm-cli --offline` including subprocess tests.
3. `cargo test -p akm-desktop --offline`.
4. `cargo test --workspace --offline` and `cargo clippy --workspace --all-targets --offline -- -D warnings`.
5. `npm run build` in `desktop`.
6. `npm run tauri build` in `desktop`, then verify that exact rebuilt executable, not an older binary.
7. Real KeePassXC 2.8.0: read AKM vault, save with KeePassXC, explicit upgrade in AKM, save and read again. Attachments/history must survive.
8. Real Windows desktop checks: create/unlock, import preview scoped to dev versus prod, invalid/empty import refusal, clone/foreign entry targeting, CLI while GUI open, clipboard flags and close cleanup, forced lock, export selection, upgrade confirmation.

No repository, push, PR, release, signing enrollment or real-secret migration is authorized by this task. The source snapshot is in `%TEMP%/opencode/akm-phase1-before.zip`. GitHub/merge gates are not applicable to this non-git folder. A permanent verification skill is not created as part of these bug fixes; runnable tests and local proof scripts provide the verification instead.

Integration adds a short-lived journal lock as well as the vault write lock. A public AuditLog regression reproduced corrupted JSON from concurrent appends once sessions could coexist; appends now serialize the tail/read/write sequence and readers take a shared journal lock. This does not make the plaintext journal authenticated or safe for distributed writers.

The review score is not increased by projected points. Phase 1 remains an untrusted development build until independently checked. Windows Hello, updater/signing, full rotation workflow and broader phase-2 work remain separate gates.

## Verification completed before fresh review findings

- Full workspace: 93 tests passed, 0 failed. The subprocess fixture is intentionally invoked by its integration tests; the opt-in real-clipboard case was separately run and passed. Strict workspace Clippy and rustfmt checks passed.
- Frontend: 7 unit/component tests and 4 Edge browser scenarios passed. Browser scenarios mock native IPC and cover 390/1440 pixels in both themes; they are not native security proof.
- Release: the Windows executable, MSI and NSIS installer rebuilt successfully from the changed source. Installers remain unsigned and were built, not installed.
- `tools/verify_keepassxc.py` passed against real KeePassXC 2.8.0 and the rebuilt release CLI. It checks refusal before an explicit upgrade, retained original bytes, stable UUIDs, foreign-entry operations, attachments, existing history and unrelated entries.
- `tools/verify_windows_app.ps1` passed against the actual rebuilt release app. It creates fake keys only, restores the prior settings file, and closes only its own process. It proves a concurrent CLI write, refused stale GUI mutation with no phantom entry, reload, secret-free import review with no disk change, dev/prod isolation after confirmation, clipboard history/cloud flag values, monitoring exclusion, lock/suspend message handling, clipboard cleanup on lock/suspend/normal exit, and Windows display affinity 0x11.
- The lock/suspend proof sends synthetic Windows notifications to the notification window belonging to the test process. It does not lock or suspend the user's computer. The 10-minute idle deadline is tested with the native fake-clock policy, not a ten-minute wall-clock wait.
- Two initial GUI-proof attempts failed because UI Automation splits paragraph text into separate names. The proof's name aggregation was corrected, then the complete proof passed; no product bypass was added.
- House layout checks pass for the modified Settings screen and task graph in light and dark themes. The existing Keys screen still produces checker findings for unequal ledger/stat/filter widths, its empty action-heading grid track and the visually hidden search label. These are recorded visual/accessibility debt, not hidden behind a claimed all-screen pass. Both browser widths have no horizontal page overflow.
- Production npm audit reported no known vulnerabilities. This is not a Rust dependency audit or an external security certification.

The source folder is not a git repository. CI was configured but has not run remotely, and nothing was pushed or published.

## Review gate failed. Correction pass

The first fresh-context review rejected phase 1 with six supported findings. Its scratch reproductions are in `%TEMP%/opencode/akm-phase1-review/`. Prior passing checks do not close this gate.

Goal: preserve truthful committed outcomes and enforce lock, preview and export ownership across asynchronous races.

- Planner owns `desktop/src/**`, `desktop/src-tauri/src/{session,commands,private_export,error}.rs`, verification tools and documentation. Convert the review's deferred-response and native cancellation probes into permanent regressions. Separate completed mutations from authorization to disclose results; reject every frontend response overtaken by a lock; bind previews to immutable inputs; keep blocking file I/O outside the security gate.
- Clipboard worker owns only `desktop/src-tauri/src/clipboard.rs`. Publish text and exclusion formats and capture the Windows sequence while the clipboard remains open. Prove a newer identical copy is not claimed or cleared and test partial publication failures. Keep `ClipboardBackend`, `CopyAttempt`, and lease callers compatible.
- CLI worker owns only `crates/akm-cli/src/export.rs` and export regressions in `crates/akm-cli/tests/security.rs`. Fail closed on non-persistent-ACL storage, clean up owned failed exports and publish without overwriting. Keep `destination`, `write_new`, and `write` signatures unchanged. Do not change dependencies or root lock files.
- One new fresh-context reviewer/collector receives the plan, combined diff and verification commands, not worker reasoning. Existing failed review evidence remains available. No new model or effort is selected; the host defaults apply for the same harness-policy reason as above. The two independent Windows API units warrant workers because they need substantial implementation/test output; the planner keeps the coupled frontend/session fixes in context.

Correction contracts:

- A successful mutation whose result is suppressed by a newer lock returns a secret-free `completedWhileLocked` outcome with an explicit instruction not to repeat it, never plain `locked`. This distinction survives command postchecks, imports, upgrades, vault creation and export publication. Reads still suppress results after locking. The frontend shows this receipt without reopening protected views.
- A frontend lock/session epoch guards unlock/create, reload/upgrade, polling and other asynchronous UI results. Input/request generations independently invalidate import/export previews on edits, cancellation, hide and unmount. The review label comes from the same immutable scope snapshot as its token.
- Native export preparation, writing, flush/sync and verification run without the security gate. Authorization immediately before publication is the cancellation boundary. A lock before authorization cancels and cleans the temp; a lock after authorization may overlap the already-authorized OS publication and must preserve a truthful completed receipt. No impossible cancellation of an in-flight filesystem call is claimed.

Acceptance: reproduce before fixing; retain the original core/CLI/IPC contracts except the explicit completed outcome above; all six findings have permanent regression coverage; all workspace/frontend checks pass; rebuild and rerun exact-release Windows/KeePassXC proofs. Extend safe GUI coverage for invalid/empty imports, clone targeting, native save-dialog cancellation/save and explicit upgrade confirmation. Actual OS lock/suspend delivery cannot be inferred from synthetic notifications and remains a separately disclosed gap unless safely verified with the owner's participation.

Verification uses the commands above plus `npm test`, `npm run test:ui`, the opt-in fake-value real-clipboard test and `cargo fmt --all -- --check`. Work proceeds locally without git, commits, installation, publication, real secrets or a new verification skill. Above-90 readiness is still not claimed.

Correction verification so far:

- `cargo test --workspace --offline --locked`: 117 passed, 0 failed; the subprocess fixture and real-clipboard case remain separately gated. Strict workspace Clippy and rustfmt passed.
- Frontend: 19 tests passed, including the review's stale responses, preview edits, cancelled requests and hidden-window responses. All 4 Edge browser scenarios passed after rebuilding. Settings layout checks pass in both themes; previously recorded Keys findings remain.
- Release GUI/MSI/NSIS and CLI rebuilt. Real KeePassXC interchange passed again with release CLI SHA-256 `788d26afa834697e4183476a504b1f8159826b77ac065d39a281fbc7c5b2a154`.
- The corrected real-clipboard test refused to overwrite an existing non-empty clipboard before making any change. An isolated window-station attempt was denied by Windows; no elevation or interactive-clipboard clearing was attempted. `verify_windows_app.ps1 -SkipClipboard` explicitly preserves this verification gap while exercising other native flows. The default proof now also refuses a non-empty clipboard. Clipboard publication/cleanup has deterministic fault/interleaving coverage, but the changed native transaction still needs a successful real-clipboard runtime check.
- Expanded exact-release GUI proof and the new independent review remain in progress. A passing source/test review is not a substitute for the blocked clipboard runtime proof or actual OS lock/suspend delivery.

## Correction review also failed. Remaining work

The second fresh review found five additional defects: desktop export retained pathname rather than handle ownership; failed opening emitted a lock event that hid its own error; an overtaken rejected bootstrap request left no retry; hide did not invalidate pending import reviews; CLI documentation claimed unsupported Unix export permissions. Its reproducible evidence is in `%TEMP%/opencode/akm-phase1-correction-review/`.

After the owner cleared the clipboard and requested rechecking, the real clipboard test reached publication and failed: sequence 2350 captured while open became 2353 after return. The native transaction must account for Windows' own post-close behavior without claiming another application's copy. The test failure is a real acceptance blocker, not a reason to weaken the assertion or adopt a later sequence blindly.

Second correction ownership:

- Clipboard worker owns `desktop/src-tauri/src/clipboard.rs` and only any required Windows feature addition in its `Cargo.toml`. Investigate Windows synthesis/owner lifetime, make the captured lease valid after close, preserve atomic ownership and newer-copy protection, and extend the shared-transaction regressions. No interactive clipboard access by the worker; the planner drives the owner-approved real check.
- Export worker owns only `desktop/src-tauri/src/private_export.rs`. Retain and publish/delete the owned file by handle, preserving existing ACL/path restrictions and cancellation semantics. A renamed-temp/replaced-name regression must prove no unrelated file deletion or replacement publication.
- Planner owns frontend, `session.rs`, verification tools and docs. Fix terminal errors and bootstrap retry without accepting stale successes; invalidate pending import reviews on hide but preserve already-dispatched confirmation acknowledgments; correct Windows-only CLI docs. Complete the native proof with exact filename-control read-back before Save.
- Settings remain host defaults. The harness does not permit selecting a stronger unrequested model or routing preset; narrower runtime evidence and fresh bounded units replace guessed model escalation. A fresh combined review follows the corrections, not the previous failed verdict.

The expanded release GUI proof has now passed invalid/empty/duplicate import rejection, scoped reviewed import, native clone editing with original isolation, GUI read-only upgrade gating, cancellation, explicit 4.1 confirmation and backup preservation. Native save-dialog automation remains under repair; a failed harness run created only its known fake export in the dialog's default folder, which the planner verified and removed. Current assertions require the exact test destination, not merely any saved message. No real secret or vault was used.

The review follow-up also reproduced a cross-session receipt defect: an old session A completion hid active B without locking B natively, stranding subsequent unlocks. Receipts now carry their originating frontend generation. Old receipts retain their secret-free acknowledgment but cannot invalidate a newer session. An actual App/API-wrapper regression reproduced the failure and now passes.

The revised clipboard transaction explicitly materializes Windows' Unicode, ANSI, OEM and locale formats before capturing the sequence. This removes post-close synthesis without predicting increments or sampling a newer copy. After the owner's explicit empty-clipboard recheck, the real Windows test **passed**, including post-close/owner-destruction sequence validity, all private flags and owned cleanup. Its empty precondition is now inside the same open interval, so an intervening user copy is refused. This closes the previously blocked standalone real-clipboard check for this candidate; the rebuilt GUI proof still needs to run.

Native export ownership and all five additional review findings are implemented with permanent regressions. Targeted frontend checks now include startup retry, terminal unlock errors, hidden import review invalidation, dispatched-confirmation acknowledgments and cross-session receipts. Final workspace checks, release rebuild, expanded native proof and a fresh combined review remain required before acceptance.

Current acceptance candidate: 132 workspace tests passed, 24 frontend tests and 4 browser scenarios passed, strict workspace Clippy and rustfmt passed. The opt-in real Windows clipboard test passed separately. The release rebuild and exact-release expanded GUI proof are in progress; a new fresh-context review checks this combined candidate rather than reusing the failed verdict. Existing release hashes above describe the prior build and must not be substituted for the new rebuilt candidate.

Server-restart recovery: completed checks/builds are retained and are not being rerun. The interrupted GUI proof stopped just after fake-vault creation; its process has exited. Its in-memory settings preimage was lost, and the current preference points to `%TEMP%/opencode/akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2/vault.kdbx`. That preference is preserved rather than overwritten with a guessed original; the owner can choose their intended vault on the unlock screen. No real vault is removed or restored by guess. New proof runs persist `recovery.json` and `settings-before.bin` before launching, so ownership and preference restoration survive interruption. Only the interrupted native proof is restarted, and the existing fresh review resumes from its saved probes.

## Final correction checkpoint, 30 October 2026

**Source corrections are approved within the reviewed phase-1 scope, and the remaining rebuilt native proof passed. This is not approval for real secrets or a new readiness score.** The original 40.5/100 (rounded to 41) baseline is not replaced by a projected score.

- The fresh acceptance review established no additional P0–P2 blocker, but reproduced one P3: an old lock reply could clear or replace a newer session's error. Success replies no longer clear errors, and failure replies are fenced by epoch and transition. Both new regressions failed first and then passed. The existing reviewer independently passed those two tests and approved the bounded correction. Its merged verdict is `%TEMP%/opencode/akm-phase1-acceptance-review/followup-review-result.md`; it does not claim to have driven the final GUI.
- Preserved evidence: 132 Rust workspace tests, strict Clippy/rustfmt, the previous complete 24-test frontend run, 4 browser scenarios, independent KeePassXC interchange and the repaired standalone real-Windows clipboard test. After the final frontend-only P3, the affected App/receipt run passed 10 tests, including the two new regressions. Unaffected full suites were not repeated merely because of the restart.
- `npm run tauri -- build -- --locked --offline` passed after the P3, including TypeScript checking, production assets, the release GUI and both installer builds. No Rust/CLI source changed after its passing proof, and the release CLI hash remains unchanged.
- The final GUI was exercised with `tools/verify_windows_app.ps1 -DesktopExe target/release/akm-desktop.exe -CliExe target/release/akm.exe -RemainingOnly`. This prepares only a fresh fake CLI fixture, opens it in the GUI and resumes the unfinished checks. It does **not** repeat or relabel the already-passed import/clone/upgrade probes from the preceding GUI build.
- The resumed native proof passed save-dialog cancellation, exact filename read-back before Save, the exact saved path and scoped file content, native clipboard history/cloud/monitor exclusions, clipboard cleanup on synthetic session lock and suspend, notification-window ownership, normal exit cleanup and display affinity `0x11`. Every clipboard publication was preceded by an empty-clipboard guard; no existing user copy was automatically cleared.
- Startup UI Automation succeeded on this run. Its harness and input desktops were both `Default` on `WinSta0`. The earlier missing-controls failure did not reproduce; its root cause is not established, and it is not asserted to be a product defect or a security fix.
- Cleanup check found zero AKM or matching WebView processes. The successful proof's fake workspace was removed. Settings match the durable recovery baseline byte-for-byte and still point to the known fake vault above. The unknown pre-restart original preference was not guessed or recovered.

Final rebuilt artifacts, SHA-256:

- `target/release/akm-desktop.exe`: `9539610849a4a43b868703c2bd5e1725219fc2718aa5b01bd816890ffa269085`
- `target/release/akm.exe`: `788d26afa834697e4183476a504b1f8159826b77ac065d39a281fbc7c5b2a154`
- `target/release/bundle/msi/AKM_0.1.0_x64_en-US.msi`: `4f8d7332d7543a315659222f4a3e32fb65781e162441da97a9581b983d84f27b`
- `target/release/bundle/nsis/AKM_0.1.0_x64-setup.exe`: `22b50086186f1f3181d7964d8de26549288e46d98a2054e1fbefc12a34c684f7`

Remaining gates: actual OS lock/suspend notification delivery with the owner's participation; a wall-clock idle check (the fake-clock policy is tested); later-phase rotation/history, private per-device journal, dotenv compatibility, Hello/DPAPI, publisher signing/updater and professional review. Synthetic messages prove handlers only. Installers are unsigned, built but not installed. No repository, commit, push, publication, real-secret migration, new numerical score or audited-integration claim is made.
