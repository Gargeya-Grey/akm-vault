use std::{
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, TryLockError},
    time::{Duration, Instant},
};

use akm_core::{
    vault::{VaultWarning, VaultWarningKind},
    Vault,
};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::{
    clipboard::{CleanupFailure, ClipboardBackend, ClipboardLease},
    error::CommandError,
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
    pub presentation: u64,
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

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OperationWarningKind {
    Journal,
    BackupRefresh,
    BackupPrune,
    BackupRetention,
    ExportJournal,
    Preference,
    ImportApplied,
    ImportSessionEnded,
    ImportFailed,
    ImportDeclined,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationNotice {
    id: String,
    origin_session: String,
    kind: OperationWarningKind,
}

#[derive(Serialize)]
pub struct WarningReport {
    application: Vec<ApplicationNotice>,
    operations: Vec<OperationNotice>,
}

#[derive(Clone, Serialize)]
pub struct ApplicationNotice {
    id: String,
    message: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeLockNotice {
    pub session_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeViewNotice {
    pub session_id: String,
    pub hide_id: String,
}

struct NativeOwner {
    id: uuid::Uuid,
    ticket: Ticket,
    claimed: bool,
}

struct Gate {
    policy: Policy,
    owner: Option<NativeOwner>,
    revision: u64,
    presentation: u64,
    clipboard: ClipboardLease,
    cleanup_episode: Option<u64>,
    path: Option<PathBuf>,
    warnings: Vec<ApplicationNotice>,
    notices: Vec<OperationNotice>,
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
    notify_locked: Arc<dyn Fn(NativeLockNotice) + Send + Sync>,
}

#[derive(Clone)]
pub struct VaultState(pub Arc<Session>);

impl Session {
    pub fn new(
        clock: Arc<dyn Clock>,
        clipboard: Box<dyn ClipboardBackend>,
        notify_locked: Arc<dyn Fn(NativeLockNotice) + Send + Sync>,
    ) -> Self {
        Self {
            gate: Mutex::new(Gate {
                policy: Policy::default(),
                owner: None,
                revision: 0,
                presentation: 0,
                clipboard: ClipboardLease::new(clipboard),
                cleanup_episode: None,
                path: None,
                warnings: Vec::new(),
                notices: Vec::new(),
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
        cleanup_clipboard(gate, None);
        self.retire_owner(gate);
    }

    fn retire_owner(&self, gate: &mut Gate) {
        if let Some(owner) = gate.owner.take() {
            (self.notify_locked)(NativeLockNotice {
                session_id: owner.id.to_string(),
            });
        }
    }

    pub fn observe(&self, signal: Signal) {
        if let Ok(mut gate) = self.gate() {
            gate.policy.observe(signal);
            if signal != Signal::Available {
                self.locked_gate(&mut gate);
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

    pub fn lock_owned(&self, expected: Option<&str>) -> Result<(), CommandError> {
        let expected = expected
            .map(uuid::Uuid::parse_str)
            .transpose()
            .map_err(|_| CommandError::locked())?;
        {
            let mut gate = self.gate()?;
            if gate.owner.as_ref().map(|owner| owner.id) != expected {
                return Err(CommandError::locked());
            }
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
            let due = (!gate.policy.is_locked()).then(|| self.clock.now());
            cleanup_clipboard(&mut gate, due);
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

    #[cfg(test)]
    pub fn begin_open(&self) -> Result<Ticket, CommandError> {
        self.start_open().map(|(ticket, _)| ticket)
    }

    pub fn prepare_open(&self) -> Result<String, CommandError> {
        self.start_open().map(|(_, owner)| owner.to_string())
    }

    fn start_open(&self) -> Result<(Ticket, uuid::Uuid), CommandError> {
        let reservation = {
            let mut gate = self.gate()?;
            let ticket = gate.policy.begin_open(self.clock.now());
            cleanup_clipboard(&mut gate, None);
            self.retire_owner(&mut gate);
            let ticket = ticket.ok_or_else(|| {
                CommandError::new(
                    "sessionUnavailable",
                    "The Windows session is locked or its lock state cannot be observed.",
                )
            })?;
            let id = uuid::Uuid::new_v4();
            gate.owner = Some(NativeOwner {
                id,
                ticket,
                claimed: false,
            });
            (ticket, id)
        };
        self.discard_locked_vault();
        Ok(reservation)
    }

    pub fn claim_open(&self, session_id: &str) -> Result<Ticket, CommandError> {
        let id = uuid::Uuid::parse_str(session_id).map_err(|_| CommandError::locked())?;
        let mut gate = self.gate()?;
        let ticket = gate
            .owner
            .as_ref()
            .filter(|owner| owner.id == id && !owner.claimed)
            .map(|owner| owner.ticket)
            .ok_or_else(CommandError::locked)?;
        if !self.opening(&mut gate, ticket) {
            return Err(CommandError::locked());
        }
        if let Some(owner) = &mut gate.owner {
            owner.claimed = true;
        }
        Ok(ticket)
    }

    pub fn active_owner(&self, ticket: Ticket) -> Result<String, CommandError> {
        let mut gate = self.gate()?;
        if self.current(&mut gate)?.ticket != ticket {
            return Err(CommandError::locked());
        }
        gate.owner
            .as_ref()
            .filter(|owner| owner.ticket == ticket)
            .map(|owner| owner.id.to_string())
            .ok_or_else(CommandError::locked)
    }

    pub fn expire_presentation(&self) -> Result<Option<NativeViewNotice>, CommandError> {
        let mut gate = self.gate()?;
        let Ok(current) = self.current(&mut gate) else {
            return Ok(None);
        };
        let session_id = gate
            .owner
            .as_ref()
            .filter(|owner| owner.ticket == current.ticket)
            .map(|owner| owner.id.to_string())
            .ok_or_else(CommandError::locked)?;
        gate.presentation = gate.presentation.wrapping_add(1);
        Ok(Some(NativeViewNotice {
            session_id,
            hide_id: uuid::Uuid::new_v4().to_string(),
        }))
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
                    gate.owner = None;
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
            gate.presentation = 0;
            core_notices(&mut gate, ticket, vault.take_warning_records());
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
        self.access_checked(access, expected, true, operation)
    }

    /// File-save extraction uses the immutable approved export authority, not
    /// the lifetime of an on-screen preview. This never authorizes UI reveal or
    /// clipboard publication; the file path/ACL and publication checks follow.
    pub fn read_for_publication<T>(
        &self,
        expected: Snapshot,
        operation: impl FnOnce(&mut Vault) -> Result<T, CommandError>,
    ) -> Result<Stamped<T>, CommandError> {
        self.access_checked(Access::Read, Some(expected), false, operation)
    }

    fn access_checked<T>(
        &self,
        access: Access,
        expected: Option<Snapshot>,
        presentation_bound: bool,
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
                    if expected.is_some_and(|s| {
                        s.ticket != current.ticket
                            || s.revision != current.revision
                            || (presentation_bound && s.presentation != current.presentation)
                    }) {
                        return Err(CommandError::new(
                            "stalePreview",
                            "The session or vault changed. Start this operation again.",
                        ));
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
        let warnings = vault.take_warning_records();
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
        core_notices(&mut gate, before.ticket, warnings);
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
        }
        Ok(Stamped {
            snapshot: Snapshot {
                ticket: before.ticket,
                revision: gate.revision,
                // A concurrent hide must not re-stamp an old read/review as if
                // it originated in the new presentation. Writes still finish
                // truthfully; their completion check is session-owned instead.
                presentation: before.presentation,
            },
            value: result?,
        })
    }

    pub fn finish<T>(&self, access: Access, completed: Stamped<T>) -> Result<T, CommandError> {
        let mut gate = self
            .gate()
            .map_err(|_| access.cancelled_after_completion())?;
        let current = self
            .current(&mut gate)
            .map_err(|_| access.cancelled_after_completion())?;
        if current.ticket != completed.snapshot.ticket {
            return Err(access.cancelled_after_completion());
        }
        if matches!(access, Access::Read) && current.presentation != completed.snapshot.presentation
        {
            return Err(view_changed());
        }
        Ok(completed.value)
    }

    pub fn check_completed(&self, access: Access, ticket: Ticket) -> Result<(), CommandError> {
        self.ensure(ticket)
            .map_err(|_| access.cancelled_after_completion())
    }

    #[cfg(test)]
    pub fn copy(&self, ticket: Ticket, text: Zeroizing<String>) -> Result<(), CommandError> {
        self.copy_value(ticket, None, text)
    }

    pub fn copy_stamped(&self, secret: Stamped<Zeroizing<String>>) -> Result<(), CommandError> {
        self.copy_value(
            secret.snapshot.ticket,
            Some(secret.snapshot.presentation),
            secret.value,
        )
    }

    fn copy_value(
        &self,
        ticket: Ticket,
        presentation: Option<u64>,
        text: Zeroizing<String>,
    ) -> Result<(), CommandError> {
        // This gate serializes copy with lock/close/expiry. No Vault mutex is held here.
        let mut gate = self.gate()?;
        let current = self.current(&mut gate)?;
        if current.ticket != ticket {
            return Err(CommandError::locked());
        }
        if presentation.is_some_and(|expected| expected != current.presentation) {
            return Err(view_changed());
        }
        match gate.clipboard.copy_reported(text, self.clock.now()) {
            Ok(_) => account_cleanup(&mut gate, None),
            Err(failure) => {
                account_cleanup(&mut gate, failure.cleanup);
                return Err(failure.error);
            }
        }
        let current = self.current(&mut gate)?;
        if current.ticket != ticket {
            return Err(CommandError::locked());
        }
        if presentation.is_some_and(|expected| expected != current.presentation) {
            return Err(view_changed());
        }
        Ok(())
    }

    pub fn reveal(&self, secret: Stamped<Zeroizing<String>>) -> Result<String, CommandError> {
        let mut gate = self.gate()?;
        let current = self.current(&mut gate)?;
        if current.ticket != secret.snapshot.ticket {
            return Err(CommandError::locked());
        }
        if current.presentation != secret.snapshot.presentation {
            return Err(view_changed());
        }
        Ok(secret.value.as_str().to_owned())
    }

    pub fn publish<P, T>(
        &self,
        expected: Snapshot,
        prepare: impl FnOnce() -> Result<P, CommandError>,
        publish: impl FnOnce(P) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        self.ensure_publication(expected)?;
        let prepared = prepare()?;
        // Authorization is the cancellation boundary; an in-flight OS publication cannot be undone.
        self.ensure_publication(expected)?;
        let value = publish(prepared)?;
        self.finish(
            Access::Write,
            Stamped {
                snapshot: expected,
                value,
            },
        )
    }

    fn ensure_publication(&self, expected: Snapshot) -> Result<(), CommandError> {
        let current = self.snapshot()?;
        // Presentation expiry hides local results; it does not revoke an
        // approved export scope/destination. Keep native policy and vault
        // revision as the original filesystem publication authority.
        if current.ticket != expected.ticket || current.revision != expected.revision {
            return Err(CommandError::new(
                "stalePreview",
                "The vault changed. Start this operation again.",
            ));
        }
        Ok(())
    }

    /// An import request names the session it was made for. The reply of an older
    /// session, or a request that arrives after a newer opening, is refused here.
    pub fn begin_import(&self, session_id: &str) -> Result<Snapshot, CommandError> {
        let id = uuid::Uuid::parse_str(session_id).map_err(|_| CommandError::locked())?;
        let mut gate = self.gate()?;
        let current = self.current(&mut gate)?;
        gate.owner
            .as_ref()
            .filter(|owner| owner.ticket == current.ticket && owner.id == id)
            .ok_or_else(CommandError::locked)?;
        Ok(current)
    }

    /// The record belongs to the application, not to a session: it must survive a
    /// lock and a lost reply. It holds an opaque operation id and a fixed kind only.
    pub fn record_import(&self, ticket: Ticket, operation_id: &str, kind: OperationWarningKind) {
        if let Ok(mut gate) = self.gate() {
            gate.notices.push(OperationNotice {
                id: operation_id.to_string(),
                origin_session: ticket.0.to_string(),
                kind,
            });
        }
    }

    /// A page load means the document that held any on-screen result is gone.
    pub fn document_started(&self) {
        if let Ok(mut gate) = self.gate() {
            gate.presentation = gate.presentation.wrapping_add(1);
        }
    }

    pub fn warnings(&self) -> Result<WarningReport, CommandError> {
        let gate = self.gate()?;
        Ok(WarningReport {
            application: gate.warnings.clone(),
            operations: gate.notices.clone(),
        })
    }

    pub fn dismiss_warnings(&self, ids: &[String]) -> Result<(), CommandError> {
        let mut gate = self.gate()?;
        gate.notices.retain(|notice| !ids.contains(&notice.id));
        gate.warnings.retain(|notice| !ids.contains(&notice.id));
        Ok(())
    }

    pub fn warn_for(&self, ticket: Ticket, kind: OperationWarningKind) {
        if let Ok(mut gate) = self.gate() {
            operation_notice(&mut gate, ticket, kind);
        }
    }

    pub fn warn(&self, message: String) {
        if let Ok(mut gate) = self.gate() {
            application_warning(&mut gate, message);
        }
    }

    fn current(&self, gate: &mut Gate) -> Result<Snapshot, CommandError> {
        let was_locked = gate.policy.is_locked();
        let ticket = gate.policy.active(self.clock.now());
        if !was_locked && gate.policy.is_locked() {
            self.locked_gate(gate);
        }
        let ticket = ticket.ok_or_else(CommandError::locked)?;
        if !gate
            .owner
            .as_ref()
            .is_some_and(|owner| owner.ticket == ticket)
        {
            gate.policy.lock();
            self.locked_gate(gate);
            return Err(CommandError::locked());
        }
        Ok(Snapshot {
            ticket,
            revision: gate.revision,
            presentation: gate.presentation,
        })
    }

    fn opening(&self, gate: &mut Gate, ticket: Ticket) -> bool {
        let was_locked = gate.policy.is_locked();
        let opening = gate.policy.opening(ticket, self.clock.now());
        if !was_locked && gate.policy.is_locked() {
            self.locked_gate(gate);
        }
        opening
            && gate
                .owner
                .as_ref()
                .is_some_and(|owner| owner.ticket == ticket)
    }
}

fn view_changed() -> CommandError {
    CommandError::new(
        "viewChanged",
        "The protected view expired. Start this disclosure or preview again.",
    )
}

fn cleanup_clipboard(gate: &mut Gate, due: Option<Duration>) {
    let lease = gate.clipboard.owned_generation();
    let result = match due {
        Some(now) => gate.clipboard.tick(now),
        None => gate.clipboard.clear(),
    };
    let failure = result
        .err()
        .zip(lease)
        .map(|(error, lease_generation)| CleanupFailure {
            lease_generation,
            error,
        });
    account_cleanup(gate, failure);
}

fn account_cleanup(gate: &mut Gate, failure: Option<CleanupFailure>) {
    if let Some(failure) = failure {
        if gate.cleanup_episode != Some(failure.lease_generation) {
            gate.cleanup_episode = Some(failure.lease_generation);
            application_warning(gate, failure.error.message);
        }
    }
    // An actual relinquishment or replacement ends the old episode. A successful
    // not-due tick retains the same owned lease, so it does not count as recovery.
    if gate.clipboard.owned_generation() != gate.cleanup_episode {
        gate.cleanup_episode = None;
    }
}

fn application_warning(gate: &mut Gate, message: String) {
    gate.warnings.push(ApplicationNotice {
        id: uuid::Uuid::new_v4().to_string(),
        message,
    });
}

fn operation_notice(gate: &mut Gate, ticket: Ticket, kind: OperationWarningKind) {
    gate.notices.push(OperationNotice {
        id: uuid::Uuid::new_v4().to_string(),
        origin_session: ticket.0.to_string(),
        kind,
    });
}

fn core_notices(gate: &mut Gate, ticket: Ticket, warnings: Vec<VaultWarning>) {
    for warning in warnings {
        let kind = match warning.kind {
            VaultWarningKind::Journal => OperationWarningKind::Journal,
            VaultWarningKind::BackupRefresh => OperationWarningKind::BackupRefresh,
            VaultWarningKind::BackupPrune => OperationWarningKind::BackupPrune,
            VaultWarningKind::BackupRetention => OperationWarningKind::BackupRetention,
        };
        operation_notice(gate, ticket, kind);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard::tests::{FakeClipboard, FakeData};
    use std::sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
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
        native_notices: Arc<Mutex<Vec<NativeLockNotice>>>,
        _directory: tempfile::TempDir,
    }
    fn fixture() -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let clock = Arc::new(TestClock::default());
        let clipboard = Arc::new(Mutex::new(FakeData::default()));
        let events = Arc::new(AtomicUsize::new(0));
        let notify = events.clone();
        let native_notices = Arc::new(Mutex::new(Vec::new()));
        let notices = native_notices.clone();
        let session = Arc::new(Session::new(
            clock.clone(),
            Box::new(FakeClipboard(clipboard.clone())),
            Arc::new(move |notice| {
                notify.fetch_add(1, Ordering::SeqCst);
                notices.lock().unwrap().push(notice);
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
            native_notices,
            _directory: directory,
        }
    }

    #[test]
    fn native_presentation_expiry_keeps_the_owner_but_expires_old_read_results_and_bound_writes() {
        let f = fixture();
        let before = f.session.snapshot().unwrap();
        let owner = f.session.active_owner(before.ticket).unwrap();
        let read = f
            .session
            .access(Access::Read, Some(before), |_| Ok("fake-old-read"))
            .unwrap();
        let notice = f.session.expire_presentation().unwrap().unwrap();
        let current = f.session.snapshot().unwrap();
        assert_eq!(current.ticket, before.ticket);
        assert_eq!(current.revision, before.revision);
        assert_ne!(current.presentation, before.presentation);
        assert_eq!(notice.session_id, owner);
        assert!(uuid::Uuid::parse_str(&notice.hide_id).is_ok());
        assert_eq!(
            serde_json::to_value(&notice)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            2
        );
        assert!(
            f.native_notices.lock().unwrap().is_empty(),
            "presentation expiry is not a native lock"
        );
        assert_eq!(
            f.session
                .access(Access::Write, Some(before), |_| Ok(()))
                .err()
                .unwrap()
                .kind,
            "stalePreview",
            "an operation bound to the old presentation is refused"
        );
        assert_eq!(
            f.session.finish(Access::Read, read).unwrap_err().kind,
            "viewChanged"
        );
        assert_eq!(f.session.active_owner(before.ticket).unwrap(), owner);
    }

    #[test]
    fn native_presentation_expiry_is_prompt_during_a_read_and_does_not_restamp_its_preview() {
        let f = fixture();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let reader = f.session.clone();
        let worker = std::thread::spawn(move || {
            reader.access(Access::Read, None, |_| {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (expiry_tx, expiry_rx) = mpsc::channel();
        let session = f.session.clone();
        let expiry =
            std::thread::spawn(move || expiry_tx.send(session.expire_presentation()).unwrap());
        let prompt = expiry_rx.recv_timeout(Duration::from_millis(500));
        release_tx.send(()).unwrap();
        let read = worker.join().unwrap().unwrap();
        expiry.join().unwrap();
        assert!(prompt
            .expect("expiry must not wait for the stored vault")
            .unwrap()
            .is_some());
        assert_ne!(
            read.snapshot.presentation,
            f.session.snapshot().unwrap().presentation
        );
        assert_eq!(
            f.session
                .access(Access::Write, Some(read.snapshot), |_| Ok(()))
                .err()
                .unwrap()
                .kind,
            "stalePreview"
        );
    }

    #[test]
    fn native_presentation_expiry_refuses_old_disclosure_and_copy_without_touching_a_user_value() {
        let f = fixture();
        let snapshot = f.session.snapshot().unwrap();
        f.clipboard.lock().unwrap().text = "fake-unrelated-user-value".into();
        f.session.expire_presentation().unwrap();
        let old = || Stamped {
            snapshot,
            value: Zeroizing::new("fake-old-protected-value".into()),
        };
        assert_eq!(f.session.reveal(old()).unwrap_err().kind, "viewChanged");
        assert_eq!(
            f.session.copy_stamped(old()).unwrap_err().kind,
            "viewChanged"
        );
        assert_eq!(
            f.clipboard.lock().unwrap().text,
            "fake-unrelated-user-value"
        );
        assert!(f
            .session
            .gate()
            .unwrap()
            .clipboard
            .owned_generation()
            .is_none());
        let fresh = f
            .session
            .access(Access::Read, None, |_| {
                Ok(Zeroizing::new("fake-new-presentation".into()))
            })
            .unwrap();
        assert_eq!(f.session.reveal(fresh).unwrap(), "fake-new-presentation");
    }

    #[test]
    fn native_presentation_expiry_during_a_committed_write_preserves_its_success_without_a_lock() {
        let f = fixture();
        let before = f.session.snapshot().unwrap();
        let (committed_tx, committed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let session = f.session.clone();
        let writer = std::thread::spawn(move || {
            session.access(Access::Write, Some(before), |vault| {
                let key = vault.add(akm_core::ApiKeyInput::new(
                    "Fake presentation commit",
                    "fake-committed-value",
                ))?;
                let id = key.id.clone();
                committed_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(id)
            })
        });
        committed_rx.recv_timeout(Duration::from_secs(20)).unwrap();
        let (expiry_tx, expiry_rx) = mpsc::channel();
        let session = f.session.clone();
        let expiry =
            std::thread::spawn(move || expiry_tx.send(session.expire_presentation()).unwrap());
        let prompt = expiry_rx.recv_timeout(Duration::from_millis(500));
        release_tx.send(()).unwrap();
        let written = writer.join().unwrap().unwrap();
        expiry.join().unwrap();
        assert!(prompt
            .expect("expiry must not wait for post-commit work")
            .unwrap()
            .is_some());
        let id = f.session.finish(Access::Write, written).unwrap();
        assert_eq!(f.session.snapshot().unwrap().ticket, before.ticket);
        assert!(f.native_notices.lock().unwrap().is_empty());
        assert!(Vault::open(
            f._directory.path().join("vault.kdbx"),
            "fake-test-password-123!"
        )
        .unwrap()
        .get_by_id(&id)
        .is_ok());
    }

    #[test]
    fn native_presentation_expiry_preserves_approved_publication_and_its_truthful_completion() {
        for authorized in [false, true] {
            let f = fixture();
            let snapshot = f.session.snapshot().unwrap();
            let target = f._directory.path().join("presentation.env");
            let (stalled_tx, stalled_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let session = f.session.clone();
            let path = target.clone();
            let worker = std::thread::spawn(move || {
                let release_rx = std::sync::Mutex::new(release_rx);
                session.publish(
                    snapshot,
                    || {
                        let prepared = crate::private_export::prepare_private(
                            &path,
                            "TOKEN=fake-view-export\n",
                            &[],
                            &[],
                        )?;
                        if !authorized {
                            stalled_tx.send(()).unwrap();
                            release_rx.lock().unwrap().recv().unwrap();
                        }
                        Ok(prepared)
                    },
                    |prepared| {
                        if authorized {
                            stalled_tx.send(()).unwrap();
                            release_rx.lock().unwrap().recv().unwrap();
                        }
                        prepared.publish()
                    },
                )
            });
            stalled_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            f.session.expire_presentation().unwrap();
            release_tx.send(()).unwrap();
            let result = worker.join().unwrap();
            assert_eq!(result.unwrap(), target);
            assert_eq!(
                std::fs::read_to_string(&target).unwrap(),
                "TOKEN=fake-view-export\n"
            );
            assert_eq!(f.session.snapshot().unwrap().ticket, snapshot.ticket);
            assert!(!std::fs::read_dir(f._directory.path())
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "tmp")));
        }
    }

    #[test]
    fn native_presentation_does_not_bypass_lock_or_revision_publication_checks() {
        for lock in [false, true] {
            let f = fixture();
            let snapshot = f.session.snapshot().unwrap();
            let target = f._directory.path().join("refused-presentation.env");
            let (prepared_tx, prepared_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let session = f.session.clone();
            let path = target.clone();
            let worker = std::thread::spawn(move || {
                session.publish(
                    snapshot,
                    || {
                        let prepared = crate::private_export::prepare_private(
                            &path,
                            "TOKEN=fake-refused\n",
                            &[],
                            &[],
                        )?;
                        prepared_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        Ok(prepared)
                    },
                    |prepared| prepared.publish(),
                )
            });
            prepared_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            f.session.expire_presentation().unwrap();
            if lock {
                f.session.lock().unwrap();
            } else {
                f.session
                    .access(Access::Write, None, |vault| {
                        vault.add(akm_core::ApiKeyInput::new(
                            "Fake intervening revision",
                            "fake-revision-value",
                        ))?;
                        Ok(())
                    })
                    .unwrap();
            }
            release_tx.send(()).unwrap();
            assert_eq!(
                worker.join().unwrap().unwrap_err().kind,
                if lock { "locked" } else { "stalePreview" }
            );
            assert!(!target.exists());
            assert!(!std::fs::read_dir(f._directory.path())
                .unwrap()
                .any(|entry| entry
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|ext| ext == "tmp")));
        }
    }

    #[test]
    fn native_presentation_delivery_failure_cannot_lock_a_replacement_owner() {
        let f = fixture();
        let a = f
            .session
            .active_owner(f.session.snapshot().unwrap().ticket)
            .unwrap();
        let mut b = None;
        let error = crate::visibility::expire_and_notify(&f.session, |notice| {
            assert_eq!(notice.session_id, a);
            let owner = f.session.prepare_open().unwrap();
            let ticket = f.session.claim_open(&owner).unwrap();
            f.session
                .open(ticket, || {
                    Vault::open(
                        f._directory.path().join("vault.kdbx"),
                        "fake-test-password-123!",
                    )
                    .map_err(Into::into)
                })
                .unwrap();
            b = Some(owner);
            Err(())
        })
        .unwrap_err();
        assert_eq!(error.kind, "internal");
        assert_eq!(
            f.session
                .active_owner(f.session.snapshot().unwrap().ticket)
                .unwrap(),
            b.unwrap()
        );
        assert_eq!(
            f.native_notices.lock().unwrap().len(),
            1,
            "only A's normal opening replacement event, no B lock"
        );
        let report = serde_json::to_string(&f.session.warnings().unwrap()).unwrap();
        assert!(report.contains("could not be delivered"));
        assert!(!report.contains("vault.kdbx"));
    }

    #[test]
    fn native_presentation_failed_delivery_locks_the_same_owner_without_erasing_an_unrelated_copy()
    {
        let f = fixture();
        let owner = f
            .session
            .active_owner(f.session.snapshot().unwrap().ticket)
            .unwrap();
        f.clipboard.lock().unwrap().text = "fake-user-owned-copy".into();
        assert!(crate::visibility::expire_and_notify(&f.session, |_| Err(())).is_err());
        assert!(f.session.snapshot().is_err());
        assert_eq!(f.native_notices.lock().unwrap().len(), 1);
        assert_eq!(f.native_notices.lock().unwrap()[0].session_id, owner);
        assert_eq!(f.clipboard.lock().unwrap().text, "fake-user-owned-copy");
    }

    #[test]
    fn native_owner_reservation_and_owned_lock_never_target_a_replacement() {
        let f = fixture();
        let a = f
            .session
            .active_owner(f.session.snapshot().unwrap().ticket)
            .unwrap();
        let b = f.session.prepare_open().unwrap();
        assert_ne!(a, b);
        assert_eq!(f.native_notices.lock().unwrap().len(), 1);
        assert_eq!(f.native_notices.lock().unwrap()[0].session_id, a);
        assert!(
            f.session.snapshot().is_err(),
            "a reservation is not an unlocked session"
        );
        let ticket = f.session.claim_open(&b).unwrap();
        assert!(
            f.session.claim_open(&b).is_err(),
            "a reservation is single-use"
        );
        f.session
            .open(ticket, || {
                Vault::open(
                    f._directory.path().join("vault.kdbx"),
                    "fake-test-password-123!",
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(f.session.active_owner(ticket).unwrap(), b);
        assert!(f.session.lock_owned(Some(&a)).is_err());
        assert!(f.session.lock_owned(None).is_err());
        assert_eq!(f.session.snapshot().unwrap().ticket, ticket);
        f.session.lock_owned(Some(&b)).unwrap();
        assert!(f.session.snapshot().is_err());
        assert_eq!(f.native_notices.lock().unwrap()[1].session_id, b);
        assert!(f.session.lock_owned(Some(&b)).is_err());
        f.session.lock_owned(None).unwrap();
        assert_eq!(
            f.native_notices.lock().unwrap().len(),
            2,
            "duplicate cleanup/lock does not emit a new invalidated owner"
        );
        let payload = serde_json::to_value(&f.native_notices.lock().unwrap()[1]).unwrap();
        assert_eq!(payload.as_object().unwrap().len(), 1);
        assert!(uuid::Uuid::parse_str(payload["sessionId"].as_str().unwrap()).is_ok());
        assert!(!payload.to_string().contains("vault.kdbx"));
    }

    #[test]
    fn native_owner_replaced_reservations_cannot_be_claimed_or_opened() {
        let f = fixture();
        let b = f.session.prepare_open().unwrap();
        let c = f.session.prepare_open().unwrap();
        assert_ne!(b, c);
        assert_eq!(f.native_notices.lock().unwrap()[1].session_id, b);
        assert!(f.session.claim_open(&b).is_err());
        assert!(f.session.claim_open("foreign-or-malformed-owner").is_err());
        assert!(f.session.lock_owned(Some(&b)).is_err());
        let ticket = f.session.claim_open(&c).unwrap();
        f.session
            .open(ticket, || {
                Vault::open(
                    f._directory.path().join("vault.kdbx"),
                    "fake-test-password-123!",
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(f.session.active_owner(ticket).unwrap(), c);
    }

    #[test]
    fn native_owner_current_signals_and_expiry_invalidate_the_opening_reservation() {
        for signal in [
            Some(Signal::Locked),
            Some(Signal::Suspend),
            Some(Signal::Unobservable),
            None,
        ] {
            let f = fixture();
            let b = f.session.prepare_open().unwrap();
            if let Some(signal) = signal {
                f.session.observe(signal);
            } else {
                f.clock.set(600);
                f.session.tick();
            }
            assert!(f.session.claim_open(&b).is_err());
            assert!(f.session.snapshot().is_err());
            assert_eq!(
                f.native_notices.lock().unwrap().last().unwrap().session_id,
                b
            );
            assert_eq!(f.native_notices.lock().unwrap().len(), 2);
        }
    }

    #[test]
    fn native_owner_failed_opening_retires_without_hiding_its_terminal_error() {
        let f = fixture();
        let b = f.session.prepare_open().unwrap();
        let ticket = f.session.claim_open(&b).unwrap();
        let before = f.native_notices.lock().unwrap().len();
        let error = f
            .session
            .open(ticket, || {
                Err(CommandError::new(
                    "open",
                    "Fake current authentication error",
                ))
            })
            .unwrap_err();
        assert_eq!(error.message, "Fake current authentication error");
        assert!(f.session.claim_open(&b).is_err());
        assert!(f.session.snapshot().is_err());
        assert_eq!(f.native_notices.lock().unwrap().len(), before);
        assert_ne!(f.session.prepare_open().unwrap(), b);
    }

    #[test]
    fn native_owner_active_signals_expiry_and_malformed_locks_keep_exact_ownership() {
        for signal in [
            Some(Signal::Locked),
            Some(Signal::Suspend),
            Some(Signal::Unobservable),
            None,
        ] {
            let f = fixture();
            let b = f.session.prepare_open().unwrap();
            let ticket = f.session.claim_open(&b).unwrap();
            f.session
                .open(ticket, || {
                    Vault::open(
                        f._directory.path().join("vault.kdbx"),
                        "fake-test-password-123!",
                    )
                    .map_err(Into::into)
                })
                .unwrap();
            for expected in [None, Some("not-a-native-uuid"), Some("9007199254740993")] {
                assert!(f.session.lock_owned(expected).is_err());
                assert_eq!(f.session.active_owner(ticket).unwrap(), b);
            }
            if let Some(signal) = signal {
                f.session.observe(signal);
            } else {
                f.clock.set(600);
                f.session.tick();
            }
            assert!(f.session.snapshot().is_err());
            assert!(f.session.claim_open(&b).is_err());
            assert_eq!(
                f.native_notices.lock().unwrap().last().unwrap().session_id,
                b
            );
            let count = f.native_notices.lock().unwrap().len();
            f.session.lock().unwrap(); // Same unconditional path used by native exit.
            assert_eq!(f.native_notices.lock().unwrap().len(), count);
        }
    }

    #[test]
    fn native_owner_poisoned_gate_invalidates_its_owner_once_without_a_replacement() {
        let f = fixture();
        let a = f
            .session
            .active_owner(f.session.snapshot().unwrap().ticket)
            .unwrap();
        let session = f.session.clone();
        assert!(std::thread::spawn(move || {
            let _gate = session.gate().unwrap();
            panic!("fake poisoned security gate");
        })
        .join()
        .is_err());
        assert!(f.session.snapshot().is_err());
        assert!(f.session.prepare_open().is_err());
        let notices = f.native_notices.lock().unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].session_id, a);
    }

    #[test]
    fn native_owner_lock_during_kdf_rejects_the_current_result_without_a_new_owner() {
        let f = fixture();
        let b = f.session.prepare_open().unwrap();
        let ticket = f.session.claim_open(&b).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let session = f.session.clone();
        let path = f._directory.path().join("vault.kdbx");
        let worker = std::thread::spawn(move || {
            session.open(ticket, || {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Vault::open(path, "fake-test-password-123!").map_err(Into::into)
            })
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        f.session.lock_owned(Some(&b)).unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(worker.join().unwrap().unwrap_err().kind, "locked");
        assert!(f.session.snapshot().is_err());
        assert_eq!(
            f.native_notices.lock().unwrap().last().unwrap().session_id,
            b
        );
        assert!(f.session.claim_open(&b).is_err());
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
    fn warning_from_a_committed_change_is_owned_after_another_vault_opens() {
        let f = fixture();
        let origin = f.session.snapshot().unwrap().ticket;
        std::fs::create_dir(f._directory.path().join("vault.kdbx.audit.jsonl")).unwrap();
        f.session
            .access(Access::Write, None, |vault| {
                vault.add(akm_core::ApiKeyInput::new(
                    "Private A title",
                    "fake-a-secret",
                ))?;
                Ok(())
            })
            .unwrap();
        f.session.lock().unwrap();
        let newer = f.session.begin_open().unwrap();
        f.session
            .open(newer, || {
                Vault::create(
                    f._directory.path().join("b.kdbx"),
                    "fake-test-password-123!",
                    None,
                )
                .map_err(Into::into)
            })
            .unwrap();
        let report = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_eq!(report["operations"][0]["kind"], "journal");
        assert_eq!(
            report["operations"][0]["originSession"],
            origin.0.to_string()
        );
        assert_ne!(
            report["operations"][0]["originSession"],
            newer.0.to_string()
        );
        assert!(!report.to_string().contains("Private A title"));
        assert!(!report.to_string().contains("vault.kdbx"));
        assert_eq!(f.session.snapshot().unwrap().ticket, newer);
    }

    #[test]
    fn warning_from_a_lock_overtaken_commit_keeps_a_secret_free_origin_notice() {
        let f = fixture();
        let origin = f.session.snapshot().unwrap().ticket;
        std::fs::create_dir(f._directory.path().join("vault.kdbx.audit.jsonl")).unwrap();
        let (committed_tx, committed_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let session = f.session.clone();
        let writer = std::thread::spawn(move || {
            session.access(Access::Write, None, |vault| {
                vault.add(akm_core::ApiKeyInput::new(
                    "Private A title",
                    "fake-a-secret",
                ))?;
                committed_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(())
            })
        });
        committed_rx.recv_timeout(Duration::from_secs(20)).unwrap();
        f.session.lock().unwrap();
        release_tx.send(()).unwrap();
        assert_eq!(
            writer.join().unwrap().err().unwrap().kind,
            "completedWhileLocked"
        );
        let newer = f.session.begin_open().unwrap();
        f.session
            .open(newer, || {
                Vault::create(
                    f._directory.path().join("b.kdbx"),
                    "fake-test-password-123!",
                    None,
                )
                .map_err(Into::into)
            })
            .unwrap();
        let report = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_eq!(report["operations"][0]["kind"], "journal");
        assert_eq!(
            report["operations"][0]["originSession"],
            origin.0.to_string()
        );
        assert!(!report.to_string().contains("fake-a-secret"));
        assert!(!report.to_string().contains("vault.kdbx"));
        assert_eq!(f.session.snapshot().unwrap().ticket, newer);
    }

    #[test]
    fn application_warnings_remain_available_while_locked_without_session_attribution() {
        let f = fixture();
        f.session
            .warn("Windows notification availability warning".into());
        f.session.lock().unwrap();
        let report = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_eq!(
            report["application"][0]["message"],
            "Windows notification availability warning"
        );
        assert_eq!(report["operations"], serde_json::json!([]));
        assert_eq!(
            serde_json::to_value(f.session.warnings().unwrap()).unwrap()["application"],
            report["application"]
        );
        let id = report["application"][0]["id"].as_str().unwrap().to_string();
        f.session
            .dismiss_warnings(std::slice::from_ref(&id))
            .unwrap();
        f.session
            .warn("Windows notification availability warning".into());
        let recurrence = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_ne!(recurrence["application"][0]["id"], id);
        f.session.dismiss_warnings(&[id]).unwrap();
        assert_eq!(
            serde_json::to_value(f.session.warnings().unwrap()).unwrap()["application"],
            recurrence["application"]
        );
    }

    struct BusyCleanup {
        data: Arc<Mutex<FakeData>>,
        busy: Arc<AtomicBool>,
        partial: bool,
    }
    impl ClipboardBackend for BusyCleanup {
        fn set_private(&mut self, text: &str) -> crate::clipboard::Publication {
            let sequence = FakeClipboard(self.data.clone())
                .set_private(text)
                .published_for_test();
            if self.partial {
                // A partial write whose rollback failed: the value may remain under this sequence.
                return crate::clipboard::Publication::for_test(
                    crate::clipboard::Outcome::Residual(
                        CommandError::new("clipboard", "Fake partial publication error"),
                        crate::clipboard::LeaseEvidence::for_test(sequence),
                    ),
                );
            }
            crate::clipboard::Publication::for_test(crate::clipboard::Outcome::Published(
                crate::clipboard::LeaseEvidence::for_test(sequence),
            ))
        }
        fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
            if self.busy.load(Ordering::SeqCst) {
                // The production OpenClipboard failure text, which is written for copying.
                return Err(CommandError::new(
                    "clipboard",
                    "The clipboard is busy. Try copying again.",
                ));
            }
            FakeClipboard(self.data.clone()).clear_matching(sequence, text)
        }
    }

    // 1A-R6-02: a busy cleanup warns that the copy may remain, retries, and keeps a newer copy.
    #[test]
    fn a_busy_cleanup_warns_that_the_copy_may_remain_and_retries_in_one_episode() {
        for partial in [false, true] {
            let f = fixture();
            let busy = Arc::new(AtomicBool::new(true));
            f.session.gate().unwrap().clipboard = ClipboardLease::new(Box::new(BusyCleanup {
                data: f.clipboard.clone(),
                busy: busy.clone(),
                partial,
            }));
            let ticket = f.session.snapshot().unwrap().ticket;
            let copied = f
                .session
                .copy(ticket, Zeroizing::new("fake-busy-value".into()));
            assert_eq!(copied.is_err(), partial);
            if !partial {
                f.clock.set(30);
                f.session.tick();
            }
            let warnings = f.session.warnings().unwrap().application;
            assert_eq!(warnings.len(), 1, "partial={partial}");
            assert_eq!(
                warnings[0].message,
                crate::clipboard::uncertain_cleanup().message,
                "partial={partial}"
            );
            assert!(!warnings[0].message.contains("Try copying again"));
            assert_eq!(f.clipboard.lock().unwrap().text, "fake-busy-value");
            f.session.tick();
            assert_eq!(application_ids(&f), vec![warnings[0].id.clone()]);
            // The episode ends by a newer identical user copy, or by AKM's own clear.
            let newer_copy = partial;
            if newer_copy {
                f.clipboard.lock().unwrap().sequence += 1;
            }
            busy.store(false, Ordering::SeqCst);
            f.session.tick();
            let expected = if newer_copy { "fake-busy-value" } else { "" };
            assert_eq!(f.clipboard.lock().unwrap().text, expected);
            assert_eq!(f.clipboard.lock().unwrap().clears, usize::from(!newer_copy));
            assert_eq!(f.session.gate().unwrap().clipboard.owned_generation(), None);
            // The kept warning is history: it never promised a retry that has now ended.
            let kept = f.session.warnings().unwrap().application;
            assert_eq!(kept.len(), 1);
            assert_eq!(kept[0].id, warnings[0].id);
            assert!(!kept[0].message.contains("try again") && !kept[0].message.contains("will"));
        }
    }

    struct RollbackAfterLease {
        data: Arc<Mutex<FakeData>>,
        roll_back: Arc<AtomicBool>,
        clear_calls: Arc<AtomicUsize>,
    }
    impl ClipboardBackend for RollbackAfterLease {
        fn set_private(&mut self, text: &str) -> crate::clipboard::Publication {
            if self.roll_back.load(Ordering::SeqCst) {
                // The publication emptied the clipboard, failed, and rolled back.
                let mut data = self.data.lock().unwrap();
                data.sequence += 2;
                data.text.clear();
                return crate::clipboard::Publication::for_test(
                    crate::clipboard::Outcome::Emptied(CommandError::new(
                        "clipboard",
                        "Fake rolled-back publication",
                    )),
                );
            }
            FakeClipboard(self.data.clone()).set_private(text)
        }
        fn clear_matching(&mut self, _sequence: u32, _text: &str) -> Result<bool, CommandError> {
            self.clear_calls.fetch_add(1, Ordering::SeqCst);
            Err(CommandError::new(
                "clipboard",
                "The clipboard is busy. Try copying again.",
            ))
        }
    }

    // 1A-R7-02: a rolled-back publication also ends the earlier lease; nothing is left to retry.
    #[test]
    fn a_rolled_back_publication_after_an_owned_copy_leaves_no_lease_retry_or_warning() {
        let f = fixture();
        let roll_back = Arc::new(AtomicBool::new(false));
        let clear_calls = Arc::new(AtomicUsize::new(0));
        f.session.gate().unwrap().clipboard = ClipboardLease::new(Box::new(RollbackAfterLease {
            data: f.clipboard.clone(),
            roll_back: roll_back.clone(),
            clear_calls: clear_calls.clone(),
        }));
        let ticket = f.session.snapshot().unwrap().ticket;
        f.session
            .copy(ticket, Zeroizing::new("fake-first-copy".into()))
            .unwrap();
        roll_back.store(true, Ordering::SeqCst);
        let error = f
            .session
            .copy(ticket, Zeroizing::new("fake-second-copy".into()))
            .unwrap_err();
        assert_eq!(
            error.message,
            "Fake rolled-back publication The earlier clipboard content was removed, and the new value was not copied. Copy again."
        );
        assert_eq!(f.session.gate().unwrap().clipboard.owned_generation(), None);
        f.clock.set(30);
        f.session.tick();
        f.session.lock().unwrap();
        assert_eq!(clear_calls.load(Ordering::SeqCst), 0);
        assert!(application_ids(&f).is_empty());
    }

    struct CleanupFault {
        data: Arc<Mutex<FakeData>>,
        fail: Arc<AtomicBool>,
        attempts: Arc<AtomicUsize>,
    }
    impl ClipboardBackend for CleanupFault {
        fn set_private(&mut self, text: &str) -> crate::clipboard::Publication {
            FakeClipboard(self.data.clone()).set_private(text)
        }
        fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(CommandError::new(
                    "clipboard",
                    "Could not clean up AKM's clipboard copy.",
                ));
            }
            FakeClipboard(self.data.clone()).clear_matching(sequence, text)
        }
    }

    fn cleanup_fault(f: &Fixture) -> (Arc<AtomicBool>, Arc<AtomicUsize>) {
        let fail = Arc::new(AtomicBool::new(true));
        let attempts = Arc::new(AtomicUsize::new(0));
        f.session.gate().unwrap().clipboard = ClipboardLease::new(Box::new(CleanupFault {
            data: f.clipboard.clone(),
            fail: fail.clone(),
            attempts: attempts.clone(),
        }));
        let ticket = f.session.snapshot().unwrap().ticket;
        f.session
            .copy(ticket, Zeroizing::new("fake-cleanup-value".into()))
            .unwrap();
        (fail, attempts)
    }

    fn application_ids(f: &Fixture) -> Vec<String> {
        f.session
            .warnings()
            .unwrap()
            .application
            .into_iter()
            .map(|n| n.id)
            .collect()
    }

    fn reopen_fixture(f: &Fixture) {
        let ticket = f.session.begin_open().unwrap();
        f.session
            .open(ticket, || {
                Vault::open(
                    f._directory.path().join("vault.kdbx"),
                    "fake-test-password-123!",
                )
                .map_err(Into::into)
            })
            .unwrap();
    }

    #[test]
    fn cleanup_episode_uninterrupted_retries_keep_one_id_even_after_acknowledgment() {
        let f = fixture();
        let (_fail, _) = cleanup_fault(&f);
        f.session.lock().unwrap();
        let first = application_ids(&f);
        assert_eq!(first.len(), 1);
        f.session.tick();
        f.session.observe(Signal::Locked);
        assert_eq!(application_ids(&f), first);
        f.session.dismiss_warnings(&first).unwrap();
        f.session.tick();
        f.session.lock().unwrap();
        assert!(
            application_ids(&f).is_empty(),
            "acknowledging one unresolved episode must not make each retry a new warning"
        );
    }

    #[test]
    fn cleanup_episode_recovery_then_recurrence_before_old_ack_keeps_a_new_id() {
        let f = fixture();
        let (fail, _) = cleanup_fault(&f);
        f.clock.set(30);
        f.session.tick();
        let first = application_ids(&f);
        assert_eq!(first.len(), 1);
        fail.store(false, Ordering::SeqCst);
        f.session.tick();
        assert_eq!(f.clipboard.lock().unwrap().text, "");
        assert_eq!(application_ids(&f), first);
        let ticket = f.session.snapshot().unwrap().ticket;
        f.session
            .copy(ticket, Zeroizing::new("fake-next-copy".into()))
            .unwrap();
        fail.store(true, Ordering::SeqCst);
        f.clock.set(60);
        f.session.tick();
        let both = application_ids(&f);
        assert_eq!(
            both.len(),
            2,
            "a recovered episode cannot absorb a later failure"
        );
        assert_ne!(both[0], both[1]);
        f.session.lock().unwrap();
        reopen_fixture(&f);
        assert_eq!(application_ids(&f), both);
        f.session.dismiss_warnings(&first).unwrap();
        assert_eq!(application_ids(&f), vec![both[1].clone()]);
        let report = serde_json::to_string(&f.session.warnings().unwrap()).unwrap();
        assert!(!report.contains("fake-next-copy"));
        assert!(!report.contains("vault.kdbx"));
    }

    #[test]
    fn cleanup_episode_a_new_owned_lease_does_not_reuse_an_old_pending_warning() {
        let f = fixture();
        let (_fail, _) = cleanup_fault(&f);
        f.clock.set(30);
        f.session.tick();
        let first = application_ids(&f);
        let ticket = f.session.snapshot().unwrap().ticket;
        f.session
            .copy(ticket, Zeroizing::new("fake-new-lease".into()))
            .unwrap();
        f.clock.set(60);
        f.session.tick();
        let both = application_ids(&f);
        assert_eq!(both.len(), 2);
        assert_eq!(both[0], first[0]);
        assert_ne!(both[0], both[1]);
        f.session.dismiss_warnings(&first).unwrap();
        f.session.tick();
        assert_eq!(application_ids(&f), vec![both[1].clone()]);
    }

    #[test]
    fn cleanup_episode_a_successful_not_due_tick_is_not_recovery() {
        let f = fixture();
        let (_fail, attempts) = cleanup_fault(&f);
        f.session.lock().unwrap();
        reopen_fixture(&f);
        let first = application_ids(&f);
        assert_eq!(first.len(), 1);
        f.session.dismiss_warnings(&first).unwrap();
        let before = attempts.load(Ordering::SeqCst);
        f.clock.set(1);
        f.session.tick();
        assert_eq!(
            attempts.load(Ordering::SeqCst),
            before,
            "not-due lease must not attempt cleanup"
        );
        f.clock.set(30);
        f.session.tick();
        assert!(
            application_ids(&f).is_empty(),
            "not-due success must not reset the acknowledged unresolved episode"
        );
    }

    #[test]
    fn cleanup_episode_relinquishing_a_newer_user_copy_is_recovery_without_erasing_it() {
        let f = fixture();
        let (fail, _) = cleanup_fault(&f);
        f.clock.set(30);
        f.session.tick();
        let first = application_ids(&f);
        {
            let mut data = f.clipboard.lock().unwrap();
            data.sequence += 1;
            data.text = "fake-user-copy".into();
        }
        fail.store(false, Ordering::SeqCst);
        f.session.tick();
        assert_eq!(f.clipboard.lock().unwrap().text, "fake-user-copy");
        assert_eq!(f.clipboard.lock().unwrap().clears, 0);
        let ticket = f.session.snapshot().unwrap().ticket;
        f.session
            .copy(ticket, Zeroizing::new("fake-owned-again".into()))
            .unwrap();
        fail.store(true, Ordering::SeqCst);
        f.clock.set(60);
        f.session.tick();
        assert_eq!(application_ids(&f).len(), 2);
        f.session.dismiss_warnings(&first).unwrap();
        assert_eq!(application_ids(&f).len(), 1);
    }

    #[test]
    fn cleanup_episode_explicit_application_warnings_are_separate_occurrences() {
        let f = fixture();
        f.session.warn("Explicit safety occurrence".into());
        let first = application_ids(&f);
        f.session.warn("Explicit safety occurrence".into());
        assert_eq!(application_ids(&f).len(), 2);
        f.session.dismiss_warnings(&first).unwrap();
        assert_eq!(application_ids(&f).len(), 1);
    }

    struct PartialCleanupFault {
        data: Arc<Mutex<FakeData>>,
        partial: Arc<AtomicBool>,
        fail_clear: Arc<AtomicBool>,
    }
    impl ClipboardBackend for PartialCleanupFault {
        fn set_private(&mut self, text: &str) -> crate::clipboard::Publication {
            let sequence = FakeClipboard(self.data.clone())
                .set_private(text)
                .published_for_test();
            if self.partial.load(Ordering::SeqCst) {
                // A partial write whose rollback failed: the value may remain under this sequence.
                return crate::clipboard::Publication::for_test(
                    crate::clipboard::Outcome::Residual(
                        CommandError::new("clipboard", "Fake partial publication error"),
                        crate::clipboard::LeaseEvidence::for_test(sequence),
                    ),
                );
            }
            crate::clipboard::Publication::for_test(crate::clipboard::Outcome::Published(
                crate::clipboard::LeaseEvidence::for_test(sequence),
            ))
        }
        fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
            if self.fail_clear.load(Ordering::SeqCst) {
                return Err(CommandError::new(
                    "clipboard",
                    "Could not clean up AKM's clipboard copy.",
                ));
            }
            FakeClipboard(self.data.clone()).clear_matching(sequence, text)
        }
    }

    fn partial_cleanup_fault(f: &Fixture) -> (Arc<AtomicBool>, Arc<AtomicBool>) {
        let partial = Arc::new(AtomicBool::new(true));
        let fail = Arc::new(AtomicBool::new(true));
        f.session.gate().unwrap().clipboard = ClipboardLease::new(Box::new(PartialCleanupFault {
            data: f.clipboard.clone(),
            partial: partial.clone(),
            fail_clear: fail.clone(),
        }));
        (partial, fail)
    }

    #[test]
    fn partial_cleanup_failure_is_retained_before_the_first_recovery_tick() {
        let f = fixture();
        let (_partial, fail) = partial_cleanup_fault(&f);
        let ticket = f.session.snapshot().unwrap().ticket;
        let error = f
            .session
            .copy(ticket, Zeroizing::new("fake-partial-value".into()))
            .unwrap_err();
        assert_eq!(
            error.message,
            "Fake partial publication error AKM could not confirm that the partial copy was removed. Check the clipboard before you paste."
        );
        assert_eq!(f.clipboard.lock().unwrap().text, "fake-partial-value");
        let first = application_ids(&f);
        assert_eq!(
            first.len(),
            1,
            "failed immediate cleanup must allocate a retained episode before recovery"
        );
        fail.store(false, Ordering::SeqCst);
        f.session.tick();
        assert_eq!(f.clipboard.lock().unwrap().text, "");
        assert_eq!(application_ids(&f), first);
        assert!(!serde_json::to_string(&f.session.warnings().unwrap())
            .unwrap()
            .contains("fake-partial-value"));
    }

    #[test]
    fn partial_cleanup_recurrence_before_an_old_ack_gets_a_new_id() {
        let f = fixture();
        let (partial, fail) = partial_cleanup_fault(&f);
        partial.store(false, Ordering::SeqCst);
        let ticket = f.session.snapshot().unwrap().ticket;
        f.session
            .copy(ticket, Zeroizing::new("fake-first".into()))
            .unwrap();
        f.clock.set(30);
        f.session.tick();
        let first = application_ids(&f);
        assert_eq!(first.len(), 1);
        fail.store(false, Ordering::SeqCst);
        f.session.tick();
        partial.store(true, Ordering::SeqCst);
        fail.store(true, Ordering::SeqCst);
        assert!(f
            .session
            .copy(ticket, Zeroizing::new("fake-partial-second".into()))
            .is_err());
        let both = application_ids(&f);
        assert_eq!(
            both.len(),
            2,
            "new partial-publication cleanup failure must not disappear into an older warning"
        );
        assert_ne!(both[0], both[1]);
        fail.store(false, Ordering::SeqCst);
        f.session.tick();
        f.session.dismiss_warnings(&first).unwrap();
        assert_eq!(application_ids(&f), vec![both[1].clone()]);
    }

    #[test]
    fn partial_cleanup_retries_do_not_recreate_a_dismissed_unresolved_episode() {
        let f = fixture();
        let (_partial, fail) = partial_cleanup_fault(&f);
        let ticket = f.session.snapshot().unwrap().ticket;
        assert!(f
            .session
            .copy(ticket, Zeroizing::new("fake-partial".into()))
            .is_err());
        let first = application_ids(&f);
        assert_eq!(first.len(), 1);
        f.session.dismiss_warnings(&first).unwrap();
        f.session.tick();
        f.session.lock().unwrap();
        assert!(application_ids(&f).is_empty());
        fail.store(false, Ordering::SeqCst);
        f.session.tick();
        assert_eq!(f.clipboard.lock().unwrap().text, "");
    }

    #[test]
    fn partial_cleanup_recovery_relinquishes_a_newer_identical_user_copy() {
        let f = fixture();
        let (_partial, fail) = partial_cleanup_fault(&f);
        let ticket = f.session.snapshot().unwrap().ticket;
        assert!(f
            .session
            .copy(ticket, Zeroizing::new("fake-identical".into()))
            .is_err());
        let first = application_ids(&f);
        assert_eq!(first.len(), 1);
        f.clipboard.lock().unwrap().sequence += 1;
        fail.store(false, Ordering::SeqCst);
        f.session.tick();
        assert_eq!(f.clipboard.lock().unwrap().text, "fake-identical");
        assert_eq!(f.clipboard.lock().unwrap().clears, 0);
        assert_eq!(application_ids(&f), first);
    }

    #[test]
    fn backup_and_journal_warnings_keep_distinct_origins_across_lock_and_open() {
        for (backup_fails, journal_fails, prune_fails) in [
            (true, false, false),
            (false, true, false),
            (true, true, false),
            (false, false, true),
            (true, true, true),
        ] {
            for overtaken in [false, true] {
                let f = fixture();
                let origin = f.session.snapshot().unwrap().ticket;
                if backup_fails {
                    std::fs::create_dir(f._directory.path().join("vault.kdbx.bak")).unwrap();
                }
                if prune_fails {
                    for _ in 0..6 {
                        std::fs::create_dir(f._directory.path().join(format!(
                            "vault.kdbx.bak.20000101T000000000000000Z_{}.kdbx",
                            uuid::Uuid::new_v4()
                        )))
                        .unwrap();
                    }
                }
                let journal =
                    akm_core::audit::AuditLog::for_vault(&f._directory.path().join("vault.kdbx"));
                if journal_fails {
                    std::fs::create_dir(journal.path()).unwrap();
                }
                let (saved_tx, saved_rx) = mpsc::channel();
                let (release_tx, release_rx) = mpsc::channel();
                let session = f.session.clone();
                let writer = std::thread::spawn(move || {
                    session.access(Access::Write, None, |vault| {
                        vault.add(akm_core::ApiKeyInput::new(
                            "Private A title",
                            "fake-a-secret",
                        ))?;
                        saved_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        Ok(())
                    })
                });
                saved_rx.recv_timeout(Duration::from_secs(20)).unwrap();
                if overtaken {
                    f.session.lock().unwrap();
                }
                release_tx.send(()).unwrap();
                let result = writer.join().unwrap();
                if overtaken {
                    assert_eq!(result.err().unwrap().kind, "completedWhileLocked");
                } else {
                    assert!(result.is_ok());
                    f.session.lock().unwrap();
                }
                let newer = f.session.begin_open().unwrap();
                f.session
                    .open(newer, || {
                        Vault::create(
                            f._directory.path().join("b.kdbx"),
                            "fake-test-password-123!",
                            None,
                        )
                        .map_err(Into::into)
                    })
                    .unwrap();
                let report = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
                let notices = report["operations"].as_array().unwrap();
                let mut kinds: Vec<_> = notices
                    .iter()
                    .map(|n| n["kind"].as_str().unwrap())
                    .collect();
                kinds.sort_unstable();
                let expected: Vec<_> = [
                    (prune_fails, "backupPrune"),
                    (backup_fails, "backupRefresh"),
                    (journal_fails, "journal"),
                ]
                .into_iter()
                .filter_map(|(yes, kind)| yes.then_some(kind))
                .collect();
                assert_eq!(
                    kinds, expected,
                    "backup={backup_fails}, journal={journal_fails}, overtaken={overtaken}"
                );
                let origin_string = origin.0.to_string();
                assert!(notices
                    .iter()
                    .all(|n| n["originSession"].as_str() == Some(origin_string.as_str())));
                for private in ["Private A title", "fake-a-secret", "vault.kdbx"] {
                    assert!(!report.to_string().contains(private));
                }
                assert_eq!(f.session.snapshot().unwrap().ticket, newer);
                if !journal_fails {
                    assert_eq!(journal.read(1).unwrap()[0].action, "add-completed");
                }
            }
        }
    }

    #[test]
    fn backup_retention_warning_is_origin_owned_and_does_not_echo_core_details() {
        let f = fixture();
        let origin = f.session.snapshot().unwrap().ticket;
        core_notices(
            &mut f.session.gate().unwrap(),
            origin,
            vec![VaultWarning {
                kind: VaultWarningKind::BackupRetention,
                message: "fake-private-retention-detail C:/fake/a.kdbx fake-value".into(),
            }],
        );
        let before = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_eq!(before["operations"][0]["kind"], "backupRetention");
        assert_eq!(
            before["operations"][0]["originSession"],
            origin.0.to_string()
        );
        assert!(!before.to_string().contains("fake-private"));
        assert!(!before.to_string().contains("a.kdbx"));
        f.session.lock().unwrap();
        reopen_fixture(&f);
        let after = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_eq!(before["operations"], after["operations"]);
    }

    #[test]
    fn operation_warning_polling_does_not_acknowledge_or_consume_notices() {
        let f = fixture();
        let origin = f.session.snapshot().unwrap().ticket;
        f.session.warn_for(origin, OperationWarningKind::Journal);
        let first = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        f.session.lock().unwrap();
        let second = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_eq!(second["operations"], first["operations"]);
        assert_eq!(second["operations"].as_array().unwrap().len(), 1);
        f.session.warn_for(origin, OperationWarningKind::Preference);
        let id = first["operations"][0]["id"].as_str().unwrap().to_string();
        f.session
            .dismiss_warnings(&[id.clone(), "unseen-id".into()])
            .unwrap();
        let remaining = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_eq!(remaining["operations"].as_array().unwrap().len(), 1);
        assert_eq!(remaining["operations"][0]["kind"], "preference");
        f.session.dismiss_warnings(&[id]).unwrap();
        assert_eq!(
            serde_json::to_value(f.session.warnings().unwrap()).unwrap()["operations"],
            remaining["operations"]
        );
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
                vault.add(akm_core::ApiKeyInput::new(
                    "Saved once",
                    "fake-private-result",
                ))?;
                committed_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                Ok(Zeroizing::new("fake-private-result".to_string()))
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
            crate::test_support::secret_titled(&reopened, "Saved once"),
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
    fn locking_clears_only_the_sessions_clipboard() {
        let f = fixture();
        let stamp = f.session.snapshot().unwrap();
        f.session
            .copy(stamp.ticket, Zeroizing::new("fake".into()))
            .unwrap();
        f.session.lock().unwrap();
        assert_eq!(f.clipboard.lock().unwrap().text, "");
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
    fn a_snapshot_from_before_an_intervening_write_or_reload_cannot_authorize_a_write() {
        let f = fixture();
        let stamp = f.session.snapshot().unwrap();
        f.session
            .access(Access::Write, None, |vault| {
                vault.add(akm_core::ApiKeyInput::new("Other", "fake-other"))?;
                Ok(())
            })
            .unwrap();
        let stale = |stamp| {
            f.session
                .access(Access::Write, Some(stamp), |_| Ok(()))
                .err()
                .map(|error| error.kind)
        };
        assert_eq!(stale(stamp).as_deref(), Some("stalePreview"));
        let stamp = f.session.snapshot().unwrap();
        f.session
            .access(Access::Reload, None, |vault| {
                vault.reload().map_err(Into::into)
            })
            .unwrap();
        assert_eq!(stale(stamp).as_deref(), Some("stalePreview"));
    }

    #[test]
    fn poisoned_vault_mutex_returns_an_error_and_locks_instead_of_panicking() {
        let f = fixture();
        let owner = f
            .session
            .active_owner(f.session.snapshot().unwrap().ticket)
            .unwrap();
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
        assert_eq!(f.native_notices.lock().unwrap().len(), 1);
        assert_eq!(f.native_notices.lock().unwrap()[0].session_id, owner);
    }

    struct BlockingClipboard {
        data: Arc<Mutex<FakeData>>,
        started: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        calls: usize,
        block_on: usize,
    }
    impl ClipboardBackend for BlockingClipboard {
        fn set_private(&mut self, text: &str) -> crate::clipboard::Publication {
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

    /// Unit 1a, design revision 3, section 3.1: the real publisher and cleanup over the fake
    /// Win32 API, through `copy_stamped` (the function both production copy commands call).
    #[cfg(windows)]
    mod real_publisher {
        use super::*;
        use crate::clipboard::win32_fake::{FakeApi, FakeState, Fault, TransactionBackend};

        const A: &str = "fake-copy-A";
        const B: &str = "fake-copy-B";
        // 1A-A1-01: AKM holds a lease, but does not observe that its copy is still there.
        const EARLIER_MAY_REMAIN: &str = "AKM's earlier copy may still be on the clipboard.";
        const EMPTIED: &str =
            "The earlier clipboard content was removed, and the new value was not copied. Copy again.";
        // 1A-A1-02: one text for each result of the immediate cleanup after a failed rollback.
        const RESIDUAL: &str = "AKM could not confirm that the partial copy was removed. Check the clipboard before you paste.";
        const RESIDUAL_REMOVED: &str = "AKM then removed the partial copy. Copy again.";
        const RESIDUAL_REPLACED: &str =
            "The clipboard changed after the partial copy, and AKM left the newer content in place. Copy again.";
        const USER: &str = "fake-user-copy";

        /// The Unicode text bytes that a user copy puts on the fake clipboard.
        fn units(text: &str) -> Vec<u8> {
            text.encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_ne_bytes)
                .collect()
        }

        fn install(f: &Fixture) -> Arc<Mutex<FakeState>> {
            let state = Arc::new(Mutex::new(FakeState::default()));
            f.session.gate().unwrap().clipboard =
                ClipboardLease::new(Box::new(TransactionBackend(FakeApi(state.clone()))));
            state
        }

        fn copy(f: &Fixture, text: &str) -> Result<(), CommandError> {
            f.session.copy_stamped(Stamped {
                snapshot: f.session.snapshot().unwrap(),
                value: Zeroizing::new(text.into()),
            })
        }

        fn generation(f: &Fixture) -> Option<u64> {
            f.session.gate().unwrap().clipboard.owned_generation()
        }

        fn uncertain() -> String {
            crate::clipboard::uncertain_cleanup().message
        }

        fn application_messages(f: &Fixture) -> Vec<String> {
            f.session
                .warnings()
                .unwrap()
                .application
                .into_iter()
                .map(|n| n.message)
                .collect()
        }

        #[derive(Clone, Copy, Debug, PartialEq)]
        enum Lease {
            Kept,
            Gone,
            New,
        }

        #[derive(Clone, Copy, Debug)]
        enum Said {
            Untouched,
            Emptied,
            Uncertain,
            Residual,
            ResidualRemoved,
            Ok,
        }

        struct Base {
            registered: usize,
            allocations: usize,
            opens: usize,
            empties: usize,
            sets: usize,
        }

        struct Case {
            name: &'static str,
            text: &'static str,
            faults: fn(&Base) -> Vec<Fault>,
            lease: Lease,
            said: Said,
            remains: &'static str,
            warns: bool,
        }

        fn cases() -> Vec<Case> {
            vec![
                Case {
                    name: "NUL in the value",
                    text: "fake\0copy",
                    faults: |_| vec![],
                    lease: Lease::Kept,
                    said: Said::Untouched,
                    remains: A,
                    warns: false,
                },
                Case {
                    name: "register",
                    text: B,
                    faults: |b| vec![Fault::Register(b.registered + 1)],
                    lease: Lease::Kept,
                    said: Said::Untouched,
                    remains: A,
                    warns: false,
                },
                Case {
                    name: "allocate",
                    text: B,
                    faults: |b| vec![Fault::Allocate(b.allocations + 1)],
                    lease: Lease::Kept,
                    said: Said::Untouched,
                    remains: A,
                    warns: false,
                },
                Case {
                    name: "owner",
                    text: B,
                    faults: |_| vec![Fault::Owner],
                    lease: Lease::Kept,
                    said: Said::Untouched,
                    remains: A,
                    warns: false,
                },
                Case {
                    name: "open busy",
                    text: B,
                    faults: |b| vec![Fault::Open(b.opens + 1)],
                    lease: Lease::Kept,
                    said: Said::Untouched,
                    remains: A,
                    warns: false,
                },
                Case {
                    name: "empty fails, sequence unchanged",
                    text: B,
                    faults: |b| vec![Fault::Empty(b.empties + 1)],
                    lease: Lease::Kept,
                    said: Said::Untouched,
                    remains: A,
                    warns: false,
                },
                Case {
                    name: "empty fails, sequence changed",
                    text: B,
                    faults: |b| vec![Fault::EmptyChanged(b.empties + 1)],
                    lease: Lease::Gone,
                    said: Said::Uncertain,
                    remains: A,
                    warns: true,
                },
                Case {
                    name: "exclusion set",
                    text: B,
                    faults: |b| vec![Fault::Set(b.sets + 1)],
                    lease: Lease::Gone,
                    said: Said::Emptied,
                    remains: "",
                    warns: false,
                },
                Case {
                    name: "exclusion check",
                    text: B,
                    faults: |_| vec![Fault::ZeroRead(0xc000)],
                    lease: Lease::Gone,
                    said: Said::Emptied,
                    remains: "",
                    warns: false,
                },
                Case {
                    name: "empty frame set",
                    text: B,
                    faults: |b| vec![Fault::Set(b.sets + 4)],
                    lease: Lease::Gone,
                    said: Said::Emptied,
                    remains: "",
                    warns: false,
                },
                Case {
                    name: "first secret set",
                    text: B,
                    faults: |b| vec![Fault::Set(b.sets + 8)],
                    lease: Lease::Gone,
                    said: Said::Emptied,
                    remains: "",
                    warns: false,
                },
                Case {
                    name: "later secret set",
                    text: B,
                    faults: |b| vec![Fault::Set(b.sets + 9)],
                    lease: Lease::Gone,
                    said: Said::Emptied,
                    remains: "",
                    warns: false,
                },
                Case {
                    name: "read-back unreadable",
                    text: B,
                    faults: |_| vec![Fault::TextRead],
                    lease: Lease::Gone,
                    said: Said::Emptied,
                    remains: "",
                    warns: false,
                },
                // Design 3.3, mutant 2: the written phase starts before the first secret set.
                // Only the empty frame formats remain; cleanup cannot read them, so the
                // lease stays for a retry and the uncertainty warning shows.
                Case {
                    name: "rollback fails after the first secret set",
                    text: B,
                    faults: |b| vec![Fault::Set(b.sets + 8), Fault::Empty(b.empties + 2)],
                    lease: Lease::New,
                    said: Said::Residual,
                    remains: "",
                    warns: true,
                },
                Case {
                    name: "rollback fails, sequence unchanged",
                    text: B,
                    faults: |b| vec![Fault::Set(b.sets + 9), Fault::Empty(b.empties + 2)],
                    lease: Lease::Gone,
                    said: Said::ResidualRemoved,
                    remains: "",
                    warns: false,
                },
                Case {
                    name: "rollback fails, sequence changed",
                    text: B,
                    faults: |b| vec![Fault::Set(b.sets + 9), Fault::EmptyChanged(b.empties + 2)],
                    lease: Lease::Gone,
                    said: Said::Uncertain,
                    remains: B,
                    warns: true,
                },
                Case {
                    name: "success",
                    text: B,
                    faults: |_| vec![],
                    lease: Lease::New,
                    said: Said::Ok,
                    remains: B,
                    warns: false,
                },
            ]
        }

        #[test]
        fn every_copy_outcome_through_copy_stamped_matches_the_lease_table() {
            let mut failures = Vec::new();
            for earlier in [true, false] {
                for case in cases() {
                    let name = case.name;
                    let run = std::panic::AssertUnwindSafe(|| run_case(case, earlier));
                    if let Err(panic) = std::panic::catch_unwind(run) {
                        let text = panic
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| panic.downcast_ref::<&str>().map(|t| t.to_string()))
                            .unwrap_or_default();
                        failures.push(text.lines().next().unwrap_or(name).to_string());
                    }
                }
            }
            assert!(
                failures.is_empty(),
                "failing cases:
{}",
                failures.join(
                    "
"
                )
            );
        }

        fn run_case(case: Case, earlier: bool) {
            let label = format!("{} (earlier lease: {earlier})", case.name);
            let f = fixture();
            let state = install(&f);
            if earlier {
                copy(&f, A).unwrap();
            }
            let before = generation(&f);
            {
                let mut s = state.lock().unwrap();
                let base = Base {
                    registered: s.registered_names.len(),
                    allocations: s.allocations,
                    opens: s.open_calls,
                    empties: s.empty_calls,
                    sets: s.set_calls,
                };
                s.faults = (case.faults)(&base);
            }
            let result = copy(&f, case.text);
            assert!(
                state.lock().unwrap().faults.is_empty(),
                "{label}: the fault was not reached"
            );
            let after = generation(&f);
            let lease = match (before, after) {
                (_, None) => Lease::Gone,
                (Some(a), Some(b)) if a == b => Lease::Kept,
                _ => Lease::New,
            };
            let expected = match (case.lease, earlier) {
                (Lease::Kept, false) => Lease::Gone,
                (other, _) => other,
            };
            assert_eq!(lease, expected, "{label}: lease");
            let remains = if !earlier && case.remains == A {
                ""
            } else {
                case.remains
            };
            assert_eq!(
                state.lock().unwrap().text(),
                remains,
                "{label}: clipboard text"
            );
            match (case.said, result) {
                (Said::Ok, Ok(())) => {}
                (Said::Untouched, Err(e)) => {
                    assert_eq!(
                        e.message.ends_with(EARLIER_MAY_REMAIN),
                        earlier,
                        "{label}: {}",
                        e.message
                    )
                }
                (Said::Emptied, Err(e)) => {
                    assert!(e.message.ends_with(EMPTIED), "{label}: {}", e.message);
                    if case.name == "read-back unreadable" {
                        assert!(
                            e.message.starts_with("Windows did not confirm the copy."),
                            "{label}: {}",
                            e.message
                        );
                    }
                }
                (Said::Residual, Err(e)) => {
                    assert!(e.message.ends_with(RESIDUAL), "{label}: {}", e.message)
                }
                (Said::ResidualRemoved, Err(e)) => {
                    assert!(
                        e.message.ends_with(RESIDUAL_REMOVED),
                        "{label}: {}",
                        e.message
                    )
                }
                (Said::Uncertain, Err(e)) => {
                    assert!(e.message.ends_with(&uncertain()), "{label}: {}", e.message)
                }
                (said, result) => panic!(
                    "{label}: expected {said:?}, got {:?}",
                    result.err().map(|e| e.message)
                ),
            }
            let warned = application_messages(&f).iter().any(|m| m == &uncertain());
            assert_eq!(
                warned,
                case.warns && (earlier || case.name.starts_with("rollback")),
                "{label}: warning"
            );
            // Later cleanup: a lease that is gone causes no further clipboard calls.
            let empties = state.lock().unwrap().empty_calls;
            f.clock.set(30);
            f.session.tick();
            f.session.lock().unwrap();
            let s = state.lock().unwrap();
            match expected {
                Lease::Gone => assert_eq!(
                    s.empty_calls, empties,
                    "{label}: cleanup after the lease ended"
                ),
                Lease::Kept | Lease::New => {
                    assert_eq!(s.text(), "", "{label}: the lease was cleaned up")
                }
            }
        }

        #[test]
        fn a_failed_cleanup_empty_that_changed_the_sequence_ends_the_lease_with_one_warning() {
            let f = fixture();
            let state = install(&f);
            copy(&f, A).unwrap();
            let next = state.lock().unwrap().empty_calls + 1;
            state.lock().unwrap().faults = vec![Fault::EmptyChanged(next)];
            f.clock.set(30);
            f.session.tick();
            assert!(state.lock().unwrap().faults.is_empty());
            assert_eq!(
                generation(&f),
                None,
                "the lease must not wait for a later sequence"
            );
            assert_eq!(application_messages(&f), vec![uncertain()]);
            let empties = state.lock().unwrap().empty_calls;
            f.clock.set(31);
            f.session.tick();
            f.session.lock().unwrap();
            assert_eq!(state.lock().unwrap().empty_calls, empties);
            assert_eq!(application_messages(&f), vec![uncertain()]);
        }

        #[test]
        fn a_failed_cleanup_empty_with_the_sequence_unchanged_is_retried() {
            let f = fixture();
            let state = install(&f);
            copy(&f, A).unwrap();
            let next = state.lock().unwrap().empty_calls + 1;
            state.lock().unwrap().faults = vec![Fault::Empty(next)];
            f.clock.set(30);
            f.session.tick();
            assert!(generation(&f).is_some());
            assert_eq!(application_messages(&f), vec![uncertain()]);
            f.clock.set(31);
            f.session.tick();
            assert_eq!(generation(&f), None);
            assert_eq!(state.lock().unwrap().text(), "");
        }

        #[test]
        fn cleanup_keeps_a_newer_identical_user_copy() {
            let f = fixture();
            let state = install(&f);
            copy(&f, A).unwrap();
            let mut units: Vec<u8> = A
                .encode_utf16()
                .chain(Some(0))
                .flat_map(u16::to_ne_bytes)
                .collect();
            state.lock().unwrap().user_copy(std::mem::take(&mut units));
            f.clock.set(30);
            f.session.tick();
            assert_eq!(generation(&f), None);
            assert_eq!(state.lock().unwrap().text(), A);
            assert!(application_messages(&f).is_empty());
        }

        #[test]
        fn a_busy_clipboard_at_expiry_lock_and_exit_leaves_the_copy_and_one_warning() {
            let f = fixture();
            let state = install(&f);
            copy(&f, A).unwrap();
            let opens = state.lock().unwrap().open_calls;
            state.lock().unwrap().faults = vec![
                Fault::Open(opens + 1),
                Fault::Open(opens + 2),
                Fault::Open(opens + 3),
            ];
            f.clock.set(30);
            f.session.tick();
            f.session.lock().unwrap();
            assert_eq!(application_messages(&f), vec![uncertain()]);
            drop(f);
            let s = state.lock().unwrap();
            assert!(
                s.faults.is_empty(),
                "expiry, lock and exit each made one attempt"
            );
            assert_eq!(s.text(), A, "best effort: the copy remains");
        }

        /// 1A-A1-01: the user replaced copy A, then copy B failed before any clipboard
        /// change. AKM still holds A's lease, but it did not observe A on the clipboard.
        #[test]
        fn a_failed_copy_after_a_user_copy_states_uncertainty_and_keeps_the_user_copy() {
            let f = fixture();
            let state = install(&f);
            copy(&f, A).unwrap();
            state.lock().unwrap().user_copy(units(USER));
            let opens = state.lock().unwrap().open_calls;
            state.lock().unwrap().faults = vec![Fault::Open(opens + 1)];
            let error = copy(&f, B).unwrap_err();
            assert!(
                state.lock().unwrap().faults.is_empty(),
                "the busy open was reached"
            );
            assert!(
                error.message.ends_with(EARLIER_MAY_REMAIN),
                "{}",
                error.message
            );
            assert!(
                !error.message.contains("is still on the clipboard"),
                "{}",
                error.message
            );
            assert_eq!(state.lock().unwrap().text(), USER);
            f.clock.set(30);
            f.session.tick();
            f.session.lock().unwrap();
            assert_eq!(
                state.lock().unwrap().text(),
                USER,
                "cleanup keeps the user copy"
            );
            assert_eq!(generation(&f), None);
            assert!(application_messages(&f).is_empty());
        }

        fn fail_rollback(state: &Arc<Mutex<FakeState>>) {
            let mut s = state.lock().unwrap();
            // The second secret format fails after the Unicode value is set, and the rollback
            // empty fails with the sequence unchanged: outcome `Residual`.
            s.faults = vec![Fault::Set(s.set_calls + 9), Fault::Empty(s.empty_calls + 2)];
        }

        /// 1A-A1-02: the immediate cleanup after a failed rollback removed the partial copy.
        #[test]
        fn a_failed_rollback_whose_immediate_cleanup_succeeds_says_the_copy_was_removed() {
            for earlier in [false, true] {
                let f = fixture();
                let state = install(&f);
                if earlier {
                    copy(&f, A).unwrap();
                }
                fail_rollback(&state);
                let clears = state.lock().unwrap().clears;
                let error = copy(&f, B).unwrap_err();
                let s = state.lock().unwrap();
                assert!(
                    s.faults.is_empty(),
                    "earlier {earlier}: the faults were reached"
                );
                // The publication's first empty removes A; the immediate cleanup removes B.
                assert_eq!(
                    s.clears,
                    clears + 1 + usize::from(earlier),
                    "earlier {earlier}: the cleanup emptied B"
                );
                assert_eq!(s.text(), "");
                drop(s);
                assert!(
                    error.message.ends_with(RESIDUAL_REMOVED),
                    "earlier {earlier}: {}",
                    error.message
                );
                assert_eq!(generation(&f), None);
                assert!(application_messages(&f).is_empty());
            }
        }

        /// 1A-A1-02: a user copy replaced the partial copy before the immediate cleanup.
        #[test]
        fn a_failed_rollback_then_a_newer_user_copy_says_akm_left_it() {
            let f = fixture();
            let state = install(&f);
            fail_rollback(&state);
            // Applied when the publication closes the clipboard, before the cleanup opens it.
            state.lock().unwrap().pending_user_copy = Some(units(USER));
            let error = copy(&f, B).unwrap_err();
            assert!(state.lock().unwrap().faults.is_empty());
            assert!(
                error.message.ends_with(RESIDUAL_REPLACED),
                "{}",
                error.message
            );
            assert_eq!(state.lock().unwrap().text(), USER);
            assert_eq!(generation(&f), None);
            assert!(application_messages(&f).is_empty());
        }

        /// 1A-A1-02: the immediate cleanup is busy, so the partial copy may remain.
        #[test]
        fn a_failed_rollback_with_a_busy_immediate_cleanup_warns_and_retries() {
            let f = fixture();
            let state = install(&f);
            fail_rollback(&state);
            {
                let mut s = state.lock().unwrap();
                let opens = s.open_calls;
                s.faults.push(Fault::Open(opens + 2));
            }
            let error = copy(&f, B).unwrap_err();
            assert!(state.lock().unwrap().faults.is_empty());
            assert!(error.message.ends_with(RESIDUAL), "{}", error.message);
            assert_eq!(
                state.lock().unwrap().text(),
                B,
                "best effort: the partial copy remains"
            );
            assert!(generation(&f).is_some(), "the lease stays for a retry");
            assert_eq!(application_messages(&f), vec![uncertain()]);
            f.session.tick();
            assert_eq!(state.lock().unwrap().text(), "", "the due retry removed it");
            assert_eq!(generation(&f), None);
        }

        #[derive(Clone, Copy, Debug)]
        enum At {
            Tick,
            Lock,
        }

        fn attempt(f: &Fixture, at: At) {
            match at {
                At::Tick => {
                    f.clock.set(30);
                    f.session.tick();
                }
                At::Lock => f.session.lock().unwrap(),
            }
        }

        struct Cleanup {
            name: &'static str,
            setup: fn(&mut FakeState),
            /// The lease stays, and the next tick tries again.
            retried: bool,
            warns: bool,
            /// The sequence change that the attempt itself made.
            sequence_step: u32,
            remains: &'static str,
            /// The text after the next tick.
            after_retry: &'static str,
        }

        fn cleanups() -> Vec<Cleanup> {
            vec![
                Cleanup {
                    name: "busy open",
                    setup: |s| s.faults = vec![Fault::Open(s.open_calls + 1)],
                    retried: true,
                    warns: true,
                    sequence_step: 0,
                    remains: A,
                    after_retry: "",
                },
                Cleanup {
                    name: "current sequence 0",
                    setup: |s| s.faults = vec![Fault::Sequence],
                    retried: true,
                    warns: true,
                    sequence_step: 0,
                    remains: A,
                    after_retry: "",
                },
                Cleanup {
                    name: "text different, same sequence",
                    setup: |s| {
                        s.formats.insert(13, units("fake-other"));
                    },
                    retried: true,
                    warns: true,
                    sequence_step: 0,
                    remains: "fake-other",
                    after_retry: "fake-other",
                },
                Cleanup {
                    name: "text unreadable",
                    setup: |s| s.faults = vec![Fault::TextRead],
                    retried: true,
                    warns: true,
                    sequence_step: 0,
                    remains: A,
                    after_retry: "",
                },
                Cleanup {
                    name: "empty fails, sequence unchanged",
                    setup: |s| s.faults = vec![Fault::Empty(s.empty_calls + 1)],
                    retried: true,
                    warns: true,
                    sequence_step: 0,
                    remains: A,
                    after_retry: "",
                },
                Cleanup {
                    name: "empty fails, sequence changed (clipboardLost)",
                    setup: |s| s.faults = vec![Fault::EmptyChanged(s.empty_calls + 1)],
                    retried: false,
                    warns: true,
                    sequence_step: 1,
                    remains: A,
                    after_retry: A,
                },
                Cleanup {
                    name: "newer identical user copy",
                    setup: |s| s.user_copy(units(A)),
                    retried: false,
                    warns: false,
                    sequence_step: 0,
                    remains: A,
                    after_retry: A,
                },
                Cleanup {
                    name: "success",
                    setup: |_| {},
                    retried: false,
                    warns: false,
                    sequence_step: 1,
                    remains: "",
                    after_retry: "",
                },
            ]
        }

        /// 1A-A1-03: every cleanup case of design revision 3, section 3.1, through the real
        /// cleanup over fake Win32, at a due tick and at lock.
        #[test]
        fn every_cleanup_case_at_a_due_tick_and_at_lock_matches_the_table() {
            let mut failures = Vec::new();
            for at in [At::Tick, At::Lock] {
                for case in cleanups() {
                    let run = std::panic::AssertUnwindSafe(|| run_cleanup(&case, at));
                    if let Err(panic) = std::panic::catch_unwind(run) {
                        let text = panic
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| panic.downcast_ref::<&str>().map(|t| t.to_string()))
                            .unwrap_or_default();
                        failures.push(format!(
                            "{} at {at:?}: {}",
                            case.name,
                            text.lines().next().unwrap_or("")
                        ));
                    }
                }
            }
            assert!(
                failures.is_empty(),
                "failing cases:\n{}",
                failures.join("\n")
            );
        }

        fn run_cleanup(case: &Cleanup, at: At) {
            let label = format!("{} at {at:?}", case.name);
            let f = fixture();
            let state = install(&f);
            copy(&f, A).unwrap();
            let lease = generation(&f);
            (case.setup)(&mut state.lock().unwrap());
            let sequence = state.lock().unwrap().sequence;
            attempt(&f, at);
            let s = state.lock().unwrap();
            assert!(s.faults.is_empty(), "{label}: the fault was reached");
            assert_eq!(
                s.sequence,
                sequence + case.sequence_step,
                "{label}: sequence"
            );
            assert_eq!(s.text(), case.remains, "{label}: clipboard text");
            let opens = s.open_calls;
            drop(s);
            let expected_lease = if case.retried { lease } else { None };
            assert_eq!(generation(&f), expected_lease, "{label}: lease");
            let warnings = f.session.warnings().unwrap().application;
            let messages: Vec<_> = warnings.iter().map(|n| n.message.clone()).collect();
            let expected: Vec<String> = if case.warns {
                vec![uncertain()]
            } else {
                vec![]
            };
            assert_eq!(messages, expected, "{label}: warnings");
            let ids: Vec<_> = warnings.iter().map(|n| n.id.clone()).collect();
            // The retry: a due tick while unlocked, a locked tick after lock.
            f.clock.set(31);
            f.session.tick();
            let s = state.lock().unwrap();
            assert_eq!(s.open_calls > opens, case.retried, "{label}: retry");
            assert_eq!(s.text(), case.after_retry, "{label}: text after the retry");
            drop(s);
            let after: Vec<_> = f
                .session
                .warnings()
                .unwrap()
                .application
                .into_iter()
                .map(|n| n.id)
                .collect();
            assert_eq!(after, ids, "{label}: one warning ID for the episode");
        }
    }
}
