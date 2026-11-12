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
    lineage: Vec<String>,
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
            lineage: Vec::new(),
        }
    }

    /// Evidence of an unlocked vault: the bytes that it last read or wrote, and its vault ID.
    pub fn unlocked(path: &Path, sha: Option<String>, vault_id: String) -> Self {
        Self {
            path: path.to_path_buf(),
            sha,
            root: Some(vault_id),
            lineage: Vec::new(),
        }
    }

    /// The fingerprints that the unlocked vault held before the current one (the bytes it
    /// took from the disk, then each of its own saves), oldest first. A save changes the
    /// fingerprint, and a clone shares the vault ID, so this is how the journal knows which
    /// vault the current version came from (design D12).
    pub fn with_lineage(mut self, lineage: Vec<String>) -> Self {
        self.lineage = lineage;
        self
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
            lineage: self.lineage.clone(),
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
    /// Versions that were recorded here while the file sat at another path (design D12).
    /// They name this journal as the owner of the version, but they are not part of the
    /// order of `seen`, so they never make a rollback.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elsewhere: Vec<String>,
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
    /// Fingerprints that the evidence's session held before `sha`, oldest first.
    pub lineage: Vec<String>,
    /// The evidence is a snapshot that no longer matches the file at the path (unit 1b, D10).
    pub stale: bool,
}

/// The identity rules of the design (R1 to R6 and D10 to D12), as data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Resolution {
    Same {
        index: usize,
        rolled_back: bool,
        adopt_root: bool,
    },
    /// An unseen version at the path of a binding, with no record that links it to this
    /// binding (no evidence of a save made from a recorded version), and vault IDs that do
    /// not contradict each other. A clone of the vault has the same vault ID, so the ID
    /// cannot say that the file is the same vault (FR-12, P2R2-04). The journal continues
    /// in this binding, and the caller must warn that the continuity is not confirmed.
    Continued {
        index: usize,
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
    /// Stale evidence that no active binding vouches for.
    Superseded,
    /// A version that another binding recorded shows up at a path that is bound to a
    /// different journal (D12). The record belongs to the binding that recorded the version.
    /// The version is added to that binding's `elsewhere` list, so the next observation of
    /// it, or of a save made from it, finds the same journal.
    Claimed {
        index: usize,
    },
    /// Two or more bindings recorded the version, and none of them is bound to this path
    /// (D12). No journal is chosen.
    Ambiguous {
        candidates: Vec<usize>,
    },
}

fn active(binding: &Binding) -> bool {
    binding.state == BindingState::Active
}

/// The binding recorded this version, at its own path or at another one.
fn knows(binding: &Binding, sha: &str) -> bool {
    binding
        .seen
        .iter()
        .chain(&binding.elsewhere)
        .any(|known| known == sha)
}

/// The vault IDs do not contradict each other. `strict`: both are known and equal. Clones
/// share a vault ID, so equal IDs never say which instance a file is. They can only rule an
/// instance out.
fn roots_agree(binding: &Binding, observation: &Observation, strict: bool) -> bool {
    match (&binding.root, &observation.root) {
        (Some(known), Some(now)) => known == now,
        _ => !strict,
    }
}

/// The active bindings, except `skip`, that recorded the version in the evidence. If none
/// did, those that recorded a version the evidence's session held before it (a save made
/// from a recorded version). A direct match always beats a lineage match.
fn owners(
    bindings: &[Binding],
    observation: &Observation,
    skip: Option<usize>,
    strict: bool,
    use_lineage: bool,
) -> Vec<usize> {
    let among = |versions: &[&String]| -> Vec<usize> {
        bindings
            .iter()
            .enumerate()
            .filter(|(index, binding)| {
                active(binding)
                    && Some(*index) != skip
                    && roots_agree(binding, observation, strict)
                    && versions.iter().any(|version| knows(binding, version))
            })
            .map(|(index, _)| index)
            .collect()
    };
    let direct = among(&observation.sha.iter().collect::<Vec<_>>());
    if !direct.is_empty() || !use_lineage {
        return direct;
    }
    among(&observation.lineage.iter().collect::<Vec<_>>())
}

pub(crate) fn resolve(bindings: &[Binding], observation: &Observation) -> Resolution {
    let same_path = bindings
        .iter()
        .position(|binding| active(binding) && path_key_of(binding) == observation.key);
    if observation.stale {
        return resolve_stale(bindings, observation, same_path);
    }
    let Some(index) = same_path else {
        return resolve_elsewhere(bindings, observation);
    };
    let binding = &bindings[index];
    let adopt_root = binding.root.is_none() && observation.root.is_some();
    if let Some(sha) = &observation.sha {
        if let Some(position) = binding.seen.iter().position(|seen| seen == sha) {
            return Resolution::Same {
                index,
                rolled_back: position != 0,
                adopt_root,
            };
        }
        if binding.elsewhere.contains(sha) {
            return Resolution::Same {
                index,
                rolled_back: false,
                adopt_root,
            };
        }
    }
    let direct = owners(bindings, observation, Some(index), false, false);
    let continues_here = observation
        .lineage
        .iter()
        .any(|earlier| knows(binding, earlier));
    let claimants = if !direct.is_empty() || continues_here {
        direct
    } else {
        owners(bindings, observation, Some(index), false, true)
    };
    match claimants.as_slice() {
        [] => {}
        [only] => return Resolution::Claimed { index: *only },
        _ => {
            return Resolution::Ambiguous {
                candidates: claimants,
            }
        }
    }
    // A save made from a version that the binding recorded is the vault itself. Anything
    // else that carries an unseen version is a file that changed outside AKM.
    let continued = |adopt_root: bool| {
        if continues_here {
            Resolution::Same {
                index,
                rolled_back: false,
                adopt_root,
            }
        } else {
            Resolution::Continued { index, adopt_root }
        }
    };
    match (&binding.root, &observation.root) {
        (Some(known), Some(now)) if known == now => continued(false),
        (Some(_), Some(_)) => Resolution::Replaced { index },
        (None, Some(_)) => continued(true),
        (_, None) => Resolution::Unconfirmed { index },
    }
}

/// Evidence that no longer matches the file at its path never changes the mapping (D10).
/// A path without a binding gives it nothing to stand on: a copy cannot be told from a file
/// that moved. At a bound path, the binding that recorded its version vouches for it. Only
/// when no binding did, the binding at the path vouches if the vault IDs are known and equal.
fn resolve_stale(
    bindings: &[Binding],
    observation: &Observation,
    same_path: Option<usize>,
) -> Resolution {
    let Some(at_path) = same_path else {
        return Resolution::Superseded;
    };
    let vouching = owners(bindings, observation, None, true, true);
    if vouching.contains(&at_path) {
        return Resolution::Observed { index: at_path };
    }
    match vouching.as_slice() {
        [] => {}
        [only] => return Resolution::Observed { index: *only },
        _ => {
            return Resolution::Ambiguous {
                candidates: vouching,
            }
        }
    }
    if roots_agree(&bindings[at_path], observation, true) {
        Resolution::Observed { index: at_path }
    } else {
        Resolution::Superseded
    }
}

/// No active binding is bound to this path. A file ID can say that the file moved, unless
/// another journal recorded the version it holds now. Everything else is a new place, and a
/// new place starts its own journal, a copy included (R6).
fn resolve_elsewhere(bindings: &[Binding], observation: &Observation) -> Resolution {
    let Some(file_id) = &observation.file_id else {
        return Resolution::New;
    };
    let same_file: Vec<usize> = bindings
        .iter()
        .enumerate()
        .filter(|(_, binding)| active(binding) && binding.file_id.as_ref() == Some(file_id))
        .map(|(index, _)| index)
        .collect();
    let recorded = |index: &usize| {
        observation
            .sha
            .as_ref()
            .is_some_and(|sha| knows(&bindings[*index], sha))
    };
    let by_version: Vec<usize> = same_file.iter().copied().filter(recorded).collect();
    if let [only] = by_version.as_slice() {
        return Resolution::Moved { index: *only };
    }
    if !by_version.is_empty() {
        return Resolution::New;
    }
    let version_recorded_elsewhere = |index: usize| {
        observation.sha.as_ref().is_some_and(|sha| {
            bindings
                .iter()
                .enumerate()
                .any(|(other, binding)| other != index && active(binding) && knows(binding, sha))
        })
    };
    let by_vault: Vec<usize> = same_file
        .into_iter()
        .filter(|index| {
            roots_agree(&bindings[*index], observation, true) && !version_recorded_elsewhere(*index)
        })
        .collect();
    match by_vault.as_slice() {
        [only] => Resolution::Moved { index: *only },
        _ => Resolution::New,
    }
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

/// The notice of a read at a bound path whose file holds a version that the journal never
/// recorded, with no record that links the two (FR-12, core review 1, finding 5).
fn continued_notice() -> Notice {
    Notice {
        kind: NoticeKind::IdentityUnconfirmed,
        message: "The vault file at this path holds a version that this journal never recorded, and no record links the two. It can be a copy that was changed somewhere else. This journal cannot be confirmed as the journal of this file. The next change or disclosure records the version, with a warning. Nothing was changed.".into(),
    }
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
                .is_some_and(|sha| knows(binding, sha))
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
        elsewhere: Vec::new(),
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
        binding.elsewhere.retain(|known| known != sha);
    }
    *binding != before
}

/// Records a version that was seen at another path. Returns whether the binding changed.
/// The order of `seen` is not touched: the version is no step of this binding's own file.
fn note_elsewhere(binding: &mut Binding, observation: &Observation) -> bool {
    let Some(sha) = &observation.sha else {
        return false;
    };
    if knows(binding, sha) {
        return false;
    }
    binding.elsewhere.insert(0, sha.clone());
    binding.elsewhere.truncate(SEEN_LIMIT);
    true
}

/// The journal that a record about this vault goes to.
pub(crate) struct Placement {
    pub id: String,
    /// The vault file at a bound path holds a version that this journal never saw and that
    /// no earlier record links to it. The journal continues, but the caller must say that
    /// the continuity is not confirmed (FR-12).
    pub lineage_unconfirmed: bool,
    /// The vault went back to a version that this device saw. The record of that is owed
    /// before the requested record.
    pub owes_rollback: bool,
}

fn superseded() -> Error {
    journal_error(
        "the file at this vault's path is no longer the one this session opened, so its journal cannot be confirmed from here. Reopen the vault",
    )
}

fn candidate_paths(bindings: &[Binding], candidates: &[usize]) -> String {
    candidates
        .iter()
        .map(|index| bindings[*index].path.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn ambiguous_message(bindings: &[Binding], candidates: &[usize]) -> String {
    format!(
        "this vault version was recorded in more than one journal on this device (bound to {}), so AKM cannot tell which journal this file belongs to. Nothing was written. Copy the vault to a new path to start a journal of its own",
        candidate_paths(bindings, candidates)
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
    let mut lineage_unconfirmed = false;
    let id = match resolve(&mapping.journals, observation) {
        Resolution::Continued { index, adopt_root } => {
            lineage_unconfirmed = true;
            changed = note(&mut mapping.journals[index], observation, adopt_root);
            mapping.journals[index].id.clone()
        }
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
        Resolution::Claimed { index } => {
            changed = note_elsewhere(&mut mapping.journals[index], observation);
            mapping.journals[index].id.clone()
        }
        Resolution::Ambiguous { candidates } => {
            return Err(journal_error(ambiguous_message(
                &mapping.journals,
                &candidates,
            )))
        }
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
        placement: Placement {
            id,
            lineage_unconfirmed,
            owes_rollback,
        },
    })
}

/// Runs `write` for the journal that a record about this vault goes to, with the mapping
/// locked. The only place that stores a mapping for a record.
///
/// The mapping changes in two phases: `decide` observes, and this function stores. A change
/// that settles something the person must learn is stored only after `write` succeeded, and
/// only when the record carries it. Two changes settle something: the rollback indication
/// (the observed version moves to the front of `seen`) and an unconfirmed lineage (the
/// unseen version joins `seen`, so it is never reported again). `carries_lineage` says that
/// the caller passes `Placement::lineage_unconfirmed` on to the person. If `write` fails,
/// or the record does not carry the lineage, the mapping is as it was, and the next record
/// owes the same again (FR-12, core review 1, finding 4). Every other change is stored
/// first: a binding without a journal file yet is harmless, a record without a binding is
/// not.
pub(crate) fn record<T>(
    folder: &Folder,
    evidence: &VaultEvidence,
    carries_lineage: bool,
    write: impl FnOnce(&Placement) -> Result<T>,
) -> Result<T> {
    let lock = lock_mapping(folder, true)?;
    let decision = decide(load(folder)?, &evidence.observe())?;
    let placement = &decision.placement;
    if placement.owes_rollback || placement.lineage_unconfirmed {
        let written = write(placement)?;
        let settled = placement.owes_rollback || carries_lineage;
        if decision.changed && settled {
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

/// The journal ID for a vault, creating the binding if there is none. It settles nothing:
/// when a rollback record or a lineage warning is owed, the mapping is left as it is.
pub(crate) fn place(folder: &Folder, evidence: &VaultEvidence) -> Result<String> {
    let _lock = lock_mapping(folder, true)?;
    let decision = decide(load(folder)?, &evidence.observe())?;
    let placement = &decision.placement;
    if decision.changed && !placement.owes_rollback && !placement.lineage_unconfirmed {
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
    let resolution = resolve(&mapping.journals, &observation);
    let (found, identity) = match resolution.clone() {
        Resolution::Same { index, .. } | Resolution::Moved { index } => {
            (Some(index), Identity::Confirmed)
        }
        // A read never confirms an unseen version, and never settles its warning.
        Resolution::Continued { index, .. } => {
            notices.push(continued_notice());
            (Some(index), Identity::Unconfirmed)
        }
        Resolution::Unconfirmed { index } => {
            notices.push(unconfirmed_notice());
            (Some(index), Identity::Unconfirmed)
        }
        Resolution::Observed { index } | Resolution::Claimed { index } => {
            (Some(index), Identity::Confirmed)
        }
        Resolution::Ambiguous { candidates } => {
            notices.push(Notice {
                kind: NoticeKind::IdentityUnconfirmed,
                message: format!("{}.", ambiguous_message(&mapping.journals, &candidates)),
            });
            (None, Identity::Unconfirmed)
        }
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
    if matches!(
        resolution,
        Resolution::Claimed { .. } | Resolution::Ambiguous { .. }
    ) {
        for (index, binding) in mapping.journals.iter().enumerate() {
            if active(binding) && Some(index) != found && path_key_of(binding) == observation.key {
                notices.push(Notice {
                    kind: NoticeKind::EarlierJournal,
                    message: format!(
                        "Another journal is bound to this path: {}. It belongs to the vault version that was recorded here before, and it is not merged into the one shown.",
                        folder.journal_file(&binding.id).display()
                    ),
                });
            }
        }
    }
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
            elsewhere: Vec::new(),
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
            lineage: Vec::new(),
            stale: false,
        }
    }

    fn with_lineage(mut observation: Observation, lineage: &[&str]) -> Observation {
        observation.lineage = lineage.iter().map(|sha| sha.to_string()).collect();
        observation
    }

    fn stale_of(observation: Observation) -> Observation {
        Observation {
            stale: true,
            ..observation
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
                "R2 changed elsewhere, same vault ID: the journal continues, unconfirmed",
                seen(r"C:\V\a.kdbx", None, Some("x"), Some("r1")),
                Resolution::Continued {
                    index: 0,
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

    /// Four clones of one vault family (root `r1`) and one other vault. `a` and `c` both
    /// recorded the version `a1`: `c` is a copy that was made before either changed.
    fn family() -> Vec<Binding> {
        let mut family = vec![
            binding(r"C:\V\a.kdbx", Some("fA"), Some("r1"), &["a2", "a1"]),
            binding(r"C:\V\b.kdbx", Some("fB"), Some("r1"), &["b2", "b1"]),
            binding(r"C:\V\c.kdbx", Some("fC"), Some("r1"), &["a1"]),
            binding(r"C:\V\d.kdbx", Some("fD"), Some("r1"), &["d1"]),
            binding(r"C:\V\x.kdbx", Some("fX"), Some("r9"), &["x1"]),
        ];
        family[1].elsewhere = vec!["g1".into()];
        family
    }

    #[test]
    fn d12_a_seen_version_wins_over_the_vault_id() {
        let bound = family();
        let at = |path: &str, sha: &str, root: Option<&str>| seen(path, None, Some(sha), root);
        let same = |index: usize, rolled_back: bool| Resolution::Same {
            index,
            rolled_back,
            adopt_root: false,
        };
        let cases: Vec<(&str, Observation, Resolution)> = vec![
            (
                "the reviewer's case: a clone's version replaces the original's path",
                at(r"C:\V\a.kdbx", "b1", Some("r1")),
                Resolution::Claimed { index: 1 },
            ),
            (
                "the same, in a locked read",
                at(r"C:\V\a.kdbx", "b1", None),
                Resolution::Claimed { index: 1 },
            ),
            (
                "a version that was recorded at another path before",
                at(r"C:\V\a.kdbx", "g1", Some("r1")),
                Resolution::Claimed { index: 1 },
            ),
            (
                "the clone's own file: the path decides",
                at(r"C:\V\b.kdbx", "b2", Some("r1")),
                same(1, false),
            ),
            (
                "the clone returns to its own earlier version: a rollback in its own journal",
                at(r"C:\V\b.kdbx", "b1", Some("r1")),
                same(1, true),
            ),
            (
                "the original returns to its own earlier version: a rollback in its own journal",
                at(r"C:\V\a.kdbx", "a1", Some("r1")),
                same(0, true),
            ),
            (
                "the newest version of the clone at another path is not a rollback of anyone",
                at(r"C:\V\a.kdbx", "b2", Some("r1")),
                Resolution::Claimed { index: 1 },
            ),
            (
                "a version that nobody recorded falls back to the vault ID, unconfirmed (FR-12)",
                at(r"C:\V\a.kdbx", "z", Some("r1")),
                Resolution::Continued {
                    index: 0,
                    adopt_root: false,
                },
            ),
            (
                "a version that nobody recorded, in a locked read, stays unconfirmed",
                at(r"C:\V\a.kdbx", "z", None),
                Resolution::Unconfirmed { index: 0 },
            ),
            (
                "an unrecorded version of another vault is a replacement",
                at(r"C:\V\a.kdbx", "z", Some("r2")),
                Resolution::Replaced { index: 0 },
            ),
            (
                "a version of another vault that a binding recorded",
                at(r"C:\V\a.kdbx", "x1", Some("r9")),
                Resolution::Claimed { index: 4 },
            ),
            (
                "a recorded version with a contradicting vault ID names no one",
                at(r"C:\V\a.kdbx", "x1", Some("r2")),
                Resolution::Replaced { index: 0 },
            ),
            (
                "two journals recorded the version and a third path is bound",
                at(r"C:\V\d.kdbx", "a1", Some("r1")),
                Resolution::Ambiguous {
                    candidates: vec![0, 2],
                },
            ),
            (
                "the same in a locked read",
                at(r"C:\V\d.kdbx", "a1", None),
                Resolution::Ambiguous {
                    candidates: vec![0, 2],
                },
            ),
            (
                "two journals recorded the version and one is bound to the path: the path decides",
                at(r"C:\V\c.kdbx", "a1", Some("r1")),
                same(2, false),
            ),
            (
                "and the other one, with its own order",
                at(r"C:\V\a.kdbx", "a2", Some("r1")),
                same(0, false),
            ),
            (
                "a save made from a version that one journal recorded",
                with_lineage(at(r"C:\V\a.kdbx", "z", Some("r1")), &["b1"]),
                Resolution::Claimed { index: 1 },
            ),
            (
                "a save made from a version that two journals recorded",
                with_lineage(at(r"C:\V\d.kdbx", "z", Some("r1")), &["a1"]),
                Resolution::Ambiguous {
                    candidates: vec![0, 2],
                },
            ),
            (
                "a save made from a version that the path's own journal recorded",
                with_lineage(at(r"C:\V\a.kdbx", "z", Some("r1")), &["a2"]),
                same(0, false),
            ),
            (
                "the path's journal and another one recorded the origin: the path continues",
                with_lineage(at(r"C:\V\a.kdbx", "z", Some("r1")), &["a1", "q"]),
                same(0, false),
            ),
            (
                "the version itself beats the lineage",
                with_lineage(at(r"C:\V\a.kdbx", "b1", Some("r1")), &["a2"]),
                Resolution::Claimed { index: 1 },
            ),
            (
                "a lineage nobody recorded falls back to the vault ID, unconfirmed (FR-12)",
                with_lineage(at(r"C:\V\a.kdbx", "z", Some("r1")), &["q", "p"]),
                Resolution::Continued {
                    index: 0,
                    adopt_root: false,
                },
            ),
        ];
        for (name, observation, expected) in cases {
            assert_eq!(resolve(&bound, &observation), expected, "{name}");
        }
    }

    #[test]
    fn d12_a_new_place_starts_its_own_journal_and_a_file_id_names_no_clone() {
        let bound = family();
        let cases: Vec<(&str, Observation, Resolution)> = vec![
            (
                "a fresh copy of a recorded version at a new path (unit 1b, R6)",
                seen(r"C:\V\n.kdbx", Some("fN"), Some("a1"), Some("r1")),
                Resolution::New,
            ),
            (
                "a recorded version of a clone at a new path",
                seen(r"C:\V\n.kdbx", Some("fN"), Some("b1"), Some("r1")),
                Resolution::New,
            ),
            (
                "the file moved and was saved: the file ID and the vault ID say so",
                seen(r"C:\V\n.kdbx", Some("fA"), Some("z"), Some("r1")),
                Resolution::Moved { index: 0 },
            ),
            (
                "the file moved, same bytes",
                seen(r"C:\V\n.kdbx", Some("fA"), Some("a2"), None),
                Resolution::Moved { index: 0 },
            ),
            (
                "the replaced file was renamed: its file ID is the original's, its bytes are a clone's",
                seen(r"C:\V\n.kdbx", Some("fA"), Some("b1"), Some("r1")),
                Resolution::New,
            ),
            (
                "the clone's own file moved",
                seen(r"C:\V\n.kdbx", Some("fB"), Some("b1"), Some("r1")),
                Resolution::Moved { index: 1 },
            ),
        ];
        for (name, observation, expected) in cases {
            assert_eq!(resolve(&bound, &observation), expected, "{name}");
        }
        let mut twins = family();
        twins[2].file_id = Some("fA".into());
        assert_eq!(
            resolve(
                &twins,
                &seen(r"C:\V\n.kdbx", Some("fA"), Some("a1"), Some("r1"))
            ),
            Resolution::New,
            "two bindings with the same file ID that both recorded the version: no guess"
        );
        assert_eq!(
            resolve(
                &twins,
                &seen(r"C:\V\n.kdbx", Some("fA"), Some("z"), Some("r1"))
            ),
            Resolution::New,
            "two bindings with the same file ID and vault ID: no guess"
        );
    }

    #[test]
    fn d12_stale_evidence_follows_the_binding_that_recorded_its_version() {
        let bound = family();
        let stale = |path: &str, sha: &str, root: Option<&str>, lineage: &[&str]| {
            stale_of(with_lineage(seen(path, None, Some(sha), root), lineage))
        };
        let cases: Vec<(&str, Observation, Resolution)> = vec![
            (
                "the version of the binding at the path",
                stale(r"C:\V\a.kdbx", "a1", Some("r1"), &[]),
                Resolution::Observed { index: 0 },
            ),
            (
                "the version of a clone: its journal, not the path's",
                stale(r"C:\V\a.kdbx", "b1", Some("r1"), &[]),
                Resolution::Observed { index: 1 },
            ),
            (
                "a save made from a clone's version",
                stale(r"C:\V\a.kdbx", "z", Some("r1"), &["b1"]),
                Resolution::Observed { index: 1 },
            ),
            (
                "two journals recorded it and neither is bound to the path",
                stale(r"C:\V\d.kdbx", "a1", Some("r1"), &[]),
                Resolution::Ambiguous {
                    candidates: vec![0, 2],
                },
            ),
            (
                "two journals recorded it and one is bound to the path",
                stale(r"C:\V\c.kdbx", "a1", Some("r1"), &[]),
                Resolution::Observed { index: 2 },
            ),
            (
                "nobody recorded it: the vault ID and the path vouch",
                stale(r"C:\V\a.kdbx", "z", Some("r1"), &[]),
                Resolution::Observed { index: 0 },
            ),
            (
                "no binding at the path: a copy cannot be told from a file that moved",
                stale(r"C:\V\gone.kdbx", "a2", Some("r1"), &[]),
                Resolution::Superseded,
            ),
            (
                "a contradicting vault ID vouches for no one",
                stale(r"C:\V\a.kdbx", "b1", Some("r2"), &[]),
                Resolution::Superseded,
            ),
            (
                "no vault ID vouches for no one",
                stale(r"C:\V\a.kdbx", "b1", None, &[]),
                Resolution::Superseded,
            ),
            (
                "nobody recorded it and no binding is at the path",
                stale(r"C:\V\gone.kdbx", "z", Some("r1"), &[]),
                Resolution::Superseded,
            ),
        ];
        for (name, observation, expected) in cases {
            assert_eq!(resolve(&bound, &observation), expected, "{name}");
        }
        let mut detached = family();
        detached[1].state = BindingState::Detached;
        assert_eq!(
            resolve(&detached, &stale(r"C:\V\a.kdbx", "b1", Some("r1"), &[])),
            Resolution::Observed { index: 0 },
            "a detached binding never vouches; the path's vault ID does"
        );
    }

    #[test]
    fn a_binding_without_a_vault_id_adopts_the_first_one_and_detached_bindings_never_match() {
        let mut bound = [binding(r"C:\V\a.kdbx", None, None, &["old"])];
        assert_eq!(
            resolve(&bound, &seen(r"C:\V\a.kdbx", None, Some("x"), Some("r1"))),
            Resolution::Continued {
                index: 0,
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
