use akm_core::Error;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub kind: String,
    pub message: String,
}

impl CommandError {
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
        }
    }

    pub fn locked() -> Self {
        Self::new(
            "locked",
            "The session is locked or expired. Unlock the vault again.",
        )
    }

    pub fn completed_while_locked() -> Self {
        Self::new(
            "completedWhileLocked",
            "An operation completed, but its originating session is no longer available. Do not repeat it. Open the relevant vault or check its export to inspect the saved result.",
        )
    }

    pub fn internal(error: impl std::fmt::Display) -> Self {
        Self::new("internal", error.to_string())
    }

    /// A native step stopped in a way that leaves its result unknown: it panicked, and the
    /// panic text can name a path or hold a value, so it is not shown. The text never invites a
    /// retry, because the step may have finished (U1-R1-04).
    pub fn failed_step() -> Self {
        Self::new(
            "internal",
            "A native step failed, so AKM cannot say whether it finished. Check the result before you try again.",
        )
    }
}

impl From<Error> for CommandError {
    fn from(error: Error) -> Self {
        match &error {
            Error::VaultNotFound(path) => Self::new("notFound", format!("No vault at {path}.")),
            Error::Locked => Self::new("locked", "Another writer is saving this vault. Try again shortly."),
            Error::VaultChangedOnDisk => Self::new("changedOnDisk", "The vault changed on disk. Your change was not saved. Reload before trying again."),
            Error::UpgradeRequired => Self::new("upgradeRequired", "Confirm the KDBX 4.1 upgrade before writing this vault."),
            Error::Open(_) => Self::new("open", "That password does not open this vault, or the file is not a supported KDBX vault."),
            Error::Save(detail) => Self::new("save", format!("The vault could not be saved. ({detail})")),
            Error::EntryNotFound(_) => Self::new("notFound", error.to_string()),
            Error::Ambiguous(_, _) => Self::new("ambiguous", error.to_string()),
            Error::Invalid(_) => Self::new("invalid", error.to_string()),
            Error::Journal(_) => Self::new("journal", error.to_string()),
            Error::Io(_) | Error::Json(_) => Self::new("io", error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_upgrade_requirement_has_a_distinct_ipc_error_kind() {
        let error = CommandError::from(Error::UpgradeRequired);
        assert_eq!(
            serde_json::to_value(error).unwrap()["kind"],
            "upgradeRequired"
        );
    }
}
