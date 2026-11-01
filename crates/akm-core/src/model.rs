use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Custom field names written into the KDBX entry. KeePassXC preserves unknown
/// fields, so the vault stays fully readable outside this app.
pub const META_ID: &str = "AKM_ID";
pub const META_ISSUER: &str = "AKM_ISSUER";
pub const META_PROJECT: &str = "AKM_PROJECT";
pub const META_ENVIRONMENT: &str = "AKM_ENV";
pub const META_SCOPES: &str = "AKM_SCOPES";
pub const META_ENV_VAR: &str = "AKM_ENV_VAR";
pub const META_ROTATION_DAYS: &str = "AKM_ROTATION_DAYS";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub changes: Vec<ImportChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportChange {
    pub name: String,
    pub title: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: String,
    pub title: String,
    /// The secret itself, stored in the KDBX Password field (protected).
    pub secret: String,
    pub username: Option<String>,
    pub issuer: Option<String>,
    pub project: Option<String>,
    pub environment: Option<String>,
    pub scopes: Vec<String>,
    /// Environment variable this key is injected as. Derived from the title if unset.
    pub env_var: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub rotation_days: Option<u32>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl ApiKey {
    /// Case-insensitive match across the fields a person would search by.
    pub fn matches(&self, needle: &str) -> bool {
        let needle = needle.to_lowercase();
        let haystack = [
            Some(self.title.as_str()),
            self.issuer.as_deref(),
            self.project.as_deref(),
            self.environment.as_deref(),
            self.username.as_deref(),
            self.url.as_deref(),
            self.notes.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(self.tags.iter().map(String::as_str))
        .chain(self.scopes.iter().map(String::as_str));

        haystack
            .into_iter()
            .any(|v| v.to_lowercase().contains(&needle))
    }

    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|e| e <= now)
    }

    /// Days until the rotation interval elapses, based on last modification.
    pub fn rotation_due_in(&self, now: DateTime<Utc>) -> Option<i64> {
        let days = self.rotation_days? as i64;
        let updated = self.updated_at.or(self.created_at)?;
        Some((updated + chrono::Duration::days(days) - now).num_days())
    }

    /// The environment variable name used when injecting this key.
    pub fn resolved_env_var(&self) -> String {
        self.env_var
            .clone()
            .unwrap_or_else(|| derive_env_var(&self.title))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ApiKeyInput {
    pub title: String,
    pub secret: String,
    pub username: Option<String>,
    pub issuer: Option<String>,
    pub project: Option<String>,
    pub environment: Option<String>,
    pub scopes: Vec<String>,
    pub env_var: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub rotation_days: Option<u32>,
}

impl ApiKeyInput {
    pub fn new(title: impl Into<String>, secret: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            secret: secret.into(),
            ..Default::default()
        }
    }
    pub fn validate(&self) -> crate::Result<()> {
        if self.title.trim().is_empty() {
            return Err(crate::Error::Invalid("title must not be empty".into()));
        }
        if self.secret.is_empty() {
            return Err(crate::Error::Invalid("secret must not be empty".into()));
        }
        if let Some(var) = &self.env_var {
            if !is_valid_env_var(var) {
                return Err(crate::Error::Invalid(format!(
                    "{var:?} is not a valid environment variable name (use A-Z, 0-9, _)"
                )));
            }
        }
        if let Some(days) = self.rotation_days {
            if days == 0 {
                return Err(crate::Error::Invalid(
                    "rotation_days must be greater than zero".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Reuse an existing key's metadata when changing only its secret.
impl From<&ApiKey> for ApiKeyInput {
    fn from(key: &ApiKey) -> Self {
        Self {
            title: key.title.clone(),
            secret: key.secret.clone(),
            username: key.username.clone(),
            issuer: key.issuer.clone(),
            project: key.project.clone(),
            environment: key.environment.clone(),
            scopes: key.scopes.clone(),
            env_var: key.env_var.clone(),
            url: key.url.clone(),
            notes: key.notes.clone(),
            tags: key.tags.clone(),
            expires_at: key.expires_at,
            rotation_days: key.rotation_days,
        }
    }
}

pub fn is_valid_env_var(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

pub(crate) fn validate_process_var(name: &str, value: &str) -> crate::Result<()> {
    if !is_valid_env_var(name) {
        return Err(crate::Error::Invalid(format!(
            "{name:?} is not a valid environment variable name"
        )));
    }
    if ["AKM_", "LD_", "DYLD_"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
        || matches!(
            name,
            "PATH"
                | "PATHEXT"
                | "COMSPEC"
                | "SYSTEMROOT"
                | "WINDIR"
                | "NODE_OPTIONS"
                | "NODE_PATH"
                | "PYTHONPATH"
                | "PYTHONHOME"
        )
    {
        return Err(crate::Error::Invalid(format!(
            "{name:?} is reserved for process control"
        )));
    }
    if value.is_empty() || value.contains('\0') {
        return Err(crate::Error::Invalid(format!(
            "{name:?} must have a non-empty value without NUL characters"
        )));
    }
    Ok(())
}

/// "OpenAI — Personal" -> "OPENAI_PERSONAL"
pub fn derive_env_var(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut last_was_sep = true;

    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_uppercase());
            last_was_sep = false;
        } else if !last_was_sep {
            out.push('_');
            last_was_sep = true;
        }
    }

    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        "API_KEY".to_string()
    } else if out.as_bytes()[0].is_ascii_digit() {
        format!("KEY_{out}")
    } else {
        out
    }
}
