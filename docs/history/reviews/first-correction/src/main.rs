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
#[path = "W:/API Keys Manager/crates/akm-cli/src/export.rs"]
mod cli_export;
#[path = "W:/API Keys Manager/desktop/src-tauri/src/private_export.rs"]
mod gui_export;

use akm_core::{ApiKeyInput, Vault};
use fs2::FileExt;
use std::{fs, sync::Arc, time::{Duration, Instant}};

struct NoClipboard;
impl clipboard::ClipboardBackend for NoClipboard {
    fn set_private(&mut self, _: &str) -> Result<clipboard::CopyAttempt, error::CommandError> {
        panic!("Probe must not touch clipboard")
    }
    fn clear_matching(&mut self, _: u32, _: &str) -> Result<bool, error::CommandError> { Ok(false) }
}

fn main() {
    let root = std::env::current_dir().unwrap();
    let path = root.join(format!("cancel-{}.kdbx", uuid::Uuid::new_v4()));
    let password = "fake independent review password";
    let session = Arc::new(session::Session::new(
        Arc::new(session::MonotonicClock::default()), Box::new(NoClipboard), Arc::new(|| {}),
    ));
    session.observe(policy::Signal::Available);
    let ticket = session.begin_open().unwrap();
    session.open(ticket, || Vault::create(&path, password, None).map_err(Into::into)).unwrap();
    let initial = fs::read(&path).unwrap();
    let journal_lock = fs::OpenOptions::new().create_new(true).read(true).write(true)
        .open(format!("{}.audit.jsonl.lock", path.display())).unwrap();
    journal_lock.lock_exclusive().unwrap();
    let worker_session = session.clone();
    let writer = std::thread::spawn(move || worker_session.access(session::Access::Write, None, |vault| {
        let saved = vault.add(ApiKeyInput::new("Committed before lock", "fake-review-secret"))?;
        Ok(saved.id)
    }));
    let deadline = Instant::now() + Duration::from_secs(20);
    while fs::read(&path).unwrap() == initial {
        assert!(Instant::now() < deadline, "write did not reach commit");
        std::thread::yield_now();
    }
    session.lock().unwrap();
    drop(journal_lock);
    let result = writer.join().unwrap();
    let error = result.err().expect("current source unexpectedly reported success");
    let persisted = Vault::open(&path, password).unwrap().find("Committed before lock").unwrap();
    assert_eq!(persisted.secret, "fake-review-secret");
    assert_eq!(error.kind, "locked");
    println!("REPRODUCED: committed mutation returned {} while reopened vault contains the change", error.kind);

    let export_root = root.join(format!("export-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&export_root).unwrap();
    for name in ["vault.kdbx.bak.env", "vault.kdbx.lock", "out.cmd"] {
        let raw = export_root.join(name);
        let cli = cli_export::destination(&raw).unwrap();
        cli_export::write_new(&cli, &[("TOKEN".into(), "fake-review-secret".into())]).unwrap();
        let gui_refuses = gui_export::validate_destination(&raw, &[export_root.join("vault.kdbx")], &[]).is_err();
        assert!(gui_refuses);
        println!("OBSERVED: CLI writes {name}; GUI rejects that destination");
    }

    let ticket = session.begin_open().unwrap();
    session.open(ticket, || Vault::open(&path, password).map_err(Into::into)).unwrap();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let exporter_session = session.clone();
    let exporter = std::thread::spawn(move || exporter_session.while_active(ticket, || {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        Ok(())
    }));
    started_rx.recv().unwrap();
    let (lock_started_tx, lock_started_rx) = std::sync::mpsc::channel();
    let (lock_done_tx, lock_done_rx) = std::sync::mpsc::channel();
    let locking_session = session.clone();
    let locker = std::thread::spawn(move || {
        lock_started_tx.send(()).unwrap();
        locking_session.observe(policy::Signal::Locked);
        lock_done_tx.send(()).unwrap();
    });
    lock_started_rx.recv().unwrap();
    assert!(lock_done_rx.recv_timeout(Duration::from_millis(200)).is_err());
    release_tx.send(()).unwrap();
    exporter.join().unwrap().unwrap();
    locker.join().unwrap();
    assert!(session.snapshot().is_err());
    println!("REPRODUCED: an in-progress while_active export blocks native lock handling until its filesystem closure returns");
}
