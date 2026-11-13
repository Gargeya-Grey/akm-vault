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

/// One field of a request: `None` when the request does not name it, `Some(None)` when it
/// names it with `null`, `Some(Some(value))` when it gives a value.
type Named<T> = Option<Option<T>>;

/// A key as the page sends it (FW-03). One rule for every field: a field that the request does
/// not name keeps the stored value exactly, with no conversion; `null`, an empty text or an
/// empty list clears it; a value sets it, cleaned as an add cleans it. An add starts from an
/// empty key, so it names every field that it fills. The title cannot be cleared. The secret
/// keeps its own rule: absent, `null` or empty keeps the stored secret.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KeyDraft {
    #[serde(default, deserialize_with = "named")]
    title: Named<String>,
    secret: Option<String>,
    #[serde(default, deserialize_with = "named")]
    username: Named<String>,
    #[serde(default, deserialize_with = "named")]
    issuer: Named<String>,
    #[serde(default, deserialize_with = "named")]
    project: Named<String>,
    #[serde(default, deserialize_with = "named")]
    environment: Named<String>,
    #[serde(default, deserialize_with = "named")]
    scopes: Named<Vec<String>>,
    #[serde(default, deserialize_with = "named")]
    env_var: Named<String>,
    #[serde(default, deserialize_with = "named")]
    url: Named<String>,
    #[serde(default, deserialize_with = "named")]
    notes: Named<String>,
    #[serde(default, deserialize_with = "named")]
    tags: Named<Vec<String>>,
    /// Text, read with the core's one expiry rule (`schedule::parse_expiry`), which checks the
    /// text: a typed instant has already lost every fraction digit after the ninth. `null`
    /// clears the expiry; an empty text is refused.
    #[serde(default, deserialize_with = "named")]
    expires_at: Named<String>,
    /// `null` clears the interval.
    #[serde(default, deserialize_with = "named")]
    rotation_days: Named<u32>,
}

fn named<'de, D, T>(deserializer: D) -> Result<Named<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl KeyDraft {
    /// `key` with the fields that the request names: the stored key of an update
    /// (`Vault::edit_input`, as `akm edit` starts), or an empty key for an add.
    pub fn apply(self, mut key: ApiKeyInput) -> Result<ApiKeyInput, CommandError> {
        fn text(field: &mut Option<String>, named: Named<String>) {
            if let Some(value) = named {
                *field = clean(value);
            }
        }
        fn list(field: &mut Vec<String>, named: Named<Vec<String>>) {
            if let Some(values) = named {
                *field = tidy(values.unwrap_or_default());
            }
        }
        if let Some(title) = self.title {
            key.title = title.unwrap_or_default();
        }
        if let Some(secret) = self.secret.filter(|value| !value.is_empty()) {
            key.secret = Secret::from(secret);
        }
        text(&mut key.username, self.username);
        text(&mut key.issuer, self.issuer);
        text(&mut key.project, self.project);
        text(&mut key.environment, self.environment);
        text(&mut key.env_var, self.env_var);
        text(&mut key.url, self.url);
        text(&mut key.notes, self.notes);
        list(&mut key.scopes, self.scopes);
        list(&mut key.tags, self.tags);
        if let Some(expires) = self.expires_at {
            key.expires_at = expires
                .as_deref()
                .map(akm_core::schedule::parse_expiry)
                .transpose()?;
        }
        if let Some(days) = self.rotation_days {
            key.rotation_days = days;
        }
        Ok(key)
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
            let error = draft(refused)
                .apply(ApiKeyInput::default())
                .expect_err(refused);
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
            let input = draft(accepted).apply(ApiKeyInput::default()).unwrap();
            assert_eq!(input.expires_at.unwrap().to_rfc3339(), stored, "{accepted}");
        }
        let key = vault
            .add(
                draft("2030-01-02T03:04:05Z")
                    .apply(ApiKeyInput::default())
                    .unwrap(),
            )
            .unwrap();
        let whole = key.expires_at.unwrap();
        let existing = vault.edit_input(&key.id).unwrap();
        let error = draft("2031-01-02T03:04:05.25Z")
            .apply(existing)
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
