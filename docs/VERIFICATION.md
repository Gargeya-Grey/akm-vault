# Verification and evidence guide

This guide distinguishes recorded proof from commands to run after a relevant change. Do not repeat the old suites merely to populate a new context window.

## Durable phase-one evidence

- [Final collected record](history/phase1-final/evidence.json) retains the main session's result collection and artifact hashes without rewriting its provenance.
- [Source manifest](history/phase1-final/source-sha256.json) and [combined diff](history/phase1-final/changes.diff) pin that source checkpoint.
- [Historical pre-phase-one source archive](history/phase1-final/before.zip) preserves the original code snapshot. It is reference material, not the current working tree.
- [Archive manifest](history/manifest.json) records exact source paths, sizes and SHA-256 for the allowlisted copies. No live settings snapshot, fake vault, build binary or dependency directory is included.
- [Supplemental GUI provenance](history/phase1-supplement/manifest.json) preserves recovered earlier partial-run/build stdout. The earlier GUI hash is only a secondary conversation record and is not embedded in those logs; its exact binary-to-run binding is unavailable. The partial run failed after the earlier creation/import/clone/upgrade passes. It is not a final-GUI rerun or a complete proof.
- [PHASE1.md](../PHASE1.md) preserves contracts, failures, correction history and final native results. Its intermediate in-progress statements are superseded by its final checkpoint.

At the preserved phase-one checkpoint, application source matched the merged accepted phase-one source. Current unit-1a changes and their failed/corrected acceptance history are recorded below; do not equate today's tree with that earlier accepted binary.

## Calibration evidence

The [raw Terra report](reviews/2026-10-31-calibration/review-result.md), [provenance](reviews/2026-10-31-calibration/provenance.json), [original input checks](reviews/2026-10-31-calibration/input-and-validation.json) and [disposition](reviews/2026-10-31-calibration/disposition.md) are project-local. Twelve original reviewed input snapshots were frozen before disposition edits. The raw report SHA-256 is `069878cbca80ec8f976193e8d4d7210d4b9f63c46690ab903813a98cc3842666`.

This was one source/plan/context review with the caller requesting `openai/gpt-6.1-terra#max`. The reviewer saw the Terra model identity but not independent active-effort telemetry. It ran no runtime probe, build, suite, GUI, clipboard, settings or network operation. The main session's source inspection supports recording the new findings as open, not claiming them reproduced or repaired. Documentation dispositions after that frozen review were not independently re-reviewed.

## What passed and when

- Full Rust workspace, 132 passing tests, before the final frontend-only P3. Strict workspace Clippy and rustfmt passed.
- Full frontend, 24 passing tests, and 4 mocked-native Edge scenarios before the final P3.
- Affected App/receipt files, 10 passing tests after the P3, including the two new regressions. The independent reviewer separately passed those two. These are overlapping test sets, not 36 unique tests or a freshly run complete 26-test suite.
- Production frontend/type check, release GUI, MSI and NSIS builds after the P3.
- Real KeePassXC 2.8.0 interchange with the unchanged release CLI. Explicit upgrades, stable UUIDs, foreign entries, attachments, existing history and unrelated entries survived.
- Standalone actual Windows clipboard privacy formats, post-close sequence validity and owned cleanup passed after the owner manually cleared the clipboard.
- The final GUI's `-RemainingOnly` native proof passed exact-path save/cancel/content, clipboard exclusions, synthetic session/power cleanup, exit and capture affinity. Earlier GUI creation/import/clone/upgrade proofs were preserved from the prior build.

## Coverage not established

Actual OS lock/suspend delivery, a ten-minute wall-clock idle wait, installed/upgrade/uninstall behavior, remote CI execution, publisher signing, updater security, broad dotenv compatibility, a full post-phase-one rubric rescore and professional audit are not established. Repaired code may be accepted within a narrow review without closing these gates.

## Core commands

Run from the repository root unless noted. Prefer the affected checks while editing; run the full relevant policy once for a stable cross-cutting candidate.

```powershell
cargo test --workspace --offline --locked
cargo clippy --workspace --all-targets --offline --locked -- -D warnings
cargo fmt --all -- --check
cargo build --release -p akm-cli --offline --locked
```

Run in `desktop`:

```powershell
npm test
npm run build
npm run test:ui
npm run tauri -- build -- --locked --offline
```

Independent reader and full native proof from the root:

```powershell
python tools/verify_keepassxc.py --akm target/release/akm.exe --keepassxc "C:/path/to/keepassxc-cli.exe"
./tools/verify_windows_app.ps1 -DesktopExe target/release/akm-desktop.exe -CliExe target/release/akm.exe -KeePassXC "C:/path/to/keepassxc-cli.exe"
```

The native proof refuses another AKM process and nonempty clipboard. `-SkipClipboard` preserves user content and explicitly leaves that proof gap open. `-RemainingOnly` resumes only the named unfinished checks when prior evidence is still applicable; it is not a full-proof alias.

The standalone clipboard test is opt-in. Only the main session, with the owner's participation and the test's empty-clipboard guard, may run it:

```powershell
$env:AKM_TEST_REAL_CLIPBOARD = '1'
cargo test -p akm-desktop --lib --offline --locked clipboard::windows::actual_windows_private_formats_are_present -- --ignored --exact
```

Restore any environment variable owned by a proof. Do not clear the clipboard on behalf of the owner or escalate access to an alternate window station.

## Unit 1a correction evidence

The [unit evidence](work/phase2-1a/evidence.md) preserves the rejected first candidate and [raw failed review](work/phase2-1a/reviews/round1/manifest.json). The [first correction evidence](work/phase2-1a/corrections/evidence.md) records Rust153/frontend43/browser10, explicit exits, hashes, exact-GUI proofs and KeePassXC on the subsequently rejected round-two source. Its failed export startup-discovery run remains beside the root-refresh recovery; do not report an all-first-try native pass. The [second correction evidence](work/phase2-1a/corrections2/evidence.md) records the six newer findings and affected checks. No earlier pass substitutes for the current candidate's acceptance. Clipboard was not accessed. Native late-lock/sync faults are injected, and browser reload has mocked IPC. No numerical readiness score is implied.

Round-three bounded acceptance also [rejected that candidate](work/phase2-1a/reviews/round3/review-result.md), with three supported contract findings. Its157/55/10/lint/build/native/KeePassXC results remain valid for their checked scenarios but do not override rejection. The owner subsequently authorized one renewed cycle under the [recalibrated plan](work/phase2-1a/corrections3/PLAN.md); new [evidence](work/phase2-1a/corrections3/evidence.md) is separate from the earlier failed acceptance.

For focused fake-data native flows, use `tools/verify_windows_app.ps1` with `-JournalOnly`, `-JournalExportOnly` or `-OutcomeOnly`. These skip clipboard entirely. OutcomeOnly additionally requires owner readiness confirmation and `-OwnerReadyForVisibility`; it delays replies using its own fake-journal lock and minimizes/restores only the owned AKM window. Failure to demonstrate a real hide is a proof gap, not a synthetic pass. They are scoped proofs, not aliases for full native/OS delivery or installation coverage.

The renewed cycle was [rejected in round four](work/phase2-1a/reviews/round4/review-result.md). Its163 unique Rust passes (timeout plus exact-workspace selective resume),105 frontend,18 mocked browser/lint/builds and exact-GUI journal/export results are preserved in [cycle evidence](work/phase2-1a/corrections3/evidence.md). Two native minimize attempts failed before handoff assertions; actual delayed outcome handoff is not proven. Independent fake CLI/native-backend/mocked-event probes found two P2 and one P3 defects. The owner subsequently authorized [corrections4](work/phase2-1a/corrections4/PLAN.md); its new checks and rebuilds are separate evidence. Do not repeat completed checks solely for context, or run any new actual minimize attempt without readiness confirmation.

Corrections4 now records184 Rust/2 gated ignored,161 frontend, strict lint/format/types/builds and20 unique browser scenarios (18 plus2 selective after an archived fixture-assertion correction), with current compiled-input fingerprints verified. New CLI/GUI/MSI/NSIS are hashed; current-CLI KeePassXC and exact-GUI backup/journal/export proofs passed with settings restored and no remaining owned process observed. [Current evidence](work/phase2-1a/corrections4/evidence.md) retains raw failures, startup root-discovery recovery and layout exit1. Actual native outcome hide/navigation proof and fresh scoped acceptance are still pending; these passes do not close R4 or establish a numerical score.

The owner-ready corrections4 outcome attempt then confirmed actual native minimization but failed its pre-release import-dialog-expiry predicate. It stopped before navigation/file-save/cancel/error assertions. Post-release completed acknowledgment and exact1 scoped fake import are recorded, not receipt loss or a hide-contract pass. Raw diagnostics/recovery remain archived and in the retained fake directory; settings/artifacts restored, no matching process observed. Do not retry, change visibility mechanisms or dispatch acceptance automatically; obtain the owner's decision under STATE.md.

The owner's subsequent safe-correction approval authorizes the bounded [visibility/invocation continuation](work/phase2-1a/corrections4/visibility/evidence.md), not another actual minimize without renewed readiness. Generated MockRuntime IPC and DOM-visible native-expiry callers now have separately preserved red/green evidence; the changed GUI invalidates older exact-GUI proofs. [Publication authority clarification](work/phase2-1a/corrections4/visibility/PUBLICATION_CONTRACT.md) retains original ticket/revision filesystem policy. Retain CLI/KeePassXC only through unchanged input/artifact fingerprints; new candidate-aware proof names cannot overwrite the old failed identities.

Visibility-1 stable collection:196 Rust/2 gated ignored plus9 standalone IPC cases, strict lint/format,190 unique frontend cases (181+9 selective after preserved worker-startup error, not one full green), production/e2e types and32 browser cases. GUI/MSI/NSIS rebuilt unsigned; CLI/reader retained through exact unchanged inputs/hash. New exact-GUI backup/journal/export proofs pass onbc393ed9, with both startup-root refreshes retained. Settings restored exactly,141 inputs/4 artifacts unchanged, no matching process observed; no clipboard/minimize/OS signals. [Collection2](work/phase2-1a/corrections4/visibility/verification-record-2.json) scopes the31 raw records. Actual outcome hide/navigation and one fresh acceptance remain open; readiness must be renewed before native minimization.

## Evidence invalidation rules

- Core data/persistence changes require the affected core regressions, actual CLI callers, independent-reader proof and relevant GUI flows. Preserve KDF/cipher/history/attachment checks.
- Journal changes require multi-session append/read/failure tests plus CLI and Activity callers. A journal warning must never become a false unsaved vault error.
- Session/clipboard/export changes require deterministic interleavings and real Windows evidence. Mocked browser tests cannot establish native clipboard or OS behavior.
- Shared DTO changes require TypeScript checks and the actual frontend/native boundary, not only one Rust unit test.
- Parser changes require a specified external dialect and cross-reader fixtures, not only self-round-trips.
- Frontend changes require affected lifecycle/interaction tests, a rebuilt GUI and the affected native flow. Scope retained historical evidence explicitly.
- Documentation/context-only changes require link, manifest, provenance and consistency checks. They do not call for a fresh Rust build or clipboard test.

## Recovery and recording

Before a GUI launch, record the exact executable hash, settings preimage, owned paths, process creation identity and cleanup plan outside process memory. Treat a surviving PID as untrusted until identity is checked. Restore settings only while the proof still owns their value. Never guess an unknown preference.

For each new result retain the command, working directory, source/artifact identity, test mode, result, skips, cleanup status and whether it was independently run or reported by another session. Archive only allowlisted records. A hash proves byte identity, not security or correctness.
