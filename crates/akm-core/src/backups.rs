//! The ledger of the dated backups that AKM's save code wrote (FR-01, core review 1,
//! finding 1).
//!
//! Pruning deletes a file only with evidence that AKM's save code wrote it. A name and a hash
//! are not that evidence: anyone who can write the folder can make a file with an AKM name
//! whose bytes hash to the value in the name. The evidence is the ledger: a list of backup
//! names and SHA-256 values in the database's own custom data, inside the encrypted vault.
//! Only a writer who knows the master password can add an entry.
//!
//! The order keeps the record ahead of the delete (fault class C). A save chooses the name of
//! its backup first and records it in the version that it publishes. The backup is written
//! before that version replaces the vault, and pruning runs after, from the ledger of the
//! published version. So every file that is deleted is listed in a durable version, and a
//! crash leaves at worst one backup that no ledger lists. A file that no ledger lists is never
//! deleted: a foreign file, a backup from before the ledger, and that one crash leftover.
//!
//! Pruning keeps the newest five entries in save order (contract 2), so a clock that moved
//! back does not change which backups stay. It deletes an older entry only when the file still
//! holds the recorded bytes. The check and the delete use one handle that no other writer can
//! change or delete through (`private_fs::remove_file_if`).

use std::path::{Path, PathBuf};

use chrono::{NaiveDateTime, Utc};
use keepass::db::{CustomDataItem, CustomDataValue, Database};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// The dated backups that pruning keeps (contract 2). The `.bak` copy is apart from them.
pub(crate) const KEPT: usize = 5;

/// The key of the ledger in the custom data of the database (KDBX `Meta/CustomData`).
pub(crate) const LEDGER_KEY: &str = "AKM_BACKUPS";
const LEDGER_VERSION: u32 = 1;

/// One backup that AKM's save code wrote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Backup {
    pub name: String,
    pub sha256: String,
}

#[derive(Serialize, Deserialize)]
struct Ledger {
    version: u32,
    backups: Vec<Backup>,
}

fn sha256(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn file_name(vault: &Path) -> String {
    vault
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

impl Backup {
    /// A new name for a backup of `bytes` beside `vault`:
    /// `<vault file name>.bak.<UTC time>_<uuid>_<SHA-256>.kdbx`. The time is for a person;
    /// the order of the ledger is the order of the saves.
    pub(crate) fn plan(vault: &Path, bytes: &[u8]) -> Self {
        let sha256 = sha256(bytes);
        let name = format!(
            "{}.bak.{}_{}_{sha256}.kdbx",
            file_name(vault),
            Utc::now().format("%Y%m%dT%H%M%S%fZ"),
            Uuid::new_v4()
        );
        Self { name, sha256 }
    }

    pub(crate) fn path(&self, vault: &Path) -> PathBuf {
        vault.with_file_name(&self.name)
    }

    /// The entry names a backup of this vault in the format that `plan` writes, in the
    /// vault's own folder. A vault that was renamed leaves its old entries without a match,
    /// so they are kept, never deleted.
    fn belongs_to(&self, vault: &Path) -> bool {
        let plain = Path::new(&self.name).file_name() == Some(self.name.as_ref())
            && !self.name.contains(['/', '\\', ':']);
        plain && certified_hash(&file_name(vault), &self.name) == Some(self.sha256.as_str())
    }

    /// The file is a regular file that holds the recorded bytes.
    fn holds_its_bytes(&self, vault: &Path) -> bool {
        let path = self.path(vault);
        std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_file())
            && std::fs::File::open(&path)
                .and_then(|mut file| {
                    crate::private_fs::read_bounded(&mut file, crate::vault::MAX_VAULT_BYTES)
                })
                .is_ok_and(|bytes| sha256(&bytes) == self.sha256)
    }
}

/// The SHA-256 that a dated backup name claims, if the name has the format that
/// [`Backup::plan`] writes: `<vault file name>.bak.<25-character time>_<uuid>_<64 lower-case
/// hex digits>.kdbx`.
fn certified_hash<'a>(vault_name: &str, file_name: &'a str) -> Option<&'a str> {
    let rest = file_name
        .strip_prefix(vault_name)?
        .strip_prefix(".bak.")?
        .strip_suffix(".kdbx")?;
    let (timestamp, rest) = rest.split_once('_')?;
    let (id, hash) = rest.split_once('_')?;
    let well_formed = timestamp.len() == 25
        && timestamp.bytes().all(|byte| byte.is_ascii_alphanumeric())
        && id.len() == 36
        && Uuid::parse_str(id).is_ok()
        && hash.len() == 64
        && hash
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
    well_formed.then_some(hash)
}

/// The ledger of `db`, oldest first. A missing or unreadable ledger is empty, so pruning
/// deletes nothing.
pub(crate) fn read(db: &Database) -> Vec<Backup> {
    match db
        .meta
        .custom_data
        .get(LEDGER_KEY)
        .and_then(|item| item.value.as_ref())
    {
        Some(CustomDataValue::String(text)) => serde_json::from_str::<Ledger>(text)
            .ok()
            .filter(|ledger| ledger.version == LEDGER_VERSION)
            .map(|ledger| ledger.backups)
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Records `new` in the ledger of `db`, the version that is about to be published. The
/// entries that stay are those that belong to this vault and whose file still holds the
/// recorded bytes, so a pruned or replaced file leaves the ledger here.
pub(crate) fn record(db: &mut Database, vault: &Path, new: Backup, now: NaiveDateTime) {
    let mut backups: Vec<Backup> = read(db)
        .into_iter()
        .filter(|backup| backup.belongs_to(vault) && backup.holds_its_bytes(vault))
        .collect();
    backups.push(new);
    let ledger = Ledger {
        version: LEDGER_VERSION,
        backups,
    };
    let text = serde_json::to_string(&ledger).expect("a ledger is plain data");
    db.meta.custom_data.insert(
        LEDGER_KEY.to_string(),
        CustomDataItem {
            value: Some(CustomDataValue::String(text)),
            last_modification_time: Some(now),
        },
    );
}

/// Deletes the backups that the ledger of the published version `db` lists beyond the newest
/// [`KEPT`], each only while its file holds the recorded bytes. Nothing else is touched.
pub(crate) fn prune(db: &Database, vault: &Path) -> std::io::Result<()> {
    let ledger = read(db);
    let excess = ledger.len().saturating_sub(KEPT);
    for backup in &ledger[..excess] {
        if !backup.belongs_to(vault) {
            continue;
        }
        crate::private_fs::remove_file_if(&backup.path(vault), |bytes| {
            sha256(bytes) == backup.sha256
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ledger entry whose name leaves the folder, or does not have the backup format of
    /// this vault, is never a file to delete.
    #[test]
    fn an_entry_that_names_another_folder_or_format_does_not_belong() {
        let vault = Path::new(r"C:\vaults\vault.kdbx");
        let good = Backup::plan(vault, b"fake bytes");
        assert!(good.belongs_to(vault));
        assert_eq!(good.path(vault).parent(), vault.parent());
        let hash = good.sha256.clone();
        for name in [
            format!(
                r"..\vault.kdbx.bak.20260101T000000000000000Z_{}_{hash}.kdbx",
                Uuid::new_v4()
            ),
            format!(
                r"vault.kdbx.bak.2026010\..\0000000000000Z_{}_{hash}.kdbx",
                Uuid::new_v4()
            ),
            format!(
                "other.kdbx.bak.20260101T000000000000000Z_{}_{hash}.kdbx",
                Uuid::new_v4()
            ),
            format!(
                "vault.kdbx.bak.20260101T000000000000000Z_{}_{hash}.txt",
                Uuid::new_v4()
            ),
            "vault.kdbx".to_string(),
        ] {
            let entry = Backup {
                name: name.clone(),
                sha256: hash.clone(),
            };
            assert!(!entry.belongs_to(vault), "{name}");
        }
        let wrong = Backup {
            name: good.name.clone(),
            sha256: sha256(b"other bytes"),
        };
        assert!(
            !wrong.belongs_to(vault),
            "the hash in the name is the recorded one"
        );
    }
}
