//! Dates and due times (unit 3, findings L3 and L6; design: `docs/design/unit3.md`, section 3).
//!
//! Scheduling uses exact instants. Whole days exist only to show a distance to a person, and
//! they round so that a key is never shown as safe when it is past due.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;

use crate::error::{Error, Result};

const DAY_MILLISECONDS: i64 = 86_400_000;

/// The instant of a date-only expiry: the last second of that date in UTC. The GUI, the CLI
/// and every export use it, so the same date means the same moment everywhere.
pub fn end_of_day(date: NaiveDate) -> DateTime<Utc> {
    date.and_hms_opt(23, 59, 59)
        .expect("23:59:59 is a valid time")
        .and_utc()
}

/// An expiry from `YYYY-MM-DD` (see [`end_of_day`]) or from a full RFC 3339 instant.
pub fn parse_expiry(raw: &str) -> Result<DateTime<Utc>> {
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Ok(end_of_day(date));
    }
    if let Ok(instant) = DateTime::parse_from_rfc3339(raw) {
        return Ok(instant.with_timezone(&Utc));
    }
    Err(Error::Invalid(format!(
        "could not parse expiry {raw:?}; use YYYY-MM-DD"
    )))
}

/// How far a due instant is from now.
///
/// `In` counts whole days rounded up, so it is at least 1 and a partial day counts as a day.
/// `Overdue` counts whole days already late, so it is 0 for the first day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum Remaining {
    In { days: i64 },
    Now,
    Overdue { days: i64 },
}

impl Remaining {
    pub fn of(due: DateTime<Utc>, now: DateTime<Utc>) -> Self {
        let left = (due - now).num_milliseconds();
        match left {
            0 => Self::Now,
            left if left > 0 => Self::In {
                days: (left + DAY_MILLISECONDS - 1) / DAY_MILLISECONDS,
            },
            late => Self::Overdue {
                days: -late / DAY_MILLISECONDS,
            },
        }
    }

    /// True at the due instant and after it.
    pub fn is_past(self) -> bool {
        !matches!(self, Self::In { .. })
    }
}

/// The due instant of a rotation interval that starts at `changed`. `None` when the instant
/// is out of the range that a date can hold: a stored interval is not trusted (FR-15), and
/// no value can make this panic.
pub fn rotation_due(changed: DateTime<Utc>, days: u32) -> Option<DateTime<Utc>> {
    changed.checked_add_signed(Duration::try_days(i64::from(days))?)
}
