use akm_core::model::{derive_env_var, is_valid_env_var};
use akm_core::{ApiKeyInput, EnvDisclosure, Error, KeyDisclosure, Vault};

const PASSWORD: &str = "correct horse battery staple";

/// A stored secret, read through a recorded disclosure request (the journal sits in the
/// temp folder of the test vault).
fn secret_of(vault: &Vault, id: &str) -> String {
    let key = vault.get_by_id(id).unwrap();
    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &key, Some("test"))
        .unwrap();
    vault.secret(id, &proof).unwrap().to_string()
}

fn sample() -> ApiKeyInput {
    let mut input = ApiKeyInput::new("OpenAI", "sk-test-123456789");
    input.project = Some("demo".into());
    input.environment = Some("dev".into());
    input.issuer = Some("OpenAI".into());
    input.scopes = vec!["chat".into(), "models".into()];
    input.rotation_days = Some(90);
    input.tags = vec!["ai".into()];
    input
}

#[test]
fn round_trips_through_a_real_kdbx_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");

    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    let created = vault.add(sample()).unwrap();
    assert_eq!(vault.list().len(), 1);
    drop(vault);

    let vault = Vault::open(&path, PASSWORD).unwrap();
    let found = vault.find("openai").unwrap();

    assert_eq!(found.id, created.id);
    assert_eq!(secret_of(&vault, &found.id), "sk-test-123456789");
    assert_eq!(found.project.as_deref(), Some("demo"));
    assert_eq!(found.environment.as_deref(), Some("dev"));
    assert_eq!(found.scopes, vec!["chat", "models"]);
    assert_eq!(found.rotation_days, Some(90));
    assert_eq!(found.tags, vec!["ai"]);
    assert_eq!(found.resolved_env_var(), "OPENAI");
}

#[test]
fn rejects_the_wrong_password() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");

    Vault::create(&path, PASSWORD, None).unwrap();
    let error = match Vault::open(&path, "wrong password") {
        Ok(_) => panic!("vault opened with the wrong password"),
        Err(error) => error,
    };
    assert!(matches!(error, Error::Open(_)));
}

#[test]
fn refuses_to_overwrite_a_vault_changed_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");

    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    vault.add(sample()).unwrap();

    // Simulate a cloud client replacing the file underneath us.
    std::fs::write(&path, b"replaced by another device").unwrap();

    let error = vault.save().unwrap_err();
    assert!(matches!(error, Error::VaultChangedOnDisk));
}

#[test]
fn updates_and_removes_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");

    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    let first = vault.add(sample()).unwrap();

    let mut changed = sample();
    changed.title = "OpenAI production".into();
    changed.environment = Some("prod".into());
    let updated = vault.update(&first.id, changed).unwrap();

    assert_eq!(updated.id, first.id);
    assert_eq!(updated.title, "OpenAI production");
    assert_eq!(updated.environment.as_deref(), Some("prod"));

    vault.remove(&first.id).unwrap();
    assert!(vault.list().is_empty());
}

#[test]
fn rejects_colliding_environment_variables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");

    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    let mut one = sample();
    one.env_var = Some("SHARED".into());
    let mut two = sample();
    two.title = "Another".into();
    two.env_var = Some("SHARED".into());

    vault.add(one).unwrap();
    vault.add(two).unwrap();

    assert!(vault.plan_env(None, None, true).is_err());
}

#[test]
fn audit_log_is_a_valid_hash_chain() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");

    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    let key = vault.add(sample()).unwrap();
    vault.log("reveal", Some(&key), None).unwrap();

    let log = vault.audit_log();
    assert_eq!(log.verify().unwrap(), None);
    assert_eq!(log.read(10).unwrap().len(), 2);
    assert!(log.journal_path().unwrap().unwrap().exists());
}

#[test]
fn imports_dotenv_pairs_and_updates_matching_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");

    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    let mut existing = sample();
    existing.env_var = Some("OPENAI_API_KEY".into());
    vault.add(existing).unwrap();

    let (added, updated) = vault
        .import_env(
            &[
                ("OPENAI_API_KEY".to_string(), "rotated-secret".to_string()),
                ("GITHUB_TOKEN".to_string(), "ghp_from_env".to_string()),
            ],
            Some("demo"),
            Some("dev"),
        )
        .unwrap();

    assert_eq!((added, updated), (1, 1));

    let keys = vault.list();
    let openai = keys
        .iter()
        .find(|key| key.resolved_env_var() == "OPENAI_API_KEY")
        .unwrap();
    assert_eq!(secret_of(&vault, &openai.id), "rotated-secret");
    assert_eq!(openai.title, "OpenAI");

    let github = keys
        .iter()
        .find(|key| key.resolved_env_var() == "GITHUB_TOKEN")
        .unwrap();
    assert_eq!(secret_of(&vault, &github.id), "ghp_from_env");
    assert_eq!(github.project.as_deref(), Some("demo"));
}

#[test]
fn import_never_updates_a_different_project_or_environment() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    let mut existing = sample();
    existing.env_var = Some("OPENAI_API_KEY".into());
    let original = vault.add(existing).unwrap();

    assert_eq!(
        vault
            .import_env(
                &[("OPENAI_API_KEY".into(), "other-project-secret".into())],
                Some("other"),
                Some("prod"),
            )
            .unwrap(),
        (1, 0),
    );
    assert_eq!(vault.get_by_id(&original.id).unwrap(), original);
    assert_eq!(vault.list().len(), 2);
}

#[test]
fn empty_import_is_refused_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    let original = vault.add(sample()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(vault
        .import_env(
            &[
                ("VALID".into(), "new-secret".into()),
                ("EMPTY".into(), String::new())
            ],
            Some("demo"),
            Some("dev"),
        )
        .is_err());
    assert_eq!(vault.list(), vec![original]);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn duplicate_import_names_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert!(vault
        .import_env(
            &[
                ("TOKEN".into(), "one".into()),
                ("TOKEN".into(), "two".into())
            ],
            Some("demo"),
            Some("dev"),
        )
        .is_err());
    assert!(vault.list().is_empty());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn import_requires_scope_and_safe_process_variables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    for name in [
        "invalid-name",
        "AKM_PASSWORD",
        "PATH",
        "PATHEXT",
        "COMSPEC",
        "SYSTEMROOT",
        "WINDIR",
        "NODE_OPTIONS",
        "NODE_PATH",
        "PYTHONPATH",
        "PYTHONHOME",
        "LD_PRELOAD",
        "DYLD_INSERT_LIBRARIES",
    ] {
        let entries = [(name.to_owned(), "fake-secret".into())];
        assert!(
            vault
                .import_env(&entries, Some("demo"), Some("dev"))
                .is_err(),
            "accepted {name}"
        );
    }
    for (project, environment) in [
        (None, None),
        (Some("demo"), None),
        (None, Some("dev")),
        (Some(""), Some("dev")),
        (Some("demo"), Some("  ")),
    ] {
        assert!(vault
            .import_env(
                &[("TOKEN".into(), "fake-secret".into())],
                project,
                environment
            )
            .is_err());
    }
    assert!(vault.list().is_empty());
}

#[test]
fn unchanged_import_does_not_save_or_modify_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
    let original = vault.add(sample()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(
        vault
            .import_env(
                &[("OPENAI".into(), "sk-test-123456789".into())],
                Some("demo"),
                Some("dev")
            )
            .unwrap(),
        (0, 0)
    );
    assert_eq!(vault.list(), vec![original]);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn failed_mutations_leave_the_open_snapshot_unchanged() {
    for operation in ["add", "update", "remove", "import"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.kdbx");
        let mut vault = Vault::create(&path, PASSWORD, None).unwrap();
        let original = vault.add(sample()).unwrap();
        std::fs::write(&path, b"external replacement").unwrap();
        let result = match operation {
            "add" => vault
                .add(ApiKeyInput::new("Another", "new-secret"))
                .map(|_| ()),
            "update" => vault
                .update(&original.id, ApiKeyInput::new("Changed", "changed-secret"))
                .map(|_| ()),
            "remove" => vault.remove(&original.id).map(|_| ()),
            "import" => vault
                .import_env(
                    &[
                        ("OPENAI".into(), "changed-secret".into()),
                        ("NEW".into(), "new-secret".into()),
                    ],
                    Some("demo"),
                    Some("dev"),
                )
                .map(|_| ()),
            _ => unreachable!(),
        };
        assert!(
            matches!(result, Err(Error::VaultChangedOnDisk)),
            "{operation}"
        );
        assert_eq!(vault.list(), vec![original], "{operation}");
    }
}

fn write_database(path: &std::path::Path, db: &keepass::Database, password: &str) {
    db.save(
        &mut std::fs::File::create(path).unwrap(),
        keepass::DatabaseKey::new().with_password(password),
    )
    .unwrap();
}

fn fast_database() -> keepass::Database {
    let mut db = keepass::Database::new();
    db.config.kdf_config = keepass::config::KdfConfig::Aes { rounds: 10 };
    db
}

#[test]
fn native_uuid_targets_clones_and_foreign_entries_in_case_insensitive_groups() {
    use akm_core::model::META_ID;
    use keepass::db::fields;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    let mut db = fast_database();
    let mut root = db.root_mut();
    let mut group = root.add_group();
    group.name = "api keys".into();
    let mut first = group.add_entry();
    first.set_unprotected(fields::TITLE, "Original");
    first.set_protected(fields::PASSWORD, "original-secret");
    first.set_unprotected(META_ID, "same-cloned-metadata");
    let first_id = first.id().to_string();
    let mut cloned = group.add_entry();
    cloned.set_unprotected(fields::TITLE, "Clone");
    cloned.set_protected(fields::PASSWORD, "clone-secret");
    cloned.set_unprotected(META_ID, "same-cloned-metadata");
    let clone_id = cloned.id().to_string();
    let mut foreign = group.add_entry();
    foreign.set_unprotected(fields::TITLE, "Foreign");
    foreign.set_protected(fields::PASSWORD, "foreign-secret");
    let foreign_id = foreign.id().to_string();
    write_database(&path, &db, PASSWORD);

    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    assert_eq!(vault.get_by_id(&clone_id).unwrap().title, "Clone");
    vault
        .update(
            &clone_id,
            ApiKeyInput::new("Updated clone", "updated-secret"),
        )
        .unwrap();
    assert_eq!(secret_of(&vault, &first_id), "original-secret");
    vault
        .update(
            &foreign_id,
            ApiKeyInput::new("Updated foreign", "updated-foreign-secret"),
        )
        .unwrap();
    vault.remove(&clone_id).unwrap();
    assert!(vault.get_by_id(&clone_id).is_err());
    vault
        .add(ApiKeyInput::new("Added", "added-secret"))
        .unwrap();
    assert_eq!(vault.list().len(), 3);
    drop(vault);
    let reopened = Vault::open(&path, PASSWORD).unwrap();
    assert_eq!(secret_of(&reopened, &foreign_id), "updated-foreign-secret");
    let db = keepass::Database::open(
        &mut std::fs::File::open(path).unwrap(),
        keepass::DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
    // The removed clone sits in the Recycle Bin next to the original group.
    assert_eq!(db.root().groups().count(), 2);
    assert_eq!(db.recycle_bin().unwrap().entries().count(), 1);
}

#[test]
fn two_snapshots_can_open_and_the_stale_writer_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    write_database(&path, &fast_database(), PASSWORD);
    let mut first = Vault::open(&path, PASSWORD).unwrap();
    let mut second = Vault::open(&path, PASSWORD).unwrap();
    first
        .add(ApiKeyInput::new("First", "first-secret"))
        .unwrap();
    assert!(matches!(
        second.add(ApiKeyInput::new("Second", "second-secret")),
        Err(Error::VaultChangedOnDisk)
    ));
    assert!(second.list().is_empty());
    assert_eq!(
        Vault::open(&path, PASSWORD).unwrap().list()[0].title,
        "First"
    );
}

#[test]
fn create_requires_twelve_unicode_characters_before_any_files_are_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not-created").join("vault.kdbx");
    for password in ["", "12345678901", "🔑🔑🔑🔑🔑🔑🔑🔑🔑🔑🔑"] {
        assert!(Vault::create(&path, password, None).is_err());
        assert!(!path.parent().unwrap().exists());
    }
    let vault = Vault::create(&path, "🔑🔑🔑🔑🔑🔑🔑🔑🔑🔑🔑🔑", None).unwrap();
    assert!(vault.list().is_empty());
    let old_path = dir.path().join("short-existing.kdbx");
    write_database(&old_path, &fast_database(), "short");
    assert!(Vault::open(&old_path, "short").is_ok());
}

#[test]
fn preview_is_secret_free_and_matches_the_applied_import() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    write_database(&path, &fast_database(), PASSWORD);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let mut first = sample();
    first.env_var = Some("FIRST".into());
    vault.add(first).unwrap();
    let entries = vec![
        ("FIRST".into(), "replacement-secret".into()),
        ("SECOND".into(), "another-secret".into()),
    ];
    let preview: akm_core::ImportPreview = vault
        .preview_import_env(&entries, Some("demo"), Some("dev"))
        .unwrap();
    assert_eq!(
        (preview.added, preview.updated, preview.unchanged),
        (1, 1, 0)
    );
    assert_eq!(preview.changes[0].name, "FIRST");
    assert_eq!(preview.changes[0].title, "OpenAI");
    assert_eq!(preview.changes[0].action, "update");
    let serialized = serde_json::to_string(&preview).unwrap();
    assert!(!serialized.contains("replacement-secret"));
    assert!(!serialized.contains("another-secret"));
    assert_eq!(vault.list().len(), 1);
    assert_eq!(
        vault
            .import_env(&entries, Some("demo"), Some("dev"))
            .unwrap(),
        (1, 1)
    );
    let preview = vault
        .preview_import_env(&entries, Some("demo"), Some("dev"))
        .unwrap();
    assert_eq!(
        (preview.added, preview.updated, preview.unchanged),
        (0, 0, 2)
    );
    assert!(preview
        .changes
        .iter()
        .all(|change| change.action == "unchanged"));
}

#[test]
fn extraction_requires_exact_scope_or_explicit_all_and_rejects_unsafe_entries() {
    use akm_core::model::{META_ENVIRONMENT, META_ENV_VAR, META_PROJECT};
    use keepass::db::fields;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    let mut db = fast_database();
    let mut root = db.root_mut();
    let mut group = root.add_group();
    group.name = "API Keys".into();
    for (project, name, secret) in [
        ("safe", "TOKEN", "safe-secret"),
        ("bad-name", "BAD-NAME", "fake"),
        ("reserved", "NODE_OPTIONS", "fake"),
        ("empty", "TOKEN", ""),
        ("nul", "TOKEN", "has\0nul"),
    ] {
        let mut entry = group.add_entry();
        entry.set_unprotected(fields::TITLE, "Readable title");
        entry.set_protected(fields::PASSWORD, secret);
        entry.set_unprotected(META_ENV_VAR, name);
        entry.set_unprotected(META_PROJECT, project);
        entry.set_unprotected(META_ENVIRONMENT, "dev");
    }
    write_database(&path, &db, PASSWORD);
    let vault = Vault::open(&path, PASSWORD).unwrap();
    for (project, environment, all) in [
        (None, None, false),
        (Some("safe"), None, false),
        (None, Some("dev"), false),
        (Some(""), Some("dev"), false),
        (Some("safe"), Some("dev"), true),
    ] {
        assert!(vault.plan_env(project, environment, all).is_err());
    }
    let plan = vault.plan_env(Some("safe"), Some("dev"), false).unwrap();
    assert_eq!(plan.names().collect::<Vec<_>>(), ["TOKEN"]);
    let proof = vault
        .disclose_env(EnvDisclosure::Export, &plan, Some("test"))
        .unwrap();
    assert_eq!(
        vault.env_vars(&plan, &proof).unwrap(),
        vec![("TOKEN".into(), "safe-secret".into())]
    );
    assert!(vault
        .plan_env(Some("SAFE"), Some("dev"), false)
        .unwrap()
        .is_empty());
    for project in ["bad-name", "reserved", "empty", "nul"] {
        assert!(
            vault.plan_env(Some(project), Some("dev"), false).is_err(),
            "accepted {project}"
        );
    }
    assert!(vault.plan_env(None, None, true).is_err());
}

#[test]
fn damaged_audit_log_does_not_turn_a_committed_mutation_into_a_retryable_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    write_database(&path, &fast_database(), PASSWORD);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    std::fs::write(
        vault.audit_log().create_journal_path().unwrap(),
        b"damaged audit JSON\n",
    )
    .unwrap();
    let created = vault.add(sample()).unwrap();
    assert_eq!(Vault::open(&path, PASSWORD).unwrap().list().len(), 1);
    assert_eq!(vault.list(), vec![created.clone()]);
    let warnings = vault.take_warnings();
    assert_eq!(warnings.len(), 1);
    assert!(warnings[0].contains("audit"));
    assert!(vault.take_warnings().is_empty());
    let mut changed = sample();
    changed.notes = Some("a real change; an identical update saves nothing".into());
    vault.update(&created.id, changed).unwrap();
    assert_eq!(vault.take_warnings().len(), 1);
    vault
        .import_env(
            &[("OTHER".into(), "fake-secret".into())],
            Some("demo"),
            Some("dev"),
        )
        .unwrap();
    assert_eq!(vault.take_warnings().len(), 1);
    vault.remove(&created.id).unwrap();
    assert_eq!(vault.take_warnings().len(), 1);
    assert_eq!(Vault::open(&path, PASSWORD).unwrap().list().len(), 1);
}

#[cfg(windows)]
#[test]
fn blocked_rename_rolls_back_memory_cleans_temps_and_keeps_known_good_backup() {
    use std::os::windows::fs::OpenOptionsExt;
    for operation in ["add", "update", "remove", "import"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.kdbx");
        write_database(&path, &fast_database(), PASSWORD);
        let mut vault = Vault::open(&path, PASSWORD).unwrap();
        let original = vault.add(sample()).unwrap();
        let backup = path.with_file_name("vault.kdbx.bak");
        let known_good = std::fs::read(&backup).unwrap();
        let before = std::fs::read(&path).unwrap();
        let holder = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let result = match operation {
            "add" => vault
                .add(ApiKeyInput::new("Another", "new-secret"))
                .map(|_| ()),
            "update" => vault
                .update(&original.id, ApiKeyInput::new("Changed", "changed-secret"))
                .map(|_| ()),
            "remove" => vault.remove(&original.id).map(|_| ()),
            "import" => vault
                .import_env(
                    &[
                        ("OPENAI".into(), "changed-secret".into()),
                        ("NEW".into(), "new-secret".into()),
                    ],
                    Some("demo"),
                    Some("dev"),
                )
                .map(|_| ()),
            _ => unreachable!(),
        };
        assert!(result.is_err(), "{operation}");
        assert_eq!(vault.list(), vec![original.clone()], "{operation}");
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_eq!(std::fs::read(&backup).unwrap(), known_good);
        assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp")));
        drop(holder);
        vault
            .add(ApiKeyInput::new("Later", "later-secret"))
            .unwrap();
        assert_eq!(
            Vault::open(&path, PASSWORD)
                .unwrap()
                .get_by_id(&original.id)
                .unwrap(),
            original
        );
    }
}

#[test]
fn older_formats_require_explicit_upgrade_and_preserve_the_database_and_kdf() {
    const FIXTURE_PASSWORD: &str = PASSWORD;
    for bytes in [
        include_bytes!("fixtures/kdbx3.kdbx").as_slice(),
        include_bytes!("fixtures/kdbx40.kdbx").as_slice(),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.kdbx");
        std::fs::write(&path, bytes).unwrap();
        let mut expected = keepass::Database::parse(
            bytes,
            keepass::DatabaseKey::new().with_password(FIXTURE_PASSWORD),
        )
        .unwrap();
        let mut vault = Vault::open(&path, FIXTURE_PASSWORD).unwrap();
        let original = vault.list();
        assert!(vault.needs_upgrade());
        assert!(matches!(vault.save(), Err(Error::UpgradeRequired)));
        assert!(matches!(vault.add(sample()), Err(Error::UpgradeRequired)));
        assert!(matches!(
            vault.update("missing", sample()),
            Err(Error::UpgradeRequired)
        ));
        assert!(matches!(
            vault.remove("missing"),
            Err(Error::UpgradeRequired)
        ));
        assert!(matches!(
            vault.import_env(
                &[("TOKEN".into(), "fake".into())],
                Some("demo"),
                Some("dev")
            ),
            Err(Error::UpgradeRequired)
        ));
        assert_eq!(vault.list(), original);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        vault.upgrade().unwrap();
        assert!(!vault.needs_upgrade());
        assert_eq!(
            std::fs::read(path.with_file_name("vault.kdbx.bak")).unwrap(),
            bytes
        );
        expected.config.version = keepass::config::DatabaseVersion::KDB4(1);
        let saved = keepass::Database::parse(
            &std::fs::read(&path).unwrap(),
            keepass::DatabaseKey::new().with_password(FIXTURE_PASSWORD),
        )
        .unwrap();
        assert_eq!(saved, expected);
        let upgraded = std::fs::read(&path).unwrap();
        vault.upgrade().unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), upgraded);
        vault.add(sample()).unwrap();
    }
}

#[test]
fn find_prefers_native_uuid_then_exact_title_before_substring_search() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    write_database(&path, &fast_database(), PASSWORD);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let first = vault
        .add(ApiKeyInput::new("Exact", "first-secret"))
        .unwrap();
    vault
        .add(ApiKeyInput::new("Exact suffix", "suffix-secret"))
        .unwrap();
    assert_eq!(vault.find("Exact").unwrap().id, first.id);
    assert_eq!(vault.find(&first.id).unwrap().id, first.id);
    assert_eq!(
        vault.get_by_id(&first.id.to_uppercase()).unwrap().id,
        first.id
    );
    vault
        .add(ApiKeyInput::new(&first.id, "uuid-title-secret"))
        .unwrap();
    assert_eq!(vault.find(&first.id).unwrap().id, first.id);
    vault
        .add(ApiKeyInput::new("EXACT", "duplicate-title-secret"))
        .unwrap();
    assert!(matches!(vault.find("Exact"), Err(Error::Ambiguous(_, 2))));
    assert!(matches!(vault.find("exa"), Err(Error::Ambiguous(_, 3))));
    assert_eq!(vault.find("suffix").unwrap().title, "Exact suffix");
}

#[test]
fn reload_adopts_a_complete_external_snapshot_and_failed_reload_keeps_the_old_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    write_database(&path, &fast_database(), PASSWORD);
    let mut first = Vault::open(&path, PASSWORD).unwrap();
    let mut second = Vault::open(&path, PASSWORD).unwrap();
    let created = first.add(sample()).unwrap();
    second.reload().unwrap();
    assert_eq!(second.list(), vec![created.clone()]);
    let good = std::fs::read(&path).unwrap();
    std::fs::write(&path, b"truncated external vault").unwrap();
    assert!(second.reload().is_err());
    assert_eq!(second.list(), vec![created]);
    std::fs::write(&path, good).unwrap();
    second
        .add(ApiKeyInput::new("After reload", "reload-secret"))
        .unwrap();
    assert_eq!(Vault::open(&path, PASSWORD).unwrap().list().len(), 2);
}

#[test]
fn scope_metadata_and_explicit_variable_names_are_not_silently_trimmed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    write_database(&path, &fast_database(), PASSWORD);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let mut input = sample();
    input.project = Some("demo ".into());
    input.environment = Some("dev ".into());
    let original = vault.add(input).unwrap();
    assert_eq!(
        vault
            .import_env(
                &[("OPENAI".into(), "new-secret".into())],
                Some("demo"),
                Some("dev")
            )
            .unwrap(),
        (1, 0)
    );
    assert_eq!(secret_of(&vault, &original.id), "sk-test-123456789");
    assert_eq!(
        vault.get_by_id(&original.id).unwrap().project.as_deref(),
        Some("demo ")
    );
}

#[test]
fn deleting_one_attachment_owner_preserves_other_entries_and_their_binary_data() {
    use keepass::db::{fields, Value};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    let mut db = fast_database();
    let mut root = db.root_mut();
    let mut group = root.add_group();
    group.name = "API Keys".into();
    for (title, body) in [
        ("First", "first attachment"),
        ("Second", "second attachment"),
    ] {
        let mut entry = group.add_entry();
        entry.set_unprotected(fields::TITLE, title);
        entry.set_protected(fields::PASSWORD, "fake-secret");
        entry.add_attachment("file.txt", Value::protected(body.as_bytes().to_vec()));
    }
    write_database(&path, &db, PASSWORD);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let first = vault.find("First").unwrap();
    vault.remove(&first.id).unwrap();
    assert_eq!(vault.list().len(), 1);
    let saved = keepass::Database::parse(
        &std::fs::read(&path).unwrap(),
        keepass::DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
    let root = saved.root();
    let group = root.group_by_name("API Keys").unwrap();
    let entry = group.entry_by_name("Second").unwrap();
    let attachment = entry.attachment_by_name("file.txt").unwrap();
    assert_eq!(attachment.get(), b"second attachment");
    assert!(attachment.is_protected());
    vault.save().unwrap();
}

#[test]
fn removing_an_api_key_preserves_shared_and_history_only_attachments() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    std::fs::write(&path, include_bytes!("fixtures/kdbx40.kdbx")).unwrap();
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    vault.upgrade().unwrap();
    let foreign = vault.find("Foreign fixture").unwrap();
    vault.remove(&foreign.id).unwrap();
    let saved = keepass::Database::parse(
        &std::fs::read(&path).unwrap(),
        keepass::DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
    let root = saved.root();
    let group = root.group_by_name("Unrelated").unwrap();
    let other = group.entry_by_name("Other entry").unwrap();
    assert_eq!(
        other.attachment_by_name("shared.txt").unwrap().get(),
        b"attachment fixture bytes"
    );
    let historical = group.entry_by_name("Historical attachment owner").unwrap();
    assert_eq!(historical.attachments().count(), 0);
    let history = historical.historical(0).unwrap();
    let attachment = history.attachment_by_name("history-only.txt").unwrap();
    assert_eq!(attachment.get(), b"attachment fixture bytes");
    assert!(attachment.is_protected());
    vault.save().unwrap();
}

#[test]
fn retains_the_five_latest_encrypted_originals_and_a_legacy_backup() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    write_database(&path, &fast_database(), PASSWORD);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let mut originals = vec![std::fs::read(&path).unwrap()];
    let first = vault.add(sample()).unwrap();
    for index in 0..7 {
        originals.push(std::fs::read(&path).unwrap());
        let mut input = sample();
        input.secret = format!("fake-secret-version-{index}").into();
        vault.update(&first.id, input).unwrap();
    }
    let mut backups: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("vault.kdbx.bak.")
        })
        .collect();
    backups.sort();
    assert_eq!(backups.len(), 5);
    for (backup, original) in backups
        .iter()
        .zip(originals.iter().skip(originals.len() - 5))
    {
        assert_eq!(&std::fs::read(backup).unwrap(), original);
        assert!(Vault::open(backup, PASSWORD).is_ok());
    }
    assert_eq!(
        std::fs::read(path.with_file_name("vault.kdbx.bak")).unwrap(),
        *originals.last().unwrap()
    );
}

#[test]
fn an_exclusive_writer_lock_does_not_block_opening_a_snapshot() {
    use fs2::FileExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    write_database(&path, &fast_database(), PASSWORD);
    let lock_path = path.with_file_name("vault.kdbx.lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    assert!(matches!(vault.add(sample()), Err(Error::Locked)));
    assert!(vault.list().is_empty());
    drop(lock);
    vault.add(sample()).unwrap();
    assert_eq!(vault.list().len(), 1);
}

#[cfg(windows)]
#[test]
fn failed_upgrade_is_transactional_and_a_postcommit_audit_error_is_a_warning() {
    use std::os::windows::fs::OpenOptionsExt;
    for bytes in [
        include_bytes!("fixtures/kdbx3.kdbx").as_slice(),
        include_bytes!("fixtures/kdbx40.kdbx").as_slice(),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.kdbx");
        std::fs::write(&path, bytes).unwrap();
        let mut vault = Vault::open(&path, PASSWORD).unwrap();
        let original = vault.list();
        let holder = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(vault.upgrade().is_err());
        assert!(vault.needs_upgrade());
        assert_eq!(vault.list(), original);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp")));
        drop(holder);
        std::fs::write(
            vault.audit_log().create_journal_path().unwrap(),
            b"damaged log\n",
        )
        .unwrap();
        vault.upgrade().unwrap();
        assert!(!vault.needs_upgrade());
        assert_eq!(vault.take_warnings().len(), 1);
        assert_eq!(
            std::fs::read(path.with_file_name("vault.kdbx.bak")).unwrap(),
            bytes
        );
    }
}

#[test]
fn creates_vaults_with_a_hardened_kdf() {
    use keepass::config::{DatabaseVersion, KdfConfig};
    use keepass::{Database, DatabaseKey};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");

    Vault::create(&path, PASSWORD, None).unwrap();

    let mut file = std::fs::File::open(&path).unwrap();
    let db = Database::open(&mut file, DatabaseKey::new().with_password(PASSWORD)).unwrap();

    assert!(matches!(db.config.version, DatabaseVersion::KDB4(1)));

    match db.config.kdf_config {
        KdfConfig::Argon2id {
            iterations,
            memory,
            parallelism,
            ..
        } => {
            assert_eq!(iterations, 3);
            assert_eq!(memory, 64 * 1024 * 1024);
            assert_eq!(parallelism, 4);
        }
        other => panic!("expected Argon2id, got {other:?}"),
    }
}

#[test]
fn derives_safe_environment_variable_names() {
    assert_eq!(derive_env_var("OpenAI — Personal"), "OPENAI_PERSONAL");
    assert_eq!(derive_env_var("  GitHub  "), "GITHUB");
    assert_eq!(derive_env_var("9lives"), "KEY_9LIVES");
    assert_eq!(derive_env_var("***"), "API_KEY");

    assert!(is_valid_env_var("OPENAI_API_KEY"));
    assert!(!is_valid_env_var("openai"));
    assert!(!is_valid_env_var("1KEY"));
    assert!(!is_valid_env_var("BAD-NAME"));
}
