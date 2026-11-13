# Threat model

This document states which attackers AKM stops, which it makes harder, and which it does not help. It describes the code as it is on 2026-11-07. It extends the trust boundaries in [PROJECT.md](PROJECT.md#non-goals-and-trust-boundaries). It adds no promise that the code does not already keep.

AKM has had no professional audit, and the latest measured score is 87.5/100 ([second full review](reviews/2026-11-13-full-review-2.md)). Open defects are in [FINDINGS.md](FINDINGS.md). Do not use real secrets before the release gate in [AGENTS.md](../AGENTS.md).

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
| 8 | Another Windows user or program that reads exported files | Makes harder for exports, and for the vault files that a save writes (owner-only permissions; on a drive that cannot keep them, the save warns). | C8 | `crates/akm-cli/tests/security.rs :: file_export_rejects_an_ignored_private_dacl_before_writing`, `crates/akm-core/src/vault.rs :: a_save_leaves_the_vault_and_every_backup_private_to_the_current_user` |
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

- **Gets:** the `.kdbx` file. The folder also holds the convenience `.bak`, the dated encrypted backups (AKM keeps the newest five that its list inside the vault names, and never deletes one that the list does not name, so more can be there), and, for vaults that older AKM versions used, a legacy plaintext journal `<vault>.audit.jsonl` that AKM now reports and never writes (read: `crates/akm-core/src/vault.rs`, `journal.rs`; test: `crates/akm-core/tests/journal_identity.rs :: a_legacy_sidecar_is_reported_and_stays_byte_identical`).
- **AKM does:** the file is encrypted and authenticated by the KDBX format. Test: a wrong password does not open it (`crates/akm-core/tests/vault.rs :: rejects_the_wrong_password`). Test: new vaults use KDBX 4.1 and Argon2id with 64 MiB, 3 iterations and parallelism 4 (`creates_vaults_with_a_hardened_kdf`). Test: a new vault needs 12 Unicode characters (`create_requires_twelve_unicode_characters_before_any_files_are_written`).
- **Limits:**
  - Offline guessing is limited only by the password and the KDF. AKM does not check password strength.
  - Backups keep old values of rotated secrets, and a purged key (FR-06 under "Disclosed limits"). The backups use the same password (read; test: `retains_the_five_latest_encrypted_originals_and_a_legacy_backup`).
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
  - The parser follows a written dotenv profile, pinned with reference fixtures, and never executes or interpolates the file ([design/unit3.md](design/unit3.md); `crates/akm-core/tests/dotenv_profile.rs`). It refuses a file above 256 KiB, a line above 16 KiB and more than 500 entries before any dialog (`the_size_limits_are_exact_and_refuse_with_a_clear_message`; `desktop/src-tauri/src/commands.rs :: a_hostile_or_oversized_env_text_is_refused_with_a_clear_message_before_any_dialog`). It rejects bad names, repeated names and lines without `=` (`every_fixture_parses_as_the_profile_says`, with the fixtures `lowercase_name`, `duplicates` and `malformed_no_equals`). An empty import changes nothing (`empty_import_is_refused_atomically`).
  - The desktop import is confirmed in one native command: a native dialog lists the names and the scope, never the values, and the import is applied only after Import in that dialog, so no review token exists to abandon. After the dialog returns, the user's click stands against a hide, a page load, a discard of the page's form and a hidden window. Only a lock, another session or a change of the vault since the preview refuses it, and the write checks all three under the session gate ([design/unit1c.md](design/unit1c.md); `an_import_runs_only_after_the_user_approves_the_names_and_scope_that_were_shown`, `p2r2_02_the_click_in_the_dialog_stands_after_a_hide_a_page_load_a_discard_or_a_hidden_window`, `every_change_while_the_dialog_is_open_leaves_the_import_unapplied_and_recorded`). The page's `view_discard` names its session, and a request for a session that is not the current one changes nothing (`p2r2_03_a_discard_for_a_session_that_is_not_current_succeeds_and_changes_nothing`). Matching is exact on project, environment and variable name (C7; `crates/akm-core/tests/vault.rs :: import_never_updates_a_different_project_or_environment`).
  - Nine classes of variable names that change how a child program runs, loads code, sends traffic or writes its output, or that name the file that it reads its settings or credentials from, are reserved, compared without regard to case, at import, add, edit, `run` and `export`: for example `AKM_*`, `PATH`, `PYTHONSTARTUP`, `NODE_OPTIONS`, `JAVA_TOOL_OPTIONS`, `DOTNET_STARTUP_HOOKS`, `GIT_SSH_COMMAND`, `HTTPS_PROXY`, `SSL_CERT_FILE`, `KUBECONFIG`, `GOOGLE_APPLICATION_CREDENTIALS` and `USERPROFILE` (the table is in `crates/akm-core/src/model.rs`, and the first design is in [design/unit3.md](design/unit3.md); `crates/akm-core/tests/reserved_names.rs :: every_listed_name_is_reserved_in_its_class_in_any_case`, `an_import_with_a_reserved_name_is_refused_for_every_class_and_names_only_the_variable`; `crates/akm-cli/tests/security.rs :: every_reserved_class_is_refused_by_run_and_export_with_the_key_title_and_never_the_value`).
  - One rule, `reserved_class`, decides every name: a name is reserved in one class, or it is free (`crates/akm-core/tests/name_classes.rs :: every_reserved_row_has_its_class_and_every_free_name_has_none_in_any_case`). A name is reserved by an exact name, by a tool family's prefix, by a build-variable shape such as `HOST_CC` or `CC_<target>`, or by its last word, for example `PATH`, `DIR`, `HOME`, `PROXY`, `OPTS`, `FILE` or `CONFIG`. A credential word such as `TOKEN` gives a name no other treatment than any free name (`a_credential_word_gives_a_name_the_same_treatment_as_any_free_name`). The rule is broad on purpose, so it also reserves names that no known tool reads, for example `UPLOAD_DIR`. A stored key under a reserved name stays visible, and `run` and `export` use it again after a rename. The desktop import confirmation says the same caution about every name that it lists: a program gets the name as a variable and can read it as a setting (`desktop/src-tauri/src/import_review.rs :: every_name_is_listed_the_same_way_and_the_review_says_what_a_name_does`).
- **Limits:**
  - The reserved table is a list, not a sandbox. A name that no class covers, and that a particular child program reads, can still change that program. A clean child environment is a proposal ([ROADMAP.md](ROADMAP.md) item 19).
  - The real native dialog has not been shown and clicked in a recorded proof; the tests use a scripted confirmation. That proof needs the owner.
  - A hostile file cannot read the vault. It can only add or change keys in the scope the user chose.

### 8. Another Windows user, or a program, that reads exported files

- **AKM does:** a file export goes to a new local file with a protected, owner-only DACL. Unsupported storage, UNC paths, device paths and alternate data streams are refused before plaintext is written. An existing file is never overwritten by the CLI. A desktop export can replace a file only after the Windows save dialog's own replace prompt (`desktop/src-tauri/src/private_export.rs :: overwriting_an_existing_file_keeps_the_export_private`) (C8; `crates/akm-cli/tests/security.rs :: file_export_rejects_an_ignored_private_dacl_before_writing`, `file_export_rejects_nonpersistent_acls_before_unlock_and_before_writing`, `file_export_never_overwrites_a_target_created_after_validation`; `desktop/src-tauri/src/private_export.rs :: an_ignored_private_dacl_is_rejected_before_plaintext_is_written`).
- **Limits:** the export file holds plaintext until the user removes it. A program that runs as the same user can read it (row 3). Each save writes the vault, its `.bak` copy and its dated backups as new files that only the current Windows account can use (`crates/akm-core/src/vault.rs :: a_save_leaves_the_vault_and_every_backup_private_to_the_current_user`). On a drive that reports that it cannot keep Windows permissions (FAT, exFAT), the save succeeds and warns that the files are not private (`a_save_on_a_file_system_without_private_files_succeeds_and_warns`). On a drive that keeps them, a file that cannot be made private stops the save, and nothing is written (`a_failed_private_creation_on_a_volume_that_keeps_permissions_fails_the_save`). A path longer than 260 characters gets the same private files (`a_save_on_a_long_path_in_a_shared_folder_keeps_every_file_private`). An administrator can still read them. The vault's folder keeps its own permissions, and a vault that AKM has not saved yet keeps the permissions it had. Stdout export has no protection.

### 9. Clipboard history, cloud clipboard or a clipboard reader

- **AKM does:** a copy sets the Windows formats that exclude the item from clipboard history, from cloud clipboard sync and from monitor processing, in the same open interval as the text (C10; `desktop/src-tauri/src/clipboard.rs :: success_publishes_utf16_and_all_three_zero_exclusions_in_one_interval`). Cleanup clears only AKM's own copy and never a newer identical copy by the user (`identical_user_copy_between_publication_and_capture_is_not_owned_or_cleared`, `a_later_users_clipboard_is_preserved_even_when_its_text_matches`). A lock sends its notice first and then makes one cleanup attempt that never waits for a busy clipboard (`desktop/src-tauri/src/session.rs :: a_lock_delivers_its_notice_while_a_tick_cleanup_is_stalled_and_the_gate_stays_free`, `every_public_entry_sends_its_notice_first_and_makes_exactly_one_attempt`).
- **Limits:** a program that polls the clipboard reads the secret while it is there. The exclusions rely on other programs to respect them. AKM attempts to clear its copy; it does not promise to. A busy clipboard at lock or exit can keep the copy, and AKM then says so. A copy also stays if AKM stops unexpectedly, because no cleanup runs then. A real-clipboard test exists but is opt-in and needs the owner ([TESTING.md](TESTING.md)).

### 10. Memory dump, crash dump, page file or hibernation file

- **AKM does:** some secret buffers use `zeroize` (read: `Zeroize` and `Zeroizing` in `crates/akm-cli/src/child.rs`, `crates/akm-cli/src/main.rs` and `desktop/src-tauri/src/session.rs`). PROJECT.md calls memory cleanup defense in depth.
- **Limits:** the `keepass` crate's database model and the webview hold the decrypted data while the vault is unlocked. No test shows that memory is clean after a lock. Windows can write memory to the page file, a crash dump or the hibernation file. AKM does not lock pages or disable dumps. Not checked.

### 11. Someone at the unlocked computer

- **AKM does:** a 10-minute idle timeout, and a lock on Windows session lock, suspend or sign-out. A status poll does not renew the idle timer. Only explicit activity does (C6; `desktop/src-tauri/src/policy.rs :: status_checks_do_not_extend_the_ten_minute_deadline`, `only_explicit_activity_in_an_active_session_renews_idle`, `unobservable_or_locked_windows_session_fails_closed`). A late unlock cannot reopen after a lock signal (`delayed_unlock_cannot_reopen_after_any_lock_signal`).
- **Limits:** within the 10 minutes the person has the open vault. The CLI has no timeout: after it unlocks, the process lives only for one command. Not tested: real OS lock and suspend delivery, and a real 10-minute idle wait ([TESTING.md](TESTING.md), "Not tested yet").

### 12. Vault replaced or rolled back by a sync client or an attacker with write access

- **AKM does:** a writer compares the file fingerprint to the one it opened and refuses a stale write; reload adopts a complete external snapshot (C2; `crates/akm-core/tests/vault.rs :: two_snapshots_can_open_and_the_stale_writer_is_refused`, `reload_adopts_a_complete_external_snapshot_and_failed_reload_keeps_the_old_one`; `crates/akm-core/src/vault.rs :: a_postcommit_external_write_is_not_adopted_as_our_fingerprint`). The KDBX format authenticates its content, so an edit without the password is detected by the format (reported by the format specification; no AKM test). The journal remembers the last 32 versions of each vault that this device saw. When an earlier one of them returns, the next journaled action first records a rollback indication in the same journal (`crates/akm-core/tests/journal_identity.rs :: restoring_an_earlier_version_is_recorded_in_the_same_journal`). When the file at the vault's path holds a version that the journal never saw, with the same vault ID, the journal continues, and AKM says that it cannot confirm that the file is the same vault (`a_read_of_an_unseen_clone_is_unconfirmed_with_a_notice_and_changes_nothing`; ordinary saves give no such notice, `ordinary_saves_and_reopens_give_no_lineage_warning`).
- **Limits:** an older valid copy of the vault still opens normally; the rollback indication is a record, not a refusal. A version that this device never saw, or one older than its last 32, is not recognized. A replacement by a different vault starts a new journal and is not reported as a rollback. An attacker who can write to the folder can also delete the vault and its backups.

### 13. Someone who can write to the journal

- **AKM does:** the journal has a hash chain, and AKM reports damage and preserves the damaged bytes (`crates/akm-core/tests/audit_states.rs :: audit_snapshot_distinguishes_absent_empty_and_consistent_without_modifying_bytes`, `audit_damage_is_structured_and_display_limit_does_not_skip_verification`). A damaged journal, or a damaged mapping file, stops disclosure until the user approves a recovery in a native window that names the file. The recovery keeps a copy of that file byte for byte under a dated name. Then one move puts the new file in its place, and no other file changes. A stop before that move leaves the damaged file in place, so the recovery is offered again (FW-02). A mapping that is missing beside journals counts as damaged. After a damaged mapping, the next record of each vault starts a new journal with a record of the gap and a lineage warning (tests: `crates/akm-core/src/audit.rs :: fv01_a_recovery_of_a_damaged_mapping_resumes_both_vaults_and_states_the_gap`, `fv01_a_failed_mapping_recovery_changes_nothing_and_a_retry_ends_the_same`, `fw02_a_journal_recovery_that_stops_before_its_last_step_leaves_the_damage_in_place`, `fw02_a_mapping_recovery_that_stops_before_its_last_step_leaves_the_damage_in_place`, `fw02_a_missing_mapping_beside_a_journal_is_damage_and_its_recovery_states_the_gap`; `desktop/src-tauri/src/commands.rs :: fv01_a_damaged_mapping_is_recovered_through_the_handler_and_both_vaults_resume`).
- **Limits:** the chain has no independent anchor. A process that runs as the user can damage the mapping file and so stop disclosure for every vault until the user recovers; after that recovery, the earlier journals are linked to no vault, and the Activity view does not show them. Truncating the file at a record boundary, or rewriting it with a new valid chain, is not detected ([design/unit1b.md](design/unit1b.md) D7). A journal file that is deleted reads as an empty journal, in the same way; a build from before 13 Nov 2026 that stopped in the middle of a journal recovery can have left that state, and no such build was released. A recovery that stops leaves its copy and a `.recovering-*` file in the folder; AKM reads neither and removes neither. The journal is not authenticated evidence (C12 and PROJECT.md). Unit 1b moved it to a private folder that grants only the current user (test: `crates/akm-core/tests/journal_identity.rs :: a_folder_with_an_extra_account_is_refused_and_nothing_is_written`), so another Windows user cannot write it; the same user still can.

### 14. Malicious or vulnerable dependency (supply chain)

- **AKM does:** `Cargo.lock` is committed and builds use `--locked` ([TESTING.md](TESTING.md)). The checks `tools/check_dependencies.ps1` (advisories, licenses, sources, bans) and `tools/check_no_network.py` run locally. They find known advisories and the entry of a networking crate. The KDBX writer is a community crate (PROJECT.md).
- **Limits:** an advisory database lists only known problems. A malicious new version in the lock file passes if it has no advisory. Builds are unsigned development builds ([RELEASING.md](RELEASING.md) compares the signing options). AKM does not promise a reproducible build: CI rebuilds the CLI and reports whether the hashes match, and does not fail on a difference ([TESTING.md](TESTING.md#reproducible-builds)). Fuzzing is a proposal ([ROADMAP.md](ROADMAP.md) item 5). Findings from the checks are in [FINDINGS.md](FINDINGS.md) when a person has read them.

### 15. Network attacker

- **AKM does:** the Rust code has no network client and no listener. The Windows dependency tree has no networking crate, the content security policy has no remote origin, the capabilities grant no http, shell, opener or updater access, and the frontend packages hold no network client. `tools/check_no_network.py` checks this. `tools/test_check_no_network.py` shows that the check fails for a planted `reqwest`.
- **Limits:**
  - The check cannot prove the absence of a connection. It looks for the usual entry points.
  - The WebView2 runtime is Microsoft software. AKM does not control its own network use.
  - The Windows installer holds the WebView2 bootstrapper (`webviewInstallMode: embedBootstrapper` in `desktop/src-tauri/tauri.conf.json`). When the WebView2 runtime is missing, the bootstrapper downloads it at install time. That is an install-time request by Microsoft's bootstrapper, not by AKM at run time. The promise must say "at run time".
  - The Tauri crate declares `reqwest` for Android and iOS builds. AKM does not build for them. The shipped Windows tree does not hold it.

## Disclosed limits

These limits are known and open. Each decision below is a decision to disclose, not a fix. The finding numbers are in [FINDINGS.md](FINDINGS.md).

- **FR-06. A purged key stays in older copies.** A purge removes the key from the live vault. The key stays in older encrypted backups beside the vault and in copies that a sync provider or the user made elsewhere. AKM deletes a dated backup only when the list inside the vault names it, five newer listed backups exist, and the file still holds the recorded bytes (`crates/akm-core/src/backups.rs`; test: `pruning_deletes_only_backups_that_prove_they_are_owned`). So a purged key stays in the `.bak` copy until the next save, and in a listed backup until five later saves. It stays until the user removes it by hand in every backup that the list does not name (one from an AKM version before the list, one that a crash left during a save, one from before the vault was renamed) and in every copy elsewhere. AKM never deletes an unlisted backup, because it cannot prove that it wrote it, and a backup can be the only good copy.
- **FR-07. The journal has no independent anchor.** An anchor kept outside the journal (a signed head, a second file, a service) would not help against the attacker in row 13. A program that runs as the same Windows account could change any anchor too. AKM adds none, and it keeps saying that the journal is not authenticated evidence.
- **FR-09. `AKM_PASSWORD` stays in the parent's environment.** `akm run` removes it from the child. The shell that set it keeps it, and every program that this shell starts can read it (row 5). The interactive prompt is the safer path, and the README says so.
- **2-W-01. KeePassXC merge does not apply deleted-object records.** KeePassXC 2.8.0 `merge` ignores the `DeletedObjects` records that AKM writes on purge. When the user reconciles a stale copy with KeePassXC, a key that AKM purged can return. kdbxweb applies the records. After a KeePassXC merge, check the Recycle Bin and the key list, and purge again if needed. Test: `tools/verify_keepassxc.py` compares an AKM purge with a KeePassXC permanent delete ([TESTING.md](TESTING.md)).
- **FV-09. A script in the page could read single keys without a native window.** The desktop app answers up to 5 single-key requests (a reveal or a copy of one key) in each 60 seconds without a native approval (`FREE_SINGLE_DISCLOSURES` and `DISCLOSURE_WINDOW` in `desktop/src-tauri/src/session.rs`). AKM writes a journal record before each of them. A sixth request in that time, a preview, a copy of several keys, an export and a terminal each need a native window. So a script that ran in the page could read 5 keys in a minute while the vault is open, and each read would be in the journal. The content security policy of the page allows scripts from the app only, and no review found a way to run such a script. AKM keeps the allowance, because a native window for every single reveal would make the normal use of the app slow.
- **FV-10. AKM clears only the copies that its own code holds.** `akm run` gives the keys to the child through `std::process::Command` (`crates/akm-cli/src/child.rs`), which keeps its own copy of each value until the command is dropped. AKM cannot clear that copy. It also does not clear a copy that the standard library, Windows, the `keepass` crate, the password prompt, the Tauri bridge or the webview makes. What its own code holds, it clears: the dotenv text is built with no intermediate copy (test: `a_dotenv_line_leaves_no_uncleared_copy_of_its_value`), and the dotenv parser and the import preview hold each value in a buffer that never grows and that is cleared when dropped, also when the document is refused (FW-09; tests: `a_dotenv_parse_leaves_no_uncleared_copy_of_a_value_on_any_way_out`, `an_import_preview_leaves_no_uncleared_copy_of_a_value`). The caller clears the text that it passes and the entries that it gets. This is cleanup, not a promise about memory (row 10).

## Out of scope

Hardware attacks, a malicious operating system, a compromised Windows administrator, firmware, and coercion. A separate Windows account, a container or a broker would change several rows. They are not built.

## Keep this document true

Change a row when a contract, a test or a seam changes. Do not add a claim that has no test or source reading behind it. A promise to users also goes in [CLAIMS.md](CLAIMS.md), where `tools/check_docs.py` checks that its evidence exists.
