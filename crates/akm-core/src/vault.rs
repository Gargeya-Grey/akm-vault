use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use fs2::FileExt;
use keepass::config::{
    CompressionConfig, DatabaseConfig, DatabaseVersion, InnerCipherConfig, KdfConfig,
    OuterCipherConfig,
};
#[cfg(test)]
use keepass::db::Times;
use keepass::db::{fields, Database, Entry, EntryId, EntryMut, EntryRef, GroupId, Value};
use keepass::DatabaseKey;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::audit::AuditLog;
use crate::disclosure::{Disclosure, EnvDisclosure, EnvPlan, KeyDisclosure, Stamp};
use crate::error::{Error, Result};
use crate::journal::{JournalRoot, VaultEvidence};
use crate::lifecycle::{Clock, HistoryVersion, RecycledKey, Rotation, SecretChange, SystemClock};
use crate::model::*;
use crate::private_fs;

/// Entries live under this top-level group so a vault can also hold unrelated
/// KeePass entries without them showing up as API keys.
const GROUP_NAME: &str = "API Keys";
/// How many earlier fingerprints a session keeps for the journal. The first one is always kept.
const LINEAGE_LIMIT: usize = 16;

/// The largest vault file that AKM reads (CORE-R2-06). The bytes of a vault file are not
/// authenticated until its key is checked, so the size is checked before they are read.
pub const MAX_VAULT_BYTES: u64 = 1 << 30;

fn too_large(what: &str) -> String {
    format!(
        "{what} is larger than the {} GiB that AKM reads",
        MAX_VAULT_BYTES >> 30
    )
}

/// The bytes of the vault file at `path`, with the bound of [`MAX_VAULT_BYTES`].
fn read_vault_file(path: &Path) -> Result<Vec<u8>> {
    match private_fs::read_bounded(&mut File::open(path)?, MAX_VAULT_BYTES) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::FileTooLarge => {
            Err(Error::Open(too_large("the vault file")))
        }
        Err(error) => Err(error.into()),
    }
}

/// A save refuses a vault file with more than one name (hard link): the lock is named from one
/// path, and the replacement changes one name only, so the other names would keep the old
/// vault (CORE-R2-05). Reading stays possible.
fn refuse_names(names: u32) -> Result<()> {
    if names > 1 {
        return Err(Error::Save(format!(
            "the vault file has {names} names (hard links), and a save through one name would leave the old vault under the others. Nothing was changed. Copy the vault to a new file, open the copy, and save there"
        )));
    }
    Ok(())
}

fn refuse_second_name(file: &File) -> Result<()> {
    refuse_names(private_fs::link_count(file)?)
}

/// The file at `path` holds exactly `expected`. It reads at most one byte past its length.
fn holds_exactly(path: &Path, expected: &[u8]) -> std::io::Result<bool> {
    match private_fs::read_bounded(&mut File::open(path)?, expected.len() as u64) {
        Ok(actual) => Ok(actual == expected),
        Err(error) if error.kind() == std::io::ErrorKind::FileTooLarge => Ok(false),
        Err(error) => Err(error),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultWarningKind {
    Journal,
    BackupRefresh,
    BackupPrune,
    BackupRetention,
    /// A file that AKM wrote beside the vault could not be made private to the current
    /// user (the file system keeps no Windows permissions, or refused them).
    FilePermissions,
    /// The vault file changed outside AKM, and AKM cannot confirm that it is the same vault.
    /// Its history continues in the existing journal (FR-12).
    JournalLineageUnconfirmed,
}

#[derive(Debug)]
pub struct VaultWarning {
    pub kind: VaultWarningKind,
    pub message: String,
}

pub struct Vault {
    path: PathBuf,
    db: Database,
    key: DatabaseKey,
    /// SHA-256 of the file as it was when opened/last saved. Used to refuse
    /// overwriting a vault that a cloud client replaced underneath us.
    fingerprint: Option<String>,
    /// The fingerprints this session held before the current one, oldest first: the bytes it
    /// took from the disk, then each of its own saves. It lets the journal tell which vault
    /// the current version came from, because a save changes the fingerprint and a clone
    /// shares the vault ID (unit 1b, D12).
    lineage: Vec<String>,
    warnings: Vec<VaultWarning>,
    /// Set by `log` (which takes `&self`) when a record went into a journal whose link to
    /// this file is not confirmed. `take_warning_records` turns it into a warning (FR-12).
    lineage_unconfirmed: AtomicBool,
    journal: JournalRoot,
    clock: Arc<dyn Clock>,
    /// Names this `Vault` value in the proofs and plans that it makes (review 1, finding 3).
    session: Uuid,
    /// Counts the changes of `db`: every commit and every reload.
    state: u64,
    /// The gate that each save passes its file replacement through. `None` replaces at once.
    commit_gate: Option<Arc<dyn CommitGate>>,
}

/// The point at which a save replaces the vault file. An application that runs sessions
/// installs one (`Vault::set_commit_gate`), so that its own session check and the replacement
/// are one step. The command line installs none.
pub trait CommitGate: Send + Sync {
    /// Runs `replace` once and returns its result, or refuses with an error and does not run
    /// it. A refused save writes nothing: its temporary files are removed.
    fn commit(&self, replace: &mut dyn FnMut() -> Result<()>) -> Result<()>;
}

struct ImportPlan {
    preview: ImportPreview,
    replacements: Vec<(EntryId, String)>,
    additions: Vec<ApiKeyInput>,
}

impl Vault {
    pub fn create(path: impl AsRef<Path>, password: &str, name: Option<&str>) -> Result<Self> {
        Self::create_in(path, password, name, JournalRoot::from_environment())
    }

    /// Like [`Vault::create`], with an explicit journal folder.
    pub fn create_in(
        path: impl AsRef<Path>,
        password: &str,
        name: Option<&str>,
        journal: JournalRoot,
    ) -> Result<Self> {
        if password.chars().count() < 12 {
            return Err(Error::Invalid(
                "master password must contain at least 12 characters".into(),
            ));
        }

        let path = path.as_ref().to_path_buf();
        if path.exists() {
            return Err(Error::Invalid(format!(
                "{} already exists; open it instead of creating a new vault",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let key = DatabaseKey::new().with_password(password);
        let mut db = Database::with_config(hardened_config());
        db.meta.database_name = Some(name.unwrap_or("API Keys").to_string());

        let mut vault = Self {
            path,
            db,
            key,
            fingerprint: None,
            lineage: Vec::new(),
            warnings: Vec::new(),
            lineage_unconfirmed: AtomicBool::new(false),
            journal,
            clock: Arc::new(SystemClock),
            session: Uuid::new_v4(),
            state: 0,
            commit_gate: None,
        };
        vault.save()?;
        Ok(vault)
    }

    pub fn open(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        Self::open_in(path, password, JournalRoot::from_environment())
    }

    /// Like [`Vault::open`], with an explicit journal folder.
    pub fn open_in(path: impl AsRef<Path>, password: &str, journal: JournalRoot) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(Error::VaultNotFound(path.display().to_string()));
        }

        let key = DatabaseKey::new().with_password(password);
        let bytes = read_vault_file(&path)?;
        let fingerprint = Some(bytes_fingerprint(&bytes));
        // FR-28: the header names the cost of the key derivation. Read it first.
        crate::kdf_guard::check(&bytes)?;
        let db = Database::parse(&bytes, key.clone()).map_err(|e| Error::Open(e.to_string()))?;

        Ok(Self {
            path,
            db,
            key,
            fingerprint,
            lineage: Vec::new(),
            warnings: Vec::new(),
            lineage_unconfirmed: AtomicBool::new(false),
            journal,
            clock: Arc::new(SystemClock),
            session: Uuid::new_v4(),
            state: 0,
            commit_gate: None,
        })
    }

    /// The time by this vault's clock. Callers use it for "now" so one clock decides.
    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Installs the gate that every later save passes its file replacement through.
    pub fn set_commit_gate(&mut self, gate: Option<Arc<dyn CommitGate>>) {
        self.commit_gate = gate;
    }

    /// Replaces the clock that stamps every time this vault writes (design D9).
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn take_warnings(&mut self) -> Vec<String> {
        self.take_warning_records()
            .into_iter()
            .map(|warning| warning.message)
            .collect()
    }

    pub fn take_warning_records(&mut self) -> Vec<VaultWarning> {
        let mut warnings = std::mem::take(&mut self.warnings);
        if self.lineage_unconfirmed.swap(false, Ordering::Relaxed) {
            warnings.push(VaultWarning {
                kind: VaultWarningKind::JournalLineageUnconfirmed,
                message: "this vault file changed outside AKM, so AKM cannot confirm that it is the same vault. Its history continues in the existing journal.".into(),
            });
        }
        warnings
    }

    pub fn reload(&mut self) -> Result<()> {
        let bytes = read_vault_file(&self.path)?;
        let fingerprint = bytes_fingerprint(&bytes);
        crate::kdf_guard::check(&bytes)?;
        let db = Database::parse(&bytes, self.key.clone())
            .map_err(|error| Error::Open(error.to_string()))?;
        self.db = db;
        self.fingerprint = Some(fingerprint);
        self.lineage.clear();
        self.state += 1;
        Ok(())
    }

    pub fn needs_upgrade(&self) -> bool {
        !matches!(self.db.config.version, DatabaseVersion::KDB4(1))
    }

    pub fn upgrade(&mut self) -> Result<()> {
        if !self.needs_upgrade() {
            return Ok(());
        }
        if !matches!(
            self.db.config.version,
            DatabaseVersion::KDB3(_) | DatabaseVersion::KDB4(0)
        ) {
            return Err(Error::Invalid(
                "only KDBX 3.x and 4.0 vaults can be upgraded".into(),
            ));
        }
        let mut candidate = self.db.clone();
        candidate.config.version = DatabaseVersion::KDB4(1);
        self.commit(candidate)?;
        self.log_committed("upgrade", None, Some("explicit upgrade to KDBX 4.1"));
        Ok(())
    }

    fn ensure_writable(&self) -> Result<()> {
        if self.needs_upgrade() {
            Err(Error::UpgradeRequired)
        } else {
            Ok(())
        }
    }

    /// Name recorded in the KDBX metadata, if any.
    pub fn name(&self) -> Option<&str> {
        self.db.meta.database_name.as_deref()
    }

    pub fn list(&self) -> Vec<ApiKey> {
        let root = self.db.root();
        let keys = root
            .groups()
            .filter(|group| group.name.eq_ignore_ascii_case(GROUP_NAME))
            .flat_map(|group| {
                group
                    .entries()
                    .map(|entry| read_entry(&entry))
                    .collect::<Vec<_>>()
            })
            .collect();
        keys
    }

    pub fn get_by_id(&self, id: &str) -> Result<ApiKey> {
        self.find_entry_id(id)
            .and_then(|id| self.db.entry(id))
            .map(|entry| read_entry(&entry))
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))
    }

    /// Select one key by its native UUID or by its exact title, in any letter case. Both sides
    /// go through [`crate::model::caseless`], so `Éclair` and `éclair`, or `ΛΌΓΟΣ` and `λόγος`,
    /// are one title (FR-17). A partial word never selects a key, so a word found in a note or
    /// a URL cannot reveal, rotate or delete one (L2). Two keys with the same title are
    /// refused. Use [`Vault::candidates`] to list near matches.
    pub fn find(&self, query: &str) -> Result<ApiKey> {
        if let Ok(key) = self.get_by_id(query) {
            return Ok(key);
        }
        let wanted = crate::model::caseless(query);
        let mut exact: Vec<ApiKey> = self
            .list()
            .into_iter()
            .filter(|key| crate::model::caseless(&key.title) == wanted)
            .collect();
        match exact.len() {
            0 => Err(Error::EntryNotFound(query.to_string())),
            1 => Ok(exact.remove(0)),
            n => Err(Error::Ambiguous(query.to_string(), n)),
        }
    }

    /// Keys whose metadata contains the word, for a refusal message. It reads no secret and
    /// grants nothing: it only helps the user pick an exact title or id. A blank word has no
    /// candidates.
    pub fn candidates(&self, query: &str) -> Vec<ApiKey> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        self.list()
            .into_iter()
            .filter(|key| key.matches(query))
            .collect()
    }

    pub fn add(&mut self, input: ApiKeyInput) -> Result<ApiKey> {
        self.ensure_writable()?;
        input.validate()?;
        let now = self.clock.now();
        let mut candidate = self.db.clone();

        let key = {
            let group_id = ensure_group(&mut candidate);
            let mut root = candidate.root_mut();
            let mut group = root
                .group_mut(group_id)
                .ok_or_else(|| Error::Invalid("API Keys group disappeared".into()))?;
            let mut entry = group.add_entry();
            let id = entry.id().to_string();
            finish_new_entry(&mut entry, &input, &id, whole_second(now));
            read_entry(&entry)
        };

        self.commit(candidate)?;
        self.log_committed("add", Some(&key), None);
        Ok(key)
    }

    pub fn update(&mut self, id: &str, input: ApiKeyInput) -> Result<ApiKey> {
        self.ensure_writable()?;
        input.validate_fields()?;
        let now = self.clock.now();
        let entry_id = self
            .find_entry_id(id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        let before: Entry = (*self
            .db
            .entry(entry_id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?)
        .clone();

        // A tracked edit of the `keepass` crate adds history even when nothing changed.
        // Apply the input to a copy first: if the copy equals the entry, there is nothing
        // to save, to record or to stamp (design D4).
        let created = before.times.creation.unwrap_or_else(|| whole_second(now));
        let mut probe = before.clone();
        write_entry(
            &mut probe,
            &input,
            id,
            created,
            before.times.last_modification.unwrap_or(created),
        );
        if probe == before {
            return Ok(read_entry(&before));
        }

        let secret_changed = before.get(fields::PASSWORD) != Some(input.secret.as_str());
        if secret_changed {
            input.reject_nul_secret()?;
        }
        let stamp = next_stamp(now, &before)?;
        let mut candidate = self.db.clone();
        let key = {
            let mut entry = candidate
                .entry_mut(entry_id)
                .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
            keep_version(&mut entry, before);
            write_entry(&mut entry, &input, id, created, stamp);
            if secret_changed {
                entry.set_unprotected(META_SECRET_CHANGED, format_secret_change(stamp));
            }
            read_entry(&entry)
        };

        self.commit(candidate)?;
        self.log_committed("update", Some(&key), None);
        Ok(key)
    }

    /// Replaces the secret of one live key and records the time (design D3). The old
    /// secret stays in the native history. The same value is refused: it is no rotation.
    pub fn rotate(&mut self, id: &str, secret: Secret) -> Result<Rotation> {
        self.ensure_writable()?;
        if secret.is_empty() {
            return Err(Error::Invalid("secret must not be empty".into()));
        }
        if secret.contains_nul() {
            return Err(Error::Invalid(
                "secret must not contain a NUL character".into(),
            ));
        }
        let entry_id = self
            .find_entry_id(id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        let before: Entry = (*self
            .db
            .entry(entry_id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?)
        .clone();
        if before.get(fields::PASSWORD) == Some(secret.as_str()) {
            return Err(Error::Invalid(
                "the new secret is the same as the current secret, so nothing was changed. \
                 If an earlier attempt was saved, the key is already rotated; check its secret-change time."
                    .into(),
            ));
        }
        let previous_changed_at = read_entry(&before).secret_changed_at;
        let stamp = next_stamp(self.clock.now(), &before)?;
        let mut candidate = self.db.clone();
        let key = {
            let mut entry = candidate
                .entry_mut(entry_id)
                .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
            keep_version(&mut entry, before);
            entry.set_protected(fields::PASSWORD, secret.as_str());
            entry.set_unprotected(META_SECRET_CHANGED, format_secret_change(stamp));
            entry.times.last_modification = Some(stamp);
            read_entry(&entry)
        };
        self.commit(candidate)?;
        self.log_committed("rotate", Some(&key), None);
        Ok(Rotation {
            key,
            changed_at: stamp.and_utc(),
            previous_changed_at,
        })
    }

    /// Moves a live key to the Recycle Bin, which it creates when the vault has none
    /// (design D6). Nothing is deleted for good.
    pub fn remove(&mut self, id: &str) -> Result<ApiKey> {
        self.ensure_writable()?;
        let key = self.get_by_id(id)?;
        let entry_id = self
            .find_entry_id(id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        if self.db.meta.recyclebin_enabled == Some(false) {
            return Err(Error::Invalid(
                "this vault's Recycle Bin is turned off, so AKM will not move the key there. \
                 Turn the Recycle Bin on in KeePassXC, or delete the key for good with purge."
                    .into(),
            ));
        }
        let stamp = self.stamp_for(entry_id)?;

        let mut candidate = self.db.clone();
        let bin = ensure_recycle_bin(&mut candidate, stamp);
        let mut entry = candidate
            .entry_mut(entry_id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        entry
            .move_to(bin)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        entry.times.location_changed = Some(stamp);

        self.commit(candidate)?;
        self.log_committed("remove", Some(&key), Some("moved to the Recycle Bin"));
        Ok(key)
    }

    /// The keys in the Recycle Bin, newest deletion first.
    pub fn recycled(&self) -> Vec<RecycledKey> {
        let Some(bin) = self.db.recycle_bin() else {
            return Vec::new();
        };
        let mut keys: Vec<RecycledKey> = bin
            .entries()
            .filter(|entry| is_api_key(entry))
            .map(|entry| RecycledKey {
                key: read_entry(&entry),
                deleted_at: entry.times.location_changed.map(|time| time.and_utc()),
            })
            .collect();
        keys.sort_by(|a, b| {
            b.deleted_at
                .cmp(&a.deleted_at)
                .then(a.key.title.cmp(&b.key.title))
        });
        keys
    }

    /// A recycled key by native UUID, then by exact title in any letter case (the same rule as
    /// [`Vault::find`]).
    pub fn find_recycled(&self, query: &str) -> Result<RecycledKey> {
        let all = self.recycled();
        if let Some(found) = all.iter().find(|item| item.key.id == query) {
            return Ok(found.clone());
        }
        let wanted = crate::model::caseless(query);
        let exact: Vec<_> = all
            .into_iter()
            .filter(|item| crate::model::caseless(&item.key.title) == wanted)
            .collect();
        match exact.len() {
            0 => Err(Error::EntryNotFound(query.to_string())),
            1 => Ok(exact.into_iter().next().expect("checked len == 1")),
            n => Err(Error::Ambiguous(query.to_string(), n)),
        }
    }

    /// Recycled keys whose metadata contains the word, for a refusal message. A blank word has none.
    pub fn recycled_candidates(&self, query: &str) -> Vec<RecycledKey> {
        if query.trim().is_empty() {
            return Vec::new();
        }
        self.recycled()
            .into_iter()
            .filter(|item| item.key.matches(query))
            .collect()
    }

    /// Moves a recycled key back to its group, with its UUID, secret, history and times
    /// (design D8). It is refused while a live key uses the same variable in the same scope.
    pub fn restore(&mut self, id: &str) -> Result<ApiKey> {
        self.ensure_writable()?;
        let entry_id = self
            .find_recycled_entry_id(id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        let key = {
            let entry = self
                .db
                .entry(entry_id)
                .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
            read_entry(&entry)
        };
        let variable = key.resolved_env_var();
        if self.list().iter().any(|live| {
            live.project == key.project
                && live.environment == key.environment
                && live.resolved_env_var() == variable
        }) {
            return Err(Error::Invalid(format!(
                "cannot restore: a live key already uses the variable {variable} in the same project and environment. \
                 Delete or rename that key first."
            )));
        }
        let stamp = self.stamp_for(entry_id)?;

        let mut candidate = self.db.clone();
        let previous = candidate
            .entry(entry_id)
            .and_then(|entry| entry.previous_parent().map(|group| group.id()));
        let target = previous
            .filter(|group| {
                candidate
                    .group(*group)
                    .is_some_and(|group| group.name.eq_ignore_ascii_case(GROUP_NAME))
            })
            .unwrap_or_else(|| ensure_group(&mut candidate));
        let mut entry = candidate
            .entry_mut(entry_id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        entry
            .move_to(target)
            .map_err(|error| Error::Invalid(error.to_string()))?;
        entry.times.location_changed = Some(stamp);

        self.commit(candidate)?;
        self.log_committed("restore", Some(&key), None);
        Ok(key)
    }

    /// Deletes one key for good, live or recycled, and writes the KDBX deletion record.
    /// Readers that apply the record keep the key deleted; KeePassXC 2.8.0 `merge` does
    /// not (design D9).
    pub fn purge(&mut self, id: &str) -> Result<ApiKey> {
        self.ensure_writable()?;
        let entry_id = self
            .find_entry_id(id)
            .or_else(|| self.find_recycled_entry_id(id))
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        let mut purged = self.purge_entries(&[entry_id], None)?;
        Ok(purged.remove(0))
    }

    /// The retention action: deletes for good every recycled key that moved to the
    /// Recycle Bin more than `days` days ago. It runs only when called.
    pub fn purge_recycled_older_than(&mut self, days: u32) -> Result<Vec<ApiKey>> {
        self.ensure_writable()?;
        let cutoff = crate::schedule::days_before(self.clock.now(), days);
        let ids: Vec<EntryId> = self
            .recycled()
            .iter()
            .filter(|item| item.deleted_at.is_some_and(|deleted| deleted < cutoff))
            .filter_map(|item| self.find_recycled_entry_id(&item.key.id))
            .collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.purge_entries(&ids, Some("retention"))
    }

    fn purge_entries(&mut self, ids: &[EntryId], detail: Option<&str>) -> Result<Vec<ApiKey>> {
        let mut candidate = self.db.clone();
        rebuild_attachment_references(&mut candidate);
        let mut keys = Vec::new();
        for id in ids {
            let entry = candidate
                .entry(*id)
                .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
            let key = read_entry(&entry);
            let stamp = next_stamp(self.clock.now(), &entry)?;
            candidate
                .entry_mut(*id)
                .ok_or_else(|| Error::EntryNotFound(id.to_string()))?
                .remove();
            candidate.deleted_objects.insert(id.uuid(), Some(stamp));
            keys.push(key);
        }
        self.commit(candidate)?;
        for key in &keys {
            self.log_committed("purge", Some(key), detail.or(Some("deleted for good")));
        }
        Ok(keys)
    }

    /// The earlier versions of one key, live or recycled, newest first. It names fields
    /// and never returns a value.
    pub fn history(&self, id: &str) -> Result<Vec<HistoryVersion>> {
        let entry_id = self
            .find_entry_id(id)
            .or_else(|| self.find_recycled_entry_id(id))
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        let entry = self
            .db
            .entry(entry_id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        let versions: &[Entry] = entry
            .history
            .as_ref()
            .map_or(&[], |history| history.get_entries().as_slice());
        Ok(versions
            .iter()
            .enumerate()
            .map(|(index, version)| {
                let newer: &Entry = if index == 0 {
                    &entry
                } else {
                    &versions[index - 1]
                };
                HistoryVersion {
                    index,
                    modified_at: version.times.last_modification.map(|time| time.and_utc()),
                    changed: changed_fields(version, newer),
                    secret_changed: version.get(fields::PASSWORD) != newer.get(fields::PASSWORD),
                }
            })
            .collect())
    }

    pub fn preview_import_env(
        &self,
        entries: &[(String, String)],
        project: Option<&str>,
        environment: Option<&str>,
    ) -> Result<ImportPreview> {
        Ok(self.plan_import(entries, project, environment)?.preview)
    }

    fn plan_import(
        &self,
        entries: &[(String, String)],
        project: Option<&str>,
        environment: Option<&str>,
    ) -> Result<ImportPlan> {
        require_scope(project, environment)?;
        if entries.is_empty() {
            return Err(Error::Invalid("nothing to import".into()));
        }
        let mut names = HashSet::new();
        for (name, secret) in entries {
            validate_process_var(name, secret)?;
            if !names.insert(name) {
                return Err(Error::Invalid(format!("{name:?} appears more than once")));
            }
        }

        let mut index = HashMap::new();
        for key in self.list() {
            if key.project.as_deref() != project || key.environment.as_deref() != environment {
                continue;
            }
            let name = key.resolved_env_var();
            if index.insert(name.clone(), key).is_some() {
                return Err(Error::Invalid(format!(
                    "two keys resolve to the same environment variable {name:?}; fix that before importing"
                )));
            }
        }

        let mut plan = ImportPlan {
            preview: ImportPreview::default(),
            replacements: Vec::new(),
            additions: Vec::new(),
        };
        for (name, secret) in entries {
            let (title, action) = if let Some(existing) = index.get(name) {
                let id = self
                    .find_entry_id(&existing.id)
                    .ok_or_else(|| Error::EntryNotFound(existing.id.clone()))?;
                if *self.stored_secret(id) == *secret {
                    plan.preview.unchanged += 1;
                    (existing.title.clone(), "unchanged")
                } else {
                    plan.replacements.push((id, secret.clone()));
                    plan.preview.updated += 1;
                    (existing.title.clone(), "update")
                }
            } else {
                let mut input = ApiKeyInput::new(name.clone(), secret.clone());
                input.env_var = Some(name.clone());
                input.project = project.map(str::to_string);
                input.environment = environment.map(str::to_string);
                input.validate()?;
                plan.additions.push(input);
                plan.preview.added += 1;
                (name.clone(), "add")
            };
            plan.preview.changes.push(ImportChange {
                name: name.clone(),
                title,
                action: action.into(),
            });
        }
        Ok(plan)
    }

    /// Apply one validated, exactly scoped batch, skipping unchanged secrets.
    pub fn import_env(
        &mut self,
        entries: &[(String, String)],
        project: Option<&str>,
        environment: Option<&str>,
    ) -> Result<(usize, usize)> {
        self.ensure_writable()?;
        let plan = self.plan_import(entries, project, environment)?;
        let (added, updated) = (plan.preview.added, plan.preview.updated);
        if added == 0 && updated == 0 {
            return Ok((0, 0));
        }
        let mut candidate = self.db.clone();
        let now = self.clock.now();
        for (id, secret) in plan.replacements {
            let mut entry = candidate
                .entry_mut(id)
                .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
            let before: Entry = (*entry).clone();
            let stamp = next_stamp(now, &before)?;
            keep_version(&mut entry, before);
            entry.set_protected(fields::PASSWORD, secret);
            entry.set_unprotected(META_SECRET_CHANGED, format_secret_change(stamp));
            entry.times.last_modification = Some(stamp);
        }
        for input in plan.additions {
            let group_id = ensure_group(&mut candidate);
            let mut group = candidate
                .group_mut(group_id)
                .ok_or_else(|| Error::Invalid("API Keys group disappeared".into()))?;
            let mut entry = group.add_entry();
            let id = entry.id().to_string();
            finish_new_entry(&mut entry, &input, &id, whole_second(now));
        }
        self.commit(candidate)?;
        self.log_committed(
            "import",
            None,
            Some(&format!("{added} added, {updated} updated")),
        );
        Ok((added, updated))
    }

    /// The variables that a project/environment slice would disclose: names and key IDs, no
    /// values. Refuses duplicate names, so one key can never silently shadow another at
    /// injection time, and values that cannot be injected.
    pub fn plan_env(
        &self,
        project: Option<&str>,
        environment: Option<&str>,
        all: bool,
    ) -> Result<EnvPlan> {
        if all {
            if project.is_some() || environment.is_some() {
                return Err(Error::Invalid(
                    "all cannot be combined with scope filters".into(),
                ));
            }
        } else {
            require_scope(project, environment)?;
        }
        let mut out = Vec::new();
        let mut seen = HashSet::new();

        for key in self.list() {
            if let Some(project) = project {
                if key.project.as_deref() != Some(project) {
                    continue;
                }
            }
            if let Some(environment) = environment {
                if key.environment.as_deref() != Some(environment) {
                    continue;
                }
            }

            let name = key.resolved_env_var();
            let id = self
                .find_entry_id(&key.id)
                .ok_or_else(|| Error::EntryNotFound(key.id.clone()))?;
            validate_process_var(&name, &self.stored_secret(id)).map_err(|error| {
                // A key stored before its name became reserved stays viewable and can be
                // renamed. Say how, when the name is the problem (FR-02).
                let hint = if reserved_class(&name).is_some() || !is_valid_env_var(&name) {
                    ". The key is not lost: edit it and set a different env var name to use it"
                } else {
                    ""
                };
                Error::Invalid(format!("key {:?} cannot be used: {error}{hint}", key.title))
            })?;
            if !seen.insert(name.clone()) {
                return Err(Error::Invalid(format!(
                    "two keys resolve to the same environment variable {name:?}; set AKM_ENV_VAR explicitly"
                )));
            }
            out.push((key.id, name));
        }

        out.sort_by(|a, b| a.1.cmp(&b.1));
        Ok(EnvPlan::new(self.stamp(), out))
    }

    /// Journals a request to disclose one key's secret. The proof exists only when the
    /// record was appended: a journal failure is an error and no secret can be read.
    pub fn disclose_key(
        &self,
        action: KeyDisclosure,
        key: &ApiKey,
        detail: Option<&str>,
    ) -> Result<Disclosure> {
        self.find_entry_id(&key.id)
            .ok_or_else(|| Error::EntryNotFound(key.id.clone()))?;
        self.log(action.action(), Some(key), detail)?;
        Ok(Disclosure::key(self.stamp(), action, key.id.clone()))
    }

    /// Journals a request to disclose a planned scope, before any value leaves the vault.
    pub fn disclose_env(
        &self,
        action: EnvDisclosure,
        plan: &EnvPlan,
        detail: Option<&str>,
    ) -> Result<Disclosure> {
        plan.stamp().plan_is_current(self.stamp())?;
        self.log(action.action(), None, detail)?;
        Ok(Disclosure::env(self.stamp(), action, plan.key_ids()))
    }

    /// One key's secret, for the key that `proof` names.
    pub fn secret(&self, id: &str, proof: &Disclosure) -> Result<Zeroizing<String>> {
        proof.redeem_key(self.stamp(), id)?;
        let entry = self
            .find_entry_id(id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        Ok(self.stored_secret(entry))
    }

    /// The variables of a plan, with their values. `proof` must be the record of this plan.
    pub fn env_vars(&self, plan: &EnvPlan, proof: &Disclosure) -> Result<Vec<(String, String)>> {
        proof.redeem_env(self.stamp(), plan)?;
        let mut out = Vec::with_capacity(plan.len());
        for (id, name) in plan.entries() {
            let entry = self
                .find_entry_id(id)
                .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
            let key = self.get_by_id(id)?;
            if key.resolved_env_var() != name {
                return Err(Error::Invalid(
                    "the vault changed since the selection was planned".into(),
                ));
            }
            let value = self.stored_secret(entry);
            validate_process_var(name, &value)?;
            out.push((name.to_string(), (*value).clone()));
        }
        Ok(out)
    }

    /// An input that holds a key's stored metadata and secret, for an update that keeps
    /// the secret. It is not a disclosure: the secret goes back into the vault.
    pub fn edit_input(&self, id: &str) -> Result<ApiKeyInput> {
        let key = self.get_by_id(id)?;
        let entry = self
            .find_entry_id(id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        Ok(ApiKeyInput {
            title: key.title,
            secret: Secret::new((*self.stored_secret(entry)).clone()),
            username: key.username,
            issuer: key.issuer,
            project: key.project,
            environment: key.environment,
            scopes: key.scopes,
            env_var: key.env_var,
            url: key.url,
            notes: key.notes,
            tags: key.tags,
            expires_at: key.expires_at,
            rotation_days: key.rotation_days,
        })
    }

    fn stored_secret(&self, id: EntryId) -> Zeroizing<String> {
        Zeroizing::new(
            self.db
                .entry(id)
                .and_then(|entry| entry.get(fields::PASSWORD).map(str::to_string))
                .unwrap_or_default(),
        )
    }

    pub fn expiring(&self, within_days: i64) -> Vec<ApiKey> {
        let cutoff = crate::schedule::days_after(self.clock.now(), within_days);
        let mut keys: Vec<ApiKey> = self
            .list()
            .into_iter()
            .filter(|key| key.expires_at.is_some_and(|expires| expires <= cutoff))
            .collect();
        keys.sort_by_key(|key| key.expires_at);
        keys
    }

    /// The journal of this vault. The evidence is what this `Vault` last read or wrote and
    /// its vault ID, so the journal can be confirmed without writing anything to the vault.
    pub fn audit_log(&self) -> AuditLog {
        AuditLog::new(
            self.journal.clone(),
            VaultEvidence::unlocked(
                &self.path,
                self.fingerprint.clone(),
                self.db.root().id().uuid().simple().to_string(),
            )
            .with_lineage(self.lineage.clone()),
        )
    }

    pub fn log(&self, action: &str, key: Option<&ApiKey>, detail: Option<&str>) -> Result<()> {
        let unconfirmed =
            self.audit_log()
                .append_reporting(action, key.map(|k| k.id.as_str()), detail, true)?;
        if unconfirmed {
            self.lineage_unconfirmed.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    pub fn save(&mut self) -> Result<()> {
        self.commit(self.db.clone())
    }

    fn log_committed(&mut self, action: &str, key: Option<&ApiKey>, detail: Option<&str>) {
        if let Err(error) = self.log(&format!("{action}-completed"), key, detail) {
            self.warnings.push(VaultWarning {
                kind: VaultWarningKind::Journal,
                message: format!("vault saved, but its activity entry could not be confirmed in the audit log for {action}: {error}. Do not repeat the saved operation; check its journal."),
            });
        }
    }

    fn commit(&mut self, mut candidate: Database) -> Result<()> {
        if !matches!(candidate.config.version, DatabaseVersion::KDB4(1)) {
            return Err(Error::UpgradeRequired);
        }
        let _lock = acquire_lock(&self.path)?;
        let original = self.checked_source()?;
        // The backup of the original is named now and recorded in the version that this save
        // publishes, so pruning has a durable record of every backup it may delete (FR-01,
        // core review 1, finding 1; see `backups`).
        let planned = original
            .as_deref()
            .map(|bytes| crate::backups::Backup::plan(&self.path, bytes));
        if let Some(planned) = &planned {
            let now = whole_second(self.clock.now());
            crate::backups::record(&mut candidate, &self.path, planned.clone(), now);
        }
        let mut tmp = PendingFile::temporary(&self.path)?;
        let mut not_private = tmp.not_private.take();
        candidate
            .save(tmp.file_mut(), self.key.clone())
            .map_err(|e| Error::Save(e.to_string()))?;
        tmp.sync_and_close()?;
        #[cfg(test)]
        tests::run_save_hook(tests::SaveStage::Serialized, &tmp.path, &self.path);
        let encrypted = match private_fs::read_bounded(&mut File::open(&tmp.path)?, MAX_VAULT_BYTES)
        {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::FileTooLarge => {
                return Err(Error::Save(too_large("the encrypted temporary vault")));
            }
            Err(error) => return Err(error.into()),
        };
        // The temporary file is read back from disk, so it gets the same header guard as an
        // opened vault (FR-28): a file swapped in under its name cannot choose the cost.
        crate::kdf_guard::check(&encrypted).map_err(|error| {
            Error::Save(format!(
                "encrypted temporary vault could not be reopened: {error}"
            ))
        })?;
        let verified = Database::parse(&encrypted, self.key.clone()).map_err(|error| {
            Error::Save(format!(
                "encrypted temporary vault could not be reopened: {error}"
            ))
        })?;
        if !same_database_content(&candidate, &verified) {
            return Err(Error::Save(
                "encrypted temporary vault differs from the intended database".into(),
            ));
        }
        let fingerprint = bytes_fingerprint(&encrypted);

        self.checked_source()?;
        let mut backup = if let (Some(bytes), Some(planned)) = (&original, &planned) {
            let mut backup = PendingFile::new(planned.path(&self.path))?;
            not_private = not_private.or(backup.not_private.take());
            backup.file_mut().write_all(bytes)?;
            backup.sync_and_close()?;
            if !holds_exactly(&backup.path, bytes)? {
                return Err(Error::Save("encrypted backup verification failed".into()));
            }
            Some(backup)
        } else {
            None
        };

        // The sidecar coordinates AKM writers, not arbitrary sync clients. They
        // can still race this final check and replacement; this is not a CAS.
        self.checked_source()?;
        #[cfg(test)]
        tests::run_save_hook(tests::SaveStage::BeforeReplace, &tmp.path, &self.path);
        let new_vault = self.fingerprint.is_none();
        let mut replace = || -> Result<()> {
            if new_vault {
                fs::hard_link(&tmp.path, &self.path).map_err(|error| {
                    if error.kind() == std::io::ErrorKind::AlreadyExists {
                        Error::VaultChangedOnDisk
                    } else {
                        Error::Io(error)
                    }
                })
            } else {
                // Again under the lock: a second name made after the last check (CORE-R2-05).
                refuse_second_name(&File::open(&self.path)?)?;
                fs::rename(&tmp.path, &self.path).map_err(Error::Io)
            }
        };
        match &self.commit_gate {
            Some(gate) => gate.commit(&mut replace)?,
            None => replace()?,
        }
        #[cfg(test)]
        tests::run_save_hook(tests::SaveStage::Committed, &tmp.path, &self.path);
        if let Some(previous) = self.fingerprint.replace(fingerprint) {
            self.lineage.push(previous);
            if self.lineage.len() > LINEAGE_LIMIT {
                self.lineage.remove(1);
            }
        }
        self.db = verified;
        self.state += 1;
        if let Some(backup) = &mut backup {
            backup.keep();
        }
        if let Some(bytes) = original {
            match self.refresh_legacy_backup(&bytes) {
                Ok(reason) => not_private = not_private.or(reason),
                Err(error) => self.warnings.push(VaultWarning {
                    kind: VaultWarningKind::BackupRefresh,
                    message: format!("vault saved, but .bak could not be refreshed: {error}"),
                }),
            }
            let retained = backup.as_ref().is_some_and(|backup| {
                // A pre-commit copy is not proof of post-maintenance retention.
                // Do not prune other recovery candidates if this identity is already lost.
                if !holds_exactly(&backup.path, &bytes).unwrap_or(false) {
                    return false;
                }
                if let Err(error) = crate::backups::prune(&self.db, &self.path) {
                    self.warnings.push(VaultWarning {
                        kind: VaultWarningKind::BackupPrune,
                        message: format!(
                            "vault saved, but old backups could not be pruned: {error}"
                        ),
                    });
                }
                holds_exactly(&backup.path, &bytes).unwrap_or(false)
            });
            if !retained {
                self.warnings.push(VaultWarning {
                    kind: VaultWarningKind::BackupRetention,
                    message: "vault saved, but retention of the pre-change encrypted backup could not be confirmed; inspect backups before relying on recovery".into(),
                });
            }
        }
        if let Some(reason) = not_private {
            self.warnings.push(VaultWarning {
                kind: VaultWarningKind::FilePermissions,
                message: format!(
                    "vault saved, but its files could not be made private to your account: {reason}. Anyone who can read this folder can copy the encrypted vault and its backups."
                ),
            });
        }
        Ok(())
    }

    fn checked_source(&self) -> Result<Option<Vec<u8>>> {
        let read = File::open(&self.path).and_then(|mut file| {
            let names = private_fs::link_count(&file)?;
            Ok((names, private_fs::read_bounded(&mut file, MAX_VAULT_BYTES)?))
        });
        let bytes = match read {
            Ok((names, bytes)) => {
                refuse_names(names)?;
                bytes
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return if self.fingerprint.is_none() {
                    Ok(None)
                } else {
                    Err(Error::VaultChangedOnDisk)
                };
            }
            // A fingerprint is only ever taken of a file within the bound.
            Err(error) if error.kind() == std::io::ErrorKind::FileTooLarge => {
                return Err(Error::VaultChangedOnDisk)
            }
            Err(error) => return Err(error.into()),
        };
        match &self.fingerprint {
            Some(expected) if *expected == bytes_fingerprint(&bytes) => Ok(Some(bytes)),
            _ => Err(Error::VaultChangedOnDisk),
        }
    }

    /// Returns why the new `.bak` is not private to the current user, if it is not.
    fn refresh_legacy_backup(&self, bytes: &[u8]) -> Result<Option<String>> {
        let mut tmp = PendingFile::temporary(&self.path)?;
        tmp.file_mut().write_all(bytes)?;
        tmp.sync_and_close()?;
        fs::rename(&tmp.path, self.sibling(".bak"))?;
        Ok(tmp.not_private.take())
    }

    fn stamp(&self) -> Stamp {
        Stamp::new(self.session, self.state)
    }

    fn stamp_for(&self, id: EntryId) -> Result<NaiveDateTime> {
        let entry = self
            .db
            .entry(id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        next_stamp(self.clock.now(), &entry)
    }

    fn find_entry_id(&self, id: &str) -> Option<EntryId> {
        let id = EntryId::from_uuid(Uuid::parse_str(id).ok()?);
        let root = self.db.root();
        let entry_id = root
            .groups()
            .filter(|group| group.name.eq_ignore_ascii_case(GROUP_NAME))
            .find_map(|group| {
                group
                    .entries()
                    .find(|entry| entry.id() == id)
                    .map(|entry| entry.id())
            });
        entry_id
    }

    fn find_recycled_entry_id(&self, id: &str) -> Option<EntryId> {
        let id = EntryId::from_uuid(Uuid::parse_str(id).ok()?);
        self.db
            .recycle_bin()?
            .entries()
            .find(|entry| entry.id() == id && is_api_key(entry))
            .map(|entry| entry.id())
    }

    fn sibling(&self, suffix: &str) -> PathBuf {
        let mut os = self.path.as_os_str().to_os_string();
        os.push(suffix);
        PathBuf::from(os)
    }
}

/// A time with whole seconds, the precision of KDBX.
fn whole_second(time: DateTime<Utc>) -> NaiveDateTime {
    time.naive_utc()
        .with_nanosecond(0)
        .expect("zero nanoseconds are valid")
}

/// Every time already recorded for one key: its own times and secret-change record, and the
/// same for every version in its history, whoever wrote them (design D5).
fn recorded_times(entry: &Entry) -> impl Iterator<Item = NaiveDateTime> + '_ {
    let own = |entry: &Entry| {
        [
            entry.times.creation,
            entry.times.last_modification,
            entry.times.location_changed,
            recorded_secret_change(entry),
        ]
    };
    own(entry)
        .into_iter()
        .chain(
            entry
                .history
                .iter()
                .flat_map(|history| history.get_entries())
                .flat_map(own),
        )
        .flatten()
}

/// The time of a change: now, but later than every time already recorded for the entry
/// (`recorded_times`). Time can then never run backwards for one key, also when the clock
/// moves back, when a foreign editor left a newer history version, or when two changes land
/// in one second. This is the only way AKM picks a time for a change to an existing key.
fn next_stamp(now: DateTime<Utc>, entry: &Entry) -> Result<NaiveDateTime> {
    let now = whole_second(now);
    match recorded_times(entry).max() {
        Some(latest) if now <= latest => crate::schedule::second_after(latest).ok_or_else(|| {
            Error::Invalid(
                "a time recorded for this key is too far in the future to order a change after it"
                    .into(),
            )
        }),
        _ => Ok(now),
    }
}

const SECRET_CHANGE_FORMAT: &str = "%Y-%m-%dT%H:%M:%SZ";

fn format_secret_change(time: NaiveDateTime) -> String {
    time.format(SECRET_CHANGE_FORMAT).to_string()
}

/// The time in `AKM_SECRET_CHANGED`, if the field holds one.
fn recorded_secret_change(entry: &Entry) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(entry.get(META_SECRET_CHANGED)?, SECRET_CHANGE_FORMAT).ok()
}

/// What AKM knows about the last secret change. A history version that is not older than
/// the record and holds another secret shows that a program other than AKM changed the
/// secret afterwards (AKM sets the record in the same save as the change). The record is
/// then no longer true, and no date is shown.
fn secret_change_of(entry: &Entry) -> (Option<DateTime<Utc>>, SecretChange) {
    let Some(recorded) = recorded_secret_change(entry) else {
        return (None, SecretChange::Unknown);
    };
    let current = entry.get(fields::PASSWORD);
    let changed_outside = entry
        .history
        .iter()
        .flat_map(|history| history.get_entries())
        .any(|version| {
            version
                .times
                .last_modification
                .is_some_and(|modified| modified >= recorded)
                && version.get(fields::PASSWORD) != current
        });
    if changed_outside {
        (None, SecretChange::ChangedOutside)
    } else {
        (Some(recorded.and_utc()), SecretChange::Known)
    }
}

/// Pushes the entry as it was onto its native history before a real change.
fn keep_version(entry: &mut EntryMut<'_>, before: Entry) {
    entry
        .history
        .get_or_insert_with(Default::default)
        .add_entry(before);
}

fn finish_new_entry(entry: &mut EntryMut<'_>, input: &ApiKeyInput, id: &str, stamp: NaiveDateTime) {
    write_entry(entry, input, id, stamp, stamp);
    entry.times.location_changed = Some(stamp);
    entry.times.last_access = Some(stamp);
    entry.set_unprotected(META_SECRET_CHANGED, format_secret_change(stamp));
}

/// The Recycle Bin group of the vault. A missing group, or a `RecycleBinUUID` that names
/// no group, gets a new "Recycle Bin" group in the way KeePassXC makes it.
fn ensure_recycle_bin(db: &mut Database, stamp: NaiveDateTime) -> GroupId {
    if let Some(bin) = db.recycle_bin() {
        return bin.id();
    }
    let id = {
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = "Recycle Bin".to_string();
        group.set_icon_builtin(43);
        group.enable_autotype = Some(false);
        group.enable_searching = Some(false);
        group.times.creation = Some(stamp);
        group.times.last_modification = Some(stamp);
        group.times.last_access = Some(stamp);
        group.times.location_changed = Some(stamp);
        group.id()
    };
    db.meta.recyclebin_uuid = Some(id.uuid());
    db.meta.recyclebin_enabled = Some(true);
    db.meta.recyclebin_changed = Some(stamp);
    id
}

/// Entries that AKM lists in the Recycle Bin: those with an AKM ID or whose previous
/// group was the AKM group. Other entries in the bin belong to other programs.
fn is_api_key(entry: &EntryRef<'_>) -> bool {
    entry.get(META_ID).is_some()
        || entry
            .previous_parent()
            .is_some_and(|group| group.name.eq_ignore_ascii_case(GROUP_NAME))
}

/// The names of the fields that differ between two versions of an entry. No values.
fn changed_fields(older: &Entry, newer: &Entry) -> Vec<String> {
    let mut names: Vec<String> = older
        .fields
        .keys()
        .chain(newer.fields.keys())
        .filter(|name| older.fields.get(*name) != newer.fields.get(*name))
        .cloned()
        .collect();
    if older.tags != newer.tags {
        names.push("Tags".to_string());
    }
    if older.times.expires != newer.times.expires
        || (older.times.expires == Some(true) && older.times.expiry != newer.times.expiry)
    {
        names.push("Expiry".to_string());
    }
    names.sort();
    names.dedup();
    names
}

fn ensure_group(db: &mut Database) -> GroupId {
    let existing = db
        .root()
        .groups()
        .find(|group| group.name.eq_ignore_ascii_case(GROUP_NAME))
        .map(|group| group.id());
    if let Some(id) = existing {
        return id;
    }
    let mut root = db.root_mut();
    let mut group = root.add_group();
    group.name = GROUP_NAME.to_string();
    group.id()
}

fn require_scope<'a>(
    project: Option<&'a str>,
    environment: Option<&'a str>,
) -> Result<(&'a str, &'a str)> {
    match (project, environment) {
        (Some(project), Some(environment))
            if !project.trim().is_empty() && !environment.trim().is_empty() =>
        {
            Ok((project, environment))
        }
        _ => Err(Error::Invalid(
            "a non-empty project and environment are required".into(),
        )),
    }
}

/// Security parameters used for vaults created by this app.
///
/// The `keepass` crate's defaults (1 MiB of memory, 50 iterations) are far
/// too weak. Argon2id at 64 MiB / 3 passes matches what mainstream password
/// managers use and makes offline guessing against a stolen vault expensive.
///
/// The library can only *write* KDBX 4.1. That is read by KeePassXC 2.7+ and
/// KeePass 2.48+; older clients will need to upgrade before opening the file.
fn hardened_config() -> DatabaseConfig {
    let mut config = DatabaseConfig::default();
    config.version = DatabaseVersion::KDB4(1);
    config.outer_cipher_config = OuterCipherConfig::AES256;
    config.inner_cipher_config = InnerCipherConfig::ChaCha20;
    config.compression_config = CompressionConfig::GZip;
    config.kdf_config = KdfConfig::Argon2id {
        iterations: 3,
        // Stored in bytes; the Argon2 implementation converts to KiB internally.
        memory: 64 * 1024 * 1024,
        parallelism: 4,
        version: argon2::Version::Version13,
    };
    config
}

/// The lock file of a vault. Two spellings of one file (a junction, a file symbolic link in
/// another folder, `.` and `..`, another letter case) must use one lock (P2R2-07, core review
/// 1, finding 9). When the vault exists, the name is its canonical path, so a link and its
/// target share one lock. When it does not exist yet, the name is the canonical folder and the
/// written file name. If the folder cannot be made canonical either, the lock is named from
/// the path as it was written, which is what AKM did before.
fn lock_path(vault_path: &Path) -> PathBuf {
    lock_name(vault_path, &|path| fs::canonicalize(path))
}

type Resolve<'a> = &'a dyn Fn(&Path) -> std::io::Result<PathBuf>;

fn lock_name(vault_path: &Path, resolve: Resolve<'_>) -> PathBuf {
    let with_lock = |path: &Path| {
        let mut os = path.as_os_str().to_os_string();
        os.push(".lock");
        PathBuf::from(os)
    };
    if let Ok(real) = resolve(vault_path) {
        return with_lock(&real);
    }
    let Some(name) = vault_path.file_name() else {
        return with_lock(vault_path);
    };
    let folder = vault_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    match resolve(folder) {
        Ok(folder) => with_lock(&folder.join(name)),
        Err(_) => with_lock(vault_path),
    }
}

fn acquire_lock(vault_path: &Path) -> Result<File> {
    if let Some(parent) = vault_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let lock_path = lock_path(vault_path);

    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)?;

    file.try_lock_exclusive().map_err(|_| Error::Locked)?;
    Ok(file)
}

fn bytes_fingerprint(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[derive(PartialEq, Eq)]
struct AttachmentSnapshot {
    data: Vec<Value<Vec<u8>>>,
    references: HashMap<(EntryId, Option<usize>, String), usize>,
}

fn attachment_snapshot(db: &Database) -> Option<AttachmentSnapshot> {
    let mut attachments: Vec<_> = db.iter_all_attachments().collect();
    attachments.sort_by_key(|attachment| attachment.id().id());
    let indices: HashMap<_, _> = attachments
        .iter()
        .enumerate()
        .map(|(index, attachment)| (attachment.id(), index))
        .collect();
    let mut snapshot = AttachmentSnapshot {
        data: attachments
            .iter()
            .map(|attachment| attachment.data.clone())
            .collect(),
        references: HashMap::new(),
    };
    let mut record = |entry: &EntryRef<'_>, history: Option<usize>| -> Option<()> {
        for (name, attachment) in entry.attachments_named() {
            let index = *indices.get(&attachment.id())?;
            snapshot
                .references
                .insert((entry.id(), history, name.to_string()), index);
        }
        Some(())
    };
    for entry in db.iter_all_entries() {
        record(&entry, None)?;
        for index in 0..entry
            .history
            .as_ref()
            .map_or(0, |history| history.get_entries().len())
        {
            record(&entry.historical(index)?, Some(index))?;
        }
    }
    Some(snapshot)
}

fn same_database_content(intended: &Database, verified: &Database) -> bool {
    // Seeds are not stored in Database, and AKM timestamps use KDBX's second
    // precision. Full equality is otherwise exact, including protected values.
    if intended == verified {
        return true;
    }
    // KDBX stores attachments by position. Deletion can leave sparse in-memory
    // IDs, which the serializer renumbers. Compare every binary and named use
    // (including history and shared references) before ignoring only those IDs.
    match (attachment_snapshot(intended), attachment_snapshot(verified)) {
        (Some(expected), Some(actual)) if expected == actual => {}
        _ => return false,
    }
    // The library keeps the group that an entry had in each old history version when the
    // entry moves, and the parser gives every version the current group. KDBX does not
    // store that group for a version, so compare the versions field by field without it,
    // then compare everything else with the histories removed (design D6).
    if !same_histories(intended, verified) {
        return false;
    }
    let mut intended = intended.clone();
    let mut verified = verified.clone();
    intended.foreach_entry_mut(|mut entry| entry.history = None);
    verified.foreach_entry_mut(|mut entry| entry.history = None);
    intended.remove_unused_attachments();
    verified.remove_unused_attachments();
    intended.foreach_attachment_mut(|attachment| attachment.remove());
    verified.foreach_attachment_mut(|attachment| attachment.remove());
    intended == verified
}

fn same_histories(intended: &Database, verified: &Database) -> bool {
    intended.iter_all_entries().all(|entry| {
        let Some(other) = verified.entry(entry.id()) else {
            return false;
        };
        let count = |entry: &EntryRef<'_>| {
            entry
                .history
                .as_ref()
                .map_or(0, |history| history.get_entries().len())
        };
        count(&entry) == count(&other)
            && (0..count(&entry)).all(|index| {
                match (entry.historical(index), other.historical(index)) {
                    (Some(a), Some(b)) => same_version(&a, &b),
                    _ => false,
                }
            })
    })
}

/// Every stored property of one history version, except the group it was in.
fn same_version(a: &EntryRef<'_>, b: &EntryRef<'_>) -> bool {
    let attachments = |entry: &EntryRef<'_>| {
        let mut named: Vec<_> = entry
            .attachments_named()
            .map(|(name, attachment)| (name.to_string(), attachment.data.clone()))
            .collect();
        named.sort_by(|x, y| x.0.cmp(&y.0));
        named
    };
    a.id() == b.id()
        && a.fields == b.fields
        && a.autotype == b.autotype
        && a.tags == b.tags
        && a.times == b.times
        && a.custom_data == b.custom_data
        && a.foreground_color == b.foreground_color
        && a.background_color == b.background_color
        && a.override_url == b.override_url
        && a.quality_check == b.quality_check
        && a.icon() == b.icon()
        && a.previous_parent().map(|group| group.id())
            == b.previous_parent().map(|group| group.id())
        && attachments(a) == attachments(b)
}

fn rebuild_attachment_references(db: &mut Database) {
    // The parser does not populate attachment back-references. Entry removal
    // needs them to retain binaries shared with other entries or history. Use
    // the library's rebuild on a copy so orphan binaries are not discarded.
    let mut indexed = db.clone();
    indexed.remove_unused_attachments();
    db.foreach_attachment_mut(|mut attachment| {
        if let Some(indexed) = indexed.attachment(attachment.id()) {
            *attachment = (*indexed).clone();
        }
    });
}

struct PendingFile {
    path: PathBuf,
    file: Option<File>,
    remove_on_drop: bool,
    /// Why the file is not private to the current user, if it is not (FR-05).
    not_private: Option<String>,
}

impl PendingFile {
    /// Every file that AKM writes beside a vault (the temporary vault, dated backups and
    /// the `.bak` copy) starts here, so each one gets the private descriptor.
    fn new(path: PathBuf) -> Result<Self> {
        let (file, not_private) = crate::private_fs::create_new_private(&path)?;
        Ok(Self {
            path,
            file: Some(file),
            remove_on_drop: true,
            not_private,
        })
    }

    fn temporary(vault_path: &Path) -> Result<Self> {
        let mut path = vault_path.as_os_str().to_os_string();
        path.push(format!(".tmp-{}", Uuid::new_v4()));
        Self::new(PathBuf::from(path))
    }

    fn file_mut(&mut self) -> &mut File {
        self.file.as_mut().expect("pending file is still open")
    }

    fn sync_and_close(&mut self) -> Result<()> {
        self.file_mut().sync_all()?;
        self.file.take();
        Ok(())
    }

    fn keep(&mut self) {
        self.remove_on_drop = false;
    }
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        self.file.take();
        if self.remove_on_drop {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn write_entry(
    entry: &mut Entry,
    input: &ApiKeyInput,
    id: &str,
    created: NaiveDateTime,
    updated: NaiveDateTime,
) {
    entry.set_unprotected(fields::TITLE, input.title.trim());
    entry.set_unprotected(
        fields::USERNAME,
        input.username.as_deref().unwrap_or("api-key"),
    );
    entry.set_protected(fields::PASSWORD, input.secret.as_str());

    set_or_remove(entry, fields::URL, input.url.as_deref());
    set_or_remove(entry, fields::NOTES, input.notes.as_deref());

    entry.tags = input.tags.clone();
    entry.times.creation = Some(created);
    entry.times.last_modification = Some(updated);

    match input.expires_at {
        Some(expires) => {
            entry.times.expires = Some(true);
            entry.times.expiry = expires
                .naive_utc()
                .with_nanosecond(0)
                .expect("zero nanoseconds are valid");
        }
        None => entry.times.expires = Some(false),
    }

    entry.set_unprotected(META_ID, id);
    set_or_remove(entry, META_ISSUER, input.issuer.as_deref());
    set_or_remove(entry, META_PROJECT, input.project.as_deref());
    set_or_remove(entry, META_ENVIRONMENT, input.environment.as_deref());
    entry.set_unprotected(
        META_SCOPES,
        serde_json::to_string(&input.scopes).unwrap_or_else(|_| "[]".to_string()),
    );
    set_or_remove(entry, META_ENV_VAR, input.env_var.as_deref());
    set_or_remove(
        entry,
        META_ROTATION_DAYS,
        input.rotation_days.map(|days| days.to_string()).as_deref(),
    );
}

fn set_or_remove(entry: &mut Entry, key: &str, value: Option<&str>) {
    match value {
        Some(value) if !value.is_empty() => entry.set_unprotected(key, value),
        _ => {
            entry.fields.remove(key);
        }
    }
}

fn read_entry(entry: &Entry) -> ApiKey {
    let (secret_changed_at, secret_change) = secret_change_of(entry);
    ApiKey {
        id: entry.id().to_string(),
        title: entry.get(fields::TITLE).unwrap_or_default().to_string(),
        username: owned(entry.get(fields::USERNAME)).filter(|v| v != "api-key"),
        issuer: owned(entry.get(META_ISSUER)),
        project: owned(entry.get(META_PROJECT)),
        environment: owned(entry.get(META_ENVIRONMENT)),
        scopes: entry
            .get(META_SCOPES)
            .and_then(|raw| serde_json::from_str(raw).ok())
            .unwrap_or_default(),
        env_var: entry.get(META_ENV_VAR).map(str::to_string),
        url: owned(entry.get(fields::URL)),
        notes: owned(entry.get(fields::NOTES)),
        tags: entry.tags.clone(),
        expires_at: (entry.times.expires == Some(true)).then(|| entry.times.expiry.and_utc()),
        rotation_days: entry
            .get(META_ROTATION_DAYS)
            .and_then(|raw| raw.parse().ok()),
        created_at: entry.times.creation.map(|ts| ts.and_utc()),
        updated_at: entry.times.last_modification.map(|ts| ts.and_utc()),
        secret_changed_at,
        secret_change,
        history_count: entry.history.as_ref().map_or(0, |h| h.get_entries().len()),
    }
}

fn owned(value: Option<&str>) -> Option<String> {
    value.filter(|value| !value.is_empty()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    const PASSWORD: &str = "correct horse battery staple";

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(super) enum SaveStage {
        Serialized,
        BeforeReplace,
        Committed,
    }

    type SaveHook = (SaveStage, Box<dyn FnOnce(&Path, &Path)>);
    thread_local! {
        static SAVE_HOOK: RefCell<Option<SaveHook>> = const { RefCell::new(None) };
    }

    fn on_save(stage: SaveStage, hook: impl FnOnce(&Path, &Path) + 'static) {
        SAVE_HOOK.with(|slot| *slot.borrow_mut() = Some((stage, Box::new(hook))));
    }

    pub(super) fn run_save_hook(stage: SaveStage, temp: &Path, vault: &Path) {
        let hook = SAVE_HOOK.with(|slot| {
            let mut slot = slot.borrow_mut();
            if slot.as_ref().is_some_and(|(wanted, _)| *wanted == stage) {
                slot.take()
            } else {
                None
            }
        });
        if let Some((_, hook)) = hook {
            hook(temp, vault);
        }
    }

    #[test]
    fn corrupt_temporary_saves_are_refused_without_changing_the_snapshot() {
        for corruption in [
            "truncated",
            "secret",
            "protection",
            "group",
            "history",
            "attachment",
            "meta",
            "custom",
            "uuid",
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("vault.kdbx");
            fs::write(&path, include_bytes!("../tests/fixtures/kdbx40.kdbx")).unwrap();
            let mut vault = Vault::open(&path, PASSWORD).unwrap();
            vault.upgrade().unwrap();
            let original = vault.list();
            let bytes = fs::read(&path).unwrap();
            on_save(SaveStage::Serialized, move |temp, _| {
                if corruption == "truncated" {
                    fs::write(temp, b"truncated encrypted output").unwrap();
                    return;
                }
                let key = DatabaseKey::new().with_password(PASSWORD);
                let mut db = Database::parse(&fs::read(temp).unwrap(), key.clone()).unwrap();
                match corruption {
                    "secret" => db.foreach_entry_mut(|mut entry| {
                        entry.set_protected(fields::PASSWORD, "corrupt-secret")
                    }),
                    "protection" => db.foreach_entry_mut(|mut entry| {
                        let secret = entry.get(fields::PASSWORD).unwrap().to_owned();
                        entry.set_unprotected(fields::PASSWORD, secret);
                    }),
                    "group" => db.root_mut().name = "wrong group name".into(),
                    "history" => db.foreach_entry_mut(|mut entry| entry.history = None),
                    "attachment" => db.foreach_attachment_mut(|mut attachment| {
                        attachment.data =
                            keepass::db::Value::protected(b"corrupt attachment".to_vec())
                    }),
                    "meta" => db.meta.database_description = Some("corrupt metadata".into()),
                    "custom" => db.meta.custom_data.clear(),
                    "uuid" => {
                        let id = db.iter_all_entries().next().unwrap().id();
                        db.entry_mut(id).unwrap().remove();
                        db.root_mut().add_entry().set_unprotected(
                            fields::TITLE,
                            "replacement entry with a different UUID",
                        );
                    }
                    _ => unreachable!(),
                }
                db.save(&mut File::create(temp).unwrap(), key).unwrap();
            });
            assert!(
                matches!(
                    vault.add(ApiKeyInput::new("Candidate", "candidate-secret")),
                    Err(Error::Save(_))
                ),
                "accepted {corruption}"
            );
            assert_eq!(vault.list(), original, "{corruption}");
            assert_eq!(fs::read(&path).unwrap(), bytes, "{corruption}");
            assert!(!fs::read_dir(dir.path()).unwrap().any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".tmp")));
        }
    }

    fn round_trip(db: &Database) -> Database {
        let key = DatabaseKey::new().with_password(PASSWORD);
        let mut bytes = Vec::new();
        db.save(&mut bytes, key.clone()).unwrap();
        Database::parse(&bytes, key).unwrap()
    }

    /// A database with one entry that has two history versions and an attachment in the
    /// oldest one, plus a second group.
    fn database_with_history() -> (Database, EntryId, GroupId) {
        let mut db = Database::with_config(hardened_config());
        db.config.kdf_config = KdfConfig::Aes { rounds: 10 };
        let target = db.root_mut().add_group().id();
        let mut root = db.root_mut();
        let mut entry = root.add_entry();
        entry.set_unprotected(fields::TITLE, "Moves");
        entry.set_protected(fields::PASSWORD, "version-0");
        entry.add_attachment("old.txt", Value::protected(b"old bytes".to_vec()));
        let id = entry.id();
        for version in 1..=2 {
            let before: Entry = (*entry).clone();
            entry.history.get_or_insert_default().add_entry(before);
            entry.set_protected(fields::PASSWORD, format!("version-{version}"));
        }
        (db, id, target)
    }

    #[test]
    fn a_moved_entry_with_history_passes_the_content_comparison() {
        let (mut db, id, target) = database_with_history();
        db.entry_mut(id).unwrap().move_to(target).unwrap();
        let parsed = round_trip(&db);
        assert!(
            db != parsed,
            "the library keeps the old group in the versions, so strict equality fails"
        );
        assert!(same_database_content(&db, &parsed));
    }

    #[test]
    fn the_content_comparison_still_rejects_a_changed_history_version() {
        for change in ["secret", "time", "field", "dropped", "attachment"] {
            let (mut db, id, target) = database_with_history();
            db.entry_mut(id).unwrap().move_to(target).unwrap();
            let parsed = round_trip(&db);
            let mut altered = db.clone();
            let mut entry = altered.entry_mut(id).unwrap();
            let versions: Vec<Entry> = entry.history.as_ref().unwrap().get_entries().clone();
            let mut history = keepass::db::History::default();
            let count = versions.len();
            for (index, version) in versions.into_iter().enumerate().rev() {
                let mut version = version;
                match change {
                    "secret" if index == 0 => version.set_protected(fields::PASSWORD, "forged"),
                    "time" if index == 1 => version.times.last_modification = Some(Times::epoch()),
                    "field" if index == 0 => version.set_unprotected("Extra", "1"),
                    "dropped" if index == count - 1 => continue,
                    _ => {}
                }
                history.add_entry(version);
            }
            entry.history = Some(history);
            if change == "attachment" {
                altered.foreach_attachment_mut(|mut attachment| {
                    attachment.data = Value::protected(b"forged bytes".to_vec())
                });
            }
            assert!(
                !same_database_content(&altered, &parsed),
                "accepted a changed history: {change}"
            );
        }
    }

    fn fixture_vault() -> (tempfile::TempDir, PathBuf, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.kdbx");
        fs::write(&path, include_bytes!("../tests/fixtures/kdbx40.kdbx")).unwrap();
        let mut vault = Vault::open(&path, PASSWORD).unwrap();
        vault.upgrade().unwrap();
        (dir, path, vault)
    }

    /// FR-05: the vault, the `.bak` copy and every dated backup that a save writes carry
    /// the private descriptor (owner and one allow entry: the current user).
    #[cfg(windows)]
    #[test]
    fn a_save_leaves_the_vault_and_every_backup_private_to_the_current_user() {
        let (dir, path, mut vault) = fixture_vault();
        vault.take_warning_records();
        vault
            .add(ApiKeyInput::new("Fake first", "fake-first-value"))
            .unwrap();
        vault
            .add(ApiKeyInput::new("Fake second", "fake-second-value"))
            .unwrap();
        let mut checked = 0;
        for entry in fs::read_dir(dir.path()).unwrap() {
            let file = entry.unwrap().path();
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            if name.ends_with(".lock") {
                continue;
            }
            crate::private_fs::check_private(&file)
                .unwrap_or_else(|error| panic!("{name} is not private: {error}"));
            checked += 1;
        }
        assert!(path.exists());
        assert!(dir.path().join("vault.kdbx.bak").exists());
        // The vault, the .bak copy and at least the dated backups of the three saves.
        assert!(checked >= 5, "only {checked} files were checked");
        assert!(
            vault.take_warning_records().is_empty(),
            "a normal save warns about nothing"
        );
    }

    /// FV-21: on a path longer than 260 characters every file of a save is private too. The
    /// folder grants Everyone read access to what it holds, so a file that took the folder's
    /// permissions would be readable by Everyone. A vault path of 130 characters gives dated
    /// backups of 267 characters, as in the reviewer's run.
    #[cfg(windows)]
    #[test]
    fn a_save_on_a_long_path_in_a_shared_folder_keeps_every_file_private() {
        use std::os::windows::ffi::OsStrExt;
        let units = |path: &Path| path.as_os_str().encode_wide().count();
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("shared");
        fs::create_dir(&shared).unwrap();
        let granted = std::process::Command::new("icacls")
            .arg(&shared)
            .args(["/grant", "*S-1-1-0:(OI)(CI)(R)"])
            .output()
            .unwrap();
        assert!(granted.status.success(), "{granted:?}");
        let stem = 130 - units(&shared) - 1 - ".kdbx".len();
        let path = shared.join(format!("{}.kdbx", "v".repeat(stem)));
        assert_eq!(units(&path), 130);
        fs::write(&path, include_bytes!("../tests/fixtures/kdbx40.kdbx")).unwrap();
        let mut vault = Vault::open(&path, PASSWORD).unwrap();
        vault.upgrade().unwrap();
        vault
            .add(ApiKeyInput::new("Fake first", "fake-first-value"))
            .unwrap();
        vault
            .add(ApiKeyInput::new("Fake second", "fake-second-value"))
            .unwrap();
        let warnings = vault.take_warning_records();
        assert!(
            warnings
                .iter()
                .all(|warning| warning.kind != VaultWarningKind::FilePermissions),
            "{warnings:?}"
        );
        let (mut checked, mut long) = (0, 0);
        for entry in fs::read_dir(&shared).unwrap() {
            let file = entry.unwrap().path();
            let name = file.file_name().unwrap().to_string_lossy().into_owned();
            if name.ends_with(".lock") {
                continue;
            }
            crate::private_fs::check_private(&file)
                .unwrap_or_else(|error| panic!("{name} is not private: {error}"));
            checked += 1;
            if units(&file) > 260 {
                long += 1;
            }
        }
        // The vault, the .bak copy and the dated backups of the three saves.
        assert_eq!((checked, long), (5, 3));
    }

    /// FV-21: on a volume that keeps Windows permissions, a failed private creation fails the
    /// save. A plain file there would take the folder's permissions. Nothing is written: the
    /// vault keeps its bytes and the folder gets no new file.
    #[cfg(windows)]
    #[test]
    fn a_failed_private_creation_on_a_volume_that_keeps_permissions_fails_the_save() {
        use crate::private_fs::testing::{set, Mode};
        let (dir, path, mut vault) = fixture_vault();
        let names = |dir: &Path| {
            let mut names: Vec<_> = fs::read_dir(dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect();
            names.sort();
            names
        };
        let (before, files) = (fs::read(&path).unwrap(), names(dir.path()));
        set(Mode::CreateFails);
        let refused = vault.add(ApiKeyInput::new("Fake refused", "fake-refused-value"));
        set(Mode::Normal);
        let error = refused.expect_err("the save is refused").to_string();
        assert!(error.contains("private to your account"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(names(dir.path()), files);
        let reopened = Vault::open(&path, PASSWORD).unwrap();
        assert!(reopened
            .list()
            .iter()
            .all(|key| key.title != "Fake refused"));
    }

    /// FR-05: a file system that cannot keep private permissions does not stop the save.
    /// The save succeeds and a typed warning says why the files are not private. Both ways
    /// to fail are covered: the create call fails on a volume that reports no persistent
    /// permissions, and the file system ignores the descriptor (the warning then comes from
    /// the real permissions check).
    #[cfg(windows)]
    #[test]
    fn a_save_on_a_file_system_without_private_files_succeeds_and_warns() {
        use crate::private_fs::testing::{set, Mode};
        for mode in [
            Mode::CreateFailsOnVolumeWithoutPermissions,
            Mode::DescriptorIgnored,
        ] {
            let (dir, path, mut vault) = fixture_vault();
            vault.take_warning_records();
            set(mode);
            let added = vault
                .add(ApiKeyInput::new("Fake plain", "fake-plain-value"))
                .unwrap();
            set(Mode::Normal);
            let warnings = vault.take_warning_records();
            let warning = warnings
                .iter()
                .find(|warning| warning.kind == VaultWarningKind::FilePermissions)
                .expect("the save warns");
            assert!(
                warning.message.contains("vault saved"),
                "{}",
                warning.message
            );
            assert!(warning.message.contains("private"), "{}", warning.message);
            assert!(!warning.message.contains("fake-plain-value"));
            let reopened = Vault::open(&path, PASSWORD).unwrap();
            assert_eq!(reopened.get_by_id(&added.id).unwrap().title, "Fake plain");
            drop(dir);
        }
    }

    /// FR-28: a vault whose header asks for more key derivation than the limits is refused
    /// before the key is derived, on open and on reload, with the limit in the message. The
    /// files hold only a header, so the test allocates nothing for the cost they name.
    #[test]
    fn a_vault_that_asks_for_too_much_key_derivation_is_refused_before_it_is_derived() {
        use crate::kdf_guard::crafted;
        let started = std::time::Instant::now();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.kdbx");
        let cases = [
            (crafted::argon2(true, 1 << 40, 3, 4), "4 GiB"),
            (crafted::argon2(false, 64 << 20, u64::MAX, 4), "100000"),
            (crafted::argon2(true, 64 << 20, 3, u32::MAX), "256"),
            (crafted::aes4(u64::MAX), "2000000000"),
            (crafted::aes3(3_000_000_000), "2000000000"),
        ];
        for (bytes, limit) in cases {
            fs::write(&path, &bytes).unwrap();
            let text = Vault::open(&path, PASSWORD)
                .err()
                .expect("the vault is refused")
                .to_string();
            assert!(text.contains(limit), "{limit}: {text}");
            assert!(!text.contains("wrong password"), "{text}");
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "the refusal must come before any key derivation"
        );

        // The same on reload of a vault that is already open.
        let (_dir, path, mut vault) = fixture_vault();
        fs::write(&path, crafted::argon2(true, 1 << 40, 3, 4)).unwrap();
        let text = vault.reload().unwrap_err().to_string();
        assert!(text.contains("4 GiB"), "{text}");
    }

    /// FR-28 sweep (core review 1, finding 3): the save reads its temporary vault back from
    /// disk. A file swapped in under that name gets the same header guard as an opened vault:
    /// here a field one byte short, which the reader would panic on, is a save error.
    #[test]
    fn a_swapped_temporary_vault_gets_the_header_guard_before_it_is_read_back() {
        let (_dir, path, mut vault) = fixture_vault();
        let before = fs::read(&path).unwrap();
        on_save(SaveStage::Serialized, |temp, _| {
            let bytes = fs::read(temp).unwrap();
            let short = crate::kdf_guard::rewrite::kdf_field(&bytes, "R", 7);
            fs::write(temp, short).unwrap();
        });
        let error = vault
            .add(ApiKeyInput::new("Fake swapped", "fake-swapped-value"))
            .unwrap_err();
        assert!(matches!(error, Error::Save(_)), "{error}");
        assert!(error.to_string().contains("stops before"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), before, "the vault is unchanged");
    }

    /// FR-28: a header that is not readable keeps the error that it had before.
    #[test]
    fn a_malformed_header_keeps_its_existing_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.kdbx");
        let good = crate::kdf_guard::crafted::argon2(false, 64 << 20, 3, 4);
        for bytes in [
            Vec::new(),
            b"not a vault".to_vec(),
            good[..good.len() / 2].to_vec(),
            good,
        ] {
            fs::write(&path, &bytes).unwrap();
            let text = Vault::open(&path, PASSWORD)
                .err()
                .expect("the file is not a vault")
                .to_string();
            assert!(text.starts_with("could not open vault"), "{text}");
        }
    }

    /// FR-28: usual settings still open: the KeePassXC-made fixture and AKM's own vaults.
    #[test]
    fn vaults_at_usual_key_derivation_settings_still_open() {
        let (_dir, path, _vault) = fixture_vault();
        Vault::open(&path, PASSWORD).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join("own.kdbx");
        Vault::create(&own, PASSWORD, None).unwrap();
        Vault::open(&own, PASSWORD).unwrap();
    }

    /// P2R2-07: two spellings of one vault file use one lock: a junction to the folder, a
    /// `.` and `..` detour, and another letter case.
    #[cfg(windows)]
    #[test]
    fn two_spellings_of_one_vault_file_use_one_lock() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("Real Folder");
        fs::create_dir(&real).unwrap();
        let vault = real.join("Vault.kdbx");
        fs::write(&vault, b"fake vault bytes").unwrap();
        let link = dir.path().join("link");
        let made = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&link)
            .arg(&real)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");

        let spellings = [
            vault.clone(),
            link.join("Vault.kdbx"),
            real.join(".").join("Vault.kdbx"),
            real.join("..").join("Real Folder").join("Vault.kdbx"),
            dir.path().join("REAL FOLDER").join("VAULT.KDBX"),
            link.join("vault.kdbx"),
        ];
        let first = lock_path(&spellings[0]);
        for spelling in &spellings {
            assert_eq!(lock_path(spelling), first, "{}", spelling.display());
        }

        // And the lock itself: held through one spelling, refused through every other.
        let held = acquire_lock(&spellings[1]).unwrap();
        for spelling in &spellings {
            assert!(
                matches!(acquire_lock(spelling), Err(Error::Locked)),
                "{} was not locked out",
                spelling.display()
            );
        }
        drop(held);
        acquire_lock(&spellings[2]).expect("the lock is free again");
        let _ = fs::remove_dir(&link);
    }

    /// Core review 1, finding 9 (P2R2-07): a file symbolic link in another folder. The lock is
    /// named from the canonical path of the file when the file exists, so `B\link.kdbx`, which
    /// points at `A\vault.kdbx`, shares `A\vault.kdbx.lock` with its target. A file that does
    /// not exist is named from its canonical folder and the written name. The resolver is
    /// injected, so this test needs no privilege to make a link.
    #[cfg(windows)]
    #[test]
    fn a_file_symlink_and_its_target_get_one_lock_name() {
        let resolve = |path: &Path| -> std::io::Result<PathBuf> {
            let real = match path.to_str().unwrap() {
                r"C:\B\link.kdbx" | r"C:\A\vault.kdbx" | r"C:\J\link.kdbx" => {
                    r"\\?\C:\A\vault.kdbx"
                }
                r"C:\A" => r"\\?\C:\A",
                r"C:\B" | r"C:\J" => r"\\?\C:\B",
                _ => return Err(std::io::ErrorKind::NotFound.into()),
            };
            Ok(PathBuf::from(real))
        };
        let target = PathBuf::from(r"\\?\C:\A\vault.kdbx.lock");
        for spelling in [r"C:\B\link.kdbx", r"C:\A\vault.kdbx", r"C:\J\link.kdbx"] {
            assert_eq!(
                lock_name(Path::new(spelling), &resolve),
                target,
                "{spelling}"
            );
        }
        for spelling in [r"C:\B\new.kdbx", r"C:\J\new.kdbx"] {
            assert_eq!(
                lock_name(Path::new(spelling), &resolve),
                PathBuf::from(r"\\?\C:\B\new.kdbx.lock"),
                "{spelling}"
            );
        }
        assert_eq!(
            lock_name(Path::new(r"Z:\no\vault.kdbx"), &resolve),
            PathBuf::from(r"Z:\no\vault.kdbx.lock")
        );
    }

    /// Core review 1, finding 9, on the real file system: the lock held through a file symbolic
    /// link refuses its target, and the other way round. Making a file link needs the
    /// symbolic-link privilege or Developer Mode; without it Windows returns error 1314, and
    /// this test prints that it did not run and passes.
    #[cfg(windows)]
    #[test]
    fn a_lock_held_through_a_file_symlink_refuses_its_target_and_back() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("A"), dir.path().join("B"));
        fs::create_dir(&a).unwrap();
        fs::create_dir(&b).unwrap();
        let target = a.join("vault.kdbx");
        fs::write(&target, b"fake vault bytes").unwrap();
        let link = b.join("link.kdbx");
        match std::os::windows::fs::symlink_file(&target, &link) {
            Ok(()) => {}
            Err(error) if error.raw_os_error() == Some(1314) => {
                eprintln!(
                    "NOT RUN: a_lock_held_through_a_file_symlink_refuses_its_target_and_back: Windows error 1314, no privilege to make a file symbolic link"
                );
                return;
            }
            Err(error) => panic!("could not make the file link: {error}"),
        }
        let held = acquire_lock(&link).unwrap();
        assert!(matches!(acquire_lock(&target), Err(Error::Locked)));
        drop(held);
        let held = acquire_lock(&target).unwrap();
        assert!(matches!(acquire_lock(&link), Err(Error::Locked)));
        drop(held);
    }

    /// Core review 1, finding 9: a vault that does not exist yet. While one spelling holds the
    /// lock (a junction to the folder, or another letter case), `create` through another
    /// spelling is refused and writes nothing.
    #[cfg(windows)]
    #[test]
    fn a_vault_is_not_created_through_one_spelling_while_another_holds_the_lock() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("Real");
        fs::create_dir(&real).unwrap();
        let junction = dir.path().join("junction");
        let made = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&junction)
            .arg(&real)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        for (held, created) in [
            (junction.join("new.kdbx"), real.join("new.kdbx")),
            (real.join("NEW.KDBX"), junction.join("new.kdbx")),
        ] {
            let lock = acquire_lock(&held).unwrap();
            let refused = Vault::create(&created, PASSWORD, None);
            assert!(
                matches!(refused, Err(Error::Locked)),
                "{} was created while {} held the lock",
                created.display(),
                held.display()
            );
            assert!(!real.join("new.kdbx").exists(), "nothing was written");
            drop(lock);
        }
        let _ = fs::remove_dir(&junction);
    }

    /// P2R2-07: a vault that does not exist yet still gets a lock beside its folder, and a
    /// path that cannot be made canonical keeps the old name.
    #[test]
    fn the_lock_of_a_new_vault_and_of_an_unresolvable_path_is_named_as_before() {
        let dir = tempfile::tempdir().unwrap();
        let new = dir.path().join("sub").join("new.kdbx");
        let lock = acquire_lock(&new).unwrap();
        assert!(dir.path().join("sub").exists());
        let named = lock_path(&new);
        assert!(
            named.to_string_lossy().ends_with("new.kdbx.lock"),
            "{named:?}"
        );
        assert!(named.exists());
        drop(lock);
        // No folder to resolve: the name is the written path plus `.lock`.
        let missing = Path::new(r"Z:\no\such\folder\vault.kdbx");
        assert_eq!(
            lock_path(missing),
            PathBuf::from(r"Z:\no\such\folder\vault.kdbx.lock")
        );
    }

    #[test]
    fn protected_original_postcommit_loss_is_a_typed_warning_not_an_unsaved_error() {
        for remove in [true, false] {
            let (_dir, path, mut vault) = fixture_vault();
            let original = fs::read(&path).unwrap();
            on_save(SaveStage::Committed, move |_, vault_path| {
                for entry in fs::read_dir(vault_path.parent().unwrap()).unwrap() {
                    let p = entry.unwrap().path();
                    if p.file_name().unwrap().to_string_lossy().contains(".bak.")
                        && fs::read(&p).ok().as_ref() == Some(&original)
                    {
                        if remove {
                            fs::remove_file(&p).unwrap();
                        } else {
                            fs::write(&p, b"fake externally replaced backup").unwrap();
                        }
                    }
                }
            });
            let added = vault
                .add(ApiKeyInput::new("Fake committed", "fake-committed-value"))
                .unwrap();
            let reopened = Vault::open(&path, PASSWORD).unwrap();
            let found = reopened.get_by_id(&added.id).unwrap();
            let proof = reopened
                .disclose_key(KeyDisclosure::Reveal, &found, None)
                .unwrap();
            assert_eq!(
                *reopened.secret(&added.id, &proof).unwrap(),
                "fake-committed-value"
            );
            let warnings = vault.take_warning_records();
            assert!(warnings.iter().any(|w| format!("{:?}", w.kind) == "BackupRetention"), "postcommit retention uncertainty must be typed, not ignored or reported as an unsaved write");
        }
    }

    #[test]
    fn source_replaced_while_serializing_is_not_overwritten_or_backed_up_as_ours() {
        let (dir, path, mut vault) = fixture_vault();
        let original = vault.list();
        let backup = fs::read(vault.sibling(".bak")).unwrap();
        on_save(SaveStage::Serialized, |_, source| {
            fs::write(source, b"external replacement").unwrap()
        });
        assert!(matches!(
            vault.add(ApiKeyInput::new("Candidate", "fake-secret")),
            Err(Error::VaultChangedOnDisk)
        ));
        assert_eq!(vault.list(), original);
        assert_eq!(fs::read(&path).unwrap(), b"external replacement");
        assert_eq!(fs::read(vault.sibling(".bak")).unwrap(), backup);
        assert!(!fs::read_dir(dir.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp")));
    }

    #[test]
    fn atomic_create_does_not_clobber_a_file_appearing_after_the_last_check() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.kdbx");
        on_save(SaveStage::BeforeReplace, |_, source| {
            fs::write(source, b"appeared during create").unwrap()
        });
        assert!(matches!(
            Vault::create(&path, PASSWORD, None),
            Err(Error::VaultChangedOnDisk)
        ));
        assert_eq!(fs::read(&path).unwrap(), b"appeared during create");
        assert!(!fs::read_dir(dir.path()).unwrap().any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains(".tmp")));
    }

    #[test]
    fn a_postcommit_external_write_is_not_adopted_as_our_fingerprint() {
        let (_dir, path, mut vault) = fixture_vault();
        on_save(SaveStage::Committed, |_, source| {
            fs::write(source, b"external postcommit replacement").unwrap()
        });
        let added = vault
            .add(ApiKeyInput::new("Committed", "fake-secret"))
            .unwrap();
        assert_eq!(vault.get_by_id(&added.id).unwrap().title, "Committed");
        assert!(matches!(vault.save(), Err(Error::VaultChangedOnDisk)));
        assert_eq!(fs::read(&path).unwrap(), b"external postcommit replacement");
    }

    #[test]
    fn save_does_not_touch_a_preexisting_predictable_temp_file() {
        let (_dir, _path, mut vault) = fixture_vault();
        let foreign_temp = vault.sibling(".tmp");
        fs::write(&foreign_temp, b"not owned by this save").unwrap();
        vault.save().unwrap();
        assert_eq!(fs::read(&foreign_temp).unwrap(), b"not owned by this save");
    }

    #[test]
    fn a_complete_journal_write_with_failed_sync_keeps_the_saved_mutation_and_uncertain_warning() {
        let (_dir, path, mut vault) = fixture_vault();
        crate::audit::fail_next_sync();
        let key = vault
            .add(ApiKeyInput::new("Saved once", "fake-sync-secret"))
            .unwrap();
        assert_eq!(
            Vault::open(path, PASSWORD)
                .unwrap()
                .get_by_id(&key.id)
                .unwrap()
                .title,
            "Saved once"
        );
        let warnings = vault.take_warning_records();
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].kind, VaultWarningKind::Journal);
        assert!(warnings[0].message.contains("could not be confirmed"));
        assert!(warnings[0].message.contains("Do not repeat"));
        assert!(!warnings[0].message.contains("not recorded"));
        assert_eq!(
            vault.audit_log().read(1).unwrap()[0].action,
            "add-completed"
        );
        assert!(vault.take_warnings().is_empty());
        // The stored secret is read after the journal checks: a disclosure adds a record.
        let found = vault.get_by_id(&key.id).unwrap();
        let proof = vault
            .disclose_key(KeyDisclosure::Reveal, &found, None)
            .unwrap();
        assert_eq!(*vault.secret(&key.id, &proof).unwrap(), "fake-sync-secret");
    }

    /// CORE-R2-06: the bytes of a vault file are unauthenticated until the key is checked.
    /// Every read of a vault file takes its size from the open handle first, and reads at most
    /// one byte past the bound. Open, reload, the check before a save and the read-back of the
    /// encrypted temporary file each refuse a file past the bound.
    #[test]
    fn every_read_of_a_vault_file_refuses_a_file_past_the_bound() {
        fn grow(path: &Path) {
            fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_len(MAX_VAULT_BYTES + 1)
                .unwrap();
        }

        let dir = tempfile::tempdir().unwrap();
        let large = dir.path().join("large.kdbx");
        fs::write(&large, b"").unwrap();
        grow(&large);
        private_fs::take_read_count();
        let Err(error) = Vault::open(&large, PASSWORD) else {
            panic!("a file past the bound was opened");
        };
        assert!(error.to_string().contains("larger than"), "{error}");
        assert_eq!(private_fs::take_read_count(), 0, "the file was never read");
        drop(dir);

        let (_dir, path, mut vault) = fixture_vault();
        let saved = fs::read(&path).unwrap();
        grow(&path);
        private_fs::take_read_count();
        let error = vault.reload().unwrap_err();
        assert!(error.to_string().contains("larger than"), "{error}");
        assert_eq!(private_fs::take_read_count(), 0, "the file was never read");
        let error = vault
            .add(ApiKeyInput::new("Fake", "fake-value"))
            .unwrap_err();
        assert!(matches!(error, Error::VaultChangedOnDisk), "{error}");

        fs::write(&path, &saved).unwrap();
        on_save(SaveStage::Serialized, |temp, _| grow(temp));
        let error = vault
            .add(ApiKeyInput::new("Fake", "fake-value"))
            .unwrap_err();
        assert!(error.to_string().contains("larger than"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), saved, "the vault is unchanged");
    }

    /// CORE-R2-05, the reviewer's closing test: two names (hard links) of one vault file. The
    /// lock is named from one path, so it cannot cover the other name, and a save replaces one
    /// name only. A save through either name is refused before it writes anything, whichever
    /// name holds the lock, and also when two saves start at once: the two names never fork.
    /// Reading works through both names, and a save works again once the second name is gone.
    #[test]
    fn a_vault_file_with_two_names_is_read_but_never_written() {
        let (dir, path, vault) = fixture_vault();
        drop(vault);
        let alias = dir.path().join("alias.kdbx");
        fs::hard_link(&path, &alias).unwrap();
        let before = fs::read(&path).unwrap();
        let names = [path.clone(), alias.clone()];
        let serialized = std::rc::Rc::new(std::cell::Cell::new(false));
        for held in &names {
            let lock = acquire_lock(held).unwrap();
            for name in &names {
                let mut vault = Vault::open(name, PASSWORD).unwrap();
                vault.list();
                let flag = serialized.clone();
                on_save(SaveStage::Serialized, move |_, _| flag.set(true));
                let error = vault
                    .add(ApiKeyInput::new("Fake key", "fake-value"))
                    .unwrap_err();
                assert!(
                    matches!(error, Error::Locked) || error.to_string().contains("hard links"),
                    "{error}"
                );
            }
            drop(lock);
        }
        assert!(!serialized.get(), "a refused save wrote no new version");
        let start = std::sync::Arc::new(std::sync::Barrier::new(2));
        let saves: Vec<_> = names
            .iter()
            .cloned()
            .map(|name| {
                let start = start.clone();
                std::thread::spawn(move || {
                    let mut vault = Vault::open(&name, PASSWORD).unwrap();
                    start.wait();
                    vault
                        .add(ApiKeyInput::new("Fake key", "fake-value"))
                        .is_err()
                })
            })
            .collect();
        for save in saves {
            assert!(save.join().unwrap(), "a save through two names was refused");
        }
        assert_eq!(fs::read(&path).unwrap(), before, "the vault is unchanged");
        assert_eq!(fs::read(&alias).unwrap(), before, "the names never fork");

        fs::remove_file(&alias).unwrap();
        let mut vault = Vault::open(&path, PASSWORD).unwrap();
        vault
            .add(ApiKeyInput::new("Fake key", "fake-value"))
            .unwrap();
    }

    /// CORE-R2-05: the check again under the lock, just before the replacement. A second name
    /// made after the first check stops the save, and both names keep the old vault.
    #[test]
    fn a_second_name_made_during_a_save_stops_the_replacement() {
        let (dir, path, mut vault) = fixture_vault();
        let alias = dir.path().join("alias.kdbx");
        let before = fs::read(&path).unwrap();
        let made = alias.clone();
        on_save(SaveStage::BeforeReplace, move |_, vault_path| {
            fs::hard_link(vault_path, &made).unwrap();
        });
        let error = vault
            .add(ApiKeyInput::new("Fake key", "fake-value"))
            .unwrap_err();
        assert!(error.to_string().contains("hard links"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), before, "the vault is unchanged");
        assert_eq!(fs::read(&alias).unwrap(), before, "the names never fork");
    }
}
