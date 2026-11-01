#![allow(dead_code)]
#[path = "W:/API Keys Manager/desktop/src-tauri/src/error.rs"]
mod error;
#[path = "W:/API Keys Manager/desktop/src-tauri/src/clipboard.rs"]
mod clipboard;
#[path = "W:/API Keys Manager/desktop/src-tauri/src/policy.rs"]
mod policy;
#[path = "W:/API Keys Manager/desktop/src-tauri/src/pending.rs"]
mod pending;
#[path = "W:/API Keys Manager/desktop/src-tauri/src/session.rs"]
mod session;
#[path = "W:/API Keys Manager/desktop/src-tauri/src/private_export.rs"]
mod private_export;

use std::{fs, sync::{Arc, atomic::{AtomicUsize, Ordering}, mpsc}, time::Duration};
use akm_core::Vault;

struct NoClipboard;
impl clipboard::ClipboardBackend for NoClipboard {
    fn set_private(&mut self, _: &str) -> Result<clipboard::CopyAttempt, error::CommandError> {
        panic!("Real clipboard access is forbidden in this review")
    }
    fn clear_matching(&mut self, _: u32, _: &str) -> Result<bool, error::CommandError> { Ok(false) }
}

fn main() {
    let directory = std::env::current_dir().unwrap().join(format!("fake-late-receipt-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&directory).unwrap();
    let vault_path = directory.join("vault.kdbx");
    let password = "fake independent late receipt password";
    let events = Arc::new(AtomicUsize::new(0));
    let notify = events.clone();
    let session = Arc::new(session::Session::new(
        Arc::new(session::MonotonicClock::default()), Box::new(NoClipboard),
        Arc::new(move || { notify.fetch_add(1, Ordering::SeqCst); }),
    ));
    session.observe(policy::Signal::Available);
    let ticket_a = session.begin_open().unwrap();
    session.open(ticket_a, || Vault::create(&vault_path, password, None).map_err(Into::into)).unwrap();
    let snapshot_a = session.snapshot().unwrap();
    let target = directory.join("out.env");
    let exporter = session.clone();
    let (authorized_tx, authorized_rx) = mpsc::channel();
    let (finish_tx, finish_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || exporter.publish(snapshot_a,
        || private_export::prepare_private(&target, "TOKEN=fake-authorized\n", &[], &[]),
        |prepared| {
            authorized_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
            prepared.publish()
        },
    ));
    authorized_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    session.lock().unwrap();
    let ticket_b = session.begin_open().unwrap();
    session.open(ticket_b, || Vault::open(&vault_path, password).map_err(Into::into)).unwrap();
    assert_eq!(session.snapshot().unwrap().ticket, ticket_b);
    finish_tx.send(()).unwrap();
    let old_receipt = worker.join().unwrap().unwrap_err();
    assert_eq!(old_receipt.kind, "completedWhileLocked");
    assert_eq!(session.snapshot().unwrap().ticket, ticket_b);
    println!("CONFIRMED: session-A export can return completedWhileLocked after session B has opened, while native B remains active");
    let count = events.load(Ordering::SeqCst);
    let ticket_c = session.begin_open().unwrap();
    assert_eq!(events.load(Ordering::SeqCst), count + 1);
    session.open(ticket_c, || Vault::open(&vault_path, password).map_err(Into::into)).unwrap();
    assert_eq!(session.snapshot().unwrap().ticket, ticket_c);
    println!("CONFIRMED: unlocking over that active native session emits vault-locked, then successfully installs another active native session");
    session.lock().unwrap();
    let failed_ticket = session.begin_open().unwrap();
    let before_failure = events.load(Ordering::SeqCst);
    let error = session.open(failed_ticket, || Vault::open(&vault_path, "fake-wrong-password").map_err(Into::into)).unwrap_err();
    assert_eq!(error.kind, "open");
    assert_eq!(events.load(Ordering::SeqCst), before_failure);
    println!("PASS: corrected wrong-password opening returns its terminal error without an extra lock event");
    println!("Evidence directory: {}", directory.display());
}
