use akm_core::{ApiKey, ApiKeyInput, ImportPreview, Vault};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::error::CommandError;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub path: String,
    pub exists: bool,
    pub unlocked: bool,
    pub key_count: usize,
    pub vault_name: Option<String>,
    pub needs_upgrade: bool,
}

pub fn status_for(vault: &Vault) -> Status {
    let mut keys = vault.list();
    let key_count = keys.len();
    for key in &mut keys {
        key.secret.zeroize();
    }
    Status {
        path: vault.path().display().to_string(),
        exists: true,
        unlocked: true,
        key_count,
        vault_name: vault.name().map(str::to_string),
        needs_upgrade: vault.needs_upgrade(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyView {
    id: String,
    title: String,
    username: Option<String>,
    issuer: Option<String>,
    project: Option<String>,
    environment: Option<String>,
    scopes: Vec<String>,
    env_var: Option<String>,
    resolved_env_var: String,
    url: Option<String>,
    notes: Option<String>,
    tags: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
    rotation_days: Option<u32>,
    created_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    expired: bool,
    rotation_due_in: Option<i64>,
}

impl From<&ApiKey> for KeyView {
    fn from(key: &ApiKey) -> Self {
        let now = Utc::now();
        Self {
            id: key.id.clone(),
            title: key.title.clone(),
            username: key.username.clone(),
            issuer: key.issuer.clone(),
            project: key.project.clone(),
            environment: key.environment.clone(),
            scopes: key.scopes.clone(),
            env_var: key.env_var.clone(),
            resolved_env_var: key.resolved_env_var(),
            url: key.url.clone(),
            notes: key.notes.clone(),
            tags: key.tags.clone(),
            expires_at: key.expires_at,
            rotation_days: key.rotation_days,
            created_at: key.created_at,
            updated_at: key.updated_at,
            expired: key.is_expired(now),
            rotation_due_in: key.rotation_due_in(now),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDraft {
    title: String,
    secret: Option<String>,
    username: Option<String>,
    issuer: Option<String>,
    project: Option<String>,
    environment: Option<String>,
    #[serde(default)]
    scopes: Vec<String>,
    env_var: Option<String>,
    url: Option<String>,
    notes: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
    rotation_days: Option<u32>,
}

impl KeyDraft {
    pub fn into_input(mut self, existing: Option<ApiKey>) -> Result<ApiKeyInput, CommandError> {
        let secret = self
            .secret
            .take()
            .filter(|value| !value.is_empty())
            .or_else(|| existing.as_ref().map(|key| key.secret.clone()))
            .ok_or_else(|| CommandError::new("invalid", "The secret must not be empty."))?;
        let username = self
            .username
            .take()
            .or_else(|| existing.and_then(|key| key.username));
        Ok(ApiKeyInput {
            title: self.title,
            secret,
            username: clean(username),
            issuer: clean(self.issuer),
            project: clean(self.project),
            environment: clean(self.environment),
            scopes: tidy(self.scopes),
            env_var: clean(self.env_var),
            url: clean(self.url),
            notes: clean(self.notes),
            tags: tidy(self.tags),
            expires_at: self.expires_at,
            rotation_days: self.rotation_days,
        })
    }
}

pub fn clean(value: Option<String>) -> Option<String> {
    value
        .map(|inner| inner.trim().to_string())
        .filter(|inner| !inner.is_empty())
}

fn tidy(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditView {
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub action: String,
    pub key_title: Option<String>,
    pub project: Option<String>,
    pub environment: Option<String>,
    pub detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub path: String,
    pub entries: Vec<AuditView>,
    pub broken_at: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub added: usize,
    pub updated: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewReport {
    #[serde(flatten)]
    pub preview: ImportPreview,
    pub token: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub path: String,
    pub exists: bool,
    pub bytes: u64,
    pub modified: Option<DateTime<Utc>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn omitted_username_is_preserved_on_update() {
        let existing = ApiKeyInput {
            title: "Fake".into(),
            secret: "fake-secret".into(),
            username: Some("alice".into()),
            ..Default::default()
        };
        let directory = tempfile::tempdir().unwrap();
        let mut vault = Vault::create(
            directory.path().join("vault.kdbx"),
            "fake-test-password-123!",
            None,
        )
        .unwrap();
        let key = vault.add(existing).unwrap();
        let draft: KeyDraft =
            serde_json::from_value(serde_json::json!({ "title": "Renamed" })).unwrap();
        let input = draft.into_input(Some(key)).unwrap();
        assert_eq!(input.username.as_deref(), Some("alice"));
        assert_eq!(input.secret, "fake-secret");
    }
}
