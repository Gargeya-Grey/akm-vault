# Terminal launch and key history, review 2

- Model: Codex GPT-6.1 Terra, effort max.
- Reviewed commit: `34c21af7cadf27c75fe29563d5b3dd3f36d8e0af` on `feat/terminal-and-history`, against `d63c2a1`.
- Build hashes: the reviewer did not check a build.
- Date: 2026-11-08.
- Result under the pass rule: **fail** (6 P2, 1 P3). This is the second failed review of this work unit, so the AGENTS.md stop rule applies (see STATE.md).

The text below is the reviewer's, unchanged, except that absolute links are shortened to repository paths and the reviewer's local temporary paths are removed.

---

**Source acceptance: NO. Work unit complete: NO.** The review found six P2 defects and one P3 documentation defect in commit `34c21af7cadf27c75fe29563d5b3dd3f36d8e0af`. Under the project’s pass rule, this is a failed second review. The two-failure stop rule applies: patching should stop, and the main session should record the design question for the owner. I did not edit project files.

## Ranked findings

1. **P2 — Journal contention can prevent AKM from locking.** In `desktop/src-tauri/src/launch.rs:383`, the session gate is held while `disclose_env` writes the journal. The journal takes a blocking file lock. If another process holds that lock, the explicit lock command, idle expiry and Windows lock/suspend handling all wait for the same session gate. The vault can therefore remain unlocked for as long as the journal is stalled. **Found by inspection.**  
   **Closing test:** Hold a fake journal lock, start a reviewed launch, and verify that an explicit lock and a simulated Windows lock complete promptly without a later disclosure or launch. Release the fake lock and verify the final outcome.

2. **P2 — A stale native review can still appear after a lock.** `desktop/src-tauri/src/review_window.rs:97` spawns a thread under the gate, but that thread calls `show` independently. The gate can be released and the session locked before the thread displays the old project, environment and variable names. This affects both terminal launch and import. Their fake hosts record presentation synchronously, so the named tests miss this interleaving. **Found by inspection.**  
   **Closing test:** Put a barrier in the production scheduling seam immediately before actual presentation, lock the session, then release it. Assert that neither review is displayed.

3. **P2 — Review text can identify different scopes and variables identically.** `desktop/src-tauri/src/review_window.rs:53` truncates every name after 80 characters. Two valid variable names with the same 80-character prefix and equal length consequently produce identical review text. It also renders an actual directional control identically to a literal `\u{202e}` sequence because literal backslashes are not distinguished from escapes. This prevents an exact review of the affected launch or import. **Reproduced** using the formatter extracted from the reviewed source.  
   **Closing test:** Feed both collisions through launch and import review generation. Require distinguishable, complete names or refusal before approval. Include omitted invisible Unicode characters in the cases.

4. **P2 — A launch overtaken by a session change loses its terminal-specific notice.** In `desktop/src/api.ts:49`, a successful reply from an earlier generation takes the generic `reportCompletion` path before a `terminal` outcome is created. The retained message tells the user to inspect a vault or export; it does not say that a terminal holding keys opened and remains open after locking. **Reproduced** with the copied frontend code and a delayed fake native reply.  
   **Closing test:** Start a launch in session A, change to B, then deliver success. Assert that the retained, dismissible notice identifies the terminal and its persistence, and that it cannot invalidate B.

5. **P2 — A lock before import presentation can leave no native outcome record.** In `desktop/src-tauri/src/commands.rs:990`, if the session locks after `prepare_review` but before `while_current`, `?` returns before `record_import`. If the page also loses the reply or reloads, the dispatched import has no native outcome to recover, contrary to contract 7. The new lock-before-review test checks the error and absence of a window but does not check the record. **Found by inspection.**  
   **Closing test:** Lock in that preparation gap, lose the IPC reply, and assert that the native outcome for the original operation remains available and truthfully reports that the import did not complete.

6. **P2 — A native dialog failure is recorded as the user declining.** `desktop/src-tauri/src/review_window.rs:160` maps every failed `TaskDialogIndirect` call to `false`. Import then records `ImportDeclined`, and launch reports cancellation, even if no dialog appeared. Microsoft distinguishes API failure from Cancel, Escape and close. TaskDialogIndirect documentation: https://learn.microsoft.com/en-us/windows/win32/api/commctrl/nf-commctrl-taskdialogindirect **Found by inspection.**  
   **Closing test:** Inject a failed HRESULT and a worker/channel failure. Assert an error outcome and no approval or false decline record. Separately test Cancel, Escape and close as declines.

7. **P3 — The verification guide still promises the disproved journal ordering.** `.agents/skills/verify-akm/features/terminal-and-history.md:10` says the journal entry precedes *any value read*, but `plan_env` reads each selected value during validation before the dialogs. The design note now states the narrower intended behavior. **Found by inspection.**  
   **Closing test:** Correct the guide and check the relevant documentation for the old claim; retain a regression that verifies journal failure prevents values leaving the vault and prevents launch.

## Earlier findings

No first-review finding is formally closed because this review fails. The assessments below distinguish the implementation from the remaining evidence.

| Finding | Assessment and regression evidence |
|---|---|
| T-R1-01 | The source refuses `PROMPT`, `DIRCMD` and `COPYCMD` before a dialog. The named uppercase `PROMPT` case would detect the old behavior by inspection; it passed at this revision. |
| T-R1-02 | The production command now supplies `/d`. Its argument assertion would fail on the old command. The real-child test has no controlled AutoRun value, so it does not independently demonstrate the original scenario. |
| T-R1-03 | The review now discloses inherited environment and folder lookup; its literal-text test would fail on the old message. It does not exercise an unrelated fake parent secret as the requested closing test specified. |
| T-R1-04 | The named newline and bidi tests would fail on the old raw interpolation. The formatter still has the collisions in finding 3. |
| T-R1-05 | The named fake-host tests cover a lock before the gated closure, but not the asynchronous presentation gap in finding 2. |
| T-R1-06 | The journal and value copy now run under the gate. The named test would detect the old journal append after a lock, by inspection, but it does not instrument value reads. The new blocking-gate defect is finding 1. |
| T-R1-07 | The vault mutex is released before start, and the named test checks that boundary. The design and state documents disclose that the decrypted vault remains in the unlocked desktop session. The owner’s contract decision remains open. |
| T-R1-08 | The native history mapper allowlists field names. The named constructed-`HistoryVersion` test would detect the old raw mapping, but it is not a foreign-KDBX-to-IPC test. |
| T-R1-09 | The same-session late-reply test would fail on the old code and passes now. The session-change case remains defective as described in finding 4. |
| T-R1-10 | The source sets Cancel as default and approves only the confirm ID; Escape and close map to non-approval by inspection. The named unit test checks constants and the mapper, but would still pass if the actual `nDefaultButton` assignment were changed to zero. The real window remains untested. |
| T-R1-11 | The design note and test name use the narrower promise, but the verification guide still makes the old claim. See finding 7. |
| T-R1-12 | Remains open for the release review, as directed. |

I compared the tests with the old source by inspection. I did **not** run them against the unfixed revision, so these are not observed red-test results.

For AutoRun, **`/d` is the documented control for this specific issue**. Microsoft states that it disables AutoRun commands (https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/cmd). The production argument assertion provides useful regression coverage, but a controlled AutoRun fixture remains unrun; this review did not write the registry.

## Checks and limits

- **PASS:** `cargo test -p akm-desktop --offline --locked --lib launch` — 29 tests.
- **PASS:** the corresponding `--lib review_window` filter — 4 tests.
- **PASS:** three changed Vitest files in an isolated copy of the frontend — 92 tests. A separate temporary probe reproduced the generic terminal outcome.
- **PASS:** the formatter collision probe against source extracted from the requested commit.
- Cargo ran from the repository root with an isolated target and fake journal root. The checkout remained clean and detached at the requested commit.

No build hash was checked. I did not launch the GUI, open a dialog or visible console, touch the clipboard, read saved settings, write the registry, or change the OS session. The real picker, dialog ownership and buttons, terminal window, six owner-present tests, PR 9’s separate findings, and a full audit remain unverified. The native dialog’s buffer lifetimes and fail-closed button mapping appear consistent by inspection, apart from the failure-reporting and scheduling defects above; that is not real-window proof.

The shared observer requires an artifact root inside the project, while this review required no project-file edits and all temporary files in a removable external folder. Registration was therefore not completed. A task-local handoff receipt records the gap.

Automatic policy rejected the command to delete the review-created temporary folder, with the stated reason “blocked by policy.” The folder remains in the reviewer's temporary files, containing approximately 2.70 GB of files. No space was freed.
