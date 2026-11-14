# Second full review, 13 November 2026

- Model: Codex GPT-6.1 Sol, effort max ([REVIEWING.md](../REVIEWING.md)).
- Reviewed commit: `51cbb0eb3aaa31563b759e6f9607725410419099` on branch `gate/integration`. Its product code equals `20b9359`; the later commit changes documents only.
- Builds of `20b9359` (unsigned, SHA-256): `akm.exe` `f0acb4b9ee636dee08c1225988c7c7c82945bbe0beb99a3e1f9fe1c84b0a602c`; `akm-desktop.exe` `411881ab91bd93520ff9f4c4006f4d74450297adcae812be2743b84aba50823f`; setup program `83ae22757ce42b06736a78a321f70907b9d83f7e6a2f56ff55f4b1f12576584c`.
- Date: 2026-11-13.
- Rubric: the nine areas and 100 points of the [baseline review](2026-10-30-baseline.md), with the scale of the [first full review](2026-11-13-full-review.md) of the same day.
- Brief: [2026-11-13-full-review-2-brief.md](2026-11-13-full-review-2-brief.md).
- Reviewers: six new independent reviewers, each with its own checkout and no view of the others' work. Two of them reviewed the security areas (C and D).
- What changed since the first full review: one fix round for its findings (the FV rows of [FINDINGS.md](../FINDINGS.md)) and a test audit that removed 87 tests and added 2 ([STATE.md](../STATE.md), "Destination, measures and path").

## Result

**87.5 / 100. Not ready for real secrets.** The bar is above 90/100, followed by the owner's testing.

| Area | Points | First review | Now | Reviewer | Main deductions |
| --- | ---: | ---: | ---: | --- | --- |
| A. Premise and positioning | 10 | 10 | 10 | Architecture and premise | None |
| B. Architecture and design | 15 | 14 | 14 | Architecture and premise | A recovery of the journal mapping that stops between its two file moves splits the history with no gap record |
| C. Security at rest | 15 | 11.5 | 14 | Security, first reviewer | Two disclosed limits: a purged key stays in backups; the journal has no anchor |
| D. Security in use | 15 | 12.5 | 11.5 | Security, first reviewer | A lock signal can wait behind clipboard work; a script in the page can read keys slowly; memory copies; `AKM_PASSWORD` in the parent shell |
| E. Data integrity and durability | 15 | 12 | 13.5 | Data and correctness | The native side trims untouched fields of a desktop edit, so a key can move between scopes |
| F. Functional correctness | 10 | 8.75 | 8.75 | Data and correctness | No old value in the history; `restore` refuses an upper-case UUID; the desktop search misses the username and the caseless rule |
| G. Desktop app quality | 10 | 9 | 9.5 | Desktop quality | Enlarged text pushes the drawer's buttons out of the window |
| H. Verification and hygiene | 5 | 3 | 3.25 | Verification and shareability | The owner-present proofs were not run; a helper defect; no CI run; no independent rebuild |
| I. Shareability | 5 | 2.75 | 3 | Verification and shareability | Unsigned and no release key; no clean-computer install; private paths in the tree |
| **Total** | **100** | **83.5** | **87.5** | | |

### How the total was counted

- **Two security reviewers.** The rule is the one of the first full review: the gate number uses the lower of the two security results, and both are recorded. The first reviewer gave C 14 and D 11.5 (25.5 of 30). The second gave C 14 and D 13 (27 of 30).
- With the second reviewer's security scores the total would be 89.
- **No root cause is counted twice.** Both security reviewers named the unrun owner-present proofs and left the deduction in H.
- No points were added for any other reason. A finding that a reviewer reported outside its own areas has no points.

### What the score does not hold

- **The two security reviewers again found different defects.** Only the first found that a lock signal can wait behind clipboard work (1.5 points). Only the second found that a failed `.env` parse frees text with a secret in it without clearing it (it counted this inside the memory-copy deduction that both made). A count of every security finding once gives the lower result, 25.5 of 30.
- **Findings with no points.** The first security reviewer reproduced that a desktop export overwrites a file that appears at the destination after the last check; the reviewer of that area did not find it. The desktop reviewer reproduced that a scope with a comma cannot be extended safely in the edit form. The desktop reviewer and the data reviewer found the trim of untouched fields independently.
- **Every fix of the fix round that a reviewer attacked held, except three that hold in part**: the mapping recovery (FV-01: an interrupted recovery), the edit form (FV-02: the native trim), and the cleared copies (FV-10: the error paths of the parser and the child's environment).
- **The removed tests.** All six reviewers inspected or sampled the test removals. None made a deduction for them. One noted that the structural rules now rest on review alone. Two noted that the remaining tests do not reach a new defect of their areas (the interrupted recovery, the late lock signal).
- **Completion.** The CLI has no import, no copy and no journal recovery. Neither surface shows or restores an old value of a rotated key.
- **Behaviour on a real desktop.** No reviewer started the app. Nobody ran these: the real clipboard test, the full native GUI proof, the native import and terminal windows, R4-04, a real OS lock, suspend and idle wait, install on a clean computer, real high-contrast themes, a real screen reader, real text scaling.

### Kinds of the deductions

The 12.5 points that the counted reviewers deducted, by the kind that each reviewer gave:

| Kind | Points | First review |
|---|---:|---:|
| `defect`: the code does a wrong or unsafe thing | 5.5 | 6.75 |
| `limit`: a weakness that the documents disclose | 2.25 | 2.75 |
| `release`: signing, licence, CI, install, publication | 2.25 | 2.5 |
| `design`: a design or threat-model choice | 1.0 | 2.0 |
| `proof`: nobody ran it on the real surface | 1.0 | 1.0 |
| `missing`: a required workflow is absent on a surface | 0.5 | 1.5 |

The 5.5 points of defects are all new findings of this review, or the remaining part of one earlier finding (the native trim, 1.5). Six of the eight are in the desktop app or in code that the fix round added.

### The reviewers' own answers

Each reviewer answered two questions with no points: would it keep its own revocable API keys in this build today, and do its points say the same thing.

| Reviewer | Area score | Keep its own keys in this build? | Do the points say the same? |
|---|---:|---|---|
| Architecture and premise | 24 / 25 | Yes, with precautions; the CLI for disclosure until the owner-present desktop proofs pass | Only partly |
| Security, first | 25.5 / 30 | Yes, with precautions, through the CLI; no desktop clipboard use until the lock-signal defect is repaired and the proofs pass | They support cautious CLI use, and overstate the desktop app |
| Security, second | 27 / 30 | Yes, with precautions, for limited CLI use | Only conditionally |
| Data and correctness | 22.25 / 25 | Yes, with precautions; metadata edits through the CLI until the trim is repaired | Only partly |
| Desktop quality | 9.5 / 10 | No, through the desktop app yet | No |
| Verification and shareability | 6.25 / 10 | No, for daily desktop use | Only partly |

As in the first review, every reviewer wrote that its points overstate the readiness of the desktop app for daily use, because the points cannot stand for the flows that nobody ran on a real Windows session.

### Limits of this score

- A score from one review model, not a professional audit.
- Each area except C and D has one reviewer.
- Reviewers read code and ran small probes with fake data. The suite and real-window results that they saw are reported by the implementer. Each reviewer matched the three build hashes.
- The scale follows the first full review, with the same model and effort.

The text below is each reviewer's, except that links to a reviewer's local checkout are written as repository paths, and paths in a reviewer's temporary folder start at `%TEMP%`. Each reviewer left its probe folder in place, because the tool policy refused the deletion.

---

## Reviewer 1: architecture and premise (A and B)

Reviewed `51cbb0e`, with builds attributed to `20b9359`. All three executable hashes match the brief. I ran 21 release-CLI invocations and seven small core-probe invocations, with 37 assertions. The checkout remains clean and unchanged.

**Area A: 10 / 10**

| Points | Severity | Kind | Finding | Evidence | What wins points back |
|---:|---|---|---|---|---|
| 0 | None | None | No deduction established. The positioning accurately distinguishes the established KDBX format from the unaudited integration and experimental writer. | Inspection | Not applicable. |

| Last review’s deductions | Assessment | Evidence |
|---|---|---|
| None; A received 10/10 | Not applicable | The narrow personal Windows workflow and its trust boundaries remain accurately qualified. |

**Area B: 14 / 15**

| Points | Severity | Kind | Finding | Evidence | What wins points back |
|---:|---|---|---|---|---|
| −1.0 | Medium | defect | **B-01. Interrupted mapping recovery can silently split activity history.** Recovery moves the damaged mapping aside before installing its replacement at `crates/akm-core/src/audit.rs:361`; a restart between those moves treats the missing mapping as an ordinary empty mapping at `crates/akm-core/src/journal.rs:770`, abandoning the earlier journal association without recording the recovery gap. | **Reproduced from a simulated interruption state.** The crash window itself is inspection evidence. | Add restart regressions that terminate recovery between the two moves, for mapping and chain recovery. Restart must preserve the evidence and either refuse disclosure or complete recovery with an explicit gap record. |

In that constructed disk state, the release CLI revealed the fake key with exit 0 and empty stderr. Its replacement journal contained only `reveal-requested`, with no recovery record; `audit --verify` also exited 0. The old journal and pending recovery file remained on disk. I did not force an actual process crash.

| Last review’s deduction | Assessment | Evidence |
|---|---|---|
| FV-01: damaged mapping blocks unrelated vaults and cannot be recovered, −1.0 | **Partly holds** | Explicit recovery of malformed and oversized mappings resumed both vaults, preserved the damaged mapping and both old journals byte for byte, and reported lineage warnings. The interrupted-publication case above remains. |

Candidate snapshots, stale-write refusal, session-bound disclosure capabilities and the native commit boundary support the remaining architecture points. My stale-writer probe also confirmed that a refused update leaves its in-memory snapshot unchanged.

I inspected the test-removal commits and surviving recovery tests. The retained failure tests exercise returned errors and rollback; they do not establish restart behavior after process termination. I assign no separate deduction for removing the source scans.

**4. Findings outside my areas, with no points**

| Area | Severity / kind | Finding | Evidence | Closing change |
|---|---|---|---|---|
| F | Low / missing | Existing **FV-23 remains**. History lists versions and changed fields but provides neither an old value nor version restoration. `crates/akm-core/src/vault.rs:703`. | Inspection; not a new finding. | Provide an explicit, journaled old-value or restoration workflow and verify it after reopening. Currently this requires KeePassXC or a backup. |

**5. What I did not cover**

I did not run desktop windows, clipboard operations, settings access, OS-session tests, installation, network activity, full builds or full suites. I did not independently rerun interoperability. Those results remain project-reported. This was an architecture and premise review, not an independent audit of cryptography or every security and correctness path.

**6(a). Would I keep my own revocable API keys here?**

**Yes, with named precautions:** low-value revocable keys, a local vault, an independent encrypted backup, a tested KeePassXC escape route and an interactive password prompt. I would use the CLI for disclosure until the owner-present desktop proofs pass. After a recovery error or interruption, I would preserve the journal folder and resolve recovery before continuing.

The first workflow failure I expect is a CLI user trying to review a `.env` import and discovering that this requires switching to the desktop.

**6(b). Required workflows**

“Complete” below describes available implementation within its disclosed limits. It does not substitute for native acceptance.

| Required workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Complete.** Ran creation, add, inspection and metadata editing; read the remaining command paths. | **Complete by code inspection.** Unlock/list proof is project-reported; other real-window management remains unrun. |
| Preserve metadata | **Complete by inspected code and reported interchange.** Ran username/notes editing and readback. | **Complete by inspection.** The form now preserves untouched stored values; native execution was not run. |
| Rotation tracking and recoverable history | **Partly complete within AKM.** Read rotation/history code; rotation proof is project-reported. Old-value recovery requires another application. | **Partly complete** for the same reason. Read code; no real-window rotation/history proof. |
| Privacy-aware copy and cleanup | **Missing.** No CLI copy command; command map inspected. | **Complete in implementation by inspection.** Real clipboard behavior remains unproved. |
| Reviewed import; scoped export and injection | **Partly complete.** Export and injection are implemented and project-reported proved; import is absent. | **Complete in implementation by inspection.** Save `.env` proof is project-reported; native import and terminal windows remain unrun. |
| Encrypted backups and useful private journal | **Partly complete.** Ran disclosure, damage and recovery probes; B-01 remains, and CLI recovery is absent. Backup behavior was inspected. | **Partly complete.** Shared recovery has B-01. Read the native recovery caller; focused Activity proof is project-reported. |
| Defensive cloud-folder use and stale-write refusal | **Complete within disclosed local-lock limits.** Ran a stale-writer core probe and inspected persistence. | **Complete by shared-core and caller inspection.** No live desktop sync-conflict proof was run here. |

**7. Do the points say the same thing as 6(a)?**

Only partly. They support cautious CLI use and credit a coherent architecture and candid positioning. They overstate daily desktop readiness if interpreted as proof of complete workflows or working Windows integrations. Revocable keys reduce recovery costs; they do not replace those proofs.

The probe folder (`%TEMP%/akm-review-ab-63f6499508c44529bd335eb6191508fc`) and handoff receipt (`%TEMP%/akm-review-ab-63f6499508c44529bd335eb6191508fc/handoff.json`) are retained; no owned process remains. Observer registration rejected the required TEMP root because it is outside the project. Tool policy separately rejected deletion without a more specific reason. Nothing was deleted; **0 bytes freed**.

## Reviewer 2: security (C and D), first reviewer

Reviewed `51cbb0eb3aaa31563b759e6f9607725410419099`, with product builds from `20b9359`. All three executable hashes match the brief. The checkout remains unchanged.

**Area C: 14 / 15**

| Points deducted | Severity | Kind | Finding and evidence | What would win the points back |
|---:|---|---|---|---|
| 0.5 | Low | limit | Purge leaves a usable secret in encrypted backups protected by the same password. **Reproduced:** the convenience backup revealed the purged fake key. `crates/akm-core/src/vault.rs:1230` | A deliberate backup-retirement or re-key workflow, with failure tests and honest limits for external copies. |
| 0.5 | Low | limit | The journal cannot establish completeness. **Reproduced:** removing its final complete record still made `audit --verify` succeed. `crates/akm-core/src/audit.rs:582` | An independently protected checkpoint that detects tail deletion and replacement. Another same-account writable file would not suffice. |

| Deduction from the last review | Fix assessment | Evidence |
|---|---|---|
| FV-06, redirected CLI export, 1.5 points | **Holds** | Holding the isolated mapping lock and substituting a junction now refused export, leaving neither plaintext nor temporary files in either folder. The before-write and after-publication handle checks also cover later redirection by inspection. `crates/akm-cli/src/export.rs:264` |
| FV-07, cleanup warning names another file, 1 point | **Holds** | A production-module probe moved the plaintext temporary file to a path exceeding 512 UTF-16 units and placed a sentinel at its original name. The warning named the moved file; the sentinel survived. `desktop/src-tauri/src/private_export.rs:263` |
| FR-06, backups retain purged keys, 0.5 points | **Does not hold; disclosed limit** | Backup recovery still returned the purged fake value. |
| FR-07, journal has no integrity anchor, 0.5 points | **Does not hold; disclosed limit** | Complete-record tail removal remained undetected. |

I also checked FV-21, which previously carried no security points. **Its fix holds:** beneath an Everyone-read parent, the fake vault, convenience backup and dated backup retained protected single-user DACLs at path lengths of 375, 379 and 512 characters.

**Area D: 11.5 / 15**

| Points deducted | Severity | Kind | Finding and evidence | What would win the points back |
|---:|---|---|---|---|
| 1.5 | Medium | defect | **Clipboard maintenance can block delivery of a native lock signal.** The Windows message thread calls `Session::tick`, which waits for the clipboard lease; a source-level reproduction blocked a copy, queued a lock behind that timer work, then successfully journaled, extracted and revealed a fake vault secret before the lock was processed. **Reproduced with a fake clipboard and serialized event queue; Windows wiring inspected.** `desktop/src-tauri/src/windows_session.rs:128`, `desktop/src-tauri/src/session.rs:544` | Keep clipboard waiting and I/O off the Windows notification thread. Add a regression that stalls a copy, delivers lock/suspend through the monitor’s dispatch path, and requires session invalidation before releasing the copy. |
| 1.0 | Medium | design | A compromised page can collect five single-key values per rolling minute without independent approval and renew availability through `vault_activity`. **Inspection:** this requires page compromise; I found no entry point for it. `desktop/src-tauri/src/session.rs:723` | Native authorization that arbitrary page requests cannot renew. Test distinct keys across successive rolling windows. |
| 0.5 | Low | limit | CLI injection still copies values into `std::process::Command`, whose environment storage is not zeroized. **Inspection.** `crates/akm-cli/src/child.rs:57` | Remove avoidable owned copies and verify cleanup, including failure paths. Retain the separate database, JavaScript and OS memory limitations. |
| 0.5 | Low | limit | `AKM_PASSWORD` remains in the supplying shell’s environment, available to its other children. **Inspection; selected-child scrubbing reproduced.** `crates/akm-cli/src/main.rs:1181` | An automation authentication path that does not retain the master password in the parent environment. The hidden prompt already avoids this for interactive use. |

The new monitor finding concerns a lock signal waiting **before native invalidation**, rather than cleanup continuing after a lock notice. In the control case, processing the queued lock immediately made the same disclosure fail.

| Deduction from the last review | Fix assessment | Evidence |
|---|---|---|
| FV-08, versioned interpreter controls, 2 points | **Holds for the established fault** | The release CLI refused versioned Lua names, including previously unlisted version suffixes, and additional Perl, Ruby and R startup controls without echoing their values. No interpreter hook execution was attempted. |
| FV-09, gradual page disclosure, 1 point | **Does not hold; disclosed design limit** | The renewable allowance and page-callable activity command remain. |
| FV-10, uncleared secret copies, 0.5 points | **Partly holds** | The focused dotenv allocator regression passed. The CLI command-environment copy remains, so the deduction stays. |
| FR-09, parent password environment, 0.5 points | **Does not hold; disclosed limit** | Child scrubbing passed; the parent exposure remains. |
| FV-11, `add --secret`, 0.5 points | **Holds** | Both `--secret value` and `--secret=value` were refused without printing the supplied fake value. |
| Native security proof gap, 1 point in the second reviewer’s raw result | **Does not hold; still unrun** | Real clipboard, native approval and Windows session proofs remain absent. As in the published review’s aggregation, this belongs to H/FV-14 and receives no duplicate deduction here. |

For the security-relevant test removals inspected, I established no separate lost-behaviour deduction. The remaining tests do not cover the monitor-thread blockage above; calling `Session::lock` from another thread does not establish that Windows notifications can reach it promptly.

**4. Findings outside C and D, without points**

**Medium, E, reproduced: desktop export can overwrite a newly arrived vault.** After `prepare_private` completed its final destination validation, I copied a valid fake KDBX to the previously absent `.env` destination. Validation then correctly refused that file, but `PreparedExport::publish` overwrote it because publication unconditionally permits replacement. `desktop/src-tauri/src/private_export.rs:284`, `desktop/src-tauri/src/private_export.rs:635`

A closing regression should preserve a file that appears or changes after validation, including a valid KDBX. Publication must carry and enforce the overwrite authority for the approved destination.

**5. What I did not cover**

I did not start windows, access settings or the real clipboard, change the Windows session, install anything, use the network, start workflows, or run full builds or suites. The monitor reproduction used production session code with a fake clipboard and simulated event queue; actual Windows notification delivery remains untested.

I did not audit cryptographic dependency internals, inspect memory dumps, exercise real cloud synchronization, or independently repeat the reported interoperability and GUI proofs. The export probe used production filesystem code with a path-rendering shim.

**6(a). Would I keep my own revocable API keys here today?**

**Yes, with named precautions, through the CLI:** a strong unique master password, private NTFS storage, prompt/stdin input, explicit scopes, trusted child programs and a separate encrypted backup. I would defer desktop clipboard use until the monitor defect is repaired and the owner-present security proofs pass. I would revoke keys at their providers before treating purge as retirement.

The first failure I expect a user to meet is a copied key remaining after a busy-clipboard cleanup attempt. This is a prediction, not a real-clipboard observation.

**6(b). Required workflows**

“Complete by inspection” means implementation exists, not that its real desktop journey passed.

| Required workflow | CLI | Desktop |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Complete by inspection and partial execution.** Ran create, add, list/get, metadata edit and purge; read selection code. | **Complete by inspection.** Took the project’s word for native unlock/list proof; broader management journey unrun. |
| Preserve metadata | **Complete by inspection and partial execution.** Checked username, tags and metadata editing; broader interchange is project-reported. | **Complete by inspection.** Read field handling; native preservation journey unrun. |
| Rotation tracking and recoverable history | **Partly complete.** Ran history after an edit and read rotation code. AKM exposes no old-value retrieval or version restore. | **Partly complete.** Read history mapping; the same recovery limitation remains. |
| Clipboard privacy and ownership-aware cleanup | **Missing**, deliberately; no CLI copy command. | **Partly complete.** Read publication/ownership code and reproduced monitor blockage with a fake backend. Real clipboard proof remains absent. |
| Reviewed import; scoped export/injection | **Partly complete.** No import command. Ran scoped export/injection, control-name refusals and junction substitution. | **Partly complete.** Read native import/export/launch code; reproduced the export destination race. Save proof is project-reported; import and terminal windows remain unrun. |
| Encrypted backups and a useful private journal | **Complete with disclosed limits.** Ran backup recovery, long-path ACL, journal truncation and damaged-mapping refusal probes. CLI journal recovery remains absent. | **Complete by inspection with limits.** Read callers; Activity proof is project-reported. |
| Defensive sync-folder use and stale-write refusal | **Complete at the stated local-refusal level, by inspection and reported tests.** No real sync-client probe. | **Complete at the same level.** Read shared persistence/session checks; no real sync-client probe. |

Native idle/session/suspend protection is additionally **partly complete** on desktop because of the monitor finding and missing real-session proofs. It does not apply to the short-lived CLI.

**7. Do the points say the same thing?**

They support cautious CLI use. They overstate readiness for daily desktop use: passing source checks cannot replace the unrun clipboard, approval and Windows-session journeys. Missing recovery workflows also matter more to a user than their small numerical deductions suggest.

Evidence is retained in `%TEMP%\akm-security-review2-8105fd2533894fe9b998433261a5bdea`, including the probe source (`%TEMP%/akm-security-review2-8105fd2533894fe9b998433261a5bdea/native_probe.rs`), native results (`%TEMP%/akm-security-review2-8105fd2533894fe9b998433261a5bdea/native-confirmed-results.txt`) and handoff receipt (`%TEMP%/akm-security-review2-8105fd2533894fe9b998433261a5bdea/handoff-receipt.json`). Observer registration was incompatible with the brief’s TEMP-only rule; the receipt records that integration gap.

Automatic approval review rejected temporary-folder removal with “blocked by policy.” The folder remains, approximately 8.5 MiB. Nothing was deleted; zero space was freed.

## Reviewer 3: security (C and D), second reviewer

Reviewed `51cbb0e`; product source matches `20b9359`. All three build hashes match the brief. The checkout remains unchanged. I ran 34 CLI checks across 23 invocations, four filesystem probes, and focused allocator probes.

**Area C: 14 / 15**

| Points | Severity | Kind | Finding | Evidence | What wins the points back |
|---:|---|---|---|---|---|
| −0.5 | Low | limit | Purging a key leaves its usable value in encrypted backups under the same master password. Backup creation remains at `crates/akm-core/src/vault.rs:1104`. | Reproduced. After live purge, `.bak` revealed the fake value. | A deliberate managed-backup retirement or re-key workflow, with failure tests and honest limits for external copies. |
| −0.5 | Low | limit | The journal cannot establish completeness. Its reader accepts EOF without an independently protected checkpoint at `crates/akm-core/src/audit.rs:582`. | Reproduced. Removing the final complete record still yielded “consistent” verification. | Demonstrate detection of tail removal and chain replacement using an independently protected checkpoint. |

| Last review deduction | Assessment | Evidence |
|---|---|---|
| FV-06, substituted export directory, 1.5 | **Holds.** | A new probe substituted a grandparent junction during temporary-file creation. Export refused before writing plaintext and removed its empty temporary file. Windows also refused a later parent move while the temporary file was open; cleanup removed the plaintext. |
| FV-07, cleanup warning identifies another file, 1.0 | **Holds.** | A renamed desktop remnant beyond 512 path characters was identified through its handle; the replacement sentinel survived. A CLI current-name failure refused writing and gave no deletion instruction for the cached name. |
| FR-06, backups retain purged keys, 0.5 | **Does not hold; disclosed limit.** | Backup recovery still returned the purged fake value. |
| FR-07, journal lacks an integrity anchor, 0.5 | **Does not hold; disclosed limit.** | Complete-tail removal remained undetected. |

FV-21, previously unscored, also holds against my new scenario. In a fixture granting inheritable Everyone-read access, the vault, convenience backup and both 311-character dated-backup paths had protected, single-user ACLs.

**Area D: 13 / 15**

| Points | Severity | Kind | Finding | Evidence | What wins the points back |
|---:|---|---|---|---|---|
| −1.0 | Medium | design | A compromised page can obtain five single-key disclosures per rolling minute and renew activity indefinitely. The allowance remains at `desktop/src-tauri/src/session.rs:723`, with activity renewal at line 697. | Inspection. Conditional on page compromise; I found no compromise entry point. | Independent authorization that the page cannot renew. Test paced extraction across multiple minute boundaries. |
| −0.5 | Low | defect | Failed `.env` parsing frees secret-bearing strings without clearing them. Partial results at `crates/akm-core/src/dotenv.rs:103` and the quoted-value rejection at line 158 escape the caller’s successful-result cleanup. | Reproduced using the current parser and an allocator observer. A malformed later line and an invalid quoted tail each freed one allocation still containing the fake secret. | Clear partial results and rejected quoted values. Add allocator regressions for both failure paths. |
| −0.5 | Low | limit | `AKM_PASSWORD` remains in the supplying shell’s environment. Reading it at `crates/akm-cli/src/main.rs:1182` cannot clear that parent exposure. | Inspection. Selected-child scrubbing passed independently. | Provide automation authentication that does not retain the password in the parent environment. |

The parser finding continues the existing FV-10 memory-cleanup fault class. I deduct for that class once.

| Last review deduction | Assessment | Evidence |
|---|---|---|
| FV-08, versioned Lua controls, 2.0 | **Holds.** | The release CLI refused `LUA_INIT_5_5`, `LUA_PATH_5_3` and `LUA_CPATH_5_2`, without leaking the fake value or changing the vault. The prefix rule also covers legacy planning by inspection. Actual Lua execution was not run. |
| FV-09, gradual page disclosure, 1.0 | **Does not hold; disclosed limit.** | The renewable allowance and page-controlled activity remain. |
| FV-10, uncleared plaintext copies, 0.5 | **Partly holds.** | The escaped-line renderer control freed no uncleared marker allocation. Parser error paths still did. The disclosed `std::process::Command` environment copy also remains at `crates/akm-cli/src/child.rs:55`. |
| FR-09, parent password environment, 0.5 | **Does not hold; disclosed limit.** | Parent exposure remains; child scrubbing works. |
| FV-11, `add --secret`, 0.5 | **Holds.** | Both separated and `--secret=value` forms were refused without echoing the fake value or changing the vault. |
| Native-proof gap, raw 1.0 | **Does not hold.** | The required owner-present paths remain unrun. This deduction belongs to H, as the last review’s counted result specifies; I do not deduct it again. |

I inspected the test-removal commits and remaining behavioral coverage. I established no security regression caused by those removals.

**4. Findings outside C/D, without points**

Existing F completion gaps remain by inspection. The CLI has no import or copy command. History exposes descriptions, without retrieving or restoring an old value inside AKM; that requires another KDBX reader or a backup. See `crates/akm-cli/src/main.rs:250` and `desktop/src-tauri/src/commands.rs:520`.

**5. What I did not cover**

I ran no desktop app, real window, clipboard operation, settings access, OS-session change, installer, network check, workflow, full build or full suite. I did not audit cryptographic dependencies, inspect a live memory dump, execute Lua startup hooks, or exercise live cloud synchronization. Supplied suite, interchange and real-window results remain project-reported evidence.

Filesystem probes used production modules with injected failures and simple error/path-presentation stubs. They did not exercise Tauri transport.

The retained probe folder (`%TEMP%/akm-security-review-177978b439e1`) contains sources, evidence, compiled probes and fake fixtures, approximately 35 MiB. Nothing was deleted; **0 bytes freed**. Its handoff receipt (`%TEMP%/akm-security-review-177978b439e1/handoff-receipt.json`) records the checks and shutdown status. Shared-observer integration was unavailable because it requires artifacts inside the project, while this brief permits probe files only under TEMP.

**6(a). Would I keep my own revocable keys here?**

**Yes, with named precautions, for limited CLI use.** I would use private NTFS storage, a strong unique password, prompt/stdin input, trusted child programs, explicit scopes and an independent encrypted backup. I would revoke keys at their providers before treating purge as retirement, and defer desktop disclosure and clipboard-dependent use until the native proofs pass.

The first desktop security failure I expect is a copied value surviving an unsuccessful busy-clipboard cleanup, particularly during exit. That is a prediction from inspected handling, not a real-clipboard observation.

**6(b). Required workflows**

“Complete” below describes implementation within its stated limits, not completed native acceptance.

| Required workflow | CLI | Desktop |
|---|---|---|
| Create, unlock, search, inspect and manage | **Complete.** Ran creation, add, inspection, edit and purge; read selectors and remaining commands. | **Complete by inspection.** Unlock/list proof is project-reported; other real-window management flows were not run. |
| Preserve metadata | **Complete by inspection/reported interchange.** Ran a metadata-only edit. | **Complete according to inspected code and reported preservation tests.** Native journey unrun. |
| Rotation and recoverable history | **Partly complete within AKM.** Read commands and took reported rotation proof; old-value recovery requires another reader or backup. | **Partly complete within AKM.** Read native history representation; same recovery limitation. |
| Copy with privacy exclusions and ownership-aware cleanup | **Missing intentionally.** Command inventory inspected. | **Complete in inspected code.** Real clipboard behavior remains unproved. |
| Reviewed import; scoped export/injection | **Partly complete.** Import absent. Ran scoped file export and child injection. | **Complete in inspected code.** Save proof is project-reported; native import and terminal confirmations remain unrun. |
| Encrypted backups and useful private journal | **Partly complete.** Ran backup permissions, purge recovery, journal minimization and disclosure refusal. CLI journal recovery is absent. | **Complete in inspected callers/reported tests.** Activity proof is project-reported; recovery window unrun. |
| Defensive downloaded-sync-folder use | **Complete within disclosed local-refusal limits.** Read fingerprint and replacement checks; no live sync run. | **Complete on the same shared-core basis.** No independent live sync proof. |

Native idle, Windows-lock and suspend protection is implemented by inspection. Its real Windows delivery remains unproved.

**7. Do the points agree with 6(a)?**

Only conditionally. They support cautious CLI use. They overstate confidence in everyday desktop use because repaired source and mocked tests do not establish the important Windows behaviors that nobody ran.

Automatic approval review rejected the cleanup command with “blocked by policy” and supplied no more specific reason. The probe folder remains as permitted by the brief.

## Reviewer 4: data integrity and correctness (E and F)

Reviewed `51cbb0e`; product code matches `20b9359`. All three build hashes match the brief. I ran 59 isolated release-CLI calls and small source/core probes with fake data. The checkout is unchanged.

**Area E: 13.5 / 15**

| Points | Severity | Kind | Finding and evidence | What would win the points back |
|---:|---|---|---|---|
| −1.5 | Medium | defect | **FV-02 partly remains.** Native conversion trims untouched metadata at `desktop/src-tauri/src/views.rs:169`. A notes-only update changed project `" demo "` to `"demo"`, environment `" dev "` to `"dev"`, and padded username, issuer, URL and scope values. This can silently move a key between exact scopes. **Reproduced** through the production conversion, core save and reopen; no desktop window ran. | Preserve untouched values through the native boundary. Add a regression that submits a notes-only edit, saves, reopens and compares every other field exactly. |

| Last review’s deduction | Assessment | Evidence |
|---|---|---|
| FV-02: untouched metadata changes, 2 points | **Partly holds** | Exact expiry, comma-containing scopes and rotation interval survived my probe. Whitespace preservation still fails after native conversion. I return 0.5 point. |
| FV-03: symbolic-link saves split the vault, 1 point | **Holds by inspection** | `crates/akm-core/src/vault.rs:78` refuses the link, with another check immediately before replacement. My real file-link attempt remained unrun because Windows returned error 1314. |

Additional probes upheld exact import isolation, unchanged-import no-ops, stale-write refusal, rollback after a blocked replacement, hard-link refusal and backup sentinel preservation. Damaged-journal and two-vault mapping recovery preserved the damaged and unrelated bytes. The FV-22 combined KDF bound also held in a header-only probe; no expensive derivation ran.

**Area F: 8.75 / 10**

| Points | Severity | Kind | Finding and evidence | What would win the points back |
|---:|---|---|---|---|
| −0.5 | Low | missing | **Earlier values cannot be recovered within AKM.** `crates/akm-core/src/vault.rs:703` returns times and field names; neither surface exposes or restores a historical value. **Inspection.** This is existing FV-23, previously unscored. KeePassXC provides an escape route, but completing the required recoverable-history workflow requires another application. | Provide deliberate historical-value recovery or restoration. Verify an earlier value after rotation and reopening, while preserving unrelated data. |
| −0.25 | Low | defect | **Recycled UUID selection differs from live selection.** `crates/akm-core/src/vault.rs:570` compares the UUID’s text literally. **Reproduced:** `restore <UPPERCASE-UUID>` failed; the same lowercase UUID restored the key. | Parse UUIDs consistently in live and recycled selectors. Test uppercase UUIDs through recycled history, restore and purge. |
| −0.25 | Low | defect | **Desktop search omits username.** The searchable fields at `desktop/src/rowFacts.ts:204` exclude it. **Reproduced through the production search helper:** searching the stored access-key username returned no match. No browser ran. | Include username and test the visible key-list result. |
| −0.25 | Low | defect | **Desktop search still diverges from AKM’s caseless rule.** `desktop/src/components/KeysView.tsx:103` and the search helper use ordinary lowercase conversion. **Reproduced through the helper:** title `Σ` did not match query `ς`, although CLI selection did. | Apply the established caseless rule to both search operands and add non-ASCII search regressions. |

| Last review’s deduction | Assessment | Evidence |
|---|---|---|
| FV-04: no CLI metadata edit, 1 point | **Holds** | Release-CLI edits preserved identity, secret and rotation time; unnamed fields survived, optional fields cleared, invalid changes refused, and a repeated edit left vault bytes unchanged. |
| FV-05: desktop rotation bound mismatch, 0.25 point | **Holds** | `desktop/src/components/KeyForm.tsx:42` now permits 36,500 days. That value survived native conversion and save/reopen; form bounds were inspected. |

Earlier CLI regressions for NUL rejection, BOM handling, fractional-expiry refusal, Unicode title selection and extreme day counts also held.

**4. Findings outside my areas**

None newly established. I have not charged the owner-present proof gaps again in E or F.

**5. What I did not cover**

No full builds or suites, real desktop workflows, clipboard, saved settings, native dialogs, terminal windows, OS-session tests, installation or network activity. Interoperability results remain project-reported. Native conversion probes used current production source linked to existing local core libraries; they were not real Tauri-handler execution.

I inspected the test-removal records and relevant surviving tests. I established no separate deduction caused by those removals. The metadata regression still stops short of the native preservation boundary.

**6(a). Would I keep my own revocable keys here?**

**Yes, with named precautions, considering E/F alone:** an independent encrypted backup, a direct vault-file path, CLI metadata edits until FV-02 is fully repaired, and a tested KeePassXC recovery route.

The first failure I expect is searching by a stored access-key username and getting no result.

**6(b). Required workflows touching E/F**

“Complete” describes available functionality within documented limits, rather than completed desktop acceptance.

| Workflow | CLI | Desktop |
|---|---|---|
| Create, unlock, search, inspect and manage | **Complete for ordinary paths.** Ran creation, inspection, editing and lifecycle operations; recycled UUID variant fails as above. | **Partly complete.** Read management code; source search probes found omissions. Real unlock/list proof is project-reported. |
| Preserve metadata | **Complete for sampled paths.** Ran edits and readback. Broader interchange is project-reported. | **Partly complete.** Reproduced native trimming after save/reopen. |
| Rotation and recoverable history | **Partly complete.** Ran rotation and history; recovering earlier values requires KeePassXC or a backup. | **Partly complete.** Read code and checked shared storage behavior; no historical-value recovery within AKM or real-window rotation proof. |
| Reviewed scoped import | **Missing.** Read command map. | **Partly complete.** Ran core isolation and read native approval/write code; real confirmation unrun. |
| Scoped export and injection | **Complete for sampled paths.** Ran both, including child exit-code preservation and `AKM_*` removal. | **Partly complete.** Save proof is project-reported; terminal code inspected, real terminal unrun. |
| Encrypted backups and useful private journal | **Complete within disclosed limits.** Ran retention sentinels, verification and core recovery. CLI recovery itself remains absent. | **Complete in code, partly proved.** Core recovery ran; Activity proof is project-reported; real recovery dialog unrun. |
| Conflict detection and stale-write refusal | **Complete within local-lock limits.** Ran stale-write refusal, rollback and reload. | **Complete in shared core code.** Native desktop conflict journey unrun. |

Clipboard behavior was outside my assessment.

**7. Do the points agree with 6(a)?**

Only partly. They support cautious use of the tested CLI paths. They overstate readiness if interpreted as proof of daily desktop operation, complete recovery within AKM, or the unrun native workflows.

Evidence and handoff receipt (`%TEMP%/akm-data-review-20261113-a9be7c/receipt.json`) remain in the named temporary folder, about **87.5 MiB**. Shared registration rejected the temporary root; shared handoff then failed because no registry record existed.

Automatic approval review rejected deletion of the six task-owned probe binaries and symbol files with **“blocked by policy.”** Nothing was deleted; **0 bytes freed**.

## Reviewer 5: desktop quality (G)

Area G: 9.5 / 10.

Reviewed `51cbb0e`, whose product code matches `20b9359`, and the supplied built page. Browser reproductions used headless Edge on loopback with fake data and my own native mock.

| Points | Severity | Kind | Finding and evidence | What wins the points back |
|---:|---|---|---|---|
| 0.50 | Low | defect | Enlarged text pushes drawer actions offscreen. At 1180 × 800, changing the root font from 16 to 24 px puts Edit entirely outside the viewport; at 32 px, Close is outside too. The drawer remains 540 px wide and its footer does not wrap. `desktop/src/styles.css:528`. **Reproduced with synthetic text sizing.** | Let the footer wrap or size the drawer with the text. Test every drawer action at 900 and 1180 px with 150% and 200% text, without horizontal scrolling. |

The deductions of the last review:

| Previous deduction | Result | Evidence |
|---|---|---|
| FV-12, missing username controls, 0.50 | **Holds** | The drawer displayed a directional mark visibly. The edit form showed the stored username, dispatched a replacement and dispatched an empty string when cleared. Native normalization is a separate E finding below. |
| FV-13, stale Keys after import, 0.50 | **Holds** | Refresh worked through the reply, a retained outcome and an outcome without a page receipt. Further probes rejected an older list reply, refreshed a failed list and cleared an open reveal after import. |

4. Findings outside G, with no points.

- **Medium, defect, E. FV-02 partly holds.** The frontend now preserves untouched values, but the native adapter trims them. A notes-only draft retained project `" demo "` and environment `" dev "`; passing it through the actual production adapter, saving a fake vault and reopening changed them to `"demo"` and `"dev"`. Username, issuer and scopes were also trimmed. This changes the exact scope used for export and injection. `desktop/src-tauri/src/views.rs:169`. **Reproduced using the production adapter and an existing core library, without Tauri.** Close it with a native update regression that edits only Notes and compares every untouched field after reopening.
- **Low, defect, E/F.** A comma-bearing scope survives an unrelated edit but cannot be safely extended in the form. Stored scopes `["resource,subresource", "read"]` display as `resource,subresource, read`; appending `, write` dispatches four scopes instead of preserving the original item. `desktop/src/components/KeyForm.tsx:40`. **Reproduced at the outgoing IPC boundary.** Use an unambiguous list editor and test appending an item while preserving existing comma-bearing items.

5. What I did not cover.

I did not run the CLI binary, native app, installer, full builds or full suites. I did not access settings, real vaults, the clipboard or actual OS-session events. Native dialogs, terminal delivery, idle locking, suspend, screen readers and real Windows text sizing/high contrast remain unproved here. Supplied-page build provenance was taken from the brief.

Sampled create/add/reveal, rotation/history, Recycle Bin, scoped export/launch, error recovery, keyboard focus, distinguishing descriptions and forced-colour boundaries worked. I inspected the test-removal commits and surviving coverage; I established no additional G deduction from those removals.

6(a). Would I keep my own revocable API keys in this build today?

**No, through the desktop app yet.** I would first fix native metadata preservation and complete the owner-present disclosure tests. The first routine failure I expect for an affected entry is a notes edit silently moving it from a whitespace-bearing project/environment scope to another scope.

6(b). Required workflows.

“Complete” describes an available workflow, not completed native acceptance.

| Required workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Complete** for reported journeys; took the project’s word. | **Complete in implementation.** Ran mocked create/add/reveal and inspected management code. |
| Preserve metadata | **Complete** for reported ordinary cases; not independently run. | **Partly complete.** Ran the native-adapter save/reopen reproduction above. |
| Rotation tracking and recoverable history | **Partly complete.** Reported rotation/history/bin journeys exist; old-value retrieval and version restoration require another tool or backup, FV-23. | **Partly complete** for that same recovery limitation. Ran mocked rotation/history and inspected recovery controls. |
| Copy with privacy exclusions and ownership-aware cleanup | **Missing.** No copy command; inspected the command list. | **Complete in code.** Inspected the path; real clipboard proof remains unrun. |
| Reviewed import; scoped export and injection | **Partly complete.** Export/run are reported proved; no import command. | **Complete in implementation.** Ran mocked import completion, preview/save cancellation and scoped terminal dispatch; native confirmations remain unrun. |
| Encrypted backups and useful private activity journal | **Complete** according to reported proofs and inspected code. | **Complete in implementation.** Inspected the views and took the focused native proofs from the project. |
| Detect conflicts and refuse stale writes in downloaded sync folders | **Complete in code** according to reported tests. | **Complete in code.** Inspected refusal/reload handling; no live sync race run. |

7. Do the points say the same thing as 6(a)?

No. The 9.5 credits desktop presentation and implemented workflows. It excludes the separately assigned metadata defect and cannot turn mocked journeys into native Windows proof. Read alone, it overstates readiness.

The checkout is unchanged. Evidence remains in the probe folder (`%TEMP%/akm-review-g-42efa3a1186d4cb497f6b5ec75917736`), with a local handoff receipt (`%TEMP%/akm-review-g-42efa3a1186d4cb497f6b5ec75917736/handoff-receipt.json`). Browsers and loopback servers closed; no matching probe process remained. Shared-observer registration requires an in-project artifact root, which conflicts with the brief’s TEMP-only writes.

Automatic approval review rejected deletion of the probe executable, debug symbols and browser log with “blocked by policy,” without a further reason. They remain in the named folder. Deleted nothing; freed 0 bytes.

## Reviewer 6: verification and shareability (H and I)

Reviewed `51cbb0e`, with builds attributed to `20b9359`. All three executable hashes match the brief. The checkout remained unchanged.

Area H: 3.25 / 5

| Points | Severity | Kind | Finding | Evidence | What wins the points back |
|---:|---|---|---|---|---|
| 1.00 | Medium | proof | Real clipboard, native import and terminal confirmations, hide, OS lock/suspend and idle behavior remain unrun. The production-handler harness uses a mocked runtime. `docs/STATE.md:92`. | inspection | Complete the required Windows proofs on identified builds, including an accepted observation method for R4-04. |
| 0.25 | Low | defect | The verification helper corrupts arguments requiring Windows backslash escaping. Passing `C:\fake folder\` to `edit --notes` returned success but stored `C:\fake folder"`; direct CLI invocation preserved the intended value. `.agents/skills/verify-akm/scripts/akm-verify.ps1:109`. | reproduced | Correct argument encoding and add a helper-to-binary regression comparing exact readback for spaces, trailing backslashes and embedded quotes. |
| 0.25 | Low | release | No CI run validates this revision or the changed workflows. Dependency checks are configured, but local results do not establish runner behavior. `docs/STATE.md:91`. | inspection | A successful complete CI run on the reviewed revision. |
| 0.25 | Low | limit | Independent matching rebuilds remain unproved. The comparison is reporting-only and has never run. This is disclosed. `docs/TESTING.md:66`. | inspection | Demonstrate independent matching rebuilds and enforce the comparison. |

The deductions of the previous 13 November review:

| Previous deduction | Fix verdict | Evidence |
|---|---|---|
| FV-14, native proof gaps, 1.00 | Does not hold; no fix claimed | The consequential Windows paths remain unrun. Focused journal/export proofs do not cover them. |
| FV-15, disabled workflow evidence, 0.25 | Holds | Ten new probes covering disabled jobs and steps, Boolean/string conditions and expression spellings were rejected. Echoed command text still counts, within the narrowed documented promise. |
| FV-16, long-path cleanup, 0.25 | Holds | Windows PowerShell 5.1 removed a session containing a 374-character path and its state record. A sentinel reached through a nested junction survived. |
| FV-17, missing CI execution, 0.25 | Does not hold; no fix claimed | CI has not run since 8 November, as the brief permits me to count. |
| FV-17, unproved reproducibility, 0.25 | Does not hold; no fix claimed | No independent comparison result exists; differences still do not fail CI. |

I sampled the test-removal changes and retained behavioral assertions, including secret comparisons for import isolation and rollback, and complete child-environment comparisons across scopes. I established no additional removal-specific deduction. The structural rules now assigned to review remain less automatically protected; I did not perform an exhaustive mutation audit.

Area I: 3.00 / 5

| Points | Severity | Kind | Finding | Evidence | What wins the points back |
|---:|---|---|---|---|---|
| 1.00 | Medium | release | The builds remain unsigned, and the manual channel has neither a release key nor a signed release. Recipients cannot authenticate the installer or a later security fix. `docs/RELEASING.md:52`. | inspection | An authenticated release with separately obtainable verification material and a tested verification procedure. |
| 0.50 | Low | release | Clean-computer install, reinstall and uninstall remain unrun. The prepared job covers same-version reinstall, not migration from an earlier version. `docs/TESTING.md:160`. | inspection | Run the clean-machine preservation proof; add migration coverage when an earlier release exists. |
| 0.50 | Low | release | Publication privacy remains unresolved. The current-tree scan refused publication with 50 hits in 12 files, including record files and the newer review briefs. `docs/FINDINGS.md:405`. | reproduced | Resolve the hits, inspect the resulting parentless snapshot and publish only cleared history. |

The deductions of the previous 13 November review:

| Previous deduction | Fix verdict | Evidence |
|---|---|---|
| FV-18, release authentication, 1.00 | Does not hold; no fix claimed | Builds remain unsigned; the release-key section remains empty. |
| FV-19, installation proof, 0.50 | Does not hold; no fix claimed | Scripts exist, but no clean-machine run exists. |
| FR-26/FV-19, publication privacy, 0.50 | Partly holds | The scan fails closed and avoids publishing old history. Current-tree clearance remains unfinished, reproduced with 50 hits. |
| FV-20, licensing files absent from installer, 0.25 | Holds | Bundle resources name both files. The generated NSIS script includes both, and staged resource bytes match the checkout. A new nested-notice fixture also retained its text and author. Installer execution remains unproved under FV-19. |

4. Findings outside my areas, with no points.

Known FV-23 remains by inspection. History lists times and changed fields, but AKM provides neither historical values nor version restoration. Recovery requires KeePassXC or a backup. `crates/akm-cli/src/main.rs:310`. This limits the required recoverable-history workflow. I established no new outside-area finding.

5. What I did not cover.

I ran the fast documentation check, which passed for 40 claims; focused checker, cleanup, packaging and notices probes; the publication scan; and 20 direct release-CLI commands. Those CLI checks covered metadata readback, scoped export/injection, `AKM_*` scrubbing, child exit code 37, rotation, history, Recycle Bin, restore, purge and journal verification.

I did not run full builds or suites, interchange suites, GUI or installer flows, clipboard or OS-session tests, network checks, a screen reader, or a legal review. Existing full-suite, interchange and real-window results remain project-reported. Matching build hashes establishes artifact identity, not provenance.

6. Answers with no points.

(a) **No for the intended daily desktop use today.** The disclosure and locking paths still lack real Windows proof. The first obstacle I expect a new recipient to meet is authenticating the installer or a subsequent update.

(b) These judgments include verification completeness.

| Required workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage entries | Complete for sampled ordinary paths. Ran create/add/list/inspect/edit/delete/restore/purge; remaining selection behavior inspected or project-reported. | Partly complete. Focused unlock/list proofs are project-reported; the complete real-window journey remains unrun. |
| Preserve metadata | Complete for sampled paths. Ran metadata readback and inspected retained regressions; interchange results are project-reported. | Partly complete. Mock/native test evidence is project-reported; real editing and preservation remain unrun. |
| Correct rotation and recoverable history | Partly complete. Ran rotation/history; historical-value recovery remains external to AKM, FV-23. | Partly complete. Read the history presentation; FV-23 remains and no real-window rotation journey exists. |
| Privacy-aware copy and cleanup | Missing, intentionally desktop-only. | Partly complete. Scripted coverage exists; real clipboard proof remains absent. |
| Reviewed `.env` import | Missing, intentionally desktop-only. | Partly complete. Read scripted native-confirmation coverage; the real confirmation remains unrun. |
| Explicitly scoped export and injection | Complete for sampled paths. Ran export and a child receiving only the selected scope, with scrubbing and exit propagation. | Partly complete. Save `.env` is project-reported proved; terminal and native approval journeys remain unrun. |
| Encrypted backups and a useful private journal | Complete by sampled journal execution, inspected backup regressions and reported interchange results. | Partly complete. Activity is project-reported proved; broader native failure/recovery journeys remain unrun. |
| Downloaded sync folders: conflicts and stale-write refusal | Complete by inspected automated coverage and project reports; concurrent writers were not independently run. | Partly complete. Automated evidence exists; the real desktop conflict journey remains unrun. |

7. Do the points agree with 6(a)?

Only partly. They recognize repaired tools and packaging, while several consequential Windows behaviors remain unknown. The points overstate desktop readiness by assigning those unknowns one bounded deduction. Release deductions also matter less to an owner using a locally built CLI than to another person downloading AKM.

The helper removed two owned fake-vault sessions and their state records; freed space was not measured. Automatic approval review blocked final probe-folder deletion with “blocked by policy” and no more specific reason. The probe folder (`%TEMP%/akm-hi-review-522bc6c77f104cc9b13d0e590c6dede7`) remains with a local handoff receipt (`%TEMP%/akm-hi-review-522bc6c77f104cc9b13d0e590c6dede7/final-handoff.json`). Observer registration rejected its location outside the project; handoff then failed because no registry record existed.
