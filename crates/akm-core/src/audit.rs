use std::collections::VecDeque;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::journal::{self, Identity, JournalRoot, Notice, VaultEvidence};
use crate::private_fs::{self, OpenMode};

/// One line of a journal. The journal lives in a private per-device folder, never beside
/// the vault (unit 1b). A record holds the least that makes it useful: what happened, when,
/// and the native ID of the key it was about. Titles, projects, environments, scopes and
/// paths are not recorded; a reader that has the vault unlocked looks them up by ID.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditEntry {
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub prev: String,
    pub hash: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditState {
    Absent,
    Empty,
    Consistent,
    Corrupt,
}

/// What is wrong with a damaged journal, as observed. It names no cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Damage {
    TruncatedTail,
    MalformedJson,
    BrokenLink,
    HashMismatch,
}

impl Damage {
    pub fn describe(self) -> &'static str {
        match self {
            Self::TruncatedTail => "the last line has no line end",
            Self::MalformedJson => "a line is not a valid record",
            Self::BrokenLink => "a record does not follow the one before it",
            Self::HashMismatch => "a record does not match its hash",
        }
    }
}

pub struct AuditSnapshot {
    pub state: AuditState,
    pub entries: Vec<AuditEntry>,
    pub total_entries: usize,
    pub problem_line: Option<usize>,
    pub broken_at: Option<u64>,
    pub damage: Option<Damage>,
    pub identity: Identity,
    pub notices: Vec<Notice>,
    /// The journal file, or the journal folder when no journal exists yet.
    pub location: PathBuf,
}

impl AuditSnapshot {
    fn new(state: AuditState, location: PathBuf) -> Self {
        Self {
            state,
            entries: Vec::new(),
            total_entries: 0,
            problem_line: None,
            broken_at: None,
            damage: None,
            identity: Identity::Confirmed,
            notices: Vec::new(),
            location,
        }
    }

    pub fn into_entries(self) -> Result<Vec<AuditEntry>> {
        if self.state == AuditState::Corrupt {
            return Err(Error::Invalid(
                "Activity journal data is damaged; original bytes were preserved.".into(),
            ));
        }
        Ok(self.entries)
    }

    /// Check consistency only. An unanchored chain cannot authenticate records or prove completeness.
    pub fn verification(&self) -> Result<Option<u64>> {
        match self.state {
            AuditState::Consistent => Ok(None),
            AuditState::Corrupt if self.broken_at.is_some() => Ok(self.broken_at),
            AuditState::Corrupt => Err(Error::Invalid(
                "Activity journal data is malformed; original bytes were preserved.".into(),
            )),
            AuditState::Absent | AuditState::Empty => Err(Error::Invalid(
                "No activity records are available. Completeness is unknown.".into(),
            )),
        }
    }
}

/// The action of the first record of a journal that replaced a damaged one.
pub const RECOVERED: &str = "journal-recovered";

/// What a recovery kept and what it started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    /// The damaged journal, unchanged, under its dated name beside the new journal.
    pub kept: PathBuf,
    /// The size of the kept file.
    pub bytes: u64,
    /// The SHA-256 of the kept file, in hex. The first record of the new journal states it.
    pub sha256: String,
}

/// The steps of a recovery at which a test can make the file system fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryStep {
    CreateNew,
    SyncNew,
    RenameAside,
    Install,
    Restore,
}

#[cfg(test)]
thread_local! {
    static FAIL_RECOVERY_AT: std::cell::RefCell<Vec<RecoveryStep>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Makes the next arrival at `step` fail, once.
#[cfg(test)]
pub(crate) fn fail_recovery_at(step: RecoveryStep) {
    FAIL_RECOVERY_AT.with(|fail| fail.borrow_mut().push(step));
}

#[cfg(test)]
type Barrier = (RecoveryStep, Box<dyn FnOnce()>);

#[cfg(test)]
thread_local! {
    static AT_RECOVERY_STEP: std::cell::RefCell<Vec<Barrier>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Runs `action` at the next arrival at `step`, once, before the step: a barrier where a test
/// acts as another writer.
#[cfg(test)]
pub(crate) fn at_recovery_step(step: RecoveryStep, action: impl FnOnce() + 'static) {
    AT_RECOVERY_STEP.with(|barriers| barriers.borrow_mut().push((step, Box::new(action))));
}

/// Production does nothing here. A test makes a step fail.
fn reach(step: RecoveryStep) -> Result<()> {
    #[cfg(test)]
    {
        let barrier = AT_RECOVERY_STEP.with(|barriers| {
            let mut barriers = barriers.borrow_mut();
            let position = barriers.iter().position(|(planned, _)| *planned == step)?;
            Some(barriers.remove(position).1)
        });
        if let Some(action) = barrier {
            action();
        }
        let injected = FAIL_RECOVERY_AT.with(|fail| {
            let mut fail = fail.borrow_mut();
            let position = fail.iter().position(|planned| *planned == step)?;
            fail.remove(position);
            Some(())
        });
        if injected.is_some() {
            return Err(std::io::Error::other(format!("injected failure at {step:?}")).into());
        }
    }
    let _ = step;
    Ok(())
}

/// Removes the pending file that this call created, and says so in the error if it cannot.
fn with_leftover(error: Error, pending: &Path) -> Error {
    match fs::remove_file(pending) {
        Ok(()) => error,
        Err(_) => Error::Journal(format!(
            "{error}. The file {} was left behind and may be deleted",
            pending.display()
        )),
    }
}

/// A record to append. The chain adds the sequence number, the time and the links.
pub(crate) struct NewRecord<'a> {
    pub action: &'a str,
    pub key_id: Option<&'a str>,
    pub detail: Option<&'a str>,
}

/// How a chain file is opened. `PlainStore` is for the chain format tests; the journal
/// folder uses `PrivateStore`, which checks permissions and links on every open.
trait Store: Send + Sync {
    fn read(&self) -> Result<Option<File>>;
    fn lock(&self, exclusive: bool) -> Result<File>;
    fn append(&self) -> Result<File>;
    /// A new file in the same folder, for writing. It fails if the name is taken.
    fn create_new(&self, path: &Path) -> Result<File>;
}

fn lock_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

fn take_lock(file: File, exclusive: bool) -> Result<File> {
    if exclusive {
        file.lock_exclusive()?;
    } else {
        FileExt::lock_shared(&file)?;
    }
    Ok(file)
}

struct PlainStore(PathBuf);

impl PlainStore {
    fn parent(&self) -> Result<()> {
        if let Some(parent) = self
            .0
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }
}

impl Store for PlainStore {
    fn read(&self) -> Result<Option<File>> {
        match File::open(&self.0) {
            Ok(file) => Ok(Some(file)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn lock(&self, exclusive: bool) -> Result<File> {
        self.parent()?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_path(&self.0))?;
        take_lock(file, exclusive)
    }

    fn append(&self) -> Result<File> {
        self.parent()?;
        Ok(OpenOptions::new().create(true).append(true).open(&self.0)?)
    }

    fn create_new(&self, path: &Path) -> Result<File> {
        self.parent()?;
        Ok(OpenOptions::new().create_new(true).write(true).open(path)?)
    }
}

struct PrivateStore(PathBuf);

impl Store for PrivateStore {
    fn read(&self) -> Result<Option<File>> {
        private_fs::open_file(&self.0, OpenMode::Read)
    }

    fn lock(&self, exclusive: bool) -> Result<File> {
        let file = private_fs::open_file(&lock_path(&self.0), OpenMode::LockCreate)?
            .ok_or_else(|| Error::Journal("cannot create the journal lock".into()))?;
        take_lock(file, exclusive)
    }

    fn append(&self) -> Result<File> {
        private_fs::open_file(&self.0, OpenMode::Append)?
            .ok_or_else(|| Error::Journal("cannot open the journal".into()))
    }

    fn create_new(&self, path: &Path) -> Result<File> {
        private_fs::open_file(path, OpenMode::CreateNew)?
            .ok_or_else(|| Error::Journal("cannot create the new journal file".into()))
    }
}

fn genesis(journal_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"akm-journal-v1\n");
    hasher.update(journal_id.as_bytes());
    hex::encode(hasher.finalize())
}

/// One journal file: an append-only hash chain. The first record links to a value that is
/// derived from the journal ID, so a file moved under another journal's name fails at record 1.
pub struct ChainFile {
    path: PathBuf,
    genesis: String,
    store: Box<dyn Store>,
}

impl ChainFile {
    /// A chain file at an ordinary path, without permission checks. For tests of the format.
    pub fn at(path: impl Into<PathBuf>, journal_id: &str) -> Self {
        let path = path.into();
        Self {
            store: Box::new(PlainStore(path.clone())),
            path,
            genesis: genesis(journal_id),
        }
    }

    pub(crate) fn private(path: PathBuf, journal_id: &str) -> Self {
        Self {
            store: Box::new(PrivateStore(path.clone())),
            path,
            genesis: genesis(journal_id),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(&self, action: &str, key_id: Option<&str>, detail: Option<&str>) -> Result<()> {
        self.append_all(&[NewRecord {
            action,
            key_id,
            detail,
        }])
    }

    pub(crate) fn append_all(&self, records: &[NewRecord<'_>]) -> Result<()> {
        let _lock = self.store.lock(true)?;
        let (seq, prev) = self.tail()?.unwrap_or((0, self.genesis.clone()));
        let bytes = self.encode(records, &prev, seq, Utc::now())?;
        let mut file = self.store.append()?;
        file.write_all(&bytes)?;
        #[cfg(test)]
        if FAIL_NEXT_SYNC.with(|fail| fail.replace(false)) {
            return Err(std::io::Error::other("injected journal sync failure").into());
        }
        file.sync_data()?;
        Ok(())
    }

    /// The lines of `records`, chained after `prev` (the hash of record number `seq`).
    fn encode(
        &self,
        records: &[NewRecord<'_>],
        prev: &str,
        mut seq: u64,
        first_time: DateTime<Utc>,
    ) -> Result<Vec<u8>> {
        let mut prev = prev.to_string();
        let mut bytes = Vec::new();
        for (index, record) in records.iter().enumerate() {
            seq += 1;
            let ts = if index == 0 { first_time } else { Utc::now() };
            let hash = digest(seq, &ts, record.action, record.key_id, record.detail, &prev);
            let entry = AuditEntry {
                seq,
                ts,
                action: record.action.to_string(),
                key_id: record.key_id.map(str::to_string),
                detail: record.detail.map(str::to_string),
                prev: std::mem::replace(&mut prev, hash.clone()),
                hash,
            };
            serde_json::to_writer(&mut bytes, &entry)?;
            bytes.push(b'\n');
        }
        Ok(bytes)
    }

    pub fn read(&self, limit: usize) -> Result<Vec<AuditEntry>> {
        self.snapshot(limit)?.into_entries()
    }

    pub fn verify(&self) -> Result<Option<u64>> {
        self.snapshot(0)?.verification()
    }

    /// Read and validate the displayed snapshot under one cooperating-reader lock.
    pub fn snapshot(&self, limit: usize) -> Result<AuditSnapshot> {
        let Some(file) = self.store.read()? else {
            return Ok(AuditSnapshot::new(AuditState::Absent, self.path.clone()));
        };
        let _lock = self.store.lock(false)?;
        self.chain(file, limit)
    }

    fn chain(&self, file: File, limit: usize) -> Result<AuditSnapshot> {
        let mut reader = BufReader::new(file);
        let mut retained = VecDeque::new();
        let mut snapshot = AuditSnapshot::new(AuditState::Empty, self.path.clone());
        let mut physical_line = 0;
        let mut buffer = Vec::new();
        let mut prev = self.genesis.clone();
        let mut expected_seq = 1u64;
        loop {
            buffer.clear();
            if reader.read_until(b'\n', &mut buffer)? == 0 {
                break;
            }
            physical_line += 1;
            let terminated = buffer.last() == Some(&b'\n');
            // A blank record has a line terminator and only JSON whitespace.
            // Broader ASCII whitespace includes controls JSON does not permit.
            if terminated
                && buffer
                    .iter()
                    .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
            {
                continue;
            }
            let entry: AuditEntry = match serde_json::from_slice(&buffer) {
                Ok(entry) => entry,
                Err(_) => {
                    snapshot.state = AuditState::Corrupt;
                    snapshot.problem_line = Some(physical_line);
                    // Untrusted, like every damaged record's sequence; displays say so.
                    snapshot.broken_at = declared_seq(&buffer);
                    snapshot.damage = Some(if terminated {
                        Damage::MalformedJson
                    } else {
                        Damage::TruncatedTail
                    });
                    break;
                }
            };
            let damage = if !terminated {
                Some(Damage::TruncatedTail)
            } else if entry.seq != expected_seq || entry.prev != prev {
                Some(Damage::BrokenLink)
            } else if entry.hash
                != digest(
                    entry.seq,
                    &entry.ts,
                    &entry.action,
                    entry.key_id.as_deref(),
                    entry.detail.as_deref(),
                    &entry.prev,
                )
            {
                Some(Damage::HashMismatch)
            } else {
                None
            };
            if let Some(damage) = damage {
                snapshot.state = AuditState::Corrupt;
                snapshot.problem_line = Some(physical_line);
                snapshot.broken_at = Some(entry.seq);
                snapshot.damage = Some(damage);
                break;
            }
            prev.clone_from(&entry.hash);
            expected_seq += 1;
            snapshot.total_entries += 1;
            snapshot.state = AuditState::Consistent;
            if limit > 0 {
                retained.push_back(entry);
                if retained.len() > limit {
                    retained.pop_front();
                }
            }
        }
        snapshot.entries = retained.into_iter().collect();
        Ok(snapshot)
    }

    /// Keeps a damaged chain file aside under a dated name, byte for byte, and starts a new
    /// chain in its place. The first record of the new chain states the gap and the SHA-256 of
    /// the kept file; `then` follows it. The chain file must be damaged.
    ///
    /// The order is what keeps the old journal usable as evidence after every failure. The
    /// new chain is written and synced to a file of its own first. Then the damaged file is
    /// renamed, and only then the new file takes its name. A failure before the first
    /// rename leaves the damaged file where it was. A failure of the second rename puts the
    /// damaged file back. Only a failure of both leaves the name free, and the error says so.
    /// Only a file that this call created is ever removed.
    pub(crate) fn recover(&self, now: DateTime<Utc>, then: &[NewRecord<'_>]) -> Result<Recovery> {
        let _lock = self.store.lock(true)?;
        let Some(file) = self.store.read()? else {
            return Err(Error::Journal(
                "there is no journal file to keep aside".into(),
            ));
        };
        if self.chain(file, 0)?.state != AuditState::Corrupt {
            return Err(Error::Invalid(
                "The activity journal is not damaged, so nothing was moved.".into(),
            ));
        }
        let (bytes, sha256) = self.fingerprint()?;
        let kept = self.free_kept_path(now)?;
        let kept_name = kept
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let detail = format!(
            "history before this record cannot be proved complete; the damaged journal ({bytes} bytes, SHA-256 {sha256}) is kept as {kept_name}"
        );
        let mut records = vec![NewRecord {
            action: RECOVERED,
            key_id: None,
            detail: Some(&detail),
        }];
        records.extend(then.iter().map(|record| NewRecord {
            action: record.action,
            key_id: record.key_id,
            detail: record.detail,
        }));
        let new_chain = self.encode(&records, &self.genesis, 0, now)?;

        let pending = self.pending_path();
        reach(RecoveryStep::CreateNew)?;
        let mut created = self.store.create_new(&pending)?;
        // From here the pending file is ours, and only this call removes it.
        let written = reach(RecoveryStep::SyncNew)
            .and_then(|()| Ok(created.write_all(&new_chain)?))
            .and_then(|()| Ok(created.sync_data()?));
        drop(created);
        if let Err(error) = written {
            return Err(with_leftover(error, &pending));
        }

        // Every move below refuses a taken destination (`move_new`): the kept name was chosen
        // by an absence check, and the journal name is free only because the damaged journal
        // moved away, so another writer can take either name in between (core review 1,
        // finding 8). The new chain already names the kept file, so a taken name refuses.
        let aside = reach(RecoveryStep::RenameAside)
            .and_then(|()| private_fs::move_new(&self.path, &kept).map_err(Error::from));
        if let Err(error) = aside {
            return Err(with_leftover(
                Error::Journal(format!(
                    "the damaged journal could not be kept aside as {kept_name}, so nothing was changed; try again ({error})"
                )),
                &pending,
            ));
        }
        let installed = reach(RecoveryStep::Install)
            .and_then(|()| private_fs::move_new(&pending, &self.path).map_err(Error::from));
        if let Err(error) = installed {
            let restored = reach(RecoveryStep::Restore)
                .and_then(|()| private_fs::move_new(&kept, &self.path).map_err(Error::from));
            return Err(match restored {
                Ok(()) => with_leftover(
                    Error::Journal(format!(
                        "the new journal could not be started, so the damaged journal was put back ({error})"
                    )),
                    &pending,
                ),
                Err(restore) => Error::Journal(format!(
                    "the new journal could not be started ({error}) and the damaged journal could not be put back ({restore}). The damaged journal is kept as {}. The new journal is {}. Move one of them to {} to continue",
                    kept.display(),
                    pending.display(),
                    self.path.display()
                )),
            });
        }
        Ok(Recovery {
            kept,
            bytes,
            sha256,
        })
    }

    /// The size and the SHA-256 of the file as it is now.
    fn fingerprint(&self) -> Result<(u64, String)> {
        let Some(file) = self.store.read()? else {
            return Err(Error::Journal("the journal file disappeared".into()));
        };
        let mut reader = BufReader::new(file);
        let mut hasher = Sha256::new();
        let mut bytes = 0u64;
        let mut block = [0u8; 8192];
        loop {
            let read = std::io::Read::read(&mut reader, &mut block)?;
            if read == 0 {
                break;
            }
            hasher.update(&block[..read]);
            bytes += read as u64;
        }
        Ok((bytes, hex::encode(hasher.finalize())))
    }

    /// `<journal>.damaged-<UTC time>`, with a number added if that name is taken. The name does
    /// not end in `.jsonl`, so nothing reads it as a journal.
    fn free_kept_path(&self, now: DateTime<Utc>) -> Result<PathBuf> {
        let stamp = now.format("%Y%m%dT%H%M%SZ");
        for number in 1..=100u32 {
            let mut name = self.path.as_os_str().to_os_string();
            name.push(if number == 1 {
                format!(".damaged-{stamp}")
            } else {
                format!(".damaged-{stamp}-{number}")
            });
            let candidate = PathBuf::from(name);
            match fs::symlink_metadata(&candidate) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(candidate),
                Err(error) => return Err(error.into()),
                Ok(_) => {}
            }
        }
        Err(Error::Journal(
            "too many kept journals have the same time stamp".into(),
        ))
    }

    fn pending_path(&self) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(format!(".recovering-{}", uuid::Uuid::new_v4().simple()));
        PathBuf::from(name)
    }

    // The caller holds the exclusive lock.
    fn tail(&self) -> Result<Option<(u64, String)>> {
        let Some(file) = self.store.read()? else {
            return Ok(None);
        };
        let snapshot = self.chain(file, 1)?;
        if snapshot.state == AuditState::Corrupt {
            let sequence = snapshot
                .broken_at
                .map(|seq| format!(", declared sequence {seq} (not verified)"))
                .unwrap_or_default();
            let what = snapshot.damage.map(Damage::describe).unwrap_or("damaged");
            return Err(Error::Journal(format!(
                "the journal is damaged at physical line {} ({what}){sequence}; append refused and original bytes preserved.",
                snapshot.problem_line.unwrap_or(0),
            )));
        }
        Ok(snapshot
            .entries
            .into_iter()
            .last()
            .map(|entry| (entry.seq, entry.hash)))
    }
}

/// The journal of one vault: a private per-device chain file that the mapping selects by
/// evidence about the vault (unit 1b, design D2).
pub struct AuditLog {
    root: JournalRoot,
    evidence: VaultEvidence,
}

impl AuditLog {
    pub fn new(root: JournalRoot, evidence: VaultEvidence) -> Self {
        Self { root, evidence }
    }

    /// The journal of a vault that is not unlocked, in the default root. Reads only.
    pub fn for_vault(vault_path: &Path) -> Self {
        Self::new(
            JournalRoot::from_environment(),
            VaultEvidence::from_file(vault_path),
        )
    }

    pub fn root(&self) -> &JournalRoot {
        &self.root
    }

    /// Appends one record. Creates the private folder, the binding and the journal when
    /// they do not exist. Refuses when the identity of the vault cannot be confirmed.
    pub fn append(&self, action: &str, key_id: Option<&str>, detail: Option<&str>) -> Result<()> {
        self.append_reporting(action, key_id, detail).map(|_| ())
    }

    /// Like [`AuditLog::append`]. The answer is `true` when the record went into a journal
    /// that continues, but whose link to this vault file is not confirmed: the file holds a
    /// version that the journal never saw (FR-12).
    pub(crate) fn append_reporting(
        &self,
        action: &str,
        key_id: Option<&str>,
        detail: Option<&str>,
    ) -> Result<bool> {
        let folder = self.root.open_or_create()?;
        journal::record(&folder, &self.evidence, |placement| {
            let chain = ChainFile::private(folder.journal_file(&placement.id), &placement.id);
            let mut records = Vec::new();
            if placement.owes_rollback {
                records.push(NewRecord {
                    action: "vault-rolled-back",
                    key_id: None,
                    detail: Some("vault content matches an earlier version that this device saw"),
                });
            }
            records.push(NewRecord {
                action,
                key_id,
                detail,
            });
            chain.append_all(&records)?;
            Ok(placement.lineage_unconfirmed)
        })
    }

    pub fn read(&self, limit: usize) -> Result<Vec<AuditEntry>> {
        self.snapshot(limit)?.into_entries()
    }

    pub fn verify(&self) -> Result<Option<u64>> {
        self.snapshot(0)?.verification()
    }

    /// A deliberate recovery from a damaged journal. It touches the journal of this vault and
    /// no other file of the folder. The damaged file is kept under a dated name beside the
    /// journal, byte for byte, and a new journal starts with one record that states the gap and
    /// the SHA-256 of the kept file. Appends and disclosure work again after that. A journal
    /// that is not damaged is refused. A failure at any step leaves the damaged journal as
    /// evidence, in place.
    pub fn recover_damaged(&self) -> Result<Recovery> {
        self.recover_damaged_at(Utc::now())
    }

    pub fn recover_damaged_at(&self, now: DateTime<Utc>) -> Result<Recovery> {
        let Some(folder) = self.root.open()? else {
            return Err(Error::Journal("there is no journal to recover".into()));
        };
        journal::record(&folder, &self.evidence, |placement| {
            let chain = ChainFile::private(folder.journal_file(&placement.id), &placement.id);
            let owed = placement.owes_rollback.then_some(NewRecord {
                action: "vault-rolled-back",
                key_id: None,
                detail: Some("vault content matches an earlier version that this device saw"),
            });
            chain.recover(now, owed.as_slice())
        })
    }

    /// Reads the journal and the notices about it. Creates nothing and changes nothing.
    pub fn snapshot(&self, limit: usize) -> Result<AuditSnapshot> {
        let located = journal::locate(&self.root, &self.evidence)?;
        let mut snapshot = match (&located.folder, &located.id) {
            (Some(folder), Some(id)) => {
                ChainFile::private(folder.journal_file(id), id).snapshot(limit)?
            }
            (folder, _) => AuditSnapshot::new(
                AuditState::Absent,
                folder
                    .as_ref()
                    .map(|folder| folder.path().to_path_buf())
                    .or_else(|| self.root.path().ok().map(Path::to_path_buf))
                    .unwrap_or_default(),
            ),
        };
        snapshot.identity = located.identity;
        snapshot.notices = located.notices;
        Ok(snapshot)
    }

    /// The journal file of this vault, if one is bound. Creates nothing.
    pub fn journal_path(&self) -> Result<Option<PathBuf>> {
        let located = journal::locate(&self.root, &self.evidence)?;
        Ok(match (&located.folder, &located.id) {
            (Some(folder), Some(id)) => Some(folder.journal_file(id)),
            _ => None,
        })
    }

    /// Creates the folder and the binding and returns the journal path, for tests that
    /// need to place bytes in a journal before the first record.
    #[doc(hidden)]
    pub fn create_journal_path(&self) -> Result<PathBuf> {
        let folder = self.root.open_or_create()?;
        let id = journal::place(&folder, &self.evidence)?;
        Ok(folder.journal_file(&id))
    }
}

/// The `seq` a record declares when typed parsing failed (1A-R5-06). `Some` only for a
/// top-level JSON object with exactly one `seq` member (names compared after unescaping)
/// whose value is an integer literal in `0..=u64::MAX`. Nothing else is read or kept.
fn declared_seq(line: &[u8]) -> Option<u64> {
    struct Declared(Option<u64>);

    impl<'de> Deserialize<'de> for Declared {
        fn deserialize<D: Deserializer<'de>>(
            deserializer: D,
        ) -> std::result::Result<Self, D::Error> {
            struct Members;

            impl<'de> Visitor<'de> for Members {
                type Value = Declared;

                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("a JSON object")
                }

                fn visit_map<A: MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> std::result::Result<Declared, A::Error> {
                    // Count every `seq` before deciding: a map would keep only one duplicate.
                    let mut count = 0usize;
                    let mut value = None;
                    while let Some(name) = map.next_key::<String>()? {
                        if name == "seq" {
                            count += 1;
                            value = match map.next_value::<serde_json::Value>()? {
                                serde_json::Value::Number(number) if number.is_u64() => {
                                    number.as_u64()
                                }
                                _ => None,
                            };
                        } else {
                            map.next_value::<IgnoredAny>()?;
                        }
                    }
                    Ok(Declared(if count == 1 { value } else { None }))
                }
            }

            deserializer.deserialize_map(Members)
        }
    }

    serde_json::from_slice::<Declared>(line)
        .ok()
        .and_then(|declared| declared.0)
}

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_SYNC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn fail_next_sync() {
    FAIL_NEXT_SYNC.with(|fail| fail.set(true));
}

fn digest(
    seq: u64,
    ts: &DateTime<Utc>,
    action: &str,
    key_id: Option<&str>,
    detail: Option<&str>,
    prev: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prev.as_bytes());
    hasher.update(b"\n");
    hasher.update(seq.to_string().as_bytes());
    hasher.update(b"\n");
    hasher.update(ts.to_rfc3339().as_bytes());
    hasher.update(b"\n");
    hasher.update(action.as_bytes());
    hasher.update(b"\n");
    hasher.update(key_id.unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(detail.unwrap_or("").as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    use crate::journal::{JournalRoot, VaultEvidence};
    use chrono::TimeZone;

    struct Fixture {
        directory: tempfile::TempDir,
        log: AuditLog,
    }

    impl Fixture {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let vault = directory.path().join("vault.kdbx");
            fs::write(&vault, b"fake vault bytes").unwrap();
            let log = AuditLog::new(
                JournalRoot::at(directory.path().join("journal")),
                VaultEvidence::from_file(&vault),
            );
            Self { directory, log }
        }

        fn journal(&self) -> PathBuf {
            self.log.journal_path().unwrap().expect("a bound journal")
        }

        /// A journal of three records, then a line that is no record.
        fn damaged(&self) -> Vec<u8> {
            for action in ["add-completed", "reveal-requested", "copy-requested"] {
                self.log.append(action, None, Some("fake")).unwrap();
            }
            let path = self.journal();
            let mut file = OpenOptions::new().append(true).open(&path).unwrap();
            file.write_all(b"{this is not a record}\n").unwrap();
            drop(file);
            fs::read(&path).unwrap()
        }

        /// Every file of the journal folder with its bytes, to compare before and after.
        fn folder(&self) -> Vec<(String, Vec<u8>)> {
            let mut files: Vec<_> = fs::read_dir(self.directory.path().join("journal"))
                .unwrap()
                .map(|entry| {
                    let entry = entry.unwrap();
                    (
                        entry.file_name().to_string_lossy().into_owned(),
                        fs::read(entry.path()).unwrap_or_default(),
                    )
                })
                .collect();
            files.sort();
            files
        }
    }

    fn then() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 9, 10, 15, 0).unwrap()
    }

    #[test]
    fn fr14_a_damaged_journal_is_kept_byte_for_byte_and_a_new_one_starts_with_the_gap() {
        let fixture = Fixture::new();
        let damaged = fixture.damaged();
        let path = fixture.journal();
        assert!(
            fixture.log.append("reveal-requested", None, None).is_err(),
            "appends are refused while the journal is damaged"
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            damaged,
            "the refusal changed nothing"
        );

        let recovery = fixture.log.recover_damaged_at(then()).unwrap();

        assert_eq!(
            fs::read(&recovery.kept).unwrap(),
            damaged,
            "kept byte for byte"
        );
        assert_eq!(recovery.bytes, damaged.len() as u64);
        assert_eq!(recovery.sha256, hex::encode(Sha256::digest(&damaged)));
        assert_eq!(
            recovery.kept.file_name().unwrap().to_string_lossy(),
            format!(
                "{}.damaged-20261109T101500Z",
                path.file_name().unwrap().to_string_lossy()
            )
        );
        assert_eq!(recovery.kept.parent(), path.parent(), "beside the journal");
        let entries = fixture.log.read(10).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].seq, 1);
        assert_eq!(entries[0].action, RECOVERED);
        let detail = entries[0].detail.as_deref().unwrap();
        assert!(detail.contains(&recovery.sha256), "{detail}");
        assert!(detail.contains("cannot be proved complete"), "{detail}");
        assert!(
            detail.contains(
                &recovery
                    .kept
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            ),
            "{detail}"
        );
        assert_eq!(
            fixture.log.verify().unwrap(),
            None,
            "the new chain is consistent"
        );

        fixture
            .log
            .append("reveal-requested", None, Some("after"))
            .unwrap();
        let entries = fixture.log.read(10).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|e| e.action.as_str())
                .collect::<Vec<_>>(),
            [RECOVERED, "reveal-requested"]
        );
    }

    #[test]
    fn fr14_a_journal_that_is_not_damaged_is_refused_and_nothing_moves() {
        let fixture = Fixture::new();
        fixture.log.append("add-completed", None, None).unwrap();
        let before = fixture.folder();

        let error = fixture.log.recover_damaged_at(then()).unwrap_err();

        assert!(error.to_string().contains("not damaged"), "{error}");
        assert_eq!(fixture.folder(), before);
    }

    #[test]
    fn fr14_without_a_journal_nothing_is_created() {
        let fixture = Fixture::new();

        assert!(fixture.log.recover_damaged_at(then()).is_err());
        assert!(!fixture.directory.path().join("journal").exists());
    }

    #[test]
    fn fr14_a_failure_at_each_step_leaves_the_damaged_journal_in_place_and_blocked() {
        for step in [
            RecoveryStep::CreateNew,
            RecoveryStep::SyncNew,
            RecoveryStep::RenameAside,
            RecoveryStep::Install,
        ] {
            let fixture = Fixture::new();
            let damaged = fixture.damaged();
            let before = fixture.folder();
            fail_recovery_at(step);

            let error = fixture.log.recover_damaged_at(then()).unwrap_err();

            assert!(error.to_string().contains("injected"), "{step:?}: {error}");
            assert_eq!(
                fixture.folder(),
                before,
                "{step:?}: the folder is as it was: no kept file, no leftover"
            );
            assert_eq!(fs::read(fixture.journal()).unwrap(), damaged, "{step:?}");
            assert!(
                fixture.log.append("reveal-requested", None, None).is_err(),
                "{step:?}: disclosure stays blocked"
            );
            // A retry after the cause is gone works.
            let recovery = fixture.log.recover_damaged_at(then()).unwrap();
            assert_eq!(fs::read(recovery.kept).unwrap(), damaged, "{step:?}");
            fixture.log.append("reveal-requested", None, None).unwrap();
        }
    }

    #[test]
    fn fr14_when_the_damaged_journal_cannot_be_put_back_the_error_names_both_files() {
        let fixture = Fixture::new();
        let damaged = fixture.damaged();
        fail_recovery_at(RecoveryStep::Install);
        fail_recovery_at(RecoveryStep::Restore);

        let error = fixture
            .log
            .recover_damaged_at(then())
            .unwrap_err()
            .to_string();

        assert!(error.contains(".damaged-20261109T101500Z"), "{error}");
        assert!(error.contains(".recovering-"), "{error}");
        let names: Vec<_> = fixture.folder().into_iter().map(|(name, _)| name).collect();
        let kept = names
            .iter()
            .find(|name| name.contains(".damaged-"))
            .expect("the damaged journal is kept");
        let kept_bytes = fs::read(fixture.directory.path().join("journal").join(kept)).unwrap();
        assert_eq!(kept_bytes, damaged, "the evidence is intact");
    }

    /// Core review 1, finding 8: another writer creates a file at the kept name after
    /// recovery chose it. The file survives byte for byte, recovery refuses and changes
    /// nothing, and a retry keeps the damaged journal under the next free name.
    #[test]
    fn fr14_a_file_made_at_the_kept_name_after_it_was_chosen_survives() {
        let fixture = Fixture::new();
        let damaged = fixture.damaged();
        let path = fixture.journal();
        let taken = PathBuf::from(format!("{}.damaged-20261109T101500Z", path.display()));
        let sentinel = taken.clone();
        at_recovery_step(RecoveryStep::RenameAside, move || {
            fs::write(&sentinel, b"another writer's file").unwrap();
        });

        let refused = fixture.log.recover_damaged_at(then());

        assert_eq!(fs::read(&taken).unwrap(), b"another writer's file");
        let error = refused
            .expect_err("recovery must not replace the file")
            .to_string();
        assert!(error.contains("nothing was changed"), "{error}");
        assert_eq!(fs::read(&path).unwrap(), damaged, "the journal is in place");
        assert!(fixture.log.append("reveal-requested", None, None).is_err());
        assert!(
            !fixture
                .folder()
                .iter()
                .any(|(name, _)| name.contains(".recovering-")),
            "no pending file is left"
        );

        let recovery = fixture.log.recover_damaged_at(then()).unwrap();
        assert!(
            recovery
                .kept
                .to_string_lossy()
                .ends_with("damaged-20261109T101500Z-2"),
            "{}",
            recovery.kept.display()
        );
        assert_eq!(fs::read(&recovery.kept).unwrap(), damaged);
        assert_eq!(fs::read(&taken).unwrap(), b"another writer's file");
    }

    /// Core review 1, finding 8: another writer creates a file at the journal name in the
    /// moment after the damaged journal moved aside. The file survives, the new chain does
    /// not replace it, and the error names the kept journal and the new one.
    #[test]
    fn fr14_a_file_made_at_the_journal_name_during_install_survives() {
        let fixture = Fixture::new();
        let damaged = fixture.damaged();
        let path = fixture.journal();
        let sentinel = path.clone();
        at_recovery_step(RecoveryStep::Install, move || {
            fs::write(&sentinel, b"another writer's file").unwrap();
        });

        let error = fixture
            .log
            .recover_damaged_at(then())
            .expect_err("recovery must not replace the file")
            .to_string();

        assert_eq!(fs::read(&path).unwrap(), b"another writer's file");
        assert!(error.contains(".damaged-20261109T101500Z"), "{error}");
        assert!(error.contains(".recovering-"), "{error}");
        let kept = PathBuf::from(format!("{}.damaged-20261109T101500Z", path.display()));
        assert_eq!(fs::read(&kept).unwrap(), damaged, "the evidence is intact");
    }

    #[test]
    fn fr14_it_touches_only_the_journal_it_was_opened_for() {
        let fixture = Fixture::new();
        fixture.damaged();
        let folder = fixture.directory.path().join("journal");
        let sentinels = [
            (
                "j-00000000000000000000000000000000.jsonl",
                b"another journal".as_slice(),
            ),
            ("notes.txt", b"not ours".as_slice()),
        ];
        for (name, bytes) in sentinels {
            fs::write(folder.join(name), bytes).unwrap();
        }
        // The first dated name is taken by a file that this call did not create.
        let path = fixture.journal();
        let taken = PathBuf::from(format!("{}.damaged-20261109T101500Z", path.display()));
        fs::write(&taken, b"someone else's file").unwrap();

        let recovery = fixture.log.recover_damaged_at(then()).unwrap();

        for (name, bytes) in sentinels {
            assert_eq!(fs::read(folder.join(name)).unwrap(), bytes, "{name}");
        }
        assert_eq!(fs::read(&taken).unwrap(), b"someone else's file");
        assert!(
            recovery
                .kept
                .to_string_lossy()
                .ends_with("damaged-20261109T101500Z-2"),
            "{}",
            recovery.kept.display()
        );
        assert!(
            fs::read_dir(&folder).unwrap().all(|entry| !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".recovering-")),
            "no pending file is left"
        );
    }
}
