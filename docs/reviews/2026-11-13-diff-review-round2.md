# Diff review of fix round 2

- Model: Codex GPT-6.1 Sol, effort max (the campaign's review model; see REVIEWING.md). Two reviewers, each in its own checkout: A on the core and the desktop native code, B on the desktop edit form, the search and the layout.
- Reviewed range: `717d481..bc28959` on `gate/integration` (fix round 2 for the FW rows).
- Build hashes: none. The reviewers ran selected tests and probes, and checked no build.
- Date: 2026-11-13.
- Result under the pass rule: **fail** (A: 2 P2, 3 P3; B: 5 P2, 4 P3). This review is not a score: it is a gate before the third full review.

The text below is the reviewers', except that links to a reviewer's local checkout are written as repository paths, and each reviewer's closing notes about its folders and its cleanup are removed. The rows are in [FINDINGS.md](../FINDINGS.md), "Diff review of fix round 2".

---

## Reviewer A: core and desktop native

**Fail. 0 P0, 0 P1, 2 P2 findings.** Reviewed `717d481..bc2895902321e545514f00fda6b474476f02a1ed`.

Source acceptance is **NO**. Work-unit completion is **NO**. The tracked tree remains clean; no commit was made. All **55 selected Rust tests**, seven helper tests, and the page build passed.

| Finding | Assessment | Evidence |
|---|---|---|
| FW-02 | **Partly holds** | Interrupted recovery, retry, failure-step tests, and missing-mapping reads without `mapping.lock` pass. Two ownership races remain, detailed below. |
| FW-04 | **Holds** | The real CLI selected recycled history using a compact uppercase UUID and restored the same key using an uppercase UUID inside a URN. |
| FW-09 | **Partly holds** | Parser and import-preview allocator tests pass, including alternate error inputs. The desktop export builder still frees uncleared value copies. |
| FW-01 | **Holds within checked scope** | Sign-out delivered during stalled expiry cleanup immediately refused reveal; two cleanup attempts completed after release. Dropping `Delivery` drained queued work and released the worker's session reference. |
| FW-10 | **Holds within checked scope** | All 27 export tests pass. Additional directory, writer-open and KDBX destinations refused without leftovers. Read-only replacement succeeded and reported its undeleted aside file. |
| FW-08 | **Holds** | Seven helper tests and 16 additional payloads passed, including tabs, CR/LF, Unicode, empty arguments and quote/backslash runs. Installer execution was excluded. |

The filesystem reproductions below used unchanged production helper bodies with injected actor interleavings and fake files. They did not exercise a live save dialog.

1. **D2A-01 — P2 — Recovery can overwrite a different file after its final check.**  
   At `crates/akm-core/src/audit.rs:376` and `crates/akm-core/src/audit.rs:381`, the fingerprint checks an opened file, but installation replaces a pathname.

   **Reproduced.** After the final read handle opened, another actor renamed that file away and installed an unrelated sentinel at the live name. The handle still yielded the expected damaged bytes. Recovery succeeded and overwrote the sentinel.

   **Classes A and B**, with a class-F preservation failure. The existing changed-journal regression substitutes bytes *before* the final open, so it misses this case.

   **Smallest closing change:** publication must preserve a substituted destination. The export's held-object/non-replacing pattern is a candidate, but adopting it also requires durable interruption handling so a missing journal cannot become silently empty. Another pathname check leaves the race.

2. **D2A-02 — P2 — Recovery cleanup deletes files it no longer owns.**  
   `crates/akm-core/src/audit.rs:199` deletes by name after the created handles have closed; `crates/akm-core/src/audit.rs:395` now applies this to the kept copy too.

   **Reproduced.** Rename the owned kept copy away, place an unrelated sentinel at its former name, then fail preparation. Cleanup deletes the sentinel. The displaced owned copy survives.

   **Class F.** The sweep also found:

   - `crates/akm-core/src/journal.rs:906` unconditionally removes `mapping.json.tmp`. **Reproduced through public journal placement:** an existing sentinel was removed.
   - `crates/akm-core/src/vault.rs:1737` drops the handle and removes `PendingFile` by pathname. **Inspection.**
   - `desktop/src-tauri/src/settings.rs:148` and `desktop/src-tauri/src/settings.rs:196` have the same pattern. **Inspection; already disclosed in the commit-point design's limit 7.**

   **Smallest fix:** retain creation handles and delete through those handles. Replace the fixed mapping temporary name with a unique `create_new` file rather than deleting an unproved predecessor. Closing tests must substitute sentinels after creation.

3. **D2A-03 — P3 — Desktop export retains the uncleared-buffer fault class.**  
   `desktop/src-tauri/src/commands.rs:838` creates a growing `Zeroizing<String>`. Its final allocation is cleared; allocations released during growth are not.

   **Reproduced with an allocator observer.** A two-key export freed **three allocations containing the fake marker**. Materializing and clearing the same core environment values freed **zero** uncleared marker allocations.

   This is the FW-09 memory-cleanup class; no dedicated fault-table row exists. I rate it P3 as a defense-in-depth defect, without demonstrating application-level extraction.

   **Smallest fix:** calculate the complete output size and allocate once before appending secret text. Add an allocator regression around the complete export builder.

4. **D2A-04 — P3 — The new UUID regression asserts an incidental random-data property.**  
   `crates/akm-cli/tests/selection.rs:381` requires the randomly generated UUID to contain enough alphabetic hex digits to produce both cases.

   **Inspection.** A valid UUID containing zero or one alphabetic digit fails this assertion despite correct selection behavior. No fault-table class fits.

   **Smallest fix:** use a deterministic fixture UUID containing multiple alphabetic digits. Keep the selection/readback assertions.

5. **D2A-05 — P3 — Clipboard design text contradicts the accepted mechanism.**  
   `docs/design/unit1h.md:26` still rejects `try_lock`; lines 121, 133 and 137 still describe waiting for cleanup. These contradict the revised D2 and current code.

   **Inspection. Class D.** Reconcile the rejected alternative, failure table and limits with the hand-off mechanism.

The requested document checks found two overstatements:

- `docs/THREAT_MODEL.md:146` says recovery changes no other file. D2A-01 and D2A-02 contradict that preservation promise under substitution.
- `docs/THREAT_MODEL.md:172` says AKM clears its owned copies and builds dotenv text without intermediate copies. D2A-03 contradicts that for the complete desktop document.

I established no additional overstatement in row 12, the export row of `CLAIMS.md`, or the commit-point lock-order description. The disclosed CLI `Command` environment copy remains outside the narrowed cleanup promise.

For test value, D2A-04 is the new assertion that does not earn its place. I found no newly added test that cannot fail. The recovery tests are useful but omit substitution **after the final file opens** and **after temporary-file ownership is established**.

I did not check actual Windows notification delivery, real clipboard behavior, native dialogs, settings, installation, another volume, actual process crashes/power loss, or the full suites. Baseline regression failures were not independently rerun. The other reviewer's frontend scope was excluded.

---

## Reviewer B: desktop edit, search and layout

**Fail: 0 P0, 0 P1, 5 P2 findings.** Source acceptance **NO**; work-unit complete **NO**.

Reviewed `717d481..bc2895902321e545514f00fda6b474476f02a1ed`. Tracked files are unchanged; no commit was made.

| Finding | Assessment | Evidence |
|---|---|---|
| FW-03 | Partly holds | Named-field requests work, but foreign titles still change, untouched intervals can block edits, empty drafts can write, and a lost-reply retry leaves stale metadata. |
| FW-11 | Holds | Commas, CR/CRLF paste, blank/trailing lines and a 100,000-character scope behaved correctly. Tags use the expected separators. Accessibility gap below. |
| FW-05 | Holds | Username searches matched exact, partial and differently cased queries through the page. |
| FW-06 | Partly holds | Sampled search folding agrees with the core. Select type-to-find ignores supplementary-plane letters. |
| FW-07 | Partly holds | All 192 layout configurations and 65 shipped browser tests passed. Remaining fill-only separators disappear in forced colours. |

1. **D2B-01 — P2: an unrelated edit still trims a foreign title.**  
   `desktop/src-tauri/src/commands.rs:485` calls the storage writer, which trims every title at `crates/akm-core/src/vault.rs:1749`. **Reproduced through generated production handlers, save and reopen:** `{"notes":"new note"}` removed the outer spaces from `"  Foreign\u200B e\u0301  "`. The ID, secret and secret-change time stayed unchanged. This disproves the requested preservation sentence.  
   Class: FW-03 preservation class; no exact table class. Smallest fix: clean a title only when explicitly supplied, and preserve an untouched title in storage.

2. **D2B-02 — P2: an untouched foreign interval prevents unrelated editing.**  
   `desktop/src/components/KeyForm.tsx:315` applies the maximum even when unchanged; `crates/akm-core/src/vault.rs:415` revalidates the complete input. **Reproduced:** a stored 50,000-day interval blocked a notes-only save in Edge. Direct handler execution also rejected it without writing.  
   Class: FW-03 preservation class. Smallest fix: enforce interval bounds on changed intervals at both boundaries.

3. **D2B-03 — P2: an empty draft rewrites a foreign entry.**  
   `desktop/src-tauri/src/commands.rs:485` always calls `Vault::update`. **Reproduced through the handler and reopen:** `{}` changed vault bytes, increased history from zero to one, advanced modification time and returned a key instead of `null`. The fixture had never been written by AKM; the writer also synthesizes `AKM_ID`.  
   Class: FW-03 no-change contract. Smallest fix: detect unchanged effective input before invoking the rewriting path.

4. **D2B-04 — P2: retrying a lost save reply leaves the page stale.**  
   `desktop/src/components/KeysView.tsx:145`. **Reproduced with the actual frontend API/boundary over mocked IPC:** the first update committed `Renamed` but lost its reply; retry returned `null`. The page still displayed `Original` in its row and drawer, and its notice named `Original`. There were two updates and only one list read.  
   Class B: stale snapshot. Smallest fix: refresh current metadata on the `null` branch.

5. **D2B-05 — P2: unlocking trims an untouched selected/stored vault path.**  
   `desktop/src/components/Unlock.tsx:64`. **Reproduced at the outgoing form boundary:** a trailing U+00A0 was removed. A disk probe opened two different valid fake vaults at the original and trimmed Windows paths, using the same password and returning different titles. This is another untouched-value transformation outside the fix.  
   Class: FW-03 preservation class. Smallest fix: validate path emptiness without changing the path.

6. **D2B-06 — P3: the scopes hint is not associated with its field.**  
   `desktop/src/components/KeyForm.tsx:343`. **Reproduced:** the textarea has no accessible-description association with the new one-scope-per-line instruction. Smallest fix: give the hint an ID and reference it with `aria-describedby`. No exact table class.

7. **D2B-07 — P3: supplementary-plane type-to-find is ignored.**  
   `desktop/src/components/FilterSelect.tsx:159`. **Reproduced:** typing Deseret lowercase U+10428 left "Choose" highlighted instead of the option starting with capital U+10400. `caseless` matches them correctly; `event.key.length === 1` rejects the input first. Smallest fix: count Unicode code points. No exact table class.

8. **D2B-08 — P3: fill-only separators remain invisible in forced colours.**  
   `desktop/src/styles.css:562`. **Reproduced in headless Edge:** the fact-row separator composites to white on its white surface and has no border. Inspection found the same unhandled pattern for key/bin rows, form groups and settings sections at lines 472, 583 and 620.  
   Class J. Smallest fix: add system-colour borders for these separators in the forced-colours block.

9. **D2B-09 — P3: an existing fixture-only test does not earn its place.**  
   `desktop/src/components/RowIdentity.test.tsx:96`, "the fixture pairs draw the same key glyph", checks fixture collisions rather than a required product behaviour. By inspection, a permissible glyph-generation change can fail it while row identity remains correct. This test predates the round. Remove the standalone fixture assertion or incorporate its necessary premise into the behavioural test.

`desktop/DESIGN.md:58` overpromises forced-colour boundaries, as D2B-08 demonstrates. The preservation wording in `docs/design/frontend-reads-and-rows.md:379` needs qualification until D2B-01-03 are resolved. I found no additional scoped contradiction in GETTING_STARTED, FIRST_USE or the README desktop section.

The new tests generally earn their place, but their fixtures miss the foreign-title, foreign-entry no-op, invalid untouched interval and lost-reply retry cases. The isolated form-reuse probe also retained old fields after a prop change; I did **not** count that as a production defect because the sole caller unmounts between distinct targets.

Checks passed: production page build, 65 targeted page tests, 65 shipped browser tests, three targeted Rust regressions, fast documentation checks, and the independent 192-case layout matrix. Exact padded and plain scopes stayed separate through export and a mocked launch host.

I did not run full suites, baseline regression replays, KeePassXC itself, real screen readers, Windows scaling/contrast settings, or owner-present native workflows. Foreign fixtures used the KeePass writer directly. No real app window, clipboard, saved app settings, OS-session operation or installation was used.
