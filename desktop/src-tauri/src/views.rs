use akm_core::{ApiKey, ApiKeyInput, RecycledKey, Rotation, Secret, SecretChange, Vault};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::CommandError;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub session_id: Option<String>,
    pub path: String,
    pub exists: bool,
    pub unlocked: bool,
    pub key_count: usize,
    pub vault_name: Option<String>,
    pub needs_upgrade: bool,
}

pub fn status_for(vault: &Vault) -> Status {
    let key_count = vault.list().len();
    Status {
        session_id: None, // Filled from the completed native policy ticket at the IPC boundary.
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
    /// When the rotation interval elapses. The page works out "due" and "overdue" from this
    /// and the time of the moment, so a state never goes stale while the list stays open.
    rotation_due_at: Option<DateTime<Utc>>,
    /// When the secret last changed. `None` when AKM knows no such time.
    secret_changed_at: Option<DateTime<Utc>>,
    secret_change: SecretChange,
    history_count: usize,
}

impl From<&ApiKey> for KeyView {
    fn from(key: &ApiKey) -> Self {
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
            rotation_due_at: key.rotation_due_at(),
            secret_changed_at: key.secret_changed_at,
            secret_change: key.secret_change,
            history_count: key.history_count,
        }
    }
}

/// The receipt of a rotation. It carries no secret.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationView {
    key: KeyView,
    changed_at: DateTime<Utc>,
    previous_changed_at: Option<DateTime<Utc>>,
}

impl From<&Rotation> for RotationView {
    fn from(rotation: &Rotation) -> Self {
        Self {
            key: KeyView::from(&rotation.key),
            changed_at: rotation.changed_at,
            previous_changed_at: rotation.previous_changed_at,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecycledView {
    key: KeyView,
    deleted_at: Option<DateTime<Utc>>,
}

impl From<&RecycledKey> for RecycledView {
    fn from(item: &RecycledKey) -> Self {
        Self {
            key: KeyView::from(&item.key),
            deleted_at: item.deleted_at,
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
    /// The text that the page sent. `into_input` reads it with the core's one expiry rule
    /// (`schedule::parse_expiry`), which checks the text: a typed instant has already lost
    /// every fraction digit after the ninth.
    expires_at: Option<String>,
    rotation_days: Option<u32>,
}

impl KeyDraft {
    pub fn into_input(
        mut self,
        existing: Option<ApiKeyInput>,
    ) -> Result<ApiKeyInput, CommandError> {
        let secret = self
            .secret
            .take()
            .filter(|value| !value.is_empty())
            .map(Secret::from)
            .or_else(|| existing.as_ref().map(|input| input.secret.clone()))
            .ok_or_else(|| CommandError::new("invalid", "The secret must not be empty."))?;
        let username = self
            .username
            .take()
            .or_else(|| existing.and_then(|key| key.username));
        let expires_at = self
            .expires_at
            .as_deref()
            .map(akm_core::schedule::parse_expiry)
            .transpose()?;
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
            expires_at,
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
    #[serde(serialize_with = "serialize_sequence")]
    pub seq: u64,
    pub ts: DateTime<Utc>,
    pub action: String,
    pub key_title: Option<String>,
    pub project: Option<String>,
    pub environment: Option<String>,
    pub key_id: Option<String>,
    pub detail: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub path: String,
    pub entries: Vec<AuditView>,
    #[serde(serialize_with = "serialize_optional_sequence")]
    pub broken_at: Option<u64>,
    pub state: akm_core::audit::AuditState,
    pub damage: Option<akm_core::audit::Damage>,
    pub identity: akm_core::journal::Identity,
    pub notices: Vec<akm_core::journal::Notice>,
    pub problem_line: Option<usize>,
    pub total_entries: usize,
}

fn serialize_sequence<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(value)
}

fn serialize_optional_sequence<S: serde::Serializer>(
    value: &Option<u64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.serialize_some(&value.to_string()),
        None => serializer.serialize_none(),
    }
}

#[cfg(test)]
mod audit_precision_tests {
    use super::*;

    #[test]
    fn ipc_audit_sequences_are_lossless_decimal_strings() {
        for seq in [(1u64 << 53) - 1, 1u64 << 53, (1u64 << 53) + 1, u64::MAX] {
            let report = AuditReport {
                path: "fake journal".into(),
                entries: vec![AuditView {
                    seq,
                    ts: Utc::now(),
                    action: "__proto__".into(),
                    key_title: None,
                    project: None,
                    environment: None,
                    key_id: None,
                    detail: None,
                }],
                broken_at: Some(seq),
                state: akm_core::audit::AuditState::Corrupt,
                damage: Some(akm_core::audit::Damage::BrokenLink),
                identity: akm_core::journal::Identity::Confirmed,
                notices: vec![],
                problem_line: Some(3),
                total_entries: 0,
            };
            let json = serde_json::to_value(report).unwrap();
            assert_eq!(json["brokenAt"].as_str(), Some(seq.to_string().as_str()));
            assert_eq!(
                json["entries"][0]["seq"].as_str(),
                Some(seq.to_string().as_str())
            );
        }
        let unknown = AuditReport {
            path: "fake".into(),
            entries: vec![],
            broken_at: None,
            state: akm_core::audit::AuditState::Corrupt,
            damage: None,
            identity: akm_core::journal::Identity::Confirmed,
            notices: vec![],
            problem_line: Some(1),
            total_entries: 0,
        };
        assert!(serde_json::to_value(unknown).unwrap()["brokenAt"].is_null());
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub added: usize,
    pub updated: usize,
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
        let input = draft
            .into_input(Some(vault.edit_input(&key.id).unwrap()))
            .unwrap();
        assert_eq!(input.username.as_deref(), Some("alice"));
        vault.update(&key.id, input).unwrap();
        assert_eq!(
            crate::test_support::secret_of(&vault, &key.id),
            "fake-secret"
        );
    }

    /// CORE-R1-07 at the desktop's boundary: the draft reads its expiry as text, with the one
    /// rule of the core. A fraction of a second is refused before add and before update, also a
    /// fraction beyond the nine digits that a typed instant keeps, and nothing is stored or
    /// changed. A whole second, a fraction of zero and a date pass.
    #[test]
    fn a_draft_with_a_fraction_of_a_second_in_its_expiry_is_refused_and_stores_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let mut vault = Vault::create(
            directory.path().join("vault.kdbx"),
            "fake-test-password-123!",
            None,
        )
        .unwrap();
        let draft = |expires: &str| -> KeyDraft {
            serde_json::from_value(serde_json::json!({
                "title": "Fake",
                "secret": "fake-secret",
                "expiresAt": expires,
            }))
            .unwrap()
        };
        for refused in [
            "2030-01-02T03:04:05.5Z",
            "2030-01-02T03:04:05.000000001Z",
            "2030-01-02T03:04:05.0000000001Z",
            "2030-01-02T03:04:05.00000000000000000001Z",
            "2030-01-02T03:04:05.999+00:00",
            "tomorrow",
            "",
        ] {
            let error = draft(refused).into_input(None).expect_err(refused);
            assert_eq!(error.kind, "invalid", "{refused}");
            assert!(!error.message.contains("fake-secret"), "{refused}");
        }
        assert!(vault.list().is_empty());
        for (accepted, stored) in [
            ("2030-01-02T03:04:05Z", "2030-01-02T03:04:05+00:00"),
            ("2030-01-02T03:04:05.000Z", "2030-01-02T03:04:05+00:00"),
            ("2030-01-02T05:04:05+02:00", "2030-01-02T03:04:05+00:00"),
            ("2030-01-02", "2030-01-02T23:59:59+00:00"),
        ] {
            let input = draft(accepted).into_input(None).unwrap();
            assert_eq!(input.expires_at.unwrap().to_rfc3339(), stored, "{accepted}");
        }
        let key = vault
            .add(draft("2030-01-02T03:04:05Z").into_input(None).unwrap())
            .unwrap();
        let whole = key.expires_at.unwrap();
        let existing = vault.edit_input(&key.id).unwrap();
        let error = draft("2031-01-02T03:04:05.25Z")
            .into_input(Some(existing))
            .err()
            .unwrap();
        assert_eq!(error.kind, "invalid");
        assert_eq!(vault.list()[0].expires_at, Some(whole));
        // The core holds the same rule for a typed instant from any other caller.
        let mut typed = vault.edit_input(&key.id).unwrap();
        typed.expires_at = Some("2030-01-02T03:04:05.000000001Z".parse().unwrap());
        assert!(vault.update(&key.id, typed).is_err());
        assert_eq!(vault.list()[0].expires_at, Some(whole));
    }
}
