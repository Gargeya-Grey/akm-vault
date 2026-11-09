//! Input that must be refused before a save, with a vault that stays as it was (FR-16).
//! Every test uses a fake vault and literal expected values.

use akm_core::{ApiKeyInput, KeyDisclosure, Secret, Vault};

const PASSWORD: &str = "correct horse battery staple";

fn vault() -> (tempfile::TempDir, std::path::PathBuf, Vault) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.kdbx");
    let vault = Vault::create(&path, PASSWORD, None).unwrap();
    (directory, path, vault)
}

fn reveal(vault: &Vault, id: &str) -> String {
    let key = vault.get_by_id(id).unwrap();
    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &key, Some("test"))
        .unwrap();
    (*vault.secret(id, &proof).unwrap()).clone()
}

/// A vault file that another program wrote, with a key whose stored secret has a NUL.
fn vault_with_stored_nul() -> (tempfile::TempDir, Vault, String) {
    use keepass::db::fields;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("vault.kdbx");
    let mut db = keepass::Database::new();
    db.config.kdf_config = keepass::config::KdfConfig::Aes { rounds: 10 };
    let mut root = db.root_mut();
    let mut group = root.add_group();
    group.name = "API Keys".into();
    let mut entry = group.add_entry();
    entry.set_unprotected(fields::TITLE, "Stored with NUL");
    entry.set_protected(fields::PASSWORD, "fake\0stored");
    entry.set_unprotected("AKM_PROJECT", "demo");
    entry.set_unprotected("AKM_ENV", "dev");
    db.save(
        &mut std::fs::File::create(&path).unwrap(),
        keepass::DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
    let vault = Vault::open(&path, PASSWORD).unwrap();
    let id = vault.list().remove(0).id;
    (directory, vault, id)
}

/// FR-16: add, rotate, edit and import refuse a secret with a NUL before any save. The file
/// is byte for byte what it was, and the old secret still works.
#[test]
fn a_secret_with_a_nul_is_refused_by_add_rotate_edit_and_import_and_the_vault_is_unchanged() {
    let (_directory, path, mut vault) = vault();
    let key = vault
        .add(ApiKeyInput::new("Keeper", "fake-old-secret"))
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    let count = vault.list().len();

    let text = vault
        .add(ApiKeyInput::new("Bad one", "fake\0secret"))
        .unwrap_err()
        .to_string();
    assert!(text.contains("NUL"), "{text}");
    assert!(!text.contains("fake"), "{text}");

    let text = vault
        .rotate(&key.id, Secret::new("fake\0rotated"))
        .unwrap_err()
        .to_string();
    assert!(text.contains("NUL"), "{text}");

    let mut edit = vault.edit_input(&key.id).unwrap();
    edit.secret = Secret::new("fake\0edited");
    let text = vault.update(&key.id, edit).unwrap_err().to_string();
    assert!(text.contains("NUL"), "{text}");

    for entries in [
        vec![("NUL_KEY".to_string(), "fake\0value".to_string())],
        vec![("KEEPER".to_string(), "a\0b".to_string())],
    ] {
        let text = vault
            .import_env(&entries, Some("demo"), Some("dev"))
            .unwrap_err()
            .to_string();
        assert!(text.contains("NUL"), "{text}");
    }

    assert_eq!(std::fs::read(&path).unwrap(), before, "no byte changed");
    assert_eq!(vault.list().len(), count);
    assert_eq!(reveal(&vault, &key.id), "fake-old-secret");
    let reopened = Vault::open(&path, PASSWORD).unwrap();
    assert_eq!(reveal(&reopened, &key.id), "fake-old-secret");
}

/// FR-16: a text field with a NUL cannot be stored either.
#[test]
fn a_nul_in_any_text_field_is_refused_at_input() {
    let (_directory, path, mut vault) = vault();
    let before = std::fs::read(&path).unwrap();
    type Set = fn(&mut ApiKeyInput);
    let cases: [(&str, Set); 9] = [
        ("title", |input| input.title = "Bad\0title".into()),
        ("username", |input| input.username = Some("a\0b".into())),
        ("issuer", |input| input.issuer = Some("a\0b".into())),
        ("project", |input| input.project = Some("a\0b".into())),
        ("environment", |input| {
            input.environment = Some("a\0b".into())
        }),
        ("url", |input| input.url = Some("https://a\0b".into())),
        ("notes", |input| input.notes = Some("a\0b".into())),
        ("scope", |input| input.scopes = vec!["a\0b".into()]),
        ("tag", |input| input.tags = vec!["a\0b".into()]),
    ];
    for (label, set) in cases {
        let mut input = ApiKeyInput::new("Fine title", "fake-secret");
        set(&mut input);
        let text = vault.add(input).unwrap_err().to_string();
        assert!(
            text.contains(label) && text.contains("NUL"),
            "{label}: {text}"
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

/// FR-16: a secret that was stored with a NUL before this check stays viewable. The key
/// cannot be used by `run` or `export` (the plan refuses it), its metadata can be edited,
/// and a new secret is checked.
#[test]
fn a_stored_secret_with_a_nul_stays_viewable_and_its_key_stays_editable() {
    let (_directory, mut vault, id) = vault_with_stored_nul();
    assert_eq!(reveal(&vault, &id), "fake\0stored");

    let text = vault
        .plan_env(Some("demo"), Some("dev"), false)
        .unwrap_err()
        .to_string();
    assert!(text.contains("NUL"), "{text}");
    assert!(text.contains("Stored with NUL"), "{text}");
    assert!(!text.contains("fake"), "{text}");

    let mut edit = vault.edit_input(&id).unwrap();
    edit.notes = Some("a note".into());
    vault
        .update(&id, edit)
        .expect("a metadata edit keeps the secret as it is");
    assert_eq!(reveal(&vault, &id), "fake\0stored");
    assert_eq!(
        vault.get_by_id(&id).unwrap().notes.as_deref(),
        Some("a note")
    );

    vault
        .rotate(&id, Secret::new("fake-clean-secret"))
        .expect("a clean secret replaces it");
    assert_eq!(reveal(&vault, &id), "fake-clean-secret");
}
