#![allow(dead_code)]
#[path = "probe-source/clipboard.rs"] mod clipboard;
#[path = "probe-source/error.rs"] mod error;
#[path = "probe-source/pending.rs"] mod pending;
#[path = "probe-source/policy.rs"] mod policy;
mod session {
    include!("probe-source/session.rs");

    use crate::clipboard::CopyAttempt;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    struct FixedClock;
    impl Clock for FixedClock {
        fn now(&self) -> Duration { Duration::ZERO }
    }

    struct PartialCopy {
        clears: Arc<AtomicUsize>,
        value: Arc<Mutex<String>>,
    }
    impl ClipboardBackend for PartialCopy {
        fn set_private(&mut self, text: &str) -> Result<CopyAttempt, CommandError> {
            *self.value.lock().unwrap() = text.to_string();
            Ok(CopyAttempt {
                sequence: 17,
                failure: Some(CommandError::new("clipboard", "fake partial publication failure")),
            })
        }
        fn clear_matching(&mut self, _sequence: u32, _text: &str) -> Result<bool, CommandError> {
            if self.clears.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(CommandError::new("clipboard", "fake immediate cleanup failure"));
            }
            self.value.lock().unwrap().clear();
            Ok(true)
        }
    }

    struct AdjustableClock(Arc<AtomicU64>);
    impl Clock for AdjustableClock {
        fn now(&self) -> Duration { Duration::from_secs(self.0.load(Ordering::SeqCst)) }
    }

    struct RepeatedFailure {
        copies: usize,
        clears: Arc<AtomicUsize>,
        value: Arc<Mutex<String>>,
    }
    impl ClipboardBackend for RepeatedFailure {
        fn set_private(&mut self, text: &str) -> Result<CopyAttempt, CommandError> {
            self.copies += 1;
            *self.value.lock().unwrap() = text.to_string();
            Ok(CopyAttempt {
                sequence: self.copies as u32,
                failure: (self.copies == 2).then(|| CommandError::new("clipboard", "fake partial publication failure")),
            })
        }
        fn clear_matching(&mut self, _sequence: u32, _text: &str) -> Result<bool, CommandError> {
            let attempt = self.clears.fetch_add(1, Ordering::SeqCst) + 1;
            if attempt == 1 || attempt == 3 {
                return Err(CommandError::new("clipboard", "fake repeated cleanup failure"));
            }
            self.value.lock().unwrap().clear();
            Ok(true)
        }
    }

    pub fn run_probe() {
        let clears = Arc::new(AtomicUsize::new(0));
        let value = Arc::new(Mutex::new(String::new()));
        let session = Session::new(
            Arc::new(FixedClock),
            Box::new(PartialCopy { clears: clears.clone(), value: value.clone() }),
            Arc::new(|| {}),
        );
        session.observe(Signal::Available);
        let ticket = session.begin_open().unwrap();
        assert!(session.gate().unwrap().policy.opened(ticket, Duration::ZERO));
        let error = session.copy(ticket, Zeroizing::new("fake-only-value".into())).unwrap_err();
        let first_count = session.warnings().unwrap().application.len();
        let retained_after_error = !value.lock().unwrap().is_empty();
        println!("copy_error={}", error.message);
        println!("immediate_cleanup_attempts={}", clears.load(Ordering::SeqCst));
        println!("fake_clipboard_remains_after_failed_cleanup={retained_after_error}");
        println!("warning_count_after_failed_cleanup={first_count}");
        session.tick();
        let after_count = session.warnings().unwrap().application.len();
        println!("cleanup_attempts_after_tick={}", clears.load(Ordering::SeqCst));
        println!("fake_clipboard_empty_after_recovery={}", value.lock().unwrap().is_empty());
        println!("warning_count_after_recovery={after_count}");
        assert_eq!(error.message, "fake partial publication failure");
        assert!(retained_after_error);
        assert_eq!(first_count, 0);
        assert_eq!(clears.load(Ordering::SeqCst), 2);
        assert!(value.lock().unwrap().is_empty());
        assert_eq!(after_count, 0);

        let time = Arc::new(AtomicU64::new(0));
        let clears = Arc::new(AtomicUsize::new(0));
        let value = Arc::new(Mutex::new(String::new()));
        let session = Session::new(
            Arc::new(AdjustableClock(time.clone())),
            Box::new(RepeatedFailure { copies: 0, clears: clears.clone(), value: value.clone() }),
            Arc::new(|| {}),
        );
        session.observe(Signal::Available);
        let ticket = session.begin_open().unwrap();
        assert!(session.gate().unwrap().policy.opened(ticket, Duration::ZERO));
        session.copy(ticket, Zeroizing::new("fake-first".into())).unwrap();
        time.store(30, Ordering::SeqCst);
        session.tick();
        let first = session.warnings().unwrap().application;
        assert_eq!(first.len(), 1);
        let first_id = first[0].id.clone();
        session.tick();
        assert!(value.lock().unwrap().is_empty());
        assert!(session.copy(ticket, Zeroizing::new("fake-second".into())).is_err());
        let after_second_failure = session.warnings().unwrap().application;
        println!("prior_episode_warning_count={}", first.len());
        println!("new_partial_failure_warning_count={}", after_second_failure.len());
        println!("new_partial_value_remains_after_immediate_failure={}", !value.lock().unwrap().is_empty());
        assert_eq!(after_second_failure.len(), 1);
        assert_eq!(after_second_failure[0].id, first_id);
        session.tick();
        assert!(value.lock().unwrap().is_empty());
        assert_eq!(clears.load(Ordering::SeqCst), 4);
        session.dismiss_warnings(&[first_id]).unwrap();
        let remaining = session.warnings().unwrap().application.len();
        println!("warnings_after_acknowledging_only_the_first_episode={remaining}");
        assert_eq!(remaining, 0);
    }
}

fn main() { session::run_probe(); }
