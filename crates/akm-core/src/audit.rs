use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

/// One line of the append-only access log. Lives *outside* the vault: writes
/// here never churn the KDBX file. The journal is plaintext metadata and syncs
/// if the chosen vault folder syncs; it does expose access patterns there.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub prev: String,
    pub hash: String,
}

pub struct AuditLog {
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AuditState {
    Absent,
    Empty,
    Consistent,
    Corrupt,
}

pub struct AuditSnapshot {
    pub state: AuditState,
    pub entries: Vec<AuditEntry>,
    pub total_entries: usize,
    pub problem_line: Option<usize>,
    pub broken_at: Option<u64>,
}

impl AuditLog {
    pub fn for_vault(vault_path: &Path) -> Self {
        let mut os = vault_path.as_os_str().to_os_string();
        os.push(".audit.jsonl");
        Self {
            path: PathBuf::from(os),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(
        &self,
        action: &str,
        key_id: Option<&str>,
        key_title: Option<&str>,
        project: Option<&str>,
        environment: Option<&str>,
        detail: Option<&str>,
    ) -> Result<()> {
        let _lock = self.lock(true)?;
        let (seq, prev) = self.tail()?.unwrap_or((0, String::new()));
        let ts = Utc::now();

        let hash = digest(
            seq + 1,
            &ts,
            action,
            key_id,
            key_title,
            project,
            environment,
            detail,
            &prev,
        );

        let entry = AuditEntry {
            seq: seq + 1,
            ts,
            action: action.to_string(),
            key_id: key_id.map(str::to_string),
            key_title: key_title.map(str::to_string),
            project: project.map(str::to_string),
            environment: environment.map(str::to_string),
            detail: detail.map(str::to_string),
            prev,
            hash,
        };

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        let mut line = serde_json::to_vec(&entry)?;
        line.push(b'\n');
        file.write_all(&line)?;
        #[cfg(test)]
        if FAIL_NEXT_SYNC.with(|fail| fail.replace(false)) {
            return Err(std::io::Error::other("injected journal sync failure").into());
        }
        file.sync_data()?;
        Ok(())
    }

    pub fn read(&self, limit: usize) -> Result<Vec<AuditEntry>> {
        let snapshot = self.snapshot(limit)?;
        if snapshot.state == AuditState::Corrupt {
            return Err(Error::Invalid(
                "Activity journal data is damaged; original bytes were preserved.".into(),
            ));
        }
        Ok(snapshot.entries)
    }

    /// Check consistency only. An unanchored chain cannot authenticate records or prove completeness.
    pub fn verify(&self) -> Result<Option<u64>> {
        let snapshot = self.snapshot(0)?;
        match snapshot.state {
            AuditState::Consistent => Ok(None),
            AuditState::Corrupt if snapshot.broken_at.is_some() => Ok(snapshot.broken_at),
            AuditState::Corrupt => Err(Error::Invalid(
                "Activity journal data is malformed; original bytes were preserved.".into(),
            )),
            AuditState::Absent | AuditState::Empty => Err(Error::Invalid(
                "No activity records are available. Completeness is unknown.".into(),
            )),
        }
    }

    /// Read and validate the displayed snapshot under one cooperating-reader lock.
    pub fn snapshot(&self, limit: usize) -> Result<AuditSnapshot> {
        let _lock = self.lock(false)?;
        self.snapshot_locked(limit)
    }

    // Caller holds the shared/exclusive sidecar lock throughout the read and any following append.
    fn snapshot_locked(&self, limit: usize) -> Result<AuditSnapshot> {
        let file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(AuditSnapshot {
                    state: AuditState::Absent,
                    entries: Vec::new(),
                    total_entries: 0,
                    problem_line: None,
                    broken_at: None,
                })
            }
            Err(error) => return Err(error.into()),
        };
        let mut reader = BufReader::new(file);
        let mut retained = VecDeque::new();
        let mut snapshot = AuditSnapshot {
            state: AuditState::Empty,
            entries: Vec::new(),
            total_entries: 0,
            problem_line: None,
            broken_at: None,
        };
        let mut physical_line = 0;
        let mut buffer = Vec::new();
        let mut prev = String::new();
        let mut expected_seq = 1u64;
        loop {
            buffer.clear();
            if reader.read_until(b'\n', &mut buffer)? == 0 {
                break;
            }
            physical_line += 1;
            // A blank record has a line terminator and only JSON whitespace.
            // Broader ASCII whitespace includes controls JSON does not permit.
            if buffer.last() == Some(&b'\n')
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
                    break;
                }
            };
            if buffer.last() != Some(&b'\n')
                || entry.seq != expected_seq
                || entry.prev != prev
                || entry.hash
                    != digest(
                        entry.seq,
                        &entry.ts,
                        &entry.action,
                        entry.key_id.as_deref(),
                        entry.key_title.as_deref(),
                        entry.project.as_deref(),
                        entry.environment.as_deref(),
                        entry.detail.as_deref(),
                        &entry.prev,
                    )
            {
                snapshot.state = AuditState::Corrupt;
                snapshot.problem_line = Some(physical_line);
                snapshot.broken_at = Some(entry.seq);
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

    fn lock(&self, exclusive: bool) -> Result<File> {
        if let Some(parent) = self
            .path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        let mut path = self.path.as_os_str().to_os_string();
        path.push(".lock");
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(PathBuf::from(path))?;
        if exclusive {
            file.lock_exclusive()?;
        } else {
            FileExt::lock_shared(&file)?;
        }
        Ok(file)
    }

    fn tail(&self) -> Result<Option<(u64, String)>> {
        let snapshot = self.snapshot_locked(1)?;
        if snapshot.state == AuditState::Corrupt {
            let sequence = snapshot
                .broken_at
                .map(|seq| format!(", sequence {seq}"))
                .unwrap_or_default();
            return Err(Error::Invalid(format!(
                "Activity journal data is damaged at physical line {}{sequence}; append refused and original bytes preserved.",
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

#[cfg(test)]
thread_local! {
    static FAIL_NEXT_SYNC: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn fail_next_sync() {
    FAIL_NEXT_SYNC.with(|fail| fail.set(true));
}

#[allow(clippy::too_many_arguments)]
fn digest(
    seq: u64,
    ts: &DateTime<Utc>,
    action: &str,
    key_id: Option<&str>,
    key_title: Option<&str>,
    project: Option<&str>,
    environment: Option<&str>,
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
    hasher.update(key_title.unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(project.unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(environment.unwrap_or("").as_bytes());
    hasher.update(b"\n");
    hasher.update(detail.unwrap_or("").as_bytes());
    hex::encode(hasher.finalize())
}
