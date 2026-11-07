# Threat model

This document states which attackers AKM stops, which it makes harder, and which it does not help. It describes the code as it is on 2026-11-07. It extends the trust boundaries in [PROJECT.md](PROJECT.md#non-goals-and-trust-boundaries). It adds no promise that the code does not already keep.

AKM has had no professional audit, and the only measured score is 40.5/100 ([baseline review](reviews/2026-10-30-baseline.md)). Open defects are in [FINDINGS.md](FINDINGS.md). Do not use real secrets before the release gate in [AGENTS.md](../AGENTS.md).

## How to read this document

- **Stops**: with the stated limit, the attacker cannot get the asset through this path.
- **Makes harder**: AKM removes an easy path or a mistake. A determined attacker in the same position still succeeds.
- **Does not help**: AKM gives no protection against this attacker.
- **Backed by** names the [PROJECT.md contract](PROJECT.md#contracts-every-change-keeps) (C1 to C12) and a real test as `file :: test name`. A test shows that the code behaves as stated. It does not show that the attacker fails.
- **Evidence labels**: "read" means that this document's author read the source. "Test" means that a named automated test checks the behaviour. "Not checked" means neither.

## Assumptions

- Windows only. A trusted Windows account and OS ([PROJECT.md](PROJECT.md#non-goals-and-trust-boundaries)).
- The vault is a KDBX 4.1 file with AES-256, ChaCha20 inner protection and Argon2id (64 MiB, 3 iterations, parallelism 4). AKM did not design this. The `keepass` crate (0.17.0) that writes it labels its writer experimental.
- The master password has at least 12 Unicode characters for a new vault. That is a length rule, not a strength check. Older vaults with shorter passwords still open.
- The attacker does not have the master password, unless a row says otherwise.
- AKM has no second factor and no key file.

## Summary

| # | Attacker | Verdict | Contract | Real test |
|---|---|---|---|---|
| 1 | Stolen or copied vault file | Stops reading the secrets. Old backups travel with it; the journal does not (unit 1b). | C2 | `crates/akm-core/tests/vault.rs :: creates_vaults_with_a_hardened_kdf` |
| 2 | Synced cloud copy | Stops reading the secrets. Puts no journal beside the vault. Makes stale overwrites harder. | C2, C3 | `crates/akm-cli/tests/journal.rs :: nothing_but_the_vault_and_its_backups_appears_beside_a_synced_vault` |
| 3 | Malware that runs as the same Windows user | Does not help | none | none |
| 4 | Keylogger | Does not help | none | none |
| 5 | AI coding agent or other local program in the user's shell | Makes harder for accidents. Does not stop a deliberate read. | C7, C11 | `crates/akm-cli/tests/security.rs :: run_scrubs_all_akm_variables_preserves_normal_environment_and_child_exit` |
| 6 | Screen capture, or a person watching | Makes harder for software capture. Does not stop a camera or a person. | C6 (lock policy) | `desktop/src-tauri/src/commands.rs :: native_window_configuration_excludes_screenshots` |
| 7 | Hostile `.env` file | Makes harder (native review, scoped, reserved variable classes, size limits) | C7 | `crates/akm-cli/tests/security.rs :: every_reserved_class_is_refused_by_run_and_export_with_the_key_title_and_never_the_value` |
| 8 | Another Windows user or program that reads exported files | Makes harder for exports. Does not protect the vault file by ACL. | C8 | `crates/akm-cli/tests/security.rs :: file_export_rejects_an_ignored_private_dacl_before_writing` |
| 9 | Clipboard history, cloud clipboard or a clipboard reader | Makes harder. Does not stop a reader that is fast. | C10 | `desktop/src-tauri/src/clipboard.rs :: success_publishes_utf16_and_all_three_zero_exclusions_in_one_interval` |
| 10 | Memory dump, crash dump, page file or hibernation file | Does not help (best-effort cleanup only) | none | none |
| 11 | Someone at the unlocked computer | Makes harder (idle and session lock) | C6 | `desktop/src-tauri/src/policy.rs :: status_checks_do_not_extend_the_ten_minute_deadline` |
| 12 | Vault replaced or rolled back by a sync client or an attacker with write access | Makes harder for races. Records a return to a version this device saw; does not refuse it. | C2 | `crates/akm-core/tests/vault.rs :: two_snapshots_can_open_and_the_stale_writer_is_refused` |
| 13 | Someone who can write to the journal | Does not help (the journal is not authenticated evidence) | C12 | `crates/akm-core/tests/audit_states.rs :: audit_snapshot_distinguishes_absent_empty_and_consistent_without_modifying_bytes` |
| 14 | Malicious or vulnerable dependency (supply chain) | Makes harder, with the new checks. Does not stop a bad version in the lock file. | none | `tools/test_check_no_network.py` (new) |
| 15 | Network attacker | Stops, for AKM's own code: no network client or listener was found. The WebView2 runtime is outside this claim. | none | `tools/test_check_no_network.py` (new) |

Rows 1 to 7 are the attackers named in the security proposals ([ROADMAP.md](ROADMAP.md#security-proposals-not-authorized), item 1). Rows 8 to 15 are attackers found in the code.

## Notes per row

### 1. Stolen or copied vault file

- **Gets:** the `.kdbx` file. The folder also holds the convenience `.bak`, up to five dated encrypted backups, and, for vaults that older AKM versions used, a legacy plaintext journal `<vault>.audit.jsonl` that AKM now reports and never writes (read: `crates/akm-core/src/vault.rs`, `journal.rs`; test: `crates/akm-core/tests/journal_identity.rs :: a_legacy_sidecar_is_reported_and_stays_byte_identical`).
- **AKM does:** the file is encrypted and authenticated by the KDBX format. Test: a wrong password does not open it (`crates/akm-core/tests/vault.rs :: rejects_the_wrong_password`). Test: new vaults use KDBX 4.1 and Argon2id with 64 MiB, 3 iterations and parallelism 4 (`creates_vaults_with_a_hardened_kdf`). Test: a new vault needs 12 Unicode characters (`create_requires_twelve_unicode_characters_before_any_files_are_written`).
- **Limits:**
  - Offline guessing is limited only by the password and the KDF. AKM does not check password strength.
  - Backups keep old values of rotated secrets. The backups use the same password (read; test: `retains_the_five_latest_encrypted_originals_and_a_legacy_backup`).
  - The vault keeps native KDBX history, so old secret values stay inside the encrypted file until a purge (unit 2; test: `crates/akm-core/tests/lifecycle.rs :: rotate_replaces_the_secret_records_time_history_and_a_secret_free_record`).
  - The journal lives in a private per-device folder (`%LOCALAPPDATA%\akm\journal\`), not beside the vault, so a copied vault folder does not carry it. It is not encrypted. It holds action names, key IDs, counts and timestamps, and no title, project, environment, export path or secret ([design/unit1b.md](design/unit1b.md) D3; test: `crates/akm-cli/tests/journal.rs :: akm_audit_runs_without_a_password_and_shows_key_ids_only`). A legacy sidecar that already synced stays wherever it synced.
  - No second factor exists ([ROADMAP.md](ROADMAP.md) item 12).

### 2. Synced cloud copy

- **Gets:** everything in row 1, from the cloud provider or from anyone who has the cloud account. The sync folder holds the vault and its backups. The journal is not there (unit 1b), except a legacy sidecar from an older AKM version.
- **AKM does:** the vault content is encrypted as in row 1. AKM detects a vault that changed on disk since it was opened and refuses to overwrite it (test: `crates/akm-core/tests/vault.rs :: refuses_to_overwrite_a_vault_changed_on_disk`, `two_snapshots_can_open_and_the_stale_writer_is_refused`; `crates/akm-core/src/vault.rs :: source_replaced_while_serializing_is_not_overwritten_or_backed_up_as_ours`). The CLI's default vault path is `%APPDATA%\akm\vault.kdbx`, the Roaming application data folder (read: `default_vault_path` in `crates/akm-cli/src/main.rs`). Ordinary Windows does not sync it, but a domain roaming profile can. The user can choose any folder, including a sync folder. Detection and refusal cover only a fully downloaded folder.
- **Limits:**
  - Today the plaintext journal leaks access patterns (which key, when) to the cloud provider if the vault folder syncs. Unit 1b removes the journal from the vault folder. It cannot remove copies that already synced.
  - A short local lock gives no consensus and no atomic compare-and-swap in the cloud. A sync client can race the final check (PROJECT.md, trust boundaries).
  - AKM does not merge conflicts (PROJECT.md decision history).

### 3. Malware that runs as the same Windows user

- **Gets:** anything the user gets. It can read the vault file, the journal and the exports. It can read the memory of AKM, the environment of other processes, and the screen. It can wait for the user to type the master password. It can call the AKM CLI.
- **AKM does:** nothing that stops this. PROJECT.md says: "An unlocked app cannot protect secrets from malware running as that account."
- **Limits:** the idle lock (row 11), clipboard exclusions (row 9) and capture exclusion (row 6) reduce the time or the paths. They do not stop malware. The proposed antivirus hint ([ROADMAP.md](ROADMAP.md) item 10) would also not detect it. Real protection needs a separate Windows account or a separate machine.

### 4. Keylogger

- **Gets:** the master password as the user types it, then the vault file (row 1) and everything in row 3.
- **AKM does:** nothing. The CLI reads a hidden terminal prompt (`rpassword`). The desktop app reads a webview text field. Neither resists a keystroke hook (read: `crates/akm-cli/src/main.rs :: master_password`).
- **Limits:** a second factor would make a stolen password alone insufficient. It would not help against malware on an unlocked session (ROADMAP item 12 says the same).

### 5. AI coding agent or other local program in the user's shell

This attacker runs as the user, in a terminal or a build tool, and may start child processes.

- **What AKM does (test or read):**
  - There is no `--password` argument (`crates/akm-cli/tests/security.rs :: password_argument_is_not_accepted_or_echoed`).
  - `get` masks secrets by default and prints nothing derived from them, not even their length (`the_default_view_and_metadata_fields_print_nothing_derived_from_the_secret`).
  - `run` and `export` need an explicit project and environment, or `--all` (`unscoped_export_is_rejected_before_opening_the_vault`, `scope_is_complete_nonempty_and_exclusive_before_unlock`). This is C7.
  - Every `AKM_*` variable is removed from the child environment, so the master password is not passed on (C11; `run_scrubs_all_akm_variables_preserves_normal_environment_and_child_exit`).
  - A reveal or export is written to the journal before any secret is shown. If the journal cannot be written, AKM refuses (C12; `reveals_are_audited_before_any_secret_is_printed`, `unsafe_journal_appends_refuse_disclosure_export_and_launch_without_echo_or_byte_changes`).
  - Without a terminal and without `AKM_PASSWORD`, the CLI refuses to unlock ("no terminal available"). An agent that has no terminal cannot unlock the vault by itself (read: `master_password`).
- **What AKM does not do:**
  - If `AKM_PASSWORD` is set in the shell, any program in that shell can read it and unlock the vault. The README recommends the prompt for this reason (`crates/akm-cli/README.md`).
  - `run` puts the selected secrets in the child's environment. The child and every process it starts can read them. Child output is not masked, so a captured log can hold them.
  - `get --reveal`, `get --field secret` and `export` print secrets on purpose. The CLI cannot tell a person from an agent.
  - The journal is plaintext in the user's private folder, so an agent that runs as the user can read it. It records which key ID was revealed, not its title, and it is not authenticated evidence.
- **Verdict:** AKM reduces accidental leaks. It does not stop an agent that decides to read a key. The proposed agent guardrails are in [ROADMAP.md](ROADMAP.md) group D and have the same limit. The real boundary is a separate Windows account, a container, or a broker that holds the keys.

### 6. Screen capture, or a person watching the screen

- **AKM does:** the desktop window asks Windows to exclude it from capture (`contentProtected: true` in `desktop/src-tauri/tauri.conf.json`). Test: `desktop/src-tauri/src/commands.rs :: native_window_configuration_excludes_screenshots` checks the setting. The native proof `tools/verify_windows_app.ps1` reads the window's display affinity (`0x11`) on a running app. Secrets are masked by default and a revealed value is cleared when the view expires or the page becomes hidden, and the reveal countdown is tested (read: `desktop/src/components/KeyDetail.tsx`; test: `desktop/src/components/KeyDetail.timer.test.tsx`).
- **Limits:**
  - The exclusion works against capture software that honours the Windows setting. A phone camera, a person, a hardware capture card, or malware that reads memory is not stopped.
  - The CLI prints revealed secrets in the terminal, which has no capture exclusion.
  - Not checked: the setting on every Windows version.

### 7. Hostile `.env` file

This attacker supplies a `.env` file that the user imports (for example from a cloned repository).

- **AKM does:**
  - The parser follows a written dotenv profile, pinned with reference fixtures, and never executes or interpolates the file ([design/unit3.md](design/unit3.md); `crates/akm-core/tests/dotenv_profile.rs`). It refuses a file above 256 KiB, a line above 16 KiB and more than 500 entries before any dialog (`the_size_limits_are_exact_and_refuse_with_a_clear_message`; `desktop/src-tauri/src/commands.rs :: a_hostile_or_oversized_env_text_is_refused_with_a_clear_message_before_any_dialog`). It rejects bad names, repeated names and lines without `=` (`rejects_bad_names_duplicates_and_missing_equals`). An empty import changes nothing (`empty_import_is_refused_atomically`).
  - The desktop import is confirmed in one native command: a native dialog lists the names and the scope, never the values, and the import is applied only after Import in that dialog, so no review token exists to abandon ([design/unit1c.md](design/unit1c.md); `an_import_runs_only_after_the_user_approves_the_names_and_scope_that_were_shown`, `every_change_while_the_dialog_is_open_leaves_the_import_unapplied_and_recorded`). Matching is exact on project, environment and variable name (C7; `crates/akm-core/tests/vault.rs :: import_never_updates_a_different_project_or_environment`).
  - Seven classes of variable names that change how a child program runs, loads code or sends traffic are reserved, compared without regard to case, at import, add, edit, `run` and `export`: for example `AKM_*`, `PATH`, `PYTHONSTARTUP`, `NODE_OPTIONS`, `JAVA_TOOL_OPTIONS`, `DOTNET_STARTUP_HOOKS`, `GIT_SSH_COMMAND`, `HTTPS_PROXY`, `SSL_CERT_FILE` and `USERPROFILE` (the full table is in [design/unit3.md](design/unit3.md); `crates/akm-core/tests/reserved_names.rs :: every_listed_name_is_reserved_in_its_class_in_any_case`, `an_import_with_a_reserved_name_is_refused_for_every_class_and_names_only_the_variable`; `crates/akm-cli/tests/security.rs :: every_reserved_class_is_refused_by_run_and_export_with_the_key_title_and_never_the_value`).
- **Limits:**
  - The reserved table is a list, not a sandbox. A name that no class covers, and that a particular child program reads, can still change that program. A clean child environment is a proposal ([ROADMAP.md](ROADMAP.md) item 19).
  - The real native dialog has not been shown and clicked in a recorded proof; the tests use a scripted confirmation. That proof needs the owner.
  - A hostile file cannot read the vault. It can only add or change keys in the scope the user chose.

### 8. Another Windows user, or a program, that reads exported files

- **AKM does:** a file export goes to a new local file with a protected, owner-only DACL. Unsupported storage, UNC paths, device paths and alternate data streams are refused before plaintext is written. An existing file is never overwritten by the CLI (C8; `crates/akm-cli/tests/security.rs :: file_export_rejects_an_ignored_private_dacl_before_writing`, `file_export_rejects_nonpersistent_acls_before_unlock_and_before_writing`, `file_export_never_overwrites_a_target_created_after_validation`; `desktop/src-tauri/src/private_export.rs :: an_ignored_private_dacl_is_rejected_before_plaintext_is_written`).
- **Limits:** the export file holds plaintext until the user removes it. A program that runs as the same user can read it (row 3). AKM sets no ACL on the vault file; it relies on the permissions of the folder (read: no ACL code in `crates/akm-core/src`). Stdout export has no protection.

### 9. Clipboard history, cloud clipboard or a clipboard reader

- **AKM does:** a copy sets the Windows formats that exclude the item from clipboard history, from cloud clipboard sync and from monitor processing, in the same open interval as the text (C10; `desktop/src-tauri/src/clipboard.rs :: success_publishes_utf16_and_all_three_zero_exclusions_in_one_interval`). Cleanup clears only AKM's own copy and never a newer identical copy by the user (`identical_user_copy_between_publication_and_capture_is_not_owned_or_cleared`, `a_later_users_clipboard_is_preserved_even_when_its_text_matches`). A lock sends its notice first and then makes one cleanup attempt that never waits for a busy clipboard (`desktop/src-tauri/src/session.rs :: a_lock_delivers_its_notice_while_a_tick_cleanup_is_stalled_and_the_gate_stays_free`, `every_public_entry_sends_its_notice_first_and_makes_exactly_one_attempt`).
- **Limits:** a program that polls the clipboard reads the secret while it is there. The exclusions rely on other programs to respect them. A busy clipboard at lock or exit can keep the copy; AKM then says so. A real-clipboard test exists but is opt-in and needs the owner ([TESTING.md](TESTING.md)).

### 10. Memory dump, crash dump, page file or hibernation file

- **AKM does:** some secret buffers use `zeroize` (read: `Zeroize` and `Zeroizing` in `crates/akm-cli/src/child.rs`, `crates/akm-cli/src/main.rs` and `desktop/src-tauri/src/session.rs`). PROJECT.md calls memory cleanup defense in depth.
- **Limits:** the `keepass` crate's database model and the webview hold the decrypted data while the vault is unlocked. No test shows that memory is clean after a lock. Windows can write memory to the page file, a crash dump or the hibernation file. AKM does not lock pages or disable dumps. Not checked.

### 11. Someone at the unlocked computer

- **AKM does:** a 10-minute idle timeout, and a lock on Windows session lock, suspend or sign-out. A status poll does not renew the idle timer. Only explicit activity does (C6; `desktop/src-tauri/src/policy.rs :: status_checks_do_not_extend_the_ten_minute_deadline`, `only_explicit_activity_in_an_active_session_renews_idle`, `unobservable_or_locked_windows_session_fails_closed`). A late unlock cannot reopen after a lock signal (`delayed_unlock_cannot_reopen_after_any_lock_signal`).
- **Limits:** within the 10 minutes the person has the open vault. The CLI has no timeout: after it unlocks, the process lives only for one command. Not tested: real OS lock and suspend delivery, and a real 10-minute idle wait ([TESTING.md](TESTING.md), "Not tested yet").

### 12. Vault replaced or rolled back by a sync client or an attacker with write access

- **AKM does:** a writer compares the file fingerprint to the one it opened and refuses a stale write; reload adopts a complete external snapshot (C2; `crates/akm-core/tests/vault.rs :: two_snapshots_can_open_and_the_stale_writer_is_refused`, `reload_adopts_a_complete_external_snapshot_and_failed_reload_keeps_the_old_one`; `crates/akm-core/src/vault.rs :: a_postcommit_external_write_is_not_adopted_as_our_fingerprint`). The KDBX format authenticates its content, so an edit without the password is detected by the format (reported by the format specification; no AKM test). The journal remembers the last 32 versions of each vault that this device saw. When an earlier one of them returns, the next journaled action first records a rollback indication in the same journal (`crates/akm-core/tests/journal_identity.rs :: restoring_an_earlier_version_is_recorded_in_the_same_journal`).
- **Limits:** an older valid copy of the vault still opens normally; the rollback indication is a record, not a refusal. A version that this device never saw, or one older than its last 32, is not recognized. A replacement by a different vault starts a new journal and is not reported as a rollback. An attacker who can write to the folder can also delete the vault and its backups.

### 13. Someone who can write to the journal

- **AKM does:** the journal has a hash chain, and AKM reports damage and preserves the damaged bytes (`crates/akm-core/tests/audit_states.rs :: audit_snapshot_distinguishes_absent_empty_and_consistent_without_modifying_bytes`, `audit_damage_is_structured_and_display_limit_does_not_skip_verification`).
- **Limits:** the chain has no independent anchor. Truncating the file at a record boundary, or rewriting it with a new valid chain, is not detected ([design/unit1b.md](design/unit1b.md) D7). The journal is not authenticated evidence (C12 and PROJECT.md). Unit 1b moved it to a private folder that grants only the current user (test: `crates/akm-core/tests/journal_identity.rs :: a_folder_with_an_extra_account_is_refused_and_nothing_is_written`), so another Windows user cannot write it; the same user still can.

### 14. Malicious or vulnerable dependency (supply chain)

- **AKM does:** `Cargo.lock` is committed and builds use `--locked` ([TESTING.md](TESTING.md)). The checks `tools/check_dependencies.ps1` (advisories, licenses, sources, bans) and `tools/check_no_network.py` run locally. They find known advisories and the entry of a networking crate. The KDBX writer is a community crate (PROJECT.md).
- **Limits:** an advisory database lists only known problems. A malicious new version in the lock file passes if it has no advisory. Builds are unsigned development builds. Reproducible builds and fuzzing are proposals ([ROADMAP.md](ROADMAP.md) items 4 and 5). Findings from the checks are in [FINDINGS.md](FINDINGS.md) when a person has read them.

### 15. Network attacker

- **AKM does:** the Rust code has no network client and no listener. The Windows dependency tree has no networking crate, the content security policy has no remote origin, the capabilities grant no http, shell, opener or updater access, and the frontend packages hold no network client. `tools/check_no_network.py` checks this. `tools/test_check_no_network.py` shows that the check fails for a planted `reqwest`.
- **Limits:**
  - The check cannot prove the absence of a connection. It looks for the usual entry points.
  - The WebView2 runtime is Microsoft software. AKM does not control its own network use.
  - The Windows installer may download the WebView2 bootstrapper at install time (`webviewInstallMode: downloadBootstrapper`). That is an install-time request by the installer, not by AKM at run time. The promise must say "at run time".
  - The Tauri crate declares `reqwest` for Android and iOS builds. AKM does not build for them. The shipped Windows tree does not hold it.

## Out of scope

Hardware attacks, a malicious operating system, a compromised Windows administrator, firmware, and coercion. A separate Windows account, a container or a broker would change several rows. They are not built.

## Keep this document true

Change a row when a contract, a test or a seam changes. Do not add a claim that has no test or source reading behind it.
