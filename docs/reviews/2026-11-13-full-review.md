# Full review, 13 November 2026

- Model: Codex GPT-6.1 Sol, effort max ([REVIEWING.md](../REVIEWING.md)).
- Reviewed commit: `1d5cc820098678172adbd8da4365f66c2704ceb7` on branch `gate/integration`. Its product code equals `d3b6fb1`; the later commits change documents only.
- Builds of `d3b6fb1` (unsigned, SHA-256): `akm.exe` `094b76fbdfc8129aa54e5d45e49234a8b111056254de2d9036b666c53c9c71f5`; `akm-desktop.exe` `926ac145c18b001a704f635cfc13f3fac23d436a6339b627337cd260dce614b9`; setup program `4a276c32962d097b8b956b318fe0e9f4138619c4c3c341fee3d5a75b0f4818ac`.
- Date: 2026-11-13.
- Rubric: the nine areas and 100 points of the [baseline review](2026-10-30-baseline.md), with the scale of the [9 Nov rescore](2026-11-09-full-rescore.md).
- Brief: [2026-11-13-full-review-brief.md](2026-11-13-full-review-brief.md).
- Reviewers: six independent reviewers, each with its own checkout and no view of the others' work. Two of them reviewed the security areas (C and D), so that the spread between two reviewers is measured once.

## Result

**83.5 / 100. Not ready for real secrets.** The bar is above 90/100, followed by the owner's testing.

| Area | Points | 9 Nov | Now | Reviewer | Main deductions |
| --- | ---: | ---: | ---: | --- | --- |
| A. Premise and positioning | 10 | 8.5 | 10 | Architecture and premise | None |
| B. Architecture and design | 15 | 11 | 14 | Architecture and premise | A damaged journal mapping stops disclosure for every vault, and recovery cannot repair it |
| C. Security at rest | 15 | 11 | 11.5 | Security, second reviewer | A CLI export follows a replaced parent folder; a cleanup warning can name the wrong file; two disclosed limits |
| D. Security in use | 15 | 9.5 | 12.5 | Security, second reviewer | A compromised page can read keys slowly; memory copies; `AKM_PASSWORD` in the parent shell; `add --secret` |
| E. Data integrity and durability | 15 | 12 | 12 | Data and correctness | A desktop edit changes untouched metadata; a save through a file symbolic link |
| F. Functional correctness | 10 | 6 | 8.75 | Data and correctness | The CLI cannot edit metadata; the desktop form refuses a valid rotation interval |
| G. Desktop app quality | 10 | 5 | 9 | Desktop quality | A stored username is not shown; a late import leaves the key list stale |
| H. Verification and hygiene | 5 | 1.75 | 3 | Verification and shareability | The owner-present proofs were not run; checker and helper defects; no CI run |
| I. Shareability | 5 | 1.5 | 2.75 | Verification and shareability | Unsigned and no release key; no clean-computer install; private paths in the tree; no licence files in the installer |
| **Total** | **100** | **66.25** | **83.5** | | |

### How the total was counted

- **Two security reviewers.** The rule was written before the review started ([STATE.md](../STATE.md) at `a13e506`): the gate number uses the lower of the two security results, and both are recorded. The first reviewer gave C 14 and D 11 (25 of 30). The second gave C 11.5 and D 11.5 (23 of 30).
- **One root cause counted twice.** The second security reviewer deducted 1.0 in D because the owner-present proofs were not run, and asked that the gap count once if H also reports it. H deducted 1.0 for the same gap. The deduction stays in H, and the second reviewer's D goes from 11.5 to 12.5 (24 of 30). That is still the lower security result. No other root cause appears twice in the counted areas.
- With the first reviewer's security scores the total would be 84.5.
- No points were added for any other reason. A finding that a reviewer reported outside its own areas has no points.

### What the score does not hold

- **The two security reviewers found different defects.** Both found deductions worth 3.5 points. Only the first found the versioned Lua variables (2.0). Only the second found the export race, the wrong cleanup warning and `add --secret` (3.0 after the proof gap moved to H). A count of every security finding once gives 21.5 of 30, and a total of 81. So one reviewer for each area finds only a part of the defects, and a single total overstates the product.
- **Findings with no points.** Three reviewers reported defects outside their areas that no reviewer of the area found: private backup files are not private on a path longer than 260 characters (reproduced by the verification reviewer); the key-derivation guard accepts 4 GiB of memory together with 100,000 passes; AKM shows no old value of a rotated key.
- **Completion.** The CLI has no metadata edit (named by four reviewers) and no import. The desktop app shows no username.
- **Behaviour on a real desktop.** No reviewer started the app. Nobody ran these: the real clipboard test, the full native GUI proof, the native import and terminal windows, R4-04, a real OS lock, suspend and idle wait, install on a clean computer, real high-contrast themes, a real screen reader.

### Kinds of the deductions

The 16.5 points that the counted reviewers deducted, by the kind that each reviewer gave:

| Kind | Points |
|---|---:|
| `defect`: the code does a wrong or unsafe thing | 6.75 |
| `limit`: a weakness that the documents disclose | 2.75 |
| `release`: signing, licence, CI, install, publication | 2.5 |
| `design`: a design or threat-model choice | 2.0 |
| `missing`: a required workflow is absent on a surface | 1.5 |
| `proof`: nobody ran it on the real surface | 1.0 |

### The reviewers' own answers

Each reviewer answered two questions with no points: would it keep its own revocable API keys in this build today, and do its points say the same thing.

| Reviewer | Area score | Keep its own keys in this build? | Do the points say the same? |
|---|---:|---|---|
| Architecture and premise | 24 / 25 | Yes, with precautions; the CLI for disclosure until the owner-present desktop proofs pass | Only partly |
| Security, first | 25 / 30 | Yes, with precautions, for CLI use; no desktop disclosure or clipboard use until the proofs pass | Only conditionally |
| Security, second | 23 / 30 | Yes, with precautions; no file export until the two defects are repaired, no clipboard use until the proofs run | Only partly |
| Data and correctness | 20.75 / 25 | Yes, with precautions; no desktop metadata edit until the defect is fixed | Only partly |
| Desktop quality | 9 / 10 | No | No |
| Verification and shareability | 5.75 / 10 | No, for daily desktop use | Only partly |

Every reviewer wrote that its points overstate the readiness of the desktop app for daily use, because the points cannot stand for the flows that nobody ran on a real Windows session.

### Limits of this score

- A score from one review model, not a professional audit.
- Each area except C and D has one reviewer.
- Reviewers read code and ran small probes with fake data. The suite and real-window results that they saw are reported by the implementer. Each reviewer matched the three build hashes.
- The scale follows the 9 Nov rescore, with another model (Sol, not Terra).

The text below is each reviewer's, except that links to a reviewer's local checkout are written as repository paths, and each reviewer's closing notes about its temporary folder are removed.

---

## Reviewer 1: architecture and premise (A and B)

Reviewed `1d5cc82`, with builds reported from `d3b6fb1`. All three build hashes match the brief. I ran 17 isolated CLI invocations with fake data; all 18 probe assertions passed. The checkout remains unchanged.

**Area A: 10 / 10**

The narrow Windows/KeePass positioning and the integration’s limitations are stated accurately.

| Points | Severity | Kind | Finding | Evidence | What would win points back |
|---:|---|---|---|---|---|
| 0 | None | None | No deduction established. | Inspection | Not applicable. |

| 9 Nov deduction | Result | Evidence |
|---|---|---|
| FR-10: clipboard cleanup promise, −1.0 | Holds | `docs/FIRST_USE.md:19` now covers busy clipboard, unexpected exit and clipboard readers. The copy messages also describe an attempt. I checked the exit case by inspection; I did not touch the clipboard. |
| FR-11: unconditional cipher/KDF promise, −0.5 | Holds | `desktop/src/components/Unlock.tsx:127` limits AES-256/Argon2id wording to creation. Switching to unlock mode shows the existing-settings qualification, including for an existing 4.1 vault with different settings. Inspection. |

**Area B: 14 / 15**

| Points | Severity | Kind | Finding | Evidence | What would win points back |
|---:|---|---|---|---|---|
| −1.0 | Medium | design | One damaged device-local mapping disables disclosures for unrelated healthy vaults, and the new recovery flow cannot repair this condition. `crates/akm-core/src/journal.rs:747` refuses the mapping; `desktop/src-tauri/src/commands.rs:679` requires a successful snapshot before recovery. | **Reproduced:** truncating the fake mapping made reveals from two independent vaults and `audit --verify` exit 1. Restoring that mapping restored disclosure. **Inspection:** Activity offers recovery only after receiving a corrupt-chain snapshot. | Provide deliberate recovery that preserves the damaged mapping and existing journals, resumes healthy vaults, and identifies any history gap. Test two vaults through the recovery caller. |

| 9 Nov deduction | Result | Evidence |
|---|---|---|
| FR-12: unseen clone silently joins another journal, −2.5 | Holds | An unseen replacement produced an unconfirmed-lineage notice without changing the mapping. A failed append followed by reopening still warned on successful disclosure. A subsequent rename retained the journal. Reproduced on the release CLI. |
| FR-14: no in-app journal recovery, −1.5 | Partly holds | Damaged chain files now have confirmed, evidence-preserving recovery through `crates/akm-core/src/audit.rs:522` and the native caller, inspected. Mapping damage remains outside that recovery path, as scored above. The real recovery window was not run. |
| Desktop launch, raw −2.0 | Holds by inspection; no B points reclaimed | Scoped terminal launch now exists in `desktop/src-tauri/src/launch.rs:394`. The official 9 Nov score had already assigned this deduction to G. Its real windows remain unproved. |

The candidate-save comparison, short writer locks, disclosure capabilities and session commit boundary support the remaining architecture points. These are source observations within the stated cloud-sync and trusted-account limits.

**4. Findings outside my areas, with no points**

| Area | Severity / kind | Finding and evidence | Closing test |
|---|---|---|---|
| F | Medium / missing | **CLI metadata editing is absent.** The command map at `crates/akm-cli/src/main.rs:122` exposes no update operation, although the core and desktop support one. Changing an existing key’s project or variable name cannot be completed through the CLI while retaining its identity/history. **Reproduced:** `edit` and `update` exit 2; the vault remains unchanged. | Edit existing metadata through the CLI, reopen, and verify the same UUID, secret and history with only the requested fields changed. |
| F | Medium / defect | **Editing unrelated metadata changes an existing expiry time.** `desktop/src/components/KeyForm.tsx:56` keeps only the date; submission at line 111 reconstructs its last second through `desktop/src/schedule.ts:26`. An expiry of `2030-06-01T03:04:05Z` becomes `2030-06-01T23:59:59Z` after a notes-only edit. **Inspection.** | Edit notes on a key with a non-midnight expiry. Require the original expiry in the submitted draft and reopened vault. |

**5. What I did not cover**

I did not run builds, full suites, interoperability tools, desktop windows, clipboard operations, settings access, OS-session tests, installation or network activity. The supplied suite and native-proof results remain project-reported evidence. I did not independently audit the other reviewers’ areas.

**6(a). Would I keep my own revocable keys here?**

**Yes, with named precautions:** low-value revocable keys, a local vault, an independent encrypted backup, a tested KeePassXC escape route, and an interactive password prompt. I would use the CLI for disclosure until the owner-present desktop proofs pass.

The first failure I expect a CLI user to meet is the inability to change an existing key’s scope or metadata without switching applications.

**6(b). Required workflows**

“Complete” below describes the available implementation within its documented limits. Each cell states the evidence I used.

| Required workflow | CLI | Desktop |
|---|---|---|
| Create, unlock, search, inspect and manage | **Partly complete.** Creation and disclosure ran; command map inspected. Metadata editing is missing. | **Complete by inspection.** Project reports real unlock/list proof; I did not run it. |
| Preserve metadata | **Complete for storage by inspection.** Broader preservation relies on reported interchange results. | **Partly complete.** The inspected editor changes untouched expiry times. |
| Rotation and recoverable history | **Complete by inspection**, with old-value recovery through KeePassXC. I ran rotation; history/interchange proof is project-reported. | **Complete by inspection** on the same storage basis. No real-window rotation/history proof. |
| Copy with privacy exclusions and cleanup | **Missing.** No CLI copy command. | **Complete by inspection.** Real clipboard behaviour remains unproved. |
| Reviewed import; scoped export/injection | **Partly complete.** No import command. Export/injection code inspected; their execution is project-reported. | **Complete by inspection.** Save `.env` proof is reported; native import and terminal windows remain unrun. |
| Encrypted backups and useful private journal | **Partly complete.** Journal failure paths ran; mapping recovery is missing. Backup behaviour inspected and project-reported. | **Partly complete.** Same recovery limit. Real Activity proof is project-reported. |
| Defensive cloud-folder use | **Complete within disclosed limits, by inspection.** Stale-write tests are project-reported. | **Complete within disclosed limits, by inspection.** No independent desktop sync proof. |

**7. Do the points say the same thing?**

Only partly. The scores reflect a coherent architecture and candid positioning. They overstate practical readiness if read as proof that every required workflow is complete or that desktop disclosures work on Windows. The completion table and owner-present proofs remain necessary.

---

## Reviewer 2: security (C and D), first reviewer

Reviewed `1d5cc820`. The compiled source matches `d3b6fb1`; subsequent changes are documentation and instructions. All three executable hashes matched the brief.

**Area C: 14 / 15. Security at rest.**

| Deduction | Severity | Kind | Finding and evidence | What would win the points back |
|---:|---|---|---|---|
| 0.5 | Low | limit | Purging a key leaves it recoverable from encrypted backups using the same password. **Reproduced:** the pre-purge `.bak` revealed the purged fake key. Backup creation is at `crates/akm-core/src/vault.rs:1090`. | An explicit backup-retirement or re-key workflow, tested across failures, with honest limits for copies elsewhere. |
| 0.5 | Low | limit | The journal cannot establish that its valid tail is complete. **Reproduced:** deleting its last complete record still produced a consistent verification result. The reader accepts the remaining chain at EOF in `crates/akm-core/src/audit.rs:423`. | An independently protected integrity anchor, tested against truncation and replacement. Another same-account writable file would not provide that protection. |

The disclosed limits remain real weaknesses. I found no additional C deduction in the inspected encryption, save-verification or protected-export paths.

| 9 Nov deduction | Fix assessment | Evidence |
|---|---|---|
| FR-04: unreported plaintext export remnant, 2 points | **Holds** | Inspection of a second scenario, validation failing after plaintext preparation: dropping the temporary file records an unsuccessful delete, and the caller preserves an application warning across locking. See `desktop/src-tauri/src/private_export.rs:227` and `desktop/src-tauri/src/commands.rs:1072`. I did not rerun injected desktop failures. |
| FR-05: inherited vault/backup permissions, 1 point | **Holds** | Reproduced under a fake folder granting inheritable Everyone-read access. The new vault, `.bak` and dated backup each had a protected DACL containing only the current user. |
| FR-06: purged keys remain in backups, 0.5 points | **Does not hold as a fix** | Disclosure is accurate, but the fake key remained recoverable after purge. |
| FR-07: no journal anchor, 0.5 points | **Does not hold as a fix** | Record-boundary tail deletion remained undetected. The UI’s qualification is accurate. |

**Area D: 11 / 15. Security in use.**

| Deduction | Severity | Kind | Finding and evidence | What would win the points back |
|---:|---|---|---|---|
| 2 | High | limit | The documented finite deny list still admits `LUA_INIT_5_4`, `LUA_PATH_5_4` and `LUA_CPATH_5_4`. **Reproduced:** add, export and windowless child injection accepted them alongside another fake secret. The Lua rule lists only unsuffixed forms at `crates/akm-core/src/model.rs:784`; a Lua 5.4 child can interpret the versioned forms as startup code or module locations. Actual Lua execution was not run. | Reserve the versioned Lua families. Add regressions through import, add/update, export and launch, including a harmless real Lua startup sentinel. |
| 1 | Medium | design | A compromised page can still collect five single-key values per rolling minute without native approval. Calling `vault_activity` can keep the session available while this continues. **Inspection:** `desktop/src-tauri/src/session.rs:723` grants requests from a renewable budget, and `desktop/src-tauri/src/commands.rs:374` reads the requested key. This is conditional on page compromise; I found no compromise entry point. | Independent approval for single-key disclosure, or another native authority that an arbitrary page request cannot renew. Test a paced loop beyond one minute. |
| 0.5 | Low | limit | Avoidable plaintext buffers remain outside zeroizing ownership. **Inspection:** dotenv escaping builds an ordinary intermediate `String` at `crates/akm-core/src/dotenv.rs:46`, and CLI injection copies values into `std::process::Command` at `crates/akm-cli/src/child.rs:55`. The corrected stdin buffer does not eliminate these copies. | Remove or zero the avoidable owned copies, with focused allocation/cleanup evidence. Keep the separate JavaScript, database and OS memory limits explicit. |
| 0.5 | Low | limit | `AKM_PASSWORD` remains in the invoking shell’s environment. Child scrubbing protects the selected child, but other children of that shell can still receive it. **Inspection:** `crates/akm-cli/src/main.rs:995`. The fake child-scrubbing probe passed. | A supported automation authentication path that does not retain the master password in the parent environment. The hidden prompt already avoids this exposure for interactive use. |

| 9 Nov deduction | Fix assessment | Evidence |
|---|---|---|
| FR-02: executable-control variables, 2 points | **Partly holds** | Cargo wrappers, `HOST_CC`, `TARGET_CC`, `GIT_EDITOR` and credential-file controls were refused in my probes. Versioned Lua controls still reached the child. |
| FR-03: compromised-page bulk disclosure, 2 points | **Partly holds** | Bulk preview/copy now require native approval and recheck the approved snapshot. Paced single-key extraction remains possible through the free budget. Inspection only. |
| FR-08: unzeroized secret strings, 1 point | **Partly holds** | Stdin now uses bounded zeroizing storage. Escaping intermediates and CLI command-environment copies remain. Inspection only; no memory-dump test. |
| FR-09: parent password environment, 0.5 points | **Does not hold as a fix** | The exposure remains deliberately supported and disclosed. Child scrubbing still works. |

**Findings outside C and D, with no points.**

- **E, availability:** the KDF guard accepts 4 GiB of Argon2 memory and 100,000 passes together. A header-only program using the exact current source returned `Ok(())`. These independent ceilings do not provide a practical work bound for an unauthenticated file. See `crates/akm-core/src/kdf_guard.rs:24`. Guard acceptance was **reproduced**; the expensive derivation was deliberately not executed. A closing test should enforce a combined work budget or a safely cancellable derivation.
- **F, workflow completion:** the CLI command set has no reviewed import or metadata-edit operation. See `crates/akm-cli/src/main.rs:121`. Earlier versions remain stored, but AKM exposes history descriptions rather than an old-value retrieval or version-restore action; desktop history returns only `desktop/src-tauri/src/commands.rs:520`. These are **inspection** findings relevant to the completion table.

**What I did not cover.**

I did not launch the desktop app or installer, access settings or the clipboard, change the Windows session, use the network, run workflows, or run full suites/builds. I did not reproduce Lua hook execution, inspect process-memory remnants, exercise real cloud synchronization, or independently rerun interoperability proofs. Supplied suite and native-window results remain project-reported evidence. Hash matches establish artifact identity, not independent build provenance.

**6(a). Would I keep my own revocable keys in this build?**

**Yes with named precautions, for CLI use:** private NTFS storage, the hidden password prompt, secret input through stdin, trusted child programs and scopes, and a separate encrypted backup. I would defer desktop disclosure and clipboard use until the owner-present proofs pass.

The first failure I expect a desktop user to meet is a copied key surviving its intended cleanup because the clipboard is busy. That is a prediction from the failure handling, not a real-clipboard observation in this review.

**6(b). Required workflow completion.**

“Complete in code” below does not mean proved on the real desktop.

| Required workflow | CLI | Desktop |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Partly complete.** I ran create, add, inspect and purge; read selection code. Metadata editing is absent. | **Complete in code.** Read commands/session handling; took the project’s word for native unlock/list proof. Full management journey unrun. |
| Preserve metadata | **Complete in code.** Read field handling; relied on reported interchange results for broader preservation. | **Complete in code.** Read drafts and shared core; native metadata-edit journey unrun. |
| Rotation and recoverable history | **Partly complete within AKM.** Rotation and history listing exist; old values require an external KDBX reader or backup. Read code and relied on reported rotation proof. | **Partly complete within AKM.** Same recovery limitation. Read code; native flow unrun. |
| Clipboard privacy and ownership-aware cleanup | **Missing**, intentionally: no CLI copy command. | **Complete in code, native proof pending.** Read publication, ownership and cleanup paths. Did not touch the clipboard. |
| Reviewed import; scoped export/injection | **Partly complete.** Import is absent. Ran scoped export/injection; reproduced the Lua-control gap. | **Partly complete.** Read native import, export and terminal paths. Lua-control gap applies; took reported Save .env proof, while import/terminal windows remain unrun. |
| Encrypted backups and a private journal | **Complete with disclosed limits.** Ran backup recovery, ACL, journal-damage and truncation probes. | **Complete in code with the same limits.** Read callers; took reported Activity proof. |
| Sync-folder conflict detection and stale-write refusal | **Complete at the disclosed local-refusal level.** Read fingerprint/commit checks; relied on reported tests. No real sync-client probe. | **Complete at the same level.** Read shared core/session integration; no real sync-client probe. |

**7. Do the points say the same thing?**

Only conditionally. They support cautious CLI use. They overstate confidence for daily desktop use because real clipboard, native approval, lock/suspend and idle behavior remain unproved. C and D also do not measure the missing CLI workflows.

---

## Reviewer 3: security (C and D), second reviewer

Reviewed `1d5cc820098678172adbd8da4365f66c2704ceb7`. Product source matches `d3b6fb1`; all three build hashes match the brief. The checkout remains unchanged.

**Area C: 11.5 / 15**

| Points | Severity | Kind | Finding and evidence | What would win the points back |
|---:|---|---|---|---|
| −1.5 | Medium | defect | **A CLI export can follow a substituted parent directory.** After destination validation, I held the isolated journal mapping lock, replaced the chosen parent with a junction, then released the lock. The release CLI exited 0 and wrote fake plaintext into the redirected local folder. Its private ACL survived; another-account access was not demonstrated. **Reproduced.** `crates/akm-cli/src/export.rs:162`. | Preserve the checked directory identity through publication, or refuse its replacement before writing plaintext. Add a regression with this interleaving that leaves no plaintext in the redirected folder. |
| −1.0 | Medium | defect | **A failed desktop cleanup can identify the wrong file.** Rename the owned temporary file, put an unrelated sentinel at its old name, then refuse deletion. The warning names the sentinel and says “Delete it yourself”; the renamed plaintext remains elsewhere. **Reproduced with the production module and injected delete refusal, without the GUI.** `desktop/src-tauri/src/private_export.rs:227`. | Report the current name obtained from the owned handle. If that cannot be resolved, identify the cached name as historical rather than directing deletion there. Test renamed plaintext, replacement sentinel and refused cleanup together. |
| −0.5 | Low | limit | **Purge leaves usable secrets in encrypted backups.** After purging a live fake entry, the convenience backup still revealed its value with the same password. This is disclosed. **Reproduced.** `crates/akm-core/src/vault.rs:1142`. | Provide deliberate retirement or re-keying of managed backups, with preservation tests. Copies outside AKM still require provider revocation and user handling. |
| −0.5 | Low | limit | **The journal cannot establish completeness.** Removing its final complete record still produced a successful, “consistent” verification result. The output correctly disclaims authentication and completeness. **Reproduced.** `crates/akm-core/src/audit.rs:419`. | Demonstrate an independently protected checkpoint against tail removal and chain replacement. Disclosure alone does not remove this weakness. |

The 9 November deductions resolve as follows.

| Earlier deduction | Fix verdict | Evidence |
|---|---|---|
| FR-04, unreported plaintext remnant, −2 | **Partly holds** | Failed cleanup now produces a warning, including after cancellation. The renamed-file scenario above makes that warning identify an unrelated file. One point returns. |
| FR-05, inherited vault/backup permissions, −1 | **Holds** | In a fake parent deliberately granting Everyone read access, the new vault and its backups had protected private ACLs. Unsupported-storage fallback remains explicitly warned. |
| FR-06, backups after purge, −0.5 | **Does not hold; disclosed limit** | The live-purge backup follow-up recovered the purged fake value. |
| FR-07, no journal anchor, −0.5 | **Does not hold; disclosed limit** | Removing a complete tail record remained undetectable. |

**Area D: 11.5 / 15**

| Points | Severity | Kind | Finding and evidence | What would win the points back |
|---:|---|---|---|---|
| −1.0 | Medium | design | **Native bulk approval does not prevent gradual disclosure by a compromised page.** Five single-key requests per rolling minute need no approval. A compromised page can obtain IDs, read five distinct keys, wait for slots to expire and repeat; the activity command can keep its session active. **Inspection; no page-compromise entry point or exploit reproduced.** `desktop/src-tauri/src/session.rs:723`. | Require independently established authorization for the disclosures this threat model intends to prevent. Test distinct keys across multiple rolling windows, rather than only a burst. |
| −0.5 | Low | limit | **Secret cleanup remains incomplete.** The CLI copies injected values into `std::process::Command`, whose copies are not zeroized. Webview and OS memory also have no erasure guarantee. This is defense in depth under the stated trusted-account boundary. **Inspection.** `crates/akm-cli/src/child.rs:57`. | Remove avoidable owned copies and verify cleanup of buffers AKM controls, including failure paths. Retain honest limits for JavaScript and OS copies. |
| −0.5 | Low | limit | **`AKM_PASSWORD` remains exposed in the supplying shell’s environment.** Child scrubbing works, but cannot remove the password from that parent. **Inspection; child scrubbing reproduced.** `crates/akm-cli/src/main.rs:996`. | Retire the parent-environment credential path in favour of an automation input that does not retain the master password there. The interactive prompt is the current precaution. |
| −0.5 | Low | limit | **`add --secret` still permits command-line exposure of a key.** The help warns against it, but the value can enter process inspection and shell history. **Inspection.** `crates/akm-cli/src/main.rs:116`. | Remove this option and test that secret arguments are refused without echoing their values. Keep prompt and stdin input. |
| −1.0 | Medium | proof | **The principal native protections remain unproved on their real Windows paths.** Real clipboard exclusions/cleanup, OS lock and suspend, idle expiry, native disclosure/import approval and terminal interaction have not run on this build. **Inspection of the accepted project facts.** `docs/STATE.md:88`. | Run the owner-present security scenarios on the identified build and retain their outcomes. Count this same proof gap only once if H also reports it. |

The 9 November deductions resolve as follows.

| Earlier deduction | Fix verdict | Evidence |
|---|---|---|
| FR-02, executable-control variables, −2 | **Holds for the established fault** | The release CLI refused `RUSTC_WORKSPACE_WRAPPER`, a Cargo target runner, `HOST_CC`, `GOOGLE_APPLICATION_CREDENTIALS` and `AWS_SHARED_CREDENTIALS_FILE`, without printing their values. Import and legacy-entry planning use the same rule by inspection. The finite-list limitation remains. |
| FR-03, page-controlled bulk disclosure, −2 | **Partly holds** | Bulk preview and copy now require native approval, with revision/session checks afterwards. The rolling single-key allowance retains the exposure above. One point returns. |
| FR-08, ordinary secret strings, −1 | **Partly holds** | Stdin now uses bounded zeroizing storage, and the desktop builds a zeroizing environment block. CLI command copies and broader memory limits remain. Half a point returns. |
| FR-09, parent password environment, −0.5 | **Does not hold; disclosed limit** | The child received no `AKM_*` variables, but the parent exposure remains unchanged. |

**4. Findings outside C and D, without points**

The CLI has no metadata-edit command and no import command. Both `edit --help` and `import --help` returned “unrecognized subcommand” on the release binary. Existing entries cannot have their project, environment or variable name changed through the CLI.

This is a completion gap for F to assess against the required workflows, rather than another security deduction. The command inventory is at `crates/akm-cli/src/main.rs:122`. A closing metadata-edit test should preserve the UUID, secret and history while changing scope metadata.

**5. What I did not cover**

I did not start the desktop app, open windows, touch clipboard/settings/real vaults, change the OS session, install anything, use the network, start workflows, or run full builds or suites.

The renamed-remnant reproduction exercised unchanged production filesystem code with fault injection, not Tauri’s error transport. I did not reproduce a compromised-page entry point, inspect process memory, audit cryptographic dependencies, or independently repeat interchange and reported GUI proofs. The initial CLI driver stopped at an incorrect lookup of a recycled backup entry; the separate live-purge follow-up succeeded.

**6(a). Would I keep my own revocable API keys here today?**

**Yes, with named precautions.** I would use a strong unique master password, prompt/stdin input and scoped CLI injection on a trusted Windows account. I would avoid file exports until the two C defects are repaired, avoid clipboard-dependent use until native proof exists, and revoke keys at their providers before treating purge as retirement.

The first security failure I expect a user to meet is a copied value remaining after a busy-clipboard cleanup attempt.

**6(b). Required workflows**

“Complete by inspection” means implementation is present; it does not claim real-desktop proof.

| Required workflow | CLI | Desktop |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Partly complete.** Ran create/add/list/get/remove/purge; read selection code. Metadata editing is missing. | **Complete by inspection.** Read native commands and views; took the project’s word for real unlock/list proof. Other management flows were not run. |
| Preserve metadata | **Complete by inspection.** Read field handling; interchange results are project-reported. | **Complete by inspection.** Read draft merging and view mapping; no independent real-window preservation proof. |
| Rotation tracking and recoverable history | **Complete by inspection/reported proof.** Read the relevant contracts and code; did not independently run rotation/history. | **Complete by inspection/reported implementation.** No real-window proof. |
| Copy with privacy exclusions and ownership-aware cleanup | **Missing intentionally.** No CLI copy command. | **Complete by inspection; runtime unproved.** Read publisher, lease and session handling; no clipboard use. |
| Reviewed import; scoped export or injection | **Partly complete.** Import is missing. Scoped export/injection ran; export has the reproduced directory race. | **Partly complete.** Native import/launch paths exist by inspection; Save .env proof is project-reported, and cleanup has the reproduced defect. |
| Encrypted backups and a private useful journal | **Complete within disclosed limits.** Ran permission, metadata-minimization, refusal, backup and tail-removal probes. Retention-suite results are project-reported. | **Complete by inspection/reported proof.** Activity proof is project-reported; no independent GUI run. |
| Defensive sync use and stale-write refusal | **Complete by inspection.** Read fingerprint/replacement checks; no live sync-client experiment. | **Complete by inspection.** Read shared persistence and session handling; automated results are project-reported. |

**7. Do the points agree with 6(a)?**

Only partly. The defects support conditional personal use through selected workflows. The points overstate confidence in everyday desktop use because one proof deduction represents several important Windows behaviours that remain unobserved. They measure established weaknesses, not the probability of losing or exposing a key.

---

## Reviewer 4: data integrity and correctness (E and F)

Reviewed `1d5cc820098678172adbd8da4365f66c2704ceb7`. Product code is unchanged from `d3b6fb1`. Both executable hashes match the brief. I ran 36 recorded CLI commands and small source/core probes with fake data. The checkout remains unchanged.

**Area E: 12 / 15**

| Deduction | Severity | Kind | Finding and evidence | What would win the points back |
|---:|---|---|---|---|
| 2 | Medium | defect | **Saving an edit changes untouched metadata.** `desktop/src/components/KeyForm.tsx:53` joins and splits scopes/tags, and reduces expiry to a date. Submitting an unchanged fake entry changed `00:00:45Z` to `23:59:59Z`, split `"literal,scope"` into two scopes, and split a comma-containing tag. **Reproduced through the production form’s submit function with mocked React/API boundaries; no desktop window ran.** | Preserve original values for untouched fields. Add a regression that edits only notes, passes through native update, reopens the vault, and checks the exact expiry, scopes and tags. |
| 1 | Medium | defect | **Saving through a file symbolic link splits the vault.** Opening retains the supplied path, while only the lock path is canonicalized. Replacement at `crates/akm-core/src/vault.rs:1121` therefore replaces the link rather than its target, leaving the original vault stale. **Inspection.** The runtime probe could not create a symbolic link because Windows returned error 1314. | On a link-capable Windows runner, prove that a save updates the intended target while preserving the link, or refuses before changing either file. |

| 9 Nov deduction | Result | Evidence |
|---|---|---|
| 3 points, backup pruning deletes unowned files | **Holds under the narrowed contract.** | Seven unlisted sentinel backups, including valid-looking names with true content hashes, survived seven further saves. Current pruning follows the encrypted ledger. Copied folders rotating their listed backups is now explicitly permitted. |

The focused core probe also verified exact import isolation between same-project/different-environment and different-project/same-environment entries. A blocked replacement preserved the old secret in memory and after a later save. Damaged-journal recovery preserved the damaged bytes and restored disclosure.

**Area F: 8.75 / 10**

| Deduction | Severity | Kind | Finding and evidence | What would win the points back |
|---:|---|---|---|---|
| 1 | Medium | missing | **The CLI cannot edit existing metadata.** The complete command set at `crates/akm-cli/src/main.rs:122` has no update/edit operation. A CLI user cannot correct a project, issuer, expiry or environment-variable name while preserving the entry’s identity and history. This leaves the required management workflow incomplete on that surface. **Inspection.** | Provide metadata editing through the CLI. Verify after reopening that the requested field changed while the UUID, secret and rotation time remained intact. |
| 0.25 | Low | defect | **A valid stored rotation interval blocks desktop metadata editing.** Core accepts up to 36,500 days, but `desktop/src/components/KeyForm.tsx:92` rejects anything above 3,650. The CLI accepted 36,500; submitting that unchanged value through the production form failed before update. **Reproduced with the CLI and source-level form probe.** | Share the supported bound, and test editing an entry at that bound without changing its interval. |

| 9 Nov deduction | Result | Evidence |
|---|---|---|
| 2 points, rotation interval causes panics | **Holds.** | Oversized inputs were refused without changing the vault. The largest valid interval worked. Extreme expiry-filter and retention counts returned controlled results. Stored-date arithmetic now uses checked operations. |
| 1 point, NUL-containing secrets accepted | **Holds.** | Add and rotate refused fake NUL-containing stdin; vault bytes remained unchanged. Update’s changed-secret path also checks NUL by inspection. |
| 0.5 points, Unicode title matching | **Holds.** | Accented Latin, Greek final sigma and capital sharp-S cases selected correctly through live and recycled CLI selectors. |
| 0.5 points, expiry precision discarded | **Partly holds.** | Nonzero fractions, including beyond nine digits, were refused without mutation. Zero fractions and offsets round-tripped correctly. Desktop editing still discards time-of-day precision, charged once in E above. |

**Findings outside my areas:** none established. I have not charged the owner-present proof gaps again in E or F.

**What I did not cover:** full builds or suites; real desktop CRUD, import confirmation or terminal windows; clipboard; settings; OS lock/suspend/idle; installation; network access; exhaustive foreign-KDBX preservation; real cloud-sync races; every child-command quoting case. The symbolic-link scenario remains unrun. Reported interoperability and native-window results were taken from the brief.

**6(a).** **Yes, with named precautions, considering E and F alone.** I would keep an independent encrypted backup, use a direct vault-file path, and avoid desktop metadata edits until the preservation defect is fixed. The first failure I expect is an edit to notes silently moving a precisely timed expiry to the end of its date.

**6(b).**

| Required workflow touching E/F | CLI | Desktop |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Partly complete.** Creation, inspection, selection and lifecycle operations ran; metadata editing is missing. | **Partly complete.** Commands and forms read; unlock/list reported as run. CRUD was not run on a real window. |
| Preserve metadata | **Complete in the reviewed code.** Expiry spot checks ran; broader interchange was reported. | **Partly complete.** Untouched metadata changes during editing. |
| Rotation and recoverable history | **Complete.** Rotation and history ran; core probe checked history after import and failed rotation. | **Partly complete.** Code read; interval mismatch reproduced; real-window flow unrun. |
| Reviewed scoped import | **Missing.** No CLI import command. | **Partly complete.** Core isolation ran and native review/write code was read; actual confirmation remains unrun. |
| Scoped export and injection | **Complete.** Explicit scope, export and direct-child injection ran. | **Partly complete.** Export proof reported; launch code read; terminal window unrun. |
| Encrypted backups and useful private journal | **Complete within disclosed limits.** Backup sentinels, audit, refusal on damage and core recovery ran. | **Complete in code; partly proved.** Activity/export proofs reported; recovery window unrun. |
| Detect conflicts and refuse stale writes | **Complete within the local-lock limit.** Stale writer was refused. | **Complete in shared core code; desktop execution unrun.** |

Clipboard privacy and cleanup were outside my assigned assessment.

**7.** Only partly. These points reflect a strong transactional core, but overstate readiness for daily desktop use. A preservation defect in an ordinary edit matters more than its small share of the rubric, and points cannot establish that the unrun native workflows work.

---

## Reviewer 5: desktop quality (G)

**Area G: 9.0 / 10.**

Reviewed source `1d5cc82`, whose desktop code matches `d3b6fb1`, and the supplied built page. All reproductions below used headless Edge on loopback with a mocked native boundary.

| Deduction | Severity | Kind | Finding and evidence | What wins the points back |
|---:|---|---|---|---|
| 0.50 | Low | missing | A stored username cannot be viewed or edited in the desktop app. A fake entry with username `existing-user` showed no username in its drawer or edit form. `desktop/src/components/KeyDetail.tsx:182`. **Reproduced.** | Expose the username in detail and editing, with a regression proving that it remains visible and round-trips correctly. |
| 0.50 | Low | defect | An import that finishes after the user returns to Keys leaves that view stale. After a mocked view-expiry event, navigation to Keys, and import completion, native state held two keys while the page still showed “All keys 1” and omitted the imported project. Navigation away and back repaired it. `desktop/src/components/KeysView.tsx:53`. **Reproduced.** | Refresh an already-mounted key list after an applied import. Test completion through both the reply and the retained outcome record after leaving Use keys. |

The nine deductions from 9 November were checked separately.

| Previous deduction | Result | New evidence |
|---|---|---|
| UI-R2-01, Recycle Bin identification, 1.25 | **Holds** | At 900 × 620, duplicate titles with long common-prefix projects, long variables and different history counts remained distinguishable. The permanent-delete confirmation contained the selected entry’s full ID and facts. |
| FR-19, identical accessible descriptions, 0.75 | **Holds** | IDs differing only at their final character produced distinct descriptions in Edge’s accessibility tree. |
| FR-13, desktop scoped launch missing, 1.00 | **Holds** | The built page dispatched the selected project and environment, handled cancellation and success, and explained terminal retention. Native launch code was inspected; no real terminal was opened. |
| FR-20, detailed history missing, 0.50 | **Holds** | The drawer displayed a notes-change version and a secret-change version, including an unknown timestamp, without values. |
| FR-21, failed reads look empty, 0.50 | **Holds** | Initial key-list and Recycle Bin failures showed failure states without empty-state claims. Retrying recovered both. |
| UI-R2-04, clipped variables, 0.25 | **Holds** | Long variables differing at their suffix wrapped without horizontal overflow at 900 px. |
| UI-R2-03, missing unfiled heading, 0.25 | **Holds** | An entirely unfiled fixture showed “No project” and described the unset environment. |
| UI-R2-05, missing project total and expiry marker, 0.25 | **Holds** | The project total and visible “no expiry” text were present. |
| UI-R2-02, stale Settings count, 0.25 | **Holds** | Changing the mocked status count from one to two caused a fresh read and displayed two. |

**Findings outside G, with no points.**

- **Medium, defect, data integrity/correctness:** A notes-only edit changes untouched metadata. The outgoing draft changed expiry `2031-05-06T08:09:10Z` to `2031-05-06T23:59:59Z`, scope `["resource,subresource"]` to two scopes, and tag `["one,two"]` to two tags. The form reconstructs stored values from date-only and comma-separated controls. `desktop/src/components/KeyForm.tsx:53`. **Reproduced at the outgoing IPC boundary; persistence was inspected, not run.** Close it with a regression that changes only Notes and verifies that every untouched field retains its original value through save and reopen.

**What I did not cover.**

I did not run the native app, CLI, installer, full builds or suites. I did not access settings, real vaults, the clipboard or actual OS-session events. Real dialogs, terminal delivery, screen-reader operation, Windows high contrast, idle locking and suspend remain unproved here. Browser probes also cannot independently establish the supplied page’s build provenance.

Sampled drawer controls remained reachable at 900, 1041, 1180 and 1440 px. Sampled focus trapping, focus restoration, keyboard filtering and forced-colour boundaries worked. These were bounded checks, not exhaustive accessibility acceptance.

**6(a). Would I keep my own revocable API keys in this build today?**

**No.** I would first fix the metadata-edit defect and run the owner-present disclosure flows. The first failure I expect is an ordinary notes edit silently moving a precisely timed expiry to the end of its date.

**6(b). Required workflows.**

“Complete” below describes available implementation, not completed native acceptance.

| Required workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Complete** for the reported journeys; took the project’s word. | **Complete** for basic management in code; ran mocked create, list and detail journeys, and inspected management code. |
| Preserve metadata | **Complete** according to reported CLI and interchange proofs; not independently run. | **Partly complete.** Username inspection/editing is absent, and metadata editing is lossy. Ran the boundary reproductions. |
| Rotation tracking and recoverable history | **Complete** according to reported lifecycle and KDBX proofs. | **Complete in implementation.** Read rotation code and ran the history display with mocked data; native execution remains unrun. |
| Copy with privacy exclusions and ownership-aware cleanup | **Missing:** no CLI copy command, by inspection. | **Complete in code according to the project’s evidence.** Real clipboard behavior remains unrun. |
| Reviewed import; scoped export and injection | **Partly complete:** export and injection are reported proved; no CLI import. | **Partly complete.** Controls and dispatch exist, but delayed import completion can leave Keys stale. Ran mocked launch/export/import paths; inspected native callers. |
| Encrypted backups and useful private activity journal | **Complete** according to reported proofs. | **Complete in implementation.** Read the views and took the reported focused native proofs as evidence. |
| Detect conflicts and refuse stale writes in downloaded sync folders | **Complete in code** according to reported tests. | **Complete in code** according to reported native tests and inspected error/reload presentation; no live sync race run. |

**7. Do the points say the same thing as 6(a)?**

No. The 9/10 credits repaired desktop presentation and available workflows. It does not absorb the separately assigned data-integrity defect or turn mocked journeys into real Windows proof. It would overstate readiness if read as permission to trust the whole product.

---

## Reviewer 6: verification and shareability (H and I)

Reviewed `1d5cc82`, with builds attributed to `d3b6fb1`. All three executable hashes match the brief. Later commits change documents only. Hash matching establishes artifact identity, not build provenance.

Area H: 3.00 / 5

| Deduction | Severity | Kind | Finding and evidence | What wins the points back |
|---:|---|---|---|---|
| 1.00 | Medium | proof | Clipboard, native import and terminal confirmations, hide, OS lock/suspend and idle behaviour remain unrun. Production-handler tests still use a mocked runtime. **Inspection**, including the admitted gaps in `docs/STATE.md:88`. | Complete the required Windows proofs on the reviewed builds, including an accepted method for observing R4-04. |
| 0.25 | Low | defect | Both documentation-checker modes accept synthetic workflow evidence whose step has `if: false`, or only prints `'tools/test_install.ps1'`. The checker concatenates `run` text and searches for command substrings. **Reproduced** through `tools/check_docs.py:415`. | Add these negative fixtures. Reject the obvious disabled case and narrow the remaining promise to command-text presence rather than execution. |
| 0.25 | Low | release | Dependency checks are now configured, but no CI run validates this head or the substantially changed workflow. **Inspection**, using the supplied CI status and `.github/workflows/build.yml:141`. | A successful complete CI run on the reviewed revision. |
| 0.25 | Low | limit | Independent byte-for-byte reproduction remains unavailable. The new comparison is reporting-only and has never run; this limitation is disclosed in `docs/TESTING.md:66`. **Inspection**. | Demonstrate independent matching rebuilds and make that comparison enforceable. |
| 0.25 | Low | defect | After successful fake-vault operations, the verification helper’s `clean` terminated on a 260-character backup path and left owned session/state remnants. **Reproduced**, at `.agents/skills/verify-akm/scripts/akm-verify.ps1:130`. | A long-path cleanup regression that removes the owned session and state while preserving an unowned sentinel. |

The 9 November deductions:

| Earlier deduction | Fix verdict | Evidence |
|---|---|---|
| 0.75: native proof gaps | **Does not hold** | The focused window proofs still exclude the consequential Windows paths listed above. |
| 0.50: redesigns lacked independent acceptance | **Partly holds** | Later core and tools reviews independently challenged the redesigns, but acceptance remains pending. I charge the concrete remaining weaknesses, without an additional penalty for this full review being unfinished. |
| 0.75: metadata-only assertions and one-scope child fixture | **Holds** | `crates/akm-core/tests/vault.rs:177` now checks protected values across project-only and environment-only differences, reopening and later saves. `crates/akm-cli/tests/security.rs:357` compares the complete child environment across three scopes. My release-CLI probe independently confirmed scoped injection and scrubbing. |
| 0.25: “byte for byte” interoperability overclaim | **Holds** | `tools/verify_kdbxweb/verify.mjs:435` now names the compared properties. Omitted properties are explicitly outside that claim. |
| 0.25: control characters and non-test evidence accepted | **Partly holds** | Vertical tab, form feed, DEL and a non-test function were rejected in my probes. The workflow-evidence weakness above remains. |
| 0.50: dependency checks omitted from CI | **Partly holds** | The workflow invokes the checks and installs pinned tools; execution on this head remains absent. |
| 0.25: moving inputs and unproved reproducibility | **Partly holds** | Action, Rust and Node pins improved. The runner image still moves, and binary reproduction remains unproved. |

Area I: 2.75 / 5

| Deduction | Severity | Kind | Finding and evidence | What wins the points back |
|---:|---|---|---|---|
| 1.00 | Medium | release | The supplied builds are unsigned, and the proposed authenticated manual channel has no release key or signed release. A recipient cannot authenticate either the installer or a subsequent security fix. **Inspection**, `docs/RELEASING.md:52`. Signing and update-channel deficiencies are counted together here. | Deliver a release with authenticated checksums, a separately obtainable verification key and a tested verification procedure. |
| 0.50 | Low | release | Install, reinstall and uninstall remain unrun on a clean computer. The prepared job also tests same-version reinstall, rather than migration from an older version. **Inspection**, `docs/TESTING.md:152`. | Run the clean-machine preservation proof; add version migration coverage when an earlier release exists. |
| 0.50 | Low | release | A parentless snapshot avoids publishing private history, but the current tree still contains machine paths and unresolved publication decisions. **Inspection**, `docs/FINDINGS.md:375`. | Resolve the current-tree hits, inspect the resulting snapshot and publish only that cleared history. |
| 0.25 | Low | release | Repository licensing material now exists, but the installer and app deliver none of it. A recipient receiving only the installer misses the notices and source offer. This is disclosed in `docs/RELEASING.md:17`. **Inspection**. | Include the required licensing files with every distributed package and verify their presence during packaging. |

The 9 November deductions:

| Earlier deduction | Fix verdict | Evidence |
|---|---|---|
| 0.75: unsigned installer | **Does not hold** | Builds remain unsigned. Counted within the single authentication deduction above. |
| 0.25: no authenticated update/manual channel | **Partly holds** | A manual channel is specified, but its key and authenticated release do not exist. Counted with signing. |
| 0.75: missing project licence and dependency notices | **Partly holds** | Source licensing material exists. My nested-policy, SPDX JSON and author fixtures were included by the generator. Distribution packaging remains incomplete. |
| 0.50: missing security reporting route | **Holds** | `SECURITY.md:11` supplies public and private reporting routes and a fallback contact procedure. I did not test delivery. |
| 0.50: clean-computer installation unverified | **Does not hold** | Prepared scripts are useful, but no installation proof has run. |
| 0.50: private material in tree and history | **Partly holds** | The snapshot design excludes earlier history, and my Unicode-address, UTF-16 and lone-CR probes were refused. Current-tree clearance remains open. |
| 0.25: machine-specific verification defaults | **Holds** | Fetch, evidence and ownership defaults now derive from environment/profile locations, with explicit overrides. No particular drive is required. |

Findings outside my areas, with no points:

- **Medium, security at rest, reproduced.** A valid long NTFS backup path defeats private-file creation. In a fake directory with an inheritable Everyone-read ACE, the 130-character live vault was private, but both 267-character dated backups inherited Everyone access. The latest backup contained the prior fake key. `crates/akm-core/src/private_fs.rs:395` passes the ordinary path directly to Win32; failure then reaches the permissive fallback at `crates/akm-core/src/private_fs.rs:155`. Saves returned success with a warning. This establishes readable encrypted backups, not plaintext disclosure or successful access by another account. Close with a regression covering long paths in a shared fake NTFS directory, requiring private vault and backup ACLs.

What I did not cover:

I ran the fast documentation check, which passed for 40 claims, and small checker, notices, cleanup and release-CLI probes. I did not run full builds or suites, interoperability suites, GUI or installer flows, clipboard or OS-session tests, or network checks. Full-suite and existing window-proof results remain project-reported. I did not clear the complete history for publication or provide a legal assessment of the notices.

(a) **No for daily desktop use today, within these areas.** Revocable keys reduce recovery costs, but do not resolve the unproved disclosure and locking paths. The first difficulty I expect a new recipient to meet is installer publisher verification without an authenticated release.

(b) For H’s completion assessment, an implemented path remains partly complete where its required real-surface proof is outstanding.

| Required workflow | CLI | Desktop app |
|---|---|---|
| Create, unlock, search, inspect and manage entries | **Complete for ordinary paths.** Ran create, add, list and masked inspection; remaining paths read/project-reported. | **Partly complete.** Implementation and automated coverage present; full real-window journey unrun. |
| Preserve metadata | **Complete by inspected tests and reported interchange proofs.** My probe also read back scope metadata. | **Partly complete.** Tests/code evidence; full real-window preservation journey unrun. |
| Rotation and recoverable history | **Complete by code/tests and project reports.** I did not run rotation. | **Partly complete.** Implemented; real-window journey unrun. |
| Privacy-aware clipboard copy and cleanup | **Missing**, intentionally provided through the desktop. | **Partly complete.** Scripted coverage exists; real clipboard proof absent. |
| Reviewed `.env` import | **Missing**, intentionally provided through the desktop. | **Partly complete.** Native confirmation and dispatch tests exist; real confirmation unrun. |
| Explicitly scoped export and injection | **Complete for sampled paths.** Ran both export and a child receiving only the selected scope, no `AKM_*`, and preserving exit code 37. | **Partly complete.** Save `.env` is project-reported proved; native disclosure and terminal journeys remain unrun. |
| Encrypted backups and useful private journal | **Partly complete.** Journal verification ran successfully; the long-path backup-permission defect remains. | **Partly complete.** Activity is project-reported proved; broader backup and failure journeys remain unrun. |
| Conflict detection and stale-write refusal in downloaded sync folders | **Complete by inspected automated evidence and project reports.** Not independently run. | **Partly complete.** Automated evidence exists; real desktop conflict journey unrun. |

7. **Only partly.** The points reflect substantial engineering progress, while my desktop-use answer remains no. They underweight the uncertainty across several unrun native paths, and give release work weight that does not directly measure the safety of a locally built CLI.
