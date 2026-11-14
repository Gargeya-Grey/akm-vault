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

/// The longest line of a journal, with its line end, that the reader takes (CORE-R2-06).
/// AKM writes records of a few hundred bytes.
pub const MAX_RECORD_BYTES: usize = 64 * 1024;

/// What a recovery kept and what it started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    /// A copy of the damaged file, byte for byte, under its dated name beside the new file.
    pub kept: PathBuf,
    /// The size of the kept file.
    pub bytes: u64,
    /// The SHA-256 of the kept file, in hex. The first record of the new journal states it.
    pub sha256: String,
}

/// The steps of a recovery at which a test can make the file system fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryStep {
    CreateKept,
    SyncKept,
    CreateNew,
    SyncNew,
    Install,
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
#[derive(Clone, Copy)]
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

/// Keeps a copy of the damaged file at `path` under a dated name, byte for byte, and puts the
/// bytes that `fresh` makes from the copy and its name in its place: a journal chain (FR-14)
/// or the mapping (FV-01). The caller holds the lock of the file and checked that it is
/// damaged. `read` opens the file; `None` is a mapping that is missing beside its journals,
/// and its copy is empty.
///
/// The damaged file stays in place until the last step (FW-02, class C). The copy is written
/// and synced first, then the new file under a pending name, and one replacing move puts the
/// new file in the place of the damaged one. A stop before that move leaves the damaged file
/// where it was, so the next start refuses disclosure and offers the recovery again, which
/// makes one more copy. No state has the file missing. Just before the move, the file must
/// still hold the bytes of the copy, so a file that changed is not replaced. A failure that
/// this call returns removes the copy and the pending file, which it created, and nothing else.
pub(crate) fn set_aside(
    path: &Path,
    now: DateTime<Utc>,
    read: impl Fn() -> Result<Option<File>>,
    create_new: impl Fn(&Path) -> Result<File>,
    fresh: impl FnOnce(&Recovery, &str) -> Result<Vec<u8>>,
) -> Result<Recovery> {
    let kept = free_kept_path(path, now)?;
    let kept_name = kept
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // The name was chosen by an absence check, and `create_new` refuses it when another writer
    // took it since (core review 1, finding 8).
    let mut copy = reach(RecoveryStep::CreateKept)
        .and_then(|()| create_new(&kept))
        .map_err(|error| {
            Error::Journal(format!(
                "the damaged file could not be kept as {kept_name}, so nothing was changed; try again ({error})"
            ))
        })?;
    // From here the copy is ours, and only this call removes it.
    let copied = reach(RecoveryStep::SyncKept)
        .and_then(|()| fingerprint(read()?, &mut copy))
        .and_then(|fingerprint| {
            copy.sync_data()?;
            Ok(fingerprint)
        });
    drop(copy);
    let (bytes, sha256) = copied.map_err(|error| stopped(error, &[&kept]))?;
    let recovery = Recovery {
        kept,
        bytes,
        sha256,
    };
    let kept = recovery.kept.as_path();
    let new_bytes = fresh(&recovery, &kept_name).map_err(|error| stopped(error, &[kept]))?;

    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".recovering-{}", uuid::Uuid::new_v4().simple()));
    let pending = PathBuf::from(name);
    let mut created = reach(RecoveryStep::CreateNew)
        .and_then(|()| create_new(&pending))
        .map_err(|error| stopped(error, &[kept]))?;
    // From here the pending file is ours too.
    let written = reach(RecoveryStep::SyncNew)
        .and_then(|()| Ok(created.write_all(&new_bytes)?))
        .and_then(|()| Ok(created.sync_data()?));
    drop(created);
    written
        .and_then(|()| reach(RecoveryStep::Install))
        .and_then(|()| {
            if fingerprint(read()?, std::io::sink())? != (recovery.bytes, recovery.sha256.clone()) {
                return Err(Error::Journal(
                    "the damaged file changed during the recovery".into(),
                ));
            }
            Ok(fs::rename(&pending, path)?)
        })
        .map_err(|error| stopped(error, &[&pending, kept]))?;
    Ok(recovery)
}

/// The error of a recovery that stopped before the new file took the place of the damaged
/// one. It removes `created`, the files that the recovery made, and says so if it cannot.
fn stopped(error: Error, created: &[&Path]) -> Error {
    let error = Error::Journal(format!(
        "the recovery stopped, so nothing was changed; try again ({error})"
    ));
    created
        .iter()
        .fold(error, |error, path| with_leftover(error, path))
}

/// The refusal of a recovery whose damaged file is not the one that the person approved.
pub(crate) fn not_approved(damaged: &Path) -> Error {
    Error::Journal(format!(
        "the damaged file is now {}, not the file that you approved, so nothing was changed. Try again",
        damaged.display()
    ))
}

/// The size and the SHA-256 of a file, or of no bytes when there is no file. Each byte read
/// is also written to `copy`.
fn fingerprint(file: Option<File>, mut copy: impl Write) -> Result<(u64, String)> {
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    let mut block = [0u8; 8192];
    if let Some(mut file) = file {
        loop {
            let read = std::io::Read::read(&mut file, &mut block)?;
            if read == 0 {
                break;
            }
            hasher.update(&block[..read]);
            copy.write_all(&block[..read])?;
            bytes += read as u64;
        }
    }
    Ok((bytes, hex::encode(hasher.finalize())))
}

/// `<file>.damaged-<UTC time>`, with a number added if that name is taken. The name does not
/// end in `.jsonl` or `.json`, so nothing reads it as a journal or a mapping.
fn free_kept_path(path: &Path, now: DateTime<Utc>) -> Result<PathBuf> {
    let stamp = now.format("%Y%m%dT%H%M%SZ");
    for number in 1..=100u32 {
        let mut name = path.as_os_str().to_os_string();
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
        "too many kept files have the same time stamp".into(),
    ))
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
        self.append_all(
            None,
            &[NewRecord {
                action,
                key_id,
                detail,
            }],
        )
    }

    /// `opening` goes first when the chain has no record yet.
    pub(crate) fn append_all(
        &self,
        opening: Option<NewRecord<'_>>,
        records: &[NewRecord<'_>],
    ) -> Result<()> {
        let _lock = self.store.lock(true)?;
        let tail = self.tail()?;
        let records: Vec<_> = opening
            .filter(|_| tail.is_none())
            .into_iter()
            .chain(records.iter().copied())
            .collect();
        let (seq, prev) = tail.unwrap_or((0, self.genesis.clone()));
        let bytes = self.encode(&records, &prev, seq, Utc::now())?;
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
            let start = bytes.len();
            serde_json::to_writer(&mut bytes, &entry)?;
            bytes.push(b'\n');
            if bytes.len() - start > MAX_RECORD_BYTES {
                return Err(Error::Journal(format!(
                    "a record would be longer than the {} KiB that the journal reader takes, so it was not written",
                    MAX_RECORD_BYTES / 1024
                )));
            }
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
            // At most one byte past the bound, so a line of any length costs the reader no
            // more than the bound (CORE-R2-06).
            let bound = MAX_RECORD_BYTES as u64 + 1;
            if std::io::Read::take(&mut reader, bound).read_until(b'\n', &mut buffer)? == 0 {
                break;
            }
            physical_line += 1;
            if buffer.len() > MAX_RECORD_BYTES {
                // Longer than any record that AKM writes: not a valid record.
                snapshot.state = AuditState::Corrupt;
                snapshot.problem_line = Some(physical_line);
                snapshot.damage = Some(Damage::MalformedJson);
                break;
            }
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

    /// Keeps a copy of a damaged chain file under a dated name, byte for byte, and starts a
    /// new chain in its place (`set_aside`). The first record of the new chain states the gap
    /// and the SHA-256 of the kept file; `then` follows it. The chain file must be damaged.
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
        set_aside(
            &self.path,
            now,
            || self.store.read(),
            |pending| self.store.create_new(pending),
            |recovery, kept_name| {
                let detail = format!(
                "history before this record cannot be proved complete; the damaged journal ({} bytes, SHA-256 {}) is kept as {kept_name}",
                recovery.bytes, recovery.sha256
            );
                let opening = NewRecord {
                    action: RECOVERED,
                    key_id: None,
                    detail: Some(&detail),
                };
                let records: Vec<_> = [opening].into_iter().chain(then.iter().copied()).collect();
                self.encode(&records, &self.genesis, 0, now)
            },
        )
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
    /// they do not exist. Refuses when the identity of the vault cannot be confirmed. It
    /// reports no lineage warning, so it leaves an unseen version unsettled for the next
    /// record through the vault, which warns (FR-12, CORE-R2-04).
    pub fn append(&self, action: &str, key_id: Option<&str>, detail: Option<&str>) -> Result<()> {
        self.append_reporting(action, key_id, detail, false)
            .map(|_| ())
    }

    /// Like [`AuditLog::append`]. The answer is `true` when the record went into a journal
    /// that continues, but whose link to this vault file is not confirmed: the file holds a
    /// version that the journal never saw (FR-12). `carries_lineage`: the caller passes the
    /// answer on to the person, so this record settles the version (only `Vault::log`).
    pub(crate) fn append_reporting(
        &self,
        action: &str,
        key_id: Option<&str>,
        detail: Option<&str>,
        carries_lineage: bool,
    ) -> Result<bool> {
        let folder = self.root.open_or_create()?;
        journal::record(&folder, &self.evidence, carries_lineage, |placement| {
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
            let opening = placement.opening.as_deref().map(|detail| NewRecord {
                action: RECOVERED,
                key_id: None,
                detail: Some(detail),
            });
            chain.append_all(opening, &records)?;
            Ok(placement.lineage_unconfirmed)
        })
    }

    pub fn read(&self, limit: usize) -> Result<Vec<AuditEntry>> {
        self.snapshot(limit)?.into_entries()
    }

    pub fn verify(&self) -> Result<Option<u64>> {
        self.snapshot(0)?.verification()
    }

    /// A deliberate recovery from a damaged mapping of the folder (FV-01), else from a damaged
    /// journal of this vault (FR-14). It touches that one file and no other file of the
    /// folder. A copy of the damaged file is kept under a dated name beside it, byte for byte;
    /// a mapping that is missing beside journals counts as damaged, and its copy is empty. A
    /// damaged journal is replaced by a new journal with one record that states the gap and
    /// the SHA-256 of the kept file. A damaged mapping is replaced by an empty one, and each
    /// vault's new journal starts with that record. Appends and disclosure work again after
    /// that. When nothing is damaged, the recovery is refused. A failure or a stop at any step
    /// leaves the damaged file in place (FW-02).
    pub fn recover_damaged(&self) -> Result<Recovery> {
        self.recover_at(Utc::now(), None)
    }

    /// Like [`AuditLog::recover_damaged`], for the file that the person approved, from
    /// [`AuditLog::damaged_file`]. When another file is damaged now, it refuses and nothing
    /// moves (class B).
    pub fn recover_damaged_file(&self, approved: &Path) -> Result<Recovery> {
        self.recover_at(Utc::now(), Some(approved))
    }

    pub fn recover_damaged_at(&self, now: DateTime<Utc>) -> Result<Recovery> {
        self.recover_at(now, None)
    }

    fn recover_at(&self, now: DateTime<Utc>, approved: Option<&Path>) -> Result<Recovery> {
        let Some(folder) = self.root.open()? else {
            return Err(Error::Journal("there is no journal to recover".into()));
        };
        if let Some(recovery) = journal::recover_mapping(&folder, now, approved)? {
            return Ok(recovery);
        }
        // A recovery reports no lineage warning, so it leaves an unseen version unsettled
        // for the next record.
        journal::record(&folder, &self.evidence, false, |placement| {
            let path = folder.journal_file(&placement.id);
            if approved.is_some_and(|approved| approved != path) {
                return Err(not_approved(&path));
            }
            let chain = ChainFile::private(path, &placement.id);
            let owed = placement.owes_rollback.then_some(NewRecord {
                action: "vault-rolled-back",
                key_id: None,
                detail: Some("vault content matches an earlier version that this device saw"),
            });
            chain.recover(now, owed.as_slice())
        })
    }

    /// The file that a recovery would keep aside: the mapping of the folder when it is
    /// damaged, else the journal of this vault when it is damaged. Changes nothing.
    pub fn damaged_file(&self) -> Result<Option<PathBuf>> {
        let Some(folder) = self.root.open()? else {
            return Ok(None);
        };
        if let Some(mapping) = journal::damaged_mapping_file(&folder)? {
            return Ok(Some(mapping));
        }
        let snapshot = self.snapshot(0)?;
        Ok((snapshot.state == AuditState::Corrupt).then_some(snapshot.location))
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
            fs::write(directory.path().join("vault.kdbx"), b"fake vault bytes").unwrap();
            let log = Self::log_in(directory.path());
            Self { directory, log }
        }

        fn log_in(directory: &Path) -> AuditLog {
            AuditLog::new(
                JournalRoot::at(directory.join("journal")),
                VaultEvidence::from_file(&directory.join("vault.kdbx")),
            )
        }

        /// A new log on the same folder and vault, as after a restart of AKM.
        fn reopened(&self) -> AuditLog {
            Self::log_in(self.directory.path())
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

    /// Runs `recover` and stops it at `step` as a crash does: nothing of the call runs after
    /// that point, no later step and no clean-up.
    fn stopped_at<T>(step: RecoveryStep, recover: impl FnOnce() -> T) {
        at_recovery_step(step, || panic!("AKM stops here"));
        let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(recover));
        assert!(stopped.is_err(), "the recovery did not reach {step:?}");
    }

    /// FW-02, class C: AKM stops during a journal recovery, after it kept the damaged
    /// journal and before the new journal takes its place. A new log on the same folder still
    /// finds the damage and refuses a record, and a retry ends with the gap record.
    #[test]
    fn fw02_a_journal_recovery_that_stops_before_its_last_step_leaves_the_damage_in_place() {
        let fixture = Fixture::new();
        let damaged = fixture.damaged();
        let path = fixture.journal();

        stopped_at(RecoveryStep::Install, || {
            fixture.log.recover_damaged_at(then())
        });

        let log = fixture.reopened();
        assert_eq!(
            log.damaged_file().unwrap(),
            Some(path.clone()),
            "the recovery is offered again"
        );
        assert_eq!(fs::read(&path).unwrap(), damaged, "the damaged journal");
        assert!(
            log.append("reveal-requested", None, None).is_err(),
            "disclosure stays blocked"
        );
        let recovery = log.recover_damaged_at(then()).unwrap();
        assert_eq!(fs::read(&recovery.kept).unwrap(), damaged);
        log.append("reveal-requested", None, None).unwrap();
        let entries = log.read(10).unwrap();
        assert_eq!(entries[0].action, RECOVERED);
        let detail = entries[0].detail.as_deref().unwrap();
        assert!(detail.contains(&recovery.sha256), "{detail}");
        assert_eq!(entries[1].action, "reveal-requested");
    }

    /// Classes B and F: another writer changes the damaged journal while the recovery runs.
    /// The copy holds what the recovery read, so the changed file is not replaced: the
    /// recovery refuses, the writer's bytes stay, and no copy and no new file are left.
    #[test]
    fn fr14_a_journal_that_changes_during_the_recovery_is_not_replaced() {
        let fixture = Fixture::new();
        fixture.damaged();
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
        assert!(error.contains("nothing was changed"), "{error}");
        let names: Vec<_> = fixture.folder().into_iter().map(|(name, _)| name).collect();
        assert!(
            !names
                .iter()
                .any(|name| name.contains(".damaged-") || name.contains(".recovering-")),
            "{names:?}"
        );
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
            RecoveryStep::CreateKept,
            RecoveryStep::SyncKept,
            RecoveryStep::CreateNew,
            RecoveryStep::SyncNew,
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
        at_recovery_step(RecoveryStep::CreateKept, move || {
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

    // ---- FV-01: a damaged mapping ----

    const KEPT_MAPPING: &str = "mapping.json.damaged-20261109T101500Z";

    fn folder_of(directory: &Path) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<_> = fs::read_dir(directory.join("journal"))
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

    /// Cuts the mapping of the folder in half, and returns the bytes that are left.
    fn damage_mapping(directory: &Path) -> Vec<u8> {
        let mapping = directory.join("journal").join("mapping.json");
        let bytes = fs::read(&mapping).unwrap();
        fs::write(&mapping, &bytes[..bytes.len() / 2]).unwrap();
        fs::read(&mapping).unwrap()
    }

    /// Two fake vault files that share one journal folder, with the evidence of an unlocked
    /// vault and one record each.
    struct TwoLogs {
        directory: tempfile::TempDir,
        logs: Vec<AuditLog>,
    }

    impl TwoLogs {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            for name in ["a.kdbx", "b.kdbx"] {
                fs::write(
                    directory.path().join(name),
                    format!("fake vault bytes of {name}"),
                )
                .unwrap();
            }
            let logs = Self::logs_in(directory.path());
            for log in &logs {
                log.append("add-completed", None, None).unwrap();
            }
            Self { directory, logs }
        }

        fn logs_in(directory: &Path) -> Vec<AuditLog> {
            ["a.kdbx", "b.kdbx"]
                .into_iter()
                .map(|name| {
                    let vault = directory.join(name);
                    AuditLog::new(
                        JournalRoot::at(directory.join("journal")),
                        VaultEvidence::unlocked(
                            &vault,
                            Some(hex::encode(Sha256::digest(fs::read(&vault).unwrap()))),
                            format!("fake-vault-id-{name}"),
                        ),
                    )
                })
                .collect()
        }

        /// New logs on the same folder and vaults, as after a restart of AKM.
        fn reopened(&self) -> Vec<AuditLog> {
            Self::logs_in(self.directory.path())
        }
    }

    /// FW-02, class C: AKM stops during a mapping recovery, after it kept the damaged mapping
    /// and before the new mapping takes its place. New logs on the same folder still find the
    /// damage and refuse every record, and a retry resumes both vaults with the gap record.
    #[test]
    fn fw02_a_mapping_recovery_that_stops_before_its_last_step_leaves_the_damage_in_place() {
        let f = TwoLogs::new();
        let damaged = damage_mapping(f.directory.path());
        let mapping = f.directory.path().join("journal").join("mapping.json");

        stopped_at(RecoveryStep::Install, || {
            f.logs[0].recover_damaged_at(then())
        });

        let logs = f.reopened();
        for log in &logs {
            assert_eq!(log.damaged_file().unwrap(), Some(mapping.clone()));
            assert!(
                log.append("reveal-requested", None, None).is_err(),
                "disclosure stays blocked"
            );
        }
        assert_eq!(fs::read(&mapping).unwrap(), damaged, "the damaged mapping");
        let recovery = logs[0].recover_damaged_at(then()).unwrap();
        assert_eq!(fs::read(&recovery.kept).unwrap(), damaged);
        for log in &logs {
            assert!(
                log.append_reporting("reveal-requested", None, None, true)
                    .unwrap(),
                "the first record warns"
            );
            let entries = log.read(10).unwrap();
            assert_eq!(entries[0].action, RECOVERED);
            let detail = entries[0].detail.as_deref().unwrap();
            assert!(detail.contains(&recovery.sha256), "{detail}");
        }
    }

    /// FW-02, the reviewer's state: an AKM from before the fix stopped a mapping recovery
    /// after it moved the damaged mapping aside and before the new mapping took its place. No
    /// mapping, and a journal beside it: the history of this folder exists, and nothing links
    /// it. The same holds when the mapping was deleted. AKM reads this as a damaged mapping: a
    /// reveal is refused and the recovery is offered, never an empty mapping that starts a new
    /// journal with no record of the gap. The recovery keeps an empty file as the record of
    /// the missing mapping, changes no other file, and the next reveal states the gap.
    #[test]
    fn fw02_a_missing_mapping_beside_a_journal_is_damage_and_its_recovery_states_the_gap() {
        for interrupted in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let journal = directory.path().join("journal");
            let mut vault = crate::Vault::create_in(
                directory.path().join("a.kdbx"),
                "fake-mapping-password",
                None,
                JournalRoot::at(&journal),
            )
            .unwrap();
            let key = vault
                .add(crate::ApiKeyInput::new("Fake", "fake-mapping-value"))
                .unwrap();
            reveal(&vault, &key).unwrap();
            assert_eq!(lineage_warnings(&mut vault), 0);
            let mapping = journal.join("mapping.json");
            if interrupted {
                let bytes = fs::read(&mapping).unwrap();
                fs::write(
                    journal.join("mapping.json.damaged-20261113T101500Z"),
                    &bytes[..bytes.len() / 2],
                )
                .unwrap();
                fs::write(
                    journal.join("mapping.json.recovering-00000000000000000000000000000000"),
                    br#"{"version": 1, "journals": []}"#,
                )
                .unwrap();
            }
            fs::remove_file(&mapping).unwrap();
            let before = folder_of(directory.path());

            let refused = reveal(&vault, &key).unwrap_err().to_string();

            assert!(refused.contains("mapping"), "{interrupted}: {refused}");
            assert_eq!(folder_of(directory.path()), before, "{interrupted}");
            let log = vault.audit_log();
            assert_eq!(log.damaged_file().unwrap(), Some(mapping.clone()));
            let recovery = log.recover_damaged_file(&mapping).unwrap();
            assert_eq!(recovery.bytes, 0, "{interrupted}");
            assert_eq!(fs::read(&recovery.kept).unwrap(), b"", "{interrupted}");
            let after = folder_of(directory.path());
            for file in &before {
                assert!(after.contains(file), "{interrupted}: {} changed", file.0);
            }
            reveal(&vault, &key).unwrap();
            assert_eq!(lineage_warnings(&mut vault), 1, "{interrupted}");
            let entries = vault.audit_log().read(10).unwrap();
            assert_eq!(entries[0].action, RECOVERED, "{interrupted}");
            let detail = entries[0].detail.as_deref().unwrap();
            let kept = recovery.kept.file_name().unwrap().to_string_lossy();
            for fact in ["cannot be proved complete", "missing", kept.as_ref()] {
                assert!(detail.contains(fact), "{interrupted}: {fact}: {detail}");
            }
        }
    }

    fn reveal(vault: &crate::Vault, key: &crate::ApiKey) -> Result<()> {
        vault
            .disclose_key(crate::KeyDisclosure::Reveal, key, None)
            .map(|_| ())
    }

    fn lineage_warnings(vault: &mut crate::Vault) -> usize {
        vault
            .take_warning_records()
            .iter()
            .filter(|warning| {
                warning.kind == crate::vault::VaultWarningKind::JournalLineageUnconfirmed
            })
            .count()
    }

    /// FV-01, the reviewer's probe: a damaged mapping stops the reveals of two vaults, and
    /// nothing recovers by itself. A deliberate recovery keeps the damaged bytes under a new
    /// name and touches no journal file. Both vaults then reveal again. The new journal of
    /// each one starts with a record that states the gap, and its first record warns that
    /// the lineage is unconfirmed.
    #[test]
    fn fv01_a_recovery_of_a_damaged_mapping_resumes_both_vaults_and_states_the_gap() {
        let directory = tempfile::tempdir().unwrap();
        let mut vaults: Vec<_> = ["a.kdbx", "b.kdbx"]
            .into_iter()
            .map(|name| {
                let mut vault = crate::Vault::create_in(
                    directory.path().join(name),
                    "fake-mapping-password",
                    None,
                    JournalRoot::at(directory.path().join("journal")),
                )
                .unwrap();
                let key = vault
                    .add(crate::ApiKeyInput::new("Fake", "fake-mapping-value"))
                    .unwrap();
                reveal(&vault, &key).unwrap();
                assert_eq!(lineage_warnings(&mut vault), 0);
                (vault, key)
            })
            .collect();
        let journals: Vec<_> = vaults
            .iter()
            .map(|(vault, _)| {
                let path = vault.audit_log().journal_path().unwrap().unwrap();
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect();
        let damaged = damage_mapping(directory.path());
        let mapping = directory.path().join("journal").join("mapping.json");
        for (vault, key) in &vaults {
            let refused = reveal(vault, key).unwrap_err().to_string();
            assert!(refused.contains("mapping"), "{refused}");
        }
        assert_eq!(
            fs::read(&mapping).unwrap(),
            damaged,
            "nothing recovers by itself"
        );

        let recovery = vaults[0].0.audit_log().recover_damaged_at(then()).unwrap();

        assert_eq!(recovery.kept, mapping.with_file_name(KEPT_MAPPING));
        assert_eq!(
            fs::read(&recovery.kept).unwrap(),
            damaged,
            "kept byte for byte"
        );
        assert_eq!(recovery.bytes, damaged.len() as u64);
        assert_eq!(recovery.sha256, hex::encode(Sha256::digest(&damaged)));
        for (vault, key) in &mut vaults {
            reveal(vault, key).unwrap();
            assert_eq!(lineage_warnings(vault), 1, "the first record warns");
            reveal(vault, key).unwrap();
            assert_eq!(lineage_warnings(vault), 0, "and only the first");
            let entries = vault.audit_log().read(10).unwrap();
            let actions: Vec<_> = entries.iter().map(|entry| entry.action.as_str()).collect();
            assert_eq!(actions, [RECOVERED, "reveal-requested", "reveal-requested"]);
            let detail = entries[0].detail.as_deref().unwrap();
            for fact in [
                "cannot be proved complete",
                recovery.sha256.as_str(),
                KEPT_MAPPING,
            ] {
                assert!(detail.contains(fact), "{fact}: {detail}");
            }
        }
        for (path, bytes) in journals {
            assert_eq!(fs::read(&path).unwrap(), bytes, "{}", path.display());
        }
    }

    /// FV-01, classes C and F: a failure at any step of a mapping recovery leaves the folder
    /// as it was, and disclosure stays blocked. A retry then ends in the state of a recovery
    /// that never failed (`None`): the damaged bytes under the dated name, an empty mapping
    /// that states the gap, every journal file as it was, and a first record of each vault
    /// that warns.
    #[test]
    fn fv01_a_failed_mapping_recovery_changes_nothing_and_a_retry_ends_the_same() {
        for step in [
            None,
            Some(RecoveryStep::CreateKept),
            Some(RecoveryStep::SyncKept),
            Some(RecoveryStep::CreateNew),
            Some(RecoveryStep::SyncNew),
            Some(RecoveryStep::Install),
        ] {
            let f = TwoLogs::new();
            let damaged = damage_mapping(f.directory.path());
            let before = folder_of(f.directory.path());
            if let Some(step) = step {
                fail_recovery_at(step);
                let error = f.logs[0].recover_damaged_at(then()).unwrap_err();
                assert!(error.to_string().contains("injected"), "{step:?}: {error}");
                assert_eq!(folder_of(f.directory.path()), before, "{step:?}");
                assert!(
                    f.logs[1].append("reveal-requested", None, None).is_err(),
                    "{step:?}: disclosure stays blocked"
                );
            }

            f.logs[0].recover_damaged_at(then()).unwrap();

            let mut after = folder_of(f.directory.path());
            let fresh = after.remove(
                after
                    .iter()
                    .position(|(name, _)| name == "mapping.json")
                    .expect("a new mapping"),
            );
            let sha256 = hex::encode(Sha256::digest(&damaged));
            let mut expected: Vec<_> = before
                .into_iter()
                .filter(|(name, _)| name != "mapping.json")
                .chain([(KEPT_MAPPING.to_string(), damaged)])
                .collect();
            expected.sort();
            assert_eq!(after, expected, "{step:?}");
            let fresh: serde_json::Value = serde_json::from_slice(&fresh.1).unwrap();
            assert_eq!(fresh["journals"], serde_json::json!([]), "{step:?}");
            let gap = fresh["recovered"].as_str().unwrap();
            for fact in ["cannot be proved complete", sha256.as_str(), KEPT_MAPPING] {
                assert!(gap.contains(fact), "{step:?}: {fact}: {gap}");
            }
            for log in &f.logs {
                assert!(
                    log.append_reporting("reveal-requested", None, None, true)
                        .unwrap(),
                    "{step:?}: the first record warns"
                );
            }
        }
    }

    /// FV-01, class B: the person approves a recovery of one damaged file. The recovery moves
    /// that file only, and refuses with nothing moved when another one is damaged now. A
    /// mapping of another version is no damage. Before its first record, the journal of a
    /// vault says that the mapping was started again.
    #[test]
    fn fv01_a_recovery_approved_for_one_file_moves_no_other() {
        let f = TwoLogs::new();
        let journal = f.logs[0].journal_path().unwrap().unwrap();
        let mapping = f.directory.path().join("journal").join("mapping.json");
        damage_mapping(f.directory.path());
        assert_eq!(f.logs[0].damaged_file().unwrap(), Some(mapping.clone()));
        let before = folder_of(f.directory.path());

        let refused = f.logs[0].recover_damaged_file(&journal).unwrap_err();

        assert!(
            refused.to_string().contains("nothing was changed"),
            "{refused}"
        );
        assert_eq!(folder_of(f.directory.path()), before);
        f.logs[0].recover_damaged_file(&mapping).unwrap();
        assert_eq!(f.logs[0].damaged_file().unwrap(), None);
        let snapshot = f.logs[1].snapshot(10).unwrap();
        assert_eq!(snapshot.state, AuditState::Absent);
        assert!(
            snapshot
                .notices
                .iter()
                .any(|notice| notice.message.contains("started again")),
            "{:?}",
            snapshot.notices
        );
        let before = folder_of(f.directory.path());
        assert!(f.logs[0].recover_damaged_file(&mapping).is_err());
        assert_eq!(folder_of(f.directory.path()), before, "not damaged now");

        fs::write(&mapping, br#"{"version": 99, "journals": []}"#).unwrap();
        let before = folder_of(f.directory.path());
        assert!(f.logs[0].damaged_file().is_err());
        assert!(f.logs[0].recover_damaged().is_err());
        assert_eq!(folder_of(f.directory.path()), before, "another version");
    }

    /// FV-01, class C: the first record of a vault after a mapping recovery fails at its sync.
    /// The retry warns again, because the version was not settled, and the new journal states
    /// the gap once, first.
    #[test]
    fn fv01_a_failed_first_record_after_a_mapping_recovery_warns_again_on_the_retry() {
        let f = TwoLogs::new();
        damage_mapping(f.directory.path());
        f.logs[0].recover_damaged_at(then()).unwrap();
        let reveal = || f.logs[0].append_reporting("reveal-requested", None, None, true);

        fail_next_sync();
        assert!(reveal().is_err(), "the injected failure");

        assert!(reveal().unwrap(), "the retry warns");
        assert!(!reveal().unwrap(), "and the next record does not");
        let actions: Vec<_> = f.logs[0]
            .read(10)
            .unwrap()
            .into_iter()
            .map(|entry| entry.action)
            .collect();
        assert_eq!(actions[0], RECOVERED);
        assert_eq!(
            actions.iter().filter(|action| *action == RECOVERED).count(),
            1
        );
    }
}
