//! What a clipboard publication did, as AKM observed it (unit 1h, design D6).
//!
//! Only this module can build a [`Publication`]. The publisher walks a chain of states.
//! Each step consumes its state and every failure builds the one outcome that its state
//! allows, so no exit can claim an outcome from a phase it did not reach.
//!
//! ```
//! use akm_desktop_lib::{error::CommandError, publication::*};
//! use std::num::NonZeroU32;
//!
//! // A full chain compiles, and ends in a verified publication.
//! let done = begin()
//!     .opened(5)
//!     .emptied()
//!     .writing()
//!     .published(NonZeroU32::new(6).unwrap());
//! assert!(matches!(done.into_outcome(), Outcome::Published(_)));
//!
//! // A failed first empty is an outcome of the open phase.
//! let error = CommandError::new("clipboard", "fake");
//! let failed = begin().opened(5).empty_failed(error, 5);
//! assert!(matches!(failed.into_outcome(), Outcome::Untouched(_)));
//! ```
//!
//! ```compile_fail
//! // The state before the first empty is moved by `emptied`; an outcome that belongs to
//! // that earlier phase cannot be built afterwards.
//! use akm_desktop_lib::{error::CommandError, publication::*};
//! let opened = begin().opened(5);
//! let _emptied = opened.emptied();
//! let _ = opened.empty_failed(CommandError::new("clipboard", "fake"), 5);
//! ```
//!
//! ```compile_fail
//! // A publication cannot be verified before the clipboard was emptied and written.
//! use akm_desktop_lib::publication::*;
//! use std::num::NonZeroU32;
//! let _ = begin().opened(5).published(NonZeroU32::new(6).unwrap());
//! ```
//!
//! ```compile_fail
//! // An empty cannot be skipped: the chain has no step from `Preparing` to `Emptied`.
//! use akm_desktop_lib::publication::*;
//! let _ = begin().emptied();
//! ```
//!
//! ```compile_fail
//! // An outcome cannot be wrapped into a publication outside this module.
//! use akm_desktop_lib::{error::CommandError, publication::*};
//! let _ = Publication::new(Outcome::Untouched(CommandError::new("clipboard", "fake")));
//! ```
//!
//! ```compile_fail
//! // Lease evidence cannot be built outside this module.
//! use akm_desktop_lib::publication::*;
//! let _ = LeaseEvidence(std::num::NonZeroU32::new(1).unwrap());
//! ```

use std::num::NonZeroU32;

use crate::error::CommandError;

/// What a publication did, as AKM observed it.
pub struct Publication {
    outcome: Outcome,
    owner_leaked: bool,
}

pub enum Outcome {
    /// No clipboard change was observed. An earlier lease stays.
    Untouched(CommandError),
    /// `EmptyClipboard` failed and the sequence changed: the effect is unknown.
    EarlierCopyUncertain(CommandError),
    /// The first `EmptyClipboard` succeeded, and no byte of the new value remains.
    Emptied(CommandError),
    /// Published and verified.
    Published(LeaseEvidence),
    /// A secret was written, the rollback failed, and the sequence did not change.
    Residual(CommandError, LeaseEvidence),
    /// A secret was written, the rollback failed, and the sequence changed or was 0.
    ResidualUntracked(CommandError),
}

/// A nonzero clipboard sequence sampled in the publication's own open interval.
pub struct LeaseEvidence(NonZeroU32);

impl LeaseEvidence {
    pub(crate) fn sequence(&self) -> u32 {
        self.0.get()
    }
}

/// What closing the clipboard and destroying the owner window reported (design D5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Release {
    pub close_failed: bool,
    pub owner_leaked: bool,
}

const NOT_RELEASED: &str = "Windows did not confirm that AKM released the clipboard.";

fn with(error: CommandError, suffix: &str) -> CommandError {
    CommandError::new(&error.kind, format!("{} {suffix}", error.message))
}

impl Outcome {
    /// The outcome after `CloseClipboard` failed (the table in design D5). A failed close can
    /// leave the clipboard held, so a sequence sampled before it proves nothing about the end
    /// state: a published copy is no longer tracked.
    fn after_failed_close(self) -> Self {
        match self {
            Self::Untouched(error) => Self::Untouched(with(error, NOT_RELEASED)),
            Self::EarlierCopyUncertain(error) => {
                Self::EarlierCopyUncertain(with(error, NOT_RELEASED))
            }
            Self::Emptied(error) => Self::Emptied(with(error, NOT_RELEASED)),
            Self::Published(_) => {
                Self::ResidualUntracked(CommandError::new("clipboard", NOT_RELEASED))
            }
            Self::Residual(error, _) | Self::ResidualUntracked(error) => {
                Self::ResidualUntracked(with(error, NOT_RELEASED))
            }
        }
    }
}

impl Publication {
    fn new(outcome: Outcome) -> Self {
        Self {
            outcome,
            owner_leaked: false,
        }
    }

    /// Platforms without the private clipboard: nothing was touched.
    #[cfg(not(windows))]
    pub(crate) fn unsupported(error: CommandError) -> Self {
        Self::new(Outcome::Untouched(error))
    }

    pub fn into_outcome(self) -> Outcome {
        self.outcome
    }

    /// The owner window was not destroyed. The clipboard content and sequence are not
    /// affected, because the owner provides no delayed-rendered data.
    pub fn owner_leaked(&self) -> bool {
        self.owner_leaked
    }

    /// Applies what the release of the clipboard and the owner reported.
    pub fn released(self, release: Release) -> Self {
        let outcome = if release.close_failed {
            self.outcome.after_failed_close()
        } else {
            self.outcome
        };
        Self {
            outcome,
            owner_leaked: self.owner_leaked || release.owner_leaked,
        }
    }
}

/// The result of the second `EmptyClipboard` that a failure after the first one tries.
pub struct Rollback {
    emptied: bool,
    before: u32,
    after: u32,
}

impl Rollback {
    /// `before` and `after` are the sequences just before and just after the second empty.
    pub fn new(emptied: bool, before: u32, after: u32) -> Self {
        Self {
            emptied,
            before,
            after,
        }
    }
}

/// The start of a publication: nothing is open and nothing changed.
pub fn begin() -> Preparing {
    Preparing(())
}

pub struct Preparing(());

impl Preparing {
    /// A failure before the clipboard was opened.
    pub fn fail(self, error: CommandError) -> Publication {
        Publication::new(Outcome::Untouched(error))
    }

    /// The clipboard is open; `sequence` was sampled before any change.
    pub fn opened(self, sequence: u32) -> Opened {
        Opened { sequence }
    }
}

pub struct Opened {
    sequence: u32,
}

impl Opened {
    /// A failure before the first empty.
    pub fn fail(self, error: CommandError) -> Publication {
        Publication::new(Outcome::Untouched(error))
    }

    /// The first `EmptyClipboard` failed; `after` was sampled in the same open interval.
    pub fn empty_failed(self, error: CommandError, after: u32) -> Publication {
        if after == self.sequence && after != 0 {
            Publication::new(Outcome::Untouched(error))
        } else {
            Publication::new(Outcome::EarlierCopyUncertain(error))
        }
    }

    pub fn emptied(self) -> Emptied {
        Emptied(())
    }
}

pub struct Emptied(());

impl Emptied {
    /// A failure after the first empty and before any secret format. The second empty runs,
    /// and its result does not matter, because no byte of the value was written.
    pub fn fail(self, error: CommandError, rollback: impl FnOnce() -> Rollback) -> Publication {
        let _ = rollback();
        Publication::new(Outcome::Emptied(error))
    }

    /// Called before the first secret format, so that every later failure rolls back.
    pub fn writing(self) -> Written {
        Written(())
    }
}

pub struct Written(());

impl Written {
    /// A failure after a secret format may exist: the rollback decides the outcome.
    pub fn fail(self, error: CommandError, rollback: impl FnOnce() -> Rollback) -> Publication {
        let result = rollback();
        if result.emptied {
            return Publication::new(Outcome::Emptied(error));
        }
        match NonZeroU32::new(result.before) {
            Some(sequence) if result.after == result.before => {
                Publication::new(Outcome::Residual(error, LeaseEvidence(sequence)))
            }
            _ => Publication::new(Outcome::ResidualUntracked(error)),
        }
    }

    /// Verified, with a sequence from the same open interval.
    pub fn published(self, sequence: NonZeroU32) -> Publication {
        Publication::new(Outcome::Published(LeaseEvidence(sequence)))
    }
}

#[cfg(test)]
impl Publication {
    /// Test backends only: production outcomes come from the chain above.
    pub(crate) fn for_test(outcome: Outcome) -> Self {
        Self::new(outcome)
    }

    /// The published sequence; panics on any failure.
    pub(crate) fn published_for_test(self) -> u32 {
        match self.outcome {
            Outcome::Published(evidence) => evidence.sequence(),
            _ => panic!("expected a publication, got {}", self.name_for_test()),
        }
    }

    /// The failure's error; panics on a publication.
    pub(crate) fn error_for_test(self) -> CommandError {
        match self.outcome {
            Outcome::Untouched(e)
            | Outcome::EarlierCopyUncertain(e)
            | Outcome::Emptied(e)
            | Outcome::Residual(e, _)
            | Outcome::ResidualUntracked(e) => e,
            Outcome::Published(_) => panic!("expected a failure, got a publication"),
        }
    }

    pub(crate) fn name_for_test(&self) -> &'static str {
        match &self.outcome {
            Outcome::Untouched(_) => "Untouched",
            Outcome::EarlierCopyUncertain(_) => "EarlierCopyUncertain",
            Outcome::Emptied(_) => "Emptied",
            Outcome::Published(_) => "Published",
            Outcome::Residual(..) => "Residual",
            Outcome::ResidualUntracked(_) => "ResidualUntracked",
        }
    }
}

#[cfg(test)]
impl LeaseEvidence {
    pub(crate) fn for_test(sequence: u32) -> Self {
        Self(NonZeroU32::new(sequence).expect("test evidence is nonzero"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(text: &str) -> CommandError {
        CommandError::new("clipboard", text)
    }

    #[test]
    fn the_chain_derives_each_outcome_from_its_phase() {
        assert_eq!(
            begin().fail(error("a")).name_for_test(),
            "Untouched",
            "before open"
        );
        assert_eq!(
            begin().opened(5).fail(error("a")).name_for_test(),
            "Untouched",
            "open, before empty"
        );
        assert_eq!(
            begin()
                .opened(5)
                .empty_failed(error("a"), 5)
                .name_for_test(),
            "Untouched"
        );
        assert_eq!(
            begin()
                .opened(5)
                .empty_failed(error("a"), 6)
                .name_for_test(),
            "EarlierCopyUncertain"
        );
        assert_eq!(
            begin()
                .opened(5)
                .empty_failed(error("a"), 0)
                .name_for_test(),
            "EarlierCopyUncertain"
        );
        let emptied = begin().opened(5).emptied();
        assert_eq!(
            emptied
                .fail(error("a"), || Rollback::new(false, 6, 6))
                .name_for_test(),
            "Emptied"
        );
        let written = || begin().opened(5).emptied().writing();
        assert_eq!(
            written()
                .fail(error("a"), || Rollback::new(true, 6, 7))
                .name_for_test(),
            "Emptied"
        );
        assert_eq!(
            written()
                .fail(error("a"), || Rollback::new(false, 6, 6))
                .name_for_test(),
            "Residual"
        );
        assert_eq!(
            written()
                .fail(error("a"), || Rollback::new(false, 6, 7))
                .name_for_test(),
            "ResidualUntracked"
        );
        assert_eq!(
            written()
                .fail(error("a"), || Rollback::new(false, 0, 0))
                .name_for_test(),
            "ResidualUntracked"
        );
    }
}
