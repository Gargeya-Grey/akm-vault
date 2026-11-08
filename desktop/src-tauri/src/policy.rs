use std::time::Duration;

pub const IDLE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    Available,
    Locked,
    Suspend,
    Unobservable,
}

#[derive(Clone, Copy, Default)]
enum Phase {
    #[default]
    Locked,
    Opening(Duration),
    Active(Duration),
}

#[derive(Default)]
pub struct Policy {
    available: bool,
    generation: u64,
    phase: Phase,
}

impl Policy {
    pub fn observe(&mut self, signal: Signal) {
        self.available = signal == Signal::Available;
        if !self.available {
            self.lock();
        }
    }

    pub fn begin_open(&mut self, now: Duration) -> Option<Ticket> {
        self.lock();
        if !self.available {
            return None;
        }
        self.phase = Phase::Opening(now);
        Some(Ticket(self.generation))
    }

    pub fn opening(&mut self, ticket: Ticket, now: Duration) -> bool {
        self.expire(now);
        self.available && ticket.0 == self.generation && matches!(self.phase, Phase::Opening(_))
    }

    pub fn opened(&mut self, ticket: Ticket, now: Duration) -> bool {
        if !self.opening(ticket, now) {
            return false;
        }
        self.phase = Phase::Active(now);
        true
    }

    pub fn active(&mut self, now: Duration) -> Option<Ticket> {
        self.expire(now);
        matches!(self.phase, Phase::Active(_)).then_some(Ticket(self.generation))
    }

    pub fn activity(&mut self, now: Duration) -> bool {
        if self.active(now).is_none() {
            return false;
        }
        self.phase = Phase::Active(now);
        true
    }

    pub fn lock(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.phase = Phase::Locked;
    }

    pub fn is_locked(&self) -> bool {
        matches!(self.phase, Phase::Locked)
    }

    /// The ticket of the latest lock or open. A lease made under any other value is stale.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn expire(&mut self, now: Duration) {
        if let Phase::Active(last) | Phase::Opening(last) = self.phase {
            if now.saturating_sub(last) >= IDLE_TIMEOUT {
                self.lock();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unlocked() -> Policy {
        let mut policy = Policy::default();
        policy.observe(Signal::Available);
        let ticket = policy.begin_open(Duration::ZERO).unwrap();
        assert!(policy.opened(ticket, Duration::ZERO));
        policy
    }

    #[test]
    fn status_checks_do_not_extend_the_ten_minute_deadline() {
        let mut policy = unlocked();
        assert!(policy.active(Duration::from_secs(599)).is_some());
        assert_eq!(policy.active(Duration::from_secs(600)), None);
    }

    #[test]
    fn only_explicit_activity_in_an_active_session_renews_idle() {
        let mut policy = unlocked();
        assert!(policy.activity(Duration::from_secs(590)));
        assert!(policy.active(Duration::from_secs(1189)).is_some());
        assert!(!policy.activity(Duration::from_secs(1190)));
        assert_eq!(policy.active(Duration::from_secs(1191)), None);
    }

    #[test]
    fn delayed_unlock_cannot_reopen_after_any_lock_signal() {
        for signal in [Signal::Locked, Signal::Suspend, Signal::Unobservable] {
            let mut policy = Policy::default();
            policy.observe(Signal::Available);
            let ticket = policy.begin_open(Duration::ZERO).unwrap();
            policy.observe(signal);
            policy.observe(Signal::Available);
            assert!(!policy.opened(ticket, Duration::from_secs(1)));
        }
    }

    #[test]
    fn an_explicit_lock_and_a_newer_open_invalidate_older_results() {
        let mut policy = Policy::default();
        policy.observe(Signal::Available);
        let old = policy.begin_open(Duration::ZERO).unwrap();
        policy.lock();
        assert!(!policy.opened(old, Duration::ZERO));
        let first = policy.begin_open(Duration::ZERO).unwrap();
        let second = policy.begin_open(Duration::ZERO).unwrap();
        assert!(!policy.opened(first, Duration::ZERO));
        assert!(policy.opened(second, Duration::ZERO));
    }

    #[test]
    fn unobservable_or_locked_windows_session_fails_closed() {
        let mut policy = Policy::default();
        assert_eq!(policy.begin_open(Duration::ZERO), None);
        policy.observe(Signal::Locked);
        assert_eq!(policy.begin_open(Duration::ZERO), None);
    }

    #[test]
    fn large_native_tickets_remain_exact_and_distinct_across_transitions() {
        let mut policy = Policy {
            generation: 9_007_199_254_740_992,
            ..Policy::default()
        };
        policy.observe(Signal::Available);
        let a = policy.begin_open(Duration::ZERO).unwrap();
        assert_eq!(a.0, 9_007_199_254_740_993);
        assert!(policy.opened(a, Duration::ZERO));
        let b = policy.begin_open(Duration::ZERO).unwrap();
        assert_ne!(a, b);
        assert_eq!(b.0, 9_007_199_254_740_994);
        assert!(!policy.opened(a, Duration::ZERO));
        assert!(policy.opened(b, Duration::ZERO));
    }
}
