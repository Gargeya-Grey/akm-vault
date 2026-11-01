use std::sync::{Arc, Barrier};

use akm_core::audit::AuditLog;

#[test]
fn concurrent_sessions_append_one_complete_journal_chain() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault.kdbx");
    let start = Arc::new(Barrier::new(8));
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let vault = &vault;
            let start = start.clone();
            scope.spawn(move || {
                start.wait();
                for _ in 0..16 {
                    AuditLog::for_vault(vault)
                        .append("reveal", None, None, None, None, None)
                        .unwrap();
                }
            });
        }
    });
    let log = AuditLog::for_vault(&vault);
    assert_eq!(log.read(usize::MAX).unwrap().len(), 128);
    assert_eq!(log.verify().unwrap(), None);
}
