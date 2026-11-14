# Third full review, 14 November 2026

- Model: Codex GPT-6.1 Sol, effort max ([REVIEWING.md](../REVIEWING.md)).
- Reviewed commit: `dff3d5c3ec8da22b93e80ec375562a0467c55936` on branch `gate/integration`. Its product code equals `4436dee`; the later commit changes documents only.
- Builds of `4436dee` (unsigned, SHA-256): `akm.exe` `50883615ad58ca527d6188ab8294d7a8f6f5ae5f51e98189becd37a1466ba391`; `akm-desktop.exe` `6f1648f53a428466e9b1c59ade5c6cbe41cabacc03457c51d41740d07d8991c1`; setup program `bc3c3fc087eed482ca602344550c3b819e63bad4cd6612f43bbb10dc05651773`.
- Date: 2026-11-14.
- Rubric: the nine areas and 100 points of the [baseline review](2026-10-30-baseline.md), with the scale of the two full reviews of 13 November.
- Brief: [2026-11-14-full-review-3-brief.md](2026-11-14-full-review-3-brief.md).
- Reviewers: six new independent reviewers, each with its own checkout and no view of the others' work. Two of them reviewed the security areas (C and D).
- What changed since the second full review: fix rounds 2 and 3 for its findings and for the findings of one diff review (the FW, D2A and D2B rows of [FINDINGS.md](../FINDINGS.md)).

## Result

**91.75 / 100. Above the bar of 90/100.** The release gate has two parts: this score, and then the owner's own testing. The second part is not done, so real secrets still wait.

| Area | Points | Second review | Now | Reviewer | Main deductions |
| --- | ---: | ---: | ---: | --- | --- |
| A. Premise and positioning | 10 | 10 | 10 | Architecture and premise | None |
| B. Architecture and design | 15 | 14 | 14.75 | Architecture and premise | A Save .env that ends after a reload of the page leaves no notice on the new page (a disclosed limit) |
| C. Security at rest | 15 | 14 | 14 | Both security reviewers | Two disclosed limits: a purged key stays in backups; the journal has no anchor |
| D. Security in use | 15 | 11.5 | 13 | Both security reviewers | A script in the page can read keys slowly; the copy in the child's environment; `AKM_PASSWORD` in the parent shell |
| E. Data integrity and durability | 15 | 13.5 | 14.75 | Data and correctness | An edit invents a creation time for a key of another program that has none |
| F. Functional correctness | 10 | 8.75 | 9 | Data and correctness | No old value in the history; the username `api-key` reads as empty; a desktop import cannot write to a scope with outer spaces |
| G. Desktop app quality | 10 | 9.5 | 9.25 | Desktop quality | With enlarged text at the smallest window, buttons of the rail overlap and one status text is cut |
| H. Verification and hygiene | 5 | 3.25 | 3.5 | Verification and shareability | The owner-present proofs were not run; no CI run; no independent rebuild |
| I. Shareability | 5 | 3 | 3.5 | Verification and shareability | Unsigned and no release key; no clean-computer install |
| **Total** | **100** | **87.5** | **91.75** | | |

### How the total was counted

- **Two security reviewers.** The rule is the one of the two earlier full reviews: the gate number uses the lower of the two security results, and both are recorded. This time both gave C 14 and D 13 (27 of 30), with the same five deductions. Neither established a new security defect.
- **No root cause is counted twice.** Both security reviewers named the unrun owner-present proofs and left the deduction in H.
- No points were added for any other reason. A finding that a reviewer reported outside its own areas has no points.

### What the score does not hold

- **The margin is small.** The total is 1.75 points above the bar. Each area except C and D has one reviewer, and in the second full review the two security reviewers differed by 1.5 points on the same code. So another reviewer set could give a total below 90.
- **Each review still finds defects that the one before did not see, but fewer.** The new defects cost 6.75 points in the first full review, 5.5 in the second and 1.25 in this one. The four of this review are all Low: two in the layout of the page with enlarged text, and two in the core (one of them in code that fix round 3 added).
- **Every fix that a reviewer attacked held, except one that holds in part.** The edit rule (FW-03, contract 13) keeps every stored value that the reviewer tried, but an edit gives a creation time to a key that had none.
- **The journal recovery.** Four reviewers judged D2A-01 and D2A-02 (a same-user program swaps files while a recovery runs) as disclosed limits inside the trust boundary, and made no deduction.
- **Findings with no points.** Five reviewers named FV-23 again outside their areas (no old value and no version restore inside AKM). The data reviewer scored it in F.
- **The removed tests.** All six reviewers inspected or sampled the test removals. None made a deduction for them.
- **Completion.** The CLI has no import, no copy and no journal recovery. Neither surface shows or restores an old value of a rotated key.
- **Behaviour on a real desktop.** No reviewer started the app. Nobody ran these: the real clipboard test, the full native GUI proof, the native import and terminal windows, R4-04, a real OS lock, suspend and idle wait, a real lock while a copy stalls, the replace prompt of the real save window, install on a clean computer, real high-contrast themes, a real screen reader, real text scaling.

### Kinds of the deductions

The 8.25 points that the reviewers deducted, by the kind that each reviewer gave:

| Kind | Points | Second review | First review |
|---|---:|---:|---:|
| `defect`: the code does a wrong or unsafe thing | 1.25 | 5.5 | 6.75 |
| `limit`: a weakness that the documents disclose | 2.75 | 2.25 | 2.75 |
| `release`: signing, licence, CI, install, publication | 1.75 | 2.25 | 2.5 |
| `design`: a design or threat-model choice | 1.0 | 1.0 | 2.0 |
| `proof`: nobody ran it on the real surface | 1.0 | 1.0 | 1.0 |
| `missing`: a required workflow is absent on a surface | 0.5 | 0.5 | 1.5 |

Of the 8.25 points, 3.0 need the owner (the desktop proofs, CI, signing, the install test), 3.75 are disclosed limits or the one design choice, 0.5 is the missing old value, and 1.25 are the four new defects.

### The reviewers' own answers

Each reviewer answered two questions with no points: would it keep its own revocable API keys in this build today, and do its points say the same thing.

| Reviewer | Area score | Keep its own keys in this build? | Do the points say the same? |
|---|---:|---|---|
| Architecture and premise | 24.75 / 25 | Yes, with precautions, for limited CLI use; daily desktop use needs the owner-present proofs | Only partly |
| Security, first | 27 / 30 | Yes, with precautions, through the CLI; desktop disclosure and clipboard use after the proofs pass | They support cautious CLI use, and overstate the desktop app |
| Security, second | 27 / 30 | Yes, with precautions, for limited CLI use | Only conditionally |
| Data and correctness | 23.75 / 25 | Yes, with precautions, through the CLI | Mostly, for cautious CLI use |
| Desktop quality | 9.25 / 10 | No, for daily desktop use yet | Only partly |
| Verification and shareability | 7 / 10 | No, for daily desktop use | Only partly |

The answers are the same as in the second full review: four reviewers would use the CLI, and two would not use the desktop app for daily work. Every reviewer wrote again that its points overstate the readiness of the desktop app, because the points cannot stand for the flows that nobody ran on a real Windows session. The higher score did not change these answers. Only the owner-present tests can.

### Limits of this score

- A score from one review model, not a professional audit.
- Each area except C and D has one reviewer.
- Reviewers read code and ran small probes with fake data. The suite and real-window results that they saw are reported by the implementer. Each reviewer matched the three build hashes.
- The scale follows the two full reviews of 13 November, with the same model and effort.

The text below is each reviewer's, except that links to a reviewer's local checkout are written as repository paths, paths in a reviewer's temporary folder start at `%TEMP%`, and the build folder is written `<build>`. Some reviewers left a probe folder in place, because the tool policy refused the deletion.

---

## Reviewer 1: architecture and premise (A and B)

Reviewed `dff3d5c`, with builds attributed to `4436dee`. All three executable hashes match the brief. The checkout is unchanged.

**Area A: 10 / 10**

| Points deducted | Severity | Kind | Finding | Evidence | What wins points back |
|---:|---|---|---|---|---|
| 0 | None | None | No deduction established. The narrow Windows/KeePass positioning accurately qualifies the experimental writer, unaudited integration and trusted-account boundary. | Inspection | Not applicable. |

| Last review’s deduction | Assessment | Evidence |
|---|---|---|
| None; A received 10/10 | Not applicable | The positioning remains appropriately limited. |

**Area B: 14.75 / 15**

| Points deducted | Severity | Kind | Finding | Evidence | What wins points back |
|---:|---|---|---|---|---|
| 0.25 | Low | limit | **AB3-01. Save .env can finish after a webview reload, but the new page cannot recover its completion notice.** Native export has no operation ID at `desktop/src-tauri/src/commands.rs:973`, and its page receipt exists only in memory at `desktop/src/native.ts:74`; the `docs/design/commit-point.md:221`, leaving manual inspection of the chosen file and Activity. | Inspection | Give `env_save` a native operation ID and outcome guard. Test lost replies and page reloads before and after publication; the new page must receive a truthful, secret-free outcome. |

| Last review’s deduction | Assessment | Evidence |
|---|---|---|
| FW-02, interrupted mapping recovery silently splits history, −1.0 | **Holds** | Recovery now copies the damaged file before one final replacement at `crates/akm-core/src/audit.rs:324`. The focused interruption regression passed. Release-CLI probes refused disclosure from the old missing-mapping state; recovery preserved old journal bytes and produced an explicit gap record visible to a fresh process. |

The stale-writer probe also preserved both disk and the refused reader’s snapshot; reload adopted the winning change. An unseen replacement clone produced an unconfirmed-lineage warning.

I assessed D2A-01 and D2A-02 as disclosed limits, not repaired defects. Their established scenarios require another program under the trusted account to substitute files in AKM’s private working paths. I assign no additional A/B deduction within the stated boundary.

I inspected the test-removal records and surviving behavioral coverage. I established no removal-specific deduction. AB3-01 predates those removals.

**4. Findings outside my areas, with no points**

| Area | Severity / kind | Finding | Evidence | Closing change |
|---|---|---|---|---|
| F | Low / missing | Existing **FV-23 remains**. History exposes times and changed-field names, but no historical value or version restoration. Recovery requires KeePassXC or a backup. | Inspection, `crates/akm-core/src/vault.rs:709` | Provide an explicit, journaled historical-value or restoration workflow and verify it after reopening. |

**5. What I did not cover**

I did not launch desktop windows, access settings or the clipboard, exercise OS-session events, install anything, use the network, or run full builds or suites. Interchange and native-window results remain project-reported.

Focused verification comprised release-CLI journeys, four small core-probe invocations, and one existing mapping-interruption regression. The core probe linked the supplied existing debug library. I did not terminate a process during recovery or inject power loss. These checks do not establish native dialog behavior or cryptographic assurance.

**6(a). Would I keep my own revocable keys here?**

**Yes, with named precautions, for limited CLI use based on A/B alone:** low-privilege keys, a local vault, an independent encrypted backup, a tested KeePassXC recovery route and an interactive password prompt. Daily desktop use still needs the owner-present proofs.

The first failure I expect is opening History to recover an earlier key value and finding only times and field names.

**6(b). Required workflows**

“Complete” below describes available implementation within disclosed limits. It does not establish native acceptance.

| Required workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Complete for ordinary paths.** Ran create/add/list/get/edit; remaining management is code-inspected or project-reported. | **Complete in code.** Inspected native command/session paths; unlock/list proof is project-reported. |
| Preserve metadata | **Complete for sampled paths.** Ran metadata editing and readback; broader interchange is project-reported. | **Complete by inspection.** Changed-field updates preserve untouched stored fields. Real editing unrun. |
| Rotation and recoverable history | **Partly complete.** Read code; rotation proof is project-reported. Historical-value recovery remains external. | **Partly complete** for the same reason. Read implementation; real-window proof unrun. |
| Copy with privacy exclusions and cleanup | **Missing**, intentionally desktop-only; command map inspected. | **Complete in code.** Inspected session integration; real clipboard proof remains unrun. |
| Reviewed scoped `.env` import | **Missing**; command map inspected. | **Complete in code.** Read native approval and commit paths; real confirmation unrun. |
| Explicitly scoped export and injection | **Complete for sampled/reported paths.** Ran file export; injection proof is project-reported. | **Partly complete.** Export and terminal implementations exist; AB3-01 remains. Save proof is project-reported; real terminal approval unrun. |
| Encrypted backups and useful private journal | **Partly complete.** Ran damage, refusal and recovery probes; inspected backups. Recovery has no CLI command. | **Complete within disclosed limits by inspection.** Shared recovery was probed; Activity proof is project-reported. |
| Downloaded sync folders: conflict detection and stale-write refusal | **Complete within local-lock limits.** Ran stale-write refusal and reload. | **Complete by shared-core and caller inspection.** No live desktop sync-conflict proof ran here. |

**7. Do the points agree with 6(a)?**

Only partly. They support cautious CLI use and credit candid positioning and coherent storage/session boundaries. They overstate readiness if interpreted as complete recovery within AKM or proof that consequential Windows integrations work.

Evidence and the handoff receipt (`%TEMP%/akm-review3-ab-811b13b82c1745138ebe414438a5f036/handoff.json`) remain in that named probe folder. No owned process remains. Shared-observer registration was blocked by its in-project-root requirement and this brief’s TEMP-only writes.

Automatic approval review rejected deletion of `probe.exe` and `probe.pdb` with “blocked by policy.” Both remain, totaling **35.85 MiB**. Nothing was deleted; **0 bytes freed**.

## Reviewer 2: security (C and D), first reviewer

Reviewed `dff3d5c3ec8da22b93e80ec375562a0467c55936`. Product source matches `4436dee`; all three supplied executable hashes match. The checkout remains unchanged.

**Area C: 14 / 15**

| Points | Severity | Kind | Finding | Evidence | What wins the points back |
|---:|---|---|---|---|---|
| 0.5 | Low | limit | Purge leaves usable secrets in encrypted backups under the same password. Backup creation remains at `crates/akm-core/src/vault.rs:1112`. | **Reproduced.** After live purge, restoring the recycled entry in `.bak` recovered the rotated fake value. | A deliberate managed-backup retirement or re-key workflow, tested across failures, with honest limits for external copies. |
| 0.5 | Low | limit | Journal verification cannot establish completeness. Its reader accepts a valid remaining chain at EOF in `crates/akm-core/src/audit.rs:589`. | **Reproduced.** Removing the last complete record still produced a consistent verification result. | Independently protected evidence that detects tail removal and replacement. Another same-account writable file would not suffice. |

Deductions of the second full review:

| Previous deduction | Assessment | Evidence |
|---|---|---|
| FR-06, retained backup secrets, 0.5 | **Does not hold as a fix; disclosed limit.** | The purged fake value remained recoverable. |
| FR-07, journal completeness, 0.5 | **Does not hold as a fix; disclosed limit.** | Complete-record tail removal remained undetected. |

I judge D2A-01 and D2A-02 as disclosed limits, not repairs. Recovery still replaces and cleans up by pathname. Within the stated trusted-account boundary, an actor able to substitute these private files already has authority to delete them. I make no additional security deduction for that capability.

Private ACL probes passed for the vault, convenience backup, dated backup and CLI export. Desktop filesystem probes preserved a newly arrived export destination and identified a moved owned remnant without deleting its replacement sentinel.

**Area D: 13 / 15**

| Points | Severity | Kind | Finding | Evidence | What wins the points back |
|---:|---|---|---|---|---|
| 1.0 | Medium | design | A compromised page can collect five single-key values per rolling minute and renew the session through `activity`. The allowance remains at `desktop/src-tauri/src/session.rs:752`. | **Reproduced with production Session/core code and a simulated clock.** Ten distinct fake values were disclosed across two windows. Activity renewal kept the session available through simulated second 1500. I found no page-compromise entry point. | Independent native authority that arbitrary page requests cannot renew, tested across successive disclosure windows. |
| 0.5 | Low | limit | CLI injection gives `std::process::Command` owned environment copies that AKM cannot zeroize. See `crates/akm-cli/src/child.rs:55`. | **Inspection.** The parser and document repairs do not remove this disclosed copy. | A launch path that avoids these copies and demonstrates cleanup, including failure paths. Retain the separate OS, database and webview memory limits. |
| 0.5 | Low | limit | `AKM_PASSWORD` remains in the supplying shell’s environment, available to its other children. Reading it at `crates/akm-cli/src/main.rs:1182` cannot clear that parent exposure. | **Inspection; selected-child scrubbing reproduced.** | Supported automation authentication that does not retain the password in the parent environment. The interactive prompt already avoids this exposure. |

Deductions of the second full review:

| Previous deduction | Assessment | Evidence |
|---|---|---|
| FW-01, clipboard work delays native invalidation, 1.5 | **Holds within checked scope.** | With timer cleanup stalled, production `Delivery::message` delivered lock, suspend and sign-out signals before release. Obsolete reveals failed, and a replacement session survived the older cleanup. The existing stalled-copy regression also passed. |
| FV-09, gradual page disclosure, 1.0 | **Does not hold as a fix; disclosed design limit.** | The ten-key, two-window probe confirmed the renewable allowance. |
| FV-10 / FW-09, uncleared copies, 0.5 | **Partly holds.** | Five new parser cases and a 71-entry Unicode/escaped export freed no uncleared marker buffers. Both targeted allocator regressions passed. The `Command` copy remains. |
| FR-09, parent password environment, 0.5 | **Does not hold as a fix; disclosed limit.** | The selected child received no `AKM_*` variables; parent exposure remains. |

The FW-09 and D2A-03 repairs hold for the checked paths. These results support the narrowed cleanup promise, not clean process memory.

**4. Findings outside C and D, without points**

- **H, existing proof gap:** real clipboard behavior, native import and terminal confirmations, Windows lock/suspend delivery and a real idle wait remain unrun. Fake dispatch establishes source behavior, not Windows delivery.
- **B/F, existing completion gaps:** the CLI has no journal recovery, import or copy command. AKM history does not retrieve or restore historical values internally. No new outside-area defect was established.

**5. What I did not cover**

I ran 40 isolated release-CLI invocations, three targeted existing tests, and small production-module probes. These covered disclosure refusal, scoping, child scrubbing, export ownership, backup recovery, journal damage, locking interleavings and measured buffer cleanup.

I did not start the desktop app, any window, or an installer; access settings or the real clipboard; change the Windows session; use the network; start workflows; or run full builds or suites. I did not audit cryptographic dependency internals, inspect memory dumps, exercise real cloud synchronization, or independently repeat interchange proofs. Supplied full-suite and real-window results remain project-reported.

Native probes used unchanged production modules and the existing core library, with a fake clipboard and a path-display shim. They did not exercise Tauri transport.

I inspected relevant test-removal records and surviving behavioral coverage. I established no separate security deduction caused by those removals; this was not an exhaustive mutation audit.

**6(a). Would I keep my own revocable API keys here today?**

**Yes, with named precautions, through the CLI:** a strong unique master password, private NTFS storage, prompt/stdin input, explicit scopes, trusted child programs and an independent encrypted backup. I would revoke keys at their providers before treating purge as retirement. I would defer desktop disclosure and clipboard-dependent use until the owner-present proofs pass.

The first security failure I expect a desktop user to meet is a copied value surviving busy-clipboard cleanup. This is a prediction from inspected failure handling, not a real-clipboard observation.

**6(b). Required workflows**

“Complete” below describes implemented functionality within its documented limits. It does not mean complete desktop acceptance.

| Required workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage | **Complete for sampled paths.** Ran creation, add, list/get, editing, removal and purge; read remaining selectors. | **Partly complete in proof.** Read native management code; unlock/list proof is project-reported. |
| Preserve metadata | **Complete for sampled paths.** Ran metadata editing/readback; broader interchange is project-reported. | **Partly complete in proof.** Read preservation boundaries and fix records; real editing journey unrun. |
| Correct rotation and recoverable history | **Partly complete.** Ran rotation/history and backup recovery. Historical-value recovery requires another reader or backup. | **Partly complete.** Read history representation; the same internal recovery limitation remains. |
| Privacy-aware copy and cleanup | **Missing intentionally.** Command inventory inspected. | **Partly complete in proof.** Inspected publication/ownership code and ran fake-backend interleavings; real clipboard unrun. |
| Reviewed `.env` import | **Missing intentionally.** Command inventory inspected. | **Partly complete in proof.** Read parsing, native confirmation and post-approval checks; real confirmation unrun. |
| Explicitly scoped export and injection | **Complete for sampled paths.** Ran scoped export, no-overwrite, reserved-name refusal and windowless child injection with scrubbing and exit propagation. | **Partly complete in proof.** Filesystem probes passed; Save `.env` proof is project-reported; native terminal confirmation unrun. |
| Encrypted backups and useful private journal | **Partly complete.** Ran permissions, recovery, verification and damage refusal; CLI journal recovery is absent. | **Partly complete in proof.** Shared storage inspected/probed; Activity proof is project-reported; recovery window unrun. |
| Defensive downloaded-sync-folder use | **Complete at the documented local-refusal level, by inspection and reported tests.** No live sync-client test. | **Partly complete in proof.** Shared fingerprint/refusal implementation exists; real desktop conflict journey unrun. |

Desktop idle/session/suspend protection is **partly complete in proof**. The repaired dispatch path passed simulated interleavings; actual Windows delivery remains unproved.

**7. Do the points agree with 6(a)?**

They support cautious CLI use. They overstate readiness for daily desktop use because source repairs and bounded deductions cannot establish the Windows workflows that nobody ran.

Evidence remains in the probe folder (`%TEMP%/akm-security3-12b29dae69ae489fb26da836d1452b44`), approximately **39.6 MiB**, with a handoff receipt (`%TEMP%/akm-security3-12b29dae69ae489fb26da836d1452b44/handoff-receipt.json`). No probe process remains. Shared-observer registration was incompatible with the TEMP-only brief; the local receipt records that gap.

Automatic approval review rejected deletion of the task-owned probe executable and symbols with **“blocked by policy.”** They remain. Nothing was deleted; **0 bytes freed**.

## Reviewer 3: security (C and D), second reviewer

Reviewed `dff3d5c3ec8da22b93e80ec375562a0467c55936`. Its product source matches `4436dee`; all three supplied executable hashes match the brief. The checkout and `<build>` remain unchanged.

**Area D: 13 / 15**

| Points deducted | Severity | Kind | Finding | Evidence | What wins the points back |
|---:|---|---|---|---|---|
| 1.0 | Medium | design | A compromised page can obtain five single-key disclosures per rolling minute and renew the session through `vault_activity`. The allowance remains renewable at `desktop/src-tauri/src/session.rs:752`. | **Inspection.** A source probe confirmed 15 free admissions across three windows. I found no page-compromise entry point. | Require native authority that arbitrary page calls cannot renew. Test paced requests for distinct keys across several windows. |
| 0.5 | Low | limit | CLI injection copies values into `std::process::Command`, whose environment storage is not zeroized. The narrower cleanup promise accurately discloses this at `crates/akm-cli/src/child.rs:55`. | **Inspection.** | Use a controlled, zeroized environment block and verify spawn-success and failure cleanup. Keep the separate runtime and OS limitations explicit. |
| 0.5 | Low | limit | `AKM_PASSWORD` remains in the supplying shell and can reach its other children. Reading it at `crates/akm-cli/src/main.rs:1181` cannot clear the parent. | **Inspection.** Selected-child scrubbing passed independently. | Provide automation authentication that avoids retaining the master password in the parent environment. The interactive prompt already avoids this exposure. |

The last review’s deductions:

| Previous deduction | Assessment | Evidence |
|---|---|---|
| FW-01, delayed lock delivery, 1.5 points | **Holds.** | The selected regression passed. New production-module probes delivered disconnect, suspend and resume while expiry cleanup was blocked; delivery returned promptly and stale disclosure was refused before cleanup was released. |
| FV-09, gradual page disclosure, 1.0 point | **Does not hold as a fix; limit remains.** | The renewable admission budget and page-callable activity renewal remain. |
| FV-10, uncleared copies, including FW-09, 0.5 points | **Partly holds.** | Alternate parser-error cases and a 57-value escaped document freed no uncleared marker buffers. The parser regression passed. The disclosed `Command` copy remains. |
| FR-09, parent password environment, 0.5 points | **Does not hold as a fix; limit remains.** | The parent exposure remains; the selected child received no `AKM_*` variables. |

The sampled versioned Lua and other interpreter controls were refused without echoing values or changing vault bytes. Both removed `--secret` argument forms were also refused without echoing their supplied values.

The owner-present security proof gap remains assigned to H. I make no duplicate deduction here.

**Area C: 14 / 15**

| Points deducted | Severity | Kind | Finding | Evidence | What wins the points back |
|---:|---|---|---|---|---|
| 0.5 | Low | limit | Purge leaves usable values in encrypted backups under the same password. Backup refresh remains at `crates/akm-core/src/vault.rs:1237`. | **Reproduced.** After rotation and live purge, `.bak` revealed the purged key’s latest value. | A deliberate managed-backup retirement or re-key workflow, with failure recovery and honest limits for external copies. |
| 0.5 | Low | limit | The journal cannot establish completeness. Its reader accepts a valid remaining chain at EOF at `crates/akm-core/src/audit.rs:589`. | **Reproduced.** Removing the final complete record still made `audit --verify` succeed. | An independently protected checkpoint, tested against tail removal and whole-chain replacement. Another same-account writable file is insufficient. |

The last review’s deductions:

| Previous deduction | Assessment | Evidence |
|---|---|---|
| FR-06, backups retain purged keys, 0.5 points | **Does not hold as a fix; no fix claimed.** | The rotated, purged fake value remained readable from the convenience backup. |
| FR-07, journal lacks an integrity anchor, 0.5 points | **Does not hold as a fix; no fix claimed.** | Complete-record tail removal remained undetected. The disclosure of this limitation is accurate. |

Additional checks supported the existing protections:

- The release-created header contained KDBX 4.1, AES-256 and Argon2id with 64 MiB, three iterations and parallelism four. Wrong-password opening and short-password creation were refused.
- Under an Everyone-readable fake parent, the vault and backups retained protected current-user-only permissions, including dated-backup paths beyond 500 characters. An Everyone-readable journal root refused disclosure without writing.
- A late desktop export destination survived publication failure, and the owned plaintext temporary file was removed.
- With deletion deliberately refused, a renamed desktop remnant beyond 512 path units was correctly named. The replacement sentinel survived.

D2A-01 and D2A-02 remain disclosed pathname-substitution risks, not repaired behavior. Under the trusted-account boundary, I found no additional C/D confidentiality consequence beyond the existing journal limitation. Their preservation consequences belong to the other reviewers.

**4. Findings outside my areas, without points**

| Area | Finding | Evidence and closing change |
|---|---|---|
| F | Known FV-23 remains. History describes versions but neither retrieves an old value nor restores a version within AKM. | **Inspection; CLI history ran.** Add an explicit, journaled historical-value or restoration workflow and verify it after reopening. |
| F | The CLI still has no reviewed import or clipboard-copy command. | **Inspection of the command inventory.** Implement and verify those workflows if CLI parity is required. |

I established no new scored C/D defect or unsupported C/D claim. I inspected the security-related test-removal records and surviving behavioral coverage; I established no separate removal-caused defect.

**5. What I did not cover**

I ran 35 release-CLI invocations, two selected regression tests and small source-based probes. Module probes used fake clipboard behavior and supplied dependency libraries. The filesystem probe used a path-display shim and did not exercise Tauri transport.

I did not start the desktop app or any window, touch clipboard/settings/real vaults, change the Windows session, install anything, use the network, start workflows, or run full builds or suites. Actual Windows notification delivery, clipboard behavior, native confirmations and idle locking remain unproved here. Interchange and supplied real-window results remain project-reported.

The additional CLI junction-substitution probe was inconclusive because its fixture could not create the junction. FV-06 was therefore checked by inspection only. I did not audit cryptographic dependency internals, inspect memory dumps or exercise live cloud synchronization.

**6(a). Would I keep my own revocable API keys in this build today?**

**Yes, with named precautions, for limited CLI use.** I would use private NTFS storage, a strong unique password entered through the prompt, stdin for new secrets, explicit scopes, trusted child programs and a separate encrypted backup. I would revoke keys at their providers before treating purge as retirement. I would defer daily desktop disclosure and clipboard use until the owner-present proofs pass.

The first security failure I expect is a copied value remaining after cleanup encounters a busy clipboard. That is a prediction from the failure handling, not a real-clipboard observation.

**6(b). Required workflows**

“Complete in code” describes available implementation within its disclosed limits. It does not mean that the native acceptance journey passed.

| Required workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Complete in code.** Ran create, add, list, inspect, edit and purge; read remaining paths. | **Complete in code by inspection.** Unlock/list proof is project-reported; broader management remains unrun. |
| Preserve metadata | **Complete for sampled paths.** Ran metadata editing; inspected shared field handling. Broader interchange is project-reported. | **Complete in code by inspection.** Read changed-field handling and exact scope propagation; native preservation journey unrun. |
| Rotation tracking and recoverable history | **Partly complete.** Ran rotation, history and backup recovery. Historical-value recovery inside AKM is absent. | **Partly complete** for the same missing recovery action. Read code; real-window journey unrun. |
| Privacy-aware copy and cleanup | **Missing.** No copy command. | **Complete in code; real proof pending.** Read publication/ownership code and ran fake-backend lock probes. |
| Reviewed import; scoped export and injection | **Partly complete.** Export and injection passed; import is absent. | **Complete in code by inspection.** Save proof is project-reported; actual import and terminal confirmations remain unrun. |
| Encrypted backups and useful private journal | **Partly complete.** Private files, disclosure refusal and backup recovery passed; CLI journal recovery is absent. | **Complete in code with disclosed limits.** Read recovery/callers; Activity proof is project-reported. |
| Defensive downloaded-sync-folder use and stale-write refusal | **Complete at the disclosed local-refusal level by inspection.** Automated results are project-reported. | **Complete at the same level by shared-core/caller inspection.** No live sync-client proof. |

**7. Do the points say the same thing as 6(a)?**

Only conditionally. They support cautious CLI use and credit the repaired source behavior. They overstate daily desktop readiness if read as proof that Windows integrations work. They also underrepresent the practical importance of missing recovery workflows.

Deleted 40 task-owned files, including compiled probes and disposable fixtures, freeing **56,103,410 bytes, about 53.5 MiB**. No owned process remains. Sources, results and the handoff receipt (`%TEMP%/akm-security-r3-da25505209334f25b0c307d1d96c4a7c/handoff.json`) remain in the named probe folder (`%TEMP%/akm-security-r3-da25505209334f25b0c307d1d96c4a7c`). Shared-observer registration was not performed because its required in-project root conflicts with this brief’s TEMP-only writes; the local receipt records that integration gap.

## Reviewer 4: data integrity and correctness (E and F)

Reviewed `dff3d5c`, whose product code matches `4436dee`. All three build hashes match the brief. I ran 24 isolated release-CLI calls, production-source probes and four individual native export tests. The checkout remains unchanged.

**Area E: 14.75 / 15**

| Deduction | Severity | Kind | Finding and evidence | What wins the points back |
|---:|---|---|---|---|
| 0.25 | Low | defect | A notes-only edit invents a creation time when a foreign entry has no `CreationTime`. The update calls a stamper that always fills it: `crates/akm-core/src/vault.rs:1754`. **Reproduced** through native draft conversion, save/reopen and the release CLI. This violates contract 13’s preservation rule, although the secret remains intact. | Preserve absent creation times on updates. Add a regression that edits notes, reopens the vault and verifies that creation remains absent. |

| Last review’s deduction | Fix assessment | Evidence |
|---|---|---|
| FW-03, untouched metadata changes, 1.5 points | **Partly holds** | Padded titles, usernames, exact scopes, protection flags, a legacy NUL secret and an untouched 50,000-day interval survived. Foreign no-op updates preserved the complete file bytes. The missing-creation-time case above remains. |

Additional checks upheld exact scoped import/export, unchanged-import no-ops, stale-write refusal, rollback after blocked replacement, hard-link refusal and journal recovery. All four FW-10 export regressions passed.

D2A-01 and D2A-02 remain disclosed limits, rather than fixes. I add no E/F deduction for a same-account program actively substituting those filenames: that actor can already replace or remove the files within the stated trust boundary.

**Area F: 9.0 / 10**

| Deduction | Severity | Kind | Finding and evidence | What wins the points back |
|---:|---|---|---|---|
| 0.50 | Low | missing | **FV-23 remains.** History returns times and field names, with no historical-value retrieval or version restoration: `crates/akm-core/src/vault.rs:709`. **Inspection**, supported by release-CLI history execution. Recovery still requires KeePassXC or a backup. | Provide deliberate historical-value recovery or restoration, verified after rotation and reopening. |
| 0.25 | Low | defect | A legitimate username equal to `api-key` becomes `None` in shared metadata: `crates/akm-core/src/vault.rs:1840`. **Reproduced:** `add --username api-key` stored that exact value, but `get --field username` printed an empty line. The desktop receives the same missing value. | Distinguish an unset username from this literal value. Test exact CLI and desktop metadata readback after reopening. |
| 0.25 | Low | limit | Desktop import cannot target an existing project/environment with outer whitespace. Native code trims both before preview and writing: `desktop/src-tauri/src/commands.rs:1198`. **Inspection.** D2B-05 discloses this limitation; the confirmation shows the trimmed scope. | Support exact existing scope names through import. Test padded and plain scopes containing the same variable through native approval and readback. |

| Last review’s deduction | Fix assessment | Evidence |
|---|---|---|
| FV-23, historical-value recovery, 0.50 | **Does not hold; no fix claimed** | Both surfaces still expose history descriptions only. |
| FW-04, recycled UUID selection, 0.25 | **Holds** | Release CLI accepted uppercase URN, braced and compact UUID spellings across recycled history, restore and purge. |
| FW-05, username search, 0.25 | **Holds** | Production search-helper probes matched partial and differently cased usernames, including a Kelvin-sign case pair. |
| FW-06, caseless search, 0.25 | **Holds** | New production-helper cases passed for final sigma, Deseret and sharp S. Composed and decomposed accents remained distinct as documented. |

**4. Findings outside my areas**

None newly established. I have not charged desktop proof gaps again in E or F.

**5. What I did not cover**

No desktop window, clipboard, saved settings, native confirmation, terminal window, OS-session action, installer, network, full build or full suite ran. Interchange and the focused real-window results remain project-reported.

Native draft probes compiled the production adapter against an existing core library; they did not execute Tauri handlers. Search probes executed production helpers without a browser. Backup retention was inspected rather than independently exercised exhaustively.

I inspected the test-removal records and relevant surviving coverage. I established no removal-specific deduction. The surviving preservation regression does not cover absent creation times.

**6(a). Would I keep my own revocable API keys here?**

**Yes, with named precautions, considering E/F alone:** use the CLI, keep an independent encrypted backup, test KeePassXC history recovery, and use unpadded scope names for desktop imports.

The first limitation I expect to meet is needing an earlier secret after rotation and finding only history descriptions in AKM.

**6(b). Required workflows touching E/F**

“Complete” describes available functionality within its documented limits; the evidence column identifies what actually ran.

| Workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Complete for ordinary management.** Ran creation, inspection, edits and lifecycle operations. Username exception above. | **Complete in implementation.** Read code and ran search helpers; real unlock/list evidence is project-reported. |
| Preserve metadata | **Partly complete.** Ran preservation/readback probes; creation-time and username defects remain. | **Partly complete.** Ran production draft conversion and save/reopen; same shared-core defects apply. |
| Rotation and recoverable history | **Partly complete.** Ran rotation/history; old-value recovery requires another application. | **Partly complete.** Read code; the same recovery gap remains, and real-window rotation is unrun. |
| Reviewed scoped import | **Missing**, intentionally desktop-only; read command map. | **Partly complete.** Read approval/write code and ran core isolation probes. Padded scopes are unsupported; real confirmation is unrun. |
| Scoped export and injection | **Complete for sampled paths.** Ran export and a child receiving the selected key, no `AKM_*` variables, and exit-code propagation. | **Complete in implementation.** Read code and ran focused export tests. Save-window proof is project-reported; real terminal launch remains unrun. |
| Encrypted backups and useful private journal | **Complete within disclosed limits.** Inspected backup handling; ran CLI audit and core damage/recovery probes. CLI recovery itself remains absent. | **Complete in implementation, partly proved.** Activity evidence is project-reported; real recovery confirmation remains unrun. |
| Conflict detection and stale-write refusal | **Complete within local-lock limits.** Ran stale-write refusal, rollback, reload and hard-link refusal. | **Complete in shared-core implementation.** Native desktop conflict handling was inspected, not driven live. |

Clipboard behavior was outside this assessment.

**7. Do the points agree with 6(a)?**

Mostly for cautious CLI use. They overstate readiness if read as proof of daily desktop operation or recovery entirely within AKM. These rare metadata defects also matter less to fresh AKM-created entries than to imported KeePass data.

Evidence and the local handoff receipt (`%TEMP%/akm-data-review-20261114-64df8587/handoff.json`) remain in the named temporary folder. Task-owned build, fake-vault, journal and helper-state directories were deleted, removing **5,063,901 bytes, about 4.83 MiB**. The unexpected opencode directory (`%TEMP%/akm-data-review-20261114-64df8587/opencode`) remains because its ownership was not established.

Shared-observer registration and handoff were incompatible with the brief’s TEMP-only writes: the observer requires an in-project root and external registry writes. The receipt records this integration gap.

## Reviewer 5: desktop quality (G)

Area G: **9.25 / 10**.

Reviewed `dff3d5c`, whose product code matches `4436dee`, and the supplied built page. All three executable hashes match the brief. Reproductions used headless Edge on loopback, fake data and my own native mock.

| Points | Severity | Kind | Finding and evidence | What wins the points back |
|---:|---|---|---|---|
| −0.50 | Low | defect | **Enlarged text makes navigation controls overlap.** At the supported 900 × 620 size, a 24 px root font puts Settings beneath Reload vault. At 32 px, Recycle Bin is covered too; clicking its centre dispatched `vault_reload`. The rail has no scrollable overflow to uncover these controls. `desktop/src/styles.css:315`. **Reproduced in both themes with synthetic text sizing.** | Prevent navigation from shrinking beneath its contents, or provide effective scrolling. Hit-test every rail control after scrolling at 900 × 620 with 150% and 200% text, in both themes. |
| −0.25 | Low | defect | **Enlarged status text is clipped.** At 900 × 800 with a 32 px root font, “Rotation unknown” extends to x=934 while its panel ends at x=874. The fixed status column and `white-space: nowrap` leave the ending hidden. `desktop/src/styles.css:499`, column at line 636. **Reproduced in both themes with synthetic text sizing.** | Wrap or rearrange status text as it grows. Test that every complete status label remains inside its row at 150% and 200% text, without overlapping neighbouring content. |

The deduction of the second full review:

| Previous deduction | Result | Evidence |
|---|---|---|
| FW-07, drawer actions offscreen, 0.50 | **Holds. The points come back.** | With expanded history, drawer actions remained reachable at 900 and 1180 px with 24 and 32 px text in both themes. Additional drawer, add, edit and import checks passed at 900 × 620. |

Further probes supported the page fixes for untouched invalid metadata, lost-reply retries, exact selected vault paths, supplementary-character filtering and accessible field hints. Comma-bearing scopes survived appending another scope. A delayed import refreshed an already-mounted Keys view. Sampled forced-colour separators and restored history styling held.

4. Findings outside G, with no points.

No new finding outside G. Existing **FV-23** remains a missing part of recoverable history: both surfaces show version times and changed fields, but retrieving or restoring an old value requires another tool or backup. **Inspection.**

5. What I did not cover.

I did not execute the CLI, native desktop app, installer, full builds or full suites. I did not access saved settings, real vaults, the clipboard or actual OS-session events. Native confirmations, terminal delivery, locking, suspend, real Windows text scaling, contrast themes and screen readers remain unproved here. Build-page attribution and reported native proofs were taken from the brief.

I inspected the frontend test-removal record and relevant surviving coverage. I established no additional deduction caused by those removals.

6(a). Would I keep my own revocable API keys here today?

**No for daily desktop use yet.** I would first complete the owner-present disclosure and locking proofs. The first failure I expect for someone using enlarged text at the minimum window size is selecting Settings and triggering Reload vault.

6(b). Required workflows.

“Complete” below means implemented, within disclosed limits. It does not mean native desktop acceptance.

| Required workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage | **Complete.** Read command coverage; took the project’s reported execution results. | **Partly complete.** Mocked ordinary journeys worked; navigation fails under the enlarged-text conditions above. |
| Preserve metadata | **Complete.** Inspected shared update logic; execution and interchange are project-reported. | **Complete in inspected implementation.** Ran notes-only outgoing-draft probes; native persistence was not independently run. |
| Rotation tracking and recoverable history | **Partly complete.** Read code and project results; FV-23 remains. | **Partly complete.** Ran mocked rotation and history display; FV-23 remains. |
| Copy with privacy exclusions and ownership-aware cleanup | **Missing.** No copy command, by inspection. | **Complete in code.** Read the path and reported evidence; real clipboard operation remains unrun. |
| Reviewed scoped import | **Missing.** No import command, by inspection. | **Partly complete.** Mocked dispatch and delayed completion worked. The disclosed normalization limit excludes import targets with outer spaces; native confirmation remains unrun. |
| Explicitly scoped export and injection | **Complete.** Read commands; took reported export/run proofs. | **Complete in implementation.** Ran mocked preview and scoped terminal cancellation/success; native delivery remains unrun. |
| Encrypted backups and useful private journal | **Complete within disclosed limits.** Inspected code and took reported proofs. | **Complete in implementation.** Read views and callers; focused real-window proofs are project-reported. |
| Downloaded sync folders: conflicts and stale-write refusal | **Complete within local-lock limits.** Inspected shared implementation and reported evidence. | **Complete in implementation.** Read refusal/reload handling; no live sync race was run. |

7. Do the points agree with 6(a)?

Only partly. The 9.25 credits implemented desktop workflows and presentation. It cannot establish the consequential Windows behaviour that remains unrun, or supply historical-value recovery assigned to F. Read as permission for daily use, it overstates readiness.

The checkout is unchanged. Browsers and loopback servers closed; no matching probe writer remained. Evidence and the local handoff receipt (`%TEMP%/akm-fullreview3-g-d73256fd44374666a812a098a193ecd9/handoff-receipt.json`) remain in that probe folder. Shared registration rejected the TEMP location; handoff then failed because no registry record existed.

Automatic approval review rejected deletion of the task-owned browser temporary folder with “blocked by policy.” It remains retained. Nothing was deleted; **0 bytes freed**.

## Reviewer 6: verification and shareability (H and I)

Reviewed `dff3d5c3ec8da22b93e80ec375562a0467c55936`. Product code matches `4436dee`, and all three build hashes match the brief. The checkout remained unchanged.

Area H: 3.50 / 5

| Points | Severity | Kind | Finding and location | Evidence | What wins the points back |
|---:|---|---|---|---|---|
| 1.00 | Medium | proof | Consequential Windows paths remain unrun, including the real clipboard, native approvals, hide, lock/suspend and idle behavior. The handler harness uses a mocked runtime. `docs/STATE.md:98`. | inspection | Complete the required Windows proofs on identified builds, including an accepted observation method for R4-04. |
| 0.25 | Low | release | No CI run validates this revision or the changed workflows. Local success does not establish runner behavior. `docs/STATE.md:97`. | inspection | A successful complete CI run on the reviewed revision. |
| 0.25 | Low | limit | Published binaries cannot currently be verified by an independent matching rebuild. Reproducibility is explicitly unpromised, and the comparison remains reporting-only and unrun. `docs/TESTING.md:66`. | inspection | Demonstrate independent matching rebuilds and enforce the comparison. |

The last review’s deductions:

| Previous deduction | Fix verdict | Evidence |
|---|---|---|
| FV-14, Windows proof gaps, 1.00 | **Does not hold; no fix claimed.** | The brief confirms these paths remain unrun. Focused journal/export proofs do not cover them. |
| FW-08, corrupted helper arguments, 0.25 | **Holds.** | Six notes round trips through the helper and release CLI preserved trailing backslashes, embedded quotes, tabs, multiline text and Unicode exactly under Windows PowerShell 5.1. The permanent argument regression covers the corrected encoding at `.agents/skills/verify-akm/scripts/akm-verify.ps1:82`. |
| FV-17, missing CI execution, 0.25 | **Does not hold; no fix claimed.** | CI has not run since 8 November, as the brief permits me to count. |
| FV-17, reproducibility, 0.25 | **Does not hold; disclosed limit remains.** | The documentation explains the limitation more precisely, but matching rebuild evidence still does not exist. |

I read the test-removal records and sampled surviving secret comparisons for cross-scope import, rollback after a failed save, and complete child-environment comparisons. I established no additional removal-specific deduction. The removed structural guards leave rules assigned to review; I did not perform an exhaustive mutation audit.

Area I: 3.50 / 5

| Points | Severity | Kind | Finding and location | Evidence | What wins the points back |
|---:|---|---|---|---|---|
| 1.00 | Medium | release | The builds remain unsigned, and the manual release channel has no release key or signed release. A recipient cannot authenticate the installer or a subsequent security fix. `docs/RELEASING.md:52`. | inspection | An authenticated release, independently obtainable verification material, and a tested verification procedure. |
| 0.50 | Low | release | Clean-computer install, reinstall and uninstall remain unrun. The prepared job tests same-version reinstall; no earlier release exists for migration testing. `docs/TESTING.md:160`. | inspection | Run the clean-machine preservation proof. Add migration coverage when an earlier release exists. |

The last review’s deductions:

| Previous deduction | Fix verdict | Evidence |
|---|---|---|
| FV-18, release authentication, 1.00 | **Does not hold; no fix claimed.** | Builds remain unsigned and the release-key section remains empty. |
| FV-19, installation proof, 0.50 | **Does not hold; no fix claimed.** | The scripts exist, but no clean-machine execution exists. |
| FR-26, publication privacy, 0.50 | **Holds for the established current-tree defect.** | The actual scan returned **CLEAN, 405 files**. Inspection supports exporting only the selected tree into a parentless repository. Private history remains unsuitable for publication; the required snapshot route and owner inspection remain necessary. |

The earlier licensing-package fix remains supported. Bundle configuration and the generated NSIS script include `LICENSE` and `THIRD_PARTY_NOTICES.md`; their referenced source bytes match this checkout. A fresh nested-notice fixture retained its licence text, SPDX copyright and manifest author. Installer execution remains covered by the installation deduction.

4. Findings outside my areas, with no points.

Known FV-23 remains. History exposes times and changed fields, but provides no historical values or version restoration. Recovery requires KeePassXC or a backup. I ran history and inspected the command interface at `crates/akm-cli/src/main.rs:310`. I established no new outside-area finding.

5. What I did not cover.

I ran 30 release-CLI commands, including six exact argument round trips, scoped stdout/file export, scoped child injection, `AKM_*` scrubbing, child exit code 37, rotation, history, Recycle Bin, restore, purge and journal verification. The fast documentation check passed for 40 claims. Sixteen focused checker, Unicode-scan, notices and packaging cases passed.

I did not run full builds or suites, interoperability suites, GUI or installer flows, clipboard or OS-session tests, workflows, network checks, a screen reader, or a legal review. Full-suite, interoperability and focused real-window results remain project-reported. I created no public snapshot. Matching hashes establishes artifact identity, not independently verified build provenance.

6(a). Would I keep my own revocable API keys here?

**No for the intended daily desktop use today, considering these areas.** The disclosure and locking paths still lack real Windows proof.

The first obstacle I expect a new recipient to meet is being unable to authenticate the installer.

6(b). Required workflows.

These judgments include verification completeness.

| Required workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage | **Complete for sampled ordinary management.** Ran creation, listing, inspection, editing and lifecycle operations; selection coverage inspected. | **Partly complete.** Took the focused unlock/list proof from the project; the complete real-window journey remains unrun. |
| Preserve metadata | **Complete for sampled fields.** Ran exact notes readback and inspected preservation regressions. Broader interchange is project-reported. | **Partly complete.** Automated evidence is project-reported; real editing and preservation remain unrun. |
| Correct rotation and recoverable history | **Partly complete.** Ran rotation/history; historical-value recovery remains external, FV-23. | **Partly complete.** The same recovery limitation remains, and real-window rotation is unrun. |
| Privacy-aware copy and cleanup | **Missing, intentionally desktop-only.** Read the command interface. | **Partly complete.** Read proof/test code; the real clipboard proof remains absent. |
| Reviewed `.env` import | **Missing, intentionally desktop-only.** Read the command interface. | **Partly complete.** Scripted confirmation evidence is project-reported; the real native confirmation remains unrun. |
| Explicitly scoped export and injection | **Complete for sampled paths.** Ran two-scope export/injection checks, scrubbing and exit propagation. | **Partly complete.** Save `.env` proof is project-reported; terminal and native approval journeys remain unrun. |
| Encrypted backups and useful private journal | **Complete within inspected/reported limits.** Ran journal verification; backup retention coverage inspected. | **Partly complete.** Focused Activity proof is project-reported; broader recovery journeys remain unrun. |
| Downloaded sync folders: conflict detection and stale-write refusal | **Complete within documented local-lock limits.** Inspected regression coverage; did not independently run concurrent writers. | **Partly complete.** Automated evidence exists; the real desktop conflict journey remains unrun. |

7. Do the points agree with 6(a)?

Only partly. They credit repaired verification tools and publication preparation. They overstate desktop readiness because consequential Windows behaviors remain unknown under one bounded deduction. Release deductions matter more to another person downloading AKM than to an owner using a locally built CLI.

Evidence and the local handoff receipt (`%TEMP%/akm-hi-r3-7aa74eaaa856436bac7fe9591bd3a8b0/handoff-receipt.json`) remain in that TEMP folder, totaling 28,957 bytes. Observer registration rejected its location outside the project; shared handoff then failed because registration had not succeeded. The helper removed its owned fake-vault session and state token; freed space was not measured.

Automatic approval review rejected deletion of the remaining 4,892 bytes of fake probe data as **“blocked by policy,”** with no further reason. Those files remain in the named folder.
