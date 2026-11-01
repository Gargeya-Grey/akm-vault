# Verification and evidence guide

This guide distinguishes recorded proof from commands to run after a relevant change. Do not repeat the old suites merely to populate a new context window.

## Durable phase-one evidence

- [Final collected record](history/phase1-final/evidence.json) retains the main session's result collection and artifact hashes without rewriting its provenance.
- [Source manifest](history/phase1-final/source-sha256.json) and [combined diff](history/phase1-final/changes.diff) pin that source checkpoint.
- [Historical pre-phase-one source archive](history/phase1-final/before.zip) preserves the original code snapshot. It is reference material, not the current working tree.
- [Archive manifest](history/manifest.json) records exact source paths, sizes and SHA-256 for the allowlisted copies. No live settings snapshot, fake vault, build binary or dependency directory is included.
- [PHASE1.md](../PHASE1.md) preserves contracts, failures, correction history and final native results. Its intermediate in-progress statements are superseded by its final checkpoint.

At this checkpoint, the current application source matches the merged accepted phase-one source. New context documents do not certify or change the application binaries.

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
