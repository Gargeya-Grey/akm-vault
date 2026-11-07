use std::path::{Path, PathBuf};

use akm_core::audit::AuditLog;
use akm_core::{KeyDisclosure, Vault};

/// A vault's journal in the Cargo-provided test root, with the path of its journal file
/// (bound, and not created until the first record).
pub(crate) struct Journal {
    log: AuditLog,
    path: PathBuf,
}

impl Journal {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// The lock file that a journal append takes.
    pub(crate) fn lock_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(".lock");
        PathBuf::from(name)
    }

    /// The journal ID, which names the chain that a record must continue.
    pub(crate) fn id(&self) -> String {
        let name = self
            .path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        name.trim_start_matches("j-")
            .trim_end_matches(".jsonl")
            .to_string()
    }
}

impl std::ops::Deref for Journal {
    type Target = AuditLog;
    fn deref(&self) -> &AuditLog {
        &self.log
    }
}

pub(crate) fn journal_of(vault: &Path) -> Journal {
    let log = AuditLog::for_vault(vault);
    let path = log.create_journal_path().unwrap();
    Journal { log, path }
}

/// A stored secret, read through a recorded disclosure request.
pub(crate) fn secret_of(vault: &Vault, id: &str) -> String {
    let key = vault.get_by_id(id).unwrap();
    let proof = vault
        .disclose_key(KeyDisclosure::Reveal, &key, Some("test"))
        .unwrap();
    vault.secret(id, &proof).unwrap().to_string()
}

pub(crate) fn secret_titled(vault: &Vault, title: &str) -> String {
    secret_of(vault, &vault.find(title).unwrap().id)
}

/// The variables of one scope with values, through a recorded export request.
pub(crate) fn env_of(vault: &Vault, project: &str, environment: &str) -> Vec<(String, String)> {
    let plan = vault
        .plan_env(Some(project), Some(environment), false)
        .unwrap();
    let proof = vault
        .disclose_env(akm_core::EnvDisclosure::Export, &plan, Some("test"))
        .unwrap();
    vault.env_vars(&plan, &proof).unwrap()
}
