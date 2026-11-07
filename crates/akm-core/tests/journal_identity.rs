use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Barrier};

use akm_core::audit::{AuditLog, AuditState};
use akm_core::journal::{Identity, JournalRoot, NoticeKind, VaultEvidence};
use sha2::{Digest, Sha256};

struct Fixture {
    directory: tempfile::TempDir,
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("journal");
        Self { directory, root }
    }

    fn vault(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.directory.path().join(name);
        fs::write(&path, bytes).unwrap();
        path
    }

    fn log(&self, path: &Path, bytes: &[u8], vault_id: &str) -> AuditLog {
        let sha = hex::encode(Sha256::digest(bytes));
        AuditLog::new(
            JournalRoot::at(&self.root),
            VaultEvidence::unlocked(path, Some(sha), vault_id.to_string()),
        )
    }

    fn locked(&self, path: &Path) -> AuditLog {
        AuditLog::new(JournalRoot::at(&self.root), VaultEvidence::from_file(path))
    }
}

fn actions(log: &AuditLog) -> Vec<String> {
    log.read(usize::MAX)
        .unwrap()
        .into_iter()
        .map(|entry| entry.action)
        .collect()
}

#[test]
fn the_same_vault_keeps_one_journal_across_sessions() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    f.log(&path, b"fake-one", "root-a")
        .append("first", None, None)
        .unwrap();
    f.log(&path, b"fake-one", "root-a")
        .append("second", None, None)
        .unwrap();
    assert_eq!(actions(&f.locked(&path)), ["first", "second"]);
}

#[test]
fn a_changed_vault_with_the_same_root_keeps_its_journal() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    f.log(&path, b"fake-one", "root-a")
        .append("first", None, None)
        .unwrap();
    fs::write(&path, b"fake-two").unwrap();
    f.log(&path, b"fake-two", "root-a")
        .append("second", None, None)
        .unwrap();
    assert_eq!(
        actions(&f.log(&path, b"fake-two", "root-a")),
        ["first", "second"]
    );
}

#[test]
fn a_renamed_vault_follows_its_journal() {
    let f = Fixture::new();
    let old = f.vault("a.kdbx", b"fake-one");
    let log = f.log(&old, b"fake-one", "root-a");
    log.append("first", None, None).unwrap();
    let journal = log.journal_path().unwrap().unwrap();
    let new = f.directory.path().join("renamed.kdbx");
    fs::rename(&old, &new).unwrap();
    let moved = f.log(&new, b"fake-one", "root-a");
    moved.append("second", None, None).unwrap();
    assert_eq!(moved.journal_path().unwrap().unwrap(), journal);
    assert_eq!(actions(&moved), ["first", "second"]);
}

#[test]
fn a_copy_never_shares_the_journal_of_its_source() {
    let f = Fixture::new();
    let source = f.vault("a.kdbx", b"fake-one");
    let log = f.log(&source, b"fake-one", "root-a");
    log.append("first", None, None).unwrap();
    let clone = f.directory.path().join("clone.kdbx");
    fs::copy(&source, &clone).unwrap();
    let copy = f.log(&clone, b"fake-one", "root-a");
    assert_eq!(copy.snapshot(10).unwrap().state, AuditState::Absent);
    copy.append("on-copy", None, None).unwrap();
    assert_ne!(
        copy.journal_path().unwrap().unwrap(),
        log.journal_path().unwrap().unwrap()
    );
    assert_eq!(actions(&copy), ["on-copy"]);
    assert_eq!(actions(&log), ["first"]);
}

#[test]
fn a_replacement_at_the_same_path_starts_a_new_journal_and_leaves_the_old_bytes() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    let old = f.log(&path, b"fake-one", "root-a");
    old.append("first", None, None).unwrap();
    let old_journal = old.journal_path().unwrap().unwrap();
    let before = fs::read(&old_journal).unwrap();
    fs::write(&path, b"fake-other").unwrap();
    let new = f.log(&path, b"fake-other", "root-b");
    new.append("fresh", None, None).unwrap();
    assert_ne!(new.journal_path().unwrap().unwrap(), old_journal);
    assert_eq!(actions(&new), ["fresh"]);
    assert_eq!(fs::read(&old_journal).unwrap(), before);
    let kinds: Vec<NoticeKind> = new
        .snapshot(10)
        .unwrap()
        .notices
        .iter()
        .map(|notice| notice.kind)
        .collect();
    assert_eq!(kinds, [NoticeKind::EarlierJournal]);
}

#[test]
fn restoring_an_earlier_version_is_recorded_in_the_same_journal() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    f.log(&path, b"fake-one", "root-a")
        .append("first", None, None)
        .unwrap();
    fs::write(&path, b"fake-two").unwrap();
    f.log(&path, b"fake-two", "root-a")
        .append("second", None, None)
        .unwrap();
    fs::write(&path, b"fake-one").unwrap();
    let restored = f.log(&path, b"fake-one", "root-a");
    restored.append("third", None, None).unwrap();
    assert_eq!(
        actions(&restored),
        ["first", "second", "vault-rolled-back", "third"]
    );
}

#[test]
fn two_vaults_keep_two_journals() {
    let f = Fixture::new();
    let a = f.vault("a.kdbx", b"fake-a");
    let b = f.vault("b.kdbx", b"fake-b");
    let log_a = f.log(&a, b"fake-a", "root-a");
    let log_b = f.log(&b, b"fake-b", "root-b");
    log_a.append("on-a", None, None).unwrap();
    log_b.append("on-b", None, None).unwrap();
    assert_ne!(
        log_a.journal_path().unwrap().unwrap(),
        log_b.journal_path().unwrap().unwrap()
    );
    assert_eq!(actions(&log_a), ["on-a"]);
    assert_eq!(actions(&log_b), ["on-b"]);
}

#[test]
fn an_unclear_identity_is_read_only_and_says_so() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    let log = f.log(&path, b"fake-one", "root-a");
    log.append("first", None, None).unwrap();
    let journal = log.journal_path().unwrap().unwrap();
    let before = fs::read(&journal).unwrap();
    fs::write(&path, b"fake-unseen").unwrap();
    let locked = f.locked(&path);
    let snapshot = locked.snapshot(10).unwrap();
    assert_eq!(snapshot.identity, Identity::Unconfirmed);
    assert_eq!(snapshot.entries.len(), 1);
    let kinds: Vec<NoticeKind> = snapshot.notices.iter().map(|notice| notice.kind).collect();
    assert_eq!(kinds, [NoticeKind::IdentityUnconfirmed]);
    assert!(locked.append("refused", None, None).is_err());
    assert_eq!(fs::read(&journal).unwrap(), before);
}

#[test]
fn a_read_creates_no_folder_and_no_file() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    let log = f.log(&path, b"fake-one", "root-a");
    assert_eq!(log.snapshot(10).unwrap().state, AuditState::Absent);
    assert_eq!(log.journal_path().unwrap(), None);
    assert!(!f.root.exists());
}

#[test]
fn a_legacy_sidecar_is_reported_and_stays_byte_identical() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    let sidecar = f.directory.path().join("a.kdbx.audit.jsonl");
    let legacy = b"{\"seq\":1,\"note\":\"fake legacy line\"}\n";
    fs::write(&sidecar, legacy).unwrap();
    let log = f.log(&path, b"fake-one", "root-a");
    let kinds = |log: &AuditLog| -> Vec<NoticeKind> {
        log.snapshot(10)
            .unwrap()
            .notices
            .iter()
            .map(|notice| notice.kind)
            .collect()
    };
    assert_eq!(kinds(&log), [NoticeKind::LegacySidecar]);
    log.append("first", None, None).unwrap();
    assert_eq!(kinds(&log), [NoticeKind::LegacySidecar]);
    assert_eq!(fs::read(&sidecar).unwrap(), legacy);
    assert_eq!(actions(&log), ["first"]);
}

#[test]
fn concurrent_mapping_appends_and_reads_keep_every_binding_and_every_chain() {
    let f = Fixture::new();
    let vaults: Vec<(PathBuf, Vec<u8>, String)> = (0..24)
        .map(|index| {
            let bytes = format!("fake-vault-{index}").into_bytes();
            (
                f.vault(&format!("v{index}.kdbx"), &bytes),
                bytes,
                format!("root-{index}"),
            )
        })
        .collect();
    let start = Arc::new(Barrier::new(9));
    std::thread::scope(|scope| {
        for lane in 0..8 {
            let start = start.clone();
            let f = &f;
            let vaults = &vaults;
            scope.spawn(move || {
                start.wait();
                for (index, (path, bytes, id)) in vaults.iter().enumerate() {
                    if index % 8 != lane && index % 3 != 0 {
                        continue;
                    }
                    for _ in 0..3 {
                        f.log(path, bytes, id).append("write", None, None).unwrap();
                    }
                }
            });
        }
        let reader = start.clone();
        let f = &f;
        let vaults = &vaults;
        scope.spawn(move || {
            reader.wait();
            for _ in 0..40 {
                for (path, bytes, id) in vaults {
                    let snapshot = f.log(path, bytes, id).snapshot(usize::MAX).unwrap();
                    assert_ne!(snapshot.state, AuditState::Corrupt);
                }
            }
        });
    });
    let mut journals = Vec::new();
    for (index, (path, bytes, id)) in vaults.iter().enumerate() {
        let log = f.log(path, bytes, id);
        let entries = log.read(usize::MAX).unwrap();
        assert_eq!(
            entries.len(),
            if index % 3 == 0 { 24 } else { 3 },
            "vault {index}"
        );
        assert_eq!(log.verify().unwrap(), None);
        journals.push(log.journal_path().unwrap().unwrap());
    }
    journals.sort();
    journals.dedup();
    assert_eq!(journals.len(), 24);
}

fn icacls(path: &Path, args: &[&str]) {
    let output = Command::new("icacls")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "icacls {args:?}: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}

struct ResetAcl<'a>(&'a Path);
impl Drop for ResetAcl<'_> {
    fn drop(&mut self) {
        let _ = Command::new("icacls")
            .arg(self.0)
            .args(["/reset", "/T", "/C", "/Q"])
            .output();
    }
}

#[test]
fn a_folder_with_an_extra_account_is_refused_and_nothing_is_written() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    let log = f.log(&path, b"fake-one", "root-a");
    log.append("first", None, None).unwrap();
    let _reset = ResetAcl(&f.root);
    icacls(&f.root, &["/grant", "*S-1-5-32-545:(OI)(CI)(R)"]);
    let journal = log.journal_path();
    assert!(journal.is_err(), "{journal:?}");
    assert!(log.append("second", None, None).is_err());
    assert!(JournalRoot::at(&f.root).ensure_private().is_err());
}

#[test]
fn a_root_that_is_a_junction_is_refused() {
    let f = Fixture::new();
    let target = f.directory.path().join("target");
    JournalRoot::at(&target).ensure_private().unwrap();
    let output = Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(&f.root)
        .arg(&target)
        .output()
        .unwrap();
    assert!(output.status.success());
    let path = f.vault("a.kdbx", b"fake-one");
    let log = f.log(&path, b"fake-one", "root-a");
    assert!(log.append("first", None, None).is_err());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    let _ = fs::remove_dir(&f.root);
}

#[test]
fn an_unusable_root_never_falls_back() {
    let relative = AuditLog::new(
        JournalRoot::at("relative-journal"),
        VaultEvidence::from_file(Path::new("missing.kdbx")),
    );
    assert!(relative.append("first", None, None).is_err());
    assert!(!Path::new("relative-journal").exists());
}

#[test]
fn a_root_below_a_junction_is_refused_even_when_the_target_is_private() {
    let f = Fixture::new();
    let target = f.directory.path().join("target");
    JournalRoot::at(target.join("journal"))
        .ensure_private()
        .unwrap();
    let link = f.directory.path().join("link");
    let output = Command::new("cmd")
        .arg("/C")
        .arg("mklink")
        .arg("/J")
        .arg(&link)
        .arg(&target)
        .output()
        .unwrap();
    assert!(output.status.success());
    let path = f.vault("a.kdbx", b"fake-one");
    let log = AuditLog::new(
        JournalRoot::at(link.join("journal")),
        VaultEvidence::from_file(&path),
    );
    assert!(log.append("first", None, None).is_err());
    assert!(log.snapshot(10).is_err());
    assert_eq!(fs::read_dir(target.join("journal")).unwrap().count(), 0);
    let _ = fs::remove_dir(&link);
}

// ----------------------------------- review 1, finding 5: a failed append consumes nothing

impl Fixture {
    fn mapping_bytes(&self) -> Vec<u8> {
        fs::read(self.root.join("mapping.json")).unwrap()
    }

    /// Two versions of one vault are observed, then the first one is restored. Returns the
    /// log of the restored version, with the journal holding `first` and `second`.
    fn rolled_back_vault(&self) -> (PathBuf, AuditLog) {
        let path = self.vault("a.kdbx", b"fake-one");
        self.log(&path, b"fake-one", "root-a")
            .append("first", None, None)
            .unwrap();
        fs::write(&path, b"fake-two").unwrap();
        self.log(&path, b"fake-two", "root-a")
            .append("second", None, None)
            .unwrap();
        fs::write(&path, b"fake-one").unwrap();
        let restored = self.log(&path, b"fake-one", "root-a");
        (path, restored)
    }
}

#[test]
fn a_damaged_journal_keeps_the_rollback_indication_for_the_retry() {
    let f = Fixture::new();
    let (_path, restored) = f.rolled_back_vault();
    let journal = restored.journal_path().unwrap().unwrap();
    let good = fs::read(&journal).unwrap();
    let mapping = f.mapping_bytes();
    let mut damaged = good.clone();
    damaged.extend_from_slice(b"{\"seq\":3,\"ts\":\"torn");
    fs::write(&journal, &damaged).unwrap();

    let error = restored.append("third", None, None).unwrap_err();
    assert!(error.to_string().contains("damaged"), "{error}");
    assert_eq!(fs::read(&journal).unwrap(), damaged, "damaged bytes stay");
    assert_eq!(f.mapping_bytes(), mapping, "the mapping did not advance");

    fs::write(&journal, &good).unwrap();
    restored.append("third", None, None).unwrap();
    restored.append("fourth", None, None).unwrap();
    assert_eq!(
        actions(&restored),
        ["first", "second", "vault-rolled-back", "third", "fourth"]
    );
}

#[test]
fn an_unopenable_journal_keeps_the_rollback_indication_for_the_retry() {
    let f = Fixture::new();
    let (_path, restored) = f.rolled_back_vault();
    let journal = restored.journal_path().unwrap().unwrap();
    let good = fs::read(&journal).unwrap();
    let mapping = f.mapping_bytes();
    fs::remove_file(&journal).unwrap();
    fs::create_dir(&journal).unwrap();

    assert!(restored.append("third", None, None).is_err());
    assert_eq!(f.mapping_bytes(), mapping, "the mapping did not advance");

    fs::remove_dir(&journal).unwrap();
    fs::write(&journal, &good).unwrap();
    restored.append("third", None, None).unwrap();
    assert_eq!(
        actions(&restored),
        ["first", "second", "vault-rolled-back", "third"]
    );
}

#[test]
fn many_retries_after_a_failed_append_write_one_rollback_indication() {
    let f = Fixture::new();
    let (path, restored) = f.rolled_back_vault();
    let journal = restored.journal_path().unwrap().unwrap();
    let good = fs::read(&journal).unwrap();
    let mut damaged = good.clone();
    damaged.extend_from_slice(b"torn");
    fs::write(&journal, &damaged).unwrap();
    assert!(restored.append("refused", None, None).is_err());
    fs::write(&journal, &good).unwrap();

    let start = Arc::new(Barrier::new(6));
    std::thread::scope(|scope| {
        for _ in 0..6 {
            let start = start.clone();
            let f = &f;
            let path = &path;
            scope.spawn(move || {
                let log = f.log(path, b"fake-one", "root-a");
                start.wait();
                log.append("retry", None, None).unwrap();
            });
        }
    });
    let all = actions(&restored);
    assert_eq!(
        all.iter()
            .filter(|action| *action == "vault-rolled-back")
            .count(),
        1,
        "{all:?}"
    );
    assert_eq!(all.iter().filter(|action| *action == "retry").count(), 6);
    assert_eq!(restored.verify().unwrap(), None);
}

#[test]
fn a_binding_that_only_creates_the_journal_path_does_not_consume_the_indication() {
    let f = Fixture::new();
    let (_path, restored) = f.rolled_back_vault();
    let before = f.mapping_bytes();
    restored.create_journal_path().unwrap();
    assert_eq!(f.mapping_bytes(), before);
    restored.append("third", None, None).unwrap();
    assert_eq!(
        actions(&restored),
        ["first", "second", "vault-rolled-back", "third"]
    );
}

// --------------------------- review 1, finding 4: a stale reader never rebinds the current vault

mod stale {
    use super::*;
    use akm_core::{KeyDisclosure, Vault};
    use serde_json::Value;

    const PASSWORD_A: &str = "fake-stale-password-a";
    const PASSWORD_B: &str = "fake-stale-password-b";

    pub struct Pair {
        pub _directory: tempfile::TempDir,
        pub path: PathBuf,
        pub root: PathBuf,
        pub a: Vault,
        pub b_bytes: Vec<u8>,
    }

    fn create(path: &Path, root: &Path, password: &str, title: &str) -> Vault {
        let mut vault = Vault::create_in(path, password, None, JournalRoot::at(root)).unwrap();
        vault
            .add(akm_core::ApiKeyInput::new(title, "fake-secret-value"))
            .unwrap();
        vault
    }

    impl Pair {
        /// Reader A is open on the vault at `path`. The bytes of vault B wait to replace it.
        pub fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().join("journal");
            let path = directory.path().join("shared.kdbx");
            let elsewhere = directory.path().join("b-source.kdbx");
            drop(create(
                &elsewhere,
                &directory.path().join("other-journal"),
                PASSWORD_B,
                "Beta",
            ));
            let b_bytes = fs::read(&elsewhere).unwrap();
            let a = create(&path, &root, PASSWORD_A, "Alpha");
            Self {
                _directory: directory,
                path,
                root,
                a,
                b_bytes,
            }
        }

        pub fn replace_with_b(&self) {
            fs::write(&self.path, &self.b_bytes).unwrap();
        }

        pub fn open_b(&self) -> Vault {
            Vault::open_in(&self.path, PASSWORD_B, JournalRoot::at(&self.root)).unwrap()
        }

        pub fn mapping(&self) -> Value {
            serde_json::from_slice(&fs::read(self.root.join("mapping.json")).unwrap()).unwrap()
        }

        pub fn journal_files(&self) -> Vec<PathBuf> {
            let mut found: Vec<PathBuf> = fs::read_dir(&self.root)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| {
                    let name = path.file_name().unwrap().to_string_lossy().into_owned();
                    name.starts_with("j-") && name.ends_with(".jsonl")
                })
                .collect();
            found.sort();
            found
        }
    }

    pub fn disclose(vault: &Vault) -> akm_core::Result<()> {
        let key = vault.list().remove(0);
        vault
            .disclose_key(KeyDisclosure::Reveal, &key, None)
            .map(|_| ())
    }

    pub fn active(mapping: &Value) -> Vec<Value> {
        mapping["journals"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|binding| binding["state"] == "active")
            .cloned()
            .collect()
    }
}

#[test]
fn a_stale_reader_cannot_detach_or_rebind_the_current_vault() {
    let pair = stale::Pair::new();
    stale::disclose(&pair.a).unwrap();
    pair.replace_with_b();
    let b = pair.open_b();
    stale::disclose(&b).unwrap();
    let after_b = pair.mapping();
    let current = stale::active(&after_b);
    assert_eq!(current.len(), 1);
    let current_id = current[0]["id"].as_str().unwrap().to_string();
    let current_seen = current[0]["seen"].clone();
    let a_journal = pair.a.audit_log().journal_path().unwrap();
    assert_eq!(a_journal, None, "A is no longer bound at this path");
    let a_files: Vec<(PathBuf, Vec<u8>)> = pair
        .journal_files()
        .into_iter()
        .map(|file| {
            let bytes = fs::read(&file).unwrap();
            (file, bytes)
        })
        .collect();

    for round in 0..3 {
        let error = stale::disclose(&pair.a).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("no longer the one this session opened"),
            "round {round}: {error}"
        );
        stale::disclose(&b).unwrap();
    }

    let mapping = pair.mapping();
    let bindings = mapping["journals"].as_array().unwrap();
    assert_eq!(bindings.len(), 2, "no third binding: {mapping}");
    let current = stale::active(&mapping);
    assert_eq!(current.len(), 1);
    assert_eq!(current[0]["id"].as_str().unwrap(), current_id);
    assert_eq!(current[0]["seen"], current_seen);
    assert_eq!(pair.journal_files().len(), 2);

    let records = b.audit_log().read(100).unwrap();
    assert_eq!(
        records.iter().map(|record| record.seq).collect::<Vec<_>>(),
        [1, 2, 3, 4],
        "the current vault keeps one continuous history"
    );
    assert!(records
        .iter()
        .all(|record| record.action == "reveal-requested"));
    let lines = |file: &PathBuf| {
        fs::read(file)
            .unwrap()
            .iter()
            .filter(|byte| **byte == 10)
            .count()
    };
    for (file, bytes) in &a_files {
        if lines(file) == 2 {
            assert_eq!(&fs::read(file).unwrap(), bytes, "A's journal is untouched");
        }
    }
    let a_records: usize = pair.journal_files().iter().map(lines).sum();
    assert_eq!(
        a_records, 6,
        "two records of A and four of B, nothing merged"
    );
}

#[test]
fn a_stale_reader_reads_nothing_from_and_changes_nothing_in_the_current_journal() {
    let pair = stale::Pair::new();
    stale::disclose(&pair.a).unwrap();
    pair.replace_with_b();
    let b = pair.open_b();
    stale::disclose(&b).unwrap();
    let mapping = fs::read_to_string(pair.root.join("mapping.json")).unwrap();

    let view = pair.a.audit_log().snapshot(10).unwrap();
    assert_eq!(view.identity, Identity::Unconfirmed);
    assert!(view.entries.is_empty(), "B's records are not A's");
    assert_eq!(pair.a.audit_log().journal_path().unwrap(), None);
    assert_eq!(
        fs::read_to_string(pair.root.join("mapping.json")).unwrap(),
        mapping
    );

    let current = b.audit_log().snapshot(10).unwrap();
    assert_eq!(current.identity, Identity::Confirmed);
    assert_eq!(current.entries.len(), 1);
    let kinds: Vec<NoticeKind> = current.notices.iter().map(|notice| notice.kind).collect();
    assert_eq!(
        kinds,
        [NoticeKind::EarlierJournal],
        "A's journal stays discoverable"
    );
}

#[test]
fn a_stale_reader_of_the_same_vault_adds_no_false_rollback_and_keeps_the_order() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    f.log(&path, b"fake-one", "root-a")
        .append("first", None, None)
        .unwrap();
    fs::write(&path, b"fake-two").unwrap();
    f.log(&path, b"fake-two", "root-a")
        .append("second", None, None)
        .unwrap();
    let mapping = f.mapping_bytes();

    let stale_reader = f.log(&path, b"fake-one", "root-a");
    stale_reader.append("from-stale", None, None).unwrap();
    f.log(&path, b"fake-two", "root-a")
        .append("from-current", None, None)
        .unwrap();
    stale_reader.append("from-stale-again", None, None).unwrap();
    assert_eq!(
        actions(&stale_reader),
        [
            "first",
            "second",
            "from-stale",
            "from-current",
            "from-stale-again"
        ]
    );
    assert_eq!(
        f.mapping_bytes(),
        mapping,
        "stale evidence changes no binding"
    );
}

#[test]
fn a_stale_reader_whose_vault_file_is_gone_still_records_in_its_own_journal() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    let reader = f.log(&path, b"fake-one", "root-a");
    reader.append("first", None, None).unwrap();
    fs::remove_file(&path).unwrap();
    reader.append("second", None, None).unwrap();
    assert_eq!(actions(&reader), ["first", "second"]);
}

#[test]
fn stale_evidence_for_a_path_without_a_binding_creates_no_binding() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-other");
    let reader = f.log(&path, b"fake-one", "root-a");
    let error = reader.append("first", None, None).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("no longer the one this session opened"),
        "{error}"
    );
    assert!(
        !f.root.join("mapping.json").exists(),
        "no mapping was written for stale evidence"
    );
}

#[test]
fn a_locked_reader_that_was_made_before_the_vault_changed_still_shows_the_journal_unconfirmed() {
    let f = Fixture::new();
    let path = f.vault("a.kdbx", b"fake-one");
    f.log(&path, b"fake-one", "root-a")
        .append("first", None, None)
        .unwrap();
    fs::write(&path, b"fake-unseen").unwrap();
    let locked = f.locked(&path);
    fs::write(&path, b"fake-two").unwrap();
    f.log(&path, b"fake-two", "root-a")
        .append("second", None, None)
        .unwrap();
    let snapshot = locked.snapshot(10).unwrap();
    assert_eq!(snapshot.identity, Identity::Unconfirmed);
    assert_eq!(snapshot.entries.len(), 2);
}
