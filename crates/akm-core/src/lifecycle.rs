//! Types for the lifecycle of a key: the clock, the secret-change record, native history,
//! the Recycle Bin and master-key changes. Design: `docs/design/unit2.md`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::model::ApiKey;

/// The source of "now" for every time that the vault writes. `Vault::with_clock` replaces
/// the system clock, so tests need no sleeps.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// What AKM knows about the last change of a key's secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SecretChange {
    /// AKM recorded the time (`AKM_SECRET_CHANGED`) and nothing contradicts it.
    Known,
    /// No time is recorded. AKM shows "unknown" and invents no date.
    #[default]
    Unknown,
    /// The recorded time is older than a secret change that another program made in the
    /// entry history. The old time would be wrong, so none is shown.
    ChangedOutside,
}

/// One earlier version of a key, from the native KDBX history. It holds names and a flag,
/// never a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryVersion {
    /// 0 is the version that the newest change replaced.
    pub index: usize,
    pub modified_at: Option<DateTime<Utc>>,
    /// The KDBX field names that differ from the version that replaced this one.
    pub changed: Vec<String>,
    pub secret_changed: bool,
}

/// A key in the Recycle Bin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecycledKey {
    pub key: ApiKey,
    /// When the key moved to the Recycle Bin (`LocationChanged`).
    pub deleted_at: Option<DateTime<Utc>>,
}

/// The receipt of a rotation. It holds no secret.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Rotation {
    pub key: ApiKey,
    pub changed_at: DateTime<Utc>,
    pub previous_changed_at: Option<DateTime<Utc>>,
}
