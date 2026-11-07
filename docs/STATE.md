# Current state

Updated 2026-11-07. Replace this file's sections when they change. History lives in Git.

## Status

- **Phase two is built and in review.** Units 1c, 1h, 1b, 2, 3 and 4, security proposals A1 to A3, and the tools are on branch `phase2/units-1c-to-4`, in [PR #7](https://github.com/Gargeya-Grey/akm-vault/pull/7). The owner authorized all of them on 2026-11-07 and let the implementer decide the design questions; each decision is under "Decisions for the owner's veto" in `docs/design/unit*.md`.
- **Reviews: two at most for phase two** (owner, 2026-11-07), by Codex GPT-6.1 Terra at max effort. [Review 1](reviews/2026-11-07-phase2-review1.md) failed with 8 P2 and 1 P3 findings. All 9 are fixed by fault class (FINDINGS.md rows P2R1-01 to P2R1-10; the class table is in AGENTS.md); each fix has a test that failed first. Review 2, the last one, runs on `3391b94`. The first attempt was refused by the provider's safety filter before it produced a review; the second attempt is running.
- **GitHub Actions is on** (owner, 2026-11-07). The first CI run failed one test that depended on the temp path (a short name such as `RUNNER~1`); `d628130` fixes the test and the product message.
- **The owner will use AKM for their own keys before the release gate.** [FIRST_USE.md](FIRST_USE.md) gives the precautions and the six tests that need the owner present.
- **Unit 1a: source accepted; work unit complete: NO.** R4-04 (open P1) is still deferred.
- **No new score exists.** The only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)). The full rescore of unit 5 needs several reviewers and waits for the owner.

## Last full check run

On `8548191` (2026-11-07, fake data only, Rust builds outside the repository). The later commits change the CI workflow, docs, and one CLI export message with its test.

- Rust: `cargo fmt --check` and Clippy (`-D warnings`) clean. `cargo test --workspace --offline --locked` 454 passed, 0 failed, 2 ignored (the opt-in real-clipboard test and one gated test).
- Frontend: `npm ci`, `npm test` (305 passed in 28 files), the production build, and `npm run test:ui` (51 passed, mocked native boundary).
- Build: `npm run tauri -- build -- --locked --offline` built the GUI and the unsigned MSI and NSIS installers. None is installed.
- Security checks: `check_no_network.py` PASS and its test; `check_dependencies.ps1` PASS, with the two Linux-only `cargo audit` warnings of SEC-01.
- Real readers: KeePassXC 2.8.0 interchange PASS (rotation, Recycle Bin, restore, purge, stale-copy merges); kdbxweb all checks PASS, 0 FAIL.
- Tools: `check_docs.py` PASS and its test; `test_fetch_keepassxc.ps1` 8 of 8; `test_proof_snapshot.ps1` all cases.
- Native GUI proofs on these builds: `-JournalOnly` and `-JournalExportOnly` passed to their last check (each script stops at its first failure). No clipboard access, no minimize. The settings file was byte-identical afterwards, and no `akm-desktop` process remained.
- After `d628130`: `cargo test -p akm-cli --test security` 48 passed, 1 ignored; Clippy on `akm-cli` clean.
- Not run: R4-04, the real clipboard test, the full native GUI proof, the native import dialog, the Windows toast and a real window hide. They need the owner present ([FIRST_USE.md](FIRST_USE.md)).

## Builds under test

Builds of `8548191` in the session build folder, all unsigned and not installed, SHA-256:

- `akm-desktop.exe` `be200c0c7b9c124b74915fefc3566a537395753196cac3b908062b1f456421f4`
- `akm.exe` `6fc47991109e603485034077051265e7299104d8d197bf679169f58211a8d21f` (before the `d628130` message fix)
- `AKM_0.1.0_x64_en-US.msi` `ea5a5c7bcfbc0686dbd6ca3a248b435e832ffce960de0d7f6515cf66c0bb7a1d`
- `AKM_0.1.0_x64-setup.exe` `bb2b8341660a753ab3a2618ea0f240fcf9a73faa964df8c712c46a5dd1edc0a0`

The CI run of PR #7 uploads its own build with a `SHA256SUMS.txt`. Build from the merged `main` before you install, and compare the hashes with that run.

## Stop rule: design questions for the owner

[Review 2](reviews/2026-11-07-phase2-review2.md) failed with 4 P2 and 1 P3 (FINDINGS.md rows P2R2-01 to P2R2-05). Units 1c, 1b and the tools failed both phase two reviews, so patching stopped (AGENTS.md). In each class that came back, the earlier fix kept a premise that the review broke:

| Finding | Premise that failed | Proposed redesign |
|---|---|---|
| P2R2-01 fetch tool deletes nested files (F) | The tool can tell its own files from others, so it may delete. | The tool never deletes. Each fetch goes to a new folder named by version and digest. |
| P2R2-02 hidden import still commits (A) | A hide that the page sees must cancel an approval that native code gives. | The native dialog is the review: an approval in it stands, a lock still refuses it. Contract 7 changes its wording for the desktop import. |
| P2R2-03 delayed discard reaches session B (B) | A discard without an identity reaches only the current session. | `view_discard` carries its session ID; a failed discard locks only that session. |
| P2R2-04 clone replacement mixes journals (B) | The root UUID names one vault. | A fingerprint that this device has seen belongs to the journal that saw it; only an unseen one falls back to the root UUID. |

## Next action

1. The owner answers the design questions above.
2. The owner merges PR #7 after CI is green (owner gate).
3. The owner runs the six tests in [FIRST_USE.md](FIRST_USE.md), and chooses option A or B for R4-04.
4. Release decisions for the owner: SEC-02 to SEC-05 and 2-W-01 in [FINDINGS.md](FINDINGS.md).

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps the tracked evidence that the cleanup removed. Git tag `baseline/pre-unit-1a` holds the source snapshot from before unit 1a.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the ignored local files that the cleanup moved out. `W:\AKM-archive\2026-11-07-leaked-test-journal\` keeps the 1288 fake journals that tests wrote to the real journal folder before the guard (1B-C-01).
- `W:\AKM-archive\proofs\` keeps the verification skill's transcripts.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx` (SHA-256 of the settings file `dacba49a…9115`). The owner's earlier setting is unknown. When the owner opens their own vault, the app replaces this setting.
- The old builds in `target\debug` and `target\release` were deleted on 2026-11-07 to free disk space; the unit 1a hashes are in Git history.
