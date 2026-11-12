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

/// Five files in the name format of an AKM backup, whose bytes hash to the value in the name
/// and whose timestamps sort after any real one. AKM did not write them, so it never deletes
/// them (core review 1, finding 1); they test that a name never decides what is pruned.
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
    let futures = dated_files(dir.path());
    vault.remove(&key.id).unwrap();
    assert!(
        Vault::open(&path, PASSWORD).unwrap().list().is_empty(),
        "deletion stays a committed success"
    );
    let retained = dated_files(dir.path());
    assert!(
        futures.iter().all(|future| retained.contains(future)),
        "files that AKM did not write are never pruned"
    );
    assert_eq!(retained.len(), futures.len() + 1);
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
    let futures = dated_files(dir.path());
    for index in 0..7 {
        let original = fs::read(&path).unwrap();
        vault
            .add(ApiKeyInput::new(format!("Fake {index}"), "fake-only-value"))
            .unwrap();
        let retained = dated_files(dir.path());
        assert!(futures.iter().all(|future| retained.contains(future)));
        assert_eq!(retained.len(), futures.len() + (index + 1).min(5));
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

/// Core review 1, finding 1 (FR-01), the reviewer's closing test: a file that AKM did not
/// write survives any number of saves, also when its name has AKM's format and its bytes
/// hash to the value in its name. Sentinels of every shape stay: a true hash (old and future
/// time), a wrong hash, a hard link to a file outside, a folder link and a folder. AKM still
/// keeps five dated backups of its own (contract 2), and the newest is the original of the
/// last save.
#[test]
fn a_foreign_file_with_a_true_hash_in_an_akm_name_survives_repeated_pruning() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    fixture(&path);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    let named = |time: &str, bytes: &[u8]| {
        dir.path().join(format!(
            "vault.kdbx.bak.{time}_{}_{}.kdbx",
            uuid::Uuid::new_v4(),
            sha256_hex(bytes)
        ))
    };
    let mut sentinels: Vec<(PathBuf, Vec<u8>)> = Vec::new();
    for (time, bytes) in [
        (
            "20200101T000000000000000Z",
            b"foreign, true hash, old".to_vec(),
        ),
        (
            "29990101T000000000000000Z",
            b"foreign, true hash, future".to_vec(),
        ),
        ("20200101T000001000000000Z", fs::read(&path).unwrap()),
    ] {
        let target = named(time, &bytes);
        fs::write(&target, &bytes).unwrap();
        sentinels.push((target, bytes));
    }
    let wrong = dir.path().join(format!(
        "vault.kdbx.bak.20200101T000002000000000Z_{}_{}.kdbx",
        uuid::Uuid::new_v4(),
        sha256_hex(b"other bytes")
    ));
    fs::write(&wrong, b"foreign, wrong hash").unwrap();
    sentinels.push((wrong, b"foreign, wrong hash".to_vec()));

    let outside = tempfile::tempdir().unwrap();
    let linked_bytes = b"foreign file reached through a hard link".to_vec();
    let linked_target = outside.path().join("user-file.kdbx");
    fs::write(&linked_target, &linked_bytes).unwrap();
    let hard_link = named("20200101T000003000000000Z", &linked_bytes);
    fs::hard_link(&linked_target, &hard_link).unwrap();
    sentinels.push((hard_link, linked_bytes.clone()));

    let folder = named("20200101T000004000000000Z", b"");
    fs::create_dir(&folder).unwrap();
    let folder_target = outside.path().join("linked-folder");
    fs::create_dir(&folder_target).unwrap();
    fs::write(folder_target.join("inside.txt"), b"keep me").unwrap();
    let junction = named("20200101T000005000000000Z", b"");
    let made = std::process::Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(&junction)
        .arg(&folder_target)
        .output()
        .unwrap();
    assert!(made.status.success(), "{made:?}");

    let mut last_original = Vec::new();
    for index in 0..9 {
        last_original = fs::read(&path).unwrap();
        vault
            .add(ApiKeyInput::new(format!("Fake {index}"), "fake-only-value"))
            .unwrap();
    }

    for (target, bytes) in &sentinels {
        assert_eq!(
            fs::read(target).as_deref().ok(),
            Some(bytes.as_slice()),
            "a foreign file must survive pruning: {}",
            target.display()
        );
    }
    assert_eq!(fs::read(&linked_target).unwrap(), linked_bytes);
    assert!(folder.is_dir());
    assert_eq!(fs::read(junction.join("inside.txt")).unwrap(), b"keep me");
    let foreign: Vec<&PathBuf> = sentinels
        .iter()
        .map(|(target, _)| target)
        .chain([&folder, &junction])
        .collect();
    let own: Vec<PathBuf> = dated_files(dir.path())
        .into_iter()
        .filter(|p| !foreign.contains(&p))
        .collect();
    assert_eq!(
        own.len(),
        5,
        "nine saves keep five backups of AKM's own: {own:?}"
    );
    assert!(
        own.iter().any(|p| fs::read(p).unwrap() == last_original),
        "the original of the last save is kept"
    );
    let _ = fs::remove_dir(&junction);
}

/// Core review 1, finding 1: the ledger is in the vault file, so a new session prunes the same
/// backups and still keeps five. A backup that another writer replaced with other bytes is no
/// longer AKM's: it leaves the ledger and is never deleted.
#[test]
fn a_new_session_prunes_by_the_ledger_in_the_file_and_a_replaced_backup_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("vault.kdbx");
    fixture(&path);
    let mut vault = Vault::open(&path, PASSWORD).unwrap();
    for index in 0..2 {
        vault
            .add(ApiKeyInput::new(format!("Fake {index}"), "fake-only-value"))
            .unwrap();
    }
    let mut first = dated_files(dir.path());
    first.sort();
    assert_eq!(first.len(), 2);
    let replaced = first[0].clone();
    fs::write(&replaced, b"another writer replaced this backup").unwrap();
    drop(vault);

    for session in 0..4 {
        let mut vault = Vault::open(&path, PASSWORD).unwrap();
        for index in 0..3 {
            vault
                .add(ApiKeyInput::new(
                    format!("Fake {session} {index}"),
                    "fake-only-value",
                ))
                .unwrap();
        }
    }
    assert_eq!(
        fs::read(&replaced).unwrap(),
        b"another writer replaced this backup"
    );
    let own: Vec<PathBuf> = dated_files(dir.path())
        .into_iter()
        .filter(|p| *p != replaced)
        .collect();
    assert_eq!(own.len(), 5, "{own:?}");
}
