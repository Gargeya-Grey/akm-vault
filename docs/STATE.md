# Current state

Updated 2026-11-07. Replace this file's sections when they change. History lives in Git.

## Status

- **Phase two is built, and its two reviews are done.** Units 1c, 1h, 1b, 2, 3 and 4, security proposals A1 to A3, and the tools are on branch `phase2/units-1c-to-4`, in [PR #7](https://github.com/Gargeya-Grey/akm-vault/pull/7). The owner authorized all of them on 2026-11-07 and let the implementer decide the design questions; each decision is under "Decisions for the owner's veto" in `docs/design/unit*.md`.
- **Reviews: two at most for phase two** (owner, 2026-11-07), by Codex GPT-6.1 Terra at max effort. Both failed. [Review 1](reviews/2026-11-07-phase2-review1.md): 8 P2 and 1 P3, fixed by fault class (P2R1 rows). [Review 2](reviews/2026-11-07-phase2-review2.md): 4 P2 and 1 P3 (P2R2 rows). Units 1c, 1b and the tools failed twice, so patching stopped and the owner chose one redesign per finding, with no third review: the fetch tool never deletes; the native dialog is the review; every vault command names its session; a seen fingerprint wins. Each redesign has tests that failed first and caught mutants. No reviewer has read the redesigns, so every phase two fix stays "fixed, not accepted".
- **GitHub Actions is on** (owner, 2026-11-07). The first two CI runs failed tests that compared raw temp paths on the runner's `RUNNER~1` path (P2R2-06): `d628130` fixed the CLI test and its message, and CI now uses a temp path without short names.
- **The owner will use AKM for their own keys before the release gate.** [FIRST_USE.md](FIRST_USE.md) gives the precautions and the six tests that need the owner present.
- **Unit 1a: source accepted; work unit complete: NO.** R4-04 (open P1) is still deferred.
- **No new score exists.** The only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)). The full rescore of unit 5 needs several reviewers and waits for the owner.

## Last full check run

On `3d663e0`, the head after the review 2 redesigns (2026-11-07, fake data only, Rust builds outside the repository). Later commits change docs only.

- Rust: `cargo fmt --check` and Clippy (`-D warnings`) clean. `cargo test --workspace --offline --locked --no-fail-fast` 475 passed, 0 failed, 2 ignored (the opt-in real-clipboard test and one gated test).
- Frontend: `npm ci`, `npm test` (353 passed in 28 files), the production build, and `npm run test:ui` (51 passed, mocked native boundary).
- Build: `npm run tauri -- build -- --locked --offline` built the GUI and the unsigned MSI and NSIS installers. None is installed.
- Security checks: `check_no_network.py` PASS and its test; `check_dependencies.ps1` PASS, with the two Linux-only `cargo audit` warnings of SEC-01.
- Real readers: KeePassXC 2.8.0 interchange PASS (rotation, Recycle Bin, restore, purge, stale-copy merges); kdbxweb all checks PASS, 0 FAIL.
- Tools: `check_docs.py` PASS and its test; `test_fetch_keepassxc.ps1` 9 of 9; `test_proof_snapshot.ps1` all cases.
- Native GUI proofs on these builds, after the verification skill's doctor check: `-JournalOnly` 27 checks PASS, `-JournalExportOnly` 9 checks PASS. No clipboard access, no minimize. The settings file was byte-identical afterwards (`dacba49a…9115`), and no `akm-desktop` process remained.
- Not run: R4-04, the real clipboard test, the full native GUI proof, the native import dialog, the Windows toast and a real window hide. They need the owner present ([FIRST_USE.md](FIRST_USE.md)).

## Builds under test

Builds of `3d663e0`, all unsigned and not installed, SHA-256. The session build folder that held them is deleted; rebuild with the commands in [TESTING.md](TESTING.md) to compare.

- `akm-desktop.exe` `bb551c2facc7610a67a6c5a315b1dc102c43802c1a34244d51246a79685135dd`
- `akm.exe` `323ea48c98715bd8b556071cc13b1afe87559b83ae4b73bf66dfdb9e3dd2586f`
- `AKM_0.1.0_x64_en-US.msi` `c312c6341c571812128fdaa0a072c9258ee0d8a65da67dd35034a9be74d86cec`
- `AKM_0.1.0_x64-setup.exe` `a3dab48bc3aa5ed73b8e034782f6d7356c44faac4e496b4c86e79aada6f3f5b7`

The CI run of PR #7 uploads its own build with a `SHA256SUMS.txt`. Build from the merged `main` before you install, and compare the hashes with that run.

## Stop rule: the owner's decisions (2026-11-07)

[Review 2](reviews/2026-11-07-phase2-review2.md) failed with 4 P2 and 1 P3 (FINDINGS.md rows P2R2-01 to P2R2-05). Units 1c, 1b and the tools failed both phase two reviews, so patching stopped (AGENTS.md). In each class that came back, the earlier fix kept a premise that the review broke. The owner chose each proposed redesign, and one round without a third review:

| Finding | Premise that failed | Proposed redesign |
|---|---|---|
| P2R2-01 fetch tool deletes nested files (F) | The tool can tell its own files from others, so it may delete. | The tool never deletes. Each fetch goes to a new folder named by version and digest. |
| P2R2-02 hidden import still commits (A) | A hide that the page sees must cancel an approval that native code gives. | The native dialog is the review: an approval in it stands, a lock still refuses it. Contract 7 changes its wording for the desktop import. |
| P2R2-03 delayed discard reaches session B (B) | A discard without an identity reaches only the current session. | `view_discard` carries its session ID; a failed discard locks only that session. |
| P2R2-04 clone replacement mixes journals (B) | The root UUID names one vault. | A fingerprint that this device has seen belongs to the journal that saw it; only an unseen one falls back to the root UUID. |

## Next action

1. The owner merges PR #7 after CI is green (owner gate).
2. The owner runs the six tests in [FIRST_USE.md](FIRST_USE.md), and chooses option A or B for R4-04. `tools/verify_windows_app.ps1` still follows the import design from before P2R2-02; update its import steps before the full GUI proof.
3. Release decisions for the owner: SEC-02 to SEC-05, 2-W-01 and P2R2-07 in [FINDINGS.md](FINDINGS.md).
4. Later units: P2R2-06 (tests that compare raw temp paths) and the unit 5 full rescore, which the release gate needs.

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps the tracked evidence that the cleanup removed. Git tag `baseline/pre-unit-1a` holds the source snapshot from before unit 1a.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the ignored local files that the cleanup moved out. `W:\AKM-archive\2026-11-07-leaked-test-journal\` keeps the 1288 fake journals that tests wrote to the real journal folder before the guard (1B-C-01).
- `W:\AKM-archive\proofs\` keeps the verification skill's transcripts.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx` (SHA-256 of the settings file `dacba49a…9115`). The owner's earlier setting is unknown. When the owner opens their own vault, the app replaces this setting.
- The old builds in `target\debug` and `target\release` were deleted on 2026-11-07 to free disk space; the unit 1a hashes are in Git history.
