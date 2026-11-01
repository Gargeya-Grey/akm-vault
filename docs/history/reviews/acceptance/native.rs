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
mod desktop_export;
#[path = "W:/API Keys Manager/crates/akm-cli/src/export.rs"]
mod cli_export;

use akm_core::{ApiKeyInput, Vault};
use std::{fs, path::{Path, PathBuf}, sync::{Arc, atomic::{AtomicUsize, Ordering}}};
use session::{Access, Session};
const PASSWORD: &str = "fake acceptance review password";
const BODY: &str = "TOKEN=fake-acceptance-value\n";

struct NoClipboard;
impl clipboard::ClipboardBackend for NoClipboard {
    fn set_private(&mut self, _: &str) -> Result<clipboard::CopyAttempt, error::CommandError> {
        panic!("Real clipboard access is forbidden in this probe")
    }
    fn clear_matching(&mut self, _: u32, _: &str) -> Result<bool, error::CommandError> {
        panic!("Real clipboard access is forbidden in this probe")
    }
}

fn open(session: &Session, path: &Path, password: &str) {
    let ticket = session.begin_open().unwrap();
    session.open(ticket, || Vault::open(path, password).map_err(Into::into)).unwrap();
}

fn completed<T>(result: Result<T, error::CommandError>) {
    let error = result.err().expect("locked completion must suppress its result");
    assert_eq!(error.kind, "completedWhileLocked");
    assert!(error.message.contains("Do not repeat"));
    assert!(!serde_json::to_string(&error).unwrap().contains("fake-acceptance-value"));
}

fn displace(directory: &Path) -> (PathBuf, PathBuf) {
    let temp = fs::read_dir(directory).unwrap().map(|entry| entry.unwrap().path())
        .find(|path| path.file_name().unwrap().to_string_lossy().starts_with(".akm-export-"))
        .expect("one export temp");
    let moved = directory.join("renamed-owned.tmp");
    fs::rename(&temp, &moved).unwrap();
    fs::write(&temp, b"unrelated replacement\n").unwrap();
    (temp, moved)
}

fn main() {
    let root = std::env::current_dir().unwrap().join(format!("fake-native-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let path = root.join("vault.kdbx");
    let events = Arc::new(AtomicUsize::new(0));
    let notify = events.clone();
    let session = Session::new(Arc::new(session::MonotonicClock::default()), Box::new(NoClipboard),
        Arc::new(move || { notify.fetch_add(1, Ordering::SeqCst); }));
    session.observe(policy::Signal::Available);
    let ticket = session.begin_open().unwrap();
    completed(session.open_created(ticket, || {
        let vault = Vault::create(&path, PASSWORD, None)?;
        session.lock()?;
        Ok(vault)
    }));
    assert!(session.snapshot().is_err());
    assert!(Vault::open(&path, PASSWORD).is_ok());
    println!("PASS: create remains committed with a secret-free receipt after lock");

    open(&session, &path, PASSWORD);
    completed(session.access(Access::Write, None, |vault| {
        let key = vault.add(ApiKeyInput::new("Once", "fake-acceptance-value"))?;
        session.lock()?;
        Ok(key)
    }));
    let key = Vault::open(&path, PASSWORD).unwrap().find("Once").unwrap();
    open(&session, &path, PASSWORD);
    completed(session.access(Access::Write, None, |vault| {
        vault.update(&key.id, ApiKeyInput::new("Updated once", "fake-acceptance-value"))?;
        session.lock()?;
        Ok(())
    }));
    assert_eq!(Vault::open(&path, PASSWORD).unwrap().get_by_id(&key.id).unwrap().title, "Updated once");
    open(&session, &path, PASSWORD);
    completed(session.access(Access::Write, None, |vault| {
        vault.import_env(&[("TOKEN".into(), "fake-acceptance-value".into())], Some("demo"), Some("dev"))?;
        session.lock()?;
        Ok(())
    }));
    let saved = Vault::open(&path, PASSWORD).unwrap();
    assert_eq!(saved.env_vars(Some("demo"), Some("dev"), false).unwrap(), [("TOKEN".into(), "fake-acceptance-value".into())]);
    assert!(saved.env_vars(Some("demo"), Some("prod"), false).unwrap().is_empty());
    drop(saved);
    open(&session, &path, PASSWORD);
    completed(session.access(Access::Write, None, |vault| {
        vault.remove(&key.id)?;
        session.lock()?;
        Ok(())
    }));
    assert!(Vault::open(&path, PASSWORD).unwrap().get_by_id(&key.id).is_err());
    println!("PASS: CRUD and scoped import preserve committed outcomes after lock");

    let legacy = root.join("legacy.kdbx");
    fs::copy("W:/API Keys Manager/crates/akm-core/tests/fixtures/kdbx40.kdbx", &legacy).unwrap();
    let legacy_before = fs::read(&legacy).unwrap();
    open(&session, &legacy, "correct horse battery staple");
    completed(session.access(Access::Upgrade, None, |vault| {
        vault.upgrade()?;
        session.lock()?;
        Ok(())
    }));
    assert!(!Vault::open(&legacy, "correct horse battery staple").unwrap().needs_upgrade());
    assert_eq!(fs::read(root.join("legacy.kdbx.bak")).unwrap(), legacy_before);
    println!("PASS: explicit upgrade preserves committed receipt and original backup");

    let before = events.load(Ordering::SeqCst);
    let ticket = session.begin_open().unwrap();
    assert_eq!(session.open(ticket, || Vault::open(&path, "fake-wrong-password").map_err(Into::into)).unwrap_err().kind, "open");
    assert_eq!(events.load(Ordering::SeqCst), before);
    open(&session, &path, PASSWORD);
    let secret = session.access(Access::Read, None, |_| Ok(zeroize::Zeroizing::new("fake-acceptance-value".to_string()))).unwrap();
    session.lock().unwrap();
    assert_eq!(session.reveal(secret).unwrap_err().kind, "locked");
    println!("PASS: terminal authentication error keeps its own explanation; obsolete read suppresses plaintext");

    open(&session, &path, PASSWORD);
    let directory = root.join("cancel");
    fs::create_dir(&directory).unwrap();
    let target = directory.join("out.env");
    let mut displaced = None;
    let result = session.publish(session.snapshot().unwrap(), || {
        let prepared = desktop_export::prepare_private(&target, BODY, &[], &[])?;
        displaced = Some(displace(&directory));
        session.lock()?;
        Ok(prepared)
    }, |prepared| prepared.publish());
    assert_eq!(result.unwrap_err().kind, "locked");
    let (replacement, moved) = displaced.unwrap();
    assert_eq!(fs::read(&replacement).unwrap(), b"unrelated replacement\n");
    assert!(!moved.exists() && !target.exists());
    println!("PASS: preauthorization cancellation removes the renamed owned file and preserves the replacement");

    open(&session, &path, PASSWORD);
    let directory = root.join("publish");
    fs::create_dir(&directory).unwrap();
    let target = directory.join("out.env");
    let mut displaced = None;
    completed(session.publish(session.snapshot().unwrap(), || {
        desktop_export::prepare_private(&target, BODY, &[], &[])
    }, |prepared| {
        displaced = Some(displace(&directory));
        session.lock()?;
        open(&session, &path, PASSWORD);
        prepared.publish()
    }));
    assert!(session.snapshot().is_ok(), "old export receipt must not lock new native B");
    let (replacement, moved) = displaced.unwrap();
    assert_eq!(fs::read(&replacement).unwrap(), b"unrelated replacement\n");
    assert!(!moved.exists());
    assert_eq!(fs::read_to_string(&target).unwrap(), BODY);
    println!("PASS: authorized old export publishes the owned bytes, preserves replacements, and leaves native B active");

    struct Swap;
    impl cli_export::ExportIo for Swap {
        fn publish(&mut self, file: &fs::File, destination: &Path) -> akm_core::Result<()> {
            let (replacement, moved) = displace(destination.parent().unwrap());
            cli_export::publish_new(file, destination)?;
            assert_eq!(fs::read(replacement).unwrap(), b"unrelated replacement\n");
            assert!(!moved.exists());
            Ok(())
        }
    }
    let directory = root.join("cli");
    fs::create_dir(&directory).unwrap();
    let target = cli_export::destination(&directory.join("out.env")).unwrap();
    let values = [("TOKEN".into(), "fake-acceptance-value".into())];
    cli_export::write_new_with(&target, &values, &mut Swap).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), BODY);
    assert!(cli_export::write_new(&target, &[("TOKEN".into(), "fake-other".into())]).is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), BODY);
    println!("PASS: CLI uses handle ownership and refuses to overwrite an existing destination");
    println!("Fake evidence retained at {}", root.display());
}
