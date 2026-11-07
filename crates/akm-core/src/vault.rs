use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Timelike, Utc};
use fs2::FileExt;
use keepass::config::{
    CompressionConfig, DatabaseConfig, DatabaseVersion, InnerCipherConfig, KdfConfig,
    OuterCipherConfig,
};
use keepass::db::{fields, Database, Entry, EntryId, EntryRef, GroupId, Value};
use keepass::DatabaseKey;
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::audit::AuditLog;
use crate::disclosure::{Disclosure, EnvDisclosure, EnvPlan, KeyDisclosure};
use crate::error::{Error, Result};
use crate::model::*;

/// Entries live under this top-level group so a vault can also hold unrelated
/// KeePass entries without them showing up as API keys.
const GROUP_NAME: &str = "API Keys";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultWarningKind {
    Journal,
    BackupRefresh,
    BackupPrune,
    BackupRetention,
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
    warnings: Vec<VaultWarning>,
}

struct ImportPlan {
    preview: ImportPreview,
    replacements: Vec<(EntryId, String)>,
    additions: Vec<ApiKeyInput>,
}

impl Vault {
    pub fn create(path: impl AsRef<Path>, password: &str, name: Option<&str>) -> Result<Self> {
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
            warnings: Vec::new(),
        };
        vault.save()?;
        Ok(vault)
    }

    pub fn open(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(Error::VaultNotFound(path.display().to_string()));
        }

        let key = DatabaseKey::new().with_password(password);
        let bytes = fs::read(&path)?;
        let fingerprint = Some(bytes_fingerprint(&bytes));
        let db = Database::parse(&bytes, key.clone()).map_err(|e| Error::Open(e.to_string()))?;

        Ok(Self {
            path,
            db,
            key,
            fingerprint,
            warnings: Vec::new(),
        })
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
        std::mem::take(&mut self.warnings)
    }

    pub fn reload(&mut self) -> Result<()> {
        let bytes = fs::read(&self.path)?;
        let fingerprint = bytes_fingerprint(&bytes);
        let db = Database::parse(&bytes, self.key.clone())
            .map_err(|error| Error::Open(error.to_string()))?;
        self.db = db;
        self.fingerprint = Some(fingerprint);
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

    /// Prefer a native UUID, then an exact case-insensitive title, then a
    /// case-insensitive substring match across searchable metadata.
    pub fn find(&self, query: &str) -> Result<ApiKey> {
        if let Ok(key) = self.get_by_id(query) {
            return Ok(key);
        }
        let keys = self.list();
        let exact: Vec<_> = keys
            .iter()
            .filter(|key| key.title.eq_ignore_ascii_case(query))
            .collect();
        match exact.len() {
            0 => {}
            1 => return Ok(exact[0].clone()),
            n => return Err(Error::Ambiguous(query.to_string(), n)),
        }
        let matches: Vec<ApiKey> = keys.into_iter().filter(|key| key.matches(query)).collect();

        match matches.len() {
            0 => Err(Error::EntryNotFound(query.to_string())),
            1 => Ok(matches.into_iter().next().expect("checked len == 1")),
            n => Err(Error::Ambiguous(query.to_string(), n)),
        }
    }

    pub fn add(&mut self, input: ApiKeyInput) -> Result<ApiKey> {
        self.ensure_writable()?;
        input.validate()?;
        let now = Utc::now();
        let mut candidate = self.db.clone();

        let key = {
            let group_id = ensure_group(&mut candidate);
            let mut root = candidate.root_mut();
            let mut group = root
                .group_mut(group_id)
                .ok_or_else(|| Error::Invalid("API Keys group disappeared".into()))?;
            let mut entry = group.add_entry();
            let id = entry.id().to_string();
            write_entry(&mut entry, &input, &id, now, now);
            read_entry(&entry)
        };

        self.commit(candidate)?;
        self.log_committed("add", Some(&key), None);
        Ok(key)
    }

    pub fn update(&mut self, id: &str, input: ApiKeyInput) -> Result<ApiKey> {
        self.ensure_writable()?;
        input.validate()?;
        let now = Utc::now();
        let mut candidate = self.db.clone();

        let key = {
            let entry_id = self
                .find_entry_id(id)
                .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
            let created = self.get_by_id(id)?.created_at.unwrap_or(now);

            let mut entry = candidate
                .entry_mut(entry_id)
                .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
            write_entry(&mut entry, &input, id, created, now);
            read_entry(&entry)
        };

        self.commit(candidate)?;
        self.log_committed("update", Some(&key), None);
        Ok(key)
    }

    pub fn remove(&mut self, id: &str) -> Result<ApiKey> {
        self.ensure_writable()?;
        let key = self.get_by_id(id)?;
        let entry_id = self
            .find_entry_id(id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;

        let mut candidate = self.db.clone();
        rebuild_attachment_references(&mut candidate);
        candidate
            .entry_mut(entry_id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?
            .remove();

        self.commit(candidate)?;
        self.log_committed("remove", Some(&key), None);
        Ok(key)
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
        let now = Utc::now();
        for (id, secret) in plan.replacements {
            let mut entry = candidate
                .entry_mut(id)
                .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
            entry.set_protected(fields::PASSWORD, secret);
            entry.times.last_modification = now.naive_utc().with_nanosecond(0);
        }
        for input in plan.additions {
            let group_id = ensure_group(&mut candidate);
            let mut group = candidate
                .group_mut(group_id)
                .ok_or_else(|| Error::Invalid("API Keys group disappeared".into()))?;
            let mut entry = group.add_entry();
            let id = entry.id().to_string();
            write_entry(&mut entry, &input, &id, now, now);
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
            validate_process_var(&name, &self.stored_secret(id))?;
            if !seen.insert(name.clone()) {
                return Err(Error::Invalid(format!(
                    "two keys resolve to the same environment variable {name:?}; set AKM_ENV_VAR explicitly"
                )));
            }
            out.push((key.id, name));
        }

        out.sort_by(|a, b| a.1.cmp(&b.1));
        Ok(EnvPlan::new(out))
    }

    /// Journals a request to disclose one key's secret. The proof exists only when the
    /// record was appended: a journal failure is an error and no secret can be read.
    pub fn disclose_key(
        &self,
        action: KeyDisclosure,
        key: &ApiKey,
        detail: Option<&str>,
    ) -> Result<Disclosure> {
        self.log(action.action(), Some(key), detail)?;
        Ok(Disclosure::key(action, key.id.clone()))
    }

    /// Journals a request to disclose a planned scope, before any value is read.
    pub fn disclose_env(
        &self,
        action: EnvDisclosure,
        plan: &EnvPlan,
        detail: Option<&str>,
    ) -> Result<Disclosure> {
        self.log(action.action(), None, detail)?;
        Ok(Disclosure::env(action, plan.key_ids()))
    }

    /// One key's secret, for the key that `proof` names.
    pub fn secret(&self, id: &str, proof: &Disclosure) -> Result<Zeroizing<String>> {
        if !proof.covers_key(id) {
            return Err(Error::Invalid(
                "this disclosure record does not cover that key".into(),
            ));
        }
        let entry = self
            .find_entry_id(id)
            .ok_or_else(|| Error::EntryNotFound(id.to_string()))?;
        Ok(self.stored_secret(entry))
    }

    /// The variables of a plan, with their values. `proof` must be the record of this plan.
    pub fn env_vars(&self, plan: &EnvPlan, proof: &Disclosure) -> Result<Vec<(String, String)>> {
        if !proof.covers_env(plan) {
            return Err(Error::Invalid(
                "this disclosure record does not cover that selection".into(),
            ));
        }
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
        let cutoff = Utc::now() + Duration::days(within_days);
        let mut keys: Vec<ApiKey> = self
            .list()
            .into_iter()
            .filter(|key| key.expires_at.is_some_and(|expires| expires <= cutoff))
            .collect();
        keys.sort_by_key(|key| key.expires_at);
        keys
    }

    pub fn audit_log(&self) -> AuditLog {
        AuditLog::for_vault(&self.path)
    }

    pub fn log(&self, action: &str, key: Option<&ApiKey>, detail: Option<&str>) -> Result<()> {
        self.audit_log().append(
            action,
            key.map(|k| k.id.as_str()),
            key.map(|k| k.title.as_str()),
            key.and_then(|k| k.project.as_deref()),
            key.and_then(|k| k.environment.as_deref()),
            detail,
        )
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

    fn commit(&mut self, candidate: Database) -> Result<()> {
        if !matches!(candidate.config.version, DatabaseVersion::KDB4(1)) {
            return Err(Error::UpgradeRequired);
        }
        let _lock = acquire_lock(&self.path)?;
        let original = self.checked_source()?;
        let mut tmp = PendingFile::temporary(&self.path)?;
        candidate
            .save(tmp.file_mut(), self.key.clone())
            .map_err(|e| Error::Save(e.to_string()))?;
        tmp.sync_and_close()?;
        #[cfg(test)]
        tests::run_save_hook(tests::SaveStage::Serialized, &tmp.path, &self.path);
        let encrypted = fs::read(&tmp.path)?;
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
        let mut backup = if let Some(bytes) = &original {
            let suffix = format!(
                ".bak.{}_{}.kdbx",
                Utc::now().format("%Y%m%dT%H%M%S%fZ"),
                Uuid::new_v4()
            );
            let mut backup = PendingFile::new(self.sibling(&suffix))?;
            backup.file_mut().write_all(bytes)?;
            backup.sync_and_close()?;
            if fs::read(&backup.path)? != *bytes {
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
        if self.fingerprint.is_none() {
            fs::hard_link(&tmp.path, &self.path).map_err(|error| {
                if error.kind() == std::io::ErrorKind::AlreadyExists {
                    Error::VaultChangedOnDisk
                } else {
                    Error::Io(error)
                }
            })?;
        } else {
            fs::rename(&tmp.path, &self.path)?;
        }
        #[cfg(test)]
        tests::run_save_hook(tests::SaveStage::Committed, &tmp.path, &self.path);
        self.fingerprint = Some(fingerprint);
        self.db = verified;
        if let Some(backup) = &mut backup {
            backup.keep();
        }
        if let Some(bytes) = original {
            if let Err(error) = self.refresh_legacy_backup(&bytes) {
                self.warnings.push(VaultWarning {
                    kind: VaultWarningKind::BackupRefresh,
                    message: format!("vault saved, but .bak could not be refreshed: {error}"),
                });
            }
            let retained = backup.as_ref().is_some_and(|backup| {
                // A pre-commit copy is not proof of post-maintenance retention.
                // Do not prune other recovery candidates if this identity is already lost.
                if !fs::read(&backup.path).is_ok_and(|actual| actual == bytes) {
                    return false;
                }
                if let Err(error) = self.prune_backups(&backup.path) {
                    self.warnings.push(VaultWarning {
                        kind: VaultWarningKind::BackupPrune,
                        message: format!(
                            "vault saved, but old backups could not be pruned: {error}"
                        ),
                    });
                }
                fs::read(&backup.path).is_ok_and(|actual| actual == bytes)
            });
            if !retained {
                self.warnings.push(VaultWarning {
                    kind: VaultWarningKind::BackupRetention,
                    message: "vault saved, but retention of the pre-change encrypted backup could not be confirmed; inspect backups before relying on recovery".into(),
                });
            }
        }
        Ok(())
    }

    fn checked_source(&self) -> Result<Option<Vec<u8>>> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return if self.fingerprint.is_none() {
                    Ok(None)
                } else {
                    Err(Error::VaultChangedOnDisk)
                };
            }
            Err(error) => return Err(error.into()),
        };
        match &self.fingerprint {
            Some(expected) if *expected == bytes_fingerprint(&bytes) => Ok(Some(bytes)),
            _ => Err(Error::VaultChangedOnDisk),
        }
    }

    fn refresh_legacy_backup(&self, bytes: &[u8]) -> Result<()> {
        let mut tmp = PendingFile::temporary(&self.path)?;
        tmp.file_mut().write_all(bytes)?;
        tmp.sync_and_close()?;
        fs::rename(&tmp.path, self.sibling(".bak"))?;
        Ok(())
    }

    fn prune_backups(&self, protected: &Path) -> Result<()> {
        let parent = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let prefix = format!(
            "{}.bak.",
            self.path.file_name().unwrap_or_default().to_string_lossy()
        );
        let mut backups = Vec::new();
        for entry in fs::read_dir(parent)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if let Some((timestamp, id)) = name
                .strip_prefix(&prefix)
                .and_then(|name| name.split_once('_'))
            {
                if timestamp.len() == 25
                    && id
                        .strip_suffix(".kdbx")
                        .is_some_and(|id| Uuid::parse_str(id).is_ok())
                    // The directory enumeration may add a leading `./` for a
                    // basename-only vault path. Compare the entry within this
                    // same parent, not two differently-spelled relative paths.
                    && entry.path().file_name() != protected.file_name()
                {
                    backups.push(entry.path());
                }
            }
        }
        backups.sort();
        // Reserve one of the five slots for this commit's verified original,
        // regardless of clock rollback or later-sorting historical filenames.
        for backup in backups.iter().take(backups.len().saturating_sub(4)) {
            fs::remove_file(backup)?;
        }
        Ok(())
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

    fn sibling(&self, suffix: &str) -> PathBuf {
        let mut os = self.path.as_os_str().to_os_string();
        os.push(suffix);
        PathBuf::from(os)
    }
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

fn acquire_lock(vault_path: &Path) -> Result<File> {
    let mut os = vault_path.as_os_str().to_os_string();
    os.push(".lock");
    let lock_path = PathBuf::from(os);

    if let Some(parent) = lock_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

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
    let mut intended = intended.clone();
    let mut verified = verified.clone();
    intended.remove_unused_attachments();
    verified.remove_unused_attachments();
    intended.foreach_attachment_mut(|attachment| attachment.remove());
    verified.foreach_attachment_mut(|attachment| attachment.remove());
    intended == verified
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
}

impl PendingFile {
    fn new(path: PathBuf) -> Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        Ok(Self {
            path,
            file: Some(file),
            remove_on_drop: true,
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
    created: DateTime<Utc>,
    updated: DateTime<Utc>,
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
    entry.times.creation = created.naive_utc().with_nanosecond(0);
    entry.times.last_modification = updated.naive_utc().with_nanosecond(0);

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

    fn fixture_vault() -> (tempfile::TempDir, PathBuf, Vault) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("vault.kdbx");
        fs::write(&path, include_bytes!("../tests/fixtures/kdbx40.kdbx")).unwrap();
        let mut vault = Vault::open(&path, PASSWORD).unwrap();
        vault.upgrade().unwrap();
        (dir, path, vault)
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
}
