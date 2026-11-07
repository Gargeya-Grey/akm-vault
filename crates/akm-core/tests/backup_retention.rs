use akm_core::{ApiKeyInput, Vault};
use keepass::{config::KdfConfig, Database, DatabaseKey};
use std::{
    fs,
    path::{Path, PathBuf},
};

const PASSWORD: &str = "fake-backup-retention-password";

fn dated_files(directory: &Path) -> Vec<PathBuf> {
    fs::read_dir(directory)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("vault.kdbx.bak.")
        })
        .collect()
}

fn future_backups(directory: &Path, empty_bytes: &[u8]) {
    for index in 0..5 {
        let name = format!(
            "vault.kdbx.bak.29990101T00000{index}000000000Z_{}.kdbx",
            uuid::Uuid::new_v4()
        );
        fs::write(directory.join(name), empty_bytes).unwrap();
    }
}

fn fixture(path: &Path) -> Vec<u8> {
    // A fast test-only KDF. Production creation parameters are checked elsewhere.
    let mut db = Database::new();
    db.config.kdf_config = KdfConfig::Aes { rounds: 10 };
    db.save(
        &mut fs::File::create(path).unwrap(),
        DatabaseKey::new().with_password(PASSWORD),
    )
    .unwrap();
    fs::read(path).unwrap()
}

#[test]
fn protected_original_survives_future_names_and_failed_convenience_refresh() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    let empty = fixture(&path);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let key = vault
        .add(ApiKeyInput::new(
            "Fake deleted key",
            "fake-only-retained-value",
        ))
        .unwrap();
    let before_delete = fs::read(&path).unwrap();
    for p in dated_files(dir.path()) {
        fs::remove_file(p).unwrap();
    }
    future_backups(dir.path(), &empty);
    fs::remove_file(dir.path().join("vault.kdbx.bak")).unwrap();
    fs::create_dir(dir.path().join("vault.kdbx.bak")).unwrap();
    vault.remove(&key.id).unwrap();
    assert!(
        Vault::open(&path, PASSWORD).unwrap().list().is_empty(),
        "deletion stays a committed success"
    );
    let retained = dated_files(dir.path());
    assert_eq!(retained.len(), 5);
    let exact = retained
        .iter()
        .find(|p| fs::read(p).unwrap() == before_delete);
    assert!(
        exact.is_some(),
        "the verified immediate pre-deletion original must not be pruned by wall-clock names"
    );
    let recovered = Vault::open(exact.unwrap(), PASSWORD).unwrap();
    let found = recovered.get_by_id(&key.id).unwrap();
    let proof = recovered
        .disclose_key(akm_core::KeyDisclosure::Reveal, &found, Some("test"))
        .unwrap();
    assert_eq!(
        *recovered.secret(&key.id, &proof).unwrap(),
        "fake-only-retained-value"
    );
}

#[test]
fn protected_original_is_reselected_for_each_write_despite_future_names() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    let empty = fixture(&path);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    future_backups(dir.path(), &empty);
    for index in 0..3 {
        let original = fs::read(&path).unwrap();
        vault
            .add(ApiKeyInput::new(format!("Fake {index}"), "fake-only-value"))
            .unwrap();
        let retained = dated_files(dir.path());
        assert_eq!(retained.len(), 5);
        assert!(
            retained.iter().any(|p| fs::read(p).unwrap() == original),
            "write {index} must protect its own immediate original"
        );
    }
}
