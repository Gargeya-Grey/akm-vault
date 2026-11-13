//! The outcome record of a dispatched request (fault class N).
//!
//! The page can lose a reply: a page load, a crash of the webview, or a lock that the page
//! handles first. A request whose effect outlives the page (an import writes the vault, a launch
//! starts a terminal) leaves a record. `vault_warnings` returns it until the page acknowledges it
//! with `vault_dismiss_warnings`. The record holds the operation ID that the page made, the
//! session of the request and one fixed kind: no name, no value, no path.
//!
//! One guard is made when the request passes its session check. It records once, whatever way
//! the request ends: a reply, an early return through `?`, or a panic, which `dispatched`
//! catches. The default is "failed". After the effect, no reply invites a retry: the session
//! ends, and the reply is `completedWhileLocked` (U1-R1-04).

use std::cell::Cell;

use crate::error::CommandError;
use crate::policy::Ticket;
use crate::session::{OperationWarningKind, Session, Snapshot};

/// A request that leaves an outcome record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Import,
    Terminal,
}

/// What is known about a dispatched request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fact {
    /// Nothing is known to have happened. The default, and the answer to every error.
    Failed,
    /// The user cancelled in a native window. Nothing happened.
    Declined,
    /// The effect happened.
    Done,
}

impl Operation {
    fn kind(self, fact: Fact, session_ended: bool) -> OperationWarningKind {
        use OperationWarningKind as Kind;
        match (self, fact) {
            (Self::Import, Fact::Done) if session_ended => Kind::ImportSessionEnded,
            (Self::Import, Fact::Done) => Kind::ImportApplied,
            (Self::Import, Fact::Declined) => Kind::ImportDeclined,
            (Self::Import, Fact::Failed) => Kind::ImportFailed,
            (Self::Terminal, Fact::Done) if session_ended => Kind::TerminalSessionEnded,
            (Self::Terminal, Fact::Done) => Kind::TerminalStarted,
            (Self::Terminal, Fact::Declined) => Kind::TerminalDeclined,
            (Self::Terminal, Fact::Failed) => Kind::TerminalFailed,
        }
    }
}

/// The guard of one dispatched request. Its drop writes the record.
pub struct Outcome<'s> {
    session: &'s Session,
    operation: Operation,
    id: String,
    origin: Ticket,
    fact: Cell<Fact>,
}

impl Outcome<'_> {
    /// The effect happened. The launch calls it right after the commit that resumed the
    /// terminal, so a panic after that point still records the start.
    pub fn done(&self) {
        self.fact.set(Fact::Done);
    }

    fn settle<T>(&self, reply: &Result<Option<T>, CommandError>) {
        let fact = match reply {
            Ok(Some(_)) => Fact::Done,
            Ok(None) => Fact::Declined,
            Err(error) if error.kind == "completedWhileLocked" => Fact::Done,
            Err(_) => Fact::Failed,
        };
        // An effect that happened stays on record, whatever the reply says after it.
        if self.fact.get() != Fact::Done {
            self.fact.set(fact);
        }
    }
}

impl Drop for Outcome<'_> {
    fn drop(&mut self) {
        let fact = self.fact.get();
        // The session of the request ended before the record: a lock, or another opening. The
        // check reads the gate only. It changes nothing in the session that is current now.
        let session_ended = fact == Fact::Done && self.session.ensure(self.origin).is_err();
        self.session.record_operation(
            self.origin,
            &self.id,
            self.operation.kind(fact, session_ended),
        );
    }
}

/// The one accepted spelling of an operation ID: the canonical UUID, lower case with hyphens,
/// as `crypto.randomUUID()` makes it. Every other spelling (upper case, braces, no hyphens, a
/// URN) is refused, so the ID of a record is always the exact text that the page acknowledges
/// (U1-R1-07). Dispatch and acknowledgement both use this function.
pub fn parse_operation_id(text: &str) -> Result<&str, CommandError> {
    match uuid::Uuid::parse_str(text) {
        Ok(parsed) if parsed.hyphenated().to_string() == text => Ok(text),
        _ => Err(CommandError::new("invalid", "The request is malformed.")),
    }
}

/// Runs a request that leaves an outcome record. The operation ID must be a canonical UUID
/// (`parse_operation_id`): any other text is refused before anything runs, with no record. A
/// request whose session is not the current one is refused with no record, because nothing ran
/// and the reply says so. From the session check on, the request records exactly once.
pub fn dispatched<T>(
    session: &Session,
    operation: Operation,
    operation_id: &str,
    session_id: &str,
    run: impl FnOnce(&Outcome<'_>, Snapshot) -> Result<Option<T>, CommandError>,
) -> Result<Option<T>, CommandError> {
    let id = parse_operation_id(operation_id)?.to_string();
    let expected = session.snapshot_for(session_id)?;
    let outcome = Outcome {
        session,
        operation,
        id,
        origin: expected.ticket,
        fact: Cell::new(Fact::Failed),
    };
    let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&outcome, expected)))
        .unwrap_or_else(|_| Err(CommandError::failed_step()));
    outcome.settle(&reply);
    match reply {
        Err(error) if outcome.fact.get() == Fact::Done && error.kind != "completedWhileLocked" => {
            let _ = session.lock();
            Err(CommandError::completed_while_locked())
        }
        reply => reply,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        clipboard::tests::{FakeClipboard, FakeData},
        policy::Signal,
        session::MonotonicClock,
    };
    use std::sync::{Arc, Mutex};

    const PASSWORD: &str = "fake-outcome-password-123!";

    struct Fixture {
        session: Arc<Session>,
        owner: String,
        directory: tempfile::TempDir,
    }

    fn opened() -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let session = Session::new(
            Arc::new(MonotonicClock::default()),
            Box::new(FakeClipboard(Arc::new(Mutex::new(FakeData::default())))),
            Arc::new(|_| {}),
        );
        session.observe(Signal::Available);
        let owner = open(&session, &directory, "a.kdbx");
        Fixture {
            session,
            owner,
            directory,
        }
    }

    fn open(session: &Session, directory: &tempfile::TempDir, name: &str) -> String {
        let owner = session.prepare_open().unwrap();
        let ticket = session.claim_open(&owner).unwrap();
        session
            .open(ticket, || {
                akm_core::Vault::create(directory.path().join(name), PASSWORD, None)
                    .map_err(Into::into)
            })
            .unwrap();
        owner
    }

    /// The records under one operation ID: (origin, kind).
    fn records(session: &Session, id: &str) -> Vec<(String, String)> {
        let report = serde_json::to_value(session.warnings().unwrap()).unwrap();
        report["operations"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|record| record["id"] == id)
            .map(|record| {
                (
                    record["originSession"].as_str().unwrap().to_string(),
                    record["kind"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    fn failing_step() -> Result<(), CommandError> {
        Err(CommandError::new("invalid", "A fake refused step."))
    }

    type Run = fn(&Session, &Outcome<'_>) -> Result<Option<u8>, CommandError>;

    /// Each way out: the reply ("1", "null" or the error kind), and the record of an import and
    /// of a launch. A panic never unwinds out of `dispatched`. After the effect, no reply invites
    /// a retry: the session ends and the reply is `completedWhileLocked` (U1-R1-04).
    #[test]
    fn class_n_every_way_out_of_a_dispatched_request_leaves_one_true_record() {
        let ways: [(&str, Run, &str, &str, &str); 9] = [
            (
                "a value",
                |_, _| Ok(Some(1)),
                "1",
                "importApplied",
                "terminalStarted",
            ),
            (
                "null",
                |_, _| Ok(None),
                "null",
                "importDeclined",
                "terminalDeclined",
            ),
            (
                "an error",
                |_, _| Err(CommandError::locked()),
                "locked",
                "importFailed",
                "terminalFailed",
            ),
            (
                "an early return through ?",
                |_, _| {
                    failing_step()?;
                    Ok(Some(1))
                },
                "invalid",
                "importFailed",
                "terminalFailed",
            ),
            (
                "completedWhileLocked",
                |session, _| {
                    session.lock().unwrap();
                    Err(CommandError::completed_while_locked())
                },
                "completedWhileLocked",
                "importSessionEnded",
                "terminalSessionEnded",
            ),
            (
                "a panic before the effect",
                |_, _| panic!("a fake fault before the effect"),
                "internal",
                "importFailed",
                "terminalFailed",
            ),
            (
                "a panic after the effect",
                |_, outcome| {
                    outcome.done();
                    panic!("a fake fault after the effect")
                },
                "completedWhileLocked",
                "importSessionEnded",
                "terminalSessionEnded",
            ),
            (
                "an error after the effect",
                |_, outcome| {
                    outcome.done();
                    Err(CommandError::internal("a fake error after the effect"))
                },
                "completedWhileLocked",
                "importSessionEnded",
                "terminalSessionEnded",
            ),
            (
                "a lock after the effect, then a value",
                |session, outcome| {
                    outcome.done();
                    session.lock().unwrap();
                    Ok(Some(1))
                },
                "1",
                "importSessionEnded",
                "terminalSessionEnded",
            ),
        ];
        for (way, run, answer, import, terminal) in ways {
            for (operation, expected) in
                [(Operation::Import, import), (Operation::Terminal, terminal)]
            {
                let f = opened();
                let ticket = f.session.snapshot().unwrap().ticket;
                let id = uuid::Uuid::new_v4().to_string();
                let session = f.session.clone();
                let reply = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    dispatched(&f.session, operation, &id, &f.owner, |outcome, expected| {
                        assert_eq!(expected.ticket, ticket);
                        run(&session, outcome)
                    })
                }));
                let reply = match reply.expect("no panic unwinds out of dispatched") {
                    Ok(value) => serde_json::to_value(value).unwrap().to_string(),
                    Err(error) => {
                        assert!(!error.message.contains("Try again"), "{way}");
                        error.kind
                    }
                };
                assert_eq!(reply, answer, "{way}, {operation:?}");
                assert_eq!(
                    records(&f.session, &id),
                    [(ticket.0.to_string(), expected.to_string())],
                    "{way}, {operation:?}"
                );
            }
        }
    }

    #[test]
    fn class_n_a_malformed_operation_or_another_session_runs_nothing_and_leaves_no_record() {
        let f = opened();
        let ran = std::cell::Cell::new(false);
        let run = |_: &Outcome<'_>, _: Snapshot| -> Result<Option<u8>, CommandError> {
            ran.set(true);
            Ok(Some(1))
        };
        let malformed = dispatched(&f.session, Operation::Terminal, "not-a-uuid", &f.owner, run);
        assert_eq!(malformed.unwrap_err().kind, "invalid");
        let id = uuid::Uuid::new_v4().to_string();
        for session_id in [uuid::Uuid::new_v4().to_string(), "not-a-uuid".to_string()] {
            let stale = dispatched(&f.session, Operation::Import, &id, &session_id, run);
            assert_eq!(stale.unwrap_err().kind, "locked");
        }
        assert!(!ran.get(), "nothing ran");
        let report = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_eq!(report["operations"], serde_json::json!([]));
    }

    #[test]
    fn class_n_a_record_of_session_a_changes_nothing_in_session_b() {
        let f = opened();
        let a = f.session.snapshot().unwrap().ticket;
        let id = uuid::Uuid::new_v4().to_string();
        let mut b = None;
        let reply = dispatched(
            &f.session,
            Operation::Terminal,
            &id,
            &f.owner,
            |outcome, _| {
                // Session B opens while A's request runs, and A's effect happens after it.
                let owner = open(&f.session, &f.directory, "b.kdbx");
                b = Some((owner, f.session.snapshot().unwrap()));
                outcome.done();
                Ok(Some(1))
            },
        );
        assert_eq!(reply.unwrap(), Some(1));
        let (b_owner, b_before) = b.unwrap();
        assert_ne!(b_before.ticket, a);
        assert_eq!(
            records(&f.session, &id),
            [(a.0.to_string(), "terminalSessionEnded".to_string())],
            "the record names A, and says that A's session ended"
        );
        let b_after = f.session.snapshot_for(&b_owner).unwrap();
        assert_eq!(
            (b_after.ticket, b_after.revision, b_after.presentation),
            (b_before.ticket, b_before.revision, b_before.presentation),
            "B is still current, with the same revision and presentation"
        );
        let report = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_eq!(report["application"], serde_json::json!([]));
        assert_eq!(report["operations"].as_array().unwrap().len(), 1);

        let b_id = uuid::Uuid::new_v4().to_string();
        dispatched(&f.session, Operation::Terminal, &b_id, &b_owner, |_, _| {
            Ok(Some(1u8))
        })
        .unwrap();
        f.session
            .dismiss_warnings(std::slice::from_ref(&id))
            .unwrap();
        assert!(records(&f.session, &id).is_empty());
        assert_eq!(
            records(&f.session, &b_id),
            [(b_before.ticket.0.to_string(), "terminalStarted".to_string())],
            "dismissing A's record leaves B's"
        );
    }

    /// U1-R1-07: one spelling of an operation ID. Every other spelling of a valid UUID is
    /// refused at dispatch, with nothing run and no record, and at acknowledgement, with nothing
    /// dismissed. The canonical spelling round-trips exactly.
    #[test]
    fn u1_r1_07_an_operation_id_has_one_spelling_at_dispatch_and_at_acknowledgement() {
        let f = opened();
        let canonical = uuid::Uuid::new_v4();
        let text = canonical.to_string();
        let others = [
            ("upper case", text.to_uppercase()),
            ("braces", format!("{{{text}}}")),
            ("no hyphens", canonical.simple().to_string()),
            ("URN", canonical.urn().to_string()),
        ];
        for (spelling, other) in &others {
            assert!(
                uuid::Uuid::parse_str(other).is_ok(),
                "{spelling} is a valid UUID"
            );
            let ran = std::cell::Cell::new(false);
            let reply = dispatched(&f.session, Operation::Import, other, &f.owner, |_, _| {
                ran.set(true);
                Ok(Some(1u8))
            });
            assert_eq!(reply.unwrap_err().kind, "invalid", "{spelling}: refused");
            assert!(!ran.get(), "{spelling}: nothing ran");
        }
        let report = serde_json::to_value(f.session.warnings().unwrap()).unwrap();
        assert_eq!(report["operations"], serde_json::json!([]), "no record");

        let reply = dispatched(&f.session, Operation::Import, &text, &f.owner, |_, _| {
            Ok(Some(1u8))
        });
        assert_eq!(reply.unwrap(), Some(1));
        let ticket = f.session.snapshot().unwrap().ticket;
        let record = [(ticket.0.to_string(), "importApplied".to_string())];
        assert_eq!(
            records(&f.session, &text),
            record,
            "the record ID is the supplied text"
        );
        for (spelling, other) in &others {
            assert_eq!(
                f.session
                    .dismiss_warnings(&[other.clone(), text.clone()])
                    .unwrap_err()
                    .kind,
                "invalid",
                "{spelling}: an acknowledgement in another spelling is refused"
            );
            assert_eq!(
                records(&f.session, &text),
                record,
                "{spelling}: nothing dismissed"
            );
        }
        f.session
            .dismiss_warnings(std::slice::from_ref(&text))
            .unwrap();
        assert!(
            records(&f.session, &text).is_empty(),
            "the canonical ID round-trips"
        );
    }
}
