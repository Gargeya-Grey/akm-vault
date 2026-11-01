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
mod gui_export;
#[path = "W:/API Keys Manager/crates/akm-cli/src/export.rs"]
mod cli_export;

use akm_core::{ApiKeyInput, Vault};
use std::{fs, path::{Path, PathBuf}, sync::Arc};

const PASSWORD: &str = "fake independent correction review password";
const BODY: &str = "TOKEN=fake-independent-review-secret\n";

struct NoClipboard;
impl clipboard::ClipboardBackend for NoClipboard {
    fn set_private(&mut self, _: &str) -> Result<clipboard::CopyAttempt, error::CommandError> {
        panic!("The reviewer must never access the real clipboard")
    }
    fn clear_matching(&mut self, _: u32, _: &str) -> Result<bool, error::CommandError> { Ok(false) }
}

fn session_for(path: &Path) -> session::Session {
    let session = session::Session::new(
        Arc::new(session::MonotonicClock::default()), Box::new(NoClipboard), Arc::new(|| {}),
    );
    session.observe(policy::Signal::Available);
    let ticket = session.begin_open().unwrap();
    session.open(ticket, || Vault::create(path, PASSWORD, None).map_err(Into::into)).unwrap();
    session
}

fn open_again(session: &session::Session, path: &Path, password: &str) {
    let ticket = session.begin_open().unwrap();
    session.open(ticket, || Vault::open(path, password).map_err(Into::into)).unwrap();
}

fn assert_completed<T>(result: Result<T, error::CommandError>) {
    let error = result.err().expect("Result must be suppressed after lock");
    assert_eq!(error.kind, "completedWhileLocked");
    assert!(error.message.contains("Do not repeat"));
    assert!(!error.message.contains("fake-independent-review-secret"));
}

fn only_temp(path: &Path) -> PathBuf {
    let mut files: Vec<_> = fs::read_dir(path).unwrap().map(|e| e.unwrap().path()).filter(|p| {
        p.file_name().unwrap().to_string_lossy().starts_with(".akm-export-")
    }).collect();
    assert_eq!(files.len(), 1);
    files.remove(0)
}

fn main() {
    let root = std::env::current_dir().unwrap().join(format!("fake-probes-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let vault_path = root.join("vault.kdbx");
    let session = session_for(&vault_path);
    assert_completed(session.access(session::Access::Write, None, |vault| {
        let key = vault.add(ApiKeyInput::new("Saved once", "fake-independent-review-secret"))?;
        session.lock()?;
        Ok(key)
    }));
    let original = Vault::open(&vault_path, PASSWORD).unwrap().find("Saved once").unwrap();
    assert_eq!(original.secret, "fake-independent-review-secret");
    open_again(&session, &vault_path, PASSWORD);
    assert_completed(session.access(session::Access::Write, None, |vault| {
        vault.update(&original.id, ApiKeyInput::new("Updated once", "fake-updated-secret"))?;
        session.lock()?;
        Ok(())
    }));
    assert_eq!(Vault::open(&vault_path, PASSWORD).unwrap().get_by_id(&original.id).unwrap().title, "Updated once");
    open_again(&session, &vault_path, PASSWORD);
    assert_completed(session.access(session::Access::Write, None, |vault| {
        vault.import_env(&[("TOKEN".into(), "fake-imported".into())], Some("demo"), Some("dev"))?;
        session.lock()?;
        Ok(())
    }));
    assert_eq!(Vault::open(&vault_path, PASSWORD).unwrap().env_vars(Some("demo"), Some("dev"), false).unwrap(), [("TOKEN".into(), "fake-imported".into())]);
    open_again(&session, &vault_path, PASSWORD);
    assert_completed(session.access(session::Access::Write, None, |vault| {
        vault.remove(&original.id)?;
        session.lock()?;
        Ok(())
    }));
    assert!(Vault::open(&vault_path, PASSWORD).unwrap().get_by_id(&original.id).is_err());
    println!("PASS: add/update/import/remove remain committed and return secret-free completedWhileLocked after lock");

    let legacy = root.join("legacy.kdbx");
    fs::copy("W:/API Keys Manager/crates/akm-core/tests/fixtures/kdbx40.kdbx", &legacy).unwrap();
    open_again(&session, &legacy, "correct horse battery staple");
    assert_completed(session.access(session::Access::Upgrade, None, |vault| {
        vault.upgrade()?;
        session.lock()?;
        Ok(())
    }));
    assert!(!Vault::open(&legacy, "correct horse battery staple").unwrap().needs_upgrade());
    let created = root.join("created.kdbx");
    let ticket = session.begin_open().unwrap();
    assert_completed(session.open_created(ticket, || {
        let vault = Vault::create(&created, PASSWORD, None)?;
        session.lock()?;
        Ok(vault)
    }));
    assert!(Vault::open(&created, PASSWORD).is_ok());
    println!("PASS: upgrade/create preserve committed receipts without reopening a locked session");

    open_again(&session, &vault_path, PASSWORD);
    let export_dir = root.join("normal-cancellation");
    fs::create_dir(&export_dir).unwrap();
    let target = export_dir.join("out.env");
    let result = session.publish(session.snapshot().unwrap(), || {
        let prepared = gui_export::prepare_private(&target, BODY, &[], &[])?;
        session.lock()?;
        Ok(prepared)
    }, |prepared| prepared.publish());
    assert_eq!(result.unwrap_err().kind, "locked");
    assert_eq!(fs::read_dir(&export_dir).unwrap().count(), 0);
    open_again(&session, &vault_path, PASSWORD);
    assert_completed(session.publish(session.snapshot().unwrap(), || {
        gui_export::prepare_private(&target, BODY, &[], &[])
    }, |prepared| {
        session.lock()?;
        prepared.publish()
    }));
    assert_eq!(fs::read_to_string(&target).unwrap(), BODY);
    println!("PASS: ordinary preauthorization export cancellation cleans temp; authorized publication preserves completion");

    for succeed in [false, true] {
        let directory = root.join(if succeed { "gui-renamed-publish" } else { "gui-renamed-cancel" });
        fs::create_dir(&directory).unwrap();
        let target = directory.join("out.env");
        let prepared = gui_export::prepare_private(&target, BODY, &[], &[]).unwrap();
        let temp = only_temp(&directory);
        let moved = directory.join("renamed-owned.tmp");
        fs::rename(&temp, &moved).unwrap();
        fs::write(&temp, "unrelated replacement file\n").unwrap();
        if succeed {
            assert_eq!(prepared.publish().unwrap(), target);
            assert_eq!(fs::read_to_string(&target).unwrap(), "unrelated replacement file\n");
            assert!(!temp.exists());
            println!("REPRODUCED: GUI published the unrelated replacement as a successful export");
        } else {
            drop(prepared);
            assert!(!temp.exists());
            assert!(!target.exists());
            println!("REPRODUCED: GUI cancellation deleted the unrelated replacement temp-name file");
        }
        assert_eq!(fs::read_to_string(&moved).unwrap(), BODY);
        println!("REPRODUCED: GUI left the renamed owned plaintext behind");
    }

    struct ReplacedTemp { moved: PathBuf, replacement: Option<PathBuf> }
    impl cli_export::ExportIo for ReplacedTemp {
        fn publish(&mut self, file: &fs::File, destination: &Path) -> akm_core::Result<()> {
            let temp = only_temp(destination.parent().unwrap());
            fs::rename(&temp, &self.moved)?;
            fs::write(&temp, "unrelated replacement file\n")?;
            self.replacement = Some(temp);
            cli_export::publish_new(file, destination)
        }
    }
    let directory = root.join("cli-renamed-publish");
    fs::create_dir(&directory).unwrap();
    let target = directory.join("out.env");
    let mut io = ReplacedTemp { moved: directory.join("renamed-owned.tmp"), replacement: None };
    cli_export::write_new_with(&target, &[("TOKEN".into(), "fake-independent-review-secret".into())], &mut io).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), BODY);
    assert_eq!(fs::read_to_string(io.replacement.unwrap()).unwrap(), "unrelated replacement file\n");
    assert!(!io.moved.exists());
    println!("PASS: corrected CLI retains file identity through this same interleaving");

    use std::sync::atomic::{AtomicUsize, Ordering};
    let lock_events = Arc::new(AtomicUsize::new(0));
    let events = lock_events.clone();
    let failed_open = session::Session::new(
        Arc::new(session::MonotonicClock::default()), Box::new(NoClipboard),
        Arc::new(move || { events.fetch_add(1, Ordering::SeqCst); }),
    );
    failed_open.observe(policy::Signal::Available);
    let ticket = failed_open.begin_open().unwrap();
    assert_eq!(lock_events.load(Ordering::SeqCst), 0);
    let error = failed_open.open(ticket, || {
        Vault::open(&vault_path, "fake-wrong-password").map_err(Into::into)
    }).unwrap_err();
    assert_eq!(error.kind, "open");
    assert_eq!(lock_events.load(Ordering::SeqCst), 1);
    println!("CONFIRMED: a real wrong-password open emits vault-locked before returning its authentication error");
    println!("Evidence directory: {}", root.display());
}
