use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, TryLockError},
    time::{Duration, Instant},
};

use akm_core::Vault;
use zeroize::Zeroizing;

use crate::{
    clipboard::{ClipboardBackend, ClipboardLease},
    error::CommandError,
    pending::{PendingImport, PendingImports},
    policy::{Policy, Signal, Ticket},
};

pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}

pub struct MonotonicClock(Instant);
impl Default for MonotonicClock {
    fn default() -> Self {
        Self(Instant::now())
    }
}
impl Clock for MonotonicClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub ticket: Ticket,
    pub revision: u64,
}

pub struct Stamped<T> {
    pub snapshot: Snapshot,
    pub value: T,
}

#[derive(Clone, Copy)]
pub enum Access {
    Read,
    Write,
    Reload,
    Upgrade,
}

impl Access {
    fn persisted(self) -> bool {
        matches!(self, Self::Write | Self::Upgrade)
    }

    pub fn cancelled_after_completion(self) -> CommandError {
        if self.persisted() {
            CommandError::completed_while_locked()
        } else {
            CommandError::locked()
        }
    }
}

struct Gate {
    policy: Policy,
    revision: u64,
    pending: PendingImports,
    clipboard: ClipboardLease,
    path: Option<PathBuf>,
    warnings: Vec<String>,
}

#[derive(Default)]
struct StoredVault {
    ticket: Option<Ticket>,
    vault: Option<Vault>,
}

pub struct Session {
    gate: Mutex<Gate>,
    vault: Mutex<StoredVault>,
    open_serial: Mutex<()>,
    clock: Arc<dyn Clock>,
    notify_locked: Arc<dyn Fn() + Send + Sync>,
}

#[derive(Clone)]
pub struct VaultState(pub Arc<Session>);

impl Session {
    pub fn new(
        clock: Arc<dyn Clock>,
        clipboard: Box<dyn ClipboardBackend>,
        notify_locked: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            gate: Mutex::new(Gate {
                policy: Policy::default(),
                revision: 0,
                pending: PendingImports::default(),
                clipboard: ClipboardLease::new(clipboard),
                path: None,
                warnings: Vec::new(),
            }),
            vault: Mutex::new(StoredVault::default()),
            open_serial: Mutex::new(()),
            clock,
            notify_locked,
        }
    }

    fn gate(&self) -> Result<MutexGuard<'_, Gate>, CommandError> {
        match self.gate.lock() {
            Ok(mut gate) => {
                let was_locked = gate.policy.is_locked();
                gate.policy.expire(self.clock.now());
                if !was_locked && gate.policy.is_locked() {
                    self.locked_gate(&mut gate);
                }
                Ok(gate)
            }
            Err(poisoned) => {
                let mut gate = poisoned.into_inner();
                gate.policy.observe(Signal::Unobservable);
                self.locked_gate(&mut gate);
                Err(CommandError::internal(
                    "The native session state failed. Restart AKM.",
                ))
            }
        }
    }

    fn locked_gate(&self, gate: &mut Gate) {
        gate.pending.clear();
        if let Err(error) = gate.clipboard.clear() {
            warn_once(gate, error.message);
        }
        (self.notify_locked)();
    }

    pub fn observe(&self, signal: Signal) {
        if let Ok(mut gate) = self.gate() {
            let was_locked = gate.policy.is_locked();
            gate.policy.observe(signal);
            if signal != Signal::Available {
                gate.pending.clear();
                if let Err(error) = gate.clipboard.clear() {
                    warn_once(&mut gate, error.message);
                }
                if !was_locked {
                    (self.notify_locked)();
                }
            }
        }
        self.discard_locked_vault();
    }

    pub fn lock(&self) -> Result<(), CommandError> {
        {
            let mut gate = self.gate()?;
            gate.policy.lock();
            self.locked_gate(&mut gate);
        }
        self.discard_locked_vault();
        Ok(())
    }

    fn discard_locked_vault(&self) {
        let mut stored = match self.vault.try_lock() {
            Ok(stored) => stored,
            Err(TryLockError::Poisoned(poisoned)) => {
                *poisoned.into_inner() = StoredVault::default();
                return;
            }
            Err(TryLockError::WouldBlock) => return,
        };
        let valid = self
            .gate()
            .ok()
            .and_then(|mut gate| self.current(&mut gate).ok())
            .map(|s| s.ticket);
        if stored.ticket != valid {
            *stored = StoredVault::default();
        }
    }

    pub fn tick(&self) {
        if let Ok(mut gate) = self.gate() {
            let result = if gate.policy.is_locked() {
                gate.clipboard.clear()
            } else {
                gate.clipboard.tick(self.clock.now())
            };
            if let Err(error) = result {
                warn_once(&mut gate, error.message);
            }
        }
        self.discard_locked_vault();
    }

    pub fn activity(&self) -> Result<(), CommandError> {
        let mut gate = self.gate()?;
        let was_locked = gate.policy.is_locked();
        let active = gate.policy.activity(self.clock.now());
        if !was_locked && gate.policy.is_locked() {
            self.locked_gate(&mut gate);
        }
        if !active {
            return Err(CommandError::locked());
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Result<Snapshot, CommandError> {
        let mut gate = self.gate()?;
        self.current(&mut gate)
    }

    pub fn ensure(&self, ticket: Ticket) -> Result<(), CommandError> {
        if self.snapshot()?.ticket != ticket {
            return Err(CommandError::locked());
        }
        Ok(())
    }

    pub fn path(&self) -> Result<Option<PathBuf>, CommandError> {
        Ok(self.gate()?.path.clone())
    }

    pub fn begin_open(&self) -> Result<Ticket, CommandError> {
        let ticket = {
            let mut gate = self.gate()?;
            let was_locked = gate.policy.is_locked();
            let ticket = gate.policy.begin_open(self.clock.now());
            gate.pending.clear();
            if let Err(error) = gate.clipboard.clear() {
                warn_once(&mut gate, error.message);
            }
            if !was_locked {
                (self.notify_locked)();
            }
            ticket.ok_or_else(|| {
                CommandError::new(
                    "sessionUnavailable",
                    "The Windows session is locked or its lock state cannot be observed.",
                )
            })?
        };
        self.discard_locked_vault();
        Ok(ticket)
    }

    pub fn open(
        &self,
        ticket: Ticket,
        load: impl FnOnce() -> Result<Vault, CommandError>,
    ) -> Result<(), CommandError> {
        self.open_with(ticket, false, load)
    }

    pub fn open_created(
        &self,
        ticket: Ticket,
        create: impl FnOnce() -> Result<Vault, CommandError>,
    ) -> Result<(), CommandError> {
        self.open_with(ticket, true, create)
    }

    fn open_with(
        &self,
        ticket: Ticket,
        create: bool,
        load: impl FnOnce() -> Result<Vault, CommandError>,
    ) -> Result<(), CommandError> {
        // KDFs and creation are serialized, while native lock signals remain independent.
        let _serial = self
            .open_serial
            .lock()
            .map_err(|_| CommandError::internal("The unlock worker failed. Restart AKM."))?;
        {
            let mut gate = self.gate()?;
            if !self.opening(&mut gate, ticket) {
                return Err(CommandError::locked());
            }
        }
        let loaded = load();
        let created = create && loaded.is_ok();
        let accept = || {
            let mut stored = self.vault.lock().map_err(|_| {
                let _ = self.lock();
                CommandError::internal("The vault worker failed. Restart AKM.")
            })?;
            let mut gate = self.gate()?;
            if !self.opening(&mut gate, ticket) {
                return Err(CommandError::locked());
            }
            let mut vault = match loaded {
                Ok(vault) => vault,
                Err(error) => {
                    gate.policy.lock();
                    // Opening never installed protected state; its own event would hide the terminal error.
                    gate.pending.clear();
                    *stored = StoredVault::default();
                    return Err(error);
                }
            };
            if !gate.policy.opened(ticket, self.clock.now()) {
                self.locked_gate(&mut gate);
                return Err(CommandError::locked());
            }
            gate.path = Some(vault.path().to_path_buf());
            gate.revision = 0;
            gate.pending.clear();
            gate.warnings.extend(vault.take_warnings());
            *stored = StoredVault {
                ticket: Some(ticket),
                vault: Some(vault),
            };
            Ok(())
        };
        accept().map_err(|error| {
            if created {
                CommandError::completed_while_locked()
            } else {
                error
            }
        })
    }

    pub fn access<T>(
        &self,
        access: Access,
        expected: Option<Snapshot>,
        operation: impl FnOnce(&mut Vault) -> Result<T, CommandError>,
    ) -> Result<Stamped<T>, CommandError> {
        let mut stored = self.vault.lock().map_err(|_| {
            let _ = self.lock();
            CommandError::internal("The vault worker failed. Restart AKM.")
        })?;
        let before = {
            let mut gate = self.gate()?;
            let current = self.current(&mut gate);
            match current {
                Ok(current) if Some(current.ticket) == stored.ticket => {
                    if expected.is_some_and(|s| s != current) {
                        return Err(CommandError::new(
                            "stalePreview",
                            "The session or vault changed. Start this operation again.",
                        ));
                    }
                    if matches!(access, Access::Reload | Access::Upgrade) {
                        gate.pending.clear();
                    }
                    current
                }
                _ => {
                    *stored = StoredVault::default();
                    return Err(CommandError::locked());
                }
            }
        };
        let vault = stored.vault.as_mut().ok_or_else(CommandError::locked)?;
        let result = operation(vault);
        let warnings = vault.take_warnings();
        let mut gate = match self.gate() {
            Ok(gate) => gate,
            Err(error) => {
                *stored = StoredVault::default();
                return Err(if result.is_ok() && access.persisted() {
                    CommandError::completed_while_locked()
                } else {
                    error
                });
            }
        };
        gate.warnings.extend(warnings);
        let current = self.current(&mut gate);
        if !current.is_ok_and(|s| s.ticket == before.ticket) {
            *stored = StoredVault::default();
            return Err(if result.is_ok() {
                access.cancelled_after_completion()
            } else {
                CommandError::locked()
            });
        }
        if result.is_ok() && !matches!(access, Access::Read) {
            gate.revision = gate.revision.wrapping_add(1);
            gate.pending.clear();
        }
        Ok(Stamped {
            snapshot: Snapshot {
                ticket: before.ticket,
                revision: gate.revision,
            },
            value: result?,
        })
    }

    pub fn finish<T>(&self, access: Access, completed: Stamped<T>) -> Result<T, CommandError> {
        self.check_completed(access, completed.snapshot.ticket)?;
        Ok(completed.value)
    }

    pub fn check_completed(&self, access: Access, ticket: Ticket) -> Result<(), CommandError> {
        self.ensure(ticket)
            .map_err(|_| access.cancelled_after_completion())
    }

    pub fn copy(&self, ticket: Ticket, text: Zeroizing<String>) -> Result<(), CommandError> {
        // This gate serializes copy with lock/close/expiry. No Vault mutex is held here.
        let mut gate = self.gate()?;
        if self.current(&mut gate)?.ticket != ticket {
            return Err(CommandError::locked());
        }
        gate.clipboard.copy(text, self.clock.now())?;
        if self.current(&mut gate)?.ticket != ticket {
            return Err(CommandError::locked());
        }
        Ok(())
    }

    pub fn reveal(&self, secret: Stamped<Zeroizing<String>>) -> Result<String, CommandError> {
        let mut gate = self.gate()?;
        if self.current(&mut gate)?.ticket != secret.snapshot.ticket {
            return Err(CommandError::locked());
        }
        Ok(secret.value.as_str().to_owned())
    }

    pub fn publish<P, T>(
        &self,
        expected: Snapshot,
        prepare: impl FnOnce() -> Result<P, CommandError>,
        publish: impl FnOnce(P) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        self.ensure_snapshot(expected)?;
        let prepared = prepare()?;
        // Authorization is the cancellation boundary; an in-flight OS publication cannot be undone.
        self.ensure_snapshot(expected)?;
        let value = publish(prepared)?;
        self.finish(
            Access::Write,
            Stamped {
                snapshot: expected,
                value,
            },
        )
    }

    fn ensure_snapshot(&self, expected: Snapshot) -> Result<(), CommandError> {
        if self.snapshot()? != expected {
            return Err(CommandError::new(
                "stalePreview",
                "The vault changed. Start this operation again.",
            ));
        }
        Ok(())
    }

    pub fn store_preview(
        &self,
        expected: Snapshot,
        text: Zeroizing<String>,
        project: Option<String>,
        environment: Option<String>,
    ) -> Result<String, CommandError> {
        let mut gate = self.gate()?;
        if self.current(&mut gate)? != expected {
            return Err(CommandError::new(
                "stalePreview",
                "The vault changed. Preview again.",
            ));
        }
        Ok(gate.pending.set(
            expected.ticket,
            expected.revision,
            text,
            project,
            environment,
        ))
    }

    pub fn take_preview(&self, token: &str) -> Result<Stamped<PendingImport>, CommandError> {
        let mut gate = self.gate()?;
        let current = self.current(&mut gate)?;
        let pending = gate.pending.take(token, current.ticket, current.revision)?;
        Ok(Stamped {
            snapshot: current,
            value: pending,
        })
    }

    pub fn warnings(&self) -> Result<Vec<String>, CommandError> {
        let mut gate = self.gate()?;
        self.current(&mut gate)?;
        Ok(std::mem::take(&mut gate.warnings))
    }

    pub fn warn(&self, message: String) {
        if let Ok(mut gate) = self.gate() {
            warn_once(&mut gate, message);
        }
    }

    fn current(&self, gate: &mut Gate) -> Result<Snapshot, CommandError> {
        let was_locked = gate.policy.is_locked();
        let ticket = gate.policy.active(self.clock.now());
        if !was_locked && gate.policy.is_locked() {
            self.locked_gate(gate);
        }
        Ok(Snapshot {
            ticket: ticket.ok_or_else(CommandError::locked)?,
            revision: gate.revision,
        })
    }

    fn opening(&self, gate: &mut Gate, ticket: Ticket) -> bool {
        let was_locked = gate.policy.is_locked();
        let opening = gate.policy.opening(ticket, self.clock.now());
        if !was_locked && gate.policy.is_locked() {
            self.locked_gate(gate);
        }
        opening
    }
}

fn warn_once(gate: &mut Gate, message: String) {
    if !gate.warnings.contains(&message) {
        gate.warnings.push(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::tests::{FakeClipboard, FakeData};
    use std::sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    };

    #[derive(Default)]
    struct TestClock(AtomicU64);
    impl TestClock {
        fn set(&self, seconds: u64) {
            self.0.store(seconds, Ordering::SeqCst);
        }
    }
    impl Clock for TestClock {
        fn now(&self) -> Duration {
            Duration::from_secs(self.0.load(Ordering::SeqCst))
        }
    }

    struct Fixture {
        session: Arc<Session>,
        clock: Arc<TestClock>,
        clipboard: Arc<Mutex<FakeData>>,
        events: Arc<AtomicUsize>,
        _directory: tempfile::TempDir,
    }
    fn fixture() -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let clock = Arc::new(TestClock::default());
        let clipboard = Arc::new(Mutex::new(FakeData::default()));
        let events = Arc::new(AtomicUsize::new(0));
        let notify = events.clone();
        let session = Arc::new(Session::new(
            clock.clone(),
            Box::new(FakeClipboard(clipboard.clone())),
            Arc::new(move || {
                notify.fetch_add(1, Ordering::SeqCst);
            }),
        ));
        session.observe(Signal::Available);
        let ticket = session.begin_open().unwrap();
        session
            .open(ticket, || {
                Vault::create(
                    directory.path().join("vault.kdbx"),
                    "fake-test-password-123!",
                    None,
                )
                .map_err(Into::into)
            })
            .unwrap();
        Fixture {
            session,
            clock,
            clipboard,
            events,
            _directory: directory,
        }
    }

    #[test]
    fn status_and_commands_expire_without_frontend_polling_or_activity() {
        let f = fixture();
        f.clock.set(599);
        let status = f
            .session
            .access(Access::Read, None, |vault| {
                Ok(crate::views::status_for(vault))
            })
            .unwrap();
        assert!(status.value.unlocked);
        f.clock.set(600);
        assert!(f
            .session
            .access(Access::Read, None, |vault| Ok(vault.list()))
            .is_err());
        assert!(f.session.activity().is_err());
        assert_eq!(f.events.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn lock_invalidates_a_worker_that_already_started_opening() {
        let f = fixture();
        let vault = Vault::open(
            f._directory.path().join("vault.kdbx"),
            "fake-test-password-123!",
        )
        .unwrap();
        let ticket = f.session.begin_open().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let worker_session = f.session.clone();
        let worker = std::thread::spawn(move || {
            worker_session.open(ticket, || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(vault)
            })
        });
        started_rx.recv().unwrap();
        f.session.lock().unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(worker.join().unwrap().unwrap_err().kind, "locked");
        assert!(f.session.snapshot().is_err());
    }

    #[test]
    fn a_secret_producing_operation_cannot_return_after_lock() {
        let f = fixture();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let session = f.session.clone();
        let worker = std::thread::spawn(move || {
            session.access(Access::Read, None, |_| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(Zeroizing::new("fake-delayed-secret".to_string()))
            })
        });
        started_rx.recv().unwrap();
        f.session.lock().unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(worker.join().unwrap().err().unwrap().kind, "locked");
    }

    #[test]
    fn a_committed_write_is_not_reported_as_cancelled_when_lock_overtakes_its_reply() {
        let f = fixture();
        let (committed_tx, committed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let session = f.session.clone();
        let writer = std::thread::spawn(move || {
            session.access(Access::Write, None, |vault| {
                let key = vault.add(akm_core::ApiKeyInput::new(
                    "Saved once",
                    "fake-private-result",
                ))?;
                committed_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(Zeroizing::new(key.secret))
            })
        });
        committed_rx.recv_timeout(Duration::from_secs(20)).unwrap();
        f.session.lock().unwrap();
        release_tx.send(()).unwrap();
        let error = writer.join().unwrap().err().unwrap();
        assert_eq!(error.kind, "completedWhileLocked");
        assert!(error.message.contains("Do not repeat"));
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("fake-private-result"));
        assert!(f.session.snapshot().is_err());
        let reopened = Vault::open(
            f._directory.path().join("vault.kdbx"),
            "fake-test-password-123!",
        )
        .unwrap();
        assert_eq!(reopened.list().len(), 1);
        assert_eq!(
            reopened.find("Saved once").unwrap().secret,
            "fake-private-result"
        );
    }

    #[test]
    fn stalled_export_preparation_does_not_delay_lock_or_clipboard_cleanup() {
        let f = fixture();
        let snapshot = f.session.snapshot().unwrap();
        f.session
            .copy(snapshot.ticket, Zeroizing::new("fake-copy".into()))
            .unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let session = f.session.clone();
        let exporter = std::thread::spawn(move || {
            session.publish(
                snapshot,
                || {
                    started_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(())
                },
                |_| -> Result<(), CommandError> { panic!("A locked export must not publish") },
            )
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (locked_tx, locked_rx) = mpsc::channel();
        let session = f.session.clone();
        let locker = std::thread::spawn(move || {
            session.observe(Signal::Suspend);
            locked_tx.send(()).unwrap();
        });
        let locked_before_io_finished = locked_rx.recv_timeout(Duration::from_secs(1)).is_ok();
        release_tx.send(()).unwrap();
        assert_eq!(exporter.join().unwrap().unwrap_err().kind, "locked");
        locker.join().unwrap();
        assert!(
            locked_before_io_finished,
            "suspend observation waited on export I/O"
        );
        assert!(f.session.snapshot().is_err());
        assert_eq!(f.clipboard.lock().unwrap().text, "");
    }

    #[test]
    fn command_postchecks_preserve_completion_but_never_return_cancelled_results() {
        let f = fixture();
        let snapshot = f.session.snapshot().unwrap();
        f.session.lock().unwrap();
        for (access, kind) in [
            (Access::Write, "completedWhileLocked"),
            (Access::Upgrade, "completedWhileLocked"),
            (Access::Read, "locked"),
            (Access::Reload, "locked"),
        ] {
            let result = f.session.finish(
                access,
                Stamped {
                    snapshot,
                    value: Zeroizing::new("fake-result-must-not-return".to_string()),
                },
            );
            let error = result.err().unwrap();
            assert_eq!(error.kind, kind);
            assert!(!serde_json::to_string(&error)
                .unwrap()
                .contains("fake-result"));
        }
    }

    #[test]
    fn vault_creation_overtaken_by_lock_reports_completion_without_opening_a_session() {
        let f = fixture();
        let target = f._directory.path().join("created.kdbx");
        let ticket = f.session.begin_open().unwrap();
        let (created_tx, created_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let session = f.session.clone();
        let path = target.clone();
        let creator = std::thread::spawn(move || {
            session.open_created(ticket, || {
                let vault = Vault::create(path, "fake-created-password-123!", None)?;
                created_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(vault)
            })
        });
        created_rx.recv_timeout(Duration::from_secs(20)).unwrap();
        f.session.lock().unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(
            creator.join().unwrap().unwrap_err().kind,
            "completedWhileLocked"
        );
        assert!(f.session.snapshot().is_err());
        assert!(Vault::open(target, "fake-created-password-123!").is_ok());
    }

    #[test]
    fn a_failed_opening_returns_its_error_without_an_extra_lock_transition() {
        let f = fixture();
        let ticket = f.session.begin_open().unwrap();
        let before = f.events.load(Ordering::SeqCst);
        let result = f.session.open(ticket, || {
            Vault::open(
                f._directory.path().join("vault.kdbx"),
                "fake-wrong-password",
            )
            .map_err(Into::into)
        });
        assert_eq!(result.unwrap_err().kind, "open");
        assert!(f.session.snapshot().is_err());
        assert_eq!(
            f.events.load(Ordering::SeqCst),
            before,
            "a failed opening never installed protected state"
        );
    }

    #[test]
    fn export_cancelled_after_preparation_removes_plaintext_without_publishing() {
        let f = fixture();
        let snapshot = f.session.snapshot().unwrap();
        let target = f._directory.path().join("out.env");
        let (prepared_tx, prepared_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let session = f.session.clone();
        let path = target.clone();
        let exporter = std::thread::spawn(move || {
            session.publish(
                snapshot,
                || {
                    let prepared =
                        crate::private_export::prepare_private(&path, "TOKEN=fake\n", &[], &[])?;
                    prepared_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(prepared)
                },
                |prepared| prepared.publish(),
            )
        });
        prepared_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        f.session.observe(Signal::Locked);
        release_tx.send(()).unwrap();
        assert_eq!(exporter.join().unwrap().unwrap_err().kind, "locked");
        assert!(!target.exists());
        assert!(!std::fs::read_dir(f._directory.path())
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "tmp")));
    }

    #[test]
    fn lock_during_authorized_publication_does_not_block_and_preserves_its_receipt() {
        let f = fixture();
        let snapshot = f.session.snapshot().unwrap();
        let target = f._directory.path().join("out.env");
        let (authorized_tx, authorized_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let session = f.session.clone();
        let path = target.clone();
        let exporter = std::thread::spawn(move || {
            session.publish(
                snapshot,
                || crate::private_export::prepare_private(&path, "TOKEN=fake\n", &[], &[]),
                |prepared| {
                    authorized_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    prepared.publish()
                },
            )
        });
        authorized_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (locked_tx, locked_rx) = mpsc::channel();
        let session = f.session.clone();
        let locker = std::thread::spawn(move || {
            session.lock().unwrap();
            locked_tx.send(()).unwrap();
        });
        let prompt_lock = locked_rx.recv_timeout(Duration::from_secs(1)).is_ok();
        release_tx.send(()).unwrap();
        let result = exporter.join().unwrap();
        locker.join().unwrap();
        assert!(prompt_lock);
        assert_eq!(result.unwrap_err().kind, "completedWhileLocked");
        assert_eq!(std::fs::read_to_string(target).unwrap(), "TOKEN=fake\n");
        assert!(f.session.snapshot().is_err());
    }

    #[test]
    fn locking_clears_only_the_sessions_clipboard_and_invalidates_imports() {
        let f = fixture();
        let stamp = f.session.snapshot().unwrap();
        f.session
            .copy(stamp.ticket, Zeroizing::new("fake".into()))
            .unwrap();
        let token = f
            .session
            .store_preview(
                stamp,
                Zeroizing::new("TOKEN=fake".into()),
                Some("app".into()),
                Some("dev".into()),
            )
            .unwrap();
        f.session.lock().unwrap();
        assert_eq!(f.clipboard.lock().unwrap().text, "");
        assert!(f.session.take_preview(&token).is_err());
        assert!(f
            .session
            .copy(stamp.ticket, Zeroizing::new("late".into()))
            .is_err());

        let f = fixture();
        let stamp = f.session.snapshot().unwrap();
        f.session
            .copy(stamp.ticket, Zeroizing::new("fake".into()))
            .unwrap();
        f.clipboard.lock().unwrap().sequence += 1;
        f.session.lock().unwrap();
        assert_eq!(f.clipboard.lock().unwrap().text, "fake");
    }

    #[test]
    fn intervening_writes_and_reload_discard_previews() {
        let f = fixture();
        let stamp = f.session.snapshot().unwrap();
        let token = f
            .session
            .store_preview(
                stamp,
                Zeroizing::new("TOKEN=fake".into()),
                Some("app".into()),
                Some("dev".into()),
            )
            .unwrap();
        f.session
            .access(Access::Write, None, |vault| {
                vault.add(akm_core::ApiKeyInput::new("Other", "fake-other"))?;
                Ok(())
            })
            .unwrap();
        assert!(f.session.take_preview(&token).is_err());
        let stamp = f.session.snapshot().unwrap();
        let token = f
            .session
            .store_preview(
                stamp,
                Zeroizing::new("TOKEN=fake".into()),
                Some("app".into()),
                Some("dev".into()),
            )
            .unwrap();
        f.session
            .access(Access::Reload, None, |vault| {
                vault.reload().map_err(Into::into)
            })
            .unwrap();
        assert!(f.session.take_preview(&token).is_err());
    }

    #[test]
    fn poisoned_vault_mutex_returns_an_error_and_locks_instead_of_panicking() {
        let f = fixture();
        let session = f.session.clone();
        assert!(std::thread::spawn(move || {
            let _: Result<Stamped<()>, _> =
                session.access(Access::Read, None, |_| panic!("injected worker panic"));
        })
        .join()
        .is_err());
        assert!(f
            .session
            .access(Access::Read, None, |vault| Ok(vault.list()))
            .is_err());
        assert!(f.session.snapshot().is_err());
        assert!(f.session.path().is_ok());
    }

    struct BlockingClipboard {
        data: Arc<Mutex<FakeData>>,
        started: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        calls: usize,
        block_on: usize,
    }
    impl ClipboardBackend for BlockingClipboard {
        fn set_private(
            &mut self,
            text: &str,
        ) -> Result<crate::clipboard::CopyAttempt, CommandError> {
            self.calls += 1;
            if self.calls == self.block_on {
                self.started.send(()).unwrap();
                self.release.recv().unwrap();
            }
            FakeClipboard(self.data.clone()).set_private(text)
        }
        fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
            FakeClipboard(self.data.clone()).clear_matching(sequence, text)
        }
    }

    #[test]
    fn closing_serializes_with_a_copy_in_progress_and_clears_it() {
        let f = fixture();
        let stamp = f.session.snapshot().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        f.session.gate().unwrap().clipboard = ClipboardLease::new(Box::new(BlockingClipboard {
            data: f.clipboard.clone(),
            started: started_tx,
            release: release_rx,
            calls: 0,
            block_on: 1,
        }));
        let session = f.session.clone();
        let copy =
            std::thread::spawn(move || session.copy(stamp.ticket, Zeroizing::new("fake".into())));
        started_rx.recv().unwrap();
        let session = f.session.clone();
        let close = std::thread::spawn(move || session.lock());
        release_tx.send(()).unwrap();
        copy.join().unwrap().unwrap();
        close.join().unwrap().unwrap();
        assert_eq!(f.clipboard.lock().unwrap().text, "");
        assert_eq!(f.clipboard.lock().unwrap().clears, 1);
        assert!(f.session.snapshot().is_err());
    }

    #[test]
    fn an_expired_timer_racing_a_new_copy_cannot_clear_the_new_copy() {
        let f = fixture();
        let stamp = f.session.snapshot().unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        f.session.gate().unwrap().clipboard = ClipboardLease::new(Box::new(BlockingClipboard {
            data: f.clipboard.clone(),
            started: started_tx,
            release: release_rx,
            calls: 0,
            block_on: 2,
        }));
        f.session
            .copy(stamp.ticket, Zeroizing::new("old-fake".into()))
            .unwrap();
        f.clock.set(20);
        let session = f.session.clone();
        let copy = std::thread::spawn(move || {
            session.copy(stamp.ticket, Zeroizing::new("new-fake".into()))
        });
        started_rx.recv().unwrap();
        f.clock.set(30);
        let session = f.session.clone();
        let timer = std::thread::spawn(move || session.tick());
        release_tx.send(()).unwrap();
        copy.join().unwrap().unwrap();
        timer.join().unwrap();
        assert_eq!(f.clipboard.lock().unwrap().text, "new-fake");
        f.clock.set(50);
        f.session.tick();
        assert_eq!(f.clipboard.lock().unwrap().text, "");
    }
}
