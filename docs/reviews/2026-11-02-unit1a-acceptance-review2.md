# Unit 1a acceptance review, round A1 (raw)

- Date: 2026-11-02
- Model: GPT-6.1 Terra (`gpt-6.1-terra`), Codex provider, requested `reasoningEffort: max`
- Reviewed: branch `fix/unit1a-sound-findings` at `9eedfe2` (application source `94d8008`), judged against the repair design revision 3 with its amended section 2.1 messages
- Build hashes checked by the reviewer (all matched STATE.md, before and after its probes): `akm-desktop.exe` 32f591a7…9104, `akm.exe` ad2ec01b…afc1, MSI d7c457b9…852c, NSIS 05f8124b…a3cf
- Verdict: **Source acceptance: NO. Work unit complete: NO.** 1 P2 and 4 P3 findings, recorded as 1A-A2-01 to 1A-A2-05 in FINDINGS.md.
- Result under the pass rule: **fail** (a P2 finding). This is the tenth rejection of unit 1a. Patching stops; the result goes to the owner (STATE.md).

The text below is the reviewer's report. The reviewer's links to repository files became plain text with the repository path and line. One link to a file in the reviewer's local temporary folder (a handoff receipt) became "(local temporary file, not kept)". Other differences cannot be excluded.

---

## Ranked findings

1. **P2. An older opening can overwrite the newer vault's saved path.** In desktop/src-tauri/src/commands.rs:150, opening A checks its session ticket and then writes the path preference without holding an ownership fence. If A pauses after that check, B opens and saves its path, and A resumes, A can replace B's preference. On a later restart AKM can select A even though B was the latest vault opened. The ordering also exists in the baseline, but the function is a changed caller in this unit. **Found by inspection**, supported by a synthetic in-memory Session interleaving; the real settings file was not touched.
   **Closing test:** Pause A after its ownership check, complete B's opening and preference write using an isolated fake settings destination, then release A through the production command path. Assert that B remains active and its saved preference remains B.

2. **P3. The copy table still omits the specified sequence and warning-ID assertions.** desktop/src-tauri/src/session.rs:3026 checks clipboard text, lease state and whether *any* matching warning exists. It does not check the sequence or exact warning IDs for each publication outcome. For example, duplicate warnings from an `EarlierCopyUncertain` publication could satisfy the table's boolean assertion. The new tick-and-lock cleanup matrix checks these properties for cleanup, but does not fill this copy-table gap. **Found by inspection.**
   **Closing test:** For every copy-table row, assert the exact fake sequence, warning count and IDs, including a second failure episode while the first warning remains pending.

3. **P3. The new export timing test does not observe file publication.** crates/akm-cli/src/main.rs:1037 observes journal state at each **stdout** write. The existing file tests check the eventual records and a failed publication, but do not observe the request before the first temporary-file byte and completion after the file is published. The native proof likewise reads both records after publication. **Found by inspection.**
   **Closing test:** Drive `run_with` through the file-export path with an instrumented fake-data sink and publisher. Observe the journal before the first byte, at publication, and when completion is appended; make an early-completion mutant fail.

4. **P3. The cause assigned to the three export-proof failures is not established.** docs/STATE.md:22 says they were proof-script timing rather than AKM. The later pass, identical frontend bundle and older-build pass show that a successful run is possible; they do not establish why the earlier runs failed. `Select-FilterOnce` does wait up to 12 seconds for options after expansion, although it does not retry an exception from the selection attempt. **Found by inspection of the script and retained logs.**
   **Closing test:** Capture the combo box's pattern, option availability and application state at each failed attempt, and use a controlled delayed-pattern/option test to distinguish an automation race from an application failure.

5. **P3. The whole-unit diff fails `git diff --check`.** crates/akm-cli/src/main.rs:823 contains trailing whitespace. **Reproduced** by `git diff --check baseline/pre-unit-1a 94d8008 -- crates desktop tools .github Cargo.toml Cargo.lock`.
   **Closing test:** Remove the whitespace and rerun that command.

## Source acceptance: NO

The P2 saved-path race fails the pass rule. This is a **failed review**, rather than an incomplete verdict, even though the coverage limits below remain.

I checked these earlier in-scope P2 findings against the source, tests and retained evidence:

- **R4-01:** protected backup identity, clock-skew and failed `.bak` refresh.
- **P2-WARN-OWNERSHIP, 1A-R1-01, 1A-R1-03, 1A-R2-02 and 1A-R3-01:** warning classification, ownership and retained outcome handoff.
- **P2-JOURNAL-CLAIMS, 1A-R1-02, 1A-R1-04, 1A-R1-05 and 1A-R2-01:** journal state, append boundaries, sanitization and uncertain post-commit outcomes.
- **1A-R5-02, 1A-R6-02 and 1A-R8-02:** unreadable clipboard cleanup, warnings and best-effort wording.
- **1A-R5-04, 1A-R6-01, 1A-R7-01 and 1A-R8-01:** export completion, closed-output behavior and the fixed secret-independent mask.

The retained historical red logs support several of those checks. I did **not** independently replay the affected-version regression for every earlier finding. The open P1 **R4-04-SCRIPT** remains part of the owner-deferred runtime proof and is not accepted as fixed here.

For the last review's claims, four independent fake-backend text scenarios failed using the `836fc8c` clipboard source with the current Session and passed with the current clipboard source. A new fake Session scenario retained distinct warning IDs across recovery and recurrence. A blocked-consumer probe against the release CLI observed `export-requested` without completion while stdout was blocked, then completion after it drained. These probes do not replace the missing permanent coverage identified above.

The fourth `-JournalExportOnly` run is evidence for the successful fake-data flow it exercised. It does not establish the cause of the first three failures or prove that an intermittent application failure is absent.

Reviewed head was `9eedfe2`; application source was `94d8008`. The checkout was clean. All 139 archived baseline-manifest hashes matched. The four build hashes matched STATE.md before and after the probes:

```text
akm-desktop.exe  32f591a7d197ed93e7aba57d6c7a3ff8548ce44172a3b27f183222c0578c9104
akm.exe          ad2ec01b491de883493b12d0aad802084ad8831b45083083a8e77ef8c6ceafc1
MSI              d7c457b92d98e730db56e211eee6ebf716aee157f95fc20514d13a26427c852c
NSIS             05f8124b441f96c2c086151961e9487fd40b48c946b25b3bee7d8f97085ea3cf
```

## Work unit complete: NO

Source acceptance failed. R4-04 remains deferred and unproved. The owner-present real clipboard test and full native GUI copy proof were not run. KeePassXC interchange was not run on this build.

## What the review did not cover

I did not launch the GUI, touch the real clipboard or app settings, change the Windows session, or run full builds or suites. I did not exhaustively inspect every historical test and caller in the 56-file diff, independently replay every older failing-first regression, or establish real Win32 fault behavior. The snapshot has 22 added-looking paths whose pre-unit preimages remain unproved, apart from the separately identified pre-existing icon.

The shared observer requires a project-local artifact root and registry writes, which conflict with this review's temporary-folder-only restriction. The fake probes and task-local handoff receipt (local temporary file, not kept) are retained; no project files were changed.
