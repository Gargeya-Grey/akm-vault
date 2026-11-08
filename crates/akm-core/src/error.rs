use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("vault not found: {0}")]
    VaultNotFound(String),

    #[error("another process is writing the vault (lock file is held)")]
    Locked,

    #[error("vault changed on disk since it was opened; refusing to overwrite it")]
    VaultChangedOnDisk,

    #[error("this vault requires an explicit upgrade to KDBX 4.1 before it can be changed")]
    UpgradeRequired,

    #[error("could not open vault (wrong password, or corrupt file): {0}")]
    Open(String),

    #[error("could not save vault: {0}")]
    Save(String),

    #[error("no entry matches {0:?}")]
    EntryNotFound(String),

    #[error("{0:?} is the exact title of {1} keys; select one by its id")]
    Ambiguous(String, usize),

    #[error("activity journal unavailable: {0}")]
    Journal(String),

    #[error("invalid input: {0}")]
    Invalid(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
