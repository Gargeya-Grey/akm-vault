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

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// Five backups that AKM owns by its own rule (the name holds the SHA-256 of the bytes) and
/// whose timestamps sort after any real one.
fn future_backups(directory: &Path, empty_bytes: &[u8]) {
    for index in 0..5 {
        let name = format!(
            "vault.kdbx.bak.29990101T00000{index}000000000Z_{}_{}.kdbx",
            uuid::Uuid::new_v4(),
            sha256_hex(empty_bytes)
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

/// FR-01: pruning deletes only a file that proves it is an AKM backup: the name holds the
/// SHA-256 of the file's bytes and the bytes hash to that value.
#[test]
fn pruning_deletes_only_backups_that_prove_they_are_owned() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    fixture(&path);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();

    let old_format = |tag: u8| {
        format!(
            "vault.kdbx.bak.20200101T0000{tag:02}000000000Z_{}.kdbx",
            uuid::Uuid::new_v4()
        )
    };
    let new_format = |tag: u8, hash: &str| {
        format!(
            "vault.kdbx.bak.20200102T0000{tag:02}000000000Z_{}_{hash}.kdbx",
            uuid::Uuid::new_v4()
        )
    };
    let wrong_hash = sha256_hex(b"some other bytes");
    let mut sentinels: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for (name, bytes) in [
        // The old name pattern: not self-certifying, so never owned.
        (old_format(1), b"foreign file, old pattern".to_vec()),
        (old_format(2), b"another foreign file".to_vec()),
        // The new pattern with a hash that the bytes do not have.
        (
            new_format(3, &wrong_hash),
            b"foreign file, wrong hash".to_vec(),
        ),
        // The new pattern with an upper-case hash is not the format AKM writes.
        (
            new_format(4, &sha256_hex(b"upper").to_uppercase()),
            b"upper".to_vec(),
        ),
        // The new pattern with a short hash.
        (new_format(5, "abc123"), b"short hash".to_vec()),
    ] {
        let target = dir.path().join(name);
        fs::write(&target, &bytes).unwrap();
        sentinels.push((target, bytes));
    }
    // A directory that carries a genuine-looking name is not a file AKM wrote.
    let directory_sentinel = dir.path().join(new_format(6, &sha256_hex(b"")));
    fs::create_dir(&directory_sentinel).unwrap();

    for index in 0..9 {
        vault
            .add(ApiKeyInput::new(format!("Fake {index}"), "fake-only-value"))
            .unwrap();
    }

    for (target, bytes) in &sentinels {
        assert_eq!(
            &fs::read(target).expect("a foreign file must survive pruning"),
            bytes,
            "{}",
            target.display()
        );
    }
    assert!(directory_sentinel.is_dir());
    let owned: Vec<PathBuf> = dated_files(dir.path())
        .into_iter()
        .filter(|p| {
            let name = p.file_name().unwrap().to_string_lossy().into_owned();
            p.is_file()
                && name.rsplit_once('_').is_some_and(|(_, tail)| {
                    tail.strip_suffix(".kdbx").is_some_and(|hash| {
                        hash.len() == 64 && hash == sha256_hex(&fs::read(p).unwrap())
                    })
                })
        })
        .collect();
    assert_eq!(
        owned.len(),
        5,
        "nine saves keep five owned backups: {owned:?}"
    );
}
