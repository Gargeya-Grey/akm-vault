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

// ------------------------------- review 2, finding 4: clones share a root and never share a journal
//
// Real fake KDBX vaults. A clone shares the root group UUID of its source, so the UUID cannot
// say which instance a file is. A version that this device saw belongs to the journal that
// recorded it, wherever that version shows up (unit 1b, D12).

mod clones {
    use super::*;
    use akm_core::{ApiKeyInput, Vault};
    use serde_json::Value;

    pub const PASSWORD: &str = "fake-clone-password";

    pub struct World {
        pub directory: tempfile::TempDir,
        pub root: PathBuf,
    }

    impl World {
        pub fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let root = directory.path().join("journal");
            Self { directory, root }
        }

        pub fn path(&self, name: &str) -> PathBuf {
            self.directory.path().join(name)
        }

        /// A new vault with one key. Its journal gets `add-completed`.
        pub fn create(&self, name: &str) -> Vault {
            let mut vault =
                Vault::create_in(self.path(name), PASSWORD, None, JournalRoot::at(&self.root))
                    .unwrap();
            vault
                .add(ApiKeyInput::new("Alpha", "fake-secret-value"))
                .unwrap();
            vault
        }

        pub fn open(&self, name: &str) -> Vault {
            Vault::open_in(self.path(name), PASSWORD, JournalRoot::at(&self.root)).unwrap()
        }

        pub fn copy(&self, from: &str, to: &str) {
            fs::copy(self.path(from), self.path(to)).unwrap();
        }

        pub fn bytes(&self, name: &str) -> Vec<u8> {
            fs::read(self.path(name)).unwrap()
        }

        /// Overwrites the file in place, so its file ID stays.
        pub fn put(&self, name: &str, bytes: &[u8]) {
            fs::write(self.path(name), bytes).unwrap();
        }

        pub fn mapping_text(&self) -> String {
            fs::read_to_string(self.root.join("mapping.json")).unwrap()
        }

        pub fn mapping(&self) -> Value {
            serde_json::from_str(&self.mapping_text()).unwrap()
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

        fn actions_in(file: &Path) -> Vec<String> {
            fs::read_to_string(file)
                .unwrap()
                .lines()
                .map(|line| {
                    serde_json::from_str::<Value>(line).unwrap()["action"]
                        .as_str()
                        .unwrap()
                        .to_string()
                })
                .collect()
        }

        /// The journal file that holds `marker`. There must be exactly one.
        pub fn journal_with(&self, marker: &str) -> PathBuf {
            let holding: Vec<PathBuf> = self
                .journal_files()
                .into_iter()
                .filter(|file| Self::actions_in(file).iter().any(|action| action == marker))
                .collect();
            assert_eq!(holding.len(), 1, "{marker}: {holding:?}");
            holding.into_iter().next().unwrap()
        }

        /// The actions of the journal that holds `marker`.
        pub fn actions_with(&self, marker: &str) -> Vec<String> {
            Self::actions_in(&self.journal_with(marker))
        }

        /// A password-less read of the vault at `name`, as `akm audit` does it.
        pub fn view(&self, name: &str) -> akm_core::audit::AuditSnapshot {
            AuditLog::new(
                JournalRoot::at(&self.root),
                VaultEvidence::from_file(&self.path(name)),
            )
            .snapshot(100)
            .unwrap()
        }
    }

    pub fn mark(vault: &Vault, action: &str) -> akm_core::Result<()> {
        vault.audit_log().append(action, None, None)
    }

    pub fn change(vault: &mut Vault, title: &str) {
        vault
            .add(ApiKeyInput::new(title, "fake-secret-value"))
            .unwrap();
    }

    pub fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    pub fn seen_lengths(mapping: &Value) -> Vec<usize> {
        mapping["journals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|binding| binding["seen"].as_array().unwrap().len())
            .collect()
    }
}

#[test]
fn a_clone_that_replaces_its_original_keeps_its_own_journal_and_the_original_keeps_its_own() {
    use clones::*;
    let w = World::new();
    let original = w.create("a.kdbx");
    mark(&original, "original-marker").unwrap();
    w.copy("a.kdbx", "b.kdbx");
    let mut clone = w.open("b.kdbx");
    change(&mut clone, "Beta");
    mark(&clone, "clone-marker").unwrap();
    let original_journal = w.journal_with("original-marker");
    let clone_journal = w.journal_with("clone-marker");
    assert_ne!(original_journal, clone_journal);
    let original_before = fs::read(&original_journal).unwrap();

    w.copy("b.kdbx", "a.kdbx");
    let replacement = w.open("a.kdbx");
    mark(&replacement, "replacement-marker").unwrap();

    assert_eq!(
        w.actions_with("original-marker"),
        strings(&["add-completed", "original-marker"]),
        "the original's journal gained nothing from the clone's bytes"
    );
    assert_eq!(
        fs::read(&original_journal).unwrap(),
        original_before,
        "the original's journal bytes are unchanged"
    );
    assert_eq!(
        w.actions_with("clone-marker"),
        strings(&["add-completed", "clone-marker", "replacement-marker"]),
        "the replacement is the clone's version, so its record is the clone's"
    );
    let mapping = w.mapping();
    assert_eq!(
        mapping["journals"].as_array().unwrap().len(),
        2,
        "{mapping}"
    );
    assert!(
        mapping["journals"]
            .as_array()
            .unwrap()
            .iter()
            .all(|binding| binding["state"] == "active"),
        "no binding was detached: {mapping}"
    );
    assert_eq!(
        seen_lengths(&mapping),
        [1, 1],
        "no version moved between bindings"
    );
}

#[test]
fn a_reader_at_the_replaced_path_sees_the_clone_journal_and_a_notice_for_the_original_one() {
    use clones::*;
    let w = World::new();
    let original = w.create("a.kdbx");
    mark(&original, "original-marker").unwrap();
    w.copy("a.kdbx", "b.kdbx");
    let mut clone = w.open("b.kdbx");
    change(&mut clone, "Beta");
    mark(&clone, "clone-marker").unwrap();
    w.copy("b.kdbx", "a.kdbx");
    let original_journal = w.journal_with("original-marker");
    let mapping = w.mapping_text();

    let view = w.view("a.kdbx");
    assert_eq!(view.identity, Identity::Confirmed);
    let shown: Vec<&str> = view
        .entries
        .iter()
        .map(|entry| entry.action.as_str())
        .collect();
    assert_eq!(shown, ["add-completed", "clone-marker"]);
    let named = view
        .notices
        .iter()
        .filter(|notice| {
            notice
                .message
                .contains(&original_journal.display().to_string())
        })
        .count();
    assert_eq!(
        named, 1,
        "the original's journal file is named: {:?}",
        view.notices
    );
    assert_eq!(w.mapping_text(), mapping, "a read changes nothing");

    let at_clone = w.view("b.kdbx");
    assert_eq!(at_clone.identity, Identity::Confirmed);
    assert_eq!(at_clone.entries.len(), 2);
}

#[test]
fn a_session_that_saves_after_the_replacement_stays_with_the_journal_of_the_version_it_opened() {
    use clones::*;
    let w = World::new();
    let original = w.create("a.kdbx");
    mark(&original, "original-marker").unwrap();
    w.copy("a.kdbx", "b.kdbx");
    let mut clone = w.open("b.kdbx");
    change(&mut clone, "Beta");
    mark(&clone, "clone-marker").unwrap();
    w.copy("b.kdbx", "a.kdbx");

    let mut replacement = w.open("a.kdbx");
    mark(&replacement, "opened-marker").unwrap();
    change(&mut replacement, "Gamma");
    mark(&replacement, "saved-marker").unwrap();
    change(&mut replacement, "Delta");
    let mut next = w.open("a.kdbx");
    mark(&next, "next-process-marker").unwrap();
    change(&mut next, "Epsilon");
    let later = w.open("a.kdbx");
    mark(&later, "later-process-marker").unwrap();

    assert_eq!(
        w.actions_with("original-marker"),
        strings(&["add-completed", "original-marker"]),
        "saves of the replacement never reach the original's journal"
    );
    assert_eq!(
        w.actions_with("clone-marker"),
        strings(&[
            "add-completed",
            "clone-marker",
            "opened-marker",
            "add-completed",
            "saved-marker",
            "add-completed",
            "next-process-marker",
            "add-completed",
            "later-process-marker",
        ])
    );
    assert_eq!(w.journal_files().len(), 2);
}

#[test]
fn stale_and_current_sessions_alternate_reads_and_disclosures_without_crossing_journals() {
    use clones::*;
    let w = World::new();
    let original = w.create("a.kdbx");
    mark(&original, "original-marker").unwrap();
    w.copy("a.kdbx", "b.kdbx");
    let mut clone = w.open("b.kdbx");
    change(&mut clone, "Beta");
    mark(&clone, "clone-marker").unwrap();
    w.copy("b.kdbx", "a.kdbx");
    let replacement = w.open("a.kdbx");

    for round in 0..3 {
        stale::disclose(&original).unwrap_or_else(|error| panic!("round {round}: {error}"));
        stale::disclose(&replacement).unwrap();
        stale::disclose(&clone).unwrap();
        mark(&replacement, "replacement-marker").unwrap();
        let reader = w.view("a.kdbx");
        assert_eq!(reader.identity, Identity::Confirmed, "round {round}");
        assert_eq!(
            reader.entries.last().map(|entry| entry.action.as_str()),
            Some("replacement-marker")
        );
    }

    let mut expected_original = strings(&["add-completed", "original-marker"]);
    expected_original.extend(strings(&["reveal-requested"; 3]));
    assert_eq!(w.actions_with("original-marker"), expected_original);
    let mut expected_clone = strings(&["add-completed", "clone-marker"]);
    for _ in 0..3 {
        expected_clone.extend(strings(&[
            "reveal-requested",
            "reveal-requested",
            "replacement-marker",
        ]));
    }
    assert_eq!(w.actions_with("clone-marker"), expected_clone);
    let mapping = w.mapping();
    assert_eq!(
        mapping["journals"].as_array().unwrap().len(),
        2,
        "{mapping}"
    );
    let records = AuditLog::new(
        JournalRoot::at(&w.root),
        VaultEvidence::from_file(&w.path("b.kdbx")),
    )
    .read(100)
    .unwrap();
    assert_eq!(
        records.iter().map(|record| record.seq).collect::<Vec<_>>(),
        (1..=records.len() as u64).collect::<Vec<_>>(),
        "the clone's chain is one continuous sequence"
    );
}

#[test]
fn putting_the_original_back_returns_to_its_journal_even_when_a_copy_saw_the_same_version() {
    use clones::*;
    let w = World::new();
    let original = w.create("a.kdbx");
    mark(&original, "original-marker").unwrap();
    let original_bytes = w.bytes("a.kdbx");
    w.copy("a.kdbx", "fresh.kdbx");
    let fresh = w.open("fresh.kdbx");
    mark(&fresh, "fresh-marker").unwrap();
    w.copy("a.kdbx", "b.kdbx");
    let mut clone = w.open("b.kdbx");
    change(&mut clone, "Beta");
    mark(&clone, "clone-marker").unwrap();
    w.copy("b.kdbx", "a.kdbx");
    mark(&w.open("a.kdbx"), "replacement-marker").unwrap();

    w.put("a.kdbx", &original_bytes);
    mark(&w.open("a.kdbx"), "restored-marker").unwrap();

    assert_eq!(
        w.actions_with("original-marker"),
        strings(&["add-completed", "original-marker", "restored-marker"]),
        "the version is in the original's list and in the fresh copy's list; the path decides"
    );
    assert_eq!(w.actions_with("fresh-marker"), strings(&["fresh-marker"]));
    assert_eq!(
        w.actions_with("clone-marker"),
        strings(&["add-completed", "clone-marker", "replacement-marker"])
    );
}

#[test]
fn a_rollback_indication_lands_in_the_journal_of_the_instance_that_went_back() {
    use clones::*;
    let w = World::new();
    let mut original = w.create("a.kdbx");
    mark(&original, "a-one").unwrap();
    let a_earlier = w.bytes("a.kdbx");
    change(&mut original, "Alpha2");
    w.copy("a.kdbx", "b.kdbx");
    let mut clone = w.open("b.kdbx");
    change(&mut clone, "Beta");
    mark(&clone, "b-one").unwrap();
    let b_earlier = w.bytes("b.kdbx");
    change(&mut clone, "Gamma");

    w.put("a.kdbx", &b_earlier);
    mark(&w.open("a.kdbx"), "b-old-version-at-a").unwrap();
    mark(&w.open("b.kdbx"), "b-current").unwrap();
    assert_eq!(
        w.actions_with("b-one")[2..],
        ["add-completed", "b-old-version-at-a", "b-current"],
        "the old version of the clone at the original's path is recorded in the clone's journal"
    );
    assert!(
        !w.actions_with("a-one")
            .iter()
            .any(|a| a == "b-old-version-at-a"),
        "and not in the original's"
    );
    assert!(
        !w.actions_with("b-one")
            .iter()
            .any(|a| a == "vault-rolled-back"),
        "an old version of the clone at another path is not a rollback of the clone's own file: {:?}",
        w.actions_with("b-one")
    );

    w.put("b.kdbx", &b_earlier);
    mark(&w.open("b.kdbx"), "b-back").unwrap();
    let b_actions = w.actions_with("b-one");
    let at = b_actions.iter().position(|a| a == "b-back").unwrap();
    assert_eq!(b_actions[at - 1], "vault-rolled-back", "{b_actions:?}");
    assert_eq!(
        b_actions
            .iter()
            .filter(|a| *a == "vault-rolled-back")
            .count(),
        1
    );
    assert!(
        !w.actions_with("a-one")
            .iter()
            .any(|a| a == "vault-rolled-back"),
        "the original has not gone back yet"
    );

    w.put("a.kdbx", &a_earlier);
    mark(&w.open("a.kdbx"), "a-back").unwrap();
    let a_actions = w.actions_with("a-one");
    let at = a_actions.iter().position(|a| a == "a-back").unwrap();
    assert_eq!(a_actions[at - 1], "vault-rolled-back", "{a_actions:?}");
    assert_eq!(
        w.actions_with("b-one")
            .iter()
            .filter(|a| *a == "vault-rolled-back")
            .count(),
        1,
        "the original's rollback is not written to the clone's journal"
    );
}

#[test]
fn a_version_that_two_journals_recorded_fails_closed_at_a_path_bound_to_a_third() {
    use clones::*;
    let w = World::new();
    let original = w.create("a.kdbx");
    mark(&original, "a-marker").unwrap();
    let shared = w.bytes("a.kdbx");
    w.copy("a.kdbx", "c.kdbx");
    mark(&w.open("c.kdbx"), "c-marker").unwrap();
    w.copy("a.kdbx", "d.kdbx");
    let mut third = w.open("d.kdbx");
    change(&mut third, "Delta");
    mark(&third, "d-marker").unwrap();

    w.put("d.kdbx", &shared);
    let mapping = w.mapping_text();
    let files: Vec<(PathBuf, Vec<u8>)> = w
        .journal_files()
        .into_iter()
        .map(|file| (file.clone(), fs::read(&file).unwrap()))
        .collect();
    let reopened = w.open("d.kdbx");
    for round in 0..2 {
        let error = mark(&reopened, "ambiguous-marker").unwrap_err();
        assert!(
            error.to_string().contains("more than one journal"),
            "round {round}: {error}"
        );
        assert!(
            stale::disclose(&reopened).is_err(),
            "a disclosure fails closed too"
        );
    }
    assert_eq!(w.mapping_text(), mapping, "nothing was stored");
    for (file, bytes) in &files {
        assert_eq!(&fs::read(file).unwrap(), bytes, "{}", file.display());
    }

    let view = w.view("d.kdbx");
    assert_eq!(view.identity, Identity::Unconfirmed);
    assert!(view.entries.is_empty(), "no guessed journal is shown");
    let kinds: Vec<NoticeKind> = view.notices.iter().map(|notice| notice.kind).collect();
    assert_eq!(kinds[0], NoticeKind::IdentityUnconfirmed);
    assert!(
        view.notices
            .iter()
            .any(|notice| notice.message.contains("c.kdbx")),
        "the candidates are named: {:?}",
        view.notices
    );

    w.copy("d.kdbx", "e.kdbx");
    mark(&w.open("e.kdbx"), "e-marker").unwrap();
    assert_eq!(w.actions_with("e-marker"), strings(&["e-marker"]));
    assert_eq!(w.mapping()["journals"].as_array().unwrap().len(), 4);
}

#[test]
fn a_fresh_copy_at_a_new_path_still_starts_its_own_journal() {
    use clones::*;
    let w = World::new();
    let original = w.create("a.kdbx");
    mark(&original, "original-marker").unwrap();
    w.copy("a.kdbx", "copy.kdbx");
    let mut copy = w.open("copy.kdbx");
    assert_eq!(w.view("copy.kdbx").entries.len(), 0, "no journal yet");
    mark(&copy, "copy-marker").unwrap();
    change(&mut copy, "Beta");
    mark(&copy, "copy-saved-marker").unwrap();

    assert_eq!(
        w.actions_with("original-marker"),
        strings(&["add-completed", "original-marker"])
    );
    assert_eq!(
        w.actions_with("copy-marker"),
        strings(&["copy-marker", "add-completed", "copy-saved-marker"])
    );
    assert_eq!(w.mapping()["journals"].as_array().unwrap().len(), 2);
}

#[test]
fn a_renamed_replacement_is_not_moved_onto_the_journal_of_the_file_it_replaced() {
    use clones::*;
    let w = World::new();
    let original = w.create("a.kdbx");
    mark(&original, "original-marker").unwrap();
    w.copy("a.kdbx", "b.kdbx");
    let mut clone = w.open("b.kdbx");
    change(&mut clone, "Beta");
    mark(&clone, "clone-marker").unwrap();

    w.put("a.kdbx", &w.bytes("b.kdbx"));
    fs::rename(w.path("a.kdbx"), w.path("c.kdbx")).unwrap();
    mark(&w.open("c.kdbx"), "renamed-marker").unwrap();

    assert_eq!(
        w.actions_with("original-marker"),
        strings(&["add-completed", "original-marker"]),
        "the file ID of the replaced file does not make the clone the original"
    );
    assert_eq!(
        w.actions_with("clone-marker"),
        strings(&["add-completed", "clone-marker"])
    );
    assert_eq!(
        w.actions_with("renamed-marker"),
        strings(&["renamed-marker"])
    );
}

// ---------------------------------------------- FR-12: an unseen clone at a bound path warns

fn lineage_warnings(vault: &mut akm_core::Vault) -> usize {
    vault
        .take_warning_records()
        .iter()
        .filter(|warning| {
            warning.kind == akm_core::vault::VaultWarningKind::JournalLineageUnconfirmed
        })
        .count()
}

/// FR-12 and P2R2-04, the reviewer's probe: copy A to B, change B where A's journal cannot
/// see it, then replace A with B. The vault ID is the same, so the journal of A continues
/// (that fallback stays), but the vault says that the continuity is not confirmed. It says
/// so once; the version is then known to the journal.
#[test]
fn an_unseen_clone_that_replaces_a_bound_vault_continues_its_journal_with_a_warning() {
    use clones::*;
    let w = World::new();
    let original = w.create("a.kdbx");
    mark(&original, "original-marker").unwrap();
    drop(original);
    w.copy("a.kdbx", "b.kdbx");
    let elsewhere = w.directory.path().join("other-journal");
    let mut clone =
        akm_core::Vault::open_in(w.path("b.kdbx"), PASSWORD, JournalRoot::at(&elsewhere)).unwrap();
    change(&mut clone, "Beta");
    drop(clone);

    w.copy("b.kdbx", "a.kdbx");
    let mut replacement = w.open("a.kdbx");
    assert_eq!(
        lineage_warnings(&mut replacement),
        0,
        "opening records nothing"
    );
    replacement.log("replacement-marker", None, None).unwrap();
    assert_eq!(lineage_warnings(&mut replacement), 1);
    let warning_text = {
        replacement.log("again-marker", None, None).unwrap();
        replacement.take_warnings()
    };
    assert!(
        warning_text.is_empty(),
        "the warning is given once: {warning_text:?}"
    );

    assert_eq!(
        w.actions_with("original-marker"),
        strings(&[
            "add-completed",
            "original-marker",
            "replacement-marker",
            "again-marker"
        ]),
        "the history continues in the existing journal"
    );
    // The message says what happened and does not claim a cause it cannot know.
    let mut fresh = w.open("a.kdbx");
    fresh.log("third-marker", None, None).unwrap();
    assert_eq!(
        lineage_warnings(&mut fresh),
        0,
        "a later session sees a known version"
    );
}

/// FR-12: the warning is for an unseen file only. Ordinary saves, a second session on the
/// version the first one saved, and a rename give none.
#[test]
fn ordinary_saves_and_reopens_give_no_lineage_warning() {
    use clones::*;
    let w = World::new();
    let mut vault = w.create("a.kdbx");
    change(&mut vault, "Beta");
    vault.log("one", None, None).unwrap();
    assert_eq!(lineage_warnings(&mut vault), 0);
    change(&mut vault, "Gamma");
    vault.log("two", None, None).unwrap();
    assert_eq!(lineage_warnings(&mut vault), 0);
    drop(vault);

    let mut again = w.open("a.kdbx");
    again.log("three", None, None).unwrap();
    assert_eq!(lineage_warnings(&mut again), 0);
    drop(again);

    w.copy("a.kdbx", "b.kdbx");
    let mut copy = w.open("b.kdbx");
    copy.log("copy-marker", None, None).unwrap();
    assert_eq!(
        lineage_warnings(&mut copy),
        0,
        "a copy starts its own journal"
    );
}

// ------------------------- core review 1, findings 4 and 5: the lineage table, row by row
//
// | File at the bound path | Read            | Append succeeds | Append fails, retry  | Lock between |
// | Bound vault            | confirmed       | no warning      | no change; no warning | no warning   |
// | Unseen clone           | unconfirmed +   | one warning,    | no change; the retry  | the first    |
// |                        | notice          | then known      | warns                 | append warns |
// | Seen clone (earlier)   | confirmed       | rollback record | no change; rollback   | rollback     |
//
// "No change" means that the mapping bytes are as they were. A read never changes them.

/// The reviewer's probe: the file at `a.kdbx` is a copy of the bound vault that was changed
/// where the bound journal could not see it.
fn bound_with_unseen_clone() -> clones::World {
    use clones::*;
    let w = World::new();
    let original = w.create("a.kdbx");
    mark(&original, "original-marker").unwrap();
    drop(original);
    w.copy("a.kdbx", "b.kdbx");
    let elsewhere = w.directory.path().join("other-journal");
    let mut clone =
        akm_core::Vault::open_in(w.path("b.kdbx"), PASSWORD, JournalRoot::at(&elsewhere)).unwrap();
    change(&mut clone, "Beta");
    drop(clone);
    w.copy("b.kdbx", "a.kdbx");
    w
}

/// Holds a journal file open with no sharing, so an append cannot open it: the reviewer's
/// way to make an append fail.
#[cfg(windows)]
fn hold(path: &Path) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap()
}

fn unconfirmed_notices(snapshot: &akm_core::audit::AuditSnapshot) -> usize {
    snapshot
        .notices
        .iter()
        .filter(|notice| notice.kind == NoticeKind::IdentityUnconfirmed)
        .count()
}

/// Finding 5, the read column: an Activity read of an unseen clone, with the password-less
/// evidence and with the unlocked evidence that the desktop's `audit_read` uses, is
/// unconfirmed with a notice. It changes no mapping byte. After the first successful append
/// (which warns), the version is known and a read is confirmed.
#[test]
fn a_read_of_an_unseen_clone_is_unconfirmed_with_a_notice_and_changes_nothing() {
    let w = bound_with_unseen_clone();
    let mapping = w.mapping_text();
    let locked = w.view("a.kdbx");
    assert_eq!(locked.identity, Identity::Unconfirmed);
    assert_eq!(unconfirmed_notices(&locked), 1, "{:?}", locked.notices);
    let mut vault = w.open("a.kdbx");
    let unlocked = vault.audit_log().snapshot(100).unwrap();
    assert_eq!(
        unlocked.identity,
        Identity::Unconfirmed,
        "{:?}",
        unlocked.notices
    );
    assert_eq!(unconfirmed_notices(&unlocked), 1, "{:?}", unlocked.notices);
    assert_eq!(w.mapping_text(), mapping, "a read changes nothing");
    assert_eq!(
        lineage_warnings(&mut vault),
        0,
        "a read consumes no warning"
    );

    vault.log("first-marker", None, None).unwrap();
    assert_eq!(lineage_warnings(&mut vault), 1);
    let after = vault.audit_log().snapshot(100).unwrap();
    assert_eq!(after.identity, Identity::Confirmed);
    assert_eq!(unconfirmed_notices(&after), 0);
}

/// Finding 4, the failure column: an append that fails under an unseen clone leaves the
/// mapping as it was, so the retry still warns. The same holds when the vault is locked and
/// opened again between the failure and the retry.
#[cfg(windows)]
#[test]
fn a_failed_append_under_an_unseen_clone_keeps_the_lineage_warning_for_the_retry() {
    for lock_between in [false, true] {
        let w = bound_with_unseen_clone();
        let mapping = w.mapping_text();
        let journal = w.journal_with("original-marker");
        let mut vault = w.open("a.kdbx");
        {
            let _held = hold(&journal);
            assert!(
                vault.log("failed-marker", None, None).is_err(),
                "the append fails while the journal is held"
            );
        }
        assert_eq!(
            w.mapping_text(),
            mapping,
            "lock between: {lock_between}: a failed append changes no mapping"
        );
        assert_eq!(lineage_warnings(&mut vault), 0);
        if lock_between {
            drop(vault);
            vault = w.open("a.kdbx");
        }
        vault.log("retry-marker", None, None).unwrap();
        assert_eq!(
            lineage_warnings(&mut vault),
            1,
            "lock between: {lock_between}: the retry warns"
        );
        vault.log("next-marker", None, None).unwrap();
        assert_eq!(lineage_warnings(&mut vault), 0, "the warning is given once");
        assert_eq!(
            w.actions_with("original-marker"),
            clones::strings(&[
                "add-completed",
                "original-marker",
                "retry-marker",
                "next-marker"
            ])
        );
    }
}

/// The other rows: a bound vault and a seen clone (an earlier version that the journal saw).
/// Reads are confirmed and change nothing, a failed append changes nothing, and the retry
/// gives no lineage warning. For the seen clone the retry records the rollback first.
#[cfg(windows)]
#[test]
fn the_lineage_table_rows_for_a_bound_vault_and_a_seen_clone() {
    use clones::*;
    for seen_clone in [false, true] {
        let w = World::new();
        let mut vault = w.create("a.kdbx");
        mark(&vault, "original-marker").unwrap();
        w.copy("a.kdbx", "earlier.kdbx");
        change(&mut vault, "Beta");
        drop(vault);
        if seen_clone {
            let earlier = w.bytes("earlier.kdbx");
            w.put("a.kdbx", &earlier);
        }
        let mapping = w.mapping_text();
        let view = w.view("a.kdbx");
        assert_eq!(
            view.identity,
            Identity::Confirmed,
            "seen clone: {seen_clone}"
        );
        let mut vault = w.open("a.kdbx");
        let unlocked = vault.audit_log().snapshot(100).unwrap();
        assert_eq!(
            unlocked.identity,
            Identity::Confirmed,
            "seen clone: {seen_clone}"
        );
        assert_eq!(
            w.mapping_text(),
            mapping,
            "seen clone: {seen_clone}: a read"
        );

        let journal = w.journal_with("original-marker");
        {
            let _held = hold(&journal);
            assert!(vault.log("failed-marker", None, None).is_err());
        }
        assert_eq!(
            w.mapping_text(),
            mapping,
            "seen clone: {seen_clone}: a failure"
        );
        vault.log("retry-marker", None, None).unwrap();
        assert_eq!(lineage_warnings(&mut vault), 0, "seen clone: {seen_clone}");
        let actions = w.actions_with("original-marker");
        let tail: Vec<&str> = actions.iter().rev().take(2).map(String::as_str).collect();
        if seen_clone {
            assert_eq!(tail, ["retry-marker", "vault-rolled-back"], "{actions:?}");
        } else {
            assert_eq!(tail[0], "retry-marker", "{actions:?}");
            assert!(!actions.iter().any(|action| action == "vault-rolled-back"));
        }
    }
}

/// Findings 4 and 5, the sweep of the class: every other path that reaches the mapping under
/// an unseen clone. Creating the journal path and recovering a damaged journal report no
/// lineage warning, so neither settles it: the next append still warns.
#[test]
fn neither_recovery_nor_placing_the_journal_settles_an_unseen_lineage() {
    for recover in [false, true] {
        let w = bound_with_unseen_clone();
        let mut vault = w.open("a.kdbx");
        let log = vault.audit_log();
        let journal = log.create_journal_path().unwrap();
        if recover {
            let mut file = fs::OpenOptions::new().append(true).open(&journal).unwrap();
            std::io::Write::write_all(&mut file, b"{this is not a record}").unwrap();
            drop(file);
            log.recover_damaged().unwrap();
        }
        assert_eq!(lineage_warnings(&mut vault), 0, "recover: {recover}");
        vault.log("after-marker", None, None).unwrap();
        assert_eq!(
            lineage_warnings(&mut vault),
            1,
            "recover: {recover}: the next append still warns"
        );
    }
}
