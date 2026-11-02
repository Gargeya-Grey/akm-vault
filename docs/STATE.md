# Current state

Updated 2026-11-02. Replace this file's sections when they change. History lives in Git.

## Status

- **Phase one is complete.** Its review rounds accepted the fixes for the original critical and high findings. H9 (rotation) is still open, and H6 is partial: real Windows lock and suspend delivery is not tested.
- **Unit 1a: source accepted; work unit complete: NO.** [Acceptance review 2](reviews/2026-11-02-unit1a-acceptance-review2.md) passed under the [pass rule](REVIEWING.md#pass-rule), after the owner's scope split of 2026-11-02 (option A): 1A-A2-01 moved to unit 1c, 1A-A2-02 and A2-03 to unit 1h, 1A-A2-04 to unit 1b, and A2-05 was fixed. The unit is complete only when R4-04 passes. The owner-present real clipboard test and the full native GUI copy proof have not run.
- **Unit 1a is on `main`.** PRs #3, #4 and #5 landed together through one merge of #5 (`60a7347`). PRs #1 and #2 landed earlier (`d46bcce`, `4656b0c`). No pull request is open, and `main` is the only branch.
- **The pass rule is in force** (owner decision, 2026-11-02). Every review from now on is judged by it.
- **No new score exists.** The only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)).

## Last full check run

This run checked branch `fix/unit1a-sound-findings` at `94d8008` (unit 1a, round A1 fix) on 2026-11-02, in a separate check session. All data was fake.

- Rust: `cargo fmt --all -- --check` and Clippy (`-D warnings`) were clean. `cargo test --workspace --offline --locked` passed 230 tests, with 2 gated tests ignored. The release CLI was rebuilt with `--locked` after the GUI build.
- Frontend: `npm ci` passed. `npm test` passed 192 cases in 16 files. The production build passed. `npm run test:ui` passed 32 mocked-native cases.
- Build: `npm run tauri -- build -- --locked --offline` built the GUI and the unsigned MSI and NSIS installers. None is installed.
- Final-build checks, between two hash runs that matched:
  - Release CLI, fake vaults (`.scratch` scripts, not committed): the earlier script passed 39 of 39, and the unit 1a script passed 49 of 49.
  - Native GUI proof `-JournalOnly` passed 27 checks.
  - Native GUI proof `-JournalExportOnly` passed 9 checks on its fourth run. Runs 1 to 3 failed in the filter step of the proof script: twice `Missing select option in the test process: Environment / prod` (`tools/verify_windows_app.ps1:303`), once `Unsupported Pattern` at the Project filter's Expand call (line 270). The cause is timing in the proof script, not AKM: the same binary then passed; the frontend bundle is byte-identical to the one built from `2664e51`; and a build of `2664e51` passed the same proof in the same conditions. `Select-FilterOnce` calls Expand once, with no wait or retry when UI Automation does not yet expose the combo box pattern or its options. Making that step robust is open tool work (TESTING.md).
  - Both proofs used fake vaults, did not touch the clipboard and did not minimize or hide the window. The settings file was byte-identical before and after (SHA-256 `dacba49a…9115`), and no `akm-desktop` process remained.
- Round A1 tests (`836fc8c`): the four text tests and the copy table failed on the old production code and pass at `94d8008`. Mutants (`.scratch/mutants_a1.py`): 5 of 5 caught.
- Not run: KeePassXC interchange (no KDBX format change; the portable KeePassXC path was not found), the full native GUI proof and the real clipboard test (they use the clipboard and need the owner present), `-OutcomeOnly` (owner gate), and R4-04 (deferred).
- CI: GitHub Actions is off, so no CI ran (`CI RESULT: none`).

## Unit 1a build under test

- Source: `94d8008`, now in `main` through the merge `60a7347`. The branch is deleted. `main` also has the UI change of PR #2, so a build of `main` will not match these hashes: rebuild and run the checks again before a proof on `main`.
- Builds in `target/release`, all unsigned and not installed, SHA-256:
  - `akm-desktop.exe` `32f591a7d197ed93e7aba57d6c7a3ff8548ce44172a3b27f183222c0578c9104`
  - `akm.exe` `ad2ec01b491de883493b12d0aad802084ad8831b45083083a8e77ef8c6ceafc1`
  - `bundle/msi/AKM_0.1.0_x64_en-US.msi` `d7c457b92d98e730db56e211eee6ebf716aee157f95fc20514d13a26427c852c`
  - `bundle/nsis/AKM_0.1.0_x64-setup.exe` `05f8124b441f96c2c086151961e9487fd40b48c946b25b3bee7d8f97085ea3cf`
- The R4-04 pass conditions in FINDINGS.md still name `cfeb1b0`; update them to the accepted source before R4-04 runs.
- Before you use a build for a proof, check its hash against this list. A different hash needs a rebuild and a new check run.

## Next action

Nothing is in progress. Each item below waits for the owner. Each unit needs its own authorization ([ROADMAP.md](ROADMAP.md)).

1. **R4-04 (open P1, deferred by the owner on 2026-11-01).** The proof script failed review twice (R4-04-SCRIPT in [FINDINGS.md](FINDINGS.md)). The owner chooses: **A**, a fake-data-only proof log in the app, which needs a rebuild and must stay off in release builds; or **B**, a stronger script that watches UI Automation events, with its two gaps as written limits. The run needs the owner present. Before it runs, update the R4-04 pass conditions from `cfeb1b0` to the build of `main`. R4-04 must pass before unit 1a is complete and before the release gate.
2. **Units 1b, 1c and 1h** are not authorized. Their findings are in [FINDINGS.md](FINDINGS.md). Unit 1b now also owns the cause of the `-JournalExportOnly` proof-script failures (1A-A2-04).
3. **Security proposals** (owner request, 2026-11-02) are in [ROADMAP.md](ROADMAP.md#security-proposals-not-authorized). None is authorized. The suggested first two, a threat model and a no-network check, add no product code. Group D adds agent-safety ideas from existing free tools (kdbx, kprun, fnox, secretspec, dotenvx).
4. **Owner-present tests:** the real clipboard test and the full native GUI copy proof for unit 1a.

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps all the tracked evidence that the cleanup removed: phase-one records, the review rounds, probes and hash manifests.
- Git tag `baseline/pre-unit-1a` is a commit with no parent. It holds the 139-file source snapshot from before unit 1a, rebuilt from `before.zip`, and each file matches the archived `before-sha256.json`.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the 265 ignored local files that the cleanup moved out: raw logs, screenshots and source archives.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx`. An earlier restart lost the owner's previous setting, so it is unknown. Keep that folder while the app selects it.
- `%TEMP%\opencode\akm-app-proof-030c41dd-649e-44ae-8086-285677373566` holds the diagnostics of the failed R4-04 attempt.
