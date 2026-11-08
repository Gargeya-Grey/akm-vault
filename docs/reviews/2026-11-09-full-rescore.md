# Full rescore, 9 November 2026

- Model: Codex GPT-6.1 Terra, effort max. Owner's instruction, 8 November 2026.
- Reviewed commit: `c0a7d3bedc7306df260cc4cd91dbcbfd4c2b7ef8`, the head of branch `ui/key-cabinet`.
- Builds at that commit (unsigned, SHA-256): `akm.exe` `701f34f2155981e7397be083d81b87ff1cecc1331f39305486b366c4f7197bce`; `akm-desktop.exe` `5d6bd9cb4e5495691d74fab2b40fefc587cbc02689d5086b97804955774a49bd`; setup program `1f44ad9734f0a747d8a24637b9353e5d234788065962bd38b40ca04a57556e59`.
- Date: 2026-11-09.
- Rubric: the nine areas and 100 points of the [baseline review](2026-10-30-baseline.md).
- Reviewers: five independent reviewers, each with its own checkout and no view of the others' work. Together they cover the nine areas once.

## Result

**66.25 / 100. Not ready for real secrets.** The bar is above 90/100, followed by the owner's testing.

| Area | Points | Baseline | Now | Reviewer | Main deductions |
| --- | ---: | ---: | ---: | --- | --- |
| A. Premise and positioning | 10 | 5.5 | 8.5 | Architecture and premise | A clipboard promise and a vault-format promise that are too strong |
| B. Architecture and design | 15 | 7 | 11 | Architecture and premise | Journal identity of an unseen clone; no in-app recovery from a damaged journal |
| C. Security at rest | 15 | 7.5 | 11 | Security | A failed export can leave an unreported plaintext temporary file; vault permissions; backups after a purge; no journal anchor |
| D. Security in use | 15 | 4 | 9.5 | Security | `RUSTC_WRAPPER` and other execution controls are not reserved; a compromised page can reach bulk export and copy; unzeroized strings |
| E. Data integrity and durability | 15 | 4 | 12 | Data and correctness | Backup pruning deletes files that AKM did not create |
| F. Functional correctness | 10 | 3.5 | 6 | Data and correctness | A large rotation interval panics reads; a NUL secret is accepted; Unicode matching; fractional seconds |
| G. Desktop app quality | 10 | 6 | 5 | Desktop quality | UI-R2 findings; same-title keys cannot be told apart; launch and history only in the CLI; a failed read looks empty |
| H. Verification and hygiene | 5 | 2 | 1.75 | Verification and shareability | Native gaps; redesigns with no acceptance; tests that compare metadata and not secrets |
| I. Shareability | 5 | 1 | 1.5 | Verification and shareability | Unsigned; no licence text or SECURITY.md; no clean-computer install test; private paths in history |
| **Total** | **100** | **40.5** | **66.25** | | |

### How the total was counted

The reviewers' raw total is 64.25. One root cause was deducted in two areas: "the app cannot launch a program with scoped keys" cost 2.0 in B and 1.0 in G. The rule is that a root cause counts once, in the area where it belongs, and the rescore brief puts the desktop experience in G. The deduction stays in G, and B goes from 9.0 to 11.0. No other root cause appears twice. No points were added for any other reason.

### Limits of this score

- A score from one review model, not a professional audit.
- Each area has one reviewer, so an area score is one judgement. The baseline used three reviewers across the areas.
- Reviewers read code and ran small probes with fake data. They did not launch the app. The test-suite and real-window results that they saw are reported by the implementer. They matched the three build hashes, and some of them ran their own CLI probes.
- Nobody ran these: R4-04, the real clipboard test, the full native GUI proof, the native import dialog, Windows toast delivery, real OS lock and suspend, a real idle wait, install on a clean computer, real high-contrast themes, a real screen reader.
- Pull request 10 (terminal launch and key history) is not in this commit. It would address two of the G deductions, but it earns no points until it is reviewed and merged.
- The scale may differ from the baseline's: a different model and a different reviewer split. Compare the two totals with care.

The text below is each reviewer's, except that links to a reviewer's local checkout are written as repository paths, and each reviewer's closing note about its temporary folder is removed.

---

## Reviewer 1: security (C and D)

**Security scores: C 11/15; D 9.5/15.** These are one reviewer's scores for the requested dimensions at commit `c0a7d3bedc7306df260cc4cd91dbcbfd4c2b7ef8`. They do not establish the project's total score or its release gate of **above 90/100**.

I applied a bar suitable for a personal Windows secret manager handling real keys: strong protection of a stolen vault, clear disclosure boundaries, and truthful handling of plaintext and cleanup failures. The scores account for documented limitations as well as defects. High-severity scenarios below are marked **inspection** where their full effect was not run.

| Area | Maximum | Deductions | Score |
|---|---:|---:|---:|
| C. Security at rest | 15 | 4 | **11** |
| D. Security in use | 15 | 5.5 | **9.5** |

### Deductions

#### C. Security at rest

| Points | Severity | Root cause and scenario | Evidence and closing test |
|---:|---|---|---|
| -2 | Medium | A failed desktop export can leave a plaintext temporary file without telling the user cleanup was unconfirmed. The drop handler ignores the result of the Windows delete request after a write, sync, validation, or publication failure. The file has a private ACL, but may remain in a synced local folder or be readable by the same account. | **Inspection:** `desktop/src-tauri/src/private_export.rs:177` and discard at `:485`. Inject a failed delete after plaintext is written and require a warning that identifies the possible remnant. |
| -1 | Medium | Vaults and encrypted backups inherit the destination folder's permissions. In a shared folder, another account may obtain a copy and attempt an offline password attack. | **Inspection; partially reproduced:** `crates/akm-core/src/vault.rs:1521`. The fake vault inherited its ACL; that does not establish that this machine exposed it to another account. Test creation in a deliberately shared fake folder and define whether AKM refuses or applies a private ACL. |
| -0.5 | Low | Purging a key does not remove it from encrypted backups. This is disclosed and supports recovery, but there is no AKM operation that makes old backup copies inaccessible. | **Reproduced:** after purging a fake key, the pre-purge `.bak` still revealed it with the same password. See backup creation at `crates/akm-core/src/vault.rs:951`. A closing decision should cover backup retirement and copies already synchronized elsewhere. |
| -0.5 | Low | The local journal has no independent integrity anchor. A same-account program can remove a valid tail or replace the entire chain without AKM proving the omission. | **Inspection:** `crates/akm-core/src/audit.rs:102`. The UI correctly describes the journal as unauthenticated. A stronger design would need an independently protected anchor and tests for tail removal and full replacement. |

#### D. Security in use

| Points | Severity | Root cause and scenario | Evidence and closing test |
|---:|---|---|---|
| -2 | High | The reserved-variable list misses Cargo execution controls, including `RUSTC_WRAPPER`. An attacker can supply one in an imported `.env`; if the user later runs Cargo with that scope, Cargo can execute the selected wrapper with the scope's other secrets in its environment. | **Inspection of full attack; injection reproduced:** `crates/akm-core/src/model.rs:261` and `crates/akm-cli/src/child.rs:52`. A fake key named `RUSTC_WRAPPER` reached a child through `akm run`. I did not run a Cargo build; the [Cargo Book](https://doc.rust-lang.org/cargo/reference/environment-variables.html) documents the wrapper behavior. Add import, add, run and export regressions for Cargo's execution controls and review the broader policy. |
| -2 | High | A compromised webview can obtain the current session ID through `vault_status`, request `env_export` with `all: true`, and receive plaintext without an independent native approval. It can also request `env_copy` for all keys and place them on the system clipboard. Session IDs prevent stale-session actions; they do not authorize a user gesture. | **Inspection:** `desktop/src-tauri/src/commands.rs:67`, export at `:655` and copy at `:673`. This assumes page compromise; I did not find or reproduce a page-compromise entry point or network exfiltration. Test direct generated IPC calls from a compromised-page fixture and require native authorization for bulk disclosure. |
| -1 | Medium | Some secret-bearing strings are not zeroized. For example, `--secret-stdin` reads into an ordinary `String` and creates another ordinary string before wrapping the value later. The webview and decrypted database also retain plaintext while in use. | **Inspection:** `crates/akm-cli/src/main.rs:1022` and `desktop/src-tauri/src/session.rs:928`. Reduce avoidable copies and test the buffers under AKM's control; JavaScript and OS memory cannot be promised clean solely by such tests. |
| -0.5 | Low | When automation uses `AKM_PASSWORD`, the password remains in the parent process environment even though AKM removes `AKM_*` from the child environment. A hostile same-account program with sufficient process access can inspect the parent or shell. | **Inspection; child scrub reproduced:** `crates/akm-cli/src/main.rs:992` and `crates/akm-cli/src/child.rs:27`. The interactive prompt avoids this particular environment exposure. |

### What earned the points

- The fake-data CLI refused a short new password, an unscoped `run`, and the reserved `PATH` variable. The child received no `AKM_*` variables.
- A fake file export had a protected ACL, refused an existing destination, and left the existing file unchanged. An unusable fake journal blocked a secret reveal without printing the secret.
- Source inspection found encrypted temporary vault writes, read-back verification before replacement, encrypted backups, and Argon2id/AES settings for new vaults. A scan of the fake vault and backups found no literal fake secret bytes.
- Source inspection found session-bound commands, a native import confirmation with a post-dialog session and revision check, a restrictive CSP, clipboard exclusion formats, and a ten-minute native idle policy. These are source observations, not proof of the unrun native flows.

### Notes for other areas

- **Data integrity and availability:** vault opening at `crates/akm-core/src/vault.rs:131` reads the whole file and passes its KDF parameters to the library without an AKM-level resource bound. A malicious vault may cause excessive resource use. This is an inspection concern, not a reproduced denial of service.
- **Import-review correctness:** the native confirmation limits the number of changes but does not bound each displayed name or title. Whether an extreme valid input makes the real dialog unreadable or incomplete needs an owner-present native test. I did not score an unobserved display failure.

### Top actions to reach above 90

1. Close the Cargo and other executable-control variable gaps, then test hostile imports through the actual import and launch paths.
2. Decide and enforce what independent native approval a compromised webview must need for bulk reveal and clipboard disclosure.
3. Make failed desktop export cleanup observable and test the failure after plaintext has been written.
4. Decide the vault and backup permission policy for shared folders, and provide a clear backup-retirement procedure.
5. Complete the owner-gated native clipboard, import, idle, OS lock and suspend proofs on the exact reviewed build.

### Not covered

I did not launch the desktop app or installer, touch the clipboard, settings or real vaults, run a full build or test suite, or independently rerun the supplied GUI and interoperability proofs. The real import dialog, screen-capture behavior, Windows notifications, real idle timeout, lock and suspend remain unverified in this review. The supplied suite and native-proof results remain implementer-reported evidence.

---

## Reviewer 2: architecture and premise (A and B)

### Score table

Reviewed commit **`c0a7d3bedc7306df260cc4cd91dbcbfd4c2b7ef8`**. The bar is the project's proposed use with real secrets: an independent full review above 90/100, followed by owner testing. These are scores for my two assigned areas only; they are not a total product score.

| Area | Maximum | Deductions | Score |
|---|---:|---:|---:|
| A. Premise and positioning | 10 | 1.5 | **8.5** |
| B. Architecture and design | 15 | 6.0 | **9.0** |
| **Assigned areas** | **25** | **7.5** | **17.5** |

The premise has a reason to exist for a narrow audience: a Windows user who wants an account-free, portable KeePass vault with API-key metadata and scoped injection. It is not a broad replacement for existing tools. [KeePassXC](https://keepassxc.org/docs/KeePassXC_UserGuide) already offers local vault management and a CLI; [1Password](https://www.1password.dev/cli/reference/commands/run) and [Bitwarden](https://bitwarden.com/help/secrets-manager-cli/) provide subprocess injection; [dotenvx](https://dotenvx.com/docs/cli/run/) offers encrypted dotenv injection; and [Windows Credential Manager](https://learn.microsoft.com/en-us/windows/win32/secbp/handling-passwords) supplies OS-bound credential storage. AKM's combination is plausible, but its GUI does not yet deliver the file-free injection part of that combination.

The decision everything else rests on is to treat an independently editable KDBX file as the source of truth while inferring a separate, device-local journal's identity from file observations. The clone probe below shows why those observations cannot always establish the identity AKM claims.

### Deductions

| Area | Points | Severity and evidence | Scenario and closing test |
|---|---:|---|---|
| A | -1.0 | **Medium, inspection.** `docs/FIRST_USE.md:18` promises that a clipboard copy "is cleared" after a short time and on lock. `README.md:87` correctly says cleanup is attempted and the copy can remain. | A busy clipboard or an unexpected stop can leave the copy behind. Make the first-use guidance state the same best-effort limit, and check all user-facing clipboard claims against the failure cases. I did not touch the clipboard. |
| A | -0.5 | **Low, inspection.** The unconditional unlock-screen promise in `desktop/src/components/Unlock.tsx:122` describes "KDBX 4.1 with AES-256 and Argon2id," including when opening an existing vault. | Existing KDBX 3.x/4.0 vaults can open, and an upgrade preserves their existing cipher and KDF settings. Qualify the text as applying to newly created vaults; check the copy in both create and unlock modes. |
| B | -2.5 | **Medium, reproduced with fake data.** `crates/akm-core/src/journal.rs:429` falls back to the shared KDBX root ID for an unseen version at an already bound path. | I copied vault A to B, modified B under a separate fake journal root, replaced A's file with B, and revealed B's key through A's path. The request was appended to A's journal. This contradicts the unconditional separate-copy claim in `README.md:168`, but I count the underlying identity decision once, here. The journal is explicitly unauthenticated; I did not establish secret exposure or vault loss. Decide how the product should represent an **unconfirmed lineage** when an external edit and an unseen clone are indistinguishable. Test both cases through the CLI and Activity view. |
| B | -2.0 | **Medium, inspection.** The desktop's registered commands at `desktop/src-tauri/src/commands.rs:29` have no program-launch operation; `desktop/src/components/UseKeys.tsx:265` offers `.env` import and export. | A user who wants scoped, file-free environment injection must use the CLI at this commit. A GUI-only user can copy or save plaintext instead. Provide and verify a desktop launch flow if the owner's no-command-line objective includes this core workflow. PR 10 was expressly outside this review. |
| B | -1.5 | **Medium, CLI behavior reproduced; recovery UI inspected.** A damaged journal refuses appends in `crates/akm-core/src/audit.rs:379`, and the Activity view at `desktop/src/components/Activity.tsx:117` reports damage but offers no recovery action. | After I damaged the fake journal, revealing a key exited 1 and audit verification exited 2. Refusing disclosure is the stated fail-closed policy, but a GUI-only user has no in-app route to preserve the damaged evidence and resume use. Design an explicit recovery flow that retains the old bytes, identifies the history gap, and requires a deliberate user choice; test it through the native GUI. |

### What earned the points

The positioning is unusually candid about the experimental community KDBX writer, lack of a professional audit, and cloud synchronization limits. The source also separates the core, CLI and native desktop responsibilities. By inspection, vault mutations use candidate snapshots, reopen and compare the encrypted candidate, and check the source fingerprint before replacement. The native session carries a ticket, revision and presentation identity, and `Held` ties selected effects to the session gate.

The fake CLI probe used the supplied executable after its SHA-256 matched the brief. It established the two specific journal outcomes above. It did **not** independently establish the implementer's reported full-suite, interchange, GUI or no-network results. The static network restrictions and threat model are useful, but they do not prove that every runtime component makes no connection.

### Notes for other areas

The journal identity result can also make Activity attribute an operation to the wrong history. I have deducted for the identity design in B and have taken no additional points for that effect.

The vault write lock is derived from the path spelling in `crates/akm-core/src/vault.rs:1354`. Two aliases for one file can therefore use different sidecar locks; this is already listed as P2R2-07. A resulting lost-write race belongs with data integrity and was **not reproduced or scored** here.

### Top actions to reach above 90

1. Resolve the journal identity premise and state its uncertainty honestly. Exercise unseen clones, external edits, moves and replacements through the real callers.
2. Complete the GUI's scoped launch workflow and verify that a user can use keys without a command line or plaintext `.env` file.
3. Add a deliberate, evidence-preserving journal recovery path without weakening the pre-disclosure failure policy.
4. Correct the clipboard and vault-format claims and check the remaining user-facing security promises against their actual limits.

### Not covered

I did not launch the GUI or installer, use the clipboard, alter app settings or the OS session, run a full build or suite, or use real secrets. I did not independently verify the reported KeePassXC or kdbxweb proofs, native event delivery, real clipboard behavior, or GUI recovery behavior. This is not a score for the other seven rubric areas.

---

## Reviewer 3: data integrity and correctness (E and F)

**Review scores: E 12/15 and F 6/10.** These are scores for the two assigned areas only, not an overall release score. I applied the project's real-secret bar: an independent full review must score above 90/100, followed by the owner's testing.

| Area | Maximum | Deductions | Score |
|---|---:|---:|---:|
| E. Data integrity and durability | 15 | 3 | **12** |
| F. Functional correctness | 10 | 4 | **6** |

The checkout was detached at `c0a7d3bedc7306df260cc4cd91dbcbfd4c2b7ef8`. The prebuilt CLI matched the supplied SHA-256, `701f34f2…f7197bce`. The hash identifies the tested binary; it does not independently establish how it was built.

### Deductions

| Area | Points | Severity | Finding and evidence | Closing test |
|---|---:|---|---|---|
| E | -3 | High | **Backup pruning deletes files without proving ownership.** `crates/akm-core/src/vault.rs:1072` recognizes files by name and removes them at `:1092`. I placed six independently created sentinel files beside a fake vault, using names that match the dated-backup pattern. One successful `add` deleted three of them. **Reproduced.** | Put foreign sentinel files and genuine AKM backups in the same directory. Save repeatedly and assert that retention removes only files whose ownership AKM can establish. |
| F | -2 | Medium | **An accepted rotation interval can make reads panic.** `crates/akm-core/src/model.rs:206` accepts `4294967295` days, but `crates/akm-core/src/schedule.rs:68` adds it without checking date overflow. `add` succeeded; ordinary `list`, `get`, and `get --field secret` panicked with exit code 101. `list --json` and export remained usable. **Reproduced.** | Test the largest accepted interval and an out-of-range interval through the CLI and desktop mapping. Invalid stored values must produce a controlled result, not a panic. |
| F | -1 | Medium | **Add and rotate accept a NUL-containing secret that AKM cannot inject or export.** `crates/akm-core/src/model.rs:182` checks only for an empty secret, while `crates/akm-core/src/model.rs:386` rejects NUL at use time. Both add and rotate reported success with fake NUL-containing stdin; `export --all` then failed. KeePassXC showed the NUL in its direct read, but its XML export omitted it. **Reproduced.** | Feed NUL through add, rotate, and other mutation callers. Assert rejection before a save, an unchanged vault, and a usable prior secret. |
| F | -0.5 | Low | **Unicode title matching contradicts the CLI's "any letter case" promise.** `crates/akm-core/src/vault.rs:262` compares ASCII case only; the recycled-key selector does the same. Adding `Éclair2` succeeded, but `get éclair2` failed while the exact-case query succeeded. **Reproduced.** | Exercise non-ASCII case pairs and ambiguous titles through live and recycled selectors, with behavior matching the documented contract. |
| F | -0.5 | Low | **Fractional expiry seconds are silently discarded.** `crates/akm-core/src/vault.rs:1584` truncates the instant, although the design says a full RFC 3339 instant is kept exactly. An input of `2030-01-02T05:30:45.900+05:30` was read back as `2030-01-02T00:00:45+00:00`. **Reproduced.** | Round-trip fractional RFC 3339 values and check the expiry boundary. Preserve the instant or reject unsupported precision explicitly. |

### What earned the points

- A stale CLI writer paused before receiving its secret, while a second writer saved. The stale writer refused to overwrite the changed vault, and the second writer's key remained. Four simultaneous writers produced one success and three lock refusals in the attempted race.
- On a fake vault, BOM-prefixed stdin stored the intended secret, an empty secret and repeated rotation were refused, and rotation, history, Recycle Bin, restore, purge, and audit verification worked for the exercised cases.
- `akm run` propagated a child's exit code 37. Direct executable and `.cmd` probes preserved the tested arguments, including spaces and `&`; the direct child received the selected fake key and did not receive `AKM_PASSWORD`.
- A read-only fake vault rejected a write and retained its hash. KeePassXC 2.8.0 changed another fake vault to 4.0; AKM refused a write until explicit upgrade, retained a byte-identical pre-upgrade backup, and preserved the tested foreign entry and notes after upgrade.

These points reflect the cases I observed. I used the **Prove It Works** principle to read back stored results and invoke the prebuilt CLI and KeePassXC directly.

### Notes for other areas

No separate root cause outside E or F is reported from this review.

### Top actions to reach above 90

1. Make backup retention establish ownership before deleting anything, and add the foreign-sentinel regression.
2. Bound or safely handle rotation dates at both input and stored-data boundaries.
3. Reject unusable secret values before mutation, then resolve the documented Unicode matching and expiry-precision behavior.

### Not covered

I did not test a crash or injected failure at each save stage, clone and merge behavior, import overwrite, KDBX 3.x, every command and flag, controlled clock rollback, reminders, long names, or kdbxweb. I did not launch the GUI or test the clipboard, OS session, or real secrets. The KeePassXC checks above were focused probes, not the supplied full interchange suite. The brief prohibited full builds and full test suites.

---

## Reviewer 4: desktop quality (G)

### Score

| Area | Maximum | Deductions | Score |
|---|---:|---:|---:|
| G. Desktop app quality and user experience | 10 | 5.00 | **5.00 / 10** |

The bar is a dependable, understandable desktop key manager that a new user can operate without a command line, including at the supported 900 px width and with assistive technology. This is an area score, not a total project score or a release verdict. The project's real-secret gate requires an independent total score above 90/100.

The review covered source at `c0a7d3bedc7306df260cc4cd91dbcbfd4c2b7ef8`. Browser probes used the supplied built frontend with a mocked native boundary. I did not independently establish that the supplied frontend build was produced from that commit.

### Deductions

| Points | Severity | Finding and evidence |
|---:|---|---|
| 1.25 | Medium | **UI-R2-01 remains open.** At 900 px, two recycled keys with the same title and long project names differing at the end have clipped project, environment and history text. Their permanent-delete buttons have identical accessible names, and the confirmation names only the shared title. A user cannot reliably identify the target before confirming. **Reproduced** without deleting either key. See `desktop/src/components/RecycleBin.tsx:110` and `desktop/src/styles.css:484`. |
| 0.75 | Medium | Key-list buttons use only the title as their accessible name; their description contains expiry and rotation status but not project, environment or variable. In the Edge accessibility tree, same-title keys in different projects had identical names and descriptions. A screen-reader user must open entries individually to distinguish them. **Reproduced with the accessibility tree**, not a real screen reader. See `desktop/src/components/KeysView.tsx:195`. |
| 1.00 | Medium | **Launching a program with scoped keys is CLI-only.** The GUI offers plaintext `.env` export, preview and copy, while `akm run` injects the scope into a child program. A person who cannot use the command line cannot use that workflow from the app. **Inspection.** See `desktop/src/components/UseKeys.tsx:263` and `crates/akm-cli/src/main.rs:217`. |
| 0.50 | Medium | **Detailed key history is CLI-only.** The drawer reports a count of earlier versions, but does not show their times or changed fields. The CLI provides that history. **Inspection.** See `desktop/src/components/KeyDetail.tsx:189` and `crates/akm-cli/src/main.rs:174`. |
| 0.50 | Low | A failed initial read displays an error **and** an empty-state claim. The Keys view says "No keys yet," and the Recycle Bin says it is empty, although neither read succeeded. **Reproduced** with fake native read failures. See `desktop/src/components/KeysView.tsx:344` and `desktop/src/components/RecycleBin.tsx:96`. |
| 0.25 | Low | **UI-R2-04 remains open.** Long variable names are ellipsized in the list. Two names differing at the end require opening their drawers to distinguish them. **Reproduced** at 900 px. See `desktop/src/styles.css:486`. |
| 0.25 | Low | **UI-R2-03 remains open.** When every visible result is unfiled, the list suppresses its "No project" group heading and the rows have no project field. **Reproduced.** See `desktop/src/components/KeysView.tsx:140`. |
| 0.25 | Low | **UI-R2-05 remains open.** The compact list omits the earlier project total and a visible marker for an unset expiry. The accessible status description still says no expiry date. **Inspection.** See `desktop/src/components/KeysView.tsx:214`. |
| 0.25 | Low | **UI-R2-02 remains open.** Settings prefers its first successful list count over later polled status counts. A mocked status change from one to two left Settings showing one. The corresponding timing through the native app was not reproduced. See `desktop/src/components/Settings.tsx:45`. |

### What earned the points

- A mocked first-use journey followed the guide's create-vault and add-key steps through to revealing a fake secret. This proves the frontend sequence only; the mock did not create a real vault.
- The custom `FilterSelect` opened for a mouse press, kept focus on the field, and accepted a keyboard selection. A synthetic press with zero click count was not intercepted. That last check does **not** establish real UI Automation or screen-reader operation.
- At 900 px, a nonempty Activity detail remained visible. The Add drawer and permanent-delete confirmation covered the rail in sampled hit tests, and focus returned to Add key after Escape.
- Long project headings kept Add key in view at 900, 1040, 1180 and 1440 px in the tested fixture. A 120-project shelf remained bounded and left search and the first result near the top.

For the **LOOK-R1** rows, I found the following at this commit:

| Row | Review result |
|---|---|
| 01 | Sampled Add and permanent-delete overlays covered the rail at 900 px. Other dialogs and widths were not independently exhausted. |
| 02 | Sampled forced-colours checks showed a list boundary, active option and system checkbox. Real Windows high-contrast themes remain untested. |
| 03 | The tested long heading and drawer content produced no horizontal overflow at the four widths above. |
| 04 | The getting-started guide now says fake keys only. **Inspection.** |
| 05 | After a synthetic text-size change, the marker reached the current item in both themes. |
| 06 | The 120-project fixture kept the shelf bounded. |
| 07 | The tested destructive button retained its danger colour at rest and hover in both themes. |
| 08 | Sampled drawer backgrounds computed as opaque in both themes. I did not independently check every animation frame. |

These checks support the named fixes within their tested scenarios; they do not constitute formal acceptance of every LOOK-R1 regression test.

### Notes for other areas

No additional root cause is assigned to another area in this report. The permanent-delete finding is scored here for the missing identifying information; no deletion or data loss was performed or demonstrated.

### Top actions to reach the project's target

1. Show complete distinguishing details in Recycle Bin rows and in the permanent-delete confirmation. Test duplicate titles and long common prefixes at 900 px, including accessible names.
2. Give key-list controls distinct accessible names or descriptions containing project and environment, then test with a real Windows screen reader.
3. Add GUI workflows for scoped program launch and detailed history, with clear explanations of their limits.
4. Separate failed reads from empty results, and resolve the remaining compact-list and stale-count findings.
5. Run the native and accessibility checks that a mocked browser cannot establish.

### Not covered

I did not launch the native desktop app or installer, use the clipboard, alter saved app settings or the OS session, or run full builds or suites. Real Windows high contrast, a real screen reader, native `FilterSelect` automation, installation, native import confirmation, Windows notifications, lock and suspend behavior, and the complete native first-use journey remain unverified. Busy and error states were sampled, not exhaustively exercised.

---

## Reviewer 5: verification and shareability (H and I)

### Score table

Reviewed commit `c0a7d3bedc7306df260cc4cd91dbcbfd4c2b7ef8`.

| Area | Maximum | Deductions | Score |
|---|---:|---:|---:|
| H. Verification and engineering hygiene | 5 | 3.25 | **1.75** |
| I. Shareability | 5 | 3.50 | **1.50** |

I applied a release-readiness bar for a Windows tool intended to hold API keys and protect vault data. The project's bar for real secrets is an independent full review above **90/100**, followed by owner testing. These two scores are contributions to that review, not a total product score.

### Deductions

#### H. Verification and engineering hygiene

| Deduction | Severity | Location | Scenario and evidence |
|---:|---|---|---|
| 0.75 | Medium | `docs/TESTING.md:103` | The focused real-window runs do not cover the real clipboard, native import dialog, lock/suspend, idle wait, or R4-04. Browser tests use a handwritten native mock; the Rust dispatch test uses production handlers but a mock runtime. A mismatch across the complete frontend, native and Windows path can escape both. **Inspection and retained-log review.** |
| 0.50 | Medium | `docs/STATE.md:8` | Phase-two redesigns after two failed reviews remain "fixed, not accepted." Their tests and mutant reports are recorded, but the redesigns have not received the independent acceptance the project requires. **Inspection.** |
| 0.75 | Medium | `crates/akm-core/tests/vault.rs:172` | The cross-scope import and failed-rename tests compare `ApiKey` metadata, which excludes secrets. An unrelated protected value could change while the compared metadata remains equal. The rename test could also miss that changed value being persisted by a later save. **Inspection.** |
| 0.25 | Low | `tools/verify_kdbxweb/verify.mjs:158` | The interoperability check describes foreign data as surviving "byte for byte," but compares selected semantic fields, timestamps, history and attachments. It does not compare every database or entry property. A change to an omitted property could pass. **Inspection.** |
| 0.25 | Low | `tools/check_docs.py:50` | `splitlines()` removes vertical tab and form feed before the control-character regex sees them. A reference to a non-test Rust function also satisfies the name check. **Reproduced in memory:** both control characters and the non-test function produced no findings; a control-character positive case was detected. |
| 0.50 | Medium | `.github/workflows/build.yml:39` | CI runs substantial checks, but omits `check_dependencies.ps1` despite the roadmap calling for dependency checks in CI. The supplied brief reports that CI did not start on this head; GitHub returned 404 to both available read paths, so I could not independently establish its current status. **Inspection; head status unverified.** |
| 0.25 | Low | `.github/workflows/build.yml:14` | Lockfiles and a pinned Rust version help, but the runner image, Node patch version and action references can move. A checksum of one build does not prove an independent rebuild produces the same artifact. **Inspection; reproducibility not tested.** |

#### I. Shareability

| Deduction | Severity | Location | Scenario and evidence |
|---:|---|---|---|
| 0.75 | Medium | `README.md:200` | A recipient cannot authenticate the publisher of the supplied installer. Its Authenticode status is `NotSigned`. The README correctly calls it an unsigned development build. **Reproduced by signature inspection.** |
| 0.25 | Low | `README.md:220` | There is no updater or established authenticated manual release channel. An offline product can use signed manual releases, but recipients still need a reliable way to obtain security fixes. **Inspection.** |
| 0.75 | Medium | `Cargo.toml:9` | The Rust manifest declares MIT, but the tracked tree has no project license text or third-party notice inventory. A recipient lacks a complete, reviewed licensing package. **Inspection.** |
| 0.50 | Medium | `docs/ROADMAP.md:140` | There is no `SECURITY.md` or tracked vulnerability-reporting route. A recipient who discovers a vulnerability has no documented private disclosure process. **Inspection.** |
| 0.50 | Medium | `docs/GETTING_STARTED.md:13` | Install, upgrade and uninstall instructions have not been validated on a clean computer. The per-user NSIS configuration is present, but it is not evidence that those flows work as described. **Inspection; execution prohibited by the brief.** |
| 0.50 | Medium | `docs/STATE.md:9,54,62-63`; `b187e55:docs/history/phase1-final/evidence.json:5`; `b187e55:docs/work/phase2-1a/corrections3/metrics-report.json:3-8`; `b187e55:docs/work/phase2-1a/reviews/round4/provenance-result.json:279-330` | Machine-specific paths and retained historical evidence occur in the tracked tree and earlier Git history. Other historical evidence appears under `b187e55:docs/work/phase2-1a/` and `b187e55:docs/history/`. Later deletion does not remove it from history. **Inspection.** |
| 0.25 | Low | `tools/fetch_keepassxc.ps1:26`; `.agents/skills/verify-akm/scripts/akm-verify.ps1:21` | The documented defaults depend on a machine-specific drive and archive location. A recipient without that drive must discover and override the defaults to use these verification tools. **Inspection.** |

### Five tests that matter most

These are failure modes of the **tests**, not claims that the current product exhibits the hypothetical defect.

1. **Cross-scope import isolation.** The test at `crates/akm-core/tests/vault.rs:172` checks the original entry's metadata and count. It can pass if the original entry's secret alone changes. Read the protected values of both scopes after import and after reopening the vault.

2. **Rollback after a failed rename.** The Windows test at `crates/akm-core/tests/vault.rs:588` checks disk bytes before retry and metadata after a later save. It can pass if a failed mutation changes only the in-memory secret and the later save persists it. Assert the original secret in memory and after reopening following the later save.

3. **Scoped child-process injection and password scrubbing.** The child test at `crates/akm-cli/tests/security.rs:330` checks mixed-case `AKM_*` removal and the intended value, but its fixture contains only one vault key. It can pass if the `run` path also injects another project's key. Use at least two scopes and assert the child's complete relevant environment. The separate export test covers two scopes, but does not establish this child-process case.

4. **Copy followed by lock.** The IPC test at `desktop/src-tauri/tests/ipc_dispatch.rs:1066` exercises generated production handlers and meaningful delayed-copy barriers, but uses a fake Win32 clipboard API. It can pass if Windows clipboard formats, ownership, or cleanup differ from the fake. The opt-in real clipboard test at `desktop/src-tauri/src/clipboard.rs:894` and full owner-present GUI proof remain necessary.

5. **Independent KDBX interoperability.** The kdbxweb test at `tools/verify_kdbxweb/verify.mjs:431` checks real secret values, history and attachment bytes through another implementation. It can still pass if AKM drops a foreign property absent from both the fixture and snapshot. Add populated examples and comparisons for the properties the preservation claim covers, or narrow that claim.

### What earned the points

- The retained focused GUI logs identify the desktop executable hash and record **27** journal and **9** journal-export PASS lines. They explicitly report skipped flows. I independently matched all three supplied artifact hashes and confirmed the installer is unsigned. The logs establish what was recorded, not an independently repeated run.
- The IPC harness invokes the same `production_handlers!` macro as the app and includes barrier-controlled import and clipboard scenarios. The browser boundary is separately and explicitly mocked.
- The CI definition includes Rust and frontend tests, builds, a checksum-pinned KeePassXC download, two interoperability checks, and a no-network check. Lockfiles and a pinned Rust toolchain version support repeatable dependency selection.
- Review records retain failed verdicts and identify unaccepted redesigns. `check_docs.py` passed on the checked-out tree, within the checker limits described above.

### Notes for other areas

- **Data integrity, High by inspection.** `.agents/skills/verify-akm/scripts/akm-verify.ps1:76` treats a temp directory with the expected name prefix and a readable `session.json` as owned, then recursively removes it. A user-created or unrelated directory meeting those conditions can be deleted. No deletion was attempted. A closing test should put a sentinel in an unowned matching directory and prove cleanup refuses it.
- The incomplete real-window and clipboard proof is a coverage finding here. I did not infer an actual secret disclosure or vault corruption from that absence.

### Top actions to reach the project's bar

1. Close the owner-present native proof gaps, including clipboard, import, R4-04, OS lock/suspend and the idle wait, against a build bound to the reviewed revision.
2. Strengthen the five tests above, starting with secret-value assertions after failed and cross-scope mutations. Independently review the phase-two redesigns.
3. Add dependency audits to CI, obtain a successful run on the exact head, and document or establish the intended reproducible-build standard.
4. Prepare a release package with publisher authentication, a secure update or manual-release process, license and third-party notices, a disclosure route, and clean-machine installer tests.
5. Review the identified tracked and historical private material before sharing history. Complete a broader secret and personal-data review before any public push.

### Not covered

I did not run full suites or builds, launch the GUI or installer, access the clipboard or app settings, or test real Windows session changes. I did not independently rerun the reported KeePassXC or kdbxweb proofs. GitHub returned 404 for the queried commit, so the reported billing limit and CI result remain unverified.

I read the patch history reachable from the reviewed commit and the two named archive/snapshot tags, and ran a limited credential-pattern scan over additions. It found no matches for those patterns. That is **not** a complete secret scan or a clearance to publish. Binary contents and every possible credential format were not exhaustively assessed.
