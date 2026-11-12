# Current state

Updated 2026-11-13. Replace this file's sections when they change. History lives in Git.

## Status

- **Phase two is on `main`.** Units 1c, 1h, 1b, 2, 3 and 4, security proposals A1 to A3, and the tools landed through [PR #7](https://github.com/Gargeya-Grey/akm-vault/pull/7), merged on 2026-11-08 with the owner's yes, after green CI. The branch is deleted. The owner authorized all of them on 2026-11-07 and let the implementer decide the design questions; each decision is under "Decisions for the owner's veto" in `docs/design/unit*.md`.
- **Reviews: two at most for phase two** (owner, 2026-11-07), by Codex GPT-6.1 Terra at max effort. Both failed. [Review 1](reviews/2026-11-07-phase2-review1.md): 8 P2 and 1 P3, fixed by fault class (P2R1 rows). [Review 2](reviews/2026-11-07-phase2-review2.md): 4 P2 and 1 P3 (P2R2 rows). Units 1c, 1b and the tools failed twice, so patching stopped and the owner chose one redesign per finding, with no third review: the fetch tool never deletes; the native dialog is the review; every vault command names its session; a seen fingerprint wins. Each redesign has tests that failed first and caught mutants. No reviewer has read the redesigns, so every phase two fix stays "fixed, not accepted".
- **A run of the first-use commands found one defect** (2026-11-08). On the CI build of `ee7b353`, with a fake vault, `init`, `add`, `list`, `get`, `run`, `export`, `rotate`, `history`, `rm`, `bin`, `restore` and `audit --verify` worked. `--secret-stdin` kept a leading byte order mark as part of the secret (P2F-01). The fix has a test that failed first. No review read it. Evidence: `W:\AKM-archive\proofs\20261108T083940Z-first-use-commands`; in that transcript `rm` refuses without `--yes`, and `rm --yes`, `bin` and `restore` passed in a second fake-vault run that kept no transcript.
- **GitHub Actions is on** (owner, 2026-11-07). **CI is green on the PR #7 merge `ee7b353`** ([run 42453604620](https://github.com/Gargeya-Grey/akm-vault/actions/runs/42453604620)). **CI is green on `4f88b3a`** ([run 42402999565](https://github.com/Gargeya-Grey/akm-vault/actions/runs/42402999565), all 27 steps). A later docs-only run failed because the CI cache restored the fake test journal folder without its private permissions (P2R2-06); CI now removes that folder first, and the run on `0ac81a7` is green. The four earlier runs failed for four separate reasons, each fixed: tests that compared raw temp paths on the runner's `RUNNER~1` path (P2R2-06; `d628130`, and CI now uses a temp path without short names); the fetch tool missed the root of the current folder's drive (`7ec9166`); and a lint that only a newer Clippy has (`4f88b3a` pins CI to Rust 1.90.0, the local toolchain).
- **The owner will use AKM for their own keys before the release gate.** [FIRST_USE.md](FIRST_USE.md) gives the precautions and the six tests that need the owner present.
- **Unit 1a: source accepted; work unit complete: NO.** R4-04 (open P1) is still deferred.
- **UI rebuild in progress on branch `ui/key-cabinet`** (owner, 2026-11-08). Every screen is new; the design is in [desktop/DESIGN.md](../desktop/DESIGN.md). A new Use keys view holds .env export and import, which left Settings. The session logic and every accessible name that the proofs use are unchanged. One per-user NSIS setup program replaces the MSI and NSIS pair. Next on that branch: one Codex review, then the app view for launching a program with keys, so that a user needs no command line.
- **Terminal and history on branch `feat/terminal-and-history`** (on top of `ui/key-cabinet`). **Open terminal with keys…** opens the system command prompt with one project's keys, and a key's drawer lists its earlier versions, with times and field names only. The design and its failure cases are in [docs/design/launch-with-keys.md](design/launch-with-keys.md). The real windows of this flow are an owner-present test ([FIRST_USE.md](FIRST_USE.md)). A unit test starts a real, windowless child process. Reviews [1](reviews/2026-11-08-terminal-review1.md) and [2](reviews/2026-11-08-terminal-review2.md) failed (T-R1 and T-R2 rows in [FINDINGS.md](FINDINGS.md)), so patching stopped (AGENTS.md).
- **Contract 11, decided (owner, 2026-11-09).** PROJECT.md is amended for the desktop: the vault lock is released before a child starts, the child inherits no vault handle and no `AKM_*` variable, and the decrypted vault stays in AKM's memory while AKM is unlocked.
- **The latest measured score is 66.25/100** ([full rescore](reviews/2026-11-09-full-rescore.md), 2026-11-09, on `c0a7d3b`, by five independent GPT-6.1 Terra reviewers at max effort; one duplicate root cause was removed from a raw 64.25). The baseline was 40.5/100. The bar is above 90/100, so real secrets still wait. The new defects are the FR rows in [FINDINGS.md](FINDINGS.md). The scale may differ from the baseline's.
- **New look on branch `ui/key-cabinet`** (owner, 2026-11-08). The owner gave the design of their product Odicto as the model, and rejected the large brass keys. Every screen now uses glass panels, sunken fields, one accent and the glowing stem. A key is a fine 28 px line drawing. The fonts are the same. The design is in [desktop/DESIGN.md](../desktop/DESIGN.md). New users have [GETTING_STARTED.md](GETTING_STARTED.md). The steps on that page come from the screens and the setup configuration: nobody has run the setup program on a clean computer yet. An open select is now AKM's own list; the real select stays as the field. [Look review 1](reviews/2026-11-08-look-review1.md) failed with 4 P2 and 4 P3 (LOOK-R1 rows); all eight are fixed with tests that failed first and caught mutants. [Look review 2](reviews/2026-11-09-look-review2.md) failed with 2 P2 and 2 P3 (LOOK-R2 rows), so patching stopped (see "Next action"). A screen reader or UI Automation gets the system's list, because those tools could not drive AKM's own list (owner's choice, 2026-11-08). The UI-R2 findings stay open, and the choice between options (a), (b) and (c) in "Next action" stays with the owner. Pull request #10 must take the new `styles.css` in a merge.

## Last full check run

On `c0a7d3b`, the head of `ui/key-cabinet` (2026-11-09, fake data only, Rust builds in `C:\akm-build`). Rust sources did not change after `3c4cf2a`.

- Rust, at `3c4cf2a`: `cargo fmt --check` and Clippy (`-D warnings`) clean. `cargo test --workspace --offline --locked --no-fail-fast` 476 passed, 0 failed, 2 ignored.
- Frontend: `npm test` 374 passed in 30 files, the production build, and `npm run test:ui` 81 passed (mocked native boundary).
- Security checks: `check_no_network.py` PASS and its test; `check_dependencies.ps1` PASS at `3c4cf2a`.
- Real readers on the `akm.exe` below: KeePassXC 2.8.0 interchange PASS; kdbxweb 27 PASS, 0 FAIL.
- Tools: `check_docs.py` PASS and its test; `test_proof_snapshot.ps1` all cases.
- Native GUI proofs on the build below: `-JournalOnly` 27 checks PASS, `-JournalExportOnly` 9 checks PASS. No clipboard access, no minimize. The settings file was byte-identical afterwards (`dacba49a…9115`), and no `akm-desktop` process remained. Evidence: `W:\AKM-archive\proofs\20261108-193439-quiet-instrument`.
- CI: green on `3c4cf2a` ([run 42509623631](https://github.com/Gargeya-Grey/akm-vault/actions/runs/42509623631)). At `c0a7d3b` the job did not start: GitHub reported a failed payment or a spending limit on the account.
- Not run: R4-04, the real clipboard test, the full native GUI proof, the native import dialog, the Windows toast, a real window hide, install on a clean computer, real high-contrast themes and a real screen reader. They need the owner present ([FIRST_USE.md](FIRST_USE.md)).

## Builds under test

Builds of `c0a7d3b`, all unsigned and not installed by an agent, SHA-256.

- `akm-desktop.exe` `5d6bd9cb4e5495691d74fab2b40fefc587cbc02689d5086b97804955774a49bd`
- `akm.exe` `701f34f2155981e7397be083d81b87ff1cecc1331f39305486b366c4f7197bce`
- `AKM_0.1.0_x64-setup.exe` `1f44ad9734f0a747d8a24637b9353e5d234788065962bd38b40ca04a57556e59`, in `W:\AKM-archive\installers\`

Each CI run uploads its own build with a `SHA256SUMS.txt`, and keeps it for 90 days.

## Stop rule: the owner's decisions (2026-11-07)

[Review 2](reviews/2026-11-07-phase2-review2.md) failed with 4 P2 and 1 P3 (FINDINGS.md rows P2R2-01 to P2R2-05). Units 1c, 1b and the tools failed both phase two reviews, so patching stopped (AGENTS.md). In each class that came back, the earlier fix kept a premise that the review broke. The owner chose each proposed redesign, and one round without a third review:

| Finding | Premise that failed | Proposed redesign |
|---|---|---|
| P2R2-01 fetch tool deletes nested files (F) | The tool can tell its own files from others, so it may delete. | The tool never deletes. Each fetch goes to a new folder named by version and digest. |
| P2R2-02 hidden import still commits (A) | A hide that the page sees must cancel an approval that native code gives. | The native dialog is the review: an approval in it stands, a lock still refuses it. Contract 7 changes its wording for the desktop import. |
| P2R2-03 delayed discard reaches session B (B) | A discard without an identity reaches only the current session. | `view_discard` carries its session ID; a failed discard locks only that session. |
| P2R2-04 clone replacement mixes journals (B) | The root UUID names one vault. | A fingerprint that this device has seen belongs to the journal that saw it; only an unseen one falls back to the root UUID. |

## Campaign to the release gate (2026-11-13)

The owner asked for the cause of the stall and for a plan to pass the bar, and gave the planner the design decisions. The plan is [design/gate-campaign.md](design/gate-campaign.md). The score stays 66.25/100: no new full review ran.

**Cause, in short.** The review loop did not converge: 30 reviews with a verdict are recorded, and none passed without a scope split. Fixes were made by instance. New features started before the gate. Areas H and I hold 10 points that are not product code, and the bar cannot be passed without them.

**Done on 2026-11-13.**

- Branch `gate/integration` holds all five open branches (`fix/score-findings`, `fix/terminal-rework`, `fix/core-findings`, `fix/tools-docs`, `ui/key-cabinet`). One conflict, in `docs/FIRST_USE.md`, was resolved by hand. The branch is local and not pushed.
- Checks on the merged tree: `cargo fmt --check` and Clippy (`-D warnings`) clean; `npm run build` passed; `npm test` 443 passed in 32 files; `npm run test:ui` 104 passed; `check_docs` passed. The Rust workspace tests: see "Last full check run".
- Three workers started, each in its own worktree from `955e3c5`: unit 1 (native session and launch), unit 2 (frontend), unit 4 (tools, CI and release package). Unit 3 (core and CLI) gets its first review before any new fix.

**Decisions taken by the planner, for the owner's veto.**

| Question | Decision |
|---|---|
| Terminal launch: a prompt lock, or no terminal after a lock | Both. The process starts suspended outside the session gate. Under the gate, AKM checks the session and the vault revision, then resumes the process or ends it. |
| Rows: option (a), (b) or (c) | Fix LOOK-R4-01 and LOOK-R4-02 by class (one function builds the facts that a row shows; one read mechanism owns the request identity), then review them with the frontend unit. |
| Stop rule | A unit gets at most 4 review rounds. A class that fails two rounds gets a redesign that removes its premise. A question goes to the owner only for money, an account, an irreversible step or a contract change. |
| Review model | The first review runs on GPT-6.1 Terra and GPT-6.1 Sol, both at max effort, with one brief. The better one reviews the rest. |

**Gates that need the owner.**

1. GitHub Actions billing. CI has not run since 8 Nov 2026.
2. A rule that lets a disposable CI runner run the clipboard, real-window and install proofs (AGENTS.md, Safety). Unit 4 writes the workflow and the proposed wording.
3. OS lock, suspend and a real idle wait on the owner's computer ([FIRST_USE.md](FIRST_USE.md)).
4. Each merge. Publisher signing. The choice to publish.

## Next action

1. Collect units 1, 2 and 4: check each against its brief and the contract in the plan, merge, run the full checks, build, and run the real-surface proofs that need no owner.
2. Record the two first reviews of unit 3, choose the review model, and start the unit 3 fixes after unit 1 has landed.
3. One review for each of units 1, 2 and 4, with fix rounds by class.
4. Area probes, then the owner's gates above, then the full review.

## Local state outside the repository

- Git tag `archive/pre-cleanup-2026-11-01` keeps the tracked evidence that the cleanup removed. Git tag `baseline/pre-unit-1a` holds the source snapshot from before unit 1a.
- `W:\AKM-archive\2026-11-01-local-evidence\` keeps the ignored local files that the cleanup moved out. `W:\AKM-archive\2026-11-07-leaked-test-journal\` keeps the 1288 fake journals that tests wrote to the real journal folder before the guard (1B-C-01).
- `W:\AKM-archive\proofs\` keeps the verification skill's transcripts.
- The desktop app's settings select the fake vault `%TEMP%\opencode\akm-app-proof-07ee8d95-96d0-40df-bf1c-e24b33600ef2\vault.kdbx` (SHA-256 of the settings file `dacba49a…9115`). The owner's earlier setting is unknown. When the owner opens their own vault, the app replaces this setting.
- The old builds in `target\debug` and `target\release` were deleted on 2026-11-07 to free disk space; the unit 1a hashes are in Git history.
