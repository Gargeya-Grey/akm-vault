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
        file.sync_data()?;
        Ok(())
    }

    pub fn read(&self, limit: usize) -> Result<Vec<AuditEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let _lock = self.lock(false)?;

        let file = OpenOptions::new().read(true).open(&self.path)?;
        let mut entries: Vec<AuditEntry> = Vec::new();

        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            entries.push(serde_json::from_str(&line)?);
        }

        if entries.len() > limit {
            entries.drain(0..entries.len() - limit);
        }
        Ok(entries)
    }

    /// Recompute the chain and report the first entry that does not verify.
    pub fn verify(&self) -> Result<Option<u64>> {
        let mut prev = String::new();
        let mut expected_seq = 1u64;

        for entry in self.read(usize::MAX)? {
            if entry.seq != expected_seq || entry.prev != prev {
                return Ok(Some(entry.seq));
            }
            let hash = digest(
                entry.seq,
                &entry.ts,
                &entry.action,
                entry.key_id.as_deref(),
                entry.key_title.as_deref(),
                entry.project.as_deref(),
                entry.environment.as_deref(),
                entry.detail.as_deref(),
                &entry.prev,
            );
            if hash != entry.hash {
                return Ok(Some(entry.seq));
            }
            prev = entry.hash;
            expected_seq += 1;
        }
        Ok(None)
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
        if !self.path.exists() {
            return Ok(None);
        }
        let file = OpenOptions::new().read(true).open(&self.path)?;
        let mut last: Option<AuditEntry> = None;
        for line in BufReader::new(file).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: AuditEntry = serde_json::from_str(&line)
                .map_err(|e| Error::Invalid(format!("audit log is corrupt: {e}")))?;
            last = Some(entry);
        }
        Ok(last.map(|e| (e.seq, e.hash)))
    }
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
