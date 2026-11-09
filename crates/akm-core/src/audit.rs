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
        let (mut seq, mut prev) = self.tail()?.unwrap_or((0, self.genesis.clone()));
        let mut bytes = Vec::new();
        for record in records {
            seq += 1;
            let ts = Utc::now();
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
        let mut file = self.store.append()?;
        file.write_all(&bytes)?;
        #[cfg(test)]
        if FAIL_NEXT_SYNC.with(|fail| fail.replace(false)) {
            return Err(std::io::Error::other("injected journal sync failure").into());
        }
        file.sync_data()?;
        Ok(())
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
