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
