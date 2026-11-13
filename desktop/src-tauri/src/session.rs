use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard, PoisonError, TryLockError, Weak,
    },
    time::{Duration, Instant},
};

use akm_core::{
    vault::{CommitGate, VaultWarning, VaultWarningKind},
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

impl Snapshot {
    /// The same session and the same vault contents. The presentation is not part of it.
    pub fn same_vault(&self, other: &Snapshot) -> bool {
        self.ticket == other.ticket && self.revision == other.revision
    }
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
    TerminalStarted,
    TerminalSessionEnded,
    TerminalFailed,
    TerminalDeclined,
    FilePermissions,
    JournalLineageUnconfirmed,
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

/// How many single-key disclosures (a reveal or a copy) AKM makes in a rolling minute before
/// the next one needs the approval of the user in a native window. A page can read its
/// session ID, so the ID is no user gesture: this budget stops a loop of single requests
/// from doing the work of one bulk request (finding FR-03). The budget counts across
/// sessions, because a page that knows the password can lock and unlock.
pub const FREE_SINGLE_DISCLOSURES: usize = 5;
pub const DISCLOSURE_WINDOW: Duration = Duration::from_secs(60);

/// What the budget decided about one single-key request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    /// Inside the budget. The request is counted.
    Free,
    /// Over the budget. The request goes on only after the user approves it in a native window.
    NeedsApproval,
}

/// The times of the single-key disclosures of this run of AKM. Nothing clears it: a lock and
/// a new opening leave it as it is. A time leaves it when its rolling minute has passed.
#[derive(Default)]
struct DisclosureLog {
    times: Vec<Duration>,
}

impl DisclosureLog {
    fn recent(&mut self, now: Duration) -> &mut Vec<Duration> {
        self.times
            .retain(|at| now.saturating_sub(*at) < DISCLOSURE_WINDOW);
        &mut self.times
    }
}

/// The one native confirmation window for a disclosure that is open. A second request waits
/// no turn: it is refused, so a page cannot stack windows over the user.
pub struct ReviewSlot<'a>(&'a AtomicBool);

impl Drop for ReviewSlot<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct Gate {
    policy: Policy,
    owner: Option<NativeOwner>,
    revision: u64,
    presentation: u64,
    /// A lock transition was seen and its cleanup attempt is still owed. The guard that
    /// saw the transition settles it after it released the gate (unit 1h, design D2).
    cleanup_owed: bool,
    disclosures: DisclosureLog,
    path: Option<PathBuf>,
    warnings: Vec<ApplicationNotice>,
    notices: Vec<OperationNotice>,
}

#[derive(Default)]
struct StoredVault {
    ticket: Option<Ticket>,
    vault: Option<Vault>,
}

/// The clipboard lease and the bookkeeping that belongs to it. Lock order is vault, then this
/// cell, then the gate: code that holds the gate never waits for the cell.
struct LeaseCell {
    clipboard: ClipboardLease,
    /// The session that made the current lease; a lease of any other ticket is stale.
    ticket: Option<Ticket>,
    /// The lease generation whose cleanup failure already has a warning.
    episode: Option<u64>,
}

/// A gate guard. A lock transition that the call saw is settled when the guard is dropped,
/// after the gate was released, so a cleanup never runs under the gate.
struct GateGuard<'a> {
    session: &'a Session,
    guard: Option<MutexGuard<'a, Gate>>,
    flush: bool,
}

impl Deref for GateGuard<'_> {
    type Target = Gate;
    fn deref(&self) -> &Gate {
        self.guard.as_ref().expect("the guard lives until drop")
    }
}

impl DerefMut for GateGuard<'_> {
    fn deref_mut(&mut self) -> &mut Gate {
        self.guard.as_mut().expect("the guard lives until drop")
    }
}

impl Drop for GateGuard<'_> {
    fn drop(&mut self) {
        let Some(mut guard) = self.guard.take() else {
            return;
        };
        let owed = self.flush && std::mem::take(&mut guard.cleanup_owed);
        drop(guard);
        if owed {
            self.session.settle(false);
        }
    }
}

enum Step {
    None,
    Clear,
    Tick(Duration),
}

pub struct Session {
    /// This session, for the vault commit gate that a vault access installs. Weak, because
    /// the stored vault holds that gate and the session holds the stored vault.
    me: Weak<Session>,
    gate: Mutex<Gate>,
    lease: Mutex<LeaseCell>,
    exiting: AtomicBool,
    review_open: AtomicBool,
    vault: Mutex<StoredVault>,
    open_serial: Mutex<()>,
    clock: Arc<dyn Clock>,
    notify_locked: Arc<dyn Fn(NativeLockNotice) + Send + Sync>,
    /// Runs at the vault commit point, before the gate is taken. A test places a barrier here.
    #[cfg(test)]
    commit_hook: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
}

#[derive(Clone)]
pub struct VaultState(pub Arc<Session>);

/// What a commit is bound to. The commit checks it under the gate, just before the act.
#[derive(Clone, Copy)]
pub enum Binding<'a> {
    /// The session of a native step that holds its ticket (an opening).
    Ticket(Ticket),
    /// The session that a page request names.
    Named(&'a str),
    /// A session and a vault revision: a write since then refuses the act.
    Revision(Snapshot),
}

/// A prepared act (`docs/design/commit-point.md`). The slow work is done before the commit:
/// `act` is one short step under the gate, and `revoke` undoes the preparation.
pub trait Effect {
    type Output;
    /// Runs under the gate, for the session that the binding names. It must be short and must
    /// not call back into the session: the gate is not reentrant.
    fn act(&mut self, held: &Held<'_>) -> Result<Self::Output, CommandError>;
    /// Runs after the gate is released, when the check refused the act or the act failed. An
    /// error is a fixed text for an application warning: no path and no value.
    fn revoke(&mut self) -> Result<(), String>;
}

/// An act with nothing to undo, such as a copy into memory that is zeroed on drop.
pub struct Act<F>(Option<F>);

impl<F> Act<F> {
    pub fn new(act: F) -> Self {
        Self(Some(act))
    }
}

impl<T, F: FnOnce(&Held<'_>) -> Result<T, CommandError>> Effect for Act<F> {
    type Output = T;

    fn act(&mut self, held: &Held<'_>) -> Result<T, CommandError> {
        match self.0.take() {
            Some(act) => act(held),
            None => Err(CommandError::internal("An act ran twice.")),
        }
    }

    fn revoke(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// The vault file replacement, as an act. The core removes its own temporary files when the
/// replacement is refused or fails, so there is nothing to revoke here.
struct Replacement<'r> {
    replace: &'r mut dyn FnMut() -> akm_core::Result<()>,
    failure: Option<akm_core::Error>,
}

impl Effect for Replacement<'_> {
    type Output = ();

    fn act(&mut self, _held: &Held<'_>) -> Result<(), CommandError> {
        (self.replace)().map_err(|error| {
            self.failure = Some(error);
            CommandError::new("io", "The vault file could not be replaced.")
        })
    }

    fn revoke(&mut self) -> Result<(), String> {
        Ok(())
    }
}

/// What the vault commit gate of one access saw.
#[derive(Default)]
struct CommitLog {
    /// The revision that the next replacement is bound to.
    revision: u64,
    committed: bool,
    refusal: Option<CommandError>,
}

/// The core `CommitGate` of one vault access (`docs/design/commit-point.md`, section 3.3).
/// A write access binds it to its session and revision. Any other access, and the stored
/// vault between accesses, gets a closed gate that refuses every replacement.
struct VaultCommit {
    session: Weak<Session>,
    ticket: Option<Ticket>,
    log: Mutex<CommitLog>,
}

impl VaultCommit {
    fn new(session: Weak<Session>, ticket: Option<Ticket>, revision: u64) -> Self {
        Self {
            session,
            ticket,
            log: Mutex::new(CommitLog {
                revision,
                ..CommitLog::default()
            }),
        }
    }

    fn closed() -> Arc<dyn CommitGate> {
        Arc::new(Self::new(Weak::new(), None, 0))
    }

    fn finish(&self) -> CommitLog {
        std::mem::take(&mut *self.log.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl CommitGate for VaultCommit {
    fn commit(&self, replace: &mut dyn FnMut() -> akm_core::Result<()>) -> akm_core::Result<()> {
        let mut log = self.log.lock().unwrap_or_else(PoisonError::into_inner);
        let (Some(session), Some(ticket)) = (self.session.upgrade(), self.ticket) else {
            log.refusal = Some(CommandError::internal(
                "The vault file can change only in a vault write.",
            ));
            return Err(not_written());
        };
        #[cfg(test)]
        session.reach_commit();
        let binding = Binding::Revision(Snapshot {
            ticket,
            revision: log.revision,
            presentation: 0,
        });
        let mut replacement = Replacement {
            replace,
            failure: None,
        };
        match session.commit_point(binding, &mut replacement, true) {
            Ok(()) => {
                log.committed = true;
                log.revision = log.revision.wrapping_add(1);
                Ok(())
            }
            Err(error) => match replacement.failure.take() {
                Some(failure) => Err(failure),
                None => {
                    log.refusal = Some(error);
                    Err(not_written())
                }
            },
        }
    }
}

/// What the core reports for a refused replacement. The access answers with the refusal that
/// the gate recorded, not with this text.
fn not_written() -> akm_core::Error {
    akm_core::Error::Save("the vault file was not replaced: nothing was written".into())
}

/// Proof that the session gate is held for a session that is still current. Only
/// `Session::commit_point` makes one, and it is neither `Clone` nor `Copy`, so it cannot
/// outlive the call that made it. Every act for a session (the vault replacement, the terminal
/// resume, the saved-path rename, the notification) takes `&Held`: a caller that checked the
/// session earlier and acts later has nothing to pass.
pub struct Held<'gate> {
    gate: std::marker::PhantomData<&'gate ()>,
}

impl Held<'_> {
    fn new() -> Self {
        Self {
            gate: std::marker::PhantomData,
        }
    }

    /// For a unit test of a function that takes `&Held` and has no session to hold.
    #[cfg(test)]
    pub fn for_test() -> Self {
        Self::new()
    }
}

impl Session {
    pub fn new(
        clock: Arc<dyn Clock>,
        clipboard: Box<dyn ClipboardBackend>,
        notify_locked: Arc<dyn Fn(NativeLockNotice) + Send + Sync>,
    ) -> Arc<Self> {
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            gate: Mutex::new(Gate {
                policy: Policy::default(),
                owner: None,
                revision: 0,
                presentation: 0,
                cleanup_owed: false,
                disclosures: DisclosureLog::default(),
                path: None,
                warnings: Vec::new(),
                notices: Vec::new(),
            }),
            lease: Mutex::new(LeaseCell {
                clipboard: ClipboardLease::new(clipboard),
                ticket: None,
                episode: None,
            }),
            exiting: AtomicBool::new(false),
            review_open: AtomicBool::new(false),
            vault: Mutex::new(StoredVault::default()),
            open_serial: Mutex::new(()),
            clock,
            notify_locked,
            #[cfg(test)]
            commit_hook: Mutex::new(None),
        })
    }

    /// The gate. A guard that saw a lock transition makes the owed cleanup attempt when it
    /// is dropped.
    fn gate(&self) -> Result<GateGuard<'_>, CommandError> {
        self.gate_with(true)
    }

    /// The gate, for a caller that settles the cleanup itself (it holds the lease cell, or
    /// it is a tick or an open, which settle exactly once).
    fn gate_deferred(&self) -> Result<GateGuard<'_>, CommandError> {
        self.gate_with(false)
    }

    fn gate_with(&self, flush: bool) -> Result<GateGuard<'_>, CommandError> {
        match self.gate.lock() {
            Ok(mut gate) => {
                let was_locked = gate.policy.is_locked();
                gate.policy.expire(self.clock.now());
                if !was_locked && gate.policy.is_locked() {
                    self.locked_gate(&mut gate);
                }
                Ok(GateGuard {
                    session: self,
                    guard: Some(gate),
                    flush,
                })
            }
            Err(poisoned) => {
                let mut gate = poisoned.into_inner();
                gate.policy.observe(Signal::Unobservable);
                self.locked_gate(&mut gate);
                drop(GateGuard {
                    session: self,
                    guard: Some(gate),
                    flush,
                });
                Err(CommandError::internal(
                    "The native session state failed. Restart AKM.",
                ))
            }
        }
    }

    /// A lock transition: the notice goes out first, under the gate. The cleanup attempt is
    /// owed, and happens after the gate was released (design D2).
    fn locked_gate(&self, gate: &mut Gate) {
        gate.cleanup_owed = true;
        self.retire_owner(gate);
    }

    fn cell(&self) -> MutexGuard<'_, LeaseCell> {
        self.lease.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn raw_gate(&self) -> MutexGuard<'_, Gate> {
        self.gate.lock().unwrap_or_else(PoisonError::into_inner)
    }

    #[cfg(test)]
    fn cell_for_test(&self) -> MutexGuard<'_, LeaseCell> {
        self.cell()
    }

    /// The cleanup attempt of one public call (at most one, never a loop). `tick` also
    /// expires a due lease of the current session.
    fn settle(&self, tick: bool) {
        let mut cell = self.cell();
        self.settle_held(&mut cell, tick, false);
    }

    fn settle_held(&self, cell: &mut LeaseCell, tick: bool, only_if_owed: bool) {
        let step = {
            let mut gate = self.raw_gate();
            let owed = std::mem::take(&mut gate.cleanup_owed);
            if (only_if_owed && !owed && !tick) || cell.clipboard.owned_generation().is_none() {
                Step::None
            } else {
                // A tick keeps the due time of a lease that outlived its session, as unit 1a
                // accepted. Any other settle is a lock transition: it cleans a lease that
                // does not belong to the active session, so a late settle of an older session
                // cannot touch a newer session's copy.
                let stale = match cell.ticket {
                    Some(ticket) => gate.policy.is_locked() || gate.policy.generation() != ticket.0,
                    None => true,
                };
                if tick {
                    if gate.policy.is_locked() {
                        Step::Clear
                    } else {
                        Step::Tick(self.clock.now())
                    }
                } else if stale {
                    Step::Clear
                } else {
                    Step::None
                }
            }
        };
        let lease = cell.clipboard.owned_generation();
        let result = match step {
            Step::Clear => cell.clipboard.clear(),
            Step::Tick(now) => cell.clipboard.tick(now),
            Step::None => Ok(()),
        };
        let failure = result
            .err()
            .zip(lease)
            .map(|(error, lease_generation)| CleanupFailure {
                lease_generation,
                error,
            });
        self.account_cleanup(cell, failure);
    }

    fn account_cleanup(&self, cell: &mut LeaseCell, failure: Option<CleanupFailure>) {
        if let Some(failure) = failure {
            if cell.episode != Some(failure.lease_generation) {
                cell.episode = Some(failure.lease_generation);
                application_warning(&mut self.raw_gate(), failure.error.message);
            }
        }
        for note in cell.clipboard.take_notes() {
            application_warning(&mut self.raw_gate(), note.message().to_string());
        }
        // An actual relinquishment or replacement ends the old episode. A successful
        // not-due tick retains the same owned lease, so it does not count as recovery.
        if cell.clipboard.owned_generation() != cell.episode {
            cell.episode = None;
        }
        if cell.clipboard.owned_generation().is_none() {
            cell.ticket = None;
        }
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
        // Expiry seen here is settled by the one attempt below.
        drop(self.gate_deferred());
        self.settle(true);
        self.discard_locked_vault();
    }

    /// Window close and application exit: lock, with one cleanup attempt, once. Later calls
    /// do nothing until the next open. The lease's own drop does not attempt again.
    pub fn shutdown(&self) {
        if self.exiting.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = self.lock();
        self.cell().clipboard.exit_attempted();
    }

    /// Renews the idle deadline of the session that the request names. The page makes the request
    /// when the user acts, but it can arrive after that session ended: a session that is not
    /// the current one is refused, so it never keeps its replacement awake.
    pub fn activity(&self, session_id: &str) -> Result<(), CommandError> {
        let id = uuid::Uuid::parse_str(session_id).map_err(|_| CommandError::locked())?;
        let mut gate = self.gate()?;
        let current = self.current(&mut gate)?;
        gate.owner
            .as_ref()
            .filter(|owner| owner.ticket == current.ticket && owner.id == id)
            .ok_or_else(CommandError::locked)?;
        gate.policy.activity(self.clock.now());
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

    /// Counts one single-key request of the session `ticket` against the budget. The gate is
    /// held only for this count in memory.
    pub fn admit_single_disclosure(&self, ticket: Ticket) -> Result<Admission, CommandError> {
        let mut gate = self.gate()?;
        let now = self.clock.now();
        if self.current(&mut gate)?.ticket != ticket {
            return Err(CommandError::locked());
        }
        let times = gate.disclosures.recent(now);
        if times.len() < FREE_SINGLE_DISCLOSURES {
            times.push(now);
            Ok(Admission::Free)
        } else {
            Ok(Admission::NeedsApproval)
        }
    }

    /// Counts a request that the user approved, so that approvals fill the window too.
    pub fn count_approved_disclosure(&self, ticket: Ticket) -> Result<(), CommandError> {
        let mut gate = self.gate()?;
        let now = self.clock.now();
        if self.current(&mut gate)?.ticket != ticket {
            return Err(CommandError::locked());
        }
        gate.disclosures.recent(now).push(now);
        Ok(())
    }

    /// Takes the one place for a native confirmation window of a disclosure.
    pub fn open_review(&self) -> Result<ReviewSlot<'_>, CommandError> {
        self.review_open
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| {
                CommandError::new(
                    "busy",
                    "Another confirmation window is open. Answer it first, then try again.",
                )
            })?;
        Ok(ReviewSlot(&self.review_open))
    }

    /// The one way to perform a session-scoped effect (`docs/design/commit-point.md`). Under
    /// the gate that lock and `start_open` take, it checks the binding and runs the act, so
    /// neither can retire the session between the check and the act. After the gate is
    /// released, it revokes the preparation when the check refused or the act failed.
    pub fn commit<E: Effect + ?Sized>(
        &self,
        binding: Binding<'_>,
        effect: &mut E,
    ) -> Result<E::Output, CommandError> {
        self.commit_point(binding, effect, false)
    }

    /// `commit`, and for the vault replacement (`advance`) the revision moves under the same
    /// gate hold as the act, so a later commit bound to the old revision refuses.
    fn commit_point<E: Effect + ?Sized>(
        &self,
        binding: Binding<'_>,
        effect: &mut E,
        advance: bool,
    ) -> Result<E::Output, CommandError> {
        let acted = self.gate().and_then(|mut gate| {
            self.bound(&mut gate, binding)?;
            // A panic in the act is caught here, so it never poisons the gate. The act may have
            // happened, so the session ends: nothing builds on memory that may not match the file
            // or the process (U1-R1-04).
            let output =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| effect.act(&Held::new())))
                    .unwrap_or_else(|_| {
                        gate.policy.lock();
                        self.locked_gate(&mut gate);
                        Err(CommandError::failed_step())
                    })?;
            if advance {
                gate.revision = gate.revision.wrapping_add(1);
            }
            Ok(output)
        });
        if acted.is_err() {
            if let Err(message) = effect.revoke() {
                self.warn(message);
            }
        }
        acted
    }

    fn bound(&self, gate: &mut Gate, binding: Binding<'_>) -> Result<(), CommandError> {
        let current = self.current(gate)?;
        match binding {
            Binding::Ticket(ticket) if ticket == current.ticket => Ok(()),
            Binding::Named(session_id) => owned_by(gate, current, session_id),
            Binding::Revision(expected) if expected.ticket == current.ticket => {
                if expected.revision == current.revision {
                    Ok(())
                } else {
                    Err(CommandError::new(
                        "stalePreview",
                        "The vault changed. Start this operation again.",
                    ))
                }
            }
            Binding::Ticket(_) | Binding::Revision(_) => Err(CommandError::locked()),
        }
    }

    /// For a test: `hook` runs at the vault commit point, before the gate is taken.
    #[cfg(test)]
    pub fn at_commit(&self, hook: impl Fn() + Send + Sync + 'static) {
        *self.commit_hook.lock().unwrap() = Some(Box::new(hook));
    }

    #[cfg(test)]
    fn reach_commit(&self) {
        if let Some(hook) = self.commit_hook.lock().unwrap().as_ref() {
            hook();
        }
    }

    #[cfg(test)]
    pub fn vault_is_released(&self) -> bool {
        self.vault.try_lock().is_ok()
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
        self.exiting.store(false, Ordering::SeqCst);
        let reservation = (|| {
            let mut gate = self.gate_deferred()?;
            let ticket = gate.policy.begin_open(self.clock.now());
            self.locked_gate(&mut gate);
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
            Ok((ticket, id))
        })();
        // The older owner was retired above; the one attempt for its lease is here.
        self.settle(false);
        self.discard_locked_vault();
        reservation
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

    /// Expires the presentation of the current session. For a native event (a minimize), which
    /// happens now, in the window that the user sees.
    pub fn expire_presentation(&self) -> Result<Option<NativeViewNotice>, CommandError> {
        self.expire_presentation_matching(None)
    }

    /// Expires the presentation of the session that a request names. The request was made by a
    /// page earlier and may arrive after that session ended: a session that is not the current
    /// one has nothing to expire, so the answer is `None` and the session that replaced it is
    /// not touched. The check and the expiry share one hold of the gate.
    pub fn expire_presentation_of(
        &self,
        session_id: &str,
    ) -> Result<Option<NativeViewNotice>, CommandError> {
        let Ok(id) = uuid::Uuid::parse_str(session_id) else {
            return Ok(None);
        };
        self.expire_presentation_matching(Some(id))
    }

    fn expire_presentation_matching(
        &self,
        only: Option<uuid::Uuid>,
    ) -> Result<Option<NativeViewNotice>, CommandError> {
        let mut gate = self.gate()?;
        let Ok(current) = self.current(&mut gate) else {
            return Ok(None);
        };
        let session_id = gate
            .owner
            .as_ref()
            .filter(|owner| owner.ticket == current.ticket)
            .map(|owner| owner.id)
            .ok_or_else(CommandError::locked)?;
        if only.is_some_and(|id| id != session_id) {
            return Ok(None);
        }
        let session_id = session_id.to_string();
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
            vault.set_commit_gate(Some(VaultCommit::closed()));
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

    /// An access bound to the session and the vault revision, not to the on-screen presentation.
    /// An import is reviewed in a native window, so a hide, a page load or a discard does not
    /// revoke what the user approved there. A lock, another session and another write do, and
    /// they are checked under the gate at the moment of the access.
    pub fn access_reviewed<T>(
        &self,
        access: Access,
        expected: Snapshot,
        operation: impl FnOnce(&mut Vault) -> Result<T, CommandError>,
    ) -> Result<Stamped<T>, CommandError> {
        self.access_checked(access, Some(expected), false, operation)
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
        // Only a write can replace the file, and only through this gate: it checks the
        // session and the revision under the gate at the replacement itself.
        let commit = Arc::new(VaultCommit::new(
            self.me.clone(),
            access.persisted().then_some(before.ticket),
            before.revision,
        ));
        vault.set_commit_gate(Some(commit.clone()));
        // A panic in the operation is caught here, so `finish` below still learns whether the
        // file was replaced (U1-R1-04).
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(vault)));
        let panicked = result.is_err();
        let result = result.unwrap_or_else(|_| Err(CommandError::failed_step()));
        vault.set_commit_gate(Some(VaultCommit::closed()));
        let log = commit.finish();
        let warnings = vault.take_warning_records();
        let ended = |error: CommandError| {
            if log.committed {
                CommandError::completed_while_locked()
            } else {
                error
            }
        };
        let mut gate = match self.gate() {
            Ok(gate) => gate,
            Err(error) => {
                *stored = StoredVault::default();
                return Err(ended(error));
            }
        };
        core_notices(&mut gate, before.ticket, warnings);
        let current = self.current(&mut gate);
        if !current.is_ok_and(|s| s.ticket == before.ticket) {
            *stored = StoredVault::default();
            return Err(ended(CommandError::locked()));
        }
        if let Some(refusal) = log.refusal {
            return Err(refusal);
        }
        let result = match result {
            // After a panic the memory may not match the file, and after a replacement no reply
            // may invite a retry. So the session ends, and a replacement answers
            // `completedWhileLocked` (U1-R1-04).
            Err(error) if panicked || log.committed => {
                *stored = StoredVault::default();
                gate.policy.lock();
                self.locked_gate(&mut gate);
                return Err(ended(error));
            }
            result => result,
        };
        // A write moved the revision at its commit. A reload changes what is in memory.
        if result.is_ok() && matches!(access, Access::Reload) {
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
        self.copy_value(secret.snapshot.ticket, Some(secret.snapshot), secret.value)
    }

    /// `read` is the snapshot that the value was read under. Without one (a test), only the
    /// session is checked.
    fn copy_value(
        &self,
        ticket: Ticket,
        read: Option<Snapshot>,
        text: Zeroizing<String>,
    ) -> Result<(), CommandError> {
        // The cell serializes copy with cleanup. No Vault mutex and no gate is held while the
        // OS clipboard is used, so a lock is never delayed by the copy (design D1).
        let mut cell = self.cell();
        let result = self.copy_held(&mut cell, ticket, read, text);
        self.settle_held(&mut cell, false, true);
        result
    }

    /// Check, publish, check. A refusal at the second check takes the published copy back
    /// (U1-R1-02). That take-back is the one cleanup attempt of the call, so it also settles a
    /// cleanup that a lock owes.
    ///
    /// | Cause (lock, new session, same-session write, view expiry) | Clipboard after | Reply |
    /// |---|---|---|
    /// | before the first check | untouched | the refusal |
    /// | after publication, the copy still AKM's | AKM's copy removed | the refusal |
    /// | after publication, the user copied again | the user's copy kept | the refusal |
    /// | after publication, the take-back fails | AKM's copy may remain; one warning says so | the cleanup error kind, both texts |
    fn copy_held(
        &self,
        cell: &mut LeaseCell,
        ticket: Ticket,
        read: Option<Snapshot>,
        text: Zeroizing<String>,
    ) -> Result<(), CommandError> {
        let check = |this: &Self| -> Result<(), CommandError> {
            let mut gate = this.gate_deferred()?;
            let current = this.current(&mut gate)?;
            deliverable(current, read.unwrap_or(Snapshot { ticket, ..current }))
        };
        check(self)?;
        let before = cell.clipboard.owned_generation();
        let outcome = cell.clipboard.copy_reported(text, self.clock.now());
        let after = cell.clipboard.owned_generation();
        if after.is_some() && after != before {
            cell.ticket = Some(ticket);
        }
        match outcome {
            Ok(_) => self.account_cleanup(cell, None),
            Err(failure) => {
                self.account_cleanup(cell, failure.cleanup);
                return Err(failure.error);
            }
        }
        let refusal = match check(self) {
            Ok(()) => return Ok(()),
            Err(refusal) => refusal,
        };
        if after.is_none() || after == before {
            return Err(refusal);
        }
        self.raw_gate().cleanup_owed = false;
        let taken = cell.clipboard.clear();
        let failure = taken
            .as_ref()
            .err()
            .zip(after)
            .map(|(error, lease_generation)| CleanupFailure {
                lease_generation,
                error: CommandError::new(&error.kind, error.message.clone()),
            });
        self.account_cleanup(cell, failure);
        match taken {
            Ok(()) => Err(refusal),
            Err(error) => Err(CommandError::new(
                &error.kind,
                format!("{} {}", refusal.message, error.message),
            )),
        }
    }

    pub fn reveal(&self, secret: Stamped<Zeroizing<String>>) -> Result<String, CommandError> {
        let mut gate = self.gate()?;
        let current = self.current(&mut gate)?;
        deliverable(current, secret.snapshot)?;
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

    /// The snapshot of the current session, for a request that names the session it was made
    /// for. A request of an older session, or one that arrives after a newer opening, is
    /// refused here, so a page request never acts on a session other than its own.
    pub fn snapshot_for(&self, session_id: &str) -> Result<Snapshot, CommandError> {
        let mut gate = self.gate()?;
        let current = self.current(&mut gate)?;
        owned_by(&gate, current, session_id)?;
        Ok(current)
    }

    /// The outcome record of a dispatched request (`outcome.rs`). It belongs to the
    /// application, not to a session: it must survive a lock and a lost reply. It holds the
    /// opaque operation ID of the page, the session of the request and a fixed kind only.
    /// A poisoned gate still takes the record, so a record is never skipped (U1-R1-04).
    pub fn record_operation(&self, ticket: Ticket, operation_id: &str, kind: OperationWarningKind) {
        self.raw_gate().notices.push(OperationNotice {
            id: operation_id.to_string(),
            origin_session: ticket.0.to_string(),
            kind,
        });
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

    /// Every ID must have the one spelling of `outcome::parse_operation_id`. An ID with any
    /// other spelling refuses the whole call, and nothing is dismissed.
    pub fn dismiss_warnings(&self, ids: &[String]) -> Result<(), CommandError> {
        for id in ids {
            crate::outcome::parse_operation_id(id)?;
        }
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

/// A page request names its session: it acts only while that session is the current one.
fn owned_by(gate: &Gate, current: Snapshot, session_id: &str) -> Result<(), CommandError> {
    let id = uuid::Uuid::parse_str(session_id).map_err(|_| CommandError::locked())?;
    gate.owner
        .as_ref()
        .filter(|owner| owner.ticket == current.ticket && owner.id == id)
        .map(|_| ())
        .ok_or_else(CommandError::locked)
}

/// A value read from the vault leaves AKM (a reveal or a copy) only while the session, the vault
/// revision and the view that it was read under are all still current. A same-session write or
/// reload after the read moves the revision, so the old value is refused (U1-R1-01).
fn deliverable(current: Snapshot, read: Snapshot) -> Result<(), CommandError> {
    if current.ticket != read.ticket {
        return Err(CommandError::locked());
    }
    if current.revision != read.revision {
        return Err(vault_changed());
    }
    if current.presentation != read.presentation {
        return Err(view_changed());
    }
    Ok(())
}

fn vault_changed() -> CommandError {
    CommandError::new(
        "stalePreview",
        "The vault changed. Start this operation again.",
    )
}

fn view_changed() -> CommandError {
    CommandError::new(
        "viewChanged",
        "The protected view expired. Start this disclosure or preview again.",
    )
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
        operation_notice(gate, ticket, notice_kind(warning.kind));
    }
}

/// Each core warning has a notice kind of its own. The page holds the text of each kind; the
/// core's message, which can name a path, never crosses to the page.
fn notice_kind(kind: VaultWarningKind) -> OperationWarningKind {
    match kind {
        VaultWarningKind::Journal => OperationWarningKind::Journal,
        VaultWarningKind::BackupRefresh => OperationWarningKind::BackupRefresh,
        VaultWarningKind::BackupPrune => OperationWarningKind::BackupPrune,
        VaultWarningKind::BackupRetention => OperationWarningKind::BackupRetention,
        VaultWarningKind::FilePermissions => OperationWarningKind::FilePermissions,
        VaultWarningKind::JournalLineageUnconfirmed => {
            OperationWarningKind::JournalLineageUnconfirmed
        }
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

    #[test]
    fn fr05_fr12_each_core_warning_has_a_notice_kind_of_its_own_on_the_wire() {
        let wire = |kind| serde_json::to_value(notice_kind(kind)).unwrap();
        for (kind, name) in [
            (VaultWarningKind::Journal, "journal"),
            (VaultWarningKind::BackupRefresh, "backupRefresh"),
            (VaultWarningKind::BackupPrune, "backupPrune"),
            (VaultWarningKind::BackupRetention, "backupRetention"),
            (VaultWarningKind::FilePermissions, "filePermissions"),
            (
                VaultWarningKind::JournalLineageUnconfirmed,
                "journalLineageUnconfirmed",
            ),
        ] {
            assert_eq!(wire(kind), name, "{kind:?}");
        }
    }

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
        fixture_with(false)
    }

    /// `own_root`: the vault journals in `test_support::own_root` of the folder.
    fn fixture_with(own_root: bool) -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let clock = Arc::new(TestClock::default());
        let clipboard = Arc::new(Mutex::new(FakeData::default()));
        let events = Arc::new(AtomicUsize::new(0));
        let notify = events.clone();
        let native_notices = Arc::new(Mutex::new(Vec::new()));
        let notices = native_notices.clone();
        let session = Session::new(
            clock.clone(),
            Box::new(FakeClipboard(clipboard.clone())),
            Arc::new(move |notice| {
                notify.fetch_add(1, Ordering::SeqCst);
                notices.lock().unwrap().push(notice);
            }),
        );
        session.observe(Signal::Available);
        let ticket = session.begin_open().unwrap();
        session
            .open(ticket, || {
                let path = directory.path().join("vault.kdbx");
                let password = "fake-test-password-123!";
                if own_root {
                    let root = crate::test_support::own_root(directory.path());
                    Vault::create_in(path, password, None, root).map_err(Into::into)
                } else {
                    Vault::create(path, password, None).map_err(Into::into)
                }
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
        entered_rx
            .recv_timeout(crate::test_support::PATIENCE)
            .unwrap();
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
            .cell_for_test()
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
        committed_rx
            .recv_timeout(crate::test_support::PATIENCE)
            .unwrap();
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
        use crate::test_support::canonical;
        for authorized in [false, true] {
            let f = fixture_with(true);
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
                            &crate::private_export::Remnants::default(),
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
            stalled_rx
                .recv_timeout(crate::test_support::PATIENCE)
                .unwrap();
            f.session.expire_presentation().unwrap();
            release_tx.send(()).unwrap();
            let result = worker.join().unwrap();
            assert_eq!(canonical(&result.unwrap()), canonical(&target));
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
                            &crate::private_export::Remnants::default(),
                        )?;
                        prepared_tx.send(()).unwrap();
                        release_rx.recv().unwrap();
                        Ok(prepared)
                    },
                    |prepared| prepared.publish(),
                )
            });
            prepared_rx
                .recv_timeout(crate::test_support::PATIENCE)
                .unwrap();
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
        started_rx
            .recv_timeout(crate::test_support::PATIENCE)
            .unwrap();
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
        let owner = f
            .session
            .active_owner(f.session.snapshot().unwrap().ticket)
            .unwrap();
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
        assert!(f.session.activity(&owner).is_err());
        assert_eq!(f.events.load(Ordering::SeqCst), 1);
    }

    /// P2R2-03, class B: a page request that renews the idle deadline names its session. A late
    /// request of session A must not keep session B awake. B's own activity still does.
    #[test]
    fn p2r2_03_a_stale_sessions_activity_never_keeps_its_replacement_awake() {
        let f = fixture();
        let a = f
            .session
            .active_owner(f.session.snapshot().unwrap().ticket)
            .unwrap();
        reopen_fixture(&f);
        let b = f
            .session
            .active_owner(f.session.snapshot().unwrap().ticket)
            .unwrap();
        assert_ne!(a, b);
        f.clock.set(590);
        assert_eq!(
            f.session.activity(&a).unwrap_err().kind,
            "locked",
            "a request of the retired session is refused"
        );
        assert!(f.session.activity("not-an-id").is_err());
        f.clock.set(600);
        assert!(
            f.session.snapshot().is_err(),
            "the stale request did not renew the deadline of B, so B expires on time"
        );

        // Control: B's own activity does keep B awake.
        let f = fixture();
        reopen_fixture(&f);
        let b = f
            .session
            .active_owner(f.session.snapshot().unwrap().ticket)
            .unwrap();
        f.clock.set(590);
        f.session.activity(&b).unwrap();
        f.clock.set(600);
        assert!(f.session.snapshot().is_ok(), "B's own activity renewed it");
    }

    #[test]
    fn warning_from_a_committed_change_is_owned_after_another_vault_opens() {
        let f = fixture();
        let origin = f.session.snapshot().unwrap().ticket;
        std::fs::create_dir(
            crate::test_support::journal_of(&f._directory.path().join("vault.kdbx")).path(),
        )
        .unwrap();
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
        std::fs::create_dir(
            crate::test_support::journal_of(&f._directory.path().join("vault.kdbx")).path(),
        )
        .unwrap();
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
        committed_rx
            .recv_timeout(crate::test_support::PATIENCE)
            .unwrap();
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
            f.session.cell_for_test().clipboard = ClipboardLease::new(Box::new(BusyCleanup {
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
            assert_eq!(f.session.cell_for_test().clipboard.owned_generation(), None);
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
        f.session.cell_for_test().clipboard = ClipboardLease::new(Box::new(RollbackAfterLease {
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
        assert_eq!(f.session.cell_for_test().clipboard.owned_generation(), None);
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
        f.session.cell_for_test().clipboard = ClipboardLease::new(Box::new(CleanupFault {
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
        f.session.cell_for_test().clipboard = ClipboardLease::new(Box::new(PartialCleanupFault {
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
                // AKM deletes only a backup that its list inside the vault names (FR-01), so the
                // old backups come from real saves: five saves leave five dated backups, and
                // the save under test makes the sixth and prunes the oldest. Each one is held
                // open without delete sharing, so removing it fails and the save reports it.
                let mut held = Vec::new();
                if prune_fails {
                    use std::os::windows::fs::OpenOptionsExt;
                    for index in 0..5 {
                        f.session
                            .access(Access::Write, None, |vault| {
                                vault.add(akm_core::ApiKeyInput::new(
                                    format!("Earlier key {index}"),
                                    "fake-earlier-secret",
                                ))?;
                                Ok(())
                            })
                            .unwrap();
                    }
                    assert!(serde_json::to_value(f.session.warnings().unwrap()).unwrap()
                        ["operations"]
                        .as_array()
                        .unwrap()
                        .is_empty());
                    let dated: Vec<_> = std::fs::read_dir(f._directory.path())
                        .unwrap()
                        .map(|entry| entry.unwrap().path())
                        .filter(|path| {
                            path.file_name()
                                .unwrap()
                                .to_string_lossy()
                                .starts_with("vault.kdbx.bak.")
                        })
                        .collect();
                    assert_eq!(dated.len(), 5);
                    for path in dated {
                        held.push(
                            std::fs::OpenOptions::new()
                                .read(true)
                                .share_mode(1)
                                .open(&path)
                                .unwrap(),
                        );
                    }
                }
                if backup_fails {
                    let legacy = f._directory.path().join("vault.kdbx.bak");
                    if legacy.exists() {
                        std::fs::remove_file(&legacy).unwrap();
                    }
                    std::fs::create_dir(legacy).unwrap();
                }
                let journal =
                    crate::test_support::journal_of(&f._directory.path().join("vault.kdbx"));
                if journal_fails {
                    if journal.path().is_file() {
                        std::fs::remove_file(journal.path()).unwrap();
                    }
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
                saved_rx
                    .recv_timeout(crate::test_support::PATIENCE)
                    .unwrap();
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
            .dismiss_warnings(&[id.clone(), uuid::Uuid::new_v4().to_string()])
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
    fn review1_a_session_effect_runs_only_for_the_current_owner() {
        let f = fixture();
        let old_ticket = f.session.snapshot().unwrap().ticket;
        let old_owner = f.session.active_owner(old_ticket).unwrap();
        let old = f.session.snapshot().unwrap();
        let ran = AtomicUsize::new(0);
        let run = |binding: Binding<'_>| {
            f.session.commit(
                binding,
                &mut Act::new(|_: &Held<'_>| Ok(ran.fetch_add(1, Ordering::SeqCst))),
            )
        };
        assert!(run(Binding::Ticket(old_ticket)).is_ok());
        assert!(run(Binding::Named(&old_owner)).is_ok());
        assert!(run(Binding::Revision(old)).is_ok());
        assert_eq!(ran.load(Ordering::SeqCst), 3);
        let new_owner = f.session.prepare_open().unwrap();
        for (name, result) in [
            ("old ticket", run(Binding::Ticket(old_ticket)).err()),
            ("old owner", run(Binding::Named(&old_owner)).err()),
            ("old revision", run(Binding::Revision(old)).err()),
            (
                "new owner, not opened",
                run(Binding::Named(&new_owner)).err(),
            ),
            ("malformed", run(Binding::Named("not-a-uuid")).err()),
        ] {
            assert_eq!(
                result.unwrap_or_else(|| panic!("{name} ran")).kind,
                "locked"
            );
        }
        assert_eq!(
            ran.load(Ordering::SeqCst),
            3,
            "a retired owner runs no effect"
        );
    }

    #[test]
    fn review1_a_session_effect_holds_the_gate_so_lock_and_a_new_opening_wait_for_it() {
        for newer in ["lock", "open"] {
            let f = fixture();
            let owner = f
                .session
                .active_owner(f.session.snapshot().unwrap().ticket)
                .unwrap();
            let (inside_tx, inside_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let effect_session = f.session.clone();
            let effect = std::thread::spawn(move || {
                effect_session
                    .commit(
                        Binding::Named(&owner),
                        &mut Act::new(|_: &Held<'_>| {
                            inside_tx.send(()).unwrap();
                            release_rx.recv().unwrap();
                            Ok(())
                        }),
                    )
                    .unwrap();
            });
            inside_rx.recv().unwrap();
            let (done_tx, done_rx) = mpsc::channel();
            let retiring = f.session.clone();
            let retire = std::thread::spawn(move || {
                match newer {
                    "lock" => retiring.lock().unwrap(),
                    _ => drop(retiring.prepare_open().unwrap()),
                }
                done_tx.send(()).unwrap();
            });
            assert!(
                done_rx.recv_timeout(Duration::from_millis(300)).is_err(),
                "{newer}: the owner was retired while its effect was running"
            );
            release_tx.send(()).unwrap();
            effect.join().unwrap();
            done_rx.recv().unwrap();
            retire.join().unwrap();
        }
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
        committed_rx
            .recv_timeout(crate::test_support::PATIENCE)
            .unwrap();
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
        started_rx
            .recv_timeout(crate::test_support::PATIENCE)
            .unwrap();
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
        created_rx
            .recv_timeout(crate::test_support::PATIENCE)
            .unwrap();
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
                    let prepared = crate::private_export::prepare_private(
                        &path,
                        "TOKEN=fake\n",
                        &[],
                        &[],
                        &crate::private_export::Remnants::default(),
                    )?;
                    prepared_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok(prepared)
                },
                |prepared| prepared.publish(),
            )
        });
        prepared_rx
            .recv_timeout(crate::test_support::PATIENCE)
            .unwrap();
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
                || {
                    crate::private_export::prepare_private(
                        &path,
                        "TOKEN=fake\n",
                        &[],
                        &[],
                        &crate::private_export::Remnants::default(),
                    )
                },
                |prepared| {
                    authorized_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    prepared.publish()
                },
            )
        });
        authorized_rx
            .recv_timeout(crate::test_support::PATIENCE)
            .unwrap();
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
        // A panic in an access no longer poisons the mutex (U1-R1-04,
        // `u1_r1_04_an_access_that_fails_after_its_replacement_answers_completed_while_locked`),
        // so the test poisons it with a panic while its guard is held.
        let session = f.session.clone();
        assert!(std::thread::spawn(move || {
            let _stored = session.vault.lock().unwrap();
            panic!("injected worker panic");
        })
        .join()
        .is_err());
        assert!(f.session.vault.is_poisoned());
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
        f.session.cell_for_test().clipboard = ClipboardLease::new(Box::new(BlockingClipboard {
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
        // The lock no longer waits for the gate behind a copy (1H-01). It can end the session
        // while the copy runs, so the copy answers `locked`; either way the lock's cleanup
        // removes what the copy published.
        match copy.join().unwrap() {
            Ok(()) => {}
            Err(error) => assert_eq!(error.kind, "locked"),
        }
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
        f.session.cell_for_test().clipboard = ClipboardLease::new(Box::new(BlockingClipboard {
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

    /// Unit 1h, 1H-01 and 1H-02: the lock notice comes first, and one public call makes at
    /// most one cleanup attempt (design D1 to D3).
    mod lock_order {
        use super::*;
        use crate::clipboard::Publication;

        /// Counts cleanup attempts and records how many lock notices had been delivered at
        /// each one. A busy clipboard keeps the lease, as the production backend does.
        struct Counting {
            inner: FakeClipboard,
            notices: Arc<AtomicUsize>,
            attempts: Arc<Mutex<Vec<usize>>>,
            busy: Arc<AtomicBool>,
        }
        impl ClipboardBackend for Counting {
            fn set_private(&mut self, text: &str) -> Publication {
                self.inner.set_private(text)
            }
            fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
                self.attempts
                    .lock()
                    .unwrap()
                    .push(self.notices.load(Ordering::SeqCst));
                if self.busy.load(Ordering::SeqCst) {
                    return Err(CommandError::new(
                        "clipboard",
                        "The clipboard is busy. Try copying again.",
                    ));
                }
                self.inner.clear_matching(sequence, text)
            }
        }

        struct Counted {
            f: Fixture,
            attempts: Arc<Mutex<Vec<usize>>>,
            busy: Arc<AtomicBool>,
        }

        /// A session that owns a copy, with a busy clipboard.
        fn counted() -> Counted {
            let f = fixture();
            let attempts = Arc::new(Mutex::new(Vec::new()));
            let busy = Arc::new(AtomicBool::new(true));
            f.session.cell_for_test().clipboard = ClipboardLease::new(Box::new(Counting {
                inner: FakeClipboard(f.clipboard.clone()),
                notices: f.events.clone(),
                attempts: attempts.clone(),
                busy: busy.clone(),
            }));
            let ticket = f.session.snapshot().unwrap().ticket;
            f.session
                .copy(ticket, Zeroizing::new("fake-owned-copy".into()))
                .unwrap();
            Counted { f, attempts, busy }
        }

        type Entry = (&'static str, bool, bool, fn(&Fixture, Ticket, &str));

        // Columns: entry, the session expires before the call, a lock notice is expected,
        // the call. Each entry is a public call of `Session` that can see a lock transition.
        fn entries() -> Vec<Entry> {
            vec![
                ("lock", false, true, |f, _, _| {
                    let _ = f.session.lock();
                }),
                ("lock_owned", false, true, |f, _, owner| {
                    let _ = f.session.lock_owned(Some(owner));
                }),
                ("observe locked", false, true, |f, _, _| {
                    f.session.observe(Signal::Locked)
                }),
                ("observe suspend", false, true, |f, _, _| {
                    f.session.observe(Signal::Suspend)
                }),
                ("observe unobservable", false, true, |f, _, _| {
                    f.session.observe(Signal::Unobservable)
                }),
                ("shutdown", false, true, |f, _, _| f.session.shutdown()),
                ("prepare_open", false, true, |f, _, _| {
                    let _ = f.session.prepare_open();
                }),
                ("prepare_open after expiry", true, true, |f, _, _| {
                    let _ = f.session.prepare_open();
                }),
                ("tick after expiry", true, true, |f, _, _| f.session.tick()),
                ("snapshot after expiry", true, true, |f, _, _| {
                    let _ = f.session.snapshot();
                }),
                ("ensure after expiry", true, true, |f, t, _| {
                    let _ = f.session.ensure(t);
                }),
                ("activity after expiry", true, true, |f, _, owner| {
                    let _ = f.session.activity(owner);
                }),
                ("path after expiry", true, true, |f, _, _| {
                    let _ = f.session.path();
                }),
                ("warnings after expiry", true, true, |f, _, _| {
                    let _ = f.session.warnings();
                }),
                ("warn after expiry", true, true, |f, _, _| {
                    f.session.warn("fake".into())
                }),
                ("document_started after expiry", true, true, |f, _, _| {
                    f.session.document_started()
                }),
                ("expire_presentation after expiry", true, true, |f, _, _| {
                    let _ = f.session.expire_presentation();
                }),
                ("active_owner after expiry", true, true, |f, t, _| {
                    let _ = f.session.active_owner(t);
                }),
                ("access after expiry", true, true, |f, _, _| {
                    let _ = f.session.access(Access::Read, None, |_| Ok(()));
                }),
                ("copy after expiry", true, true, |f, t, _| {
                    let _ = f.session.copy(t, Zeroizing::new("fake-late".into()));
                }),
                ("claim_open after expiry", true, true, |f, _, _| {
                    let _ = f.session.claim_open(&uuid::Uuid::new_v4().to_string());
                }),
                ("dismiss_warnings after expiry", true, true, |f, _, _| {
                    let _ = f.session.dismiss_warnings(&[]);
                }),
            ]
        }

        #[test]
        fn every_public_entry_sends_its_notice_first_and_makes_exactly_one_attempt() {
            for (name, expire, notice, call) in entries() {
                let c = counted();
                let ticket = c.f.session.snapshot().unwrap().ticket;
                let owner = c.f.session.active_owner(ticket).unwrap();
                if expire {
                    c.f.clock.set(601);
                }
                call(&c.f, ticket, &owner);
                let attempts = c.attempts.lock().unwrap().clone();
                assert_eq!(attempts.len(), 1, "{name}: attempts {attempts:?}");
                assert_eq!(
                    attempts[0],
                    usize::from(notice),
                    "{name}: the notice goes out before the attempt"
                );
                assert_eq!(c.f.events.load(Ordering::SeqCst), 1, "{name}: one notice");
                assert!(
                    c.f.session
                        .cell_for_test()
                        .clipboard
                        .owned_generation()
                        .is_some(),
                    "{name}: a busy attempt keeps the lease"
                );
                // The next public call is a new call: at most one more attempt, never a loop.
                call(&c.f, ticket, &owner);
                assert!(
                    c.attempts.lock().unwrap().len() <= 2,
                    "{name}: a second call made {} attempts",
                    c.attempts.lock().unwrap().len()
                );
            }
        }

        #[test]
        fn a_tick_when_the_lease_is_due_makes_one_attempt_and_no_notice() {
            let c = counted();
            c.f.clock.set(30);
            c.f.session.tick();
            assert_eq!(*c.attempts.lock().unwrap(), vec![0]);
            assert_eq!(c.f.events.load(Ordering::SeqCst), 0);
        }

        #[test]
        fn window_and_app_exit_events_make_one_attempt_in_all() {
            let c = counted();
            for _ in 0..4 {
                c.f.session.shutdown();
            }
            assert_eq!(*c.attempts.lock().unwrap(), vec![1]);
            let Counted { f, attempts, .. } = c;
            let Fixture { session, .. } = f;
            drop(session);
            assert_eq!(
                attempts.lock().unwrap().len(),
                1,
                "the lease's own drop makes no second exit attempt"
            );
        }

        #[test]
        fn a_copy_after_a_cancelled_close_gets_its_own_exit_attempt() {
            let c = counted();
            c.f.session.shutdown();
            c.busy.store(false, Ordering::SeqCst);
            reopen_fixture(&c.f);
            let ticket = c.f.session.snapshot().unwrap().ticket;
            c.f.session
                .copy(ticket, Zeroizing::new("fake-second-copy".into()))
                .unwrap();
            c.busy.store(true, Ordering::SeqCst);
            let before = c.attempts.lock().unwrap().len();
            c.f.session.shutdown();
            assert_eq!(c.attempts.lock().unwrap().len(), before + 1);
            let Counted { f, attempts, .. } = c;
            let Fixture { session, .. } = f;
            drop(session);
            assert_eq!(attempts.lock().unwrap().len(), before + 1);
        }

        #[test]
        fn the_window_and_exit_events_call_shutdown_and_never_lock() {
            let source = include_str!("lib.rs");
            assert_eq!(source.matches(".shutdown()").count(), 2);
            assert!(
                !source.contains(".lock()"),
                "a close or exit event that calls lock can make a second attempt"
            );
        }

        /// A backend whose first cleanup attempt waits until the test releases it.
        struct StalledCleanup {
            inner: FakeClipboard,
            started: mpsc::Sender<()>,
            release: mpsc::Receiver<()>,
            stall: bool,
        }
        impl ClipboardBackend for StalledCleanup {
            fn set_private(&mut self, text: &str) -> Publication {
                self.inner.set_private(text)
            }
            fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
                if std::mem::take(&mut self.stall) {
                    self.started.send(()).unwrap();
                    self.release.recv().unwrap();
                }
                self.inner.clear_matching(sequence, text)
            }
        }

        // 1H-01: a stalled tick cleanup holds the lease cell, never the gate.
        #[test]
        fn a_lock_delivers_its_notice_while_a_tick_cleanup_is_stalled_and_the_gate_stays_free() {
            let f = fixture();
            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            f.session.cell_for_test().clipboard = ClipboardLease::new(Box::new(StalledCleanup {
                inner: FakeClipboard(f.clipboard.clone()),
                started: started_tx,
                release: release_rx,
                stall: true,
            }));
            let ticket = f.session.snapshot().unwrap().ticket;
            f.session
                .copy(ticket, Zeroizing::new("fake-stalled".into()))
                .unwrap();
            f.clock.set(30);
            let session = f.session.clone();
            let tick = std::thread::spawn(move || session.tick());
            started_rx
                .recv_timeout(crate::test_support::PATIENCE)
                .unwrap();

            let (free_tx, free_rx) = mpsc::channel();
            let session = f.session.clone();
            std::thread::spawn(move || {
                let _ = session.warnings();
                let _ = session.snapshot();
                free_tx.send(()).unwrap();
            });
            free_rx
                .recv_timeout(crate::test_support::PATIENCE)
                .expect("a stalled cleanup must not hold the gate");

            let session = f.session.clone();
            let lock = std::thread::spawn(move || session.lock());
            let deadline = Instant::now() + crate::test_support::PATIENCE;
            while f.events.load(Ordering::SeqCst) == 0 {
                assert!(
                    Instant::now() < deadline,
                    "the notice must arrive before the stalled cleanup ends"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(
                !lock.is_finished(),
                "the lock's own attempt waits for the cleanup in progress (recorded limit)"
            );
            assert_eq!(f.clipboard.lock().unwrap().text, "fake-stalled");
            release_tx.send(()).unwrap();
            tick.join().unwrap();
            lock.join().unwrap().unwrap();
            assert_eq!(f.clipboard.lock().unwrap().text, "");
            assert_eq!(f.clipboard.lock().unwrap().clears, 1);
            assert_eq!(f.events.load(Ordering::SeqCst), 1);
        }

        // The fence of design D2: a settle that follows a newer session's copy leaves it.
        #[test]
        fn a_late_settle_of_an_older_session_cannot_clean_a_newer_sessions_copy() {
            let c = counted();
            c.f.session.lock().unwrap();
            assert!(c
                .f
                .session
                .cell_for_test()
                .clipboard
                .owned_generation()
                .is_some());
            reopen_fixture(&c.f);
            c.busy.store(false, Ordering::SeqCst);
            let ticket = c.f.session.snapshot().unwrap().ticket;
            c.f.session
                .copy(ticket, Zeroizing::new("fake-newer-copy".into()))
                .unwrap();
            let before = c.attempts.lock().unwrap().len();
            c.f.session.settle(false);
            c.f.session.tick();
            assert_eq!(c.attempts.lock().unwrap().len(), before);
            assert_eq!(c.f.clipboard.lock().unwrap().text, "fake-newer-copy");
        }

        // A copy that waits behind a stalled cleanup holds no gate while it waits.
        #[test]
        fn a_copy_waiting_behind_a_stalled_cleanup_leaves_the_gate_free() {
            let f = fixture();
            let (started_tx, started_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            f.session.cell_for_test().clipboard = ClipboardLease::new(Box::new(StalledCleanup {
                inner: FakeClipboard(f.clipboard.clone()),
                started: started_tx,
                release: release_rx,
                stall: true,
            }));
            let ticket = f.session.snapshot().unwrap().ticket;
            f.session
                .copy(ticket, Zeroizing::new("fake-first".into()))
                .unwrap();
            f.clock.set(30);
            let session = f.session.clone();
            let tick = std::thread::spawn(move || session.tick());
            started_rx
                .recv_timeout(crate::test_support::PATIENCE)
                .unwrap();
            let session = f.session.clone();
            let copy = std::thread::spawn(move || {
                session.copy(ticket, Zeroizing::new("fake-second".into()))
            });
            let (free_tx, free_rx) = mpsc::channel();
            let session = f.session.clone();
            std::thread::spawn(move || {
                let _ = session.snapshot();
                free_tx.send(()).unwrap();
            });
            free_rx
                .recv_timeout(crate::test_support::PATIENCE)
                .expect("a waiting copy must not hold the gate");
            release_tx.send(()).unwrap();
            tick.join().unwrap();
            copy.join().unwrap().unwrap();
            assert_eq!(f.clipboard.lock().unwrap().text, "fake-second");
        }
    }

    /// Unit 1a, design revision 3, section 3.1: the real publisher and cleanup over the fake
    /// Win32 API, through `copy_stamped` (the function both production copy commands call).
    #[cfg(windows)]
    mod real_publisher {
        use super::*;
        use crate::clipboard_fake::{FakeApi, FakeState, Fault, TransactionBackend};

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
            f.session.cell_for_test().clipboard =
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
            f.session.cell_for_test().clipboard.owned_generation()
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

        /// The fake Win32 backend, with `after` run right after each publication, before the
        /// copy's final check.
        struct Between {
            inner: TransactionBackend,
            after: Box<dyn FnMut() + Send>,
        }
        impl ClipboardBackend for Between {
            fn set_private(&mut self, text: &str) -> crate::clipboard::Publication {
                let published = self.inner.set_private(text);
                (self.after)();
                published
            }
            fn clear_matching(&mut self, sequence: u32, text: &str) -> Result<bool, CommandError> {
                self.inner.clear_matching(sequence, text)
            }
        }

        /// U1-R1-02: the state table of `copy_held`, row by row. Each cause comes before the
        /// first check, or between the publication and the final check. An idle lock stands
        /// for a lock; a new session fails the same ticket comparison.
        #[test]
        fn u1_r1_02_a_copy_refused_after_its_publication_is_taken_back_row_by_row() {
            #[derive(Clone, Copy, Debug, PartialEq)]
            enum When {
                Before,
                StillOurs,
                UserCopiedTheSameText,
                TakeBackFails,
            }
            let refusals = [
                ("lock", CommandError::locked()),
                ("write", vault_changed()),
                ("view", view_changed()),
            ];
            for (cause, refusal) in refusals {
                for when in [
                    When::Before,
                    When::StillOurs,
                    When::UserCopiedTheSameText,
                    When::TakeBackFails,
                ] {
                    let row = format!("{cause}, {when:?}");
                    let f = fixture();
                    let state = Arc::new(Mutex::new(FakeState::default()));
                    state.lock().unwrap().user_copy(units(USER));
                    let clock = f.clock.clone();
                    let apply = Arc::new(move |session: &Session| match cause {
                        "lock" => clock.set(crate::policy::IDLE_TIMEOUT.as_secs() + 1),
                        "write" => session
                            .access(Access::Write, None, |vault| {
                                vault.add(akm_core::ApiKeyInput::new(
                                    "Fake intervening revision",
                                    "fake-revision-value",
                                ))?;
                                Ok(())
                            })
                            .map(drop)
                            .unwrap(),
                        _ => drop(session.expire_presentation().unwrap()),
                    });
                    let (hook_apply, hook_state) = (apply.clone(), state.clone());
                    let weak = Arc::downgrade(&f.session);
                    f.session.cell_for_test().clipboard = ClipboardLease::new(Box::new(Between {
                        inner: TransactionBackend(FakeApi(state.clone())),
                        after: Box::new(move || {
                            if when == When::Before {
                                return;
                            }
                            hook_apply(&weak.upgrade().unwrap());
                            let mut s = hook_state.lock().unwrap();
                            match when {
                                When::UserCopiedTheSameText => s.user_copy(units(A)),
                                When::TakeBackFails => {
                                    let next = s.open_calls + 1;
                                    s.faults = vec![Fault::Open(next)];
                                }
                                _ => {}
                            }
                        }),
                    }));
                    let snapshot = f.session.snapshot().unwrap();
                    if when == When::Before {
                        apply(&f.session);
                    }
                    let error = f
                        .session
                        .copy_stamped(Stamped {
                            snapshot,
                            value: Zeroizing::new(A.into()),
                        })
                        .unwrap_err();
                    let s = state.lock().unwrap();
                    let (text, sets, faults) = (s.text(), s.set_calls, s.faults.len());
                    drop(s);
                    match when {
                        When::Before => {
                            assert_eq!(error.kind, refusal.kind, "{row}");
                            assert_eq!(text, USER, "{row}: untouched");
                            assert_eq!(sets, 0, "{row}: nothing published");
                        }
                        When::StillOurs => {
                            assert_eq!(error.kind, refusal.kind, "{row}");
                            assert_eq!(text, "", "{row}: AKM's copy removed");
                            assert_eq!(generation(&f), None, "{row}: the lease ended");
                        }
                        When::UserCopiedTheSameText => {
                            assert_eq!(error.kind, refusal.kind, "{row}");
                            assert_eq!(text, A, "{row}: the user's copy kept");
                            assert_eq!(generation(&f), None, "{row}: the lease ended");
                        }
                        When::TakeBackFails => {
                            assert_eq!(error.kind, "clipboardUncertain", "{row}");
                            assert_eq!(
                                error.message,
                                format!("{} {}", refusal.message, uncertain()),
                                "{row}"
                            );
                            assert_eq!(faults, 0, "{row}: the busy open was reached");
                            assert_eq!(text, A, "{row}: one attempt only, so the copy remains");
                            assert!(generation(&f).is_some(), "{row}: the lease is kept");
                            assert_eq!(application_messages(&f), vec![uncertain()], "{row}");
                        }
                    }
                    if when != When::TakeBackFails {
                        assert!(application_messages(&f).is_empty(), "{row}");
                    }
                }
            }
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

        /// 1A-A2-02: the fake clipboard's sequence change for each row, counted from the
        /// calls the row makes (an empty, a set, a rollback empty), and the same with or
        /// without an earlier copy. A row that changes it differently has changed behaviour.
        fn sequence_delta(name: &str) -> u32 {
            match name {
                "NUL in the value"
                | "register"
                | "allocate"
                | "owner"
                | "open busy"
                | "empty fails, sequence unchanged" => 0,
                "empty fails, sequence changed" => 1,
                "exclusion set" => 2,
                "exclusion check" | "empty frame set" => 5,
                "rollback fails after the first secret set" => 8,
                "first secret set" => 9,
                "later secret set"
                | "rollback fails, sequence unchanged"
                | "rollback fails, sequence changed" => 10,
                "success" => 11,
                "read-back unreadable" => 12,
                other => panic!("no sequence for {other}"),
            }
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
            let sequence_before = state.lock().unwrap().sequence;
            let result = copy(&f, case.text);
            assert!(
                state.lock().unwrap().faults.is_empty(),
                "{label}: the fault was not reached"
            );
            assert_eq!(
                state.lock().unwrap().sequence - sequence_before,
                sequence_delta(case.name),
                "{label}: fake sequence"
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
            let expected_warning = case.warns && (earlier || case.name.starts_with("rollback"));
            let warnings = application_messages(&f);
            assert_eq!(
                warnings,
                if expected_warning {
                    vec![uncertain()]
                } else {
                    vec![]
                },
                "{label}: the exact warnings"
            );
            let ids = application_ids(&f);
            assert_eq!(
                ids.len(),
                usize::from(expected_warning),
                "{label}: warning IDs"
            );
            // Later cleanup: a lease that is gone causes no further clipboard calls.
            let empties = state.lock().unwrap().empty_calls;
            f.clock.set(30);
            f.session.tick();
            f.session.lock().unwrap();
            assert_eq!(
                application_ids(&f),
                ids,
                "{label}: tick and lock add no second warning for the same episode"
            );
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

        /// 1A-A2-02: a second failure episode, while the first warning is still pending (not
        /// dismissed), is a new occurrence with its own ID. It never reuses the first.
        #[test]
        fn a_second_failure_episode_while_the_first_warning_is_pending_gets_its_own_id() {
            let warning_rows = [
                "empty fails, sequence changed",
                "rollback fails after the first secret set",
                "rollback fails, sequence changed",
            ];
            for name in warning_rows {
                let f = fixture();
                let state = install(&f);
                let attempt = |case: &Case| {
                    let mut s = state.lock().unwrap();
                    let base = Base {
                        registered: s.registered_names.len(),
                        allocations: s.allocations,
                        opens: s.open_calls,
                        empties: s.empty_calls,
                        sets: s.set_calls,
                    };
                    s.faults = (case.faults)(&base);
                    drop(s);
                    let before = state.lock().unwrap().sequence;
                    copy(&f, case.text).unwrap_err();
                    assert_eq!(
                        state.lock().unwrap().sequence - before,
                        sequence_delta(case.name),
                        "{name}: fake sequence"
                    );
                };
                let case = || cases().into_iter().find(|c| c.name == name).unwrap();
                copy(&f, A).unwrap();
                attempt(&case());
                let first = application_ids(&f);
                assert_eq!(first.len(), 1, "{name}: first episode");
                // A lease again, then the same failure again; the first warning is still there.
                copy(&f, A).unwrap();
                attempt(&case());
                let both = application_ids(&f);
                assert_eq!(both.len(), 2, "{name}: second episode");
                assert_eq!(both[0], first[0], "{name}: the first warning is untouched");
                assert_ne!(
                    both[1], first[0],
                    "{name}: the second episode has its own ID"
                );
                assert_eq!(
                    application_messages(&f),
                    vec![uncertain(), uncertain()],
                    "{name}"
                );
                f.clock.set(30);
                f.session.tick();
                f.session.lock().unwrap();
                assert_eq!(application_ids(&f), both, "{name}: no third warning");
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

    /// The commit point (`docs/design/commit-point.md`): the vault file replacement is checked
    /// under the gate, the revision moves there, and no other path can replace the file.
    mod commit_point {
        use super::*;
        use akm_core::ApiKeyInput;
        use std::{fs, path::Path};

        const PASSWORD: &str = "fake-test-password-123!";

        fn vault_path(f: &Fixture) -> PathBuf {
            f._directory.path().join("vault.kdbx")
        }

        /// The vault bytes and the name of every file beside it. A refused write changes
        /// neither, and leaves no temporary file or backup behind.
        fn on_disk(f: &Fixture) -> (Vec<u8>, Vec<String>) {
            let mut names: Vec<String> = fs::read_dir(f._directory.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            (fs::read(vault_path(f)).unwrap(), names)
        }

        /// The fake KDBX 4.0 vault of the core tests, which needs the explicit upgrade.
        const LEGACY_PASSWORD: &str = "correct horse battery staple";

        fn password_of(kind: &str) -> &'static str {
            if kind == "upgrade" {
                LEGACY_PASSWORD
            } else {
                PASSWORD
            }
        }

        /// A session over the KDBX 4.0 vault.
        fn legacy_fixture() -> Fixture {
            let f = fixture();
            f.session.lock().unwrap();
            let path = f._directory.path().join("vault.kdbx");
            fs::write(
                &path,
                include_bytes!("../../../crates/akm-core/tests/fixtures/kdbx40.kdbx"),
            )
            .unwrap();
            let ticket = f.session.begin_open().unwrap();
            f.session
                .open(ticket, || {
                    Vault::open(&path, LEGACY_PASSWORD).map_err(Into::into)
                })
                .unwrap();
            f
        }

        fn listed(f: &Fixture, kind: &str) -> Vec<String> {
            Vault::open(vault_path(f), password_of(kind))
                .unwrap()
                .list()
                .into_iter()
                .map(|key| key.id)
                .collect()
        }

        const KINDS: [&str; 9] = [
            "add", "update", "rotate", "remove", "restore", "purge", "import", "save", "upgrade",
        ];

        /// The state that the write of `kind` needs, written before the test arms its barrier.
        /// Returns the ID of the key that the write acts on.
        fn arrange(f: &Fixture, kind: &str) -> String {
            if kind == "upgrade" {
                return String::new();
            }
            f.session
                .access(Access::Write, None, |vault| {
                    let key = vault.add(ApiKeyInput::new("Fake", "fake-value"))?;
                    if matches!(kind, "restore" | "purge") {
                        vault.remove(&key.id)?;
                    }
                    Ok(key.id)
                })
                .unwrap()
                .value
        }

        fn write(vault: &mut Vault, kind: &str, id: &str) -> Result<(), CommandError> {
            match kind {
                "add" => vault.add(ApiKeyInput::new("Other", "fake-other")).map(drop),
                "update" => {
                    let mut input = vault.edit_input(id)?;
                    input.title = "Renamed".into();
                    vault.update(id, input).map(drop)
                }
                "rotate" => vault.rotate(id, "fake-rotated".into()).map(drop),
                "remove" => vault.remove(id).map(drop),
                "restore" => vault.restore(id).map(drop),
                "purge" => vault.purge(id).map(drop),
                "import" => vault
                    .import_env(
                        &[("FAKE_IMPORTED".into(), "fake-imported".into())],
                        Some("app"),
                        Some("dev"),
                    )
                    .map(drop),
                "save" => vault.save(),
                "upgrade" => vault.upgrade(),
                _ => unreachable!(),
            }
            .map_err(Into::into)
        }

        fn access_of(kind: &str) -> Access {
            if kind == "upgrade" {
                Access::Upgrade
            } else {
                Access::Write
            }
        }

        /// T-R3-04. A lock that lands after the entry check of a write, at either of two
        /// moments: as the write starts, and at the commit point itself, after the key
        /// derivation, the temporary file and the backup. The replacement is refused, the
        /// answer is `locked`, and the vault and its folder are as they were.
        #[test]
        fn t_r3_04_a_lock_after_the_entry_check_writes_nothing_for_every_kind_of_write() {
            for moment in ["as the write starts", "at the commit point"] {
                for kind in KINDS {
                    let f = if kind == "upgrade" {
                        legacy_fixture()
                    } else {
                        fixture()
                    };
                    let id = arrange(&f, kind);
                    let before = on_disk(&f);
                    let keys = listed(&f, kind);
                    let session = f.session.clone();
                    if moment == "at the commit point" {
                        f.session.at_commit(move || session.lock().unwrap());
                    }
                    let locker = f.session.clone();
                    let error = f
                        .session
                        .access(access_of(kind), None, |vault| {
                            if moment == "as the write starts" {
                                locker.lock().unwrap();
                            }
                            write(vault, kind, &id)
                        })
                        .err()
                        .unwrap_or_else(|| panic!("{kind} {moment}: the write answered success"));
                    assert_eq!(error.kind, "locked", "{kind} {moment}");
                    assert!(on_disk(&f) == before, "{kind} {moment}: the vault changed");
                    assert_eq!(listed(&f, kind), keys, "{kind} {moment}");
                    assert!(f.session.snapshot().is_err(), "{kind} {moment}");
                }
            }
        }

        /// The same writes, with a session that stays current: each one replaces the file
        /// once and moves the revision once.
        #[test]
        fn every_kind_of_write_replaces_the_file_and_moves_the_revision_once() {
            for kind in KINDS {
                let f = if kind == "upgrade" {
                    legacy_fixture()
                } else {
                    fixture()
                };
                let id = arrange(&f, kind);
                let before = f.session.snapshot().unwrap();
                let bytes = fs::read(vault_path(&f)).unwrap();
                let done = f
                    .session
                    .access(access_of(kind), None, |vault| write(vault, kind, &id))
                    .unwrap_or_else(|error| panic!("{kind}: {}", error.message));
                assert_eq!(done.snapshot.revision, before.revision + 1, "{kind}");
                assert_eq!(f.session.snapshot().unwrap().revision, before.revision + 1);
                assert_ne!(fs::read(vault_path(&f)).unwrap(), bytes, "{kind}");
            }
        }

        /// A lock during the slow part of a write (here, a barrier at the commit point, before
        /// the gate) finishes at once. The write then refuses.
        #[test]
        fn a_lock_during_the_slow_part_of_a_write_is_prompt_and_the_write_then_writes_nothing() {
            let f = fixture();
            let before = on_disk(&f);
            let (arrived, at_commit) = mpsc::channel::<()>();
            let (release, go) = mpsc::channel::<()>();
            let (arrived, go) = (Mutex::new(arrived), Mutex::new(go));
            f.session.at_commit(move || {
                arrived.lock().unwrap().send(()).unwrap();
                go.lock().unwrap().recv().unwrap();
            });
            let session = f.session.clone();
            let writer = std::thread::spawn(move || {
                session
                    .access(Access::Write, None, |vault| {
                        vault
                            .add(ApiKeyInput::new("Fake", "fake-value"))
                            .map_err(Into::into)
                    })
                    .map(|done| done.value.id)
            });
            at_commit
                .recv_timeout(crate::test_support::PATIENCE)
                .unwrap();
            let (locked, done) = mpsc::channel();
            let locker = f.session.clone();
            std::thread::spawn(move || {
                locker.lock().unwrap();
                locked.send(()).unwrap();
            });
            let prompt = done.recv_timeout(Duration::from_secs(10)).is_ok();
            release.send(()).unwrap();
            assert_eq!(writer.join().unwrap().unwrap_err().kind, "locked");
            assert!(prompt, "the lock waited for the write");
            assert!(on_disk(&f) == before, "the vault changed");
        }

        /// A new session that opens while a write of the old one waits at its commit point:
        /// the old write refuses and the new session's vault is untouched.
        #[test]
        fn a_new_session_while_a_write_waits_at_its_commit_point_refuses_the_write() {
            let f = fixture();
            let before = on_disk(&f);
            let (arrived, at_commit) = mpsc::channel::<()>();
            let (release, go) = mpsc::channel::<()>();
            let (arrived, go) = (Mutex::new(arrived), Mutex::new(go));
            f.session.at_commit(move || {
                arrived.lock().unwrap().send(()).unwrap();
                go.lock().unwrap().recv().unwrap();
            });
            let session = f.session.clone();
            let writer = std::thread::spawn(move || {
                session
                    .access(Access::Write, None, |vault| {
                        vault
                            .add(ApiKeyInput::new("Fake", "fake-value"))
                            .map_err(Into::into)
                    })
                    .map(drop)
            });
            at_commit
                .recv_timeout(crate::test_support::PATIENCE)
                .unwrap();
            let ticket = f.session.begin_open().unwrap();
            release.send(()).unwrap();
            assert_eq!(writer.join().unwrap().unwrap_err().kind, "locked");
            let path = vault_path(&f);
            f.session
                .open(ticket, || Vault::open(&path, PASSWORD).map_err(Into::into))
                .unwrap();
            assert_eq!(f.session.snapshot().unwrap().revision, 0);
            assert!(on_disk(&f) == before, "the vault changed");
        }

        /// The revision moves when the file is replaced, not when the access ends. An act
        /// bound to the old revision refuses while the write is still inside its access.
        #[test]
        fn a_write_moves_the_revision_at_its_commit_so_an_act_bound_to_the_old_revision_refuses() {
            let f = fixture();
            let old = f.session.snapshot().unwrap();
            let session = f.session.clone();
            f.session
                .access(Access::Write, None, |vault| {
                    vault.add(ApiKeyInput::new("Fake", "fake-value"))?;
                    let refused = session
                        .commit(Binding::Revision(old), &mut Act::new(|_: &Held<'_>| Ok(())));
                    assert_eq!(refused.unwrap_err().kind, "stalePreview");
                    Ok(())
                })
                .unwrap();
            assert_eq!(
                f.session.snapshot().unwrap().revision,
                old.revision + 1,
                "one write moves the revision once"
            );
        }

        /// The guard against a bypass at run time. A read, a reload and the stored vault
        /// between accesses hold a closed gate: a save through any of them is refused, and the
        /// file stays as it was.
        #[test]
        fn a_replacement_outside_a_write_access_is_refused_and_writes_nothing() {
            let f = fixture();
            let before = on_disk(&f);
            for access in [Access::Read, Access::Reload] {
                let error = f
                    .session
                    .access(access, None, |vault| vault.save().map_err(Into::into))
                    .err()
                    .expect("a save in a read answered success");
                assert_eq!(error.kind, "internal");
            }
            {
                let mut stored = f.session.vault.lock().unwrap();
                assert!(stored.vault.as_mut().unwrap().save().is_err());
            }
            assert!(on_disk(&f) == before, "the vault changed");
            assert!(f.session.snapshot().is_ok(), "the session stays open");
        }

        /// A revoke runs after the gate is released, and only when the act did not happen.
        #[test]
        fn a_refused_commit_revokes_after_the_gate_is_released_and_a_done_one_never_revokes() {
            struct Probe<'a> {
                session: &'a Session,
                acted: bool,
                revoked_with_free_gate: Option<bool>,
            }
            impl Effect for Probe<'_> {
                type Output = ();
                fn act(&mut self, _held: &Held<'_>) -> Result<(), CommandError> {
                    self.acted = true;
                    Ok(())
                }
                fn revoke(&mut self) -> Result<(), String> {
                    self.revoked_with_free_gate = Some(self.session.gate.try_lock().is_ok());
                    Err("fake revoke failure".into())
                }
            }
            let f = fixture();
            let ticket = f.session.snapshot().unwrap().ticket;
            let mut done = Probe {
                session: &f.session,
                acted: false,
                revoked_with_free_gate: None,
            };
            f.session
                .commit(Binding::Ticket(ticket), &mut done)
                .unwrap();
            assert!(done.acted);
            assert_eq!(done.revoked_with_free_gate, None);
            f.session.lock().unwrap();
            let mut refused = Probe {
                session: &f.session,
                acted: false,
                revoked_with_free_gate: None,
            };
            let error = f
                .session
                .commit(Binding::Ticket(ticket), &mut refused)
                .unwrap_err();
            assert_eq!(error.kind, "locked");
            assert!(!refused.acted);
            assert_eq!(refused.revoked_with_free_gate, Some(true));
            let messages: Vec<String> = f
                .session
                .warnings()
                .unwrap()
                .application
                .into_iter()
                .map(|notice| notice.message)
                .collect();
            assert_eq!(messages, ["fake revoke failure"]);
        }

        /// U1-R1-04: an act that panics under the gate. The panic does not poison the gate, the
        /// revoke runs, the session ends, the reply does not invite a retry, and the next opening
        /// works. A gate that is poisoned some other way still takes a record.
        #[test]
        fn u1_r1_04_a_panic_under_the_gate_leaves_a_working_gate_and_no_record_is_skipped() {
            struct Panics {
                revoked: bool,
            }
            impl Effect for Panics {
                type Output = ();
                fn act(&mut self, _held: &Held<'_>) -> Result<(), CommandError> {
                    panic!("a fake fault under the gate")
                }
                fn revoke(&mut self) -> Result<(), String> {
                    self.revoked = true;
                    Ok(())
                }
            }
            let f = fixture();
            let ticket = f.session.snapshot().unwrap().ticket;
            let mut effect = Panics { revoked: false };
            let error = f
                .session
                .commit(Binding::Ticket(ticket), &mut effect)
                .unwrap_err();
            assert_eq!(error.message, CommandError::failed_step().message);
            assert!(effect.revoked, "the revoke ran");
            assert!(!f.session.gate.is_poisoned(), "the gate is not poisoned");
            assert_eq!(f.session.snapshot().unwrap_err().kind, "locked");
            assert!(f.session.warnings().is_ok());
            let next = f.session.begin_open().unwrap();
            f.session
                .open(next, || {
                    let path = f._directory.path().join("next.kdbx");
                    Vault::create(path, "fake-test-password-123!", None).map_err(Into::into)
                })
                .unwrap();
            assert!(f.session.snapshot().is_ok(), "the next opening works");

            let session = f.session.clone();
            let _ = std::thread::spawn(move || {
                let _gate = session.gate.lock().unwrap();
                panic!("a fake fault that poisons the gate");
            })
            .join();
            assert!(f.session.gate.is_poisoned());
            let id = uuid::Uuid::new_v4().to_string();
            f.session
                .record_operation(ticket, &id, OperationWarningKind::ImportFailed);
            assert!(
                f.session
                    .raw_gate()
                    .notices
                    .iter()
                    .any(|notice| notice.id == id),
                "the record was taken"
            );
        }

        /// U1-R1-04: a write access that fails or panics. Before the replacement, a panic answers
        /// the fixed text, ends the session and leaves the file. After the replacement, an error
        /// or a panic answers `completedWhileLocked` and ends the session; the file holds the
        /// write. The vault mutex is never poisoned.
        #[test]
        fn u1_r1_04_an_access_that_fails_after_its_replacement_answers_completed_while_locked() {
            for (way, replaces, answer) in [
                ("a panic before the replacement", false, "internal"),
                (
                    "an error after the replacement",
                    true,
                    "completedWhileLocked",
                ),
                (
                    "a panic after the replacement",
                    true,
                    "completedWhileLocked",
                ),
            ] {
                let f = fixture();
                let before = on_disk(&f);
                let error = f
                    .session
                    .access(Access::Write, None, |vault| -> Result<(), CommandError> {
                        if replaces {
                            vault.add(akm_core::ApiKeyInput::new(
                                "Fake write before the fault",
                                "fake-fault-value",
                            ))?;
                        }
                        if way.starts_with("a panic") {
                            panic!("a fake fault in the access");
                        }
                        Err(CommandError::internal("a fake error after the replacement"))
                    })
                    .err()
                    .expect("the access failed");
                assert_eq!(error.kind, answer, "{way}");
                assert!(!error.message.contains("Try again"), "{way}");
                assert_eq!(on_disk(&f) != before, replaces, "{way}: the file");
                assert_eq!(f.session.snapshot().unwrap_err().kind, "locked", "{way}");
                assert!(!f.session.vault.is_poisoned(), "{way}");
                assert!(!f.session.gate.is_poisoned(), "{way}");
            }
        }

        fn sources() -> Vec<(String, String)> {
            let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
            let mut files: Vec<(String, String)> = fs::read_dir(folder)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
                .map(|path| {
                    (
                        path.file_name().unwrap().to_string_lossy().into_owned(),
                        fs::read_to_string(&path).unwrap(),
                    )
                })
                .collect();
            files.sort();
            files
        }

        /// The names of the functions whose parameters mention `needle`.
        fn functions_taking(source: &str, needle: &str) -> Vec<String> {
            let mut found = Vec::new();
            let mut rest = source;
            while let Some(at) = rest.find("fn ") {
                rest = &rest[at + 3..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                let Some(open) = rest.find('(') else { break };
                let mut depth = 0;
                let mut end = open;
                for (index, c) in rest[open..].char_indices() {
                    match c {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                end = open + index;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                if !name.is_empty() && rest[open..end].contains(needle) {
                    found.push(name);
                }
            }
            found
        }

        /// The structural guard: a tripwire for the usual spelling, not a proof. It reads the
        /// source text, so a new caller that checks and then acts outside `Session::commit`, a
        /// second maker of the witness, or a second way to replace the vault file fails it only
        /// when the code uses the names below. A renamed import (`use std::process::Command as
        /// Child`), a re-export or a macro passes it (U1-R1-06). The type `Held` is the part that
        /// the compiler enforces.
        #[test]
        fn every_session_scoped_act_goes_through_the_commit_point() {
            let sources = sources();
            assert!(sources.len() >= 15, "the scan finds the sources");
            let witness = concat!("Held::", "new()");
            let makers: Vec<&str> = sources
                .iter()
                .flat_map(|(name, text)| text.matches(witness).map(move |_| name.as_str()))
                .collect();
            assert_eq!(
                makers,
                ["session.rs"],
                "only Session::commit_point makes a Held"
            );
            let point = &sources
                .iter()
                .find(|(name, _)| name == "session.rs")
                .unwrap()
                .1;
            let body = &point[point.find("fn commit_point").unwrap()..];
            assert!(
                body[..body.find("\n    }\n").unwrap()].contains(witness),
                "the one Held is made in commit_point"
            );
            for (name, text) in &sources {
                for retired in [concat!("while_", "current"), concat!("while_", "session")] {
                    assert!(!text.contains(retired), "{name} uses {retired}");
                }
                for process in [
                    concat!("Command", "::new"),
                    concat!("CreateProcess", "W("),
                    concat!("Resume", "Thread("),
                    concat!("Terminate", "Process("),
                ] {
                    assert!(
                        name == "process.rs" || !text.contains(process),
                        "{name} starts or ends a process ({process}) outside process.rs"
                    );
                }
                assert!(
                    name == "session.rs" || !text.contains(concat!("set_commit", "_gate")),
                    "{name} installs a vault commit gate"
                );
                for function in functions_taking(text, "Held<") {
                    assert!(
                        ["act", "resume", "show"].contains(&function.as_str()),
                        "{name}: fn {function} takes the witness but is not an act"
                    );
                }
            }
            let core = include_str!("../../../crates/akm-core/src/vault.rs");
            let commit = &core[core.find("fn commit(&mut self").unwrap()..];
            let commit = &commit[..commit.find("\n    }\n").unwrap()];
            let replace = commit.find("let mut replace = ||").unwrap();
            let gated = commit.find("match &self.commit_gate").unwrap();
            for act in [
                "fs::hard_link(&tmp.path, &self.path)",
                "fs::rename(&tmp.path, &self.path)",
            ] {
                assert_eq!(core.matches(act).count(), 1, "{act}");
                let at = commit.find(act).unwrap();
                assert!(
                    replace < at && at < gated,
                    "{act} runs only inside the gate"
                );
            }
            assert!(commit.contains("Some(gate) => gate.commit(&mut replace)?"));
        }
    }
}
