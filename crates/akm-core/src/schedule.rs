//! Dates and due times (unit 3, findings L3 and L6; design: `docs/design/unit3.md`, section 3).
//!
//! Scheduling uses exact instants. Whole days exist only to show a distance to a person, and
//! they round so that a key is never shown as safe when it is past due.
//!
//! This module is the only place in AKM that computes with dates (core review 1, finding 6).
//! A day count comes from a person or from a vault, so no count can make it panic: each
//! function here gives a checked or a saturated result, and the lint below refuses an
//! operator that can overflow. No test stops other code from computing with dates: a review
//! does.
#![deny(clippy::arithmetic_side_effects)]

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, SecondsFormat, Utc};
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
///
/// A vault keeps whole seconds. An instant with a fraction of a second is refused, so a
/// stored expiry is always the instant that the person wrote (FR-18). A fraction of zero
/// (`.000`) loses nothing and is accepted. The check reads the text, because chrono keeps
/// nine fraction digits and drops the rest (core review 1, finding 7).
pub fn parse_expiry(raw: &str) -> Result<DateTime<Utc>> {
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return Ok(end_of_day(date));
    }
    if let Ok(instant) = DateTime::parse_from_rfc3339(raw) {
        let fraction = raw.split_once('.').map_or("", |(_, rest)| rest);
        if fraction
            .chars()
            .take_while(char::is_ascii_digit)
            .any(|digit| digit != '0')
        {
            return Err(fraction_refused(raw));
        }
        return Ok(instant.with_timezone(&Utc));
    }
    Err(Error::Invalid(format!(
        "could not parse expiry {raw:?}; use YYYY-MM-DD"
    )))
}

/// Refuses an expiry with a fraction of a second, from any caller. [`ApiKeyInput`] calls it, so
/// core `add` and `update` hold the rule for the CLI and the desktop alike (FR-18, core review 1,
/// finding 7).
///
/// [`ApiKeyInput`]: crate::model::ApiKeyInput
pub fn require_whole_second(expires: DateTime<Utc>) -> Result<()> {
    if expires.timestamp_subsec_nanos() == 0 {
        return Ok(());
    }
    Err(fraction_refused(
        &expires.to_rfc3339_opts(SecondsFormat::AutoSi, true),
    ))
}

fn fraction_refused(shown: &str) -> Error {
    Error::Invalid(format!(
        "expiry {shown:?} has a fraction of a second, and a vault keeps whole seconds; write it without the fraction, for example 2026-11-10T12:00:00Z"
    ))
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
    /// The distance between two instants. The range of a date is far smaller than the range
    /// of a millisecond count, so the saturating steps here never saturate in practice.
    pub fn of(due: DateTime<Utc>, now: DateTime<Utc>) -> Self {
        let left = due.signed_duration_since(now).num_milliseconds();
        match left {
            0 => Self::Now,
            left if left > 0 => Self::In {
                days: left
                    .saturating_sub(1)
                    .div_euclid(DAY_MILLISECONDS)
                    .saturating_add(1),
            },
            late => Self::Overdue {
                days: late.saturating_neg().div_euclid(DAY_MILLISECONDS),
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

/// The instant `days` whole days after `instant`. A count beyond the range of a date
/// saturates at the last or the first instant that a date can hold, so `list --expiring` with
/// any count lists every key with an expiry, or none (core review 1, finding 6).
pub fn days_after(instant: DateTime<Utc>, days: i64) -> DateTime<Utc> {
    Duration::try_days(days)
        .and_then(|span| instant.checked_add_signed(span))
        .unwrap_or(if days < 0 {
            DateTime::<Utc>::MIN_UTC
        } else {
            DateTime::<Utc>::MAX_UTC
        })
}

/// The instant `days` whole days before `instant`. A count beyond the range of a date
/// saturates at the first instant that a date can hold, so a retention purge with any count
/// finds nothing older than that (core review 1, finding 6).
pub fn days_before(instant: DateTime<Utc>, days: u32) -> DateTime<Utc> {
    Duration::try_days(i64::from(days))
        .and_then(|span| instant.checked_sub_signed(span))
        .unwrap_or(DateTime::<Utc>::MIN_UTC)
}

/// The second after `time`, or `None` at the last second that a date can hold.
pub fn second_after(time: NaiveDateTime) -> Option<NaiveDateTime> {
    time.checked_add_signed(Duration::try_seconds(1)?)
}
