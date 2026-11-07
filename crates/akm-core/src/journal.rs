//! Where the activity journal lives, and which journal belongs to which vault
//! (unit 1b, design D1, D2 and D4).
//!
//! The journals sit in one private per-user folder. A random journal ID names each one.
//! `mapping.json` binds a vault to a journal ID by evidence about the vault (path, file
//! identity, content hash, vault ID). Nothing is ever written to a vault, and two bindings
//! are never merged.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::private_fs::{self, OpenMode};

/// Replaces the journal folder. Only tests and the proof scripts set it (design D1).
pub const ROOT_ENV: &str = "AKM_JOURNAL_ROOT";

const MAPPING: &str = "mapping.json";
const MAPPING_LOCK: &str = "mapping.lock";
const MAPPING_TMP: &str = "mapping.json.tmp";
const MAPPING_VERSION: u32 = 1;
const SEEN_LIMIT: usize = 32;

fn journal_error(message: impl std::fmt::Display) -> Error {
    Error::Journal(message.to_string())
}

/// The private folder that holds the mapping and the journals.
#[derive(Debug, Clone)]
pub struct JournalRoot {
    location: std::result::Result<PathBuf, String>,
}

impl JournalRoot {
    /// A root at an explicit absolute path. The folder is created, and checked, on first use.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self::checked(path.into(), "the journal folder")
    }

    /// `AKM_JOURNAL_ROOT` if it is set, else `%LOCALAPPDATA%\akm\journal`. A value that
    /// cannot be used makes the journal unavailable. It never selects another place.
    pub fn from_environment() -> Self {
        Self::resolve(
            std::env::var_os(ROOT_ENV),
            std::env::var_os("LOCALAPPDATA"),
            cfg!(debug_assertions) && std::env::var_os("CARGO_MANIFEST_DIR").is_some(),
        )
    }

    /// `under_cargo`: a debug build that cargo started. Without an explicit root it never uses
    /// the real profile folder, so a test run from outside the repository fails instead of
    /// writing fake journals there (`.cargo/config.toml` sets the root only below the repository).
    fn resolve(
        explicit: Option<std::ffi::OsString>,
        local_app_data: Option<std::ffi::OsString>,
        under_cargo: bool,
    ) -> Self {
        if let Some(value) = explicit {
            return Self::checked(PathBuf::from(value), ROOT_ENV);
        }
        if under_cargo {
            return Self {
                location: Err(format!(
                    "{ROOT_ENV} is not set in a debug build that cargo started; \
                     the real profile folder is never used for tests"
                )),
            };
        }
        match local_app_data {
            Some(base) => {
                let mut root = Self::checked(PathBuf::from(base), "LOCALAPPDATA");
                if let Ok(path) = &mut root.location {
                    path.push("akm");
                    path.push("journal");
                }
                root
            }
            None => Self {
                location: Err(
                    "LOCALAPPDATA is not set, so there is no private journal folder".into(),
                ),
            },
        }
    }

    fn checked(path: PathBuf, name: &str) -> Self {
        let location = if path.as_os_str().is_empty() {
            Err(format!("{name} is empty"))
        } else if !path.is_absolute() {
            Err(format!("{name} must be an absolute path"))
        } else {
            Ok(path)
        };
        Self { location }
    }

    pub fn path(&self) -> Result<&Path> {
        self.location.as_deref().map_err(journal_error)
    }

    /// Creates the private folder if it is missing, and checks it. Idempotent.
    pub fn ensure_private(&self) -> Result<()> {
        self.open_or_create().map(|_| ())
    }

    pub(crate) fn open(&self) -> Result<Option<Folder>> {
        let path = self.path()?;
        private_fs::reject_links(path)?;
        Ok(private_fs::open_dir(path)?.map(|handle| Folder {
            path: path.to_path_buf(),
            _handle: handle,
        }))
    }

    pub(crate) fn open_or_create(&self) -> Result<Folder> {
        if let Some(folder) = self.open()? {
            return Ok(folder);
        }
        let path = self.path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                journal_error(format!("cannot create {}: {error}", parent.display()))
            })?;
        }
        match private_fs::create_private_dir(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(journal_error(format!(
                    "cannot create {}: {error}",
                    path.display()
                )))
            }
        }
        self.open()?.ok_or_else(|| {
            journal_error(format!(
                "{} disappeared while it was created",
                path.display()
            ))
        })
    }
}

/// An open, checked journal folder. Holding it stops another program from renaming it.
pub(crate) struct Folder {
    path: PathBuf,
    _handle: File,
}

impl Folder {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn file(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    pub(crate) fn journal_file(&self, id: &str) -> PathBuf {
        self.file(&format!("j-{id}.jsonl"))
    }
}

/// What AKM knows about one vault when it asks for its journal.
#[derive(Debug, Clone)]
pub struct VaultEvidence {
    path: PathBuf,
    sha: Option<String>,
    root: Option<String>,
}

impl VaultEvidence {
    /// Evidence of a vault that is not unlocked: its path and the bytes on disk. There is no
    /// vault ID, so the journal can be read and never changed with this evidence.
    pub fn from_file(path: &Path) -> Self {
        Self {
            path: path.to_path_buf(),
            sha: fs::read(path)
                .ok()
                .map(|bytes| hex::encode(Sha256::digest(bytes))),
            root: None,
        }
    }

    /// Evidence of an unlocked vault: the bytes that it last read or wrote, and its vault ID.
    pub fn unlocked(path: &Path, sha: Option<String>, vault_id: String) -> Self {
        Self {
            path: path.to_path_buf(),
            sha,
            root: Some(vault_id),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn observe(&self) -> Observation {
        let canonical = canonical(&self.path);
        let stale = self.is_stale(&canonical);
        Observation {
            key: path_key(&canonical),
            file_id: private_fs::file_id(&canonical),
            path: canonical.display().to_string(),
            sha: self.sha.clone(),
            root: self.root.clone(),
            stale,
        }
    }

    /// The bytes this evidence describes are not the bytes at the path now. A file that
    /// cannot be read counts as different: the evidence cannot be shown to be current.
    /// Only evidence of an unlocked vault can be stale. Evidence without a vault ID is
    /// read-only (D2) and keeps the rules R1 to R6, which show an unseen file as unconfirmed.
    fn is_stale(&self, canonical: &Path) -> bool {
        let (Some(sha), Some(_)) = (&self.sha, &self.root) else {
            return false;
        };
        match fs::read(canonical) {
            Ok(bytes) => hex::encode(Sha256::digest(bytes)) != *sha,
            Err(_) => true,
        }
    }
}

fn canonical(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    if let Ok(real) = fs::canonicalize(&absolute) {
        return strip_verbatim(real);
    }
    match (absolute.parent(), absolute.file_name()) {
        (Some(parent), Some(name)) => fs::canonicalize(parent)
            .map(|real| strip_verbatim(real).join(name))
            .unwrap_or(absolute),
        _ => absolute,
    }
}

fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

fn path_key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum BindingState {
    Active,
    Detached,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Detached {
    pub reason: String,
    pub at: DateTime<Utc>,
}

/// The device-local record that ties one vault to one journal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Binding {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub file_id: Option<String>,
    #[serde(default)]
    pub root: Option<String>,
    pub seen: Vec<String>,
    pub state: BindingState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detached: Option<Detached>,
    pub created: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Mapping {
    pub version: u32,
    pub journals: Vec<Binding>,
}

impl Mapping {
    fn empty() -> Self {
        Self {
            version: MAPPING_VERSION,
            journals: Vec::new(),
        }
    }
}

/// The evidence, normalized for comparison.
#[derive(Debug, Clone)]
pub(crate) struct Observation {
    pub path: String,
    pub key: String,
    pub file_id: Option<String>,
    pub sha: Option<String>,
    pub root: Option<String>,
    /// The evidence is a snapshot that no longer matches the file at the path (unit 1b, D10).
    pub stale: bool,
}

/// The identity rules R1 to R6 of the design, as data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Resolution {
    Same {
        index: usize,
        rolled_back: bool,
        adopt_root: bool,
    },
    Replaced {
        index: usize,
    },
    Unconfirmed {
        index: usize,
    },
    Moved {
        index: usize,
    },
    New,
    /// Stale evidence of the vault that owns the active binding at this path. The journal
    /// is that binding's. Nothing about the binding is learned from it.
    Observed {
        index: usize,
    },
    /// Stale evidence that no active binding at this path vouches for.
    Superseded,
}

pub(crate) fn resolve(bindings: &[Binding], observation: &Observation) -> Resolution {
    if observation.stale {
        return bindings
            .iter()
            .position(|binding| {
                binding.state == BindingState::Active
                    && path_key_of(binding) == observation.key
                    && matches!((&binding.root, &observation.root), (Some(a), Some(b)) if a == b)
            })
            .map_or(Resolution::Superseded, |index| Resolution::Observed {
                index,
            });
    }
    let seen_at = |binding: &Binding| {
        observation
            .sha
            .as_ref()
            .and_then(|sha| binding.seen.iter().position(|seen| seen == sha))
    };
    let same_path = bindings.iter().position(|binding| {
        binding.state == BindingState::Active && path_key_of(binding) == observation.key
    });
    if let Some(index) = same_path {
        let binding = &bindings[index];
        if let Some(position) = seen_at(binding) {
            return Resolution::Same {
                index,
                rolled_back: position != 0,
                adopt_root: binding.root.is_none() && observation.root.is_some(),
            };
        }
        return match (&binding.root, &observation.root) {
            (Some(known), Some(now)) if known == now => Resolution::Same {
                index,
                rolled_back: false,
                adopt_root: false,
            },
            (Some(_), Some(_)) => Resolution::Replaced { index },
            (None, Some(_)) => Resolution::Same {
                index,
                rolled_back: false,
                adopt_root: true,
            },
            (_, None) => Resolution::Unconfirmed { index },
        };
    }
    if let Some(file_id) = &observation.file_id {
        let moved = bindings.iter().position(|binding| {
            binding.state == BindingState::Active
                && binding.file_id.as_ref() == Some(file_id)
                && (seen_at(binding).is_some()
                    || matches!((&binding.root, &observation.root), (Some(a), Some(b)) if a == b))
        });
        if let Some(index) = moved {
            return Resolution::Moved { index };
        }
    }
    Resolution::New
}

fn path_key_of(binding: &Binding) -> String {
    path_key(Path::new(&binding.path))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Identity {
    Confirmed,
    Unconfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    LegacySidecar,
    IdentityUnconfirmed,
    EarlierJournal,
    RelatedJournal,
}

/// A fact about the journal that the reader must see. The text names no cause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    pub kind: NoticeKind,
    pub message: String,
}

pub(crate) fn legacy_notice(vault: &Path) -> Option<Notice> {
    let mut name = vault.as_os_str().to_os_string();
    name.push(".audit.jsonl");
    let sidecar = PathBuf::from(name);
    let metadata = fs::symlink_metadata(&sidecar)
        .ok()
        .filter(fs::Metadata::is_file)?;
    Some(Notice {
        kind: NoticeKind::LegacySidecar,
        message: format!(
            "A legacy plaintext journal exists beside the vault: {} ({} bytes). Earlier versions wrote key titles, projects, environments and times there. If this folder syncs, those copies may already be in the cloud. Moving or deleting the file does not remove copies that already synced. AKM did not read, change, move or upload it, and it does not import it. Delete it yourself after you check it.",
            sidecar.display(),
            metadata.len()
        ),
    })
}

fn unconfirmed_notice() -> Notice {
    Notice {
        kind: NoticeKind::IdentityUnconfirmed,
        message: "The vault file is not the one that AKM last saw at this path, so this journal cannot be confirmed as its journal. Unlock the vault to confirm it. Nothing was changed.".into(),
    }
}

/// The mapping notices for one observation, over all bindings. `found` is the binding
/// that was chosen, if any.
fn mapping_notices(
    bindings: &[Binding],
    observation: &Observation,
    found: Option<usize>,
    folder: &Folder,
) -> Vec<Notice> {
    let mut notices = Vec::new();
    for (index, binding) in bindings.iter().enumerate() {
        if Some(index) == found {
            continue;
        }
        let same_path = path_key_of(binding) == observation.key;
        if binding.state == BindingState::Detached && same_path {
            notices.push(Notice {
                kind: NoticeKind::EarlierJournal,
                message: format!(
                    "An earlier journal for a different vault at this path is kept in {}. It is not merged into this one.",
                    folder.journal_file(&binding.id).display()
                ),
            });
        } else if binding.state == BindingState::Active && !same_path {
            let related = observation
                .sha
                .as_ref()
                .is_some_and(|sha| binding.seen.contains(sha))
                || matches!((&binding.root, &observation.root), (Some(a), Some(b)) if a == b);
            if related {
                notices.push(Notice {
                    kind: NoticeKind::RelatedJournal,
                    message: format!(
                        "A journal for a related vault file (same content or same vault ID) is bound to {}. Journals are not merged.",
                        binding.path
                    ),
                });
            }
        }
    }
    notices
}

fn damaged_mapping(path: &Path) -> Error {
    journal_error(format!(
        "the mapping file {} is damaged. Its bytes were kept. No record is written until you move or repair it",
        path.display()
    ))
}

fn load(folder: &Folder) -> Result<Mapping> {
    let path = folder.file(MAPPING);
    let Some(mut file) = private_fs::open_file(&path, OpenMode::Read)? else {
        return Ok(Mapping::empty());
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| journal_error(format!("cannot read {}: {error}", path.display())))?;
    let mapping: Mapping = serde_json::from_slice(&bytes).map_err(|_| damaged_mapping(&path))?;
    if mapping.version != MAPPING_VERSION {
        return Err(journal_error(format!(
            "the mapping file {} has version {}, which this AKM cannot use",
            path.display(),
            mapping.version
        )));
    }
    Ok(mapping)
}

fn store(folder: &Folder, mapping: &Mapping) -> Result<()> {
    let temporary = folder.file(MAPPING_TMP);
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(journal_error(format!(
                "cannot remove {}: {error}",
                temporary.display()
            )))
        }
    }
    let mut file = private_fs::open_file(&temporary, OpenMode::CreateNew)?
        .ok_or_else(|| journal_error("cannot create the mapping file"))?;
    let bytes = serde_json::to_vec_pretty(mapping)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| journal_error(format!("cannot write the mapping: {error}")))?;
    drop(file);
    fs::rename(&temporary, folder.file(MAPPING))
        .map_err(|error| journal_error(format!("cannot replace the mapping: {error}")))
}

fn lock_mapping(folder: &Folder, exclusive: bool) -> Result<Option<File>> {
    let path = folder.file(MAPPING_LOCK);
    let mode = if exclusive {
        OpenMode::LockCreate
    } else {
        OpenMode::LockExisting
    };
    let Some(file) = private_fs::open_file(&path, mode)? else {
        return Ok(None);
    };
    let locked = if exclusive {
        file.lock_exclusive()
    } else {
        FileExt::lock_shared(&file)
    };
    locked.map_err(|error| journal_error(format!("cannot lock {}: {error}", path.display())))?;
    Ok(Some(file))
}

fn new_binding(observation: &Observation) -> Binding {
    Binding {
        id: uuid::Uuid::new_v4().simple().to_string(),
        path: observation.path.clone(),
        file_id: observation.file_id.clone(),
        root: observation.root.clone(),
        seen: observation.sha.iter().cloned().collect(),
        state: BindingState::Active,
        detached: None,
        created: Utc::now(),
    }
}

/// Records what was seen on a binding. Returns whether the binding changed.
fn note(binding: &mut Binding, observation: &Observation, adopt_root: bool) -> bool {
    let before = binding.clone();
    if observation.file_id.is_some() {
        binding.file_id.clone_from(&observation.file_id);
    }
    if adopt_root {
        binding.root.clone_from(&observation.root);
    }
    if let Some(sha) = &observation.sha {
        binding.seen.retain(|seen| seen != sha);
        binding.seen.insert(0, sha.clone());
        binding.seen.truncate(SEEN_LIMIT);
    }
    *binding != before
}

/// The journal that a record about this vault goes to.
pub(crate) struct Placement {
    pub id: String,
    /// The vault went back to a version that this device saw. The record of that is owed
    /// before the requested record.
    pub owes_rollback: bool,
}

fn superseded() -> Error {
    journal_error(
        "the file at this vault's path is no longer the one this session opened, so its journal cannot be confirmed from here. Reopen the vault",
    )
}

/// What one observation changes in the mapping, and where its record goes.
struct Decision {
    mapping: Mapping,
    changed: bool,
    placement: Placement,
}

/// The identity rules applied to the mapping. Pure: nothing is written.
fn decide(mut mapping: Mapping, observation: &Observation) -> Result<Decision> {
    let mut changed = true;
    let mut owes_rollback = false;
    let id = match resolve(&mapping.journals, observation) {
        Resolution::Same {
            index,
            rolled_back,
            adopt_root,
        } => {
            owes_rollback = rolled_back;
            changed = note(&mut mapping.journals[index], observation, adopt_root);
            mapping.journals[index].id.clone()
        }
        Resolution::Observed { index } => {
            changed = false;
            mapping.journals[index].id.clone()
        }
        Resolution::Superseded => return Err(superseded()),
        Resolution::Moved { index } => {
            let binding = &mut mapping.journals[index];
            binding.path.clone_from(&observation.path);
            note(binding, observation, false);
            binding.id.clone()
        }
        Resolution::Replaced { index } => {
            let old = &mut mapping.journals[index];
            old.state = BindingState::Detached;
            old.detached = Some(Detached {
                reason: "replaced".into(),
                at: Utc::now(),
            });
            let binding = new_binding(observation);
            let id = binding.id.clone();
            mapping.journals.push(binding);
            id
        }
        Resolution::Unconfirmed { .. } => {
            return Err(journal_error(
                "the vault file is not the one that AKM last saw at this path, so its journal cannot be confirmed from here",
            ));
        }
        Resolution::New => {
            let binding = new_binding(observation);
            let id = binding.id.clone();
            mapping.journals.push(binding);
            id
        }
    };
    Ok(Decision {
        mapping,
        changed,
        placement: Placement { id, owes_rollback },
    })
}

/// Runs `write` for the journal that a record about this vault goes to, with the mapping
/// locked. The only place that stores a mapping for a record.
///
/// A mapping change that consumes an owed record (the rollback indication moves the
/// observed version to the front of `seen`) is stored only after `write` succeeded. If
/// `write` fails, the mapping is as it was and a retry owes the record again. Every other
/// change is stored first: a binding without a journal file yet is harmless, a record
/// without a binding is not.
pub(crate) fn record<T>(
    folder: &Folder,
    evidence: &VaultEvidence,
    write: impl FnOnce(&Placement) -> Result<T>,
) -> Result<T> {
    let lock = lock_mapping(folder, true)?;
    let decision = decide(load(folder)?, &evidence.observe())?;
    if decision.placement.owes_rollback {
        let written = write(&decision.placement)?;
        if decision.changed {
            store(folder, &decision.mapping)?;
        }
        return Ok(written);
    }
    if decision.changed {
        store(folder, &decision.mapping)?;
    }
    drop(lock);
    write(&decision.placement)
}

/// The journal ID for a vault, creating the binding if there is none. It consumes no owed
/// record: when one is owed, the mapping is left as it is.
pub(crate) fn place(folder: &Folder, evidence: &VaultEvidence) -> Result<String> {
    let _lock = lock_mapping(folder, true)?;
    let decision = decide(load(folder)?, &evidence.observe())?;
    if decision.changed && !decision.placement.owes_rollback {
        store(folder, &decision.mapping)?;
    }
    Ok(decision.placement.id)
}

/// The result of a read-only lookup.
pub(crate) struct Located {
    pub folder: Option<Folder>,
    pub id: Option<String>,
    pub identity: Identity,
    pub notices: Vec<Notice>,
}

/// Finds the journal of a vault without creating or changing anything.
pub(crate) fn locate(root: &JournalRoot, evidence: &VaultEvidence) -> Result<Located> {
    let mut notices: Vec<Notice> = legacy_notice(evidence.path()).into_iter().collect();
    let Some(folder) = root.open()? else {
        return Ok(Located {
            folder: None,
            id: None,
            identity: Identity::Confirmed,
            notices,
        });
    };
    let lock = lock_mapping(&folder, false)?;
    let mapping = if lock.is_some() {
        load(&folder)?
    } else {
        Mapping::empty()
    };
    drop(lock);
    let observation = evidence.observe();
    let (found, identity) = match resolve(&mapping.journals, &observation) {
        Resolution::Same { index, .. } | Resolution::Moved { index } => {
            (Some(index), Identity::Confirmed)
        }
        Resolution::Unconfirmed { index } => {
            notices.push(unconfirmed_notice());
            (Some(index), Identity::Unconfirmed)
        }
        Resolution::Observed { index } => (Some(index), Identity::Confirmed),
        Resolution::Superseded => {
            notices.push(unconfirmed_notice());
            (None, Identity::Unconfirmed)
        }
        Resolution::Replaced { .. } | Resolution::New => (None, Identity::Confirmed),
    };
    notices.extend(mapping_notices(
        &mapping.journals,
        &observation,
        found,
        &folder,
    ));
    let id = found.map(|index| mapping.journals[index].id.clone());
    Ok(Located {
        folder: Some(folder),
        id,
        identity,
        notices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_debug_build_under_cargo_without_a_root_never_selects_the_profile_folder() {
        let profile = std::ffi::OsString::from(r"C:\Users\fake\AppData\Local");
        let guarded = JournalRoot::resolve(None, Some(profile.clone()), true);
        let message = guarded.path().unwrap_err().to_string();
        assert!(message.contains("AKM_JOURNAL_ROOT is not set"), "{message}");

        let explicit =
            JournalRoot::resolve(Some(r"C:\fake\journal".into()), Some(profile.clone()), true);
        assert_eq!(explicit.path().unwrap(), Path::new(r"C:\fake\journal"));

        let normal = JournalRoot::resolve(None, Some(profile), false);
        assert_eq!(
            normal.path().unwrap(),
            Path::new(r"C:\Users\fake\AppData\Local\akm\journal")
        );
    }

    fn binding(path: &str, file_id: Option<&str>, root: Option<&str>, seen: &[&str]) -> Binding {
        Binding {
            id: "00".into(),
            path: path.into(),
            file_id: file_id.map(str::to_string),
            root: root.map(str::to_string),
            seen: seen.iter().map(|sha| sha.to_string()).collect(),
            state: BindingState::Active,
            detached: None,
            created: Utc::now(),
        }
    }

    fn seen(
        path: &str,
        file_id: Option<&str>,
        sha: Option<&str>,
        root: Option<&str>,
    ) -> Observation {
        Observation {
            path: path.into(),
            key: path_key(Path::new(path)),
            file_id: file_id.map(str::to_string),
            sha: sha.map(str::to_string),
            root: root.map(str::to_string),
            stale: false,
        }
    }

    #[test]
    fn identity_table_rows_r1_to_r6() {
        let bound = [binding(
            r"C:\V\a.kdbx",
            Some("f1"),
            Some("r1"),
            &["new", "old"],
        )];
        let cases: [(&str, Observation, Resolution); 15] = [
            (
                "R1 same bytes",
                seen(r"c:\v\A.KDBX", None, Some("new"), None),
                Resolution::Same {
                    index: 0,
                    rolled_back: false,
                    adopt_root: false,
                },
            ),
            (
                "R1 rollback to an earlier version this device saw",
                seen(r"C:\V\a.kdbx", None, Some("old"), Some("r1")),
                Resolution::Same {
                    index: 0,
                    rolled_back: true,
                    adopt_root: false,
                },
            ),
            (
                "R2 changed elsewhere, same vault",
                seen(r"C:\V\a.kdbx", None, Some("x"), Some("r1")),
                Resolution::Same {
                    index: 0,
                    rolled_back: false,
                    adopt_root: false,
                },
            ),
            (
                "R3 another vault at the path",
                seen(r"C:\V\a.kdbx", Some("f1"), Some("x"), Some("r2")),
                Resolution::Replaced { index: 0 },
            ),
            (
                "R4 locked read, unseen bytes",
                seen(r"C:\V\a.kdbx", Some("f1"), Some("x"), None),
                Resolution::Unconfirmed { index: 0 },
            ),
            (
                "R4 unreadable vault",
                seen(r"C:\V\a.kdbx", None, None, None),
                Resolution::Unconfirmed { index: 0 },
            ),
            (
                "R5 renamed, same bytes",
                seen(r"C:\V\b.kdbx", Some("f1"), Some("new"), None),
                Resolution::Moved { index: 0 },
            ),
            (
                "R5 renamed and changed, same vault",
                seen(r"C:\V\b.kdbx", Some("f1"), Some("x"), Some("r1")),
                Resolution::Moved { index: 0 },
            ),
            (
                "R6 renamed and changed, vault ID unknown",
                seen(r"C:\V\b.kdbx", Some("f1"), Some("x"), None),
                Resolution::New,
            ),
            (
                "R6 file ID reused by another vault",
                seen(r"C:\V\b.kdbx", Some("f1"), Some("x"), Some("r2")),
                Resolution::New,
            ),
            (
                "R6 copy: same bytes, other file",
                seen(r"C:\V\c.kdbx", Some("f2"), Some("new"), Some("r1")),
                Resolution::New,
            ),
            (
                "R6 copy without a file ID",
                seen(r"C:\V\c.kdbx", None, Some("new"), None),
                Resolution::New,
            ),
            (
                "R6 unrelated vault",
                seen(r"C:\V\d.kdbx", Some("f9"), Some("zz"), Some("r9")),
                Resolution::New,
            ),
            (
                "R1 the bytes decide before the vault ID",
                seen(r"C:\V\a.kdbx", None, Some("old"), Some("r2")),
                Resolution::Same {
                    index: 0,
                    rolled_back: true,
                    adopt_root: false,
                },
            ),
            (
                "R6 same file ID, other volume path and unseen bytes",
                seen(r"D:\V\a.kdbx", Some("f1"), Some("x"), None),
                Resolution::New,
            ),
        ];
        for (name, observation, expected) in cases {
            assert_eq!(resolve(&bound, &observation), expected, "{name}");
        }
    }

    #[test]
    fn stale_evidence_resolves_only_through_the_active_binding_with_its_vault_id() {
        let mut bound = [binding(
            r"C:\V\a.kdbx",
            Some("f1"),
            Some("r1"),
            &["new", "old"],
        )];
        let stale = |path: &str, sha: &str, root: Option<&str>| Observation {
            stale: true,
            ..seen(path, Some("f1"), Some(sha), root)
        };
        let cases: [(&str, Observation, Resolution); 6] = [
            (
                "same vault, a version the mapping knows (no rollback)",
                stale(r"c:\v\A.KDBX", "old", Some("r1")),
                Resolution::Observed { index: 0 },
            ),
            (
                "same vault, a version the mapping never saw",
                stale(r"C:\V\a.kdbx", "x", Some("r1")),
                Resolution::Observed { index: 0 },
            ),
            (
                "another vault, the replaced one",
                stale(r"C:\V\a.kdbx", "x", Some("r2")),
                Resolution::Superseded,
            ),
            (
                "the bytes are known but the vault ID is not",
                stale(r"C:\V\a.kdbx", "old", None),
                Resolution::Superseded,
            ),
            (
                "no binding at this path",
                stale(r"C:\V.kdbx", "old", Some("r1")),
                Resolution::Superseded,
            ),
            (
                "fresh evidence of the same bytes still sees the rollback",
                seen(r"C:\V\a.kdbx", Some("f1"), Some("old"), Some("r1")),
                Resolution::Same {
                    index: 0,
                    rolled_back: true,
                    adopt_root: false,
                },
            ),
        ];
        for (name, observation, expected) in cases {
            assert_eq!(resolve(&bound, &observation), expected, "{name}");
        }
        bound[0].state = BindingState::Detached;
        assert_eq!(
            resolve(&bound, &stale(r"C:\V\a.kdbx", "old", Some("r1"))),
            Resolution::Superseded,
            "a detached binding never vouches"
        );
    }

    #[test]
    fn a_binding_without_a_vault_id_adopts_the_first_one_and_detached_bindings_never_match() {
        let mut bound = [binding(r"C:\V\a.kdbx", None, None, &["old"])];
        assert_eq!(
            resolve(&bound, &seen(r"C:\V\a.kdbx", None, Some("x"), Some("r1"))),
            Resolution::Same {
                index: 0,
                rolled_back: false,
                adopt_root: true
            }
        );
        bound[0].state = BindingState::Detached;
        assert_eq!(
            resolve(&bound, &seen(r"C:\V\a.kdbx", None, Some("old"), Some("r1"))),
            Resolution::New
        );
    }

    #[test]
    fn two_bindings_never_resolve_to_each_other() {
        let bound = [
            binding(r"C:\V\a.kdbx", Some("f1"), Some("r1"), &["a"]),
            binding(r"C:\V\b.kdbx", Some("f2"), Some("r1"), &["a"]),
        ];
        assert_eq!(
            resolve(
                &bound,
                &seen(r"C:\V\b.kdbx", Some("f2"), Some("a"), Some("r1"))
            ),
            Resolution::Same {
                index: 1,
                rolled_back: false,
                adopt_root: false
            }
        );
        assert_eq!(
            resolve(
                &bound,
                &seen(r"C:\V\c.kdbx", Some("f3"), Some("a"), Some("r1"))
            ),
            Resolution::New
        );
    }

    #[test]
    fn an_unusable_root_names_its_reason_and_never_picks_another_place() {
        for (root, reason) in [
            (JournalRoot::at(""), "empty"),
            (JournalRoot::at("relative\\folder"), "absolute"),
        ] {
            let error = root.path().unwrap_err().to_string();
            assert!(error.contains(reason), "{error}");
            assert!(root.open().is_err());
        }
    }
}
