# Core and CLI fixes, review 1

- Model: Codex GPT-6.1 Sol, effort max. REVIEWING.md named GPT-6.1 Terra at that date. The owner asked on 2026-11-13 for a comparison of the two models on this unit. Terra started twice on this commit and the provider's safeguard stopped both runs before a result, so this is the only completed review.
- Reviewed commit: `8366f31` on `gate/integration` (campaign unit 3: the fixes for FR-01, FR-02, FR-05, FR-08, FR-12, FR-14 core, FR-15 to FR-18, FR-22, FR-28 and P2R2-07).
- Build hashes: `akm.exe` built from `955e3c5`, SHA-256 `c8efee06d4111aef5aed9a26aabd4c4eae4ac98d0a007c6fd10614cca6398469`. The reviewer checked the hash, and checked that the core and CLI diff from `955e3c5` to the reviewed commit is empty.
- Date: 2026-11-13.
- Result under the pass rule: **fail** (2 P1, 7 P2, 2 P3). First review of this unit.

The text below is the reviewer's, except that links to the reviewer's local checkout are written as repository paths, and the reviewer's closing notes about its temporary folders and its cleanup are removed.

---

Reviewed `8366f3148d05bd53e13a72741349edc1dfb44aa3`; HEAD matched and the checkout remained clean. **Source acceptance: NO. Work unit complete: NO.**

**1. Ranked findings**

1. **P1 — Backup hashes still do not establish ownership. Reproduced.**
   `crates/akm-core/src/vault.rs:1146` accepts any regular file whose bytes match the hash in its filename. I independently created such a foreign sentinel. Five successful saves deleted it. The new regression passes because its foreign files have missing, malformed or mismatched hashes.
   **Closing test:** retain a foreign file with a correctly matching hash alongside genuine backups through repeated pruning. Ownership must come from evidence beyond the file's own name and contents.

2. **P1 — Compiler execution controls still bypass the deny list. Injection reproduced; subsequent execution inspected.**
   `crates/akm-core/src/model.rs:418` reserves `CC`, but permits `HOST_CC` and `TARGET_CC`. Add, export and `akm run` accepted them; my child received both. A Rust build using cc-rs can execute the selected compiler with the scope's other secrets. These overrides are documented by [cc-rs](https://docs.rs/cc/latest/cc/#external-configuration-via-environment-variables).
   **Closing test:** reject these controls through add, import, stored-value planning, export and launch. Include a harmless fake compiler that records whether it ran.

3. **P2 — Padded KDF numbers bypass the resource guard. Reproduced.**
   `crates/akm-core/src/kdf_guard.rs:135` requires exactly eight bytes for a `UInt64`. The pinned KeePass reader accepts a longer value and reads its leading eight bytes. AKM treats the guard's parse failure as permission to continue. An ordinary `2000000001` AES-round field was refused in 23 ms; adding one padding byte entered derivation and exceeded the one-second probe deadline. The child was killed and reaped. A padded one-round control reached the expected incorrect-key error.
   **Closing test:** exercise padded AES and Argon2 numeric fields through open and reload, proving refusal before derivation.

4. **P2 — A failed append permanently consumes the lineage warning. Reproduced.**
   `crates/akm-core/src/journal.rs:929` stores the newly observed fingerprint before the journal append succeeds. I replaced a bound vault with an unseen clone and temporarily prevented opening its journal. The disclosure failed, but the mapping changed. After releasing the file, retrying disclosed the secret with **no lineage warning**.
   **Closing test:** fail the append after resolving `Continued`, then retry. The first successful operation must still report unconfirmed lineage.

5. **P2 — Activity reads falsely confirm an unseen lineage. Inspected.**
   `crates/akm-core/src/journal.rs:977` maps `Resolution::Continued` to `Identity::Confirmed` without a notice. After an unseen replacement, an unlocked `AuditLog::snapshot` therefore presents the original journal as confirmed before any warning-producing append. Desktop `audit_read` uses this path.
   **Closing test:** replace a bound vault with an unseen clone, then read Activity before any disclosure or mutation. Require an unconfirmed identity and an explanatory notice.

6. **P2 — The unchecked-date-arithmetic class remains in other commands. Reproduced.**
   `crates/akm-core/src/vault.rs:925` still performs unchecked addition. `list --expiring 4294967295` and `list --expiring 9223372036854775807` exited 101 with overflow panics. With one recycled key, `purge --older-than-days 4294967295 --yes` also panicked at `crates/akm-cli/src/main.rs:645`. The core purge cutoff has the same unchecked subtraction.
   **Closing test:** cover extreme expiry-filter and purge intervals through both CLI and core, requiring controlled results and unchanged vault bytes.

7. **P2 — Expiry precision is checked only on one input path. Inspected; a second CLI bypass reproduced.**
   `crates/akm-core/src/model.rs:207` does not validate expiry precision. Desktop `KeyDraft` deserializes directly into `DateTime`, bypassing `parse_expiry`; core add/update therefore still accept `.900` and silently truncate it in `write_entry`. Separately, CLI input `2030-01-02T00:00:45.0000000001Z` succeeded and read back without its fraction: Chrono discarded that precision before the new nanosecond check.
   **Closing test:** test direct core add/update, desktop draft conversion, and RFC 3339 fractions beyond nine digits. Reject unsupported precision before mutation.

8. **P2 — Journal recovery can overwrite a file it did not create. Inspected.**
   `crates/akm-core/src/audit.rs:524` uses replacing `fs::rename` after `free_kept_path` merely checked that the destination was absent. If another writer creates that destination while recovery writes and syncs its pending file, recovery overwrites the foreign file. The sentinel regression covers only files present before the check.
   **Closing test:** place a barrier after destination selection, create a sentinel there, then resume. Preserve it and either select another name or refuse safely.

9. **P2 — File-symlink aliases still obtain different write locks. Inspected; runtime probe blocked by missing privilege.**
   `crates/akm-core/src/vault.rs:1436` keeps only the canonical file's basename and joins it to the spelling's parent. For `B\link.kdbx → A\vault.kdbx`, it chooses `B\vault.kdbx.lock`, while the target chooses `A\vault.kdbx.lock`. Cooperating writers bypass the promised exclusion. The new junction/dot/case test passes without exercising this case.
   **Closing test:** hold the lock through a file symlink and require acquisition through its target to fail, and vice versa. My symlink creation attempt returned WinError 1314.

10. **P3 — Lowercasing does not provide the promised Unicode case matching. Reproduced.**
    `crates/akm-core/src/vault.rs:287` still misses valid case pairs. A key titled Greek final sigma `ς` was added successfully, but selecting its uppercase `Σ` failed. `find_recycled` repeats the comparison.
    **Closing test:** cover `Σ`/`σ`/`ς` and ambiguous titles in live and recycled selection, using a documented, consistent caseless rule.

11. **P3 — The strengthened import-isolation test still cannot distinguish either scope dimension. Inspected.**
    `crates/akm-core/tests/vault.rs:172` changes both project and environment together. It can pass if import matching checks only project, or only environment. Its new secret assertions fix the reported metadata-only weakness but leave this coverage hole.
    **Closing test:** use the same variable in `demo/dev`, `other/dev` and `demo/prod`; import into one scope and compare every secret in memory and after reopening. Mutants removing either scope comparison must fail.

**2. Source acceptance: NO — fail**

The P1 and P2 findings block acceptance under the pass rule.

| Earlier finding | Conclusion |
|---|---|
| FR-01 | **Reproduced unresolved**, finding 1. |
| FR-02 | Reported Cargo names are reserved; **sibling execution controls remain**, finding 2. |
| FR-05, SEC-03 | **Original fix reproduced.** In a deliberately shared fake folder, vault and backups had protected DACLs granting only the current user. Fallback-warning tests passed. |
| FR-08 | **Inspected improvement; selected type guard passed.** Stdin buffers now use zeroizing storage. This does not establish complete memory erasure. |
| FR-12, P2R2-04 | Successful-warning test passed; **failure and read paths remain wrong**, findings 4–5. |
| FR-14, core | Recovery and injected single-step failure tests passed; **ownership race remains**, finding 8. |
| FR-15 | Reported rotation-input and stored-value failures are corrected; **arithmetic siblings remain**, finding 6. |
| FR-16 | **Selected regression passed** for add, rotate, edit and import, including unchanged bytes and preserved prior secrets. |
| FR-17 | Accented examples improve; **Unicode case promise remains incomplete**, finding 10. |
| FR-18 | Ordinary fractional CLI input is refused; **other input paths still lose precision**, finding 7. |
| FR-22 | Secret comparisons and the three-scope child-environment test passed; **import coverage gap remains**, finding 11. |
| FR-28 | Ordinary over-limit headers are refused; **guard bypass reproduced**, finding 3. |
| P2R2-07 | Junction/dot/case regression passed; **file-symlink handling remains incorrect by inspection**, finding 9. |

**3. Work unit complete: NO**

I read the complete 21-file diff and traced its production callers, including desktop callers. **Fourteen individually selected core/CLI tests passed.** The supplied executable's SHA-256 matched:

`c8efee06d4111aef5aed9a26aabd4c4eae4ac98d0a007c6fd10614cca6398469`

The core/CLI diff from its supplied build revision `955e3c5` to reviewed HEAD was empty.

**4. Not covered**

No whole suite, desktop build or GUI run; no clipboard, settings or OS-session changes; no real secrets; no new KeePassXC/kdbxweb interchange proof; no old-version or mutant execution. Large Argon2 allocations and actual malicious-compiler execution were not attempted. Recovery races and file-symlink behavior remain inspection findings. No project files or GitHub records were changed.

Model and effort: **GPT-6.1 Sol, max**, differing from `REVIEWING.md`'s required Terra/max routing.
