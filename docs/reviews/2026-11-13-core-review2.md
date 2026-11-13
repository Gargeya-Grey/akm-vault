# Core and CLI (campaign unit 3), review 2

- Model: Codex GPT-6.1 Sol, effort max (the campaign's review model; see REVIEWING.md).
- Reviewed commit: `0ddaef5` on `gate/integration`. Fix round 1 is `8366f31..0ddaef5` on `crates`, `Cargo.toml` and `Cargo.lock`. The whole unit is the code for FR-01, FR-02, FR-05, FR-08, FR-12, FR-14 (core), FR-15 to FR-18, FR-22, FR-28 and P2R2-07 at that commit.
- Build hashes: `akm.exe` built from `e33c396`, SHA-256 `9c7ea7f40e66ba11cf0c48ce89f5dd8f5230b25500e05a56aa75344b035b04ef`. The reviewer checked the hash, and checked that the core and CLI diff from `e33c396` to the reviewed commit is empty.
- Date: 2026-11-13.
- Result under the pass rule: **fail** (6 P2). Second review of this unit in the campaign, and its second failure.

The text below is the reviewer's, except that links to the reviewer's local checkout are written as repository paths, and the reviewer's closing notes about its probe folder and its cleanup are removed.

---

Reviewed commit **`0ddaef51332b3c756d4b1a07c6136007b45f1af8`**. HEAD matched throughout; the checkout remained clean.

Model and effort: **GPT-6.1 Sol, max**.

Checked the supplied executable's SHA-256 before use and again afterward:

`9c7ea7f40e66ba11cf0c48ce89f5dd8f5230b25500e05a56aa75344b035b04ef`

The core/CLI/Cargo diff from its build revision `e33c396` to HEAD is empty.

**1. Ranked findings**

**CORE-R2-01 — P2 — The backup ledger's deletion authority transfers to another folder. Reproduced through the supplied CLI.**
`crates/akm-core/src/backups.rs:143` accepts a ledger entry using its basename and matching contents, without binding it to the file or folder where AKM created it.

I copied a vault into another folder under the same basename, then independently copied one listed backup into that folder. After five CLI saves, AKM deleted that independently created copy. The source backup survived. The encrypted record establishes which backup contents AKM previously wrote, but does not establish ownership of this filesystem object.

**Closing test:** transplant a vault's ledger into another folder containing an independently created, byte-identical file under a listed name. Also replace a listed backup with a distinct, byte-identical file. Both foreign objects must survive repeated pruning while genuine backups retain their intended policy.

**CORE-R2-02 — P2 — The credential class includes file locations that bypass the intended import distinction. Reproduced.**
`crates/akm-core/src/model.rs:1248` treats any matching whole credential word as sufficient once the reserved rules miss.

`GOOGLE_APPLICATION_CREDENTIALS` is explicitly expected to be `Credential` by the new tests. The core import preview accepted it with that class. The supplied CLI accepted and exported a fake JSON filename, and `akm run` passed it to the installed Google authentication library. That library loaded the fake credentials from the named file, without network access.

This is a demonstrated file-selection effect, not demonstrated code execution or secret disclosure; hence P2. The finite-table disclosure does not resolve it: even the planned desktop marking of **Other** names leaves this **Credential** name unmarked. `AWS_SHARED_CREDENTIALS_FILE` and several AWS credential locator names receive the same classification by inspection.

**Closing test:** exercise documented credential locators through classification, preview, storage and launch/export. A file location or endpoint must not receive the unmarked credential treatment. Include an offline consumer proving which file is selected.

**CORE-R2-03 — P2 — Making an unseen snapshot stale falsely confirms its journal lineage. Reproduced, including disclosure.**
`crates/akm-core/src/journal.rs:531` still lets a shared root UUID vouch for stale evidence whose fingerprint the journal never recorded.

I replaced a bound vault with an externally rewritten clone and opened it. Its snapshot correctly reported `Unconfirmed` with a notice. I then rewrote the file again, making that opened snapshot stale. The same snapshot now reported `Confirmed`, with no notice. `disclose_key` returned the unseen clone's fake secret without a lineage warning.

Clones sharing a root UUID therefore still cross the identity boundary through the stale-reader path.

**Closing test:** open an unseen clone at a bound path, change the file again before Activity or disclosure, and require an unconfirmed result or safe refusal. Preserve the supported behavior of stale snapshots whose lineage actually was recorded.

**CORE-R2-04 — P2 — Public journal append silently consumes the lineage warning. Reproduced.**
`crates/akm-core/src/audit.rs:688` discards `append_reporting`'s boolean, although that implementation passes `carries_lineage = true` to `journal::record`.

Starting with an unseen external revision, I observed an unconfirmed snapshot, called `vault.audit_log().append(...)`, and observed a confirmed snapshot afterward. A subsequent `Vault::log` produced no lineage warning. No returned result or journal notice conveyed the discarded warning.

The new "settle only when reported" rule covers recovery and placement, but not this public wrapper.

**Closing test:** append through the public `AuditLog::append` API under an unseen revision. It must either report the lineage condition or leave it unsettled so the next reporting operation still warns.

**CORE-R2-05 — P2 — Hard-link aliases still bypass cooperating-writer exclusion. Reproduced through the supplied CLI.**
`crates/akm-core/src/vault.rs:1403` derives the lock from a canonical pathname. Canonicalization does not unify distinct hard-link names for one file.

With `vault.kdbx` and `alias.kdbx` hard-linked, I held `vault.kdbx.lock`. A CLI mutation through `vault.kdbx` correctly refused with "lock file is held"; a mutation through `alias.kdbx` succeeded. Replacement then left the two paths with different vault contents.

This breaks contract 3's exclusion for cooperating writers reaching the same file through aliases.

**Closing test:** hold the lock through either hard-link name and require writes through both names to refuse. Verify that concurrent attempts cannot silently fork their initial shared vault.

**CORE-R2-06 — P2 — The new KDF guard performs quadratic work on unauthenticated headers. Reproduced; scaling established by inspection.**
`crates/akm-core/src/kdf_guard.rs:302` scans all prior dictionary entries with `retain` for every new entry. There is no entry-count bound.

A **1,020,183-byte** header containing 60,000 distinct unused entries exceeded a three-second deadline in all five bounded release-CLI probes, consuming approximately two seconds of process CPU before each termination. Every child was killed and reaped. No expensive KDF was requested or executed. The loop entails roughly 1.8 billion prior-key comparisons for that input and grows quadratically.

The numeric-width fix holds, but its replacement parser introduces another resource-exhaustion path before authentication.

**Closing test:** bound dictionary parsing work and add a many-entry header regression that produces a controlled result within that bound. Retain the existing width, duplicate-key and normal-vault regressions.

**2. Source acceptance: NO — fail**

The six P2 findings block acceptance under the pass rule. "Verified" below describes the checked fix and regression at HEAD; it does not formally close a finding while this unit fails review.

| Earlier finding | Result | Reason |
|---|---|---|
| CORE-R1-01 | **Not fixed** | Unlisted foreign files survive, but copied ledger authority still deletes a foreign object; R2-01. |
| CORE-R1-02 | **Not fixed** | `HOST_CC`/`TARGET_CC` are refused; the replacement credential-class design still admits unmarked locations; R2-02. |
| CORE-R1-03 | **Verified** | Exact-width, malformed-field and differential KDF tests passed. R2-06 is a separate parser-resource defect. |
| CORE-R1-04 | **Verified** | Failed append preserves mapping bytes and the warning for retry, including reopening between attempts. Other reporting paths remain defective. |
| CORE-R1-05 | **Not fixed** | Ordinary unseen reads warn, but stale unseen reads falsely confirm; R2-03. |
| CORE-R1-06 | **Verified** | Checked/saturating arithmetic and extreme CLI/core interval regressions passed. An additional near-maximum CLI interval returned normally. |
| CORE-R1-07 | **Verified, core/CLI** | Typed subsecond inputs are refused; CLI text checks reject nonzero fractions beyond nine digits. |
| CORE-R1-08 | **Verified** | Non-replacing recovery moves and destination-occupation regressions passed, including installation failure. |
| CORE-R1-09 | **Not checked fully** | Resolver regression passed; actual file-symlink acquisition skipped with Windows error 1314. |
| CORE-R1-10 | **Verified** | The documented common folding rule passed live/recycled selection, ambiguity and additional case-pair tests. |
| CORE-R1-11 | **Verified** | Import coverage now independently distinguishes project and environment and checks all secrets after reopening. |
| FR-01 | **Not fixed** | Ownership remains transferable by copied ledger and contents; R2-01. |
| FR-02 | **Not fixed** | Known compiler controls are blocked, but the credential classification fails its intended distinction; R2-02. |
| FR-05 | **Verified** | In a newly shared fake folder, CLI save produced protected vault, dated-backup and `.bak` DACLs with one allow entry and no Everyone grant. Fallback tests passed. |
| FR-08 | **Verified, stated scope** | Zeroizing secret-path guards passed; stdin storage is bounded and zeroizing. This does not establish complete memory erasure. |
| FR-12 | **Not fixed** | Stale unseen reads and public append still bypass truthful lineage reporting; R2-03/04. |
| FR-14, core | **Verified** | Recovery preserves damaged bytes and refuses occupied publication destinations. |
| FR-15 | **Verified** | Rotation validation, stored intervals, expiry filtering and retention cutoffs have controlled results. |
| FR-16 | **Verified** | Add, rotate, edit and import reject new NUL-bearing values; regressions verify unchanged bytes and preserved secrets. |
| FR-17 | **Verified** | Core selection/search and CLI ordering use the documented shared rule. |
| FR-18 | **Verified, core/CLI** | Representable subsecond inputs and arbitrarily long CLI fractions are checked before mutation. |
| FR-22 | **Verified** | Import scope, failed-save secret preservation and three-scope child-environment regressions passed. |
| FR-28 | **Verified for KDF costs** | Numeric bounds and fail-closed width parsing hold. The new header-parser availability defect is R2-06. |
| P2R2-07 | **Not fixed** | Hard-link aliases obtain separate locks; R2-05. |
| CORE-C-01 | **Not fixed** | User-facing backup descriptions still omit accumulation of unlisted/crash/renamed-vault backups. |
| CORE-C-02 | **Not checked** | Native file-symlink lock/delete branches could not run without privilege. |
| CORE-C-03 | **Not checked** | Desktop-unit fixture defect; supplied collector result remains a failure. |
| CORE-C-04 | **Not checked** | Desktop-unit timing tests were not rerun. |

Add, edit/rename, import preview/import and use-time planning route name decisions through the shared rule. Restore moves an existing entry without reclassifying it; subsequent launch/export planning still checks its name. The CLI has no import command.

**3. Work unit complete: NO**

Source acceptance fails. Native file-symlink lock and deletion proof also remains missing.

Twenty-two targeted Cargo invocations completed successfully, reporting **83 passed tests**. Two native symlink branches printed `NOT RUN` despite their enclosing tests passing. Additional fake-data library probes and supplied-release CLI probes reproduced the findings above.

No tracked file, Git record, clipboard, desktop setting or OS session was changed. The desktop settings hash remained unchanged, and the real journal folder remained absent.

**4. What the review did not cover**

- No whole-workspace suite, release build, GUI run, clipboard/session test, real secret or network access.
- No repeated interchange suites. Their supplied results were considered alongside the verified empty build-to-HEAD core/CLI diff.
- No execution against earlier revisions or mutants; regression adequacy was inspected and selected tests ran at HEAD.
- Desktop implementation was inspected only as a caller. Its outstanding import marking remains outside this unit. Desktop raw expiry deserialization beyond nanosecond precision also requires its own boundary proof.
- The header probes establish bounded timeouts and CPU consumption, not the full processing time of the large input.
