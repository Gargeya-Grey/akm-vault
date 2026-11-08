use std::sync::{Arc, Barrier};

use akm_core::audit::AuditLog;
use akm_core::journal::{JournalRoot, VaultEvidence};

#[test]
fn concurrent_sessions_append_one_complete_journal_chain() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault.kdbx");
    std::fs::write(&vault, b"fake vault bytes").unwrap();
    let root = JournalRoot::at(dir.path().join("journal"));
    let open = || AuditLog::new(root.clone(), VaultEvidence::from_file(&vault));
    let start = Arc::new(Barrier::new(8));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let start = start.clone();
            let open = &open;
            scope.spawn(move || {
                start.wait();
                for _ in 0..16 {
                    open().append("reveal", None, None).unwrap();
                }
            });
        }
    });
    let log = open();
    assert_eq!(log.read(usize::MAX).unwrap().len(), 128);
    assert_eq!(log.verify().unwrap(), None);
}
